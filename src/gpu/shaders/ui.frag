#version 450
// A premultiplied RGBA UI layer (the F1 debug menu at a wider aspect, drawn
// by the 2D pass into a clear target) over the composited frame, sampled
// sharply like the composite's frame. The pipeline blends by source alpha,
// so the filtered colour is divided back out of its coverage.

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_colour;

layout(set = 2, binding = 0) uniform sampler2D layer;

void main() {
    vec2 size = vec2(textureSize(layer, 0));
    vec2 texel = in_uv * size;
    vec2 zoom = max(1.0 / max(fwidth(texel), vec2(1.0e-6)), vec2(1.0));
    vec2 offset = fract(texel) - 0.5;
    vec2 inner = 0.5 - 0.5 / zoom;
    vec2 blend = (offset - clamp(offset, -inner, inner)) * zoom + 0.5;
    vec4 colour = texture(layer, (floor(texel) + blend) / size);
    out_colour = colour.a > 0.0 ? vec4(colour.rgb / colour.a, colour.a) : vec4(0.0);
}
