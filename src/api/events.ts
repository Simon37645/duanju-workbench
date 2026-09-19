/** 全局事件订阅与本地媒体路径转换。 */
import { convertFileSrc } from "@tauri-apps/api/core";

export const EVT_PROJECT_CHANGED = "project://changed";
export const EVT_JOB = "job://update";

/** 把本地绝对路径转成 webview 能播放/显示的 URL。 */
export function fileUrl(path?: string | null): string {
  if (!path) return "";
  try {
    return convertFileSrc(path);
  } catch {
    return path;
  }
}

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export async function onProjectChanged(cb: () => void): Promise<UnlistenFn> {
  return listen(EVT_PROJECT_CHANGED, () => cb());
}

export function baseName(path: string): string {
  const p = path.replace(/\\/g, "/");
  return p.slice(p.lastIndexOf("/") + 1);
}

export function fmtDuration(sec: number): string {
  const s = Math.max(0, sec);
  const m = Math.floor(s / 60);
  const r = s - m * 60;
  return `${m}:${r.toFixed(1).padStart(4, "0")}`;
}

export function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export function fmtTime(iso: string): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString("zh-CN", { hour12: false });
}
