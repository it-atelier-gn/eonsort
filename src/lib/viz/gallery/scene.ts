import { bindAttribute, createBuffer, createProgram, imageTexture, updateBuffer } from "../gl";
import { lookAt, multiply, perspective } from "../camera";
import {
  buildGallery,
  buildLampQuads,
  buildPaneQuads,
  nearestLamps,
  roomAt,
  standing,
  CLERESTORY_BASE,
  CLERESTORY_HEIGHT,
  EMPTY_GALLERY,
  EYE_HEIGHT,
  type Frame,
  type Gallery,
  type Lamp,
  type Room,
} from "./index";
import { buildRoomMesh, buildShaftQuads } from "./geometry";
import { eyeTarget, look, step, type Intent, type Walker } from "./walk";
import {
  ART_FRAGMENT,
  ART_VERTEX,
  GLOW_FRAGMENT,
  GLOW_VERTEX,
  ROOM_FRAGMENT,
  ROOM_VERTEX,
} from "./shaders";
import type { EntryView } from "$lib/api";

export const DRAW_DISTANCE = 62;
export const NEAR_DISTANCE = 26;
export const MAX_LIGHTS = 8;

export interface GalleryCallbacks {
  onRoom: (room: Room | null) => void;
  onNear: (entries: number[]) => void;
  onLook: (entry: number | null) => void;
}

export interface Media {
  texture: WebGLTexture;
  video: HTMLVideoElement | null;
}

