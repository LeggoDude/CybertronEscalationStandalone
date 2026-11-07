//! The maps' effects, as the game draws them: its effect meshes (light shafts, glow
//! spheres, smoke cards, pin lights), the particle emitters the level starts with
//! running (smoke columns, steam vents, fires) and its lens flares.
//!
//! tools/levelfx.py reads them from the game for each map (`levels/<map>_fx.txt`,
//! `textures/levelfx/`) and compiles their materials - the game's own material graphs
//! - into `shaders/levelfx.wgsl`, one function per material picked by a shader def.
//! [`FxMaterial`] draws with that shader: a mesh, a particle sprite or a lens flare
//! element, each its material's way (additive, translucent or modulating, lit or not,
//! soft where it meets the scene).
//!
//! * **Effect meshes** are in the map's glTF with a stand-in material whose extras
//!   say `fx:<n>`; [`swap_effect_meshes`] gives them effect material `n`.
//! * **Surfaces** drawn with their game materials (the Customize garage's: its
//!   glTF materials' extras say `surface:<n>`) are lit by their light maps as the
//!   game lights them: [`SurfaceMaterial`], given by [`swap_surfaces`].
//! * **Particles** are simulated here as the game's runtime (UE3's particle emitter
//!   instances) does: each frame the particles' velocity, size, rotation rate and
//!   colour are reset to their base values, the emitter's update modules run, new
//!   particles are spawned (by the spawn rate and bursts, each going through the
//!   spawn modules), and everything moves. The modules are the game's (see
//!   tools/ue3/particles.py for their data); positions are in Unreal's axes and
//!   centimetres, as the data has them, and converted for drawing. Each emitter is
//!   an entity whose particles are drawn anew every frame: one camera-facing quad
//!   per particle (or turned along its velocity, or locked to an axis), with its
//!   sub-image, colour and dynamic parameters, into the frame's effect data
//!   (FxWords: one buffer that every emitter's and flare's vertices are read
//!   from).
//! * **Lens flares** (LensFlareSource): each element a quad in screen space along
//!   the line from the source through the middle of the screen, sized, coloured and
//!   faded by its distributions (by the source's distance from the middle of the
//!   screen and from the camera), at the source's depth. A source hidden behind the
//!   level (a ray to it hits the collision) has no flare.
//!
//! Lit particles (the smoke) take light from all around plus the sun and the map's
//! lights at the emitter (lighting.rs's map lights), as the game's light
//! environments give them.
//!
//! While the Customize screen is up ([`GarageScene`]), the effects are its garage's
//! (`levels/garage_fx.txt`, tools/build_garage.py): its beams, glows, steam and lens
//! flares, seen by the garage camera on its render layer; the map's come back as it
//! closes.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers, VisibilitySystems};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::gltf::{GltfExtras, GltfMaterialExtras};
use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, MeshTag, MeshVertexAttribute, MeshVertexBufferLayoutRef};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Buffer, BufferSize,
    BufferUsages, Face, PrimitiveTopology, RenderPipelineDescriptor, ShaderType,
    SpecializedMeshPipelineError, VertexFormat,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::storage::{GpuShaderStorageBuffer, ShaderStorageBuffer};
use bevy::render::{ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems};
use bevy::shader::{ShaderDefVal, ShaderRef};
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::Arc;

use crate::lighting::MapLighting;

/// The effects' shader (tools/levelfx.py writes it).
const SHADER: &str = "shaders/levelfx.wgsl";

/// The effects the weapons and abilities spawn (tools/build_weapon_fx.py writes
/// them), and the set their materials' shader defs name (FXM_WEAPONS_<n>) in place
/// of a map.
const WEAPON_FX: &str = "fx/weapons_fx.txt";
pub const WEAPON_SET: u8 = 200;
/// The Customize screen's garage, in a map's place (its materials' shader defs are
/// FXM_GARAGE_<n>).
pub const GARAGE_SET: u8 = 201;
const GARAGE_STEM: &str = "garage";
const GARAGE_LIGHTS: &str = "levels/garage_lights.txt";

/// The Customize screen's garage is up (customize.rs): the effects are the garage's.
#[derive(Resource, Default)]
pub struct GarageScene(pub bool);

/// FoC's gravity (WorldInfo's DefaultGravityZ), cm/s/s: HmGravity modules scale it.
const GRAVITY_Z: f32 = -2940.0;

/// Simulated before an emitter is first seen (seconds), at least: the game has been
/// running its emitters since the level loaded (and a system's own WarmupTime, if
/// longer, is used as the game uses it).
const MIN_WARMUP: f32 = 4.0;
/// The warm-up's step (UE3's).
const WARMUP_STEP: f32 = 0.032;
/// No emitter keeps more particles than this.
const MAX_PARTICLES: usize = 2000;
/// The systems whose emitters keep no more particles out at once than the game
/// cooks for them (their LOD levels' PeakActiveParticles: EmitterDef::peak), as the
/// Scatter Blaster's own effects do (fx.rs's SCATTER_MUZZLE): the shotgunners'
/// (their muzzle flash, a tracer and an impact for each of a round's eight pellets)
/// and the Scatter Blaster's Fireblast's (a tracer and an impact for each of its
/// seven). Their bursts ask for far more (an impact's sparks 0-25 and 10 more from
/// its rate, its peaks 8 and 3; a tracer's bolts 5-10, its peak 2), which the game's
/// arrays of that size never hold, and the shotgun's videos show the few streaks
/// and the light smoke that leaves. A tank's shell's blast (TankCannon_ImpactExp_FX)
/// keeps to its peaks too: its smoke asks for 25 black puffs (SmokeBurst_Export: a
/// burst of 5 and 10 a second for 2 s) and 10 dark fire-smoke balls
/// (FireBursDark_Export), each 20-50 m across by the end of its life, and keeps 7
/// and 2; drawn whole, they piled into a black cloud that hid everything past the
/// players standing in it. So do the dust the robots' landings kick up (movefx.rs:
/// Long_Fall_Landing_1_FX), a car's landing (Vehland_Low_Med_FX) and the dashes'
/// dust should it play (Long_Fall_Landing_Sm_FX, and those two, which movefx.rs
/// now leaves out of the dashes): each asks for up to 90 puffs (a burst of 0 to 90) of dust 8 to 10 m across
/// by the end of their lives, and as many sheets of it lying on the ground, 10 m
/// across and spreading to 50, and keeps 20, 3 or 3 puffs and 1 sheet. Drawn whole,
/// they laid a thick beige fog over the floor for 15 m round him for half a second
/// after every side dash; the game's videos show a thin ring of dust about his feet,
/// gone in a few tenths of a second. The Cloak's lightning (Cloaking_FX, cloak.rs)
/// keeps to its peaks too: its second sheet of arcs asks for 60 to 120 a second
/// (9 to 24 out at once) and keeps 10, its sparks 12 and 14; the game's multiplayer
/// videos show a few thin arcs about the Infiltrator as the cloak goes on and off,
/// not a crackling cage round him. The Scatter Blaster's own muzzle flash
/// (Scatterblaster_MuzzleFlash_FX, main.rs's muzzle_system) keeps to its peaks as
/// its impact and tracer do: its smoke in front asks for 12-15 puffs and keeps 5,
/// its distortion flash 0-8 and keeps 3. A car's boosters (Booster_Jazz_FX: Jazz's
/// car's and Optimus Prime's G1 truck's) keep to theirs too: each of the three
/// cones over an exhaust is a mesh in its socket's own space, 10 to 20 a second,
/// living 20 s, and the game keeps 2 of each. Every new one lay exactly on the
/// last, so a boost of a few seconds piled up a hundred or more copies of each cone
/// (about 900 over the two exhausts by 10 s), each drawn with its heat haze over
/// the whole of the picture behind it; seen from under or behind the car they
/// filled the screen and the frame rate fell.
const PEAK_CAPPED: &[&str] = &[
    "Booster_Jazz_FX",
    "Cloaking_FX",
    "Scatterblaster_MuzzleFlash_FX",
    "TurretArm_MuzzleFlash_FX",
    "Trail_ScatterBlaster_SP_FX",
    "Squib_ScatterBlaster_FX",
    "Squib_ScatterBlaster_SP_FX",
    "TankCannon_ImpactExp_FX",
    "Long_Fall_Landing_1_FX",
    "Long_Fall_Landing_Sm_FX",
    "Vehland_Low_Med_FX",
];

/// Light from all around on lit effects (the game's units), as on characters.
const AMBIENT: f32 = 0.22;

/// A particle's or flare's own vertex data: the dynamic parameters (particles) or a
/// flare's intensity, occlusion, radial distance and ray distance.
pub const ATTRIBUTE_FX_PARAM: MeshVertexAttribute =
    MeshVertexAttribute::new("FxParam", 0x1e5f_0001, VertexFormat::Float32x4);
/// A particle's sub-image blend (x); a flare's light's depth and distance from the
/// camera (x, y; cm) and where the light is on the screen (zw: 0..1, down).
pub const ATTRIBUTE_FX_EXTRA: MeshVertexAttribute =
    MeshVertexAttribute::new("FxExtra", 0x1e5f_0002, VertexFormat::Float32x4);

pub struct LevelFxPlugin;

impl Plugin for LevelFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<FxMaterial>::default())
            .add_plugins(MaterialPlugin::<SurfaceMaterial>::default())
            .add_systems(Startup, split_shaders)
            .init_resource::<LevelFx>()
            .init_resource::<FxSystems>()
            .init_resource::<FxQuality>()
            .init_resource::<FxBudget>()
            .init_resource::<GarageScene>()
            .init_resource::<FxSlots>()
            .init_resource::<FxWords>()
            .add_systems(Startup, make_fx_words)
            // What spawns entities with materials comes before Bevy looks for meshes
            // whose materials need their pipelines made (or an entity could be drawn
            // before it has one).
            .add_systems(
                PostUpdate,
                (
                    follow_map,
                    swap_effect_meshes,
                    swap_surfaces,
                    start_spawned_fx,
                    make_twins,
                )
                    .chain()
                    .before(bevy::pbr::check_entities_needing_specialization::<FxMaterial>)
                    .before(bevy::pbr::check_entities_needing_specialization::<SurfaceMaterial>)
                    .before(bevy::pbr::check_entities_needing_specialization::<StandardMaterial>),
            )
            // A moving effect's meshes go where it is this frame before the
            // transforms are propagated (or it is drawn where it was the frame
            // before), and an effect is run the frame it starts.
            .add_systems(
                PostUpdate,
                place_spawned_fx.before(bevy::transform::TransformSystems::Propagate),
            )
            // An emitter or flare mesh shown or hidden as it draws is shown or hidden
            // that frame: they draw before the frame's visibility is worked out.
            .add_systems(
                PostUpdate,
                (
                    simulate_emitters
                        .after(roll_fx_budget)
                        .before(VisibilitySystems::VisibilityPropagate),
                    draw_flares.before(VisibilitySystems::VisibilityPropagate),
                    run_spawned_fx
                        .after(start_spawned_fx)
                        .after(roll_fx_budget)
                        .before(VisibilitySystems::VisibilityPropagate)
                        .in_set(SpawnedFxRun),
                    despawn_retired.before(run_spawned_fx),
                    roll_fx_budget,
                    gather_fx_words
                        .after(simulate_emitters)
                        .after(draw_flares)
                        .after(run_spawned_fx),
                )
                    .after(bevy::transform::TransformSystems::Propagate),
            );
        // The frame's effect data, taken from the main world and written to its
        // buffer (FxWords).
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<FxWordsRender>()
            .add_systems(ExtractSchedule, extract_fx_words)
            .add_systems(
                Render,
                write_fx_words.in_set(RenderSystems::PrepareResources),
            );
    }
}

// ---------------------------------------------------------------------------
// The material
// ---------------------------------------------------------------------------

/// The light on a lit effect (FxParams in the shader): game units, Unreal's axes.
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct FxParams {
    /// rgb: light from all around.
    pub ambient: Vec4,
    /// rgb: the sun's light.
    pub sun: Vec4,
    /// xyz: towards the sun.
    pub sun_dir: Vec4,
    /// x: the opacity mask's clip value.
    pub misc: Vec4,
    /// rgb: the level's lights gathered into one, from `light_dir` (see light_at).
    pub light: Vec4,
    /// xyz: towards that light.
    pub light_dir: Vec4,
}

/// What an effect material's pipeline is made for: the map and material (its shader
/// def), how it blends, and what it draws.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FxKey {
    /// Index into MAPS (or WEAPON_SET).
    pub map: u8,
    pub mat: u16,
    pub blend: Blend,
    pub lit: bool,
    pub two_sided: bool,
    pub kind: Kind,
    /// Which part of a modulating material this draws (see ModPart).
    pub part: ModPart,
    /// Seen through the camera's fog (not: the far backdrop, Oblivion's space).
    pub fog: bool,
    /// A far piece of the sky, seen through the fog its vertex colours carry, as
    /// the game works it out at each vertex (FX_SKYFOG; the map's effect meshes
    /// with "skyfog": Ignition's moons' glow spheres), not the camera's.
    pub sky_fog: bool,
    /// It samples a cube map, FxMaterial's `cube`: its own (its material's cube map
    /// samples) or, for an effect mesh reflecting it (its material's
    /// ReflectionCubeMapSample), the map's.
    pub cube: bool,
    /// It shifts the picture behind it (its material's Distortion: FX_DISTORT):
    /// its Distort part, and a modulating material's bright part, which adds the
    /// picture as the shift moves it.
    pub distort: bool,
    /// Its material distorts (bUsesDistortion), so what it draws (but its Distort
    /// part) is clipped where its opacity is under a 255th (FX_OPACITY_CLIP).
    pub distorting: bool,
    /// Its vertices are in the frame's effect data (FxWords), its mesh only
    /// numbering them (FX_PULLED): particles', trails', decals' and flares'.
    pub pulled: bool,
}

/// A modulating material multiplies the picture by its colour, which may be more
/// than 1. Layered (an explosion's twenty glow sprites on top of each other) that
/// runs away past what the picture can hold, so spawned effects draw it in two
/// parts: the colour up to 1 multiplied in (Dark), and what is over 1 added as that
/// much more of the scene behind (Bright, from the scene's copy). One layer comes
/// out as the multiply would; more add up instead of multiplying.
///
/// A distorting material's distortion is a part of its own (Distort), drawn just
/// before the rest of it: see FX_DISTORT_PART and distort_blend. And AlphaRestore
/// is no material's: the picture's alpha put back once everything is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ModPart {
    #[default]
    Whole,
    Dark,
    Bright,
    Distort,
    AlphaRestore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Blend {
    Translucent,
    Additive,
    Modulate,
    Masked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Mesh,
    /// Sprites, trails and decals.
    Particle,
    /// Mesh particles (the particle's colour is the material's MeshEmitterVertexColor).
    MeshParticle,
    Flare,
}

/// One of a map's effect materials (its game material, compiled into
/// shaders/levelfx.wgsl), for meshes, particles or flares.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(FxKey)]
pub struct FxMaterial {
    #[uniform(0)]
    params: FxParams,
    #[texture(1)]
    #[sampler(2)]
    tex0: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    tex1: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    tex2: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    tex3: Handle<Image>,
    #[texture(9)]
    #[sampler(10)]
    tex4: Handle<Image>,
    #[texture(11)]
    #[sampler(12)]
    tex5: Handle<Image>,
    /// The cube map it samples (with FxKey's `cube`): its material's own, or the
    /// map's, as its characters reflect it, for an effect mesh reflecting that.
    #[texture(15, dimension = "cube")]
    #[sampler(16)]
    cube: Option<Handle<Image>>,
    /// The frame's effect data (FxWords), which `pulled` meshes' vertices are read
    /// from: always FX_WORDS_BUFFER.
    #[storage(17, read_only, visibility(vertex))]
    words: Handle<ShaderStorageBuffer>,
    key: FxKey,
    alpha: AlphaMode,
    /// Where it is drawn among materials as far away (Material::depth_bias): an
    /// effect's emitters' meshes all stand where the effect does (EMITTER_ORDER).
    order: f32,
}

impl From<&FxMaterial> for FxKey {
    fn from(material: &FxMaterial) -> FxKey {
        material.key
    }
}

impl Material for FxMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha
    }

    fn depth_bias(&self) -> f32 {
        self.order
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// The material's own code (its shader def), its blend, and the vertex data
    /// meshes or sprites bring.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let k = key.bind_group_data;
        let mut defs: Vec<ShaderDefVal> = vec![
            format!("FXM_{}_{}", set_stem(k.map).to_uppercase(), k.mat).into(),
            match k.blend {
                Blend::Translucent => "FX_TRANSLUCENT",
                Blend::Additive => "FX_ADDITIVE",
                Blend::Modulate => "FX_MODULATE",
                Blend::Masked => "FX_MASKED",
            }
            .into(),
        ];
        // (A pulled one's mesh is a bucket: its vertices' numbers, in their
        // places' x.)
        let mut attributes = vec![Mesh::ATTRIBUTE_POSITION.at_shader_location(0)];
        if k.pulled {
            defs.push("FX_PULLED".into());
        } else {
            attributes.extend([
                Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
                Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
                Mesh::ATTRIBUTE_UV_1.at_shader_location(3),
                Mesh::ATTRIBUTE_TANGENT.at_shader_location(4),
                Mesh::ATTRIBUTE_COLOR.at_shader_location(5),
            ]);
            if k.kind != Kind::Mesh {
                attributes.push(ATTRIBUTE_FX_PARAM.at_shader_location(6));
                attributes.push(ATTRIBUTE_FX_EXTRA.at_shader_location(7));
            }
        }
        if k.kind != Kind::Mesh {
            defs.push("FX_SPRITE".into());
        }
        if k.kind == Kind::Flare {
            defs.push("FX_FLARE".into());
        }
        if k.kind == Kind::MeshParticle {
            defs.push("FX_MESH_PARTICLE".into());
        }
        match k.part {
            ModPart::Dark => defs.push("FX_MODULATE_DARK".into()),
            ModPart::Bright => defs.push("FX_MODULATE_BRIGHT".into()),
            ModPart::Distort => defs.push("FX_DISTORT_PART".into()),
            ModPart::AlphaRestore => defs.push("FX_ALPHA_RESTORE".into()),
            ModPart::Whole => {}
        }
        if k.lit {
            defs.push("FX_LIT".into());
        }
        if k.sky_fog {
            defs.push("FX_SKYFOG".into());
        } else if !k.fog {
            defs.push("FX_NOFOG".into());
        }
        if k.cube {
            defs.push("FX_CUBE".into());
        }
        if k.distort {
            defs.push("FX_DISTORT".into());
        }
        if k.distorting && k.part != ModPart::Distort {
            defs.push("FX_OPACITY_CLIP".into());
        }
        descriptor.vertex.buffers = vec![layout.0.get_layout(&attributes)?];
        descriptor.vertex.shader_defs.extend(defs.iter().cloned());
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.extend(defs);
        }
        use_material_shader(descriptor, k.map, k.mat);
        if k.two_sided || !matches!(k.kind, Kind::Mesh | Kind::MeshParticle) {
            descriptor.primitive.cull_mode = None;
        }
        // (The distortion's blend, a modulating part keeping the alpha the
        // distortion parts leave, and the alpha put back: distort_blend.)
        let blend = match k.part {
            ModPart::Distort => Some(distort_blend()),
            ModPart::Dark => descriptor
                .fragment
                .as_ref()
                .and_then(|f| f.targets.first().cloned().flatten())
                .and_then(|t| t.blend)
                .map(|b| BlendState {
                    alpha: KEEP_DESTINATION,
                    ..b
                }),
            ModPart::AlphaRestore => Some(BlendState {
                color: KEEP_DESTINATION,
                alpha: BlendComponent::REPLACE,
            }),
            _ => None,
        };
        if k.part == ModPart::AlphaRestore {
            descriptor.primitive.cull_mode = None;
        }
        if let Some(blend) = blend
            && let Some(target) = descriptor
                .fragment
                .as_mut()
                .and_then(|f| f.targets.first_mut())
                .and_then(|t| t.as_mut())
        {
            target.blend = Some(blend);
        }
        Ok(())
    }
}

/// A blend that leaves what is drawn as it is.
const KEEP_DESTINATION: BlendComponent = BlendComponent {
    src_factor: BlendFactor::Zero,
    dst_factor: BlendFactor::One,
    operation: BlendOperation::Add,
};

/// How a distortion part (FX_DISTORT_PART) is drawn: its shift of the picture (the
/// scene's copy moved, less the copy) added times the picture's alpha, which starts
/// at 1 (everything opaque leaves it so) and loses the part's alpha's share (as far
/// as its shift goes towards a 255th of the screen). UE3 moves the picture once by
/// the sum of every distorting layer's shift, clamped; each layer moving the copy by
/// its own and adding the difference instead summed those differences, and twenty
/// overlapping layers (the energon splashed across the screen,
/// Screen_EnergonBloodsplatter_FX) took a dark shift twenty times over, to black.
/// So the first layers to shift a place move it, and those after only as far as
/// the alpha has room left; the modulating parts drawn between keep the alpha, the
/// see-through ones raise it as they cover what was moved, and AlphaRestore puts
/// it back to 1 at the end.
fn distort_blend() -> BlendState {
    BlendState {
        color: BlendComponent {
            src_factor: BlendFactor::DstAlpha,
            dst_factor: BlendFactor::One,
            operation: BlendOperation::Add,
        },
        alpha: BlendComponent {
            src_factor: BlendFactor::Zero,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        },
    }
}

/// A surface of the level drawn with its game material (FX_SURFACE in the shader):
/// opaque, lit by its light map the way the game's base pass lights it (its
/// directional light map shader, read from the game's compiled shaders). The light
/// map keeps its light from three directions round the surface's normal (UE3's
/// light map basis), so the material's normal map shades it, its highlight (the
/// material's specular colour and power: High Moon's normalized Blinn-Phong) shines
/// where most of the light comes from, and it reflects the level's captured cube map
/// (`cube`) as far as its specular colour and ReflectionColor say, blurred by its
/// power. Bevy's Lightmap holds the light as it reaches a surface facing along its
/// normal (`levels/<page>.png`); `directions` (`levels/<page>_dir.png`,
/// tools/build_level.py) how it is shared between the three directions. Its
/// textures and shader def are its effect material's (levelfx.py writes both alike).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(SurfaceKey)]
pub struct SurfaceMaterial {
    #[uniform(0)]
    params: FxParams,
    #[texture(1)]
    #[sampler(2)]
    tex0: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    tex1: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    tex2: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    tex3: Handle<Image>,
    #[texture(9)]
    #[sampler(10)]
    tex4: Handle<Image>,
    #[texture(11)]
    #[sampler(12)]
    tex5: Handle<Image>,
    #[texture(13)]
    #[sampler(14)]
    directions: Handle<Image>,
    #[texture(15, dimension = "cube")]
    #[sampler(16)]
    cube: Handle<Image>,
    key: SurfaceKey,
}

/// A surface material's pipeline: its map and material (its shader def), and which
/// faces it culls (CULL_*).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceKey {
    pub map: u8,
    pub cull: u8,
    pub mat: u16,
}

/// SurfaceKey::cull: the back faces (a surface as most are), the front faces (a
/// mirrored one: its triangles wind the other way round, so the glTF loader gives it
/// a material culling those, which its surface material keeps) or none (two-sided).
const CULL_BACK: u8 = 0;
const CULL_FRONT: u8 = 1;
const CULL_NONE: u8 = 2;

impl From<&SurfaceMaterial> for SurfaceKey {
    fn from(material: &SurfaceMaterial) -> SurfaceKey {
        material.key
    }
}

impl Material for SurfaceMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn enable_shadows() -> bool {
        false
    }

    /// Its shader def and vertex data (levelfx.rs's); its depth prepass is Bevy's
    /// own, culling the faces it culls but otherwise left as it is.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let k = key.bind_group_data;
        descriptor.primitive.cull_mode = match k.cull {
            CULL_FRONT => Some(Face::Front),
            CULL_NONE => None,
            _ => Some(Face::Back),
        };
        let prepass: ShaderDefVal = "PREPASS_PIPELINE".into();
        if descriptor.vertex.shader_defs.contains(&prepass) {
            return Ok(());
        }
        let mut defs: Vec<ShaderDefVal> = vec![
            format!("FXM_{}_{}", set_stem(k.map).to_uppercase(), k.mat).into(),
            "FX_SURFACE".into(),
        ];
        if k.cull == CULL_NONE {
            defs.push("FX_TWO_SIDED".into());
        }
        let attributes = vec![
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            Mesh::ATTRIBUTE_UV_1.at_shader_location(3),
            Mesh::ATTRIBUTE_TANGENT.at_shader_location(4),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(5),
        ];
        descriptor.vertex.buffers = vec![layout.0.get_layout(&attributes)?];
        descriptor.vertex.shader_defs.extend(defs.iter().cloned());
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.extend(defs);
        }
        use_material_shader(descriptor, k.map, k.mat);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Each material's own shader
// ---------------------------------------------------------------------------

/// shaders/levelfx.wgsl holds every material's function (each under its shader
/// def, FXM_<SET>_<n>), and Bevy works through the whole file (650 KB, with the
/// weapons' 214 materials) for every pipeline it makes from it, one pipeline at a
/// time: some 55 ms each, most of it skipping the other materials' code. With an
/// effect pipeline for each material, part and blend, that held up everything
/// else being made (the menu, the Customize screen's garage and robots) for
/// seconds as the game started and as the screen opened. So each material gets a
/// shader of its own, made here as the game starts: the file's shared code with
/// only its own function in it (and the comments left out), some 17 KB, which
/// takes a sixth of the time. Its pipelines use it (use_material_shader); a
/// material without one uses the whole file, as before.
static SPLIT: std::sync::OnceLock<std::collections::HashSet<(u8, u16)>> =
    std::sync::OnceLock::new();

/// The material shaders' ids: fixed, so a pipeline can name its material's shader
/// from its key alone.
const SPLIT_UUID: u128 = 0x7a1c_e05d_3b92_4f61_8d2e_0000_0000_0000;

fn split_handle(map: u8, mat: u16) -> Handle<Shader> {
    Handle::Uuid(
        bevy::asset::uuid::Uuid::from_u128(SPLIT_UUID | ((map as u128) << 16) | mat as u128),
        std::marker::PhantomData,
    )
}

/// A material's pipeline compiles its own shader, if it has one.
fn use_material_shader(descriptor: &mut RenderPipelineDescriptor, map: u8, mat: u16) {
    if !SPLIT.get().is_some_and(|made| made.contains(&(map, mat))) {
        return;
    }
    let shader = split_handle(map, mat);
    descriptor.vertex.shader = shader.clone();
    if let Some(fragment) = descriptor.fragment.as_mut() {
        fragment.shader = shader;
    }
}

/// shaders/levelfx.wgsl taken apart: the code every material shares, and each
/// material's (a top-level `#ifdef FXM_<SET>_<n>` block), as lines.
struct SplitSource<'a> {
    /// The shared lines before each block (the last entry: after the last).
    shared: Vec<Vec<&'a str>>,
    /// Each block's def and lines.
    blocks: Vec<(&'a str, Vec<&'a str>)>,
}

impl<'a> SplitSource<'a> {
    fn new(source: &'a str) -> SplitSource<'a> {
        let mut shared = vec![Vec::new()];
        let mut blocks: Vec<(&str, Vec<&str>)> = Vec::new();
        let mut depth = 0usize;
        let mut in_block = false;
        for line in source.lines() {
            let t = line.trim_start();
            // (Comments and blank lines left out.)
            if t.is_empty() || t.starts_with("//") {
                continue;
            }
            let opens = t.starts_with("#if");
            let closes = t.starts_with("#endif");
            if depth == 0
                && opens
                && let Some(def) = t.strip_prefix("#ifdef FXM_")
            {
                let def = def.split_whitespace().next().unwrap_or("");
                blocks.push((def, Vec::new()));
                in_block = true;
            }
            if in_block {
                blocks.last_mut().unwrap().1.push(line);
            } else {
                shared.last_mut().unwrap().push(line);
            }
            if opens {
                depth += 1;
            } else if closes {
                depth = depth.saturating_sub(1);
                if depth == 0 && in_block {
                    in_block = false;
                    shared.push(Vec::new());
                }
            }
        }
        SplitSource { shared, blocks }
    }

    /// The shader for block `i`: the shared code with that block where it was.
    fn shader(&self, i: usize) -> String {
        let mut out = String::new();
        for (j, part) in self.shared.iter().enumerate() {
            for line in part {
                out.push_str(line);
                out.push('\n');
            }
            if j == i {
                for line in &self.blocks[i].1 {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        out
    }
}

/// Makes each material's shader (see SPLIT), before any pipeline is made.
fn split_shaders(mut shaders: ResMut<Assets<Shader>>) {
    let path = crate::assets_dir().join(SHADER);
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(e) => {
            warn!("Couldn't read {}: {e}", path.display());
            return;
        }
    };
    let split = SplitSource::new(&source);
    // (Each set's name in its defs: FXM_OBLIVION_3, FXM_WEAPONS_12, FXM_GARAGE_7.)
    let sets: HashMap<String, u8> = (0..crate::MAPS.len() as u8)
        .chain([WEAPON_SET, GARAGE_SET])
        .map(|set| (set_stem(set).to_uppercase(), set))
        .collect();
    let mut made = std::collections::HashSet::new();
    // (Every one imports what the shared code does: worked out once.)
    let imports = Shader::from_wgsl(split.shader(usize::MAX), SHADER).imports;
    for (i, (def, lines)) in split.blocks.iter().enumerate() {
        let Some((set, mat)) = def
            .rsplit_once('_')
            .and_then(|(set, mat)| Some((*sets.get(set)?, mat.parse::<u16>().ok()?)))
        else {
            continue;
        };
        let text = split.shader(i);
        let name = format!("shaders/levelfx/{}_{mat}.wgsl", set_stem(set));
        let shader = if lines.iter().any(|l| l.trim_start().starts_with("#import")) {
            Shader::from_wgsl(text, name)
        } else {
            Shader {
                path: name.clone(),
                source: bevy::shader::Source::Wgsl(text.into()),
                import_path: bevy::shader::ShaderImport::AssetPath(name),
                imports: imports.clone(),
                additional_imports: Vec::new(),
                shader_defs: Vec::new(),
                file_dependencies: Vec::new(),
                validate_shader: bevy::shader::ValidateShader::Disabled,
            }
        };
        if shaders.insert(split_handle(set, mat).id(), shader).is_ok() {
            made.insert((set, mat));
        }
    }
    info!(
        "Effect shaders: {} materials' own, from {}",
        made.len(),
        path.display()
    );
    let _ = SPLIT.set(made);
}

/// The stem a material set's shader defs name: a map's, the weapons' or the garage's.
fn set_stem(map: u8) -> &'static str {
    if map == WEAPON_SET {
        "weapons"
    } else if map == GARAGE_SET {
        GARAGE_STEM
    } else {
        crate::MAPS
            .get(map as usize)
            .map(|m| m.stem)
            .unwrap_or("none")
    }
}

// ---------------------------------------------------------------------------
// Distributions
// ---------------------------------------------------------------------------

/// A distribution as the game cooks it (ue3/particles.py's Dist): a constant, a
/// curve, or (op 2) a random pick between two (curves of) values, or (op 3) one of
/// the two. Curves start with min, max, start time and time scale.
#[derive(Clone, Debug, Default)]
struct Dist {
    kind: u8,
    op: u8,
    table: Vec<f32>,
}

impl Dist {
    fn constant(v: f32) -> Dist {
        Dist {
            kind: 1,
            op: 1,
            table: vec![v],
        }
    }

    fn width(&self) -> usize {
        if self.kind >= 7 { 3 } else { 1 }
    }

    fn is_curve(&self) -> bool {
        matches!(self.kind, 2 | 4 | 8 | 10)
    }

    /// Its value at time `t`, with `r` the random numbers a pick uses (one per
    /// component; op 3 uses the first for all).
    fn eval(&self, t: f32, r: [f32; 3]) -> [f32; 3] {
        let w = self.width();
        let pair = self.op >= 2;
        let stride = if pair { 2 * w } else { w };
        let head = if self.is_curve() { 4 } else { 0 };
        let mut out = [0.0; 3];
        if self.table.len() < head + stride || stride == 0 {
            if let Some(v) = self.table.first() {
                out = [*v; 3];
            }
            return out;
        }
        let entries = (self.table.len() - head) / stride;
        let (i0, i1, a) = if head > 0 && entries > 1 {
            let x = ((t - self.table[2]) * self.table[3]).max(0.0);
            let i0 = (x.floor() as usize).min(entries - 1);
            let i1 = (i0 + 1).min(entries - 1);
            (i0, i1, (x - i0 as f32).clamp(0.0, 1.0))
        } else {
            (0, 0, 0.0)
        };
        let value = |entry: usize, c: usize| -> f32 {
            let base = head + entry * stride;
            let lo = self.table[base + c];
            if !pair {
                return lo;
            }
            let hi = self.table[base + w + c];
            if self.op == 3 {
                if r[0] < 0.5 { lo } else { hi }
            } else {
                lo + (hi - lo) * r[c]
            }
        };
        for (c, o) in out.iter_mut().enumerate().take(w) {
            let v0 = value(i0, c);
            let v1 = value(i1, c);
            *o = v0 + (v1 - v0) * a;
        }
        if w == 1 {
            out = [out[0]; 3];
        }
        out
    }

    fn float(&self, t: f32, rng: &mut Rng) -> f32 {
        self.eval(t, [rng.next(), 0.0, 0.0])[0]
    }

    /// The most it can be (anywhere on its curve, at either end of a pick).
    fn highest(&self) -> f32 {
        let head = if self.is_curve() { 4 } else { 0 };
        match self.table.get(head..) {
            Some(values) if !values.is_empty() => values.iter().copied().fold(f32::MIN, f32::max),
            _ => self.table.first().copied().unwrap_or(0.0),
        }
    }

    fn float_r(&self, t: f32, r: f32) -> f32 {
        self.eval(t, [r, r, r])[0]
    }

    fn vector(&self, t: f32, rng: &mut Rng) -> Vec3 {
        Vec3::from(self.eval(t, [rng.next(), rng.next(), rng.next()]))
    }

    fn parse(words: &[&str]) -> Option<Dist> {
        let kind: u8 = words.first()?.parse().ok()?;
        let op: u8 = words.get(1)?.parse().ok()?;
        let n: usize = words.get(2)?.parse().ok()?;
        let table: Vec<f32> = words
            .iter()
            .skip(3)
            .take(n)
            .filter_map(|w| w.parse().ok())
            .collect();
        (table.len() == n).then_some(Dist { kind, op, table })
    }
}

/// A small random number generator (xorshift), one per emitter.
#[derive(Clone, Copy, Debug)]
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Rng {
        Rng(seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453) | 1)
    }

    /// 0..1.
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

// ---------------------------------------------------------------------------
// The effects file (levels/<map>_fx.txt)
// ---------------------------------------------------------------------------

struct MaterialDef {
    /// The game's name for it.
    name: String,
    blend: Blend,
    lit: bool,
    two_sided: bool,
    scene: bool,
    /// It shifts the picture behind it ("distort=1": its Distortion), which takes
    /// the scene's copy as `scene` does.
    distort: bool,
    /// Drawn in the fog ("nofog=1": the far backdrop, outside it).
    fog: bool,
    clip: f32,
    /// (path, sRGB, address U, address V)
    textures: Vec<(String, bool, ImageAddressMode, ImageAddressMode)>,
    /// How much its specular power blurs the level's reflection cube map (its
    /// ReflectionCubeMapSample's RoughnessScale; 0 without one).
    cube_roughness: f32,
    /// It has a ReflectionCubeMapSample ("cuberough=").
    reflects: bool,
    /// The cube map its cube samples sample ("cubetex=<path>": Oblivion's windows'
    /// ENV_Metals_CUBEMAP).
    cube_texture: Option<String>,
}

/// A module of an emitter: its type (ue3/particles.py's MODULES), distributions,
/// in-place words and (ColorByParameter) parameter name.
#[derive(Default)]
struct ModuleDef {
    kind: u16,
    floats: Vec<Dist>,
    vectors: Vec<Dist>,
    words: Vec<u32>,
    name: Option<String>,
}

impl ModuleDef {
    fn f(&self, i: usize) -> &Dist {
        static ZERO: std::sync::OnceLock<Dist> = std::sync::OnceLock::new();
        self.floats
            .get(i)
            .unwrap_or_else(|| ZERO.get_or_init(|| Dist::constant(0.0)))
    }

