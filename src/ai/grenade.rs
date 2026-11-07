//! The soldiers' grenades in the world: TnProjectileGrenade, as Escalation's AI
//! throws it (TnThrowGrenadeOperator.ThrowGrenade: spawned at the throwing hand's
//! socket, tossed to land where it aims a second later), with its AiSurvivalData
//! (WEP_Grenade_AI_p.GrenadeSkill_AI_SV_PROJDATA) and its projectile mesh
//! (WEP_Grenade_p.GrenadeSkill_PROJMESH), as TnProjectileGrenadeBase flies it:
//!
//! * it falls under the world's gravity (GravityScale 1) and bounces off what it
//!   strikes (OnHitThing: reflected, keeping 0.2 of its speed, BounceDampening),
//!   with its bounce sound: the level, the players' Diffraction Barriers (world
//!   geometry to it: equipment.rs), and any player or enemy but its thrower it
//!   touches (ProcessTouch: HitThing, its data not set to go off on a pawn,
//!   ExplodeWhenHittingPawn); it stops once its speed has dropped under 1 m/s
//!   (LowSpeedThreshold);
//! * its fuse starts as it first strikes something (HitThing: its LifeSpan, the
//!   data's FuseTime, 4 s), its flight loop giving way to its fuse's sound;
//! * going off (Explode): Grenade_EXP_FX facing out of what it rests on, its blast
//!   sound, and HurtRadius: 50 damage to each player whose body it can see within
//!   10 m (RadiusDamageScale: all of it within 5 m, falling to none at 10 m; a
//!   barrier between shields him) and to each barrier within that reach
//!   (TnBarrierSpawnable.CanAcceptRadiusDamage), and its camera shake
//!   (TnCameraShakeProjectileExtension) for a player out to 25 m.
//!
//! It flies as its mesh (WEP_Grenade_Flak_STAT: AI_Grenade_FX, a system of its
//! own, tools/build_weapon_fx.py's MESH_SYSTEMS), turned as its thrower faced (the
//! spawn's rotation: it doesn't spin, its data's RotationRate being none), with its
//! trail (FlightEffect, Grenade_Trail_FX).

use bevy::prelude::*;

use super::{AiOutputs, ue};
use crate::enemies::{Death, Hit};
use crate::equipment::{Barrier, BarrierBox};
use crate::levelfx::{FxId, FxSystems};
use crate::riot::ScreenShake;
use crate::{Colliders, Level, LocalPlayer, Player, Sfx};

/// The world's gravity (WorldInfo's DefaultGravityZ, -2940 cm/s/s).
const GRAVITY: f32 = -29.4;
/// Its collision cylinder's radius (20 cm).
const RADIUS: f32 = 0.2;
/// BounceDampening, FuseTime and LowSpeedThreshold (100 cm/s).
const BOUNCE_KEEP: f32 = 0.2;
const FUSE_SECS: f32 = 4.0;
const REST_SPEED: f32 = 1.0;
/// A bounce slower than this is silent (the bounce sound is for real knocks).
const BOUNCE_SOUND_SPEED: f32 = 1.5;
/// Damage and DamageRadius (1000 cm), and RadiusDamageScale's full-damage reach
/// (DistanceScaler_13458: constant to 500 cm, then a line to none at 1000 cm).
const DAMAGE: f32 = 50.0;
const DAMAGE_RADIUS: f32 = 10.0;
const FULL_DAMAGE_RADIUS: f32 = 5.0;
/// TnDamageTypeFlakGrenade's GibDistance (metres).
const FLAK_GIB_DISTANCE: f32 = 12.5;
/// A grenade that never strikes anything goes off after this long anyway.
const MAX_SECS: f32 = 20.0;
/// Its effects and sounds (GrenadeSkill_PROJMESH).
const FX_MESH: &str = "AI_Grenade_FX";
const FX_TRAIL: &str = "Grenade_Trail_FX";
const FX_EXPLOSION: &str = "Grenade_EXP_FX";
const SND_FLIGHT: &str = "BL_WPN_GRENADE.MAGMA_FLIGHT_LP";
const SND_FUSE: &str = "BL_WPN_GRENADE.MAGMA_FUSE";
const SND_BOUNCE: &str = "BL_WPN_GRENADE.GRENADE_FOLEY_SHELL_BOUNCE_HEAVY";
const SND_BLAST: &str = "BL_WPN_GRENADE.MAGMA_EXPL_IMPT_WORLD";
/// TnCameraShakeProjectileExtension_636 (the AI Survival data's): out to 25 m.
const SHAKE: ScreenShake = ScreenShake {
    radius: 25.0,
    secs: 0.8,
    rot_amp: Vec3::new(125.0, 75.0, 50.0),
    rot_freq: Vec3::new(40.0, 25.0, 15.0),
    loc_amp: Vec3::new(8.0, 1.0, 4.0),
    loc_freq: Vec3::new(10.0, 0.0, 5.0),
    fov_amp: 0.0,
    fov_freq: 0.0,
};

