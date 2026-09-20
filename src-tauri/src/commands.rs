//! Tauri 命令层：前端唯一入口。业务逻辑都在 actions / project / asr 等模块里。

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::State;

use crate::agent::{self, AgentEvent, AgentSession};
use crate::asr::{self, AsrCapabilities, TranscribeOptions, WhisperModelInfo};
use crate::checklist_ops;
use crate::error::{AppError, Result};
use crate::models::*;
use crate::progress;
use crate::project::{Project, ProjectPaths};
use crate::state::AppState;

/* ================================================================== 项目 */

#[tauri::command]
pub async fn project_create(
    state: State<'_, AppState>,
    parent_dir: String,
    name: String,
) -> Result<ProjectSnapshot> {
    let p = Project::create(&PathBuf::from(parent_dir), &name)?;
    state.set_project(Some(p));
    if let Some(proj) = state.current_opt() {
        state.load_sessions(&proj);
    }
    let proj = state.current()?;
    let snap = checklist_ops::with_auto(proj.clone(), checklist_ops::snapshot(&proj));
    remember_recent(&state, &snap)?;
    Ok(snap)
}

#[tauri::command]
pub async fn project_open(state: State<'_, AppState>, root: String) -> Result<ProjectSnapshot> {
    let p = Project::open(&PathBuf::from(root))?;
    state.set_project(Some(p));
    if let Some(proj) = state.current_opt() {
        state.load_sessions(&proj);
    }
    let proj = state.current()?;
    let snap = checklist_ops::with_auto(proj.clone(), checklist_ops::snapshot(&proj));
    remember_recent(&state, &snap)?;
    Ok(snap)
}

#[tauri::command]
pub async fn project_close(state: State<'_, AppState>) -> Result<()> {
    state.set_project(None);
    Ok(())
}

#[tauri::command]
pub async fn project_snapshot(state: State<'_, AppState>) -> Result<ProjectSnapshot> {
    let p = state.current()?;
    Ok(checklist_ops::with_auto(p.clone(), checklist_ops::snapshot(&p)))
}

#[tauri::command]
pub async fn project_progress(state: State<'_, AppState>) -> Result<Vec<PanelProgress>> {
    let p = state.current()?;
    Ok(progress::all_progress(&checklist_ops::snapshot(&p)))
}

fn remember_recent(state: &AppState, snap: &ProjectSnapshot) -> Result<()> {
    let mut s = state.settings();
    s.recent_projects.retain(|r| r.path != snap.root);
    s.recent_projects.insert(
        0,
        RecentProject {
            path: snap.root.clone(),
            name: snap.manifest.name.clone(),
            id: snap.manifest.id.clone(),
            opened_at: now_iso(),
            exists: true,
        },
    );
    s.recent_projects.truncate(20);
    state.save_settings(&s)
}

#[tauri::command]
pub async fn project_update_manifest(
    state: State<'_, AppState>,
    patch: Value,
) -> Result<Manifest> {
    let m = state.mutate(|p| {
        if let Some(v) = patch.get("name").and_then(|x| x.as_str()) {
            p.manifest.name = v.into();
        }
        if let Some(v) = patch.get("genre").and_then(|x| x.as_str()) {
            p.manifest.genre = v.into();
        }
        if let Some(v) = patch.get("logline").and_then(|x| x.as_str()) {
            p.manifest.logline = v.into();
        }
        if let Some(v) = patch.get("episodeCountHint").and_then(|x| x.as_u64()) {
            p.manifest.episode_count_hint = v as u32;
        }
        if let Some(v) = patch.get("aspectRatio").and_then(|x| x.as_str()) {
            p.manifest.aspect_ratio = v.into();
            p.style.spec.aspect_ratio = v.into();
        }
        Ok(p.manifest.clone())
    })?;
    Ok(m)
}

/* ================================================================== 剧本 */

#[tauri::command]
pub async fn script_read_chapter(state: State<'_, AppState>, chapter_id: String) -> Result<Chapter> {
    state.current()?.load_chapter(&chapter_id)
}

#[tauri::command]
pub async fn script_save_chapter(
    state: State<'_, AppState>,
    chapter_id: String,
    content: String,
) -> Result<ChapterMeta> {
    state.mutate(|p| {
        let meta = p.save_chapter_content(&chapter_id, &content)?;
        if let Some(i) = p.script.chapters.iter_mut().find(|c| c.id == chapter_id) {
            *i = meta.clone();
        }
        Ok(meta)
    })
}

#[tauri::command]
pub async fn script_add_chapter(
    state: State<'_, AppState>,
    title: Option<String>,
    summary: Option<String>,
) -> Result<ChapterMeta> {
    state.mutate(|p| {
        let index = p.next_chapter_index();
        let meta = ChapterMeta {
            id: format!("ch_{index:03}"),
            index,
            title: title.unwrap_or_else(|| format!("第 {index} 章")),
            status: ChapterStatus::Empty,
            summary: summary.unwrap_or_default(),
            characters: vec![],
            scenes: vec![],
            word_count: 0,
            updated_at: now_iso(),
        };
        p.script.chapters.push(meta.clone());
        Ok(meta)
    })
}

