<script setup lang="ts">
/** 小徽标：状态、标签、计数都用它，统一视觉 */
const props = withDefaults(
  defineProps<{
    tone?: "neutral" | "accent" | "ok" | "warn" | "err" | "info" | "violet";
    size?: "xs" | "sm";
    dot?: boolean;
    soft?: boolean;
  }>(),
  { tone: "neutral", size: "sm", soft: true },
);
</script>

<template>
  <span class="badge" :class="[`t-${props.tone}`, `s-${props.size}`, { soft: props.soft }]">
    <span v-if="dot" class="dot" />
    <slot />
  </span>
</template>

<style scoped>
.badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  border-radius: var(--r-sm);
  font-weight: 500;
  white-space: nowrap;
  border: 1px solid transparent;
  line-height: 1;
}
.s-xs {
  height: 17px;
  padding: 0 5px;
  font-size: 10px;
}
.s-sm {
  height: 20px;
  padding: 0 7px;
  font-size: var(--t-xs);
}
.dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: currentColor;
  flex: 0 0 auto;
}

.t-neutral {
  color: var(--fg-dim);
  background: var(--surface-4);
  border-color: var(--line);
}
.t-accent {
  color: var(--accent);
  background: var(--accent-soft);
  border-color: var(--accent-line);
}
.t-ok {
  color: var(--ok);
  background: var(--ok-soft);
  border-color: rgba(74, 222, 128, 0.25);
}
.t-warn {
  color: var(--warn);
  background: var(--warn-soft);
  border-color: rgba(251, 191, 36, 0.25);
}
.t-err {
  color: var(--err);
  background: var(--err-soft);
  border-color: rgba(248, 113, 113, 0.25);
}
.t-info {
  color: var(--info);
  background: var(--info-soft);
  border-color: rgba(96, 165, 250, 0.25);
}
.t-violet {
  color: var(--violet);
  background: var(--violet-soft);
  border-color: rgba(167, 139, 250, 0.25);
}
</style>
