<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { ImageOff, Images, Layers, Plus, Sparkles, Trash2, Wand2, Zap } from "@lucide/vue";
import { openPath } from "@tauri-apps/plugin-opener";
import { useProjectStore } from "@/stores/project";
import { useJobsStore } from "@/stores/jobs";
import { useAgentStore } from "@/stores/agent";
import { errorText } from "@/api/ipc";
import { fileUrl } from "@/api/events";
import { toast, confirmDialog } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiInput from "@/ui/Input.vue";
import UiTextarea from "@/ui/Textarea.vue";
import UiSelect from "@/ui/Select.vue";
import UiField from "@/ui/Field.vue";
import UiTagInput from "@/ui/TagInput.vue";
import UiEmpty from "@/ui/Empty.vue";
import UiProgress from "@/ui/Progress.vue";
import UiModal from "@/ui/Modal.vue";
import UiCheckbox from "@/ui/Checkbox.vue";
import type { Asset, AssetKind, AssetStatus, AssetView, ViewKind } from "@/types/models";

const project = useProjectStore();
const jobs = useJobsStore();
const agent = useAgentStore();

const selectedId = ref<string | null>(null);
const kindFilter = ref<string>("all");
const showNew = ref(false);
const planning = ref<Asset | null>(null);
const planKinds = ref<ViewKind[]>([]);
const form = ref({ name: "", description: "" });
const newAsset = ref({ kind: "character" as AssetKind, name: "", description: "" });

const KINDS: { label: string; value: AssetKind }[] = [
  { label: "人物", value: "character" },
  { label: "场景", value: "scene" },
  { label: "道具", value: "prop" },
  { label: "服装", value: "costume" },
  { label: "载具", value: "vehicle" },
  { label: "特效", value: "effect" },
  { label: "其他", value: "other" },
];
const KIND_LABEL = Object.fromEntries(KINDS.map((k) => [k.value, k.label])) as Record<AssetKind, string>;
const VIEW_LABEL: Record<ViewKind, string> = {
  front: "正面", side: "侧面", back: "背面", threeQuarter: "四分之三侧",
  fullBody: "全身", closeUp: "特写", wide: "全景", medium: "中景",
  birdView: "俯视", custom: "自定义",
};

const list = computed(() =>
  project.assets
    .filter((a) => kindFilter.value === "all" || a.kind === kindFilter.value)
    .slice()
    .sort((a, b) => a.name.localeCompare(b.name)),
);
const current = computed(() => project.assets.find((a) => a.id === selectedId.value) ?? null);
const stats = computed(() => {
  const total = project.assets.reduce((n, a) => n + a.views.length, 0);
  const done = project.assets.reduce((n, a) => n + a.views.filter((v) => v.status === "done").length, 0);
  return { total, done };
});

watch(current, (c) => {
  if (c) form.value = { name: c.name, description: c.description };
}, { immediate: true });

onMounted(async () => {
  await project.ensure();
  if (list.value.length) selectedId.value = list.value[0].id;
});

async function patch(a: Asset, p: Partial<Asset>) {
  await project.upsertAsset({ ...a, ...p });
}

async function flushForm() {
  const c = current.value;
  if (!c) return;
  if (form.value.name === c.name && form.value.description === c.description) return;
  await patch(c, { name: form.value.name.trim() || c.name, description: form.value.description });
}

async function createAsset() {
  if (!newAsset.value.name.trim()) return toast.warn("请填资产名");
  const saved = await project.upsertAsset({
    id: "", kind: newAsset.value.kind, name: newAsset.value.name.trim(),
    aliases: [], description: newAsset.value.description, tags: [], lockedTraits: [],
    views: [], createdAt: "", updatedAt: "",
  });
  selectedId.value = saved.id;
  showNew.value = false;
  newAsset.value = { kind: "character", name: "", description: "" };
  toast.ok("已创建，接着规划要生成哪些视图");
}