#[tauri::command]
pub async fn script_update_chapter_meta(
    state: State<'_, AppState>,
    chapter_id: String,
    patch: Value,
) -> Result<ChapterMeta> {
    state.mutate(|p| {
        let c = p
            .script
            .chapters
            .iter_mut()
            .find(|c| c.id == chapter_id)
            .ok_or_else(|| AppError::NotFound(format!("章节不存在：{chapter_id}")))?;
        if let Some(v) = patch.get("title").and_then(|x| x.as_str()) {
            c.title = v.into();
        }
        if let Some(v) = patch.get("summary").and_then(|x| x.as_str()) {
            c.summary = v.into();
        }
        if let Some(v) = patch.get("status").and_then(|x| x.as_str()) {
            c.status = match v {
                "empty" => ChapterStatus::Empty,
                "draft" => ChapterStatus::Draft,
                "written" => ChapterStatus::Written,
                "locked" => ChapterStatus::Locked,
                _ => c.status,
            };
        }
        c.updated_at = now_iso();
        Ok(c.clone())
    })
}

#[tauri::command]
pub async fn script_delete_chapter(state: State<'_, AppState>, chapter_id: String) -> Result<()> {
    state.mutate(|p| {
        p.script.chapters.retain(|c| c.id != chapter_id);
        p.shots.retain(|s| s.chapter_id != chapter_id);
        let ids: std::collections::BTreeSet<String> = p.shots.iter().map(|s| s.id.clone()).collect();
        p.prompts.retain(|x| ids.contains(&x.shot_id));
        let _ = crate::store::remove_file_if_exists(&p.paths.chapter(&chapter_id));
        p.reindex_chapters();
        Ok(())
    })
}

#[tauri::command]
pub async fn script_reorder_chapters(state: State<'_, AppState>, ids: Vec<String>) -> Result<()> {
    state.mutate(|p| {
        let mut ordered: Vec<ChapterMeta> = vec![];
        for id in &ids {
            if let Some(c) = p.script.chapters.iter().find(|c| &c.id == id) {
                ordered.push(c.clone());
            }
        }
        for c in p.script.chapters.iter() {
            if !ids.contains(&c.id) {
                ordered.push(c.clone());
            }
        }
        p.script.chapters = ordered;
        p.reindex_chapters();
        Ok(())
    })
}

/* ================================================================== 圣经 */

#[tauri::command]
pub async fn bible_get(state: State<'_, AppState>) -> Result<Bible> {
    Ok(state.current()?.bible.clone())
}

#[tauri::command]
pub async fn bible_set(state: State<'_, AppState>, bible: Bible) -> Result<Bible> {
    state.mutate(|p| {
        p.bible = bible.clone();
        Ok(bible)
    })
}

/* ================================================================== 风格 */

#[tauri::command]
pub async fn style_get(state: State<'_, AppState>) -> Result<StyleState> {
    Ok(state.current()?.style.clone())
}

#[tauri::command]
pub async fn style_set(state: State<'_, AppState>, style: StyleState) -> Result<StyleState> {
    state.mutate(|p| {
        p.style = style.clone();
        if !style.spec.aspect_ratio.trim().is_empty() {
            p.manifest.aspect_ratio = style.spec.aspect_ratio.clone();
        }
        Ok(style)
    })
}

#[tauri::command]
pub async fn style_presets() -> Result<Vec<StylePreset>> {
    Ok(crate::presets::style_presets())
}

#[tauri::command]
pub async fn storyboard_vocab() -> Result<Value> {
    Ok(serde_json::json!({
        "shotSizes": crate::presets::shot_sizes(),
        "cameraMoves": crate::presets::camera_moves(),
        "timeOfDay": crate::presets::time_of_day(),
    }))
}

/* ================================================================== 分镜 */

#[tauri::command]
pub async fn storyboard_set_shots(
    state: State<'_, AppState>,
    chapter_id: String,
    shots: Vec<Shot>,
) -> Result<Vec<Shot>> {
    state.mutate(|p| {
        p.shots.retain(|s| s.chapter_id != chapter_id);
        let mut out = vec![];
        for (i, mut s) in shots.into_iter().enumerate() {
            if s.id.is_empty() {
                s.id = new_id("sh");
            }
            s.chapter_id = chapter_id.clone();
            s.index = i as u32 + 1;
            out.push(s);
        }
        p.shots.extend(out.clone());
        Ok(out)
    })
}

#[tauri::command]
pub async fn storyboard_upsert_shot(state: State<'_, AppState>, shot: Shot) -> Result<Shot> {
    state.mutate(|p| {
        let mut s = shot.clone();
        if s.id.is_empty() {
            s.id = new_id("sh");
        }
        s.chapter_id = if s.chapter_id.is_empty() {
            p.script
                .chapters
                .first()
                .map(|c| c.id.clone())
                .unwrap_or_default()
        } else {
            s.chapter_id.clone()
        };
        if s.index == 0 {
            s.index = p.next_shot_index(&s.chapter_id);
        }
        match p.shots.iter().position(|x| x.id == s.id) {
            Some(i) => p.shots[i] = s.clone(),
            None => p.shots.push(s.clone()),
        }
        Ok(s)
    })
}

