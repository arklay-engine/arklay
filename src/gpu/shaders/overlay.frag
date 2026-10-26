#version 450
// The scene under the 2D overlay with the frame's HD replacements of 2D art
// drawn over it (`htexture::Overlay`). Mode 1 copies the gameplay scene;
// mode `2 + k` is the frame's quad `k`, filtered, times the draw's colour and
// blended by its own alpha, kept only where the coverage mask says quad `k`
// wrote the original texel last, so HD art never shows outside its
// original's coverage. The composite shows this target where the overlay
// left a hole for it.
//
// A recoloured quad (`htexture::Recoloured`) draws an indexed page through a
// palette lookup: like `mesh.frag`, it keeps the original's indices, and each
// pixel takes the recolouring of the original texel under it, so the lookup
// applies at the HD image's resolution.

layout(location = 0) in vec2 in_uv;
layout(location = 1) flat in vec4 in_colour;
layout(location = 2) flat in uint in_mode;

layout(location = 0) out vec4 out_colour;

layout(set = 2, binding = 0) uniform sampler2D image;
// Per frame pixel: one more than the index of the quad that last wrote it,
// or 0.
layout(set = 2, binding = 1) uniform usampler2D coverage;
// A recoloured quad's original page indices.
layout(set = 2, binding = 2) uniform usampler2D indices;
// Three rows per recoloured quad, by index: the recolouring's scale (alpha 1
// when the index draws), luma weight and flat colour.
layout(set = 2, binding = 3) uniform sampler2D lut;

layout(set = 3, binding = 0) uniform Overlay {
    // Coverage texels per target pixel.
    vec2 coverage_scale;
    // A recoloured quad's original texels per unit of `in_uv` (its HD
    // image's frame); 0 for any other quad.
    vec2 index_frame;
    // The first of a recoloured quad's `lut` rows.
    float lut_row;
};

void main() {
    vec4 texel = texture(image, in_uv);
    if (in_mode == 1u) {
        out_colour = vec4(texel.rgb, 1.0);
        return;
    }
    ivec2 size = textureSize(coverage, 0);
    ivec2 at = min(ivec2(gl_FragCoord.xy * coverage_scale), size - 1);
    if (texelFetch(coverage, at, 0).r != in_mode - 1u) {
        discard;
    }
    if (index_frame.x > 0.0) {
        ivec2 page = textureSize(indices, 0);
        ivec2 source = clamp(ivec2(floor(in_uv * index_frame)), ivec2(0), page - 1);
        int index = int(texelFetch(indices, source, 0).r);
        int row = int(lut_row);
        vec4 scale = texelFetch(lut, ivec2(index, row), 0);
        if (scale.a < 0.5) {
            discard;
        }
        vec3 luma = texelFetch(lut, ivec2(index, row + 1), 0).rgb;
        vec3 flat_colour = texelFetch(lut, ivec2(index, row + 2), 0).rgb;
        float brightness = dot(texel.rgb, vec3(0.299, 0.587, 0.114));
        texel.rgb = clamp(texel.rgb * scale.rgb + brightness * luma + flat_colour, 0.0, 1.0);
    }
    out_colour = vec4(texel.rgb * in_colour.rgb, texel.a * in_colour.a);
}
