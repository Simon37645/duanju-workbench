<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import {
  AlertTriangle, ArrowUp, Check, ChevronDown, ImagePlus, Loader2, MessageCircleQuestion,
  Minus, Plus, RefreshCw, Send, Sparkles, Wrench, X, Zap,
} from "@lucide/vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useAgentStore } from "@/stores/agent";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import { fileUrl } from "@/api/events";
import { toast } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiTextarea from "@/ui/Textarea.vue";
import UiTooltip from "@/ui/Tooltip.vue";
import UiSegmented from "@/ui/Segmented.vue";
import UiInput from "@/ui/Input.vue";
import Markdown from "@/components/Markdown.vue";
import type { AgentToolCall } from "@/types/agent";
import type { PanelId } from "@/types/models";

const props = defineProps<{ panel: PanelId }>();
const agent = useAgentStore();
const project = useProjectStore();
const settings = useSettingsStore();

const open = ref(false);
const input = ref("");
const askDraft = ref("");
const attachments = ref<string[]>([]);
const scroller = ref<HTMLElement | null>(null);
const autoScroll = ref(true);
const showReasoning = ref<Record<string, boolean>>({});

const session = computed(() => agent.current);
const messages = computed(() => session.value?.display ?? []);
const modelLabel = computed(() => {
  const p = settings.activeLlm;
  return p ? p.model || p.name : "未配置模型";
});
const cacheRate = computed(() => Math.round((agent.usage?.hitRate ?? 0) * 100));
const hasUnread = computed(() => !open.value && (agent.running || !!agent.pendingAsk));

const MODES = [
  { label: "YOLO", value: "yolo" },
  { label: "自动", value: "auto" },
  { label: "确认", value: "confirm" },
];
const mode = computed({
  get: () => settings.settings?.agentMode ?? "auto",
  set: (v: string) => settings.save({ agentMode: v as never }),
});
const MODE_HINT: Record<string, string> = {
  yolo: "任何操作都直接执行，不打断你。适合信任度高的批量作业。",
  auto: "改数据自动执行，只有生图 / 生视频这类花钱的操作才要你确认。",
  confirm: "任何会改动项目数据的操作，执行前都先问你一次。",
};

watch(
  () => [messages.value.length, agent.streamText, agent.streamTools.length, agent.pendingAsk],
  async () => {
    if (!open.value || !autoScroll.value) return;
    await nextTick();
    const el = scroller.value;
    if (el) el.scrollTop = el.scrollHeight;
  },
);

// 有需要用户介入的事情时自动弹出
watch(
  () => [agent.pendingAsk?.id, agent.pending?.id],
  ([a, b]) => {
    if (a || b) open.value = true;
  },
);

function onScroll() {
  const el = scroller.value;
  if (!el) return;
  autoScroll.value = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
}

async function attach() {
  const picked = await openDialog({
    multiple: true,
    filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp"] }],
  });
  const list = Array.isArray(picked) ? picked : picked ? [picked] : [];
  attachments.value.push(...list);
}

async function send() {
  const text = input.value;
  if (!text.trim() && !attachments.value.length) return;
  input.value = "";
  const imgs = [...attachments.value];
  attachments.value = [];
  await agent.send(props.panel, text, imgs);
}

async function refreshContext() {
  if (agent.running) return;
  await agent.send(props.panel, "（请重新读取当前项目状态）", [], true);
}

async function answer(text: string) {
  const t = text.trim();
  if (!t || !agent.pendingAsk) return;
  askDraft.value = "";
  try {
    await agent.answer(agent.pendingAsk.id, t);
  } catch (e) {
    toast.err(String(e));
  }
}

async function approve(id: string, ok: boolean) {
  try {
    await agent.approve(id, ok);
  } catch (e) {
    toast.err(String(e));
  }
}

function toolTone(t: AgentToolCall) {
  if (t.state === "ok") return "ok" as const;
  if (t.state === "failed" || t.state === "rejected") return "err" as const;
  if (t.state === "awaiting-approval") return "warn" as const;
  return "neutral" as const;
}
function toolText(t: AgentToolCall) {
  return {
    pending: "等待", "awaiting-approval": "待确认", running: "执行中",
    ok: "完成", failed: "失败", rejected: "已拒绝",
  }[t.state];
}

