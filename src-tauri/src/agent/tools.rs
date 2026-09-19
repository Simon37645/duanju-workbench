//! Agent 工具注册表。前端命令与 agent 工具共用 `actions` 层，行为一致。
//!
//! 顺序约定：`registry()` 返回的顺序是固定的（按面板 + 面板内声明顺序），
//! 因为工具定义的字节稳定性直接影响前缀缓存命中率，**不要**改成 HashMap 遍历。

pub mod schema;

use futures_util::future::BoxFuture;
use serde_json::{json, Value};

use crate::error::{AppError, Result};
use crate::llm::ToolSpec;
use crate::models::*;
use crate::progress;
use crate::project::Project;
use crate::state::AppState;

pub type ToolFuture = BoxFuture<'static, Result<ToolOutcome>>;

/// 只读工具：不会改项目数据。其余工具一律视为会写数据，
/// 「变更前确认」模式靠这个集合判断要不要弹确认。
pub const READ_ONLY_TOOLS: &[&str] = &[
    "project_snapshot",
    "checklist_report",
    "script_read_chapter",
    "style_list_presets",
    "storyboard_list_shots",
    "asset_list",
    "prompt_list",
    "video_list_takes",
    "edit_get_timeline",
    "subtitle_list",
    "asset_view_image",
    "file_view_image",
    "ask_user",
    "knowledge_list",
    "knowledge_read",
];

pub fn is_read_only(name: &str) -> bool {
    READ_ONLY_TOOLS.contains(&name)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutcome {
    pub summary: String,
    pub data: Value,
    /// 工具带回来的图片。跑完这轮会作为附加消息塞回模型，agent 就能"看见"图。
    #[serde(skip)]
    pub images: Vec<ToolImage>,
}

/// 回灌给模型的图片附件。
#[derive(Debug, Clone)]
pub struct ToolImage {
    pub label: String,
    pub media_type: String,
    pub data_b64: String,
}

impl ToolOutcome {
    pub fn new(summary: impl Into<String>, data: Value) -> Self {
        Self {
            summary: summary.into(),
            data,
            images: vec![],
        }
    }
    pub fn text(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            data: Value::Null,
            images: vec![],
        }
    }
    pub fn with_images(mut self, images: Vec<ToolImage>) -> Self {
        self.images = images;
        self
    }
}

