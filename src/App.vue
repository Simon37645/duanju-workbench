<script setup lang="ts">
import { NConfigProvider, NDialogProvider, NMessageProvider, darkTheme, zhCN, dateZhCN } from "naive-ui";
import type { GlobalThemeOverrides } from "naive-ui";
import { onMounted } from "vue";
import { useRouter } from "vue-router";
import { useSettingsStore } from "@/stores/settings";
import { useJobsStore } from "@/stores/jobs";
import { useProjectStore } from "@/stores/project";

const router = useRouter();
const settings = useSettingsStore();
const jobs = useJobsStore();
const project = useProjectStore();

const overrides: GlobalThemeOverrides = {
  common: {
    primaryColor: "#e8a33d",
    primaryColorHover: "#f2b559",
    primaryColorPressed: "#d18f2b",
    primaryColorSuppl: "#e8a33d",
    borderRadius: "6px",
    fontSize: "13px",
    bodyColor: "#0f0f14",
    cardColor: "#16161d",
    modalColor: "#1a1a22",
    popoverColor: "#1c1c24",
    tableColor: "#16161d",
    inputColor: "#1a1a22",
  },
  Card: { borderColor: "#26262f" },
  Layout: { color: "#0f0f14", siderColor: "#131319", headerColor: "#131319" },
};

onMounted(async () => {
  await settings.load();
  jobs.bind();
  await project.restoreLast();
  if (project.snapshot) {
    router.replace({ name: "workspace", params: { panel: "script" } });
  }
});
</script>

<template>
  <n-config-provider :theme="darkTheme" :theme-overrides="overrides" :locale="zhCN" :date-locale="dateZhCN">
    <n-message-provider>
      <n-dialog-provider>
        <router-view />
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>
