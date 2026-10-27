#version 450
// The native GUI's primitives (`gui::canvas::Op`, tessellated by `gpu::gui`),
// positioned in canvas pixels. Everything but the position and a polygon's
// edge distances is the primitive's own, passed through flat.

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec2 in_mode;
layout(location = 2) in vec4 in_from;
layout(location = 3) in vec4 in_to;
layout(location = 4) in vec4 in_rect;
layout(location = 5) in vec4 in_clip;
layout(location = 6) in vec4 in_source;
layout(location = 7) in vec4 in_edges;

layout(location = 0) flat out vec2 out_mode;
layout(location = 1) flat out vec4 out_from;
layout(location = 2) flat out vec4 out_to;
layout(location = 3) flat out vec4 out_rect;
layout(location = 4) flat out vec4 out_clip;
layout(location = 5) flat out vec4 out_source;
layout(location = 6) out vec3 out_edges;
layout(location = 7) flat out float out_axis;

layout(set = 1, binding = 0) uniform Canvas {
    // The canvas's size in pixels.
    vec2 canvas_size;
};

void main() {
    vec2 at = in_position / canvas_size;
    gl_Position = vec4(at.x * 2.0 - 1.0, 1.0 - at.y * 2.0, 0.0, 1.0);
    out_mode = in_mode;
    out_from = in_from;
    out_to = in_to;
    out_rect = in_rect;
    out_clip = in_clip;
    out_source = in_source;
    out_edges = in_edges.xyz;
    out_axis = in_edges.w;
}
