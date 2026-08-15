import type { EntryView } from "$lib/api";
import { MONTHS } from "./charts";

const HOUR = 3600;
const DAY = 86400;

export interface TimeRange {
  from: number;
  to: number;
}

export type GridLevel = "years" | "months" | "days";

interface Parts {
  y: number;
  m: number;
  d: number;
  h: number;
}

function parts(epoch: number): Parts {
  const date = new Date(epoch * 1000);
  return {
    y: date.getUTCFullYear(),
    m: date.getUTCMonth(),
    d: date.getUTCDate(),
    h: date.getUTCHours(),
  };
}

function utc(y: number, m = 0, d = 1, h = 0): number {
  return Date.UTC(y, m, d, h) / 1000;
}

export function planRange(entries: EntryView[]): TimeRange | null {
  if (entries.length === 0) return null;
  let first = Infinity;
  let last = -Infinity;
  for (const entry of entries) {
    first = Math.min(first, entry.taken_epoch);
    last = Math.max(last, entry.taken_epoch);
  }
  return { from: utc(parts(first).y), to: utc(parts(last).y + 1) };
}

export function inRange(entry: EntryView, range: TimeRange): boolean {
  return entry.taken_epoch >= range.from && entry.taken_epoch < range.to;
}

export function filterRange(entries: EntryView[], range: TimeRange | null): EntryView[] {
  if (range === null) return entries;
  return entries.filter((entry) => inRange(entry, range));
}

export function levelFor(range: TimeRange): GridLevel {
  const days = (range.to - range.from) / DAY;
  if (days > 366) return "years";
  if (days > 31) return "months";
  return "days";
}

const DAY_COLUMNS = Array.from({ length: 31 }, (_, day) => String(day + 1));
const HOUR_COLUMNS = Array.from({ length: 24 }, (_, hour) => String(hour).padStart(2, "0"));

export function columnsFor(level: GridLevel): string[] {
  if (level === "years") return MONTHS;
  if (level === "months") return DAY_COLUMNS;
  return HOUR_COLUMNS;
}

export const CELL_UNIT: Record<GridLevel, string> = {
  years: "months",
  months: "days",
  days: "hours",
};

export interface HeatCell {
  index: number;
  from: number;
  to: number;
  count: number;
  label: string;
}

export interface HeatRow {
  label: string;
  from: number;
  to: number;
  count: number;
  cells: (HeatCell | null)[];
}

export interface HeatGrid {
  level: GridLevel;
  columns: string[];
  rows: HeatRow[];
  max: number;
  total: number;
  emptyCells: number;
  cellCount: number;
}

function rowStarts(range: TimeRange, level: GridLevel): number[] {
  const start = parts(range.from);
  const starts: number[] = [];

  if (level === "years") {
    for (let y = start.y; utc(y) < range.to; y += 1) starts.push(utc(y));
    return starts;
  }

  if (level === "months") {
    let y = start.y;
    let m = start.m;
    while (utc(y, m) < range.to) {
      starts.push(utc(y, m));
      m += 1;
      if (m === 12) {
        m = 0;
        y += 1;
      }
    }
    return starts;
  }

  for (let day = utc(start.y, start.m, start.d); day < range.to; day += DAY) starts.push(day);
  return starts;
}

function rowEnd(rowStart: number, level: GridLevel): number {
  const row = parts(rowStart);
  if (level === "years") return utc(row.y + 1);
  if (level === "months") return utc(row.y, row.m + 1);
  return rowStart + DAY;
}

function cellBounds(rowStart: number, column: number, level: GridLevel): TimeRange | null {
  const row = parts(rowStart);
  if (level === "years") return { from: utc(row.y, column), to: utc(row.y, column + 1) };
  if (level === "months") {
    const from = utc(row.y, row.m, column + 1);
    if (parts(from).m !== row.m) return null;
    return { from, to: from + DAY };
  }
  const from = rowStart + column * HOUR;
  return { from, to: from + HOUR };
}

function cellLabel(rowStart: number, column: number, level: GridLevel): string {
  const row = parts(rowStart);
  if (level === "years") return `${MONTHS[column]} ${row.y}`;
  if (level === "months") return `${column + 1} ${MONTHS[row.m]} ${row.y}`;
  return `${HOUR_COLUMNS[column]}:00 on ${row.d} ${MONTHS[row.m]} ${row.y}`;
}

function rowLabel(rowStart: number, level: GridLevel, wide: boolean): string {
  const row = parts(rowStart);
  if (level === "years") return String(row.y);
  if (level === "months") return wide ? `${MONTHS[row.m]} ${row.y}` : MONTHS[row.m];
  return wide ? `${row.d} ${MONTHS[row.m]}` : String(row.d);
}

