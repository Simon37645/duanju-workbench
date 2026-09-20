//! 面板完成度判定。驱动左侧导航徽标与 Checklist 面板的自动项。
//!
//! 这里的规则是**唯一事实来源**：用户看到的进度、agent 拿到的 checklist_report
//! 都走同一套计算，避免两边对不上。

use crate::models::{
    Actor, AssetKind, AssetStatus, ChapterStatus, ChecklistItem, PanelId, PanelProgress,
    ProjectSnapshot, ShotStatus,
};

pub fn all_progress(p: &ProjectSnapshot) -> Vec<PanelProgress> {
    PanelId::all().iter().map(|pid| progress(p, *pid)).collect()
}

pub fn progress(p: &ProjectSnapshot, panel: PanelId) -> PanelProgress {
    let mut blockers: Vec<String> = vec![];
    let (total, done) = match panel {
        PanelId::Script => {
            let total = p.chapters.len() as u32;
            let done = p
                .chapters
                .iter()
                .filter(|c| matches!(c.status, ChapterStatus::Written | ChapterStatus::Locked))
                .count() as u32;
            if total == 0 {
                blockers.push("还没有章节，先在剧本面板设定章节数".into());
            } else {
                let empty: Vec<String> = p
                    .chapters
                    .iter()
                    .filter(|c| matches!(c.status, ChapterStatus::Empty | ChapterStatus::Draft))
                    .map(|c| format!("第{}章 {}", c.index, c.title))
                    .collect();
                if !empty.is_empty() {
                    blockers.push(format!("{} 章还没写完：{}", empty.len(), empty.join("、")));
                }
            }
            (total, done)
        }
        PanelId::Style => {
            let ok = !p.style.spec.prompt.trim().is_empty();
            if !ok {
                blockers.push("还没有确定画面风格提示词".into());
            }
            (1, if ok { 1 } else { 0 })
        }
        PanelId::Storyboard => {
            let total = p.chapters.len() as u32;
            let done = p
                .chapters
                .iter()
                .filter(|c| p.shots.iter().any(|s| s.chapter_id == c.id))
                .count() as u32;
            let missing: Vec<String> = p
                .chapters
                .iter()
                .filter(|c| !p.shots.iter().any(|s| s.chapter_id == c.id))
                .map(|c| format!("第{}章 {}", c.index, c.title))
                .collect();
            if !missing.is_empty() {
                blockers.push(format!("{} 章还没有分镜：{}", missing.len(), missing.join("、")));
            }
            if p.chapters.is_empty() {
                blockers.push("需要先有剧本章节".into());
            }
            (total, done)
        }
        PanelId::Asset => {
            if p.assets.is_empty() {
                blockers.push("还没有建立任何资产".into());
            }
            let total = p.assets.len() as u32;
            let done = p
                .assets
                .iter()
                .filter(|a| a.views.iter().any(|v| v.status == AssetStatus::Done))
                .count() as u32;
            let no_view: Vec<String> = p
                .assets
                .iter()
                .filter(|a| a.views.is_empty())
                .map(|a| a.name.clone())
                .collect();
            if !no_view.is_empty() {
                blockers.push(format!("{} 个资产还没有规划视图：{}", no_view.len(), no_view.join("、")));
            }
            let failed: Vec<String> = p
                .assets
                .iter()
                .flat_map(|a| {
                    a.views
                        .iter()
                        .filter(|v| v.status == AssetStatus::Failed)
                        .map(move |v| format!("{}·{}", a.name, v.label))
                })
                .collect();
            if !failed.is_empty() {
                blockers.push(format!("{} 张图生成失败：{}", failed.len(), failed.join("、")));
            }
            let no_char = !p
                .assets
                .iter()
                .any(|a| a.kind == AssetKind::Character);
            if no_char && !p.assets.is_empty() {
                blockers.push("还没有人物资产（人物三视图是跨镜头一致性的关键）".into());
            }
            (total, done)
        }
        PanelId::Prompt => {
            let total = p.shots.len() as u32;
            let done = p
                .shots
                .iter()
                .filter(|s| {
                    p.prompts
                        .iter()
                        .any(|x| x.shot_id == s.id && !x.prompt.trim().is_empty())
                })
                .count() as u32;
            if total == 0 {
                blockers.push("还没有分镜，先做分镜".into());
            } else if done < total {
                blockers.push(format!("还有 {} 个镜头没有视频提示词", total - done));
            }
            let no_frame = p
                .prompts
                .iter()
                .filter(|x| x.first_frame.is_none())
                .count();
            if no_frame > 0 {
                blockers.push(format!("{no_frame} 条提示词没有配首帧图"));
            }
            (total, done)
        }
        PanelId::Video => {
            let total = p.shots.len() as u32;
            let done = p
                .shots
                .iter()
                .filter(|s| {
                    p.takes
                        .iter()
                        .any(|t| t.shot_id == s.id && t.status == AssetStatus::Done)
                })
                .count() as u32;
            if total == 0 {
                blockers.push("还没有分镜".into());
            } else if done < total {
                blockers.push(format!("还有 {} 个镜头没有生成视频", total - done));
            }
            let failed: Vec<String> = p
                .takes
                .iter()
                .filter(|t| t.status == AssetStatus::Failed)
                .map(|t| t.shot_id.clone())
                .collect();
            if !failed.is_empty() {
                blockers.push(format!("{} 条生成失败，需要重试", failed.len()));
            }
            (total, done)
        }
        PanelId::Edit => {
            let clips: usize = p
                .timeline
                .tracks
                .iter()
                .filter(|t| matches!(t.kind, crate::models::TrackKind::Video))
                .map(|t| t.clips.len())
                .sum();
            let generated = p
                .takes
                .iter()
                .filter(|t| t.status == AssetStatus::Done)
                .count();
            if clips == 0 {
                blockers.push("时间线还是空的，可以用「按镜头铺轨」一键生成".into());
            } else if clips < generated {
                blockers.push(format!(
                    "还有 {} 条已生成的视频没放进时间线",
                    generated - clips
                ));
            }
            (generated as u32, clips.min(generated) as u32)
        }
        PanelId::Subtitle => {
            let ok = !p.subtitles.is_empty();
            if !ok {
                blockers.push("还没有生成字幕".into());
            }
            (1, if ok { 1 } else { 0 })
        }
        PanelId::Checklist => {
            let manual: Vec<&ChecklistItem> = p.checklist.items.iter().filter(|i| !i.auto).collect();
            let total = manual.len() as u32;
            let done = manual.iter().filter(|i| i.done).count() as u32;
            if total > 0 && done < total {
                blockers.push(format!("还有 {} 项自定义检查没勾", total - done));
            }
            (total, done)
        }
        PanelId::Previz => {
            // 预演场景是前端自治的 JSON（.workbench/previz.json），这里只数实体
            let scene = crate::store::read_json_opt::<serde_json::Value>(&previz_file(p))
                .ok()
                .flatten();
            let count = |key: &str| -> u32 {
                scene
                    .as_ref()
                    .and_then(|s| s.get(key))
                    .and_then(|v| v.as_array())
                    .map(|a| a.len() as u32)
                    .unwrap_or(0)
            };
            let props = count("primitives") + count("characters");
            let cameras = count("cameras");
            if props == 0 {
                blockers.push("还没有摆放任何预演物体".into());
            }
            if cameras == 0 {
                blockers.push("还没有添加预演机位".into());
            }
            (2, if props > 0 && cameras > 0 { 2 } else { 0 })
        }
    };

    let percent = if total == 0 {
        if done > 0 {
            100.0
        } else {
            0.0
        }
    } else {
        (done as f32 / total as f32 * 100.0).clamp(0.0, 100.0)
    };

    PanelProgress {
        panel,
        total,
        done,
        percent,
        blockers,
    }
}

