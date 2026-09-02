import type { EntryView } from "$lib/api";

export const ROOM_HEIGHT = 6.5;
export const WALL = 0.45;
export const DOOR_WIDTH = 3.4;
export const DOOR_HEIGHT = 4;
export const CORRIDOR_LENGTH = 6;
export const CORRIDOR_HEIGHT = 4.6;
export const ART_CENTRE = 2.15;
export const ART_PITCH = 2.9;
export const ART_MARGIN = 1.7;
export const MIN_SIDE = 9;
export const MAX_SIDE = 34;
export const EYE_HEIGHT = 1.7;
export const CLERESTORY_BASE = 4.5;
export const CLERESTORY_HEIGHT = 1.4;
export const LAMP_HEIGHT = 4.9;
export const LAMP_SPACING = 8;
export const LAMP_STRENGTH = 0.9;
export const WALL_LAMP_HEIGHT = 3.5;
export const MAX_ROOMS = 24;
export const PLACE_TRIES = 12;

export interface Solid {
  x0: number;
  x1: number;
  z0: number;
  z1: number;
}

export interface Box extends Solid {
  y0: number;
  y1: number;
}

export interface Facing {
  nx: number;
  nz: number;
}

export interface Frame extends Facing {
  entry: number;
  x: number;
  y: number;
  z: number;
  width: number;
  height: number;
}

export interface Pane extends Facing {
  x: number;
  y: number;
  z: number;
  width: number;
  height: number;
}

export type PieceKind = "bench" | "plinth" | "planter";

export interface Lamp {
  x: number;
  y: number;
  z: number;
  strength: number;
  warm: number;
}

export interface Piece {
  kind: PieceKind;
  x: number;
  z: number;
  width: number;
  height: number;
  depth: number;
}

export interface Run extends Facing {
  x0: number;
  x1: number;
  z0: number;
  z1: number;
  length: number;
}

export interface Room extends Solid {
  key: string;
  label: string;
  files: number;
  hung: number;
  frames: Frame[];
  panes: Pane[];
  furniture: Piece[];
  runs: Run[];
  lamps: Lamp[];
}

export interface Corridor extends Solid {
  axis: "x" | "z";
}

export interface Gallery {
  rooms: Room[];
  lamps: Lamp[];
  corridors: Corridor[];
  walls: Box[];
  frames: Frame[];
  solids: Solid[];
  bounds: Solid;
  files: number;
  start: { x: number; z: number; yaw: number };
}

export const EMPTY_GALLERY: Gallery = {
  rooms: [],
  lamps: [],
  corridors: [],
  walls: [],
  frames: [],
  solids: [],
  bounds: { x0: 0, x1: 0, z0: 0, z1: 0 },
  files: 0,
  start: { x: 0, z: 0, yaw: Math.PI },
};

export type Heading = 0 | 1 | 2 | 3;

const STEPS: Record<Heading, { dx: number; dz: number }> = {
  0: { dx: 0, dz: 1 },
  1: { dx: 1, dz: 0 },
  2: { dx: 0, dz: -1 },
  3: { dx: -1, dz: 0 },
};

export function seedFrom(entries: EntryView[]): number {
  let seed = entries.length * 2654435761;
  for (const entry of entries.slice(0, 24)) {
    for (let i = 0; i < entry.source.length; i += 1) {
      seed = (seed ^ entry.source.charCodeAt(i)) * 16777619;
      seed >>>= 0;
    }
  }
  return seed >>> 0 || 1;
}

