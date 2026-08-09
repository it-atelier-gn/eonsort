import {
  CLERESTORY_BASE,
  CLERESTORY_HEIGHT,
  DOOR_HEIGHT,
  DOOR_WIDTH,
  ROOM_HEIGHT,
  ROOM_WIDTH,
  WALL,
  type Gallery,
  type Piece,
} from "./layout";

export interface Mesh {
  position: Float32Array;
  normal: Float32Array;
  shade: Float32Array;
  count: number;
}

export interface Quad {
  position: Float32Array;
  count: number;
}

const FLOOR = 0.3;
const CEILING = 0.62;
const WALLFACE = 0.86;
const TRIM = 0.46;
const FURNITURE_TONE: Record<Piece["kind"], number> = {
  bench: 0.42,
  plinth: 0.78,
  planter: 0.34,
};

class Builder {
  position: number[] = [];
  normal: number[] = [];
  shade: number[] = [];

  face(
    a: [number, number, number],
    b: [number, number, number],
    c: [number, number, number],
    d: [number, number, number],
    normal: [number, number, number],
    shade: number,
  ) {
    for (const point of [a, b, c, a, c, d]) {
      this.position.push(point[0], point[1], point[2]);
      this.normal.push(normal[0], normal[1], normal[2]);
      this.shade.push(shade);
    }
  }

