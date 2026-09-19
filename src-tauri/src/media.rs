//! 媒体层：ffmpeg / ffprobe 的定位与调用。
//!
//! 二进制解析顺序（用户明确选了「只内置 sidecar」，所以 release 下不会去蹭系统 PATH）：
//! 1. 设置里手动指定的路径；
//! 2. 随应用分发的 sidecar（exe 同目录 / 资源目录 / 开发期的 src-tauri/binaries）；
//! 3. PATH —— 仅 debug 构建兜底，方便开发期不用每次都准备 sidecar。

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::models::Timeline;
use crate::state::AppState;

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|x| x.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn binary_names(stem: &str) -> Vec<String> {
    let triple = option_env!("TARGET_TRIPLE").unwrap_or("");
    let mut v = vec![];
    #[cfg(windows)]
    {
        if !triple.is_empty() {
            v.push(format!("{stem}-{triple}.exe"));
        }
        v.push(format!("{stem}.exe"));
    }
    #[cfg(not(windows))]
    {
        if !triple.is_empty() {
            v.push(format!("{stem}-{triple}"));
        }
        v.push(stem.to_string());
    }
    v
}

fn search_dirs() -> Vec<PathBuf> {
    let exe = exe_dir();
    let mut dirs = vec![
        exe.clone(),
        exe.join("binaries"),
        exe.join("resources"),
        exe.join("resources").join("binaries"),
    ];
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("binaries"));
        dirs.push(cwd.join("src-tauri").join("binaries"));
        dirs.push(cwd.join("..").join("src-tauri").join("binaries"));
    }
    dirs
}

