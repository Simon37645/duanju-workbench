// 生成应用图标源图（1024x1024 PNG），不依赖任何第三方库。
// 用法：node scripts/gen-icon.mjs
// 之后跑 `pnpm tauri icon src-tauri/icons/source.png` 生成各平台图标。

import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SIZE = 1024;
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/* ---------------------------------------------------------------- CRC32 */
const crcTable = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td), 0);
  return Buffer.concat([len, td, crc]);
}

/* ---------------------------------------------------------------- 画图 */
const px = Buffer.alloc(SIZE * SIZE * 4);

const setPx = (x, y, [r, g, b, a = 255]) => {
  if (x < 0 || y < 0 || x >= SIZE || y >= SIZE) return;
  const i = (y * SIZE + x) * 4;
  const ia = a / 255;
  px[i] = Math.round(px[i] * (1 - ia) + r * ia);
  px[i + 1] = Math.round(px[i + 1] * (1 - ia) + g * ia);
  px[i + 2] = Math.round(px[i + 2] * (1 - ia) + b * ia);
  px[i + 3] = 255;
};

// 圆角矩形（带抗锯齿的边缘采样）
function roundRect(x0, y0, x1, y1, radius, colorFn) {
  for (let y = y0; y < y1; y++) {
    for (let x = x0; x < x1; x++) {
      const dx = Math.max(x0 + radius - x, 0, x - (x1 - radius - 1));
      const dy = Math.max(y0 + radius - y, 0, y - (y1 - radius - 1));
      const d = Math.hypot(dx, dy);
      if (d > radius) continue;
      const alpha = d > radius - 1.5 ? Math.round((1 - (d - (radius - 1.5)) / 1.5) * 255) : 255;
      const c = colorFn(x, y);
      setPx(x, y, [c[0], c[1], c[2], Math.min(alpha, c[3] ?? 255)]);
    }
  }
}

// 背景：深色竖向渐变
for (let y = 0; y < SIZE; y++) {
  for (let x = 0; x < SIZE; x++) {
    const t = y / SIZE;
    setPx(x, y, [
      Math.round(22 + 10 * t),
      Math.round(22 + 10 * t),
      Math.round(30 + 12 * t),
    ]);
  }
}

// 主体：琥珀色胶片格
roundRect(150, 150, 874, 874, 160, (x) => {
  const t = (x - 150) / 724;
  return [Math.round(240 - 32 * t), Math.round(176 - 34 * t), Math.round(64 + 24 * t)];
});

// 左侧三个圆角孔（胶片齿孔）
for (let i = 0; i < 3; i++) {
  const cy = 320 + i * 192;
  roundRect(214, cy - 52, 302, cy + 52, 44, () => [26, 26, 34]);
}

// 右侧播放三角
const tri = (x, y) => {
  const left = 430, right = 800, top = 330, bottom = 694;
  if (x < left || x > right || y < top || y > bottom) return false;
  const t = (x - left) / (right - left);
  const half = ((bottom - top) / 2) * t;
  const mid = (top + bottom) / 2;
  return y > mid - half && y < mid + half;
};

for (let y = 330; y <= 694; y++) {
  for (let x = 430; x <= 800; x++) {
    if (!tri(x, y)) continue;
    // 边缘抗锯齿：检查邻居
    const edge =
      !tri(x - 2, y) || !tri(x + 2, y) || !tri(x, y - 2) || !tri(x, y + 2);
    setPx(x, y, [26, 26, 34, edge ? 140 : 255]);
  }
}

/* ------------------------------------------------------------ 编码 PNG */
const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0; // filter: none
  px.copy(raw, y * (SIZE * 4 + 1) + 1, y * SIZE * 4, (y + 1) * SIZE * 4);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA
ihdr[10] = 0;
ihdr[11] = 0;
ihdr[12] = 0;

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

const out = resolve(root, "src-tauri/icons/source.png");
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, png);
console.log(`图标源图已生成: ${out} (${(png.length / 1024).toFixed(1)} KB)`);
