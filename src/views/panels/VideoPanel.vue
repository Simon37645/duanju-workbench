<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Clapperboard, Film, Play, Trash2, X, Zap } from "@lucide/vue";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import { useJobsStore } from "@/stores/jobs";
import { useAgentStore } from "@/stores/agent";
import { api, errorText } from "@/api/ipc";
import { fileUrl } from "@/api/events";
import { toast } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiSelect from "@/ui/Select.vue";
import UiEmpty from "@/ui/Empty.vue";
import UiProgress from "@/ui/Progress.vue";
import type { Shot, VideoTake } from "@/types/models";

const project = useProjectStore();
const settings = useSettingsStore();
const jobs = useJobsStore();
const agent = useAgentStore();

const chapterId = ref("all");
const selectedShotId = ref<string | null>(null);
const preview = ref<VideoTake | null>(null);

const chapters = computed(() => project.chapters);
const shots = computed(() =>
  project.shots
    .filter((s) => chapterId.value === "all" || s.chapterId === chapterId.value)
    .slice()
    .sort((a, b) => a.index - b.index),
);
const stats = computed(() => ({
  total: shots.value.length,
  done: shots.value.filter((s) => project.takesOf(s.id).some((t) => t.status === "done")).length,
}));

const providerOptions = computed(() =>
  settings.byKind("video").map((p) => ({ label: p.name, hint: p.model, value: p.id })),
);

onMounted(async () => {
  await project.ensure();
  const first = shots.value[0];
  if (first) selectedShotId.value = first.id;
});

const takesOf = (id: string) =>
  project.takesOf(id).slice().sort((a, b) => b.createdAt.localeCompare(a.createdAt));

function readiness(s: Shot) {
  const p = project.promptOf(s.id);
  const problems: string[] = [];
  if (!p) problems.push("没有视频提示词");
  else {
    if (!p.prompt.trim()) problems.push("提示词为空");
    if (!p.firstFrame) problems.push("没配首帧图");
  }
  return { ok: problems.length === 0, problems };
}

