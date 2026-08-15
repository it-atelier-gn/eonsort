import type { Scene, Surface, Vec3 } from "./model";

export const TEAR = 0.055;
export const NEAR_CLAMP = 0.02;
export const MEDIAN_AT = 0.45;

export interface DepthGrid {
  width: number;
  height: number;
  data: Uint8Array;
}

export function decodeGrid(width: number, height: number, base64: string): DepthGrid {
  const binary = atob(base64);
  const data = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) data[i] = binary.charCodeAt(i);
  return { width, height, data };
}

export function medianOf(data: ArrayLike<number>): number {
  if (data.length === 0) return 0;
  const counts = new Uint32Array(256);
  for (let i = 0; i < data.length; i += 1) counts[data[i]] += 1;

  const half = data.length / 2;
  let seen = 0;
  for (let value = 0; value < 256; value += 1) {
    seen += counts[value];
    if (seen >= half) return value / 255;
  }
  return 1;
}

export function reliefOf(scene: Scene, grid: DepthGrid, strength = 1): Surface | null {
  const { width, height, data } = grid;
  if (width < 2 || height < 2 || data.length < width * height) return null;

  const { vp } = scene.fit;
  const lift = scene.eyeHeight;
  const median = Math.max(medianOf(data), NEAR_CLAMP);
  const constant = scene.depth * MEDIAN_AT * median;
  const blend = Math.min(1, Math.max(0, strength));

  const px = (u: number) => (u - vp.u) * scene.aspect;
  const py = (v: number) => vp.v - v;

  const points: Vec3[] = new Array(width * height);
  const uvs = new Float32Array(width * height * 2);
  const disparity = new Float32Array(width * height);

  for (let row = 0; row < height; row += 1) {
    for (let column = 0; column < width; column += 1) {
      const index = row * width + column;
      const u = width === 1 ? 0.5 : column / (width - 1);
      const v = height === 1 ? 0.5 : row / (height - 1);

      const g = Math.max(data[index] / 255, NEAR_CLAMP);
      disparity[index] = g;

      const relief = constant / g;
      const flat = scene.depth;
      const distance = flat + (relief - flat) * blend;
      const t = distance / scene.focal;

      points[index] = { x: px(u) * t, y: py(v) * t + lift, z: -scene.focal * t };
      uvs[index * 2] = u;
      uvs[index * 2 + 1] = v;
    }
  }

  const keep: number[] = [];
  for (let row = 0; row < height - 1; row += 1) {
    for (let column = 0; column < width - 1; column += 1) {
      const a = row * width + column;
      const b = a + 1;
      const c = a + width;
      const d = c + 1;
      if (!torn(disparity, a, b, c)) keep.push(a, b, c);
      if (!torn(disparity, b, d, c)) keep.push(b, d, c);
    }
  }

  const count = keep.length;
  const position = new Float32Array(count * 3);
  const normal = new Float32Array(count * 3);
  const uv = new Float32Array(count * 2);
  const shade = new Float32Array(count);

  for (let i = 0; i < count; i += 1) {
    const index = keep[i];
    const point = points[index];
    position[i * 3] = point.x;
    position[i * 3 + 1] = point.y;
    position[i * 3 + 2] = point.z;
    normal[i * 3] = 0;
    normal[i * 3 + 1] = 0;
    normal[i * 3 + 2] = 1;
    uv[i * 2] = uvs[index * 2];
    uv[i * 2 + 1] = uvs[index * 2 + 1];
    shade[i] = 1;
  }

  return { position, normal, uv, shade, count };
}

function torn(disparity: Float32Array, a: number, b: number, c: number): boolean {
  const low = Math.min(disparity[a], disparity[b], disparity[c]);
  const high = Math.max(disparity[a], disparity[b], disparity[c]);
  return high - low > TEAR;
}
