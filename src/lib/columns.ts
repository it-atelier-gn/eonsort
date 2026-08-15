export type ColumnId = "name" | "files" | "size";

export interface Column {
  id: ColumnId;
  label: string;
  align: "left" | "right";
}

export const COLUMNS: Record<ColumnId, Column> = {
  name: { id: "name", label: "Folder", align: "left" },
  files: { id: "files", label: "Files", align: "right" },
  size: { id: "size", label: "Size", align: "right" },
};

export const DEFAULT_ORDER: ColumnId[] = ["name", "files", "size"];
export const ORDER_KEY = "eonsort.tree.columns";

export function isColumnId(value: unknown): value is ColumnId {
  return value === "name" || value === "files" || value === "size";
}

export function cleanOrder(value: unknown): ColumnId[] {
  if (!Array.isArray(value)) return [...DEFAULT_ORDER];

  const seen: ColumnId[] = [];
  for (const item of value) {
    if (isColumnId(item) && !seen.includes(item)) seen.push(item);
  }
  for (const id of DEFAULT_ORDER) {
    if (!seen.includes(id)) seen.push(id);
  }
  return seen;
}

export function moveColumn(order: ColumnId[], from: ColumnId, to: ColumnId): ColumnId[] {
  const clean = cleanOrder(order);
  const source = clean.indexOf(from);
  const target = clean.indexOf(to);
  if (source < 0 || target < 0 || source === target) return clean;

  const next = [...clean];
  next.splice(source, 1);
  next.splice(target, 0, from);
  return next;
}

export function widthOf(id: ColumnId, rows: string[]): number {
  const longest = rows.reduce((most, text) => Math.max(most, text.length), 0);
  const header = COLUMNS[id].label.length;
  const characters = Math.max(longest, header);

  if (id === "name") return 0;
  return Math.min(140, Math.max(52, characters * 8 + 18));
}

export function template(order: ColumnId[], widths: Record<ColumnId, number>): string {
  return cleanOrder(order)
    .map((id) => (widths[id] > 0 ? `${widths[id]}px` : "minmax(0, 1fr)"))
    .join(" ");
}
