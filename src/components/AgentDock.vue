<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { NButton, NInput, NSelect, NPopover, NTooltip, NSpin } from "naive-ui";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useAgentStore } from "@/stores/agent";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import { errorText } from "@/api/ipc";
import { fileUrl } from "@/api/events";
import { message } from "@/utils/notify";
import Markdown from "./Markdown.vue";
import type { AgentMessage, AgentToolCall } from "@/types/agent";
import type { PanelId } from "@/types/models";

const props = defineProps<{ panel: PanelId }>();
const agent = useAgentStore();
const project = useProjectStore();
const settings = useSettingsStore();

const input = ref("");
const attachments = ref<string[]>([]);
const scroller = ref<HTMLElement | null>(null);
const autoScroll = ref(true);

const session = computed(() => agent.sessionOf(props.panel));
const messages = computed<AgentMessage[]>(() => session.value?.messages ?? []);
const sessionOptions = computed(() =>
  agent.sessionsOf(props.panel).map((s) => ({ label: s.title || "未命名对话", value: s.id })),
);
const modelLabel = computed(() => {
  const p = settings.activeLlm;
  return p ? `${p.name} · ${p.model}` : "占位模型（未配置）";
});

watch(
  () => [messages.value.length, agent.streamText, agent.streamTools.length],
  async () => {
    if (!autoScroll.value) return;
    await nextTick();
    const el = scroller.value;
    if (el) el.scrollTop = el.scrollHeight;
  },
);

function onScroll() {
  const el = scroller.value;
  if (!el) return;
  autoScroll.value = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
}

async function attachImages() {
  const picked = await openDialog({
    multiple: true,
    filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp"] }],
  });
  const list = Array.isArray(picked) ? picked : picked ? [picked] : [];
  attachments.value = [...attachments.value, ...list];
}

async function send() {
  const text = input.value;
  if (!text.trim() && attachments.value.length === 0) return;
  input.value = "";
  const imgs = attachments.value;
  attachments.value = [];
  await agent.send(props.panel, text, imgs);
}

async function refreshContext() {
  input.value = input.value || "（请重新读取当前项目状态）";
  await agent.send(props.panel, input.value, [], true);
  input.value = "";
}

function toolStateTag(t: AgentToolCall) {
  return (
    {
      pending: "等待",
      "awaiting-approval": "待确认",
      running: "执行中",
      ok: "完成",
      failed: "失败",
      rejected: "已拒绝",
    } as Record<string, string>
  )[t.state];
}

function toolStateClass(t: AgentToolCall) {
  if (t.state === "ok") return "ok";
  if (t.state === "failed" || t.state === "rejected") return "err";
  if (t.state === "awaiting-approval") return "warn";
  return "";
}

function prettyInput(v: unknown): string {
  try {
    const s = JSON.stringify(v, null, 2);
    return s.length > 1200 ? s.slice(0, 1200) + "\n…" : s;
  } catch {
    return String(v);
  }
}

async function doApprove(id: string, ok: boolean) {
  try {
    await agent.approve(id, ok);
  } catch (e) {
    message.error(errorText(e));
  }
}
</script>

