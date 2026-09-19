<script setup lang="ts">
import { computed } from "vue";
import { marked } from "marked";
import DOMPurify from "dompurify";

const props = defineProps<{ text: string; streaming?: boolean }>();

marked.setOptions({ breaks: true, gfm: true });

const html = computed(() => {
  const raw = marked.parse(props.text ?? "", { async: false }) as string;
  return DOMPurify.sanitize(raw, { USE_PROFILES: { html: true } });
});
</script>

<template>
  <div class="md" :class="{ caret: streaming }" v-html="html" />
</template>
