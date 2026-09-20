// 把 release 产物收拢成一个稳定的绿色版目录，并在桌面建快捷方式。
//
// 用法：
//   node scripts/make-app.mjs            # 拷贝 + 建快捷方式
//   node scripts/make-app.mjs --no-link  # 只拷贝
//
// 为什么不直接把快捷方式指向 target/release：那是 cargo 的产物目录，
// 一次 cargo clean 或者切换构建配置就会失效。这里拷到 app/ 下面，路径稳定，
// 整个目录也可以直接拷到别的机器上用（Windows 需要装 WebView2 运行时，Win11 自带）。

import { existsSync, mkdirSync, copyFileSync, statSync, readdirSync, rmSync, cpSync } from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const src = join(root, "src-tauri", "target", "release");
const dst = join(root, "app");
const exeName = "duanju-workbench.exe";

function sizeOf(p) {
  const st = statSync(p);
  if (!st.isDirectory()) return st.size;
  let total = 0;
  for (const e of readdirSync(p, { withFileTypes: true })) total += sizeOf(join(p, e.name));
  return total;
}

if (!existsSync(join(src, exeName))) {
  console.error(`找不到构建产物：${join(src, exeName)}\n先跑 npm run app:build`);
  process.exit(1);
}

mkdirSync(dst, { recursive: true });

// 主程序 + 同目录下的 sidecar（ffmpeg / ffprobe）与依赖 dll
const wanted = readdirSync(src).filter(
  (f) => /\.(exe|dll)$/i.test(f) && statSync(join(src, f)).isFile(),
);

let bytes = 0;
for (const f of wanted) {
  // 安装器之类的 exe 不要拷进来
  if (/setup|installer|\.msi$/i.test(f)) continue;
  copyFileSync(join(src, f), join(dst, f));
  bytes += statSync(join(dst, f)).size;
  console.log(`  + ${f}`);
}

// 随包运行时目录：whisper 二进制 / whisper 模型 / pi sidecar（node + pi）
// 绿色版必须带上它们，否则换台机器就没有字幕与 pi 引擎。
//
// 注意：Windows 上不能用 node 的 cpSync 拷这些目录 —— 目录里成堆的 .exe/.dll
// 会让 Node 进程被系统的行为监控直接杀掉（实测 exit 127、无任何异常输出），
// 改用独立的 powershell 进程拷贝，稳。
function copyDir(srcDir, dstDir) {
  if (process.platform === "win32") {
    const q = (s) => s.replace(/'/g, "''");
    const ps = [
      `if (Test-Path -LiteralPath '${q(dstDir)}') { Remove-Item -Recurse -Force -LiteralPath '${q(dstDir)}' }`,
      `Copy-Item -Recurse -Force -LiteralPath '${q(srcDir)}' -Destination '${q(dstDir)}'`,
    ].join("; ");
    execFileSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", ps], {
      stdio: "inherit",
    });
    return;
  }
  rmSync(dstDir, { recursive: true, force: true });
  cpSync(srcDir, dstDir, { recursive: true });
}

for (const d of ["binaries", "models", "sidecar"]) {
  const s = join(src, d);
  if (!existsSync(s)) {
    console.log(`  - 跳过 ${d}/（不存在，先跑 node scripts/prepare-sidecar.mjs）`);
    continue;
  }
  copyDir(s, join(dst, d));
  const size = sizeOf(join(dst, d));
  bytes += size;
  console.log(`  + ${d}/（${(size / 1024 / 1024).toFixed(0)} MB）`);
}
console.log(`\n已打包到 ${dst}（${(bytes / 1024 / 1024).toFixed(1)} MB）`);

/* ------------------------------------------------------------ 桌面快捷方式 */

if (process.argv.includes("--no-link")) process.exit(0);

const target = join(dst, exeName);
const ps = `
$ErrorActionPreference = 'Stop'
$desktop = [Environment]::GetFolderPath('Desktop')
$lnkPath = Join-Path $desktop 'Simon 短剧工作台.lnk'
$ws = New-Object -ComObject WScript.Shell
$lnk = $ws.CreateShortcut($lnkPath)
$lnk.TargetPath = '${target.replace(/'/g, "''")}'
$lnk.WorkingDirectory = '${dst.replace(/'/g, "''")}'
$lnk.IconLocation = '${target.replace(/'/g, "''")},0'
$lnk.Description = 'Simon 短剧工作台 · 从剧本到成片'
$lnk.WindowStyle = 1
$lnk.Save()
Write-Output $lnkPath
`;
try {
  const out = execFileSync(
    "powershell",
    ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", ps],
    { encoding: "utf8" },
  ).trim();
  console.log(`桌面快捷方式已创建：${out}`);
} catch (e) {
  console.error("创建快捷方式失败：", e.message);
  process.exitCode = 1;
}
