//! The maps' lighting, taken from the game by tools/build_map.py (and the
//! garage's by tools/build_garage.py).
//!
//! Each map is lit the way the game lights it:
//!
//! * **Baked light** - the game's own light maps (Lightmass), from its
//!   `<map>_ART_m_LM.xxx` and `<map>_BASE_m_LM.xxx` (Downfall's
//!   `ESC_Iacon_ART_m_LM.xxx`, Oblivion's `ESC_Ark_ART_m_LM.xxx`, Ancients'
//!   `ESC_Temple_ART_m_LM.xxx`, its terrain's too, Ignition's
//!   `ESC_Gladiator_ART_m_LM.xxx`). Every light-mapped node of the
//!   map's glTF names its light-map page and the part of it its second UV set covers
//!   (glTF extras); `attach_lightmaps` turns that into Bevy [`Lightmap`]s, shaded
//!   as the game's light-mapped shaders shade a surface facing along its normal
//!   ([`LIGHTMAP_FLAT`]). Vertex-lit props carry their light as vertex colours with a
//!   white page.
//! * **The sky light** - the level's DominantDirectionalLight, which the game
//!   lights everything with in real time (with shadows), so it is a shadowed
//!   [`DirectionalLight`] here too. Its light shafts, or another light's that
//!   shines the same way (a `shafts` line: [`ShaftLight`]), are lightshafts.rs's.
//! * **Lights for moving things** - the game lights characters with the level's
//!   lights; each becomes a point or spot light that lights the player and the
//!   enemies (and adds highlights to the map, whose diffuse light is baked).
//! * **Fog, bloom, depth of field and colour grading** from the level's
//!   post-process volume (the depth of field is dof.rs's). The grade is the
//!   game's 32x32x32 colour table, applied after the game's filmic tone curve
//!   (`game_filmic`) at its display gamma.
//! * **Material flags** - build_map.py marks a few materials in glTF material
//!   extras (`{"foc": "add nofog"}`): `map_material_flags` draws "add" ones
//!   additively, leaves "nofog" ones (the far backdrop) out of the fog, lets
//!   "noshadow" ones (the skies) and see-through ones (added on or blended) cast no
//!   shadow, moves "depthbias:<n>" ones nearer
//!   or further in depth (a terrain's layers, drawn over each other), and shows
//!   "flipbook:<columns>:<rows>:<frames a second>[:random]" ones (the game's
//!   TextureFlipBook screens: their texture holds the frames left to right, top to
//!   bottom) a frame at a time, in turn or at random (`animate_flip_books`).
//!
//! main.rs puts a map in play with [`MapLighting::load`] and [`MapLighting::light`],
//! and changes the camera with [`MapLighting::camera_effects`].
//!
//! Brightness: the game works in "1.0 = white" units. Bevy's camera exposure
//! (EV100 9.7, the default) divides by about 1000, so game values are multiplied by
//! `1 / exposure` (and by pi for lights, which Bevy's diffuse divides out).

use crate::dof::GameDof;
use bevy::asset::{RenderAssetUsages, UntypedAssetId};
use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::{Tonemapping, TonemappingLuts};
use bevy::gltf::{GltfExtras, GltfMaterialExtras};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{GlobalAmbientLight, NotShadowCaster};
use bevy::math::Affine2;
use bevy::pbr::Lightmap;
use bevy::post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;
use std::path::Path;

/// The lighting file of the map whose files are named `stem` (written by
/// tools/build_map.py, with tools/build_level.py's write_lights).
pub fn lights_file(stem: &str) -> String {
    format!("levels/{stem}_lights.txt")
}

/// Light-map texels hold light / this (LIGHTMAP_RANGE in tools/build_level.py).
pub(crate) const LIGHTMAP_RANGE: f32 = 4.0;

/// The game's light-mapped shading of a surface facing along its normal, against
/// what the pages hold. The pages hold stock UE3's sum: the light from each of the
/// light map's three directions times its cosine to the normal (1/sqrt(3) each).
/// FoC's shaders weigh each by (1.127 x that cosine)^2 instead (every light-mapped
/// base pass of the game's compiled shaders, the levels' materials' and Cybertron's
/// alike; levelfx_shader.wgsl's fx_surface_color does the same), which comes to
/// 1.127^2 / sqrt(3) of the page's light.
pub(crate) const LIGHTMAP_FLAT: f32 = 0.733_309_3;

/// Light (in the game's units) that the player and enemies get from all around,
/// standing in for the bounce light the game gives characters.
const CHARACTER_AMBIENT: f32 = 0.22;

/// The game's bloom (UE3's, with the engine's defaults the maps keep: WorldInfo's
/// DefaultPostProcessSettings): the light over Bloom_Threshold 1.0, blurred by a
/// kernel of DOF_BlurBloomKernelSize 16 pixels (at the game's 1280-wide reference:
/// a small glow round what is bright) and added at the map's Bloom_Scale. Bevy's
/// bloom is a chain of ever wider blurs (its mips, from 512 texels tall down to
/// 4): the game's kernel is about its first four, so those are taken whole
/// (BLOOM_HIGH_PASS: the mips to that share of the chain, fading out by twice
/// it) and the wider ones left out, which would spread a bright effect's light
/// over the whole screen as haze. Each mip taken carries the bright light's whole
/// energy, so the bloom's intensity is the map's scale shared between them
/// (BLOOM_MIPS: their weights, 1, 0.96, 0.96, 0.55 and 0.05).
const BLOOM_HIGH_PASS: f32 = 0.3;
const BLOOM_MIPS: f32 = 3.51;

