<script setup lang="ts">
import { computed } from "vue";
import { Loader2 } from "@lucide/vue";

const props = withDefaults(
  defineProps<{
    variant?: "primary" | "default" | "ghost" | "outline" | "danger" | "subtle";
    size?: "xs" | "sm" | "md";
    loading?: boolean;
    disabled?: boolean;
    block?: boolean;
    active?: boolean;
    icon?: boolean;
    type?: "button" | "submit";
  }>(),
  { variant: "default", size: "sm" },
);

const classes = computed(() => [
  "btn",
  `v-${props.variant}`,
  `s-${props.size}`,
  {
    loading: props.loading,
    block: props.block,
    active: props.active,
    icon: props.icon,
  },
]);
</script>

<template>
  <button
    :class="classes"
    :disabled="disabled || loading"
    :type="type ?? 'button'"
  >
    <Loader2 v-if="loading" class="spin" :size="size === 'xs' ? 12 : 14" />
    <slot v-else name="icon" />
    <span v-if="$slots.default" class="label"><slot /></span>
  </button>
</template>

<style scoped>
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border: 1px solid transparent;
  border-radius: var(--r);
  font-weight: 500;
  white-space: nowrap;
  /* 关键：不参与收缩。否则放在窄的 flex 工具栏里会被压成一列竖排的字 */
  flex: 0 0 auto;
  cursor: pointer;
  user-select: none;
  transition: background var(--fast), border-color var(--fast), color var(--fast),
    box-shadow var(--fast), transform var(--fast);
}
.btn:active:not(:disabled) {
  transform: translateY(0.5px);
}
.btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.btn.block {
  width: 100%;
}
.spin {
  animation: spin 900ms linear infinite;
}

/* ------------------------------------------------------------ 尺寸 */
.s-xs {
  height: 24px;
  padding: 0 8px;
  font-size: var(--t-xs);
  border-radius: var(--r-sm);
}
.s-sm {
  height: 30px;
  padding: 0 11px;
  font-size: var(--t-sm);
}
.s-md {
  height: 36px;
  padding: 0 15px;
  font-size: var(--t-base);
}
.icon.s-xs {
  width: 24px;
  padding: 0;
}
.icon.s-sm {
  width: 30px;
  padding: 0;
}
.icon.s-md {
  width: 36px;
  padding: 0;
}

/* ------------------------------------------------------------ 变体 */
.v-primary {
  background: var(--accent);
  color: var(--accent-fg);
  box-shadow: var(--shadow-xs);
}
.v-primary:hover:not(:disabled) {
  background: var(--accent-hover);
}

.v-default {
  background: var(--surface-3);
  color: var(--fg);
  border-color: var(--line);
}
.v-default:hover:not(:disabled) {
  background: var(--surface-4);
  border-color: var(--line-strong);
}

.v-outline {
  background: transparent;
  color: var(--fg);
  border-color: var(--line-strong);
}
.v-outline:hover:not(:disabled) {
  background: var(--surface-3);
}

.v-ghost {
  background: transparent;
  color: var(--fg-dim);
}
.v-ghost:hover:not(:disabled) {
  background: var(--surface-3);
  color: var(--fg);
}

/* 比 ghost 更弱：用于密集工具栏 */
.v-subtle {
  background: transparent;
  color: var(--fg-faint);
}
.v-subtle:hover:not(:disabled) {
  background: var(--surface-3);
  color: var(--fg);
}

.v-danger {
  background: transparent;
  color: var(--err);
  border-color: rgba(248, 113, 113, 0.25);
}
.v-danger:hover:not(:disabled) {
  background: var(--err-soft);
  border-color: rgba(248, 113, 113, 0.45);
}

.btn.active {
  background: var(--accent-soft);
  color: var(--accent);
  border-color: var(--accent-line);
}
</style>
