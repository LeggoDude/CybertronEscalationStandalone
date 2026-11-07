//! The pickups' glowing outlines drawn round their silhouettes only, as the game
//! draws them.
//!
//! The demo's outline is a shell round each pickup (tools/build_assets.py's
//! outline_shell: every triangle again, pushed out along the smoothed normals and
//! turned inside out, in its own glowing material, PickupOutline or
//! PickupOutlineHeavy), whose far side shows round the pickup's edges. Drawn as it
//! is, it also showed round the edges of the pickup's parts in front of its other
//! parts (a gun's barrel over its body, its body over the forearm cuff): lines
//! across the gun the game hasn't. Here the shell's material is drawn with
//! assets/shaders/pickup_outline.wgsl, which moves each corner of the shell back
//! along its line of sight to the back of the pickup's bounding box (its far side
//! as the camera sees it): where it is seen it stays where it was on the screen,
//! but it lies behind every part of the pickup, so it shows round its silhouette
//! only, in front of whatever is behind the pickup. The
//! weapons on the stations and in the pods, the ammo crates and the support items
//! all have it. A pickup in a pod goes no further back than just in front of the
//! pod's back wall (OutlineBackstop): a weapon's back (its forearm cuff) is in the
//! wall, and its box's back with it. The shell is drawn by a copy of it in that
//! material, beside it, the shell itself hidden (outline_silhouettes says why).

use bevy::camera::primitives::MeshAabb;
use bevy::gltf::GltfMaterialName;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use std::collections::HashMap;

const OUTLINE_SHADER: &str = "shaders/pickup_outline.wgsl";

/// The outline shells' materials (build_assets.py's OUTLINE_MATERIAL, and the
/// heavy weapons' PickupOutlineHeavy).
const OUTLINE_MATERIAL_PREFIX: &str = "PickupOutline";

/// A pickup's outline (pickup_outline.wgsl): the box round its shell, in its mesh's
/// space.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct PickupOutline {
    /// The box's middle (xyz).
    #[uniform(100)]
    pub centre: Vec4,
    /// Half its size along each of the mesh's axes (xyz).
    #[uniform(101)]
    pub extent: Vec4,
    /// The plane it goes no further back than (OutlineBackstop's), or none (zero).
    #[uniform(102)]
    pub backstop: Vec4,
}

/// A plane behind a pickup that its outline is moved back no further than (to
/// within pickup_outline.wgsl's BACKSTOP_GAP in front of it): a pod's back wall,
/// for what the pod made.
/// Its normal (xyz, towards the front) and offset (w), in world space: a place `p`
/// is in front of it where `n.dot(p) + w` is above 0. On the pickup's entity (the
/// root of its scene).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct OutlineBackstop(pub Vec4);

impl OutlineBackstop {
    /// The plane through `at` facing `normal`.
    pub fn facing(normal: Vec3, at: Vec3) -> OutlineBackstop {
        let n = normal.normalize_or_zero();
        OutlineBackstop(n.extend(-n.dot(at)))
    }
}

impl MaterialExtension for PickupOutline {
    fn vertex_shader() -> ShaderRef {
        OUTLINE_SHADER.into()
    }

    // (Its depth is moved back in its own vertex shader, which the depth prepass's
    // wouldn't do: drawn there, its nearer depth would hide it from itself. And it
    // casts no shadow.)
    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

pub type PickupOutlineMaterial = ExtendedMaterial<StandardMaterial, PickupOutline>;

pub struct OutlinePlugin;

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<PickupOutlineMaterial>::default())
            .add_systems(Update, outline_silhouettes);
    }
}

/// An outline material made: for the shell's material and mesh, and the plane
/// behind its pickup (its bits).
type OutlineKey = (AssetId<StandardMaterial>, AssetId<Mesh>, [u32; 4]);

