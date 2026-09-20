/**
 * 3D 预演引擎：three.js 场景的编排层，不碰 UI。
 *
 * 职责：
 * - 世界光白模渲染（半球光 + 主方向光 + 柔和阴影），所见即所得；
 * - 实体管理（几何体 / 角色 / 机位）与 JSON 序列化（types.ts 里的结构）；
 * - 编辑交互：点选、TransformControls 拖拽（移动/旋转/缩放）、机位 POV；
 * - 时间轴：关键帧插值（位置/旋转/缩放/注视点/FOV/姿态）、缓进缓出曲线、
 *   机位切换、角色动画片段调度、播放控制；
 * - 骨骼控制器：选关节 → gizmo 旋转 → 姿态快照进关键帧；
 * - 角色加载：public/models 下的 GLB（three.js 官方 X Bot，自带骨骼与动画），
 *   缺文件时自动降级为胶囊人。
 *
 * 数据流向：Entry.data 持有类别信息（颜色 / 模型 / 动画 / 关键帧），three 场景的
 * transform 是运行时唯一事实来源，serialize() 按需导出 JSON；applyScene()
 * 重建场景，期间抑制变更回调（载入不算编辑）。
 */
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { TransformControls } from "three/examples/jsm/controls/TransformControls.js";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { clone as skeletonClone } from "three/examples/jsm/utils/SkeletonUtils.js";
import { mergeGeometries } from "three/examples/jsm/utils/BufferGeometryUtils.js";
import {
  asPrevizScene,
  CHARACTER_DEFAULT_COLORS,
  DEFAULT_EASING,
  EMPTY_PREVIZ_SCENE,
  PRIMITIVE_DEFAULT_COLORS,
  type EasingType,
  type PrevizCamera,
  type PrevizCameraCue,
  type PrevizCharacter,
  type PrevizClipCue,
  type PrevizKeyframe,
  type PrevizPrimitive,
  type PrevizPrimitiveKind,
  type PrevizScene,
} from "./types";

export type PrevizEntityKind = "primitive" | "character" | "camera";
export type TransformMode = "translate" | "rotate" | "scale";

export interface PrevizEngineCallbacks {
  /** 选中变化（null = 取消选中） */
  onSelection?: (id: string | null) => void;
  /** 场景内容变化，需要保存（拖拽中会连续触发，由调用方去抖） */
  onChanged?: () => void;
  /** 角色模型加载状态：fallback = 模型缺失，已用胶囊人顶替 */
  onModelStatus?: (status: "loading" | "ready" | "fallback") => void;
  /** 播放推进时的时间回调（UI 更新播放头） */
  onTime?: (time: number) => void;
  /** 编辑视图 / 机位视角切换（UI 更新按钮与提示） */
  onViewChange?: (pov: boolean, cameraId: string | null) => void;
}

export interface JointInfo {
  name: string;
  label: string;
  /** 层级深度，UI 用来缩进 */
  depth: number;
}

interface Entry {
  id: string;
  kind: PrevizEntityKind;
  /** 挂在场景里的根对象，transform 全部作用在它上面 */
  object: THREE.Object3D;
  /** 类别私有数据（颜色 / 模型名 / 动画名 / 关键帧） */
  data?: PrimitiveData | CharacterData;
  /** 角色专用 */
  mixer?: THREE.AnimationMixer;
  actions?: Map<string, THREE.AnimationAction>;
  currentAction?: THREE.AnimationAction;
  currentClipName?: string;
  /** 角色骨骼根（用于姿态快照与关节选择） */
  bonesRoot?: THREE.Object3D;
  /** 姿态模式：动画由关键帧/手动驱动，mixer 暂停 */
  poseMode?: boolean;
  /** 机位专用 */
  camera?: THREE.PerspectiveCamera;
  helper?: THREE.CameraHelper;
  target?: THREE.Vector3;
  /** 机位锥体（POV 模式下要藏起来，否则挡自己视线） */
  body?: THREE.Mesh;
}

type PrimitiveData = Omit<PrevizPrimitive, "transform">;
type CharacterData = Omit<PrevizCharacter, "transform">;

const UP = new THREE.Vector3(0, 1, 0);
/** 预演取景参考比例：短剧竖屏。机位视锥 helper 按这个比例画 */
const VIEWFINDER_ASPECT = 9 / 16;
const CHARACTER_HEIGHT = 1.75;

/** 主要关节的中文名（Mixamo 命名），UI 下拉用 */
const JOINT_LABELS: Record<string, string> = {
  mixamorigHips: "髋部",
  mixamorigSpine: "脊柱",
  mixamorigSpine1: "脊柱 1",
  mixamorigSpine2: "脊柱 2",
  mixamorigNeck: "脖子",
  mixamorigHead: "头",
  mixamorigLeftShoulder: "左肩",
  mixamorigLeftArm: "左上臂",
  mixamorigLeftForeArm: "左前臂",
  mixamorigLeftHand: "左手",
  mixamorigRightShoulder: "右肩",
  mixamorigRightArm: "右上臂",
  mixamorigRightForeArm: "右前臂",
  mixamorigRightHand: "右手",
  mixamorigLeftUpLeg: "左大腿",
  mixamorigLeftLeg: "左小腿",
  mixamorigLeftFoot: "左脚",
  mixamorigLeftToeBase: "左脚尖",
  mixamorigRightUpLeg: "右大腿",
  mixamorigRightLeg: "右小腿",
  mixamorigRightFoot: "右脚",
  mixamorigRightToeBase: "右脚尖",
};

export class PrevizEngine {
  private container: HTMLElement;
  private cb: PrevizEngineCallbacks;

  private renderer: THREE.WebGLRenderer;
  private scene: THREE.Scene;
  private editorCamera: THREE.PerspectiveCamera;
  private orbit: OrbitControls;
  private gizmo: TransformControls;
  private gizmoHelper: THREE.Object3D;
  private raycaster = new THREE.Raycaster();
  private clock = new THREE.Clock();
  private resizeObserver: ResizeObserver;
  private rafId = 0;
  private disposed = false;