const quickPrompts = computed(() => {
  const map: Record<PanelId, string[]> = {
    script: ["按 4 章拆分故事大纲", "把第 3 章写完"],
    style: ["根据题材推荐一个风格", "把风格词改得更电影感"],
    storyboard: ["把第 1 章拆成分镜", "第 2 章节奏太慢，重写"],
    asset: ["列出剧本里所有需要出图的资产", "给女主生成三视图"],
    prompt: ["给还没有提示词的镜头补上", "检查首帧图是否都配齐"],
    video: ["批量生成还没出片的镜头", "看看哪几条失败了"],
    edit: ["按镜头顺序铺时间线", "导出成片"],
    subtitle: ["用时间线第一个片段转写字幕"],
    checklist: ["汇报现在的完成度", "哪些还没做完"],
  };
  return map[props.panel] ?? [];
});

onMounted(() => {
  agent.loadSessions();
});
</script>

<template>
  <!-- 悬浮按钮 -->
  <button v-if="!open" class="fab" data-tour="simon" :class="{ busy: agent.running }" @click="open = true">
    <Sparkles :size="18" />
    <span v-if="hasUnread" class="fabdot" />
    <span class="fabtip">Simon</span>
  </button>

  <!-- 浮动对话窗 -->
  <Transition name="float">
    <section v-if="open" class="float">
      <header class="head">
        <div class="row grow" style="gap: 8px; min-width: 0">
          <div class="orb" :class="{ busy: agent.running }">
            <Sparkles :size="13" />
          </div>
          <div class="col" style="gap: 0; min-width: 0">
            <span class="t-sm" style="font-weight: 600">Simon</span>
            <span class="t-xs faint truncate">{{ modelLabel }}</span>
          </div>
        </div>

        <div class="row" style="gap: 6px; flex: 0 0 auto">
          <UiBadge v-if="cacheRate > 0" :tone="cacheRate >= 50 ? 'ok' : 'neutral'" size="xs">
            <Zap :size="9" /> {{ cacheRate }}%
          </UiBadge>
          <UiTooltip placement="bottom" content="让 Simon 重新读取项目状态（会牺牲一次缓存命中）">
            <UiButton variant="subtle" size="xs" icon :disabled="agent.running" @click="refreshContext">
              <template #icon><RefreshCw :size="13" /></template>
            </UiButton>
          </UiTooltip>
          <UiTooltip placement="bottom" content="新建对话">
            <UiButton variant="subtle" size="xs" icon @click="agent.newSession(props.panel)">
              <template #icon><Plus :size="13" /></template>
            </UiButton>
          </UiTooltip>
          <UiButton variant="subtle" size="xs" icon @click="open = false">
            <template #icon><Minus :size="13" /></template>
          </UiButton>
        </div>
      </header>

      <!-- 权限模式 -->
      <div class="modebar">
        <UiSegmented v-model="mode" size="xs" :items="MODES" />
        <span class="t-xs faint truncate">{{ MODE_HINT[mode] }}</span>
      </div>

      <!-- 消息 -->
      <div ref="scroller" class="body scroll" @scroll="onScroll">
        <div v-if="!messages.length && !agent.streamText && !agent.streamTools.length" class="intro">
          <div class="intro-orb"><Sparkles :size="18" /></div>
          <p class="t-sm dim" style="text-align: center; line-height: 1.7">
            Simon 能跨面板动手：写剧本、拆分镜、建资产、<br />提交生成、铺时间线都在它的能力范围内。
          </p>
          <div class="quick">
            <button v-for="q in quickPrompts" :key="q" class="q" @click="input = q">{{ q }}</button>
          </div>
        </div>

        <template v-for="m in messages" :key="m.id">
          <div v-if="m.role === 'user'" class="msg-user">
            <div class="bubble-user">{{ m.text }}</div>
            <div v-if="m.images?.length" class="row wrap" style="justify-content: flex-end">
              <img v-for="(img, i) in m.images" :key="i" :src="fileUrl(img)" class="attach" />
            </div>
          </div>

          <div v-else class="msg-bot">
            <button v-if="m.reasoning" class="reason-toggle" @click="showReasoning[m.id] = !showReasoning[m.id]">
              <ChevronDown :size="11" :class="{ rot: !showReasoning[m.id] }" />
              推理过程
            </button>
            <pre v-if="m.reasoning && showReasoning[m.id]" class="reason">{{ m.reasoning }}</pre>

            <div v-for="t in m.toolCalls" :key="t.id" class="tool">
              <div class="row" style="gap: 7px; min-width: 0">
                <div class="ticon" :class="toolTone(t)">
                  <MessageCircleQuestion v-if="t.name === 'ask_user'" :size="11" />
                  <Wrench v-else :size="11" />
                </div>
                <span class="t-sm truncate" style="font-weight: 500">{{ t.title }}</span>
                <UiBadge :tone="toolTone(t)" size="xs">{{ toolText(t) }}</UiBadge>
                <span class="grow" />
                <span class="t-xs faint num">{{ t.durationMs }}ms</span>
              </div>
              <div
                v-if="t.summary"
                class="tsum t-xs"
                :class="{ bad: t.state === 'failed' || t.state === 'rejected' }"
              >
                {{ t.summary }}
              </div>
            </div>

            <div v-if="m.text" class="bubble-bot"><Markdown :text="m.text" /></div>
          </div>
        </template>

        <!-- 流式中 -->
        <div v-if="agent.running || agent.streamText || agent.streamTools.length" class="msg-bot">
          <pre v-if="agent.streamReasoning" class="reason">{{ agent.streamReasoning }}</pre>

          <div v-for="t in agent.streamTools" :key="t.id" class="tool">
            <div class="row" style="gap: 7px; min-width: 0">
              <div class="ticon" :class="toolTone(t)">
                <Loader2 v-if="t.state === 'running'" :size="11" class="spin" />
                <Wrench v-else :size="11" />
              </div>
              <span class="t-sm truncate" style="font-weight: 500">{{ t.title }}</span>
              <UiBadge :tone="toolTone(t)" size="xs">{{ toolText(t) }}</UiBadge>
            </div>
            <div
              v-if="t.summary"
              class="tsum t-xs"
              :class="{ bad: t.state === 'failed' || t.state === 'rejected' }"
            >
              {{ t.summary }}
            </div>

            <div v-if="t.state === 'awaiting-approval'" class="approve">
              <AlertTriangle :size="13" />
              <span class="t-xs grow">
                {{ t.costly ? "这个操作会消耗生成额度，确认执行？" : "这一步会修改项目数据，确认执行？" }}
              </span>
              <UiButton size="xs" variant="primary" @click="approve(t.id, true)">确认</UiButton>
              <UiButton size="xs" variant="ghost" @click="approve(t.id, false)">拒绝</UiButton>
            </div>
          </div>

          <div v-if="agent.streamText" class="bubble-bot">
            <Markdown :text="agent.streamText" streaming />
          </div>
          <div v-else-if="agent.running && !agent.streamTools.length" class="thinking">
            <span class="dotpulse" /> 思考中
          </div>
        </div>
      </div>

      <!-- 提问卡片：Simon 在等你的回答 -->
      <div v-if="agent.pendingAsk" class="askcard">
        <div class="row" style="gap: 8px; align-items: flex-start">
          <MessageCircleQuestion :size="15" style="color: var(--accent); flex: 0 0 auto; margin-top: 2px" />
          <div class="col grow" style="gap: 2px; min-width: 0">
            <span class="t-sm" style="font-weight: 500">{{ agent.pendingAsk.question }}</span>
            <span v-if="agent.pendingAsk.why" class="t-xs faint">{{ agent.pendingAsk.why }}</span>
          </div>
        </div>
        <div v-if="agent.pendingAsk.options.length" class="row wrap" style="gap: 5px">
          <UiButton
            v-for="o in agent.pendingAsk.options"
            :key="o"
            variant="outline"
            size="sm"
            @click="answer(o)"
          >
            {{ o }}
          </UiButton>
        </div>
        <div class="row" style="gap: 6px">
          <UiInput v-model="askDraft" placeholder="或者直接输入你的回答" @enter="answer(askDraft)" />
          <UiButton variant="primary" size="sm" icon :disabled="!askDraft.trim()" @click="answer(askDraft)">
            <template #icon><Send :size="13" /></template>
          </UiButton>
        </div>
      </div>

      <!-- 输入 -->
      <footer class="foot">
        <div v-if="attachments.length" class="row wrap" style="margin-bottom: 7px">
          <div v-for="(a, i) in attachments" :key="i" class="chip">
            <img :src="fileUrl(a)" />
            <span class="t-xs truncate" style="max-width: 70px">{{ a.split(/[\\/]/).pop() }}</span>
            <button class="x" @click="attachments.splice(i, 1)"><X :size="10" /></button>
          </div>
        </div>

        <div class="composer">
          <UiTextarea
            v-model="input"
            :rows="1"
            :resize="false"
            :placeholder="agent.running ? '正在生成…' : '让 Simon 做什么？'"
            :disabled="agent.running"
            @keydown="
              (e: KeyboardEvent) => {
                if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); send(); }
              }
            "
          />
          <div class="row-between" style="margin-top: 7px">
            <div class="row" style="gap: 4px">
              <UiButton variant="subtle" size="xs" icon @click="attach">
                <template #icon><ImagePlus :size="13" /></template>
              </UiButton>
              <span class="t-xs faint">Ctrl+Enter 发送</span>
            </div>
            <button class="send" :disabled="agent.running || (!input.trim() && !attachments.length)" @click="send">
              <ArrowUp :size="14" />
            </button>
          </div>
        </div>
        <div v-if="agent.lastError" class="err t-xs">{{ agent.lastError }}</div>
      </footer>
    </section>
  </Transition>
