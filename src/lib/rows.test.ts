import { describe, expect, it } from "vitest";
import { nextRow, nextTile } from "./rows";

describe("nextRow", () => {
  it("walks down and up one row at a time", () => {
    expect(nextRow("ArrowDown", 0, 3)).toBe(1);
    expect(nextRow("ArrowUp", 2, 3)).toBe(1);
  });

  it("stays put at either end", () => {
    expect(nextRow("ArrowUp", 0, 3)).toBeNull();
    expect(nextRow("ArrowDown", 2, 3)).toBeNull();
  });

  it("jumps to the first and last row", () => {
    expect(nextRow("Home", 2, 3)).toBe(0);
    expect(nextRow("End", 0, 3)).toBe(2);
  });

  it("ignores keys that are not navigation", () => {
    expect(nextRow("a", 1, 3)).toBeNull();
    expect(nextRow("ArrowDown", 0, 0)).toBeNull();
  });
});

describe("nextTile", () => {
  it("steps along a row and back", () => {
    expect(nextTile("ArrowRight", 0, 9, 4)).toBe(1);
    expect(nextTile("ArrowLeft", 3, 9, 4)).toBe(2);
  });

  it("drops a whole row down and lifts one up", () => {
    expect(nextTile("ArrowDown", 1, 9, 4)).toBe(5);
    expect(nextTile("ArrowUp", 5, 9, 4)).toBe(1);
  });

  it("settles on the last tile rather than falling off the bottom row", () => {
    expect(nextTile("ArrowDown", 6, 9, 4)).toBe(8);
    expect(nextTile("ArrowDown", 8, 9, 4)).toBeNull();
  });

  it("stays put at either end", () => {
    expect(nextTile("ArrowUp", 2, 9, 4)).toBeNull();
    expect(nextTile("ArrowLeft", 0, 9, 4)).toBeNull();
    expect(nextTile("ArrowRight", 8, 9, 4)).toBeNull();
  });

  it("jumps to the first and last tile", () => {
    expect(nextTile("Home", 5, 9, 4)).toBe(0);
    expect(nextTile("End", 0, 9, 4)).toBe(8);
  });

  it("copes with one tile to a row and with nothing to walk", () => {
    expect(nextTile("ArrowDown", 0, 3, 1)).toBe(1);
    expect(nextTile("ArrowDown", 0, 3, 0)).toBe(1);
    expect(nextTile("ArrowDown", 0, 0, 4)).toBeNull();
    expect(nextTile("a", 1, 9, 4)).toBeNull();
  });
});
