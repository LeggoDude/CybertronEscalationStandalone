//! Every Jazz's shield as the game shows it (TnShieldMaterialOverrider, each
//! player pawn's CharacterMaterials' ShieldMaterialOverrider0):
//!
//! * **Coming back** (_PowerUpParticleTemplate, Shield_Restore_FX; from empty,
//!   _FullPowerUpParticleTemplate, Shield_FullRestore_FX): as his shield starts to
//!   fill again after he was hit (its first rise since the hit: OnTick's HealthDelta),
//!   a sphere round him (Sphere_03_MESH in Shield_Break_Mat) sweeps over him and
//!   fades, Restore once the shield is past a twentieth, FullRestore below that.
//! * **Running out** (_PowerDownParticleTemplate, Shield_Break_FX): the same sphere
//!   breaking as the shield goes (a hundredth or less of it left).
//! * **Struck** (_HitParticleTemplate, Squib_Shield_FX): a ripple on the sphere
//!   where the shot came in (CreateHitVisual: the line from where it struck him back
//!   along the blow, through the shield's sphere), at most every half second
//!   (kMinTimeBetweenHitSquibs), while he has a shield.
//!
//! The sphere is the shield's bounds (CacheShieldBounds): in robot form round his
//! bones (ComputeBonesBoundingSphere, as his skeleton stands the first time an
//! effect needs it), in his vehicle round its rigid body's box (half its size x
//! 1.732, with 50 more up for a car, a truck or a tank); the effects are that size
//! over 360 (PlayPoweringEffect's scale), the ripple the square root of that, and
//! follow him (attached to his root bone, the ripple to the pawn).
//!
//! The robot's sphere is measured in his skeleton's own space (the mesh's
//! SpaceBases), before his class's size (RobotMeshMergeScale, his mesh's scale:
//! the Infiltrator 0.6, the Destroyer 0.8, the Titan 0.9, the Scientist 0.7). The
//! powering effects' scale divides that by the mesh's Scale3D (EllipsoidSize / (360
//! x SkelScale)), which stays 1 (his class's size is the mesh's Scale), and the
//! effect, attached to the mesh, takes his size: so the sphere is drawn at the size
//! of the sphere round his bones as he stands, as the game's videos show it (an
//! Infiltrator's no bigger than he is, round his middle). The ripple, on his pawn
//! and not his mesh, keeps the skeleton's own size: the square root of it over 360,
//! where the line back along the blow meets the sphere of that radius.
//!
//! The other players' Jazzes show theirs too, from the shields their PCs send (but
//! for the ripple, whose blows only their own PCs know).

use bevy::prelude::*;

use crate::enemies::Hit;
use crate::levelfx::{FxId, FxSystems};
use crate::ragdoll::Rig;
use crate::{Form, JazzOf, PLAYER_MAX_SHIELD, Player, RemotePlayer, RobotForm};

const FX_RESTORE: &str = "Shield_Restore_FX";
const FX_FULL_RESTORE: &str = "Shield_FullRestore_FX";
const FX_BREAK: &str = "Shield_Break_FX";
const FX_HIT: &str = "Squib_Shield_FX";

/// The shield's share left above which it counts as up (OnTick's and OnTakeDamage's
/// bIsShieldActive), and above which its return plays Restore rather than
/// FullRestore (PlayPowerUpEffect).
const ACTIVE: f32 = 0.01;
const PARTLY: f32 = 0.05;
/// The least rise of the shield's share that starts its return's effect.
const RISE: f32 = 1e-5;
/// kMinTimeBetweenHitSquibs.
const SQUIB_GAP: f32 = 0.5;
/// The pool's bucket for a pawn's ripples (SpawnEmitter's MaxCount, 10).
const MAX_SQUIBS: usize = 10;
/// What the effects' sizes are measured against (PlayPoweringEffect's and
/// CreateHitVisual's 360, Unreal's units).
const SIZE_UNIT: f32 = 360.0;
/// How far back along the blow its line runs, in shield radii
/// (ComputeHitVisualWorldSpaceLocation's 2.1).
const LINE_REACH: f32 = 2.1;

