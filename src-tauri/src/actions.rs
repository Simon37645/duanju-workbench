//! 高层业务动作。前端命令与 agent 工具都调用这里，保证「用户手点」和「agent 自动做」
//! 走完全相同的逻辑与副作用。

use std::path::PathBuf;
use std::sync::Arc;

use crate::asr::{self, TranscribeOptions};
use crate::error::{AppError, Result};
use crate::gen::{self, GenProvider, GenRequest};
use crate::jobs::JobQueue;
use crate::models::*;
use crate::project::Project;
use crate::state::AppState;

/* ============================================================ 通用工具 */

pub fn aspect_to_size(ratio: &str, base: u32) -> (u32, u32) {
    let parts: Vec<&str> = ratio.split([':', 'x', '/']).collect();
    if parts.len() == 2 {
        if let (Ok(a), Ok(b)) = (
            parts[0].trim().parse::<f32>(),
            parts[1].trim().parse::<f32>(),
        ) {
            if a > 0.0 && b > 0.0 {
                return if a >= b {
                    (base, (base as f32 * b / a).round() as u32)
                } else {
                    ((base as f32 * a / b).round() as u32, base)
                };
            }
        }
    }
    (1024, 1024)
}

/// 把风格圣经 + 资产特征 + 视图提示词拼成最终提示词。
pub fn compose_image_prompt(p: &Project, asset: &Asset, view: &AssetView) -> String {
    let mut parts: Vec<String> = vec![];
    if !p.style.spec.prompt.trim().is_empty() {
        parts.push(p.style.spec.prompt.trim().to_string());
    }
    if !view.prompt.trim().is_empty() {
        parts.push(view.prompt.trim().to_string());
    } else {
        parts.push(format!(
            "{} {}，{}",
            asset.name,
            crate::agent::prompt::view_kind_label(view.kind),
            asset.description
        ));
    }
    if !asset.locked_traits.is_empty() {
        parts.push(format!("固定特征：{}", asset.locked_traits.join("、")));
    }
    if !asset.description.trim().is_empty() && !parts.iter().any(|x| x.contains(&asset.description)) {
        parts.push(asset.description.trim().to_string());
    }
    parts.join("，")
}

pub fn compose_image_negative(p: &Project, view: &AssetView) -> String {
    let mut v: Vec<String> = vec![];
    if !p.style.spec.negative.trim().is_empty() {
        v.push(p.style.spec.negative.trim().to_string());
    }
    if !view.negative.trim().is_empty() {
        v.push(view.negative.trim().to_string());
    }
    v.join("，")
}

/// 视频生成的最终提示词：与生图同样的规则，风格圣经拼在最前，保证资产图与视频风格统一。
pub fn compose_video_prompt(p: &Project, text: &str) -> String {
    let mut parts: Vec<String> = vec![];
    if !p.style.spec.prompt.trim().is_empty() {
        parts.push(p.style.spec.prompt.trim().to_string());
    }
    if !text.trim().is_empty() {
        parts.push(text.trim().to_string());
    }
    parts.join("，")
}

/// 视频生成的负面词：风格 negative + 提示词负面词
pub fn compose_video_negative(p: &Project, text: &str) -> String {
    let mut v: Vec<String> = vec![];
    if !p.style.spec.negative.trim().is_empty() {
        v.push(p.style.spec.negative.trim().to_string());
    }
    if !text.trim().is_empty() {
        v.push(text.trim().to_string());
    }
    v.join("，")
}

