# 架构说明

## 一句话

前端是纯展示 + 交互，所有状态与业务逻辑在后端；命令层与 agent 工具层共用同一套
`actions`，保证「用户手点」和「agent 自动做」产生完全一样的副作用。

### 前端分层

```
src/ui/            自研设计系统（tokens.css + base.css + 20 个组件）
src/dev/mock.ts    浏览器预览用的假后端（不在 Tauri 里时自动启用）
src/stores/        Pinia：项目 / agent / 任务 / 设置
src/components/    外壳：侧栏、顶栏、agent 对话坞、任务条、设置抽屉
src/views/panels/  九个面板
```

```
┌──────────────────────────── Vue 前端 ────────────────────────────┐
│  HomeView（项目列表/新建/打开）                                     │
│  WorkspaceView                                                    │
│   ├─ PanelRail      九个面板 + 完成度徽标                          │
│   ├─ TopBar         项目信息 / 模型选择 / 缓存命中                   │
│   ├─ panels/*.vue   九个面板                                       │
│   ├─ AgentDock      流式对话、工具卡片、花钱确认、缓存分层            │
│   └─ JobBar         后台任务进度                                   │
│  stores: project / agent / jobs / settings   types: 与 Rust 对齐    │
└───────────────┬───────────────────────────────────────────────────┘
                │ invoke / Channel / event
┌───────────────▼───────────────── Rust 后端 ───────────────────────┐
│  commands.rs      前端唯一入口（薄封装）                            │
│  agent/           会话、工具循环、缓存友好提示词装配、38 个工具        │
│  pi.rs            pi sidecar（RPC）：可选对话引擎，常驻进程 + 事件映射  │
│  pi_bridge.rs     pi 工具桥：本地 HTTP，extension ↔ Rust 的工具与审批   │
│  actions.rs       高层动作（生图/生视频/铺轨/导出/转写）             │
│  llm/             openai / anthropic / mock 适配器 + SSE 解析       │
│  gen/             generic-http（配置驱动）+ mock                    │
│  jobs.rs          任务队列（并发闸门、进度、取消、持久化）            │
│  media.rs         ffmpeg 定位 / 探针 / 转码 / 时间线导出             │
│  asr.rs           whisper 能力探测 / 模型下载（断点续传）/ 转写       │
│  net.rs           代理解析（系统代理 / 环境变量 / 手动）              │
│  selftest.rs      命令行自检（不启动界面，跑真实链路）                │
│  progress.rs      面板完成度判定（唯一事实来源）                     │
│  project.rs       项目目录布局 + 加载保存                            │
│  store.rs         原子写 / JSON / Markdown / 字数 / 指纹             │
│  state.rs         当前项目、会话表、审批通道、HTTP 客户端              │
└───────────────────────────────────────────────────────────────────┘
```

## 几个关键设计

### 1. 文件即数据库

项目 = 一个普通目录。结构化数据是 JSON，长文本是 Markdown，媒体就是媒体文件。
没有原生数据库依赖，所以：

- 拷到另一台机器（含 Apple Silicon）直接打开；
- 可以用 git 管版本；
- 用户能用任何编辑器直接改剧本。

代价是并发写要自己兜。做法是：整个项目常驻内存（`AppState.project`），
读走内存，写走 `mutate()` —— 加锁 → 改 → 立刻落盘 → 发 `project://changed` 事件。
落盘用「写临时文件再替换」，避免崩溃留下半个文件（Windows 上 rename 不能覆盖，
失败时退化为删掉再 rename）。

### 2. 命令层与工具层共用 actions

`actions.rs` 里的函数是唯一实现：

```rust
pub fn generate_asset_views(state: &AppState, asset_id: &str, view_ids: Vec<String>, force: bool)
    -> Result<Vec<String>>     // 返回 jobId 列表
```

- `commands.rs` 的 `asset_generate_views` 命令直接调它（用户在界面上点「生成」）
- `agent/tools.rs` 的 `asset_generate_view` 工具也调它（agent 决定要出图）

两边的副作用、任务入队、落盘、事件完全一致。新增能力时先在 `actions.rs` 落地，
再分别接命令与工具。

### 3. 任务队列

所有耗时操作都进 `JobQueue`：

- 按 kind 分并发闸门（生图 2、生视频 2、转写 1、转码 2、下载 3）
- 统一上报进度，`job://update` 事件推给前端
- 每个任务带 `Arc<AtomicBool>` 取消标志，runner 里主动 `ctx.check_cancel()?`
- 持久化到 `.workbench/jobs.json`；重启时把「上次还在跑」的标记为已中断

任务 runner 是个 `FnOnce(JobCtx) -> Future<Output = Result<String>>`，
错误自动落到任务记录的 `error` 字段，界面上直接能看到失败原因。

### 4. 缓存友好的提示词装配

详见 README 的「Agent 与提示词缓存」。核心是 `agent/prompt.rs`：

- `build_frozen_prefix()` 产出四层 system + 一段上下文快照，算一个 blake3 指纹
- 会话把这份前缀存下来（`AgentSession.prefix`），整场对话复用
- 重建只发生在三种情况：显式刷新、前缀缺失、**换模型**（不同模型的缓存不通用）
- 每轮把 `PrefixReport` 推给前端：各层 token 估算、断点位置、指纹、稳定与否

