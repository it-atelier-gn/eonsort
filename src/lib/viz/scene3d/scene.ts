import { bindAttribute, createBuffer, createProgram, updateBuffer } from "../gl";
import { lookAt, multiply } from "../camera";
import { eyeTarget, look, step, type Intent, type Walker } from "../gallery/walk";
import {
  billboardQuads,
  shadowQuads,
  surfaceOf,
  EMPTY_SURFACE,
  type Scene,
  type Surface,
} from "./model";
import { projection } from "./titp";
import {
  BILLBOARD_FRAGMENT,
  BILLBOARD_VERTEX,
  SHADOW_FRAGMENT,
  SHADOW_VERTEX,
  SURFACE_FRAGMENT,
  SURFACE_VERTEX,
} from "./shaders";
import { swapsAxes } from "$lib/rotate";
import type { Transform } from "$lib/api";

export const MAX_EDGE = 4096;

const MATRICES: Record<Transform, [number, number, number, number, number, number]> = {
  none: [1, 0, 0, 1, 0, 0],
  flip_h: [-1, 0, 0, 1, 1, 0],
  rotate180: [-1, 0, 0, -1, 1, 1],
  flip_v: [1, 0, 0, -1, 0, 1],
  transpose: [0, 1, 1, 0, 0, 0],
  rotate90: [0, 1, -1, 0, 1, 0],
  transverse: [0, -1, -1, 0, 1, 1],
  rotate270: [0, -1, 1, 0, 0, 1],
};

export interface Photo {
  source: TexImageSource;
  width: number;
  height: number;
}

export function preparePhoto(
  image: HTMLImageElement | ImageBitmap,
  transform: Transform,
  maxEdge = MAX_EDGE,
): Photo {
  const width = image.width;
  const height = image.height;
  const turned = swapsAxes(transform);
  const outW = turned ? height : width;
  const outH = turned ? width : height;
  const scale = Math.min(1, maxEdge / Math.max(outW, outH));

  if (transform === "none" && scale === 1) {
    return { source: image, width, height };
  }

  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(outW * scale));
  canvas.height = Math.max(1, Math.round(outH * scale));

  const context = canvas.getContext("2d");
  if (!context) return { source: image, width, height };

  const [a, b, c, d, e, f] = MATRICES[transform];
  context.setTransform(
    a * scale,
    b * scale,
    c * scale,
    d * scale,
    e * outW * scale,
    f * outH * scale,
  );
  context.drawImage(image, 0, 0, width, height);

  return { source: canvas, width: canvas.width, height: canvas.height };
}

