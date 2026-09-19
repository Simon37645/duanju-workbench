//! 应用运行时状态：当前项目、任务队列、agent 会话、HTTP 客户端、配置。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use crate::agent::AgentSession;
use crate::config::ConfigStore;
use crate::error::{AppError, Result};
use crate::jobs::JobQueue;
use crate::models::{AppSettings, ProviderConfig, ProviderKind};
use crate::project::Project;

pub const EVENT_PROJECT_CHANGED: &str = "project://changed";
pub const EVENT_SESSION_CHANGED: &str = "agent://session-changed";

#[derive(Default)]
pub struct AgentRuntime {
    /// sessionId -> 会话
    pub sessions: RwLock<HashMap<String, AgentSession>>,
    /// toolCallId -> 等待用户确认（花钱操作）的通道
    pub approvals: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    /// toolCallId -> 等待用户回答提问的通道
    pub answers: Mutex<HashMap<String, oneshot::Sender<String>>>,
    pub running: RwLock<HashSet<String>>,
}

#[derive(Clone)]
pub struct AppState {
    pub config: ConfigStore,
    pub project: Arc<RwLock<Option<Arc<Project>>>>,
    pub jobs: JobQueue,
    pub agent: Arc<AgentRuntime>,
    pub http: Arc<RwLock<Option<reqwest::Client>>>,
    pub app: Arc<RwLock<Option<AppHandle>>>,
}

impl AppState {
    pub fn new(config_dir: std::path::PathBuf) -> Self {
        Self {
            config: ConfigStore::new(config_dir),
            project: Arc::new(RwLock::new(None)),
            jobs: JobQueue::new(),
            agent: Arc::new(AgentRuntime::default()),
            http: Arc::new(RwLock::new(None)),
            app: Arc::new(RwLock::new(None)),
        }
    }

    pub fn attach_app(&self, app: AppHandle) {
        *self.app.write() = Some(app.clone());
        self.jobs.attach_app(app);
    }

    /// 知识包一改，所有会话的冻结前缀就过期了，下一轮重建。
    /// 代价是一次缓存失效 —— 但内容确实变了，这是必须付的。
    pub fn reset_agent_prefix(&self) {
        for s in self.agent.sessions.write().values_mut() {
            s.prefix = None;
        }
        self.persist_sessions();
    }

    /// 某个会话被外部改动（比如压缩）后通知前端刷新
    pub fn emit_session_changed(&self, session_id: &str) -> Result<()> {
        if let Some(app) = self.app.read().clone() {
            let _ = app.emit(EVENT_SESSION_CHANGED, session_id.to_string());
        }
        Ok(())
    }

    pub fn emit_changed(&self) {
        if let Some(app) = self.app.read().clone() {
            let _ = app.emit(EVENT_PROJECT_CHANGED, ());
        }
    }

    /* ------------------------------------------------------------ 设置 */

    pub fn settings(&self) -> AppSettings {
        self.config.load_settings()
    }

    pub fn save_settings(&self, s: &AppSettings) -> Result<()> {
        self.config.save_settings(s)
    }

    pub fn provider(&self, id: &str) -> Option<ProviderConfig> {
        self.settings().providers.into_iter().find(|p| p.id == id)
    }

    pub fn active_provider(&self, kind: ProviderKind) -> Option<ProviderConfig> {
        let s = self.settings();
        let id = match kind {
            ProviderKind::Llm => s.active_llm_provider_id,
            ProviderKind::Image => s.active_image_provider_id,
            ProviderKind::Video => s.active_video_provider_id,
        };
        if let Some(id) = id {
            if let Some(p) = s.providers.iter().find(|p| p.id == id && p.enabled) {
                return Some(p.clone());
            }
        }
        s.providers
            .iter()
            .find(|p| p.kind == kind && p.enabled)
            .cloned()
    }

    pub fn api_key_for(&self, cfg: &ProviderConfig) -> Option<String> {
        self.config.get_secret(&cfg.api_key_ref)
    }

    pub fn http_client(&self) -> Result<reqwest::Client> {
        if let Some(c) = self.http.read().clone() {
            return Ok(c);
        }
        let proxy = crate::net::resolve_proxy(&self.settings());
        let c = crate::net::build_client(0, proxy.as_deref())?;
        *self.http.write() = Some(c.clone());
        Ok(c)
    }

    /// 代理设置变了之后要丢掉缓存的客户端与系统代理探测结果。
    pub fn reset_http(&self) {
        *self.http.write() = None;
        crate::net::refresh_system_proxy();
    }

    /* ------------------------------------------------------------ 项目 */

    pub fn current(&self) -> Result<Arc<Project>> {
        self.project
            .read()
            .clone()
            .ok_or(AppError::NoProject)
    }

    pub fn current_opt(&self) -> Option<Arc<Project>> {
        self.project.read().clone()
    }

    pub fn set_project(&self, p: Option<Project>) {
        let root = p.as_ref().map(|x| x.root().to_path_buf());
        *self.project.write() = p.map(Arc::new);
        self.jobs
            .set_persist_path(root.map(|r| crate::project::ProjectPaths::new(r).jobs_file()));
    }

    /// 只读访问当前项目。
    pub fn read<R>(&self, f: impl FnOnce(&Project) -> R) -> Result<R> {
        let p = self.current()?;
        Ok(f(&p))
    }

    /// 修改当前项目：改完自动落盘并通知前端刷新。
    pub fn mutate<R>(&self, f: impl FnOnce(&mut Project) -> Result<R>) -> Result<R> {
        let r = {
            let mut guard = self.project.write();
            let Some(arc) = guard.as_mut() else {
                return Err(AppError::NoProject);
            };
            let p = Arc::make_mut(arc);
            let r = f(p)?;
            p.touch();
            p.save_all()?;
            r
        };
        self.emit_changed();
        Ok(r)
    }

    /// 只落盘不通知（例如流式过程中的中间态）。
    pub fn mutate_quiet<R>(&self, f: impl FnOnce(&mut Project) -> Result<R>) -> Result<R> {
        let mut guard = self.project.write();
        let Some(arc) = guard.as_mut() else {
            return Err(AppError::NoProject);
        };
        let p = Arc::make_mut(arc);
        let r = f(p)?;
        p.save_all()?;
        Ok(r)
    }

    /* ------------------------------------------------------------ 会话 */

    pub fn session(&self, id: &str) -> Option<AgentSession> {
        self.agent.sessions.read().get(id).cloned()
    }

    pub fn put_session(&self, s: AgentSession) {
        self.agent.sessions.write().insert(s.id.clone(), s);
    }

    pub fn sessions_of(&self, project_id: &str) -> Vec<AgentSession> {
        let mut v: Vec<AgentSession> = self
            .agent
            .sessions
            .read()
            .values()
            .filter(|s| s.project_id == project_id)
            .cloned()
            .collect();
        v.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        v
    }

    pub fn persist_sessions(&self) {
        let Some(p) = self.current_opt() else { return };
        let list = self.sessions_of(&p.manifest.id);
        let path = p.paths.sessions_file();
        let _ = crate::store::write_json(&path, &list);
    }

    pub fn load_sessions(&self, project: &Project) {
        if let Ok(Some(list)) =
            crate::store::read_json_opt::<Vec<AgentSession>>(&project.paths.sessions_file())
        {
            let mut map = self.agent.sessions.write();
            for s in list {
                map.insert(s.id.clone(), s);
            }
        }
    }
}
