//! 自检：不启动界面，直接把整条生产链跑一遍。
//!
//! 触发方式：
//! ```bash
//! cargo run -- --pipeline-check
//! ```
//!
//! 依次验证：建项目 → 圣经 → 章节 → 风格 → 分镜 → 资产 → 生图 → 提示词 →
//! 生视频 → 铺时间线 → 导出成片 → 字幕能力探测 → 面板完成度统计。
//! 返回进程退出码：0 表示全部通过，非 0 表示有步骤失败。

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::actions;
use crate::asr;
use crate::checklist_ops;
use crate::jobs::JobCtx;
use crate::media;
use crate::models::*;
use crate::progress;
use crate::project::Project;
use crate::state::AppState;

static FAILURES: AtomicU32 = AtomicU32::new(0);

fn ok(step: &str, detail: impl std::fmt::Display) {
    println!("  [OK] {step}: {detail}");
}

fn fail(step: &str, detail: impl std::fmt::Display) {
    println!("  [!!] {step}: {detail}");
    FAILURES.fetch_add(1, Ordering::SeqCst);
}

async fn wait_jobs(state: &AppState, timeout: Duration) -> (u32, u32) {
    let started = Instant::now();
    loop {
        let jobs = state.jobs.snapshot();
        let running = jobs
            .iter()
            .filter(|j| matches!(j.status, JobStatus::Running | JobStatus::Queued))
            .count();
        if running == 0 || started.elapsed() > timeout {
            let done = jobs.iter().filter(|j| j.status == JobStatus::Done).count() as u32;
            let failed = jobs.iter().filter(|j| j.status == JobStatus::Failed).count() as u32;
            for j in jobs.iter().filter(|j| j.status == JobStatus::Failed) {
                println!(
                    "      ! 任务失败 [{}] {}",
                    j.title,
                    j.error.clone().unwrap_or_default()
                );
            }
            return (done, failed);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// 网络自检：不启动界面，检查代理设置与几个关键地址的连通性。
///
/// ```bash
/// cargo run -- --net-check
/// ```
pub async fn run_net_check() -> i32 {
    let dir = crate::config::default_config_dir();
    let state = AppState::new(dir);
    let settings = state.settings();

    println!("\n=== 网络与代理自检 ===\n");
    let diag = crate::net::diagnostics(&settings);
    println!("  代理模式        {}", diag["mode"].as_str().unwrap_or("-"));
    println!(
        "  环境变量代理    {}",
        diag["envProxy"].as_str().unwrap_or("（无）")
    );
    println!(
        "  系统代理        {}",
        diag["systemProxy"].as_str().unwrap_or("（无）")
    );
    println!(
        "  实际使用        {}",
        diag["effective"].as_str().unwrap_or("直连")
    );
    println!("  不走代理的地址  {}\n", diag["noProxy"].as_str().unwrap_or(""));

    let targets = [
        ("HuggingFace（whisper 模型下载源）", "https://huggingface.co/"),
        ("hf-mirror 镜像", "https://hf-mirror.com/"),
        ("Anthropic API", "https://api.anthropic.com/"),
        ("OpenAI API", "https://api.openai.com/"),
    ];

    let client = match crate::net::build_client(25, crate::net::resolve_proxy(&settings).as_deref()) {
        Ok(c) => c,
        Err(e) => {
            fail("构建 HTTP 客户端", e);
            return 1;
        }
    };

    for (name, url) in targets {
        let started = Instant::now();
        match client.get(url).send().await {
            Ok(resp) => ok(
                name,
                format!(
                    "HTTP {}（{} ms）",
                    resp.status().as_u16(),
                    started.elapsed().as_millis()
                ),
            ),
            Err(e) => {
                let msg = if e.is_timeout() {
                    "超时".to_string()
                } else if e.is_connect() {
                    "连接失败".to_string()
                } else {
                    e.to_string()
                };
                fail(name, format!("{msg}（{} ms）", started.elapsed().as_millis()));
            }
        }
    }

    let failures = FAILURES.load(Ordering::SeqCst);
    println!(
        "\n{}",
        if failures == 0 {
            "=== 网络自检通过 ===".to_string()
        } else {
            format!("=== 有 {failures} 个地址不通，去「设置 → 运行环境」调整代理 ===")
        }
    );
    failures as i32
}

/// 命令行预下载 whisper 模型（不用打开界面）：
/// ```bash
/// cargo run -- --asr-download ggml-tiny
/// ```
pub async fn run_asr_download(model_id: &str) -> i32 {
    let dir = crate::config::default_config_dir();
    let state = AppState::new(dir);
    println!("\n=== 下载 whisper 模型：{model_id} ===\n");
    let job = state.jobs.create(JobKind::Download, "模型下载", "准备中");
    let ctx = JobCtx {
        id: job.id.clone(),
        queue: state.jobs.clone(),
    };
    let watcher = {
        let q = state.jobs.clone();
        let id = job.id.clone();
        tauri::async_runtime::spawn(async move {
            let mut last = String::new();
            loop {
                tokio::time::sleep(Duration::from_millis(400)).await;
                if let Some(j) = q.get(&id) {
                    let line = format!("{:.1}% {}", j.progress * 100.0, j.detail);
                    if line != last {
                        println!("  {line}");
                        last = line.clone();
                    }
                    if matches!(
                        j.status,
                        JobStatus::Done | JobStatus::Failed | JobStatus::Canceled
                    ) {
                        break;
                    }
                }
            }
        })
    };
    let result = crate::asr::download_model(&state, &ctx, model_id).await;
    let _ = watcher.await;
    match result {
        Ok(info) => {
            ok(
                "模型下载",
                format!("{} → {}", info.label, info.path.unwrap_or_default()),
            );
            0
        }
        Err(e) => {
            fail("模型下载", e);
            1
        }
    }
}

/// 列出已安装的技能，验证技能目录解析是否正常。
/// ```bash
/// cargo run -- --skills
/// ```
pub async fn run_list_skills() -> i32 {
    let dir = crate::config::default_config_dir();
    let state = AppState::new(dir);
    // 顺带把旧版 knowledge/ 迁移过来、并补上说明文件
    let _ = crate::skills::ensure_defaults(&state);
    println!("
=== 已安装的技能 ===
");
    let skills = crate::skills::list(&state);
    if skills.is_empty() {
        println!("  （还没有技能）");
    }
    for s in &skills {
        println!(
            "  [{}] {}
      {}
      正文 {} 字{}{}",
            if s.enabled { "启用" } else { "停用" },
            s.name,
            crate::llm::truncate(&s.description, 100),
            s.chars,
            if s.files.is_empty() {
                String::new()
            } else {
                format!("，{} 个附件", s.files.len())
            },
            if s.dir.is_some() { "，目录技能" } else { "" }
        );
    }
    println!("
  技能目录：{}
", crate::skills::skills_dir(&state).display());
    let idx = crate::skills::prompt_index(&state);
    println!("  进系统提示的目录：{} 字
", idx.chars().count());
    0
}

/// 命令行导入技能（zip / 目录 / md 都行），不用打开界面：
/// ```bash
/// cargo run -- --import-skill D:/下载/剧本skill合集.zip
/// ```
pub async fn run_import_skills(paths: &[String]) -> i32 {
    let dir = crate::config::default_config_dir();
    let state = AppState::new(dir);
    let _ = crate::skills::ensure_defaults(&state);
    println!("
=== 导入技能 ===
");
    match crate::skills::import(&state, paths) {
        Ok(added) => {
            for a in &added {
                println!("  + {a}");
            }
            println!("
  共导入 {} 个。用 `cargo run -- --skills` 查看。
", added.len());
            0
        }
        Err(e) => {
            fail("导入", e);
            1
        }
    }
}

/// pi 引擎自检：真实跑一轮对话（会消耗少量模型额度）。
///
/// ```bash
/// cargo run -- --pi-check [项目路径]
/// ```
/// 不指定项目路径时用临时项目；依次验证：状态探测 → 真实对话流式事件 →
/// 会话文件落盘 → 进程复用 → 重启后会话恢复。
pub async fn run_pi_check(project_path: Option<String>) -> i32 {
    use crate::agent::AgentEvent;
    use parking_lot::Mutex;
    use tauri::ipc::{Channel, InvokeResponseBody};

    // 自检路径不经过 run()，追踪订阅器要在这里自己初始化（诊断日志才有输出）
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    println!("\n=== pi 引擎自检 ===\n");
    let state = AppState::new(crate::config::default_config_dir());

    let proj = match project_path.as_deref() {
        Some(p) => match Project::open(std::path::Path::new(p)) {
            Ok(pr) => pr,
            Err(e) => {
                fail("打开项目", e);
                return 1;
            }
        },
        None => {
            let tmp = std::env::temp_dir().join(format!("pi-check-{}", std::process::id()));
            match Project::create(&tmp, "pi-check") {
                Ok(pr) => pr,
                Err(e) => {
                    fail("建临时项目", e);
                    return 1;
                }
            }
        }
    };
    let root = proj.paths.root.clone();
    state.set_project(Some(proj));

    let st = crate::pi::status(&state).await;
    println!("  可用性    {}", if st.available { "可用" } else { "不可用" });
    println!("  pi 路径   {}", st.path.as_deref().unwrap_or("-"));
    println!("  版本      {}", st.version.as_deref().unwrap_or("-"));
    println!("  说明      {}", st.detail);
    if !st.available {
        fail("pi 状态", "不可用（先配置文本模型 provider）");
        return 1;
    }

    // 收集事件类型与流式文本
    let types: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let deltas: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let t = types.clone();
    let d = deltas.clone();
    let channel = Channel::<AgentEvent>::new(move |body: InvokeResponseBody| {
        if let InvokeResponseBody::Json(s) = body {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
                if ty == "textDelta" {
                    if let Some(txt) = v.get("text").and_then(|x| x.as_str()) {
                        d.lock().push_str(txt);
                    }
                }
                t.lock().push(ty.to_string());
            }
        }
        Ok(())
    });

    let question = "用一句话回答：你现在能正常工作吗？".to_string();
    match crate::pi::run_turn(&state, channel, question, vec![]).await {
        Ok(run_id) => {
            let seen = types.lock().clone();
            let text = deltas.lock().clone();
            let has_delta = seen.iter().any(|x| x == "textDelta");
            let has_finish = seen.iter().any(|x| x == "runFinished");
            let line = format!(
                "run {run_id}，{} 个事件，回复 {} 字",
                seen.len(),
                text.chars().count()
            );
            if has_delta && has_finish && !text.trim().is_empty() {
                ok("pi 往返", line);
                println!("      回复：{}", text.trim().chars().take(80).collect::<String>());
            } else {
                fail("pi 往返", format!("{line}；事件：{}", seen.join("、")));
            }
        }
        Err(e) => fail("pi 往返", e),
    }

    // 会话文件应已落盘
    let sess_file = root.join(".workbench").join("pi").join("last-session.txt");
    match crate::store::read_text_opt(&sess_file) {
        Ok(Some(sf)) if std::path::Path::new(sf.trim()).is_file() => {
            ok("会话落盘", sf.trim().to_string())
        }
        Ok(other) => fail("会话落盘", format!("last-session.txt 内容异常：{other:?}")),
        Err(e) => fail("会话落盘", e),
    }

    // 进程应保持常驻
    let st2 = crate::pi::status(&state).await;
    if st2.running {
        ok("进程常驻", "对话结束后 sidecar 仍在运行");
    } else {
        fail("进程常驻", "对话结束后进程不在了");
    }

    // 工具桥：让 pi 真实调用一个只读工具（project_snapshot），验证
    // extension → 本地 HTTP 桥 → actions/tools registry 的完整链路
    {
        let types3: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let results3: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let t3 = types3.clone();
        let r3 = results3.clone();
        let channel3 = Channel::<AgentEvent>::new(move |body: InvokeResponseBody| {
            if let InvokeResponseBody::Json(s) = body {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                    let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
                    if ty == "toolResult" {
                        r3.lock().push(
                            v.get("summary").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                        );
                    }
                    t3.lock().push(ty.to_string());
                }
            }
            Ok(())
        });
        match crate::pi::run_turn(
            &state,
            channel3,
            "请用 project_snapshot 工具读取项目数据，然后用一句话告诉我这个项目里已经有哪些内容。".into(),
            vec![],
        )
        .await
        {
            Ok(_) => {
                let seen = types3.lock().clone();
                let has_call = seen.iter().any(|x| x == "toolCall");
                let has_result = seen.iter().any(|x| x == "toolResult");
                let summary = results3.lock().first().cloned().unwrap_or_default();
                if has_call && has_result && !summary.is_empty() {
                    ok("工具桥", format!("pi 调用了工作台工具，返回：{summary}"));
                } else {
                    fail(
                        "工具桥",
                        format!(
                            "没有观察到工具调用（事件：{}）；模型可能没按指令用工具",
                            seen.join("、")
                        ),
                    );
                }
            }
            Err(e) => fail("工具桥", e),
        }
    }

    // 审批链路：临时切到「变更前确认」模式，让 pi 调一个写数据的工具，
    // 自检代替用户「拒绝」，验证 extension 的 tool_call 钩子确实拦住了执行。
    {
        let chapters_before = state.current().map(|p| p.script.chapters.len()).unwrap_or(0);

        let mut settings = state.settings();
        let prev_mode = settings.agent_mode.clone();
        settings.agent_mode = "confirm".into();
        let _ = state.save_settings(&settings);

        let types4: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let calls4: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let blocked = Arc::new(Mutex::new(false));
        let t4 = types4.clone();
        let c4 = calls4.clone();
        let b4 = blocked.clone();
        let st4 = state.clone();
        let channel4 = Channel::<AgentEvent>::new(move |body: InvokeResponseBody| {
            if let InvokeResponseBody::Json(s) = body {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                    let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
                    if ty == "toolCall" {
                        let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("?");
                        let nc = v
                            .get("needsConfirm")
                            .and_then(|x| x.as_bool())
                            .unwrap_or(false);
                        let raw = serde_json::to_string(&v).unwrap_or_default();
                        c4.lock().push(format!(
                            "{name}(confirm={nc}) raw={}",
                            raw.chars().take(150).collect::<String>()
                        ));
                        if nc {
                            // 自检代替用户点「拒绝」
                            if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
                                let _ = crate::agent::resolve_approval(&st4, id, false);
                                *b4.lock() = true;
                            }
                        }
                    }
                    t4.lock().push(ty.to_string());
                }
            }
            Ok(())
        });

        let ask = "请调用 script_append_chapter 工具，给项目追加一章，标题用《审批测试章》。";
        match crate::pi::run_turn(&state, channel4, ask.into(), vec![]).await {
            Ok(_) => {
                if *blocked.lock() {
                    let after = state.current().map(|p| p.script.chapters.len()).unwrap_or(0);
                    if after == chapters_before {
                        ok(
                            "审批链路",
                            format!("confirm 模式下拦截了写操作，拒绝后章节数保持 {after}"),
                        );
                    } else {
                        fail(
                            "审批链路",
                            format!("拒绝后工具仍执行了：章节数 {chapters_before} → {after}"),
                        );
                    }
                } else {
                    fail(
                        "审批链路",
                        format!(
                            "没有观察到需要确认的工具调用；工具调用：{}；事件：{}",
                            calls4.lock().join("、"),
                            types4.lock().join("、")
                        ),
                    );
                    let tail = state.pi.stderr_tail();
                    if !tail.is_empty() {
                        println!("      pi stderr（含 extension 日志）：");
                        for line in tail.iter().rev().take(12).collect::<Vec<_>>().iter().rev() {
                            println!("        {line}");
                        }
                    }
                }
            }
            Err(e) => fail("审批链路", e),
        }

        // 恢复原权限模式
        let mut s2 = state.settings();
        s2.agent_mode = prev_mode;
        let _ = state.save_settings(&s2);
    }

    // 重启进程后应恢复同一会话
    let before = crate::store::read_text_opt(&sess_file).ok().flatten();
    state.pi.shutdown().await;
    let types2: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let t2 = types2.clone();
    let channel2 = Channel::<AgentEvent>::new(move |body: InvokeResponseBody| {
        if let InvokeResponseBody::Json(s) = body {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                if let Some(ty) = v.get("type").and_then(|x| x.as_str()) {
                    t2.lock().push(ty.to_string());
                }
            }
        }
        Ok(())
    });
    match crate::pi::run_turn(&state, channel2, "再说一句：刚才我们聊过什么？".into(), vec![]).await
    {
        Ok(_) => {
            let after = crate::store::read_text_opt(&sess_file).ok().flatten();
            if before.is_some() && before == after {
                ok("会话恢复", "重启进程后仍是同一个会话文件");
            } else {
                fail("会话恢复", format!("会话文件变了：{before:?} → {after:?}"));
            }
        }
        Err(e) => fail("会话恢复（第二轮）", e),
    }

    state.pi.shutdown().await;
    FAILURES.load(Ordering::SeqCst) as i32
}

pub async fn run_pipeline_check() -> i32 {
    let root = std::env::temp_dir().join("duanju-pipeline-check");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let state = AppState::new(root.join("_config"));
    println!("\n=== 短剧工作台 · 端到端自检 ===\n");

    /* ---------------------------------------------------------- 项目 */
    let project = match Project::create(&root, "自检项目") {
        Ok(p) => p,
        Err(e) => {
            fail("建项目", e);
            return 1;
        }
    };
    let paths = project.paths.clone();
    state.set_project(Some(project));
    ok("建项目", paths.root.display());

    /* ---------------------------------------------------------- 设置 */
    let settings = AppSettings {
        providers: vec![
            ProviderConfig {
                id: "mock-llm".into(),
                kind: ProviderKind::Llm,
                name: "占位文本".into(),
                adapter: "mock".into(),
                model: "mock-model".into(),
                enabled: true,
                concurrency: 1,
                timeout_sec: 300,
                ..Default::default()
            },
            ProviderConfig {
                id: "mock-image".into(),
                kind: ProviderKind::Image,
                name: "占位生图".into(),
                adapter: "mock".into(),
                model: "mock-image".into(),
                enabled: true,
                concurrency: 2,
                timeout_sec: 300,
                ..Default::default()
            },
            ProviderConfig {
                id: "mock-video".into(),
                kind: ProviderKind::Video,
                name: "占位生视频".into(),
                adapter: "mock".into(),
                model: "mock-video".into(),
                enabled: true,
                concurrency: 2,
                timeout_sec: 600,
                ..Default::default()
            },
        ],
        active_llm_provider_id: Some("mock-llm".into()),
        active_image_provider_id: Some("mock-image".into()),
        active_video_provider_id: Some("mock-video".into()),
        agent_mode: "auto".into(),
        ..Default::default()
    };
    state.save_settings(&settings).unwrap();
    ok("写入设置", format!("{} 个 provider", settings.providers.len()));

    match state.http_client() {
        Ok(_) => ok("HTTP 客户端", "已构建"),
        Err(e) => fail("HTTP 客户端", e),
    }

    /* ------------------------------------------------------ 圣经与风格 */
    state
        .mutate(|p| {
            p.manifest.genre = "都市甜宠".into();
            p.manifest.logline = "被退婚那天，她捡到了全城最贵的男人".into();
            p.bible.synopsis = "女主被退婚，意外救下重伤的集团继承人，两人从契约婚姻走向真心。".into();
            p.bible.selling_points = vec!["开局退婚打脸".into(), "契约婚姻先婚后爱".into()];
            p.bible.characters = vec![CharacterCard {
                id: new_id("chr"),
                name: "苏晚".into(),
                role: "女主".into(),
                appearance: "二十出头，酒红色长直发，锁骨处有颗小痣".into(),
                personality: "外柔内刚".into(),
                ..Default::default()
            }];
            p.style.spec.name = "都市甜宠".into();
            p.style.spec.prompt = "电影级质感，暖调高饱和，柔光竖屏人像，浅景深".into();
            p.style.spec.negative = "低分辨率, 畸变, 水印".into();
            p.style.spec.aspect_ratio = "9:16".into();
            Ok(())
        })
        .unwrap();
    ok("项目圣经 + 风格圣经", "已写入");

    /* ---------------------------------------------------------- 章节 */
    state
        .mutate(|p| {
            for i in 1..=2u32 {
                let id = format!("ch_{i:03}");
                p.script.chapters.push(ChapterMeta {
                    id: id.clone(),
                    index: i,
                    title: format!("第 {i} 章"),
                    status: ChapterStatus::Empty,
                    summary: format!("第 {i} 章大纲"),
                    characters: vec!["苏晚".into()],
                    scenes: vec![],
                    word_count: 0,
                    updated_at: now_iso(),
                });
                p.save_chapter_content(
                    &id,
                    &format!("第 {i} 章的正文内容。苏晚推开门，看见他坐在轮椅上。"),
                )?;
            }
            Ok(())
        })
        .unwrap();
    let (chapters, wc) = state
        .read(|p| (p.script.chapters.len(), p.script.chapters[0].word_count))
        .unwrap();
    ok("章节", format!("{chapters} 章，第一章 {wc} 字"));

    /* ---------------------------------------------------------- 分镜 */
    state
        .mutate(|p| {
            for (i, action) in [
                "苏晚推门而入，目光落在轮椅上的人身上",
                "男人抬眼，唇角微微一勾",
            ]
            .iter()
            .enumerate()
            {
                p.shots.push(Shot {
                    id: new_id("sh"),
                    chapter_id: "ch_001".into(),
                    index: i as u32 + 1,
                    scene_id: None,
                    location: "别墅客厅".into(),
                    time_of_day: "黄昏".into(),
                    interior: true,
                    shot_size: if i == 0 { "全景" } else { "特写" }.into(),
                    camera: "平视".into(),
                    camera_move: if i == 0 { "推近" } else { "固定" }.into(),
                    duration_sec: if i == 0 { 3.0 } else { 2.5 },
                    characters: vec!["苏晚".into()],
                    props: vec![],
                    action: action.to_string(),
                    dialogue: String::new(),
                    narration: String::new(),
                    sfx: String::new(),
                    bgm: String::new(),
                    image_prompt: String::new(),
                    video_prompt: String::new(),
                    ref_images: vec![],
                    status: ShotStatus::Draft,
                });
            }
            Ok(())
        })
        .unwrap();
    let shots = state.read(|p| p.shots.len()).unwrap_or(0);
    ok("分镜", format!("{shots} 个镜头"));

    /* ---------------------------------------------------------- 资产 */
    let asset_id = new_id("ast");
    let view_ids: Vec<String> = (0..2).map(|_| new_id("vw")).collect();
    let aid = asset_id.clone();
    let vids = view_ids.clone();
    state
        .mutate(|p| {
            p.assets.push(Asset {
                id: aid.clone(),
                kind: AssetKind::Character,
                name: "苏晚".into(),
                aliases: vec!["晚晚".into()],
                description: "二十出头的都市女性，酒红色长直发".into(),
                tags: vec!["女主".into()],
                locked_traits: vec!["锁骨处小痣".into()],
                views: vec![
                    AssetView {
                        id: vids[0].clone(),
                        ref_images: vec![],
                        kind: ViewKind::Front,
                        label: "正面".into(),
                        prompt: "正面全身站姿".into(),
                        negative: String::new(),
                        file: None,
                        thumb: None,
                        seed: Some(42),
                        model: None,
                        provider_id: None,
                        status: AssetStatus::Planned,
                        error: None,
                        job_id: None,
                        created_at: now_iso(),
                    },
                    AssetView {
                        id: vids[1].clone(),
                        ref_images: vec![],
                        kind: ViewKind::Side,
                        label: "侧面".into(),
                        prompt: "侧面全身站姿".into(),
                        negative: String::new(),
                        file: None,
                        thumb: None,
                        seed: None,
                        model: None,
                        provider_id: None,
                        status: AssetStatus::Planned,
                        error: None,
                        job_id: None,
                        created_at: now_iso(),
                    },
                ],
                created_at: now_iso(),
                updated_at: now_iso(),
            });
            Ok(())
        })
        .unwrap();

    let composed = state
        .read(|p| {
            let a = p.assets[0].clone();
            let v = a.views[0].clone();
            actions::compose_image_prompt(p, &a, &v)
        })
        .unwrap();
    ok("提示词拼装", composed);

    /* ---------------------------------------------------------- 生图 */
    match actions::generate_asset_views(&state, &asset_id, vec![], false) {
        Ok(jobs) => {
            let (done, failed) = wait_jobs(&state, Duration::from_secs(180)).await;
            let files: Vec<String> = state
                .read(|p| {
                    p.assets
                        .iter()
                        .flat_map(|a| a.views.iter())
                        .filter_map(|v| v.file.clone())
                        .collect()
                })
                .unwrap_or_default();
            let line = format!(
                "提交 {} 个任务，完成 {done}，失败 {failed}，落盘 {} 个文件",
                jobs.len(),
                files.len()
            );
            if files.len() == jobs.len() && failed == 0 {
                ok("生图", line);
            } else {
                fail("生图", line);
            }
        }
        Err(e) => fail("生图", e),
    }

    /* -------------------------------------------------------- 提示词 */
    state
        .mutate(|p| {
            let shots: Vec<Shot> = p.shots.clone();
            for s in shots {
                p.prompts.push(VideoPrompt {
                    id: new_id("vp"),
                    ref_images: vec![],
                    shot_id: s.id.clone(),
                    index: s.index,
                    prompt: format!(
                        "{}，镜头{}，时长 {:.0} 秒",
                        s.action, s.camera_move, s.duration_sec
                    ),
                    negative: p.style.spec.negative.clone(),
                    motion: s.action.clone(),
                    camera_move: s.camera_move.clone(),
                    duration_sec: s.duration_sec,
                    first_frame: Some(AssetRef {
                        asset_id: asset_id.clone(),
                        view_id: Some(view_ids[0].clone()),
                    }),
                    last_frame: None,
                    refs: vec![],
                    model_hint: None,
                    seed: None,
                    status: PromptStatus::Ready,
                    updated_at: now_iso(),
                });
            }
            Ok(())
        })
        .unwrap();
    let pcount = state.read(|p| p.prompts.len()).unwrap_or(0);
    ok("视频提示词", format!("{pcount} 条，已绑定首帧"));

    /* -------------------------------------------------------- 生视频 */
    let shot_ids: Vec<String> = state
        .read(|p| p.shots.iter().map(|s| s.id.clone()).collect())
        .unwrap();
    match actions::generate_video_takes(&state, shot_ids, None, None) {
        Ok(jobs) => {
            let (done, failed) = wait_jobs(&state, Duration::from_secs(300)).await;
            let takes = state.read(|p| p.takes.len()).unwrap_or(0);
            let with_file = state
                .read(|p| p.takes.iter().filter(|t| t.file.is_some()).count())
                .unwrap_or(0);
            let line = format!(
                "提交 {}，完成 {done}，失败 {failed}，{takes} 条记录 / {with_file} 条有文件",
                jobs.len()
            );
            if with_file == jobs.len() && failed == 0 {
                ok("生视频", line);
            } else {
                fail("生视频", line);
            }
        }
        Err(e) => fail("生视频", e),
    }

    /* ---------------------------------------------------------- 铺轨 */
    match actions::build_timeline_from_takes(&state) {
        Ok(n) => {
            let (dur, clips) = state
                .read(|p| {
                    (
                        p.timeline.duration_sec,
                        p.timeline.tracks.iter().map(|t| t.clips.len()).sum::<usize>(),
                    )
                })
                .unwrap();
            ok(
                "铺时间线",
                format!("{n} 段，总时长 {dur:.1}s，轨道片段 {clips}"),
            );
        }
        Err(e) => fail("铺时间线", e),
    }

    /* ---------------------------------------------------------- 导出 */
    match actions::render_timeline_job(&state, false) {
        Ok(_) => {
            let (done, failed) = wait_jobs(&state, Duration::from_secs(300)).await;
            let out: Vec<std::path::PathBuf> = state
                .read(|p| {
                    std::fs::read_dir(p.paths.render_dir())
                        .ok()
                        .map(|rd| {
                            rd.filter_map(|x| x.ok())
                                .map(|x| x.path())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            if let Some(f) = out.first() {
                match media::probe(&state, f).await {
                    Ok(info) => {
                        let line = format!(
                            "{} | {}x{} | {:.1}s | 音轨 {} | 完成 {done} 失败 {failed}",
                            f.file_name().unwrap().to_string_lossy(),
                            info.width,
                            info.height,
                            info.duration_sec,
                            if info.has_audio { "有" } else { "无" }
                        );
                        if info.has_video && info.duration_sec > 0.5 {
                            ok("导出成片", line);
                        } else {
                            fail("导出成片", line);
                        }
                    }
                    Err(e) => fail("导出成片（探针）", e),
                }
            } else {
                fail("导出成片", "没有产出文件");
            }
        }
        Err(e) => fail("导出", e),
    }

    /* ------------------------------------------------------ 字幕能力 */
    let caps = asr::capabilities(&state);
    ok(
        "字幕能力探测",
        format!(
            "{} / {} 线程 | GPU: {} | 推荐后端 {} | 内置 whisper {} | 外部 CLI {}",
            caps.os,
            caps.cpu_threads,
            caps.nvidia_gpu.clone().unwrap_or_else(|| "无".into()),
            caps.recommended_backend,
            if caps.whisper_compiled { "是" } else { "否" },
            caps.external_cli_path
                .clone()
                .unwrap_or_else(|| "未找到".into())
        ),
    );

    // 真跑一次转写（拿导出的成片当输入）；环境不具备时给出明确原因
    let media_file = state
        .read(|p| {
            std::fs::read_dir(p.paths.render_dir())
                .ok()
                .and_then(|rd| rd.filter_map(|x| x.ok()).map(|x| x.path()).next())
        })
        .ok()
        .flatten();
    if let Some(file) = media_file {
        let job = state.jobs.create(JobKind::Asr, "自检转写", "测试");
        let ctx = JobCtx {
            id: job.id,
            queue: state.jobs.clone(),
        };
        let opts = asr::TranscribeOptions {
            file: file.to_string_lossy().to_string(),
            language: "zh".into(),
            model_id: "ggml-tiny".into(),
            use_gpu: false,
            threads: 2,
            max_line_chars: 18,
            name: "自检".into(),
        };
        match asr::transcribe(&state, &ctx, &opts).await {
            Ok(doc) => ok("字幕转写", format!("{} 条字幕", doc.cues.len())),
            Err(e) => println!("  [--] 字幕转写: 跳过（{e}）"),
        }
    }

    /* ------------------------------------------------------ agent 往返 */
    // 走一遍真实的 run_turn：流式事件、工具调用、工具执行、缓存前缀上报
    {
        use crate::agent::{AgentEvent, RunOptions};
        use parking_lot::Mutex;
        use tauri::ipc::{Channel, InvokeResponseBody};

        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        let sink = seen.clone();
        let channel = Channel::<AgentEvent>::new(move |body: InvokeResponseBody| {
            if let InvokeResponseBody::Json(s) = body {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                    if let Some(t) = v.get("type").and_then(|x| x.as_str()) {
                        sink.lock().push(t.to_string());
                    }
                }
            }
            Ok(())
        });

        match crate::agent::run_turn(
            state.clone(),
            channel,
            RunOptions {
                panel: PanelId::Script,
                session_id: None,
                input: "现在做到哪一步了？".into(),
                image_paths: vec![],
                refresh_context: false,
            },
        )
        .await
        {
            Ok(sid) => {
                let types = seen.lock().clone();
                let expect = [
                    "runStarted",
                    "prefix",
                    "usage",
                    "toolCall",
                    "toolResult",
                    "runFinished",
                ];
                let missing: Vec<&str> = expect
                    .iter()
                    .copied()
                    .filter(|k| !types.iter().any(|t| t == k))
                    .collect();
                let sess = state.session(&sid);
                let line = format!(
                    "{} 个事件，{} 条消息，前缀 {}",
                    types.len(),
                    sess.as_ref().map(|s| s.messages.len()).unwrap_or(0),
                    sess.as_ref()
                        .and_then(|s| s.prefix.as_ref())
                        .map(|p| p.fingerprint.clone())
                        .unwrap_or_default()
                );
                if missing.is_empty() {
                    ok("agent 往返", line);
                } else {
                    fail("agent 往返", format!("缺少事件：{}", missing.join("、")))
                }
            }
            Err(e) => fail("agent 往返", e),
        }
    }

    /* -------------------------------------------------------- 完成度 */
    let snap = state
        .read(|p| {
            let snap = checklist_ops::snapshot(p);
            checklist_ops::with_auto(Arc::new(p.clone()), snap)
        })
        .unwrap();
    println!("\n  面板完成度：");
    for p in progress::all_progress(&snap) {
        println!(
            "    {:<10} {:>6.1}%  ({}/{})  {}",
            p.panel.as_str(),
            p.percent,
            p.done,
            p.total,
            p.blockers.first().cloned().unwrap_or_default()
        );
    }
    println!(
        "\n  检查项：自动 {} 条，自定义 {} 条",
        snap.checklist
            .items
            .iter()
            .filter(|i| i.auto)
            .count(),
        snap.checklist
            .items
            .iter()
            .filter(|i| !i.auto)
            .count()
    );

    println!("\n  项目目录：{}\n", paths.root.display());
    let failures = FAILURES.load(Ordering::SeqCst);
    if failures == 0 {
        println!("=== 自检全部通过 ===\n");
    } else {
        println!("=== 自检有 {failures} 项失败 ===\n");
    }
    failures as i32
}