</template>

<style scoped>
/* ------------------------------------------------------------ 悬浮按钮 */
.fab {
  position: absolute;
  right: 20px;
  bottom: 20px;
  z-index: 120;
  width: 48px;
  height: 48px;
  border-radius: 16px;
  border: 1px solid var(--accent-line);
  background: linear-gradient(145deg, var(--accent), var(--accent-press));
  color: var(--accent-fg);
  display: grid;
  place-items: center;
  cursor: pointer;
  box-shadow: var(--shadow-lg), 0 0 0 0 var(--accent-soft);
  transition: transform var(--fast), box-shadow var(--fast);
}
.fab:hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-lg), 0 0 0 6px var(--accent-soft);
}
.fab.busy {
  animation: bobbing 1.8s ease-in-out infinite;
}
@keyframes bobbing {
  50% {
    transform: translateY(-3px);
  }
}
.fabtip {
  position: absolute;
  right: 56px;
  top: 50%;
  transform: translateY(-50%);
  background: var(--tooltip-bg);
  color: var(--tooltip-fg);
  font-size: var(--t-xs);
  padding: 3px 8px;
  border-radius: var(--r-sm);
  opacity: 0;
  pointer-events: none;
  transition: opacity var(--fast);
  white-space: nowrap;
}
.fab:hover .fabtip {
  opacity: 1;
}
.fabdot {
  position: absolute;
  top: 6px;
  right: 6px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--err);
  border: 2px solid var(--bg);
}

