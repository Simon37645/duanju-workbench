/**
 * 准备随安装包分发的运行时（幂等，可重复执行）：
 *
 *   1. ffmpeg / ffprobe   → src-tauri/binaries/{stem}-<triple>.exe   （Tauri externalBin）
 *   2. whisper-cli + dll  → src-tauri/sidecar-dist/bin/              （resources → 安装目录/binaries）
 *   3. node + pi 运行时   → src-tauri/sidecar-dist/pi/               （resources → 安装目录/sidecar/pi）
 *   4. whisper 模型        → src-tauri/sidecar-dist/models/           （resources → 安装目录/models，首启拷给用户）
 *
 * 用户装完即用：不需要自己装 node / pi / ffmpeg / whisper，也不用手动下模型。
 *
 * 用法：
 *   node scripts/prepare-sidecar.mjs                 # 全量准备（缺什么补什么）
 *   node scripts/prepare-sidecar.mjs --skip-model    # 跳过 466MB 的模型下载
 *   node scripts/prepare-sidecar.mjs --ffmpeg-zip <zip>  # 用已下载的 ffmpeg essentials zip
 *   node scripts/prepare-sidecar.mjs --slim          # 额外打印 pi node_modules 裁剪建议
 */
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { execFileSync } from "node:child_process";

const ROOT = path.resolve(import.meta.dirname, "..");
const TAURI = path.join(ROOT, "src-tauri");
const DIST = path.join(TAURI, "sidecar-dist");
const BIN = path.join(DIST, "bin");
const PI_DEST = path.join(DIST, "pi");
const MODELS = path.join(DIST, "models");
const TRIPLE = process.env.TARGET_TRIPLE || "x86_64-pc-windows-msvc";

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const opt = (name, dflt) => {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : dflt;
};

const log = (msg) => console.log(`  ${msg}`);
const mb = (bytes) => `${Math.round(bytes / 1024 / 1024)}MB`;

function sizeOf(p) {
  if (!fs.existsSync(p)) return 0;
  const st = fs.statSync(p);
  if (!st.isDirectory()) return st.size;
  let total = 0;
  for (const e of fs.readdirSync(p, { withFileTypes: true })) {
    total += sizeOf(path.join(p, e.name));
  }
  return total;
}

function whichInPath(names) {
  const dirs = (process.env.PATH || "").split(path.delimiter);
  for (const d of dirs) {
    for (const n of names) {
      const cand = path.join(d, n);
      if (fs.existsSync(cand)) return cand;
    }
  }
  return null;
}

function copyFile(src, dst) {
  fs.mkdirSync(path.dirname(dst), { recursive: true });
  fs.copyFileSync(src, dst);
}

/* ------------------------------------------------------------ 1. ffmpeg */

function prepareFfmpeg() {
  const ff = path.join(TAURI, "binaries", `ffmpeg-${TRIPLE}.exe`);
  const fp = path.join(TAURI, "binaries", `ffprobe-${TRIPLE}.exe`);
  if (fs.existsSync(ff) && fs.existsSync(fp)) {
    log(`ffmpeg/ffprobe 已就位（${mb(sizeOf(ff))} + ${mb(sizeOf(fp))}）`);
    if (flag("--slim")) {
      log("提示：若体积偏大，可用 essentials 构建替换（约 90MB/个）");
    }
    return;
  }
  const zip = opt("--ffmpeg-zip", "");
  if (zip && fs.existsSync(zip)) {
    log(`从 ${zip} 解压 ffmpeg…`);
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "ffmpeg-extract-"));
    try {
      // Windows 10+ 自带 bsdtar，支持 zip；比 PowerShell 的 Expand-Archive 稳
      execFileSync("tar", ["-xf", zip, "-C", tmp]);
    } catch {
      execFileSync("powershell", [
        "-NoProfile", "-Command",
        `Expand-Archive -LiteralPath '${zip}' -DestinationPath '${tmp}' -Force`,
      ]);
    }
    const found = [];
    const walk = (dir) => {
      for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
        const p = path.join(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name === "ffmpeg.exe") found.push(p);
        else if (e.name === "ffprobe.exe") found.push(p);
      }
    };
    walk(tmp);
    const ffSrc = found.find((p) => path.basename(p) === "ffmpeg.exe");
    const fpSrc = found.find((p) => path.basename(p) === "ffprobe.exe");
    if (ffSrc && fpSrc) {
      copyFile(ffSrc, ff);
      copyFile(fpSrc, fp);
      log(`ffmpeg/ffprobe 已写入 binaries/（${mb(sizeOf(ff))} + ${mb(sizeOf(fp))}）`);
      return;
    }
    log("zip 里没找到 ffmpeg.exe / ffprobe.exe");
  }
  const src = whichInPath(["ffmpeg.exe", "ffmpeg"]);
  const srcProbe = whichInPath(["ffprobe.exe", "ffprobe"]);
  if (src && srcProbe) {
    copyFile(src, ff);
    copyFile(srcProbe, fp);
    log(`ffmpeg/ffprobe 已从 PATH 复制（${mb(sizeOf(ff))} + ${mb(sizeOf(fp))}）`);
    return;
  }
  log("[跳过] 找不到 ffmpeg：用 --ffmpeg-zip <zip> 指定，或把它加进 PATH");
}

