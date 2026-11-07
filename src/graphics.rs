//! The Graphics settings' Graphics row: the game's GraphicPreference (Low, Medium,
//! High and Extra, SettingsMenu_GFX's GraphicsSettings), which sets the demo's
//! graphics as a group (`LEVELS`):
//!
//! * the sun's shadows: its shadow map's size, its cascades and how far they reach;
//! * bloom, depth of field and the vehicles' motion blur;
//! * anti-aliasing (none, FXAA or 4x MSAA) on every camera drawing to the window,
//!   as cameras sharing a window must agree;
//! * the live lights, those the shots, blasts, burning deaths and glowing rounds
//!   make (`LiveLight`): all of them, the nearest few or none;
//! * the effects' detail (UE3's ParticleLODBias: their less detailed levels used
//!   nearer, levelfx.rs's FxQuality), how far away they are drawn, and how many
//!   are moved every frame in a busy fight (the rest, further off, every other
//!   frame: levelfx.rs's FxBudget);
//! * the characters' shadows from the lights other than the sun (shadows.rs);
//! * texture detail and filtering, which, as in the game, take effect when the demo
//!   starts again ("Game Restart Required"): the mips skipped (the game's
//!   PCSettingManager LOD biases, its Low 2 and 1, Medium 1 and 0, High and Extra
//!   0, the maps' and effects' textures by the first, the characters', weapons'
//!   and props' by its "priority" bias) and the anisotropic filtering.
//!
//! The Graphics menu's Render Scale (the demo's own) is here too: the game camera
//! drawn at that share of the window's size each way (50 to 100%) and spread over
//! the window under the HUD, which stays sharp at the window's own size.
//!
//! High is the demo's look before there was a choice (and the characters' new
//! shadows; in a fight with more effects about than its number, the further ones
//! move every other frame), and the default. Extra adds sharper filtering on the maps, the sun's
//! shadows further out and more characters' shadows; Medium and Low take away,
//! step by step, what costs most.
//! The choice is kept in settings.txt as `graphics_quality` (0 to 3).

use crate::dof::GameDof;
use crate::lighting::{MapLighting, MapSun, game_bloom};
use bevy::anti_alias::fxaa::Fxaa;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::gltf::DefaultGltfImageSampler;
use bevy::image::ImageSamplerDescriptor;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, DirectionalLightShadowMap};
use bevy::post_process::bloom::Bloom;
use bevy::post_process::motion_blur::MotionBlur;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::settings::{WgpuFeatures, WgpuSettings};
use bevy::render::view::{Hdr, Msaa};
use bevy::window::{PrimaryWindow, WindowRef};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The levels, in the stepper's order.
pub const LEVEL_COUNT: usize = 4;
/// High: the default (the demo's look before the setting).
pub const HIGH: usize = 2;

/// Anti-aliasing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Aa {
    Off,
    Fxaa,
    Msaa4,
}

/// What a level sets.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Level {
    /// The sun's shadow map (texels each way, each cascade), its cascades, the
    /// first one's far end and how far the last reaches (m).
    pub shadow_map: usize,
    pub cascades: usize,
    pub first_cascade: f32,
    pub shadow_distance: f32,
    pub bloom: bool,
    pub dof: bool,
    pub motion_blur: bool,
    pub aa: Aa,
    /// The most live lights lit at once, the nearest the camera (None: all).
    pub live_lights: Option<usize>,
    /// Levels of detail the spawned effects drop by (ParticleLODBias).
    pub particle_lod_bias: usize,
    /// How far from the camera effects are drawn (m; None: however far).
    pub effect_distance: Option<f32>,
    /// With more effects in view than this, those further off than the nearest
    /// this many are moved (their meshes written) every other frame (None: all
    /// every frame).
    pub effect_meshes: Option<usize>,
    /// The most characters casting shadows from the lights other than the sun.
    pub character_shadows: usize,
    /// Mips skipped: the maps' and effects' textures, the others' (the game's
    /// LODBias and PriorityLODBias). Taken as the demo starts.
    pub texture_lod_bias: u32,
    pub priority_lod_bias: u32,
    /// Anisotropic filtering, the maps' and the effects' (the most samples a pixel
    /// takes). Taken as the demo starts.
    pub map_anisotropy: u16,
    pub fx_anisotropy: u16,
}

