//! Checklist 组装：把「数据自动判定的项」和「用户/agent 手工维护的项」合成一份视图。
//!
//! 自动项不落盘，每次读取时现算 —— 这样数据一变，checklist 面板立刻就是准的，
//! 不会出现「勾了但数据其实没做完」的假象。

use std::sync::Arc;

use crate::models::{ChecklistItem, ProjectSnapshot};
use crate::progress;
use crate::project::Project;

pub fn snapshot(p: &Project) -> ProjectSnapshot {
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

/// 用最新的数据重算自动项，覆盖传入快照里的 checklist。
pub fn with_auto(_p: Arc<Project>, mut snap: ProjectSnapshot) -> ProjectSnapshot {
    let autos: Vec<ChecklistItem> = progress::auto_items(&snap);
    let manual: Vec<ChecklistItem> = snap
        .checklist
        .items
        .iter()
        .filter(|i| !i.auto)
        .cloned()
        .collect();
    let mut items = autos;
    items.extend(manual);
    snap.checklist.items = items;
    snap
}
