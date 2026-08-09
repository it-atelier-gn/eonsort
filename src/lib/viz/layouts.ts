import type { EntryView, Provider } from "$lib/api";
import { isSuspect } from "$lib/dates";
import { hourOfDay, type TimeAxis } from "./timeaxis";

export const LANES: Provider[] = ["filename", "exif", "media", "filesystem"];
export const SPAN = 22;
export const DEPTH = 7;
export const LANE_GAP = 1.35;
export const STACK_STEP = 0.075;
export const SPREAD_HOURS = 48;

export type Mode = "field" | "helix" | "terrain";
export const MODES: Mode[] = ["field", "helix", "terrain"];

export const MODE_LABEL: Record<Mode, string> = {
  field: "Disagreement field",
  helix: "Time helix",
  terrain: "Memory terrain",
};

export const MODE_EXPLAINER: Record<Mode, { shows: string; axes: string; look: string }> = {
  field: {
    shows:
      "One dot per date any source reported, laid out left to right in time. A file with four sources appears four times.",
    axes: "Left to right: time. Depth: which source said so. Height: files stacked at the same moment.",
    look: "Long horizontal threads. A thread joins the readings of one file, so a long one means its sources disagree by years.",
  },
  helix: {
    shows:
      "The same points wound around a spiral, one full turn per year, so the same month always sits at the same angle.",
    axes: "Along the spiral: time. Around it: month of the year. Distance from the centre: hour of the day.",
    look: "Rings that line up. A dense band at one angle is a month you shot every year; a point far off the coil is a date out of place.",
  },
  terrain: {
    shows: "Every file dropped onto a landscape, so busy stretches pile up into hills.",
    axes: "Left to right: time. Front to back: hour of the day. Height: how many files share that moment.",
    look: "Peaks and flats. A peak is a day you took hundreds of pictures; a flat plain is a stretch with nothing in it.",
  },
};

export interface Instance {
  entry: number;
  lane: number;
  epoch: number;
  chosen: boolean;
  tone: number;
}

export function buildInstances(entries: EntryView[]): Instance[] {
  const out: Instance[] = [];

  entries.forEach((entry, index) => {
    const tone = toneOf(entry);
    let anchored = false;

    for (const candidate of entry.candidates) {
      const lane = LANES.indexOf(candidate.provider);
      if (lane < 0) continue;
      const isChosen = !anchored && candidate.taken_epoch === entry.taken_epoch;
      if (isChosen) anchored = true;
      out.push({ entry: index, lane, epoch: candidate.taken_epoch, chosen: isChosen, tone });
    }

    if (!anchored) {
      out.push({
        entry: index,
        lane: Math.max(0, LANES.indexOf(entry.provider)),
        epoch: entry.taken_epoch,
        chosen: true,
        tone,
      });
    }
  });

  return out;
}

function toneOf(entry: EntryView): number {
  if (entry.override_origin) return 3;
  if (isSuspect(entry)) return 2;
  return entry.confidence === "high" ? 0 : 1;
}

export function disagreementPairs(instances: Instance[]): number[] {
  const byEntry = new Map<number, number[]>();
  instances.forEach((instance, index) => {
    const bucket = byEntry.get(instance.entry);
    if (bucket) bucket.push(index);
    else byEntry.set(instance.entry, [index]);
  });

  const pairs: number[] = [];
  for (const bucket of byEntry.values()) {
    if (bucket.length < 2) continue;
    const anchor = bucket.find((i) => instances[i].chosen) ?? bucket[0];
    for (const index of bucket) {
      if (index === anchor) continue;
      const hours = Math.abs(instances[index].epoch - instances[anchor].epoch) / 3600;
      if (hours < SPREAD_HOURS) continue;
      pairs.push(anchor, index);
    }
  }
  return pairs;
}

export function field(instances: Instance[], axis: TimeAxis): Float32Array {
  const out = new Float32Array(instances.length * 3);
  const stacks = new Map<string, number>();

  instances.forEach((instance, index) => {
    const t = axis.map(instance.epoch);
    const x = (t - 0.5) * SPAN;
    const key = `${Math.round(t * 900)}:${instance.lane}`;
    const depth = stacks.get(key) ?? 0;
    stacks.set(key, depth + 1);

    out[index * 3] = x;
    out[index * 3 + 1] = (1.5 - instance.lane) * LANE_GAP;
    out[index * 3 + 2] = (depth - 6) * STACK_STEP;
  });

  return out;
}

export function helix(instances: Instance[], axis: TimeAxis): Float32Array {
  const out = new Float32Array(instances.length * 3);
  const turns = Math.max(1, axis.span / 365);

  instances.forEach((instance, index) => {
    const t = axis.map(instance.epoch);
    const angle = t * turns * Math.PI * 2;
    const lift = instance.tone === 2 ? 2.6 : 0;
    const radius = 3.4 + (hourOfDay(instance.epoch) / 24) * 1.8 + lift;

    out[index * 3] = Math.cos(angle) * radius;
    out[index * 3 + 1] = (t - 0.5) * 11 + (instance.tone === 2 ? 1.1 : 0);
    out[index * 3 + 2] = Math.sin(angle) * radius;
  });

  return out;
}

export function terrain(instances: Instance[], axis: TimeAxis): Float32Array {
  const out = new Float32Array(instances.length * 3);
  const columns = new Map<string, number>();

  instances.forEach((instance, index) => {
    const t = axis.map(instance.epoch);
    const hour = hourOfDay(instance.epoch);
    const key = `${Math.round(t * 700)}:${Math.round(hour)}`;
    const height = columns.get(key) ?? 0;
    columns.set(key, height + 1);

    out[index * 3] = (t - 0.5) * SPAN;
    out[index * 3 + 1] = height * 0.09 - 3;
    out[index * 3 + 2] = (hour / 24 - 0.5) * DEPTH;
  });

  return out;
}

export function layoutFor(mode: Mode, instances: Instance[], axis: TimeAxis): Float32Array {
  if (mode === "helix") return helix(instances, axis);
  if (mode === "terrain") return terrain(instances, axis);
  return field(instances, axis);
}

export function ease(t: number): number {
  const clamped = t < 0 ? 0 : t > 1 ? 1 : t;
  return clamped < 0.5
    ? 4 * clamped * clamped * clamped
    : 1 - Math.pow(-2 * clamped + 2, 3) / 2;
}