/// A soldier's grenade in the air or on the ground.
#[derive(Component)]
pub struct AiGrenade {
    /// Who threw it (its Instigator: it passes through him).
    by: Entity,
    /// Its velocity (m/s, the demo's axes), the way it is turned (its effects' X).
    velocity: Vec3,
    facing: Vec3,
    /// Seconds since it left the hand, and its fuse's seconds left once it has
    /// struck something.
    age: f32,
    fuse: Option<f32>,
    resting: bool,
    /// The last surface it struck (its blast faces out of it).
    normal: Vec3,
    mesh: Option<FxId>,
    trail: Option<FxId>,
}

/// The fraction of its damage a body `reach` metres from the blast takes
/// (RadiusDamageScale).
fn damage_scale(reach: f32) -> f32 {
    if reach <= FULL_DAMAGE_RADIUS {
        1.0
    } else if reach <= DAMAGE_RADIUS {
        1.0 - (reach - FULL_DAMAGE_RADIUS) / (DAMAGE_RADIUS - FULL_DAMAGE_RADIUS)
    } else {
        0.0
    }
}

/// A pawn's collision cylinder (its CylinderComponent): its middle, radius and
/// half height (m, the demo's axes).
#[derive(Clone, Copy, Debug)]
struct Cylinder {
    entity: Entity,
    mid: Vec3,
    radius: f32,
    half: f32,
}

/// Where a grenade moving from `at` along `dir` for `len` m first touches a
/// pawn's cylinder (the grenade's own radius round it), and the cylinder's
/// surface there (its side's outward normal, or its top's or bottom's). One it
/// is already inside it doesn't touch (a touch begins at the edge).
fn touch(cyl: &Cylinder, at: Vec3, dir: Vec3, len: f32) -> Option<(f32, Vec3)> {
    let (r, h) = (cyl.radius + RADIUS, cyl.half + RADIUS);
    let o = at - cyl.mid;
    let flat = |v: Vec3| Vec2::new(v.x, v.z);
    let (o2, d2) = (flat(o), flat(dir));
    if o2.length_squared() <= r * r && o.y.abs() <= h {
        return None;
    }
    let mut best: Option<(f32, Vec3)> = None;
    // Its side.
    let a = d2.length_squared();
    if a > 1e-8 {
        let b = 2.0 * o2.dot(d2);
        let c = o2.length_squared() - r * r;
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let s = (-b - disc.sqrt()) / (2.0 * a);
            if (0.0..=len).contains(&s) && (o.y + dir.y * s).abs() <= h {
                let n = (o2 + d2 * s).normalize_or(-d2.normalize_or(Vec2::X));
                best = Some((s, Vec3::new(n.x, 0.0, n.y)));
            }
        }
    }
    // Its top (falling onto it) or bottom.
    if dir.y.abs() > 1e-6 {
        let face = if dir.y < 0.0 { h } else { -h };
        let s = (face - o.y) / dir.y;
        if (0.0..=len).contains(&s)
            && (o2 + d2 * s).length_squared() <= r * r
            && best.is_none_or(|(bs, _)| s < bs)
        {
            best = Some((s, Vec3::Y * face.signum()));
        }
    }
    best
}

/// A grenade's step of `dt`: it falls, and bounces off the level, the arena's
/// pillars, the players' barriers and the pawns it touches (but its thrower,
/// `by`); the struck surface's normal if it struck one.
#[allow(clippy::too_many_arguments)]
fn step(
    level: &Level,
    colliders: &Colliders,
    barriers: &[(Entity, [BarrierBox; 10])],
    pawns: &[Cylinder],
    by: Entity,
    at: &mut Vec3,
    velocity: &mut Vec3,
    dt: f32,
) -> Option<Vec3> {
    velocity.y += GRAVITY * dt;
    let motion = *velocity * dt;
    let len = motion.length();
    if len < 1e-6 {
        return None;
    }
    let dir = motion / len;
    // (The pawn it touches first, if before the level.)
    let pawn = pawns
        .iter()
        .filter(|c| c.entity != by)
        .filter_map(|c| touch(c, *at, dir, len))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    let wall = level.shot_surface(colliders, *at, dir, len + RADIUS);
    let barrier = crate::equipment::barrier_hit(barriers, *at, dir, len + RADIUS);
    let wall = match (wall, barrier) {
        (Some(w), Some((t, n, _))) if t < w.0 => Some((t, n)),
        (None, Some((t, n, _))) => Some((t, n)),
        (w, _) => w,
    };
    let (to, normal) = match (pawn, wall) {
        (Some((s, n)), w) if w.is_none_or(|(t, _)| s <= (t - RADIUS).max(0.0)) => (s, n),
        (_, Some((t, n))) => ((t - RADIUS).max(0.0), n),
        _ => {
            *at += motion;
            return None;
        }
    };
    *at += dir * to;
    let n = if normal.dot(dir) > 0.0 {
        -normal
    } else {
        normal
    };
    *velocity = (*velocity - 2.0 * velocity.dot(n) * n) * BOUNCE_KEEP;
    Some(n)
}

