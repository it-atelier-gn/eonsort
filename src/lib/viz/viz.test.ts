import { describe, expect, it } from "vitest";
import type { EntryView } from "$lib/api";
import { buildTimeAxis, DAY, hourOfDay } from "./timeaxis";
import {
  buildInstances,
  disagreementPairs,
  ease,
  field,
  helix,
  layoutFor,
  terrain,
} from "./layouts";
import { lerpOrbit, multiply, perspective, project, MIN_RADIUS, PRESETS } from "./camera";
import { flightOrbit, flightWaypoints, samplePath } from "./flight";
import {
  detailFor,
  tileFade,
  tilesFrom,
  DETAIL_FAR,
  DETAIL_NEAR,
  MAX_TILE_SIZE,
  MIN_TILE_DEPTH,
  TILE_FULL_RADIUS,
  TILE_RADIUS,
} from "./scene";
import {
  formatYear,
  heatStep,
  hourHistogram,
  midnightShare,
  niceTicks,
  providerCounts,
  readingCounts,
  span,
  topFolders,
  HEAT_STEPS,
} from "./charts";
import {
  filterRange,
  heatGrid,
  levelFor,
  planRange,
  rangeLabel,
  sameRange,
  selectionRange,
} from "./range";
import { decodeId } from "./gl";

const at = (iso: string) => Date.parse(`${iso}Z`) / 1000;