export function randomFrom(seed: number): () => number {
  let state = seed >>> 0 || 1;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

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

export function roomSize(count: number, aspect: number): { width: number; depth: number } {
  const wanted = (Math.max(1, count) * ART_PITCH + ART_MARGIN * 4) / 4;
  return {
    width: clamp(wanted * aspect, MIN_SIDE, MAX_SIDE),
    depth: clamp(wanted / aspect, MIN_SIDE, MAX_SIDE),
  };
}

export function hangingSlots(runs: Run[]): number {
  return runs.reduce((sum, run) => sum + Math.floor(Math.max(0, run.length) / ART_PITCH), 0);
}

export function thinTo<T>(items: T[], limit: number): T[] {
  if (items.length <= limit) return items;
  const kept: T[] = [];
  for (let i = 0; i < limit; i += 1) {
    kept.push(items[Math.floor((i * items.length) / limit)]);
  }
  return kept;
}

export function overlaps(a: Solid, b: Solid, margin = 0): boolean {
  return (
    a.x0 - margin < b.x1 &&
    a.x1 + margin > b.x0 &&
    a.z0 - margin < b.z1 &&
    a.z1 + margin > b.z0
  );
}

export function holds(area: Solid, x: number, z: number, margin = 0): boolean {
  return (
    x >= area.x0 - margin && x <= area.x1 + margin && z >= area.z0 - margin && z <= area.z1 + margin
  );
}

export function runsOf(room: Solid, doors: { side: Heading; from: number; to: number }[]): Run[] {
  const runs: Run[] = [];

  for (const side of [2, 1, 0, 3] as Heading[]) {
    const along = side === 0 || side === 2 ? "x" : "z";
    const from = along === "x" ? room.x0 : room.z0;
    const to = along === "x" ? room.x1 : room.z1;
    const cuts = doors
      .filter((door) => door.side === side)
      .map((door) => [door.from, door.to] as [number, number])
      .sort((a, b) => a[0] - b[0]);

    let cursor = from;
    for (const [start, end] of [...cuts, [to, to] as [number, number]]) {
      const stop = Math.min(start, to);
      if (stop - cursor > ART_PITCH) runs.push(makeRun(room, side, cursor, stop));
      cursor = Math.max(cursor, end);
    }
  }

  return runs;
}

function makeRun(room: Solid, side: Heading, from: number, to: number): Run {
  if (side === 0 || side === 2) {
    const z = side === 0 ? room.z1 : room.z0;
    return { x0: from, x1: to, z0: z, z1: z, nx: 0, nz: side === 0 ? -1 : 1, length: to - from };
  }
  const x = side === 1 ? room.x1 : room.x0;
  return { x0: x, x1: x, z0: from, z1: to, nx: side === 1 ? -1 : 1, nz: 0, length: to - from };
}

export function alongRun(run: Run, t: number): { x: number; z: number } {
  return { x: run.x0 + (run.x1 - run.x0) * t, z: run.z0 + (run.z1 - run.z0) * t };
}

export function buildGallery(entries: EntryView[]): Gallery {
  const periods = thinTo(groupIntoPeriods(entries), MAX_ROOMS);
  if (periods.length === 0) return EMPTY_GALLERY;

  const random = randomFrom(seedFrom(entries));
  const placed: Solid[] = [];
  const corridors: Corridor[] = [];
  const doors = new Map<number, { side: Heading; from: number; to: number }[]>();
  const shells: Solid[] = [];
  let heading: Heading = 0;

  periods.forEach((period, index) => {
    const aspect = 0.72 + random() * 0.8;
    const { width, depth } = roomSize(Math.min(period.entries.length, 240), aspect);

    if (index === 0) {
      const room = { x0: -width / 2, x1: width / 2, z0: -depth / 2, z1: depth / 2 };
      shells.push(room);
      placed.push(room);
      return;
    }

    const previous = shells[index - 1];
    const put = place(previous, width, depth, heading, placed, random);
    heading = put.heading;
    shells.push(put.room);
    placed.push(put.room, put.corridor);
    corridors.push(put.corridor);

    keep(doors, index - 1, put.fromDoor);
    keep(doors, index, put.toDoor);
  });

  const rooms: Room[] = shells.map((shell, index) => {
    const period = periods[index];
    const runs = runsOf(shell, doors.get(index) ?? []);
    const shown = thinTo(period.entries, Math.max(1, hangingSlots(runs)));
    const frames = hang(shown, runs);

    return {
      ...shell,
      key: period.label,
      label: period.label,
      files: period.entries.length,
      hung: frames.length,
      frames,
      panes: windows(runs),
      furniture: furnish(shell, doors.get(index) ?? [], random),
      runs,
      lamps: lampsFor(shell, runs),
    };
  });

  const walls = wallsFor(rooms, corridors, doors);
  const solids: Solid[] = walls
    .filter((box) => box.y0 <= 0)
    .map((box) => ({ x0: box.x0, x1: box.x1, z0: box.z0, z1: box.z1 }));
  for (const room of rooms) {
    for (const piece of room.furniture) {
      solids.push({
        x0: piece.x - piece.width / 2,
        x1: piece.x + piece.width / 2,
        z0: piece.z - piece.depth / 2,
        z1: piece.z + piece.depth / 2,
      });
    }
  }

  return {
    rooms,
    lamps: [...rooms.flatMap((room) => room.lamps), ...corridors.flatMap(corridorLamps)],
    corridors,
    walls,
    frames: rooms.flatMap((room) => room.frames),
    solids,
    bounds: boundsOf(rooms, corridors),
    files: entries.length,
    start: standing(rooms[0], solids),
  };
}

export function standing(room: Solid, solids: Solid[]): { x: number; z: number; yaw: number } {
  const x = mid(room.x0, room.x1);
  const z = mid(room.z0, room.z1);
  const spots = [
    { x, z: room.z0 + 1.8, yaw: Math.PI },
    { x, z: room.z1 - 1.8, yaw: 0 },
    { x: room.x0 + 1.8, z, yaw: -Math.PI / 2 },
    { x: room.x1 - 1.8, z, yaw: Math.PI / 2 },
    { x, z, yaw: Math.PI },
  ];

  for (const spot of spots) {
    const walked = [0, 0.7, 1.4, 2.1, 2.8].every((reach) =>
      free(spot.x - Math.sin(spot.yaw) * reach, spot.z - Math.cos(spot.yaw) * reach, solids),
    );
    if (walked) return spot;
  }
  return spots[spots.length - 1];
}

function free(x: number, z: number, solids: Solid[], margin = 0.6): boolean {
  return !solids.some(
    (solid) =>
      x > solid.x0 - margin && x < solid.x1 + margin && z > solid.z0 - margin && z < solid.z1 + margin,
  );
}

function keep(
  doors: Map<number, { side: Heading; from: number; to: number }[]>,
  index: number,
  door: { side: Heading; from: number; to: number },
) {
  const bucket = doors.get(index);
  if (bucket) bucket.push(door);
  else doors.set(index, [door]);
}

function place(
  previous: Solid,
  width: number,
  depth: number,
  heading: Heading,
  placed: Solid[],
  random: () => number,
): {
  room: Solid;
  corridor: Corridor;
  heading: Heading;
  fromDoor: { side: Heading; from: number; to: number };
  toDoor: { side: Heading; from: number; to: number };
} {
  const first = leaning(heading, random());
  const order: Heading[] = [
    first,
    ...shuffle([heading, turn(heading, 1), turn(heading, 3)].filter((one) => one !== first), random),
  ];

  for (let attempt = 0; attempt < PLACE_TRIES; attempt += 1) {
    const going = order[attempt % order.length];
    const reach = CORRIDOR_LENGTH + WALL * 2 + Math.floor(attempt / order.length) * 4;
    const built = along(previous, width, depth, going, reach, random);
    const clear = placed.every(
      (taken) =>
        !overlaps(built.room, taken, WALL * 3) &&
        (taken === previous || !overlaps(built.corridor, taken, 0.05)),
    );
    if (clear) return { ...built, heading: going };
  }

  const forced = along(previous, width, depth, heading, CORRIDOR_LENGTH + WALL * 2 + 40, random);
  return { ...forced, heading };
}

export function leaning(heading: Heading, roll: number): Heading {
  if (roll < 0.5) return heading;
  return roll < 0.75 ? turn(heading, 1) : turn(heading, 3);
}

function along(
  previous: Solid,
  width: number,
  depth: number,
  going: Heading,
  reach: number,
  random: () => number,
): {
  room: Solid;
  corridor: Corridor;
  fromDoor: { side: Heading; from: number; to: number };
  toDoor: { side: Heading; from: number; to: number };
} {
  const step = STEPS[going];
  const sideways = going === 0 || going === 2 ? "x" : "z";

  const previousMid = sideways === "x" ? mid(previous.x0, previous.x1) : mid(previous.z0, previous.z1);
  const room = boxAt(previous, width, depth, going, reach, previousMid, random);
  const roomMid = sideways === "x" ? mid(room.x0, room.x1) : mid(room.z0, room.z1);

  const low = Math.max(
    sideways === "x" ? previous.x0 : previous.z0,
    sideways === "x" ? room.x0 : room.z0,
  );
  const high = Math.min(
    sideways === "x" ? previous.x1 : previous.z1,
    sideways === "x" ? room.x1 : room.z1,
  );
  const centre = clamp(
    (previousMid + roomMid) / 2,
    low + DOOR_WIDTH / 2 + WALL,
    high - DOOR_WIDTH / 2 - WALL,
  );

  const gapFrom = going === 0 ? previous.z1 : going === 2 ? room.z1 : going === 1 ? previous.x1 : room.x1;
  const gapTo = going === 0 ? room.z0 : going === 2 ? previous.z0 : going === 1 ? room.x0 : previous.x0;

  const corridor: Corridor =
    step.dz !== 0
      ? {
          axis: "z",
          x0: centre - DOOR_WIDTH / 2,
          x1: centre + DOOR_WIDTH / 2,
          z0: gapFrom,
          z1: gapTo,
        }
      : {
          axis: "x",
          x0: gapFrom,
          x1: gapTo,
          z0: centre - DOOR_WIDTH / 2,
          z1: centre + DOOR_WIDTH / 2,
        };

  const door = { from: centre - DOOR_WIDTH / 2, to: centre + DOOR_WIDTH / 2 };
  return {
    room,
    corridor,
    fromDoor: { side: going, ...door },
    toDoor: { side: turn(going, 2), ...door },
  };
}

function boxAt(
  previous: Solid,
  width: number,
  depth: number,
  going: Heading,
  reach: number,
  previousMid: number,
  random: () => number,
): Solid {
  const drift = (random() - 0.5) * Math.min(width, depth) * 0.5;

  if (going === 0 || going === 2) {
    const centre = previousMid + drift;
    const z0 = going === 0 ? previous.z1 + reach : previous.z0 - reach - depth;
    return { x0: centre - width / 2, x1: centre + width / 2, z0, z1: z0 + depth };
  }

  const centre = previousMid + drift;
  const x0 = going === 1 ? previous.x1 + reach : previous.x0 - reach - width;
  return { x0, x1: x0 + width, z0: centre - depth / 2, z1: centre + depth / 2 };
}

function hang(shown: number[], runs: Run[]): Frame[] {
  const frames: Frame[] = [];
  const slots: { run: Run; count: number }[] = runs.map((run) => ({
    run,
    count: Math.floor(run.length / ART_PITCH),
  }));
  const total = slots.reduce((sum, slot) => sum + slot.count, 0);
  if (total === 0) return frames;

  let taken = 0;
  for (const slot of slots) {
    const share = Math.min(slot.count, Math.round((shown.length * slot.count) / total));
    for (let i = 0; i < share && taken < shown.length; i += 1) {
      const t = (i + 0.5) / share;
      const { x, z } = alongRun(slot.run, t);
      frames.push({
        entry: shown[taken],
        x: x + slot.run.nx * 0.06,
        y: ART_CENTRE,
        z: z + slot.run.nz * 0.06,
        nx: slot.run.nx,
        nz: slot.run.nz,
        width: 2.3,
        height: 1.7,
      });
      taken += 1;
    }
  }

  return frames;
}

function windows(runs: Run[]): Pane[] {
  const panes: Pane[] = [];
  for (const run of runs) {
    const bays = Math.max(1, Math.round(run.length / 7));
    for (let bay = 0; bay < bays; bay += 1) {
      const { x, z } = alongRun(run, (bay + 0.5) / bays);
      panes.push({
        x: x + run.nx * 0.05,
        y: CLERESTORY_BASE + CLERESTORY_HEIGHT / 2,
        z: z + run.nz * 0.05,
        nx: run.nx,
        nz: run.nz,
        width: Math.min(4.4, run.length / bays - 1.2),
        height: CLERESTORY_HEIGHT,
      });
    }
  }
  return panes;
}

export function doorApron(room: Solid, door: { side: Heading; from: number; to: number }): Solid {
  const reach = 3.2;
  if (door.side === 0 || door.side === 2) {
    const z = door.side === 0 ? room.z1 : room.z0;
    return {
      x0: door.from - 0.6,
      x1: door.to + 0.6,
      z0: door.side === 0 ? z - reach : z,
      z1: door.side === 0 ? z : z + reach,
    };
  }
  const x = door.side === 1 ? room.x1 : room.x0;
  return {
    x0: door.side === 1 ? x - reach : x,
    x1: door.side === 1 ? x : x + reach,
    z0: door.from - 0.6,
    z1: door.to + 0.6,
  };
}

function furnish(
  room: Solid,
  doors: { side: Heading; from: number; to: number }[],
  random: () => number,
): Piece[] {
  const aprons = doors.map((door) => doorApron(room, door));
  const width = room.x1 - room.x0;
  const depth = room.z1 - room.z0;
  const x = mid(room.x0, room.x1);
  const z = mid(room.z0, room.z1);
  const pieces: Piece[] = [];

  const put = (piece: Piece) => {
    const footprint = {
      x0: piece.x - piece.width / 2,
      x1: piece.x + piece.width / 2,
      z0: piece.z - piece.depth / 2,
      z1: piece.z + piece.depth / 2,
    };
    if (aprons.some((apron) => overlaps(footprint, apron, 0.5))) return;
    if (pieces.some((taken) => overlaps(footprint, footprintOf(taken), 0.8))) return;
    pieces.push(piece);
  };

  const aside = (random() < 0.5 ? -1 : 1) * Math.min(width, depth) * 0.26;
  put({ kind: "plinth", x: x + aside, z: z - aside * 0.6, width: 1.1, height: 1.05, depth: 1.1 });

  const benches = Math.max(1, Math.floor(Math.min(width, depth) / 7));
  for (let i = 0; i < benches; i += 1) {
    const across = width > depth;
    const offset = ((i + 1) / (benches + 1) - 0.5) * (across ? width : depth) * 0.62;
    put({
      kind: "bench",
      x: across ? x + offset : x - aside,
      z: across ? z - aside : z + offset,
      width: across ? 2.6 : 0.8,
      height: 0.46,
      depth: across ? 0.8 : 2.6,
    });
  }

  const corners: [number, number][] = [
    [room.x0 + 1.2, room.z0 + 1.2],
    [room.x1 - 1.2, room.z0 + 1.2],
    [room.x0 + 1.2, room.z1 - 1.2],
    [room.x1 - 1.2, room.z1 - 1.2],
  ];
  for (const [cx, cz] of corners) {
    if (random() < 0.55) {
      put({ kind: "planter", x: cx, z: cz, width: 0.8, height: 1.25, depth: 0.8 });
    }
  }

  return pieces;
}

export function lampsFor(room: Solid, runs: Run[]): Lamp[] {
  const lamps: Lamp[] = [];
  const width = room.x1 - room.x0;
  const depth = room.z1 - room.z0;
  const across = Math.max(1, Math.round(width / LAMP_SPACING));
  const along = Math.max(1, Math.round(depth / LAMP_SPACING));

  for (let i = 0; i < across; i += 1) {
    for (let j = 0; j < along; j += 1) {
      lamps.push({
        x: room.x0 + ((i + 0.5) * width) / across,
        y: LAMP_HEIGHT,
        z: room.z0 + ((j + 0.5) * depth) / along,
        strength: LAMP_STRENGTH,
        warm: 1,
      });
    }
  }

  for (const run of runs) {
    if (run.length < ART_PITCH * 2) continue;
    const { x, z } = alongRun(run, 0.5);
    lamps.push({
      x: x + run.nx * 0.5,
      y: WALL_LAMP_HEIGHT,
      z: z + run.nz * 0.5,
      strength: LAMP_STRENGTH * 0.55,
      warm: 0.7,
    });
  }

  return lamps;
}

export function corridorLamps(corridor: Corridor): Lamp[] {
  const length = corridor.axis === "z" ? corridor.z1 - corridor.z0 : corridor.x1 - corridor.x0;
  const count = Math.max(1, Math.round(length / 5));
  const lamps: Lamp[] = [];

  for (let i = 0; i < count; i += 1) {
    const t = (i + 0.5) / count;
    lamps.push({
      x: corridor.axis === "z" ? mid(corridor.x0, corridor.x1) : corridor.x0 + (corridor.x1 - corridor.x0) * t,
      y: CORRIDOR_HEIGHT - 0.35,
      z: corridor.axis === "z" ? corridor.z0 + (corridor.z1 - corridor.z0) * t : mid(corridor.z0, corridor.z1),
      strength: LAMP_STRENGTH * 0.7,
      warm: 0.85,
    });
  }

  return lamps;
}

export function nearestLamps(lamps: Lamp[], x: number, z: number, limit: number): Lamp[] {
  return [...lamps]
    .sort(
      (a, b) => (a.x - x) ** 2 + (a.z - z) ** 2 - ((b.x - x) ** 2 + (b.z - z) ** 2),
    )
    .slice(0, Math.max(0, limit));
}

function footprintOf(piece: Piece): Solid {
  return {
    x0: piece.x - piece.width / 2,
    x1: piece.x + piece.width / 2,
    z0: piece.z - piece.depth / 2,
    z1: piece.z + piece.depth / 2,
  };
}

function wallsFor(
  rooms: Room[],
  corridors: Corridor[],
  doors: Map<number, { side: Heading; from: number; to: number }[]>,
): Box[] {
  const boxes: Box[] = [];

  rooms.forEach((room, index) => {
    const openings = doors.get(index) ?? [];

    for (const side of [0, 1, 2, 3] as Heading[]) {
      const along = side === 0 || side === 2 ? "x" : "z";
      const from = along === "x" ? room.x0 - WALL : room.z0;
      const to = along === "x" ? room.x1 + WALL : room.z1;
      const cuts = openings
        .filter((door) => door.side === side)
        .map((door) => [door.from, door.to] as [number, number])
        .sort((a, b) => a[0] - b[0]);

      let cursor = from;
      for (const [start, end] of [...cuts, [to, to] as [number, number]]) {
        if (start > cursor) boxes.push(wallBox(room, side, cursor, Math.min(start, to), 0));
        if (end > start) boxes.push(wallBox(room, side, start, end, DOOR_HEIGHT));
        cursor = Math.max(cursor, end);
      }
    }
  });

  for (const corridor of corridors) {
    if (corridor.axis === "z") {
      boxes.push({
        x0: corridor.x0 - WALL,
        x1: corridor.x0,
        z0: corridor.z0,
        z1: corridor.z1,
        y0: 0,
        y1: CORRIDOR_HEIGHT,
      });
      boxes.push({
        x0: corridor.x1,
        x1: corridor.x1 + WALL,
        z0: corridor.z0,
        z1: corridor.z1,
        y0: 0,
        y1: CORRIDOR_HEIGHT,
      });
    } else {
      boxes.push({
        x0: corridor.x0,
        x1: corridor.x1,
        z0: corridor.z0 - WALL,
        z1: corridor.z0,
        y0: 0,
        y1: CORRIDOR_HEIGHT,
      });
      boxes.push({
        x0: corridor.x0,
        x1: corridor.x1,
        z0: corridor.z1,
        z1: corridor.z1 + WALL,
        y0: 0,
        y1: CORRIDOR_HEIGHT,
      });
    }
  }

  return boxes;
}

function wallBox(room: Solid, side: Heading, from: number, to: number, y0: number): Box {
  if (side === 0 || side === 2) {
    const z = side === 0 ? room.z1 : room.z0 - WALL;
    return { x0: from, x1: to, z0: z, z1: z + WALL, y0, y1: ROOM_HEIGHT };
  }
  const x = side === 1 ? room.x1 : room.x0 - WALL;
  return { x0: x, x1: x + WALL, z0: from, z1: to, y0, y1: ROOM_HEIGHT };
}

function boundsOf(rooms: Room[], corridors: Corridor[]): Solid {
  const all: Solid[] = [...rooms, ...corridors];
  return {
    x0: Math.min(...all.map((one) => one.x0)) - WALL,
    x1: Math.max(...all.map((one) => one.x1)) + WALL,
    z0: Math.min(...all.map((one) => one.z0)) - WALL,
    z1: Math.max(...all.map((one) => one.z1)) + WALL,
  };
}

export function roomAt(gallery: Gallery, x: number, z: number): Room | null {
  return gallery.rooms.find((room) => holds(room, x, z, WALL)) ?? null;
}

export function turn(heading: Heading, by: number): Heading {
  return (((heading + by) % 4) + 4) % 4 as Heading;
}

function shuffle<T>(items: T[], random: () => number): T[] {
  const out = [...items];
  for (let i = out.length - 1; i > 0; i -= 1) {
    const j = Math.floor(random() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

function mid(low: number, high: number): number {
  return (low + high) / 2;
}

function clamp(value: number, low: number, high: number): number {
  return value < low ? low : value > high ? high : value;
}
