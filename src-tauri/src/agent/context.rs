//! 上下文管理。
//!
//! 模型的上下文窗口是有限资源，这个模块负责三件事：
//!
//! 1. **统计**：随时算得清「这轮对话占了多少」，并区分出工具结果占的部分；
//! 2. **压缩**：两级策略，先轻后重 ——
//!    * 第一刀（安全、零成本）：把早期工具结果的原文换成一个短说明。
//!      工具返回往往是大头（一次 `project_snapshot` 就能上万 token），
//!      而它们只在被调用的那一刻有用，之后就是纯负担。
//!    * 第二刀（花一次模型调用）：把前半段对话压成一段简报，
//!      保留目标、决定、改过什么、未解决的问题，丢掉过程噪音。
//! 3. **@ 引用**：用户在输入框里点名要带上什么（技能 / 章节 / 资产），
//!    相当于把「按需检索」的决定权交给用户。
//!
//! 有一个约束始终成立：**冻结前缀不动**。压缩只作用于对话尾部，
//! 所以那部分缓存（也是最大的一块）不会因为压缩而失效。

use serde::Serialize;

use crate::error::{AppError, Result};
use crate::llm::{Block, ChatRequest, Message, ProviderHandle, Role};
use crate::state::AppState;

use super::{prompt, AgentSession};

/// 压缩时要保留的最近消息条数
const KEEP_RECENT: usize = 8;
/// 自动压缩的触发线（占预算比例）
const TRIGGER_RATIO: f64 = 0.75;
/// 单条消息里 @ 引用最多带进来多少字
const MENTION_BUDGET: usize = 60000;
/// 单张引用图片的体积上限
const MENTION_IMAGE_MAX: u64 = 6 * 1024 * 1024;

/* ================================================================== 统计 */

pub fn est_message_tokens(m: &Message) -> u64 {
    m.content
        .iter()
        .map(|b| match b {
            Block::Text { text } => prompt::est_tokens(text),
            // 图片按固定成本粗估（各家算法不同，这里只用来提示预算）
            Block::Image { .. } => 850,
            Block::ToolUse { name, input, .. } => {
                prompt::est_tokens(name) + prompt::est_tokens(&input.to_string()) + 12
            }
            Block::ToolResult { content, .. } => prompt::est_tokens(content) + 8,
        })
        .sum()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextStats {
    /// 对话消息占用的估算 token
    pub messages_tokens: u64,
    /// 其中工具结果占了多少 —— 压缩的第一刀就砍这里
    pub tool_result_tokens: u64,
    /// 冻结前缀占用（单独算：它基本不变、且可缓存）
    pub prefix_tokens: u64,
    pub budget: u32,
    pub percent: f32,
    pub messages: usize,
    pub turns: u32,
    pub over_threshold: bool,
}

pub fn stats(state: &AppState, session: &AgentSession) -> ContextStats {
    let messages_tokens: u64 = session.messages.iter().map(est_message_tokens).sum();
    let tool_result_tokens: u64 = session
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            Block::ToolResult { content, .. } => Some(prompt::est_tokens(content) + 8),
            _ => None,
        })
        .sum();
    let prefix_tokens = session
        .prefix
        .as_ref()
        .map(|p| {
            p.layers.iter().map(|l| prompt::est_tokens(&l.text)).sum::<u64>()
                + prompt::est_tokens(&p.context)
        })
        .unwrap_or(0);
    let budget = state.settings().context_budget.max(4000);
    ContextStats {
        messages_tokens,
        tool_result_tokens,
        prefix_tokens,
        budget,
        percent: (messages_tokens as f32 / budget as f32 * 100.0).min(100.0),
        messages: session.messages.len(),
        turns: session.turns,
        over_threshold: messages_tokens > (budget as f64 * TRIGGER_RATIO) as u64,
    }
}

/* ================================================================== 压缩 */

/// 第一刀：把老的工具结果换成一个短说明。
/// 只丢「当时看过、现在用不上」的原始返回，对话与结论都还在。
pub fn clear_old_tool_results(session: &mut AgentSession) -> u64 {
    let total = session.messages.len();
    if total <= KEEP_RECENT {
        return 0;
    }
    let cut = total - KEEP_RECENT;
    let mut saved = 0u64;
    for m in session.messages.iter_mut().take(cut) {
        for b in m.content.iter_mut() {
            if let Block::ToolResult { content, .. } = b {
                let chars = content.chars().count();
                if chars > 160 {
                    saved += prompt::est_tokens(content).saturating_sub(40);
                    *content = format!("［早期工具结果已清理，原文 {chars} 字，需要时重新调用工具获取］");
                }
            }
        }
    }
    saved
}

/// 第二刀：让模型把前半段对话压成简报。
async fn summarize_history(provider: &ProviderHandle, messages: &[Message]) -> Result<String> {
    let mut transcript = String::new();
    for m in messages {
        let role = match m.role {
            Role::User => "用户",
            Role::Assistant => "助手",
        };
        let mut line = String::new();
        for b in &m.content {
            match b {
                Block::Text { text } => line.push_str(text),
                Block::ToolUse { name, input, .. } => line.push_str(&format!(
                    "\n[调用工具 {name}，参数 {}]",
                    crate::llm::truncate(&input.to_string(), 300)
                )),
                Block::ToolResult { content, .. } => line.push_str(&format!(
                    "\n[工具返回 {}]",
                    crate::llm::truncate(content, 400)
                )),
                Block::Image { .. } => line.push_str("\n[图片]"),
            }
        }
        transcript.push_str(&format!(
            "{role}：{}\n\n",
            crate::llm::truncate(&line, 3000)
        ));
    }

    let req = ChatRequest {
        model: provider.provider.model(),
        system: vec![crate::llm::SystemBlock {
            name: "compact".into(),
            text: "你在做对话压缩。把下面的工作记录压成一份简报，供后续继续工作时参考。\n\
必须保留：用户的目标与偏好、已经做出的决定、改过哪些数据、未解决的问题、下一步计划。\n\
可以丢弃：寒暄、重复内容、已经作废的中间尝试。\n\
用简洁的中文条目写，不要超过 600 字。直接输出简报，不要任何解释或前后缀。"
                .into(),
            cache_breakpoint: false,
        }],
        messages: vec![Message::user_text(transcript)],
        tools: vec![],
        max_tokens: 1200,
        temperature: 0.2,
        cache_tools: false,
        cache_conversation: false,
    };
    Ok(provider.provider.complete(req).await?.text.trim().to_string())
}

