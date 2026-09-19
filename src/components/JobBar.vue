<script setup lang="ts">
import { computed, ref } from "vue";
import { NProgress, NButton, NPopover } from "naive-ui";
import { useJobsStore } from "@/stores/jobs";
import { fmtTime } from "@/api/events";
import type { Job } from "@/types/models";

const jobs = useJobsStore();
const expanded = ref(false);

const running = computed(() => jobs.running);
const finished = computed(() => jobs.jobs.filter((j) => j.status !== "running" && j.status !== "queued").slice(0, 20));

const statusText: Record<Job["status"], string> = {
  queued: "排队",
  running: "进行中",
  done: "完成",
  failed: "失败",
  canceled: "已取消",
};

function kindColor(kind: Job["kind"]) {
  return (
    {
      image: "#7a9de0",
      video: "#d98fc0",
      asr: "#6fc0a8",
      ffmpeg: "#e0a86f",
      llm: "#b39ddb",
      download: "#8fbf6f",
    } as Record<string, string>
  )[kind];
}
</script>

<template>
  <div class="jobbar" :class="{ open: expanded }">
    <div class="head row-between" @click="expanded = !expanded">
      <div class="row" style="gap: 10px; min-width: 0">
        <span class="tiny" style="color: var(--text-dim)">任务</span>
        <template v-if="running.length">
          <div v-for="j in running.slice(0, 3)" :key="j.id" class="row" style="gap: 6px">
            <span class="dot" :style="{ background: kindColor(j.kind) }" />
            <span class="tiny truncate" style="max-width: 180px" :title="j.title">{{ j.title }}</span>
            <n-progress
              type="line"
              style="width: 76px"
              :percentage="Math.round(j.progress * 100)"
              :show-indicator="false"
              :height="4"
              :color="kindColor(j.kind)"
            />
            <span class="tiny faint">{{ Math.round(j.progress * 100) }}%</span>
          </div>
          <span v-if="running.length > 3" class="tiny faint">+{{ running.length - 3 }}</span>
        </template>
        <span v-else class="tiny faint">空闲</span>
      </div>
      <span class="tiny faint">{{ expanded ? "收起 ▾" : "展开 ▴" }}</span>
    </div>

    <div v-if="expanded" class="list scroll">
      <div v-if="!jobs.jobs.length" class="tiny faint" style="padding: 10px">还没有任务记录</div>
      <div v-for="j in [...running, ...finished]" :key="j.id" class="row job">
        <span class="dot" :style="{ background: kindColor(j.kind) }" />
        <span class="tiny truncate" style="width: 220px" :title="j.title">{{ j.title }}</span>
        <span class="tiny faint truncate grow" :title="j.error || j.detail">
          {{ j.error || j.detail }}
        </span>
        <span class="tiny" :class="{ err: j.status === 'failed' }">{{ statusText[j.status] }}</span>
        <span class="tiny faint">{{ fmtTime(j.createdAt) }}</span>
        <n-button
          v-if="j.status === 'running' || j.status === 'queued'"
          size="tiny"
          quaternary
          @click="jobs.cancel(j.id)"
        >
          取消
        </n-button>
      </div>
      <div class="row" style="padding: 6px 10px">
        <n-button size="tiny" quaternary @click="jobs.clearFinished()">清除已结束</n-button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.jobbar {
  flex: 0 0 auto;
  background: var(--bg-1);
  border-top: 1px solid var(--line);
}
.head {
  height: 30px;
  padding: 0 10px;
  cursor: pointer;
}
.head:hover {
  background: var(--bg-2);
}
.list {
  max-height: 190px;
  border-top: 1px solid var(--line-soft);
}
.job {
  padding: 4px 10px;
  gap: 8px;
}
.job:hover {
  background: var(--bg-2);
}
.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex: 0 0 auto;
}
.err {
  color: var(--err);
}
</style>
