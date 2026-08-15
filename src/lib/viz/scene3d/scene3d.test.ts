import { describe, expect, it } from "vitest";
import { lookAt, multiply, project } from "../camera";
import { blocked, eyeTarget, slide, step, RADIUS, type Walker } from "../gallery/walk";
import {
  clampFit,
  defaultFit,
  fitAround,
  flatFit,
  isPhoto,
  moveCorner,
  moveVp,
  photoAspect,
  setFocal,
  DEFAULT_INSET,
  EYE_HEIGHT,
  FOCAL_DEFAULT,
  MARGIN,
  MAX_OBJECTS,
  MIN_GAP,
  type FitObject,
  type SceneFit,
} from "./fit";
import { buildScene, fovOf, projection, MAX_DEPTH, MIN_ROOM, SKIRT } from "./titp";
import { billboardQuads, shadowQuads, surfaceOf } from "./model";
import { medianOf, reliefOf, type DepthGrid } from "./depth";
import { stripWindow, AHEAD, SPARE } from "./strip";
import { healDepth, healPixels } from "./heal";
import { applyFill, bandImage, letterbox, sideOf, FILL_SIDE } from "./fill";

const WIDE = 1.5;

const walker = (x: number, z: number, yaw = 0): Walker => ({ x, z, yaw, pitch: 0, vx: 0, vz: 0 });

function holds(fit: SceneFit): boolean {
  return (
    fit.rect.u0 < fit.vp.u &&
    fit.vp.u < fit.rect.u1 &&
    fit.rect.v0 < fit.vp.v &&
    fit.vp.v < fit.rect.v1 &&
    fit.rect.u0 >= 0 &&
    fit.rect.v0 >= 0 &&
    fit.rect.u1 <= 1 &&
    fit.rect.v1 <= 1
  );
}