/// The bloom at a map's Bloom_Scale (see BLOOM_HIGH_PASS).
pub fn game_bloom(scale: f32) -> Bloom {
    let intensity = scale / BLOOM_MIPS;
    Bloom {
        intensity,
        // (Each mip taken passed on whole to the one above: its blend factor,
        // intensity plus this boost, about 1.)
        low_frequency_boost: 1.0 - intensity,
        low_frequency_boost_curvature: 0.95,
        high_pass_frequency: BLOOM_HIGH_PASS,
        prefilter: BloomPrefilter {
            threshold: 1.0,
            threshold_softness: 0.3,
        },
        composite_mode: BloomCompositeMode::Additive,
        ..Bloom::NATURAL
    }
}

/// The game's 1.0 in Bevy's light units, at the default camera exposure.
pub fn unit() -> f32 {
    1.0 / Exposure::default().exposure()
}

/// A map's sun (its DominantDirectionalLight), whose shadows the Graphics level
/// sets (graphics.rs).
#[derive(Component)]
pub struct MapSun;

pub struct Sun {
    /// Direction the light travels.
    pub direction: Vec3,
    pub color: Color,
    pub brightness: f32,
}

/// A light that draws the game's light shafts (lightshafts.rs): the sun's rays.
/// The game's LightComponent values; its direction is the way its light travels.
pub struct ShaftLight {
    pub direction: Vec3,
    /// BloomTint.
    pub tint: Color,
    /// BloomScale, BloomThreshold and BloomScreenBlendThreshold.
    pub bloom_scale: f32,
    pub threshold: f32,
    pub screen_blend: f32,
    /// OcclusionDepthRange, in metres.
    pub range: f32,
    /// OcclusionMaskDarkness.
    pub darkness: f32,
}

pub struct MapLight {
    pub position: Vec3,
    pub color: Color,
    pub brightness: f32,
    pub radius: f32,
    pub falloff: f32,
    /// Spot lights: direction and (inner, outer) cone half-angles in degrees.
    pub spot: Option<(Vec3, f32, f32)>,
    /// It lights only what moves, the characters: not the level (its lighting
    /// channels have neither BSP nor Static: the "dynamic" word).
    pub dynamic_only: bool,
}

pub struct MapFog {
    pub color: Color,
    pub brightness: f32,
    pub density: f32,
}

/// One of a level's height-fog layers as the game has it ("heightfog" lines): fog
/// below `height` (m), from `start` (m) out from the camera, `density` per metre.
pub struct HeightFogLayer {
    pub color: Color,
    pub brightness: f32,
    pub density: f32,
    pub start: f32,
    pub height: f32,
}

/// What a map's lighting file describes: the map in play's is a resource, when it
/// has one.
#[derive(Resource)]
pub struct MapLighting {
    pub sun: Option<Sun>,
    /// The light that draws its light shafts, if it has one.
    pub shafts: Option<ShaftLight>,
    pub lights: Vec<MapLight>,
    pub fog: Option<MapFog>,
    /// Its height-fog layers, where the file lists them (the Customize garage's).
    pub height_fogs: Vec<HeightFogLayer>,
    pub bloom: Option<f32>,
    /// Its depth of field, where its post-process volume turns it on (dof.rs).
    pub dof: Option<GameDof>,
    /// 32x32x32 RGB bytes, red fastest.
    pub grade: Option<Vec<u8>>,
    /// The cube map the characters reflect (character.rs, paint.wgsl) and its mip
    /// count: the level's captured one, or the game's default (tools/build_level.py).
    pub reflect: Option<(String, u32)>,
}

impl MapLighting {
    /// The lighting of the map whose files are named `stem`, if it has a lighting file.
    pub fn load(assets: &Path, stem: &str) -> Option<MapLighting> {
        MapLighting::load_file(assets, &lights_file(stem))
    }

