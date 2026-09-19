//! 生图 / 生视频 provider 抽象。
//!
//! 目前提供两个适配器：
//! - `generic-http`：**配置驱动**。请求怎么发、任务怎么轮询、结果 JSON 在哪取，
//!   全部写在 provider 的 `options` 里（见 `generic.rs` 顶部的模板说明）。
//!   等用户给出具体接口格式，填一份配置即可接入，不需要改 Rust 代码。
//! - `mock`：用 ffmpeg 生成占位图 / 占位视频，没配任何接口也能把整条生产链跑通。

pub mod generic;
pub mod mock;
pub mod upload;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use crate::error::{AppError, Result};
use crate::jobs::JobCtx;
use crate::models::ProviderConfig;
use crate::state::AppState;

#[derive(Debug, Clone, Default)]
pub struct GenRequest {
    pub prompt: String,
    pub negative: String,
    pub width: u32,
    pub height: u32,
    pub seed: Option<i64>,
    /// 参考图 / 首帧 / 尾帧（按顺序）
    pub ref_images: Vec<PathBuf>,
    pub duration_sec: Option<f32>,
    /// 透传给适配器的额外字段（例如镜头运动、模型特定参数）
    pub extra: serde_json::Value,
}

pub enum GenOutput {
    /// 已经落在磁盘上的文件
    File(PathBuf),
    /// 原始字节，由调用方决定存哪儿
    Bytes(Vec<u8>),
}

#[async_trait]
pub trait GenProvider: Send + Sync {
    fn adapter(&self) -> &'static str;
    fn model(&self) -> String;
    async fn generate(&self, ctx: &JobCtx, req: GenRequest) -> Result<GenOutput>;
}

pub fn build_provider(
    state: &AppState,
    cfg: &ProviderConfig,
    api_key: Option<String>,
) -> Result<Arc<dyn GenProvider>> {
    let http = state.http_client()?;
    match cfg.adapter.as_str() {
        "generic-http" | "generic" | "" => Ok(Arc::new(generic::GenericHttpProvider::new(
            cfg.clone(),
            api_key,
            http,
        )?)),
        "mock" => Ok(Arc::new(mock::MockGenProvider::new(
            state.clone(),
            cfg.clone(),
        ))),
        other => Err(AppError::Config(format!(
            "未知的生图/生视频适配器：{other}（可选 generic-http / mock）"
        ))),
    }
}

/// 把 provider 的输出落到指定路径。
pub async fn persist_output(out: GenOutput, dst: &Path) -> Result<PathBuf> {
    if let Some(p) = dst.parent() {
        crate::store::ensure_dir(p)?;
    }
    match out {
        GenOutput::File(src) => {
            if src != dst {
                std::fs::copy(&src, dst)?;
            }
            Ok(dst.to_path_buf())
        }
        GenOutput::Bytes(b) => {
            std::fs::write(dst, b)?;
            Ok(dst.to_path_buf())
        }
    }
}

/// 参考图转 data-url（部分接口要求这种格式）。
pub fn ref_image_data_url(path: &Path) -> Result<String> {
    use base64::Engine;
    let bytes = std::fs::read(path)?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

pub fn ref_image_b64(path: &Path) -> Result<String> {
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(std::fs::read(path)?))
}