<template>
  <aside class="dock">
    <div class="dock-head">
      <div class="row-between">
        <div class="row" style="gap: 6px; min-width: 0">
          <span style="font-weight: 600">协作 Agent</span>
          <span class="tag" :class="{ accent: agent.running }">
            {{ agent.running ? "运行中" : "待命" }}
          </span>
        </div>
        <div class="row" style="gap: 2px">
          <n-select
            v-if="sessionOptions.length > 1"
            size="tiny"
            style="width: 116px"
            :value="session?.id"
            :options="sessionOptions"
            @update:value="(v: string) => agent.selectSession(props.panel, v)"
          />
          <n-tooltip><template #trigger>
            <n-button size="tiny" quaternary @click="agent.newSession(props.panel)">＋</n-button>
          </template>新对话</n-tooltip>
          <n-tooltip><template #trigger>
            <n-button size="tiny" quaternary @click="refreshContext">↻</n-button>
          </template>重建上下文（会牺牲一次缓存命中，但能让 agent 看到最新数据）</n-tooltip>
        </div>
      </div>
      <div class="row-between" style="margin-top: 4px">
        <span class="tiny faint truncate">{{ modelLabel }}</span>
        <n-popover trigger="hover" placement="bottom-end">
          <template #trigger>
            <span class="tiny" :style="{ color: (agent.usage?.hitRate ?? 0) > 0.5 ? 'var(--ok)' : 'var(--text-faint)' }">
              缓存 {{ ((agent.usage?.hitRate ?? 0) * 100).toFixed(0) }}%
            </span>
          </template>
          <div style="font-size: 12px; max-width: 320px; line-height: 1.7">
            <div v-if="agent.prefix">
              <div>前缀指纹：<span class="mono">{{ agent.prefix.fingerprint }}</span></div>
              <div>{{ agent.prefix.note }}</div>
              <div v-for="l in agent.prefix.layers" :key="l.name" class="row-between">
                <span class="truncate" style="max-width: 180px">{{ l.name }}</span>
                <span class="faint mono tiny">~{{ l.estTokens }} tok</span>
              </div>
            </div>
            <div v-if="agent.usage" style="margin-top: 6px">
              本轮：输入 {{ agent.usage.inputTokens }}，命中 {{ agent.usage.cacheReadTokens }}，
              写入 {{ agent.usage.cacheWriteTokens }}，输出 {{ agent.usage.outputTokens }}
            </div>
          </div>
        </n-popover>
      </div>
    </div>

    <div ref="scroller" class="dock-body scroll" @scroll="onScroll">
      <div v-if="!messages.length && !agent.streamText && !agent.streamTools.length" class="hint">
        <p>这个面板的 agent 可以直接改数据：写完分镜、建资产、写提示词、提交生成、铺时间线。</p>
        <p class="faint tiny">
          系统提示里带着一份「项目圣经 + 资产索引」的冻结快照，整场对话不会变，
          所以每轮基本都能命中服务端前缀缓存。需要最新数据时让 agent 用工具去读。
        </p>
      </div>

      <div v-for="m in messages" :key="m.id" class="msg" :class="m.role">
        <div class="bubble">
          <div v-if="m.reasoning" class="reasoning">
            <details>
              <summary class="tiny faint">推理过程</summary>
              <pre class="tiny">{{ m.reasoning }}</pre>
            </details>
          </div>
          <Markdown v-if="m.text" :text="m.text" />
          <div v-if="m.images?.length" class="row wrap" style="margin-top: 6px">
            <img v-for="(img, i) in m.images" :key="i" :src="fileUrl(img)" class="attach" />
          </div>
        </div>
        <div v-for="t in m.toolCalls" :key="t.id" class="tool">
          <div class="row-between">
            <div class="row" style="gap: 6px; min-width: 0">
              <span class="tag" :class="toolStateClass(t)">{{ toolStateTag(t) }}</span>
              <span class="small truncate">{{ t.title }}</span>
              <span class="tiny faint mono">{{ t.name }}</span>
            </div>
            <span class="tiny faint">{{ t.durationMs }}ms</span>
          </div>
          <div v-if="t.summary" class="tiny" style="margin-top: 3px">{{ t.summary }}</div>
          <details v-if="t.input && Object.keys(t.input as object).length" style="margin-top: 3px">
            <summary class="tiny faint">参数</summary>
            <pre class="tiny">{{ prettyInput(t.input) }}</pre>
          </details>
        </div>
      </div>

      <!-- 流式中的这一轮 -->
      <div v-if="agent.running || agent.streamText || agent.streamTools.length" class="msg assistant">
        <div v-if="agent.streamReasoning" class="reasoning">
          <details open>
            <summary class="tiny faint">推理过程</summary>
            <pre class="tiny">{{ agent.streamReasoning }}</pre>
          </details>
        </div>
        <div v-if="agent.streamText" class="bubble">
          <Markdown :text="agent.streamText" streaming />
        </div>
        <div v-for="t in agent.streamTools" :key="t.id" class="tool">
          <div class="row-between">
            <div class="row" style="gap: 6px">
              <span class="tag" :class="toolStateClass(t)">{{ toolStateTag(t) }}</span>
              <span class="small">{{ t.title }}</span>
              <span class="tiny faint mono">{{ t.name }}</span>
            </div>
          </div>
          <div v-if="t.summary" class="tiny" style="margin-top: 3px">{{ t.summary }}</div>
          <div v-if="t.state === 'awaiting-approval'" class="approve">
            <span class="tiny warn">这个操作会消耗生成额度，确认执行？</span>
            <n-button size="tiny" type="primary" @click="doApprove(t.id, true)">确认</n-button>
            <n-button size="tiny" quaternary @click="doApprove(t.id, false)">拒绝</n-button>
          </div>
        </div>
        <div v-if="agent.running && !agent.streamText" class="tiny faint row" style="gap: 6px">
          <n-spin :size="12" /> 思考中…
        </div>
      </div>
    </div>

    <div class="dock-foot">
      <div v-if="attachments.length" class="row wrap" style="margin-bottom: 6px">
        <div v-for="(a, i) in attachments" :key="i" class="chip">
          <img :src="fileUrl(a)" />
          <span class="tiny truncate" style="max-width: 90px">{{ a.split(/[\\/]/).pop() }}</span>
          <button class="x" @click="attachments.splice(i, 1)">×</button>
        </div>
      </div>
      <n-input
        v-model:value="input"
        type="textarea"
        :autosize="{ minRows: 2, maxRows: 7 }"
        placeholder="让 agent 做什么？（Ctrl+Enter 发送）"
        :disabled="agent.running"
        @keydown.ctrl.enter.prevent="send"
        @keydown.meta.enter.prevent="send"
      />
      <div class="row-between" style="margin-top: 6px">
        <div class="row" style="gap: 4px">
          <n-button size="tiny" quaternary @click="attachImages">附图</n-button>
          <span class="tiny faint">可贴参考图让 agent 看图</span>
        </div>
        <n-button size="tiny" type="primary" :loading="agent.running" @click="send">发送</n-button>
      </div>
      <div v-if="agent.lastError" class="tiny err" style="margin-top: 4px">
        {{ agent.lastError }}
      </div>
    </div>
  </aside>
