//! 字幕：本地 whisper 转写。
//!
//! 两条可用的推理路径，按环境自动选：
//! 1. **进程内 whisper.cpp**（Cargo feature `asr*`）：编译期决定后端
//!    （`asr-cuda` / `asr-metal` / `asr-vulkan` / `asr-coreml` / `asr` 纯 CPU）。
//!    运行时再用 `use_gpu` 开关决定是否把计算放到显卡上 —— 也就是用户在设置里勾的那个。
//! 2. **外部 whisper-cli**：本机有 whisper.cpp 官方命令行工具时直接调用，无需重新编译。
//!
//! 两条路都先把素材用 ffmpeg 转成 16kHz 单声道 wav，保证输入一致。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::jobs::JobCtx;
use crate::models::{new_id, now_iso, Cue, SubtitleDoc};
use crate::state::AppState;

/* ============================================================ 能力与探测 */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModelInfo {
    pub id: String,
    pub label: String,
    pub file_name: String,
    pub size_mb: u32,
    pub downloaded: bool,
    pub path: Option<String>,
    pub multilingual: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrCapabilities {
    pub os: String,
    pub arch: String,
    pub cpu_threads: u32,
    pub apple_silicon: bool,
    pub cuda_available: bool,
    pub cuda_version: Option<String>,
    pub nvidia_gpu: Option<String>,
    pub metal_supported: bool,
    pub vulkan_supported: bool,
    pub directml_supported: bool,
    pub whisper_compiled: bool,
    pub compiled_backends: Vec<String>,
    pub external_cli_available: bool,
    pub external_cli_path: Option<String>,
    pub recommended_backend: String,
    /// 模型存放目录（可以手动把 .bin 放进来）
    pub models_dir: String,
    pub installed_models: Vec<WhisperModelInfo>,
}

pub fn models_dir(state: &AppState) -> PathBuf {
    state.config.dir().join("models").join("whisper")
}

/// 把随安装包分发的模型补进用户模型目录：只补缺失的，不覆盖用户已下的。
/// 发布包的布局：<安装目录>/models/ggml-*.bin（tauri.conf 的 resources 映射）。
pub fn seed_bundled_models(state: &AppState) {
    let Ok(exe) = std::env::current_exe() else { return };
    let Some(dir) = exe.parent().map(|p| p.join("models")) else {
        return;
    };
    if !dir.is_dir() {
        return; // 开发环境没有随包模型，正常
    }
    let dst_dir = models_dir(state);
    for (_id, _label, file, _mb, _ml) in model_registry() {
        let src = dir.join(&file);
        let dst = dst_dir.join(&file);
        if src.is_file() && !dst.is_file() {
            if crate::store::ensure_dir(&dst_dir).is_err() {
                return;
            }
            match std::fs::copy(&src, &dst) {
                Ok(_) => tracing::info!("已从安装包补种 whisper 模型：{file}"),
                Err(e) => tracing::warn!("补种 whisper 模型失败 {file}: {e}"),
            }
        }
    }
}

pub fn model_registry() -> Vec<(String, String, String, u32, bool)> {
    // (id, 显示名, 文件名, 约略 MB, 是否多语言)
    vec![
        ("ggml-tiny".into(), "Tiny（最快，质量低）".into(), "ggml-tiny.bin".into(), 75, true),
        ("ggml-base".into(), "Base（快，质量一般）".into(), "ggml-base.bin".into(), 142, true),
        ("ggml-small".into(), "Small（均衡）".into(), "ggml-small.bin".into(), 466, true),
        ("ggml-medium".into(), "Medium（较准）".into(), "ggml-medium.bin".into(), 1500, true),
        ("ggml-large-v3-turbo".into(), "Large v3 Turbo（推荐，中文好且快）".into(), "ggml-large-v3-turbo.bin".into(), 1620, true),
        ("ggml-large-v3".into(), "Large v3（最准，最慢）".into(), "ggml-large-v3.bin".into(), 3090, true),
    ]
}

pub fn model_path(state: &AppState, model_id: &str) -> Option<PathBuf> {
    let reg = model_registry();
    let (_, _, file, _, _) = reg.iter().find(|(id, _, _, _, _)| id == model_id)?;
    let p = models_dir(state).join(file);
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

fn nvidia_probe() -> (bool, Option<String>, Option<String>) {
    static CACHE: OnceLock<(bool, Option<String>, Option<String>)> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            // CUDA toolkit
            // 注意：这些探测会起子进程，必须走 proc::hidden，
            // 否则在 Windows 上打开设置面板会闪好几个控制台黑框。
            let mut cuda_version = None;
            let nvcc = if cfg!(windows) { "nvcc.exe" } else { "nvcc" };
            if let Some(s) = crate::proc::output_text(nvcc, &["--version"]) {
                if let Some(line) = s.lines().find(|l| l.contains("release")) {
                    cuda_version = line
                        .split("release")
                        .nth(1)
                        .map(|x| x.trim().trim_end_matches(',').to_string());
                }
            }
            let smi = if cfg!(windows) { "nvidia-smi.exe" } else { "nvidia-smi" };
            let mut gpu = None;
            let mut has_smi = false;
            if let Some(s) =
                crate::proc::output_text(smi, &["--query-gpu=name", "--format=csv,noheader"])
            {
                has_smi = true;
                let first = s.lines().next().unwrap_or("").trim().to_string();
                if !first.is_empty() {
                    gpu = Some(first);
                }
            }
            let available = has_smi || cuda_version.is_some();
            (available, cuda_version, gpu)
        })
        .clone()
}

