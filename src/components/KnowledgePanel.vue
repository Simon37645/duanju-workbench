<script setup lang="ts">
/**
 * 知识包面板：用户可以把自己的创作方法论、风格圣经、自检标准导入进来。
 *
 * 这里刻意把「正文不会自动进系统提示」讲清楚 —— 它只进目录，
 * agent 需要时用工具读 —— 否则用户会以为没生效。
 */
import { computed, onMounted, ref } from "vue";
import {
  BookOpen, ChevronDown, FileText, FolderOpen, Import, Plus, Trash2,
} from "@lucide/vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiInput from "@/ui/Input.vue";
import UiTextarea from "@/ui/Textarea.vue";
import UiSwitch from "@/ui/Switch.vue";
import UiEmpty from "@/ui/Empty.vue";
import UiSpinner from "@/ui/Spinner.vue";
import { toast } from "@/ui";
import { api, errorText } from "@/api/ipc";
import type { KnowledgePack } from "@/types/models";

const packs = ref<KnowledgePack[]>([]);
const loading = ref(true);
const expanded = ref<Record<string, boolean>>({});
const editing = ref<{ id: string; body: string } | null>(null);

const KIND: Record<string, { label: string; tone: "accent" | "info" | "ok" | "neutral" }> = {
  methodology: { label: "方法论", tone: "accent" },
  style: { label: "风格圣经", tone: "info" },
  checklist: { label: "自检标准", tone: "ok" },
  reference: { label: "参考", tone: "neutral" },
};
const kindOf = (k: string) => KIND[k] ?? KIND.reference;

const enabledCount = computed(() => packs.value.filter((p) => p.enabled).length);

onMounted(load);

async function load() {
  loading.value = true;
  try {
    packs.value = await api.knowledgeList();
  } catch (e) {
    toast.err(errorText(e));
  } finally {
    loading.value = false;
  }
}

