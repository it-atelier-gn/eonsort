import { describe, expect, it } from "vitest";
import { agree, cards, folderOf } from "$lib/compare";
import type { EntryView } from "$lib/api";

function entry(source: string, taken: string, size: number): EntryView {
  return {
    source,
    destination: "/out/2023/05/a.jpg",
    name: "a.jpg",
    folder: "2023/05",
    taken,
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
    rotate_lossless: false,
    reencode: false,
    subject: null,
    tags: [],
    caption: null,
  };
}

describe("folderOf", () => {
  it("keeps the folder a file sits in", () => {
    expect(folderOf("D:\\Photos\\2023\\a.jpg")).toBe("D:\\Photos\\2023");
    expect(folderOf("/photos/2023/a.jpg")).toBe("/photos/2023");
  });

  it("has nothing to give for a bare name", () => {
    expect(folderOf("a.jpg")).toBe("");
  });
});

describe("cards", () => {
  const entries = {
    "/photos/a.jpg": entry("/photos/a.jpg", "2023-05-06 10:11", 12),
    "/backup/a.jpg": entry("/backup/a.jpg", "2023-05-06 10:11", 12),
  };

  it("marks the one that would be kept", () => {
    const shown = cards(["/photos/a.jpg", "/backup/a.jpg"], "/backup/a.jpg", entries);
    expect(shown.map((card) => card.keeper)).toEqual([false, true]);
  });

  it("shows a file the plan knows nothing about without facts", () => {
    const shown = cards(["/elsewhere/a.jpg"], null, entries);
    expect(shown[0]).toMatchObject({ name: "a.jpg", size: null, taken: null, keeper: false });
  });
});

describe("agree", () => {
  const entries = {
    "/photos/a.jpg": entry("/photos/a.jpg", "2023-05-06 10:11", 12),
    "/backup/a.jpg": entry("/backup/a.jpg", "2019-01-01 00:00", 12),
  };
  const shown = cards(["/photos/a.jpg", "/backup/a.jpg"], null, entries);

  it("says so when every copy reports the same", () => {
    expect(agree(shown, (card) => card.size)).toBe(true);
  });

  it("says so when they differ", () => {
    expect(agree(shown, (card) => card.taken)).toBe(false);
  });

  it("takes a single card as agreement", () => {
    expect(agree(shown.slice(0, 1), (card) => card.taken)).toBe(true);
  });
});