describe("the fit", () => {
  it("starts centred, inside the picture, and holding its vanishing point", () => {
    const fit = defaultFit();
    expect(fit.vp).toEqual({ u: 0.5, v: 0.5 });
    expect(fit.rect.u0).toBeCloseTo(0.29, 6);
    expect(fit.rect.v0).toBeCloseTo(0.29, 6);
    expect(fit.rect.u1).toBeCloseTo(0.71, 6);
    expect(fit.rect.v1).toBeCloseTo(0.71, 6);
    expect(holds(fit)).toBe(true);
  });

  it("pulls a vanishing point outside the picture back inside", () => {
    const fit = moveVp(defaultFit(), 1.4, -0.2);
    expect(fit.vp.u).toBeCloseTo(1 - MARGIN, 6);
    expect(fit.vp.v).toBeCloseTo(MARGIN, 6);
    expect(holds(fit)).toBe(true);
  });

  it("grows the back wall when the vanishing point is dragged out of it", () => {
    const fit = moveVp(defaultFit(), 0.9, 0.12);
    expect(fit.rect.u1 - fit.vp.u).toBeGreaterThanOrEqual(MIN_GAP - 1e-9);
    expect(fit.vp.v - fit.rect.v0).toBeGreaterThanOrEqual(MIN_GAP - 1e-9);
    expect(holds(fit)).toBe(true);
  });

  it("will not let a corner cross the vanishing point", () => {
    const fit = moveCorner(defaultFit(), "tl", 0.9, 0.9);
    expect(fit.rect.u0).toBeLessThanOrEqual(fit.vp.u - MIN_GAP + 1e-9);
    expect(fit.rect.v0).toBeLessThanOrEqual(fit.vp.v - MIN_GAP + 1e-9);
    expect(holds(fit)).toBe(true);

    const wide = moveCorner(defaultFit(), "br", 0.1, 0.1);
    expect(wide.rect.u1).toBeGreaterThanOrEqual(wide.vp.u + MIN_GAP - 1e-9);
    expect(holds(wide)).toBe(true);
  });

  it("settles hostile input in one pass and never moves again", () => {
    const hostile: SceneFit[] = [
      { vp: { u: Number.NaN, v: 0.5 }, rect: { u0: 0, v0: 0, u1: 1, v1: 1 }, focal: 1, objects: [] },
      {
        vp: { u: 0.5, v: 0.5 },
        rect: { u0: 0.8, v0: 0.7, u1: 0.2, v1: 0.1 },
        focal: FOCAL_DEFAULT,
        objects: [],
      },
      {
        vp: { u: Infinity, v: -Infinity },
        rect: { u0: Number.NaN, v0: 5, u1: -3, v1: Number.NaN },
        focal: Number.NaN,
        objects: [],
      },
      { vp: { u: 0.5, v: 0.5 }, rect: { u0: 0.5, v0: 0.5, u1: 0.5, v1: 0.5 }, focal: 0, objects: [] },
      {
        vp: { u: 0.94, v: 0.06 },
        rect: { u0: 0.95, v0: 0.05, u1: 0.96, v1: 0.04 },
        focal: 1e9,
        objects: [],
      },
    ];

    for (const fit of hostile) {
      const once = clampFit(fit);
      const twice = clampFit(once);
      expect(twice).toEqual(once);
      expect(holds(once)).toBe(true);
      expect(Number.isFinite(once.focal)).toBe(true);
    }
  });

  it("keeps only usable objects and caps the list", () => {
    const fit = clampFit({
      ...defaultFit(),
      objects: [
        { label: "  Person ", u0: 0.4, v0: 0.3, u1: 0.5, v1: 0.8 },
        { label: "", u0: 0.1, v0: 0.1, u1: 0.2, v1: 0.2 },
        { label: "nothing", u0: 0.3, v0: 0.3, u1: 0.3, v1: 0.3 },
        { label: "everything", u0: 0, v0: 0, u1: 1, v1: 1 },
        { label: "broken", u0: Number.NaN, v0: 0, u1: 1, v1: 1 },
        ...Array.from({ length: 9 }, (_, i) => ({
          label: `thing${i}`,
          u0: 0.1,
          v0: 0.4,
          u1: 0.2,
          v1: 0.7,
        })),
      ],
    });

    expect(fit.objects).toHaveLength(MAX_OBJECTS);
    expect(fit.objects[0]).toEqual({ label: "person", u0: 0.4, v0: 0.3, u1: 0.5, v1: 0.8 });
  });

  it("knows a photograph from a video", () => {
    expect(isPhoto("a.JPG")).toBe(true);
    expect(isPhoto("holiday.tiff")).toBe(true);
    expect(isPhoto("clip.mp4")).toBe(false);
    expect(isPhoto("noextension")).toBe(false);
  });

  it("swaps the aspect for a quarter turn", () => {
    expect(photoAspect(3000, 2000, "none")).toBeCloseTo(1.5, 6);
    expect(photoAspect(3000, 2000, "rotate90")).toBeCloseTo(2 / 3, 6);
    expect(photoAspect(3000, 2000, "flip_h")).toBeCloseTo(1.5, 6);
    expect(photoAspect(3000, 0, "none")).toBe(1);
  });
});