/// Each outline shell coming in is drawn in the outline material (its own colour
/// and glow, round its mesh's bounding box, no further back than the plane behind
/// its pickup if it has one) by a copy of it beside it, the shell
/// itself hidden. (Not by giving the shell the new material: an entity changing
/// its material's type while the depth prepass has it in its bins can be drawn
/// there one frame with the old pipeline and the new material's bind group, which
/// wgpu rejects: the demo stopped with a validation error. The copy is in the
/// outline material from the first, and the hidden shell leaves the bins as any
/// hidden mesh does.)
#[allow(clippy::type_complexity)]
fn outline_silhouettes(
    mut commands: Commands,
    parts: Query<
        (
            Entity,
            &GltfMaterialName,
            &MeshMaterial3d<StandardMaterial>,
            &Mesh3d,
            &Transform,
            &ChildOf,
        ),
        Added<GltfMaterialName>,
    >,
    standard: Res<Assets<StandardMaterial>>,
    meshes: Res<Assets<Mesh>>,
    mut outlines: ResMut<Assets<PickupOutlineMaterial>>,
    (parents, backstops): (Query<&ChildOf>, Query<&OutlineBackstop>),
    mut made: Local<HashMap<OutlineKey, Handle<PickupOutlineMaterial>>>,
) {
    // (Those of pickups gone are let go.)
    made.retain(|(m, s, _), _| standard.contains(*m) && meshes.contains(*s));
    for (entity, name, material, mesh, transform, child_of) in &parts {
        if !name.0.starts_with(OUTLINE_MATERIAL_PREFIX) {
            continue;
        }
        // (The plane behind its pickup, if any: on the pickup's entity, above it.)
        let backstop = std::iter::once(child_of.parent())
            .chain(parents.iter_ancestors(child_of.parent()))
            .find_map(|e| backstops.get(e).ok())
            .map_or(Vec4::ZERO, |b| b.0);
        let key = (
            material.id(),
            mesh.id(),
            backstop.to_array().map(f32::to_bits),
        );
        let handle = match made.get(&key) {
            Some(h) => h.clone(),
            None => {
                let (Some(base), Some(aabb)) = (
                    standard.get(material.id()),
                    meshes.get(mesh.id()).and_then(|m| m.compute_aabb()),
                ) else {
                    continue;
                };
                let h = outlines.add(PickupOutlineMaterial {
                    base: base.clone(),
                    extension: PickupOutline {
                        centre: Vec3::from(aabb.center).extend(0.0),
                        extent: Vec3::from(aabb.half_extents).extend(0.0),
                        backstop,
                    },
                });
                made.insert(key, h.clone());
                h
            }
        };
        commands.entity(entity).try_insert(Visibility::Hidden);
        let copy = commands
            .spawn((
                Mesh3d(mesh.0.clone()),
                MeshMaterial3d(handle),
                *transform,
                Visibility::Inherited,
                NotShadowCaster,
                NotShadowReceiver,
            ))
            .id();
        if let Ok(mut parent) = commands.get_entity(child_of.parent()) {
            parent.add_child(copy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The depth pickup_outline.wgsl moves the shell back to (view space, looking
    /// down -z): the back of the box (`centre`, `extent`, in the mesh's space) as
    /// `view_from_local` sees it.
    fn back_of(view_from_local: Mat4, centre: Vec3, extent: Vec3) -> f32 {
        let c = view_from_local * centre.extend(1.0);
        let depth = view_from_local.x_axis.z.abs() * extent.x
            + view_from_local.y_axis.z.abs() * extent.y
            + view_from_local.z_axis.z.abs() * extent.z;
        c.z - depth
    }

    /// The box's back, however it is turned, scaled and seen: every corner of the
    /// box is in front of it (or on it), and its farthest corner is on it; no
    /// farther back than its sphere's far side, and as far as its own depth when
    /// seen square on (a weapon standing in a pod: only its thickness).
    #[test]
    fn the_shell_goes_to_the_back_of_its_box() {
        let (centre, extent) = (Vec3::new(0.1, -0.05, 0.3), Vec3::new(0.08, 0.15, 0.6));
        let views = [
            Mat4::look_at_rh(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, Vec3::Y),
            Mat4::look_at_rh(Vec3::new(3.0, 2.0, -1.5), Vec3::ZERO, Vec3::Y),
            Mat4::look_at_rh(Vec3::new(-0.5, 5.0, 0.5), Vec3::ZERO, Vec3::Z),
        ];
        let models = [
            Mat4::IDENTITY,
            Mat4::from_scale_rotation_translation(
                Vec3::splat(1.3),
                Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2) * Quat::from_rotation_y(0.4),
                Vec3::new(0.2, 2.0, -0.7),
            ),
        ];
        for view in views {
            for model in models {
                let m = view * model;
                let back = back_of(m, centre, extent);
                let mut deepest = f32::INFINITY;
                for x in [-1.0, 1.0] {
                    for y in [-1.0, 1.0] {
                        for z in [-1.0, 1.0] {
                            let corner = centre + extent * Vec3::new(x, y, z);
                            let d = m.transform_point3(corner).z;
                            assert!(d >= back - 1e-4, "corner {d} behind the back {back}");
                            deepest = deepest.min(d);
                        }
                    }
                }
                assert!((deepest - back).abs() < 1e-4);
                let scale = model.x_axis.truncate().length();
                let sphere_back = m.transform_point3(centre).z - extent.length() * scale;
                assert!(back >= sphere_back - 1e-4);
            }
        }
        // (Square on, the box's own depth: a weapon's thickness, not its length.)
        let back = back_of(views[0], centre, extent);
        assert!((back - (centre.z - 4.0 - extent.z)).abs() < 1e-4);
    }

    /// Where pickup_outline.wgsl moves a corner of the shell at `p` (world space),
    /// seen from `eye` looking down `view`'s -z, given its box's back `back` (view
    /// depth) and the plane behind it (`backstop`, OutlineBackstop's).
    fn moved(view: Mat4, eye: Vec3, p: Vec3, back: f32, backstop: Vec4) -> Vec3 {
        const BACKSTOP_GAP: f32 = 0.02;
        let at = view.transform_point3(p);
        if !(at.z > back && at.z < -1e-4 && back < 0.0) {
            return p;
        }
        let mut k = back / at.z;
        let n = backstop.truncate();
        if n.length_squared() > 0.0 {
            let e = n.dot(eye) + backstop.w;
            let here = n.dot(p) + backstop.w;
            if e > BACKSTOP_GAP && here < e {
                k = ((e - BACKSTOP_GAP) / (e - here)).clamp(1.0, k);
            }
        }
        eye + (p - eye) * k
    }

    /// In a pod the outline goes back to the box's back as anywhere else, but no
    /// further than just in front of the wall behind it, along the same line from
    /// the eye (the same place on the screen); a corner already in the wall stays.
    #[test]
    fn a_pods_wall_stops_the_outline_going_back() {
        let eye = Vec3::new(4.0, 2.0, 0.5);
        let view = Mat4::look_at_rh(eye, Vec3::new(0.0, 2.0, 0.0), Vec3::Y);
        // (The pod faces +X; its wall 1 m behind its middle; the weapon's box's back
        // 1.4 m behind it.)
        let wall = OutlineBackstop::facing(Vec3::X, Vec3::new(-1.0, 0.0, 0.0)).0;
        let back = view.transform_point3(Vec3::new(-1.4, 2.0, 0.0)).z;
        let p = Vec3::new(-0.3, 2.3, 0.2);
        let free = moved(view, eye, p, back, Vec4::ZERO);
        let stopped = moved(view, eye, p, back, wall);
        assert!(free.x < -1.0, "{free}");
        assert!((stopped.x - (-1.0 + 0.02)).abs() < 1e-4, "{stopped}");
        for q in [free, stopped] {
            let along = (q - eye).normalize().dot((p - eye).normalize());
            assert!((along - 1.0).abs() < 1e-5);
        }
        // (A corner in the wall is left where it is; one with no wall behind it
        // to stop it, or seen from behind the wall, goes to the box's back.)
        let inside = Vec3::new(-1.2, 2.3, 0.2);
        assert!(moved(view, eye, inside, back, wall).distance(inside) < 1e-5);
        let behind_eye = Vec3::new(-3.0, 2.0, 0.5);
        let view2 = Mat4::look_at_rh(behind_eye, Vec3::new(0.0, 2.0, 0.0), Vec3::Y);
        let back2 = view2.transform_point3(Vec3::new(0.0, 2.0, 0.0)).z - 0.6;
        let q = Vec3::new(-1.5, 2.1, 0.1);
        assert_eq!(
            moved(view2, behind_eye, q, back2, wall),
            moved(view2, behind_eye, q, back2, Vec4::ZERO)
        );
    }

    /// What a pod made outlines no further back than the pod's wall (the plane on
    /// the pickup's entity, above its parts); a station's pickup has none.
    #[test]
    fn a_pod_pickups_shell_takes_its_wall() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<PickupOutlineMaterial>();
        let world = app.world_mut();
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(0.4, 0.2, 1.0));
        let glow = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let wall = OutlineBackstop::facing(Vec3::X, Vec3::new(-1.0, 0.0, 0.0));
        let mut shells = Vec::new();
        for backstop in [Some(wall), None] {
            let mut root = world.spawn((Transform::default(), Visibility::default()));
            if let Some(b) = backstop {
                root.insert(b);
            }
            let root = root.id();
            let node = world
                .spawn((Transform::default(), Visibility::default(), ChildOf(root)))
                .id();
            world.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(glow.clone()),
                GltfMaterialName("PickupOutline".into()),
                Transform::default(),
                ChildOf(node),
            ));
            shells.push(node);
        }
        world.run_system_once(outline_silhouettes).unwrap();
        let mut copies = world.query::<(&ChildOf, &MeshMaterial3d<PickupOutlineMaterial>)>();
        let planes: Vec<(Entity, Vec4)> = copies
            .iter(world)
            .map(|(c, m)| {
                let ext = &world
                    .resource::<Assets<PickupOutlineMaterial>>()
                    .get(m.id())
                    .unwrap()
                    .extension;
                (c.parent(), ext.backstop)
            })
            .collect();
        assert_eq!(planes.len(), 2);
        let of = |node: Entity| planes.iter().find(|(p, _)| *p == node).unwrap().1;
        assert_eq!(of(shells[0]), wall.0);
        assert_eq!(of(shells[1]), Vec4::ZERO);
    }

    /// A shell coming in keeps its own material, hidden, and a copy beside it (under
    /// the same node, where it stands) draws it in the outline material, casting no
    /// shadow; other parts are left as they are.
    #[test]
    fn a_shell_is_drawn_by_a_copy_in_the_outline_material() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<PickupOutlineMaterial>();
        let world = app.world_mut();
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(0.4, 0.2, 1.0));
        let (glow, gun) = {
            let mut m = world.resource_mut::<Assets<StandardMaterial>>();
            (
                m.add(StandardMaterial::default()),
                m.add(StandardMaterial::default()),
            )
        };
        let node = world
            .spawn((Transform::default(), Visibility::default()))
            .id();
        let at = Transform::from_xyz(0.0, 0.1, 0.0);
        let shell = world
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(glow.clone()),
                GltfMaterialName("PickupOutlineHeavy".into()),
                at,
                ChildOf(node),
            ))
            .id();
        let part = world
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(gun),
                GltfMaterialName("WEP_Shotgun_MatINST_Pickup".into()),
                ChildOf(node),
            ))
            .id();
        world.run_system_once(outline_silhouettes).unwrap();
        // (The shell: hidden, its own material kept.)
        assert_eq!(world.get::<Visibility>(shell), Some(&Visibility::Hidden));
        assert!(
            world
                .get::<MeshMaterial3d<StandardMaterial>>(shell)
                .is_some()
        );
        assert!(
            world
                .get::<MeshMaterial3d<PickupOutlineMaterial>>(shell)
                .is_none()
        );
        // (The copy: beside it, where it stands, in the outline material.)
        let mut copies = world.query::<(
            Entity,
            &ChildOf,
            &Transform,
            &Mesh3d,
            &MeshMaterial3d<PickupOutlineMaterial>,
            Has<NotShadowCaster>,
        )>();
        let found: Vec<_> = copies
            .iter(world)
            .map(|(e, c, t, m, _, n)| (e, c.parent(), *t, m.0.id(), n))
            .collect();
        assert_eq!(found.len(), 1);
        let (copy, parent, t, m, no_shadow) = found[0];
        assert_ne!(copy, shell);
        assert_eq!(parent, node);
        assert_eq!(t, at);
        assert_eq!(m, mesh.id());
        assert!(no_shadow);
        // (Round the mesh's box: its middle and half its size each way.)
        let outline = {
            let id = world
                .get::<MeshMaterial3d<PickupOutlineMaterial>>(copy)
                .unwrap()
                .id();
            world
                .resource::<Assets<PickupOutlineMaterial>>()
                .get(id)
                .unwrap()
                .extension
                .clone()
        };
        assert!(outline.centre.truncate().length() < 1e-5);
        assert!((outline.extent.truncate() - Vec3::new(0.2, 0.1, 0.5)).length() < 1e-5);
        // (The gun's part: untouched.)
        assert_ne!(world.get::<Visibility>(part), Some(&Visibility::Hidden));
    }

    /// Every outline shell the tools build is one of these (by its material's name).
    #[test]
    fn the_shells_are_found_by_their_material() {
        for name in ["PickupOutline", "PickupOutlineHeavy"] {
            assert!(name.starts_with(OUTLINE_MATERIAL_PREFIX));
        }
        assert!(!"StoragePodOutline_MAT".starts_with(OUTLINE_MATERIAL_PREFIX));
        assert!(!"ConsoleOutline".starts_with(OUTLINE_MATERIAL_PREFIX));
    }
}
