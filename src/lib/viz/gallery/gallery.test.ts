import { describe, expect, it } from "vitest";
import type { EntryView } from "$lib/api";
import {
  buildGallery,
  corridorLamps,
  groupIntoPeriods,
  hangingSlots,
  holds,
  lampsFor,
  nearestLamps,
  overlaps,
  randomFrom,
  roomAt,
  roomSize,
  runsOf,
  seedFrom,
  thinTo,
  ART_PITCH,
  DOOR_HEIGHT,
  DOOR_WIDTH,
  MAX_ROOMS,
  MAX_SIDE,
  MIN_SIDE,
  ROOM_HEIGHT,
  WALL,
  type Gallery,
  type Solid,
} from "./layout";
import { buildPaneQuads, buildRoomMesh, buildShaftQuads, type Mesh } from "./geometry";
import { blocked, eyeTarget, look, slide, step, MAX_PITCH, RADIUS } from "./walk";

const at = (iso: string) => Date.parse(`${iso}Z`) / 1000;

function entry(overrides: Partial<EntryView> = {}): EntryView {
  return {
    source: "/src/a.jpg",
    destination: "/out/2003/01/a.jpg",
    name: "a.jpg",
    folder: "2003/01",
    taken: "2003-01-01 00:00:00",
    taken_epoch: at("2003-01-01T00:00:00"),
    provider: "exif",
    provider_info: null,
    size: 1,
    destination_exists: false,
    outcome: null,
    candidates: [],
    flags: [],
    confidence: "low",
    override_origin: null,
    orientation: 0,
    rotate: "none",
    rotate_by_hand: false,
    rotate_lossless: true,
    reencode: false,
    subject: null,
    tags: [],
    caption: null,
    ...overrides,
  };
}

const years = (...list: [string, number][]) =>
  list.flatMap(([year, count]) =>
    Array.from({ length: count }, (_, i) =>
      entry({
        source: `/src/${year}-${i}.jpg`,
        taken_epoch: at(`${year}-06-0${(i % 9) + 1}T10:00:00`),
      }),
    ),
  );

function reaches(gallery: Gallery, from: { x: number; z: number }, to: { x: number; z: number }) {
  const grid = 0.5;
  const key = (x: number, z: number) => `${Math.round(x / grid)}:${Math.round(z / grid)}`;
  const seen = new Set<string>([key(from.x, from.z)]);
  const queue = [{ x: from.x, z: from.z }];

  while (queue.length > 0) {
    const spot = queue.shift()!;
    if (Math.hypot(spot.x - to.x, spot.z - to.z) < 1.2) return true;

    for (const [dx, dz] of [
      [grid, 0],
      [-grid, 0],
      [0, grid],
      [0, -grid],
    ]) {
      const x = spot.x + dx;
      const z = spot.z + dz;
      const id = key(x, z);
      if (seen.has(id)) continue;
      if (!holds(gallery.bounds, x, z, 2)) continue;
      if (blocked(x, z, gallery.solids)) continue;
      seen.add(id);
      queue.push({ x, z });
    }
  }
  return false;
}

const centreOf = (area: Solid) => ({ x: (area.x0 + area.x1) / 2, z: (area.z0 + area.z1) / 2 });

function facesAt(mesh: Mesh, axis: 0 | 1 | 2, at: number, direction: number): number {
  let found = 0;
  for (let i = 0; i < mesh.count; i += 1) {
    if (Math.abs(mesh.position[i * 3 + axis] - Math.fround(at)) > 1e-3) continue;
    if (Math.abs(mesh.normal[i * 3 + axis] - direction) > 1e-6) continue;
    found += 1;
  }
  return found;
}

