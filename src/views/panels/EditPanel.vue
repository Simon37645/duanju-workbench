<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { NButton, NInputNumber, NSwitch, NSelect, NProgress, NPopover } from "naive-ui";
import { useProjectStore } from "@/stores/project";
import { useJobsStore } from "@/stores/jobs";
import { api, errorText } from "@/api/ipc";
import { fileUrl, fmtDuration, baseName } from "@/api/events";
import { message } from "@/utils/notify";
import type { Clip, Timeline, Track } from "@/types/models";

const project = useProjectStore();
const jobs = useJobsStore();

const selected = ref<{ trackId: string; clipId: string } | null>(null);
const burnSubs = ref(true);
const local = ref<Timeline | null>(null);
const previewClip = ref<Clip | null>(null);

const tl = computed(() => local.value ?? project.timeline);

const totalDuration = computed(() => {
  const t = tl.value;
  if (!t) return 0;
  return t.tracks
    .flatMap((x) => x.clips)
    .reduce((m, c) => Math.max(m, c.startSec + (c.outSec - c.inSec) / Math.max(c.speed, 0.01)), 0);
});

const trackKindLabel: Record<string, string> = {
  video: "视频",
  audio: "音频",
  subtitle: "字幕",
  overlay: "叠加",
};

onMounted(() => {
  if (project.timeline) local.value = JSON.parse(JSON.stringify(project.timeline));
});

function syncFromStore() {
  if (project.timeline) local.value = JSON.parse(JSON.stringify(project.timeline));
}

async function persist() {
  if (!local.value) return;
  try {
    await api.editSetTimeline(JSON.parse(JSON.stringify(local.value)));
    await project.reload();
    syncFromStore();
  } catch (e) {
    message.error(errorText(e));
  }
}

async function buildFromTakes() {
  try {
    const n = await api.editBuildFromTakes();
    await project.reload();
    syncFromStore();
    message.success(`已铺入 ${n} 段`);
  } catch (e) {
    message.error(errorText(e));
  }
}

async function render() {
  try {
    await api.editRender(burnSubs.value);
    message.info("已开始导出，完成后在底部任务栏可见");
  } catch (e) {
    message.error(errorText(e));
  }
}

function clipStyle(track: Track, clip: Clip) {
  const total = Math.max(totalDuration.value, 1);
  const dur = (clip.outSec - clip.inSec) / Math.max(clip.speed, 0.01);
  return {
    left: `${(clip.startSec / total) * 100}%`,
    width: `${Math.max(0.6, (dur / total) * 100)}%`,
    opacity: clip.enabled ? 1 : 0.35,
  };
}

function pick(trackId: string, clip: Clip) {
  selected.value = { trackId, clipId: clip.id };
}

const currentClip = computed<{ track: Track; clip: Clip } | null>(() => {
  const sel = selected.value;
  if (!sel || !tl.value) return null;
  const track = tl.value.tracks.find((t) => t.id === sel.trackId);
  const clip = track?.clips.find((c) => c.id === sel.clipId);
  return track && clip ? { track, clip } : null;
});

async function removeClip() {
  const sel = selected.value;
  if (!sel || !local.value) return;
  const track = local.value.tracks.find((t) => t.id === sel.trackId);
  if (!track) return;
  track.clips = track.clips.filter((c) => c.id !== sel.clipId);
  selected.value = null;
  await persist();
}

async function updateClips(trackId: string, fn: (clips: Clip[]) => Clip[]) {
  if (!local.value) return;
  const track = local.value.tracks.find((t) => t.id === trackId);
  if (!track) return;
  track.clips = fn(track.clips);
  await persist();
}

function patchCurrent(patch: Partial<Clip>) {
  const c = currentClip.value;
  if (!c || !local.value) return;
  Object.assign(c.clip, patch);
  persist();
}

function setSize(w: number, h: number, fps: number) {
  if (!local.value) return;
  local.value.width = w;
  local.value.height = h;
  local.value.fps = fps;
  persist();
}

const videoTakes = computed(() => project.takes.filter((t) => t.status === "done" && t.file));

async function appendTake(takeId: string) {
  const t = project.takes.find((x) => x.id === takeId);
  if (!t?.file || !local.value) return;
  const track = local.value.tracks.find((x) => x.kind === "video");
  if (!track) return;
  const start = track.clips.reduce(
    (m, c) => Math.max(m, c.startSec + (c.outSec - c.inSec) / Math.max(c.speed, 0.01)),
    0,
  );
  track.clips.push({
    id: `clip_${Math.random().toString(36).slice(2, 10)}`,
    source: t.file,
    shotId: t.shotId,
    takeId: t.id,
    label: baseName(t.file),
    inSec: 0,
    outSec: t.durationSec ?? 3,
    startSec: start,
    speed: 1,
    volume: 1,
    enabled: true,
  });
  await persist();
}