  private entries = new Map<string, Entry>();
  private selectedId: string | null = null;
  /** 选中的关节（骨骼名），null = 整体选中 */
  private selectedJoint: string | null = null;
  /** 机位视角（Numpad 0 / 取景按钮进入） */
  private povMode = false;
  /** 时间轴决定的活动机位 id */
  private activeCameraId: string | null = null;
  private clayMode = false;
  private restoring = false;

  /* ------------------------------------------------------------ 时间轴 */
  private time = 0;
  private playing = false;
  private duration = EMPTY_PREVIZ_SCENE.duration;
  private fps = EMPTY_PREVIZ_SCENE.fps;
  private cameraCues: PrevizCameraCue[] = [];

  private clayMaterial = new THREE.MeshStandardMaterial({
    color: 0xe4e1da,
    roughness: 0.95,
    metalness: 0,
  });
  /** 角色模板缓存：一个 GLB 只解析一次，多实例用 SkeletonUtils.clone */
  private templates = new Map<string, Promise<THREE.Group | null>>();
  private clips: THREE.AnimationClip[] = [];

  constructor(container: HTMLElement, cb: PrevizEngineCallbacks = {}) {
    this.container = container;
    this.cb = cb;

    const w = container.clientWidth || 800;
    const h = container.clientHeight || 600;

    this.renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(w, h, false);
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    container.appendChild(this.renderer.domElement);
    this.renderer.domElement.style.display = "block";
    this.renderer.domElement.style.touchAction = "none";

    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x23262d);
    this.scene.fog = new THREE.Fog(0x23262d, 30, 90);

    // 世界光：半球光打底（天空冷 / 地面暖的柔和渐变）+ 一盏带阴影的主方向光
    this.scene.add(new THREE.HemisphereLight(0xdfe8f2, 0x39414d, 1.15));
    const sun = new THREE.DirectionalLight(0xffffff, 2.6);
    sun.position.set(9, 15, 7);
    sun.castShadow = true;
    sun.shadow.mapSize.set(2048, 2048);
    sun.shadow.camera.left = -22;
    sun.shadow.camera.right = 22;
    sun.shadow.camera.top = 22;
    sun.shadow.camera.bottom = -22;
    sun.shadow.camera.far = 60;
    sun.shadow.bias = -0.0004;
    this.scene.add(sun);

    const ground = new THREE.Mesh(
      new THREE.PlaneGeometry(120, 120),
      new THREE.MeshStandardMaterial({ color: 0x2c3038, roughness: 1 }),
    );
    ground.rotation.x = -Math.PI / 2;
    ground.receiveShadow = true;
    this.scene.add(ground);
    this.scene.add(new THREE.GridHelper(40, 40, 0x454c56, 0x33383f));

    this.editorCamera = new THREE.PerspectiveCamera(50, w / h, 0.1, 300);
    this.editorCamera.position.set(6.5, 5, 8.5);

    this.orbit = new OrbitControls(this.editorCamera, this.renderer.domElement);
    this.orbit.target.set(0, 1, 0);
    this.orbit.enableDamping = true;
    this.orbit.dampingFactor = 0.08;
    this.orbit.maxPolarAngle = Math.PI / 2 - 0.02;

    this.gizmo = new TransformControls(this.editorCamera, this.renderer.domElement);
    this.gizmo.setSize(0.75);
    // r169+ 的 TransformControls 不再是 Object3D，要拿 getHelper() 挂进场景
    const withHelper = this.gizmo as unknown as { getHelper?: () => THREE.Object3D };
    this.gizmoHelper = withHelper.getHelper
      ? withHelper.getHelper()
      : (this.gizmo as unknown as THREE.Object3D);
    this.scene.add(this.gizmoHelper);
    this.gizmo.addEventListener("mouseDown", () => (this.orbit.enabled = false));
    this.gizmo.addEventListener("mouseUp", () => (this.orbit.enabled = !this.povMode));
    this.gizmo.addEventListener("objectChange", () => {
      const entry = this.selectedId ? this.entries.get(this.selectedId) : null;
      if (entry?.kind === "camera" && entry.target) {
        if (this.gizmoMode() === "rotate") {
          // 转动机位 = 反算注视点（保持原来注视距离），与 target 语义统一
          const dir = new THREE.Vector3(0, 0, -1).applyQuaternion(entry.object.quaternion);
          const dist = Math.max(entry.target.distanceTo(entry.object.position), 1);
          entry.target.copy(entry.object.position).addScaledVector(dir, dist);
        } else {
          this.aimCamera(entry);
        }
      }
      this.emitChanged();
    });

    // 点选（按下与抬起位移小于阈值才算点击，避免与拖拽 orbit / gizmo 冲突）
    let downX = 0;
    let downY = 0;
    const dom = this.renderer.domElement;
    dom.addEventListener("pointerdown", (e) => {
      downX = e.clientX;
      downY = e.clientY;
    });
    dom.addEventListener("pointerup", (e) => {
      if (Math.hypot(e.clientX - downX, e.clientY - downY) > 5) return;
      this.pick(e);
    });

    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(container);

