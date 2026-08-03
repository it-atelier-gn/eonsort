import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";

const SIZE = 1024;
const OUT = process.argv[2] ?? "src-tauri/icons/source.png";

const BACKDROP_TOP = [0x39, 0xc5, 0xf3];
const BACKDROP_BOTTOM = [0x1b, 0x7f, 0xa0];
const GLYPH = [0x0d, 0x11, 0x17];
const CORNER = SIZE * 0.22;

const BARS = [
  { y: 0.34, width: 0.56 },
  { y: 0.47, width: 0.42 },
  { y: 0.6, width: 0.28 },
];
const BAR_HEIGHT = 0.075;
const BAR_LEFT = 0.22;

function insideRoundedSquare(x, y) {
  const near = (v) => Math.min(v, SIZE - 1 - v);
  const dx = CORNER - near(x);
  const dy = CORNER - near(y);
  if (dx <= 0 || dy <= 0) return true;
  return dx * dx + dy * dy <= CORNER * CORNER;
}

function insideGlyph(x, y) {
  return BARS.some((bar) => {
    const top = bar.y * SIZE;
    const left = BAR_LEFT * SIZE;
    return (
      y >= top &&
      y < top + BAR_HEIGHT * SIZE &&
      x >= left &&
      x < left + bar.width * SIZE
    );
  });
}

function pixels() {
  const rows = [];
  for (let y = 0; y < SIZE; y += 1) {
    const row = Buffer.alloc(SIZE * 4 + 1);
    const mix = y / (SIZE - 1);
    const backdrop = BACKDROP_TOP.map((c, i) =>
      Math.round(c + (BACKDROP_BOTTOM[i] - c) * mix),
    );
    for (let x = 0; x < SIZE; x += 1) {
      const at = 1 + x * 4;
      if (!insideRoundedSquare(x, y)) {
        row.writeUInt32BE(0, at);
        continue;
      }
      const colour = insideGlyph(x, y) ? GLYPH : backdrop;
      row[at] = colour[0];
      row[at + 1] = colour[1];
      row[at + 2] = colour[2];
      row[at + 3] = 0xff;
    }
    rows.push(row);
  }
  return Buffer.concat(rows);
}

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k += 1) {
    c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  }
  return c >>> 0;
});

function crc32(buffer) {
  let c = 0xffffffff;
  for (const byte of buffer) {
    c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

const header = Buffer.alloc(13);
header.writeUInt32BE(SIZE, 0);
header.writeUInt32BE(SIZE, 4);
header[8] = 8;
header[9] = 6;

writeFileSync(
  OUT,
  Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(pixels(), { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]),
);

console.log(`wrote ${OUT}`);
