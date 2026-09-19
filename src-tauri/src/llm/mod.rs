//! LLM 抽象层。
//!
//! 统一内部表示（`ChatRequest` / `Message` / `Block`），下面挂不同适配器：
//! - `openai`    OpenAI 兼容 `/chat/completions`（DeepSeek / Qwen / GLM / Kimi / vLLM / Ollama…），
//!               前缀缓存由服务端自动做，我们只负责让前缀**字节稳定**；
//! - `anthropic` Claude 原生 Messages API，支持显式 `cache_control` 断点，命中率可控；
//! - `mock`      没有密钥时也能把整条链路跑通。
//!
//! 关于缓存的两条铁律，写在这里提醒后来改代码的人：
//! 1. 稳定前缀（system / tools）里不允许出现时间戳、随机 ID、HashMap 迭代顺序等内容；
//! 2. 对话只能追加，不能回头改历史消息，否则服务端前缀缓存整段失效。

pub mod anthropic;
pub mod mock;
pub mod openai;
pub mod sse;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use crate::error::{AppError, Result};
use crate::models::ProviderConfig;

/* ==================================================================== 类型 */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text {
        text: String,
    },
    Image {
        media_type: String,
        /// base64（不带 data: 前缀）
        data: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

impl Block {
    pub fn text(s: impl Into<String>) -> Self {
        Block::Text { text: s.into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Block>,
}

impl Message {
    pub fn user_text(s: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![Block::text(s)],
        }
    }
    pub fn assistant_text(s: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![Block::text(s)],
        }
    }
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                Block::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

/// 系统提示的一个可缓存层。分层的意义：让「变化频率不同的内容」各自成段，
/// 低频段挂了缓存断点后，高频段怎么变都不会带崩前面的缓存。
#[derive(Debug, Clone)]
pub struct SystemBlock {
    pub name: String,
    pub text: String,
    pub cache_breakpoint: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub title: String,
    pub description: String,
    pub input_schema: Value,
    /// 花钱或不可逆的工具，可按设置要求人工确认
    pub costly: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// 命中缓存的输入 token（按缓存价计费的部分）
    pub cache_read_tokens: u64,
    /// 写入缓存的输入 token
    pub cache_write_tokens: u64,
}

impl Usage {
    pub fn merge(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_write_tokens += other.cache_write_tokens;
    }
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub system: Vec<SystemBlock>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Anthropic：在工具列表末尾打断点
    pub cache_tools: bool,
    /// Anthropic：在最后一条消息上打断点，让对话增量也能命中
    pub cache_conversation: bool,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCallStart { id: String, name: String },
    ToolCallArgsDelta { id: String, delta: String },
    Usage(Usage),
    StopReason(String),
    Done,
}

/// 一次流式调用的最终产物。
#[derive(Debug, Clone)]
pub struct Completion {
    pub text: String,
    pub reasoning: String,
    pub tool_calls: Vec<PendingToolCall>,
    pub usage: Usage,
    pub stop_reason: String,
}

#[derive(Debug, Clone)]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/* ================================================================== 适配器 */

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn adapter(&self) -> &'static str;
    fn model(&self) -> String;

    async fn stream(&self, req: ChatRequest, sink: UnboundedSender<StreamEvent>) -> Result<()>;

    /// 非流式语义的便捷封装：内部仍走流式，便于统一处理缓存与用量。
    async fn complete(&self, req: ChatRequest) -> Result<Completion> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let collector = tokio::spawn(async move {
            let mut text = String::new();
            let mut reasoning = String::new();
            let mut usage = Usage::default();
            let mut stop = String::new();
            let mut calls: Vec<PendingToolCall> = vec![];
            let mut args_buf: std::collections::BTreeMap<String, String> =
                std::collections::BTreeMap::new();
            let mut order: Vec<String> = vec![];
            while let Some(ev) = rx.recv().await {
                match ev {
                    StreamEvent::TextDelta(t) => text.push_str(&t),
                    StreamEvent::ReasoningDelta(t) => reasoning.push_str(&t),
                    StreamEvent::ToolCallStart { id, name } => {
                        args_buf.insert(id.clone(), String::new());
                        order.push(id.clone());
                        calls.push(PendingToolCall {
                            id,
                            name,
                            input: Value::Null,
                        });
                    }
                    StreamEvent::ToolCallArgsDelta { id, delta } => {
                        args_buf.entry(id).or_default().push_str(&delta);
                    }
                    StreamEvent::Usage(u) => usage.merge(&u),
                    StreamEvent::StopReason(s) => stop = s,
                    StreamEvent::Done => {}
                }
            }
            for c in calls.iter_mut() {
                let raw = args_buf.remove(&c.id).unwrap_or_default();
                c.input = parse_tool_args(&raw);
            }
            Completion {
                text,
                reasoning,
                tool_calls: calls,
                usage,
                stop_reason: stop,
            }
        });
        self.stream(req, tx).await?;
        collector
            .await
            .map_err(|e| AppError::other(format!("流式收集任务失败: {e}")))
    }
}