export function createScene(canvas: HTMLCanvasElement) {
  const context = canvas.getContext("webgl2", { antialias: true, alpha: false });
  if (!context) throw new Error("this system has no WebGL2");
  const gl = context;

  const program = createProgram(gl, SURFACE_VERTEX, SURFACE_FRAGMENT);
  const boardProgram = createProgram(gl, BILLBOARD_VERTEX, BILLBOARD_FRAGMENT);
  const shadowProgram = createProgram(gl, SHADOW_VERTEX, SHADOW_FRAGMENT);

  const empty = new Float32Array(0);
  const buffers = {
    position: createBuffer(gl, empty),
    normal: createBuffer(gl, empty),
    uv: createBuffer(gl, empty),
    shade: createBuffer(gl, empty),
  };
  const boards = {
    position: createBuffer(gl, empty),
    normal: createBuffer(gl, empty),
    uv: createBuffer(gl, empty),
  };
  const shadows = {
    position: createBuffer(gl, empty),
    normal: createBuffer(gl, empty),
  };
  const relief = {
    position: createBuffer(gl, empty),
    normal: createBuffer(gl, empty),
    uv: createBuffer(gl, empty),
    shade: createBuffer(gl, empty),
  };

  const behind = {
    position: createBuffer(gl, empty),
    normal: createBuffer(gl, empty),
    uv: createBuffer(gl, empty),
    shade: createBuffer(gl, empty),
  };

  const blank = solidTexture(gl, [14, 16, 22, 255]);
  let photo: WebGLTexture | null = null;
  let mended: WebGLTexture | null = null;
  let behindCount = 0;

  let scene: Scene | null = null;
  let surface: Surface = EMPTY_SURFACE;
  let boardCount = 0;
  let reliefCount = 0;
  let showBoards = false;
  let walker: Walker = { x: 0, z: 0, yaw: 0, pitch: 0, vx: 0, vz: 0 };
  let intent: Intent = { forward: 0, strafe: 0, running: false };
  let last = performance.now();
  let frame = 0;
  let disposed = false;

  function setScene(next: Scene) {
    scene = next;
    surface = surfaceOf(next);
    updateBuffer(gl, buffers.position, surface.position);
    updateBuffer(gl, buffers.normal, surface.normal);
    updateBuffer(gl, buffers.uv, surface.uv);
    updateBuffer(gl, buffers.shade, surface.shade);

    const quads = billboardQuads(next);
    const shade = shadowQuads(next);
    updateBuffer(gl, boards.position, quads.position);
    updateBuffer(gl, boards.normal, quads.normal);
    updateBuffer(gl, boards.uv, quads.uv);
    updateBuffer(gl, shadows.position, shade.position);
    updateBuffer(gl, shadows.normal, shade.normal);
    boardCount = quads.count;
  }

  function setBillboards(shown: boolean) {
    showBoards = shown;
  }

  function setRelief(surface: Surface | null) {
    if (!surface || surface.count === 0) {
      reliefCount = 0;
      return;
    }
    updateBuffer(gl, relief.position, surface.position);
    updateBuffer(gl, relief.normal, surface.normal);
    updateBuffer(gl, relief.uv, surface.uv);
    updateBuffer(gl, relief.shade, surface.shade);
    reliefCount = surface.count;
  }

  function setBehind(surface: Surface | null, healed: TexImageSource | null) {
    if (!surface || surface.count === 0) {
      behindCount = 0;
      return;
    }
    updateBuffer(gl, behind.position, surface.position);
    updateBuffer(gl, behind.normal, surface.normal);
    updateBuffer(gl, behind.uv, surface.uv);
    updateBuffer(gl, behind.shade, surface.shade);
    behindCount = surface.count;

    if (healed) {
      if (mended) gl.deleteTexture(mended);
      mended = imageTexture(gl, healed);
    }
  }

  function reset() {
    if (!scene) return;
    walker = { x: scene.spawn.x, z: scene.spawn.z, yaw: scene.spawn.yaw, pitch: scene.spawn.pitch, vx: 0, vz: 0 };
  }

  function setPhoto(image: TexImageSource) {
    if (photo) gl.deleteTexture(photo);
    photo = imageTexture(gl, image);
  }

  function setIntent(next: Intent) {
    intent = next;
  }

  function turn(dx: number, dy: number) {
    walker = look(walker, dx, dy);
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

  function render() {
    if (disposed) return;
    resize();

    const now = performance.now();
    const seconds = (now - last) / 1000;
    last = now;

    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.clearColor(0.03, 0.035, 0.05, 1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);

    if (!scene || surface.count === 0) {
      frame = requestAnimationFrame(render);
      return;
    }

    walker = step(walker, intent, scene.solids, seconds);

    const far = scene.depth * 4;
    const { eye, at } = eyeTarget(walker, scene.eyeHeight);
    const mvp = multiply(
      projection(scene, canvas.width / Math.max(1, canvas.height), 0.05, far),
      lookAt(eye, at),
    );

    gl.enable(gl.DEPTH_TEST);
    gl.depthMask(true);
    gl.disable(gl.BLEND);
    gl.disable(gl.CULL_FACE);

    gl.useProgram(program);
    bindAttribute(gl, program, "a_position", buffers.position, 3);
    bindAttribute(gl, program, "a_normal", buffers.normal, 3);
    bindAttribute(gl, program, "a_uv", buffers.uv, 2);
    bindAttribute(gl, program, "a_shade", buffers.shade, 1);

    gl.uniformMatrix4fv(gl.getUniformLocation(program, "u_viewProjection"), false, mvp);
    gl.uniform3fv(gl.getUniformLocation(program, "u_eye"), eye);
    gl.uniform1f(gl.getUniformLocation(program, "u_ready"), photo ? 1 : 0);
    gl.uniform1f(gl.getUniformLocation(program, "u_far"), far);

    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, photo ?? blank);
    gl.uniform1i(gl.getUniformLocation(program, "u_photo"), 0);

    gl.drawArrays(gl.TRIANGLES, 0, surface.count);

    if (behindCount > 0) {
      gl.bindTexture(gl.TEXTURE_2D, mended ?? photo ?? blank);
      bindAttribute(gl, program, "a_position", behind.position, 3);
      bindAttribute(gl, program, "a_normal", behind.normal, 3);
      bindAttribute(gl, program, "a_uv", behind.uv, 2);
      bindAttribute(gl, program, "a_shade", behind.shade, 1);
      gl.drawArrays(gl.TRIANGLES, 0, behindCount);
      gl.bindTexture(gl.TEXTURE_2D, photo ?? blank);
    }

    if (reliefCount > 0) {
      bindAttribute(gl, program, "a_position", relief.position, 3);
      bindAttribute(gl, program, "a_normal", relief.normal, 3);
      bindAttribute(gl, program, "a_uv", relief.uv, 2);
      bindAttribute(gl, program, "a_shade", relief.shade, 1);
      gl.drawArrays(gl.TRIANGLES, 0, reliefCount);
    }

    if (showBoards && boardCount > 0) {
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      gl.depthMask(false);

      gl.useProgram(shadowProgram);
      bindAttribute(gl, shadowProgram, "a_position", shadows.position, 3);
      bindAttribute(gl, shadowProgram, "a_normal", shadows.normal, 3);
      gl.uniformMatrix4fv(gl.getUniformLocation(shadowProgram, "u_viewProjection"), false, mvp);
      gl.drawArrays(gl.TRIANGLES, 0, boardCount);

      gl.useProgram(boardProgram);
      bindAttribute(gl, boardProgram, "a_position", boards.position, 3);
      bindAttribute(gl, boardProgram, "a_normal", boards.normal, 3);
      bindAttribute(gl, boardProgram, "a_uv", boards.uv, 2);
      gl.uniformMatrix4fv(gl.getUniformLocation(boardProgram, "u_viewProjection"), false, mvp);
      gl.uniform3fv(gl.getUniformLocation(boardProgram, "u_eye"), eye);
      gl.uniform1f(gl.getUniformLocation(boardProgram, "u_far"), far);
      gl.bindTexture(gl.TEXTURE_2D, photo ?? blank);
      gl.uniform1i(gl.getUniformLocation(boardProgram, "u_photo"), 0);
      gl.drawArrays(gl.TRIANGLES, 0, boardCount);

      gl.depthMask(true);
      gl.disable(gl.BLEND);
    }

    frame = requestAnimationFrame(render);
  }

  frame = requestAnimationFrame(render);

  return {
    setScene,
    setPhoto,
    setBillboards,
    setRelief,
    setBehind,
    setIntent,
    turn,
    reset,
    position() {
      return { x: walker.x, z: walker.z, yaw: walker.yaw, pitch: walker.pitch };
    },
    dispose() {
      disposed = true;
      cancelAnimationFrame(frame);
      if (photo) gl.deleteTexture(photo);
      if (mended) gl.deleteTexture(mended);
      gl.deleteTexture(blank);
      gl.deleteProgram(program);
      gl.deleteProgram(boardProgram);
      gl.deleteProgram(shadowProgram);
    },
  };
}

function solidTexture(gl: WebGL2RenderingContext, rgba: number[]): WebGLTexture {
  const texture = gl.createTexture();
  if (!texture) throw new Error("could not create a WebGL texture");
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(rgba));
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  return texture;
}

function imageTexture(gl: WebGL2RenderingContext, image: TexImageSource): WebGLTexture {
  const texture = gl.createTexture();
  if (!texture) throw new Error("could not create a WebGL texture");
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, image);
  gl.generateMipmap(gl.TEXTURE_2D);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  return texture;
}
