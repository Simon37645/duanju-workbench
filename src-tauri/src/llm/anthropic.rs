//! Anthropic Claude 原生 Messages API 适配器（SSE 流式）。
//!
//! 缓存策略（这是本工作台缓存命中率最高的一路）：
//! Anthropic 的缓存前缀顺序是 `tools → system → messages`，缓存断点标记的是
//! 「前缀的结束位置」。本工作台在最多 4 个断点里这样分配：
//!   1. 工具列表末尾        —— 缓存所有工具定义
//!   2. 系统提示最后一个可缓存层 —— 缓存 tools + 全部稳定 system 层
//!   3. 最后一条消息         —— 让对话增量部分也能复用
//! 于是同一面板里连续对话时，绝大多数输入 token 都按缓存价计费。

use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

use super::{
    join_url, send_with_retry, Block, ChatRequest, LlmProvider, Message, Role, StreamEvent, Usage,
};
use crate::error::{AppError, Result};
use crate::models::ProviderConfig;

const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    cfg: ProviderConfig,
    api_key: Option<String>,
    http: reqwest::Client,
    url: String,
    version: String,
}

impl AnthropicProvider {
    pub fn new(cfg: ProviderConfig, api_key: Option<String>, http: reqwest::Client) -> Result<Self> {
        let base = if cfg.base_url.trim().is_empty() {
            "https://api.anthropic.com".to_string()
        } else {
            cfg.base_url.clone()
        };
        let url = join_url(&base, "/v1/messages");
        let version = cfg
            .options
            .get("anthropicVersion")
            .and_then(|v| v.as_str())
            .unwrap_or(ANTHROPIC_VERSION)
            .to_string();
        Ok(Self {
            cfg,
            api_key,
            http,
            url,
            version,
        })
    }

    fn body(&self, req: &ChatRequest) -> Value {
        let mut system: Vec<Value> = vec![];
        let last_cacheable = req
            .system
            .iter()
            .rposition(|b| b.cache_breakpoint)
            .unwrap_or(usize::MAX);
        for (i, b) in req.system.iter().enumerate() {
            let mut o = Map::new();
            o.insert("type".into(), json!("text"));
            o.insert("text".into(), json!(b.text));
            if i == last_cacheable {
                o.insert("cache_control".into(), json!({"type": "ephemeral"}));
            }
            system.push(Value::Object(o));
        }

        let mut messages: Vec<Value> = req.messages.iter().map(encode_message).collect();
        if req.cache_conversation {
            if let Some(last) = messages.last_mut() {
                if let Some(blocks) = last.get_mut("content").and_then(|c| c.as_array_mut()) {
                    if let Some(b) = blocks.last_mut() {
                        if let Some(o) = b.as_object_mut() {
                            o.insert("cache_control".into(), json!({"type": "ephemeral"}));
                        }
                    }
                }
            }
        }

        let tools: Vec<Value> = req
            .tools
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut o = Map::new();
                o.insert("name".into(), json!(t.name));
                o.insert("description".into(), json!(t.description));
                o.insert("input_schema".into(), t.input_schema.clone());
                if req.cache_tools && i + 1 == req.tools.len() {
                    o.insert("cache_control".into(), json!({"type": "ephemeral"}));
                }
                Value::Object(o)
            })
            .collect();

        let mut body = Map::new();
        body.insert("model".into(), json!(req.model));
        body.insert("system".into(), json!(system));
        body.insert("messages".into(), json!(messages));
        body.insert("max_tokens".into(), json!(req.max_tokens.max(1)));
        body.insert("stream".into(), json!(true));
        if req.temperature >= 0.0 {
            body.insert("temperature".into(), json!(req.temperature));
        }
        if !tools.is_empty() {
            body.insert("tools".into(), json!(tools));
        }
        Value::Object(body)
    }

    fn request(&self, body: &Value) -> reqwest::RequestBuilder {
        let mut rb = self
            .http
            .post(&self.url)
            .header("anthropic-version", &self.version)
            .json(body);
        if let Some(k) = &self.api_key {
            rb = rb.header("x-api-key", k);
        }
        rb
    }
}

