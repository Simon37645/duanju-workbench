//! 知识包：让用户把自己的方法论接进来。
//!
//! 设计上刻意**不把正文塞进系统提示**。原因有两个：
//! 1. 一份风格圣经动辄两三万字，塞进前缀等于每轮对话都为它付一次钱；
//! 2. 前缀里加易变内容会直接打掉缓存命中。
//!
//! 所以做法是：前缀里只放一份**索引**（每个包的名称、类型、一句话摘要），
//! agent 需要正文时用 `knowledge_read` 工具按需去读 —— 读回来的内容落在对话尾部，
//! 不影响前面的缓存。
//!
//! 文件放在应用配置目录的 `knowledge/` 下，一个包一个 `.md`，可选 frontmatter：
//!
//! ```markdown
//! ---
//! name: QCH 视觉创作体系
//! kind: methodology
//! summary: 个人 AIGC 视觉创作体系，含视觉 DNA、Prompt 模块库与自检标准
//! ---
//!
//! 正文……
//! ```
//!
//! `kind` 目前有四类，只影响提示里怎么描述它：
//! - `methodology` 创作方法论 / 工作准则
//! - `style` 风格圣经 / 视觉规范
//! - `checklist` 自检标准（正文里的 `- [ ] xxx` 可以被一键导入检查清单）
//! - `reference` 其它参考资料

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::state::AppState;
use crate::store;

pub const DEFAULT_KIND: &str = "reference";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePack {
    /// 文件名（不含扩展名），同时作为 id
    pub id: String,
    pub name: String,
    pub kind: String,
    pub summary: String,
    pub enabled: bool,
    pub path: String,
    pub chars: usize,
    /// 解析出来的 Markdown 正文（不含 frontmatter），列表接口也会带上，便于预览
    pub body: String,
}

pub fn knowledge_dir(state: &AppState) -> PathBuf {
    state.config.dir().join("knowledge")
}

fn index_file(state: &AppState) -> PathBuf {
    knowledge_dir(state).join("_index.json")
}

/// 开关状态单独存，避免为了记一个布尔值去改写用户自己的文件。
fn load_flags(state: &AppState) -> std::collections::BTreeMap<String, bool> {
    store::read_json_or_default(&index_file(state)).unwrap_or_default()
}

fn save_flags(state: &AppState, flags: &std::collections::BTreeMap<String, bool>) -> Result<()> {
    store::write_json(&index_file(state), flags)
}

/* ------------------------------------------------------------ frontmatter */

struct Parsed {
    name: String,
    kind: String,
    summary: String,
    body: String,
}

fn parse(text: &str, fallback_name: &str) -> Parsed {
    let mut name = fallback_name.to_string();
    let mut kind = DEFAULT_KIND.to_string();
    let mut summary = String::new();
    let mut body = text.to_string();

    let trimmed = text.trim_start_matches('\u{feff}');
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let head = &rest[..end];
            for line in head.lines() {
                let Some((k, v)) = line.split_once(':') else { continue };
                let key = k.trim().to_ascii_lowercase();
                let val = v.trim().trim_matches('"').trim_matches('\'').to_string();
                match key.as_str() {
                    "name" | "title" => name = val,
                    "kind" | "type" => kind = val,
                    "summary" | "description" => summary = val,
                    _ => {}
                }
            }
            body = rest[end + 4..].trim_start().to_string();
        }
    }

    // 没有 frontmatter 时，用第一个一级标题当名字
    if name == fallback_name {
        if let Some(line) = body.lines().find(|l| l.trim_start().starts_with("# ")) {
            name = line.trim_start_matches('#').trim().to_string();
        }
    }
    // 没有摘要就取正文前 60 个字
    if summary.is_empty() {
        let plain: String = body
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .filter(|c| !c.is_whitespace())
            .take(60)
            .collect();
        summary = plain;
    }

    Parsed {
        name,
        kind,
        summary,
        body,
    }
}

/* ---------------------------------------------------------------- 读写 */

pub fn list(state: &AppState) -> Vec<KnowledgePack> {
    let dir = knowledge_dir(state);
    let flags = load_flags(state);
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "md" && ext != "markdown" && ext != "txt" {
            continue;
        }
        let id = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if id.is_empty() || id.starts_with('_') {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let p = parse(&text, &id);
        out.push(KnowledgePack {
            enabled: *flags.get(&id).unwrap_or(&true),
            id,
            name: p.name,
            kind: p.kind,
            summary: p.summary,
            path: path.to_string_lossy().to_string(),
            chars: p.body.chars().count(),
            body: p.body,
        });
    }
    // 方法论排前面，方便阅读
    out.sort_by_key(|p| (kind_rank(&p.kind), p.name.clone()));
    out
}

