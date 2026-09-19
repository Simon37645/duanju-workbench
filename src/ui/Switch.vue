<script setup lang="ts">
const props = withDefaults(
  defineProps<{ modelValue?: boolean; disabled?: boolean; size?: "sm" | "md" }>(),
  { size: "sm" },
);
const emit = defineEmits<{ "update:modelValue": [boolean] }>();
</script>

<template>
  <button
    type="button"
    class="sw"
    :class="[`s-${size}`, { on: modelValue, disabled }]"
    :disabled="disabled"
    role="switch"
    :aria-checked="!!modelValue"
    @click="emit('update:modelValue', !modelValue)"
  >
    <span class="knob" />
  </button>
</template>

<style scoped>
.sw {
  position: relative;
  flex: 0 0 auto;
  border: 1px solid var(--line);
  border-radius: var(--r-full);
  background: var(--surface-4);
  cursor: pointer;
  transition: background var(--fast), border-color var(--fast);
  padding: 0;
}
.sw.on {
  background: var(--accent);
  border-color: transparent;
}
.sw.disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.knob {
  position: absolute;
  top: 50%;
  left: 2px;
  transform: translateY(-50%);
  background: #fff;
  border-radius: 50%;
  box-shadow: var(--shadow-xs);
  transition: left var(--normal);
}
.sw.on .knob {
  background: var(--accent-fg);
}
.s-sm {
  width: 32px;
  height: 18px;
}
.s-sm .knob {
  width: 12px;
  height: 12px;
}
.s-sm.on .knob {
  left: 16px;
}
.s-md {
  width: 40px;
  height: 22px;
}
.s-md .knob {
  width: 16px;
  height: 16px;
}
.s-md.on .knob {
  left: 21px;
}
</style>