/// A Jazz's shield effects (TnShieldMaterialOverrider's state): its share last
/// frame, whether it was up, whether its return has played since he was last hit,
/// his last ripple's time, the effects going (the sphere coming back or breaking,
/// and the ripples, each with its place and way out in his frame), his robot's
/// bounds (the rig they were measured on, their middle in his frame and their
/// radius, metres), and the blows that struck him this frame (damage_player).
#[derive(Default)]
pub(crate) struct ShieldFx {
    on: bool,
    last: f32,
    up_last_frame: bool,
    played_up: bool,
    last_squib: Option<f32>,
    up: Option<FxId>,
    down: Option<FxId>,
    squibs: Vec<(FxId, Vec3, Vec3)>,
    robot: Option<(Entity, Vec3, f32)>,
    hits: Vec<Hit>,
}

impl ShieldFx {
    /// A blow that got through to him (TnShieldMaterialOverrider.OnTakeDamage: none
    /// that a block stopped).
    pub(crate) fn struck(&mut self, hit: Hit) {
        if self.hits.len() < MAX_SQUIBS {
            self.hits.push(hit);
        }
    }
}

/// The shield's bounds where he is now: its middle (Bevy's place), its size
/// (EllipsoidSize: along his facing, across him and up, metres) and the size of the
/// mesh the powering effects are attached to (his class's size in robot form, 1 for
/// his vehicle: the vehicles' sizes are measured as they are drawn).
struct Bounds {
    centre: Vec3,
    size: Vec3,
    mesh_scale: f32,
}

impl Bounds {
    fn radius(&self) -> f32 {
        self.size.max_element()
    }

    /// The skeleton's own scale for its size (EllipsoidSize / 360, in Unreal's
    /// units), along the effect's axes (his facing, across, up).
    fn own_scale(&self) -> Vec3 {
        self.size * 100.0 / SIZE_UNIT
    }

    /// The powering effects' scale as drawn (PlayPoweringEffect: EllipsoidSize / 360,
    /// on his mesh, its size with it).
    fn scale(&self) -> Vec3 {
        self.own_scale() * self.mesh_scale
    }

    /// The ripple's (CreateHitVisual: the square root of the skeleton's own, at
    /// least 0.01, on his pawn).
    fn squib_scale(&self) -> Vec3 {
        let own = self.own_scale();
        Vec3::new(
            own.x.sqrt().max(0.01),
            own.y.sqrt().max(0.01),
            own.z.sqrt().max(0.01),
        )
    }
}

/// UE3's CombineBoundingSpheres for a sphere and a point: the smallest sphere
/// round both that keeps the first's far side (unchanged if the point is inside).
fn take_in(centre: Vec3, radius: f32, point: Vec3) -> (Vec3, f32) {
    let d = point.distance(centre);
    if d <= radius {
        return (centre, radius);
    }
    let r = (radius + d) * 0.5;
    (centre + (point - centre) / d * (r - radius), r)
}

/// ComputeBonesBoundingSphere: from the first bone, each bone taken in in turn
/// (no padding). None without bones.
fn bones_sphere(points: impl IntoIterator<Item = Vec3>) -> Option<(Vec3, f32)> {
    let mut it = points.into_iter();
    let first = it.next()?;
    Some(it.fold((first, 0.0), |(c, r), p| take_in(c, r, p)))
}

/// Where the line from `start` along `dir` (its whole length) meets the sphere at
/// `centre`, `radius` round: where it leaves it if it starts inside, else where it
/// enters it (LineSphereIntersectionPoint); None if it misses.
fn line_meets_sphere(start: Vec3, dir: Vec3, centre: Vec3, radius: f32) -> Option<Vec3> {
    let a = dir.length_squared();
    if a < 1e-12 {
        return None;
    }
    let m = start - centre;
    let b = m.dot(dir);
    let c = m.length_squared() - radius * radius;
    let disc = b * b - a * c;
    if disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    let t = if c <= 0.0 {
        (-b + root) / a
    } else {
        (-b - root) / a
    };
    (0.0..=1.0).contains(&t).then(|| start + dir * t)
}

