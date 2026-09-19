<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import {
  NButton, NDrawer, NDrawerContent, NForm, NFormItem, NInput, NInputNumber,
  NSelect, NSwitch, NTag, NDivider, NAlert, NTabs, NTabPane, NProgress, NPopover,
} from "naive-ui";
import { useSettingsStore } from "@/stores/settings";
import { api, errorText } from "@/api/ipc";
import { openPath } from "@tauri-apps/plugin-opener";
import { message } from "@/utils/notify";
import type { AsrCapabilities, ProviderConfig, ProviderKind } from "@/types/models";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ (e: "update:show", v: boolean): void }>();

const settings = useSettingsStore();
const asr = ref<AsrCapabilities | null>(null);
const sidecar = ref<Record<string, { ok: boolean; path?: string; error?: string }> | null>(null);
const secrets = computed(() => settings.secrets);

const editing = reactive<Partial<ProviderConfig>>({});
const editingId = ref<string | null>(null);
const apiKeyDraft = ref("");
const optionsText = ref("");
const tab = ref("providers");

/** 模板变量提示。放在 script 里是因为直接在模板里写两层花括号会被 Vue 解析器截断 */
const varHint = "{{变量}}";

/* ------------------------------------------------------------------ 预设
 * 这两个接口都是实测文档里抄下来的字段，一键填好后只要补 API Key 即可。
 * 画幅映射按短剧常用的 9:16（768×1344 → 768p_portrait）。 */

