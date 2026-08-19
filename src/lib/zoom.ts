export const MIN_ZOOM = 1;
export const MAX_ZOOM = 8;
export const ZOOM_STEP = 1.25;
export const WHEEL_RATE = 0.0022;

export interface Zoom {
  scale: number;
  x: number;
  y: number;
}

export interface Box {
  width: number;
  height: number;
}

export const RESTING: Zoom = { scale: 1, x: 0, y: 0 };

export function clampScale(scale: number): number {
  if (!Number.isFinite(scale)) return MIN_ZOOM;
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, scale));
}

export function held(zoom: Zoom, box: Box): Zoom {
  const scale = clampScale(zoom.scale);
  const room = {
    x: (Math.max(0, box.width) * (scale - 1)) / 2,
    y: (Math.max(0, box.height) * (scale - 1)) / 2,
  };
  return {
    scale,
    x: Math.min(room.x, Math.max(-room.x, Number.isFinite(zoom.x) ? zoom.x : 0)),
    y: Math.min(room.y, Math.max(-room.y, Number.isFinite(zoom.y) ? zoom.y : 0)),
  };
}

export function zoomedAt(zoom: Zoom, factor: number, at: { x: number; y: number }, box: Box): Zoom {
  const scale = clampScale(zoom.scale * (Number.isFinite(factor) && factor > 0 ? factor : 1));
  const grown = scale / zoom.scale;
  if (scale === MIN_ZOOM) return RESTING;

  return held(
    {
      scale,
      x: at.x - (at.x - zoom.x) * grown,
      y: at.y - (at.y - zoom.y) * grown,
    },
    box,
  );
}

export function wheelFactor(delta: number): number {
  if (!Number.isFinite(delta) || delta === 0) return 1;
  return Math.exp(-delta * WHEEL_RATE);
}

export function pannedBy(zoom: Zoom, dx: number, dy: number, box: Box): Zoom {
  if (zoom.scale <= MIN_ZOOM) return RESTING;
  return held({ scale: zoom.scale, x: zoom.x + dx, y: zoom.y + dy }, box);
}

export function steppedIn(zoom: Zoom, box: Box): Zoom {
  return zoomedAt(zoom, ZOOM_STEP, { x: 0, y: 0 }, box);
}

export function steppedOut(zoom: Zoom, box: Box): Zoom {
  return zoomedAt(zoom, 1 / ZOOM_STEP, { x: 0, y: 0 }, box);
}

export function isResting(zoom: Zoom): boolean {
  return zoom.scale <= MIN_ZOOM && zoom.x === 0 && zoom.y === 0;
}

export function transformOf(zoom: Zoom): string {
  if (isResting(zoom)) return "";
  return `translate(${zoom.x.toFixed(2)}px, ${zoom.y.toFixed(2)}px) scale(${zoom.scale.toFixed(3)})`;
}
