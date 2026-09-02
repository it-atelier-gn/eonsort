import { describe, expect, it } from "vitest";
import type { Spot } from "$lib/api";
import type { Transform } from "$lib/api";
import {
  BIG_ENOUGH,
  boxOf,
  countFaces,
  laidOut,
  showBoxes,
  lookedAt,
  named,
  pickable,
  sureFaces,
  tally,
  anybody,
  matchingNames,
  wearingAll,
  whoLabel,
  withName,
  withFaces,
} from "$lib/faces";

const spot = (
  x: number,
  y: number,
  width = 0.2,
  height = 0.2,
  score = 0.9,
): Spot => ({
  x,
  y,
  width,
  height,
  score,
});

describe("sureFaces", () => {
  it("ignores a box too small to be worth naming", () => {
    const tiny = BIG_ENOUGH / 2;
    const held = [
      spot(0.1, 0.1, 0.2, 0.2, 0.95),
      spot(0.5, 0.5, tiny, tiny, 0.99),
      spot(0.6, 0.6, 0.2, tiny, 0.99),
      spot(0.7, 0.7, tiny, 0.2, 0.99),
    ];
    expect(sureFaces(held)).toHaveLength(1);
  });

  it("keeps a box that sits right on the floor", () => {
    const held = [spot(0.1, 0.1, BIG_ENOUGH, BIG_ENOUGH, 0.9)];
    expect(sureFaces(held)).toHaveLength(1);
  });

  it("keeps only what the detector was sure about", () => {
    const held = [
      spot(0.1, 0.1, 0.2, 0.2, 0.95),
      spot(0.5, 0.5, 0.2, 0.2, 0.2),
    ];
    expect(sureFaces(held)).toHaveLength(1);
  });

  it("treats a picture nobody looked at as having none", () => {
    expect(sureFaces(undefined)).toEqual([]);
  });
});

describe("lookedAt", () => {
  it("tells an empty answer apart from no answer", () => {
    const faces = { "a.jpg": [], "b.jpg": [spot(0.1, 0.1)] };
    expect(lookedAt(faces, "a.jpg")).toBe(true);
    expect(lookedAt(faces, "b.jpg")).toBe(true);
    expect(lookedAt(faces, "c.jpg")).toBe(false);
  });
});

describe("countFaces", () => {
  it("counts the sure ones for a source", () => {
    const faces = { "a.jpg": [spot(0.1, 0.1), spot(0.4, 0.4)] };
    expect(countFaces(faces, "a.jpg")).toBe(2);
    expect(countFaces(faces, "missing.jpg")).toBe(0);
  });
});

describe("boxOf", () => {
  it("turns a share of the picture into percentages", () => {
    expect(boxOf(spot(0.25, 0.5, 0.25, 0.1))).toEqual({
      left: "25%",
      top: "50%",
      width: "25%",
      height: "10%",
    });
  });

  it("clips a face that runs off the edge back into the frame", () => {
    const box = boxOf(spot(-0.1, -0.2, 0.3, 0.4));
    expect(box.left).toBe("0%");
    expect(box.top).toBe("0%");
    expect(box.width).toBe("20%");
    expect(box.height).toBe("20%");
  });

  it("never reports a negative size for a box wholly outside", () => {
    const box = boxOf(spot(1.4, 1.4, 0.2, 0.2));
    expect(box.width).toBe("0%");
    expect(box.height).toBe("0%");
  });
});

describe("withFaces", () => {
  it("narrows a list to the pictures with people in them", () => {
    const faces = {
      "people.jpg": [spot(0.1, 0.1)],
      "screenshot.png": [],
      "unsure.jpg": [spot(0.1, 0.1, 0.2, 0.2, 0.1)],
    };
    expect(
      withFaces(
        ["people.jpg", "screenshot.png", "unsure.jpg", "new.jpg"],
        faces,
      ),
    ).toEqual(["people.jpg"]);
  });
});

describe("tally", () => {
  it("adds up every sure face across the plan", () => {
    const faces = {
      "a.jpg": [spot(0.1, 0.1), spot(0.4, 0.4)],
      "b.jpg": [spot(0.1, 0.1)],
      "c.jpg": [],
    };
    expect(tally(faces)).toBe(3);
  });
});

