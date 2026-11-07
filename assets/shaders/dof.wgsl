// The game's depth of field (Fall of Cybertron's UE3 DOF): the gather to the
// quarter-size filter buffer, the Gaussian across and down it, and the blend.
// See src/dof.rs for the game's shaders written out; one pass per shader def
// (GATHER, BLUR with BLUR_X or BLUR_Y, BLEND).

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct GameDof {
    focus: f32,
    inv_near_falloff: f32,
    inv_far_falloff: f32,
    exponent: f32,
    max_near: f32,
    max_far: f32,
    kernel: f32,
    near: f32,
    // The view's part of the picture (0 to 1 each way): left, top, width, height
    // (split screen's views draw into part of it).
    view: vec4<f32>,
}

// The filter buffer is this many times smaller each way (FilterDownsampleFactor).
const DOWNSAMPLE: i32 = 4;
// Where nothing was drawn (Bevy's depth 0), in metres.
const FAR: f32 = 100000.0;

#ifdef GATHER
@group(0) @binding(0) var picture: texture_2d<f32>;
#ifdef MULTISAMPLED
@group(0) @binding(1) var depth_texture: texture_depth_multisampled_2d;
#else
@group(0) @binding(1) var depth_texture: texture_depth_2d;
#endif
@group(0) @binding(2) var<uniform> dof: GameDof;
#endif

#ifdef BLUR
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> dof: GameDof;
#endif

#ifdef BLEND
@group(0) @binding(0) var picture: texture_2d<f32>;
#ifdef MULTISAMPLED
@group(0) @binding(1) var depth_texture: texture_depth_multisampled_2d;
#else
@group(0) @binding(1) var depth_texture: texture_depth_2d;
#endif
@group(0) @binding(2) var blurred: texture_2d<f32>;
@group(0) @binding(3) var blurred_sampler: sampler;
@group(0) @binding(4) var<uniform> dof: GameDof;
#endif

// How out of focus something `depth` metres away is (CalcUnfocusedPercent).
fn unfocused(depth: f32) -> f32 {
    let rel = depth - dof.focus;
    let far = rel >= 0.0;
    let inv = select(dof.inv_near_falloff, dof.inv_far_falloff, far);
    let most = select(dof.max_near, dof.max_far, far);
    let x = clamp(abs(rel) * inv, 0.0, 1.0);
    // (The game's shaders take log2 of at least 0.0001.)
    let p = exp2(log2(max(x, 0.0001)) * dof.exponent);
    return min(p, most);
}

#ifndef BLUR
// Distance from the camera plane in metres (Bevy's depth is near / distance).
fn view_depth(p: vec2<i32>) -> f32 {
    let size = vec2<i32>(textureDimensions(depth_texture));
    let d = textureLoad(depth_texture, clamp(p, vec2<i32>(0), size - vec2<i32>(1)), 0);
    if d <= 0.0 {
        return FAR;
    }
    return min(dof.near / d, FAR);
}
#endif

#ifdef GATHER
// Each filter texel: the mean of the 4 x 4 pixels it covers and of their depths,
// that depth's unfocused amount u: (u x colour, u).
@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let base = vec2<i32>(in.position.xy) * DOWNSAMPLE;
    let size = vec2<i32>(textureDimensions(picture));
    var colour = vec3<f32>(0.0);
    var depth = 0.0;
    for (var y = 0; y < DOWNSAMPLE; y++) {
        for (var x = 0; x < DOWNSAMPLE; x++) {
            let p = min(base + vec2<i32>(x, y), size - vec2<i32>(1));
            colour += textureLoad(picture, p, 0).rgb;
            depth += view_depth(p);
        }
    }
    let n = f32(DOWNSAMPLE * DOWNSAMPLE);
    let u = unfocused(depth / n);
    return vec4<f32>(colour / n * u, u);
}
#endif

#ifdef BLUR
// The Gaussian whose variance is its radius r (min(kernel / 4, 16) x the view's
// width / 1280 texels, held to 15), out to r texels each way, within the view.
@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let width = f32(size.x * DOWNSAMPLE) * dof.view.z;
    let lo = clamp(vec2<i32>(floor(dof.view.xy * vec2<f32>(size))), vec2<i32>(0), size - vec2<i32>(1));
    let hi = clamp(vec2<i32>(ceil((dof.view.xy + dof.view.zw) * vec2<f32>(size))) - vec2<i32>(1), lo, size - vec2<i32>(1));
    let r = clamp(min(dof.kernel / f32(DOWNSAMPLE), 16.0) * width / 1280.0, 1e-5, 15.0);
    let n = min(i32(ceil(r)), 15);
#ifdef BLUR_X
    let step = vec2<i32>(1, 0);
#else
    let step = vec2<i32>(0, 1);
#endif
    let p = vec2<i32>(in.position.xy);
    var sum = vec4<f32>(0.0);
    var weights = 0.0;
    for (var i = -n; i <= n; i++) {
        let w = exp(-f32(i * i) / (2.0 * r));
        let q = clamp(p + step * i, lo, hi);
        sum += textureLoad(source, q, 0) * w;
        weights += w;
    }
    return sum / weights;
}
#endif

#ifdef BLEND
// picture x f + (1 - f) x (B.rgb + picture x f) / (B.a + f), f = 1 - u here.
@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let p = vec2<i32>(in.position.xy);
    let here = textureLoad(picture, p, 0);
    let f = 1.0 - unfocused(view_depth(p));
    let b = textureSample(blurred, blurred_sampler, in.uv);
    let soft = (b.rgb + here.rgb * f) / max(b.a + f, 0.0001);
    return vec4<f32>(here.rgb * f + soft * (1.0 - f), here.a);
}
#endif
