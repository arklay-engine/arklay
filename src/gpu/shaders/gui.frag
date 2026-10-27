#version 450
// One native GUI primitive, premultiplied for a `ONE, ONE_MINUS_SRC_ALPHA`
// blend, with the coverage the software canvas (`gui::canvas`) gives the
// same primitive: a pixel is covered by the area of it inside the shape and
// the clip.
//
// Modes (`in_mode.x`):
// 0: a rectangle `in_rect` (`[x0, y0, x1, y1]`), solid or a gradient from
//    `in_from` to `in_to` along `in_axis` (0 across, 1 down); rounded to
//    `in_mode.y` when above zero.
// 1: a polygon: the colour, covered by the distances to its outer edges.
// 2: a coverage mask at integer `in_rect.xy`, `in_source.zw` in size, at
//    `in_source.xy` in the atlas (its alpha), tinted `in_from`.
// 3: a straight RGBA picture `in_source.zw` in size at `in_source.xy` in the
//    atlas, filtered into `in_rect` at opacity `in_mode.y`.

layout(location = 0) flat in vec2 in_mode;
layout(location = 1) flat in vec4 in_from;
layout(location = 2) flat in vec4 in_to;
layout(location = 3) flat in vec4 in_rect;
layout(location = 4) flat in vec4 in_clip;
layout(location = 5) flat in vec4 in_source;
layout(location = 6) in vec3 in_edges;
layout(location = 7) flat in float in_axis;

layout(location = 0) out vec4 out_colour;

layout(set = 2, binding = 0) uniform sampler2D atlas;

// The part of the pixel spanning `pixel..pixel + 1` inside `lo..hi`.
float overlap(float pixel, float lo, float hi) {
    return clamp(min(pixel + 1.0, hi) - max(pixel, lo), 0.0, 1.0);
}

void main() {
    vec2 centre = gl_FragCoord.xy;
    vec2 pixel = floor(centre);
    uint mode = uint(in_mode.x + 0.5);
    vec4 colour = in_from;
    float cover;
    if (mode == 0u) {
        vec4 span = vec4(max(in_rect.xy, in_clip.xy), min(in_rect.zw, in_clip.zw));
        float row = overlap(pixel.y, span.y, span.w);
        float radius = in_mode.y;
        if (radius > 0.0) {
            vec2 half_size = (in_rect.zw - in_rect.xy) * 0.5 - radius;
            vec2 middle = (in_rect.xy + in_rect.zw) * 0.5;
            vec2 d = max(abs(centre - middle) - half_size, vec2(0.0));
            float inside = radius - length(d);
            // Kept to the clip's columns, as the canvas's span is.
            float column = step(span.x, pixel.x + 0.5) * step(pixel.x + 0.5, span.z);
            cover = clamp(inside + 0.5, 0.0, 1.0) * row * column;
        } else {
            cover = overlap(pixel.x, span.x, span.z) * row;
        }
        vec2 size = max(in_rect.zw - in_rect.xy, vec2(1.0));
        float t = in_axis < 0.5
            ? (centre.x - in_rect.x) / size.x
            : (centre.y - in_rect.y) / size.y;
        colour = mix(in_from, in_to, clamp(t, 0.0, 1.0));
    } else if (mode == 1u) {
        float edge = min(min(in_edges.x, in_edges.y), in_edges.z);
        cover = clamp(edge + 0.5, 0.0, 1.0)
            * overlap(pixel.x, in_clip.x, in_clip.z)
            * overlap(pixel.y, in_clip.y, in_clip.w);
    } else if (mode == 2u) {
        // Whole pixels only, inside the clip's whole pixels.
        if (any(lessThan(pixel, ceil(in_clip.xy))) || any(greaterThan(pixel + 1.0, floor(in_clip.zw)))) {
            discard;
        }
        ivec2 texel = ivec2(pixel - in_rect.xy);
        if (any(lessThan(texel, ivec2(0))) || any(greaterThanEqual(texel, ivec2(in_source.zw)))) {
            discard;
        }
        cover = texelFetch(atlas, texel + ivec2(in_source.xy), 0).a;
    } else {
        vec4 span = vec4(max(in_rect.xy, in_clip.xy), min(in_rect.zw, in_clip.zw));
        cover = overlap(pixel.x, span.x, span.z) * overlap(pixel.y, span.y, span.w) * in_mode.y;
        vec2 size = max(in_rect.zw - in_rect.xy, vec2(1.0));
        vec2 texel = clamp((centre - in_rect.xy) * in_source.zw / size - 0.5,
            vec2(0.0), in_source.zw - 1.0);
        vec2 atlas_size = vec2(textureSize(atlas, 0));
        colour = texture(atlas, (in_source.xy + texel + 0.5) / atlas_size);
    }
    float alpha = colour.a * cover;
    if (alpha <= 0.0) {
        discard;
    }
    out_colour = vec4(colour.rgb * alpha, alpha);
}
