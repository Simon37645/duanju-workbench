<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { CheckCircle2, Plus, Trash2, Zap, Eye, EyeOff } from "@lucide/vue";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiInput from "@/ui/Input.vue";
import UiNumber from "@/ui/NumberInput.vue";
import UiSelect from "@/ui/Select.vue";
import UiSwitch from "@/ui/Switch.vue";
import UiField from "@/ui/Field.vue";
import UiTextarea from "@/ui/Textarea.vue";
import UiModal from "@/ui/Modal.vue";
import UiSegmented from "@/ui/Segmented.vue";
import UiSpinner from "@/ui/Spinner.vue";
import SkillPanel from "@/components/SkillPanel.vue";
import { confirmDialog, toast } from "@/ui";
import { api, errorText } from "@/api/ipc";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import type { AsrCapabilities, ProviderConfig, ProviderKind } from "@/types/models";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ (e: "update:show", v: boolean): void }>();

const settings = useSettingsStore();
const project = useProjectStore();
const tab = ref("project");
const asr = ref<AsrCapabilities | null>(null);
const sidecar = ref<Record<string, { ok: boolean; path?: string; error?: string }> | null>(null);
const proxy = ref<Awaited<ReturnType<typeof api.proxyStatus>> | null>(null);
const netResult = ref("");
const testing = ref(false);
const revealKey = ref(false);

const editing = reactive<Partial<ProviderConfig>>({});
const editingId = ref<string | null>(null);
const apiKeyDraft = ref("");
const optionsText = ref("");

const varHint = "{{变量}}";

/* ------------------------------------------------------------ 预设 */
const PRESETS = [
  {
    key: "openai-image",
    label: "图片生成 · OpenAI 兼容",
    kind: "image" as ProviderKind,
    make: () => ({
      name: "图片生成（OpenAI 兼容）", adapter: "generic-http",
      baseUrl: "https://api.example.com", model: "gpt-image-1",
      options: {
        size: "1024x1536",
        submit: { method: "POST", path: "/v1/images/generations", body: { model: "{{model}}", prompt: "{{prompt}}", size: "{{size}}", n: 1, background: "opaque" } },
        result: { urlPath: ["/data/0/url"], b64Path: ["/data/0/b64_json"] },
      },
    }),
  },
  {
    key: "comfyui-firstlast",
    label: "首尾帧生视频 · ComfyUI 工作流",
    kind: "video" as ProviderKind,
    make: () => ({
      name: "首尾帧生视频（ComfyUI）", adapter: "generic-http",
      baseUrl: "https://comfy.example.com", model: "my_first_last_frame_workflow",
      options: {
        authStyle: "raw",
        aspectValues: { "768x1344": "768p_portrait", "1344x768": "768p_landscape", "768x768": "768p_square" },
        upload: { mode: "chunked", beforePath: "/api/v1/file/before_upload", initPath: "/api/v1/file", chunkPath: "/api/v1/file", field: "file", refTemplate: "{{md5}}" },
        submit: { method: "POST", path: "/api/v1/workflow/first_last_frame", body: { prompt: "{{prompt}}", first_frame: "{{image1}}", last_frame: "{{image2}}", duration: "{{durationInt}}", resolution: "{{aspect}}" } },
        taskIdPath: "/data/task_id",
        poll: { method: "GET", path: "/api/v1/workflow/result/{{taskId}}", statusPath: "/data/status", successValues: ["SUCCESS", "completed"], failureValues: ["FAILED"], intervalSec: 6, timeoutSec: 1800 },
        result: { urlPath: ["/data/results/0/url"] },
      },
    }),
  },
  {
    key: "comfyui-multi",
    label: "多图参考生视频 · ComfyUI 工作流",
    kind: "video" as ProviderKind,
    make: () => ({
      name: "多图参考生视频（ComfyUI）", adapter: "generic-http",
      baseUrl: "https://comfy.example.com", model: "my_multi_ref_workflow",
      options: {
        authStyle: "raw",
        aspectValues: { "768x1344": "768p_portrait", "1344x768": "768p_landscape", "768x768": "768p_square" },
        upload: { mode: "chunked", beforePath: "/api/v1/file/before_upload", initPath: "/api/v1/file", chunkPath: "/api/v1/file", field: "file", refTemplate: "{{md5}}" },
        submit: {
          method: "POST", path: "/api/v1/workflow/multi_ref",
          body: {
            prompt: "{{prompt}}", seed: "{{seed}}", duration: "{{durationInt}}", resolution: "{{aspect}}",
            ref_image_0: "{{image1}}", ref_image_1: "{{image2}}", ref_image_2: "{{image3}}",
            ref_image_3: "{{image4}}", ref_image_4: "{{image5}}", ref_image_5: "{{image6}}",
            ref_image_6: "{{image7}}", ref_image_7: "{{image8}}", ref_image_8: "{{image9}}",
          },
        },
        taskIdPath: "/data/task_id",
        poll: { method: "GET", path: "/api/v1/workflow/result/{{taskId}}", statusPath: "/data/status", successValues: ["SUCCESS", "completed"], failureValues: ["FAILED"], intervalSec: 6, timeoutSec: 1800 },
        result: { urlPath: ["/data/results/0/url"] },
      },
    }),
  },
];