async function removeAsset(a: Asset) {
  const ok = await confirmDialog({
    title: "删除资产",
    content: `确定删除「${a.name}」？已生成的图片文件不会删，但会从项目里移除引用。`,
    positiveText: "删除", danger: true,
  });
  if (!ok) return;
  await project.deleteAsset(a.id);
  if (selectedId.value === a.id) selectedId.value = null;
}

function openPlan(a: Asset) {
  planning.value = a;
  planKinds.value = a.views.map((v) => v.kind);
}

function quickPick(kind: "character" | "scene") {
  planKinds.value = kind === "character"
    ? ["front", "side", "back", "fullBody"]
    : ["wide", "medium", "closeUp", "birdView"];
}

async function applyPlan() {
  const a = planning.value;
  if (!a) return;
  const views: AssetView[] = planKinds.value.map((k) => {
    const prev = a.views.find((v) => v.kind === k);
    if (prev) return prev;
    return {
      id: `new_${k}`, kind: k, label: VIEW_LABEL[k], prompt: "", negative: "",
      file: null, thumb: null, seed: null, model: null, providerId: null,
      status: "planned" as AssetStatus, error: null, jobId: null, createdAt: "",
    };
  });
  await project.planViews(a.id, views);
  planning.value = null;
  toast.ok("视图已规划，可以点生成了");
}

