/**
 * 3D 预演场景数据。
 *
 * 这是前端自治的 JSON（存进项目 .workbench/previz.json，后端只透传落盘），
 * 所以不放进 types/models.ts，也不要求与 Rust models.rs 对齐；
 * 后端只有 progress.rs 会读它数一数实体个数来算面板进度。
 */

export type PrevizPrimitiveKind =
  | "box"
  | "sphere"
  | "cylinder"
  | "plane"
  | "ramp"
  | "stairs";

/** 变换统一存欧拉角（弧度），与 three 的 Object3D.rotation 一致 */
export interface PrevizTransform {
  position: [number, number, number];
  rotation: [number, number, number];
  scale: [number, number, number];
}

/** 关键帧之间的缓动曲线；默认缓进缓出 */
export type EasingType = "linear" | "easeIn" | "easeOut" | "easeInOut";

export const EASING_LABELS: Record<EasingType, string> = {
  linear: "线性",
  easeIn: "缓入",
  easeOut: "缓出",
  easeInOut: "缓进缓出",
};

export const DEFAULT_EASING: EasingType = "easeInOut";

/**
 * 一个关键帧：记录该时间点上实体的**全部可动画参数**。
 * 位置 / 旋转 / 缩放对所有实体有效；target 与 fov 只对机位；
 * pose 只对角色（骨骼名 → 四元数 xyzw）。
 */
export interface PrevizKeyframe {
  time: number;
  /** 本关键帧到下一关键帧之间的缓动 */
  ease: EasingType;
  position: [number, number, number];
  /** 欧拉角（弧度） */
  rotation: [number, number, number];
  scale: [number, number, number];
  target?: [number, number, number];
  /** 视场角（度） */
  fov?: number;
  pose?: Record<string, [number, number, number, number]>;
}

/** 角色动画片段切换点（时间点开始播某个动作） */
export interface PrevizClipCue {
  time: number;
  clip: string;
}

/** 机位切换点（时间轴上按时间顺序切换活动机位） */
export interface PrevizCameraCue {
  time: number;
  cameraId: string;
}

interface PrevizEntityBase {
  id: string;
  name: string;
  transform: PrevizTransform;
  /** 时间轴关键帧（按 time 升序；空 = 静态实体） */
  keyframes?: PrevizKeyframe[];
}

/** 基础几何体（白模布景用的方块 / 球 / 斜坡…） */
export interface PrevizPrimitive extends PrevizEntityBase {
  kind: PrevizPrimitiveKind;
  color: string;
}

/**
 * 人形角色。model 指向 public/models 下的 GLB（不含扩展名）；
 * xbot 是 three.js 官方示例资产（自带骨骼与 7 段动画），ybot 缺文件时
 * 引擎会自动用 xbot 顶替并按 color 染色区分。
 */
export interface PrevizCharacter extends PrevizEntityBase {
  model: "xbot" | "ybot";
  color: string;
  /** 当前播放的动画片段名（空 = 静止 T-pose） */
  animation: string;
  /** 动画片段的切换点 */
  clipCues?: PrevizClipCue[];
}

/** 预演机位。target 是注视点：移动机位时朝向随注视点，转动机位时反算注视点 */
export interface PrevizCamera extends PrevizEntityBase {
  fov: number;
  target: [number, number, number];
}

export interface PrevizScene {
  version: 2;
  /** 时间轴总长（秒） */
  duration: number;
  /** 导出帧率 */
  fps: number;
  primitives: PrevizPrimitive[];
  characters: PrevizCharacter[];
  cameras: PrevizCamera[];
  /** 机位切换轨道 */
  cameraCues: PrevizCameraCue[];
}

export const DEFAULT_DURATION = 10;
export const DEFAULT_FPS = 30;
/** 全画幅传感器高度（mm），焦距换算用 */
export const SENSOR_HEIGHT_MM = 24;

export const EMPTY_PREVIZ_SCENE: PrevizScene = {
  version: 2,
  duration: DEFAULT_DURATION,
  fps: DEFAULT_FPS,
  primitives: [],
  characters: [],
  cameras: [],
  cameraCues: [],
};

export function asPrevizScene(raw: unknown): PrevizScene {
  if (!raw || typeof raw !== "object") return { ...EMPTY_PREVIZ_SCENE };
  const s = raw as Partial<PrevizScene>;
  return {
    version: 2,
    duration:
      typeof s.duration === "number" && s.duration > 0 ? s.duration : DEFAULT_DURATION,
    fps: typeof s.fps === "number" && s.fps > 0 ? s.fps : DEFAULT_FPS,
    primitives: Array.isArray(s.primitives) ? s.primitives : [],
    characters: Array.isArray(s.characters) ? s.characters : [],
    cameras: Array.isArray(s.cameras) ? s.cameras : [],
    cameraCues: Array.isArray(s.cameraCues) ? s.cameraCues : [],
  };
}

/* ============================================================ 焦距 / FOV */

/** 焦距（mm）→ 垂直视场角（度），按全画幅 24mm 传感器高度 */
export function focalToFov(focalMm: number): number {
  return (2 * Math.atan(SENSOR_HEIGHT_MM / (2 * focalMm)) * 180) / Math.PI;
}

/** 垂直视场角（度）→ 焦距（mm） */
export function fovToFocal(fovDeg: number): number {
  return SENSOR_HEIGHT_MM / (2 * Math.tan((fovDeg * Math.PI) / 360));
}

/* ================================================================= 默认值 */

/** 角色默认配色：X 偏蓝、Y 偏品红，白模模式下会被统一盖掉 */
export const CHARACTER_DEFAULT_COLORS: Record<PrevizCharacter["model"], string> = {
  xbot: "#5b8cff",
  ybot: "#f065a0",
};

export const PRIMITIVE_DEFAULT_COLORS: Record<PrevizPrimitiveKind, string> = {
  box: "#9aa3ad",
  sphere: "#9aa3ad",
  cylinder: "#9aa3ad",
  plane: "#8b939c",
  ramp: "#9aa3ad",
  stairs: "#9aa3ad",
};

export const PRIMITIVE_LABELS: Record<PrevizPrimitiveKind, string> = {
  box: "立方体",
  sphere: "球体",
  cylinder: "圆柱",
  plane: "平台",
  ramp: "斜坡",
  stairs: "台阶",
};
