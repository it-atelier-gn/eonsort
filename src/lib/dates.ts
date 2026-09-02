import type { Confidence, EntryView } from "./api";

export const CONFIDENCE_TONE: Record<Confidence, string> = {
  high: "ok",
  medium: "warn",
  low: "danger",
};

export const CONFIDENCE_LABEL: Record<Confidence, string> = {
  high: "Sources agree",
  medium: "Single source",
  low: "Looks wrong",
};

export function toInputValue(epochSeconds: number): string {
  const date = new Date(epochSeconds * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    `${date.getUTCFullYear()}-${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())}` +
    `T${pad(date.getUTCHours())}:${pad(date.getUTCMinutes())}`
  );
}

const INPUT_PATTERN = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2})?$/;

export function fromInputValue(value: string): number | null {
  if (!INPUT_PATTERN.test(value)) return null;
  const withSeconds = value.length === 16 ? `${value}:00` : value;
  const parsed = Date.parse(`${withSeconds}Z`);
  return Number.isNaN(parsed) ? null : Math.floor(parsed / 1000);
}

export function shiftSeconds(entry: EntryView, targetInput: string): number | null {
  const target = fromInputValue(targetInput);
  if (target === null) return null;
  return target - entry.taken_epoch;
}

export function describeShift(seconds: number): string {
  const sign = seconds < 0 ? "-" : "+";
  const total = Math.abs(seconds);
  const days = Math.floor(total / 86400);
  const hours = Math.floor((total % 86400) / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const parts: string[] = [];
  if (days) parts.push(`${days}d`);
  if (hours) parts.push(`${hours}h`);
  if (minutes || parts.length === 0) parts.push(`${minutes}m`);
  return `${sign}${parts.join(" ")}`;
}

export function hardFlags(entry: EntryView) {
  return entry.flags.filter((flag) => flag.hard);
}

export function isSuspect(entry: EntryView): boolean {
  return entry.override_origin === null && hardFlags(entry).length > 0;
}