describe("the room the picture makes", () => {
  it("turns the default fit into a plausible room", () => {
    const scene = buildScene(defaultFit(), WIDE);
    expect(scene.depth).toBeCloseTo(10.93, 2);
    expect(scene.bounds.x0).toBeCloseTo(-2.55, 2);
    expect(scene.bounds.x1).toBeCloseTo(2.55, 2);
    expect(scene.bounds.y0).toBe(0);
    expect(scene.bounds.y1).toBeCloseTo(3.4, 2);
    expect(scene.eyeHeight).toBeCloseTo(EYE_HEIGHT, 6);
    expect(scene.fovY).toBeCloseTo(0.709, 3);
    expect(scene.warnings).toEqual([]);
  });

  it("reproduces the photograph from where you start", () => {
    const scene = buildScene(defaultFit(), WIDE);
    const { eye, at } = eyeTarget(
      walker(scene.spawn.x, scene.spawn.z, scene.spawn.yaw),
      scene.eyeHeight,
    );
    const mvp = multiply(
      projection(scene, scene.aspect, 0.05, scene.depth * 4),
      lookAt(eye, at),
    );

    const walls = scene.faces.filter((face) => !face.skirt);
    expect(walls).toHaveLength(5);

    for (const face of walls) {
      face.corners.forEach((corner, index) => {
        const seen = project([corner.x, corner.y, corner.z], mvp);
        expect(seen.visible).toBe(true);
        expect(seen.x).toBeCloseTo(face.uvs[index * 2], 4);
        expect(seen.y).toBeCloseTo(face.uvs[index * 2 + 1], 4);
      });
    }
  });

  it("reproduces the photograph when the vanishing point is off centre", () => {
    const scene = buildScene(fitAround({ u: 0.32, v: 0.61 }, 0.35), 0.75);
    const { eye, at } = eyeTarget(
      walker(scene.spawn.x, scene.spawn.z, scene.spawn.yaw),
      scene.eyeHeight,
    );
    const mvp = multiply(
      projection(scene, scene.aspect, 0.05, scene.depth * 4),
      lookAt(eye, at),
    );

    for (const face of scene.faces.filter((f) => !f.skirt)) {
      face.corners.forEach((corner, index) => {
        const seen = project([corner.x, corner.y, corner.z], mvp);
        expect(seen.x).toBeCloseTo(face.uvs[index * 2], 4);
        expect(seen.y).toBeCloseTo(face.uvs[index * 2 + 1], 4);
      });
    }
  });

  it("keeps the middle of the picture in the middle of any window", () => {
    for (const fit of [defaultFit(), fitAround({ u: 0.28, v: 0.66 }, 0.3)]) {
      const scene = buildScene(fit, WIDE);
      const { eye, at } = eyeTarget(
        walker(scene.spawn.x, scene.spawn.z, scene.spawn.yaw),
        scene.eyeHeight,
      );

      const t = scene.depth / scene.focal;
      const middle: [number, number, number] = [
        (0.5 - scene.fit.vp.u) * scene.aspect * t,
        (scene.fit.vp.v - 0.5) * t + scene.eyeHeight,
        -scene.focal * t,
      ];

      for (const canvasAspect of [WIDE, 0.5, 1, 3.2]) {
        const mvp = multiply(
          projection(scene, canvasAspect, 0.05, scene.depth * 4),
          lookAt(eye, at),
        );
        const seen = project(middle, mvp);
        expect(seen.x).toBeCloseTo(0.5, 4);
        expect(seen.y).toBeCloseTo(0.5, 4);
      }
    }
  });

  it("stands the floor at nought and the eye above it", () => {
    const scene = buildScene(defaultFit(), WIDE);
    const floor = scene.faces.find((face) => face.key === "floor" && !face.skirt);
    expect(floor?.corners.every((corner) => Math.abs(corner.y) < 1e-6)).toBe(true);
    expect(scene.bounds.y1).toBeGreaterThan(scene.eyeHeight);
  });

  it("is symmetric when the picture and the fit are", () => {
    const scene = buildScene(defaultFit(), 1);
    expect(scene.bounds.x0).toBeCloseTo(-scene.bounds.x1, 6);
    expect(scene.spawn.x).toBe(0);
  });

  it("widens with the picture", () => {
    const narrow = buildScene(defaultFit(), 1);
    const wide = buildScene(defaultFit(), 4);
    expect(wide.bounds.x1).toBeCloseTo(narrow.bounds.x1 * 4, 5);
    expect(wide.depth).toBeCloseTo(narrow.depth, 6);
  });

  it("digs deeper as the back wall shrinks", () => {
    let last = 0;
    for (const inset of [0.7, 0.5, 0.3, 0.15, 0.06]) {
      const scene = buildScene(fitAround({ u: 0.5, v: 0.5 }, inset), WIDE);
      expect(scene.depth).toBeGreaterThan(last);
      expect(scene.depth).toBeLessThanOrEqual(MAX_DEPTH);
      last = scene.depth;
    }
  });

  it("keeps every corner finite and every wall inside the picture", () => {
    for (const fit of [defaultFit(), fitAround({ u: 0.2, v: 0.8 }, 0.2), flatFit(defaultFit())]) {
      const scene = buildScene(fit, WIDE);
      for (const face of scene.faces) {
        for (const corner of face.corners) {
          expect(Number.isFinite(corner.x)).toBe(true);
          expect(Number.isFinite(corner.y)).toBe(true);
          expect(Number.isFinite(corner.z)).toBe(true);
        }
        for (const value of face.uvs) {
          expect(value).toBeGreaterThanOrEqual(0);
          expect(value).toBeLessThanOrEqual(1);
        }
      }
    }
  });

  it("smears the outermost pixels forward so you never stand over a void", () => {
    const scene = buildScene(defaultFit(), WIDE);
    const skirts = scene.faces.filter((face) => face.skirt);
    expect(skirts.map((face) => face.key).sort()).toEqual([
      "ceiling",
      "floor",
      "left",
      "right",
    ]);

    const floor = skirts.find((face) => face.key === "floor");
    expect(floor?.corners[0].z).toBeCloseTo(SKIRT, 6);
    expect(floor?.corners[1].z).toBeCloseTo(SKIRT, 6);
    expect(floor?.uvs.filter((_, i) => i % 2 === 1).every((v) => v === 1)).toBe(true);
  });

  it("warns rather than refuses when the picture has no depth in it", () => {
    const scene = buildScene(flatFit(defaultFit()), WIDE);
    expect(scene.warnings.length).toBeGreaterThan(0);
    expect(scene.faces.length).toBeGreaterThan(0);
    expect(Number.isFinite(scene.depth)).toBe(true);
    expect(scene.depth).toBeGreaterThan(0);
  });

  it("builds the same numbers twice", () => {
    const one = surfaceOf(buildScene(defaultFit(), WIDE));
    const two = surfaceOf(buildScene(defaultFit(), WIDE));
    expect(Array.from(one.position)).toEqual(Array.from(two.position));
    expect(Array.from(one.uv)).toEqual(Array.from(two.uv));
    expect(one.count).toBe(two.count);
  });

  it("opens wider as the lens is shortened", () => {
    expect(fovOf(FOCAL_DEFAULT)).toBeCloseTo(0.709, 3);
    expect(fovOf(0.6)).toBeGreaterThan(fovOf(3.2));
    const scene = buildScene(setFocal(defaultFit(), 0.6), WIDE);
    expect(scene.fovY).toBeCloseTo(fovOf(0.6), 6);
  });
});

