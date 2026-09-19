import { defineStore } from "pinia";
import { api, errorText } from "@/api/ipc";
import type { AppSettings, ProviderConfig, ProviderKind } from "@/types/models";
import { message } from "@/utils/notify";

interface State {
  settings: AppSettings | null;
  secrets: Record<string, boolean>;
  loading: boolean;
}

export const useSettingsStore = defineStore("settings", {
  state: (): State => ({
    settings: null,
    secrets: {},
    loading: false,
  }),

  getters: {
    providers(state): ProviderConfig[] {
      return state.settings?.providers ?? [];
    },
    byKind: (state) => (kind: ProviderKind) =>
      (state.settings?.providers ?? []).filter((p) => p.kind === kind),
    activeLlm(state): ProviderConfig | null {
      const s = state.settings;
      if (!s) return null;
      return s.providers.find((p) => p.id === s.activeLlmProviderId && p.enabled) ?? null;
    },
    activeImage(state): ProviderConfig | null {
      const s = state.settings;
      if (!s) return null;
      return s.providers.find((p) => p.id === s.activeImageProviderId && p.enabled) ?? null;
    },
    activeVideo(state): ProviderConfig | null {
      const s = state.settings;
      if (!s) return null;
      return s.providers.find((p) => p.id === s.activeVideoProviderId && p.enabled) ?? null;
    },
    recentProjects(state) {
      return state.settings?.recentProjects ?? [];
    },
  },

  actions: {
    async load() {
      this.loading = true;
      try {
        this.settings = await api.settingsGet();
        this.secrets = await api.secretsStatus();
      } catch (e) {
        message.error(`读取设置失败：${errorText(e)}`);
      } finally {
        this.loading = false;
      }
    },

    async save(patch?: Partial<AppSettings>) {
      if (!this.settings) return;
      if (patch) Object.assign(this.settings, patch);
      try {
        this.settings = await api.settingsSet(this.settings);
      } catch (e) {
        message.error(`保存设置失败：${errorText(e)}`);
      }
    },

    async upsertProvider(p: ProviderConfig) {
      try {
        const saved = await api.providerUpsert(p);
        if (!this.settings) return saved;
        const list = this.settings.providers;
        const i = list.findIndex((x) => x.id === saved.id);
        if (i >= 0) list[i] = saved;
        else list.push(saved);
        return saved;
      } catch (e) {
        message.error(`保存供应商失败：${errorText(e)}`);
        throw e;
      }
    },

    async deleteProvider(id: string) {
      await api.providerDelete(id);
      if (this.settings) {
        this.settings.providers = this.settings.providers.filter((p) => p.id !== id);
        if (this.settings.activeLlmProviderId === id) this.settings.activeLlmProviderId = null;
        if (this.settings.activeImageProviderId === id)
          this.settings.activeImageProviderId = null;
        if (this.settings.activeVideoProviderId === id)
          this.settings.activeVideoProviderId = null;
      }
    },

    async setSecret(keyRef: string, value: string) {
      await api.providerSetSecret(keyRef, value);
      this.secrets = await api.secretsStatus();
    },

    async test(id: string) {
      try {
        const r = await api.providerTest(id);
        message.success(r);
        return true;
      } catch (e) {
        message.error(`测试失败：${errorText(e)}`);
        return false;
      }
    },
  },
});
