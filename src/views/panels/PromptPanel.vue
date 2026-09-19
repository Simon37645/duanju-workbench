<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { NButton, NInput, NSelect, NInputNumber, NProgress, NPopover } from "naive-ui";
import { useProjectStore } from "@/stores/project";
import { errorText } from "@/api/ipc";
import { message } from "@/utils/notify";
import type { Shot, VideoPrompt } from "@/types/models";

const project = useProjectStore();

const chapterId = ref<string>("");
const currentShotId = ref<string | null>(null);
const form = ref<VideoPrompt | null>(null);

const chapters = computed(() => project.chapters);
const shots = computed(() =>
  project.shots
    .filter((s) => !chapterId.value || s.chapterId === chapterId.value)
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
  project.assets.map((a) => ({ label: `${a.name}（${a.kind}）`, value: a.id })),
);

function assetName(id: string | null | undefined) {
  if (!id) return "";
  return project.assets.find((a) => a.id === id)?.name ?? id;
}

function viewOptions(assetId: string | null) {
  const a = project.assets.find((x) => x.id === assetId);
  return (a?.views ?? [])
    .filter((v) => v.status === "done" || v.file)
    .map((v) => ({ label: v.label, value: v.id }));
}

onMounted(async () => {
  if (chapters.value.length) chapterId.value = chapters.value[0].id;
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
        id: "",
        shotId,
        index: shot?.index ?? 0,
        prompt: shot?.videoPrompt || "",
        negative: project.style?.spec.negative ?? "",
        motion: shot?.action ?? "",
        cameraMove: shot?.cameraMove ?? "",
        durationSec: shot?.durationSec ?? 3,
        firstFrame: null,
        lastFrame: null,
        refs: [],
        modelHint: null,
        seed: null,
        status: "draft",
        updatedAt: "",
      };
}

async function save() {
  if (!form.value) return;
  if (!form.value.prompt.trim()) {
    message.warning("提示词不能为空");
    return;
  }
  try {
    await project.upsertPrompt(form.value);
    message.success("已保存");
  } catch (e) {
    message.error(errorText(e));
  }
}

async function autofill() {
  try {
    const n = await project.autofillPrompts(chapterId.value || undefined);
    message.success(n ? `已补齐 ${n} 条提示词` : "没有需要补齐的");
  } catch (e) {
    message.error(errorText(e));
  }
}

/** 一键把该镜头出场人物的成图设为参考 */
function autoBind() {
  const shot = project.shots.find((s) => s.id === currentShotId.value);
  if (!shot || !form.value) return;
  const refs = shot.characters
    .map((name) => project.assets.find((a) => a.name === name))
    .filter((a): a is NonNullable<typeof a> => !!a)
    .map((a) => {
      const v = a.views.find((x) => x.file) ?? a.views[0];
      return { assetId: a.id, viewId: v?.id ?? null };
    });
  form.value.refs = refs;
  if (!form.value.firstFrame && refs.length) form.value.firstFrame = refs[0];
  message.info(`已匹配 ${refs.length} 个参考资产，记得保存`);
}

function shotOf(id: string): Shot | undefined {
  return project.shots.find((s) => s.id === id);
}
</script>

