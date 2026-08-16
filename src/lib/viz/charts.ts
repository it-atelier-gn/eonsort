import type { EntryView, Provider } from "$lib/api";
import { isSuspect } from "$lib/dates";

export const PROVIDER_ORDER: Provider[] = ["filename", "exif", "media", "filesystem"];

export const PROVIDER_LABEL: Record<Provider, string> = {
  filename: "File name",
  exif: "EXIF",
  media: "Media",
  filesystem: "File system",
};

export const PROVIDER_COLOUR: Record<Provider, string> = {
  filename: "#3987e5",
  exif: "#d95926",
  media: "#199e70",
  filesystem: "#d55181",
};

export const MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

export const HEAT_STEPS = [
  "#0d366b",
  "#184f95",
  "#256abf",
  "#3987e5",
  "#5598e7",
  "#86b6ef",
  "#cde2fb",
];

export function heatStep(count: number, max: number): string | null {
  if (count <= 0 || max <= 0) return null;
  const share = Math.log1p(count) / Math.log1p(max);
  const index = Math.min(HEAT_STEPS.length - 1, Math.floor(share * HEAT_STEPS.length));
  return HEAT_STEPS[index];
}

export interface HourBar {
  hour: number;
  files: number;
}

export function hourHistogram(entries: EntryView[]): HourBar[] {
  const counts = new Array(24).fill(0);
  for (const entry of entries) {
    counts[new Date(entry.taken_epoch * 1000).getUTCHours()] += 1;
  }
  return counts.map((files, hour) => ({ hour, files }));
}

export function midnightShare(bars: HourBar[]): number {
  const total = bars.reduce((sum, bar) => sum + bar.files, 0);
  return total === 0 ? 0 : bars[0].files / total;
}

export interface ProviderBar {
  provider: Provider;
  files: number;
}

export function providerCounts(entries: EntryView[]): ProviderBar[] {
  const counts = new Map<Provider, number>();
  for (const entry of entries) {
    const provider = entry.provider as Provider;
    counts.set(provider, (counts.get(provider) ?? 0) + 1);
  }
  return PROVIDER_ORDER.filter((provider) => counts.has(provider)).map((provider) => ({
    provider,
    files: counts.get(provider) ?? 0,
  }));
}

export type Reading = "agree" | "single" | "wrong" | "decided";

export const READINGS: Reading[] = ["agree", "single", "wrong", "decided"];

export const READING_LABEL: Record<Reading, string> = {
  agree: "sources agree",
  single: "single source",
  wrong: "looks wrong",
  decided: "you decided",
};

export const READING_COLOUR: Record<Reading, string> = {
  agree: "#38c4f2",
  single: "#f2b840",
  wrong: "#fa525c",
  decided: "#8cf29e",
};

export const READING_NOTE: Record<Reading, string> = {
  agree: "Two or more sources reported the same date, so it is almost certainly right.",
  single: "Only one source had a date. Usually fine, but nothing corroborates it.",
  wrong: "Something about the date is impossible or out of step with its neighbours.",
  decided: "You set this date by hand, so eonsort stopped second-guessing it.",
};

export function readingOf(entry: EntryView): Reading {
  if (entry.override_origin) return "decided";
  if (isSuspect(entry)) return "wrong";
  return entry.confidence === "high" ? "agree" : "single";
}

export interface ReadingBar {
  reading: Reading;
  files: number;
}

export function readingCounts(entries: EntryView[]): ReadingBar[] {
  const counts: Record<Reading, number> = { agree: 0, single: 0, wrong: 0, decided: 0 };
  for (const entry of entries) counts[readingOf(entry)] += 1;
  return READINGS.map((reading) => ({ reading, files: counts[reading] }));
}

export interface FolderBar {
  folder: string;
  files: number;
  bytes: number;
}

export function topFolders(entries: EntryView[], limit: number): FolderBar[] {
  const counts = new Map<string, FolderBar>();
  for (const entry of entries) {
    const folder = entry.folder || "the destination root";
    const bar = counts.get(folder);
    if (bar) {
      bar.files += 1;
      bar.bytes += entry.size;
    } else {
      counts.set(folder, { folder, files: 1, bytes: entry.size });
    }
  }
  return [...counts.values()].sort((a, b) => b.files - a.files).slice(0, limit);
}

export interface Span {
  first: number | null;
  last: number | null;
  years: number;
}

export function span(entries: EntryView[]): Span {
  if (entries.length === 0) return { first: null, last: null, years: 0 };
  let first = Infinity;
  let last = -Infinity;
  for (const entry of entries) {
    first = Math.min(first, entry.taken_epoch);
    last = Math.max(last, entry.taken_epoch);
  }
  return {
    first,
    last,
    years: Math.max(1, Math.round((last - first) / (365.25 * 86400))),
  };
}

export function formatYear(epoch: number | null): string {
  if (epoch === null) return "—";
  return String(new Date(epoch * 1000).getUTCFullYear());
}

export function niceTicks(max: number, count: number): number[] {
  if (max <= 0) return [0];
  const raw = max / count;
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 5, 10].map((m) => m * magnitude).find((s) => s >= raw) ?? magnitude * 10;
  const ticks: number[] = [0];
  while (ticks[ticks.length - 1] < max) ticks.push(ticks[ticks.length - 1] + step);
  return ticks;
}
