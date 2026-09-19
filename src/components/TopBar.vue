<script setup lang="ts">
import { computed, ref } from "vue";
import { Activity, FolderOpen, Pencil, Power, Zap } from "@lucide/vue";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import { useAgentStore } from "@/stores/agent";
import { useJobsStore } from "@/stores/jobs";
import { api, errorText } from "@/api/ipc";
import { toast, confirmDialog } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiSelect from "@/ui/Select.vue";
import UiPopover from "@/ui/Popover.vue";
import UiInput from "@/ui/Input.vue";
import UiNumber from "@/ui/NumberInput.vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { PANELS, type PanelId } from "@/types/models";

const props = defineProps<{ panel: PanelId }>();
const emit = defineEmits<{ (e: "settings"): void }>();
const project = useProjectStore();
const settings = useSettingsStore();
const agent = useAgentStore();
const jobs = useJobsStore();

const meta = computed(() => PANELS.find((p) => p.id === props.panel)!);
const blockers = computed(() => project.blockersOf(props.panel));
const editing = ref(false);
const draft = ref({ name: "", genre: "", logline: "", episodeCountHint: 0 as number | null, aspectRatio: "9:16" });

const llmOptions = computed(() =>
  settings.byKind("llm").map((p) => ({ label: p.name, hint: p.model, value: p.id })),
);

const cacheRate = computed(() => Math.round((agent.usage?.hitRate ?? 0) * 100));
const runningJobs = computed(() => jobs.running.length);

function startEdit() {
  const m = project.manifest;
  if (!m) return;
  draft.value = {
    name: m.name, genre: m.genre, logline: m.logline,
    episodeCountHint: m.episodeCountHint, aspectRatio: m.aspectRatio,
  };
  editing.value = true;
}