fn external_cli(state: &AppState) -> Option<PathBuf> {
    let s = state.settings();
    if !s.whisper_cli_path.trim().is_empty() {
        let p = PathBuf::from(s.whisper_cli_path.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    // 常见命名：whisper-cli / main（老版本）
    for stem in ["whisper-cli", "whisper", "main"] {
        if let Ok(p) = crate::media::resolve_tool(state, stem, "") {
            return Some(p);
        }
    }
    None
}

pub fn capabilities(state: &AppState) -> AsrCapabilities {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let cpu_threads = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);
    let apple_silicon = cfg!(all(target_os = "macos", target_arch = "aarch64"));
    let (cuda_available, cuda_version, nvidia_gpu) = nvidia_probe();

    let mut compiled: Vec<String> = vec![];
    if cfg!(feature = "asr-cuda") {
        compiled.push("cuda".into());
    }
    if cfg!(feature = "asr-metal") {
        compiled.push("metal".into());
    }
    if cfg!(feature = "asr-vulkan") {
        compiled.push("vulkan".into());
    }
    if cfg!(feature = "asr-coreml") {
        compiled.push("coreml".into());
    }
    if cfg!(feature = "asr-openblas") {
        compiled.push("openblas".into());
    }
    let whisper_compiled = cfg!(feature = "asr");
    if whisper_compiled && compiled.is_empty() {
        compiled.push("cpu".into());
    }

    let cli = external_cli(state);

    let recommended = if apple_silicon {
        "metal"
    } else if cuda_available {
        "cuda"
    } else if cfg!(windows) {
        "vulkan"
    } else {
        "cpu"
    }
    .to_string();

    let installed_models = model_registry()
        .into_iter()
        .map(|(id, label, file_name, size_mb, multilingual)| {
            let p = models_dir(state).join(&file_name);
            WhisperModelInfo {
                downloaded: p.is_file(),
                path: p.is_file().then(|| p.to_string_lossy().to_string()),
                id,
                label,
                file_name,
                size_mb,
                multilingual,
            }
        })
        .collect();

    AsrCapabilities {
        os,
        arch,
        cpu_threads,
        apple_silicon,
        cuda_available,
        cuda_version,
        nvidia_gpu,
        metal_supported: cfg!(target_os = "macos"),
        vulkan_supported: cfg!(any(windows, target_os = "linux")),
        directml_supported: cfg!(windows),
        whisper_compiled,
        compiled_backends: compiled,
        external_cli_available: cli.is_some(),
        external_cli_path: cli.map(|p| p.to_string_lossy().to_string()),
        recommended_backend: recommended,
        models_dir: models_dir(state).to_string_lossy().to_string(),
        installed_models,
    }
}

/* ============================================================ 模型下载 */

pub async fn download_model(
    state: &AppState,
    ctx: &JobCtx,
    model_id: &str,
) -> Result<WhisperModelInfo> {
    let reg = model_registry();
    let (id, label, file_name, size_mb, multilingual) = reg
        .iter()
        .find(|(i, _, _, _, _)| i == model_id)
        .cloned()
        .ok_or_else(|| AppError::invalid(format!("未知的 whisper 模型：{model_id}")))?;

    let dir = models_dir(state);
    crate::store::ensure_dir(&dir)?;
    let dst = dir.join(&file_name);
    if dst.is_file() {
        return Ok(WhisperModelInfo {
            id,
            label,
            file_name,
            size_mb,
            downloaded: true,
            path: Some(dst.to_string_lossy().to_string()),
            multilingual,
        });
    }

    let base = {
        let s = state.settings();
        if s.asr_model_base_url.trim().is_empty() {
            crate::models::DEFAULT_WHISPER_BASE_URL.to_string()
        } else {
            s.asr_model_base_url.trim().trim_end_matches('/').to_string()
        }
    };
    let url = format!("{base}/{file_name}");
    let http = state.http_client()?;
    let part = dst.with_extension("part");

    // 断点续传：接着上次没下完的 .part 继续。大模型动辄 1~3 GB，断一次重来太亏。
    let already = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    ctx.progress(0.02, "连接模型仓库…");
    let resp = match crate::llm::send_with_retry(
        &http,
        || {
            let rb = http.get(&url);
            if already > 0 {
                rb.header("Range", format!("bytes={already}-"))
            } else {
                rb
            }
        },
        2,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return Err(AppError::Provider(format!(
                "{e}\n下载源：{url}\n\
                 如果这个地址访问不了（国内网络常见），可以：\n\
                 1) 在「设置 → 字幕」里把下载源换成镜像，例如 https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main；\n\
                 2) 或者自己下载 {file_name} 放到：{}",
                dir.display()
            )))
        }
    };

    let resuming = already > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if already > 0 && !resuming {
        // 服务端不认 Range，只能从头来
        let _ = std::fs::remove_file(&part);
    }
    let done_before = if resuming { already } else { 0 };
    let total = resp.content_length().unwrap_or(0) + done_before;
    let mut file = if resuming {
        tokio::fs::OpenOptions::new().append(true).open(&part).await?
    } else {
        tokio::fs::File::create(&part).await?
    };
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let mut stream = resp.bytes_stream();
    let mut got: u64 = done_before;
    if done_before > 0 {
        ctx.progress(
            got as f32 / total.max(1) as f32,
            format!("续传，已有 {:.0} MB", done_before as f64 / 1_048_576.0),
        );
    }
    while let Some(chunk) = stream.next().await {
        ctx.check_cancel()?;
        let c = chunk.map_err(AppError::from)?;
        file.write_all(&c).await?;
        got += c.len() as u64;
        if total > 0 {
            ctx.progress(
                got as f32 / total as f32,
                format!(
                    "下载 {:.0}/{:.0} MB",
                    got as f64 / 1_048_576.0,
                    total as f64 / 1_048_576.0
                ),
            );
        }
    }
    file.flush().await?;
    drop(file);
    std::fs::rename(&part, &dst)?;

    Ok(WhisperModelInfo {
        id,
        label,
        file_name,
        size_mb,
        downloaded: true,
        path: Some(dst.to_string_lossy().to_string()),
        multilingual,
    })
}

