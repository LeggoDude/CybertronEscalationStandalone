// The maps' effects (src/levelfx.rs): light shafts, glow spheres, smoke cards and
// the like (meshes), particles and lens flares, each drawn with its game material.
//
// tools/levelfx.py writes assets/shaders/levelfx.wgsl from this file, putting the
// maps' materials where MATERIALS is below: each the game's material graph as
// `fn fx_material(input: FxIn) -> FxOut` (tools/ue3/fxmat.py), compiled in by its
// shader def (FXM_<MAP>_<n>, which the material's pipeline sets).
//
// The materials work in Unreal's axes (X forward, Y right, Z up) and units
// (centimetres), so what they read (FxIn) is given in those: Bevy's x, y, z are
// Unreal's x, z, y, and a metre is 100 units. Their colours are the game's ("1.0 =
// white"), which is what the HDR target holds here too.
//
// Shader defs (levelfx.rs): FX_ADDITIVE, FX_MODULATE, FX_TRANSLUCENT or FX_MASKED,
// the blend (FX_MODULATE_DARK or FX_MODULATE_BRIGHT with FX_MODULATE: one part of a
// modulating material, levelfx.rs's ModPart); FX_LIT, lit by the light in FxParams; FX_NOFOG, outside the
// camera's fog (the far backdrop); FX_SKYFOG, a far piece of the sky seen through the fog its vertex
// colours carry instead (tools/build_map.py's SKY_FX_DISTANCE: the light the fog adds, and how much
// of its own gets through); FX_CUBE, an effect mesh reflecting the map's cube map; FX_SPRITE, a particle or lens
// flare mesh (with its extra vertex data), FX_FLARE for a lens flare's,
// FX_MESH_PARTICLE for mesh particles' (the vertex colour is the particle's: the
// materials' MeshEmitterVertexColor). FX_SURFACE: not an effect but a surface of the
// level, opaque and lit by its light map as the game lights it (levelfx.rs's
// SurfaceMaterial: the Customize garage's). FX_DISTORT: the material shifts the
// picture behind it (its Distortion: heat haze, shock rings; see fx_distorted),
// drawn by a part of its own, FX_DISTORT_PART (a modulating one's bright part moves
// what it adds as far); FX_OPACITY_CLIP, the rest of a distorting material, clipped
// where its opacity is under a 255th. FX_ALPHA_RESTORE: the picture's alpha put back
// once all is drawn (levelfx.rs's AlphaRestore). FX_PULLED: particles, trails,
// decals and lens flares, whose vertices aren't in their mesh but in the frame's
// effect data (levelfx.rs's FxWords, fx_words below): their mesh only numbers them.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_normal_local_to_world, mesh_tangent_local_to_world, get_tag}
#import bevy_pbr::mesh_view_bindings::{view, globals, fog, view_transmission_texture, view_transmission_sampler}
#import bevy_pbr::view_transformations::{depth_ndc_to_view_z, position_world_to_view}
#ifdef DEPTH_PREPASS
#import bevy_pbr::mesh_view_bindings::depth_prepass_texture
#endif
#ifdef FX_SURFACE
#import bevy_pbr::mesh_bindings::mesh
#ifdef LIGHTMAP
#import bevy_pbr::lightmap::lightmap
#endif
#endif

// ---------------------------------------------------------------------------
// What the materials read and give
// ---------------------------------------------------------------------------

struct FxIn {
    // Texture coordinates (a particle's sub-image and its next one).
    uv0: vec2<f32>,
    uv1: vec2<f32>,
    // Vertex colour: a particle's or a lens flare element's colour (a mesh's own).
    color: vec4<f32>,
    // A mesh particle's colour (white).
    mesh_color: vec4<f32>,
    // A particle's dynamic parameters (ones elsewhere).
    dparam: vec4<f32>,
    // How far between uv0's sub-image and uv1's (ParticleSubUV).
    subuv_lerp: f32,
    // Seconds.
    time: f32,
    // The pixel's depth from the camera, and the scene's behind it.
    depth: f32,
    scene_depth: f32,
    world_pos: vec3<f32>,
    object_pos: vec3<f32>,
    // The surface's tangent frame (x along U, y along V, z out) and the pixel's
    // normal (after the material's normal map).
    normal: vec3<f32>,
    tangent: vec3<f32>,
    bitangent: vec3<f32>,
    pixel_normal: vec3<f32>,
    // Towards the camera (Unreal's CameraVector), in the world and in the tangent
    // frame. (Not "view": naga_oil would take that for Bevy's imported view.)
    camera_vector: vec3<f32>,
    camera_vector_ts: vec3<f32>,
    // The camera's axes.
    view_right: vec3<f32>,
    view_up: vec3<f32>,
    view_forward: vec3<f32>,
    // Unreal's ScreenPosition: clip x, y (times w), depth past the near plane, w.
    clip: vec4<f32>,
    // The pixel on screen, 0-1 from the top left.
    screen_uv: vec2<f32>,
    // Lens flares: intensity, occlusion, radial distance, ray distance; the
    // source's distance.
    flare: vec4<f32>,
    flare_src: f32,
}

