//! pi 工具桥：让 pi 引擎调用工作台的 46 个工具。
//!
//! pi 的 extension 跑在 sidecar 进程里，碰不到 Rust 的状态，所以这里起一个
//! **只监听 127.0.0.1** 的极简 HTTP 服务，extension 通过它回调：
//!
//! - `GET  /ping`        —— 连通性自检
//! - `GET  /tools`       —— 工具清单（extension 启动时注册用）
//! - `POST /tool/{name}` —— 执行一个工具，返回 ToolOutcome
//! - `POST /approve`     —— 工具调用前的审批；需要用户确认时走现有审批通道
//!   （emit ToolCall 事件 → 前端弹卡片 → agent_approve → oneshot 回填）
//!
//! 鉴权：启动时生成随机 token，请求必须带 `x-pi-bridge-token` 头。
//!
//! 工具实现复用 `agent::tools::registry`：**只有一份实现**，pi 与自研引擎
//! 的副作用、审批、事件完全一致。

use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::agent::tools::{self, ToolCtx};
use crate::agent::AgentEvent;
use crate::error::{AppError, Result};
use crate::models::PanelId;
use crate::state::AppState;

pub const TOKEN_HEADER: &str = "x-pi-bridge-token";
const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Default)]
pub struct Bridge {
    info: Mutex<Option<BridgeInfo>>,
    /// 当前 pi run 的事件接收端：审批卡片等事件先进 mpsc，
    /// 由 run_turn 的事件循环统一转发给前端 Channel（跨线程发 Channel 不可靠）。
    run_events: Mutex<Option<tokio::sync::mpsc::UnboundedSender<AgentEvent>>>,
}

#[derive(Clone)]
pub struct BridgeInfo {
    pub port: u16,
    pub token: String,
    pub url: String,
}

impl Bridge {
    /// 幂等启动，返回连接信息。
    pub async fn ensure(&self, state: &AppState) -> Result<BridgeInfo> {
        if let Some(info) = self.info.lock().clone() {
            return Ok(info);
        }
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| AppError::other(format!("pi 桥接端口绑定失败：{e}")))?;
        let port = listener.local_addr()?.port();
        let token = random_token();
        let info = BridgeInfo {
            port,
            token,
            url: format!("http://127.0.0.1:{port}"),
        };
        let st = state.clone();
        tokio::spawn(accept_loop(listener, st));
        *self.info.lock() = Some(info.clone());
        Ok(info)
    }

    pub fn info(&self) -> Option<BridgeInfo> {
        self.info.lock().clone()
    }

    pub fn set_run_events(&self, tx: Option<tokio::sync::mpsc::UnboundedSender<AgentEvent>>) {
        *self.run_events.lock() = tx;
    }

    /// 把事件投递给当前 run 的转发循环；返回是否送达（没有 run 时 false）。
    pub fn send_event(&self, ev: AgentEvent) -> bool {
        let guard = self.run_events.lock();
        match guard.as_ref() {
            Some(tx) => tx.send(ev).is_ok(),
            None => false,
        }
    }
}

fn random_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/* ================================================================ 服务端 */

async fn accept_loop(listener: TcpListener, state: AppState) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let st = state.clone();
                tokio::spawn(handle_conn(stream, st));
            }
            Err(_) => {
                // 单次 accept 失败不致命（比如句柄耗尽），退避后继续
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

async fn handle_conn(mut stream: TcpStream, state: AppState) {
    let mut buf: Vec<u8> = Vec::with_capacity(8192);
    let mut tmp = vec![0u8; 16384];

    // 读头
    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > MAX_HEADER_BYTES {
            return;
        }
        match stream.read(&mut tmp).await {
            Ok(0) => return,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(_) => return,
        }
    };

    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut token = String::new();
    let mut content_length: usize = 0;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
        if lower.starts_with(TOKEN_HEADER) {
            if let Some(i) = line.find(':') {
                token = line[i + 1..].trim().to_string();
            }
        }
    }

    if content_length > MAX_BODY_BYTES {
        respond(&mut stream, 413, &json!({"error":"body too large"})).await;
        return;
    }
    let mut body = buf[header_end..].to_vec();
    while body.len() < content_length {
        match stream.read(&mut tmp).await {
            Ok(0) => break,
            Ok(n) => body.extend_from_slice(&tmp[..n]),
            Err(_) => break,
        }
    }
    body.truncate(content_length);

    // 鉴权（token 未初始化时一律拒绝）
    let expected = state.pi_bridge.info().map(|i| i.token).unwrap_or_default();
    if expected.is_empty() || token != expected {
        respond(&mut stream, 401, &json!({"error":"unauthorized"})).await;
        return;
    }

    let resp = route(&state, &method, &path, &body).await;
    let status = if resp.get("error").and_then(Value::as_str) == Some("not found") {
        404
    } else {
        200
    };
    respond(&mut stream, status, &resp).await;
}

