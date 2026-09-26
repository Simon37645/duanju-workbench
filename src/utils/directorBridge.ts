/**
 * 导演台前端桥：agent 的 3D 预演工具调用从这里落到「3D预演」面板里的导演台。
 *
 * 为什么需要这一层：面板不是常驻的（切走就卸载，iframe 跟着销毁），而 agent
 * 可能在任何面板里被唤起。所以：
 * - 面板挂载时把自己的 `__director` 接口提供者注册进来（`registerDirectorProvider`）；
 * - 收到 `director://call` 时如果面板没开，先自动切过去、再等它就绪；
 *   约 15 秒还拿不到就报错，不干等 Rust 侧的兜底超时；
 * - `__capture_frame` 这种要直接摸引擎的接口在这里实现（同源 iframe，拿得到引擎对象），
 *   渲染出来的一帧会回给工具，作为图片进 agent 的上下文。
 */
import { listen } from "@tauri-apps/api/event";
import router from "@/router";
import { api, errorText } from "@/api/ipc";
import { toast } from "@/ui";

export interface DirectorCallResult {
  ok: boolean;
  data?: unknown;
  error?: string;
}

/** 导演台引擎里我们要用到的那部分（见它的 src/engine.ts） */
export interface DirectorEngine {
  time: number;
  exporting: boolean;
  project?: { aspect?: string; duration?: number };
  prepareOutput(time: number, signal?: AbortSignal): Promise<void>;
  renderOutput(
    time: number,
    width: number,
    height: number,
    cameraId?: string,
  ): HTMLCanvasElement;
  restorePreview(time: number): void;
  editorRenderer?: { domElement: HTMLCanvasElement };
}

export interface DirectorApi {
  callTool: (name: string, args?: Record<string, unknown>) => Promise<DirectorCallResult>;
  getDocument: () => unknown;
  replaceProject: (doc: unknown) => void;
  getEngine?: () => DirectorEngine | null;
}

interface DirectorCall {
  callId: string;
  name: string;
  args?: Record<string, unknown>;
}

/** 面板挂载时注册，卸载时传 null */
let provider: (() => DirectorApi | null) | null = null;

export function registerDirectorProvider(fn: (() => DirectorApi | null) | null) {
  provider = fn;
}

function currentApi(): DirectorApi | null {
  return provider?.() ?? null;
}

let installed = false;

/** 全局装一次（App.vue 调用）。 */
export function installDirectorBridge() {
  if (installed) return;
  installed = true;
  void listen<DirectorCall>("director://call", (e) => {
    void handleCall(e.payload);
  }).catch(() => {
    /* 浏览器预览里没有 Tauri */
  });
}

async function handleCall(payload: DirectorCall) {
  try {
    const director = await ensureDirector();
    let data: unknown;
    if (payload.name === "__get_document") {
      data = director.getDocument();
    } else if (payload.name === "__capture_frame") {
      data = await captureFrame(director, payload.args ?? {});
    } else {
      const r = await director.callTool(payload.name, payload.args ?? {});
      if (!r?.ok) throw new Error(r?.error ?? "执行失败");
      data = r.data;
    }
    await api.directorResult(payload.callId, true, data);
  } catch (e) {
    await api.directorResult(payload.callId, false, null, errorText(e)).catch(() => {});
  }
}

/* ------------------------------------------------------------ 面板就绪 */

const READY_TIMEOUT_MS = 15_000;
const POLL_MS = 100;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function ensureDirector(): Promise<DirectorApi> {
  const ready = currentApi();
  if (ready) return ready;

  // 面板没开：切过去，然后等 iframe 把 __director 挂出来
  if (router.currentRoute.value.params.panel !== "previz") {
    toast.info("已切到「3D预演」，正在操作导演台");
    try {
      await router.push({ name: "workspace", params: { panel: "previz" } });
    } catch {
      /* 没有打开的项目时会退回首页，下面的等待会用超时报出来 */
    }
  }

  const deadline = Date.now() + READY_TIMEOUT_MS;
  while (Date.now() < deadline) {
    await sleep(POLL_MS);
    const now = currentApi();
    if (now) return now;
  }
  throw new Error(
    `导演台没能在 ${READY_TIMEOUT_MS / 1000} 秒内就绪——打开「3D预演」面板看看它是不是没加载出来`,
  );
}

