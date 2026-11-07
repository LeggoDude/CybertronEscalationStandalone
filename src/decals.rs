//! The characters' decal sets, as the game draws them (the Customize screen's
//! Decals: TR_CharacterCustomDecals_p's DecalSet01_PART to DecalSet08_PART, the
//! game's TnMeshPartDecals, read by tools/build_decals.py).
//!
//! A set is eight decals, each a small quad (models/decals/decals.glb) and the start
//! of the names of the sockets it hangs on (DecalBack01, DecalFront02...). The game
//! hangs each quad on every socket whose name starts so (MeshPart.
//! TrySocketStaticMeshAttachmentOnComponent: DecalBack01_Left01, DecalBack01_Right01),
//! on the robot's meshes (its chest has them: character.rs's dress_robots) and on its
//! vehicle (dress_cars). [`DecalMaterial`] draws them as the game's
//! TR_Decals1_Mat_INST does (TR_FactionLogo_Mat's graph, in assets/shaders/
//! decal.wgsl): unlit and see-through, the grey sheet's picture lit in the
//! character's energon colour (its faction's: TnCharacterApplier.ApplyColorsToMesh
//! gives every mesh on him his EnergonColor) times LogoIntensity, pulsing as the
//! energon does, and as opaque as 15 times the picture.
//!
//! Like the parts they are on, they fade with the player's own robot as the camera
//! comes close (main.rs's fade_near_camera), and go as the cloak's sweep (cloak.rs)
//! or a body's dissolve (downed.rs) reaches them: [`hide_within`].

use std::collections::HashMap;
use std::path::Path;

use bevy::gltf::{Gltf, GltfMesh, GltfNode};
use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

use crate::character::{AUTOBOT_ENERGON, DECEPTICON_ENERGON, Socket};

/// The sets (tools/build_decals.py): `decal <set id> <socket name start> <mesh>` and
/// `param <name> <value>` lines.
pub const DECALS_FILE: &str = "models/decals/decals.txt";
/// The quads, a root node each named after its mesh.
const DECALS_MODEL: &str = "models/decals/decals.glb";
/// The decals' sheet (Decals_FullSheet).
const SHEET: &str = "models/decals/decals_sheet.ktx2";
const SHADER: &str = "shaders/decal.wgsl";
/// The energon's noise (fractalMaps), as the paint has it (character.rs).
const NOISE_MODEL: &str = "models/props/energon.glb";
/// The material's values the shader takes, in its order (TR_Decals1_Mat_INST's):
/// how often the energon's noise repeats over a decal, its pulse's intensity and
/// contrast, and the picture's brightness.
const PARAMS: [&str; 4] = [
    "EnergonPanUVTile",
    "energonIntensity",
    "energonContrast",
    "LogoIntensity",
];

/// A decal set: its id (customize.txt's part id) and its decals, each the start of
/// the names of the sockets it hangs on and its quad's mesh.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecalSet {
    pub id: String,
    pub decals: Vec<(String, String)>,
}

/// Every decal set, and the material's values ([`PARAMS`]).
#[derive(Clone, Debug, Default)]
pub struct DecalData {
    pub sets: Vec<DecalSet>,
    pub params: Vec4,
}

impl DecalData {
    pub fn read(text: &str) -> DecalData {
        let mut data = DecalData::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            match f.as_slice() {
                ["decal", set, socket, mesh] => {
                    if data.sets.last().is_none_or(|s| s.id != *set) {
                        data.sets.push(DecalSet {
                            id: set.to_string(),
                            decals: Vec::new(),
                        });
                    }
                    if let Some(s) = data.sets.last_mut() {
                        s.decals.push((socket.to_string(), mesh.to_string()));
                    }
                }
                ["param", name, value] => {
                    if let (Some(i), Ok(v)) =
                        (PARAMS.iter().position(|p| p == name), value.parse::<f32>())
                    {
                        data.params[i] = v;
                    }
                }
                _ => {}
            }
        }
        data
    }

    pub fn load(assets: &Path) -> DecalData {
        match std::fs::read_to_string(assets.join(DECALS_FILE)) {
            Ok(text) => DecalData::read(&text),
            Err(_) => {
                info!("{DECALS_FILE} not found (tools/build_decals.py makes it): no decals");
                DecalData::default()
            }
        }
    }

    /// A set's number in a look (character.rs's Look::decals): 1 for the first, 0
    /// for none (DecalSetNone, or a set it doesn't know).
    pub fn number(&self, id: &str) -> u8 {
        self.sets
            .iter()
            .position(|s| s.id == id)
            .map_or(0, |i| (i + 1).min(u8::MAX as usize) as u8)
    }

    /// The set a look's number stands for.
    pub fn set(&self, number: u8) -> Option<&DecalSet> {
        (number as usize)
            .checked_sub(1)
            .and_then(|i| self.sets.get(i))
    }
}

