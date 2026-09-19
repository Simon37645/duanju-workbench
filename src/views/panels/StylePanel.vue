<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { NButton, NInput, NSelect, NTag, NPopover } from "naive-ui";
import { useProjectStore } from "@/stores/project";
import { errorText } from "@/api/ipc";
import { message } from "@/utils/notify";
import type { StylePreset, StyleState } from "@/types/models";

const project = useProjectStore();
const presets = ref<StylePreset[]>([]);
const draft = ref<StyleState | null>(null);
const dirty = ref(false);
const category = ref<string>("全部");

const categories = computed(() => ["全部", ...new Set(presets.value.map((p) => p.category))]);
const shown = computed(() =>
  category.value === "全部" ? presets.value : presets.value.filter((p) => p.category === category.value),
);
const ratioOptions = ["9:16", "16:9", "3:4", "4:3", "1:1", "2.35:1"].map((v) => ({ label: v, value: v }));

onMounted(async () => {
  presets.value = await import("@/api/ipc").then((m) => m.api.stylePresets());
  const s = await project.snapshot;
  void s;
  draft.value = JSON.parse(JSON.stringify(project.style ?? { activePresetId: null, spec: {} }));
});

function markDirty() {
  dirty.value = true;
}

function applyPreset(p: StylePreset) {
  if (!draft.value) return;
  draft.value.activePresetId = p.id;
  draft.value.spec = {
    ...draft.value.spec,
    name: p.name,
    prompt: p.prompt,
    negative: p.negative,
    palette: [...p.palette],
    lighting: p.lighting,
    lens: p.lens,
    filmStock: p.filmStock,
    motionStyle: p.motionStyle,
    aspectRatio: draft.value.spec.aspectRatio || "9:16",
  };
  dirty.value = true;
  message.info(`已套用「${p.name}」，记得点保存`);
}

async function save() {
  if (!draft.value) return;
  try {
    await project.setStyle(JSON.parse(JSON.stringify(draft.value)));
    dirty.value = false;
    message.success("风格已保存");
  } catch (e) {
    message.error(`保存失败：${errorText(e)}`);
  }
}

async function clearStyle() {
  if (!draft.value) return;
  draft.value.spec = {
    ...draft.value.spec,
    name: "",
    prompt: "",
    negative: "",
    palette: [],
    lighting: "",
    lens: "",
    filmStock: "",
    motionStyle: "",
    notes: "",
  };
  draft.value.activePresetId = null;
  dirty.value = true;
}

const composed = computed(() => {
  const s = draft.value?.spec;
  if (!s) return "";
  return [
    s.prompt,
    s.lighting && `光线：${s.lighting}`,
    s.lens && `镜头：${s.lens}`,
    s.filmStock && `质感：${s.filmStock}`,
    s.palette?.length ? `色调：${s.palette.join("、")}` : "",
  ]
    .filter(Boolean)
    .join("，");
});
</script>