const PRESETS: Array<{
  key: string;
  label: string;
  hint: string;
  make: () => Partial<ProviderConfig>;
}> = [
  {
    key: "simon-image",
    label: "示例 API · GPT 生图",
    hint: "OpenAI 兼容 /v1/images/generations，gpt-image 系列只认固定尺寸，默认竖屏 1024×1536",
    make: () => ({
      kind: "image" as ProviderKind,
      name: "示例 API · GPT 生图",
      adapter: "generic-http",
      baseUrl: "https://api.example.com",
      model: "gpt-image-1",
      concurrency: 2,
      timeoutSec: 600,
      enabled: true,
      options: {
        // 登录后按实际额度选：gpt-image-1 / gpt-image-2 / gpt-image-2-1k /
        // gpt-image-1-flare / gpt-image-1-sunburst
        size: "1024x1536",
        submit: {
          method: "POST",
          path: "/v1/images/generations",
          body: {
            model: "{{model}}",
            prompt: "{{prompt}}",
            size: "{{size}}",
            n: 1,
            background: "opaque",
          },
        },
        result: {
          urlPath: ["/data/0/url", "/data/0/b64_json"],
          b64Path: ["/data/0/b64_json"],
        },
      },
    }),
  },
  {
    key: "comfyui-firstlast",
    label: "第三方平台 · H3 首尾帧生视频",
    hint: "my_first_last_frame_workflow：首帧 + 尾帧 + 提示词。图片要先上传换成引用值",
    make: () => ({
      kind: "video" as ProviderKind,
      name: "第三方平台 · H3 首尾帧生视频",
      adapter: "generic-http",
      baseUrl: "https://comfy.example.com",
      model: "my_first_last_frame_workflow",
      concurrency: 2,
      timeoutSec: 1800,
      enabled: true,
      options: {
        authStyle: "raw",
        aspectValues: {
          "768x1344": "768p_portrait",
          "1344x768": "768p_landscape",
          "768x768": "768p_square",
          "480x864": "480p竖",
          "864x480": "480p横",
        },
        upload: {
          mode: "chunked",
          beforePath: "/api/v1/file/before_upload",
          initPath: "/api/v1/file",
          chunkPath: "/api/v1/file",
          field: "file",
          refTemplate: "{{md5}}",
        },
        submit: {
          method: "POST",
          path: "/api/v1/comfyui/workflow/my_first_last_frame_workflow",
          body: {
            prompt: "{{prompt}}",
            first_frame: "{{image1}}",
            last_frame: "{{image2}}",
            duration: "{{durationInt}}",
            resolution: "{{aspect}}",
          },
        },
        taskIdPath: "/data/task_id",
        poll: {
          method: "GET",
          path: "/api/v1/comfyui/comfyui_workflow/result/{{taskId}}",
          statusPath: "/data/status",
          successValues: ["SUCCESS", "success", "completed"],
          failureValues: ["FAILED", "failed"],
          intervalSec: 6,
          timeoutSec: 1800,
        },
        result: { urlPath: ["/data/results/0/url"] },
      },
    }),
  },
  {
    key: "comfyui-multi",
    label: "第三方平台 · H3 多图参考生视频",
    hint: "my_multi_ref_workflow：最多 9 张参考图，适合用人物三视图 + 场景图保持一致性",
    make: () => ({
      kind: "video" as ProviderKind,
      name: "第三方平台 · H3 多图参考生视频",
      adapter: "generic-http",
      baseUrl: "https://comfy.example.com",
      model: "my_multi_ref_workflow",
      concurrency: 2,
      timeoutSec: 1800,
      enabled: true,
      options: {
        authStyle: "raw",
        aspectValues: {
          "768x1344": "768p_portrait",
          "1344x768": "768p_landscape",
          "768x768": "768p_square",
          "1080x1920": "1080p竖",
          "1920x1080": "1080p横",
        },
        upload: {
          mode: "chunked",
          beforePath: "/api/v1/file/before_upload",
          initPath: "/api/v1/file",
          chunkPath: "/api/v1/file",
          field: "file",
          refTemplate: "{{md5}}",
        },
        submit: {
          method: "POST",
          path: "/api/v1/comfyui/workflow/my_multi_ref_workflow",
          body: {
            prompt: "{{prompt}}",
            seed: "{{seed}}",
            duration: "{{durationInt}}",
            resolution: "{{aspect}}",
            ref_image_0: "{{image1}}",
            ref_image_1: "{{image2}}",
            ref_image_2: "{{image3}}",
            ref_image_3: "{{image4}}",
            ref_image_4: "{{image5}}",
            ref_image_5: "{{image6}}",
            ref_image_6: "{{image7}}",
            ref_image_7: "{{image8}}",
            ref_image_8: "{{image9}}",
          },
        },
        taskIdPath: "/data/task_id",
        poll: {
          method: "GET",
          path: "/api/v1/comfyui/comfyui_workflow/result/{{taskId}}",
          statusPath: "/data/status",
          successValues: ["SUCCESS", "success", "completed"],
          failureValues: ["FAILED", "failed"],
          intervalSec: 6,
          timeoutSec: 1800,
        },
        result: { urlPath: ["/data/results/0/url"] },
      },
    }),
  },
];

async function applyPreset(p: (typeof PRESETS)[number]) {
  const cfg = p.make();
  const saved = await settings.upsertProvider({
    id: "",
    kind: cfg.kind!,
    name: cfg.name!,
    adapter: cfg.adapter!,
    baseUrl: cfg.baseUrl ?? "",
    apiKeyRef: "",
    model: cfg.model ?? "",
    concurrency: cfg.concurrency ?? 2,
    timeoutSec: cfg.timeoutSec ?? 600,
    enabled: true,
    options: cfg.options ?? {},
  } as ProviderConfig);
  editProvider(saved);
  message.success(`已加入「${saved.name}」，补上 API Key 再点保存`);
}

const adapterOptions = (kind?: ProviderKind) =>  kind === "llm"
    ? [
        { label: "OpenAI 兼容（DeepSeek/Qwen/GLM/Kimi/vLLM…）", value: "openai" },
        { label: "Anthropic Claude 原生", value: "anthropic" },
        { label: "占位模型（不调用网络）", value: "mock" },
      ]
    : [
        { label: "通用 HTTP（配置驱动，见下方模板）", value: "generic-http" },
        { label: "占位生成（用 ffmpeg 造素材）", value: "mock" },
      ];

