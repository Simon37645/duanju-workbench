<script setup lang="ts">
/**
 * 3D 预演面板：白模场景搭建 + 机位设计 + 关键帧动画 + 白模视频导出。
 *
 * 布局：3D 视口铺满面板；左上角菜单栏（添加 / 显示 / 时间轴 / 导出）；
 * 右侧是对象列表与选中属性（浮层）；底部是时间轴（可折叠，
 * 显示选中物体的关键帧轨道）。
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Box, ChevronDown, Circle, Clapperboard, Crosshair, Cylinder, Diamond, Eye, Film,
  FolderOpen, Layers, ListTree, Move, Maximize, Pause, PersonStanding, Play, RotateCw,
  Save, SkipBack, SlidersHorizontal, Square, Trash2, Triangle, Video, X,
} from "@lucide/vue";
import UiButton from "@/ui/Button.vue";
import UiEmpty from "@/ui/Empty.vue";
import UiField from "@/ui/Field.vue";
import UiInput from "@/ui/Input.vue";
import UiNumberInput from "@/ui/NumberInput.vue";
import UiSelect from "@/ui/Select.vue";
import UiSwitch from "@/ui/Switch.vue";
import { api, errorText } from "@/api/ipc";
import { toast } from "@/ui";
import { useProjectStore } from "@/stores/project";
import { PrevizEngine, type TransformMode } from "@/previz/engine";
import {
  asPrevizScene,
  EASING_LABELS,
  focalToFov,
  fovToFocal,
  PRIMITIVE_LABELS,
  type EasingType,
  type PrevizCamera,
  type PrevizCharacter,
  type PrevizKeyframe,
  type PrevizPrimitive,
  type PrevizPrimitiveKind,
  type PrevizScene,
} from "@/previz/types";

type AnyEntity = PrevizPrimitive | PrevizCharacter | PrevizCamera;

const project = useProjectStore();
const canvasHost = ref<HTMLDivElement | null>(null);
const timelineHost = ref<HTMLDivElement | null>(null);

const sceneData = ref<PrevizScene>(asPrevizScene(null));
const selectedId = ref<string | null>(null);
const selectedJoint = ref<string | null>(null);
const clay = ref(true);
const mode = ref<TransformMode>("translate");
const modelStatus = ref<"loading" | "ready" | "fallback" | null>(null);
const clips = ref<string[]>([]);

/* ------------------------------------------------------------ 时间轴状态 */
const showTimeline = ref(true);
const showObjects = ref(false);
const showInspector = ref(true);
const time = ref(0);
const playing = ref(false);
const duration = ref(10);
const fps = ref(30);
/** 选中的关键帧：`${entityId}@${time}` */
const selectedKf = ref<string | null>(null);
/** 左上角展开的菜单 */
const openMenu = ref<"add" | "view" | null>(null);

/* ------------------------------------------------------------ 导出状态 */
const exporting = ref(false);
const exportDone = ref(0);
const exportTotal = ref(0);

let engine: PrevizEngine | null = null;
let saveTimer: number | null = null;
let disposed = false;
/** 播放/seek 时属性面板的节流刷新（只刷数据不保存） */
let lastPanelSync = 0;

/* ------------------------------------------------------------ 派生数据 */

const selected = computed<AnyEntity | null>(() => {
  const all = [...sceneData.value.primitives, ...sceneData.value.characters, ...sceneData.value.cameras];
  return all.find((x) => x.id === selectedId.value) ?? null;
});
const selectedKind = computed<"primitive" | "character" | "camera" | null>(() => {
  if (sceneData.value.primitives.some((x) => x.id === selectedId.value)) return "primitive";
  if (sceneData.value.characters.some((x) => x.id === selectedId.value)) return "character";
  if (sceneData.value.cameras.some((x) => x.id === selectedId.value)) return "camera";
  return null;
});
const entityName = computed(() => {
  const s = selected.value;
  return s ? s.name || defaultName(s) : "";
});
const kfs = computed<PrevizKeyframe[]>(() => {
  const s = selected.value;
  return (s?.keyframes ?? []).slice().sort((a, b) => a.time - b.time);
});
const selectedKfObj = computed<PrevizKeyframe | null>(() => {
  const key = selectedKf.value;
  if (!key) return null;
  const t = Number(key.split("@")[1]);
  return kfs.value.find((k) => Math.abs(k.time - t) < 1e-3) ?? null;
});
const focalMm = computed(() => {
  const cam = selected.value;
  if (!cam || !("fov" in cam)) return null;
  return Math.round(fovToFocal(cam.fov) * 10) / 10;
});
const jointOptions = computed(() => {
  if (selectedKind.value !== "character" || !selectedId.value || !engine) return [];
  const list = engine.listJoints(selectedId.value);
  return [
    { label: "（整体编辑）", value: "" },
    ...list.map((j) => ({ label: `${"　".repeat(j.depth)}${j.label}`, value: j.name })),
  ];
});
const clipOptions = computed(() => [
  { label: "静止（T-pose）", value: "" },
  ...clips.value.map((c) => ({ label: c, value: c })),
]);
const trimTracks = computed(() => {
  // 时间轴轨道：选中物体的关键帧 + 角色片段 / 机位切换（按类型）
  const s = selected.value;
  if (!s) return [];
  if (selectedKind.value === "character") {
    const cues = engine?.getClipCues(s.id) ?? [];
    return [{ label: "动作", cues }];
  }
  return [];
});
const cameraCueTrack = computed(() => {
  if (selectedKind.value !== "camera") return [];
  return sceneData.value.cameraCues;
});
const durationSec = computed({
  get: () => duration.value,
  set: (v: number | null) => {
    if (v == null) return;
    duration.value = v;
    engine?.setDuration(v);
  },
});
const fpsValue = computed({
  get: () => fps.value,
  set: (v: number | null) => {
    if (v == null) return;
    fps.value = v;
    engine?.setFps(v);
  },
});

