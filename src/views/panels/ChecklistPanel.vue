<script setup lang="ts">
import { computed, ref } from "vue";
import { CheckCircle2, Circle, ListChecks, Plus, RotateCcw, Trash2 } from "@lucide/vue";
import { useProjectStore } from "@/stores/project";
import { useAgentStore } from "@/stores/agent";
import { api, errorText } from "@/api/ipc";
import { toast } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiInput from "@/ui/Input.vue";
import UiSelect from "@/ui/Select.vue";
import UiProgress from "@/ui/Progress.vue";
import UiCheckbox from "@/ui/Checkbox.vue";
import UiEmpty from "@/ui/Empty.vue";
import { PANELS, type PanelId } from "@/types/models";

const project = useProjectStore();
const agent = useAgentStore();

const newText = ref("");
const newPanel = ref<PanelId>("script");
const onlyPending = ref(false);

const items = computed(() => project.checklist.items);
const progress = computed(() => project.progress);

const overall = computed(() => {
  const list = progress.value;
  if (!list.length) return 0;
  return Math.round(list.reduce((a, x) => a + x.percent, 0) / list.length);
});

const groups = computed(() =>
  PANELS.map((p) => ({
    panel: p,
    prog: progress.value.find((x) => x.panel === p.id),
    items: items.value.filter((i) => i.panel === p.id && (!onlyPending.value || !i.done)),
  })).filter((g) => g.items.length || g.prog),
);

const blockers = computed(() =>
  progress.value.flatMap((p) => p.blockers.map((b) => ({ panel: p.panel, text: b }))),
);

const panelName = (id: PanelId) => PANELS.find((p) => p.id === id)?.title ?? id;

