export const POINT_VERTEX = `#version 300 es
precision highp float;

in vec3 a_from;
in vec3 a_to;
in float a_tone;
in float a_chosen;
in float a_id;

uniform mat4 u_viewProjection;
uniform float u_morph;
uniform float u_scale;
uniform float u_selected;
uniform float u_picking;
uniform float u_detail;

out vec4 v_colour;
out float v_glow;
out float v_show;

vec3 toneColour(float tone) {
  if (tone < 0.5) return vec3(0.22, 0.77, 0.95);
  if (tone < 1.5) return vec3(0.95, 0.72, 0.25);
  if (tone < 2.5) return vec3(0.98, 0.32, 0.36);
  return vec3(0.55, 0.95, 0.62);
}

vec4 idColour(float id) {
  float value = id + 1.0;
  return vec4(
    mod(value, 256.0) / 255.0,
    mod(floor(value / 256.0), 256.0) / 255.0,
    mod(floor(value / 65536.0), 256.0) / 255.0,
    1.0
  );
}

void main() {
  vec3 position = mix(a_from, a_to, u_morph);
  vec4 clip = u_viewProjection * vec4(position, 1.0);
  gl_Position = clip;

  float distance = max(clip.w, 0.001);
  bool picked = abs(a_id - u_selected) < 0.5;

  float alarming = step(1.5, a_tone) * step(a_tone, 2.5);
  float always = max(max(a_chosen, alarming), picked ? 1.0 : 0.0);
  float show = mix(u_detail, 1.0, always);
  v_show = show;

  float emphasis = mix(1.0, 2.1, a_chosen) * mix(0.55, 1.0, show);
  if (picked) emphasis *= 2.4;
  gl_PointSize = clamp((u_scale * emphasis) / distance, 1.5, 42.0);

  if (u_picking > 0.5) {
    v_colour = idColour(a_id);
    v_glow = 0.0;
  } else {
    vec3 muted = vec3(0.34, 0.42, 0.55);
    vec3 tint = mix(muted, toneColour(a_tone), max(u_detail, alarming));
    float alpha = mix(0.42, 0.95, a_chosen) * show;
    v_colour = vec4(tint, picked ? 1.0 : alpha);
    v_glow = picked ? 1.0 : 0.0;
  }
}
`;

export const POINT_FRAGMENT = `#version 300 es
precision highp float;

in vec4 v_colour;
in float v_glow;
in float v_show;

uniform float u_picking;

out vec4 fragment;

void main() {
  vec2 offset = gl_PointCoord - vec2(0.5);
  float radius = length(offset) * 2.0;

  if (v_show < 0.04) discard;

  if (u_picking > 0.5) {
    if (radius > 1.0) discard;
    fragment = v_colour;
    return;
  }

  float core = 1.0 - smoothstep(0.0, 0.75, radius);
  float halo = (1.0 - smoothstep(0.0, 1.0, radius)) * 0.35;
  float intensity = core + halo + v_glow * (1.0 - smoothstep(0.0, 1.0, radius)) * 0.8;
  if (intensity <= 0.002) discard;

  fragment = vec4(v_colour.rgb * intensity, v_colour.a * intensity);
}
`;

export const LINE_VERTEX = `#version 300 es
precision highp float;

in vec3 a_from;
in vec3 a_to;
in float a_tone;

uniform mat4 u_viewProjection;
uniform float u_morph;
uniform float u_detail;

out vec4 v_colour;

vec3 toneColour(float tone) {
  if (tone < 0.5) return vec3(0.22, 0.77, 0.95);
  if (tone < 1.5) return vec3(0.95, 0.72, 0.25);
  if (tone < 2.5) return vec3(0.98, 0.32, 0.36);
  return vec3(0.55, 0.95, 0.62);
}

float toneAlpha(float tone) {
  return tone > 1.5 && tone < 2.5 ? 0.55 : 0.13;
}

void main() {
  gl_Position = u_viewProjection * vec4(mix(a_from, a_to, u_morph), 1.0);

  float alarming = step(1.5, a_tone) * step(a_tone, 2.5);
  float visible = max(u_detail, alarming * 0.9);
  v_colour = vec4(toneColour(a_tone), toneAlpha(a_tone) * visible);
}
`;

export const LINE_FRAGMENT = `#version 300 es
precision highp float;

in vec4 v_colour;
out vec4 fragment;

void main() {
  fragment = vec4(v_colour.rgb * v_colour.a, v_colour.a);
}
`;

export const GRID_VERTEX = `#version 300 es
precision highp float;

in vec3 a_position;
uniform mat4 u_viewProjection;

void main() {
  gl_Position = u_viewProjection * vec4(a_position, 1.0);
}
`;

export const GRID_FRAGMENT = `#version 300 es
precision highp float;

uniform vec4 u_colour;
out vec4 fragment;

void main() {
  fragment = vec4(u_colour.rgb * u_colour.a, u_colour.a);
}
`;
