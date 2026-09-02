import type { Mode } from "./layouts";

export interface Orbit {
  theta: number;
  phi: number;
  radius: number;
  target: [number, number, number];
}

export const PRESETS: Record<Mode, Orbit> = {
  field: { theta: -0.35, phi: 1.28, radius: 20, target: [0, 0, 0] },
  helix: { theta: 0.6, phi: 1.05, radius: 22, target: [0, 0, 0] },
  terrain: { theta: -0.9, phi: 1.02, radius: 21, target: [0, -1, 0] },
};

export const MIN_PHI = 0.08;
export const MAX_PHI = Math.PI - 0.08;
export const MIN_RADIUS = 4;
export const MAX_RADIUS = 70;

export function clone(orbit: Orbit): Orbit {
  return { ...orbit, target: [...orbit.target] as [number, number, number] };
}

export function lerpOrbit(from: Orbit, to: Orbit, t: number): Orbit {
  return {
    theta: from.theta + shortestAngle(from.theta, to.theta) * t,
    phi: from.phi + (to.phi - from.phi) * t,
    radius: from.radius + (to.radius - from.radius) * t,
    target: [
      from.target[0] + (to.target[0] - from.target[0]) * t,
      from.target[1] + (to.target[1] - from.target[1]) * t,
      from.target[2] + (to.target[2] - from.target[2]) * t,
    ],
  };
}

function shortestAngle(from: number, to: number): number {
  const two = Math.PI * 2;
  return ((((to - from) % two) + Math.PI * 3) % two) - Math.PI;
}

export function eyeOf(orbit: Orbit): [number, number, number] {
  const sinPhi = Math.sin(orbit.phi);
  return [
    orbit.target[0] + orbit.radius * sinPhi * Math.sin(orbit.theta),
    orbit.target[1] + orbit.radius * Math.cos(orbit.phi),
    orbit.target[2] + orbit.radius * sinPhi * Math.cos(orbit.theta),
  ];
}

export function viewProjection(orbit: Orbit, aspect: number): Float32Array {
  return multiply(perspective(0.9, aspect, 0.1, 400), lookAt(eyeOf(orbit), orbit.target));
}

export function perspective(fovY: number, aspect: number, near: number, far: number): Float32Array {
  const f = 1 / Math.tan(fovY / 2);
  const range = 1 / (near - far);
  return new Float32Array([
    f / aspect, 0, 0, 0,
    0, f, 0, 0,
    0, 0, (near + far) * range, -1,
    0, 0, near * far * range * 2, 0,
  ]);
}

export function lookAt(eye: number[], target: number[]): Float32Array {
  const zAxis = normalise([eye[0] - target[0], eye[1] - target[1], eye[2] - target[2]]);
  const xAxis = normalise(cross([0, 1, 0], zAxis));
  const yAxis = cross(zAxis, xAxis);

  return new Float32Array([
    xAxis[0], yAxis[0], zAxis[0], 0,
    xAxis[1], yAxis[1], zAxis[1], 0,
    xAxis[2], yAxis[2], zAxis[2], 0,
    -dot(xAxis, eye), -dot(yAxis, eye), -dot(zAxis, eye), 1,
  ]);
}

export function multiply(a: Float32Array, b: Float32Array): Float32Array {
  const out = new Float32Array(16);
  for (let row = 0; row < 4; row += 1) {
    for (let column = 0; column < 4; column += 1) {
      let sum = 0;
      for (let k = 0; k < 4; k += 1) {
        sum += a[k * 4 + column] * b[row * 4 + k];
      }
      out[row * 4 + column] = sum;
    }
  }
  return out;
}

export function project(
  point: [number, number, number],
  matrix: Float32Array,
): { x: number; y: number; depth: number; visible: boolean } {
  const w =
    matrix[3] * point[0] + matrix[7] * point[1] + matrix[11] * point[2] + matrix[15];
  const x = matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12];
  const y = matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13];
  if (w <= 0.0001) return { x: 0, y: 0, depth: w, visible: false };
  return { x: (x / w + 1) / 2, y: 1 - (y / w + 1) / 2, depth: w, visible: true };
}

function cross(a: number[], b: number[]): number[] {
  return [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ];
}

function dot(a: number[], b: number[]): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

function normalise(v: number[]): number[] {
  const length = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / length, v[1] / length, v[2] / length];
}
