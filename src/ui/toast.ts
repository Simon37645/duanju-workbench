/** 极简 toast：一个全局响应式列表 + 一个宿主组件。替代 naive-ui 的 message。 */
import { reactive } from "vue";

export type ToastTone = "info" | "ok" | "warn" | "err";

export interface ToastItem {
  id: number;
  tone: ToastTone;
  text: string;
}

let seq = 0;
export const toasts = reactive<ToastItem[]>([]);

function push(tone: ToastTone, text: string, ms = 3200) {
  const id = ++seq;
  toasts.push({ id, tone, text });
  window.setTimeout(() => {
    const i = toasts.findIndex((t) => t.id === id);
    if (i >= 0) toasts.splice(i, 1);
  }, ms);
}

export const toast = {
  info: (t: string) => push("info", t),
  ok: (t: string) => push("ok", t),
  warn: (t: string) => push("warn", t),
  err: (t: string) => push("err", t, 5200),
};

/** 兼容旧代码：message.success(...) / message.error(...) */
export const message = {
  success: (t: string) => push("ok", t),
  error: (t: string) => push("err", t, 5200),
  warning: (t: string) => push("warn", t),
  info: (t: string) => push("info", t),
};