    fn v(&self, i: usize) -> &Dist {
        self.vectors.get(i).unwrap_or_else(|| self.f(usize::MAX))
    }

    fn word(&self, i: usize) -> u32 {
        self.words.get(i).copied().unwrap_or(0)
    }

    fn flag(&self, i: usize) -> bool {
        self.word(i) & 1 != 0
    }
}

/// What an emitter draws (its TypeDataModule).
#[derive(Clone, Debug, Default)]
enum TypeData {
    /// Camera-facing (or turned) quads.
    #[default]
    Sprite,
    /// A ribbon through its particles, oldest at the tail (Trail2): `tess` points
    /// between each two, the texture `tile`d along it, at most `max` particles.
    Trail { tess: u32, tile: f32, max: usize },
    /// A mesh per particle (index into FxData::meshes), sized and turned by it.
    Mesh(usize),
    /// High Moon's decals: each particle a mark laid on whatever lies along `dir`
    /// (the emitter's frame) from it, sized by its size (x, y; z how far it looks).
    Decal(Dist),
    /// A beam per particle (Beam2), at most `max` at a time, from `source` to
    /// `target` (the emitter's frame), or to where the game sets it (the system's
    /// beam target: a tracer's end, SetTracerDistance). See [`Beam`].
    Beam(Box<Beam>),
}

/// A beam emitter's settings (ParticleModuleTypeDataBeam2, with its BeamSource and
/// BeamTarget modules): each beam a ribbon from its source to its target at once
/// (Speed 0), facing the camera, as wide as twice its particle's size (x) times its
/// taper there (TaperMethod Full: TaperFactor times TaperScale, both along the beam,
/// source to target: TaperScale's own notes have a curve map 0 and 1 to the source
/// and the target), the texture `tile`d along it, bent by its `waves`.
#[derive(Clone, Debug)]
struct Beam {
    max: usize,
    tile: f32,
    taper: bool,
    factor: Dist,
    scale: Dist,
    source: Vec3,
    target: Vec3,
    waves: Vec<BeamWave>,
}

/// A wave bending a beam (High Moon's ParticleModuleBeamSineWave, see
/// tools/ue3/particles.py): `amplitude` units along `direction` in the beam's frame
/// (z along the beam, x and y across it), a turn every `period` units along it,
/// running along it at `speed` units a second, `phase` turns in.
#[derive(Clone, Debug)]
struct BeamWave {
    amplitude: f32,
    period: f32,
    speed: f32,
    phase: f32,
    direction: Vec3,
}

/// A mesh drawn by mesh emitters (models/fx/<name>.fxmesh, tools/levelfx.py):
/// Unreal's axes and units.
#[derive(Default)]
struct FxMesh {
    pos: Vec<Vec3>,
    nrm: Vec<Vec3>,
    tan: Vec<Vec4>,
    uv: Vec<Vec2>,
    idx: Vec<u32>,
}

impl FxMesh {
    fn load(path: &std::path::Path) -> Option<FxMesh> {
        let b = std::fs::read(path).ok()?;
        if b.len() < 12 || &b[..4] != b"FXM1" {
            return None;
        }
        let u = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let f = |o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let (nv, ni) = (u(4) as usize, u(8) as usize);
        let stride = 12 * 4;
        if b.len() < 12 + nv * stride + ni * 4 {
            return None;
        }
        let mut m = FxMesh::default();
        for k in 0..nv {
            let o = 12 + k * stride;
            let v = |c: usize| f(o + c * 4);
            m.pos.push(Vec3::new(v(0), v(1), v(2)));
            m.nrm.push(Vec3::new(v(3), v(4), v(5)));
            m.tan.push(Vec4::new(v(6), v(7), v(8), v(9)));
            m.uv.push(Vec2::new(v(10), v(11)));
        }
        let o = 12 + nv * stride;
        m.idx = (0..ni)
            .map(|k| u(o + k * 4))
            .filter(|&i| (i as usize) < nv)
            .collect();
        Some(m)
    }
}

/// An emitter's settings (its ParticleModuleRequired) and modules.
#[derive(Default)]
struct EmitterDef {
    /// Its name (EmitterName), by which others of its system spawn at its particles
    /// (LocationEmitter), and whether any does.
    name: String,
    is_source: bool,
    material: usize,
    typedata: TypeData,
    /// Its particles go at once when the system is turned off.
    kill_on_deactivate: bool,
    /// Its particles go at once when its loops are done ("killdone=1": its
    /// bKillOnCompleted), however long they had left to live.
    kill_on_completed: bool,
    /// It is run but not drawn ("draw=0"): an emitter the game itself can't draw
    /// whose particles others follow (LocationEmitter, LocationEmitterDirect), the
    /// Snipers' tracer's Bolt.
    undrawn: bool,
    align: u8,
    local: bool,
    duration: f32,
    duration_low: f32,
    duration_range: bool,
    duration_recalc: bool,
    loops: u32,
    delay: f32,
    delay_first: bool,
    sub_h: u32,
    sub_v: u32,
    interp: u8,
    random_time: f32,
    sort: u8,
    camera_offset: f32,
    max_draw: usize,
    probability: f32,
    lock: u8,
    scale_uv: bool,
    /// The most particles the game has out of it at once (its LOD level's
    /// PeakActiveParticles), 0 where the file doesn't say. A decal emitter lays no
    /// more marks than that (1 for every one of the weapons': one scorch a blast,
    /// where their spawn rates would lay 4 to 20, one on another, darkening the
    /// wall to black), nor does one whose particles never die (`immortal`) keep
    /// more out.
    peak: usize,
    /// Its particles never die (it has no lifetime, or one of 0): a glow that stays
    /// while its effect is on, as a jet's hover jets' and afterburners' meshes are.
    /// It keeps its peak of them out (the game cooks 1 for those), spawning no more
    /// while they last, rather than piling copies up in one place (a hover jet's
    /// spawns 100 a second), and they go when the effect is turned off, nothing else
    /// ending them.
    immortal: bool,
    /// It keeps to its peak whatever its particles' lives (its system is one of
    /// PEAK_CAPPED's): a burst asking for more spawns no more than its peak allows.
    capped: bool,
    rate: Dist,
    bursts: Vec<(i32, i32, f32)>,
    modules: Vec<Option<ModuleDef>>,
    spawn: Vec<usize>,
    update: Vec<usize>,
    /// The modules with a say in how many particles spawn (UE3's SpawningModules:
    /// SpawnPerUnit; LocationEmitter is listed too, and only places them).
    spawning: Vec<usize>,
}

/// ParticleModuleSpawnPerUnit (ue3/particles.py's SPAWN_PER_UNIT): particles by the
/// distance the emitter moves.
const SPAWN_PER_UNIT: u16 = 0x81;
/// ParticleModuleSizeMultiplyLife by a vector parameter (ue3/particles.py's
/// SIZE_BY_PARAMETER): the particles' size times what the game sets.
const SIZE_BY_PARAMETER: u16 = 0x82;
/// High Moon's ParticleModuleSwitchableColorScaleOverLife (ue3/particles.py's
/// SWITCH_COLOR_SCALE): ColorScaleOverLife over each particle's life, its curves
/// (alpha and colour) A or B by the effect's module channel (MODULE_CHANNEL).
const SWITCH_COLOR_SCALE: u16 = 0x83;
/// ParticleModuleLocationEmitter: each particle where one of another emitter's is.
const LOCATION_EMITTER: u16 = 0x13;
/// ParticleModuleLocationEmitterDirect: each particle where the other emitter's
/// particle of its own place in the list is, as fast and as old, from spawning on.
const LOCATION_EMITTER_DIRECT: u16 = 0x14;
/// ParticleModuleLifetime: how long each particle lives (0: for ever).
const LIFETIME: u16 = 0x01;

struct SystemDef {
    name: String,
    warmup: f32,
    lit: bool,
    emitters: Vec<EmitterDef>,
    /// Its less detailed levels (spawned effects): from how far from the camera
    /// (cm) each is used, and the system that is that level (UE3's LODDistances).
    lods: Vec<(f32, usize)>,
}

/// Where the level puts a particle system: its origin and axes (scaled; rows: the
/// local X, Y and Z in the world) in Unreal's axes and units, and the colours its
/// emitter instance parameters set.
struct Placement {
    system: usize,
    origin: Vec3,
    axes: [Vec3; 3],
    params: Vec<(String, [u8; 4])>,
    /// The game lights it: its particle component accepts lights and has a light
    /// environment (the place line's last field, tools/levelfx.py). Otherwise its
    /// lit materials take no light, as what the game spawns (spawned_material).
    lit: bool,
}

struct FlareElement {
    source: bool,
    material: usize,
    ray: f32,
    size: Vec3,
    normalize_radial: bool,
    use_source_distance: bool,
    /// scaling, axis, rotation, color, alpha, offset, distscale, distcolor, distalpha
    dists: HashMap<String, Dist>,
}

struct FlareDef {
    screen_map: Dist,
    elements: Vec<FlareElement>,
}

struct FlarePlacement {
    flare: usize,
    origin: Vec3,
    color: Vec4,
    size: f32,
}

#[derive(Default)]
struct FxData {
    materials: Vec<MaterialDef>,
    systems: Vec<SystemDef>,
    meshes: Vec<FxMesh>,
    placements: Vec<Placement>,
    flares: Vec<FlareDef>,
    flare_places: Vec<FlarePlacement>,
    /// The level's reflection cube map (its surfaces reflect it) and its mip count.
    cube: Option<(String, u32)>,
}

fn kv<'a>(words: &[&'a str], key: &str) -> Option<&'a str> {
    words
        .iter()
        .find_map(|w| w.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

fn kvf(words: &[&str], key: &str, default: f32) -> f32 {
    kv(words, key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn address(word: &str) -> ImageAddressMode {
    match word {
        "clamp" => ImageAddressMode::ClampToEdge,
        "mirror" => ImageAddressMode::MirrorRepeat,
        _ => ImageAddressMode::Repeat,
    }
}

fn floats(words: &[&str]) -> Vec<f32> {
    words.iter().filter_map(|w| w.parse().ok()).collect()
}

impl FxData {
    fn parse(text: &str) -> FxData {
        let mut data = FxData::default();
        let mut in_emitter = false;
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            let Some(&first) = words.first() else {
                continue;
            };
            let rest = &words[1..];
            match first {
                "#" => {}
                "mat" => {
                    in_emitter = false;
                    data.materials.push(MaterialDef {
                        name: rest.get(2).map_or(String::new(), |n| n.to_string()),
                        blend: match kv(rest, "blend") {
                            Some("additive") => Blend::Additive,
                            Some("modulate") => Blend::Modulate,
                            Some("masked") => Blend::Masked,
                            _ => Blend::Translucent,
                        },
                        lit: kv(rest, "lit") == Some("1"),
                        two_sided: kv(rest, "twosided") == Some("1"),
                        scene: kv(rest, "scene") == Some("1"),
                        distort: kv(rest, "distort") == Some("1"),
                        fog: kv(rest, "nofog") != Some("1"),
                        clip: kvf(rest, "clip", 0.3333),
                        textures: Vec::new(),
                        cube_roughness: kvf(rest, "cuberough", 0.0),
                        reflects: kv(rest, "cuberough").is_some(),
                        cube_texture: kv(rest, "cubetex").map(str::to_string),
                    });
                }
                "cube" => {
                    if let [path, mips, ..] = rest {
                        data.cube = Some((path.to_string(), mips.parse().unwrap_or(1)));
                    }
                }
                "tex" => {
                    if let (Some(m), [path, srgb, u, v, ..]) = (data.materials.last_mut(), rest) {
                        m.textures
                            .push((path.to_string(), *srgb == "1", address(u), address(v)));
                    }
                }
                "system" => {
                    in_emitter = false;
                    data.systems.push(SystemDef {
                        name: rest.get(1).map(|n| n.to_string()).unwrap_or_default(),
                        warmup: kvf(rest, "warmup", 0.0),
                        lit: kv(rest, "lit") == Some("1"),
                        emitters: Vec::new(),
                        lods: Vec::new(),
                    });
                }
                // A level of detail: its distance and its system.
                "lod" => {
                    if let (Some(s), Some(Ok(d)), Some(Ok(j))) = (
                        data.systems.last_mut(),
                        rest.first().map(|w| w.parse::<f32>()),
                        rest.get(1).map(|w| w.parse::<usize>()),
                    ) {
                        s.lods.push((d, j));
                    }
                }
                "emitter" => {
                    if let Some(s) = data.systems.last_mut() {
                        s.emitters.push(EmitterDef {
                            name: rest.first().map(|n| n.to_string()).unwrap_or_default(),
                            ..default()
                        });
                        in_emitter = true;
                    }
                }
                "end" => in_emitter = false,
                "place" => {
                    in_emitter = false;
                    let v = floats(rest.get(1..).unwrap_or(&[]));
                    if let (Some(Ok(system)), true) =
                        (rest.first().map(|w| w.parse::<usize>()), v.len() >= 12)
                    {
                        data.placements.push(Placement {
                            system,
                            origin: Vec3::new(v[0], v[1], v[2]),
                            axes: [
                                Vec3::new(v[3], v[4], v[5]),
                                Vec3::new(v[6], v[7], v[8]),
                                Vec3::new(v[9], v[10], v[11]),
                            ],
                            params: Vec::new(),
                            lit: v.get(12).is_some_and(|&l| l == 1.0),
                        });
                    }
                }
                "param" => {
                    if let (Some(p), [name, r, g, b, a, ..]) = (data.placements.last_mut(), rest) {
                        let c = |s: &str| s.parse::<u8>().unwrap_or(255);
                        p.params.push((name.to_string(), [c(r), c(g), c(b), c(a)]));
                    }
                }
                "flare" => {
                    in_emitter = false;
                    let screen_map = rest
                        .iter()
                        .position(|w| *w == "screenmap")
                        .and_then(|i| Dist::parse(&rest[i + 1..]))
                        .unwrap_or_else(|| Dist::constant(1.0));
                    data.flares.push(FlareDef {
                        screen_map,
                        elements: Vec::new(),
                    });
                }
                "element" => {
                    if let Some(f) = data.flares.last_mut() {
                        let size: Vec<f32> = kv(rest, "size")
                            .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
                            .unwrap_or_default();
                        f.elements.push(FlareElement {
                            source: rest.first() == Some(&"source"),
                            material: kvf(rest, "mat", 0.0) as usize,
                            ray: kvf(rest, "ray", 0.0),
                            size: Vec3::new(
                                size.first().copied().unwrap_or(0.5),
                                size.get(1).copied().unwrap_or(0.5),
                                size.get(2).copied().unwrap_or(0.0),
                            ),
                            normalize_radial: kv(rest, "normrad") == Some("1"),
                            use_source_distance: kv(rest, "usesrc") == Some("1"),
                            dists: HashMap::new(),
                        });
                    }
                }
                "flareplace" => {
                    let v = floats(rest.get(1..).unwrap_or(&[]));
                    if let (Some(Ok(flare)), true) =
                        (rest.first().map(|w| w.parse::<usize>()), v.len() >= 17)
                    {
                        data.flare_places.push(FlarePlacement {
                            flare,
                            origin: Vec3::new(v[0], v[1], v[2]),
                            color: Vec4::new(v[12], v[13], v[14], v[15]),
                            size: v[16],
                        });
                    }
                }
                _ => {
                    if in_emitter {
                        data.emitter_line(first, rest);
                    } else if let Some(e) =
                        data.flares.last_mut().and_then(|f| f.elements.last_mut())
                        && let Some(d) = Dist::parse(rest)
                    {
                        e.dists.insert(first.to_string(), d);
                    }
                }
            }
        }
        data.mark_sources();
        data.mark_immortal();
        data.mark_capped();
        data
    }

    /// An emitter's "typedata" line: what it draws.
    fn typedata(&mut self, rest: &[&str]) -> TypeData {
        match rest.first().copied() {
            Some("trail") => TypeData::Trail {
                tess: (kvf(rest, "tess", 1.0) as u32).max(1),
                tile: kvf(rest, "tile", 1.0),
                max: (kvf(rest, "max", 500.0) as usize).clamp(2, MAX_PARTICLES),
            },
            Some("mesh") => {
                let file = kv(rest, "file").unwrap_or("");
                match FxMesh::load(&crate::assets_dir().join(file)) {
                    Some(m) => {
                        self.meshes.push(m);
                        TypeData::Mesh(self.meshes.len() - 1)
                    }
                    None => {
                        warn!("levelfx: mesh {file} can't be read; its emitter isn't drawn");
                        TypeData::Mesh(usize::MAX)
                    }
                }
            }
            Some("decal") => {
                TypeData::Decal(Dist::parse(&rest[1..]).unwrap_or_else(|| Dist::constant(0.0)))
            }
            Some("beam") => {
                let vec = |key: &str, default: Vec3| {
                    let v: Vec<f32> = kv(rest, key)
                        .map(|w| w.split(',').filter_map(|x| x.parse().ok()).collect())
                        .unwrap_or_default();
                    if v.len() == 3 {
                        Vec3::new(v[0], v[1], v[2])
                    } else {
                        default
                    }
                };
                TypeData::Beam(Box::new(Beam {
                    max: (kvf(rest, "max", 1.0) as usize).clamp(1, MAX_PARTICLES),
                    tile: kvf(rest, "tile", 1.0),
                    taper: kvf(rest, "taper", 0.0) != 0.0,
                    factor: Dist::constant(1.0),
                    scale: Dist::constant(1.0),
                    source: vec("source", Vec3::ZERO),
                    target: vec("target", Vec3::new(50.0, 0.0, 0.0)),
                    waves: Vec::new(),
                }))
            }
            _ => TypeData::Sprite,
        }
    }

    /// A line of an emitter's block (after "emitter").
    fn emitter_line(&mut self, first: &str, rest: &[&str]) {
        // (A mesh emitter's mesh is read first: the emitter is borrowed below.)
        let typedata = if first == "typedata" {
            self.typedata(rest)
        } else {
            TypeData::Sprite
        };
        let Some(e) = self.systems.last_mut().and_then(|s| s.emitters.last_mut()) else {
            return;
        };
        match first {
            "beamfactor" | "beamscale" => {
                if let (TypeData::Beam(beam), Some(d)) = (&mut e.typedata, Dist::parse(rest)) {
                    if first == "beamfactor" {
                        beam.factor = d;
                    } else {
                        beam.scale = d;
                    }
                }
            }
            // "beamwave <amplitude> <period> <speed> <phase> <x>,<y>,<z>"
            "beamwave" => {
                let n: Vec<f32> = rest.iter().take(4).filter_map(|w| w.parse().ok()).collect();
                let d: Vec<f32> = rest
                    .get(4)
                    .map(|w| w.split(',').filter_map(|x| x.parse().ok()).collect())
                    .unwrap_or_default();
                if let (TypeData::Beam(beam), [amplitude, period, speed, phase], [x, y, z]) =
                    (&mut e.typedata, n.as_slice(), d.as_slice())
                {
                    beam.waves.push(BeamWave {
                        amplitude: *amplitude,
                        period: *period,
                        speed: *speed,
                        phase: *phase,
                        direction: Vec3::new(*x, *y, *z),
                    });
                }
            }
            "material" => e.material = rest.first().and_then(|w| w.parse().ok()).unwrap_or(0),
            "peak" => e.peak = rest.first().and_then(|w| w.parse().ok()).unwrap_or(0),
            "required" => {
                e.align = kvf(rest, "align", 0.0) as u8;
                e.local = kv(rest, "local") == Some("1");
                e.duration = kvf(rest, "duration", 1.0);
                e.duration_low = kvf(rest, "durlow", 0.0);
                e.duration_range = kv(rest, "durrange") == Some("1");
                e.duration_recalc = kv(rest, "durrecalc") == Some("1");
                e.loops = kvf(rest, "loops", 0.0) as u32;
                e.delay = kvf(rest, "delay", 0.0);
                e.delay_first = kv(rest, "delayfirst") == Some("1");
                e.sub_h = (kvf(rest, "subh", 1.0) as u32).max(1);
                e.sub_v = (kvf(rest, "subv", 1.0) as u32).max(1);
                e.interp = kvf(rest, "interp", 0.0) as u8;
                e.random_time = kvf(rest, "randtime", 1.0);
                e.sort = kvf(rest, "sort", 0.0) as u8;
                e.camera_offset = kvf(rest, "camoffset", 0.0);
                e.max_draw = match kvf(rest, "maxdraw", 500.0) as usize {
                    0 => MAX_PARTICLES,
                    n => n,
                };
                e.probability = kvf(rest, "probability", 1.0);
                e.lock = kvf(rest, "lock", 0.0) as u8;
                e.scale_uv = kv(rest, "scaleuv") == Some("1");
                e.kill_on_deactivate = kv(rest, "killdeact") == Some("1");
                e.kill_on_completed = kv(rest, "killdone") == Some("1");
                e.undrawn = kv(rest, "draw") == Some("0");
            }
            "typedata" => e.typedata = typedata,
            "rate" => e.rate = Dist::parse(rest).unwrap_or_else(|| Dist::constant(0.0)),
            "burst" => {
                if let [count, low, time, ..] = rest {
                    e.bursts.push((
                        count.parse().unwrap_or(0),
                        low.parse().unwrap_or(-1),
                        time.parse().unwrap_or(0.0),
                    ));
                }
            }
            "module" => {
                if let [index, kind, ..] = rest
                    && let (Ok(index), Ok(kind)) = (index.parse::<usize>(), kind.parse::<u16>())
                {
                    if e.modules.len() <= index {
                        e.modules.resize_with(index + 1, || None);
                    }
                    e.modules[index] = Some(ModuleDef { kind, ..default() });
                }
            }
            "f" | "v" | "w" | "name" => {
                let Some(m) = e.modules.iter_mut().rev().find_map(|m| m.as_mut()) else {
                    return;
                };
                match first {
                    "f" => m.floats.extend(Dist::parse(rest)),
                    "v" => m.vectors.extend(Dist::parse(rest)),
                    "w" => m.words = rest.iter().filter_map(|w| w.parse().ok()).collect(),
                    _ => m.name = rest.first().map(|s| s.to_string()),
                }
            }
            "spawn" => e.spawn = rest.iter().filter_map(|w| w.parse().ok()).collect(),
            "update" => e.update = rest.iter().filter_map(|w| w.parse().ok()).collect(),
            "spawning" => e.spawning = rest.iter().filter_map(|w| w.parse().ok()).collect(),
            _ => {}
        }
    }

    /// Marks the emitters others of their system spawn at (LocationEmitter).
    fn mark_sources(&mut self) {
        for system in &mut self.systems {
            let wanted: Vec<String> = system
                .emitters
                .iter()
                .flat_map(|e| e.modules.iter().flatten())
                .filter(|m| m.kind == LOCATION_EMITTER || m.kind == LOCATION_EMITTER_DIRECT)
                .filter_map(|m| m.name.clone())
                .collect();
            for e in &mut system.emitters {
                e.is_source = wanted.contains(&e.name);
            }
        }
    }

    /// Marks the emitters of PEAK_CAPPED's systems (at every level of detail: their
    /// `@<level>` copies) as keeping to their peaks.
    fn mark_capped(&mut self) {
        for s in &mut self.systems {
            let base = s.name.split('@').next().unwrap_or_default();
            if PEAK_CAPPED.contains(&base) {
                for e in &mut s.emitters {
                    e.capped = true;
                }
            }
        }
    }

    /// Marks the emitters whose particles never die: none of their spawn modules
    /// gives them a lifetime over 0 (as UE3's Lifetime module leaves such a
    /// particle's OneOverMaxLifetime 0, so it never ages).
    fn mark_immortal(&mut self) {
        for e in self.systems.iter_mut().flat_map(|s| s.emitters.iter_mut()) {
            e.immortal = !e.spawn.iter().any(|&i| {
                e.modules
                    .get(i)
                    .and_then(|m| m.as_ref())
                    .is_some_and(|m| m.kind == LIFETIME && m.f(0).highest() > 0.0)
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Unreal's axes and units
// ---------------------------------------------------------------------------

/// Unreal's (x, y, z) in centimetres to Bevy's (x, z, y) in metres.
fn to_bevy(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, v.y) * 0.01
}

fn dir_to_bevy(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, v.y)
}

fn to_ue(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, v.y) * 100.0
}

fn dir_to_ue(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, v.y)
}

// ---------------------------------------------------------------------------
// Particles
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
struct Particle {
    location: Vec3,
    base_velocity: Vec3,
    velocity: Vec3,
    rotation: f32,
    base_rotation_rate: f32,
    rotation_rate: f32,
    base_size: Vec3,
    size: Vec3,
    color: Vec4,
    base_color: Vec4,
    relative_time: f32,
    one_over_max_lifetime: f32,
    /// Acceleration module's acceleration.
    accel: Vec3,
    /// Dynamic parameters, and the random numbers their picks keep.
    dyn_params: [f32; 4],
    dyn_rand: [f32; 4],
    /// Sub-image (ImageIndex) and when a random one was last picked.
    image: f32,
    random_image_time: f32,
    /// A mesh particle's turn (MeshRotation): roll, pitch and yaw (radians), and how
    /// fast it turns (MeshRotationRate, radians a second), from the rate it spawned
    /// with (each frame the rate starts from that again: FParticleMeshEmitterInstance.
    /// Tick's RotationRateBase, which MeshRotationRateMultiplyLife scales).
    mesh_rotation: Vec3,
    mesh_rotation_rate: Vec3,
    base_mesh_rotation_rate: Vec3,
    /// A decal particle's mark, once laid: its number among its emitter's marks
    /// (EmitterInstance::marks), or None if there was nothing in reach to lay it on.
    decal: Option<Option<u32>>,
}

/// What the modules need of the emitter's placement.
struct SimFrame<'a> {
    origin: Vec3,
    axes: [Vec3; 3],
    /// What its particles' sizes are scaled by (UE3's emitter replay data's Scale:
    /// the component's own scale and its actor's DrawScale). Its axes carry that
    /// (its particles' places and speeds in its frame, and a local-space mesh's
    /// size).
    size: f32,
    params: &'a [(String, [u8; 4])],
    /// Its vector parameters as the game sets them while it plays (a jet's hover
    /// jets' Size: FxSystems::set_vector).
    vectors: &'a [(String, Vec3)],
    /// Where it was last frame (new particles are spread between the two, as UE3
    /// does for a moving emitter) and how fast it moves (VelocityInheritParent).
    prev_origin: Vec3,
    velocity: Vec3,
    /// The particles of the system's emitters that others spawn theirs at
    /// (LocationEmitter), as they are this frame.
    sources: &'a [Source],
}

/// An emitter's particles as another emitter of its system sees them
/// (LocationEmitter): where each is, how fast it goes and its turn, in the
/// emitter's own space (local or the world's).
struct Source {
    name: String,
    local: bool,
    particles: Vec<(Vec3, Vec3, f32)>,
    /// Each particle's age (its RelativeTime: 0 born, 1 at its end).
    times: Vec<f32>,
    /// It draws beams: its particles are their sources' (LocationEmitterDirect
    /// takes nothing from it).
    beam: bool,
}

impl SimFrame<'_> {
    /// A direction from the emitter's frame (TransformNormal: rotated and scaled).
    fn normal(&self, v: Vec3) -> Vec3 {
        self.axes[0] * v.x + self.axes[1] * v.y + self.axes[2] * v.z
    }

    fn point(&self, v: Vec3) -> Vec3 {
        self.origin + self.normal(v)
    }

    /// A world place in the emitter's frame (InverseTransformPosition).
    fn local_point(&self, v: Vec3) -> Vec3 {
        let m = Mat3::from_cols(self.axes[0], self.axes[1], self.axes[2]);
        if m.determinant().abs() < 1e-8 {
            return v - self.origin;
        }
        m.inverse() * (v - self.origin)
    }
}

/// A frame's scale, its axes' lengths on average (a placed effect's DrawScale).
fn axes_scale(axes: &[Vec3; 3]) -> f32 {
    (axes[0].length() + axes[1].length() + axes[2].length()) / 3.0
}

/// One emitter of a placed system, running.
struct EmitterInstance {
    system: usize,
    emitter: usize,
    placement: usize,
    particles: Vec<Particle>,
    seconds: f32,
    emitter_time: f32,
    loop_count: u32,
    duration: f32,
    spawn_fraction: f32,
    bursts_fired: Vec<bool>,
    first: bool,
    rng: Rng,
    /// Where (relative to the emitter, Unreal's axes) its newest particle was
    /// spawned: a trail runs from there to the emitter as it moves on.
    spawn_offset: Vec3,
    /// Each SpawnPerUnit module's distance moved and not yet spawned for (its
    /// CurrentDistanceTravelled), by its place in the spawning list.
    travelled: Vec<f32>,
    /// The source particle a LocationEmitter picked last, taking them in turn
    /// (LastSelectedIndex).
    last_selected: usize,
    /// It is run from scratch along a path every frame (FxSystems::replay), so
    /// nothing piles up: an emitter whose particles never die isn't held to its
    /// peak (EmitterDef::immortal).
    replayed: bool,
    /// A decal emitter's marks, as its particles laid them on the level (see
    /// `decals`), by their numbers, and the next mark's number.
    marks: Vec<(u32, DecalMark)>,
    next_mark: u32,
}

/// A decal's mark: the pieces of the level's surfaces it lies on, cut to its box,
/// in Bevy's axes (world space), a little off each surface.
#[derive(Default)]
struct DecalMark {
    pos: Vec<Vec3>,
    nrm: Vec<Vec3>,
    tan: Vec<Vec4>,
    uv: Vec<Vec2>,
    idx: Vec<u32>,
}

impl EmitterInstance {
    fn new(def: &EmitterDef, system: usize, emitter: usize, placement: usize, seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        // (UE3's emitter duration takes in its delay.)
        let duration = Self::pick_duration(def, &mut rng) + def.delay;
        EmitterInstance {
            system,
            emitter,
            placement,
            particles: Vec::new(),
            seconds: 0.0,
            emitter_time: 0.0,
            loop_count: 0,
            duration,
            spawn_fraction: 0.0,
            bursts_fired: vec![false; def.bursts.len()],
            first: true,
            rng,
            spawn_offset: Vec3::ZERO,
            travelled: vec![0.0; def.spawning.len()],
            last_selected: 0,
            replayed: false,
            marks: Vec::new(),
            next_mark: 0,
        }
    }

    /// The emitter is done: a one-shot emitter (EmitterLoops > 0) past its last
    /// loop with no particles left (UE3's HasCompleted).
    fn finished(&self, def: &EmitterDef) -> bool {
        def.loops > 0
            && self.seconds >= self.duration * def.loops as f32
            && self.particles.is_empty()
    }

    fn pick_duration(def: &EmitterDef, rng: &mut Rng) -> f32 {
        if def.duration_range {
            def.duration_low + (def.duration - def.duration_low) * rng.next()
        } else {
            def.duration
        }
    }

    /// One frame of UE3's FParticleEmitterInstance::Tick (`spawning`: the system
    /// is on; off, its particles live out their lives and no more come).
    fn tick(&mut self, def: &EmitterDef, frame: &SimFrame, dt: f32, spawning: bool) {
        self.seconds += dt;
        // The time within the loop (UE3's legacy emitter time).
        self.emitter_time = self.seconds;
        let mut looped = false;
        if self.duration > 1e-4 {
            self.emitter_time = self.seconds % self.duration;
            looped = self.seconds - self.duration * self.loop_count as f32 >= self.duration;
        }
        if looped {
            self.loop_count += 1;
            self.bursts_fired.iter_mut().for_each(|b| *b = false);
            if def.duration_recalc || (def.delay_first && self.loop_count == 1) {
                // (Loops after the first don't wait when only the first is delayed.)
                let delay = if def.delay_first { 0.0 } else { def.delay };
                self.duration = Self::pick_duration(def, &mut self.rng) + delay;
            }
        }
        let delay = if def.delay_first && self.loop_count > 0 {
            0.0
        } else {
            def.delay
        };
        let et = self.emitter_time - delay;

        // Kill the dead (the last one takes a dead one's place, as in UE3; a
        // trail's stay in the order they came), and all of them once a one-shot
        // emitter that kills its own on completing is past its last loop (its
        // duration taking in its delay, as `finished` has it).
        if def.kill_on_completed
            && def.loops > 0
            && self.seconds >= self.duration * def.loops as f32
        {
            self.particles.clear();
        } else if matches!(def.typedata, TypeData::Trail { .. }) {
            self.particles.retain(|p| p.relative_time <= 1.0);
        } else {
            let mut i = self.particles.len();
            while i > 0 {
                i -= 1;
                if self.particles[i].relative_time > 1.0 {
                    self.particles.swap_remove(i);
                }
            }
        }
        // Reset to the base values.
        for p in &mut self.particles {
            p.velocity = p.base_velocity;
            p.size = p.base_size;
            p.rotation_rate = p.base_rotation_rate;
            p.mesh_rotation_rate = p.base_mesh_rotation_rate;
            p.color = p.base_color;
            p.relative_time += p.one_over_max_lifetime * dt;
        }
        // Update modules.
        for &m in &def.update {
            if let Some(Some(module)) = def.modules.get(m) {
                self.update_module(def, module, frame, et, dt);
            }
        }
        // Spawn.
        if spawning
            && et >= 0.0
            && (def.loops == 0
                || self.loop_count < def.loops
                || self.seconds < self.duration * def.loops as f32
                || self.first)
        {
            self.first = false;
            self.spawn(def, frame, et, dt);
        } else if !spawning {
            self.spawn_fraction = 0.0;
        }
        // Move.
        for p in &mut self.particles {
            p.location += p.velocity * dt;
            p.rotation += p.rotation_rate * dt;
            p.mesh_rotation += p.mesh_rotation_rate * dt;
        }
    }

    fn spawn(&mut self, def: &EmitterDef, frame: &SimFrame, et: f32, dt: f32) {
        // The spawning modules first (UE3's GetSpawnAmount): SpawnPerUnit spawns by
        // the distance the emitter moved, a particle every UnitScalar / SpawnPerUnit
        // units, and while it moves can stop the spawn rate.
        let mut per_unit = 0.0;
        let mut process_rate = true;
        for (k, &mi) in def.spawning.iter().enumerate() {
            let Some(Some(m)) = def.modules.get(mi) else {
                continue;
            };
            if m.kind != SPAWN_PER_UNIT {
                continue;
            }
            let unit = f32::from_bits(m.word(0)).max(1e-3);
            let tolerance = f32::from_bits(m.word(1));
            let per = m.f(0).float(et, &mut self.rng) / unit;
            let mut moved = false;
            if per >= 0.0 {
                let travel = (frame.origin - frame.prev_origin).length();
                if travel > 0.0 {
                    moved = travel > tolerance * unit;
                    let total = travel + self.travelled.get(k).copied().unwrap_or(0.0);
                    let number = (total * per).floor();
                    per_unit += number / dt.max(1e-6);
                    if let Some(t) = self.travelled.get_mut(k) {
                        *t = (total - number * unit).max(0.0);
                    }
                }
            }
            let rate_on = if m.flag(2) { !moved } else { m.flag(3) };
            process_rate &= rate_on;
        }
        let own = if process_rate {
            def.rate.float(et, &mut self.rng).max(0.0)
        } else {
            0.0
        };
        let rate = own + per_unit;
        let mut burst = 0i32;
        for (k, &(count, low, time)) in def.bursts.iter().enumerate() {
            if !self.bursts_fired[k] && et >= time {
                burst += if low > -1 {
                    low + ((count - low) as f32 * self.rng.next()).round() as i32
                } else {
                    count
                };
                self.bursts_fired[k] = true;
            }
        }
        if rate <= 0.0 && burst <= 0 {
            return;
        }
        let leftover = self.spawn_fraction + dt * rate;
        let number = leftover.floor().max(0.0) as usize;
        let increment = if rate > 0.0 { 1.0 / rate } else { 0.0 };
        let start = dt + self.spawn_fraction * increment - increment;
        self.spawn_fraction = leftover - number as f32;
        self.spawn_particles(def, frame, et, number, start, increment, dt);
        self.spawn_particles(def, frame, et, burst.max(0) as usize, 0.0, 0.0, dt);
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_particles(
        &mut self,
        def: &EmitterDef,
        frame: &SimFrame,
        et: f32,
        count: usize,
        start: f32,
        increment: f32,
        dt: f32,
    ) {
        let mut spawn_time = start;
        let trail_max = match def.typedata {
            TypeData::Trail { max, .. } => Some(max),
            _ => None,
        };
        // (A beam emitter has at most its MaxBeamCount beams out at a time, and a
        // decal emitter and one whose particles never die their peak of them: see
        // EmitterDef::peak. A trail keeps its own count.)
        let most = match &def.typedata {
            TypeData::Beam(beam) => beam.max,
            TypeData::Trail { .. } => MAX_PARTICLES,
            _ if self.replayed => MAX_PARTICLES,
            TypeData::Decal(_) => def.peak.clamp(1, MAX_PARTICLES),
            _ if def.immortal => def.peak.clamp(1, MAX_PARTICLES),
            _ if def.capped && def.peak > 0 => def.peak.min(MAX_PARTICLES),
            _ => MAX_PARTICLES,
        };
        // (A particle spawned `t` into this frame's `dt` starts that far back along
        // the emitter's move.)
        let moved = frame.origin - frame.prev_origin;
        let dt = dt.max(1e-6);
        for _ in 0..count {
            match trail_max {
                // A full trail spawns no more until some of it dies: UE3 gives a
                // trail emitter a fixed array of MaxTrailCount (1) times
                // MaxParticleInTrailCount particles, and clamps its spawning to what
                // is left of it (FParticleTrail2EmitterInstance.Spawn). Only the paths
                // fine aim shows, whose particles never die, ever fill theirs: they
                // show the start of the path.
                Some(max) if self.particles.len() >= max => break,
                _ if self.particles.len() >= most => break,
                _ => {}
            }
            let keep = def.probability >= 1.0 || self.rng.next() < def.probability;
            let back = if def.local || increment <= 0.0 {
                Vec3::ZERO
            } else {
                moved * (spawn_time / dt).clamp(0.0, 1.0)
            };
            let mut p = Particle {
                location: if def.local {
                    Vec3::ZERO
                } else {
                    frame.origin - back
                },
                // (A material's dynamic parameters are 1 until its modules set them,
                // and stay 1 from an emitter without a DynamicParameter module: see
                // dynamic_parameters_without_a_module_are_1.)
                dyn_params: [1.0; 4],
                ..default()
            };
            for r in p.dyn_rand.iter_mut() {
                *r = self.rng.next();
            }
            for &m in &def.spawn {
                if let Some(Some(module)) = def.modules.get(m) {
                    self.spawn_module(def, module, frame, &mut p, et, spawn_time, back);
                }
            }
            p.location += p.velocity * spawn_time;
            if keep {
                self.spawn_offset = p.location - if def.local { Vec3::ZERO } else { frame.origin };
                self.particles.push(p);
            }
            spawn_time -= increment;
        }
    }

    /// The effect's module channel is B (ParticleSystemComponent.SwitchModuleChannel(2):
    /// a team 1 effect's, TnParticleSystemParameterTeamIndex), not A (team 0's, and
    /// an effect's until it is switched).
    fn module_channel_b(frame: &SimFrame) -> bool {
        frame
            .params
            .iter()
            .any(|(n, c)| n == MODULE_CHANNEL && c[0] == MODULE_CHANNEL_B[0])
    }

    fn color_param(frame: &SimFrame, module: &ModuleDef) -> Vec4 {
        let named = module
            .name
            .as_ref()
            .and_then(|n| frame.params.iter().find(|(p, _)| p == n))
            .map(|(_, c)| *c);
        let c = named.unwrap_or_else(|| {
            let w = module.word(1).to_le_bytes();
            [w[2], w[1], w[0], w[3]]
        });
        Vec4::new(c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32) / 255.0
    }

    /// A spawn module's work on new particle `p` (`back`: how far back along the
    /// emitter's move this frame it spawns).
    #[allow(clippy::too_many_arguments)]
    fn spawn_module(
        &mut self,
        def: &EmitterDef,
        m: &ModuleDef,
        frame: &SimFrame,
        p: &mut Particle,
        et: f32,
        spawn_time: f32,
        back: Vec3,
    ) {
        let rng = &mut self.rng;
        match m.kind {
            // Lifetime
            0x01 => {
                let life = m.f(0).float(et, rng);
                p.one_over_max_lifetime = if p.one_over_max_lifetime > 0.0 {
                    1.0 / (life + 1.0 / p.one_over_max_lifetime)
                } else if life > 0.0 {
                    1.0 / life
                } else {
                    0.0
                };
                if p.relative_time <= 1.0 {
                    p.relative_time = spawn_time * p.one_over_max_lifetime;
                }
            }
            // Color. Its alpha is taken whole, past 1 too: High Moon's compiled
            // module keeps no bClampAlpha (it has no settings of its own), and the
            // game's additive materials multiply their colour by such an opacity
            // unclamped (see levelfx.wgsl's FX_ADDITIVE). The Electric Fences'
            // lightning (StartAlpha 6) shows it: held to 1, it came out a third as
            // bright as in the game's videos.
            0x02 => {
                let c = m.v(0).vector(et, rng);
                let a = m.f(0).float(et, rng).max(0.0);
                p.color = c.extend(a);
                p.base_color = p.color;
            }
            // ColorByParameter
            0x03 => {
                p.color = Self::color_param(frame, m);
                p.base_color = p.color;
            }
            // ColorOverLife
            0x04 => {
                let c = m.v(0).vector(p.relative_time, rng);
                let a = m.f(0).float(p.relative_time, rng);
                p.color = c.extend(a);
                p.base_color = p.color;
            }
            // ColorScaleOverLife
            0x05 => {
                let t = if m.flag(0) { et } else { p.relative_time };
                let c = m.v(0).vector(t, rng);
                let a = m.f(0).float(t, rng);
                p.color *= c.extend(a);
            }
            SWITCH_COLOR_SCALE => {
                let k = Self::module_channel_b(frame) as usize;
                let c = m.v(k).vector(p.relative_time, rng);
                let a = m.f(k).float(p.relative_time, rng);
                p.color *= c.extend(a);
            }
            // Size
            0x06 => {
                let s = m.v(0).vector(et, rng);
                p.size += s;
                p.base_size += s;
            }
            // SizeMultiplyLife
            0x08 => self.size_multiply_life(m, p),
            SIZE_BY_PARAMETER => Self::size_by_parameter(frame, m, p),
            // MeshRotation (in turns: X roll, Y pitch, Z yaw)
            0x0a => p.mesh_rotation += m.v(0).vector(et, &mut self.rng) * TAU,
            // MeshRotationRate (turns a second)
            0x0b => {
                let r = m.v(0).vector(et, &mut self.rng) * TAU;
                p.mesh_rotation_rate += r;
                p.base_mesh_rotation_rate += r;
            }
            // MeshRotationRateMultiplyLife
            0x0c => p.mesh_rotation_rate *= m.v(0).vector(p.relative_time, &mut self.rng),
            // Rotation
            0x0d => p.rotation += m.f(0).float(et, &mut self.rng) * TAU,
            // RotationRate
            0x0f => {
                let r = m.f(0).float(et, &mut self.rng) * TAU;
                p.rotation_rate += r;
                p.base_rotation_rate += r;
            }
            // Location
            0x11 => {
                let v = m.v(0).vector(et, &mut self.rng);
                p.location += if def.local { v } else { frame.normal(v) };
            }
            // LocationPrimitiveCylinder
            0x15 => self.cylinder(def, m, frame, p, et),
            // LocationPrimitiveSphere
            0x16 => self.sphere(def, m, frame, p, et),
            // Velocity
            0x17 => {
                let mut v = m.v(0).vector(et, &mut self.rng);
                if !def.local {
                    v = frame.normal(v);
                }
                let from = if def.local {
                    p.location
                } else {
                    p.location - frame.origin
                };
                v += from.normalize_or_zero() * m.f(0).float(et, &mut self.rng);
                p.velocity += v;
                p.base_velocity += v;
            }
            // VelocityInheritParent: some of the emitter's own velocity.
            0x18 => {
                let mut v = frame.velocity * m.v(0).vector(et, &mut self.rng);
                if def.local {
                    // (in the emitter's frame)
                    v = Vec3::new(
                        v.dot(frame.axes[0].normalize_or_zero()),
                        v.dot(frame.axes[1].normalize_or_zero()),
                        v.dot(frame.axes[2].normalize_or_zero()),
                    );
                }
                p.velocity += v;
                p.base_velocity += v;
            }
            // VelocityOverLifetime (Absolute: it sets the velocity)
            0x19 => {
                if m.flag(0) {
                    let mut v = m.v(0).vector(p.relative_time, &mut self.rng);
                    if !def.local {
                        v = frame.normal(v);
                    }
                    p.velocity = v;
                    p.base_velocity = v;
                }
            }
            // Acceleration
            0x1a => {
                let mut a = m.v(0).vector(et, &mut self.rng);
                if !def.local {
                    a = frame.normal(a);
                }
                p.accel = a;
                p.velocity += a * spawn_time;
                p.base_velocity += a * spawn_time;
            }
            // DynamicParameter
            0x1d => {
                for k in 0..4 {
                    let t = if m.flag(k) { et } else { p.relative_time };
                    p.dyn_params[k] = m.f(k).float_r(t, p.dyn_rand[k]);
                }
            }
            // SubUV
            0x1f => self.image_index(def, m, p),
            // LocationEmitter
            LOCATION_EMITTER => self.location_emitter(def, m, frame, p, back),
            // (The new particle's place in the list: its source's of the same.)
            LOCATION_EMITTER_DIRECT => {
                Self::location_emitter_direct(def, m, frame, p, self.particles.len())
            }
            _ => {}
        }
    }

    /// UE3's LocationEmitterDirect: particle `index` (its place in the emitter's
    /// list) goes where its source emitter's particle of the same place is, as fast
    /// and as old (its Location, Velocity and RelativeTime), as it spawns and every
    /// frame after; with no such particle (its source has fewer out) it is left as it
    /// is. (High Moon's compiled module has a beam setting too, as LocationEmitter's:
    /// a beam's particles, sources of its own, give it nothing here.) The Snipers'
    /// tracer's lightning rides its Bolt so, along the shot.
    fn location_emitter_direct(
        def: &EmitterDef,
        m: &ModuleDef,
        frame: &SimFrame,
        p: &mut Particle,
        index: usize,
    ) {
        let Some(source) = m
            .name
            .as_deref()
            .and_then(|n| frame.sources.iter().find(|s| s.name == n))
            .filter(|s| !s.beam)
        else {
            return;
        };
        let Some(&(mut at, mut velocity, _)) = source.particles.get(index) else {
            return;
        };
        if source.local != def.local {
            if def.local {
                at = frame.local_point(at);
            } else {
                at = frame.point(at);
                velocity = frame.normal(velocity);
            }
        }
        p.location = at;
        p.velocity = velocity;
        if let Some(&t) = source.times.get(index) {
            p.relative_time = t;
        }
    }

    /// UE3's LocationEmitter: the particle goes where one of its source emitter's
    /// particles is (one at random, or each in turn), taking on some of its velocity
    /// and turn if it says so. With no such emitter, or none of its particles out, it
    /// stays where it was put.
    fn location_emitter(
        &mut self,
        def: &EmitterDef,
        m: &ModuleDef,
        frame: &SimFrame,
        p: &mut Particle,
        back: Vec3,
    ) {
        let Some(source) = m
            .name
            .as_deref()
            .and_then(|n| frame.sources.iter().find(|s| s.name == n))
        else {
            return;
        };
        let n = source.particles.len();
        if n == 0 {
            return;
        }
        let index = if m.word(3) == 1 {
            self.last_selected += 1;
            if self.last_selected >= n {
                self.last_selected = 0;
            }
            self.last_selected
        } else {
            ((self.rng.next() * n as f32) as usize).min(n - 1)
        };
        let (mut at, mut velocity, rotation) = source.particles[index];
        if source.local != def.local {
            if def.local {
                at = frame.local_point(at);
            } else {
                at = frame.point(at);
                velocity = frame.normal(velocity);
            }
        }
        // (Spread back along the emitter's move as the others are.)
        p.location = if def.local { at } else { at - back };
        if m.flag(4) {
            let v = velocity * f32::from_bits(m.word(1));
            p.velocity += v;
            p.base_velocity += v;
        }
        if m.flag(5) {
            p.rotation += rotation * f32::from_bits(m.word(2));
        }
    }

    /// SIZE_BY_PARAMETER: the size times the parameter's value, on the axes it
    /// multiplies (as SizeMultiplyLife does with its LifeMultiplier).
    fn size_by_parameter(frame: &SimFrame, m: &ModuleDef, p: &mut Particle) {
        let s = Self::vector_parameter(frame, m, 6);
        for axis in 0..3 {
            if m.flag(axis) {
                p.size[axis] *= s[axis];
            }
        }
    }

    /// A DistributionVectorParticleParameter's value (UE3's
    /// UDistributionVectorParameterBase::GetValue), its modes at word 3 and its
    /// MinInput, MaxInput, MinOutput, MaxOutput and Constant from word `at`: the
    /// effect's vector parameter of its name (its Constant if the game hasn't set it),
    /// each axis taken as it is (ParamModes DPM_Direct) or clamped to its inputs'
    /// range and carried over to its outputs' (DPM_Normal; DPM_Abs on its size).
    fn vector_parameter(frame: &SimFrame, m: &ModuleDef, at: usize) -> Vec3 {
        let v = |k: usize| {
            let w = |j: usize| f32::from_bits(m.word(at + 3 * k + j));
            Vec3::new(w(0), w(1), w(2))
        };
        let (min_in, max_in, min_out, max_out, constant) = (v(0), v(1), v(2), v(3), v(4));
        let set = m
            .name
            .as_ref()
            .and_then(|n| frame.vectors.iter().find(|(k, _)| k == n))
            .map(|(_, v)| *v);
        let value = set.unwrap_or(constant);
        let mut out = Vec3::ZERO;
        for a in 0..3 {
            let mode = m.word(3 + a);
            let x = if mode == 1 { value[a].abs() } else { value[a] };
            out[a] = if mode == 2 {
                x
            } else {
                let gradient = if max_in[a] <= min_in[a] {
                    0.0
                } else {
                    (max_out[a] - min_out[a]) / (max_in[a] - min_in[a])
                };
                min_out[a] + (x.max(min_in[a]).min(max_in[a]) - min_in[a]) * gradient
            };
        }
        out
    }

    fn size_multiply_life(&mut self, m: &ModuleDef, p: &mut Particle) {
        let s = m.v(0).vector(p.relative_time, &mut self.rng);
        if m.flag(0) {
            p.size.x *= s.x;
        }
        if m.flag(1) {
            p.size.y *= s.y;
        }
        if m.flag(2) {
            p.size.z *= s.z;
        }
    }

    /// UE3's LocationPrimitiveBase::DetermineUnitDirection.
    fn unit_direction(&mut self, m: &ModuleDef) -> Vec3 {
        let mut out = Vec3::ZERO;
        for axis in 0..3 {
            let r = self.rng.next();
            let (pos, neg) = (m.flag(axis), m.flag(axis + 3));
            out[axis] = match (pos, neg) {
                (true, true) => r * 2.0 - 1.0,
                (true, false) => r,
                (false, true) => -r,
                _ => 0.0,
            };
        }
        out
    }

    fn cylinder(
        &mut self,
        def: &EmitterDef,
        m: &ModuleDef,
        frame: &SimFrame,
        p: &mut Particle,
        et: f32,
    ) {
        let (r0, r1, h) = match m.word(9) {
            0 => (1, 2, 0),
            1 => (0, 2, 1),
            _ => (0, 1, 2),
        };
        let start = m.v(0).vector(et, &mut self.rng);
        let radius = m.f(1).float(et, &mut self.rng);
        let height = m.f(2).float(et, &mut self.rng) / 2.0;
        // A point within the circle (UE3 tries 50 times, then clamps).
        let mut dir = Vec3::ZERO;
        for attempt in 0..50 {
            dir = self.unit_direction(m);
            let d2 = Vec2::new(dir[r0], dir[r1]);
            if d2.length_squared() <= 1.0 {
                break;
            }
            if attempt == 49 {
                let n = d2.normalize_or_zero();
                dir[r0] = n.x;
                dir[r1] = n.y;
            }
        }
        let flat = Vec2::new(dir[r0], dir[r1]).normalize_or_zero();
        let mut offset = Vec3::ZERO;
        offset[h] = dir[h] * height;
        if m.flag(6) {
            offset[r0] = flat.x * radius;
            offset[r1] = flat.y * radius;
        } else {
            offset[r0] = dir[r0] * radius;
            offset[r1] = dir[r1] * radius;
        }
        offset += start;
        let world = if def.local {
            offset
        } else {
            frame.normal(offset)
        };
        p.location += world;
        if m.flag(7) {
            let mut v = Vec3::ZERO;
            if m.flag(8) {
                v[r0] = offset[r0];
                v[r1] = offset[r1];
            } else {
                v = offset;
            }
            v *= m.f(0).float(et, &mut self.rng);
            if !def.local {
                v = frame.normal(v);
            }
            p.velocity += v;
            p.base_velocity += v;
        }
    }

    fn sphere(
        &mut self,
        def: &EmitterDef,
        m: &ModuleDef,
        frame: &SimFrame,
        p: &mut Particle,
        et: f32,
    ) {
        let start = m.v(0).vector(et, &mut self.rng);
        let mut dir = self.unit_direction(m);
        let unit = dir.normalize_or_zero();
        if m.flag(6) {
            dir = unit;
        }
        let radius = m.f(1).float(et, &mut self.rng);
        let mut offset = dir * radius;
        for axis in 0..3 {
            let max = unit[axis].abs() * radius;
            offset[axis] = offset[axis].clamp(-max, max);
        }
        offset += start;
        p.location += if def.local {
            offset
        } else {
            frame.normal(offset)
        };
        if m.flag(7) {
            let from = if def.local {
                p.location
            } else {
                p.location - frame.origin
            };
            let v = from * m.f(0).float(et, &mut self.rng);
            p.velocity += v;
            p.base_velocity += v;
        }
    }

    /// UE3's SubUV DetermineImageIndex.
    fn image_index(&mut self, def: &EmitterDef, m: &ModuleDef, p: &mut Particle) {
        let total = (def.sub_h * def.sub_v).max(1) as f32;
        match def.interp {
            1 | 2 => {
                let mut i = m.f(0).float(p.relative_time, &mut self.rng);
                if def.interp == 1 {
                    i = i.trunc();
                }
                p.image = i;
            }
            3 | 4 => {
                if def.random_time == 0.0
                    || p.relative_time - p.random_image_time > def.random_time
                    || p.random_image_time == 0.0
                {
                    p.image = (self.rng.next() * total).trunc();
                    p.random_image_time = p.relative_time.max(1e-6);
                }
            }
            _ => p.image = 0.0,
        }
    }

    /// An update module's work on every particle.
    fn update_module(
        &mut self,
        def: &EmitterDef,
        m: &ModuleDef,
        frame: &SimFrame,
        et: f32,
        dt: f32,
    ) {
        let mut particles = std::mem::take(&mut self.particles);
        match m.kind {
            // ColorOverLife
            0x04 => {
                for p in &mut particles {
                    let c = m.v(0).vector(p.relative_time, &mut self.rng);
                    let a = m.f(0).float(p.relative_time, &mut self.rng);
                    p.color = c.extend(a);
                }
            }
            // ColorScaleOverLife
            0x05 => {
                for p in &mut particles {
                    let t = if m.flag(0) { et } else { p.relative_time };
                    let c = m.v(0).vector(t, &mut self.rng);
                    let a = m.f(0).float(t, &mut self.rng);
                    p.color *= c.extend(a);
                }
            }
            SWITCH_COLOR_SCALE => {
                let k = Self::module_channel_b(frame) as usize;
                for p in &mut particles {
                    let c = m.v(k).vector(p.relative_time, &mut self.rng);
                    let a = m.f(k).float(p.relative_time, &mut self.rng);
                    p.color *= c.extend(a);
                }
            }
            // SizeScale
            0x09 => {
                for p in &mut particles {
                    let s = m.v(0).vector(p.relative_time, &mut self.rng);
                    for axis in 0..3 {
                        if m.flag(axis) {
                            p.size[axis] = p.base_size[axis] * s[axis];
                        }
                    }
                }
            }
            // SizeMultiplyLife
            0x08 => {
                for p in &mut particles {
                    self.size_multiply_life(m, p);
                }
            }
            SIZE_BY_PARAMETER => {
                for p in &mut particles {
                    Self::size_by_parameter(frame, m, p);
                }
            }
            // SizeMultiplyVelocity
            0x07 => {
                for p in &mut particles {
                    let s = m.v(0).vector(p.relative_time, &mut self.rng) * p.velocity.length();
                    for axis in 0..3 {
                        if m.flag(axis) {
                            p.size[axis] *= s[axis];
                        }
                    }
                }
            }
            // RotationRateMultiplyLife
            0x10 => {
                for p in &mut particles {
                    p.rotation_rate *= m.f(0).float(p.relative_time, &mut self.rng);
                }
            }
            // MeshRotationRateMultiplyLife
            0x0c => {
                for p in &mut particles {
                    p.mesh_rotation_rate *= m.v(0).vector(p.relative_time, &mut self.rng);
                }
            }
            // VelocityOverLifetime
            0x19 => {
                for p in &mut particles {
                    let v = m.v(0).vector(p.relative_time, &mut self.rng);
                    if m.flag(0) {
                        let v = if def.local { v } else { frame.normal(v) };
                        p.velocity = v;
                        p.base_velocity = v;
                    } else {
                        p.velocity *= v;
                    }
                }
            }
            // Acceleration
            0x1a => {
                for p in &mut particles {
                    p.velocity += p.accel * dt;
                    p.base_velocity += p.accel * dt;
                }
            }
            // AccelerationOverLifetime
            0x1b => {
                for p in &mut particles {
                    let a = m.v(0).vector(p.relative_time, &mut self.rng) * dt;
                    p.velocity += a;
                    p.base_velocity += a;
                }
            }
            // HmGravity: the world's gravity, scaled.
            0x1c => {
                let g = Vec3::new(0.0, 0.0, GRAVITY_Z * f32::from_bits(m.word(0))) * dt;
                for p in &mut particles {
                    p.velocity += g;
                    p.base_velocity += g;
                }
            }
            // DynamicParameter (not the spawn-only ones)
            0x1d => {
                for p in &mut particles {
                    for k in 0..4 {
                        if !m.flag(4 + k) {
                            let t = if m.flag(k) { et } else { p.relative_time };
                            p.dyn_params[k] = m.f(k).float_r(t, p.dyn_rand[k]);
                        }
                    }
                }
            }
            // SubUV
            0x1f => {
                for p in &mut particles {
                    if p.relative_time <= 1.0 {
                        self.image_index(def, m, p);
                    }
                }
            }
            // AttractorPoint
            0x80 => {
                let mut at = m.v(0).vector(et, &mut self.rng);
                if !def.local {
                    at = frame.point(at);
                }
                let range = m.f(0).float(et, &mut self.rng);
                for p in &mut particles {
                    let d = at - p.location;
                    let dist = d.length();
                    if dist <= range {
                        let s = if m.flag(0) {
                            let t = if range > 0.0 {
                                (dist / range).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            m.f(1).float(t, &mut self.rng)
                        } else {
                            m.f(1).float(p.relative_time, &mut self.rng)
                        };
                        let pull = d.normalize_or_zero() * s * dt;
                        p.velocity += pull;
                        if m.flag(1) {
                            p.base_velocity += pull;
                        }
                    }
                }
            }
            LOCATION_EMITTER_DIRECT => {
                for (i, p) in particles.iter_mut().enumerate() {
                    Self::location_emitter_direct(def, m, frame, p, i);
                }
            }
            _ => {}
        }
        self.particles = particles;
    }
}

// ---------------------------------------------------------------------------
// The level's effects in play
// ---------------------------------------------------------------------------

/// The effects of the map in play.
#[derive(Resource, Default)]
pub struct LevelFx {
    /// The map they belong to (index into MAPS), once loaded.
    map: Option<Option<usize>>,
    data: Option<Arc<FxData>>,
    /// Effect material `n` for meshes (lit ones lit by the light at the level's
    /// middle), particles and flares.
    mesh_materials: Vec<Option<Handle<FxMaterial>>>,
    /// The same for effect meshes in the sky (their key's `sky_fog`).
    sky_materials: Vec<Option<Handle<FxMaterial>>>,
    /// Effect-mesh entities that arrived before their materials (with whether they
    /// are in the sky).
    pending: Vec<(Entity, usize, bool)>,
    /// Surface material `n` on light map page `p`, culling faces `c` (see
    /// SurfaceMaterial).
    surface_materials: HashMap<(usize, String, u8), Handle<SurfaceMaterial>>,
    /// Surfaces that arrived before the effects' data.
    pending_surfaces: Vec<(Entity, usize)>,
}

/// An entity of the effects (they leave with the map).
#[derive(Component)]
struct FxPart;

/// An emitter of a placed particle system: an entity drawing its particles
/// (FxDrawn).
#[derive(Component)]
struct Emitter {
    sim: EmitterInstance,
    /// The entity's place (Bevy's axes), which its particles are drawn from.
    anchor: Vec3,
}

/// A map's lens flares drawn with one material: an entity drawing every visible
/// flare's elements with that material (FxDrawn).
#[derive(Component, Clone, Copy)]
struct FlareMesh {
    material: usize,
}

/// Split screen (split.rs): an effect entity's copy for the guest's view, drawing
/// the same particles (or flares) as his camera sees them, from a slot of its own,
/// on his view's layer (the entity itself on the first player's).
#[derive(Component)]
struct Twin(Entity);

/// The effect entity a copy is of.
#[derive(Component)]
struct TwinOf(Entity);

/// An effect on one player's screen (FxSystems::see_only): only his view shows it
/// (no copy), drawn for his camera.
#[derive(Component, Clone, Copy)]
struct SeenBy(usize);

fn white_image(images: &mut Assets<Image>) -> Handle<Image> {
    images.add(Image::new_fill(
        bevy::render::render_resource::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        &[255, 255, 255, 255],
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ))
}

/// The light a lit effect at `at` (Unreal's units) gets, as the game's particle
/// light environments give it: the level's lights that reach the spot gathered
/// into one light from the way most of their light comes (falling on the smoke by
/// how squarely it faces that way, as the game's lights fall on its characters),
/// what comes from elsewhere as light from all around (its average over every
/// facing, a quarter), and the sun.
///
/// (Each light's light was once added whole as light from all around: smoke
/// facing away from a light was lit as if facing it, several times too bright
/// beside the lights that are there to light the characters.) Only the maps'
/// placed effects are lit: what the game spawns takes no light (spawned_material).
fn light_at(lighting: Option<&MapLighting>, at: Vec3) -> FxParams {
    let mut ambient = Vec3::splat(AMBIENT);
    let mut sun = Vec3::ZERO;
    let mut sun_dir = Vec3::Z;
    // (Each light's light at the spot and the way to it, Unreal's axes.)
    let mut lights: Vec<(Vec3, Vec3)> = Vec::new();
    if let Some(l) = lighting {
        if let Some(s) = &l.sun {
            let c = s.color.to_linear();
            sun = Vec3::new(c.red, c.green, c.blue) * s.brightness;
            sun_dir = dir_to_ue(-s.direction).normalize_or_zero();
        }
        let p = to_bevy(at);
        for light in &l.lights {
            let d = light.position.distance(p);
            if light.radius <= 0.0 || d >= light.radius {
                continue;
            }
            let falloff = (1.0 - (d / light.radius).powi(2))
                .max(0.0)
                .powf(light.falloff.max(0.01));
            let towards = (light.position - p).normalize_or(Vec3::Y);
            // (A spot light's cone, as the game's: the square of how far into it.)
            let cone = light.spot.map_or(1.0, |(dir, inner, outer)| {
                let cos_outer = outer.to_radians().cos();
                let cos_inner = inner.to_radians().cos().max(cos_outer + 1e-4);
                let k =
                    ((-towards).dot(dir.normalize_or_zero()) - cos_outer) / (cos_inner - cos_outer);
                k.clamp(0.0, 1.0).powi(2)
            });
            let c = light.color.to_linear();
            let strength = Vec3::new(c.red, c.green, c.blue) * light.brightness * falloff * cone;
            if strength.max_element() > 0.0 {
                lights.push((strength, dir_to_ue(towards)));
            }
        }
    }
    let (light, light_dir) = gather_lights(&lights);
    ambient += light_around(&lights, light_dir);
    FxParams {
        ambient: ambient.extend(1.0),
        sun: sun.extend(1.0),
        sun_dir: sun_dir.extend(0.0),
        misc: Vec4::ZERO,
        light: light.extend(1.0),
        light_dir: light_dir.extend(0.0),
    }
}

/// Lights (strength, way to it) gathered into one: the way most of their light
/// comes (weighed by brightness) and how much of each comes that way.
fn gather_lights(lights: &[(Vec3, Vec3)]) -> (Vec3, Vec3) {
    let brightness = |c: Vec3| c.dot(Vec3::new(0.3, 0.59, 0.11));
    let way = lights
        .iter()
        .map(|(c, d)| *d * brightness(*c))
        .sum::<Vec3>()
        .normalize_or(Vec3::Z);
    let light = lights
        .iter()
        .map(|(c, d)| *c * d.dot(way).max(0.0))
        .sum::<Vec3>();
    (light, way)
}

/// What of the lights doesn't come the gathered light's way: light from all
/// around, at its average over every facing (a quarter).
fn light_around(lights: &[(Vec3, Vec3)], way: Vec3) -> Vec3 {
    lights
        .iter()
        .map(|(c, d)| *c * (1.0 - d.dot(way).max(0.0)) * 0.25)
        .sum::<Vec3>()
}

#[allow(clippy::too_many_arguments)]
fn make_material(
    def: &MaterialDef,
    map: usize,
    index: usize,
    kind: Kind,
    lit: bool,
    params: FxParams,
    asset_server: &AssetServer,
    white: &Handle<Image>,
) -> FxMaterial {
    let textures = material_textures(def, asset_server, white);
    let mut params = params;
    params.misc.x = def.clip;
    // (Its own cube map, if it samples one.)
    let cube = def.cube_texture.as_ref().map(|path| {
        asset_server.load_with_settings(path.clone(), |s: &mut ImageLoaderSettings| {
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                ..default()
            });
        })
    });
    FxMaterial {
        params,
        tex0: textures[0].clone(),
        tex1: textures[1].clone(),
        tex2: textures[2].clone(),
        tex3: textures[3].clone(),
        tex4: textures[4].clone(),
        tex5: textures[5].clone(),
        key: FxKey {
            map: map as u8,
            mat: index as u16,
            blend: def.blend,
            lit: def.lit && lit,
            two_sided: def.two_sided,
            kind,
            part: ModPart::Whole,
            fog: def.fog,
            sky_fog: false,
            cube: cube.is_some(),
            distort: def.distort && distorts_in(def.blend, ModPart::Whole),
            distorting: def.distort,
            pulled: kind != Kind::Mesh,
        },
        cube,
        words: FX_WORDS_BUFFER,
        alpha: match def.blend {
            Blend::Modulate => AlphaMode::Multiply,
            Blend::Masked => AlphaMode::Mask(def.clip),
            _ => AlphaMode::Premultiplied,
        },
        order: 0.0,
    }
}

/// How much nearer the camera each of an effect's emitters is counted than the one
/// before it (metres), so that they are drawn in the effect's order as the game
/// draws a particle system's emitters, one after another over what is behind (a
/// tank shell's fire, FireLinger, the eighth, over the smoke of the sixth and
/// seventh); their meshes all stand where the effect does, so they were drawn in
/// whatever order the renderer met them.
const EMITTER_ORDER: f32 = 0.002;

/// The draw order bias of an effect's emitter `index` (EMITTER_ORDER).
fn emitter_order(index: usize) -> f32 {
    index as f32 * EMITTER_ORDER
}

/// How sharply the effects' textures are filtered where they are seen stretched
/// (anisotropic filtering, the most samples a pixel takes along its long side). The
/// game filters them so: nearly all of them are in its default texture group,
/// TEXTUREGROUP_World (their packages set no LODGroup; a handful of colour ramps
/// are TEXTUREGROUP_Effects), which Unreal filters anisotropically. It shows on
/// beams and trails, their textures drawn out along them: the Electric Fences'
/// lightning (LightningBeamGrayPack_CLR, 512 texels along 31 m and 256 across 2 m)
/// filtered evenly came from a mip four times smaller, its thin bolts smeared into a
/// dim haze; filtered so, they stay sharp and bright, as in the game's videos. (At
/// the Graphics level the demo started at, 16 at High: graphics.rs, which sets the
/// mips they skip too, as the maps' textures do.)
fn fx_sampler(u: ImageAddressMode, v: ImageAddressMode) -> ImageSamplerDescriptor {
    let level = crate::graphics::start_level();
    ImageSamplerDescriptor {
        address_mode_u: u,
        address_mode_v: v,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: level.fx_anisotropy,
        lod_min_clamp: level.texture_lod_bias as f32,
        ..default()
    }
}

/// A material's textures (six, white where it has fewer).
fn material_textures(
    def: &MaterialDef,
    asset_server: &AssetServer,
    white: &Handle<Image>,
) -> Vec<Handle<Image>> {
    let mut textures: Vec<Handle<Image>> = def
        .textures
        .iter()
        .map(|(path, srgb, u, v)| {
            let (srgb, u, v) = (*srgb, *u, *v);
            asset_server.load_with_settings(path.clone(), move |s: &mut ImageLoaderSettings| {
                s.is_srgb = srgb;
                s.sampler = ImageSampler::Descriptor(fx_sampler(u, v));
            })
        })
        .collect();
    textures.resize(6, white.clone());
    textures
}

/// A material's part (ModPart): Dark multiplies as before; Bright adds,
/// premultiplied with no alpha, the picture as its distortion moves it; Distort,
/// drawn before the rest of it, moves the picture (distort_blend).
fn mod_part(mut m: FxMaterial, part: ModPart, def: &MaterialDef) -> FxMaterial {
    m.key.part = part;
    m.key.distort = def.distort && distorts_in(def.blend, part);
    match part {
        ModPart::Bright => {
            m.alpha = AlphaMode::Premultiplied;
            // (Over its dark part.)
            m.order += EMITTER_ORDER * 0.5;
        }
        ModPart::Distort => {
            m.alpha = AlphaMode::Premultiplied;
            // (Under the rest of it, over the emitter before.)
            m.order -= EMITTER_ORDER * 0.25;
        }
        _ => {}
    }
    m
}

/// Whether a material's distortion is drawn in this part: its Distort part's, and
/// a modulating material's bright part's (the picture it adds, moved); a map's
/// effects, drawn whole, draw none (none of the maps' materials distorts).
fn distorts_in(blend: Blend, part: ModPart) -> bool {
    match part {
        ModPart::Distort => true,
        ModPart::Bright => blend == Blend::Modulate,
        _ => false,
    }
}

/// The vertex data of a frame's sprites (or trail, beams, mesh particles, decals or
/// lens flares) as the frame's effect data holds it (FxWords): VERTEX_WORDS words to
/// a vertex (Bevy's axes, from the entity), and its triangles' indices.
#[derive(Clone)]
struct SpriteBuffers {
    verts: Vec<u32>,
    idx: Vec<u32>,
    /// The box round the vertices' places (`lo` above `hi` with none).
    lo: Vec3,
    hi: Vec3,
}

impl Default for SpriteBuffers {
    fn default() -> Self {
        SpriteBuffers::with_capacity(0, 0)
    }
}

impl SpriteBuffers {
    /// Room for `vertices` and `indices` (the buffers would otherwise grow as they
    /// fill, every emitter's, every frame).
    fn with_capacity(vertices: usize, indices: usize) -> Self {
        SpriteBuffers {
            verts: Vec::with_capacity(vertices * VERTEX_WORDS),
            idx: Vec::with_capacity(indices),
            lo: Vec3::splat(f32::MAX),
            hi: Vec3::splat(f32::MIN),
        }
    }

    fn vertex_count(&self) -> usize {
        self.verts.len() / VERTEX_WORDS
    }

    fn is_empty(&self) -> bool {
        self.verts.is_empty()
    }

    /// One vertex (Bevy's axes, relative to the entity); returns its index. Its
    /// words are in the shader's order (fx_vertex): place, normal, uv0, uv1,
    /// tangent, colour, parameters, extra.
    #[allow(clippy::too_many_arguments)]
    fn vertex(
        &mut self,
        pos: Vec3,
        normal: Vec3,
        tangent: Vec4,
        uv0: Vec2,
        uv1: Vec2,
        color: Vec4,
        param: Vec4,
        extra: Vec4,
    ) -> u32 {
        let i = self.vertex_count() as u32;
        self.lo = self.lo.min(pos);
        self.hi = self.hi.max(pos);
        let v: [f32; VERTEX_WORDS] = [
            pos.x, pos.y, pos.z, normal.x, normal.y, normal.z, uv0.x, uv0.y, uv1.x, uv1.y,
            tangent.x, tangent.y, tangent.z, tangent.w, color.x, color.y, color.z, color.w,
            param.x, param.y, param.z, param.w, extra.x, extra.y, extra.z, extra.w,
        ];
        self.verts.extend_from_slice(&v.map(f32::to_bits));
        i
    }

    /// A quad: corners in Bevy's axes (relative to the entity), facing `normal`,
    /// with its right (U) along `right` and V running along `down`.
    #[allow(clippy::too_many_arguments)]
    fn quad(
        &mut self,
        corners: [Vec3; 4],
        normal: Vec3,
        right: Vec3,
        down: Vec3,
        uv0: [Vec2; 4],
        uv1: [Vec2; 4],
        color: Vec4,
        param: Vec4,
        extra: Vec4,
    ) {
        // (Counter-clockwise on screen: the sprites' front faces.)
        let v = self.vertex_count() as u32;
        self.idx
            .extend_from_slice(&[v, v + 2, v + 1, v, v + 3, v + 2]);
        let tangent = tangent_for(normal, right, down);
        for k in 0..4 {
            self.vertex(
                corners[k], normal, tangent, uv0[k], uv1[k], color, param, extra,
            );
        }
    }

    /// Vertex `i`'s place.
    #[cfg(test)]
    fn position(&self, i: usize) -> Vec3 {
        let w = &self.verts[i * VERTEX_WORDS..];
        Vec3::new(
            f32::from_bits(w[0]),
            f32::from_bits(w[1]),
            f32::from_bits(w[2]),
        )
    }
}

/// Draws an emitter's sprites (into its FxDrawn), or, with none, hides it (an empty
/// one would still cost its draw), shown again with the next it has. Sprites out of
/// the camera's view (`cam`; `anchor` is the entity's place, which they are drawn
/// from) are hidden too, what it drew left as it was: a big wave's effects all round
/// the camera are a few hundred, whose vertices needn't go to the GPU or be drawn.
/// True if it drew them.
fn show_sprites(
    drawn: &mut FxDrawn,
    sprites: SpriteBuffers,
    (cam, anchor): (&CameraView, Vec3),
    visibility: &mut Visibility,
) -> bool {
    if sprites.is_empty() || !cam.sees_box(anchor, sprites.lo, sprites.hi) {
        if !drawn.hidden {
            *visibility = Visibility::Hidden;
            drawn.hidden = true;
        }
        return false;
    }
    drawn.sprites = sprites;
    if drawn.hidden {
        *visibility = Visibility::Inherited;
        drawn.hidden = false;
    }
    true
}

// ---------------------------------------------------------------------------
// The frame's effect data
// ---------------------------------------------------------------------------

/// Words to a vertex in the frame's effect data (FxWords): its place, normal (3
/// each), uv0, uv1 (2 each), tangent, colour, parameters and extra (4 each).
const VERTEX_WORDS: usize = 26;

/// Words to an entity's header there: where its vertices start, where its indices
/// start, how many indices it has, and one unused.
const HEADER_WORDS: usize = 4;

/// The frame's effect data's buffer: one, fixed, so every effect material can bind
/// it (FxMaterial's `words`).
const FX_WORDS_BUFFER: Handle<ShaderStorageBuffer> =
    bevy::asset::uuid_handle!("3f6b2c1e-9d47-4e15-b8a2-6c0f5e7d1a93");

/// The buffer's size to start with (words: 4 MB), doubled as the data outgrows it.
const FX_WORDS_START: usize = 1 << 20;

/// The materials are made again (to bind the buffer anew) as it grows, and this
/// many frames later again (see gather_fx_words).
const REMAKE_AFTER: u8 = 3;

/// How many frames the render world goes on writing a buffer it grew from.
const OLD_BUFFER_FRAMES: u8 = 10;

/// The buckets' ids (bucket_handle): fixed, so an entity can be spawned with one.
const BUCKET_UUID: u128 = 0x51c8_3e0a_7b6d_4f29_9e14_0000_0000_0000;

/// The biggest bucket: 6 << MAX_BUCKET vertices (some six million).
const MAX_BUCKET: u8 = 20;

/// An effect entity drawn from the frame's effect data (FxWords): an emitter (its
/// particles) or a flare mesh (its flares), with what it last drew. Its mesh only
/// numbers its vertices: a bucket of at least as many as it has indices
/// (bucket_mesh), which its other parts (a modulating material's, its children)
/// share; its slot, its header's place in the data, is its mesh tag, and is given
/// back as it goes.
#[derive(Component)]
#[component(on_remove = give_back_slot)]
struct FxDrawn {
    slot: u32,
    /// What it last drew (left as it was while it is hidden).
    sprites: SpriteBuffers,
    /// Hidden: nothing to draw, or out of view (show_sprites).
    hidden: bool,
    /// Its mesh's bucket.
    bucket: u8,
}

impl FxDrawn {
    /// Hidden, with nothing drawn yet, its mesh the smallest bucket.
    fn new(slot: u32) -> Self {
        FxDrawn {
            slot,
            sprites: SpriteBuffers::default(),
            hidden: true,
            bucket: 0,
        }
    }
}

fn give_back_slot(mut world: DeferredWorld, context: HookContext) {
    let Some(slot) = world.get::<FxDrawn>(context.entity).map(|d| d.slot) else {
        return;
    };
    if let Some(mut slots) = world.get_resource_mut::<FxSlots>() {
        slots.free.push(slot);
    }
}

/// The effect entities' slots in the frame's effect data (their mesh tags). Slot 0
/// is none's, always empty: the meshes spawned only to have their pipelines made
/// early (start_spawned_fx) draw nothing.
#[derive(Resource)]
struct FxSlots {
    /// One past the highest given out.
    next: u32,
    /// Given back, to give out again.
    free: Vec<u32>,
}

impl Default for FxSlots {
    fn default() -> Self {
        FxSlots {
            next: 1,
            free: Vec::new(),
        }
    }
}

impl FxSlots {
    fn take(&mut self) -> u32 {
        self.free.pop().unwrap_or_else(|| {
            self.next += 1;
            self.next - 1
        })
    }
}

/// The frame's effect data, which particles, trails, beams, decals and lens flares
/// are drawn from (FxKey's `pulled`; the shader's fx_words): for each slot (FxSlots)
/// a header of HEADER_WORDS words, where its entity's vertices start, where its
/// indices start and how many indices it has (none for a hidden one); then each
/// shown entity's indices and vertices (VERTEX_WORDS words each). gather_fx_words
/// builds it each frame from what they drew (FxDrawn), and the render world writes
/// it whole to the buffer every effect material binds (FX_WORDS_BUFFER).
///
/// Each of them was a mesh of its own, written every frame it changed: every
/// changed mesh is sent to the GPU anew, its old place there freed and a new one
/// found, its bounds worked out and its pipeline checked again, and the renderer
/// looks over every mesh there is, each frame and view; a big wave's few hundred
/// effects cost several milliseconds a frame so. Now their meshes are buckets that
/// only number vertices (bucket_mesh), a few shared by them all and never changed,
/// and the shader reads the vertex that number picks out of its entity's indices.
#[derive(Resource)]
struct FxWords {
    words: Vec<u32>,
    /// Built this frame, not yet taken by the render world.
    fresh: bool,
    /// The buffer's size (words).
    capacity: usize,
    /// Frames since it last grew, up to REMAKE_AFTER.
    since_grown: u8,
}

impl Default for FxWords {
    fn default() -> Self {
        FxWords {
            words: Vec::new(),
            fresh: false,
            capacity: 0,
            since_grown: REMAKE_AFTER,
        }
    }
}

/// The render world's side of the frame's effect data (FxWords).
#[derive(Resource, Default)]
struct FxWordsRender {
    words: Vec<u32>,
    fresh: bool,
    /// The buffer it last wrote.
    current: Option<Buffer>,
    /// Buffers it grew from, to be written this many frames more.
    old: Vec<(Buffer, u8)>,
}

/// Bucket `k`'s mesh (bucket_mesh).
fn bucket_handle(k: u8) -> Handle<Mesh> {
    Handle::Uuid(
        bevy::asset::uuid::Uuid::from_u128(BUCKET_UUID | k as u128),
        std::marker::PhantomData,
    )
}

/// Bucket `k`: a mesh of 6 << k vertices, each only its number (its place's x), for
/// an effect entity with up to that many indices (FxDrawn).
fn bucket_mesh(k: u8) -> Mesh {
    let n = 6usize << k;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        (0..n)
            .map(|i| [i as f32, 0.0, 0.0])
            .collect::<Vec<[f32; 3]>>(),
    );
    mesh
}

