# 短剧工作台

Tauri 2 + Vue 3 的短剧全流程制作工作台。九个面板串起从剧本到成片的完整链路，
每个面板都有 agent 深度参与：它能直接读写项目数据、调用生图生视频接口、铺时间线。

```
剧本 → 风格 → 分镜 → 资产 → 视频提示词 → 生视频 → 剪辑 → 字幕 → Checklist
```

## 快速开始

```bash
npm install          # 依赖（见下方「为什么用 npm」）
npm run app:dev      # 开发调试：起 vite + 编译 Rust + 打开窗口
npm run app:build    # 出安装包（NSIS）+ release 可执行文件
npm run pack         # 收拢成绿色版目录 app/，并在桌面建快捷方式
```

> `npm run pack` 会把 `target/release` 里的主程序与 sidecar 拷到 `app/`，路径稳定
> （`cargo clean` 不会影响它），整个 `app/` 目录可以直接拷到别的 Windows 机器上用
> ——前提是那台机器有 WebView2 运行时，Win11 自带，Win10 需要装一次。

> 命令行自检只在 **debug 构建**下看得到输出：release 版是 GUI 子系统程序，没有控制台。

首次运行会创建配置目录：

| 平台 | 位置 |
| --- | --- |
| Windows | `%APPDATA%\com.duanju.workbench\` |
| macOS | `~/Library/Application Support/com.duanju.workbench/` |

里面是 `settings.json`（偏好与供应商列表）、`secrets.json`（API Key）、`models/`（whisper 模型）。

不想开界面也能自检（出问题时先跑这个）：

```bash
cd src-tauri
cargo run -- --net-check                         # 代理设置 + 模型仓库连通性
cargo run -- --pipeline-check                    # 整条生产链跑一遍（用占位模型/占位素材）
cargo run -- --asr-download ggml-large-v3-turbo  # 预下载 whisper 模型
```

## 网络与代理

v2rayN / Clash 这类工具通常只改**系统代理**，不写环境变量；而 HTTP 库默认只认环境变量，
于是会出现「浏览器能上、应用连不上」。工作台的做法：

- 默认「**跟随系统**」：先看环境变量，再读 Windows 注册表里的
  `Internet Settings\ProxyServer`（macOS 走 `scutil --proxy`），自动用上；
- 也可以切成「直连」或「手动指定」（支持 `http://` 与 `socks5://`）；
- **本机地址永远直连**（127.0.0.1 / 局域网段），本地的 vLLM / Ollama / ComfyUI 不受影响；
- 「设置 → 运行环境」里能看到系统代理、环境变量、实际使用值，并有「测试连通性」按钮。

改了代理设置会自动重建 HTTP 客户端，不用重启。

## 开工前三件事

1. **配模型接口** —— 右上角「设置 → 模型供应商」。文本模型支持 OpenAI 兼容端点与
   Anthropic 原生；生图 / 生视频用「通用 HTTP」适配器，接口格式写在配置里，
   详见 [docs/PROVIDER.md](docs/PROVIDER.md)。
2. **不配也能跑** —— 没配文本模型时 agent 走占位适配器；没配生图 / 生视频接口时
   用 ffmpeg 生成占位素材。整条流程（含剪辑导出、字幕）都能完整走一遍。
3. **字幕的显卡加速是编译期的** —— 见下方「whisper 显卡加速」。

## 界面与设计系统

前端没有用组件库，自研了一套轻量设计系统（`src/ui/`，约 20 个组件：
按钮、输入、下拉、开关、标签输入、模态、抽屉、提示、Toast、确认框…）。
这样做的好处是视觉完全可控，代价是要自己维护交互细节。

三条设计原则（改样式时请守住）：

1. **边框用白色低透明度**（`rgba(255,255,255,.07)`）而不是灰色实线。
   灰线在深色底上会形成一圈"描边"，看起来又脏又挤。
