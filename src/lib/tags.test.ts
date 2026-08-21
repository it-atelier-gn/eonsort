import { describe, expect, it } from "vitest";
import {
  allPicked,
  keepsQuality,
  keepsTags,
  labelOf,
  matching,
  pickedLabel,
  tagCounts,
  withTag,
  UNTAGGED,
  UNTAGGED_LABEL,
} from "./tags";

const entries = [
  { tags: ["a dog", "a forest"] },
  { tags: ["a dog"] },
  { tags: [] },
  { tags: ["a beach", "a dog", "a dog"] },
];

describe("Counting the tags a plan carries", () => {
  it("counts each picture once per tag, commonest first", () => {
    expect(tagCounts(entries)).toEqual([
      { tag: "a dog", count: 3 },
      { tag: "a beach", count: 1 },
      { tag: "a forest", count: 1 },
      { tag: UNTAGGED, count: 1 },
    ]);
  });

  it("keeps the untagged pile out of the list when there is none", () => {
    const counts = tagCounts([{ tags: ["a dog"] }]);
    expect(counts.map((count) => count.tag)).toEqual(["a dog"]);
  });

  it("gives the untagged pile a name of its own", () => {
    expect(labelOf(UNTAGGED)).toBe(UNTAGGED_LABEL);
    expect(labelOf("a dog")).toBe("a dog");
  });

  it("has nothing to count in an empty plan", () => {
    expect(tagCounts([])).toEqual([]);
  });
});

describe("Keeping the pictures a filter asks for", () => {
  it("keeps everything while no choice has been made", () => {
    expect(keepsTags(["a dog"], null)).toBe(true);
    expect(keepsTags([], null)).toBe(true);
  });

  it("keeps a picture carrying any one of the ticked tags", () => {
    expect(keepsTags(["a dog", "a beach"], ["a beach"])).toBe(true);
    expect(keepsTags(["a dog"], ["a beach"])).toBe(false);
  });

  it("keeps the untagged only when the untagged pile is ticked", () => {
    expect(keepsTags([], [UNTAGGED])).toBe(true);
    expect(keepsTags([], ["a dog"])).toBe(false);
  });

  it("keeps nothing once every tick is gone", () => {
    expect(keepsTags(["a dog"], [])).toBe(false);
    expect(keepsTags([], [])).toBe(false);
  });
});

describe("Keeping the pictures a rating asks for", () => {
  it("keeps everything while the bar is on the floor", () => {
    expect(keepsQuality(null, 0)).toBe(true);
    expect(keepsQuality(2.1, 0)).toBe(true);
  });

  it("keeps what the model rated at least as high as the bar", () => {
    expect(keepsQuality(6.4, 6)).toBe(true);
    expect(keepsQuality(5.9, 6)).toBe(false);
    expect(keepsQuality(6, 6)).toBe(true);
  });

  it("drops what was never rated once the bar is raised", () => {
    expect(keepsQuality(null, 5)).toBe(false);
    expect(keepsQuality(undefined, 5)).toBe(false);
    expect(keepsQuality(Number.NaN, 5)).toBe(false);
  });
});

describe("Ticking the tags", () => {
  const counts = tagCounts(entries);

  it("starts with every tag ticked and unticks the one it is handed", () => {
    expect(withTag(null, counts, "a dog")).toEqual(["a beach", "a forest", UNTAGGED]);
  });

  it("ticks a tag that was not ticked", () => {
    expect(withTag(["a dog"], counts, "a beach")).toEqual(["a dog", "a beach"]);
  });

  it("forgets a tag the plan no longer has", () => {
    expect(withTag(["a dog", "a lighthouse"], counts, "a beach")).toEqual(["a dog", "a beach"]);
  });

  it("says when everything is ticked", () => {
    expect(allPicked(null, counts)).toBe(true);
    expect(allPicked(counts.map((count) => count.tag), counts)).toBe(true);
    expect(allPicked(["a dog"], counts)).toBe(false);
  });

  it("says in one line what the filter is holding", () => {
    expect(pickedLabel(null, counts)).toBe("All tags");
    expect(pickedLabel([], counts)).toBe("No tag");
    expect(pickedLabel(["a dog"], counts)).toBe("a dog");
    expect(pickedLabel([UNTAGGED], counts)).toBe(UNTAGGED_LABEL);
    expect(pickedLabel(["a dog", "a beach"], counts)).toBe("2 tags");
  });
});

describe("Finding a tag in a long list", () => {
  const counts = tagCounts(entries);

  it("hands back everything when nothing is typed", () => {
    expect(matching(counts, "  ")).toEqual(counts);
  });

  it("matches part of a tag, whatever the case", () => {
    expect(matching(counts, "DOG").map((count) => count.tag)).toEqual(["a dog"]);
  });

  it("finds the untagged pile by its name", () => {
    expect(matching(counts, "untag").map((count) => count.tag)).toEqual([UNTAGGED]);
  });
});
