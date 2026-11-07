// The Escalation purchase points' bases, drawn with the game's Pickup_Unlit_MAT
// (src/pickupbase.rs has its shading written out): unlit, lit only by a light of
// its own circling it, over the map's own textures and glow.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    mesh_view_bindings::{globals, view},
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::main_pass_post_lighting_processing,
}
#endif

struct PickupBase {
    // The highlight's colour (SpecularColor), and in w 1 with a cube map.
    specular: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> pickup_base: PickupBase;
// The level's cube map, in Unreal's axes as the game samples it.
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var reflection_cube: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var reflection_sampler: sampler;
// How the gloss mask is kept in the metal channel: x + y x green.
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var<uniform> pickup_gloss: vec4<f32>;

const PI: f32 = 3.141592653589793;
const LUMINANCE: vec3<f32> = vec3<f32>(0.3, 0.59, 0.11);
// The light's turn (radians a second), the highlight's power at full gloss
// (Roughness Power) and the gloss mask's offset (Roughness Offset).
const LIGHT_SPEED: f32 = 1.5;
const ROUGHNESS_POWER: f32 = 100.0;
const ROUGHNESS_OFFSET: f32 = 0.12;

fn srgb_to_linear(x: f32) -> f32 {
    if x <= 0.04045 {
        return x / 12.92;
    }
    return pow((x + 0.055) / 1.055, 2.4);
}

// Bevy's axes to Unreal's.
fn ue_dir(v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(v.x, v.z, v.y);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    // (Bevy's normal mapping can give a few texels no normal at all: NaN, where
    // the two-channel normal map's texel lies past the unit circle or the tangents
    // fall apart. Bevy's own lighting clamps it away; this shading would carry it
    // into the picture, and the depth of field would spread it into black squares.
    // There, the face's own normal.)
    var n = pbr_input.N;
    let n_len2 = dot(n, n);
    if !(n_len2 > 0.5 && n_len2 < 2.0) {
        n = normalize(in.world_normal);
    }
    let v = pbr_input.V;
    // The light: Unreal's (cos t, sin t, 1), in Bevy's axes.
    let t = globals.time * LIGHT_SPEED;
    let l = normalize(vec3<f32>(cos(t), 1.0, sin(t)));
    // (The texture sets keep the gloss mask, sampled as sRGB as the game does, in
    // their metal: 0.2 + 0.5 x it in the maps', build_map.py.)
    let green = srgb_to_linear(saturate((pbr_input.material.metallic - pickup_gloss.x) / pickup_gloss.y));
    let gloss = green + ROUGHNESS_OFFSET;
    let specular = pickup_base.specular.rgb * gloss;
    let colour = pbr_input.material.base_color.rgb;
    let wrap = saturate(pow(max(abs((dot(n, l) + 1.0) * 0.5 * 1.127), 1e-6), 2.0));
    let diffuse = wrap * colour * (1.0 - saturate(dot(LUMINANCE, specular))) * 0.5;
    let h = normalize(l + v);
    let n_dot_h = saturate(abs(dot(h, n)));
    let power = max(ROUGHNESS_POWER * gloss, 1.0);
    let highlight = pow(max(n_dot_h, 1e-6), power) * specular * ((power + 8.0) / (8.0 * PI));
    var cube = vec3<f32>(1.0);
    if pickup_base.specular.w > 0.5 {
        cube = textureSampleLevel(reflection_cube, reflection_sampler, ue_dir(reflect(-v, n)), 0.0).rgb;
    }
    let reflection = cube * gloss * specular * n_dot_h;
    // Its lights, the map's glow as Bevy's own emissive would give it.
    let emissive = pbr_input.material.emissive;
    let glow = emissive.rgb * mix(1.0, view.exposure, emissive.a);
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(
        pbr_input,
        vec4<f32>(max(glow + diffuse + highlight + reflection, vec3<f32>(0.0)), 1.0),
    );
#endif
    return out;
}
