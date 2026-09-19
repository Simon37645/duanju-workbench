<script setup lang="ts">
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  ArrowRight, Clock, Film, FolderOpen, FolderPlus, KeyRound, Sparkles, Zap,
} from "@lucide/vue";
import { useProjectStore } from "@/stores/project";
import { useSettingsStore } from "@/stores/settings";
import { errorText } from "@/api/ipc";
import { fmtTime } from "@/api/events";
import { toast } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiInput from "@/ui/Input.vue";
import UiEmpty from "@/ui/Empty.vue";

const router = useRouter();
const project = useProjectStore();
const settings = useSettingsStore();

const creating = ref(false);
const busy = ref(false);
const newName = ref("");
const newParent = ref("");

const recents = computed(() => settings.recentProjects);
const hasModel = computed(() => settings.byKind("llm").length > 0);

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
    /* store 里已提示 */
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
      <!-- 头部 -->
      <header class="hero">
        <div class="mark">S</div>
        <h1>Simon 短剧工作台</h1>
        <p class="sub">
          从剧本到成片的九个面板，Simon 全程陪你做
        </p>
        <div class="flow">
          <span v-for="(s, i) in ['剧本', '风格', '分镜', '资产', '提示词', '生视频', '剪辑', '字幕', '核对']" :key="s">
            <span class="chipx">{{ s }}</span>
            <ArrowRight v-if="i < 8" :size="11" class="arrow" />
          </span>
        </div>
      </header>

      <!-- 主操作 -->
      <section class="actions">
        <button class="action" @click="creating = false; browse()">
          <FolderOpen :size="18" />
          <div class="col" style="gap: 2px; align-items: flex-start">
            <span class="atitle">打开项目</span>
            <span class="t-xs faint">选择已有的项目目录</span>
          </div>
        </button>
        <button class="action" :class="{ on: creating }" @click="creating = true">
          <FolderPlus :size="18" />
          <div class="col" style="gap: 2px; align-items: flex-start">
            <span class="atitle">新建项目</span>
            <span class="t-xs faint">创建一份干净的工程</span>
          </div>
        </button>
      </section>

      <!-- 新建表单 -->
      <Transition name="slide">
        <section v-if="creating" class="createbox">
          <div class="grid">
            <label class="fld">
              <span>项目名</span>
              <UiInput v-model="newName" size="md" placeholder="例如 示例短剧" @enter="create" />
            </label>
            <label class="fld">
              <span>存放位置</span>
              <div class="row" style="gap: 6px">
                <UiInput v-model="newParent" size="md" placeholder="选择一个文件夹" />
                <UiButton variant="outline" size="md" @click="pickParent">浏览</UiButton>
              </div>
            </label>
          </div>
          <div class="row-between">
            <span class="t-xs faint">
              会在该位置创建同名目录，里面是纯文件结构，整个目录拷走就能在别的机器打开
            </span>
            <UiButton variant="primary" size="md" :loading="busy" @click="create">创建并开始</UiButton>
          </div>
        </section>
      </Transition>

      <!-- 最近项目 -->
      <section class="col" style="gap: 10px; margin-top: var(--sp-2)">
        <span class="section-label">最近打开</span>
        <div v-if="recents.length" class="recents">
          <button v-for="r in recents" :key="r.path" class="recent" @click="openProject(r.path)">
            <div class="corner"><Film :size="15" /></div>
            <div class="col grow" style="gap: 3px; min-width: 0">
              <span class="rname truncate">{{ r.name }}</span>
              <span class="t-xs faint truncate" :title="r.path">{{ r.path }}</span>
            </div>
            <span class="t-xs faint row" style="gap: 4px">
              <Clock :size="11" />
              {{ fmtTime(r.openedAt).slice(0, 15) }}
            </span>
          </button>
        </div>
        <UiEmpty
          v-else
          :title="'还没有项目'"
          hint="新建一个，或者打开一个已有目录"
        >
          <template #icon><FolderOpen :size="26" /></template>
        </UiEmpty>
      </section>

      <!-- 提示 -->
      <section class="tips">
        <div class="tip">
          <div class="ticon" :class="{ warn: !hasModel }">
            <KeyRound :size="14" />
          </div>
          <div class="col" style="gap: 3px">
            <span class="t-sm" style="font-weight: 500">先配模型接口</span>
            <span class="t-xs faint" style="line-height: 1.7">
              文本模型支持 OpenAI 兼容端点与 Claude；生图 / 生视频在设置里有现成预设。
              不配也能用，会用占位模型与占位素材把流程跑通。
            </span>
          </div>
        </div>
        <div class="tip">
          <div class="ticon"><Zap :size="14" /></div>
          <div class="col" style="gap: 3px">
            <span class="t-sm" style="font-weight: 500">关于缓存</span>
            <span class="t-xs faint" style="line-height: 1.7">
              每个面板的对话都带着一份冻结的项目上下文，整场对话不变，
              所以连续对话大部分输入都能命中服务端前缀缓存，成本低得多。
            </span>
          </div>
        </div>
        <div class="tip">
          <div class="ticon"><Sparkles :size="14" /></div>
          <div class="col" style="gap: 3px">
            <span class="t-sm" style="font-weight: 500">让 Simon 动手</span>
            <span class="t-xs faint" style="line-height: 1.7">
              右下角的 Simon 能跨面板直接改数据：拆章节、写分镜、建资产、提交生成、铺时间线。
              权限模式可以调，默认只有花钱的操作才问你。
            </span>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.home {
  height: 100%;
  background: radial-gradient(900px 420px at 50% -12%, var(--glow), transparent),
    var(--bg);
}
.wrap {
  max-width: 760px;
  margin: 0 auto;
  padding: 72px var(--sp-6) 56px;
  display: flex;
  flex-direction: column;
  gap: var(--sp-5);
}

