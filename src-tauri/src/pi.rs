//! pi agent sidecar（RPC 模式）。
//!
//! pi 作为**可选引擎**与自研 agent 并存：进程常驻，按 JSONL 协议通信
//! （协议见 pi 的 docs/rpc.md）。几个关键约束：
//!
//! - **分帧**：记录只按 `\n` 切分，不能用会按 Unicode 分隔符切分的通用行
//!   读取器（U+2028/U+2029 是 JSON 字符串里的合法字符），所以这里手写字节级
//!   缓冲。跨 chunk 的多字节 UTF-8 也在字节层拼接后整体解码。
//! - **请求关联**：命令带自增 id，响应按 id resolve 到 oneshot；进程退出时
//!   所有挂起请求立刻失败，绝不让调用方永久等待。
//! - **安全**：M1 阶段用 `-nt` 禁用 pi 的全部内置工具（read/write/bash…），
//!   防止它绕过 actions.rs 直接改项目文件；工具能力在 M2 通过 extension 桥接。
//!   模型密钥只经环境变量传入，不落盘、不进命令行参数。
//! - **会话**：session 存在项目 `.workbench/pi/` 下，跟随「文件即数据库」；
//!   换项目时进程自动重启到新项目目录。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::ipc::Channel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, oneshot};

use crate::agent::{AgentEvent, UsageReport};
use crate::error::{AppError, Result};
use crate::models::{ProviderConfig, ProviderKind};
use crate::state::AppState;

const EVENT_CHANNEL_CAP: usize = 4096;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// 单行 JSON 上限：超过视为协议异常，丢弃该行（防止坏进程把内存吃满）
const MAX_LINE_BYTES: usize = 64 * 1024 * 1024;
const STDERR_TAIL_LINES: usize = 40;

/* ==================================================================== 状态 */

#[derive(Default)]
pub struct PiRuntime {
    inner: Mutex<Option<PiProcess>>,
    next_id: AtomicU64,
    /// 串行化 spawn/shutdown：并发 ensure 不能起出两个进程
    lifecycle: tokio::sync::Mutex<()>,
    /// 单飞：同一时刻只允许一轮对话（两个 run 会各订阅一份事件流，必须挡住）
    run_lock: tokio::sync::Mutex<()>,
}

struct PiProcess {
    child: Child,
    stdin: Arc<tokio::sync::Mutex<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>,
    events: broadcast::Sender<Value>,
    project_root: PathBuf,
    stderr_tail: Arc<Mutex<Vec<String>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PiStatus {
    pub available: bool,
    pub running: bool,
    pub detail: String,
    pub path: Option<String>,
    pub version: Option<String>,
}

/* ==================================================================== 探测 */

struct Launch {
    program: PathBuf,
    prefix: Vec<String>,
    path_label: String,
    version: Option<String>,
}

/// 找 pi 可执行入口：优先应用自带 sidecar，其次开发环境（PATH / npm 全局）。
fn resolve_launch() -> Result<Launch> {
    // 1) 应用旁 sidecar/pi（发布形态）
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let base = dir.join("sidecar").join("pi");
            let node = base.join("node").join("node.exe");
            let entry = base.join("dist").join("bundle").join("cli.js");
            if node.is_file() && entry.is_file() {
                return Ok(make_launch(node, entry, base.join("package.json")));
            }
            // 也可能直接把 pi.exe / pi 放进来
            let direct = base.join("pi.exe");
            if direct.is_file() {
                return Ok(Launch {
                    program: direct.clone(),
                    prefix: vec![],
                    path_label: direct.to_string_lossy().to_string(),
                    version: None,
                });
            }
        }
    }

    // 2) npm 全局安装（开发形态）：%APPDATA%/npm/node_modules/@earendil-works/pi-coding-agent
    if let Ok(appdata) = std::env::var("APPDATA") {
        let pkg = PathBuf::from(&appdata)
            .join("npm")
            .join("node_modules")
            .join("@earendil-works")
            .join("pi-coding-agent");
        let entry = pkg.join("dist").join("bundle").join("cli.js");
        if entry.is_file() {
            let node = resolve_node().ok_or_else(|| {
                AppError::Config("找到 pi 但找不到 node 运行时，请确认 node 已安装".into())
            })?;
            return Ok(make_launch(node, entry, pkg.join("package.json")));
        }
    }

    Err(AppError::Config(
        "没有找到 pi 引擎（既没有应用自带的 sidecar，也没有全局安装的 pi-coding-agent）".into(),
    ))
}

fn make_launch(node: PathBuf, entry: PathBuf, pkg_json: PathBuf) -> Launch {
    let version = std::fs::read_to_string(&pkg_json)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("version").and_then(|x| x.as_str()).map(String::from));
    Launch {
        program: node,
        prefix: vec![entry.to_string_lossy().to_string()],
        path_label: entry.to_string_lossy().to_string(),
        version,
    }
}

