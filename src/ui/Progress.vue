<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    value: number;
    tone?: "accent" | "ok" | "err" | "info" | "neutral";
    height?: number;
  }>(),
  { tone: "accent", height: 4 },
);

const pct = () => Math.max(0, Math.min(100, props.value));
</script>

<template>
  <div class="track" :style="{ height: props.height + 'px' }">
    <div class="fill" :class="`t-${tone}`" :style="{ width: pct() + '%' }" />
  </div>
</template>

<style scoped>
.track {
  width: 100%;
  background: var(--surface-4);
  border-radius: var(--r-full);
  overflow: hidden;
}
.fill {
  height: 100%;
  border-radius: var(--r-full);
  transition: width 240ms var(--ease);
}
.t-accent {
  background: var(--accent);
}
.t-ok {
  background: var(--ok);
}
.t-err {
  background: var(--err);
}
.t-info {
  background: var(--info);
}
.t-neutral {
  background: var(--fg-ghost);
}
</style>
