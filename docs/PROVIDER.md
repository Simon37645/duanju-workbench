# 接模型接口

工作台不绑定任何一家模型服务。文本、生图、生视频三类都在「设置 → 模型供应商」里配置。

## 文本模型（agent 用）

| 适配器 | 适用 | 说明 |
| --- | --- | --- |
| `openai` | DeepSeek / Qwen / GLM / Kimi / OpenRouter / vLLM / Ollama / OpenAI 官方 | 走 `POST {baseUrl}/chat/completions`，SSE 流式 + function calling |
| `anthropic` | Claude 官方与兼容网关 | 走 `POST {baseUrl}/v1/messages`，支持显式 `cache_control` 缓存断点 |
| `mock` | 无 | 不联网，返回占位输出，用来验证整条链路 |

必填：**Base URL**、**模型名**、**API Key**。

- OpenAI 兼容：Base URL 填到 `/v1` 这一层即可（例如 `https://api.deepseek.com/v1`），
  程序会自动接 `/chat/completions`；直接填到 `/chat/completions` 也能识别。
- Anthropic：填 `https://api.anthropic.com` 即可，会自动接 `/v1/messages`。

可选 `options`：

```jsonc
{
  "maxTokens": 8000,        // 单次回复上限，默认 8000
  "temperature": 0.7,       // 默认 0.7
  "streamOptions": true,    // 是否发送 stream_options.include_usage（拿缓存命中数用）
                            // 有些兼容端点不认，收到 400 会自动降级重试一次
  "headers": { "X-Foo": "bar" },
  "anthropicVersion": "2023-06-01"
}
```

配完点「测试」，它会发一条最小请求验证连通性。

## 生图 / 生视频

这两类走**配置驱动的通用 HTTP 适配器**（`generic-http`）：请求怎么发、任务怎么轮询、
结果 JSON 在哪取，全写在 `options` 里。等拿到接口文档，填一份配置就能接上，不用改代码。

### options 结构

```jsonc
{
  "submit": {
    "method": "POST",
    "path": "/v1/images/generations",        // 相对 baseUrl；写完整 http(s) URL 也行
    "headers": { "X-Custom": "1" },
    "body": {                                 // 模板：{{变量}} 会被替换
      "model": "{{model}}",
      "prompt": "{{prompt}}",
      "negative_prompt": "{{negative}}",
      "width": "{{width}}",
      "height": "{{height}}",
      "seed": "{{seed}}",
      "image": "{{image1}}",                 // 参考图（data-url 形式）
      "duration": "{{duration}}"
    }
  },

  // 有 taskIdPath = 异步任务，会轮询直到完成；没有 = 同步返回，直接取 result
  "taskIdPath": "/data/0/task_id",
  "poll": {
    "method": "GET",
    "path": "/v1/tasks/{{taskId}}",
    "statusPath": "/data/status",
    "successValues": ["succeeded", "success"],
    "failureValues": ["failed", "error"],
    "progressPath": "/data/progress",        // 0~100 或 0~1 都认
    "intervalSec": 5,
    "timeoutSec": 900
  },

  "result": {
    "urlPath": ["/data/0/url", "/output/url"],  // 候选路径，取第一个非空
    "b64Path": ["/data/0/b64_json"],            // base64 直接返回的情况
    "filePath": ["/data/0/path"],               // 服务端返回本地路径的情况
    "headers": {}                               // 下载结果文件时带的头
  },

  "maxAttempts": 3,
  "downloadTimeoutSec": 600
}
```

### 可用变量

| 变量 | 含义 |
| --- | --- |
| `{{prompt}}` | 提示词（已拼好风格词 + 资产描述 + 固定特征） |
| `{{negative}}` | 负面词（风格负面词 + 视图负面词） |
| `{{model}}` | provider 配置里的模型名 |
| `{{width}}` `{{height}}` | 按项目画幅换算出的尺寸 |
| `{{seed}}` | 随机种子，留空则该字段整个不发 |
| `{{duration}}` | 时长（秒），生视频用 |
| `{{image1}}` `{{image2}}` … | 参考图，data-url 形式（第 1 张是首帧） |
| `{{image}}` | 等同于 `{{image1}}` |
| `{{image1raw}}` | 同上但只有纯 base64，不带 data-url 前缀 |
| `{{apiKey}}` | API Key（也可以靠自动带的 `Authorization: Bearer`） |

两个实用细节：

- **整个值就是一个变量时保留原始类型** —— `"width": "{{width}}"` 发出去是数字 `1024`，不是字符串。
- **变量缺失时该字段会被整个删掉** —— 没填 seed 就不会发 `"seed": null`，省得有些接口报错。

### 两张现成模板

设置里点「填入图片模板」/「填入视频模板」会填好下面这两份，按接口文档改路径和字段名即可。

**图片（同步返回 URL）**

