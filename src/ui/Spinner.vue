<script setup lang="ts">
import { Loader2 } from "@lucide/vue";

withDefaults(defineProps<{ size?: number; label?: string }>(), { size: 14 });
</script>

<template>
  <span class="spinner row">
    <Loader2 class="spin" :size="size" />
    <span v-if="label" class="faint t-sm">{{ label }}</span>
  </span>
</template>

<style scoped>
.spinner {
  display: inline-flex;
  color: var(--fg-faint);
}
.spin {
  animation: spin 900ms linear infinite;
}
</style>