describe("cut-outs standing in the room", () => {
  const standing = (overrides: Partial<FitObject> = {}): FitObject => ({
    label: "person",
    u0: 0.42,
    v0: 0.55,
    u1: 0.5,
    v1: 0.85,
    ...overrides,
  });

  const withObjects = (objects: FitObject[]) =>
    buildScene(clampFit({ ...defaultFit(), objects }), WIDE);

  it("stands an object on the floor between you and the back wall", () => {
    const scene = withObjects([standing()]);
    expect(scene.billboards).toHaveLength(1);

    const board = scene.billboards[0];
    expect(-board.z).toBeGreaterThan(0);
    expect(-board.z).toBeLessThan(scene.depth);
    expect(board.width).toBeGreaterThan(0);
    expect(board.height).toBeGreaterThan(0);
    expect(board.x).toBeGreaterThan(scene.bounds.x0);
    expect(board.x).toBeLessThan(scene.bounds.x1);
  });

  it("drops an object whose feet are above the horizon", () => {
    expect(withObjects([standing({ v0: 0.2, v1: 0.4 })]).billboards).toHaveLength(0);
  });

  it("drops an object standing behind the back wall", () => {
    expect(withObjects([standing({ v0: 0.52, v1: 0.66 })]).billboards).toHaveLength(0);
  });

  it("gives each cut-out a solid you cannot walk through", () => {
    const scene = withObjects([standing()]);
    const board = scene.billboards[0];
    expect(scene.solids).toHaveLength(5);
    expect(blocked(board.x, board.z, scene.solids)).toBe(true);
    expect(blocked(scene.spawn.x, scene.spawn.z, scene.solids)).toBe(false);
  });

  it("never blocks the spot you start on, however near the feet are drawn", () => {
    for (const v1 of [0.72, 0.8, 0.9, 0.99, 0.999]) {
      const scene = withObjects([standing({ u0: 0.44, u1: 0.56, v0: 0.55, v1 })]);
      expect(blocked(scene.spawn.x, scene.spawn.z, scene.solids)).toBe(false);
    }
  });

  it("draws two triangles a side with the picture's own coordinates", () => {
    const scene = withObjects([standing()]);
    const quads = billboardQuads(scene);
    expect(quads.count).toBe(6);
    for (let i = 0; i < quads.uv.length; i += 1) {
      expect(quads.uv[i]).toBeGreaterThanOrEqual(0);
      expect(quads.uv[i]).toBeLessThanOrEqual(1);
    }
    expect(shadowQuads(scene).count).toBe(6);
  });
});

