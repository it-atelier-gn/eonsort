import type { EntryView } from "$lib/api";
import type { FileColumnId } from "$lib/columns";

export interface Sorted<Id extends string = FileColumnId> {
  id: Id;
  down: boolean;
}

export const SORT_KEY = "eonsort.files.sort";

export function nextSort(
  held: Sorted<FileColumnId> | null,
  id: FileColumnId,
): Sorted<FileColumnId> | null {
  if (held?.id !== id) return { id, down: false };
  if (!held.down) return { id, down: true };
  return null;
}

export function cleanSort(
  value: unknown,
  columns: readonly FileColumnId[],
): Sorted | null {
  if (typeof value !== "object" || value === null) return null;
  const held = value as { id?: unknown; down?: unknown };
  if (typeof held.id !== "string" || !columns.includes(held.id as FileColumnId))
    return null;
  return { id: held.id as FileColumnId, down: held.down === true };
}

function keyOf(entry: EntryView, id: FileColumnId): string | number {
  switch (id) {
    case "name":
      return entry.name.toLowerCase();
    case "date":
      return entry.taken_epoch;
    case "from":
      return entry.provider;
    case "tags":
      return entry.tags.length === 0 ? "" : [...entry.tags].sort()[0];
    case "rated":
      return typeof entry.quality === "number" && Number.isFinite(entry.quality)
        ? entry.quality
        : Number.NEGATIVE_INFINITY;
    case "size":
      return entry.size;
    case "status":
      return entry.outcome ?? "";
  }
}

export function sortedEntries(
  entries: EntryView[],
  sorted: Sorted | null,
): EntryView[] {
  if (sorted === null) return entries;

  const ranked = [...entries];
  ranked.sort((a, b) => {
    const left = keyOf(a, sorted.id);
    const right = keyOf(b, sorted.id);
    const apart =
      typeof left === "number" && typeof right === "number"
        ? left - right
        : String(left).localeCompare(String(right));
    return (apart || a.name.localeCompare(b.name)) * (sorted.down ? -1 : 1);
  });
  return ranked;
}
