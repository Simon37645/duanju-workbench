//! 后台任务队列。所有耗时操作（生图 / 生视频 / 转码 / 转写 / 模型下载）都走这里，
//! 统一上报进度并持久化，重启后仍能看到历史。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;

use crate::error::Result;
use crate::models::{now_iso, Job, JobKind, JobStatus, PanelId};
use crate::store;

pub const EVENT_JOB: &str = "job://update";

#[derive(Clone)]
pub struct JobQueue {
    inner: Arc<Inner>,
}

struct Inner {
    jobs: RwLock<Vec<Job>>,
    /// 每个 kind 的并发闸门
    gates: RwLock<HashMap<&'static str, Arc<Semaphore>>>,
    cancel: RwLock<HashMap<String, Arc<AtomicBool>>>,
    app: RwLock<Option<AppHandle>>,
    persist_path: RwLock<Option<std::path::PathBuf>>,
}

impl JobQueue {
    pub fn new() -> Self {
        let mut gates = HashMap::new();
        gates.insert("image", Arc::new(Semaphore::new(2)));
        gates.insert("video", Arc::new(Semaphore::new(2)));
        gates.insert("asr", Arc::new(Semaphore::new(1)));
        gates.insert("ffmpeg", Arc::new(Semaphore::new(2)));
        gates.insert("download", Arc::new(Semaphore::new(3)));
        gates.insert("llm", Arc::new(Semaphore::new(4)));
        Self {
            inner: Arc::new(Inner {
                jobs: RwLock::new(vec![]),
                gates: RwLock::new(gates),
                cancel: RwLock::new(HashMap::new()),
                app: RwLock::new(None),
                persist_path: RwLock::new(None),
            }),
        }
    }

    pub fn attach_app(&self, app: AppHandle) {
        *self.inner.app.write() = Some(app);
    }

    /// 调整某一类任务的并发闸门（`"image"` / `"video"` …）。
    ///
    /// 设置里每个 provider 都有 `concurrency`，但队列闸门原本是写死的常量，
    /// 供应商只准收 1 个并发时就会一路 429。启动与保存设置时用这个对齐。
    /// 只对之后提交的任务生效：已提交的任务在提交那一刻就拿到了旧的信号量。
    pub fn set_gate(&self, key: &'static str, permits: usize) {
        let n = permits.max(1);
        self.inner
            .gates
            .write()
            .insert(key, Arc::new(Semaphore::new(n)));
    }

    /// 切换项目时把队列历史挂到当前项目；传 None 表示不持久化。
    pub fn set_persist_path(&self, path: Option<std::path::PathBuf>) {
        *self.inner.persist_path.write() = path.clone();
        if let Some(p) = path {
            if let Ok(Some(list)) = store::read_json_opt::<Vec<Job>>(&p) {
                // 上次退出时还在跑的任务，重启后标记为已取消
                let list: Vec<Job> = list
                    .into_iter()
                    .map(|mut j| {
                        if matches!(j.status, JobStatus::Running | JobStatus::Queued) {
                            j.status = JobStatus::Canceled;
                            j.error = Some("应用退出时中断".into());
                        }
                        j
                    })
                    .collect();
                *self.inner.jobs.write() = list;
            }
        } else {
            *self.inner.jobs.write() = vec![];
        }
        self.emit_all();
    }

    pub fn snapshot(&self) -> Vec<Job> {
        let mut v = self.inner.jobs.read().clone();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        v
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.inner.jobs.read().iter().find(|j| j.id == id).cloned()
    }

    fn persist(&self) {
        if let Some(p) = self.inner.persist_path.read().clone() {
            let list = self.inner.jobs.read().clone();
            let _ = store::write_json(&p, &list);
        }
    }

    fn emit_all(&self) {
        if let Some(app) = self.inner.app.read().clone() {
            let _ = app.emit(EVENT_JOB, JobEvent::Reset { jobs: self.snapshot() });
        }
    }

    fn emit_one(&self, job: &Job) {
        if let Some(app) = self.inner.app.read().clone() {
            let _ = app.emit(EVENT_JOB, JobEvent::Update { job: job.clone() });
        }
    }

    pub fn create(&self, kind: JobKind, title: impl Into<String>, detail: impl Into<String>) -> Job {
        let job = Job {
            id: crate::models::new_id("job"),
            kind,
            title: title.into(),
            detail: detail.into(),
            status: JobStatus::Queued,
            progress: 0.0,
            total: 0,
            done: 0,
            error: None,
            created_at: now_iso(),
            updated_at: now_iso(),
            finished_at: None,
        };
        self.inner.jobs.write().push(job.clone());
        self.inner
            .cancel
            .write()
            .insert(job.id.clone(), Arc::new(AtomicBool::new(false)));
        self.emit_one(&job);
        self.persist();
        job
    }

    pub fn update<F: FnOnce(&mut Job)>(&self, id: &str, f: F) {
        let updated = {
            let mut jobs = self.inner.jobs.write();
            if let Some(j) = jobs.iter_mut().find(|j| j.id == id) {
                f(j);
                j.updated_at = now_iso();
                Some(j.clone())
            } else {
                None
            }
        };
        if let Some(j) = updated {
            self.emit_one(&j);
        }
    }

