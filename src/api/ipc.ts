/**
 * Tauri 命令封装。前端不直接 invoke，统一走这里，方便类型检查和后续替换实现。
 * 命令名与 src-tauri/src/lib.rs 的 generate_handler! 一一对应。
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";

/**
 * 在浏览器里跑 `npm run dev` 时没有 Tauri 运行时，这里自动切到假后端（src/dev/mock.ts），
 * 方便调界面与截图。打包成桌面应用时走真正的 invoke，mock 也不会被打进主包。
 */
let mockModule: typeof import("@/dev/mock") | null = null;
async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    return tauriInvoke<T>(cmd, args);
  }
  mockModule ??= await import("@/dev/mock");
  return mockModule.mockInvoke(cmd, args) as Promise<T>;
}
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
  assetUpsert: (asset: Asset) => invoke<Asset>("asset_upsert", { asset }),  assetDelete: (assetId: string) => invoke<void>("asset_delete", { assetId }),
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

  /* ------------------------------------------------------- pi 引擎 */
  agentPiThinking: (level: string) => invoke<void>("agent_pi_thinking", { level }),

  /* ------------------------------------------- 生成预览（完整提示词） */
  assetPromptPreview: (assetId: string, viewId: string) =>
    invoke<{ prompt: string; negative: string }>("asset_prompt_preview", { assetId, viewId }),
  videoPromptPreview: (promptId: string) =>
    invoke<{ prompt: string; negative: string }>("video_prompt_preview", { promptId }),

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

  /* ------------------------------------------------------------ 3D预演 */
  previzGet: () => invoke<unknown>("previz_get"),
  previzPut: (scene: unknown) => invoke<void>("previz_put", { scene }),
  previzRenderBegin: () => invoke<string>("previz_render_begin"),
  previzRenderFrame: (index: number, dataB64: string) =>
    invoke<void>("previz_render_frame", { index, dataB64 }),
  previzRenderFinish: (fps: number) => invoke<string>("previz_render_finish", { fps }),

  /* -------------------------------------------------------- 导演台（预演） */
  directorSave: (content: string) => invoke<string>("director_save", { content }),
  directorLoad: () => invoke<string | null>("director_load"),
  directorResult: (callId: string, ok: boolean, data: unknown, error?: string) =>
    invoke<void>("director_result", { callId, ok, data, error }),

  /* ------------------------------------------------------------ 技能 */
  skillList: () => invoke<import("@/types/models").Skill[]>("skill_list"),
  skillRead: (id: string) => invoke<string>("skill_read", { id }),
  skillReadFile: (id: string, path: string) => invoke<string>("skill_read_file", { id, path }),
  skillImport: (paths: string[], location?: string) => invoke<string[]>("skill_import", { paths, location }),
  skillDelete: (id: string) => invoke<void>("skill_delete", { id }),
  skillSetEnabled: (id: string, enabled: boolean) =>
    invoke<void>("skill_set_enabled", { id, enabled }),
  skillOpenDir: () => invoke<string>("skill_open_dir"),

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
  agentAnswer: (toolCallId: string, answer: string) =>
    invoke<boolean>("agent_answer", { toolCallId, answer }),
  agentContext: (sessionId: string) =>
    invoke<import("@/types/agent").ContextStats>("agent_context", { sessionId }),
  agentCompact: (sessionId: string, summarize = false) =>
    invoke<import("@/types/agent").ContextStats>("agent_compact", { sessionId, summarize }),
  agentPrefixPreview: (panel: PanelId, sessionId?: string) =>
    invoke<import("@/types/agent").PrefixReport>("agent_prefix_preview", {
      panel,
      sessionId: sessionId ?? null,
    }),
};
