<script setup lang="ts">
import { onMounted, watch } from "vue";
import { useRouter } from "vue-router";
import ToastHost from "@/ui/ToastHost.vue";
import ConfirmHost from "@/ui/ConfirmHost.vue";
import TourOverlay from "@/components/TourOverlay.vue";
import { useSettingsStore } from "@/stores/settings";
import { useJobsStore } from "@/stores/jobs";
import { useProjectStore } from "@/stores/project";
import { installDirectorBridge } from "@/utils/directorBridge";

const router = useRouter();
const settings = useSettingsStore();
const jobs = useJobsStore();
const project = useProjectStore();

/** 主题挂在 <html data-theme> 上，所有颜色都从 tokens.css 里取 */
function applyTheme(theme: string) {
  document.documentElement.dataset.theme = theme === "light" ? "light" : "dark";
}

watch(
  () => settings.settings?.theme,
  (t) => applyTheme(t ?? "dark"),
);

onMounted(async () => {
  installDirectorBridge();
  await settings.load();
  applyTheme(settings.settings?.theme ?? "dark");
  jobs.bind();
  await project.restoreLast();
  if (project.snapshot) {
    router.replace({ name: "workspace", params: { panel: "script" } });
  }
});
</script>

<template>
  <router-view />
  <ToastHost />
  <ConfirmHost />
  <TourOverlay />
</template>
