<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import {
  AlertTriangle, ArrowUp, BookText, Check, ChevronDown, Gauge, ImagePlus, Images,
  Loader2, MessageCircleQuestion, Minus, Plus, RefreshCw, Send, Sparkles, Square, Trash2,
  Wrench, X, Zap,
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
import UiPopover from "@/ui/Popover.vue";
import UiSegmented from "@/ui/Segmented.vue";
import UiInput from "@/ui/Input.vue";
import Markdown from "@/components/Markdown.vue";
import { api, errorText } from "@/api/ipc";
import type { AgentToolCall, MentionItem } from "@/types/agent";
import type { PanelId } from "@/types/models";

const props = defineProps<{ panel: PanelId }>();
const agent = useAgentStore();
const project = useProjectStore();
const settings = useSettingsStore();

const open = ref(false);
const input = ref("");
const showSessions = ref(false);
const skills = ref<{ name: string; description: string }[]>([]);
const askDraft = ref("");
const attachments = ref<string[]>([]);
const scroller = ref<HTMLElement | null>(null);
const mentionIndex = ref(0);
const autoScroll = ref(true);
const showReasoning = ref<Record<string, boolean>>({});
/** 工具卡片的展开状态（看完整结果） */
const openTools = ref<Record<string, boolean>>({});
function toggleTool(id: string) {
  openTools.value[id] = !openTools.value[id];
}
function toolDataText(t: AgentToolCall): string {
  if (t.data === null || t.data === undefined) return t.summary || "（没有附加数据）";
  try {
    return typeof t.data === "string" ? t.data : JSON.stringify(t.data, null, 2);
  } catch {
    return String(t.data);
  }
}

/* ------------------------------------------------------- pi 思考强度 */

const THINKING_OPTS = [
  { label: "思考：关", value: "off" },
  { label: "思考：最低", value: "minimal" },
  { label: "思考：低", value: "low" },
  { label: "思考：中", value: "medium" },
  { label: "思考：高", value: "high" },
];
async function setThinking(v: string | null) {
  if (!v) return;
  await agent.setPiThinking(v);
  toast.info(`思考强度已设为「${THINKING_OPTS.find((o) => o.value === v)?.label ?? v}」`);
}

const session = computed(() => agent.current);
const messages = computed(() =>
  agent.engine === "pi" ? agent.piHistory : (session.value?.display ?? []),
);
const modelLabel = computed(() => {
  const p = settings.activeLlm;
  return p ? p.model || p.name : "未配置模型";
});
const cacheRate = computed(() => Math.round((agent.usage?.hitRate ?? 0) * 100));
const hasUnread = computed(() => !open.value && (agent.running || !!agent.pendingAsk));

/* ------------------------------------------------------------ 上下文 */

const stats = computed(() => agent.context);
const ctxPct = computed(() => Math.round(stats.value?.percent ?? 0));
const ctxTone = computed(() => {
  const p = stats.value?.percent ?? 0;
  if (p >= 90) return "err";
  if (p >= 75) return "warn";
  return "ok";
});

const sessions = computed(() =>
  agent.sessions
    .slice()
    .sort((a, b) => (b.updatedAt ?? "").localeCompare(a.updatedAt ?? "")),
);

/* ------------------------------------------------------------ @ 引用 */

/** 光标处正在输入的 @ 片段；没有则为 null */
const mentionQuery = computed(() => {
  const m = /@([^\s@:]*)$/.exec(input.value);
  return m ? m[1] : null;
});
const mentionOpen = computed(() => mentionQuery.value !== null);

const mentionItems = computed<MentionItem[]>(() => {
  if (!mentionOpen.value) return [];
  const q = (mentionQuery.value ?? "").toLowerCase();
  const items: MentionItem[] = [
    ...skills.value.map((k) => ({
      kind: "skill" as const,
      label: k.name,
      hint: k.description,
      token: `@skill:${k.name}`,
    })),
    ...project.chapters.map((c) => ({
      kind: "chapter" as const,
      label: `第${c.index}章 ${c.title}`,
      hint: "带上章节正文",
      token: `@chapter:${c.index}`,
    })),
    ...project.assets.map((a) => ({
      kind: "asset" as const,
      label: a.name,
      hint: "带上资产描述与参考图",
      token: `@asset:${a.name}`,
    })),
  ];
  return items.filter((i) => !q || i.label.toLowerCase().includes(q) || i.token.includes(q)).slice(0, 8);
});

function pickMention(item: MentionItem) {
  input.value = input.value.replace(/@([^\s@:]*)$/, item.token + " ");
}

const KIND_ICON: Record<string, unknown> = {
  skill: BookText,
  chapter: BookText,
  asset: Images,
};

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
// 切会话时把水位、技能候选都刷新一遍
watch(
  () => agent.activeSessionId,
  async (id) => {
    showSessions.value = false;
    await agent.refreshContext(id ?? undefined);
  },
);

watch(mentionItems, () => (mentionIndex.value = 0));

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

/** 输入框键位：@ 菜单打开时先让菜单吃掉方向键与回车 */
function onComposerKey(e: KeyboardEvent) {
  if (mentionOpen.value && mentionItems.value.length) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      mentionIndex.value = (mentionIndex.value + 1) % mentionItems.value.length;
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      mentionIndex.value =
        (mentionIndex.value - 1 + mentionItems.value.length) % mentionItems.value.length;
      return;
    }
    if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      pickMention(mentionItems.value[mentionIndex.value] ?? mentionItems.value[0]);
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      input.value = input.value.replace(/@([^\s@:]*)$/, "");
      return;
    }
  }
  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    send();
  }
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
    previz: ["帮我摆一组客厅对话的白模和机位", "看看现在的机位怎么调整更好"],
  };
  return map[props.panel] ?? [];
});