/// The vehicle's shield size (CacheShieldBounds with a physics asset, in his
/// vehicle): its rigid body's box (the hull a jet's and a tank's collision use, or
/// its body's reach and height), half of it x 1.732, plus 1 cm each way and, for a
/// car, a truck or a tank, 50 cm more up (a plane's: 1).
fn vehicle_size(kit: &crate::character::Kit) -> Vec3 {
    let (length, width, height) = match kit.hull {
        Some(h) => (h.ahead + h.behind, h.half_width * 2.0, h.top - h.bottom),
        None => (
            kit.body.ahead + kit.body.behind,
            kit.body.half_width * 2.0,
            kit.vehicle_height,
        ),
    };
    let up = if kit.plane.is_some() { 0.01 } else { 0.5 };
    Vec3::new(length, width, height) * 0.5 * 1.732 + Vec3::new(0.01, 0.01, up)
}

/// The robot's sphere round its bones as they stand (in his frame: `frame` his
/// pawn's), measured once for his robot's rig, the first time an effect needs it,
/// and kept: its middle where his bones' sphere's is, its size his skeleton's own
/// (skeleton_radius).
fn robot_bounds(
    fx: &mut ShieldFx,
    player: &Player,
    frame: &Transform,
    rig: Option<&Rig>,
    globals: &Query<&GlobalTransform>,
) -> Bounds {
    let base = rig.map(|r| r.base);
    if fx.robot.is_none_or(|(b, _, _)| Some(b) != base)
        && let Some(rig) = rig
        && let Some((centre, radius)) = bones_sphere(
            rig.bones
                .iter()
                .filter_map(|b| globals.get(*b).ok())
                .map(|g| g.translation()),
        )
    {
        fx.robot = Some((
            rig.base,
            frame.compute_affine().inverse().transform_point3(centre),
            skeleton_radius(radius, player.kit.robot.scale),
        ));
    }
    match fx.robot.filter(|(b, _, _)| Some(*b) == base) {
        Some((_, local, radius)) => Bounds {
            centre: frame.transform_point(local),
            size: Vec3::splat(radius),
            mesh_scale: player.kit.robot.scale,
        },
        // (No skeleton yet: round his robot's middle, half his height.)
        None => Bounds {
            centre: player.robot_centre(frame.translation),
            size: Vec3::splat(skeleton_radius(
                player.kit.robot.height * 0.5,
                player.kit.robot.scale,
            )),
            mesh_scale: player.kit.robot.scale,
        },
    }
}

/// The sphere round his bones as his skeleton's own space has them (the mesh's
/// SpaceBases, before his class's size): his drawn bones' (`drawn`, metres) over
/// that size (`scale`, RobotMeshMergeScale), at least 0.1 cm (CacheShieldBounds'
/// FMax(0.1, ...)).
fn skeleton_radius(drawn: f32, scale: f32) -> f32 {
    (drawn / scale.max(0.01)).max(0.001)
}