/// Low, Medium, High and Extra.
pub const LEVELS: [Level; LEVEL_COUNT] = [
    Level {
        shadow_map: 1024,
        cascades: 2,
        first_cascade: 12.0,
        shadow_distance: 60.0,
        bloom: false,
        dof: false,
        motion_blur: false,
        aa: Aa::Off,
        live_lights: Some(0),
        particle_lod_bias: 2,
        effect_distance: Some(60.0),
        effect_meshes: Some(40),
        character_shadows: 0,
        texture_lod_bias: 2,
        priority_lod_bias: 1,
        map_anisotropy: 1,
        fx_anisotropy: 4,
    },
    Level {
        shadow_map: 2048,
        cascades: 3,
        first_cascade: 15.0,
        shadow_distance: 120.0,
        bloom: true,
        dof: false,
        motion_blur: false,
        aa: Aa::Fxaa,
        live_lights: Some(6),
        particle_lod_bias: 1,
        effect_distance: Some(120.0),
        effect_meshes: Some(70),
        character_shadows: 3,
        texture_lod_bias: 1,
        priority_lod_bias: 0,
        map_anisotropy: 2,
        fx_anisotropy: 8,
    },
    Level {
        shadow_map: 4096,
        cascades: 4,
        first_cascade: 18.0,
        shadow_distance: 220.0,
        bloom: true,
        dof: true,
        motion_blur: true,
        aa: Aa::Msaa4,
        live_lights: None,
        particle_lod_bias: 0,
        effect_distance: None,
        effect_meshes: Some(100),
        character_shadows: 6,
        texture_lod_bias: 0,
        priority_lod_bias: 0,
        map_anisotropy: 4,
        fx_anisotropy: 16,
    },
    Level {
        shadow_map: 4096,
        cascades: 4,
        first_cascade: 20.0,
        shadow_distance: 320.0,
        bloom: true,
        dof: true,
        motion_blur: true,
        aa: Aa::Msaa4,
        live_lights: None,
        particle_lod_bias: 0,
        effect_distance: None,
        effect_meshes: None,
        character_shadows: 10,
        texture_lod_bias: 0,
        priority_lod_bias: 0,
        map_anisotropy: 16,
        fx_anisotropy: 16,
    },
];

/// The Graphics row's choice: an index into `LEVELS`.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub struct GraphicsQuality(pub usize);

impl GraphicsQuality {
    pub fn level(self) -> &'static Level {
        &LEVELS[self.0.min(LEVEL_COUNT - 1)]
    }

    /// Its line in settings.txt.
    pub fn setting(self) -> (String, String) {
        ("graphics_quality".to_string(), self.0.to_string())
    }
}

impl Default for GraphicsQuality {
    fn default() -> Self {
        let saved = crate::menu::saved_setting("graphics_quality").and_then(|v| v.parse().ok());
        GraphicsQuality(saved.filter(|&n| n < LEVEL_COUNT).unwrap_or(HIGH))
    }
}

/// The level the demo started at: its textures' detail and filtering, which a
/// change waits for the demo to start again to take.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub struct StartQuality(pub GraphicsQuality);

/// The same, for the models' loaders (set as the plugin is built).
static START: AtomicUsize = AtomicUsize::new(HIGH);

/// The level the demo started at.
pub fn start_level() -> &'static Level {
    GraphicsQuality(START.load(Ordering::Relaxed)).level()
}

/// The maps' textures' sampler: filtered as the level the demo started at filters
/// them, skipping its mips.
pub fn map_sampler() -> ImageSamplerDescriptor {
    let level = start_level();
    let mut sampler = ImageSamplerDescriptor::linear();
    sampler.set_anisotropic_filter(level.map_anisotropy);
    sampler.lod_min_clamp = level.texture_lod_bias as f32;
    sampler
}