fn resolve_node() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PI_WORKBENCH_NODE") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let names = ["node.exe", "node"];
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        for n in names {
            let cand = dir.join(n);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// 写 provider 桥 extension：把项目里配置的端点注册给 pi。
/// 内容固定，密钥靠环境变量注入（`$PI_WORKBENCH_API_KEY` 引用），不落盘。
fn write_provider_extension(state: &AppState) -> Result<PathBuf> {
    let dir = state.config.dir().join("pi-bridge");
    crate::store::ensure_dir(&dir)?;
    let path = dir.join("workbench-provider.ts");
    // 幂等覆写：内容升级后旧文件要被替换掉（原子写，调用不频繁）
    crate::store::write_text(&path, PROVIDER_EXTENSION_TS)?;
    Ok(path)
}

const PROVIDER_EXTENSION_TS: &str = r#"// 由「短剧工作台」自动生成 —— provider 桥 + 工具桥 + 审批钩子。
// 配置与密钥都从环境变量读：PI_WORKBENCH_PROVIDER / PI_WORKBENCH_API_KEY /
// PI_WORKBENCH_BRIDGE_URL / PI_WORKBENCH_BRIDGE_TOKEN。
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

const BRIDGE = process.env.PI_WORKBENCH_BRIDGE_URL ?? "";
const TOKEN = process.env.PI_WORKBENCH_BRIDGE_TOKEN ?? "";

async function bridgeCall(path: string, body?: unknown): Promise<any> {
  const res = await fetch(`${BRIDGE}${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: { "content-type": "application/json", "x-pi-bridge-token": TOKEN },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  return res.json();
}

export default async function (pi: ExtensionAPI) {
  /* ---------------- provider 桥：把项目里配置的模型端点注册给 pi ---------------- */
  const raw = process.env.PI_WORKBENCH_PROVIDER;
  if (raw) {
    let cfg: { baseUrl?: string; model?: string; maxTokens?: number } = {};
    try {
      cfg = JSON.parse(raw);
    } catch {
      cfg = {};
    }
    if (cfg.baseUrl && cfg.model) {
      pi.registerProvider("workbench", {
        name: "短剧工作台",
        baseUrl: cfg.baseUrl,
        apiKey: "$PI_WORKBENCH_API_KEY",
        api: "openai-completions",
        models: [
          {
            id: cfg.model,
            name: cfg.model,
            reasoning: false,
            input: ["text", "image"],
            cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
            contextWindow: 128000,
            maxTokens: cfg.maxTokens && cfg.maxTokens > 0 ? cfg.maxTokens : 8192,
          },
        ],
      });
    }
  }

  if (!BRIDGE || !TOKEN) return;

  /* ---------------- 工具桥：注册工作台的全部工具 ---------------- */
  let tools: Array<{ name: string; title?: string; description?: string; inputSchema?: unknown }> = [];
  try {
    const payload = await bridgeCall("/tools");
    tools = payload?.tools ?? [];
  } catch {
    return; // 桥不可用就只保留 provider 能力
  }

  for (const t of tools) {
    pi.registerTool({
      name: t.name,
      label: t.title ?? t.name,
      description: t.description ?? "",
      parameters: Type.Unsafe(
        (t.inputSchema as object) ?? { type: "object", properties: {} },
      ),
      async execute(_toolCallId: string, params: Record<string, unknown>) {
        try {
          const r = await bridgeCall(`/tool/${encodeURIComponent(t.name)}`, params ?? {});
          if (r?.ok) {
            const summary: string = r.result?.summary ?? "完成";
            const data = r.result?.data;
            const text =
              data === undefined || data === null
                ? summary
                : `${summary}\n\n\`\`\`json\n${JSON.stringify(data).slice(0, 12000)}\n\`\`\``;
            return { content: [{ type: "text", text }], details: {} };
          }
          return {
            content: [{ type: "text", text: `工具执行失败：${r?.error ?? "未知错误"}` }],
            details: {},
            isError: true,
          };
        } catch (e) {
          return {
            content: [{ type: "text", text: `工具桥不可用：${String(e)}` }],
            details: {},
            isError: true,
          };
        }
      },
    });
  }

  /* ---------------- 审批钩子：花钱 / 改数据的操作交给工作台把关 ---------------- */
  pi.on("tool_call", async (event: any, ctx: any) => {
    try {
      const toolName = event?.toolName ?? "";
      // 先问工作台：当前权限模式下这个工具要不要人工确认（立即返回，不等用户）
      const check = await bridgeCall("/approve-check", {
        toolName,
        input: event?.input ?? event?.args ?? {},
      });
      if (check?.needsConfirm) {
        // 交给 pi 的 UI 协议弹确认；RPC 模式下工作台会把它渲染成审批卡片
        const ask = `${check.title ?? toolName} 需要确认后才能执行`;
        const ok = await ctx.ui.confirm("操作确认", ask);
        if (!ok) {
          return { block: true, reason: "用户拒绝了这次操作" };
        }
      }
    } catch (e) {
      // 桥不可用时放行：工具真正的副作用仍由工作台侧落实，不会绕过 actions
      console.error(`[workbench] 审批检查异常（放行）：${String(e)}`);
    }
  });
}
"#;

