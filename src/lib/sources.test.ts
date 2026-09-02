import { describe, expect, it } from "vitest";
import {
  atDefaultOrder,
  atDefaults,
  clampWeight,
  defaultOrder,
  enabledIn,
  isProvider,
  LADDER,
  weightsFor,
  listing,
  moveBy,
  moveSource,
  offeredBy,
  pickProvider,
  rowAt,
  sourceOf,
  toggled,
  weightOf,
  MAX_WEIGHT,
  MIN_WEIGHT,
  SOURCES,
} from "./sources";

describe("Listing the date sources", () => {
  it("puts the ones in use first and the rest behind them", () => {
    const shown = listing(["filesystem", "exif"]);
    expect(shown.slice(0, 2)).toEqual(["filesystem", "exif"]);
    expect(shown.slice(2)).toEqual(
      defaultOrder().filter((id) => id !== "filesystem" && id !== "exif"),
    );
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
    const order = defaultOrder();
    const first = order[0];
    expect(moveSource(order, "filesystem", first)[0]).toBe("filesystem");
  });

  it("leaves the order alone when a source is dropped on itself", () => {
    const order = listing(["exif", "filename"]);
    expect(moveSource(order, "exif", "exif")).toEqual(order);
  });

  it("walks a source one place up or down", () => {
    const order = listing(["filename", "exif", "media"]);
    expect(moveBy(order, "exif", -1)[0]).toBe("exif");
    expect(moveBy(order, "exif", 1).slice(0, 3)).toEqual(["filename", "media", "exif"]);
  });

  it("leaves a source at the end of the list where it is", () => {
    const order = listing(SOURCES.map((source) => source.id));
    expect(moveBy(order, order[0], -1)).toEqual(order);
    expect(moveBy(order, order[order.length - 1], 1)).toEqual(order);
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

  it("says when nothing has been changed", () => {
    expect(atDefaults({})).toBe(true);
    expect(atDefaults({ exif: sourceOf("exif").weight })).toBe(true);
    expect(atDefaults({ exif: 1 })).toBe(false);
  });
});

describe("Finding the line a drag is over", () => {
  const rows = [
    { top: 0, bottom: 10 },
    { top: 10, bottom: 20 },
    { top: 20, bottom: 30 },
  ];

  it("names the line the pointer is on", () => {
    expect(rowAt(rows, 15)).toBe(1);
    expect(rowAt(rows, 25)).toBe(2);
  });

  it("holds on to the first and last line when the pointer leaves the list", () => {
    expect(rowAt(rows, -40)).toBe(0);
    expect(rowAt(rows, 400)).toBe(2);
  });

  it("has nothing to name when there are no lines", () => {
    expect(rowAt([], 5)).toBe(-1);
  });
});

describe("Choosing the source a whole selection is re-dated from", () => {
  it("offers each source that dated at least one of the files, once", () => {
    expect(
      offeredBy([
        ["exif", "filesystem"],
        ["filesystem", "filename"],
      ]),
    ).toEqual(["exif", "filename", "filesystem"]);
  });

  it("offers nothing when none of the files carry a date", () => {
    expect(offeredBy([[], []])).toEqual([]);
    expect(pickProvider([], "filesystem")).toBeNull();
  });

  it("keeps the source already chosen while it is still on offer", () => {
    expect(pickProvider(["exif", "filesystem"], "exif")).toBe("exif");
  });

  it("never leaves a source picked that none of the files can supply", () => {
    expect(pickProvider(["exif", "xmp"], "filesystem")).toBe("exif");
    expect(pickProvider(["media", "xmp"], undefined)).toBe("media");
  });

  it("falls back to the file system when the files can supply it", () => {
    expect(pickProvider(["exif", "filesystem"], "xmp")).toBe("filesystem");
  });
});

describe("Weights that follow the order", () => {
  it("hands every source its usual weight when nothing has been moved", () => {
    const held = weightsFor(defaultOrder());
    for (const source of SOURCES) {
      expect(held[source.id]).toBe(source.weight);
    }
    expect(atDefaults(held)).toBe(true);
  });

  it("counts a source for more once it is dragged up", () => {
    const moved = moveSource(defaultOrder(), "filesystem", defaultOrder()[0]);
    const held = weightsFor(moved);
    expect(held.filesystem).toBe(LADDER[0]);
    expect(held.filesystem!).toBeGreaterThan(sourceOf("filesystem").weight);
    expect(atDefaults(held)).toBe(false);
  });

  it("never lets a source below another count for more", () => {
    const shuffled = moveSource(defaultOrder(), "filename", "exif");
    const held = weightsFor(shuffled);
    const ranked = listing(shuffled).map((id) => held[id]!);
    for (let at = 1; at < ranked.length; at += 1) {
      expect(ranked[at - 1]).toBeGreaterThanOrEqual(ranked[at]);
    }
  });

  it("gives every source a weight, whatever it was handed", () => {
    const held = weightsFor(["exif"]);
    expect(Object.keys(held).sort()).toEqual(SOURCES.map((source) => source.id).sort());
  });

  it("keeps the ladder the same set of weights the sources started with", () => {
    expect([...LADDER].sort()).toEqual(SOURCES.map((source) => source.weight).sort());
  });

  it("knows when the order is the usual one", () => {
    expect(atDefaultOrder(defaultOrder())).toBe(true);
    expect(atDefaultOrder(moveSource(defaultOrder(), "filesystem", defaultOrder()[0]))).toBe(false);
  });
});
