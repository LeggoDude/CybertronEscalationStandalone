//! The characters' shadows from the lights other than the sun: the game's light
//! environments' composite shadows.
//!
//! In the game every character's form has a light environment
//! (TnRobotForm's LightEnvironment0, TnVehicleForm's MyLightEnv, TnInsectForm's:
//! DynamicLightEnvironmentComponents, which change only how often they update,
//! UpdateDistanceThreshold 30, and where they sample the light from), and the PC
//! version has LightEnvironmentShadows on: the environment gathers the level's
//! lights round the character (each seen from where it stands, by ray casts), and
//! casts one soft shadow along the strongest direction they come from, never lower
//! than MinShadowAngle (35 degrees) over the horizon, as dark as that direction's
//! share of the light (a modulated shadow: what it falls on is darkened, not lit
//! less). The sun casts its own shadows (the map's DominantDirectionalLight's,
//! lighting.rs), so it is left out of the environment; and where it reaches a
//! character, its shadow is the character's (the dominant light casts it), not the
//! environment's as well: in the sun a character casts the sun's shadow alone, in
//! the shade the lights' (`sun_probe`: the shader looks at the sun's shadow map
//! just out of its body towards the sun, and lets the lights' shadow give way as
//! much as the sun reaches it there).
//!
//! Here each character near the camera (the nearest `Level::character_shadows`,
//! graphics.rs) casts such a shadow of its own shape, as the game's do (a shadow
//! map of the character alone, from the light's way: the game's per-object
//! projected shadows). Each has a light of its own (`ShadowLamp`): a spot light that
//! gives no light, far off the way the light comes from with its cone round the
//! character, casting a shadow of nothing but that character (its meshes are put on
//! a render layer of its own, `SHADOW_LAYER`, which only that light's shadow sees),
//! so Bevy draws the character's shadow map from there. A box round the space the
//! shadow can fall in is drawn over the picture, multiplying it (`ShadowMaterial`):
//! its shader finds each pixel's place from the depth prepass, finds the light (by
//! its tag, among the lights of the pixel's cluster) and looks the pixel up in its
//! shadow map: shaded where the character stands between it and the light, softly
//! at the edges, more softly the further the shadow falls behind the part casting
//! it. The character's own surface is left as it is (its own shading has its dark
//! side): what is within a little of the body's capsules, or of a part only a hand's
//! breadth in front of it.
//!
//! The capsules are the body's: its physics asset's shapes (an enemy's hit shapes,
//! a player's ragdoll bodies). They tell where the character is and how big, where
//! its light is gathered from, and what of the picture is the character itself. A
//! player's vehicle has no such shapes (its physics asset is one body round all of
//! it), so its capsules are fitted to its model (`fit_bones`): to the vertices each
//! bone carries, the bones grouped into as few capsules as keep close to its shape,
//! and to the parts hung on it (a jet's wings, a tank's cannons), each moving with
//! its bone.

use crate::lighting::MapLighting;
use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::mesh::VertexAttributeValues;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{MeshTag, MeshVertexBufferLayoutRef};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    AsBindGroup, BufferUsages, CompareFunction, Face, RenderPipelineDescriptor, ShaderType,
    SpecializedMeshPipelineError, encase,
};
use bevy::render::renderer::RenderQueue;
use bevy::render::storage::{GpuShaderStorageBuffer, ShaderStorageBuffer};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use bevy::scene::{InstanceId, SceneInstance, SceneSpawner};
use bevy::shader::ShaderRef;

/// The most capsules a shadow is cast from.
pub const MAX_CAPSULES: usize = 16;
/// Characters further than this from the camera (m) cast none; their shadows fade
/// out over the last SHADOW_FADE.
const SHADOW_DISTANCE: f32 = 45.0;
const SHADOW_FADE: f32 = 10.0;
/// MinShadowAngle: the shadow's light is never lower than this over the horizon.
const MIN_SHADOW_ANGLE: f32 = 35.0;
/// How far below the body a shadow can fall (m): its light's MinShadowAngle and a
/// tall Titan's height.
const SHADOW_REACH: f32 = 12.0;
/// The light is gathered again once the character has moved this far (m; the
/// game's UpdateDistanceThreshold is 30 uu, its traces cheaper than these), and at
/// least this often (s), as lights come and go; the strongest RAY_LIGHTS lights
/// are checked for walls in the way (the weaker count as they are).
const UPDATE_DISTANCE: f32 = 1.0;
const UPDATE_SECS: f32 = 0.5;
const RAY_LIGHTS: usize = 4;
/// The darkest a shadow is (all the light from one way), and how soft its edge is
/// (m) where it touches what casts it, growing by SOFT_PER_M for each metre it falls
/// behind it, to at most MAX_BLUR.
const MAX_DARKNESS: f32 = 0.6;
const SOFTNESS: f32 = 0.04;
const SOFT_PER_M: f32 = 0.06;
const MAX_BLUR: f32 = 0.35;
/// What is less than this behind the part of the character the light meets first
/// (m) isn't in its shadow: the character's own surface (and its own shading's dark
/// side), not shaded by itself.
const MIN_GAP: f32 = 0.15;
/// A shadow's light stands this far off the character (m), towards its light.
const LAMP_DISTANCE: f32 = 20.0;
/// The render layers of the characters casting shadows, one each (their slots'),
/// from this one: a shadow's light sees only its own.
pub const SHADOW_LAYER: usize = 20;
/// The most slots (Extra's character_shadows and to spare).
pub const MAX_SLOTS: usize = 16;
/// A shadow's light's tag, its radius (which a light giving no light has no use
/// for): this plus its slot. The shader finds its light by it.
const LAMP_TAG: f32 = 4096.0;

/// A character's shadow: its light (towards it), how dark it is, its capsules.
#[derive(ShaderType, Clone, Debug, PartialEq)]
pub struct ShadowParams {
    /// Towards the light (unit), and how far the shadow can fall (m).
    pub dir: Vec4,
    /// Darkness, edge softness (m), its growth per metre, capsule count.
    pub shade: Vec4,
    /// Each capsule's ends (xyz) and, in `a.w`, its radius.
    pub a: [Vec4; MAX_CAPSULES],
    pub b: [Vec4; MAX_CAPSULES],
    /// Where to look whether the sun reaches it (xyz), and in w 1 on a map with a
    /// sun (sun_probe).
    pub sun: Vec4,
    /// Its light (`ShadowLamp`): where it stands, and in w its tag.
    pub lamp: Vec4,
    /// Its light's lens: the tangent of its cone's half angle, its near plane (m),
    /// MIN_GAP and MAX_BLUR.
    pub lens: Vec4,
}

impl Default for ShadowParams {
    fn default() -> Self {
        ShadowParams {
            dir: Vec4::new(0.0, 1.0, 0.0, SHADOW_REACH),
            shade: Vec4::new(0.0, SOFTNESS, SOFT_PER_M, 0.0),
            a: [Vec4::ZERO; MAX_CAPSULES],
            b: [Vec4::ZERO; MAX_CAPSULES],
            sun: Vec4::ZERO,
            lamp: Vec4::ZERO,
            lens: Vec4::new(0.0, 0.0, MIN_GAP, MAX_BLUR),
        }
    }
}

/// Where a shadow's light stands and how it sees: its place, its cone's half angle
/// (rad), its near plane and its reach (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lamp {
    pub at: Vec3,
    pub angle: f32,
    pub near: f32,
    pub range: f32,
}

/// A shadow's light for a body (its capsules) and the way to its light: off that
/// way by LAMP_DISTANCE, its cone round the body and its widest blur, its near plane
/// in front of the body, its reach past where the shadow can fall.
pub fn lamp_for(capsules: &[Capsule], dir: Vec3) -> Lamp {
    let (c, r) = bounds_of(capsules);
    let half = r + MAX_BLUR + 0.25;
    Lamp {
        at: c + dir * LAMP_DISTANCE,
        angle: (half / LAMP_DISTANCE).atan(),
        near: (LAMP_DISTANCE - r - 0.5).max(0.1),
        range: LAMP_DISTANCE + r + SHADOW_REACH + 1.0,
    }
}

