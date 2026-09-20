/**
 * 工作台数据模型。
 * 与 src-tauri/src/models.rs 中的 serde 结构一一对应（Rust 侧统一 camelCase 序列化）。
 * 改动这里时必须同步改 Rust 侧，两边字段名不允许漂移。
 */

export type PanelId =
  | "script"
  | "style"
  | "storyboard"
  | "asset"
  | "previz"
  | "prompt"
  | "video"
  | "edit"
  | "subtitle";

export interface PanelMeta {
  id: PanelId;
  index: number;
  title: string;
  subtitle: string;
}

export const PANELS: PanelMeta[] = [
  { id: "script", index: 1, title: "剧本", subtitle: "章节 / 大纲 / 正文" },
  { id: "style", index: 2, title: "风格", subtitle: "画面风格圣经" },
  { id: "storyboard", index: 3, title: "分镜", subtitle: "镜头表" },
  { id: "asset", index: 4, title: "资产", subtitle: "人物三视图 / 场景 / 物件" },
  { id: "previz", index: 5, title: "3D预演", subtitle: "白模走位 / 机位设计" },
  { id: "prompt", index: 6, title: "视频提示词", subtitle: "提示词与资产配对" },
  { id: "video", index: 7, title: "生视频", subtitle: "调用视频模型出片" },
  { id: "edit", index: 8, title: "剪辑", subtitle: "时间线与导出" },
  { id: "subtitle", index: 9, title: "字幕", subtitle: "whisper 转写" },
];

/** 面板完成度，由后端按数据实时判定（src-tauri/src/progress.rs） */
export interface PanelProgress {
  panel: PanelId;
  total: number;
  done: number;
  percent: number;
  /** 阻塞项：还差什么才能完成 */
  blockers: string[];
}

/* ------------------------------------------------------------------ 项目 */

export interface Manifest {
  schemaVersion: number;
  id: string;
  name: string;
  genre: string;
  logline: string;
  episodeCountHint: number;
  aspectRatio: string;
  createdAt: string;
  updatedAt: string;
  appVersion: string;
}

export interface RecentProject {
  path: string;
  name: string;
  id: string;
  openedAt: string;
  exists: boolean;
}

/* ------------------------------------------------------------- 项目圣经 */

export interface CharacterCard {
  id: string;
  name: string;
  aliases: string[];
  role: string;
  age: string;
  appearance: string;
  personality: string;
  arc: string;
  notes: string;
}

export interface Bible {
  synopsis: string;
  sellingPoints: string[];
  world: string;
  tone: string;
  audience: string;
  characters: CharacterCard[];
  notes: string;
}

/* --------------------------------------------------------------- 剧本 */

export type ChapterStatus = "empty" | "draft" | "written" | "locked";

export interface ChapterMeta {
  id: string;
  index: number;
  title: string;
  status: ChapterStatus;
  summary: string;
  characters: string[];
  scenes: string[];
  wordCount: number;
  updatedAt: string;
}

export interface Chapter {
  meta: ChapterMeta;
  content: string;
}

/* --------------------------------------------------------------- 风格 */

export interface StyleSpec {
  id: string;
  name: string;
  prompt: string;
  negative: string;
  palette: string[];
  lighting: string;
  lens: string;
  filmStock: string;
  aspectRatio: string;
  motionStyle: string;
  notes: string;
}

export interface StylePreset {
  id: string;
  name: string;
  category: string;
  prompt: string;
  negative: string;
  palette: string[];
  lighting: string;
  lens: string;
  filmStock: string;
  motionStyle: string;
}

export interface StyleState {
  activePresetId: string | null;
  spec: StyleSpec;
}

/* --------------------------------------------------------------- 分镜 */

export type ShotStatus = "draft" | "ready" | "prompted" | "generated" | "locked";

/** 资产引用：指向某个资产的某个视图（视图为空表示整个资产） */
export interface AssetRef {
  assetId: string;
  viewId: string | null;
}

export interface Shot {
  id: string;
  chapterId: string;
  index: number;
  sceneId: string | null;
  location: string;
  timeOfDay: string;
  interior: boolean;
  shotSize: string;
  camera: string;
  cameraMove: string;
  durationSec: number;
  characters: string[];
  props: string[];
  action: string;
  dialogue: string;
  narration: string;
  sfx: string;
  bgm: string;
  imagePrompt: string;
  videoPrompt: string;
  refImages: AssetRef[];
  status: ShotStatus;
}

