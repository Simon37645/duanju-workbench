//! 技能（Skills）：用户把自己的一套方法论、流程、规范装进来，Simon 按需取用。
//!
//! 采用业界通用的 **Agent Skills 目录格式**，这样现成的技能包可以直接拷进来：
//!
//! ```text
//! <配置目录>/skills/
//!   qch-methodology.md          ← 单文件技能
//!   wps-excel/                  ← 目录技能
//!     SKILL.md                  ← 必需，带 frontmatter
//!     references/formulas.md    ← 可选，附件
//! ```
//!
//! `SKILL.md` 的 frontmatter：
//!
//! ```markdown
//! ---
//! name: 剧本节奏检查
//! description: 用户要检查/优化短剧剧本节奏时使用，含钩子密度与反转间隔的判定标准
//! ---
//! ```
//!
//! ## 三级渐进披露
//!
//! 这是整套设计的关键，直接决定成本：
//!
//! | 层级 | 内容 | 什么时候进上下文 |
//! | --- | --- | --- |
//! | 1 | **名称 + 描述** | 永远在（系统提示的 L4 层） |
//! | 2 | SKILL.md 正文 | 模型判断需要时，用 `skill_read` 取 |
//! | 3 | 附件（references/ 等） | 真要用到时，用 `skill_read_file` 取 |
//!
//! 所以装五十个技能，前缀里也只有五十行目录；正文永远不会为无关话题付费，
//! 也不会因为某个技能很长而打掉缓存。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::state::AppState;
use crate::store;

pub const SKILL_FILE: &str = "SKILL.md";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    /// 目录名或文件名（不含扩展名），同时作为 id
    pub id: String,
    pub name: String,
    /// 何时使用这个技能 —— 模型靠它决定要不要加载
    pub description: String,
    pub kind: String,
    /// 目录技能的路径；单文件技能为空
    pub dir: Option<String>,
    /// 技能的入口文件
    pub entry: String,
    /// 附件相对路径列表（目录技能才有）
    pub files: Vec<String>,
    pub chars: usize,
    pub enabled: bool,
}

pub fn skills_dir(state: &AppState) -> PathBuf {
    state.config.dir().join("skills")
}

fn flags_file(state: &AppState) -> PathBuf {
    skills_dir(state).join("_flags.json")
}

fn load_flags(state: &AppState) -> std::collections::BTreeMap<String, bool> {
    store::read_json_or_default(&flags_file(state)).unwrap_or_default()
}

fn save_flags(state: &AppState, flags: &std::collections::BTreeMap<String, bool>) -> Result<()> {
    store::write_json(&flags_file(state), flags)
}

/* ---------------------------------------------------------- frontmatter */

#[derive(Default)]
struct Meta {
    name: String,
    description: String,
    kind: String,
}

fn parse_front(text: &str) -> (Meta, String) {
    let mut meta = Meta::default();
    let trimmed = text.trim_start_matches('\u{feff}');
    let mut body = trimmed.to_string();
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                let Some((k, v)) = line.split_once(':') else { continue };
                let val = v.trim().trim_matches('"').trim_matches('\'').to_string();
                match k.trim().to_ascii_lowercase().as_str() {
                    "name" | "title" => meta.name = val,
                    "description" | "summary" | "when" => meta.description = val,
                    "kind" | "type" => meta.kind = val,
                    _ => {}
                }
            }
            body = rest[end + 4..].trim_start().to_string();
        }
    }
    (meta, body)
}

/// 描述为空时，从正文里凑一句 —— 没有描述的技能模型根本不知道该不该用。
fn fallback_description(body: &str) -> String {
    let plain: String = body
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(80)
        .collect();
    if plain.is_empty() {
        "（没有写描述，建议补上 frontmatter 的 description）".into()
    } else {
        plain
    }
}

/* ---------------------------------------------------------------- 扫描 */

