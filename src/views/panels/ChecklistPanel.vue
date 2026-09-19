<script setup lang="ts">
import { computed, ref } from "vue";
import { NButton, NInput, NSelect, NProgress, NPopover, NCheckbox } from "naive-ui";
import { useProjectStore } from "@/stores/project";
import { PANELS, type PanelId } from "@/types/models";
import { message } from "@/utils/notify";
import { api, errorText } from "@/api/ipc";

const project = useProjectStore();

const newText = ref("");
const newPanel = ref<PanelId>("script");
const onlyPending = ref(false);

const items = computed(() => project.checklist.items);
const grouped = computed(() =>
  PANELS.map((p) => ({
    panel: p,
    progress: project.progress.find((x) => x.panel === p.id),
    items: items.value.filter((i) => i.panel === p.id && (!onlyPending.value || !i.done)),
  })).filter((g) => g.items.length || g.progress),
);

const overall = computed(() => {
  const list = project.progress;
  if (!list.length) return 0;
  return list.reduce((a, x) => a + x.percent, 0) / list.length;
});

const totalBlockers = computed(() =>
  project.progress.flatMap((p) => p.blockers.map((b) => ({ panel: p.panel, text: b }))),
);

async function toggle(id: string, done: boolean, auto: boolean) {
  if (auto) {
    message.info("自动项由数据决定，把对应面板的工作做完它自己就会亮");
    return;
  }
  try {
    await api.checklistToggle(id, done);
    await project.reload();
  } catch (e) {
    message.error(errorText(e));
  }
}

async function addItem() {
  const t = newText.value.trim();
  if (!t) return;
  try {
    await api.checklistAdd(newPanel.value, t);
    newText.value = "";
    await project.reload();
  } catch (e) {
    message.error(errorText(e));
  }
}

async function removeItem(id: string) {
  await api.checklistDelete(id);
  await project.reload();
}

async function resetDefaults() {
  await api.checklistResetDefaults();
  await project.reload();
  message.success("已恢复默认检查项");
}

function panelName(id: PanelId) {
  return PANELS.find((p) => p.id === id)?.title ?? id;
}
</script>

<template>
  <div class="wrap">
    <div class="pane summary">
      <div class="row-between">
        <div class="col" style="gap: 2px">
          <span style="font-weight: 600">全流程完成度</span>
          <span class="tiny faint">自动项由数据实时判定，改完工作立刻反映在这里</span>
        </div>
        <div class="big-num">{{ Math.round(overall) }}%</div>
      </div>
      <n-progress
        type="line" :percentage="Math.round(overall)" :height="6" :show-indicator="false"
        color="#e8a33d"
      />

      <div class="panels">
        <div v-for="p in project.progress" :key="p.panel" class="pcell">
          <div class="row-between">
            <span class="small">{{ panelName(p.panel) }}</span>
            <span class="tiny" :style="{ color: p.percent >= 99.9 ? 'var(--ok)' : 'var(--text-faint)' }">
              {{ Math.round(p.percent) }}%
            </span>
          </div>
          <n-progress
            type="line" :percentage="Math.round(p.percent)" :height="4" :show-indicator="false"
            :color="p.percent >= 99.9 ? '#4ea87a' : '#5b8dd9'"
          />
          <div v-if="p.blockers.length" class="tiny faint truncate" :title="p.blockers.join('；')">
            {{ p.blockers[0] }}
          </div>
        </div>
      </div>

      <div v-if="totalBlockers.length" class="blockers">
        <div style="font-weight: 600; margin-bottom: 4px">还差什么</div>
        <div v-for="(b, i) in totalBlockers" :key="i" class="row" style="gap: 6px; align-items: flex-start">
          <span class="tag" style="flex: 0 0 auto">{{ panelName(b.panel) }}</span>
          <span class="tiny dim">{{ b.text }}</span>
        </div>
      </div>
    </div>

    <div class="pane col-list">
      <div class="head col" style="gap: 6px">
        <div class="row-between">
          <span style="font-weight: 600">检查清单</span>
          <div class="row" style="gap: 4px">
            <n-button size="tiny" quaternary @click="onlyPending = !onlyPending">
              {{ onlyPending ? "看全部" : "只看未完成" }}
            </n-button>
            <n-button size="tiny" quaternary @click="resetDefaults">恢复默认</n-button>
          </div>
        </div>
        <div class="row" style="gap: 4px">
          <n-select
            v-model:value="newPanel" size="tiny" style="width: 108px"
            :options="PANELS.map((p) => ({ label: p.title, value: p.id }))"
          />
          <n-input
            v-model:value="newText" size="tiny" placeholder="加一条自己的检查项"
            @keydown.enter="addItem"
          />
          <n-button size="tiny" @click="addItem">加</n-button>
        </div>
      </div>

      <div class="scroll list">
        <div v-for="g in grouped" :key="g.panel.id" class="group">
          <div class="row-between ghead">
            <span class="small" style="font-weight: 600">
              {{ g.panel.index }}. {{ g.panel.title }}
            </span>
            <span class="tiny faint">{{ g.panel.subtitle }}</span>
          </div>
          <div v-for="it in g.items" :key="it.id" class="row item">
            <n-checkbox
              :checked="it.done"
              :disabled="it.auto"
              size="small"
              @update:checked="(v: boolean) => toggle(it.id, v, it.auto)"
            />
            <div class="col grow" style="gap: 0; min-width: 0">
              <span class="small" :class="{ done: it.done }">{{ it.text }}</span>
              <span v-if="it.note" class="tiny faint truncate">{{ it.note }}</span>
            </div>
            <span v-if="it.auto" class="tag">自动</span>
            <span v-else-if="it.doneBy === 'agent'" class="tag accent">agent</span>
            <button v-if="!it.auto" class="mini" @click="removeItem(it.id)">×</button>
          </div>
          <div v-if="!g.items.length" class="tiny faint" style="padding: 4px 8px">
            没有{{ onlyPending ? "未完成" : "" }}条目
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wrap {
  display: flex;
  gap: 12px;
  height: 100%;
  min-height: 0;
}
.summary {
  width: 380px;
  flex: 0 0 380px;
  padding: 14px;
  overflow: auto;
}
.big-num {
  font-size: 26px;
  font-weight: 700;
  color: var(--accent);
  line-height: 1;
}
.panels {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  margin-top: 14px;
}
.pcell {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.blockers {
  margin-top: 16px;
  padding-top: 12px;
  border-top: 1px solid var(--line-soft);
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.col-list {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.head {
  padding: 8px 10px;
  border-bottom: 1px solid var(--line-soft);
}
.list {
  flex: 1;
  min-height: 0;
  padding: 6px 8px;
}
.group {
  margin-bottom: 12px;
}
.ghead {
  padding: 4px 6px;
  border-bottom: 1px solid var(--line-soft);
  margin-bottom: 3px;
}
.item {
  padding: 5px 6px;
  border-radius: 5px;
}
.item:hover {
  background: var(--bg-3);
}
.done {
  text-decoration: line-through;
  color: var(--text-faint);
}
.mini {
  background: none;
  border: none;
  color: var(--text-faint);
  cursor: pointer;
  font-size: 13px;
}
.mini:hover {
  color: var(--err);
}
</style>