/// Where a point is on a shadow's light's shadow map (uv, 0 to 1 across it) and
/// how far it is along the light's axis (m), as Bevy draws a spot light's shadow
/// map (spot_light_world_from_view, spot_light_clip_from_view) and the shader
/// looks it up: `dir` towards the light (its transform's back). (The shader's
/// `lamp_view`, checked here against Bevy's own.)
#[cfg(test)]
pub fn lamp_view(lamp: &Lamp, dir: Vec3, p: Vec3) -> Vec3 {
    let basis = bevy::light::orthonormalize(Dir3::new(dir).unwrap_or(Dir3::Y));
    let v = p - lamp.at;
    let q = Vec3::new(
        v.dot(basis.x_axis),
        v.dot(basis.y_axis),
        v.dot(basis.z_axis),
    );
    let z = -q.z;
    let ndc = Vec2::new(q.x, q.y) / (lamp.angle.tan() * z);
    (ndc * Vec2::new(0.5, -0.5) + Vec2::splat(0.5)).extend(z)
}

/// The boxes the shadows are drawn in (shaders/character_shadow.wgsl): one for them
/// all, each box's shadow read by its mesh tag (its slot) from the slots' shadows
/// (SlotShadows).
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct ShadowMaterial {
    /// The slots' shadows: always SLOT_SHADOWS_BUFFER.
    #[storage(0, read_only, visibility(fragment))]
    pub slots: Handle<ShaderStorageBuffer>,
}

impl Default for ShadowMaterial {
    fn default() -> Self {
        ShadowMaterial {
            slots: SLOT_SHADOWS_BUFFER,
        }
    }
}

impl Material for ShadowMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/character_shadow.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Multiply
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// Its back faces, whatever is in front of them: each pixel the box covers is
    /// drawn once, the camera inside it or not (the shader finds what is there).
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = Some(Face::Front);
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = false;
            depth.depth_compare = CompareFunction::Always;
        }
        Ok(())
    }
}

/// A capsule: its ends and radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capsule {
    pub a: Vec3,
    pub b: Vec3,
    pub r: f32,
}

impl Capsule {
    /// A physics shape as a capsule round it: a sphere, a capsule; a box along its
    /// longest side; any other its bounding sphere.
    pub fn of(shape: &crate::ragdoll::Shape) -> Capsule {
        use crate::ragdoll::Shape;
        match shape {
            Shape::Sphere { c, r } => Capsule {
                a: *c,
                b: *c,
                r: *r,
            },
            Shape::Capsule { a, b, r } => Capsule {
                a: *a,
                b: *b,
                r: *r,
            },
            Shape::Box { c, axes, half } => {
                let i = if half.x >= half.y && half.x >= half.z {
                    0
                } else if half.y >= half.z {
                    1
                } else {
                    2
                };
                let h = half.to_array();
                let r = (h[(i + 1) % 3].powi(2) + h[(i + 2) % 3].powi(2)).sqrt();
                let along = axes[i] * (h[i] - r).max(0.0);
                Capsule {
                    a: *c - along,
                    b: *c + along,
                    r,
                }
            }
            other => {
                let (c, r) = other.bounds();
                Capsule { a: c, b: c, r }
            }
        }
    }

    fn bounds(&self) -> (Vec3, f32) {
        (
            (self.a + self.b) * 0.5,
            self.a.distance(self.b) * 0.5 + self.r,
        )
    }
}

/// The level's light at `at` (Bevy's axes), from the lights round it that reach
/// it unblocked (`blocked`: whether a wall stands between): the strongest way it
/// comes from (unit, towards the lights; never lower than MinShadowAngle) and that
/// way's share of it (0 to 1).
pub fn composite_light(
    lights: &[crate::lighting::MapLight],
    at: Vec3,
    mut blocked: impl FnMut(Vec3, Vec3) -> bool,
) -> Option<(Vec3, f32)> {
    // Each light's strength here, and which way it comes from.
    let mut reaching: Vec<(f32, Vec3, Vec3)> = Vec::new();
    for light in lights {
        let to = light.position - at;
        let d = to.length();
        if d >= light.radius || d < 1e-3 {
            continue;
        }
        let k = d / light.radius;
        let mut w = light.brightness.max(0.0) * (1.0 - k * k).max(0.0).powf(light.falloff.max(0.0));
        if let Some((dir, inner, outer)) = light.spot {
            // (Inside its cone, softly to its edge.)
            let cos = (-to / d).dot(dir.normalize_or_zero());
            let (ci, co) = (inner.to_radians().cos(), outer.to_radians().cos());
            w *= ((cos - co) / (ci - co).max(1e-4)).clamp(0.0, 1.0);
        }
        if w > 1e-4 {
            reaching.push((w, to / d, light.position));
        }
    }
    reaching.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut sum = Vec3::ZERO;
    let mut total = 0.0;
    for (i, &(w, dir, position)) in reaching.iter().enumerate() {
        if i < RAY_LIGHTS && blocked(at, position) {
            continue;
        }
        sum += dir * w;
        total += w;
    }
    if total <= 1e-4 || sum.length() <= 1e-4 {
        return None;
    }
    let share = (sum.length() / total).clamp(0.0, 1.0);
    let mut dir = sum.normalize();
    // (MinShadowAngle: no lower than that over the horizon.)
    let min = MIN_SHADOW_ANGLE.to_radians().sin();
    if dir.y < min {
        let flat = Vec2::new(dir.x, dir.z).normalize_or(Vec2::X) * (1.0 - min * min).sqrt();
        dir = Vec3::new(flat.x, min, flat.y);
    }
    Some((dir, share))
}

/// A character's gathered light, kept till it has moved UPDATE_DISTANCE or
/// UPDATE_SECS have passed.
#[derive(Component)]
struct Gathered {
    at: Vec3,
    when: f32,
    light: Option<(Vec3, f32)>,
}

/// A shadow's slot: its box (tagged with the slot), its light, the character
/// casting it (none: free) and that one's meshes put on the slot's layer.
struct Slot {
    shade: Entity,
    lamp: Entity,
    caster: Option<Entity>,
    meshes: Vec<Entity>,
}

/// The shadows' slots, by the order they're given out.
#[derive(Resource, Default)]
struct ShadowPool(Vec<Slot>);

/// A shadow's light: a spot light giving no light, on its slot's layer, whose
/// shadow map is its character's alone.
#[derive(Component)]
struct ShadowLamp;

/// A mesh of a character casting a shadow, put on its slot's layer: that layer and
/// the layers it had.
#[derive(Component)]
struct OnShadowLayer {
    layer: usize,
    before: Option<RenderLayers>,
}

pub struct ShadowsPlugin;

impl Plugin for ShadowsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<ShadowMaterial>::default())
            .init_resource::<ShadowPool>()
            .init_resource::<SlotShadows>()
            .add_systems(Startup, make_slot_shadows)
            .add_systems(
                PostUpdate,
                (
                    fit_car_shadows
                        .after(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
                    cast_shadows
                        .after(bevy::transform::TransformSystems::Propagate)
                        .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
                        .before(bevy::light::SimulationLightSystems::AssignLightsToClusters)
                        .before(bevy::light::SimulationLightSystems::UpdateLightFrusta),
                )
                    .chain(),
            );
        // The slots' shadows, copied to the render world and written to their
        // buffer as they change (SlotShadows).
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<SlotShadowsRender>()
            .add_systems(ExtractSchedule, extract_slot_shadows)
            .add_systems(
                Render,
                write_slot_shadows.in_set(RenderSystems::PrepareResources),
            );
    }
}

/// The slots' shadows' buffer (SlotShadows): one, fixed, so the boxes' material can
/// bind it.
const SLOT_SHADOWS_BUFFER: Handle<ShaderStorageBuffer> =
    bevy::asset::uuid_handle!("5b7c1e93-2f48-4d6a-9e15-a3c08d7f6b21");

/// Each slot's shadow (the shader's ShadowParams), read by its box (by its mesh
/// tag, the slot). cast_shadows sets them each frame, and the render world writes
/// them to their buffer as they change. They were each box's own material's: a
/// material changed is one prepared again for the GPU (its values' buffer made
/// anew, and its bind group, and the box's draw worked out again), and a shadow
/// moves with its character every frame.
#[derive(Resource)]
pub struct SlotShadows(Vec<ShadowParams>);

impl Default for SlotShadows {
    fn default() -> Self {
        SlotShadows(vec![ShadowParams::default(); MAX_SLOTS])
    }
}

/// The render world's copy of the slots' shadows, as the buffer has them (and
/// whether it is still to be written).
#[derive(Resource, Default)]
struct SlotShadowsRender {
    bytes: Vec<u8>,
    fresh: bool,
}

/// The slots' shadows as their buffer has them: each as the shader's ShadowParams
/// (an array's element).
fn slot_shadow_bytes(shadows: &[ShadowParams]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for p in shadows {
        let mut one = encase::StorageBuffer::new(Vec::<u8>::new());
        if one.write(p).is_ok() {
            bytes.extend_from_slice(one.as_ref());
        }
    }
    bytes
}

