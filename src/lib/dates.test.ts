import { describe, expect, it } from "vitest";
import type { EntryView } from "./api";
import {
  describeShift,
  fromInputValue,
  hardFlags,
  isSuspect,
  shiftSeconds,
  toInputValue,
} from "./dates";

function entry(overrides: Partial<EntryView> = {}): EntryView {
  return {
    source: "/src/IMG_0001.jpg",
    destination: "/out/2003/01/IMG_0001.jpg",
    name: "IMG_0001.jpg",
    folder: "2003/01",
    taken: "2003-01-01 00:00:00",
    taken_epoch: Date.parse("2003-01-01T00:00:00Z") / 1000,
    provider: "exif",
    provider_info: "DateTimeOriginal",
    size: 100,
    destination_exists: false,
    outcome: null,
    candidates: [],
    flags: [],
    confidence: "low",
    override_origin: null,
    orientation: 0,
    rotate: "none",
    rotate_by_hand: false,
    rotate_lossless: true,
    reencode: false,
    subject: null,
    tags: [],
    caption: null,
    ...overrides,
  };
}

describe("date input round-tripping", () => {
  it("survives a trip through the datetime-local field", () => {
    const epoch = Date.parse("2019-07-04T10:30:00Z") / 1000;
    expect(toInputValue(epoch)).toBe("2019-07-04T10:30");
    expect(fromInputValue("2019-07-04T10:30")).toBe(epoch);
  });

  it("does not shift the wall-clock time by the local timezone", () => {
    const value = toInputValue(Date.parse("2003-01-01T00:00:00Z") / 1000);
    expect(value).toBe("2003-01-01T00:00");
  });

  it("rejects an unparseable field value", () => {
    expect(fromInputValue("")).toBeNull();
    expect(fromInputValue("not a date")).toBeNull();
  });
});

describe("bulk shift", () => {
  it("computes the offset between the current and the true date", () => {
    const seconds = shiftSeconds(entry(), "2019-07-04T00:00");
    expect(seconds).toBe(Date.parse("2019-07-04T00:00:00Z") / 1000 - entry().taken_epoch);
  });

  it("returns nothing when the anchor date cannot be read", () => {
    expect(shiftSeconds(entry(), "nonsense")).toBeNull();
  });

  it("describes a shift in both directions", () => {
    expect(describeShift(90000)).toBe("+1d 1h");
    expect(describeShift(-3600)).toBe("-1h");
    expect(describeShift(0)).toBe("+0m");
  });
});

describe("suspect entries", () => {
  it("counts only hard flags", () => {
    const flagged = entry({
      flags: [
        { kind: "camera_epoch", description: "reset date", hard: true },
        { kind: "provider_spread", description: "disagree", hard: false },
      ],
    });
    expect(hardFlags(flagged)).toHaveLength(1);
    expect(isSuspect(flagged)).toBe(true);
  });

  it("stops flagging a file once the user has decided", () => {
    const fixed = entry({
      flags: [{ kind: "camera_epoch", description: "reset date", hard: true }],
      override_origin: "set by hand",
    });
    expect(isSuspect(fixed)).toBe(false);
  });

  it("leaves a soft-flagged file alone", () => {
    const soft = entry({
      flags: [{ kind: "provider_spread", description: "disagree", hard: false }],
    });
    expect(isSuspect(soft)).toBe(false);
  });
});
