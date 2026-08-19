import { describe, expect, it } from "vitest";
import {
  atDefaults,
  clampWeight,
  enabledIn,
  isProvider,
  listing,
  moveSource,
  sourceOf,
  toggled,
  weightOf,
  withWeight,
  MAX_WEIGHT,
  MIN_WEIGHT,
  SOURCES,
} from "./sources";

describe("Listing the date sources", () => {
  it("puts the ones in use first and the rest behind them", () => {
    expect(listing(["filesystem", "exif"])).toEqual([
      "filesystem",
      "exif",
      "filename",
      "media",
      "xmp",
      "takeout",
      "system",
    ]);
  });

  it("shows every source exactly once, whatever it was handed", () => {
    const shown = listing(["exif", "exif", "nowhere" as never]);
    expect([...shown].sort()).toEqual(SOURCES.map((source) => source.id).sort());
  });

  it("knows a source when it sees one", () => {
    expect(isProvider("takeout")).toBe(true);
    expect(isProvider("guesswork")).toBe(false);
  });

  it("names a source it was given", () => {
    expect(sourceOf("xmp").label).toBe("XMP sidecar");
  });
});

describe("Putting the sources in an order of your own", () => {
  it("drops the dragged source where the target was", () => {
    const order = listing(SOURCES.map((source) => source.id));
    expect(moveSource(order, "filesystem", "filename")[0]).toBe("filesystem");
  });

  it("leaves the order alone when a source is dropped on itself", () => {
    const order = listing(["exif", "filename"]);
    expect(moveSource(order, "exif", "exif")).toEqual(order);
  });

  it("keeps the sources in use in the order the list shows", () => {
    const order = listing(["filesystem", "exif", "filename"]);
    expect(enabledIn(order, ["filename", "exif"])).toEqual(["exif", "filename"]);
  });

  it("switches a source off and on again without losing the order", () => {
    const order = listing(["filename", "exif", "media"]);
    const off = toggled(["filename", "exif", "media"], order, "exif");
    expect(off).toEqual(["filename", "media"]);
    expect(toggled(off, order, "exif")).toEqual(["filename", "exif", "media"]);
  });
});

describe("Weighing a source", () => {
  it("uses the weight it was born with until you say otherwise", () => {
    expect(weightOf({}, "exif")).toBe(sourceOf("exif").weight);
    expect(weightOf({ exif: 12 }, "exif")).toBe(12);
  });

  it("holds a weight inside the scale", () => {
    expect(clampWeight(-4)).toBe(MIN_WEIGHT);
    expect(clampWeight(4000)).toBe(MAX_WEIGHT);
    expect(clampWeight(41.6)).toBe(42);
    expect(weightOf({ exif: Number.NaN }, "exif")).toBe(sourceOf("exif").weight);
  });

  it("writes a weight without touching the ones it was given", () => {
    const held = { exif: 50 };
    const next = withWeight(held, "filename", 70);
    expect(held).toEqual({ exif: 50 });
    expect(next).toEqual({ exif: 50, filename: 70 });
  });

  it("drops a weight to go back to the one it was born with", () => {
    expect(withWeight({ exif: 50 }, "exif", null)).toEqual({});
  });

  it("says when nothing has been changed", () => {
    expect(atDefaults({})).toBe(true);
    expect(atDefaults({ exif: sourceOf("exif").weight })).toBe(true);
    expect(atDefaults({ exif: 1 })).toBe(false);
  });
});