/// 找一张可用的参考图用于保持一致性（优先正面图）。
fn consistency_ref(asset: &Asset, exclude_view: &str) -> Option<PathBuf> {
    let order = [
        ViewKind::Front,
        ViewKind::FullBody,
        ViewKind::ThreeQuarter,
        ViewKind::Wide,
    ];
    for k in order {
        if let Some(v) = asset
            .views
            .iter()
            .find(|v| v.kind == k && v.id != exclude_view && v.status == AssetStatus::Done)
        {
            if let Some(f) = &v.file {
                let path = PathBuf::from(f);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }
    asset
        .views
        .iter()
        .filter(|v| v.id != exclude_view && v.status == AssetStatus::Done)
        .find_map(|v| v.file.as_ref().map(PathBuf::from).filter(|x| x.is_file()))
}

/* ============================================================ 生图 */

/// 提交资产视图的生图任务，返回 jobId 列表。
pub fn generate_asset_views(
    state: &AppState,
    asset_id: &str,
    view_ids: Vec<String>,
    force: bool,
) -> Result<Vec<String>> {
    let p = state.current()?;
    let asset = p
        .assets
        .iter()
        .find(|a| a.id == asset_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("资产不存在：{asset_id}")))?;

    let targets: Vec<AssetView> = asset
        .views
        .iter()
        .filter(|v| view_ids.is_empty() || view_ids.contains(&v.id))
        .filter(|v| force || !matches!(v.status, AssetStatus::Done | AssetStatus::Running | AssetStatus::Queued))
        .cloned()
        .collect();

    if targets.is_empty() {
        return Err(AppError::invalid("没有需要生成的视图（可能都已出图，可勾选「重新生成」）"));
    }

    let cfg = state
        .active_provider(ProviderKind::Image)
        .ok_or_else(|| AppError::Config("还没有启用任何生图 provider，请先在设置里添加".into()))?;
    let key = state.api_key_for(&cfg);
    let provider = gen::build_provider(state, &cfg, key)?;
    let (w, h) = aspect_to_size(&p.style.spec.aspect_ratio, 1024);

    let mut job_ids = vec![];
    for view in targets {
        let prompt = compose_image_prompt(&p, &asset, &view);
        let negative = compose_image_negative(&p, &view);
        let mut refs: Vec<PathBuf> = consistency_ref(&asset, &view.id).into_iter().collect();
        // 手动参考图（本地文件 / 已生成资产图）一并作为参考
        for path_str in &view.ref_images {
            let path = PathBuf::from(path_str);
            if path.is_file() && !refs.contains(&path) {
                refs.push(path);
            }
        }
        let dst = dst_for_view(&p, &asset.id, &view.id);
        let job_id = enqueue_image(
            state,
            &provider,
            cfg.id.clone(),
            cfg.model.clone(),
            asset.id.clone(),
            view.id.clone(),
            GenRequest {
                prompt,
                negative,
                width: w,
                height: h,
                seed: view.seed,
                ref_images: refs,
                duration_sec: None,
                extra: serde_json::json!({}),
            },
            dst,
        );
        job_ids.push(job_id);
    }
    Ok(job_ids)
}

fn dst_for_view(p: &Project, asset_id: &str, view_id: &str) -> PathBuf {
    p.paths.asset_file(asset_id, view_id, "png")
}

#[allow(clippy::too_many_arguments)]
fn enqueue_image(
    state: &AppState,
    provider: &Arc<dyn GenProvider>,
    provider_id: String,
    model: String,
    asset_id: String,
    view_id: String,
    req: GenRequest,
    dst: PathBuf,
) -> String {
    let st = state.clone();
    let provider = provider.clone();
    let queue: JobQueue = state.jobs.clone();
    let title = format!(
        "生成图片 · {}",
        dst.file_name()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_default()
    );
    let job = queue.submit(JobKind::Image, title, "排队中", move |ctx| {
        let st = st.clone();
        let provider = provider.clone();
        let asset_id = asset_id.clone();
        let view_id = view_id.clone();
        let dst = dst.clone();
        let provider_id = provider_id.clone();
        let model = model.clone();
        async move {
            set_view_status(&st, &asset_id, &view_id, AssetStatus::Running, None, Some(ctx.id.clone()));
            ctx.progress(0.05, "调用生图接口…");
            match provider.generate(&ctx, req).await {
                Ok(out) => {
                    let path = gen::persist_output(out, &dst).await?;
                    let thumb = make_thumb_for(&st, &path, &format!("{asset_id}_{view_id}")).await;
                    set_view_done(&st, &asset_id, &view_id, &path, thumb, &provider_id, &model)?;
                    Ok(format!("已生成 {}", path.display()))
                }
                Err(e) => {
                    set_view_status(&st, &asset_id, &view_id, AssetStatus::Failed, Some(e.to_string()), None);
                    Err(e)
                }
            }
        }
    });
    job.id
}

async fn make_thumb_for(state: &AppState, src: &PathBuf, key: &str) -> Option<String> {
    let p = state.current_opt()?;
    let dst = p.paths.thumbs_dir().join(format!("{key}.jpg"));
    match crate::media::make_thumb(state, src, &dst, 0.0).await {
        Ok(_) => Some(dst.to_string_lossy().to_string()),
        Err(_) => None,
    }
}

fn set_view_status(
    state: &AppState,
    asset_id: &str,
    view_id: &str,
    status: AssetStatus,
    error: Option<String>,
    job_id: Option<String>,
) {
    let _ = state.mutate(|p| {
        if let Some(a) = p.assets.iter_mut().find(|a| a.id == asset_id) {
            if let Some(v) = a.views.iter_mut().find(|v| v.id == view_id) {
                v.status = status;
                v.error = error;
                if job_id.is_some() {
                    v.job_id = job_id;
                }
            }
            a.updated_at = now_iso();
        }
        Ok(())
    });
}

fn set_view_done(
    state: &AppState,
    asset_id: &str,
    view_id: &str,
    file: &std::path::Path,
    thumb: Option<String>,
    provider_id: &str,
    model: &str,
) -> Result<()> {
    state.mutate(|p| {
        if let Some(a) = p.assets.iter_mut().find(|a| a.id == asset_id) {
            if let Some(v) = a.views.iter_mut().find(|v| v.id == view_id) {
                v.status = AssetStatus::Done;
                v.file = Some(file.to_string_lossy().to_string());
                v.thumb = thumb.clone();
                v.provider_id = Some(provider_id.to_string());
                v.model = Some(model.to_string());
                v.error = None;
            }
            a.updated_at = now_iso();
        }
        Ok(())
    })
}

/* ============================================================ 生视频 */

pub fn generate_video_takes(
    state: &AppState,
    shot_ids: Vec<String>,
    provider_id: Option<String>,
    model: Option<String>,
) -> Result<Vec<String>> {
    let p = state.current()?;
    let cfg = match provider_id.as_deref() {
        Some(id) => state
            .provider(id)
            .ok_or_else(|| AppError::NotFound(format!("provider 不存在：{id}")))?,
        None => state
            .active_provider(ProviderKind::Video)
            .ok_or_else(|| AppError::Config("还没有启用任何视频 provider，请先在设置里添加".into()))?,
    };
    let key = state.api_key_for(&cfg);
    let provider = gen::build_provider(state, &cfg, key)?;
    let model = model.unwrap_or_else(|| cfg.model.clone());

    let mut job_ids = vec![];
    for shot_id in &shot_ids {
        let shot = p
            .shots
            .iter()
            .find(|s| s.id == *shot_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("镜头不存在：{shot_id}")))?;
        let prompt = p.prompts.iter().find(|x| x.shot_id == *shot_id).cloned();

        let take_id = new_id("take");
        let dst = p.paths.video_dir().join(format!("{take_id}.mp4"));
        let (pw, ph) = aspect_to_size(&p.style.spec.aspect_ratio, 1080);

        // 参考图：首帧 → 尾帧 → 其它引用
        let mut refs: Vec<PathBuf> = vec![];
        let mut collect = |r: &Option<AssetRef>| {
            if let Some(r) = r {
                if let Some(path) = resolve_ref(&p, r) {
                    refs.push(path);
                }
            }
        };
        if let Some(pr) = &prompt {
            collect(&pr.first_frame);
            collect(&pr.last_frame);
            for r in &pr.refs {
                if let Some(path) = resolve_ref(&p, r) {
                    refs.push(path);
                }
            }
            // 手动参考图（本地文件）
            for path_str in &pr.ref_images {
                let path = PathBuf::from(path_str);
                if path.is_file() && !refs.contains(&path) {
                    refs.push(path);
                }
            }
        }

        let raw_text = prompt
            .as_ref()
            .map(|x| x.prompt.clone())
            .filter(|x| !x.trim().is_empty())
            .unwrap_or_else(|| {
                format!(
                    "{}，{}，{}",
                    shot.action, shot.shot_size, shot.camera_move
                )
            });
        let raw_negative = prompt.as_ref().map(|x| x.negative.clone()).unwrap_or_default();
        // 风格圣经作为前缀拼进来，与生图规则一致
        let text = compose_video_prompt(&p, &raw_text);
        let negative = compose_video_negative(&p, &raw_negative);
        let duration = prompt
            .as_ref()
            .map(|x| x.duration_sec)
            .filter(|d| *d > 0.0)
            .unwrap_or(shot.duration_sec);
        let seed = prompt.as_ref().and_then(|x| x.seed);

        let take = VideoTake {
            id: take_id.clone(),
            shot_id: shot.id.clone(),
            prompt_id: prompt.as_ref().map(|x| x.id.clone()),
            provider_id: cfg.id.clone(),
            model: model.clone(),
            status: AssetStatus::Queued,
            progress: 0.0,
            file: None,
            thumb: None,
            seed,
            duration_sec: None,
            error: None,
            job_id: None,
            created_at: now_iso(),
        };
        let take_id2 = take_id.clone();
        state.mutate(|p| {
            p.takes.push(take);
            Ok(())
        })?;

        let st = state.clone();
        let provider = provider.clone();
        let (pid, mdl) = (cfg.id.clone(), model.clone());
        let job = state.jobs.submit(
            JobKind::Video,
            format!("生成视频 · 镜头 {}", shot.index),
            "排队中",
            move |ctx| {
                let st = st.clone();
                let provider = provider.clone();
                let take_id = take_id2.clone();
                let dst = dst.clone();
                let pid = pid.clone();
                let mdl = mdl.clone();
                let req = GenRequest {
                    prompt: text.clone(),
                    negative: negative.clone(),
                    width: pw,
                    height: ph,
                    seed,
                    ref_images: refs.clone(),
                    duration_sec: Some(duration.max(1.0)),
                    extra: serde_json::json!({
                        "cameraMove": prompt.as_ref().map(|x| x.camera_move.clone()).unwrap_or_default(),
                    }),
                };
                async move {
                    update_take(&st, &take_id, |t| {
                        t.status = AssetStatus::Running;
                        t.job_id = Some(ctx.id.clone());
                    });
                    ctx.progress(0.05, "提交视频生成任务…");
                    match provider.generate(&ctx, req).await {
                        Ok(out) => {
                            let path = gen::persist_output(out, &dst).await?;
                            let thumb = make_thumb_for(&st, &path, &take_id).await;
                            let dur = crate::media::probe(&st, &path)
                                .await
                                .ok()
                                .map(|i| i.duration_sec);
                            update_take(&st, &take_id, |t| {
                                t.status = AssetStatus::Done;
                                t.progress = 1.0;
                                t.file = Some(path.to_string_lossy().to_string());
                                t.thumb = thumb.clone();
                                t.duration_sec = dur;
                                t.provider_id = pid.clone();
                                t.model = mdl.clone();
                                t.error = None;
                            });
                            Ok(format!("已生成 {}", path.display()))
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            update_take(&st, &take_id, |t| {
                                t.status = AssetStatus::Failed;
                                t.error = Some(msg.clone());
                            });
                            Err(e)
                        }
                    }
                }
            },
        );
        job_ids.push(job.id.clone());
        let job_id = job.id.clone();
        state.mutate_quiet(|p| {
            if let Some(t) = p.takes.iter_mut().find(|t| t.id == take_id) {
                t.job_id = Some(job_id);
            }
            Ok(())
        })?;
    }
    state.emit_changed();
    Ok(job_ids)
}