function defaultName(e: AnyEntity): string {
  if ("kind" in e) return PRIMITIVE_LABELS[e.kind];
  if ("model" in e) return e.model === "xbot" ? "X Bot" : "Y Bot";
  return "机位";
}
function displayName(e: AnyEntity): string {
  return e.name || defaultName(e);
}

/* ------------------------------------------------------------ 同步 */

function refreshList() {
  if (!engine) return;
  sceneData.value = engine.serialize();
  clips.value = engine.getAnimationClips();
  duration.value = engine.getDuration();
  fps.value = engine.getFps();
}

function scheduleSave() {
  refreshList();
  if (saveTimer !== null) window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    saveTimer = null;
    if (!engine || disposed) return;
    void api.previzPut(engine.serialize()).catch(() => {});
  }, 800);
}

onMounted(async () => {
  if (!canvasHost.value) return;
  engine = new PrevizEngine(canvasHost.value, {
    onSelection: (id) => {
      selectedId.value = id;
      selectedJoint.value = engine?.getSelectedJoint() ?? null;
      selectedKf.value = null;
    },
    onChanged: () => scheduleSave(),
    onModelStatus: (s) => (modelStatus.value = s),
    onViewChange: (pov) => (povActive.value = pov),
    onTime: (t) => {
      time.value = t;
      playing.value = engine?.isPlaying() ?? false;
      // 播放中属性面板的数值要跟着动（节流，避免每帧全量序列化）
      const now = Date.now();
      if (now - lastPanelSync > 250) {
        lastPanelSync = now;
        sceneData.value = engine!.serialize();
      }
    },
  });
  try {
    const raw = await api.previzGet();
    engine.applyScene(raw);
  } catch {
    /* 没打开项目或后端不可用：空场景，编辑仍可用 */
  }
  engine.setClay(clay.value);
  refreshList();
  window.addEventListener("keydown", onKey);
  // agent（自研 / pi 引擎）经 Rust 事件桥操作导演台；浏览器预览里没有 Tauri，忽略失败
  listen<{ callId: string; name: string; args?: unknown }>("director://call", (e) => {
    void handleDirectorCall(e.payload);
  })
    .then((fn) => (unlistenDirector = fn))
    .catch(() => {});
});

onBeforeUnmount(() => {
  disposed = true;
  window.removeEventListener("keydown", onKey);
  unlistenDirector?.();
  if (saveTimer !== null) window.clearTimeout(saveTimer);
  if (engine && saveTimer !== null) void api.previzPut(engine.serialize()).catch(() => {});
  engine?.dispose();
  engine = null;
});

/* ------------------------------------------------------------ 快捷键 */

function onKey(e: KeyboardEvent) {
  const el = e.target as HTMLElement | null;
  const typing =
    !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
  // 小键盘 0：进/出摄像机视角（Blender 习惯）
  if (e.code === "Numpad0") {
    e.preventDefault();
    const wasPov = engine?.isPovMode() ?? false;
    const entered = engine?.toggleCameraView() ?? false;
    // 只有「从编辑视角尝试进入但失败」才提示（退出时返回 false 是正常的）
    if (!wasPov && !entered) toast.warn("场景里还没有机位，先添加一个");
    return;
  }
  if (e.code === "Space" && !typing) {
    e.preventDefault();
    togglePlay();
  }
  // 变换模式快捷键（Blender 习惯：G 移动 / R 旋转 / S 缩放）
  if (!typing && selected.value) {
    const k = e.key.toLowerCase();
    if (k === "g") setMode("translate");
    else if (k === "r") setMode("rotate");
    else if (k === "s") setMode("scale");
  }
  if ((e.key === "Delete" || e.key === "Backspace") && !typing && selectedKfObj.value && selected.value) {
    e.preventDefault();
    deleteKf(selected.value.id, selectedKfObj.value.time);
  }
}

/* ------------------------------------------------------------ 工具栏 */

function toggleMenu(m: "add" | "view") {
  openMenu.value = openMenu.value === m ? null : m;
}
function closeMenu() {
  openMenu.value = null;
}
function addPrimitive(kind: PrevizPrimitiveKind) {
  engine?.addPrimitive(kind);
  closeMenu();
}
function addCharacter(model: "xbot" | "ybot") {
  engine?.addCharacter(model);
  closeMenu();
}
function addCamera() {
  engine?.addCamera();
  closeMenu();
}
function setClay(v: boolean) {
  clay.value = v;
  engine?.setClay(v);
}
function setMode(m: TransformMode) {
  mode.value = m;
  engine?.setTransformMode(m);
}

/* ------------------------------------------------------------ 属性 */

function rename(v: string) {
  if (selectedId.value) engine?.renameEntity(selectedId.value, v);
}
function setColor(v: string) {
  if (selectedId.value) engine?.setEntityColor(selectedId.value, v);
}
function setAnimation(v: string | null) {
  if (selectedId.value) engine?.setCharacterAnimation(selectedId.value, v ?? "");
}
function removeSelected() {
  if (selectedId.value) engine?.deleteEntity(selectedId.value);
}
function setTarget(axis: 0 | 1 | 2, v: number | null) {
  const cam = selected.value;
  if (!cam || !("target" in cam) || !selectedId.value || v === null) return;
  const next = [...cam.target] as [number, number, number];
  next[axis] = v;
  engine?.setEntityTransform(selectedId.value, { target: next });
}

