/**
 * 浏览器预览用的假后端。
 *
 * 在浏览器里跑 `npm run dev` 时（没有 Tauri 运行时），所有 invoke 都落到这里，
 * 返回一份像样的示例项目数据。这样调界面不用每次启动桌面程序，
 * 也方便截图对比设计。
 *
 * 只在 `!window.__TAURI_INTERNALS__` 时生效，打包进桌面版也不会被调用到。
 */
import type {
  AppSettings,
  ChapterMeta,
  Asset,
  AsrCapabilities,
  Chapter,
  Checklist,
  Job,
  PanelProgress,
  ProjectSnapshot,
  Shot,
  VideoPrompt,
  VideoTake,
} from "@/types/models";

const now = new Date().toISOString();

/* ------------------------------------------------------------------ 数据 */

const chapters: ChapterMeta[] = [
  { id: "ch_001", index: 1, title: "退婚宴上的耳光", status: "written", summary: "苏晚在订婚宴上被当众退婚，转身撞进一个坐在轮椅上的男人怀里", characters: ["苏晚", "陆沉"], scenes: ["宴会厅"], wordCount: 1840, updatedAt: now },
  { id: "ch_002", index: 2, title: "轮椅上的男人", status: "written", summary: "男人的保镖递来一份契约婚姻协议，条件只有一个：演满三年", characters: ["苏晚", "陆沉"], scenes: ["别墅客厅"], wordCount: 2130, updatedAt: now },
  { id: "ch_003", index: 3, title: "第一次交锋", status: "draft", summary: "苏晚发现陆沉的腿伤另有隐情，而她的退婚也并非偶然", characters: ["苏晚", "陆沉", "林薇"], scenes: ["书房"], wordCount: 620, updatedAt: now },
  { id: "ch_004", index: 4, title: "她开始反击", status: "empty", summary: "", characters: [], scenes: [], wordCount: 0, updatedAt: now },
];

const shotActions = [
  ["苏晚被推搡着后退一步，香槟洒在裙摆上", "全景", "推近"],
  ["她抬头，眼眶发红却硬撑着没掉泪", "特写", "固定"],
  ["镜头摇向角落，轮椅上的男人正看着她", "中景", "横移"],
  ["陆沉指尖敲了敲扶手，唇角一勾", "近景", "固定"],
  ["保镖上前，把一份文件放在苏晚面前", "中景", "跟随"],
  ["苏晚盯着文件，呼吸一滞", "特写", "推近"],
];

const shots: Shot[] = shotActions.map(([action, size, move], i) => ({
  id: `sh_${i + 1}`,
  chapterId: i < 4 ? "ch_001" : "ch_002",
  index: i + 1,
  sceneId: null,
  location: i < 3 ? "宴会厅" : "别墅客厅",
  timeOfDay: "夜晚",
  interior: true,
  shotSize: size,
  camera: "平视",
  cameraMove: move,
  durationSec: [3, 2.5, 4, 3.5, 3, 2.5][i],
  characters: i % 2 === 0 ? ["苏晚"] : ["陆沉"],
  props: i === 4 ? ["契约文件"] : [],
  action,
  dialogue: i === 3 ? "陆沉：签了它，你就是陆太太。" : "",
  narration: "",
  sfx: "",
  bgm: "低弦乐",
  imagePrompt: "",
  videoPrompt: "",
  refImages: [],
  status: "ready",
}));

const view = (id: string, kind: string, label: string, done: boolean, prompt: string) => ({
  id,
  kind: kind as never,
  label,
  prompt,
  negative: "",
  file: done ? `C:/demo/assets/${id}.png` : null,
  thumb: done ? `C:/demo/assets/${id}.png` : null,
  seed: null,
  model: done ? "gpt-image-1" : null,
  providerId: done ? "simon" : null,
  status: (done ? "done" : "planned") as never,
  error: null,
  jobId: null,
  createdAt: now,
});