/// 从本地文件读一张图作为附件（带体积上限，避免把上下文撑爆）。
fn tool_image_from_file(path: &std::path::Path, label: &str, max_bytes: u64) -> Option<ToolImage> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > max_bytes {
        return None;
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
        _ => "image/png",
    };
    use base64::Engine;
    let bytes = std::fs::read(path).ok()?;
    Some(ToolImage {
        label: label.to_string(),
        media_type: media_type.into(),
        data_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

/// 单张图上限 6 MB，一次最多回灌 4 张 —— 三视图看一致性够用了。
const MAX_TOOL_IMAGE_BYTES: u64 = 6 * 1024 * 1024;
const MAX_TOOL_IMAGES: usize = 4;

#[derive(Clone)]
pub struct ToolCtx {
    pub state: AppState,
    pub panel: PanelId,
}

impl ToolCtx {
    pub fn project(&self) -> Result<std::sync::Arc<Project>> {
        self.state.current()
    }
}

pub struct Tool {
    pub spec: ToolSpec,
    pub run: fn(ToolCtx, Value) -> ToolFuture,
}

macro_rules! tool {
    ($name:expr, $title:expr, $desc:expr, $schema:expr, $costly:expr, $f:ident) => {
        Tool {
            spec: ToolSpec {
                name: $name.into(),
                title: $title.into(),
                description: $desc.into(),
                input_schema: $schema,
                costly: $costly,
            },
            run: |ctx, args| Box::pin($f(ctx, args)),
        }
    };
}

/* ============================================================ 参数读取 */

fn arg_str(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::invalid(format!("缺少参数 {key}")))
}

fn arg_opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn arg_f32(args: &Value, key: &str, default: f32) -> f32 {
    args.get(key).and_then(|v| v.as_f64()).map(|x| x as f32).unwrap_or(default)
}

fn arg_u32(args: &Value, key: &str, default: u32) -> u32 {
    args.get(key).and_then(|v| v.as_u64()).map(|x| x as u32).unwrap_or(default)
}

fn arg_bool(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn arg_arr<'a>(args: &'a Value, key: &str) -> Vec<&'a Value> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn arg_str_list(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/* ======================================================== 项目数据视图 */

pub fn project_view(p: &Project, sections: &[String]) -> Value {
    let want = |name: &str| sections.is_empty() || sections.iter().any(|s| s == name);
    let mut out = serde_json::Map::new();

    if want("manifest") {
        out.insert("manifest".into(), json!(p.manifest));
    }
    if want("bible") {
        out.insert("bible".into(), json!(p.bible));
    }
    if want("chapters") {
        out.insert("chapters".into(), json!(p.script.chapters));
    }
    if want("style") {
        out.insert("style".into(), json!(p.style));
    }
    if want("shots") {
        out.insert("shots".into(), json!(p.shots));
    }
    if want("assets") {
        out.insert("assets".into(), json!(p.assets));
    }
    if want("prompts") {
        out.insert("prompts".into(), json!(p.prompts));
    }
    if want("takes") {
        out.insert("takes".into(), json!(p.takes));
    }
    if want("timeline") {
        out.insert("timeline".into(), json!(p.timeline));
    }
    if want("subtitles") {
        let brief: Vec<Value> = p
            .subtitles
            .iter()
            .map(|s| {
                json!({
                    "id": s.id, "name": s.name, "source": s.source,
                    "language": s.language, "model": s.model,
                    "cueCount": s.cues.len(), "file": s.file,
                })
            })
            .collect();
        out.insert("subtitles".into(), json!(brief));
    }
    if want("checklist") {
        out.insert("checklist".into(), json!(p.checklist));
    }
    if want("progress") {
        let snap = snapshot_of(p);
        out.insert("progress".into(), json!(progress::all_progress(&snap)));
    }
    Value::Object(out)
}

pub fn snapshot_of(p: &Project) -> ProjectSnapshot {
    ProjectSnapshot {
        root: p.root_string(),
        manifest: p.manifest.clone(),
        bible: p.bible.clone(),
        chapters: p.script.chapters.clone(),
        style: p.style.clone(),
        shots: p.shots.clone(),
        assets: p.assets.clone(),
        prompts: p.prompts.clone(),
        takes: p.takes.clone(),
        timeline: p.timeline.clone(),
        subtitles: p.subtitles.clone(),
        checklist: p.checklist.clone(),
    }
}

/// 每次工具改动数据后都刷新一遍自动检查项，保证 checklist 面板永远是最新的。
pub fn refresh_auto_items(state: &AppState) -> Result<()> {
    state.mutate_quiet(|p| {
        let snap = snapshot_of(p);
        let autos = progress::auto_items(&snap);
        let manual: Vec<ChecklistItem> = p
            .checklist
            .items
            .iter()
            .filter(|i| !i.auto)
            .cloned()
            .collect();
        let mut items: Vec<ChecklistItem> = autos;
        items.extend(manual);
        p.checklist.items = items;
        Ok(())
    })
}

/* ============================ 通用工具 ============================ */

async fn t_project_snapshot(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let p = ctx.project()?;
    let sections = arg_str_list(&args, "sections");
    let view = project_view(&p, &sections);
    let summary = if sections.is_empty() {
        "已返回项目全景（不含章节正文与完整镜头表，需要时指定 sections）".to_string()
    } else {
        format!("已返回 sections: {}", sections.join(", "))
    };
    Ok(ToolOutcome::new(summary, view))
}

async fn t_checklist_report(ctx: ToolCtx, _args: Value) -> Result<ToolOutcome> {
    let p = ctx.project()?;
    let snap = snapshot_of(&p);
    let pr = progress::all_progress(&snap);
    let pending: Vec<String> = pr
        .iter()
        .flat_map(|x| x.blockers.iter().cloned())
        .collect();
    Ok(ToolOutcome::new(
        format!(
            "{} 个面板中 {} 个已完成；待办问题 {} 条",
            pr.len(),
            pr.iter().filter(|x| x.percent >= 99.9).count(),
            pending.len()
        ),
        json!({ "progress": pr, "items": p.checklist.items }),
    ))
}

async fn t_checklist_add(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let panel = parse_panel(&arg_str(&args, "panel")?)?;
    let text = arg_str(&args, "text")?;
    let item = ChecklistItem {
        id: new_id("ck"),
        panel,
        text: text.clone(),
        done: false,
        done_by: None,
        done_at: None,
        note: arg_opt_str(&args, "note").unwrap_or_default(),
        auto: false,
    };
    ctx.state.mutate(|p| {
        p.checklist.items.push(item.clone());
        Ok(())
    })?;
    Ok(ToolOutcome::text(format!("已新增检查项：{text}")))
}

async fn t_checklist_toggle(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let id = arg_str(&args, "id")?;
    let done = arg_bool(&args, "done", true);
    let note = arg_opt_str(&args, "note").unwrap_or_default();
    let mut found = false;
    ctx.state.mutate(|p| {
        if let Some(i) = p.checklist.items.iter_mut().find(|i| i.id == id) {
            if i.auto {
                return Err(AppError::invalid(
                    "这是自动判定的检查项，由数据决定，不能手动改",
                ));
            }
            i.done = done;
            i.done_by = Some(if ctx.panel == PanelId::Checklist {
                Actor::Agent
            } else {
                Actor::Agent
            });
            i.done_at = Some(now_iso());
            if !note.is_empty() {
                i.note = note.clone();
            }
            found = true;
        }
        Ok(())
    })?;
    if !found {
        return Err(AppError::NotFound(format!("检查项不存在：{id}")));
    }
    Ok(ToolOutcome::text(if done {
        "已勾选"
    } else {
        "已取消勾选"
    }))
}

async fn t_bible_update(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let mut changed: Vec<&str> = vec![];
    ctx.state.mutate(|p| {
        let b = &mut p.bible;
        if let Some(v) = arg_opt_str(&args, "synopsis") {
            b.synopsis = v;
            changed.push("synopsis");
        }
        if args.get("sellingPoints").is_some() {
            b.selling_points = arg_str_list(&args, "sellingPoints");
            changed.push("sellingPoints");
        }
        if let Some(v) = arg_opt_str(&args, "world") {
            b.world = v;
            changed.push("world");
        }
        if let Some(v) = arg_opt_str(&args, "tone") {
            b.tone = v;
            changed.push("tone");
        }
        if let Some(v) = arg_opt_str(&args, "audience") {
            b.audience = v;
            changed.push("audience");
        }
        if let Some(v) = arg_opt_str(&args, "notes") {
            b.notes = v;
            changed.push("notes");
        }
        if let Some(list) = args.get("characters").and_then(|v| v.as_array()) {
            let mut chars: Vec<CharacterCard> = vec![];
            for c in list {
                let name = c
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                let existing = b.characters.iter().find(|x| x.name == name).cloned();
                chars.push(CharacterCard {
                    id: existing
                        .as_ref()
                        .map(|x| x.id.clone())
                        .unwrap_or_else(|| new_id("chr")),
                    name,
                    aliases: c
                        .get("aliases")
                        .and_then(|x| x.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|s| s.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_else(|| {
                            existing.as_ref().map(|x| x.aliases.clone()).unwrap_or_default()
                        }),
                    role: c
                        .get("role")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    age: c
                        .get("age")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    appearance: c
                        .get("appearance")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    personality: c
                        .get("personality")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    arc: c
                        .get("arc")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    notes: String::new(),
                });
            }
            b.characters = chars;
            changed.push("characters");
        }
        Ok(())
    })?;
    Ok(ToolOutcome::text(format!(
        "项目圣经已更新：{}",
        changed.join("、")
    )))
}

/* ============================ 剧本 ============================ */

async fn t_script_set_chapters(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let list = arg_arr(&args, "chapters");
    if list.is_empty() {
        return Err(AppError::invalid("chapters 不能为空"));
    }
    let mut created = vec![];
    ctx.state.mutate(|p| {
        let mut chapters: Vec<ChapterMeta> = vec![];
        for (i, c) in list.iter().enumerate() {
            let title = c
                .get("title")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let summary = c
                .get("summary")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
            // 已存在的同序号章节保留其 id 与正文
            let existing = p.script.chapters.get(i).cloned();
            chapters.push(ChapterMeta {
                id: existing
                    .as_ref()
                    .map(|x| x.id.clone())
                    .unwrap_or_else(|| format!("ch_{:03}", i + 1)),
                index: i as u32 + 1,
                title: if title.is_empty() {
                    format!("第 {} 章", i + 1)
                } else {
                    title
                },
                status: existing
                    .as_ref()
                    .map(|x| x.status)
                    .unwrap_or(ChapterStatus::Empty),
                summary,
                characters: vec![],
                scenes: vec![],
                word_count: existing.as_ref().map(|x| x.word_count).unwrap_or(0),
                updated_at: now_iso(),
            });
        }
        created = chapters
            .iter()
            .map(|c| format!("{} {}", c.id, c.title))
            .collect();
        p.script.chapters = chapters;
        p.reindex_chapters();
        if p.manifest.episode_count_hint == 0 {
            p.manifest.episode_count_hint = p.script.chapters.len() as u32;
        }
        Ok(())
    })?;
    Ok(ToolOutcome::new(
        format!("已设置 {} 个章节", created.len()),
        json!({ "chapters": created }),
    ))
}

async fn t_script_append_chapter(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let title = arg_opt_str(&args, "title").unwrap_or_default();
    let summary = arg_opt_str(&args, "summary").unwrap_or_default();
    let content = arg_opt_str(&args, "content").unwrap_or_default();
    let (id, index) = ctx.state.mutate(|p| {
        let index = p.next_chapter_index();
        let id = format!("ch_{index:03}");
        p.script.chapters.push(ChapterMeta {
            id: id.clone(),
            index,
            title: if title.is_empty() {
                format!("第 {index} 章")
            } else {
                title.clone()
            },
            status: if content.is_empty() {
                ChapterStatus::Empty
            } else {
                ChapterStatus::Draft
            },
            summary: summary.clone(),
            characters: vec![],
            scenes: vec![],
            word_count: 0,
            updated_at: now_iso(),
        });
        if !content.is_empty() {
            p.save_chapter_content(&id, &content)?;
        }
        Ok((id, index))
    })?;
    Ok(ToolOutcome::text(format!("已追加第 {index} 章（{id}）")))
}

async fn t_script_write_chapter(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let target = resolve_chapter(&args)?;
    let content = arg_str(&args, "content")?;
    let summary = arg_opt_str(&args, "summary");
    let status = arg_opt_str(&args, "status");
    let (id, words) = ctx.state.mutate(|p| {
        let id = find_chapter_id(p, &target)?;
        let mut meta = p.save_chapter_content(&id, &content)?;
        if let Some(s) = summary {
            meta.summary = s;
        }
        if content.trim().len() > 200 {
            meta.status = ChapterStatus::Written;
        }
        if let Some(st) = status {
            meta.status = match st.as_str() {
                "empty" => ChapterStatus::Empty,
                "draft" => ChapterStatus::Draft,
                "written" => ChapterStatus::Written,
                "locked" => ChapterStatus::Locked,
                _ => meta.status,
            };
        }
        if let Some(i) = p.script.chapters.iter_mut().find(|c| c.id == id) {
            *i = meta.clone();
        }
        Ok((id, meta.word_count))
    })?;
    Ok(ToolOutcome::text(format!("已写入 {id}，共 {words} 字")))
}

async fn t_script_read_chapter(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let p = ctx.project()?;
    let target = resolve_chapter(&args)?;
    let id = find_chapter_id(&p, &target)?;
    let ch = p.load_chapter(&id)?;
    Ok(ToolOutcome::new(
        format!("{}（{} 字）", ch.meta.title, ch.meta.word_count),
        json!({ "id": ch.meta.id, "title": ch.meta.title, "status": ch.meta.status, "content": ch.content }),
    ))
}

fn resolve_chapter(args: &Value) -> Result<ChapterTarget> {
    if let Some(id) = arg_opt_str(args, "chapterId") {
        return Ok(ChapterTarget::Id(id));
    }
    if let Some(i) = args.get("index").and_then(|v| v.as_u64()) {
        return Ok(ChapterTarget::Index(i as u32));
    }
    Err(AppError::invalid("需要 chapterId 或 index"))
}

enum ChapterTarget {
    Id(String),
    Index(u32),
}

fn find_chapter_id(p: &Project, t: &ChapterTarget) -> Result<String> {
    match t {
        ChapterTarget::Id(id) => p
            .script
            .chapters
            .iter()
            .find(|c| &c.id == id)
            .map(|c| c.id.clone())
            .ok_or_else(|| AppError::NotFound(format!("章节不存在：{id}"))),
        ChapterTarget::Index(i) => p
            .script
            .chapters
            .iter()
            .find(|c| c.index == *i)
            .map(|c| c.id.clone())
            .ok_or_else(|| AppError::NotFound(format!("第 {i} 章不存在"))),
    }
}

/* ============================ 风格 ============================ */

async fn t_style_list_presets(_ctx: ToolCtx, _args: Value) -> Result<ToolOutcome> {
    let presets = crate::presets::style_presets();
    Ok(ToolOutcome::new(
        format!("内置 {} 个风格预设", presets.len()),
        json!(presets),
    ))
}

async fn t_style_apply_preset(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let id = arg_str(&args, "presetId")?;
    let preset = crate::presets::style_presets()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::NotFound(format!("预设不存在：{id}")))?;
    let name = preset.name.clone();
    ctx.state.mutate(|p| {
        p.style.active_preset_id = Some(preset.id.clone());
        p.style.spec = StyleSpec {
            id: p.style.spec.id.clone(),
            name: preset.name.clone(),
            prompt: preset.prompt.clone(),
            negative: preset.negative.clone(),
            palette: preset.palette.clone(),
            lighting: preset.lighting.clone(),
            lens: preset.lens.clone(),
            film_stock: preset.film_stock.clone(),
            aspect_ratio: if p.style.spec.aspect_ratio.is_empty() {
                p.manifest.aspect_ratio.clone()
            } else {
                p.style.spec.aspect_ratio.clone()
            },
            motion_style: preset.motion_style.clone(),
            notes: String::new(),
        };
        Ok(())
    })?;
    Ok(ToolOutcome::text(format!("已套用风格预设「{name}」")))
}

async fn t_style_set(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let mut changed = vec![];
    ctx.state.mutate(|p| {
        let s = &mut p.style.spec;
        if let Some(v) = arg_opt_str(&args, "prompt") {
            s.prompt = v.clone();
            changed.push("prompt");
        }
        if let Some(v) = arg_opt_str(&args, "negative") {
            s.negative = v;
            changed.push("negative");
        }
        if let Some(v) = arg_opt_str(&args, "name") {
            s.name = v;
        }
        if args.get("palette").is_some() {
            s.palette = arg_str_list(&args, "palette");
        }
        if let Some(v) = arg_opt_str(&args, "lighting") {
            s.lighting = v;
        }
        if let Some(v) = arg_opt_str(&args, "lens") {
            s.lens = v;
        }
        if let Some(v) = arg_opt_str(&args, "filmStock") {
            s.film_stock = v;
        }
        if let Some(v) = arg_opt_str(&args, "motionStyle") {
            s.motion_style = v;
        }
        if let Some(v) = arg_opt_str(&args, "aspectRatio") {
            s.aspect_ratio = v.clone();
            p.manifest.aspect_ratio = v;
        }
        if let Some(v) = arg_opt_str(&args, "notes") {
            s.notes = v;
        }
        if s.id.is_empty() {
            s.id = new_id("sty");
        }
        Ok(())
    })?;
    Ok(ToolOutcome::text(format!("风格已更新：{}", changed.join("、"))))
}

/* ============================ 分镜 ============================ */

async fn t_storyboard_list_shots(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let p = ctx.project()?;
    let filter = arg_opt_str(&args, "chapterId");
    let items: Vec<&Shot> = p
        .shots
        .iter()
        .filter(|s| filter.as_ref().map(|f| &s.chapter_id == f).unwrap_or(true))
        .collect();
    Ok(ToolOutcome::new(
        format!("共 {} 个镜头", items.len()),
        json!(items),
    ))
}

fn parse_shot(v: &Value, chapter_id: &str, index: u32) -> Shot {
    Shot {
        id: new_id("sh"),
        chapter_id: chapter_id.to_string(),
        index,
        scene_id: None,
        location: v
            .get("location")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        time_of_day: v
            .get("timeOfDay")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        interior: v.get("interior").and_then(|x| x.as_bool()).unwrap_or(false),
        shot_size: v
            .get("shotSize")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        camera: v
            .get("camera")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        camera_move: v
            .get("cameraMove")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        duration_sec: v
            .get("durationSec")
            .and_then(|x| x.as_f64())
            .map(|x| x as f32)
            .unwrap_or(3.0),
        characters: v
            .get("characters")
            .and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        props: v
            .get("props")
            .and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        action: v
            .get("action")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        dialogue: v
            .get("dialogue")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        narration: v
            .get("narration")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        sfx: v
            .get("sfx")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        bgm: v
            .get("bgm")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        image_prompt: v
            .get("imagePrompt")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        video_prompt: v
            .get("videoPrompt")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        ref_images: vec![],
        status: ShotStatus::Draft,
    }
}

async fn t_storyboard_write_shots(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let project = ctx.project()?;
    let id = find_chapter_id(&project, &resolve_chapter(&args)?)?;
    let list = arg_arr(&args, "shots");
    if list.is_empty() {
        return Err(AppError::invalid("shots 不能为空"));
    }
    let mut binding = vec![];
    ctx.state.mutate(|p| {
        let len = p.shots.len();
        p.shots.retain(|s| s.chapter_id != id);
        for (i, v) in list.iter().enumerate() {
            let shot = parse_shot(v, &id, i as u32 + 1);
            binding.push(format!("{} {}", shot.index, shot.shot_size));
            p.shots.push(shot);
        }
        // 清理已删除镜头的提示词
        let ids: std::collections::BTreeSet<String> =
            p.shots.iter().map(|s| s.id.clone()).collect();
        p.prompts.retain(|x| ids.contains(&x.shot_id));
        let _ = len;
        Ok(())
    })?;
    Ok(ToolOutcome::new(
        format!("已写入 {} 个镜头", binding.len()),
        json!({ "shots": binding }),
    ))
}

async fn t_storyboard_append_shots(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let project = ctx.project()?;
    let id = find_chapter_id(&project, &resolve_chapter(&args)?)?;
    let list = arg_arr(&args, "shots");
    if list.is_empty() {
        return Err(AppError::invalid("shots 不能为空"));
    }
    let count = list.len();
    ctx.state.mutate(|p| {
        let mut next = p.next_shot_index(&id);
        for v in list.iter() {
            let shot = parse_shot(v, &id, next);
            next += 1;
            p.shots.push(shot);
        }
        Ok(())
    })?;
    Ok(ToolOutcome::text(format!("已追加 {count} 个镜头")))
}

async fn t_storyboard_update_shot(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let shot_id = arg_str(&args, "shotId")?;
    let patch = args
        .get("patch")
        .cloned()
        .unwrap_or_else(|| args.clone());
    ctx.state.mutate(|p| {
        let s = p
            .shots
            .iter_mut()
            .find(|s| s.id == shot_id)
            .ok_or_else(|| AppError::NotFound(format!("镜头不存在：{shot_id}")))?;
        if let Some(v) = patch.get("location").and_then(|x| x.as_str()) {
            s.location = v.into();
        }
        if let Some(v) = patch.get("timeOfDay").and_then(|x| x.as_str()) {
            s.time_of_day = v.into();
        }
        if let Some(v) = patch.get("shotSize").and_then(|x| x.as_str()) {
            s.shot_size = v.into();
        }
        if let Some(v) = patch.get("camera").and_then(|x| x.as_str()) {
            s.camera = v.into();
        }
        if let Some(v) = patch.get("cameraMove").and_then(|x| x.as_str()) {
            s.camera_move = v.into();
        }
        if let Some(v) = patch.get("action").and_then(|x| x.as_str()) {
            s.action = v.into();
        }
        if let Some(v) = patch.get("dialogue").and_then(|x| x.as_str()) {
            s.dialogue = v.into();
        }
        if let Some(v) = patch.get("narration").and_then(|x| x.as_str()) {
            s.narration = v.into();
        }
        if let Some(v) = patch.get("imagePrompt").and_then(|x| x.as_str()) {
            s.image_prompt = v.into();
        }
        if let Some(v) = patch.get("videoPrompt").and_then(|x| x.as_str()) {
            s.video_prompt = v.into();
        }
        if let Some(v) = patch.get("durationSec").and_then(|x| x.as_f64()) {
            s.duration_sec = v as f32;
        }
        if let Some(v) = patch.get("status").and_then(|x| x.as_str()) {
            s.status = match v {
                "draft" => ShotStatus::Draft,
                "ready" => ShotStatus::Ready,
                "prompted" => ShotStatus::Prompted,
                "generated" => ShotStatus::Generated,
                "locked" => ShotStatus::Locked,
                _ => s.status,
            };
        }
        Ok(())
    })?;
    Ok(ToolOutcome::text("镜头已更新"))
}

/* ============================ 资产 ============================ */

async fn t_asset_list(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let p = ctx.project()?;
    let kind = arg_opt_str(&args, "kind");
    let items: Vec<&Asset> = p
        .assets
        .iter()
        .filter(|a| {
            kind.as_ref()
                .map(|k| format!("{:?}", a.kind).to_lowercase() == k.to_lowercase())
                .unwrap_or(true)
        })
        .collect();
    Ok(ToolOutcome::new(
        format!("共 {} 项资产", items.len()),
        json!(items),
    ))
}

fn parse_asset_kind(s: &str) -> AssetKind {
    match s.to_lowercase().as_str() {
        "character" | "人物" => AssetKind::Character,
        "scene" | "场景" => AssetKind::Scene,
        "prop" | "道具" => AssetKind::Prop,
        "costume" | "服装" => AssetKind::Costume,
        "vehicle" | "载具" => AssetKind::Vehicle,
        "effect" | "特效" => AssetKind::Effect,
        _ => AssetKind::Other,
    }
}

fn parse_view_kind(s: &str) -> ViewKind {
    match s.to_lowercase().as_str() {
        "front" | "正面" => ViewKind::Front,
        "side" | "侧面" => ViewKind::Side,
        "back" | "背面" => ViewKind::Back,
        "threequarter" | "three_quarter" | "四分之三侧" => ViewKind::ThreeQuarter,
        "fullbody" | "full_body" | "全身" => ViewKind::FullBody,
        "closeup" | "close_up" | "特写" => ViewKind::CloseUp,
        "wide" | "全景" => ViewKind::Wide,
        "medium" | "中景" => ViewKind::Medium,
        "birdview" | "bird_view" | "俯视" => ViewKind::BirdView,
        _ => ViewKind::Custom,
    }
}

async fn t_asset_upsert(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let kind = parse_asset_kind(&arg_opt_str(&args, "kind").unwrap_or_else(|| "other".into()));
    let name = arg_str(&args, "name")?;
    let id = arg_opt_str(&args, "id");
    let (acted, new_id_str) = ctx.state.mutate(|p| {
        let existing = match &id {
            Some(i) => p.assets.iter_mut().find(|a| &a.id == i),
            None => p.assets.iter_mut().find(|a| a.name == name),
        };
        if let Some(a) = existing {
            a.kind = kind;
            a.name = name.clone();
            a.aliases = arg_str_list(&args, "aliases");
            if let Some(v) = arg_opt_str(&args, "description") {
                a.description = v;
            }
            if !a.locked_traits.is_empty() || args.get("lockedTraits").is_some() {
                a.locked_traits = arg_str_list(&args, "lockedTraits");
            }
            if args.get("tags").is_some() {
                a.tags = arg_str_list(&args, "tags");
            }
            a.updated_at = now_iso();
            Ok(("更新", a.id.clone()))
        } else {
            let nid = new_id("ast");
            p.assets.push(Asset {
                id: nid.clone(),
                kind,
                name: name.clone(),
                aliases: arg_str_list(&args, "aliases"),
                description: arg_opt_str(&args, "description").unwrap_or_default(),
                tags: arg_str_list(&args, "tags"),
                locked_traits: arg_str_list(&args, "lockedTraits"),
                views: vec![],
                created_at: now_iso(),
                updated_at: now_iso(),
            });
            Ok(("新建", nid))
        }
    })?;
    refresh_auto_items(&ctx.state)?;
    Ok(ToolOutcome::text(format!("{acted}资产「{name}」（{new_id_str}）")))
}

async fn t_asset_plan_views(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let asset_id = arg_str(&args, "assetId")?;
    let list = arg_arr(&args, "views");
    if list.is_empty() {
        return Err(AppError::invalid("views 不能为空"));
    }
    let count = list.len();
    ctx.state.mutate(|p| {
        let asset = p
            .assets
            .iter_mut()
            .find(|a| a.id == asset_id)
            .ok_or_else(|| AppError::NotFound(format!("资产不存在：{asset_id}")))?;
        let mut planned: Vec<AssetView> = vec![];
        for v in list.iter() {
            let kind = parse_view_kind(v.get("kind").and_then(|x| x.as_str()).unwrap_or("custom"));
            let label = v
                .get("label")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| crate::agent::prompt::view_kind_label(kind).to_string());
            // 已出图的同类型视图保留
            if let Some(existing) = asset
                .views
                .iter()
                .find(|x| x.kind == kind && x.label == label && x.status == AssetStatus::Done)
            {
                planned.push(existing.clone());
                continue;
            }
            planned.push(AssetView {
                id: new_id("vw"),
                kind,
                label,
                prompt: v
                    .get("prompt")
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string(),
                negative: v
                    .get("negative")
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string(),
                file: None,
                thumb: None,
                seed: v.get("seed").and_then(|x| x.as_i64()),
                model: None,
                provider_id: None,
                status: AssetStatus::Planned,
                error: None,
                job_id: None,
                created_at: now_iso(),
            });
        }
        asset.views = planned;
        asset.updated_at = now_iso();
        Ok(())
    })?;
    refresh_auto_items(&ctx.state)?;
    Ok(ToolOutcome::text(format!(
        "已为资产规划 {count} 个视图"
    )))
}