export function createGallery(canvas: HTMLCanvasElement, callbacks: GalleryCallbacks) {
  const gl = canvas.getContext("webgl2", { antialias: true, alpha: false });
  if (!gl) throw new Error("this system has no WebGL2");

  const roomProgram = createProgram(gl, ROOM_VERTEX, ROOM_FRAGMENT);
  const artProgram = createProgram(gl, ART_VERTEX, ART_FRAGMENT);
  const glowProgram = createProgram(gl, GLOW_VERTEX, GLOW_FRAGMENT);

  const empty = new Float32Array(0);
  const corners = createBuffer(
    gl,
    new Float32Array([-0.5, -0.5, 0.5, -0.5, 0.5, 0.5, -0.5, -0.5, 0.5, 0.5, -0.5, 0.5]),
  );
  const buffers = {
    position: createBuffer(gl, empty),
    normal: createBuffer(gl, empty),
    shade: createBuffer(gl, empty),
    pane: createBuffer(gl, empty),
    shaft: createBuffer(gl, empty),
    lamp: createBuffer(gl, empty),
  };

  const blank = solidTexture(gl, [12, 14, 20, 255]);
  const media = new Map<number, Media>();

  let gallery: Gallery = EMPTY_GALLERY;
  let roomCount = 0;
  let paneCount = 0;
  let shaftCount = 0;
  let lampCount = 0;
  let walker: Walker = { x: 0, z: 3, yaw: 0, pitch: 0, vx: 0, vz: 0 };
  let intent: Intent = { forward: 0, strafe: 0, running: false };
  let currentRoom: Room | null = null;
  let lookingAt: number | null = null;
  let nearby: number[] = [];
  let last = performance.now();
  let frame = 0;
  let disposed = false;

  function setEntries(entries: EntryView[]) {
    gallery = buildGallery(entries);

    const mesh = buildRoomMesh(gallery);
    updateBuffer(gl!, buffers.position, mesh.position);
    updateBuffer(gl!, buffers.normal, mesh.normal);
    updateBuffer(gl!, buffers.shade, mesh.shade);
    roomCount = mesh.count;

    const panes = buildPaneQuads(gallery);
    updateBuffer(gl!, buffers.pane, panes.position);
    paneCount = panes.count;

    const shafts = buildShaftQuads(gallery);
    updateBuffer(gl!, buffers.shaft, shafts.position);
    shaftCount = shafts.count;

    const fittings = buildLampQuads(gallery);
    updateBuffer(gl!, buffers.lamp, fittings.position);
    lampCount = fittings.count;

    for (const entry of media.values()) {
      gl!.deleteTexture(entry.texture);
      entry.video?.pause();
    }
    media.clear();

    walker = {
      x: gallery.start.x,
      z: gallery.start.z,
      yaw: gallery.start.yaw,
      pitch: 0,
      vx: 0,
      vz: 0,
    };
    currentRoom = null;
    nearby = [];
  }

  function setIntent(next: Intent) {
    intent = next;
  }

  function turn(dx: number, dy: number) {
    walker = look(walker, dx, dy);
  }

  function setImage(entry: number, image: TexImageSource) {
    const existing = media.get(entry);
    if (existing) gl!.deleteTexture(existing.texture);
    media.set(entry, { texture: imageTexture(gl!, image), video: null });
  }

  function setVideo(entry: number, video: HTMLVideoElement) {
    const existing = media.get(entry);
    if (existing) gl!.deleteTexture(existing.texture);
    media.set(entry, { texture: solidTexture(gl!, [10, 12, 16, 255]), video });
  }

  function resize() {
    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    const width = Math.max(1, Math.round(canvas.clientWidth * ratio));
    const height = Math.max(1, Math.round(canvas.clientHeight * ratio));
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
  }

  function matrix(): Float32Array {
    const { eye, at } = eyeTarget(walker, EYE_HEIGHT);
    return multiply(
      perspective(1.15, canvas.width / Math.max(1, canvas.height), 0.08, 260),
      lookAt(eye, at),
    );
  }

  function visibleFrames(): Frame[] {
    const out: Frame[] = [];
    for (const item of gallery.frames) {
      if (Math.hypot(item.x - walker.x, item.z - walker.z) > DRAW_DISTANCE) continue;
      out.push(item);
    }
    return out;
  }

  function aimedAt(frames: Frame[]): number | null {
    const forwardX = -Math.sin(walker.yaw);
    const forwardZ = -Math.cos(walker.yaw);
    let best: number | null = null;
    let bestScore = 0.986;

    for (const item of frames) {
      const dx = item.x - walker.x;
      const dz = item.z - walker.z;
      const distance = Math.hypot(dx, dz);
      if (distance > 9 || distance < 0.2) continue;
      const aim = (dx / distance) * forwardX + (dz / distance) * forwardZ;
      if (aim > bestScore) {
        bestScore = aim;
        best = item.entry;
      }
    }
    return best;
  }

  function render() {
    if (disposed) return;
    resize();

    const now = performance.now();
    const seconds = (now - last) / 1000;
    last = now;

    walker = step(walker, intent, gallery.solids, seconds);

    const room = roomAt(gallery, walker.x, walker.z);
    if (room?.key !== currentRoom?.key) {
      currentRoom = room;
      callbacks.onRoom(room);
    }

    const frames = visibleFrames();
    const near = frames
      .filter((item) => Math.hypot(item.x - walker.x, item.z - walker.z) < NEAR_DISTANCE)
      .map((item) => item.entry);
    if (near.length !== nearby.length || near.some((id, i) => id !== nearby[i])) {
      nearby = near;
      callbacks.onNear(near);
    }

    const aimed = aimedAt(frames);
    if (aimed !== lookingAt) {
      lookingAt = aimed;
      callbacks.onLook(aimed);
    }

    const mvp = matrix();
    const { eye } = eyeTarget(walker, EYE_HEIGHT);
    const lamps = nearestLamps(gallery.lamps, walker.x, walker.z, MAX_LIGHTS);

    gl!.viewport(0, 0, canvas.width, canvas.height);
    gl!.clearColor(0.045, 0.055, 0.075, 1);
    gl!.clear(gl!.COLOR_BUFFER_BIT | gl!.DEPTH_BUFFER_BIT);
    gl!.enable(gl!.DEPTH_TEST);
    gl!.depthMask(true);
    gl!.disable(gl!.BLEND);
    gl!.enable(gl!.CULL_FACE);
    gl!.cullFace(gl!.BACK);

    gl!.useProgram(roomProgram);
    bindAttribute(gl!, roomProgram, "a_position", buffers.position, 3);
    bindAttribute(gl!, roomProgram, "a_normal", buffers.normal, 3);
    bindAttribute(gl!, roomProgram, "a_shade", buffers.shade, 1);
    gl!.uniformMatrix4fv(gl!.getUniformLocation(roomProgram, "u_viewProjection"), false, mvp);
    gl!.uniform3fv(gl!.getUniformLocation(roomProgram, "u_eye"), eye);
    gl!.uniform1f(
      gl!.getUniformLocation(roomProgram, "u_clerestory"),
      CLERESTORY_BASE + CLERESTORY_HEIGHT / 2,
    );
    shine(roomProgram, lamps);
    gl!.drawArrays(gl!.TRIANGLES, 0, roomCount);

    gl!.disable(gl!.CULL_FACE);
    drawArtwork(mvp, eye, frames, lamps);

    gl!.enable(gl!.BLEND);
    gl!.blendFunc(gl!.SRC_ALPHA, gl!.ONE);
    gl!.depthMask(false);

    gl!.useProgram(glowProgram);
    gl!.uniformMatrix4fv(gl!.getUniformLocation(glowProgram, "u_viewProjection"), false, mvp);
    gl!.uniform1f(gl!.getUniformLocation(glowProgram, "u_top"), CLERESTORY_BASE + CLERESTORY_HEIGHT);

    bindAttribute(gl!, glowProgram, "a_position", buffers.pane, 3);
    gl!.uniform3f(gl!.getUniformLocation(glowProgram, "u_colour"), 1.0, 0.96, 0.86);
    gl!.uniform1f(gl!.getUniformLocation(glowProgram, "u_strength"), 0.95);
    gl!.drawArrays(gl!.TRIANGLES, 0, paneCount);

    bindAttribute(gl!, glowProgram, "a_position", buffers.shaft, 3);
    gl!.uniform3f(gl!.getUniformLocation(glowProgram, "u_colour"), 0.92, 0.88, 0.76);
    gl!.uniform1f(gl!.getUniformLocation(glowProgram, "u_strength"), 0.13);
    gl!.drawArrays(gl!.TRIANGLES, 0, shaftCount);

    bindAttribute(gl!, glowProgram, "a_position", buffers.lamp, 3);
    gl!.uniform3f(gl!.getUniformLocation(glowProgram, "u_colour"), 1.0, 0.9, 0.72);
    gl!.uniform1f(gl!.getUniformLocation(glowProgram, "u_strength"), 1.0);
    gl!.drawArrays(gl!.TRIANGLES, 0, lampCount);

    gl!.depthMask(true);
    gl!.disable(gl!.BLEND);

    frame = requestAnimationFrame(render);
  }

  function shine(program: WebGLProgram, lamps: Lamp[]) {
    const at = new Float32Array(MAX_LIGHTS * 3);
    const tone = new Float32Array(MAX_LIGHTS * 3);

    lamps.forEach((lamp, index) => {
      at[index * 3] = lamp.x;
      at[index * 3 + 1] = lamp.y;
      at[index * 3 + 2] = lamp.z;
      const warm = lamp.warm;
      tone[index * 3] = lamp.strength * (0.9 + 0.16 * warm);
      tone[index * 3 + 1] = lamp.strength * (0.86 + 0.08 * warm);
      tone[index * 3 + 2] = lamp.strength * (0.82 - 0.12 * warm);
    });

    gl!.uniform1i(gl!.getUniformLocation(program, "u_lightCount"), lamps.length);
    gl!.uniform3fv(gl!.getUniformLocation(program, "u_lightAt"), at);
    gl!.uniform3fv(gl!.getUniformLocation(program, "u_lightTone"), tone);
  }

  function drawArtwork(
    mvp: Float32Array,
    eye: [number, number, number],
    frames: Frame[],
    lamps: Lamp[],
  ) {
    gl!.useProgram(artProgram);
    bindAttribute(gl!, artProgram, "a_corner", corners, 2);
    gl!.uniformMatrix4fv(gl!.getUniformLocation(artProgram, "u_viewProjection"), false, mvp);
    gl!.uniform3fv(gl!.getUniformLocation(artProgram, "u_eye"), eye);
    gl!.activeTexture(gl!.TEXTURE0);
    gl!.uniform1i(gl!.getUniformLocation(artProgram, "u_image"), 0);
    shine(artProgram, lamps);

    const centre = gl!.getUniformLocation(artProgram, "u_centre");
    const size = gl!.getUniformLocation(artProgram, "u_size");
    const outward = gl!.getUniformLocation(artProgram, "u_outward");
    const ready = gl!.getUniformLocation(artProgram, "u_ready");
    const highlight = gl!.getUniformLocation(artProgram, "u_highlight");

    for (const item of frames) {
      const found = media.get(item.entry);
      if (found?.video && found.video.readyState >= 2) {
        gl!.bindTexture(gl!.TEXTURE_2D, found.texture);
        gl!.texImage2D(
          gl!.TEXTURE_2D,
          0,
          gl!.RGBA,
          gl!.RGBA,
          gl!.UNSIGNED_BYTE,
          found.video,
        );
      } else {
        gl!.bindTexture(gl!.TEXTURE_2D, found ? found.texture : blank);
      }

      gl!.uniform3f(centre, item.x, item.y, item.z);
      gl!.uniform2f(size, item.width, item.height);
      gl!.uniform2f(outward, item.nx, item.nz);
      gl!.uniform1f(ready, found ? 1 : 0);
      gl!.uniform1f(highlight, item.entry === lookingAt ? 1 : 0);
      gl!.drawArrays(gl!.TRIANGLES, 0, 6);
    }
  }

  frame = requestAnimationFrame(render);

  return {
    setEntries,
    setIntent,
    turn,
    setImage,
    setVideo,
    position() {
      return { x: walker.x, z: walker.z, yaw: walker.yaw };
    },
    rooms() {
      return gallery.rooms;
    },
    goTo(room: Room) {
      const spot = standing(room, gallery.solids);
      walker = { ...walker, x: spot.x, z: spot.z, yaw: spot.yaw, vx: 0, vz: 0 };
    },
    dispose() {
      disposed = true;
      cancelAnimationFrame(frame);
      for (const entry of media.values()) {
        gl!.deleteTexture(entry.texture);
        entry.video?.pause();
      }
      media.clear();
      gl!.deleteTexture(blank);
    },
  };
}

function solidTexture(gl: WebGL2RenderingContext, rgba: number[]): WebGLTexture {
  const texture = gl.createTexture();
  if (!texture) throw new Error("could not create a WebGL texture");
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texImage2D(
    gl.TEXTURE_2D,
    0,
    gl.RGBA,
    1,
    1,
    0,
    gl.RGBA,
    gl.UNSIGNED_BYTE,
    new Uint8Array(rgba),
  );
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  return texture;
}