const GENERIC_TEMPLATE = {
  submit: {
    method: "POST",
    path: "/v1/images/generations",
    headers: {},
    body: {
      model: "{{model}}",
      prompt: "{{prompt}}",
      negative_prompt: "{{negative}}",
      width: "{{width}}",
      height: "{{height}}",
      seed: "{{seed}}",
      image: "{{image1}}",
    },
  },
  taskIdPath: "/data/0/task_id",
  poll: {
    method: "GET",
    path: "/v1/tasks/{{taskId}}",
    statusPath: "/data/status",
    successValues: ["succeeded", "success"],
    failureValues: ["failed", "error"],
    intervalSec: 5,
    timeoutSec: 900,
  },
  result: {
    urlPath: ["/data/0/url", "/output/url"],
    b64Path: ["/data/0/b64_json"],
  },
};

const VIDEO_TEMPLATE = {
  submit: {
    method: "POST",
    path: "/v1/videos/generations",
    headers: {},
    body: {
      model: "{{model}}",
      prompt: "{{prompt}}",
      duration: "{{duration}}",
      first_frame: "{{image1}}",
      last_frame: "{{image2}}",
    },
  },
  taskIdPath: "/task_id",
  poll: {
    method: "GET",
    path: "/v1/videos/{{taskId}}",
    statusPath: "/status",
    successValues: ["succeeded"],
    failureValues: ["failed"],
    progressPath: "/progress",
    intervalSec: 8,
    timeoutSec: 1800,
  },
  result: { urlPath: ["/video_url", "/data/0/url"] },
};

watch(
  () => props.show,
  async (v) => {
    if (!v) return;
    await settings.load();
    asr.value = await api.asrCapabilities();
    sidecar.value = await api.mediaSidecarStatus();
    await loadProxy();
  },
);

function newProvider(kind: ProviderKind) {
  editingId.value = null;
  Object.assign(editing, {
    id: "",
    kind,
    name: kind === "llm" ? "新模型供应商" : kind === "image" ? "新生图供应商" : "新生视频供应商",
    adapter: kind === "llm" ? "openai" : "generic-http",
    baseUrl: "",
    model: "",
    apiKeyRef: "",
    concurrency: 2,
    timeoutSec: 600,
    enabled: true,
    options: {},
  } as Partial<ProviderConfig>);
  apiKeyDraft.value = "";
  optionsText.value = "";
}

function editProvider(p: ProviderConfig) {
  editingId.value = p.id;
  Object.assign(editing, JSON.parse(JSON.stringify(p)));
  apiKeyDraft.value = "";
  optionsText.value = JSON.stringify(p.options ?? {}, null, 2);
}

async function save() {
  if (!editing.name?.trim()) {
    message.warning("请填写名称");
    return;
  }
  let options: Record<string, unknown> = {};
  if (optionsText.value.trim()) {
    try {
      options = JSON.parse(optionsText.value);
    } catch (e) {
      message.error(`options 不是合法 JSON：${errorText(e)}`);
      return;
    }
  }
  const saved = await settings.upsertProvider({ ...(editing as ProviderConfig), options });
  if (apiKeyDraft.value) {
    await settings.setSecret(saved.apiKeyRef, apiKeyDraft.value);
    apiKeyDraft.value = "";
  }
  editingId.value = saved.id;
  message.success("已保存");
}

async function remove(id: string) {
  await settings.deleteProvider(id);
  if (editingId.value === id) editingId.value = null;
  message.success("已删除");
}

async function downloadModel(id: string) {
  try {
    await api.asrDownloadModel(id);
    message.info("已加入下载队列，进度见底部任务栏");
  } catch (e) {
    message.error(errorText(e));
  }
}

async function setAsr(patch: Record<string, unknown>) {
  await settings.save(patch as never);
  asr.value = await api.asrCapabilities();
}

async function openModelsDir() {
  const dir = asr.value?.modelsDir;
  if (!dir) return;
  try {
    await openPath(dir);
  } catch (e) {
    message.error(`打不开目录（可能还没创建）：${errorText(e)}`);
  }
}

const proxy = ref<Awaited<ReturnType<typeof api.proxyStatus>> | null>(null);
const netResult = ref("");
const testing = ref(false);