const renderJobs = computed(() =>
  jobs.jobs.filter((j) => j.kind === "ffmpeg").slice(0, 3),
);
</script>

<template>
  <div class="wrap" v-if="tl">
    <div class="toolbar">
      <n-button size="small" @click="buildFromTakes">按镜头铺轨</n-button>
      <n-button size="small" @click="persist">保存时间线</n-button>
      <div class="row" style="gap: 6px">
        <span class="tiny faint">烧字幕</span>
        <n-switch v-model:value="burnSubs" size="small" />
      </div>
      <div class="grow" />
      <span class="tiny faint">
        {{ fmtDuration(totalDuration) }} · {{ tl.width }}×{{ tl.height }} · {{ tl.fps }}fps
      </span>
      <n-button size="small" type="primary" @click="render">导出成片</n-button>
    </div>

    <div class="pane tl">
      <div class="ruler">
        <div class="track-name" />
        <div class="lane">
          <div class="ticks">
            <span v-for="i in 11" :key="i" class="tick">
              {{ fmtDuration((totalDuration / 10) * (i - 1)) }}
            </span>
          </div>
        </div>
      </div>

      <div v-for="track in tl.tracks" :key="track.id" class="track">
        <div class="track-name">
          <span class="tag">{{ trackKindLabel[track.kind] }}</span>
          <span class="tiny truncate">{{ track.name }}</span>
          <span class="tiny faint">{{ track.clips.length }}</span>
        </div>
        <div class="lane">
          <div
            v-for="clip in track.clips"
            :key="clip.id"
            class="clip"
            :class="[track.kind, { sel: selected?.clipId === clip.id }]"
            :style="clipStyle(track, clip)"
            :title="clip.label"
            @click="pick(track.id, clip)"
            @dblclick="clip.source && (previewClip = clip)"
          >
            <span class="tiny truncate">{{ clip.label }}</span>
          </div>
          <div v-if="!track.clips.length" class="tiny faint empty-lane">空</div>
        </div>
      </div>

      <div class="row wrap foot">
        <span class="tiny faint">从已出片的镜头快速追加：</span>
        <n-button
          v-for="t in videoTakes.slice(0, 12)"
          :key="t.id"
          size="tiny"
          quaternary
          @click="appendTake(t.id)"
        >
          + {{ project.shots.find((s) => s.id === t.shotId)?.index ?? "?" }}
        </n-button>
        <span v-if="!videoTakes.length" class="tiny faint">（还没有已生成的视频）</span>
      </div>
    </div>

    <div class="pane detail" v-if="currentClip">
      <div class="row-between head">
        <span style="font-weight: 600">片段设置</span>
        <n-button size="tiny" quaternary @click="selected = null">取消选择</n-button>
      </div>
      <div class="scroll body col">
        <div class="tiny faint truncate" :title="currentClip.clip.source">
          {{ currentClip.clip.label }}
        </div>
        <div class="grid2">
          <label>时间线起点(s)
            <n-input-number
              :value="currentClip.clip.startSec" size="small" :min="0" :step="0.1"
              @update:value="(v: number | null) => patchCurrent({ startSec: v ?? 0 })"
            />
          </label>
          <label>源内入点(s)
            <n-input-number
              :value="currentClip.clip.inSec" size="small" :min="0" :step="0.1"
              @update:value="(v: number | null) => patchCurrent({ inSec: v ?? 0 })"
            />
          </label>
          <label>源内出点(s)
            <n-input-number
              :value="currentClip.clip.outSec" size="small" :min="0" :step="0.1"
              @update:value="(v: number | null) => patchCurrent({ outSec: v ?? 0 })"
            />
          </label>
          <label>速度
            <n-input-number
              :value="currentClip.clip.speed" size="small" :min="0.25" :max="4" :step="0.05"
              @update:value="(v: number | null) => patchCurrent({ speed: v ?? 1 })"
            />
          </label>
          <label>音量
            <n-input-number
              :value="currentClip.clip.volume" size="small" :min="0" :max="3" :step="0.1"
              @update:value="(v: number | null) => patchCurrent({ volume: v ?? 1 })"
            />
          </label>
          <label>启用
            <div><n-switch :value="currentClip.clip.enabled" size="small" @update:value="(v: boolean) => patchCurrent({ enabled: v })" /></div>
          </label>
        </div>
        <div class="row">
          <n-button size="tiny" @click="previewClip = currentClip.clip">预览</n-button>
          <n-button size="tiny" quaternary @click="removeClip">从时间线移除</n-button>
        </div>
        <div class="tiny faint">
          当前导出实现：主视频轨按起点排序后依次拼接，音频轨混在主轨之上，
          字幕可选烧录。转场、变速曲线等留到后续版本。
        </div>
      </div>
    </div>

    <div class="pane detail" v-else>
      <div class="scroll body col">
        <div style="font-weight: 600">导出进度</div>
        <div v-for="j in renderJobs" :key="j.id" class="col" style="gap: 2px">
          <div class="row-between">
            <span class="tiny truncate">{{ j.title }}</span>
            <span class="tiny faint">{{ j.status }}</span>
          </div>
          <n-progress
            type="line" :percentage="Math.round(j.progress * 100)" :height="4"
            :status="j.status === 'failed' ? 'error' : j.status === 'done' ? 'success' : 'default'"
          />
          <div v-if="j.error" class="tiny err">{{ j.error }}</div>
        </div>
        <div v-if="!renderJobs.length" class="tiny faint">还没有导出记录</div>
        <div class="col" style="gap: 4px; margin-top: 10px">
          <span class="tiny faint">输出规格</span>
          <div class="row" style="gap: 4px">
            <n-button size="tiny" quaternary @click="setSize(1080, 1920, 30)">竖屏 1080×1920</n-button>
            <n-button size="tiny" quaternary @click="setSize(1920, 1080, 30)">横屏 1920×1080</n-button>
          </div>
          <div class="row" style="gap: 4px">
            <n-button size="tiny" quaternary @click="setSize(1080, 1080, 30)">方形 1080</n-button>
            <n-button size="tiny" quaternary @click="setSize(720, 1280, 30)">竖屏 720×1280</n-button>
          </div>
        </div>
      </div>
    </div>

    <div v-if="previewClip" class="preview" @click.self="previewClip = null">
      <div class="pane box">
        <div class="row-between head">
          <span class="truncate">{{ previewClip.label }}</span>
          <n-button size="tiny" quaternary @click="previewClip = null">关闭</n-button>
        </div>
        <video :src="fileUrl(previewClip.source)" controls autoplay class="big" />
      </div>
    </div>
  </div>
  <div v-else class="tiny faint" style="padding: 24px">正在读取时间线…</div>
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
  gap: 8px;
  height: 34px;
}
.tl {
  flex: 1;
  min-width: 420px;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: auto;
  padding: 8px;
}
.ruler,
.track {
  display: flex;
  align-items: stretch;
  gap: 8px;
}
.ruler {
  height: 20px;
}
.track-name {
  width: 150px;
  flex: 0 0 150px;
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 0 6px;
  font-size: 11px;
  color: var(--text-dim);
}
.lane {
  position: relative;
  flex: 1;
  min-width: 300px;
  height: 44px;
  background: #14141a;
  border: 1px solid var(--line-soft);
  border-radius: 5px;
  margin-bottom: 5px;
  overflow: hidden;
}
.track:last-of-type .lane {
  margin-bottom: 0;
}
.ticks {
  position: relative;
  height: 100%;
  display: flex;
  align-items: center;
}
.tick {
  position: absolute;
  font-size: 10px;
  color: var(--text-faint);
  transform: translateX(-50%);
}
.tick:nth-child(1) {
  transform: none;
}
.clip {
  position: absolute;
  top: 3px;
  bottom: 3px;
  border-radius: 4px;
  padding: 0 5px;
  display: flex;
  align-items: center;
  cursor: pointer;
  overflow: hidden;
  border: 1px solid transparent;
  background: #2a3550;
  color: #c8d4ea;
}
.clip.audio {
  background: #234034;
  color: #b6dcc8;
}
.clip.subtitle {
  background: #3a2f22;
  color: #e2c9a4;
}
.clip.overlay {
  background: #33243f;
  color: #d5bde6;
}
.clip.sel {
  border-color: var(--accent);
  box-shadow: 0 0 0 1px var(--accent);
}
.empty-lane {
  position: absolute;
  left: 8px;
  top: 12px;
}
.foot {
  padding: 8px 6px 2px;
  gap: 4px;
}
.detail {
  width: 320px;
  flex: 0 0 320px;
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