async fn t_asset_generate_view(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let asset_id = arg_str(&args, "assetId")?;
    let view_id = arg_opt_str(&args, "viewId");
    let force = arg_bool(&args, "force", false);
    let ids = view_id.map(|v| vec![v]).unwrap_or_default();
    let jobs = crate::actions::generate_asset_views(&ctx.state, &asset_id, ids, force)?;
    refresh_auto_items(&ctx.state)?;
    Ok(ToolOutcome::new(
        format!("已提交 {} 个生图任务，任务会在后台执行", jobs.len()),
        json!({ "jobIds": jobs }),
    ))
}

async fn t_asset_generate_missing(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let filter = arg_opt_str(&args, "assetId");
    let p = ctx.project()?;
    let targets: Vec<(String, String)> = p
        .assets
        .iter()
        .filter(|a| filter.as_ref().map(|f| &a.id == f).unwrap_or(true))
        .flat_map(|a| {
            a.views
                .iter()
                .filter(|v| matches!(v.status, AssetStatus::Planned | AssetStatus::Failed))
                .map(move |v| (a.id.clone(), v.id.clone()))
        })
        .collect();
    if targets.is_empty() {
        return Ok(ToolOutcome::text("没有待生成的视图"));
    }
    let mut jobs = vec![];
    let mut by_asset: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (a, v) in targets {
        by_asset.entry(a).or_default().push(v);
    }
    for (a, vs) in by_asset {
        jobs.extend(crate::actions::generate_asset_views(&ctx.state, &a, vs, false)?);
    }
    refresh_auto_items(&ctx.state)?;
    Ok(ToolOutcome::new(
        format!("已提交 {} 个生图任务", jobs.len()),
        json!({ "jobIds": jobs }),
    ))
}

