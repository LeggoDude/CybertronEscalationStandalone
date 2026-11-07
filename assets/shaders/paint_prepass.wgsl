// The characters' paint (paint.wgsl) in the depth prepass and the shadows: a dead
// robot's dissolve takes out of them what paint.wgsl takes out of the picture (once
// the robot is drawn no more, only its overlay is left: its ragged front out from
// the middle of the body, burning out), so what is behind shows through and its
// shadow goes too; and the cloak's overlay (src/cloak.rs, masked) is only where it
// is drawn, so the cloak shows where it isn't. (A dissolving robot's materials and
// the overlay's are masked, which runs this; for the others it is the standard
// prepass's output.)

#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_prepass_functions,
}
#import bevy_render::globals::Globals

#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither
#endif

struct PaintColors {
    primary: vec4<f32>,
    secondary: vec4<f32>,
    energon: vec4<f32>,
    res_in: vec4<f32>,
    deres: vec4<f32>,
    deres_fx: vec4<f32>,
    deres_fresnel: vec4<f32>,
    deres_lead: vec4<f32>,
    deres_burn: vec4<f32>,
    deres_energon: vec4<f32>,
    gloss: vec4<f32>,
    shade: vec4<f32>,
    coefficient: vec4<f32>,
    cloak: vec4<f32>,
    cloak_fx: vec4<f32>,
}

// (The prepass's view bindings: the globals second.)
@group(0) @binding(1) var<uniform> globals: Globals;
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> paint: PaintColors;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var energon_noise: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var energon_noise_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var cloak_alphas: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var cloak_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var cloak_pulse: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var cloak_pulse_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(113) var defrag_arcs: texture_2d<f32>;

// (paint.wgsl's.)
const CLOAK_TILING: f32 = 3.0;
const CLOAK_NOISE_PAN: vec2<f32> = vec2<f32>(0.15, 0.2);
const CLOAK_CLIP: f32 = 0.3333;

fn uv_of(in: VertexOutput) -> vec2<f32> {
#ifdef VERTEX_UVS_A
    return in.uv;
#else
    return vec2<f32>(0.0);
#endif
}

// An overlay's opacity mask (paint.wgsl's overlay_glow) at `near` with its k.
fn overlay_mask(near: f32, k: f32, uv: vec2<f32>, tiling: f32) -> f32 {
    let pulse = textureSample(cloak_pulse, cloak_pulse_sampler, vec2<f32>(near - k));
    let alphas = textureSample(cloak_alphas, cloak_sampler, uv * tiling).g;
    let past = (k - 0.5) * 2.0;
    let burn = floor(k * 2.0) * (alphas * past + pow(max(past, 0.0), 4.0));
    return clamp(pulse.g * (1.0 - burn * 8.0), 0.0, 1.0);
}

// A dissolving robot (paint.wgsl's defrag_overlay): gone where its overlay isn't,
// once it is drawn no more.
fn deres_discard(in: VertexOutput) {
    if paint.deres.w < 0.0 {
        return;
    }
    let uv = uv_of(in);
    let near = saturate(distance(in.world_position.xyz, paint.deres.xyz) / max(paint.deres.w, 1e-3)) * 0.75;
    let at = uv * paint.deres_energon.w + globals.time * CLOAK_NOISE_PAN;
    let fractal = textureSample(energon_noise, energon_noise_sampler, at).g;
    let arcs = textureSample(defrag_arcs, cloak_sampler, at).g;
    let noise = select(fractal, arcs, paint.deres_fx.w > 0.5);
    let k = pow(max(noise * paint.deres_fx.z + paint.deres_fx.x, 0.0), 0.75);
    let mask = overlay_mask(near, k, uv, paint.deres_energon.w);
    if paint.deres_fx.x > paint.deres_fx.y && mask < CLOAK_CLIP {
        discard;
    }
}

// The cloak's overlay's mask (paint.wgsl's cloak_overlay): gone where it isn't drawn.
fn cloak_discard(in: VertexOutput) {
    if paint.cloak.w < 0.0 || paint.cloak_fx.x > 1.5 {
        return;
    }
    let uv = uv_of(in);
    let near = saturate(distance(in.world_position.xyz, paint.cloak.xyz) / max(paint.cloak.w, 1e-3)) * 0.75;
    let noise = textureSample(energon_noise, energon_noise_sampler, uv * CLOAK_TILING + globals.time * CLOAK_NOISE_PAN).g;
    let k = pow(max(noise + paint.cloak_fx.y, 0.0), 0.75);
    if overlay_mask(near, k, uv, CLOAK_TILING) < CLOAK_CLIP {
        discard;
    }
}

#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    cloak_discard(in);
    deres_discard(in);
    var out: FragmentOutput;
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
#ifdef NORMAL_PREPASS
    out.normal = vec4(in.world_normal * 0.5 + vec3(0.5), 1.0);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector = pbr_prepass_functions::calculate_motion_vector(in.world_position, in.previous_world_position);
#endif
    return out;
}
#else
@fragment
fn fragment(in: VertexOutput) {
    cloak_discard(in);
    deres_discard(in);
}
#endif
