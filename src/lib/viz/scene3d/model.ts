import type { Solid } from "../gallery/layout";
import type { SceneFit } from "./fit";

export interface Vec3 {
  x: number;
  y: number;
  z: number;
}

export type FaceKey = "back" | "floor" | "ceiling" | "left" | "right";

export interface Face {
  key: FaceKey;
  skirt: boolean;
  corners: [Vec3, Vec3, Vec3, Vec3];
  uvs: [number, number, number, number, number, number, number, number];
  normal: Vec3;
  shade: number;
}

export interface Billboard {
  label: string;
  x: number;
  z: number;
  width: number;
  height: number;
  u0: number;
  v0: number;
  u1: number;
  v1: number;
}

export interface Bounds {
  x0: number;
  x1: number;
  y0: number;
  y1: number;
  z0: number;
  z1: number;
}

export interface Spawn {
  x: number;
  z: number;
  yaw: number;
  pitch: number;
}

export interface Scene {
  fit: SceneFit;
  aspect: number;
  focal: number;
  fovY: number;
  eyeHeight: number;
  depth: number;
  faces: Face[];
  billboards: Billboard[];
  solids: Solid[];
  bounds: Bounds;
  spawn: Spawn;
  warnings: string[];
}

export interface Surface {
  position: Float32Array;
  normal: Float32Array;
  uv: Float32Array;
  shade: Float32Array;
  count: number;
}

export const EMPTY_SURFACE: Surface = {
  position: new Float32Array(0),
  normal: new Float32Array(0),
  uv: new Float32Array(0),
  shade: new Float32Array(0),
  count: 0,
};

const ORDER = [0, 1, 2, 0, 2, 3];

export function surfaceOf(scene: Scene): Surface {
  return surfaceOfFaces(scene.faces);
}

export function surfaceOfFaces(faces: Face[]): Surface {
  const count = faces.length * 6;
  const position = new Float32Array(count * 3);
  const normal = new Float32Array(count * 3);
  const uv = new Float32Array(count * 2);
  const shade = new Float32Array(count);

  let vertex = 0;
  for (const face of faces) {
    for (const index of ORDER) {
      const corner = face.corners[index];
      position[vertex * 3] = corner.x;
      position[vertex * 3 + 1] = corner.y;
      position[vertex * 3 + 2] = corner.z;
      normal[vertex * 3] = face.normal.x;
      normal[vertex * 3 + 1] = face.normal.y;
      normal[vertex * 3 + 2] = face.normal.z;
      uv[vertex * 2] = face.uvs[index * 2];
      uv[vertex * 2 + 1] = face.uvs[index * 2 + 1];
      shade[vertex] = face.shade;
      vertex += 1;
    }
  }

  return { position, normal, uv, shade, count };
}

const LOCAL: [number, number][] = [
  [-1, 1],
  [1, 1],
  [1, -1],
  [-1, -1],
];

function quads(
  boards: Billboard[],
  corner: (board: Billboard, index: number) => Vec3,
  coords: (board: Billboard, index: number) => [number, number],
): Surface {
  const count = boards.length * 6;
  const position = new Float32Array(count * 3);
  const normal = new Float32Array(count * 3);
  const uv = new Float32Array(count * 2);
  const shade = new Float32Array(count);

  let vertex = 0;
  for (const board of boards) {
    for (const index of ORDER) {
      const point = corner(board, index);
      const texture = coords(board, index);
      position[vertex * 3] = point.x;
      position[vertex * 3 + 1] = point.y;
      position[vertex * 3 + 2] = point.z;
      normal[vertex * 3] = LOCAL[index][0];
      normal[vertex * 3 + 1] = LOCAL[index][1];
      normal[vertex * 3 + 2] = 0;
      uv[vertex * 2] = texture[0];
      uv[vertex * 2 + 1] = texture[1];
      shade[vertex] = 1;
      vertex += 1;
    }
  }

  return { position, normal, uv, shade, count };
}

export function billboardQuads(scene: Scene): Surface {
  return quads(
    scene.billboards,
    (board, index) => ({
      x: board.x + (LOCAL[index][0] * board.width) / 2,
      y: LOCAL[index][1] > 0 ? board.height : 0,
      z: board.z,
    }),
    (board, index) => [
      LOCAL[index][0] > 0 ? board.u1 : board.u0,
      LOCAL[index][1] > 0 ? board.v0 : board.v1,
    ],
  );
}

export function shadowQuads(scene: Scene): Surface {
  return quads(
    scene.billboards,
    (board, index) => ({
      x: board.x + (LOCAL[index][0] * board.width) / 2,
      y: 0.02,
      z: board.z + (LOCAL[index][1] * Math.min(board.width, 1.2)) / 2,
    }),
    () => [0, 0],
  );
}
