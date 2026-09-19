<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Check, ChevronDown, Search } from "@lucide/vue";

export interface Option {
  label: string;
  value: string;
  hint?: string;
  disabled?: boolean;
}

const props = withDefaults(
  defineProps<{
    modelValue?: string | null;
    options: Option[];
    placeholder?: string;
    size?: "sm" | "md";
    clearable?: boolean;
    searchable?: boolean;
    disabled?: boolean;
    /** 允许输入自定义值（下拉里没有的） */
    creatable?: boolean;
  }>(),
  { size: "sm", placeholder: "请选择" },
);
const emit = defineEmits<{ "update:modelValue": [string | null] }>();

const open = ref(false);
const query = ref("");
const root = ref<HTMLElement | null>(null);

const current = computed(() => props.options.find((o) => o.value === props.modelValue));
const filtered = computed(() => {
  if (!props.searchable || !query.value.trim()) return props.options;
  const q = query.value.trim().toLowerCase();
  return props.options.filter(
    (o) => o.label.toLowerCase().includes(q) || o.value.toLowerCase().includes(q),
  );
});

function toggle() {
  if (props.disabled) return;
  open.value = !open.value;
  if (open.value) query.value = "";
}

function pick(o: Option) {
  if (o.disabled) return;
  emit("update:modelValue", o.value);
  open.value = false;
}

function clear(e: Event) {
  e.stopPropagation();
  emit("update:modelValue", null);
}

function onDocClick(e: MouseEvent) {
  if (root.value && !root.value.contains(e.target as Node)) open.value = false;
}

watch(open, (v) => {
  if (v) document.addEventListener("mousedown", onDocClick);
  else document.removeEventListener("mousedown", onDocClick);
});
</script>

<template>
  <div ref="root" class="sel" :class="[`s-${size}`, { open, disabled }]">
    <button type="button" class="trigger" :disabled="disabled" @click="toggle">
      <span class="value" :class="{ ph: !current && !modelValue }">
        {{ current?.label ?? modelValue ?? placeholder }}
      </span>
      <button v-if="clearable && modelValue" class="clear" type="button" @click="clear">×</button>
      <ChevronDown class="caret" :size="13" />
    </button>

    <div v-if="open" class="menu fade-in">
      <div v-if="searchable" class="search">
        <Search :size="12" class="faint" />
        <input
          v-model="query"
          class="sinput"
          :placeholder="creatable ? '搜索或输入后回车' : '搜索…'"
          @keydown.enter="creatable && query.trim() && (emit('update:modelValue', query.trim()), (open = false))"
        />
      </div>
      <div class="list">
        <button
          v-for="o in filtered"
          :key="o.value"
          type="button"
          class="opt"
          :class="{ on: o.value === modelValue, dis: o.disabled }"
          @click="pick(o)"
        >
          <Check v-if="o.value === modelValue" :size="12" class="tick" />
          <span class="olabel truncate">{{ o.label }}</span>
          <span v-if="o.hint" class="ohint">{{ o.hint }}</span>
        </button>
        <div v-if="!filtered.length" class="none t-xs faint">没有匹配项</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sel {
  position: relative;
  min-width: 0;
}
.trigger {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 6px;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
  color: var(--fg);
  cursor: pointer;
  padding: 0 9px;
  transition: border-color var(--fast), background var(--fast);
}
.trigger:hover:not(:disabled) {
  border-color: var(--line-strong);
}
.sel.open .trigger {
  border-color: var(--accent-line);
  box-shadow: var(--ring);
}
.disabled .trigger {
  opacity: 0.5;
  cursor: not-allowed;
}
.s-sm .trigger {
  height: 30px;
  font-size: var(--t-sm);
}
.s-md .trigger {
  height: 36px;
  font-size: var(--t-base);
}
.value {
  flex: 1;
  min-width: 0;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.value.ph {
  color: var(--fg-ghost);
}
.caret {
  color: var(--fg-faint);
  flex: 0 0 auto;
}
.clear {
  background: none;
  border: none;
  color: var(--fg-faint);
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
  padding: 0 2px;
}
.clear:hover {
  color: var(--fg);
}

.menu {
  position: absolute;
  z-index: 60;
  top: calc(100% + 4px);
  left: 0;
  min-width: 100%;
  max-height: 280px;
  display: flex;
  flex-direction: column;
  background: var(--surface-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-lg);
  box-shadow: var(--shadow-lg);
  overflow: hidden;
}
.search {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--line-faint);
}
.sinput {
  flex: 1;
  background: none;
  border: none;
  color: var(--fg);
  font-size: var(--t-sm);
}
.sinput::placeholder {
  color: var(--fg-ghost);
}
.list {
  overflow: auto;
  padding: 4px;
  min-height: 0;
}
.opt {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 6px;
  background: none;
  border: none;
  border-radius: var(--r-sm);
  color: var(--fg-dim);
  cursor: pointer;
  padding: 6px 8px;
  font-size: var(--t-sm);
  text-align: left;
}
.opt:hover {
  background: var(--surface-4);
  color: var(--fg);
}
.opt.on {
  color: var(--accent);
  background: var(--accent-soft);
}
.opt.dis {
  opacity: 0.4;
  cursor: not-allowed;
}
.tick {
  flex: 0 0 auto;
}
.olabel {
  flex: 1;
  min-width: 0;
}
.ohint {
  color: var(--fg-ghost);
  font-size: var(--t-xs);
}
.none {
  padding: 10px;
  text-align: center;
}
</style>
