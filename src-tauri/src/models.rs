//! 全局数据模型。字段与 `src/types/models.ts` 严格对应。
//!
//! 稳定性约定（关系到 LLM 前缀缓存命中率）：
//! - 所有结构体用 camelCase 序列化，字段顺序即结构体声明顺序；
//! - 序列化时**不要**引入时间戳、随机数、HashMap 等不稳定内容到「稳定前缀」层，
//!   相关字段（updatedAt 等）只在落盘文件里出现，进 prompt 前会被剥离。

use serde::{Deserialize, Serialize};

pub fn new_id(prefix: &str) -> String {
    let u = uuid::Uuid::new_v4().simple().to_string();
    format!("{}_{}", prefix, &u[..10])
}

pub fn now_iso() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/* ==================================================================== 面板 */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PanelId {
    Script,
    Style,
    Storyboard,
    Asset,
    Prompt,
    Video,
    Edit,
    Subtitle,
    Checklist,
}

impl PanelId {
    pub fn as_str(&self) -> &'static str {
        match self {
            PanelId::Script => "script",
            PanelId::Style => "style",
            PanelId::Storyboard => "storyboard",
            PanelId::Asset => "asset",
            PanelId::Prompt => "prompt",
            PanelId::Video => "video",
            PanelId::Edit => "edit",
            PanelId::Subtitle => "subtitle",
            PanelId::Checklist => "checklist",
        }
    }

    pub fn all() -> Vec<PanelId> {
        vec![
            PanelId::Script,
            PanelId::Style,
            PanelId::Storyboard,
            PanelId::Asset,
            PanelId::Prompt,
            PanelId::Video,
            PanelId::Edit,
            PanelId::Subtitle,
            PanelId::Checklist,
        ]
    }
}

/// 面板完成度，驱动左侧导航徽标与 checklist 面板。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelProgress {
    pub panel: PanelId,
    pub total: u32,
    pub done: u32,
    pub percent: f32,
    /// 阻塞项：为什么还没完成，agent 与用户都读这个
    pub blockers: Vec<String>,
}

/* ================================================================== 项目 */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub genre: String,
    pub logline: String,
    pub episode_count_hint: u32,
    pub aspect_ratio: String,
    pub created_at: String,
    pub updated_at: String,
    pub app_version: String,
}