<template>
  <div class="wrap">
    <div class="pane col-list">
      <div class="head col" style="gap: 6px">
        <div class="row-between">
          <span style="font-weight: 600">镜头提示词</span>
          <n-button size="tiny" quaternary @click="autofill">自动补齐</n-button>
        </div>
        <div class="row" style="gap: 6px">
          <n-select
            v-model:value="chapterId"
            size="tiny"
            style="width: 150px"
            clearable
            placeholder="全部章节"
            :options="chapters.map((c) => ({ label: `第${c.index}章`, value: c.id }))"
          />
          <span class="tiny faint">
            {{ stats.written }}/{{ stats.total }} 已写 · {{ stats.bound }} 已配首帧
          </span>
        </div>
      </div>
      <div class="scroll list">
        <div
          v-for="s in shots"
          :key="s.id"
          class="item"
          :class="{ active: s.id === currentShotId }"
          @click="select(s.id)"
        >
          <div class="row-between">
            <div class="row" style="gap: 6px; min-width: 0">
              <span class="mono tiny faint">{{ s.index }}</span>
              <span class="truncate">{{ s.action || "（无描述）" }}</span>
            </div>
            <span
              class="tag"
              :class="project.promptOf(s.id)?.prompt ? (project.promptOf(s.id)?.firstFrame ? 'ok' : 'warn') : ''"
            >
              {{ project.promptOf(s.id)?.prompt ? (project.promptOf(s.id)?.firstFrame ? "齐" : "缺图") : "未写" }}
            </span>
          </div>
          <div v-if="project.promptOf(s.id)?.prompt" class="tiny faint truncate">
            {{ project.promptOf(s.id)!.prompt }}
          </div>
        </div>
        <div v-if="!shots.length" class="tiny faint" style="padding: 16px; text-align: center">
          还没有镜头，先去分镜面板
        </div>
      </div>
    </div>

    <div class="pane col-edit" v-if="form">
      <div class="head row-between">
        <div class="row" style="gap: 8px; min-width: 0">
          <span style="font-weight: 600">镜头 {{ shotOf(form.shotId)?.index }}</span>
          <span class="tiny faint truncate" style="max-width: 320px">
            {{ shotOf(form.shotId)?.action }}
          </span>
        </div>
        <div class="row" style="gap: 4px">
          <n-button size="tiny" quaternary @click="autoBind">按人物自动配图</n-button>
          <n-button size="tiny" type="primary" @click="save">保存</n-button>
        </div>
      </div>

      <div class="scroll body col">
        <label>
          视频提示词
          <span class="tiny faint">（描述运动过程与结束状态，不要只写静态画面）</span>
          <n-input
            v-model:value="form.prompt"
            type="textarea"
            :autosize="{ minRows: 4, maxRows: 12 }"
            placeholder="例如：女人缓缓抬头，眼眶泛红，镜头从中景缓慢推近到面部特写，最后停在嘴角一抹冷笑"
          />
        </label>

        <div class="grid2">
          <label>主体动作
            <n-input v-model:value="form.motion" size="small" />
          </label>
          <label>运镜
            <n-input v-model:value="form.cameraMove" size="small" />
          </label>
          <label>时长（秒）
            <n-input-number v-model:value="form.durationSec" size="small" :min="0.5" :max="20" :step="0.5" />
          </label>
          <label>随机种子（可留空）
            <n-input-number v-model:value="form.seed" size="small" style="width: 100%" />
          </label>
        </div>

        <label>负面词
          <n-input v-model:value="form.negative" type="textarea" :autosize="{ minRows: 2, maxRows: 4 }" />
        </label>

        <div class="pane sub">
          <div class="row-between" style="margin-bottom: 8px">
            <span style="font-weight: 600">资产配对</span>
            <span class="tiny faint">首帧图对生成质量影响最大</span>
          </div>

          <div class="grid2">
            <label>首帧图
              <div class="row" style="gap: 4px">
                <n-select
                  size="small"
                  style="flex: 1"
                  placeholder="选资产"
                  :value="form.firstFrame?.assetId"
                  :options="assetOptions"
                  clearable
                  @update:value="(v: string) => (form!.firstFrame = v ? { assetId: v, viewId: null } : null)"
                />
                <n-select
                  v-if="form.firstFrame"
                  size="small"
                  style="width: 108px"
                  placeholder="视图"
                  :value="form.firstFrame.viewId"
                  :options="viewOptions(form.firstFrame.assetId)"
                  @update:value="(v: string) => (form!.firstFrame = { assetId: form!.firstFrame!.assetId, viewId: v })"
                />
              </div>
            </label>
            <label>尾帧图（可选）
              <div class="row" style="gap: 4px">
                <n-select
                  size="small"
                  style="flex: 1"
                  placeholder="选资产"
                  :value="form.lastFrame?.assetId"
                  :options="assetOptions"
                  clearable
                  @update:value="(v: string) => (form!.lastFrame = v ? { assetId: v, viewId: null } : null)"
                />
                <n-select
                  v-if="form.lastFrame"
                  size="small"
                  style="width: 108px"
                  placeholder="视图"
                  :value="form.lastFrame.viewId"
                  :options="viewOptions(form.lastFrame.assetId)"
                  @update:value="(v: string) => (form!.lastFrame = { assetId: form!.lastFrame!.assetId, viewId: v })"
                />
              </div>
            </label>
          </div>

          <label style="margin-top: 8px">其它参考图
            <n-select
              size="small"
              multiple
              :value="form.refs.map((r) => r.assetId)"
              :options="assetOptions"
              @update:value="(ids: string[]) => (form!.refs = ids.map((id) => ({ assetId: id, viewId: null })))"
            />
          </label>

          <div v-if="form.firstFrame" class="tiny faint" style="margin-top: 6px">
            首帧：{{ assetName(form.firstFrame.assetId) }}
          </div>
        </div>
      </div>
    </div>

    <div v-else class="pane col-edit center">
      <span class="tiny faint">从左侧选一个镜头</span>
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
  width: 330px;
  flex: 0 0 330px;
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.col-edit {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.col-edit.center {
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
.sub {
  padding: 10px;
  background: var(--bg-1);
}
</style>