.hero {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  text-align: center;
}
.mark {
  width: 52px;
  height: 52px;
  border-radius: 15px;
  background: linear-gradient(145deg, #f5b155, #d98a24);
  color: #1a1206;
  display: grid;
  place-items: center;
  font-size: 24px;
  font-weight: 700;
  box-shadow: 0 8px 28px -8px rgba(240, 163, 61, 0.55);
  margin-bottom: 4px;
}
.hero h1 {
  font-size: var(--t-2xl);
  font-weight: 650;
  letter-spacing: -0.01em;
}
.sub {
  color: var(--fg-dim);
  font-size: var(--t-md);
}
.flow {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  justify-content: center;
  gap: 4px;
  margin-top: 6px;
}
.chipx {
  font-size: var(--t-xs);
  color: var(--fg-faint);
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  padding: 2px 7px;
}
.arrow {
  color: var(--fg-ghost);
  margin: 0 2px;
  vertical-align: middle;
}

.actions {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--sp-3);
}
.action {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
  color: var(--fg-dim);
  cursor: pointer;
  text-align: left;
  transition: border-color var(--fast), background var(--fast), transform var(--fast);
}
.action:hover {
  background: var(--surface-3);
  border-color: var(--line-strong);
  color: var(--fg);
  transform: translateY(-1px);
}
.action.on {
  border-color: var(--accent-line);
  background: var(--accent-soft);
  color: var(--accent);
}
.atitle {
  font-size: var(--t-base);
  font-weight: 600;
}

.createbox {
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
  padding: var(--sp-4);
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}
.grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--sp-3);
}
.fld {
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.fld > span {
  font-size: var(--t-sm);
  color: var(--fg-dim);
}
.slide-enter-active,
.slide-leave-active {
  transition: opacity 180ms var(--ease), transform 180ms var(--ease);
}
.slide-enter-from,
.slide-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}

.recents {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.recent {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
  color: var(--fg);
  cursor: pointer;
  text-align: left;
  transition: border-color var(--fast), background var(--fast);
}
.recent:hover {
  background: var(--surface-3);
  border-color: var(--line-strong);
}
.corner {
  width: 34px;
  height: 34px;
  flex: 0 0 auto;
  border-radius: var(--r);
  display: grid;
  place-items: center;
  background: var(--surface-3);
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

.tips {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--sp-3);
  margin-top: var(--sp-2);
}
.tip {
  display: flex;
  gap: 10px;
  padding: var(--sp-3);
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
}
.ticon {
  width: 26px;
  height: 26px;
  flex: 0 0 auto;
  border-radius: 7px;
  display: grid;
  place-items: center;
  background: var(--surface-3);
  color: var(--fg-faint);
}
.ticon.warn {
  background: var(--warn-soft);
  color: var(--warn);
}
</style>