describe("the depth relief", () => {
  const flatGrid = (width: number, height: number, value: number): DepthGrid => ({
    width,
    height,
    data: new Uint8Array(width * height).fill(value),
  });

  it("turns an even reading into an even wall", () => {
    const scene = buildScene(defaultFit(), WIDE);
    const surface = reliefOf(scene, flatGrid(8, 8, 128));
    expect(surface).not.toBeNull();
    expect(surface!.count).toBe(7 * 7 * 2 * 3);

    const depths: number[] = [];
    for (let i = 0; i < surface!.count; i += 1) depths.push(-surface!.position[i * 3 + 2]);
    const first = depths[0];
    for (const depth of depths) expect(depth).toBeCloseTo(first, 4);
    expect(first).toBeGreaterThan(0);
  });

  it("tears the triangles that straddle a step and keeps the rest", () => {
    const scene = buildScene(defaultFit(), WIDE);
    const width = 8;
    const height = 8;
    const data = new Uint8Array(width * height);
    for (let row = 0; row < height; row += 1) {
      for (let column = 0; column < width; column += 1) {
        data[row * width + column] = column < 4 ? 40 : 220;
      }
    }

    const whole = reliefOf(scene, flatGrid(width, height, 128))!;
    const stepped = reliefOf(scene, { width, height, data })!;

    const straddling = (height - 1) * 2;
    expect(whole.count).toBe((width - 1) * (height - 1) * 2 * 3);
    expect(stepped.count).toBe(whole.count - straddling * 3);
  });

  it("keeps every relief vertex finite and inside the picture", () => {
    const scene = buildScene(defaultFit(), WIDE);
    const data = new Uint8Array(16 * 16);
    for (let i = 0; i < data.length; i += 1) data[i] = (i * 7) % 256;

    const surface = reliefOf(scene, { width: 16, height: 16, data })!;
    for (let i = 0; i < surface.count; i += 1) {
      expect(Number.isFinite(surface.position[i * 3])).toBe(true);
      expect(Number.isFinite(surface.position[i * 3 + 1])).toBe(true);
      expect(Number.isFinite(surface.position[i * 3 + 2])).toBe(true);
      expect(surface.uv[i * 2]).toBeGreaterThanOrEqual(0);
      expect(surface.uv[i * 2]).toBeLessThanOrEqual(1);
      expect(surface.uv[i * 2 + 1]).toBeGreaterThanOrEqual(0);
      expect(surface.uv[i * 2 + 1]).toBeLessThanOrEqual(1);
    }
  });

  it("lies flat against the room at no strength and stands out at full", () => {
    const scene = buildScene(defaultFit(), WIDE);
    const data = new Uint8Array(8 * 8);
    for (let i = 0; i < data.length; i += 1) data[i] = i < 32 ? 60 : 200;

    const flat = reliefOf(scene, { width: 8, height: 8, data }, 0)!;
    for (let i = 0; i < flat.count; i += 1) {
      expect(-flat.position[i * 3 + 2]).toBeCloseTo(scene.depth, 4);
    }

    const full = reliefOf(scene, { width: 8, height: 8, data }, 1)!;
    const spread = new Set<number>();
    for (let i = 0; i < full.count; i += 1) spread.add(Math.round(-full.position[i * 3 + 2] * 100));
    expect(spread.size).toBeGreaterThan(1);
  });

  it("refuses a grid too small or too short to mesh", () => {
    const scene = buildScene(defaultFit(), WIDE);
    expect(reliefOf(scene, flatGrid(1, 8, 100))).toBeNull();
    expect(reliefOf(scene, { width: 8, height: 8, data: new Uint8Array(4) })).toBeNull();
  });

  it("finds the middle of a reading without sorting it", () => {
    expect(medianOf(new Uint8Array([0, 0, 255, 255]))).toBeCloseTo(0, 5);
    expect(medianOf(new Uint8Array([128]))).toBeCloseTo(128 / 255, 5);
    expect(medianOf(new Uint8Array([]))).toBe(0);
  });
});