    /// Reads a lighting file in build_level.py's format (`file` is relative to the
    /// assets folder; the customize screen's garage has its own).
    pub fn load_file(assets: &Path, file: &str) -> Option<MapLighting> {
        let text = std::fs::read_to_string(assets.join(file)).ok()?;
        let mut map = MapLighting {
            sun: None,
            shafts: None,
            lights: Vec::new(),
            fog: None,
            height_fogs: Vec::new(),
            bloom: None,
            dof: None,
            grade: None,
            reflect: None,
        };
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let kind = parts.next().unwrap_or("");
            let words: Vec<&str> = parts.collect();
            let v: Vec<f32> = words.iter().filter_map(|w| w.parse().ok()).collect();
            let rgb = |i: usize| Color::srgb_u8(v[i] as u8, v[i + 1] as u8, v[i + 2] as u8);
            match kind {
                "sun" if v.len() >= 7 => {
                    map.sun = Some(Sun {
                        direction: Vec3::new(v[0], v[1], v[2]).normalize_or(Vec3::NEG_Y),
                        color: rgb(3),
                        brightness: v[6],
                    });
                }
                "shafts" if v.len() >= 11 => {
                    map.shafts = Some(ShaftLight {
                        direction: Vec3::new(v[0], v[1], v[2]).normalize_or(Vec3::NEG_Y),
                        tint: rgb(3),
                        bloom_scale: v[6],
                        threshold: v[7],
                        screen_blend: v[8],
                        range: v[9],
                        darkness: v[10],
                    });
                }
                "point" if v.len() >= 9 => map.lights.push(MapLight {
                    position: Vec3::new(v[0], v[1], v[2]),
                    color: rgb(3),
                    brightness: v[6],
                    radius: v[7],
                    falloff: v[8],
                    spot: None,
                    dynamic_only: words.contains(&"dynamic"),
                }),
                "spot" if v.len() >= 14 => map.lights.push(MapLight {
                    position: Vec3::new(v[0], v[1], v[2]),
                    color: rgb(6),
                    brightness: v[9],
                    radius: v[10],
                    falloff: v[11],
                    spot: Some((Vec3::new(v[3], v[4], v[5]), v[12], v[13])),
                    dynamic_only: words.contains(&"dynamic"),
                }),
                // The first fog line is the one around the play area.
                "fog" if v.len() >= 5 && map.fog.is_none() => {
                    map.fog = Some(MapFog {
                        color: rgb(0),
                        brightness: v[3],
                        density: v[4],
                    });
                }
                "heightfog" if v.len() >= 7 => map.height_fogs.push(HeightFogLayer {
                    color: rgb(0),
                    brightness: v[3],
                    density: v[4],
                    start: v[5],
                    height: v[6],
                }),
                "bloom" if !v.is_empty() => map.bloom = Some(v[0]),
                "dof" => map.dof = GameDof::new(&v),
                "reflect" if !words.is_empty() => {
                    let mips = words.get(1).and_then(|m| m.parse().ok()).unwrap_or(1);
                    map.reflect = Some((words[0].to_string(), mips));
                }
                "grade" if !words.is_empty() => match std::fs::read(assets.join(words[0])) {
                    Ok(bytes) if bytes.len() == 32 * 32 * 32 * 3 => map.grade = Some(bytes),
                    _ => warn!("Colour grade {} missing or the wrong size", words[0]),
                },
                _ => {}
            }
        }
        info!(
            "Map lighting: {} lights, sun {}, light shafts {}, fog {}, grade {}",
            map.lights.len(),
            map.sun.is_some(),
            map.shafts.is_some(),
            map.fog.is_some(),
            map.grade.is_some()
        );
        Some(map)
    }

    /// Fog, bloom, depth of field and grading for the game camera.
    pub fn camera_effects(&self, commands: &mut EntityCommands) {
        if let Some(fog) = &self.fog {
            let c = fog.color.to_linear() * fog.brightness;
            commands.insert(DistanceFog {
                color: Color::LinearRgba(LinearRgba::new(c.red, c.green, c.blue, 1.0)),
                falloff: FogFalloff::Exponential {
                    density: fog.density,
                },
                ..default()
            });
        } else {
            commands.remove::<DistanceFog>();
        }
        if let Some(scale) = self.bloom {
            commands.insert(game_bloom(scale));
        } else {
            commands.remove::<Bloom>();
        }
        if let Some(dof) = self.dof {
            commands.insert(dof);
        } else {
            commands.remove::<GameDof>();
        }
        if self.grade.is_some() {
            // Its table is replaced with the game's grade (see Grade).
            commands.insert(Tonemapping::TonyMcMapface);
        }
    }

    /// Lights the map in play with this: the sun and the level's lights (their
    /// entities are returned, for the map to own), the light characters get from
    /// all around, and the colour grade.
    pub fn light(
        &self,
        commands: &mut Commands,
        grade: &mut Grade,
        luts: Option<&TonemappingLuts>,
        images: &mut Assets<Image>,
    ) -> Vec<Entity> {
        let unit = unit();
        // Characters get a little light from everywhere; the map's own bounce light
        // is in its light maps, so this doesn't touch light-mapped surfaces.
        commands.insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.82, 0.86, 1.0),
            brightness: CHARACTER_AMBIENT * unit,
            affects_lightmapped_meshes: false,
        });
        let mut spawned = Vec::new();
        if let Some(sun) = &self.sun {
            spawned.push(
                commands
                    .spawn((
                        DirectionalLight {
                            // Bevy's diffuse is albedo / pi * illuminance; the game's is albedo * brightness.
                            illuminance: sun.brightness * PI * unit,
                            color: sun.color,
                            shadows_enabled: true,
                            ..default()
                        },
                        Transform::default().looking_to(sun.direction, Vec3::Y),
                        // (Its cascades and shadow map as the Graphics level has
                        // them: graphics.rs.)
                        MapSun,
                    ))
                    .id(),
            );
        }
        for light in &self.lights {
            spawned.push(spawn_light(commands, light));
        }
        grade.set(self.grade.as_deref(), luts, images);
        spawned
    }
}

pub struct MapLightingPlugin;

impl Plugin for MapLightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Grade>()
            .init_resource::<LightmapPages>()
            .init_resource::<FlipBooks>()
            .add_systems(
                Update,
                (
                    (attach_lightmaps, set_lightmap_exposure).chain(),
                    (map_material_flags, animate_flip_books).chain(),
                    follow_display_gamma,
                ),
            );
    }
}

/// The colour grade on screen: the map's grade replaces Bevy's TonyMcMapface table
/// (see `grade_lut`), which is kept here meanwhile, to put back for a map without one.
/// The grade is kept too, to make its table again at another display gamma.
#[derive(Resource, Default)]
pub struct Grade {
    original: Option<Image>,
    grade: Option<Vec<u8>>,
    /// The display gamma (Settings > Display > Brightness: menu.rs's
    /// DisplaySettings::gamma), once it has been given.
    gamma: Option<f32>,
}

/// The display gamma the game sends its picture to the screen at, unless
/// Brightness has moved it.
const DISPLAY_GAMMA: f32 = 2.2;
/// While Brightness moves, the grade's table is made again at most this often.
const GAMMA_REBUILD_SECS: f32 = 0.12;

