//! The Guard's scanner beam as the game draws it: TnScannerBeamComponent is a
//! ProjectedMeshComponent, an arc (MPS_Arc) of MeshProjectionParameters'
//! Tesselation segments (20) projected out of the Guard's ScannerSocket, its
//! Distance long (5000 uu) and its SpreadAngle wide (InvestigateScanVerticalAngle,
//! 60 degrees) in the upright plane along the socket (its rotation with the roll
//! taken off: UpdateRotation), cut where the level stops it (its occlusion map),
//! in Scanner_Beam_MAT (additive: a green sheet of scan lines with a band sweeping
//! up and down it and pulses running out along it, brightest at the head and
//! glowing where it meets what it falls on).
//!
//! The arc's UVs as the material reads them: U out along it (0 at the socket, 1 at
//! its full length, so a ray the level cuts short ends at its own U), V across it
//! (0 at its bottom edge, 1 at its top).
//!
//! Its back light (a green PointLight 150 uu ahead of the socket lighting only the
//! players: the PlayerOnly lighting channel) isn't drawn: a Bevy light lights the
//! level as well.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::{Ai, guard, ue};
use crate::levelfx::{FxMaterial, FxSystems};

/// MeshProjectionParameters.Tesselation (ProjectedMeshComponent's default).
const TESSELATION: usize = 20;
/// Its material, by the game's name (in the weapons' effects: tools/build_weapon_fx.py's
/// EXTRA_MATERIALS).
const MATERIAL: &str = "Scanner_Beam_MAT";

/// A Guard's beam: its arc's entity, rebuilt every frame.
#[derive(Component)]
pub struct ScannerBeam {
    by: Entity,
}

/// The beam's material, made once the effects are read (None: they haven't got it).
#[derive(Default)]
pub struct BeamMaterial(Option<Option<Handle<FxMaterial>>>);

/// The arc out of `at` (Unreal's space) along unit `dir`, `spread` degrees across
/// in the upright plane through it and `distance` uu long, each of its rays cut at
/// `reach(from, way, distance)` (the first thing it strikes, uu): its places in the
/// demo's axes with their UVs, ray by ray from its bottom edge to its top.
pub fn arc(
    at: Vec3,
    dir: Vec3,
    spread: f32,
    distance: f32,
    mut reach: impl FnMut(Vec3, Vec3, f32) -> f32,
) -> Vec<(Vec3, Vec2)> {
    let flat = Vec3::Z.cross(dir);
    let side = if flat.length_squared() > 1e-8 {
        flat.normalize()
    } else {
        Vec3::Y
    };
    let up = dir.cross(side);
    let mut out = Vec::with_capacity((TESSELATION + 1) * 2);
    for i in 0..=TESSELATION {
        let v = i as f32 / TESSELATION as f32;
        let a = (v - 0.5) * spread * ue::DEG_TO_RAD;
        let way = dir * a.cos() + up * a.sin();
        let len = reach(at, way, distance).clamp(0.0, distance);
        out.push((ue::from_ue(at), Vec2::new(0.0, v)));
        out.push((ue::from_ue(at + way * len), Vec2::new(len / distance, v)));
    }
    out
}

/// The arc's mesh: its places and UVs (both sets), facing across it (its normal,
/// the upright plane's) with its tangent out along it, white.
fn arc_mesh(points: &[(Vec3, Vec2)], normal: Vec3, along: Vec3) -> Mesh {
    let n = points.len();
    let positions: Vec<[f32; 3]> = points.iter().map(|(p, _)| p.to_array()).collect();
    let uvs: Vec<[f32; 2]> = points.iter().map(|(_, uv)| uv.to_array()).collect();
    let mut indices = Vec::with_capacity(TESSELATION * 6);
    for i in 0..TESSELATION as u32 {
        let (near, far, near2, far2) = (2 * i, 2 * i + 1, 2 * i + 2, 2 * i + 3);
        indices.extend_from_slice(&[near, far, far2, near, far2, near2]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![normal.to_array(); n])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uvs)
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_TANGENT,
        vec![along.extend(1.0).to_array(); n],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; n])
    .with_inserted_indices(Indices::U32(indices))
}