</template>

<style scoped>
.dock {
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--bg-1);
  border-left: 1px solid var(--line);
  height: 100%;
}
.dock-head {
  padding: 8px 10px;
  border-bottom: 1px solid var(--line-soft);
  flex: 0 0 auto;
}
.dock-body {
  flex: 1;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 0;
}
.dock-foot {
  flex: 0 0 auto;
  padding: 8px 10px 10px;
  border-top: 1px solid var(--line-soft);
}
.hint {
  color: var(--text-dim);
  font-size: 12px;
  line-height: 1.8;
  padding: 4px 2px;
}
.msg {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.msg.user {
  align-items: flex-end;
}
.bubble {
  background: var(--bg-2);
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 8px 10px;
  max-width: 100%;
}
.msg.user .bubble {
  background: rgba(232, 163, 61, 0.1);
  border-color: rgba(232, 163, 61, 0.25);
}
.tool {
  background: #14141a;
  border: 1px solid var(--line-soft);
  border-left: 2px solid #33333f;
  border-radius: 6px;
  padding: 6px 8px;
}
.approve {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 6px;
  padding-top: 6px;
  border-top: 1px dashed var(--line);
}
.reasoning pre {
  white-space: pre-wrap;
  margin: 4px 0 0;
  color: var(--text-faint);
  max-height: 160px;
  overflow: auto;
}
pre.tiny {
  background: #101016;
  border-radius: 4px;
  padding: 6px;
  overflow: auto;
  max-height: 200px;
}
.attach {
  max-width: 120px;
  max-height: 90px;
  border-radius: 5px;
  border: 1px solid var(--line);
}
.chip {
  display: flex;
  align-items: center;
  gap: 5px;
  background: var(--bg-3);
  border-radius: 5px;
  padding: 2px 5px 2px 2px;
}
.chip img {
  width: 26px;
  height: 26px;
  object-fit: cover;
  border-radius: 3px;
}
.chip .x {
  background: none;
  border: none;
  color: var(--text-faint);
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
}
.warn {
  color: var(--warn);
}
.err {
  color: var(--err);
}
</style>