const assets: Asset[] = [
  {
    id: "ast_1", kind: "character", name: "苏晚", aliases: ["晚晚"],
    description: "二十出头的都市女性，酒红色长直发，眼神倔强",
    tags: ["女主"], lockedTraits: ["锁骨处小痣", "酒红色长直发"],
    views: [
      view("vw_1", "front", "正面", true, "正面全身站姿"),
      view("vw_2", "side", "侧面", true, "侧面全身站姿"),
      view("vw_3", "back", "背面", false, "背面全身站姿"),
      view("vw_4", "fullBody", "全身", false, "全身动态站姿"),
    ],
    createdAt: now, updatedAt: now,
  },
  {
    id: "ast_2", kind: "character", name: "陆沉", aliases: [],
    description: "三十岁上下，深灰西装，眉眼冷峻，左眉有疤",
    tags: ["男主"], lockedTraits: ["左眉有一道浅疤", "深灰三件套西装"],
    views: [
      view("vw_5", "front", "正面", true, "正面坐姿"),
      view("vw_6", "threeQuarter", "四分之三侧", false, "四分之三侧"),
    ],
    createdAt: now, updatedAt: now,
  },
  {
    id: "ast_3", kind: "scene", name: "宴会厅", aliases: [],
    description: "挑高水晶灯，暗金色墙面，长桌铺白布",
    tags: ["主场景"], lockedTraits: [],
    views: [view("vw_7", "wide", "全景", true, "宴会厅全景"), view("vw_8", "medium", "中景", false, "宴会厅长桌中景")],
    createdAt: now, updatedAt: now,
  },
  {
    id: "ast_4", kind: "prop", name: "契约文件", aliases: [],
    description: "牛皮纸档案袋，红色印章",
    tags: [], lockedTraits: [],
    views: [view("vw_9", "closeUp", "特写", false, "文件特写")],
    createdAt: now, updatedAt: now,
  },
];

const prompts: VideoPrompt[] = shots.slice(0, 4).map((s, i) => ({
  id: `vp_${i + 1}`,
  shotId: s.id,
  index: s.index,
  prompt: `${s.action}，镜头${s.cameraMove}，情绪从压抑到爆发，时长约 ${s.durationSec} 秒`,
  negative: "低分辨率, 畸变, 水印",
  motion: s.action,
  cameraMove: s.cameraMove,
  durationSec: s.durationSec,
  firstFrame: { assetId: i % 2 === 0 ? "ast_1" : "ast_2", viewId: i % 2 === 0 ? "vw_1" : "vw_5" },
  lastFrame: null,
  refs: [{ assetId: "ast_3", viewId: "vw_7" }],
  modelHint: null,
  seed: null,
  status: "ready",
  updatedAt: now,
}));

const takes: VideoTake[] = [0, 1, 2].map((i) => ({
  id: `take_${i + 1}`,
  shotId: shots[i].id,
  promptId: prompts[i].id,
  providerId: "vid-1",
  model: "my_first_last_frame_workflow",
  status: i === 2 ? "running" : "done",
  progress: i === 2 ? 0.42 : 1,
  file: i === 2 ? null : `C:/demo/video/take_${i + 1}.mp4`,
  thumb: null,
  seed: null,
  durationSec: shots[i].durationSec,
  error: null,
  jobId: `job_${i + 1}`,
  createdAt: now,
}));

