//! 占位适配器：没有配置任何密钥时，让整条链路（流式、工具调用、缓存统计 UI）也能跑通。
//!
//! 行为：识别用户输入里的关键词，返回一段带工具调用的演示输出。
//! 它同时是写新适配器时最好的对照样例。

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;

use super::{ChatRequest, LlmProvider, StreamEvent, Usage};
use crate::error::Result;
use crate::models::ProviderConfig;

pub struct MockProvider {
    cfg: ProviderConfig,
}

impl MockProvider {
    pub fn new(cfg: ProviderConfig) -> Self {
        Self { cfg }
    }
}

#[async_trait]
impl LlmProvider for MockProvider {
    fn adapter(&self) -> &'static str {
        "mock"
    }

    fn model(&self) -> String {
        if self.cfg.model.is_empty() {
            "mock-model".into()
        } else {
            self.cfg.model.clone()
        }
    }

    async fn stream(&self, req: ChatRequest, sink: UnboundedSender<StreamEvent>) -> Result<()> {
        let last_user = req
            .messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, super::Role::User))
            .map(|m| m.text())
            .unwrap_or_default();

        let tool_names: Vec<&str> = req.tools.iter().map(|t| t.name.as_str()).collect();
        let prefix_tokens = req
            .system
            .iter()
            .map(|b| b.text.chars().count() as u64 / 3)
            .sum::<u64>();

        // 已经执行过工具就正常回话，否则先演示一次工具调用 —— 这样既能让 UI 的
        // 工具卡片有东西展示，又不会在循环里无限调下去。
        let has_tool_result = req.messages.iter().any(|m| {
            m.content
                .iter()
                .any(|b| matches!(b, super::Block::ToolResult { .. }))
        });
        let wants_tool =
            !has_tool_result && tool_names.contains(&"checklist_report") && !last_user.contains("不要调工具");

        let text = format!(
            "（占位模型）已收到你的输入：{}\n\n当前面板可用工具 {} 个：{}。\n\
             配置真实供应商后这里会换成真实输出。\n",
            if last_user.is_empty() { "（空）" } else { &last_user },
            tool_names.len(),
            if tool_names.is_empty() {
                "无".to_string()
            } else {
                tool_names.join("、")
            }
        );

        if wants_tool {
            let id = "mock_tool_1".to_string();
            let _ = sink.send(StreamEvent::ToolCallStart {
                id: id.clone(),
                name: "checklist_report".into(),
            });
            tokio::time::sleep(std::time::Duration::from_millis(60)).await;
            let _ = sink.send(StreamEvent::ToolCallArgsDelta {
                id,
                delta: json!({}).to_string(),
            });
            let _ = sink.send(StreamEvent::StopReason("toolUse".into()));
            let _ = sink.send(StreamEvent::Usage(Usage {
                input_tokens: prefix_tokens,
                output_tokens: 30,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            }));
            let _ = sink.send(StreamEvent::Done);
            return Ok(());
        }

        let _ = sink.send(StreamEvent::ReasoningDelta(
            "（占位推理）先看前缀里有什么，再决定要不要调工具。".into(),
        ));
        for chunk in text.as_bytes().chunks(24) {
            let s = String::from_utf8_lossy(chunk).to_string();
            let _ = sink.send(StreamEvent::TextDelta(s));
            tokio::time::sleep(std::time::Duration::from_millis(18)).await;
        }
        let _ = sink.send(StreamEvent::Usage(Usage {
            input_tokens: prefix_tokens,
            output_tokens: text.chars().count() as u64 / 2,
            // 演示缓存计量：第二次起假装命中大部分前缀
            cache_read_tokens: if req.messages.len() > 2 { prefix_tokens } else { 0 },
            cache_write_tokens: if req.messages.len() <= 2 { prefix_tokens } else { 0 },
        }));
        let _ = sink.send(StreamEvent::StopReason("endTurn".into()));
        let _ = sink.send(StreamEvent::Done);
        Ok(())
    }
}
