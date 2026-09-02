export interface Column<Id extends string = string> {
  id: Id;
  label: string;
  align: "left" | "right";
  stretch?: boolean;
}

export interface ColumnSet<Id extends string = string> {
  columns: Column<Id>[];
  orderKey: string;
  widthKey: string;
}

export type TreeColumnId = "name" | "files" | "size";
export type FileColumnId = "name" | "date" | "from" | "tags" | "rated" | "size" | "status";

export const TREE_COLUMNS: ColumnSet<TreeColumnId> = {
  columns: [
    { id: "name", label: "Folder", align: "left", stretch: true },
    { id: "files", label: "Files", align: "right" },
    { id: "size", label: "Size", align: "right" },
  ],
  orderKey: "eonsort.tree.columns",
  widthKey: "eonsort.tree.widths",
};

export const FILE_COLUMNS: ColumnSet<FileColumnId> = {
  columns: [
    { id: "name", label: "Name", align: "left", stretch: true },
    { id: "date", label: "Date", align: "left" },
    { id: "from", label: "From", align: "left" },
    { id: "tags", label: "Tags", align: "left" },
    { id: "rated", label: "Rated", align: "right" },
    { id: "size", label: "Size", align: "right" },
    { id: "status", label: "Status", align: "right" },
  ],
  orderKey: "eonsort.files.columns",
  widthKey: "eonsort.files.widths",
};

export const MIN_WIDTH = 48;
export const MAX_WIDTH = 640;

export type ColumnWidths<Id extends string = string> = Partial<Record<Id, number>>;

export function defaultOrder<Id extends string>(set: ColumnSet<Id>): Id[] {
  return set.columns.map((column) => column.id);
}

export function isColumnId<Id extends string>(set: ColumnSet<Id>, value: unknown): value is Id {
  return typeof value === "string" && set.columns.some((column) => column.id === value);
}

export function columnOf<Id extends string>(set: ColumnSet<Id>, id: Id): Column<Id> {
  return set.columns.find((column) => column.id === id) ?? set.columns[0];
}

export function cleanOrder<Id extends string>(set: ColumnSet<Id>, value: unknown): Id[] {
  if (!Array.isArray(value)) return defaultOrder(set);

  const seen: Id[] = [];
  for (const item of value) {
    if (isColumnId(set, item) && !seen.includes(item)) seen.push(item);
  }
  for (const id of defaultOrder(set)) {
    if (!seen.includes(id)) seen.push(id);
  }
  return seen;
}

export function moveColumn<Id extends string>(
  set: ColumnSet<Id>,
  order: Id[],
  from: Id,
  to: Id,
): Id[] {
  const clean = cleanOrder(set, order);
  const source = clean.indexOf(from);
  const target = clean.indexOf(to);
  if (source < 0 || target < 0 || source === target) return clean;

  const next = [...clean];
  next.splice(source, 1);
  next.splice(target, 0, from);
  return next;
}

export function widthOf<Id extends string>(set: ColumnSet<Id>, id: Id, rows: string[]): number {
  const column = columnOf(set, id);
  if (column.stretch) return 0;

  const longest = rows.reduce((most, text) => Math.max(most, text.length), 0);
  const characters = Math.max(longest, column.label.length);

  return Math.min(140, Math.max(52, characters * 8 + 18));
}

export function template<Id extends string>(
  set: ColumnSet<Id>,
  order: Id[],
  widths: Record<Id, number>,
): string {
  return cleanOrder(set, order)
    .map((id) => (widths[id] > 0 ? `${widths[id]}px` : "minmax(0, 1fr)"))
    .join(" ");
}

export function clampWidth(width: number): number {
  return Math.round(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, width)));
}

export function cleanWidths<Id extends string>(
  set: ColumnSet<Id>,
  value: unknown,
): ColumnWidths<Id> {
  if (typeof value !== "object" || value === null) return {};

  const kept: ColumnWidths<Id> = {};
  for (const [id, width] of Object.entries(value)) {
    if (isColumnId(set, id) && typeof width === "number" && Number.isFinite(width)) {
      kept[id] = clampWidth(width);
    }
  }
  return kept;
}

export function withWidth<Id extends string>(
  set: ColumnSet<Id>,
  widths: ColumnWidths<Id>,
  id: Id,
  width: number | null,
): ColumnWidths<Id> {
  const next = cleanWidths(set, widths);
  if (width === null) {
    delete next[id];
  } else {
    next[id] = clampWidth(width);
  }
  return next;
}

export const TREE_PANE_KEY = "eonsort.tree.pane";
export const TREE_PANE = 230;
export const MIN_PANE = 140;
export const MAX_PANE = 720;

export function clampPane(width: number): number {
  return Math.round(Math.min(MAX_PANE, Math.max(MIN_PANE, width)));
}

export function cleanPane(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return TREE_PANE;
  return clampPane(value);
}

export const PREVIEW_PANE_KEY = "eonsort.preview.pane";
export const PREVIEW_PANE = 340;
export const MIN_PREVIEW = 240;
export const MAX_PREVIEW = 900;

export function clampPreview(width: number): number {
  return Math.round(Math.min(MAX_PREVIEW, Math.max(MIN_PREVIEW, width)));
}

export function cleanPreview(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return PREVIEW_PANE;
  return clampPreview(value);
}

export const VISUAL_PANE_KEY = "eonsort.preview.visual";
export const VISUAL_PANE = 240;
export const MIN_VISUAL = 120;
export const MAX_VISUAL = 900;

export function clampVisual(height: number): number {
  return Math.round(Math.min(MAX_VISUAL, Math.max(MIN_VISUAL, height)));
}

export function cleanVisual(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return VISUAL_PANE;
  return clampVisual(value);
}
