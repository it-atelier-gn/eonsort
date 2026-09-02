import {
  createBuffer,
  createPickTarget,
  createProgram,
  decodeId,
  disposePickTarget,
  imageTexture,
  updateBuffer,
  type PickTarget,
} from "../gl";
import { clone, lerpOrbit, project, viewProjection, type Orbit } from "../camera";
import { ease } from "../layouts";
import {
  buildRings,
  clampFly,
  clampHeight,
  flownTheta,
  heightOf,
  heightRange,
  labelAt,
  nearestTiles,
  pitchAt,
  pitchShare,
  zoomedRadius,
  EMPTY_RINGS,
  LEVEL_PITCH,
  MAX_PITCH,
  MIN_PITCH,
  MONTH_COLOURS,
  TILE_HEIGHT,
  TILE_WIDTH,
  type Ring,
  type RingMode,
  type Rings,
  type Tile,
} from "./layout";
import {
  PICK_FRAGMENT,
  PICK_VERTEX,
  PICTURE_FRAGMENT,
  PICTURE_VERTEX,
  TILE_FRAGMENT,
  TILE_VERTEX,
} from "./shaders";
import type { EntryView } from "$lib/api";

export const MORPH_MS = 900;
export const GLIDE_MS = 700;
export const HOVER_MS = 90;
export const PICTURE_LIMIT = 72;
export const MIN_ORBIT = 3;
export const MAX_ORBIT = 400;
const FIELD_OF_VIEW = 0.9;

export interface Label {
  year: number;
  count: number;
  shown: number;
  x: number;
  y: number;
  depth: number;
}

export interface Look {
  height: number;
  lowest: number;
  highest: number;
  tilt: number;
  flying: boolean;
  speed: number;
}

export interface RingsCallbacks {
  onHover: (entry: number | null) => void;
  onLook: (look: Look) => void;
  onNear: (entries: number[]) => void;
  onLabels: (labels: Label[]) => void;
}