impl Grade {
    /// Shows `grade` (a map's colour table), or Bevy's own colours for None.
    pub fn set(
        &mut self,
        grade: Option<&[u8]>,
        luts: Option<&TonemappingLuts>,
        images: &mut Assets<Image>,
    ) {
        let Some(luts) = luts else {
            return;
        };
        self.grade = grade.map(<[u8]>::to_vec);
        let id = luts.tony_mc_mapface.id();
        let table = match grade {
            Some(grade) => {
                if self.original.is_none() {
                    self.original = images.get(id).cloned();
                }
                grade_lut(grade, self.gamma.unwrap_or(DISPLAY_GAMMA))
            }
            None => match self.original.take() {
                Some(original) => original,
                None => return,
            },
        };
        if images.insert(id, table).is_err() {
            warn!("Couldn't install the map's colour grade");
        }
    }

    /// Shows the map's grade at display gamma `gamma`.
    fn set_gamma(
        &mut self,
        gamma: f32,
        luts: Option<&TonemappingLuts>,
        images: &mut Assets<Image>,
    ) {
        self.gamma = Some(gamma);
        if let (Some(grade), Some(luts)) = (&self.grade, luts)
            && images
                .insert(luts.tony_mc_mapface.id(), grade_lut(grade, gamma))
                .is_err()
        {
            warn!("Couldn't install the map's colour grade");
        }
    }
}

/// Makes the map's grade again as Brightness moves the display gamma (at most
/// every GAMMA_REBUILD_SECS while it moves: a table takes some milliseconds).
fn follow_display_gamma(
    time: Res<Time<Real>>,
    display: Res<crate::menu::DisplaySettings>,
    luts: Option<Res<TonemappingLuts>>,
    mut grade: ResMut<Grade>,
    mut images: ResMut<Assets<Image>>,
    mut made_at: Local<Option<f32>>,
) {
    let gamma = display.gamma();
    if grade.gamma == Some(gamma) {
        return;
    }
    let now = time.elapsed_secs();
    if made_at.is_some_and(|at| now - at < GAMMA_REBUILD_SECS) {
        return;
    }
    *made_at = Some(now);
    grade.set_gamma(gamma, luts.as_deref(), &mut images);
}

/// The light-map pages loaded for the map in play (by file); main.rs empties it as
/// the map leaves, so that its pages go with it.
#[derive(Resource, Default)]
pub struct LightmapPages(HashMap<String, Handle<Image>>);

