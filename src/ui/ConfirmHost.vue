<script setup lang="ts">
import { AlertTriangle } from "@lucide/vue";
import Modal from "./Modal.vue";
import Button from "./Button.vue";
import { confirmState, resolveConfirm } from "./confirm";
</script>

<template>
  <Modal
    :show="confirmState.open"
    :width="400"
    @update:show="(v: boolean) => !v && resolveConfirm(false)"
  >
    <div class="col" style="gap: 10px">
      <div class="row" style="gap: 9px">
        <AlertTriangle
          v-if="confirmState.opts?.danger"
          :size="16"
          style="color: var(--err); flex: 0 0 auto"
        />
        <h3 style="font-size: var(--t-md); font-weight: 600">{{ confirmState.opts?.title }}</h3>
      </div>
      <p v-if="confirmState.opts?.content" class="t-sm dim" style="line-height: 1.7">
        {{ confirmState.opts.content }}
      </p>
    </div>
    <template #footer>
      <Button variant="ghost" @click="resolveConfirm(false)">
        {{ confirmState.opts?.negativeText ?? "取消" }}
      </Button>
      <Button
        :variant="confirmState.opts?.danger ? 'danger' : 'primary'"
        @click="resolveConfirm(true)"
      >
        {{ confirmState.opts?.positiveText ?? "确定" }}
      </Button>
    </template>
  </Modal>
</template>