/* ------------------------------------------------------------ 渲染一帧 */

function num(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

function str(v: unknown): string | null {
  return typeof v === "string" && v.trim() ? v.trim() : null;
}

/** 渲染器要偶数尺寸（H.264 之类的老规矩，导出管线也是这么要求的） */
function evenInt(value: number, min: number, max: number): number {
  const n = Math.round(Math.min(Math.max(value, min), max));
  return n % 2 === 0 ? n : n - 1;
}

/** 项目画幅（"16:9"）→ 宽高比，拿不到就按 16:9 */
function aspectRatio(engine: DirectorEngine): number {
  const parts = str(engine.project?.aspect)?.split(":").map(Number) ?? [];
  const [w, h] = parts;
  return Number.isFinite(w) && Number.isFinite(h) && h > 0 ? w / h : 16 / 9;
}

/**
 * 出图尺寸：调用方给了就听调用方的，没给就按项目画幅出（长边 1024）——
 * 这样 agent 看到的构图和成片一致，不会因为比例不对而误判。
 */
function frameSize(
  engine: DirectorEngine,
  raw: Record<string, unknown>,
): { width: number; height: number } {
  const ratio = aspectRatio(engine);
  const askedW = num(raw.width);
  const askedH = num(raw.height);
  if (askedW || askedH) {
    const width = evenInt(askedW ?? (askedH as number) * ratio, 320, 1920);
    const height = evenInt(askedH ?? (askedW as number) / ratio, 180, 1080);
    return { width, height };
  }
  const longEdge = 1024;
  return ratio >= 1
    ? {
        width: evenInt(longEdge, 320, 1920),
        height: evenInt(longEdge / ratio, 180, 1080),
      }
    : {
        width: evenInt(longEdge * ratio, 320, 1920),
        height: evenInt(longEdge, 180, 1080),
      };
}

interface FrameResult {
  dataUrl: string;
  source: string;
  width: number;
  height: number;
  time: number;
  cameraId: string;
}

async function captureFrame(
  director: DirectorApi,
  raw: Record<string, unknown>,
): Promise<FrameResult> {
  const engine = director.getEngine?.() ?? null;
  if (!engine) throw new Error("这个版本的导演台没有开放渲染接口（getEngine），没法渲帧");
  if (engine.exporting) throw new Error("导演台正在导出，等它跑完再渲帧");

  const source = str(raw.source) === "editor" ? "editor" : "camera";
  const askedTime = num(raw.time);
  const currentTime = num(engine.time) ?? 0;
  const duration = num(engine.project?.duration) ?? Number.POSITIVE_INFINITY;
  const time = Math.min(Math.max(0, askedTime ?? currentTime), duration);

  if (source === "editor") {
    // 布景视图画布开了 preserveDrawingBuffer，最后一帧还在，读到什么就是用户看到的样子
    const canvas = engine.editorRenderer?.domElement;
    if (!canvas) throw new Error("拿不到布景视图画布");
    return {
      dataUrl: canvas.toDataURL("image/jpeg", 0.92),
      source,
      width: canvas.width,
      height: canvas.height,
      time: currentTime,
      cameraId: "editor",
    };
  }

  const { width, height } = frameSize(engine, raw);

  let cameraId = str(raw.cameraId);
  if (!cameraId && askedTime === null) {
    // 没指定机位也没指定时间：问它当前预览的是哪个机位
    // （传当前时间，等于原地 seek，不会动用户的画面）
    try {
      const r = await director.callTool("director_view", { time: currentTime });
      if (r?.ok) cameraId = str((r.data as { cameraId?: unknown } | null)?.cameraId);
    } catch {
      /* 问不到就用成片机位 */
    }
  }
  const camera = cameraId ?? "program";

  const restoreTime = currentTime;
  engine.exporting = true;
  try {
    await engine.prepareOutput(time);
    const canvas = engine.renderOutput(time, width, height, camera);
    return {
      dataUrl: canvas.toDataURL("image/jpeg", 0.92),
      source,
      width,
      height,
      time,
      cameraId: camera,
    };
  } finally {
    engine.exporting = false;
    engine.restorePreview(restoreTime);
  }
}