onMounted(async () => {
  await agent.loadSessions();
  await agent.refreshContext();
  void agent.refreshPiStatus();
  try {
    skills.value = (await api.skillList()).filter((s) => s.enabled);
  } catch {
    skills.value = [];
  }
});

/* ------------------------------------------------------------ 引擎切换 */

const llmOptions = computed(() =>
  settings.byKind("llm").map((p) => ({ label: p.name, hint: p.model, value: p.id })),
);

async function switchModel(id: string | null) {
  if (!id) return;
  await settings.save({ activeLlmProviderId: id });
  toast.info("已切换模型，提示词前缀会重建（缓存需重新写入一次）");
}

const engineHint = computed(() => {
  const s = agent.piStatus;
  if (agent.engine === "pi") {
    return `pi 引擎（${s?.version ? "v" + s.version : "已启用"}）：${s?.detail ?? "状态未知"}。点击切回自研引擎`;
  }
  if (!s) return "pi 引擎状态未知（点击检测）";
  return s.available
    ? `切换到 pi 引擎${s.version ? "（v" + s.version + "）" : ""}`
    : `pi 引擎暂不可用：${s.detail}`;
});

async function toggleEngine() {
  if (agent.running) return;
  if (agent.engine === "pi") {
    await agent.setEngine("native");
    toast.info("已切回自研引擎");
    return;
  }
  const s = await agent.refreshPiStatus();
  if (!s?.available) {
    toast.warn(`pi 引擎不可用：${s?.detail ?? "未检测到"}`);
    return;
  }
  await agent.setEngine("pi");
  toast.ok("已切换到 pi 引擎（sidecar 常驻，首次对话约 1~2 秒启动）");
}

async function newChat() {
  await agent.newSession(props.panel);
  showSessions.value = false;
}

async function switchSession(id: string) {
  agent.selectSession(props.panel, id);
  await agent.refreshContext(id);
}