/// The grenades the soldiers threw this frame (their throws, in Unreal's space),
/// leaving their hands.
pub fn spawn_grenades(
    mut commands: Commands,
    mut outputs: ResMut<AiOutputs>,
    mut fx: ResMut<FxSystems>,
) {
    for t in outputs.throws.drain(..) {
        let at = ue::from_ue(t.from);
        let velocity = ue::dir_from_ue(t.velocity) / ue::UU_PER_M;
        let facing = Vec3::new(velocity.x, 0.0, velocity.z).normalize_or(Vec3::X);
        let mesh = fx.spawn(FX_MESH, at, facing, &[]);
        let trail = fx.spawn(FX_TRAIL, at, facing, &[]);
        commands.spawn((
            AiGrenade {
                by: t.by,
                velocity,
                facing,
                age: 0.0,
                fuse: None,
                resting: false,
                normal: Vec3::Y,
                mesh,
                trail,
            },
            Transform::from_translation(at),
        ));
    }
}

/// Flies the soldiers' grenades, bouncing, and sets them off as their fuses run
/// out: their blasts hurting the players near enough to see them.
#[allow(clippy::too_many_arguments)]
pub fn fly_grenades(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut grenades: Query<(Entity, &mut Transform, &mut AiGrenade), Without<Player>>,
    mut players: Query<(Entity, &Transform, &mut Player, Has<LocalPlayer>), Without<AiGrenade>>,
    ais: Query<(Entity, &super::Ai)>,
    mut standing: Query<(Entity, &mut Barrier)>,
    (foes, mut barrels): (Query<&crate::enemies::Foe>, ResMut<crate::barrels::Barrels>),
) {
    let dt = time.delta_secs();
    if grenades.is_empty() {
        return;
    }
    let barriers = crate::equipment::barrier_shapes(standing.iter());
    // The pawns they may touch: the players (from their feet) and the enemies
    // (not those dying: their collision is off, StartDeadCollisionMode).
    let mut pawns: Vec<Cylinder> = Vec::new();
    for (e, t, p, _) in &players {
        if p.dead_left > 0.0 || p.dying || p.waiting {
            continue;
        }
        let (radius, height) = p.size();
        pawns.push(Cylinder {
            entity: e,
            mid: t.translation + Vec3::Y * height * 0.5,
            radius,
            half: height * 0.5,
        });
    }
    for (e, ai) in &ais {
        let p = &ai.pawn;
        if p.dying {
            continue;
        }
        pawns.push(Cylinder {
            entity: e,
            mid: ue::from_ue(p.loc),
            radius: p.radius / ue::UU_PER_M,
            half: p.half_height / ue::UU_PER_M,
        });
    }
    for (entity, mut transform, mut g) in &mut grenades {
        let g = &mut *g;
        g.age += dt;
        let mut at = transform.translation;
        if !g.resting {
            let speed = g.velocity.length();
            if let Some(normal) = step(
                &level,
                &colliders,
                &barriers,
                &pawns,
                g.by,
                &mut at,
                &mut g.velocity,
                dt,
            ) {
                g.normal = normal;
                if speed > BOUNCE_SOUND_SPEED {
                    sfx.play_at(SND_BOUNCE, at);
                }
                // (HitThing: the fuse starts with its first strike.)
                if g.fuse.is_none() {
                    g.fuse = Some(FUSE_SECS);
                }
                if g.velocity.length() < REST_SPEED {
                    g.resting = true;
                    g.velocity = Vec3::ZERO;
                }
            }
            transform.translation = at;
        }
        for id in [g.mesh, g.trail].into_iter().flatten() {
            fx.place(id, at, g.facing, g.velocity);
        }
        if let Some(f) = g.fuse.as_mut() {
            *f -= dt;
        }
        let gone_off = g.fuse.is_some_and(|f| f <= 0.0) || g.age > MAX_SECS;
        if !gone_off {
            let cue = if g.fuse.is_some() {
                SND_FUSE
            } else {
                SND_FLIGHT
            };
            sfx.hold(entity.to_bits(), cue, Some(at));
            continue;
        }
        // Going off.
        commands.entity(entity).despawn();
        for id in [g.mesh.take(), g.trail.take()].into_iter().flatten() {
            fx.stop(id);
        }
        fx.spawn(FX_EXPLOSION, at, g.normal, &[]);
        sfx.play_at(SND_BLAST, at);
        // (HurtOrigin: a little off what it rests on.)
        let centre = at + g.normal * RADIUS;
        // (An exploding barrel in reach: barrels.rs.)
        barrels.blast(
            centre,
            DAMAGE_RADIUS,
            |reach| DAMAGE * damage_scale(reach),
            "TnDamageTypeFlakGrenade",
            crate::barrels::By::Ai,
        );
        for (_, pt, mut player, local) in &mut players {
            let (radius, height) = player.size();
            let mid = pt.translation + Vec3::Y * height * 0.5;
            let reach = (mid.distance(centre) - radius).max(0.0);
            if local && let Some(felt) = SHAKE.felt(mid.distance(centre), DAMAGE_RADIUS) {
                player.screen_shakes.push(felt);
            }
            let scale = damage_scale(reach);
            if scale <= 0.0
                || level.blocked(centre, mid)
                || crate::equipment::barrier_between(&barriers, centre, mid)
            {
                continue;
            }
            let dir = (mid - centre).normalize_or(Vec3::Y);
            // (Its TnDamageTypeFlakGrenade gibs what it kills within its GibDistance
            // of its thrower; its momentum falls off as its damage does.)
            let mut hit = Hit::new(Death::Explosive, mid, dir)
                .with_momentum(Death::Explosive.momentum() * scale)
                .of_type("TnDamageTypeFlakGrenade");
            if let Ok((_, ai)) = ais.get(g.by) {
                hit = hit.gibs_within(FLAK_GIB_DISTANCE, ue::from_ue(ai.pawn.loc), mid);
            }
            if crate::damage_player(&mut player, DAMAGE * scale, hit) {
                // (Its thrower, for the kill feed should it kill him.)
                if let Ok(foe) = foes.get(g.by) {
                    player.last_attack.0 = crate::killfeed::Who::Enemy(foe.kind as u16);
                }
                if local {
                    player.hud_events.push(crate::hud::HudEvent::Damaged {
                        yaw: crate::heading(-dir),
                        amount: DAMAGE * scale / crate::HIT_INDICATOR_HEALTH,
                    });
                }
            }
        }
        // (And each barrier within its reach: TnBarrierSpawnable takes radius
        // damage.)
        for (e, boxes) in &barriers {
            let scale = damage_scale(crate::equipment::barrier_reach(boxes, centre));
            if scale > 0.0
                && let Ok((_, mut b)) = standing.get_mut(*e)
            {
                b.damage(DAMAGE * scale);
            }
        }
    }
}

