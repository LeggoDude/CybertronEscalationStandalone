// The energon cube's sides (src/energon.rs), as the game's
// PROP_NEU_EnergonCube_Clouds_MAT draws them: unlit,
//   clouds = fractalMaps at uv/2, pushed by fractalMaps at uv/2 panning (0.1, 0.2)/s
//   x      = CoreColor * CoreColor + clouds
//   facing = (normal . to camera)^10
//   colour = lerp(x*x, x*x * face-on colour * facing, saturate(facing)) * 4
// then fogged as Bevy fogs the rest of the scene.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::{globals, view}

// rgb: CoreColor (linear); a unused.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> core_color: vec4<f32>;
// rgb: the face-on colour; a: the brightness.
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> face: vec4<f32>;
// The camera's fog: rgb its colour (linear), a how much of it shows.
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> fog: vec4<f32>;
// x unused; y: the fog's kind (0 none, 1 linear, 2 exponential, 3 exponential
// squared); z, w: its density, or its start and end.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var clouds_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var clouds_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv * 0.5;
    // The clouds pan at 0.1 and 0.2 of the texture a second: in step again every
    // 10 s (Bevy's clock wraps every hour, a whole number of those).
    let seconds = globals.time % 10.0;
    let push = textureSample(clouds_texture, clouds_sampler, uv + vec2<f32>(0.1, 0.2) * seconds).rg;
    let clouds = textureSample(clouds_texture, clouds_sampler, uv + push).rgb;
    let x = core_color.rgb * core_color.rgb + clouds;
    let base = x * x;

    let n = normalize(in.world_normal);
    let to_camera = view.world_position - in.world_position.xyz;
    let dist = length(to_camera);
    let facing = pow(clamp(dot(n, to_camera / max(dist, 0.0001)), 0.0, 1.0), 10.0);
    let face_on = base * face.rgb * facing;
    var colour = mix(base, face_on, clamp(facing, 0.0, 1.0)) * face.a;

    // Fog: how much of the fog colour covers it at this distance.
    var amount: f32 = 0.0;
    if (params.y > 2.5) {
        let d = dist * params.z;
        amount = 1.0 - exp(-d * d);
    } else if (params.y > 1.5) {
        amount = 1.0 - exp(-dist * params.z);
    } else if (params.y > 0.5) {
        amount = 1.0 - clamp((params.w - dist) / max(params.w - params.z, 0.0001), 0.0, 1.0);
    }
    colour = mix(colour, fog.rgb, amount * fog.a);
    return vec4<f32>(colour, 1.0);
}
