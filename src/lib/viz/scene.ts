import {
  bindAttribute,
  createBuffer,
  createPickTarget,
  createProgram,
  decodeId,
  disposePickTarget,
  updateBuffer,
  type PickTarget,
} from "./gl";
import {
  buildInstances,
  disagreementPairs,
  ease,
  layoutFor,
  type Instance,
  type Mode,
} from "./layouts";
import {
  clone,
  lerpOrbit,
  project,
  viewProjection,
  MAX_PHI,
  MAX_RADIUS,
  MIN_PHI,
  MIN_RADIUS,
  PRESETS,
  type Orbit,
} from "./camera";
import {
  GRID_FRAGMENT,
  GRID_VERTEX,
  LINE_FRAGMENT,
  LINE_VERTEX,
  POINT_FRAGMENT,
  POINT_VERTEX,
} from "./shaders";
import { buildTimeAxis, type TimeAxis } from "./timeaxis";
import { flightOrbit, flightWaypoints, FLIGHT_BINS, FLIGHT_MS, type Waypoint } from "./flight";
import type { EntryView } from "$lib/api";

export const MORPH_MS = 900;
export const MAX_INSTANCES = 200_000;

export const DETAIL_FAR = 34;
export const DETAIL_NEAR = 13;

export const TILE_RADIUS = 13;
export const TILE_FULL_RADIUS = 9;
export const TILE_LIMIT = 48;
const TILE_WORLD_SIZE = 0.62;
export const MAX_TILE_SIZE = 0.26;
export const MIN_TILE_DEPTH = 1.15;
const FIELD_OF_VIEW = 0.9;

export interface Tile {
  entry: number;
  x: number;
  y: number;
  size: number;
  depth: number;
  fade: number;
}

export interface SceneCallbacks {
  onAxis: (axis: TimeAxis) => void;
  onTiles?: (tiles: Tile[]) => void;
  onFlight?: (flying: boolean) => void;
  onDetail?: (detail: number) => void;
}

export function detailFor(radius: number): number {
  if (radius <= DETAIL_NEAR) return 1;
  if (radius >= DETAIL_FAR) return 0;
  const t = (DETAIL_FAR - radius) / (DETAIL_FAR - DETAIL_NEAR);
  return t * t * (3 - 2 * t);
}

export function tileFade(radius: number): number {
  if (radius >= TILE_RADIUS) return 0;
  if (radius <= TILE_FULL_RADIUS) return 1;
  return (TILE_RADIUS - radius) / (TILE_RADIUS - TILE_FULL_RADIUS);
}

export function tilesFrom(
  positions: Float32Array,
  instances: Instance[],
  mvp: Float32Array,
  fade: number,
  limit: number,
): Tile[] {
  if (fade <= 0) return [];
  const focal = 1 / Math.tan(FIELD_OF_VIEW / 2);
  const found: Tile[] = [];

  for (let index = 0; index < instances.length; index += 1) {
    if (!instances[index].chosen) continue;
    const at = index * 3;
    const point = project(
      [positions[at], positions[at + 1], positions[at + 2]],
      mvp,
    );
    if (!point.visible || point.depth < MIN_TILE_DEPTH) continue;
    if (point.x < -0.2 || point.x > 1.2 || point.y < -0.2 || point.y > 1.2) continue;
    found.push({
      entry: instances[index].entry,
      x: point.x,
      y: point.y,
      depth: point.depth,
      size: Math.min(MAX_TILE_SIZE, (TILE_WORLD_SIZE * focal) / point.depth / 2),
      fade,
    });
  }

  found.sort((a, b) => a.depth - b.depth);
  return found.slice(0, limit);
}