/// 模型偶尔会给出空串或半截 JSON，这里统一兜住，不让它把整轮对话打断。
pub fn parse_tool_args(raw: &str) -> Value {
    let t = raw.trim();
    if t.is_empty() {
        return Value::Object(Default::default());
    }
    match serde_json::from_str::<Value>(t) {
        Ok(v) => v,
        Err(_) => {
            // 尝试截到最后一个右花括号
            if let Some(idx) = t.rfind('}') {
                if let Ok(v) = serde_json::from_str::<Value>(&t[..=idx]) {
                    return v;
                }
            }
            serde_json::json!({ "__raw": t, "__parse_error": true })
        }
    }
}

/* ==================================================================== 工厂 */

pub struct ProviderHandle {
    pub config: ProviderConfig,
    pub provider: Arc<dyn LlmProvider>,
}

pub fn build_provider(
    cfg: &ProviderConfig,
    api_key: Option<String>,
    http: reqwest::Client,
) -> Result<ProviderHandle> {
    let provider: Arc<dyn LlmProvider> = match cfg.adapter.as_str() {
        "openai" | "openai-compatible" | "" => {
            Arc::new(openai::OpenAiProvider::new(cfg.clone(), api_key, http)?)
        }
        "anthropic" | "claude" => {
            Arc::new(anthropic::AnthropicProvider::new(cfg.clone(), api_key, http)?)
        }
        "mock" => Arc::new(mock::MockProvider::new(cfg.clone())),
        other => {
            return Err(AppError::Config(format!(
                "未知的 LLM 适配器：{other}（可选 openai / anthropic / mock）"
            )))
        }
    };
    Ok(ProviderHandle {
        config: cfg.clone(),
        provider,
    })
}

/* ==================================================================== 工具 */

/// 429 / 5xx / 连接失败时重试；4xx 直接失败并把服务端原文带出来。
pub async fn send_with_retry(
    _http: &reqwest::Client,
    build: impl Fn() -> reqwest::RequestBuilder,
    max_attempts: u32,
) -> Result<reqwest::Response> {
    let mut last_err: Option<AppError> = None;
    for attempt in 1..=max_attempts.max(1) {
        match build().send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    return Ok(resp);
                }
                if status.as_u16() == 429 || status.is_server_error() {
                    let body = resp.text().await.unwrap_or_default();
                    last_err = Some(AppError::Provider(format!(
                        "HTTP {} {}",
                        status.as_u16(),
                        truncate(&body, 600)
                    )));
                } else {
                    let body = resp.text().await.unwrap_or_default();
                    return Err(AppError::Provider(format!(
                        "HTTP {} {}",
                        status.as_u16(),
                        truncate(&body, 900)
                    )));
                }
            }
            Err(e) => {
                last_err = Some(AppError::Provider(format!("请求失败: {e}")));
            }
        }
        if attempt < max_attempts {
            tokio::time::sleep(Duration::from_millis(400 * attempt as u64)).await;
        }
    }
    Err(last_err.unwrap_or_else(|| AppError::Provider("请求失败".into())))
}

pub fn truncate(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

/// 拼 base_url 与路径，容忍用户填的各种写法。
pub fn join_url(base: &str, suffix: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    if b.is_empty() {
        return suffix.to_string();
    }
    if b.ends_with(suffix) {
        return b.to_string();
    }
    format!("{b}{suffix}")
}

/// Anthropic 要求 user/assistant 严格交替，且同角色连续消息要合并。
pub fn normalize_alternation(messages: &mut Vec<Message>) {
    let mut out: Vec<Message> = Vec::with_capacity(messages.len());
    for m in messages.drain(..) {
        if let Some(last) = out.last_mut() {
            if last.role == m.role {
                last.content.extend(m.content);
                continue;
            }
        }
        out.push(m);
    }
    *messages = out;
}

/// 从本地图片文件读出 base64 与 media type。
pub fn image_block_from_file(path: &std::path::Path, max_bytes: usize) -> Result<Block> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > max_bytes {
        return Err(AppError::invalid(format!(
            "图片过大（{} MB），请先压缩或改用缩略图：{}",
            bytes.len() / 1024 / 1024,
            path.display()
        )));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let media_type = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        _ => "image/png",
    };
    use base64::Engine;
    Ok(Block::Image {
        media_type: media_type.into(),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}
