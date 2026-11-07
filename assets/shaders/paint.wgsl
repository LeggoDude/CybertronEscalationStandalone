// The characters' paint (src/character.rs): the game's customizable character
// materials (TR_BRDF_CustChar_TEXSET_MAT, ENV_BRDF_A_MAT underneath) over the
// part's own textures, shaded as the game's compiled shaders for them shade it
// (its TLightPixelShaderTwoLightUberLightPolicy, which lights a character).
//
// The texture sets keep a paint mask in Masks_NormY's blue (the parts' colour
// texture carries it in its alpha): 0.5 is unpainted, over it the primary colour
// (Cust_Color_A) is laid on, under it the secondary (Cust_COLOR_B), each as far as
// the mask is from 0.5 (saturate(m - 0.5) * 2), multiplying the colour.
//
// Each part's material sets how it is lit (`paint.gloss`, `paint.shade`,
// `paint.coefficient`: tools/build_paint.py's values, assets/models/paint.txt):
//
//   gloss    = smoothstep(SmoothStep_Min, _Max, Masks_NormY green + Roughness Offset)
//              (the green sampled as sRGB, as the game does; the parts' metal-roughness
//              texture keeps it as 0.25 + 0.55 * green in its metal)
//   power    = max(Roughness Power * gloss, 1), the highlight's
//   specular = gloss * (1 + 2 * metallic) * the reflection coefficient: the unpainted
//              metal's CoefficientColor_C, the painted parts' a mix from 0.1 (the
//              coefficient TnCharacterApplier gives them in a multiplayer game) to
//              their paint colour as the build's metallic slider (MetallicOffset)
//              goes to 1
//   diffuse  = the painted colour * DiffuseIntensityScalar * (1 - the specular's
//              luminance + DiffuseIntensityAdd)
//   reflection = specular * 3 (ENV_Reflection_BRDF_MF: NewReflectionIntensity + 2)
//              * the reflection cube map (the level's, or the garage's), blurred as
//              far as the power says (RoughnessScale 0.1 of its mips), * the light
//              coming from the reflection's way, at most white (the game's light
//              environment, AllLightsSH, saturated: here the light a white surface
//              facing that way gets)
//
// So the metallic slider turns the paint into coloured metal: its colour goes from
// the diffuse into the specular, which reflects the room around it. A part of the
// map the game shades the same way (character.rs's gild_level_parts: Ignition's gold
// statue of Starscream) is drawn so too, unpainted (`paint.shade.w` 1), its material's
// MetallicOffset as the slider and its own reflection strength (`paint.coefficient.w`). Bevy's lights
// give the highlights (its specular colour the game's / pi: the game's highlights,
// (n + 8) / (8 pi) * (N.H)^n, have no pi on them as its diffuse has none), without
// Bevy's flat ambient specular, which the game doesn't have (its reflection is the
// cube map's).
//
// The glow (the material's emissive texture, when the energon's w is 1: see
// tools/build_class.py's GLOW_COVERAGE) holds two masks: in red the part's own
// light, lit in its painted colour times DiffuseIntensityScalar and
// Emissive_Intensity (TR_Emissive_MF), in green its energon, lit in its faction's
// EnergonColor times Energon_Intensity (TR_Energon_MF) with the energon's pulse:
// fractalMaps panned across the part, its value and the time through two sines
// and a power of 10 (energonSpeed 0.15, energonIntensity 0.55, energonContrast
// 10), so bright flashes run through it now and then. The game's values are in
// its "1.0 = white" units, as the demo's lighting is (lighting.rs), so the glow
// goes out with the camera's exposure left off it (the emissive's alpha 0).

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    mesh_view_bindings::{globals, lights, view, view_transmission_texture, view_transmission_sampler},
    lighting::{EnvBRDFApprox, F_AB},
    view_transformations::position_world_to_view,
}

