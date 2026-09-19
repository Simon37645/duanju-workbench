<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { NButton, NInput, NInputNumber, NSelect, NPopover, NModal, useDialog } from "naive-ui";
import { useProjectStore } from "@/stores/project";
import { api, errorText } from "@/api/ipc";
import { message } from "@/utils/notify";
import Markdown from "@/components/Markdown.vue";
import type { ChapterStatus } from "@/types/models";

const project = useProjectStore();
const dialog = useDialog();

const currentId = ref<string | null>(null);
const content = ref("");
const title = ref("");
const summary = ref("");
const preview = ref(false);
const dirty = ref(false);
const showCount = ref(false);
const wantCount = ref(6);

const chapters = computed(() => project.chapters);
const current = computed(() => chapters.value.find((c) => c.id === currentId.value) ?? null);

const statusLabel: Record<ChapterStatus, string> = {
  empty: "空",
  draft: "草稿",
  written: "已写",
  locked: "锁定",
};
const statusClass: Record<ChapterStatus, string> = {
  empty: "",
  draft: "warn",
  written: "ok",
  locked: "accent",
};

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
    message.success("已保存");
  } catch (e) {
    message.error(`保存失败：${errorText(e)}`);
  }
}

async function addChapter() {
  const meta = await project.addChapter();
  await select(meta.id);
}

function removeChapter(id: string) {
  const ch = chapters.value.find((c) => c.id === id);
  dialog.warning({
    title: "删除章节",
    content: `确定删除「${ch?.title}」？该章的分镜与提示词也会一并删除，正文文件会被移除。`,
    positiveText: "删除",
    negativeText: "取消",
    onPositiveClick: async () => {
      await project.deleteChapter(id);
      if (currentId.value === id) {
        currentId.value = null;
        content.value = "";
        if (chapters.value.length) await select(chapters.value[0].id);
      }
      message.success("已删除");
    },
  });
}

