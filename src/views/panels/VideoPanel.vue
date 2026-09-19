<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { NButton, NSelect, NProgress, NPopover, NEmpty } from "naive-ui";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import { useJobsStore } from "@/stores/jobs";
import { api, errorText } from "@/api/ipc";
import { fileUrl, fmtDuration } from "@/api/events";
import { message } from "@/utils/notify";
import type { Shot, VideoTake } from "@/types/models";

const project = useProjectStore();
const settings = useSettingsStore();
const jobs = useJobsStore();

const chapterId = ref<string>("");
const selectedShotId = ref<string | null>(null);
const previewTake = ref<VideoTake | null>(null);

const chapters = computed(() => project.chapters);
const shots = computed(() =>
  project.shots
    .filter((s) => !chapterId.value || s.chapterId === chapterId.value)
    .slice()
    .sort((a, b) => a.index - b.index),
);

const stats = computed(() => {
  const done = shots.value.filter((s) =>
    project.takesOf(s.id).some((t: VideoTake) => t.status === "done"),
  ).length;
  return { total: shots.value.length, done };
});

const providerOptions = computed(() =>
  settings.byKind("video").map((p) => ({ label: `${p.name} · ${p.model}`, value: p.id })),
);

onMounted(() => {
  if (chapters.value.length) chapterId.value = chapters.value[0].id;
  const s = shots.value[0];
  if (s) selectedShotId.value = s.id;
});

function takesOf(shotId: string) {
  return project.takesOf(shotId).slice().sort((a: VideoTake, b: VideoTake) => b.createdAt.localeCompare(a.createdAt));
}

function readiness(shot: Shot): { ok: boolean; problems: string[] } {
  const problems: string[] = [];
  const p = project.promptOf(shot.id);
  if (!p) problems.push("还没有视频提示词");
  else {
    if (!p.prompt.trim()) problems.push("提示词为空");
    if (!p.firstFrame) problems.push("没配首帧图");
  }
  return { ok: problems.length === 0, problems };
}

async function generate(shotIds: string[]) {
  if (!settings.activeVideo) {
    message.warning("还没有启用视频 provider，去设置里加一个（也可以先选「占位生成」跑通流程）");
    return;
  }
  try {
    const ids = await api.videoGenerate(shotIds, settings.settings?.activeVideoProviderId ?? undefined);
    message.info(`已提交 ${ids.length} 个生成任务`);
  } catch (e) {
    message.error(errorText(e));
  }
}

async function generateMissing() {
  const todo = shots.value.filter(
    (s) => !project.takesOf(s.id).some((t: VideoTake) => t.status === "done"),
  );
  if (!todo.length) {
    message.info("这一章的镜头都出片了");
    return;
  }
  await generate(todo.map((s) => s.id));
}

async function removeTake(t: VideoTake) {
  await api.videoDeleteTake(t.id);
  await project.reload();
  if (previewTake.value?.id === t.id) previewTake.value = null;
}

function takeJob(t: VideoTake) {
  return jobs.byId(t.jobId);
}

const currentTakes = computed(() =>
  selectedShotId.value ? takesOf(selectedShotId.value) : [],
);
</script>