fn find_in_path(stem: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in binary_names(stem) {
            let candidate = dir.join(&name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn resolve_tool(_state: &AppState, stem: &str, override_path: &str) -> Result<PathBuf> {
    if !override_path.trim().is_empty() {
        let p = PathBuf::from(override_path.trim());
        if p.is_file() {
            return Ok(p);
        }
        return Err(AppError::Config(format!(
            "设置里指定的 {stem} 路径不存在：{}",
            p.display()
        )));
    }
    for dir in search_dirs() {
        for name in binary_names(stem) {
            let candidate = dir.join(&name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    #[cfg(debug_assertions)]
    {
        if let Some(p) = find_in_path(stem) {
            tracing::warn!("使用系统 PATH 中的 {stem}（仅开发期兜底）: {}", p.display());
            return Ok(p);
        }
    }
    Err(AppError::Config(format!(
        "找不到 {stem}。请把 {stem} 放到应用目录（发行版自带），或在设置里手动指定路径。"
    )))
}

pub fn ffmpeg_path(state: &AppState) -> Result<PathBuf> {
    let s = state.settings();
    resolve_tool(state, "ffmpeg", &s.ffmpeg_path)
}

pub fn ffprobe_path(state: &AppState) -> Result<PathBuf> {
    let s = state.settings();
    resolve_tool(state, "ffprobe", &s.ffprobe_path)
}

pub fn sidecar_status(state: &AppState) -> serde_json::Value {
    let s = state.settings();
    let f = resolve_tool(state, "ffmpeg", &s.ffmpeg_path);
    let p = resolve_tool(state, "ffprobe", &s.ffprobe_path);
    serde_json::json!({
        "ffmpeg": { "ok": f.is_ok(), "path": f.as_ref().ok().map(|x| x.to_string_lossy().to_string()), "error": f.err().map(|e| e.to_string()) },
        "ffprobe": { "ok": p.is_ok(), "path": p.as_ref().ok().map(|x| x.to_string_lossy().to_string()), "error": p.err().map(|e| e.to_string()) },
    })
}

/* ==================================================================== 执行 */

pub fn command(program: &Path) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(program);
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW，避免弹黑框
    c
}

pub async fn run(program: &Path, args: &[String]) -> Result<String> {
    let out = command(program)
        .args(args)
        .output()
        .await
        .map_err(|e| AppError::other(format!("执行 {} 失败: {e}", program.display())))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        // ffmpeg 会把版权信息也打到 stderr，真正的原因在最后几行
        let tail: Vec<&str> = err
            .lines()
            .filter(|l| !l.trim().is_empty())
            .rev()
            .take(12)
            .collect();
        let detail: String = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(AppError::other(format!(
            "{} 执行失败（退出码 {:?}）：\n{}\n命令：{} {}",
            program
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            out.status.code(),
            crate::llm::truncate(&detail, 1500),
            program.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
            crate::llm::truncate(&args.join(" "), 2000),
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/* ==================================================================== 探针 */

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub path: String,
    pub duration_sec: f32,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub has_audio: bool,
    pub has_video: bool,
    pub codec: String,
    pub size_bytes: u64,
}

pub async fn probe(state: &AppState, file: &Path) -> Result<MediaInfo> {
    let ffprobe = ffprobe_path(state)?;
    let args = vec![
        "-v".into(),
        "quiet".into(),
        "-print_format".into(),
        "json".into(),
        "-show_format".into(),
        "-show_streams".into(),
        file.to_string_lossy().to_string(),
    ];
    let out = run(&ffprobe, &args).await?;
    let v: serde_json::Value = serde_json::from_str(&out)?;
    let mut info = MediaInfo {
        path: file.to_string_lossy().to_string(),
        ..Default::default()
    };
    if let Some(fmt) = v.get("format") {
        info.duration_sec = fmt
            .get("duration")
            .and_then(|d| d.as_str())
            .and_then(|d| d.parse::<f32>().ok())
            .unwrap_or(0.0);
        info.size_bytes = fmt
            .get("size")
            .and_then(|d| d.as_str())
            .and_then(|d| d.parse::<u64>().ok())
            .unwrap_or(0);
    }
    if let Some(streams) = v.get("streams").and_then(|s| s.as_array()) {
        for s in streams {
            match s.get("codec_type").and_then(|c| c.as_str()) {
                Some("video") if !info.has_video => {
                    info.has_video = true;
                    info.width = s.get("width").and_then(|w| w.as_u64()).unwrap_or(0) as u32;
                    info.height = s.get("height").and_then(|h| h.as_u64()).unwrap_or(0) as u32;
                    info.codec = s
                        .get("codec_name")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();
                    info.fps = s
                        .get("avg_frame_rate")
                        .and_then(|f| f.as_str())
                        .and_then(parse_rate)
                        .unwrap_or(0.0);
                }
                Some("audio") => info.has_audio = true,
                _ => {}
            }
        }
    }
    Ok(info)
}

fn parse_rate(s: &str) -> Option<f32> {
    if let Some((a, b)) = s.split_once('/') {
        let a: f32 = a.parse().ok()?;
        let b: f32 = b.parse().ok()?;
        if b == 0.0 {
            return None;
        }
        Some(a / b)
    } else {
        s.parse().ok()
    }
}

/* ================================================================ 常用操作 */

pub async fn make_thumb(state: &AppState, src: &Path, dst: &Path, at_sec: f32) -> Result<()> {
    let ffmpeg = ffmpeg_path(state)?;
    if let Some(p) = dst.parent() {
        crate::store::ensure_dir(p)?;
    }
    let args = vec![
        "-y".into(),
        "-ss".into(),
        format!("{:.3}", at_sec.max(0.0)),
        "-i".into(),
        src.to_string_lossy().to_string(),
        "-frames:v".into(),
        "1".into(),
        "-vf".into(),
        "scale=360:-2".into(),
        "-q:v".into(),
        "4".into(),
        dst.to_string_lossy().to_string(),
    ];
    run(&ffmpeg, &args).await.map(|_| ())
}

/// 转成 whisper 需要的 16kHz 单声道 wav。
pub async fn extract_wav16k(state: &AppState, src: &Path, dst: &Path) -> Result<()> {
    let ffmpeg = ffmpeg_path(state)?;
    if let Some(p) = dst.parent() {
        crate::store::ensure_dir(p)?;
    }
    let args = vec![
        "-y".into(),
        "-i".into(),
        src.to_string_lossy().to_string(),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        "16000".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        dst.to_string_lossy().to_string(),
    ];
    run(&ffmpeg, &args).await.map(|_| ())
}

/// 找系统里可用的字体文件。
///
/// 必须显式给 drawtext 指定 fontfile：不指定时 ffmpeg 会走 fontconfig，
/// 而 fontconfig 找不到默认配置（Windows 上很常见）时会直接崩掉进程。
fn find_font() -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(windows) {
        &[
            "C:/Windows/Fonts/msyh.ttc",
            "C:/Windows/Fonts/msyhl.ttc",
            "C:/Windows/Fonts/simhei.ttf",
            "C:/Windows/Fonts/segoeui.ttf",
            "C:/Windows/Fonts/arial.ttf",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Helvetica.ttc",
            "/Library/Fonts/Arial.ttf",
        ]
    } else {
        &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ]
    };
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

/// 过滤图里引用路径的转义规则。
fn escape_filter_path(p: &std::path::Path) -> String {
    p.to_string_lossy()
        .replace('\\', "/")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

/// drawtext 文本转义。中文标点直接替代危险字符，省掉一层转义。
fn drawtext_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ':' => '：',
            '\'' => '’',
            '"' => '“',
            '\\' | '/' => '-',
            '%' => '％',
            ',' => '，',
            '[' | ']' => '（',
            ';' => '；',
            '\n' | '\r' | '\t' => ' ',
            c => c,
        })
        .take(60)
        .collect()
}

/// 生成 drawtext 片段；没有可用字体时返回 None，调用方退化为纯色块。
fn drawtext_filter(label: &str, fontsize_expr: &str) -> Option<String> {
    let font = find_font()?;
    let text = drawtext_escape(label);
    if text.trim().is_empty() {
        return None;
    }
    Some(format!(
        "drawtext=fontfile='{}':text='{}':fontcolor=0xE6E6EA:fontsize={}:x=(w-text_w)/2:y=(h-text_h)/2",
        escape_filter_path(&font),
        text,
        fontsize_expr
    ))
}

/// 生成一张占位图（没有配生图接口时，用来把整条链路跑通）。
pub async fn placeholder_image(
    state: &AppState,
    label: &str,
    width: u32,
    height: u32,
    dst: &Path,
) -> Result<()> {
    let ffmpeg = ffmpeg_path(state)?;
    if let Some(p) = dst.parent() {
        crate::store::ensure_dir(p)?;
    }
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!("color=c=0x1E2230:s={width}x{height}"),
        "-frames:v".into(),
        "1".into(),
        "-update".into(),
        "1".into(),
    ];
    if let Some(f) = drawtext_filter(label, "h/16") {
        args.push("-vf".into());
        args.push(f);
    }
    args.push(dst.to_string_lossy().to_string());
    run(&ffmpeg, &args).await.map(|_| ())
}

/// 生成一段带音轨的占位视频。
pub async fn placeholder_video(
    state: &AppState,
    label: &str,
    width: u32,
    height: u32,
    seconds: f32,
    dst: &Path,
) -> Result<()> {
    let ffmpeg = ffmpeg_path(state)?;
    if let Some(p) = dst.parent() {
        crate::store::ensure_dir(p)?;
    }
    let filter = match drawtext_filter(label, "h/20") {
        Some(f) => format!("{f},format=yuv420p"),
        None => "format=yuv420p".to_string(),
    };
    let args = vec![
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!("color=c=0x1B2030:s={width}x{height}:d={seconds:.2}:r=30"),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!("sine=frequency=320:duration={seconds:.2}"),
        "-vf".into(),
        filter,
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "veryfast".into(),
        "-crf".into(),
        "26".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "96k".into(),
        "-shortest".into(),
        dst.to_string_lossy().to_string(),
    ];
    run(&ffmpeg, &args).await.map(|_| ())
}

/* ============================================================== 时间线导出 */

pub fn timeline_has_clips(tl: &Timeline) -> bool {
    tl.tracks.iter().any(|t| t.clips.iter().any(|c| c.enabled))
}

/// 按时间线导出成片。
///
/// 实现取的是「够用且可预测」的路线：
/// - 主视频轨的片段按 startSec 排序后依次 concat；
/// - 每个片段自己的音轨一起拼进来，没有音轨的用等长静音补齐
///   （concat 过滤器要求各段流布局一致，不能有的有声音有的没有）；
/// - 音频轨上的片段混在拼接结果的音轨之上；
/// - 字幕可选烧录。
/// 转场、变速曲线、多轨叠加合成留到后续版本。
pub async fn render_timeline(
    state: &AppState,
    tl: &Timeline,
    out: &Path,
    burn_subtitles: Option<&Path>,
) -> Result<()> {
    let ffmpeg = ffmpeg_path(state)?;
    if let Some(p) = out.parent() {
        crate::store::ensure_dir(p)?;
    }

    let video_clips: Vec<&crate::models::Clip> = {
        let mut v: Vec<&crate::models::Clip> = tl
            .tracks
            .iter()
            .filter(|t| matches!(t.kind, crate::models::TrackKind::Video))
            .flat_map(|t| t.clips.iter())
            .filter(|c| c.enabled && Path::new(&c.source).exists())
            .collect();
        v.sort_by(|a, b| {
            a.start_sec
                .partial_cmp(&b.start_sec)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        v
    };

    if video_clips.is_empty() {
        return Err(AppError::invalid("时间线上没有可用片段（检查片段源文件是否存在）"));
    }

    let w = tl.width.max(16);
    let h = tl.height.max(16);
    let fps = tl.fps.max(1);

    let audio_clips: Vec<&crate::models::Clip> = {
        let mut v: Vec<&crate::models::Clip> = tl
            .tracks
            .iter()
            .filter(|t| matches!(t.kind, crate::models::TrackKind::Audio))
            .flat_map(|t| t.clips.iter())
            .filter(|c| c.enabled && Path::new(&c.source).exists())
            .collect();
        v.sort_by(|a, b| {
            a.start_sec
                .partial_cmp(&b.start_sec)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        v
    };

    // 先探一遍每个片段有没有音轨，决定要不要补静音
    let mut has_audio: Vec<bool> = Vec::with_capacity(video_clips.len());
    for c in &video_clips {
        let info = probe(state, Path::new(&c.source)).await;
        has_audio.push(info.map(|i| i.has_audio).unwrap_or(false));
    }
    let any_audio = has_audio.iter().any(|x| *x) || !audio_clips.is_empty();

    let mut args: Vec<String> = vec!["-y".into()];
    for c in &video_clips {
        args.push("-i".into());
        args.push(c.source.clone());
    }
    let audio_base = video_clips.len();
    for c in &audio_clips {
        args.push("-i".into());
        args.push(c.source.clone());
    }

    let mut filters: Vec<String> = vec![];

    for (i, c) in video_clips.iter().enumerate() {
        let speed = if c.speed > 0.01 { c.speed } else { 1.0 };
        let dur = (c.out_sec.max(c.in_sec + 0.05) - c.in_sec.max(0.0)) / speed;
        let mut chain = format!(
            "[{i}:v]trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS",
            c.in_sec.max(0.0),
            c.out_sec.max(c.in_sec + 0.05)
        );
        if (speed - 1.0).abs() > 0.01 {
            chain.push_str(&format!(",setpts=PTS/{speed:.4}"));
        }
        chain.push_str(&format!(
            ",scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={fps}[v{i}]"
        ));
        filters.push(chain);

        if any_audio {
            if has_audio[i] {
                let mut a = format!(
                    "[{i}:a]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS",
                    c.in_sec.max(0.0),
                    c.out_sec.max(c.in_sec + 0.05)
                );
                if (speed - 1.0).abs() > 0.01 {
                    a.push_str(&format!(",atempo={:.4}", speed.clamp(0.5, 2.0)));
                }
                if (c.volume - 1.0).abs() > 0.01 {
                    a.push_str(&format!(",volume={:.3}", c.volume.max(0.0)));
                }
                a.push_str(
                    ",aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo[a_own]",
                );
                a = a.replace("[a_own]", &format!("[a{i}]"));
                filters.push(a);
            } else {
                filters.push(format!(
                    "anullsrc=r=48000:cl=stereo,atrim=0:{dur:.3},asetpts=PTS-STARTPTS,aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo[a{i}]"
                ));
            }
        }
    }

    // concat 的输入必须「视频-音频」交替排列：v0 a0 v1 a1 …
    let video_inputs: String = (0..video_clips.len()).map(|i| format!("[v{i}]")).collect();

    let mut vlabel: String;
    let mut alabel: Option<String> = None;

    if any_audio {
        let interleaved: String = (0..video_clips.len())
            .map(|i| format!("[v{i}][a{i}]"))
            .collect();
        filters.push(format!(
            "{interleaved}concat=n={}:v=1:a=1[vcat][acat]",
            video_clips.len()
        ));
        vlabel = "[vcat]".into();

        if audio_clips.is_empty() {
            alabel = Some("[acat]".into());
        } else {
            for (j, c) in audio_clips.iter().enumerate() {
                let idx = audio_base + j;
                filters.push(format!(
                    "[{idx}:a]atrim=start={:.3}:end={:.3},asetpts=N/SR/TB,volume={:.3},adelay={:.0}|{:.0},aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo[a{j}]",
                    c.in_sec.max(0.0),
                    c.out_sec.max(c.in_sec + 0.05),
                    c.volume.max(0.0),
                    c.start_sec.max(0.0) * 1000.0,
                    c.start_sec.max(0.0) * 1000.0
                ));
            }
            let mut mix = String::from("[acat]");
            for j in 0..audio_clips.len() {
                mix.push_str(&format!("[a{j}]"));
            }
            filters.push(format!(
                "{mix}amix=inputs={}:duration=first:dropout_transition=0:normalize=0[aout]",
                audio_clips.len() + 1
            ));
            alabel = Some("[aout]".into());
        }
    } else {
        filters.push(format!(
            "{video_inputs}concat=n={}:v=1:a=0[vcat]",
            video_clips.len()
        ));
        vlabel = "[vcat]".into();
    }

    if let Some(subs) = burn_subtitles {
        let escaped = escape_filter_path(subs);
        filters.push(format!(
            "{vlabel}subtitles='{escaped}':force_style='FontSize=18,Outline=1'[vsub]"
        ));
        vlabel = "[vsub]".to_string();
    }

    args.push("-filter_complex".into());
    args.push(filters.join(";"));
    args.push("-map".into());
    args.push(vlabel);
    if let Some(a) = &alabel {
        args.push("-map".into());
        args.push(a.clone());
    }
    args.extend(
        [
            "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p",
            "-movflags", "+faststart", "-r",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    args.push(fps.to_string());
    if alabel.is_some() {
        args.extend(
            ["-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-ac", "2"]
                .iter()
                .map(|s| s.to_string()),
        );
    }
    args.push(out.to_string_lossy().to_string());

    run(&ffmpeg, &args).await.map(|_| ())
}