fn collect_files(dir: &Path, base: &Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name.starts_with('_') {
            continue;
        }
        if p.is_dir() {
            collect_files(&p, base, out);
        } else if let Ok(rel) = p.strip_prefix(base) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

pub fn list(state: &AppState) -> Vec<Skill> {
    let dir = skills_dir(state);
    let flags = load_flags(state);
    let mut out: Vec<Skill> = vec![];
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return out;
    };

    for entry in rd.flatten() {
        let path = entry.path();
        let id = entry.file_name().to_string_lossy().to_string();
        if id.starts_with('.') || id.starts_with('_') {
            continue;
        }

        if path.is_dir() {
            let entry_file = path.join(SKILL_FILE);
            if !entry_file.is_file() {
                continue; // 不是技能目录
            }
            let Ok(text) = std::fs::read_to_string(&entry_file) else { continue };
            let (meta, body) = parse_front(&text);
            let mut files = vec![];
            collect_files(&path, &path, &mut files);
            files.retain(|f| f != SKILL_FILE);
            files.sort();
            out.push(Skill {
                name: if meta.name.is_empty() { id.clone() } else { meta.name },
                description: if meta.description.is_empty() {
                    fallback_description(&body)
                } else {
                    meta.description
                },
                kind: if meta.kind.is_empty() { "skill".into() } else { meta.kind },
                enabled: *flags.get(&id).unwrap_or(&true),
                dir: Some(path.to_string_lossy().to_string()),
                entry: entry_file.to_string_lossy().to_string(),
                files,
                chars: body.chars().count(),
                id,
            });
        } else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext != "md" && ext != "markdown" && ext != "txt" {
                continue;
            }
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if stem.is_empty() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let (meta, body) = parse_front(&text);
            out.push(Skill {
                name: if meta.name.is_empty() { stem.clone() } else { meta.name },
                description: if meta.description.is_empty() {
                    fallback_description(&body)
                } else {
                    meta.description
                },
                kind: if meta.kind.is_empty() { "skill".into() } else { meta.kind },
                enabled: *flags.get(&stem).unwrap_or(&true),
                dir: None,
                entry: path.to_string_lossy().to_string(),
                files: vec![],
                chars: body.chars().count(),
                id: stem,
            });
        }
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn get(state: &AppState, id: &str) -> Result<Skill> {
    list(state)
        .into_iter()
        .find(|s| s.id == id || s.name == id)
        .ok_or_else(|| AppError::NotFound(format!("技能不存在：{id}")))
}

/// 读技能正文（三级披露里的第 2 级）。
pub fn read_body(state: &AppState, id: &str) -> Result<String> {
    let skill = get(state, id)?;
    let text = crate::store::read_text_optional(Path::new(&skill.entry))?.unwrap_or_default();
    Ok(parse_front(&text).1)
}

/// 读附件（第 3 级）。会挡住路径穿越。
pub fn read_file(state: &AppState, id: &str, rel: &str) -> Result<String> {
    let skill = get(state, id)?;
    let dir = skill
        .dir
        .ok_or_else(|| AppError::invalid("这是单文件技能，没有附件"))?;
    let base = PathBuf::from(&dir)
        .canonicalize()
        .map_err(|e| AppError::other(format!("技能目录不可读：{e}")))?;
    let target = base.join(rel.trim_start_matches('/'));
    let real = target
        .canonicalize()
        .map_err(|_| AppError::NotFound(format!("附件不存在：{rel}")))?;
    if !real.starts_with(&base) {
        return Err(AppError::invalid("附件路径越界"));
    }
    let text = store::read_text_optional(&real)?
        .ok_or_else(|| AppError::NotFound(format!("附件不存在：{rel}")))?;
    Ok(text)
}

/* ---------------------------------------------------------------- 管理 */

pub fn set_enabled(state: &AppState, id: &str, enabled: bool) -> Result<()> {
    let mut flags = load_flags(state);
    flags.insert(id.to_string(), enabled);
    save_flags(state, &flags)
}

pub fn delete(state: &AppState, id: &str) -> Result<()> {
    let dir = skills_dir(state);
    let folder = dir.join(id);
    if folder.is_dir() {
        std::fs::remove_dir_all(&folder)?;
    } else {
        for ext in ["md", "markdown", "txt"] {
            let f = dir.join(format!("{id}.{ext}"));
            if f.is_file() {
                store::remove_file_if_exists(&f)?;
            }
        }
    }
    let mut flags = load_flags(state);
    flags.remove(id);
    save_flags(state, &flags)
}

/// 导入技能。支持三种入参：
/// - 带 `SKILL.md` 的目录 → 整个目录拷进来
/// - 单个 `.md` → 作为单文件技能
/// - 其它目录 → 当作「技能合集」批量扫描其子目录
pub fn import(state: &AppState, paths: &[String]) -> Result<Vec<String>> {
    let dir = skills_dir(state);
    store::ensure_dir(&dir)?;
    let mut added = vec![];
    for raw in paths {
        let src = Path::new(raw);
        if src.is_dir() {
            if src.join(SKILL_FILE).is_file() {
                added.push(copy_skill_dir(src, &dir)?);
            } else {
                // 合集目录：扫一层子目录
                let Ok(rd) = std::fs::read_dir(src) else { continue };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() && p.join(SKILL_FILE).is_file() {
                        if let Ok(id) = copy_skill_dir(&p, &dir) {
                            added.push(id);
                        }
                    }
                }
            }
        } else if src.is_file() {
            let stem = src
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "未命名".into());
            let dest = unique_path(&dir, &stem, "md");
            std::fs::copy(src, &dest)?;
            added.push(
                dest.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
            );
        }
    }
    if added.is_empty() {
        return Err(AppError::invalid(
            "没有导入任何技能。支持：带 SKILL.md 的目录、单个 .md、或装着多个技能的合集目录",
        ));
    }
    Ok(added)
}