/// The smallest bucket with room for `indices`.
fn bucket_for(indices: usize) -> u8 {
    let mut k = 0;
    while (6usize << k) < indices && k < MAX_BUCKET {
        k += 1;
    }
    k
}

/// The frame's effect data's buffer, `words` long.
fn words_buffer(words: usize) -> ShaderStorageBuffer {
    let mut buffer = ShaderStorageBuffer::with_size(words * 4, RenderAssetUsages::default());
    buffer.buffer_description.label = Some("fx_words");
    // (Written each frame: write_fx_words.)
    buffer.buffer_description.usage |= BufferUsages::COPY_DST;
    buffer
}

/// Makes the frame's effect data's buffer, and the smallest bucket, which effect
/// entities start with.
fn make_fx_words(
    mut words: ResMut<FxWords>,
    storage: Option<ResMut<Assets<ShaderStorageBuffer>>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let _ = meshes.insert(bucket_handle(0).id(), bucket_mesh(0));
    if let Some(mut storage) = storage {
        let _ = storage.insert(FX_WORDS_BUFFER.id(), words_buffer(FX_WORDS_START));
        words.capacity = FX_WORDS_START;
    }
}

/// Builds the frame's effect data (FxWords) from what the effect entities last drew
/// (FxDrawn): each shown one's header, indices and vertices. One whose indices have
/// outgrown its bucket is given one big enough (its other parts too), and the
/// buffer grows as the data does: made anew, which the materials binding it have to
/// be too. A material is made in the render world as it changes, maybe before the
/// new buffer is: so they are made again REMAKE_AFTER frames later, and meanwhile
/// the render world writes the old buffer too (write_fx_words).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn gather_fx_words(
    mut words: ResMut<FxWords>,
    slots: Res<FxSlots>,
    mut drawn: Query<(&mut FxDrawn, &Visibility, &mut Mesh3d, Option<&Children>)>,
    mut parts: Query<&mut Mesh3d, Without<FxDrawn>>,
    mut meshes: ResMut<Assets<Mesh>>,
    storage: Option<ResMut<Assets<ShaderStorageBuffer>>>,
    mut materials: ResMut<Assets<FxMaterial>>,
    device: Option<Res<RenderDevice>>,
) {
    let words = &mut *words;
    let out = &mut words.words;
    out.clear();
    out.resize(slots.next as usize * HEADER_WORDS, 0);
    for (mut d, visibility, mut mesh, children) in &mut drawn {
        let count = d.sprites.idx.len();
        let at = d.slot as usize * HEADER_WORDS;
        if d.hidden
            || count == 0
            || *visibility == Visibility::Hidden
            || at + HEADER_WORDS > out.len()
        {
            continue;
        }
        let indices = out.len();
        out.extend_from_slice(&d.sprites.idx);
        let vertices = out.len();
        out.extend_from_slice(&d.sprites.verts);
        out[at] = vertices as u32;
        out[at + 1] = indices as u32;
        out[at + 2] = count as u32;
        let bucket = bucket_for(count);
        if bucket > d.bucket {
            d.bucket = bucket;
            let handle = bucket_handle(bucket);
            if !meshes.contains(handle.id()) {
                let _ = meshes.insert(handle.id(), bucket_mesh(bucket));
            }
            for &part in children.map(|c| &c[..]).unwrap_or_default() {
                if let Ok(mut m) = parts.get_mut(part) {
                    m.0 = handle.clone();
                }
            }
            mesh.0 = handle;
        }
    }
    words.fresh = true;

    let need = words.words.len();
    if need > words.capacity
        && let Some(mut storage) = storage
    {
        let most = device.map_or(usize::MAX, |d| {
            d.limits().max_storage_buffer_binding_size as usize / 4
        });
        let mut size = words.capacity.max(FX_WORDS_START);
        while size < need && size < most {
            size *= 2;
        }
        let size = size.min(most);
        if size > words.capacity {
            info!(
                "Effect data: its buffer grows to {} MB ({} MB this frame)",
                (size * 4) >> 20,
                (need * 4) >> 20
            );
            let _ = storage.insert(FX_WORDS_BUFFER.id(), words_buffer(size));
            words.capacity = size;
            words.since_grown = 0;
            for _ in materials.iter_mut() {}
        }
    }
    if words.since_grown < REMAKE_AFTER {
        words.since_grown += 1;
        if words.since_grown == REMAKE_AFTER {
            for _ in materials.iter_mut() {}
        }
    }
}

