//! OpenAI 兼容适配器（`POST {base}/chat/completions`，SSE 流式）。
//!
//! 前缀缓存说明：这一路端点（DeepSeek / Qwen / GLM / Kimi / OpenRouter / vLLM…）
//! 基本都由服务端自动做前缀缓存，客户端**没有**显式断点可打。
//! 所以这里的职责是保证发出去的前缀逐字节稳定：消息顺序不变、system 内容不变、
//! 工具定义顺序与字段顺序不变。

use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Map, Value};
use tokio::sync::mpsc::UnboundedSender;

use super::{
    join_url, send_with_retry, Block, ChatRequest, LlmProvider, Message, Role, StreamEvent, Usage,
};
use crate::error::{AppError, Result};
use crate::models::ProviderConfig;

pub struct OpenAiProvider {
    cfg: ProviderConfig,
    api_key: Option<String>,
    http: reqwest::Client,
    url: String,
    /// 有些兼容端点不认 stream_options，命中 400 后自动降级
    send_stream_options: bool,
    extra_headers: Vec<(String, String)>,
}

impl OpenAiProvider {
    pub fn new(cfg: ProviderConfig, api_key: Option<String>, http: reqwest::Client) -> Result<Self> {
        let base = if cfg.base_url.trim().is_empty() {
            "https://api.openai.com/v1".to_string()
        } else {
            cfg.base_url.clone()
        };
        let url = join_url(&base, "/chat/completions");
        let send_stream_options = cfg
            .options
            .get("streamOptions")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let mut extra_headers = vec![];
        if let Some(obj) = cfg.options.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    extra_headers.push((k.clone(), s.to_string()));
                }
            }
        }
        Ok(Self {
            cfg,
            api_key,
            http,
            url,
            send_stream_options,
            extra_headers,
        })
    }

    fn body(&self, req: &ChatRequest, stream_options: bool) -> Value {
        let mut messages: Vec<Value> = vec![];

        // system 层拼成一条，内容顺序即缓存前缀顺序
        let sys: String = req
            .system
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join("\n\n");
        if !sys.is_empty() {
            messages.push(json!({"role": "system", "content": sys}));
        }

        for m in &req.messages {
            messages.push(encode_message(m));
        }

        let mut body = Map::new();
        body.insert("model".into(), json!(req.model));
        body.insert("messages".into(), json!(messages));
        body.insert("stream".into(), json!(true));
        if stream_options {
            // 让最后一个 chunk 带上 usage，否则拿不到缓存命中数
            body.insert("stream_options".into(), json!({"include_usage": true}));
        }
        if req.max_tokens > 0 {
            body.insert("max_tokens".into(), json!(req.max_tokens));
        }
        if req.temperature >= 0.0 {
            body.insert("temperature".into(), json!(req.temperature));
        }
        if !req.tools.is_empty() {
            let tools: Vec<Value> = req
                .tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.input_schema,
                        }
                    })
                })
                .collect();
            body.insert("tools".into(), json!(tools));
            body.insert("tool_choice".into(), json!("auto"));
        }
        Value::Object(body)
    }

    fn request(&self, body: &Value) -> reqwest::RequestBuilder {
        let mut rb = self.http.post(&self.url).json(body);
        if let Some(k) = &self.api_key {
            rb = rb.bearer_auth(k);
        }
        for (k, v) in &self.extra_headers {
            rb = rb.header(k, v);
        }
        rb
    }
}

fn encode_message(m: &Message) -> Value {
    let mut parts: Vec<Value> = vec![];
    let mut tool_calls: Vec<Value> = vec![];
    let mut tool_results: Vec<Value> = vec![];

    for b in &m.content {
        match b {
            Block::Text { text } => {
                if !text.is_empty() {
                    parts.push(json!({"type": "text", "text": text}));
                }
            }
            Block::Image { media_type, data } => parts.push(json!({
                "type": "image_url",
                "image_url": {"url": format!("data:{media_type};base64,{data}")}
            })),
            Block::ToolUse { id, name, input } => tool_calls.push(json!({
                "id": id,
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": serde_json::to_string(input).unwrap_or_else(|_| "{}".into())
                }
            })),
            Block::ToolResult {
                tool_use_id,
                content,
                ..
            } => tool_results.push(json!({
                "role": "tool",
                "tool_call_id": tool_use_id,
                "content": content,
            })),
        }
    }

    // 工具结果在 OpenAI 里是独立的消息，需要拆成多条
    if !tool_results.is_empty() {
        if tool_results.len() == 1 {
            return tool_results.pop().unwrap();
        }
        return json!({ "role": "tool", "tool_call_id": "", "content": "", "__multi": tool_results });
    }

    let content = if parts.is_empty() {
        Value::Null
    } else if parts.len() == 1 && parts[0].get("type").and_then(|t| t.as_str()) == Some("text") {
        parts[0].get("text").cloned().unwrap_or(Value::Null)
    } else {
        Value::Array(parts)
    };

    let role = match m.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    let mut obj = Map::new();
    obj.insert("role".into(), json!(role));
    obj.insert("content".into(), content);
    if !tool_calls.is_empty() {
        obj.insert("tool_calls".into(), json!(tool_calls));
    }
    Value::Object(obj)
}

