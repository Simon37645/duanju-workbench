/** Agent 事件流协议。与 src-tauri/src/agent/mod.rs 的 AgentEvent 枚举一一对应。 */

import type { PanelId } from "./models";

export interface LayerReport {
  name: string;
  chars: number;
  estTokens: number;
  /** 该层内容指纹，用于判断前缀是否变动 */
  hash: string;
  /** 该层是否挂了 Anthropic 显式缓存断点 */
  breakpoint: boolean;
}

export interface PrefixReport {
  /** 整个稳定前缀的指纹；不变 = 缓存可复用 */
  fingerprint: string;
  /** 相对上一次请求，稳定前缀是否发生变化 */
  stable: boolean;
  layers: LayerReport[];
  estTokens: number;
  note: string;
}

export interface UsageReport {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  /** 本次会话累计 */
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCacheReadTokens: number;
  totalCacheWriteTokens: number;
  /** 缓存命中率 = cacheRead / (input + cacheRead) */
  hitRate: number;
}

export type StopReason =
  | "endTurn"
  | "toolUse"
  | "maxTokens"
  | "stopSequence"
  | "refusal"
  | "error";

export type AgentEvent =
  | { type: "runStarted"; runId: string; sessionId: string; providerId: string; model: string }
  | { type: "prefix"; report: PrefixReport }
  | { type: "reasoningDelta"; text: string }
  | { type: "textDelta"; text: string }
  | {
      type: "toolCall";
      id: string;
      name: string;
      title: string;
      input: unknown;
      costly: boolean;
      /** 会不会改项目数据 */
      mutates: boolean;
      needsConfirm: boolean;
    }
  | {
      type: "askUser";
      id: string;
      question: string;
      options: string[];
      why: string;
    }
  | {
      type: "toolResult";
      id: string;
      name: string;
      ok: boolean;
      summary: string;
      data: unknown;
      durationMs: number;
    }
  | { type: "usage"; report: UsageReport }
  | { type: "round"; round: number }
  | { type: "context"; stats: ContextStats }
  | { type: "contextCompacted"; before: number; after: number }
  | {
      type: "runFinished";
      runId: string;
      stopReason: StopReason;
      text: string;
      error: string | null;
    }
  | { type: "error"; message: string };

export interface AgentMessage {
  id: string;
  role: "user" | "assistant" | "system";
  /** 正文（可能含 markdown） */
  text: string;
  /** 推理模型的过程输出 */
  reasoning?: string;
  toolCalls?: AgentToolCall[];
  /** 该条消息关联的图片（用户上传或 agent 读取的资产） */
  images?: string[];
  createdAt: string;
  streaming?: boolean;
}

export interface AgentToolCall {
  id: string;
  name: string;
  title: string;
  input: unknown;
  costly: boolean;
  mutates?: boolean;
  state: "pending" | "awaiting-approval" | "running" | "ok" | "failed" | "rejected";
  summary?: string;
  data?: unknown;
  durationMs?: number;
}

/** 与 Rust 侧 AgentSession 一一对应 */
export interface AgentSession {
  id: string;
  projectId: string;
  panel: PanelId;
  title: string;
  /** 给模型看的原始消息（含 content blocks），界面不要直接渲染 */
  messages: unknown[];
  /** 给界面渲染的消息，包含工具卡片 */
  display: AgentMessage[];
  prefix: unknown | null;
  lastProviderId: string | null;
  lastModel: string | null;
  turns: number;
  totalUsage: unknown;
  totalCacheRead: number;
  totalInput: number;
  createdAt: string;
  updatedAt: string;
}

/** pi sidecar 引擎的状态（后端 src-tauri/src/pi.rs） */
export interface PiStatus {
  available: boolean;
  running: boolean;
  detail: string;
  path: string | null;
  version: string | null;
}

export interface AgentRunOptions {
  /** 用户输入的文本 */
  input: string;
  /** 附带图片的绝对路径 */
  imagePaths?: string[];
  /** 强制刷新稳定前缀快照（会牺牲一次缓存命中） */
  refreshContext?: boolean;
}

/** 对话上下文水位（后端 src-tauri/src/agent/context.rs） */
export interface ContextStats {
  messagesTokens: number;
  /** 其中工具结果占的部分 —— 压缩的第一刀砍这里 */
  toolResultTokens: number;
  /** 冻结前缀占用，单独算：它基本不变且可缓存 */
  prefixTokens: number;
  budget: number;
  percent: number;
  messages: number;
  turns: number;
  overThreshold: boolean;
}

/** @ 引用的候选项 */
export interface MentionItem {
  kind: "skill" | "chapter" | "asset";
  label: string;
  hint: string;
  token: string;
}
