<script setup lang="ts">
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { NButton, NInput, NEmpty, NSpin, NPopover } from "naive-ui";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import { errorText } from "@/api/ipc";
import { fmtTime } from "@/api/events";
import { message } from "@/utils/notify";

const router = useRouter();
const project = useProjectStore();
const settings = useSettingsStore();

const newName = ref("");
const newParent = ref("");
const busy = ref(false);
const showCreate = ref(false);

const recents = computed(() => settings.recentProjects);

async function pickParent() {
  const picked = await openDialog({ directory: true, multiple: false, title: "选择项目存放位置" });
  if (typeof picked === "string") newParent.value = picked;
}

async function create() {
  if (!newName.value.trim()) {
    message.warning("请填写项目名");
    return;
  }
  if (!newParent.value) {
    message.warning("请选择存放位置");
    return;
  }
  busy.value = true;
  try {
    await project.create(newParent.value, newName.value.trim());
    await settings.load();
    router.push({ name: "workspace", params: { panel: "script" } });
  } catch {
    /* 已在 store 里提示 */
  } finally {
    busy.value = false;
  }
}

async function openProject(path: string) {
  busy.value = true;
  try {
    await project.open(path);
    await settings.load();
    router.push({ name: "workspace", params: { panel: "script" } });
  } catch {
    /* ignore */
  } finally {
    busy.value = false;
  }
}

async function browseAndOpen() {
  const picked = await openDialog({
    directory: true,
    multiple: false,
    title: "选择已有的项目目录（含 project.json）",
  });
  if (typeof picked === "string") await openProject(picked);
}

function fromError(e: unknown) {
  return errorText(e);
}
void fromError;
</script>

<template>
  <div class="home">
    <div class="hero">
      <div class="row" style="gap: 12px">
        <div class="logo">短</div>
        <div class="col" style="gap: 2px">
          <h1 style="font-size: 22px">短剧工作台</h1>
          <div class="dim small">
            剧本 → 风格 → 分镜 → 资产 → 提示词 → 生视频 → 剪辑 → 字幕 → Checklist，
            每一步都有 agent 深度参与
          </div>
        </div>
      </div>
    </div>

    <div class="body">
      <div class="pane section">
        <div class="row-between" style="margin-bottom: 10px">
          <span style="font-weight: 600">打开项目</span>
          <div class="row">
            <n-button size="small" @click="browseAndOpen">浏览目录…</n-button>
            <n-button size="small" type="primary" @click="showCreate = !showCreate">
              {{ showCreate ? "收起" : "新建项目" }}
            </n-button>
          </div>
        </div>

        <div v-if="showCreate" class="create">
          <div class="row">
            <n-input v-model:value="newName" size="small" placeholder="项目名，例如 示例短剧" style="max-width: 320px" />
            <n-button size="small" @click="pickParent">选择位置…</n-button>
            <span class="tiny faint truncate" style="max-width: 260px">{{ newParent || "未选择" }}</span>
            <n-button size="small" type="primary" :loading="busy" @click="create">创建</n-button>
          </div>
          <div class="tiny faint" style="margin-top: 6px">
            会在该位置创建一个以项目名命名的目录，里面是纯文件结构（JSON / Markdown / 媒体），
            整个目录拷走就能在别的机器打开，包括 Apple Silicon。
          </div>
        </div>

        <div v-if="recents.length" class="list">
          <div v-for="r in recents" :key="r.path" class="row item">
            <div class="col grow" style="gap: 0; min-width: 0">
              <div class="row" style="gap: 6px">
                <span style="font-weight: 500">{{ r.name }}</span>
                <span class="tiny faint">{{ fmtTime(r.openedAt) }}</span>
              </div>
              <div class="tiny faint truncate" :title="r.path">{{ r.path }}</div>
            </div>
            <n-button size="tiny" type="primary" :loading="busy" @click="openProject(r.path)">
              打开
            </n-button>
          </div>
        </div>
        <n-empty v-else-if="!showCreate" description="还没有项目" style="padding: 28px 0" />
      </div>

      <div class="pane section">
        <div style="font-weight: 600; margin-bottom: 8px">开始之前</div>
        <div class="tips">
          <div class="tip">
            <span class="n">1</span>
            <div>
              <div>配置模型接口</div>
              <div class="tiny faint">
                右上角「设置」里加文本模型（OpenAI 兼容或 Claude）与生图 / 生视频接口。
                没有接口也能用：不配文本模型时 agent 走占位模型，不配生图接口时用 ffmpeg 造占位素材，
                整条流程照样能跑通。
              </div>
            </div>
          </div>
          <div class="tip">
            <span class="n">2</span>
            <div>
              <div>关于缓存</div>
              <div class="tiny faint">
                每个面板的对话都有一份冻结的「项目圣经 + 资产索引」前缀，整场对话不变，
                所以连续对话时大部分输入 token 都能命中服务端前缀缓存。数据改了想让 agent 看到，
                让它自己调工具去读，或者点对话坞的 ↻ 重建上下文（会牺牲一次命中）。
              </div>
            </div>
          </div>
          <div class="tip">
            <span class="n">3</span>
            <div>
              <div>关于显卡加速</div>
              <div class="tiny faint">
                字幕转写用本地 whisper。是否用显卡在设置里勾，但后端本身是编译期决定的
                （CUDA / Metal / Vulkan），换机器要按 docs 里的说明重新构建。
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.home {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: radial-gradient(1200px 500px at 20% -10%, rgba(232, 163, 61, 0.08), transparent), var(--bg);
}
.hero {
  padding: 34px 40px 18px;
}
.logo {
  width: 46px;
  height: 46px;
  border-radius: 12px;
  background: linear-gradient(140deg, #f0b040, #c97f24);
  color: #17171d;
  display: grid;
  place-items: center;
  font-weight: 700;
  font-size: 20px;
}
.body {
  flex: 1;
  overflow: auto;
  padding: 0 40px 40px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 980px;
}
.section {
  padding: 16px;
}
.create {
  background: var(--bg-3);
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 10px;
}
.list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.item {
  padding: 8px 10px;
  border-radius: 6px;
}
.item:hover {
  background: var(--bg-3);
}
.tips {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.tip {
  display: flex;
  gap: 10px;
  line-height: 1.75;
}
.n {
  width: 20px;
  height: 20px;
  flex: 0 0 auto;
  border-radius: 50%;
  background: var(--accent-soft);
  color: var(--accent);
  display: grid;
  place-items: center;
  font-size: 11px;
  margin-top: 2px;
}
</style>