impl Manifest {
    pub fn new(name: &str) -> Self {
        let now = now_iso();
        Self {
            schema_version: 1,
            id: new_id("prj"),
            name: name.to_string(),
            genre: String::new(),
            logline: String::new(),
            episode_count_hint: 0,
            aspect_ratio: "9:16".into(),
            created_at: now.clone(),
            updated_at: now,
            app_version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    pub id: String,
    pub opened_at: String,
    pub exists: bool,
}

/* ================================================================ 项目圣经 */

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterCard {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub role: String,
    pub age: String,
    pub appearance: String,
    pub personality: String,
    pub arc: String,
    pub notes: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bible {
    pub synopsis: String,
    #[serde(default)]
    pub selling_points: Vec<String>,
    pub world: String,
    pub tone: String,
    pub audience: String,
    #[serde(default)]
    pub characters: Vec<CharacterCard>,
    pub notes: String,
}

/* ================================================================== 剧本 */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChapterStatus {
    Empty,
    Draft,
    Written,
    Locked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterMeta {
    pub id: String,
    pub index: u32,
    pub title: String,
    pub status: ChapterStatus,
    pub summary: String,
    #[serde(default)]
    pub characters: Vec<String>,
    #[serde(default)]
    pub scenes: Vec<String>,
    pub word_count: u32,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    pub meta: ChapterMeta,
    pub content: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptIndex {
    pub chapters: Vec<ChapterMeta>,
}

/* ================================================================== 风格 */

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleSpec {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub negative: String,
    #[serde(default)]
    pub palette: Vec<String>,
    pub lighting: String,
    pub lens: String,
    pub film_stock: String,
    pub aspect_ratio: String,
    pub motion_style: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StylePreset {
    pub id: String,
    pub name: String,
    pub category: String,
    pub prompt: String,
    #[serde(default)]
    pub negative: String,
    #[serde(default)]
    pub palette: Vec<String>,
    #[serde(default)]
    pub lighting: String,
    #[serde(default)]
    pub lens: String,
    #[serde(default)]
    pub film_stock: String,
    #[serde(default)]
    pub motion_style: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleState {
    pub active_preset_id: Option<String>,
    pub spec: StyleSpec,
}

/* ================================================================== 分镜 */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShotStatus {
    Draft,
    Ready,
    Prompted,
    Generated,
    Locked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRef {
    pub asset_id: String,
    pub view_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shot {
    pub id: String,
    pub chapter_id: String,
    pub index: u32,
    pub scene_id: Option<String>,
    pub location: String,
    pub time_of_day: String,
    pub interior: bool,
    pub shot_size: String,
    pub camera: String,
    pub camera_move: String,
    pub duration_sec: f32,
    #[serde(default)]
    pub characters: Vec<String>,
    #[serde(default)]
    pub props: Vec<String>,
    pub action: String,
    pub dialogue: String,
    pub narration: String,
    pub sfx: String,
    pub bgm: String,
    pub image_prompt: String,
    pub video_prompt: String,
    #[serde(default)]
    pub ref_images: Vec<AssetRef>,
    pub status: ShotStatus,
}

/* ================================================================== 资产 */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetKind {
    Character,
    Scene,
    Prop,
    Costume,
    Vehicle,
    Effect,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewKind {
    Front,
    Side,
    Back,
    ThreeQuarter,
    FullBody,
    CloseUp,
    Wide,
    Medium,
    BirdView,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetStatus {
    Planned,
    Queued,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetView {
    pub id: String,
    pub kind: ViewKind,
    pub label: String,
    pub prompt: String,
    pub negative: String,
    pub file: Option<String>,
    pub thumb: Option<String>,
    pub seed: Option<i64>,
    pub model: Option<String>,
    pub provider_id: Option<String>,
    pub status: AssetStatus,
    pub error: Option<String>,
    pub job_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub kind: AssetKind,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub locked_traits: Vec<String>,
    #[serde(default)]
    pub views: Vec<AssetView>,
    pub created_at: String,
    pub updated_at: String,
}

/* ============================================================== 视频提示词 */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptStatus {
    Draft,
    Ready,
    Bound,
    Locked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoPrompt {
    pub id: String,
    pub shot_id: String,
    pub index: u32,
    pub prompt: String,
    pub negative: String,
    pub motion: String,
    pub camera_move: String,
    pub duration_sec: f32,
    pub first_frame: Option<AssetRef>,
    pub last_frame: Option<AssetRef>,
    #[serde(default)]
    pub refs: Vec<AssetRef>,
    pub model_hint: Option<String>,
    pub seed: Option<i64>,
    pub status: PromptStatus,
    pub updated_at: String,
}

/* ================================================================== 视频 */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoTake {
    pub id: String,
    pub shot_id: String,
    pub prompt_id: Option<String>,
    pub provider_id: String,
    pub model: String,
    pub status: AssetStatus,
    pub progress: f32,
    pub file: Option<String>,
    pub thumb: Option<String>,
    pub seed: Option<i64>,
    pub duration_sec: Option<f32>,
    pub error: Option<String>,
    pub job_id: Option<String>,
    pub created_at: String,
}

/* ================================================================== 剪辑 */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrackKind {
    Video,
    Audio,
    Subtitle,
    Overlay,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: String,
    pub source: String,
    pub shot_id: Option<String>,
    pub take_id: Option<String>,
    pub label: String,
    pub in_sec: f32,
    pub out_sec: f32,
    pub start_sec: f32,
    pub speed: f32,
    pub volume: f32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub kind: TrackKind,
    pub name: String,
    pub muted: bool,
    pub locked: bool,
    #[serde(default)]
    pub clips: Vec<Clip>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    pub duration_sec: f32,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub tracks: Vec<Track>,
    pub updated_at: String,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            duration_sec: 0.0,
            fps: 30,
            width: 1080,
            height: 1920,
            tracks: vec![
                Track {
                    id: "tr_video".into(),
                    kind: TrackKind::Video,
                    name: "主视频".into(),
                    muted: false,
                    locked: false,
                    clips: vec![],
                },
                Track {
                    id: "tr_audio".into(),
                    kind: TrackKind::Audio,
                    name: "音频".into(),
                    muted: false,
                    locked: false,
                    clips: vec![],
                },
                Track {
                    id: "tr_sub".into(),
                    kind: TrackKind::Subtitle,
                    name: "字幕".into(),
                    muted: false,
                    locked: false,
                    clips: vec![],
                },
                Track {
                    id: "tr_overlay".into(),
                    kind: TrackKind::Overlay,
                    name: "叠加".into(),
                    muted: false,
                    locked: false,
                    clips: vec![],
                },
            ],
            updated_at: now_iso(),
        }
    }
}

/* ================================================================== 字幕 */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub index: u32,
    pub start: f32,
    pub end: f32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleDoc {
    pub id: String,
    pub name: String,
    pub source: String,
    pub language: String,
    pub model: String,
    #[serde(default)]
    pub cues: Vec<Cue>,
    pub file: Option<String>,
    pub created_at: String,
}

/* ============================================================== checklist */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Actor {
    User,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItem {
    pub id: String,
    pub panel: PanelId,
    pub text: String,
    pub done: bool,
    pub done_by: Option<Actor>,
    pub done_at: Option<String>,
    pub note: String,
    /// 由数据自动判定的条目，agent 只能读不能改
    pub auto: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checklist {
    #[serde(default)]
    pub items: Vec<ChecklistItem>,
}

/* ============================================================ 项目快照 */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
    pub root: String,
    pub manifest: Manifest,
    pub bible: Bible,
    pub chapters: Vec<ChapterMeta>,
    pub style: StyleState,
    pub shots: Vec<Shot>,
    pub assets: Vec<Asset>,
    pub prompts: Vec<VideoPrompt>,
    pub takes: Vec<VideoTake>,
    pub timeline: Timeline,
    pub subtitles: Vec<SubtitleDoc>,
    pub checklist: Checklist,
}

/* ================================================================ 任务队列 */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    Image,
    Video,
    Asr,
    Ffmpeg,
    Llm,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub kind: JobKind,
    pub title: String,
    pub detail: String,
    pub status: JobStatus,
    pub progress: f32,
    pub total: u32,
    pub done: u32,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

/* ================================================================== 设置 */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Image,
    Video,
    Llm,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub name: String,
    /// 内置适配器：openai | anthropic | generic-http | mock
    pub adapter: String,
    pub base_url: String,
    /// 密钥在 secrets 里的键名，不存明文
    pub api_key_ref: String,
    pub model: String,
    pub concurrency: u32,
    pub timeout_sec: u64,
    pub enabled: bool,
    #[serde(default)]
    pub options: serde_json::Value,
}

impl Default for ProviderKind {
    fn default() -> Self {
        ProviderKind::Llm
    }
}

/// whisper 模型默认下载源
pub const DEFAULT_WHISPER_BASE_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: String,
    pub workspace_panel_width: u32,
    pub agent_dock_width: u32,
    pub agent_dock_open: bool,
    pub confirm_costly_tools: bool,
    pub max_tool_rounds: u32,
    pub asr_backend: String,
    pub asr_use_gpu: bool,
    pub asr_gpu_layers: i32,
    pub asr_threads: i32,
    /// 代理模式：auto（跟随系统/环境变量）/ off（直连）/ manual（手动填）
    pub proxy_mode: String,
    pub proxy_url: String,
    pub asr_model: String,
    /// whisper 模型下载源。国内网络访问不了 huggingface 时换成镜像，
    /// 例如 https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main
    pub asr_model_base_url: String,
    pub whisper_cli_path: String,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub active_llm_provider_id: Option<String>,
    pub active_image_provider_id: Option<String>,
    pub active_video_provider_id: Option<String>,
    #[serde(default)]
    pub recent_projects: Vec<RecentProject>,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            workspace_panel_width: 260,
            agent_dock_width: 420,
            agent_dock_open: true,
            confirm_costly_tools: true,
            max_tool_rounds: 12,
            asr_backend: "auto".into(),
            asr_use_gpu: true,
            asr_gpu_layers: 999,
            asr_threads: 0,
            proxy_mode: "auto".into(),
            proxy_url: String::new(),
            asr_model: "ggml-large-v3-turbo".into(),
            asr_model_base_url: DEFAULT_WHISPER_BASE_URL.into(),
            whisper_cli_path: String::new(),
            ffmpeg_path: String::new(),
            ffprobe_path: String::new(),
            active_llm_provider_id: None,
            active_image_provider_id: None,
            active_video_provider_id: None,
            recent_projects: vec![],
            providers: vec![],
        }
    }
}
