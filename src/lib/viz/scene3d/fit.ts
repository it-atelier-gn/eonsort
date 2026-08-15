import type { Transform } from "$lib/api";
import { swapsAxes } from "$lib/rotate";

export interface Point {
  u: number;
  v: number;
}

export interface Rect {
  u0: number;
  v0: number;
  u1: number;
  v1: number;
}

export interface FitObject {
  label: string;
  u0: number;
  v0: number;
  u1: number;
  v1: number;
}

export interface SceneFit {
  vp: Point;
  rect: Rect;
  focal: number;
  objects: FitObject[];
}

export type Corner = "tl" | "tr" | "bl" | "br";

export const DEFAULT_INSET = 0.42;
export const FOCAL_DEFAULT = 1.35;
export const FOCAL_MIN = 0.6;
export const FOCAL_MAX = 3.2;
export const MARGIN = 0.06;
export const MIN_GAP = 0.03;
export const EYE_HEIGHT = 1.7;
export const MAX_OBJECTS = 6;

const PHOTO = new Set([
  "jpg",
  "jpeg",
  "jpe",
  "png",
  "webp",
  "bmp",
  "tif",
  "tiff",
  "avif",
  "gif",
]);

export function clamp(value: number, low: number, high: number): number {
  return value < low ? low : value > high ? high : value;
}

function finite(value: number | undefined, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

export function clampFit(fit: SceneFit): SceneFit {
  const u = clamp(finite(fit?.vp?.u, 0.5), MARGIN, 1 - MARGIN);
  const v = clamp(finite(fit?.vp?.v, 0.5), MARGIN, 1 - MARGIN);

  let u0 = clamp(finite(fit?.rect?.u0, 0), 0, 1);
  let u1 = clamp(finite(fit?.rect?.u1, 1), 0, 1);
  let v0 = clamp(finite(fit?.rect?.v0, 0), 0, 1);
  let v1 = clamp(finite(fit?.rect?.v1, 1), 0, 1);

  if (u0 > u1) [u0, u1] = [u1, u0];
  if (v0 > v1) [v0, v1] = [v1, v0];

  return {
    vp: { u, v },
    rect: {
      u0: Math.min(u0, u - MIN_GAP),
      v0: Math.min(v0, v - MIN_GAP),
      u1: Math.max(u1, u + MIN_GAP),
      v1: Math.max(v1, v + MIN_GAP),
    },
    focal: clamp(finite(fit?.focal, FOCAL_DEFAULT), FOCAL_MIN, FOCAL_MAX),
    objects: cleanObjects(fit?.objects),
  };
}

export function cleanObjects(objects: FitObject[] | undefined): FitObject[] {
  if (!Array.isArray(objects)) return [];
  const out: FitObject[] = [];
  for (const object of objects) {
    const label = typeof object?.label === "string" ? object.label.trim().toLowerCase() : "";
    if (label.length === 0) continue;

    let u0 = clamp(finite(object?.u0, Number.NaN), 0, 1);
    let u1 = clamp(finite(object?.u1, Number.NaN), 0, 1);
    let v0 = clamp(finite(object?.v0, Number.NaN), 0, 1);
    let v1 = clamp(finite(object?.v1, Number.NaN), 0, 1);
    if (![u0, u1, v0, v1].every(Number.isFinite)) continue;
    if (u0 > u1) [u0, u1] = [u1, u0];
    if (v0 > v1) [v0, v1] = [v1, v0];

    const area = (u1 - u0) * (v1 - v0);
    if (area <= 0.0004 || area > 0.9) continue;

    out.push({ label, u0, v0, u1, v1 });
    if (out.length >= MAX_OBJECTS) break;
  }
  return out;
}

export function fitAround(
  vp: Point,
  inset = DEFAULT_INSET,
  focal = FOCAL_DEFAULT,
  objects: FitObject[] = [],
): SceneFit {
  const s = clamp(finite(inset, DEFAULT_INSET), 0.04, 0.96);
  const u = clamp(finite(vp?.u, 0.5), MARGIN, 1 - MARGIN);
  const v = clamp(finite(vp?.v, 0.5), MARGIN, 1 - MARGIN);

  return clampFit({
    vp: { u, v },
    rect: {
      u0: u * (1 - s),
      v0: v * (1 - s),
      u1: u + s * (1 - u),
      v1: v + s * (1 - v),
    },
    focal,
    objects,
  });
}

export function defaultFit(): SceneFit {
  return fitAround({ u: 0.5, v: 0.5 }, DEFAULT_INSET);
}

export function flatFit(fit: SceneFit): SceneFit {
  const base = clampFit(fit);
  return clampFit({ ...base, rect: { u0: 0, v0: 0, u1: 1, v1: 1 } });
}

export function moveVp(fit: SceneFit, u: number, v: number): SceneFit {
  return clampFit({ ...clampFit(fit), vp: { u, v } });
}

export function moveCorner(fit: SceneFit, corner: Corner, u: number, v: number): SceneFit {
  const base = clampFit(fit);
  const rect = { ...base.rect };

  if (corner === "tl" || corner === "bl") {
    rect.u0 = clamp(finite(u, rect.u0), 0, base.vp.u - MIN_GAP);
  } else {
    rect.u1 = clamp(finite(u, rect.u1), base.vp.u + MIN_GAP, 1);
  }

  if (corner === "tl" || corner === "tr") {
    rect.v0 = clamp(finite(v, rect.v0), 0, base.vp.v - MIN_GAP);
  } else {
    rect.v1 = clamp(finite(v, rect.v1), base.vp.v + MIN_GAP, 1);
  }

  return clampFit({ ...base, rect });
}

export function setFocal(fit: SceneFit, focal: number): SceneFit {
  return clampFit({ ...clampFit(fit), focal });
}

export function isPhoto(name: string): boolean {
  const dot = name.lastIndexOf(".");
  if (dot < 0) return false;
  return PHOTO.has(name.slice(dot + 1).toLowerCase());
}

export function photoAspect(width: number, height: number, transform: Transform): number {
  const turned = swapsAxes(transform);
  const w = turned ? height : width;
  const h = turned ? width : height;
  return h > 0 && Number.isFinite(w / h) ? w / h : 1;
}