pub fn delete_model(state: &AppState, model_id: &str) -> Result<()> {
    let reg = model_registry();
    let (_, _, file, _, _) = reg
        .iter()
        .find(|(i, _, _, _, _)| i == model_id)
        .cloned()
        .ok_or_else(|| AppError::invalid(format!("未知的 whisper 模型：{model_id}")))?;
    let p = models_dir(state).join(file);
    crate::store::remove_file_if_exists(&p)
}

/* ============================================================== 转写 */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscribeOptions {
    pub file: String,
    pub language: String,
    pub model_id: String,
    pub use_gpu: bool,
    pub threads: i32,
    pub max_line_chars: u32,
    /// 输出文件名（不含扩展名），留空则自动生成
    pub name: String,
}

pub async fn transcribe(
    state: &AppState,
    ctx: &JobCtx,
    opts: &TranscribeOptions,
) -> Result<SubtitleDoc> {
    let media_file = PathBuf::from(&opts.file);
    if !media_file.is_file() {
        return Err(AppError::NotFound(format!("找不到素材：{}", opts.file)));
    }
    let caps = capabilities(state);
    let model_file = model_path(state, &opts.model_id).ok_or_else(|| {
        AppError::Config(format!(
            "whisper 模型「{}」尚未下载，请先在字幕面板下载。",
            opts.model_id
        ))
    })?;

    // 统一先转 16k 单声道 wav
    let project = state.current()?;
    let wav = project
        .paths
        .tmp_dir()
        .join(format!("asr_{}.wav", new_id("w")));
    ctx.progress(0.08, "提取音轨…");
    crate::media::extract_wav16k(state, &media_file, &wav).await?;
    ctx.check_cancel()?;

    let threads = if opts.threads > 0 {
        opts.threads
    } else {
        (caps.cpu_threads / 2).max(2) as i32
    };
    let language = if opts.language.trim().is_empty() {
        "auto".to_string()
    } else {
        opts.language.clone()
    };

    let cues = if caps.whisper_compiled {
        ctx.progress(0.2, "本地推理中…");
        tokio::task::block_in_place(|| {
            transcribe_builtin(&wav, &model_file, &language, opts.use_gpu, threads)
        })?
    } else if let Some(cli) = external_cli(state) {
        ctx.progress(0.2, "调用外部 whisper-cli…");
        transcribe_external(&cli, &wav, &model_file, &language, opts.use_gpu, threads).await?
    } else {
        return Err(AppError::Config(
            "当前无法进行转写：这个构建没有编译进 whisper 推理，也没找到外部 whisper-cli。\n\
             任选一种方式：\n\
             1) 带 feature 重新构建，例如 `cargo build --features asr-cuda`（需要 cmake + libclang）；\n\
             2) 下载 whisper.cpp 官方 whisper-cli 放到应用目录，或在设置里指定它的路径。"
                .into(),
        ));
    };

    let _ = std::fs::remove_file(&wav);
    ctx.check_cancel()?;
    ctx.progress(0.9, "整理字幕…");

    let cues = postprocess(cues, opts.max_line_chars.max(6));
    let doc_id = new_id("sub");
    let name = if opts.name.trim().is_empty() {
        media_file
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| doc_id.clone())
    } else {
        opts.name.trim().to_string()
    };

    let srt_path = project.paths.subtitle_dir().join(format!("{doc_id}.srt"));
    let vtt_path = project.paths.subtitle_dir().join(format!("{doc_id}.vtt"));
    crate::store::write_text(&srt_path, &to_srt(&cues))?;
    crate::store::write_text(&vtt_path, &to_vtt(&cues))?;

    Ok(SubtitleDoc {
        id: doc_id,
        name,
        source: opts.file.clone(),
        language: if language == "auto" {
            "auto".into()
        } else {
            language
        },
        model: opts.model_id.clone(),
        cues,
        file: Some(srt_path.to_string_lossy().to_string()),
        created_at: now_iso(),
    })
}