/* 数值变换（位置/旋转/缩放），旋转按度输入 */
function numOf(arr: number[] | undefined, i: number): number {
  const v = arr?.[i];
  return typeof v === "number" ? Math.round(v * 1000) / 1000 : 0;
}
function rotDeg(i: number): number {
  const r = selected.value?.transform.rotation[i] ?? 0;
  return Math.round((r * 180) / Math.PI * 10) / 10;
}
function setPos(i: 0 | 1 | 2, v: number | null) {
  if (!selected.value || !selectedId.value || v === null) return;
  const p = [...selected.value.transform.position] as [number, number, number];
  p[i] = v;
  engine?.setEntityTransform(selectedId.value, { position: p });
}
function setRotDeg(i: 0 | 1 | 2, v: number | null) {
  if (!selected.value || !selectedId.value || v === null) return;
  const r = [...selected.value.transform.rotation] as [number, number, number];
  r[i] = (v * Math.PI) / 180;
  engine?.setEntityTransform(selectedId.value, { rotation: r });
}
function setScale(i: 0 | 1 | 2, v: number | null) {
  if (!selected.value || !selectedId.value || v === null) return;
  const s = [...selected.value.transform.scale] as [number, number, number];
  s[i] = Math.max(0.01, v);
  engine?.setEntityTransform(selectedId.value, { scale: s });
}
function setFocal(v: number | null) {
  if (v === null || !selectedId.value) return;
  engine?.setCameraFov(selectedId.value, focalToFov(Math.max(4, v)));
}
function setJoint(v: string | null) {
  if (!selectedId.value) return;
  selectedJoint.value = v || null;
  engine?.selectJoint(selectedId.value, v || null);
}
function resetPose() {
  if (selectedId.value) engine?.resetPose(selectedId.value);
  selectedJoint.value = null;
}

/* ------------------------------------------------------------ 时间轴 */

function togglePlay() {
  playing.value = !playing.value;
  engine?.setPlaying(playing.value);
}
function stopPlay() {
  playing.value = false;
  engine?.setPlaying(false);
  engine?.seek(0);
  time.value = 0;
}
function pct(t: number): string {
  const d = duration.value || 1;
  return `${Math.max(0, Math.min(100, (t / d) * 100))}%`;
}
function captureKf() {
  if (!selectedId.value) return;
  engine?.captureKeyframe(selectedId.value);
  refreshList();
}
function deleteKf(id: string, t: number) {
  engine?.removeKeyframe(id, t);
  selectedKf.value = null;
  refreshList();
}
function setEaseForSelected(v: string | null) {
  if (!v || !selected.value || !selectedKfObj.value) return;
  engine?.setKeyframeEase(selected.value.id, selectedKfObj.value.time, v as EasingType);
  refreshList();
}
function addCameraCue() {
  if (!selectedId.value || selectedKind.value !== "camera") return;
  engine?.addCameraCue(time.value, selectedId.value);
  refreshList();
}
function removeCameraCue(t: number) {
  engine?.removeCameraCue(t);
  refreshList();
}
function setClipCue(clip: string | null) {
  if (!selectedId.value) return;
  engine?.addClipCue(selectedId.value, time.value, clip ?? "");
  refreshList();
}
function removeClipCue(t: number) {
  if (!selectedId.value) return;
  engine?.removeClipCue(selectedId.value, t);
  refreshList();
}

