// 把 release 产物收拢成一个稳定的绿色版目录，并在桌面建快捷方式。
//
// 用法：
//   node scripts/make-app.mjs            # 拷贝 + 建快捷方式
//   node scripts/make-app.mjs --no-link  # 只拷贝
//
// 为什么不直接把快捷方式指向 target/release：那是 cargo 的产物目录，
// 一次 cargo clean 或者切换构建配置就会失效。这里拷到 app/ 下面，路径稳定，
// 整个目录也可以直接拷到别的机器上用（Windows 需要装 WebView2 运行时，Win11 自带）。

import { existsSync, mkdirSync, copyFileSync, statSync, readdirSync, rmSync } from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const src = join(root, "src-tauri", "target", "release");
const dst = join(root, "app");
const exeName = "duanju-workbench.exe";

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
