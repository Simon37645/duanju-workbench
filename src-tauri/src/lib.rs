//! 短剧工作台 —— 后端入口。
//!
//! 说明：这里允许 dead_code。作为一个还在长的骨架，很多能力（缓存统计、任务查询、
//! 资产摘要等）已经按接口写好，供后续面板与 agent 工具陆续接进来。

#![allow(dead_code)]

pub mod actions;
pub mod agent;
pub mod asr;
pub mod checklist_ops;
mod commands;
pub mod config;
pub mod error;
pub mod gen;
pub mod jobs;
pub mod llm;
pub mod media;
pub mod models;
pub mod net;
pub mod presets;
pub mod progress;
pub mod project;
pub mod selftest;
pub mod state;
pub mod store;

use tauri::Manager;

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 日志：RUST_LOG=debug 可以打开详细输出
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            std::fs::create_dir_all(&dir)?;
            tracing::info!("配置目录: {}", dir.display());
            let state = AppState::new(dir);
            state.attach_app(app.handle().clone());
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 项目
            commands::project_create,
            commands::project_open,
            commands::project_close,
            commands::project_snapshot,
            commands::project_progress,
            commands::project_update_manifest,
            commands::project_paths,
            // 剧本
            commands::script_read_chapter,
            commands::script_save_chapter,
            commands::script_add_chapter,
            commands::script_update_chapter_meta,
            commands::script_delete_chapter,
            commands::script_reorder_chapters,
            // 圣经 / 风格
            commands::bible_get,
            commands::bible_set,
            commands::style_get,
            commands::style_set,
            commands::style_presets,
            commands::storyboard_vocab,
            // 分镜
            commands::storyboard_set_shots,
            commands::storyboard_upsert_shot,
            commands::storyboard_delete_shot,
            // 资产
            commands::asset_upsert,
            commands::asset_delete,
            commands::asset_plan_views,
            commands::asset_generate_views,
            commands::asset_view_delete,
            // 提示词
            commands::prompt_upsert,
            commands::prompt_delete,
            commands::prompt_autofill,
            // 生视频
            commands::video_generate,
            commands::video_delete_take,
            // 剪辑
            commands::edit_set_timeline,
            commands::edit_build_from_takes,
            commands::edit_render,
            // 字幕
            commands::subtitle_transcribe,
            commands::subtitle_get,
            commands::subtitle_save_cues,
            commands::subtitle_delete,
            // checklist
            commands::checklist_get,
            commands::checklist_add,
            commands::checklist_toggle,
            commands::checklist_delete,
            commands::checklist_reset_defaults,
            // 任务
            commands::jobs_list,
            commands::job_cancel,
            commands::jobs_clear_finished,
            // 设置
            commands::settings_get,
            commands::settings_set,
            commands::provider_upsert,
            commands::provider_delete,
            commands::provider_set_secret,
            commands::secrets_status,
            commands::provider_test,
            commands::proxy_status,
            commands::net_test,
            // 媒体 / ASR
            commands::media_probe,
            commands::media_sidecar_status,
            commands::asr_capabilities,
            commands::asr_download_model,
            commands::asr_delete_model,
            // agent
            commands::agent_sessions,
            commands::agent_session_get,
            commands::agent_session_delete,
            commands::agent_session_new,
            commands::agent_run,
            commands::agent_approve,
            commands::agent_prefix_preview,
        ])
        .run(tauri::generate_context!())
        .expect("短剧工作台启动失败");
}
