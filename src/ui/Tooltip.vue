<script setup lang="ts">
import { ref } from "vue";

const props = withDefaults(
  defineProps<{ content?: string; placement?: "top" | "bottom" | "right"; delay?: number }>(),
  { placement: "top", delay: 120 },
);
const show = ref(false);
let timer: number | undefined;

function enter() {
  timer = window.setTimeout(() => (show.value = true), props.delay);
}
function leave() {
  window.clearTimeout(timer);
  show.value = false;
}
</script>

<template>
  <span class="tip-wrap" @mouseenter="enter" @mouseleave="leave" @mousedown="leave">
    <slot />
    <Transition name="tip">
      <span v-if="show && (content || $slots.content)" class="tip" :class="placement">
        <slot name="content">{{ content }}</slot>
      </span>
    </Transition>
  </span>
</template>

<style scoped>
.tip-wrap {
  position: relative;
  display: inline-flex;
  min-width: 0;
}
.tip {
  position: absolute;
  z-index: 300;
  background: var(--tooltip-bg);
  color: var(--tooltip-fg);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-sm);
  padding: 6px 9px;
  font-size: var(--t-xs);
  line-height: 1.55;
  max-width: 300px;
  width: max-content;
  box-shadow: var(--shadow);
  pointer-events: none;
}
.tip.top {
  bottom: calc(100% + 6px);
  left: 50%;
  transform: translateX(-50%);
}
.tip.bottom {
  top: calc(100% + 6px);
  left: 50%;
  transform: translateX(-50%);
}
.tip.right {
  left: calc(100% + 6px);
  top: 50%;
  transform: translateY(-50%);
}
.tip-enter-active,
.tip-leave-active {
  transition: opacity 100ms var(--ease);
}
.tip-enter-from,
.tip-leave-to {
  opacity: 0;
}
</style>