/// The renderer's settings (main's RenderPlugin): Bevy's, but without buffer
/// binding arrays, so that the standard materials are not bindless.
///
/// Bindless, Bevy keeps the standard materials (the maps', the models') in a few
/// big shared groups, up to 2048 textures and samplers each, a bind group each
/// that the shader indexes into. But each time a pass binds such a group, wgpu goes
/// over every texture in it (to track how each is used), 40 to 70 µs for the maps'
/// groups, and the passes bind them again whenever another kind of material comes
/// between: in the transparent pass, at every one of the maps' see-through
/// surfaces among the effects. In a big wave on Oblivion that was some 65 binds a
/// view each frame, about 2.5 ms of the render thread's frame for every view. Not
/// bindless, each material has a bind group of its own, of its own few textures,
/// about a microsecond to bind. The light maps keep their binding arrays (they
/// need texture arrays only), and so do Bevy's other uses of them.
pub fn wgpu_settings() -> WgpuSettings {
    WgpuSettings {
        disabled_features: Some(WgpuFeatures::BUFFER_BINDING_ARRAY),
        ..default()
    }
}

/// Whether going from the level the demo started at to `to` changes what only
/// takes effect as it starts again (its textures).
pub fn needs_restart(start: GraphicsQuality, to: GraphicsQuality) -> bool {
    let (a, b) = (start.level(), to.level());
    (
        a.texture_lod_bias,
        a.priority_lod_bias,
        a.map_anisotropy,
        a.fx_anisotropy,
    ) != (
        b.texture_lod_bias,
        b.priority_lod_bias,
        b.map_anisotropy,
        b.fx_anisotropy,
    )
}

/// A light that comes and goes with what happens (a shot's flash, a blast, a
/// burning death, a glowing round): the Graphics level's `live_lights` limits
/// how many are lit.
#[derive(Component)]
pub struct LiveLight;

/// A live light the level has put out (lit again when it allows it).
#[derive(Component)]
struct PutOut;

/// The camera's motion blur (its shutter opened by camera_follow while a car or
/// truck boosts: the driving camera's): sixteen samples along each pixel's way
/// (the game's five, MBSC_5_Samples, smear the spinning wheels into speckles here).
pub fn camera_motion_blur() -> MotionBlur {
    MotionBlur {
        shutter_angle: 0.0,
        samples: 8,
    }
}

/// The Graphics menu's Render Scale: the share of the window's size each way the
/// game camera is drawn at (RENDER_SCALES), kept in settings.txt as
/// `render_scale` (100% until it is set).
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct RenderScale(pub f32);

/// Its range and step.
pub const RENDER_SCALES: (f32, f32, f32) = (0.5, 1.0, 0.05);

impl RenderScale {
    pub fn setting(self) -> (String, String) {
        ("render_scale".to_string(), format!("{:.2}", self.0))
    }
}

impl Default for RenderScale {
    fn default() -> Self {
        let saved = crate::menu::saved_setting("render_scale").and_then(|v| v.parse::<f32>().ok());
        let (min, max, _) = RENDER_SCALES;
        RenderScale(saved.map_or(max, |v| v.clamp(min, max)))
    }
}

/// The render layer of the picture the scaled game camera draws, spread over the
/// window (no other camera draws it).
const SCALED_LAYER: usize = 9;

/// The scaled game camera's picture and what shows it in the window: its camera
/// and its sprite.
#[derive(Resource, Default)]
struct ScaledView {
    image: Option<Handle<Image>>,
    size: UVec2,
    camera: Option<Entity>,
    sprite: Option<Entity>,
}

pub struct GraphicsPlugin;

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        let quality = GraphicsQuality::default();
        START.store(quality.0, Ordering::Relaxed);
        app.insert_resource(quality)
            .insert_resource(StartQuality(quality))
            .insert_resource(DirectionalLightShadowMap {
                size: quality.level().shadow_map,
            })
            .init_resource::<RenderScale>()
            .init_resource::<ScaledView>()
            .add_systems(Startup, set_texture_detail)
            .add_systems(
                PostUpdate,
                (apply_render_scale, refresh_retargeted)
                    .chain()
                    .before(apply_anti_aliasing)
                    .before(bevy::camera::CameraUpdateSystems),
            )
            .add_systems(
                PostUpdate,
                (
                    apply_sun_shadows,
                    apply_camera_effects,
                    apply_anti_aliasing,
                    limit_live_lights,
                    crate::levelfx::set_fx_quality,
                )
                    .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
            );
        if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
            render_app.add_systems(
                bevy::render::ExtractSchedule,
                keep_meshes_drawable.after(bevy::pbr::late_sweep_material_instances),
            );
        }
    }
}

