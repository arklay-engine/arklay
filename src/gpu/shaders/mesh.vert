#version 450
// Rigid-skin TMD vertex shader: one object view transform per mesh object,
// the reference's per-vertex room lighting, then the software rasterizer's
// projection. Everything per frame arrives in the uniform block; the vertex
// buffer is the mesh's static geometry, uploaded once (see
// `src/gpu/scene.rs`).

// The packet's corners, input Y already conjugated: the three it is culled
// on, then a quad's fourth (else the first again).
layout(location = 0) in vec3 in_corner0;
layout(location = 1) in vec3 in_corner1;
layout(location = 2) in vec3 in_corner2;
layout(location = 3) in vec3 in_corner3;
layout(location = 4) in vec3 in_normal;    // stored normal with Y negated
layout(location = 5) in vec2 in_uv;        // texel coordinates with the page offset
layout(location = 6) in vec3 in_colour;    // untextured packet colour / 1024, else 1
// joint | row << 8 | triangle corners << 16 | blend << 24 | untextured << 25 |
// nudge << 26 | corner << 29 | additive << 31
layout(location = 7) in uint in_packed;

// Normalised texture coordinates and the shade in 0..1, both interpolated
// perspective-correct like every llvmpipe attribute.
layout(location = 0) out vec2 out_uv;
layout(location = 1) out vec3 out_shade;
layout(location = 2) flat out uint out_packed;
// The triangle's painter's key: its corners' mean view Z, rounded.
layout(location = 3) flat out float out_key;

layout(std140, set = 1, binding = 0) uniform Draw {
    vec4 projection;    // fov, shake x, shake y, near limit
    vec4 ambient;       // rgb in 0..255
    vec4 light_colour[3]; // rgb * 255 / 128
    vec4 tint;          // rgb in 0..255
    uvec4 flags;        // x = hidden joints (bit per joint), y = mode
                        // (1 cull, 2 transparent record, 4 record blends),
                        // zw = texture page size
    vec4 joints[32 * 6]; // per joint: 3 view rows (xyz, w = t), then 3 light vectors
};

// `RefProjection::project`: view-space point of a stored corner.
vec3 view_point(uint base, vec3 p) {
    return vec3(
        dot(joints[base + 0u].xyz, p) + joints[base + 0u].w,
        dot(joints[base + 1u].xyz, p) + joints[base + 1u].w,
        dot(joints[base + 2u].xyz, p) + joints[base + 2u].w);
}

// Screen position in the logical 320x240 frame (Y down).
vec2 screen(vec3 v) {
    float iz = projection.x / v.z;
    return vec2(160.0 + projection.y + v.x * iz, 120.0 + projection.z - v.y * iz);
}

void drop_vertex() {
    gl_Position = vec4(0.0, 0.0, 0.0, 1.0);
    out_uv = vec2(0.0);
    out_shade = vec3(0.0);
    out_packed = in_packed;
    out_key = -1.0;
}

vec3 corner_of(uint slot, vec3 v0, vec3 v1, vec3 v2, vec3 v3) {
    return slot == 0u ? v0 : slot == 1u ? v1 : slot == 2u ? v2 : v3;
}

void main() {
    uint joint = in_packed & 0xFFu;
    if (joint >= 32u || ((flags.x >> joint) & 1u) != 0u) {
        drop_vertex();
        return;
    }
    uint base = joint * 6u;
    // Every vertex of a packet evaluates the same corners the same way, so
    // they all agree on dropping it.
    precise vec3 v0 = view_point(base, in_corner0);
    precise vec3 v1 = view_point(base, in_corner1);
    precise vec3 v2 = view_point(base, in_corner2);
    precise vec3 v3 = view_point(base, in_corner3);
    float near = projection.w;
    // The software drops the whole packet when any corner is inside the
    // near limit (or not a number).
    if (!(v0.z >= near && v1.z >= near && v2.z >= near && v3.z >= near)) {
        drop_vertex();
        return;
    }
    precise vec2 s0 = screen(v0);
    precise vec2 s1 = screen(v1);
    precise vec2 s2 = screen(v2);
    if ((flags.y & 1u) != 0u) {
        precise float area = (s1.x - s0.x) * (s2.y - s0.y) - (s2.x - s0.x) * (s1.y - s0.y);
        if (!(area > 0.0)) {
            drop_vertex();
            return;
        }
    }
    uint corner = (in_packed >> 29) & 3u;
    vec3 v = corner_of(corner, v0, v1, v2, v3);
    vec2 s = corner == 0u ? s0 : corner == 1u ? s1 : corner == 2u ? s2 : screen(v3);
    // The reference's ortho matrix, premultiplied by `w`, and the linear
    // depth ramp `TmdDepthNdc` (view Z 1..131072 onto 0..1).
    float w = v.z;
    float depth = clamp((w - 1.0) * (1.0 / 131071.0), 0.0, 1.0);
    gl_Position = vec4(
        (s.x * (2.0 / 320.0) - 1.0) * w,
        (1.0 - s.y * (2.0 / 240.0)) * w,
        depth * w,
        w);

    // A transparent record blends every packet by its weight, or none.
    uint packed = in_packed;
    bool record = (flags.y & 2u) != 0u;
    if (record) {
        packed = (packed & ~(1u << 24)) | (((flags.y >> 2) & 1u) << 24);
    }
    bool blend = ((packed >> 24) & 1u) != 0u;
    bool additive = ((packed >> 31) & 1u) != 0u;
    vec3 shade;
    if ((blend && !additive) || record) {
        // A semi-transparent packet and every packet of a transparent record
        // are full-bright: the mesh tint alone. An additive packet (an RE2
        // door part) is lit.
        shade = tint.rgb;
    } else {
        shade = ambient.rgb;
        for (uint light = 0u; light < 3u; light++) {
            float d = -dot(in_normal, joints[base + 3u + light].xyz);
            if (d > 0.0) {
                shade += d * light_colour[light].rgb;
            }
        }
        shade = min(shade, vec3(255.0)) * tint.rgb / 255.0;
        shade *= in_colour;
    }
    // `PSXObject_Store`: normalised coordinates, a degenerate corner nudged.
    vec2 size = vec2(max(flags.zw, uvec2(1u)));
    vec2 uv = in_uv / size;
    uint nudge = (in_packed >> 26) & 7u;
    float step = (nudge & 4u) != 0u ? 0.0001 : 0.003;
    if ((nudge & 1u) != 0u) {
        uv.x += step;
    }
    if ((nudge & 2u) != 0u) {
        uv.y += step;
    }
    out_uv = uv;
    out_shade = shade / 255.0;
    out_packed = packed;
    // `triangle_depth_key`, the same for all three vertices.
    precise float mean = (corner_of(in_packed >> 16 & 3u, v0, v1, v2, v3).z
        + corner_of(in_packed >> 18 & 3u, v0, v1, v2, v3).z
        + corner_of(in_packed >> 20 & 3u, v0, v1, v2, v3).z) * (1.0 / 3.0);
    out_key = floor(mean + 0.5);
}
