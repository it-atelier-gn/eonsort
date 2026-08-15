export const BILLBOARD_VERTEX = `#version 300 es
precision highp float;

in vec3 a_position;
in vec3 a_normal;
in vec2 a_uv;
in float a_shade;

uniform mat4 u_viewProjection;
uniform vec3 u_eye;

out vec2 v_uv;
out vec2 v_local;
out float v_depth;

void main() {
  gl_Position = u_viewProjection * vec4(a_position, 1.0);
  v_uv = a_uv;
  v_local = a_normal.xy;
  v_depth = length(a_position - u_eye);
}
`;

export const BILLBOARD_FRAGMENT = `#version 300 es
precision highp float;

in vec2 v_uv;
in vec2 v_local;
in float v_depth;

uniform sampler2D u_photo;
uniform float u_far;

out vec4 fragment;

void main() {
  float edge = length(v_local * vec2(1.04, 1.0));
  float alpha = 1.0 - smoothstep(0.78, 1.0, edge);
  if (alpha < 0.02) discard;

  vec3 colour = texture(u_photo, clamp(v_uv, 0.0, 1.0)).rgb;
  float haze = clamp(v_depth / max(u_far, 0.001), 0.0, 1.0);
  fragment = vec4(mix(colour, vec3(0.03, 0.035, 0.05), haze * haze * 0.18), alpha);
}
`;

export const SHADOW_VERTEX = `#version 300 es
precision highp float;

in vec3 a_position;
in vec3 a_normal;

uniform mat4 u_viewProjection;

out vec2 v_local;

void main() {
  gl_Position = u_viewProjection * vec4(a_position, 1.0);
  v_local = a_normal.xy;
}
`;

export const SHADOW_FRAGMENT = `#version 300 es
precision highp float;

in vec2 v_local;

out vec4 fragment;

void main() {
  float fade = 1.0 - smoothstep(0.0, 1.0, length(v_local));
  if (fade < 0.01) discard;
  fragment = vec4(0.0, 0.0, 0.0, fade * 0.4);
}
`;

export const SURFACE_VERTEX = `#version 300 es
precision highp float;

in vec3 a_position;
in vec3 a_normal;
in vec2 a_uv;
in float a_shade;

uniform mat4 u_viewProjection;
uniform vec3 u_eye;

out vec3 v_normal;
out vec2 v_uv;
out float v_shade;
out float v_depth;

void main() {
  gl_Position = u_viewProjection * vec4(a_position, 1.0);
  v_normal = a_normal;
  v_uv = a_uv;
  v_shade = a_shade;
  v_depth = length(a_position - u_eye);
}
`;

export const SURFACE_FRAGMENT = `#version 300 es
precision highp float;

in vec3 v_normal;
in vec2 v_uv;
in float v_shade;
in float v_depth;

uniform sampler2D u_photo;
uniform float u_ready;
uniform float u_far;

out vec4 fragment;

void main() {
  vec3 colour = texture(u_photo, clamp(v_uv, 0.0, 1.0)).rgb;
  if (u_ready < 0.5) {
    colour = vec3(0.06, 0.07, 0.09);
  }

  float key = 0.5 + 0.5 * dot(normalize(v_normal), normalize(vec3(-0.3, 0.9, 0.25)));
  colour *= v_shade * mix(0.9, 1.05, key);

  float haze = clamp(v_depth / max(u_far, 0.001), 0.0, 1.0);
  fragment = vec4(mix(colour, vec3(0.03, 0.035, 0.05), haze * haze * 0.18), 1.0);
}
`;
