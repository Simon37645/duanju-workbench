<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import {
  NButton, NInput, NSelect, NModal, NPopover, NProgress, NEmpty, NSpin, useDialog, NCheckbox,
} from "naive-ui";
import { openPath } from "@tauri-apps/plugin-opener";
import { useProjectStore } from "@/stores/project";
import { useJobsStore } from "@/stores/jobs";
import { errorText } from "@/api/ipc";
import { fileUrl } from "@/api/events";
import { message } from "@/utils/notify";
import type { Asset, AssetKind, AssetView, ViewKind } from "@/types/models";

const project = useProjectStore();
const jobs = useJobsStore();
const dialog = useDialog();

const selectedId = ref<string | null>(null);
const kindFilter = ref<AssetKind | "all">("all");
const showNew = ref(false);
const newAsset = ref<{ kind: AssetKind; name: string; description: string }>({
  kind: "character",
  name: "",
  description: "",
});
const planTarget = ref<Asset | null>(null);
const planKinds = ref<ViewKind[]>([]);

const kindLabel: Record<AssetKind, string> = {
  character: "人物",
  scene: "场景",
  prop: "道具",
  costume: "服装",
  vehicle: "载具",
  effect: "特效",
  other: "其他",
};

const viewLabel: Record<ViewKind, string> = {
  front: "正面",
  side: "侧面",
  back: "背面",
  threeQuarter: "四分之三侧",
  fullBody: "全身",
  closeUp: "特写",
  wide: "全景",
  medium: "中景",
  birdView: "俯视",
  custom: "自定义",
};

const kinds: AssetKind[] = ["character", "scene", "prop", "costume", "vehicle", "effect", "other"];

const list = computed(() =>
  project.assets
    .filter((a) => kindFilter.value === "all" || a.kind === kindFilter.value)
    .slice()
    .sort((a, b) => kindLabel[a.kind].localeCompare(kindLabel[b.kind]) || a.name.localeCompare(b.name)),
);

const current = computed(() => project.assets.find((a) => a.id === selectedId.value) ?? null);

const stats = computed(() => {
  const total = project.assets.reduce((n, a) => n + a.views.length, 0);
  const done = project.assets.reduce(
    (n, a) => n + a.views.filter((v) => v.status === "done").length,
    0,
  );
  return { total, done };
});

onMounted(() => {
  if (list.value.length) selectedId.value = list.value[0].id;
});

function createAsset() {
  if (!newAsset.value.name.trim()) {
    message.warning("请填资产名");
    return;
  }
  const a: Asset = {
    id: "",
    kind: newAsset.value.kind,
    name: newAsset.value.name.trim(),
    aliases: [],
    description: newAsset.value.description,
    tags: [],
    lockedTraits: [],
    views: [],
    createdAt: "",
    updatedAt: "",
  };
  project.upsertAsset(a).then((saved) => {
    selectedId.value = saved.id;
    showNew.value = false;
    newAsset.value = { kind: "character", name: "", description: "" };
    message.success("已创建，接着规划要生成哪些视图");
  });
}

function removeAsset(a: Asset) {
  dialog.warning({
    title: "删除资产",
    content: `确定删除「${a.name}」？已生成的图片文件不会被删除，但会从项目里移除引用。`,
    positiveText: "删除",
    negativeText: "取消",
    onPositiveClick: async () => {
      await project.deleteAsset(a.id);
      if (selectedId.value === a.id) selectedId.value = null;
    },
  });
}

async function patchAsset(a: Asset, patch: Partial<Asset>) {
  await project.upsertAsset({ ...a, ...patch });
}

/** 文本字段用本地草稿 + 失焦保存，避免每敲一个字就写一次盘 */
const form = ref({ name: "", description: "" });
watch(
  current,
  (c) => {
    if (c) form.value = { name: c.name, description: c.description };
  },
  { immediate: true },
);