impl LightmapPages {
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// One of a level's lights as a Bevy light (its own entity, returned).
pub fn spawn_light(commands: &mut Commands, light: &MapLight) -> Entity {
    // The game's lights fade as (1 - (d / radius)^2)^falloff, Bevy's with the
    // square of the distance; they are matched a quarter of the way out.
    let d = light.radius * 0.25;
    let fade = (1.0 - 0.25f32 * 0.25).powf(light.falloff);
    let intensity = light.brightness * fade * 4.0 * PI * PI * d * d * unit();
    spawn_light_with(commands, light, intensity)
}

/// As [`spawn_light`], but as strong as the game's light `distance` from it rather
/// than a quarter of the way out: for a light that is there to light what stands
/// at that distance (the Customize screen's robots), so they get its full
/// strength. (Bevy's range window, which dims a light towards its radius, is
/// allowed for.)
pub fn spawn_light_at(commands: &mut Commands, light: &MapLight, distance: f32) -> Entity {
    let r = light.radius.max(1e-3);
    let d = distance.clamp(0.25 * r, 0.9 * r);
    let k = d / r;
    let fade = (1.0 - k * k).powf(light.falloff);
    let window = (1.0 - k.powi(4)).powi(2);
    let intensity = light.brightness * fade / window * 4.0 * PI * PI * d * d * unit();
    spawn_light_with(commands, light, intensity)
}

fn spawn_light_with(commands: &mut Commands, light: &MapLight, intensity: f32) -> Entity {
    let transform = Transform::from_translation(light.position);
    match light.spot {
        None => commands
            .spawn((
                PointLight {
                    color: light.color,
                    intensity,
                    range: light.radius,
                    radius: 0.1,
                    shadows_enabled: false,
                    // Its diffuse light is already in the light maps.
                    affects_lightmapped_mesh_diffuse: false,
                    ..default()
                },
                transform,
            ))
            .id(),
        Some((dir, inner, outer)) => {
            let outer = outer.clamp(1.0, 89.0).to_radians();
            commands
                .spawn((
                    SpotLight {
                        color: light.color,
                        intensity,
                        range: light.radius,
                        radius: 0.1,
                        shadows_enabled: false,
                        affects_lightmapped_mesh_diffuse: false,
                        outer_angle: outer,
                        inner_angle: inner.to_radians().clamp(0.0, outer),
                        ..default()
                    },
                    transform.looking_to(dir, if dir.y.abs() > 0.99 { Vec3::X } else { Vec3::Y }),
                ))
                .id()
        }
    }
}

/// Reads a glTF extras value `{"lightmap": "<page> x0 y0 x1 y1"}`.
pub(crate) fn parse_lightmap(extras: &str) -> Option<(String, Rect)> {
    let key = "\"lightmap\"";
    let rest = &extras[extras.find(key)? + key.len()..];
    let start = rest.find('"')? + 1;
    let end = start + rest[start..].find('"')?;
    let mut words = rest[start..end].split_whitespace();
    let file = words.next()?.to_string();
    let v: Vec<f32> = words.filter_map(|w| w.parse().ok()).collect();
    if v.len() != 4 {
        return None;
    }
    Some((file, Rect::new(v[0], v[1], v[2], v[3])))
}

/// Gives the map's meshes their baked light as their nodes come in.
fn attach_lightmaps(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    nodes: Query<(Entity, &GltfExtras, Option<&Children>), Added<GltfExtras>>,
    meshes: Query<(), With<Mesh3d>>,
    mut pages: ResMut<LightmapPages>,
) {
    for (entity, extras, children) in &nodes {
        let Some((file, uv_rect)) = parse_lightmap(&extras.value) else {
            continue;
        };
        let image = pages
            .0
            .entry(file.clone())
            .or_insert_with(|| asset_server.load(file))
            .clone();
        let lightmap = Lightmap {
            image,
            uv_rect,
            bicubic_sampling: false,
        };
        let mut parts = vec![entity];
        if let Some(children) = children {
            parts.extend_from_slice(children);
        }
        for part in parts {
            if meshes.contains(part) {
                commands.entity(part).insert(lightmap.clone());
            }
        }
    }
}

/// Reads a glTF material extras value `{"foc": "<flags>"}` (tools/build_map.py).
pub(crate) fn parse_flags(extras: &str) -> Option<&str> {
    let key = "\"foc\"";
    let rest = &extras[extras.find(key)? + key.len()..];
    let start = rest.find('"')? + 1;
    let end = start + rest[start..].find('"')?;
    Some(&rest[start..end])
}

/// How much further out a sky dome's own surface is drawn than the layers added
/// over it (`sky_base`): 0.2% of its size, some 30 m at Downfall's 16 km.
const SKY_BASE_PUSH: f32 = 1.002;

/// Whether a map material with these flags is a sky dome's own surface, under the
/// layers added on over it (its haze, Downfall's sun glow): a sky ("noshadow",
/// "nofog") not added on. Each such mesh is drawn a little further out than the
/// dome's triangles are (`SKY_BASE_PUSH`, scaled about the dome's middle), so that
/// the layers, on the dome's own triangles, never lose the depth test to it: by
/// their depth bias alone ("depthbias:16") whole triangles of a layer dropped out
/// now and then as the camera turned, showing the darker dome under them - dark
/// patches popping up in Downfall's sky.
fn sky_base(flags: &str) -> bool {
    let has = |f: &str| flags.split_whitespace().any(|w| w == f);
    has("noshadow") && has("nofog") && !has("add")
}

/// The map materials' flags (see the module docs): "add" ones are drawn additively,
/// "nofog" ones (the far backdrop) outside the fog, "noshadow" ones (the skies)
/// and see-through ones cast no shadow, a sky dome's own surface a little further
/// out than the layers over it (`sky_base`), "depthbias:<n>" ones nearer or further
/// in depth (the terrain's layers, `depth_bias_flag`), "flipbook:..." ones a frame
/// at a time (`FlipBooks`). ("nolight" ones are unlit already; "brdf:..." ones are
/// shaded as the characters are: character.rs's gild_level_parts; "prelit" ones
/// have their light map's light in their colour: set_lightmap_exposure.)
#[allow(clippy::type_complexity)]
fn map_material_flags(
    mut commands: Commands,
    mut parts: Query<
        (
            Entity,
            &GltfMaterialExtras,
            &MeshMaterial3d<StandardMaterial>,
            Option<&mut Transform>,
        ),
        Added<GltfMaterialExtras>,
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut books: ResMut<FlipBooks>,
    mut done: Local<HashSet<AssetId<StandardMaterial>>>,
) {
    for (entity, extras, material, transform) in &mut parts {
        let Some(flags) = parse_flags(&extras.value) else {
            continue;
        };
        if sky_base(flags)
            && let Some(mut t) = transform
        {
            t.scale *= SKY_BASE_PUSH;
        }
        // A sky surrounds the map: the sun's shadows would start at it and cover
        // everything. (Each mesh drawn with such a material.) Nor does anything
        // see-through or added on cast one, as in the game (its translucent
        // materials cast no shadows): the layers added over a sky dome (its haze,
        // its sun's glow) surround the map as the dome does, and Bevy's shadow
        // pass would draw them as solid.
        let blended = materials.get(material.id()).is_some_and(|m| {
            matches!(
                m.alpha_mode,
                AlphaMode::Blend | AlphaMode::Premultiplied | AlphaMode::Add
            )
        });
        if casts_no_shadow(flags) || blended {
            commands.entity(entity).insert(NotShadowCaster);
        }
        if done.contains(&material.id()) {
            continue;
        }
        let Some(m) = materials.get_mut(material.id()) else {
            continue;
        };
        done.insert(material.id());
        for flag in flags.split_whitespace() {
            match flag {
                "add" => m.alpha_mode = AlphaMode::Add,
                "nofog" => m.fog_enabled = false,
                "nolight" | "noshadow" | "prelit" => {}
                _ => {
                    if let Some(bias) = depth_bias_flag(flag) {
                        m.depth_bias = bias;
                        continue;
                    }
                    let seed = books.0.len() as u32;
                    if let Some(book) = FlipBook::parse(flag, material.id(), seed) {
                        m.uv_transform = book.frame_transform(0);
                        books.0.push(book);
                    }
                }
            }
        }
    }
}

/// Whether a map material with these flags casts no shadow: "noshadow" ones (the
/// skies) and "add" ones (drawn added on: see-through).
fn casts_no_shadow(flags: &str) -> bool {
    flags
        .split_whitespace()
        .any(|flag| flag == "noshadow" || flag == "add")
}

/// A "depthbias:<n>" flag's n: the material's depth pushed nearer (n > 0) or further
/// (n < 0) by n of the depth buffer's smallest steps, and, if it is see-through,
/// drawn as if n metres nearer (Bevy sorts those by distance plus the bias). The
/// terrain's first layer is pushed back so far (-1024) that the layers drawn over
/// it, each a little nearer than the one below (-1016, -1008), pass the depth test
/// just where it is, and are drawn before every other see-through thing.
fn depth_bias_flag(flag: &str) -> Option<f32> {
    flag.strip_prefix("depthbias:")?
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite())
}