struct FxOut {
    emissive: vec3<f32>,
    diffuse: vec3<f32>,
    opacity: f32,
    opacity_mask: f32,
    two_sided_mask: f32,
    specular: vec3<f32>,
    // The specular highlight's power (Unreal's SpecularPower; High Moon's Roughness).
    spec_power: f32,
    // What the level's reflection cube map is multiplied by (High Moon's
    // ReflectionColor, its ReflectionCubeMapSample taken as 1: the surface shader
    // samples the cube map).
    reflection: vec3<f32>,
    // Tangent space.
    normal: vec3<f32>,
    // How far it shifts the picture behind it, in pixels (UE3's Distortion).
    distortion: vec2<f32>,
}

// The light on lit materials (game units; directions in Unreal's axes).
struct FxParams {
    // rgb: light from all around.
    ambient: vec4<f32>,
    // rgb: the sun's light; sun_dir.xyz: towards the sun.
    sun: vec4<f32>,
    sun_dir: vec4<f32>,
    // x: the opacity mask's clip value.
    misc: vec4<f32>,
    // rgb: the level's lights gathered into one; light_dir.xyz: towards it.
    light: vec4<f32>,
    light_dir: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> fx_params: FxParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var fx_tex0: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var fx_samp0: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var fx_tex1: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var fx_samp1: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var fx_tex2: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var fx_samp2: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var fx_tex3: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var fx_samp3: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(9) var fx_tex4: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var fx_samp4: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(11) var fx_tex5: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(12) var fx_samp5: sampler;
#ifdef FX_SURFACE
// A surface's light map's direction page (tools/build_level.py's <page>_dir.png):
// how its light is shared between the light map's three basis directions.
@group(#{MATERIAL_BIND_GROUP}) @binding(13) var fx_dir_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(14) var fx_dir_samp: sampler;
// The level's captured cube map (its StaticSceneCaptureCubeMapActor's), in
// Unreal's axes, as the game samples it.
@group(#{MATERIAL_BIND_GROUP}) @binding(15) var fx_cube_tex: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(16) var fx_cube_samp: sampler;
#else ifdef FX_CUBE
// (The material's own cube map (its cube samples'), or for an effect mesh that
// reflects the map's cube map, the one its characters reflect: the level's captured
// one or the game's default.)
@group(#{MATERIAL_BIND_GROUP}) @binding(15) var fx_cube_tex: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(16) var fx_cube_samp: sampler;
#endif
#ifdef FX_PULLED
// The frame's effect vertex data (levelfx.rs's FxWords), one buffer for every
// emitter and flare mesh: first FX_HEADER_WORDS words for each mesh tag (each
// entity's slot), where its vertices start, where its indices start and how many
// indices it has; then each entity's indices and vertices, FX_VERTEX_WORDS words to
// a vertex (place, normal, both UVs, tangent, colour, parameters, extra: f32 bits),
// in its mesh's frame as a mesh's would be.
@group(#{MATERIAL_BIND_GROUP}) @binding(17) var<storage, read> fx_words: array<u32>;
#endif

// ---------------------------------------------------------------------------
// What the materials call
// ---------------------------------------------------------------------------

fn fx_out_default() -> FxOut {
    var o: FxOut;
    o.emissive = vec3<f32>(0.0);
    o.diffuse = vec3<f32>(0.0);
    o.opacity = 1.0;
    o.opacity_mask = 1.0;
    o.two_sided_mask = 0.0;
    o.specular = vec3<f32>(0.0);
    // (UE3's own when a material doesn't say.)
    o.spec_power = 15.0;
    o.reflection = vec3<f32>(0.0);
    o.normal = vec3<f32>(0.0, 0.0, 1.0);
    o.distortion = vec2<f32>(0.0);
    return o;
}

// A texture set's normal (High Moon's TextureSetSampleParameter): X and Y as its
// textures keep them (already -1 to 1), Z made up.
fn fx_texset_normal(x: f32, y: f32) -> vec3<f32> {
    return vec3<f32>(x, y, sqrt(saturate(1.0 - x * x - y * y)));
}

fn fx_sample0(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(fx_tex0, fx_samp0, uv);
}

fn fx_sample1(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(fx_tex1, fx_samp1, uv);
}

fn fx_sample2(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(fx_tex2, fx_samp2, uv);
}

fn fx_sample3(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(fx_tex3, fx_samp3, uv);
}

fn fx_sample4(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(fx_tex4, fx_samp4, uv);
}

fn fx_sample5(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(fx_tex5, fx_samp5, uv);
}

fn fx_normalize2(v: vec2<f32>) -> vec2<f32> {
    return v * inverseSqrt(max(dot(v, v), 1e-16));
}

fn fx_normalize3(v: vec3<f32>) -> vec3<f32> {
    return v * inverseSqrt(max(dot(v, v), 1e-16));
}

fn fx_normalize4(v: vec4<f32>) -> vec4<f32> {
    return v * inverseSqrt(max(dot(v, v), 1e-16));
}

// Unreal's Rotator: uv turned by `angle` radians about `center`.
fn fx_rotate(uv: vec2<f32>, angle: f32, center: vec2<f32>) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    let d = uv - center;
    return vec2<f32>(d.x * c - d.y * s, d.x * s + d.y * c) + center;
}

fn fx_tangent_to_world(i: FxIn, v: vec3<f32>) -> vec3<f32> {
    return i.tangent * v.x + i.bitangent * v.y + i.normal * v.z;
}

fn fx_world_to_tangent(i: FxIn, v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(v, i.tangent), dot(v, i.bitangent), dot(v, i.normal));
}

fn fx_world_to_view(i: FxIn, v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(v, i.view_right), dot(v, i.view_up), dot(v, i.view_forward));
}

fn fx_reflect_ws(i: FxIn) -> vec3<f32> {
    return reflect(-i.camera_vector, i.pixel_normal);
}

fn fx_reflect_ts(i: FxIn) -> vec3<f32> {
    return fx_world_to_tangent(i, fx_reflect_ws(i));
}

// Unreal's DepthBiasedAlpha: alpha faded out as the scene behind comes within
// (1 - bias) * scale of the pixel.
fn fx_depth_biased_alpha(i: FxIn, alpha: f32, bias: f32, scale: f32) -> f32 {
    let fade = max((1.0 - bias) * scale, 0.001);
    return alpha * saturate((i.scene_depth - i.depth) / fade);
}

// The scene behind (SceneTexture): the picture as the opaque pass left it.
// A cube map sample (tools/ue3/fxmat.py's TextureSampleParameterCube): the material's
// own cube map (FX_CUBE) in direction `dir`, in Unreal's axes; a mid grey for an
// effect that has none (as the levels' captured ones average out).
fn fx_cube_sample(dir: vec3<f32>) -> vec4<f32> {
#ifdef FX_CUBE
    return textureSample(fx_cube_tex, fx_cube_samp, dir);
#else
    return vec4<f32>(0.5, 0.5, 0.5, 1.0);
#endif
}

// A ReflectionCubeMapSample (tools/ue3/fxmat.py): with FX_CUBE (an effect mesh of a
// map that reflects it: Oblivion's tower window) the map's cube map in the pixel's
// reflection, sharp; otherwise 1 (a surface's shader samples the cube itself, as
// much as o.reflection says; the weapons' effects have none).
fn fx_reflection_sample(i: FxIn) -> vec4<f32> {
#ifdef FX_CUBE
    let r = reflect(-i.camera_vector, i.pixel_normal);
    return vec4<f32>(textureSampleLevel(fx_cube_tex, fx_cube_samp, r, 0.0).rgb, 1.0);
#else
    return vec4<f32>(1.0);
#endif
}

// High Moon's particle G-buffers (SceneTex_GBuffer1 and 2), which the game's lit
// smoke is drawn into and its fire reads to hide behind it: the demo has none, so
// a read finds one empty, no smoke's cover (R) and its depth (G, over 65536 uu) as
// far as it goes. OccludedHeat_MAT's fire shows whole.
fn fx_empty_gbuffer() -> vec4<f32> {
    return vec4<f32>(0.0, 1.0, 0.0, 0.0);
}

fn fx_scene_color(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(view_transmission_texture, view_transmission_sampler, uv, 0.0);
}

// A distorting material's shift (its Distortion) as the game's buffer keeps it. UE3
// draws the shifts of everything distorting into a buffer and then moves the
// picture by their sum; Fall of Cybertron's buffer holds four times a shift over
// 255 in 8 bits (its DistortAccumulate shaders: up to 63.75 each way, those under a
// twelfth left out), and its DistortApply shader moves the picture by a quarter of
// what it holds: a shift of 1 is a 255th of the screen, V the other way (as the
// Cloak's, paint.wgsl's cloak_glass).
fn fx_shift(shift: vec2<f32>) -> vec2<f32> {
    let d = clamp(shift, vec2<f32>(-63.75), vec2<f32>(63.75));
    if (dot(d, d) * 16.0 < 0.1) {
        return vec2<f32>(0.0);
    }
    return d;
}

// Where shift `d` (fx_shift's) takes the picture from, for the place `uv`.
fn fx_moved_uv(d: vec2<f32>, uv: vec2<f32>) -> vec2<f32> {
    return uv + vec2<f32>(d.x, -d.y) / 255.0;
}

// What a distorting material's shift does to the picture behind it here: the
// scene `shift` away less the scene here, to be added to the picture (by its own
// part: FX_DISTORT_PART).
fn fx_distorted(shift: vec2<f32>, uv: vec2<f32>) -> vec3<f32> {
    let d = fx_shift(shift);
    if (all(d == vec2<f32>(0.0))) {
        return vec3<f32>(0.0);
    }
    return fx_scene_color(fx_moved_uv(d, uv)).rgb - fx_scene_color(uv).rgb;
}

// How long a shift uses up the place's distortion (FX_DISTORT_PART's alpha): a
// 255th of the screen.
const FX_DISTORT_FULL: f32 = 1.0;

// ---------------------------------------------------------------------------
// The maps' materials
// ---------------------------------------------------------------------------

//{{MATERIALS}}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    // (FX_PULLED: its number, in x.)
    @location(0) position: vec3<f32>,
#ifndef FX_PULLED
    @location(1) normal: vec3<f32>,
    @location(2) uv0: vec2<f32>,
    @location(3) uv1: vec2<f32>,
    @location(4) tangent: vec4<f32>,
    @location(5) color: vec4<f32>,
#ifdef FX_SPRITE
    // Particles: the dynamic parameters; lens flares: intensity, occlusion,
    // radial distance and ray distance.
    @location(6) param: vec4<f32>,
    // x: the sub-images' blend; a lens flare's light's depth and distance (x, y;
    // cm) and where the light is on the screen (zw: 0..1, down).
    @location(7) extra: vec4<f32>,
#endif
#endif
}

// A vertex's data, from its mesh or (FX_PULLED) the frame's effect data.
struct FxVertex {
    position: vec3<f32>,
    normal: vec3<f32>,
    uv0: vec2<f32>,
    uv1: vec2<f32>,
    tangent: vec4<f32>,
    color: vec4<f32>,
    param: vec4<f32>,
    extra: vec4<f32>,
    // Past its entity's indices (or out of the data): nothing to draw.
    none: bool,
}

#ifdef FX_PULLED
const FX_HEADER_WORDS: u32 = 4u;
const FX_VERTEX_WORDS: u32 = 26u;

fn fx_float(i: u32) -> f32 {
    return bitcast<f32>(fx_words[i]);
}

fn fx_vec2(i: u32) -> vec2<f32> {
    return vec2<f32>(fx_float(i), fx_float(i + 1u));
}

fn fx_vec3(i: u32) -> vec3<f32> {
    return vec3<f32>(fx_float(i), fx_float(i + 1u), fx_float(i + 2u));
}

fn fx_vec4(i: u32) -> vec4<f32> {
    return vec4<f32>(fx_float(i), fx_float(i + 1u), fx_float(i + 2u), fx_float(i + 3u));
}
#endif

fn fx_vertex(v: Vertex) -> FxVertex {
    var o: FxVertex;
#ifdef FX_PULLED
    o.none = true;
    o.position = vec3<f32>(0.0);
    o.normal = vec3<f32>(0.0, 1.0, 0.0);
    o.tangent = vec4<f32>(1.0, 0.0, 0.0, 1.0);
    o.uv0 = vec2<f32>(0.0);
    o.uv1 = vec2<f32>(0.0);
    o.color = vec4<f32>(0.0);
    o.param = vec4<f32>(0.0);
    o.extra = vec4<f32>(0.0);
    // (Every read kept inside the data: a slot or index past it draws nothing.)
    let size = arrayLength(&fx_words);
    let h = get_tag(v.instance_index) * FX_HEADER_WORDS;
    let n = u32(v.position.x);
    if (h + 2u < size) {
        let vertices = fx_words[h];
        let indices = fx_words[h + 1u];
        let count = fx_words[h + 2u];
        if (n < count && indices + n < size) {
            let w = vertices + fx_words[indices + n] * FX_VERTEX_WORDS;
            if (w + FX_VERTEX_WORDS <= size) {
                o.none = false;
                o.position = fx_vec3(w);
                o.normal = fx_vec3(w + 3u);
                o.uv0 = fx_vec2(w + 6u);
                o.uv1 = fx_vec2(w + 8u);
                o.tangent = fx_vec4(w + 10u);
                o.color = fx_vec4(w + 14u);
                o.param = fx_vec4(w + 18u);
                o.extra = fx_vec4(w + 22u);
            }
        }
    }
#else
    o.none = false;
    o.position = v.position;
    o.normal = v.normal;
    o.uv0 = v.uv0;
    o.uv1 = v.uv1;
    o.tangent = v.tangent;
    o.color = v.color;
#ifdef FX_SPRITE
    o.param = v.param;
    o.extra = v.extra;
#else
    o.param = vec4<f32>(1.0);
    o.extra = vec4<f32>(0.0);
#endif
#endif
    return o;
}

struct FxVaryings {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) world_tangent: vec4<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,
    @location(5) color: vec4<f32>,
    @location(6) param: vec4<f32>,
    @location(7) extra: vec4<f32>,
    // (The mesh's place, the same for all of it: flat, as interpolating it leaves
    // the last bits differing from pixel to pixel, which RandomSeed's hash of it
    // magnifies into noise.)
    @location(8) @interpolate(flat) object_position: vec3<f32>,
#ifdef FX_SURFACE
    @location(9) @interpolate(flat) instance_index: u32,
#endif
}

