/**
 * Agent 会话状态。
 *
 * 流式期间前端只维护一份「实时缓冲区」（streamText / streamTools），
 * run 结束后再从后端拉一次会话（后端是唯一事实来源），避免两边状态漂移。
 */
import { defineStore } from "pinia";
import { Channel, invoke } from "@tauri-apps/api/core";
import { api, errorText } from "@/api/ipc";
import { message } from "@/ui";
import type { AgentEvent, AgentSession, AgentToolCall, PrefixReport, UsageReport } from "@/types/agent";
import type { PanelId } from "@/types/models";

interface State {
  sessions: AgentSession[];
  /** Simon 是共享上下文的助手：全局只有一个当前会话，不按面板分 */
  activeSessionId: string | null;
  running: boolean;
  streamText: string;
  streamReasoning: string;
  streamTools: AgentToolCall[];
  prefix: PrefixReport | null;
  usage: UsageReport | null;
  pending: { id: string; title: string } | null;
  /** agent 正在等用户回答的问题 */
  pendingAsk: { id: string; question: string; options: string[]; why: string } | null;
  lastError: string | null;
  /** 上一次请求的前缀指纹，用来提示「前缀已变，缓存会失效」 */
  lastSentContext: Partial<Record<PanelId, string>>;
}

export const useAgentStore = defineStore("agent", {
  state: (): State => ({
    sessions: [],
    activeSessionId: null,
    running: false,
    streamText: "",
    streamReasoning: "",
    streamTools: [],
    prefix: null,
    usage: null,
    pending: null,
    pendingAsk: null,
    lastError: null,
    lastSentContext: {},
  }),

  getters: {
    current: (s): AgentSession | null =>
      s.sessions.find((x) => x.id === s.activeSessionId) ?? null,
    cacheHint(s): string {
      if (!s.usage) return "尚无用量数据";
      const r = (s.usage.hitRate * 100).toFixed(1);
      return `缓存命中 ${r}%｜本轮输入 ${s.usage.inputTokens}，命中 ${s.usage.cacheReadTokens}`;
    },
  },

  actions: {
    async loadSessions() {
      try {
        this.sessions = await api.agentSessions();
        if (!this.activeSessionId && this.sessions.length) {
          this.activeSessionId = this.sessions[this.sessions.length - 1].id;
        }
      } catch {
        this.sessions = [];
      }
    },

    async ensureSession(panel: PanelId): Promise<string> {
      if (this.activeSessionId && this.sessions.some((s) => s.id === this.activeSessionId)) {
        return this.activeSessionId;
      }
      const s = await api.agentSessionNew(panel);
      this.sessions.push(s);
      this.activeSessionId = s.id;
      return s.id;
    },

    async newSession(panel: PanelId) {
      const s = await api.agentSessionNew(panel);
      this.sessions.push(s);
      this.activeSessionId = s.id;
      this.streamText = "";
      this.streamReasoning = "";
      this.streamTools = [];
      this.prefix = null;
      this.usage = null;
      return s;
    },

    selectSession(_panel: PanelId, id: string) {
      this.activeSessionId = id;
      this.streamText = "";
      this.streamReasoning = "";
      this.streamTools = [];
      this.usage = null;
      this.prefix = null;
    },

    async deleteSession(id: string) {
      await api.agentSessionDelete(id);
      this.sessions = this.sessions.filter((s) => s.id !== id);
      if (this.activeSessionId === id) {
        this.activeSessionId = this.sessions[this.sessions.length - 1]?.id ?? null;
      }
    },

    async refreshSession(id: string) {
      const s = await api.agentSessionGet(id);
      if (!s) return;
      const i = this.sessions.findIndex((x) => x.id === id);
      if (i >= 0) this.sessions[i] = s;
      else this.sessions.push(s);
    },

    async previewPrefix(panel: PanelId) {
      try {
        this.prefix = await api.agentPrefixPreview(panel, this.activeSessionId ?? undefined);
      } catch {
        /* 没有项目时忽略 */
      }
    },

    async send(panel: PanelId, input: string, imagePaths: string[] = [], refreshContext = false) {
      if (this.running) {
        message.warning("上一轮还在进行中");
        return;
      }
      const text = input.trim();
      if (!text && imagePaths.length === 0) return;

      const sessionId = await this.ensureSession(panel);
      this.running = true;
      this.streamText = "";
      this.streamReasoning = "";
      this.streamTools = [];
      this.lastError = null;
      this.pending = null;
      this.pendingAsk = null;

      const channel = new Channel<AgentEvent>();
      channel.onmessage = (ev: AgentEvent) => this.handleEvent(ev, sessionId);

      try {
        await invoke<string>("agent_run", {
          channel,
          panel,
          sessionId,
          input: text,
          imagePaths,
          refreshContext,
        });
      } catch (e) {
        this.running = false;
        this.lastError = errorText(e);
        message.error(`Agent 运行失败：${errorText(e)}`);
      } finally {
        this.running = false;
        this.pending = null;
        this.pendingAsk = null;
        await this.refreshSession(sessionId);
        await this.loadSessions();
      }
    },

    handleEvent(ev: AgentEvent, sessionId: string) {
      switch (ev.type) {
        case "runStarted":
          this.running = true;
          break;
        case "prefix":
          this.prefix = ev.report;
          break;
        case "textDelta":
          this.streamText += ev.text;
          break;
        case "reasoningDelta":
          this.streamReasoning += ev.text;
          break;
        case "toolCall":
          this.streamTools.push({
            id: ev.id,
            name: ev.name,
            title: ev.title,
            input: ev.input,
            costly: ev.costly,
            mutates: ev.mutates,
            state: ev.needsConfirm ? "awaiting-approval" : "running",
          });
          if (ev.needsConfirm) {
            this.pending = { id: ev.id, title: ev.title };
          }
          break;
        case "toolResult": {
          const t = this.streamTools.find((x) => x.id === ev.id);
          if (t) {
            t.state = ev.ok ? "ok" : "failed";
            t.summary = ev.summary;
            t.data = ev.data;
            t.durationMs = ev.durationMs;
          }
          if (this.pending?.id === ev.id) this.pending = null;
          break;
        }
        case "askUser":
          this.pendingAsk = {
            id: ev.id,
            question: ev.question,
            options: ev.options ?? [],
            why: ev.why ?? "",
          };
          break;
        case "usage":
          this.usage = ev.report;
          break;
        case "error":
          this.lastError = ev.message;
          message.error(ev.message);
          break;
        case "runFinished":
          this.running = false;
          if (ev.error) this.lastError = ev.error;
          break;
        default:
          break;
      }
      void sessionId;
    },

    async answer(toolCallId: string, text: string) {
      await api.agentAnswer(toolCallId, text);
      this.pendingAsk = null;
    },

    async approve(toolCallId: string, approved: boolean) {
      await api.agentApprove(toolCallId, approved);
      this.pending = null;
      const t = this.streamTools.find((x) => x.id === toolCallId);
      if (t) t.state = approved ? "running" : "rejected";
    },
  },
});
