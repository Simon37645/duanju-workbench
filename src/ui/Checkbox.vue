<script setup lang="ts">
import { Check, Minus } from "@lucide/vue";

const props = withDefaults(
  defineProps<{
    modelValue?: boolean;
    indeterminate?: boolean;
    disabled?: boolean;
    label?: string;
  }>(),
  {},
);
const emit = defineEmits<{ "update:modelValue": [boolean] }>();
</script>

<template>
  <label class="cb" :class="{ disabled }" @click.prevent="!disabled && emit('update:modelValue', !modelValue)">
    <span class="box" :class="{ on: modelValue || indeterminate }">
      <Minus v-if="indeterminate && !modelValue" :size="10" />
      <Check v-else-if="modelValue" :size="10" />
    </span>
    <span v-if="label || $slots.default" class="txt"><slot>{{ label }}</slot></span>
  </label>
</template>

<style scoped>
.cb {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  cursor: pointer;
  user-select: none;
  font-size: var(--t-sm);
  color: var(--fg-dim);
}
.cb:hover:not(.disabled) {
  color: var(--fg);
}
.cb.disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.box {
  width: 15px;
  height: 15px;
  flex: 0 0 auto;
  border-radius: var(--r-xs);
  border: 1px solid var(--line-strong);
  background: var(--surface-3);
  display: grid;
  place-items: center;
  color: var(--accent-fg);
  transition: background var(--fast), border-color var(--fast);
}
.box.on {
  background: var(--accent);
  border-color: var(--accent);
}
.txt {
  min-width: 0;
}
</style>
