// The characters' decals (src/decals.rs), as the game's TR_Decals1_Mat_INST draws
// them (TR_FactionLogo_Mat's graph): unlit and see-through. The sheet's picture
// (grey: Decals_FullSheet) is lit in the character's EnergonColor times
// LogoIntensity, pulsing as the energon does: fractalMaps panned over the decal
// (EnergonPanUVTile times its texture coordinates, moving at 0.01, -0.02 a second),
// its value and the time (0.15 a second) through two sines, times
// energonIntensity, to the power of energonContrast, so bright flashes run through
// it now and then; and it is as opaque as 15 times the picture. The game's values
// are in its "1.0 = white" units, as the demo's lighting is (lighting.rs), so the
// camera's exposure is left off it, as off the paint's glow (paint.wgsl). The fog
// covers it as it covers the effects (levelfx.wgsl's fx_fog_at), and the player's
// own robot's dither (main.rs's fade_near_camera) takes it too.

#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_view_bindings::{globals, view, fog},
    pbr_functions,
}

const TAU: f32 = 6.283185307179586;

struct DecalValues {
    // The energon's colour (linear).
    energon: vec4<f32>,
    // EnergonPanUVTile, energonIntensity, energonContrast, LogoIntensity.
    params: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> decal: DecalValues;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var sheet: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var sheet_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var noise: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var noise_sampler: sampler;

// How much of the camera's fog covers what is `d` metres away (Bevy's fog
// settings, as levelfx.wgsl's fx_fog_at takes them).
fn fog_at(d: f32) -> f32 {
    var amount = 0.0;
    if (fog.mode == 1u) {
        amount = 1.0 - clamp((fog.be.y - d) / max(fog.be.y - fog.be.x, 0.0001), 0.0, 1.0);
    } else if (fog.mode == 2u || fog.mode == 4u) {
        amount = 1.0 - exp(-d * fog.be.x);
    } else if (fog.mode == 3u) {
        let x = d * fog.be.x;
        amount = 1.0 - exp(-x * x);
    }
    return amount * fog.base_color.a;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
#ifdef VISIBILITY_RANGE_DITHER
    pbr_functions::visibility_range_dither(in.position, in.visibility_range_dither);
#endif
#ifdef VERTEX_UVS_A
    let uv = in.uv;
#else
    let uv = vec2<f32>(0.0);
#endif
    let picture = textureSample(sheet, sheet_sampler, uv).r;
    let t = globals.time;
    let n = textureSample(noise, noise_sampler, uv * decal.params.x + t * vec2<f32>(0.01, -0.02)).r;
    let wave = sin(sin((n + t * 0.15) * TAU) * TAU);
    let pulse = pow(max((wave + 1.0) * decal.params.y, 0.0), decal.params.z);
    let color = picture * decal.energon.rgb * ((pulse + 1.0) * decal.params.w);
    let a = saturate(picture * 15.0);
    let fogged = mix(color, fog.base_color.rgb, fog_at(length(in.world_position.xyz - view.world_position)));
    return vec4<f32>(fogged * a, a);
}
