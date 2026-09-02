import type { NameCount, Spot, Transform } from "$lib/api";

export interface Box {
  left: string;
  top: string;
  width: string;
  height: string;
}

export const SURE_ENOUGH = 0.6;
export const BIG_ENOUGH = 0.025;

export const BOXES_KEY = "eonsort.faces.boxes";

export function showBoxes(held: unknown): boolean {
  return held !== false;
}

export function sureFaces(
  spots: Spot[] | undefined,
  floor = SURE_ENOUGH,
  smallest = BIG_ENOUGH,
): Spot[] {
  return (spots ?? []).filter(
    (spot) =>
      spot.score >= floor && spot.width >= smallest && spot.height >= smallest,
  );
}

export function countFaces(
  faces: Record<string, Spot[]>,
  source: string,
  floor = SURE_ENOUGH,
): number {
  return sureFaces(faces[source], floor).length;
}

export function lookedAt(
  faces: Record<string, Spot[]>,
  source: string,
): boolean {
  return faces[source] !== undefined;
}

type Corner = [number, number];

const PLACED: Record<Transform, (at: Corner) => Corner> = {
  none: ([x, y]) => [x, y],
  rotate90: ([x, y]) => [y, 1 - x],
  rotate180: ([x, y]) => [1 - x, 1 - y],
  rotate270: ([x, y]) => [1 - y, x],
  flip_h: ([x, y]) => [1 - x, y],
  flip_v: ([x, y]) => [x, 1 - y],
  transpose: ([x, y]) => [1 - y, 1 - x],
  transverse: ([x, y]) => [y, x],
};

export function laidOut(spot: Spot, transform: Transform = "none"): Spot {
  const put = PLACED[transform];
  const [x0, y0] = put([spot.x, spot.y]);
  const [x1, y1] = put([spot.x + spot.width, spot.y + spot.height]);
  return {
    ...spot,
    x: Math.min(x0, x1),
    y: Math.min(y0, y1),
    width: Math.abs(x1 - x0),
    height: Math.abs(y1 - y0),
  };
}

export function boxOf(spot: Spot, transform: Transform = "none"): Box {
  const laid = laidOut(spot, transform);
  const held = (value: number) => Math.min(Math.max(value, 0), 1);
  const left = held(laid.x);
  const top = held(laid.y);
  const right = held(laid.x + laid.width);
  const bottom = held(laid.y + laid.height);
  const share = (value: number) => `${Math.round(value * 1e6) / 1e4}%`;
  return {
    left: share(left),
    top: share(top),
    width: share(Math.max(right - left, 0)),
    height: share(Math.max(bottom - top, 0)),
  };
}

export function withFaces(
  sources: string[],
  faces: Record<string, Spot[]>,
  floor = SURE_ENOUGH,
): string[] {
  return sources.filter((source) => countFaces(faces, source, floor) > 0);
}

export function tally(
  faces: Record<string, Spot[]>,
  floor = SURE_ENOUGH,
): number {
  return Object.values(faces).reduce(
    (sum, spots) => sum + sureFaces(spots, floor).length,
    0,
  );
}

export function named(
  spots: Spot[] | undefined,
  floor = SURE_ENOUGH,
): string[] {
  const held = new Set<string>();
  for (const spot of sureFaces(spots, floor)) {
    if (spot.label) held.add(spot.label);
  }
  return [...held].sort();
}

export function pickable(
  names: NameCount[] | undefined,
  current?: string | null,
): NameCount[] {
  const held = new Map<string, number>();
  for (const person of names ?? []) {
    const name = person.name.trim();
    if (name === "" || name === current) continue;
    held.set(name, Math.max(held.get(name) ?? 0, person.count));
  }
  return [...held]
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
}

export function withName(
  picked: string[] | null,
  name: string,
): string[] | null {
  const held = picked ?? [];
  const next = held.includes(name)
    ? held.filter((one) => one !== name)
    : [...held, name];
  return next.length === 0 ? null : next;
}

export function anybody(picked: string[] | null): boolean {
  return (picked ?? []).length === 0;
}

export function wearingAll(
  faces: Record<string, Spot[]>,
  source: string,
  picked: string[] | null,
  floor = SURE_ENOUGH,
): boolean {
  const wanted = picked ?? [];
  if (wanted.length === 0) return true;
  const there = new Set(named(faces[source], floor));
  return wanted.every((name) => there.has(name));
}

export function whoLabel(picked: string[] | null): string {
  const wanted = picked ?? [];
  if (wanted.length === 0) return "anybody";
  if (wanted.length === 1) return wanted[0];
  return `${wanted.length} people`;
}

export function matchingNames(
  names: NameCount[],
  needle: string,
): NameCount[] {
  const wanted = needle.trim().toLowerCase();
  if (wanted === "") return names;
  return names.filter((person) => person.name.toLowerCase().includes(wanted));
}