describe("walking the room", () => {
  it("lets you stand where you start", () => {
    const scene = buildScene(defaultFit(), WIDE);
    expect(blocked(scene.spawn.x, scene.spawn.z, scene.solids)).toBe(false);
  });

  it("holds you inside the four walls however far you push", () => {
    const scene = buildScene(defaultFit(), WIDE);
    const runs: [number, number][] = [
      [-500, 0],
      [500, 0],
      [0, -500],
      [0, 500],
    ];

    for (const [dx, dz] of runs) {
      const moved = slide(scene.spawn.x, scene.spawn.z, dx, dz, scene.solids);
      expect(moved.x).toBeGreaterThan(scene.bounds.x0 - RADIUS);
      expect(moved.x).toBeLessThan(scene.bounds.x1 + RADIUS);
      expect(moved.z).toBeGreaterThan(scene.bounds.z0 - RADIUS);
      expect(moved.z).toBeLessThan(scene.bounds.z1 + RADIUS);
    }
  });

  it("leaves the middle of the room clear all the way to the back wall", () => {
    const scene = buildScene(defaultFit(), WIDE);
    for (let z = scene.spawn.z; z > scene.bounds.z0 + 0.6; z -= 0.25) {
      expect(blocked(0, z, scene.solids)).toBe(false);
    }
  });

  it("cannot be shoved through a wall by a long frame", () => {
    const scene = buildScene(defaultFit(), WIDE);
    let where = walker(scene.spawn.x, scene.spawn.z);
    for (let i = 0; i < 12; i += 1) {
      where = step(where, { forward: 0, strafe: 1, running: true }, scene.solids, 10);
    }
    expect(where.x).toBeLessThan(scene.bounds.x1 + RADIUS);
    expect(blocked(where.x, where.z, scene.solids)).toBe(false);
  });

  it("eases the collision walls, not the picture, when the room is a slot", () => {
    const fit = moveCorner(moveCorner(defaultFit(), "tl", 0.46, 0.29), "br", 0.54, 0.71);
    const scene = buildScene(fit, 0.2);
    expect(scene.bounds.x1 - scene.bounds.x0).toBeLessThan(MIN_ROOM);
    expect(blocked(0, scene.spawn.z, scene.solids)).toBe(false);
    expect(scene.warnings.some((line) => line.includes("corridor"))).toBe(true);
  });
});

describe("The filmstrip", () => {
  const ITEM = 76;

  it("shows only what fits on the screen out of a plan of sixty thousand pictures", () => {
    const seen = stripWindow(61000, 0, 900, ITEM);
    expect(seen.from).toBe(0);
    expect(seen.to).toBe(Math.ceil(900 / ITEM) + SPARE);
    expect(seen.to - seen.from).toBeLessThan(30);
  });

  it("follows the scroll and keeps a few pictures ready on either side", () => {
    const seen = stripWindow(61000, 100 * ITEM, 900, ITEM);
    expect(seen.from).toBe(100 - AHEAD);
    expect(seen.to).toBe(100 - AHEAD + Math.ceil(900 / ITEM) + SPARE);
  });

  it("never asks for a picture that is not there", () => {
    for (const count of [0, 1, 5, 23, 61000]) {
      for (const left of [0, 500, 12345, count * ITEM * 2]) {
        const seen = stripWindow(count, left, 900, ITEM);
        expect(seen.from).toBeGreaterThanOrEqual(0);
        expect(seen.to).toBeLessThanOrEqual(count);
        expect(seen.to).toBeGreaterThanOrEqual(seen.from);
      }
    }
  });

  it("shows the whole strip when the whole strip fits", () => {
    expect(stripWindow(9, 0, 900, ITEM)).toEqual({ from: 0, to: 9 });
  });

  it("shows nothing rather than something wrong when the numbers are nonsense", () => {
    expect(stripWindow(0, 0, 900, ITEM)).toEqual({ from: 0, to: 0 });
    expect(stripWindow(10, 0, 900, 0)).toEqual({ from: 0, to: 0 });
    expect(stripWindow(10, Number.NaN, Number.NaN, ITEM)).toEqual({ from: 0, to: SPARE });
    expect(stripWindow(Number.NaN, 0, 900, ITEM)).toEqual({ from: 0, to: 0 });
  });
});