/* 时间轴拖动（播放头 seek / 关键帧拖拽） */
function timeFromEvent(e: PointerEvent): number {
  const host = timelineHost.value;
  if (!host) return 0;
  const rect = host.getBoundingClientRect();
  const u = (e.clientX - rect.left) / Math.max(rect.width, 1);
  return Math.max(0, Math.min(1, u)) * duration.value;
}
function onTrackDown(e: PointerEvent) {
  if (e.button !== 0) return;
  const move = (ev: PointerEvent) => {
    const t = timeFromEvent(ev);
    time.value = t;
    engine?.seek(t);
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    refreshList(); // seek 后把属性面板数值同步过来
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  move(e);
}
let dragKf: { id: string; from: number; moved: boolean } | null = null;
function onKfDown(e: PointerEvent, id: string, t: number) {
  if (e.button !== 0) return;
  e.stopPropagation();
  dragKf = { id, from: t, moved: false };
  const move = (ev: PointerEvent) => {
    if (!dragKf) return;
    const next = timeFromEvent(ev);
    if (Math.abs(next - dragKf.from) < 0.02) return;
    dragKf.moved = true;
    engine?.moveKeyframe(dragKf.id, dragKf.from, next);
    dragKf.from = Math.round(next * 1000) / 1000;
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    if (dragKf && !dragKf.moved) {
      selectedKf.value = `${id}@${dragKf.from}`;
      engine?.seek(dragKf.from);
      time.value = dragKf.from;
    }
    dragKf = null;
    refreshList();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

/* ------------------------------------------------------------ POV */

const povActive = ref(false);

function enterPov(id: string) {
  engine?.setView("camera", id);
}
function exitPov() {
  engine?.setView("editor");
}

/* ------------------------------------------------------ 导演台（DirectorDesk） */

/**
 * 预演面板有两个引擎：
 * - 自研预演：轻量白模 + 关键帧时间轴，与项目数据/教学一体；
 * - 导演台：集成 DirectorDesk（MIT）的成熟预演引擎（灯光/运镜/路径调度/曲线），
 *   以 iframe 承载（隔离它自己的全局样式），换皮对齐工作台设计，工程随项目保存。
 */
const surface = ref<"builtin" | "director">(
  (() => {
    try {
      return localStorage.getItem("previz.surface") === "director" ? "director" : "builtin";
    } catch {
      return "builtin";
    }
  })(),
);
const ddFrame = ref<HTMLIFrameElement | null>(null);
const ddSrc = `${import.meta.env.BASE_URL || "/"}director/index.html`;

function switchSurface(v: "builtin" | "director") {
  surface.value = v;
  try {
    localStorage.setItem("previz.surface", v);
  } catch {
    /* 隐私模式等场景忽略 */
  }
}

/** 换皮：把导演台的中性灰主题对齐工作台设计 tokens（在 iframe 内注入覆盖） */
const DIRECTOR_SKIN = `
  :root {
    --panel: #1a1d23 !important;
    --line: #2c3037 !important;
    --muted: #8b93a0 !important;
    --blue: #f2a13c !important;
    color-scheme: dark !important;
  }
  body { background: #16181d !important; color: #e6e8ec !important; }
  button { background: #23262d !important; border-color: #343943 !important; border-radius: 6px !important; }
  button:hover { background: #2c313a !important; border-color: #4a515e !important; }
  input, select { background: #1e2128 !important; border-color: #343943 !important; border-radius: 6px !important; }
  select option { background-color: #1e2128 !important; color: #e6e8ec !important; }
`;

function onDdLoad() {
  const doc = ddFrame.value?.contentDocument;
  if (!doc) return;
  if (!doc.head.querySelector("style[data-workbench-skin]")) {
    const style = doc.createElement("style");
    style.dataset.workbenchSkin = "1";
    style.textContent = DIRECTOR_SKIN;
    doc.head.appendChild(style);
  }
  // iframe 里应用初始化时可能读到的是未布局的尺寸，触发一次 resize 让它按实际大小重排
  const win = ddFrame.value?.contentWindow;
  const kick = () => win?.dispatchEvent(new Event("resize"));
  setTimeout(kick, 200);
  setTimeout(kick, 800);
}

/** 导演台暴露的自动化接口（window.__director，同源可直接访问） */
interface DirectorApi {
  callTool: (name: string, args?: Record<string, unknown>) => Promise<{ ok: boolean; data?: unknown; error?: string }>;
  getDocument: () => unknown;
  replaceProject: (doc: unknown) => void;
}
function ddApi(): DirectorApi | null {
  return (ddFrame.value?.contentWindow as unknown as { __director?: DirectorApi } | null)?.__director ?? null;
}

/** 保存导演台工程到项目目录（<项目>/previz/director.json） */
async function saveDirector() {
  const api_ = ddApi();
  if (!api_) return toast.warn("导演台还没加载完成");
  try {
    const doc = api_.getDocument();
    const path = await api.directorSave(JSON.stringify(doc));
    toast.ok(`工程已保存到项目：${path}`);
  } catch (e) {
    toast.err(`保存失败：${errorText(e)}`);
  }
}

/** 从项目目录恢复工程（覆盖当前导演台里的内容） */
async function loadDirector() {
  const api_ = ddApi();
  if (!api_) return toast.warn("导演台还没加载完成");
  try {
    const raw = await api.directorLoad();
    if (!raw) return toast.info("这个项目还没有保存过导演台工程，先在导演台里搭好再点「保存工程」");
    api_.replaceProject(JSON.parse(raw));
    toast.ok("已从项目恢复工程");
  } catch (e) {
    toast.err(`恢复失败：${errorText(e)}`);
  }
}

/** agent（自研 / pi 引擎）通过事件桥调导演台：执行并把结果回填 */
let unlistenDirector: UnlistenFn | null = null;
async function handleDirectorCall(payload: { callId: string; name: string; args?: unknown }) {
  try {
    const api_ = ddApi();
    if (!api_) throw new Error("导演台面板未打开：请先切到「导演台」模式并等它加载完成");
    let data: unknown;
    if (payload.name === "__get_document") {
      data = api_.getDocument();
    } else {
      const r = await api_.callTool(payload.name, (payload.args as Record<string, unknown>) ?? {});
      if (!r?.ok) throw new Error(r?.error ?? "执行失败");
      data = r.data;
    }
    await api.directorResult(payload.callId, true, data);
  } catch (e) {
    await api.directorResult(payload.callId, false, null, errorText(e)).catch(() => {});
  }
}

/* ------------------------------------------------------------ 导出白模视频 */

async function exportVideo() {
  if (!engine || exporting.value) return;
  const ar = (project.manifest?.aspectRatio ?? "9:16").split(":");
  const aw = Number(ar[0]) || 9;
  const ah = Number(ar[1]) || 16;
  const shortEdge = 720;
  const width = aw < ah ? shortEdge : Math.round((shortEdge * aw) / ah);
  const height = aw < ah ? Math.round((shortEdge * ah) / aw) : shortEdge;
  const total = Math.max(1, Math.ceil(duration.value * fps.value));

  const wasClay = clay.value;
  const wasPlaying = playing.value;
  engine.setPlaying(false);
  playing.value = false;
  engine.setClay(true); // 白模（无材质）
  exporting.value = true;
  exportDone.value = 0;
  exportTotal.value = total;
  try {
    await api.previzRenderBegin();
    for (let i = 0; i < total; i++) {
      const dataUrl = engine.renderFrameAt(i / fps.value, width, height);
      await api.previzRenderFrame(i, dataUrl);
      exportDone.value = i + 1;
      if (i % 4 === 0) await new Promise((r) => setTimeout(r, 0)); // 让 UI 喘口气
    }
    const path = await api.previzRenderFinish(fps.value);
    toast.ok(`白模视频已导出：${path}`);
  } catch (e) {
    toast.err(`导出失败：${errorText(e)}`);
  } finally {
    exporting.value = false;
    engine.setClay(wasClay);
    engine.seek(0);
    time.value = 0;
    if (wasPlaying) {
      playing.value = true;
      engine.setPlaying(true);
    }
  }
}
</script>

<template>
  <div class="previz" @pointerdown="closeMenu">
    <div v-show="surface === 'builtin'" ref="canvasHost" class="canvas" />

    <!-- 导演台：iframe 承载（隔离它自己的全局样式），换皮与桥接见 onDdLoad -->
    <iframe
      v-if="surface === 'director'"
      ref="ddFrame"
      :src="ddSrc"
      class="dd-frame"
      title="导演台 DirectorDesk"
      @load="onDdLoad"
    />

    <!-- 左上角菜单栏 -->
    <div class="menubar" data-tour="previz-menubar" @pointerdown.stop>
      <!-- 预演引擎切换 -->
      <div class="mode-group">
        <button
          class="mode-btn"
          :class="{ on: surface === 'builtin' }"
          title="轻量白模 + 关键帧时间轴，与项目数据一体"
          @click="switchSurface('builtin')"
        >
          自研预演
        </button>
        <button
          class="mode-btn"
          :class="{ on: surface === 'director' }"
          title="DirectorDesk（MIT 开源）：灯光 / 运镜 / 走位路径 / 曲线编辑"
          @click="switchSurface('director')"
        >
          导演台
        </button>
      </div>
      <template v-if="surface === 'builtin'">
      <div class="menu-wrap">
        <UiButton variant="outline" size="sm" @click="toggleMenu('add')">
          <Layers :size="13" /> 添加 <ChevronDown :size="11" />
        </UiButton>
        <div v-if="openMenu === 'add'" class="menu">
          <div class="menu-group">几何体</div>
          <button class="menu-item" @click="addPrimitive('box')"><Box :size="13" /> 立方体</button>
          <button class="menu-item" @click="addPrimitive('sphere')"><Circle :size="13" /> 球体</button>
          <button class="menu-item" @click="addPrimitive('cylinder')"><Cylinder :size="13" /> 圆柱</button>
          <button class="menu-item" @click="addPrimitive('plane')"><Square :size="13" /> 平台</button>
          <button class="menu-item" @click="addPrimitive('ramp')"><Triangle :size="13" /> 斜坡</button>
          <button class="menu-item" @click="addPrimitive('stairs')"><Layers :size="13" /> 台阶</button>
          <div class="menu-group">角色</div>
          <button class="menu-item" @click="addCharacter('xbot')"><PersonStanding :size="13" /> X Bot</button>
          <button class="menu-item" @click="addCharacter('ybot')"><PersonStanding :size="13" /> Y Bot (染色)</button>
          <div class="menu-group">机位</div>
          <button class="menu-item" @click="addCamera"><Video :size="13" /> 添加机位</button>
        </div>
      </div>

      <div class="menu-wrap">
        <UiButton variant="outline" size="sm" @click="toggleMenu('view')">
          <SlidersHorizontal :size="13" /> 显示 <ChevronDown :size="11" />
        </UiButton>
        <div v-if="openMenu === 'view'" class="menu">
          <label class="menu-row">
            <span>白模渲染</span>
            <UiSwitch :model-value="clay" @update:model-value="setClay" />
          </label>
          <label class="menu-row">
            <span>对象列表</span>
            <UiSwitch :model-value="showObjects" @update:model-value="(v: boolean) => (showObjects = v)" />
          </label>
          <label class="menu-row">
            <span>属性面板</span>
            <UiSwitch :model-value="showInspector" @update:model-value="(v: boolean) => (showInspector = v)" />
          </label>
        </div>
      </div>

      <!-- 变换模式：常驻显示（选中物体后用它切换拖拽手柄） -->
      <div class="mode-group" :title="selected ? '拖拽场景里的手柄来变换选中物体' : '先选中一个物体'">
        <button
          class="mode-btn"
          :class="{ on: mode === 'translate' }"
          title="移动（快捷键 G）"
          @click="setMode('translate')"
        >
          <Move :size="13" /> 移动
        </button>
        <button
          class="mode-btn"
          :class="{ on: mode === 'rotate' }"
          title="旋转（快捷键 R）：拖住圆环转动"
          @click="setMode('rotate')"
        >
          <RotateCw :size="13" /> 旋转
        </button>
        <button
          class="mode-btn"
          :class="{ on: mode === 'scale' }"
          title="缩放（快捷键 S）：拖住方块改变大小"
          @click="setMode('scale')"
        >
          <Maximize :size="13" /> 缩放
        </button>
      </div>

      <UiButton
        :variant="showTimeline ? 'primary' : 'outline'"
        size="sm"
        title="时间轴（选中物体显示它的轨道）"
        @click="showTimeline = !showTimeline"
      >
        <ListTree :size="13" /> 时间轴
      </UiButton>

      <UiButton
        variant="outline"
        size="sm"
        data-tour="previz-export"
        :disabled="exporting"
        title="按项目画幅逐帧渲染白模视频"
        @click="exportVideo"
      >
        <Film :size="13" /> {{ exporting ? `渲染中 ${exportDone}/${exportTotal}` : "导出白模视频" }}
      </UiButton>

      <span v-if="modelStatus === 'loading'" class="t-xs faint">模型加载中…</span>
      <span v-else-if="modelStatus === 'fallback'" class="t-xs faint">模型缺失，已用胶囊人顶替</span>
      </template>
      <template v-else>
        <UiButton variant="outline" size="sm" @click="saveDirector">
          <Save :size="13" /> 保存工程
        </UiButton>
        <UiButton variant="outline" size="sm" @click="loadDirector">
          <FolderOpen :size="13" /> 恢复工程
        </UiButton>
        <span class="t-xs faint">DirectorDesk（MIT 开源）· 工程随项目保存 · agent 可操作</span>
      </template>
    </div>

    <!-- 返回编辑视图 -->
    <UiButton v-if="povActive && surface === 'builtin'" variant="outline" size="sm" class="exit-pov" @click="exitPov">
      ← 返回编辑视图（Numpad 0）
    </UiButton>

    <!-- 右上角：对象列表 -->
    <div v-if="showObjects && surface === 'builtin'" class="objects" @pointerdown.stop>
      <div class="panel-title">场景对象</div>
      <template v-if="sceneData.primitives.length || sceneData.characters.length || sceneData.cameras.length">
        <div v-if="sceneData.cameras.length" class="elist">
          <div class="egroup">机位</div>
          <button
            v-for="c in sceneData.cameras"
            :key="c.id"
            class="eitem"
            :class="{ on: c.id === selectedId }"
            @click="engine?.select(c.id)"
          >
            <Video :size="12" class="eic" />
            <span class="grow truncate">{{ displayName(c) }}</span>
            <span class="pov" title="从这个机位取景" @click.stop="enterPov(c.id)"><Eye :size="12" /></span>
          </button>
        </div>
        <div v-if="sceneData.characters.length" class="elist">
          <div class="egroup">角色</div>
          <button
            v-for="c in sceneData.characters"
            :key="c.id"
            class="eitem"
            :class="{ on: c.id === selectedId }"
            @click="engine?.select(c.id)"
          >
            <PersonStanding :size="12" class="eic" />
            <span class="grow truncate">{{ displayName(c) }}</span>
            <span class="dotc" :style="{ background: c.color }" />
          </button>
        </div>
        <div v-if="sceneData.primitives.length" class="elist">
          <div class="egroup">几何体</div>
          <button
            v-for="p in sceneData.primitives"
            :key="p.id"
            class="eitem"
            :class="{ on: p.id === selectedId }"
            @click="engine?.select(p.id)"
          >
            <Box :size="12" class="eic" />
            <span class="grow truncate">{{ displayName(p) }}</span>
            <span class="dotc" :style="{ background: p.color }" />
          </button>
        </div>
      </template>
      <UiEmpty v-else text="场景还是空的" />
    </div>

    <!-- 右侧：选中属性 -->
    <div v-if="selected && showInspector && surface === 'builtin'" class="inspector scroll" @pointerdown.stop>
      <div class="row-between">
        <span class="panel-title">属性</span>
        <button class="close" @click="engine?.select(null)"><X :size="12" /></button>
      </div>

      <UiField label="名称">
        <UiInput :model-value="entityName" @update:model-value="rename($event as string)" />
      </UiField>

      <UiField v-if="selectedKind === 'primitive'" label="颜色">
        <input type="color" class="color" :value="(selected as any).color" @input="setColor(($event.target as HTMLInputElement).value)" />
      </UiField>

      <template v-if="selectedKind === 'character'">
        <UiField label="颜色">
          <input type="color" class="color" :value="(selected as any).color" @input="setColor(($event.target as HTMLInputElement).value)" />
        </UiField>
        <UiField label="动作片段">
          <UiSelect :model-value="(selected as any).animation || ''" :options="clipOptions" @update:model-value="setAnimation" />
        </UiField>
        <UiField label="骨骼控制">
          <UiSelect :model-value="selectedJoint ?? ''" :options="jointOptions" @update:model-value="setJoint" />
        </UiField>
        <UiButton v-if="selectedJoint" variant="subtle" size="sm" style="width: 100%" @click="resetPose">
          退出姿态编辑（恢复动画）
        </UiButton>
      </template>

      <template v-if="selectedKind === 'camera'">
        <UiField label="焦距 / FOV">
          <div class="row" style="gap: 6px">
            <UiNumberInput :model-value="focalMm" :min="4" :max="300" suffix="mm" @update:model-value="setFocal" />
            <UiNumberInput
              :model-value="Math.round((selected as PrevizCamera).fov)"
              :min="5" :max="150" suffix="°"
              @update:model-value="(v: number | null) => v !== null && engine?.setCameraFov(selected!.id, v)"
            />
          </div>
        </UiField>
        <UiField label="注视点 X / Y / Z">
          <div class="grid3">
            <UiNumberInput :model-value="numOf((selected as PrevizCamera).target, 0)" :step="0.1" @update:model-value="setTarget(0, $event)" />
            <UiNumberInput :model-value="numOf((selected as PrevizCamera).target, 1)" :step="0.1" @update:model-value="setTarget(1, $event)" />
            <UiNumberInput :model-value="numOf((selected as PrevizCamera).target, 2)" :step="0.1" @update:model-value="setTarget(2, $event)" />
          </div>
        </UiField>
        <UiButton variant="outline" size="sm" style="width: 100%" @click="addCameraCue">
          <Clapperboard :size="13" /> 在当前时间加「切到此机位」
        </UiButton>
      </template>

      <UiField label="位置 X / Y / Z">
        <div class="grid3">
          <UiNumberInput :model-value="numOf(selected.transform.position, 0)" :step="0.1" @update:model-value="setPos(0, $event)" />
          <UiNumberInput :model-value="numOf(selected.transform.position, 1)" :step="0.1" @update:model-value="setPos(1, $event)" />
          <UiNumberInput :model-value="numOf(selected.transform.position, 2)" :step="0.1" @update:model-value="setPos(2, $event)" />
        </div>
      </UiField>
      <UiField label="旋转 X / Y / Z（度）">
        <div class="grid3">
          <UiNumberInput :model-value="rotDeg(0)" :step="5" @update:model-value="setRotDeg(0, $event)" />
          <UiNumberInput :model-value="rotDeg(1)" :step="5" @update:model-value="setRotDeg(1, $event)" />
          <UiNumberInput :model-value="rotDeg(2)" :step="5" @update:model-value="setRotDeg(2, $event)" />
        </div>
      </UiField>
      <UiField label="缩放 X / Y / Z">
        <div class="grid3">
          <UiNumberInput :model-value="numOf(selected.transform.scale, 0)" :step="0.05" @update:model-value="setScale(0, $event)" />
          <UiNumberInput :model-value="numOf(selected.transform.scale, 1)" :step="0.05" @update:model-value="setScale(1, $event)" />
          <UiNumberInput :model-value="numOf(selected.transform.scale, 2)" :step="0.05" @update:model-value="setScale(2, $event)" />
        </div>
      </UiField>

      <div class="row" style="gap: 6px">
        <UiButton variant="primary" size="sm" style="flex: 1" @click="captureKf">
          <Diamond :size="13" /> 打关键帧（{{ time.toFixed(2) }}s）
        </UiButton>
      </div>

      <UiButton variant="subtle" size="sm" style="width: 100%; color: var(--danger)" @click="removeSelected">
        <Trash2 :size="13" /> 删除选中对象
      </UiButton>
    </div>

    <!-- 左下角：快捷键提示 -->
    <div v-if="surface === 'builtin'" class="keyhint" :class="{ lifted: showTimeline }">
      <b>G</b> 移动 · <b>R</b> 旋转 · <b>S</b> 缩放 · 空格 播放 · <b>Numpad 0</b> 机位视角
    </div>

    <!-- 底部：时间轴 -->
    <div v-if="showTimeline && surface === 'builtin'" class="timeline" data-tour="previz-timeline" @pointerdown.stop>
      <div class="tl-head">
        <UiButton variant="outline" size="xs" icon :title="playing ? '暂停（空格）' : '播放（空格）'" @click="togglePlay">
          <template #icon><Pause v-if="playing" :size="12" /><Play v-else :size="12" /></template>
        </UiButton>
        <UiButton variant="subtle" size="xs" icon title="回到起点" @click="stopPlay">
          <template #icon><SkipBack :size="12" /></template>
        </UiButton>
        <span class="t-xs num" style="min-width: 90px">{{ time.toFixed(2) }} / {{ duration.toFixed(1) }}s</span>
        <span class="t-xs faint">时长</span>
        <div style="width: 74px"><UiNumberInput v-model="durationSec" :min="1" :max="600" :step="1" suffix="s" /></div>
        <span class="t-xs faint">帧率</span>
        <div style="width: 66px"><UiNumberInput v-model="fpsValue" :min="1" :max="60" :step="1" /></div>

        <span class="grow" />

        <template v-if="selectedKfObj">
          <span class="t-xs faint">缓动</span>
          <div style="width: 110px">
            <UiSelect
              :model-value="selectedKfObj.ease"
              :options="Object.entries(EASING_LABELS).map(([value, label]) => ({ value, label }))"
              @update:model-value="setEaseForSelected"
            />
          </div>
          <UiButton variant="subtle" size="xs" style="color: var(--danger)" @click="deleteKf(selected!.id, selectedKfObj.time)">
            <Trash2 :size="12" />
          </UiButton>
        </template>
        <span v-if="!selected" class="t-xs faint">选中一个物体后，这里显示它的关键帧轨道</span>
      </div>

      <div class="tl-body">
        <div class="tl-names">
          <div class="tl-name">{{ selected ? displayName(selected) : "—" }}</div>
          <div v-if="selectedKind === 'character'" class="tl-name sub">动作</div>
          <div v-if="selectedKind === 'camera'" class="tl-name sub">机位切换</div>
        </div>
        <div ref="timelineHost" class="tl-area" @pointerdown="onTrackDown">
          <div class="tl-ruler">
            <span v-for="m in Math.floor(duration / 1) + 1" :key="m" class="tick" :style="{ left: pct(m - 1) }">
              {{ m - 1 }}s
            </span>
          </div>

          <div class="tl-track main">
            <button
              v-for="k in kfs"
              :key="k.time"
              class="kf"
              :class="{ sel: selectedKf === `${selected!.id}@${k.time}` }"
              :style="{ left: pct(k.time) }"
              :title="`${k.time.toFixed(2)}s（${EASING_LABELS[k.ease]}）拖拽移动，Delete 删除`"
              @pointerdown="onKfDown($event, selected!.id, k.time)"
            />
          </div>

          <div v-if="selectedKind === 'character'" class="tl-track sub">
            <button
              v-for="c in trimTracks[0]?.cues ?? []"
              :key="c.time"
              class="cue"
              :style="{ left: pct(c.time) }"
              :title="`${c.time.toFixed(2)}s → ${c.clip || '静止'}（双击删除）`"
              @dblclick.stop="removeClipCue(c.time)"
            >
              {{ c.clip || "静止" }}
            </button>
            <span class="tl-hint">在当前时间设置动作：</span>
          </div>

          <div v-if="selectedKind === 'camera'" class="tl-track sub">
            <button
              v-for="c in cameraCueTrack"
              :key="c.time"
              class="cue cam"
              :style="{ left: pct(c.time) }"
              :title="`${c.time.toFixed(2)}s 切到该机位（双击删除）`"
              @dblclick.stop="removeCameraCue(c.time)"
            />
          </div>

          <div class="playhead" :style="{ left: pct(time) }" />
        </div>
      </div>

      <div v-if="selectedKind === 'character'" class="tl-foot" @pointerdown.stop>
        <span class="t-xs faint">动作切换点（{{ time.toFixed(2) }}s）：</span>
        <div style="width: 140px">
          <UiSelect :model-value="''" :options="clipOptions" placeholder="选一个动作" @update:model-value="setClipCue" />
        </div>
      </div>
    </div>

    <!-- 导出进度 -->
    <div v-if="exporting && surface === 'builtin'" class="export-mask" @pointerdown.stop>
      <div class="export-card">
        <Film :size="16" />
        <div class="col" style="gap: 4px; min-width: 200px">
          <span class="t-sm">正在渲染白模视频…</span>
          <div class="bar"><div class="fill" :style="{ width: `${(exportDone / Math.max(exportTotal, 1)) * 100}%` }" /></div>
          <span class="t-xs faint num">{{ exportDone }} / {{ exportTotal }} 帧</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.previz {
  position: relative;
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--surface-2);
}
.canvas {
  position: absolute;
  inset: 0;
}
/* 导演台 iframe：铺满面板（它自己是完整应用界面） */
.dd-frame {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  border: none;
  background: #16181d;
}

/* ------------------------------------------------------------ 菜单栏 */
.menubar {
  position: absolute;
  top: 10px;
  left: 10px;
  z-index: 30;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px;
  border-radius: var(--r);
  background: color-mix(in srgb, var(--surface-1) 88%, transparent);
  border: 1px solid var(--line-faint);
  backdrop-filter: blur(6px);
}
.menu-wrap {
  position: relative;
}
.menu {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  min-width: 176px;
  padding: 6px;
  border-radius: var(--r-lg);
  background: var(--surface-2);
  border: 1px solid var(--line-strong);
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.menu-group {
  padding: 6px 8px 2px;
  color: var(--fg-ghost);
  font-size: var(--t-xs);
}
.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 8px;
  border: none;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--fg-dim);
  cursor: pointer;
  font-size: var(--t-sm);
  text-align: left;
}
.menu-item:hover {
  background: var(--surface-4);
  color: var(--fg);
}
.menu-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 6px 8px;
  font-size: var(--t-sm);
  color: var(--fg-dim);
}