/// 预演场景文件（前端自治 JSON，Rust 只读它算进度）。
fn previz_file(p: &ProjectSnapshot) -> std::path::PathBuf {
    std::path::Path::new(&p.root)
        .join(".workbench")
        .join("previz.json")
}

/// 自动检查项：与上面的完成度同源，agent 只能读不能改。
pub fn auto_items(p: &ProjectSnapshot) -> Vec<ChecklistItem> {
    let mut out = vec![];
    for pr in all_progress(p) {
        let text = match pr.panel {
            PanelId::Script => format!("剧本：{} 章完成（共 {} 章）", pr.done, pr.total),
            PanelId::Style => "风格：已确定画面风格圣经".to_string(),
            PanelId::Storyboard => format!("分镜：{} 章有分镜（共 {} 章）", pr.done, pr.total),
            PanelId::Asset => format!("资产：{} 项已有成图（共 {} 项）", pr.done, pr.total),
            PanelId::Prompt => format!("提示词：{} 个镜头已写（共 {} 个）", pr.done, pr.total),
            PanelId::Video => format!("生视频：{} 个镜头已出片（共 {} 个）", pr.done, pr.total),
            PanelId::Edit => format!("剪辑：时间线 {} 段（可铺 {} 段）", pr.done, pr.total),
            PanelId::Subtitle => "字幕：已生成字幕文档".to_string(),
            PanelId::Checklist => continue,
            PanelId::Previz if pr.done > 0 => "3D预演：白模场景与机位已就绪".to_string(),
            PanelId::Previz => continue,
        };
        out.push(ChecklistItem {
            id: format!("auto_{}", pr.panel.as_str()),
            panel: pr.panel,
            text,
            done: pr.total > 0 && pr.done >= pr.total,
            done_by: Some(Actor::Agent),
            done_at: None,
            note: pr.blockers.first().cloned().unwrap_or_default(),
            auto: true,
        });
    }
    out
}

