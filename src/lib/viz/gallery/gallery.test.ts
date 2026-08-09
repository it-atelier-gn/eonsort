import { describe, expect, it } from "vitest";
import type { EntryView } from "$lib/api";
import {
  buildGallery,
  groupIntoPeriods,
  hangingSlots,
  roomAt,
  roomDepth,
  thinTo,
  DOOR_WIDTH,
  MAX_DEPTH,
  MAX_ROOMS,
  MIN_DEPTH,
  ROOM_WIDTH,
} from "./layout";
import { buildPaneQuads, buildRoomMesh, buildShaftQuads } from "./geometry";
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
    const periods = groupIntoPeriods(entries);
    expect(periods[0].entries).toEqual([1, 0]);
  });

  it("grows a room for a busy year but keeps it inside sane bounds", () => {
    expect(roomDepth(1)).toBe(MIN_DEPTH);
    expect(roomDepth(400)).toBe(MAX_DEPTH);
    expect(roomDepth(20)).toBeGreaterThan(roomDepth(6));
  });

  it("never hangs more pictures than the walls hold", () => {
    const gallery = buildGallery(years(["2019", 500]));
    const room = gallery.rooms[0];
    expect(room.files).toBe(500);
    expect(room.hung).toBeLessThanOrEqual(hangingSlots(room.z1 - room.z0));
    expect(room.frames).toHaveLength(room.hung);
    expect(room.hung).toBeGreaterThan(0);
  });

  it("keeps every picture inside its own room and flat against a wall", () => {
    const gallery = buildGallery(years(["2003", 6], ["2019", 30]));
    const half = ROOM_WIDTH / 2;

    for (const room of gallery.rooms) {
      for (const frame of room.frames) {
        expect(Math.abs(frame.x)).toBeLessThan(half);
        expect(Math.abs(frame.x)).toBeGreaterThan(half - 0.5);
        expect(frame.z).toBeGreaterThanOrEqual(room.z0);
        expect(frame.z).toBeLessThanOrEqual(room.z1);
        expect(Math.sign(frame.facing)).toBe(-Math.sign(frame.x));
      }
    }
  });

  it("hangs on both walls rather than crowding one", () => {
    const gallery = buildGallery(years(["2019", 12]));
    const left = gallery.rooms[0].frames.filter((f) => f.x < 0).length;
    const right = gallery.rooms[0].frames.filter((f) => f.x > 0).length;
    expect(Math.abs(left - right)).toBeLessThanOrEqual(1);
  });

  it("caps how many rooms are built at all", () => {
    const many: [string, number][] = [];
    for (let year = 1970; year < 2030; year += 1) many.push([String(year), 2]);
    expect(buildGallery(years(...many)).rooms.length).toBe(MAX_ROOMS);
  });

  it("leaves a doorway between neighbouring rooms and seals both ends", () => {
    const gallery = buildGallery(years(["2003", 4], ["2019", 4]));
    const [first, second] = gallery.rooms;

    const between = gallery.solids.filter((s) => s.z0 >= first.z1 && s.z1 <= second.z0);
    expect(between).toHaveLength(2);
    expect(between.some((s) => s.x1 === -DOOR_WIDTH / 2)).toBe(true);
    expect(between.some((s) => s.x0 === DOOR_WIDTH / 2)).toBe(true);

    expect(blocked(0, (first.z1 + second.z0) / 2, gallery.solids)).toBe(false);

    const back = slide(0, first.z0 + 1, 0, -20, gallery.solids);
    expect(back.z).toBeGreaterThan(-1);
    const out = slide(0, gallery.depth - 1, 0, 20, gallery.solids);
    expect(out.z).toBeLessThan(gallery.depth + 1);
    const through = slide(0, first.z0 + 2, 0, gallery.depth, gallery.solids);
    expect(through.z).toBeGreaterThan(second.z0);
  });

  it("leaves the middle of the gallery clear to walk end to end", () => {
    const gallery = buildGallery(years(["2003", 8], ["2019", 30], ["2021", 6]));
    const start = gallery.rooms[0].z0 + 1;

    for (let z = start; z < gallery.depth - 0.6; z += 0.25) {
      expect(blocked(0, z, gallery.solids)).toBe(false);
    }

    const walked = slide(0, start, 0, gallery.depth, gallery.solids);
    expect(walked.z).toBeGreaterThan(gallery.rooms[2].z0);
  });

  it("cannot be squeezed through a side wall", () => {
    const gallery = buildGallery(years(["2019", 8]));
    const escape = slide(0, gallery.rooms[0].z0 + 3, 500, 0, gallery.solids);
    expect(Math.abs(escape.x)).toBeLessThan(ROOM_WIDTH / 2);
  });

  it("puts furniture in the room and makes it solid", () => {
    const gallery = buildGallery(years(["2019", 20]));
    const room = gallery.rooms[0];
    const kinds = new Set(room.furniture.map((piece) => piece.kind));

    expect(kinds).toContain("bench");
    expect(kinds).toContain("plinth");
    expect(kinds).toContain("planter");
    for (const piece of room.furniture) {
      expect(blocked(piece.x, piece.z, gallery.solids)).toBe(true);
    }
  });

  it("puts windows on both walls of every room", () => {
    const gallery = buildGallery(years(["2003", 4], ["2019", 40]));
    for (const room of gallery.rooms) {
      expect(room.panes.some((pane) => pane.x < 0)).toBe(true);
      expect(room.panes.some((pane) => pane.x > 0)).toBe(true);
      for (const pane of room.panes) {
        expect(pane.z).toBeGreaterThanOrEqual(room.z0);
        expect(pane.z).toBeLessThanOrEqual(room.z1);
      }
    }
  });

  it("says which room a spot belongs to", () => {
    const gallery = buildGallery(years(["2003", 4], ["2019", 4]));
    expect(roomAt(gallery, gallery.rooms[0].z0 + 1)?.label).toBe("2003");
    expect(roomAt(gallery, gallery.rooms[1].z0 + 1)?.label).toBe("2019");
    expect(roomAt(gallery, 10_000)).toBeNull();
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
  const gallery = buildGallery(years(["2003", 6], ["2019", 24]));

  it("builds a mesh with one normal and one shade per vertex", () => {
    const mesh = buildRoomMesh(gallery);
    expect(mesh.count).toBeGreaterThan(0);
    expect(mesh.position).toHaveLength(mesh.count * 3);
    expect(mesh.normal).toHaveLength(mesh.count * 3);
    expect(mesh.shade).toHaveLength(mesh.count);
    expect([...mesh.position].every(Number.isFinite)).toBe(true);
  });

  it("builds two triangles for every window and every light shaft", () => {
    const panes = gallery.rooms.reduce((sum, room) => sum + room.panes.length, 0);
    expect(buildPaneQuads(gallery).count).toBe(panes * 6);
    expect(buildShaftQuads(gallery).count).toBe(panes * 6);
  });

  it("keeps every wall inside the building", () => {
    const mesh = buildRoomMesh(gallery);
    for (let i = 0; i < mesh.count; i += 1) {
      expect(Math.abs(mesh.position[i * 3])).toBeLessThanOrEqual(ROOM_WIDTH);
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
    const result = step(
      { ...walker, x: 0 },
      { forward: 0, strafe: 1, running: true },
      wall,
      10,
    );
    expect(result.x).toBeLessThan(2);
  });
});
