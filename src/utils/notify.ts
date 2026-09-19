/** 极薄的提示封装，避免每个组件都 import naive-ui 的 useMessage。
 *  naive-ui 的 message 需要在 provider 内使用，这里用全局 API 实例。 */
import { createDiscreteApi } from "naive-ui";

const { message } = createDiscreteApi(["message"], {
  configProviderProps: {},
});

export { message };
