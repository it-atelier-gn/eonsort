const FACING = `
vec3 place(vec3 centre, vec2 corner, vec2 size, vec2 outward) {
  vec3 right = vec3(outward.y, 0.0, -outward.x);
  vec3 up = vec3(0.0, 1.0, 0.0);
  return centre + right * corner.x * size.x + up * corner.y * size.y;
}
`;

export const TILE_VERTEX = `#version 300 es
precision highp float;

in vec2 a_corner;
in vec3 a_centre;
in vec2 a_outward;
in vec3 a_colour;

uniform mat4 u_viewProjection;
uniform vec2 u_size;
uniform vec3 u_eye;

out vec2 v_uv;
out vec3 v_colour;
out float v_facing;
out float v_depth;
${FACING}

void main() {
  vec3 world = place(a_centre, a_corner, u_size, a_outward);
  gl_Position = u_viewProjection * vec4(world, 1.0);

  vec3 toEye = normalize(u_eye - a_centre);
  v_facing = dot(toEye, vec3(a_outward.x, 0.0, a_outward.y));
  v_uv = vec2(a_corner.x + 0.5, 0.5 - a_corner.y);
  v_colour = a_colour;
  v_depth = length(world - u_eye);
}
`;

export const TILE_FRAGMENT = `#version 300 es
precision highp float;

in vec2 v_uv;
in vec3 v_colour;
in float v_facing;
in float v_depth;

out vec4 fragment;

void main() {
  float edge = 0.09;
  bool border =
    v_uv.x < edge || v_uv.x > 1.0 - edge || v_uv.y < edge || v_uv.y > 1.0 - edge;

  float lit = mix(0.55, 1.15, clamp(v_facing, 0.0, 1.0));
  vec3 colour = v_colour * (border ? lit : lit * 0.78);

  float haze = 1.0 - exp(-v_depth * 0.012);
  colour = mix(colour, vec3(0.045, 0.055, 0.075), clamp(haze, 0.0, 0.65));

  fragment = vec4(colour, 1.0);
}
`;

export const PICK_VERTEX = `#version 300 es
precision highp float;

in vec2 a_corner;
in vec3 a_centre;
in vec2 a_outward;
in float a_id;

uniform mat4 u_viewProjection;
uniform vec2 u_size;

out vec3 v_id;
${FACING}

void main() {
  vec3 world = place(a_centre, a_corner, u_size, a_outward);
  gl_Position = u_viewProjection * vec4(world, 1.0);

  float id = a_id + 1.0;
  v_id = vec3(
    mod(id, 256.0),
    mod(floor(id / 256.0), 256.0),
    mod(floor(id / 65536.0), 256.0)
  ) / 255.0;
}
`;

export const PICK_FRAGMENT = `#version 300 es
precision highp float;

in vec3 v_id;

out vec4 fragment;

void main() {
  fragment = vec4(v_id, 1.0);
}
`;

export const PICTURE_VERTEX = `#version 300 es
precision highp float;

in vec2 a_corner;

uniform mat4 u_viewProjection;
uniform vec3 u_centre;
uniform vec2 u_outward;
uniform vec2 u_size;
uniform vec3 u_eye;

out vec2 v_uv;
out float v_depth;
${FACING}

void main() {
  vec3 world = place(u_centre, a_corner, u_size, u_outward);
  gl_Position = u_viewProjection * vec4(world, 1.0);
  v_uv = vec2(a_corner.x + 0.5, 0.5 - a_corner.y);
  v_depth = length(world - u_eye);
}
`;

export const PICTURE_FRAGMENT = `#version 300 es
precision highp float;

in vec2 v_uv;
in float v_depth;

uniform sampler2D u_image;
uniform vec3 u_colour;
uniform float u_highlight;

out vec4 fragment;

void main() {
  float edge = 0.09;
  if (v_uv.x < edge || v_uv.x > 1.0 - edge || v_uv.y < edge || v_uv.y > 1.0 - edge) {
    vec3 frame = u_colour * mix(0.85, 1.6, u_highlight);
    fragment = vec4(frame, 1.0);
    return;
  }

  vec2 inner = (v_uv - edge) / (1.0 - edge * 2.0);
  vec3 colour = texture(u_image, inner).rgb;
  colour += u_highlight * 0.18;

  float haze = 1.0 - exp(-v_depth * 0.012);
  colour = mix(colour, vec3(0.045, 0.055, 0.075), clamp(haze, 0.0, 0.65));

  fragment = vec4(colour, 1.0);
}
`;