async function importFiles() {
  const picked = await openDialog({
    multiple: true,
    filters: [{ name: "Markdown", extensions: ["md", "markdown", "txt"] }],
    title: "选择知识包（Markdown 文件）",
  });
  const list = Array.isArray(picked) ? picked : picked ? [picked] : [];
  if (!list.length) return;
  try {
    const added = await api.knowledgeImport(list);
    await load();
    toast.ok(`已导入 ${added.length} 个知识包`);
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function openDir() {
  try {
    const dir = await api.knowledgeOpenDir();
    await openPath(dir);
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function toggle(p: KnowledgePack, v: boolean) {
  try {
    await api.knowledgeSetEnabled(p.id, v);
    p.enabled = v;
    toast.info(v ? "已启用，下一轮对话生效" : "已停用");
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function remove(p: KnowledgePack) {
  try {
    await api.knowledgeDelete(p.id);
    await load();
    toast.ok(`已删除「${p.name}」`);
  } catch (e) {
    toast.err(errorText(e));
  }
}

function startNew() {
  editing.value = {
    id: "",
    body: "---\nname: 我的创作方法\nkind: methodology\nsummary: 一句话说明它管什么\n---\n\n# 我的创作方法\n\n在这里写正文……\n",
  };
}

async function saveEdit() {
  if (!editing.value) return;
  try {
    await api.knowledgeSave(editing.value.id, editing.value.body);
    editing.value = null;
    await load();
    toast.ok("已保存");
  } catch (e) {
    toast.err(errorText(e));
  }
}
</script>

<template>
  <div class="col" style="gap: var(--sp-3)">
    <div class="hint">
      <div class="row-between">
        <span class="section-label">知识包</span>
        <span class="t-xs faint">{{ enabledCount }} / {{ packs.length }} 启用中</span>
      </div>
      <p class="t-xs" style="line-height: 1.8; margin-top: 6px">
        把你自己的创作方法论、风格圣经、自检标准放进来，Simon 就会按它工作。
        <b>正文不会塞进系统提示</b> —— 提示里只带一份目录，它需要细节时自己去读。
        这样几万字的方法论不会每轮重复计费，也不会打掉提示词缓存。
      </p>
      <div class="row wrap" style="gap: 6px; margin-top: 10px">
        <UiButton variant="outline" size="sm" @click="importFiles">
          <template #icon><Import :size="13" /></template>
          导入 Markdown
        </UiButton>
        <UiButton variant="subtle" size="sm" @click="openDir">
          <template #icon><FolderOpen :size="13" /></template>
          打开目录
        </UiButton>
        <UiButton variant="subtle" size="sm" @click="startNew">
          <template #icon><Plus :size="13" /></template>
          新建
        </UiButton>
      </div>
      <p class="t-xs faint" style="line-height: 1.7; margin-top: 8px">
        手上的 Word 文档先用仓库里的 <span class="mono">scripts/docx2md.py</span> 转成
        <span class="mono">.md</span> 再导入。
      </p>
    </div>

    <!-- 编辑器 -->
    <div v-if="editing" class="card card-pad col" style="gap: 10px">
      <div class="row-between">
        <span class="section-label">{{ editing.id ? `编辑 ${editing.id}` : "新建知识包" }}</span>
        <div class="row" style="gap: 4px">
          <UiButton variant="ghost" size="xs" @click="editing = null">取消</UiButton>
          <UiButton variant="primary" size="xs" @click="saveEdit">保存</UiButton>
        </div>
      </div>
      <UiTextarea v-model="editing.body" :rows="12" mono />
    </div>

    <UiSpinner v-if="loading" label="正在读取知识包…" />

    <UiEmpty
      v-else-if="!packs.length"
      title="还没有知识包"
      hint="导入一份 Markdown，或点「打开目录」直接把文件丢进去"
    >
      <template #icon><BookOpen :size="26" /></template>
      <UiButton variant="outline" size="sm" @click="importFiles">导入 Markdown</UiButton>
    </UiEmpty>

    <div v-else class="col" style="gap: 8px">
      <div v-for="p in packs" :key="p.id" class="pack">
        <div class="row" style="gap: 8px; min-width: 0">
          <FileText :size="14" class="faint" style="flex: 0 0 auto" />
          <div class="col grow" style="gap: 1px; min-width: 0">
            <div class="row" style="gap: 6px">
              <span class="t-sm truncate" style="font-weight: 500">{{ p.name }}</span>
              <UiBadge :tone="kindOf(p.kind).tone" size="xs">{{ kindOf(p.kind).label }}</UiBadge>
              <span class="t-xs faint num">{{ (p.chars / 1000).toFixed(1) }}k 字</span>
            </div>
            <span class="t-xs faint clamp-2">{{ p.summary }}</span>
          </div>
          <div class="row" style="gap: 4px; flex: 0 0 auto">
            <UiSwitch :model-value="p.enabled" @update:model-value="(v: boolean) => toggle(p, v)" />
            <UiButton variant="subtle" size="xs" icon @click="expanded[p.id] = !expanded[p.id]">
              <template #icon>
                <ChevronDown :size="12" :class="{ rot: !expanded[p.id] }" />
              </template>
            </UiButton>
            <UiButton
              variant="subtle"
              size="xs"
              icon
              @click="editing = { id: p.id, body: `---\nname: ${p.name}\nkind: ${p.kind}\nsummary: ${p.summary}\n---\n\n${p.body}` }"
            >
              <template #icon><BookOpen :size="12" /></template>
            </UiButton>
            <UiButton variant="subtle" size="xs" icon @click="remove(p)">
              <template #icon><Trash2 :size="12" /></template>
            </UiButton>
          </div>
        </div>
        <pre v-if="expanded[p.id]" class="preview">{{ p.body.slice(0, 3000) }}{{ p.body.length > 3000 ? "\n\n…（后面还有 " + (p.chars - 3000) + " 字）" : "" }}</pre>
      </div>
    </div>
  </div>
</template>

<style scoped>
.hint {
  padding: 12px;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
}
.pack {
  padding: 10px;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
}
.pack:hover {
  border-color: var(--line-strong);
}
.preview {
  margin: 8px 0 0;
  padding: 10px 12px;
  background: var(--code-bg);
  border-radius: var(--r-sm);
  color: var(--fg-dim);
  font-size: var(--t-xs);
  line-height: 1.7;
  white-space: pre-wrap;
  max-height: 240px;
  overflow: auto;
  font-family: inherit;
}
.rot {
  transform: rotate(-90deg);
}
.mono {
  font-family: "JetBrains Mono", Consolas, monospace;
}
</style>