```json
{
  "submit": {
    "method": "POST",
    "path": "/v1/images/generations",
    "body": {
      "model": "{{model}}",
      "prompt": "{{prompt}}",
      "negative_prompt": "{{negative}}",
      "width": "{{width}}",
      "height": "{{height}}",
      "seed": "{{seed}}",
      "image": "{{image1}}"
    }
  },
  "result": { "urlPath": ["/data/0/url"], "b64Path": ["/data/0/b64_json"] }
}
```

**视频（提交后轮询）**

```json
{
  "submit": {
    "method": "POST",
    "path": "/v1/videos/generations",
    "body": {
      "model": "{{model}}",
      "prompt": "{{prompt}}",
      "duration": "{{duration}}",
      "first_frame": "{{image1}}",
      "last_frame": "{{image2}}"
    }
  },
  "taskIdPath": "/task_id",
  "poll": {
    "method": "GET",
    "path": "/v1/videos/{{taskId}}",
    "statusPath": "/status",
    "successValues": ["succeeded"],
    "failureValues": ["failed"],
    "progressPath": "/progress",
    "intervalSec": 8,
    "timeoutSec": 1800
  },
  "result": { "urlPath": ["/video_url"] }
}
```

### 路径写法

`statusPath` / `urlPath` 等用的是 **JSON Pointer**：

- `/data/0/url` → `{"data":[{"url":"..."}]}` 里第一个元素的 url
- `/output/url` → 顶层对象里的 `output.url`
- 传数组表示「依次尝试，取第一个非空的」

## 占位适配器（mock）

选 `mock` 时用 ffmpeg 现场造素材：

- 图片 = 深底色的提示词卡片
- 视频 = 4 秒（可调）的纯色片 + 提示词 + 一条正弦音轨

用来在没有接口的情况下把「提示词 → 任务队列 → 落盘 → 时间线 → 导出 → 转写」整条链路跑通。

## API Key 存哪儿

存在配置目录的 `secrets.json`，和项目目录分开，不会随项目拷贝外传。
文件权限依赖系统用户目录权限，属于「明文但私有」。后续可以换成系统钥匙串，
接口已经收在 `src-tauri/src/config.rs` 的 `get_secret` / `set_secret` 里。

---

# 两份实战配置范例

下面两份配置是「把接口文档翻译成 `options`」的范例，对应两类很常见的服务形态：

1. **OpenAI 兼容的生图网关**（New API / one-api 这类自建网关都算）；
2. **ComfyUI 工作流平台**（提交任务 → 轮询 → 取结果，图片要先上传换成 URL）。

> 域名、模型名、工作流 id 一律是**占位值**，照着自己的接口文档改。
> 设置面板里的「快速预设」填的就是这两份骨架。

## 范例一：OpenAI 兼容生图网关