/* ------------------------------------------------------------ 浮窗 */
.float {
  position: absolute;
  right: 20px;
  bottom: 20px;
  z-index: 130;
  width: 460px;
  height: min(72vh, 640px);
  display: flex;
  flex-direction: column;
  background: var(--surface-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-xl);
  box-shadow: var(--shadow-lg);
  overflow: hidden;
}
.float-enter-active,
.float-leave-active {
  transition: opacity 160ms var(--ease), transform 200ms var(--ease);
}
.float-enter-from,
.float-leave-to {
  opacity: 0;
  transform: translateY(10px) scale(0.98);
}

.head {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: 10px 10px 10px 12px;
  border-bottom: 1px solid var(--line-faint);
  flex: 0 0 auto;
}
.orb {
  width: 26px;
  height: 26px;
  flex: 0 0 auto;
  border-radius: 8px;
  display: grid;
  place-items: center;
  background: var(--accent-soft);
  color: var(--accent);
  border: 1px solid var(--accent-line);
}
.orb.busy {
  animation: glow 1.6s ease-in-out infinite;
}
@keyframes glow {
  50% {
    box-shadow: 0 0 0 4px var(--accent-soft);
  }
}

.modebar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 12px;
  border-bottom: 1px solid var(--line-faint);
  background: var(--surface-1);
  flex: 0 0 auto;
  min-width: 0;
}

.body {
  flex: 1;
  min-height: 0;
  padding: var(--sp-4) var(--sp-3);
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}

.intro {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-4);
  padding: var(--sp-5) var(--sp-2);
}
.intro-orb {
  width: 44px;
  height: 44px;
  border-radius: 14px;
  display: grid;
  place-items: center;
  background: var(--accent-soft);
  color: var(--accent);
  border: 1px solid var(--accent-line);
}
.quick {
  display: flex;
  flex-direction: column;
  gap: 5px;
  width: 100%;
}
.q {
  text-align: left;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  color: var(--fg-dim);
  padding: 8px 11px;
  font-size: var(--t-sm);
  cursor: pointer;
  transition: border-color var(--fast), color var(--fast), background var(--fast);
}
.q:hover {
  background: var(--surface-4);
  color: var(--fg);
  border-color: var(--line-strong);
}