/// Every mesh with a material that Bevy will draw this frame is in its table of
/// when each mesh's pipelines were last set up (EntitySpecializationTicks), as
/// Bevy's drawing expects: it stops the demo (a panic in
/// specialize_material_meshes, specialize_prepass_material_meshes or
/// specialize_shadows: "called `Option::unwrap()` on a `None` value") on one that
/// isn't. Bevy notes the meshes that came in or changed material in PostUpdate, in
/// a system the demo's own PostUpdate systems aren't ordered against: a mesh the
/// demo made there after it looked (a Guard's scanner beam, made as the Guard comes
/// in, scanning: ai/scanner.rs's draw_beams) was drawn that frame with no entry,
/// and the demo stopped (the joined players' demos closing in Ancients' waves 7
/// and 8, the Guards' first). Here, after Bevy's own bookkeeping, a mesh left out
/// is put in, its pipelines to be set up this frame (as Bevy does them the frame
/// after, once it has seen it).
fn keep_meshes_drawable(
    instances: Res<bevy::pbr::RenderMaterialInstances>,
    mut ticks: ResMut<bevy::pbr::EntitySpecializationTicks>,
    this: bevy::ecs::system::SystemChangeTick,
) {
    // (This frame's tick of the material instances': late_sweep_material_instances
    // has moved it on for the next.)
    let frame =
        bevy::ecs::change_detection::Tick::new(instances.current_change_tick.get().wrapping_sub(1));
    let mut filled = 0;
    for entity in instances.instances.keys() {
        if !ticks.entities.contains_key(entity) {
            ticks.entities.insert(
                *entity,
                bevy::pbr::EntitySpecializationTickPair {
                    system_tick: this.this_run(),
                    material_instances_tick: frame,
                },
            );
            filled += 1;
        }
    }
    if filled > 0 {
        debug!("meshes put back in Bevy's specialization table: {filled}");
    }
}

/// The models' textures (every glTF's but the maps', which have their own
/// sampler: map_sampler) skip the level's priority mips.
fn set_texture_detail(start: Res<StartQuality>, sampler: Option<Res<DefaultGltfImageSampler>>) {
    let bias = start.0.level().priority_lod_bias;
    if let (Some(sampler), true) = (sampler, bias > 0) {
        let mut descriptor = ImageSamplerDescriptor::linear();
        descriptor.lod_min_clamp = bias as f32;
        sampler.set(&descriptor);
    }
}

/// The map's sun's shadows as the level has them (its shadow map, its cascades),
/// when the level changes or a map's sun comes in.
fn apply_sun_shadows(
    quality: Res<GraphicsQuality>,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
    mut suns: Query<(Ref<MapSun>, &mut CascadeShadowConfig)>,
) {
    let level = quality.level();
    if quality.is_changed() && shadow_map.size != level.shadow_map {
        shadow_map.size = level.shadow_map;
    }
    for (sun, mut config) in &mut suns {
        if quality.is_changed() || sun.is_added() {
            *config = sun_cascades(level);
        }
    }
}

/// The sun's cascades at `level`.
pub fn sun_cascades(level: &Level) -> CascadeShadowConfig {
    CascadeShadowConfigBuilder {
        num_cascades: level.cascades,
        first_cascade_far_bound: level.first_cascade,
        maximum_distance: level.shadow_distance,
        ..default()
    }
    .build()
}