/// Each Jazz's shield effects from his shield (TnShieldMaterialOverrider's OnStart,
/// OnTick and OnTakeDamage), for as long as he is in the match and up.
pub(crate) fn shield_effects(
    time: Res<Time>,
    mut fx: ResMut<FxSystems>,
    mut players: Query<(Entity, &Transform, &mut Player, Has<RemotePlayer>)>,
    robots: Query<(&JazzOf, &Rig), With<RobotForm>>,
    globals: Query<&GlobalTransform>,
) {
    let now = time.elapsed_secs();
    for (entity, transform, mut player, remote) in &mut players {
        let player = &mut *player;
        let mut s = std::mem::take(&mut player.shield_fx);
        let share = (player.shield / PLAYER_MAX_SHIELD).clamp(0.0, 1.0);
        let live = player.dead_left <= 0.0 && !player.waiting && player.revive.downed.is_none();
        // His ripples follow him; the spent ones go.
        s.squibs.retain(|(id, at, out)| {
            fx.alive(*id) && {
                fx.place(
                    *id,
                    transform.transform_point(*at),
                    transform.rotation * *out,
                    Vec3::ZERO,
                );
                true
            }
        });
        if !live {
            // (OnStop: the sphere's return cut short; one breaking breaks on.)
            if let Some(id) = s.up.take() {
                fx.stop(id);
            }
            s.down = None;
            s.on = false;
            s.hits.clear();
            player.shield_fx = s;
            continue;
        }
        // (Its bounds, measured the first time an effect needs them:
        // GetShieldBounds.)
        let rig = robots.iter().find(|(j, _)| j.0 == entity).map(|(_, r)| r);
        let bounds_of = |s: &mut ShieldFx| match player.form {
            Form::Robot => robot_bounds(s, player, transform, rig, &globals),
            Form::Vehicle => Bounds {
                centre: player.centre(transform.translation),
                size: vehicle_size(&player.kit),
                mesh_scale: 1.0,
            },
        };
        let forward = *transform.forward();
        if !s.on {
            // OnStart: as he comes into the match (or back), his shield as it is.
            s.on = true;
            s.last = share;
            s.up_last_frame = true;
        }
        // OnTakeDamage: the return cut short, and a ripple where the blow came in.
        // (Another player's blows only his PC knows: his shield dropping here.)
        let hits = std::mem::take(&mut s.hits);
        if !hits.is_empty() || (remote && share < s.last - RISE) {
            if let Some(id) = s.up.take() {
                fx.stop(id);
            }
            s.played_up = false;
        }
        for hit in hits {
            if share <= ACTIVE || s.last_squib.is_some_and(|t| now - t < SQUIB_GAP) {
                continue;
            }
            let bounds = bounds_of(&mut s);
            if let Some(id) = squib(&mut fx, &bounds, player, transform, &hit) {
                if s.squibs.len() >= MAX_SQUIBS {
                    let (old, _, _) = s.squibs.remove(0);
                    fx.kill(old);
                }
                s.squibs.push(id);
            }
            s.last_squib = Some(now);
        }
        // OnTick: its first rise since the hit brings the sphere back; running out
        // breaks it.
        let up = share > ACTIVE;
        if up {
            if !s.played_up && share - s.last > RISE && !s.up.is_some_and(|id| fx.alive(id)) {
                let name = if share > PARTLY {
                    FX_RESTORE
                } else {
                    FX_FULL_RESTORE
                };
                if let Some(id) = s.down.take() {
                    fx.stop(id);
                }
                let bounds = bounds_of(&mut s);
                s.up = fx.spawn_sized(name, bounds.centre, forward, bounds.scale());
                s.played_up = true;
            }
        } else {
            s.played_up = false;
            if s.up_last_frame {
                if let Some(id) = s.up.take() {
                    fx.stop(id);
                }
                if let Some(id) = s.down.take() {
                    fx.stop(id);
                }
                let bounds = bounds_of(&mut s);
                s.down = fx.spawn_sized(FX_BREAK, bounds.centre, forward, bounds.scale());
            }
        }
        s.up_last_frame = up;
        s.last = share;
        // The sphere follows him as long as it plays.
        for id in [&mut s.up, &mut s.down] {
            if id.is_some_and(|i| !fx.alive(i)) {
                *id = None;
            }
        }
        if s.up.is_some() || s.down.is_some() {
            let centre = bounds_of(&mut s).centre;
            for i in [s.up, s.down].into_iter().flatten() {
                fx.place(i, centre, forward, Vec3::ZERO);
            }
        }
        player.shield_fx = s;
    }
}

