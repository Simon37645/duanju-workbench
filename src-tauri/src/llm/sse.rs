//! 极简 SSE 解析。不引第三方依赖，OpenAI 与 Anthropic 的流都能吃。

#[derive(Debug, Clone, Default)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub struct SseDecoder {
    buf: String,
    event_name: Option<String>,
    data_lines: Vec<String>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂入一段原始文本，吐出其中完整的 SSE 事件。
    pub fn push(&mut self, chunk: &str) -> Vec<SseEvent> {
        let mut out = vec![];
        self.buf.push_str(chunk);
        loop {
            let Some(pos) = self.buf.find('\n') else { break };
            let mut line = self.buf[..pos].to_string();
            self.buf.drain(..=pos);
            if line.ends_with('\r') {
                line.pop();
            }
            if line.is_empty() {
                if let Some(ev) = self.flush_event() {
                    out.push(ev);
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("event:") {
                self.event_name = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("data:") {
                self.data_lines.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
            }
            // id: / retry: / 注释行直接忽略
        }
        out
    }

    fn flush_event(&mut self) -> Option<SseEvent> {
        if self.data_lines.is_empty() && self.event_name.is_none() {
            return None;
        }
        let ev = SseEvent {
            event: self.event_name.take(),
            data: self.data_lines.join("\n"),
        };
        self.data_lines.clear();
        Some(ev)
    }
}