/// The game camera's bloom, depth of field and motion blur, on or off as the
/// level has them (the bloom and depth of field as the map in play has them:
/// MapLighting::camera_effects puts them on with the map).
#[allow(clippy::type_complexity)]
fn apply_camera_effects(
    mut commands: Commands,
    quality: Res<GraphicsQuality>,
    map: Option<Res<MapLighting>>,
    cameras: Query<(Entity, Has<Bloom>, Has<GameDof>, Has<MotionBlur>), With<crate::OrbitCamera>>,
) {
    let level = quality.level();
    let changed = quality.is_changed() || map.as_ref().is_some_and(|m| m.is_changed());
    for (entity, bloom, dof, blur) in &cameras {
        let mut camera = commands.entity(entity);
        match (level.bloom, bloom) {
            (false, true) => {
                camera.remove::<Bloom>();
            }
            (true, false) if changed => {
                if let Some(scale) = map.as_ref().and_then(|m| m.bloom) {
                    camera.insert(game_bloom(scale));
                }
            }
            _ => {}
        }
        match (level.dof, dof) {
            (false, true) => {
                camera.remove::<GameDof>();
            }
            (true, false) if changed => {
                if let Some(d) = map.as_ref().and_then(|m| m.dof) {
                    camera.insert(d);
                }
            }
            _ => {}
        }
        match (level.motion_blur, blur) {
            (false, true) => {
                camera.remove::<MotionBlur>();
            }
            (true, false) => {
                camera.insert(camera_motion_blur());
            }
            _ => {}
        }
    }
}

/// The level's anti-aliasing on every camera drawing to the window (their
/// multisampling must agree: they draw into the same picture), FXAA on the game
/// camera; a camera that comes in takes it too.
#[allow(clippy::type_complexity)]
fn apply_anti_aliasing(
    mut commands: Commands,
    quality: Res<GraphicsQuality>,
    mut cameras: Query<(
        Entity,
        Ref<Camera>,
        Option<&RenderTarget>,
        &mut Msaa,
        Has<Fxaa>,
        Has<crate::OrbitCamera>,
    )>,
) {
    let level = quality.level();
    let msaa = if level.aa == Aa::Msaa4 {
        Msaa::Sample4
    } else {
        Msaa::Off
    };
    for (entity, camera, target, mut samples, fxaa, game) in &mut cameras {
        if !(quality.is_changed() || camera.is_added()) {
            continue;
        }
        // (The game camera too while it draws into its scaled picture.)
        if !game && !matches!(target, None | Some(RenderTarget::Window(_))) {
            continue;
        }
        if *samples != msaa {
            *samples = msaa;
        }
        let want_fxaa = game && level.aa == Aa::Fxaa;
        if want_fxaa && !fxaa {
            commands.entity(entity).insert(Fxaa::default());
        } else if !want_fxaa && fxaa {
            commands.entity(entity).remove::<Fxaa>();
        }
    }
}

