import type { EntryView } from "$lib/api";

export const TILE_WIDTH = 0.62;
export const TILE_HEIGHT = 0.44;
export const TILE_GAP = 1.14;
export const RING_GAP = 1.05;
export const MAX_GAP = 6;
export const GAP_SHARE = 0.11;
export const MIN_RADIUS = 2.2;
export const MAX_RADIUS = 46;
export const RING_CAPACITY = Math.floor((MAX_RADIUS * Math.PI * 2) / (TILE_WIDTH * TILE_GAP));
export const LABEL_MARGIN = 0.9;
export const MIN_PITCH = 0.12;
export const LEVEL_PITCH = Math.PI / 2;
export const FLY_TURN = 0.12;
export const TURN_RADIUS = 6;
export const ZOOM_RATE = 0.0016;
export const ZOOM_SHARE = 0.5;
export const MIN_STEP = 0.04;
export const MIN_FLY = 0.02;
export const MAX_FLY = 2;
export const MAX_PITCH = Math.PI - 0.12;

export type RingMode = "rings" | "spiral";
export const RING_MODES: RingMode[] = ["rings", "spiral"];

export const MODE_LABEL: Record<RingMode, string> = {
  rings: "Rings",
  spiral: "Spiral",
};

export const MONTH_NAMES = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

export const MONTH_COLOURS: [number, number, number][] = MONTH_NAMES.map((_, month) =>
  hueColour((month / 12) * 360),
);

export function monthCss(month: number): string {
  const [r, g, b] = MONTH_COLOURS[wrapMonth(month)];
  return `rgb(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)})`;
}

export interface Tile {
  entry: number;
  year: number;
  month: number;
  ring: number;
  angle: number;
  radius: number;
  y: number;
  coil: number;
}

export interface Ring {
  year: number;
  count: number;
  shown: number;
  radius: number;
  y: number;
}

export interface Rings {
  tiles: Tile[];
  rings: Ring[];
  gap: number;
  height: number;
  radius: number;
  files: number;
}

export const EMPTY_RINGS: Rings = {
  tiles: [],
  rings: [],
  gap: RING_GAP,
  height: 0,
  radius: MIN_RADIUS,
  files: 0,
};

export function yearOf(epoch: number): number {
  return new Date(epoch * 1000).getUTCFullYear();
}

export function monthOf(epoch: number): number {
  return new Date(epoch * 1000).getUTCMonth();
}

export function radiusFor(count: number): number {
  const wanted = (Math.min(count, RING_CAPACITY) * TILE_WIDTH * TILE_GAP) / (Math.PI * 2);
  return Math.min(MAX_RADIUS, Math.max(MIN_RADIUS, wanted));
}

export function gapFor(radius: number): number {
  return Math.min(MAX_GAP, Math.max(RING_GAP, radius * GAP_SHARE));
}

export function thinned(members: number[]): number[] {
  if (members.length <= RING_CAPACITY) return members;

  const kept: number[] = [];
  for (let index = 0; index < RING_CAPACITY; index += 1) {
    kept.push(members[Math.floor((index * members.length) / RING_CAPACITY)]);
  }
  return kept;
}

export function buildRings(entries: EntryView[]): Rings {
  if (entries.length === 0) return EMPTY_RINGS;

  const years = new Map<number, number[]>();
  entries.forEach((entry, index) => {
    const year = yearOf(entry.taken_epoch);
    const bucket = years.get(year);
    if (bucket) bucket.push(index);
    else years.set(year, [index]);
  });

  const ordered = [...years.keys()].sort((a, b) => a - b);
  const rings: Ring[] = [];
  const tiles: Tile[] = [];
  const gap = gapFor(
    ordered.reduce((most, year) => Math.max(most, radiusFor(years.get(year)!.length)), MIN_RADIUS),
  );

  ordered.forEach((year, ring) => {
    const found = years
      .get(year)!
      .sort((a, b) => entries[a].taken_epoch - entries[b].taken_epoch);
    const members = thinned(found);
    const radius = radiusFor(found.length);
    const y = ring * gap;

    rings.push({ year, count: found.length, shown: members.length, radius, y });

    members.forEach((entry, index) => {
      const turn = index / members.length;
      tiles.push({
        entry,
        year,
        month: monthOf(entries[entry].taken_epoch),
        ring,
        angle: turn * Math.PI * 2,
        radius,
        y,
        coil: y + turn * gap,
      });
    });
  });

  return {
    tiles,
    rings,
    gap,
    height: Math.max(0, (rings.length - 1) * gap),
    radius: rings.reduce((most, ring) => Math.max(most, ring.radius), MIN_RADIUS),
    files: entries.length,
  };
}

