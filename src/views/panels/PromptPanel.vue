<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Check, Eye, ImagePlus, Sparkles, Wand2, X } from "@lucide/vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useProjectStore } from "@/stores/project";
import { useAgentStore } from "@/stores/agent";
import { api, errorText } from "@/api/ipc";
import { fileUrl } from "@/api/events";
import { toast } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiInput from "@/ui/Input.vue";
import UiNumber from "@/ui/NumberInput.vue";
import UiTextarea from "@/ui/Textarea.vue";
import UiSelect from "@/ui/Select.vue";
import UiField from "@/ui/Field.vue";
import UiEmpty from "@/ui/Empty.vue";
import UiModal from "@/ui/Modal.vue";
import type { AssetRef, Shot, VideoPrompt } from "@/types/models";

const project = useProjectStore();
const agent = useAgentStore();

const chapterId = ref("all");
const currentShotId = ref<string | null>(null);
const form = ref<VideoPrompt | null>(null);

/* --------------------------------- 完整提示词预览（含风格）与本地参考图 */

const previewOpen = ref(false);
const previewData = ref<{ prompt: string; negative: string } | null>(null);

async function openPreview() {
  if (!form.value?.id) return toast.info("先把这条提示词保存一下再预览");
  previewOpen.value = true;
  previewData.value = null;
  try {
    previewData.value = await api.videoPromptPreview(form.value.id);
  } catch (e) {
    previewData.value = { prompt: errorText(e), negative: "" };
  }
}