/* --------------------------------------------------------------- 资产 */

export type AssetKind =
  | "character"
  | "scene"
  | "prop"
  | "costume"
  | "vehicle"
  | "effect"
  | "other";

export type ViewKind =
  | "front"
  | "side"
  | "back"
  | "threeQuarter"
  | "fullBody"
  | "closeUp"
  | "wide"
  | "medium"
  | "birdView"
  | "custom";

export type AssetStatus = "planned" | "queued" | "running" | "done" | "failed";

export interface AssetView {
  id: string;
  kind: ViewKind;
  label: string;
  prompt: string;
  negative: string;
  /** 手动参考图（本地文件或资产图路径） */
  refImages?: string[];
  file: string | null;
  thumb: string | null;
  seed: number | null;
  model: string | null;
  providerId: string | null;
  status: AssetStatus;
  error: string | null;
  jobId: string | null;
  createdAt: string;
}

export interface Asset {
  id: string;
  kind: AssetKind;
  name: string;
  aliases: string[];
  description: string;
  tags: string[];
  /** 一致性锁定项：出图时必须原样带上的特征，如 "左眉疤" */
  lockedTraits: string[];
  views: AssetView[];
  createdAt: string;
  updatedAt: string;
}

/* --------------------------------------------------------- 视频提示词 */

export type PromptStatus = "draft" | "ready" | "bound" | "locked";

export interface VideoPrompt {
  id: string;
  shotId: string;
  index: number;
  prompt: string;
  negative: string;
  motion: string;
  cameraMove: string;
  durationSec: number;
  firstFrame: AssetRef | null;
  lastFrame: AssetRef | null;
  refs: AssetRef[];
  /** 手动参考图（本地文件路径） */
  refImages?: string[];
  modelHint: string | null;
  seed: number | null;
  status: PromptStatus;
  updatedAt: string;
}

/* --------------------------------------------------------------- 视频 */

export interface VideoTake {
  id: string;
  shotId: string;
  promptId: string | null;
  providerId: string;
  model: string;
  status: AssetStatus;
  progress: number;
  file: string | null;
  thumb: string | null;
  seed: number | null;
  durationSec: number | null;
  error: string | null;
  jobId: string | null;
  createdAt: string;
}

/* --------------------------------------------------------------- 剪辑 */

export type TrackKind = "video" | "audio" | "subtitle" | "overlay";

export interface Clip {
  id: string;
  /** 源文件绝对路径 */
  source: string;
  /** 关联的镜头 / 生成结果，便于追溯 */
  shotId: string | null;
  takeId: string | null;
  label: string;
  /** 源内起点（秒） */
  inSec: number;
  /** 源内终点（秒） */
  outSec: number;
  /** 时间线起点（秒） */
  startSec: number;
  speed: number;
  volume: number;
  enabled: boolean;
}

export interface Track {
  id: string;
  kind: TrackKind;
  name: string;
  muted: boolean;
  locked: boolean;
  clips: Clip[];
}

export interface Timeline {
  durationSec: number;
  fps: number;
  width: number;
  height: number;
  tracks: Track[];
  updatedAt: string;
}

/* --------------------------------------------------------------- 字幕 */

export interface Cue {
  index: number;
  start: number;
  end: number;
  text: string;
}

export interface SubtitleDoc {
  id: string;
  name: string;
  source: string;
  language: string;
  model: string;
  cues: Cue[];
  file: string | null;
  createdAt: string;
}

/* ----------------------------------------------------------- checklist */

export type Actor = "user" | "agent";

export interface ChecklistItem {
  id: string;
  panel: PanelId;
  text: string;
  done: boolean;
  doneBy: Actor | null;
  doneAt: string | null;
  note: string;
  /** 由系统按规则自动判定的项，agent 不能直接勾 */
  auto: boolean;
}

export interface Checklist {
  items: ChecklistItem[];
}

/* ----------------------------------------------------- 项目聚合快照 */

export interface ProjectSnapshot {
  root: string;
  manifest: Manifest;
  bible: Bible;
  chapters: ChapterMeta[];
  style: StyleState;
  shots: Shot[];
  assets: Asset[];
  prompts: VideoPrompt[];
  takes: VideoTake[];
  timeline: Timeline;
  subtitles: SubtitleDoc[];
  checklist: Checklist;
}

/* ------------------------------------------------------------ 任务队列 */

