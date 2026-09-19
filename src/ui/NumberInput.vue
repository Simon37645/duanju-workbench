<script setup lang="ts">
import { computed } from "vue";
import { Minus, Plus } from "@lucide/vue";

const props = withDefaults(
  defineProps<{
    modelValue?: number | null;
    min?: number;
    max?: number;
    step?: number;
    placeholder?: string;
    size?: "sm" | "md";
    suffix?: string;
    stepper?: boolean;
  }>(),
  { size: "sm", step: 1, stepper: false },
);
const emit = defineEmits<{ "update:modelValue": [number | null] }>();

const display = computed(() => (props.modelValue === null || props.modelValue === undefined ? "" : String(props.modelValue)));

function commit(raw: string) {
  if (raw.trim() === "") return emit("update:modelValue", null);
  let v = Number(raw);
  if (Number.isNaN(v)) return;
  if (props.min !== undefined) v = Math.max(props.min, v);
  if (props.max !== undefined) v = Math.min(props.max, v);
  emit("update:modelValue", v);
}

function bump(dir: 1 | -1) {
  const base = props.modelValue ?? 0;
  commit(String(base + dir * props.step));
}
</script>

<template>
  <div class="num" :class="[`s-${size}`]">
    <button v-if="stepper" class="step" type="button" @click="bump(-1)">
      <Minus :size="11" />
    </button>
    <input
      class="field"
      type="text"
      inputmode="decimal"
      :value="display"
      :placeholder="placeholder"
      @change="commit(($event.target as HTMLInputElement).value)"
      @blur="commit(($event.target as HTMLInputElement).value)"
    />
    <span v-if="suffix" class="suffix">{{ suffix }}</span>
    <button v-if="stepper" class="step" type="button" @click="bump(1)">
      <Plus :size="11" />
    </button>
  </div>
</template>

<style scoped>
.num {
  display: inline-flex;
  align-items: center;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  transition: border-color var(--fast), box-shadow var(--fast);
}
.num:hover {
  border-color: var(--line-strong);
}
.num:focus-within {
  border-color: var(--accent-line);
  box-shadow: var(--ring);
}
.field {
  flex: 1;
  min-width: 0;
  width: 100%;
  background: none;
  border: none;
  color: var(--fg);
  font-variant-numeric: tabular-nums;
  padding: 0 8px;
  height: 100%;
}
.field::placeholder {
  color: var(--fg-ghost);
}
.suffix {
  color: var(--fg-faint);
  font-size: var(--t-xs);
  padding-right: 8px;
}
.step {
  background: none;
  border: none;
  color: var(--fg-faint);
  cursor: pointer;
  display: grid;
  place-items: center;
  width: 22px;
  height: 100%;
}
.step:hover {
  color: var(--fg);
}
.s-sm {
  height: 30px;
  font-size: var(--t-sm);
  min-width: 78px;
}
.s-md {
  height: 36px;
  font-size: var(--t-base);
  min-width: 90px;
}
</style>
