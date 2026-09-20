<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { Eye, Hash, Lock, Pencil, Plus, Trash2, Wand2 } from "@lucide/vue";
import { useProjectStore } from "@/stores/project";
import { useAgentStore } from "@/stores/agent";
import { api, errorText } from "@/api/ipc";
import { toast, confirmDialog } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiInput from "@/ui/Input.vue";
import UiTextarea from "@/ui/Textarea.vue";
import UiSelect from "@/ui/Select.vue";
import UiSegmented from "@/ui/Segmented.vue";
import UiEmpty from "@/ui/Empty.vue";
import UiTooltip from "@/ui/Tooltip.vue";
import UiModal from "@/ui/Modal.vue";
import UiNumber from "@/ui/NumberInput.vue";
import Markdown from "@/components/Markdown.vue";
import type { ChapterStatus } from "@/types/models";

const project = useProjectStore();
const agent = useAgentStore();

const currentId = ref<string | null>(null);
const content = ref("");
const title = ref("");
const summary = ref("");
const dirty = ref(false);
const mode = ref("edit");
const showCount = ref(false);
const wantCount = ref(6);

const chapters = computed(() => project.chapters);
const current = computed(() => chapters.value.find((c) => c.id === currentId.value) ?? null);

const STATUS: Record<ChapterStatus, { label: string; tone: "neutral" | "warn" | "ok" | "accent" }> = {
  empty: { label: "空", tone: "neutral" },
  draft: { label: "草稿", tone: "warn" },
  written: { label: "已写", tone: "ok" },
  locked: { label: "锁定", tone: "accent" },
};

const statusOptions = (Object.keys(STATUS) as ChapterStatus[]).map((k) => ({
  label: STATUS[k].label,
  value: k,
}));

const wordCount = computed(() => content.value.replace(/\s/g, "").length);

onMounted(async () => {
  if (chapters.value.length) await select(chapters.value[0].id);
});

watch(
  () => project.currentChapterId,
  async (id) => {
    if (id && id !== currentId.value) await select(id);
  },
);

async function select(id: string) {
  if (dirty.value) await save();
  currentId.value = id;
  const ch = await project.openChapter(id);
  content.value = ch.content;
  title.value = ch.meta.title;
  summary.value = ch.meta.summary;
  dirty.value = false;
}

async function save() {
  if (!currentId.value || !dirty.value) return;
  try {
    await project.saveChapter(currentId.value, content.value);
    await project.updateChapterMeta(currentId.value, { title: title.value, summary: summary.value });
    dirty.value = false;
    toast.ok("已保存");
  } catch (e) {
    toast.err(`保存失败：${errorText(e)}`);
  }
}

async function addChapter() {
  const meta = await project.addChapter();
  await select(meta.id);
}

async function removeChapter(id: string) {
  const ch = chapters.value.find((c) => c.id === id);
  const ok = await confirmDialog({
    title: "删除章节",
    content: `确定删除「${ch?.title}」？该章的分镜与提示词会一并删除，正文文件也会移除。`,
    positiveText: "删除",
    danger: true,
  });
  if (!ok) return;
  await project.deleteChapter(id);
  if (currentId.value === id) {
    currentId.value = null;
    content.value = "";
    if (chapters.value.length) await select(chapters.value[0].id);
  }
}

async function move(id: string, dir: -1 | 1) {
  const idx = chapters.value.findIndex((c) => c.id === id);
  const next = idx + dir;
  if (next < 0 || next >= chapters.value.length) return;
  const ids = chapters.value.map((c) => c.id);
  [ids[idx], ids[next]] = [ids[next], ids[idx]];
  await api.scriptReorderChapters(ids);
  await project.reload();
}

async function applyCount() {
  const want = Math.max(1, wantCount.value);
  const have = chapters.value.length;
  if (want > have) {
    for (let i = have; i < want; i++) await project.addChapter();
    toast.ok(`已追加 ${want - have} 章`);
  } else if (want < have) {
    toast.warn(`当前已有 ${have} 章，减少请手动删除，避免误删正文`);
  }
  showCount.value = false;
}

function askAgent() {
  agent.send(
    "script",
    currentId.value
      ? `帮我把「${current.value?.title}」这一章写完整，保持短剧节奏。`
      : "帮我按 4 章拆分故事大纲。",
  );
}
</script>

