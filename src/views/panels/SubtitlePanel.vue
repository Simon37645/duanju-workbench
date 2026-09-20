<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { AlertTriangle, Captions, Cpu, Download, FileText, Trash2, Zap } from "@lucide/vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import { useJobsStore } from "@/stores/jobs";
import { api, errorText } from "@/api/ipc";
import { baseName } from "@/api/events";
import { toast } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiInput from "@/ui/Input.vue";
import UiSelect from "@/ui/Select.vue";
import UiField from "@/ui/Field.vue";
import UiEmpty from "@/ui/Empty.vue";
import UiProgress from "@/ui/Progress.vue";
import UiNumber from "@/ui/NumberInput.vue";
import type { AsrCapabilities, Cue, SubtitleDoc } from "@/types/models";

const project = useProjectStore();
const settings = useSettingsStore();
const jobs = useJobsStore();

const caps = ref<AsrCapabilities | null>(null);
const currentId = ref<string | null>(null);
const cues = ref<Cue[]>([]);
const sourceFile = ref("");
const dirty = ref(false);

const docs = computed(() => project.subtitles);
const current = computed(() => docs.value.find((d) => d.id === currentId.value) ?? null);

const modelOptions = computed(() =>
  (caps.value?.installedModels ?? []).map((m) => ({
    label: `${m.label}${m.downloaded ? "" : "（未下载）"}`,
    value: m.id,
    disabled: !m.downloaded,
  })),
);

const sources = computed(() => {
  const tl = project.timeline;
  if (!tl) return [];
  const seen = new Set<string>();
  return tl.tracks
    .flatMap((t) => t.clips)
    .filter((c) => c.source && !seen.has(c.source) && seen.add(c.source))
    .map((c) => ({ label: baseName(c.source), value: c.source }));
});

const asrJobs = computed(() => jobs.jobs.filter((j) => j.kind === "asr").slice(0, 3));
const model = computed(() => caps.value?.installedModels.find((m) => m.id === settings.settings?.asrModel));

onMounted(async () => {
  caps.value = await api.asrCapabilities();
  await project.ensure();
  if (docs.value.length) select(docs.value[0].id);
});

async function select(id: string) {
  currentId.value = id;
  const d = await api.subtitleGet(id);
  cues.value = JSON.parse(JSON.stringify(d.cues));
  dirty.value = false;
}

async function pickFile() {
  const picked = await openDialog({
    multiple: false,
    filters: [{ name: "媒体", extensions: ["mp4", "mov", "mkv", "webm", "mp3", "wav", "m4a"] }],
  });
  if (typeof picked === "string") sourceFile.value = picked;
}