const checklistSeed: Checklist = {
  items: [
    { id: "auto_script", panel: "script", text: "剧本：2 章完成（共 4 章）", done: false, doneBy: "agent", doneAt: null, note: "2 章还没写完", auto: true },
    { id: "auto_style", panel: "style", text: "风格：已确定画面风格圣经", done: true, doneBy: "agent", doneAt: null, note: "", auto: true },
    { id: "ck_1", panel: "script", text: "每章结尾都有钩子", done: true, doneBy: "user", doneAt: now, note: "", auto: false },
    { id: "ck_2", panel: "script", text: "人物关系在前三章交代清楚", done: false, doneBy: null, doneAt: null, note: "", auto: false },
    { id: "ck_3", panel: "storyboard", text: "没有超过 6 秒的长镜头", done: true, doneBy: "user", doneAt: now, note: "", auto: false },
    { id: "ck_4", panel: "asset", text: "主要人物都有三视图", done: false, doneBy: null, doneAt: null, note: "苏晚缺背面", auto: false },
    { id: "ck_5", panel: "prompt", text: "首帧图已配齐", done: false, doneBy: null, doneAt: null, note: "", auto: false },
  ],
};
const snapshot: ProjectSnapshot = {
  root: "E:/projects/示例短剧",
  manifest: {
    schemaVersion: 1, id: "prj_demo", name: "示例短剧", genre: "都市甜宠",
    logline: "被退婚那天，她捡到了全城最贵的男人", episodeCountHint: 4,
    aspectRatio: "9:16", createdAt: now, updatedAt: now, appVersion: "0.1.0",
  },
  bible: {
    synopsis: "苏晚在订婚宴上被当众退婚，意外救下重伤的集团继承人陆沉，两人从契约婚姻走向真心。",
    sellingPoints: ["开局退婚打脸", "契约婚姻先婚后爱", "男主腿伤反转"],
    world: "现代都市，商战背景", tone: "甜中带虐", audience: "18-30 女性",
    characters: [
      { id: "chr_1", name: "苏晚", aliases: ["晚晚"], role: "女主", age: "22", appearance: "酒红色长直发，锁骨有痣", personality: "外柔内刚", arc: "从被欺负到掌控全局", notes: "" },
      { id: "chr_2", name: "陆沉", aliases: [], role: "男主", age: "30", appearance: "深灰西装，左眉有疤", personality: "冷峻克制", arc: "从封闭到交付真心", notes: "" },
    ],
    notes: "",
  },
  chapters,
  style: {
    activePresetId: "sty_urban_sweet",
    spec: {
      id: "sty_1", name: "都市甜宠", prompt: "电影级质感，暖调高饱和，柔光竖屏人像，浅景深虚化背景",
      negative: "低分辨率, 畸变, 多余手指, 水印",
      palette: ["暖橘", "奶油白", "浅粉"], lighting: "柔光箱 + 逆光轮廓光",
      lens: "35mm/85mm，浅景深", filmStock: "数码感，轻微高光溢出",
      aspectRatio: "9:16", motionStyle: "镜头平稳，缓慢推近", notes: "",
    },
  },
  shots,
  assets,
  prompts,
  takes,
  timeline: {
    durationSec: 12.5, fps: 30, width: 768, height: 1344, updatedAt: now,
    tracks: [
      { id: "tr_video", kind: "video", name: "主视频", muted: false, locked: false, clips: [
        { id: "clip_1", source: "C:/demo/video/take_1.mp4", shotId: "sh_1", takeId: "take_1", label: "1 · 全景", inSec: 0, outSec: 3, startSec: 0, speed: 1, volume: 1, enabled: true },
        { id: "clip_2", source: "C:/demo/video/take_2.mp4", shotId: "sh_2", takeId: "take_2", label: "2 · 特写", inSec: 0, outSec: 2.5, startSec: 3, speed: 1, volume: 1, enabled: true },
        { id: "clip_3", source: "C:/demo/video/take_3.mp4", shotId: "sh_3", takeId: "take_3", label: "3 · 中景", inSec: 0, outSec: 4, startSec: 5.5, speed: 1, volume: 1, enabled: true },
      ] },
      { id: "tr_audio", kind: "audio", name: "音频", muted: false, locked: false, clips: [] },
      { id: "tr_sub", kind: "subtitle", name: "字幕", muted: false, locked: false, clips: [] },
      { id: "tr_overlay", kind: "overlay", name: "叠加", muted: false, locked: false, clips: [] },
    ],
  },
  subtitles: [],
  checklist: checklistSeed,
};