<template>
  <div class="layout">
    <!-- 章节列表 -->
    <section class="panel list-panel">
      <header class="panel-head">
        <div class="row" style="gap: 6px">
          <span class="section-label">章节</span>
          <UiBadge tone="neutral" size="xs">{{ chapters.length }}</UiBadge>
        </div>
        <div class="row" style="gap: 2px">
          <UiTooltip content="按数量补齐章节">
            <UiButton variant="subtle" size="xs" icon @click="showCount = true">
              <template #icon><Hash :size="13" /></template>
            </UiButton>
          </UiTooltip>
          <UiTooltip content="新增一章">
            <UiButton variant="subtle" size="xs" icon @click="addChapter">
              <template #icon><Plus :size="13" /></template>
            </UiButton>
          </UiTooltip>
        </div>
      </header>

      <div class="list scroll">
        <div
          v-for="c in chapters"
          :key="c.id"
          class="chap"
          :class="{ on: c.id === currentId }"
          @click="select(c.id)"
        >
          <div class="row-between">
            <div class="row" style="gap: 7px; min-width: 0">
              <span class="idx num">{{ c.index }}</span>
              <span class="ctitle truncate">{{ c.title }}</span>
            </div>
            <UiBadge :tone="STATUS[c.status].tone" size="xs">{{ STATUS[c.status].label }}</UiBadge>
          </div>
          <div class="csum t-xs faint truncate">{{ c.summary || "（无大纲）" }}</div>
          <div class="row-between">
            <span class="t-xs faint num">{{ c.wordCount }} 字</span>
            <div class="ops">
              <button class="op" @click.stop="move(c.id, -1)">↑</button>
              <button class="op" @click.stop="move(c.id, 1)">↓</button>
              <button class="op danger" @click.stop="removeChapter(c.id)"><Trash2 :size="11" /></button>
            </div>
          </div>
        </div>

        <UiEmpty
          v-if="!chapters.length"
          compact
          title="还没有章节"
          hint="点右上角补齐，或直接让右侧 agent 生成章节骨架"
        />
      </div>
    </section>

    <!-- 编辑区 -->
    <section v-if="current" class="panel edit-panel">
      <header class="panel-head">
        <div class="row grow" style="gap: 8px; min-width: 0">
          <UiInput
            v-model="title"
            placeholder="章节标题"
            class="title-input"
            @update:model-value="dirty = true"
          />
          <UiSelect
            :model-value="current.status"
            :options="statusOptions"
            style="width: 96px"
            @update:model-value="(v: string | null) => project.updateChapterMeta(current!.id, { status: v as ChapterStatus })"
          />
          <UiBadge v-if="current.status === 'locked'" tone="accent" size="xs">
            <Lock :size="9" /> 锁定
          </UiBadge>
        </div>
        <div class="row" style="gap: 8px">
          <span class="t-xs faint num">{{ wordCount }} 字</span>
          <UiSegmented
            v-model="mode"
            size="xs"
            :items="[
              { label: '编辑', value: 'edit', icon: Pencil },
              { label: '预览', value: 'preview', icon: Eye },
            ]"
          />
          <UiButton variant="primary" size="sm" :disabled="!dirty" @click="save">
            {{ dirty ? "保存" : "已保存" }}
          </UiButton>
        </div>
      </header>

      <div class="edit-body">
        <UiTextarea
          v-model="summary"
          :rows="2"
          placeholder="本章大纲 —— 会喂给分镜与资产面板，写清出场人物与关键道具"
          @update:model-value="dirty = true"
          @blur="save"
        />
        <Markdown v-if="mode === 'preview'" :text="content || '*（还没有正文）*'" class="preview scroll" />
        <UiTextarea
          v-else
          v-model="content"
          class="editor"
          placeholder="章节正文。短剧节奏：开场 3 秒抓人，中间有小反转，结尾留钩子。"
          @update:model-value="dirty = true"
          @keydown="(e: KeyboardEvent) => { if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); save(); } }"
        />
      </div>
    </section>

    <section v-else class="panel edit-panel">
      <UiEmpty title="选择一章开始写" hint="左侧选一章，或者新建一章">
        <template #icon><Pencil :size="26" /></template>
        <UiButton variant="outline" size="sm" @click="askAgent">
          <template #icon><Wand2 :size="13" /></template>
          让 agent 生成大纲
        </UiButton>
      </UiEmpty>
    </section>

    <UiModal :show="showCount" title="设定章节数" :width="400" @update:show="(v: boolean) => (showCount = v)">
      <div class="col" style="gap: 12px">
        <p class="t-sm dim">当前 {{ chapters.length }} 章。增加会追加空白章节，减少请手动删除。</p>
        <UiNumber v-model="wantCount" :min="1" :max="200" />
      </div>
      <template #footer>
        <UiButton variant="ghost" @click="showCount = false">取消</UiButton>
        <UiButton variant="primary" @click="applyCount">确定</UiButton>
      </template>
    </UiModal>
  </div>
</template>

<style scoped>
.layout {
  flex: 1;
  min-width: 0;
  display: flex;
  gap: var(--sp-3);
  height: 100%;
  min-height: 0;
}
.list-panel {
  width: 272px;
  flex: 0 0 272px;
}
.edit-panel {
  flex: 1;
  min-width: 0;
}

.list {
  flex: 1;
  min-height: 0;
  padding: var(--sp-2);
}
.chap {
  padding: 7px 9px;
  border-radius: var(--r);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 3px;
  border: 1px solid transparent;
  transition: background var(--fast), border-color var(--fast);
}
.chap:hover {
  background: var(--surface-3);
}
.chap.on {
  background: var(--surface-3);
  border-color: var(--line-strong);
}
.idx {
  width: 18px;
  flex: 0 0 auto;
  font-size: var(--t-xs);
  color: var(--fg-ghost);
  text-align: right;
}
.ctitle {
  font-size: var(--t-base);
  font-weight: 500;
}
.csum {
  padding-left: 25px;
  margin-top: -2px;
}
.chap .row-between:last-child {
  padding-left: 25px;
  height: 16px;
}
.ops {
  display: flex;
  gap: 2px;
  opacity: 0;
  transition: opacity var(--fast);
}
.chap:hover .ops {
  opacity: 1;
}
.op {
  background: none;
  border: none;
  color: var(--fg-ghost);
  cursor: pointer;
  font-size: 11px;
  padding: 0 3px;
  border-radius: var(--r-xs);
  line-height: 1;
}
.op:hover {
  color: var(--fg);
  background: var(--surface-4);
}
.op.danger:hover {
  color: var(--err);
}

.title-input {
  flex: 1 1 180px;
  min-width: 140px;
  max-width: 320px;
}

.edit-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
  padding: var(--sp-4);
}
.editor {
  flex: 1;
  min-height: 0;
  font-size: var(--t-md);
  line-height: 1.9;
}
.editor :deep(textarea) {
  height: 100%;
}
.preview {
  flex: 1;
  min-height: 0;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  padding: var(--sp-4) var(--sp-5);
  font-size: var(--t-md);
}
</style>
