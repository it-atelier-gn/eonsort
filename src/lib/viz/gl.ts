export function createProgram(gl: WebGL2RenderingContext, vertex: string, fragment: string) {
  const program = gl.createProgram();
  if (!program) throw new Error("could not create a WebGL program");

  const vs = compile(gl, gl.VERTEX_SHADER, vertex);
  const fs = compile(gl, gl.FRAGMENT_SHADER, fragment);
  gl.attachShader(program, vs);
  gl.attachShader(program, fs);
  gl.linkProgram(program);
  gl.deleteShader(vs);
  gl.deleteShader(fs);

  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    const log = gl.getProgramInfoLog(program);
    gl.deleteProgram(program);
    throw new Error(`could not link the WebGL program: ${log}`);
  }
  return program;
}

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type);
  if (!shader) throw new Error("could not create a WebGL shader");
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    const log = gl.getShaderInfoLog(shader);
    gl.deleteShader(shader);
    throw new Error(`could not compile a WebGL shader: ${log}`);
  }
  return shader;
}

export function createBuffer(
  gl: WebGL2RenderingContext,
  data: Float32Array<ArrayBufferLike>,
): WebGLBuffer {
  const buffer = gl.createBuffer();
  if (!buffer) throw new Error("could not create a WebGL buffer");
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW);
  return buffer;
}

export function updateBuffer(
  gl: WebGL2RenderingContext,
  buffer: WebGLBuffer,
  data: Float32Array<ArrayBufferLike>,
) {
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW);
}

export function bindAttribute(
  gl: WebGL2RenderingContext,
  program: WebGLProgram,
  name: string,
  buffer: WebGLBuffer,
  size: number,
) {
  const location = gl.getAttribLocation(program, name);
  if (location < 0) return;
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.enableVertexAttribArray(location);
  gl.vertexAttribPointer(location, size, gl.FLOAT, false, 0, 0);
}

export interface PickTarget {
  framebuffer: WebGLFramebuffer;
  texture: WebGLTexture;
  depth: WebGLRenderbuffer;
  width: number;
  height: number;
}

export function createPickTarget(
  gl: WebGL2RenderingContext,
  width: number,
  height: number,
): PickTarget {
  const framebuffer = gl.createFramebuffer();
  const texture = gl.createTexture();
  const depth = gl.createRenderbuffer();
  if (!framebuffer || !texture || !depth) throw new Error("could not create a WebGL pick target");

  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);

  gl.bindRenderbuffer(gl.RENDERBUFFER, depth);
  gl.renderbufferStorage(gl.RENDERBUFFER, gl.DEPTH_COMPONENT16, width, height);

  gl.bindFramebuffer(gl.FRAMEBUFFER, framebuffer);
  gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
  gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.DEPTH_ATTACHMENT, gl.RENDERBUFFER, depth);
  gl.bindFramebuffer(gl.FRAMEBUFFER, null);

  return { framebuffer, texture, depth, width, height };
}

export function disposePickTarget(gl: WebGL2RenderingContext, target: PickTarget) {
  gl.deleteFramebuffer(target.framebuffer);
  gl.deleteTexture(target.texture);
  gl.deleteRenderbuffer(target.depth);
}

export function decodeId(pixel: Uint8Array): number {
  const value = pixel[0] + pixel[1] * 256 + pixel[2] * 65536;
  return value - 1;
}
