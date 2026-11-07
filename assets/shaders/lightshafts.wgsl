// The game's light shafts (Fall of Cybertron's UE3 light shafts for a directional
// light): the downsample to the quarter-size filter buffer, the blur toward the
// sun and the apply to the picture. See src/lightshafts.rs for the game's shaders
// written out; one pass per shader def (DOWNSAMPLE, BLUR, APPLY).

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct LightShafts {
    // The sun's place on the screen: x in screen heights (u x aspect), y = v.
    origin: vec2<f32>,
    aspect: f32,
    // 1 / OcclusionDepthRange, per metre.
    inv_range: f32,
    // BloomTint (linear) and BloomThreshold.
    tint: vec4<f32>,
    bloom_scale: f32,
    darkness: f32,
    screen_blend: f32,
    distance_fade: f32,
    near: f32,
    // The view's part of the picture (0 to 1 each way): left, top, width, height.
    // The screen below is the view's (split screen's views draw into part of it).
    view: vec4<f32>,
}

// A place on the picture (0 to 1 each way) as a place on the view's screen.
fn on_view(uv: vec2<f32>) -> vec2<f32> {
    return (uv - shafts.view.xy) / shafts.view.zw;
}

// The filter buffer is this many times smaller each way.
const DOWNSAMPLE: i32 = 4;
// Where nothing was drawn (Bevy's depth 0), in metres.
const FAR: f32 = 100000.0;
// The blur's samples.
const SAMPLES: i32 = 48;
const LUMINANCE: vec3<f32> = vec3<f32>(0.3, 0.59, 0.11);

#ifdef DOWNSAMPLE
@group(0) @binding(0) var picture: texture_2d<f32>;
#ifdef MULTISAMPLED
@group(0) @binding(1) var depth_texture: texture_depth_multisampled_2d;
#else
@group(0) @binding(1) var depth_texture: texture_depth_2d;
#endif
@group(0) @binding(2) var<uniform> shafts: LightShafts;

// Distance from the camera plane in metres (Bevy's depth is near / distance).
fn view_depth(p: vec2<i32>) -> f32 {
    let size = vec2<i32>(textureDimensions(depth_texture));
    let d = textureLoad(depth_texture, clamp(p, vec2<i32>(0), size - vec2<i32>(1)), 0);
    if d <= 0.0 {
        return FAR;
    }
    return min(shafts.near / d, FAR);
}

// Each filter texel from the 4 x 4 pixels it covers: (bloom, occlusion).
@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let base = vec2<i32>(in.position.xy) * DOWNSAMPLE;
    let size = vec2<i32>(textureDimensions(picture));
    var colour = vec3<f32>(0.0);
    var occlusion = 0.0;
    var far_half = 0.0;
    let range = 1.0 / shafts.inv_range;
    for (var y = 0; y < DOWNSAMPLE; y++) {
        for (var x = 0; x < DOWNSAMPLE; x++) {
            let p = min(base + vec2<i32>(x, y), size - vec2<i32>(1));
            colour += textureLoad(picture, p, 0).rgb;
            let depth = view_depth(p);
            occlusion += saturate(depth * shafts.inv_range);
            far_half += saturate((depth - 0.5 * range) * shafts.inv_range);
        }
    }
    let n = f32(DOWNSAMPLE * DOWNSAMPLE);
    colour /= n;
    occlusion /= n;
    far_half /= n;
    // 1 at the screen's edges, 1/16 in its middle.
    let t = on_view(in.uv);
    let edge = 1.0 - 8.0 * t.x * (1.0 - t.x) * t.y * (1.0 - t.y);
    let edge4 = edge * edge * edge * edge;
    // The light over the threshold, from the far half of the occlusion range.
    let luminance = max(dot(colour, LUMINANCE), 6.10352e-5);
    let over = luminance - shafts.tint.w;
    var bloom = vec3<f32>(0.0);
    if over >= 0.0 {
        bloom = colour * shafts.bloom_scale / luminance * over * 2.0 * far_half;
    }
    // Only round the sun's place on the screen.
    let d = length(shafts.origin - vec2<f32>(t.x * shafts.aspect, t.y));
    let near_sun = 1.0 - saturate(d * 5.0);
    bloom *= (1.0 - edge4) * near_sun * near_sun * 0.25;
    return vec4<f32>(bloom, max(occlusion, edge4));
}
#endif

#ifdef BLUR
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> shafts: LightShafts;

// 48 samples from the texel toward the sun (all the way up to half a screen height
// off, sqrt(d / 2) of the way further out), weighted 2 - k / 24.
@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let scale = vec2<f32>(shafts.aspect, 1.0);
    let here = on_view(in.uv) * scale;
    let to_sun = shafts.origin - here;
    let d = length(to_sun);
    var reach = to_sun;
    if d > 1e-6 {
        reach = to_sun * min(1.0, sqrt(0.5 * d) / d);
    }
    // (Within the view's part of the filter buffer.)
    let size = vec2<f32>(textureDimensions(source));
    let lo = shafts.view.xy + 0.5 / size;
    let hi = shafts.view.xy + shafts.view.zw - 0.5 / size;
    var sum = vec4<f32>(0.0);
    for (var k = 0; k < SAMPLES; k++) {
        let w = 2.0 - f32(k) / 24.0;
        let on = (here + reach * (f32(k) / f32(SAMPLES))) / scale;
        let uv = clamp(shafts.view.xy + on * shafts.view.zw, lo, hi);
        let s = textureSampleLevel(source, source_sampler, uv, 0.0);
        sum += vec4<f32>(s.rgb * min(w, 4.0 * w * w), s.a * w);
    }
    return sum / f32(SAMPLES);
}
#endif

#ifdef APPLY
@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var blurred: texture_2d<f32>;
@group(0) @binding(2) var blurred_sampler: sampler;
@group(0) @binding(3) var<uniform> shafts: LightShafts;

// The picture darkened where the way to the sun is blocked (brighter where it is
// open), eased back two screen heights from the sun and as it leaves the screen,
// plus the bloom (less over what is already bright).
@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let here = textureLoad(picture, vec2<i32>(in.position.xy), 0);
    let b = textureSampleLevel(blurred, blurred_sampler, in.uv, 0.0);
    let t = on_view(in.uv);
    let d = length(shafts.origin - vec2<f32>(t.x * shafts.aspect, t.y));
    let fade = saturate(d * 0.5);
    let open = mix(shafts.darkness, 1.5 - 0.5 * shafts.darkness, b.a * b.a);
    let s = mix(open, 1.0, fade);
    let f3 = shafts.distance_fade * shafts.distance_fade * shafts.distance_fade;
    let scale = mix(s, 1.0, f3);
    let screen = saturate(exp2(-3.0 * dot(here.rgb, LUMINANCE)) * shafts.screen_blend);
    let bloom = b.rgb * shafts.tint.rgb * 4.0 * screen;
    return vec4<f32>(here.rgb * scale + bloom, here.a);
}
#endif
