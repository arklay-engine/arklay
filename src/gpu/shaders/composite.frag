#version 450
// The presented frame into the window: the 2D pass's target, with the
// gameplay scene drawn into it, or an image (a software frame, an HD film
// frame).
//
// The frame is scaled to the viewport sharply: each screen pixel takes its
// texel's colour, and only a pixel straddling two texels blends them by its
// coverage. At a whole zoom no pixel straddles, so the result is exactly
// nearest sampling; at a fractional zoom every texel still covers the same
// width. The sampler is bilinear, so a frame larger than the viewport (an HD
// film frame) is minified bilinearly.

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_colour;

layout(set = 2, binding = 0) uniform sampler2D frame;

void main() {
    vec2 size = vec2(textureSize(frame, 0));
    vec2 texel = in_uv * size;
    // Screen pixels per texel along each axis.
    vec2 zoom = max(1.0 / max(fwidth(texel), vec2(1.0e-6)), vec2(1.0));
    vec2 offset = fract(texel) - 0.5;
    vec2 inner = 0.5 - 0.5 / zoom;
    vec2 blend = (offset - clamp(offset, -inner, inner)) * zoom + 0.5;
    vec2 uv = (floor(texel) + blend) / size;
    out_colour = vec4(texture(frame, uv).rgb, 1.0);
}