async function flushForm() {
  const c = current.value;
  if (!c) return;
  if (form.value.name === c.name && form.value.description === c.description) return;
  await patchAsset(c, { name: form.value.name.trim() || c.name, description: form.value.description });
}

function previewFile(path: string | null) {
  if (path) openPath(path).catch(() => undefined);
}

function openPlan(a: Asset) {
  planTarget.value = a;
  planKinds.value = a.views.map((v) => v.kind);
}

function quickPick(preset: "character" | "scene") {
  planKinds.value =
    preset === "character"
      ? ["front", "side", "back", "fullBody"]
      : ["wide", "medium", "closeUp", "birdView"];
}

async function applyPlan() {
  const a = planTarget.value;
  if (!a) return;
  const views: AssetView[] = planKinds.value.map((k) => {
    const existing = a.views.find((v) => v.kind === k);
    return (
      existing ?? {
        id: "",
        kind: k,
        label: viewLabel[k],
        prompt: "",
        negative: "",
        file: null,
        thumb: null,
        seed: null,
        model: null,
        providerId: null,
        status: "planned" as const,
        error: null,
        jobId: null,
        createdAt: "",
      }
    );
  });
  await project.planViews(a.id, views);
  planTarget.value = null;
  message.success("视图已规划，可以点生成");
}

async function generate(a: Asset, viewIds: string[], force = false) {
  try {
    const ids = await project.generateViews(a.id, viewIds, force);
    message.info(`已提交 ${ids.length} 个生图任务，进度见底部任务栏`);
  } catch (e) {
    message.error(errorText(e));
  }
}

async function generateAllMissing() {
  try {
    let n = 0;
    for (const a of project.assets) {
      const ids = a.views.filter((v) => v.status === "planned" || v.status === "failed").map((v) => v.id);
      if (ids.length) n += (await project.generateViews(a.id, ids)).length;
    }
    message.info(n ? `已提交 ${n} 个任务` : "没有待生成的视图");
  } catch (e) {
    message.error(errorText(e));
  }
}

function viewJob(view: AssetView) {
  return jobs.byId(view.jobId);
}

const planOptions = Object.entries(viewLabel).map(([value, label]) => ({ label, value }));
</script>

