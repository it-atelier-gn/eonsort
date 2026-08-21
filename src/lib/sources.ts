import type { Provider } from "$lib/api";

export interface Source {
  id: Provider;
  label: string;
  hint: string;
  weight: number;
}

export const SOURCES: Source[] = [
  { id: "filename", label: "File name", hint: "Dates written into the name", weight: 30 },
  { id: "exif", label: "EXIF", hint: "Camera metadata in photos", weight: 40 },
  { id: "media", label: "Media", hint: "Recording time in videos", weight: 40 },
  {
    id: "xmp",
    label: "XMP sidecar",
    hint: "The .xmp a raw developer writes beside a picture",
    weight: 42,
  },
  {
    id: "takeout",
    label: "Google Takeout",
    hint: "The JSON sidecar an export leaves behind",
    weight: 38,
  },
  {
    id: "system",
    label: "System properties",
    hint: "Windows Details and macOS Spotlight, for files nothing else could date",
    weight: 36,
  },
  { id: "filesystem", label: "File system", hint: "Created / modified time", weight: 10 },
];

export const MIN_WEIGHT = 0;
export const MAX_WEIGHT = 100;

export type Weights = Partial<Record<Provider, number>>;

export function sourceOf(id: Provider): Source {
  return SOURCES.find((source) => source.id === id) ?? SOURCES[0];
}

export function isProvider(value: unknown): value is Provider {
  return typeof value === "string" && SOURCES.some((source) => source.id === value);
}

export function listing(enabled: Provider[]): Provider[] {
  const seen: Provider[] = [];
  for (const id of enabled) {
    if (isProvider(id) && !seen.includes(id)) seen.push(id);
  }
  for (const source of SOURCES) {
    if (!seen.includes(source.id)) seen.push(source.id);
  }
  return seen;
}

export function moveSource(order: Provider[], from: Provider, to: Provider): Provider[] {
  const clean = listing(order);
  const source = clean.indexOf(from);
  const target = clean.indexOf(to);
  if (source < 0 || target < 0 || source === target) return clean;

  const next = [...clean];
  next.splice(source, 1);
  next.splice(target, 0, from);
  return next;
}

export function moveBy(order: Provider[], id: Provider, by: number): Provider[] {
  const clean = listing(order);
  const target = clean[clean.indexOf(id) + by];
  return target === undefined ? clean : moveSource(clean, id, target);
}

export interface Span {
  top: number;
  bottom: number;
}

export function rowAt(rows: Span[], y: number): number {
  if (rows.length === 0) return -1;
  const under = rows.findIndex((row) => y >= row.top && y <= row.bottom);
  if (under >= 0) return under;
  return y < rows[0].top ? 0 : y > rows[rows.length - 1].bottom ? rows.length - 1 : -1;
}

export function enabledIn(order: Provider[], enabled: Provider[]): Provider[] {
  return listing(order).filter((id) => enabled.includes(id));
}

export function toggled(enabled: Provider[], order: Provider[], id: Provider): Provider[] {
  const next = enabled.includes(id) ? enabled.filter((p) => p !== id) : [...enabled, id];
  return enabledIn(order, next);
}

export function clampWeight(weight: number): number {
  return Math.round(Math.min(MAX_WEIGHT, Math.max(MIN_WEIGHT, weight)));
}

export function weightOf(weights: Weights, id: Provider): number {
  const held = weights[id];
  return held === undefined || !Number.isFinite(held) ? sourceOf(id).weight : clampWeight(held);
}

export function atDefaults(weights: Weights): boolean {
  return SOURCES.every((source) => weightOf(weights, source.id) === source.weight);
}