/* ========================== 用户知识包 ========================== */

async fn t_knowledge_list(ctx: ToolCtx, _args: Value) -> Result<ToolOutcome> {
    let packs = crate::knowledge::list(&ctx.state);
    if packs.is_empty() {
        return Ok(ToolOutcome::text(
            "用户还没有装知识包。可以在「设置 → 知识包」里导入 Markdown。",
        ));
    }
    let items: Vec<Value> = packs
        .iter()
        .map(|p| {
            json!({
                "id": p.id, "name": p.name, "kind": p.kind,
                "summary": p.summary, "chars": p.chars, "enabled": p.enabled,
            })
        })
        .collect();
    Ok(ToolOutcome::new(
        format!("共 {} 个知识包", items.len()),
        json!(items),
    ))
}

async fn t_knowledge_read(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let id = arg_str(&args, "id")?;
    let body = crate::knowledge::read(&ctx.state, &id)?;
    let chars = body.chars().count();
    // 太长的包分片给，避免一次撑爆上下文
    let text: String = body.chars().take(60000).collect();
    Ok(ToolOutcome::new(
        format!("已读取知识包「{id}」（{chars} 字）"),
        json!({ "id": id, "chars": chars, "truncated": chars > 60000, "body": text }),
    ))
}

/* ============================ 提问 ============================ */

