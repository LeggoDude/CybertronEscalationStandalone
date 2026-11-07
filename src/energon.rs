//! The energon cubes: the game's cube (PROP_NEU_EnergonCube_STAT, models/props/
//! energon.glb) drawn the way its materials draw it, for the energon pickups and
//! the cubes over the health stations.
//!
//! Its sides are PROP_NEU_EnergonCube_Clouds_MAT: unlit, a cyan energon colour
//! (its CoreColor, 0, 0.82, 0.82) squared and added to a swirl of colour clouds
//! (fractalMaps, sampled where a second, panning sample of itself pushes it),
//! squared again and four times as bright; where a side faces the camera head on
//! it turns pink and violet (blended, by the tenth power of the facing ratio, into
//! its colour times that power times 2, 0.05, 2 times five). Its edges are
//! PROP_NEU_EnergonCube_EdgeGlow_MAT: unlit white, three times as bright.
//! assets/shaders/energon_cube.wgsl draws the sides; the clouds texture comes with
//! the model (its texture 0).

use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

use crate::gltffiles::GltfParts;

/// The cube's sides' CoreColor (linear).
const CORE_COLOR: Vec3 = Vec3::new(0.0, 0.82, 0.823);
/// The colour the sides take facing the camera head on (the material's second
/// CoreColor, 2, 0.05, 2, times its 5).
const FACE_ON_COLOR: Vec3 = Vec3::new(10.0, 0.25, 10.0);
/// The sides' and the edges' brightness (the materials' Multiply by 4 and by 3).
const SIDES_STRENGTH: f32 = 4.0;
const EDGE_STRENGTH: f32 = 3.0;

/// The cube's glow as a light colour (linear): the sides' average colour.
pub fn glow_color() -> Color {
    Color::linear_rgb(0.125, 0.9, 1.0)
}

pub struct EnergonPlugin;

impl Plugin for EnergonPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<EnergonCubeMaterial>::default())
            .add_systems(Update, update_cube_material);
    }
}

/// The cube's sides (PROP_NEU_EnergonCube_Clouds_MAT; see the top of this file and
/// assets/shaders/energon_cube.wgsl).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct EnergonCubeMaterial {
    /// rgb: CoreColor; a unused.
    #[uniform(0)]
    core: Vec4,
    /// rgb: the face-on colour; a: the brightness.
    #[uniform(1)]
    face: Vec4,
    /// The game camera's fog: rgb its colour (linear), a how much of it shows.
    #[uniform(2)]
    fog: Vec4,
    /// x unused (the clouds pan by Bevy's clock, in the shader); y: the fog's kind
    /// (0 none, 1 linear, 2 exponential, 3 exponential squared); z, w: its
    /// density, or its start and end (linear).
    #[uniform(3)]
    params: Vec4,
    #[texture(4)]
    #[sampler(5)]
    clouds: Handle<Image>,
}

impl Material for EnergonCubeMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/energon_cube.wgsl".into()
    }
}

/// How the energon cubes are drawn: the game's cube, its sides in their material and
/// its edges glowing white, or a plain glowing block where the model hasn't been
/// built.
#[derive(Clone)]
pub enum CubeLook {
    Model {
        sides: Handle<Mesh>,
        edges: Handle<Mesh>,
        sides_material: Handle<EnergonCubeMaterial>,
        edge_material: Handle<StandardMaterial>,
    },
    Block {
        mesh: Handle<Mesh>,
        material: Handle<StandardMaterial>,
    },
}

