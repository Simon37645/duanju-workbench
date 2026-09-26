# 短剧工作台（Duanju Workbench）

[English](README.md) · **中文说明**

Tauri 2 + Vue 3 的短剧全流程制作工作台。九个面板串起从剧本到成片的完整链路，
助手 **Simon** 全程参与：它能跨面板直接读写项目数据、调用生图生视频接口、铺时间线。

```
剧本 → 风格 → 分镜 → 资产 → 3D预演 → 视频提示词 → 生视频 → 剪辑 → 字幕
```

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB.svg)](https://tauri.app/)
[![Vue](https://img.shields.io/badge/Vue-3-42b883.svg)](https://vuejs.org/)

> 本项目不绑定任何一家模型服务；文本、生图、生视频三类后端都可自行配置，
> 详见 [docs/PROVIDER.md](docs/PROVIDER.md)。

## 目录

- [亮点](#亮点)
- [快速开始](#快速开始)
- [开工前三件事](#开工前三件事)
- [项目目录长什么样](#项目目录长什么样)
- [Simon：共享上下文的助手](#simon共享上下文的助手)
- [技能（Skills）](#技能skills)
- [提示词缓存](#提示词缓存)
- [whisper 显卡加速](#whisper-显卡加速)
- [ffmpeg](#ffmpeg)
- [已知边界](#已知边界)
- [第三方组件](#第三方组件)
- [许可](#许可)

## 亮点

- **九个面板一条流水线** —— 剧本、风格、分镜、资产、3D 预演、视频提示词、生视频、
  剪辑、字幕。项目就是一个普通目录，没有数据库。
- **一个真正会操作软件的 agent** —— 47 个工具合并成一份共享工具集，一个回合里能跨面板连着干。
- **不配任何接口也能跑** —— 没配文本模型时走占位适配器，没配生图 / 生视频接口时用
  ffmpeg 生成占位素材。整条流程（含剪辑导出、字幕）都能完整走一遍。
- **把提示词缓存当设计约束** —— 系统提示切成「冻结前缀 + 只追加的尾部」，
  项目数据变了也不会打断服务端前缀缓存。
- **两个 agent 引擎** —— 自研引擎，外加可选的 [pi](https://github.com/earendil-works/pi)
  sidecar，后者复用完全相同的工具、审批流与数据校验。

### 3D 预演（导演台）

预演面板集成 [导演台 DirectorDesk](https://github.com/mangfufu/director-desk)（MIT 开源）：
搭白模场景、摆人物走位、设计多机位与运镜（希区柯克变焦 / 手持晃动等预设）、排灯光，
最后导出参考视频。

- **开箱即用**：它的构建产物随应用打包（`npm run director:prepare` 从上游 clone →
  打补丁 → 构建 → 部署；上游约几个月更新一次，重跑一次即可）；
- **工程随项目**：顶部条「保存工程 / 恢复工程」写进 `<项目>/previz/director.json`，
  换台机器接着改；
- **agent 可操作**：对 Simon 说「读一下预演工程，在舞台上加个人物、设计一条从左到右的运镜」，
  两个引擎（自研 / pi）都会调用导演台的能力读工程、布景、排走位、设计运镜；
  你没开这个面板也没关系，agent 一动手就会自动切过去；
- **agent 能「看见」**：`director_frame` 把当前机位（或布景视图）渲染成一帧图放进它的上下文，
  所以它会自己渲出来核对构图、人物朝向和穿模，而不是凭想象说「已经摆好了」。

## 快速开始

```bash
npm install          # 依赖
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

**深色 / 浅色**两套主题都在 tokens.css 里（`[data-theme="light"]` 覆盖同名变量），
侧栏底部的按钮切换，设置里持久化。组件里不要写死颜色 —— 需要新颜色就先加变量。

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

## Simon：共享上下文的助手

Simon 是**一个**助手，不是九个。它不按面板分割上下文，工具集也是合并的 ——
你在剧本面板让它「把分镜写完然后建资产」，它会依次调用两个面板的工具。
面板只决定开场白与快捷提示，不限制它的能力范围。也正因如此，切换面板不会让
提示词前缀失效，缓存反而更稳。

入口是右下角的悬浮按钮，点开是一个浮窗。副作用是面板能拿到全部宽度。

### 两个对话引擎

浮窗输入框下方有一个引擎切换按钮：

- **自研引擎**（默认）—— 项目内置的 agent：47 个工具、冻结前缀缓存、花钱确认流，
  完全离线可用（没配模型时走占位适配器）。
- **pi 引擎**（可选）—— 对接 [pi](https://github.com/earendil-works/pi)（MIT 的开源
  agent harness）。以 sidecar 子进程方式常驻，走它的 RPC 协议；事件流会翻译成与自研
  引擎相同的事件类型，因此界面渲染完全复用。pi 内置的 shell / 文件工具被显式关闭，
  它能用的 47 个工具全部经本地桥回调到同一套 `actions`，**副作用、审批、数据校验与
  自研引擎完全一致**；花钱操作会走 pi 的确认协议弹出审批卡片。
  会话存在项目 `.workbench/pi/` 下，换项目自动切换。

需要文本模型 provider（设置 → 模型供应商，OpenAI 兼容端点即可），密钥只经环境变量
传给子进程，不落盘。

### 随包分发（安装即用）

安装包自带全部运行时，用户不需要自己装任何东西：

| 组件 | 用途 | 形态 |
| --- | --- | --- |
| node + pi 运行时 | pi 引擎 sidecar | `sidecar/pi/`（约 460MB） |
| ffmpeg / ffprobe | 转码、剪辑导出 | `binaries/`（约 200MB） |
| whisper-cli | 字幕转写（CPU 推理） | `binaries/` |
| whisper small 模型 | 转写默认模型 | `models/`（约 470MB，首启自动补种到用户目录） |

准备脚本：`node scripts/prepare-sidecar.mjs`（幂等，按需补齐/更新各组件）。

**两种分发形态**：

- **安装包**：`npm run app:build` → NSIS / MSI 安装器（约 580 / 660 MB）。
- **绿色版（portable）**：`npm run pack` → 产出 `app/` 目录（约 1.1 GB），
  整个目录拷到任意机器直接运行，无需安装。目录里已含全部运行时（exe 旁边的
  `binaries/`、`models/`、`sidecar/`）。唯一系统依赖是 WebView2 运行时——
  Win11 自带，Win10 一般也随系统更新预装。

### 三种权限模式

| 模式 | 行为 |
| --- | --- |
| **YOLO** | 任何操作直接执行，完全不打断 |
| **自动编辑**（默认） | 改数据自动执行，只有生图 / 生视频这类花钱的操作才弹确认 |
| **变更前确认** | 任何会改动项目数据的操作，执行前都先问一次 |

判断依据是工具是否在只读白名单里（`src-tauri/src/agent/tools.rs` 的 `READ_ONLY_TOOLS`）。

### 上下文管理

对话不能无限长，Simon 这边有三层手段：

**1. 按需喂 —— 别一次全塞进去**

输入框里打 `@` 可以点名引用：技能、章节、资产。只有你点的东西会随这条消息带进去，
其余一律留在外面。

```
帮我按 @skill:剧本节奏检查 过一遍 @chapter:3
```

**2. 主动压缩 —— 水位到了才动手**

浮窗顶部的百分比是上下文水位（鼠标悬停看明细：对话多少、其中工具结果多少、
冻结前缀多少）。超过预算的 75% 自动压缩，也可以手动点。

压缩分两刀，先轻后重：

| 顺序 | 做什么 | 代价 |
| --- | --- | --- |
| 第一刀 | 把早期工具结果的**原文**换成一个短说明 | 免费、零风险 |
| 第二刀 | 让模型把前半段对话压成一段简报 | 一次模型调用 |

工具结果往往是最大的一块 —— 一次 `project_snapshot` 就能上万 token，而它只在被调用的
那一刻有用。所以第一刀经常就够了。

**关键**：压缩只动对话尾部，**冻结前缀不受影响**，那块缓存不会被压掉。

预算和自动压缩开关在「设置 → 运行环境 → Agent 行为」。

**3. 换新对话 —— 最干净的上下文**

浮窗标题栏的下拉里能看到所有对话、随时切换，也可以新建一个从零开始的。
旧对话都留着，切回来接着聊。

### 它会主动问你

拿不准的时候 Simon 会用 `ask_user` 提问，前端渲染成一张卡片：可以给候选项按钮，
也可以让你自由作答。它问完就停下等，不会自己猜着往下做。

## 技能（Skills）

把你自己的流程、规范、方法论装进来，Simon 用得上时自己取。

技能放在 `<配置目录>/skills/`，采用通用的 Agent Skills 目录格式，现成的技能包可以直接拷进来：

```
skills/
  剧本节奏检查.md          ← 单文件技能
  my-methodology/         ← 目录技能
    SKILL.md              ← 必需，带 frontmatter
    references/           ← 可选，附件
      visual-dna.md
```

`SKILL.md` 的 frontmatter：

```markdown
---
name: 剧本节奏检查
description: 用户要检查或优化短剧剧本节奏时使用，含钩子密度与反转间隔的判定标准
---
```

**`description` 最重要** —— 它是模型判断要不要用这个技能的唯一依据。
写「什么时候用」，别写「这是什么」。

### 三级渐进披露

这是整套设计的关键，直接决定成本：

| 层级 | 内容 | 什么时候进上下文 |
| --- | --- | --- |
| 1 | 名称 + 描述 | 永远在（系统提示的 L4 层，可缓存） |
| 2 | SKILL.md 正文 | 模型判断对得上时，用 `skill_read` 取 |
| 3 | 附件 | 真要用到时，用 `skill_read_file` 取 |

实测：装 4 个技能、正文合计 4.6 万字，进系统提示的目录只有 **372 字**。

导入方式：「设置 → 技能」

- **选 `.zip`**（可多选）—— 最省事。一个 zip 里装一个技能、或装一整个合集，都能自动识别
- 选技能目录，或装着多个技能的**合集目录**
- 选单个 `.md`

命令行也能导，不用开界面：

```bash
cd src-tauri
cargo run -- --import-skill D:/下载/剧本skill合集.zip
cargo run -- --skills        # 看装了什么
```

### 手上的 Word 文档

先转成 Markdown 再导入，仓库里带了脚本：

```bash
python scripts/docx2md.py 输入.docx 输出目录/ --kind style --name "我的风格圣经"
python scripts/docx2md.py 某个目录/ 输出目录/ --guess-headings   # 批量，并还原小标题
```

## 提示词缓存

这是本工作台在成本上最花心思的地方。

服务端前缀缓存（Anthropic 显式断点 / OpenAI 兼容端点的自动前缀缓存）都是**按前缀逐字节匹配**的：
前面任何一个字符变了，从那里往后全部重新计费。所以决定命中率的不是「断点打在哪」，
而是「前缀里有没有会变的东西」。

做法是把提示词切成两层：

- **冻结前缀**（会话创建时构建一次，之后整场对话不变）
  - `L0 核心指令` — 全局工作准则
  - `L1 工作台职责` — 九个面板的产出规范（**不随当前面板变化**）
  - `L2 项目圣经` — 设定、人物、风格圣经
  - `L3 资产索引` — 资产清单与视图状态
  - 会话首条消息里的**上下文快照**（当前面板、章节现状、缺口统计）
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
- whisper 的进程内推理需要 libclang 才能编译，**这条路径尚未实测**；
  外部 CLI 路径与能力探测是通的。
- agent 的花钱工具（生图 / 生视频）默认要人工确认，可在设置里关掉。

## 已经实测过的部分

`cargo run -- --pipeline-check` 目前全绿，覆盖：建项目 → 圣经/风格 → 章节 →
分镜 → 资产 → **真实调用 ffmpeg 出图**（占位适配器）→ 提示词拼装 → **出视频** →
铺时间线 → **导出成片（1080×1920，带音轨）** → 字幕能力探测 → 面板完成度统计。

`cargo run -- --net-check` 在开着 v2rayN 的机器上验证了系统代理能被正确识别与使用。

## 第三方组件

以下组件由构建脚本在准备阶段拉取，源码不随本仓库分发。

| 组件 | 用途 | 许可 |
| --- | --- | --- |
| [导演台 DirectorDesk](https://github.com/mangfufu/director-desk) | 3D 预演面板 | MIT |
| [pi](https://github.com/earendil-works/pi) | 可选的 agent 引擎 sidecar | MIT |
| [ffmpeg / ffprobe](https://ffmpeg.org/) | 转码、剪辑导出 | LGPL / GPL（视构建而定） |
| [whisper.cpp](https://github.com/ggerganov/whisper.cpp) | 语音识别 | MIT |
| [Tauri](https://tauri.app/)、[Vue](https://vuejs.org/) | 应用外壳与前端 | MIT |

其余依赖及其许可见 `package.json` / `src-tauri/Cargo.toml`。

## 许可

本项目采用 [Apache License 2.0](LICENSE) 许可。