/// A flip-book material: its texture holds `columns` x `rows` frames (left to right,
/// top to bottom), shown one at a time, `rate` a second, in turn or (`random`, as the
/// game's TFBM_RANDOM) a frame picked anew each time.
struct FlipBook {
    material: AssetId<StandardMaterial>,
    columns: u32,
    rows: u32,
    rate: f32,
    random: bool,
    seed: u32,
    shown: u32,
}

impl FlipBook {
    /// A "flipbook:<columns>:<rows>:<frames a second>[:random]" flag's flip book.
    fn parse(flag: &str, material: AssetId<StandardMaterial>, seed: u32) -> Option<FlipBook> {
        let mut parts = flag.strip_prefix("flipbook:")?.split(':');
        let columns: u32 = parts.next()?.parse().ok()?;
        let rows: u32 = parts.next()?.parse().ok()?;
        let rate: f32 = parts.next()?.parse().ok()?;
        let random = parts.next() == Some("random");
        (columns > 0 && rows > 0 && rate >= 0.0).then_some(FlipBook {
            material,
            columns,
            rows,
            rate,
            random,
            seed,
            shown: 0,
        })
    }

    /// The frame on show `seconds` in.
    fn frame_at(&self, seconds: f32) -> u32 {
        let frames = self.columns * self.rows;
        let tick = (seconds * self.rate).max(0.0) as u32;
        if !self.random {
            return tick % frames;
        }
        // A hash of the tick (and of the flip book, so that they differ).
        let mut x = tick.wrapping_mul(0x9E37_79B9) ^ self.seed.wrapping_mul(0x85EB_CA6B);
        x ^= x >> 15;
        x = x.wrapping_mul(0x2C1B_3C6D);
        x ^= x >> 12;
        x % frames
    }

    /// The UV transform that shows frame `frame` of the texture.
    fn frame_transform(&self, frame: u32) -> Affine2 {
        let size = Vec2::new(1.0 / self.columns as f32, 1.0 / self.rows as f32);
        let at = Vec2::new((frame % self.columns) as f32, (frame / self.columns) as f32);
        Affine2::from_scale_angle_translation(size, 0.0, at * size)
    }
}

/// The flip books of the map in play (`map_material_flags`).
#[derive(Resource, Default)]
pub(crate) struct FlipBooks(Vec<FlipBook>);

impl FlipBooks {
    /// Whether `material` turns its pages (each its own: two materials alike, one
    /// a flip book, aren't the same for long).
    pub(crate) fn has(&self, material: AssetId<StandardMaterial>) -> bool {
        self.0.iter().any(|book| book.material == material)
    }
}

/// Turns the flip books' pages: a material changes only when its frame does. Those
/// whose material has gone (with its map) are dropped.
fn animate_flip_books(
    time: Res<Time>,
    mut books: ResMut<FlipBooks>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let now = time.elapsed_secs();
    books.0.retain_mut(|book| {
        let frame = book.frame_at(now);
        if frame == book.shown {
            return materials.contains(book.material);
        }
        let Some(m) = materials.get_mut(book.material) else {
            return false;
        };
        m.uv_transform = book.frame_transform(frame);
        book.shown = frame;
        true
    });
}

/// Scales the light maps back up (their texels are light / LIGHTMAP_RANGE) to the
/// light the game's shading gives a surface facing along its normal
/// (LIGHTMAP_FLAT). A "prelit" material (flags) has the game's shading baked into
/// its colour already, its light map plain white light (Oblivion's Cybertron). A
/// part drawn in a live material (its glow pulsing: live.rs) has its live
/// material's set so.
#[allow(clippy::type_complexity)]
fn set_lightmap_exposure(
    lit: Query<
        (
            AnyOf<(
                &MeshMaterial3d<StandardMaterial>,
                &MeshMaterial3d<crate::live::LiveMaterial>,
            )>,
            Option<&GltfMaterialExtras>,
        ),
        Added<Lightmap>,
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut live: ResMut<Assets<crate::live::LiveMaterial>>,
    mut done: Local<HashSet<UntypedAssetId>>,
) {
    for ((standard, alive), extras) in &lit {
        let id = match (standard, alive) {
            (Some(m), _) => m.id().untyped(),
            (None, Some(m)) => m.id().untyped(),
            (None, None) => continue,
        };
        if done.contains(&id) {
            continue;
        }
        let prelit = extras
            .and_then(|e| parse_flags(&e.value))
            .is_some_and(|flags| flags.split_whitespace().any(|f| f == "prelit"));
        let exposure = LIGHTMAP_RANGE * unit() * if prelit { 1.0 } else { LIGHTMAP_FLAT };
        let set = match (standard, alive) {
            (Some(m), _) => materials
                .get_mut(m.id())
                .map(|m| m.lightmap_exposure = exposure),
            (None, Some(m)) => live
                .get_mut(m.id())
                .map(|m| m.base.lightmap_exposure = exposure),
            (None, None) => None,
        };
        if set.is_some() {
            done.insert(id);
        }
    }
}

