#version 450
// The 2D pass's quads (`draw2d::Item`): six corners per item, positioned in
// frame pixels. Everything but the position and the image coordinates is
// the item's own, passed through flat.

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec4 in_colour;
layout(location = 3) in ivec4 in_dst;
layout(location = 4) in ivec4 in_source;
layout(location = 5) in ivec4 in_clip;
layout(location = 6) in uvec2 in_mode;
layout(location = 7) in vec2 in_index_frame;

layout(location = 0) out vec2 out_uv;
layout(location = 1) flat out vec4 out_colour;
layout(location = 2) flat out ivec4 out_dst;
layout(location = 3) flat out ivec4 out_source;
layout(location = 4) flat out ivec4 out_clip;
layout(location = 5) flat out uvec2 out_mode;
layout(location = 6) flat out vec2 out_index_frame;

layout(set = 1, binding = 0) uniform Frame {
    // The frame's size in pixels.
    vec2 frame_size;
};

void main() {
    vec2 at = in_position / frame_size;
    gl_Position = vec4(at.x * 2.0 - 1.0, 1.0 - at.y * 2.0, 0.0, 1.0);
    out_uv = in_uv;
    out_colour = in_colour;
    out_dst = in_dst;
    out_source = in_source;
    out_clip = in_clip;
    out_mode = in_mode;
    out_index_frame = in_index_frame;
}
