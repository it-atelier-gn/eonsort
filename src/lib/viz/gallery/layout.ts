import type { EntryView } from "$lib/api";

export const ROOM_WIDTH = 15;
export const ROOM_HEIGHT = 6.5;
export const WALL = 0.45;
export const DOOR_WIDTH = 3.6;
export const DOOR_HEIGHT = 4;
export const ART_CENTRE = 2.15;
export const ART_PITCH = 2.9;
export const ART_MARGIN = 2.6;
export const MIN_DEPTH = 12;
export const MAX_DEPTH = 90;
export const EYE_HEIGHT = 1.7;
export const CLERESTORY_BASE = 4.5;
export const CLERESTORY_HEIGHT = 1.4;
export const MAX_ROOMS = 24;

export interface Frame {
  entry: number;
  x: number;
  y: number;
  z: number;
  facing: -1 | 1;
  width: number;
  height: number;
}

export interface Pane {
  x: number;
  y: number;
  z: number;
  facing: -1 | 1;
  width: number;
  height: number;
}

export type PieceKind = "bench" | "plinth" | "planter";

export interface Piece {
  kind: PieceKind;
  x: number;
  z: number;
  width: number;
  height: number;
  depth: number;
}

export interface Solid {
  x0: number;
  x1: number;
  z0: number;
  z1: number;
}

export interface Room {
  key: string;
  label: string;
  files: number;
  hung: number;
  z0: number;
  z1: number;
  frames: Frame[];
  panes: Pane[];
  furniture: Piece[];
}

export interface Gallery {
  rooms: Room[];
  frames: Frame[];
  solids: Solid[];
  depth: number;
  files: number;
}

export const EMPTY_GALLERY: Gallery = {
  rooms: [],
  frames: [],
  solids: [],
  depth: 0,
  files: 0,
};

export function periodOf(entry: EntryView): string {
  return String(new Date(entry.taken_epoch * 1000).getUTCFullYear());
}

export function groupIntoPeriods(entries: EntryView[]): { label: string; entries: number[] }[] {
  const byPeriod = new Map<string, number[]>();
  entries.forEach((entry, index) => {
    const key = periodOf(entry);
    const bucket = byPeriod.get(key);
    if (bucket) bucket.push(index);
    else byPeriod.set(key, [index]);
  });

  return [...byPeriod.entries()]
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([label, indexes]) => ({
      label,
      entries: indexes.sort((a, b) => entries[a].taken_epoch - entries[b].taken_epoch),
    }));
}

export function roomDepth(count: number): number {
  const perWall = Math.ceil(count / 2);
  const wanted = perWall * ART_PITCH + ART_MARGIN;
  return Math.min(MAX_DEPTH, Math.max(MIN_DEPTH, wanted));
}

export function hangingSlots(depth: number): number {
  return Math.max(1, Math.floor((depth - ART_MARGIN) / ART_PITCH)) * 2;
}

export function thinTo<T>(items: T[], limit: number): T[] {
  if (items.length <= limit) return items;
  const kept: T[] = [];
  for (let i = 0; i < limit; i += 1) {
    kept.push(items[Math.floor((i * items.length) / limit)]);
  }
  return kept;
}

export function buildGallery(entries: EntryView[]): Gallery {
  const periods = thinTo(groupIntoPeriods(entries), MAX_ROOMS);
  if (periods.length === 0) return EMPTY_GALLERY;

  const rooms: Room[] = [];
  const solids: Solid[] = [];
  const half = ROOM_WIDTH / 2;
  let cursor = 0;

  for (const period of periods) {
    const depth = roomDepth(period.entries.length);
    const slots = hangingSlots(depth);
    const shown = thinTo(period.entries, slots);
    const z0 = cursor;
    const z1 = cursor + depth;

    const frames: Frame[] = [];
    const perWall = Math.ceil(shown.length / 2);
    shown.forEach((entry, index) => {
      const side = index % 2 === 0 ? -1 : 1;
      const along = Math.floor(index / 2);
      const spread = perWall > 1 ? (depth - ART_MARGIN * 2) / (perWall - 1) : 0;
      const z = perWall > 1 ? z0 + ART_MARGIN + along * spread : (z0 + z1) / 2;
      frames.push({
        entry,
        x: side * (half - 0.06),
        y: ART_CENTRE,
        z,
        facing: side === -1 ? 1 : -1,
        width: 2.3,
        height: 1.7,
      });
    });

    const panes: Pane[] = [];
    const bays = Math.max(2, Math.round(depth / 6));
    for (let bay = 0; bay < bays; bay += 1) {
      const z = z0 + ((bay + 0.5) * depth) / bays;
      for (const side of [-1, 1] as const) {
        panes.push({
          x: side * (half - 0.05),
          y: CLERESTORY_BASE + CLERESTORY_HEIGHT / 2,
          z,
          facing: side === -1 ? 1 : -1,
          width: Math.min(4.4, depth / bays - 1.2),
          height: CLERESTORY_HEIGHT,
        });
      }
    }

    const aside = DOOR_WIDTH / 2 + 1.1;
    const furniture: Piece[] = [
      { kind: "plinth", x: -aside, z: z0 + 2.4, width: 0.9, height: 1.05, depth: 0.9 },
    ];
    const benches = Math.max(1, Math.floor(depth / 9));
    for (let i = 0; i < benches; i += 1) {
      furniture.push({
        kind: "bench",
        x: (i % 2 === 0 ? 1 : -1) * aside,
        z: z0 + ((i + 1) * depth) / (benches + 1),
        width: 0.8,
        height: 0.46,
        depth: 2.6,
      });
    }
    for (const side of [-1, 1] as const) {
      furniture.push({
        kind: "planter",
        x: side * (half - 1.1),
        z: z1 - 1.4,
        width: 0.8,
        height: 1.25,
        depth: 0.8,
      });
    }

    rooms.push({
      key: period.label,
      label: period.label,
      files: period.entries.length,
      hung: shown.length,
      z0,
      z1,
      frames,
      panes,
      furniture,
    });

    solids.push({ x0: -half - WALL, x1: -half, z0, z1 });
    solids.push({ x0: half, x1: half + WALL, z0, z1 });
    for (const piece of furniture) {
      solids.push({
        x0: piece.x - piece.width / 2,
        x1: piece.x + piece.width / 2,
        z0: piece.z - piece.depth / 2,
        z1: piece.z + piece.depth / 2,
      });
    }

    cursor = z1 + WALL;
  }

  const last = rooms[rooms.length - 1];
  solids.push({ x0: -half - WALL, x1: half + WALL, z0: -WALL, z1: 0 });
  solids.push({ x0: -half - WALL, x1: half + WALL, z0: last.z1, z1: last.z1 + WALL });

  for (let i = 0; i + 1 < rooms.length; i += 1) {
    const z0 = rooms[i].z1;
    const z1 = rooms[i + 1].z0;
    solids.push({ x0: -half - WALL, x1: -DOOR_WIDTH / 2, z0, z1 });
    solids.push({ x0: DOOR_WIDTH / 2, x1: half + WALL, z0, z1 });
  }

  return {
    rooms,
    frames: rooms.flatMap((room) => room.frames),
    solids,
    depth: last.z1,
    files: entries.length,
  };
}

export function roomAt(gallery: Gallery, z: number): Room | null {
  return gallery.rooms.find((room) => z >= room.z0 - WALL && z <= room.z1 + WALL) ?? null;
}
