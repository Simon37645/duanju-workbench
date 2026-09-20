//! Agent 会话与工具循环。
//!
//! 一次 `run_turn` 的流程：
//!   1. 取出（或新建）会话，确保**冻结前缀**存在；
//!   2. 追加用户消息（会话首条消息里带上冻结的上下文快照）；
//!   3. 循环：调模型 → 流式回传 → 若有工具调用则执行 → 把结果作为新消息追加到尾 部；
//!   4. 全程把事件推给前端（文本增量、工具卡片、用量与缓存命中）。
//!
//! 缓存相关的两个不变量，改代码时别破坏：
//!   * `ChatRequest.system` 一旦冻结就不再变；
//!   * `messages` 只追加，历史消息永不改写。

pub mod context;
pub mod prompt;
pub mod tools;

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::ipc::Channel;
use tokio::sync::oneshot;

use crate::error::{AppError, Result};
use crate::llm::{
    self, Block, ChatRequest, Message, ProviderHandle, Role, StreamEvent, Usage,
};
use crate::models::*;
use crate::state::AppState;
use prompt::FrozenPrefix;

pub use tools::ToolCtx;

/* ================================================================== 会话 */

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayToolCall {
    pub id: String,
    pub name: String,
    pub title: String,
    pub input: Value,
    pub summary: String,
    pub ok: bool,
    pub costly: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayMessage {
    pub id: String,
    /// user | assistant | system
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub reasoning: String,
    #[serde(default)]
    pub tool_calls: Vec<DisplayToolCall>,
    #[serde(default)]
    pub images: Vec<String>,
    pub created_at: String,
}

impl DisplayMessage {
    fn new(role: &str, text: impl Into<String>) -> Self {
        Self {
            id: new_id("msg"),
            role: role.into(),
            text: text.into(),
            reasoning: String::new(),
            tool_calls: vec![],
            images: vec![],
            created_at: now_iso(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: String,
    pub project_id: String,
    pub panel: PanelId,
    pub title: String,
    /// 给模型看的消息（含工具调用与结果）
    pub messages: Vec<Message>,
    /// 给界面看的消息
    pub display: Vec<DisplayMessage>,
    /// 冻结前缀；重建它意味着一次缓存失效
    pub prefix: Option<FrozenPrefix>,
    /// 上一轮用过的主体模型，用于判断缓存前缀是否延续
    pub last_provider_id: Option<String>,
    pub last_model: Option<String>,
    pub turns: u32,
    #[serde(default)]
    pub total_usage: Usage,
    #[serde(default)]
    pub total_cache_read: u64,
    #[serde(default)]
    pub total_input: u64,
    pub created_at: String,
    pub updated_at: String,
}

impl AgentSession {
    pub fn new(project_id: &str, panel: PanelId, prefix: FrozenPrefix) -> Self {
        Self {
            id: new_id("sess"),
            project_id: project_id.to_string(),
            panel,
            title: "新对话".into(),
            messages: vec![],
            display: vec![],
            prefix: Some(prefix),
            last_provider_id: None,
            last_model: None,
            turns: 0,
            total_usage: Usage::default(),
            total_cache_read: 0,
            total_input: 0,
            created_at: now_iso(),
            updated_at: now_iso(),
        }
    }
}

/* ================================================================== 事件 */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_write_tokens: u64,
    pub hit_rate: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum AgentEvent {
    // 注意：serde 的容器级 rename_all 只改变体名，**不改 struct variant 的字段**，
    // 所以每个带多词字段的变体都要单独标 rename_all（前端按 camelCase 读）。
    #[serde(rename_all = "camelCase")]
    RunStarted {
        run_id: String,
        session_id: String,
        provider_id: String,
        model: String,
    },
    Prefix {
        report: prompt::PrefixReport,
    },
    ReasoningDelta {
        text: String,
    },
    TextDelta {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    ToolCall {
        id: String,
        name: String,
        title: String,
        input: Value,
        costly: bool,
        /// 会不会改项目数据
        mutates: bool,
        needs_confirm: bool,
    },
    #[serde(rename_all = "camelCase")]
    ToolResult {
        id: String,
        name: String,
        ok: bool,
        summary: String,
        data: Value,
        duration_ms: u64,
    },
    Usage {
        report: UsageReport,
    },
    /// agent 主动向用户提问，前端渲染成卡片并等回答
    AskUser {
        id: String,
        question: String,
        options: Vec<String>,
        why: String,
    },
    Round {
        round: u32,
    },
    /// 上下文水位
    Context {
        stats: context::ContextStats,
    },
    /// 刚做过一次自动压缩
    ContextCompacted {
        before: u64,
        after: u64,
    },
    #[serde(rename_all = "camelCase")]
    RunFinished {
        run_id: String,
        stop_reason: String,
        text: String,
        error: Option<String>,
    },
    Error {
        message: String,
    },
}

fn send(ch: &Channel<AgentEvent>, ev: AgentEvent) {
    let _ = ch.send(ev);
}

/* ============================================================== 主流程 */

pub struct RunOptions {
    pub panel: PanelId,
    pub session_id: Option<String>,
    pub input: String,
    pub image_paths: Vec<String>,
    pub refresh_context: bool,
}

pub async fn run_turn(
    state: AppState,
    channel: Channel<AgentEvent>,
    opts: RunOptions,
) -> Result<String> {
    let project = state.current()?;
    let provider = resolve_provider(&state)?;
    let provider_id = provider.config.id.clone();
    let model = provider.provider.model();

    // 1) 会话
    let mut session = match opts
        .session_id
        .as_deref()
        .and_then(|id| state.session(id))
    {
        Some(s) if s.project_id == project.manifest.id => s,
        _ => AgentSession::new(
            &project.manifest.id,
            opts.panel,
            prompt::build_frozen_prefix(&project, opts.panel, knowledge_index(&state))?,
        ),
    };

    // 前缀需要重建的两种情况：显式刷新，或换到了别的模型（不同模型的缓存不通用）
    let prev_fingerprint = session.prefix.as_ref().map(|p| p.fingerprint.clone());
    let model_changed = session
        .last_model
        .as_deref()
        .map(|m| m != model)
        .unwrap_or(false);
    if session.prefix.is_none() || opts.refresh_context || model_changed {
        session.prefix = Some(prompt::build_frozen_prefix(
            &project,
            opts.panel,
            knowledge_index(&state),
        )?);
    }
    let prefix = session.prefix.clone().unwrap();

    // 2) 用户消息（首条带上下文快照）
    let mut blocks: Vec<Block> = vec![];
    if session.messages.is_empty() {
        blocks.push(Block::text(format!(
            "{}\n\n---\n\n{}",
            prefix.context, opts.input
        )));
    } else {
        blocks.push(Block::text(opts.input.clone()));
    }
    // @ 引用：用户点名的技能 / 章节 / 资产，按需带进来
    let mentioned = context::resolve_mentions(&state, &opts.input);
    if !mentioned.is_empty() {
        blocks.extend(mentioned);
    }

    for path in &opts.image_paths {
        let p = std::path::Path::new(path);
        match llm::image_block_from_file(p, 12 * 1024 * 1024) {
            Ok(b) => blocks.push(b),
            Err(e) => {
                send(
                    &channel,
                    AgentEvent::Error {
                        message: format!("读取图片失败：{e}"),
                    },
                );
            }
        }
    }
    session.messages.push(Message {
        role: Role::User,
        content: blocks,
    });
    let mut user_display = DisplayMessage::new("user", opts.input.clone());
    user_display.images = opts.image_paths.clone();
    session.display.push(user_display);
    if session.title == "新对话" {
        session.title = crate::llm::truncate(opts.input.trim(), 24);
    }

    let run_id = new_id("run");
    send(
        &channel,
        AgentEvent::RunStarted {
            run_id: run_id.clone(),
            session_id: session.id.clone(),
            provider_id: provider_id.clone(),
            model: model.clone(),
        },
    );

    // 3) 前缀报告：告诉前端这一轮能不能吃到缓存
    let first_request = session.turns == 0;
    let stable = !first_request && prev_fingerprint.as_deref() == Some(prefix.fingerprint.as_str());
    send(
        &channel,
        AgentEvent::Prefix {
            report: prefix.report(stable),
        },
    );

    let settings = state.settings();
    let max_rounds = settings.max_tool_rounds.max(1);
    let mode = settings.agent_mode.clone();
    let tools = tools::registry(opts.panel);
    let mut assistant_display = DisplayMessage::new("assistant", "");
    let mut stop_reason = "endTurn".to_string();
    let mut final_text = String::new();

    // 4) 工具循环
    for round in 0..max_rounds {
        // 用户点了「停止」：不再继续后续轮次（正在执行的那一步会跑完）
        if state.agent.abort_requests.lock().remove(&session.id) {
            stop_reason = "aborted".into();
            send(
                &channel,
                AgentEvent::Error {
                    message: "已被用户停止（当前步骤已收尾）".into(),
                },
            );
            break;
        }
        send(&channel, AgentEvent::Round { round: round + 1 });

        // 进模型之前先看上下文水位；超了就压缩（第一刀免费，必要时才花一次调用）
        let before = context::stats(&state, &session);
        if before.over_threshold && settings.auto_compact {
            match context::compact(&state, &provider, &session.id, false).await {
                Ok(after) => {
                    session = state.session(&session.id).unwrap_or(session);
                    send(
                        &channel,
                        AgentEvent::ContextCompacted {
                            before: before.messages_tokens,
                            after: after.messages_tokens,
                        },
                    );
                }
                Err(e) => tracing::warn!("自动压缩失败：{e}"),
            }
        }
        send(
            &channel,
            AgentEvent::Context {
                stats: context::stats(&state, &session),
            },
        );

        let req = ChatRequest {
            model: model.clone(),
            system: prefix.layer_texts(),
            messages: session.messages.clone(),
            tools: tools.iter().map(|t| t.spec.clone()).collect(),
            max_tokens: provider
                .config
                .options
                .get("maxTokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(8000) as u32,
            temperature: provider
                .config
                .options
                .get("temperature")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
                .unwrap_or(0.7),
            cache_tools: true,
            cache_conversation: true,
        };

        // 流式收集
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<StreamEvent>();
        let ch = channel.clone();
        let collector = tokio::spawn(async move {
            let mut text = String::new();
            let mut reasoning = String::new();
            let mut usage = Usage::default();
            let mut stop = String::new();
            let mut order: Vec<String> = vec![];
            let mut names: std::collections::BTreeMap<String, String> = Default::default();
            let mut args: std::collections::BTreeMap<String, String> = Default::default();
            while let Some(ev) = rx.recv().await {
                match ev {
                    StreamEvent::TextDelta(t) => {
                        text.push_str(&t);
                        send(&ch, AgentEvent::TextDelta { text: t });
                    }
                    StreamEvent::ReasoningDelta(t) => {
                        reasoning.push_str(&t);
                        send(&ch, AgentEvent::ReasoningDelta { text: t });
                    }
                    StreamEvent::ToolCallStart { id, name } => {
                        args.entry(id.clone()).or_default();
                        names.insert(id.clone(), name);
                        if !order.contains(&id) {
                            order.push(id);
                        }
                    }
                    StreamEvent::ToolCallArgsDelta { id, delta } => {
                        args.entry(id).or_default().push_str(&delta);
                    }
                    StreamEvent::Usage(u) => usage.merge(&u),
                    StreamEvent::StopReason(s) => stop = s,
                    StreamEvent::Done => {}
                }
            }
            (text, reasoning, usage, stop, order, names, args)
        });

        let stream_result = provider.provider.stream(req, tx).await;
        let (text, reasoning, usage, stop, order, names, args) = collector
            .await
            .map_err(|e| AppError::other(format!("收集流式输出失败: {e}")))?;

        if let Err(e) = stream_result {
            let msg = e.to_string();
            send(
                &channel,
                AgentEvent::Error {
                    message: msg.clone(),
                },
            );
            session.display.push(assistant_display.clone());
            final_text = text;
            stop_reason = "error".into();
            state.put_session(session.clone());
            let _ = state.persist_sessions();
            send(
                &channel,
                AgentEvent::RunFinished {
                    run_id,
                    stop_reason,
                    text: final_text,
                    error: Some(msg),
                },
            );
            return Ok(session.id);
        }

        // 累计用量
        session.total_usage.merge(&usage);
        session.total_input += usage.input_tokens;
        session.total_cache_read += usage.cache_read_tokens;
        let total_reported = session.total_usage.input_tokens + session.total_usage.cache_read_tokens;
        send(
            &channel,
            AgentEvent::Usage {
                report: UsageReport {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    cache_read_tokens: usage.cache_read_tokens,
                    cache_write_tokens: usage.cache_write_tokens,
                    total_input_tokens: session.total_usage.input_tokens,
                    total_output_tokens: session.total_usage.output_tokens,
                    total_cache_read_tokens: session.total_usage.cache_read_tokens,
                    total_cache_write_tokens: session.total_usage.cache_write_tokens,
                    hit_rate: if total_reported == 0 {
                        0.0
                    } else {
                        session.total_usage.cache_read_tokens as f32 / total_reported as f32
                    },
                },
            },
        );

        // 组装 assistant 消息
        let mut assistant_blocks: Vec<Block> = vec![];
        if !text.is_empty() {
            assistant_blocks.push(Block::text(text.clone()));
        }
        let mut calls: Vec<(String, String, Value)> = vec![];
        for id in &order {
            let name = names.get(id).cloned().unwrap_or_default();
            let input = llm::parse_tool_args(args.get(id).map(|s| s.as_str()).unwrap_or(""));
            assistant_blocks.push(Block::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            });
            calls.push((id.clone(), name, input));
        }
        if !assistant_blocks.is_empty() {
            session.messages.push(Message {
                role: Role::Assistant,
                content: assistant_blocks,
            });
        }
        if !text.is_empty() {
            assistant_display.text = text.clone();
            final_text = text.clone();
        }
        if !reasoning.is_empty() {
            assistant_display.reasoning = reasoning;
        }

        if calls.is_empty() {
            stop_reason = if stop.is_empty() {
                "endTurn".into()
            } else {
                stop
            };
            session.display.push(assistant_display.clone());
            session.turns += 1;
            break;
        }

        // 执行工具
        let mut result_blocks: Vec<Block> = vec![];
        let mut vision_feedback: Vec<Block> = vec![];
        for (id, name, input) in calls {
            let spec = tools.iter().find(|t| t.spec.name == name);
            let title = spec
                .map(|t| t.spec.title.clone())
                .unwrap_or_else(|| name.clone());
            let costly = spec.map(|t| t.spec.costly).unwrap_or(false);
            let mutates = !tools::is_read_only(&name);
            // 三种权限模式
            let needs_confirm = match mode.as_str() {
                "yolo" => false,
                "confirm" => mutates,
                _ => costly, // auto：自动改数据，只有花钱的才问
            };

            send(
                &channel,
                AgentEvent::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    title: title.clone(),
                    input: input.clone(),
                    costly,
                    mutates,
                    needs_confirm,
                },
            );

            let mut call_display = DisplayToolCall {
                id: id.clone(),
                name: name.clone(),
                title,
                input: input.clone(),
                summary: String::new(),
                ok: true,
                costly,
                duration_ms: 0,
            };

            if needs_confirm && !wait_for_approval(&state, &id).await {
                let msg = "用户拒绝了这次操作".to_string();
                call_display.ok = false;
                call_display.summary = msg.clone();
                assistant_display.tool_calls.push(call_display);
                send(
                    &channel,
                    AgentEvent::ToolResult {
                        id: id.clone(),
                        name: name.clone(),
                        ok: false,
                        summary: msg.clone(),
                        data: Value::Null,
                        duration_ms: 0,
                    },
                );
                result_blocks.push(Block::ToolResult {
                    tool_use_id: id.clone(),
                    content: msg,
                    is_error: true,
                });
                continue;
            }

            let started = Instant::now();
            let outcome = if name == "ask_user" {
                // 提问：把问题推给前端，挂起等用户回答，回答直接作为工具结果
                let question = input
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let why = input
                    .get("why")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let options: Vec<String> = input
                    .get("options")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                send(
                    &channel,
                    AgentEvent::AskUser {
                        id: id.clone(),
                        question: question.clone(),
                        options,
                        why,
                    },
                );
                match wait_for_answer(&state, &id).await {
                    Some(a) => Ok(tools::ToolOutcome::new(
                        format!("用户回答：{a}"),
                        Value::String(a),
                    )),
                    None => Err(AppError::other("用户没有回答（等待超时）")),
                }
            } else {
                match spec {
                    Some(t) => {
                        let ctx = ToolCtx {
                            state: state.clone(),
                            panel: opts.panel,
                        };
                        (t.run)(ctx, input).await
                    }
                    None => Err(AppError::NotFound(format!("工具不存在：{name}"))),
                }
            };
            let ms = started.elapsed().as_millis() as u64;

            let (ok, summary, data, content, is_error) = match outcome {
                Ok(o) => {
                    let content = if o.data.is_null() {
                        o.summary.clone()
                    } else {
                        format!(
                            "{}\n{}",
                            o.summary,
                            crate::llm::truncate(&o.data.to_string(), 6000)
                        )
                    };
                    // 工具带回来的图片，作为一条独立消息追加在工具结果之后，
                    // 这样两家协议都能吃：OpenAI 的 tool 消息不能带图，
                    // Anthropic 则会把连续的同角色消息合并，效果一样。
                    for img in &o.images {
                        vision_feedback.push(Block::Text {
                            text: format!("【图片】{}", img.label),
                        });
                        vision_feedback.push(Block::Image {
                            media_type: img.media_type.clone(),
                            data: img.data_b64.clone(),
                        });
                    }
                    (true, o.summary, o.data, content, false)
                }
                Err(e) => {
                    let m = e.to_string();
                    (false, m.clone(), Value::Null, m, true)
                }
            };

            call_display.ok = ok;
            call_display.summary = summary.clone();
            call_display.duration_ms = ms;
            assistant_display.tool_calls.push(call_display);

            send(
                &channel,
                AgentEvent::ToolResult {
                    id: id.clone(),
                    name: name.clone(),
                    ok,
                    summary: summary.clone(),
                    data: data.clone(),
                    duration_ms: ms,
                },
            );
            result_blocks.push(Block::ToolResult {
                tool_use_id: id.clone(),
                content,
                is_error,
            });
        }

        session.messages.push(Message {
            role: Role::User,
            content: result_blocks,
        });

        // 有图就把图作为下一条用户消息塞回去，模型下一轮就能看到
        if !vision_feedback.is_empty() {
            let mut content = vec![Block::text(
                "以下是刚才工具返回的图片，请结合它们继续。",
            )];
            content.extend(vision_feedback);
            session.messages.push(Message {
                role: Role::User,
                content,
            });
            assistant_display.text.push_str(&format!(
                "\n\n（已载入 {} 张图片到视觉上下文）",
                session
                    .messages
                    .last()
                    .map(|m| m
                        .content
                        .iter()
                        .filter(|b| matches!(b, Block::Image { .. }))
                        .count())
                    .unwrap_or(0)
            ));
        }
        session.turns += 1;

        if round + 1 == max_rounds {
            stop_reason = "maxTokens".into();
            let note = format!("已达到最大工具轮次（{max_rounds}），本轮到此为止。");
            assistant_display.text.push_str(&format!("\n\n{note}"));
        }
    }

    if !assistant_display.text.is_empty() || !assistant_display.tool_calls.is_empty() {
        if !session.display.contains(&assistant_display) {
            session.display.push(assistant_display.clone());
        }
    }

    // 刷新自动检查项，并把会话落盘
    let _ = tools::refresh_auto_items(&state);
    session.updated_at = now_iso();
    session.last_provider_id = Some(provider_id);
    session.last_model = Some(model);
    state.put_session(session.clone());
    let _ = state.persist_sessions();

    send(
        &channel,
        AgentEvent::RunFinished {
            run_id,
            stop_reason,
            text: final_text,
            error: None,
        },
    );
    Ok(session.id)
}

/// 等用户对花钱操作放行。pi 桥接的工具审批复用同一通道。
pub async fn wait_for_approval(state: &AppState, call_id: &str) -> bool {
    let (tx, rx) = oneshot::channel::<bool>();
    state
        .agent
        .approvals
        .lock()
        .insert(call_id.to_string(), tx);
    match tokio::time::timeout(Duration::from_secs(900), rx).await {
        Ok(Ok(v)) => v,
        Ok(Err(_)) => false,
        Err(_) => {
            state.agent.approvals.lock().remove(call_id);
            false
        }
    }
}

/// 等用户回答提问。与审批共用同一个通道表，key 都是 tool_call_id。
async fn wait_for_answer(state: &AppState, call_id: &str) -> Option<String> {
    let (tx, rx) = oneshot::channel::<String>();
    state
        .agent
        .answers
        .lock()
        .insert(call_id.to_string(), tx);
    match tokio::time::timeout(Duration::from_secs(1800), rx).await {
        Ok(Ok(v)) => Some(v),
        _ => {
            state.agent.answers.lock().remove(call_id);
            None
        }
    }
}

/// 前端提交用户对提问的回答。
pub fn resolve_answer(state: &AppState, call_id: &str, answer: &str) -> bool {
    if let Some(tx) = state.agent.answers.lock().remove(call_id) {
        let _ = tx.send(answer.to_string());
        true
    } else {
        false
    }
}

pub fn resolve_approval(state: &AppState, call_id: &str, approved: bool) -> bool {
    if let Some(tx) = state.agent.approvals.lock().remove(call_id) {
        let _ = tx.send(approved);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 事件必须序列化成前端约定的 camelCase 字段名
    /// （前端读 needsConfirm；字段名漂移会让审批卡片静默失效）。
    #[test]
    fn tool_call_event_uses_camel_case_fields() {
        let ev = AgentEvent::ToolCall {
            id: "call_x".into(),
            name: "script_append_chapter".into(),
            title: "追加章节".into(),
            input: serde_json::Value::Null,
            costly: false,
            mutates: true,
            needs_confirm: true,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("\"type\":\"toolCall\""), "type 标签不对：{s}");
        assert!(s.contains("\"needsConfirm\":true"), "needsConfirm 字段不对：{s}");
        assert!(s.contains("\"mutates\":true"), "mutates 字段不对：{s}");
    }
}

/// 当前用户装的技能目录（只含启用中的）
fn knowledge_index(state: &AppState) -> String {
    crate::skills::prompt_index(state)
}

pub fn resolve_provider(state: &AppState) -> Result<ProviderHandle> {
    let http = state.http_client()?;
    match state.active_provider(ProviderKind::Llm) {
        Some(cfg) => {
            let key = state.api_key_for(&cfg);
            llm::build_provider(&cfg, key, http)
        }
        None => {
            // 没配供应商时退回占位适配器，保证整条链路（流式/工具/缓存统计）可演示
            let cfg = ProviderConfig {
                id: "builtin-mock".into(),
                kind: ProviderKind::Llm,
                name: "占位模型".into(),
                adapter: "mock".into(),
                model: "mock-model".into(),
                enabled: true,
                concurrency: 1,
                timeout_sec: 300,
                ..Default::default()
            };
            llm::build_provider(&cfg, None, http)
        }
    }
}

/// 供 UI 使用：当前会话的缓存统计。
pub fn session_cache_stats(s: &AgentSession) -> UsageReport {
    let total_reported = s.total_usage.input_tokens + s.total_usage.cache_read_tokens;
    UsageReport {
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        total_input_tokens: s.total_usage.input_tokens,
        total_output_tokens: s.total_usage.output_tokens,
        total_cache_read_tokens: s.total_usage.cache_read_tokens,
        total_cache_write_tokens: s.total_usage.cache_write_tokens,
        hit_rate: if total_reported == 0 {
            0.0
        } else {
            s.total_usage.cache_read_tokens as f32 / total_reported as f32
        },
    }
}

pub type SharedSession = Arc<AgentSession>;