/// Takes the frame's effect data from the main world as it was built (FxWords),
/// giving it back the render world's last to build the next in.
fn extract_fx_words(mut main: ResMut<MainWorld>, mut render: ResMut<FxWordsRender>) {
    let Some(mut words) = main.get_resource_mut::<FxWords>() else {
        return;
    };
    if !words.fresh {
        return;
    }
    words.fresh = false;
    std::mem::swap(&mut words.words, &mut render.words);
    render.fresh = true;
}

/// Writes the frame's effect data to its buffer, and for a few frames to any it
/// grew from, as much as each holds (see gather_fx_words): the shader reads none
/// past the end of what it binds, so an entity whose data didn't fit is left out.
fn write_fx_words(
    mut words: ResMut<FxWordsRender>,
    buffers: Res<RenderAssets<GpuShaderStorageBuffer>>,
    queue: Res<RenderQueue>,
) {
    let words = &mut *words;
    if !std::mem::take(&mut words.fresh) {
        return;
    }
    let Some(gpu) = buffers.get(FX_WORDS_BUFFER.id()) else {
        return;
    };
    let grown = words
        .current
        .as_ref()
        .is_none_or(|b| b.id() != gpu.buffer.id());
    if grown {
        let old = words.current.replace(gpu.buffer.clone());
        words.old.extend(old.map(|b| (b, OLD_BUFFER_FRAMES)));
    }
    write_words(&queue, &gpu.buffer, &words.words);
    for (buffer, frames) in &mut words.old {
        write_words(&queue, buffer, &words.words);
        *frames -= 1;
    }
    words.old.retain(|(_, frames)| *frames > 0);
}

/// Writes `words` to the start of `buffer`, as many as it holds.
fn write_words(queue: &RenderQueue, buffer: &Buffer, words: &[u32]) {
    let n = words.len().min(buffer.size() as usize / 4);
    let Some(size) = BufferSize::new(n as u64 * 4) else {
        return;
    };
    let Some(mut view) = queue.write_buffer_with(buffer, 0, size) else {
        return;
    };
    // (Written only: reading the view is slow, and logs a warning each time.)
    let bytes: &mut [u8] = view.as_mut();
    for (to, word) in bytes.chunks_exact_mut(4).zip(words) {
        to.copy_from_slice(&word.to_le_bytes());
    }
}

/// Puts the effects of the map in play (when it changes): its emitters (warmed up)
/// and lens flares, and the materials its effect meshes take.
#[allow(clippy::too_many_arguments)]
fn follow_map(
    mut commands: Commands,
    current: Res<crate::CurrentMap>,
    garage: Res<GarageScene>,
    mut fx: ResMut<LevelFx>,
    parts: Query<Entity, With<FxPart>>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FxMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    lighting: Option<Res<MapLighting>>,
    mut spawned: ResMut<FxSystems>,
    mut slots: ResMut<FxSlots>,
) {
    // (The garage's while the Customize screen is up.)
    let want = if garage.0 {
        Some(GARAGE_SET as usize)
    } else {
        current.0
    };
    if fx.map == Some(want) {
        return;
    }
    // (The spawned effects go with the map too.)
    for e in &parts {
        commands.entity(e).despawn();
    }
    spawned.live.clear();
    fx.map = Some(want);
    fx.data = None;
    fx.mesh_materials.clear();
    fx.sky_materials.clear();
    fx.surface_materials.clear();
    let Some(map) = want else {
        return;
    };
    let in_garage = map == GARAGE_SET as usize;
    let stem = if in_garage {
        GARAGE_STEM
    } else {
        crate::MAPS[map].stem
    };
    let path = crate::assets_dir().join(format!("levels/{stem}_fx.txt"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let data = Arc::new(FxData::parse(&text));
    let white = white_image(&mut images);
    // The garage's own lights, and its render layer (the garage camera's).
    let garage_lighting = in_garage
        .then(|| MapLighting::load_file(&crate::assets_dir(), GARAGE_LIGHTS))
        .flatten();
    let lighting = if in_garage {
        garage_lighting.as_ref()
    } else {
        lighting.as_deref()
    };
    // (The game's on the first player's view's layer: split screen's guest's view
    // has copies of them, make_twins.)
    let layer = if in_garage {
        RenderLayers::layer(crate::customize::GARAGE_LAYER)
    } else {
        RenderLayers::layer(crate::split::VIEW_LAYERS[0])
    };

    // The effect meshes' materials (lit ones by the light at the level's middle).
    let middle = data
        .placements
        .iter()
        .map(|p| p.origin)
        .fold(Vec3::ZERO, |a, b| a + b)
        / data.placements.len().max(1) as f32;
    // (Those that reflect a cube map reflect the one the map's characters do.)
    let cube = lighting.and_then(|l| l.reflect.as_ref()).map(|(path, _)| {
        asset_server.load_with_settings(path.clone(), |s: &mut ImageLoaderSettings| {
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                ..default()
            });
        })
    });
    fx.mesh_materials = data
        .materials
        .iter()
        .enumerate()
        .map(|(k, def)| {
            let mut m = make_material(
                def,
                map,
                k,
                Kind::Mesh,
                true,
                light_at(lighting, middle),
                &asset_server,
                &white,
            );
            if def.reflects && !in_garage && m.cube.is_none() && cube.is_some() {
                m.cube = cube.clone();
                m.key.cube = true;
            }
            Some(materials.add(m))
        })
        .collect();
    fx.sky_materials = fx
        .mesh_materials
        .iter()
        .map(|m| {
            let mut sky = materials.get(m.as_ref()?)?.clone();
            sky.key.sky_fog = true;
            Some(materials.add(sky))
        })
        .collect();

    // The particle systems: an entity per emitter of each placement.
    let mut shared: HashMap<(usize, bool, usize), Handle<FxMaterial>> = HashMap::new();
    let mut seed = 1u32;
    for (pi, place) in data.placements.iter().enumerate() {
        let Some(system) = data.systems.get(place.system) else {
            continue;
        };
        let frame = SimFrame {
            origin: place.origin,
            axes: place.axes,
            size: axes_scale(&place.axes),
            params: &place.params,
            vectors: &[],
            prev_origin: place.origin,
            velocity: Vec3::ZERO,
            sources: &[],
        };
        for (ei, em) in system.emitters.iter().enumerate() {
            let Some(def) = data.materials.get(em.material) else {
                continue;
            };
            if matches!(em.typedata, TypeData::Mesh(usize::MAX)) {
                continue;
            }
            let mesh_kind = matches!(em.typedata, TypeData::Mesh(_));
            let kind = if mesh_kind {
                Kind::MeshParticle
            } else {
                Kind::Particle
            };
            let material = if def.lit && system.lit && place.lit {
                let mut m = make_material(
                    def,
                    map,
                    em.material,
                    kind,
                    true,
                    light_at(lighting, place.origin),
                    &asset_server,
                    &white,
                );
                m.order = emitter_order(ei);
                materials.add(m)
            } else {
                shared
                    .entry((em.material, mesh_kind, ei))
                    .or_insert_with(|| {
                        let mut m = make_material(
                            def,
                            map,
                            em.material,
                            kind,
                            false,
                            FxParams::default(),
                            &asset_server,
                            &white,
                        );
                        m.order = emitter_order(ei);
                        materials.add(m)
                    })
                    .clone()
            };
            seed = seed.wrapping_add(0x9e37_79b9);
            let mut sim = EmitterInstance::new(em, place.system, ei, pi, seed);
            let warmup = system.warmup.max(MIN_WARMUP);
            let mut t = 0.0;
            while t < warmup {
                sim.tick(em, &frame, WARMUP_STEP, true);
                t += WARMUP_STEP;
            }
            let anchor = to_bevy(place.origin);
            let slot = slots.take();
            commands.spawn((
                FxPart,
                Emitter { sim, anchor },
                FxDrawn::new(slot),
                MeshTag(slot),
                Mesh3d(bucket_handle(0)),
                MeshMaterial3d(material),
                Transform::from_translation(anchor),
                // (Shown once it has something to draw.)
                Visibility::Hidden,
                // (Its mesh says nothing of where its particles are: it is culled
                // by them, show_sprites.)
                NoFrustumCulling,
                NotShadowCaster,
                NotShadowReceiver,
                layer.clone(),
            ));
        }
    }

    // The lens flares: a mesh per material their elements use.
    let mut flare_materials: Vec<usize> = data
        .flares
        .iter()
        .flat_map(|f| f.elements.iter().map(|e| e.material))
        .collect();
    flare_materials.sort_unstable();
    flare_materials.dedup();
    for m in flare_materials {
        let Some(def) = data.materials.get(m) else {
            continue;
        };
        let material = materials.add(make_material(
            def,
            map,
            m,
            Kind::Flare,
            false,
            FxParams::default(),
            &asset_server,
            &white,
        ));
        let slot = slots.take();
        commands.spawn((
            FxPart,
            FlareMesh { material: m },
            FxDrawn::new(slot),
            MeshTag(slot),
            Mesh3d(bucket_handle(0)),
            MeshMaterial3d(material),
            Transform::default(),
            // (Shown while a flare shows.)
            Visibility::Hidden,
            NoFrustumCulling,
            NotShadowCaster,
            NotShadowReceiver,
            layer.clone(),
        ));
    }

    // Materials that read the scene behind (SceneTexture) or shift it (their
    // Distortion) need Bevy to keep a copy of it: it does while something
    // transmissive is drawn, so an invisible (zero-area) transmissive triangle is.
    if data
        .materials
        .iter()
        .any(|m| m.scene || (m.distort && distorts_in(m.blend, ModPart::Whole)))
    {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
        mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
        commands.spawn((
            FxPart,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(standard.add(StandardMaterial {
                base_color: Color::NONE,
                specular_transmission: 1.0,
                unlit: true,
                ..default()
            })),
            Transform::default(),
            NoFrustumCulling,
            NotShadowCaster,
            NotShadowReceiver,
            layer,
        ));
    }
    fx.data = Some(data);
}

/// The material extras' "fx:<n>": effect material n.
fn fx_flag(extras: &str) -> Option<usize> {
    let i = extras.find("fx:")?;
    extras[i + 3..]
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// Gives an effect mesh the vertex data the effect shader reads that it doesn't
/// have: a second UV set (a copy of the first, as Unreal reads a mesh's missing UV
/// sets), tangents (worked out from its UVs, MikkTSpace, as the glTF loader does
/// for a normal map; along X if they can't be) and vertex colours (white, as
/// Unreal's for a mesh with none). Without any of them the mesh can't be drawn
/// with an effect material at all. Oblivion's force fields over its locked doors
/// (`ForcefieldPlane_STAT` in models/oblivion_doors.glb, an InterpActor's mesh,
/// with no light map: one UV set, no tangents, no colours) were left undrawn so,
/// Bevy logging "Mesh is missing requested attribute: Vertex_Uv_1" (then
/// "Vertex_Tangent") each frame.
fn fill_fx_attributes(meshes: &mut Assets<Mesh>, handle: &Handle<Mesh>) {
    let wants = |m: &Mesh| {
        !m.contains_attribute(Mesh::ATTRIBUTE_UV_1)
            || !m.contains_attribute(Mesh::ATTRIBUTE_TANGENT)
            || !m.contains_attribute(Mesh::ATTRIBUTE_COLOR)
    };
    if !meshes.get(handle).is_some_and(wants) {
        return;
    }
    let Some(mesh) = meshes.get_mut(handle) else {
        return;
    };
    if !mesh.contains_attribute(Mesh::ATTRIBUTE_UV_1)
        && let Some(uv0) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).cloned()
    {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv0);
    }
    let count = mesh.count_vertices();
    if !mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT) && mesh.generate_tangents().is_err() {
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_TANGENT,
            vec![[1.0f32, 0.0, 0.0, 1.0]; count],
        );
    }
    if !mesh.contains_attribute(Mesh::ATTRIBUTE_COLOR) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; count]);
    }
}

/// Gives the map's effect meshes (glTF material extras "fx:<n>") their effect
/// materials: those in the sky ("skyfog") the copies seen through the fog their
/// vertex colours carry. (Those without all the vertex data the effect shader
/// reads are given the rest: fill_fx_attributes.)
fn swap_effect_meshes(
    mut commands: Commands,
    mut fx: ResMut<LevelFx>,
    parts: Query<(Entity, &GltfMaterialExtras), Added<GltfMaterialExtras>>,
    mesh_handles: Query<&Mesh3d>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mut waiting = std::mem::take(&mut fx.pending);
    for (entity, extras) in &parts {
        if let Some(n) = fx_flag(&extras.value) {
            let sky = extras
                .value
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|w| w == "skyfog");
            waiting.push((entity, n, sky));
        }
    }
    for (entity, n, sky) in waiting {
        let found = if sky {
            &fx.sky_materials
        } else {
            &fx.mesh_materials
        };
        match found.get(n).cloned().flatten() {
            Some(material) => {
                if let Ok(Mesh3d(mesh)) = mesh_handles.get(entity) {
                    fill_fx_attributes(&mut meshes, mesh);
                }
                if let Ok(mut e) = commands.get_entity(entity) {
                    e.remove::<MeshMaterial3d<StandardMaterial>>().insert((
                        MeshMaterial3d(material),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            None if fx.data.is_none() => fx.pending.push((entity, n, sky)),
            None => {}
        }
    }
}

/// How strong a surface's reflection of the level's cube map is (the game's
/// ReflectionCubeMapIntensityScale, which its cube maps leave at 1).
const REFLECTION_INTENSITY: f32 = 1.0;

/// A 1 x 1 black cube map.
fn black_cube_image() -> Image {
    let mut image = Image::new_fill(
        bevy::render::render_resource::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 6,
        },
        bevy::render::render_resource::TextureDimension::D2,
        &[0, 0, 0, 255],
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(bevy::render::render_resource::TextureViewDescriptor {
        dimension: Some(bevy::render::render_resource::TextureViewDimension::Cube),
        ..default()
    });
    image
}

/// The material extras' "surface:<n>": surface material n.
fn surface_flag(extras: &str) -> Option<usize> {
    let i = extras.find("surface:")?;
    extras[i + 8..]
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// Gives the surfaces drawn with their game materials (glTF material extras
/// "surface:<n>") their SurfaceMaterials, on their light maps' pages. One with no
/// light map keeps its stand-in (the texture set's StandardMaterial).
#[allow(clippy::too_many_arguments)]
fn swap_surfaces(
    mut commands: Commands,
    mut fx: ResMut<LevelFx>,
    parts: Query<(Entity, &GltfMaterialExtras), Added<GltfMaterialExtras>>,
    node_extras: Query<&GltfExtras>,
    parents: Query<&ChildOf>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<SurfaceMaterial>>,
    gltf_materials: Query<&MeshMaterial3d<StandardMaterial>>,
    standard: Res<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut white: Local<Option<Handle<Image>>>,
    mut black_cube: Local<Option<Handle<Image>>>,
) {
    let mut waiting = std::mem::take(&mut fx.pending_surfaces);
    for (entity, extras) in &parts {
        if let Some(n) = surface_flag(&extras.value) {
            waiting.push((entity, n));
        }
    }
    if waiting.is_empty() {
        return;
    }
    // (They are the garage's: they wait for its effects' data.)
    let (Some(data), Some(Some(map))) = (fx.data.clone(), fx.map) else {
        fx.pending_surfaces = waiting;
        return;
    };
    if map != GARAGE_SET as usize {
        fx.pending_surfaces = waiting;
        return;
    }
    let white = white
        .get_or_insert_with(|| white_image(&mut images))
        .clone();
    // The level's reflection cube map (black without one: no reflections).
    let (cube, cube_mips) = match &data.cube {
        Some((path, mips)) => (
            asset_server.load_with_settings(path.clone(), |s: &mut ImageLoaderSettings| {
                s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    mag_filter: ImageFilterMode::Linear,
                    min_filter: ImageFilterMode::Linear,
                    mipmap_filter: ImageFilterMode::Linear,
                    ..default()
                });
            }),
            *mips,
        ),
        None => (
            black_cube
                .get_or_insert_with(|| images.add(black_cube_image()))
                .clone(),
            1,
        ),
    };
    for (entity, n) in waiting {
        let Some(def) = data.materials.get(n) else {
            continue;
        };
        // (Its node's light map: the entity's own extras, or its parent's.)
        let page = std::iter::once(entity)
            .chain(parents.get(entity).ok().map(|p| p.parent()))
            .filter_map(|e| node_extras.get(e).ok())
            .find_map(|x| crate::lighting::parse_lightmap(&x.value))
            .map(|(file, _)| file);
        let Some(page) = page else {
            continue;
        };
        // (The faces the glTF loader's material culls: the front ones on a mirrored
        // mesh.)
        let cull = gltf_materials
            .get(entity)
            .ok()
            .and_then(|m| standard.get(&m.0))
            .map_or(CULL_BACK, |m| match m.cull_mode {
                Some(Face::Front) => CULL_FRONT,
                None => CULL_NONE,
                _ => CULL_BACK,
            });
        let handle = fx
            .surface_materials
            .entry((n, page.clone(), cull))
            .or_insert_with(|| {
                let tex = material_textures(def, &asset_server, &white);
                let dir_path = format!("{}_dir.png", page.trim_end_matches(".png"));
                let directions =
                    asset_server.load_with_settings(dir_path, |s: &mut ImageLoaderSettings| {
                        s.is_srgb = false;
                        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                            mag_filter: ImageFilterMode::Linear,
                            min_filter: ImageFilterMode::Linear,
                            ..default()
                        });
                    });
                materials.add(SurfaceMaterial {
                    // (misc: the clip value, the light maps' range, the cube map's last
                    // mip and how much the specular power blurs it; ambient.x: the
                    // reflection's strength.)
                    params: FxParams {
                        ambient: Vec4::new(REFLECTION_INTENSITY, 0.0, 0.0, 0.0),
                        misc: Vec4::new(
                            def.clip,
                            crate::lighting::LIGHTMAP_RANGE,
                            cube_mips.saturating_sub(1) as f32,
                            def.cube_roughness,
                        ),
                        ..default()
                    },
                    tex0: tex[0].clone(),
                    tex1: tex[1].clone(),
                    tex2: tex[2].clone(),
                    tex3: tex[3].clone(),
                    tex4: tex[4].clone(),
                    tex5: tex[5].clone(),
                    directions,
                    cube: cube.clone(),
                    key: SurfaceKey {
                        map: map as u8,
                        cull,
                        mat: n as u16,
                    },
                })
            })
            .clone();
        if let Ok(mut e) = commands.get_entity(entity) {
            e.remove::<MeshMaterial3d<StandardMaterial>>()
                .insert(MeshMaterial3d(handle));
        }
    }
}

/// The camera the effects are seen by: the game camera, or the garage camera while
/// the Customize screen is up.
type FxCameras<'w, 's> = Query<
    'w,
    's,
    (
        &'static GlobalTransform,
        &'static Projection,
        Has<crate::customize::GarageCamera>,
        Option<&'static crate::split::Seat>,
    ),
    Or<(
        With<crate::OrbitCamera>,
        With<crate::customize::GarageCamera>,
    )>,
>;

/// (The first player's, of split screen's two game cameras.)
fn fx_camera<'a>(
    cameras: &'a FxCameras,
    garage: &GarageScene,
) -> Option<(&'a GlobalTransform, &'a Projection)> {
    cameras
        .iter()
        .find(|(_, _, in_garage, seat)| *in_garage == garage.0 && crate::split::seat_of(*seat) == 0)
        .map(|(t, p, ..)| (t, p))
}

/// The game camera in Unreal's axes and units: its place, right, up and forward.
struct CameraView {
    pos: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    /// tan of half the field of view, across and up.
    tan_half: Vec2,
    /// Its near plane's distance (cm).
    near: f32,
    /// How far effects are drawn (cm: the Graphics level's, FxQuality).
    far: f32,
}

/// How much wider than the camera's field of view an effect counts as in view (so
/// one that comes into view as the camera turns is drawn as it does: its mesh is
/// written before the frame's visibility is worked out), and by how much more
/// (cm) round its sprites.
const VIEW_MARGIN: f32 = 1.25;
const VIEW_MARGIN_CM: f32 = 200.0;

impl CameraView {
    /// Whether a sphere (Unreal's units) is in the camera's view, or by it
    /// (VIEW_MARGIN): in front of it, inside its four sides and no further than
    /// effects are drawn.
    fn sees(&self, centre: Vec3, radius: f32) -> bool {
        let d = centre - self.pos;
        let z = d.dot(self.forward);
        if z < -radius || d.length() - radius > self.far {
            return false;
        }
        let tan = self.tan_half * VIEW_MARGIN;
        // (A side's plane is at x = z tan; a point's distance from it, the overshoot
        // over its normal's length.)
        d.dot(self.right).abs() <= z * tan.x + radius * (1.0 + tan.x * tan.x).sqrt()
            && d.dot(self.up).abs() <= z * tan.y + radius * (1.0 + tan.y * tan.y).sqrt()
    }