function entry(overrides: Partial<EntryView> = {}): EntryView {
  return {
    source: "/src/a.jpg",
    destination: "/out/2003/01/a.jpg",
    name: "a.jpg",
    folder: "2003/01",
    taken: "2003-01-01 00:00:00",
    taken_epoch: at("2003-01-01T00:00:00"),
    provider: "exif",
    provider_info: null,
    size: 1,
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

describe("time axis", () => {
  it("keeps time in order", () => {
    const epochs = [at("2019-07-04T10:00:00"), at("2019-07-06T10:00:00"), at("2003-01-01T00:00:00")];
    const axis = buildTimeAxis(epochs);
    const sorted = [...epochs].sort((a, b) => a - b);
    const mapped = sorted.map((e) => axis.map(e));

    for (let i = 1; i < mapped.length; i += 1) {
      expect(mapped[i]).toBeGreaterThanOrEqual(mapped[i - 1]);
    }
    expect(mapped[0]).toBeCloseTo(0, 5);
    expect(mapped[mapped.length - 1]).toBeLessThanOrEqual(1);
  });

  it("compresses a long empty stretch instead of collapsing both ends into a dot", () => {
    const island = at("2003-01-01T00:00:00");
    const cluster = at("2019-07-04T10:00:00");
    const axis = buildTimeAxis([island, island + DAY, cluster, cluster + DAY, cluster + 2 * DAY]);

    expect(axis.breaks).toHaveLength(1);
    const gapWidth = axis.map(cluster) - axis.map(island + DAY);
    expect(gapWidth).toBeLessThan(0.95);
    expect(axis.map(cluster + 2 * DAY) - axis.map(cluster)).toBeGreaterThan(0.02);
  });

  it("labels each year once", () => {
    const axis = buildTimeAxis([
      at("2019-07-04T10:00:00"),
      at("2019-08-04T10:00:00"),
      at("2020-01-04T10:00:00"),
    ]);
    expect(axis.ticks.map((t) => t.label)).toEqual(["2019", "2020"]);
  });

  it("survives an empty and a single-file plan", () => {
    expect(buildTimeAxis([]).map(0)).toBe(0.5);
    const single = buildTimeAxis([at("2019-07-04T10:00:00")]);
    expect(single.map(at("2019-07-04T10:00:00"))).toBe(0.5);
    expect(single.ticks).toHaveLength(1);
  });

  it("reads the hour of day without a timezone shift", () => {
    expect(hourOfDay(at("2019-07-04T09:30:00"))).toBeCloseTo(9.5, 5);
  });
});

describe("instances", () => {
  it("makes one point per candidate and marks the chosen one", () => {
    const instances = buildInstances([
      entry({
        taken_epoch: at("2019-07-04T10:00:00"),
        candidates: [
          {
            provider: "exif",
            provider_info: null,
            taken: "2003-01-01 00:00:00",
            taken_epoch: at("2003-01-01T00:00:00"),
          },
          {
            provider: "filesystem",
            provider_info: null,
            taken: "2019-07-04 10:00:00",
            taken_epoch: at("2019-07-04T10:00:00"),
          },
        ],
      }),
    ]);

    expect(instances).toHaveLength(2);
    expect(instances.filter((i) => i.chosen)).toHaveLength(1);
    expect(instances.find((i) => i.chosen)?.lane).toBe(3);
  });

  it("still plots a file whose plan predates candidate recording", () => {
    const instances = buildInstances([entry({ candidates: [] })]);
    expect(instances).toHaveLength(1);
    expect(instances[0].chosen).toBe(true);
  });

  it("colours a flagged file apart from an agreed one", () => {
    const suspect = buildInstances([
      entry({ flags: [{ kind: "camera_epoch", description: "x", hard: true }] }),
    ]);
    const agreed = buildInstances([entry({ confidence: "high" })]);
    const decided = buildInstances([entry({ override_origin: "set by hand" })]);

    expect(suspect[0].tone).toBe(2);
    expect(agreed[0].tone).toBe(0);
    expect(decided[0].tone).toBe(3);
  });
});

describe("disagreement lines", () => {
  function twoCandidates(exif: string, fs: string, chosen: string) {
    return entry({
      taken_epoch: at(chosen),
      candidates: [
        { provider: "exif", provider_info: null, taken: exif, taken_epoch: at(exif) },
        { provider: "filesystem", provider_info: null, taken: fs, taken_epoch: at(fs) },
      ],
    });
  }

  it("joins candidates that are far apart", () => {
    const instances = buildInstances([
      twoCandidates("2003-01-01T00:00:00", "2019-07-04T10:00:00", "2019-07-04T10:00:00"),
    ]);
    expect(disagreementPairs(instances)).toHaveLength(2);
  });

  it("leaves candidates that agree unconnected", () => {
    const instances = buildInstances([
      twoCandidates("2019-07-04T10:00:00", "2019-07-04T11:00:00", "2019-07-04T10:00:00"),
    ]);
    expect(disagreementPairs(instances)).toHaveLength(0);
  });
});

describe("layouts", () => {
  const entries = [
    entry({ source: "/a", taken_epoch: at("2003-01-01T00:00:00") }),
    entry({ source: "/b", taken_epoch: at("2019-07-04T10:00:00") }),
    entry({ source: "/c", taken_epoch: at("2019-07-04T18:00:00") }),
  ];
  const instances = buildInstances(entries);
  const axis = buildTimeAxis(instances.map((i) => i.epoch));

  it("gives every mode one position per instance", () => {
    for (const build of [field, helix, terrain]) {
      expect(build(instances, axis)).toHaveLength(instances.length * 3);
    }
  });

  it("keeps the field ordered along time", () => {
    const positions = field(instances, axis);
    expect(positions[0]).toBeLessThan(positions[3]);
  });

  it("separates hours along depth in the terrain", () => {
    const positions = terrain(instances, axis);
    expect(positions[5]).toBeLessThan(positions[8]);
  });

  it("dispatches by mode name", () => {
    expect(layoutFor("helix", instances, axis)).toEqual(helix(instances, axis));
    expect(layoutFor("terrain", instances, axis)).toEqual(terrain(instances, axis));
    expect(layoutFor("field", instances, axis)).toEqual(field(instances, axis));
  });

  it("produces no NaN anywhere", () => {
    for (const build of [field, helix, terrain]) {
      expect([...build(instances, axis)].every(Number.isFinite)).toBe(true);
    }
  });
});

describe("morph easing", () => {
  it("is exact at both ends and monotonic between", () => {
    expect(ease(0)).toBe(0);
    expect(ease(1)).toBe(1);
    expect(ease(-1)).toBe(0);
    expect(ease(2)).toBe(1);

    let previous = -1;
    for (let t = 0; t <= 1; t += 0.05) {
      const value = ease(t);
      expect(value).toBeGreaterThanOrEqual(previous);
      previous = value;
    }
  });
});

describe("camera", () => {
  it("lands exactly on each end of an orbit transition", () => {
    const from = PRESETS.field;
    const to = PRESETS.terrain;
    expect(lerpOrbit(from, to, 0).radius).toBeCloseTo(from.radius, 6);
    expect(lerpOrbit(from, to, 1).radius).toBeCloseTo(to.radius, 6);
    expect(lerpOrbit(from, to, 1).phi).toBeCloseTo(to.phi, 6);
  });

  it("takes the short way around when spinning", () => {
    const from = { ...PRESETS.field, theta: 3.0 };
    const to = { ...PRESETS.field, theta: -3.0 };
    const middle = lerpOrbit(from, to, 0.5).theta;
    expect(Math.abs(middle)).toBeGreaterThan(3.0);
  });

  it("multiplies matrices in the order used for view projection", () => {
    const identity = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
    const p = perspective(0.9, 1.5, 0.1, 100);
    expect([...multiply(p, identity)]).toEqual([...p]);
  });

  it("puts a point in front of the camera on screen and one behind it off screen", () => {
    const mvp = multiply(
      perspective(0.9, 1, 0.1, 100),
      new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, -10, 1]),
    );
    const front = project([0, 0, 0], mvp);
    expect(front.visible).toBe(true);
    expect(front.x).toBeCloseTo(0.5, 5);
    expect(project([0, 0, 20], mvp).visible).toBe(false);
  });
});

