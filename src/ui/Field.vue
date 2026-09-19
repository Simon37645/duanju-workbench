<script setup lang="ts">
/** 表单字段：统一 label + 说明 + 控件的排布，避免每个面板各写一套 */
withDefaults(defineProps<{ label?: string; hint?: string; inline?: boolean }>(), {});
</script>

<template>
  <div class="field" :class="{ inline }">
    <div v-if="label || $slots.label" class="head">
      <span class="label"><slot name="label">{{ label }}</slot></span>
      <span v-if="hint" class="hint">{{ hint }}</span>
      <slot name="head" />
    </div>
    <slot />
  </div>
</template>

<style scoped>
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}
.field.inline {
  flex-direction: row;
  align-items: center;
  justify-content: space-between;
}
.head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}
.label {
  font-size: var(--t-sm);
  color: var(--fg-dim);
  flex: 0 0 auto;
}
.hint {
  font-size: var(--t-xs);
  color: var(--fg-ghost);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
