import { describe, expect, it } from "vitest";
import type { Transform } from "./api";
import { TRANSFORM_CSS, describeTransform, forOrientation, swapsAxes, turn } from "./rotate";

const ALL: Transform[] = [
  "none",
  "rotate90",
  "rotate180",
  "rotate270",
  "flip_h",
  "flip_v",
  "transpose",
  "transverse",
];

describe("reading the orientation tag", () => {
  it("maps every orientation to the turn that rights it", () => {
    expect(forOrientation(1)).toBe("none");
    expect(forOrientation(2)).toBe("flip_h");
    expect(forOrientation(3)).toBe("rotate180");
    expect(forOrientation(4)).toBe("flip_v");
    expect(forOrientation(5)).toBe("transpose");
    expect(forOrientation(6)).toBe("rotate90");
    expect(forOrientation(7)).toBe("transverse");
    expect(forOrientation(8)).toBe("rotate270");
  });

  it("treats anything it does not recognise as upright", () => {
    expect(forOrientation(0)).toBe("none");
    expect(forOrientation(9)).toBe("none");
  });
});

describe("turning", () => {
  it("comes back to the start after four turns to the right", () => {
    for (const start of ALL) {
      let turned = start;
      for (let i = 0; i < 4; i += 1) turned = turn(turned, 1);
      expect(turned).toBe(start);
    }
  });

  it("undoes a turn to the right with a turn to the left", () => {
    for (const start of ALL) {
      expect(turn(turn(start, 1), -1)).toBe(start);
    }
  });

  it("keeps a mirror that is already there", () => {
    expect(turn("flip_h", 1)).toBe("transverse");
    expect(turn("flip_h", 2)).toBe("flip_v");
    expect(turn("flip_h", 3)).toBe("transpose");
    expect(turn("transpose", 1)).toBe("flip_h");
  });

  it("turns a plain picture the way you would expect", () => {
    expect(turn("none", 1)).toBe("rotate90");
    expect(turn("none", 2)).toBe("rotate180");
    expect(turn("none", -1)).toBe("rotate270");
  });
});

describe("laying the preview out", () => {
  it("knows which turns swap the sides of the picture", () => {
    expect(swapsAxes("rotate90")).toBe(true);
    expect(swapsAxes("rotate270")).toBe(true);
    expect(swapsAxes("transpose")).toBe(true);
    expect(swapsAxes("transverse")).toBe(true);

    expect(swapsAxes("none")).toBe(false);
    expect(swapsAxes("rotate180")).toBe(false);
    expect(swapsAxes("flip_h")).toBe(false);
    expect(swapsAxes("flip_v")).toBe(false);
  });

  it("has css and words for every turn", () => {
    for (const transform of ALL) {
      expect(TRANSFORM_CSS[transform]).toBeTruthy();
      expect(describeTransform(transform)).toBeTruthy();
    }
  });
});