const MODE_HINT: Record<string, string> = {
  yolo: "任何操作都直接执行，不打断你",
  auto: "自动改数据，只有生图 / 生视频这类花钱的才确认",
  confirm: "任何会改项目数据的操作都先问你",
};

const TABS = [
  { label: "项目", value: "project" },
  { label: "模型供应商", value: "providers" },
  { label: "技能", value: "skills" },
  { label: "字幕与显卡", value: "asr" },
  { label: "运行环境", value: "runtime" },
];

/* ------------------------------------------------------------ 项目 */

const proj = reactive({
  name: "",
  genre: "",
  logline: "",
  episodeCountHint: 0 as number | null,
  aspectRatio: "9:16",
});

watch(
  () => [props.show, project.manifest] as const,
  () => {
    const m = project.manifest;
    if (!m) return;
    Object.assign(proj, {
      name: m.name,
      genre: m.genre,
      logline: m.logline,
      episodeCountHint: m.episodeCountHint,
      aspectRatio: m.aspectRatio,
    });
  },
  { immediate: true },
);

async function saveProjectInfo() {
  try {
    await api.projectUpdateManifest({ ...proj, episodeCountHint: proj.episodeCountHint ?? 0 });
    await project.reload();
    toast.ok("项目信息已保存");
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function openProjectFolder() {
  try {
    const paths = await api.projectPaths(project.root);
    await openPath(paths.root);
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function openOtherProject() {
  const picked = await openDialog({ directory: true, multiple: false, title: "选择项目目录" });
  if (typeof picked !== "string") return;
  try {
    await project.open(picked);
    toast.ok("项目已打开");
    emit("update:show", false);
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function closeProject() {
  const ok = await confirmDialog({
    title: "关闭项目",
    content: "数据都在磁盘上，不会丢失，之后可以从项目列表再打开。",
    positiveText: "关闭",
  });
  if (!ok) return;
  await project.close();
  emit("update:show", false);
  location.hash = "#/";
}

const adapterOptions = (kind?: ProviderKind) =>
  kind === "llm"
    ? [
        { label: "OpenAI 兼容（DeepSeek / Qwen / GLM / vLLM…）", value: "openai" },
        { label: "Anthropic Claude 原生", value: "anthropic" },
        { label: "占位模型（不联网）", value: "mock" },
      ]
    : [
        { label: "通用 HTTP（配置驱动）", value: "generic-http" },
        { label: "占位生成（ffmpeg 造素材）", value: "mock" },
      ];

const kindOptions = [
  { label: "文本模型（agent）", value: "llm" },
  { label: "生图", value: "image" },
  { label: "生视频", value: "video" },
];

const KIND_LABEL: Record<string, string> = { llm: "文本", image: "生图", video: "生视频" };
const KIND_TONE: Record<string, "info" | "ok" | "violet"> = { llm: "info", image: "ok", video: "violet" };

watch(
  () => props.show,
  async (v) => {
    if (!v) return;
    await settings.load();
    asr.value = await api.asrCapabilities();
    sidecar.value = await api.mediaSidecarStatus();
    proxy.value = await api.proxyStatus();
  },
);

function newProvider(kind: ProviderKind) {
  editingId.value = null;
  Object.assign(editing, {
    id: "", kind, name: "", adapter: kind === "llm" ? "openai" : "generic-http",
    baseUrl: "", model: "", apiKeyRef: "", concurrency: 2, timeoutSec: 600,
    enabled: true, options: {},
  });
  apiKeyDraft.value = "";
  optionsText.value = "";
}

function editProvider(p: ProviderConfig) {
  editingId.value = p.id;
  Object.assign(editing, JSON.parse(JSON.stringify(p)));
  apiKeyDraft.value = "";
  optionsText.value = JSON.stringify(p.options ?? {}, null, 2);
}

async function applyPreset(p: (typeof PRESETS)[number]) {
  const cfg = p.make();
  const saved = await settings.upsertProvider({
    id: "", kind: p.kind, name: cfg.name!, adapter: cfg.adapter!,
    baseUrl: cfg.baseUrl!, apiKeyRef: "", model: cfg.model!,
    concurrency: 2, timeoutSec: p.kind === "video" ? 1800 : 600,
    enabled: true, options: cfg.options ?? {},
  } as ProviderConfig);
  editProvider(saved);
  toast.ok(`已加入「${saved.name}」，补上 API Key 再保存`);
}

async function save() {
  if (!editing.name?.trim()) return toast.warn("请填写名称");
  let options: Record<string, unknown> = {};
  if (optionsText.value.trim()) {
    try {
      options = JSON.parse(optionsText.value);
    } catch {
      return toast.err("options 不是合法 JSON");
    }
  }
  const saved = await settings.upsertProvider({ ...(editing as ProviderConfig), options });
  if (apiKeyDraft.value) {
    await settings.setSecret(saved.apiKeyRef, apiKeyDraft.value);
    apiKeyDraft.value = "";
  }
  editingId.value = saved.id;
  toast.ok("已保存");
}

async function removeProvider(id: string) {
  await settings.deleteProvider(id);
  if (editingId.value === id) editingId.value = null;
}

const activeIdFor = (kind: ProviderKind) =>
  kind === "llm" ? settings.settings?.activeLlmProviderId
    : kind === "image" ? settings.settings?.activeImageProviderId
      : settings.settings?.activeVideoProviderId;

async function setActive(p: ProviderConfig) {
  const patch =
    p.kind === "llm" ? { activeLlmProviderId: p.id }
      : p.kind === "image" ? { activeImageProviderId: p.id }
        : { activeVideoProviderId: p.id };
  await settings.save(patch);
}

async function test(p: ProviderConfig) {
  await settings.test(p.id);
}

async function setAsr(patch: Record<string, unknown>) {
  await settings.save(patch as never);
  asr.value = await api.asrCapabilities();
}

async function restartTour() {
  await settings.save({ onboarded: false });
  emit("update:show", false);
  toast.info("关掉设置后会重新弹出新手教程");
}

async function testNet(url?: string) {
  testing.value = true;
  netResult.value = "测试中…";
  try {
    netResult.value = await api.netTest(url);
  } catch (e) {
    netResult.value = errorText(e);
  } finally {
    testing.value = false;
    proxy.value = await api.proxyStatus();
  }
}

async function openModelsDir() {
  if (!asr.value?.modelsDir) return;
  try {
    await openPath(asr.value.modelsDir);
  } catch (e) {
    toast.err(`打不开目录：${errorText(e)}`);
  }
}

const asrBackendOptions = computed(() => [
  { label: "自动（按显卡推荐）", value: "auto" },
  { label: `CPU（${asr.value?.cpuThreads ?? "?"} 线程）`, value: "cpu" },
  { label: `CUDA / NVIDIA${asr.value?.cudaAvailable ? "" : "（未检测到）"}`, value: "cuda" },
  { label: "Metal / Apple Silicon", value: "metal" },
  { label: "Vulkan", value: "vulkan" },
]);
</script>

<template>
  <UiModal :show="props.show" drawer :width="620" @update:show="(v: boolean) => emit('update:show', v)">
    <div class="col" style="gap: var(--sp-4)">
      <div class="row-between">
        <h2 style="font-size: var(--t-lg); font-weight: 600">设置</h2>
        <UiSegmented v-model="tab" :items="TABS" />
      </div>

      <!-- ==================================================== 项目 -->
      <template v-if="tab === 'project'">
        <div class="card card-pad col" style="gap: 12px">
          <span class="section-label">项目信息</span>
          <div class="grid2" style="display: grid; grid-template-columns: 1fr 1fr; gap: 10px">
            <UiField label="项目名"><UiInput v-model="proj.name" /></UiField>
            <UiField label="题材"><UiInput v-model="proj.genre" /></UiField>
          </div>
          <UiField label="一句话卖点"><UiInput v-model="proj.logline" /></UiField>
          <div class="grid2" style="display: grid; grid-template-columns: 1fr 1fr; gap: 10px">
            <UiField label="计划章节数"><UiNumber v-model="proj.episodeCountHint" :min="0" /></UiField>
            <UiField label="画幅（如 9:16）"><UiInput v-model="proj.aspectRatio" /></UiField>
          </div>
          <div class="row" style="justify-content: flex-end">
            <UiButton variant="primary" size="sm" @click="saveProjectInfo">保存项目信息</UiButton>
          </div>
        </div>

        <div class="card card-pad col" style="gap: 10px">
          <span class="section-label">项目目录</span>
          <div class="kv">
            <span class="faint">位置</span><span class="truncate" :title="project.root">{{ project.root || "未打开项目" }}</span>
          </div>
          <div class="row" style="gap: 8px; flex-wrap: wrap">
            <UiButton variant="outline" size="sm" @click="openProjectFolder">在文件管理器打开</UiButton>
            <UiButton variant="outline" size="sm" @click="openOtherProject">打开其他项目</UiButton>
            <UiButton variant="subtle" size="sm" @click="closeProject">关闭项目</UiButton>
          </div>
          <p class="t-xs faint" style="line-height: 1.7">
            项目就是一个普通文件夹：JSON 存结构化数据、Markdown 存正文、媒体就是媒体文件，整个目录拷走就能在另一台机器打开。
          </p>
        </div>
      </template>

      <!-- ==================================================== 供应商 -->
      <template v-else-if="tab === 'providers'">
        <div class="hint">
          生图 / 生视频用「通用 HTTP」适配器 —— 请求地址、body 模板、轮询与结果字段都写在
          options 里，换一家接口只需要改配置。详细说明见 <span class="mono">docs/PROVIDER.md</span>。
        </div>

        <div class="card card-pad col" style="gap: 10px">
          <div class="row-between">
            <span class="section-label">快速预设</span>
            <span class="t-xs faint">点一下填好接口，补上密钥即可</span>
          </div>
          <div class="row wrap">
            <UiButton v-for="p in PRESETS" :key="p.key" variant="outline" size="sm" @click="applyPreset(p)">
              <template #icon><Zap :size="13" /></template>
              {{ p.label }}
            </UiButton>
          </div>
        </div>

        <div class="col" style="gap: 8px">
          <div class="row-between">
            <span class="section-label">已配置（{{ settings.providers.length }}）</span>
            <div class="row" style="gap: 4px">
              <UiButton v-for="k in (['llm', 'image', 'video'] as ProviderKind[])" :key="k" variant="subtle" size="xs" @click="newProvider(k)">
                <template #icon><Plus :size="12" /></template>
                {{ KIND_LABEL[k] }}
              </UiButton>
            </div>
          </div>

          <div v-for="p in settings.providers" :key="p.id" class="prov">
            <div class="row grow" style="gap: 7px; min-width: 0">
              <UiBadge :tone="KIND_TONE[p.kind]" size="xs">{{ KIND_LABEL[p.kind] }}</UiBadge>
              <span class="t-sm truncate" style="font-weight: 500">{{ p.name }}</span>
              <span class="t-xs faint truncate" style="max-width: 150px">{{ p.model }}</span>
              <UiBadge v-if="activeIdFor(p.kind) === p.id" tone="accent" size="xs">使用中</UiBadge>
              <UiBadge v-if="settings.secrets[p.apiKeyRef]" tone="ok" size="xs">
                <CheckCircle2 :size="9" /> 密钥
              </UiBadge>
            </div>
            <div class="row" style="gap: 2px; flex: 0 0 auto">
              <UiButton v-if="activeIdFor(p.kind) !== p.id" variant="subtle" size="xs" @click="setActive(p)">启用</UiButton>
              <UiButton variant="subtle" size="xs" @click="test(p)">测试</UiButton>
              <UiButton variant="subtle" size="xs" @click="editProvider(p)">编辑</UiButton>
              <UiButton variant="subtle" size="xs" icon @click="removeProvider(p.id)">
                <template #icon><Trash2 :size="12" /></template>
              </UiButton>
            </div>
          </div>
        </div>

        <!-- 编辑表单 -->
        <div v-if="editing.kind" class="card card-pad col" style="gap: 12px">
          <div class="row-between">
            <span class="section-label">{{ editingId ? "编辑供应商" : "新增供应商" }}</span>
            <UiButton variant="subtle" size="xs" @click="editing.kind = undefined">收起</UiButton>
          </div>

          <div class="grid2">
            <UiField label="类型">
              <UiSelect v-model="editing.kind" :options="kindOptions" />
            </UiField>
            <UiField label="适配器">
              <UiSelect v-model="editing.adapter" :options="adapterOptions(editing.kind)" />
            </UiField>
          </div>

          <UiField label="名称"><UiInput v-model="editing.name" placeholder="显示名" /></UiField>

          <div class="grid2">
            <UiField label="Base URL"><UiInput v-model="editing.baseUrl" placeholder="https://api.example.com" /></UiField>
            <UiField label="模型名"><UiInput v-model="editing.model" placeholder="gpt-image-1" /></UiField>
          </div>

          <UiField label="API Key" :hint="settings.secrets[editing.apiKeyRef ?? ''] ? '已保存，留空表示不修改' : ''">
            <div class="row" style="gap: 6px">
              <UiInput
                v-model="apiKeyDraft"
                :password="!revealKey"
                placeholder="粘贴密钥"
              />
              <UiButton variant="subtle" size="sm" icon @click="revealKey = !revealKey">
                <template #icon>
                  <EyeOff v-if="revealKey" :size="13" />
                  <Eye v-else :size="13" />
                </template>
              </UiButton>
            </div>
          </UiField>

          <div class="grid2">
            <UiField label="并发数"><UiNumber v-model="editing.concurrency" :min="1" :max="16" /></UiField>
            <UiField label="超时（秒）"><UiNumber v-model="editing.timeoutSec" :min="30" :max="3600" /></UiField>
          </div>

          <UiField label="启用"><UiSwitch v-model="editing.enabled" /></UiField>

          <UiField
            label="options"
            :hint="editing.kind === 'llm' ? 'maxTokens / temperature / headers' : '请求模板与轮询配置'"
          >
            <UiTextarea v-model="optionsText" :rows="8" mono placeholder='{ "submit": { … } }' />
          </UiField>

          <div class="row" style="justify-content: flex-end">
            <UiButton variant="ghost" @click="editing.kind = undefined">取消</UiButton>
            <UiButton variant="primary" @click="save">保存</UiButton>
          </div>
        </div>
      </template>

      <!-- ======================================================== 技能 -->
      <template v-else-if="tab === 'skills'">
        <SkillPanel />
      </template>

      <!-- ======================================================== 字幕 -->
      <template v-else-if="tab === 'asr'">
        <div v-if="asr" class="col" style="gap: var(--sp-3)">
          <div class="card card-pad col" style="gap: 10px">
            <div class="row-between">
              <span class="section-label">本机检测</span>
              <UiBadge :tone="asr.whisperCompiled ? 'ok' : 'warn'" size="xs">
                {{ asr.whisperCompiled ? "内置推理已就绪" : "需外部 whisper-cli" }}
              </UiBadge>
            </div>
            <div class="kv">
              <span class="faint">显卡</span><span>{{ asr.nvidiaGpu ?? "未检测到" }}</span>
              <span class="faint">CUDA</span><span>{{ asr.cudaAvailable ? asr.cudaVersion ?? "可用" : "不可用" }}</span>
              <span class="faint">Apple Silicon</span><span>{{ asr.appleSilicon ? "是" : "否" }}</span>
              <span class="faint">CPU 线程</span><span class="num">{{ asr.cpuThreads }}</span>
              <span class="faint">推荐后端</span><span style="color: var(--accent)">{{ asr.recommendedBackend }}</span>
            </div>
            <div v-if="!asr.whisperCompiled" class="note">
              当前构建没有内置 whisper 推理，转写需要外部 <span class="mono">whisper-cli</span>。
              想要进程内 GPU 加速，用 feature 重新构建：
              <div class="mono" style="margin-top: 5px; line-height: 1.8">
                cargo build --features asr-cuda &nbsp;# Windows + NVIDIA<br />
                cargo build --features asr-metal &nbsp;# Apple Silicon
              </div>
            </div>
          </div>

          <div class="card card-pad col" style="gap: 12px">
            <span class="section-label">推理设置</span>
            <UiField label="加速后端">
              <UiSelect
                :model-value="settings.settings?.asrBackend ?? 'auto'"
                :options="asrBackendOptions"
                @update:model-value="(v: string | null) => setAsr({ asrBackend: v })"
              />
            </UiField>
            <UiField label="使用显卡" inline hint="关掉就是纯 CPU">
              <UiSwitch
                :model-value="settings.settings?.asrUseGpu"
                @update:model-value="(v: boolean) => setAsr({ asrUseGpu: v })"
              />
            </UiField>
            <div class="grid2">
              <UiField label="线程数" hint="0 = 自动">
                <UiNumber
                  :model-value="settings.settings?.asrThreads"
                  :min="0" :max="64"
                  @update:model-value="(v: number | null) => setAsr({ asrThreads: v ?? 0 })"
                />
              </UiField>
              <UiField label="whisper-cli 路径">
                <UiInput
                  :model-value="settings.settings?.whisperCliPath"
                  placeholder="留空自动查找"
                  @update:model-value="(v: string) => setAsr({ whisperCliPath: v })"
                />
              </UiField>
            </div>
          </div>

          <div class="card card-pad col" style="gap: 12px">
            <span class="section-label">模型</span>
            <UiField label="下载源" :hint="'国内访问不了 HuggingFace 时换镜像'">
              <UiInput
                :model-value="settings.settings?.asrModelBaseUrl"
                @update:model-value="(v: string) => setAsr({ asrModelBaseUrl: v })"
              />
            </UiField>
            <div class="row" style="gap: 6px">
              <UiButton variant="subtle" size="xs" @click="setAsr({ asrModelBaseUrl: 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main' })">官方</UiButton>
              <UiButton variant="subtle" size="xs" @click="setAsr({ asrModelBaseUrl: 'https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main' })">hf-mirror</UiButton>
              <span class="grow" />
              <UiButton variant="subtle" size="xs" @click="openModelsDir">打开模型目录</UiButton>
            </div>

            <div v-for="m in asr.installedModels" :key="m.id" class="mrow">
              <div class="row" style="gap: 7px; min-width: 0">
                <UiBadge :tone="m.downloaded ? 'ok' : 'neutral'" size="xs">
                  {{ m.downloaded ? "已下载" : m.sizeMb + " MB" }}
                </UiBadge>
                <span class="t-sm truncate">{{ m.label }}</span>
              </div>
              <div class="row" style="gap: 4px">
                <UiButton v-if="!m.downloaded" variant="outline" size="xs" @click="api.asrDownloadModel(m.id).then(() => toast.info('已加入下载队列，见底部任务栏'))">
                  下载
                </UiButton>
                <template v-else>
                  <UiButton
                    :variant="settings.settings?.asrModel === m.id ? 'primary' : 'subtle'"
                    size="xs"
                    @click="setAsr({ asrModel: m.id })"
                  >
                    {{ settings.settings?.asrModel === m.id ? "使用中" : "使用" }}
                  </UiButton>
                  <UiButton variant="subtle" size="xs" @click="api.asrDeleteModel(m.id).then(() => api.asrCapabilities().then((c) => (asr = c)))">
                    删除
                  </UiButton>
                </template>
              </div>
            </div>
          </div>
        </div>
        <UiSpinner v-else label="正在检测…" />
      </template>

      <!-- ==================================================== 运行环境 -->
      <template v-else>
        <div class="col" style="gap: var(--sp-3)">
          <div class="card card-pad col" style="gap: 12px">
            <div class="row-between">
              <span class="section-label">网络代理</span>
              <UiBadge :tone="proxy?.effective ? 'ok' : 'neutral'" size="xs">
                {{ proxy?.effective ? "走代理" : "直连" }}
              </UiBadge>
            </div>
            <div class="note">
              v2rayN / Clash 通常只改系统代理、不写环境变量。默认的「跟随系统」会自己读出来。
              本机地址永远直连，本地 vLLM / Ollama 不受影响。
            </div>
            <UiField label="代理模式">
              <UiSelect
                :model-value="settings.settings?.proxyMode ?? 'auto'"
                :options="[
                  { label: '跟随系统 / 环境变量（推荐）', value: 'auto' },
                  { label: '直连', value: 'off' },
                  { label: '手动指定', value: 'manual' },
                ]"
                @update:model-value="(v: string | null) => settings.save({ proxyMode: (v ?? 'auto') as never }).then(() => api.proxyStatus().then((x) => (proxy = x)))"
              />
            </UiField>
            <UiField v-if="settings.settings?.proxyMode === 'manual'" label="代理地址">
              <UiInput
                :model-value="settings.settings?.proxyUrl"
                placeholder="http://127.0.0.1:10808"
                @update:model-value="(v: string) => settings.save({ proxyUrl: v })"
              />
            </UiField>
            <div class="kv">
              <span class="faint">系统代理</span><span class="truncate">{{ proxy?.systemProxy ?? "未检测到" }}</span>
              <span class="faint">环境变量</span><span class="truncate">{{ proxy?.envProxy ?? "未设置" }}</span>
              <span class="faint">实际使用</span><span style="color: var(--accent)" class="truncate">{{ proxy?.effective ?? "直连" }}</span>
            </div>
            <div class="row" style="gap: 6px">
              <UiButton variant="outline" size="sm" :loading="testing" @click="testNet()">测试连通性</UiButton>
              <UiButton variant="subtle" size="sm" :loading="testing" @click="testNet('https://hf-mirror.com/')">测模型镜像</UiButton>
            </div>
            <div v-if="netResult" class="t-xs mono" style="line-height: 1.7; white-space: pre-wrap">{{ netResult }}</div>
          </div>

          <div class="card card-pad col" style="gap: 12px">
            <span class="section-label">ffmpeg</span>
            <div v-for="(v, k) in sidecar ?? {}" :key="k" class="mrow">
              <div class="row" style="gap: 7px">
                <UiBadge :tone="v.ok ? 'ok' : 'err'" size="xs">{{ v.ok ? "就绪" : "缺失" }}</UiBadge>
                <span class="mono t-sm">{{ k }}</span>
              </div>
              <span class="t-xs faint truncate" style="max-width: 280px" :title="v.path || v.error">
                {{ v.path || v.error }}
              </span>
            </div>
            <UiField label="ffmpeg 路径">
              <UiInput :model-value="settings.settings?.ffmpegPath" placeholder="留空用内置 sidecar" @update:model-value="(v: string) => settings.save({ ffmpegPath: v })" />
            </UiField>
            <UiField label="ffprobe 路径">
              <UiInput :model-value="settings.settings?.ffprobePath" placeholder="留空用内置 sidecar" @update:model-value="(v: string) => settings.save({ ffprobePath: v })" />
            </UiField>
          </div>

          <div class="card card-pad col" style="gap: 12px">
            <span class="section-label">新手教程</span>
            <div class="row-between">
              <span class="t-xs faint" style="line-height: 1.7">
                第一次使用时自动弹出，随时可以重看一遍
              </span>
              <UiButton variant="outline" size="sm" @click="restartTour">重新过一遍</UiButton>
            </div>
          </div>

          <div class="card card-pad col" style="gap: 12px">
            <span class="section-label">Agent 行为</span>
            <UiField label="权限模式" :hint="MODE_HINT[settings.settings?.agentMode ?? 'auto']">
              <UiSegmented
                :model-value="settings.settings?.agentMode ?? 'auto'"
                :items="[
                  { label: 'YOLO', value: 'yolo' },
                  { label: '自动编辑', value: 'auto' },
                  { label: '变更前确认', value: 'confirm' },
                ]"
                @update:model-value="(v: string) => settings.save({ agentMode: v as never })"
              />
            </UiField>
            <UiField label="最大工具轮次">
              <UiNumber
                :model-value="settings.settings?.maxToolRounds"
                :min="1" :max="40"
                @update:model-value="(v: number | null) => settings.save({ maxToolRounds: v ?? 12 })"
              />
            </UiField>
            <UiField
              label="上下文预算"
              :hint="'单次对话允许占用的 token 估算，超过 75% 自动压缩；当前 ' + ((settings.settings?.contextBudget ?? 0) / 1000) + 'k'"
            >
              <UiNumber
                :model-value="settings.settings?.contextBudget"
                :min="8000" :max="500000" :step="8000"
                @update:model-value="(v: number | null) => settings.save({ contextBudget: v ?? 64000 })"
              />
            </UiField>
            <UiField label="自动压缩" inline hint="超预算时自动清理旧工具结果、必要时做摘要">
              <UiSwitch
                :model-value="settings.settings?.autoCompact"
                @update:model-value="(v: boolean) => settings.save({ autoCompact: v })"
              />
            </UiField>
          </div>
        </div>
      </template>
    </div>
  </UiModal>
</template>

<style scoped>
.hint {
  font-size: var(--t-sm);
  color: var(--fg-dim);
  line-height: 1.75;
  padding: 10px 12px;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
}
.note {
  font-size: var(--t-xs);
  color: var(--fg-faint);
  line-height: 1.75;
  padding: 9px 11px;
  background: var(--surface-3);
  border-radius: var(--r-sm);
  border-left: 2px solid var(--accent-line);
}
.prov {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  padding: 8px 10px;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
}
.prov:hover {
  border-color: var(--line-strong);
}
.mrow {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-3);
  padding: 7px 0;
  border-bottom: 1px solid var(--line-faint);
}
.mrow:last-child {
  border-bottom: none;
}
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--sp-3);
}
.kv {
  display: grid;
  grid-template-columns: 110px 1fr;
  gap: 5px 12px;
  font-size: var(--t-sm);
}
</style>
