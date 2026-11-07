//! Models' scenes, meshes and pictures from glTF files, each file read once.
//!
//! Bevy reads a whole glTF file for each part of it asked for by its label
//! (`robot.glb#Scene0`, `energon.glb#Mesh0/Primitive1`) before the file is in, and
//! again for the file itself (asked for whole for its clips: main.rs's
//! [`crate::GltfClips`]), every part already in made again and every scene of it in
//! play spawned again. So a file is asked for whole, once, and kept from then on:
//!
//! * a model's scene is its [`SceneOf`]: the file and the scene's number in it, put
//!   in as its SceneRoot as soon as the file is in (before the scenes are spawned
//!   that frame: a file already in gives it at once);
//! * a mesh or picture from a file is a handle of its own, filled with a copy of the
//!   file's once the file is in ([`GltfParts`]).

use std::collections::{HashMap, HashSet};

use bevy::asset::{AssetPath, UntypedAssetId};
use bevy::gltf::{Gltf, GltfMesh};
use bevy::prelude::*;

pub struct GltfFilesPlugin;

impl Plugin for GltfFilesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GltfParts>()
            .add_systems(
                SpawnScene,
                fill_scenes.before(bevy::scene::SceneSpawnerSystems::Spawn),
            )
            .add_systems(Update, fill_parts);
    }
}

/// A model's scene from its glTF file (the scene's number there), its SceneRoot once
/// the file is in.
#[derive(Component, Clone, Debug)]
pub struct SceneOf {
    pub file: Handle<Gltf>,
    pub scene: usize,
}

impl SceneOf {
    /// The first scene of the file at `path` (the file asked for whole).
    pub fn load<'a>(asset_server: &AssetServer, path: impl Into<AssetPath<'a>>) -> Self {
        SceneOf {
            file: asset_server.load(path),
            scene: 0,
        }
    }
}

/// The models whose files are in given their scenes. The files are kept from then on
/// (let go and asked for again, a file would be read again and its scenes in play
/// spawned again).
fn fill_scenes(
    mut commands: Commands,
    pending: Query<(Entity, &SceneOf)>,
    gltfs: Res<Assets<Gltf>>,
    asset_server: Res<AssetServer>,
    mut kept: Local<HashMap<AssetId<Gltf>, Handle<Gltf>>>,
) {
    for (entity, of) in &pending {
        let scene = match gltfs.get(&of.file) {
            Some(gltf) => gltf.scenes.get(of.scene).cloned(),
            None if asset_server.load_state(&of.file).is_failed() => None,
            None => continue,
        };
        let first = !kept.contains_key(&of.file.id());
        kept.entry(of.file.id()).or_insert_with(|| of.file.clone());
        let mut e = commands.entity(entity);
        e.try_remove::<SceneOf>();
        match scene {
            Some(scene) => {
                e.try_insert(SceneRoot(scene));
            }
            None if first => warn!(
                "{:?} has no scene {} (or can't be read): its models are left out",
                asset_server.get_path(&of.file),
                of.scene
            ),
            None => {}
        }
    }
}

/// A part of a glTF file: a mesh (its mesh's number, and its primitive's) or a
/// picture (its texture's number).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Part {
    Mesh(usize, usize),
    Texture(usize),
}

/// Meshes and pictures from glTF files, each a handle of its own (the same for the
/// same part), filled with a copy of the file's once the file is in. The files are
/// kept.
#[derive(Resource, Default)]
pub struct GltfParts {
    pending: Vec<(Handle<Gltf>, Part, UntypedHandle)>,
    made: HashMap<(AssetId<Gltf>, Part), UntypedHandle>,
    kept: HashSet<Handle<Gltf>>,
}

impl GltfParts {
    /// Mesh `mesh`'s primitive `primitive` of the file at `path`.
    pub fn mesh(
        &mut self,
        asset_server: &AssetServer,
        meshes: &Assets<Mesh>,
        path: &str,
        mesh: usize,
        primitive: usize,
    ) -> Handle<Mesh> {
        self.part(asset_server, path, Part::Mesh(mesh, primitive), || {
            meshes.reserve_handle().untyped()
        })
        .typed()
    }

    /// Texture `index` of the file at `path` (a picture in it, as its label
    /// `Texture<index>` has it).
    pub fn texture(
        &mut self,
        asset_server: &AssetServer,
        images: &Assets<Image>,
        path: &str,
        index: usize,
    ) -> Handle<Image> {
        self.part(asset_server, path, Part::Texture(index), || {
            images.reserve_handle().untyped()
        })
        .typed()
    }

    fn part(
        &mut self,
        asset_server: &AssetServer,
        path: &str,
        part: Part,
        reserve: impl FnOnce() -> UntypedHandle,
    ) -> UntypedHandle {
        let file: Handle<Gltf> = asset_server.load(path.to_string());
        if let Some(h) = self.made.get(&(file.id(), part)) {
            return h.clone();
        }
        let h = reserve();
        self.made.insert((file.id(), part), h.clone());
        self.pending.push((file, part, h.clone()));
        h
    }
}

/// The parts whose files are in, filled.
fn fill_parts(
    mut parts: ResMut<GltfParts>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
) {
    if parts.pending.is_empty() {
        return;
    }
    let parts = &mut *parts;
    let kept = &mut parts.kept;
    parts.pending.retain(|(file, part, into)| {
        let Some(gltf) = gltfs.get(file) else {
            if asset_server.load_state(file).is_failed() {
                warn!(
                    "{:?} can't be read: its {part:?} is left out",
                    asset_server.get_path(file)
                );
                return false;
            }
            return true;
        };
        let id: UntypedAssetId = into.id();
        let done = match *part {
            Part::Mesh(m, p) => gltf
                .meshes
                .get(m)
                .and_then(|h| gltf_meshes.get(h))
                .and_then(|g| g.primitives.get(p))
                .and_then(|p| meshes.get(&p.mesh))
                .cloned()
                .is_some_and(|mesh| meshes.insert(id.typed::<Mesh>(), mesh).is_ok()),
            // (The file's pictures are its materials': kept with it.)
            Part::Texture(i) => asset_server
                .get_path(file)
                .map(|p| p.into_owned().with_label(format!("Texture{i}")))
                .and_then(|p| asset_server.get_handle::<Image>(p))
                .and_then(|h| images.get(&h))
                .cloned()
                .is_some_and(|image| images.insert(id.typed::<Image>(), image).is_ok()),
        };
        if !done {
            warn!(
                "{:?} has no {part:?}: left out",
                asset_server.get_path(file)
            );
        }
        kept.insert(file.clone());
        false
    });
}