    /// Whether a box (Bevy's axes, from `anchor`: its least and greatest corners;
    /// none if `lo` is above `hi`) is in view, or near it: the sphere round it.
    fn sees_box(&self, anchor: Vec3, lo: Vec3, hi: Vec3) -> bool {
        if lo.x > hi.x {
            return false;
        }
        let centre = to_ue(anchor + (lo + hi) * 0.5);
        let radius = (hi - lo).length() * 0.5 * 100.0 + VIEW_MARGIN_CM;
        self.sees(centre, radius)
    }
}

fn camera_view(transform: &GlobalTransform, projection: &Projection) -> CameraView {
    let (tx, ty, near) = match projection {
        Projection::Perspective(p) => {
            let ty = (p.fov * 0.5).tan();
            (ty * p.aspect_ratio, ty, p.near)
        }
        _ => (1.0, 1.0, 0.1),
    };
    CameraView {
        pos: to_ue(transform.translation()),
        right: dir_to_ue(transform.right().as_vec3()),
        up: dir_to_ue(transform.up().as_vec3()),
        forward: dir_to_ue(transform.forward().as_vec3()),
        tan_half: Vec2::new(tx, ty),
        near: near * 100.0,
        far: f32::MAX,
    }
}

/// Runs the emitters and rebuilds their meshes for the camera.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn simulate_emitters(
    time: Res<Time>,
    fx: Res<LevelFx>,
    level: Option<Res<crate::Level>>,
    garage: Res<GarageScene>,
    cameras: FxCameras,
    mut emitters: Query<
        (
            Entity,
            &mut Emitter,
            &mut FxDrawn,
            &mut Visibility,
            Option<&Twin>,
        ),
        Without<TwinOf>,
    >,
    mut twins: Query<(&mut FxDrawn, &mut Visibility), With<TwinOf>>,
    fxq: Res<FxQuality>,
    mut budget: ResMut<FxBudget>,
) {
    let Some(data) = fx.data.as_ref() else {
        return;
    };
    let Some((cam_transform, projection)) = fx_camera(&cameras, &garage) else {
        return;
    };
    // (The garage has no collision: nothing lands on it.)
    let level = level.filter(|_| !garage.0);
    let mut cam = camera_view(cam_transform, projection);
    // (The garage's are all near.)
    if !garage.0 {
        cam.far = fxq.far;
    }
    // (Split screen's guest's view, for the copies: make_twins.)
    let guest_cam = guest_camera_view(&cameras, &garage, Some(fxq.far));
    let dt = time.delta_secs().min(0.1);
    for (entity, mut emitter, mut drawn, mut visibility, twin) in &mut emitters {
        let emitter = &mut *emitter;
        let sim = &mut emitter.sim;
        let (Some(place), Some(system)) = (
            data.placements.get(sim.placement),
            data.systems.get(sim.system),
        ) else {
            continue;
        };
        let Some(def) = system.emitters.get(sim.emitter) else {
            continue;
        };
        let frame = SimFrame {
            origin: place.origin,
            axes: place.axes,
            size: axes_scale(&place.axes),
            params: &place.params,
            vectors: &[],
            prev_origin: place.origin,
            velocity: Vec3::ZERO,
            sources: &[],
        };
        if dt > 0.0 {
            sim.tick(def, &frame, dt, true);
        }
        if sim.particles.is_empty() && drawn.hidden {
            continue;
        }
        let distance = (to_ue(emitter.anchor) - cam.pos).length();
        let decal = matches!(def.typedata, TypeData::Decal(_));
        if budget.defer(distance, drawn.hidden || decal, entity.index_u32()) {
            continue;
        }
        let buffers = draw_emitter(
            data,
            def,
            sim,
            &frame,
            &cam,
            emitter.anchor,
            None,
            level.as_deref(),
            None,
        );
        // Split screen's guest's copy, for his camera (make_twins).
        if let (Some(gcam), Some(twin)) = (guest_cam.as_ref(), twin)
            && let Ok((mut t_drawn, mut t_visibility)) = twins.get_mut(twin.0)
        {
            let copy = if view_dependent(def) {
                draw_emitter(
                    data,
                    def,
                    sim,
                    &frame,
                    gcam,
                    emitter.anchor,
                    None,
                    level.as_deref(),
                    None,
                )
            } else {
                buffers.clone()
            };
            show_sprites(
                &mut t_drawn,
                copy,
                (gcam, emitter.anchor),
                &mut t_visibility,
            );
        }
        if show_sprites(&mut drawn, buffers, (&cam, emitter.anchor), &mut visibility) {
            budget.wrote(distance);
        }
    }
}

/// An emitter's particles as quads (UE3's sprite vertex factory): each at its
/// place, sized and turned, facing the camera (or turned along its velocity, or
/// locked to an axis), showing its sub-image.
fn sprites(
    def: &EmitterDef,
    sim: &EmitterInstance,
    frame: &SimFrame,
    cam: &CameraView,
    anchor: Vec3,
) -> SpriteBuffers {
    let to_world = |p: Vec3| if def.local { frame.point(p) } else { p };
    // Draw order.
    let mut order: Vec<usize> = (0..sim.particles.len()).collect();
    let depth = |i: usize| (to_world(sim.particles[i].location) - cam.pos).dot(cam.forward);
    let dist = |i: usize| (to_world(sim.particles[i].location) - cam.pos).length();
    let age = |i: usize| sim.particles[i].relative_time;
    match def.sort {
        1 => order.sort_by(|&a, &b| depth(b).total_cmp(&depth(a))),
        2 => order.sort_by(|&a, &b| dist(b).total_cmp(&dist(a))),
        3 => order.sort_by(|&a, &b| age(b).total_cmp(&age(a))),
        4 => order.sort_by(|&a, &b| age(a).total_cmp(&age(b))),
        _ => {}
    }
    order.truncate(def.max_draw);
    let mut out = SpriteBuffers::with_capacity(order.len() * 4, order.len() * 6);
    let scale = frame.size;
    // (An emitter without a `required` line has no sub-image counts: one image.)
    let (sub_h_n, sub_v_n) = (def.sub_h.max(1), def.sub_v.max(1));
    let (sub_h, sub_v) = (sub_h_n as f32, sub_v_n as f32);
    let total = sub_h_n * sub_v_n;
    let blend_images = matches!(def.interp, 2 | 4);
    let uv_scale = if def.scale_uv || total > 1 {
        Vec2::new(1.0 / sub_h, 1.0 / sub_v)
    } else {
        Vec2::ONE
    };
    let corners_uv = [
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ];
    // The axes a locked sprite keeps to (UE3's GetAxisLockValues: its
    // AxisLocalToWorld): the emitter's frame's for a local-space emitter, the
    // world's for one in the world's space (a car's landing dust, locked to Z,
    // lies flat on the ground however the socket it comes from is turned).
    let lock_axis = |v: Vec3| {
        if def.local {
            frame.normal(v).normalize_or_zero()
        } else {
            v
        }
    };
    let lock_axes = |lock: u8| -> Option<(Vec3, Vec3)> {
        // (right, up) for a sprite locked to face along an axis.
        let v = match lock {
            1 => (Vec3::Y, Vec3::Z),
            2 => (-Vec3::X, Vec3::Z),
            3 => (Vec3::Y, Vec3::X),
            4 => (-Vec3::Y, Vec3::Z),
            5 => (Vec3::X, Vec3::Z),
            6 => (-Vec3::Y, Vec3::X),
            _ => return None,
        };
        Some((lock_axis(v.0), lock_axis(v.1)))
    };
    for i in order {
        let p = &sim.particles[i];
        let at = to_world(p.location);
        let mut size = Vec2::new(p.size.x, p.size.y) * scale;
        if def.align == 0 {
            size.y = size.x;
        }
        let to_cam = (cam.pos - at).normalize_or(-cam.forward);
        // The sprite's right and down (the image's U and V) in the world.
        let (mut right, mut down, turn) = if def.align == 2 {
            let along = if def.local {
                frame.normal(p.velocity)
            } else {
                p.velocity
            };
            // (A still one has no direction to lie along: the game's sprite shader
            // normalizes its motion, which leaves it no length, so it isn't drawn.
            // Turned upright here, the sharpshooters' and snipers' tracer,
            // Tracer_Sniper_AI_FX, whose lightning bolts take their velocity from
            // the effect's own, which stands still, rose 25 to 30 m straight up
            // from each of them as they fired, like a lightning strike.)
            let Some(along) = along.try_normalize() else {
                continue;
            };
            let r = to_cam.cross(along).normalize_or(cam.right);
            (r, -along, false)
        } else if let Some((r, u)) = lock_axes(def.lock) {
            (r, -u, true)
        } else if (7..=9).contains(&def.lock) {
            let axis = lock_axis([Vec3::X, Vec3::Y, Vec3::Z][(def.lock - 7) as usize])
                .normalize_or(Vec3::Z);
            let r = axis.cross(to_cam).normalize_or(cam.right);
            (r, -axis, false)
        } else if def.align == 4 {
            let r = Vec3::Z.cross(to_cam).normalize_or(cam.right);
            let u = to_cam.cross(r);
            (r, -u, true)
        } else {
            (cam.right, -cam.up, true)
        };
        if turn && p.rotation != 0.0 {
            let (s, c) = p.rotation.sin_cos();
            let (r0, d0) = (right, down);
            right = r0 * c + d0 * s;
            down = d0 * c - r0 * s;
        }
        let at = at + to_cam * def.camera_offset;
        let mut corners = [Vec3::ZERO; 4];
        for (k, uv) in corners_uv.iter().enumerate() {
            let w = at + right * (size.x * (uv.x - 0.5)) + down * (size.y * (uv.y - 0.5));
            corners[k] = to_bevy(w) - anchor;
        }
        // The sub-images: this one and (blending) the next.
        let image = p.image.max(0.0);
        let frame_uv = |index: u32| -> Vec2 {
            let index = index % total.max(1);
            Vec2::new(
                (index % sub_h_n) as f32 / sub_h,
                (index / sub_h_n) as f32 / sub_v,
            )
        };
        let first = image.floor() as u32;
        let a = frame_uv(first);
        let b = if blend_images { frame_uv(first + 1) } else { a };
        let lerp = if blend_images { image.fract() } else { 0.0 };
        let uv0 = corners_uv.map(|c| a + c * uv_scale);
        let uv1 = corners_uv.map(|c| b + c * uv_scale);
        out.quad(
            corners,
            dir_to_bevy(to_cam),
            dir_to_bevy(right),
            dir_to_bevy(down),
            uv0,
            uv1,
            p.color,
            Vec4::from_array(p.dyn_params),
            Vec4::new(lerp, 0.0, 0.0, 0.0),
        );
    }
    out
}

/// Bevy's tangent (w the bitangent's sign) for a surface facing `normal` whose
/// texture runs right along `right` and down along `down` (Bevy's bitangent runs
/// against V; see SpriteBuffers::quad).
fn tangent_for(normal: Vec3, right: Vec3, down: Vec3) -> Vec4 {
    let w = if normal.cross(right).dot(down) > 0.0 {
        -1.0
    } else {
        1.0
    };
    right.extend(w)
}

/// What an emitter draws this frame, by its type data: sprites, a trail, mesh
/// particles, decals or beams. `head`: where a trail runs on to (its emitter, while
/// it is on); `level`: what decals are laid on; `beam_end`: where the system's beams
/// end, when the game sets it (Unreal's axes).
#[allow(clippy::too_many_arguments)]
fn draw_emitter(
    data: &FxData,
    def: &EmitterDef,
    sim: &mut EmitterInstance,
    frame: &SimFrame,
    cam: &CameraView,
    anchor: Vec3,
    head: Option<Vec3>,
    level: Option<&crate::Level>,
    beam_end: Option<Vec3>,
) -> SpriteBuffers {
    // (Run for the others that follow it, not drawn.)
    if def.undrawn {
        return SpriteBuffers::default();
    }
    match &def.typedata {
        TypeData::Beam(beam) => beams(sim, frame, cam, anchor, beam, beam_end),
        TypeData::Sprite => sprites(def, sim, frame, cam, anchor),
        TypeData::Trail { tess, tile, .. } => {
            trail(def, sim, frame, cam, anchor, head, *tess, *tile)
        }
        TypeData::Mesh(m) => data
            .meshes
            .get(*m)
            .map(|mesh| mesh_particles(def, sim, frame, mesh, anchor))
            .unwrap_or_default(),
        TypeData::Decal(dir) => decals(def, sim, frame, anchor, dir, level),
    }
}

/// A trail emitter's ribbon (UE3's Trail2): from its newest particle (or, while it
/// is on, from the emitter, `head`) back to its oldest, on a curve through them
/// with `tess` points to each stretch, facing the camera and as wide as twice each
/// particle's size (x). The texture runs along it (U, 0 at the head, `tile` times
/// over its length) and across (V).
#[allow(clippy::too_many_arguments)]
fn trail(
    def: &EmitterDef,
    sim: &EmitterInstance,
    frame: &SimFrame,
    cam: &CameraView,
    anchor: Vec3,
    head: Option<Vec3>,
    tess: u32,
    tile: f32,
) -> SpriteBuffers {
    let mut out = SpriteBuffers::default();
    let to_world = |p: Vec3| if def.local { frame.point(p) } else { p };
    let scale = frame.size;
    // (place, half width, colour, dynamic parameters), newest first.
    type Node = (Vec3, f32, Vec4, Vec4);
    let mut nodes: Vec<Node> = sim
        .particles
        .iter()
        .rev()
        .map(|p| {
            (
                to_world(p.location),
                p.size.x * scale,
                p.color,
                Vec4::from_array(p.dyn_params),
            )
        })
        .collect();
    if let (Some(h), Some(&first)) = (head, nodes.first())
        && h.distance(first.0) > 0.5
    {
        nodes.insert(0, (h, first.1, first.2, first.3));
    }
    nodes.dedup_by(|b, a| a.0.distance_squared(b.0) < 0.25);
    let n = nodes.len();
    if n < 2 {
        return out;
    }
    // The curve (Catmull-Rom through the nodes).
    let tess = tess.max(1) as usize;
    let mut points: Vec<Node> = Vec::with_capacity((n - 1) * tess + 1);
    let slope = |i: usize| (nodes[(i + 1).min(n - 1)].0 - nodes[i.saturating_sub(1)].0) * 0.5;
    for i in 0..n - 1 {
        let (a, b) = (nodes[i], nodes[i + 1]);
        let (m0, m1) = (slope(i), slope(i + 1));
        for k in 0..tess {
            let t = k as f32 / tess as f32;
            let (t2, t3) = (t * t, t * t * t);
            let pos = a.0 * (2.0 * t3 - 3.0 * t2 + 1.0)
                + m0 * (t3 - 2.0 * t2 + t)
                + b.0 * (3.0 * t2 - 2.0 * t3)
                + m1 * (t3 - t2);
            points.push((
                pos,
                a.1 + (b.1 - a.1) * t,
                a.2.lerp(b.2, t),
                a.3.lerp(b.3, t),
            ));
        }
    }
    points.push(nodes[n - 1]);
    let count = points.len();
    let mut last: Option<(u32, u32)> = None;
    for k in 0..count {
        let (at, half, color, param) = points[k];
        let along = (points[(k + 1).min(count - 1)].0 - points[k.saturating_sub(1)].0)
            .normalize_or(cam.forward);
        let to_cam = (cam.pos - at).normalize_or(-cam.forward);
        let up = along.cross(to_cam).normalize_or(cam.up);
        let u = k as f32 / (count - 1) as f32 * tile;
        let normal = dir_to_bevy(to_cam);
        let tangent = tangent_for(normal, dir_to_bevy(along), dir_to_bevy(-up));
        let mut corner = |side: f32, v: f32| {
            let uv = Vec2::new(u, v);
            out.vertex(
                to_bevy(at + up * (half * side)) - anchor,
                normal,
                tangent,
                uv,
                uv,
                color,
                param,
                Vec4::ZERO,
            )
        };
        let (top, bottom) = (corner(1.0, 0.0), corner(-1.0, 1.0));
        if let Some((a, b)) = last {
            out.idx.extend_from_slice(&[a, b, top, top, b, bottom]);
        }
        last = Some((top, bottom));
    }
    out
}

/// How many stretches a tapering beam is drawn in (its width changes along it).
const BEAM_STEPS: usize = 16;

/// How many stretches a bent beam (its waves) is drawn in.
const BEAM_WAVE_STEPS: usize = 48;

/// A beam emitter's beams (UE3's Beam2, see [`Beam`]): each from its source (the
/// emitter's frame) to `end` (or its target in the emitter's frame), a ribbon
/// facing the camera, its U running along it from the source (`tile` times over)
/// and V across.
fn beams(
    sim: &EmitterInstance,
    frame: &SimFrame,
    cam: &CameraView,
    anchor: Vec3,
    beam: &Beam,
    end: Option<Vec3>,
) -> SpriteBuffers {
    let mut out = SpriteBuffers::default();
    let from = frame.point(beam.source);
    let to = end.unwrap_or_else(|| frame.point(beam.target));
    let along = to - from;
    if along.length_squared() < 1e-4 {
        return out;
    }
    let dir = along.normalize();
    let steps = if !beam.waves.is_empty() {
        BEAM_WAVE_STEPS
    } else if beam.taper && (beam.factor.table.len() > 1 || beam.scale.table.len() > 1) {
        BEAM_STEPS
    } else {
        1
    };
    // The beam's frame for its waves: z along it, x and y across it.
    let across_x = dir.cross(Vec3::Z).try_normalize().unwrap_or(Vec3::X);
    let across_y = dir.cross(across_x);
    let length = along.length();
    let scale = frame.size;
    for p in &sim.particles {
        let param = Vec4::from_array(p.dyn_params);
        let mut last: Option<(u32, u32)> = None;
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let mut at = from + along * t;
            if !beam.waves.is_empty() {
                // (Held to its ends - the source and target stay put - easing in
                // over the first and last seventh or so: an assumption, as the
                // module's code isn't known.)
                let hold = (8.0 * t * (1.0 - t)).min(1.0);
                let d = t * length;
                for w in &beam.waves {
                    let turns = d / w.period - sim.seconds * w.speed / w.period + w.phase;
                    let bend = w.amplitude * (turns * std::f32::consts::TAU).sin() * hold;
                    at +=
                        (across_x * w.direction.x + across_y * w.direction.y + dir * w.direction.z)
                            * bend;
                }
            }
            let taper = if beam.taper {
                beam.factor.eval(t, [0.5; 3])[0] * beam.scale.eval(t, [0.5; 3])[0]
            } else {
                1.0
            };
            let half = p.size.x * scale * taper;
            let to_cam = (cam.pos - at).normalize_or(-cam.forward);
            let up = dir.cross(to_cam).normalize_or(cam.up);
            let normal = dir_to_bevy(to_cam);
            let tangent = tangent_for(normal, dir_to_bevy(dir), dir_to_bevy(-up));
            let u = t * beam.tile;
            let mut corner = |side: f32, v: f32| {
                let uv = Vec2::new(u, v);
                out.vertex(
                    to_bevy(at + up * (half * side)) - anchor,
                    normal,
                    tangent,
                    uv,
                    uv,
                    p.color,
                    param,
                    Vec4::ZERO,
                )
            };
            let (top, bottom) = (corner(1.0, 0.0), corner(-1.0, 1.0));
            if let Some((a, b)) = last {
                out.idx.extend_from_slice(&[a, b, top, top, b, bottom]);
            }
            last = Some((top, bottom));
        }
    }
    out
}

/// UE3's FRotationMatrix for (pitch, yaw, roll) in radians: rows are the turned X,
/// Y and Z.
fn rotation_axes(pitch: f32, yaw: f32, roll: f32) -> [Vec3; 3] {
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    let (sr, cr) = roll.sin_cos();
    [
        Vec3::new(cp * cy, cp * sy, sp),
        Vec3::new(sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, -sr * cp),
        Vec3::new(-(cr * sp * cy + sr * sy), cy * sr - cr * sp * sy, cr * cp),
    ]
}

/// A mesh emitter's particles: its mesh for each, scaled by the particle's size,
/// turned by its MeshRotation (in the world, or the emitter's frame for a local
/// emitter) and set at its place, in its colour.
fn mesh_particles(
    def: &EmitterDef,
    sim: &EmitterInstance,
    frame: &SimFrame,
    mesh: &FxMesh,
    anchor: Vec3,
) -> SpriteBuffers {
    let drawn = sim.particles.len().min(def.max_draw);
    let mut out = SpriteBuffers::with_capacity(drawn * mesh.pos.len(), drawn * mesh.idx.len());
    let scale = frame.size;
    for p in sim.particles.iter().take(def.max_draw) {
        let r = p.mesh_rotation;
        let axes = rotation_axes(r.y, r.z, r.x);
        let size = p.size * if def.local { 1.0 } else { scale };
        let turn = |v: Vec3| {
            let w = axes[0] * v.x + axes[1] * v.y + axes[2] * v.z;
            if def.local { frame.normal(w) } else { w }
        };
        let at = if def.local {
            frame.point(p.location)
        } else {
            p.location
        };
        let inv = Vec3::new(
            1.0 / size.x.max(1e-4),
            1.0 / size.y.max(1e-4),
            1.0 / size.z.max(1e-4),
        );
        let base = out.vertex_count() as u32;
        let param = Vec4::from_array(p.dyn_params);
        for k in 0..mesh.pos.len() {
            let pos = at + turn(mesh.pos[k] * size);
            let n = turn(mesh.nrm[k] * inv).normalize_or(Vec3::Z);
            let t = mesh.tan[k];
            let tw = turn(t.truncate() * size).normalize_or(Vec3::X);
            // (Unreal's axes to Bevy's mirror the frame: the bitangent's sign turns.)
            out.vertex(
                to_bevy(pos) - anchor,
                dir_to_bevy(n),
                dir_to_bevy(tw).extend(-t.w),
                mesh.uv[k],
                mesh.uv[k],
                p.color,
                param,
                Vec4::ZERO,
            );
        }
        out.idx.extend(mesh.idx.iter().map(|i| base + i));
    }
    out
}

/// How far off the surface a decal lies (Unreal's units).
const DECAL_LIFT: f32 = 2.0;
/// How far in front of a decal particle its trace for a surface starts (Unreal's
/// units): a blast's particles start on the struck surface itself, which a trace
/// from there would miss, finding what lies behind it instead.
const DECAL_BACK: f32 = 10.0;
/// How far a mark reaches into and out of the surface it is laid on (Unreal's
/// units): onto the surfaces round it that sink into it or stand out of it by no
/// more than this.
const DECAL_DEPTH: f32 = 50.0;
/// The least a surface can face a decal and take its mark (the cosine of the
/// angle between them: about 70 degrees). The floor at the foot of a struck wall,
/// say, is left bare.
const DECAL_FACING: f32 = 0.35;
/// The most pieces of the level's surfaces a mark is laid on.
const DECAL_MAX_TRIS: usize = 4096;

/// A decal emitter's particles (High Moon's decals): each lays its mark once, on
/// the first surface a trace along the emitter's projection direction finds from
/// just in front of it, within its size's reach (z). The mark is as wide and long
/// as its size's x and y, turned by its rotation, and cut to the level's surfaces
/// round there that face it (`lay_decal`), so it wraps over them rather than
/// hanging past their edges in the air; with nothing in reach there is no mark. Its
/// vertex colour is white (the decal's, not the particle's).
fn decals(
    def: &EmitterDef,
    sim: &mut EmitterInstance,
    frame: &SimFrame,
    anchor: Vec3,
    dir: &Dist,
    level: Option<&crate::Level>,
) -> SpriteBuffers {
    let mut out = SpriteBuffers::default();
    let scale = frame.size;
    let along = frame
        .normal(Vec3::from(dir.eval(0.0, [0.5; 3])))
        .normalize_or(-Vec3::Z);
    for p in sim.particles.iter_mut().take(def.max_draw) {
        if p.decal.is_none() {
            let at = if def.local {
                frame.point(p.location)
            } else {
                p.location
            };
            let mark = level.and_then(|l| lay_decal(l, frame, p, at, along, scale));
            p.decal = Some(mark.map(|m| {
                let n = sim.next_mark;
                sim.next_mark = n.wrapping_add(1);
                sim.marks.push((n, m));
                n
            }));
        }
        let Some(Some(n)) = p.decal else {
            continue;
        };
        let Some((_, mark)) = sim.marks.iter().find(|(k, _)| *k == n) else {
            continue;
        };
        let base = out.vertex_count() as u32;
        let corners = mark.pos.iter().zip(&mark.nrm).zip(&mark.tan).zip(&mark.uv);
        for (((&at, &normal), &tangent), &uv) in corners {
            out.vertex(
                at - anchor,
                normal,
                tangent,
                uv,
                uv,
                Vec4::ONE,
                Vec4::from_array(p.dyn_params),
                Vec4::ZERO,
            );
        }
        out.idx.extend(mark.idx.iter().map(|i| base + i));
    }
    // (A particle's mark goes with it.)
    let particles = &sim.particles;
    sim.marks
        .retain(|(n, _)| particles.iter().any(|p| p.decal == Some(Some(*n))));
    out
}

/// Lays decal particle `p` (at `at`, Unreal's axes, its emitter projecting along
/// `along`): see `decals`. None if there is nothing in its reach to lay it on.
fn lay_decal(
    level: &crate::Level,
    frame: &SimFrame,
    p: &Particle,
    at: Vec3,
    along: Vec3,
    scale: f32,
) -> Option<DecalMark> {
    let reach = (p.size.z * scale).max(100.0) + DECAL_BACK;
    let start = at - along * DECAL_BACK;
    let (t, hit_normal) = level.ray_hit(to_bevy(start), dir_to_bevy(along), reach * 0.01)?;
    let normal = dir_to_ue(hit_normal).normalize_or(-along);
    // Its sides: the emitter's Y laid on the surface, and across it.
    let mut right = frame.axes[1] - normal * frame.axes[1].dot(normal);
    if right.length_squared() < 1e-6 {
        right = frame.axes[2] - normal * frame.axes[2].dot(normal);
    }
    let mut right = right.normalize_or(Vec3::X);
    let mut down = normal.cross(right);
    if p.rotation != 0.0 {
        let (s, c) = p.rotation.sin_cos();
        let (r0, d0) = (right, down);
        right = r0 * c + d0 * s;
        down = d0 * c - r0 * s;
    }
    // From here in Bevy's axes and metres: the mark's middle, the way the surface
    // faces, its right and down, and its half width and length.
    let centre = to_bevy(start) + dir_to_bevy(along) * t;
    let (n, r, d) = (dir_to_bevy(normal), dir_to_bevy(right), dir_to_bevy(down));
    let hx = (p.size.x * 0.5 * scale * 0.01).max(1e-3);
    let hy = (p.size.y * 0.5 * scale * 0.01).max(1e-3);
    let depth = DECAL_DEPTH * 0.01;
    let lift = DECAL_LIFT * 0.01;
    let uv = |v: Vec3| {
        Vec2::new(
            ((v - centre).dot(r) / hx + 1.0) * 0.5,
            ((v - centre).dot(d) / hy + 1.0) * 0.5,
        )
    };
    let mut mark = DecalMark::default();
    let reach_box = r.abs() * hx + d.abs() * hy + n.abs() * depth;
    let Some(surfaces) = level.surfaces_in(centre - reach_box, centre + reach_box) else {
        // (The arena: its floor is a plane, which the mark lies flat on.)
        let tangent = tangent_for(n, r, d);
        for (u, v) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
            mark.pos
                .push(centre + r * (hx * (u * 2.0 - 1.0)) + d * (hy * (v * 2.0 - 1.0)) + n * lift);
            mark.nrm.push(n);
            mark.tan.push(tangent);
            mark.uv.push(Vec2::new(u, v));
        }
        mark.idx.extend_from_slice(&[0, 2, 1, 0, 3, 2]);
        return Some(mark);
    };
    // The box it is cut to: its sides, and how far into and out of the surface.
    let planes = [
        (r, hx),
        (-r, hx),
        (d, hy),
        (-d, hy),
        (n, depth),
        (-n, depth),
    ];
    for [a, b, c] in surfaces {
        let face = (b - a).cross(c - a);
        let Some(mut face) = face.try_normalize() else {
            continue;
        };
        let facing = face.dot(n);
        if facing.abs() < DECAL_FACING {
            continue;
        }
        // (Wound round the way the mark faces, as a quad facing it is.)
        let mut piece = if facing > 0.0 {
            vec![a, b, c]
        } else {
            face = -face;
            vec![a, c, b]
        };
        for &(plane, offset) in &planes {
            piece = clip_polygon(&piece, centre, plane, offset);
            if piece.len() < 3 {
                break;
            }
        }
        if piece.len() < 3 {
            continue;
        }
        let tangent = tangent_for(
            face,
            (r - face * r.dot(face)).normalize_or(r),
            d - face * d.dot(face),
        );
        let base = mark.pos.len() as u32;
        for &v in &piece {
            mark.pos.push(v + face * lift);
            mark.nrm.push(face);
            mark.tan.push(tangent);
            mark.uv.push(uv(v));
        }
        for k in 1..piece.len() as u32 - 1 {
            mark.idx.extend_from_slice(&[base, base + k, base + k + 1]);
        }
        if mark.idx.len() / 3 >= DECAL_MAX_TRIS {
            break;
        }
    }
    (!mark.idx.is_empty()).then_some(mark)
}