fn resolve_ref(p: &Project, r: &AssetRef) -> Option<PathBuf> {
    let a = p.assets.iter().find(|a| a.id == r.asset_id)?;
    let v = match &r.view_id {
        Some(id) => a.views.iter().find(|v| v.id == *id),
        None => a.views.iter().find(|v| v.status == AssetStatus::Done),
    }?;
    v.file.as_ref().map(PathBuf::from).filter(|x| x.is_file())
}

fn update_take(state: &AppState, take_id: &str, f: impl FnOnce(&mut VideoTake)) {
    let _ = state.mutate(|p| {
        if let Some(t) = p.takes.iter_mut().find(|t| t.id == take_id) {
            f(t);
        }
        Ok(())
    });
}

/* ============================================================ 时间线 */

/// 按「章节顺序 + 镜号」把所有已生成的视频铺到主视频轨。
pub fn build_timeline_from_takes(state: &AppState) -> Result<usize> {
    state.mutate(|p| {
        let mut ordered: Vec<&VideoTake> = p
            .takes
            .iter()
            .filter(|t| t.status == AssetStatus::Done && t.file.is_some())
            .collect();
        ordered.sort_by_key(|t| {
            let shot = p.shots.iter().find(|s| s.id == t.shot_id);
            let chapter = shot.and_then(|s| {
                p.script
                    .chapters
                    .iter()
                    .find(|c| c.id == s.chapter_id)
                    .map(|c| c.index)
            });
            (
                chapter.unwrap_or(0),
                shot.map(|s| s.index).unwrap_or(0),
                t.created_at.clone(),
            )
        });
        // 同一镜头有多条 take 时只取最新一条
        let mut seen = std::collections::BTreeSet::new();
        let picks: Vec<VideoTake> = ordered
            .into_iter()
            .rev()
            .filter(|t| seen.insert(t.shot_id.clone()))
            .map(|t| t.clone())
            .collect();
        let picks: Vec<VideoTake> = picks.into_iter().rev().collect();

        let video_track = p
            .timeline
            .tracks
            .iter_mut()
            .find(|t| matches!(t.kind, TrackKind::Video))
            .ok_or_else(|| AppError::other("时间线缺少视频轨"))?;
        video_track.clips.clear();

        let mut cursor = 0.0f32;
        for t in &picks {
            let dur = t.duration_sec.unwrap_or(3.0).max(0.2);
            video_track.clips.push(Clip {
                id: new_id("clip"),
                source: t.file.clone().unwrap_or_default(),
                shot_id: Some(t.shot_id.clone()),
                take_id: Some(t.id.clone()),
                label: p
                    .shots
                    .iter()
                    .find(|s| s.id == t.shot_id)
                    .map(|s| format!("{} · {}", s.index, s.shot_size))
                    .unwrap_or_else(|| t.shot_id.clone()),
                in_sec: 0.0,
                out_sec: dur,
                start_sec: cursor,
                speed: 1.0,
                volume: 1.0,
                enabled: true,
            });
            cursor += dur;
        }
        p.timeline.duration_sec = cursor;
        p.timeline.updated_at = now_iso();
        Ok(picks.len())
    })
}

