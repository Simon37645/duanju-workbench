<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  Download, Film, ListPlus, Maximize2, MousePointerClick, Save, Scissors, Trash2,
} from "@lucide/vue";
import { useProjectStore } from "@/stores/project";
import { useJobsStore } from "@/stores/jobs";
import { api, errorText } from "@/api/ipc";
import { fileUrl, fmtDuration } from "@/api/events";
import { toast } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiNumber from "@/ui/NumberInput.vue";
import UiSwitch from "@/ui/Switch.vue";
import UiField from "@/ui/Field.vue";
import UiEmpty from "@/ui/Empty.vue";
import UiProgress from "@/ui/Progress.vue";
import type { Clip, Timeline, Track } from "@/types/models";

const project = useProjectStore();
const jobs = useJobsStore();

const local = ref<Timeline | null>(null);
const selected = ref<{ trackId: string; clipId: string } | null>(null);
const burnSubs = ref(true);
const fullscreen = ref(false);

const tl = computed(() => local.value ?? project.timeline);
const total = computed(() => {
  if (!tl.value) return 0;
  return tl.value.tracks
    .flatMap((t) => t.clips)
    .reduce((m, c) => Math.max(m, c.startSec + (c.outSec - c.inSec) / Math.max(c.speed, 0.01)), 0);
});

const KIND_LABEL: Record<string, string> = { video: "视频", audio: "音频", subtitle: "字幕", overlay: "叠加" };

onMounted(async () => {
  await project.ensure();
  sync();
  // 默认选中第一段，播放器就有内容
  const first = tl.value?.tracks.find((t) => t.kind === "video")?.clips[0];
  if (first && tl.value) selected.value = { trackId: "tr_video", clipId: first.id };
});

function sync() {
  if (project.timeline) local.value = JSON.parse(JSON.stringify(project.timeline));
}

