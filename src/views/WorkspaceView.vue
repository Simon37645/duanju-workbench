<script setup lang="ts">
import { computed, defineAsyncComponent, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { NSpin } from "naive-ui";
import PanelRail from "@/components/PanelRail.vue";
import TopBar from "@/components/TopBar.vue";
import JobBar from "@/components/JobBar.vue";
import AgentDock from "@/components/AgentDock.vue";
import SettingsDrawer from "@/components/SettingsDrawer.vue";
import { useProjectStore } from "@/stores/project";
import { useAgentStore } from "@/stores/agent";
import { useSettingsStore } from "@/stores/settings";
import { PANELS, type PanelId } from "@/types/models";

const props = defineProps<{ panel: string }>();
const router = useRouter();
const project = useProjectStore();
const agent = useAgentStore();
const settings = useSettingsStore();

const showSettings = ref(false);
const dockOpen = ref(true);
const loading = ref(true);

const panelComponents: Record<PanelId, ReturnType<typeof defineAsyncComponent>> = {
  script: defineAsyncComponent(() => import("@/views/panels/ScriptPanel.vue")),
  style: defineAsyncComponent(() => import("@/views/panels/StylePanel.vue")),
  storyboard: defineAsyncComponent(() => import("@/views/panels/StoryboardPanel.vue")),
  asset: defineAsyncComponent(() => import("@/views/panels/AssetPanel.vue")),
  prompt: defineAsyncComponent(() => import("@/views/panels/PromptPanel.vue")),
  video: defineAsyncComponent(() => import("@/views/panels/VideoPanel.vue")),
  edit: defineAsyncComponent(() => import("@/views/panels/EditPanel.vue")),
  subtitle: defineAsyncComponent(() => import("@/views/panels/SubtitlePanel.vue")),
  checklist: defineAsyncComponent(() => import("@/views/panels/ChecklistPanel.vue")),
};

const activePanel = computed<PanelId>(() => {
  const p = props.panel as PanelId;
  return PANELS.some((x) => x.id === p) ? p : "script";
});

const currentComponent = computed(() => panelComponents[activePanel.value]);
const activeMeta = computed(() => PANELS.find((p) => p.id === activePanel.value)!);

watch(activePanel, async () => {
  await agent.previewPrefix(activePanel.value);
});

onMounted(async () => {
  if (!project.snapshot) {
    try {
      await project.ensure();
    } catch {
      router.replace({ name: "home" });
      return;
    }
  }
  await agent.loadSessions();
  await agent.previewPrefix(activePanel.value);
  dockOpen.value = settings.settings?.agentDockOpen ?? true;
  loading.value = false;
});

watch(dockOpen, (v) => settings.save({ agentDockOpen: v }));
</script>

<template>
  <div class="shell">
    <TopBar @open-settings="showSettings = true" />

    <div class="main">
      <PanelRail :active="activePanel" />

      <section class="center">
        <div class="center-head">
          <div class="row" style="gap: 8px">
            <span class="breadcrumb">
              第 {{ activeMeta.index }} 步 / 共 9 步
            </span>
            <h2 style="font-size: 15px">{{ activeMeta.title }}</h2>
            <span class="tiny faint">{{ activeMeta.subtitle }}</span>
          </div>
          <div class="row" style="gap: 6px">
            <span
              v-for="b in project.blockersOf(activePanel).slice(0, 1)"
              :key="b"
              class="tag warn truncate"
              style="max-width: 340px"
              :title="b"
            >
              {{ b }}
            </span>
            <button class="dock-toggle" @click="dockOpen = !dockOpen">
              {{ dockOpen ? "隐藏 Agent ▸" : "◂ Agent" }}
            </button>
          </div>
        </div>

        <div class="center-body scroll">
          <n-spin v-if="loading" style="width: 100%; margin-top: 60px" />
          <component :is="currentComponent" v-else :panel="activePanel" />
        </div>
      </section>

      <AgentDock v-if="dockOpen" :panel="activePanel" style="width: 420px; flex: 0 0 420px" />
    </div>

    <JobBar />
    <SettingsDrawer v-model:show="showSettings" />
  </div>
</template>

<style scoped>
.shell {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.main {
  flex: 1;
  display: flex;
  min-height: 0;
}
.center {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.center-head {
  height: 42px;
  flex: 0 0 42px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 0 14px;
  border-bottom: 1px solid var(--line);
  background: var(--bg-1);
}
.breadcrumb {
  font-size: 11px;
  color: var(--text-faint);
}
.center-body {
  flex: 1;
  min-height: 0;
  padding: 14px;
}
.dock-toggle {
  background: transparent;
  border: 1px solid var(--line);
  border-radius: 6px;
  color: var(--text-dim);
  font-size: 11px;
  padding: 3px 8px;
  cursor: pointer;
}
.dock-toggle:hover {
  color: var(--text);
  border-color: #3a3a47;
}
</style>