describe("charts", () => {
  it("has no shading for a month with nothing in it", () => {
    expect(heatStep(0, 10)).toBeNull();
    expect(heatStep(1, 10)).not.toBeNull();
    expect(heatStep(10, 10)).toBe(HEAT_STEPS[HEAT_STEPS.length - 1]);
  });

  it("reads hours as wall-clock, not local time", () => {
    const bars = hourHistogram([
      entry({ taken_epoch: at("2019-03-04T00:00:00") }),
      entry({ taken_epoch: at("2019-03-05T00:00:00") }),
      entry({ taken_epoch: at("2019-03-04T13:00:00") }),
    ]);

    expect(bars).toHaveLength(24);
    expect(bars[0].files).toBe(2);
    expect(bars[13].files).toBe(1);
    expect(midnightShare(bars)).toBeCloseTo(2 / 3, 5);
    expect(midnightShare(hourHistogram([]))).toBe(0);
  });

  it("counts date sources in a fixed order, leaving out the unused ones", () => {
    const bars = providerCounts([
      entry({ provider: "filesystem" }),
      entry({ provider: "exif" }),
      entry({ provider: "exif" }),
    ]);

    expect(bars).toEqual([
      { provider: "exif", files: 2 },
      { provider: "filesystem", files: 1 },
    ]);
  });

  it("sorts a file into exactly one confidence bucket, hand-set dates first", () => {
    const flag = { kind: "future_date", description: "is in the future", hard: true };
    const bars = readingCounts([
      entry({ confidence: "high" }),
      entry({ confidence: "medium" }),
      entry({ confidence: "high", flags: [flag] }),
      entry({ confidence: "low", flags: [flag], override_origin: "set by hand" }),
    ]);

    expect(bars).toEqual([
      { reading: "agree", files: 1 },
      { reading: "single", files: 1 },
      { reading: "wrong", files: 1 },
      { reading: "decided", files: 1 },
    ]);
  });

  it("ranks destination folders by how many files land in them", () => {
    const bars = topFolders(
      [
        entry({ folder: "2019/03", size: 5 }),
        entry({ folder: "2019/03", size: 7 }),
        entry({ folder: "2021/11", size: 1 }),
        entry({ folder: "", size: 2 }),
      ],
      2,
    );

    expect(bars).toEqual([
      { folder: "2019/03", files: 2, bytes: 12 },
      { folder: "2021/11", files: 1, bytes: 1 },
    ]);
    expect(topFolders([entry({ folder: "" })], 5)[0].folder).toBe("the destination root");
  });

  it("describes an empty plan without dividing by zero", () => {
    expect(span([])).toEqual({ first: null, last: null, years: 0 });
    expect(formatYear(null)).toBe("—");
    expect(providerCounts([])).toEqual([]);
  });

  it("counts a span of less than a year as one year", () => {
    const reach = span([
      entry({ taken_epoch: at("2019-03-04T00:00:00") }),
      entry({ taken_epoch: at("2019-06-04T00:00:00") }),
    ]);
    expect(reach.years).toBe(1);
    expect(formatYear(reach.first)).toBe("2019");
  });

  it("picks round axis ticks that reach the top of the data", () => {
    const ticks = niceTicks(37, 4);
    expect(ticks[0]).toBe(0);
    expect(ticks[ticks.length - 1]).toBeGreaterThanOrEqual(37);
    expect(niceTicks(0, 4)).toEqual([0]);
  });
});

