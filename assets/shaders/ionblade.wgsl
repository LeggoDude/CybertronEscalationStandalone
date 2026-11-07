// The Ion Blade's edge (src/melee.rs), as the game's WEP_BLadeAut_MAT draws it:
// unlit and added to what is behind it, from its mask (WEP_IaconMask: its red the
// circuitry's level, its colour its own):
//   pulse  = saturate(sin((frac(-0.75 t) + r) * 2pi))^20 * (1, 0.415, 0.142)
//   glow   = (4 r)^2 * (0.864, 0.145, 0.238)
//   glint  = (camera vector . (r, r, r))^128 * (0.478, 0.122, 0.137) * 15
//            (the camera vector in the surface's tangent space, as Unreal's
//            CameraVector; the power kept to at most 1)
//   colour = pulse + glow + glint + mask
// faded out by the camera's fog as the scene behind it is fogged. (Its normal map
// and its distortion of the scene behind are left out.)

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::{globals, view}

// The camera's fog: rgb its colour (linear), a how much of it shows.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> fog: vec4<f32>;
// x unused; y: the fog's kind (0 none, 1 linear, 2 exponential, 3 exponential
// squared); z, w: its density, or its start and end.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var mask_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var mask_sampler: sampler;

const TAU: f32 = 6.283185307179586;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let mask = textureSample(mask_texture, mask_sampler, in.uv);
    let r = mask.r;
    // The pulses run through the circuitry at 0.75 a second (in step again every
    // 4 s: Bevy's clock wraps every hour, a whole number of those).
    let phase = fract((globals.time % 4.0) * -0.75) + r;
    let pulse = pow(clamp(sin(phase * TAU), 0.0, 1.0), 20.0) * vec3<f32>(1.0, 0.41460627, 0.14218920);
    let level = r * 4.0;
    let glow = level * level * vec3<f32>(0.86413044, 0.14527969, 0.23785387);

    let to_camera = view.world_position - in.world_position.xyz;
    let dist = length(to_camera);
    let v = to_camera / max(dist, 0.0001);
    let n = normalize(in.world_normal);
#ifdef VERTEX_TANGENTS
    let t = normalize(in.world_tangent.xyz);
    let b = cross(n, t) * in.world_tangent.w;
    let camera_vector = vec3<f32>(dot(v, t), dot(v, b), dot(v, n));
#else
    let camera_vector = vec3<f32>(0.0, 0.0, dot(v, n));
#endif
    let glint = min(pow(max(dot(camera_vector, vec3<f32>(r)), 0.0), 128.0), 1.0)
        * vec3<f32>(0.47826087, 0.12249230, 0.13654695) * 15.0;
    var colour = pulse + glow + glint + mask.rgb;

    // Fog: how much of the fog covers it at this distance, fading it out.
    var amount: f32 = 0.0;
    if (params.y > 2.5) {
        let d = dist * params.z;
        amount = 1.0 - exp(-d * d);
    } else if (params.y > 1.5) {
        amount = 1.0 - exp(-dist * params.z);
    } else if (params.y > 0.5) {
        amount = 1.0 - clamp((params.w - dist) / max(params.w - params.z, 0.0001), 0.0, 1.0);
    }
    colour = colour * (1.0 - amount * fog.a);
    // (Added: Bevy's additive blend is the premultiplied one, with none of what is
    // behind taken away.)
    return vec4<f32>(colour, 0.0);
}