/// 工作台角色说明：pi 默认是 coding assistant，这里把它拉回工作台语境。
/// 用 `--append-system-prompt <file>` 传入（避免命令行长度与转义问题）。
const WORKBENCH_SYSTEM_PROMPT: &str = r#"你是「短剧工作台」的制作协作 agent。工作台把一部短剧的生产拆成九个面板：
剧本 → 风格 → 分镜 → 资产 → 3D预演 → 视频提示词 → 生视频 → 剪辑 → 字幕。

一切对项目数据的读写都必须通过提供的工具完成（project_snapshot、script_*、storyboard_*、
asset_*、prompt_*、video_*、edit_*、subtitle_*、bible_update、checklist_report、ask_user 等）。
pi 自带的 shell / 文件读写工具在这里已被禁用：唯一有效的操作通道就是工作台的工具链。

工作准则：
1. 只用中文回复，直接给结果，不要寒暄、不要复述用户已经说过的话。
2. 动手前先用 project_snapshot 了解现状；能一次批量做完的，不要拆成很多次调用。
3. 生图 / 生视频这类花钱的操作会由工作台弹确认，用户同意后才继续，不要绕过。
4. 需要用户拍板时用 ask_user 直接问，不要猜。
"#;

/// 写工作台角色说明文件（幂等覆写）。
fn write_system_prompt(state: &AppState) -> Result<PathBuf> {
    let dir = state.config.dir().join("pi-bridge");
    crate::store::ensure_dir(&dir)?;
    let path = dir.join("system.md");
    crate::store::write_text(&path, WORKBENCH_SYSTEM_PROMPT)?;
    Ok(path)
}

/* ================================================================ 生命周期 */

impl PiRuntime {
    /// 确保进程在：项目变了、进程死了都会重启。
    pub async fn ensure(&self, state: &AppState) -> Result<()> {
        let _guard = self.lifecycle.lock().await;
        let root = state.current()?.root().to_path_buf();
        let alive = {
            let mut g = self.inner.lock();
            match g.as_mut() {
                Some(p) => p.project_root == root && is_alive(&mut p.child),
                None => false,
            }
        };
        if alive {
            return Ok(());
        }
        self.shutdown_locked().await;
        self.spawn(state, &root).await
    }

    async fn spawn(&self, state: &AppState, root: &Path) -> Result<()> {
        let launch = resolve_launch()?;
        let (provider, key, ext_path, sess_dir, last_session) = prepare(state, root)?;
        // 工具桥：extension 通过它回调工作台的工具与审批
        let bridge = state.pi_bridge.ensure(state).await?;
        let system_file = write_system_prompt(state)?;

        let mut cmd = Command::new(&launch.program);
        cmd.args(&launch.prefix);
        cmd.args(["--mode", "rpc", "--session-dir"]);
        cmd.arg(&sess_dir);
        if let Some(s) = last_session.as_deref() {
            if Path::new(s).is_file() {
                cmd.arg("--session").arg(s);
            }
        }
        // workbench 是 provider 桥注册出来的 provider 名（见 extension）
        cmd.args(["--provider", "workbench", "--model", &provider.model]);
        // -nbt：禁掉 pi 内置的 read/write/bash 等工具（防止绕过 actions 直接改文件），
        // 但保留 extension 工具 —— 也就是桥过来的 38 个工作台工具。
        cmd.args(["-nbt", "-ne", "-ns", "-np", "-nc"]);
        cmd.arg("-e").arg(&ext_path);
        cmd.arg("--append-system-prompt").arg(&system_file);
        cmd.current_dir(root);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.kill_on_drop(true);
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW：不要给用户弹控制台黑框
            // （tokio::process::Command 在 Windows 上有 inherent creation_flags）
            cmd.creation_flags(0x0800_0000);
        }
        cmd.env("PI_WORKBENCH_PROVIDER", provider_env_json(&provider)?);
        cmd.env("PI_WORKBENCH_BRIDGE_URL", &bridge.url);
        cmd.env("PI_WORKBENCH_BRIDGE_TOKEN", &bridge.token);
        if let Some(k) = &key {
            cmd.env("PI_WORKBENCH_API_KEY", k);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| AppError::Config(format!("启动 pi 进程失败：{e}（{}）", launch.path_label)))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::other("pi 进程没有 stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::other("pi 进程没有 stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::other("pi 进程没有 stderr"))?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (events, _) = broadcast::channel(EVENT_CHANNEL_CAP);
        let stderr_tail = Arc::new(Mutex::new(Vec::new()));
        let stdin = Arc::new(tokio::sync::Mutex::new(stdin));

