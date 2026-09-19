<script setup lang="ts">
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  ArrowRight, BookOpen, Clock, FolderOpen, FolderPlus, KeyRound, Play, Sparkles, Zap,
} from "@lucide/vue";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import { api, errorText } from "@/api/ipc";
import { fmtTime } from "@/api/events";
import { toast } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiInput from "@/ui/Input.vue";
import UiBadge from "@/ui/Badge.vue";
import UiEmpty from "@/ui/Empty.vue";

const router = useRouter();
const project = useProjectStore();
const settings = useSettingsStore();

const busy = ref(false);
const newName = ref("");
const newParent = ref("");

const recents = computed(() => settings.recentProjects);
const hasModel = computed(() => settings.byKind("llm").length > 0);

const FLOW = [
  { name: "剧本", desc: "拆章节、写大纲与正文" },
  { name: "风格", desc: "定画面风格圣经，下游共享" },
  { name: "分镜", desc: "把剧本拆成可拍的镜头表" },
  { name: "资产", desc: "人物三视图、场景、道具出图" },
  { name: "提示词", desc: "写运动提示词并配参考图" },
  { name: "生视频", desc: "调视频模型按镜头出片" },
  { name: "剪辑", desc: "铺时间线、裁切、导出成片" },
  { name: "字幕", desc: "本地 whisper 转写与校对" },
  { name: "核对", desc: "Checklist 过一遍再交片" },
];

async function pickParent() {
  const picked = await openDialog({ directory: true, multiple: false, title: "项目存放位置" });
  if (typeof picked === "string") newParent.value = picked;
}

async function create() {
  if (!newName.value.trim()) return toast.warn("请填写项目名");
  if (!newParent.value) return toast.warn("请选择存放位置");
  busy.value = true;
  try {
    await project.create(newParent.value, newName.value.trim());
    await settings.load();
    router.push({ name: "workspace", params: { panel: "script" } });
  } catch {
    /* store 已提示 */
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
  } catch (e) {
    toast.err(errorText(e));
  } finally {
    busy.value = false;
  }
}

async function browse() {
  const picked = await openDialog({ directory: true, multiple: false, title: "选择项目目录" });
  if (typeof picked === "string") await openProject(picked);
}
</script>

<template>
  <div class="home scroll">
    <div class="wrap">
      <!-- 顶栏：品牌 + 全局动作 -->
      <header class="bar">
        <div class="row" style="gap: 10px; min-width: 0">
          <div class="mark">S</div>
          <div class="col" style="gap: 1px; min-width: 0">
            <h1>Simon 短剧工作台</h1>
            <span class="t-xs faint truncate">从剧本到成片的九个面板，Simon 全程陪你做</span>
          </div>
        </div>
        <div class="row" style="gap: 6px; flex: 0 0 auto">
          <UiBadge v-if="!hasModel" tone="warn" size="xs">未配置模型</UiBadge>
          <UiButton variant="subtle" size="sm" @click="router.push({ name: 'home' })" v-if="false" />
          <UiButton variant="outline" size="sm" @click="browse">
            <template #icon><FolderOpen :size="13" /></template>
            打开项目
          </UiButton>
        </div>
      </header>

      <div class="grid">
        <!-- 主列 -->
        <main class="col" style="gap: var(--sp-4)">
          <!-- 新建 -->
          <section class="card card-pad col" style="gap: 12px">
            <div class="row-between">
              <span class="section-label">新建项目</span>
              <span class="t-xs faint">一个项目 = 一个普通文件夹</span>
            </div>
            <div class="newrow">
              <UiInput v-model="newName" size="md" placeholder="项目名，例如 示例短剧" @enter="create" />
              <UiButton variant="outline" size="md" @click="pickParent">
                <template #icon><FolderPlus :size="13" /></template>
                位置
              </UiButton>
              <UiButton variant="primary" size="md" :loading="busy" @click="create">
                创建并开始
                <template #icon><ArrowRight :size="13" /></template>
              </UiButton>
            </div>
            <div class="path t-xs faint truncate" :title="newParent">
              {{ newParent || "还没有选择存放位置 —— 会在该位置创建同名目录" }}
            </div>
          </section>

          <!-- 最近项目 -->
          <section class="card col" style="gap: 0; overflow: hidden">
            <div class="row-between card-pad" style="padding-bottom: 10px">
              <span class="section-label">最近打开</span>
              <span class="t-xs faint">{{ recents.length }} 个</span>
            </div>
            <div v-if="recents.length" class="recents">
              <button v-for="r in recents" :key="r.path" class="recent" @click="openProject(r.path)">
                <div class="corner"><Play :size="13" /></div>
                <div class="col grow" style="gap: 1px; min-width: 0">
                  <span class="rname truncate">{{ r.name }}</span>
                  <span class="t-xs faint truncate" :title="r.path">{{ r.path }}</span>
                </div>
                <span class="t-xs faint row" style="gap: 4px; flex: 0 0 auto">
                  <Clock :size="11" />
                  {{ fmtTime(r.openedAt).slice(0, 11) }}
                </span>
              </button>
            </div>
            <UiEmpty v-else compact title="还没有项目" hint="新建一个，或者打开一个已有目录" />
          </section>

          <!-- 九步概览 -->
          <section class="card card-pad col" style="gap: 12px">
            <div class="row-between">
              <span class="section-label">九个面板</span>
              <span class="t-xs faint">左侧导航按这个顺序排，做到哪一步点哪一步</span>
            </div>
            <div class="steps">
              <div v-for="(s, i) in FLOW" :key="s.name" class="step">
                <span class="snum num">{{ i + 1 }}</span>
                <div class="col" style="gap: 1px; min-width: 0">
                  <span class="t-sm" style="font-weight: 500">{{ s.name }}</span>
                  <span class="t-xs faint truncate">{{ s.desc }}</span>
                </div>
              </div>
            </div>
          </section>
        </main>

        <!-- 侧栏 -->
        <aside class="col" style="gap: var(--sp-4)">
          <section class="card card-pad col" style="gap: 10px">
            <span class="section-label">开工前</span>

            <div class="tip">
              <div class="ticon" :class="{ warn: !hasModel }"><KeyRound :size="13" /></div>
              <div class="col" style="gap: 2px; min-width: 0">
                <span class="t-sm" style="font-weight: 500">配模型接口</span>
                <span class="t-xs faint" style="line-height: 1.7">
                  文本模型支持 OpenAI 兼容端点与 Claude；生图 / 生视频在设置里有现成模板。
                  不配也能用 —— 占位模型与占位素材能把流程跑通。
                </span>
              </div>
            </div>

            <div class="tip">
              <div class="ticon"><BookOpen :size="13" /></div>
              <div class="col" style="gap: 2px; min-width: 0">
                <span class="t-sm" style="font-weight: 500">装自己的知识包</span>
                <span class="t-xs faint" style="line-height: 1.7">
                  把自己的创作方法论、风格圣经放进「设置 → 知识包」，
                  Simon 就会按它工作。正文按需读取，不吃缓存。
                </span>
              </div>
            </div>
          </section>

          <section class="card card-pad col" style="gap: 10px">
            <span class="section-label">两个要点</span>

            <div class="tip">
              <div class="ticon"><Zap :size="13" /></div>
              <div class="col" style="gap: 2px; min-width: 0">
                <span class="t-sm" style="font-weight: 500">缓存决定成本</span>
                <span class="t-xs faint" style="line-height: 1.7">
                  每个会话带着一份冻结的项目上下文，整场对话不变，所以连续对话大部分输入
                  都按缓存价计费。
                </span>
              </div>
            </div>

            <div class="tip">
              <div class="ticon"><Sparkles :size="13" /></div>
              <div class="col" style="gap: 2px; min-width: 0">
                <span class="t-sm" style="font-weight: 500">Simon 能跨面板动手</span>
                <span class="t-xs faint" style="line-height: 1.7">
                  它是共享上下文的助手：你说「把分镜写完然后建资产」，它会依次调用两个面板的工具。
                </span>
              </div>
            </div>
          </section>
        </aside>
      </div>
    </div>
  </div>
