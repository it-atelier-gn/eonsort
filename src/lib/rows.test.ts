import { describe, expect, it } from "vitest";
import { nextRow } from "./rows";

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