        tokio::spawn(reader_loop(
            stdout,
            pending.clone(),
            events.clone(),
            stdin.clone(),
            state.clone(),
        ));
        tokio::spawn(stderr_loop(stderr, stderr_tail.clone()));

        *self.inner.lock() = Some(PiProcess {
            child,
            stdin,
            pending,
            events,
            project_root: root.to_path_buf(),
            stderr_tail,
        });
        Ok(())
    }

    pub async fn shutdown(&self) {
        let _guard = self.lifecycle.lock().await;
        self.shutdown_locked().await;
    }

    /// 同步版：应用退出路径用（不能 await，kill_on_drop 与 OS 兜底）。
    pub fn shutdown_now(&self) {
        if let Some(mut p) = self.inner.lock().take() {
            let _ = p.child.start_kill();
        }
    }

    /// 调用方必须已持有 lifecycle 锁（ensure 内部路径）。
    async fn shutdown_locked(&self) {
        let old = self.inner.lock().take();
        if let Some(mut p) = old {
            let _ = p.child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(3), p.child.wait()).await;
        }
    }

    /// 发一条命令并等响应。id 由此处统一分配，响应失败转成 Provider 错误。
    pub async fn send(&self, cmd: Value) -> Result<Value> {
        let (id, stdin, pending) = {
            let g = self.inner.lock();
            let p = g.as_ref().ok_or_else(|| AppError::other("pi 进程未启动"))?;
            let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
            (id, p.stdin.clone(), p.pending.clone())
        };
        let mut line = cmd;
        line["id"] = json!(id);
        let mut payload = serde_json::to_vec(&line)?;
        payload.push(b'\n');

        let (tx, rx) = oneshot::channel();
        pending.lock().insert(id, tx);
        {
            let mut w = stdin.lock().await;
            if let Err(e) = w.write_all(&payload).await {
                pending.lock().remove(&id);
                return Err(AppError::other(format!("写入 pi 失败：{e}")));
            }
            let _ = w.flush().await;
        }
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => {
                pending.lock().remove(&id);
                Err(AppError::other("pi 进程已退出"))
            }
            Err(_) => {
                pending.lock().remove(&id);
                Err(AppError::other("pi 响应超时（60s）"))
            }
        }
    }

    pub fn subscribe(&self) -> Result<broadcast::Receiver<Value>> {
        let g = self.inner.lock();
        let p = g.as_ref().ok_or_else(|| AppError::other("pi 进程未启动"))?;
        Ok(p.events.subscribe())
    }

    pub fn stderr_tail(&self) -> Vec<String> {
        self.inner
            .lock()
            .as_ref()
            .map(|p| p.stderr_tail.lock().clone())
            .unwrap_or_default()
    }

    pub async fn abort(&self) {
        if self.inner.lock().is_some() {
            let _ = tokio::time::timeout(Duration::from_secs(10), self.send(json!({"type":"abort"}))).await;
        }
    }
}

/// 设置 pi 的思考强度（off/minimal/low/medium/high）。
/// 进程没起来就先拉起来，保证设置立刻生效。
pub async fn set_thinking(state: &AppState, level: &str) -> Result<()> {
    let rt = &state.pi;
    rt.ensure(state).await?;
    rt.send(json!({"type": "set_thinking_level", "level": level}))
        .await?;
    Ok(())
}

fn is_alive(child: &mut Child) -> bool {
    matches!(child.try_wait(), Ok(None))
}

/// 当前可用的文本模型：优先用户配置，其次自检用的环境变量注入
/// （PI_WORKBENCH_TEST_BASE_URL / _MODEL / _API_KEY，不落盘、不改用户配置）。
fn effective_llm(state: &AppState) -> Option<(ProviderConfig, Option<String>)> {
    if let Some(p) = state.active_provider(ProviderKind::Llm) {
        let k = state.api_key_for(&p);
        return Some((p, k));
    }
    let base = std::env::var("PI_WORKBENCH_TEST_BASE_URL").ok()?;
    let model = std::env::var("PI_WORKBENCH_TEST_MODEL").ok()?;
    if base.trim().is_empty() || model.trim().is_empty() {
        return None;
    }
    let key = std::env::var("PI_WORKBENCH_TEST_API_KEY").ok();
    Some((
        ProviderConfig {
            id: "pi-check".into(),
            kind: ProviderKind::Llm,
            name: "自检临时 provider".into(),
            adapter: "openai".into(),
            base_url: base,
            api_key_ref: String::new(),
            model,
            concurrency: 1,
            timeout_sec: 900,
            enabled: true,
            options: Value::Null,
        },
        key,
    ))
}