describe("time range drill-down", () => {
  const spread = [
    entry({ source: "/a", taken_epoch: at("2019-03-04T10:00:00") }),
    entry({ source: "/b", taken_epoch: at("2019-03-09T10:00:00") }),
    entry({ source: "/c", taken_epoch: at("2021-11-02T10:00:00") }),
  ];

  it("starts from whole years around the plan", () => {
    expect(planRange(spread)).toEqual({
      from: at("2019-01-01T00:00:00"),
      to: at("2022-01-01T00:00:00"),
    });
    expect(planRange([])).toBeNull();
  });

  it("picks a finer grid as the range narrows", () => {
    const level = (from: string, to: string) => levelFor({ from: at(from), to: at(to) });

    expect(level("2019-01-01T00:00:00", "2022-01-01T00:00:00")).toBe("years");
    expect(level("2019-01-01T00:00:00", "2020-01-01T00:00:00")).toBe("months");
    expect(level("2019-03-01T00:00:00", "2019-04-01T00:00:00")).toBe("days");
    expect(level("2019-03-04T00:00:00", "2019-03-05T00:00:00")).toBe("days");
  });

  it("counts a month per square with a row per year, filling in the empty years", () => {
    const grid = heatGrid(spread, planRange(spread)!);

    expect(grid.level).toBe("years");
    expect(grid.rows.map((row) => row.label)).toEqual(["2019", "2020", "2021"]);
    expect(grid.rows[0].cells[2]?.count).toBe(2);
    expect(grid.rows[1].count).toBe(0);
    expect(grid.rows[2].cells[10]?.count).toBe(1);
    expect(grid.max).toBe(2);
    expect(grid.total).toBe(3);
    expect(grid.cellCount).toBe(36);
    expect(grid.emptyCells).toBe(34);
  });

  it("counts a day per square inside one year, leaving out days the month has not got", () => {
    const grid = heatGrid([entry({ taken_epoch: at("2019-02-28T10:00:00") })], {
      from: at("2019-01-01T00:00:00"),
      to: at("2020-01-01T00:00:00"),
    });

    expect(grid.level).toBe("months");
    expect(grid.rows).toHaveLength(12);
    expect(grid.rows[1].label).toBe("Feb");
    expect(grid.rows[1].cells[27]?.count).toBe(1);
    expect(grid.rows[1].cells[28]).toBeNull();
    expect(grid.cellCount).toBe(365);
  });

  it("counts an hour per square inside one day", () => {
    const grid = heatGrid([entry({ taken_epoch: at("2019-03-04T10:30:00") })], {
      from: at("2019-03-04T00:00:00"),
      to: at("2019-03-05T00:00:00"),
    });

    expect(grid.level).toBe("days");
    expect(grid.rows).toHaveLength(1);
    expect(grid.rows[0].label).toBe("4");
    expect(grid.rows[0].cells[10]?.count).toBe(1);
    expect(grid.cellCount).toBe(24);
  });

  it("blanks the squares either side of a range that starts mid-year", () => {
    const grid = heatGrid(spread, {
      from: at("2019-03-01T00:00:00"),
      to: at("2021-01-01T00:00:00"),
    });

    expect(grid.level).toBe("years");
    expect(grid.rows[0].cells[0]).toBeNull();
    expect(grid.rows[0].cells[2]?.count).toBe(2);
    expect(grid.rows[0].from).toBe(at("2019-03-01T00:00:00"));
    expect(grid.total).toBe(2);
  });

  it("hands a row back as the range its own files sit in", () => {
    const grid = heatGrid(spread, planRange(spread)!);
    const row = grid.rows[0];

    expect(filterRange(spread, { from: row.from, to: row.to }).map((e) => e.source)).toEqual([
      "/a",
      "/b",
    ]);
  });

  it("stretches a dragged selection from the first square to the last", () => {
    const grid = heatGrid(spread, planRange(spread)!);
    const first = grid.rows[0].cells[2]!;
    const last = grid.rows[2].cells[10]!;

    expect(selectionRange(grid, last.index, first.index)).toEqual({
      from: at("2019-03-01T00:00:00"),
      to: at("2021-12-01T00:00:00"),
    });
    expect(selectionRange(grid, -5, -1)).toBeNull();
  });

  it("keeps the end of a range out of it", () => {
    const edges = [
      entry({ source: "/start", taken_epoch: at("2019-01-01T00:00:00") }),
      entry({ source: "/end", taken_epoch: at("2019-02-01T00:00:00") }),
    ];
    const kept = filterRange(edges, {
      from: at("2019-01-01T00:00:00"),
      to: at("2019-02-01T00:00:00"),
    });

    expect(kept.map((e) => e.source)).toEqual(["/start"]);
    expect(filterRange(edges, null)).toHaveLength(2);
  });

  it("names a range by the coarsest unit it lines up with", () => {
    const label = (from: string, to: string) => rangeLabel({ from: at(from), to: at(to) });

    expect(label("2019-01-01T00:00:00", "2020-01-01T00:00:00")).toBe("2019");
    expect(label("2019-01-01T00:00:00", "2022-01-01T00:00:00")).toBe("2019–2021");
    expect(label("2019-03-01T00:00:00", "2019-04-01T00:00:00")).toBe("Mar 2019");
    expect(label("2019-03-01T00:00:00", "2019-06-01T00:00:00")).toBe("Mar 2019 – May 2019");
    expect(label("2019-03-04T00:00:00", "2019-03-05T00:00:00")).toBe("4 Mar 2019");
    expect(label("2019-03-04T10:00:00", "2019-03-04T12:00:00")).toBe(
      "4 Mar 2019 10:00 – 4 Mar 2019 12:00",
    );
  });

  it("tells two ranges apart, and no range from a range", () => {
    const range = { from: at("2019-01-01T00:00:00"), to: at("2020-01-01T00:00:00") };

    expect(sameRange(range, { ...range })).toBe(true);
    expect(sameRange(range, { ...range, to: range.to + 1 })).toBe(false);
    expect(sameRange(null, null)).toBe(true);
    expect(sameRange(range, null)).toBe(false);
  });
});