async function loadProxy() {
  try {
    proxy.value = await api.proxyStatus();
  } catch {
    proxy.value = null;
  }
}

async function setProxy(patch: Record<string, unknown>) {
  await settings.save(patch as never);
  await loadProxy();
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
    await loadProxy();
  }
}

const asrBackendOptions = computed(() => {
  const a = asr.value;
  return [
    { label: "自动（按显卡推荐）", value: "auto" },
    { label: `CPU（${a?.cpuThreads ?? "?"} 线程）`, value: "cpu" },
    { label: `CUDA / NVIDIA${a?.cudaAvailable ? "" : "（未检测到）"}`, value: "cuda" },
    { label: `Metal / Apple Silicon${a?.appleSilicon ? "" : "（非本机）"}`, value: "metal" },
    { label: "Vulkan", value: "vulkan" },
  ];
});
</script>

<template>
  <n-drawer
    :show="props.show"
    :width="720"
    placement="right"
    @update:show="(v: boolean) => emit('update:show', v)"
  >
    <n-drawer-content title="设置" :native-scrollbar="false">
      <n-tabs v-model:value="tab" type="line">
        <!-- ============================================ 供应商 -->
        <n-tab-pane name="providers" tab="模型供应商">
          <div class="col">
            <n-alert type="info" :bordered="false">
              模型接口全部在这里配。生图 / 生视频用「通用 HTTP」适配器，
              把接口的请求地址、请求体模板、轮询与结果字段填进 options 就能接上，不用改代码。
              详细的模板说明见 <span class="mono">docs/PROVIDER.md</span>。
            </n-alert>

            <div class="row wrap">
              <n-button size="small" @click="newProvider('llm')">＋ 文本模型（agent）</n-button>
              <n-button size="small" @click="newProvider('image')">＋ 生图</n-button>
              <n-button size="small" @click="newProvider('video')">＋ 生视频</n-button>
            </div>

            <div class="pane" style="padding: 10px">
              <div class="row-between" style="margin-bottom: 6px">
                <span style="font-weight: 600">快速预设</span>
                <span class="tiny faint">点一下填好接口，补上 API Key 就能用</span>
              </div>
              <div class="row wrap" style="gap: 6px">
                <n-popover v-for="p in PRESETS" :key="p.key" trigger="hover" placement="bottom">
                  <template #trigger>
                    <n-button size="small" type="primary" ghost @click="applyPreset(p)">
                      {{ p.label }}
                    </n-button>
                  </template>
                  <div style="max-width: 320px; font-size: 12px; line-height: 1.7">
                    {{ p.hint }}
                  </div>
                </n-popover>
              </div>
            </div>

            <div v-for="p in settings.providers" :key="p.id" class="pane prov">
              <div class="row-between">
                <div class="row" style="gap: 6px; min-width: 0">
                  <n-tag size="small" :type="p.kind === 'llm' ? 'info' : p.kind === 'image' ? 'success' : 'warning'">
                    {{ p.kind === "llm" ? "文本" : p.kind === "image" ? "生图" : "生视频" }}
                  </n-tag>
                  <span style="font-weight: 600">{{ p.name }}</span>
                  <span class="tiny faint mono truncate" style="max-width: 200px">{{ p.adapter }}</span>
                  <span class="tiny faint truncate" style="max-width: 220px">{{ p.baseUrl }}</span>
                  <span class="tag" :class="secrets[p.apiKeyRef] ? 'ok' : ''">
                    {{ secrets[p.apiKeyRef] ? "已配密钥" : "无密钥" }}
                  </span>
                  <span v-if="!p.enabled" class="tag warn">已停用</span>
                </div>
                <div class="row" style="gap: 4px">
                  <n-button size="tiny" quaternary @click="settings.test(p.id)">测试</n-button>
                  <n-button size="tiny" quaternary @click="editProvider(p)">编辑</n-button>
                  <n-button size="tiny" quaternary @click="remove(p.id)">删除</n-button>
                </div>
              </div>
              <div class="row" style="gap: 6px; margin-top: 6px">
                <span class="tiny faint">当前启用：</span>
                <n-button
                  size="tiny"
                  :type="(p.kind === 'llm' ? settings.settings?.activeLlmProviderId : p.kind === 'image' ? settings.settings?.activeImageProviderId : settings.settings?.activeVideoProviderId) === p.id ? 'primary' : 'default'"
                  @click="
                    settings.save(
                      p.kind === 'llm'
                        ? { activeLlmProviderId: p.id }
                        : p.kind === 'image'
                          ? { activeImageProviderId: p.id }
                          : { activeVideoProviderId: p.id },
                    )
                  "
                >
                  {{ (p.kind === 'llm' ? settings.settings?.activeLlmProviderId : p.kind === 'image' ? settings.settings?.activeImageProviderId : settings.settings?.activeVideoProviderId) === p.id ? '使用中' : '设为当前' }}
                </n-button>
              </div>
            </div>

            <n-divider v-if="editing.kind" style="margin: 4px 0" />

            <!-- 编辑表单 -->
            <div v-if="editing.kind" class="pane" style="padding: 12px">
              <div style="font-weight: 600; margin-bottom: 8px">
                {{ editingId ? "编辑供应商" : "新增供应商" }}
              </div>
              <n-form label-placement="left" label-width="88" size="small">
                <n-form-item label="类型">
                  <n-select v-model:value="editing.kind" :options="[
                    { label: '文本模型（agent）', value: 'llm' },
                    { label: '生图', value: 'image' },
                    { label: '生视频', value: 'video' },
                  ]" />
                </n-form-item>
                <n-form-item label="适配器">
                  <n-select v-model:value="editing.adapter" :options="adapterOptions(editing.kind)" />
                </n-form-item>
                <n-form-item label="名称">
                  <n-input v-model:value="editing.name" placeholder="显示名，随便起" />
                </n-form-item>
                <n-form-item label="Base URL">
                  <n-input v-model:value="editing.baseUrl" placeholder="https://api.example.com/v1" />
                </n-form-item>
                <n-form-item label="模型名">
                  <n-input v-model:value="editing.model" placeholder="deepseek-chat / claude-sonnet-4-5 / your-model" />
                </n-form-item>
                <n-form-item label="API Key">
                  <n-input
                    v-model:value="apiKeyDraft"
                    type="password"
                    show-password-on="click"
                    :placeholder="secrets[editing.apiKeyRef ?? ''] ? '已保存（留空表示不修改）' : '粘贴密钥'"
                  />
                </n-form-item>
                <n-form-item label="并发 / 超时">
                  <div class="row">
                    <n-input-number v-model:value="editing.concurrency" :min="1" :max="16" style="width: 110px" />
                    <n-input-number v-model:value="editing.timeoutSec" :min="30" :max="3600" style="width: 130px" />
                    <span class="tiny faint">秒</span>
                  </div>
                </n-form-item>
                <n-form-item label="启用">
                  <n-switch v-model:value="editing.enabled" />
                </n-form-item>
                <n-form-item label="options">
                  <div class="col grow" style="width: 100%">
                    <div class="row" v-if="editing.kind !== 'llm'">
                      <n-button
                        size="tiny"
                        @click="optionsText = JSON.stringify(editing.kind === 'video' ? VIDEO_TEMPLATE : GENERIC_TEMPLATE, null, 2)"
                      >
                        填入{{ editing.kind === "video" ? "视频" : "图片" }}模板
                      </n-button>
                      <span class="tiny faint">
                        模板里的 <span class="mono" v-text="varHint"></span> 会被自动替换
                      </span>
                    </div>
                    <n-input
                      v-model:value="optionsText"
                      type="textarea"
                      :autosize="{ minRows: 4, maxRows: 16 }"
                      placeholder='{"submit": {...}}'
                      class="mono"
                    />
                  </div>
                </n-form-item>
                <div class="row">
                  <n-button type="primary" size="small" @click="save">保存</n-button>
                  <n-button size="small" quaternary @click="editing.kind = undefined">取消</n-button>
                </div>
              </n-form>
            </div>
          </div>
        </n-tab-pane>

        <!-- ============================================ 字幕 -->
        <n-tab-pane name="asr" tab="字幕 / 显卡加速">
          <div v-if="asr" class="col">
            <div class="pane" style="padding: 12px">
              <div class="row-between">
                <span style="font-weight: 600">本机检测</span>
                <span class="tag" :class="asr.whisperCompiled ? 'ok' : 'warn'">
                  {{ asr.whisperCompiled ? "已编译 whisper 推理" : "未编译 whisper（用外部 CLI）" }}
                </span>
              </div>
              <div class="grid">
                <div>系统</div><div class="dim">{{ asr.os }} / {{ asr.arch }}</div>
                <div>CPU 线程</div><div class="dim">{{ asr.cpuThreads }}</div>
                <div>NVIDIA 显卡</div>
                <div class="dim">{{ asr.nvidiaGpu || "未检测到" }}</div>
                <div>CUDA</div>
                <div class="dim">{{ asr.cudaAvailable ? `可用（${asr.cudaVersion || "版本未知"}）` : "不可用" }}</div>
                <div>Apple Silicon</div><div class="dim">{{ asr.appleSilicon ? "是" : "否" }}</div>
                <div>Metal</div><div class="dim">{{ asr.metalSupported ? "系统支持" : "不支持" }}</div>
                <div>编译进去的后端</div>
                <div class="dim">{{ asr.compiledBackends.join("、") || "无" }}</div>
                <div>外部 whisper-cli</div>
                <div class="dim truncate">{{ asr.externalCliPath || "未找到" }}</div>
                <div>推荐后端</div><div class="accent">{{ asr.recommendedBackend }}</div>
              </div>
              <n-alert v-if="!asr.whisperCompiled" type="warning" :bordered="false" style="margin-top: 10px">
                当前构建没有内置 whisper 推理，转写需要外部 whisper-cli。
                想要进程内 GPU 加速，用 feature 重新构建，例如
                <span class="mono">cargo build --features asr-cuda</span>（Windows + NVIDIA）
                或 <span class="mono">--features asr-metal</span>（Apple Silicon）。
                这些后端是编译期决定的，所以换卡要重新编译。
              </n-alert>
            </div>

            <div class="pane" style="padding: 12px">
              <div style="font-weight: 600; margin-bottom: 8px">推理设置</div>
              <n-form label-placement="left" label-width="96" size="small">
                <n-form-item label="加速后端">
                  <n-select
                    :value="settings.settings?.asrBackend"
                    :options="asrBackendOptions"
                    @update:value="(v: string) => setAsr({ asrBackend: v })"
                  />
                </n-form-item>
                <n-form-item label="使用显卡">
                  <div class="row">
                    <n-switch
                      :value="settings.settings?.asrUseGpu"
                      @update:value="(v: boolean) => setAsr({ asrUseGpu: v })"
                    />
                    <span class="tiny faint">
                      关掉就是纯 CPU；开了会走编译期选定的后端
                    </span>
                  </div>
                </n-form-item>
                <n-form-item label="线程数">
                  <n-input-number
                    :value="settings.settings?.asrThreads"
                    :min="0"
                    :max="64"
                    style="width: 120px"
                    @update:value="(v: number | null) => setAsr({ asrThreads: v ?? 0 })"
                  />
                  <span class="tiny faint" style="margin-left: 8px">0 = 自动</span>
                </n-form-item>
                <n-form-item label="whisper-cli">
                  <n-input
                    :value="settings.settings?.whisperCliPath"
                    placeholder="留空则自动在应用目录和 PATH 里找"
                    @update:value="(v: string) => setAsr({ whisperCliPath: v })"
                  />
                </n-form-item>
                <n-form-item label="模型目录">
                  <div class="row" style="gap: 6px; min-width: 0">
                    <span class="tiny faint mono truncate" style="max-width: 300px" :title="asr.modelsDir">
                      {{ asr.modelsDir }}
                    </span>
                    <n-button size="tiny" quaternary @click="openModelsDir">打开</n-button>
                    <span class="tiny faint">下载不了时，手动把 ggml-*.bin 放进来即可</span>
                  </div>
                </n-form-item>
              </n-form>
            </div>

            <div class="pane" style="padding: 12px">
              <div style="font-weight: 600; margin-bottom: 8px">模型</div>
              <n-form label-placement="left" label-width="96" size="small" style="margin-bottom: 8px">
                <n-form-item label="下载源">
                  <div class="col grow" style="width: 100%">
                    <n-input
                      :value="settings.settings?.asrModelBaseUrl"
                      placeholder="https://huggingface.co/ggerganov/whisper.cpp/resolve/main"
                      @update:value="(v: string) => setAsr({ asrModelBaseUrl: v })"
                    />
                    <div class="row" style="gap: 6px; margin-top: 4px">
                      <n-button
                        size="tiny"
                        quaternary
                        @click="setAsr({ asrModelBaseUrl: 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main' })"
                      >
                        官方
                      </n-button>
                      <n-button
                        size="tiny"
                        quaternary
                        @click="setAsr({ asrModelBaseUrl: 'https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main' })"
                      >
                        hf-mirror 镜像
                      </n-button>
                      <span class="tiny faint">访问不了 huggingface 时换镜像</span>
                    </div>
                  </div>
                </n-form-item>
              </n-form>
              <div v-for="m in asr.installedModels" :key="m.id" class="row-between model">
                <div class="row" style="gap: 8px; min-width: 0">
                  <span class="tag" :class="m.downloaded ? 'ok' : ''">
                    {{ m.downloaded ? "已下载" : `${m.sizeMb} MB` }}
                  </span>
                  <span class="small truncate">{{ m.label }}</span>
                  <span class="tiny faint mono">{{ m.id }}</span>
                </div>
                <div class="row" style="gap: 4px">
                  <n-button
                    v-if="!m.downloaded"
                    size="tiny"
                    @click="downloadModel(m.id)"
                  >
                    下载
                  </n-button>
                  <template v-else>
                    <n-button
                      size="tiny"
                      :type="settings.settings?.asrModel === m.id ? 'primary' : 'default'"
                      @click="setAsr({ asrModel: m.id })"
                    >
                      {{ settings.settings?.asrModel === m.id ? "使用中" : "使用" }}
                    </n-button>
                    <n-button size="tiny" quaternary @click="api.asrDeleteModel(m.id).then(() => api.asrCapabilities().then((c) => (asr = c)))">
                      删除
                    </n-button>
                  </template>
                </div>
              </div>
            </div>
          </div>
          <div v-else class="tiny faint">正在检测…</div>
        </n-tab-pane>

        <!-- ============================================ 运行环境 -->
        <n-tab-pane name="runtime" tab="运行环境">
          <div class="col">
            <div class="pane" style="padding: 12px">
              <div class="row-between" style="margin-bottom: 8px">
                <span style="font-weight: 600">网络代理</span>
                <span class="tag" :class="proxy?.effective ? 'ok' : ''">
                  {{ proxy?.effective ? "走代理" : "直连" }}
                </span>
              </div>
              <n-alert type="info" :bordered="false" style="margin-bottom: 10px">
                v2rayN / Clash 这类工具通常只改系统代理，不走环境变量。
                默认的「跟随系统」会自己把系统代理读出来，所以一般不用手动配。
                本机地址（127.0.0.1 等）永远直连，本地的 vLLM / Ollama 不会被代理拦掉。
              </n-alert>
              <n-form label-placement="left" label-width="96" size="small">
                <n-form-item label="代理模式">
                  <n-select
                    :value="settings.settings?.proxyMode"
                    :options="[
                      { label: '跟随系统 / 环境变量（推荐）', value: 'auto' },
                      { label: '直连（不走代理）', value: 'off' },
                      { label: '手动指定', value: 'manual' },
                    ]"
                    @update:value="(v: string) => setProxy({ proxyMode: v })"
                  />
                </n-form-item>
                <n-form-item v-if="settings.settings?.proxyMode === 'manual'" label="代理地址">
                  <n-input
                    :value="settings.settings?.proxyUrl"
                    placeholder="http://127.0.0.1:10808 或 socks5://127.0.0.1:10808"
                    @update:value="(v: string) => settings.save({ proxyUrl: v })"
                  />
                </n-form-item>
              </n-form>
              <div class="grid">
                <div>系统代理</div>
                <div class="dim truncate">{{ proxy?.systemProxy || "未检测到" }}</div>
                <div>环境变量</div>
                <div class="dim truncate">{{ proxy?.envProxy || "未设置" }}</div>
                <div>实际使用</div>
                <div class="accent truncate">{{ proxy?.effective || "直连" }}</div>
              </div>
              <div class="row" style="margin-top: 10px; gap: 6px">
                <n-button size="small" :loading="testing" @click="testNet()">测试连通性</n-button>
                <n-button size="small" quaternary :loading="testing" @click="testNet('https://hf-mirror.com/')">
                  测模型镜像
                </n-button>
              </div>
              <div v-if="netResult" class="tiny" style="margin-top: 8px; line-height: 1.7">
                {{ netResult }}
              </div>
            </div>

            <div class="pane" style="padding: 12px">
              <div style="font-weight: 600; margin-bottom: 8px">ffmpeg（剪辑 / 转码 / 导出）</div>
              <div v-for="(v, k) in sidecar ?? {}" :key="k" class="row-between model">
                <div class="row" style="gap: 8px">
                  <span class="tag" :class="v.ok ? 'ok' : 'err'">{{ v.ok ? "就绪" : "缺失" }}</span>
                  <span class="mono small">{{ k }}</span>
                </div>
                <span class="tiny faint truncate" style="max-width: 380px" :title="v.path || v.error">
                  {{ v.path || v.error }}
                </span>
              </div>
              <n-alert type="info" :bordered="false" style="margin-top: 10px">
                发行版会随包带上 ffmpeg / ffprobe。开发期如果取不到 sidecar，会自动回退到系统 PATH。
                也可以在下面手动指定路径。
              </n-alert>
              <n-form label-placement="left" label-width="96" size="small" style="margin-top: 10px">
                <n-form-item label="ffmpeg 路径">
                  <n-input
                    :value="settings.settings?.ffmpegPath"
                    @update:value="(v: string) => settings.save({ ffmpegPath: v })"
                  />
                </n-form-item>
                <n-form-item label="ffprobe 路径">
                  <n-input
                    :value="settings.settings?.ffprobePath"
                    @update:value="(v: string) => settings.save({ ffprobePath: v })"
                  />
                </n-form-item>
              </n-form>
            </div>

            <div class="pane" style="padding: 12px">
              <div style="font-weight: 600; margin-bottom: 8px">Agent 行为</div>
              <n-form label-placement="left" label-width="96" size="small">
                <n-form-item label="花钱需确认">
                  <div class="row">
                    <n-switch
                      :value="settings.settings?.confirmCostlyTools"
                      @update:value="(v: boolean) => settings.save({ confirmCostlyTools: v })"
                    />
                    <span class="tiny faint">生图 / 生视频这类会消耗额度的工具，执行前先问一次</span>
                  </div>
                </n-form-item>
                <n-form-item label="最大工具轮次">
                  <n-input-number
                    :value="settings.settings?.maxToolRounds"
                    :min="1"
                    :max="40"
                    style="width: 120px"
                    @update:value="(v: number | null) => settings.save({ maxToolRounds: v ?? 12 })"
                  />
                </n-form-item>
              </n-form>
            </div>
          </div>
        </n-tab-pane>
      </n-tabs>
    </n-drawer-content>
  </n-drawer>
</template>

<style scoped>
.pane {
  border-radius: 8px;
}
.prov {
  padding: 10px;
}
.grid {
  display: grid;
  grid-template-columns: 130px 1fr;
  gap: 4px 10px;
  margin-top: 8px;
  font-size: 12px;
}
.model {
  padding: 6px 0;
  border-bottom: 1px solid var(--line-soft);
}
.model:last-child {
  border-bottom: none;
}
.accent {
  color: var(--accent);
}
</style>