/// 占位实现。真正的「挂起等人回答」在 agent 的 run loop 里做 ——
/// 那里才能拿到事件通道与等待队列。这个 handler 不会被调用到。
async fn t_ask_user(_ctx: ToolCtx, _args: Value) -> Result<ToolOutcome> {
    Err(AppError::other("ask_user 由运行循环处理，不应直接执行"))
}

/* ============================== 看图 ============================== */

async fn t_asset_view_image(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let asset_id = arg_str(&args, "assetId")?;
    let view_id = arg_opt_str(&args, "viewId");
    let p = ctx.project()?;
    let asset = p
        .assets
        .iter()
        .find(|a| a.id == asset_id)
        .ok_or_else(|| AppError::NotFound(format!("资产不存在：{asset_id}")))?;
    let views: Vec<&AssetView> = match &view_id {
        Some(v) => asset.views.iter().filter(|x| &x.id == v).collect(),
        None => asset
            .views
            .iter()
            .filter(|x| x.file.is_some())
            .take(MAX_TOOL_IMAGES)
            .collect(),
    };
    if views.is_empty() {
        return Err(AppError::NotFound(format!(
            "「{}」还没有已生成的图可以看",
            asset.name
        )));
    }
    let mut images = vec![];
    let mut skipped = vec![];
    for v in &views {
        let Some(file) = &v.file else { continue };
        match tool_image_from_file(
            std::path::Path::new(file),
            &format!("{}·{}", asset.name, v.label),
            MAX_TOOL_IMAGE_BYTES,
        ) {
            Some(img) => images.push(img),
            None => skipped.push(v.label.clone()),
        }
    }
    let summary = if skipped.is_empty() {
        format!("已把「{}」的 {} 张图放进上下文", asset.name, images.len())
    } else {
        format!(
            "已放入 {} 张图；{} 因为体积过大跳过",
            images.len(),
            skipped.join("、")
        )
    };
    Ok(ToolOutcome::new(
        summary,
        json!({
            "assetId": asset.id,
            "name": asset.name,
            "images": views.iter().map(|v| json!({
                "viewId": v.id, "label": v.label, "kind": v.kind,
                "prompt": v.prompt, "seed": v.seed, "model": v.model,
            })).collect::<Vec<_>>(),
        }),
    )
    .with_images(images))
}

async fn t_file_view_image(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let paths = if let Some(p) = arg_opt_str(&args, "path") {
        vec![p]
    } else {
        arg_str_list(&args, "paths")
    };
    if paths.is_empty() {
        return Err(AppError::invalid("需要 path 或 paths"));
    }
    let project = ctx.project()?;
    let mut images = vec![];
    let mut missing = vec![];
    for raw in paths.iter().take(MAX_TOOL_IMAGES) {
        let path = std::path::Path::new(raw);
        if !path.is_file() {
            missing.push(raw.clone());
            continue;
        }
        let label = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| raw.clone());
        match tool_image_from_file(path, &label, MAX_TOOL_IMAGE_BYTES) {
            Some(img) => images.push(img),
            None => missing.push(format!("{label}（太大）")),
        }
    }
    if images.is_empty() {
        return Err(AppError::NotFound(format!(
            "没有可用的图片。{}",
            missing.join("、")
        )));
    }
    let _ = project;
    Ok(ToolOutcome::new(
        format!("已放入 {} 张图", images.len()),
        json!({ "loaded": images.iter().map(|i| i.label.clone()).collect::<Vec<_>>(), "skipped": missing }),
    )
    .with_images(images))
}

/* ============================ 视频提示词 ============================ */

async fn t_prompt_list(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let p = ctx.project()?;
    let shot = arg_opt_str(&args, "shotId");
    let items: Vec<&VideoPrompt> = p
        .prompts
        .iter()
        .filter(|x| shot.as_ref().map(|s| &x.shot_id == s).unwrap_or(true))
        .collect();
    let missing = p
        .shots
        .iter()
        .filter(|s| !p.prompts.iter().any(|x| x.shot_id == s.id))
        .count();
    Ok(ToolOutcome::new(
        format!("已有 {} 条提示词，{} 个镜头还缺提示词", items.len(), missing),
        json!({ "prompts": items, "missingShots": missing }),
    ))
}

async fn t_prompt_upsert(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let shot_id = arg_str(&args, "shotId")?;
    let id = ctx.state.mutate(|p| {
        if !p.shots.iter().any(|s| s.id == shot_id) {
            return Err(AppError::NotFound(format!("镜头不存在：{shot_id}")));
        }
        let next_index = p.prompts.iter().map(|x| x.index).max().unwrap_or(0) + 1;
        let shot_index = p
            .shots
            .iter()
            .find(|s| s.id == shot_id)
            .map(|s| s.index)
            .unwrap_or(next_index);
        let existing_idx = p.prompts.iter().position(|x| x.shot_id == shot_id);
        let prev = existing_idx.map(|i| p.prompts[i].clone());
        let mut item = VideoPrompt {
            id: prev
                .as_ref()
                .map(|x| x.id.clone())
                .unwrap_or_else(|| new_id("vp")),
            shot_id: shot_id.clone(),
            index: shot_index,
            prompt: arg_opt_str(&args, "prompt")
                .or_else(|| prev.as_ref().map(|x| x.prompt.clone()))
                .unwrap_or_default(),
            negative: arg_opt_str(&args, "negative")
                .or_else(|| prev.as_ref().map(|x| x.negative.clone()))
                .unwrap_or_default(),
            motion: arg_opt_str(&args, "motion")
                .or_else(|| prev.as_ref().map(|x| x.motion.clone()))
                .unwrap_or_default(),
            camera_move: arg_opt_str(&args, "cameraMove")
                .or_else(|| prev.as_ref().map(|x| x.camera_move.clone()))
                .unwrap_or_default(),
            duration_sec: args
                .get("durationSec")
                .and_then(|x| x.as_f64())
                .map(|x| x as f32)
                .or_else(|| prev.as_ref().map(|x| x.duration_sec))
                .unwrap_or(3.0),
            first_frame: parse_ref(args.get("firstFrame"))
                .or_else(|| prev.as_ref().and_then(|x| x.first_frame.clone())),
            last_frame: parse_ref(args.get("lastFrame"))
                .or_else(|| prev.as_ref().and_then(|x| x.last_frame.clone())),
            refs: if args.get("refs").is_some() {
                args.get("refs")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| parse_ref(Some(x))).collect())
                    .unwrap_or_default()
            } else {
                prev.as_ref().map(|x| x.refs.clone()).unwrap_or_default()
            },
            model_hint: arg_opt_str(&args, "modelHint")
                .or_else(|| prev.as_ref().and_then(|x| x.model_hint.clone())),
            seed: args
                .get("seed")
                .and_then(|x| x.as_i64())
                .or_else(|| prev.as_ref().and_then(|x| x.seed)),
            status: PromptStatus::Ready,
            updated_at: now_iso(),
        };
        if item.prompt.trim().is_empty() {
            return Err(AppError::invalid("prompt 不能为空"));
        }
        if item.first_frame.is_none() && !item.refs.is_empty() {
            item.first_frame = item.refs.first().cloned();
        }
        match existing_idx {
            Some(i) => p.prompts[i] = item.clone(),
            None => p.prompts.push(item.clone()),
        }
        Ok(item.id)
    })?;
    refresh_auto_items(&ctx.state)?;
    Ok(ToolOutcome::text(format!("提示词已保存（{id}）")))
}

fn parse_ref(v: Option<&Value>) -> Option<AssetRef> {
    let v = v?;
    if v.is_null() {
        return None;
    }
    let asset_id = v.get("assetId").and_then(|x| x.as_str())?.to_string();
    Some(AssetRef {
        asset_id,
        view_id: v.get("viewId").and_then(|x| x.as_str()).map(String::from),
    })
}

