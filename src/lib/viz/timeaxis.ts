export const DAY = 86400;
export const GAP_DAYS = 90;
export const GAP_FRACTION = 0.45;

export interface TimeAxis {
  map(epoch: number): number;
  ticks: { at: number; label: string }[];
  breaks: { at: number; days: number }[];
  span: number;
}

export function buildTimeAxis(epochs: number[]): TimeAxis {
  const days = [...new Set(epochs.map((e) => Math.floor(e / DAY)))].sort((a, b) => a - b);

  if (days.length < 2) {
    const only = days[0] ?? 0;
    return {
      map: () => 0.5,
      ticks: days.length === 1 ? [{ at: 0.5, label: yearOf(only) }] : [],
      breaks: [],
      span: 0,
    };
  }

  const gaps = days.map((day, i) => (i + 1 < days.length ? days[i + 1] - day : 1));
  const content = gaps.reduce((sum, gap) => sum + (gap > GAP_DAYS ? 0 : gap), 0) || 1;
  const weights = gaps.map((gap) => (gap > GAP_DAYS ? Math.log2(1 + gap) : 0));
  const weightTotal = weights.reduce((sum, weight) => sum + weight, 0);
  const budget = content * GAP_FRACTION;

  const positions = new Float64Array(days.length);
  const widths = new Float64Array(days.length);
  let cursor = 0;

  for (let i = 0; i < days.length; i += 1) {
    positions[i] = cursor;
    widths[i] =
      gaps[i] > GAP_DAYS ? (budget * weights[i]) / (weightTotal || 1) : gaps[i];
    cursor += widths[i];
  }

  const total = cursor || 1;

  const breaks: { at: number; days: number }[] = [];
  for (let i = 0; i + 1 < days.length; i += 1) {
    const gap = days[i + 1] - days[i];
    if (gap > GAP_DAYS) {
      breaks.push({ at: (positions[i] + widths[i] / 2) / total, days: gap });
    }
  }

  const ticks: { at: number; label: string }[] = [];
  let lastYear = "";
  for (let i = 0; i < days.length; i += 1) {
    const year = yearOf(days[i]);
    if (year !== lastYear) {
      ticks.push({ at: positions[i] / total, label: year });
      lastYear = year;
    }
  }

  function map(epoch: number): number {
    const day = Math.floor(epoch / DAY);
    const index = locate(days, day);
    if (index < 0) return 0;
    const within = clamp01((epoch - days[index] * DAY) / DAY);
    return clamp01((positions[index] + Math.min(1, widths[index]) * within) / total);
  }

  return { map, ticks, breaks, span: days[days.length - 1] - days[0] };
}

function locate(days: number[], day: number): number {
  if (day < days[0]) return -1;
  let low = 0;
  let high = days.length - 1;
  while (low < high) {
    const mid = Math.ceil((low + high) / 2);
    if (days[mid] <= day) low = mid;
    else high = mid - 1;
  }
  return low;
}

function clamp01(value: number): number {
  return value < 0 ? 0 : value > 1 ? 1 : value;
}

function yearOf(day: number): string {
  return String(new Date(day * DAY * 1000).getUTCFullYear());
}

export function hourOfDay(epoch: number): number {
  const date = new Date(epoch * 1000);
  return date.getUTCHours() + date.getUTCMinutes() / 60;
}