    this.animate();
  }

  /* ------------------------------------------------------------ 回调 */

  private emitChanged(): void {
    if (this.restoring) return;
    this.cb.onChanged?.();
  }

  private emitSelection(id: string | null): void {
    if (this.restoring) return;
    this.cb.onSelection?.(id);
  }

  private gizmoMode(): TransformMode {
    const m = (this.gizmo as unknown as { mode?: string }).mode;
    return (m as TransformMode) ?? "translate";
  }

  /* ============================================================== 时间轴 */

  getTime(): number {
    return this.time;
  }
  getDuration(): number {
    return this.duration;
  }
  getFps(): number {
    return this.fps;
  }
  isPlaying(): boolean {
    return this.playing;
  }
  isPovMode(): boolean {
    return this.povMode;
  }
  getActiveCameraId(): string | null {
    return this.activeCameraId;
  }
  getCameraCues(): PrevizCameraCue[] {
    return [...this.cameraCues];
  }

  setPlaying(v: boolean): void {
    this.playing = v;
    if (!v) this.applyAt(this.time); // 停住时把姿态定在当前时间
  }

  seek(t: number): void {
    this.time = Math.max(0, Math.min(t, this.duration));
    this.applyAt(this.time);
    this.cb.onTime?.(this.time);
  }

  setDuration(sec: number): void {
    this.duration = Math.max(1, sec);
    if (this.time > this.duration) this.seek(this.duration);
    this.emitChanged();
  }

  setFps(v: number): void {
    this.fps = Math.max(1, Math.min(60, Math.round(v)));
    this.emitChanged();
  }

  /** 应用某个时刻的全部动画（关键帧 + 片段调度 + 机位切换） */
  applyAt(t: number): void {
    for (const entry of this.entries.values()) {
      const kfs = entry.data?.keyframes;
      if (kfs && kfs.length > 0) this.applyKeyframes(entry, kfs, t);
    }
    // 角色动画片段切换点
    for (const entry of this.entries.values()) {
      if (entry.kind !== "character" || !entry.data) continue;
      const cues = (entry.data as CharacterData).clipCues;
      if (!cues || cues.length === 0 || !entry.actions) continue;
      const clip = clipCueAt(cues, t);
      if (clip && clip !== entry.currentClipName) {
        this.playAnimation(entry, clip);
      }
    }
    // 机位切换
    this.activeCameraId = cameraCueAt(this.cameraCues, t);
    if (this.povMode) this.refreshPov();
  }

  /** 给实体打一个关键帧（记录当前时刻的全部参数） */
  captureKeyframe(id: string): void {
    const entry = this.entries.get(id);
    if (!entry) return;
    const o = entry.object;
    const kf: PrevizKeyframe = {
      time: round3(this.time),
      ease: DEFAULT_EASING, // 默认缓进缓出
      position: o.position.toArray() as [number, number, number],
      rotation: [o.rotation.x, o.rotation.y, o.rotation.z],
      scale: o.scale.toArray() as [number, number, number],
    };
    if (entry.kind === "camera" && entry.camera && entry.target) {
      kf.target = entry.target.toArray() as [number, number, number];
      kf.fov = round3(entry.camera.fov);
    }
    if (entry.kind === "character") {
      const pose = this.capturePose(entry);
      if (pose && Object.keys(pose).length > 0) kf.pose = pose;
    }
    const data = entry.data as PrimitiveData | CharacterData | undefined;
    if (!data) return;
    const kfs = (data.keyframes ??= []);
    const i = kfs.findIndex((k) => Math.abs(k.time - kf.time) < 1e-3);
    if (i >= 0) kfs[i] = kf;
    else {
      kfs.push(kf);
      kfs.sort((a, b) => a.time - b.time);
    }
    this.emitChanged();
  }

  removeKeyframe(id: string, time: number): void {
    const entry = this.entries.get(id);
    const data = entry?.data;
    if (!data?.keyframes) return;
    data.keyframes = data.keyframes.filter((k) => Math.abs(k.time - time) > 1e-3);
    this.emitChanged();
  }

  /** 拖动关键帧改时间 */
  moveKeyframe(id: string, from: number, to: number): void {
    const entry = this.entries.get(id);
    const data = entry?.data;
    if (!data?.keyframes) return;
    const kf = data.keyframes.find((k) => Math.abs(k.time - from) < 1e-3);
    if (!kf) return;
    kf.time = round3(Math.max(0, Math.min(to, this.duration)));
    data.keyframes.sort((a, b) => a.time - b.time);
    this.seek(this.time);
    this.emitChanged();
  }

  setKeyframeEase(id: string, time: number, ease: EasingType): void {
    const entry = this.entries.get(id);
    const data = entry?.data;
    if (!data?.keyframes) return;
    const kf = data.keyframes.find((k) => Math.abs(k.time - time) < 1e-3);
    if (!kf) return;
    kf.ease = ease;
    this.seek(this.time);
    this.emitChanged();
  }

  getKeyframes(id: string): PrevizKeyframe[] {
    return this.entries.get(id)?.data?.keyframes ?? [];
  }

  /* ---------------------------------------------------------- 机位切换 */

  addCameraCue(time: number, cameraId: string): void {
    this.cameraCues.push({ time: round3(Math.max(0, time)), cameraId });
    this.cameraCues.sort((a, b) => a.time - b.time);
    this.emitChanged();
  }

  removeCameraCue(time: number): void {
    this.cameraCues = this.cameraCues.filter((c) => Math.abs(c.time - time) > 1e-3);
    this.emitChanged();
  }

  /* ------------------------------------------------------- 角色片段 */

  getClipCues(id: string): PrevizClipCue[] {
    const entry = this.entries.get(id);
    const data = entry?.data as CharacterData | undefined;
    return data?.clipCues ?? [];
  }

  addClipCue(id: string, time: number, clip: string): void {
    const entry = this.entries.get(id);
    const data = entry?.data as CharacterData | undefined;
    if (!data) return;
    const cues = (data.clipCues ??= []);
    const i = cues.findIndex((c) => Math.abs(c.time - time) < 1e-3);
    if (i >= 0) cues[i].clip = clip;
    else cues.push({ time: round3(Math.max(0, time)), clip });
    cues.sort((a, b) => a.time - b.time);
    this.seek(this.time);
    this.emitChanged();
  }

  removeClipCue(id: string, time: number): void {
    const entry = this.entries.get(id);
    const data = entry?.data as CharacterData | undefined;
    if (!data?.clipCues) return;
    data.clipCues = data.clipCues.filter((c) => Math.abs(c.time - time) > 1e-3);
    this.seek(this.time);
    this.emitChanged();
  }

  /* ------------------------------------------------------------ 关节 */

  /** 角色的可操作关节（Mixamo 命名 + 中文标签） */
  listJoints(id: string): JointInfo[] {
    const entry = this.entries.get(id);
    if (!entry || entry.kind !== "character") return [];
    const out: JointInfo[] = [];
    entry.object.traverse((o) => {
      if (!(o instanceof THREE.Bone)) return;
      if (!Object.prototype.hasOwnProperty.call(JOINT_LABELS, o.name)) return;
      let depth = 0;
      let p: THREE.Object3D | null = o.parent;
      while (p && p !== entry.object) {
        if (p instanceof THREE.Bone) depth++;
        p = p.parent;
      }
      out.push({ name: o.name, label: JOINT_LABELS[o.name], depth });
    });
    return out;
  }

  getSelectedJoint(): string | null {
    return this.selectedJoint;
  }

  /** 选中关节：gizmo 挂到骨骼上；进入姿态模式（暂停动画驱动） */
  selectJoint(id: string, boneName: string | null): void {
    const entry = this.entries.get(id);
    if (!entry || entry.kind !== "character") return;
    if (!boneName) {
      this.selectedJoint = null;
      this.select(this.selectedId);
      return;
    }
    const bone = this.findBone(entry, boneName);
    if (!bone) return;
    this.selectedJoint = boneName;
    entry.poseMode = true; // 手动姿态与动画片段互斥，暂停 mixer
    this.gizmo.setMode("rotate"); // 关节只支持旋转
    this.gizmo.attach(bone);
    this.emitChanged();
  }

  /** 退出姿态编辑，回到动画驱动 */
  resetPose(id: string): void {
    const entry = this.entries.get(id);
    if (!entry || entry.kind !== "character") return;
    entry.poseMode = false;
    this.selectedJoint = null;
    const clip = entry.currentClipName || "idle";
    this.playAnimation(entry, clip);
    this.select(id);
    this.emitChanged();
  }

  private findBone(entry: Entry, name: string): THREE.Bone | null {
    let found: THREE.Bone | null = null;
    entry.object.traverse((o) => {
      if (!found && o instanceof THREE.Bone && o.name === name) found = o;
    });
    return found;
  }

  /** 快照当前全部骨骼旋转 */
  private capturePose(entry: Entry): Record<string, [number, number, number, number]> | null {
    const pose: Record<string, [number, number, number, number]> = {};
    let any = false;
    entry.object.traverse((o) => {
      if (o instanceof THREE.Bone) {
        pose[o.name] = [o.quaternion.x, o.quaternion.y, o.quaternion.z, o.quaternion.w];
        any = true;
      }
    });
    return any ? pose : null;
  }

  private applyPose(
    entry: Entry,
    pose: Record<string, [number, number, number, number]>,
  ): void {
    entry.poseMode = true;
    entry.object.traverse((o) => {
      if (!(o instanceof THREE.Bone)) return;
      const q = pose[o.name];
      if (q) o.quaternion.set(q[0], q[1], q[2], q[3]);
    });
  }

  /* ------------------------------------------------------- 关键帧插值 */

  private applyKeyframes(entry: Entry, kfs: PrevizKeyframe[], t: number): void {
    const { a, b, u } = sampleKeyframes(kfs, t);
    const e = b ? easeValue(u, a.ease) : 0;

    entry.object.position.set(...lerp3(a.position, b ? b.position : a.position, e));
    entry.object.scale.set(...lerp3(a.scale, b ? b.scale : a.scale, e));
    const qa = quatFromEuler(a.rotation);
    if (b) {
      const qb = quatFromEuler(b.rotation);
      entry.object.quaternion.copy(qa.slerp(qb, e));
    } else {
      entry.object.quaternion.copy(qa);
    }

    if (entry.kind === "camera" && entry.camera && entry.target) {
      const tgt = b && b.target && a.target ? lerp3(a.target, b.target, e) : a.target;
      if (tgt) entry.target.set(tgt[0], tgt[1], tgt[2]);
      const fov = b && b.fov != null && a.fov != null ? lerpNum(a.fov, b.fov, e) : a.fov;
      if (fov != null) {
        entry.camera.fov = fov;
        entry.camera.updateProjectionMatrix();
      }
      entry.helper?.update();
    }

    if (entry.kind === "character") {
      const pose = a.pose && b?.pose ? slerpPose(a.pose, b.pose, e) : (a.pose ?? b?.pose);
      if (pose) this.applyPose(entry, pose);
    }
  }

  /* ============================================================== 实体 */

  addPrimitive(kind: PrevizPrimitiveKind): string {
    const id = crypto.randomUUID();
    const group = new THREE.Group();
    group.userData.entityId = id;
    const mesh = new THREE.Mesh(
      buildPrimitiveGeometry(kind),
      new THREE.MeshStandardMaterial({ color: PRIMITIVE_DEFAULT_COLORS[kind], roughness: 0.9 }),
    );
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    group.add(mesh);
    // 在原点附近错开摆放，避免堆在一起
    const n = this.entries.size;
    group.position.set(((n % 5) - 2) * 1.6, 0, Math.floor(n / 5) * 1.6);
    if (this.clayMode) this.applyClayToObject(group, true);

    const entry: Entry = {
      id,
      kind: "primitive",
      object: group,
      data: { id, name: "", kind, color: PRIMITIVE_DEFAULT_COLORS[kind], keyframes: [] },
    };
    this.entries.set(id, entry);
    this.scene.add(group);
    this.select(id);
    this.emitChanged();
    return id;
  }

  addCharacter(model: PrevizCharacter["model"]): string {
    const id = crypto.randomUUID();
    const color = CHARACTER_DEFAULT_COLORS[model];
    const group = new THREE.Group();
    group.userData.entityId = id;
    const n = this.entries.size;
    group.position.set(((n % 5) - 2) * 1.8, 0, Math.floor(n / 5) * 1.8);
    const entry: Entry = {
      id,
      kind: "character",
      object: group,
      data: { id, name: "", model, color, animation: "idle", keyframes: [], clipCues: [] },
    };
    this.entries.set(id, entry);
    this.scene.add(group);
    this.select(id);

    // 模板异步加载，到位后替换占位
    this.loadTemplate(model).then((template) => {
      if (this.disposed || !this.entries.has(id)) return;
      if (!template) {
        this.cb.onModelStatus?.("fallback");
        this.buildCapsuleInto(group, color);
        this.emitChanged();
        return;
      }
      this.cb.onModelStatus?.("ready");
      const inner = skeletonClone(template);
      inner.traverse((o) => {
        if (o instanceof THREE.Mesh) {
          o.castShadow = true;
          // 材质克隆成实例私有，染色互不影响
          o.material = (o.material as THREE.Material).clone();
          (o.material as THREE.MeshStandardMaterial).color.set(color);
        }
      });
      // 归一化：脚底贴地、统一身高
      const box = new THREE.Box3().setFromObject(inner);
      const scale = CHARACTER_HEIGHT / (box.max.y - box.min.y || 1);
      inner.scale.setScalar(scale);
      const scaled = new THREE.Box3().setFromObject(inner);
      inner.position.y -= scaled.min.y;
      group.add(inner);
      entry.bonesRoot = inner;

      entry.mixer = new THREE.AnimationMixer(inner);
      entry.actions = new Map(this.clips.map((clip) => [clip.name, entry.mixer!.clipAction(clip)]));
      const data = entry.data as CharacterData;
      this.playAnimation(entry, data.animation || "idle");
      if (this.clayMode) this.applyClayToObject(group, true);
      this.seek(this.time); // 若已有片段/姿态关键帧，立即应用到新角色
      this.emitChanged();
    });
    this.emitChanged();
    return id;
  }

  addCamera(): string {
    const id = crypto.randomUUID();
    const group = new THREE.Group();
    group.userData.entityId = id;

    const camera = new THREE.PerspectiveCamera(50, VIEWFINDER_ASPECT, 0.1, 200);
    group.add(camera);

    // 机位本体：一个指向 -Z 的锥体（与相机朝向一致），便于在编辑视图里点选
    const body = new THREE.Mesh(
      new THREE.ConeGeometry(0.14, 0.42, 4),
      new THREE.MeshStandardMaterial({ color: 0xf2b544, roughness: 0.6 }),
    );
    body.rotation.x = -Math.PI / 2;
    body.position.z = -0.1;
    body.castShadow = true;
    group.add(body);

    const helper = new THREE.CameraHelper(camera);
    this.scene.add(helper);

    const entry: Entry = {
      id,
      kind: "camera",
      object: group,
      camera,
      helper,
      target: new THREE.Vector3(0, 1.2, -3),
      body,
    };
    this.entries.set(id, entry);
    this.scene.add(group);
    const n = this.entries.size;
    group.position.set(((n % 4) - 1.5) * 2.2, 1.5, 4);
    this.aimCamera(entry);
    this.select(id);
    this.emitChanged();
    return id;
  }

  deleteEntity(id: string): void {
    const entry = this.entries.get(id);
    if (!entry) return;
    if (this.selectedId === id) this.select(null);
    if (this.povTarget() === entry) this.setView("editor");
    this.scene.remove(entry.object);
    if (entry.helper) this.scene.remove(entry.helper);
    disposeTree(entry.object);
    entry.helper?.dispose();
    this.entries.delete(id);
    this.cameraCues = this.cameraCues.filter((c) => c.cameraId !== id);
    this.emitChanged();
  }

  /* ------------------------------------------------------------ 属性 */

  renameEntity(id: string, name: string): void {
    const entry = this.entries.get(id);
    if (!entry) return;
    if (entry.kind === "camera") entry.camera!.name = name;
    else if (entry.data) entry.data.name = name;
    this.emitChanged();
  }

  setEntityColor(id: string, color: string): void {
    const entry = this.entries.get(id);
    if (!entry || !entry.data || !("color" in entry.data)) return;
    entry.data.color = color;
    entry.object.traverse((o) => {
      if (o instanceof THREE.Mesh && o.userData.__origMaterial === undefined) {
        (o.material as THREE.MeshStandardMaterial).color.set(color);
      }
    });
    this.emitChanged();
  }

  /** 数值面板：直接设置变换（位置 / 旋转 / 缩放 / 注视点 / FOV） */
  setEntityTransform(
    id: string,
    patch: {
      position?: [number, number, number];
      rotation?: [number, number, number];
      scale?: [number, number, number];
      target?: [number, number, number];
      fov?: number;
    },
  ): void {
    const entry = this.entries.get(id);
    if (!entry) return;
    if (patch.position) entry.object.position.fromArray(patch.position);
    if (patch.rotation) entry.object.rotation.set(...patch.rotation);
    if (patch.scale) entry.object.scale.fromArray(patch.scale);
    if (entry.kind === "camera") {
      if (patch.target && entry.target) {
        entry.target.fromArray(patch.target);
        this.aimCamera(entry);
      }
      if (patch.fov != null && entry.camera) {
        entry.camera.fov = Math.max(5, Math.min(150, patch.fov));
        entry.camera.updateProjectionMatrix();
      }
      entry.helper?.update();
    }
    this.emitChanged();
  }

  setCameraFov(id: string, fov: number): void {
    this.setEntityTransform(id, { fov });
  }

  setCameraTarget(id: string, target: [number, number, number]): void {
    this.setEntityTransform(id, { target });
  }

  setCharacterAnimation(id: string, name: string): void {
    const entry = this.entries.get(id);
    if (!entry) return;
    if (entry.data && "animation" in entry.data) entry.data.animation = name;
    this.playAnimation(entry, name);
  }

  /** 某角色可用的动画片段名（来自 GLB），供 UI 做下拉 */
  getAnimationClips(): string[] {
    return this.clips.map((c) => c.name);
  }

  /* ============================================================ 交互 */

  select(id: string | null): void {
    this.selectedId = id;
    this.selectedJoint = null;
    const entry = id ? this.entries.get(id) : null;
    if (entry) {
      this.gizmo.attach(entry.object);
      // 机位与角色/几何体都支持移动、旋转、缩放；机位的旋转会反算注视点
    } else {
      this.gizmo.detach();
    }
    this.emitSelection(id);
  }

  getSelectedId(): string | null {
    return this.selectedId;
  }

  setTransformMode(mode: TransformMode): void {
    this.gizmo.setMode(mode);
  }

  setClay(on: boolean): void {
    this.clayMode = on;
    for (const entry of this.entries.values()) {
      if (entry.kind === "camera") continue; // 机位本体是编辑辅助，不参与白模
      this.applyClayToObject(entry.object, on);
    }
  }

  /* ------------------------------------------------------------ 机位视图 */

  /** 手动从某个机位取景（列表里的取景按钮） */
  setView(mode: "editor" | "camera", cameraId?: string): void {
    if (mode === "editor" || !cameraId) {
      this.povMode = false;
      this.orbit.enabled = true;
      for (const e of this.entries.values()) {
        if (e.body) e.body.visible = true;
        if (e.helper) e.helper.visible = true;
      }
      this.cb.onViewChange?.(false, null);
      return;
    }
    const entry = this.entries.get(cameraId);
    if (!entry?.camera) return;
    this.povMode = true;
    this.activeCameraId = cameraId;
    this.refreshPov();
    this.cb.onViewChange?.(true, cameraId);
  }

  /** Numpad 0 / 按钮：进摄像机视角（用时间轴上的活动机位），再按返回编辑视图 */
  toggleCameraView(): boolean {
    if (this.povMode) {
      this.setView("editor");
      return false;
    }
    // 活动机位：时间轴 cue > 场景里的第一个机位
    let id = cameraCueAt(this.cameraCues, this.time);
    if (!id) {
      for (const e of this.entries.values()) {
        if (e.kind === "camera") {
          id = e.id;
          break;
        }
      }
    }
    if (!id) return false; // 场景里没有机位
    this.setView("camera", id);
    return true;
  }

  /** POV 状态刷新：隐藏机位辅助体、锁定轨道控制 */
  private refreshPov(): void {
    if (!this.povMode) return;
    const entry = this.povTarget();
    this.orbit.enabled = false;
    this.gizmo.detach();
    for (const e of this.entries.values()) {
      if (e.body) e.body.visible = e !== entry;
      if (e.helper) e.helper.visible = e !== entry;
    }
  }

  private povTarget(): Entry | null {
    if (!this.povMode || !this.activeCameraId) return null;
    return this.entries.get(this.activeCameraId) ?? null;
  }

  /** 当前渲染用相机（POV 模式下是活动机位） */
  private activeRenderCamera(): THREE.Camera {
    return this.povTarget()?.camera ?? this.editorCamera;
  }

  /* ------------------------------------------------------------ 序列化 */

  serialize(): PrevizScene {
    const scene: PrevizScene = {
      version: 2,
      duration: this.duration,
      fps: this.fps,
      primitives: [],
      characters: [],
      cameras: [],
      cameraCues: [...this.cameraCues],
    };
    for (const entry of this.entries.values()) {
      const t = entry.object;
      const transform = {
        position: t.position.toArray() as [number, number, number],
        rotation: [t.rotation.x, t.rotation.y, t.rotation.z] as [number, number, number],
        scale: t.scale.toArray() as [number, number, number],
      };
      if (entry.kind === "camera") {
        scene.cameras.push({
          id: entry.id,
          name: entry.camera!.name || `机位 ${shortId(entry.id)}`,
          fov: round3(entry.camera!.fov),
          target: entry.target
            ? (entry.target.toArray() as [number, number, number])
            : [0, 1, 0],
          transform,
        });
      } else if (entry.kind === "primitive" && entry.data && "kind" in entry.data) {
        scene.primitives.push({ ...entry.data, transform });
      } else if (entry.kind === "character" && entry.data && "model" in entry.data) {
        scene.characters.push({ ...entry.data, transform });
      }
    }
    return scene;
  }

  /** 还原存档。期间不触发选中 / 保存回调，结束后取消选中 */
  applyScene(raw: unknown): void {
    const scene = asPrevizScene(raw);
    this.restoring = true;
    try {
      for (const id of [...this.entries.keys()]) this.deleteEntity(id);
      this.duration = scene.duration;
      this.fps = scene.fps;
      this.cameraCues = [...scene.cameraCues];
      for (const p of scene.primitives) this.restorePrimitive(p);
      for (const c of scene.characters) this.restoreCharacter(c);
      for (const c of scene.cameras) this.restoreCamera(c);
      this.time = 0;
      this.playing = false;
    } finally {
      this.restoring = false;
    }
    this.select(null);
    this.applyAt(0);
    this.cb.onChanged?.(); // 让面板拿到完整场景刷一次列表
  }

  private restorePrimitive(p: PrevizPrimitive): void {
    const id = this.addPrimitive(p.kind);
    const entry = this.entries.get(id);
    if (!entry) return;
    entry.data = { ...p, id, keyframes: p.keyframes ?? [] };
    const mesh = entry.object.children[0] as THREE.Mesh;
    (mesh.material as THREE.MeshStandardMaterial).color.set(p.color);
    applyTransform(entry.object, p.transform);
  }

  private restoreCharacter(c: PrevizCharacter): void {
    const id = this.addCharacter(c.model);
    const entry = this.entries.get(id);
    if (!entry) return;
    entry.data = { ...c, id, keyframes: c.keyframes ?? [], clipCues: c.clipCues ?? [] };
    applyTransform(entry.object, c.transform);
  }

  private restoreCamera(c: PrevizCamera): void {
    const id = this.addCamera();
    const entry = this.entries.get(id);
    if (!entry) return;
    entry.camera!.fov = c.fov;
    entry.camera!.name = c.name;
    entry.camera!.updateProjectionMatrix();
    entry.target!.fromArray(c.target);
    applyTransform(entry.object, c.transform);
    this.aimCamera(entry);
    entry.helper?.update();
    entry.data = { ...c, id, keyframes: c.keyframes ?? [] } as unknown as Entry["data"];
  }

  private playAnimation(entry: Entry, name: string): void {
    if (!entry.mixer || !entry.actions) return;
    const next = name ? entry.actions.get(name) : undefined;
    if (entry.currentAction === next) return;
    entry.currentAction?.fadeOut(0.25);
    if (next) {
      next.reset().fadeIn(0.25).play();
      entry.currentAction = next;
      entry.currentClipName = name;
      entry.poseMode = false;
    } else {
      entry.currentAction = undefined;
      entry.currentClipName = "";
    }
  }

  /* ------------------------------------------------------- 角色模板 */

  private loadTemplate(model: PrevizCharacter["model"]): Promise<THREE.Group | null> {
    const cached = this.templates.get(model);
    if (cached) return cached;
    const base = import.meta.env.BASE_URL || "/";
    const loader = new GLTFLoader();
    const load = async (): Promise<THREE.Group | null> => {
      let gltf: Awaited<ReturnType<typeof loader.loadAsync>>;
      try {
        // ybot 有独立文件就优先用（把 Mixamo 导出的 GLB 放进 public/models 即生效）
        gltf = await loader.loadAsync(`${base}models/${model}.glb`);
      } catch {
        if (model === "xbot") return null;
        try {
          gltf = await loader.loadAsync(`${base}models/xbot.glb`);
        } catch {
          return null;
        }
      }
      this.clips = gltf.animations;
      const root = gltf.scene;
      // 模板按米归一，实例克隆时再各自染色
      const box = new THREE.Box3().setFromObject(root);
      root.scale.setScalar(CHARACTER_HEIGHT / (box.max.y - box.min.y || 1));
      return root;
    };
    const p = load();
    this.templates.set(model, p);
    return p;
  }

  private buildCapsuleInto(group: THREE.Group, color: string): void {
    const mat = new THREE.MeshStandardMaterial({ color, roughness: 0.8 });
    const body = new THREE.Mesh(new THREE.CapsuleGeometry(0.26, 0.85, 6, 16), mat);
    body.position.y = 0.95;
    const head = new THREE.Mesh(new THREE.SphereGeometry(0.17, 20, 14), mat);
    head.position.y = 1.62;
    for (const m of [body, head]) {
      m.castShadow = true;
      group.add(m);
    }
  }

  /* ------------------------------------------------------------ 内部 */

  private aimCamera(entry: Entry): void {
    if (!entry.target) return;
    const m = new THREE.Matrix4().lookAt(entry.object.position, entry.target, UP);
    entry.object.quaternion.setFromRotationMatrix(m);
  }

  private applyClayToObject(root: THREE.Object3D, on: boolean): void {
    root.traverse((o) => {
      if (!(o instanceof THREE.Mesh)) return;
      if (on) {
        if (!o.userData.__origMaterial) o.userData.__origMaterial = o.material;
        o.material = this.clayMaterial;
      } else if (o.userData.__origMaterial) {
        o.material = o.userData.__origMaterial;
        delete o.userData.__origMaterial;
      }
    });
  }

  private pick(e: PointerEvent): void {
    if (this.povMode) return; // 机位视角下不做编辑点选
    const rect = this.renderer.domElement.getBoundingClientRect();
    const ndc = new THREE.Vector2(
      ((e.clientX - rect.left) / rect.width) * 2 - 1,
      -((e.clientY - rect.top) / rect.height) * 2 + 1,
    );
    this.raycaster.setFromCamera(ndc, this.editorCamera);
    const targets: THREE.Object3D[] = [];
    for (const entry of this.entries.values()) targets.push(entry.object);
    const hits = this.raycaster.intersectObjects(targets, true);
    // 沿父链找实体 id；TransformControls 的 gizmo 不在实体子树里，天然排除
    const hit = hits.find((h) => findEntityId(h.object) !== null);
    if (!hit) {
      this.select(null);
      return;
    }
    this.select(findEntityId(hit.object));
  }

  private resize(): void {
    const w = this.container.clientWidth;
    const h = this.container.clientHeight;
    if (w === 0 || h === 0) return;
    this.renderer.setSize(w, h, false);
    this.editorCamera.aspect = w / h;
    this.editorCamera.updateProjectionMatrix();
    const pov = this.povTarget()?.camera;
    if (pov) {
      pov.aspect = w / h;
      pov.updateProjectionMatrix();
    }
  }

  /** 导出用：把渲染尺寸临时设成目标分辨率渲染一帧，返回 canvas 的 JPEG dataURL */
  renderFrameAt(t: number, width: number, height: number, quality = 0.92): string {
    this.seek(t);
    const oldSize = new THREE.Vector2();
    this.renderer.getSize(oldSize);
    const oldPixelRatio = this.renderer.getPixelRatio();
    const cam = this.activeRenderCamera();
    const oldAspect = cam instanceof THREE.PerspectiveCamera ? cam.aspect : null;
    try {
      this.renderer.setPixelRatio(1);
      this.renderer.setSize(width, height, false);
      if (cam instanceof THREE.PerspectiveCamera) {
        cam.aspect = width / height;
        cam.updateProjectionMatrix();
      }
      this.renderer.render(this.scene, cam);
      return this.renderer.domElement.toDataURL("image/jpeg", quality);
    } finally {
      this.renderer.setPixelRatio(oldPixelRatio);
      this.renderer.setSize(oldSize.x, oldSize.y, false);
      if (cam instanceof THREE.PerspectiveCamera && oldAspect != null) {
        cam.aspect = oldAspect;
        cam.updateProjectionMatrix();
      }
    }
  }

  private animate = (): void => {
    if (this.disposed) return;
    this.rafId = requestAnimationFrame(this.animate);
    const dt = Math.min(this.clock.getDelta(), 0.1);
    if (this.playing) {
      this.time += dt;
      if (this.time > this.duration) this.time = 0; // 循环播放
      this.applyAt(this.time);
      this.cb.onTime?.(this.time);
    }
    for (const entry of this.entries.values()) {
      if (!entry.poseMode) entry.mixer?.update(dt);
    }
    this.orbit.update();
    for (const entry of this.entries.values()) entry.helper?.update();
    this.renderer.render(this.scene, this.activeRenderCamera());
  };

  dispose(): void {
    this.disposed = true;
    cancelAnimationFrame(this.rafId);
    this.resizeObserver.disconnect();
    this.gizmo.detach();
    this.gizmo.dispose();
    this.orbit.dispose();
    for (const id of [...this.entries.keys()]) this.deleteEntity(id);
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }
}