/// 准备启动参数：provider 校验、extension 文件、会话目录与上次的会话文件。
fn prepare(
    state: &AppState,
    root: &Path,
) -> Result<(ProviderConfig, Option<String>, PathBuf, PathBuf, Option<String>)> {
    let (provider, key) = effective_llm(state).ok_or_else(|| {
        AppError::Config("pi 引擎需要先在设置里配一个文本模型 provider（OpenAI 兼容端点）".into())
    })?;
    if !matches!(
        provider.adapter.as_str(),
        "openai" | "openai-compatible" | ""
    ) {
        return Err(AppError::Config(format!(
            "pi 引擎目前只支持 OpenAI 兼容端点，provider「{}」的适配器是 {}",
            provider.name, provider.adapter
        )));
    }
    if provider.base_url.trim().is_empty() {
        return Err(AppError::Config(format!(
            "provider「{}」没有填 Base URL",
            provider.name
        )));
    }
    if key.is_none() {
        return Err(AppError::Config(format!(
            "provider「{}」还没有填 API Key",
            provider.name
        )));
    }
    let ext_path = write_provider_extension(state)?;
    let sess_dir = root.join(".workbench").join("pi");
    crate::store::ensure_dir(&sess_dir)?;
    let last_session = crate::store::read_text_opt(&sess_dir.join("last-session.txt"))?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok((provider, key, ext_path, sess_dir, last_session))
}

fn provider_env_json(p: &ProviderConfig) -> Result<String> {
    let max_tokens = p
        .options
        .get("maxTokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    Ok(serde_json::to_string(&json!({
        "baseUrl": p.base_url,
        "model": p.model,
        "maxTokens": max_tokens,
    }))?)
}

/// 记下 pi 当前会话文件，下次启动用它恢复。
pub fn remember_session(state: &AppState, session_file: Option<&str>) {
    let Some(sf) = session_file else { return };
    if let Ok(proj) = state.current() {
        let path = proj.root().join(".workbench").join("pi").join("last-session.txt");
        let _ = crate::store::write_text(&path, sf);
    }
}

/* ================================================================ 后台任务 */

/// stdout 读取：字节级按 `\n` 分帧 → 解析 JSON → response 路由 / 事件广播。
async fn reader_loop(
    mut stdout: tokio::process::ChildStdout,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>,
    events: broadcast::Sender<Value>,
    stdin: Arc<tokio::sync::Mutex<ChildStdin>>,
    state: AppState,
) {
    let mut buf: Vec<u8> = Vec::with_capacity(64 * 1024);
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        match stdout.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                    let mut line: Vec<u8> = buf.drain(..=pos).collect();
                    line.pop(); // \n
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    if line.is_empty() {
                        continue;
                    }
                    if line.len() > MAX_LINE_BYTES {
                        continue; // 协议异常行，丢弃
                    }
                    dispatch_line(&line, &pending, &events, &stdin, &state).await;
                }
                // 防 OOM：长时间收不到换行的异常流直接丢弃
                if buf.len() > MAX_LINE_BYTES {
                    buf.clear();
                }
            }
            Err(_) => break,
        }
    }
    // EOF = 进程退出：挂起的请求立刻失败，广播退出事件让 run 循环收尾
    let drained: Vec<_> = pending.lock().drain().map(|(_, tx)| tx).collect();
    for tx in drained {
        let _ = tx.send(Err(AppError::other("pi 进程已退出")));
    }
    let _ = events.send(json!({"type": "__pi_exit"}));
}

async fn dispatch_line(
    line: &[u8],
    pending: &Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>,
    events: &broadcast::Sender<Value>,
    stdin: &Arc<tokio::sync::Mutex<ChildStdin>>,
    state: &AppState,
) {
    let text = String::from_utf8_lossy(line);
    let ev: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return, // 解析失败的行不打断整条流
    };
    match ev.get("type").and_then(Value::as_str).unwrap_or("") {
        "response" => {
            let id = ev.get("id").and_then(Value::as_u64);
            if let Some(id) = id {
                if let Some(tx) = pending.lock().remove(&id) {
                    let ok = ev.get("success").and_then(Value::as_bool).unwrap_or(false);
                    let res = if ok {
                        Ok(ev)
                    } else {
                        Err(AppError::Provider(format!(
                            "pi 命令失败：{}",
                            ev.get("error")
                                .and_then(Value::as_str)
                                .unwrap_or("未知错误")
                        )))
                    };
                    let _ = tx.send(res);
                }
            }
        }
        "extension_ui_request" => {
            let method = ev.get("method").and_then(Value::as_str).unwrap_or("");
            let ui_id = ev.get("id").and_then(Value::as_str).unwrap_or("").to_string();
            if ui_id.is_empty() {
                return;
            }
            if method != "confirm" {
                // 只支持确认对话框（工作台的审批卡片）；其它类型直接回「取消」，
                // 免得 pi 侧一直等（notify / setStatus 这类无响应的请求在上面直接忽略）
                if matches!(method, "select" | "input" | "editor") {
                    let resp =
                        json!({"type":"extension_ui_response","id":ui_id,"cancelled":true});
                    if let Ok(mut payload) = serde_json::to_vec(&resp) {
                        payload.push(b'\n');
                        let mut w = stdin.lock().await;
                        let _ = w.write_all(&payload).await;
                        let _ = w.flush().await;
                    }
                }
                return;
            }

            // ctx.ui.confirm → 转成前端审批卡片 → 用户点确认/拒绝 → 写回 pi
            let title = ev.get("title").and_then(Value::as_str).unwrap_or("确认").to_string();
            let message = ev
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let label = if message.is_empty() {
                title
            } else {
                format!("{title}：{message}")
            };

            // 先注册等待通道再弹卡片，避免用户秒点确认时通道还没建
            let (tx, rx) = oneshot::channel::<bool>();
            state.agent.approvals.lock().insert(ui_id.clone(), tx);
            let sent = state.pi_bridge.send_event(AgentEvent::ToolCall {
                id: ui_id.clone(),
                name: "approval".into(),
                title: label,
                input: Value::Null,
                costly: true,
                mutates: true,
                needs_confirm: true,
            });
            tracing::info!("pi 确认请求已投递={sent}（id={ui_id}）");

            let stdin = stdin.clone();
            let st = state.clone();
            tokio::spawn(async move {
                let approved = match tokio::time::timeout(Duration::from_secs(900), rx).await {
                    Ok(Ok(v)) => v,
                    Ok(Err(_)) => false,
                    Err(_) => {
                        st.agent.approvals.lock().remove(&ui_id);
                        false
                    }
                };
                let resp = json!({
                    "type": "extension_ui_response",
                    "id": ui_id,
                    "confirmed": approved,
                });
                if let Ok(mut payload) = serde_json::to_vec(&resp) {
                    payload.push(b'\n');
                    let mut w = stdin.lock().await;
                    let _ = w.write_all(&payload).await;
                    let _ = w.flush().await;
                }
            });
        }
        _ => {
            let _ = events.send(ev);
        }
    }
}

