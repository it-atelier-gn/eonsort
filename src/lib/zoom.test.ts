import { describe, expect, it } from "vitest";
import {
  clampScale,
  held,
  isResting,
  pannedBy,
  steppedIn,
  steppedOut,
  transformOf,
  wheelFactor,
  zoomedAt,
  MAX_ZOOM,
  MIN_ZOOM,
  RESTING,
  ZOOM_STEP,
} from "./zoom";

const box = { width: 400, height: 240 };

describe("How far a preview may be zoomed", () => {
  it("stays between all of it and eight times over", () => {
    expect(clampScale(0.1)).toBe(MIN_ZOOM);
    expect(clampScale(99)).toBe(MAX_ZOOM);
    expect(clampScale(Number.NaN)).toBe(MIN_ZOOM);
    expect(clampScale(2.5)).toBe(2.5);
  });

  it("gives a step in and a step out that undo each other", () => {
    const once = steppedIn(RESTING, box);
    expect(once.scale).toBeCloseTo(ZOOM_STEP, 6);
    expect(steppedOut(once, box)).toEqual(RESTING);
  });

  it("falls back to the resting view when it is zoomed all the way out", () => {
    const far = steppedOut(steppedOut(steppedIn(RESTING, box), box), box);
    expect(isResting(far)).toBe(true);
    expect(transformOf(far)).toBe("");
  });
});

describe("Keeping the picture in the frame", () => {
  it("never lets the picture be dragged off the edge", () => {
    const zoomed = { scale: 2, x: 9999, y: -9999 };
    const inside = held(zoomed, box);

    expect(inside.x).toBeCloseTo(box.width / 2, 6);
    expect(inside.y).toBeCloseTo(-box.height / 2, 6);
  });

  it("has no room to pan when the whole picture already fits", () => {
    expect(pannedBy(RESTING, 40, 40, box)).toEqual(RESTING);
    expect(held({ scale: 1, x: 30, y: 30 }, box)).toEqual(RESTING);
  });

  it("pans by the distance dragged while there is room", () => {
    const panned = pannedBy({ scale: 3, x: 0, y: 0 }, 20, -12, box);
    expect(panned.x).toBeCloseTo(20, 6);
    expect(panned.y).toBeCloseTo(-12, 6);
  });
});

describe("Zooming where the pointer is", () => {
  it("holds the spot under the pointer still", () => {
    const at = { x: 60, y: -30 };
    const zoomed = zoomedAt(RESTING, 2, at, { width: 4000, height: 4000 });

    expect(zoomed.scale).toBeCloseTo(2, 6);
    expect(at.x - (at.x - 0) * 2).toBeCloseTo(zoomed.x, 6);
    expect(at.y - (at.y - 0) * 2).toBeCloseTo(zoomed.y, 6);
  });

  it("comes to rest again when it shrinks back to the whole picture", () => {
    const zoomed = zoomedAt(RESTING, 2, { x: 60, y: -30 }, box);
    expect(zoomedAt(zoomed, 0.1, { x: 60, y: -30 }, box)).toEqual(RESTING);
  });

  it("reads the wheel so that scrolling up comes closer", () => {
    expect(wheelFactor(-100)).toBeGreaterThan(1);
    expect(wheelFactor(100)).toBeLessThan(1);
    expect(wheelFactor(0)).toBe(1);
    expect(wheelFactor(Number.NaN)).toBe(1);
  });

  it("writes a transform only once it has moved", () => {
    expect(transformOf(RESTING)).toBe("");
    expect(transformOf({ scale: 2, x: 10, y: -5 })).toBe(
      "translate(10.00px, -5.00px) scale(2.000)",
    );
  });
});
