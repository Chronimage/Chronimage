#!/usr/bin/env node
/*
 * Generates a minimal 512×512 solid-color PNG as the source icon.
 * Real branding comes later — this unblocks `cargo check` + `tauri icon`.
 */

const fs = require('node:fs');
const zlib = require('node:zlib');

const size = 512;
const accentRGB = [102, 217, 160]; // oklch(0.88 0.18 150) approx → hex #66D9A0

// Build raw RGBA buffer
const rgba = Buffer.alloc(size * size * 4);
for (let i = 0; i < size * size; i++) {
  rgba[i * 4] = accentRGB[0];
  rgba[i * 4 + 1] = accentRGB[1];
  rgba[i * 4 + 2] = accentRGB[2];
  rgba[i * 4 + 3] = 255;
}

// Build PNG scanlines (filter byte + row)
const stride = size * 4;
const scanlines = Buffer.alloc((stride + 1) * size);
for (let y = 0; y < size; y++) {
  scanlines[(stride + 1) * y] = 0; // filter: None
  rgba.copy(scanlines, (stride + 1) * y + 1, stride * y, stride * (y + 1));
}

// CRC32 per PNG spec
const crcTable = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();
function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = (c >>> 8) ^ crcTable[(c ^ buf[i]) & 0xff];
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const lenBuf = Buffer.alloc(4);
  lenBuf.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, 'ascii');
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([lenBuf, typeBuf, data, crcBuf]);
}

const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(size, 0);
ihdr.writeUInt32BE(size, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // color type (RGBA)
ihdr[10] = 0;
ihdr[11] = 0;
ihdr[12] = 0;
const idat = zlib.deflateSync(scanlines);
const png = Buffer.concat([
  signature,
  chunk('IHDR', ihdr),
  chunk('IDAT', idat),
  chunk('IEND', Buffer.alloc(0)),
]);

fs.writeFileSync('app-icon.png', png);
console.log('Wrote app-icon.png (512×512, solid mint) — run `pnpm exec tauri icon` to regenerate all sizes.');
