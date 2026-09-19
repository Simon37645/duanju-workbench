<script setup lang="ts">
/** 多值输入：标签列表（别名、固定特征、色调等都用它） */
import { ref } from "vue";
import { X } from "@lucide/vue";

const props = withDefaults(
  defineProps<{
    modelValue: string[];
    placeholder?: string;
    suggestions?: string[];
    disabled?: boolean;
  }>(),
  { modelValue: () => [] },
);
const emit = defineEmits<{ "update:modelValue": [string[]] }>();

const draft = ref("");
const focused = ref(false);

function add(v?: string) {
  const t = (v ?? draft.value).trim();
  if (!t) return;
  if (!props.modelValue.includes(t)) emit("update:modelValue", [...props.modelValue, t]);
  draft.value = "";
}

function remove(i: number) {
  const next = [...props.modelValue];
  next.splice(i, 1);
  emit("update:modelValue", next);
}

const filtered = () =>
  (props.suggestions ?? []).filter((s) => !props.modelValue.includes(s)).slice(0, 6);
</script>

<template>
  <div class="tags" :class="{ focused }">
    <span v-for="(t, i) in modelValue" :key="t" class="chip">
      <span class="truncate">{{ t }}</span>
      <button type="button" class="rm" @click="remove(i)"><X :size="10" /></button>
    </span>
    <input
      v-model="draft"
      class="entry"
      :placeholder="modelValue.length ? '' : placeholder ?? '回车添加'"
      :disabled="disabled"
      @focus="focused = true"
      @blur="(focused = false), add()"
      @keydown.enter.prevent="add()"
      @keydown.backspace="!draft && modelValue.length && remove(modelValue.length - 1)"
      @keydown.,.prevent="add()"
    />
  </div>
  <div v-if="focused && filtered().length" class="sugg">
    <button v-for="s in filtered()" :key="s" type="button" class="s" @mousedown.prevent="add(s)">
      {{ s }}
    </button>
  </div>
</template>

<style scoped>
.tags {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 5px;
  min-height: 30px;
  padding: 4px 8px;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  transition: border-color var(--fast), box-shadow var(--fast);
  cursor: text;
}
.tags:hover {
  border-color: var(--line-strong);
}
.tags.focused {
  border-color: var(--accent-line);
  box-shadow: var(--ring);
}
.chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  max-width: 160px;
  height: 20px;
  padding: 0 4px 0 7px;
  background: var(--surface-4);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  font-size: var(--t-xs);
  color: var(--fg-dim);
}
.rm {
  background: none;
  border: none;
  color: var(--fg-ghost);
  cursor: pointer;
  padding: 0;
  display: grid;
  place-items: center;
}
.rm:hover {
  color: var(--err);
}
.entry {
  flex: 1;
  min-width: 70px;
  background: none;
  border: none;
  color: var(--fg);
  font-size: var(--t-sm);
  height: 20px;
}
.entry::placeholder {
  color: var(--fg-ghost);
}
.sugg {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 5px;
}
.s {
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  color: var(--fg-faint);
  font-size: var(--t-xs);
  padding: 2px 7px;
  cursor: pointer;
}
.s:hover {
  color: var(--fg);
  border-color: var(--line-strong);
}
</style>