const progress: PanelProgress[] = [
  { panel: "script", total: 4, done: 2, percent: 50, blockers: ["2 章还没写完：第3章 第一次交锋、第4章 她开始反击"] },
  { panel: "style", total: 1, done: 1, percent: 100, blockers: [] },
  { panel: "storyboard", total: 4, done: 2, percent: 50, blockers: ["2 章还没有分镜"] },
  { panel: "asset", total: 4, done: 2, percent: 50, blockers: ["6 个视图还没有生成"] },
  { panel: "prompt", total: 6, done: 4, percent: 67, blockers: ["还有 2 个镜头没有视频提示词"] },
  { panel: "video", total: 6, done: 3, percent: 50, blockers: ["还有 3 个镜头没有生成视频"] },
  { panel: "edit", total: 3, done: 3, percent: 100, blockers: [] },
  { panel: "subtitle", total: 1, done: 0, percent: 0, blockers: ["还没有生成字幕"] },
  { panel: "checklist", total: 12, done: 5, percent: 42, blockers: ["还有 7 项自定义检查没勾"] },
];

const jobs: Job[] = [
  { id: "job_1", kind: "video", title: "生成视频 · 镜头 3", detail: "生成中…", status: "running", progress: 0.42, total: 0, done: 0, error: null, createdAt: now, updatedAt: now, finishedAt: null },
  { id: "job_2", kind: "image", title: "生成图片 · 苏晚·背面", detail: "已生成", status: "done", progress: 1, total: 0, done: 0, error: null, createdAt: now, updatedAt: now, finishedAt: now },
  { id: "job_3", kind: "image", title: "生成图片 · 陆沉·四分之三侧", detail: "HTTP 429 限流，已重试", status: "failed", progress: 0.6, total: 0, done: 0, error: "生图接口返回 429：rate limit exceeded", createdAt: now, updatedAt: now, finishedAt: now },
];

const settings: AppSettings = {
  theme: "dark", workspacePanelWidth: 240, agentDockWidth: 400, agentDockOpen: true,
  agentMode: "auto", maxToolRounds: 12,
  proxyMode: "auto", proxyUrl: "",
  asrBackend: "auto", asrUseGpu: true, asrGpuLayers: 999, asrThreads: 0,
  asrModel: "ggml-large-v3-turbo",
  asrModelBaseUrl: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main",
  whisperCliPath: "", ffmpegPath: "", ffprobePath: "",
  activeLlmProviderId: "llm-1", activeImageProviderId: "img-1", activeVideoProviderId: "vid-1",
  recentProjects: [
    { path: "E:/projects/示例短剧", name: "示例短剧", id: "prj_demo", openedAt: now, exists: true },
  ],
  providers: [
    { id: "llm-1", kind: "llm", name: "主力文本模型", adapter: "openai", baseUrl: "https://api.example.com", apiKeyRef: "key_image_api", model: "gpt-4o-mini", concurrency: 4, timeoutSec: 900, enabled: true, options: { maxTokens: 8000, temperature: 0.7 } },
    { id: "img-1", kind: "image", name: "图片生成", adapter: "generic-http", baseUrl: "https://api.example.com", apiKeyRef: "key_image_api", model: "gpt-image-1", concurrency: 2, timeoutSec: 600, enabled: true, options: { size: "1024x1536" } },
    { id: "vid-1", kind: "video", name: "首尾帧生视频", adapter: "generic-http", baseUrl: "https://comfy.example.com", apiKeyRef: "key_video_api", model: "my_first_last_frame_workflow", concurrency: 2, timeoutSec: 1800, enabled: true, options: { authStyle: "raw" } },
  ],
};



const asrCaps: AsrCapabilities = {
  os: "windows", arch: "x86_64", cpuThreads: 24, appleSilicon: false,
  cudaAvailable: true, cudaVersion: "12.4", nvidiaGpu: "NVIDIA GeForce RTX 4060 Laptop GPU",
  metalSupported: false, vulkanSupported: true, directmlSupported: true,
  whisperCompiled: false, compiledBackends: [], externalCliAvailable: false, externalCliPath: null,
  recommendedBackend: "cuda", modelsDir: "C:/Users/demo/AppData/Roaming/com.duanju.workbench/models/whisper",
  installedModels: [
    { id: "ggml-large-v3-turbo", label: "Large v3 Turbo（推荐，中文好且快）", fileName: "ggml-large-v3-turbo.bin", sizeMb: 1620, downloaded: true, path: "C:/demo/ggml-large-v3-turbo.bin", multilingual: true },
    { id: "ggml-small", label: "Small（均衡）", fileName: "ggml-small.bin", sizeMb: 466, downloaded: false, path: null, multilingual: true },
    { id: "ggml-tiny", label: "Tiny（最快，质量低）", fileName: "ggml-tiny.bin", sizeMb: 75, downloaded: false, path: null, multilingual: true },
  ],
};

