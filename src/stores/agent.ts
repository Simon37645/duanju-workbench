/**
 * Agent 会话状态。
 *
 * 流式期间前端只维护一份「实时缓冲区」（streamText / streamTools），
 * run 结束后再从后端拉一次会话（后端是唯一事实来源），避免两边状态漂移。
 */
import { defineStore } from "pinia";
import { Channel, invoke } from "@tauri-apps/api/core";
import { api, errorText } from "@/api/ipc";
import { message, toast } from "@/ui";
import type {
  AgentEvent, AgentMessage, AgentSession, AgentToolCall, ContextStats, PiStatus, PrefixReport,
  UsageReport,
} from "@/types/agent";
import type { PanelId } from "@/types/models";

/** 对话引擎：自研（native）或 pi sidecar */
export type AgentEngine = "native" | "pi";

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
  context: ContextStats | null;
  pending: { id: string; title: string } | null;
  /** agent 正在等用户回答的问题 */
  pendingAsk: { id: string; question: string; options: string[]; why: string } | null;
  lastError: string | null;
  compacting: boolean;
  /** 上一次请求的前缀指纹，用来提示「前缀已变，缓存会失效」 */
  lastSentContext: Partial<Record<PanelId, string>>;
  /** 当前使用的对话引擎 */
  engine: AgentEngine;
  /** pi 引擎可用性（未检测时为 null） */
  piStatus: PiStatus | null;
  /** pi 会话历史（浮窗渲染用，来自 pi 的 get_messages） */
  piHistory: AgentMessage[];
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
    context: null,
    pending: null,
    pendingAsk: null,
    lastError: null,
    compacting: false,
    lastSentContext: {},
    engine: ((): AgentEngine => {
      try {
        return localStorage.getItem("simon.engine") === "pi" ? "pi" : "native";
      } catch {
        return "native";
      }
    })(),
    piStatus: null,
    piHistory: [],
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
      if (this.engine === "pi") {
        await this.sendPi(input, imagePaths);
        return;
      }
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

    /* ------------------------------------------------------- pi 引擎 */

    async refreshPiStatus() {
      try {
        this.piStatus = await invoke<PiStatus>("agent_pi_status");
      } catch {
        this.piStatus = null;
      }
      return this.piStatus;
    },

    /** 拉 pi 会话历史（切到 pi 引擎时调；进程没起来会顺便把它拉起来） */
    async loadPiHistory() {
      try {
        const rows = await invoke<AgentMessage[]>("agent_pi_history");
        this.piHistory = rows.map((m) => ({ ...m, toolCalls: m.toolCalls ?? [], images: m.images ?? [] }));
      } catch {
        this.piHistory = [];
      }
    },

    async setEngine(e: AgentEngine) {
      if (this.running) {
        message.warning("上一轮还在进行中，先等它结束");
        return;
      }
      this.engine = e;
      try {
        localStorage.setItem("simon.engine", e);
      } catch {
        /* 隐私模式等场景下 localStorage 不可用，忽略 */
      }
      if (e === "pi") {
        await this.refreshPiStatus();
        void this.loadPiHistory();
      }
    },

    /** pi 引擎跑一轮：事件流与自研引擎同构，渲染逻辑完全复用 */
    async sendPi(input: string, imagePaths: string[] = []) {
      if (this.running) {
        message.warning("上一轮还在进行中");
        return;
      }
      const text = input.trim();
      if (!text && imagePaths.length === 0) return;

      this.running = true;
      this.streamText = "";
      this.streamReasoning = "";
      this.streamTools = [];
      this.lastError = null;
      this.pending = null;
      this.pendingAsk = null;
      this.prefix = null; // pi 引擎没有自研的前缀报告

      const channel = new Channel<AgentEvent>();
      channel.onmessage = (ev: AgentEvent) => this.handleEvent(ev, "pi");
      try {
        await invoke<string>("agent_pi_run", { channel, input: text, imagePaths });
      } catch (e) {
        this.lastError = errorText(e);
        message.error(`pi 引擎运行失败：${errorText(e)}`);
      } finally {
        this.running = false;
        this.pending = null;
        this.pendingAsk = null;
        // 用 pi 的历史刷新气泡（含刚才这轮），再清流式缓冲避免重复显示
        await this.loadPiHistory();
        this.streamText = "";
        this.streamReasoning = "";
        this.streamTools = [];
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
        case "context":
          this.context = ev.stats;
          break;
        case "contextCompacted":
          toast.info(
            `上下文已自动压缩：${Math.round(ev.before / 1000)}k → ${Math.round(ev.after / 1000)}k token`,
          );
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

    /** 拉一次上下文水位（切换会话、压缩之后用） */
    async refreshContext(sessionId?: string) {
      const id = sessionId ?? this.activeSessionId;
      if (!id) return;
      try {
        this.context = await api.agentContext(id);
      } catch {
        this.context = null;
      }
    },

    /** 手动压缩。summarize=true 直接走模型摘要（花一次调用）。 */
    async compact(summarize = false) {
      const id = this.activeSessionId;
      if (!id) return;
      this.compacting = true;
      try {
        this.context = await api.agentCompact(id, summarize);
        await this.refreshSession(id);
        toast.ok("上下文已压缩");
      } catch (e) {
        toast.err(errorText(e));
      } finally {
        this.compacting = false;
      }
    },

    async approve(toolCallId: string, approved: boolean) {
      await api.agentApprove(toolCallId, approved);
      this.pending = null;
      const t = this.streamTools.find((x) => x.id === toolCallId);
      if (t) t.state = approved ? "running" : "rejected";
    },
  },
});