export type JobKind =
  | "image"
  | "video"
  | "asr"
  | "ffmpeg"
  | "llm"
  | "download";

export type JobStatus = "queued" | "running" | "done" | "failed" | "canceled";

export interface Job {
  id: string;
  kind: JobKind;
  title: string;
  detail: string;
  status: JobStatus;
  progress: number;
  total: number;
  done: number;
  error: string | null;
  createdAt: string;
  updatedAt: string;
  finishedAt: string | null;
}

/* --------------------------------------------------------- 设置/供应商 */

export type ProviderKind = "image" | "video" | "llm";

export interface ProviderConfig {
  id: string;
  kind: ProviderKind;
  name: string;
  /** 内置适配器 id：openai | anthropic | generic-http | mock */
  adapter: string;
  baseUrl: string;
  /** 指向 secrets 中的 key 名，不直接存明文 */
  apiKeyRef: string;
  model: string;
  concurrency: number;
  timeoutSec: number;
  enabled: boolean;
  /** 适配器私有参数（generic-http 的请求模板等） */
  options: Record<string, unknown>;
}

export interface LlmProviderConfig extends ProviderConfig {
  kind: "llm";
  /** 是否声明支持视觉输入 */
  vision: boolean;
  /** 该端点是否自带前缀缓存（OpenAI 兼容多为自动） */
  prefixCache: "auto" | "explicit" | "none";
  maxTokens: number;
  temperature: number;
}

export interface AppSettings {
  theme: "dark" | "light";
  /** 新手教程是否已经走过 */
  onboarded: boolean;
  workspacePanelWidth: number;
  agentDockWidth: number;
  agentDockOpen: boolean;
  /**
   * agent 权限模式：
   * - yolo    任何操作都直接执行
   * - auto    自动改数据，只有花钱的生成操作要确认（默认）
   * - confirm 任何会改数据的操作都先确认
   */
  agentMode: "yolo" | "auto" | "confirm";
  maxToolRounds: number;
  /** 单次对话的上下文预算（token 估算），超了自动压缩 */
  contextBudget: number;
  autoCompact: boolean;
  /** 代理模式：auto 跟随系统/环境变量 / off 直连 / manual 手动填 */
  proxyMode: "auto" | "off" | "manual";
  proxyUrl: string;
  asrBackend: "auto" | "cpu" | "cuda" | "metal" | "vulkan" | "coreml";
  asrUseGpu: boolean;
  asrGpuLayers: number;
  asrThreads: number;
  asrModel: string;
  /** whisper 模型下载源，国内可换镜像 */
  asrModelBaseUrl: string;
  whisperCliPath: string;
  ffmpegPath: string;
  ffprobePath: string;
  activeLlmProviderId: string | null;
  activeImageProviderId: string | null;
  activeVideoProviderId: string | null;
  recentProjects: RecentProject[];
  providers: ProviderConfig[];
}

/* --------------------------------------------------------- ASR 能力 */

export interface AsrCapabilities {
  os: string;
  arch: string;
  cpuThreads: number;
  appleSilicon: boolean;
  cudaAvailable: boolean;
  cudaVersion: string | null;
  nvidiaGpu: string | null;
  metalSupported: boolean;
  vulkanSupported: boolean;
  directmlSupported: boolean;
  /** 本二进制是否编译进了 whisper 推理 */
  whisperCompiled: boolean;
  /** 编译期启用的后端 */
  compiledBackends: string[];
  /** 外部 whisper-cli 是否可用 */
  externalCliAvailable: boolean;
  externalCliPath: string | null;
  recommendedBackend: string;
  /** 模型存放目录（可以手动把 .bin 放进来） */
  modelsDir: string;
  installedModels: WhisperModelInfo[];
}

export interface WhisperModelInfo {
  id: string;
  label: string;
  fileName: string;
  sizeMb: number;
  downloaded: boolean;
  path: string | null;
  multilingual: boolean;
}

/* --------------------------------------------------------------- 技能 */

export interface Skill {
  id: string;
  name: string;
  /** 何时使用这个技能 —— 模型靠它决定要不要加载 */
  description: string;
  kind: string;
  /** 目录技能的路径；单文件技能为 null */
  dir: string | null;
  entry: string;
  /** 附件相对路径列表 */
  files: string[];
  chars: number;
  enabled: boolean;
  /** 存储位置：user = 全局（所有项目可用）；project = 只属于当前项目 */
  location: string;
}