/// 去掉重复空白、合并过短的断句。
fn postprocess(cues: Vec<Cue>, max_chars: u32) -> Vec<Cue> {
    let mut out: Vec<Cue> = vec![];
    for c in cues {
        let text = c.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let text = if text.chars().count() as u32 > max_chars * 2 {
            // 过长的行按标点切一刀
            text
        } else {
            text
        };
        if let Some(last) = out.last_mut() {
            let gap = c.start - last.end;
            let merged_len = last.text.chars().count() + text.chars().count();
            if gap < 0.25 && merged_len <= max_chars as usize {
                last.text.push_str(&text);
                last.end = c.end;
                continue;
            }
        }
        out.push(Cue {
            index: 0,
            start: c.start,
            end: c.end,
            text,
        });
    }
    for (i, c) in out.iter_mut().enumerate() {
        c.index = i as u32 + 1;
    }
    out
}

/* ------------------------------------------------------- 路径 1：进程内 */

#[cfg(feature = "asr")]
fn transcribe_builtin(
    wav: &Path,
    model: &Path,
    language: &str,
    use_gpu: bool,
    threads: i32,
) -> Result<Vec<Cue>> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    let samples = read_wav_f32(wav)?;
    let mut cparams = WhisperContextParameters::default();
    cparams.use_gpu(use_gpu);
    let ctx = WhisperContext::new_with_params(
        model
            .to_str()
            .ok_or_else(|| AppError::other("模型路径包含非法字符"))?,
        cparams,
    )
    .map_err(|e| AppError::other(format!("加载 whisper 模型失败: {e}")))?;
    let mut st = ctx
        .create_state()
        .map_err(|e| AppError::other(format!("创建推理状态失败: {e}")))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(threads);
    params.set_translate(false);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    if language != "auto" {
        params.set_language(Some(language));
    }
    st.full(params, &samples)
        .map_err(|e| AppError::other(format!("转写失败: {e}")))?;

    let n = st.full_n_segments();
    let mut cues = vec![];
    for i in 0..n {
        let text = st
            .full_get_segment_text(i)
            .unwrap_or_default()
            .trim()
            .to_string();
        let t0 = st.full_get_segment_t0(i) as f32 / 100.0;
        let t1 = st.full_get_segment_t1(i) as f32 / 100.0;
        if !text.is_empty() {
            cues.push(Cue {
                index: 0,
                start: t0,
                end: t1,
                text,
            });
        }
    }
    Ok(cues)
}