fn encode_message(m: &Message) -> Value {
    let role = match m.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    let blocks: Vec<Value> = m
        .content
        .iter()
        .map(|b| match b {
            Block::Text { text } => json!({"type": "text", "text": text}),
            Block::Image { media_type, data } => json!({
                "type": "image",
                "source": {"type": "base64", "media_type": media_type, "data": data}
            }),
            Block::ToolUse { id, name, input } => json!({
                "type": "tool_use", "id": id, "name": name, "input": input
            }),
            Block::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": [{"type": "text", "text": content}],
                "is_error": is_error,
            }),
        })
        .collect();
    json!({"role": role, "content": blocks})
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn adapter(&self) -> &'static str {
        "anthropic"
    }

    fn model(&self) -> String {
        self.cfg.model.clone()
    }

    async fn stream(&self, req: ChatRequest, sink: UnboundedSender<StreamEvent>) -> Result<()> {
        let mut req = req;
        // Anthropic 严格要求 user/assistant 交替
        super::normalize_alternation(&mut req.messages);
        let body = self.body(&req);

        let resp = send_with_retry(&self.http, || self.request(&body), 3).await?;
        let mut decoder = super::sse::SseDecoder::new();
        let mut stream = resp.bytes_stream();

        // content_block index -> tool_call id
        let mut block_tool: HashMap<u64, String> = HashMap::new();
        let mut pending_output: u64 = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(AppError::from)?;
            let text = String::from_utf8_lossy(&chunk);
            for ev in decoder.push(&text) {
                let data = ev.data.trim();
                if data.is_empty() {
                    continue;
                }
                let v: Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let etype = ev
                    .event
                    .clone()
                    .or_else(|| v.get("type").and_then(|t| t.as_str()).map(String::from))
                    .unwrap_or_default();

                match etype.as_str() {
                    "message_start" => {
                        if let Some(u) = v.pointer("/message/usage") {
                            let _ = sink.send(StreamEvent::Usage(parse_usage(u)));
                        }
                    }
                    "content_block_start" => {
                        let idx = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                        let cb = v.get("content_block").cloned().unwrap_or(Value::Null);
                        match cb.get("type").and_then(|t| t.as_str()) {
                            Some("tool_use") => {
                                let id = cb
                                    .get("id")
                                    .and_then(|i| i.as_str())
                                    .unwrap_or("tool_0")
                                    .to_string();
                                let name = cb
                                    .get("name")
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                block_tool.insert(idx, id.clone());
                                let _ = sink
                                    .send(StreamEvent::ToolCallStart { id, name });
                            }
                            Some("text") => {
                                if let Some(t) = cb.get("text").and_then(|t| t.as_str()) {
                                    if !t.is_empty() {
                                        let _ =
                                            sink.send(StreamEvent::TextDelta(t.to_string()));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    "content_block_delta" => {
                        let idx = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                        let d = v.get("delta").cloned().unwrap_or(Value::Null);
                        match d.get("type").and_then(|t| t.as_str()) {
                            Some("text_delta") => {
                                if let Some(t) = d.get("text").and_then(|t| t.as_str()) {
                                    let _ = sink.send(StreamEvent::TextDelta(t.to_string()));
                                }
                            }
                            Some("thinking_delta") => {
                                if let Some(t) = d.get("thinking").and_then(|t| t.as_str()) {
                                    let _ =
                                        sink.send(StreamEvent::ReasoningDelta(t.to_string()));
                                }
                            }
                            Some("input_json_delta") => {
                                if let (Some(id), Some(pj)) = (
                                    block_tool.get(&idx),
                                    d.get("partial_json").and_then(|p| p.as_str()),
                                ) {
                                    let _ = sink.send(StreamEvent::ToolCallArgsDelta {
                                        id: id.clone(),
                                        delta: pj.to_string(),
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                    "message_delta" => {
                        if let Some(u) = v.get("usage") {
                            let out = u
                                .get("output_tokens")
                                .and_then(|x| x.as_u64())
                                .unwrap_or(0);
                            if out > 0 {
                                let delta = out.saturating_sub(pending_output);
                                pending_output = out;
                                let _ = sink.send(StreamEvent::Usage(Usage {
                                    input_tokens: 0,
                                    output_tokens: delta,
                                    cache_read_tokens: 0,
                                    cache_write_tokens: 0,
                                }));
                            }
                        }
                        if let Some(sr) = v.pointer("/delta/stop_reason").and_then(|s| s.as_str()) {
                            let mapped = match sr {
                                "tool_use" => "toolUse",
                                "end_turn" => "endTurn",
                                "max_tokens" => "maxTokens",
                                "stop_sequence" => "stopSequence",
                                "refusal" => "refusal",
                                other => other,
                            };
                            let _ = sink.send(StreamEvent::StopReason(mapped.to_string()));
                        }
                    }
                    "error" => {
                        let m = v
                            .pointer("/error/message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Anthropic 返回错误");
                        return Err(AppError::Provider(m.to_string()));
                    }
                    _ => {}
                }
            }
        }
        let _ = sink.send(StreamEvent::Done);
        Ok(())
    }
}

fn parse_usage(u: &Value) -> Usage {
    let get = |k: &str| u.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
    Usage {
        input_tokens: get("input_tokens"),
        output_tokens: get("output_tokens"),
        cache_read_tokens: get("cache_read_input_tokens"),
        cache_write_tokens: get("cache_creation_input_tokens"),
    }
}
