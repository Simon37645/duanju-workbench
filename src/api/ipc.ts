/**
 * Tauri 命令封装。前端不直接 invoke，统一走这里，方便类型检查和后续替换实现。
 * 命令名与 src-tauri/src/lib.rs 的 generate_handler! 一一对应。
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AsrCapabilities,
  Asset,
  AssetView,
  Bible,
  Chapter,
  ChapterMeta,
  Checklist,
  ChecklistItem,
  Cue,
  Job,
  Manifest,
  PanelId,
  PanelProgress,
  ProjectSnapshot,
  ProviderConfig,
  Shot,
  StylePreset,
  StyleState,
  SubtitleDoc,
  Timeline,
  VideoPrompt,
  VideoTake,
} from "@/types/models";

export interface TranscribeOptions {
  file: string;
  language: string;
  modelId: string;
  useGpu: boolean;
  threads: number;
  maxLineChars: number;
  name: string;
}

export interface MediaInfo {
  path: string;
  durationSec: number;
  width: number;
  height: number;
  fps: number;
  hasAudio: boolean;
  hasVideo: boolean;
  codec: string;
  sizeBytes: number;
}

/** 后端返回的错误结构 */
export interface AppErrorShape {
  code: string;
  message: string;
}

export function errorText(e: unknown): string {
  if (!e) return "未知错误";
  if (typeof e === "string") return e;
  const o = e as AppErrorShape;
  if (o.message) return o.message;
  return JSON.stringify(e);
}

