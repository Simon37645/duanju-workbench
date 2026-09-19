/** 命令式确认框：confirmDialog({...}) 返回 Promise<boolean>。替代 naive-ui 的 useDialog。 */
import { reactive } from "vue";

export interface ConfirmOptions {
  title: string;
  content?: string;
  positiveText?: string;
  negativeText?: string;
  danger?: boolean;
}

export const confirmState = reactive({
  open: false,
  opts: null as ConfirmOptions | null,
  resolve: null as ((v: boolean) => void) | null,
});

export function confirmDialog(opts: ConfirmOptions): Promise<boolean> {
  confirmState.opts = opts;
  confirmState.open = true;
  return new Promise<boolean>((resolve) => {
    confirmState.resolve = resolve;
  });
}

export function resolveConfirm(v: boolean) {
  confirmState.open = false;
  confirmState.resolve?.(v);
  confirmState.resolve = null;
}
