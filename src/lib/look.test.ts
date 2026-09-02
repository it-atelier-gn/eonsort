import { describe, expect, it } from "vitest";
import { cleanLook, cleanTile, perRow, TILE_SIZES } from "./look";

describe("Remembering how the file list is shown", () => {
  it("falls back to the details it has always shown", () => {
    expect(cleanLook(null)).toBe("details");
    expect(cleanLook("gallery")).toBe("details");
    expect(cleanLook("details")).toBe("details");
    expect(cleanLook("thumbnails")).toBe("thumbnails");
  });

  it("takes only a tile size it offers", () => {
    for (const size of TILE_SIZES) {
      expect(cleanTile(size.edge)).toBe(size.edge);
    }
    expect(cleanTile(37)).toBe(TILE_SIZES[1].edge);
    expect(cleanTile("large")).toBe(TILE_SIZES[1].edge);
    expect(cleanTile(null)).toBe(TILE_SIZES[1].edge);
  });
});

describe("Fitting tiles across the pane", () => {
  it("counts the tiles that fit, gaps and all", () => {
    expect(perRow(400, 96, 8)).toBe(3);
    expect(perRow(320, 96, 8)).toBe(3);
    expect(perRow(303, 96, 8)).toBe(2);
  });

  it("always leaves room for one, however narrow the pane", () => {
    expect(perRow(10, 224, 8)).toBe(1);
    expect(perRow(0, 96, 8)).toBe(1);
    expect(perRow(400, 0, 8)).toBe(1);
  });
});