/// Draws each living Guard's beam while its scanner is on its head, where this
/// frame's pose puts its socket (after read_sockets); a beam whose Guard has
/// stopped scanning, died or gone is taken away.
#[allow(clippy::too_many_arguments)]
pub fn draw_beams(
    mut commands: Commands,
    ais: Query<(Entity, &Ai, &crate::enemies::Enemy)>,
    beams: Query<(Entity, &ScannerBeam, &Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fx: ResMut<FxSystems>,
    mut materials: ResMut<Assets<FxMaterial>>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
    level: Res<crate::Level>,
    colliders: Res<crate::Colliders>,
    mut material: Local<BeamMaterial>,
) {
    let mut drawn = Vec::new();
    for (entity, ai, enemy) in &ais {
        let p = &ai.pawn;
        let Some(g) = p.guard.as_ref() else {
            continue;
        };
        if !g.scanner.attached || !enemy.alive() {
            continue;
        }
        let Some((at, axes)) = p.socket_frame(guard::SCANNER_SOCKET) else {
            continue;
        };
        let handle = material
            .0
            .get_or_insert_with(|| {
                fx.mesh_material(MATERIAL, &mut materials, &mut images, &asset_server)
            })
            .clone();
        let Some(handle) = handle else {
            return;
        };
        // (Its rotation with no roll: its X along the socket's.)
        let dir = ue::normal(axes[0]);
        let points = arc(
            at,
            dir,
            g.scanner.spread_angle,
            g.scanner.distance,
            |from, way, max| {
                let (from, way) = (ue::from_ue(from), ue::dir_from_ue(way));
                level
                    .shot_surface(&colliders, from, way, max / ue::UU_PER_M)
                    .map_or(max, |(t, _)| t * ue::UU_PER_M)
            },
        );
        let normal = ue::dir_from_ue(Vec3::Z.cross(dir).normalize_or(Vec3::Y));
        let mesh = arc_mesh(&points, normal, ue::dir_from_ue(dir));
        drawn.push(entity);
        match beams.iter().find(|(_, b, _)| b.by == entity) {
            Some((_, _, m)) => {
                if let Some(old) = meshes.get_mut(&m.0) {
                    *old = mesh;
                }
            }
            None => {
                commands.spawn((
                    ScannerBeam { by: entity },
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(handle),
                    Transform::IDENTITY,
                    Visibility::default(),
                    // (Its places are the world's, and change every frame.)
                    NoFrustumCulling,
                    Name::new("Scanner beam"),
                ));
            }
        }
    }
    for (e, b, _) in &beams {
        if !drawn.contains(&b.by) {
            commands.entity(e).despawn();
        }
    }
}

/// Every beam taken away (a new match).
pub fn clear_beams(mut commands: Commands, beams: Query<Entity, With<ScannerBeam>>) {
    for e in &beams {
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arc: its rays from its bottom edge to its top in the upright plane along
    /// its way, each cut where it strikes, its U the share of its full length it
    /// reaches.
    #[test]
    fn the_arc_spreads_upright_and_stops_at_walls() {
        let at = Vec3::new(0.0, 0.0, 500.0);
        // (A floor 500 uu down: the rays below level strike it.)
        let points = arc(at, Vec3::X, 60.0, 5000.0, |from, way, max| {
            if way.z < 0.0 {
                (-from.z / way.z).min(max)
            } else {
                max
            }
        });
        assert_eq!(points.len(), (TESSELATION + 1) * 2);
        // Bottom edge: 30 degrees down, to the floor (1000 uu out).
        let (near, uv0) = points[0];
        let (far, uv1) = points[1];
        assert_eq!(uv0, Vec2::new(0.0, 0.0));
        let reach = ue::to_ue(far) - ue::to_ue(near);
        assert!((reach.length() - 1000.0).abs() < 1.0, "{reach}");
        assert!((uv1.x - 0.2).abs() < 1e-3);
        // Top edge: 30 degrees up, its full length; all in the upright plane.
        let (_, top_uv) = points[points.len() - 1];
        assert_eq!(top_uv, Vec2::new(1.0, 1.0));
        assert!(points.iter().all(|(p, _)| ue::to_ue(*p).y.abs() < 1e-3));
    }
}
