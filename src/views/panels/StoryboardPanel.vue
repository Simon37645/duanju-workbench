<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Clock, Film, Plus, Sparkles, Trash2, Wand2 } from "@lucide/vue";
import { useProjectStore } from "@/stores/project";
import { useAgentStore } from "@/stores/agent";
import { api, errorText } from "@/api/ipc";
import { toast, confirmDialog } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiInput from "@/ui/Input.vue";
import UiNumber from "@/ui/NumberInput.vue";
import UiTextarea from "@/ui/Textarea.vue";
import UiSelect from "@/ui/Select.vue";
import UiField from "@/ui/Field.vue";
import UiSwitch from "@/ui/Switch.vue";
import UiEmpty from "@/ui/Empty.vue";
import UiSegmented from "@/ui/Segmented.vue";
import type { Shot, ShotStatus } from "@/types/models";

const project = useProjectStore();
const agent = useAgentStore();

const chapterId = ref("all");
const editing = ref<Shot | null>(null);
const statusFilter = ref("all");
const vocab = ref<{ shotSizes: string[]; cameraMoves: string[]; timeOfDay: string[] }>({
  shotSizes: [], cameraMoves: [], timeOfDay: [],
});

const chapters = computed(() => project.chapters);
/** "all" = 跨章节总览；否则只看某一章 */
const allShots = computed(() =>
  project.shots
    .filter((s) => chapterId.value === "all" || s.chapterId === chapterId.value)
    .slice()
    .sort((a, b) => a.index - b.index),
);
/** 镜头所属章节的短标签（总览模式下显示） */
function chapterLabel(id: string): string {
  const c = chapters.value.find((x) => x.id === id);
  return c ? `第${c.index}章` : "";
}
const shots = computed(() =>
  statusFilter.value === "all" ? allShots.value : allShots.value.filter((s) => s.status === statusFilter.value),
);
const total = computed(() => allShots.value.reduce((a, s) => a + (s.durationSec || 0), 0));

const STATUS: Record<ShotStatus, { label: string; tone: "neutral" | "warn" | "ok" | "accent" | "info" }> = {
  draft: { label: "草稿", tone: "neutral" },
  ready: { label: "就绪", tone: "info" },
  prompted: { label: "有提示词", tone: "warn" },
  generated: { label: "已出片", tone: "ok" },
  locked: { label: "锁定", tone: "accent" },
};
const statusOptions = (Object.keys(STATUS) as ShotStatus[]).map((k) => ({ label: STATUS[k].label, value: k }));

onMounted(async () => {
  vocab.value = await api.storyboardVocab();
  await project.ensure();
  const first = allShots.value[0];
  if (first) edit(first);
});

function blank(): Shot {
  return {
    id: "", chapterId: chapterId.value, index: allShots.value.length + 1, sceneId: null,
    location: "", timeOfDay: "白天", interior: true, shotSize: "中景", camera: "平视",
    cameraMove: "固定", durationSec: 3, characters: [], props: [], action: "",
    dialogue: "", narration: "", sfx: "", bgm: "", imagePrompt: "", videoPrompt: "",
    refImages: [], status: "draft",
  };
}

function edit(s: Shot) {
  // 总览模式下点某条：自动跟到它所在的章节，方便直接编辑保存
  if (chapterId.value === "all") chapterId.value = s.chapterId;
  editing.value = JSON.parse(JSON.stringify(s));
}