#[tauri::command]
pub async fn storyboard_delete_shot(state: State<'_, AppState>, shot_id: String) -> Result<()> {
    state.mutate(|p| {
        p.shots.retain(|s| s.id != shot_id);
        p.prompts.retain(|x| x.shot_id != shot_id);
        Ok(())
    })
}

/* ================================================================== 资产 */

#[tauri::command]
pub async fn asset_upsert(state: State<'_, AppState>, asset: Asset) -> Result<Asset> {
    state.mutate(|p| {
        let mut a = asset.clone();
        if a.id.is_empty() {
            a.id = new_id("ast");
            a.created_at = now_iso();
        }
        a.updated_at = now_iso();
        match p.assets.iter().position(|x| x.id == a.id) {
            Some(i) => p.assets[i] = a.clone(),
            None => p.assets.push(a.clone()),
        }
        Ok(a)
    })
}

#[tauri::command]
pub async fn asset_delete(state: State<'_, AppState>, asset_id: String) -> Result<()> {
    state.mutate(|p| {
        p.assets.retain(|a| a.id != asset_id);
        p.prompts.iter_mut().for_each(|pr| {
            pr.refs.retain(|r| r.asset_id != asset_id);
            if pr.first_frame.as_ref().map(|r| &r.asset_id) == Some(&asset_id) {
                pr.first_frame = None;
            }
            if pr.last_frame.as_ref().map(|r| &r.asset_id) == Some(&asset_id) {
                pr.last_frame = None;
            }
        });
        Ok(())
    })
}

#[tauri::command]
pub async fn asset_plan_views(
    state: State<'_, AppState>,
    asset_id: String,
    views: Vec<AssetView>,
) -> Result<Asset> {
    state.mutate(|p| {
        let a = p
            .assets
            .iter_mut()
            .find(|a| a.id == asset_id)
            .ok_or_else(|| AppError::NotFound(format!("资产不存在：{asset_id}")))?;
        let mut planned: Vec<AssetView> = vec![];
        for mut v in views {
            // 已出图的旧视图保留文件
            if let Some(old) = a.views.iter().find(|x| x.id == v.id) {
                v.file = old.file.clone().or(v.file);
                v.thumb = old.thumb.clone().or(v.thumb);
                v.status = old.status;
                v.error = old.error.clone();
            }
            if v.id.is_empty() {
                v.id = new_id("vw");
            }
            planned.push(v);
        }
        a.views = planned;
        a.updated_at = now_iso();
        Ok(a.clone())
    })
}

#[tauri::command]
pub async fn asset_generate_views(
    state: State<'_, AppState>,
    asset_id: String,
    view_ids: Vec<String>,
    force: Option<bool>,
) -> Result<Vec<String>> {
    crate::actions::generate_asset_views(&state, &asset_id, view_ids, force.unwrap_or(false))
}

#[tauri::command]
pub async fn asset_view_delete(
    state: State<'_, AppState>,
    asset_id: String,
    view_id: String,
) -> Result<()> {
    state.mutate(|p| {
        if let Some(a) = p.assets.iter_mut().find(|a| a.id == asset_id) {
            a.views.retain(|v| v.id != view_id);
        }
        Ok(())
    })
}

/* ============================================================== 提示词 */

#[tauri::command]
pub async fn prompt_upsert(state: State<'_, AppState>, prompt: VideoPrompt) -> Result<VideoPrompt> {
    state.mutate(|p| {
        let mut item = prompt.clone();
        if item.id.is_empty() {
            item.id = new_id("vp");
        }
        item.updated_at = now_iso();
        match p.prompts.iter().position(|x| x.shot_id == item.shot_id) {
            Some(i) => p.prompts[i] = item.clone(),
            None => p.prompts.push(item.clone()),
        }
        Ok(item)
    })
}

#[tauri::command]
pub async fn prompt_delete(state: State<'_, AppState>, prompt_id: String) -> Result<()> {
    state.mutate(|p| {
        p.prompts.retain(|x| x.id != prompt_id);
        Ok(())
    })
}

#[tauri::command]
pub async fn prompt_autofill(
    state: State<'_, AppState>,
    chapter_id: Option<String>,
) -> Result<usize> {
    let ctx = agent::ToolCtx {
        state: state.inner().clone(),
        panel: PanelId::Prompt,
    };
    let mut args = serde_json::json!({});
    if let Some(c) = chapter_id {
        args["chapterId"] = serde_json::json!(c);
    }
    // 复用 agent 工具，保证行为一致
    let tools = agent::tools::registry(PanelId::Prompt);
    let t = tools
        .iter()
        .find(|t| t.spec.name == "prompt_autofill_from_shots")
        .ok_or_else(|| AppError::other("工具缺失"))?;
    let out = (t.run)(ctx, args).await?;
    Ok(out.data.get("created").and_then(|v| v.as_u64()).unwrap_or(0) as usize)
}

/* ============================================================== 生视频 */

#[tauri::command]
pub async fn video_generate(
    state: State<'_, AppState>,
    shot_ids: Vec<String>,
    provider_id: Option<String>,
    model: Option<String>,
) -> Result<Vec<String>> {
    crate::actions::generate_video_takes(&state, shot_ids, provider_id, model)
}