async fn t_prompt_bind_assets(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let shot_id = arg_str(&args, "shotId")?;
    let refs: Vec<AssetRef> = args
        .get("refs")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| parse_ref(Some(x))).collect())
        .unwrap_or_default();
    let first = parse_ref(args.get("firstFrame"));
    let n = refs.len();
    ctx.state.mutate(|p| {
        let item = p
            .prompts
            .iter_mut()
            .find(|x| x.shot_id == shot_id)
            .ok_or_else(|| AppError::NotFound("该镜头还没有提示词，先写提示词".into()))?;
        item.refs = refs.clone();
        if let Some(f) = first {
            item.first_frame = Some(f);
        } else if item.first_frame.is_none() {
            item.first_frame = refs.first().cloned();
        }
        item.status = PromptStatus::Bound;
        item.updated_at = now_iso();
        Ok(())
    })?;
    Ok(ToolOutcome::text(format!("已绑定 {n} 个参考资产")))
}

async fn t_prompt_autofill(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let chapter = arg_opt_str(&args, "chapterId");
    let mut created = 0;
    ctx.state.mutate(|p| {
        let assets: Vec<(String, String, Vec<(String, String)>)> = p
            .assets
            .iter()
            .map(|a| {
                (
                    a.id.clone(),
                    a.name.clone(),
                    a.views
                        .iter()
                        .filter(|v| v.status == AssetStatus::Done && v.file.is_some())
                        .map(|v| (v.id.clone(), v.label.clone()))
                        .collect(),
                )
            })
            .collect();
        let shots: Vec<Shot> = p
            .shots
            .iter()
            .filter(|s| chapter.as_ref().map(|c| &s.chapter_id == c).unwrap_or(true))
            .cloned()
            .collect();
        for shot in shots {
            if p.prompts.iter().any(|x| x.shot_id == shot.id) {
                continue;
            }
            let text = if !shot.video_prompt.trim().is_empty() {
                shot.video_prompt.clone()
            } else {
                format!(
                    "{}，{}，镜头{}，时长约 {:.0} 秒",
                    shot.action, shot.shot_size, shot.camera_move, shot.duration_sec
                )
            };
            // 按名字匹配出场人物的成图，凑够参考图
            let mut refs: Vec<AssetRef> = vec![];
            for name in shot.characters.iter().chain(shot.props.iter()) {
                if let Some((aid, _, views)) =
                    assets.iter().find(|(_, n, _)| n == name)
                {
                    refs.push(AssetRef {
                        asset_id: aid.clone(),
                        view_id: views.first().map(|(id, _)| id.clone()),
                    });
                }
            }
            let first = refs.first().cloned();
            p.prompts.push(VideoPrompt {
                id: new_id("vp"),
                shot_id: shot.id.clone(),
                index: shot.index,
                prompt: text,
                negative: p.style.spec.negative.clone(),
                motion: shot.action.clone(),
                camera_move: shot.camera_move.clone(),
                duration_sec: shot.duration_sec,
                first_frame: first,
                last_frame: None,
                refs,
                model_hint: None,
                seed: None,
                status: PromptStatus::Ready,
                updated_at: now_iso(),
            });
            created += 1;
        }
        Ok(())
    })?;
    refresh_auto_items(&ctx.state)?;
    Ok(ToolOutcome::text(format!("已自动补齐 {created} 条提示词")))
}

/* ============================ 生视频 ============================ */

async fn t_video_list_takes(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let p = ctx.project()?;
    let shot = arg_opt_str(&args, "shotId");
    let items: Vec<&VideoTake> = p
        .takes
        .iter()
        .filter(|t| shot.as_ref().map(|s| &t.shot_id == s).unwrap_or(true))
        .collect();
    let done = items
        .iter()
        .filter(|t| t.status == AssetStatus::Done)
        .count();
    Ok(ToolOutcome::new(
        format!("{} 条生成记录，其中 {} 条成功", items.len(), done),
        json!(items),
    ))
}

async fn t_video_generate(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let shot_ids = if let Some(s) = arg_opt_str(&args, "shotId") {
        vec![s]
    } else {
        arg_str_list(&args, "shotIds")
    };
    if shot_ids.is_empty() {
        return Err(AppError::invalid("需要 shotId 或 shotIds"));
    }
    let jobs = crate::actions::generate_video_takes(
        &ctx.state,
        shot_ids.clone(),
        arg_opt_str(&args, "providerId"),
        arg_opt_str(&args, "model"),
    )?;
    refresh_auto_items(&ctx.state)?;
    Ok(ToolOutcome::new(
        format!("已提交 {} 个视频生成任务", jobs.len()),
        json!({ "jobIds": jobs, "shotIds": shot_ids }),
    ))
}

async fn t_video_generate_batch(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let chapter = arg_opt_str(&args, "chapterId");
    let only_missing = arg_bool(&args, "onlyMissing", true);
    let p = ctx.project()?;
    let ids: Vec<String> = p
        .shots
        .iter()
        .filter(|s| chapter.as_ref().map(|c| &s.chapter_id == c).unwrap_or(true))
        .filter(|s| {
            !only_missing
                || !p
                    .takes
                    .iter()
                    .any(|t| t.shot_id == s.id && t.status == AssetStatus::Done)
        })
        .map(|s| s.id.clone())
        .collect();
    if ids.is_empty() {
        return Ok(ToolOutcome::text("没有需要生成的镜头"));
    }
    let jobs = crate::actions::generate_video_takes(&ctx.state, ids.clone(), None, None)?;
    refresh_auto_items(&ctx.state)?;
    Ok(ToolOutcome::new(
        format!("已批量提交 {} 个视频任务（还有 {} 个镜头待生成）", jobs.len(), ids.len()),
        json!({ "jobIds": jobs }),
    ))
}

/* ============================ 剪辑 ============================ */

async fn t_edit_get_timeline(ctx: ToolCtx, _args: Value) -> Result<ToolOutcome> {
    let p = ctx.project()?;
    let clips: usize = p.timeline.tracks.iter().map(|t| t.clips.len()).sum();
    Ok(ToolOutcome::new(
        format!(
            "时间线 {:.1} 秒，共 {} 个片段",
            p.timeline.duration_sec, clips
        ),
        json!(p.timeline),
    ))
}

async fn t_edit_build_from_takes(ctx: ToolCtx, _args: Value) -> Result<ToolOutcome> {
    let n = crate::actions::build_timeline_from_takes(&ctx.state)?;
    refresh_auto_items(&ctx.state)?;
    Ok(ToolOutcome::text(format!("已按镜头顺序铺入 {n} 段")))
}

async fn t_edit_append_clip(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let track_id = arg_opt_str(&args, "trackId").unwrap_or_else(|| "tr_video".into());
    let take_id = arg_opt_str(&args, "takeId");
    let source = arg_opt_str(&args, "source");
    let (src, label, shot_id) = match (&take_id, &source) {
        (Some(t), _) => {
            let p = ctx.project()?;
            let take = p
                .takes
                .iter()
                .find(|x| &x.id == t)
                .ok_or_else(|| AppError::NotFound(format!("生成记录不存在：{t}")))?;
            (
                take.file.clone().ok_or_else(|| AppError::invalid("该记录还没有成片文件"))?,
                take.shot_id.clone(),
                Some(take.shot_id.clone()),
            )
        }
        (None, Some(s)) => (s.clone(), s.clone(), None),
        _ => return Err(AppError::invalid("需要 takeId 或 source")),
    };
    let dur = arg_f32(&args, "durationSec", 0.0);
    ctx.state.mutate(|p| {
        let track = p
            .timeline
            .tracks
            .iter_mut()
            .find(|t| t.id == track_id)
            .ok_or_else(|| AppError::NotFound(format!("轨道不存在：{track_id}")))?;
        let start = args
            .get("startSec")
            .and_then(|x| x.as_f64())
            .map(|x| x as f32)
            .unwrap_or_else(|| {
                track
                    .clips
                    .iter()
                    .map(|c| c.start_sec + (c.out_sec - c.in_sec))
                    .fold(0.0f32, f32::max)
            });
        let d = if dur > 0.0 { dur } else { 3.0 };
        track.clips.push(Clip {
            id: new_id("clip"),
            source: src.clone(),
            shot_id,
            take_id,
            label,
            in_sec: arg_f32(&args, "inSec", 0.0),
            out_sec: arg_f32(&args, "outSec", d),
            start_sec: start,
            speed: 1.0,
            volume: 1.0,
            enabled: true,
        });
        p.timeline.duration_sec = p
            .timeline
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .map(|c| c.start_sec + (c.out_sec - c.in_sec))
            .fold(0.0f32, f32::max);
        p.timeline.updated_at = now_iso();
        Ok(())
    })?;
    refresh_auto_items(&ctx.state)?;
    Ok(ToolOutcome::text("已添加片段时间线"))
}