pub fn render_timeline_job(state: &AppState, burn_subtitles: bool) -> Result<String> {
    let p = state.current()?;
    if !crate::media::timeline_has_clips(&p.timeline) {
        return Err(AppError::invalid("时间线上没有可用片段"));
    }
    let tl = p.timeline.clone();
    let out = p
        .paths
        .render_dir()
        .join(format!("render_{}.mp4", new_id("r")));
    let subs = if burn_subtitles {
        p.subtitles
            .iter()
            .find_map(|s| s.file.as_ref().map(PathBuf::from))
            .filter(|x| x.is_file())
    } else {
        None
    };
    let st = state.clone();
    let job = state.jobs.submit(
        JobKind::Ffmpeg,
        "导出成片",
        "准备中",
        move |ctx| {
            let st = st.clone();
            let tl = tl.clone();
            let out = out.clone();
            let subs = subs.clone();
            async move {
                ctx.progress(0.1, "构建滤镜图…");
                crate::media::render_timeline(&st, &tl, &out, subs.as_deref()).await?;
                ctx.progress(0.95, "完成");
                Ok(format!("已导出 {}", out.display()))
            }
        },
    );
    Ok(job.id)
}

/* ============================================================ 字幕 */

pub fn transcribe_job(state: &AppState, opts: TranscribeOptions) -> Result<String> {
    let st = state.clone();
    let job = state.jobs.submit(
        JobKind::Asr,
        format!("转写字幕 · {}", opts.name.clone()),
        "准备中",
        move |ctx| {
            let st = st.clone();
            let opts = opts.clone();
            async move {
                let doc = asr::transcribe(&st, &ctx, &opts).await?;
                let name = doc.name.clone();
                st.mutate(|p| {
                    p.subtitles.retain(|s| s.id != doc.id);
                    p.subtitles.push(doc.clone());
                    Ok(())
                })?;
                Ok(format!("字幕《{name}》已生成"))
            }
        },
    );
    Ok(job.id)
}

/* ========================================================= 小工具 */