describe("gallery layout", () => {
  it("gives each year its own room, oldest first", () => {
    const periods = groupIntoPeriods(years(["2021", 2], ["2003", 1], ["2019", 3]));
    expect(periods.map((p) => p.label)).toEqual(["2003", "2019", "2021"]);
    expect(periods[2].entries).toHaveLength(2);
  });

  it("orders the files inside a room by when they were taken", () => {
    const entries = [
      entry({ source: "/b", taken_epoch: at("2019-08-01T00:00:00") }),
      entry({ source: "/a", taken_epoch: at("2019-02-01T00:00:00") }),
    ];
    expect(groupIntoPeriods(entries)[0].entries).toEqual([1, 0]);
  });

  it("grows a room for a busy year but keeps it inside sane bounds", () => {
    expect(roomSize(1, 1).width).toBe(MIN_SIDE);
    expect(roomSize(400, 1).width).toBe(MAX_SIDE);
    expect(roomSize(30, 1).width).toBeGreaterThan(roomSize(8, 1).width);
    expect(roomSize(30, 1.4).width).toBeGreaterThan(roomSize(30, 1.4).depth);
  });

  it("draws the same building twice for the same files, another for other files", () => {
    const entries = years(["2003", 8], ["2019", 30], ["2021", 12]);
    const once = buildGallery(entries);
    const again = buildGallery(entries);
    const other = buildGallery(years(["2003", 9], ["2019", 31], ["2021", 13]));

    expect(once.rooms.map((room) => [room.x0, room.z0])).toEqual(
      again.rooms.map((room) => [room.x0, room.z0]),
    );
    expect(other.rooms.map((room) => [room.x0, room.z0])).not.toEqual(
      once.rooms.map((room) => [room.x0, room.z0]),
    );
    expect(seedFrom(entries)).toBe(seedFrom(entries));
  });

  it("hands out the same numbers for the same seed", () => {
    const first = randomFrom(7);
    const second = randomFrom(7);
    const drawn = [first(), first(), first()];
    expect(drawn).toEqual([second(), second(), second()]);
    expect(drawn.every((value) => value >= 0 && value < 1)).toBe(true);
  });

  it("never lets two rooms stand in the same place", () => {
    const gallery = buildGallery(
      years(["2015", 40], ["2016", 8], ["2017", 90], ["2018", 20], ["2019", 60], ["2020", 12]),
    );
    for (let i = 0; i < gallery.rooms.length; i += 1) {
      for (let j = i + 1; j < gallery.rooms.length; j += 1) {
        expect(overlaps(gallery.rooms[i], gallery.rooms[j], WALL)).toBe(false);
      }
    }
  });

  it("does not lay the rooms out in one straight line", () => {
    const gallery = buildGallery(
      years(["2014", 30], ["2015", 40], ["2016", 20], ["2017", 60], ["2018", 25], ["2019", 45]),
    );
    const axes = new Set(gallery.corridors.map((one) => one.axis));
    expect(gallery.corridors).toHaveLength(gallery.rooms.length - 1);
    expect(axes.size).toBeGreaterThan(1);
    const spreadX = Math.max(...gallery.rooms.map((room) => room.x1)) -
      Math.min(...gallery.rooms.map((room) => room.x0));
    const spreadZ = Math.max(...gallery.rooms.map((room) => room.z1)) -
      Math.min(...gallery.rooms.map((room) => room.z0));
    expect(Math.min(spreadX, spreadZ)).toBeGreaterThan(MIN_SIDE);
  });

  it("lets you walk from the first room to every other one", () => {
    const gallery = buildGallery(years(["2016", 24], ["2017", 60], ["2018", 12], ["2019", 40]));
    for (const room of gallery.rooms) {
      expect(reaches(gallery, gallery.start, centreOf(room))).toBe(true);
    }
  });

  it("keeps every wall solid to walk into", () => {
    const gallery = buildGallery(years(["2019", 30], ["2020", 20]));
    expect(gallery.walls.length).toBeGreaterThan(gallery.rooms.length * 4);
    for (const wall of gallery.walls) {
      if (wall.y0 > 0) continue;
      expect(blocked((wall.x0 + wall.x1) / 2, (wall.z0 + wall.z1) / 2, gallery.solids)).toBe(true);
    }
    const room = gallery.rooms[0];
    const escape = slide(centreOf(room).x, centreOf(room).z, 500, 0, gallery.solids);
    expect(escape.x).toBeLessThan(room.x1 + WALL * 2);
  });

  it("cuts a doorway wide enough to pass through", () => {
    const gallery = buildGallery(years(["2019", 30], ["2020", 20]));
    const corridor = gallery.corridors[0];
    expect(Math.min(corridor.x1 - corridor.x0, corridor.z1 - corridor.z0)).toBeCloseTo(
      DOOR_WIDTH,
      6,
    );
    expect(blocked(centreOf(corridor).x, centreOf(corridor).z, gallery.solids)).toBe(false);
    expect(reaches(gallery, gallery.start, centreOf(gallery.rooms[1]))).toBe(true);
  });

  it("puts a lintel over each doorway rather than leaving a slot to the ceiling", () => {
    const gallery = buildGallery(years(["2019", 30], ["2020", 20]));
    const lintels = gallery.walls.filter((wall) => wall.y0 === DOOR_HEIGHT);
    expect(lintels.length).toBeGreaterThanOrEqual(2);
    for (const lintel of lintels) expect(lintel.y1).toBe(ROOM_HEIGHT);
  });

  it("hangs pictures flat on a wall and never across a doorway", () => {
    const gallery = buildGallery(years(["2018", 40], ["2019", 70], ["2020", 25]));

    for (const room of gallery.rooms) {
      expect(room.frames).toHaveLength(room.hung);
      expect(room.hung).toBeGreaterThan(0);
      expect(room.hung).toBeLessThanOrEqual(hangingSlots(room.runs));

      for (const frame of room.frames) {
        expect(holds(room, frame.x, frame.z, 0.1)).toBe(true);
        expect(Math.abs(frame.nx) + Math.abs(frame.nz)).toBe(1);
        const toWall =
          frame.nx !== 0
            ? Math.min(Math.abs(frame.x - room.x0), Math.abs(frame.x - room.x1))
            : Math.min(Math.abs(frame.z - room.z0), Math.abs(frame.z - room.z1));
        expect(toWall).toBeLessThan(0.2);
      }
    }
  });

  it("hangs on more than one wall of a room", () => {
    const gallery = buildGallery(years(["2019", 60]));
    const walls = new Set(gallery.rooms[0].frames.map((frame) => `${frame.nx}:${frame.nz}`));
    expect(walls.size).toBeGreaterThan(1);
  });

  it("leaves the wall beside a door long enough to hang on", () => {
    const room = { x0: 0, x1: 20, z0: 0, z1: 12 };
    const runs = runsOf(room, [{ side: 0, from: 8, to: 8 + DOOR_WIDTH }]);
    const north = runs.filter((run) => run.nz === -1);

    expect(north).toHaveLength(2);
    expect(north[0].length).toBeCloseTo(8, 6);
    expect(north[1].length).toBeCloseTo(20 - 8 - DOOR_WIDTH, 6);
    for (const run of runs) expect(run.length).toBeGreaterThan(ART_PITCH);
  });

  it("caps how many rooms are built at all", () => {
    const many: [string, number][] = [];
    for (let year = 1970; year < 2030; year += 1) many.push([String(year), 2]);
    expect(buildGallery(years(...many)).rooms.length).toBe(MAX_ROOMS);
  });

  it("puts furniture in the room and makes it solid", () => {
    const gallery = buildGallery(years(["2019", 40]));
    const room = gallery.rooms[0];
    expect(room.furniture.some((piece) => piece.kind === "plinth")).toBe(true);
    expect(room.furniture.some((piece) => piece.kind === "bench")).toBe(true);
    for (const piece of room.furniture) {
      expect(blocked(piece.x, piece.z, gallery.solids)).toBe(true);
      expect(holds(room, piece.x, piece.z)).toBe(true);
    }
  });

  it("hangs lamps in every room, in the corridors, and over the pictures", () => {
    const gallery = buildGallery(years(["2018", 40], ["2019", 70]));

    for (const room of gallery.rooms) {
      expect(room.lamps.length).toBeGreaterThan(0);
      for (const lamp of room.lamps) {
        expect(holds(room, lamp.x, lamp.z, 1)).toBe(true);
        expect(lamp.y).toBeGreaterThan(DOOR_HEIGHT - 1.5);
        expect(lamp.y).toBeLessThan(ROOM_HEIGHT);
        expect(lamp.strength).toBeGreaterThan(0);
      }
    }

    const inCorridors = gallery.corridors.flatMap(corridorLamps);
    expect(inCorridors.length).toBeGreaterThanOrEqual(gallery.corridors.length);
    expect(gallery.lamps.length).toBeGreaterThan(inCorridors.length);
  });

  it("lights a big room with more lamps than a small one", () => {
    const small = lampsFor({ x0: 0, x1: 9, z0: 0, z1: 9 }, []);
    const large = lampsFor({ x0: 0, x1: 34, z0: 0, z1: 34 }, []);
    expect(large.length).toBeGreaterThan(small.length);
  });

  it("hands the shader only the lamps closest to the eye", () => {
    const gallery = buildGallery(years(["2018", 40], ["2019", 70], ["2020", 30]));
    const near = nearestLamps(gallery.lamps, gallery.start.x, gallery.start.z, 8);

    expect(near).toHaveLength(8);
    const reach = (lamp: (typeof near)[number]) =>
      Math.hypot(lamp.x - gallery.start.x, lamp.z - gallery.start.z);
    expect(reach(near[0])).toBeLessThanOrEqual(reach(near[7]));
    for (const lamp of gallery.lamps) {
      if (near.includes(lamp)) continue;
      expect(reach(lamp)).toBeGreaterThanOrEqual(reach(near[0]) - 1e-9);
    }
  });

  it("puts windows high on the walls of every room", () => {
    const gallery = buildGallery(years(["2003", 8], ["2019", 40]));
    for (const room of gallery.rooms) {
      expect(room.panes.length).toBeGreaterThan(0);
      for (const pane of room.panes) {
        expect(holds(room, pane.x, pane.z, 0.1)).toBe(true);
        expect(pane.width).toBeGreaterThan(0);
        expect(pane.y).toBeGreaterThan(DOOR_HEIGHT);
      }
    }
  });

  it("says which room a spot belongs to", () => {
    const gallery = buildGallery(years(["2003", 8], ["2019", 8]));
    expect(roomAt(gallery, centreOf(gallery.rooms[0]).x, centreOf(gallery.rooms[0]).z)?.label).toBe(
      "2003",
    );
    expect(roomAt(gallery, centreOf(gallery.rooms[1]).x, centreOf(gallery.rooms[1]).z)?.label).toBe(
      "2019",
    );
    expect(roomAt(gallery, 10_000, 10_000)).toBeNull();
  });

  it("starts you standing inside the first room", () => {
    const gallery = buildGallery(years(["2003", 8], ["2019", 8]));
    expect(holds(gallery.rooms[0], gallery.start.x, gallery.start.z)).toBe(true);
    expect(blocked(gallery.start.x, gallery.start.z, gallery.solids)).toBe(false);
  });

  it("has nothing to show for an empty plan", () => {
    const gallery = buildGallery([]);
    expect(gallery.rooms).toHaveLength(0);
    expect(gallery.frames).toHaveLength(0);
    expect(buildRoomMesh(gallery).count).toBe(0);
  });

  it("keeps a sample spread across the whole list when thinning", () => {
    const kept = thinTo([0, 1, 2, 3, 4, 5, 6, 7, 8, 9], 4);
    expect(kept).toHaveLength(4);
    expect(kept[0]).toBe(0);
    expect(kept[3]).toBeGreaterThan(5);
    expect(thinTo([1, 2], 9)).toEqual([1, 2]);
  });
});