#[tauri::command]
pub async fn video_delete_take(state: State<'_, AppState>, take_id: String) -> Result<()> {
    state.mutate(|p| {
        if let Some(t) = p.takes.iter().find(|t| t.id == take_id) {
            if let Some(f) = &t.file {
                let _ = crate::store::remove_file_if_exists(std::path::Path::new(f));
            }
        }
        p.takes.retain(|t| t.id != take_id);
        Ok(())
    })
}

/* ================================================================ 剪辑 */

#[tauri::command]
pub async fn edit_set_timeline(state: State<'_, AppState>, timeline: Timeline) -> Result<Timeline> {
    state.mutate(|p| {
        p.timeline = timeline.clone();
        p.timeline.updated_at = now_iso();
        Ok(timeline)
    })
}

#[tauri::command]
pub async fn edit_build_from_takes(state: State<'_, AppState>) -> Result<usize> {
    crate::actions::build_timeline_from_takes(&state)
}

#[tauri::command]
pub async fn edit_render(
    state: State<'_, AppState>,
    burn_subtitles: Option<bool>,
) -> Result<String> {
    crate::actions::render_timeline_job(&state, burn_subtitles.unwrap_or(true))
}

/* ================================================================ 字幕 */

#[tauri::command]
pub async fn subtitle_transcribe(
    state: State<'_, AppState>,
    options: TranscribeOptions,
) -> Result<String> {
    crate::actions::transcribe_job(&state, options)
}

#[tauri::command]
pub async fn subtitle_get(state: State<'_, AppState>, subtitle_id: String) -> Result<SubtitleDoc> {
    state
        .current()?
        .subtitles
        .iter()
        .find(|s| s.id == subtitle_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("字幕不存在：{subtitle_id}")))
}

#[tauri::command]
pub async fn subtitle_save_cues(
    state: State<'_, AppState>,
    subtitle_id: String,
    cues: Vec<Cue>,
) -> Result<SubtitleDoc> {
    state.mutate(|p| {
        let doc = p
            .subtitles
            .iter_mut()
            .find(|s| s.id == subtitle_id)
            .ok_or_else(|| AppError::NotFound(format!("字幕不存在：{subtitle_id}")))?;
        doc.cues = cues.clone();
        if let Some(f) = doc.file.clone() {
            crate::store::write_text(std::path::Path::new(&f), &asr::to_srt(&cues))?;
            let vtt = std::path::Path::new(&f).with_extension("vtt");
            crate::store::write_text(&vtt, &asr::to_vtt(&cues))?;
        }
        Ok(doc.clone())
    })
}

#[tauri::command]
pub async fn subtitle_delete(state: State<'_, AppState>, subtitle_id: String) -> Result<()> {
    state.mutate(|p| {
        p.subtitles.retain(|s| s.id != subtitle_id);
        Ok(())
    })
}

/* ============================================================ checklist */

#[tauri::command]
pub async fn checklist_get(state: State<'_, AppState>) -> Result<Checklist> {
    let p = state.current()?;
    Ok(checklist_ops::with_auto(p.clone(), checklist_ops::snapshot(&p)).checklist)
}

#[tauri::command]
pub async fn checklist_add(
    state: State<'_, AppState>,
    panel: PanelId,
    text: String,
) -> Result<ChecklistItem> {
    state.mutate(|p| {
        let item = ChecklistItem {
            id: new_id("ck"),
            panel,
            text,
            done: false,
            done_by: None,
            done_at: None,
            note: String::new(),
            auto: false,
        };
        p.checklist.items.push(item.clone());
        Ok(item)
    })
}

