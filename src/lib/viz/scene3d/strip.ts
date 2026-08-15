export interface StripWindow {
  from: number;
  to: number;
}

export const AHEAD = 4;
export const SPARE = 8;

export function stripWindow(
  count: number,
  scrollLeft: number,
  clientWidth: number,
  itemWidth: number,
): StripWindow {
  if (!Number.isFinite(count) || count <= 0 || itemWidth <= 0) return { from: 0, to: 0 };

  const left = Number.isFinite(scrollLeft) ? Math.max(0, scrollLeft) : 0;
  const width = Number.isFinite(clientWidth) ? Math.max(0, clientWidth) : 0;

  const from = Math.min(count, Math.max(0, Math.floor(left / itemWidth) - AHEAD));
  const to = Math.min(count, from + Math.ceil(width / itemWidth) + SPARE);
  return { from, to };
}
