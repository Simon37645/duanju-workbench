<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { NButton, NSelect, NSwitch, NInputNumber, NInput, NProgress, NAlert, NTag } from "naive-ui";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import { useJobsStore } from "@/stores/jobs";
import { api, errorText } from "@/api/ipc";
import { baseName, fmtDuration } from "@/api/events";
import { message } from "@/utils/notify";
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

const timelineSources = computed(() => {
  const tl = project.timeline;
  if (!tl) return [];
  const seen = new Set<string>();
  return tl.tracks
    .flatMap((t) => t.clips)
    .filter((c) => c.source && !seen.has(c.source) && seen.add(c.source))
    .map((c) => ({ label: baseName(c.source), value: c.source }));
});

const asrJobs = computed(() => jobs.jobs.filter((j) => j.kind === "asr").slice(0, 3));

onMounted(async () => {
  caps.value = await api.asrCapabilities();
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
    filters: [{ name: "媒体", extensions: ["mp4", "mov", "mkv", "webm", "mp3", "wav", "m4a", "aac"] }],
  });
  if (typeof picked === "string") sourceFile.value = picked;
}

async function transcribe() {
  const file = sourceFile.value || timelineSources.value[0]?.value;
  if (!file) {
    message.warning("先选一个要转写的视频/音频");
    return;
  }
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
    message.info("已开始转写，完成后会自动出现在左侧列表");
  } catch (e) {
    message.error(errorText(e));
  }
}

