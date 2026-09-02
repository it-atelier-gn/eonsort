export type Look = "details" | "thumbnails";

export const LOOK_KEY = "eonsort.files.look";
export const TILE_KEY = "eonsort.files.tile";

export interface TileSize {
  id: string;
  label: string;
  edge: number;
}

export const TILE_SIZES: TileSize[] = [
  { id: "small", label: "Small", edge: 96 },
  { id: "medium", label: "Medium", edge: 148 },
  { id: "large", label: "Large", edge: 224 },
];

export const TILE_GAP = 8;
export const THUMBNAIL_EDGE = 320;

export function cleanLook(value: unknown): Look {
  return value === "thumbnails" ? "thumbnails" : "details";
}

export function cleanTile(value: unknown): number {
  const found = TILE_SIZES.find((size) => size.edge === value);
  return found?.edge ?? TILE_SIZES[1].edge;
}

export function perRow(width: number, tile: number, gap = TILE_GAP): number {
  if (width <= 0 || tile <= 0) return 1;
  return Math.max(1, Math.floor((width + gap) / (tile + gap)));
}
