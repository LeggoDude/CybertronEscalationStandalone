// The characters' shadows from the lights other than the sun (src/shadows.rs, the
// game's light environments' composite shadows), each of the character's own shape.
// Drawn over the picture in a box round where a character's shadow can fall,
// multiplying it: each pixel's place is found from the depth prepass and looked up
// in the shadow map of the character's own light (a spot light giving no light, far
// off the way the light comes from, whose shadow map holds that character alone:
// found by its tag among the lights of the pixel's cluster). The pixel is shaded
// where the character stands between it and that light by more than MIN_GAP (so not
// by its own surface), softly at the edges, more softly the further the shadow falls
// behind the part casting it (the blockers found round it: the edge as wide as
// SOFTNESS plus SOFT_PER_M of that, at most MAX_BLUR). What is within a little of the
// body's capsules is the character itself and left as it is (its own shading has its
// dark side). Where the sun reaches the character (its shadow map at a point just
// out of the character towards it: shadow.sun), the sun's shadow is the character's,
// and this one gives way as much as the sun reaches it.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_functions::get_tag
#import bevy_pbr::view_transformations::{frag_coord_to_ndc, position_ndc_to_world}
#import bevy_pbr::mesh_view_bindings::{lights, view, clusterable_objects, directional_shadow_textures, directional_shadow_textures_comparison_sampler}
#import bevy_pbr::mesh_view_types::{DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT, POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT}
#import bevy_pbr::shadows::fetch_directional_shadow
#import bevy_pbr::clustered_forward as clustering
#import bevy_pbr::utils::interleaved_gradient_noise
#import bevy_render::maths::{orthonormalize, PI_2}
#ifdef DEPTH_PREPASS
#import bevy_pbr::mesh_view_bindings::depth_prepass_texture
#endif

struct ShadowParams {
    // Towards the light (unit), and how far the shadow can fall (m).
    dir: vec4<f32>,
    // Darkness, edge softness (m), its growth per metre, capsule count.
    shade: vec4<f32>,
    // Each capsule's ends, and in a.w its radius.
    a: array<vec4<f32>, 16>,
    b: array<vec4<f32>, 16>,
    // Where to look whether the sun reaches it, and in w 1 on a map with a sun.
    sun: vec4<f32>,
    // Its light: where it stands, and in w its tag.
    lamp: vec4<f32>,
    // Its light's lens: the tangent of its cone's half angle, its near plane, the
    // least gap behind what casts the shadow, the widest blur (m).
    lens: vec4<f32>,
}

// The slots' shadows (shadows.rs's SlotShadows), each box's by its mesh tag.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<storage, read> shadows: array<ShadowParams>;
// This box's slot (its mesh tag), set as its fragment starts.
var<private> slot: u32;

// A pixel this near a capsule (its radius times SELF_SCALE, plus SELF_MARGIN m)
// is taken as the character's own surface.
const SELF_SCALE: f32 = 1.05;
const SELF_MARGIN: f32 = 0.03;
// The blocker search's taps (and its middle), and the edge's.
const SEARCH_TAPS: u32 = 24u;
const EDGE_TAPS: u32 = 48u;
const GOLDEN_ANGLE: f32 = 2.39996323;

// The distance from p to the segment from a to b.
fn point_segment(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>) -> f32 {
    let d = b - a;
    let dd = dot(d, d);
    var s = 0.0;
    if (dd > 1e-8) {
        s = clamp(dot(p - a, d) / dd, 0.0, 1.0);
    }
    return length(p - a - s * d);
}

// How much the sun reaches `at` (0 to 1): its shadow map there (the first
// directional light casting shadows: the map's sun). None where its cascades don't
// reach, as there it casts no shadow of the character either.
fn sun_reaches(at: vec3<f32>) -> f32 {
    for (var i = 0u; i < lights.n_directional_lights; i++) {
        let light = &lights.directional_lights[i];
        if (((*light).flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) == 0u) {
            continue;
        }
        let n = (*light).num_cascades;
        let p = vec4<f32>(at, 1.0);
        let view_z = view_depth(at);
        if (n == 0u || -view_z >= (*light).cascades[n - 1u].far_bound) {
            return 0.0;
        }
        return fetch_directional_shadow(i, p, (*light).direction_to_light, view_z);
    }
    return 0.0;
}

// A point's depth in the camera's view (negative in front of it).
fn view_depth(p: vec3<f32>) -> f32 {
    return dot(vec4<f32>(
        view.view_from_world[0].z,
        view.view_from_world[1].z,
        view.view_from_world[2].z,
        view.view_from_world[3].z,
    ), vec4<f32>(p, 1.0));
}

// The layer of the shadow maps' texture this shadow's light draws into: its light
// found among the spot lights of the pixel's cluster by its tag (-1: not there, or
// Bevy draws it no shadow map).
fn lamp_layer(frag: vec2<f32>, p: vec3<f32>) -> i32 {
    let ortho = view.clip_from_view[3].w == 1.0;
    let cluster = clustering::fragment_cluster_index(frag, view_depth(p), ortho);
    let ranges = clustering::unpack_clusterable_object_index_ranges(cluster);
    for (var i = ranges.first_spot_light_index_offset;
            i < ranges.first_reflection_probe_index_offset; i++) {
        let id = clustering::get_clusterable_object_id(i);
        let light = &clusterable_objects.data[id];
        if (abs((*light).position_radius.w - shadows[slot].lamp.w) < 0.25
                && ((*light).flags & POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u) {
            return i32(id) + lights.spot_light_shadowmap_offset;
        }
    }
    return -1;
}

// Where p is on the light's shadow map (uv) and how far along its axis (m), as
// Bevy draws a spot light's (shadows.rs's lamp_view).
fn lamp_view(p: vec3<f32>) -> vec3<f32> {
    let q = (p - shadows[slot].lamp.xyz) * orthonormalize(shadows[slot].dir.xyz);
    let z = -q.z;
    let ndc = q.xy / (shadows[slot].lens.x * z);
    return vec3<f32>(ndc * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5), z);
}