/// CreateHitVisual: the ripple where blow `hit` meets his shield's sphere (the line
/// from where it struck back along it, 2.1 radii long, stretched by the sphere's
/// shape), turned out from his middle and kept there as he moves (its place and
/// way out in his frame); None if the line misses.
fn squib(
    fx: &mut FxSystems,
    bounds: &Bounds,
    player: &Player,
    frame: &Transform,
    hit: &Hit,
) -> Option<(FxId, Vec3, Vec3)> {
    let radius = bounds.radius();
    // (GetTraceDirection: back along the blow; straight up without one.)
    let back = (-hit.momentum).try_normalize().unwrap_or(Vec3::Y);
    // (Its size along his facing, across him and up, in the world's axes: the game
    // divides by them as they are, in Unreal's x, y, z.)
    let size = Vec3::new(bounds.size.x, bounds.size.z, bounds.size.y).max(Vec3::splat(1e-3));
    let dir = back * radius * LINE_REACH * radius / size;
    let at = line_meets_sphere(hit.at, dir, bounds.centre, radius)?;
    let out = (at - player.centre(frame.translation)).normalize_or(back);
    let id = fx.spawn_sized(FX_HIT, at, out, bounds.squib_scale())?;
    let inverse = frame.compute_affine().inverse();
    Some((
        id,
        inverse.transform_point3(at),
        frame.rotation.inverse() * out,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bones_sphere_takes_every_bone_in() {
        let bones = [
            Vec3::ZERO,
            Vec3::new(0.0, 2.0, 0.0),
            Vec3::new(0.5, 1.0, 0.3),
            Vec3::new(-0.6, 1.4, 0.0),
            Vec3::new(0.0, 2.6, 0.1),
        ];
        let (c, r) = bones_sphere(bones).unwrap();
        for b in bones {
            assert!(b.distance(c) <= r + 1e-4, "{b} outside ({c}, {r})");
        }
        assert!(bones_sphere(std::iter::empty()).is_none());
    }

    /// The robot's sphere is measured at his skeleton's own size (his drawn bones'
    /// sphere over his class's size: the Infiltrator's, 0.6, 1 / 0.6 of it), and
    /// drawn at his: the restore effect's sphere (Sphere_03_MESH, 43 units round at
    /// its size 8: 344 at a scale of 1) about as big as his bones' sphere, for every
    /// class; the ripple the square root of the skeleton's own size over 360.
    #[test]
    fn the_robots_sphere_is_drawn_round_his_bones() {
        assert!((skeleton_radius(1.2, 0.6) - 2.0).abs() < 1e-5);
        assert!((skeleton_radius(1.2, 1.0) - 1.2).abs() < 1e-5);
        // (Never none.)
        assert!(skeleton_radius(0.0, 0.7) > 0.0);
        for scale in [0.6, 0.7, 0.8, 0.9] {
            let b = Bounds {
                centre: Vec3::ZERO,
                size: Vec3::splat(skeleton_radius(1.2, scale)),
                mesh_scale: scale,
            };
            let drawn = 43.0 * 8.0 * b.scale().x / 100.0;
            assert!(
                (drawn - 1.2 * 344.0 / 360.0).abs() < 1e-3,
                "{scale}: {drawn} m round"
            );
            let squib = b.squib_scale().x;
            assert!((squib - (skeleton_radius(1.2, scale) * 100.0 / 360.0).sqrt()).abs() < 1e-5);
        }
    }

    #[test]
    fn the_line_meets_the_sphere_where_it_leaves_it() {
        // From the middle, out along +X: where it leaves.
        let at = line_meets_sphere(Vec3::ZERO, Vec3::X * 4.0, Vec3::ZERO, 2.0).unwrap();
        assert!(at.distance(Vec3::X * 2.0) < 1e-4);
        // From outside, towards it: where it enters.
        let at = line_meets_sphere(Vec3::X * 5.0, Vec3::NEG_X * 4.0, Vec3::ZERO, 2.0).unwrap();
        assert!(at.distance(Vec3::X * 2.0) < 1e-4);
        // Too short, or wide of it: nowhere.
        assert!(line_meets_sphere(Vec3::X * 5.0, Vec3::NEG_X, Vec3::ZERO, 2.0).is_none());
        assert!(line_meets_sphere(Vec3::Y * 5.0, Vec3::X * 9.0, Vec3::ZERO, 2.0).is_none());
    }
}