Anthropic 用显式断点（工具末尾 + 最后一个可缓存 system 层 + 最后一条消息，占满 4 个额度）；
OpenAI 兼容端点靠自动前缀缓存，客户端能做的是让前缀逐字节稳定 —— 所以：

- system 层里不放时间戳、随机 ID、HashMap 遍历顺序
- `serde_json` 不开 `preserve_order`，Map 是 BTreeMap，键序天然确定
- 工具注册顺序固定
- 对话只追加

### 5. 一个 agent，不是九个

早期版本按面板开九个会话、工具也按面板裁剪。改成共享上下文后：

- **会话只有一条**（`activeSessionId`），面板不再决定用哪个会话；
- **工具集是全量合并的**，Simon 能在一个回合里跨面板操作；
- **L1 提示词层写的是九个面板的规范**，不再随当前面板变化 —— 这反而让前缀更稳，
  切面板不会导致缓存失效；
- 当前面板只出现在上下文快照（首条用户消息）与开场白里。

### 7. 完成度判定只有一个来源

`progress.rs` 是唯一实现，两处消费：

- `project_progress` 命令 → 左侧导航徽标
- `checklist_report` 工具 → agent 想知道「还差什么」

面板完成度里的**自动项**由它实时算出来，不落盘；用户和 agent 能改的只有自定义项。
这样不会出现「勾了但数据其实没做完」的假象。

### 6. 权限模式与人工介入

agent 有三种模式（`agent_mode`），在 run loop 里按工具性质决定要不要拦下来：

| 模式 | 拦截条件 |
| --- | --- |
| yolo | 从不拦截 |
| auto（默认） | 只拦 `costly`（生图 / 生视频这类花钱的） |
| confirm | 拦截所有非只读工具 |

两种挂起复用同一套 oneshot 通道：

```
run loop ──emit toolCall{needsConfirm}──▶ 前端弹确认
        ◀──agent_approve(callId, ok)── AgentRuntime.approvals

run loop ──emit askUser{question,options}──▶ 前端渲染提问卡片
        ◀──agent_answer(callId, text)── AgentRuntime.answers
```

`ask_user` 在 run loop 里特判（不走工具 handler），因为只有那里同时握着事件通道和等待队列。
超时都按「拒绝 / 未回答」处理（审批 15 分钟、提问 30 分钟）。

### 8. agent 的视觉

模型要"看见"图，必须把图片放进消息里。两条协议的处理方式不同：

- 用户在对话坞附图 → 直接作为用户消息的 image block；
- **agent 自己看图** → 工具（`asset_view_image` / `file_view_image`）返回
  `ToolOutcome.images`，run loop 在工具结果之后**追加一条用户消息**带图片。

之所以不把图塞进工具结果本身：OpenAI 的 `role: tool` 消息不能带图，
而 Anthropic 会把连续同角色消息合并，所以「追加一条用户消息」是两边都成立的做法。

单张上限 6 MB、一次最多 4 张（够看人物三视图做一致性检查），避免把上下文撑爆。

### 9. 代理

v2rayN / Clash 只改系统代理、不写环境变量，而 HTTP 库默认只认环境变量 ——
于是"浏览器能上、应用连不上"。`net.rs` 把三种来源统一成一个 `resolve_proxy()`：
环境变量 → Windows 注册表 `Internet Settings` / macOS `scutil --proxy` → 手动指定。
拿到后交给 reqwest，并强制 `no_proxy` 掉本地地址（本地的 vLLM / Ollama 不能被拦）。
设置一改就丢缓存重建客户端，不必重启。

## 数据流：一次 agent 调用

```
用户输入
  └─ agent_run(command) ─▶ agent::run_turn
       ├─ 取/建会话，确保冻结前缀
       ├─ 追加用户消息（首条带上下文快照）
       ├─ emit runStarted / prefix
       └─ 循环（最多 maxToolRounds 轮）
            ├─ llm.stream() ──▶ emit textDelta / reasoningDelta / usage
            ├─ 组装 assistant 消息（文本 + tool_use）
            ├─ 无工具调用 → 结束
            └─ 逐个工具
                 ├─ emit toolCall（花钱的等审批）
                 ├─ (tool.run)(ctx, args) ─▶ actions::* ─▶ state.mutate()
                 │                                └─▶ emit project://changed ─▶ 前端刷新
                 └─ emit toolResult
       ├─ refresh_auto_items()（重算 checklist 自动项）
       ├─ 会话落盘
       └─ emit runFinished
```

前端在流式期间只维护一份实时缓冲区；run 结束后重新拉一次会话，后端是唯一事实来源。

## 加东西时改哪里

| 想做的事 | 改哪里 |
| --- | --- |
| 加一个 agent 能力 | `actions.rs` 加实现 → `agent/tools.rs` 注册工具 + `agent/tools/schema.rs` 加 schema |
| 加一个界面按钮 | `commands.rs` 加命令 → `lib.rs` 的 `generate_handler!` 注册 → `src/api/ipc.ts` 加封装 |
| 改数据结构 | `models.rs` 与 `src/types/models.ts` 同步改（字段名必须一致） |
| 接一家新模型服务 | 一般只改配置（见 PROVIDER.md）；协议太特殊才在 `llm/` 或 `gen/` 加适配器 |
| 改面板完成规则 | `progress.rs`（自动项与徽标会一起变） |
| 改缓存策略 | `agent/prompt.rs` 的分层，注意别破坏「前缀稳定」与「只追加」两条不变量 |
