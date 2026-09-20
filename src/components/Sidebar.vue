<script setup lang="ts">
/**
 * 左侧导航：默认收成一条 48px 的图标条（面板区域尽量全屏），
 * 鼠标悬停或点击图钉展开完整导航（浮层覆盖，不挤压面板）。
 */
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import {
  Axis3d, BookText, Captions, Check, Clapperboard, FolderOpen, Images, LayoutGrid,
  Palette, PanelLeftClose, PanelLeftOpen, Scissors, Settings, Sparkles, Sun, Moon,
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
  previz: Axis3d,
  prompt: Sparkles,
  video: Clapperboard,
  edit: Scissors,
  subtitle: Captions,
};

const pinned = ref((() => {
  try {
    return localStorage.getItem("nav.pinned") === "1";
  } catch {
    return false;
  }
})());
const hovering = ref(false);
const expanded = computed(() => pinned.value || hovering.value);

function togglePin() {
  pinned.value = !pinned.value;
  try {
    localStorage.setItem("nav.pinned", pinned.value ? "1" : "0");
  } catch {
    /* 忽略 */
  }
}

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
  <aside
    class="side"
    :class="{ expanded }"
    @mouseenter="hovering = true"
    @mouseleave="hovering = false"
  >
    <!-- 顶部：品牌 + 图钉 -->
    <div class="brand" :class="{ bare: !expanded }">
      <div class="mark">S</div>
      <div v-if="expanded" class="col grow" style="gap: 0; min-width: 0">
        <span class="bname truncate">Simon</span>
        <span class="t-xs faint truncate" :title="project.root">
          {{ project.manifest?.name ?? "未打开项目" }}
        </span>
      </div>
      <button class="pin" :title="pinned ? '收起导航' : '固定展开'" @click="togglePin">
        <PanelLeftClose v-if="expanded" :size="14" />
        <PanelLeftOpen v-else :size="14" />
      </button>
    </div>

    <!-- 全流程进度（展开态） -->
    <div v-if="expanded" class="prog">
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
        :content="expanded ? '' : row.blockers.join('；') || row.title"
      >
        <button class="item" :class="{ on: props.active === row.id }" @click="go(row.id)">
          <component :is="row.icon" :size="15" class="ic" />
          <template v-if="expanded">
            <span class="lbl grow truncate">{{ row.title }}</span>
            <Check v-if="row.done" :size="11" class="done" />
            <span v-else-if="row.percent > 0" class="pct num">{{ Math.round(row.percent) }}</span>
            <span v-else class="dot" />
          </template>
        </button>
      </UiTooltip>
    </nav>

    <!-- 底部 -->
    <div class="foot">
      <UiTooltip placement="right" :content="expanded ? '' : '项目列表'">
        <button class="item" @click="router.push({ name: 'home' })">
          <FolderOpen :size="15" class="ic" />
          <span v-if="expanded" class="lbl">项目列表</span>
        </button>
      </UiTooltip>
      <UiTooltip placement="right" :content="expanded ? '' : isLight ? '切换暗色' : '切换亮色'">
        <button class="item" @click="toggleTheme">
          <Sun v-if="isLight" :size="15" class="ic" />
          <Moon v-else :size="15" class="ic" />
          <span v-if="expanded" class="lbl">{{ isLight ? "亮色" : "暗色" }}</span>
        </button>
      </UiTooltip>
      <UiTooltip placement="right" :content="expanded ? '' : '设置（项目信息 / 模型供应商 / 技能…）'">
        <button class="item" @click="emit('settings')">
          <Settings :size="15" class="ic" />
          <span v-if="expanded" class="lbl">设置</span>
        </button>
      </UiTooltip>
    </div>
  </aside>
</template>

<style scoped>
.side {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  z-index: 60;
  width: 48px;
  flex: 0 0 auto;
  background: var(--surface-1);
  border-right: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  transition: width 160ms var(--ease);
}
.side.expanded {
  width: var(--sidebar-w);
  box-shadow: var(--shadow-lg);
}

.brand {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 0 8px;
  padding-left: 11px;
  min-height: 46px;
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
.pin {
  border: none;
  background: none;
  color: var(--fg-faint);
  cursor: pointer;
  padding: 4px;
  flex: 0 0 auto;
}
.pin:hover {
  color: var(--fg);
}
.brand:not(.bare) .pin {
  margin-right: 6px;
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

/* 折叠态图标居中、展开态左对齐 */
.item {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  height: 30px;
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
