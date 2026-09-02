import { CORRIDOR_HEIGHT, ROOM_HEIGHT, type Gallery, type Piece, type Room } from "./layout";

const LAMP_WIDTH = 0.85;
const LAMP_DEPTH = 0.5;

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
    const corners = facingOut(a, b, c, normal) ? [a, b, c, a, c, d] : [a, d, c, a, c, b];
    for (const point of corners) {
      this.position.push(point[0], point[1], point[2]);
      this.normal.push(normal[0], normal[1], normal[2]);
      this.shade.push(shade);
    }
  }

  box(x0: number, y0: number, z0: number, x1: number, y1: number, z1: number, shade: number) {
    this.face([x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1], [0, 1, 0], shade);
    this.face([x0, y0, z1], [x1, y0, z1], [x1, y0, z0], [x0, y0, z0], [0, -1, 0], shade * 0.7);
    this.face([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [0, 0, 1], shade * 0.92);
    this.face([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [0, 0, -1], shade * 0.92);
    this.face([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [1, 0, 0], shade * 0.84);
    this.face([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [-1, 0, 0], shade * 0.84);
  }

  floor(x0: number, z0: number, x1: number, z1: number, shade: number) {
    this.face([x0, 0, z0], [x1, 0, z0], [x1, 0, z1], [x0, 0, z1], [0, 1, 0], shade);
  }

  ceiling(x0: number, z0: number, x1: number, z1: number, y: number, shade: number) {
    this.face([x0, y, z1], [x1, y, z1], [x1, y, z0], [x0, y, z0], [0, -1, 0], shade);
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

function facingOut(
  a: [number, number, number],
  b: [number, number, number],
  c: [number, number, number],
  normal: [number, number, number],
): boolean {
  const ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
  const ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
  const cross = [
    ab[1] * ac[2] - ab[2] * ac[1],
    ab[2] * ac[0] - ab[0] * ac[2],
    ab[0] * ac[1] - ab[1] * ac[0],
  ];
  return cross[0] * normal[0] + cross[1] * normal[1] + cross[2] * normal[2] >= 0;
}

export function buildRoomMesh(gallery: Gallery): Mesh {
  const build = new Builder();
  if (gallery.rooms.length === 0) return build.mesh();

  for (const room of gallery.rooms) {
    build.floor(room.x0, room.z0, room.x1, room.z1, FLOOR);
    build.ceiling(room.x0, room.z0, room.x1, room.z1, ROOM_HEIGHT, CEILING);
    skirting(build, room);
  }

  for (const corridor of gallery.corridors) {
    build.floor(corridor.x0, corridor.z0, corridor.x1, corridor.z1, FLOOR * 0.9);
    build.ceiling(corridor.x0, corridor.z0, corridor.x1, corridor.z1, CORRIDOR_HEIGHT, CEILING * 0.8);
  }

  for (const wall of gallery.walls) {
    build.box(wall.x0, wall.y0, wall.z0, wall.x1, wall.y1, wall.z1, WALLFACE);
  }

  for (const room of gallery.rooms) {
    for (const piece of room.furniture) {
      furniture(build, piece);
    }
  }

  return build.mesh();
}

function skirting(build: Builder, room: Room) {
  const lip = 0.16;
  build.box(room.x0, 0, room.z0, room.x1, lip, room.z0 + lip, TRIM);
  build.box(room.x0, 0, room.z1 - lip, room.x1, lip, room.z1, TRIM);
  build.box(room.x0, 0, room.z0, room.x0 + lip, lip, room.z1, TRIM);
  build.box(room.x1 - lip, 0, room.z0, room.x1, lip, room.z1, TRIM);
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
      const half = pane.width / 2;
      const alongX = pane.nz !== 0 ? half : 0;
      const alongZ = pane.nx !== 0 ? half : 0;
      const y0 = pane.y - pane.height / 2;
      const y1 = pane.y + pane.height / 2;

      const a: [number, number, number] = [pane.x - alongX, y0, pane.z - alongZ];
      const b: [number, number, number] = [pane.x + alongX, y0, pane.z + alongZ];
      const c: [number, number, number] = [pane.x + alongX, y1, pane.z + alongZ];
      const d: [number, number, number] = [pane.x - alongX, y1, pane.z - alongZ];
      push(out, a, b, c);
      push(out, a, c, d);
    }
  }
  return { position: new Float32Array(out), count: out.length / 3 };
}

export function buildShaftQuads(gallery: Gallery): Quad {
  const out: number[] = [];

  for (const room of gallery.rooms) {
    for (const pane of room.panes) {
      const half = pane.width / 2;
      const alongX = pane.nz !== 0 ? half : 0;
      const alongZ = pane.nx !== 0 ? half : 0;
      const top = pane.y + pane.height / 2;
      const reach = 9;
      const spread = 1.5;

      const nearA: [number, number, number] = [pane.x - alongX, top, pane.z - alongZ];
      const nearB: [number, number, number] = [pane.x + alongX, top, pane.z + alongZ];
      const farA: [number, number, number] = [
        pane.x + pane.nx * reach - alongX * spread,
        0,
        pane.z + pane.nz * reach - alongZ * spread,
      ];
      const farB: [number, number, number] = [
        pane.x + pane.nx * reach + alongX * spread,
        0,
        pane.z + pane.nz * reach + alongZ * spread,
      ];

      push(out, nearA, nearB, farB);
      push(out, nearA, farB, farA);
    }
  }
  return { position: new Float32Array(out), count: out.length / 3 };
}

export function buildLampQuads(gallery: Gallery): Quad {
  const out: number[] = [];

  for (const lamp of gallery.lamps) {
    const x0 = lamp.x - LAMP_WIDTH / 2;
    const x1 = lamp.x + LAMP_WIDTH / 2;
    const z0 = lamp.z - LAMP_DEPTH / 2;
    const z1 = lamp.z + LAMP_DEPTH / 2;
    const y = lamp.y;

    push(out, [x0, y, z0], [x1, y, z0], [x1, y, z1]);
    push(out, [x0, y, z0], [x1, y, z1], [x0, y, z1]);
  }

  return { position: new Float32Array(out), count: out.length / 3 };
}

function push(out: number[], ...points: [number, number, number][]) {
  for (const point of points) out.push(point[0], point[1], point[2]);
}
