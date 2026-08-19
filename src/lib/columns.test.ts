import { describe, expect, it } from "vitest";
import {
  cleanOrder,
  cleanWidths,
  isColumnId,
  moveColumn,
  template,
  widthOf,
  withWidth,
  MAX_WIDTH,
  MIN_WIDTH,
  DEFAULT_ORDER,
  type ColumnId,
} from "./columns";

describe("Remembering which order the columns sit in", () => {
  it("accepts an order it wrote itself", () => {
    expect(cleanOrder(["size", "name", "files"])).toEqual(["size", "name", "files"]);
  });

  it("falls back to the default when nothing was stored", () => {
    expect(cleanOrder(null)).toEqual(DEFAULT_ORDER);
    expect(cleanOrder("files")).toEqual(DEFAULT_ORDER);
  });

  it("drops names it does not know and keeps the rest", () => {
    expect(cleanOrder(["size", "colour", 7, "name"])).toEqual(["size", "name", "files"]);
  });

  it("appends a column that was added since the order was stored", () => {
    expect(cleanOrder(["files"])).toEqual(["files", "name", "size"]);
  });

  it("never lets one column appear twice", () => {
    expect(cleanOrder(["name", "name", "size"])).toEqual(["name", "size", "files"]);
  });

  it("knows a column name when it sees one", () => {
    expect(isColumnId("name")).toBe(true);
    expect(isColumnId("depth")).toBe(false);
  });
});

describe("Dragging a column somewhere else", () => {
  it("puts the dragged column where the target was", () => {
    expect(moveColumn(DEFAULT_ORDER, "size", "name")).toEqual(["size", "name", "files"]);
  });

  it("moves a column to the right without losing anything", () => {
    const moved = moveColumn(DEFAULT_ORDER, "name", "size");
    expect(moved).toEqual(["files", "size", "name"]);
    expect([...moved].sort()).toEqual([...DEFAULT_ORDER].sort());
  });

  it("leaves the order alone when a column is dropped on itself", () => {
    expect(moveColumn(DEFAULT_ORDER, "files", "files")).toEqual(DEFAULT_ORDER);
  });

  it("ignores a column it does not know", () => {
    expect(moveColumn(DEFAULT_ORDER, "name", "gone" as ColumnId)).toEqual(DEFAULT_ORDER);
  });

  it("repairs a damaged order on the way through", () => {
    expect(moveColumn(["size", "size"] as ColumnId[], "name", "size")).toEqual([
      "name",
      "size",
      "files",
    ]);
  });
});

describe("Sizing a column to what is in it", () => {
  it("lets the folder name take the space that is left", () => {
    expect(widthOf("name", ["2023", "a very long folder name indeed"])).toBe(0);
  });

  it("grows a number column with its longest value", () => {
    const narrow = widthOf("files", ["7"]);
    const wide = widthOf("files", ["7", "1284391"]);
    expect(wide).toBeGreaterThan(narrow);
  });

  it("stays wide enough for its own heading", () => {
    expect(widthOf("size", [])).toBeGreaterThanOrEqual(52);
  });

  it("refuses to grow without limit", () => {
    expect(widthOf("size", ["x".repeat(400)])).toBeLessThanOrEqual(140);
  });

  it("builds a grid template in the order given", () => {
    const widths = { name: 0, files: 60, size: 80 };
    expect(template(["size", "name", "files"], widths)).toBe("80px minmax(0, 1fr) 60px");
  });
});

describe("column widths", () => {
  it("keeps only known columns with usable numbers", () => {
    expect(cleanWidths({ name: 200, nope: 90, size: "wide", files: NaN })).toEqual({ name: 200 });
  });

  it("survives anything that is not an object", () => {
    expect(cleanWidths(null)).toEqual({});
    expect(cleanWidths("120")).toEqual({});
  });

  it("holds a width inside its limits", () => {
    expect(withWidth({}, "files", 4).files).toBe(MIN_WIDTH);
    expect(withWidth({}, "files", 5000).files).toBe(MAX_WIDTH);
    expect(withWidth({}, "files", 120.4).files).toBe(120);
  });

  it("drops a width to go back to fitting the content", () => {
    expect(withWidth({ files: 120 }, "files", null)).toEqual({});
  });

  it("leaves the widths it was given alone", () => {
    const held = { files: 120 };
    withWidth(held, "size", 90);
    expect(held).toEqual({ files: 120 });
  });

  it("gives a resized folder column a fixed track", () => {
    expect(template(DEFAULT_ORDER, { name: 240, files: 60, size: 80 })).toBe("240px 60px 80px");
  });
});