@vertex
fn vertex(vin: Vertex) -> FxVaryings {
    let v = fx_vertex(vin);
    var out: FxVaryings;
    let world_from_local = get_world_from_local(vin.instance_index);
    let world = world_from_local * vec4<f32>(v.position, 1.0);
    out.position = view.clip_from_world * world;
    out.world_position = world.xyz;
    out.world_normal = mesh_normal_local_to_world(v.normal, vin.instance_index);
    out.world_tangent = mesh_tangent_local_to_world(world_from_local, v.tangent, vin.instance_index);
    out.uv0 = v.uv0;
    out.uv1 = v.uv1;
    out.color = v.color;
    out.param = v.param;
    out.extra = v.extra;
    out.object_position = world_from_local[3].xyz;
#ifdef FX_SURFACE
    out.instance_index = vin.instance_index;
#endif
#ifdef FX_ALPHA_RESTORE
    // (The picture's alpha put back to 1 once everything is drawn: levelfx.rs's
    // AlphaRestore, the distortion parts having used it up. A triangle over the
    // whole view, its vertices already where they go on the screen.)
    out.position = vec4<f32>(v.position.xy, 1.0, 1.0);
#endif
    if (v.none) {
        // (Off the screen: its triangle is left out.)
        out.position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    return out;
}

#ifdef FX_SURFACE
// UE3's light map basis: the three directions (in the surface's tangent frame) its
// directional light maps keep their light from.
const FX_LM_BASIS0: vec3<f32> = vec3<f32>(0.0, 0.81649658, 0.57735027);
const FX_LM_BASIS1: vec3<f32> = vec3<f32>(-0.70710678, -0.40824829, 0.57735027);
const FX_LM_BASIS2: vec3<f32> = vec3<f32>(0.70710678, -0.40824829, 0.57735027);

// The colour of a surface lit by its light map, as the game's base pass shades it
// with a directional light map (TBasePassPixelShaderFDirectionalTextureLightMapPolicy,
// read from the game's compiled shaders): the light map keeps light from its three
// basis directions, and
//   diffuse   = diffuse colour * sum of each direction's light * (1.127 * cos to the normal)^2
//   highlight = specular colour * (n + 8) / (8 pi) * (N.H)^n * light * |d|^2 / sum d,
//               H halfway between the camera and where the light comes from (the
//               directions weighed by their light, as long as 1 at most)
//   reflection = specular colour * ReflectionColor * the cube map, blurred by the
//               power, * the light map's light from the reflection's way
// (n: the specular power; d: the three directions' light). `flat` is the light map's
// light on a surface facing along its normal (the page's: its colour * sum d /
// sqrt(3)); `w_in` how it is shared between the directions (d / sum d).
fn fx_surface_color(i: FxIn, o: FxOut, flat: vec3<f32>, w_in: vec3<f32>) -> vec3<f32> {
    let w = w_in / max(w_in.x + w_in.y + w_in.z, 0.0001);
    let n = fx_normalize3(o.normal);
    let sqrt3 = 1.7320508;
    // The light map's light, times sum d (per colour), and how much there is of it.
    let total = flat * sqrt3;
    let lum = dot(total, vec3<f32>(0.3, 0.59, 0.11));
    let c0 = saturate(dot(n, FX_LM_BASIS0)) * 1.127;
    let c1 = saturate(dot(n, FX_LM_BASIS1)) * 1.127;
    let c2 = saturate(dot(n, FX_LM_BASIS2)) * 1.127;
    let diffuse = o.diffuse * total * (w.x * c0 * c0 + w.y * c1 * c1 + w.z * c2 * c2);
    // Where the light comes from (no longer than 1).
    var l = (w.x * FX_LM_BASIS0 + w.y * FX_LM_BASIS1 + w.z * FX_LM_BASIS2) * lum;
    let len = length(l) + 0.001;
    if (len > 1.0) {
        l = l / len;
    }
    let v = fx_normalize3(i.camera_vector_ts);
    let h = fx_normalize3(l + v);
    let power = max(o.spec_power, 1.0);
    let highlight = (power + 8.0) * 0.0397887 * pow(saturate(dot(n, h)), power);
    let spec = max(o.specular, vec3<f32>(0.0));
    var color = diffuse + spec * highlight * total * dot(w, w);
    // The cube map's reflection, as blurred as the power says (RoughnessScale: misc.w
    // of the cube's misc.z mip levels), times the light from the reflection's way.
    let r_ts = reflect(-v, n);
    let r_light = total * (w.x * saturate(dot(r_ts, FX_LM_BASIS0)) + w.y * saturate(dot(r_ts, FX_LM_BASIS1))
        + w.z * saturate(dot(r_ts, FX_LM_BASIS2)));
    let r_ws = fx_tangent_to_world(i, r_ts);
    let lod = fx_params.misc.z * max(1.0 - fx_params.misc.w * log2(power), 0.0);
    let cube = textureSampleLevel(fx_cube_tex, fx_cube_samp, r_ws, lod).rgb;
    color += spec * o.reflection * cube * fx_params.ambient.x * r_light;
    return color;
}
#endif

// Bevy's axes to Unreal's: a direction, and a position (metres to centimetres).
fn ue_dir(v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(v.x, v.z, v.y);
}

fn ue_pos(v: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(v.x, v.z, v.y) * 100.0;
}

// High Moon's RandomSeed: a number from 0 to 1 for each placed mesh or effect, here
// from where it is placed (Unreal's units, rounded to the unit, hashed).
fn fx_random_seed(p: vec3<f32>) -> f32 {
    let q = vec3<i32>(round(p));
    var h = (bitcast<u32>(q.x) * 73856093u) ^ (bitcast<u32>(q.y) * 19349663u) ^ (bitcast<u32>(q.z) * 83492791u);
    h = (h ^ (h >> 16u)) * 0x45d9f3bu;
    h = (h ^ (h >> 16u)) * 0x45d9f3bu;
    h = h ^ (h >> 16u);
    return f32(h & 0xffffffu) / 16777216.0;
}

// How much of the camera's fog covers a point (Bevy's fog settings).
fn fx_fog_amount(world_position: vec3<f32>) -> f32 {
    return fx_fog_at(length(world_position - view.world_position));
}

// How much of it covers what is `d` metres away.
fn fx_fog_at(d: f32) -> f32 {
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

#ifdef FX_FLARE
// How much of a lens flare shows (the garage's are drawn in front of everything,
// levelfx.rs's flare_elements): none of it when something stands more than a metre
// in front of its light, by the scene's depth where the light is on the screen
// (extra.zw) against the light's own (extra.x, cm).
fn fx_flare_seen(extra: vec4<f32>) -> f32 {
#ifdef DEPTH_PREPASS
    if (any(extra.zw < vec2<f32>(0.0)) || any(extra.zw >= vec2<f32>(1.0))) {
        return 1.0;
    }
    let px = vec2<i32>(extra.zw * view.viewport.zw + view.viewport.xy);
    let d = textureLoad(depth_prepass_texture, px, 0);
    if (d <= 0.0) {
        return 1.0;
    }
    let in_front = extra.x + depth_ndc_to_view_z(d) * 100.0;
    return saturate(1.0 - in_front / 100.0);
#else
    return 1.0;
#endif
}
#endif

@fragment
fn fragment(in: FxVaryings, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
#ifdef FX_ALPHA_RESTORE
    // (Its alpha alone: the blend keeps the colour.)
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
#else
    var i: FxIn;
    i.uv0 = in.uv0;
    i.uv1 = in.uv1;
    i.color = in.color;
    i.mesh_color = vec4<f32>(1.0);
#ifdef FX_MESH_PARTICLE
    i.color = vec4<f32>(1.0);
    i.mesh_color = in.color;
#endif
#ifdef FX_SURFACE
    // (A surface's vertex colour is its vertex lighting.)
    i.color = vec4<f32>(1.0);
#endif
#ifdef FX_SKYFOG
    // (A far piece of the sky's vertex colour is the fog it is seen through.)
    i.color = vec4<f32>(1.0);
#endif
    i.dparam = vec4<f32>(1.0);
    i.subuv_lerp = in.extra.x;
    i.flare = vec4<f32>(0.0);
    i.flare_src = 0.0;
#ifdef FX_FLARE
    i.flare = in.param;
    i.flare_src = in.extra.y;
    i.subuv_lerp = 0.0;
#else
    i.dparam = in.param;
#endif
    i.time = globals.time;

    let depth_m = -position_world_to_view(in.world_position).z;
    i.depth = depth_m * 100.0;
    var scene_m = 1.0e7;
#ifdef DEPTH_PREPASS
    let d = textureLoad(depth_prepass_texture, vec2<i32>(in.position.xy), 0);
    if (d > 0.0) {
        scene_m = -depth_ndc_to_view_z(d);
    }
#endif
    i.scene_depth = scene_m * 100.0;
    i.world_pos = ue_pos(in.world_position);
    i.object_pos = ue_pos(in.object_position);

    // The tangent frame; a two-sided surface seen from behind faces the other way
    // (sprites face the camera as they are).
    var n = fx_normalize3(in.world_normal);
#ifdef FX_SURFACE
    // (A surface's normal already faces the side it is drawn from, a mirrored
    // mesh's too, whose front faces are the ones culled: only a two-sided one seen
    // from behind turns round.)
#ifdef FX_TWO_SIDED
    if (!front) {
        n = -n;
    }
#endif
#else
#ifndef FX_SPRITE
    if (!front) {
        n = -n;
    }
#endif
#endif
#ifdef FX_MESH_PARTICLE
    if (!front) {
        n = -n;
    }
#endif
    let t = fx_normalize3(in.world_tangent.xyz);
    let b = cross(n, t) * select(-1.0, 1.0, in.world_tangent.w >= 0.0);
    i.normal = ue_dir(n);
    i.tangent = ue_dir(t);
    // (Bevy's bitangent runs against V; Unreal's tangent frame's y with it.)
    i.bitangent = ue_dir(-b);
    i.pixel_normal = i.normal;
    i.camera_vector = ue_dir(fx_normalize3(view.world_position - in.world_position));
    i.camera_vector_ts = fx_world_to_tangent(i, i.camera_vector);
    i.view_right = ue_dir(view.world_from_view[0].xyz);
    i.view_up = ue_dir(view.world_from_view[1].xyz);
    i.view_forward = ue_dir(-view.world_from_view[2].xyz);
    let clip = view.clip_from_world * vec4<f32>(in.world_position, 1.0);
    let ndc = clip.xy / max(clip.w, 0.0001);
    i.clip = vec4<f32>(ndc * i.depth, i.depth - 10.0, i.depth);
    i.screen_uv = (in.position.xy - view.viewport.xy) / view.viewport.zw;

    var o = fx_material(i);
#ifdef FX_FLARE
    // (Hidden by what stands in front of its light.)
    let seen = fx_flare_seen(in.extra);
    o.emissive *= seen;
    o.opacity *= seen;
#endif
#ifdef FX_OPACITY_CLIP
    // A distorting material's base pass draws nothing where its opacity is under a
    // 255th (the game's compiled shaders for every material with both a Distortion
    // and an Opacity: texkill on the opacity less 1/255; one without an Opacity
    // keeps its 1). It matters where the opacity goes below nothing: the screen's
    // energon splash (ScreenSplatter_01_MAT) lerps past its ends by its particles'
    // alpha of up to 1.5, and its multiply there took the picture to black in
    // blotches the game never draws.
    if (o.opacity < 1.0 / 255.0) {
        discard;
    }
#endif

#ifdef FX_SURFACE
    // Its light map's light (times its vertex lighting, where that is how it is lit)
    // and directions, at the same place on its page.
    var flat = vec3<f32>(0.0);
    var weights = vec3<f32>(1.0);
#ifdef LIGHTMAP
    flat = lightmap(in.uv1, fx_params.misc.y, in.instance_index) * in.color.rgb;
    let packed = mesh[in.instance_index].lightmap_uv_rect;
    let rect = vec4<f32>(unpack2x16unorm(packed.x), unpack2x16unorm(packed.y));
    weights = textureSampleLevel(fx_dir_tex, fx_dir_samp, mix(rect.xy, rect.zw, in.uv1), 0.0).rgb;
#endif
    return vec4<f32>(fx_surface_color(i, o, flat, weights) + o.emissive, 1.0);
#else
#ifdef FX_DISTORT_PART
    // A distorting material's distortion, drawn as a part of its own just before the
    // rest of it: the scene's copy moved by its shift, less the copy here, added to
    // the picture as far as no distortion drawn before it here has moved it yet
    // (the picture's alpha: levelfx.rs's distort blend adds it times the alpha and
    // takes this alpha's share off the alpha). So however many layers overlap, the
    // picture is moved about as far as the furthest of them, as the game's single
    // move by their (clamped) sum does, never darkened past black by their sum.
    let distort_by = fx_shift(o.distortion);
    return vec4<f32>(fx_distorted(o.distortion, i.screen_uv), saturate(length(distort_by) / FX_DISTORT_FULL));
#else
    var color = o.emissive;
#ifdef FX_LIT
    // The game's lit translucency: diffuse light from all around, from the sun
    // (wrapped round the dark side) and from the level's lights there, gathered
    // into one (falling on it by how squarely it faces them).
    let pn = fx_tangent_to_world(i, fx_normalize3(o.normal));
    let wrap = saturate(dot(pn, fx_params.sun_dir.xyz) * 0.5 + 0.5);
    let facing = saturate(dot(pn, fx_params.light_dir.xyz));
    color += o.diffuse * (fx_params.ambient.rgb + fx_params.sun.rgb * wrap + fx_params.light.rgb * facing);
#ifdef FX_MASKED
    // A solid piece the game lights (a destructible's chunk, by its light
    // environment): its highlights too, the sun's and the gathered light's (Blinn-Phong
    // normalized as the surfaces' are: fx_surface_color).
    let v = fx_normalize3(i.camera_vector);
    let power = max(o.spec_power, 1.0);
    let norm = (power + 8.0) * 0.0397887;
    let to_sun = saturate(dot(pn, fx_params.sun_dir.xyz));
    let sun_hi = pow(saturate(dot(pn, fx_normalize3(fx_params.sun_dir.xyz + v))), power) * to_sun;
    let light_hi = pow(saturate(dot(pn, fx_normalize3(fx_params.light_dir.xyz + v))), power) * facing;
    color += max(o.specular, vec3<f32>(0.0)) * norm * (fx_params.sun.rgb * sun_hi + fx_params.light.rgb * light_hi);
#endif
#endif
#ifdef FX_FLARE
    // (Its fog is its light's: the garage's are drawn just in front of the camera.)
    let fog_amount = fx_fog_at(i.flare_src * 0.01);
#else ifdef FX_NOFOG
    // (The far backdrop, Oblivion's space: outside the fog.)
    let fog_amount = 0.0;
#else ifdef FX_SKYFOG
    // (A far piece of the sky: the fog its vertex colours carry, as the game works
    // it out at each vertex, the layers of the level's height fog it is seen
    // through. Alpha: how much of its own light gets through.)
    let fog_amount = 1.0 - in.color.a;
#else
    let fog_amount = fx_fog_amount(in.world_position);
#endif
    // The light the fog lays over it.
#ifdef FX_SKYFOG
    let fog_light = in.color.rgb;
#else
    let fog_light = fog.base_color.rgb * fog_amount;
#endif

#ifdef FX_ADDITIVE
    // Added to the scene as much as its opacity says (as Unreal adds an additive
    // material's colour times its opacity: premultiplied blending with no alpha),
    // an opacity over 1 too: the game's base pass multiplies by it unclamped (its
    // compiled shaders, LightningBeamTrail_MAT's: mul_pp r2.xyz, r2.w, r0), and
    // its particles' alphas go past 1 to make them brighter (the Electric Fences'
    // lightning, StartAlpha 6). Fog hides it.
    let added = max(color, vec3<f32>(0.0)) * max(o.opacity, 0.0);
    return vec4<f32>(added * (1.0 - fog_amount), 0.0);
#else ifdef FX_MODULATE
    // Multiplies the scene; fog takes it back to white.
    let factor = mix(max(color, vec3<f32>(0.0)), vec3<f32>(1.0), fog_amount);
#ifdef FX_MODULATE_BRIGHT
    // (Spawned effects: what is over 1, added as that much more of the scene
    // behind, as its distortion moves it; drawn with additive blending.)
#ifdef FX_DISTORT
    let behind = fx_scene_color(fx_moved_uv(fx_shift(o.distortion), i.screen_uv)).rgb;
#else
    let behind = fx_scene_color(i.screen_uv).rgb;
#endif
    return vec4<f32>(behind * max(factor - vec3<f32>(1.0), vec3<f32>(0.0)), 0.0);
#else ifdef FX_MODULATE_DARK
    // (and up to 1, multiplied in)
    return vec4<f32>(min(factor, vec3<f32>(1.0)), 1.0);
#else
    return vec4<f32>(factor, 1.0);
#endif
#else ifdef FX_MASKED
    if (o.opacity_mask < fx_params.misc.x) {
        discard;
    }
    return vec4<f32>(color * (1.0 - fog_amount) + fog_light, 1.0);
#else
    // Translucent: blended over the scene by its opacity (premultiplied).
    let a = saturate(o.opacity);
    let fogged = max(color, vec3<f32>(0.0)) * (1.0 - fog_amount) + fog_light;
    return vec4<f32>(fogged * a, a);
#endif
#endif
#endif
#endif
}
