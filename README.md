# Duanju Workbench

**English** · [中文说明](README_zh_cn.md)

A desktop workbench that takes a short-form drama from script to finished cut. Nine panels
cover the whole pipeline, and a built-in agent — **Simon** — works across all of them:
it can read and write project data, call image/video generation endpoints, and build the
timeline for you.

```
Script → Style → Storyboard → Assets → 3D Previz → Video Prompts → Generate → Edit → Subtitles
```

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB.svg)](https://tauri.app/)
[![Vue](https://img.shields.io/badge/Vue-3-42b883.svg)](https://vuejs.org/)

> Not affiliated with any model provider. Text, image, and video backends are all pluggable —
> see [docs/PROVIDER.md](docs/PROVIDER.md). **The docs under `docs/` are currently written in Chinese.**

## Contents

- [Highlights](#highlights)
- [Quick start](#quick-start)
- [Before your first run](#before-your-first-run)
- [Project layout](#project-layout)
- [Simon: one agent, shared context](#simon-one-agent-shared-context)
- [Skills](#skills)
- [Prompt caching](#prompt-caching)
- [whisper GPU acceleration](#whisper-gpu-acceleration)
- [ffmpeg](#ffmpeg)
- [Known limitations](#known-limitations)
- [Third-party components](#third-party-components)
- [License](#license)

## Highlights

- **Nine panels, one pipeline** — script, style bible, storyboard, assets, 3D previz,
  video prompts, generation, editing, subtitles. Everything lives in a plain project
  directory; no database.
- **An agent that actually operates the app** — 47 tools, merged into one shared toolset,
  so it can chain work across panels in a single turn.
- **Runs with zero configuration** — no text model configured falls back to a placeholder
  adapter; no image/video endpoint configured falls back to ffmpeg-generated placeholder
  assets. The entire pipeline (including edit export and subtitles) is runnable out of the box.
- **Prompt caching as a design constraint** — the system prompt is split into a frozen
  prefix plus an append-only tail, so the server-side prefix cache keeps hitting even as
  your project data changes.
- **Two agent engines** — a built-in one, and an optional [pi](https://github.com/earendil-works/pi)
  sidecar that reuses the exact same tools, approvals, and validation.

### 3D previz (DirectorDesk)

The previz panel integrates [DirectorDesk](https://github.com/mangfufu/director-desk) (MIT):
block out a grey-box set, place characters, design multi-camera work and camera moves
(Hitchcock zoom, handheld shake presets, …), light the scene, and export a reference video.

- **Works out of the box** — its build output ships inside the app
  (`npm run director:prepare` clones upstream → patches → builds → deploys; upstream moves
  every few months, so re-running it occasionally is enough).
- **The project travels with the scene** — "Save / Restore project" in the top bar writes to
  `<project>/previz/director.json`, so you can pick it up on another machine.
- **Agent-operable** — tell Simon "read the previz project, add a character to the stage and
  design a left-to-right camera move" and both engines (built-in and pi) will drive
  DirectorDesk: read the project, dress the set, block movement, design camera work.
  The panel doesn't even need to be open — the agent switches to it automatically.
- **The agent can *see* it** — `director_frame` renders the current camera (or the staging
  view) into an image and puts it in the model's context, so it checks composition, character
  facing, and interpenetration by actually looking, instead of claiming "it's all placed".

## Quick start

```bash
npm install          # dependencies
npm run app:dev      # dev: vite + cargo build + open the window
npm run app:build    # produce installers (NSIS) + a release binary
npm run pack         # gather a portable app/ directory and create a desktop shortcut
```

> `npm run pack` copies the main binary and sidecars out of `target/release` into `app/`,
> where the paths stay stable (a `cargo clean` won't disturb them). The whole `app/` directory
> can be copied to another Windows machine and run directly — provided WebView2 is present
> (bundled on Windows 11; a one-time install on Windows 10).

> Command-line self-checks only print under **debug builds**: the release binary is a GUI
> subsystem program with no console attached.

On first run a config directory is created:

| Platform | Location |
| --- | --- |
| Windows | `%APPDATA%\com.duanju.workbench\` |
| macOS | `~/Library/Application Support/com.duanju.workbench/` |

It holds `settings.json` (preferences and provider list), `secrets.json` (API keys), and
`models/` (whisper models).

You can run self-checks without opening the UI (try this first when something breaks):

```bash
cd src-tauri
cargo run -- --net-check                         # proxy settings + model registry reachability
cargo run -- --pipeline-check                    # run the whole production chain (placeholder model/assets)
cargo run -- --asr-download ggml-large-v3-turbo  # pre-download a whisper model
```

## Networking and proxies

Tools like v2rayN / Clash typically only set the **system proxy** and don't write environment
variables, while HTTP libraries only honour environment variables by default — so you get
"the browser works but the app can't connect". The workbench handles this:

- **Follow system** by default: reads environment variables first, then the Windows registry
  `Internet Settings\ProxyServer` (or `scutil --proxy` on macOS), and uses what it finds.
- Or switch to **direct** or **manual** (supports `http://` and `socks5://`).
- **Loopback addresses always bypass the proxy** (127.0.0.1 / LAN ranges), so a local
  vLLM / Ollama / ComfyUI is unaffected.
- "Settings → Runtime" shows the system proxy, the environment variables, and the value
  actually in use, plus a "Test connectivity" button.

Changing proxy settings rebuilds the HTTP client automatically — no restart needed.

## Before your first run

1. **Configure a model endpoint** — top right, "Settings → Providers". Text models support
   OpenAI-compatible endpoints and Anthropic's native API; image/video generation use the
   generic HTTP adapter, where the request format is part of the configuration. See
   [docs/PROVIDER.md](docs/PROVIDER.md) *(Chinese)*.
2. **Or don't configure anything** — see "Runs with zero configuration" above.
3. **GPU acceleration for subtitles is a compile-time choice** — see
   [whisper GPU acceleration](#whisper-gpu-acceleration).

## UI and design system

The frontend uses no component library; it ships a small in-house design system
(`src/ui/`, ~20 components: button, input, select, switch, tag input, modal, drawer,
tooltip, toast, confirm dialog, …). The upside is total control over the visuals; the
cost is maintaining the interaction details yourself.

Three design rules (please keep them when changing styles):

1. **Borders are low-opacity white** (`rgba(255,255,255,.07)`), not solid grey.
   A grey line on a dark background reads as an outline — dirty and cramped.
2. **Express structure with surface elevation**, not borders everywhere. The lightness
   steps of `--surface-1..5` are the separation.
3. **Give it room.** Panel padding starts at 16px, headers are 44–48px tall — better to
   show less content.

Colors, spacing, radii, and font sizes all live in `src/ui/tokens.css`; to re-theme,
touch only that file.

Both **dark and light** themes live in `tokens.css` (`[data-theme="light"]` overrides the
same variables). The switch is at the bottom of the sidebar and the choice is persisted.
Don't hard-code colors in components — add a variable first.

### Working on the UI without the desktop app

```bash
npm run dev     # open http://localhost:1420
```

With no Tauri runtime, `src/api/ipc.ts` routes commands to `src/dev/mock.ts` instead — a
realistic sample project (4 script chapters, 6 shots, 4 assets, an agent conversation with
tool calls). No Rust rebuild needed when tweaking the UI, which also makes screenshots easy
to compare. The mock only loads in the browser and is never bundled into the desktop build.

## Project layout

Every project is just a plain directory — no database, and copying it to another machine
(including Apple Silicon) is enough to open it:

```
my-drama/
  project.json               project manifest
  bible.json                 project bible (setting / characters / hooks)
  script/index.json          chapter index
  script/chapters/*.md       chapter bodies (Markdown — edit them directly)
  style/style.json           visual style bible
  storyboard/index.json      shot list
  assets/index.json          asset manifest
  assets/files/<asset>/*.png character turnarounds / scenes / props
  prompts/video_prompts.json video prompts and their asset bindings
  video/takes.json           generation results
  video/files/*.mp4          generated video
  edit/timeline.json         timeline
  edit/renders/*.mp4         exported cuts
  subtitles/                 subtitles (srt / vtt)
  checklist/checklist.json   checklist
  .workbench/                runtime data — safe to delete entirely
```

## Simon: one agent, shared context

Simon is **one** assistant, not nine. It doesn't partition context per panel, and its toolset
is merged — ask it in the script panel to "finish the storyboard and then create the assets"
and it will call tools from both panels in sequence. Panels only decide the opening line and
quick suggestions; they don't limit what it can do. A useful side effect: switching panels
doesn't invalidate the prompt prefix, so the cache stays more stable.

It lives behind the floating button in the bottom-right corner. Opening it as a floating
window is what lets panels take the full width.

### Two engines

There's an engine toggle below the input box:

- **Built-in** (default) — the project's own agent: 47 tools, a frozen prefix cache, and a
  spend-confirmation flow. Fully usable offline (the placeholder adapter runs when no model
  is configured).
- **pi** (optional) — talks to [pi](https://github.com/earendil-works/pi), an MIT-licensed
  open-source agent harness. It runs permanently as a sidecar subprocess over its RPC
  protocol; its event stream is translated into the same event types as the built-in engine,
  so the UI reuses the rendering path entirely. pi's own shell and file tools are explicitly
  disabled — the 47 tools it can use all round-trip through a local bridge into the same
  `actions`, so **side effects, approvals, and data validation are identical** to the built-in
  engine. Spend operations go through pi's confirmation protocol and surface an approval card.
  Sessions are stored under the project's `.workbench/pi/`, switching automatically with the
  project.

A text-model provider is required (Settings → Providers; any OpenAI-compatible endpoint will
do). The key is passed to the subprocess via an environment variable only and never touches disk.

### Bundled runtime (install and go)

Installers ship every runtime, so users don't have to install anything themselves:

| Component | Purpose | Form |
| --- | --- | --- |
| node + pi runtime | pi engine sidecar | `sidecar/pi/` (~460 MB) |
| ffmpeg / ffprobe | transcoding, edit export | `binaries/` (~200 MB) |
| whisper-cli | subtitle transcription (CPU inference) | `binaries/` |
| whisper small model | default transcription model | `models/` (~470 MB, seeded to the user directory on first launch) |

Preparation script: `node scripts/prepare-sidecar.mjs` (idempotent; fills in or updates each
component as needed).

**Two distribution forms:**

- **Installer**: `npm run app:build` → NSIS / MSI installers (~580 / 660 MB).
- **Portable**: `npm run pack` → an `app/` directory (~1.1 GB) that runs from anywhere without
  installation. All runtimes are already inside it (`binaries/`, `models/`, `sidecar/` next to
  the exe). The only system dependency is the WebView2 runtime — bundled on Windows 11 and
  normally preinstalled by Windows Update on Windows 10.

### Three permission modes

| Mode | Behaviour |
| --- | --- |
| **YOLO** | Everything executes immediately, no interruptions |
| **Auto-edit** (default) | Data changes run automatically; only spend operations (image/video generation) ask first |
| **Confirm before change** | Any operation that mutates project data asks first |

The criterion is whether a tool is on the read-only allowlist (`READ_ONLY_TOOLS` in
`src-tauri/src/agent/tools.rs`).

### Context management

Conversations can't grow forever. Three mechanisms keep Simon's context in check:

**1. Feed on demand — don't dump everything in**

Type `@` in the input to reference skills, chapters, and assets explicitly. Only what you
name travels with that message; everything else stays out.

```
Walk @chapter:3 through @skill:script-pacing-check
```

**2. Compact proactively — only when the water level rises**

The percentage at the top of the window is the context water level (hover for a breakdown:
conversation, how much of it is tool results, frozen prefix). Past 75% of budget it compacts
automatically, and you can also trigger it manually.

Compaction comes in two cuts, light first:

| Order | What it does | Cost |
| --- | --- | --- |
| First cut | Replaces the **raw text** of early tool results with a short description | Free, zero risk |
| Second cut | Has the model summarise the first half of the conversation | One model call |

Tool results are usually the biggest block — a single `project_snapshot` can run into tens of
thousands of tokens, and it was only useful at the moment it was called. So the first cut
often suffices.

**Crucially**: compaction only touches the tail of the conversation — the **frozen prefix is
untouched**, so that cache is never compacted away.

Budget and the auto-compaction switch are under "Settings → Runtime → Agent behaviour".

**3. Start a new conversation — the cleanest context there is**

The dropdown in the window's title bar lists every conversation and lets you switch at any
time, or start a fresh one from zero. Old conversations stick around; switch back and carry on.

### It asks you when it's unsure

When Simon is uncertain it uses `ask_user`, which the frontend renders as a card: you can
get option buttons or free-form input. It then stops and waits rather than guessing its way
forward.

## Skills

Drop in your own processes, conventions, and methodologies — Simon picks them up when they're
relevant.

Skills live in `<config dir>/skills/` and use the generic Agent Skills directory format, so
existing skill packs can be copied straight in:

```
skills/
  script-pacing-check.md   ← single-file skill
  my-methodology/          ← directory skill
    SKILL.md               ← required, with frontmatter
    references/            ← optional attachments
      visual-dna.md
```

`SKILL.md` frontmatter:

```markdown
---
name: script-pacing-check
description: Use when the user wants to review or improve short-drama script pacing, including hook density and reversal interval criteria
---
```

**`description` matters most** — it's the only thing the model has to decide whether to use a
skill. Write *when to use it*, not *what it is*.

### Three-level progressive disclosure

This is the key to the whole design, and it directly determines cost:

| Level | Content | When it enters context |
| --- | --- | --- |
| 1 | Name + description | Always (L4 layer of the system prompt, cacheable) |
| 2 | `SKILL.md` body | When the model judges it relevant — fetched with `skill_read` |
| 3 | Attachments | When actually needed — fetched with `skill_read_file` |

Measured: with 4 skills installed and 46,000 characters of combined body text, the directory
that enters the system prompt is only **372 characters**.

Import via "Settings → Skills":

- **Pick a `.zip`** (multi-select) — easiest. A zip containing one skill or a whole collection
  is recognised automatically.
- Pick a skill directory, or a **collection directory** holding several skills.
- Pick a single `.md`.

You can also import from the command line without opening the UI:

```bash
cd src-tauri
cargo run -- --import-skill D:/downloads/skill-pack.zip
cargo run -- --skills        # see what's installed
```

### Working from Word documents

Convert to Markdown first, then import. A script ships with the repo:

```bash
python scripts/docx2md.py input.docx out/ --kind style --name "My Style Bible"
python scripts/docx2md.py some-dir/ out/ --guess-headings   # batch, restoring headings
```

## Prompt caching

This is where the workbench spends the most thought on cost.

Server-side prefix caching (Anthropic's explicit breakpoints, or the automatic prefix cache of
OpenAI-compatible endpoints) matches **byte-for-byte on the prefix**: change one character and
everything after it is re-billed. So what determines the hit rate isn't "where you put the
breakpoint" but "is there anything in the prefix that changes".

The approach splits the prompt into two layers:

- **Frozen prefix** (built once when the session is created, then unchanged for the whole
  conversation)
  - `L0 Core instructions` — global working principles
  - `L1 Workbench responsibilities` — output conventions for the nine panels (**does not vary
    with the current panel**)
  - `L2 Project bible` — setting, characters, style bible
  - `L3 Asset index` — asset manifest and view status
  - the **context snapshot** in the session's first message (current panel, chapter state,
    gap statistics)
- **Incremental tail** — tool results and new turns, always appended, never rewritten

So "project data changed" doesn't pollute the cache: when the agent needs fresh data it calls
a tool, and the result lands in the tail. The window's top right shows the live hit rate and
per-layer token estimates; pressing ↻ rebuilds the context at the cost of one cache miss,
in exchange for the agent seeing current data.

Two invariants to preserve when changing code (documented at the top of
`src-tauri/src/agent/prompt.rs`):

1. Once `ChatRequest.system` is frozen, it never changes.
2. `messages` is append-only; historical messages are never rewritten.

Tool definition order and field order affect the cache too, which is why the registration
order in `agent/tools.rs` is fixed — don't turn it into a HashMap iteration.

## whisper GPU acceleration

Transcription runs on a local whisper.cpp, via one of two paths:

1. **In-process inference** (requires building with a feature)
   ```bash
   # Windows / Linux + NVIDIA
   cargo build --features asr-cuda
   # Apple Silicon
   cargo build --features asr-metal
   # CPU only
   cargo build --features asr
   ```
   Requires `cmake` and `libclang` (LLVM). The backend is decided **at compile time**;
   changing GPUs means rebuilding. The "use GPU" runtime toggle only applies within the
   backends that were compiled in.
2. **External whisper-cli** — put whisper.cpp's official `whisper-cli` next to the app, or
   point to it in settings. The default build takes this path and doesn't need libclang.

Both paths first use ffmpeg to normalise input to 16 kHz mono wav, so the input is consistent.
The subtitle panel shows local detection results (GPU model, CUDA version, recommended
backend, compiled backend).

**Model downloads**: ggml models are pulled from HuggingFace by default. If that's unreachable,
switch "Settings → Subtitles → Download source" to the `hf-mirror` mirror, or download a
`ggml-*.bin` yourself and drop it into the "model directory" (which settings can open for you).
For command-line pre-download see `--asr-download` above.

## ffmpeg

Release builds bundle `ffmpeg` / `ffprobe` (`src-tauri/binaries/`). During development, if the
sidecar isn't found it falls back to the system PATH and warns in the log. You can also point
to them manually under "Settings → Runtime".

Sidecar naming convention:

```
src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe
src-tauri/binaries/ffprobe-x86_64-pc-windows-msvc.exe
```

On Windows, copying the system ffmpeg is enough (the triple in the name must match
`rustc -vV`'s host):

```bash
cd src-tauri && mkdir -p binaries
cp "$(where.exe ffmpeg | head -1 | tr -d '\r')"  binaries/ffmpeg-x86_64-pc-windows-msvc.exe
cp "$(where.exe ffprobe | head -1 | tr -d '\r')" binaries/ffprobe-x86_64-pc-windows-msvc.exe
```

> Those two files are ~440 MB combined and are excluded in `.gitignore` — don't commit them.

## Repository layout

```
src/                      frontend
  api/                    Tauri command wrappers + event subscriptions
  stores/                 Pinia: project / agent / jobs / settings
  components/             agent dock, panel navigation, top bar, job bar, settings drawer
  views/panels/           the nine panels
  types/                  TS types mirroring the Rust models
src-tauri/src/            backend
  models.rs               all data structures
  project.rs / store.rs   project directory layout and atomic writes
  llm/                    provider adapters (openai / anthropic / mock)
  agent/                  sessions, tool loop, cache-friendly prompt assembly
  gen/                    image/video generation (config-driven generic HTTP adapter)
  actions.rs              high-level actions shared by frontend commands and agent tools
  jobs.rs                 background job queue
  media.rs                ffmpeg invocation and timeline export
  asr.rs                  whisper capability detection, model download, transcription
  progress.rs             panel completion logic (single source of truth)
docs/                     provider configuration and architecture notes (Chinese)
```

## Known limitations

- The edit panel's export is "concatenate the main video track in start-time order (**each
  clip's own audio track comes along**, padded with equal-length silence if absent) + audio
  clips mixed on top + optional burned-in subtitles". Transitions, speed curves, and
  multi-track compositing are not implemented.
- whisper's in-process inference needs libclang to compile and hasn't been tested on the
  author's machine; the external CLI path and capability detection are verified working.
- The agent's spend tools (image/video generation) require manual confirmation by default;
  this can be turned off in settings.

## Third-party components

The build scripts fetch these at prepare time; their source isn't vendored here.

| Component | Used for | License |
| --- | --- | --- |
| [DirectorDesk](https://github.com/mangfufu/director-desk) | 3D previz panel | MIT |
| [pi](https://github.com/earendil-works/pi) | optional agent engine sidecar | MIT |
| [ffmpeg / ffprobe](https://ffmpeg.org/) | transcoding, edit export | LGPL / GPL (per build) |
| [whisper.cpp](https://github.com/ggerganov/whisper.cpp) | speech recognition | MIT |
| [Tauri](https://tauri.app/), [Vue](https://vuejs.org/) | app shell and frontend | MIT |

Package dependencies and their licenses are listed in `package.json` / `src-tauri/Cargo.toml`.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