async function saveEdit() {
  try {
    await api.projectUpdateManifest({ ...draft.value, episodeCountHint: draft.value.episodeCountHint ?? 0 });
    await project.reload();
    editing.value = false;
    toast.ok("项目信息已保存");
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function switchModel(id: string | null) {
  if (!id) return;
  await settings.save({ activeLlmProviderId: id });
  toast.info("已切换模型，提示词前缀会重建（缓存需重新写入一次）");
}

async function openFolder() {
  try {
    const paths = await api.projectPaths(project.root);
    await openPath(paths.root);
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function openOther() {
  const picked = await openDialog({ directory: true, multiple: false, title: "选择项目目录" });
  if (typeof picked === "string") {
    try {
      await project.open(picked);
      toast.ok("项目已打开");
    } catch (e) {
      toast.err(errorText(e));
    }
  }
}

async function closeProject() {
  const ok = await confirmDialog({
    title: "关闭项目",
    content: "数据都在磁盘上，不会丢失，之后可以从项目列表再打开。",
    positiveText: "关闭",
  });
  if (!ok) return;
  await project.close();
  location.hash = "#/";
}
</script>

<template>
  <header class="top">
    <!-- 左：当前步骤 -->
    <div class="row grow" style="gap: 10px; min-width: 0">
      <div class="step t-xs faint num">{{ meta.index }} / 9</div>
      <div class="col" style="gap: 1px; min-width: 0">
        <div class="row" style="gap: 7px">
          <span class="title">{{ meta.title }}</span>
          <UiBadge v-if="blockers.length" tone="warn" size="xs">{{ blockers.length }} 项待办</UiBadge>
        </div>
        <span class="t-xs faint truncate" style="max-width: 52vw" :title="blockers.join('；')">
          {{ blockers[0] ?? meta.subtitle }}
        </span>
      </div>
    </div>

    <!-- 右：全局控制 -->
    <div class="row" style="gap: 6px; flex: 0 0 auto">
      <!-- 缓存 -->
      <UiPopover :width="330" placement="bottom-end">
        <template #trigger>
          <button class="pill" :class="{ hot: cacheRate >= 50 }">
            <Zap :size="12" />
            <span class="num">{{ cacheRate }}%</span>
            <span class="faint" style="font-size: 10px">缓存</span>
          </button>
        </template>
        <div class="col" style="gap: 8px">
          <div class="row-between">
            <span class="section-label">前缀缓存</span>
            <UiBadge :tone="agent.prefix?.stable ? 'ok' : 'warn'" size="xs">
              {{ agent.prefix?.stable ? "稳定" : "待重建" }}
            </UiBadge>
          </div>
          <div v-if="agent.usage" class="kv">
            <span class="faint">本轮输入</span><span class="num">{{ agent.usage.inputTokens }}</span>
            <span class="faint">命中缓存</span><span class="num ok">{{ agent.usage.cacheReadTokens }}</span>
            <span class="faint">写入缓存</span><span class="num">{{ agent.usage.cacheWriteTokens }}</span>
            <span class="faint">输出</span><span class="num">{{ agent.usage.outputTokens }}</span>
          </div>
          <div v-if="agent.prefix" class="col" style="gap: 4px">
            <div v-for="l in agent.prefix.layers" :key="l.name" class="layer">
              <span class="truncate">{{ l.name }}</span>
              <span class="faint num t-xs">~{{ l.estTokens }} tok</span>
            </div>
          </div>
          <p class="t-xs faint" style="line-height: 1.7">
            前缀在会话开始时冻结，整场对话不变，所以连续对话基本都能命中服务端缓存。
          </p>
        </div>
      </UiPopover>

      <!-- 任务 -->
      <button class="pill" :class="{ hot: runningJobs > 0 }" @click="jobs.barOpen = !jobs.barOpen">
        <Activity :size="12" />
        <span class="num">{{ runningJobs }}</span>
      </button>

      <!-- 模型 -->
      <UiSelect
        v-if="llmOptions.length"
        :model-value="settings.settings?.activeLlmProviderId ?? null"
        :options="llmOptions"
        placeholder="选择模型"
        style="width: 170px"
        @update:model-value="switchModel"
      />
      <UiButton v-else variant="outline" size="sm" @click="emit('settings')">未配置模型</UiButton>

      <div class="sep" />

      <UiButton variant="subtle" size="sm" icon @click="startEdit">
        <template #icon><Pencil :size="14" /></template>
      </UiButton>
      <UiButton variant="subtle" size="sm" icon @click="openFolder">
        <template #icon><FolderOpen :size="14" /></template>
      </UiButton>
      <UiButton variant="subtle" size="sm" icon @click="openOther">
        <template #icon><Power :size="14" /></template>
      </UiButton>
    </div>

    <!-- 项目信息编辑 -->
    <Teleport to="body">
      <div v-if="editing" class="editmask" @click.self="editing = false">
        <div class="editbox fade-in">
          <div class="section-label">项目信息</div>
          <div class="grid2">
            <label class="fld"><span>项目名</span><UiInput v-model="draft.name" size="md" /></label>
            <label class="fld"><span>题材</span><UiInput v-model="draft.genre" size="md" /></label>
          </div>
          <label class="fld"><span>一句话卖点</span><UiInput v-model="draft.logline" size="md" /></label>
          <div class="grid2">
            <label class="fld"><span>计划章节数</span><UiNumber v-model="draft.episodeCountHint" size="md" :min="0" /></label>
            <label class="fld"><span>画幅</span><UiInput v-model="draft.aspectRatio" size="md" /></label>
          </div>
          <div class="row" style="justify-content: flex-end; margin-top: 4px">
            <UiButton variant="ghost" @click="editing = false">取消</UiButton>
            <UiButton variant="primary" @click="saveEdit">保存</UiButton>
          </div>
        </div>
      </div>
    </Teleport>
  </header>
</template>

<style scoped>
.top {
  height: var(--topbar-h);
  flex: 0 0 var(--topbar-h);
  display: flex;
  align-items: center;
  gap: var(--sp-3);
  padding: 0 var(--sp-3) 0 var(--sp-4);
  background: var(--surface-1);
  border-bottom: 1px solid var(--line);
}
.step {
  flex: 0 0 auto;
  padding: 2px 7px;
  border-radius: var(--r-full);
  background: var(--surface-3);
  border: 1px solid var(--line);
}
.title {
  font-size: var(--t-md);
  font-weight: 600;
}
.pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 30px;
  padding: 0 10px;
  border-radius: var(--r);
  background: var(--surface-3);
  border: 1px solid var(--line);
  color: var(--fg-dim);
  cursor: pointer;
  font-size: var(--t-sm);
  transition: border-color var(--fast), color var(--fast);
}
.pill:hover {
  border-color: var(--line-strong);
  color: var(--fg);
}
.pill.hot {
  color: var(--accent);
  border-color: var(--accent-line);
  background: var(--accent-soft);
}
.sep {
  width: 1px;
  height: 18px;
  background: var(--line);
  margin: 0 2px;
}
.kv {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 3px 12px;
  font-size: var(--t-sm);
}
.ok {
  color: var(--ok);
}
.layer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  font-size: var(--t-xs);
  color: var(--fg-dim);
}

.editmask {
  position: fixed;
  inset: 0;
  z-index: 240;
  background: rgba(0, 0, 0, 0.6);
  display: grid;
  place-items: center;
}
.editbox {
  width: 460px;
  background: var(--surface-1);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-xl);
  box-shadow: var(--shadow-lg);
  padding: var(--sp-5);
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--sp-3);
}
.fld {
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.fld > span {
  font-size: var(--t-sm);
  color: var(--fg-dim);
}
</style>