describe("gallery geometry", () => {
  const gallery = buildGallery(years(["2003", 12], ["2019", 40], ["2020", 18]));

  it("builds a mesh with one normal and one shade per vertex", () => {
    const mesh = buildRoomMesh(gallery);
    expect(mesh.count).toBeGreaterThan(0);
    expect(mesh.position).toHaveLength(mesh.count * 3);
    expect(mesh.normal).toHaveLength(mesh.count * 3);
    expect(mesh.shade).toHaveLength(mesh.count);
    expect([...mesh.position].every(Number.isFinite)).toBe(true);
  });

  it("closes every wall, so none of them is a one-sided sheet", () => {
    const mesh = buildRoomMesh(gallery);

    for (const wall of gallery.walls) {
      expect(facesAt(mesh, 0, wall.x0, -1)).toBeGreaterThanOrEqual(6);
      expect(facesAt(mesh, 0, wall.x1, 1)).toBeGreaterThanOrEqual(6);
      expect(facesAt(mesh, 2, wall.z0, -1)).toBeGreaterThanOrEqual(6);
      expect(facesAt(mesh, 2, wall.z1, 1)).toBeGreaterThanOrEqual(6);
      expect(facesAt(mesh, 1, wall.y1, 1)).toBeGreaterThanOrEqual(6);
      expect(facesAt(mesh, 1, wall.y0, -1)).toBeGreaterThanOrEqual(6);
    }
  });

  it("winds every triangle to face the way its normal points", () => {
    const mesh = buildRoomMesh(gallery);

    for (let triangle = 0; triangle < mesh.count / 3; triangle += 1) {
      const at = triangle * 9;
      const a = [mesh.position[at], mesh.position[at + 1], mesh.position[at + 2]];
      const b = [mesh.position[at + 3], mesh.position[at + 4], mesh.position[at + 5]];
      const c = [mesh.position[at + 6], mesh.position[at + 7], mesh.position[at + 8]];
      const normal = [mesh.normal[at], mesh.normal[at + 1], mesh.normal[at + 2]];

      const ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
      const ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
      const cross = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
      ];
      const facing = cross[0] * normal[0] + cross[1] * normal[1] + cross[2] * normal[2];
      expect(facing).toBeGreaterThanOrEqual(0);
    }
  });

  it("gives a doorway a soffit you can see from underneath", () => {
    const mesh = buildRoomMesh(gallery);
    const lintel = gallery.walls.find((wall) => wall.y0 === DOOR_HEIGHT)!;
    expect(facesAt(mesh, 1, lintel.y0, -1)).toBeGreaterThanOrEqual(6);
  });

  it("floors and roofs every room and every corridor", () => {
    const mesh = buildRoomMesh(gallery);
    for (const room of gallery.rooms) {
      expect(facesAt(mesh, 1, ROOM_HEIGHT, -1)).toBeGreaterThanOrEqual(6);
      expect(holds(room, room.x0 + 0.1, room.z0 + 0.1)).toBe(true);
    }
    expect(facesAt(mesh, 1, 0, 1)).toBeGreaterThanOrEqual(
      (gallery.rooms.length + gallery.corridors.length) * 6,
    );
  });

  it("builds two triangles for every window and every light shaft", () => {
    const panes = gallery.rooms.reduce((sum, room) => sum + room.panes.length, 0);
    expect(buildPaneQuads(gallery).count).toBe(panes * 6);
    expect(buildShaftQuads(gallery).count).toBe(panes * 6);
  });

  it("keeps every wall inside the building", () => {
    const mesh = buildRoomMesh(gallery);
    for (let i = 0; i < mesh.count; i += 1) {
      expect(holds(gallery.bounds, mesh.position[i * 3], mesh.position[i * 3 + 2], 0.5)).toBe(true);
      expect(mesh.position[i * 3 + 1]).toBeGreaterThanOrEqual(0);
    }
  });
});