fn copy_skill_dir(src: &Path, dest_root: &Path) -> Result<String> {
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "skill".into());
    let dest = unique_path(dest_root, &name, "dir");
    copy_dir_all(src, &dest)?;
    Ok(name)
}

fn unique_path(root: &Path, stem: &str, kind: &str) -> PathBuf {
    let first = if kind == "dir" {
        root.join(stem)
    } else {
        root.join(format!("{stem}.md"))
    };
    if !first.exists() {
        return first;
    }
    let mut n = 2;
    loop {
        let p = if kind == "dir" {
            root.join(format!("{stem}-{n}"))
        } else {
            root.join(format!("{stem}-{n}.md"))
        };
        if !p.exists() {
            return p;
        }
        n += 1;
    }
}

fn copy_dir_all(src: &Path, dest: &Path) -> Result<()> {
    store::ensure_dir(dest)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let from = e.path();
        let name = e.file_name();
        let name_str = name.to_string_lossy();
        if name_str == ".git" || name_str.starts_with('.') {
            continue;
        }
        let to = dest.join(&name);
        if from.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// 把旧版的「知识包」目录迁移过来（一次性）。
fn migrate_legacy(state: &AppState) -> Result<()> {
    let old = state.config.dir().join("knowledge");
    if !old.is_dir() {
        return Ok(());
    }
    let dest = skills_dir(state);
    store::ensure_dir(&dest)?;
    if let Ok(rd) = std::fs::read_dir(&old) {
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('_') || !p.is_file() {
                continue;
            }
            let to = unique_path(&dest, p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(name).as_str(), "md");
            let _ = std::fs::copy(&p, &to);
        }
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// 首次运行放一份说明。
pub fn ensure_defaults(state: &AppState) -> Result<()> {
    let _ = migrate_legacy(state);
    let dir = skills_dir(state);
    store::ensure_dir(&dir)?;
    let readme = dir.join("_README.md");
    if !readme.exists() {
        store::write_text(
            &readme,
            &format!(
                r#"# 技能目录

把你自己的一套流程、规范、方法论放进来，Simon 会在需要的时候自己取用。

## 两种放法

**目录技能**（推荐，可以带附件）：

```
我的技能/
  SKILL.md          ← 必需
  references/       ← 可选，附件
    细则.md
```

`SKILL.md` 开头写 frontmatter：

```
---
name: 剧本节奏检查
description: 用户要检查或优化短剧剧本节奏时使用，含钩子密度与反转间隔的判定标准
---
```

**单文件技能**：直接丢一个 `.md` 进来也可以，frontmatter 同上。

## description 最重要

它是模型判断「要不要用这个技能」的唯一依据。写清**什么时候用**，
而不是写这个技能是什么 —— 前者才有用。

## 它是怎么进到对话里的

三级渐进披露，决定成本：

1. **名称 + 描述** —— 永远在系统提示里（很短，且可缓存）
2. **SKILL.md 正文** —— 模型觉得需要时，用 `skill_read` 取
3. **附件** —— 真要用到时，用 `skill_read_file` 取

所以装五十个技能，前缀里也只有五十行目录；正文永远不会为无关话题付费。

## 批量导入

「设置 → 技能 → 导入」可以一次选多个。也可以直接选一个**合集目录**
（里面装着多个技能子目录），会一次性全部扫描导入。

目录：{}
"#,
                dir.display()
            ),
        )?;
    }
    Ok(())
}

/// 系统提示里的技能目录（只含启用中的）。
pub fn prompt_index(state: &AppState) -> String {
    let skills: Vec<Skill> = list(state).into_iter().filter(|s| s.enabled).collect();
    if skills.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "# 技能库\n\n\
         用户装了一批技能。这里只有**名称与用途**，正文不在上下文里。\n\
         察觉到某个技能正好对得上当前任务时，先用 `skill_read` 读它的正文再动手；\n\
         技能带附件时，正文里通常会写明该去看哪个附件，用 `skill_read_file` 取。\n\
         不要把技能目录当任务清单挨个读一遍。\n\n",
    );
    for s in &skills {
        let extra = if s.files.is_empty() {
            String::new()
        } else {
            format!("［{} 个附件］", s.files.len())
        };
        out.push_str(&format!("- {}：{}{}\n", s.name, s.description, extra));
    }
    out
}