2. **用表面层级表达结构**，别到处画边框。`--surface-1..5` 的明度差本身就是分隔。
3. **留白要够**。面板内边距 16px 起，头高 44~48px，宁可少放点内容。

配色、间距、圆角、字号全部收在 `src/ui/tokens.css`，改主题只动这一个文件。

### 不装桌面程序也能调界面

```bash
npm run dev     # 浏览器打开 http://localhost:1420
```

在浏览器里没有 Tauri 运行时，`src/api/ipc.ts` 会自动把命令路由到
`src/dev/mock.ts` —— 一份像样的示例项目（4 章剧本、6 个镜头、4 个资产、
带工具调用的 agent 对话记录）。改界面时不用每次编译 Rust，也方便截图对比。
mock 只在浏览器里加载，打包进桌面版不会带上。

## 项目目录长什么样

每个项目就是一个普通目录，没有数据库，拷走就能在另一台机器打开（包括 Apple Silicon）：

```
我的短剧/
  project.json              项目清单
  bible.json                项目圣经（设定 / 人物 / 卖点）
  script/index.json         章节索引
  script/chapters/*.md      章节正文（Markdown，可以直接改）
  style/style.json          画面风格圣经
  storyboard/index.json     镜头表
  assets/index.json         资产清单
  assets/files/<资产>/*.png  人物三视图 / 场景 / 物件
  prompts/video_prompts.json 视频提示词与资产配对
  video/takes.json          生成结果清单
  video/files/*.mp4         生成的视频
  edit/timeline.json        时间线
  edit/renders/*.mp4        导出的成片
  subtitles/                字幕（srt / vtt）
  checklist/checklist.json  检查清单
  .workbench/               运行时数据，可以整个删掉
```

## Agent 与提示词缓存

这是本工作台在成本上最花心思的地方。

服务端前缀缓存（Anthropic 显式断点 / OpenAI 兼容端点的自动前缀缓存）都是**按前缀逐字节匹配**的：
前面任何一个字符变了，从那里往后全部重新计费。所以决定命中率的不是「断点打在哪」，
而是「前缀里有没有会变的东西」。

做法是把提示词切成两层：

- **冻结前缀**（会话创建时构建一次，之后整场对话不变）
  - `L0 核心指令` — 全局工作准则
  - `L1 面板职责` — 当前面板的产出规范
  - `L2 项目圣经` — 设定、人物、风格圣经
  - `L3 资产索引` — 资产清单与视图状态
  - 会话首条消息里的**上下文快照**（章节现状、缺口统计）
- **增量尾部** —— 工具结果与新对话轮次，永远只追加，绝不回头改历史消息

所以「项目数据变了」不会污染缓存：agent 需要新数据时调工具去读，结果落在尾部。
对话坞右上角能看到实时命中率与各层 token 估算；点 ↻ 重建上下文会牺牲一次命中，
换来 agent 看到最新数据。

改代码时请守住两条不变量（`src-tauri/src/agent/prompt.rs` 顶部有详细说明）：

1. `ChatRequest.system` 一旦冻结就不再变；
2. `messages` 只追加，历史消息永不改写。

工具定义的顺序与字段顺序同样影响缓存，所以 `agent/tools.rs` 的注册顺序是固定的，
不要改成 HashMap 遍历。

## whisper 显卡加速

转写走本地 whisper.cpp，有两条路：

1. **进程内推理**（需要带 feature 编译）
   ```bash
   # Windows / Linux + NVIDIA
   cargo build --features asr-cuda
   # Apple Silicon
   cargo build --features asr-metal
   # 纯 CPU
   cargo build --features asr
   ```
   需要 `cmake` 与 `libclang`（LLVM）。后端是**编译期**决定的，换显卡要重新编译。
   运行时「是否使用显卡」这个开关只在已编译进去的后端范围内生效。
2. **外部 whisper-cli** —— 把 whisper.cpp 官方的 `whisper-cli` 放到应用目录，
   或在设置里指定路径。默认构建走这条路，不需要 libclang。

