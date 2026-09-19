<script setup lang="ts">
import { AlertTriangle, Check, Info, X } from "@lucide/vue";
import { toasts } from "./toast";

const icon = { ok: Check, err: AlertTriangle, warn: AlertTriangle, info: Info };
</script>

<template>
  <Teleport to="body">
    <div class="host">
      <TransitionGroup name="toast">
        <div v-for="t in toasts" :key="t.id" class="toast fade-in" :class="t.tone">
          <component :is="icon[t.tone]" :size="14" class="ic" />
          <span class="txt">{{ t.text }}</span>
          <button class="x" type="button" @click="toasts.splice(toasts.indexOf(t), 1)">
            <X :size="12" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.host {
  position: fixed;
  z-index: 500;
  bottom: 18px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: center;
  pointer-events: none;
}
.toast {
  pointer-events: auto;
  display: flex;
  align-items: flex-start;
  gap: 9px;
  min-width: 260px;
  max-width: 460px;
  padding: 10px 12px;
  background: var(--surface-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-lg);
  box-shadow: var(--shadow-lg);
  font-size: var(--t-sm);
}
.ic {
  flex: 0 0 auto;
  margin-top: 2px;
}
.txt {
  flex: 1;
  min-width: 0;
  line-height: 1.55;
  word-break: break-word;
}
.x {
  background: none;
  border: none;
  color: var(--fg-ghost);
  cursor: pointer;
  padding: 0;
  margin-top: 2px;
}
.x:hover {
  color: var(--fg);
}
.ok .ic {
  color: var(--ok);
}
.warn .ic {
  color: var(--warn);
}
.err .ic {
  color: var(--err);
}
.info .ic {
  color: var(--info);
}
.toast-enter-active,
.toast-leave-active {
  transition: opacity 180ms var(--ease), transform 180ms var(--ease);
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(8px);
}
</style>