/// The values [`DecalMaterial`] gives its shader.
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct DecalValues {
    /// The energon's colour (linear).
    pub energon: Vec4,
    /// [`PARAMS`].
    pub params: Vec4,
}

/// A decal's material (TR_Decals1_Mat_INST): one for each faction's energon.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct DecalMaterial {
    #[uniform(0)]
    pub values: DecalValues,
    #[texture(1)]
    #[sampler(2)]
    pub sheet: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    pub noise: Handle<Image>,
}

impl Material for DecalMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    /// See-through (its colour comes premultiplied by its opacity).
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// Both sides: the sockets on a character's right are its left ones mirrored
    /// (a negative scale), which turns their quads over.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// A decal hung on a character (a child of its socket's bone).
#[derive(Component)]
pub struct Decal;

/// The decals' model and materials (once [`load_decals`] has asked for them).
#[derive(Resource, Default)]
pub struct DecalAssets {
    pub gltf: Handle<Gltf>,
    /// The Autobots' and the Decepticons'.
    pub materials: [Handle<DecalMaterial>; 2],
}

fn load_decals(
    asset_server: Res<AssetServer>,
    (mut parts, images): (ResMut<crate::gltffiles::GltfParts>, Res<Assets<Image>>),
    chars: Res<crate::character::Characters>,
    mut materials: ResMut<Assets<DecalMaterial>>,
    mut decals: ResMut<DecalAssets>,
) {
    if chars.decals.sets.is_empty() {
        return;
    }
    decals.gltf = asset_server.load(DECALS_MODEL);
    // (Each decal is a cell of the sheet: kept from its neighbours at its edges.)
    let sheet = asset_server.load_with_settings(SHEET, |s: &mut ImageLoaderSettings| {
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            mipmap_filter: ImageFilterMode::Linear,
            ..ImageSamplerDescriptor::linear()
        });
    });
    // (The energon cube's picture: its file read once, gltffiles.rs.)
    let noise = parts.texture(&asset_server, &images, NOISE_MODEL, 0);
    decals.materials = [AUTOBOT_ENERGON, DECEPTICON_ENERGON].map(|energon| {
        materials.add(DecalMaterial {
            values: DecalValues {
                energon: energon.extend(1.0),
                params: chars.decals.params,
            },
            sheet: sheet.clone(),
            noise: noise.clone(),
        })
    });
}

