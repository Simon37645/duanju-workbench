<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Check, Palette, RotateCcw, Save } from "@lucide/vue";
import { useProjectStore } from "@/stores/project";
import { api, errorText } from "@/api/ipc";
import { toast } from "@/ui";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiInput from "@/ui/Input.vue";
import UiTextarea from "@/ui/Textarea.vue";
import UiSelect from "@/ui/Select.vue";
import UiField from "@/ui/Field.vue";
import UiTagInput from "@/ui/TagInput.vue";
import type { StylePreset, StyleState } from "@/types/models";

const project = useProjectStore();
const presets = ref<StylePreset[]>([]);
const draft = ref<StyleState | null>(null);
const dirty = ref(false);
const cat = ref("全部");

const CATS = computed(() => ["全部", ...new Set(presets.value.map((p) => p.category))]);
const shown = computed(() =>
  cat.value === "全部" ? presets.value : presets.value.filter((p) => p.category === cat.value),
);
const ratioOptions = ["9:16", "16:9", "3:4", "4:3", "1:1", "2.35:1"].map((v) => ({ label: v, value: v }));

/** 中文色名 → 色块，纯装饰 */
function swatch(name: string) {
  const map: Record<string, string> = {
    暖橘: "#e8933d", 奶油白: "#f2e8d5", 浅粉: "#f0b9c4", 青蓝: "#3d8fa8", 深灰: "#3a3f47",
    冷白: "#dfe7ee", 朱红: "#c0392b", 鎏金: "#d4a017", 墨色: "#22252b", 青碧: "#2fa89a",
    金霞: "#e8b04a", 月白: "#e6eef5", 褐绿: "#5a6b4a", 暗金: "#a8842c", 烟灰: "#6b6f75",
    浅绿: "#a8d5a2", 天蓝: "#7ab8e0", 暖白: "#f5efe4", 石墨灰: "#4a4f57", 香槟金: "#d9c9a3",
    铁灰: "#5a6068", 暗红: "#8b2f2a", 米白: "#efe9dd", 木色: "#a97c50", 暖灰: "#8a8580",
    明黄: "#f2c744", 湖蓝: "#3d9bc4", 橙红: "#e2603a", 浅青: "#a8d8dc", 银灰: "#b8bec4",
    霓虹青: "#28d8d8", 品红: "#e0389b", 深紫: "#4a2d6b", 嫩绿: "#8fce6a", 麦黄: "#e0c060",
    雾白: "#e8ecef", 暖黄: "#e8c05a", 墨绿: "#2f4a3a", 土黄: "#b09050",
  };
  return map[name] ?? "#6b6f75";
}

const activePresetId = computed(() => draft.value?.activePresetId ?? null);

onMounted(async () => {
  presets.value = await api.stylePresets();
  await project.ensure();
  draft.value = JSON.parse(JSON.stringify(project.style ?? { activePresetId: null, spec: {} }));
});

function mark() {
  dirty.value = true;
}

function apply(p: StylePreset) {
  if (!draft.value) return;
  draft.value.activePresetId = p.id;
  draft.value.spec = {
    ...draft.value.spec,
    name: p.name, prompt: p.prompt, negative: p.negative,
    palette: [...p.palette], lighting: p.lighting, lens: p.lens,
    filmStock: p.filmStock, motionStyle: p.motionStyle,
    aspectRatio: draft.value.spec.aspectRatio || "9:16",
  };
  dirty.value = true;
}

async function save() {
  if (!draft.value) return;
  try {
    await project.setStyle(JSON.parse(JSON.stringify(draft.value)));
    dirty.value = false;
    toast.ok("风格已保存");
  } catch (e) {
    toast.err(errorText(e));
  }
}

function clear() {
  if (!draft.value) return;
  draft.value.spec = {
    ...draft.value.spec, name: "", prompt: "", negative: "", palette: [],
    lighting: "", lens: "", filmStock: "", motionStyle: "", notes: "",
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
  ].filter(Boolean).join("，");
});
</script>