/// 压缩会话。返回压缩后的统计。
pub async fn compact(
    state: &AppState,
    provider: &ProviderHandle,
    session_id: &str,
    force_summarize: bool,
) -> Result<ContextStats> {
    let mut session = state
        .session(session_id)
        .ok_or_else(|| AppError::NotFound("会话不存在".into()))?;

    let saved = clear_old_tool_results(&mut session);
    let after_trim = stats(state, &session);
    let need_summary =
        force_summarize || after_trim.messages_tokens > after_trim.budget as u64;

    if need_summary && session.messages.len() > KEEP_RECENT + 2 {
        let cut = session.messages.len() - KEEP_RECENT;
        let old: Vec<Message> = session.messages[..cut].to_vec();
        match summarize_history(provider, &old).await {
            Ok(summary) if !summary.is_empty() => {
                let mut next: Vec<Message> = vec![Message::user_text(format!(
                    "【前情摘要】以下是本次对话早先发生过的事，供你继续时参考：\n\n{summary}"
                ))];
                next.extend(session.messages[cut..].iter().cloned());
                session.messages = next;
            }
            Ok(_) => {}
            // 摘要失败不影响主流程，工具结果那刀已经生效了
            Err(e) => tracing::warn!("压缩摘要失败，只清理了工具结果：{e}"),
        }
    }

    session.updated_at = crate::models::now_iso();
    state.put_session(session.clone());
    let _ = state.persist_sessions();

    let out = stats(state, &session);
    tracing::info!(
        "上下文压缩完成：工具结果省下约 {saved} tok，当前 {} tok / {}",
        out.messages_tokens,
        out.budget
    );
    Ok(out)
}

/* ================================================================ @ 引用 */

/// 解析消息里的 @ 引用，返回要额外带上的内容块。
///
/// 支持的写法（用户手打，或在输入框里点选插入）：
/// - `@skill:名称`   —— 读取技能正文（也就是让用户手动决定「加载哪个技能」）
/// - `@chapter:序号或标题` —— 带上章节正文
/// - `@asset:资产名` —— 带上资产描述与已出的图（视觉）
pub fn resolve_mentions(state: &AppState, text: &str) -> Vec<Block> {
    let mut out: Vec<Block> = vec![];
    let mut budget = MENTION_BUDGET;

    for raw in text.split_whitespace() {
        let token = raw.trim_matches(|c: char| "，。！？、,.!?()（）[]【】".contains(c));
        let Some(rest) = token.strip_prefix('@') else {
            continue;
        };
        let Some((kind, name)) = rest.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }

        match kind {
            "skill" => {
                let Ok(skill) = crate::skills::get(state, name) else {
                    continue;
                };
                let Ok(body) = crate::skills::read_body(state, &skill.id) else {
                    continue;
                };
                let mut head = format!("技能：{}", skill.name);
                if !skill.files.is_empty() {
                    head.push_str(&format!(
                        "\n（该技能带附件：{}，需要时用 skill_read_file 读）",
                        skill.files.join("、")
                    ));
                }
                push_capped(&mut out, &mut budget, &head, body);
            }
            "chapter" => {
                let Ok(project) = state.current() else {
                    continue;
                };
                let found = project
                    .script
                    .chapters
                    .iter()
                    .find(|c| c.id == name || c.index.to_string() == name || c.title == name)
                    .cloned();
                let Some(meta) = found else { continue };
                let Ok(ch) = project.load_chapter(&meta.id) else {
                    continue;
                };
                push_capped(
                    &mut out,
                    &mut budget,
                    &format!("第 {} 章 {}", meta.index, meta.title),
                    ch.content,
                );
            }
            "asset" => {
                let Ok(project) = state.current() else {
                    continue;
                };
                let Some(asset) = project.asset_by_name(name).cloned() else {
                    continue;
                };
                let file = asset
                    .views
                    .iter()
                    .find(|v| v.file.is_some())
                    .and_then(|v| v.file.clone());
                push_capped(
                    &mut out,
                    &mut budget,
                    &format!("资产：{}", asset.name),
                    format!(
                        "{}\n固定特征：{}",
                        asset.description,
                        asset.locked_traits.join("、")
                    ),
                );
                if let Some(f) = file {
                    if let Ok(b) =
                        crate::llm::image_block_from_file(std::path::Path::new(&f), MENTION_IMAGE_MAX as usize)
                    {
                        out.push(b);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn push_capped(out: &mut Vec<Block>, budget: &mut usize, label: &str, body: String) {
    if *budget == 0 {
        return;
    }
    let body: String = body.chars().take(*budget).collect();
    if body.trim().is_empty() {
        return;
    }
    *budget = budget.saturating_sub(body.chars().count());
    out.push(Block::text(format!("【{label}】\n{body}")));
}
