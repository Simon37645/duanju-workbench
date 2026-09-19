<script setup lang="ts">
withDefaults(
  defineProps<{
    modelValue?: string;
    placeholder?: string;
    rows?: number;
    disabled?: boolean;
    mono?: boolean;
    resize?: boolean;
  }>(),
  { modelValue: "", rows: 3, resize: true },
);
const emit = defineEmits<{ "update:modelValue": [string]; blur: []; keydown: [KeyboardEvent] }>();
</script>

<template>
  <textarea
    class="ta"
    :class="{ mono, fixed: !resize }"
    :rows="rows"
    :value="modelValue"
    :placeholder="placeholder"
    :disabled="disabled"
    @input="emit('update:modelValue', ($event.target as HTMLTextAreaElement).value)"
    @blur="emit('blur')"
    @keydown="emit('keydown', $event as KeyboardEvent)"
  />
</template>

<style scoped>
.ta {
  width: 100%;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  color: var(--fg);
  padding: 9px 11px;
  font-size: var(--t-sm);
  line-height: 1.65;
  resize: vertical;
  transition: border-color var(--fast), box-shadow var(--fast), background var(--fast);
}
.ta.fixed {
  resize: none;
}
.ta::placeholder {
  color: var(--fg-ghost);
}
.ta:hover:not(:disabled) {
  border-color: var(--line-strong);
}
.ta:focus {
  border-color: var(--accent-line);
  box-shadow: var(--ring);
  background: var(--surface-2);
}
.ta:disabled {
  opacity: 0.5;
}
.mono {
  font-family: "JetBrains Mono", Consolas, monospace;
  font-size: var(--t-xs);
}
</style>