/// The Render Scale: at 100% the game camera draws into the window, as it always
/// did; below it, into a picture that much smaller each way (`ScaledView`), which a
/// camera of its own spreads over the window (filtered), before the HUD draws over
/// it at the window's own size. The picture is made again as the window's size
/// changes.
///
/// Split screen's cameras (split.rs) draw into such a picture at any scale (at 100%
/// one the window's size), together, each into his part, each in an order of his
/// own (the first player's last), and both HUDs draw over the camera showing it.
/// Cameras drawing into one picture share their colour textures whole: each draws
/// his part, then works his effects (the colour grade, the depth of field...) over
/// all of them, and only his part goes on to the picture itself. Straight into the
/// window, the first player's effects would be worked over the guest's half too,
/// and the window's other cameras (the menu's, the Flash front end's), which put
/// all of the window's shared texture on the screen, would show that: the guest's
/// half graded twice, darker, with the first player's depth of field's lowest row
/// drawn down it. In a picture of their own only each view's part comes through.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn apply_render_scale(
    mut commands: Commands,
    scale: Res<RenderScale>,
    mut view: ResMut<ScaledView>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut game: Query<
        (
            Entity,
            &mut Camera,
            Option<&crate::split::Seat>,
            &RenderTarget,
        ),
        With<crate::OrbitCamera>,
    >,
    mut images: ResMut<Assets<Image>>,
    mut shown: Query<&mut Sprite>,
    mut present: Query<&mut Camera, (Without<crate::OrbitCamera>, Without<crate::hud::HudCamera>)>,
    mut huds: Query<
        (&mut Camera, &crate::split::Seat),
        (With<crate::hud::HudCamera>, Without<crate::OrbitCamera>),
    >,
    split: Res<crate::split::Split>,
) {
    if game.is_empty() {
        return;
    }
    // (Each camera's order: the guest's below the first player's, with room for
    // his HUD's between them.)
    let order = |base: isize, seat: Option<&crate::split::Seat>| {
        base - 2 * seat.map_or(0, |s| s.0 as isize)
    };
    let window_size = window.physical_size().max(UVec2::ONE);
    let k = scale.0.clamp(RENDER_SCALES.0, RENDER_SCALES.1);
    // (Split screen's views into a picture of their own whatever the scale.)
    let whole = k >= RENDER_SCALES.1 - 1e-3 && !split.active();
    for (mut camera, seat) in &mut huds {
        let order = 1 + seat.0 as isize;
        if camera.order != order {
            camera.order = order;
        }
    }
    if whole {
        // Whole: straight into the window, as before.
        let was_scaled = view.image.take().is_some();
        for (entity, mut camera, seat, _) in &mut game {
            if was_scaled {
                commands
                    .entity(entity)
                    .insert(RenderTarget::Window(WindowRef::Primary));
            }
            let order = order(0, seat);
            if camera.order != order {
                camera.order = order;
            }
        }
        if was_scaled {
            for e in [view.camera.take(), view.sprite.take()]
                .into_iter()
                .flatten()
            {
                commands.entity(e).despawn();
            }
            view.size = UVec2::ZERO;
        }
        return;
    }
    let size = (window_size.as_vec2() * k)
        .round()
        .as_uvec2()
        .max(UVec2::ONE);
    // The picture shows over the window, whatever its scale factor.
    let logical = Vec2::new(window.width(), window.height());
    if view.image.is_none() || view.size != size {
        let mut image = Image::new_fill(
            Extent3d {
                width: size.x,
                height: size.y,
                ..default()
            },
            TextureDimension::D2,
            &[0, 0, 0, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | TextureUsages::RENDER_ATTACHMENT;
        let handle = images.add(image);
        if let Some(old) = view.image.replace(handle.clone()) {
            images.remove(&old);
        }
        view.size = size;
        match view.sprite.and_then(|e| shown.get_mut(e).ok()) {
            Some(mut sprite) => sprite.image = handle,
            None => {
                view.sprite = Some(
                    commands
                        .spawn((
                            Sprite {
                                image: handle,
                                custom_size: Some(logical),
                                ..default()
                            },
                            Transform::default(),
                            RenderLayers::layer(SCALED_LAYER),
                        ))
                        .id(),
                );
            }
        }
        if view.camera.is_none() {
            // Under the HUD (order 1), HDR without tone mapping like the cameras
            // drawing over it (the picture is already the game camera's
            // finished one).
            view.camera = Some(
                commands
                    .spawn((
                        Camera2d,
                        Camera {
                            order: 0,
                            clear_color: ClearColorConfig::Custom(Color::BLACK),
                            ..default()
                        },
                        Hdr,
                        Tonemapping::None,
                        RenderLayers::layer(SCALED_LAYER),
                    ))
                    .id(),
            );
        }
    }
    // Every game camera into it, one that comes after it was made too (split
    // screen's guest's, made from the first player's as he joins).
    if let Some(handle) = &view.image {
        for (entity, _, _, target) in &game {
            if !matches!(target, RenderTarget::Image(t) if t.handle == *handle) {
                commands
                    .entity(entity)
                    .insert(RenderTarget::Image(ImageRenderTarget::from(handle.clone())));
            }
        }
    }
    // (Before the camera that shows it.)
    let mut drawing = false;
    for (_, mut camera, seat, _) in &mut game {
        let order = order(-2, seat);
        if camera.order != order {
            camera.order = order;
        }
        drawing |= camera.is_active;
    }
    if let Some(mut sprite) = view.sprite.and_then(|e| shown.get_mut(e).ok())
        && sprite.custom_size != Some(logical)
    {
        sprite.custom_size = Some(logical);
    }
    // (Shown while the game camera draws.)
    if let Some(mut c) = view.camera.and_then(|e| present.get_mut(e).ok())
        && c.is_active != drawing
    {
        c.is_active = drawing;
    }
}

/// Bevy works a camera's picture size out again as its window or image changes, or
/// as it comes, but not as it is pointed at another picture: pointed at one of
/// another size (the Render Scale's, as the window's size changes), it would go on
/// with the old size for a frame or more, and cameras drawing into one picture must
/// agree on its size (they share their colour and depth textures: one of another
/// size is a wgpu validation error, which stops the demo). Marking its projection
/// changed has Bevy work it out again at once.
fn refresh_retargeted(mut cameras: Query<&mut Projection, Changed<RenderTarget>>) {
    for mut projection in &mut cameras {
        projection.set_changed();
    }
}

/// The level's limit on the live lights: the nearest the camera lit, the rest
/// put out (and lit again when the level allows them). (With two players on one
/// screen, the nearest either camera.)
#[allow(clippy::type_complexity)]
fn limit_live_lights(
    mut commands: Commands,
    quality: Res<GraphicsQuality>,
    camera: Query<&GlobalTransform, With<crate::OrbitCamera>>,
    mut lights: Query<(Entity, &GlobalTransform, &mut Visibility, Has<PutOut>), With<LiveLight>>,
    mut order: Local<Vec<(Vec3, Entity)>>,
) {
    let limit = quality.level().live_lights;
    let eyes: Vec<Vec3> = camera.iter().map(|t| t.translation()).collect();
    order.clear();
    order.extend(lights.iter().map(|(e, t, ..)| (t.translation(), e)));
    nearest_first(&eyes, limit, &mut order);
    for (i, &(_, entity)) in order.iter().enumerate() {
        let Ok((_, _, mut visibility, out)) = lights.get_mut(entity) else {
            continue;
        };
        let lit = limit.is_none_or(|n| i < n);
        match (lit, out) {
            (false, false) => {
                *visibility = Visibility::Hidden;
                commands.entity(entity).insert(PutOut);
            }
            (true, true) => {
                *visibility = Visibility::Inherited;
                commands.entity(entity).remove::<PutOut>();
            }
            _ => {}
        }
    }
}

/// The lights (where each is) nearest one of the `eyes` first (each by the eye
/// nearest it), when there are more than `limit`.
fn nearest_first(eyes: &[Vec3], limit: Option<usize>, lights: &mut [(Vec3, Entity)]) {
    if let Some(n) = limit
        && n < lights.len()
        && !eyes.is_empty()
    {
        let away = |at: Vec3| {
            eyes.iter()
                .map(|e| at.distance_squared(*e))
                .fold(f32::INFINITY, f32::min)
        };
        lights.sort_by(|a, b| away(a.0).total_cmp(&away(b.0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mesh Bevy has a material for but no specialization entry (its material
    /// swapped late in the frame) is put back in its table, to be set up now; one
    /// that has its entry keeps it as it was.
    #[test]
    fn a_mesh_left_out_of_bevys_table_is_put_back() {
        use bevy::ecs::change_detection::Tick;
        use bevy::ecs::system::RunSystemOnce;
        use bevy::pbr::{
            EntitySpecializationTickPair, EntitySpecializationTicks, RenderMaterialInstance,
            RenderMaterialInstances,
        };
        use bevy::render::sync_world::MainEntity;
        let mut world = World::new();
        let left_out = MainEntity::from(world.spawn_empty().id());
        let kept = MainEntity::from(world.spawn_empty().id());
        let mut instances = RenderMaterialInstances::default();
        for e in [left_out, kept] {
            instances.instances.insert(
                e,
                RenderMaterialInstance {
                    asset_id: AssetId::<StandardMaterial>::default().untyped(),
                    last_change_tick: Tick::new(4),
                },
            );
        }
        instances.current_change_tick = Tick::new(5);
        let mut ticks = EntitySpecializationTicks::default();
        let old = EntitySpecializationTickPair {
            system_tick: Tick::new(1),
            material_instances_tick: Tick::new(2),
        };
        ticks.entities.insert(kept, old);
        world.insert_resource(instances);
        world.insert_resource(ticks);
        world.run_system_once(keep_meshes_drawable).unwrap();
        let ticks = world.resource::<EntitySpecializationTicks>();
        let put = ticks.entities.get(&left_out).expect("put back");
        assert_eq!(put.material_instances_tick, Tick::new(4));
        let k = ticks.entities.get(&kept).unwrap();
        assert_eq!(
            (k.system_tick, k.material_instances_tick),
            (old.system_tick, old.material_instances_tick)
        );
    }

    #[test]
    fn high_is_the_demos_look_and_the_levels_step_down() {
        let high = &LEVELS[HIGH];
        // (The look before the setting: lighting.rs's 4096 shadow map, its four
        // cascades out to 220 m from 18; MAP_ANISOTROPY 4, levelfx's 16.)
        assert_eq!((high.shadow_map, high.cascades), (4096, 4));
        assert_eq!((high.first_cascade, high.shadow_distance), (18.0, 220.0));
        assert!(high.bloom && high.dof && high.motion_blur);
        assert_eq!(high.aa, Aa::Msaa4);
        assert_eq!((high.map_anisotropy, high.fx_anisotropy), (4, 16));
        assert_eq!((high.texture_lod_bias, high.priority_lod_bias), (0, 0));
        assert_eq!((high.live_lights, high.particle_lod_bias), (None, 0));
        assert_eq!(
            (high.effect_distance, high.effect_meshes),
            (None, Some(100))
        );
        // The game's LOD biases by level.
        let biases: Vec<(u32, u32)> = LEVELS
            .iter()
            .map(|l| (l.texture_lod_bias, l.priority_lod_bias))
            .collect();
        assert_eq!(biases, [(2, 1), (1, 0), (0, 0), (0, 0)]);
        // Each level up costs at least as much as the one below it.
        for w in LEVELS.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            assert!(a.shadow_map <= b.shadow_map && a.cascades <= b.cascades);
            assert!(a.shadow_distance <= b.shadow_distance);
            assert!(a.particle_lod_bias >= b.particle_lod_bias);
            assert!(a.character_shadows <= b.character_shadows);
            assert!(a.map_anisotropy <= b.map_anisotropy);
            let far = |l: &Level| l.effect_distance.unwrap_or(f32::MAX);
            assert!(far(a) <= far(b));
            let meshes = |l: &Level| l.effect_meshes.unwrap_or(usize::MAX);
            assert!(meshes(a) <= meshes(b));
            let lights = |l: &Level| l.live_lights.unwrap_or(usize::MAX);
            assert!(lights(a) <= lights(b));
        }
        // Only the texture settings wait for a restart.
        assert!(!needs_restart(GraphicsQuality(2), GraphicsQuality(2)));
        assert!(needs_restart(GraphicsQuality(2), GraphicsQuality(3)));
        assert!(needs_restart(GraphicsQuality(2), GraphicsQuality(0)));
    }

    #[test]
    fn live_lights_past_the_limit_are_put_out_nearest_kept() {
        // The nearest first, when there are more than the limit.
        let e = |i: u32| Entity::from_raw_u32(i).unwrap();
        let mut order = vec![
            (Vec3::X * 9.0, e(1)),
            (Vec3::X * 2.0, e(2)),
            (Vec3::X * 5.0, e(3)),
        ];
        nearest_first(&[Vec3::ZERO], Some(2), &mut order);
        assert_eq!(
            order.iter().map(|o| o.1).collect::<Vec<_>>(),
            [e(2), e(3), e(1)]
        );
        // (With two players on one screen, by the nearer camera.)
        nearest_first(&[Vec3::ZERO, Vec3::X * 10.0], Some(2), &mut order);
        assert_eq!(
            order.iter().map(|o| o.1).collect::<Vec<_>>(),
            [e(1), e(2), e(3)]
        );

        let mut app = App::new();
        app.insert_resource(GraphicsQuality(1))
            .add_systems(Update, limit_live_lights);
        let lights: Vec<Entity> = (0..8)
            .map(|i| {
                app.world_mut()
                    .spawn((
                        LiveLight,
                        GlobalTransform::from_translation(Vec3::X * i as f32),
                        Visibility::Inherited,
                    ))
                    .id()
            })
            .collect();
        app.update();
        let lit =
            |app: &App, e: Entity| app.world().get::<Visibility>(e) == Some(&Visibility::Inherited);
        // Medium lights six of the eight.
        assert_eq!(lights.iter().filter(|&&e| lit(&app, e)).count(), 6);
        // High lights them all again.
        app.insert_resource(GraphicsQuality(2));
        app.update();
        assert!(lights.iter().all(|&e| lit(&app, e)));
        // Low, none.
        app.insert_resource(GraphicsQuality(0));
        app.update();
        assert!(lights.iter().all(|&e| !lit(&app, e)));
    }
}