/// The part of convex polygon `poly` on the inner side of the plane facing
/// `normal` at `offset` from `origin` (Sutherland-Hodgman).
fn clip_polygon(poly: &[Vec3], origin: Vec3, normal: Vec3, offset: f32) -> Vec<Vec3> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    for (i, &p) in poly.iter().enumerate() {
        let q = poly[(i + 1) % poly.len()];
        let dp = (p - origin).dot(normal) - offset;
        let dq = (q - origin).dot(normal) - offset;
        if dp <= 0.0 {
            out.push(p);
        }
        if (dp < 0.0 && dq > 0.0) || (dp > 0.0 && dq < 0.0) {
            out.push(p + (q - p) * (dp / (dp - dq)));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Effects spawned as the game is played
// ---------------------------------------------------------------------------

/// Where the spawned effects are run each frame (PostUpdate, after the transforms
/// are propagated): what moves them to where their carriers' sockets are this
/// frame comes before it (src/ai's held effects).
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpawnedFxRun;

/// An effect spawned with FxSystems::spawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FxId(u32);

/// A spawned effect's module channel (ParticleSystemComponent.SwitchModuleChannel),
/// given among its parameters as (MODULE_CHANNEL, MODULE_CHANNEL_A or _B): the
/// TnParticleSystemParameterTeamIndex of the projectile or weapon whose effect it is
/// switches it to A (1) for team 0, the Autobots, and to B (2) for team 1, the
/// Decepticons. An effect never switched is A. It picks the curves of its emitters'
/// SwitchableColorScaleOverLife modules.
pub const MODULE_CHANNEL: &str = "@channel";
pub const MODULE_CHANNEL_A: [u8; 4] = [1, 0, 0, 0];
pub const MODULE_CHANNEL_B: [u8; 4] = [2, 0, 0, 0];

/// A spawned effect's state.
struct Live {
    system: usize,
    /// Its place and axes (Unreal's axes and units), where it was last frame and
    /// how fast it moves.
    origin: Vec3,
    prev_origin: Vec3,
    axes: [Vec3; 3],
    /// Its size (the component's scale: SetScale), which its axes carry, and how
    /// much longer each of them is besides (spawn_sized's own scale along each;
    /// 1 each way otherwise): SimFrame::size.
    size: f32,
    stretch: Vec3,
    velocity: Vec3,
    params: Vec<(String, [u8; 4])>,
    /// Its vector parameters (set_vector).
    vectors: Vec<(String, Vec3)>,
    /// Spawning (DeactivateSystem turns it off: what's out lives on).
    on: bool,
    /// Run it from scratch along this path this frame, `step` seconds a place
    /// (the grenade's arc), and a colour parameter set anew at each place, if given
    /// (the Gear Shredder's path's: TnProjectileSawblade.OnSimulationStepped).
    replay: Option<(Vec<(Vec3, [Vec3; 3])>, f32)>,
    replay_colors: Option<(String, Vec<[u8; 4]>)>,
    /// Its emitters' entities exist, and how many are left.
    started: bool,
    emitters: usize,
    seed: u32,
    /// Where its beams end (Unreal's axes), when the game sets it: a tracer's,
    /// SetTracerDistance; and where each emitter's do, where the game sets them
    /// one by one (SetBeamTargetPoint's EmitterIndex: a laser sight's beams).
    beam_end: Option<Vec3>,
    beam_ends: Vec<Option<Vec3>>,
    /// A tracer's line (SetTracerDistance): where it starts and the way it goes
    /// (Unreal's axes) and how far it reaches (Unreal's units), to where the round
    /// struck. Its particles go as they get there: none flies on past it.
    tracer: Option<(Vec3, Vec3, f32)>,
    /// Drawn in the Customize garage (its render layer) rather than the world.
    garage: bool,
    /// Lit by the level's lights where it starts (light_at), as a map's emitter the
    /// game lights (its particle component accepts lights and has a light
    /// environment: set_lit); what the game spawns takes no light
    /// (spawned_material).
    lit: bool,
    /// Lit with no sun: where the sun doesn't reach it (set_lit_shaded).
    sunless: bool,
    /// On one player's screen (see_only): only his view shows it.
    seen_by: Option<usize>,
}

/// The weapons' and abilities' effects (fx/weapons_fx.txt), spawned where the game
/// needs them: an explosion where a grenade goes off, a grenade's flight following
/// it, a buff's effect on a sentry. Each runs as the game's would (UE3's particle
/// system component): one-shot emitters finish by themselves, looping ones until
/// the effect is stopped; an effect is gone once nothing of it is left.
#[derive(Resource, Default)]
pub struct FxSystems {
    data: Option<Arc<FxData>>,
    tried: bool,
    /// The white texture the materials of meshes drawn outside the effects take
    /// where they have fewer textures (mesh_material).
    white: Option<Handle<Image>>,
    next: u32,
    live: HashMap<u32, Live>,
    /// Where the camera was last frame (Unreal's axes and units): an effect is
    /// spawned at the level of detail its distance from it calls for (from the
    /// nearer, split screen's guest's camera too: `guest_camera`).
    camera: Option<Vec3>,
    guest_camera: Option<Vec3>,
    /// Levels of detail an effect drops by (the Graphics level's ParticleLODBias:
    /// FxQuality), to none kept for further off than effects are drawn (cm; None:
    /// however far: lod_level).
    lod_bias: usize,
    lod_drawn: Option<f32>,
}

/// The level of detail an effect `far` (cm) from the camera starts at (0: its own,
/// n: its lods[n - 1]), by its LODDistances, `bias` levels less detailed (the
/// Graphics level's ParticleLODBias), though never one it keeps for further off
/// than effects are drawn at all on that level (`drawn`, cm): a level the game
/// only shows from 500 m away (Ignition's flame jets' a fifth of the flames, which
/// sputtered on and off up close) is never brought up near.
fn lod_level(lods: &[(f32, usize)], far: f32, bias: usize, drawn: Option<f32>) -> usize {
    let at = lods
        .iter()
        .rposition(|(d, _)| far >= *d)
        .map_or(0, |i| i + 1);
    let mut level = (at + bias).min(lods.len());
    while level > at && drawn.is_some_and(|drawn| lods[level - 1].0 > drawn) {
        level -= 1;
    }
    level
}

/// The Graphics level's effects (graphics.rs): the levels of detail they drop by
/// (UE3's ParticleLODBias), how far from the camera they are drawn (cm) and how
/// many are written every frame in a busy one (FxBudget).
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct FxQuality {
    pub lod_bias: usize,
    pub far: f32,
    pub meshes: usize,
}

impl Default for FxQuality {
    fn default() -> Self {
        FxQuality {
            lod_bias: 0,
            far: f32::MAX,
            meshes: usize::MAX,
        }
    }
}

/// The effects drawn anew a frame in a busy one. Drawing an effect anew (its
/// particles sorted and turned to the camera, its vertices made, then copied into
/// the frame's effect data and sent to the graphics card: FxWords) takes its time,
/// and a big fight's effects in view are a few hundred. With more effects in view
/// than the Graphics level's number (FxQuality::meshes), those further off than the
/// nearest that many are drawn anew every other frame, half of them one frame and
/// half the next, and drawn as they were in between (they go on running every
/// frame).
#[derive(Resource, Default)]
pub struct FxBudget {
    /// The distances (cm) of the effects in view this frame.
    wanted: Vec<f32>,
    /// Past this distance (cm) an effect in view is written every other frame.
    far: f32,
    /// Which half of those is written this frame.
    odd: bool,
}

impl FxBudget {
    /// Whether an effect at `distance` (cm), shown, is left as it was this frame
    /// (counted as in view); `key` picks its half. One `hidden` (or a decal: drawn
    /// only as it changes) is written as it comes.
    fn defer(&mut self, distance: f32, hidden: bool, key: u32) -> bool {
        let defer = !hidden && distance > self.far && (key & 1 == 1) == self.odd;
        if defer {
            self.wanted.push(distance);
        }
        defer
    }

    /// An effect at `distance` (cm) written this frame.
    fn wrote(&mut self, distance: f32) {
        self.wanted.push(distance);
    }
}

/// Starts the frame's effects (FxBudget): the distance past which they are written
/// every other frame, from those in view last frame.
fn roll_fx_budget(mut budget: ResMut<FxBudget>, fxq: Res<FxQuality>) {
    let b = &mut *budget;
    b.odd = !b.odd;
    b.far = if b.wanted.len() > fxq.meshes {
        *b.wanted
            .select_nth_unstable_by(fxq.meshes, f32::total_cmp)
            .1
    } else {
        f32::INFINITY
    };
    b.wanted.clear();
}

/// The effects as the Graphics level has them.
pub fn set_fx_quality(
    quality: Res<crate::graphics::GraphicsQuality>,
    mut fxq: ResMut<FxQuality>,
    mut systems: ResMut<FxSystems>,
) {
    if !quality.is_changed() {
        return;
    }
    let level = quality.level();
    *fxq = FxQuality {
        lod_bias: level.particle_lod_bias,
        far: level.effect_distance.map_or(f32::MAX, |m| m * 100.0),
        meshes: level.effect_meshes.unwrap_or(usize::MAX),
    };
    systems.lod_bias = fxq.lod_bias;
    systems.lod_drawn = Some(fxq.far);
}

/// An effect's axes from the way its X points (Bevy's axes): Unreal's
/// FRotationMatrix of that direction's rotator (no roll), in Unreal's axes, `scale`
/// long.
fn axes_along(x: Vec3, scale: Vec3) -> [Vec3; 3] {
    let x = dir_to_ue(x).normalize_or(Vec3::X);
    let flat = Vec2::new(-x.y, x.x);
    let y = if flat.length_squared() > 1e-8 {
        flat.normalize().extend(0.0)
    } else {
        Vec3::Y
    };
    [x * scale.x, y * scale.y, x.cross(y) * scale.z]
}

impl FxSystems {
    fn load(&mut self) -> Option<&Arc<FxData>> {
        if !self.tried {
            self.tried = true;
            let path = crate::assets_dir().join(WEAPON_FX);
            match std::fs::read_to_string(&path) {
                Ok(text) => self.data = Some(Arc::new(FxData::parse(&text))),
                Err(e) => warn!(
                    "levelfx: {} can't be read ({e}): no weapon effects",
                    path.display()
                ),
            }
        }
        self.data.as_ref()
    }

    /// Effect material `name` (the game's name for it) as a still effect mesh draws
    /// it (unlit), for a mesh the game builds itself: the Guard's scanner beam
    /// (src/ai/scanner.rs). Its mesh brings the attributes an effect mesh has:
    /// places, normals, tangents, both UV sets and colours. None if the effects
    /// haven't got it.
    pub fn mesh_material(
        &mut self,
        name: &str,
        materials: &mut Assets<FxMaterial>,
        images: &mut Assets<Image>,
        asset_server: &AssetServer,
    ) -> Option<Handle<FxMaterial>> {
        let data = self.load()?.clone();
        let index = data.materials.iter().position(|m| m.name == name)?;
        let white = self
            .white
            .get_or_insert_with(|| white_image(images))
            .clone();
        let m = make_material(
            &data.materials[index],
            WEAPON_SET as usize,
            index,
            Kind::Mesh,
            false,
            FxParams::default(),
            asset_server,
            &white,
        );
        Some(materials.add(m))
    }

    /// The box round effect `name`'s mesh (its first mesh emitter's: a mesh system,
    /// build_weapon_fx.py's MESH_SYSTEMS), in its own frame as place_turned turns it
    /// (its X along the X axis given, its Y along the Y axis given, its Z the third),
    /// in the demo's axes and metres: its least and greatest corners. None for an
    /// effect without one.
    pub fn mesh_bounds(&mut self, name: &str) -> Option<(Vec3, Vec3)> {
        let data = self.load()?;
        let system = data.systems.iter().find(|s| s.name == name)?;
        let mesh = system.emitters.iter().find_map(|em| match em.typedata {
            TypeData::Mesh(m) => data.meshes.get(m),
            _ => None,
        })?;
        // (Unreal's axes in the effect's frame: its X, Y and Z are the demo's X,
        // Z and Y there.)
        let local = |v: &Vec3| Vec3::new(v.x, v.z, v.y) / crate::deaths::UU;
        let first = local(mesh.pos.first()?);
        Some(
            mesh.pos
                .iter()
                .map(local)
                .fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))),
        )
    }

    /// Starts effect `name` at `at` with its X along `x_axis` (Bevy's axes), its
    /// colour parameters (EnergonColor and the like) set to `params`. None if the
    /// demo hasn't got it.
    pub fn spawn(
        &mut self,
        name: &str,
        at: Vec3,
        x_axis: Vec3,
        params: &[(&str, [u8; 4])],
    ) -> Option<FxId> {
        self.spawn_scaled(name, at, x_axis, 1.0, params)
    }

    /// The same, `scale` times its size (ParticleSystemComponent.SetScale: its
    /// particles' places, sizes and speeds).
    pub fn spawn_scaled(
        &mut self,
        name: &str,
        at: Vec3,
        x_axis: Vec3,
        scale: f32,
        params: &[(&str, [u8; 4])],
    ) -> Option<FxId> {
        self.spawn_with(name, at, x_axis, (scale, Vec3::ONE), params)
    }

    /// The same, sized along its own axes (ParticleSystemComponent.SetScale3D:
    /// `scale`'s x along its X, y across, z up): a local-space emitter's frame is
    /// that long each way, the others' particles the longest of them times their
    /// size.
    pub fn spawn_sized(&mut self, name: &str, at: Vec3, x_axis: Vec3, scale: Vec3) -> Option<FxId> {
        let size = scale.max_element().max(1e-6);
        self.spawn_with(name, at, x_axis, (size, scale / size), &[])
    }

    /// Starts effect `name` on a socket, at `at` with its X along `x_axis` (see
    /// place_turned), as the game's weapons attach their effects: at the socket's
    /// place and turn but with none of its RelativeScale, its size or its signs
    /// (HmParticleEffectManagerComponent.PlayEffectOnSocket attaches with
    /// AttachComponentToSocketWithOffsets and the effect's own RelativeScale, 1 for
    /// every gun: TnWeaponEffectPlayFXOnSocket.CalculateRelativeScale). The X18
    /// Scrapmaker's barrels are at a tenth, and its flash is the size of its barrels'
    /// housing in the game's videos, not a tenth of it. A mirroring scale doesn't
    /// turn it round either: the A-4 Pulsar Cannon's glow sits 50 units along its
    /// effect's -X from its IdleFXSocket, on the front of the gun on either arm
    /// only so (the right gun's socket's scale is -0.7, -0.7, 0.7, the left's
    /// turned round by its rotation instead); turned round by that scale, the
    /// right gun's sat back by the elbow. The X18 Scrapmaker's idle smoke's sockets
    /// mirror each other only so too.
    pub fn spawn_on_socket(
        &mut self,
        name: &str,
        at: Vec3,
        x_axis: Vec3,
        params: &[(&str, [u8; 4])],
    ) -> Option<FxId> {
        self.spawn_with(name, at, x_axis, (1.0, Vec3::ONE), params)
    }

    fn spawn_with(
        &mut self,
        name: &str,
        at: Vec3,
        x_axis: Vec3,
        (scale, stretch): (f32, Vec3),
        params: &[(&str, [u8; 4])],
    ) -> Option<FxId> {
        let (camera, lod_bias, lod_drawn) = (self.camera, self.lod_bias, self.lod_drawn);
        let guest_camera = self.guest_camera;
        let data = self.load()?;
        let mut system = data.systems.iter().position(|s| s.name == name)?;
        let origin = to_ue(at);
        // Its level of detail, by how far it is from the camera as it starts (as a
        // particle system component picks it as it is activated, by LODDistances).
        // (The Graphics level's ParticleLODBias: that many levels less detailed,
        // as far as it has them, to those for effects as far as are drawn.)
        if let Some(cam) = camera {
            let lods = &data.systems[system].lods;
            let far = guest_camera.map_or(origin.distance(cam), |g| {
                origin.distance(cam).min(origin.distance(g))
            });
            let level = lod_level(lods, far, lod_bias, lod_drawn);
            if level > 0 {
                system = lods[level - 1].1;
            }
        }
        self.next = self.next.wrapping_add(1);
        let id = self.next;
        self.live.insert(
            id,
            Live {
                system,
                origin,
                prev_origin: origin,
                axes: axes_along(x_axis, stretch * scale),
                size: scale,
                stretch,
                velocity: Vec3::ZERO,
                params: params.iter().map(|(n, c)| (n.to_string(), *c)).collect(),
                vectors: Vec::new(),
                on: true,
                replay: None,
                replay_colors: None,
                started: false,
                emitters: 0,
                seed: id.wrapping_mul(0x9e37_79b9),
                beam_end: None,
                beam_ends: Vec::new(),
                tracer: None,
                garage: false,
                lit: false,
                sunless: false,
                seen_by: None,
            },
        );
        Some(FxId(id))
    }

    /// Lights effect `id` by the level's lights where it starts (before it starts):
    /// one of the maps' emitters the game lights (a trap's flames: their particle
    /// components accept lights and have a light environment, doors.rs's
    /// TrapThing::Emitter).
    pub fn set_lit(&mut self, id: FxId) {
        if let Some(l) = self.live.get_mut(&id.0) {
            l.lit = true;
        }
    }

    /// An effect on player `seat`'s screen (split.rs: a screen effect in front of
    /// his camera): only his view shows it, drawn for his camera. (Before its
    /// emitters are made: the frame it is spawned.)
    pub fn see_only(&mut self, id: FxId, seat: usize) {
        if let Some(l) = self.live.get_mut(&id.0) {
            l.seen_by = Some(seat);
        }
    }

    /// The same where the sun doesn't reach it: lit by the level's lights alone (a
    /// light environment out of the sun: movefx.rs's effects on a robot standing in
    /// the shade).
    pub fn set_lit_shaded(&mut self, id: FxId) {
        if let Some(l) = self.live.get_mut(&id.0) {
            l.lit = true;
            l.sunless = true;
        }
    }

    /// Sets effect `id`'s vector parameter `name` (ParticleSystemComponent.
    /// SetVectorParameter: what its modules that take it read, in the game's units).
    pub fn set_vector(&mut self, id: FxId, name: &str, value: Vec3) {
        if let Some(l) = self.live.get_mut(&id.0) {
            match l.vectors.iter_mut().find(|(n, _)| n == name) {
                Some((_, v)) => *v = value,
                None => l.vectors.push((name.to_string(), value)),
            }
        }
    }

    /// Sets effect `id`'s colour parameter `name` (ParticleSystemComponent.
    /// SetColorParameter: what its ColorByParameter modules give the particles it
    /// spawns from now on).
    pub fn set_color(&mut self, id: FxId, name: &str, rgba: [u8; 4]) {
        if let Some(l) = self.live.get_mut(&id.0) {
            match l.params.iter_mut().find(|(n, _)| n == name) {
                Some((_, c)) => *c = rgba,
                None => l.params.push((name.to_string(), rgba)),
            }
        }
    }

    /// Sets effect `id`'s scale (ParticleSystemComponent.SetScale, or its actor's
    /// DrawScale changing): `scale` times its size from now on, its frame's axes
    /// as long (a mesh it draws in its own frame grows with them: the Diffraction
    /// Barrier's wall as it is put up).
    pub fn set_scale(&mut self, id: FxId, scale: f32) {
        if let Some(l) = self.live.get_mut(&id.0) {
            let k = scale / l.size.max(1e-6);
            for a in &mut l.axes {
                *a *= k;
            }
            l.size = scale;
        }
    }

    /// Moves effect `id` (one following something): to `at`, X along `x_axis`,
    /// moving at `velocity` (Bevy's axes, metres a second).
    pub fn place(&mut self, id: FxId, at: Vec3, x_axis: Vec3, velocity: Vec3) {
        if let Some(l) = self.live.get_mut(&id.0) {
            l.origin = to_ue(at);
            l.axes = axes_along(x_axis, l.stretch * l.size);
            l.velocity = to_ue(velocity);
        }
    }

    /// Moves effect `id` (one carried on a socket) to `at`, turned as the socket is:
    /// its X along `x_axis` and its Y along `y_axis` (Bevy's axes), its Z theirs in
    /// Unreal's axes (X cross Y there).
    pub fn place_turned(&mut self, id: FxId, at: Vec3, x_axis: Vec3, y_axis: Vec3) {
        if let Some(l) = self.live.get_mut(&id.0) {
            let x = dir_to_ue(x_axis).normalize_or(Vec3::X);
            let y = dir_to_ue(y_axis).reject_from_normalized(x);
            let y = y
                .try_normalize()
                .unwrap_or_else(|| x.any_orthonormal_vector());
            let s = l.stretch * l.size;
            l.origin = to_ue(at);
            l.axes = [x * s.x, y * s.y, x.cross(y) * s.z];
            l.velocity = Vec3::ZERO;
        }
    }

    /// Runs effect `id` from its start along `path` (places and X axes, Bevy's
    /// axes) this frame, `step` seconds a place, as the game runs its trajectory
    /// preview along a simulated throw; it is left at the path's end.
    pub fn replay(&mut self, id: FxId, path: &[(Vec3, Vec3)], step: f32) {
        if let Some(l) = self.live.get_mut(&id.0) {
            let path: Vec<(Vec3, [Vec3; 3])> = path
                .iter()
                .map(|(p, x)| (to_ue(*p), axes_along(*x, l.stretch * l.size)))
                .collect();
            if let Some(&(end, axes)) = path.last() {
                l.origin = end;
                l.prev_origin = end;
                l.axes = axes;
            }
            l.replay = Some((path, step));
            l.replay_colors = None;
        }
    }

    /// Runs effect `id` along `path` as replay does, its colour parameter `param` set
    /// to `colors[k]` at place k (SetColorParameter at each step of the simulation:
    /// what spawns there takes it).
    pub fn replay_colored(
        &mut self,
        id: FxId,
        path: &[(Vec3, Vec3)],
        step: f32,
        param: &str,
        colors: Vec<[u8; 4]>,
    ) {
        self.replay(id, path, step);
        if let Some(l) = self.live.get_mut(&id.0) {
            l.replay_colors = Some((param.to_string(), colors));
        }
    }

    /// Starts tracer effect `name` at `at` with its X along `dir`, reaching
    /// `distance` metres out (TnWeaponEffectPlayTracerFX: SpawnEmitter, then
    /// SetTracerDistance): its beams end there (Bevy's axes), and its other
    /// particles (its bolts and streaks) go as they reach it, where the round struck,
    /// rather than flying on through what it struck for the rest of their lives.
    pub fn spawn_tracer(&mut self, name: &str, at: Vec3, dir: Vec3, distance: f32) -> Option<FxId> {
        let id = self.spawn(name, at, dir, &[])?;
        if let Some(l) = self.live.get_mut(&id.0) {
            let dir = dir.normalize_or(Vec3::X);
            l.beam_end = Some(to_ue(at + dir * distance));
            l.tracer = Some((to_ue(at), dir_to_ue(dir), distance.max(0.0) * 100.0));
        }
        Some(id)
    }

    /// Sets where effect `id`'s beams end (SetBeamTargetPoint, SetTracerEndPoint: a
    /// looping tracer's end, kept on where the latest round struck), in Bevy's axes.
    pub fn set_beam_end(&mut self, id: FxId, at: Vec3) {
        if let Some(l) = self.live.get_mut(&id.0) {
            l.beam_end = Some(to_ue(at));
        }
    }

    /// Sets where emitter `emitter` of effect `id` ends its beams
    /// (ParticleSystemComponent.SetBeamTargetPoint with its EmitterIndex), in
    /// Bevy's axes.
    pub fn set_emitter_beam_end(&mut self, id: FxId, emitter: usize, at: Vec3) {
        if let Some(l) = self.live.get_mut(&id.0) {
            if l.beam_ends.len() <= emitter {
                l.beam_ends.resize(emitter + 1, None);
            }
            l.beam_ends[emitter] = Some(to_ue(at));
        }
    }

    /// Turns effect `id` off (DeactivateSystem): nothing more spawns, what is out
    /// lives out its life (or goes at once, for emitters killed on deactivation).
    pub fn stop(&mut self, id: FxId) {
        if let Some(l) = self.live.get_mut(&id.0) {
            l.on = false;
        }
    }

    /// Draws effect `id` (just spawned) in the Customize garage, which its own
    /// camera sees, rather than in the game's world.
    pub fn in_garage(&mut self, id: FxId) {
        if let Some(l) = self.live.get_mut(&id.0) {
            l.garage = true;
        }
    }

    /// Takes effect `id` away now.
    pub fn kill(&mut self, id: FxId) {
        self.live.remove(&id.0);
    }

    /// Effect `id` is still showing.
    pub fn alive(&self, id: FxId) -> bool {
        self.live.contains_key(&id.0)
    }

    /// Effect `id` is still showing and on (not turned off: stop).
    pub fn is_on(&self, id: FxId) -> bool {
        self.live.get(&id.0).is_some_and(|l| l.on)
    }

    /// How long the first loop of effect `id`'s emitters that loop forever
    /// (EmitterLoops 0) lasts, the longest of them: None if none does (it ends by
    /// itself). A muzzle flash with one (the A-4 Pulsar Cannon's Plasma_Ball_Export,
    /// a burning ball spawned ten times a second for as long as it is on) is turned
    /// off then, as the game deactivates the gun's flash.
    pub fn forever_loop_secs(&self, id: FxId) -> Option<f32> {
        let live = self.live.get(&id.0)?;
        let system = self.data.as_ref()?.systems.get(live.system)?;
        system
            .emitters
            .iter()
            .filter(|e| e.loops == 0)
            .map(|e| e.duration.max(e.duration_low))
            .reduce(f32::max)
    }
}

/// An emitter of a spawned effect gone (its effect killed, or its particles done):
/// hidden at once, and despawned these many frames later.
///
/// (Not despawned at once: Bevy could draw an entity despawned in the frame others
/// are spawned once more, in its own material but with a new one's mesh where the
/// new one is. A Chaos-Rift Combustor's cluster going off, its canister's mesh
/// emitter going as its blast's emitters came, drew the canister's metal over the
/// blast's giant sprites for a frame: a huge grey picture over the screen. Hidden
/// first, it is out of the frame's draws before it goes.)
#[derive(Component)]
struct Retired(u8);

const RETIRED_FRAMES: u8 = 3;

/// Takes a spawned effect's emitter `entity` out of play: hidden now, despawned
/// RETIRED_FRAMES later (despawn_retired; see Retired).
/// Split screen's guest's copy of an effect entity (make_twins) hidden with it.
fn hide_twin(
    twins: &mut Query<(&mut FxDrawn, &mut Visibility, &mut Transform), With<TwinOf>>,
    twin: Option<&Twin>,
) {
    if let Some((mut d, mut v, _)) = twin.and_then(|t| twins.get_mut(t.0).ok()) {
        *v = Visibility::Hidden;
        d.hidden = true;
    }
}

fn retire(commands: &mut Commands, entity: Entity, visibility: &mut Visibility) {
    *visibility = Visibility::Hidden;
    commands
        .entity(entity)
        .remove::<SpawnedEmitter>()
        .insert(Retired(RETIRED_FRAMES));
}

/// Despawns the spawned effects' emitters retired long enough ago (Retired).
fn despawn_retired(mut commands: Commands, mut retired: Query<(Entity, &mut Retired)>) {
    for (entity, mut r) in &mut retired {
        r.0 = r.0.saturating_sub(1);
        if r.0 == 0 {
            commands.entity(entity).despawn();
        }
    }
}

/// An emitter of a spawned effect: an entity drawing its particles (FxDrawn).
#[derive(Component)]
struct SpawnedEmitter {
    id: u32,
    emitter: usize,
    seed: u32,
    sim: EmitterInstance,
    /// A decal emitter's marks as it last drew them (see `decal_key`).
    drawn_marks: Option<DecalKey>,
}

/// What a decal emitter draws: where from (the effect's place), and its particles'
/// marks with their dynamic parameters. Marks lie still, so they are drawn again
/// only when this changes: a particle laying its mark, or going.
type DecalKey = (Vec3, Vec<(Option<Option<u32>>, [f32; 4])>);

fn decal_key(def: &EmitterDef, sim: &EmitterInstance, anchor: Vec3) -> DecalKey {
    let marks = sim.particles.iter().take(def.max_draw);
    (anchor, marks.map(|p| (p.decal, p.dyn_params)).collect())
}

/// What spawned effects share: unlit materials (by material, kind and part), a
/// white texture, and the invisible transmissive triangle that has Bevy copy the
/// scene for materials that read it (SceneTexture, and modulation's bright part).
#[derive(Default)]
struct SpawnCache {
    white: Option<Handle<Image>>,
    /// (Material, kind, part, place in its effect's order.)
    shared: HashMap<(usize, Kind, ModPart, usize), Handle<FxMaterial>>,
    scene_copy: Option<Entity>,
    /// The picture's alpha put back (ModPart::AlphaRestore).
    alpha_restore: Option<Handle<FxMaterial>>,
    warmed: bool,
}

/// The material that puts the picture's alpha back to 1 once everything is drawn
/// (ModPart::AlphaRestore, FX_ALPHA_RESTORE in the shader): drawn after every other
/// effect part (its order), over the whole view.
fn alpha_restore_material(white: &Handle<Image>) -> FxMaterial {
    FxMaterial {
        params: FxParams::default(),
        tex0: white.clone(),
        tex1: white.clone(),
        tex2: white.clone(),
        tex3: white.clone(),
        tex4: white.clone(),
        tex5: white.clone(),
        key: FxKey {
            // (Any material's code: FX_ALPHA_RESTORE draws nothing of it.)
            map: WEAPON_SET,
            mat: 0,
            blend: Blend::Additive,
            lit: false,
            two_sided: true,
            kind: Kind::Particle,
            part: ModPart::AlphaRestore,
            fog: false,
            sky_fog: false,
            cube: false,
            distort: false,
            distorting: false,
            // (Its mesh has its own vertices: alpha_restore_mesh.)
            pulled: false,
        },
        cube: None,
        words: FX_WORDS_BUFFER,
        alpha: AlphaMode::Premultiplied,
        order: ALPHA_RESTORE_ORDER,
    }
}

/// Drawn after everything (Material::depth_bias, metres).
const ALPHA_RESTORE_ORDER: f32 = 1.0e6;

/// A triangle over the whole view, in clip space (FX_ALPHA_RESTORE draws it as it is).
fn alpha_restore_mesh() -> Mesh {
    restore_mesh(vec![[-1.0, -1.0, 0.0], [3.0, -1.0, 0.0], [-1.0, 3.0, 0.0]])
}

/// A triangle (`corners`) with every vertex attribute the alpha restore's material
/// reads: it isn't pulled (FxKey), its mesh has them all.
fn restore_mesh(corners: Vec<[f32; 3]>) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, corners);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0f32; 2]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, 1.0]; 3]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; 3]);
    mesh.insert_attribute(ATTRIBUTE_FX_PARAM, vec![[1.0f32; 4]; 3]);
    mesh.insert_attribute(ATTRIBUTE_FX_EXTRA, vec![[0.0f32; 4]; 3]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
    mesh
}

/// The kind an emitter's material is drawn as.
fn emitter_kind(em: &EmitterDef) -> Kind {
    if matches!(em.typedata, TypeData::Mesh(_)) {
        Kind::MeshParticle
    } else {
        Kind::Particle
    }
}

/// The parts an emitter's material is drawn in (its distortion's last in the list,
/// first in the drawing: mod_part).
fn emitter_parts(def: &MaterialDef) -> &'static [ModPart] {
    match (def.blend == Blend::Modulate, def.distort) {
        (true, true) => &[ModPart::Dark, ModPart::Bright, ModPart::Distort],
        (true, false) => &[ModPart::Dark, ModPart::Bright],
        (false, true) => &[ModPart::Whole, ModPart::Distort],
        (false, false) => &[ModPart::Whole],
    }
}

/// A spawned effect's emitter's material, drawn `order`-th of its effect's emitters
/// (EMITTER_ORDER).
///
/// It takes no light, lit material or not: what the game spawns (its emitter
/// pool's particle components, `EmitterPool.PSCTemplate`, and the weapons' own)
/// is a ParticleSystemComponent as the engine makes one, which accepts no lights
/// (`bAcceptsLights` is PrimitiveComponent's default, false, and neither
/// ParticleSystemComponent's nor the pool's template sets it) and has no light
/// environment: its lit smoke shows its own emissive colour alone. Only the maps'
/// placed emitters that turn both on (Oblivion's incinerator fire, say) are lit
/// (light_at). So a tank shell's blast is its fire and a dark, see-through haze, as
/// in the game, where it was a cloud of grey or white smoke lit by the lights
/// round it, as dark as the light from all around in the darker places.
#[allow(clippy::too_many_arguments)]
fn spawned_material(
    data: &FxData,
    em: &EmitterDef,
    order: usize,
    part: ModPart,
    lit: Option<FxParams>,
    cache: &mut SpawnCache,
    materials: &mut Assets<FxMaterial>,
    asset_server: &AssetServer,
    white: &Handle<Image>,
) -> Option<Handle<FxMaterial>> {
    let def = data.materials.get(em.material)?;
    let kind = emitter_kind(em);
    // (One the game lights, by the light where it starts: set_lit.)
    if let Some(params) = lit.filter(|_| def.lit) {
        let mut m = make_material(
            def,
            WEAPON_SET as usize,
            em.material,
            kind,
            true,
            params,
            asset_server,
            white,
        );
        m.order = emitter_order(order);
        return Some(materials.add(mod_part(m, part, def)));
    }
    Some(
        cache
            .shared
            .entry((em.material, kind, part, order))
            .or_insert_with(|| {
                let mut m = make_material(
                    def,
                    WEAPON_SET as usize,
                    em.material,
                    kind,
                    false,
                    FxParams::default(),
                    asset_server,
                    white,
                );
                m.order = emitter_order(order);
                materials.add(mod_part(m, part, def))
            })
            .clone(),
    )
}

/// The effect reads the scene behind it (its copy): to shift it, too.
fn needs_scene(data: &FxData, system: &SystemDef) -> bool {
    system.emitters.iter().any(|em| {
        data.materials
            .get(em.material)
            .is_some_and(|m| m.scene || m.distort || m.blend == Blend::Modulate)
    })
}

/// Gives newly spawned effects (FxSystems) their emitters' entities (a modulating
/// one a second, its bright part, as a child), and keeps the scene's copy while
/// anything needs it. Once the effects are read, every pipeline they draw with is
/// made at once (a zero-area mesh with each material), so none is missing the first
/// time an effect plays.
#[allow(clippy::too_many_arguments)]
fn start_spawned_fx(
    mut commands: Commands,
    mut systems: ResMut<FxSystems>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FxMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    lighting: Option<Res<MapLighting>>,
    mut cache: Local<SpawnCache>,
    mut slots: ResMut<FxSlots>,
) {
    let systems = &mut *systems;
    let Some(data) = systems.load().cloned() else {
        systems.live.clear();
        return;
    };
    let cache = &mut *cache;
    let white = cache
        .white
        .get_or_insert_with(|| white_image(&mut images))
        .clone();

    if !std::mem::replace(&mut cache.warmed, true) {
        // (Slot 0's: nothing to draw.)
        let mesh = bucket_handle(0);
        // (The alpha's restore's too, with a mesh of its own: a triangle of no size.)
        let restore = materials.add(alpha_restore_material(&white));
        cache.alpha_restore = Some(restore.clone());
        commands.spawn((
            Mesh3d(meshes.add(restore_mesh(vec![[0.0; 3]; 3]))),
            MeshMaterial3d(restore),
            Transform::default(),
            NoFrustumCulling,
            NotShadowCaster,
            NotShadowReceiver,
        ));
        // (Every pipeline once: an emitter's place in its effect's order doesn't
        // change its pipeline. The solid pieces that are lit, gibs.rs's chunks, lit
        // too, as they are drawn.)
        let mut lit_solid = std::collections::HashSet::new();
        for system in &data.systems {
            for em in &system.emitters {
                let Some(def) = data.materials.get(em.material) else {
                    continue;
                };
                if system.lit
                    && def.lit
                    && def.blend == Blend::Masked
                    && lit_solid.insert((em.material, emitter_kind(em)))
                    && let Some(material) = spawned_material(
                        &data,
                        em,
                        0,
                        ModPart::Whole,
                        Some(FxParams::default()),
                        cache,
                        &mut materials,
                        &asset_server,
                        &white,
                    )
                {
                    commands.spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material),
                        Transform::default(),
                        NoFrustumCulling,
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
                for &part in emitter_parts(def) {
                    let key = (em.material, emitter_kind(em), part, 0);
                    if cache.shared.contains_key(&key) {
                        continue;
                    }
                    let Some(material) = spawned_material(
                        &data,
                        em,
                        0,
                        part,
                        None,
                        cache,
                        &mut materials,
                        &asset_server,
                        &white,
                    ) else {
                        continue;
                    };
                    commands.spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material),
                        Transform::default(),
                        NoFrustumCulling,
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
        }
    }

    for (&id, live) in systems.live.iter_mut().filter(|(_, l)| !l.started) {
        live.started = true;
        let Some(system) = data.systems.get(live.system) else {
            continue;
        };
        // (The game's on the first player's view's layer, copied for split screen's
        // guest's, make_twins; one on a player's screen on his alone.)
        let layer = if live.garage {
            RenderLayers::layer(crate::customize::GARAGE_LAYER)
        } else {
            RenderLayers::layer(crate::split::VIEW_LAYERS[live.seen_by.unwrap_or(0).min(1)])
        };
        let lit = (live.lit && system.lit).then(|| {
            let mut p = light_at(lighting.as_deref(), live.origin);
            if live.sunless {
                p.sun = Vec4::new(0.0, 0.0, 0.0, 1.0);
            }
            p
        });
        for (ei, em) in system.emitters.iter().enumerate() {
            if matches!(em.typedata, TypeData::Mesh(usize::MAX)) {
                continue;
            }
            let Some(def) = data.materials.get(em.material) else {
                continue;
            };
            let parts = emitter_parts(def);
            let mut handles = Vec::new();
            for &part in parts {
                handles.extend(spawned_material(
                    &data,
                    em,
                    ei,
                    part,
                    lit,
                    cache,
                    &mut materials,
                    &asset_server,
                    &white,
                ));
            }
            let Some(first) = handles.first().cloned() else {
                continue;
            };
            let seed = live
                .seed
                .wrapping_add((ei as u32).wrapping_mul(0x85eb_ca6b));
            let anchor = to_bevy(live.origin);
            let slot = slots.take();
            let entity = commands
                .spawn((
                    FxPart,
                    SpawnedEmitter {
                        id,
                        emitter: ei,
                        seed,
                        sim: EmitterInstance::new(em, live.system, ei, 0, seed),
                        drawn_marks: None,
                    },
                    FxDrawn::new(slot),
                    MeshTag(slot),
                    Mesh3d(bucket_handle(0)),
                    MeshMaterial3d(first),
                    Transform::from_translation(anchor),
                    // (Shown once it has something to draw.)
                    Visibility::Hidden,
                    NoFrustumCulling,
                    NotShadowCaster,
                    NotShadowReceiver,
                    layer.clone(),
                ))
                .id();
            // (On one player's screen: his view alone has it.)
            if let Some(seat) = live.seen_by {
                commands.entity(entity).insert(SeenBy(seat));
            }
            // (Its other parts draw what it draws.)
            for material in handles.into_iter().skip(1) {
                commands.spawn((
                    MeshTag(slot),
                    Mesh3d(bucket_handle(0)),
                    MeshMaterial3d(material),
                    Transform::default(),
                    NoFrustumCulling,
                    NotShadowCaster,
                    NotShadowReceiver,
                    layer.clone(),
                    ChildOf(entity),
                ));
            }
            live.emitters += 1;
        }
    }

    // The scene's copy, while an effect reading it is out.
    let wanted = systems.live.values().any(|l| {
        data.systems
            .get(l.system)
            .is_some_and(|s| needs_scene(&data, s))
    });
    match (wanted, cache.scene_copy) {
        (true, None) => {
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 3]);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3]);
            mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
            let copy = commands
                .spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(standard.add(StandardMaterial {
                        base_color: Color::NONE,
                        specular_transmission: 1.0,
                        unlit: true,
                        ..default()
                    })),
                    Transform::default(),
                    NoFrustumCulling,
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id();
            // (And, as long as there is something distorting to use up the
            // picture's alpha, its restore: in the game's view and the garage's.)
            if let Some(restore) = cache.alpha_restore.clone() {
                commands.spawn((
                    Mesh3d(meshes.add(alpha_restore_mesh())),
                    MeshMaterial3d(restore),
                    Transform::default(),
                    NoFrustumCulling,
                    NotShadowCaster,
                    NotShadowReceiver,
                    RenderLayers::from_layers(&[0, crate::customize::GARAGE_LAYER]),
                    ChildOf(copy),
                ));
            }
            cache.scene_copy = Some(copy);
        }
        (false, Some(e)) => {
            commands.entity(e).despawn();
            cache.scene_copy = None;
        }
        _ => {}
    }
}

/// Puts the spawned effects' meshes where their effects are now (FxSystems::place).
fn place_spawned_fx(
    systems: Res<FxSystems>,
    mut emitters: Query<(&SpawnedEmitter, &mut Transform, Option<&Twin>), Without<TwinOf>>,
    mut twins: Query<&mut Transform, With<TwinOf>>,
) {
    for (e, mut transform, twin) in &mut emitters {
        if let Some(live) = systems.live.get(&e.id) {
            let anchor = to_bevy(live.origin);
            if transform.translation != anchor {
                transform.translation = anchor;
            }
            // (Split screen's guest's copy with it.)
            if let Some(mut t) = twin.and_then(|t| twins.get_mut(t.0).ok())
                && t.translation != anchor
            {
                t.translation = anchor;
            }
        }
    }
}