describe("auto-fly", () => {
  const instances = (count: number) =>
    Array.from({ length: count }, (_, i) => ({
      entry: i,
      lane: 0,
      epoch: 0,
      chosen: true,
      tone: 0,
    }));

  it("lays waypoints across the whole spread of the data", () => {
    const positions = new Float32Array([-10, 1, 0, 0, 2, 0, 10, 3, 0]);
    const points = flightWaypoints(positions, instances(3), 8);

    expect(points).toHaveLength(8);
    expect(points[0].x).toBeLessThan(points[points.length - 1].x);
    expect(points[0].x).toBeGreaterThanOrEqual(-10);
    expect(points[points.length - 1].x).toBeLessThanOrEqual(10);
  });

  it("carries a height across bins that hold nothing", () => {
    const positions = new Float32Array([-10, 5, 0, 10, 5, 0]);
    const points = flightWaypoints(positions, instances(2), 6);

    expect(points).toHaveLength(6);
    for (const point of points) expect(point.y).toBeCloseTo(5, 3);
  });

  it("has no path to fly when there is nothing, or everything sits on one spot", () => {
    expect(flightWaypoints(new Float32Array(0), [], 8)).toEqual([]);
    expect(flightWaypoints(new Float32Array([1, 1, 1]), instances(1), 8)).toEqual([]);
  });

  it("passes through its first and last waypoint", () => {
    const points = flightWaypoints(
      new Float32Array([-6, 0, 0, 0, 1, 0, 6, 0, 0]),
      instances(3),
      5,
    );
    expect(samplePath(points, 0).x).toBeCloseTo(points[0].x, 5);
    expect(samplePath(points, 1).x).toBeCloseTo(points[points.length - 1].x, 5);
  });

  it("moves forward the whole way without doubling back", () => {
    const points = flightWaypoints(
      new Float32Array([-8, 0, 0, -2, 3, 1, 4, -1, 2, 9, 2, 0]),
      instances(4),
      12,
    );

    let last = -Infinity;
    for (let step = 0; step <= 40; step += 1) {
      const x = samplePath(points, step / 40).x;
      expect(x).toBeGreaterThan(last - 1e-6);
      last = x;
    }
  });

  it("keeps the camera close enough for the pictures to show", () => {
    const points = flightWaypoints(
      new Float32Array([-8, 0, 0, 8, 0, 0]),
      instances(2),
      10,
    );

    for (let step = 0; step <= 20; step += 1) {
      const orbit = flightOrbit(points, step / 20);
      expect(orbit.radius).toBeLessThan(TILE_RADIUS);
      expect(orbit.radius).toBeGreaterThan(MIN_RADIUS);
      expect(orbit.phi).toBeGreaterThan(0);
      expect(orbit.phi).toBeLessThan(Math.PI);
    }
  });

  it("survives being asked for a frame of an empty flight", () => {
    expect(samplePath([], 0.5)).toEqual({ x: 0, y: 0, z: 0 });
    expect(flightOrbit([], 0.5).target).toEqual([0, 0, 0]);
  });
});

