<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";

const props = withDefaults(
  defineProps<{ width?: number; placement?: "bottom" | "bottom-end" | "right" }>(),
  { width: 260, placement: "bottom" },
);
const open = ref(false);
const root = ref<HTMLElement | null>(null);

function onDoc(e: MouseEvent) {
  if (root.value && !root.value.contains(e.target as Node)) open.value = false;
}
watch(open, (v) => {
  if (v) document.addEventListener("mousedown", onDoc);
  else document.removeEventListener("mousedown", onDoc);
});
onBeforeUnmount(() => document.removeEventListener("mousedown", onDoc));

defineExpose({ close: () => (open.value = false) });
</script>

<template>
  <div ref="root" class="pop">
    <div class="trigger" @click="open = !open"><slot name="trigger" /></div>
    <Transition name="pop">
      <div v-if="open" class="panel fade-in" :class="placement" :style="{ width: width + 'px' }">
        <slot />
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.pop {
  position: relative;
  display: inline-flex;
  min-width: 0;
}
.trigger {
  display: inline-flex;
  min-width: 0;
  cursor: pointer;
}
.panel {
  position: absolute;
  z-index: 220;
  top: calc(100% + 6px);
  background: var(--surface-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-lg);
  box-shadow: var(--shadow-lg);
  padding: var(--sp-3);
  max-height: 70vh;
  overflow: auto;
}
.bottom {
  left: 0;
}
.bottom-end {
  right: 0;
}
.right {
  top: 0;
  left: calc(100% + 6px);
}
.pop-enter-active,
.pop-leave-active {
  transition: opacity 120ms var(--ease);
}
.pop-enter-from,
.pop-leave-to {
  opacity: 0;
}
</style>
