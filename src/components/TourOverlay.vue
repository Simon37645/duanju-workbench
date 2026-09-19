<script setup lang="ts">
/**
 * 新手教程。
 *
 * 实现取的是「有目标元素就高亮它，没有就把卡片居中」的稳妥路线 ——
 * 教程会在首页和工作区都可能弹出，硬绑 DOM 很容易在某些页面上找不到锚点。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  ArrowLeft, ArrowRight, BookOpen, Check, Layers, Sparkles, Terminal, X, Zap,
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
      "这里把一部短剧的生产拆成九个面板，从剧本一路做到成片。\n" +
      "助手 Simon 全程参与，能跨面板直接改数据。\n\n" +
      "大概两分钟看完，之后随时可以在「设置 → 运行环境」里重看。",
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
      "九步按顺序走：剧本 → 风格 → 分镜 → 资产 → 视频提示词 → 生视频 → 剪辑 → 字幕 → 核对。\n" +
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
    title: "右下角的 Simon",
    body:
      "点开它，就能让 Simon 干活：拆章节、写分镜、建资产、提交生成、铺时间线。\n" +
      "它是一个助手 —— 你在哪个面板打开它都行，不受面板限制。\n\n" +
      "三种权限模式在浮窗顶部切换：\n" +
      "· YOLO —— 全部直接执行\n" +
      "· 自动编辑 —— 只有花钱的操作才问你（默认）\n" +
      "· 变更前确认 —— 改数据前都问一次\n\n" +
      "拿不准的时候它还会主动弹卡片问你。\n\n" +
      "输入框里打 @ 可以点名引用技能 / 章节 / 资产，只把需要的东西喂进去；\n" +
      "顶部那个百分比是上下文水位，快满了会自动压缩，也可以手动压。",
    icon: Sparkles,
    target: "[data-tour='simon']",
    place: "left",
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
      "建议的顺序：先把剧本写完 → 定风格 → 拆分镜 → 建资产出参考图 → 写视频提示词 → 批量生视频 → 铺时间线导出 → 转字幕 → 最后过一遍 Checklist。\n\n" +
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