describe("Healing what the foreground hides", () => {
  const FAR = 40;
  const NEAR = 200;

  function edge(width: number, height: number, at: number): DepthGrid {
    const data = new Uint8Array(width * height);
    for (let row = 0; row < height; row += 1) {
      for (let column = 0; column < width; column += 1) {
        data[row * width + column] = column < at ? FAR : NEAR;
      }
    }
    return { width, height, data };
  }

  it("leaves a picture with nothing in front of anything alone", () => {
    const flat: DepthGrid = { width: 8, height: 8, data: new Uint8Array(64).fill(120) };
    const healed = healDepth(flat);
    expect(healed.filled).toBe(0);
    expect(Array.from(healed.mask)).toEqual(Array.from(new Uint8Array(64)));
    expect(Array.from(healed.healed.data)).toEqual(Array.from(flat.data));
  });

  it("continues the background behind the near edge instead of leaving a hole", () => {
    const grid = edge(40, 4, 20);
    const { healed, mask, filled } = healDepth(grid, 6);

    expect(filled).toBeGreaterThan(0);
    for (let column = 20; column < 26; column += 1) {
      expect(healed.data[column]).toBe(FAR);
      expect(mask[column]).toBe(255);
    }
  });

  it("reaches no further into the foreground than the band allows", () => {
    const grid = edge(40, 4, 20);
    const { healed, mask } = healDepth(grid, 6);

    for (let column = 26; column < 40; column += 1) {
      expect(healed.data[column]).toBe(NEAR);
      expect(mask[column]).toBe(0);
    }
  });

  it("never paints the foreground into the space behind it", () => {
    const grid = edge(24, 3, 12);
    const { source } = healDepth(grid, 5);

    const pixels = new Uint8ClampedArray(24 * 3 * 4);
    for (let i = 0; i < 24 * 3; i += 1) {
      const near = grid.data[i] === NEAR;
      pixels[i * 4] = near ? 255 : 0;
      pixels[i * 4 + 1] = 0;
      pixels[i * 4 + 2] = near ? 0 : 255;
      pixels[i * 4 + 3] = 255;
    }

    const painted = healPixels(pixels, source);
    for (let i = 0; i < 24 * 3; i += 1) {
      if (source[i] === i) continue;
      expect(painted[i * 4]).toBe(0);
      expect(painted[i * 4 + 2]).toBe(255);
    }
  });

  it("leaves the picture itself untouched where nothing was hidden", () => {
    const grid = edge(24, 3, 12);
    const { source } = healDepth(grid, 5);
    const pixels = new Uint8ClampedArray(24 * 3 * 4).fill(77);
    expect(Array.from(healPixels(pixels, source))).toEqual(Array.from(pixels));
  });

  it("keeps every healed reading inside the grid it came from", () => {
    const grid = edge(32, 5, 16);
    const { source, healed } = healDepth(grid, 8);
    expect(healed.data).toHaveLength(32 * 5);
    for (let i = 0; i < source.length; i += 1) {
      expect(source[i]).toBeGreaterThanOrEqual(0);
      expect(source[i]).toBeLessThan(32 * 5);
      expect(healed.data[i]).toBe(grid.data[source[i]]);
    }
  });

  it("survives a grid too small or too broken to hold a silhouette", () => {
    for (const grid of [
      { width: 1, height: 1, data: new Uint8Array([9]) },
      { width: 4, height: 4, data: new Uint8Array(3) },
    ] as DepthGrid[]) {
      const healed = healDepth(grid);
      expect(healed.filled).toBe(0);
      expect(healed.healed.width).toBe(grid.width);
    }
  });

  it("gives the room behind the tear a mesh of its own", () => {
    const scene = buildScene(defaultFit(), WIDE);
    const grid = edge(24, 24, 12);
    const behind = reliefOf(scene, healDepth(grid, 6).healed, 1);
    const front = reliefOf(scene, grid, 1);

    expect(behind).not.toBeNull();
    expect(front).not.toBeNull();
    expect(behind!.count).toBeGreaterThan(0);
    for (let i = 0; i < behind!.position.length; i += 1) {
      expect(Number.isFinite(behind!.position[i])).toBe(true);
    }
  });
});

