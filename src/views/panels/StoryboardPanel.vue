<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { NButton, NInput, NSelect, NInputNumber, NSwitch, NPopover, useDialog } from "naive-ui";
import { useProjectStore } from "@/stores/project";
import { api, errorText } from "@/api/ipc";
import { message } from "@/utils/notify";
import type { Shot, ShotStatus } from "@/types/models";

const project = useProjectStore();
const dialog = useDialog();

const chapterId = ref<string>("");
const vocab = ref({ shotSizes: [] as string[], cameraMoves: [] as string[], timeOfDay: [] as string[] });
const editing = ref<Shot | null>(null);

const chapters = computed(() => project.chapters);
const shots = computed(() =>
  project.shots
    .filter((s) => s.chapterId === chapterId.value)
    .slice()
    .sort((a, b) => a.index - b.index),
);
const totalDuration = computed(() => shots.value.reduce((a, s) => a + (s.durationSec || 0), 0));

const statusOptions: { label: string; value: ShotStatus }[] = [
  { label: "草稿", value: "draft" },
  { label: "就绪", value: "ready" },
  { label: "已写提示词", value: "prompted" },
  { label: "已出片", value: "generated" },
  { label: "锁定", value: "locked" },
];

onMounted(async () => {
  vocab.value = await api.storyboardVocab();
  if (chapters.value.length) chapterId.value = chapters.value[0].id;
});

function emptyShot(): Shot {
  return {
    id: "",
    chapterId: chapterId.value,
    index: shots.value.length + 1,
    sceneId: null,
    location: "",
    timeOfDay: "",
    interior: false,
    shotSize: "中景",
    camera: "",
    cameraMove: "固定",
    durationSec: 3,
    characters: [],
    props: [],
    action: "",
    dialogue: "",
    narration: "",
    sfx: "",
    bgm: "",
    imagePrompt: "",
    videoPrompt: "",
    refImages: [],
    status: "draft",
  };
}

function addShot() {
  editing.value = emptyShot();
}

function edit(shot: Shot) {
  editing.value = JSON.parse(JSON.stringify(shot));
}

async function saveShot() {
  if (!editing.value) return;
  if (!editing.value.action.trim()) {
    message.warning("画面动作不能为空");
    return;
  }
  try {
    await project.upsertShot(editing.value);
    editing.value = null;
  } catch (e) {
    message.error(errorText(e));
  }
}

function removeShot(shot: Shot) {
  dialog.warning({
    title: "删除镜头",
    content: `确定删除镜头 ${shot.index}？对应的视频提示词也会一起删掉。`,
    positiveText: "删除",
    negativeText: "取消",
    onPositiveClick: async () => {
      await project.deleteShot(shot.id);
      message.success("已删除");
    },
  });
}

async function move(shot: Shot, dir: -1 | 1) {
  const list = shots.value.map((s) => ({ ...s }));
  const i = list.findIndex((s) => s.id === shot.id);
  const j = i + dir;
  if (j < 0 || j >= list.length) return;
  [list[i], list[j]] = [list[j], list[i]];
  list.forEach((s, idx) => (s.index = idx + 1));
  await project.setShots(chapterId.value, list);
}

/** 从章节大纲粗略估算镜头数，生成占位行，方便 agent 在此基础上细化 */
async function draftFromChapter() {
  const ch = chapters.value.find((c) => c.id === chapterId.value);
  if (!ch) return;
  const n = Math.max(3, Math.min(12, Math.round((ch.wordCount || 600) / 120)));
  const list: Shot[] = [];
  for (let i = 0; i < n; i++) {
    list.push({
      ...emptyShot(),
      index: i + 1,
      shotSize: i === 0 ? "全景" : i % 3 === 0 ? "特写" : "中景",
      action: "",
      status: "draft",
    });
  }
  await project.setShots(chapterId.value, list);
  message.info(`已生成 ${n} 行空白镜头，可以逐条填，或让右侧 agent 直接写完整章分镜`);
}
</script>