impl CubeLook {
    /// The cube from `model` (its first mesh: the sides, then the edges, and the
    /// clouds texture, the file read once: gltffiles.rs), or the block when `model`
    /// is None.
    pub fn new(
        model: Option<&'static str>,
        (asset_server, parts, images): (&AssetServer, &mut GltfParts, &Assets<Image>),
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        cube_materials: &mut Assets<EnergonCubeMaterial>,
    ) -> CubeLook {
        let Some(model) = model else {
            return CubeLook::Block {
                mesh: meshes.add(Cuboid::new(1.4, 1.4, 1.4)),
                material: materials.add(StandardMaterial {
                    base_color: Color::BLACK,
                    emissive: LinearRgba::rgb(0.27, 1.93, 2.15),
                    ..default()
                }),
            };
        };
        let sides = parts.mesh(asset_server, meshes, model, 0, 0);
        let edges = parts.mesh(asset_server, meshes, model, 0, 1);
        let clouds = parts.texture(asset_server, images, model, 0);
        CubeLook::Model {
            sides,
            edges,
            sides_material: cube_materials.add(EnergonCubeMaterial {
                core: CORE_COLOR.extend(1.0),
                face: FACE_ON_COLOR.extend(SIDES_STRENGTH),
                fog: Vec4::ZERO,
                params: Vec4::ZERO,
                clouds,
            }),
            edge_material: materials.add(StandardMaterial {
                base_color: Color::linear_rgb(EDGE_STRENGTH, EDGE_STRENGTH, EDGE_STRENGTH),
                unlit: true,
                ..default()
            }),
        }
    }

    /// Gives `cube` (an entity with a transform and visibility) the cube to show.
    pub fn attach(&self, cube: &mut EntityCommands) {
        match self {
            CubeLook::Model {
                sides,
                edges,
                sides_material,
                edge_material,
            } => {
                cube.with_child((
                    Mesh3d(sides.clone()),
                    MeshMaterial3d(sides_material.clone()),
                ));
                cube.with_child((Mesh3d(edges.clone()), MeshMaterial3d(edge_material.clone())));
            }
            CubeLook::Block { mesh, material } => {
                cube.insert((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone())));
            }
        }
    }
}

/// A camera's fog (or none) as the shaders that fog themselves take it: its colour
/// (linear rgb, a how much of it shows), and its kind (y: 0 none, 1 linear, 2
/// exponential, 3 exponential squared) with its density, or its start and end (z, w).
pub fn fog_uniforms(fog: Option<&DistanceFog>) -> (Vec4, Vec4) {
    let (color, kind, a, b) = match fog {
        Some(fog) => {
            let (kind, a, b) = match fog.falloff {
                FogFalloff::Linear { start, end } => (1.0, start, end),
                FogFalloff::Exponential { density } => (2.0, density, 0.0),
                FogFalloff::ExponentialSquared { density } => (3.0, density, 0.0),
                // (The mean of its extinction, as an exponential fog.)
                FogFalloff::Atmospheric { extinction, .. } => {
                    (2.0, (extinction.x + extinction.y + extinction.z) / 3.0, 0.0)
                }
            };
            let c = fog.color.to_linear();
            (Vec4::new(c.red, c.green, c.blue, c.alpha), kind, a, b)
        }
        None => (Vec4::ZERO, 0.0, 0.0, 0.0),
    };
    (color, Vec4::new(0.0, kind, a, b))
}

/// Fogs the cubes as the game camera's fog fogs everything else (the shader draws
/// them itself, so Bevy's fog doesn't reach them). The material changes only when
/// the fog does: a changed material is prepared again, and its cubes' pipelines
/// looked up again, the frame it changes.
fn update_cube_material(
    camera: Query<Option<&DistanceFog>, (With<Camera3d>, With<crate::OrbitCamera>)>,
    cubes: Query<&MeshMaterial3d<EnergonCubeMaterial>>,
    mut materials: ResMut<Assets<EnergonCubeMaterial>>,
) {
    // (Every cube shares the one material.)
    let Some(cube) = cubes.iter().next() else {
        return;
    };
    let (fog, params) = fog_uniforms(camera.iter().next().flatten());
    let changed = materials
        .get(&cube.0)
        .is_some_and(|m| m.fog != fog || m.params != params);
    if changed && let Some(m) = materials.get_mut(&cube.0) {
        m.fog = fog;
        m.params = params;
    }
}