/// Makes the slots' shadows' buffer.
fn make_slot_shadows(storage: Option<ResMut<Assets<ShaderStorageBuffer>>>) {
    let Some(mut storage) = storage else {
        return;
    };
    let size = slot_shadow_bytes(&SlotShadows::default().0).len();
    let mut buffer = ShaderStorageBuffer::with_size(size, RenderAssetUsages::default());
    buffer.buffer_description.label = Some("slot_shadows");
    // (Written as they change: write_slot_shadows.)
    buffer.buffer_description.usage |= BufferUsages::COPY_DST;
    let _ = storage.insert(SLOT_SHADOWS_BUFFER.id(), buffer);
}

/// Copies the slots' shadows to the render world when they have changed.
fn extract_slot_shadows(shadows: Extract<Res<SlotShadows>>, mut render: ResMut<SlotShadowsRender>) {
    if shadows.is_changed() {
        render.bytes = slot_shadow_bytes(&shadows.0);
        render.fresh = true;
    }
}

/// Writes the slots' shadows to their buffer, once it is made.
fn write_slot_shadows(
    mut shadows: ResMut<SlotShadowsRender>,
    buffers: Res<RenderAssets<GpuShaderStorageBuffer>>,
    queue: Res<RenderQueue>,
) {
    if !shadows.fresh {
        return;
    }
    let Some(gpu) = buffers.get(SLOT_SHADOWS_BUFFER.id()) else {
        return;
    };
    shadows.fresh = false;
    let n = shadows.bytes.len().min(gpu.buffer.size() as usize);
    queue.write_buffer(&gpu.buffer, 0, &shadows.bytes[..n]);
}

/// The layers of all the slots.
fn shadow_layers() -> RenderLayers {
    RenderLayers::from_layers(&(SHADOW_LAYER..SHADOW_LAYER + MAX_SLOTS).collect::<Vec<_>>())
}

/// Which slot each caster takes: the one it had, or else the first free (one whose
/// caster no longer casts), in the order given (the nearest first). `slots`: each
/// slot's caster.
pub fn assign_slots(slots: &[Option<Entity>], casters: &[Entity]) -> Vec<Option<usize>> {
    let mut taken = vec![false; slots.len()];
    let mut out: Vec<Option<usize>> = casters
        .iter()
        .map(|c| {
            let s = slots.iter().position(|s| *s == Some(*c))?;
            taken[s] = true;
            Some(s)
        })
        .collect();
    for o in out.iter_mut().filter(|o| o.is_none()) {
        if let Some(s) = (0..slots.len()).find(|&s| !taken[s]) {
            taken[s] = true;
            *o = Some(s);
        }
    }
    out
}