/// Looks up the game's colour table (gamma-encoded in and out) with trilinear
/// interpolation.
fn sample_grade(grade: &[u8], c: [f32; 3]) -> [f32; 3] {
    let n = 32usize;
    let pos = c.map(|v| (v.clamp(0.0, 1.0) * (n - 1) as f32).min((n - 1) as f32));
    let i0 = pos.map(|p| (p.floor() as usize).min(n - 2));
    let f = [
        pos[0] - i0[0] as f32,
        pos[1] - i0[1] as f32,
        pos[2] - i0[2] as f32,
    ];
    let mut out = [0.0f32; 3];
    for corner in 0..8 {
        let d = [corner & 1, (corner >> 1) & 1, (corner >> 2) & 1];
        let w = (0..3)
            .map(|k| if d[k] == 1 { f[k] } else { 1.0 - f[k] })
            .product::<f32>();
        let (r, g, b) = (i0[0] + d[0], i0[1] + d[1], i0[2] + d[2]);
        let at = ((b * n + g) * n + r) * 3;
        for k in 0..3 {
            out[k] += w * grade[at + k] as f32 / 255.0;
        }
    }
    out
}

/// The game's tone curve: its default post-process chain
/// (ENV_PostProcess_p.Trans_DefaultScenePostProcess, the engine ini's
/// DefaultPostProcessName, which no level replaces: the maps' and the Customize
/// garage's alike) has an UberPostProcessEffect that maps the light with
/// TMM_FilmicSCurve (Engine.xxx's FUberPostProcessPixelShader<TMM_FilmicSCurve>),
/// John Hable's filmic curve (A 0.22, B 0.3, C 0.1, D 0.2, E 0.01, F 0.3) with its
/// white at 11.2, before the display gamma and the level's colour grade: 0..1 of
/// the light `x`.
pub fn game_filmic(x: f32) -> f32 {
    let x = x.clamp(0.0, 500.0);
    let f = (x * (0.22 * x + 0.03) + 0.002) / (x * (0.22 * x + 0.3) + 0.06) - 0.01 / 0.3;
    (f * 1.153).clamp(0.0, 1.0)
}