function cellStartOf(epoch: number, level: GridLevel): number {
  const at = parts(epoch);
  if (level === "years") return utc(at.y, at.m);
  if (level === "months") return utc(at.y, at.m, at.d);
  return utc(at.y, at.m, at.d, at.h);
}

export function heatGrid(entries: EntryView[], range: TimeRange): HeatGrid {
  const level = levelFor(range);
  const columns = columnsFor(level);
  const starts = rowStarts(range, level);
  const first = starts.length > 0 ? parts(starts[0]) : null;
  const last = starts.length > 0 ? parts(starts[starts.length - 1]) : null;
  const wide =
    first !== null &&
    last !== null &&
    (level === "months" ? first.y !== last.y : first.y !== last.y || first.m !== last.m);

  const byStart = new Map<number, HeatCell>();
  const rows: HeatRow[] = [];
  let index = 0;

  for (const start of starts) {
    const cells: (HeatCell | null)[] = [];
    for (let column = 0; column < columns.length; column += 1) {
      const bounds = cellBounds(start, column, level);
      if (bounds === null || bounds.to <= range.from || bounds.from >= range.to) {
        cells.push(null);
      } else {
        const cell: HeatCell = {
          index,
          from: bounds.from,
          to: bounds.to,
          count: 0,
          label: cellLabel(start, column, level),
        };
        byStart.set(bounds.from, cell);
        cells.push(cell);
      }
      index += 1;
    }
    rows.push({
      label: rowLabel(start, level, wide),
      from: Math.max(start, range.from),
      to: Math.min(rowEnd(start, level), range.to),
      count: 0,
      cells,
    });
  }

  let total = 0;
  for (const entry of entries) {
    const cell = byStart.get(cellStartOf(entry.taken_epoch, level));
    if (cell === undefined) continue;
    cell.count += 1;
    total += 1;
  }

  let max = 0;
  let emptyCells = 0;
  let cellCount = 0;
  for (const row of rows) {
    for (const cell of row.cells) {
      if (cell === null) continue;
      cellCount += 1;
      row.count += cell.count;
      max = Math.max(max, cell.count);
      if (cell.count === 0) emptyCells += 1;
    }
  }

  return { level, columns, rows, max, total, emptyCells, cellCount };
}

export function selectionRange(grid: HeatGrid, a: number, b: number): TimeRange | null {
  const low = Math.min(a, b);
  const high = Math.max(a, b);
  let from = Infinity;
  let to = -Infinity;

  for (const row of grid.rows) {
    for (const cell of row.cells) {
      if (cell === null || cell.index < low || cell.index > high) continue;
      from = Math.min(from, cell.from);
      to = Math.max(to, cell.to);
    }
  }

  return from === Infinity ? null : { from, to };
}

export function sameRange(a: TimeRange | null, b: TimeRange | null): boolean {
  if (a === null || b === null) return a === b;
  return a.from === b.from && a.to === b.to;
}

function isYearStart(epoch: number): boolean {
  const at = parts(epoch);
  return at.m === 0 && at.d === 1 && at.h === 0 && epoch === utc(at.y);
}

function isMonthStart(epoch: number): boolean {
  const at = parts(epoch);
  return epoch === utc(at.y, at.m);
}

function isDayStart(epoch: number): boolean {
  const at = parts(epoch);
  return epoch === utc(at.y, at.m, at.d);
}

function stamp(epoch: number): string {
  const at = parts(epoch);
  return `${at.d} ${MONTHS[at.m]} ${at.y} ${String(at.h).padStart(2, "0")}:00`;
}

export function rangeLabel(range: TimeRange): string {
  const from = parts(range.from);
  const last = parts(range.to - 1);

  if (isYearStart(range.from) && isYearStart(range.to)) {
    return from.y === last.y ? String(from.y) : `${from.y}–${last.y}`;
  }

  if (isMonthStart(range.from) && isMonthStart(range.to)) {
    if (from.y === last.y && from.m === last.m) return `${MONTHS[from.m]} ${from.y}`;
    return `${MONTHS[from.m]} ${from.y} – ${MONTHS[last.m]} ${last.y}`;
  }

  if (isDayStart(range.from) && isDayStart(range.to)) {
    if (from.y === last.y && from.m === last.m && from.d === last.d) {
      return `${from.d} ${MONTHS[from.m]} ${from.y}`;
    }
    return `${from.d} ${MONTHS[from.m]} ${from.y} – ${last.d} ${MONTHS[last.m]} ${last.y}`;
  }

  return `${stamp(range.from)} – ${stamp(range.to)}`;
}