/// Each character near the camera casts its shadow (the nearest
/// `character_shadows`): its light gathered (as it moves), its body's capsules, its
/// meshes on its slot's layer, its slot's light off the way the light comes from,
/// a box round where the shadow can fall.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn cast_shadows(
    mut commands: Commands,
    time: Res<Time>,
    quality: Res<crate::graphics::GraphicsQuality>,
    map: Option<Res<MapLighting>>,
    level: Option<Res<crate::Level>>,
    camera: Query<
        (Entity, &GlobalTransform, Option<&RenderLayers>),
        (
            With<crate::OrbitCamera>,
            Without<ShadowBox>,
            Without<ShadowLamp>,
        ),
    >,
    enemies: Query<(Entity, &crate::enemies::Enemy, Option<&Gathered>)>,
    players: Query<(Entity, &crate::Player, &Transform, Option<&Gathered>)>,
    (robots, cars, shown): (
        Query<(&crate::JazzOf, &crate::ragdoll::Rig), With<crate::RobotForm>>,
        Query<(&crate::JazzOf, &CarShadow), With<crate::VehicleForm>>,
        Query<&InheritedVisibility>,
    ),
    globals: Query<&GlobalTransform, (Without<ShadowBox>, Without<ShadowLamp>)>,
    mut pool: ResMut<ShadowPool>,
    (mut boxes, mut lamps): (
        Query<
            (&mut Transform, &mut GlobalTransform, &mut Visibility),
            (With<ShadowBox>, Without<crate::Player>, Without<ShadowLamp>),
        >,
        Query<
            (
                &mut SpotLight,
                &mut Transform,
                &mut GlobalTransform,
                &mut Visibility,
            ),
            (With<ShadowLamp>, Without<crate::Player>, Without<ShadowBox>),
        >,
    ),
    (children, mesh_layers, on_layer): (
        Query<&Children>,
        Query<
            (Option<&RenderLayers>, Option<&OnShadowLayer>),
            (With<Mesh3d>, Without<NotShadowCaster>),
        >,
        Query<&OnShadowLayer>,
    ),
    (mut materials, mut shadows): (ResMut<Assets<ShadowMaterial>>, ResMut<SlotShadows>),
    mut meshes: ResMut<Assets<Mesh>>,
    (mut cube, mut material): (
        Local<Option<Handle<Mesh>>>,
        Local<Option<Handle<ShadowMaterial>>>,
    ),
) {
    let limit = quality.level().character_shadows.min(MAX_SLOTS);
    let view = camera.iter().next();
    // (The cameras see the slots' lights, so their clusters hold them: split
    // screen's two.)
    for (entity, _, layers) in &camera {
        let all = shadow_layers();
        let had = layers.cloned().unwrap_or_default();
        if !had.intersects(&RenderLayers::layer(SHADOW_LAYER + MAX_SLOTS - 1)) {
            commands.entity(entity).try_insert(had.union(&all));
        }
    }
    let eye = view.map(|(_, t, _)| (t.translation(), t.forward().as_vec3()));
    // (The characters near either camera, with two players on one screen.)
    let eyes: Vec<Vec3> = camera.iter().map(|(_, t, _)| t.translation()).collect();
    let (Some(map), Some((eye, ahead))) = (map.filter(|_| limit > 0), eye) else {
        for (k, slot) in pool.0.iter_mut().enumerate() {
            release(&mut commands, k, slot, &on_layer, &mut boxes, &mut lamps);
        }
        return;
    };
    let now = time.elapsed_secs();
    // The characters near enough: their bodies as capsules.
    let mut casters: Vec<(f32, Entity, Vec<Capsule>, Option<&Gathered>)> = Vec::new();
    for (entity, enemy, gathered) in &enemies {
        let capsules: Vec<Capsule> = enemy.body.shapes().map(Capsule::of).collect();
        push_caster(&mut casters, &eyes, entity, capsules, gathered);
    }
    for (entity, player, transform, gathered) in &players {
        let capsules = match player.form {
            crate::Form::Robot => robots
                .iter()
                .find(|(of, _)| of.0 == entity)
                .map(|(_, rig)| rig_capsules(rig, &globals))
                .unwrap_or_default(),
            // His vehicle model's capsules (fit_car_shadows), where its bones are;
            // till they're fitted (or with no model to fit them to), one round
            // his collision.
            crate::Form::Vehicle => cars
                .iter()
                .find(|(of, _)| of.0 == entity)
                .map(|(_, shadow)| shadow.placed(&globals, &shown))
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| {
                    let (radius, height) = player.size();
                    let r = height * 0.5;
                    let up = transform.translation + Vec3::Y * r;
                    let along = transform.forward().as_vec3() * (radius - r).max(0.0);
                    vec![Capsule {
                        a: up - along,
                        b: up + along,
                        r,
                    }]
                }),
        };
        push_caster(&mut casters, &eyes, entity, capsules, gathered);
    }
    casters.sort_by(|a, b| a.0.total_cmp(&b.0));
    casters.truncate(limit);

    // Each one's shadow: its light, gathered again once it has moved or a while
    // has passed, and how dark.
    let mut casts: Vec<(Entity, Vec<Capsule>, Vec3, ShadowParams)> = Vec::new();
    for (distance, entity, capsules, gathered) in casters {
        let (centre, _) = bounds_of(&capsules);
        let light = match gathered {
            Some(g) if g.at.distance(centre) < UPDATE_DISTANCE && now - g.when < UPDATE_SECS => {
                g.light
            }
            _ => {
                let light = composite_light(&map.lights, centre, |from, to| {
                    level.as_ref().is_some_and(|l| {
                        let d = to - from;
                        let len = d.length();
                        len > 0.5
                            && l.ray_hit_for(
                                from,
                                d / len,
                                len - 0.25,
                                crate::collision::Mover::Player,
                            )
                            .is_some()
                    })
                });
                commands.entity(entity).try_insert(Gathered {
                    at: centre,
                    when: now,
                    light,
                });
                light
            }
        };
        let Some((dir, share)) = light else {
            continue;
        };
        let fade = ((SHADOW_DISTANCE - distance) / SHADOW_FADE).clamp(0.0, 1.0);
        let darkness = MAX_DARKNESS * share * fade;
        if darkness < 0.01 {
            continue;
        }
        let mut params = shadow_params(&capsules, dir, darkness);
        params.sun = sun_probe(&capsules, map.sun.as_ref());
        casts.push((entity, capsules, dir, params));
    }

    // The slots: as many as the level casts (each light kept alive while the map
    // is up, so the shadow maps Bevy keeps for the lights stay as many), each
    // caster in the one it had.
    let cube = cube
        .get_or_insert_with(|| meshes.add(Cuboid::new(1.0, 1.0, 1.0)))
        .clone();
    let material = material
        .get_or_insert_with(|| materials.add(ShadowMaterial::default()))
        .clone();
    while pool.0.len() < limit {
        let k = pool.0.len();
        let shade = commands
            .spawn((
                ShadowBox,
                Mesh3d(cube.clone()),
                MeshMaterial3d(material.clone()),
                // (Its shadow: the slot's, SlotShadows.)
                MeshTag(k as u32),
                Transform::default(),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();
        let lamp = commands
            .spawn((
                ShadowLamp,
                SpotLight {
                    intensity: 0.0,
                    radius: LAMP_TAG + k as f32,
                    shadows_enabled: true,
                    affects_lightmapped_mesh_diffuse: false,
                    shadow_depth_bias: 0.0,
                    shadow_normal_bias: 0.0,
                    ..default()
                },
                RenderLayers::layer(SHADOW_LAYER + k),
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        pool.0.push(Slot {
            shade,
            lamp,
            caster: None,
            meshes: Vec::new(),
        });
    }
    for (k, slot) in pool.0.iter_mut().enumerate().skip(limit) {
        release(&mut commands, k, slot, &on_layer, &mut boxes, &mut lamps);
    }
    let held: Vec<Option<Entity>> = pool.0.iter().take(limit).map(|s| s.caster).collect();
    let wanted: Vec<Entity> = casts.iter().map(|c| c.0).collect();
    let mut given: Vec<Option<usize>> = vec![None; limit];
    for (i, s) in assign_slots(&held, &wanted).into_iter().enumerate() {
        if let Some(s) = s {
            given[s] = Some(i);
        }
    }
    for (k, slot) in pool.0.iter_mut().take(limit).enumerate() {
        let layer = SHADOW_LAYER + k;
        let Some((entity, capsules, dir, mut params)) = given[k].map(|i| casts[i].clone()) else {
            // (Free: no box, its light kept just ahead of the camera, small, with
            // nothing on its layer.)
            untag(
                &mut commands,
                std::mem::take(&mut slot.meshes),
                layer,
                &on_layer,
            );
            slot.caster = None;
            if let Ok((_, _, mut visibility)) = boxes.get_mut(slot.shade)
                && *visibility != Visibility::Hidden
            {
                *visibility = Visibility::Hidden;
            }
            if let Ok((mut light, mut t, mut g, mut visibility)) = lamps.get_mut(slot.lamp) {
                light.range = 1.0;
                light.shadow_map_near_z = 0.1;
                light.outer_angle = 0.1;
                light.inner_angle = 0.1;
                *t = Transform::from_translation(eye + ahead * 3.0).looking_to(ahead, Vec3::Y);
                *g = GlobalTransform::from(*t);
                visibility.set_if_neq(Visibility::Inherited);
            }
            continue;
        };
        // Its meshes on its layer (and those no longer its, a caster before it's
        // or ones gone, off it).
        let mut now: Vec<Entity> = Vec::new();
        tag(
            &mut commands,
            entity,
            layer,
            &children,
            &mesh_layers,
            &mut now,
        );
        let kept: bevy::platform::collections::HashSet<Entity> = now.iter().copied().collect();
        let gone: Vec<Entity> = std::mem::take(&mut slot.meshes)
            .into_iter()
            .filter(|e| !kept.contains(e))
            .collect();
        untag(&mut commands, gone, layer, &on_layer);
        slot.meshes = now;
        slot.caster = Some(entity);
        // Its light, off the way the light comes from, its cone round the body.
        let lamp = lamp_for(&capsules, dir);
        params.lamp = lamp.at.extend(LAMP_TAG + k as f32);
        params.lens = Vec4::new(lamp.angle.tan(), lamp.near, MIN_GAP, MAX_BLUR);
        if let Ok((mut light, mut t, mut g, mut visibility)) = lamps.get_mut(slot.lamp) {
            light.range = lamp.range;
            light.shadow_map_near_z = lamp.near;
            light.outer_angle = lamp.angle;
            light.inner_angle = lamp.angle;
            *t = Transform::from_translation(lamp.at)
                .looking_to(-dir, if dir.y.abs() > 0.99 { Vec3::X } else { Vec3::Y });
            *g = GlobalTransform::from(*t);
            visibility.set_if_neq(Visibility::Inherited);
        }
        // Its box, from in front of the body (towards the light) to as far as the
        // shadow falls away from it, as wide as the body and its softest edge.
        let (c, r) = bounds_of(&capsules);
        let wide = 2.0 * (r + MAX_BLUR);
        let length = SHADOW_REACH + 2.0 * r;
        let transform = Transform {
            translation: c - dir * (SHADOW_REACH * 0.5),
            rotation: Quat::from_rotation_arc(Vec3::Z, -dir),
            scale: Vec3::new(wide, wide, length),
        };
        if let Some(p) = shadows.bypass_change_detection().0.get_mut(k)
            && *p != params
        {
            *p = params;
            shadows.set_changed();
        }
        if let Ok((mut t, mut g, mut visibility)) = boxes.get_mut(slot.shade) {
            *t = transform;
            *g = GlobalTransform::from(transform);
            visibility.set_if_neq(Visibility::Inherited);
        }
    }
}

/// A slot let go: its box and light hidden, its character's meshes back on the
/// layers they had.
#[allow(clippy::type_complexity)]
fn release(
    commands: &mut Commands,
    k: usize,
    slot: &mut Slot,
    on_layer: &Query<&OnShadowLayer>,
    boxes: &mut Query<
        (&mut Transform, &mut GlobalTransform, &mut Visibility),
        (With<ShadowBox>, Without<crate::Player>, Without<ShadowLamp>),
    >,
    lamps: &mut Query<
        (
            &mut SpotLight,
            &mut Transform,
            &mut GlobalTransform,
            &mut Visibility,
        ),
        (With<ShadowLamp>, Without<crate::Player>, Without<ShadowBox>),
    >,
) {
    untag(
        commands,
        std::mem::take(&mut slot.meshes),
        SHADOW_LAYER + k,
        on_layer,
    );
    slot.caster = None;
    if let Ok((_, _, mut visibility)) = boxes.get_mut(slot.shade)
        && *visibility != Visibility::Hidden
    {
        *visibility = Visibility::Hidden;
    }
    if let Ok((_, _, _, mut visibility)) = lamps.get_mut(slot.lamp)
        && *visibility != Visibility::Hidden
    {
        *visibility = Visibility::Hidden;
    }
}

/// Puts a character's meshes (those that cast shadows) on a slot's layer as well as
/// their own; `out`: them.
#[allow(clippy::type_complexity)]
fn tag(
    commands: &mut Commands,
    root: Entity,
    layer: usize,
    children: &Query<&Children>,
    mesh_layers: &Query<
        (Option<&RenderLayers>, Option<&OnShadowLayer>),
        (With<Mesh3d>, Without<NotShadowCaster>),
    >,
    out: &mut Vec<Entity>,
) {
    let ours = RenderLayers::layer(layer);
    for e in children.iter_descendants(root) {
        let Ok((layers, on)) = mesh_layers.get(e) else {
            continue;
        };
        out.push(e);
        if on.is_some_and(|o| o.layer == layer) && layers.is_some_and(|l| l.intersects(&ours)) {
            continue;
        }
        let before = match on {
            Some(o) => o.before.clone(),
            None => layers.cloned(),
        };
        let on = before.clone().unwrap_or_default().with(layer);
        commands
            .entity(e)
            .try_insert((on, OnShadowLayer { layer, before }));
    }
}

/// Takes meshes off a slot's layer, back on the layers they had (those still on it).
fn untag(
    commands: &mut Commands,
    meshes: Vec<Entity>,
    layer: usize,
    on_layer: &Query<&OnShadowLayer>,
) {
    for e in meshes {
        let Ok(on) = on_layer.get(e) else {
            continue;
        };
        if on.layer != layer {
            continue;
        }
        let Ok(mut ec) = commands.get_entity(e) else {
            continue;
        };
        match on.before.clone() {
            Some(l) => {
                ec.try_insert(l);
            }
            None => {
                ec.try_remove::<RenderLayers>();
            }
        }
        ec.try_remove::<OnShadowLayer>();
    }
}

/// A shadow's box.
#[derive(Component)]
struct ShadowBox;

/// A character as a caster, if it has a body and is near enough the camera.
fn push_caster<'a>(
    casters: &mut Vec<(f32, Entity, Vec<Capsule>, Option<&'a Gathered>)>,
    eyes: &[Vec3],
    entity: Entity,
    capsules: Vec<Capsule>,
    gathered: Option<&'a Gathered>,
) {
    if capsules.is_empty() {
        return;
    }
    let (c, r) = bounds_of(&capsules);
    let distance = eyes
        .iter()
        .map(|eye| (c.distance(*eye) - r).max(0.0))
        .fold(f32::INFINITY, f32::min);
    if distance < SHADOW_DISTANCE {
        casters.push((distance, entity, capsules, gathered));
    }
}

/// The sphere round capsules.
fn bounds_of(capsules: &[Capsule]) -> (Vec3, f32) {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for c in capsules {
        let (m, r) = c.bounds();
        lo = lo.min(m - Vec3::splat(r));
        hi = hi.max(m + Vec3::splat(r));
    }
    ((lo + hi) * 0.5, (hi - lo).length() * 0.5)
}

/// A player's robot's physics asset's bodies where his bones are (as hit_body
/// places them for the AI), as capsules.
fn rig_capsules<F: bevy::ecs::query::QueryFilter>(
    rig: &crate::ragdoll::Rig,
    globals: &Query<&GlobalTransform, F>,
) -> Vec<Capsule> {
    let mut out = Vec::new();
    for body in &rig.asset.bodies {
        let Some(g) = rig
            .names
            .iter()
            .position(|n| *n == body.bone)
            .and_then(|i| globals.get(rig.bones[i]).ok())
        else {
            continue;
        };
        let m = g.affine();
        out.extend(body.shapes.iter().map(|s| Capsule::of(&s.placed(&m))));
    }
    out
}

/// A vehicle model's shadow (fit_car_shadows): capsules fitted to its pieces, each
/// on a bone of it (or a part hung on one) in that one's space, and the meshes it
/// was fitted to (it is cast while one of them is shown: a car's guns only while
/// they're out). Fitted again when its scene or what hangs on it changes.
#[derive(Component)]
pub struct CarShadow {
    instance: InstanceId,
    hung: Vec<Entity>,
    pieces: Vec<Piece>,
}

/// One of a vehicle's capsules: what it is on, it in that one's space, and the
/// meshes it covers.
struct Piece {
    on: Entity,
    capsule: Capsule,
    meshes: Vec<Entity>,
}

impl CarShadow {
    /// Its capsules where the bones and parts they're on are now.
    fn placed<F: bevy::ecs::query::QueryFilter>(
        &self,
        globals: &Query<&GlobalTransform, F>,
        shown: &Query<&InheritedVisibility>,
    ) -> Vec<Capsule> {
        self.pieces
            .iter()
            .filter(|p| {
                p.meshes
                    .iter()
                    .any(|m| shown.get(*m).is_ok_and(|v| v.get()))
            })
            .filter_map(|p| {
                let c = place(&p.capsule, &globals.get(p.on).ok()?.affine());
                (c.r > 0.005).then_some(c)
            })
            .collect()
    }
}

/// A capsule in a bone's space where the bone is (its radius by the bone's
/// largest scale: a bone scaled to nothing, a piece hidden, casts nothing).
fn place(c: &Capsule, at: &bevy::math::Affine3A) -> Capsule {
    let m = at.matrix3;
    let scale = m
        .x_axis
        .length()
        .max(m.y_axis.length())
        .max(m.z_axis.length());
    Capsule {
        a: at.transform_point3(c.a),
        b: at.transform_point3(c.b),
        r: c.r * scale,
    }
}

/// Fits each player's vehicle model's shadow (CarShadow) once its scene is in, and
/// again when its scene or the parts hung on it change (character.rs's HungPart)
/// or its bones are put in afresh.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn fit_car_shadows(
    mut commands: Commands,
    spawner: Res<SceneSpawner>,
    cars: Query<(Entity, &SceneInstance, Option<&CarShadow>), With<crate::VehicleForm>>,
    hung: Query<(Entity, &crate::character::HungPart, &Aabb)>,
    skins: Query<(Entity, &SkinnedMesh, &Mesh3d)>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    alive: Query<(), With<GlobalTransform>>,
    meshes: Res<Assets<Mesh>>,
    bindposes: Res<Assets<SkinnedMeshInverseBindposes>>,
) {
    for (car, instance, shadow) in &cars {
        if !spawner.instance_is_ready(**instance) {
            continue;
        }
        let mut on: Vec<(Entity, Aabb)> = hung
            .iter()
            .filter(|(_, h, _)| h.0 == car)
            .map(|(e, _, b)| (e, *b))
            .collect();
        on.sort_by_key(|(e, _)| *e);
        let parts: Vec<Entity> = on.iter().map(|(e, _)| *e).collect();
        if shadow.is_some_and(|s| {
            s.instance == **instance
                && s.hung == parts
                && s.pieces.iter().all(|p| alive.contains(p.on))
        }) {
            continue;
        }
        // The parts' capsules (round their meshes' boxes; the biggest, up to half
        // of them all), then the model's own in what is left.
        let mut pieces: Vec<Piece> = Vec::new();
        for (e, aabb) in &on {
            let (c, h) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
            pieces.extend(
                box_capsules(c - h, c + h, 3)
                    .into_iter()
                    .map(|capsule| Piece {
                        on: *e,
                        capsule,
                        meshes: vec![*e],
                    }),
            );
        }
        pieces.sort_by(|a, b| volume(&b.capsule).total_cmp(&volume(&a.capsule)));
        pieces.truncate(MAX_CAPSULES / 2);
        // The model's skinned meshes' bones: each the box round the vertices it
        // carries most (in its own space, by its inverse bind pose), the bone of
        // its skin it hangs from.
        let mut bones: Vec<FitBone> = Vec::new();
        let mut entities: Vec<Entity> = Vec::new();
        let mut index: bevy::platform::collections::HashMap<Entity, usize> = default();
        let mut skin_of: Vec<usize> = Vec::new();
        let mut skin_joints: Vec<bevy::platform::collections::HashSet<Entity>> = Vec::new();
        let mut mesh_list: Vec<Entity> = Vec::new();
        for e in children.iter_descendants(car) {
            let Ok((mesh_entity, skinned, mesh3d)) = skins.get(e) else {
                continue;
            };
            let (Some(mesh), Some(ibm)) = (
                meshes.get(&mesh3d.0),
                bindposes.get(&skinned.inverse_bindposes),
            ) else {
                continue;
            };
            let (
                Some(VertexAttributeValues::Float32x3(positions)),
                Some(VertexAttributeValues::Uint16x4(joints)),
                Some(VertexAttributeValues::Float32x4(weights)),
            ) = (
                mesh.attribute(Mesh::ATTRIBUTE_POSITION),
                mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX),
                mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT),
            )
            else {
                continue;
            };
            if mesh_list.len() >= 64 {
                break;
            }
            let bit = 1u64 << mesh_list.len();
            mesh_list.push(mesh_entity);
            let skin = skin_joints.len();
            skin_joints.push(skinned.joints.iter().copied().collect());
            for (k, &joint) in skinned.joints.iter().enumerate() {
                if !index.contains_key(&joint) && k < ibm.len() {
                    index.insert(joint, bones.len());
                    entities.push(joint);
                    skin_of.push(skin);
                    bones.push(FitBone {
                        parent: None,
                        inverse_bind: ibm[k],
                        own: None,
                        meshes: 0,
                    });
                }
            }
            // (Each bone's vertices, counted first: a bone with only a few, at a
            // seam, gets no box of its own.)
            let mut own: Vec<(usize, Vec3, Vec3)> =
                vec![(0, Vec3::MAX, Vec3::MIN); skinned.joints.len()];
            for ((p, j), w) in positions.iter().zip(joints).zip(weights) {
                let most = (0..4).max_by(|&a, &b| w[a].total_cmp(&w[b])).unwrap_or(0);
                let k = j[most] as usize;
                let (Some(m), Some(o)) = (ibm.get(k), own.get_mut(k)) else {
                    continue;
                };
                let q = m.transform_point3(Vec3::from(*p));
                *o = (o.0 + 1, o.1.min(q), o.2.max(q));
            }
            for (k, (count, lo, hi)) in own.into_iter().enumerate() {
                if count < MIN_BONE_VERTICES {
                    continue;
                }
                let Some(&i) = index.get(&skinned.joints[k]) else {
                    continue;
                };
                let b = &mut bones[i];
                b.own = Some(match b.own {
                    Some((l, h)) => (l.min(lo), h.max(hi)),
                    None => (lo, hi),
                });
                b.meshes |= bit;
            }
        }
        for i in 0..bones.len() {
            let joints = &skin_joints[skin_of[i]];
            bones[i].parent = parents
                .iter_ancestors(entities[i])
                .find(|a| joints.contains(a))
                .and_then(|a| index.get(&a).copied());
        }
        let budget = MAX_CAPSULES - pieces.len();
        pieces.extend(
            fit_bones(&bones, budget)
                .into_iter()
                .map(|(bone, capsule, mask)| Piece {
                    on: entities[bone],
                    capsule,
                    meshes: (0..mesh_list.len())
                        .filter(|m| mask & (1 << m) != 0)
                        .map(|m| mesh_list[m])
                        .collect(),
                }),
        );
        commands.entity(car).try_insert(CarShadow {
            instance: **instance,
            hung: parts,
            pieces,
        });
    }
}