  box(
    x0: number,
    y0: number,
    z0: number,
    x1: number,
    y1: number,
    z1: number,
    shade: number,
  ) {
    this.face([x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1], [0, 1, 0], shade);
    this.face([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [0, 0, 1], shade * 0.92);
    this.face([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [0, 0, -1], shade * 0.92);
    this.face([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [1, 0, 0], shade * 0.84);
    this.face([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [-1, 0, 0], shade * 0.84);
  }

  mesh(): Mesh {
    return {
      position: new Float32Array(this.position),
      normal: new Float32Array(this.normal),
      shade: new Float32Array(this.shade),
      count: this.position.length / 3,
    };
  }
}

export function buildRoomMesh(gallery: Gallery): Mesh {
  const build = new Builder();
  if (gallery.rooms.length === 0) return build.mesh();
  const half = ROOM_WIDTH / 2;

  for (const room of gallery.rooms) {
    const { z0, z1 } = room;

    build.face(
      [-half, 0, z0],
      [half, 0, z0],
      [half, 0, z1],
      [-half, 0, z1],
      [0, 1, 0],
      FLOOR,
    );
    build.face(
      [-half, ROOM_HEIGHT, z1],
      [half, ROOM_HEIGHT, z1],
      [half, ROOM_HEIGHT, z0],
      [-half, ROOM_HEIGHT, z0],
      [0, -1, 0],
      CEILING,
    );

    for (const side of [-1, 1] as const) {
      const x = side * half;
      const inward: [number, number, number] = [-side, 0, 0];
      const bays = room.panes.filter((pane) => Math.sign(pane.x) === side);

      build.face(
        [x, 0, z0],
        [x, 0, z1],
        [x, CLERESTORY_BASE, z1],
        [x, CLERESTORY_BASE, z0],
        inward,
        WALLFACE,
      );
      build.face(
        [x, CLERESTORY_BASE + CLERESTORY_HEIGHT, z0],
        [x, CLERESTORY_BASE + CLERESTORY_HEIGHT, z1],
        [x, ROOM_HEIGHT, z1],
        [x, ROOM_HEIGHT, z0],
        inward,
        WALLFACE * 0.9,
      );

      let cursor = z0;
      for (const pane of [...bays].sort((a, b) => a.z - b.z)) {
        const start = pane.z - pane.width / 2;
        const end = pane.z + pane.width / 2;
        if (start > cursor) {
          build.face(
            [x, CLERESTORY_BASE, cursor],
            [x, CLERESTORY_BASE, start],
            [x, CLERESTORY_BASE + CLERESTORY_HEIGHT, start],
            [x, CLERESTORY_BASE + CLERESTORY_HEIGHT, cursor],
            inward,
            WALLFACE,
          );
        }
        cursor = Math.max(cursor, end);
      }
      if (cursor < z1) {
        build.face(
          [x, CLERESTORY_BASE, cursor],
          [x, CLERESTORY_BASE, z1],
          [x, CLERESTORY_BASE + CLERESTORY_HEIGHT, z1],
          [x, CLERESTORY_BASE + CLERESTORY_HEIGHT, cursor],
          inward,
          WALLFACE,
        );
      }

      build.box(
        side === -1 ? -half - 0.08 : half - 0.08,
        0,
        z0,
        side === -1 ? -half + 0.08 : half + 0.08,
        0.18,
        z1,
        TRIM,
      );
    }
  }

  for (let i = 0; i <= gallery.rooms.length; i += 1) {
    const previous = gallery.rooms[i - 1];
    const next = gallery.rooms[i];
    const z0 = previous ? previous.z1 : -WALL;
    const z1 = next ? next.z0 : previous!.z1 + WALL;
    const opening = Boolean(previous && next);
    divider(build, z0, z1, opening);
  }

  for (const room of gallery.rooms) {
    for (const piece of room.furniture) {
      furniture(build, piece);
    }
  }

  return build.mesh();
}

function divider(build: Builder, z0: number, z1: number, opening: boolean) {
  const half = ROOM_WIDTH / 2;
  const shade = WALLFACE * 0.94;

  if (!opening) {
    for (const [z, normal] of [
      [z1, [0, 0, -1]],
      [z0, [0, 0, 1]],
    ] as const) {
      build.face(
        [-half, 0, z],
        [half, 0, z],
        [half, ROOM_HEIGHT, z],
        [-half, ROOM_HEIGHT, z],
        normal as [number, number, number],
        shade,
      );
    }
    return;
  }

  const door = DOOR_WIDTH / 2;
  for (const [z, normal] of [
    [z1, [0, 0, -1]],
    [z0, [0, 0, 1]],
  ] as const) {
    const n = normal as [number, number, number];
    build.face([-half, 0, z], [-door, 0, z], [-door, ROOM_HEIGHT, z], [-half, ROOM_HEIGHT, z], n, shade);
    build.face([door, 0, z], [half, 0, z], [half, ROOM_HEIGHT, z], [door, ROOM_HEIGHT, z], n, shade);
    build.face(
      [-door, DOOR_HEIGHT, z],
      [door, DOOR_HEIGHT, z],
      [door, ROOM_HEIGHT, z],
      [-door, ROOM_HEIGHT, z],
      n,
      shade,
    );
  }

  build.face(
    [-door, DOOR_HEIGHT, z0],
    [door, DOOR_HEIGHT, z0],
    [door, DOOR_HEIGHT, z1],
    [-door, DOOR_HEIGHT, z1],
    [0, -1, 0],
    TRIM,
  );
  for (const side of [-1, 1] as const) {
    const x = side * door;
    build.face(
      [x, 0, z0],
      [x, 0, z1],
      [x, DOOR_HEIGHT, z1],
      [x, DOOR_HEIGHT, z0],
      [-side, 0, 0],
      TRIM,
    );
  }
}

function furniture(build: Builder, piece: Piece) {
  const shade = FURNITURE_TONE[piece.kind];
  const x0 = piece.x - piece.width / 2;
  const x1 = piece.x + piece.width / 2;
  const z0 = piece.z - piece.depth / 2;
  const z1 = piece.z + piece.depth / 2;

  if (piece.kind === "bench") {
    const legs = 0.12;
    build.box(x0, piece.height - 0.12, z0, x1, piece.height, z1, shade);
    for (const x of [x0 + legs, x1 - legs * 2]) {
      build.box(x, 0, z0 + legs, x + legs, piece.height - 0.12, z1 - legs, shade * 0.7);
    }
    return;
  }

  if (piece.kind === "planter") {
    build.box(x0, 0, z0, x1, piece.height * 0.42, z1, shade);
    const inner = piece.width * 0.18;
    build.box(
      piece.x - inner,
      piece.height * 0.42,
      piece.z - inner,
      piece.x + inner,
      piece.height,
      piece.z + inner,
      shade * 1.4,
    );
    return;
  }

  build.box(x0, 0, z0, x1, piece.height, z1, shade);
}

export function buildPaneQuads(gallery: Gallery): Quad {
  const out: number[] = [];
  for (const room of gallery.rooms) {
    for (const pane of room.panes) {
      const x = pane.x;
      const y0 = pane.y - pane.height / 2;
      const y1 = pane.y + pane.height / 2;
      const z0 = pane.z - pane.width / 2;
      const z1 = pane.z + pane.width / 2;
      push(out, [x, y0, z0], [x, y0, z1], [x, y1, z1]);
      push(out, [x, y0, z0], [x, y1, z1], [x, y1, z0]);
    }
  }
  return { position: new Float32Array(out), count: out.length / 3 };
}

export function buildShaftQuads(gallery: Gallery): Quad {
  const out: number[] = [];
  const half = ROOM_WIDTH / 2;

  for (const room of gallery.rooms) {
    for (const pane of room.panes) {
      const side = Math.sign(pane.x);
      const top = pane.y + pane.height / 2;
      const bottom = 0;
      const reach = half * 1.25;
      const nearX = pane.x;
      const farX = pane.x - side * reach;
      const z0 = pane.z - pane.width / 2;
      const z1 = pane.z + pane.width / 2;
      const spread = 1.5;

      push(out, [nearX, top, z0], [nearX, top, z1], [farX, bottom, z1 + spread]);
      push(out, [nearX, top, z0], [farX, bottom, z1 + spread], [farX, bottom, z0 - spread]);
    }
  }
  return { position: new Float32Array(out), count: out.length / 3 };
}

function push(out: number[], ...points: [number, number, number][]) {
  for (const point of points) out.push(point[0], point[1], point[2]);
}