// (The player's own Jazz fades as the camera comes close: main.rs's
// fade_near_camera dithers him with Bevy's visibility-range dither.)
#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither
#endif

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct PaintColors {
    primary: vec4<f32>,
    secondary: vec4<f32>,
    energon: vec4<f32>,
    res_in: vec4<f32>,
    // A dead robot dissolving (src/enemies.rs: the death sets' TnDeresser): where
    // its overlay comes out from and in w its reach (below 0: not dissolving); its
    // DefragValue, past which the robot is drawn no more, the overlay's noise
    // multiplier and 1 for ElectricArcs as its noise; the overlay's look
    // (OverlayLook's).
    deres: vec4<f32>,
    deres_fx: vec4<f32>,
    deres_fresnel: vec4<f32>,
    deres_lead: vec4<f32>,
    deres_burn: vec4<f32>,
    deres_energon: vec4<f32>,
    // Roughness Power, Roughness Offset, SmoothStep_Min, SmoothStep_Max.
    gloss: vec4<f32>,
    // DiffuseIntensityScalar, DiffuseIntensityAdd, the reflection cube map's last
    // mip level (below 0: no cube map, no reflection), and 1 for a part of the map
    // shaded as the characters are (character.rs's gild_level_parts: Ignition's gold
    // statue), unpainted, its gloss mask kept as the level's texture sets keep it.
    shade: vec4<f32>,
    // CoefficientColor_C, and the reflection's strength (0: the characters', 3).
    coefficient: vec4<f32>,
    // A Scout cloaking (src/cloak.rs): his elbow's joint, where the cloak comes out
    // from (CloakOverlayOriginBoneName), and in w the overlay's reach (ObjectRadius:
    // his bounds' radius times 1.2; below 0: not one of the cloak's copies).
    cloak: vec4<f32>,
    // How the copy draws him: x 1 for the overlay, 2 for the cloak; y the overlay's
    // DefragValue (its time over CloakActivateDuration); z the cloak's Fade_In (its
    // meter running low).
    cloak_fx: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> paint: PaintColors;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var energon_noise: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var energon_noise_sampler: sampler;
// The cube map the characters reflect, in Unreal's axes as the game samples it.
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var reflection_cube: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var reflection_sampler: sampler;
// The cloak's textures (tools/build_cloak.py's textures/cloak/), repeating: the
// overlay's WEP_Defrag_Alphas and its two gradients (pulseGradien_r01 in red,
// pulseGradien_r02 in green), the cloak's DiffClouds_CLR, bubbles and
// Pixel_4_NORM_DXT1 (the normal maps' x and y).
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var cloak_alphas: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var cloak_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var cloak_pulse: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var cloak_pulse_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var cloak_clouds: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var cloak_bubbles: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var cloak_pixels: texture_2d<f32>;
// The dissolve's own (on a dissolving robot, with the overlay's three above):
// IaconMask_NORM's alpha (its lines, in red) and ElectricArcs.
@group(#{MATERIAL_BIND_GROUP}) @binding(112) var defrag_lines: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(113) var defrag_arcs: texture_2d<f32>;

// The characters' material's values (TR_BRDF_CustChar_TEXSET_MAT and its parts'
// instances: DiffuseIntensityScalar 0.75 times Emissive_Intensity 5; Energon_Intensity).
const OWN_LIGHT: f32 = 3.75;
const ENERGON_INTENSITY: f32 = 10.0;
const TAU: f32 = 6.283185307179586;
const PI: f32 = 3.141592653589793;
// The dissolve's overlay's lines: IaconMask_NORM laid over the view's x and y from
// the overlay's middle, once every 1,000 centimetres (0.1 a metre).
const DEFRAG_LINES_SCALE: f32 = 0.1;
// The painted parts' reflection coefficient with the metallic slider at 0
// (TnCharacterApplier.ExtractColors: _PrimaryCoefColor and _SecondaryCoefColor in a
// multiplayer game).
const PAINT_COEFFICIENT: f32 = 0.1;
// The cloak's overlay (TR_Cloak_Overlay_MATINST, tools/build_cloak.py): its noise's
// tiling and drift (MaterialTiling; fractalMaps' panner), its front's glow (Lead
// Intensity), the glow round the joint (Core Intensity and Exponent), the rim
// (Fresnel Intensity and Exponent), their colours, and the masked material's clip
// value (OpacityMaskClipValue).
const CLOAK_TILING: f32 = 3.0;
const CLOAK_NOISE_PAN: vec2<f32> = vec2<f32>(0.15, 0.2);
const CLOAK_LEAD: f32 = 45.0;
const CLOAK_CORE: f32 = 35.0;
const CLOAK_CORE_EXPONENT: f32 = 24.0;
const CLOAK_FRESNEL: f32 = 7.0;
const CLOAK_FRESNEL_EXPONENT: f32 = 3.9137261;
const CLOAK_BURN_COLOR: vec3<f32> = vec3<f32>(0.0, 0.20392466, 1.0);
const CLOAK_LEAD_COLOR: vec3<f32> = vec3<f32>(0.0, 0.12960832, 0.86413044);
const CLOAK_FRESNEL_COLOR: vec3<f32> = vec3<f32>(1.0, 0.28791046, 0.08314788);
const CLOAK_CLIP: f32 = 0.3333;
// The cloak (TR_Cloak_MATINST): its normal's drift (the clouds' and the bubbles'
// panners, the bubbles' and the fine normal's tiling), how far off its distortion
// fades (DistortionIntensity's 1 times 1 - 0.00015 x the distance in centimetres),
// and its waves as it runs low (Radial Falloff Intensity over 400 centimetres from
// the joint, Fade Out Wave Color).
const CLOAK_CLOUDS_PAN: vec2<f32> = vec2<f32>(0.05, -0.035);
const CLOAK_BUBBLES_PAN: vec2<f32> = vec2<f32>(-0.05, 0.08);
const CLOAK_BUBBLES_TILING: f32 = 2.0;
const CLOAK_PIXELS_TILING: f32 = 35.0;
const CLOAK_DISTORTION: f32 = 1.0;
const CLOAK_FADE_PER_CM: f32 = 0.00015;
const CLOAK_WAVE_REACH_CM: f32 = 400.0;
const CLOAK_WAVE_FALLOFF: f32 = 2.0;
const CLOAK_WAVE_COLOR: vec3<f32> = vec3<f32>(0.02, 0.01, 0.55);
// The reflection's strength (ENV_Reflection_BRDF_MF: NewReflectionIntensity 1 + 2)
// and how its blur follows the highlight's power (its ReflectionCubeMapSample's
// RoughnessScale).
const REFLECTION_INTENSITY: f32 = 3.0;
const REFLECTION_ROUGHNESS_SCALE: f32 = 0.1;

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

// An overlay's look (TR_Cloak_Overlay_Mat's and TR_DefragOverlay_AB_Mat's
// parameters): its rim's colour times its intensity and its exponent (Fresnel
// Color, Intensity, Exponent); its front's colour and intensity (Defrag Lead Color,
// Lead Intensity); its burn-out colour and the glow round its middle (Burn Out
// Color, Core Intensity); its lines' intensity and contrast, the core's exponent and
// its noise's tiling (Energon Intensity and Contrast, Core Exponent,
// MaterialTiling).
struct OverlayLook {
    fresnel: vec4<f32>,
    lead: vec4<f32>,
    burn: vec4<f32>,
    energon: vec4<f32>,
}

// An overlay drawn over a robot (TR_Cloak_Overlay_Mat's graph and
// TR_DefragOverlay_AB_Mat's, as tools/ue3/fxmat.py reads them): its glow (rgb) and
// its opacity mask (a) at a point `near` (0 to 0.75: its distance from the
// overlay's middle over its reach) on the part (texture coordinates `uv`, facing
// `n`, the camera's way `v`). Its front comes out from the middle as the
// DefragValue rises: a ragged one, k (the noise and the DefragValue, to the 0.75th)
// running ahead of near; where k has just passed near it glows (pulseGradien_r01
// just under each whole number of near - k; the dissolve's the more the further it
// has come, `lead_k` k), and behind it the overlay holds (pulseGradien_r02 over
// half) with its rim (and the dissolve's `lines`), until it burns out where k is
// over a half (WEP_Defrag_Alphas' green and its fourth power) and is gone.
fn overlay_glow(look: OverlayLook, near: f32, k: f32, lead_k: f32, lines: f32, uv: vec2<f32>, n: vec3<f32>, v: vec3<f32>) -> vec4<f32> {
    let pulse = textureSample(cloak_pulse, cloak_pulse_sampler, vec2<f32>(near - k));
    let alphas = textureSample(cloak_alphas, cloak_sampler, uv * look.energon.w).g;
    let past = (k - 0.5) * 2.0;
    let burn = floor(k * 2.0) * (alphas * past + pow(max(past, 0.0), 4.0));
    let mask = clamp(pulse.g * (1.0 - burn * 8.0), 0.0, 1.0);
    let rim = pow(max(1.0 - max(dot(n, v), 0.0), 0.0), look.fresnel.w);
    let core = pow(max(1.0 - near, 0.0), look.energon.z) * look.burn.w;
    let burnt = look.burn.rgb * burn;
    let glow = look.fresnel.rgb * rim
        + look.lead.rgb * (core + pulse.r * pulse.r * look.lead.w * lead_k + lines)
        + burnt * burnt * 100.0;
    return vec4<f32>(glow, mask);
}

// The cloak's overlay (TR_Cloak_Overlay_MATINST): out from his joint
// (`paint.cloak`) as its DefragValue (`paint.cloak_fx.y`) rises, its noise
// fractalMaps' green, drifting.
fn cloak_overlay(uv: vec2<f32>, world: vec3<f32>, n: vec3<f32>, v: vec3<f32>) -> vec4<f32> {
    let near = saturate(distance(world, paint.cloak.xyz) / max(paint.cloak.w, 1e-3)) * 0.75;
    let noise = textureSample(energon_noise, energon_noise_sampler, uv * CLOAK_TILING + globals.time * CLOAK_NOISE_PAN).g;
    let k = pow(max(noise + paint.cloak_fx.y, 0.0), 0.75);
    let look = OverlayLook(
        vec4<f32>(CLOAK_FRESNEL_COLOR * CLOAK_FRESNEL, CLOAK_FRESNEL_EXPONENT),
        vec4<f32>(CLOAK_LEAD_COLOR, CLOAK_LEAD),
        vec4<f32>(CLOAK_BURN_COLOR, CLOAK_CORE),
        vec4<f32>(0.0, 1.0, CLOAK_CORE_EXPONENT, CLOAK_TILING),
    );
    return overlay_glow(look, near, k, 1.0, 0.0, uv, n, v);
}

// The dead's dissolve's overlay (TR_DefragOverlay_MATINST, or its variants: their
// look in `paint.deres_*`): out from the body's middle (`paint.deres`) as the
// deresser's DefragValue (`paint.deres_fx.x`) rises, its noise (fractalMaps', or
// ElectricArcs', green) times its multiplier; and where it holds, the lines of
// IaconMask_NORM's alpha laid across the view from the middle glow in its lead
// colour (|sin(alpha 2 pi)| to the Energon Contrast, times the Energon Intensity:
// the edges of its lines).
fn defrag_overlay(uv: vec2<f32>, world: vec3<f32>, n: vec3<f32>, v: vec3<f32>) -> vec4<f32> {
    let near = saturate(distance(world, paint.deres.xyz) / max(paint.deres.w, 1e-3)) * 0.75;
    let at = uv * paint.deres_energon.w + globals.time * CLOAK_NOISE_PAN;
    let fractal = textureSample(energon_noise, energon_noise_sampler, at).g;
    let arcs = textureSample(defrag_arcs, cloak_sampler, at).g;
    let noise = select(fractal, arcs, paint.deres_fx.w > 0.5);
    let k = pow(max(noise * paint.deres_fx.z + paint.deres_fx.x, 0.0), 0.75);
    let across = (position_world_to_view(world) - position_world_to_view(paint.deres.xyz)).xy;
    let line = textureSample(defrag_lines, cloak_sampler, across * DEFRAG_LINES_SCALE).r;
    let lines = pow(max(abs(sin(line * TAU)), 1e-6), paint.deres_energon.y) * paint.deres_energon.x;
    let look = OverlayLook(paint.deres_fresnel, paint.deres_lead, paint.deres_burn, paint.deres_energon);
    return overlay_glow(look, near, k, k, lines, uv, n, v);
}

// The cloak itself (TR_Cloak_MAT): the picture behind him shifted by its normal (the
// bubbles, drifting over the clouds, flattened with the distance and laid over a fine
// normal), as the game's distortion pass shifts it: the normal in the world, its
// Unreal X and Y times 2 (and DistortionIntensity) fading with the distance, a
// 255th of the screen each (its accumulate shaders keep 4 x that over 255, its apply
// shader moves the picture by a quarter of what they keep, V the other way), under
// the smallest it bothers with left out. Over that, as his meter runs low (Fade_In,
// `paint.cloak_fx.z`), its waves out from the joint, moving on as it rises.
// Premultiplied: what it adds over the picture, and how much of it it covers.
fn cloak_glass(in: VertexOutput, uv: vec2<f32>, n: vec3<f32>) -> vec4<f32> {
    let t = globals.time;
    let world = in.world_position.xyz;
    let clouds = textureSample(cloak_clouds, cloak_sampler, uv + t * CLOAK_CLOUDS_PAN).xy;
    let bubbles = textureSample(cloak_bubbles, cloak_sampler, uv * CLOAK_BUBBLES_TILING + t * CLOAK_BUBBLES_PAN + clouds).xy * 2.0 - 1.0;
    let fine = textureSample(cloak_pixels, cloak_sampler, uv * CLOAK_PIXELS_TILING).xy * 2.0 - 1.0;
    let fine_z = sqrt(saturate(1.0 - dot(fine, fine)));
    let centimetres = -position_world_to_view(world).z * 100.0;
    let near = 1.0 - saturate(centimetres * CLOAK_FADE_PER_CM);
    let ts = normalize(vec3<f32>(bubbles * 0.3 * near + fine, fine_z));
    var normal = n * ts.z;
#ifdef VERTEX_TANGENTS
    // (Unreal's tangent frame, as levelfx.wgsl takes it from Bevy's.)
    let tangent = normalize(in.world_tangent.xyz);
    let bitangent = -cross(n, tangent) * select(-1.0, 1.0, in.world_tangent.w >= 0.0);
    normal = tangent * ts.x + bitangent * ts.y + n * ts.z;
#endif
    let shift = vec2<f32>(normal.x, normal.z) * (2.0 * near * CLOAK_DISTORTION);
    let at = (in.position.xy - view.viewport.xy) / view.viewport.zw;
    var moved = vec3<f32>(0.0);
    if dot(shift, shift) * 16.0 >= 0.1 {
        let to = at + vec2<f32>(shift.x, -shift.y) / 255.0;
        moved = textureSampleLevel(view_transmission_texture, view_transmission_sampler, to, 0.0).rgb
            - textureSampleLevel(view_transmission_texture, view_transmission_sampler, at, 0.0).rgb;
    }
    let fade = paint.cloak_fx.z;
    let reach = saturate(1.0 - distance(world, paint.cloak.xyz) * 100.0 / CLOAK_WAVE_REACH_CM) * CLOAK_WAVE_FALLOFF;
    let s = clamp(sin((reach + fade * fade * 12.0) * TAU), 0.0, 1.0);
    let waves = pow(s, 75.0 * 60.0) + pow(s, 60.0);
    let a = saturate(waves * fade);
    let glow = CLOAK_WAVE_COLOR * (waves * 15.0 * fade);
    return vec4<f32>(glow * a + moved * (1.0 - a), a);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let mask = pbr_input.material.base_color.a;
    let level = paint.shade.w > 0.5;
    let a = select(saturate(mask - 0.5) * 2.0, 0.0, level);
    let b = select(saturate(0.5 - mask) * 2.0, 0.0, level);
    var color = pbr_input.material.base_color.rgb;
    color = mix(color, color * paint.primary.rgb, a);
    color = mix(color, color * paint.secondary.rgb, b);

    // The game's shading (see the top).
    let metallic = paint.primary.w;
    // (The parts' metal holds 0.25 + 0.55 x the gloss mask; the level's texture sets'
    // 0.2 + 0.5 x it: tools/build_level.py.)
    let green = select(
        saturate((pbr_input.material.metallic - 0.25) / 0.55),
        saturate((pbr_input.material.metallic - 0.2) / 0.5),
        level,
    );
    let gloss = smoothstep(
        paint.gloss.z,
        max(paint.gloss.w, paint.gloss.z + 1e-4),
        srgb_to_linear(green) + paint.gloss.y,
    );
    let power = max(paint.gloss.x * gloss, 1.0);
    let coef_a = mix(vec3<f32>(PAINT_COEFFICIENT), paint.primary.rgb, metallic);
    let coef_b = mix(vec3<f32>(PAINT_COEFFICIENT), paint.secondary.rgb, metallic);
    let coefficient = mix(mix(paint.coefficient.rgb, coef_a, a), coef_b, b);
    let specular = coefficient * ((1.0 + 2.0 * metallic) * gloss);
    let luminance = saturate(dot(specular, vec3<f32>(0.3, 0.59, 0.11)));
    let diffuse = color * (paint.shade.x * (1.0 - luminance + paint.shade.y));
    // Bevy's lights give the highlights: F0 = 0.16 * reflectance^2 = specular / pi,
    // its roughness the highlight power's (GGX's alpha^2 = 2 / (n + 2)).
    let f0 = specular / PI;
    pbr_input.material.base_color = vec4<f32>(diffuse, 1.0);
    pbr_input.material.metallic = 0.0;
    pbr_input.material.reflectance = sqrt(f0 / 0.16);
    pbr_input.material.perceptual_roughness = sqrt(sqrt(2.0 / (power + 2.0)));

    // The glow.
#ifdef VERTEX_UVS_A
    let uv = in.uv;
#else
    let uv = vec2<f32>(0.0);
#endif

#ifndef PREPASS_PIPELINE
    // Cloaking (the Scout's Cloak, src/cloak.rs: TnCloakMaterialOverrider), one of
    // his meshes' copies: the overlay drawn over him as it goes on (masked, its own
    // glow and nothing else: unlit), or the cloak he is in (only the picture behind
    // him shifted, and its waves).
    if paint.cloak.w >= 0.0 {
        var cloaked: FragmentOutput;
        if paint.cloak_fx.x < 1.5 {
            let o = cloak_overlay(uv, in.world_position.xyz, pbr_input.N, pbr_input.V);
            if o.a < CLOAK_CLIP {
                discard;
            }
            cloaked.color = main_pass_post_lighting_processing(pbr_input, vec4<f32>(o.rgb, 1.0));
        } else {
            cloaked.color = cloak_glass(in, uv, normalize(in.world_normal));
        }
        return cloaked;
    }

    // A dead robot dissolving (the death sets' TnDeresser: enemies.rs): its overlay
    // drawn over it (masked, its own glow and nothing else: unlit), and where the
    // overlay isn't, the robot as it is, until the DefragValue passes
    // NormalizedTimeToMakeMeshInvisible and only the overlay is left
    // (paint_prepass.wgsl takes the same out of the depth and the shadows).
    if paint.deres.w >= 0.0 {
        let o = defrag_overlay(uv, in.world_position.xyz, pbr_input.N, pbr_input.V);
        if o.a >= CLOAK_CLIP {
            var overlaid: FragmentOutput;
            overlaid.color = main_pass_post_lighting_processing(pbr_input, vec4<f32>(o.rgb, 1.0));
            return overlaid;
        }
        if paint.deres_fx.x > paint.deres_fx.y {
            discard;
        }
    }
#endif

    let t = globals.time;
    let n = textureSample(energon_noise, energon_noise_sampler, uv + t * vec2<f32>(0.01, -0.02)).r;
    let wave = sin(sin((n + t * 0.15) * TAU) * TAU);
    let pulse = pow(max((wave + 1.0) * 0.55, 0.0), 10.0);
    if paint.energon.w > 0.5 {
        let masks = pbr_input.material.emissive.rgb;
        let own = color * (OWN_LIGHT * masks.r);
        let energon = paint.energon.rgb * ((1.0 + pulse) * ENERGON_INTENSITY * masks.g);
        pbr_input.material.emissive = vec4<f32>(own + energon, 0.0);
    }

    // An upgrade's part glowing as it goes on (ENV_BRDF_WEP_ResIn_MAT's shader code:
    // half3(SafePow(1 - dot(CameraVector, Normal), 12 * (1 - ResInValue)) *
    // ResInColor) * ResInValue, times 50, added to its emissive): a rim of the
    // colour that spreads over the whole part as the value reaches 1.
    let res_in = paint.res_in.w;
    if res_in > 0.0 {
        let camera = normalize(pbr_input.V);
        let facing = saturate(1.0 - dot(camera, pbr_input.N));
        let rim = pow(max(facing, 1e-4), 12.0 * (1.0 - res_in));
        let glow = paint.res_in.rgb * (rim * res_in * 50.0);
        pbr_input.material.emissive = vec4<f32>(pbr_input.material.emissive.rgb + glow, 0.0);
    }

    pbr_input.material.base_color.a = 1.0;
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);

    // Less Bevy's ambient specular (pbr_ambient.wgsl's, for this F0), which the
    // game hasn't.
    let n_dot_v = max(dot(pbr_input.N, pbr_input.V), 0.0001);
    let ambient_specular = EnvBRDFApprox(f0, F_AB(pbr_input.material.perceptual_roughness, n_dot_v))
        * saturate(dot(f0, vec3<f32>(50.0 * 0.33))) * lights.ambient_color.rgb * pbr_input.diffuse_occlusion;
    out.color = vec4<f32>(max(out.color.rgb - view.exposure * ambient_specular, vec3<f32>(0.0)), out.color.a);

    // The reflection of the room around it, lit as the light from the reflection's
    // way: what a white surface facing that way gets (Bevy's lights and ambient), at
    // most white, as the game's light environment's (its saturated AllLightsSH).
    if paint.shade.z >= 0.0 && max(specular.r, max(specular.g, specular.b)) > 1e-4 {
        let r = reflect(-pbr_input.V, pbr_input.N);
        var facing_r = pbr_input;
        facing_r.material.base_color = vec4<f32>(1.0);
        facing_r.material.emissive = vec4<f32>(0.0);
        facing_r.material.metallic = 0.0;
        facing_r.material.reflectance = vec3<f32>(0.0);
        facing_r.material.perceptual_roughness = 1.0;
        facing_r.N = r;
        facing_r.V = r;
        let light = min(apply_pbr_lighting(facing_r).rgb, vec3<f32>(1.0));
        let lod = paint.shade.z * max(1.0 - REFLECTION_ROUGHNESS_SCALE * log2(power), 0.0);
        let cube = textureSampleLevel(reflection_cube, reflection_sampler, ue_dir(r), lod).rgb;
        let strength = select(REFLECTION_INTENSITY, paint.coefficient.w, paint.coefficient.w > 0.0);
        let reflection = specular * strength * cube * light;
        out.color = vec4<f32>(out.color.rgb + reflection, out.color.a);
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