/* ----------------------------------------------------------- 2. whisper */

function prepareWhisperCli() {
  const cli = path.join(BIN, "whisper-cli.exe");
  if (fs.existsSync(cli)) {
    log(`whisper-cli 已就位（${mb(sizeOf(BIN))} 含 dll）`);
    return;
  }
  log("[跳过] sidecar-dist/bin 里没有 whisper-cli.exe：请解压 whisper.cpp 的 whisper-bin-x64.zip 进去");
}

/* ---------------------------------------------------------------- 3. pi */

function preparePi() {
  if (fs.existsSync(path.join(PI_DEST, "dist", "bundle", "cli.js"))) {
    log(`pi 运行时已就位（${mb(sizeOf(PI_DEST))}）`);
    if (flag("--slim")) printSlimAdvice();
    return;
  }
  const piRoot = opt(
    "--pi-root",
    path.join(os.homedir(), "AppData", "Roaming", "npm", "node_modules", "@earendil-works", "pi-coding-agent"),
  );
  if (!fs.existsSync(path.join(piRoot, "dist", "bundle", "cli.js"))) {
    log(`[跳过] 找不到 pi 安装：${piRoot}（npm i -g @earendil-works/pi-coding-agent）`);
    return;
  }
  log(`从 ${piRoot} 复制 pi 运行时（约 400MB，稍等）…`);
  fs.mkdirSync(PI_DEST, { recursive: true });
  fs.cpSync(path.join(piRoot, "dist"), path.join(PI_DEST, "dist"), { recursive: true });
  copyFile(path.join(piRoot, "package.json"), path.join(PI_DEST, "package.json"));
  fs.cpSync(path.join(piRoot, "node_modules"), path.join(PI_DEST, "node_modules"), { recursive: true });
  const node = whichInPath(["node.exe", "node"]);
  if (node) {
    copyFile(node, path.join(PI_DEST, "node", "node.exe"));
    log(`node 已复制：${node}`);
  } else {
    log("[警告] 找不到 node.exe，pi 无法启动");
  }
  log(`pi 运行时就绪（${mb(sizeOf(PI_DEST))}）`);
  if (flag("--slim")) printSlimAdvice();
}

function printSlimAdvice() {
  const nm = path.join(PI_DEST, "node_modules");
  if (!fs.existsSync(nm)) return;
  const rows = fs
    .readdirSync(nm, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => ({ name: e.name, size: sizeOf(path.join(nm, e.name)) }))
    .sort((a, b) => b.size - a.size)
    .slice(0, 8);
  log("node_modules 体积 TOP8（人工评估可裁剪项，删错会导致 pi 启动失败）：");
  for (const r of rows) log(`    ${mb(r.size).padStart(7)}  ${r.name}`);
}

/* -------------------------------------------------------------- 4. 模型 */

async function prepareModel() {
  if (flag("--skip-model")) {
    log("[跳过] 模型准备（--skip-model）");
    return;
  }
  const dst = path.join(MODELS, "ggml-small.bin");
  if (fs.existsSync(dst) && fs.statSync(dst).size > 100 * 1024 * 1024) {
    log(`whisper 模型已就位（${mb(fs.statSync(dst).size)}）`);
    return;
  }
  const url = opt(
    "--model-url",
    "https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
  );
  log(`下载 whisper 模型 ${url}（约 466MB，国内走 hf-mirror）…`);
  fs.mkdirSync(MODELS, { recursive: true });
  execFileSync("curl", ["-sL", "--max-time", "3000", "-o", dst, url], { stdio: "inherit" });
  log(`模型就绪（${mb(fs.statSync(dst).size)}）`);
}

/* ---------------------------------------------------------------- main */

console.log("\n=== 准备随包运行时 ===\n");
fs.mkdirSync(BIN, { recursive: true });
prepareFfmpeg();
prepareWhisperCli();
preparePi();
await prepareModel();
console.log(`\n总计：${mb(sizeOf(DIST))}（sidecar-dist）+ ${mb(sizeOf(path.join(TAURI, "binaries")))}（binaries）\n`);