fn kind_rank(kind: &str) -> u8 {
    match kind {
        "methodology" => 0,
        "style" => 1,
        "checklist" => 2,
        _ => 3,
    }
}

pub fn read(state: &AppState, id: &str) -> Result<String> {
    let path = knowledge_dir(state).join(format!("{id}.md"));
    if !path.is_file() {
        return Err(AppError::NotFound(format!("知识包不存在：{id}")));
    }
    let text = crate::store::read_text_optional(&path)?.unwrap_or_default();
    Ok(parse(&text, id).body)
}

pub fn set_enabled(state: &AppState, id: &str, enabled: bool) -> Result<()> {
    let mut flags = load_flags(state);
    flags.insert(id.to_string(), enabled);
    save_flags(state, &flags)
}

pub fn delete(state: &AppState, id: &str) -> Result<()> {
    let path = knowledge_dir(state).join(format!("{id}.md"));
    store::remove_file_if_exists(&path)?;
    let mut flags = load_flags(state);
    flags.remove(id);
    save_flags(state, &flags)
}

/// 导入若干 `.md` 文件（复制进知识包目录）。
pub fn import(state: &AppState, paths: &[String]) -> Result<Vec<String>> {
    let dir = knowledge_dir(state);
    store::ensure_dir(&dir)?;
    let mut added = vec![];
    for raw in paths {
        let src = Path::new(raw);
        if !src.is_file() {
            continue;
        }
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "md" && ext != "markdown" && ext != "txt" {
            return Err(AppError::invalid(format!(
                "只支持导入 .md 文件：{}",
                src.display()
            )));
        }
        let stem = src
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "未命名".into());
        let mut dest = dir.join(format!("{stem}.md"));
        let mut n = 2;
        while dest.exists() {
            dest = dir.join(format!("{stem}-{n}.md"));
            n += 1;
        }
        std::fs::copy(src, &dest)?;
        added.push(
            dest.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
    }
    Ok(added)
}

/// 首次运行时放一份说明 + 模板，让用户知道该往里放什么。
pub fn ensure_defaults(state: &AppState) -> Result<()> {
    let dir = knowledge_dir(state);
    store::ensure_dir(&dir)?;
    let readme = dir.join("_README.md");
    if !readme.exists() {
        store::write_text(
            &readme,
            &format!(
                r#"# 知识包

把你自己的一套创作方法放进这个目录，Simon 就会按它来工作。

## 怎么用

1. 一个包 = 一个 `.md` 文件，直接丢进本目录（或在「设置 → 知识包」里导入）
2. 文件开头可选写一段 frontmatter：

```
---
name: 我的视觉体系
kind: methodology
summary: 一句话说明它管什么
---

正文……
```

`kind` 可选 `methodology`（创作方法论）、`style`（风格圣经）、`checklist`（自检标准）、`reference`（参考资料）。

## 它是怎么进到对话里的

Simon 的系统提示里只带一份**目录**（名称 + 类型 + 摘要）。
它需要细节时会用 `knowledge_read` 工具把正文读进来 ——
这样几万字的方法论不会每轮都重复计费，也不会打掉提示词缓存。

## 关于 docx

本目录只认 Markdown。手上的 Word 文档请先转成 `.md`：
仓库里带了一个转换脚本 `scripts/docx2md.py`，用法见 README。

目录：{}
"#,
                dir.display()
            ),
        )?;
    }
    Ok(())
}

/// 给系统提示用的索引文本。只包含启用中的包。
pub fn prompt_index(state: &AppState) -> String {
    let packs: Vec<KnowledgePack> = list(state).into_iter().filter(|p| p.enabled).collect();
    if packs.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "# 用户知识包（索引）\n\n用户装了自己的创作方法论。这里只有目录 —— \
         **需要正文时用 `knowledge_read` 工具去读**，不要凭标题猜内容。\n\n",
    );
    for p in &packs {
        out.push_str(&format!(
            "- [{}] {}（{}，约 {} 字）：{}\n",
            p.id,
            p.name,
            kind_label(&p.kind),
            p.chars,
            p.summary
        ));
    }
    out.push_str(
        "\n用户明确要求「按我的体系/风格来」时，先去读相关的包再动手。\n",
    );
    out
}

pub fn kind_label(kind: &str) -> &str {
    match kind {
        "methodology" => "方法论",
        "style" => "风格圣经",
        "checklist" => "自检标准",
        _ => "参考资料",
    }
}
