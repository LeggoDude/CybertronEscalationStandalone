// The pause menu's hologram view of the world: UI_Materials_p.Hologram_Post_MAT
// (with PauseMenu_Post_MAT_INST's parameters) and the pause chain's depth of field.
// See src/hologram.rs for the material written out.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
#ifdef MULTISAMPLED
@group(0) @binding(2) var depth_texture: texture_depth_multisampled_2d;
#else
@group(0) @binding(2) var depth_texture: texture_depth_2d;
#endif

struct Hologram {
    world_from_clip: mat4x4<f32>,
    location: vec3<f32>,
    radius_in: f32,
    radius_out: f32,
    scene: f32,
    enabled: f32,
    near: f32,
}
@group(0) @binding(3) var<uniform> hologram: Hologram;

const PI: f32 = 3.14159265;
// The material's parameters (its defaults; the instance only sets the sweep).
const VOLUME_COLOR: vec3<f32> = vec3<f32>(0.0, 0.3, 0.6);
const BASE_COLOR: vec3<f32> = vec3<f32>(0.0, 0.01, 0.02);
const EDGE_COLOR: vec3<f32> = vec3<f32>(0.5, 3.0, 2.0);
const FEATHER: f32 = 150.0;
// The edge test's offsets (a fraction of the screen) and depth unit (500 UU = 5 m).
const EDGE_OFFSET: f32 = 0.001;
const DEPTH_UNIT: f32 = 5.0;
// Depth of field (the game's DOF defaults with the pause chain's settings): in focus
// to 8 m, fully blurred 10 m further (squared falloff), a 32-pixel kernel at 720
// lines.
const FOCUS: f32 = 8.0;
const FAR_FALLOFF: f32 = 10.0;
const KERNEL_720: f32 = 16.0;
// Nothing is further than this (the sky), in metres.
const FAR: f32 = 1000.0;

fn raw_depth(uv: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(depth_texture));
    let p = clamp(vec2<i32>(uv * size), vec2<i32>(0), vec2<i32>(size) - vec2<i32>(1));
    return textureLoad(depth_texture, p, 0);
}

// Distance from the camera plane in metres (Bevy's depth is near / distance).
fn view_depth(uv: vec2<f32>) -> f32 {
    let d = raw_depth(uv);
    if d <= 0.0 {
        return FAR;
    }
    return min(hologram.near / d, FAR);
}

// The glow: depth jumps between three nearby points, in the material's units.
fn volume(uv: vec2<f32>) -> vec3<f32> {
    let za = clamp(view_depth(uv + vec2<f32>(-EDGE_OFFSET, -EDGE_OFFSET)) / DEPTH_UNIT, 0.0, 100.0);
    let zb = clamp(view_depth(uv + vec2<f32>(EDGE_OFFSET, -EDGE_OFFSET)) / DEPTH_UNIT, 0.0, 100.0);
    let zc = clamp(view_depth(uv + vec2<f32>(-EDGE_OFFSET, EDGE_OFFSET)) / DEPTH_UNIT, 0.0, 100.0);
    let edge = clamp(abs(za - zb) + abs(za - zc), 0.0, 1.0);
    return edge * VOLUME_COLOR + BASE_COLOR;
}

// The material's "1 / (1 + 2.72^(-(x - .9) * feather))".
fn ring(radius: f32, dist: f32) -> f32 {
    let x = clamp(radius / max(dist, 0.001), 0.0, 1.0);
    return 1.0 / (1.0 + pow(2.72, -(x - 0.9) * FEATHER));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let scene = textureSample(screen_texture, screen_sampler, uv).rgb;

    // The glow, blurred with distance (12 taps on a disc).
    let depth = view_depth(uv);
    var glow = volume(uv);
    let blur = pow(clamp((depth - FOCUS) / FAR_FALLOFF, 0.0, 1.0), 2.0);
    if blur > 0.0 {
        let size = vec2<f32>(textureDimensions(screen_texture));
        let radius = KERNEL_720 * size.y / 720.0 / size;
        var sum = glow;
        for (var i = 0; i < 12; i++) {
            let a = f32(i) * 2.39996;
            let r = sqrt((f32(i) + 0.5) / 12.0);
            sum += volume(uv + vec2<f32>(cos(a), sin(a)) * r * radius);
        }
        glow = mix(glow, sum / 13.0, blur);
    }
    let holo = abs(1.0 - sin(uv.y * 1440.0)) * (glow + 0.01) + glow;

    // Where this pixel is in the world, for the sweep.
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let raw = max(raw_depth(uv), hologram.near / FAR);
    let world = hologram.world_from_clip * vec4<f32>(ndc, raw, 1.0);
    let dist = length(hologram.location - world.xyz / world.w);

    let s_in = ring(hologram.radius_in, dist);
    let s_out = ring(hologram.radius_out, dist);
    let seen = scene * hologram.scene;
    let sweep_in = mix(seen + EDGE_COLOR * (1.0 - cos(PI * s_in)), holo, s_in);
    let colour = mix(sweep_in, seen + EDGE_COLOR * (1.0 - cos(2.0 * PI * s_out)), s_out);
    return vec4<f32>(colour, 1.0);
}
