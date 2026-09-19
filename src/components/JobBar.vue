<script setup lang="ts">
import { computed } from "vue";
import {
  ChevronDown, Clapperboard, Download, FileVideo, Image as ImageIcon,
  Captions, Loader2, CheckCircle2, AlertCircle, XCircle,
} from "@lucide/vue";
import { useJobsStore } from "@/stores/jobs";
import { fmtTime } from "@/api/events";
import UiButton from "@/ui/Button.vue";
import UiProgress from "@/ui/Progress.vue";
import UiEmpty from "@/ui/Empty.vue";
import type { Job, JobKind } from "@/types/models";

const jobs = useJobsStore();

const running = computed(() => jobs.running);
const finished = computed(() =>
  jobs.jobs.filter((j) => j.status !== "running" && j.status !== "queued").slice(0, 30),
);

const ICON: Record<JobKind, unknown> = {
  image: ImageIcon, video: Clapperboard, asr: Captions,
  ffmpeg: FileVideo, llm: Loader2, download: Download,
};
const LABEL: Record<JobKind, string> = {
  image: "生图", video: "生视频", asr: "转写", ffmpeg: "转码", llm: "模型", download: "下载",
};
const TONE: Record<JobKind, string> = {
  image: "var(--info)", video: "var(--violet)", asr: "var(--ok)",
  ffmpeg: "var(--warn)", llm: "var(--fg-dim)", download: "var(--accent)",
};

function statusIcon(j: Job) {
  if (j.status === "done") return CheckCircle2;
  if (j.status === "failed") return AlertCircle;
  if (j.status === "canceled") return XCircle;
  return Loader2;
}
</script>

<template>
  <div class="bar" :class="{ open: jobs.barOpen }">
    <button class="head" @click="jobs.barOpen = !jobs.barOpen">
      <div class="row grow" style="gap: 10px; min-width: 0">
        <span class="section-label">任务</span>

        <template v-if="running.length">
          <div v-for="j in running.slice(0, 3)" :key="j.id" class="mini">
            <span class="micon" :style="{ color: TONE[j.kind] }">
              <Loader2 :size="11" class="spin" />
            </span>
            <span class="t-xs truncate" style="max-width: 160px" :title="j.title">{{ j.title }}</span>
            <div style="width: 60px">
              <UiProgress :value="j.progress * 100" :height="3" />
            </div>
            <span class="t-xs faint num">{{ Math.round(j.progress * 100) }}%</span>
          </div>
          <span v-if="running.length > 3" class="t-xs faint">+{{ running.length - 3 }}</span>
        </template>
        <span v-else class="t-xs faint">空闲</span>
      </div>
      <ChevronDown :size="13" class="chev" :class="{ rot: !jobs.barOpen }" />
    </button>

    <div v-if="jobs.barOpen" class="list scroll">
      <UiEmpty
        v-if="!jobs.jobs.length"
        compact
        title="还没有任务"
        hint="生图、生视频、转码、转写都会出现在这里"
      />
      <div v-for="j in [...running, ...finished]" :key="j.id" class="job">
        <span class="jicon" :style="{ color: TONE[j.kind] }">
          <component :is="ICON[j.kind]" :size="13" :class="{ spin: j.status === 'running' }" />
        </span>
        <span class="jkind t-xs faint">{{ LABEL[j.kind] }}</span>
        <span class="jtitle t-sm truncate" :title="j.title">{{ j.title }}</span>
        <div v-if="j.status === 'running'" class="jbar">
          <UiProgress :value="j.progress * 100" :height="3" />
        </div>
        <span class="jdetail t-xs faint truncate" :title="j.error || j.detail" :class="{ bad: !!j.error }">
          {{ j.error || j.detail }}
        </span>
        <span class="t-xs faint num">{{ fmtTime(j.createdAt).slice(5) }}</span>
        <component :is="statusIcon(j)" :size="13" class="jstate" :class="j.status" />
        <UiButton
          v-if="j.status === 'running' || j.status === 'queued'"
          variant="subtle"
          size="xs"
          @click="jobs.cancel(j.id)"
        >
          取消
        </UiButton>
      </div>
      <div v-if="finished.length" class="row" style="padding: 4px 2px">
        <UiButton variant="subtle" size="xs" @click="jobs.clearFinished()">清除已结束</UiButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
.bar {
  flex: 0 0 auto;
  border-top: 1px solid var(--line);
  background: var(--surface-1);
}
.head {
  width: 100%;
  height: 32px;
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: 0 var(--sp-4);
  background: none;
  border: none;
  cursor: pointer;
  text-align: left;
}
.head:hover {
  background: var(--surface-2);
}
.mini {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 8px 2px 6px;
  border-radius: var(--r-sm);
  background: var(--surface-3);
  border: 1px solid var(--line);
}
.micon {
  display: grid;
  place-items: center;
}
.chev {
  color: var(--fg-ghost);
  transition: transform var(--fast);
}
.chev.rot {
  transform: rotate(-90deg);
}
.spin {
  animation: spin 900ms linear infinite;
}

.list {
  max-height: 210px;
  border-top: 1px solid var(--line-faint);
  padding: 6px var(--sp-4) 8px;
}
.job {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  height: 28px;
  border-radius: var(--r-sm);
  padding: 0 4px;
}
.job:hover {
  background: var(--surface-2);
}
.jicon {
  display: grid;
  place-items: center;
  flex: 0 0 auto;
}
.jkind {
  flex: 0 0 40px;
  color: var(--fg-ghost);
}
.jtitle {
  flex: 0 1 220px;
  min-width: 0;
}
.jbar {
  width: 80px;
  flex: 0 0 auto;
}
.jdetail {
  flex: 1 1 auto;
  min-width: 0;
  max-width: 340px;
}
.jdetail.bad {
  color: var(--err);
}
.jstate {
  flex: 0 0 auto;
  color: var(--fg-ghost);
}
.jstate.done {
  color: var(--ok);
}
.jstate.failed {
  color: var(--err);
}
.jstate.canceled {
  color: var(--fg-ghost);
}
</style>
