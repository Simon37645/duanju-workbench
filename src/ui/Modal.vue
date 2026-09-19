<script setup lang="ts">
import { onBeforeUnmount, watch } from "vue";
import { X } from "@lucide/vue";

const props = withDefaults(
  defineProps<{
    show: boolean;
    title?: string;
    subtitle?: string;
    width?: number;
    /** 贴右侧的抽屉样式；false 为居中弹窗 */
    drawer?: boolean;
  }>(),
  { width: 460, drawer: false },
);
const emit = defineEmits<{ "update:show": [boolean] }>();

function close() {
  emit("update:show", false);
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") close();
}

watch(
  () => props.show,
  (v) => {
    if (v) window.addEventListener("keydown", onKey);
    else window.removeEventListener("keydown", onKey);
  },
);
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <Teleport to="body">
    <Transition name="mask">
      <div v-if="show" class="mask" @click.self="close">
        <div
          class="sheet"
          :class="{ drawer }"
          :style="{ width: drawer ? width + 'px' : Math.min(width, 92) + 'vw' }"
          @click.stop
        >
          <header v-if="title" class="head">
            <div class="col" style="gap: 1px; min-width: 0">
              <h2 class="title">{{ title }}</h2>
              <p v-if="subtitle" class="sub t-xs faint">{{ subtitle }}</p>
            </div>
            <button class="x" type="button" @click="close"><X :size="15" /></button>
          </header>
          <div class="body scroll"><slot /></div>
          <footer v-if="$slots.footer" class="foot"><slot name="footer" /></footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.mask {
  position: fixed;
  inset: 0;
  z-index: 200;
  background: var(--mask-bg);
  backdrop-filter: blur(3px);
  display: flex;
}
.sheet {
  margin: auto;
  background: var(--surface-1);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-xl);
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
  max-height: 86vh;
  overflow: hidden;
}
.sheet.drawer {
  margin: 0 0 0 auto;
  height: 100%;
  max-height: 100%;
  border-radius: 0;
  border-right: none;
  border-top: none;
  border-bottom: none;
}
.head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--sp-3);
  padding: var(--sp-4) var(--sp-5);
  border-bottom: 1px solid var(--line-faint);
  flex: 0 0 auto;
}
.title {
  font-size: var(--t-md);
  font-weight: 600;
}
.x {
  background: none;
  border: none;
  color: var(--fg-faint);
  cursor: pointer;
  border-radius: var(--r-sm);
  width: 26px;
  height: 26px;
  display: grid;
  place-items: center;
  flex: 0 0 auto;
}
.x:hover {
  background: var(--surface-3);
  color: var(--fg);
}
.body {
  flex: 1;
  min-height: 0;
  padding: var(--sp-5);
}
.foot {
  flex: 0 0 auto;
  padding: var(--sp-3) var(--sp-5);
  border-top: 1px solid var(--line-faint);
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--sp-2);
}

.mask-enter-active,
.mask-leave-active {
  transition: opacity 160ms var(--ease);
}
.mask-enter-from,
.mask-leave-to {
  opacity: 0;
}
.mask-enter-active .sheet {
  transition: transform 200ms var(--ease);
}
.mask-enter-from .sheet {
  transform: translateY(8px) scale(0.99);
}
.mask-enter-from .sheet.drawer {
  transform: translateX(24px);
}
</style>