describe("walking", () => {
  const walker = { x: 0, z: 5, yaw: 0, pitch: 0, vx: 0, vz: 0 };
  const wall = [{ x0: 2, x1: 3, z0: 0, z1: 10 }];

  it("walks forward along its own facing", () => {
    const moved = step(walker, { forward: 1, strafe: 0, running: false }, [], 0.1);
    expect(moved.z).toBeLessThan(walker.z);
    expect(moved.x).toBeCloseTo(0, 5);
  });

  it("runs faster than it walks", () => {
    const intent = { forward: 1, strafe: 0, running: false };
    const walked = step(walker, intent, [], 0.1);
    const ran = step(walker, { ...intent, running: true }, [], 0.1);
    expect(walker.z - ran.z).toBeGreaterThan(walker.z - walked.z);
  });

  it("does not go faster diagonally", () => {
    const straight = step(walker, { forward: 1, strafe: 0, running: false }, [], 0.1);
    const diagonal = step(walker, { forward: 1, strafe: 1, running: false }, [], 0.1);
    const straightSpeed = Math.hypot(straight.x - walker.x, straight.z - walker.z);
    const diagonalSpeed = Math.hypot(diagonal.x - walker.x, diagonal.z - walker.z);
    expect(diagonalSpeed).toBeLessThanOrEqual(straightSpeed + 1e-6);
  });

  it("walks up to a wall and stops there instead of passing through", () => {
    const result = slide(0, 5, 5, 0, wall);
    expect(result.hitX).toBe(true);
    expect(result.x).toBeGreaterThan(0);
    expect(result.x).toBeLessThanOrEqual(2 - RADIUS);
    expect(blocked(2.5, 5, wall)).toBe(true);
    expect(blocked(2 - RADIUS - 0.01, 5, wall)).toBe(false);
  });

  it("slides along a wall rather than sticking to it", () => {
    const result = slide(0, 5, 5, -1, wall);
    expect(result.x).toBeLessThanOrEqual(2 - RADIUS);
    expect(result.z).toBeCloseTo(4, 5);
  });

  it("keeps looking from tipping over backwards", () => {
    const far = look({ ...walker }, 0, -100_000);
    expect(far.pitch).toBeCloseTo(MAX_PITCH, 5);
    const down = look({ ...walker }, 0, 100_000);
    expect(down.pitch).toBeCloseTo(-MAX_PITCH, 5);
  });

  it("looks where it is facing", () => {
    const { eye, at: target } = eyeTarget({ ...walker, yaw: 0 }, 1.7);
    expect(eye).toEqual([0, 1.7, 5]);
    expect(target[2]).toBeLessThan(eye[2]);

    const turned = eyeTarget({ ...walker, yaw: Math.PI / 2 }, 1.7);
    expect(turned.at[0]).toBeLessThan(0);
  });

  it("cannot be shoved through a wall by a huge time step", () => {
    const result = step({ ...walker, x: 0 }, { forward: 0, strafe: 1, running: true }, wall, 10);
    expect(result.x).toBeLessThan(2);
  });
});