/// Bevy's Blender Filmic tone mapping is a 64^3 table looked up with the log2 of
/// the light (from 2^-11 to 2^12 times 0.18); this fills one with the game's
/// filmic curve at display gamma `gamma`: the screen shows game_filmic(x) ^ (1 /
/// gamma) of each colour, as the game sends its picture to the screen (texels are
/// sRGB, so storing those values makes the screen show exactly them).
pub fn filmic_lut(gamma: f32) -> Image {
    const N: usize = 64;
    let curve: Vec<u8> = (0..N)
        .map(|i| {
            let ev = i as f32 / (N - 1) as f32 * 23.0 - 11.0;
            let shown = game_filmic(0.18 * ev.exp2()).powf(1.0 / gamma);
            (shown * 255.0).round().clamp(0.0, 255.0) as u8
        })
        .collect();
    let mut data = Vec::with_capacity(N * N * N * 4);
    for b in 0..N {
        for g in 0..N {
            for r in 0..N {
                data.extend([curve[r], curve[g], curve[b], 255]);
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: N as u32,
            height: N as u32,
            depth_or_array_layers: N as u32,
        },
        TextureDimension::D3,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        label: Some("Game filmic curve".into()),
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// Bevy's TonyMcMapface tone mapping is a 48^3 table looked up with
/// x / (1 + x) of the light; this fills one with the game's filmic curve
/// (game_filmic) and the map's grade, the picture at display gamma `gamma` (2.2
/// unless Brightness has moved it). Texels are sRGB, so storing the grade's
/// (display) values directly makes the screen show exactly those values.
fn grade_lut(grade: &[u8], gamma: f32) -> Image {
    const N: usize = 48;
    let light = |i: usize| {
        let s = i as f32 / (N - 1) as f32;
        if i == N - 1 { 1.0e4 } else { s / (1.0 - s) }
    };
    // The game grades its picture after its tone curve, at its display gamma.
    let shown: [f32; N] = std::array::from_fn(|i| game_filmic(light(i)).powf(1.0 / gamma));
    let mut data = Vec::with_capacity(N * N * N * 4);
    for b in 0..N {
        for g in 0..N {
            for r in 0..N {
                let c = [shown[r], shown[g], shown[b]];
                let out = sample_grade(grade, c);
                data.extend(out.map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8));
                data.push(255);
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: N as u32,
            height: N as u32,
            depth_or_array_layers: N as u32,
        },
        TextureDimension::D3,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        label: Some("Map grade".into()),
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_filmic_curve() {
        // Black stays black, the white point (11.2) is white, 1.0 is under half.
        assert!(game_filmic(0.0).abs() < 1e-4);
        assert!((game_filmic(11.2) - 1.0).abs() < 2e-3);
        assert!((game_filmic(1.0) - 0.4625).abs() < 2e-3);
        let mut last = -1.0;
        for i in 0..200 {
            let v = game_filmic(i as f32 * 0.1);
            assert!(v >= last);
            last = v;
        }
    }

    #[test]
    fn reads_lightmap_extras() {
        let (file, rect) =
            parse_lightmap(r#"{"lightmap":"levels/downfall_lightmaps/3.png 0.1 0.2 0.3 0.4"}"#)
                .unwrap();
        assert_eq!(file, "levels/downfall_lightmaps/3.png");
        assert!((rect.min.x - 0.1).abs() < 1e-6 && (rect.max.y - 0.4).abs() < 1e-6);
        assert!(parse_lightmap(r#"{"other":1}"#).is_none());
    }

    #[test]
    fn reads_material_flags() {
        assert_eq!(
            parse_flags(r#"{"foc":"add nofog nolight"}"#),
            Some("add nofog nolight")
        );
        assert_eq!(parse_flags(r#"{"foc": "nolight"}"#), Some("nolight"));
        assert_eq!(parse_flags(r#"{"other":1}"#), None);
        assert_eq!(lights_file("oblivion"), "levels/oblivion_lights.txt");
        // The terrain's layers.
        assert_eq!(depth_bias_flag("depthbias:-1024"), Some(-1024.0));
        assert_eq!(depth_bias_flag("depthbias:-1016"), Some(-1016.0));
        assert_eq!(depth_bias_flag("depthbias:x"), None);
        assert_eq!(depth_bias_flag("noshadow"), None);
        // The skies and whatever is added on cast no shadow (a dome's haze and
        // its sun's glow surround the map as the dome does).
        assert!(casts_no_shadow("nofog nolight noshadow"));
        assert!(casts_no_shadow("add nofog nolight depthbias:16"));
        assert!(!casts_no_shadow("nolight"));
        assert!(!casts_no_shadow("depthbias:-1024"));
        // A dome's own surface is drawn a little further out than the layers over
        // it; the layers and everything else stay where they are.
        assert!(sky_base("nofog nolight noshadow"));
        assert!(!sky_base("add nofog nolight noshadow depthbias:16"));
        assert!(!sky_base("fx:2 noshadow"));
        assert!(!sky_base("nofog"));
    }

    #[test]
    fn the_sunny_maps_have_their_light_shafts() {
        let assets = crate::assets_dir();
        // Downfall's shaft light (shining as its sun does, lighting nothing), and
        // Ancients' and Ignition's suns: their BloomScale, BloomScreenBlendThreshold
        // and OcclusionDepthRange. Oblivion has none.
        for (stem, scale, blend, range) in [
            ("downfall", 1.0, 0.2, 1000.0),
            ("ancients", 10.0, 1.0, 200.0),
            ("ignition", 0.5, 1.0, 200.0),
        ] {
            let Some(map) = MapLighting::load(&assets, stem) else {
                continue;
            };
            let shafts = map
                .shafts
                .unwrap_or_else(|| panic!("{stem}: no shafts line"));
            assert_eq!(shafts.bloom_scale, scale, "{stem}");
            assert!((shafts.screen_blend - blend).abs() < 1e-3, "{stem}");
            assert_eq!(shafts.range, range, "{stem}");
            assert_eq!(shafts.darkness, 0.3, "{stem}");
            let sun = map.sun.unwrap_or_else(|| panic!("{stem}: no sun"));
            assert!(shafts.direction.dot(sun.direction) > 0.999, "{stem}");
        }
        if let Some(map) = MapLighting::load(&assets, "oblivion") {
            assert!(map.shafts.is_none());
        }
    }

    #[test]
    fn every_map_names_what_its_characters_reflect() {
        let assets = crate::assets_dir();
        for stem in ["downfall", "oblivion", "ancients", "ignition", "garage"] {
            // (An install without that map has nothing to check.)
            let Some(map) = MapLighting::load(&assets, stem) else {
                continue;
            };
            let (cube, mips) = map
                .reflect
                .unwrap_or_else(|| panic!("{stem}: no reflect line"));
            assert!(assets.join(&cube).exists(), "{stem}: {cube} missing");
            assert!(mips > 1, "{stem}: {mips} mips");
        }
    }

    #[test]
    fn identity_grade_keeps_values() {
        let mut grade = Vec::new();
        for b in 0..32u32 {
            for g in 0..32u32 {
                for r in 0..32u32 {
                    grade.extend([r, g, b].map(|v| (v * 255 / 31) as u8));
                }
            }
        }
        let out = sample_grade(&grade, [0.5, 0.25, 1.0]);
        assert!(
            (out[0] - 0.5).abs() < 0.01
                && (out[1] - 0.25).abs() < 0.01
                && (out[2] - 1.0).abs() < 0.01
        );
    }

    #[test]
    fn light_maps_are_shaded_as_the_game_shades_them() {
        // A surface facing along its normal: each of the three directions at 54.7
        // degrees, (1.127 cos)^2 each, against the pages' cos (1/sqrt(3)) each.
        let cos = 1.0f32 / 3.0f32.sqrt();
        let game = 3.0 * (1.127 * cos).powi(2);
        let page = 3.0 * cos;
        assert!((LIGHTMAP_FLAT - game / page).abs() < 1e-5);
    }

    #[test]
    fn flip_books() {
        let id = AssetId::<StandardMaterial>::default();
        // The consoles' door screen: 3 x 3 frames, six a second.
        let book = FlipBook::parse("flipbook:3:3:6", id, 0).unwrap();
        assert_eq!(book.frame_at(0.0), 0);
        assert_eq!(book.frame_at(0.5), 3);
        assert_eq!(book.frame_at(1.6), 0);
        // Frame 5: the second row's third frame.
        let uv = book
            .frame_transform(5)
            .transform_point2(Vec2::new(1.0, 1.0));
        assert!((uv - Vec2::new(1.0, 2.0 / 3.0)).length() < 1e-6, "{uv:?}");
        let random = FlipBook::parse("flipbook:2:2:2:random", id, 1).unwrap();
        assert!((0..50).all(|i| random.frame_at(i as f32 * 0.5) < 4));
        assert!(FlipBook::parse("flipbook:0:2:2", id, 0).is_none());
        assert!(FlipBook::parse("nofog", id, 0).is_none());
    }
}
