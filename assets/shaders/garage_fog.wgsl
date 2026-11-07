// The Customize garage's height fog (src/customize.rs's GarageFog): the game's
// HeightFog layers (UI_CustomCharacter_m's four), each a fog that fills everything
// below its height, from its start distance out. The garage's lie under the
// platforms' tops: they fill the pit below them with teal haze, which the camera
// looks down into, and leave the room above clear.
//
// Drawn as one triangle over the whole screen, after the opaque pass: for each
// pixel the ray from the camera to what the depth prepass holds there, how much of
// it runs below each layer's top, and the fog that much of the layer lays over it
// (1 - e^(-density * length), its light in the game's units), the layers one over
// another. Premultiplied: the fog's light, and how much of the scene it hides.

#import bevy_pbr::mesh_view_bindings::view
#ifdef DEPTH_PREPASS
#import bevy_pbr::mesh_view_bindings::depth_prepass_texture
#endif

struct FogLayers {
    // rgb: a layer's light (the game's units); w: its density (per metre).
    color: array<vec4<f32>, 4>,
    // x: its top (m); y: how far from the camera it starts (m); z: 1 if it is there.
    shape: array<vec4<f32>, 4>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> layers: FogLayers;

struct Vertex {
    @location(0) position: vec3<f32>,
}

struct Varyings {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vertex(v: Vertex) -> Varyings {
    var out: Varyings;
    // (The mesh is a triangle over the whole screen in clip space, at the near
    // plane: in front of everything.)
    out.position = vec4<f32>(v.position.xy, 1.0, 1.0);
    return out;
}

// How much of a layer covers what the ray from `cam` to `p` reaches.
fn layer_amount(cam: vec3<f32>, p: vec3<f32>, top: f32, density: f32, start: f32) -> f32 {
    let d = p - cam;
    let len = length(d);
    if (len < 0.0001) {
        return 0.0;
    }
    // The part of the ray (0 at the camera, 1 at p) below the top.
    var t0 = 0.0;
    var t1 = 1.0;
    if (abs(d.y) < 0.000001) {
        if (cam.y >= top) {
            return 0.0;
        }
    } else {
        let t = (top - cam.y) / d.y;
        if (d.y > 0.0) {
            t1 = min(t1, t);
        } else {
            t0 = max(t0, t);
        }
    }
    t0 = max(t0, start / len);
    let inside = max(t1 - t0, 0.0) * len;
    return 1.0 - exp(-density * inside);
}

@fragment
fn fragment(in: Varyings) -> @location(0) vec4<f32> {
    var depth = 0.0;
#ifdef DEPTH_PREPASS
    depth = textureLoad(depth_prepass_texture, vec2<i32>(in.position.xy), 0);
#endif
    let uv = (in.position.xy - view.viewport.xy) / view.viewport.zw;
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let cam = view.world_position;
    var p: vec3<f32>;
    if (depth > 0.0) {
        let w = view.world_from_clip * vec4<f32>(ndc, depth, 1.0);
        p = w.xyz / w.w;
    } else {
        // (Nothing there: far out along the ray.)
        let w = view.world_from_clip * vec4<f32>(ndc, 1.0, 1.0);
        p = cam + normalize(w.xyz / w.w - cam) * 1000.0;
    }
    var light = vec3<f32>(0.0);
    var through = 1.0;
    for (var k = 0; k < 4; k++) {
        let shape = layers.shape[k];
        if (shape.z < 0.5) {
            continue;
        }
        let a = layer_amount(cam, p, shape.x, layers.color[k].w, shape.y);
        light = light * (1.0 - a) + layers.color[k].rgb * a;
        through *= 1.0 - a;
    }
    return vec4<f32>(light, 1.0 - through);
}