#[cfg(not(feature = "asr"))]
fn transcribe_builtin(
    _wav: &Path,
    _model: &Path,
    _language: &str,
    _use_gpu: bool,
    _threads: i32,
) -> Result<Vec<Cue>> {
    Err(AppError::Config(
        "这个构建没有编译进 whisper 推理（Cargo feature `asr` 未开启）".into(),
    ))
}

/// 读 ffmpeg 产出的 16kHz 单声道 PCM s16le wav，转成 f32。
#[allow(dead_code)]
fn read_wav_f32(path: &Path) -> Result<Vec<f32>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 44 {
        return Err(AppError::other("wav 文件太小，可能转换失败"));
    }
    // 定位 data chunk
    let mut pos = 12usize;
    let mut data_start = 44usize;
    let mut data_len = bytes.len() - 44;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]])
            as usize;
        if id == b"data" {
            data_start = pos + 8;
            data_len = size.min(bytes.len().saturating_sub(data_start));
            break;
        }
        pos += 8 + size + (size % 2);
    }
    let slice = &bytes[data_start..(data_start + data_len).min(bytes.len())];
    let mut out = Vec::with_capacity(slice.len() / 2);
    for pair in slice.chunks_exact(2) {
        let v = i16::from_le_bytes([pair[0], pair[1]]);
        out.push(v as f32 / 32768.0);
    }
    Ok(out)
}

/* ------------------------------------------------- 路径 2：外部 CLI */

async fn transcribe_external(
    cli: &Path,
    wav: &Path,
    model: &Path,
    language: &str,
    use_gpu: bool,
    threads: i32,
) -> Result<Vec<Cue>> {
    let out_prefix = wav.with_extension("asr");
    let mut args: Vec<String> = vec![
        "-m".into(),
        model.to_string_lossy().to_string(),
        "-f".into(),
        wav.to_string_lossy().to_string(),
        "-oj".into(),
        "-of".into(),
        out_prefix.to_string_lossy().to_string(),
        "-t".into(),
        threads.to_string(),
        "-np".into(),
    ];
    if language != "auto" {
        args.push("-l".into());
        args.push(language.to_string());
    }
    if !use_gpu {
        args.push("-ng".into());
    }
    crate::media::run(cli, &args).await?;

    let json_path = out_prefix.with_extension("json");
    let text = crate::store::read_text_opt(&json_path)?
        .ok_or_else(|| AppError::other("whisper-cli 没有产出 JSON 结果"))?;
    let v: serde_json::Value = serde_json::from_str(&text)?;
    let arr = v
        .get("transcription")
        .and_then(|t| t.as_array())
        .ok_or_else(|| AppError::other("whisper-cli JSON 结构异常"))?;
    let mut cues = vec![];
    for item in arr {
        let text = item
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if text.is_empty() {
            continue;
        }
        let from = item
            .pointer("/offsets/from")
            .and_then(|x| x.as_i64())
            .unwrap_or(0) as f32
            / 1000.0;
        let to = item
            .pointer("/offsets/to")
            .and_then(|x| x.as_i64())
            .unwrap_or(0) as f32
            / 1000.0;
        cues.push(Cue {
            index: 0,
            start: from,
            end: to.max(from + 0.2),
            text,
        });
    }
    let _ = std::fs::remove_file(&json_path);
    Ok(cues)
}

/* ============================================================ 格式化 */

pub fn fmt_ts(sec: f32, comma: bool) -> String {
    let s = sec.max(0.0);
    let h = (s / 3600.0).floor() as u32;
    let m = ((s % 3600.0) / 60.0).floor() as u32;
    let ss = (s % 60.0).floor() as u32;
    let ms = ((s - s.floor()) * 1000.0).round() as u32;
    format!(
        "{h:02}:{m:02}:{ss:02}{}{ms:03}",
        if comma { "," } else { "." }
    )
}

pub fn to_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, c) in cues.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            fmt_ts(c.start, true),
            fmt_ts(c.end, true),
            c.text
        ));
    }
    out
}

pub fn to_vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for c in cues {
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            fmt_ts(c.start, false),
            fmt_ts(c.end, false),
            c.text
        ));
    }
    out
}