两条路都先用 ffmpeg 把素材转成 16kHz 单声道 wav，保证输入一致。
字幕面板会显示本机检测结果（显卡型号、CUDA 版本、推荐后端、已编译后端）。

**模型下载**：默认从 HuggingFace 拉 ggml 模型。国内网络访问不了时，
在「设置 → 字幕 → 下载源」切到 `hf-mirror` 镜像，或者自己下载 `ggml-*.bin`
丢进「模型目录」（设置里能直接打开该目录）。命令行预下载见上面的 `--asr-download`。

## ffmpeg

发行版随包携带 `ffmpeg` / `ffprobe`（`src-tauri/binaries/`）。
开发期如果找不到 sidecar，会临时回退到系统 PATH，并在日志里提醒。
也可以在「设置 → 运行环境」里手动指定路径。

放置 sidecar 的命名规则：

```
src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe
src-tauri/binaries/ffprobe-x86_64-pc-windows-msvc.exe
```

Windows 上直接从系统 ffmpeg 拷一份就行（名字里的三元组要和 `rustc -vV` 的 host 一致）：

```bash
cd src-tauri && mkdir -p binaries
cp "$(where.exe ffmpeg | head -1 | tr -d '\r')"  binaries/ffmpeg-x86_64-pc-windows-msvc.exe
cp "$(where.exe ffprobe | head -1 | tr -d '\r')" binaries/ffprobe-x86_64-pc-windows-msvc.exe
```

> 这两个文件加起来约 440 MB，已在 `.gitignore` 里排除，别提交进仓库。

## 为什么用 npm 而不是 pnpm

本机上 pnpm 在 Windows 上安装 esbuild 时稳定报 `EPERM: rename`（文件被占用），
换 `--package-import-method=copy` 也一样，npm 一次通过。所以 `tauri.conf.json` 里的
`beforeDevCommand` / `beforeBuildCommand` 用的是 `npm run`。
如果你那边 pnpm 正常，改回去也可以。

## 目录结构

```
src/                      前端
  api/                    Tauri 命令封装 + 事件订阅
  stores/                 Pinia：项目 / agent / 任务 / 设置
  components/             对话坞、面板导航、顶栏、任务条、设置抽屉
  views/panels/           九个面板
  types/                  与 Rust 模型一一对应的 TS 类型
src-tauri/src/            后端
  models.rs               全部数据结构
  project.rs / store.rs   项目目录布局与原子写
  llm/                    供应商适配器（openai / anthropic / mock）
  agent/                  会话、工具循环、缓存友好的提示词装配
  gen/                    生图生视频（配置驱动的通用 HTTP 适配器）
  actions.rs              高层动作，前端命令与 agent 工具共用
  jobs.rs                 后台任务队列
  media.rs                ffmpeg 调用与时间线导出
  asr.rs                  whisper 能力探测、模型下载、转写
  progress.rs             面板完成度判定（唯一事实来源）
docs/                     接口配置与架构说明
```

## 已知边界

- 剪辑面板的导出是「主视频轨按起点排序依次拼接（**每个片段自己的音轨一起拼进来**，
  无音轨的用等长静音补齐）+ 音频轨片段混在其上 + 可选烧字幕」，
  转场、变速曲线、多轨叠加合成还没做。
- whisper 的进程内推理需要 libclang 才能编译，本机没装，**这条路径尚未实测**；
  外部 CLI 路径与能力探测是通的。
- agent 的花钱工具（生图 / 生视频）默认要人工确认，可在设置里关掉。

## 已经实测过的部分

`cargo run -- --pipeline-check` 目前全绿，覆盖：建项目 → 圣经/风格 → 章节 →
分镜 → 资产 → **真实调用 ffmpeg 出图**（占位适配器）→ 提示词拼装 → **出视频** →
铺时间线 → **导出成片（1080×1920，带音轨）** → 字幕能力探测 → 面板完成度统计。

`cargo run -- --net-check` 在开着 v2rayN 的机器上验证了系统代理能被正确识别与使用。