async function generate(a: Asset, viewIds: string[], force = false) {
  try {
    const ids = await project.generateViews(a.id, viewIds, force);
    toast.info(`已提交 ${ids.length} 个生图任务，进度见底部任务栏`);
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function generateAll() {
  let n = 0;
  try {
    for (const a of project.assets) {
      const ids = a.views.filter((v) => v.status === "planned" || v.status === "failed").map((v) => v.id);
      if (ids.length) n += (await project.generateViews(a.id, ids)).length;
    }
    toast.info(n ? `已提交 ${n} 个任务` : "没有待生成的视图");
  } catch (e) {
    toast.err(errorText(e));
  }
}

const viewJob = (v: AssetView) => jobs.byId(v.jobId);
const planOptions = Object.entries(VIEW_LABEL).map(([value, label]) => ({ label, value }));
</script>

<template>
  <div class="layout">
    <!-- 资产列表 -->
    <section class="panel list-panel">
      <header class="panel-head">
        <div class="row" style="gap: 6px">
          <span class="section-label">资产</span>
          <UiBadge tone="neutral" size="xs">{{ project.assets.length }}</UiBadge>
        </div>
        <div class="row" style="gap: 2px">
          <UiButton variant="subtle" size="xs" icon @click="showNew = true">
            <template #icon><Plus :size="13" /></template>
          </UiButton>
        </div>
      </header>

      <div class="filterbar">
        <UiSelect
          v-model="kindFilter"
          :options="[{ label: '全部类型', value: 'all' }, ...KINDS]"
          style="flex: 1"
        />
        <span class="t-xs faint num">{{ stats.done }}/{{ stats.total }}</span>
      </div>

      <div class="list scroll">
        <button
          v-for="a in list"
          :key="a.id"
          class="asset"
          :class="{ on: a.id === selectedId }"
          @click="selectedId = a.id"
        >
          <div class="thumb">
            <img v-if="a.views.find((v) => v.file)?.file" :src="fileUrl(a.views.find((v) => v.file)!.thumb || a.views.find((v) => v.file)!.file)" />
            <ImageOff v-else :size="14" />
          </div>
          <div class="col grow" style="gap: 2px; min-width: 0">
            <div class="row" style="gap: 5px">
              <span class="aname truncate">{{ a.name }}</span>
              <UiBadge tone="neutral" size="xs">{{ KIND_LABEL[a.kind] }}</UiBadge>
            </div>
            <span class="t-xs faint">
              {{ a.views.filter((v) => v.status === "done").length }}/{{ a.views.length }} 张
              <span v-if="a.lockedTraits.length"> · 锁定 {{ a.lockedTraits.length }} 项</span>
            </span>
          </div>
        </button>

        <UiEmpty
          v-if="!list.length"
          compact
          title="还没有资产"
          hint="建人物、场景、道具，或让 agent 从剧本里自动整理"
        />
      </div>

      <div class="list-foot">
        <UiButton variant="default" size="sm" block @click="generateAll">
          <template #icon><Zap :size="13" /></template>
          批量生成待出图
        </UiButton>
      </div>
    </section>

    <!-- 详情 -->
    <section class="panel detail-panel">
      <template v-if="current">
        <header class="panel-head">
          <div class="row" style="gap: 8px; min-width: 0">
            <UiInput v-model="form.name" style="width: 160px" @blur="flushForm" />
            <UiSelect
              :model-value="current.kind"
              :options="KINDS"
              style="width: 92px"
              @update:model-value="(v: string | null) => patch(current!, { kind: v as AssetKind })"
            />
          </div>
          <div class="row" style="gap: 4px">
            <UiButton variant="default" size="xs" @click="openPlan(current)">
              <template #icon><Layers :size="12" /></template>
              规划视图
            </UiButton>
            <UiButton variant="subtle" size="xs" icon @click="removeAsset(current)">
              <template #icon><Trash2 :size="12" /></template>
            </UiButton>
          </div>
        </header>

        <div class="detail scroll">
          <UiField label="描述" hint="这段会进入生成提示词">
            <UiTextarea
              v-model="form.description"
              :rows="2"
              placeholder="外形、材质、气质…"
              @blur="flushForm"
            />
          </UiField>

          <UiField label="固定特征" hint="跨图一致性锁定，出图时原样带上">
            <UiTagInput
              :model-value="current.lockedTraits"
              placeholder="如：左眉有疤、银色细框眼镜"
              @update:model-value="(v: string[]) => patch(current!, { lockedTraits: v })"
            />
          </UiField>

          <div class="grid2">
            <UiField label="别名">
              <UiTagInput
                :model-value="current.aliases"
                placeholder="剧本里出现的其他叫法"
                @update:model-value="(v: string[]) => patch(current!, { aliases: v })"
              />
            </UiField>
            <UiField label="标签">
              <UiTagInput
                :model-value="current.tags"
                @update:model-value="(v: string[]) => patch(current!, { tags: v })"
              />
            </UiField>
          </div>

          <div class="row-between" style="margin-top: var(--sp-2)">
            <span class="section-label">视图与成图</span>
            <div class="row" style="gap: 4px">
              <UiButton variant="subtle" size="xs" @click="generate(current, [], false)">生成待出</UiButton>
              <UiButton variant="subtle" size="xs" @click="generate(current, [], true)">全部重出</UiButton>
            </div>
          </div>

          <div v-if="!current.views.length" class="tipbox">
            还没规划视图。人物建议正面 / 侧面 / 背面 / 全身；场景建议全景 / 中景 / 特写 / 俯视。
          </div>

          <div class="views">
            <div v-for="v in current.views" :key="v.id" class="view">
              <div class="vthumb" @click="v.file && openPath(v.file)">
                <img v-if="v.file" :src="fileUrl(v.thumb || v.file)" />
                <div v-else class="t-xs faint">
                  {{ v.status === "running" ? "生成中…" : v.status === "failed" ? "失败" : "未生成" }}
                </div>
              </div>
              <div class="row-between">
                <span class="t-sm truncate">{{ v.label }}</span>
                <UiBadge
                  :tone="v.status === 'done' ? 'ok' : v.status === 'failed' ? 'err' : 'neutral'"
                  size="xs"
                >
                  {{ v.status === "planned" ? "待生成" : v.status === "done" ? "完成" : v.status === "failed" ? "失败" : v.status }}
                </UiBadge>
              </div>
              <UiProgress
                v-if="viewJob(v)?.status === 'running'"
                :value="(viewJob(v)!.progress ?? 0) * 100"
                :height="3"
              />
              <div v-if="v.error" class="t-xs truncate" style="color: var(--err)" :title="v.error">
                {{ v.error }}
              </div>
              <UiButton
                variant="subtle"
                size="xs"
                block
                @click="generate(current, [v.id], v.status === 'done')"
              >
                {{ v.status === "done" ? "重新生成" : "生成" }}
              </UiButton>
            </div>
          </div>
        </div>
      </template>

      <UiEmpty v-else title="选择或新建一个资产" hint="左侧点一个资产查看详情">
        <template #icon><Images :size="26" /></template>
        <UiButton variant="outline" size="sm" @click="showNew = true">新建资产</UiButton>
      </UiEmpty>
    </section>

    <!-- 新建 -->
    <UiModal :show="showNew" title="新建资产" :width="440" @update:show="(v: boolean) => (showNew = v)">
      <div class="col" style="gap: 12px">
        <div class="grid2">
          <UiField label="类型">
            <UiSelect v-model="newAsset.kind" :options="KINDS" />
          </UiField>
          <UiField label="名称">
            <UiInput v-model="newAsset.name" placeholder="分镜里就用这个名字引用" />
          </UiField>
        </div>
        <UiField label="外形描述">
          <UiTextarea v-model="newAsset.description" :rows="3" />
        </UiField>
      </div>
      <template #footer>
        <UiButton variant="ghost" @click="showNew = false">取消</UiButton>
        <UiButton variant="primary" @click="createAsset">创建</UiButton>
      </template>
    </UiModal>

    <!-- 规划视图 -->
    <UiModal
      :show="!!planning"
      title="规划视图"
      :width="440"
      @update:show="(v: boolean) => !v && (planning = null)"
    >
      <div class="col" style="gap: 12px">
        <div class="row" style="gap: 6px">
          <UiButton variant="outline" size="xs" @click="quickPick('character')">人物三视图</UiButton>
          <UiButton variant="outline" size="xs" @click="quickPick('scene')">场景多角度</UiButton>
        </div>
        <div class="checks">
          <UiCheckbox
            v-for="o in planOptions"
            :key="o.value"
            :model-value="planKinds.includes(o.value as ViewKind)"
            :label="o.label"
            @update:model-value="(v: boolean) => {
              const k = o.value as ViewKind;
              planKinds = v ? [...planKinds, k] : planKinds.filter((x) => x !== k);
            }"
          />
        </div>
        <p class="t-xs faint">已出图的同类型视图会保留，不会被清掉。</p>
      </div>
      <template #footer>
        <UiButton variant="ghost" @click="planning = null">取消</UiButton>
        <UiButton variant="primary" @click="applyPlan">确定</UiButton>
      </template>
    </UiModal>
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
  width: 268px;
  flex: 0 0 268px;
}
.detail-panel {
  flex: 1;
  min-width: 0;
}
.filterbar {
  display: flex;
  align-items: center;
  gap: 8px;
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
.list-foot {
  padding: 8px 10px;
  border-top: 1px solid var(--line-faint);
}
.asset {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 6px 7px;
  border-radius: var(--r);
  border: 1px solid transparent;
  background: none;
  cursor: pointer;
  text-align: left;
  transition: background var(--fast), border-color var(--fast);
}
.asset:hover {
  background: var(--surface-3);
}
.asset.on {
  background: var(--surface-3);
  border-color: var(--line-strong);
}
.thumb {
  width: 34px;
  height: 34px;
  flex: 0 0 auto;
  border-radius: var(--r-sm);
  background: var(--surface-4);
  border: 1px solid var(--line);
  display: grid;
  place-items: center;
  overflow: hidden;
  color: var(--fg-ghost);
}
.thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.aname {
  font-size: var(--t-base);
  font-weight: 500;
  color: var(--fg);
}

.detail {
  flex: 1;
  min-height: 0;
  padding: var(--sp-4);
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--sp-3);
}
.tipbox {
  padding: 14px;
  text-align: center;
  font-size: var(--t-sm);
  color: var(--fg-faint);
  background: var(--surface-3);
  border-radius: var(--r);
  line-height: 1.7;
}
.views {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(136px, 1fr));
  gap: var(--sp-3);
}
.view {
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  padding: 7px;
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.vthumb {
  aspect-ratio: 1;
  background: var(--media-bg);
  border-radius: var(--r-sm);
  overflow: hidden;
  display: grid;
  place-items: center;
  cursor: zoom-in;
}
.vthumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.checks {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 8px 12px;
}
</style>
