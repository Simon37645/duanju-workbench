<script setup lang="ts">
/**
 * 新手教程。
 *
 * 实现取的是「有目标元素就高亮它，没有就把卡片居中」的稳妥路线 ——
 * 教程会在首页和工作区都可能弹出，硬绑 DOM 很容易在某些页面上找不到锚点。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  ArrowLeft, ArrowRight, BookOpen, Boxes, Check, Clapperboard, Cpu, Layers, Sparkles,
  Terminal, X, Zap,
} from "@lucide/vue";
import UiButton from "@/ui/Button.vue";
import { useSettingsStore } from "@/stores/settings";

interface Step {
  title: string;
  body: string;
  icon: unknown;
  /** CSS 选择器，找不到就把卡片居中 */
  target?: string;
  place?: "right" | "bottom" | "left" | "top";
}

const STEPS: Step[] = [
  {
    title: "欢迎来到 Simon 短剧工作台",
    body:
      "九个面板把一部短剧从剧本做到成片，助手 Simon 全程参与：它能跨面板直接改数据、\n" +
      "调生图生视频、铺时间线。\n\n" +
      "大约三分钟看完，之后随时能在「设置 → 运行环境」里重看。",
    icon: Sparkles,
  },
  {
    title: "先建一个项目",
    body:
      "一个项目就是一个普通文件夹：JSON 存结构化数据、Markdown 存正文、媒体就是媒体文件。\n" +
      "没有数据库，整个目录拷走就能在另一台机器打开。",
    icon: Layers,
  },
  {
    title: "左侧是流水线",
    body:
      "九步按顺序走：剧本 → 风格 → 分镜 → 资产 → 3D预演 → 视频提示词 → 生视频 → 剪辑 → 字幕。\n" +
      "每一项右边的数字是完成度，打勾表示这一步做完了。\n\n" +
      "不用严格按顺序，缺什么补什么也行。",
    icon: Layers,
    target: "nav.side-nav",
    place: "right",
  },
  {
    title: "选好模型，看清缓存",
    body:
      "右上角选 agent 用哪个模型。还没配接口也没关系，占位模型能把整条流程跑通。\n\n" +
      "旁边那个百分比是「提示词缓存命中率」。每个会话的上下文前缀是冻结的，" +
      "连续对话时大部分输入都按缓存价计费，成本低很多。",
    icon: Zap,
    target: "[data-tour='model']",
    place: "bottom",
  },
  {
    title: "右下角的 Simon：两种对话引擎",
    body:
      "点开浮窗就能让 Simon 干活，它不受面板限制。\n\n" +
      "输入框下方的引擎按钮可切换：\n" +
      "· 自研引擎（默认）—— 38 个工具、冻结前缀缓存、花钱确认；\n" +
      "· pi 引擎 —— 接入开源 agent harness，工具与审批和自研完全一致。\n\n" +
      "三种权限模式在浮窗顶部：YOLO 全放开 / 自动编辑（默认，只有花钱操作才问）/ 变更前确认。\n" +
      "输入框打 @ 可以点名引用技能、章节、资产。",
    icon: Cpu,
    target: "[data-tour='simon']",
    place: "left",
  },
  {
    title: "3D 预演 · 摆场景和转动物体",
    body:
      "「3D预演」面板的白模视口铺满整个区域，左上角菜单负责添加：几何体、X Bot / Y Bot、机位。\n\n" +
      "选中物体后就能变换它，三种方式任选：\n" +
      "· 拖场景里的彩色手柄（当前档位就是菜单栏点亮的按钮）；\n" +
      "· 按键盘 G 移动 / R 旋转 / S 缩放 —— 按 R 会出现圆环，拖住圆环转动物体；\n" +
      "· 右侧属性面板直接填数值（位置 / 旋转按度 / 缩放）。",
    icon: Boxes,
    target: "[data-tour='previz-menubar']",
    place: "bottom",
  },
  {
    title: "3D 预演 · 打关键帧做动画",
    body:
      "底部时间轴：拖到某个时刻 → 摆好位置 → 点属性面板的「打关键帧」。\n" +
      "换个时刻再摆一次、再打一个 → 按空格播放，中间会自动补间（默认缓进缓出，\n" +
      "点中关键帧可切换线性 / 缓入 / 缓出，也能拖着改时间）。\n\n" +
      "机位也一样能打关键帧；小键盘 0 进出机位视角看构图，焦距（mm）在属性面板调，\n" +
      "要给多个机位切换就在机位属性里点「在当前时间加『切到此机位』」。\n" +
      "角色可以选动作片段（idle / walk…），也能选关节用旋转手柄摆姿势后打关键帧。",
    icon: Clapperboard,
    target: "[data-tour='previz-timeline']",
    place: "top",
  },
  {
    title: "3D 预演 · 导出白模参考视频",
    body:
      "菜单栏的「导出白模视频」按项目画幅逐帧渲染（无材质白模）再交给 ffmpeg 合成 mp4，\n" +
      "落在项目 previz/ 目录，拿去给生视频模型当运镜参考刚好。\n\n" +
      "如果你的团队用专业预演工具（比如开源的导演台 DirectorDesk），\n" +
      "也可以两边并行：我们这边出快速白模，它那边做精细灯光运镜。",
    icon: Clapperboard,
    target: "[data-tour='previz-export']",
    place: "bottom",
  },
  {
    title: "把你自己的方法接进来",
    body:
      "「设置 → 技能」可以导入你自己的一套流程与规范（Markdown，或带 SKILL.md 的技能目录）。\n\n" +
      "放进来的正文不会塞进系统提示 —— 提示里只有一份目录，Simon 需要时自己去读。\n" +
      "所以几万字的手册也不会让每轮对话都变贵。",
    icon: BookOpen,
  },
  {
    title: "底部是任务队列",
    body:
      "生图、生视频、转码、转写都在后台跑，进度和失败原因都在这里。\n" +
      "任务会并发执行，你可以一边等一边继续做别的。",
    icon: Terminal,
    target: "[data-tour='jobs']",
    place: "top",
  },
  {
    title: "可以开始了",
    body:
      "建议的顺序：先把剧本写完 → 定风格 → 拆分镜 → 建资产出参考图 → 3D预演摆机位/打动画 → " +
      "写视频提示词 → 批量生视频 → 铺时间线导出 → 转字幕。\n\n" +
      "随时可以点右下角的 Simon 让它替你干活。",
    icon: Check,
  },
];