.msg-user {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 6px;
}
.bubble-user {
  max-width: 88%;
  background: var(--accent);
  color: var(--accent-fg);
  border-radius: var(--r-lg) var(--r-lg) 4px var(--r-lg);
  padding: 8px 12px;
  font-size: var(--t-base);
  white-space: pre-wrap;
  word-break: break-word;
}
.msg-bot {
  display: flex;
  flex-direction: column;
  gap: 7px;
  align-items: flex-start;
}
.bubble-bot {
  max-width: 100%;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: 4px var(--r-lg) var(--r-lg) var(--r-lg);
  padding: 10px 12px;
  font-size: var(--t-base);
}

.reason-toggle {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: none;
  border: none;
  color: var(--fg-ghost);
  font-size: var(--t-xs);
  cursor: pointer;
  padding: 0;
}
.reason-toggle:hover {
  color: var(--fg-dim);
}
.reason-toggle .rot {
  transform: rotate(-90deg);
}
.reason {
  width: 100%;
  background: var(--code-bg);
  border: 1px solid var(--line-faint);
  border-left: 2px solid var(--surface-5);
  border-radius: var(--r-sm);
  padding: 8px 10px;
  margin: 0;
  color: var(--fg-faint);
  font-size: var(--t-xs);
  line-height: 1.7;
  white-space: pre-wrap;
  max-height: 200px;
  overflow: auto;
}

.tool {
  width: 100%;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  padding: 7px 9px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.ticon {
  width: 18px;
  height: 18px;
  flex: 0 0 auto;
  border-radius: 5px;
  display: grid;
  place-items: center;
  background: var(--surface-4);
  color: var(--fg-faint);
}
.ticon.ok {
  background: var(--ok-soft);
  color: var(--ok);
}
.ticon.err {
  background: var(--err-soft);
  color: var(--err);
}
.ticon.warn {
  background: var(--warn-soft);
  color: var(--warn);
}
.tsum {
  color: var(--fg-faint);
  line-height: 1.6;
  padding-left: 25px;
}
.tsum.bad {
  color: var(--err);
}
.spin {
  animation: spin 900ms linear infinite;
}

.approve {
  display: flex;
  align-items: center;
  gap: 7px;
  margin-top: 2px;
  padding: 7px 9px;
  background: var(--warn-soft);
  border: 1px solid var(--warn-soft);
  border-radius: var(--r-sm);
  color: var(--warn);
}

.thinking {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  color: var(--fg-faint);
  font-size: var(--t-sm);
}
.dotpulse {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent);
  animation: pulse 1.2s ease-in-out infinite;
}
@keyframes pulse {
  50% {
    opacity: 0.25;
    transform: scale(0.8);
  }
}

/* ------------------------------------------------------------ 提问卡片 */
.askcard {
  flex: 0 0 auto;
  margin: 0 var(--sp-3) var(--sp-2);
  padding: var(--sp-3);
  background: var(--accent-soft);
  border: 1px solid var(--accent-line);
  border-radius: var(--r-lg);
  display: flex;
  flex-direction: column;
  gap: 10px;
  animation: fade-in 200ms var(--ease);
}

.foot {
  flex: 0 0 auto;
  padding: var(--sp-3);
  border-top: 1px solid var(--line-faint);
}
.composer {
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
  padding: 9px 10px;
  transition: border-color var(--fast);
}
.composer:focus-within {
  border-color: var(--accent-line);
}
.send {
  width: 26px;
  height: 26px;
  border-radius: var(--r-sm);
  border: none;
  background: var(--accent);
  color: var(--accent-fg);
  display: grid;
  place-items: center;
  cursor: pointer;
  transition: background var(--fast), opacity var(--fast);
}
.send:hover:not(:disabled) {
  background: var(--accent-hover);
}
.send:disabled {
  background: var(--surface-5);
  color: var(--fg-ghost);
  cursor: not-allowed;
}
.chip {
  display: flex;
  align-items: center;
  gap: 5px;
  background: var(--surface-4);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  padding: 2px 5px 2px 2px;
}
.chip img {
  width: 24px;
  height: 24px;
  object-fit: cover;
  border-radius: 4px;
}
.chip .x {
  background: none;
  border: none;
  color: var(--fg-ghost);
  cursor: pointer;
  padding: 0;
  display: grid;
}
.attach {
  max-width: 110px;
  max-height: 80px;
  border-radius: var(--r);
  border: 1px solid var(--line);
}
.err {
  color: var(--err);
  margin-top: 6px;
  line-height: 1.6;
}
</style>
