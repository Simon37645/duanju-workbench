//! 占位生成适配器：用 ffmpeg 造素材，把「提示词 → 任务 → 落盘 → 时间线 → 导出」整条链路跑通。
//! 没有配置任何生图/生视频接口时，UI 与 agent 依然可以完整走一遍流程。

use async_trait::async_trait;
use std::sync::Arc;

use super::{GenOutput, GenProvider, GenRequest};
use crate::error::Result;
use crate::jobs::JobCtx;
use crate::models::{ProviderConfig, ProviderKind};
use crate::state::AppState;

pub struct MockGenProvider {
    state: AppState,
    cfg: ProviderConfig,
}

impl MockGenProvider {
    pub fn new(state: AppState, cfg: ProviderConfig) -> Self {
        Self { state, cfg }
    }
}

#[async_trait]
impl GenProvider for MockGenProvider {
    fn adapter(&self) -> &'static str {
        "mock"
    }

    fn model(&self) -> String {
        if self.cfg.model.is_empty() {
            "mock".into()
        } else {
            self.cfg.model.clone()
        }
    }

    async fn generate(&self, ctx: &JobCtx, req: GenRequest) -> Result<GenOutput> {
        let is_video = matches!(self.cfg.kind, ProviderKind::Video);
        let tmp_dir = std::env::temp_dir().join("duanju-workbench-mock");
        crate::store::ensure_dir(&tmp_dir)?;

        let label = if req.prompt.trim().is_empty() {
            "MOCK".to_string()
        } else {
            req.prompt.clone()
        };
        let seed_part = req.seed.unwrap_or(0);

        ctx.progress(0.15, "占位生成中…");
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        ctx.check_cancel()?;

        if is_video {
            let dst = tmp_dir.join(format!(
                "mock_{}.mp4",
                crate::store::fingerprint(&format!("{label}{seed_part}"))
            ));
            let dur = req.duration_sec.unwrap_or(4.0).clamp(1.0, 20.0);
            ctx.progress(0.45, "编码占位视频…");
            crate::media::placeholder_video(
                &self.state,
                &label,
                if req.width > 0 { req.width } else { 720 },
                if req.height > 0 { req.height } else { 1280 },
                dur,
                &dst,
            )
            .await?;
            ctx.progress(0.9, "完成");
            Ok(GenOutput::File(dst))
        } else {
            let dst = tmp_dir.join(format!(
                "mock_{}.png",
                crate::store::fingerprint(&format!("{label}{seed_part}"))
            ));
            ctx.progress(0.45, "渲染占位图…");
            crate::media::placeholder_image(
                &self.state,
                &label,
                if req.width > 0 { req.width } else { 1024 },
                if req.height > 0 { req.height } else { 1024 },
                &dst,
            )
            .await?;
            ctx.progress(0.9, "完成");
            Ok(GenOutput::File(dst))
        }
    }
}

pub type SharedMock = Arc<MockGenProvider>;