async fn stderr_loop(mut stderr: tokio::process::ChildStderr, tail: Arc<Mutex<Vec<String>>>) {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 8192];
    loop {
        match stderr.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                    let line: Vec<u8> = buf.drain(..=pos).collect();
                    let s = String::from_utf8_lossy(&line).trim().to_string();
                    if s.is_empty() {
                        continue;
                    }
                    let mut t = tail.lock();
                    t.push(s);
                    let len = t.len();
                    if len > STDERR_TAIL_LINES {
                        t.drain(..len - STDERR_TAIL_LINES);
                    }
                }
            }
            Err(_) => break,
        }
    }
}

/* ================================================================== 对外 */

pub async fn status(state: &AppState) -> PiStatus {
    let launch = resolve_launch();
    let running = {
        let mut g = state.pi.inner.lock();
        match g.as_mut() {
            Some(p) => is_alive(&mut p.child),
            None => false,
        }
    };
    match launch {
        Ok(l) => {
            let provider_ok = effective_llm(state)
                .map(|(p, _)| !p.model.trim().is_empty())
                .unwrap_or(false);
            let detail = if !provider_ok {
                "pi 已找到；还需要在设置里配置一个文本模型 provider".to_string()
            } else if running {
                "就绪（进程运行中）".to_string()
            } else {
                "就绪（首次对话时启动进程）".to_string()
            };
            PiStatus {
                available: provider_ok,
                running,
                detail,
                path: Some(l.path_label),
                version: l.version,
            }
        }
        Err(e) => PiStatus {
            available: false,
            running: false,
            detail: e.to_string(),
            path: None,
            version: None,
        },
    }
}

/// 跑一轮对话：确保进程 → 建事件订阅 → prompt → 收事件流直到 settled。
pub async fn run_turn(
    state: &AppState,
    channel: Channel<AgentEvent>,
    input: String,
    image_paths: Vec<String>,
) -> Result<String> {
    // 桥（工具审批卡片等）的事件先进 mpsc，由下面的循环统一转发给前端——
    // 跨线程直接 send Tauri Channel 不可靠，统一从 run 循环发。
    let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();
    state.pi_bridge.set_run_events(Some(ev_tx));
    let result = run_turn_inner(state, channel, input, image_paths, &mut ev_rx).await;
    state.pi_bridge.set_run_events(None);
    result
}

