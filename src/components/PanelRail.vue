<script setup lang="ts">
import { computed } from "vue";
import { useRouter } from "vue-router";
import { NTooltip } from "naive-ui";
import { PANELS, type PanelId } from "@/types/models";
import { useProjectStore } from "@/stores/project";

const props = defineProps<{ active: PanelId }>();
const router = useRouter();
const project = useProjectStore();

const rows = computed(() =>
  PANELS.map((p) => {
    const prog = project.progress.find((x) => x.panel === p.id);
    return {
      ...p,
      percent: prog?.percent ?? 0,
      blockers: prog?.blockers ?? [],
      done: (prog?.percent ?? 0) >= 99.9,
    };
  }),
);

function go(id: PanelId) {
  router.push({ name: "workspace", params: { panel: id } });
}
</script>

<template>
  <nav class="rail">
    <div class="rail-head">
      <div class="brand">
        <div class="logo">短</div>
        <div class="col" style="gap: 0">
          <div style="font-weight: 600">短剧工作台</div>
          <div class="tiny faint truncate" :title="project.root">
            {{ project.manifest?.name || "未打开项目" }}
          </div>
        </div>
      </div>
    </div>

    <div class="rail-list scroll">
      <n-tooltip
        v-for="row in rows"
        :key="row.id"
        trigger="hover"
        placement="right"
        :disabled="row.blockers.length === 0"
      >
        <template #trigger>
          <button class="item" :class="{ active: props.active === row.id }" @click="go(row.id)">
            <span class="idx mono">{{ row.index }}</span>
            <span class="col grow" style="gap: 0; align-items: flex-start">
              <span class="name">{{ row.title }}</span>
              <span class="tiny faint truncate" style="max-width: 100%">{{ row.subtitle }}</span>
            </span>
            <span class="pct" :class="{ done: row.done }">
              {{ row.done ? "✓" : Math.round(row.percent) + "%" }}
            </span>
          </button>
        </template>
        <div style="max-width: 320px">
          <div style="font-weight: 600; margin-bottom: 4px">还差什么</div>
          <div v-for="(b, i) in row.blockers" :key="i">· {{ b }}</div>
        </div>
      </n-tooltip>
    </div>

    <div class="rail-foot">
      <button class="ghost" @click="router.push({ name: 'home' })">← 项目列表</button>
    </div>
  </nav>
</template>

<style scoped>
.rail {
  width: 236px;
  flex: 0 0 236px;
  background: var(--bg-1);
  border-right: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.rail-head {
  padding: 14px 12px 10px;
  border-bottom: 1px solid var(--line-soft);
}
.brand {
  display: flex;
  gap: 9px;
  align-items: center;
}
.logo {
  width: 28px;
  height: 28px;
  border-radius: 7px;
  background: linear-gradient(140deg, #f0b040, #c97f24);
  color: #17171d;
  display: grid;
  place-items: center;
  font-weight: 700;
  flex: 0 0 auto;
}
.rail-list {
  flex: 1;
  padding: 6px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.item {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 7px 8px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
  text-align: left;
  font-size: 13px;
}
.item:hover {
  background: var(--bg-3);
  color: var(--text);
}
.item.active {
  background: var(--accent-soft);
  color: var(--text);
  box-shadow: inset 2px 0 0 var(--accent);
}
.idx {
  width: 18px;
  height: 18px;
  flex: 0 0 auto;
  border-radius: 4px;
  background: var(--bg-3);
  font-size: 10px;
  display: grid;
  place-items: center;
  color: var(--text-faint);
}
.item.active .idx {
  background: rgba(232, 163, 61, 0.25);
  color: var(--accent);
}
.name {
  font-weight: 500;
}
.pct {
  font-size: 10px;
  color: var(--text-faint);
  flex: 0 0 auto;
}
.pct.done {
  color: var(--ok);
}
.rail-foot {
  padding: 8px;
  border-top: 1px solid var(--line-soft);
}
.ghost {
  width: 100%;
  padding: 6px;
  background: transparent;
  border: 1px solid var(--line);
  border-radius: 6px;
  color: var(--text-dim);
  cursor: pointer;
}
.ghost:hover {
  color: var(--text);
  border-color: #3a3a47;
}
</style>
