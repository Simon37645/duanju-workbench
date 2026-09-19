<script setup lang="ts">
withDefaults(
  defineProps<{
    title?: string;
    hint?: string;
    compact?: boolean;
  }>(),
  { compact: false },
);
</script>

<template>
  <div class="empty" :class="{ compact }">
    <div class="icon">
      <slot name="icon" />
    </div>
    <div v-if="title" class="title">{{ title }}</div>
    <div v-if="hint" class="hint">{{ hint }}</div>
    <div v-if="$slots.default" class="actions"><slot /></div>
  </div>
</template>

<style scoped>
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 48px 24px;
  text-align: center;
  color: var(--fg-faint);
}
.empty.compact {
  padding: 24px 16px;
  gap: 6px;
}
.icon {
  color: var(--fg-ghost);
  margin-bottom: 2px;
}
.title {
  color: var(--fg-dim);
  font-size: var(--t-base);
}
.hint {
  font-size: var(--t-sm);
  max-width: 380px;
  line-height: 1.7;
}
.actions {
  margin-top: 6px;
  display: flex;
  gap: 6px;
}
</style>
