import { defineStore } from "pinia";
import { api, errorText } from "@/api/ipc";
import { onProjectChanged } from "@/api/events";
import { message } from "@/utils/notify";
import type {
  Asset,
  AssetView,
  Chapter,
  ChapterMeta,
  PanelId,
  PanelProgress,
  ProjectSnapshot,
  Shot,
  StyleState,
  VideoPrompt,
} from "@/types/models";
import type { UnlistenFn } from "@tauri-apps/api/event";

interface State {
  snapshot: ProjectSnapshot | null;
  progress: PanelProgress[];
  /** 当前打开的章节正文（按章节 id 缓存） */
  chapterCache: Record<string, Chapter>;
  currentChapterId: string | null;
  loading: boolean;
  saving: boolean;
  _unlisten: UnlistenFn | null;
  /** 防止 agent 改动与本地编辑互相覆盖 */
  pendingLocalEdit: boolean;
}

export const useProjectStore = defineStore("project", {
  state: (): State => ({
    snapshot: null,
    progress: [],
    chapterCache: {},
    currentChapterId: null,
    loading: false,
    saving: false,
    _unlisten: null,
    pendingLocalEdit: false,
  }),

  getters: {
    root: (s) => s.snapshot?.root ?? "",
    manifest: (s) => s.snapshot?.manifest ?? null,
    chapters: (s) => s.snapshot?.chapters ?? [],
    shots: (s) => s.snapshot?.shots ?? [],
    assets: (s) => s.snapshot?.assets ?? [],
    prompts: (s) => s.snapshot?.prompts ?? [],
    takes: (s) => s.snapshot?.takes ?? [],
    subtitles: (s) => s.snapshot?.subtitles ?? [],
    timeline: (s) => s.snapshot?.timeline ?? null,
    checklist: (s) => s.snapshot?.checklist ?? { items: [] },
    style: (s) => s.snapshot?.style ?? null,
    bible: (s) => s.snapshot?.bible ?? null,

    progressOf: (s) => (panel: PanelId) =>
      s.progress.find((p) => p.panel === panel)?.percent ?? 0,
    blockersOf: (s) => (panel: PanelId) =>
      s.progress.find((p) => p.panel === panel)?.blockers ?? [],

    shotsOf: (s) => (chapterId: string) =>
      (s.snapshot?.shots ?? []).filter((x) => x.chapterId === chapterId),
    promptOf: (s) => (shotId: string) =>
      (s.snapshot?.prompts ?? []).find((p) => p.shotId === shotId) ?? null,
    takesOf: (s) => (shotId: string) =>
      (s.snapshot?.takes ?? []).filter((t) => t.shotId === shotId),
    assetByName: (s) => (name: string) =>
      (s.snapshot?.assets ?? []).find((a) => a.name === name || a.aliases.includes(name)) ?? null,
  },

  actions: {
    /** 绑定后端 project://changed 事件，agent 改完数据界面会自动刷新 */
    bind() {
      if (this._unlisten) return;
      onProjectChanged(async () => {
        await this.reload();
      }).then((fn) => {
        this._unlisten = fn;
      });
    },

    async reload() {
      if (!this.snapshot) return;
      if (this.pendingLocalEdit) return;
      const [snap, prog] = await Promise.all([api.projectSnapshot(), api.projectProgress()]);
      this.snapshot = snap;
      this.progress = prog;
      if (this.currentChapterId && !this.chapterCache[this.currentChapterId]) {
        await this.openChapter(this.currentChapterId);
      }
    },

    async ensure(force = false) {
      if (this.snapshot && !force) return this.snapshot;
      const [snap, prog] = await Promise.all([api.projectSnapshot(), api.projectProgress()]);
      this.snapshot = snap;
      this.progress = prog;
      if (!this.currentChapterId && snap.chapters.length) {
        await this.openChapter(snap.chapters[0].id);
      }
      return snap;
    },

    /** 启动时自动恢复上次打开的项目；打不开就安静地停在首页 */
    async restoreLast() {
      if (this.snapshot) return;
      const { useSettingsStore } = await import("./settings");
      const settings = useSettingsStore();
      const last = settings.recentProjects[0];
      if (!last) return;
      try {
        const snap = await api.projectOpen(last.path);
        this.snapshot = snap;
        this.progress = await api.projectProgress();
        this.bind();
      } catch {
        /* 目录被删或移动过，忽略 */
      }
    },

    async create(parentDir: string, name: string) {
      this.loading = true;
      try {
        const snap = await api.projectCreate(parentDir, name);
        this.snapshot = snap;
        this.chapterCache = {};
        this.currentChapterId = null;
        await this.refreshProgress();
        this.bind();
        return snap;
      } catch (e) {
        message.error(`新建项目失败：${errorText(e)}`);
        throw e;
      } finally {
        this.loading = false;
      }
    },

    async open(root: string) {
      this.loading = true;
      try {
        const snap = await api.projectOpen(root);
        this.snapshot = snap;
        this.chapterCache = {};
        this.currentChapterId = null;
        await this.refreshProgress();
        this.bind();
        return snap;
      } catch (e) {
        message.error(`打开项目失败：${errorText(e)}`);
        throw e;
      } finally {
        this.loading = false;
      }
    },

    async close() {
      await api.projectClose();
      this.snapshot = null;
      this.progress = [];
      this.chapterCache = {};
      this.currentChapterId = null;
    },

    async refreshProgress() {
      try {
        this.progress = await api.projectProgress();
      } catch {
        /* 没有项目时忽略 */
      }
    },

    /* ------------------------------------------------------------ 剧本 */

    async openChapter(id: string) {
      this.currentChapterId = id;
      if (this.chapterCache[id]) return this.chapterCache[id];
      const ch = await api.scriptReadChapter(id);
      this.chapterCache[id] = ch;
      return ch;
    },

    async saveChapter(id: string, content: string) {
      this.saving = true;
      this.pendingLocalEdit = true;
      try {
        const meta = await api.scriptSaveChapter(id, content);
        if (this.chapterCache[id]) this.chapterCache[id].content = content;
        if (this.snapshot) {
          const i = this.snapshot.chapters.findIndex((c) => c.id === id);
          if (i >= 0) this.snapshot.chapters[i] = meta;
        }
        await this.refreshProgress();
      } catch (e) {
        message.error(`保存章节失败：${errorText(e)}`);
      } finally {
        this.saving = false;
        this.pendingLocalEdit = false;
      }
    },

    async addChapter(title?: string, summary?: string) {
      const meta = await api.scriptAddChapter(title, summary);
      await this.reload();
      return meta;
    },

    async updateChapterMeta(id: string, patch: Partial<ChapterMeta>) {
      await api.scriptUpdateChapterMeta(id, patch);
      await this.reload();
    },

    async deleteChapter(id: string) {
      await api.scriptDeleteChapter(id);
      delete this.chapterCache[id];
      if (this.currentChapterId === id) this.currentChapterId = null;
      await this.reload();
    },

    /* ------------------------------------------------------------ 分镜 */

    async setShots(chapterId: string, shots: Shot[]) {
      await api.storyboardSetShots(chapterId, shots);
      await this.reload();
      await this.refreshProgress();
    },

    async upsertShot(shot: Shot) {
      await api.storyboardUpsertShot(shot);
      await this.reload();
    },

    async deleteShot(id: string) {
      await api.storyboardDeleteShot(id);
      await this.reload();
    },

    /* ------------------------------------------------------------ 资产 */

    async upsertAsset(asset: Asset) {
      const saved = await api.assetUpsert(asset);
      await this.reload();
      await this.refreshProgress();
      return saved;
    },

    async deleteAsset(id: string) {
      await api.assetDelete(id);
      await this.reload();
    },

    async planViews(assetId: string, views: AssetView[]) {
      await api.assetPlanViews(assetId, views);
      await this.reload();
    },

    async generateViews(assetId: string, viewIds: string[], force = false) {
      const jobs = await api.assetGenerateViews(assetId, viewIds, force);
      await this.reload();
      return jobs;
    },

    /* ---------------------------------------------------------- 风格 */

    async setStyle(style: StyleState) {
      await api.styleSet(style);
      await this.reload();
    },

    /* -------------------------------------------------------- 提示词 */

    async upsertPrompt(p: VideoPrompt) {
      await api.promptUpsert(p);
      await this.reload();
      await this.refreshProgress();
    },

    async deletePrompt(id: string) {
      await api.promptDelete(id);
      await this.reload();
    },

    async autofillPrompts(chapterId?: string) {
      const n = await api.promptAutofill(chapterId);
      await this.reload();
      await this.refreshProgress();
      return n;
    },
  },
});
