#version 450
// One item of the 2D pass (`draw2d::Item`), premultiplied for a
// `ONE, ONE_MINUS_SRC_ALPHA` blend.
//
// Every target pixel works in the frame pixel it lies in, so the pass draws
// the frame's pixel art at any target size the way the software rasterizer
// draws it at 320x240: the item's clip, and a sprite's nearest texel
// (`draw2d::Map`, the same integer mapping), are the frame pixel's. Images
// with detail of their own (HD art, the scene, a model) are filtered at the
// target's resolution instead.
//
// Modes (`in_mode.x & 0xFF`):
// 0: the item's colour.
// 1: the atlas texel through the map, times the colour; a clear texel draws
//    nothing.
// 2: HD art times the colour, kept to the map's opaque atlas texels when
//    `MASKED`.
// 3: as 2, through the original page's indices and the recolouring rows of
//    `lut` from `in_mode.y` (see `htexture::Recoloured`).
// 4: a premultiplied image: the scene, a model's target.
//
// An `ADDITIVE` item writes zero alpha, so the blend adds its colour to the
// target (`draw2d::ADDITIVE`, the PSX additive semi-transparency).

layout(location = 0) in vec2 in_uv;
layout(location = 1) flat in vec4 in_colour;
// `draw2d::Map::dst`: `[x, y, w, h]` in frame pixels.
layout(location = 2) flat in ivec4 in_dst;
// The source's `[w, h]`, then the atlas offset of its texels.
layout(location = 3) flat in ivec4 in_source;
// `[x0, y0, x1, y1]` in frame pixels.
layout(location = 4) flat in ivec4 in_clip;
// The mode, `draw2d::Map::flags << 8`; the first `lut` row.
layout(location = 5) flat in uvec2 in_mode;
// A recoloured item's original texels per unit of `in_uv`.
layout(location = 6) flat in vec2 in_index_frame;

layout(location = 0) out vec4 out_colour;

layout(set = 2, binding = 0) uniform sampler2D atlas;
layout(set = 2, binding = 1) uniform sampler2D image;
layout(set = 2, binding = 2) uniform usampler2D indices;
layout(set = 2, binding = 3) uniform sampler2D lut;

layout(set = 3, binding = 0) uniform Target {
    // Frame pixels per target pixel.
    vec2 frame_per_target;
};

const uint MIRROR_X = 1u;
const uint MIRROR_Y = 2u;
const uint CENTRED = 4u;
const uint MASKED = 8u;
const uint ADDITIVE = 32u;

int source_texel(int pixel, int start, int extent, int source, bool centred, bool mirror) {
    // As `draw2d::Map::texel` computes it.
    int local = pixel - start;
    int offset = centred
        ? min(max(local * 2 + 1, 0) * source / (2 * extent), source - 1)
        : local * source / extent;
    return mirror ? source - 1 - offset : offset;
}

vec4 atlas_texel(ivec2 pixel, uint flags) {
    ivec2 texel = ivec2(
        source_texel(pixel.x, in_dst.x, max(in_dst.z, 1), in_source.x,
            (flags & CENTRED) != 0u, (flags & MIRROR_X) != 0u),
        source_texel(pixel.y, in_dst.y, max(in_dst.w, 1), in_source.y,
            (flags & CENTRED) != 0u, (flags & MIRROR_Y) != 0u));
    ivec2 size = textureSize(atlas, 0);
    return texelFetch(atlas, clamp(texel + in_source.zw, ivec2(0), size - 1), 0);
}

void main() {
    ivec2 pixel = ivec2(floor(gl_FragCoord.xy * frame_per_target));
    if (any(lessThan(pixel, in_clip.xy)) || any(greaterThanEqual(pixel, in_clip.zw))) {
        discard;
    }
    uint mode = in_mode.x & 0xFFu;
    uint flags = in_mode.x >> 8;
    vec4 colour;
    if (mode == 0u) {
        colour = in_colour;
    } else if (mode == 1u) {
        vec4 texel = atlas_texel(pixel, flags);
        if (texel.a < 0.5) {
            discard;
        }
        colour = vec4(texel.rgb * in_colour.rgb, in_colour.a);
    } else if (mode == 4u) {
        out_colour = texture(image, in_uv) * in_colour.a;
        return;
    } else {
        if ((flags & MASKED) != 0u && atlas_texel(pixel, flags).a < 0.5) {
            discard;
        }
        vec4 texel = texture(image, in_uv);
        if (mode == 3u) {
            ivec2 page = textureSize(indices, 0);
            ivec2 source = clamp(ivec2(floor(in_uv * in_index_frame)), ivec2(0), page - 1);
            int index = int(texelFetch(indices, source, 0).r);
            int row = int(in_mode.y);
            vec4 scale = texelFetch(lut, ivec2(index, row), 0);
            if (scale.a < 0.5) {
                discard;
            }
            vec3 luma = texelFetch(lut, ivec2(index, row + 1), 0).rgb;
            vec3 flat_colour = texelFetch(lut, ivec2(index, row + 2), 0).rgb;
            float brightness = dot(texel.rgb, vec3(0.299, 0.587, 0.114));
            texel.rgb = clamp(texel.rgb * scale.rgb + brightness * luma + flat_colour, 0.0, 1.0);
        }
        colour = vec4(min(texel.rgb * in_colour.rgb, vec3(1.0)), texel.a * in_colour.a);
    }
    if ((flags & ADDITIVE) != 0u) {
        out_colour = vec4(colour.rgb * colour.a, 0.0);
        return;
    }
    out_colour = vec4(colour.rgb * colour.a, colour.a);
}
