import type { EntryView, Transform } from "./api";

const PARTS: Record<Transform, [number, boolean]> = {
  none: [0, false],
  rotate90: [1, false],
  rotate180: [2, false],
  rotate270: [3, false],
  flip_h: [0, true],
  transverse: [1, true],
  flip_v: [2, true],
  transpose: [3, true],
};

const BY_PARTS: Transform[][] = [
  ["none", "flip_h"],
  ["rotate90", "transverse"],
  ["rotate180", "flip_v"],
  ["rotate270", "transpose"],
];

export const TRANSFORM_CSS: Record<Transform, string> = {
  none: "none",
  rotate90: "rotate(90deg)",
  rotate180: "rotate(180deg)",
  rotate270: "rotate(-90deg)",
  flip_h: "scaleX(-1)",
  flip_v: "scaleY(-1)",
  transpose: "scaleX(-1) rotate(-90deg)",
  transverse: "scaleX(-1) rotate(90deg)",
};

const LABELS: Record<Transform, string> = {
  none: "left as it is",
  rotate90: "turned a quarter to the right",
  rotate180: "turned upside down",
  rotate270: "turned a quarter to the left",
  flip_h: "mirrored left to right",
  flip_v: "mirrored top to bottom",
  transpose: "mirrored along the main diagonal",
  transverse: "mirrored along the other diagonal",
};

export function swapsAxes(transform: Transform): boolean {
  return PARTS[transform][0] % 2 === 1;
}

export function turn(transform: Transform, quarterTurns: number): Transform {
  const [quarters, mirrored] = PARTS[transform];
  const shifted = (((quarters + quarterTurns) % 4) + 4) % 4;
  return BY_PARTS[shifted][mirrored ? 1 : 0];
}

export function describeTransform(transform: Transform): string {
  return LABELS[transform];
}

export function forOrientation(orientation: number): Transform {
  switch (orientation) {
    case 2:
      return "flip_h";
    case 3:
      return "rotate180";
    case 4:
      return "flip_v";
    case 5:
      return "transpose";
    case 6:
      return "rotate90";
    case 7:
      return "transverse";
    case 8:
      return "rotate270";
    default:
      return "none";
  }
}

export function canTurn(entry: EntryView | null): boolean {
  return entry !== null && entry.orientation > 0;
}

export function describeRotation(entry: EntryView): string {
  if (entry.reencode) return `${describeTransform(entry.rotate)} by re-encoding it`;
  if (entry.rotate_by_hand) return `you turned this, ${describeTransform(entry.rotate)}`;
  return `turned upright from the tag, ${describeTransform(entry.rotate)}`;
}