/// Split screen (split.rs): while there is a guest, every effect entity of the
/// game's (not the garage's, nor one on a player's screen: SeenBy) has a copy for
/// his view (Twin), drawing from a slot of its own with the same materials (its
/// other parts' too) on his view's layer; a copy goes with what it is a copy of,
/// and all go as he leaves.
#[allow(clippy::type_complexity)]
fn make_twins(
    mut commands: Commands,
    split: Res<crate::split::Split>,
    mut slots: ResMut<FxSlots>,
    mains: Query<
        (
            Entity,
            &MeshMaterial3d<FxMaterial>,
            &Transform,
            &RenderLayers,
            Option<&Children>,
            Option<&FlareMesh>,
            Has<Twin>,
        ),
        (
            With<FxDrawn>,
            Without<TwinOf>,
            Without<SeenBy>,
            Without<Retired>,
        ),
    >,
    parts: Query<&MeshMaterial3d<FxMaterial>, (With<MeshTag>, Without<FxDrawn>)>,
    twins: Query<(Entity, &TwinOf)>,
    drawn: Query<(), (With<FxDrawn>, Without<TwinOf>)>,
) {
    for (twin, of) in &twins {
        if !split.active() || !drawn.contains(of.0) {
            commands.entity(twin).despawn();
            if let Ok(mut main) = commands.get_entity(of.0) {
                main.try_remove::<Twin>();
            }
        }
    }
    if !split.active() {
        return;
    }
    let view = RenderLayers::layer(crate::split::VIEW_LAYERS[0]);
    let guest = RenderLayers::layer(crate::split::VIEW_LAYERS[1]);
    for (main, material, transform, layers, children, flare, has_twin) in &mains {
        if has_twin || !layers.intersects(&view) {
            continue;
        }
        let slot = slots.take();
        let mut twin = commands.spawn((
            FxPart,
            TwinOf(main),
            FxDrawn::new(slot),
            MeshTag(slot),
            Mesh3d(bucket_handle(0)),
            material.clone(),
            *transform,
            Visibility::Hidden,
            NoFrustumCulling,
            NotShadowCaster,
            NotShadowReceiver,
            guest.clone(),
        ));
        if let Some(flare) = flare {
            twin.insert(*flare);
        }
        let twin = twin.id();
        for &child in children.map(|c| &c[..]).unwrap_or_default() {
            if let Ok(m) = parts.get(child) {
                commands.spawn((
                    MeshTag(slot),
                    Mesh3d(bucket_handle(0)),
                    m.clone(),
                    Transform::default(),
                    NoFrustumCulling,
                    NotShadowCaster,
                    NotShadowReceiver,
                    guest.clone(),
                    ChildOf(twin),
                ));
            }
        }
        commands.entity(main).insert(Twin(twin));
    }
}

/// The view split screen's guest's copies are drawn for (his camera's), while there
/// is one: as `fx_camera`, the effects' far limit `far` (None: the garage's own,
/// all near).
fn guest_camera_view(
    cameras: &FxCameras,
    garage: &GarageScene,
    far: Option<f32>,
) -> Option<CameraView> {
    if garage.0 {
        return None;
    }
    cameras
        .iter()
        .find(|(_, _, in_garage, seat)| !*in_garage && crate::split::seat_of(*seat) == 1)
        .map(|(t, p, ..)| {
            let mut cam = camera_view(t, p);
            if let Some(far) = far {
                cam.far = far;
            }
            cam
        })
}

/// Whether what an emitter draws depends on the camera (sprites, trails and beams
/// face it; mesh particles and decals don't): a copy draws them anew for its view,
/// the others as they are.
fn view_dependent(def: &EmitterDef) -> bool {
    matches!(
        def.typedata,
        TypeData::Sprite | TypeData::Trail { .. } | TypeData::Beam(_)
    )
}

/// Runs and draws the spawned effects (FxSystems), and lets go of those that are
/// done. Each effect's emitters run in their order, as UE3 ticks them, so those
/// that spawn at another's particles (LocationEmitter) see where that one's are.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn run_spawned_fx(
    mut commands: Commands,
    time: Res<Time>,
    mut systems: ResMut<FxSystems>,
    level: Option<Res<crate::Level>>,
    camera: Query<
        (&GlobalTransform, &Projection, Option<&crate::split::Seat>),
        With<crate::OrbitCamera>,
    >,
    mut emitters: Query<
        (
            Entity,
            &mut SpawnedEmitter,
            &mut FxDrawn,
            &mut Transform,
            &mut Visibility,
            (Option<&Twin>, Option<&SeenBy>),
        ),
        Without<TwinOf>,
    >,
    mut twins: Query<(&mut FxDrawn, &mut Visibility, &mut Transform), With<TwinOf>>,
    mut order: Local<Vec<(u32, usize, Entity)>>,
    fxq: Res<FxQuality>,
    mut budget: ResMut<FxBudget>,
) {
    let systems = &mut *systems;
    let Some(data) = systems.data.clone() else {
        return;
    };

    // Run them.
    // (The first player's camera; split screen's guest's draws the copies, and
    // what is on his screen alone: make_twins, SeenBy.)
    let view_of = |seat: usize| {
        camera
            .iter()
            .find(|(.., s)| crate::split::seat_of(*s) == seat)
            .map(|(t, p, _)| {
                let mut cam = camera_view(t, p);
                cam.far = fxq.far;
                (cam, t.translation())
            })
    };
    let Some((cam, eye)) = view_of(0) else {
        return;
    };
    let guest = view_of(1);
    systems.camera = Some(to_ue(eye));
    systems.guest_camera = guest.as_ref().map(|(_, eye)| to_ue(*eye));
    let guest_cam = guest.map(|(cam, _)| cam);
    let dt = time.delta_secs().min(0.1);
    order.clear();
    order.extend(
        emitters
            .iter()
            .map(|(entity, e, ..)| (e.id, e.emitter, entity)),
    );
    order.sort_unstable();
    // The source emitters' particles of the effect being run.
    let mut sources: Vec<Source> = Vec::new();
    let mut sources_of = None;
    for &(id, _, entity) in order.iter() {
        let Ok((entity, mut e, mut drawn, mut transform, mut visibility, (twin, seen))) =
            emitters.get_mut(entity)
        else {
            continue;
        };
        let e = &mut *e;
        if sources_of != Some(id) {
            sources_of = Some(id);
            sources.clear();
        }
        let Some(live) = systems.live.get_mut(&e.id) else {
            retire(&mut commands, entity, &mut visibility);
            hide_twin(&mut twins, twin);
            continue;
        };
        let Some(def) = data
            .systems
            .get(live.system)
            .and_then(|s| s.emitters.get(e.emitter))
        else {
            continue;
        };
        if let Some((path, step)) = &live.replay {
            // From scratch, along the path (its colour parameter as it was at each
            // place, if it changes along it).
            e.sim = EmitterInstance::new(def, live.system, e.emitter, 0, e.seed);
            e.sim.replayed = true;
            let mut prev = path.first().map(|p| p.0).unwrap_or(live.origin);
            let mut stepped = live.params.clone();
            for (k, (at, axes)) in path.iter().enumerate() {
                if let Some((param, colors)) = &live.replay_colors
                    && let Some(&c) = colors.get(k)
                {
                    match stepped.iter_mut().find(|(n, _)| n == param) {
                        Some((_, v)) => *v = c,
                        None => stepped.push((param.clone(), c)),
                    }
                }
                let frame = SimFrame {
                    origin: *at,
                    axes: *axes,
                    size: live.size,
                    params: &stepped,
                    vectors: &live.vectors,
                    prev_origin: prev,
                    velocity: (*at - prev) / step.max(1e-4),
                    sources: &[],
                };
                e.sim.tick(def, &frame, *step, true);
                prev = *at;
            }
        } else {
            let frame = SimFrame {
                origin: live.origin,
                axes: live.axes,
                size: live.size,
                params: &live.params,
                vectors: &live.vectors,
                prev_origin: live.prev_origin,
                velocity: live.velocity,
                sources: &sources,
            };
            // (Turned off, the particles of an emitter killed on deactivation go at
            // once, as do those of an emitter whose particles never die: nothing else
            // would end them.)
            if !live.on && (def.kill_on_deactivate || def.immortal) {
                e.sim.particles.clear();
            }
            if dt > 0.0 {
                e.sim.tick(def, &frame, dt, live.on);
            }
            // A tracer's particles go as they reach where its round struck.
            if let Some((from, along, reach)) = live.tracer
                && !matches!(def.typedata, TypeData::Beam(_))
            {
                let local = def.local;
                e.sim.particles.retain(|p| {
                    let at = if local {
                        frame.point(p.location)
                    } else {
                        p.location
                    };
                    (at - from).dot(along) < reach
                });
            }
        }
        if def.is_source {
            sources.push(Source {
                name: def.name.clone(),
                local: def.local,
                particles: e
                    .sim
                    .particles
                    .iter()
                    .map(|p| (p.location, p.velocity, p.rotation))
                    .collect(),
                times: e.sim.particles.iter().map(|p| p.relative_time).collect(),
                beam: matches!(def.typedata, TypeData::Beam(_)),
            });
        }
        if e.sim.finished(def) || (!live.on && e.sim.particles.is_empty()) {
            retire(&mut commands, entity, &mut visibility);
            hide_twin(&mut twins, twin);
            live.emitters = live.emitters.saturating_sub(1);
            continue;
        }
        // (On the guest's screen alone, drawn for his camera.)
        let cam = match (seen, guest_cam.as_ref()) {
            (Some(SeenBy(1)), Some(gcam)) => gcam,
            _ => &cam,
        };
        let frame = SimFrame {
            origin: live.origin,
            axes: live.axes,
            size: live.size,
            params: &live.params,
            vectors: &live.vectors,
            prev_origin: live.prev_origin,
            velocity: live.velocity,
            sources: &[],
        };
        if e.sim.particles.is_empty() && drawn.hidden {
            continue;
        }
        let anchor = to_bevy(live.origin);
        let decal = matches!(def.typedata, TypeData::Decal(_));
        if decal && e.drawn_marks.as_ref() == Some(&decal_key(def, &e.sim, anchor)) {
            continue;
        }
        let distance = (live.origin - cam.pos).length();
        if budget.defer(distance, drawn.hidden || decal, entity.index_u32()) {
            continue;
        }
        transform.translation = anchor;
        let head = (live.on && matches!(def.typedata, TypeData::Trail { .. })).then(|| {
            live.origin
                + if def.local {
                    Vec3::ZERO
                } else {
                    e.sim.spawn_offset
                }
        });
        let beam_end = live
            .beam_ends
            .get(e.emitter)
            .copied()
            .flatten()
            .or(live.beam_end);
        let buffers = draw_emitter(
            &data,
            def,
            &mut e.sim,
            &frame,
            cam,
            anchor,
            head,
            level.as_deref(),
            beam_end,
        );
        // Split screen's guest's copy, for his camera (make_twins).
        if let (Some(gcam), Some(twin)) = (guest_cam.as_ref(), twin)
            && let Ok((mut t_drawn, mut t_visibility, mut t_transform)) = twins.get_mut(twin.0)
        {
            t_transform.translation = anchor;
            let copy = if view_dependent(def) {
                draw_emitter(
                    &data,
                    def,
                    &mut e.sim,
                    &frame,
                    gcam,
                    anchor,
                    head,
                    level.as_deref(),
                    beam_end,
                )
            } else {
                buffers.clone()
            };
            show_sprites(&mut t_drawn, copy, (gcam, anchor), &mut t_visibility);
        }
        let wrote = show_sprites(&mut drawn, buffers, (cam, anchor), &mut visibility);
        if wrote {
            budget.wrote(distance);
        }
        // (A decal out of view is drawn when it comes into view.)
        if decal && wrote {
            e.drawn_marks = Some(decal_key(def, &e.sim, anchor));
        }
    }
    // Effects with nothing left go; the rest remember where they were.
    systems.live.retain(|_, l| !l.started || l.emitters > 0);
    for l in systems.live.values_mut() {
        l.prev_origin = l.origin;
        l.replay = None;
        l.replay_colors = None;
    }
}

// ---------------------------------------------------------------------------
// Lens flares
// ---------------------------------------------------------------------------

/// The flares' half-size of their source (FixedRelativeBoundingBox), cm: how much of
/// the screen it covers sets the flare's occlusion value (ScreenPercentageMap).
const FLARE_SOURCE_RADIUS: f32 = 100.0;

/// How far in front of the camera the garage's flares are drawn (cm; twice its near
/// plane at least): see flare_elements.
const FLARE_DRAW_DEPTH: f32 = 30.0;

/// Sight lines to the flares' lights tested a frame, at most, and the time they may
/// take (after the first two). Each walks the level's collision grid to its light
/// (up to a few tenths of a millisecond on the long maps), so the tests are spread
/// over frames.
const FLARE_RAYS_PER_FRAME: usize = 8;
const FLARE_RAY_TIME: std::time::Duration = std::time::Duration::from_micros(300);

/// The flare sources' visibility, kept from frame to frame.
#[derive(Default)]
struct FlareSight {
    /// The map's effects these are for.
    data: Option<Arc<FxData>>,
    /// How visible each source is (0-1, eased).
    visible: Vec<f32>,
    /// Its sight line went through a wall when last tested.
    blocked: Vec<bool>,
    /// Tested since it came on screen.
    checked: Vec<bool>,
    /// The source the next tests in turn start from.
    next: usize,
}

/// Rebuilds the lens flares' meshes: every flare whose source is in front of the
/// camera and not hidden, its elements along the line through the screen's middle.
///
/// Split screen (split.rs): the guest's view has its own, for his camera, in the
/// flare meshes' copies (make_twins), each source's visibility its own.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw_flares(
    time: Res<Time>,
    fx: Res<LevelFx>,
    level: Option<Res<crate::Level>>,
    garage: Res<GarageScene>,
    cameras: FxCameras,
    mut flares: Query<(&FlareMesh, &mut FxDrawn, &mut Visibility), Without<TwinOf>>,
    mut copies: Query<(&FlareMesh, &mut FxDrawn, &mut Visibility), With<TwinOf>>,
    mut sight: Local<[FlareSight; crate::split::SEATS]>,
) {
    let Some(data) = fx.data.as_ref() else {
        return;
    };
    let Some((cam_transform, projection)) = fx_camera(&cameras, &garage) else {
        return;
    };
    // (The garage has no collision: its flares are hidden by nothing but the
    // depth they are drawn at.)
    let level = level.filter(|_| !garage.0);
    let cam = camera_view(cam_transform, projection);
    let dt = time.delta_secs();
    let [first, second] = &mut *sight;
    flares_for(
        (&cam, garage.0, dt),
        data,
        level.as_deref(),
        flares.iter_mut(),
        first,
    );
    if let Some(gcam) = guest_camera_view(&cameras, &garage, None) {
        flares_for(
            (&gcam, garage.0, dt),
            data,
            level.as_deref(),
            copies.iter_mut(),
            second,
        );
    }
}

/// The lens flares as one camera sees them (draw_flares), into its view's flare
/// meshes, `sight` its sources' visibility.
fn flares_for<'a>(
    (cam, garage, dt): (&CameraView, bool, f32),
    data: &Arc<FxData>,
    level: Option<&crate::Level>,
    flares: impl Iterator<Item = (&'a FlareMesh, Mut<'a, FxDrawn>, Mut<'a, Visibility>)>,
    sight: &mut FlareSight,
) {
    let n = data.flare_places.len();
    if !sight.data.as_ref().is_some_and(|d| Arc::ptr_eq(d, data)) {
        *sight = FlareSight {
            data: Some(data.clone()),
            visible: vec![0.0; n],
            blocked: vec![true; n],
            checked: vec![false; n],
            next: 0,
        };
    }
    if n == 0 {
        return;
    }

    // The sources that could show: in front, on screen, and not faded out by their
    // distance (every element's DistMap_Alpha zero there).
    let in_front: Vec<bool> = data
        .flare_places
        .iter()
        .map(|place| (place.origin - cam.pos).dot(cam.forward) > 10.0)
        .collect();
    let candidate: Vec<bool> = data
        .flare_places
        .iter()
        .zip(&in_front)
        .map(|(place, &front)| {
            let Some(flare) = data.flares.get(place.flare) else {
                return false;
            };
            if !front {
                return false;
            }
            let to_source = place.origin - cam.pos;
            let depth = to_source.dot(cam.forward);
            let source_distance = to_source.length();
            let far = flare.elements.iter().all(|e| {
                e.dists
                    .get("distalpha")
                    .map(|d| d.float_r(source_distance, 0.5) <= 0.001)
                    .unwrap_or(false)
            });
            let ndc = Vec2::new(
                to_source.dot(cam.right) / (depth * cam.tan_half.x),
                to_source.dot(cam.up) / (depth * cam.tan_half.y),
            );
            !far && ndc.x.abs() < 1.2 && ndc.y.abs() < 1.2
        })
        .collect();

    // Whether a wall hides each: a ray to it through the level's collision. The
    // sources that came on screen are tested first (until then they stay hidden, so
    // a light behind a wall doesn't flash up), then the others in turn.
    for k in 0..n {
        if !candidate[k] {
            sight.checked[k] = false;
        }
    }
    match level.and_then(|l| l.collision.as_ref()) {
        Some(collision) => {
            let from = to_bevy(cam.pos);
            let blocked = |k: usize| {
                let dir = to_bevy(data.flare_places[k].origin) - from;
                let len = dir.length();
                len > 0.5
                    && collision
                        .raycast_hit(from.to_array(), (dir / len).to_array(), len - 0.5)
                        .is_some()
            };
            let started = std::time::Instant::now();
            let mut budget = FLARE_RAYS_PER_FRAME;
            let out_of_time = |budget: usize| {
                budget == 0
                    || (budget <= FLARE_RAYS_PER_FRAME - 2 && started.elapsed() > FLARE_RAY_TIME)
            };
            let mut tested = vec![false; n];
            for k in 0..n {
                if out_of_time(budget) {
                    break;
                }
                if candidate[k] && !sight.checked[k] {
                    sight.blocked[k] = blocked(k);
                    sight.checked[k] = true;
                    tested[k] = true;
                    budget -= 1;
                }
            }
            let mut k = sight.next % n;
            for _ in 0..n {
                if out_of_time(budget) {
                    break;
                }
                if candidate[k] && !tested[k] {
                    sight.blocked[k] = blocked(k);
                    sight.checked[k] = true;
                    budget -= 1;
                }
                k = (k + 1) % n;
            }
            sight.next = k;
        }
        None => {
            for k in 0..n {
                sight.blocked[k] = false;
                sight.checked[k] = candidate[k];
            }
        }
    }

    let ease = (dt * 12.0).min(1.0);
    for k in 0..n {
        let target = if candidate[k] && sight.checked[k] && !sight.blocked[k] {
            1.0
        } else {
            0.0
        };
        sight.visible[k] += (target - sight.visible[k]) * ease;
    }
    for (flare_mesh, mut drawn, mut visibility) in flares {
        let mut out = SpriteBuffers::default();
        for (k, place) in data.flare_places.iter().enumerate() {
            if !in_front[k] || sight.visible[k] < 0.01 {
                continue;
            }
            let Some(flare) = data.flares.get(place.flare) else {
                continue;
            };
            flare_elements(
                flare,
                place,
                cam,
                sight.visible[k],
                flare_mesh.material,
                garage,
                &mut out,
            );
        }
        // (Hidden while none shows.)
        if out.is_empty() {
            if !drawn.hidden {
                *visibility = Visibility::Hidden;
                drawn.hidden = true;
            }
            continue;
        }
        drawn.sprites = out;
        if drawn.hidden {
            *visibility = Visibility::Inherited;
            drawn.hidden = false;
        }
    }
}

