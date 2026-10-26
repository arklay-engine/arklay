#version 450
// Modes: 0 = textured (rgb * colour, alpha = colour.a), 1 = shadow (the
// bilinear texel times the colour, alpha as coverage), 2 = clamped background
// (opaque). Texture coordinates are texels for modes 0 and 2 and normalised
// for mode 1. An HD replacement (htexture) is bound in place of the original
// with `scale` set to its texels per original texel; it is (1, 1) for the
// original.

layout(location = 0) in vec2 in_uv;
layout(location = 1) flat in vec4 in_colour;
layout(location = 2) flat in uint in_mode;
layout(location = 3) flat in float in_key;

layout(location = 0) out vec4 out_colour;

layout(set = 2, binding = 0) uniform sampler2D page;
// The nearest mask's painter's key per pixel over 65535 (1 = none), see
// `scene::Sprites`.
layout(set = 2, binding = 1) uniform sampler2D mask_keys;

layout(std140, set = 3, binding = 0) uniform Replacement {
    vec4 scale; // xy = texels per original texel
};

// `lerp_unorm8`: llvmpipe's 8-bit lerp in 16-bit lanes.
uvec4 lerp_unorm8(uint weight, uvec4 from, uvec4 to) {
    uvec4 product = (uvec4(weight) * ((to - from) & 0xFFFFu)) & 0xFFFFu;
    product = (product + 0x7Fu + ((product & 0x100u) >> 8)) & 0xFFFFu;
    return (from + (product >> 8)) & 0xFFu;
}

// `sample_bilinear`: llvmpipe's AoS bilinear fetch of an RGBA8
// clamp-to-edge texture, with 8.8 fixed-point coordinates.
uvec4 sample_bilinear(vec2 uv, ivec2 size) {
    vec2 scaled = clamp(roundEven(uv * (vec2(size) * 256.0)), -16777216.0, 16777216.0);
    ivec2 fixed_point = ivec2(scaled) - 128;
    ivec2 whole = fixed_point >> 8;
    uvec2 weight = uvec2(fixed_point & 0xFF);
    ivec2 c0 = clamp(whole, ivec2(0), size - 1);
    ivec2 c1 = clamp(whole + 1, ivec2(0), size - 1);
    uvec4 t00 = uvec4(round(texelFetch(page, ivec2(c0.x, c0.y), 0) * 255.0));
    uvec4 t10 = uvec4(round(texelFetch(page, ivec2(c1.x, c0.y), 0) * 255.0));
    uvec4 t01 = uvec4(round(texelFetch(page, ivec2(c0.x, c1.y), 0) * 255.0));
    uvec4 t11 = uvec4(round(texelFetch(page, ivec2(c1.x, c1.y), 0) * 255.0));
    uvec4 top = lerp_unorm8(weight.x, t00, t10);
    uvec4 bottom = lerp_unorm8(weight.x, t01, t11);
    return lerp_unorm8(weight.y, top, bottom);
}

void main() {
    // The painter's sort: a mask at or in front of the key paints over it.
    ivec2 keys = textureSize(mask_keys, 0);
    float mask = texelFetch(mask_keys, min(ivec2(gl_FragCoord.xy), keys - 1), 0).r;
    if (mask < 1.0 && in_key >= roundEven(mask * 65535.0)) {
        discard;
    }
    ivec2 size = textureSize(page, 0);
    if (in_mode == 1u) {
        // `rasterize_shadow`: the bilinear texel times the vertex colour,
        // blended by its alpha, each rounded to unorm8 like llvmpipe.
        vec4 texel = vec4(sample_bilinear(in_uv, size)) * (1.0 / 255.0);
        vec4 colour = roundEven(clamp(texel * in_colour, 0.0, 1.0) * 255.0) / 255.0;
        if (colour.a == 0.0) {
            discard;
        }
        out_colour = colour;
        return;
    }
    ivec2 coord = clamp(ivec2(floor(in_uv * scale.xy)), ivec2(0), size - 1);
    vec4 texel = texelFetch(page, coord, 0);
    if (in_mode == 2u) {
        out_colour = vec4(texel.rgb * in_colour.rgb, 1.0);
        return;
    }
    // An original texel is clear or opaque; a replacement's soft edge keeps
    // its nearer half, as a mask's key covers whole pixels.
    if (texel.a < 0.5) {
        discard;
    }
    out_colour = vec4(texel.rgb * in_colour.rgb, in_colour.a);
}
