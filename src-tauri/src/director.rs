//! 导演台（DirectorDesk）桥：让两个 agent 引擎都能操作 3D 预演工程。
//!
//! 集成形态：导演台以 iframe 承载在「3D 预演」面板里，它把自己暴露成
//! `window.__director`（读工程 / 替换工程 / 调工具 / 导出视频）。前端面板
//! 直接调用这些接口；Rust 侧的 agent 工具通过 **事件 + oneshot** 把请求
//! 转给前端执行（与审批通道同构）：
//!
//! ```text
//! agent 工具 director_tool/director_scene
//!   └─ call_director() ──emit "director://call"──▶ 前端面板
//!         ▲                                          └─ iframe.__director.callTool / getDocument
//!         └──oneshot◀── director_result(callId) ──────┘
//! ```
//!
//! 工程数据随项目保存：`<项目>/previz/director.json`。

use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::error::{AppError, Result};
use crate::state::AppState;

pub const EVENT_DIRECTOR_CALL: &str = "director://call";

/// 让前端（iframe 里的导演台）执行一个操作并等待回执。
pub async fn call_director(state: &AppState, name: &str, args: Value) -> Result<Value> {
    let app = state
        .app
        .read()
        .clone()
        .ok_or_else(|| AppError::other("应用未就绪"))?;
    let call_id = format!("dc_{}", uuid::Uuid::new_v4().simple());
    let (tx, rx) = oneshot::channel::<Result<Value>>();
    state.agent.director_calls.lock().insert(call_id.clone(), tx);
    let _ = tauri::Emitter::emit(
        &app,
        EVENT_DIRECTOR_CALL,
        json!({"callId": call_id, "name": name, "args": args}),
    );
    match tokio::time::timeout(Duration::from_secs(60), rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(AppError::other("导演台未响应（面板可能没打开）")),
        Err(_) => {
            state.agent.director_calls.lock().remove(&call_id);
            Err(AppError::other(
                "导演台执行超时（60s）——先在「3D预演 → 导演台」里把面板打开再试",
            ))
        }
    }
}

/// 前端回填执行结果。
pub fn resolve_call(state: &AppState, call_id: &str, ok: bool, data: Value, error: Option<String>) {
    if let Some(tx) = state.agent.director_calls.lock().remove(call_id) {
        let _ = tx.send(if ok {
            Ok(data)
        } else {
            Err(AppError::other(error.unwrap_or_else(|| "未知错误".into())))
        });
    }
}