describe("Handing the hole to a filling service", () => {
  const BAND_MASK = new Uint8Array([0, 255, 0, 255]);

  it("reads the side of the picture it should send", () => {
    expect(sideOf("1024x1024")).toBe(1024);
    expect(sideOf("512x512")).toBe(512);
  });

  it("falls back rather than sending a size no service would take", () => {
    for (const size of ["", "auto", "32x32", "not a size"]) {
      expect(sideOf(size)).toBe(FILL_SIDE);
    }
  });

  it("fits the picture inside the square without stretching it", () => {
    const box = letterbox(300, 200, 1024);
    expect(box.width).toBe(1024);
    expect(box.height).toBe(683);
    expect(box.width / box.height).toBeCloseTo(300 / 200, 2);
  });

  it("centres what it cannot fill", () => {
    const box = letterbox(300, 200, 1024);
    expect(box.x).toBe(0);
    expect(box.y).toBe(Math.floor((1024 - box.height) / 2));
    expect(box.y + box.height).toBeLessThanOrEqual(1024);

    const tall = letterbox(200, 300, 512);
    expect(tall.height).toBe(512);
    expect(tall.x).toBe(Math.floor((512 - tall.width) / 2));
  });

  it("never spills outside the square, whatever it is given", () => {
    for (const [width, height] of [
      [1, 4000],
      [4000, 1],
      [1024, 1024],
      [7, 3],
    ]) {
      const box = letterbox(width, height, 512);
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.y).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(512);
      expect(box.y + box.height).toBeLessThanOrEqual(512);
    }
  });

  it("asks for nothing when there is no picture to send", () => {
    for (const [width, height, side] of [
      [0, 10, 512],
      [10, 0, 512],
      [10, 10, 0],
      [Number.NaN, 10, 512],
      [10, 10, Number.POSITIVE_INFINITY],
    ]) {
      expect(letterbox(width, height, side).width).toBe(0);
    }
  });

  it("marks the band and only the band for the service to paint", () => {
    const band = bandImage(BAND_MASK);
    expect(band.length).toBe(16);
    expect(Array.from(band.slice(0, 4))).toEqual([0, 0, 0, 0]);
    expect(Array.from(band.slice(4, 8))).toEqual([255, 255, 255, 255]);
    expect(Array.from(band.slice(8, 12))).toEqual([0, 0, 0, 0]);
  });

  it("takes the painted band and leaves the photograph alone everywhere else", () => {
    const base = new Uint8ClampedArray([
      10, 10, 10, 255, 20, 20, 20, 255, 30, 30, 30, 255, 40, 40, 40, 255,
    ]);
    const filled = new Uint8ClampedArray(16).fill(200);
    const out = applyFill(base, filled, BAND_MASK);

    expect(Array.from(out.slice(0, 4))).toEqual([10, 10, 10, 255]);
    expect(Array.from(out.slice(4, 8))).toEqual([200, 200, 200, 255]);
    expect(Array.from(out.slice(8, 12))).toEqual([30, 30, 30, 255]);
    expect(Array.from(out.slice(12, 16))).toEqual([200, 200, 200, 255]);
  });

  it("makes the painted band solid even if the service sent it through", () => {
    const base = new Uint8ClampedArray(8).fill(50);
    const filled = new Uint8ClampedArray([1, 2, 3, 0, 4, 5, 6, 0]);
    const out = applyFill(base, filled, new Uint8Array([0, 255]));
    expect(out[7]).toBe(255);
  });

  it("paints only where the healing reached", () => {
    const grid: DepthGrid = { width: 40, height: 4, data: new Uint8Array(160) };
    for (let row = 0; row < 4; row += 1) {
      for (let column = 0; column < 40; column += 1) {
        grid.data[row * 40 + column] = column < 20 ? 40 : 200;
      }
    }

    const { mask } = healDepth(grid, 6);
    const base = new Uint8ClampedArray(160 * 4).fill(90);
    const filled = new Uint8ClampedArray(160 * 4).fill(7);
    const out = applyFill(base, filled, mask);

    for (let i = 0; i < 160; i += 1) {
      expect(out[i * 4]).toBe(mask[i] === 0 ? 90 : 7);
    }
  });

  it("cannot be handed a shorter answer than it asked for and run off the end", () => {
    const base = new Uint8ClampedArray(16).fill(90);
    const out = applyFill(base, new Uint8ClampedArray(4), new Uint8Array([255, 255, 255, 255]));
    expect(out.length).toBe(16);
    expect(out[12]).toBe(90);
  });
});
