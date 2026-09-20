/**
 * 准备「导演台」（DirectorDesk，MIT）的构建产物：
 *
 *   clone/更新源码 → 打补丁 → 构建 → 拷进 public/director/
 *
 * 为什么不直接用它的发布版：我们需要它暴露 `window.__director`
 * （读工程 / 替换工程 / 调工具），原版只在开发模式暴露 —— 补丁去掉这个条件，
 * 让生产构建也保留，宿主才能把 agent 桥接过来。
 *
 * 用法：
 *   node scripts/prepare-director.mjs            # 首次准备（clone + 构建）
 *   node scripts/prepare-director.mjs --update   # 拉上游最新再构建（四五个月一次的节奏）
 *   node scripts/prepare-director.mjs --proxy http://127.0.0.1:10808   # 走代理 clone/pull
 */
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";

const ROOT = path.resolve(import.meta.dirname, "..");
const SRC = path.join(ROOT, ".director-src");
const DEST = path.join(ROOT, "public", "director");
const REPO = "https://github.com/mangfufu/director-desk.git";

const args = process.argv.slice(2);
const opt = (name, dflt) => {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : dflt;
};
const proxy = opt("--proxy", process.env.HTTPS_PROXY || process.env.https_proxy || "");
const gitArgs = [
  "-c",
  "core.autocrlf=false", // 保持 LF：补丁按原样匹配，不受 Windows 行尾转换影响
  ...(proxy ? ["-c", `http.proxy=${proxy}`, "-c", `https.proxy=${proxy}`] : []),
];

const log = (m) => console.log(`  ${m}`);
const run = (cmd, argv, cwd, useShell = false) =>
  execFileSync(cmd, argv, { cwd, stdio: "inherit", shell: useShell });

/* 1) 源码 */
if (!fs.existsSync(path.join(SRC, "package.json"))) {
  log(`克隆导演台源码…${proxy ? `（代理 ${proxy}）` : "（直连，慢的话用 --proxy）"}`);
  run("git", [...gitArgs, "clone", "--depth", "1", REPO, SRC], ROOT);
} else if (args.includes("--update")) {
  log("拉取上游更新…");
  run("git", [...gitArgs, "pull", "--ff-only"], SRC);
} else {
  log("源码已存在（要更新加 --update）");
}

/* 2) 补丁：生产构建也暴露 __director（宿主桥 + agent 操作的前提） */
const mainTs = path.join(SRC, "src", "main.ts");
// 统一成 LF 再匹配：clone 到 Windows 时行尾可能被转过
let src = fs.readFileSync(mainTs, "utf8").replace(/\r\n/g, "\n");
const MARK = "__WORKBENCH_PATCH__";
if (src.includes(MARK)) {
  log("补丁已应用");
} else {
  const FROM = `if (import.meta.env.DEV) {
    if (!document.title.endsWith(' · 开发测试版')) document.title += ' · 开发测试版';
    Object.assign(window, { __director: {`;
  const TO = `// __WORKBENCH_PATCH__：宿主（短剧工作台）需要 __director 接口把 agent 桥过来，
// 原版只在 DEV 暴露；这里去掉条件，生产构建也保留。
{
    Object.assign(window, { __director: {`;
  if (!src.includes(FROM)) {
    console.error(
      "\n补丁锚点没找到：上游 main.ts 结构可能变了。\n" +
        "请检查 src/main.ts 里 __director 的暴露条件，把条件去掉后重新构建。\n",
    );
    process.exit(1);
  }
  src = src.replace(FROM, TO);
  fs.writeFileSync(mainTs, src);
  log("补丁已应用（去掉 __director 的 DEV 条件）");
}

/* 3) 依赖与构建 */
if (!fs.existsSync(path.join(SRC, "node_modules"))) {
  log("安装依赖（首次约 1-2 分钟）…");
  // Windows 上 npm 是 .cmd，必须过 shell
  run("npm", ["install", "--no-audit", "--no-fund"], SRC, true);
}
log("构建（相对 base，便于嵌进宿主）…");
run(process.execPath, ["node_modules/vite/bin/vite.js", "build", "--base=./"], SRC);

/* 4) 部署到 public/director */
fs.rmSync(DEST, { recursive: true, force: true });
fs.cpSync(path.join(SRC, "dist"), DEST, { recursive: true });
const size = (() => {
  let total = 0;
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else total += fs.statSync(p).size;
    }
  };
  walk(DEST);
  return total;
})();
log(`已部署到 public/director（${(size / 1024 / 1024).toFixed(1)} MB）`);
