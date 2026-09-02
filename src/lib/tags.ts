export interface Sighting {
  tags: string[];
  quality: number | null;
}

export interface TagCount {
  tag: string;
  count: number;
}

export const UNTAGGED = "__untagged__";
export const UNTAGGED_LABEL = "Untagged";
export const MIN_QUALITY = 4;
export const MAX_QUALITY = 7.5;

export function merged(
  held: Record<string, Sighting>,
  batch: Record<string, Sighting>,
): Record<string, Sighting> {
  return Object.keys(batch).length === 0 ? held : { ...held, ...batch };
}

export function tagCounts(entries: { tags: string[] }[]): TagCount[] {
  const tally = new Map<string, number>();
  let bare = 0;

  for (const entry of entries) {
    if (entry.tags.length === 0) {
      bare += 1;
      continue;
    }
    for (const tag of new Set(entry.tags)) {
      tally.set(tag, (tally.get(tag) ?? 0) + 1);
    }
  }

  const counted = [...tally]
    .map(([tag, count]) => ({ tag, count }))
    .sort((a, b) => b.count - a.count || a.tag.localeCompare(b.tag));

  return bare > 0 ? [...counted, { tag: UNTAGGED, count: bare }] : counted;
}

export function labelOf(tag: string): string {
  if (tag === UNTAGGED) return UNTAGGED_LABEL;
  const bare = tag.replace(/^(an?|the)\s+/i, "");
  return bare === "" ? tag : bare;
}

export function keepsTags(tags: string[], picked: string[] | null): boolean {
  if (picked === null) return true;
  if (tags.length === 0) return picked.includes(UNTAGGED);
  return tags.some((tag) => picked.includes(tag));
}

export function keepsQuality(
  quality: number | null | undefined,
  least: number,
): boolean {
  if (least <= MIN_QUALITY) return true;
  return (
    typeof quality === "number" && Number.isFinite(quality) && quality >= least
  );
}

export function withTag(
  picked: string[] | null,
  counts: TagCount[],
  tag: string,
): string[] {
  const all = counts.map((count) => count.tag);
  const held =
    picked === null ? all : picked.filter((one) => all.includes(one));
  return held.includes(tag)
    ? held.filter((one) => one !== tag)
    : [...held, tag];
}

export function allPicked(
  picked: string[] | null,
  counts: TagCount[],
): boolean {
  return picked === null || counts.every((count) => picked.includes(count.tag));
}

export function pickedLabel(
  picked: string[] | null,
  counts: TagCount[],
): string {
  if (allPicked(picked, counts)) return "All tags";
  if (picked === null || picked.length === 0) return "No tag";
  if (picked.length === 1) return labelOf(picked[0]);
  return `${picked.length} tags`;
}

export function matching(tags: TagCount[], needle: string): TagCount[] {
  const wanted = needle.trim().toLowerCase();
  if (wanted === "") return tags;
  return tags.filter((count) =>
    labelOf(count.tag).toLowerCase().includes(wanted),
  );
}