<template>
  <div class="wrap3">
    <div class="pane col-presets">
      <div class="head row-between">
        <span style="font-weight: 600">预设风格</span>
        <n-select
          v-model:value="category"
          size="tiny"
          style="width: 96px"
          :options="categories.map((c) => ({ label: c, value: c }))"
        />
      </div>
      <div class="scroll list">
        <div v-for="p in shown" :key="p.id" class="preset" @click="applyPreset(p)">
          <div class="row-between">
            <span style="font-weight: 500">{{ p.name }}</span>
            <span class="tag">{{ p.category }}</span>
          </div>
          <div class="tiny faint clamp">{{ p.prompt }}</div>
          <div class="row wrap" style="gap: 3px; margin-top: 3px">
            <span v-for="c in p.palette" :key="c" class="dot-tag">
              <span class="dotc" :style="{ background: colorOf(c) }" />{{ c }}
            </span>
          </div>
        </div>
      </div>
    </div>

    <div class="pane col-edit" v-if="draft">
      <div class="head row-between">
        <span style="font-weight: 600">风格圣经</span>
        <div class="row" style="gap: 4px">
          <n-button size="tiny" quaternary @click="clearStyle">清空</n-button>
          <n-button size="tiny" type="primary" :disabled="!dirty" @click="save">
            {{ dirty ? "保存" : "已保存" }}
          </n-button>
        </div>
      </div>

      <div class="scroll body col">
        <div class="field">
          <label>风格名</label>
          <n-input v-model:value="draft.spec.name" size="small" @update:value="markDirty" />
        </div>

        <div class="field">
          <label>
            风格提示词
            <span class="tiny faint">（会拼到每条生图提示词最前面）</span>
          </label>
          <n-input
            v-model:value="draft.spec.prompt"
            type="textarea"
            :autosize="{ minRows: 3, maxRows: 8 }"
            placeholder="例如：电影级质感，暖调高饱和，柔光竖屏人像，浅景深…"
            @update:value="markDirty"
          />
        </div>

        <div class="field">
          <label>负面词</label>
          <n-input
            v-model:value="draft.spec.negative"
            type="textarea"
            :autosize="{ minRows: 2, maxRows: 5 }"
            placeholder="要避免的东西：低分辨率, 畸变, 多余手指, 水印…"
            @update:value="markDirty"
          />
        </div>

        <div class="grid2">
          <div class="field">
            <label>光线</label>
            <n-input v-model:value="draft.spec.lighting" size="small" @update:value="markDirty" />
          </div>
          <div class="field">
            <label>镜头 / 焦段</label>
            <n-input v-model:value="draft.spec.lens" size="small" @update:value="markDirty" />
          </div>
          <div class="field">
            <label>质感 / 胶片感</label>
            <n-input v-model:value="draft.spec.filmStock" size="small" @update:value="markDirty" />
          </div>
          <div class="field">
            <label>画幅</label>
            <n-select
              v-model:value="draft.spec.aspectRatio"
              size="small"
              :options="ratioOptions"
              @update:value="markDirty"
            />
          </div>
          <div class="field">
            <label>运动风格</label>
            <n-input v-model:value="draft.spec.motionStyle" size="small" @update:value="markDirty" />
          </div>
          <div class="field">
            <label>色调（回车添加）</label>
            <n-select
              v-model:value="draft.spec.palette"
              multiple
              filterable
              tag
              size="small"
              :options="(draft.spec.palette ?? []).map((c) => ({ label: c, value: c }))"
              @update:value="markDirty"
            />
          </div>
        </div>

        <div class="field">
          <label>备注</label>
          <n-input
            v-model:value="draft.spec.notes"
            type="textarea"
            :autosize="{ minRows: 2, maxRows: 5 }"
            @update:value="markDirty"
          />
        </div>

        <div class="field">
          <label>合成后的画面描述预览</label>
          <div class="preview">{{ composed || "（还没有内容）" }}</div>
        </div>

        <div class="tiny faint">
          风格是下游所有生图与生视频的共享前缀。定稿之后尽量别频繁改 —— 一改，已出的图就和新风格不一致了。
        </div>
      </div>
    </div>
  </div>
</template>

<script lang="ts">
// 把中文色名粗略映射到色块，纯装饰用
function colorOf(name: string): string {
  const map: Record<string, string> = {
    暖橘: "#e8933d",
    奶油白: "#f2e8d5",
    浅粉: "#f0b9c4",
    青蓝: "#3d8fa8",
    深灰: "#3a3f47",
    冷白: "#dfe7ee",
    朱红: "#c0392b",
    鎏金: "#d4a017",
    墨色: "#22252b",
    青碧: "#2fa89a",
    金霞: "#e8b04a",
    月白: "#e6eef5",
    褐绿: "#5a6b4a",
    暗金: "#a8842c",
    烟灰: "#6b6f75",
    浅绿: "#a8d5a2",
    天蓝: "#7ab8e0",
    暖白: "#f5efe4",
    石墨灰: "#4a4f57",
    香槟金: "#d9c9a3",
    铁灰: "#5a6068",
    暗红: "#8b2f2a",
    米白: "#efe9dd",
    木色: "#a97c50",
    暖灰: "#8a8580",
    明黄: "#f2c744",
    湖蓝: "#3d9bc4",
    橙红: "#e2603a",
    浅青: "#a8d8dc",
    银灰: "#b8bec4",
    土黄: "#b09050",
    硝烟灰: "#75787c",
    暗褐: "#6b4f3a",
    霓虹青: "#28d8d8",
    品红: "#e0389b",
    深紫: "#4a2d6b",
    嫩绿: "#8fce6a",
    麦黄: "#e0c060",
    雾白: "#e8ecef",
    暖黄: "#e8c05a",
    墨绿: "#2f4a3a",
  };
  return map[name] ?? "#6b6f75";
}

export default { name: "StylePanel" };
</script>

<style scoped>
.wrap3 {
  display: flex;
  gap: 12px;
  height: 100%;
  min-height: 0;
}
.col-presets {
  width: 300px;
  flex: 0 0 300px;
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
.preset {
  padding: 8px;
  border-radius: 6px;
  cursor: pointer;
  border: 1px solid transparent;
}
.preset:hover {
  background: var(--bg-3);
  border-color: var(--line);
}
.clamp {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  margin-top: 2px;
}
.dot-tag {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-size: 10px;
  color: var(--text-faint);
}
.dotc {
  width: 8px;
  height: 8px;
  border-radius: 2px;
  display: inline-block;
}
.body {
  flex: 1;
  min-height: 0;
  padding: 12px;
  gap: 12px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.field label {
  font-size: 12px;
  color: var(--text-dim);
}
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.preview {
  background: var(--bg-1);
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 10px;
  font-size: 12.5px;
  line-height: 1.8;
  color: var(--text);
}
</style>