async function saveShot() {
  if (!editing.value) return;
  if (!editing.value.action.trim()) return toast.warn("画面动作不能为空");
  try {
    const isNew = !editing.value.id;
    await project.upsertShot(editing.value);
    toast.ok(isNew ? "镜头已新增" : "镜头已更新");
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function remove(s: Shot) {
  const ok = await confirmDialog({
    title: "删除镜头",
    content: `确定删除镜头 ${s.index}？对应的视频提示词会一起删掉。`,
    positiveText: "删除",
    danger: true,
  });
  if (!ok) return;
  await project.deleteShot(s.id);
  if (editing.value?.id === s.id) editing.value = null;
}

async function move(s: Shot, dir: -1 | 1) {
  const list = allShots.value.map((x) => ({ ...x }));
  const i = list.findIndex((x) => x.id === s.id);
  const j = i + dir;
  if (j < 0 || j >= list.length) return;
  [list[i], list[j]] = [list[j], list[i]];
  list.forEach((x, idx) => (x.index = idx + 1));
  await project.setShots(chapterId.value, list);
}

async function draftRows() {
  const ch = chapters.value.find((c) => c.id === chapterId.value);
  if (!ch) return;
  const n = Math.max(3, Math.min(12, Math.round((ch.wordCount || 600) / 120)));
  const list: Shot[] = Array.from({ length: n }, (_, i) => ({
    ...blank(),
    index: i + 1,
    shotSize: i === 0 ? "全景" : i % 3 === 0 ? "特写" : "中景",
  }));
  await project.setShots(chapterId.value, list);
  toast.info(`已生成 ${n} 行空白镜头，或让右侧 agent 直接写完整章`);
}

function askAgent() {
  const ch = chapters.value.find((c) => c.id === chapterId.value);
  if (!ch) return toast.warn("先在左上角选一个章节");
  agent.send("storyboard", `帮我把「${ch.title}」这一章写完整的分镜，用 storyboard_write_shots 一次写入。`);
}
</script>

<template>
  <div class="layout">
    <section class="panel shots-panel">
      <header class="panel-head">
        <UiSelect
          :model-value="chapterId"
          :options="[
            { label: `全部章节（共 ${project.shots.length} 个镜头）`, value: 'all' },
            ...chapters.map((c) => ({ label: `第${c.index}章 ${c.title}`, value: c.id })),
          ]"
          style="flex: 1; min-width: 0"
          @update:model-value="(v: string | null) => { chapterId = v ?? 'all'; editing = null; }"
        />
        <div class="row" style="gap: 4px; flex: 0 0 auto">
          <UiBadge tone="neutral" size="xs"><Film :size="9" /> {{ allShots.length }}</UiBadge>
          <UiBadge tone="neutral" size="xs"><Clock :size="9" /> {{ total.toFixed(1) }}s</UiBadge>
        </div>
      </header>

      <div class="filterbar">
        <UiSegmented
          v-model="statusFilter"
          size="xs"
          :items="[
            { label: '全部', value: 'all' },
            { label: '草稿', value: 'draft' },
            { label: '就绪', value: 'ready' },
          ]"
        />
        <span class="grow" />
        <UiButton
          variant="subtle"
          size="xs"
          :disabled="chapterId === 'all'"
          :title="chapterId === 'all' ? '总览模式下先在左上角选一个章节' : ''"
          @click="draftRows"
        >
          生成空行
        </UiButton>
        <UiButton
          variant="default"
          size="xs"
          :disabled="chapterId === 'all'"
          :title="chapterId === 'all' ? '总览模式下先在左上角选一个章节再新增' : ''"
          @click="editing = blank()"
        >
          <template #icon><Plus :size="12" /></template>
          新增
        </UiButton>
      </div>

      <div class="list scroll">
        <div
          v-for="s in shots"
          :key="s.id"
          class="shot"
          :class="{ on: editing?.id === s.id }"
          @click="edit(s)"
        >
          <div class="sidx num">{{ s.index }}</div>
          <div class="col grow" style="gap: 3px; min-width: 0">
            <div class="row wrap" style="gap: 4px">
              <UiBadge v-if="chapterId === 'all'" tone="info" size="xs">{{ chapterLabel(s.chapterId) }}</UiBadge>
              <UiBadge tone="accent" size="xs">{{ s.shotSize }}</UiBadge>
              <UiBadge tone="neutral" size="xs">{{ s.cameraMove }}</UiBadge>
              <UiBadge tone="neutral" size="xs">{{ s.durationSec }}s</UiBadge>
              <span v-if="s.location" class="t-xs faint">{{ s.location }}</span>
              <span v-if="s.characters.length" class="t-xs faint">· {{ s.characters.join("、") }}</span>
            </div>
            <div class="saction truncate">{{ s.action || "（未填画面动作）" }}</div>
            <div v-if="s.dialogue" class="t-xs faint truncate">「{{ s.dialogue }}」</div>
          </div>
          <div class="col" style="gap: 4px; align-items: flex-end; flex: 0 0 auto">
            <UiBadge :tone="STATUS[s.status].tone" size="xs">{{ STATUS[s.status].label }}</UiBadge>
            <div class="ops">
              <button class="op" @click.stop="move(s, -1)">↑</button>
              <button class="op" @click.stop="move(s, 1)">↓</button>
              <button class="op danger" @click.stop="remove(s)"><Trash2 :size="11" /></button>
            </div>
          </div>
        </div>

        <UiEmpty
          v-if="!shots.length"
          :title="chapterId === 'all' ? '还没有任何镜头' : '这一章还没有分镜'"
          hint="按大纲生成空行逐条填，或者直接让 agent 写完整章镜头表"
        >
          <template #icon><Film :size="26" /></template>
          <UiButton variant="outline" size="sm" :disabled="chapterId === 'all'" @click="askAgent">
            <template #icon><Wand2 :size="13" /></template>
            让 agent 写这一章
          </UiButton>
        </UiEmpty>
      </div>
    </section>

    <section class="panel editor-panel">
      <template v-if="editing">
        <header class="panel-head">
          <span class="section-label">{{ editing.id ? `镜头 ${editing.index}` : "新增镜头" }}</span>
          <div class="row" style="gap: 4px">
            <UiButton variant="ghost" size="xs" @click="editing = null">关闭</UiButton>
            <UiButton variant="primary" size="sm" @click="saveShot">保存</UiButton>
          </div>
        </header>

        <div class="form scroll">
          <div class="grid3">
            <UiField label="景别">
              <UiSelect v-model="editing.shotSize" creatable :options="vocab.shotSizes.map((v) => ({ label: v, value: v }))" />
            </UiField>
            <UiField label="运镜">
              <UiSelect v-model="editing.cameraMove" creatable :options="vocab.cameraMoves.map((v) => ({ label: v, value: v }))" />
            </UiField>
            <UiField label="时长">
              <UiNumber v-model="editing.durationSec" :min="0.5" :max="20" :step="0.5" suffix="s" />
            </UiField>
          </div>

          <div class="grid3">
            <UiField label="时间">
              <UiSelect v-model="editing.timeOfDay" creatable :options="vocab.timeOfDay.map((v) => ({ label: v, value: v }))" />
            </UiField>
            <UiField label="地点">
              <UiInput v-model="editing.location" placeholder="办公室" />
            </UiField>
            <UiField label="内景">
              <UiSwitch v-model="editing.interior" />
            </UiField>
          </div>

          <UiField label="机位">
            <UiInput v-model="editing.camera" placeholder="低角度仰拍" />
          </UiField>

          <UiField label="出场人物">
            <UiSelect
              :model-value="editing.characters[0] ?? null"
              :options="project.assets.map((a) => ({ label: a.name, value: a.name }))"
              clearable
              placeholder="选一个（或让 agent 批量填）"
              @update:model-value="(v: string | null) => (editing!.characters = v ? [v] : [])"
            />
          </UiField>

          <UiField label="画面动作">
            <UiTextarea v-model="editing.action" :rows="3" placeholder="这一镜画面里发生了什么" />
          </UiField>

          <UiField label="台词">
            <UiTextarea v-model="editing.dialogue" :rows="2" placeholder="角色名：内容" />
          </UiField>

          <UiField label="画外音"><UiInput v-model="editing.narration" /></UiField>

          <div class="grid2">
            <UiField label="音效"><UiInput v-model="editing.sfx" /></UiField>
            <UiField label="配乐"><UiInput v-model="editing.bgm" /></UiField>
          </div>

          <UiField label="静态画面提示词" hint="可选">
            <UiTextarea v-model="editing.imagePrompt" :rows="2" />
          </UiField>

          <UiField label="状态">
            <UiSelect v-model="editing.status" :options="statusOptions" />
          </UiField>
        </div>
      </template>

      <UiEmpty v-else title="选一个镜头" hint="从左侧点一个镜头，或在右上新增">
        <template #icon><Sparkles :size="26" /></template>
      </UiEmpty>
    </section>
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
.shots-panel {
  flex: 1;
  min-width: 0;
}
.editor-panel {
  width: 340px;
  flex: 0 0 340px;
}
.filterbar {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--line-faint);
}
.list {
  flex: 1;
  min-height: 0;
  padding: var(--sp-2);
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.shot {
  display: flex;
  gap: 10px;
  padding: 9px 10px;
  border-radius: var(--r);
  border: 1px solid transparent;
  cursor: pointer;
  transition: background var(--fast), border-color var(--fast);
}
.shot:hover {
  background: var(--surface-3);
}
.shot.on {
  background: var(--surface-3);
  border-color: var(--line-strong);
}
.sidx {
  width: 20px;
  flex: 0 0 auto;
  font-size: var(--t-xs);
  color: var(--fg-ghost);
  text-align: right;
  padding-top: 3px;
}
.saction {
  font-size: var(--t-base);
}
.ops {
  display: flex;
  gap: 2px;
  opacity: 0;
  transition: opacity var(--fast);
}
.shot:hover .ops {
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
.form {
  flex: 1;
  min-height: 0;
  padding: var(--sp-4);
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--sp-3);
}
.grid3 {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: var(--sp-2);
}
</style>