<template>
  <div class="wrap">
    <div class="pane col-list">
      <div class="head col" style="gap: 6px">
        <div class="row-between">
          <span style="font-weight: 600">镜头出片</span>
          <n-button size="tiny" type="primary" @click="generateMissing">批量生成缺失</n-button>
        </div>
        <div class="row" style="gap: 6px">
          <n-select
            v-model:value="chapterId"
            size="tiny"
            style="width: 130px"
            clearable
            placeholder="全部章节"
            :options="chapters.map((c) => ({ label: `第${c.index}章`, value: c.id }))"
          />
          <span class="tiny faint">{{ stats.done }}/{{ stats.total }} 已出片</span>
        </div>
        <n-select
          v-if="providerOptions.length"
          size="tiny"
          :value="settings.settings?.activeVideoProviderId ?? undefined"
          :options="providerOptions"
          placeholder="视频 provider"
          @update:value="(v: string) => settings.save({ activeVideoProviderId: v })"
        />
      </div>

      <div class="scroll list">
        <div
          v-for="s in shots"
          :key="s.id"
          class="item"
          :class="{ active: s.id === selectedShotId }"
          @click="selectedShotId = s.id"
        >
          <div class="row-between">
            <div class="row" style="gap: 6px; min-width: 0">
              <span class="mono tiny faint">{{ s.index }}</span>
              <span class="truncate">{{ s.action || "（无描述）" }}</span>
            </div>
            <span
              class="tag"
              :class="takesOf(s.id).some((t: VideoTake) => t.status === 'done') ? 'ok' : readiness(s).ok ? 'warn' : ''"
            >
              {{
                takesOf(s.id).some((t) => t.status === "done")
                  ? "已出片"
                  : readiness(s).ok
                    ? "待生成"
                    : "缺条件"
              }}
            </span>
          </div>
          <div v-if="!readiness(s).ok" class="tiny faint truncate">
            缺：{{ readiness(s).problems.join("、") }}
          </div>
        </div>
        <div v-if="!shots.length" class="tiny faint" style="padding: 16px; text-align: center">
          还没有镜头
        </div>
      </div>
    </div>

    <div class="pane col-detail">
      <template v-if="selectedShotId">
        <div class="head row-between">
          <span style="font-weight: 600">
            镜头 {{ project.shots.find((s) => s.id === selectedShotId)?.index }} 的成片
          </span>
          <div class="row" style="gap: 4px">
            <n-button size="tiny" quaternary @click="generate([selectedShotId!])">再出一条</n-button>
          </div>
        </div>
        <div class="scroll body">
          <div v-if="!currentTakes.length" class="tiny faint" style="padding: 24px; text-align: center">
            还没有生成记录。点右上「再出一条」提交一条，或用左侧「批量生成缺失」。
          </div>
          <div class="takes">
            <div v-for="t in currentTakes" :key="t.id" class="take">
              <div class="video" @click="t.file && (previewTake = t)">
                <video v-if="t.file" :src="fileUrl(t.file)" preload="metadata" muted />
                <div v-else class="ph tiny faint">
                  {{ t.status === "running" ? "生成中…" : t.status === "failed" ? "失败" : t.status }}
                </div>
                <span class="tag" :class="t.status === 'done' ? 'ok' : t.status === 'failed' ? 'err' : ''">
                  {{ t.status }}
                </span>
              </div>
              <n-progress
                v-if="takeJob(t)?.status === 'running'"
                type="line"
                :percentage="Math.round((takeJob(t)!.progress) * 100)"
                :height="3"
                :show-indicator="false"
              />
              <div class="row-between">
                <span class="tiny faint">{{ t.model }} · {{ t.durationSec?.toFixed(1) ?? "?" }}s</span>
                <n-button size="tiny" quaternary @click="removeTake(t)">删</n-button>
              </div>
              <div v-if="t.error" class="tiny err">{{ t.error }}</div>
            </div>
          </div>
        </div>
      </template>
      <div v-else class="center">
        <span class="tiny faint">从左侧选一个镜头</span>
      </div>
    </div>

    <!-- 预览 -->
    <div v-if="previewTake" class="preview" @click.self="previewTake = null">
      <div class="pane box">
        <div class="row-between head">
          <span>{{ previewTake.model }}</span>
          <n-button size="tiny" quaternary @click="previewTake = null">关闭</n-button>
        </div>
        <video :src="fileUrl(previewTake.file)" controls autoplay class="big" />
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
}
.col-list {
  width: 330px;
  flex: 0 0 330px;
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.col-detail {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.col-detail .center {
  flex: 1;
  display: grid;
  place-items: center;
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
.item {
  padding: 7px 8px;
  border-radius: 6px;
  cursor: pointer;
  border-left: 2px solid transparent;
}
.item:hover {
  background: var(--bg-3);
}
.item.active {
  background: var(--accent-soft);
  border-left-color: var(--accent);
}
.body {
  flex: 1;
  min-height: 0;
  padding: 12px;
}
.takes {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 10px;
}
.take {
  display: flex;
  flex-direction: column;
  gap: 5px;
  background: var(--bg-1);
  border: 1px solid var(--line);
  border-radius: 7px;
  padding: 7px;
}
.video {
  position: relative;
  aspect-ratio: 9 / 16;
  background: #0b0b10;
  border-radius: 5px;
  overflow: hidden;
  cursor: pointer;
  display: grid;
  place-items: center;
}
.video video {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.video .tag {
  position: absolute;
  top: 5px;
  left: 5px;
}
.preview {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.75);
  display: grid;
  place-items: center;
  z-index: 100;
}
.box {
  padding: 10px;
  max-width: 80vw;
  max-height: 88vh;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.big {
  max-height: 78vh;
  max-width: 76vw;
  border-radius: 6px;
}
.err {
  color: var(--err);
}
</style>