export const api = {
  /* ------------------------------------------------------------ 项目 */
  projectCreate: (parentDir: string, name: string) =>
    invoke<ProjectSnapshot>("project_create", { parentDir, name }),
  projectOpen: (root: string) => invoke<ProjectSnapshot>("project_open", { root }),
  projectClose: () => invoke<void>("project_close"),
  projectSnapshot: () => invoke<ProjectSnapshot>("project_snapshot"),
  projectProgress: () => invoke<PanelProgress[]>("project_progress"),
  projectUpdateManifest: (patch: Partial<Manifest>) =>
    invoke<Manifest>("project_update_manifest", { patch }),
  projectPaths: (root: string) =>
    invoke<Record<string, string>>("project_paths", { root }),

  /* ------------------------------------------------------------ 剧本 */
  scriptReadChapter: (chapterId: string) =>
    invoke<Chapter>("script_read_chapter", { chapterId }),
  scriptSaveChapter: (chapterId: string, content: string) =>
    invoke<ChapterMeta>("script_save_chapter", { chapterId, content }),
  scriptAddChapter: (title?: string, summary?: string) =>
    invoke<ChapterMeta>("script_add_chapter", { title, summary }),
  scriptUpdateChapterMeta: (chapterId: string, patch: Record<string, unknown>) =>
    invoke<ChapterMeta>("script_update_chapter_meta", { chapterId, patch }),
  scriptDeleteChapter: (chapterId: string) =>
    invoke<void>("script_delete_chapter", { chapterId }),
  scriptReorderChapters: (ids: string[]) =>
    invoke<void>("script_reorder_chapters", { ids }),

  /* ------------------------------------------------------ 圣经 / 风格 */
  bibleGet: () => invoke<Bible>("bible_get"),
  bibleSet: (bible: Bible) => invoke<Bible>("bible_set", { bible }),
  styleGet: () => invoke<StyleState>("style_get"),
  styleSet: (style: StyleState) => invoke<StyleState>("style_set", { style }),
  stylePresets: () => invoke<StylePreset[]>("style_presets"),
  storyboardVocab: () =>
    invoke<{ shotSizes: string[]; cameraMoves: string[]; timeOfDay: string[] }>(
      "storyboard_vocab",
    ),

  /* ------------------------------------------------------------ 分镜 */
  storyboardSetShots: (chapterId: string, shots: Shot[]) =>
    invoke<Shot[]>("storyboard_set_shots", { chapterId, shots }),
  storyboardUpsertShot: (shot: Shot) => invoke<Shot>("storyboard_upsert_shot", { shot }),
  storyboardDeleteShot: (shotId: string) =>
    invoke<void>("storyboard_delete_shot", { shotId }),

  /* ------------------------------------------------------------ 资产 */
  assetUpsert: (asset: Asset) => invoke<Asset>("asset_upsert", { asset }),
  assetDelete: (assetId: string) => invoke<void>("asset_delete", { assetId }),
  assetPlanViews: (assetId: string, views: AssetView[]) =>
    invoke<Asset>("asset_plan_views", { assetId, views }),
  assetGenerateViews: (assetId: string, viewIds: string[], force = false) =>
    invoke<string[]>("asset_generate_views", { assetId, viewIds, force }),
  assetViewDelete: (assetId: string, viewId: string) =>
    invoke<void>("asset_view_delete", { assetId, viewId }),

  /* -------------------------------------------------------- 提示词 */
  promptUpsert: (prompt: VideoPrompt) => invoke<VideoPrompt>("prompt_upsert", { prompt }),
  promptDelete: (promptId: string) => invoke<void>("prompt_delete", { promptId }),
  promptAutofill: (chapterId?: string) =>
    invoke<number>("prompt_autofill", { chapterId: chapterId ?? null }),

  /* ---------------------------------------------------------- 生视频 */
  videoGenerate: (shotIds: string[], providerId?: string, model?: string) =>
    invoke<string[]>("video_generate", {
      shotIds,
      providerId: providerId ?? null,
      model: model ?? null,
    }),
  videoDeleteTake: (takeId: string) => invoke<void>("video_delete_take", { takeId }),

  /* ------------------------------------------------------------ 剪辑 */
  editSetTimeline: (timeline: Timeline) =>
    invoke<Timeline>("edit_set_timeline", { timeline }),
  editBuildFromTakes: () => invoke<number>("edit_build_from_takes"),
  editRender: (burnSubtitles = true) =>
    invoke<string>("edit_render", { burnSubtitles }),

  /* ------------------------------------------------------------ 字幕 */
  subtitleTranscribe: (options: TranscribeOptions) =>
    invoke<string>("subtitle_transcribe", { options }),
  subtitleGet: (subtitleId: string) => invoke<SubtitleDoc>("subtitle_get", { subtitleId }),
  subtitleSaveCues: (subtitleId: string, cues: Cue[]) =>
    invoke<SubtitleDoc>("subtitle_save_cues", { subtitleId, cues }),
  subtitleDelete: (subtitleId: string) =>
    invoke<void>("subtitle_delete", { subtitleId }),

  /* -------------------------------------------------------- checklist */
  checklistGet: () => invoke<Checklist>("checklist_get"),
  checklistAdd: (panel: PanelId, text: string) =>
    invoke<ChecklistItem>("checklist_add", { panel, text }),
  checklistToggle: (id: string, done: boolean) =>
    invoke<void>("checklist_toggle", { id, done }),
  checklistDelete: (id: string) => invoke<void>("checklist_delete", { id }),
  checklistResetDefaults: () => invoke<Checklist>("checklist_reset_defaults"),

  /* ------------------------------------------------------------- 任务 */
  jobsList: () => invoke<Job[]>("jobs_list"),
  jobCancel: (jobId: string) => invoke<boolean>("job_cancel", { jobId }),
  jobsClearFinished: () => invoke<void>("jobs_clear_finished"),

  /* ------------------------------------------------------------- 设置 */
  settingsGet: () => invoke<AppSettings>("settings_get"),
  settingsSet: (settings: AppSettings) => invoke<AppSettings>("settings_set", { settings }),
  providerUpsert: (provider: ProviderConfig) =>
    invoke<ProviderConfig>("provider_upsert", { provider }),
  providerDelete: (providerId: string) => invoke<void>("provider_delete", { providerId }),
  providerSetSecret: (keyRef: string, value: string) =>
    invoke<void>("provider_set_secret", { keyRef, value }),
  secretsStatus: () => invoke<Record<string, boolean>>("secrets_status"),
  providerTest: (providerId: string) => invoke<string>("provider_test", { providerId }),

  /* -------------------------------------------------------- 网络诊断 */
  proxyStatus: () =>
    invoke<{
      mode: string;
      manualUrl: string;
      envProxy: string | null;
      systemProxy: string | null;
      effective: string | null;
      noProxy: string;
    }>("proxy_status"),
  netTest: (url?: string) => invoke<string>("net_test", { url: url ?? null }),

  /* -------------------------------------------------------- 媒体/ASR */
  mediaProbe: (path: string) => invoke<MediaInfo>("media_probe", { path }),
  mediaSidecarStatus: () =>
    invoke<Record<string, { ok: boolean; path?: string; error?: string }>>(
      "media_sidecar_status",
    ),
  asrCapabilities: () => invoke<AsrCapabilities>("asr_capabilities"),
  asrDownloadModel: (modelId: string) => invoke<string>("asr_download_model", { modelId }),
  asrDeleteModel: (modelId: string) => invoke<void>("asr_delete_model", { modelId }),

  /* ------------------------------------------------------------ agent */
  agentSessions: () => invoke<import("@/types/agent").AgentSession[]>("agent_sessions"),
  agentSessionGet: (sessionId: string) =>
    invoke<import("@/types/agent").AgentSession | null>("agent_session_get", { sessionId }),
  agentSessionDelete: (sessionId: string) =>
    invoke<void>("agent_session_delete", { sessionId }),
  agentSessionNew: (panel: PanelId) =>
    invoke<import("@/types/agent").AgentSession>("agent_session_new", { panel }),
  agentApprove: (toolCallId: string, approved: boolean) =>
    invoke<boolean>("agent_approve", { toolCallId, approved }),
  agentPrefixPreview: (panel: PanelId, sessionId?: string) =>
    invoke<import("@/types/agent").PrefixReport>("agent_prefix_preview", {
      panel,
      sessionId: sessionId ?? null,
    }),
};
