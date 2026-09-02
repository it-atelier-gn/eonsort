import { describe, expect, it } from "vitest";
import type { EntryView } from "$lib/api";
import {
  buildRings,
  clampHeight,
  gapFor,
  heightOf,
  heightRange,
  labelAt,
  monthCss,
  monthOf,
  clampFly,
  flownTheta,
  nearestTiles,
  pitchAt,
  pitchShare,
  placeOf,
  radiusFor,
  turnRate,
  zoomedRadius,
  yearOf,
  thinned,
  MAX_FLY,
  MAX_PITCH,
  MIN_FLY,
  MIN_PITCH,
  MIN_STEP,
  TURN_RADIUS,
  MIN_RADIUS,
  MONTH_COLOURS,
  RING_CAPACITY,
  LABEL_MARGIN,
  MAX_GAP,
  MAX_RADIUS,
  RING_GAP,
  TILE_WIDTH,
} from "./layout";
import { frameOrbit, MAX_ORBIT, MIN_ORBIT } from "./scene";

function entry(taken: string): EntryView {
  const epoch = Date.parse(`${taken}Z`) / 1000;
  return {
    source: `C:/pictures/${taken}.jpg`,
    destination: `E:/sorted/${taken}.jpg`,
    name: `${taken}.jpg`,
    folder: "2020",
    taken,
    taken_epoch: epoch,
    provider: "exif",
    provider_info: null,
    size: 1000,
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
  } as unknown as EntryView;
}

describe("Stacking a year on top of the one before it", () => {
  it("gives every year a ring of its own, oldest at the bottom", () => {
    const rings = buildRings([
      entry("2021-05-02T10:00:00"),
      entry("2019-03-01T09:00:00"),
      entry("2021-01-04T08:00:00"),
    ]);

    expect(rings.rings.map((ring) => ring.year)).toEqual([2019, 2021]);
    expect(rings.rings[0].y).toBe(0);
    expect(rings.rings[1].y).toBeGreaterThan(rings.rings[0].y);
    expect(rings.rings[1].count).toBe(2);
    expect(rings.rings[1].shown).toBe(2);
    expect(rings.height).toBe(RING_GAP);
    expect(rings.files).toBe(3);
  });

  it("says nothing about an empty plan", () => {
    const rings = buildRings([]);
    expect(rings.tiles).toEqual([]);
    expect(rings.rings).toEqual([]);
    expect(rings.radius).toBe(MIN_RADIUS);
  });

  it("widens a ring as more pictures land in that year", () => {
    const narrow = radiusFor(10);
    const wide = radiusFor(400);
    expect(wide).toBeGreaterThan(narrow);
    expect(radiusFor(1)).toBe(MIN_RADIUS);
    expect(radiusFor(400)).toBeCloseTo((400 * TILE_WIDTH * 1.14) / (Math.PI * 2), 5);
    expect(radiusFor(RING_CAPACITY * 40)).toBeLessThanOrEqual(MAX_RADIUS);
  });

  it("thins a year too crowded to sit side by side, keeping its spread", () => {
    const many = Array.from({ length: RING_CAPACITY * 3 }, (_, index) => index);
    const kept = thinned(many);

    expect(kept).toHaveLength(RING_CAPACITY);
    expect(kept[0]).toBe(0);
    expect(kept[kept.length - 1]).toBeGreaterThan(many.length - 4);
    expect(kept).toEqual([...kept].sort((a, b) => a - b));
    expect(thinned([3, 1, 2])).toEqual([3, 1, 2]);
  });

  it("stands the rings further apart as they grow wider", () => {
    expect(gapFor(MIN_RADIUS)).toBe(RING_GAP);
    expect(gapFor(MAX_RADIUS)).toBeGreaterThan(RING_GAP);
    expect(gapFor(10_000)).toBe(MAX_GAP);

    const wide = buildRings([
      ...Array.from({ length: 300 }, (_, i) => entry(`2020-01-01T${String(i % 24).padStart(2, "0")}:00:00`)),
      entry("2021-01-01T10:00:00"),
    ]);
    expect(wide.gap).toBeGreaterThan(RING_GAP);
    expect(wide.rings[1].y).toBeCloseTo(wide.gap, 6);
  });

  it("hangs a year label just outside its own ring", () => {
    const rings = buildRings([entry("2020-05-01T10:00:00")]);
    const [x, y, z] = labelAt(rings.rings[0], 0);

    expect(y).toBe(rings.rings[0].y);
    expect(Math.hypot(x, z)).toBeCloseTo(rings.rings[0].radius + LABEL_MARGIN, 6);
  });

  it("puts the pictures of a year side by side, in the order they were taken", () => {
    const rings = buildRings([
      entry("2020-08-01T10:00:00"),
      entry("2020-02-01T10:00:00"),
      entry("2020-11-01T10:00:00"),
    ]);
    const angles = rings.tiles.map((tile) => tile.angle);

    expect(rings.tiles.map((tile) => tile.month)).toEqual([1, 7, 10]);
    expect(angles).toEqual([...angles].sort((a, b) => a - b));
    expect(angles[0]).toBe(0);
    expect(angles[2]).toBeCloseTo((2 / 3) * Math.PI * 2, 6);
  });

  it("faces every picture outwards, away from the middle", () => {
    const rings = buildRings([entry("2020-02-01T10:00:00"), entry("2020-08-01T10:00:00")]);
    for (const tile of rings.tiles) {
      const [x, , z] = placeOf(tile);
      expect(Math.hypot(x, z)).toBeCloseTo(tile.radius, 6);
      expect(Math.sin(tile.angle) * x + Math.cos(tile.angle) * z).toBeCloseTo(tile.radius, 6);
    }
  });
});