describe("Picking out the people in a picture", () => {
  const faces: Record<string, Spot[]> = {
    "anna.jpg": [{ ...spot(0.1, 0.1), label: "Anna" }],
    "crowd.jpg": [
      { ...spot(0.1, 0.1), label: "Anna" },
      { ...spot(0.5, 0.5), label: "Bo" },
    ],
    "nobody.jpg": [spot(0.1, 0.1)],
  };

  it("shows every picture when nobody is ticked", () => {
    expect(wearingAll(faces, "nobody.jpg", null)).toBe(true);
    expect(wearingAll(faces, "nobody.jpg", [])).toBe(true);
    expect(anybody(null)).toBe(true);
    expect(anybody([])).toBe(true);
    expect(anybody(["Anna"])).toBe(false);
  });

  it("keeps only a picture everyone ticked is in together", () => {
    expect(wearingAll(faces, "crowd.jpg", ["Anna", "Bo"])).toBe(true);
    expect(wearingAll(faces, "anna.jpg", ["Anna", "Bo"])).toBe(false);
    expect(wearingAll(faces, "anna.jpg", ["Anna"])).toBe(true);
    expect(wearingAll(faces, "anna.jpg", ["Bo"])).toBe(false);
    expect(wearingAll(faces, "nobody.jpg", ["Anna"])).toBe(false);
    expect(wearingAll(faces, "missing.jpg", ["Anna"])).toBe(false);
  });

  it("passes over a face the detector was unsure about", () => {
    const unsure: Record<string, Spot[]> = {
      "maybe.jpg": [{ ...spot(0.1, 0.1, 0.2, 0.2, 0.2), label: "Anna" }],
    };
    expect(wearingAll(unsure, "maybe.jpg", ["Anna"])).toBe(false);
  });

  it("counts one person once, however many of their faces are in the picture", () => {
    const twice: Record<string, Spot[]> = {
      "mirror.jpg": [
        { ...spot(0.1, 0.1), label: "Anna" },
        { ...spot(0.6, 0.1), label: "Anna" },
      ],
    };
    expect(wearingAll(twice, "mirror.jpg", ["Anna"])).toBe(true);
    expect(wearingAll(twice, "mirror.jpg", ["Anna", "Bo"])).toBe(false);
  });

  it("ticks a name on and off again", () => {
    expect(withName(null, "Anna")).toEqual(["Anna"]);
    expect(withName(["Anna"], "Bo")).toEqual(["Anna", "Bo"]);
    expect(withName(["Anna", "Bo"], "Anna")).toEqual(["Bo"]);
    expect(withName(["Anna"], "Anna")).toBeNull();
  });

  it("says who is being shown", () => {
    expect(whoLabel(null)).toBe("anybody");
    expect(whoLabel([])).toBe("anybody");
    expect(whoLabel(["Anna"])).toBe("Anna");
    expect(whoLabel(["Anna", "Bo"])).toBe("2 people");
  });

  it("finds a name however it is typed", () => {
    const names = [
      { name: "Anna", count: 4 },
      { name: "Bo", count: 2 },
    ];
    expect(matchingNames(names, "an").map((p) => p.name)).toEqual(["Anna"]);
    expect(matchingNames(names, "  BO ").map((p) => p.name)).toEqual(["Bo"]);
    expect(matchingNames(names, "")).toEqual(names);
  });
});

describe("named", () => {
  it("lists each name once, in order", () => {
    const spots = [
      { ...spot(0.1, 0.1), label: "Bo" },
      { ...spot(0.3, 0.3), label: "Anna" },
      { ...spot(0.5, 0.5), label: "Bo" },
      spot(0.7, 0.7),
    ];
    expect(named(spots)).toEqual(["Anna", "Bo"]);
  });

  it("says nobody for a picture with no names on it", () => {
    expect(named([spot(0.1, 0.1)])).toEqual([]);
    expect(named(undefined)).toEqual([]);
  });
});

