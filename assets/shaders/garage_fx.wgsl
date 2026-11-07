// The customize garage's light effects (src/customize.rs): the blue light beams
// over the platforms (blubeams_MATINST, a LightVolume_Base_MAT) and the glow over
// the focused class (ccGlow_MAT_INST, GlowSphereCheap_Parent_MAT). Both are added
// to what is behind them, strongest where the surface faces the camera and fading
// towards its outline, the way the game's volume materials fade at their edges.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view

// rgb: colour times strength (linear); a unused.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> fx_color: vec4<f32>;
// x: how quickly it fades towards the outline (a power of the facing ratio).
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> fx_params: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let v = normalize(view.world_position - in.world_position.xyz);
    let facing = abs(dot(n, v));
    let k = pow(facing, max(fx_params.x, 0.01));
    return vec4<f32>(fx_color.rgb * k, 0.0);
}
