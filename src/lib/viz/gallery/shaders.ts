
const LIGHTS = `
const int MAX_LIGHTS = 8;
uniform int u_lightCount;
uniform vec3 u_lightAt[MAX_LIGHTS];
uniform vec3 u_lightTone[MAX_LIGHTS];

vec3 lampLight(vec3 position, vec3 normal) {
  vec3 sum = vec3(0.0);
  for (int i = 0; i < MAX_LIGHTS; i++) {
    if (i >= u_lightCount) break;
    vec3 toLamp = u_lightAt[i] - position;
    float distance = length(toLamp);
    float fall = 1.0 / (1.0 + 0.16 * distance + 0.055 * distance * distance);
    float lambert = max(0.0, dot(normalize(normal), toLamp / max(distance, 0.001)));
    sum += u_lightTone[i] * fall * (0.2 + 0.8 * lambert);
  }
  return min(sum, vec3(1.15));
}
`;

const FOG = `
vec3 withFog(vec3 colour, float depth) {
  float amount = 1.0 - exp(-depth * 0.014);
  return mix(colour, vec3(0.045, 0.055, 0.075), clamp(amount, 0.0, 0.72));
}
`;

export const ROOM_VERTEX = `#version 300 es
precision highp float;

in vec3 a_position;
in vec3 a_normal;
in float a_shade;

uniform mat4 u_viewProjection;
uniform vec3 u_eye;

out vec3 v_position;
out vec3 v_normal;
out float v_shade;
out float v_depth;

void main() {
  gl_Position = u_viewProjection * vec4(a_position, 1.0);
  v_position = a_position;
  v_normal = a_normal;
  v_shade = a_shade;
  v_depth = length(a_position - u_eye);
}
`;

export const ROOM_FRAGMENT = `#version 300 es
precision highp float;

in vec3 v_position;
in vec3 v_normal;
in float v_shade;
in float v_depth;

uniform vec3 u_eye;
uniform float u_clerestory;

out vec4 fragment;
${LIGHTS}
${FOG}

void main() {
  vec3 normal = normalize(v_normal);

  vec3 sun = normalize(vec3(-0.35, -1.0, 0.18));
  float key = max(0.0, dot(normal, -sun));

  float sky = 0.5 + 0.5 * normal.y;
  vec3 ambient = mix(vec3(0.10, 0.11, 0.14), vec3(0.44, 0.47, 0.55), sky);

  float toWindow = clamp(1.0 - abs(v_position.y - u_clerestory) / 5.0, 0.0, 1.0);
  vec3 daylight = vec3(1.02, 0.97, 0.86) * (key * 0.55 + toWindow * 0.42);

  float pool = clamp(1.0 - v_position.y * 0.22, 0.0, 1.0);
  float bounce = max(0.0, normal.y) * 0.18;
  vec3 lamps = lampLight(v_position, normal);
  vec3 colour =
    v_shade * (ambient + daylight + bounce + lamps) + vec3(0.06, 0.055, 0.05) * pool;

  colour = pow(colour, vec3(0.92));
  fragment = vec4(withFog(colour, v_depth), 1.0);
}
`;

export const ART_VERTEX = `#version 300 es
precision highp float;

in vec2 a_corner;

uniform mat4 u_viewProjection;
uniform vec3 u_centre;
uniform vec2 u_size;
uniform vec2 u_outward;
uniform vec3 u_eye;

out vec2 v_uv;
out float v_depth;
out vec3 v_position;

void main() {
  vec3 right = vec3(u_outward.y, 0.0, -u_outward.x);
  vec3 up = vec3(0.0, 1.0, 0.0);
  vec3 world = u_centre + right * a_corner.x * u_size.x + up * a_corner.y * u_size.y;

  gl_Position = u_viewProjection * vec4(world, 1.0);
  v_uv = vec2(a_corner.x + 0.5, 0.5 - a_corner.y);
  v_depth = length(world - u_eye);
  v_position = world;
}
`;

export const ART_FRAGMENT = `#version 300 es
precision highp float;

in vec2 v_uv;
in float v_depth;
in vec3 v_position;

uniform sampler2D u_image;
uniform float u_ready;
uniform float u_highlight;
uniform vec2 u_outward;

out vec4 fragment;
${LIGHTS}
${FOG}

void main() {
  vec2 uv = v_uv;
  float frame = 0.055;

  if (uv.x < frame || uv.x > 1.0 - frame || uv.y < frame || uv.y > 1.0 - frame) {
    vec3 lit = lampLight(v_position, vec3(u_outward.x, 0.0, u_outward.y));
    vec3 wood =
      mix(vec3(0.13, 0.12, 0.12), vec3(0.30, 0.28, 0.26), u_highlight) * (0.7 + lit * 0.6);
    fragment = vec4(withFog(wood, v_depth), 1.0);
    return;
  }

  vec2 inner = (uv - frame) / (1.0 - frame * 2.0);
  vec3 colour = texture(u_image, inner).rgb;
  if (u_ready < 0.5) {
    colour = vec3(0.055, 0.06, 0.075);
  }

  vec3 normal = vec3(u_outward.x, 0.0, u_outward.y);
  vec3 lamps = lampLight(v_position, normal);
  float lit = 0.75 + 0.32 * clamp(lamps.r + lamps.g + lamps.b, 0.0, 1.4);
  float fall = 1.15 - 0.4 * clamp(inner.y, 0.0, 1.0);
  float edge = smoothstep(0.0, 0.14, inner.x) * smoothstep(0.0, 0.14, 1.0 - inner.x);
  colour *= lit * fall * mix(0.86, 1.0, edge);
  colour += u_highlight * 0.16;

  fragment = vec4(withFog(colour, v_depth), 1.0);
}
`;

export const GLOW_VERTEX = `#version 300 es
precision highp float;

in vec3 a_position;

uniform mat4 u_viewProjection;

out float v_height;

void main() {
  gl_Position = u_viewProjection * vec4(a_position, 1.0);
  v_height = a_position.y;
}
`;

export const GLOW_FRAGMENT = `#version 300 es
precision highp float;

in float v_height;

uniform float u_top;
uniform vec3 u_colour;
uniform float u_strength;

out vec4 fragment;

void main() {
  float fall = clamp(v_height / max(u_top, 0.001), 0.0, 1.0);
  float amount = u_strength * mix(0.05, 1.0, fall * fall);
  fragment = vec4(u_colour * amount, amount);
}
`;