async function saveRefs(refs: string[]) {
  const f = form.value;
  if (!f) return;
  const updated = { ...f, refImages: refs };
  form.value = updated;
  try {
    await project.upsertPrompt(updated);
    toast.ok("参考图已更新");
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function pickLocalRefs() {
  const picked = await openDialog({ multiple: true, title: "选择参考图" });
  const list = Array.isArray(picked) ? picked : picked ? [picked] : [];
  if (!form.value || !list.length) return;
  const refs = [...(form.value.refImages ?? [])];
  for (const p of list) {
    const s = String(p);
    if (!refs.includes(s)) refs.push(s);
  }
  await saveRefs(refs);
}

async function removeRef(path: string) {
  if (!form.value) return;
  await saveRefs((form.value.refImages ?? []).filter((x) => x !== path));
}

const chapters = computed(() => project.chapters);
const shots = computed(() =>
  project.shots
    .filter((s) => chapterId.value === "all" || s.chapterId === chapterId.value)
    .slice()
    .sort((a, b) => a.index - b.index),
);

const stats = computed(() => {
  const total = shots.value.length;
  const written = shots.value.filter((s) => project.promptOf(s.id)?.prompt?.trim()).length;
  const bound = shots.value.filter((s) => project.promptOf(s.id)?.firstFrame).length;
  return { total, written, bound };
});

const assetOptions = computed(() =>
  project.assets.map((a) => ({ label: a.name, value: a.id })),
);
const viewOptions = (assetId: string | null) =>
  (project.assets.find((x) => x.id === assetId)?.views ?? [])
    .filter((v) => v.status === "done" || v.file)
    .map((v) => ({ label: v.label, value: v.id }));

function state(s: Shot) {
  const p = project.promptOf(s.id);
  if (!p?.prompt) return { label: "未写", tone: "neutral" as const };
  if (!p.firstFrame) return { label: "缺首帧", tone: "warn" as const };
  return { label: "齐", tone: "ok" as const };
}

onMounted(async () => {
  await project.ensure();
  if (chapters.value.length) chapterId.value = "all";
  const first = shots.value[0];
  if (first) select(first.id);
});

function select(shotId: string) {
  currentShotId.value = shotId;
  const existing = project.promptOf(shotId);
  const shot = project.shots.find((s) => s.id === shotId);
  form.value = existing
    ? JSON.parse(JSON.stringify(existing))
    : {
        id: "", shotId, index: shot?.index ?? 0,
        prompt: shot?.videoPrompt || "", negative: project.style?.spec.negative ?? "",
        motion: shot?.action ?? "", cameraMove: shot?.cameraMove ?? "",
        durationSec: shot?.durationSec ?? 3, firstFrame: null, lastFrame: null,
        refs: [], modelHint: null, seed: null, status: "draft", updatedAt: "",
      };
}

async function save() {
  if (!form.value) return;
  if (!form.value.prompt.trim()) return toast.warn("提示词不能为空");
  try {
    await project.upsertPrompt(form.value);
    toast.ok("已保存");
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function autofill() {
  try {
    const n = await project.autofillPrompts(chapterId.value === "all" ? undefined : chapterId.value);
    toast.ok(n ? `已补齐 ${n} 条提示词` : "没有需要补齐的");
  } catch (e) {
    toast.err(errorText(e));
  }
}

/** 按镜头出场人物，自动把已出图的人物/场景配成参考 */
function autoBind() {
  const shot = project.shots.find((s) => s.id === currentShotId.value);
  if (!shot || !form.value) return;
  const refs: AssetRef[] = shot.characters
    .map((name) => project.assets.find((a) => a.name === name))
    .filter((a): a is NonNullable<typeof a> => !!a)
    .map((a) => ({ assetId: a.id, viewId: (a.views.find((v) => v.file) ?? a.views[0])?.id ?? null }));
  form.value.refs = refs;
  if (!form.value.firstFrame && refs.length) form.value.firstFrame = refs[0];
  toast.info(`已匹配 ${refs.length} 个参考资产，记得保存`);
}

function setFrame(which: "firstFrame" | "lastFrame", assetId: string | null) {
  if (!form.value) return;
  form.value[which] = assetId ? { assetId, viewId: null } : null;
}

const shotOf = (id: string) => project.shots.find((s) => s.id === id);
</script>

<template>
  <div class="layout">
    <section class="panel list-panel">
      <header class="panel-head">
        <div class="row" style="gap: 6px; min-width: 0">
          <span class="section-label">提示词</span>
          <UiBadge tone="neutral" size="xs">{{ stats.written }}/{{ stats.total }}</UiBadge>
        </div>
        <UiButton variant="default" size="xs" @click="autofill">
          <template #icon><Wand2 :size="12" /></template>
          自动补齐
        </UiButton>
      </header>

      <div class="filterbar" style="gap: 8px">
        <UiSelect
          v-model="chapterId"
          :options="[{ label: '全部章节', value: 'all' }, ...chapters.map((c) => ({ label: `第${c.index}章`, value: c.id }))]"
          style="flex: 1"
        />
        <span class="t-xs" :style="{ color: stats.bound === stats.total && stats.total ? 'var(--ok)' : 'var(--fg-faint)' }">
          首帧 {{ stats.bound }}/{{ stats.total }}
        </span>
      </div>

      <div class="list scroll">
        <button
          v-for="s in shots"
          :key="s.id"
          class="item"
          :class="{ on: s.id === currentShotId }"
          @click="select(s.id)"
        >
          <span class="idx num">{{ s.index }}</span>
          <div class="col grow" style="gap: 2px; min-width: 0">
            <span class="t-sm truncate">{{ s.action || "（无描述）" }}</span>
            <span class="t-xs faint truncate">
              {{ project.promptOf(s.id)?.prompt || "还没有提示词" }}
            </span>
          </div>
          <UiBadge :tone="state(s).tone" size="xs">{{ state(s).label }}</UiBadge>
        </button>

        <UiEmpty v-if="!shots.length" compact title="还没有镜头" hint="先去分镜面板" />
      </div>
    </section>

    <section class="panel edit-panel">
      <template v-if="form">
        <header class="panel-head">
          <div class="row" style="gap: 8px; min-width: 0">
            <span class="section-label">镜头 {{ shotOf(form.shotId)?.index }}</span>
            <span class="t-xs faint truncate" style="max-width: 320px">
              {{ shotOf(form.shotId)?.action }}
            </span>
          </div>
          <div class="row" style="gap: 4px">
            <UiButton variant="subtle" size="xs" @click="autoBind">按人物配图</UiButton>
            <UiButton variant="subtle" size="xs" @click="openPreview">
              <Eye :size="12" /> 完整提示词
            </UiButton>
            <UiButton variant="primary" size="sm" @click="save">
              <template #icon><Check :size="12" /></template>
              保存
            </UiButton>
          </div>
        </header>

        <div class="form scroll">
          <UiField label="视频提示词" hint="描述运动过程与结束状态，不要只写静态画面">
            <UiTextarea
              v-model="form.prompt"
              :rows="5"
              placeholder="女人缓缓抬头，眼眶泛红，镜头从中景缓慢推近到面部特写，最后停在嘴角一抹冷笑"
            />
          </UiField>

          <div class="grid2">
            <UiField label="主体动作"><UiInput v-model="form.motion" /></UiField>
            <UiField label="运镜"><UiInput v-model="form.cameraMove" /></UiField>
            <UiField label="时长"><UiNumber v-model="form.durationSec" :min="0.5" :max="20" :step="0.5" suffix="s" /></UiField>
            <UiField label="随机种子"><UiNumber v-model="form.seed" placeholder="留空" /></UiField>
          </div>

          <UiField label="负面词">
            <UiTextarea v-model="form.negative" :rows="2" />
          </UiField>

          <div class="bindbox">
            <div class="row-between">
              <span class="section-label">资产配对</span>
              <span class="t-xs faint">首帧图对生成质量影响最大</span>
            </div>

            <div class="grid2">
              <UiField label="首帧图">
                <div class="row" style="gap: 5px">
                  <UiSelect
                    :model-value="form.firstFrame?.assetId ?? null"
                    :options="assetOptions"
                    placeholder="选资产"
                    clearable
                    style="flex: 1"
                    @update:model-value="(v: string | null) => setFrame('firstFrame', v)"
                  />
                  <UiSelect
                    v-if="form.firstFrame"
                    :model-value="form.firstFrame.viewId"
                    :options="viewOptions(form.firstFrame.assetId)"
                    placeholder="视图"
                    style="width: 100px"
                    @update:model-value="(v: string | null) => (form!.firstFrame = { assetId: form!.firstFrame!.assetId, viewId: v })"
                  />
                </div>
              </UiField>
              <UiField label="尾帧图">
                <div class="row" style="gap: 5px">
                  <UiSelect
                    :model-value="form.lastFrame?.assetId ?? null"
                    :options="assetOptions"
                    placeholder="可选"
                    clearable
                    style="flex: 1"
                    @update:model-value="(v: string | null) => setFrame('lastFrame', v)"
                  />
                  <UiSelect
                    v-if="form.lastFrame"
                    :model-value="form.lastFrame.viewId"
                    :options="viewOptions(form.lastFrame.assetId)"
                    placeholder="视图"
                    style="width: 100px"
                    @update:model-value="(v: string | null) => (form!.lastFrame = { assetId: form!.lastFrame!.assetId, viewId: v })"
                  />
                </div>
              </UiField>
            </div>

            <UiField label="其它参考图">
              <UiSelect
                :model-value="form.refs.map((r) => r.assetId)[0] ?? null"
                :options="assetOptions"
                placeholder="可选"
                clearable
                @update:model-value="(v: string | null) => (form!.refs = v ? [{ assetId: v, viewId: null }] : [])"
              />
            </UiField>

            <div v-if="form.firstFrame" class="t-xs faint">
              首帧：
              {{ project.assets.find((a) => a.id === form!.firstFrame!.assetId)?.name }}
            </div>
          </div>

          <UiButton variant="subtle" size="xs" @click="agent.send('prompt', `帮镜头 ${shotOf(form!.shotId)?.index} 重写一版视频提示词，强调运动过程。`)">
            <template #icon><Sparkles :size="12" /></template>
            让 agent 重写这一条
          </UiButton>
        </div>
      </template>

      <UiEmpty v-else title="选一个镜头" hint="从左侧点一个镜头开始写提示词">
        <template #icon><ImagePlus :size="26" /></template>
      </UiEmpty>
    </section>

    <!-- 视频完整提示词（含自动拼接的风格）+ 本地参考图 -->
    <UiModal
      :show="previewOpen"
      title="视频完整提示词"
      :width="560"
      @update:show="(v: boolean) => (previewOpen = v)"
    >
      <div class="col" style="gap: 12px">
        <div class="t-xs faint">
          生成视频时风格圣经的词会自动拼在最前（和资产图同样的规则），下面是最终发给模型的版本
        </div>
        <div class="col" style="gap: 4px">
          <span class="section-label">提示词</span>
          <pre class="pv">{{ previewData?.prompt ?? "读取中…" }}</pre>
        </div>
        <div class="col" style="gap: 4px">
          <span class="section-label">负面词</span>
          <pre class="pv">{{ previewData?.negative || "（无）" }}</pre>
        </div>
        <div class="col" style="gap: 6px">
          <span class="section-label">本地参考图（随生成一起提交）</span>
          <div v-if="(form?.refImages ?? []).length" class="row wrap" style="gap: 6px">
            <div v-for="r in form?.refImages ?? []" :key="r" class="refchip">
              <img :src="fileUrl(r)" alt="" />
              <button class="refdel" title="移除" @click="removeRef(r)">×</button>
            </div>
          </div>
          <div class="row" style="gap: 6px">
            <UiButton variant="outline" size="sm" @click="pickLocalRefs">本地文件…</UiButton>
            <span class="t-xs faint">资产的配对参考图在下方「参考」区选择</span>
          </div>
        </div>
      </div>
    </UiModal>
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
.edit-panel {
  flex: 1;
  min-width: 0;
}
.filterbar {
  padding: 8px 10px;
  border-bottom: 1px solid var(--line-faint);
  display: flex;
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
  gap: 9px;
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
.idx {
  width: 18px;
  flex: 0 0 auto;
  font-size: var(--t-xs);
  color: var(--fg-ghost);
  text-align: right;
}
.form {
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
.bindbox {
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  padding: var(--sp-3);
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}
/* 完整提示词预览 */
.pv {
  margin: 0;
  padding: 10px 12px;
  max-height: 220px;
  overflow: auto;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  color: var(--fg-dim);
  font-size: 12px;
  line-height: 1.7;
  white-space: pre-wrap;
  word-break: break-word;
}
.refchip {
  position: relative;
}
.refchip img {
  width: 56px;
  height: 56px;
  object-fit: cover;
  border-radius: var(--r-sm);
  border: 1px solid var(--line);
  display: block;
}
.refdel {
  position: absolute;
  top: -6px;
  right: -6px;
  width: 17px;
  height: 17px;
  border-radius: 50%;
  border: none;
  background: var(--surface-5);
  color: var(--fg);
  cursor: pointer;
  font-size: 11px;
  line-height: 1;
  display: grid;
  place-items: center;
}
.refdel:hover {
  background: var(--err);
  color: #fff;
}
</style>