</template>

<style scoped>
.home {
  height: 100%;
  background: radial-gradient(1100px 460px at 50% -14%, var(--glow), transparent), var(--bg);
}
.wrap {
  max-width: 1180px;
  margin: 0 auto;
  padding: var(--sp-6) var(--sp-6) var(--sp-8);
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
  min-height: 100%;
}

.bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-4);
  padding: var(--sp-2) 0 var(--sp-3);
}
.mark {
  width: 42px;
  height: 42px;
  border-radius: 13px;
  background: linear-gradient(145deg, var(--accent), var(--accent-press));
  color: var(--accent-fg);
  display: grid;
  place-items: center;
  font-size: 19px;
  font-weight: 700;
  box-shadow: var(--shadow);
  flex: 0 0 auto;
}
h1 {
  font-size: var(--t-xl);
  font-weight: 650;
  letter-spacing: -0.01em;
}

.grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 320px;
  gap: var(--sp-4);
  align-items: start;
}

.newrow {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto auto;
  gap: var(--sp-2);
}
.path {
  padding: 6px 10px;
  background: var(--surface-3);
  border-radius: var(--r-sm);
}

.recents {
  display: flex;
  flex-direction: column;
}
.recent {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 16px;
  background: none;
  border: none;
  border-top: 1px solid var(--line-faint);
  color: var(--fg);
  cursor: pointer;
  text-align: left;
  transition: background var(--fast);
}
.recent:hover {
  background: var(--surface-3);
}
.corner {
  width: 30px;
  height: 30px;
  flex: 0 0 auto;
  border-radius: var(--r-sm);
  display: grid;
  place-items: center;
  background: var(--surface-4);
  color: var(--fg-faint);
}
.recent:hover .corner {
  color: var(--accent);
  background: var(--accent-soft);
}
.rname {
  font-size: var(--t-base);
  font-weight: 500;
}

.steps {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 6px;
}
.step {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 8px 10px;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  min-width: 0;
}
.snum {
  width: 20px;
  height: 20px;
  flex: 0 0 auto;
  border-radius: var(--r-sm);
  background: var(--accent-soft);
  color: var(--accent);
  font-size: 10px;
  display: grid;
  place-items: center;
  border: 1px solid var(--accent-line);
}
@media (max-width: 1180px) {
  .steps {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

.tip {
  display: flex;
  gap: 9px;
  align-items: flex-start;
}
.ticon {
  width: 24px;
  height: 24px;
  flex: 0 0 auto;
  border-radius: 7px;
  display: grid;
  place-items: center;
  background: var(--surface-4);
  color: var(--fg-faint);
  margin-top: 1px;
}
.ticon.warn {
  background: var(--warn-soft);
  color: var(--warn);
}

@media (max-width: 1000px) {
  .grid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
