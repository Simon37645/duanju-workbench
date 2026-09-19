<script setup lang="ts">
/**
 * 技能面板。
 *
 * 重点是让用户理解「三级渐进披露」——否则会以为没生效：
 * 名称和描述永远在提示里，正文和附件都是模型按需取的。
 */
import { computed, onMounted, ref } from "vue";
import {
  ChevronDown, FileText, FolderOpen, FolderUp, Import, Paperclip, Trash2,
} from "@lucide/vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import UiButton from "@/ui/Button.vue";
import UiBadge from "@/ui/Badge.vue";
import UiSwitch from "@/ui/Switch.vue";
import UiEmpty from "@/ui/Empty.vue";
import UiSpinner from "@/ui/Spinner.vue";
import { toast } from "@/ui";
import { api, errorText } from "@/api/ipc";
import type { Skill } from "@/types/models";

const skills = ref<Skill[]>([]);
const loading = ref(true);
const expanded = ref<Record<string, boolean>>({});
const bodies = ref<Record<string, string>>({});

const enabledCount = computed(() => skills.value.filter((s) => s.enabled).length);
const totalChars = computed(() => skills.value.filter((s) => s.enabled).reduce((a, s) => a + s.chars, 0));

onMounted(load);

async function load() {
  loading.value = true;
  try {
    skills.value = await api.skillList();
  } catch (e) {
    toast.err(errorText(e));
  } finally {
    loading.value = false;
  }
}

/** 导入文件：可选多个 .md 或 .zip */
async function importFiles() {
  const picked = await openDialog({
    multiple: true,
    filters: [
      { name: "技能包 / 文档", extensions: ["zip", "md", "markdown", "txt"] },
      { name: "压缩包", extensions: ["zip"] },
      { name: "Markdown", extensions: ["md", "markdown", "txt"] },
    ],
    title: "选择技能压缩包或 Markdown（可多选）",
  });
  const list = Array.isArray(picked) ? picked : picked ? [picked] : [];
  if (!list.length) return;
  await doImport(list);
}

/** 导入目录：可以是单个技能目录，也可以是装着多个技能的合集目录 */
async function importDir() {
  const picked = await openDialog({
    directory: true,
    multiple: true,
    title: "选择技能目录，或装着多个技能的合集目录",
  });
  const list = Array.isArray(picked) ? picked : picked ? [picked] : [];
  if (!list.length) return;
  await doImport(list);
}