async function applyCount() {
  const want = Math.max(1, wantCount.value);
  const have = chapters.value.length;
  if (want > have) {
    for (let i = have; i < want; i++) await project.addChapter();
    message.success(`已追加 ${want - have} 章`);
  } else if (want < have) {
    message.warning(`当前已有 ${have} 章，减少章节请手动删除，避免误删正文`);
  }
  showCount.value = false;
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
</script>

<template>
  <div class="wrap2">
    <div class="pane col-list">
      <div class="row-between head">
        <span style="font-weight: 600">章节（{{ chapters.length }}）</span>
        <div class="row" style="gap: 2px">
          <n-button size="tiny" quaternary @click="showCount = true">设定章数</n-button>
          <n-button size="tiny" quaternary @click="addChapter">＋</n-button>
        </div>
      </div>
      <div class="scroll list">
        <div
          v-for="c in chapters"
          :key="c.id"
          class="chap"
          :class="{ active: c.id === currentId }"
          @click="select(c.id)"
        >
          <div class="row-between">
            <div class="row" style="gap: 6px; min-width: 0">
              <span class="idx mono">{{ c.index }}</span>
              <span class="truncate" style="font-weight: 500">{{ c.title }}</span>
            </div>
            <span class="tag" :class="statusClass[c.status]">{{ statusLabel[c.status] }}</span>
          </div>
          <div class="tiny faint truncate" style="margin-left: 24px">{{ c.summary || "（无大纲）" }}</div>
          <div class="row-between" style="margin-left: 24px; margin-top: 2px">
            <span class="tiny faint">{{ c.wordCount }} 字</span>
            <div class="row" style="gap: 0">
              <button class="mini" @click.stop="move(c.id, -1)">↑</button>
              <button class="mini" @click.stop="move(c.id, 1)">↓</button>
              <button class="mini" @click.stop="removeChapter(c.id)">×</button>
            </div>
          </div>
        </div>
        <div v-if="!chapters.length" class="tiny faint" style="padding: 16px; text-align: center">
          还没有章节。<br />点右上「设定章数」，或直接让右侧 agent 生成章节骨架。
        </div>
      </div>
    </div>

    <div class="pane col-edit">
      <template v-if="current">
        <div class="row head" style="gap: 8px">
          <n-input
            v-model:value="title"
            size="small"
            placeholder="章节标题"
            style="max-width: 260px"
            @update:value="dirty = true"
          />
          <n-select
            size="small"
            style="width: 108px"
            :value="current.status"
            :options="[
              { label: '空', value: 'empty' },
              { label: '草稿', value: 'draft' },
              { label: '已写', value: 'written' },
              { label: '锁定', value: 'locked' },
            ]"
            @update:value="(v: ChapterStatus) => project.updateChapterMeta(current!.id, { status: v })"
          />
          <div class="grow" />
          <span class="tiny faint">{{ content.length }} 字</span>
          <n-button size="tiny" quaternary @click="preview = !preview">
            {{ preview ? "编辑" : "预览" }}
          </n-button>
          <n-button size="tiny" type="primary" :disabled="!dirty" @click="save">
            {{ dirty ? "保存" : "已保存" }}
          </n-button>
        </div>

        <div class="col grow" style="min-height: 0; padding: 10px 12px">
          <n-input
            v-model:value="summary"
            size="small"
            type="textarea"
            :autosize="{ minRows: 2, maxRows: 4 }"
            placeholder="本章大纲（会喂给分镜与资产面板，写清出场人物与关键道具）"
            @update:value="dirty = true"
          />
          <Markdown v-if="preview" :text="content" class="editor md-preview scroll" />
          <n-input
            v-else
            v-model:value="content"
            type="textarea"
            class="editor"
            placeholder="章节正文（Markdown）。短剧节奏：开场抓人、中间反转、结尾留钩子。"
            @update:value="dirty = true"
            @keydown.ctrl.s.prevent="save"
            @keydown.meta.s.prevent="save"
          />
        </div>
      </template>
      <div v-else class="empty">
        <div class="tiny faint">从左侧选一章，或新建一章开始写</div>
      </div>
    </div>

    <n-modal v-model:show="showCount" preset="dialog" title="设定章节数" style="width: 380px">
      <div class="col" style="gap: 10px">
        <div class="tiny dim">当前 {{ chapters.length }} 章。增加会追加空白章节，减少请手动删除。</div>
        <n-input-number v-model:value="wantCount" :min="1" :max="200" />
      </div>
      <template #action>
        <n-button size="small" @click="showCount = false">取消</n-button>
        <n-button size="small" type="primary" @click="applyCount">确定</n-button>
      </template>
    </n-modal>
  </div>
</template>

<style scoped>
.wrap2 {
  display: flex;
  gap: 12px;
  height: 100%;
  min-height: 0;
}
.col-list {
  width: 300px;
  flex: 0 0 300px;
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.col-edit {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.head {
  padding: 8px 10px;
  border-bottom: 1px solid var(--line-soft);
  flex: 0 0 auto;
}
.list {
  flex: 1;
  min-height: 0;
  padding: 6px;
}
.chap {
  padding: 7px 8px;
  border-radius: 6px;
  cursor: pointer;
  border-left: 2px solid transparent;
}
.chap:hover {
  background: var(--bg-3);
}
.chap.active {
  background: var(--accent-soft);
  border-left-color: var(--accent);
}
.idx {
  width: 16px;
  text-align: right;
  color: var(--text-faint);
  font-size: 11px;
}
.mini {
  background: none;
  border: none;
  color: var(--text-faint);
  cursor: pointer;
  font-size: 11px;
  padding: 0 3px;
}
.mini:hover {
  color: var(--text);
}
.editor {
  flex: 1;
  min-height: 0;
}
.editor :deep(textarea) {
  height: 100% !important;
  font-family: "PingFang SC", "Microsoft YaHei", sans-serif;
  line-height: 1.9;
  font-size: 13.5px;
}
.md-preview {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--bg-1);
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 14px 18px;
}
.empty {
  flex: 1;
  display: grid;
  place-items: center;
}
</style>