describe("level of detail", () => {
  it("shows nothing extra from far away and everything up close", () => {
    expect(detailFor(DETAIL_FAR)).toBe(0);
    expect(detailFor(DETAIL_FAR + 30)).toBe(0);
    expect(detailFor(DETAIL_NEAR)).toBe(1);
    expect(detailFor(1)).toBe(1);
  });

  it("grows without a jump as the camera comes in", () => {
    let previous = -1;
    for (let radius = DETAIL_FAR + 5; radius >= 1; radius -= 0.5) {
      const detail = detailFor(radius);
      expect(detail).toBeGreaterThanOrEqual(previous - 1e-9);
      expect(detail).toBeGreaterThanOrEqual(0);
      expect(detail).toBeLessThanOrEqual(1);
      previous = detail;
    }
  });

  it("hands over to the pictures exactly where they start appearing", () => {
    expect(DETAIL_NEAR).toBe(TILE_RADIUS);
    expect(detailFor(TILE_RADIUS)).toBe(1);
    expect(tileFade(TILE_RADIUS)).toBe(0);
  });

  it("is halfway along partway between the two ends", () => {
    const middle = detailFor((DETAIL_FAR + DETAIL_NEAR) / 2);
    expect(middle).toBeCloseTo(0.5, 5);
  });
});