<template>
  <div class="wrap">
    <!-- 左：资产列表 -->
    <div class="pane col-list">
      <div class="head col" style="gap: 6px">
        <div class="row-between">
          <span style="font-weight: 600">资产（{{ project.assets.length }}）</span>
          <n-button size="tiny" quaternary @click="showNew = true">＋</n-button>
        </div>
        <div class="row" style="gap: 4px">
          <n-select
            v-model:value="kindFilter"
            size="tiny"
            style="width: 96px"
            :options="[{ label: '全部', value: 'all' }, ...kinds.map((k) => ({ label: kindLabel[k], value: k }))]"
          />
          <span class="tiny faint">{{ stats.done }}/{{ stats.total }} 张已出</span>
        </div>
      </div>
      <div class="scroll list">
        <div
          v-for="a in list"
          :key="a.id"
          class="item"
          :class="{ active: a.id === selectedId }"
          @click="selectedId = a.id"
        >
          <div class="row-between">
            <div class="row" style="gap: 6px; min-width: 0">
              <span class="tag">{{ kindLabel[a.kind] }}</span>
              <span class="truncate" style="font-weight: 500">{{ a.name }}</span>
            </div>
            <span class="tiny faint">
              {{ a.views.filter((v) => v.status === "done").length }}/{{ a.views.length }}
            </span>
          </div>
          <div v-if="a.views.length" class="thumbs">
            <img
              v-for="v in a.views.filter((x) => x.file).slice(0, 4)"
              :key="v.id"
              :src="fileUrl(v.thumb || v.file)"
            />
          </div>
        </div>
        <div v-if="!list.length" class="tiny faint" style="padding: 16px; text-align: center">
          还没有资产。<br />建人物、场景、道具，或让右侧 agent 从剧本里自动整理出来。
        </div>
      </div>
      <div class="foot">
        <n-button size="tiny" block @click="generateAllMissing">批量生成待出图</n-button>
      </div>
    </div>

    <!-- 右：资产详情 -->
    <div class="pane col-detail" v-if="current">
      <div class="head row-between">
        <div class="row" style="gap: 8px; min-width: 0">
          <n-input
            v-model:value="form.name"
            size="small"
            style="width: 160px"
            @blur="flushForm"
          />
          <n-select
            :value="current.kind"
            size="small"
            style="width: 96px"
            :options="kinds.map((k) => ({ label: kindLabel[k], value: k }))"
            @update:value="(v: AssetKind) => patchAsset(current!, { kind: v })"
          />
        </div>
        <div class="row" style="gap: 4px">
          <n-button size="tiny" @click="openPlan(current)">规划视图</n-button>
          <n-button size="tiny" quaternary @click="removeAsset(current)">删除</n-button>
        </div>
      </div>

      <div class="scroll body col">
        <label>描述
          <n-input
            v-model:value="form.description"
            type="textarea"
            :autosize="{ minRows: 2, maxRows: 4 }"
            placeholder="外形、材质、气质…这段会进入生成提示词"
            @blur="flushForm"
          />
        </label>
        <label>
          固定特征（跨图一致性锁定，出图时原样带上）
          <n-select
            :value="current.lockedTraits"
            multiple filterable tag size="small"
            placeholder="如：左眉有疤、银色细框眼镜、酒红色长发"
            :options="current.lockedTraits.map((t) => ({ label: t, value: t }))"
            @update:value="(v: string[]) => patchAsset(current!, { lockedTraits: v })"
          />
        </label>
        <div class="grid2">
          <label>别名（用逗号分隔，剧本里出现也能对上）
            <n-select
              :value="current.aliases" multiple filterable tag size="small"
              :options="current.aliases.map((t) => ({ label: t, value: t }))"
              @update:value="(v: string[]) => patchAsset(current!, { aliases: v })"
            />
          </label>
          <label>标签
            <n-select
              :value="current.tags" multiple filterable tag size="small"
              :options="current.tags.map((t) => ({ label: t, value: t }))"
              @update:value="(v: string[]) => patchAsset(current!, { tags: v })"
            />
          </label>
        </div>

        <div class="row-between" style="margin-top: 4px">
          <span style="font-weight: 600">视图与成图</span>
          <div class="row" style="gap: 4px">
            <n-button size="tiny" quaternary @click="generate(current, [], false)">生成待出</n-button>
            <n-button size="tiny" quaternary @click="generate(current, [], true)">全部重出</n-button>
          </div>
        </div>

        <div v-if="!current.views.length" class="tiny faint" style="padding: 20px; text-align: center">
          还没规划视图。人物建议正面 / 侧面 / 背面 / 全身；场景建议全景 / 中景 / 特写 / 俯视。
        </div>

        <div class="views">
          <div v-for="v in current.views" :key="v.id" class="view">
            <div class="thumb">
              <img v-if="v.file" :src="fileUrl(v.thumb || v.file)" @click="previewFile(v.file)" />
              <div v-else class="ph tiny faint">
                {{ v.status === "running" ? "生成中…" : v.status === "failed" ? "失败" : "未生成" }}
              </div>
            </div>
            <div class="row-between">
              <span class="small truncate">{{ v.label }}</span>
              <span class="tag" :class="v.status === 'done' ? 'ok' : v.status === 'failed' ? 'err' : ''">
                {{ v.status }}
              </span>
            </div>
            <n-progress
              v-if="viewJob(v) && viewJob(v)!.status === 'running'"
              type="line" :percentage="Math.round((viewJob(v)!.progress) * 100)"
              :height="3" :show-indicator="false"
            />
            <div v-if="v.error" class="tiny err truncate" :title="v.error">{{ v.error }}</div>
            <div class="row" style="gap: 3px">
              <n-button size="tiny" quaternary @click="generate(current, [v.id], v.status === 'done')">
                生成
              </n-button>
              <n-popover trigger="click" placement="bottom">
                <template #trigger>
                  <n-button size="tiny" quaternary>提示词</n-button>
                </template>
                <div style="width: 300px" class="col">
                  <n-input
                    :value="v.prompt" type="textarea" :autosize="{ minRows: 4, maxRows: 10 }"
                    placeholder="这个视图自身的提示词（风格词会自动拼在前面）"
                    @update:value="(val: string) => {
                      const views = current!.views.map((x) => (x.id === v.id ? { ...x, prompt: val } : x));
                      project.planViews(current!.id, views);
                    }"
                  />
                  <div class="tiny faint">改完自动保存</div>
                </div>
              </n-popover>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div v-else class="pane col-detail center">
      <n-empty description="选择或新建一个资产" />
    </div>

    <!-- 新建资产 -->
    <n-modal v-model:show="showNew" preset="dialog" title="新建资产" style="width: 420px">
      <div class="col" style="gap: 10px">
        <div class="row">
          <n-select
            v-model:value="newAsset.kind"
            style="width: 120px"
            :options="kinds.map((k) => ({ label: kindLabel[k], value: k }))"
          />
          <n-input v-model:value="newAsset.name" placeholder="资产名，分镜里就用这个名字引用" />
        </div>
        <n-input
          v-model:value="newAsset.description"
          type="textarea"
          :autosize="{ minRows: 3, maxRows: 6 }"
          placeholder="外形描述"
        />
      </div>
      <template #action>
        <n-button size="small" @click="showNew = false">取消</n-button>
        <n-button size="small" type="primary" @click="createAsset">创建</n-button>
      </template>
    </n-modal>

    <!-- 规划视图 -->
    <n-modal
      :show="!!planTarget"
      preset="dialog"
      title="规划视图"
      style="width: 420px"
      @update:show="(v: boolean) => { if (!v) planTarget = null; }"
    >
      <div class="col" style="gap: 10px">
        <div class="row">
          <n-button size="tiny" @click="quickPick('character')">人物三视图</n-button>
          <n-button size="tiny" @click="quickPick('scene')">场景多角度</n-button>
        </div>
        <n-select v-model:value="planKinds" multiple :options="planOptions" placeholder="选择要生成哪些视图" />
        <div class="tiny faint">
          已出图的同类型视图会保留，不会被清掉。规划完记得点「生成」。
        </div>
      </div>
      <template #action>
        <n-button size="small" @click="planTarget = null">取消</n-button>
        <n-button size="small" type="primary" @click="applyPlan">确定</n-button>
      </template>
    </n-modal>
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
  width: 290px;
  flex: 0 0 290px;
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
.col-detail.center {
  display: grid;
  place-items: center;
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
.thumbs {
  display: flex;
  gap: 3px;
  margin-top: 4px;
  margin-left: 2px;
}
.thumbs img {
  width: 30px;
  height: 30px;
  object-fit: cover;
  border-radius: 4px;
  border: 1px solid var(--line);
}
.body {
  flex: 1;
  min-height: 0;
  padding: 12px;
  gap: 10px;
}
.body label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
  color: var(--text-dim);
}
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.views {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 10px;
}
.view {
  background: var(--bg-1);
  border: 1px solid var(--line);
  border-radius: 7px;
  padding: 6px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.thumb {
  aspect-ratio: 1;
  background: #0c0c11;
  border-radius: 5px;
  overflow: hidden;
  display: grid;
  place-items: center;
}
.thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  cursor: zoom-in;
}
.ph {
  text-align: center;
}
.err {
  color: var(--err);
}
</style>