#[tauri::command]
pub async fn checklist_toggle(
    state: State<'_, AppState>,
    id: String,
    done: bool,
) -> Result<()> {
    state.mutate(|p| {
        if let Some(i) = p.checklist.items.iter_mut().find(|i| i.id == id) {
            if i.auto {
                return Err(AppError::invalid("自动检查项由数据决定，不能手动改"));
            }
            i.done = done;
            i.done_by = Some(Actor::User);
            i.done_at = Some(now_iso());
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn checklist_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    state.mutate(|p| {
        p.checklist.items.retain(|i| i.id != id || i.auto);
        Ok(())
    })
}

#[tauri::command]
pub async fn checklist_reset_defaults(state: State<'_, AppState>) -> Result<Checklist> {
    state.mutate(|p| {
        let manual: Vec<ChecklistItem> = p.checklist.items.iter().filter(|i| !i.auto).cloned().collect();
        let mut items = progress::default_items();
        items.extend(manual);
        p.checklist.items = items;
        Ok(())
    })?;
    checklist_get(state).await
}

/* ================================================================ 任务 */

#[tauri::command]
pub async fn jobs_list(state: State<'_, AppState>) -> Result<Vec<Job>> {
    Ok(state.jobs.snapshot())
}

#[tauri::command]
pub async fn job_cancel(state: State<'_, AppState>, job_id: String) -> Result<bool> {
    Ok(state.jobs.cancel(&job_id))
}

#[tauri::command]
pub async fn jobs_clear_finished(state: State<'_, AppState>) -> Result<()> {
    state.jobs.clear_finished();
    Ok(())
}

/* ================================================================ 设置 */

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> Result<AppSettings> {
    Ok(state.settings())
}

#[tauri::command]
pub async fn settings_set(state: State<'_, AppState>, settings: AppSettings) -> Result<AppSettings> {
    state.save_settings(&settings)?;
    // 代理设置可能变了，丢掉缓存的 HTTP 客户端
    state.reset_http();
    Ok(settings)
}

/* ================================================================== 技能 */

#[tauri::command]
pub async fn skill_list(state: State<'_, AppState>) -> Result<Vec<crate::skills::Skill>> {
    crate::skills::ensure_defaults(&state)?;
    Ok(crate::skills::list(&state))
}

/// 读技能正文（渐进披露第二级）
#[tauri::command]
pub async fn skill_read(state: State<'_, AppState>, id: String) -> Result<String> {
    crate::skills::read_body(&state, &id)
}

/// 读技能附件（第三级）
#[tauri::command]
pub async fn skill_read_file(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<String> {
    crate::skills::read_file(&state, &id, &path)
}

#[tauri::command]
pub async fn skill_import(
    state: State<'_, AppState>,
    paths: Vec<String>,
    location: Option<String>,
) -> Result<Vec<String>> {
    let added =
        crate::skills::import(&state, &paths, location.as_deref().unwrap_or("user")).await?;
    // 热生效：前缀一重置，下一条消息就能用上新技能（不用重启）
    state.reset_agent_prefix();
    Ok(added)
}

#[tauri::command]
pub async fn skill_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    crate::skills::delete(&state, &id)?;
    state.reset_agent_prefix();
    Ok(())
}

#[tauri::command]
pub async fn skill_set_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<()> {
    crate::skills::set_enabled(&state, &id, enabled)?;
    state.reset_agent_prefix();
    Ok(())
}

#[tauri::command]
pub async fn skill_open_dir(state: State<'_, AppState>) -> Result<String> {
    let dir = crate::skills::skills_dir(&state);
    crate::store::ensure_dir(&dir)?;
    Ok(dir.to_string_lossy().to_string())
}

/* ============================================================== 网络诊断 */

#[tauri::command]
pub async fn proxy_status(state: State<'_, AppState>) -> Result<Value> {
    Ok(crate::net::diagnostics(&state.settings()))
}

/// 走当前代理设置访问一个地址，用来确认代理是否真的通了。
#[tauri::command]
pub async fn net_test(state: State<'_, AppState>, url: Option<String>) -> Result<String> {
    let url = url.unwrap_or_else(|| "https://huggingface.co/".into());
    let settings = state.settings();
    let client = crate::net::build_client(30, crate::net::resolve_proxy(&settings).as_deref())?;
    let started = std::time::Instant::now();
    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            let len = resp.content_length().unwrap_or(0);
            Ok(format!(
                "通：{} → HTTP {}（{} ms，{} 字节）｜代理：{}",
                url,
                status.as_u16(),
                started.elapsed().as_millis(),
                len,
                crate::net::resolve_proxy(&settings).unwrap_or_else(|| "直连".into())
            ))
        }
        Err(e) => Err(AppError::Provider(format!(
            "不通：{url}\n{e}\n当前代理：{}\n\
             如果这里是超时/连接被拒，去「设置 → 运行环境」检查代理模式；\
             v2rayN / Clash 只改系统代理时，选「跟随系统」即可。",
            crate::net::resolve_proxy(&settings).unwrap_or_else(|| "直连".into())
        ))),
    }
}

#[tauri::command]
pub async fn provider_upsert(
    state: State<'_, AppState>,
    provider: ProviderConfig,
) -> Result<ProviderConfig> {
    let mut s = state.settings();
    let mut p = provider.clone();
    if p.id.is_empty() {
        p.id = new_id("prov");
    }
    if p.api_key_ref.is_empty() {
        p.api_key_ref = format!("key_{}", p.id);
    }
    match s.providers.iter().position(|x| x.id == p.id) {
        Some(i) => s.providers[i] = p.clone(),
        None => s.providers.push(p.clone()),
    }
    state.save_settings(&s)?;
    Ok(p)
}

#[tauri::command]
pub async fn provider_delete(state: State<'_, AppState>, provider_id: String) -> Result<()> {
    let mut s = state.settings();
    s.providers.retain(|p| p.id != provider_id);
    if s.active_llm_provider_id.as_deref() == Some(&provider_id) {
        s.active_llm_provider_id = None;
    }
    if s.active_image_provider_id.as_deref() == Some(&provider_id) {
        s.active_image_provider_id = None;
    }
    if s.active_video_provider_id.as_deref() == Some(&provider_id) {
        s.active_video_provider_id = None;
    }
    state.save_settings(&s)?;
    Ok(())
}

#[tauri::command]
pub async fn provider_set_secret(
    state: State<'_, AppState>,
    key_ref: String,
    value: String,
) -> Result<()> {
    state.config.set_secret(&key_ref, &value)
}

#[tauri::command]
pub async fn secrets_status(state: State<'_, AppState>) -> Result<std::collections::BTreeMap<String, bool>> {
    Ok(state.config.secret_status())
}

/// 测试一家 provider 是否可用。LLM 发一条最小请求；生图/生视频按次计费，
/// 只走 gen 工厂做配置校验，不发起真实生成。
#[tauri::command]
pub async fn provider_test(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<String> {
    let cfg = state
        .provider(&provider_id)
        .ok_or_else(|| AppError::NotFound(format!("provider 不存在：{provider_id}")))?;
    let key = state.api_key_for(&cfg);

    if matches!(cfg.kind, ProviderKind::Image | ProviderKind::Video) {
        let provider = crate::gen::build_provider(&state, &cfg, key.clone())?;
        let key_state = if key.is_some() {
            "密钥已配置"
        } else {
            "未配置密钥（接口需要鉴权时生成会失败）"
        };
        return Ok(format!(
            "配置校验通过：{} 适配器，模型 {}，{}。生图/生视频按次计费，未发送真实请求。",
            provider.adapter(),
            provider.model(),
            key_state
        ));
    }

    if matches!(cfg.adapter.as_str(), "openai" | "anthropic" | "openai-compatible" | "claude")
        && key.is_none()
    {
        return Err(AppError::Config("这家 provider 还没有填 API Key".into()));
    }
    let http = state.http_client()?;
    let handle = crate::llm::build_provider(&cfg, key, http)?;
    let req = crate::llm::ChatRequest {
        model: handle.provider.model(),
        system: vec![crate::llm::SystemBlock {
            name: "test".into(),
            text: "你是连通性测试。只回复两个字：可用".into(),
            cache_breakpoint: false,
        }],
        messages: vec![crate::llm::Message::user_text("测试连接")],
        tools: vec![],
        max_tokens: 32,
        temperature: 0.0,
        cache_tools: false,
        cache_conversation: false,
    };
    let out = handle.provider.complete(req).await?;
    Ok(format!(
        "连接正常，返回：{}（输入 {} tokens，输出 {}）",
        crate::llm::truncate(out.text.trim(), 80),
        out.usage.input_tokens + out.usage.cache_read_tokens,
        out.usage.output_tokens
    ))
}

/* ============================================================ 3D预演 */

/// 预演场景（对象 / 角色 / 机位）是前端自治的 JSON，后端只负责落盘。
#[tauri::command]
pub async fn previz_get(state: State<'_, AppState>) -> Result<Option<Value>> {
    let path = state.current()?.paths.workbench().join("previz.json");
    crate::store::read_json_opt(&path)
}

#[tauri::command]
pub async fn previz_put(state: State<'_, AppState>, scene: Value) -> Result<()> {
    let path = state.current()?.paths.workbench().join("previz.json");
    crate::store::write_json(&path, &scene)
}

/* ------------------------------------------- 白模参考视频（逐帧 → ffmpeg） */

/// 开始一帧序列渲染：清空旧帧目录，返回目录路径。
#[tauri::command]
pub fn previz_render_begin(state: State<'_, AppState>) -> Result<String> {
    let dir = state.current()?.paths.workbench().join("previz-frames");
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    crate::store::ensure_dir(&dir)?;
    Ok(dir.to_string_lossy().to_string())
}

/// 收一帧（前端 canvas 导出的 JPEG dataURL）。
#[tauri::command]
pub fn previz_render_frame(
    state: State<'_, AppState>,
    index: u32,
    data_b64: String,
) -> Result<()> {
    use base64::Engine as _;
    let raw = data_b64
        .split_once(',')
        .map(|(_, b)| b)
        .unwrap_or(data_b64.as_str());
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|e| AppError::invalid(format!("帧数据解码失败：{e}")))?;
    let dir = state.current()?.paths.workbench().join("previz-frames");
    crate::store::write_bytes_atomic(&dir.join(format!("frame_{index:05}.jpg")), &bytes)
}

/// 把帧序列交给 ffmpeg 合成 mp4，输出到项目 previz/ 目录。
#[tauri::command]
pub async fn previz_render_finish(state: State<'_, AppState>, fps: u32) -> Result<String> {
    let proj = state.current()?;
    let frames = proj.paths.workbench().join("previz-frames");
    let out_dir = proj.root().join("previz");
    crate::store::ensure_dir(&out_dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let out = out_dir.join(format!("previz-{stamp}.mp4"));
    let ffmpeg = crate::media::resolve_tool(&state, "ffmpeg", "")?;
    let args: Vec<String> = vec![
        "-y".into(),
        "-framerate".into(),
        fps.max(1).to_string(),
        "-i".into(),
        frames.join("frame_%05d.jpg").to_string_lossy().to_string(),
        "-c:v".into(),
        "libx264".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-crf".into(),
        "18".into(),
        "-movflags".into(),
        "+faststart".into(),
        out.to_string_lossy().to_string(),
    ];
    crate::media::run(&ffmpeg, &args).await?;
    // 清掉帧序列（几百 MB，不留着）
    let _ = std::fs::remove_dir_all(&frames);
    Ok(out.to_string_lossy().to_string())
}

/* ============================================================ 生成预览 */

/// 预览某个资产视图最终发给模型的完整提示词（含风格前缀 / 固定特征）。
#[tauri::command]
pub fn asset_prompt_preview(
    state: State<'_, AppState>,
    asset_id: String,
    view_id: String,
) -> Result<Value> {
    let p = state.current()?;
    let asset = p
        .assets
        .iter()
        .find(|a| a.id == asset_id)
        .ok_or_else(|| AppError::NotFound("资产不存在".into()))?;
    let view = asset
        .views
        .iter()
        .find(|v| v.id == view_id)
        .ok_or_else(|| AppError::NotFound("视图不存在".into()))?;
    Ok(serde_json::json!({
        "prompt": crate::actions::compose_image_prompt(&p, asset, view),
        "negative": crate::actions::compose_image_negative(&p, view),
    }))
}

/// 预览某个镜头视频生成时最终使用的完整提示词（含风格前缀）。
#[tauri::command]
pub fn video_prompt_preview(state: State<'_, AppState>, prompt_id: String) -> Result<Value> {
    let p = state.current()?;
    let pr = p
        .prompts
        .iter()
        .find(|x| x.id == prompt_id)
        .ok_or_else(|| AppError::NotFound("提示词不存在".into()))?;
    let shoot = p.shots.iter().find(|s| s.id == pr.shot_id);
    let raw = if pr.prompt.trim().is_empty() {
        shoot
            .map(|s| format!("{}，{}，{}", s.action, s.shot_size, s.camera_move))
            .unwrap_or_default()
    } else {
        pr.prompt.clone()
    };
    Ok(serde_json::json!({
        "prompt": crate::actions::compose_video_prompt(&p, &raw),
        "negative": crate::actions::compose_video_negative(&p, &pr.negative),
    }))
}

/* ============================================================ 媒体/ASR */

#[tauri::command]
pub async fn media_probe(state: State<'_, AppState>, path: String) -> Result<crate::media::MediaInfo> {
    crate::media::probe(&state, std::path::Path::new(&path)).await
}

#[tauri::command]
pub async fn media_sidecar_status(state: State<'_, AppState>) -> Result<Value> {
    Ok(crate::media::sidecar_status(&state))
}

#[tauri::command]
pub async fn asr_capabilities(state: State<'_, AppState>) -> Result<AsrCapabilities> {
    Ok(asr::capabilities(&state))
}

#[tauri::command]
pub async fn asr_download_model(state: State<'_, AppState>, model_id: String) -> Result<String> {
    let st = state.inner().clone();
    let id = model_id.clone();
    let job = state.jobs.submit(
        JobKind::Download,
        format!("下载 whisper 模型 · {model_id}"),
        "排队中",
        move |ctx| {
            let st = st.clone();
            let id = id.clone();
            async move {
                let info: WhisperModelInfo = asr::download_model(&st, &ctx, &id).await?;
                Ok(format!("模型已就绪：{}", info.path.unwrap_or_default()))
            }
        },
    );
    Ok(job.id)
}

#[tauri::command]
pub async fn asr_delete_model(state: State<'_, AppState>, model_id: String) -> Result<()> {
    asr::delete_model(&state, &model_id)
}

/* ================================================================ Agent */

#[tauri::command]
pub async fn agent_sessions(state: State<'_, AppState>) -> Result<Vec<AgentSession>> {
    let p = state.current()?;
    Ok(state.sessions_of(&p.manifest.id))
}

#[tauri::command]
pub async fn agent_session_get(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<AgentSession>> {
    Ok(state.session(&session_id))
}

#[tauri::command]
pub async fn agent_session_delete(state: State<'_, AppState>, session_id: String) -> Result<()> {
    state.agent.sessions.write().remove(&session_id);
    state.persist_sessions();
    Ok(())
}

#[tauri::command]
pub async fn agent_session_new(
    state: State<'_, AppState>,
    panel: PanelId,
) -> Result<AgentSession> {
    let p = state.current()?;
    let s = AgentSession::new(
        &p.manifest.id,
        panel,
        agent::prompt::build_frozen_prefix(&p, panel, crate::skills::prompt_index(&state))?,
    );
    state.put_session(s.clone());
    Ok(s)
}

#[tauri::command]
pub async fn agent_run(
    state: State<'_, AppState>,
    channel: Channel<AgentEvent>,
    panel: PanelId,
    session_id: Option<String>,
    input: String,
    image_paths: Option<Vec<String>>,
    refresh_context: Option<bool>,
) -> Result<String> {
    let st = state.inner().clone();
    let sid = agent::run_turn(
        st,
        channel,
        agent::RunOptions {
            panel,
            session_id,
            input,
            image_paths: image_paths.unwrap_or_default(),
            refresh_context: refresh_context.unwrap_or(false),
        },
    )
    .await?;
    Ok(sid)
}

/* ============================================================ 导演台 */

/// 保存导演台工程到项目目录（前端从 iframe 取 JSON 后送来）。
#[tauri::command]
pub fn director_save(state: State<'_, AppState>, content: String) -> Result<String> {
    let path = state
        .current()?
        .root()
        .join("previz")
        .join("director.json");
    crate::store::write_text(&path, &content)?;
    Ok(path.to_string_lossy().to_string())
}

/// 读取上次保存的导演台工程（没有则 null）。
#[tauri::command]
pub fn director_load(state: State<'_, AppState>) -> Result<Option<String>> {
    let path = state
        .current()?
        .root()
        .join("previz")
        .join("director.json");
    crate::store::read_text_opt(&path)
}

/// 前端回填导演台工具的执行结果（agent 工具在等它）。
#[tauri::command]
pub fn director_result(
    state: State<'_, AppState>,
    call_id: String,
    ok: bool,
    data: Option<Value>,
    error: Option<String>,
) -> Result<()> {
    crate::director::resolve_call(&state, &call_id, ok, data.unwrap_or(Value::Null), error);
    Ok(())
}

/* ============================================================ pi 引擎 */

/// pi 引擎可用性（是否找到 pi、有没有可用模型、进程是否在跑）
#[tauri::command]
pub async fn agent_pi_status(state: State<'_, AppState>) -> Result<crate::pi::PiStatus> {
    Ok(crate::pi::status(state.inner()).await)
}

/// 用 pi 引擎跑一轮对话；事件流复用 AgentEvent，前端渲染逻辑不变。
#[tauri::command]
pub async fn agent_pi_run(
    state: State<'_, AppState>,
    channel: Channel<AgentEvent>,
    input: String,
    image_paths: Option<Vec<String>>,
) -> Result<String> {
    let st = state.inner().clone();
    crate::pi::run_turn(&st, channel, input, image_paths.unwrap_or_default()).await
}

/// 中止 pi 当前回合
#[tauri::command]
pub async fn agent_pi_abort(state: State<'_, AppState>) -> Result<()> {
    state.pi.abort().await;
    Ok(())
}

/// 停止自研引擎的当前回合：在下一个轮次/工具边界生效（不打断正在进行的 HTTP 请求）
#[tauri::command]
pub fn agent_abort(state: State<'_, AppState>, session_id: String) -> Result<()> {
    state.agent.abort_requests.lock().insert(session_id);
    Ok(())
}

/// pi 会话历史（浮窗切到 pi 引擎时拉取渲染）
#[tauri::command]
pub async fn agent_pi_history(state: State<'_, AppState>) -> Result<Vec<Value>> {
    let st = state.inner().clone();
    crate::pi::history(&st).await
}

/// 设置 pi 的思考强度（off / minimal / low / medium / high）
#[tauri::command]
pub async fn agent_pi_thinking(state: State<'_, AppState>, level: String) -> Result<()> {
    let st = state.inner().clone();
    crate::pi::set_thinking(&st, &level).await
}

/// 当前会话的上下文水位
#[tauri::command]
pub async fn agent_context(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<agent::context::ContextStats> {
    let s = state
        .session(&session_id)
        .ok_or_else(|| AppError::NotFound("会话不存在".into()))?;
    Ok(agent::context::stats(&state, &s))
}

/// 手动压缩上下文。`summarize` 为真时直接走模型摘要（会花一次调用）。
#[tauri::command]
pub async fn agent_compact(
    state: State<'_, AppState>,
    session_id: String,
    summarize: Option<bool>,
) -> Result<agent::context::ContextStats> {
    let st = state.inner().clone();
    let provider = agent::resolve_provider(&state)?;
    let stats = agent::context::compact(&st, &provider, &session_id, summarize.unwrap_or(false)).await?;
    let _ = state.emit_session_changed(&session_id);
    Ok(stats)
}

#[tauri::command]
pub async fn agent_answer(
    state: State<'_, AppState>,
    tool_call_id: String,
    answer: String,
) -> Result<bool> {
    Ok(agent::resolve_answer(&state, &tool_call_id, &answer))
}

#[tauri::command]
pub async fn agent_approve(
    state: State<'_, AppState>,
    tool_call_id: String,
    approved: bool,
) -> Result<bool> {
    Ok(agent::resolve_approval(&state, &tool_call_id, approved))
}

/// 让前端能预先看到当前会话的前缀分层，用于「缓存」面板展示。
#[tauri::command]
pub async fn agent_prefix_preview(
    state: State<'_, AppState>,
    panel: PanelId,
    session_id: Option<String>,
) -> Result<agent::prompt::PrefixReport> {
    let p = state.current()?;
    let live = agent::prompt::build_frozen_prefix(&p, panel, crate::skills::prompt_index(&state))?;
    let stable = session_id
        .as_deref()
        .and_then(|id| state.session(id))
        .and_then(|s| s.prefix.map(|x| x.fingerprint))
        .map(|fp| fp == live.fingerprint)
        .unwrap_or(false);
    Ok(live.report(stable))
}

/// 前端用来构造项目路径（打开文件夹等）。
#[tauri::command]
pub async fn project_paths(_state: State<'_, AppState>, root: String) -> Result<Value> {
    let pp = ProjectPaths::new(PathBuf::from(root));
    Ok(serde_json::json!({
        "root": pp.root.to_string_lossy(),
        "manifest": pp.manifest().to_string_lossy(),
        "assets": pp.asset_dir("").to_string_lossy(),
        "videos": pp.video_dir().to_string_lossy(),
        "renders": pp.render_dir().to_string_lossy(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct _Placeholder;