<template>
  <div class="layout">
    <!-- 预设 -->
    <section class="panel presets-panel">
      <header class="panel-head">
        <div class="row" style="gap: 6px">
          <span class="section-label">预设风格</span>
          <UiBadge tone="neutral" size="xs">{{ shown.length }}</UiBadge>
        </div>
        <UiSelect
          v-model="cat"
          :options="CATS.map((c) => ({ label: c, value: c }))"
          style="width: 90px"
        />
      </header>
      <div class="grid scroll">
        <button
          v-for="p in shown"
          :key="p.id"
          class="preset"
          :class="{ on: activePresetId === p.id }"
          @click="apply(p)"
        >
          <div class="row-between">
            <span class="pname truncate">{{ p.name }}</span>
            <Check v-if="activePresetId === p.id" :size="12" class="tick" />
            <span v-else class="t-xs faint">{{ p.category }}</span>
          </div>
          <p class="pdesc clamp-2">{{ p.prompt }}</p>
          <div class="row wrap" style="gap: 4px">
            <span v-for="c in p.palette" :key="c" class="sw">
              <span class="sdot" :style="{ background: swatch(c) }" />{{ c }}
            </span>
          </div>
        </button>
      </div>
    </section>

    <!-- 编辑 -->
    <section v-if="draft" class="panel edit-panel">
      <header class="panel-head">
        <div class="row" style="gap: 7px">
          <Palette :size="14" class="faint" />
          <span class="section-label">风格圣经</span>
        </div>
        <div class="row" style="gap: 4px">
          <UiButton variant="subtle" size="xs" @click="clear">
            <template #icon><RotateCcw :size="12" /></template>
            清空
          </UiButton>
          <UiButton variant="primary" size="sm" :disabled="!dirty" @click="save">
            <template #icon><Save :size="12" /></template>
            {{ dirty ? "保存" : "已保存" }}
          </UiButton>
        </div>
      </header>

      <div class="form scroll">
        <UiField label="风格名">
          <UiInput v-model="draft.spec.name" @update:model-value="mark" />
        </UiField>

        <UiField label="风格提示词" hint="会拼到每条生图提示词最前面">
          <UiTextarea
            v-model="draft.spec.prompt"
            :rows="4"
            placeholder="例如：电影级质感，暖调高饱和，柔光竖屏人像，浅景深虚化背景"
            @update:model-value="mark"
          />
        </UiField>

        <UiField label="负面词" hint="要避免的东西">
          <UiTextarea
            v-model="draft.spec.negative"
            :rows="2"
            placeholder="低分辨率, 畸变, 多余手指, 塑料感, 水印"
            @update:model-value="mark"
          />
        </UiField>

        <div class="grid2">
          <UiField label="光线">
            <UiInput v-model="draft.spec.lighting" @update:model-value="mark" />
          </UiField>
          <UiField label="镜头 / 焦段">
            <UiInput v-model="draft.spec.lens" @update:model-value="mark" />
          </UiField>
          <UiField label="质感 / 胶片感">
            <UiInput v-model="draft.spec.filmStock" @update:model-value="mark" />
          </UiField>
          <UiField label="画幅">
            <UiSelect v-model="draft.spec.aspectRatio" :options="ratioOptions" @update:model-value="mark" />
          </UiField>
          <UiField label="运动风格">
            <UiInput v-model="draft.spec.motionStyle" @update:model-value="mark" />
          </UiField>
          <UiField label="色调" hint="回车添加">
            <UiTagInput v-model="draft.spec.palette" @update:model-value="mark" />
          </UiField>
        </div>

        <UiField label="备注">
          <UiTextarea v-model="draft.spec.notes" :rows="2" @update:model-value="mark" />
        </UiField>

        <UiField label="合成后的画面描述">
          <div class="composed">{{ composed || "（还没有内容）" }}</div>
        </UiField>

        <p class="t-xs faint" style="line-height: 1.75">
          风格是下游所有生图与生视频的共享前缀。定稿后尽量别频繁改 ——
          一改，已经出的图就和新风格不一致了。
        </p>
      </div>
    </section>
  </div>
</template>

<style scoped>
.layout {
  display: flex;
  gap: var(--sp-3);
  height: 100%;
  min-height: 0;
}
.presets-panel {
  width: 300px;
  flex: 0 0 300px;
}
.edit-panel {
  flex: 1;
  min-width: 0;
}
.grid {
  flex: 1;
  min-height: 0;
  padding: var(--sp-2);
  display: grid;
  grid-template-columns: 1fr;
  gap: 6px;
  align-content: start;
}
.preset {
  text-align: left;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  padding: 9px 10px;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 5px;
  transition: border-color var(--fast), background var(--fast);
}
.preset:hover {
  background: var(--surface-4);
  border-color: var(--line-strong);
}
.preset.on {
  border-color: var(--accent-line);
  background: var(--accent-soft);
}
.pname {
  font-size: var(--t-base);
  font-weight: 500;
}
.tick {
  color: var(--accent);
}
.pdesc {
  font-size: var(--t-xs);
  color: var(--fg-faint);
  line-height: 1.6;
  margin: 0;
}
.sw {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-size: 10px;
  color: var(--fg-ghost);
}
.sdot {
  width: 8px;
  height: 8px;
  border-radius: 2px;
}

.form {
  flex: 1;
  min-height: 0;
  padding: var(--sp-4);
  display: flex;
  flex-direction: column;
  gap: var(--sp-4);
  max-width: 720px;
}
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--sp-4);
}
.composed {
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  padding: 10px 12px;
  font-size: var(--t-sm);
  line-height: 1.8;
  color: var(--fg-dim);
}
</style>