// The shadow map's depth there (Bevy's spot lights: reversed, near over the
// distance; 0 where nothing is drawn).
fn map_depth(uv: vec2<f32>, layer: i32, size: vec2<f32>) -> f32 {
    let texel = clamp(vec2<i32>(uv * size), vec2<i32>(0), vec2<i32>(size) - 1);
#ifdef NO_ARRAY_TEXTURES_SUPPORT
    return textureLoad(directional_shadow_textures, texel, 0);
#else
    return textureLoad(directional_shadow_textures, texel, layer, 0);
#endif
}

// 1 where nothing nearer the light than `depth` is drawn there (the hardware's
// 2x2 comparison).
fn map_clear(uv: vec2<f32>, layer: i32, depth: f32) -> f32 {
#ifdef NO_ARRAY_TEXTURES_SUPPORT
    return textureSampleCompareLevel(directional_shadow_textures,
        directional_shadow_textures_comparison_sampler, uv, depth);
#else
    return textureSampleCompareLevel(directional_shadow_textures,
        directional_shadow_textures_comparison_sampler, uv, layer, depth);
#endif
}

// A tap of a spiral (Vogel's) over the unit disc, turned by `turn`.
fn spiral(i: u32, n: u32, turn: f32) -> vec2<f32> {
    let r = sqrt((f32(i) + 0.5) / f32(n));
    let a = f32(i) * GOLDEN_ANGLE + turn;
    return vec2<f32>(cos(a), sin(a)) * r;
}

// How much the character's shadow covers p (0 to 1), from its light's shadow map.
fn occlusion(p: vec3<f32>, frag: vec2<f32>) -> f32 {
    let layer = lamp_layer(frag, p);
    if (layer < 0) {
        return 0.0;
    }
    let s = lamp_view(p);
    if (s.z <= shadows[slot].lens.y || any(s.xy < vec2<f32>(0.0)) || any(s.xy > vec2<f32>(1.0))) {
        return 0.0;
    }
    let near = shadows[slot].lens.y;
    let least = shadows[slot].lens.z;
    let widest = shadows[slot].lens.w;
    let size = vec2<f32>(textureDimensions(directional_shadow_textures).xy);
    // (Metres the map spans at p's distance: a metre across is this much of it.)
    let per_m = 1.0 / (2.0 * shadows[slot].lens.x * s.z);
    let turn = interleaved_gradient_noise(frag, 0u) * PI_2;
    // The blockers round it (more than `least` nearer the light), and how far in
    // front of it they are on average.
    var found = 0.0;
    var gaps = 0.0;
    for (var i = 0u; i <= SEARCH_TAPS; i++) {
        var o = vec2<f32>(0.0);
        if (i < SEARCH_TAPS) {
            o = spiral(i, SEARCH_TAPS, turn) * widest;
        }
        let d = map_depth(s.xy + o * per_m, layer, size);
        if (d > 0.0) {
            let gap = s.z - near / d;
            if (gap > least) {
                found += 1.0;
                gaps += gap;
            }
        }
    }
    if (found == 0.0) {
        return 0.0;
    }
    let gap = gaps / found;
    // (Fading out towards the end of its reach.)
    let fade = 1.0 - smoothstep(shadows[slot].dir.w * 0.6, shadows[slot].dir.w, gap);
    if (found == f32(SEARCH_TAPS + 1u)) {
        return fade;
    }
    // Its edge: as soft as the shadow falls far behind them.
    let blur = clamp(shadows[slot].shade.y + shadows[slot].shade.z * gap, shadows[slot].shade.y, widest);
    let depth = near / max(s.z - least, near);
    var clear = 0.0;
    for (var i = 0u; i < EDGE_TAPS; i++) {
        let o = spiral(i, EDGE_TAPS, turn) * blur;
        clear += map_clear(s.xy + o * per_m, layer, depth);
    }
    return (1.0 - clear / f32(EDGE_TAPS)) * fade;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    slot = get_tag(in.instance_index);
#ifdef DEPTH_PREPASS
    let depth = textureLoad(depth_prepass_texture, vec2<i32>(in.position.xy), 0);
    // (Nothing there: the sky, the far backdrop.)
    if (depth <= 0.0) {
        return vec4<f32>(1.0);
    }
    let p = position_ndc_to_world(frag_coord_to_ndc(vec4<f32>(in.position.xy, depth, 1.0)));
    let count = u32(shadows[slot].shade.w);
    for (var i = 0u; i < count; i++) {
        // (On the character itself: left as it is.)
        if (point_segment(p, shadows[slot].a[i].xyz, shadows[slot].b[i].xyz)
                < shadows[slot].a[i].w * SELF_SCALE + SELF_MARGIN) {
            return vec4<f32>(1.0);
        }
    }
    var occluded = occlusion(p, in.position.xy);
    // (In the sun, the sun's shadow alone.)
    if (shadows[slot].sun.w > 0.5 && occluded > 0.0) {
        occluded *= 1.0 - sun_reaches(shadows[slot].sun.xyz);
    }
    // (A darkness of 1, which the demo never gives, shows the shadow in red
    // instead: the tests' way to see where it falls.)
    if (shadows[slot].shade.x >= 1.0) {
        return vec4<f32>(1.0, 1.0 - occluded, 1.0 - occluded, 1.0);
    }
    let k = occluded * shadows[slot].shade.x;
    return vec4<f32>(vec3<f32>(1.0 - k), 1.0);
#else
    return vec4<f32>(1.0);
#endif
}