/// A flare's elements drawn with `material`: as quads at the source's depth, or (the
/// garage's, `over_picture`) over the picture as the game draws them.
#[allow(clippy::too_many_arguments)]
fn flare_elements(
    flare: &FlareDef,
    place: &FlarePlacement,
    cam: &CameraView,
    visible: f32,
    material: usize,
    over_picture: bool,
    out: &mut SpriteBuffers,
) {
    let to_source = place.origin - cam.pos;
    let depth = to_source.dot(cam.forward).max(1.0);
    let source_distance = to_source.length();
    let ndc = Vec2::new(
        to_source.dot(cam.right) / (depth * cam.tan_half.x),
        to_source.dot(cam.up) / (depth * cam.tan_half.y),
    );
    let radial = ndc.length();
    // How much of the screen the source covers, mapped to the occlusion value.
    let r = Vec2::new(
        FLARE_SOURCE_RADIUS / (depth * cam.tan_half.x),
        FLARE_SOURCE_RADIUS / (depth * cam.tan_half.y),
    );
    let coverage = (std::f32::consts::PI * r.x * r.y / 4.0).min(1.0) * visible;
    let occlusion = flare.screen_map.float_r(coverage, 0.5);
    // The game draws a flare over the picture: what is round its light (the lamp it
    // shines from, a wall it is set against) doesn't hide it or fade it out (the
    // material's DepthBiasedAlpha), only what stands in front of the light (the
    // sight line to it, above, and the shader's look at the depth where it is on
    // the screen). So the garage's quads are drawn just in front of the camera,
    // where they cover the part of the screen they would at the light; the shader
    // takes their fog from the light's distance. (The maps' are drawn at their
    // lights.)
    let pull = if over_picture {
        ((cam.near * 2.0).max(FLARE_DRAW_DEPTH) / depth).min(1.0)
    } else {
        1.0
    };
    let draw_depth = depth * pull;
    // A point on the screen at that depth, in the world.
    let at_screen = |p: Vec2| -> Vec3 {
        cam.pos
            + cam.forward * draw_depth
            + cam.right * (p.x * draw_depth * cam.tan_half.x)
            + cam.up * (p.y * draw_depth * cam.tan_half.y)
    };
    // (For the shader: the light's depth, and where it is on the screen.)
    let source_uv = Vec2::new(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let one = Dist::constant(1.0);
    for el in flare.elements.iter().filter(|e| e.material == material) {
        let d = |key: &str| el.dists.get(key).unwrap_or(&one);
        let t = if el.use_source_distance {
            source_distance
        } else if el.normalize_radial {
            radial / std::f32::consts::SQRT_2
        } else {
            radial
        };
        let pick = [0.5; 3];
        let scaling = d("scaling").eval(t, pick)[0];
        let axis = Vec3::from(d("axis").eval(t, pick));
        let rotation = el
            .dists
            .get("rotation")
            .map(|r| r.eval(t, pick)[0])
            .unwrap_or(0.0);
        let color = Vec3::from(d("color").eval(t, pick))
            * Vec3::from(d("distcolor").eval(source_distance, pick));
        let alpha = d("alpha").eval(t, pick)[0] * d("distalpha").eval(source_distance, pick)[0];
        let offset = el
            .dists
            .get("offset")
            .map(|o| Vec3::from(o.eval(t, pick)))
            .unwrap_or(Vec3::ZERO);
        let dist_scale = Vec3::from(d("distscale").eval(source_distance, pick));
        if alpha * place.color.w <= 0.0 {
            continue;
        }
        let color = (color * place.color.truncate()).extend(alpha * place.color.w);
        let size = Vec2::new(
            el.size.x * scaling * axis.x * dist_scale.x,
            el.size.y * scaling * axis.y * dist_scale.y,
        ) * place.size;
        let (s, c) = rotation.sin_cos();
        let corners_uv = [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        let mut corners = [Vec3::ZERO; 4];
        let (right, down);
        if el.source {
            // The source element: in the world at the source, its size in units.
            right = cam.right * c - cam.up * s;
            down = -(cam.up * c + cam.right * s);
            for (k, uv) in corners_uv.iter().enumerate() {
                let w =
                    place.origin + right * (size.x * (uv.x - 0.5)) + down * (size.y * (uv.y - 0.5));
                corners[k] = to_bevy(cam.pos + (w - cam.pos) * pull);
            }
        } else {
            // A reflection: on the line from the source through the middle (0 at
            // the source, 1 mirrored), its size a part of the screen: of its width
            // in the garage (as wide as the screen at 1), where the glows round
            // the lamps in footage of the game are that size wherever they are
            // seen from; of its height on the maps. Square on the screen.
            let center = ndc * (1.0 - 2.0 * el.ray) + offset.truncate();
            let aspect = cam.tan_half.x / cam.tan_half.y;
            // (Half-widths of the screen, or half-heights, and the screen's x and y
            // in them.)
            let (unit, axes) = if over_picture {
                (2.0, Vec2::new(1.0, aspect))
            } else {
                (1.0, Vec2::new(1.0 / aspect, 1.0))
            };
            for (k, uv) in corners_uv.iter().enumerate() {
                let q = Vec2::new(size.x * (uv.x - 0.5), -size.y * (uv.y - 0.5)) * unit;
                let q = Vec2::new(q.x * c - q.y * s, q.x * s + q.y * c);
                corners[k] = to_bevy(at_screen(center + q * axes));
            }
            right = cam.right;
            down = -cam.up;
        }
        out.quad(
            corners,
            dir_to_bevy(-cam.forward),
            dir_to_bevy(right),
            dir_to_bevy(down),
            corners_uv,
            corners_uv,
            color,
            Vec4::new(visible, occlusion, radial, el.ray),
            Vec4::new(depth, source_distance, source_uv.x, source_uv.y),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each material's own shader (SPLIT) from the real shaders/levelfx.wgsl: one
    /// for every material's function, each the shared code (entry points and all)
    /// with that function and no other, under a tenth of the whole; and the stems
    /// their defs name are the sets'. (FOC_DUMP_SPLIT=<dir> writes them out, as
    /// FXM_<SET>_<n>.wgsl, for a check that composes and validates them.)
    #[test]
    fn every_material_gets_a_shader_of_its_own() {
        let source = std::fs::read_to_string(crate::assets_dir().join(SHADER)).unwrap();
        let defs: Vec<&str> = source
            .lines()
            .filter_map(|l| l.strip_prefix("#ifdef FXM_"))
            .map(|d| d.trim())
            .collect();
        let split = SplitSource::new(&source);
        assert!(defs.len() > 300, "{}", defs.len());
        assert_eq!(split.blocks.len(), defs.len());
        let sets: std::collections::HashSet<String> = (0..crate::MAPS.len() as u8)
            .chain([WEAPON_SET, GARAGE_SET])
            .map(|set| set_stem(set).to_uppercase())
            .collect();
        let dump = std::env::var("FOC_DUMP_SPLIT").ok();
        let shared = split.shader(usize::MAX);
        assert!(shared.contains("fn vertex(") && shared.contains("fn fragment("));
        assert!(!shared.contains("#ifdef FXM_"));
        for (i, (def, _)) in split.blocks.iter().enumerate() {
            assert_eq!(*def, defs[i]);
            let (set, mat) = def.rsplit_once('_').unwrap();
            assert!(sets.contains(set), "{def}");
            assert!(mat.parse::<u16>().is_ok(), "{def}");
            let text = split.shader(i);
            assert_eq!(text.matches("#ifdef FXM_").count(), 1, "{def}");
            assert!(text.contains(&format!("#ifdef FXM_{def}\n")), "{def}");
            assert!(text.len() < source.len() / 10, "{def}: {}", text.len());
            // (The same lines as the whole has, but the other materials'.)
            assert_eq!(
                text.matches("#endif").count(),
                shared.matches("#endif").count()
                    + split.blocks[i]
                        .1
                        .iter()
                        .filter(|l| l.trim_start().starts_with("#endif"))
                        .count()
            );
            if let Some(dir) = &dump {
                std::fs::create_dir_all(dir).unwrap();
                std::fs::write(format!("{dir}/FXM_{def}.wgsl"), &text).unwrap();
            }
        }
        // Each one's id is its own.
        let ids: std::collections::HashSet<_> = split
            .blocks
            .iter()
            .map(|(def, _)| def.to_string())
            .collect();
        assert_eq!(ids.len(), split.blocks.len());
        assert_ne!(
            split_handle(WEAPON_SET, 1).id(),
            split_handle(WEAPON_SET, 2).id()
        );
        assert_ne!(split_handle(0, 1).id(), split_handle(1, 1).id());
    }

    /// A distorting material's distortion is a part of its own (the screen's energon
    /// splash's: its multiply, what it adds over 1 moved by its shift, and the shift),
    /// drawn before the rest of it with the blend that moves each place once; the
    /// rest is clipped by its opacity (FX_OPACITY_CLIP) and moves nothing itself.
    #[test]
    fn a_distorting_material_draws_its_distortion_once() {
        let data = FxData::parse(
            "mat 0 FXM_WEAPONS_0 Splat blend=modulate lit=0 twosided=0 scene=0 clip=0.3333 distort=1\n\
             mat 1 FXM_WEAPONS_1 Haze blend=additive lit=0 twosided=0 scene=0 clip=0.3333 distort=1\n\
             mat 2 FXM_WEAPONS_2 Glow blend=additive lit=0 twosided=0 scene=0 clip=0.3333\n",
        );
        let parts = |m: usize| emitter_parts(&data.materials[m]).to_vec();
        assert_eq!(parts(0), [ModPart::Dark, ModPart::Bright, ModPart::Distort]);
        assert_eq!(parts(1), [ModPart::Whole, ModPart::Distort]);
        assert_eq!(parts(2), [ModPart::Whole]);
        assert!(distorts_in(Blend::Modulate, ModPart::Distort));
        assert!(distorts_in(Blend::Modulate, ModPart::Bright));
        assert!(!distorts_in(Blend::Modulate, ModPart::Dark));
        assert!(!distorts_in(Blend::Additive, ModPart::Whole));
        // (Added times what is left of the alpha, which loses the part's share.)
        let b = distort_blend();
        assert_eq!(b.color.src_factor, BlendFactor::DstAlpha);
        assert_eq!(b.color.dst_factor, BlendFactor::One);
        assert_eq!(b.alpha.src_factor, BlendFactor::Zero);
        assert_eq!(b.alpha.dst_factor, BlendFactor::OneMinusSrcAlpha);
    }

    /// An effect out of view is hidden, what it drew left as it was (show_sprites):
    /// one ahead, or just beyond the side (in the margin), is in view; behind, far
    /// to the side or above the top, not.
    #[test]
    fn effects_out_of_view_are_hidden_and_left_unwritten() {
        // A camera at 2 m up, looking down -Z (Bevy's axes), 90 degrees across.
        let transform = GlobalTransform::from(Transform::from_xyz(0.0, 2.0, 0.0));
        let projection = Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio: 16.0 / 9.0,
            ..default()
        });
        let cam = camera_view(&transform, &projection);
        // (A quad a metre wide and tall, standing on `at`: its box.)
        let quad = |at: Vec3| {
            (
                at + Vec3::new(-0.5, 0.0, 0.0),
                at + Vec3::new(0.5, 1.0, 0.0),
            )
        };
        let sees = |anchor: Vec3, at: Vec3| {
            let (lo, hi) = quad(at);
            cam.sees_box(anchor, lo, hi)
        };
        assert!(sees(Vec3::new(0.0, 0.0, -10.0), Vec3::ZERO));
        assert!(sees(Vec3::new(18.0, 0.0, -10.0), Vec3::ZERO));
        assert!(!sees(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO));
        assert!(!sees(Vec3::new(60.0, 0.0, -10.0), Vec3::ZERO));
        assert!(!sees(Vec3::new(0.0, 40.0, -10.0), Vec3::ZERO));
        // Round the camera (a smoke it stands in), in view.
        assert!(sees(Vec3::new(0.0, 2.0, 3.0), Vec3::new(0.0, 0.0, -6.0)));
        let none = SpriteBuffers::default();
        assert!(!cam.sees_box(Vec3::ZERO, none.lo, none.hi));

        // Drawn in view; hidden behind, what it drew as it was; drawn again in view.
        let mut drawn = FxDrawn::new(1);
        let mut visibility = Visibility::Hidden;
        let buffers = |at: Vec3| {
            let (lo, hi) = quad(at);
            let mut b = SpriteBuffers::default();
            let uv = [Vec2::ZERO; 4];
            b.quad(
                [
                    lo,
                    Vec3::new(hi.x, lo.y, lo.z),
                    hi,
                    Vec3::new(lo.x, hi.y, hi.z),
                ],
                Vec3::Z,
                Vec3::X,
                Vec3::NEG_Y,
                uv,
                uv,
                Vec4::ONE,
                Vec4::ZERO,
                Vec4::ZERO,
            );
            b
        };
        let ahead = (&cam, Vec3::new(0.0, 0.0, -10.0));
        assert!(show_sprites(
            &mut drawn,
            buffers(Vec3::ZERO),
            ahead,
            &mut visibility
        ));
        assert!(!drawn.hidden && visibility == Visibility::Inherited);
        assert_eq!(drawn.sprites.vertex_count(), 4);
        let behind = (&cam, Vec3::new(0.0, 0.0, 10.0));
        assert!(!show_sprites(
            &mut drawn,
            buffers(Vec3::X),
            behind,
            &mut visibility
        ));
        assert!(drawn.hidden && visibility == Visibility::Hidden);
        assert_eq!(drawn.sprites.position(0), Vec3::new(-0.5, 0.0, 0.0));
        assert!(show_sprites(
            &mut drawn,
            buffers(Vec3::ZERO),
            ahead,
            &mut visibility
        ));
        assert!(!drawn.hidden && visibility == Visibility::Inherited);
    }

    /// The frame's effect data as the shader reads it (fx_vertex): each slot's
    /// header (where its entity's vertices start, where its indices start, how many
    /// indices), then each shown entity's indices and vertices, a vertex's words in
    /// the shader's order. A hidden entity's header is empty. One with more indices
    /// than its bucket numbers gets a bucket big enough, and its other parts with
    /// it; its slot is given back as it goes.
    #[test]
    fn effects_are_gathered_as_the_shader_reads_them() {
        assert_eq!(bucket_for(0), 0);
        assert_eq!(bucket_for(6), 0);
        assert_eq!(bucket_for(7), 1);
        assert_eq!(bucket_for(12), 1);
        assert_eq!(bucket_for(13), 2);
        assert_eq!(bucket_for(usize::MAX), MAX_BUCKET);

        let mut app = App::new();
        app.init_resource::<FxSlots>()
            .init_resource::<FxWords>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<FxMaterial>>()
            .add_systems(Update, gather_fx_words);
        // Two quads, shown, with another part (a child); one hidden.
        let mut two = SpriteBuffers::default();
        let corners = [Vec3::ZERO, Vec3::X, Vec3::new(1.0, 1.0, 0.0), Vec3::Y];
        let uv0 = [Vec2::new(0.25, 0.5); 4];
        let uv1 = [Vec2::new(0.75, 1.0); 4];
        let (color, param, extra) = (
            Vec4::new(0.1, 0.2, 0.3, 0.4),
            Vec4::new(1.0, 2.0, 3.0, 4.0),
            Vec4::new(5.0, 6.0, 7.0, 8.0),
        );
        for _ in 0..2 {
            two.quad(
                corners,
                Vec3::Z,
                Vec3::X,
                Vec3::NEG_Y,
                uv0,
                uv1,
                color,
                param,
                extra,
            );
        }
        let (a, b) = {
            let mut slots = app.world_mut().resource_mut::<FxSlots>();
            (slots.take(), slots.take())
        };
        let mut shown = FxDrawn::new(a);
        shown.sprites = two;
        shown.hidden = false;
        let shown = app
            .world_mut()
            .spawn((shown, Mesh3d(bucket_handle(0)), Visibility::Inherited))
            .id();
        let part = app
            .world_mut()
            .spawn((Mesh3d(bucket_handle(0)), ChildOf(shown)))
            .id();
        let mut hidden = FxDrawn::new(b);
        hidden.sprites.quad(
            corners,
            Vec3::Z,
            Vec3::X,
            Vec3::NEG_Y,
            uv0,
            uv1,
            color,
            param,
            extra,
        );
        app.world_mut()
            .spawn((hidden, Mesh3d(bucket_handle(0)), Visibility::Hidden));
        app.update();

        let words = app.world().resource::<FxWords>();
        assert!(words.fresh);
        let w = &words.words;
        let header = |slot: u32| &w[slot as usize * HEADER_WORDS..][..3];
        assert_eq!(header(0), [0, 0, 0]);
        assert_eq!(header(b), [0, 0, 0]);
        let [vertices, indices, count] = [header(a)[0], header(a)[1], header(a)[2]];
        assert_eq!(count, 12);
        assert_eq!(indices as usize, 3 * HEADER_WORDS);
        assert_eq!(
            &w[indices as usize..][..12],
            [0, 2, 1, 0, 3, 2, 4, 6, 5, 4, 7, 6]
        );
        assert_eq!(vertices, indices + 12);
        assert_eq!(w.len(), vertices as usize + 8 * VERTEX_WORDS);
        // (Vertex 6: the second quad's third corner, as the shader reads it.)
        let f = |k: usize| f32::from_bits(w[vertices as usize + 6 * VERTEX_WORDS + k]);
        let read: Vec<f32> = (0..VERTEX_WORDS).map(f).collect();
        let tangent = tangent_for(Vec3::Z, Vec3::X, Vec3::NEG_Y);
        let mut want = vec![1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.25, 0.5, 0.75, 1.0];
        want.extend(tangent.to_array());
        want.extend(color.to_array());
        want.extend(param.to_array());
        want.extend(extra.to_array());
        assert_eq!(read, want);

        // Its 12 indices outgrew the smallest bucket: the next numbers 12.
        let world = app.world();
        assert_eq!(world.get::<Mesh3d>(shown).unwrap().0, bucket_handle(1));
        assert_eq!(world.get::<Mesh3d>(part).unwrap().0, bucket_handle(1));
        let bucket = world
            .resource::<Assets<Mesh>>()
            .get(&bucket_handle(1))
            .unwrap();
        let numbers = bucket
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        assert_eq!(numbers.len(), 12);
        assert!(
            numbers
                .iter()
                .enumerate()
                .all(|(i, p)| *p == [i as f32, 0.0, 0.0])
        );

        app.world_mut().entity_mut(shown).despawn();
        assert_eq!(app.world().resource::<FxSlots>().free, [a]);
        assert_eq!(app.world_mut().resource_mut::<FxSlots>().take(), a);
    }

    /// The effect data's buffer grows (doubling, from FX_WORDS_START) when a frame's
    /// data outgrows it: made anew, written each frame (COPY_DST), and the materials
    /// made again then and REMAKE_AFTER frames later, to bind the new one.
    #[test]
    fn the_effect_datas_buffer_grows_with_it() {
        let mut app = App::new();
        app.init_resource::<FxSlots>()
            .init_resource::<FxWords>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<FxMaterial>>()
            .init_resource::<Assets<ShaderStorageBuffer>>()
            .add_systems(Update, gather_fx_words);
        // (A buffer too small for a quad's data.)
        let small = 16;
        app.world_mut()
            .resource_mut::<Assets<ShaderStorageBuffer>>()
            .insert(FX_WORDS_BUFFER.id(), words_buffer(small))
            .unwrap();
        app.world_mut().resource_mut::<FxWords>().capacity = small;
        let slot = app.world_mut().resource_mut::<FxSlots>().take();
        let mut drawn = FxDrawn::new(slot);
        drawn.hidden = false;
        let uv = [Vec2::ZERO; 4];
        drawn.sprites.quad(
            [Vec3::ZERO, Vec3::X, Vec3::ONE, Vec3::Y],
            Vec3::Z,
            Vec3::X,
            Vec3::NEG_Y,
            uv,
            uv,
            Vec4::ONE,
            Vec4::ZERO,
            Vec4::ZERO,
        );
        app.world_mut()
            .spawn((drawn, Mesh3d(bucket_handle(0)), Visibility::Inherited));
        app.update();
        let words = app.world().resource::<FxWords>();
        assert!(words.words.len() > small);
        assert_eq!(words.capacity, FX_WORDS_START);
        assert_eq!(words.since_grown, 1);
        let buffer = app
            .world()
            .resource::<Assets<ShaderStorageBuffer>>()
            .get(FX_WORDS_BUFFER.id())
            .unwrap();
        assert_eq!(buffer.buffer_description.size, FX_WORDS_START as u64 * 4);
        assert!(
            buffer
                .buffer_description
                .usage
                .contains(BufferUsages::STORAGE | BufferUsages::COPY_DST)
        );
        // (Made again REMAKE_AFTER frames after it grew; not grown again.)
        for _ in 1..REMAKE_AFTER {
            app.update();
        }
        let words = app.world().resource::<FxWords>();
        assert_eq!(words.since_grown, REMAKE_AFTER);
        assert_eq!(words.capacity, FX_WORDS_START);
    }

    /// With more effects in view than the Graphics level's number, those further
    /// off than the nearest that many are written every other frame, half of them
    /// each frame; hidden ones (and decals) as they come; with fewer, all.
    #[test]
    fn far_effects_in_a_busy_frame_are_written_every_other_frame() {
        let mut app = App::new();
        app.init_resource::<FxBudget>()
            .insert_resource(FxQuality {
                meshes: 3,
                ..default()
            })
            .add_systems(Update, roll_fx_budget);
        // A frame: effects at 1 to 8 m, the deferred ones' keys and distances.
        let frame = |app: &mut App, hidden: bool| {
            app.update();
            let mut budget = app.world_mut().resource_mut::<FxBudget>();
            let mut deferred = Vec::new();
            for key in 0..8u32 {
                let distance = (key + 1) as f32 * 100.0;
                if budget.defer(distance, hidden, key) {
                    deferred.push(key);
                } else {
                    budget.wrote(distance);
                }
            }
            deferred
        };
        // The first frame knows of none: all written.
        assert!(frame(&mut app, false).is_empty());
        // Then the nearest three and the one at the cut every frame; of the four
        // further, half one frame, the other half the next.
        let a = frame(&mut app, false);
        let b = frame(&mut app, false);
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
        let mut both: Vec<u32> = a.iter().chain(&b).copied().collect();
        both.sort();
        assert_eq!(both, [4, 5, 6, 7]);
        // Hidden ones are written.
        assert!(frame(&mut app, true).is_empty());
        // Fewer in view than the number: all written.
        app.insert_resource(FxQuality {
            meshes: 8,
            ..default()
        });
        assert!(frame(&mut app, false).is_empty());
        assert!(frame(&mut app, false).is_empty());
    }

    /// An effect mesh with one UV set, no tangents and no colours (Oblivion's force
    /// fields) is given all three, the effect shader's vertex data, and so can be
    /// drawn: its first UVs again, tangents along its U and white.
    #[test]
    fn effect_meshes_get_the_vertex_data_the_shader_reads() {
        use bevy::asset::RenderAssetUsages;
        use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![
                [0.0f32, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 4]);
        let uv = vec![[0.0f32, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.clone());
        mesh.insert_indices(Indices::U16(vec![0, 1, 2, 0, 2, 3]));
        let mut meshes = Assets::<Mesh>::default();
        let handle = meshes.add(mesh);
        fill_fx_attributes(&mut meshes, &handle);
        let mesh = meshes.get(&handle).unwrap();
        assert_eq!(
            mesh.attribute(Mesh::ATTRIBUTE_UV_1),
            Some(&VertexAttributeValues::Float32x2(uv))
        );
        let Some(VertexAttributeValues::Float32x4(t)) = mesh.attribute(Mesh::ATTRIBUTE_TANGENT)
        else {
            panic!("no tangents");
        };
        assert!(
            t.iter()
                .all(|t| (t[0] - 1.0).abs() < 1e-4 && t[1].abs() < 1e-4)
        );
        assert_eq!(
            mesh.attribute(Mesh::ATTRIBUTE_COLOR),
            Some(&VertexAttributeValues::Float32x4(vec![[1.0; 4]; 4]))
        );
    }

    /// A placement is lit only where its line says the game lights it (and one
    /// without the field, an older file's, is not); Downfall's burning pillars are,
    /// Oblivion's steam vents are not.
    #[test]
    fn placements_are_lit_as_the_game_lights_them() {
        let data = FxData::parse(
            "system 0 A warmup=0 lit=1\n\
             place 0 0 0 0 1 0 0 0 1 0 0 0 1 1\n\
             place 0 0 0 0 1 0 0 0 1 0 0 0 1 0\n\
             place 0 0 0 0 1 0 0 0 1 0 0 0 1\n",
        );
        let lit: Vec<bool> = data.placements.iter().map(|p| p.lit).collect();
        assert_eq!(lit, [true, false, false]);
        let dir = crate::assets_dir().join("levels");
        for (stem, system, want) in [
            ("downfall", "SmokeFire_Pillar_FX", true),
            ("oblivion", "Steam_Directional_FX", false),
            ("oblivion", "ReloadSteam_Harvester_FX", true),
        ] {
            let Ok(text) = std::fs::read_to_string(dir.join(format!("{stem}_fx.txt"))) else {
                continue;
            };
            let data = FxData::parse(&text);
            let placed: Vec<&Placement> = data
                .placements
                .iter()
                .filter(|p| data.systems.get(p.system).is_some_and(|s| s.name == system))
                .collect();
            assert!(!placed.is_empty(), "{stem}: no {system}");
            assert!(placed.iter().all(|p| p.lit == want), "{stem}: {system}");
        }
    }

    /// A lit effect's lights: one light is all the gathered light, from its way
    /// (none of it from all around); two from opposite sides gathered towards the
    /// brighter, the dimmer one's light from all around; and nothing from beyond a
    /// light's radius.
    #[test]
    fn lit_effects_lights_are_gathered() {
        let one = [(Vec3::splat(2.0), Vec3::Z)];
        let (light, way) = gather_lights(&one);
        assert!((light - Vec3::splat(2.0)).length() < 1e-5);
        assert!((way - Vec3::Z).length() < 1e-5);
        assert!(light_around(&one, way).length() < 1e-5);

        let two = [(Vec3::splat(3.0), Vec3::X), (Vec3::splat(1.0), -Vec3::X)];
        let (light, way) = gather_lights(&two);
        assert!((way - Vec3::X).length() < 1e-5);
        assert!((light - Vec3::splat(3.0)).length() < 1e-5);
        assert!((light_around(&two, way) - Vec3::splat(0.25)).length() < 1e-5);

        let lighting = MapLighting {
            lights: vec![crate::lighting::MapLight {
                position: Vec3::new(0.0, 10.0, 0.0),
                color: Color::WHITE,
                brightness: 5.0,
                radius: 20.0,
                falloff: 2.0,
                spot: None,
                dynamic_only: true,
            }],
            sun: None,
            shafts: None,
            fog: None,
            height_fogs: Vec::new(),
            bloom: None,
            dof: None,
            grade: None,
            reflect: None,
        };
        // (Under it, 4 m down: the light from above, Unreal's +z.)
        let under = light_at(Some(&lighting), Vec3::new(0.0, 0.0, 600.0));
        let falloff = (1.0f32 - 0.2 * 0.2).powi(2);
        assert!((under.light.x - 5.0 * falloff).abs() < 1e-4);
        assert!((under.light_dir.truncate() - Vec3::Z).length() < 1e-5);
        assert!((under.ambient.x - AMBIENT).abs() < 1e-5);
        let far = light_at(Some(&lighting), Vec3::new(0.0, 3000.0, 1000.0));
        assert_eq!(far.light.truncate(), Vec3::ZERO);
    }

    /// A one-shot emitter's particle, born after its delay and living a second: on
    /// past its emitter's end (its delay and duration) as UE3 has it, or gone as it
    /// ends when the emitter kills its own on completing ("killdone=1", the
    /// Intercooler's blast's sphere's).
    /// The robots' dash and landing dust keeps to the game's peaks: a side dash's
    /// (Long_Fall_Landing_1_FX) no more than 20 puffs and 1 sheet on the ground
    /// however many its bursts ask for (0 to 90 of each), a dash's end's
    /// (Long_Fall_Landing_Sm_FX) 3 and 1.
    /// An effect's level of detail by its distance and the Graphics level: the
    /// bias drops it that many levels, but never to one it keeps for further than
    /// effects are drawn on that level. Ignition's flame jets (Flamethrower_FX, its
    /// one lower level from 500 m) burn whole up close on Low and Medium (60 m,
    /// 120 m); an effect with a level from 25 m drops to it.
    #[test]
    fn the_graphics_level_drops_detail_only_to_levels_seen_that_near() {
        let lods = [(2500.0, 7), (10000.0, 8)];
        assert_eq!(lod_level(&lods, 1000.0, 0, None), 0);
        assert_eq!(lod_level(&lods, 3000.0, 0, None), 1);
        assert_eq!(lod_level(&lods, 1000.0, 2, None), 2);
        // Low (60 m): to the 25 m level, not the 100 m one; Medium (120 m): both.
        assert_eq!(lod_level(&lods, 1000.0, 2, Some(6000.0)), 1);
        assert_eq!(lod_level(&lods, 1000.0, 2, Some(12000.0)), 2);
        // Far enough off for a level by itself, it keeps it.
        assert_eq!(lod_level(&lods, 11000.0, 0, Some(6000.0)), 2);
        let path = crate::assets_dir().join("fx/weapons_fx.txt");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("fx/weapons_fx.txt not built: skipped");
            return;
        };
        let data = FxData::parse(&text);
        let flame = data
            .systems
            .iter()
            .find(|s| s.name == "Flamethrower_FX")
            .expect("Flamethrower_FX");
        assert_eq!(flame.lods.len(), 1);
        for drawn in [6000.0, 12000.0] {
            assert_eq!(lod_level(&flame.lods, 2000.0, 2, Some(drawn)), 0);
        }
    }

    #[test]
    fn dash_dust_keeps_to_its_peaks() {
        let path = crate::assets_dir().join("fx/weapons_fx.txt");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("fx/weapons_fx.txt not built: skipped");
            return;
        };
        let data = FxData::parse(&text);
        let frame = SimFrame {
            origin: Vec3::ZERO,
            axes: [Vec3::X, Vec3::Y, Vec3::Z],
            size: 1.0,
            params: &[],
            vectors: &[],
            prev_origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            sources: &[],
        };
        for (name, puffs) in [
            ("Long_Fall_Landing_1_FX", 20),
            ("Long_Fall_Landing_Sm_FX", 3),
        ] {
            let system = data
                .systems
                .iter()
                .find(|s| s.name == name)
                .expect("the system");
            let most = |emitter: &str| {
                let def = system
                    .emitters
                    .iter()
                    .find(|e| e.name == emitter)
                    .expect("the emitter");
                assert!(def.capped, "{name} {emitter}");
                let mut most = 0;
                for seed in 0..20 {
                    let mut e = EmitterInstance::new(def, 0, 0, 0, seed + 1);
                    for _ in 0..30 {
                        e.tick(def, &frame, 1.0 / 60.0, true);
                        most = most.max(e.particles.len());
                    }
                }
                most
            };
            assert_eq!(most("Particle_Emitter_Dup_Export"), puffs, "{name}");
            assert_eq!(most("Particle_Emitter_Dup_Dup_Export"), 1, "{name}");
        }
    }

    /// The Cloak's lightning keeps to the game's peaks: its second sheet of arcs
    /// (ElectricArcs_Dup_Dup, 60 to 120 a second for half a second) no more than 10
    /// at once, its sparks (Sparks_bolts_Dup, 50 a second living up to 0.4 s) 12.
    #[test]
    fn cloak_lightning_keeps_to_its_peaks() {
        let path = crate::assets_dir().join("fx/weapons_fx.txt");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("fx/weapons_fx.txt not built: skipped");
            return;
        };
        let data = FxData::parse(&text);
        let frame = SimFrame {
            origin: Vec3::ZERO,
            axes: [Vec3::X, Vec3::Y, Vec3::Z],
            size: 1.0,
            params: &[],
            vectors: &[],
            prev_origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            sources: &[],
        };
        let system = data
            .systems
            .iter()
            .find(|s| s.name == "Cloaking_FX")
            .expect("the system");
        for (emitter, peak) in [("ElectricArcs_Dup_Dup", 10), ("Sparks_bolts_Dup", 12)] {
            let def = system
                .emitters
                .iter()
                .find(|e| e.name == emitter)
                .expect("the emitter");
            assert!(def.capped, "{emitter}");
            let mut most = 0;
            for seed in 0..20 {
                let mut e = EmitterInstance::new(def, 0, 0, 0, seed + 1);
                for _ in 0..30 {
                    e.tick(def, &frame, 1.0 / 60.0, true);
                    most = most.max(e.particles.len());
                }
            }
            assert_eq!(most, peak, "{emitter}");
        }
    }

    /// A car's boosters (Booster_Jazz_FX) keep to the game's peaks through a long
    /// boost: each of its three cones 2 at once, however long it burns (it piled up
    /// about 15 more of each a second, each one on the last, 150 of each by 10 s).
    #[test]
    fn a_cars_boosters_keep_to_their_peaks() {
        let path = crate::assets_dir().join("fx/weapons_fx.txt");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("fx/weapons_fx.txt not built: skipped");
            return;
        };
        let data = FxData::parse(&text);
        let frame = SimFrame {
            origin: Vec3::ZERO,
            axes: [Vec3::X, Vec3::Y, Vec3::Z],
            size: 1.0,
            params: &[],
            vectors: &[],
            prev_origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            sources: &[],
        };
        let system = data
            .systems
            .iter()
            .find(|s| s.name == "Booster_Jazz_FX")
            .expect("the system");
        assert_eq!(system.emitters.len(), 3);
        for def in &system.emitters {
            assert!(def.capped, "{}", def.name);
            assert_eq!(def.peak, 2, "{}", def.name);
            let mut e = EmitterInstance::new(def, 0, 0, 0, 1);
            let mut most = 0;
            for _ in 0..600 {
                e.tick(def, &frame, 1.0 / 60.0, true);
                most = most.max(e.particles.len());
            }
            assert_eq!(most, 2, "{}", def.name);
            assert_eq!(e.particles.len(), 2, "{}", def.name);
        }
    }

    /// The Scatter Blaster's own muzzle flash keeps to the game's peaks as its other
    /// effects do: its smoke in front (SmokeFront, a burst of 12 to 15) no more than
    /// 5 at once, its sparks (SparksSM, 5 to 30) 30; the other guns' flashes,
    /// uncapped, let their bursts out whole (the Nucleon Charge Rifle's smoke in
    /// front, 12 to 15).
    #[test]
    fn the_scatter_blasters_flash_keeps_to_its_peaks() {
        let path = crate::assets_dir().join("fx/weapons_fx.txt");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("fx/weapons_fx.txt not built: skipped");
            return;
        };
        let data = FxData::parse(&text);
        let frame = SimFrame {
            origin: Vec3::ZERO,
            axes: [Vec3::X, Vec3::Y, Vec3::Z],
            size: 1.0,
            params: &[],
            vectors: &[],
            prev_origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            sources: &[],
        };
        for (system, emitter, capped, (low, high)) in [
            ("Scatterblaster_MuzzleFlash_FX", "SmokeFront", true, (5, 5)),
            ("Scatterblaster_MuzzleFlash_FX", "SparksSM", true, (5, 30)),
            (
                "NucleonRifle_MuzzleFlash_FX",
                "SmokeFront_Export",
                false,
                (12, 15),
            ),
        ] {
            let def = data
                .systems
                .iter()
                .find(|s| s.name == system)
                .and_then(|s| s.emitters.iter().find(|e| e.name == emitter))
                .expect("the emitter");
            assert_eq!(def.capped, capped, "{emitter}");
            for seed in 0..20 {
                // (The most out at once over its first sixth of a second: the
                // sparks start a thirtieth of a second in, their EmitterDelay.)
                let mut e = EmitterInstance::new(def, 0, 0, 0, seed + 1);
                let mut n = 0;
                for _ in 0..10 {
                    e.tick(def, &frame, 1.0 / 60.0, true);
                    n = n.max(e.particles.len());
                }
                assert!((low..=high).contains(&n), "{system} {emitter}: {n}");
            }
        }
    }

    /// The Photon Burst Rifle's streaks (Streak_horz_mat) draw their own texture,
    /// FX_OS_Common_Textures_p's Streak_CLR, a thin line across a 512 by 256 image,
    /// not the broad 256 by 64 bar of the same name the tank's tracer draws
    /// (Tank_Tracer_MAT: FX_Textures_p's), which the flash drew as long, fat white
    /// bars (tools/levelfx.py's texture_stems).
    #[test]
    fn the_photon_streak_draws_its_own_texture() {
        let path = crate::assets_dir().join("fx/weapons_fx.txt");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("fx/weapons_fx.txt not built: skipped");
            return;
        };
        let data = FxData::parse(&text);
        let texture = |name: &str| {
            let m = data
                .materials
                .iter()
                .find(|m| m.name == name)
                .expect("the material");
            m.textures[0].0.clone()
        };
        let size = |file: &str| {
            let b = std::fs::read(crate::assets_dir().join(file)).expect("the texture");
            let u = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
            (u(20), u(24))
        };
        let (streak, tracer) = (texture("Streak_horz_mat"), texture("Tank_Tracer_MAT"));
        assert_ne!(streak, tracer);
        assert_eq!(size(&streak), (512, 256), "{streak}");
        assert_eq!(size(&tracer), (256, 64), "{tracer}");
    }

    #[test]
    fn kill_on_completed() {
        for (kill, alive_after) in [("", true), (" killdone=1", false)] {
            let text = format!(
                "system 0 T warmup=0 lit=0\nemitter E\n material 0\n required align=0 local=1 \
                 duration=0.2 loops=1 delay=0.4{kill}\n rate 1 1 1 0\n burst 1 1 0\nmodule 0 1 1 0\n \
                 f 1 1 1 1\n spawn 0\n update\nend\n"
            );
            let data = FxData::parse(&text);
            let def = &data.systems[0].emitters[0];
            assert_eq!(def.kill_on_completed, !kill.is_empty());
            let mut e = EmitterInstance::new(def, 0, 0, 0, 1);
            let frame = SimFrame {
                origin: Vec3::ZERO,
                axes: [Vec3::X, Vec3::Y, Vec3::Z],
                size: 1.0,
                params: &[],
                vectors: &[],
                prev_origin: Vec3::ZERO,
                velocity: Vec3::ZERO,
                sources: &[],
            };
            let dt = 1.0 / 60.0;
            let mut counts = Vec::new();
            for _ in 0..48 {
                e.tick(def, &frame, dt, true);
                counts.push(e.particles.len());
            }
            // Born at 0.4 s, and still there at 0.55 s either way...
            assert_eq!(counts[22], 0, "{counts:?}");
            assert_eq!(counts[32], 1, "{counts:?}");
            // ...and at 0.75 s only if it lives out its life.
            assert_eq!(counts[44], usize::from(alive_after), "{counts:?}");
        }
    }

    /// The Snipers' and Sharpshooters' tracer, Tracer_Sniper_AI_FX: its Bolt, which the
    /// game doesn't draw (its material has no shaders for it), is run all the same,
    /// two particles flying out along the shot at 450 m/s; its two lightning
    /// emitters' bolts ride the Bolt's (LocationEmitterDirect: where it is, as fast,
    /// so they lie along the shot, 10 m and 25 to 30 m long), drawn, and all go as
    /// their 0.1 s is up (bKillOnCompleted), as in the game's videos.
    #[test]
    fn the_snipers_tracer_rides_its_bolt() {
        let path = crate::assets_dir().join("fx/weapons_fx.txt");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("fx/weapons_fx.txt not built: skipped");
            return;
        };
        let data = FxData::parse(&text);
        let system = data
            .systems
            .iter()
            .find(|s| s.name == "Tracer_Sniper_AI_FX")
            .expect("the tracer");
        let names: Vec<&str> = system.emitters.iter().map(|e| e.name.as_str()).collect();
        let bolt = names.iter().position(|n| *n == "Bolt").expect("its Bolt");
        assert!(system.emitters[bolt].undrawn && system.emitters[bolt].is_source);
        let lightning: Vec<usize> = (0..names.len())
            .filter(|&i| names[i].starts_with("Bolt_Dup"))
            .collect();
        assert_eq!(lightning.len(), 2);
        let mut sims: Vec<EmitterInstance> = system
            .emitters
            .iter()
            .enumerate()
            .map(|(i, def)| EmitterInstance::new(def, 0, i, 0, 5))
            .collect();
        let dt = 1.0 / 60.0;
        let mut seen_moving = false;
        for frame_no in 0..12 {
            let mut sources: Vec<Source> = Vec::new();
            for (i, def) in system.emitters.iter().enumerate() {
                let frame = SimFrame {
                    origin: Vec3::ZERO,
                    axes: [Vec3::X, Vec3::Y, Vec3::Z],
                    size: 1.0,
                    params: &[],
                    vectors: &[],
                    prev_origin: Vec3::ZERO,
                    velocity: Vec3::ZERO,
                    sources: &sources,
                };
                sims[i].tick(def, &frame, dt, true);
                if def.is_source {
                    let ps = &sims[i].particles;
                    sources.push(Source {
                        name: def.name.clone(),
                        local: def.local,
                        particles: ps
                            .iter()
                            .map(|p| (p.location, p.velocity, p.rotation))
                            .collect(),
                        times: ps.iter().map(|p| p.relative_time).collect(),
                        beam: false,
                    });
                }
            }
            let t = (frame_no + 1) as f32 * dt;
            if t < 0.09 {
                // The Bolt flies out along X at 450 m/s...
                let b = &sims[bolt].particles;
                assert_eq!(b.len(), 2, "at {t}");
                assert!((b[0].velocity - Vec3::X * 45000.0).length() < 1.0);
                assert!((b[0].location.x - 45000.0 * t).abs() < 45000.0 * dt * 1.5);
                // ...and each lightning emitter's first bolt with it, along the shot.
                for &l in &lightning {
                    let p = &sims[l].particles[0];
                    assert!((p.velocity - b[0].velocity).length() < 1.0, "{}", names[l]);
                    assert!(
                        (p.location - b[0].location).length() <= 45000.0 * dt + 1.0,
                        "{}: {} from its Bolt's {}",
                        names[l],
                        p.location,
                        b[0].location
                    );
                    assert!(!system.emitters[l].undrawn);
                    // (10 m long, and 25 to 30 m: Bolt_Dup's and Bolt_Dup_Export's.)
                    assert!(p.size.y >= 1000.0 && p.size.y <= 3000.0, "{}", p.size);
                }
                seen_moving = true;
            } else if t > 0.11 {
                for i in std::iter::once(bolt).chain(lightning.iter().copied()) {
                    assert!(sims[i].particles.is_empty(), "{} at {t}", names[i]);
                }
            }
        }
        assert!(seen_moving);
    }

    /// LocationEmitterDirect: each particle takes the source's particle of its own
    /// place in the list (where it is, how fast, how old); one with none (its source
    /// has fewer out) is left as it is.
    #[test]
    fn location_emitter_direct_follows_its_sources_particles() {
        let m = ModuleDef {
            kind: LOCATION_EMITTER_DIRECT,
            floats: Vec::new(),
            vectors: Vec::new(),
            words: vec![0, 1, 0],
            name: Some("Lead".into()),
        };
        let def = EmitterDef::default();
        let sources = [Source {
            name: "Lead".into(),
            local: false,
            particles: vec![(Vec3::new(10.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.0)],
            times: vec![0.25],
            beam: false,
        }];
        let frame = SimFrame {
            origin: Vec3::ZERO,
            axes: [Vec3::X, Vec3::Y, Vec3::Z],
            size: 1.0,
            params: &[],
            vectors: &[],
            prev_origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            sources: &sources,
        };
        let mut p = Particle::default();
        EmitterInstance::location_emitter_direct(&def, &m, &frame, &mut p, 0);
        assert_eq!(p.location, Vec3::new(10.0, 0.0, 0.0));
        assert_eq!(p.velocity, Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(p.relative_time, 0.25);
        let mut q = Particle {
            location: Vec3::ONE,
            ..default()
        };
        EmitterInstance::location_emitter_direct(&def, &m, &frame, &mut q, 1);
        assert_eq!(q.location, Vec3::ONE);
        assert_eq!(q.velocity, Vec3::ZERO);
    }

    /// The Chaos-Rift Combustor's muzzle flash: its cap and core (mesh emitters with a
    /// burst of one and no SpawnRate of their own, the engine's default 0) flash once,
    /// a sixth of a second, as the game's video shows; not on for their emitters'
    /// whole second, as they were drawn when a missing SpawnRate was taken for 10 a
    /// second (a cream-white cloud over the gun after every shot).
    #[test]
    fn the_combustors_muzzle_cap_flashes_once() {
        let path = crate::assets_dir().join("fx/weapons_fx.txt");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("fx/weapons_fx.txt not built: skipped");
            return;
        };
        let data = FxData::parse(&text);
        let frame = SimFrame {
            origin: Vec3::ZERO,
            axes: [Vec3::X, Vec3::Y, Vec3::Z],
            size: 1.0,
            params: &[],
            vectors: &[],
            prev_origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            sources: &[],
        };
        let system = data
            .systems
            .iter()
            .find(|s| s.name == "ClusterCannon_MuzzleFlash_FX")
            .expect("the muzzle flash");
        for name in ["MuzzleCap", "MuzzleCore"] {
            let def = system
                .emitters
                .iter()
                .find(|e| e.name == name)
                .expect("the emitter");
            let mut e = EmitterInstance::new(def, 0, 0, 0, 7);
            let mut counts = Vec::new();
            for _ in 0..72 {
                e.tick(def, &frame, 1.0 / 60.0, true);
                counts.push(e.particles.len());
            }
            assert_eq!(counts[2], 1, "{name}: {counts:?}");
            assert!(counts.iter().all(|&n| n <= 1), "{name}: {counts:?}");
            assert!(counts[15..].iter().all(|&n| n == 0), "{name}: {counts:?}");
        }
    }

    /// A spawned effect's emitter taken out of play is hidden at once and no longer
    /// run, and despawned RETIRED_FRAMES frames later (not in the frame it goes).
    #[test]
    fn a_retired_emitter_is_hidden_then_despawned() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let entity = world.spawn(Visibility::Visible).id();
        world
            .run_system_once(
                move |mut commands: Commands, mut v: Query<&mut Visibility>| {
                    retire(&mut commands, entity, &mut v.get_mut(entity).unwrap());
                },
            )
            .unwrap();
        assert_eq!(world.get::<Visibility>(entity), Some(&Visibility::Hidden));
        assert!(world.get::<SpawnedEmitter>(entity).is_none());
        for _ in 1..RETIRED_FRAMES {
            world.run_system_once(despawn_retired).unwrap();
            assert!(world.get_entity(entity).is_ok());
        }
        world.run_system_once(despawn_retired).unwrap();
        assert!(world.get_entity(entity).is_err());
    }

    /// An emitter without a DynamicParameter module gives a material that reads
    /// dynamic parameters 1s; one with the module gives the module's values. (The
    /// game draws such emitters: see tools/ue3/shadercache.py. TFOC.exe's fills read
    /// the parameters at the emitter's offset for the module with no check for one,
    /// which without the module is the start of each particle's own data, its
    /// OldLocation and RelativeTime. Taken so, at the maps' world positions in the
    /// thousands, the Riot Cannon's and the heavies' muzzle smoke, Clouds_Anim_MAT,
    /// whose colour the parameters lerp, came out as screen-filling yellow and white
    /// smears that no video of the game shows; with 1s it is the even purple haze
    /// the videos show.)
    #[test]
    fn dynamic_parameters_without_a_module_are_1() {
        for (module, own) in [
            ("", false),
            (
                "module 2 29 1 0\n f 1 1 1 0.25\n f 1 1 1 0.5\n f 1 1 1 0.75\n f 1 1 1 2\n w 0 0 0 0 0 0 0 0\n",
                true,
            ),
        ] {
            let spawn = if own { "0 1 2" } else { "0 1" };
            let text = format!(
                "system 0 T warmup=0 lit=0\nemitter E\n material 0\n required align=0 local=0 \
                 duration=1 loops=1\n rate 1 1 1 0\n burst 1 1 0\nmodule 0 1 1 0\n \
                 f 1 1 1 2\nmodule 1 23 1 0\n f 1 1 1 0\n v 7 1 3 600 0 0\n{module} spawn {spawn}\n update\nend\n"
            );
            let data = FxData::parse(&text);
            let def = &data.systems[0].emitters[0];
            let mut e = EmitterInstance::new(def, 0, 0, 0, 1);
            let frame = SimFrame {
                origin: Vec3::new(1000.0, -2000.0, 300.0),
                axes: [Vec3::X, Vec3::Y, Vec3::Z],
                size: 1.0,
                params: &[],
                vectors: &[],
                prev_origin: Vec3::new(1000.0, -2000.0, 300.0),
                velocity: Vec3::ZERO,
                sources: &[],
            };
            let dt = 0.1;
            for _ in 0..3 {
                e.tick(def, &frame, dt, true);
            }
            let p = &e.particles[0];
            if own {
                assert_eq!(p.dyn_params, [0.25, 0.5, 0.75, 2.0]);
            } else {
                // (Far out in the world and moving: still 1s.)
                assert!(p.location.y < -1999.0, "{:?}", p.location);
                assert_eq!(p.dyn_params, [1.0; 4]);
            }
        }
    }

    /// A sprite locked to Z (EPAL_Z) lies flat in the world's axes from an emitter
    /// in the world's space however its frame is turned (a car's landing dust from
    /// a socket pitched up 30 degrees), and in its frame's from a local-space one.
    #[test]
    fn axis_locks_follow_the_emitters_space() {
        let (s, c) = 30f32.to_radians().sin_cos();
        // (Pitched up 30 degrees: X forward and up, Z up and back.)
        let axes = [Vec3::new(c, 0.0, s), Vec3::Y, Vec3::new(-s, 0.0, c)];
        for local in [0, 1] {
            let text = format!(
                "system 0 T warmup=0 lit=0\nemitter E\n material 0\n required align=0 local={local} \
                 lock=3 duration=1 loops=1\n rate 1 1 1 0\n burst 1 1 0\nmodule 0 1 1 0\n \
                 f 1 1 1 1\nmodule 1 6 1 0\n v 7 1 3 100 100 100\n spawn 0 1\n update\nend\n"
            );
            let data = FxData::parse(&text);
            let def = &data.systems[0].emitters[0];
            assert_eq!(def.lock, 3);
            let mut e = EmitterInstance::new(def, 0, 0, 0, 1);
            let frame = SimFrame {
                origin: Vec3::ZERO,
                axes,
                size: 1.0,
                params: &[],
                vectors: &[],
                prev_origin: Vec3::ZERO,
                velocity: Vec3::ZERO,
                sources: &[],
            };
            e.tick(def, &frame, 1.0 / 60.0, true);
            assert_eq!(e.particles.len(), 1);
            let cam = CameraView {
                pos: Vec3::new(-500.0, 0.0, 200.0),
                right: Vec3::Y,
                up: Vec3::Z,
                forward: Vec3::X,
                tan_half: Vec2::ONE,
                near: 10.0,
                far: f32::MAX,
            };
            let quad = sprites(def, &e, &frame, &cam, Vec3::ZERO);
            assert_eq!(quad.vertex_count(), 4);
            // (Bevy's y is up.)
            let heights: Vec<f32> = (0..4).map(|i| quad.position(i).y).collect();
            let spread = heights.iter().copied().fold(f32::MIN, f32::max)
                - heights.iter().copied().fold(f32::MAX, f32::min);
            if local == 0 {
                assert!(spread < 1e-3, "world space: flat, {heights:?}");
            } else {
                // (A metre square tilted 30 degrees: half a metre from edge to edge.)
                assert!(
                    (spread - 0.5).abs() < 0.01,
                    "local space: tilted, {heights:?}"
                );
            }
        }
    }
}
