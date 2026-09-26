<script setup lang="ts">
/**
 * 3D 预演面板：承载 DirectorDesk（导演台，MIT 开源）。
 *
 * 集成形态：
 * - 它的构建产物随应用分发（scripts/prepare-director.mjs 生成），iframe 承载
 *   （隔离它自己的全局样式），加载后注入换皮样式对齐工作台设计；
 * - 工程随项目保存到 <项目>/previz/director.json（顶部条「保存/恢复工程」）；
 * - agent（自研 / pi 两个引擎）的工具调用经 `@/utils/directorBridge` 落到这里：
 *   桥负责面板没开时自动切过来，本组件只把 `window.__director` 注册给它。
 */
import { onBeforeUnmount, onMounted, ref } from "vue";
import { FolderOpen, Save } from "@lucide/vue";
import UiButton from "@/ui/Button.vue";
import { api, errorText } from "@/api/ipc";
import { toast } from "@/ui";
import { registerDirectorProvider, type DirectorApi } from "@/utils/directorBridge";

const ddFrame = ref<HTMLIFrameElement | null>(null);
const ddSrc = `${import.meta.env.BASE_URL || "/"}director/index.html`;

/** 换皮：把导演台的中性灰主题对齐工作台设计 tokens（在 iframe 内注入覆盖） */
const DIRECTOR_SKIN = `
  :root {
    --panel: #1a1d23 !important;
    --line: #2c3037 !important;
    --muted: #8b93a0 !important;
    --blue: #f2a13c !important;
    color-scheme: dark !important;
  }
  body { background: #16181d !important; color: #e6e8ec !important; }
  button { background: #23262d !important; border-color: #343943 !important; border-radius: 6px !important; }
  button:hover { background: #2c313a !important; border-color: #4a515e !important; }
  input, select { background: #1e2128 !important; border-color: #343943 !important; border-radius: 6px !important; }
  select option { background-color: #1e2128 !important; color: #e6e8ec !important; }
`;

function onDdLoad() {
  const doc = ddFrame.value?.contentDocument;
  if (!doc) return;
  if (!doc.head.querySelector("style[data-workbench-skin]")) {
    const style = doc.createElement("style");
    style.dataset.workbenchSkin = "1";
    style.textContent = DIRECTOR_SKIN;
    doc.head.appendChild(style);
  }
  // iframe 里应用初始化时可能读到的是未布局的尺寸，触发一次 resize 让它按实际大小重排
  const win = ddFrame.value?.contentWindow;
  const kick = () => win?.dispatchEvent(new Event("resize"));
  setTimeout(kick, 200);
  setTimeout(kick, 800);
}

function ddApi(): DirectorApi | null {
  return (
    (ddFrame.value?.contentWindow as unknown as { __director?: DirectorApi } | null)?.__director ??
    null
  );
}

/** 保存导演台工程到项目目录（<项目>/previz/director.json） */
async function saveDirector() {
  const director = ddApi();
  if (!director) return toast.warn("导演台还没加载完成");
  try {
    const doc = director.getDocument();
    const path = await api.directorSave(JSON.stringify(doc));
    toast.ok(`工程已保存到项目：${path}`);
  } catch (e) {
    toast.err(`保存失败：${errorText(e)}`);
  }
}

/** 从项目目录恢复工程（覆盖当前导演台里的内容） */
async function loadDirector() {
  const director = ddApi();
  if (!director) return toast.warn("导演台还没加载完成");
  try {
    const raw = await api.directorLoad();
    if (!raw) return toast.info("这个项目还没有保存过工程，先在导演台里搭好再点「保存工程」");
    director.replaceProject(JSON.parse(raw));
    toast.ok("已从项目恢复工程");
  } catch (e) {
    toast.err(`恢复失败：${errorText(e)}`);
  }
}

onMounted(() => {
  // agent 的工具调用走全局桥；这里只提供「当前 iframe 的 __director」
  registerDirectorProvider(ddApi);
});

onBeforeUnmount(() => {
  registerDirectorProvider(null);
});
</script>

<template>
  <div class="previz">
    <div class="topbar" data-tour="previz-menubar">
      <span class="title">3D 预演 · 导演台</span>
      <UiButton variant="outline" size="sm" @click="saveDirector">
        <Save :size="13" /> 保存工程
      </UiButton>
      <UiButton variant="outline" size="sm" @click="loadDirector">
        <FolderOpen :size="13" /> 恢复工程
      </UiButton>
      <span class="grow" />
      <span class="t-xs faint">
        DirectorDesk（MIT 开源）· 工程随项目保存 · 可以直接让 Simon 替你搭景和设计运镜
      </span>
    </div>
    <iframe
      ref="ddFrame"
      :src="ddSrc"
      class="dd-frame"
      title="导演台 DirectorDesk"
      @load="onDdLoad"
    />
  </div>
</template>

<style scoped>
.previz {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  background: var(--surface-2);
}
.topbar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border-bottom: 1px solid var(--line-faint);
}
.title {
  font-size: var(--t-sm);
  font-weight: 600;
}
.grow {
  flex: 1;
}
.dd-frame {
  flex: 1;
  min-height: 0;
  width: 100%;
  border: none;
  background: #16181d;
}
</style>
