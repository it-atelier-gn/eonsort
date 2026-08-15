export const FILL_SIDE = 1024;
export const FILL_SIZES = ["512x512", "1024x1024"];

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function sideOf(size: string, fallback = FILL_SIDE): number {
  const first = /(\d+)/.exec(size ?? "");
  if (!first) return fallback;
  const side = Number(first[1]);
  if (!Number.isFinite(side) || side < 64) return fallback;
  return Math.min(4096, Math.round(side));
}

export function letterbox(width: number, height: number, side: number): Box {
  if (
    !Number.isFinite(width) ||
    !Number.isFinite(height) ||
    !Number.isFinite(side) ||
    width <= 0 ||
    height <= 0 ||
    side <= 0
  ) {
    return { x: 0, y: 0, width: 0, height: 0 };
  }

  const scale = Math.min(side / width, side / height);
  const box = {
    width: Math.max(1, Math.min(side, Math.round(width * scale))),
    height: Math.max(1, Math.min(side, Math.round(height * scale))),
    x: 0,
    y: 0,
  };
  box.x = Math.floor((side - box.width) / 2);
  box.y = Math.floor((side - box.height) / 2);
  return box;
}

export function bandImage(mask: Uint8Array): Uint8ClampedArray<ArrayBuffer> {
  const pixels = new Uint8ClampedArray(mask.length * 4);
  for (let i = 0; i < mask.length; i += 1) {
    if (mask[i] === 0) continue;
    pixels[i * 4] = 255;
    pixels[i * 4 + 1] = 255;
    pixels[i * 4 + 2] = 255;
    pixels[i * 4 + 3] = 255;
  }
  return pixels;
}

export function applyFill(
  base: Uint8ClampedArray<ArrayBufferLike>,
  filled: Uint8ClampedArray<ArrayBufferLike>,
  mask: Uint8Array,
): Uint8ClampedArray<ArrayBuffer> {
  const out = new Uint8ClampedArray(base);
  for (let i = 0; i < mask.length; i += 1) {
    if (mask[i] === 0) continue;
    if ((i + 1) * 4 > filled.length || (i + 1) * 4 > out.length) break;
    out[i * 4] = filled[i * 4];
    out[i * 4 + 1] = filled[i * 4 + 1];
    out[i * 4 + 2] = filled[i * 4 + 2];
    out[i * 4 + 3] = 255;
  }
  return out;
}