const settings = useSettingsStore();
const index = ref(0);
const rect = ref<DOMRect | null>(null);
const open = computed(() => settings.settings?.onboarded === false);
const step = computed(() => STEPS[index.value]);
const isLast = computed(() => index.value === STEPS.length - 1);

function locate() {
  const sel = step.value.target;
  if (!sel) {
    rect.value = null;
    return;
  }
  const el = document.querySelector(sel);
  rect.value = el ? el.getBoundingClientRect() : null;
}

watch([open, index], async () => {
  await nextTick();
  locate();
});

function onResize() {
  if (open.value) locate();
}

onMounted(() => {
  window.addEventListener("resize", onResize);
  window.addEventListener("keydown", onKey);
});
onBeforeUnmount(() => {
  window.removeEventListener("resize", onResize);
  window.removeEventListener("keydown", onKey);
});

function onKey(e: KeyboardEvent) {
  if (!open.value) return;
  if (e.key === "Escape") finish();
  if (e.key === "ArrowRight") next();
  if (e.key === "ArrowLeft") prev();
}

async function next() {
  if (isLast.value) return finish();
  index.value += 1;
}
function prev() {
  if (index.value > 0) index.value -= 1;
}
async function finish() {
  await settings.save({ onboarded: true });
}

/** 卡片位置：有锚点就贴锚点，没有就居中 */
const cardStyle = computed(() => {
  const r = rect.value;
  if (!r) return { left: "50%", top: "50%", transform: "translate(-50%, -50%)" };
  const gap = 16;
  const W = 380;
  switch (step.value.place) {
    case "right":
      return { left: `${Math.min(r.right + gap, window.innerWidth - W - 16)}px`, top: `${Math.max(16, Math.min(r.top, window.innerHeight - 300))}px` };
    case "left":
      return { left: `${Math.max(16, r.left - W - gap)}px`, top: `${Math.max(16, Math.min(r.top, window.innerHeight - 360))}px` };
    case "top":
      return { left: `${Math.max(16, Math.min(r.left, window.innerWidth - W - 16))}px`, top: `${Math.max(16, r.top - 300)}px` };
    default:
      return { left: `${Math.max(16, Math.min(r.left, window.innerWidth - W - 16))}px`, top: `${Math.min(r.bottom + gap, window.innerHeight - 300)}px` };
  }
});
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="tour">
      <!-- 高亮挖孔：用四块遮罩围出来，比 clip-path 兼容性好 -->
      <template v-if="rect">
        <div class="dim" :style="{ left: 0, top: 0, right: 0, height: rect.top + 'px' }" />
        <div class="dim" :style="{ left: 0, top: rect.bottom + 'px', right: 0, bottom: 0 }" />
        <div class="dim" :style="{ left: 0, top: rect.top + 'px', width: rect.left + 'px', height: rect.height + 'px' }" />
        <div class="dim" :style="{ left: rect.right + 'px', top: rect.top + 'px', right: 0, height: rect.height + 'px' }" />
        <div class="ring" :style="{ left: rect.left - 4 + 'px', top: rect.top - 4 + 'px', width: rect.width + 8 + 'px', height: rect.height + 8 + 'px' }" />
      </template>
      <div v-else class="dim" style="inset: 0" />

      <!-- 卡片 -->
      <div class="card-tour fade-in" :style="cardStyle">
        <div class="row-between">
          <div class="row" style="gap: 8px">
            <div class="ticon"><component :is="step.icon" :size="15" /></div>
            <span class="t-xs faint num">{{ index + 1 }} / {{ STEPS.length }}</span>
          </div>
          <button class="x" @click="finish"><X :size="14" /></button>
        </div>

        <h3>{{ step.title }}</h3>
        <p>{{ step.body }}</p>

        <div class="row-between" style="margin-top: 4px">
          <div class="dots">
            <span v-for="(s, i) in STEPS" :key="i" class="dot" :class="{ on: i === index }" />
          </div>
          <div class="row" style="gap: 6px">
            <UiButton variant="ghost" size="sm" @click="finish">跳过</UiButton>
            <UiButton v-if="index > 0" variant="ghost" size="sm" @click="prev">
              <template #icon><ArrowLeft :size="12" /></template>
              上一步
            </UiButton>
            <UiButton variant="primary" size="sm" @click="next">
              {{ isLast ? "开始使用" : "下一步" }}
              <template v-if="!isLast" #icon><ArrowRight :size="12" /></template>
            </UiButton>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.tour {
  position: fixed;
  inset: 0;
  z-index: 400;
}
.dim {
  position: fixed;
  background: var(--mask-bg);
  backdrop-filter: blur(1.5px);
}
.ring {
  position: fixed;
  border-radius: var(--r-lg);
  border: 2px solid var(--accent);
  box-shadow: 0 0 0 4px var(--accent-soft);
  pointer-events: none;
  transition: all 200ms var(--ease);
}

.card-tour {
  position: fixed;
  width: 380px;
  max-width: calc(100vw - 32px);
  background: var(--surface-1);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-xl);
  box-shadow: var(--shadow-lg);
  padding: var(--sp-4);
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.ticon {
  width: 26px;
  height: 26px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  background: var(--accent-soft);
  color: var(--accent);
  border: 1px solid var(--accent-line);
}
.x {
  background: none;
  border: none;
  color: var(--fg-ghost);
  cursor: pointer;
  border-radius: var(--r-sm);
  width: 24px;
  height: 24px;
  display: grid;
  place-items: center;
}
.x:hover {
  background: var(--surface-3);
  color: var(--fg);
}
h3 {
  font-size: var(--t-md);
  font-weight: 600;
}
p {
  font-size: var(--t-sm);
  color: var(--fg-dim);
  line-height: 1.85;
  white-space: pre-wrap;
  margin: 0;
}
.dots {
  display: flex;
  gap: 4px;
}
.dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--surface-5);
  transition: background var(--fast), width var(--fast);
}
.dot.on {
  background: var(--accent);
  width: 14px;
  border-radius: var(--r-full);
}
</style>
