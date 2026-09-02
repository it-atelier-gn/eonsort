import { describe, expect, it } from "vitest";
import { cleanSort, nextSort, sortedEntries, type Sorted } from "./ordering";
import { FILE_COLUMNS } from "./columns";
import type { EntryView } from "./api";

const ids = FILE_COLUMNS.columns.map((c) => c.id);

function entry(name: string, over: Partial<EntryView> = {}): EntryView {
  return {
    source: `/src/${name}`,
    destination: `/out/${name}`,
    name,
    folder: "/out",
    taken: "2019-07-04T10:00:00",
    taken_epoch: 1_562_234_400,
    provider: "exif",
    provider_info: null,
    size: 1000,
    destination_exists: false,
    outcome: null,
    candidates: [],
    flags: [],
    confidence: "medium",
    override_origin: null,
    orientation: 0,
    rotate: "none",
    rotate_by_hand: false,
    rotate_lossless: true,
    reencode: false,
    subject: null,
    tags: [],
    ...over,
  } as EntryView;
}

const names = (entries: EntryView[]) => entries.map((e) => e.name);

describe("clicking a column head", () => {
  it("sorts up, then down, then not at all", () => {
    let held: Sorted | null = null;
    held = nextSort(held, "rated");
    expect(held).toEqual({ id: "rated", down: false });
    held = nextSort(held, "rated");
    expect(held).toEqual({ id: "rated", down: true });
    expect(nextSort(held, "rated")).toBeNull();
  });

  it("starts afresh on another column", () => {
    expect(nextSort({ id: "rated", down: true }, "size")).toEqual({
      id: "size",
      down: false,
    });
  });
});

describe("the remembered sort", () => {
  it("keeps one it recognises", () => {
    expect(cleanSort({ id: "rated", down: true }, ids)).toEqual({
      id: "rated",
      down: true,
    });
  });

  it("refuses anything else", () => {
    expect(cleanSort(null, ids)).toBeNull();
    expect(cleanSort({ id: "nonsense" }, ids)).toBeNull();
    expect(cleanSort("rated", ids)).toBeNull();
    expect(cleanSort({ id: "rated" }, ids)).toEqual({
      id: "rated",
      down: false,
    });
  });
});

describe("sorting the files", () => {
  const shelf = [
    entry("b.jpg", { quality: 6.5, size: 300 }),
    entry("a.jpg", { quality: 4.1, size: 900 }),
    entry("c.jpg", { size: 100 }),
  ];

  it("leaves them alone when nothing is chosen", () => {
    expect(names(sortedEntries(shelf, null))).toEqual([
      "b.jpg",
      "a.jpg",
      "c.jpg",
    ]);
  });

  it("puts the best rated last going up, first going down", () => {
    expect(names(sortedEntries(shelf, { id: "rated", down: false }))).toEqual([
      "c.jpg",
      "a.jpg",
      "b.jpg",
    ]);
    expect(names(sortedEntries(shelf, { id: "rated", down: true }))).toEqual([
      "b.jpg",
      "a.jpg",
      "c.jpg",
    ]);
  });

  it("puts a picture with no rating below every rated one", () => {
    const ranked = sortedEntries(shelf, { id: "rated", down: true });
    expect(ranked.at(-1)?.name).toBe("c.jpg");
  });

  it("sorts by size and by name", () => {
    expect(names(sortedEntries(shelf, { id: "size", down: false }))).toEqual([
      "c.jpg",
      "b.jpg",
      "a.jpg",
    ]);
    expect(names(sortedEntries(shelf, { id: "name", down: false }))).toEqual([
      "a.jpg",
      "b.jpg",
      "c.jpg",
    ]);
  });

  it("never changes the list it was handed", () => {
    const before = names(shelf);
    sortedEntries(shelf, { id: "size", down: true });
    expect(names(shelf)).toEqual(before);
  });

  it("settles ties by name so the order never wobbles", () => {
    const tied = [entry("z.jpg", { size: 5 }), entry("y.jpg", { size: 5 })];
    expect(names(sortedEntries(tied, { id: "size", down: false }))).toEqual([
      "y.jpg",
      "z.jpg",
    ]);
  });
});