async function persist() {
  if (!local.value) return;
  try {
    await api.editSetTimeline(JSON.parse(JSON.stringify(local.value)));
    await project.reload();
    sync();
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function build() {
  try {
    const n = await api.editBuildFromTakes();
    await project.reload();
    sync();
    const first = tl.value?.tracks.find((t) => t.kind === "video")?.clips[0];
    if (first && tl.value) selected.value = { trackId: "tr_video", clipId: first.id };
    toast.ok(`已按镜头顺序铺入 ${n} 段`);
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function render() {
  try {
    await api.editRender(burnSubs.value);
    toast.info("已开始导出，完成后在底部任务栏可见");
  } catch (e) {
    toast.err(errorText(e));
  }
}

function clipStyle(c: Clip) {
  const t = Math.max(total.value, 1);
  const dur = (c.outSec - c.inSec) / Math.max(c.speed, 0.01);
  return {
    left: `${(c.startSec / t) * 100}%`,
    width: `${Math.max(0.8, (dur / t) * 100)}%`,
    opacity: c.enabled ? 1 : 0.3,
  };
}

const currentClip = computed<{ track: Track; clip: Clip } | null>(() => {
  const s = selected.value;
  if (!s || !tl.value) return null;
  const track = tl.value.tracks.find((t) => t.id === s.trackId);
  const clip = track?.clips.find((c) => c.id === s.clipId);
  return track && clip ? { track, clip } : null;
});

/** 播放器显示当前选中的片段；没选就退回第一段 */
const playing = computed<Clip | null>(() => {
  if (currentClip.value) return currentClip.value.clip;
  return tl.value?.tracks.find((t) => t.kind === "video")?.clips[0] ?? null;
});

function patch(p: Partial<Clip>) {
  const c = currentClip.value;
  if (!c) return;
  Object.assign(c.clip, p);
  persist();
}

async function removeClip() {
  const s = selected.value;
  if (!s || !local.value) return;
  const track = local.value.tracks.find((t) => t.id === s.trackId);
  if (track) track.clips = track.clips.filter((c) => c.id !== s.clipId);
  selected.value = null;
  await persist();
}

function setSize(w: number, h: number, fps: number) {
  if (!local.value) return;
  Object.assign(local.value, { width: w, height: h, fps });
  persist();
}

const doneTakes = computed(() => project.takes.filter((t) => t.status === "done" && t.file));

async function appendTake(takeId: string) {
  const t = project.takes.find((x) => x.id === takeId);
  if (!t?.file || !local.value) return;
  const track = local.value.tracks.find((x) => x.kind === "video");
  if (!track) return;
  const start = track.clips.reduce(
    (m, c) => Math.max(m, c.startSec + (c.outSec - c.inSec) / Math.max(c.speed, 0.01)), 0);
  track.clips.push({
    id: `clip_${Math.random().toString(36).slice(2, 10)}`,
    source: t.file, shotId: t.shotId, takeId: t.id,
    label: `镜头 ${project.shots.find((s) => s.id === t.shotId)?.index ?? "?"}`,
    inSec: 0, outSec: t.durationSec ?? 3, startSec: start,
    speed: 1, volume: 1, enabled: true,
  });
  await persist();
}

const renderJobs = computed(() => jobs.jobs.filter((j) => j.kind === "ffmpeg").slice(0, 4));

/** 时间线刻度 */
const TICKS = 6;
</script>

<template>
  <div class="editor">
    <!-- 工具栏 -->
    <div class="toolbar">
      <UiBadge tone="neutral" size="xs"><Film :size="9" /> {{ fmtDuration(total) }}</UiBadge>
      <span class="t-xs faint num">{{ tl?.width }}×{{ tl?.height }} · {{ tl?.fps }}fps</span>
      <span class="grow" />
      <div class="row" style="gap: 5px">
        <span class="t-xs faint">烧字幕</span>
        <UiSwitch v-model="burnSubs" />
      </div>
      <UiButton variant="subtle" size="xs" @click="build">
        <template #icon><ListPlus :size="12" /></template>
        按镜头铺轨
      </UiButton>
      <UiButton variant="subtle" size="xs" @click="persist">
        <template #icon><Save :size="12" /></template>
        保存
      </UiButton>
      <UiButton variant="primary" size="sm" @click="render">
        <template #icon><Download :size="12" /></template>
        导出成片
      </UiButton>
    </div>

    <!-- 上方：预览 + 检查器 -->
    <div class="stage">
      <div class="viewer">
        <div class="screen" :class="{ empty: !playing?.source }">
          <video v-if="playing?.source" :key="playing.source" :src="fileUrl(playing.source)" controls />
          <div v-else class="col" style="align-items: center; gap: 8px">
            <Scissors :size="24" class="faint" />
            <span class="t-sm faint">时间线上还没有片段</span>
            <UiButton variant="outline" size="sm" @click="build">按镜头铺轨</UiButton>
          </div>
        </div>

        <div class="vbar">
          <div class="col" style="gap: 1px; min-width: 0">
            <span class="t-sm truncate" style="font-weight: 500">{{ playing?.label ?? "—" }}</span>
            <span class="t-xs faint num">
              {{ playing ? `${playing.inSec.toFixed(1)}s → ${playing.outSec.toFixed(1)}s · 速度 ${playing.speed}` : "选中轨道上的片段即可预览" }}
            </span>
          </div>
          <span class="grow" />
          <UiButton
            v-if="playing"
            variant="subtle"
            size="xs"
            icon
            @click="fullscreen = true"
          >
            <template #icon><Maximize2 :size="13" /></template>
          </UiButton>
        </div>
      </div>

      <!-- 检查器 -->
      <aside class="inspector">
        <template v-if="currentClip">
          <div class="section-label">片段</div>
          <div class="grid2">
            <UiField label="起点">
              <UiNumber :model-value="currentClip.clip.startSec" :min="0" :step="0.1" suffix="s" @update:model-value="(v: number | null) => patch({ startSec: v ?? 0 })" />
            </UiField>
            <UiField label="源入点">
              <UiNumber :model-value="currentClip.clip.inSec" :min="0" :step="0.1" suffix="s" @update:model-value="(v: number | null) => patch({ inSec: v ?? 0 })" />
            </UiField>
            <UiField label="源出点">
              <UiNumber :model-value="currentClip.clip.outSec" :min="0" :step="0.1" suffix="s" @update:model-value="(v: number | null) => patch({ outSec: v ?? 0 })" />
            </UiField>
            <UiField label="速度">
              <UiNumber :model-value="currentClip.clip.speed" :min="0.25" :max="4" :step="0.05" @update:model-value="(v: number | null) => patch({ speed: v ?? 1 })" />
            </UiField>
            <UiField label="音量">
              <UiNumber :model-value="currentClip.clip.volume" :min="0" :max="3" :step="0.1" @update:model-value="(v: number | null) => patch({ volume: v ?? 1 })" />
            </UiField>
            <UiField label="启用">
              <UiSwitch :model-value="currentClip.clip.enabled" @update:model-value="(v: boolean) => patch({ enabled: v })" />
            </UiField>
          </div>
          <UiButton variant="danger" size="sm" block @click="removeClip">
            <template #icon><Trash2 :size="12" /></template>
            从时间线移除
          </UiButton>
        </template>

        <template v-else>
          <div class="section-label">检查器</div>
          <div class="hint">
            <MousePointerClick :size="14" />
            <span class="t-xs">点下方轨道上的片段，这里会显示它的入点、出点、速度与音量。</span>
          </div>
        </template>

        <div class="divider" />

        <div class="section-label">导出</div>
        <div v-for="j in renderJobs" :key="j.id" class="job">
          <div class="row-between">
            <span class="t-xs truncate">{{ j.title }}</span>
            <UiBadge :tone="j.status === 'done' ? 'ok' : j.status === 'failed' ? 'err' : 'warn'" size="xs">
              {{ j.status }}
            </UiBadge>
          </div>
          <UiProgress :value="j.progress * 100" :height="3" />
          <div v-if="j.error" class="t-xs" style="color: var(--err)">{{ j.error }}</div>
        </div>
        <div v-if="!renderJobs.length" class="t-xs faint">还没有导出记录</div>

        <div class="row wrap" style="gap: 4px">
          <UiButton variant="subtle" size="xs" @click="setSize(768, 1344, 30)">768×1344</UiButton>
          <UiButton variant="subtle" size="xs" @click="setSize(1080, 1920, 30)">1080×1920</UiButton>
          <UiButton variant="subtle" size="xs" @click="setSize(1920, 1080, 30)">1920×1080</UiButton>
          <UiButton variant="subtle" size="xs" @click="setSize(1080, 1080, 30)">1080²</UiButton>
        </div>
      </aside>
    </div>

    <!-- 下方：轨道 -->
    <div class="timeline">
      <div v-if="tl" class="tracks scroll">
        <div class="rowline ruler">
          <div class="tname" />
          <div class="lane">
            <span
              v-for="i in TICKS"
              :key="i"
              class="tick num"
              :style="{ left: ((i - 1) / (TICKS - 1)) * 100 + '%' }"
            >
              {{ fmtDuration((total / (TICKS - 1)) * (i - 1)) }}
            </span>
          </div>
        </div>

        <div v-for="track in tl.tracks" :key="track.id" class="rowline">
          <div class="tname">
            <span class="t-sm truncate">{{ track.name }}</span>
            <span class="t-xs faint num">{{ track.clips.length }}</span>
          </div>
          <div class="lane">
            <button
              v-for="c in track.clips"
              :key="c.id"
              class="clip"
              :class="[track.kind, { sel: selected?.clipId === c.id }]"
              :style="clipStyle(c)"
              :title="c.label"
              @click="selected = { trackId: track.id, clipId: c.id }"
            >
              <span class="t-xs truncate">{{ c.label }}</span>
            </button>
            <span v-if="!track.clips.length" class="lane-empty t-xs">空</span>
          </div>
        </div>

        <div class="quick">
          <span class="t-xs faint">追加已出片的镜头：</span>
          <button v-for="t in doneTakes.slice(0, 16)" :key="t.id" class="q" @click="appendTake(t.id)">
            +{{ project.shots.find((s) => s.id === t.shotId)?.index ?? "?" }}
          </button>
          <span v-if="!doneTakes.length" class="t-xs faint">（还没有已生成的视频）</span>
        </div>
      </div>

      <UiEmpty v-else compact title="正在读取时间线…" />
    </div>

    <!-- 全屏预览 -->
    <Teleport to="body">
      <div v-if="fullscreen && playing" class="mask" @click.self="fullscreen = false">
        <div class="playerbox fade-in">
          <div class="row-between" style="margin-bottom: 8px">
            <span class="t-sm truncate">{{ playing.label }}</span>
            <UiButton variant="subtle" size="xs" @click="fullscreen = false">关闭</UiButton>
          </div>
          <video :src="fileUrl(playing.source)" controls autoplay class="big" />
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.editor {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
  min-height: 0;
}

/* ---------------------------------------------------------- 工具栏 */
.toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  padding: 8px 12px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
  flex: 0 0 auto;
}

/* ------------------------------------------------- 上方 预览 + 检查器 */
.stage {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  gap: var(--sp-3);
}
.viewer {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
  overflow: hidden;
}
.screen {
  flex: 1;
  min-height: 0;
  background: var(--media-bg);
  display: grid;
  place-items: center;
  overflow: hidden;
}
.screen video {
  max-width: 100%;
  max-height: 100%;
}
.vbar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-top: 1px solid var(--line-faint);
  background: var(--surface-2);
}

.inspector {
  width: 236px;
  flex: 0 0 236px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
  padding: var(--sp-3);
  display: flex;
  flex-direction: column;
  gap: 10px;
  overflow: auto;
}
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}
.divider {
  height: 1px;
  background: var(--line-faint);
  margin: 2px 0;
}
.hint {
  display: flex;
  gap: 7px;
  align-items: flex-start;
  padding: 9px 10px;
  background: var(--surface-3);
  border-radius: var(--r);
  color: var(--fg-faint);
  line-height: 1.6;
}
.job {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

/* -------------------------------------------------------- 下方轨道 */
.timeline {
  flex: 0 0 212px;
  min-height: 0;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.tracks {
  flex: 1;
  min-height: 0;
  padding: 8px 12px 10px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.rowline {
  display: flex;
  align-items: stretch;
  gap: 8px;
}
.ruler {
  height: 16px;
}
.tname {
  width: 84px;
  flex: 0 0 84px;
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.lane {
  position: relative;
  flex: 1;
  min-width: 160px;
  height: 32px;
  background: var(--surface-3);
  border: 1px solid var(--line-faint);
  border-radius: var(--r-sm);
  overflow: hidden;
}
.ruler .lane {
  height: 100%;
  background: none;
  border: none;
  overflow: visible;
}
.tick {
  position: absolute;
  top: 0;
  font-size: 10px;
  color: var(--fg-ghost);
  transform: translateX(-50%);
  white-space: nowrap;
}
.tick:first-child {
  transform: none;
}
.clip {
  position: absolute;
  top: 3px;
  bottom: 3px;
  border-radius: var(--r-xs);
  border: 1px solid transparent;
  padding: 0 6px;
  display: flex;
  align-items: center;
  cursor: pointer;
  overflow: hidden;
  text-align: left;
  transition: border-color var(--fast), filter var(--fast);
}
.clip:hover {
  filter: brightness(1.1);
}
.clip.video {
  background: var(--clip-video-bg);
  color: var(--clip-video-fg);
}
.clip.audio {
  background: var(--clip-audio-bg);
  color: var(--clip-audio-fg);
}
.clip.subtitle {
  background: var(--clip-sub-bg);
  color: var(--clip-sub-fg);
}
.clip.overlay {
  background: var(--clip-overlay-bg);
  color: var(--clip-overlay-fg);
}
.clip.sel {
  border-color: var(--accent);
  box-shadow: 0 0 0 1px var(--accent);
}
.lane-empty {
  position: absolute;
  left: 10px;
  top: 8px;
  color: var(--fg-ghost);
}
.quick {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 5px;
  margin-top: 2px;
}
.q {
  background: var(--surface-4);
  border: 1px solid var(--line);
  border-radius: var(--r-xs);
  color: var(--fg-dim);
  font-size: var(--t-xs);
  padding: 1px 7px;
  cursor: pointer;
  font-variant-numeric: tabular-nums;
}
.q:hover {
  color: var(--accent);
  border-color: var(--accent-line);
}

.mask {
  position: fixed;
  inset: 0;
  z-index: 260;
  background: var(--mask-bg);
  display: grid;
  place-items: center;
}
.playerbox {
  background: var(--surface-1);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-lg);
  padding: 10px;
  max-width: 86vw;
  max-height: 92vh;
}
.big {
  max-height: 78vh;
  max-width: 78vw;
  border-radius: var(--r);
}
</style>