async function doImport(paths: string[]) {
  try {
    const added = await api.skillImport(paths);
    await load();
    toast.ok(`已导入 ${added.length} 个技能`);
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function openDir() {
  try {
    await openPath(await api.skillOpenDir());
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function toggle(s: Skill, v: boolean) {
  try {
    await api.skillSetEnabled(s.id, v);
    s.enabled = v;
    toast.info(v ? "已启用，下一轮对话生效" : "已停用");
  } catch (e) {
    toast.err(errorText(e));
  }
}

async function remove(s: Skill) {
  try {
    await api.skillDelete(s.id);
    await load();
    toast.ok(`已删除「${s.name}」`);
  } catch (e) {
    toast.err(errorText(e));
  }
}

/** 展开时按需拉正文 —— 和模型的读取方式一致 */
async function expand(s: Skill) {
  const next = !expanded.value[s.id];
  expanded.value[s.id] = next;
  if (next && !bodies.value[s.id]) {
    try {
      bodies.value[s.id] = await api.skillRead(s.id);
    } catch (e) {
      bodies.value[s.id] = `读取失败：${errorText(e)}`;
    }
  }
}

async function previewFile(s: Skill, path: string) {
  try {
    const text = await api.skillReadFile(s.id, path);
    bodies.value[s.id + "::" + path] = text;
    expanded.value[s.id + "::" + path] = !expanded.value[s.id + "::" + path];
  } catch (e) {
    toast.err(errorText(e));
  }
}
</script>

<template>
  <div class="col" style="gap: var(--sp-3)">
    <div class="hint">
      <div class="row-between">
        <span class="section-label">技能</span>
        <span class="t-xs faint">
          {{ enabledCount }} / {{ skills.length }} 启用中 ·
          正文合计 {{ (totalChars / 1000).toFixed(1) }}k 字
        </span>
      </div>
      <p class="t-xs" style="line-height: 1.85; margin-top: 6px">
        把你的流程、规范、方法论做成技能放进来，Simon 会在用得上时自己取。
        三级渐进披露决定了成本：
      </p>
      <div class="levels">
        <div class="lv">
          <span class="n">1</span>
          <div>
            <b>名称 + 描述</b>
            <span class="faint">永远在提示里，决定模型要不要用它</span>
          </div>
        </div>
        <div class="lv">
          <span class="n">2</span>
          <div>
            <b>SKILL.md 正文</b>
            <span class="faint">模型觉得对得上时才读</span>
          </div>
        </div>
        <div class="lv">
          <span class="n">3</span>
          <div>
            <b>附件</b>
            <span class="faint">正文里指明要看哪个时再读</span>
          </div>
        </div>
      </div>
      <p class="t-xs faint" style="line-height: 1.7; margin-top: 8px">
        所以装几十个技能，前缀里也只有几十行目录，正文永远不会为无关话题付费。
        <b>description 要写「什么时候用」，别写「这是什么」</b> —— 模型只靠它做判断。
      </p>
      <div class="row wrap" style="gap: 6px; margin-top: 10px">
        <UiButton variant="outline" size="sm" @click="importFiles">
          <template #icon><Import :size="13" /></template>
          导入 zip / md
        </UiButton>
        <UiButton variant="outline" size="sm" @click="importDir">
          <template #icon><FolderUp :size="13" /></template>
          导入技能目录
        </UiButton>
        <UiButton variant="subtle" size="sm" @click="openDir">
          <template #icon><FolderOpen :size="13" /></template>
          打开目录
        </UiButton>
      </div>
      <p class="t-xs faint" style="margin-top: 6px; line-height: 1.7">
        压缩包最省事：直接选 <span class="mono">.zip</span>（可多选），会自动找出里面的所有技能 ——
        不管是「一个 zip 一个技能」还是「一个 zip 一整个合集」都认。
        也可以选文件夹：「导入技能目录」支持单个技能目录，也支持装着多个技能的合集目录。
      </p>
    </div>

    <UiSpinner v-if="loading" label="正在读取技能…" />

    <UiEmpty
      v-else-if="!skills.length"
      title="还没有技能"
      hint="导入 Markdown，或把技能目录（含 SKILL.md）丢进来"
    >
      <template #icon><FileText :size="26" /></template>
      <UiButton variant="outline" size="sm" @click="importDir">导入技能目录</UiButton>
    </UiEmpty>

    <div v-else class="col" style="gap: 8px">
      <div v-for="s in skills" :key="s.id" class="skill">
        <div class="row" style="gap: 8px; min-width: 0">
          <FileText :size="14" class="faint" style="flex: 0 0 auto" />
          <div class="col grow" style="gap: 2px; min-width: 0">
            <div class="row wrap" style="gap: 6px">
              <span class="t-sm" style="font-weight: 500">{{ s.name }}</span>
              <UiBadge v-if="s.dir" tone="info" size="xs">
                <Paperclip :size="9" /> {{ s.files.length }} 个附件
              </UiBadge>
              <span class="t-xs faint num">{{ (s.chars / 1000).toFixed(1) }}k 字</span>
            </div>
            <span class="t-xs faint" style="line-height: 1.6">{{ s.description }}</span>
          </div>
          <div class="row" style="gap: 4px; flex: 0 0 auto">
            <UiSwitch :model-value="s.enabled" @update:model-value="(v: boolean) => toggle(s, v)" />
            <UiButton variant="subtle" size="xs" @click="expand(s)">
              <template #icon>
                <ChevronDown :size="12" :class="{ rot: !expanded[s.id] }" />
              </template>
              正文
            </UiButton>
            <UiButton variant="subtle" size="xs" icon @click="remove(s)">
              <template #icon><Trash2 :size="12" /></template>
            </UiButton>
          </div>
        </div>

        <!-- 附件 -->
        <div v-if="s.files.length" class="files">
          <button v-for="f in s.files" :key="f" class="file" @click="previewFile(s, f)">
            <Paperclip :size="10" />
            <span class="truncate">{{ f }}</span>
          </button>
        </div>
        <pre
          v-for="f in s.files.filter((x) => expanded[s.id + '::' + x])"
          :key="f + '-body'"
          class="preview"
          >{{ bodies[s.id + "::" + f]?.slice(0, 4000) }}</pre
        >

        <pre v-if="expanded[s.id]" class="preview">{{
          (bodies[s.id] ?? "读取中…").slice(0, 6000)
        }}</pre>
      </div>
    </div>
  </div>
</template>

<style scoped>
.hint {
  padding: 12px;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
}
.levels {
  display: flex;
  flex-direction: column;
  gap: 5px;
  margin-top: 8px;
}
.lv {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  font-size: var(--t-xs);
  line-height: 1.6;
}
.lv .n {
  width: 16px;
  height: 16px;
  flex: 0 0 auto;
  border-radius: var(--r-xs);
  background: var(--accent-soft);
  color: var(--accent);
  border: 1px solid var(--accent-line);
  display: grid;
  place-items: center;
  font-size: 9px;
  margin-top: 1px;
}
.lv b {
  color: var(--fg);
  font-weight: 600;
  margin-right: 6px;
}
.skill {
  padding: 10px;
  background: var(--surface-3);
  border: 1px solid var(--line);
  border-radius: var(--r);
}
.skill:hover {
  border-color: var(--line-strong);
}
.files {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 7px;
  padding-left: 22px;
}
.file {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: var(--surface-4);
  border: 1px solid var(--line);
  border-radius: var(--r-xs);
  color: var(--fg-faint);
  font-size: 10px;
  padding: 2px 7px;
  cursor: pointer;
  max-width: 220px;
}
.file:hover {
  color: var(--accent);
  border-color: var(--accent-line);
}
.preview {
  margin: 8px 0 0;
  padding: 10px 12px;
  background: var(--code-bg);
  border-radius: var(--r-sm);
  color: var(--fg-dim);
  font-size: var(--t-xs);
  line-height: 1.7;
  white-space: pre-wrap;
  max-height: 260px;
  overflow: auto;
  font-family: inherit;
}
.rot {
  transform: rotate(-90deg);
}
</style>
