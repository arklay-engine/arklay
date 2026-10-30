#version 450
// Indexed TIM sampling through the primitive's CLUT row. The palette's alpha
// marks the entries a semi-transparent packet blends: its STP entries, or
// every entry of a row without a knock-out twin (`scene::palette_rgba`).
// With an HD replacement (htexture) the colour comes from its layer for the
// CLUT row while the original index still decides transparency and STP.

// Normalised texture coordinates and the shade in 0..1, as the software
// rasterizer (llvmpipe's GL path) interpolates them.
layout(location = 0) in vec2 in_uv;
layout(location = 1) in vec3 in_shade;
layout(location = 2) flat in uint in_packed;
layout(location = 3) flat in float in_key;

layout(location = 0) out vec4 out_colour;

layout(set = 2, binding = 0) uniform usampler2D indices;
layout(set = 2, binding = 1) uniform sampler2D palette;
// One layer per replaced CLUT row, each in its top-left corner.
layout(set = 2, binding = 2) uniform sampler2DArray hd;
// Per CLUT row: (layer + 1 or 0, original texels covered x, y, origin x) at
// x = 0 and (replacement width, height, origin y, 0) at x = 1. The origin is
// the original texel the replacement's top-left covers (an RE2 model page).
layout(set = 2, binding = 3) uniform usampler2D hd_rows;
// The nearest mask's painter's key per pixel over 65535 (1 = none), see
// `scene::Sprites`.
layout(set = 2, binding = 4) uniform sampler2D mask_keys;

layout(std140, set = 3, binding = 0) uniform Material {
    // x = alpha of a blended texel, y = pass (0 opaque, 1 blend, 2 additive),
    // z = 1 when `hd` and `hd_rows` hold this page's replacements
    vec4 params;
};

void main() {
    // The painter's sort: a mask at or in front of the triangle's key paints
    // over all of it.
    ivec2 keys = textureSize(mask_keys, 0);
    float mask = texelFetch(mask_keys, min(ivec2(gl_FragCoord.xy), keys - 1), 0).r;
    if (mask < 1.0 && in_key >= roundEven(mask * 65535.0)) {
        discard;
    }
    bool untextured = ((in_packed >> 25) & 1u) != 0u;
    bool packet_blend = ((in_packed >> 24) & 1u) != 0u;
    vec3 texel = vec3(255.0);
    bool stp = false;
    if (!untextured) {
        // GL_NEAREST under CLAMP_TO_EDGE (`clamp_texel`).
        ivec2 size = textureSize(indices, 0);
        vec2 source = in_uv * vec2(size);
        ivec2 coord = clamp(ivec2(floor(source)), ivec2(0), size - 1);
        uint index = texelFetch(indices, coord, 0).r;
        if (index == 0u) {
            discard;
        }
        int row = int((in_packed >> 8) & 0xFFu);
        vec4 entry = vec4(0.0);
        if (row < textureSize(palette, 0).y) {
            entry = texelFetch(palette, ivec2(int(index), row), 0);
        }
        texel = entry.rgb * 255.0;
        stp = entry.a > 0.5; // blends under a semi-transparent packet
        if (params.z > 0.5 && row < textureSize(hd_rows, 0).y) {
            uvec4 place = texelFetch(hd_rows, ivec2(0, row), 0);
            if (place.x != 0u) {
                uvec4 size = texelFetch(hd_rows, ivec2(1, row), 0);
                ivec2 extent = ivec2(size.xy);
                vec2 origin = vec2(float(place.w), float(size.z));
                vec2 at = floor((source - origin) * vec2(extent) / vec2(place.yz));
                ivec2 hd_coord = clamp(ivec2(at), ivec2(0), extent - 1);
                texel = texelFetch(hd, ivec3(hd_coord, int(place.x) - 1), 0).rgb * 255.0;
            }
        }
    }
    bool blend = packet_blend && stp;
    if ((params.y < 0.5) == blend) {
        discard;
    }
    // An additive packet's blending texels draw in the additive pass alone,
    // at alpha 0 so the premultiplied blend adds them.
    bool additive = ((in_packed >> 31) & 1u) != 0u;
    if (blend && additive != (params.y > 1.5)) {
        discard;
    }
    // The target's unorm conversion rounds `texel * shade` like `unorm8`.
    vec3 rgb = clamp(texel * (1.0 / 255.0) * in_shade, 0.0, 1.0);
    out_colour = vec4(rgb, blend ? (additive ? 0.0 : params.x) : 1.0);
}