/// Every grenade gone (a new match).
pub fn clear_grenades(
    mut commands: Commands,
    mut fx: ResMut<FxSystems>,
    grenades: Query<(Entity, &AiGrenade)>,
) {
    for (e, g) in &grenades {
        for id in [g.mesh, g.trail].into_iter().flatten() {
            fx.stop(id);
        }
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grenades_touch_a_pawns_side_and_top() {
        let cyl = Cylinder {
            entity: Entity::PLACEHOLDER,
            mid: Vec3::new(0.0, 2.0, 0.0),
            radius: 0.8,
            half: 2.0,
        };
        // Flying at its side, a metre up: it touches at the grown radius.
        let (s, n) = touch(&cyl, Vec3::new(-5.0, 1.0, 0.0), Vec3::X, 10.0).unwrap();
        assert!((s - (5.0 - 0.8 - RADIUS)).abs() < 1e-4);
        assert!((n - Vec3::NEG_X).length() < 1e-4);
        // Falling onto its head.
        let (s, n) = touch(&cyl, Vec3::new(0.1, 6.0, 0.0), Vec3::NEG_Y, 5.0).unwrap();
        assert!((s - (6.0 - 4.0 - RADIUS)).abs() < 1e-4);
        assert_eq!(n, Vec3::Y);
        // Passing over it, or starting inside it: no touch.
        assert!(touch(&cyl, Vec3::new(-5.0, 4.5, 0.0), Vec3::X, 10.0).is_none());
        assert!(touch(&cyl, Vec3::new(0.0, 1.0, 0.0), Vec3::X, 10.0).is_none());
    }

    #[test]
    fn blast_falls_off_from_five_to_ten_metres() {
        assert_eq!(damage_scale(0.0), 1.0);
        assert_eq!(damage_scale(5.0), 1.0);
        assert!((damage_scale(7.5) - 0.5).abs() < 1e-6);
        assert_eq!(damage_scale(10.0), 0.0);
        assert_eq!(damage_scale(12.0), 0.0);
    }
}