export function zoomedRadius(
  radius: number,
  amount: number,
  band: number,
  min: number,
  max: number,
): number {
  if (!Number.isFinite(amount) || amount === 0) return Math.min(max, Math.max(min, radius));

  const raw = radius * (Math.exp(amount * ZOOM_RATE) - 1);
  const room = Math.abs(radius - band) * ZOOM_SHARE;
  const step = Math.min(Math.abs(raw), Math.max(MIN_STEP, room));
  const next = radius + (raw < 0 ? -step : step);
  return Math.min(max, Math.max(min, next));
}

export function clampFly(speed: number): number {
  if (!Number.isFinite(speed)) return 1;
  return Math.min(MAX_FLY, Math.max(MIN_FLY, speed));
}

export function turnRate(speed: number, radius = TURN_RADIUS): number {
  const wide = Math.max(TURN_RADIUS, Number.isFinite(radius) ? radius : TURN_RADIUS);
  return (FLY_TURN * clampFly(speed) * TURN_RADIUS) / wide;
}

export function flownTheta(
  theta: number,
  seconds: number,
  speed: number,
  radius = TURN_RADIUS,
): number {
  const turned = theta + turnRate(speed, radius) * Math.max(0, seconds);
  const two = Math.PI * 2;
  return ((turned % two) + two) % two;
}

export function heightRange(rings: Rings): { min: number; max: number } {
  return { min: -rings.gap, max: rings.height + rings.gap };
}

export function clampHeight(rings: Rings, y: number): number {
  const { min, max } = heightRange(rings);
  if (!Number.isFinite(y)) return min;
  return Math.min(max, Math.max(min, y));
}

export function pitchAt(share: number): number {
  const t = share < 0 ? 0 : share > 1 ? 1 : share;
  return MIN_PITCH + (MAX_PITCH - MIN_PITCH) * t;
}

export function pitchShare(phi: number): number {
  const t = (phi - MIN_PITCH) / (MAX_PITCH - MIN_PITCH);
  return t < 0 ? 0 : t > 1 ? 1 : t;
}

export function heightOf(tile: Tile, coiling: number): number {
  return tile.y + (tile.coil - tile.y) * clamp01(coiling);
}

export function placeOf(tile: Tile, coiling = 0): [number, number, number] {
  return [
    Math.sin(tile.angle) * tile.radius,
    heightOf(tile, coiling),
    Math.cos(tile.angle) * tile.radius,
  ];
}

export function labelAt(ring: Ring, theta: number): [number, number, number] {
  const reach = ring.radius + LABEL_MARGIN;
  return [Math.sin(theta) * reach, ring.y, Math.cos(theta) * reach];
}

export function nearestTiles(
  rings: Rings,
  eye: [number, number, number],
  limit: number,
  coiling = 0,
): Tile[] {
  const scored = rings.tiles.map((tile) => {
    const [x, y, z] = placeOf(tile, coiling);
    return { tile, distance: Math.hypot(x - eye[0], y - eye[1], z - eye[2]) };
  });
  scored.sort((a, b) => a.distance - b.distance);
  return scored.slice(0, Math.max(0, limit)).map((found) => found.tile);
}

function clamp01(value: number): number {
  return value < 0 ? 0 : value > 1 ? 1 : value;
}

function wrapMonth(month: number): number {
  return ((month % 12) + 12) % 12;
}

function hueColour(hue: number): [number, number, number] {
  const saturation = 0.62;
  const lightness = 0.58;
  const c = (1 - Math.abs(2 * lightness - 1)) * saturation;
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = lightness - c / 2;

  const sector = Math.floor(hue / 60) % 6;
  const rgb: [number, number, number] =
    sector === 0
      ? [c, x, 0]
      : sector === 1
        ? [x, c, 0]
        : sector === 2
          ? [0, c, x]
          : sector === 3
            ? [0, x, c]
            : sector === 4
              ? [x, 0, c]
              : [c, 0, x];

  return [rgb[0] + m, rgb[1] + m, rgb[2] + m];
}