describe("Winding the rings into a spiral", () => {
  it("lifts each picture along its year, ending where the next ring starts", () => {
    const rings = buildRings([
      entry("2020-01-01T10:00:00"),
      entry("2020-07-01T10:00:00"),
      entry("2021-01-01T10:00:00"),
    ]);
    const [first, second, third] = rings.tiles;

    expect(heightOf(first, 1)).toBe(first.y);
    expect(heightOf(second, 1)).toBeCloseTo(first.y + RING_GAP / 2, 6);
    expect(heightOf(third, 1)).toBe(third.y);
  });

  it("holds every picture in its ring until the spiral is asked for", () => {
    const rings = buildRings([entry("2020-01-01T10:00:00"), entry("2020-07-01T10:00:00")]);
    for (const tile of rings.tiles) {
      expect(heightOf(tile, 0)).toBe(tile.y);
      expect(heightOf(tile, 0.5)).toBeCloseTo((tile.y + tile.coil) / 2, 6);
      expect(heightOf(tile, 4)).toBe(tile.coil);
    }
  });
});

describe("Colouring by month", () => {
  it("gives each month a colour of its own", () => {
    const seen = new Set(MONTH_COLOURS.map((colour) => colour.join(",")));
    expect(seen.size).toBe(12);
    expect(MONTH_COLOURS).toHaveLength(12);
  });

  it("writes a colour a stylesheet understands", () => {
    expect(monthCss(0)).toMatch(/^rgb\(\d+, \d+, \d+\)$/);
    expect(monthCss(12)).toBe(monthCss(0));
    expect(monthCss(-1)).toBe(monthCss(11));
  });

  it("reads the year and the month out of a date", () => {
    const epoch = Date.parse("2019-04-07T12:00:00Z") / 1000;
    expect(yearOf(epoch)).toBe(2019);
    expect(monthOf(epoch)).toBe(3);
  });
});

describe("Choosing what to hang a picture on", () => {
  it("takes the tiles closest to the eye first", () => {
    const rings = buildRings([
      entry("2020-01-01T10:00:00"),
      entry("2020-04-01T10:00:00"),
      entry("2020-08-01T10:00:00"),
      entry("2020-12-01T10:00:00"),
    ]);
    const near = nearestTiles(rings, [0, 0, 40], 2);

    expect(near).toHaveLength(2);
    const far = nearestTiles(rings, [0, 0, 40], 4);
    const distance = (tile: (typeof far)[number]) => {
      const [x, y, z] = placeOf(tile);
      return Math.hypot(x, y, z - 40);
    };
    expect(distance(far[0])).toBeLessThanOrEqual(distance(far[3]));
    expect(nearestTiles(rings, [0, 0, 40], 0)).toEqual([]);
  });
});

describe("Framing the stack", () => {
  it("pulls the eye back far enough to see every ring", () => {
    const rings = buildRings([entry("2019-01-01T10:00:00"), entry("2021-01-01T10:00:00")]);
    const orbit = frameOrbit(rings, { theta: 0, phi: 1, radius: 5, target: [0, 0, 0] });

    expect(orbit.target[1]).toBeCloseTo(rings.height / 2, 6);
    expect(orbit.radius).toBeGreaterThanOrEqual(MIN_ORBIT);
    expect(orbit.radius).toBeLessThanOrEqual(MAX_ORBIT);
    expect(orbit.radius).toBeGreaterThan(rings.radius);
  });
});

