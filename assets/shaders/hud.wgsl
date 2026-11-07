// The HUD's material (src/hud.rs): a texture through a Flash colour transform.
//
// Flash works out colour transforms (colour * mul + add) on sRGB values, so the
// sample is taken back to sRGB, transformed, and returned to linear for the
// framebuffer. Output is premultiplied; additive layers (params.x = 1) write no
// alpha, so they add to what is behind them. Text with marked-up runs carries
// each character's colour (sRGB) per vertex, its piece drawn white. Each piece's
// transform is the HUD's colours' (hud.rs's HudColors), by its mesh tag.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_functions::get_tag

@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> hud_params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var hud_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var hud_sampler: sampler;
// The pieces' colour transforms: by mesh tag, a piece's mul then its add.
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var<storage, read> hud_colors: array<vec4<f32>>;

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((max(c, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    // (A piece past the colours' room is drawn as its texture is.)
    var hud_mul = vec4<f32>(1.0);
    var hud_add = vec4<f32>(0.0);
    let at = get_tag(mesh.instance_index) * 2u;
    if (at + 1u < arrayLength(&hud_colors)) {
        hud_mul = hud_colors[at];
        hud_add = hud_colors[at + 1u];
    }
    let tex = textureSample(hud_texture, hud_sampler, mesh.uv);
    var tint = vec3<f32>(1.0);
#ifdef VERTEX_COLORS
    tint = mesh.color.rgb;
#endif
    let rgb = clamp(to_srgb(tex.rgb) * tint * hud_mul.rgb + hud_add.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    let a = clamp(tex.a * hud_mul.a + hud_add.a, 0.0, 1.0);
    return vec4<f32>(to_linear(rgb) * a, a * (1.0 - hud_params.x));
}