/// 一条消息里可能有多个工具结果，OpenAI 需要展开成多条 tool 消息。
fn expand_messages(messages: &[Message]) -> Vec<Value> {
    let mut out = vec![];
    for m in messages {
        let encoded = encode_message(m);
        if let Some(multi) = encoded.get("__multi").and_then(|v| v.as_array()) {
            out.extend(multi.iter().cloned());
        } else {
            out.push(encoded);
        }
    }
    out
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn adapter(&self) -> &'static str {
        "openai"
    }

    fn model(&self) -> String {
        self.cfg.model.clone()
    }

    async fn stream(&self, req: ChatRequest, sink: UnboundedSender<StreamEvent>) -> Result<()> {
        let use_so = self.send_stream_options;
        let mut body = self.body(&req, use_so);
        body["messages"] = json!(expand_messages(&req.messages));

        let resp = match send_with_retry(&self.http, || self.request(&body), 3).await {
            Ok(r) => r,
            Err(e) => {
                let msg = e.to_string();
                // 有些兼容端点不认 stream_options，收到 400 就降级重试一次
                if use_so && (msg.contains("HTTP 400") || msg.contains("HTTP 422")) {
                    let mut b2 = self.body(&req, false);
                    b2["messages"] = json!(expand_messages(&req.messages));
                    send_with_retry(&self.http, || self.request(&b2), 2).await?
                } else {
                    return Err(e);
                }
            }
        };

        let mut decoder = super::sse::SseDecoder::new();
        let mut stream = resp.bytes_stream();
        // 工具调用的分片按 index 归并（OpenAI 只给 index，不给稳定 id）
        let mut call_ids: Vec<String> = vec![];
        let mut announced: Vec<bool> = vec![];
        let mut finished = false;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(AppError::from)?;
            let text = String::from_utf8_lossy(&chunk);
            for ev in decoder.push(&text) {
                let data = ev.data.trim();
                if data.is_empty() {
                    continue;
                }
                if data == "[DONE]" {
                    finished = true;
                    break;
                }
                let v: Value = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                if let Some(err) = v.get("error") {
                    let m = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("未知错误");
                    return Err(AppError::Provider(m.to_string()));
                }

                if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
                    let _ = sink.send(StreamEvent::Usage(parse_usage(u)));
                }

                let Some(choice) = v.get("choices").and_then(|c| c.get(0)) else {
                    continue;
                };
                let delta = choice.get("delta").cloned().unwrap_or(Value::Null);

                if let Some(t) = delta.get("reasoning_content").and_then(|x| x.as_str()) {
                    if !t.is_empty() {
                        let _ = sink.send(StreamEvent::ReasoningDelta(t.to_string()));
                    }
                }
                if let Some(t) = delta.get("reasoning").and_then(|x| x.as_str()) {
                    if !t.is_empty() {
                        let _ = sink.send(StreamEvent::ReasoningDelta(t.to_string()));
                    }
                }
                if let Some(t) = delta.get("content").and_then(|x| x.as_str()) {
                    if !t.is_empty() {
                        let _ = sink.send(StreamEvent::TextDelta(t.to_string()));
                    }
                }
                if let Some(tcs) = delta.get("tool_calls").and_then(|x| x.as_array()) {
                    for tc in tcs {
                        let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                        while call_ids.len() <= idx {
                            call_ids.push(String::new());
                            announced.push(false);
                        }
                        if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                            if !id.is_empty() {
                                call_ids[idx] = id.to_string();
                            }
                        }
                        if call_ids[idx].is_empty() {
                            call_ids[idx] = format!("call_{idx}");
                        }
                        let name = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        if !announced[idx] && !name.is_empty() {
                            announced[idx] = true;
                            let _ = sink.send(StreamEvent::ToolCallStart {
                                id: call_ids[idx].clone(),
                                name: name.to_string(),
                            });
                        }
                        if let Some(args) = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                        {
                            if !args.is_empty() {
                                let _ = sink.send(StreamEvent::ToolCallArgsDelta {
                                    id: call_ids[idx].clone(),
                                    delta: args.to_string(),
                                });
                            }
                        }
                    }
                }
                if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                    let mapped = match fr {
                        "tool_calls" => "toolUse",
                        "stop" => "endTurn",
                        "length" => "maxTokens",
                        "content_filter" => "refusal",
                        other => other,
                    };
                    let _ = sink.send(StreamEvent::StopReason(mapped.to_string()));
                }
            }
            if finished {
                break;
            }
        }
        let _ = sink.send(StreamEvent::Done);
        Ok(())
    }
}

fn parse_usage(u: &Value) -> Usage {
    let get = |k: &str| u.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
    let cached = u
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|v| v.as_u64())
        .or_else(|| u.get("prompt_cache_hit_tokens").and_then(|v| v.as_u64()))
        .unwrap_or(0);
    let total_prompt = get("prompt_tokens");
    Usage {
        // 各家口径不一：这里统一成「未命中缓存的输入」
        input_tokens: total_prompt.saturating_sub(cached),
        output_tokens: get("completion_tokens"),
        cache_read_tokens: cached,
        cache_write_tokens: 0,
    }
}