describe("Looking up and down the stack", () => {
  it("reaches a little past the lowest and highest ring", () => {
    const rings = buildRings([
      entry("2019-01-01T10:00:00"),
      entry("2020-01-01T10:00:00"),
      entry("2021-01-01T10:00:00"),
    ]);
    const range = heightRange(rings);

    expect(range.min).toBeLessThan(0);
    expect(range.max).toBeGreaterThan(rings.height);
    expect(range.min).toBeCloseTo(-rings.gap, 6);
    expect(range.max).toBeCloseTo(rings.height + rings.gap, 6);
  });

  it("holds the eye inside that reach", () => {
    const rings = buildRings([entry("2019-01-01T10:00:00"), entry("2021-01-01T10:00:00")]);
    const { min, max } = heightRange(rings);

    expect(clampHeight(rings, 999)).toBeCloseTo(max, 6);
    expect(clampHeight(rings, -999)).toBeCloseTo(min, 6);
    expect(clampHeight(rings, rings.height / 2)).toBeCloseTo(rings.height / 2, 6);
    expect(clampHeight(rings, Number.NaN)).toBeCloseTo(min, 6);
  });

  it("tilts from just below to just above without looking through the pole", () => {
    expect(pitchAt(0)).toBeCloseTo(MIN_PITCH, 6);
    expect(pitchAt(1)).toBeCloseTo(MAX_PITCH, 6);
    expect(pitchAt(-3)).toBeCloseTo(MIN_PITCH, 6);
    expect(pitchAt(9)).toBeCloseTo(MAX_PITCH, 6);
    expect(pitchAt(0.5)).toBeGreaterThan(MIN_PITCH);
    expect(pitchAt(0.5)).toBeLessThan(MAX_PITCH);
  });

  it("reads a tilt back out as the share it came from", () => {
    for (const share of [0, 0.25, 0.5, 0.75, 1]) {
      expect(pitchShare(pitchAt(share))).toBeCloseTo(share, 6);
    }
    expect(pitchShare(-1)).toBe(0);
    expect(pitchShare(Math.PI * 2)).toBe(1);
  });
});

describe("Drifting on its own", () => {
  it("keeps the pace inside what the slider offers", () => {
    expect(clampFly(0)).toBeCloseTo(MIN_FLY, 6);
    expect(clampFly(99)).toBeCloseTo(MAX_FLY, 6);
    expect(clampFly(Number.NaN)).toBe(1);
    expect(clampFly(0.5)).toBeCloseTo(0.5, 6);
  });

  it("turns a wide stack more slowly than a narrow one", () => {
    const narrow = turnRate(1, TURN_RADIUS);
    const wide = turnRate(1, TURN_RADIUS * 8);

    expect(wide).toBeLessThan(narrow);
    expect(wide).toBeCloseTo(narrow / 8, 6);
    expect(turnRate(1, 0.1)).toBeCloseTo(narrow, 6);
    expect(turnRate(2, TURN_RADIUS)).toBeCloseTo(narrow * 2, 6);
  });

  it("comes back around after a full turn", () => {
    let theta = 0;
    for (let step = 0; step < 400; step += 1) {
      theta = flownTheta(theta, 0.5, MAX_FLY, TURN_RADIUS);
      expect(theta).toBeGreaterThanOrEqual(0);
      expect(theta).toBeLessThan(Math.PI * 2);
    }
    expect(flownTheta(1, -5, 1)).toBeCloseTo(1, 6);
  });
});

describe("Zooming towards the pictures", () => {
  it("never steps more than halfway to the ring", () => {
    const band = 20;
    let radius = 60;
    while (radius - band > MIN_STEP * 2) {
      const before = radius - band;
      radius = zoomedRadius(radius, -400, band, 3, 400);
      expect(radius).toBeGreaterThan(band);
      expect(radius - band).toBeGreaterThanOrEqual(before / 2 - 1e-6);
    }
    expect(radius).toBeGreaterThan(band);
  });

  it("takes finer steps the closer it gets", () => {
    const band = 20;
    const far = 60 - zoomedRadius(60, -400, band, 3, 400);
    const near = 21 - zoomedRadius(21, -400, band, 3, 400);

    expect(near).toBeLessThan(far);
    expect(near).toBeGreaterThan(0);
  });

  it("still creeps through the ring when you keep going", () => {
    const band = 20;
    let radius = band + MIN_STEP / 2;
    for (let step = 0; step < 50; step += 1) radius = zoomedRadius(radius, -400, band, 3, 400);
    expect(radius).toBeLessThan(band);
  });

  it("stays inside the near and far limits and holds still without a wheel", () => {
    expect(zoomedRadius(300, 4000, 20, 3, 400)).toBeLessThanOrEqual(400);
    expect(zoomedRadius(4, -4000, 20, 3, 400)).toBeGreaterThanOrEqual(3);
    expect(zoomedRadius(18, 0, 20, 3, 400)).toBeCloseTo(18, 6);
    expect(zoomedRadius(18, Number.NaN, 20, 3, 400)).toBeCloseTo(18, 6);
  });
});