async function removeSession(id: string) {
  await agent.deleteSession(id);
  await agent.refreshContext();
}
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

        <div class="row" style="gap: 4px; flex: 0 0 auto">
          <!-- pi 思考强度 -->
          <div v-if="agent.engine === 'pi'" style="width: 104px">
            <UiSelect
              :model-value="agent.piThinking"
              :options="THINKING_OPTS"
              @update:model-value="setThinking"
            />
          </div>
          <!-- 上下文水位 -->
          <UiPopover :width="320" placement="bottom-end">
            <template #trigger>
              <button class="meter" :class="ctxTone" title="上下文水位">
                <Gauge :size="11" />
                <span class="num">{{ ctxPct }}%</span>
                <span class="mtrack"><span class="mfill" :style="{ width: ctxPct + '%' }" /></span>
              </button>
            </template>
            <div class="col" style="gap: 9px">
              <div class="row-between">
                <span class="section-label">对话上下文</span>
                <span class="t-xs faint num">
                  {{ ((stats?.messagesTokens ?? 0) / 1000).toFixed(1) }}k /
                  {{ ((stats?.budget ?? 0) / 1000).toFixed(0) }}k token
                </span>
              </div>
              <div class="kv">
                <span class="faint">其中工具结果</span>
                <span class="num">{{ ((stats?.toolResultTokens ?? 0) / 1000).toFixed(1) }}k</span>
                <span class="faint">冻结前缀</span>
                <span class="num">{{ ((stats?.prefixTokens ?? 0) / 1000).toFixed(1) }}k（可缓存）</span>
                <span class="faint">轮次 / 消息</span>
                <span class="num">{{ stats?.turns ?? 0 }} / {{ stats?.messages ?? 0 }}</span>
              </div>
              <div class="row" style="gap: 5px">
                <UiButton variant="outline" size="xs" :loading="agent.compacting" @click="agent.compact(false)">
                  压缩
                </UiButton>
                <UiButton variant="subtle" size="xs" :loading="agent.compacting" @click="agent.compact(true)">
                  总结并压缩
                </UiButton>
              </div>
              <p class="t-xs faint" style="line-height: 1.75">
                「压缩」先清掉早期工具结果的原文（免费）；还超预算才调模型做摘要。
                冻结前缀不受影响，那块缓存不会被压掉。
              </p>
            </div>
          </UiPopover>

          <UiBadge v-if="cacheRate > 0" :tone="cacheRate >= 50 ? 'ok' : 'neutral'" size="xs">
            <Zap :size="9" /> {{ cacheRate }}%
          </UiBadge>

          <!-- 对话列表（自研引擎的会话；pi 引擎的会话由 sidecar 自己管理） -->
          <UiPopover v-if="agent.engine !== 'pi'" :width="290" placement="bottom-end">
            <template #trigger>
              <UiButton variant="subtle" size="xs" icon title="对话列表">
                <template #icon><ChevronDown :size="13" /></template>
              </UiButton>
            </template>
            <div class="col" style="gap: 6px">
              <div class="row-between">
                <span class="section-label">对话</span>
                <UiButton variant="outline" size="xs" @click="newChat">
                  <template #icon><Plus :size="11" /></template>
                  新建
                </UiButton>
              </div>
              <div class="sessions">
                <div
                  v-for="s in sessions"
                  :key="s.id"
                  class="srow"
                  :class="{ on: s.id === agent.activeSessionId }"
                  @click="switchSession(s.id)"
                >
                  <div class="col grow" style="gap: 1px; min-width: 0">
                    <span class="t-sm truncate">{{ s.title || "未命名对话" }}</span>
                    <span class="t-xs faint num">
                      {{ s.turns }} 轮 · {{ (s.updatedAt ?? "").slice(5, 16).replace("T", " ") }}
                    </span>
                  </div>
                  <button class="sdel" @click.stop="removeSession(s.id)">
                    <Trash2 :size="11" />
                  </button>
                </div>
              </div>
              <p class="t-xs faint" style="line-height: 1.7">
                换新对话就是换一个干净的上下文窗口。旧对话还在，随时切回来。
              </p>
            </div>
          </UiPopover>

          <UiTooltip
            v-if="agent.engine !== 'pi'"
            placement="bottom"
            content="让 Simon 重新读取项目状态（会牺牲一次缓存命中）"
          >
            <UiButton variant="subtle" size="xs" icon :disabled="agent.running" @click="refreshContext">
              <template #icon><RefreshCw :size="13" /></template>
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

            <div
              v-for="t in m.toolCalls"
              :key="t.id"
              class="tool clickable"
              :title="openTools[t.id] ? '点击收起' : '点击展开完整结果'"
              @click="toggleTool(t.id)"
            >
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
              <div v-if="openTools[t.id]" class="tool-full">
                <pre>{{ toolDataText(t) }}</pre>
              </div>
            </div>

            <div v-if="m.text" class="bubble-bot"><Markdown :text="m.text" /></div>
          </div>
        </template>

        <!-- 流式中 -->
        <div v-if="agent.running || agent.streamText || agent.streamTools.length" class="msg-bot">
          <pre v-if="agent.streamReasoning" class="reason">{{ agent.streamReasoning }}</pre>

          <div
            v-for="t in agent.streamTools"
            :key="t.id"
            class="tool clickable"
            :title="openTools[t.id] ? '点击收起' : '点击展开完整结果'"
            @click="toggleTool(t.id)"
          >
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
            <div v-if="openTools[t.id]" class="tool-full">
              <pre>{{ toolDataText(t) }}</pre>
            </div>

            <div v-if="t.state === 'awaiting-approval'" class="approve">
              <AlertTriangle :size="13" />
              <span class="t-xs grow">
                {{ t.costly ? "这个操作会消耗生成额度，确认执行？" : "这一步会修改项目数据，确认执行？" }}
              </span>
              <UiButton size="xs" variant="primary" @click.stop="approve(t.id, true)">确认</UiButton>
              <UiButton size="xs" variant="ghost" @click.stop="approve(t.id, false)">拒绝</UiButton>
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

        <!-- @ 引用候选 -->
      <Transition name="float">
        <div v-if="mentionOpen && mentionItems.length" class="mentions">
          <div class="mhead t-xs faint">引用内容 —— 选中后会随这条消息一起带给 Simon</div>
          <button
            v-for="(m, i) in mentionItems"
            :key="m.token"
            class="mrow"
            :class="{ on: i === mentionIndex }"
            @mousedown.prevent="pickMention(m)"
          >
            <component :is="KIND_ICON[m.kind]" :size="12" class="faint" />
            <span class="t-sm truncate" style="max-width: 140px">{{ m.label }}</span>
            <span class="t-xs faint truncate grow">{{ m.hint }}</span>
          </button>
        </div>
      </Transition>

      <div class="composer">
          <UiTextarea
            v-model="input"
            :rows="1"
            :resize="false"
            :placeholder="agent.running ? '正在生成…' : '让 Simon 做什么？输入 @ 引用技能 / 章节 / 资产'"
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
              <UiButton
                variant="subtle"
                size="xs"
                :title="engineHint"
                :disabled="agent.running"
                @click="toggleEngine"
              >
                {{ agent.engine === "pi" ? "pi 引擎" : "自研引擎" }}
              </UiButton>
              <div v-if="llmOptions.length" style="width: 148px">
                <UiSelect
                  :model-value="settings.settings?.activeLlmProviderId ?? null"
                  :options="llmOptions"
                  placeholder="选择模型"
                  @update:model-value="switchModel"
                />
              </div>
              <span
                v-else
                class="t-xs faint"
                title="在「设置 → 模型供应商」里配置文本模型"
              >
                未配置模型
              </span>
              <span class="t-xs faint">Ctrl+Enter 发送</span>
            </div>
            <button
              v-if="agent.running"
              class="send stop"
              title="停止：正在执行的这一步会先收尾，之后不再继续"
              @click="agent.stop()"
            >
              <Square :size="13" />
            </button>
            <button v-else class="send" :disabled="!input.trim() && !attachments.length" @click="send">
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
/* 工具卡片可点击展开完整结果 */
.tool.clickable {
  cursor: pointer;
}
.tool.clickable:hover {
  border-color: var(--line-strong);
}
.tool-full pre {
  margin: 4px 0 0;
  padding: 8px 9px;
  max-height: 320px;
  overflow: auto;
  background: var(--surface-2);
  border: 1px solid var(--line-faint);
  border-radius: var(--r-sm);
  color: var(--fg-dim);
  font-size: 11px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-all;
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

/* 上下文水位 */
.meter {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 30px;
  padding: 0 9px;
  border-radius: var(--r);
  background: var(--surface-3);
  border: 1px solid var(--line);
  color: var(--fg-dim);
  cursor: pointer;
  font-size: var(--t-sm);
}
.meter:hover {
  border-color: var(--line-strong);
  color: var(--fg);
}
.mtrack {
  display: block;
  width: 26px;
  height: 3px;
  border-radius: var(--r-full);
  background: var(--surface-5);
  overflow: hidden;
}
.mfill {
  display: block;
  height: 100%;
  background: var(--ok);
  transition: width 240ms var(--ease);
}
.meter.warn {
  color: var(--warn);
}
.meter.warn .mfill {
  background: var(--warn);
}
.meter.err {
  color: var(--err);
}
.meter.err .mfill {
  background: var(--err);
}
.kv {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 4px 12px;
  font-size: var(--t-sm);
}

/* 对话列表 */
.sessions {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 240px;
  overflow: auto;
}
.srow {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  border-radius: var(--r-sm);
  border: 1px solid transparent;
  cursor: pointer;
}
.srow:hover {
  background: var(--surface-3);
}
.srow.on {
  background: var(--accent-soft);
  border-color: var(--accent-line);
}
.sdel {
  background: none;
  border: none;
  color: var(--fg-ghost);
  cursor: pointer;
  padding: 2px;
  border-radius: var(--r-xs);
  opacity: 0;
}
.srow:hover .sdel {
  opacity: 1;
}
.sdel:hover {
  color: var(--err);
  background: var(--surface-4);
}

/* @ 引用候选 */
.mentions {
  margin-bottom: 7px;
  background: var(--surface-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-lg);
  box-shadow: var(--shadow-lg);
  padding: 6px;
  max-height: 230px;
  overflow: auto;
}
.mhead {
  padding: 3px 6px 6px;
}
.mrow {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  padding: 6px 8px;
  border: none;
  border-radius: var(--r-sm);
  background: none;
  color: var(--fg-dim);
  cursor: pointer;
  text-align: left;
}
.mrow:hover,
.mrow.on {
  background: var(--surface-4);
  color: var(--fg);
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
/* 停止按钮：生成中替代发送键 */
.send.stop {
  background: var(--err);
  color: #fff;
}
.send.stop:hover:not(:disabled) {
  background: var(--err);
  filter: brightness(1.12);
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
