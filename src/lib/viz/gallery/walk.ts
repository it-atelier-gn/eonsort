import type { Solid } from "./layout";

export const WALK_SPEED = 4.4;
export const RUN_SPEED = 10.5;
export const RADIUS = 0.42;
export const LOOK_SPEED = 0.0022;
export const MAX_PITCH = 1.45;
const ACCELERATION = 12;

export interface Walker {
  x: number;
  z: number;
  yaw: number;
  pitch: number;
  vx: number;
  vz: number;
}

export interface Intent {
  forward: number;
  strafe: number;
  running: boolean;
}

export function look(walker: Walker, dx: number, dy: number): Walker {
  return {
    ...walker,
    yaw: walker.yaw - dx * LOOK_SPEED,
    pitch: clamp(walker.pitch - dy * LOOK_SPEED, -MAX_PITCH, MAX_PITCH),
  };
}

export function step(walker: Walker, intent: Intent, solids: Solid[], seconds: number): Walker {
  const dt = Math.min(0.05, Math.max(0, seconds));
  const speed = intent.running ? RUN_SPEED : WALK_SPEED;

  const sin = Math.sin(walker.yaw);
  const cos = Math.cos(walker.yaw);
  let wishX = -sin * intent.forward + cos * intent.strafe;
  let wishZ = -cos * intent.forward - sin * intent.strafe;

  const length = Math.hypot(wishX, wishZ);
  if (length > 1) {
    wishX /= length;
    wishZ /= length;
  }

  const blend = Math.min(1, ACCELERATION * dt);
  const vx = walker.vx + (wishX * speed - walker.vx) * blend;
  const vz = walker.vz + (wishZ * speed - walker.vz) * blend;

  const moved = slide(walker.x, walker.z, vx * dt, vz * dt, solids);

  return {
    ...walker,
    x: moved.x,
    z: moved.z,
    vx: moved.hitX ? 0 : vx,
    vz: moved.hitZ ? 0 : vz,
  };
}

export function slide(
  x: number,
  z: number,
  dx: number,
  dz: number,
  solids: Solid[],
): { x: number; z: number; hitX: boolean; hitZ: boolean } {
  const steps = Math.max(1, Math.ceil(Math.hypot(dx, dz) / (RADIUS * 0.5)));
  const stepX = dx / steps;
  const stepZ = dz / steps;

  let atX = x;
  let atZ = z;
  let hitX = false;
  let hitZ = false;

  for (let i = 0; i < steps; i += 1) {
    if (!hitX) {
      if (blocked(atX + stepX, atZ, solids)) hitX = true;
      else atX += stepX;
    }
    if (!hitZ) {
      if (blocked(atX, atZ + stepZ, solids)) hitZ = true;
      else atZ += stepZ;
    }
    if (hitX && hitZ) break;
  }

  return { x: atX, z: atZ, hitX, hitZ };
}

export function blocked(x: number, z: number, solids: Solid[]): boolean {
  for (const solid of solids) {
    if (
      x > solid.x0 - RADIUS &&
      x < solid.x1 + RADIUS &&
      z > solid.z0 - RADIUS &&
      z < solid.z1 + RADIUS
    ) {
      return true;
    }
  }
  return false;
}

export function eyeTarget(walker: Walker, height: number): {
  eye: [number, number, number];
  at: [number, number, number];
} {
  const cosPitch = Math.cos(walker.pitch);
  return {
    eye: [walker.x, height, walker.z],
    at: [
      walker.x - Math.sin(walker.yaw) * cosPitch,
      height + Math.sin(walker.pitch),
      walker.z - Math.cos(walker.yaw) * cosPitch,
    ],
  };
}

function clamp(value: number, low: number, high: number): number {
  return value < low ? low : value > high ? high : value;
}