自建网关（[New API](https://github.com/Calcium-Ion/new-api)、one-api 等）对外就是标准
OpenAI 图片协议，所以配置很短。

| 项 | 值 |
| --- | --- |
| Base URL | `https://api.example.com` |
| 提交 | `POST /v1/images/generations` |
| 认证 | `Authorization: Bearer <token>`（即 `authStyle: bearer`，默认） |
| 返回 | `{ created, data: [{ url, b64_json }], usage }` |

**注意尺寸**：gpt-image 系列只认 `1024x1024` / `1536x1024`(横) / `1024x1536`(竖) / `auto`。
短剧竖屏就固定用 `1024x1536`（在 `options.size` 里写死，覆盖按画幅算出来的值）。

```jsonc
{
  "size": "1024x1536",
  "submit": {
    "method": "POST",
    "path": "/v1/images/generations",
    "body": {
      "model": "{{model}}",
      "prompt": "{{prompt}}",
      "size": "{{size}}",
      "n": 1,
      "background": "opaque"       // 人物三视图可改 "transparent"
    }
  },
  "result": {
    "urlPath": ["/data/0/url"],
    "b64Path": ["/data/0/b64_json"]
  }
}
```

provider 里填：适配器 `generic-http`、模型名 `gpt-image-1`、Base URL `https://api.example.com`。

> 结果拿 `n=1`，一次一张。批量出图由工作台的任务队列并发控制（`concurrency`）。

## 范例二：ComfyUI 工作流平台

### 基础协议

| 项 | 值 |
| --- | --- |
| Base URL | `https://comfy.example.com` |
| 提交 | `POST /api/v1/workflow/{workflow_id}` |
| 查询 | `GET /api/v1/workflow/result/{task_id}` |
| 认证 | **裸 Token**：`Authorization: <token>`（**不带** `Bearer `）→ `authStyle: "raw"` |
| 状态 | `QUEUED` → `RUNNING` → `SUCCESS`（失败为 `FAILED`），无百分比进度 |
| 结果 | `data.results[].url`（`type: video`、`file_type: mp4`），**URL 有效期很短，要立刻下载** |

自查一下：平台里每个工作流的详情页一般会给出该工作流的确切入参；
有些还提供元数据接口（`GET /api/v1/workflow/{uuid}`），其中的 `input_rules`
是机器可读版本（含类型、必填、范围、枚举值）。

### 关键约束：图片必须是 URL，不能是 base64

`first_frame` / `ref_image_0` 这些字段收的是「图片 URL」。这类平台通常自带一套上传接口，
工作台已经把它实现成通用的 `upload` 配置：

```
POST /api/v1/file/before_upload   {md5, file_size}
                                  → data.data.{token, host, quickly_upload}
POST {host}/api/v1/file           {md5, total_size, chunks} + FileToken 头 → next_seq
PUT  {host}/api/v1/file           分片 multipart：FileToken/md5/chunk/chunks/chunk_size/file
```

`quickly_upload: true` 表示服务端已有同 md5 的文件（秒传）。
传完之后用文件 md5 作为引用值 —— 这一条就是 `refTemplate: "{{md5}}"`。

### 工作流入参长什么样

工作流 id 由平台自己定义。举几个典型形态，看规律即可：

| workflow_id（示例） | 用途 | 必填入参 |
| --- | --- | --- |
| `my_first_last_frame_workflow` | **首尾帧生视频** | `first_frame` `last_frame` `prompt`，`duration` 1~15，`resolution` |
| `my_multi_ref_workflow` | **多图参考生视频**（最多 9 张） | `ref_image_0` `prompt`，`duration` 1~10 |
| `my_text_to_video_workflow` | 文生视频 | `prompt`，`duration` 1~15 |
| `my_image_audio_to_video_workflow` | 图 + 音频 → 视频（自动对口型） | `ref_image_0` `ref_audio_0` |
| `my_tts_workflow` | 语音合成（配音用） | `prompt_text`，`prompt_simple`(参考音) |

时长类工作流通常按秒计费，别乱拉长。

### 分辨率枚举

短剧竖屏常用 **768×1344**。有些平台不收 `768x1344` 这样的字面尺寸，只收自己的枚举值，
这时用 `aspectValues` 把「按画幅算出来的尺寸」映射过去：

```jsonc
"aspectValues": { "768x1344": "768p_portrait", "1344x768": "768p_landscape" }
```

### 完整配置（首尾帧）

```jsonc
{
  "authStyle": "raw",
  "aspectValues": { "768x1344": "768p_portrait", "1344x768": "768p_landscape" },
  "upload": {
    "mode": "chunked",
    "beforePath": "/api/v1/file/before_upload",
    "initPath": "/api/v1/file",
    "chunkPath": "/api/v1/file",
    "field": "file",
    "refTemplate": "{{md5}}"
  },
  "submit": {
    "method": "POST",
    "path": "/api/v1/workflow/my_first_last_frame_workflow",
    "body": {
      "prompt": "{{prompt}}",
      "first_frame": "{{image1}}",
      "last_frame": "{{image2}}",
      "duration": "{{durationInt}}",
      "resolution": "{{aspect}}"
    }
  },
  "taskIdPath": "/data/task_id",
  "poll": {
    "method": "GET",
    "path": "/api/v1/workflow/result/{{taskId}}",
    "statusPath": "/data/status",
    "successValues": ["SUCCESS", "completed"],
    "failureValues": ["FAILED"],
    "intervalSec": 6,
    "timeoutSec": 1800
  },
  "result": { "urlPath": ["/data/results/0/url"] }
}
```

`{{image1}}` 是首帧、`{{image2}}` 是尾帧 —— 工作台按「首帧 → 尾帧 → 其它参考图」的顺序
把资产图放进 `ref_images`。多图参考工作流里 `ref_image_0..8` 依次对应 `{{image1}}..{{image9}}`，
没给的会自动从请求体里去掉（而不是发一个空字符串）。

## 接新接口时的排查顺序

真接一个没见过的接口，按这个顺序试最省事：

1. **认证风格** —— 先确认是 `Bearer <token>` 还是裸 `<token>`。文档常写成
   `Authorization: 您的Token`，那就先试 `authStyle: "raw"`；报 401 就换回 `bearer`。
2. **提交是否异步** —— 提交后返回体里有没有 task id。有就配 `taskIdPath` + `poll`，
   没有就直接配 `result`。
3. **结果路径** —— 拿一次真实返回体，把 `urlPath` 指到那个字段上。
4. **图片怎么给** —— 收 data-url / base64 就直接用；收「图片 URL」就加 `upload` 段，
   并用 `refTemplate` 说明传完之后 `{{imageN}}` 该换成什么值
   （支持 `{{md5}}` / `{{url}}` / `{{path}}`）。
5. **尺寸** —— 接口不认 `WxH` 字面值就加 `aspectValues` 映射。

以上全部只改配置，不用改代码。
