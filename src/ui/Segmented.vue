<script setup lang="ts">
export interface SegItem {
  label: string;
  value: string;
  icon?: unknown;
}

const props = withDefaults(
  defineProps<{ modelValue?: string; items: SegItem[]; size?: "xs" | "sm" }>(),
  { size: "sm" },
);
const emit = defineEmits<{ "update:modelValue": [string] }>();
</script>

<template>
  <div class="seg" :class="`s-${props.size}`">
    <button
      v-for="it in items"
      :key="it.value"
      type="button"
      class="it"
      :class="{ on: it.value === modelValue }"
      @click="emit('update:modelValue', it.value)"
    >
      <component :is="it.icon" v-if="it.icon" :size="12" />
      <span>{{ it.label }}</span>
    </button>
  </div>
</template>

<style scoped>
.seg {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  padding: 2px;
}
.it {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  background: none;
  border: none;
  border-radius: var(--r-sm);
  color: var(--fg-faint);
  cursor: pointer;
  white-space: nowrap;
  transition: background var(--fast), color var(--fast);
}
.it:hover {
  color: var(--fg);
}
.it.on {
  background: var(--surface-1);
  color: var(--fg);
  box-shadow: var(--shadow-xs);
}
.s-xs .it {
  height: 20px;
  padding: 0 7px;
  font-size: var(--t-xs);
}
.s-sm .it {
  height: 24px;
  padding: 0 10px;
  font-size: var(--t-sm);
}
</style>