    pub fn finish_ok(&self, id: &str, detail: Option<String>) {
        self.update(id, |j| {
            j.status = JobStatus::Done;
            j.progress = 1.0;
            if let Some(d) = detail {
                j.detail = d;
            }
            j.finished_at = Some(now_iso());
        });
        self.persist();
    }

    pub fn finish_err(&self, id: &str, err: impl Into<String>) {
        let err = err.into();
        self.update(id, |j| {
            j.status = JobStatus::Failed;
            j.error = Some(err);
            j.finished_at = Some(now_iso());
        });
        self.persist();
    }

    pub fn cancel(&self, id: &str) -> bool {
        if let Some(flag) = self.inner.cancel.read().get(id).cloned() {
            flag.store(true, Ordering::SeqCst);
            self.update(id, |j| {
                if matches!(j.status, JobStatus::Queued | JobStatus::Running) {
                    j.status = JobStatus::Canceled;
                    j.finished_at = Some(now_iso());
                }
            });
            self.persist();
            true
        } else {
            false
        }
    }

    pub fn is_canceled(&self, id: &str) -> bool {
        self.inner
            .cancel
            .read()
            .get(id)
            .map(|f| f.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    pub fn clear_finished(&self) {
        self.inner
            .jobs
            .write()
            .retain(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running));
        self.emit_all();
        self.persist();
    }

    /// 提交一个后台任务。runner 拿到 JobCtx 用于上报进度与检查取消。
    /// 返回刚建好的任务记录（含 id），调用方可以把它挂到业务数据上。
    pub fn submit<F, Fut>(
        &self,
        kind: JobKind,
        title: impl Into<String>,
        detail: impl Into<String>,
        runner: F,
    ) -> Job
    where
        F: FnOnce(JobCtx) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<String>> + Send + 'static,
    {
        let job = self.create(kind, title, detail);
        let ctx = JobCtx {
            id: job.id.clone(),
            queue: self.clone(),
        };
        let this = self.clone();
        let gate_key = match kind {
            JobKind::Image => "image",
            JobKind::Video => "video",
            JobKind::Asr => "asr",
            JobKind::Ffmpeg => "ffmpeg",
            JobKind::Download => "download",
            JobKind::Llm => "llm",
        };
        let sem = self
            .inner
            .gates
            .read()
            .get(gate_key)
            .cloned()
            .unwrap_or_else(|| Arc::new(Semaphore::new(2)));

        tauri::async_runtime::spawn(async move {
            let _permit = match sem.acquire_owned().await {
                Ok(p) => p,
                Err(_) => return,
            };
            if this.is_canceled(&ctx.id) {
                return;
            }
            this.update(&ctx.id, |j| {
                j.status = JobStatus::Running;
                j.progress = 0.02;
            });
            match runner(ctx.clone()).await {
                Ok(detail) => {
                    if this.is_canceled(&ctx.id) {
                        return;
                    }
                    this.finish_ok(&ctx.id, Some(detail));
                }
                Err(e) => {
                    if this.is_canceled(&ctx.id) {
                        return;
                    }
                    if matches!(e, crate::error::AppError::Canceled) {
                        this.update(&ctx.id, |j| {
                            j.status = JobStatus::Canceled;
                            j.finished_at = Some(now_iso());
                        });
                        this.persist();
                    } else {
                        this.finish_err(&ctx.id, e.to_string());
                    }
                }
            }
        });
        job
    }
}

impl Default for JobQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct JobCtx {
    pub id: String,
    pub queue: JobQueue,
}

impl JobCtx {
    pub fn progress(&self, p: f32, detail: impl Into<String>) {
        let p = p.clamp(0.0, 1.0);
        let d = detail.into();
        self.queue.update(&self.id, |j| {
            j.progress = p;
            if !d.is_empty() {
                j.detail = d;
            }
        });
    }

    pub fn step(&self, done: u32, total: u32, detail: impl Into<String>) {
        let p = if total == 0 {
            0.0
        } else {
            done as f32 / total as f32
        };
        let d = detail.into();
        self.queue.update(&self.id, |j| {
            j.progress = p;
            j.done = done;
            j.total = total;
            if !d.is_empty() {
                j.detail = d;
            }
        });
    }

    pub fn canceled(&self) -> bool {
        self.queue.is_canceled(&self.id)
    }

    pub fn check_cancel(&self) -> Result<()> {
        if self.canceled() {
            Err(crate::error::AppError::Canceled)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum JobEvent {
    Reset { jobs: Vec<Job> },
    Update { job: Job },
}

/// 面板相关的任务过滤，供 UI 用。
pub fn job_panel(kind: JobKind) -> PanelId {
    match kind {
        JobKind::Image => PanelId::Asset,
        JobKind::Video => PanelId::Video,
        JobKind::Asr => PanelId::Subtitle,
        JobKind::Ffmpeg => PanelId::Edit,
        JobKind::Llm => PanelId::Script,
        JobKind::Download => PanelId::Checklist,
    }
}