export function createRings(canvas: HTMLCanvasElement, callbacks: RingsCallbacks) {
  const gl = canvas.getContext("webgl2", { antialias: true, alpha: false });
  if (!gl) throw new Error("this system has no WebGL2");

  const tileProgram = createProgram(gl, TILE_VERTEX, TILE_FRAGMENT);
  const pickProgram = createProgram(gl, PICK_VERTEX, PICK_FRAGMENT);
  const pictureProgram = createProgram(gl, PICTURE_VERTEX, PICTURE_FRAGMENT);

  const corners = createBuffer(
    gl,
    new Float32Array([-0.5, -0.5, 0.5, -0.5, 0.5, 0.5, -0.5, -0.5, 0.5, 0.5, -0.5, 0.5]),
  );
  const centres = createBuffer(gl, new Float32Array(0));
  const outwards = createBuffer(gl, new Float32Array(0));
  const colours = createBuffer(gl, new Float32Array(0));
  const ids = createBuffer(gl, new Float32Array(0));

  const images = new Map<number, WebGLTexture>();

  let rings: Rings = EMPTY_RINGS;
  let mode: RingMode = "rings";
  let coiling = 0;
  let morphFrom = 0;
  let morphAt = 0;
  let orbit: Orbit = { theta: 0.6, phi: 1.32, radius: 18, target: [0, 0, 0] };
  let centreData = new Float32Array(0);
  let pick: PickTarget | null = null;
  let pointer: { x: number; y: number } | null = null;
  let aiming = false;
  let hovered: number | null = null;
  let near: number[] = [];
  let picture: Tile[] = [];
  let picked = 0;
  let frame = 0;
  let disposed = false;
  let glide: { from: Orbit; to: Orbit; at: number } | null = null;
  let flying = false;
  let speed = 1;
  let beat = 0;

  function setEntries(entries: EntryView[]) {
    rings = buildRings(entries);
    for (const texture of images.values()) gl!.deleteTexture(texture);
    images.clear();
    near = [];
    picture = [];
    hovered = null;

    const count = rings.tiles.length;
    centreData = new Float32Array(count * 3);
    const outward = new Float32Array(count * 2);
    const colour = new Float32Array(count * 3);
    const id = new Float32Array(count);

    rings.tiles.forEach((tile, index) => {
      outward[index * 2] = Math.sin(tile.angle);
      outward[index * 2 + 1] = Math.cos(tile.angle);
      const [r, g, b] = MONTH_COLOURS[tile.month];
      colour[index * 3] = r;
      colour[index * 3 + 1] = g;
      colour[index * 3 + 2] = b;
      id[index] = index;
    });

    writeCentres();
    updateBuffer(gl!, outwards, outward);
    updateBuffer(gl!, colours, colour);
    updateBuffer(gl!, ids, id);

    orbit = frameOrbit(rings, orbit);
    told();
  }

  function writeCentres() {
    rings.tiles.forEach((tile, index) => {
      centreData[index * 3] = Math.sin(tile.angle) * tile.radius;
      centreData[index * 3 + 1] = heightOf(tile, coiling);
      centreData[index * 3 + 2] = Math.cos(tile.angle) * tile.radius;
    });
    updateBuffer(gl!, centres, centreData);
    picked = 0;
  }

  function setMode(next: RingMode) {
    if (next === mode) return;
    mode = next;
    morphFrom = coiling;
    morphAt = performance.now();
    aiming = true;
  }

  function setImage(entry: number, image: TexImageSource) {
    const existing = images.get(entry);
    if (existing) gl!.deleteTexture(existing);
    images.set(entry, imageTexture(gl!, image));
  }

  function lookOf(): Look {
    const { min, max } = heightRange(rings);
    return {
      height: orbit.target[1],
      lowest: min,
      highest: max,
      tilt: pitchShare(orbit.phi),
      flying,
      speed,
    };
  }

  function told() {
    callbacks.onLook(lookOf());
  }

  function turn(dx: number, dy: number) {
    aiming = true;
    glide = null;
    orbit = {
      ...orbit,
      theta: orbit.theta - dx * 0.005,
      phi: Math.min(MAX_PITCH, Math.max(MIN_PITCH, orbit.phi - dy * 0.005)),
    };
    told();
  }

  function zoom(amount: number) {
    aiming = true;
    glide = null;
    orbit = {
      ...orbit,
      radius: zoomedRadius(orbit.radius, amount, rings.radius, MIN_ORBIT, MAX_ORBIT),
    };
  }

  function raise(amount: number) {
    setHeight(orbit.target[1] + amount * 0.01);
  }

  function setHeight(y: number) {
    aiming = true;
    glide = null;
    const height = clampHeight(rings, y);
    orbit = { ...orbit, target: [orbit.target[0], height, orbit.target[2]] };
    told();
  }

  function setTilt(share: number) {
    aiming = true;
    glide = null;
    orbit = { ...orbit, phi: pitchAt(share) };
    told();
  }

  function goTo(ring: Ring) {
    aiming = true;
    glide = { from: clone(orbit), to: levelAt(ring), at: performance.now() };
    told();
  }

  function levelAt(ring: Ring): Orbit {
    return {
      theta: orbit.theta,
      phi: LEVEL_PITCH,
      radius: Math.min(MAX_ORBIT, Math.max(MIN_ORBIT, ring.radius * 2.1 + 3)),
      target: [0, clampHeight(rings, ring.y), 0],
    };
  }

  function setFlying(on: boolean) {
    if (flying === on) return;
    flying = on;
    glide = null;
    told();
  }

  function setSpeed(next: number) {
    speed = clampFly(next);
    told();
  }

  function setPointer(next: { x: number; y: number } | null) {
    pointer = next;
    picked = 0;
    if (next === null && hovered !== null) {
      hovered = null;
      callbacks.onHover(null);
    }
  }

  function entryAt(x: number, y: number): number | null {
    if (!pick || rings.tiles.length === 0) return null;

    gl!.bindFramebuffer(gl!.FRAMEBUFFER, pick.framebuffer);
    gl!.viewport(0, 0, pick.width, pick.height);
    gl!.clearColor(0, 0, 0, 1);
    gl!.clear(gl!.COLOR_BUFFER_BIT | gl!.DEPTH_BUFFER_BIT);
    gl!.enable(gl!.DEPTH_TEST);

    gl!.useProgram(pickProgram);
    instance(gl!, pickProgram, "a_corner", corners, 2, 0);
    instance(gl!, pickProgram, "a_centre", centres, 3, 1);
    instance(gl!, pickProgram, "a_outward", outwards, 2, 1);
    instance(gl!, pickProgram, "a_id", ids, 1, 1);
    gl!.uniformMatrix4fv(
      gl!.getUniformLocation(pickProgram, "u_viewProjection"),
      false,
      matrix(),
    );
    gl!.uniform2f(gl!.getUniformLocation(pickProgram, "u_size"), TILE_WIDTH, TILE_HEIGHT);
    gl!.drawArraysInstanced(gl!.TRIANGLES, 0, 6, rings.tiles.length);

    const pixel = new Uint8Array(4);
    const px = Math.round(x * pick.width);
    const py = Math.round((1 - y) * pick.height);
    gl!.readPixels(px, py, 1, 1, gl!.RGBA, gl!.UNSIGNED_BYTE, pixel);
    gl!.bindFramebuffer(gl!.FRAMEBUFFER, null);

    const index = decodeId(pixel);
    return index >= 0 && index < rings.tiles.length ? rings.tiles[index].entry : null;
  }

  function resize() {
    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    const width = Math.max(1, Math.round(canvas.clientWidth * ratio));
    const height = Math.max(1, Math.round(canvas.clientHeight * ratio));
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }

    const wanted = { width: Math.max(1, width >> 1), height: Math.max(1, height >> 1) };
    if (!pick || pick.width !== wanted.width || pick.height !== wanted.height) {
      if (pick) disposePickTarget(gl!, pick);
      pick = createPickTarget(gl!, wanted.width, wanted.height);
    }
  }

  function matrix(): Float32Array {
    return viewProjection(orbit, canvas.width / Math.max(1, canvas.height));
  }

  function eyeOf(): [number, number, number] {
    const sinPhi = Math.sin(orbit.phi);
    return [
      orbit.target[0] + orbit.radius * sinPhi * Math.sin(orbit.theta),
      orbit.target[1] + orbit.radius * Math.cos(orbit.phi),
      orbit.target[2] + orbit.radius * sinPhi * Math.cos(orbit.theta),
    ];
  }

  function labelsFrom(mvp: Float32Array): Label[] {
    const out: Label[] = [];
    for (const ring of rings.rings) {
      const point = project(labelAt(ring, orbit.theta), mvp);
      if (!point.visible || point.depth <= 0.2) continue;
      if (point.x < -0.1 || point.x > 1.1 || point.y < -0.1 || point.y > 1.1) continue;
      out.push({
        year: ring.year,
        count: ring.count,
        shown: ring.shown,
        x: point.x,
        y: point.y,
        depth: point.depth,
      });
    }
    return out;
  }

  function render() {
    if (disposed) return;
    resize();

    const now = performance.now();
    const seconds = beat === 0 ? 0 : Math.min(0.1, (now - beat) / 1000);
    beat = now;

    if (glide) {
      const t = Math.min(1, (now - glide.at) / GLIDE_MS);
      orbit = lerpOrbit(glide.from, glide.to, ease(t));
      aiming = true;
      if (t >= 1) glide = null;
      told();
    } else if (flying && seconds > 0) {
      orbit = { ...orbit, theta: flownTheta(orbit.theta, seconds, speed, rings.radius) };
      aiming = true;
      told();
    }
    const wanted = mode === "spiral" ? 1 : 0;
    if (coiling !== wanted) {
      const t = Math.min(1, (now - morphAt) / MORPH_MS);
      coiling = morphFrom + (wanted - morphFrom) * ease(t);
      if (t >= 1) coiling = wanted;
      writeCentres();
    }

    const mvp = matrix();
    const eye = eyeOf();

    if (pointer && now - picked > HOVER_MS) {
      picked = now;
      const found = entryAt(pointer.x, pointer.y);
      if (found !== hovered) {
        hovered = found;
        callbacks.onHover(found);
      }
    }

    if (aiming || picture.length === 0) {
      aiming = false;
      picture = nearestTiles(rings, eye, PICTURE_LIMIT, coiling);
      const wantedNear = picture.map((tile) => tile.entry);
      if (wantedNear.length !== near.length || wantedNear.some((id, i) => id !== near[i])) {
        near = wantedNear;
        callbacks.onNear(near);
      }
    }

    callbacks.onLabels(labelsFrom(mvp));

    gl!.viewport(0, 0, canvas.width, canvas.height);
    gl!.clearColor(0.045, 0.055, 0.075, 1);
    gl!.clear(gl!.COLOR_BUFFER_BIT | gl!.DEPTH_BUFFER_BIT);
    gl!.enable(gl!.DEPTH_TEST);
    gl!.disable(gl!.BLEND);
    gl!.disable(gl!.CULL_FACE);

    if (rings.tiles.length > 0) {
      gl!.useProgram(tileProgram);
      instance(gl!, tileProgram, "a_corner", corners, 2, 0);
      instance(gl!, tileProgram, "a_centre", centres, 3, 1);
      instance(gl!, tileProgram, "a_outward", outwards, 2, 1);
      instance(gl!, tileProgram, "a_colour", colours, 3, 1);
      gl!.uniformMatrix4fv(
        gl!.getUniformLocation(tileProgram, "u_viewProjection"),
        false,
        mvp,
      );
      gl!.uniform2f(gl!.getUniformLocation(tileProgram, "u_size"), TILE_WIDTH, TILE_HEIGHT);
      gl!.uniform3fv(gl!.getUniformLocation(tileProgram, "u_eye"), eye);
      gl!.drawArraysInstanced(gl!.TRIANGLES, 0, 6, rings.tiles.length);

      drawPictures(mvp, eye);
    }

    frame = requestAnimationFrame(render);
  }

  function drawPictures(mvp: Float32Array, eye: [number, number, number]) {
    gl!.useProgram(pictureProgram);
    instance(gl!, pictureProgram, "a_corner", corners, 2, 0);
    gl!.uniformMatrix4fv(
      gl!.getUniformLocation(pictureProgram, "u_viewProjection"),
      false,
      mvp,
    );
    gl!.uniform3fv(gl!.getUniformLocation(pictureProgram, "u_eye"), eye);
    gl!.uniform2f(gl!.getUniformLocation(pictureProgram, "u_size"), TILE_WIDTH, TILE_HEIGHT);
    gl!.activeTexture(gl!.TEXTURE0);
    gl!.uniform1i(gl!.getUniformLocation(pictureProgram, "u_image"), 0);

    const centre = gl!.getUniformLocation(pictureProgram, "u_centre");
    const outward = gl!.getUniformLocation(pictureProgram, "u_outward");
    const colour = gl!.getUniformLocation(pictureProgram, "u_colour");
    const highlight = gl!.getUniformLocation(pictureProgram, "u_highlight");

    for (const tile of picture) {
      const texture = images.get(tile.entry);
      if (!texture) continue;

      const nx = Math.sin(tile.angle);
      const nz = Math.cos(tile.angle);
      gl!.bindTexture(gl!.TEXTURE_2D, texture);
      gl!.uniform3f(
        centre,
        nx * (tile.radius + 0.012),
        heightOf(tile, coiling),
        nz * (tile.radius + 0.012),
      );
      gl!.uniform2f(outward, nx, nz);
      const [r, g, b] = MONTH_COLOURS[tile.month];
      gl!.uniform3f(colour, r, g, b);
      gl!.uniform1f(highlight, tile.entry === hovered ? 1 : 0);
      gl!.drawArrays(gl!.TRIANGLES, 0, 6);
    }
  }

  frame = requestAnimationFrame(render);

  return {
    setEntries,
    setMode,
    setImage,
    setPointer,
    turn,
    zoom,
    raise,
    setHeight,
    setTilt,
    setFlying,
    setSpeed,
    look: lookOf,
    goTo,
    entryAt,
    rings() {
      return rings;
    },
    dispose() {
      disposed = true;
      cancelAnimationFrame(frame);
      for (const texture of images.values()) gl!.deleteTexture(texture);
      images.clear();
      if (pick) disposePickTarget(gl!, pick);
    },
  };
}

export function frameOrbit(rings: Rings, from: Orbit): Orbit {
  const half = Math.tan(FIELD_OF_VIEW / 2);
  const reach = Math.max(rings.radius * 1.2, rings.height / 2 + 2) / half;
  return {
    ...from,
    radius: Math.min(MAX_ORBIT, Math.max(MIN_ORBIT, reach)),
    target: [0, rings.height / 2, 0],
  };
}

function instance(
  gl: WebGL2RenderingContext,
  program: WebGLProgram,
  name: string,
  target: WebGLBuffer,
  size: number,
  divisor: number,
) {
  const location = gl.getAttribLocation(program, name);
  if (location < 0) return;
  gl.bindBuffer(gl.ARRAY_BUFFER, target);
  gl.enableVertexAttribArray(location);
  gl.vertexAttribPointer(location, size, gl.FLOAT, false, 0, 0);
  gl.vertexAttribDivisor(location, divisor);
}