async function saveCues() {
  if (!currentId.value) return;
  try {
    await api.subtitleSaveCues(currentId.value, cues.value);
    await project.reload();
    dirty.value = false;
    message.success("字幕已保存并写回 srt / vtt");
  } catch (e) {
    message.error(errorText(e));
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

function splitLong() {
  cues.value = cues.value.map((c) => {
    const t = c.text.trim();
    if (t.length <= 18) return { ...c, text: t };
    const mid = Math.floor(t.length / 2);
    return { ...c, text: t };
  });
  dirty.value = true;
  message.info("已去除首尾空白；自动断句建议由 agent 处理（它能看到上下文）");
}

async function setAsrPref(patch: Record<string, unknown>) {
  await settings.save(patch as never);
  caps.value = await api.asrCapabilities();
}
</script>

<template>
  <div class="wrap">
    <div class="pane col-list">
      <div class="head col" style="gap: 6px">
        <div class="row-between">
          <span style="font-weight: 600">字幕文档（{{ docs.length }}）</span>
        </div>
        <div class="col" style="gap: 4px">
          <n-select
            v-model:value="sourceFile"
            size="tiny"
            filterable
            tag
            placeholder="选择或粘贴要转写的文件路径"
            :options="timelineSources"
          />
          <div class="row" style="gap: 4px">
            <n-button size="tiny" quaternary @click="pickFile">浏览…</n-button>
            <n-button size="tiny" type="primary" class="grow" @click="transcribe">开始转写</n-button>
          </div>
        </div>
      </div>

      <div class="scroll list">
        <div
          v-for="d in docs"
          :key="d.id"
          class="item"
          :class="{ active: d.id === currentId }"
          @click="select(d.id)"
        >
          <div class="row-between">
            <span class="truncate" style="font-weight: 500">{{ d.name }}</span>
            <span class="tag">{{ d.cues.length }} 条</span>
          </div>
          <div class="tiny faint truncate">{{ baseName(d.source) }} · {{ d.model }}</div>
        </div>
        <div v-if="!docs.length" class="tiny faint" style="padding: 16px; text-align: center">
          还没有字幕。选一个文件点「开始转写」。
        </div>

        <div v-for="j in asrJobs" :key="j.id" class="job">
          <div class="row-between">
            <span class="tiny truncate">{{ j.title }}</span>
            <span class="tiny faint">{{ j.status }}</span>
          </div>
          <n-progress type="line" :percentage="Math.round(j.progress * 100)" :height="3" :show-indicator="false" />
          <div class="tiny faint truncate">{{ j.error || j.detail }}</div>
        </div>
      </div>

      <div class="foot col" style="gap: 4px">
        <div class="row" style="gap: 6px">
          <span class="tiny faint">显卡加速</span>
          <n-switch
            size="small"
            :value="settings.settings?.asrUseGpu"
            @update:value="(v: boolean) => setAsrPref({ asrUseGpu: v })"
          />
          <span class="tiny faint">{{ caps?.recommendedBackend ?? "?" }}</span>
        </div>
        <n-select
          size="tiny"
          :value="settings.settings?.asrModel"
          :options="modelOptions"
          placeholder="whisper 模型"
          @update:value="(v: string) => setAsrPref({ asrModel: v })"
        />
      </div>
    </div>

    <div class="pane col-detail">
      <div v-if="current" class="col" style="min-height: 0; flex: 1">
        <div class="head row-between">
          <div class="row" style="gap: 8px; min-width: 0">
            <span style="font-weight: 600">{{ current.name }}</span>
            <span class="tag">{{ current.language }}</span>
            <span class="tag">{{ current.model }}</span>
          </div>
          <div class="row" style="gap: 4px">
            <n-button size="tiny" quaternary @click="splitLong">整理</n-button>
            <n-button size="tiny" type="primary" :disabled="!dirty" @click="saveCues">保存</n-button>
            <n-button size="tiny" quaternary @click="removeDoc(current.id)">删除</n-button>
          </div>
        </div>

        <div class="scroll cuebody">
          <div v-for="(c, i) in cues" :key="i" class="cue">
            <div class="times">
              <n-input-number
                v-model:value="c.start" size="tiny" :min="0" :step="0.1" :show-button="false"
                style="width: 74px" @update:value="dirty = true"
              />
              <span class="tiny faint">→</span>
              <n-input-number
                v-model:value="c.end" size="tiny" :min="0" :step="0.1" :show-button="false"
                style="width: 74px" @update:value="dirty = true"
              />
              <span class="tiny faint">{{ (c.end - c.start).toFixed(1) }}s</span>
            </div>
            <n-input
              v-model:value="c.text" size="small" type="textarea"
              :autosize="{ minRows: 1, maxRows: 3 }"
              @update:value="dirty = true"
            />
            <span v-if="c.text.length > 18" class="tag warn">偏长</span>
          </div>
          <div v-if="!cues.length" class="tiny faint" style="padding: 24px; text-align: center">
            这份字幕还没有内容
          </div>
        </div>
      </div>

      <div v-else class="empty col">
        <n-alert v-if="caps && !caps.whisperCompiled" type="warning" :bordered="false" style="max-width: 620px">
          <div style="font-weight: 600; margin-bottom: 4px">当前构建没有内置 whisper 推理</div>
          <div style="line-height: 1.8">
            转写需要外部 <span class="mono">whisper-cli</span>；
            想要进程内 GPU 加速请带 feature 重新构建：
            <div class="mono" style="margin-top: 4px">
              cargo build --features asr-cuda &nbsp;# Windows / Linux + NVIDIA<br />
              cargo build --features asr-metal &nbsp;# Apple Silicon
            </div>
            需要 cmake 与 libclang。后端是编译期决定的，所以「是否用显卡」这个开关只在
            编译进去的后端范围内生效。
          </div>
        </n-alert>
        <div v-else-if="caps" class="col" style="gap: 6px; max-width: 620px">
          <div class="row" style="gap: 8px">
            <n-tag :type="caps.whisperCompiled ? 'success' : 'warning'" size="small">
              {{ caps.whisperCompiled ? "内置 whisper 已就绪" : "需外部 whisper-cli" }}
            </n-tag>
            <n-tag v-if="caps.nvidiaGpu" size="small" type="info">{{ caps.nvidiaGpu }}</n-tag>
            <n-tag v-if="caps.cudaAvailable" size="small" type="success">
              CUDA {{ caps.cudaVersion ?? "" }}
            </n-tag>
            <n-tag v-if="caps.appleSilicon" size="small" type="success">Apple Silicon</n-tag>
            <n-tag size="small">{{ caps.cpuThreads }} 线程</n-tag>
          </div>
          <div class="tiny faint">
            推荐后端 {{ caps.recommendedBackend }}；已编译后端
            {{ caps.compiledBackends.join("、") || "无" }}。
          </div>
        </div>
        <div class="tiny faint">选一份字幕查看和校对，或先在左侧开始转写</div>
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
  width: 320px;
  flex: 0 0 320px;
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
.col-detail .empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 10px;
  align-items: center;
  justify-content: center;
  padding: 20px;
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
.foot {
  padding: 8px;
  border-top: 1px solid var(--line-soft);
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
.job {
  margin-top: 8px;
  padding: 6px 8px;
  background: var(--bg-1);
  border-radius: 6px;
}
.cuebody {
  flex: 1;
  min-height: 0;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.cue {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  background: var(--bg-1);
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 6px 8px;
}
.times {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: 0 0 auto;
  padding-top: 2px;
}
</style>
