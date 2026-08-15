import { perspective } from "../camera";
import { RADIUS } from "../gallery/walk";
import type { Solid } from "../gallery/layout";
import { clampFit, EYE_HEIGHT, type SceneFit } from "./fit";
import { placeBillboards } from "./billboards";
import type { Bounds, Face, FaceKey, Scene, Vec3 } from "./model";

export const SKIRT = 0.9;
export const WALL = 0.5;
export const MAX_DEPTH = 90;
export const MIN_ROOM = RADIUS * 2 + 0.3;
export const FLAT_AREA = 0.75;
export const LOW_CEILING = 1.9;

type Axis = 0 | 1 | 2;

interface Plan {
  key: FaceKey;
  axis: Axis;
  value: number;
  points: [number, number][];
  normal: Vec3;
  shade: number;
  skirt: boolean;
}

export function fovOf(focal: number): number {
  return 2 * Math.atan(0.5 / focal);
}

export function buildScene(input: SceneFit, aspect: number): Scene {
  const fit = clampFit(input);
  const ratio = Number.isFinite(aspect) && aspect > 0 ? aspect : 1;
  const { vp, rect } = fit;
  const focal = fit.focal;
  const warnings: string[] = [];

  const px = (u: number) => (u - vp.u) * ratio;
  const py = (v: number) => vp.v - v;

  let depth = (EYE_HEIGHT * focal) / (rect.v1 - vp.v);
  if (depth > MAX_DEPTH) {
    depth = MAX_DEPTH;
    warnings.push("The back wall is so close to the horizon that the room had to be cut short.");
  }

  const scale = depth / focal;
  const xl = px(rect.u0) * scale;
  const xr = px(rect.u1) * scale;
  const yt = py(rect.v0) * scale;
  const yb = py(rect.v1) * scale;
  const lift = -yb;

  const point = (u: number, v: number, axis: Axis, value: number): Vec3 | null => {
    const dir = [px(u), py(v), -focal];
    const along = dir[axis];
    if (Math.abs(along) < 1e-6) return null;
    const t = value / along;
    if (!Number.isFinite(t) || t <= 0) return null;
    return { x: dir[0] * t, y: dir[1] * t + lift, z: dir[2] * t };
  };

  const plans: Plan[] = [
    {
      key: "back",
      axis: 2,
      value: -depth,
      points: [
        [rect.u0, rect.v0],
        [rect.u1, rect.v0],
        [rect.u1, rect.v1],
        [rect.u0, rect.v1],
      ],
      normal: { x: 0, y: 0, z: 1 },
      shade: 1,
      skirt: false,
    },
    {
      key: "floor",
      axis: 1,
      value: yb,
      points: [
        [0, 1],
        [1, 1],
        [1, rect.v1],
        [0, rect.v1],
      ],
      normal: { x: 0, y: 1, z: 0 },
      shade: 0.94,
      skirt: true,
    },
    {
      key: "ceiling",
      axis: 1,
      value: yt,
      points: [
        [0, 0],
        [1, 0],
        [1, rect.v0],
        [0, rect.v0],
      ],
      normal: { x: 0, y: -1, z: 0 },
      shade: 1.04,
      skirt: true,
    },
    {
      key: "left",
      axis: 0,
      value: xl,
      points: [
        [0, 0],
        [0, 1],
        [rect.u0, 1],
        [rect.u0, 0],
      ],
      normal: { x: 1, y: 0, z: 0 },
      shade: 0.97,
      skirt: true,
    },
    {
      key: "right",
      axis: 0,
      value: xr,
      points: [
        [1, 0],
        [1, 1],
        [rect.u1, 1],
        [rect.u1, 0],
      ],
      normal: { x: -1, y: 0, z: 0 },
      shade: 0.97,
      skirt: true,
    },
  ];

  const faces: Face[] = [];
  for (const plan of plans) {
    const corners = plan.points.map(([u, v]) => point(u, v, plan.axis, plan.value));
    if (corners.some((corner) => corner === null)) {
      warnings.push("Part of the picture could not be turned into a wall.");
      continue;
    }

    const solid = corners as Vec3[];
    const uvs = plan.points.flat() as Face["uvs"];
    faces.push({
      key: plan.key,
      skirt: false,
      corners: [solid[0], solid[1], solid[2], solid[3]],
      uvs,
      normal: plan.normal,
      shade: plan.shade,
    });

    if (!plan.skirt) continue;
    if (solid[0].z >= SKIRT && solid[1].z >= SKIRT) continue;

    faces.push({
      key: plan.key,
      skirt: true,
      corners: [
        { x: solid[0].x, y: solid[0].y, z: SKIRT },
        { x: solid[1].x, y: solid[1].y, z: SKIRT },
        solid[1],
        solid[0],
      ],
      uvs: [uvs[0], uvs[1], uvs[2], uvs[3], uvs[2], uvs[3], uvs[0], uvs[1]],
      normal: plan.normal,
      shade: plan.shade,
    });
  }

  const ceiling = yt + lift;
  const bounds: Bounds = { x0: xl, x1: xr, y0: 0, y1: ceiling, z0: -depth, z1: SKIRT };

  let guardL = xl;
  let guardR = xr;
  if (xr - xl < MIN_ROOM) {
    const middle = (xl + xr) / 2;
    guardL = middle - (RADIUS + 0.15);
    guardR = middle + (RADIUS + 0.15);
    warnings.push("The back wall is so wide that the room is barely a corridor.");
  }

  const solids: Solid[] = [
    { x0: guardL - WALL, x1: guardL, z0: -depth - WALL, z1: SKIRT + WALL },
    { x0: guardR, x1: guardR + WALL, z0: -depth - WALL, z1: SKIRT + WALL },
    { x0: guardL - WALL, x1: guardR + WALL, z0: -depth - WALL, z1: -depth },
    { x0: guardL - WALL, x1: guardR + WALL, z0: SKIRT, z1: SKIRT + WALL },
  ];

  const area = (rect.u1 - rect.u0) * (rect.v1 - rect.v0);
  if (area > FLAT_AREA) {
    warnings.push("The back wall covers most of the picture, so there is little depth to walk into.");
  }
  if (ceiling < LOW_CEILING) {
    warnings.push("The ceiling comes out lower than head height.");
  }

  const scene: Scene = {
    fit,
    aspect: ratio,
    focal,
    fovY: fovOf(focal),
    eyeHeight: lift,
    depth,
    faces,
    billboards: [],
    solids,
    bounds,
    spawn: { x: 0, z: 0, yaw: 0, pitch: 0 },
    warnings,
  };

  const placed = placeBillboards(scene);
  scene.billboards = placed.billboards;
  scene.solids = [...solids, ...placed.solids];
  return scene;
}

export function projection(
  scene: Scene,
  canvasAspect: number,
  near: number,
  far: number,
): Float32Array {
  const aspect = Number.isFinite(canvasAspect) && canvasAspect > 0 ? canvasAspect : scene.aspect;
  const matrix = perspective(scene.fovY, aspect, near, far);
  matrix[8] = ((1 - 2 * scene.fit.vp.u) * scene.aspect) / aspect;
  matrix[9] = 2 * scene.fit.vp.v - 1;
  return matrix;
}