describe("Remembering whether the boxes are shown", () => {
  it("shows them until someone turns them off", () => {
    expect(showBoxes(null)).toBe(true);
    expect(showBoxes(undefined)).toBe(true);
    expect(showBoxes(true)).toBe(true);
  });

  it("keeps them hidden once that is what was asked for", () => {
    expect(showBoxes(false)).toBe(false);
  });

  it("shows them again when what was remembered makes no sense", () => {
    expect(showBoxes("yes")).toBe(true);
    expect(showBoxes(0)).toBe(true);
  });
});

describe("Putting a face where the picture is drawn", () => {
  const face = spot(0.279, 0.398, 0.285, 0.324);
  const every: Transform[] = [
    "none",
    "rotate90",
    "rotate180",
    "rotate270",
    "flip_h",
    "flip_v",
    "transpose",
    "transverse",
  ];

  it("leaves a face alone when the picture is drawn as it is", () => {
    const laid = laidOut(face, "none");
    expect(laid.x).toBeCloseTo(face.x);
    expect(laid.y).toBeCloseTo(face.y);
    expect(laid.width).toBeCloseTo(face.width);
    expect(laid.height).toBeCloseTo(face.height);
  });

  it("swaps the sides when the picture is drawn on its side", () => {
    const laid = laidOut(face, "rotate90");
    expect(laid.width).toBeCloseTo(face.height);
    expect(laid.height).toBeCloseTo(face.width);
    expect(laid.x).toBeCloseTo(face.y);
    expect(laid.y).toBeCloseTo(1 - face.x - face.width);
  });

  it("puts a face upside down on the other side of the picture", () => {
    const laid = laidOut(face, "rotate180");
    expect(laid.x).toBeCloseTo(1 - face.x - face.width);
    expect(laid.y).toBeCloseTo(1 - face.y - face.height);
    expect(laid.width).toBeCloseTo(face.width);
    expect(laid.height).toBeCloseTo(face.height);
  });

  it("mirrors a face without moving it up or down", () => {
    const laid = laidOut(face, "flip_h");
    expect(laid.x).toBeCloseTo(1 - face.x - face.width);
    expect(laid.y).toBeCloseTo(face.y);
  });

  it("keeps every face inside the picture, whichever way it is drawn", () => {
    for (const transform of every) {
      const laid = laidOut(face, transform);
      expect(laid.x).toBeGreaterThanOrEqual(0);
      expect(laid.y).toBeGreaterThanOrEqual(0);
      expect(laid.x + laid.width).toBeLessThanOrEqual(1.000001);
      expect(laid.y + laid.height).toBeLessThanOrEqual(1.000001);
    }
  });

  it("keeps the ground it covers, whichever way it is drawn", () => {
    for (const transform of every) {
      const laid = laidOut(face, transform);
      expect(laid.width * laid.height).toBeCloseTo(face.width * face.height);
    }
  });

  it("says the same as the box the pane draws", () => {
    const box = boxOf(face, "rotate90");
    const laid = laidOut(face, "rotate90");
    expect(box.left).toBe(`${Math.round(laid.x * 1e6) / 1e4}%`);
    expect(box.width).toBe(`${Math.round(laid.width * 1e6) / 1e4}%`);
  });

  it("carries the name and the score along", () => {
    const named = { ...face, label: "Anne" };
    expect(laidOut(named, "rotate270").label).toBe("Anne");
    expect(laidOut(named, "rotate270").score).toBe(named.score);
  });
});

describe("Offering the names already given to a face", () => {
  const people = [
    { name: "Luise", count: 3 },
    { name: "Anne", count: 12 },
    { name: "Bo", count: 3 },
  ];

  it("puts the most often used first, then settles ties by name", () => {
    expect(pickable(people).map((p) => p.name)).toEqual(["Anne", "Bo", "Luise"]);
  });

  it("leaves out the name this face already wears", () => {
    expect(pickable(people, "Anne").map((p) => p.name)).toEqual(["Bo", "Luise"]);
  });

  it("drops blanks and keeps a name once", () => {
    expect(
      pickable([
        { name: "  ", count: 4 },
        { name: "Anne", count: 2 },
        { name: "Anne", count: 9 },
      ]),
    ).toEqual([{ name: "Anne", count: 9 }]);
  });

  it("copes with nothing named yet", () => {
    expect(pickable(undefined)).toEqual([]);
  });
});