async function transcribe() {
  const file = sourceFile.value || sources.value[0]?.value;
  if (!file) return toast.warn("先选一个要转写的视频 / 音频");
  const s = settings.settings;
  try {
    await api.subtitleTranscribe({
      file,
      language: "zh",
      modelId: s?.asrModel ?? "ggml-large-v3-turbo",
      useGpu: s?.asrUseGpu ?? true,
      threads: s?.asrThreads ?? 0,
      maxLineChars: 18,
      name: baseName(file).replace(/\.[^.]+$/, ""),
    });
    toast.info("已开始转写，完成后会出现在左侧列表");
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function saveCues() {
  if (!currentId.value) return;
  try {
    await api.subtitleSaveCues(currentId.value, cues.value);
    await project.reload();
    dirty.value = false;
    toast.ok("字幕已保存并写回 srt / vtt");
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function removeDoc(id: string) {
  await api.subtitleDelete(id);
  await project.reload();
  if (currentId.value === id) {
    currentId.value = null;
    cues.value = [];
  }
}

async function setAsr(patch: Record<string, unknown>) {
  await settings.save(patch as never);
  caps.value = await api.asrCapabilities();
}
</script>

<template>
  <div class="layout">
    <!-- 左：文档与转写 -->
    <section class="panel list-panel">
      <header class="panel-head">
        <div class="row" style="gap: 6px">
          <span class="section-label">字幕</span>
          <UiBadge tone="neutral" size="xs">{{ docs.length }}</UiBadge>
        </div>
      </header>

      <div class="transcribe col" style="gap: 6px">
        <UiSelect
          v-model="sourceFile"
          :options="sources"
          creatable
          searchable
          placeholder="选择或粘贴要转写的文件"
        />
        <div class="row" style="gap: 5px">
          <UiButton variant="subtle" size="sm" style="flex: 0 0 60px" @click="pickFile">浏览</UiButton>
          <UiButton variant="primary" size="sm" class="grow" @click="transcribe">
            <template #icon><Captions :size="13" /></template>
            开始转写
          </UiButton>
        </div>
      </div>

      <div class="list scroll">
        <button
          v-for="d in docs"
          :key="d.id"
          class="item"
          :class="{ on: d.id === currentId }"
          @click="select(d.id)"
        >
          <FileText :size="14" class="faint" />
          <div class="col grow" style="gap: 1px; min-width: 0">
            <span class="t-sm truncate" style="font-weight: 500">{{ d.name }}</span>
            <span class="t-xs faint truncate">{{ d.cues.length }} 条 · {{ d.model }}</span>
          </div>
        </button>

        <div v-for="j in asrJobs" :key="j.id" class="jobrow">
          <div class="row-between">
            <span class="t-xs truncate">{{ j.title }}</span>
            <UiBadge :tone="j.status === 'failed' ? 'err' : j.status === 'done' ? 'ok' : 'warn'" size="xs">
              {{ j.status }}
            </UiBadge>
          </div>
          <UiProgress :value="j.progress * 100" :height="3" />
        </div>

        <UiEmpty v-if="!docs.length" compact title="还没有字幕" hint="选一个文件点「开始转写」" />
      </div>

      <div class="asrfoot col" style="gap: 7px">
        <div class="row-between">
          <span class="t-xs faint">显卡加速</span>
          <div class="row" style="gap: 6px">
            <span class="t-xs" style="color: var(--accent)">{{ caps?.recommendedBackend ?? "?" }}</span>
            <UiBadge :tone="settings.settings?.asrUseGpu ? 'ok' : 'neutral'" size="xs">
              {{ settings.settings?.asrUseGpu ? "开" : "关" }}
            </UiBadge>
          </div>
        </div>
        <UiSelect
          :model-value="settings.settings?.asrModel ?? null"
          :options="modelOptions"
          placeholder="whisper 模型"
          @update:model-value="(v: string | null) => v && setAsr({ asrModel: v })"
        />
        <div v-if="!model?.downloaded" class="t-xs" style="color: var(--warn)">
          当前模型还没下载，去「设置 → 字幕与显卡」里下载
        </div>
      </div>
    </section>

    <!-- 右：字幕校对 -->
    <section class="panel detail-panel">
      <template v-if="current">
        <header class="panel-head">
          <div class="row" style="gap: 7px; min-width: 0">
            <span class="section-label">{{ current.name }}</span>
            <UiBadge tone="neutral" size="xs">{{ current.language }}</UiBadge>
            <UiBadge tone="neutral" size="xs">{{ current.model }}</UiBadge>
          </div>
          <div class="row" style="gap: 4px">
            <UiButton
              v-if="current.file"
              variant="subtle"
              size="xs"
              @click="openPath(current.file!)"
            >
              打开 srt
            </UiButton>
            <UiButton variant="primary" size="sm" :disabled="!dirty" @click="saveCues">保存</UiButton>
            <UiButton variant="subtle" size="xs" icon @click="removeDoc(current.id)">
              <template #icon><Trash2 :size="12" /></template>
            </UiButton>
          </div>
        </header>

        <div class="cues scroll">
          <div v-for="(c, i) in cues" :key="i" class="cue">
            <div class="times">
              <UiNumber v-model="c.start" :step="0.1" :min="0" suffix="s" @update:model-value="dirty = true" />
              <span class="t-xs faint">→</span>
              <UiNumber v-model="c.end" :step="0.1" :min="0" suffix="s" @update:model-value="dirty = true" />
            </div>
            <UiInput v-model="c.text" @update:model-value="dirty = true" />
            <UiBadge v-if="c.text.length > 18" tone="warn" size="xs">偏长</UiBadge>
          </div>
          <UiEmpty v-if="!cues.length" compact title="这份字幕还没有内容" />
        </div>
      </template>

      <div v-else class="emptywrap col">
        <div v-if="caps" class="card card-pad col" style="gap: 10px; max-width: 560px">
          <div class="row-between">
            <span class="section-label">本机能力</span>
            <UiBadge :tone="caps.whisperCompiled ? 'ok' : 'warn'" size="xs">
              {{ caps.whisperCompiled ? "内置推理已就绪" : "需外部 whisper-cli" }}
            </UiBadge>
          </div>
          <div class="row wrap" style="gap: 6px">
            <UiBadge v-if="caps.nvidiaGpu" tone="info" size="xs"><Cpu :size="9" /> {{ caps.nvidiaGpu }}</UiBadge>
            <UiBadge v-if="caps.cudaAvailable" tone="ok" size="xs">CUDA {{ caps.cudaVersion ?? "" }}</UiBadge>
            <UiBadge tone="neutral" size="xs">{{ caps.cpuThreads }} 线程</UiBadge>
            <UiBadge tone="accent" size="xs">推荐 {{ caps.recommendedBackend }}</UiBadge>
          </div>
          <div v-if="!caps.whisperCompiled" class="warnbox">
            <AlertTriangle :size="13" />
            <div class="t-xs" style="line-height: 1.8">
              当前构建没有内置 whisper 推理。转写需要外部
              <span class="mono">whisper-cli</span>；想要进程内 GPU 加速就用 feature 重新构建
              （<span class="mono">--features asr-cuda</span> 或
              <span class="mono">--features asr-metal</span>，需要 cmake 与 libclang）。
            </div>
          </div>
        </div>
        <UiEmpty title="选一份字幕查看和校对" hint="或在左侧开始转写">
          <template #icon><Captions :size="26" /></template>
          <UiButton variant="outline" size="sm" @click="transcribe">
            <template #icon><Zap :size="13" /></template>
            转写时间线第一个片段
          </UiButton>
        </UiEmpty>
        <UiButton
          v-if="caps && !caps.installedModels.some((m) => m.downloaded)"
          variant="subtle"
          size="xs"
          @click="setAsr({})"
        >
          <template #icon><Download :size="12" /></template>
          去设置里下载模型
        </UiButton>
      </div>
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
.list-panel {
  width: 300px;
  flex: 0 0 300px;
}
.detail-panel {
  flex: 1;
  min-width: 0;
}
.transcribe {
  padding: var(--sp-3);
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
  gap: 8px;
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
.jobrow {
  padding: 7px 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  background: var(--surface-3);
  border-radius: var(--r-sm);
  margin-top: 4px;
}
.asrfoot {
  padding: var(--sp-3);
  border-top: 1px solid var(--line-faint);
}

.cues {
  flex: 1;
  min-height: 0;
  padding: var(--sp-3);
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.cue {
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  padding: 6px 8px;
}
.times {
  display: flex;
  align-items: center;
  gap: 5px;
  flex: 0 0 auto;
}

.emptywrap {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--sp-3);
  padding: var(--sp-5);
}
.warnbox {
  display: flex;
  gap: 8px;
  padding: 9px 11px;
  background: var(--warn-soft);
  border: 1px solid rgba(251, 191, 36, 0.22);
  border-radius: var(--r-sm);
  color: var(--warn);
}
</style>