/* ---------------------------------------------------------------- 工具 */

/** 关键帧采样：返回 t 所在区间的起止帧与进度 u */
function sampleKeyframes(
  kfs: PrevizKeyframe[],
  t: number,
): { a: PrevizKeyframe; b: PrevizKeyframe | null; u: number } {
  if (kfs.length === 0) throw new Error("empty keyframes");
  if (t <= kfs[0].time) return { a: kfs[0], b: null, u: 0 };
  const last = kfs[kfs.length - 1];
  if (t >= last.time) return { a: last, b: null, u: 0 };
  for (let i = 0; i < kfs.length - 1; i++) {
    const a = kfs[i];
    const b = kfs[i + 1];
    if (t >= a.time && t < b.time) {
      const span = b.time - a.time || 1e-6;
      return { a, b, u: (t - a.time) / span };
    }
  }
  return { a: last, b: null, u: 0 };
}

/** 缓动曲线：默认 easeInOut（缓进缓出） */
function easeValue(u: number, kind: EasingType): number {
  const x = Math.max(0, Math.min(1, u));
  switch (kind) {
    case "linear":
      return x;
    case "easeIn":
      return x * x;
    case "easeOut":
      return 1 - (1 - x) * (1 - x);
    case "easeInOut":
    default:
      return x < 0.5 ? 2 * x * x : 1 - Math.pow(-2 * x + 2, 2) / 2;
  }
}