<template>
  <div class="wrap">
    <div class="toolbar">
      <n-select
        v-model:value="chapterId"
        size="small"
        style="width: 200px"
        :options="chapters.map((c) => ({ label: `第${c.index}章 ${c.title}`, value: c.id }))"
        placeholder="选择章节"
      />
      <span class="tiny faint">
        {{ shots.length }} 个镜头 · 合计 {{ totalDuration.toFixed(1) }} 秒
      </span>
      <div class="grow" />
      <n-button size="small" quaternary @click="draftFromChapter">按大纲生成空行</n-button>
      <n-button size="small" @click="addShot">＋ 新增镜头</n-button>
    </div>

    <div class="pane scroll table">
      <div v-if="!shots.length" class="tiny faint empty">
        这一章还没有分镜。可以手动加，或让右侧 agent 写完整章镜头表（它会调用
        storyboard_write_shots 一次性写入）。
      </div>
      <div v-for="s in shots" :key="s.id" class="shot" :class="{ editing: editing?.id === s.id }">
        <div class="num mono">{{ s.index }}</div>
        <div class="col grow" style="gap: 2px; min-width: 0">
          <div class="row wrap" style="gap: 4px">
            <span class="tag accent">{{ s.shotSize }}</span>
            <span class="tag">{{ s.cameraMove }}</span>
            <span v-if="s.timeOfDay" class="tag">{{ s.timeOfDay }}</span>
            <span v-if="s.location" class="tag">{{ s.location }}</span>
            <span class="tag">{{ s.durationSec }}s</span>
            <span v-if="s.characters.length" class="tag">
              {{ s.characters.join("、") }}
            </span>
          </div>
          <div class="truncate" :title="s.action">{{ s.action || "（未填画面动作）" }}</div>
          <div v-if="s.dialogue" class="tiny faint truncate">台词：{{ s.dialogue }}</div>
        </div>
        <div class="row" style="gap: 0; flex: 0 0 auto">
          <button class="mini" @click="move(s, -1)">↑</button>
          <button class="mini" @click="move(s, 1)">↓</button>
          <n-button size="tiny" quaternary @click="edit(s)">编辑</n-button>
          <n-button size="tiny" quaternary @click="removeShot(s)">删</n-button>
        </div>
      </div>
    </div>

    <!-- 编辑抽屉 -->
    <div v-if="editing" class="pane editor">
      <div class="row-between head">
        <span style="font-weight: 600">
          {{ editing.id ? `编辑镜头 ${editing.index}` : "新增镜头" }}
        </span>
        <div class="row" style="gap: 4px">
          <n-button size="tiny" quaternary @click="editing = null">取消</n-button>
          <n-button size="tiny" type="primary" @click="saveShot">保存</n-button>
        </div>
      </div>
      <div class="scroll body">
        <div class="grid3">
          <label>景别
            <n-select v-model:value="editing.shotSize" size="small" tag filterable :options="vocab.shotSizes.map((v) => ({ label: v, value: v }))" />
          </label>
          <label>运镜
            <n-select v-model:value="editing.cameraMove" size="small" tag filterable :options="vocab.cameraMoves.map((v) => ({ label: v, value: v }))" />
          </label>
          <label>时长（秒）
            <n-input-number v-model:value="editing.durationSec" size="small" :min="0.5" :max="20" :step="0.5" />
          </label>
          <label>时间
            <n-select v-model:value="editing.timeOfDay" size="small" tag filterable :options="vocab.timeOfDay.map((v) => ({ label: v, value: v }))" />
          </label>
          <label>地点
            <n-input v-model:value="editing.location" size="small" placeholder="如 总裁办公室" />
          </label>
          <label>内景
            <div><n-switch v-model:value="editing.interior" size="small" /></div>
          </label>
        </div>

        <label>机位描述
          <n-input v-model:value="editing.camera" size="small" placeholder="如 低角度仰拍" />
        </label>
        <label>出场人物（逗号分隔，用资产里的名字）
          <n-select
            v-model:value="editing.characters"
            multiple filterable tag size="small"
            :options="project.assets.map((a) => ({ label: a.name, value: a.name }))"
          />
        </label>
        <label>涉及道具
          <n-select
            v-model:value="editing.props"
            multiple filterable tag size="small"
            :options="project.assets.filter((a) => a.kind !== 'character').map((a) => ({ label: a.name, value: a.name }))"
          />
        </label>
        <label>画面动作
          <n-input v-model:value="editing.action" type="textarea" :autosize="{ minRows: 2, maxRows: 4 }" placeholder="这一镜画面里发生了什么" />
        </label>
        <label>台词
          <n-input v-model:value="editing.dialogue" type="textarea" :autosize="{ minRows: 1, maxRows: 3 }" />
        </label>
        <label>画外音
          <n-input v-model:value="editing.narration" size="small" />
        </label>
        <div class="grid2">
          <label>音效
            <n-input v-model:value="editing.sfx" size="small" />
          </label>
          <label>配乐
            <n-input v-model:value="editing.bgm" size="small" />
          </label>
        </div>
        <label>静态画面提示词（可选）
          <n-input v-model:value="editing.imagePrompt" type="textarea" :autosize="{ minRows: 2, maxRows: 4 }" />
        </label>
        <label>状态
          <n-select v-model:value="editing.status" size="small" :options="statusOptions" />
        </label>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wrap {
  display: flex;
  gap: 12px;
  height: 100%;
  min-height: 0;
  flex-wrap: wrap;
}
.toolbar {
  flex: 0 0 100%;
  display: flex;
  align-items: center;
  gap: 10px;
  height: 34px;
}
.table {
  flex: 1;
  min-width: 420px;
  min-height: 0;
  padding: 6px;
}
.shot {
  display: flex;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 6px;
  border-left: 2px solid transparent;
}
.shot:hover {
  background: var(--bg-3);
}
.shot.editing {
  border-left-color: var(--accent);
  background: var(--accent-soft);
}
.num {
  width: 22px;
  flex: 0 0 auto;
  color: var(--text-faint);
  font-size: 12px;
  padding-top: 2px;
}
.editor {
  width: 420px;
  flex: 0 0 420px;
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.head {
  padding: 8px 10px;
  border-bottom: 1px solid var(--line-soft);
}
.body {
  flex: 1;
  min-height: 0;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.body label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: var(--text-dim);
}
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.grid3 {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 10px;
}
.grid3 label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: var(--text-dim);
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
.empty {
  padding: 24px;
  text-align: center;
  line-height: 1.9;
}
</style>
