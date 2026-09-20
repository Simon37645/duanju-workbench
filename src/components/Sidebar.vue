<script setup lang="ts">
import { computed } from "vue";
import { useRouter } from "vue-router";
import {
  Axis3d, BookText, Captions, Check, Clapperboard, FolderOpen, Images, LayoutGrid,
  Palette, Scissors, Settings, Sparkles, Sun, Moon,
} from "@lucide/vue";
import { PANELS, type PanelId } from "@/types/models";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import UiTooltip from "@/ui/Tooltip.vue";

const props = defineProps<{ active: PanelId }>();
const emit = defineEmits<{ (e: "settings"): void }>();
const router = useRouter();
const project = useProjectStore();
const settings = useSettingsStore();

const ICONS: Record<PanelId, unknown> = {
  script: BookText,
  style: Palette,
  storyboard: LayoutGrid,
  asset: Images,
  prompt: Sparkles,
  video: Clapperboard,
  edit: Scissors,
  subtitle: Captions,
  previz: Axis3d,
};

const rows = computed(() =>
  PANELS.map((p) => {
    const prog = project.progress.find((x) => x.panel === p.id);
    const percent = prog?.percent ?? 0;
    return {
      ...p,
      icon: ICONS[p.id],
      percent,
      done: percent >= 99.9,
      blockers: prog?.blockers ?? [],
    };
  }),
);

const overall = computed(() => {
  const list = project.progress;
  if (!list.length) return 0;
  return Math.round(list.reduce((a, x) => a + x.percent, 0) / list.length);
});

const isLight = computed(() => settings.settings?.theme === "light");
function toggleTheme() {
  settings.save({ theme: isLight.value ? "dark" : "light" });
}

function go(id: PanelId) {
  router.push({ name: "workspace", params: { panel: id } });
}
</script>

<template>
  <aside class="side">
    <!-- 品牌 + 项目 -->
    <div class="brand">
      <div class="mark">S</div>
      <div class="col grow" style="gap: 0; min-width: 0">
        <span class="bname truncate">Simon</span>
        <span class="t-xs faint truncate" :title="project.root">
          {{ project.manifest?.name ?? "未打开项目" }}
        </span>
      </div>
    </div>

    <div class="prog">
      <div class="row-between">
        <span class="t-xs faint">全流程</span>
        <span class="t-xs num" style="color: var(--accent)">{{ overall }}%</span>
      </div>
      <div class="track">
        <div class="fill" :style="{ width: overall + '%' }" />
      </div>
    </div>

    <!-- 流程导航 -->
    <nav class="nav scroll side-nav">
      <UiTooltip
        v-for="row in rows"
        :key="row.id"
        placement="right"
        :content="row.blockers.length ? `${row.title}：${row.blockers.join('；')}` : ''"
      >
        <button class="item" :class="{ on: props.active === row.id }" @click="go(row.id)">
          <component :is="row.icon" :size="14" class="ic" />
          <span class="lbl grow truncate">{{ row.title }}</span>
          <Check v-if="row.done" :size="11" class="done" />
          <span v-else-if="row.percent > 0" class="pct num">{{ Math.round(row.percent) }}</span>
          <span v-else class="dot" />
        </button>
      </UiTooltip>
    </nav>

    <!-- 底部 -->
    <div class="foot">
      <button class="item" @click="router.push({ name: 'home' })">
        <FolderOpen :size="14" class="ic" />
        <span class="lbl">项目列表</span>
      </button>
      <button class="item" @click="toggleTheme">
        <Sun v-if="isLight" :size="14" class="ic" />
        <Moon v-else :size="14" class="ic" />
        <span class="lbl">{{ isLight ? "亮色" : "暗色" }}</span>
      </button>
      <button class="item" @click="emit('settings')">
        <Settings :size="14" class="ic" />
        <span class="lbl">设置</span>
      </button>
    </div>
  </aside>
</template>

<style scoped>
.side {
  width: var(--sidebar-w);
  flex: 0 0 var(--sidebar-w);
  background: var(--surface-1);
  border-right: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 10px 8px;
}
.mark {
  width: 26px;
  height: 26px;
  flex: 0 0 auto;
  border-radius: 8px;
  background: linear-gradient(145deg, var(--accent), var(--accent-press));
  color: var(--accent-fg);
  display: grid;
  place-items: center;
  font-weight: 700;
  font-size: 13px;
  box-shadow: var(--shadow-sm);
}
.bname {
  font-size: var(--t-base);
  font-weight: 650;
  letter-spacing: -0.01em;
  line-height: 1.25;
}

.prog {
  padding: 0 10px 10px;
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.track {
  height: 3px;
  border-radius: var(--r-full);
  background: var(--surface-4);
  overflow: hidden;
}
.fill {
  height: 100%;
  background: var(--accent);
  border-radius: var(--r-full);
  transition: width 240ms var(--ease);
}

.nav {
  flex: 1;
  min-height: 0;
  padding: 0 6px;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

/* 紧凑：28px 行高、13px 字号，面板多也不挤 */
.item {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  height: 28px;
  padding: 0 8px;
  border: none;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--fg-dim);
  cursor: pointer;
  font-size: var(--t-sm);
  text-align: left;
  transition: background var(--fast), color var(--fast);
}
.item:hover {
  background: var(--surface-3);
  color: var(--fg);
}
.item.on {
  background: var(--surface-4);
  color: var(--fg);
  font-weight: 500;
}
.item.on .ic {
  color: var(--accent);
}
.ic {
  flex: 0 0 auto;
  color: var(--fg-faint);
  transition: color var(--fast);
}
.item:hover .ic {
  color: var(--fg-dim);
}
.lbl {
  min-width: 0;
}
.pct {
  font-size: 10px;
  color: var(--fg-ghost);
}
.done {
  color: var(--ok);
}
.dot {
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: var(--surface-5);
}

.foot {
  padding: 6px;
  border-top: 1px solid var(--line-faint);
  display: flex;
  flex-direction: column;
  gap: 1px;
}
</style>