/* 变换模式：常驻小分段按钮 */
.mode-group {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 2px;
  border-radius: var(--r);
  background: var(--surface-3);
  border: 1px solid var(--line);
}
.mode-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 8px;
  border: none;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--fg-dim);
  cursor: pointer;
  font-size: var(--t-xs);
  white-space: nowrap;
}
.mode-btn:hover {
  color: var(--fg);
}
.mode-btn.on {
  background: var(--accent);
  color: var(--accent-fg);
}

/* 左下角快捷键提示 */
.keyhint {
  position: absolute;
  left: 12px;
  bottom: 12px;
  z-index: 20;
  padding: 4px 10px;
  border-radius: var(--r-full);
  background: color-mix(in srgb, var(--surface-1) 72%, transparent);
  border: 1px solid var(--line-faint);
  color: var(--fg-faint);
  font-size: var(--t-xs);
  pointer-events: none;
}
.keyhint b {
  color: var(--fg-dim);
  font-weight: 600;
}
/* 时间轴展开时提示上移，避免压住时间轴 */
.keyhint.lifted {
  bottom: 214px;
}

/* ------------------------------------------------------------ 浮层 */
.objects,
.inspector {
  position: absolute;
  top: 10px;
  z-index: 25;
  width: 236px;
  max-height: calc(100% - 20px);
  overflow: auto;
  padding: 10px;
  border-radius: var(--r-lg);
  background: color-mix(in srgb, var(--surface-1) 92%, transparent);
  border: 1px solid var(--line-faint);
  backdrop-filter: blur(6px);
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.objects {
  left: 10px;
  top: 62px;
  width: 216px;
}
.inspector {
  right: 10px;
  width: 258px;
}
.panel-title {
  color: var(--fg-dim);
  font-size: var(--t-xs);
  letter-spacing: 0.04em;
}
.close {
  border: none;
  background: none;
  color: var(--fg-faint);
  cursor: pointer;
  padding: 2px;
}
.close:hover {
  color: var(--fg);
}
.exit-pov {
  position: absolute;
  top: 10px;
  right: 10px;
  z-index: 26;
}

.grid3 {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 6px;
}
.color {
  width: 100%;
  height: 28px;
  padding: 2px 4px;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  cursor: pointer;
}

/* 对象列表 */
.elist {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.egroup {
  padding: 2px 6px;
  color: var(--fg-ghost);
  font-size: var(--t-xs);
}
.eitem {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  height: 26px;
  padding: 0 8px;
  border: none;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--fg-dim);
  cursor: pointer;
  font-size: var(--t-sm);
  text-align: left;
}
.eitem:hover {
  background: var(--surface-3);
  color: var(--fg);
}
.eitem.on {
  background: var(--surface-4);
  color: var(--fg);
}
.eic {
  color: var(--fg-faint);
  flex: 0 0 auto;
}
.dotc {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  flex: 0 0 auto;
  border: 1px solid rgba(255, 255, 255, 0.25);
}
.pov {
  display: grid;
  place-items: center;
  width: 20px;
  height: 20px;
  border-radius: var(--r-sm);
  color: var(--fg-faint);
}
.pov:hover {
  color: var(--accent);
  background: var(--accent-soft);
}

/* ------------------------------------------------------------ 时间轴 */
.timeline {
  position: absolute;
  left: 10px;
  right: 10px;
  bottom: 10px;
  z-index: 28;
  border-radius: var(--r-lg);
  background: color-mix(in srgb, var(--surface-1) 92%, transparent);
  border: 1px solid var(--line-faint);
  backdrop-filter: blur(6px);
  display: flex;
  flex-direction: column;
}
.tl-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-bottom: 1px solid var(--line-faint);
}
.tl-body {
  display: flex;
  min-height: 96px;
}
.tl-names {
  width: 110px;
  flex: 0 0 110px;
  padding: 20px 8px 6px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  border-right: 1px solid var(--line-faint);
}
.tl-name {
  height: 20px;
  line-height: 20px;
  font-size: var(--t-xs);
  color: var(--fg-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tl-name.sub {
  color: var(--fg-ghost);
}
.tl-area {
  position: relative;
  flex: 1;
  min-width: 0;
  padding: 20px 8px 6px;
  cursor: crosshair;
}
.tl-ruler {
  position: absolute;
  top: 0;
  left: 8px;
  right: 8px;
  height: 18px;
  border-bottom: 1px solid var(--line-faint);
}
.tick {
  position: absolute;
  top: 2px;
  transform: translateX(-50%);
  font-size: 9px;
  color: var(--fg-ghost);
  white-space: nowrap;
}
.tl-track {
  position: relative;
  height: 20px;
  margin-bottom: 4px;
}
.tl-track.sub {
  height: 22px;
}
.kf {
  position: absolute;
  top: 50%;
  width: 9px;
  height: 9px;
  margin-left: -4.5px;
  transform: translateY(-50%) rotate(45deg);
  background: var(--accent);
  border: none;
  border-radius: 2px;
  cursor: grab;
  padding: 0;
}
.kf:hover {
  box-shadow: 0 0 0 3px var(--accent-soft);
}
.kf.sel {
  background: #fff;
  box-shadow: 0 0 0 2px var(--accent);
}
.cue {
  position: absolute;
  top: 1px;
  height: 18px;
  padding: 0 5px;
  transform: translateX(-2px);
  font-size: 9px;
  line-height: 18px;
  border-radius: 4px;
  border: 1px solid var(--line-strong);
  background: var(--surface-4);
  color: var(--fg-dim);
  cursor: default;
  white-space: nowrap;
}
.cue.cam {
  width: 10px;
  height: 10px;
  padding: 0;
  border-radius: 2px;
  background: #f2b544;
  border-color: #f2b544;
}
.tl-hint {
  position: absolute;
  right: 6px;
  top: 2px;
  font-size: 9px;
  color: var(--fg-ghost);
}
.playhead {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--accent);
  pointer-events: none;
}
.playhead::before {
  content: "";
  position: absolute;
  top: 0;
  left: -4px;
  border: 4px solid transparent;
  border-top-color: var(--accent);
}
.tl-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-top: 1px solid var(--line-faint);
}

/* 导出进度 */
.export-mask {
  position: absolute;
  inset: 0;
  z-index: 40;
  display: grid;
  place-items: center;
  background: rgba(8, 10, 14, 0.5);
}
.export-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px 20px;
  border-radius: var(--r-lg);
  background: var(--surface-2);
  border: 1px solid var(--line-strong);
  box-shadow: var(--shadow-lg);
}
.bar {
  width: 100%;
  height: 4px;
  border-radius: var(--r-full);
  background: var(--surface-4);
  overflow: hidden;
}
.bar .fill {
  height: 100%;
  background: var(--accent);
  transition: width 120ms linear;
}
</style>