/* -------------------------------------------------------------- 分发器 */

function chapterContent(id: string) {
  const c = chapters.find((x) => x.id === id);
  return `# ${c?.title}\n\n水晶灯的光在香槟塔上碎成一片。\n\n苏晚站在人群中央，戒指还捏在手心，指尖冰凉。\n\n「苏晚，」男人的声音不高，却让整个宴会厅安静下来，「婚约到此为止。」\n\n她抬起头。\n\n没有哭。\n\n她只是把戒指放在侍者的托盘上，转身，一步步走出人群。\n\n然后她撞进了一个人的怀里。\n\n那是一个坐在轮椅上的男人。`;
}

export async function mockInvoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 40));
  switch (cmd) {
    case "project_snapshot":
      return snapshot;
    case "project_progress":
      return progress;
    case "settings_get":
      return settings;
    case "secrets_status":
      return { key_image_api: true, key_video_api: true };
    case "jobs_list":
      return jobs;
    case "checklist_get":
      return snapshot.checklist;
    case "agent_sessions":
      return [
        {
          id: "sess_demo",
          projectId: "prj_demo",
          panel: "storyboard",
          title: "把第一章拆成分镜",
          messages: [],
          lastProviderId: "simon-llm",
          lastModel: "gpt-4o-mini",
          turns: 3,
          totalUsage: { inputTokens: 4821, outputTokens: 1340, cacheReadTokens: 41200, cacheWriteTokens: 6200 },
          totalCacheRead: 41200,
          totalInput: 4821,
          createdAt: now,
          updatedAt: now,
          prefix: null,
          display: [
            {
              id: "m1",
              role: "user",
              text: "把第一章拆成分镜，节奏快一点",
              reasoning: "",
              toolCalls: [],
              images: [],
              createdAt: now,
            },
            {
              id: "m2",
              role: "assistant",
              text:
                "已经按短剧节奏把第一章拆成 4 个镜头，每镜 2.5~4 秒，情绪走向是\n**压抑 → 爆发 → 转折 → 钩子**。\n\n| 镜号 | 景别 | 时长 | 作用 |\n| --- | --- | --- | --- |\n| 1 | 全景 | 3s | 交代处境 |\n| 2 | 特写 | 2.5s | 情绪顶点 |\n| 3 | 中景 | 4s | 引入男主 |\n| 4 | 近景 | 3.5s | 留钩子 |",
              reasoning: "先看章节大纲确认人物和场景，再决定镜头数量。短剧一集 60~90 秒，这里 4 镜偏少但第一章信息量不大，够用。",
              toolCalls: [
                {
                  id: "t1", name: "script_read_chapter", title: "读取章节",
                  input: { index: 1 }, summary: "第 1 章，1840 字", ok: true, costly: false, durationMs: 34,
                },
                {
                  id: "t2", name: "storyboard_write_shots", title: "覆盖写入整章分镜",
                  input: { chapterId: "ch_001", shots: "…" },
                  summary: "已写入 4 个镜头", ok: true, costly: false, durationMs: 61,
                },
              ],
              images: [],
              createdAt: now,
            },
            {
              id: "m3",
              role: "user",
              text: "第 2 镜给苏晚出个特写参考图",
              reasoning: "",
              toolCalls: [],
              images: [],
              createdAt: now,
            },
            {
              id: "m4",
              role: "assistant",
              text: "这个操作会调用生图接口（1 张，约 ¥0.4）。确认后我提交。",
              reasoning: "",
              toolCalls: [
                {
                  id: "t3", name: "asset_generate_view", title: "生成资产图片",
                  input: { assetId: "ast_1", viewId: "vw_1" },
                  summary: "已提交 1 个生图任务，任务会在后台执行",
                  ok: true, costly: true, durationMs: 128,
                },
                {
                  id: "t4", name: "asset_generate_view", title: "生成资产图片",
                  input: { assetId: "ast_2", viewId: "vw_5" },
                  summary: "生图接口返回 429：rate limit exceeded",
                  ok: false, costly: true, durationMs: 2204,
                },
              ],
              images: [],
              createdAt: now,
            },
          ],
        },
      ];
    case "agent_session_new":
      return {
        id: "sess_demo", projectId: "prj_demo", panel: args?.panel ?? "script",
        title: "新对话", messages: [], display: [], prefix: null, lastProviderId: null,
        lastModel: "gpt-4o-mini", turns: 0, totalUsage: {}, totalCacheRead: 0,
        totalInput: 0, createdAt: now, updatedAt: now,
      };
    case "script_read_chapter":
      return { meta: chapters.find((c) => c.id === args?.chapterId) ?? chapters[0], content: chapterContent(String(args?.chapterId)) };
    case "style_presets":
      return [
        { id: "sty_urban_sweet", name: "都市甜宠", category: "都市", prompt: "现代都市短剧质感，暖调高饱和，柔光竖屏人像，浅景深虚化背景", negative: "低分辨率, 畸变", palette: ["暖橘", "奶油白", "浅粉"], lighting: "柔光箱 + 逆光轮廓光", lens: "35mm/85mm", filmStock: "数码感", motionStyle: "镜头平稳" },
        { id: "sty_suspense", name: "都市悬疑", category: "悬疑", prompt: "冷峻都市夜景，青蓝与深灰对比，硬光高反差", negative: "低分辨率", palette: ["青蓝", "深灰"], lighting: "硬质侧逆光", lens: "24mm/50mm", filmStock: "细颗粒", motionStyle: "缓慢横移" },
        { id: "sty_costume", name: "古装宫廷", category: "古装", prompt: "古代宫廷质感，工笔重彩，暖金色调", negative: "低分辨率", palette: ["朱红", "鎏金"], lighting: "柔和顶光", lens: "50mm/85mm", filmStock: "细腻胶片感", motionStyle: "横移展示" },
        { id: "sty_cyber", name: "赛博未来", category: "科幻", prompt: "赛博朋克未来都市，霓虹青紫，湿地反光", negative: "低分辨率", palette: ["霓虹青", "品红"], lighting: "霓虹色光源", lens: "24mm/35mm", filmStock: "高对比数码", motionStyle: "快速穿越" },
      ];
    case "storyboard_vocab":
      return {
        shotSizes: ["大远景", "远景", "全景", "中景", "中近景", "近景", "特写", "大特写", "过肩"],
        cameraMoves: ["固定", "横移", "推近", "拉远", "摇摄", "跟随", "升降", "环绕", "手持", "变焦"],
        timeOfDay: ["清晨", "白天", "黄昏", "夜晚", "深夜", "不限"],
      };
    case "asr_capabilities":
      return asrCaps;
    case "media_sidecar_status":
      return {
        ffmpeg: { ok: true, path: "C:/demo/app/ffmpeg.exe" },
        ffprobe: { ok: true, path: "C:/demo/app/ffprobe.exe" },
      };
    case "proxy_status":
      return {
        mode: "auto", manualUrl: "", envProxy: null,
        systemProxy: "http://127.0.0.1:10808", effective: "http://127.0.0.1:10808",
        noProxy: "localhost,127.0.0.1,::1",
      };
    case "subtitle_get":
      return { id: args?.subtitleId, name: "第1集", source: "C:/demo/render.mp4", language: "zh", model: "ggml-large-v3-turbo", cues: [], file: null, createdAt: now };
    default:
      return null;
  }
}

export const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