async function generate(shotIds: string[]) {
  if (!settings.activeVideo) {
    return toast.warn("还没有启用视频 provider，去设置里加一个（也可以先用占位生成跑通流程）");
  }
  try {
    const ids = await api.videoGenerate(shotIds, settings.settings?.activeVideoProviderId ?? undefined);
    toast.info(`已提交 ${ids.length} 个生成任务`);
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function generateMissing() {
  const todo = shots.value.filter((s) => !project.takesOf(s.id).some((t) => t.status === "done"));
  if (!todo.length) return toast.info("这一段的镜头都出片了");
  await generate(todo.map((s) => s.id));
}

async function removeTake(t: VideoTake) {
  await api.videoDeleteTake(t.id);
  await project.reload();
  if (preview.value?.id === t.id) preview.value = null;
}

const takeJob = (t: VideoTake) => jobs.byId(t.jobId);
const currentTakes = computed(() => (selectedShotId.value ? takesOf(selectedShotId.value) : []));
const currentShot = computed(() => project.shots.find((s) => s.id === selectedShotId.value));
</script>

<template>
  <div class="layout">
    <section class="panel list-panel">
      <header class="panel-head">
        <div class="row" style="gap: 6px">
          <span class="section-label">出片</span>
          <UiBadge :tone="stats.done === stats.total && stats.total > 0 ? 'ok' : 'neutral'" size="xs">
            {{ stats.done }}/{{ stats.total }}
          </UiBadge>
        </div>
        <UiButton variant="default" size="xs" @click="generateMissing">
          <template #icon><Zap :size="12" /></template>
          批量补齐
        </UiButton>
      </header>

      <div class="filterbar col" style="gap: 6px">
        <UiSelect
          v-model="chapterId"
          :options="[{ label: '全部章节', value: 'all' }, ...chapters.map((c) => ({ label: `第${c.index}章`, value: c.id }))]"
        />
        <UiSelect
          v-if="providerOptions.length"
          :model-value="settings.settings?.activeVideoProviderId ?? null"
          :options="providerOptions"
          placeholder="视频 provider"
          @update:model-value="(v: string | null) => settings.save({ activeVideoProviderId: v })"
        />
      </div>

      <div class="list scroll">
        <button
          v-for="s in shots"
          :key="s.id"
          class="item"
          :class="{ on: s.id === selectedShotId }"
          @click="selectedShotId = s.id"
        >
          <span class="idx num">{{ s.index }}</span>
          <div class="col grow" style="gap: 2px; min-width: 0">
            <span class="t-sm truncate">{{ s.action || "（无描述）" }}</span>
            <span v-if="!readiness(s).ok" class="t-xs truncate" style="color: var(--warn)">
              缺：{{ readiness(s).problems.join("、") }}
            </span>
            <span v-else class="t-xs faint">{{ takesOf(s.id).length }} 条记录</span>
          </div>
          <UiBadge
            :tone="takesOf(s.id).some((t) => t.status === 'done') ? 'ok' : readiness(s).ok ? 'warn' : 'neutral'"
            size="xs"
          >
            {{ takesOf(s.id).some((t) => t.status === "done") ? "已出片" : readiness(s).ok ? "待生成" : "缺条件" }}
          </UiBadge>
        </button>

        <UiEmpty v-if="!shots.length" compact title="还没有镜头" hint="先去分镜面板" />
      </div>
    </section>

    <section class="panel detail-panel">
      <template v-if="selectedShotId">
        <header class="panel-head">
          <div class="row" style="gap: 8px; min-width: 0">
            <span class="section-label">镜头 {{ currentShot?.index }} 的成片</span>
            <UiBadge tone="neutral" size="xs">{{ currentTakes.length }} 条</UiBadge>
          </div>
          <UiButton variant="primary" size="sm" @click="generate([selectedShotId!])">
            <template #icon><Clapperboard :size="13" /></template>
            再出一条
          </UiButton>
        </header>

        <div class="body scroll">
          <UiEmpty
            v-if="!currentTakes.length"
            title="还没有生成记录"
            :hint="readiness(currentShot!).ok ? '点右上「再出一条」提交' : `还缺：${readiness(currentShot!).problems.join('、')}`"
          >
            <template #icon><Film :size="26" /></template>
          </UiEmpty>

          <div v-else class="takes">
            <div v-for="t in currentTakes" :key="t.id" class="take">
              <div class="video" @click="t.file && (preview = t)">
                <video v-if="t.file" :src="fileUrl(t.file)" preload="metadata" muted />
                <div v-else class="ph t-xs faint">
                  {{ t.status === "running" ? "生成中…" : t.status === "failed" ? "失败" : t.status }}
                </div>
                <div v-if="t.file" class="playhint"><Play :size="16" /></div>
                <UiBadge
                  class="vbadge"
                  :tone="t.status === 'done' ? 'ok' : t.status === 'failed' ? 'err' : 'warn'"
                  size="xs"
                >
                  {{ t.status === "done" ? "完成" : t.status === "failed" ? "失败" : t.status }}
                </UiBadge>
              </div>
              <UiProgress v-if="takeJob(t)?.status === 'running'" :value="(takeJob(t)!.progress ?? 0) * 100" :height="3" />
              <div class="row-between">
                <span class="t-xs faint truncate">{{ t.model }}</span>
                <span class="t-xs faint num">{{ t.durationSec?.toFixed(1) ?? "?" }}s</span>
              </div>
              <div v-if="t.error" class="t-xs" style="color: var(--err); line-height: 1.6">{{ t.error }}</div>
              <UiButton variant="subtle" size="xs" block @click="removeTake(t)">
                <template #icon><Trash2 :size="11" /></template>
                删除
              </UiButton>
            </div>
          </div>
        </div>
      </template>

      <UiEmpty v-else title="选一个镜头" hint="从左侧点一个镜头看它的成片">
        <template #icon><Clapperboard :size="26" /></template>
      </UiEmpty>
    </section>

    <!-- 预览 -->
    <Teleport to="body">
      <div v-if="preview" class="mask" @click.self="preview = null">
        <div class="player fade-in">
          <div class="row-between" style="margin-bottom: 8px">
            <span class="t-sm">{{ preview.model }}</span>
            <UiButton variant="subtle" size="xs" icon @click="preview = null">
              <template #icon><X :size="13" /></template>
            </UiButton>
          </div>
          <video :src="fileUrl(preview.file)" controls autoplay class="big" />
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.layout {
  display: flex;
  gap: var(--sp-3);
  height: 100%;
  min-height: 0;
}
.list-panel {
  width: 300px;
  flex: 0 0 300px;
}
.detail-panel {
  flex: 1;
  min-width: 0;
}
.filterbar {
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
.item {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 7px 8px;
  border-radius: var(--r);
  border: 1px solid transparent;
  background: none;
  cursor: pointer;
  text-align: left;
  transition: background var(--fast), border-color var(--fast);
}
.item:hover {
  background: var(--surface-3);
}
.item.on {
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

.body {
  flex: 1;
  min-height: 0;
  padding: var(--sp-4);
}
.takes {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: var(--sp-3);
}
.take {
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.video {
  position: relative;
  aspect-ratio: 9 / 16;
  background: var(--media-bg);
  border-radius: var(--r-sm);
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
.playhint {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  color: rgba(255, 255, 255, 0.85);
  background: rgba(0, 0, 0, 0.22);
  opacity: 0;
  transition: opacity var(--fast);
}
.video:hover .playhint {
  opacity: 1;
}
.vbadge {
  position: absolute;
  top: 6px;
  left: 6px;
}

.mask {
  position: fixed;
  inset: 0;
  z-index: 260;
  background: var(--mask-bg);
  display: grid;
  place-items: center;
}
.player {
  background: var(--surface-1);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-lg);
  padding: 10px;
  max-width: 82vw;
  max-height: 90vh;
}
.big {
  max-height: 76vh;
  max-width: 74vw;
  border-radius: var(--r);
}
</style>