async fn t_edit_update_clip(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let clip_id = arg_str(&args, "clipId")?;
    ctx.state.mutate(|p| {
        let mut found = false;
        for t in p.timeline.tracks.iter_mut() {
            if let Some(c) = t.clips.iter_mut().find(|c| c.id == clip_id) {
                if let Some(v) = args.get("startSec").and_then(|x| x.as_f64()) {
                    c.start_sec = v as f32;
                }
                if let Some(v) = args.get("inSec").and_then(|x| x.as_f64()) {
                    c.in_sec = v as f32;
                }
                if let Some(v) = args.get("outSec").and_then(|x| x.as_f64()) {
                    c.out_sec = v as f32;
                }
                if let Some(v) = args.get("speed").and_then(|x| x.as_f64()) {
                    c.speed = v as f32;
                }
                if let Some(v) = args.get("volume").and_then(|x| x.as_f64()) {
                    c.volume = v as f32;
                }
                if let Some(v) = args.get("enabled").and_then(|x| x.as_bool()) {
                    c.enabled = v;
                }
                found = true;
                break;
            }
        }
        if !found {
            return Err(AppError::NotFound(format!("片段不存在：{clip_id}")));
        }
        p.timeline.duration_sec = p
            .timeline
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .map(|c| c.start_sec + (c.out_sec - c.in_sec) / c.speed.max(0.01))
            .fold(0.0f32, f32::max);
        Ok(())
    })?;
    Ok(ToolOutcome::text("片段已更新"))
}

async fn t_edit_remove_clip(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let clip_id = arg_str(&args, "clipId")?;
    ctx.state.mutate(|p| {
        for t in p.timeline.tracks.iter_mut() {
            t.clips.retain(|c| c.id != clip_id);
        }
        Ok(())
    })?;
    Ok(ToolOutcome::text("片段已删除"))
}

async fn t_edit_render(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let burn = arg_bool(&args, "burnSubtitles", true);
    let job = crate::actions::render_timeline_job(&ctx.state, burn)?;
    Ok(ToolOutcome::new(
        "已开始导出成片，完成后会在任务栏提示",
        json!({ "jobId": job }),
    ))
}

/* ============================ 字幕 ============================ */

async fn t_subtitle_list(ctx: ToolCtx, _args: Value) -> Result<ToolOutcome> {
    let p = ctx.project()?;
    let brief: Vec<Value> = p
        .subtitles
        .iter()
        .map(|s| {
            json!({
                "id": s.id, "name": s.name, "source": s.source,
                "language": s.language, "model": s.model,
                "cueCount": s.cues.len(), "file": s.file, "createdAt": s.created_at,
            })
        })
        .collect();
    Ok(ToolOutcome::new(
        format!("共 {} 份字幕", brief.len()),
        json!(brief),
    ))
}

async fn t_subtitle_transcribe(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let file = arg_opt_str(&args, "file").or_else(|| {
        ctx.state
            .current_opt()
            .and_then(|p| p.timeline.tracks.iter().flat_map(|t| t.clips.iter()).next().map(|c| c.source.clone()))
    });
    let file = file.ok_or_else(|| AppError::invalid("需要 file 参数（要转写的视频/音频路径）"))?;
    let s = ctx.state.settings();
    let opts = crate::asr::TranscribeOptions {
        file,
        language: arg_opt_str(&args, "language").unwrap_or_else(|| "zh".into()),
        model_id: arg_opt_str(&args, "modelId").unwrap_or(s.asr_model),
        use_gpu: arg_bool(&args, "useGpu", s.asr_use_gpu),
        threads: s.asr_threads,
        max_line_chars: arg_u32(&args, "maxLineChars", 18),
        name: arg_opt_str(&args, "name").unwrap_or_default(),
    };
    let job = crate::actions::transcribe_job(&ctx.state, opts)?;
    Ok(ToolOutcome::new(
        "已开始转写，完成后字幕会自动出现在面板里",
        json!({ "jobId": job }),
    ))
}

async fn t_subtitle_update_cues(ctx: ToolCtx, args: Value) -> Result<ToolOutcome> {
    let id = arg_str(&args, "subtitleId")?;
    let cues: Vec<Cue> = args
        .get("cues")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    Some(Cue {
                        index: c.get("index").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                        start: c.get("start").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32,
                        end: c.get("end").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32,
                        text: c.get("text").and_then(|x| x.as_str())?.to_string(),
                    })
                })
                .collect()
        })
        .ok_or_else(|| AppError::invalid("cues 不能为空"))?;
    let n = cues.len();
    ctx.state.mutate(|p| {
        let doc = p
            .subtitles
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| AppError::NotFound(format!("字幕不存在：{id}")))?;
        doc.cues = cues.clone();
        if let Some(f) = doc.file.clone() {
            let srt = crate::asr::to_srt(&cues);
            crate::store::write_text(std::path::Path::new(&f), &srt)?;
        }
        Ok(())
    })?;
    Ok(ToolOutcome::text(format!("字幕已更新，共 {n} 条")))
}

/* ============================ 面板解析 ============================ */

fn parse_panel(s: &str) -> Result<PanelId> {
    match s.to_lowercase().as_str() {
        "script" => Ok(PanelId::Script),
        "style" => Ok(PanelId::Style),
        "storyboard" => Ok(PanelId::Storyboard),
        "asset" => Ok(PanelId::Asset),
        "prompt" => Ok(PanelId::Prompt),
        "video" => Ok(PanelId::Video),
        "edit" => Ok(PanelId::Edit),
        "subtitle" => Ok(PanelId::Subtitle),
        "checklist" => Ok(PanelId::Checklist),
        other => Err(AppError::invalid(format!("未知面板：{other}"))),
    }
}

/* ============================ 注册表 ============================ */

