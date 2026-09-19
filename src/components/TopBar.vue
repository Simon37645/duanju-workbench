<script setup lang="ts">
import { computed, ref } from "vue";
import { NButton, NSelect, NPopover, NInput, NInputNumber, useDialog } from "naive-ui";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import { useAgentStore } from "@/stores/agent";
import { api, errorText } from "@/api/ipc";
import { message } from "@/utils/notify";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";

const emit = defineEmits<{ (e: "open-settings"): void }>();
const project = useProjectStore();
const settings = useSettingsStore();
const agent = useAgentStore();
const dialog = useDialog();

const editing = ref(false);
const draft = ref({ name: "", genre: "", logline: "", episodeCountHint: 0, aspectRatio: "9:16" });

const llmOptions = computed(() =>
  settings.byKind("llm").map((p) => ({ label: `${p.name} · ${p.model || "未填模型"}`, value: p.id })),
);

function startEdit() {
  const m = project.manifest;
  if (!m) return;
  draft.value = {
    name: m.name,
    genre: m.genre,
    logline: m.logline,
    episodeCountHint: m.episodeCountHint,
    aspectRatio: m.aspectRatio,
  };
  editing.value = true;
}

async function saveEdit() {
  try {
    await api.projectUpdateManifest(draft.value);
    await project.reload();
    editing.value = false;
    message.success("项目信息已保存");
  } catch (e) {
    message.error(errorText(e));
  }
}

async function pickLlm(id: string) {
  await settings.save({ activeLlmProviderId: id });
  // 换模型会让缓存前缀失效，这里明确提示一下
  message.info("已切换 agent 模型；模型变化会重建提示词前缀（缓存需重新写入一次）");
}

async function openFolder() {
  try {
    const paths = await api.projectPaths(project.root);
    await openPath(paths.root);
  } catch (e) {
    message.error(errorText(e));
  }
}

async function pickFolderAsProject() {
  const picked = await openDialog({ directory: true, multiple: false, title: "选择项目目录" });
  if (typeof picked === "string") {
    try {
      await project.open(picked);
      message.success("项目已打开");
    } catch (e) {
      message.error(errorText(e));
    }
  }
}

function closeProject() {
  dialog.warning({
    title: "关闭项目",
    content: "关闭后可以再从项目列表打开，数据都在磁盘上，不会丢失。",
    positiveText: "关闭",
    negativeText: "取消",
    onPositiveClick: async () => {
      await project.close();
      location.hash = "#/";
    },
  });
}

const cacheText = computed(() => {
  const u = agent.usage;
  if (!u) return "缓存：暂无数据";
  const pct = (u.hitRate * 100).toFixed(0);
  return `缓存命中 ${pct}%`;
});
</script>

<template>
  <header class="topbar">
    <div class="row grow" style="min-width: 0">
      <template v-if="!editing">
        <div class="col" style="gap: 0; min-width: 0">
          <div class="row" style="gap: 6px">
            <span style="font-weight: 600">{{ project.manifest?.name }}</span>
            <span v-if="project.manifest?.genre" class="tag">{{ project.manifest.genre }}</span>
            <span v-if="project.manifest?.episodeCountHint" class="tag">
              {{ project.manifest.episodeCountHint }} 章
            </span>
            <span class="tag">{{ project.manifest?.aspectRatio }}</span>
          </div>
          <div class="tiny faint truncate" style="max-width: 46vw" :title="project.root">
            {{ project.manifest?.logline || project.root }}
          </div>
        </div>
        <n-button size="tiny" quaternary @click="startEdit">编辑</n-button>
      </template>

      <template v-else>
        <n-input v-model:value="draft.name" size="tiny" placeholder="项目名" style="width: 140px" />
        <n-input v-model:value="draft.genre" size="tiny" placeholder="题材" style="width: 96px" />
        <n-input
          v-model:value="draft.logline"
          size="tiny"
          placeholder="一句话卖点"
          style="width: 220px"
        />
        <n-input-number v-model:value="draft.episodeCountHint" size="tiny" style="width: 96px" />
        <n-input v-model:value="draft.aspectRatio" size="tiny" style="width: 70px" />
        <n-button size="tiny" type="primary" @click="saveEdit">保存</n-button>
        <n-button size="tiny" quaternary @click="editing = false">取消</n-button>
      </template>
    </div>

    <div class="row" style="gap: 6px; flex: 0 0 auto">
      <span v-if="project.saving" class="tiny faint">保存中…</span>

      <n-popover trigger="hover" placement="bottom">
        <template #trigger>
          <span class="tag" :class="agent.usage && agent.usage.hitRate > 0.5 ? 'ok' : ''">
            {{ cacheText }}
          </span>
        </template>
        <div style="max-width: 340px; font-size: 12px; line-height: 1.7">
          <div style="font-weight: 600; margin-bottom: 4px">Agent 前缀缓存</div>
          <div v-if="agent.usage">
            本轮输入 {{ agent.usage.inputTokens }} · 命中 {{ agent.usage.cacheReadTokens }} ·
            写入 {{ agent.usage.cacheWriteTokens }}
          </div>
          <div v-if="agent.usage">
            累计：输入 {{ agent.usage.totalInputTokens }} · 命中
            {{ agent.usage.totalCacheReadTokens }} · 输出 {{ agent.usage.totalOutputTokens }}
          </div>
          <div v-if="agent.prefix" style="margin-top: 6px">
            <div>前缀指纹 <span class="mono">{{ agent.prefix.fingerprint }}</span></div>
            <div :style="{ color: agent.prefix.stable ? 'var(--ok)' : 'var(--warn)' }">
              {{ agent.prefix.note }}
            </div>
            <div class="faint" style="margin-top: 4px">分层：</div>
            <div v-for="l in agent.prefix.layers" :key="l.name" class="row-between">
              <span class="truncate" style="max-width: 190px">{{ l.name }}</span>
              <span class="faint mono tiny">
                ~{{ l.estTokens }} tok {{ l.breakpoint ? "·缓存点" : "" }}
              </span>
            </div>
          </div>
        </div>
      </n-popover>

      <n-select
        v-if="llmOptions.length"
        size="tiny"
        style="width: 190px"
        :value="settings.settings?.activeLlmProviderId ?? undefined"
        :options="llmOptions"
        placeholder="选择 agent 模型"
        @update:value="pickLlm"
      />
      <n-button v-else size="tiny" type="warning" ghost @click="emit('open-settings')">
        未配置模型
      </n-button>

      <n-button size="tiny" quaternary @click="pickFolderAsProject">打开其他项目</n-button>
      <n-button size="tiny" quaternary @click="openFolder">目录</n-button>
      <n-button size="tiny" quaternary @click="closeProject">关闭</n-button>
      <n-button size="tiny" quaternary @click="emit('open-settings')">设置</n-button>
    </div>
  </header>
</template>

<style scoped>
.topbar {
  height: 50px;
  flex: 0 0 50px;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 0 12px;
  background: var(--bg-1);
  border-bottom: 1px solid var(--line);
}
</style>
