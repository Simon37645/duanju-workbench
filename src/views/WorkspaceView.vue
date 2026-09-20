<script setup lang="ts">
import { computed, defineAsyncComponent, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import Sidebar from "@/components/Sidebar.vue";
import JobBar from "@/components/JobBar.vue";
import AgentFloat from "@/components/AgentFloat.vue";
import SettingsModal from "@/components/SettingsModal.vue";
import { useProjectStore } from "@/stores/project";
import { useAgentStore } from "@/stores/agent";
import { PANELS, type PanelId } from "@/types/models";
import UiSpinner from "@/ui/Spinner.vue";

const props = defineProps<{ panel: string }>();
const router = useRouter();
const project = useProjectStore();
const agent = useAgentStore();

const showSettings = ref(false);
const loading = ref(true);

const PANEL_COMPONENTS: Record<PanelId, ReturnType<typeof defineAsyncComponent>> = {
  script: defineAsyncComponent(() => import("@/views/panels/ScriptPanel.vue")),
  style: defineAsyncComponent(() => import("@/views/panels/StylePanel.vue")),
  storyboard: defineAsyncComponent(() => import("@/views/panels/StoryboardPanel.vue")),
  asset: defineAsyncComponent(() => import("@/views/panels/AssetPanel.vue")),
  prompt: defineAsyncComponent(() => import("@/views/panels/PromptPanel.vue")),
  video: defineAsyncComponent(() => import("@/views/panels/VideoPanel.vue")),
  edit: defineAsyncComponent(() => import("@/views/panels/EditPanel.vue")),
  subtitle: defineAsyncComponent(() => import("@/views/panels/SubtitlePanel.vue")),
  previz: defineAsyncComponent(() => import("@/views/panels/PrevizPanel.vue")),
};

const activePanel = computed<PanelId>(() => {
  const p = props.panel as PanelId;
  return PANELS.some((x) => x.id === p) ? p : "script";
});
const currentComponent = computed(() => PANEL_COMPONENTS[activePanel.value]);

watch(activePanel, (p) => agent.previewPrefix(p));

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
  loading.value = false;
});
</script>

<template>
  <div class="shell">
    <Sidebar :active="activePanel" @settings="showSettings = true" />

    <div class="main">
      <div class="work">
        <div v-if="loading" class="center-all">
          <UiSpinner :size="18" label="正在读取项目…" />
        </div>
        <component :is="currentComponent" v-else :panel="activePanel" />
      </div>

      <JobBar />
    </div>

    <!-- Simon 浮在内容之上，面板因此能拿到全部宽度 -->
    <AgentFloat :panel="activePanel" />
    <SettingsModal :show="showSettings" @update:show="(v: boolean) => (showSettings = v)" />
  </div>
</template>

<style scoped>
.shell {
  position: relative;
  height: 100%;
  display: flex;
  min-height: 0;
  background: var(--bg);
  overflow: hidden;
}
.main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
  /* 给折叠态的导航条留位置（它是 absolute 浮层，展开时会盖在上面） */
  margin-left: 48px;
}
.work {
  flex: 1;
  min-height: 0;
  display: flex;
  /* 面板铺满：不留外边距与间隙 */
  padding: 0;
  gap: 0;
}
.center-all {
  flex: 1;
  display: grid;
  place-items: center;
}
</style>