export function createScene(canvas: HTMLCanvasElement, callbacks: SceneCallbacks) {
  const gl = canvas.getContext("webgl2", { antialias: true, alpha: false });
  if (!gl) throw new Error("this system has no WebGL2");

  const pointProgram = createProgram(gl, POINT_VERTEX, POINT_FRAGMENT);
  const lineProgram = createProgram(gl, LINE_VERTEX, LINE_FRAGMENT);
  const gridProgram = createProgram(gl, GRID_VERTEX, GRID_FRAGMENT);

  const grid = gridLines();
  const gridCount = grid.length / 3;
  const empty = new Float32Array(0);

  const buffers = {
    pointFrom: createBuffer(gl, empty),
    pointTo: createBuffer(gl, empty),
    tone: createBuffer(gl, empty),
    chosen: createBuffer(gl, empty),
    id: createBuffer(gl, empty),
    lineFrom: createBuffer(gl, empty),
    lineTo: createBuffer(gl, empty),
    lineTone: createBuffer(gl, empty),
    grid: createBuffer(gl, grid),
  };

  let instances: Instance[] = [];
  let axis: TimeAxis = buildTimeAxis([]);
  let pairs: number[] = [];
  let layoutFrom: Float32Array<ArrayBufferLike> = empty;
  let layoutTo: Float32Array<ArrayBufferLike> = empty;

  let mode: Mode = "field";
  let morph = 1;
  let morphStart = 0;
  let orbit = clone(PRESETS.field);
  let orbitFrom = clone(PRESETS.field);
  let orbitTo = clone(PRESETS.field);
  let selected = -1;
  let pick: PickTarget | null = null;
  let frame = 0;
  let disposed = false;
  let dirty = true;
  let hadTiles = false;
  let flying = false;
  let waypoints: Waypoint[] = [];
  let cruiseStart = 0;
  let cruising = false;
  let reportedDetail = -1;

  function blend(): Float32Array {
    if (morph >= 1 || layoutFrom.length !== layoutTo.length) return layoutTo;
    const out = new Float32Array(layoutTo.length);
    const t = ease(morph);
    for (let i = 0; i < out.length; i += 1) {
      out[i] = layoutFrom[i] + (layoutTo[i] - layoutFrom[i]) * t;
    }
    return out;
  }

  function uploadLayouts() {
    updateBuffer(gl!, buffers.pointFrom, layoutFrom);
    updateBuffer(gl!, buffers.pointTo, layoutTo);
    updateBuffer(gl!, buffers.lineFrom, expand(layoutFrom, pairs));
    updateBuffer(gl!, buffers.lineTo, expand(layoutTo, pairs));
  }

  function setEntries(entries: EntryView[]) {
    const all = buildInstances(entries);
    instances = all.length > MAX_INSTANCES ? thin(all, MAX_INSTANCES) : all;
    axis = buildTimeAxis(instances.map((i) => i.epoch));
    callbacks.onAxis(axis);
    pairs = disagreementPairs(instances);

    const tone = new Float32Array(instances.length);
    const chosen = new Float32Array(instances.length);
    const ids = new Float32Array(instances.length);
    instances.forEach((instance, index) => {
      tone[index] = instance.tone;
      chosen[index] = instance.chosen ? 1 : 0;
      ids[index] = index;
    });
    updateBuffer(gl!, buffers.tone, tone);
    updateBuffer(gl!, buffers.chosen, chosen);
    updateBuffer(gl!, buffers.id, ids);

    const lineTone = new Float32Array(pairs.length);
    pairs.forEach((instanceIndex, index) => {
      lineTone[index] = instances[instanceIndex].tone;
    });
    updateBuffer(gl!, buffers.lineTone, lineTone);

    layoutTo = layoutFor(mode, instances, axis);
    layoutFrom = layoutTo;
    uploadLayouts();
    waypoints = flightWaypoints(layoutTo, instances, FLIGHT_BINS);

    morph = 1;
    selected = -1;
    dirty = true;
  }

  function setMode(next: Mode) {
    if (next === mode) return;
    const current = blend();
    mode = next;

    if (instances.length === 0) {
      orbit = clone(PRESETS[mode]);
      dirty = true;
      return;
    }

    layoutFrom = current;
    layoutTo = layoutFor(mode, instances, axis);
    uploadLayouts();
    waypoints = flightWaypoints(layoutTo, instances, FLIGHT_BINS);

    orbitFrom = clone(orbit);
    orbitTo = clone(PRESETS[mode]);
    flying = true;
    morph = 0;
    morphStart = performance.now();
    dirty = true;
  }

  function setSelected(entryIndex: number | null) {
    selected =
      entryIndex === null
        ? -1
        : instances.findIndex((i) => i.entry === entryIndex && i.chosen);
    dirty = true;
  }

  function resize() {
    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    const width = Math.max(1, Math.round(canvas.clientWidth * ratio));
    const height = Math.max(1, Math.round(canvas.clientHeight * ratio));
    if (canvas.width === width && canvas.height === height) return;
    canvas.width = width;
    canvas.height = height;
    if (pick) {
      disposePickTarget(gl!, pick);
      pick = null;
    }
    dirty = true;
  }

  function matrix(): Float32Array {
    return viewProjection(orbit, canvas.width / Math.max(1, canvas.height));
  }

  function scaleForCount(): number {
    if (instances.length > 60_000) return 0.55;
    if (instances.length > 12_000) return 0.75;
    return 1;
  }

  function drawPoints(mvp: Float32Array, picking: number) {
    gl!.useProgram(pointProgram);
    bindAttribute(gl!, pointProgram, "a_from", buffers.pointFrom, 3);
    bindAttribute(gl!, pointProgram, "a_to", buffers.pointTo, 3);
    bindAttribute(gl!, pointProgram, "a_tone", buffers.tone, 1);
    bindAttribute(gl!, pointProgram, "a_chosen", buffers.chosen, 1);
    bindAttribute(gl!, pointProgram, "a_id", buffers.id, 1);

    gl!.uniformMatrix4fv(gl!.getUniformLocation(pointProgram, "u_viewProjection"), false, mvp);
    gl!.uniform1f(gl!.getUniformLocation(pointProgram, "u_morph"), ease(morph));
    gl!.uniform1f(gl!.getUniformLocation(pointProgram, "u_scale"), 46 * scaleForCount());
    gl!.uniform1f(gl!.getUniformLocation(pointProgram, "u_selected"), selected);
    gl!.uniform1f(gl!.getUniformLocation(pointProgram, "u_picking"), picking);
    gl!.uniform1f(gl!.getUniformLocation(pointProgram, "u_detail"), detailFor(orbit.radius));
    gl!.drawArrays(gl!.POINTS, 0, instances.length);
  }

  function pickAt(x: number, y: number): number | null {
    if (instances.length === 0) return null;
    resize();
    if (!pick) pick = createPickTarget(gl!, canvas.width, canvas.height);

    gl!.bindFramebuffer(gl!.FRAMEBUFFER, pick.framebuffer);
    gl!.viewport(0, 0, canvas.width, canvas.height);
    gl!.disable(gl!.BLEND);
    gl!.enable(gl!.DEPTH_TEST);
    gl!.clearColor(0, 0, 0, 1);
    gl!.clear(gl!.COLOR_BUFFER_BIT | gl!.DEPTH_BUFFER_BIT);

    drawPoints(matrix(), 1);

    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    const pixel = new Uint8Array(4);
    gl!.readPixels(
      Math.round(x * ratio),
      Math.round(canvas.height - y * ratio),
      1,
      1,
      gl!.RGBA,
      gl!.UNSIGNED_BYTE,
      pixel,
    );
    gl!.bindFramebuffer(gl!.FRAMEBUFFER, null);
    dirty = true;

    const id = decodeId(pixel);
    if (id < 0 || id >= instances.length) return null;
    return instances[id].entry;
  }

  function stopCruise() {
    if (!cruising) return;
    cruising = false;
    callbacks.onFlight?.(false);
  }

  function publishDetail() {
    if (!callbacks.onDetail) return;
    const detail = detailFor(orbit.radius);
    if (Math.abs(detail - reportedDetail) < 0.02 && detail !== 0 && detail !== 1) return;
    if (detail === reportedDetail) return;
    reportedDetail = detail;
    callbacks.onDetail(detail);
  }

  function publishTiles(mvp: Float32Array) {
    if (!callbacks.onTiles) return;
    const fade = tileFade(orbit.radius);
    if (fade <= 0) {
      if (hadTiles) {
        hadTiles = false;
        callbacks.onTiles([]);
      }
      return;
    }
    hadTiles = true;
    callbacks.onTiles(tilesFrom(blend(), instances, mvp, fade, TILE_LIMIT));
  }

  function render() {
    if (disposed) return;
    resize();

    if (morph < 1) {
      morph = Math.min(1, (performance.now() - morphStart) / MORPH_MS);
      if (flying) orbit = lerpOrbit(orbitFrom, orbitTo, ease(morph));
      dirty = true;
    }

    if (cruising) {
      const elapsed = (performance.now() - cruiseStart) / FLIGHT_MS;
      orbit = flightOrbit(waypoints, elapsed % 1);
      dirty = true;
    }

    if (dirty) {
      dirty = false;
      const mvp = matrix();

      gl!.bindFramebuffer(gl!.FRAMEBUFFER, null);
      gl!.viewport(0, 0, canvas.width, canvas.height);
      gl!.clearColor(0.031, 0.043, 0.063, 1);
      gl!.clear(gl!.COLOR_BUFFER_BIT | gl!.DEPTH_BUFFER_BIT);
      gl!.enable(gl!.BLEND);
      gl!.blendFunc(gl!.ONE, gl!.ONE_MINUS_SRC_ALPHA);
      gl!.disable(gl!.DEPTH_TEST);

      gl!.useProgram(gridProgram);
      bindAttribute(gl!, gridProgram, "a_position", buffers.grid, 3);
      gl!.uniformMatrix4fv(gl!.getUniformLocation(gridProgram, "u_viewProjection"), false, mvp);
      gl!.uniform4f(gl!.getUniformLocation(gridProgram, "u_colour"), 0.35, 0.45, 0.6, 0.1);
      gl!.drawArrays(gl!.LINES, 0, gridCount);

      if (pairs.length > 0) {
        gl!.useProgram(lineProgram);
        bindAttribute(gl!, lineProgram, "a_from", buffers.lineFrom, 3);
        bindAttribute(gl!, lineProgram, "a_to", buffers.lineTo, 3);
        bindAttribute(gl!, lineProgram, "a_tone", buffers.lineTone, 1);
        gl!.uniformMatrix4fv(gl!.getUniformLocation(lineProgram, "u_viewProjection"), false, mvp);
        gl!.uniform1f(gl!.getUniformLocation(lineProgram, "u_morph"), ease(morph));
        gl!.uniform1f(gl!.getUniformLocation(lineProgram, "u_detail"), detailFor(orbit.radius));
        gl!.drawArrays(gl!.LINES, 0, pairs.length);
      }

      drawPoints(mvp, 0);
      publishTiles(mvp);
      publishDetail();
    }

    frame = requestAnimationFrame(render);
  }

  frame = requestAnimationFrame(render);

  return {
    setEntries,
    setMode,
    setSelected,
    pickAt,
    matrix,
    orbitBy(dx: number, dy: number) {
      flying = false;
      stopCruise();
      orbit.theta -= dx * 0.006;
      orbit.phi = Math.min(MAX_PHI, Math.max(MIN_PHI, orbit.phi - dy * 0.006));
      dirty = true;
    },
    panBy(dx: number, dy: number) {
      flying = false;
      stopCruise();
      const scale = orbit.radius * 0.0016;
      orbit.target[0] -= Math.cos(orbit.theta) * dx * scale;
      orbit.target[2] += Math.sin(orbit.theta) * dx * scale;
      orbit.target[1] += dy * scale;
      dirty = true;
    },
    zoomBy(delta: number) {
      flying = false;
      stopCruise();
      orbit.radius = Math.min(
        MAX_RADIUS,
        Math.max(MIN_RADIUS, orbit.radius * (1 + delta * 0.0014)),
      );
      dirty = true;
    },
    reset() {
      flying = false;
      stopCruise();
      orbit = clone(PRESETS[mode]);
      dirty = true;
    },
    cruise(on: boolean) {
      if (on && waypoints.length > 1) {
        flying = false;
        cruising = true;
        cruiseStart = performance.now();
        callbacks.onFlight?.(true);
        dirty = true;
      } else {
        stopCruise();
      }
    },
    canCruise() {
      return waypoints.length > 1;
    },
    invalidate() {
      dirty = true;
    },
    dispose() {
      disposed = true;
      cancelAnimationFrame(frame);
      if (pick) disposePickTarget(gl!, pick);
    },
  };
}

function expand(positions: Float32Array, pairs: number[]): Float32Array {
  const out = new Float32Array(pairs.length * 3);
  for (let i = 0; i < pairs.length; i += 1) {
    const source = pairs[i] * 3;
    out[i * 3] = positions[source];
    out[i * 3 + 1] = positions[source + 1];
    out[i * 3 + 2] = positions[source + 2];
  }
  return out;
}

function thin(instances: Instance[], limit: number): Instance[] {
  const kept = instances.filter((i) => i.tone >= 2);
  const rest = instances.filter((i) => i.tone < 2);
  const room = Math.max(0, limit - kept.length);
  const step = Math.max(1, Math.ceil(rest.length / Math.max(1, room)));
  for (let i = 0; i < rest.length; i += step) kept.push(rest[i]);
  return kept;
}

function gridLines(): Float32Array {
  const out: number[] = [];
  const half = 12;
  for (let i = -half; i <= half; i += 2) {
    out.push(-half, -3.05, i, half, -3.05, i);
    out.push(i, -3.05, -half, i, -3.05, half);
  }
  return new Float32Array(out);
}