async fn run_turn_inner(
    state: &AppState,
    channel: Channel<AgentEvent>,
    input: String,
    image_paths: Vec<String>,
    ev_rx: &mut tokio::sync::mpsc::UnboundedReceiver<AgentEvent>,
) -> Result<String> {
    let rt = &state.pi;
    let _run_guard = rt
        .run_lock
        .try_lock()
        .map_err(|_| AppError::other("pi 引擎已有一轮对话在进行中"))?;
    rt.ensure(state).await?;

    // 进程信息（会话 id / 会话文件）
    let st = rt.send(json!({"type":"get_state"})).await?;
    let data = st.get("data").cloned().unwrap_or(Value::Null);
    let session_id = data
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or("pi")
        .to_string();
    remember_session(
        state,
        data.get("sessionFile").and_then(Value::as_str),
    );
    // 上一轮没收干净（比如前端刷新）时先中止，避免 prompt 被拒
    if data.get("isStreaming").and_then(Value::as_bool).unwrap_or(false) {
        let _ = tokio::time::timeout(Duration::from_secs(15), rt.send(json!({"type":"abort"}))).await;
    }

    let (provider, _) = effective_llm(state)
        .ok_or_else(|| AppError::Config("没有可用的文本模型 provider".into()))?;
    let run_id = format!("pi_{}", uuid::Uuid::new_v4().simple());

    let mut rx = rt.subscribe()?;
    send(
        &channel,
        AgentEvent::RunStarted {
            run_id: run_id.clone(),
            session_id,
            provider_id: provider.id.clone(),
            model: provider.model.clone(),
        },
    );

    let mut prompt = json!({"type":"prompt","message": input});
    if !image_paths.is_empty() {
        prompt["images"] = images_payload(&image_paths)?;
    }
    rt.send(prompt).await?;

    let mut acc = UsageAcc::default();
    let mut final_text = String::new();
    let mut round: u32 = 0;
    let mut exit_error: Option<String> = None;

    loop {
        tokio::select! {
            r = rx.recv() => match r {
            Ok(ev) => {
                let ty = ev.get("type").and_then(Value::as_str).unwrap_or("");
                match ty {
                    "turn_start" => {
                        round += 1;
                        send(&channel, AgentEvent::Round { round });
                    }
                    "message_update" => {
                        // 注意：message_update 顶层的 usage 是「最新累计值」，累加会虚高，
                        // 所以用量只在 message_end 用单条消息的 usage 统计。
                        let ame = ev.get("assistantMessageEvent").cloned().unwrap_or(Value::Null);
                        match ame.get("type").and_then(Value::as_str).unwrap_or("") {
                            "text_delta" => {
                                if let Some(d) = ame.get("delta").and_then(Value::as_str) {
                                    send(&channel, AgentEvent::TextDelta { text: d.to_string() });
                                }
                            }
                            "thinking_delta" => {
                                if let Some(d) = ame.get("delta").and_then(Value::as_str) {
                                    send(
                                        &channel,
                                        AgentEvent::ReasoningDelta { text: d.to_string() },
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                    "message_end" => {
                        let msg = ev.get("message").cloned().unwrap_or(Value::Null);
                        if msg.get("role").and_then(Value::as_str) == Some("assistant") {
                            acc.observe(msg.get("usage"));
                            final_text = assistant_text(&msg).unwrap_or(final_text);
                        }
                    }
                    "tool_execution_start" => {
                        // M1 禁用了全部工具，这里理论上不会出现；M2 接工具桥后启用
                        let id = ev.get("toolCallId").and_then(Value::as_str).unwrap_or("").to_string();
                        let name = ev.get("toolName").and_then(Value::as_str).unwrap_or("tool").to_string();
                        send(
                            &channel,
                            AgentEvent::ToolCall {
                                id,
                                title: name.clone(),
                                name,
                                input: ev.get("args").cloned().unwrap_or(Value::Null),
                                costly: false,
                                mutates: false,
                                needs_confirm: false,
                            },
                        );
                    }
                    "tool_execution_end" => {
                        let id = ev.get("toolCallId").and_then(Value::as_str).unwrap_or("").to_string();
                        let name = ev.get("toolName").and_then(Value::as_str).unwrap_or("tool").to_string();
                        let ok = !ev.get("isError").and_then(Value::as_bool).unwrap_or(false);
                        // 工具结果原文带给前端，卡片可以展开看全（不含图片，本地 IPC 体积可控）
                        let data = ev.get("result").cloned().unwrap_or(Value::Null);
                        send(
                            &channel,
                            AgentEvent::ToolResult {
                                id,
                                name,
                                ok,
                                summary: summarize_tool_result(&ev),
                                data,
                                duration_ms: 0,
                            },
                        );
                    }
                    "compaction_end" => {
                        if let Some(res) = ev.get("result") {
                            let before = res.get("tokensBefore").and_then(Value::as_u64).unwrap_or(0);
                            let after = res
                                .get("estimatedTokensAfter")
                                .and_then(Value::as_u64)
                                .unwrap_or(0);
                            send(&channel, AgentEvent::ContextCompacted { before, after });
                        }
                    }
                    "auto_retry_start" => {
                        let attempt = ev.get("attempt").and_then(Value::as_u64).unwrap_or(0);
                        let msg = ev
                            .get("errorMessage")
                            .and_then(Value::as_str)
                            .unwrap_or("上游错误");
                        send(
                            &channel,
                            AgentEvent::Error {
                                message: format!(
                                    "上游出错，pi 正在自动重试（第 {attempt} 次）：{}",
                                    crate::llm::truncate(msg, 200)
                                ),
                            },
                        );
                    }
                    "agent_settled" => break,
                    "__pi_exit" => {
                        exit_error = Some(describe_exit(state).await);
                        break;
                    }
                    _ => {}
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                send(
                    &channel,
                    AgentEvent::Error {
                        message: format!("事件流落后，丢弃了 {n} 条事件"),
                    },
                );
            }
            Err(broadcast::error::RecvError::Closed) => {
                exit_error = Some("pi 事件通道已关闭".into());
                break;
            }
            },
            // 桥投递的事件（工具审批卡片等）直接转发给前端
            Some(ev) = ev_rx.recv() => {
                tracing::info!("桥事件已转发给前端：{}", serde_json::to_string(&ev).map(|s| s.chars().take(120).collect::<String>()).unwrap_or_default());
                send(&channel, ev);
            }
        }
    }

    send(&channel, AgentEvent::Usage { report: acc.report() });
    send(
        &channel,
        AgentEvent::RunFinished {
            run_id: run_id.clone(),
            stop_reason: if exit_error.is_some() { "error".into() } else { "stop".into() },
            text: final_text,
            error: exit_error,
        },
    );
    Ok(run_id)
}

/// pi 会话历史（浮窗切到 pi 引擎时拉取渲染）：只保留 user/assistant 的文本，
/// 工具结果与 bash 执行记录不进气泡。
pub async fn history(state: &AppState) -> Result<Vec<Value>> {
    let rt = &state.pi;
    rt.ensure(state).await?;
    let r = rt.send(json!({"type":"get_messages"})).await?;
    let msgs = r
        .get("data")
        .and_then(|d| d.get("messages"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for (i, m) in msgs.iter().enumerate() {
        let role = m.get("role").and_then(Value::as_str).unwrap_or("");
        let text = match role {
            "user" => content_text(m.get("content")),
            "assistant" => assistant_text(m).unwrap_or_default(),
            _ => continue,
        };
        if text.trim().is_empty() {
            continue;
        }
        out.push(json!({
            "id": format!("pi_h_{i}"),
            "role": role,
            "text": text,
            "createdAt": "",
        }));
    }
    Ok(out)
}

/// user 消息的 content 可能是字符串或 blocks 数组
fn content_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(Value::as_str) == Some("text") {
                    b.get("text").and_then(Value::as_str).map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// 进程异常退出时，把 stderr 尾部（pi 的报错通常在这）带给用户。
async fn describe_exit(state: &AppState) -> String {
    let tail = state.pi.stderr_tail();
    if tail.is_empty() {
        "pi 进程已退出".to_string()
    } else {
        format!(
            "pi 进程已退出：{}",
            crate::llm::truncate(&tail.join("\n"), 500)
        )
    }
}

/* ================================================================== 工具 */

fn send(ch: &Channel<AgentEvent>, ev: AgentEvent) {
    let _ = ch.send(ev);
}

/// 图片转 pi 的 ImageContent（base64）
fn images_payload(paths: &[String]) -> Result<Value> {
    use base64::Engine;
    let mut out = Vec::new();
    for p in paths {
        let bytes = std::fs::read(p)?;
        let ext = Path::new(p)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_ascii_lowercase();
        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            _ => "image/png",
        };
        out.push(json!({
            "type": "image",
            "data": base64::engine::general_purpose::STANDARD.encode(bytes),
            "mimeType": mime,
        }));
    }
    Ok(Value::Array(out))
}

/// 从 AssistantMessage 里取拼接后的文本
fn assistant_text(msg: &Value) -> Option<String> {
    let content = msg.get("content")?;
    let mut out = String::new();
    if let Some(arr) = content.as_array() {
        for block in arr {
            if block.get("type").and_then(Value::as_str) == Some("text") {
                if let Some(t) = block.get("text").and_then(Value::as_str) {
                    out.push_str(t);
                }
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn summarize_tool_result(ev: &Value) -> String {
    let text = ev
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    crate::llm::truncate(&text, 160)
}

/// 本次 run 的用量累计（pi 的 usage 字段语义：单条消息或累计值，都按最新值采纳）
#[derive(Default)]
struct UsageAcc {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
}

impl UsageAcc {
    fn observe(&mut self, usage: Option<&Value>) {
        let Some(u) = usage else { return };
        let get = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
        self.input += get("input");
        self.output += get("output");
        self.cache_read += get("cacheRead");
        self.cache_write += get("cacheWrite");
    }

    fn report(&self) -> UsageReport {
        let total_input = self.input + self.cache_read;
        let hit_rate = if total_input > 0 {
            self.cache_read as f32 / total_input as f32
        } else {
            0.0
        };
        UsageReport {
            input_tokens: self.input,
            output_tokens: self.output,
            cache_read_tokens: self.cache_read,
            cache_write_tokens: self.cache_write,
            total_input_tokens: self.input,
            total_output_tokens: self.output,
            total_cache_read_tokens: self.cache_read,
            total_cache_write_tokens: self.cache_write,
            hit_rate,
        }
    }
}
