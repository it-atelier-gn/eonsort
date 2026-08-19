import { describe, expect, it } from "vitest";
import {
  cleanOrder,
  cleanWidths,
  defaultOrder,
  isColumnId,
  moveColumn,
  template,
  widthOf,
  withWidth,
  FILE_COLUMNS,
  MAX_WIDTH,
  MIN_WIDTH,
  TREE_COLUMNS,
  type TreeColumnId,
} from "./columns";

const DEFAULT_ORDER = defaultOrder(TREE_COLUMNS);

describe("Remembering which order the columns sit in", () => {
  it("accepts an order it wrote itself", () => {
    expect(cleanOrder(TREE_COLUMNS, ["size", "name", "files"])).toEqual(["size", "name", "files"]);
  });

  it("falls back to the default when nothing was stored", () => {
    expect(cleanOrder(TREE_COLUMNS, null)).toEqual(DEFAULT_ORDER);
    expect(cleanOrder(TREE_COLUMNS, "files")).toEqual(DEFAULT_ORDER);
  });

  it("drops names it does not know and keeps the rest", () => {
    expect(cleanOrder(TREE_COLUMNS, ["size", "colour", 7, "name"])).toEqual(["size", "name", "files"]);
  });

  it("appends a column that was added since the order was stored", () => {
    expect(cleanOrder(TREE_COLUMNS, ["files"])).toEqual(["files", "name", "size"]);
  });

  it("never lets one column appear twice", () => {
    expect(cleanOrder(TREE_COLUMNS, ["name", "name", "size"])).toEqual(["name", "size", "files"]);
  });

  it("knows a column name when it sees one", () => {
    expect(isColumnId(TREE_COLUMNS, "name")).toBe(true);
    expect(isColumnId(TREE_COLUMNS, "depth")).toBe(false);
  });
});

describe("Dragging a column somewhere else", () => {
  it("puts the dragged column where the target was", () => {
    expect(moveColumn(TREE_COLUMNS, DEFAULT_ORDER, "size", "name")).toEqual(["size", "name", "files"]);
  });

  it("moves a column to the right without losing anything", () => {
    const moved = moveColumn(TREE_COLUMNS, DEFAULT_ORDER, "name", "size");
    expect(moved).toEqual(["files", "size", "name"]);
    expect([...moved].sort()).toEqual([...DEFAULT_ORDER].sort());
  });

  it("leaves the order alone when a column is dropped on itself", () => {
    expect(moveColumn(TREE_COLUMNS, DEFAULT_ORDER, "files", "files")).toEqual(DEFAULT_ORDER);
  });

  it("ignores a column it does not know", () => {
    expect(moveColumn(TREE_COLUMNS, DEFAULT_ORDER, "name", "gone" as TreeColumnId)).toEqual(DEFAULT_ORDER);
  });

  it("repairs a damaged order on the way through", () => {
    expect(moveColumn(TREE_COLUMNS, ["size", "size"] as TreeColumnId[], "name", "size")).toEqual([
      "name",
      "size",
      "files",
    ]);
  });
});

describe("Sizing a column to what is in it", () => {
  it("lets the folder name take the space that is left", () => {
    expect(widthOf(TREE_COLUMNS, "name", ["2023", "a very long folder name indeed"])).toBe(0);
  });

  it("grows a number column with its longest value", () => {
    const narrow = widthOf(TREE_COLUMNS, "files", ["7"]);
    const wide = widthOf(TREE_COLUMNS, "files", ["7", "1284391"]);
    expect(wide).toBeGreaterThan(narrow);
  });

  it("stays wide enough for its own heading", () => {
    expect(widthOf(TREE_COLUMNS, "size", [])).toBeGreaterThanOrEqual(52);
  });

  it("refuses to grow without limit", () => {
    expect(widthOf(TREE_COLUMNS, "size", ["x".repeat(400)])).toBeLessThanOrEqual(140);
  });

  it("builds a grid template in the order given", () => {
    const widths = { name: 0, files: 60, size: 80 };
    expect(template(TREE_COLUMNS, ["size", "name", "files"], widths)).toBe("80px minmax(0, 1fr) 60px");
  });
});

describe("column widths", () => {
  it("keeps only known columns with usable numbers", () => {
    expect(cleanWidths(TREE_COLUMNS, { name: 200, nope: 90, size: "wide", files: NaN })).toEqual({ name: 200 });
  });

  it("survives anything that is not an object", () => {
    expect(cleanWidths(TREE_COLUMNS, null)).toEqual({});
    expect(cleanWidths(TREE_COLUMNS, "120")).toEqual({});
  });

  it("holds a width inside its limits", () => {
    expect(withWidth(TREE_COLUMNS, {}, "files", 4).files).toBe(MIN_WIDTH);
    expect(withWidth(TREE_COLUMNS, {}, "files", 5000).files).toBe(MAX_WIDTH);
    expect(withWidth(TREE_COLUMNS, {}, "files", 120.4).files).toBe(120);
  });

  it("drops a width to go back to fitting the content", () => {
    expect(withWidth(TREE_COLUMNS, { files: 120 }, "files", null)).toEqual({});
  });

  it("leaves the widths it was given alone", () => {
    const held = { files: 120 };
    withWidth(TREE_COLUMNS, held, "size", 90);
    expect(held).toEqual({ files: 120 });
  });

  it("gives a resized folder column a fixed track", () => {
    expect(template(TREE_COLUMNS, DEFAULT_ORDER, { name: 240, files: 60, size: 80 })).toBe("240px 60px 80px");
  });
});

describe("The columns over the file list", () => {
  it("starts with the name first and the rest behind it", () => {
    expect(defaultOrder(FILE_COLUMNS)).toEqual(["name", "date", "from", "size", "status"]);
  });

  it("keeps its own order apart from the one over the tree", () => {
    expect(FILE_COLUMNS.orderKey).not.toBe(TREE_COLUMNS.orderKey);
    expect(FILE_COLUMNS.widthKey).not.toBe(TREE_COLUMNS.widthKey);
  });

  it("knows nothing of a column that belongs to the tree", () => {
    expect(isColumnId(FILE_COLUMNS, "files")).toBe(false);
    expect(isColumnId(FILE_COLUMNS, "status")).toBe(true);
  });

  it("moves a column the same way the tree does", () => {
    expect(moveColumn(FILE_COLUMNS, defaultOrder(FILE_COLUMNS), "status", "name")).toEqual([
      "status",
      "name",
      "date",
      "from",
      "size",
    ]);
  });

  it("lets the file name take the space that is left", () => {
    expect(widthOf(FILE_COLUMNS, "name", ["a very long file name.jpg"])).toBe(0);
    expect(widthOf(FILE_COLUMNS, "date", ["2024-03-11 14:22"])).toBeGreaterThan(0);
  });

  it("builds a grid template in the order given", () => {
    const widths = { name: 0, date: 130, from: 90, size: 80, status: 110 };
    expect(template(FILE_COLUMNS, ["date", "name", "from", "size", "status"], widths)).toBe(
      "130px minmax(0, 1fr) 90px 80px 110px",
    );
  });

  it("forgets a width that belongs to another table", () => {
    expect(cleanWidths(FILE_COLUMNS, { files: 90, date: 120 })).toEqual({ date: 120 });
  });
});
