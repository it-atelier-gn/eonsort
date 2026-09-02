import { describe, expect, it } from "vitest";
import type { BurstView, DuplicateReport, LookalikeView } from "$lib/api";
import { listed, removableCopies, tallyBursts } from "$lib/copies";

function burst(members: number, extra: number): BurstView {
  return {
    keeper: `/cam/a${members}.jpg`,
    members: Array.from({ length: members }, (_, i) => `/cam/a${i}.jpg`),
    folder: "/cam",
    taken: "2019-07-04 10:00:00",
    extra_bytes: extra,
  };
}

describe("Showing a long list of copies", () => {
  it("says how many it had to leave out", () => {
    const many = Array.from({ length: 12 }, (_, i) => i);
    expect(listed(many, 5)).toEqual({ shown: [0, 1, 2, 3, 4], hidden: 7 });
  });

  it("leaves nothing out when the list fits", () => {
    expect(listed([1, 2, 3], 5)).toEqual({ shown: [1, 2, 3], hidden: 0 });
    expect(listed([], 5)).toEqual({ shown: [], hidden: 0 });
  });
});

describe("Counting what removing the extra copies would take", () => {
  const report = (files: number, groups: number): DuplicateReport => ({
    files,
    wasted: 0,
    groups: Array.from({ length: groups }, (_, i) => ({
      sources: [`/a${i}.jpg`, `/b${i}.jpg`],
      folder: "/",
      bytes: 0,
      wasted: 0,
    })),
  });

  it("keeps one file out of every group", () => {
    expect(removableCopies(report(9, 3))).toBe(6);
  });

  it("has nothing to take before the files have been read", () => {
    expect(removableCopies(null)).toBe(0);
    expect(removableCopies(report(0, 0))).toBe(0);
  });
});

describe("Summing up the bursts", () => {
  it("counts the runs, the files in them and the bytes beyond the first shot", () => {
    expect(tallyBursts([burst(3, 1000), burst(5, 4000)])).toEqual({
      bursts: 2,
      files: 8,
      extra: 5000,
    });
  });

  it("counts nothing when no burst was found", () => {
    expect(tallyBursts([])).toEqual({ bursts: 0, files: 0, extra: 0 });
  });

  it("counts sets of look-alikes the same way", () => {
    const across: LookalikeView[] = [
      { keeper: "/a/x.jpg", members: ["/a/x.jpg", "/b/x.jpg"], folders: 2, extra_bytes: 200 },
    ];
    expect(tallyBursts(across)).toEqual({ bursts: 1, files: 2, extra: 200 });
  });
});
