// The customize screen's material (src/flash.rs): a texture, or part of one,
// through a Flash colour transform.
//
// (params.x = 2: the multiply blend mode. The output is then the factor what is
// behind is multiplied by; the pipeline's blend does the multiplying.)
//
// Flash works out colour transforms (colour * mul + add) on sRGB values, so the
// sample is taken back to sRGB, transformed, and returned to linear for the
// framebuffer. Output is premultiplied. Text comes from a texture Bevy's UI drew
// into, whose colours are premultiplied (params.y = 1): they are divided back out
// first. A text field's glow filter (params.z > 0) goes under its letters: their
// alpha box-blurred params.z texels either way, params.w times as strong, in the
// glow's colour.

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> cx_mul: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> cx_add: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> params: vec4<f32>;
// The part of the texture shown: uv = xy + mesh uv * zw.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> uv_rect: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var flash_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var flash_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var<uniform> glow: vec4<f32>;

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
    let uv = uv_rect.xy + mesh.uv * uv_rect.zw;
    var tex = textureSample(flash_texture, flash_sampler, uv);
    if (params.y > 0.5) {
        tex = vec4<f32>(tex.rgb / max(tex.a, 0.0001), tex.a);
    }
    let rgb = clamp(to_srgb(tex.rgb) * cx_mul.rgb + cx_add.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    let a = clamp(tex.a * cx_mul.a + cx_add.a, 0.0, 1.0);
    if (params.x > 1.5) {
        // The multiply blend mode: what is behind times the colour, as much as the
        // alpha says (worked out in sRGB, as Flash does; the pipeline multiplies).
        return vec4<f32>(to_linear(mix(vec3<f32>(1.0), rgb, a)), 0.0);
    }
    var out = vec4<f32>(to_linear(rgb) * a, a);
    if (params.z > 0.0) {
        // 7 x 7 taps over the box, kept inside the line's own part of the atlas.
        let texel = 1.0 / vec2<f32>(textureDimensions(flash_texture));
        let lo = uv_rect.xy + texel * 0.5;
        let hi = uv_rect.xy + uv_rect.zw - texel * 0.5;
        let step = params.z / 3.0;
        var sum = 0.0;
        for (var j = -3; j <= 3; j++) {
            for (var i = -3; i <= 3; i++) {
                let o = vec2<f32>(f32(i), f32(j)) * step * texel;
                sum += textureSampleLevel(flash_texture, flash_sampler, clamp(uv + o, lo, hi), 0.0).a;
            }
        }
        let ga = clamp(sum / 49.0 * params.w, 0.0, 1.0) * clamp(cx_mul.a + cx_add.a, 0.0, 1.0);
        out = out + vec4<f32>(to_linear(glow.rgb) * ga, ga) * (1.0 - out.a);
    }
    return out;
}