function clipCueAt(cues: PrevizClipCue[], t: number): string | null {
  let out: string | null = null;
  for (const c of [...cues].sort((a, b) => a.time - b.time)) {
    if (c.time <= t + 1e-6) out = c.clip;
    else break;
  }
  return out;
}

function cameraCueAt(cues: PrevizCameraCue[], t: number): string | null {
  let out: string | null = null;
  for (const c of [...cues].sort((a, b) => a.time - b.time)) {
    if (c.time <= t + 1e-6) out = c.cameraId;
    else break;
  }
  return out;
}

function lerp3(
  a: [number, number, number],
  b: [number, number, number],
  u: number,
): [number, number, number] {
  return [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u, a[2] + (b[2] - a[2]) * u];
}

function lerpNum(a: number, b: number, u: number): number {
  return a + (b - a) * u;
}

function quatFromEuler(e: [number, number, number]): THREE.Quaternion {
  return new THREE.Quaternion().setFromEuler(new THREE.Euler(e[0], e[1], e[2]));
}

/** 姿态插值：逐骨骼四元数 slerp（缺失的骨骼取任一侧） */
function slerpPose(
  a: Record<string, [number, number, number, number]>,
  b: Record<string, [number, number, number, number]>,
  u: number,
): Record<string, [number, number, number, number]> {
  const out: Record<string, [number, number, number, number]> = {};
  const names = new Set([...Object.keys(a), ...Object.keys(b)]);
  const qa = new THREE.Quaternion();
  const qb = new THREE.Quaternion();
  for (const n of names) {
    const pa = a[n];
    const pb = b[n];
    if (pa && pb) {
      qa.set(pa[0], pa[1], pa[2], pa[3]);
      qb.set(pb[0], pb[1], pb[2], pb[3]);
      qa.slerp(qb, u);
      out[n] = [qa.x, qa.y, qa.z, qa.w];
    } else {
      const p = pa ?? pb!;
      out[n] = p;
    }
  }
  return out;
}

