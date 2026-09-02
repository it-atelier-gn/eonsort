import { describe, expect, it } from "vitest";
import { fittedTo } from "$lib/fit";

const room = { width: 300, height: 220 };

describe("Fitting a picture into the space there is for it", () => {
  it("gives a tall picture the height of the room and no more width than it needs", () => {
    const box = fittedTo({ width: 3000, height: 4000 }, room, false)!;
    expect(box.height).toBeCloseTo(220);
    expect(box.width).toBeCloseTo(165);
    expect(box.width).toBeLessThan(room.width);
  });

  it("holds a wide picture back to whichever edge runs out first", () => {
    const box = fittedTo({ width: 4000, height: 3000 }, room, false)!;
    expect(box.height).toBeCloseTo(220);
    expect(box.width).toBeCloseTo(293.33, 1);
    expect(box.width).toBeLessThanOrEqual(room.width);
  });

  it("keeps the shape the picture came with", () => {
    const box = fittedTo({ width: 1600, height: 900 }, room, false)!;
    expect(box.width / box.height).toBeCloseTo(1600 / 900);
  });

  it("leaves a small picture at its own size rather than blowing it up", () => {
    expect(fittedTo({ width: 40, height: 30 }, room, false)).toEqual({ width: 40, height: 30 });
  });

  it("measures against the turned room when the picture is laid on its side", () => {
    const upright = fittedTo({ width: 4000, height: 3000 }, room, false)!;
    const sideways = fittedTo({ width: 4000, height: 3000 }, room, true)!;
    expect(sideways.width).toBeCloseTo(220);
    expect(sideways.height).toBeCloseTo(165);
    expect(sideways.width).toBeLessThan(upright.width);
  });

  it("has nothing to say about a picture or a room with no size", () => {
    expect(fittedTo({ width: 0, height: 10 }, room, false)).toBeNull();
    expect(fittedTo({ width: 10, height: 0 }, room, false)).toBeNull();
    expect(fittedTo({ width: 10, height: 10 }, { width: 0, height: 0 }, false)).toBeNull();
  });
});
