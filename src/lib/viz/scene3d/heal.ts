import { TEAR, type DepthGrid } from "./depth";

export const BAND = 12;

export interface Heal {
  source: Int32Array;
  mask: Uint8Array;
  healed: DepthGrid;
  filled: number;
}

export function healDepth(grid: DepthGrid, band = BAND): Heal {
  const { width, height, data } = grid;
  const count = width * height;

  const source = new Int32Array(count);
  const mask = new Uint8Array(count);
  for (let i = 0; i < count; i += 1) source[i] = i;

  if (width < 2 || height < 2 || data.length < count || band < 1) {
    return { source, mask, healed: { width, height, data: data.slice(0, count) }, filled: 0 };
  }

  const step = new Int32Array(count).fill(-1);
  const queue = new Int32Array(count);
  let head = 0;
  let tail = 0;

  const nearer = (a: number, b: number) => (data[a] - data[b]) / 255 > TEAR;

  for (let row = 0; row < height; row += 1) {
    for (let column = 0; column < width; column += 1) {
      const here = row * width + column;
      for (const other of around(here, row, column, width, height)) {
        if (nearer(here, other) && step[other] !== 0) {
          step[other] = 0;
          queue[tail] = other;
          tail += 1;
        }
      }
    }
  }

  let filled = 0;
  while (head < tail) {
    const here = queue[head];
    head += 1;
    if (step[here] >= band) continue;

    const row = Math.floor(here / width);
    const column = here - row * width;
    const behind = source[here];

    for (const other of around(here, row, column, width, height)) {
      if (step[other] !== -1 || !nearer(other, behind)) continue;
      step[other] = step[here] + 1;
      source[other] = behind;
      mask[other] = 255;
      queue[tail] = other;
      tail += 1;
      filled += 1;
    }
  }

  const healedData = new Uint8Array(count);
  for (let i = 0; i < count; i += 1) healedData[i] = data[source[i]];

  return { source, mask, healed: { width, height, data: healedData }, filled };
}

export function healPixels<T extends Uint8ClampedArray | Uint8Array>(
  pixels: T,
  source: Int32Array,
  channels = 4,
): T {
  const out = pixels.slice(0) as T;
  for (let i = 0; i < source.length; i += 1) {
    const from = source[i];
    if (from === i) continue;
    for (let channel = 0; channel < channels; channel += 1) {
      out[i * channels + channel] = pixels[from * channels + channel];
    }
  }
  return out;
}

function around(
  index: number,
  row: number,
  column: number,
  width: number,
  height: number,
): number[] {
  const found: number[] = [];
  if (column > 0) found.push(index - 1);
  if (column < width - 1) found.push(index + 1);
  if (row > 0) found.push(index - width);
  if (row < height - 1) found.push(index + width);
  return found;
}