describe("media tiles", () => {
  const mvp = () =>
    multiply(
      perspective(0.9, 1, 0.1, 100),
      new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, -10, 1]),
    );

  it("shows nothing until the camera is close, then fades all the way in", () => {
    expect(tileFade(TILE_RADIUS)).toBe(0);
    expect(tileFade(TILE_RADIUS + 5)).toBe(0);
    expect(tileFade(TILE_FULL_RADIUS)).toBe(1);
    expect(tileFade(1)).toBe(1);
    const middle = tileFade((TILE_RADIUS + TILE_FULL_RADIUS) / 2);
    expect(middle).toBeGreaterThan(0);
    expect(middle).toBeLessThan(1);
  });

  it("draws only the chosen reading of each file", () => {
    const instances = buildInstances([
      entry({
        source: "/src/a.jpg",
        candidates: [
          { provider: "exif", provider_info: null, taken: "2003-01-01 00:00:00", taken_epoch: at("2003-01-01T00:00:00") },
          { provider: "filename", provider_info: null, taken: "2019-07-04 00:00:00", taken_epoch: at("2019-07-04T00:00:00") },
        ],
      }),
    ]);
    const positions = new Float32Array(instances.length * 3);

    const tiles = tilesFrom(positions, instances, mvp(), 1, 10);
    expect(instances.length).toBeGreaterThan(1);
    expect(tiles).toHaveLength(1);
    expect(tiles[0].entry).toBe(0);
  });

  it("gives nearer files bigger tiles and drops what the camera cannot see", () => {
    const instances = [entry({ source: "/a.jpg" }), entry({ source: "/b.jpg" })].flatMap((e) =>
      buildInstances([e]),
    );
    instances[1].entry = 1;
    const positions = new Float32Array([0, 0, 5, 0, 0, -5]);

    const tiles = tilesFrom(positions, instances, mvp(), 1, 10);
    expect(tiles.map((t) => t.entry)).toEqual([0, 1]);
    expect(tiles[0].size).toBeGreaterThan(tiles[1].size);

    const behind = tilesFrom(new Float32Array([0, 0, 20, 0, 0, 30]), instances, mvp(), 1, 10);
    expect(behind).toHaveLength(0);
  });

  it("keeps only the nearest tiles once the limit is reached", () => {
    const instances = [0, 1, 2].flatMap((index) => {
      const built = buildInstances([entry({ source: `/${index}.jpg` })]);
      built.forEach((i) => (i.entry = index));
      return built;
    });
    const positions = new Float32Array([0, 0, 6, 0, 0, 0, 0, 0, 3]);

    const tiles = tilesFrom(positions, instances, mvp(), 1, 2);
    expect(tiles.map((t) => t.entry)).toEqual([0, 2]);
  });

  it("never lets a tile swallow the screen or shove itself into the lens", () => {
    const instances = [entry({ source: "/a.jpg" }), entry({ source: "/b.jpg" })].flatMap((e) =>
      buildInstances([e]),
    );
    instances[1].entry = 1;
    const mvp = multiply(
      perspective(0.9, 1, 0.1, 100),
      new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, -10, 1]),
    );

    const tiles = tilesFrom(new Float32Array([0, 0, 9.5, 0, 0, 5]), instances, mvp, 1, 10);
    expect(tiles.every((tile) => tile.size <= MAX_TILE_SIZE)).toBe(true);
    expect(tiles.every((tile) => tile.depth >= MIN_TILE_DEPTH)).toBe(true);
    expect(tiles.map((t) => t.entry)).not.toContain(0);
  });

  it("costs nothing while the camera is far away", () => {
    const instances = buildInstances([entry()]);
    expect(tilesFrom(new Float32Array(3), instances, mvp(), 0, 10)).toHaveLength(0);
  });
});

describe("pick ids", () => {
  it("round-trips through the colour encoding", () => {
    for (const id of [0, 1, 255, 256, 65535, 65536, 199999]) {
      const value = id + 1;
      const pixel = new Uint8Array([
        value % 256,
        Math.floor(value / 256) % 256,
        Math.floor(value / 65536) % 256,
        255,
      ]);
      expect(decodeId(pixel)).toBe(id);
    }
  });

  it("reads the cleared background as nothing", () => {
    expect(decodeId(new Uint8Array([0, 0, 0, 255]))).toBe(-1);
  });
});