/// 返回**全部**工具。
///
/// 以前是按面板裁剪的，但 agent 现在是共享上下文、可以跨面板操作，
/// 所以工具集也合并成一份。参数 `_panel` 保留只是为了调用点少改一点。
pub fn registry(_panel: PanelId) -> Vec<Tool> {
    let mut tools = vec![
        tool!(
            "project_snapshot",
            "读取项目数据",
            "读取当前项目的结构化数据。默认返回全景概览；需要细节时用 sections 指定，可选：manifest, bible, chapters, style, shots, assets, prompts, takes, timeline, subtitles, checklist, progress。章节正文不在其中，要用 script_read_chapter 读。",
            schema::sections(),
            false,
            t_project_snapshot
        ),
        tool!(
            "checklist_report",
            "读取完成度",
            "读取每个面板的完成度与阻塞项，以及当前检查清单。想确认「还差什么」时用它。",
            schema::empty(),
            false,
            t_checklist_report
        ),
        tool!(
            "checklist_add",
            "新增检查项",
            "往检查清单里加一条自定义检查项。",
            schema::checklist_add(),
            false,
            t_checklist_add
        ),
        tool!(
            "checklist_toggle",
            "勾选检查项",
            "勾选或取消一个自定义检查项。自动判定的项不能改。",
            schema::checklist_toggle(),
            false,
            t_checklist_toggle
        ),
        tool!(
            "knowledge_list",
            "列出知识包",
            "列出用户装的知识包（名称、类型、摘要、字数）。用户说「按我的体系来」时先用它看看有什么。",
            schema::empty(),
            false,
            t_knowledge_list
        ),
        tool!(
            "knowledge_read",
            "读取知识包正文",
            "读取某个知识包的完整正文。**只在真正要用的时候读** —— 正文可能几万字，读进来会占上下文。",
            schema::knowledge_read(),
            false,
            t_knowledge_read
        ),
        tool!(
            "ask_user",
            "向用户提问",
            "拿不准用户想要什么、或者需要在几个方案里做选择时，用它直接问，不要猜。
可以给出候选项（options），前端会渲染成按钮；也可以不给，用户自由作答。
一次问一件事，问完就停下等回答。",
            schema::ask_user(),
            false,
            t_ask_user
        ),
        tool!(
            "asset_view_image",
            "查看资产图片",
            "把某个资产已生成的图放进你的视觉上下文，用来做一致性检查（人物三视图是否同一个人、场景风格是否统一）。不传 viewId 时最多取 4 张。",
            schema::asset_view_image(),
            false,
            t_asset_view_image
        ),
        tool!(
            "file_view_image",
            "查看本地图片",
            "把任意本地图片文件放进你的视觉上下文，例如导出的成片抽帧、用户给的参考图。",
            schema::file_view_image(),
            false,
            t_file_view_image
        ),
        tool!(
            "bible_update",
            "更新项目圣经",
            "更新故事梗概、卖点、世界观、基调、目标观众、备注或人物卡。人物卡按 name 匹配，已存在则更新、不存在则新建。",
            schema::bible_update(),
            false,
            t_bible_update
        ),
    ];

    // 九个面板的工具全部注册进来
    {
        tools.extend(vec![
            tool!(
                "script_set_chapters",
                "设定章节骨架",
                "覆盖式设置整部剧的章节列表（标题 + 本 章大纲）。已存在的同序号章节会保留 id 与正文，只更新标题与大纲。用于用户指定「要多少章」之后。",
                schema::script_set_chapters(),
                false,
                t_script_set_chapters
            ),
            tool!(
                "script_append_chapter",
                "追加章节",
                "在末尾追加一章，可同时写入正文。",
                schema::script_append_chapter(),
                false,
                t_script_append_chapter
            ),
            tool!(
                "script_read_chapter",
                "读取章节",
                "按 chapterId 或 index 读取章节正文与元信息。",
                schema::chapter_ref(false),
                false,
                t_script_read_chapter
            ),
            tool!(
                "script_write_chapter",
                "写入章节正文",
                "按 chapterId 或 index 写入章节正文（Markdown）。可同时更新大纲与状态。正文超过 200 字会自动标记为 written。",
                schema::chapter_ref(true),
                false,
                t_script_write_chapter
            ),
        ]);
        tools.extend(vec![
            tool!(
                "style_list_presets",
                "列出风格预设",
                "列出内置风格预设，含每个预设的提示词、负面词、色调、光线、镜头等字段。",
                schema::empty(),
                false,
                t_style_list_presets
            ),
            tool!(
                "style_apply_preset",
                "套用风格预设",
                "把某个内置预设整套写进当前风格圣经。",
                schema::style_apply_preset(),
                false,
                t_style_apply_preset
            ),
            tool!(
                "style_set",
                "修改风格",
                "逐字段修改风格圣经。风格是下游所有生图生视频的共享前缀，改之前想清楚影响面。",
                schema::style_set(),
                false,
                t_style_set
            ),
        ]);
        tools.extend(vec![
            tool!(
                "storyboard_list_shots",
                "列出镜头",
                "列出镜头表，可用 chapterId 过滤。",
                schema::chapter_filter(),
                false,
                t_storyboard_list_shots
            ),
            tool!(
                "storyboard_write_shots",
                "覆盖写入整章分镜",
                "用给定的镜头数组**替换**该章全部分镜（镜号自动重排 1..n）。一次写完一章时用它。",
                schema::storyboard_write_shots(),
                false,
                t_storyboard_write_shots
            ),
            tool!(
                "storyboard_append_shots",
                "追加镜头",
                "在该章末尾追加若干镜头，镜号自动接续。",
                schema::storyboard_write_shots(),
                false,
                t_storyboard_append_shots
            ),
            tool!(
                "storyboard_update_shot",
                "修改单个镜头",
                "按 shotId 局部修改一个镜头的字段。",
                schema::storyboard_update_shot(),
                false,
                t_storyboard_update_shot
            ),
        ]);
        tools.extend(vec![
            tool!(
                "asset_list",
                "列出资产",
                "列出资产清单，可用 kind 过滤（character/scene/prop/costume/vehicle/effect/other）。",
                schema::asset_list(),
                false,
                t_asset_list
            ),
            tool!(
                "asset_upsert",
                "新增或更新资产",
                "新建资产或更新已有资产。人物要把跨图一致的特征写进 lockedTraits。",
                schema::asset_upsert(),
                false,
                t_asset_upsert
            ),
            tool!(
                "asset_plan_views",
                "规划资产视图",
                "为该资产设定要生成哪些视图（人物三视图、场景多角度等）。已出图的同类型视图会保留。",
                schema::asset_plan_views(),
                false,
                t_asset_plan_views
            ),
            tool!(
                "asset_generate_view",
                "生成资产图片",
                "调用生图模型生成某个视图。**会消耗额度**，一次一个视图，提交后后台执行。",
                schema::asset_generate_view(),
                true,
                t_asset_generate_view
            ),
            tool!(
                "asset_generate_missing",
                "批量生成缺失图片",
                "把所有待生成/失败的视图一次性提交。**会消耗额度**，提交前先跟用户确认清楚。",
                schema::asset_generate_missing(),
                true,
                t_asset_generate_missing
            ),
        ]);
        tools.extend(vec![
            tool!(
                "prompt_list",
                "列出视频提示词",
                "列出视频提示词，并告诉你有多少镜头还缺提示词。",
                schema::shot_filter(),
                false,
                t_prompt_list
            ),
            tool!(
                "prompt_upsert",
                "写入视频提示词",
                "为一个镜头写入/更新视频提示词。提示词要描述运动过程。firstFrame 传 {assetId, viewId} 表示首帧用哪张图。",
                schema::prompt_upsert(),
                false,
                t_prompt_upsert
            ),
            tool!(
                "prompt_bind_assets",
                "绑定参考资产",
                "给某个镜头的提示词绑定参考图资产。",
                schema::prompt_bind_assets(),
                false,
                t_prompt_bind_assets
            ),
            tool!(
                "prompt_autofill_from_shots",
                "批量补齐提示词",
                "给还没有提示词的镜头自动生成初稿，并按出场人物/道具名字自动配对已出图的资产。",
                schema::chapter_filter(),
                false,
                t_prompt_autofill
            ),
        ]);
        tools.extend(vec![
            tool!(
                "video_list_takes",
                "列出生成记录",
                "列出视频生成记录（take），含状态、文件路径、失败原因。",
                schema::shot_filter(),
                false,
                t_video_list_takes
            ),
            tool!(
                "video_generate",
                "生成视频",
                "为指定镜头提交视频生成任务。**会消耗额度**。",
                schema::video_generate(),
                true,
                t_video_generate
            ),
            tool!(
                "video_generate_batch",
                "批量生成视频",
                "按章节批量提交视频生成任务，默认只补还没出片的镜头。**会消耗额度**，提交前先跟用户确认。",
                schema::video_generate_batch(),
                true,
                t_video_generate_batch
            ),
        ]);
        tools.extend(vec![
            tool!(
                "edit_get_timeline",
                "读取时间线",
                "读取当前时间线（轨道与片段）。",
                schema::empty(),
                false,
                t_edit_get_timeline
            ),
            tool!(
                "edit_build_from_takes",
                "按镜头铺轨",
                "把所有已生成的视频按「章节顺序 + 镜号」自动铺到主视频轨，会清空原主轨。",
                schema::empty(),
                false,
                t_edit_build_from_takes
            ),
            tool!(
                "edit_append_clip",
                "添加片段",
                "往某条轨道追加一个片段。可给 takeId 或直接给 source 文件路径。",
                schema::edit_append_clip(),
                false,
                t_edit_append_clip
            ),
            tool!(
                "edit_update_clip",
                "修改片段",
                "修改片段的入点/出点/起点/速度/音量。",
                schema::edit_update_clip(),
                false,
                t_edit_update_clip
            ),
            tool!(
                "edit_remove_clip",
                "删除片段",
                "从时间线删除一个片段。",
                schema::clip_ref(),
                false,
                t_edit_remove_clip
            ),
            tool!(
                "edit_render",
                "导出成片",
                "按当前时间线导出成片（ffmpeg 拼接）。可选把字幕烧进画面。",
                schema::edit_render(),
                false,
                t_edit_render
            ),
        ]);
        tools.extend(vec![
            tool!(
                "subtitle_list",
                "列出字幕",
                "列出已有的字幕文档。",
                schema::empty(),
                false,
                t_subtitle_list
            ),
            tool!(
                "subtitle_transcribe",
                "转写字幕",
                "用本地 whisper 把视频/音频转写成字幕。不传 file 时默认用时间线上第一个片段。转写在本机进行，素材不外传。",
                schema::subtitle_transcribe(),
                false,
                t_subtitle_transcribe
            ),
            tool!(
                "subtitle_update_cues",
                "修改字幕文本",
                "整体替换某份字幕的条目（校对断句时用），会同时写回 srt 文件。",
                schema::subtitle_update_cues(),
                false,
                t_subtitle_update_cues
            ),
        ]);
    }

    tools
}