/// Hangs decal set `set`'s decals (a look's number for it) on `sockets` (every one
/// whose name starts with a decal's socket name start), each on its socket's bone
/// among `bones`, in the faction's colour; returns them. None while the decals'
/// model isn't in.
#[allow(clippy::too_many_arguments)]
pub fn hang<'a>(
    commands: &mut Commands,
    chars: &crate::character::Characters,
    assets: &DecalAssets,
    (gltfs, gltf_nodes, gltf_meshes): (&Assets<Gltf>, &Assets<GltfNode>, &Assets<GltfMesh>),
    set: u8,
    decepticon: bool,
    sockets: impl IntoIterator<Item = &'a Socket>,
    bones: &HashMap<String, Entity>,
) -> Vec<Entity> {
    let mut out = Vec::new();
    let Some(set) = chars.decals.set(set) else {
        return out;
    };
    let Some(gltf) = gltfs.get(&assets.gltf) else {
        return out;
    };
    let material = assets.materials[usize::from(decepticon)].clone();
    for s in sockets {
        let Some((_, mesh)) = set
            .decals
            .iter()
            .find(|(start, _)| s.name.starts_with(start.as_str()))
        else {
            continue;
        };
        let Some(&bone) = bones.get(&s.bone) else {
            continue;
        };
        let Some(prims) = gltf
            .named_nodes
            .get(mesh.as_str())
            .and_then(|h| gltf_nodes.get(h))
            .and_then(|n| n.mesh.as_ref())
            .and_then(|h| gltf_meshes.get(h))
        else {
            warn!("{mesh} isn't in {DECALS_MODEL}");
            continue;
        };
        for prim in &prims.primitives {
            out.push(
                commands
                    .spawn((
                        Mesh3d(prim.mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        s.transform,
                        Visibility::Inherited,
                        Name::new(format!("{}_{mesh}", s.name)),
                        Decal,
                        NotShadowCaster,
                        NotShadowReceiver,
                        ChildOf(bone),
                    ))
                    .id(),
            );
        }
    }
    out
}

/// Whether the decals' model is in (a look with decals waits for it).
pub fn ready(assets: &DecalAssets, gltfs: &Assets<Gltf>) -> bool {
    gltfs.get(&assets.gltf).is_some()
}

/// The decals among `entities` within `radius` of `centre` hidden (the cloak's sweep
/// or a dissolve has reached them), the rest shown; with no radius, all shown.
pub fn hide_within<F: bevy::ecs::query::QueryFilter>(
    entities: impl IntoIterator<Item = Entity>,
    decals: &mut Query<(&GlobalTransform, &mut Visibility), F>,
    within: Option<(Vec3, f32)>,
) {
    for e in entities {
        let Ok((at, mut visibility)) = decals.get_mut(e) else {
            continue;
        };
        let gone =
            within.is_some_and(|(centre, radius)| at.translation().distance(centre) < radius);
        visibility.set_if_neq(if gone {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
}

pub struct DecalsPlugin;

impl Plugin for DecalsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DecalAssets>()
            .add_plugins(MaterialPlugin::<DecalMaterial>::default())
            .add_systems(Startup, load_decals);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decal_sets_are_read_in_their_order() {
        let data = DecalData::read(
            "# comment\n\
             decal\tDecalSet01\tDecalBack01\tDecal17_STAT\n\
             decal\tDecalSet01\tDecalFront01\tDecal04_STAT\n\
             decal\tDecalSet02\tDecalBack01\tDecal16_STAT\n\
             param\tLogoIntensity\t2.5\n\
             param\tEnergonPanUVTile\t0.275\n\
             param\tenergonIntensity\t0.6\n\
             param\tenergonContrast\t7.73726\n",
        );
        assert_eq!(data.sets.len(), 2);
        assert_eq!(
            data.sets[0].decals,
            vec![
                ("DecalBack01".to_string(), "Decal17_STAT".to_string()),
                ("DecalFront01".to_string(), "Decal04_STAT".to_string()),
            ]
        );
        assert_eq!(data.params, Vec4::new(0.275, 0.6, 7.73726, 2.5));
        // A look keeps a set as its number: 0 for none.
        assert_eq!(data.number("DecalSet02"), 2);
        assert_eq!(data.number("DecalSetNone"), 0);
        assert_eq!(data.set(2).map(|s| s.id.as_str()), Some("DecalSet02"));
        assert!(data.set(0).is_none() && data.set(3).is_none());
    }

    #[test]
    fn the_games_decal_sets_are_all_there() {
        let data = DecalData::load(&crate::assets_dir());
        assert_eq!(data.sets.len(), 8);
        for (i, set) in data.sets.iter().enumerate() {
            assert_eq!(set.id, format!("DecalSet{:02}", i + 1));
            assert_eq!(set.decals.len(), 8, "{}", set.id);
        }
        assert!(data.params.w > 0.0 && data.params.z > 0.0);
    }
}
