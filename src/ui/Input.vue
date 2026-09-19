<script setup lang="ts">
withDefaults(
  defineProps<{
    modelValue?: string;
    placeholder?: string;
    size?: "sm" | "md";
    disabled?: boolean;
    password?: boolean;
    invalid?: boolean;
  }>(),
  { modelValue: "", size: "sm" },
);
const emit = defineEmits<{ "update:modelValue": [string]; enter: []; blur: [] }>();
</script>

<template>
  <input
    class="input"
    :class="[`s-${size}`, { invalid }]"
    :type="password ? 'password' : 'text'"
    :value="modelValue"
    :placeholder="placeholder"
    :disabled="disabled"
    @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
    @keydown.enter="emit('enter')"
    @blur="emit('blur')"
  />
</template>

<style scoped>
.input {
  width: 100%;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  color: var(--fg);
  transition: border-color var(--fast), background var(--fast), box-shadow var(--fast);
}
.input::placeholder {
  color: var(--fg-ghost);
}
.input:hover:not(:disabled) {
  border-color: var(--line-strong);
}
.input:focus {
  border-color: var(--accent-line);
  box-shadow: var(--ring);
  background: var(--surface-2);
}
.input:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.input.invalid {
  border-color: rgba(248, 113, 113, 0.5);
}
.s-sm {
  height: 30px;
  padding: 0 10px;
  font-size: var(--t-sm);
}
.s-md {
  height: 36px;
  padding: 0 12px;
  font-size: var(--t-base);
}
</style>
