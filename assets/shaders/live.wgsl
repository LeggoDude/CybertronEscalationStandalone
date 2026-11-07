// The live materials (src/live.rs): a standard material drawn as Bevy's own
// shader draws it (bevy_pbr's pbr.wgsl, forward), with what changes in it over
// time worked out here instead of in the material:
//
//   kind 1, a pulse: its glow clamp(sin(2 pi rate t) + 1, 0, 1.5) times as bright
//           (mapmotion.rs's pulse, the game's ENV_Emissive_MF)
//   kind 2, a flare: its glow 1 + amount (1 - |sin(2 pi rate t)|)^2 times as
//           bright (the consoles' and pods' outlines)
//   kind 3, tinted: its colour times its sprite's own, from the sprites' colours
//           (live.rs's LiveTints) by its mesh tag (fx.rs's fading sprites)
//
// t is the frame's time (Bevy's globals.time: the clock the game's systems read).

#import bevy_pbr::{
    pbr_types,
    pbr_functions::alpha_discard,
    pbr_fragment::pbr_input_from_standard_material,
    decal::clustered::apply_decals,
}
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}
#import bevy_pbr::mesh_view_bindings::globals
#import bevy_pbr::mesh_functions::get_tag

#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither;
#endif

#ifdef OIT_ENABLED
#import bevy_core_pipeline::oit::oit_draw
#endif // OIT_ENABLED

// Its kind, rate and amount (live.rs's Wave::uniform).
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> live_wave: vec4<f32>;
// The sprites' colours, each by its mesh tag (kind 3).
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<storage, read> live_tints: array<vec4<f32>>;

const TAU: f32 = 6.28318530718;

// How many times as bright its glow is t seconds in.
fn glow_at(t: f32) -> f32 {
    let kind = u32(live_wave.x + 0.5);
    let s = sin(TAU * live_wave.y * t);
    if (kind == 1u) {
        return clamp(s + 1.0, 0.0, 1.5);
    }
    if (kind == 2u) {
        let k = 1.0 - abs(s);
        return 1.0 + live_wave.z * k * k;
    }
    return 1.0;
}

@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in = vertex_output;

#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif

    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // Its colour times its sprite's (a slot past the colours' room: its own).
    if (u32(live_wave.x + 0.5) == 3u) {
        let at = get_tag(in.instance_index);
        if (at < arrayLength(&live_tints)) {
            pbr_input.material.base_color *= live_tints[at];
        }
    }
    // Its glow as bright as its wave has it now.
    let glow = glow_at(globals.time);
    pbr_input.material.emissive = vec4<f32>(
        pbr_input.material.emissive.rgb * glow,
        pbr_input.material.emissive.a,
    );

    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    apply_decals(&pbr_input);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

#ifdef OIT_ENABLED
    let alpha_mode = pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if alpha_mode != pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE {
        oit_draw(in.position, out.color);
        discard;
    }
#endif // OIT_ENABLED

    return out;
}
