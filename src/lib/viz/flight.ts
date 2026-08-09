import type { Instance } from "./layouts";
import type { Orbit } from "./camera";

export const FLIGHT_MS = 24_000;
export const FLIGHT_BINS = 48;
const CRUISE_RADIUS = 6.2;
const SWOOP = 2.1;
const SWOOPS = 3;
const SPINS = 1.5;

export interface Waypoint {
  x: number;
  y: number;
  z: number;
}

export function flightWaypoints(
  positions: Float32Array,
  instances: Instance[],
  bins: number,
): Waypoint[] {
  if (instances.length === 0 || bins < 2) return [];

  let low = Infinity;
  let high = -Infinity;
  for (let i = 0; i < instances.length; i += 1) {
    const x = positions[i * 3];
    if (x < low) low = x;
    if (x > high) high = x;
  }
  if (!Number.isFinite(low) || high - low < 1e-6) return [];

  const sums = new Float64Array(bins * 3);
  const counts = new Int32Array(bins);
  const width = (high - low) / bins;

  for (let i = 0; i < instances.length; i += 1) {
    const at = i * 3;
    const bin = Math.min(bins - 1, Math.floor((positions[at] - low) / width));
    sums[bin * 3] += positions[at];
    sums[bin * 3 + 1] += positions[at + 1];
    sums[bin * 3 + 2] += positions[at + 2];
    counts[bin] += 1;
  }

  const filled: { bin: number; point: Waypoint }[] = [];
  for (let bin = 0; bin < bins; bin += 1) {
    if (counts[bin] === 0) continue;
    filled.push({
      bin,
      point: {
        x: sums[bin * 3] / counts[bin],
        y: sums[bin * 3 + 1] / counts[bin],
        z: sums[bin * 3 + 2] / counts[bin],
      },
    });
  }
  if (filled.length === 0) return [];

  const out: Waypoint[] = [];
  for (let bin = 0; bin < bins; bin += 1) {
    const after = filled.find((f) => f.bin >= bin) ?? filled[filled.length - 1];
    const before = [...filled].reverse().find((f) => f.bin <= bin) ?? filled[0];
    if (before.bin === after.bin) {
      out.push({ ...before.point, x: low + (bin + 0.5) * width });
      continue;
    }
    const t = (bin - before.bin) / (after.bin - before.bin);
    out.push({
      x: low + (bin + 0.5) * width,
      y: before.point.y + (after.point.y - before.point.y) * t,
      z: before.point.z + (after.point.z - before.point.z) * t,
    });
  }
  return out;
}

export function samplePath(waypoints: Waypoint[], progress: number): Waypoint {
  if (waypoints.length === 0) return { x: 0, y: 0, z: 0 };
  if (waypoints.length === 1) return waypoints[0];

  const clamped = Math.min(1, Math.max(0, progress));
  const scaled = clamped * (waypoints.length - 1);
  const index = Math.min(waypoints.length - 2, Math.floor(scaled));
  const t = scaled - index;

  const p0 = waypoints[Math.max(0, index - 1)];
  const p1 = waypoints[index];
  const p2 = waypoints[index + 1];
  const p3 = waypoints[Math.min(waypoints.length - 1, index + 2)];

  return {
    x: spline(p0.x, p1.x, p2.x, p3.x, t),
    y: spline(p0.y, p1.y, p2.y, p3.y, t),
    z: spline(p0.z, p1.z, p2.z, p3.z, t),
  };
}

function spline(a: number, b: number, c: number, d: number, t: number): number {
  const t2 = t * t;
  const t3 = t2 * t;
  return (
    0.5 *
    (2 * b + (c - a) * t + (2 * a - 5 * b + 4 * c - d) * t2 + (-a + 3 * b - 3 * c + d) * t3)
  );
}

export function flightOrbit(waypoints: Waypoint[], progress: number): Orbit {
  const at = samplePath(waypoints, progress);
  const angle = progress * Math.PI * 2 * SPINS;
  return {
    theta: angle,
    phi: 1.34 + Math.sin(progress * Math.PI * 2 * SWOOPS) * 0.16,
    radius: CRUISE_RADIUS + Math.sin(progress * Math.PI * 2 * SWOOPS) * SWOOP,
    target: [at.x, at.y, at.z],
  };
}