/// The fewest vertices a bone carries (most) for its own box in a vehicle's fit.
const MIN_BONE_VERTICES: usize = 6;

/// A bone of a skinned model, for fit_bones: the bone it hangs from (an index), its
/// inverse bind pose (the model's space to its own), the box round the vertices it
/// carries in its own space (none: it carries none) and which of the model's meshes
/// they're of (a bit each).
#[derive(Clone, Debug)]
pub struct FitBone {
    pub parent: Option<usize>,
    pub inverse_bind: Mat4,
    pub own: Option<(Vec3, Vec3)>,
    pub meshes: u64,
}

/// Capsules round a skinned model's bones, at most `budget`: at first the whole
/// model in one (box_capsules) round its root bone's box; then again and again the
/// bone (with all it carries) whose peeling off the capsule it is in, into one of
/// its own round its box, saves the most room, while that saves any. Each: the
/// bone it is on (an index), the capsule in that bone's space, the meshes it covers.
/// (Each bone's box as its corners: a group's box in its first bone's space is the
/// one round its bones' corners.)
pub fn fit_bones(bones: &[FitBone], budget: usize) -> Vec<(usize, Capsule, u64)> {
    let n = bones.len();
    let corners: Vec<Vec<Vec3>> = bones
        .iter()
        .map(|b| {
            let Some((lo, hi)) = b.own else {
                return Vec::new();
            };
            let to_model = b.inverse_bind.inverse();
            (0..8)
                .map(|i| {
                    let pick = |bit: usize, l: f32, h: f32| if i & bit == 0 { l } else { h };
                    to_model.transform_point3(Vec3::new(
                        pick(1, lo.x, hi.x),
                        pick(2, lo.y, hi.y),
                        pick(4, lo.z, hi.z),
                    ))
                })
                .collect()
        })
        .collect();
    let mut children = vec![Vec::new(); n];
    for (i, b) in bones.iter().enumerate() {
        if let Some(p) = b.parent.filter(|&p| p < n && p != i) {
            children[p].push(i);
        }
    }
    // A group's capsules: round its bones' corners, in its first bone's space (at
    // most `most` side by side).
    let fit = |anchor: usize, members: &[usize], most: usize| -> Vec<Capsule> {
        let m = bones[anchor].inverse_bind;
        let (mut lo, mut hi) = (Vec3::MAX, Vec3::MIN);
        for &k in members {
            for c in &corners[k] {
                let p = m.transform_point3(*c);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        if lo.x > hi.x {
            return Vec::new();
        }
        box_capsules(lo, hi, most)
    };
    let total_volume = |c: &[Capsule]| c.iter().map(volume).sum::<f32>();
    // A bone and those it carries, in group `g`.
    let subtree = |from: usize, g: usize, group: &[usize]| -> Vec<usize> {
        let mut out = vec![from];
        let mut i = 0;
        while i < out.len() {
            out.extend(children[out[i]].iter().filter(|&&c| group[c] == g));
            i += 1;
        }
        out
    };
    // The groups: each its first bone and capsules; and which each bone is in.
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut group = vec![usize::MAX; n];
    for root in (0..n).filter(|&i| bones[i].parent.is_none_or(|p| p >= n || p == i)) {
        let g = groups.len();
        let mut members = vec![root];
        let mut i = 0;
        while i < members.len() {
            let more: Vec<usize> = children[members[i]]
                .iter()
                .copied()
                .filter(|&c| group[c] == usize::MAX && !members.contains(&c))
                .collect();
            members.extend(more);
            i += 1;
        }
        for &m in &members {
            group[m] = g;
        }
        groups.push((root, members));
    }
    // (Its roots' boxes each in one capsule if there isn't room for more.)
    let fits: Vec<Vec<Capsule>> = groups.iter().map(|(root, m)| fit(*root, m, 3)).collect();
    let most = if fits.iter().map(Vec::len).sum::<usize>() > budget {
        1
    } else {
        3
    };
    let mut groups: Vec<(usize, Vec<Capsule>)> = groups
        .into_iter()
        .map(|(root, m)| (root, fit(root, &m, most)))
        .collect();
    loop {
        let total: usize = groups.iter().map(|(_, c)| c.len()).sum();
        // (The best peel yet: the room it saves, the bone, those it carries, their
        // capsules and those of what is left.)
        type Peel = (f32, usize, Vec<usize>, Vec<Capsule>, Vec<Capsule>);
        let mut best: Option<Peel> = None;
        let mut peeling = vec![false; n];
        // (Each bone's box in its group's first bone's space: what's left of a
        // group is round those.)
        let in_anchor: Vec<Option<(Vec3, Vec3)>> = (0..n)
            .map(|k| {
                let g = *group.get(k).filter(|&&g| g != usize::MAX)?;
                let m = bones[groups[g].0].inverse_bind;
                (!corners[k].is_empty()).then(|| {
                    corners[k].iter().fold((Vec3::MAX, Vec3::MIN), |(l, h), c| {
                        let p = m.transform_point3(*c);
                        (l.min(p), h.max(p))
                    })
                })
            })
            .collect();
        for c in 0..n {
            let g = group[c];
            if g == usize::MAX || groups[g].0 == c {
                continue;
            }
            let peeled_bones = subtree(c, g, &group);
            let peeled = fit(c, &peeled_bones, 3);
            if peeled.is_empty() {
                continue;
            }
            for &k in &peeled_bones {
                peeling[k] = true;
            }
            let (mut lo, mut hi) = (Vec3::MAX, Vec3::MIN);
            for k in (0..n).filter(|&k| group[k] == g && !peeling[k]) {
                if let Some((l, h)) = in_anchor[k] {
                    lo = lo.min(l);
                    hi = hi.max(h);
                }
            }
            for &k in &peeled_bones {
                peeling[k] = false;
            }
            let kept = if lo.x > hi.x {
                Vec::new()
            } else {
                box_capsules(lo, hi, 3)
            };
            if total - groups[g].1.len() + peeled.len() + kept.len() > budget {
                continue;
            }
            let saved = total_volume(&groups[g].1) - total_volume(&peeled) - total_volume(&kept);
            if best.as_ref().is_none_or(|b| saved > b.0) {
                best = Some((saved, c, peeled_bones, peeled, kept));
            }
        }
        let Some((saved, c, peeled_bones, peeled, kept)) = best else {
            break;
        };
        if saved <= 1e-6 {
            break;
        }
        let g = group[c];
        groups[g].1 = kept;
        let new = groups.len();
        for &k in &peeled_bones {
            group[k] = new;
        }
        groups.push((c, peeled));
    }
    // (More roots than capsules: the biggest.)
    let mut out: Vec<(usize, Capsule, u64)> = Vec::new();
    for (g, (anchor, capsules)) in groups.iter().enumerate() {
        let meshes = (0..n)
            .filter(|&k| group[k] == g)
            .fold(0u64, |m, k| m | bones[k].meshes);
        out.extend(capsules.iter().map(|c| (*anchor, *c, meshes)));
    }
    if out.len() > budget {
        out.sort_by(|a, b| volume(&b.1).total_cmp(&volume(&a.1)));
        out.truncate(budget);
    }
    out
}

/// Capsules filling a box (its corners): one along its longest side, as wide as
/// its middle one; a flat box (its middle side half again as long as its shortest,
/// or more) as up to `most` side by side across it, each as thick as the box.
pub fn box_capsules(lo: Vec3, hi: Vec3, most: usize) -> Vec<Capsule> {
    let c = (lo + hi) * 0.5;
    let h = ((hi - lo) * 0.5).max(Vec3::ZERO).to_array();
    let mut axes = [0usize, 1, 2];
    axes.sort_by(|&a, &b| h[b].total_cmp(&h[a]));
    let [long, mid, short] = axes;
    let unit = |i: usize| Vec3::AXES[i];
    let n = (h[mid] / h[short].max(1e-4))
        .round()
        .clamp(1.0, most.max(1) as f32) as usize;
    let r = if n == 1 {
        h[mid]
    } else {
        h[short].max(h[mid] / n as f32)
    };
    let along = unit(long) * (h[long] - r).max(0.0);
    (0..n)
        .map(|k| {
            let off = if n > 1 {
                unit(mid) * (h[mid] - r) * (2.0 * k as f32 / (n - 1) as f32 - 1.0)
            } else {
                Vec3::ZERO
            };
            Capsule {
                a: c + off - along,
                b: c + off + along,
                r,
            }
        })
        .collect()
}

/// A capsule's volume.
fn volume(c: &Capsule) -> f32 {
    let r2 = c.r * c.r;
    std::f32::consts::PI * r2 * (c.a.distance(c.b) + c.r * 4.0 / 3.0)
}

/// How far out of a body, towards the sun, the shader looks whether the sun
/// reaches it (m): out of its own shadow, close enough that what shades it shades
/// that.
const SUN_PROBE_OUT: f32 = 0.3;

/// Where the shader looks whether the sun reaches a body (its shadow map there):
/// just out of the sphere round its capsules, towards the sun; w 1. None (all 0) on
/// a map without a sun.
pub fn sun_probe(capsules: &[Capsule], sun: Option<&crate::lighting::Sun>) -> Vec4 {
    match sun.and_then(|s| (-s.direction).try_normalize()) {
        Some(to_sun) if !capsules.is_empty() => {
            let (c, r) = bounds_of(capsules);
            (c + to_sun * (r + SUN_PROBE_OUT)).extend(1.0)
        }
        _ => Vec4::ZERO,
    }
}

/// The shader's values for capsules (the biggest MAX_CAPSULES), the way to the
/// light and the darkness.
pub fn shadow_params(capsules: &[Capsule], dir: Vec3, darkness: f32) -> ShadowParams {
    let mut list: Vec<&Capsule> = capsules.iter().collect();
    if list.len() > MAX_CAPSULES {
        list.sort_by(|a, b| b.bounds().1.total_cmp(&a.bounds().1));
        list.truncate(MAX_CAPSULES);
    }
    let mut p = ShadowParams {
        dir: dir.extend(SHADOW_REACH),
        shade: Vec4::new(darkness, SOFTNESS, SOFT_PER_M, list.len() as f32),
        ..default()
    };
    for (i, c) in list.iter().enumerate() {
        p.a[i] = c.a.extend(c.r);
        p.b[i] = c.b.extend(0.0);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::MapLight;

    /// The slots' shadows are written to their buffer as the shader's array of
    /// ShadowParams has them: each 37 vec4s (towards the light, the shade, 16
    /// capsules' starts and 16 ends, the sun probe, the lamp, its lens), a slot's at
    /// its tag's place.
    #[test]
    fn slot_shadows_are_written_as_the_shader_reads_them() {
        let p = ShadowParams {
            dir: Vec4::new(1.0, 2.0, 3.0, 4.0),
            a: {
                let mut a = [Vec4::ZERO; MAX_CAPSULES];
                a[15] = Vec4::splat(5.0);
                a
            },
            b: {
                let mut b = [Vec4::ZERO; MAX_CAPSULES];
                b[0] = Vec4::splat(6.0);
                b
            },
            lens: Vec4::new(7.0, 8.0, 9.0, 10.0),
            ..default()
        };
        let bytes = slot_shadow_bytes(&[ShadowParams::default(), p]);
        assert_eq!(bytes.len(), 2 * 37 * 16);
        let word = |i: usize| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        // (The second slot's, from its first word.)
        let at = 37 * 4;
        assert_eq!((word(at), word(at + 3)), (1.0, 4.0));
        assert_eq!(word(at + 4 * (2 + 15)), 5.0);
        assert_eq!(word(at + 4 * (2 + 16)), 6.0);
        assert_eq!((word(at + 4 * 36), word(at + 4 * 36 + 3)), (7.0, 10.0));
        assert_eq!(SlotShadows::default().0.len(), MAX_SLOTS);
    }

    fn light(at: Vec3, brightness: f32) -> MapLight {
        MapLight {
            position: at,
            color: Color::WHITE,
            brightness,
            radius: 20.0,
            falloff: 2.0,
            spot: None,
            dynamic_only: true,
        }
    }

    #[test]
    fn the_shadow_falls_from_the_strongest_light_never_too_low() {
        // One light high on the left: the light comes from there, wholly.
        let lights = [light(Vec3::new(-5.0, 6.0, 0.0), 4.0)];
        let (dir, share) = composite_light(&lights, Vec3::ZERO, |_, _| false).unwrap();
        assert!((share - 1.0).abs() < 1e-4);
        assert!(dir.x < 0.0 && dir.y > 0.6 && dir.z.abs() < 1e-4);
        // One low on the horizon: raised to MinShadowAngle.
        let low = [light(Vec3::new(8.0, 0.2, 0.0), 4.0)];
        let (dir, _) = composite_light(&low, Vec3::ZERO, |_, _| false).unwrap();
        assert!((dir.y - MIN_SHADOW_ANGLE.to_radians().sin()).abs() < 1e-4 && dir.x > 0.0);
        // Two the same either side: no one way, no shadow worth casting.
        let both = [
            light(Vec3::new(-5.0, 5.0, 0.0), 4.0),
            light(Vec3::new(5.0, 5.0, 0.0), 4.0),
        ];
        let (dir, share) = composite_light(&both, Vec3::ZERO, |_, _| false).unwrap();
        assert!(dir.y > 0.99 && share < 0.75);
        // A light behind a wall doesn't count; out of reach, nothing.
        assert!(composite_light(&lights, Vec3::ZERO, |_, _| true).is_none());
        assert!(composite_light(&lights, Vec3::new(100.0, 0.0, 0.0), |_, _| false).is_none());
    }

    #[test]
    fn bodies_become_capsules_and_the_biggest_are_kept() {
        use crate::ragdoll::Shape;
        let boxy = Shape::Box {
            c: Vec3::ZERO,
            axes: [Vec3::X, Vec3::Y, Vec3::Z],
            half: Vec3::new(0.2, 1.0, 0.3),
        };
        let c = Capsule::of(&boxy);
        // (Along its longest side, Y, as wide as its other two.)
        let r = (0.2f32 * 0.2 + 0.3 * 0.3).sqrt();
        assert!((c.r - r).abs() < 1e-5);
        assert!((c.a - Vec3::new(0.0, -(1.0 - r), 0.0)).length() < 1e-5);
        let many: Vec<Capsule> = (0..20)
            .map(|i| Capsule {
                a: Vec3::ZERO,
                b: Vec3::ZERO,
                r: i as f32 * 0.1,
            })
            .collect();
        let p = shadow_params(&many, Vec3::Y, 0.5);
        assert_eq!(p.shade.w, MAX_CAPSULES as f32);
        // (The smallest four left out.)
        assert!(p.a.iter().all(|a| a.w >= 0.35));
    }

    /// A character's shadow is of its own shape: its light (a spot light whose
    /// shadow map holds it alone) stands off the way its light comes from, its
    /// cone round the whole body and the widest blur, its near plane in front of
    /// it; the shader finds a point on that shadow map as Bevy draws it.
    #[test]
    fn a_shadows_light_sees_the_whole_body() {
        let body = [
            Capsule {
                a: Vec3::new(1.0, 0.4, -2.0),
                b: Vec3::new(1.0, 2.6, -2.0),
                r: 0.5,
            },
            Capsule {
                a: Vec3::new(0.0, 2.2, -2.0),
                b: Vec3::new(2.0, 2.2, -2.0),
                r: 0.3,
            },
        ];
        let dir = Vec3::new(-0.4, 0.8, 0.3).normalize();
        let lamp = lamp_for(&body, dir);
        let (c, r) = bounds_of(&body);
        assert!((lamp.at - c - dir * LAMP_DISTANCE).length() < 1e-4);
        // (Bevy's view of it: its transform looking back along the light.)
        let t = Transform::from_translation(lamp.at).looking_to(-dir, Vec3::Y);
        let world_from_view = bevy::light::spot_light_world_from_view(&GlobalTransform::from(t));
        let clip_from_view = bevy::light::spot_light_clip_from_view(lamp.angle, lamp.near);
        let clip_from_world = clip_from_view * Mat4::from(world_from_view.inverse());
        // Points of the body, its edges and the blur round them, and the ground
        // under it, all on the map, as Bevy puts them.
        let mut points = Vec::new();
        for cap in &body {
            for k in 0..=8 {
                let m = cap.a.lerp(cap.b, k as f32 / 8.0);
                for side in [
                    Vec3::X,
                    Vec3::NEG_X,
                    Vec3::Y,
                    Vec3::NEG_Y,
                    Vec3::Z,
                    Vec3::NEG_Z,
                ] {
                    points.push(m + side * (cap.r + MAX_BLUR));
                }
            }
        }
        points.push(c - dir * 3.0);
        for p in points {
            let s = lamp_view(&lamp, dir, p);
            let clip = clip_from_world * p.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            let uv = Vec2::new(ndc.x, ndc.y) * Vec2::new(0.5, -0.5) + Vec2::splat(0.5);
            assert!(
                (Vec2::new(s.x, s.y) - uv).length() < 1e-4,
                "{p:?}: {s:?} {uv:?}"
            );
            // (Bevy's depth: near over the distance along its axis.)
            assert!((ndc.z - lamp.near / s.z).abs() < 1e-5);
            if p.distance(c) <= r + MAX_BLUR {
                assert!(
                    s.x > 0.0 && s.x < 1.0 && s.y > 0.0 && s.y < 1.0,
                    "{p:?} off the map"
                );
                assert!(s.z > lamp.near, "{p:?} nearer than its near plane");
            }
            assert!(s.z < lamp.range);
        }
    }

    /// Each caster keeps its slot (its light and layer) while it casts; a new one
    /// takes a free one, the nearest first, and one too many none.
    #[test]
    fn casters_keep_their_slots() {
        let [a, b, c, d] = [1, 2, 3, 4].map(Entity::from_raw_u32).map(Option::unwrap);
        let slots = [Some(a), None, Some(b)];
        // (b keeps 2, a keeps 0, c takes the free 1.)
        assert_eq!(
            assign_slots(&slots, &[b, c, a]),
            vec![Some(2), Some(1), Some(0)]
        );
        // (a gone: d takes its slot; c the free one; b none left.)
        assert_eq!(
            assign_slots(&slots, &[d, c, b]),
            vec![Some(0), Some(1), Some(2)]
        );
        assert_eq!(assign_slots(&slots, &[c, d]), vec![Some(0), Some(1)]);
        assert_eq!(assign_slots(&[None], &[c, d]), vec![Some(0), None]);
        assert!(
            (SHADOW_LAYER..SHADOW_LAYER + MAX_SLOTS)
                .all(|l| shadow_layers().intersects(&RenderLayers::layer(l)))
        );
        assert!(!shadow_layers().intersects(&RenderLayers::layer(0)));
    }

    /// In the sun a character casts the sun's shadow alone: the shader is told
    /// where to look whether the sun reaches it, just out of its body towards the
    /// sun (out of its own shadow); on a map without a sun, nowhere (it keeps the
    /// lights' shadow).
    #[test]
    fn the_sun_is_looked_for_just_out_of_the_body() {
        let sun = crate::lighting::Sun {
            direction: Vec3::new(0.3, -0.9, 0.1),
            color: Color::WHITE,
            brightness: 3.0,
        };
        let body = [
            Capsule {
                a: Vec3::new(0.0, 0.5, 0.0),
                b: Vec3::new(0.0, 1.5, 0.0),
                r: 0.4,
            },
            Capsule {
                a: Vec3::new(-0.6, 1.4, 0.0),
                b: Vec3::new(0.6, 1.4, 0.0),
                r: 0.2,
            },
        ];
        let p = sun_probe(&body, Some(&sun));
        assert_eq!(p.w, 1.0);
        let to_sun = (-sun.direction).normalize();
        let (c, r) = bounds_of(&body);
        let at = p.truncate();
        // (Out of the body, towards the sun, SUN_PROBE_OUT past its bounds.)
        assert!(((at - c).normalize() - to_sun).length() < 1e-4);
        assert!(((at - c).length() - (r + SUN_PROBE_OUT)).abs() < 1e-4);
        for cap in &body {
            let (m, cr) = cap.bounds();
            assert!(at.distance(m) > cr, "inside {cap:?}");
        }
        assert_eq!(sun_probe(&body, None), Vec4::ZERO);
        assert_eq!(sun_probe(&[], Some(&sun)), Vec4::ZERO);
        // (The shader's uniform: unset, no sun.)
        assert_eq!(shadow_params(&body, Vec3::Y, 0.5).sun, Vec4::ZERO);
    }

    /// A vehicle's shadow keeps to its shape (not one round blot): an arm of two
    /// bones at a right angle, a box of vertices on each, fits two capsules along
    /// them, each on its bone, rather than one round both; a flat box fits capsules
    /// side by side; a capsule goes where its bone is, as big as the bone's scale.
    #[test]
    fn a_vehicles_shadow_keeps_to_its_shape() {
        let near = |a: Vec3, b: Vec3| a.distance(b) < 1e-4;
        let thin = (Vec3::new(0.0, -0.2, -0.2), Vec3::new(2.0, 0.2, 0.2));
        // (The second bone at the first's end, turned up: its X along the model's Y.)
        let elbow = Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0))
            * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let bones = [
            FitBone {
                parent: None,
                inverse_bind: Mat4::IDENTITY,
                own: Some(thin),
                meshes: 1,
            },
            FitBone {
                parent: Some(0),
                inverse_bind: elbow.inverse(),
                own: Some(thin),
                meshes: 2,
            },
        ];
        let fitted = fit_bones(&bones, MAX_CAPSULES);
        assert_eq!(fitted.len(), 2, "{fitted:?}");
        for (bone, c, meshes) in &fitted {
            // (Each along its own bone's X, as thick as its box.)
            assert!((c.r - 0.2).abs() < 1e-4, "{c:?}");
            assert!(
                near(c.a, Vec3::new(0.2, 0.0, 0.0)) && near(c.b, Vec3::new(1.8, 0.0, 0.0)),
                "{c:?}"
            );
            assert_eq!(*meshes, if *bone == 0 { 1 } else { 2 });
        }
        // Where the second bone is, up from the first's end.
        let (_, up, _) = fitted.iter().find(|f| f.0 == 1).unwrap();
        let placed = place(up, &bevy::math::Affine3A::from_mat4(elbow));
        assert!(
            near(placed.a, Vec3::new(2.0, 0.2, 0.0)) && near(placed.b, Vec3::new(2.0, 1.8, 0.0))
        );
        // With room for only one, one round both (as wide as the two).
        let one = fit_bones(&bones, 1);
        assert_eq!(one.len(), 1);
        assert!((one[0].1.r - 1.1).abs() < 1e-4, "{one:?}");
        // A flat box: three side by side across it, as thick as it.
        let flat = box_capsules(Vec3::new(-2.0, -1.0, -0.2), Vec3::new(2.0, 1.0, 0.2), 3);
        assert_eq!(flat.len(), 3);
        let r = 1.0 / 3.0;
        for (c, y) in flat.iter().zip([-(1.0 - r), 0.0, 1.0 - r]) {
            assert!((c.r - r).abs() < 1e-4);
            assert!(
                near(c.a, Vec3::new(-(2.0 - r), y, 0.0)) && near(c.b, Vec3::new(2.0 - r, y, 0.0))
            );
        }
        // A bone scaled twice as big: its capsule too.
        let big = place(
            &flat[1],
            &bevy::math::Affine3A::from_scale(Vec3::splat(2.0)),
        );
        assert!((big.r - 2.0 * r).abs() < 1e-4);
    }
}
