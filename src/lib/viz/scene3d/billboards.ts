import type { Solid } from "../gallery/layout";
import { RADIUS } from "../gallery/walk";
import type { Billboard, Scene } from "./model";

export const BOARD_DEPTH = 0.16;
export const MIN_BOARD = 0.25;

export function placeBillboards(scene: Scene): { billboards: Billboard[]; solids: Solid[] } {
  const { vp } = scene.fit;
  const lift = scene.eyeHeight;
  const yb = -lift;

  const px = (u: number) => (u - vp.u) * scene.aspect;
  const py = (v: number) => vp.v - v;

  const billboards: Billboard[] = [];
  const solids: Solid[] = [];

  for (const object of scene.fit.objects) {
    const foot = py(object.v1);
    if (foot >= -1e-4) continue;

    const t = yb / foot;
    if (!Number.isFinite(t) || t <= 0) continue;

    const middle = (object.u0 + object.u1) / 2;
    const x = px(middle) * t;
    const z = -scene.focal * t;
    if (-z > scene.depth) continue;
    if (x < scene.bounds.x0 || x > scene.bounds.x1) continue;

    const width = (px(object.u1) - px(object.u0)) * t;
    const height = (py(object.v0) - foot) * t;
    if (width < MIN_BOARD || height < MIN_BOARD) continue;

    const solid: Solid = {
      x0: x - width / 2,
      x1: x + width / 2,
      z0: z - BOARD_DEPTH,
      z1: z + BOARD_DEPTH,
    };

    if (swallows(solid, scene.spawn.x, scene.spawn.z)) continue;

    billboards.push({
      label: object.label,
      x,
      z,
      width,
      height,
      u0: object.u0,
      v0: object.v0,
      u1: object.u1,
      v1: object.v1,
    });
    solids.push(solid);
  }

  return { billboards, solids };
}

function swallows(solid: Solid, x: number, z: number): boolean {
  return (
    x > solid.x0 - RADIUS && x < solid.x1 + RADIUS && z > solid.z0 - RADIUS && z < solid.z1 + RADIUS
  );
}