/// 新项目默认带上的自定义检查项，用户可以删。
pub fn default_items() -> Vec<ChecklistItem> {
    let rows = [
        (PanelId::Script, "每章结尾都有钩子"),
        (PanelId::Script, "人物关系在前三章交代清楚"),
        (PanelId::Style, "风格词与剧本题材匹配"),
        (PanelId::Storyboard, "没有超过 6 秒的长镜头"),
        (PanelId::Asset, "主要人物都有三视图"),
        (PanelId::Asset, "人物的固定特征（疤痕/眼镜/发型）已写入 lockedTraits"),
        (PanelId::Prompt, "每条提示词都描述了运动而不是静态画面"),
        (PanelId::Prompt, "首帧图已配齐"),
        (PanelId::Video, "重点镜头的效果已人工确认"),
        (PanelId::Edit, "片段衔接处没有黑帧"),
        (PanelId::Subtitle, "字幕断句不超过 18 个字"),
    ];
    rows.iter()
        .map(|(panel, text)| ChecklistItem {
            id: crate::models::new_id("ck"),
            panel: *panel,
            text: text.to_string(),
            done: false,
            done_by: None,
            done_at: None,
            note: String::new(),
            auto: false,
        })
        .collect()
}

/// 镜头是否已经具备生成视频的条件。
pub fn shot_ready_for_video(
    p: &ProjectSnapshot,
    shot_id: &str,
) -> (bool, Vec<String>) {
    let mut problems = vec![];
    let Some(shot) = p.shots.iter().find(|s| s.id == shot_id) else {
        return (false, vec!["镜头不存在".into()]);
    };
    if !matches!(shot.status, ShotStatus::Ready | ShotStatus::Prompted | ShotStatus::Generated | ShotStatus::Locked) {
        // draft 状态仍允许，只是提示
    }
    let prompt = p.prompts.iter().find(|x| x.shot_id == shot_id);
    match prompt {
        None => problems.push("还没有视频提示词".to_string()),
        Some(pr) => {
            if pr.prompt.trim().is_empty() {
                problems.push("视频提示词为空".to_string());
            }
            if pr.first_frame.is_none() {
                problems.push("没有配首帧图".to_string());
            }
        }
    }
    (problems.is_empty(), problems)
}
