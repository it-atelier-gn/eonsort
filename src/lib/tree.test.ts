import { describe, expect, it } from "vitest";
import { buildTree, folderKey, isLeafFolder } from "./tree";
import { baseName, formatBytes } from "./api";

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