function round3(v: number): number {
  return Math.round(v * 1000) / 1000;
}

function findEntityId(o: THREE.Object3D | null): string | null {
  let cur = o;
  while (cur) {
    if (cur.userData.entityId) return cur.userData.entityId as string;
    cur = cur.parent;
  }
  return null;
}

/** 几何体底面都在 y=0，摆到 (x, 0, z) 就落地 */
function buildPrimitiveGeometry(kind: PrevizPrimitiveKind): THREE.BufferGeometry {
  switch (kind) {
    case "box": {
      const g = new THREE.BoxGeometry(1, 1, 1);
      g.translate(0, 0.5, 0);
      return g;
    }
    case "sphere": {
      const g = new THREE.SphereGeometry(0.5, 24, 18);
      g.translate(0, 0.5, 0);
      return g;
    }
    case "cylinder": {
      const g = new THREE.CylinderGeometry(0.5, 0.5, 1, 24);
      g.translate(0, 0.5, 0);
      return g;
    }
    case "plane": {
      const g = new THREE.BoxGeometry(4, 0.08, 4);
      g.translate(0, 0.04, 0);
      return g;
    }
    case "ramp": {
      const shape = new THREE.Shape();
      shape.moveTo(0, 0);
      shape.lineTo(2, 0);
      shape.lineTo(0, 1);
      shape.closePath();
      const g = new THREE.ExtrudeGeometry(shape, { depth: 1.2, bevelEnabled: false });
      g.translate(-1, 0, -0.6);
      return g;
    }
    case "stairs": {
      const parts: THREE.BufferGeometry[] = [];
      for (let i = 0; i < 5; i++) {
        const step = new THREE.BoxGeometry(1.2, 0.3, 0.35);
        step.translate(0, 0.15 + i * 0.3, -i * 0.35);
        parts.push(step);
      }
      const merged = mergeGeometries(parts);
      return merged ?? parts[0];
    }
  }
}

function applyTransform(
  object: THREE.Object3D,
  t: { position: number[]; rotation: number[]; scale: number[] },
): void {
  object.position.fromArray(t.position);
  object.rotation.set(t.rotation[0], t.rotation[1], t.rotation[2]);
  object.scale.fromArray(t.scale);
}

function disposeTree(root: THREE.Object3D): void {
  root.traverse((o) => {
    if (o instanceof THREE.Mesh) {
      o.geometry.dispose();
      const mat = o.material;
      if (Array.isArray(mat)) mat.forEach((m) => m.dispose());
      else mat.dispose();
    }
  });
}

function shortId(id: string): string {
  return id.slice(0, 4);
}