async function toggle(id: string, done: boolean, auto: boolean) {
  if (auto) return toast.info("自动项由数据决定，把对应面板的活干完它自己就会亮");
  try {
    await api.checklistToggle(id, done);
    await project.reload();
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function add() {
  const t = newText.value.trim();
  if (!t) return;
  await api.checklistAdd(newPanel.value, t);
  newText.value = "";
  await project.reload();
}

async function remove(id: string) {
  await api.checklistDelete(id);
  await project.reload();
}

async function reset() {
  await api.checklistResetDefaults();
  await project.reload();
  toast.ok("已恢复默认检查项");
}
</script>

<template>
  <div class="layout">
    <!-- 左：总览 -->
    <section class="panel sum-panel">
      <header class="panel-head">
        <span class="section-label">全流程完成度</span>
        <span class="big num">{{ overall }}%</span>
      </header>

      <div class="sum scroll">
        <UiProgress :value="overall" :height="6" />

        <div class="panels">
          <div v-for="p in progress" :key="p.panel" class="pcell">
            <div class="row-between">
              <span class="t-sm">{{ panelName(p.panel) }}</span>
              <span class="t-xs num" :style="{ color: p.percent >= 99.9 ? 'var(--ok)' : 'var(--fg-faint)' }">
                {{ Math.round(p.percent) }}%
              </span>
            </div>
            <UiProgress :value="p.percent" :height="3" :tone="p.percent >= 99.9 ? 'ok' : 'info'" />
          </div>
        </div>

        <div v-if="blockers.length" class="blockers">
          <span class="section-label">还差什么</span>
          <div v-for="(b, i) in blockers" :key="i" class="blocker">
            <UiBadge tone="neutral" size="xs">{{ panelName(b.panel) }}</UiBadge>
            <span class="t-xs dim">{{ b.text }}</span>
          </div>
        </div>
        <div v-else class="alldone">
          <CheckCircle2 :size="15" />
          <span class="t-sm">全部完成</span>
        </div>
      </div>
    </section>

    <!-- 右：检查项 -->
    <section class="panel list-panel">
      <header class="panel-head">
        <div class="row" style="gap: 6px">
          <span class="section-label">检查清单</span>
          <UiBadge tone="neutral" size="xs">
            {{ items.filter((i) => i.done).length }}/{{ items.length }}
          </UiBadge>
        </div>
        <div class="row" style="gap: 4px">
          <UiButton variant="subtle" size="xs" @click="onlyPending = !onlyPending">
            {{ onlyPending ? "看全部" : "只看未完成" }}
          </UiButton>
          <UiButton variant="subtle" size="xs" @click="reset">
            <template #icon><RotateCcw :size="12" /></template>
            恢复默认
          </UiButton>
        </div>
      </header>

      <div class="addbar">
        <UiSelect v-model="newPanel" :options="PANELS.map((p) => ({ label: p.title, value: p.id }))" style="width: 104px" />
        <UiInput v-model="newText" placeholder="加一条自己的检查项" @enter="add" />
        <UiButton variant="default" size="sm" icon @click="add">
          <template #icon><Plus :size="13" /></template>
        </UiButton>
      </div>

      <div class="list scroll">
        <div v-for="g in groups" :key="g.panel.id" class="group">
          <div class="row-between ghead">
            <span class="t-sm" style="font-weight: 600">
              {{ g.panel.index }}. {{ g.panel.title }}
            </span>
            <span class="t-xs faint">{{ g.panel.subtitle }}</span>
          </div>

          <div v-for="it in g.items" :key="it.id" class="rowitem">
            <UiCheckbox
              :model-value="it.done"
              :disabled="it.auto"
              @update:model-value="(v: boolean) => toggle(it.id, v, it.auto)"
            />
            <div class="col grow" style="gap: 1px; min-width: 0">
              <span class="t-sm" :class="{ done: it.done }">{{ it.text }}</span>
              <span v-if="it.note" class="t-xs faint truncate">{{ it.note }}</span>
            </div>
            <UiBadge v-if="it.auto" tone="neutral" size="xs">自动</UiBadge>
            <UiBadge v-else-if="it.doneBy === 'agent'" tone="accent" size="xs">agent</UiBadge>
            <UiButton v-if="!it.auto" variant="subtle" size="xs" icon @click="remove(it.id)">
              <template #icon><Trash2 :size="11" /></template>
            </UiButton>
          </div>

          <div v-if="!g.items.length" class="t-xs faint" style="padding: 2px 10px 6px">
            没有{{ onlyPending ? "未完成" : "" }}条目
          </div>
        </div>

        <UiEmpty v-if="!groups.length" compact title="清单是空的" hint="点「恢复默认」套一份常用检查项">
          <template #icon><ListChecks :size="22" /></template>
        </UiEmpty>
      </div>
    </section>
  </div>
</template>

<style scoped>
.layout {
  display: flex;
  gap: var(--sp-3);
  height: 100%;
  min-height: 0;
}
.sum-panel {
  width: 320px;
  flex: 0 0 320px;
}
.list-panel {
  flex: 1;
  min-width: 0;
}
.big {
  font-size: var(--t-xl);
  font-weight: 650;
  color: var(--accent);
  line-height: 1;
}
.sum {
  flex: 1;
  min-height: 0;
  padding: var(--sp-4);
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
}
.panels {
  display: grid;
  grid-template-columns: 1fr;
  gap: 10px;
}
.pcell {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.blockers {
  display: flex;
  flex-direction: column;
  gap: 7px;
  padding-top: var(--sp-3);
  border-top: 1px solid var(--line-faint);
}
.blocker {
  display: flex;
  gap: 7px;
  align-items: flex-start;
}
.alldone {
  display: flex;
  align-items: center;
  gap: 7px;
  color: var(--ok);
  padding-top: var(--sp-3);
  border-top: 1px solid var(--line-faint);
}

.addbar {
  display: flex;
  gap: 6px;
  padding: var(--sp-3);
  border-bottom: 1px solid var(--line-faint);
}
.list {
  flex: 1;
  min-height: 0;
  padding: var(--sp-2) var(--sp-3);
}
.group {
  margin-bottom: var(--sp-4);
}
.ghead {
  padding: 4px 6px;
  border-bottom: 1px solid var(--line-faint);
  margin-bottom: 3px;
}
.rowitem {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 6px;
  border-radius: var(--r-sm);
}
.rowitem:hover {
  background: var(--surface-3);
}
.done {
  text-decoration: line-through;
  color: var(--fg-faint);
}
</style>