async fn respond(stream: &mut TcpStream, status: u16, body: &Value) {
    let payload = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
    let head = format!(
        "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        payload.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(&payload).await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
}

async fn route(state: &AppState, method: &str, path: &str, body: &[u8]) -> Value {
    if method == "GET" && path == "/ping" {
        return json!({"ok": true, "app": "duanju-workbench"});
    }
    if method == "GET" && path == "/tools" {
        return tools_payload();
    }
    if method == "POST" && path == "/approve-check" {
        return approval(state, body).await;
    }
    if method == "POST" && path.starts_with("/tool/") {
        let name = &path["/tool/".len()..];
        let args: Value = serde_json::from_slice(body).unwrap_or_else(|_| json!({}));
        return match run_tool(state, name, args).await {
            Ok(v) => json!({"ok": true, "result": v}),
            Err(e) => json!({"ok": false, "error": e.to_string()}),
        };
    }
    json!({"error": "not found"})
}

/* ================================================================== 工具 */

/// 工具清单：extension 启动时按这个注册 pi 工具。
pub fn tools_payload() -> Value {
    let list: Vec<Value> = tools::registry(PanelId::Script)
        .iter()
        .map(|t| {
            json!({
                "name": t.spec.name,
                "title": t.spec.title,
                "description": t.spec.description,
                "inputSchema": t.spec.input_schema,
                "costly": t.spec.costly,
                "mutates": !tools::is_read_only(&t.spec.name),
            })
        })
        .collect();
    json!({"tools": list})
}

/// 执行一个工具。与自研引擎走同一个 registry，行为一致。
pub async fn run_tool(state: &AppState, name: &str, args: Value) -> Result<Value> {
    let all = tools::registry(PanelId::Script);
    let tool = all
        .iter()
        .find(|t| t.spec.name == name)
        .ok_or_else(|| AppError::NotFound(format!("未知工具：{name}")))?;
    let ctx = ToolCtx {
        state: state.clone(),
        panel: PanelId::Script,
    };
    let out = (tool.run)(ctx, args).await?;
    // images 在 ToolOutcome 上是 #[serde(skip)]（自研引擎直接读字段），
    // 这里手动带上，extension 会把它们作为图片块回给 pi，模型就能「看见」。
    let images: Vec<Value> = out
        .images
        .iter()
        .map(|i| {
            json!({
                "label": i.label,
                "mediaType": i.media_type,
                "data": i.data_b64,
            })
        })
        .collect();
    Ok(json!({
        "summary": out.summary,
        "data": out.data,
        "images": images,
    }))
}

/* ================================================================== 审批 */

/// 审批判定（**立即返回，不等待用户**）：与自研 agent 的三模式规则一致
/// （yolo 不问 / confirm 全问 / auto 只问花钱的）。
/// 需要确认时由 extension 侧走 pi 的 `ctx.ui.confirm()`，用户交互经
/// extension UI 子协议回到 pi.rs 转发给前端——HTTP 请求不承担长等待。
async fn approval(state: &AppState, body: &[u8]) -> Value {
    let v: Value = serde_json::from_slice(body).unwrap_or_else(|_| json!({}));
    let name = v.get("toolName").and_then(Value::as_str).unwrap_or("");

    let spec = tools::registry(PanelId::Script)
        .iter()
        .find(|t| t.spec.name == name)
        .map(|t| (t.spec.title.clone(), t.spec.costly));
    let (title, costly) = spec.unwrap_or_else(|| (name.to_string(), false));
    let mutates = !tools::is_read_only(name);
    let mode = state.settings().agent_mode;

    let needs_confirm = match mode.as_str() {
        "yolo" => false,
        "confirm" => mutates,
        _ => costly, // auto
    };
    json!({
        "needsConfirm": needs_confirm,
        "title": title,
        "costly": costly,
        "mutates": mutates,
    })
}

/* ================================================================== 便捷 */

/// 桥接信息（供 pi 启动参数与 extension 环境变量使用）。
pub fn bridge_env(state: &AppState) -> Option<(String, String)> {
    state
        .pi_bridge
        .info()
        .map(|i| (i.url.clone(), i.token.clone()))
}
