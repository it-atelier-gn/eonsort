import type { EntryView } from "$lib/api";
import { baseName } from "$lib/api";

export interface CompareCard {
  source: string;
  name: string;
  folder: string;
  size: number | null;
  taken: string | null;
  provider: string | null;
  outcome: string | null;
  keeper: boolean;
}

export function folderOf(path: string): string {
  const at = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return at > 0 ? path.slice(0, at) : "";
}

export function cards(
  sources: string[],
  keeper: string | null,
  entries: Record<string, EntryView>,
): CompareCard[] {
  return sources.map((source) => {
    const entry = entries[source];
    return {
      source,
      name: baseName(source),
      folder: folderOf(source),
      size: entry?.size ?? null,
      taken: entry?.taken ?? null,
      provider: entry?.provider ?? null,
      outcome: entry?.outcome ?? null,
      keeper: source === keeper,
    };
  });
}

export function agree<T>(cards: CompareCard[], of: (card: CompareCard) => T): boolean {
  if (cards.length < 2) return true;
  const first = of(cards[0]);
  return cards.every((card) => of(card) === first);
}
