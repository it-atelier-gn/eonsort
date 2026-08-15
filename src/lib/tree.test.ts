import { describe, expect, it } from "vitest";
import { buildTree, folderKey, foldersOf, isLeafFolder, under } from "./tree";
import { baseName, formatBytes } from "./api";
import type { EntryView } from "./api";

function entry(folder: string, size = 10): EntryView {
  return {
    source: `${folder}/${size}/${Math.random()}`,
    destination: "",
    name: "a.jpg",
    folder,
    taken: "2023-05-06 10:11:12",
    taken_epoch: 0,
    provider: "exif",
    provider_info: null,
    size,
    destination_exists: false,
    outcome: null,
    candidates: [],
    flags: [],
    confidence: "high",
    override_origin: null,
    orientation: 1,
    rotate: "none",
    rotate_by_hand: false,
    rotate_lossless: true,
    reencode: false,
    subject: null,
    tags: [],
    caption: null,
  };
}

describe("buildTree", () => {
  it("nests year and month folders", () => {
    const tree = buildTree([
      { path: "2023/05", files: 2, bytes: 200 },
      { path: "2023/06", files: 1, bytes: 100 },
      { path: "2019/11", files: 4, bytes: 400 },
    ]);

    expect(tree.map((n) => n.name)).toEqual(["2019", "2023"]);
    const y2023 = tree[1];
    expect(y2023.files).toBe(3);
    expect(y2023.bytes).toBe(300);
    expect(y2023.children.map((n) => n.path)).toEqual(["2023/05", "2023/06"]);
  });

  it("rolls counts up through every level", () => {
    const tree = buildTree([{ path: "2023/05/06", files: 7, bytes: 70 }]);
    expect(tree[0].files).toBe(7);
    expect(tree[0].children[0].files).toBe(7);
    expect(tree[0].children[0].children[0].files).toBe(7);
  });

  it("gives files at the destination root their own node", () => {
    const tree = buildTree([{ path: "", files: 1, bytes: 10 }]);
    expect(tree).toHaveLength(1);
    expect(folderKey(tree[0].path)).toBe("");
    expect(isLeafFolder(tree[0])).toBe(true);
  });

  it("sorts month folders numerically", () => {
    const tree = buildTree([
      { path: "2023/10", files: 1, bytes: 1 },
      { path: "2023/02", files: 1, bytes: 1 },
    ]);
    expect(tree[0].children.map((n) => n.name)).toEqual(["02", "10"]);
  });

  it("returns nothing for an empty plan", () => {
    expect(buildTree([])).toEqual([]);
  });
});

describe("Counting the folders the files actually land in", () => {
  it("totals files and bytes per folder", () => {
    const folders = foldersOf([entry("2023/05", 100), entry("2023/05", 50), entry("2019/11", 7)]);
    expect(folders).toEqual([
      { path: "2019/11", files: 1, bytes: 7 },
      { path: "2023/05", files: 2, bytes: 150 },
    ]);
  });

  it("counts nothing for an empty scope, so the tree empties with it", () => {
    expect(foldersOf([])).toEqual([]);
  });

  it("feeds buildTree, so a narrowed scope prunes whole years away", () => {
    const tree = buildTree(foldersOf([entry("2023/05"), entry("2023/06")]));
    expect(tree.map((n) => n.name)).toEqual(["2023"]);
    expect(tree[0].files).toBe(2);
  });
});

describe("Picking a month or a whole year out of the tree", () => {
  const pool = [entry("2023/05"), entry("2023/06"), entry("2019/11"), entry("")];

  it("shows every file under a year, not just the ones filed directly in it", () => {
    expect(under(pool, "2023")).toHaveLength(2);
  });

  it("shows the files of a single month", () => {
    expect(under(pool, "2023/05")).toHaveLength(1);
  });

  it("does not confuse a year with one whose name merely starts the same", () => {
    expect(under([entry("2023"), entry("20230")], "2023")).toHaveLength(1);
  });

  it("keeps the destination root to itself", () => {
    expect(under(pool, "")).toHaveLength(1);
  });

  it("shows nothing when no folder is picked", () => {
    expect(under(pool, null)).toEqual([]);
  });
});

describe("formatting helpers", () => {
  it("formats byte counts", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(50 * 1024 * 1024)).toBe("50 MB");
  });

  it("takes the last segment of a path on both platforms", () => {
    expect(baseName("C:\\photos\\a.jpg")).toBe("a.jpg");
    expect(baseName("/home/me/a.jpg")).toBe("a.jpg");
  });
});
