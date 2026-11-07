//! The AIs' projectiles in the world (HmProjectile, TnProjectile and
//! TnProjectileHoming, each with its class's AiSurvivalData and its projectile
//! mesh's looks: ai.txt's, weapon.rs's ProjectileData): the rocket troopers'
//! rockets (TnProjectileRocketAI), the Guard's blaster rounds
//! (TnProjectileGuardianBlaster) and its barrage from its wings
//! (TnProjectileHeavyGuardianBlaster, its Rocket_Barrage's notifies):
//!
//! * it leaves at its InitialSpeed (along its launcher's aim with its spread:
//!   TnWeapon.ProjectileFire; or its socket's way, a random turn within its
//!   notify's cone: TnProjectileSpawner.SpawnProjectile) and flies under its share
//!   of the world's gravity (GravityScale), no faster than its MaxSpeed (none: as
//!   fast as it goes);
//! * a homing one (TnProjectileHoming) steers at its target, its shooter's enemy
//!   (TnProjectileSpawner.FindTarget): pulled towards his targetable place, led by
//!   his speed (GetLeadOffset: LeadTargetSpeed, MaxLeadDistance), by HomingForce
//!   (or HomingForceScale by its time out), then, once within ClosingDistance of
//!   him (the Closing state), straight at him by ClosingForce, going off
//!   ClosingTime later; it stops steering at a robot it may not lock on to
//!   (CanLockOnToRobots);
//! * it goes off on the level, on a player it touches or on the players'
//!   Diffraction Barriers (world geometry to it: equipment.rs) (the enemies are
//!   on its side: StopsProjectile lets it through them, its shooter too), or is
//!   gone after 14 s (Projectile's LifeSpan);
//! * going off (Explode): its ExplosionEffect (at ExplosionEffectScale) facing
//!   out of what it struck, its ExplosionSound, and HurtRadius: its Damage scaled
//!   by its RadiusDamageScale (FloatScaler.GetValueAtPoint: by the distance in
//!   Unreal units; none given, all of it) to each player whose body it can see
//!   within its DamageRadius (any there, IgnoreHurtRadiusTrace; a barrier between
//!   shields him) and to each barrier within it.
//!
//! It flies as its FlightEffect with its FlightSound's loop.
//!
//! A sticky round (TnProjectileGrenadeLauncher: the jets' rockets,
//! TnProjectileFlyerRocket) sticks in what it strikes instead (OnHitThing: it stops,
//! and AttachToActor: the level where it touched; a player where it struck him, in
//! his own frame, riding him), its fuse lit (HitThing: its LifeSpan its FuseTime) and
//! its flight's loop let go for its FuseSound; it goes off as its fuse runs out
//! (facing back the way it flew: OnWaitingForLifeSpanDestruction), at once on a
//! barrier (ShouldExplodeOnHit: TnBarrierSpawnable), or, facing up, as the player
//! it is in is gone (ShouldDetach: its Base gone or hidden). The player it is in
//! shakes a jet's rocket off by changing form (TnProjectileFlyerRocket.Tick: its
//! detach effect where it was, let go after a second, and its sound at him, and
//! it is gone), his HUD
//! telling him so meanwhile ("<<Transform>> to Disable Sticky Rocket": its
//! TnContextualCommandFlyerRocket, while he isn't downed). Its blast shakes the
//! players' views beyond its reach (its TnCameraShakeProjectileExtension), and
//! while it is out its grenade marker shows on the HUD (ShowMarker).

use std::sync::Arc;

use bevy::prelude::*;

use super::weapon::ProjectileData;
use super::{AiOutputs, ue};
use crate::enemies::{Death, Hit};
use crate::equipment::Barrier;
use crate::levelfx::{FxId, FxSystems};
use crate::{Colliders, Level, LocalPlayer, Player, Sfx};

/// Its collision cylinder's radius (m).
const RADIUS: f32 = 0.1;
/// Projectile's LifeSpan.
const LIFE_SECS: f32 = 14.0;
/// How long a jet's rocket's detach effect plays where it came off before it is
/// let go (its bursts and flash done): its data
/// (FlyersSticky_Detach_FX) keeps its crackling ball and its lightning going for
/// eight loops (24 s), and it hung in the air there long after the rocket was gone.
const DETACH_FX_SECS: f32 = 1.0;
/// The world's gravity (WorldInfo's DefaultGravityZ, -2940 cm/s/s).
const GRAVITY: f32 = -29.4;

/// An AI's projectile in flight.
#[derive(Component)]
pub struct AiProjectile {
    /// Who fired it (its Instigator: it passes through him).
    by: Entity,
    data: Arc<ProjectileData>,
    /// Its velocity (m/s, the demo's axes).
    velocity: Vec3,
    age: f32,
    /// A homing one's target (none: it doesn't steer), and seconds in its Closing
    /// state once it is near him.
    target: Option<Entity>,
    closing: Option<f32>,
    flight: Option<FxId>,
    /// A sticky round stuck (and the way it flew as it struck), and its fuse's
    /// seconds left.
    stuck: Option<(Stuck, Vec3)>,
    fuse: Option<f32>,
}

/// What a sticky round is stuck in: the level, or a player at its place on him (in
/// his own frame: its offset from his origin, turned back by his turn).
#[derive(Clone, Copy, Debug)]
enum Stuck {
    Level,
    Player(Entity, Vec3),
}

/// An AI projectile the HUD marks as a grenade (its data's ShowMarker).
#[derive(Component)]
pub struct Marked;

/// An effect's name as the data gives it (its package path left off).
fn fx_name(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

/// Where a round moving from `at` along unit `dir` for `len` m first touches an
/// upright cylinder (its middle, radius and half height, grown by the round's
/// radius), and the cylinder's surface there.
fn touch(mid: Vec3, radius: f32, half: f32, at: Vec3, dir: Vec3, len: f32) -> Option<(f32, Vec3)> {
    let (r, h) = (radius + RADIUS, half + RADIUS);
    let o = at - mid;
    let flat = |v: Vec3| Vec2::new(v.x, v.z);
    let (o2, d2) = (flat(o), flat(dir));
    if o2.length_squared() <= r * r && o.y.abs() <= h {
        return Some((0.0, -dir));
    }
    let mut best: Option<(f32, Vec3)> = None;
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

/// The projectiles the AIs fired this frame, leaving their launchers.
pub fn spawn_projectiles(
    mut commands: Commands,
    mut outputs: ResMut<AiOutputs>,
    mut fx: ResMut<FxSystems>,
) {
    for l in outputs.launches.drain(..) {
        let at = ue::from_ue(l.from);
        let dir = ue::dir_from_ue(l.dir).normalize_or(Vec3::X);
        let velocity = dir * l.data.speed / ue::UU_PER_M;
        let flight = fx.spawn(fx_name(&l.data.flight_fx), at, dir, &[]);
        let target = l.data.homing.as_ref().and(l.target);
        let marked = l.data.marker;
        let mut e = commands.spawn((
            AiProjectile {
                by: l.by,
                data: l.data,
                velocity,
                age: 0.0,
                target,
                closing: None,
                flight,
                stuck: None,
                fuse: None,
            },
            Transform::from_translation(at),
        ));
        if marked {
            e.insert(Marked);
        }
    }
}

/// TnProjectileHoming's pull this frame (m/s/s): towards where it homes on its
/// target (his middle `aim`, led by his velocity `vel` while it homes, not once it
/// closes), by its force; and whether it is closing on him now.
fn homing_pull(
    data: &ProjectileData,
    at: Vec3,
    aim: Vec3,
    vel: Vec3,
    age: f32,
    closing: bool,
) -> (Vec3, bool) {
    let Some(h) = data.homing.as_ref() else {
        return (Vec3::ZERO, closing);
    };
    // (Homing.UpdateState: within ClosingDistance it closes.)
    let closing = closing || (at - aim).length() * ue::UU_PER_M <= h.closing_distance;
    let (to, force) = if closing {
        (aim, h.closing_force)
    } else {
        // GetLeadOffset: his speed past LeadTargetSpeed.Min, of its range, of
        // MaxLeadDistance, his way.
        let speed = vel.length() * ue::UU_PER_M;
        let (min, max) = h.lead_target_speed;
        let lead = if speed < min || max - min <= 0.0 {
            Vec3::ZERO
        } else {
            let k = ((speed - min) / (max - min)).min(1.0);
            vel / vel.length() * (k * h.max_lead_distance / ue::UU_PER_M)
        };
        let force = h.force_scale.as_ref().map_or(h.force, |s| s.value_at(age));
        (aim + lead, force)
    };
    (
        (to - at).normalize_or_zero() * force / ue::UU_PER_M,
        closing,
    )
}

/// Where a round goes off: at, facing.
struct GoOff(Vec3, Vec3);

/// What came of a round's step in flight: on its way (or stuck), going off, or
/// gone at the end of its life.
enum Flight {
    On,
    Off(GoOff),
    Gone,
}

/// The players' barriers as the rounds meet them (equipment.rs's shapes).
type Barriers = [(Entity, [crate::equipment::BarrierBox; 10])];

/// Flies the AIs' projectiles and sets them off on what they strike: their blasts
/// hurting the players near enough. A sticky round sticks instead and goes off as
/// its fuse runs out (or is shaken off: the module's doc); one in the local player
/// puts its line on his HUD.
#[allow(clippy::too_many_arguments)]
pub fn fly_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut projectiles: Query<(Entity, &mut Transform, &mut AiProjectile), Without<Player>>,
    mut players: Query<(Entity, &Transform, &mut Player, Has<LocalPlayer>), Without<AiProjectile>>,
    (mut huds, seats): (
        crate::split::BySeat<crate::hud::HudState>,
        Res<crate::split::Seats>,
    ),
    mut standing: Query<(Entity, &mut Barrier)>,
    (controls, pads, mut barrels): (
        Res<crate::controls::Controls>,
        crate::pad::Pads,
        ResMut<crate::barrels::Barrels>,
    ),
    foes: Query<&crate::enemies::Foe>,
    mut detached: Local<Vec<(FxId, f32)>>,
) {
    let dt = time.delta_secs();
    // (Each of this PC's players': split screen's guest has his own HUD.)
    let mut stuck_in_me = [false; crate::split::SEATS];
    // (The rockets shaken off: their detach effects let go once played.)
    detached.retain_mut(|(id, left)| {
        *left -= dt;
        if *left > 0.0 {
            return true;
        }
        fx.stop(*id);
        false
    });
    if projectiles.is_empty() || dt <= 0.0 {
        for hud in huds.each() {
            hud.sticky_rocket = false;
        }
        return;
    }
    let barriers = crate::equipment::barrier_shapes(standing.iter());
    // The players they may strike (from their feet) and home on: each one's
    // middle, radius, half height, velocity and form.
    let bodies: Vec<(Entity, Vec3, f32, f32, Vec3, bool)> = players
        .iter()
        .filter(|(_, _, p, _)| !(p.dead_left > 0.0 || p.dying || p.waiting))
        .map(|(e, t, p, _)| {
            let (radius, height) = p.size();
            let vel = Vec3::new(p.velocity.x, p.vertical_velocity, p.velocity.z);
            let robot = p.form == crate::Form::Robot;
            (
                e,
                t.translation + Vec3::Y * height * 0.5,
                radius,
                height * 0.5,
                vel,
                robot,
            )
        })
        .collect();
    for (entity, mut transform, mut r) in &mut projectiles {
        let r = &mut *r;
        r.age += dt;
        let at = transform.translation;
        let off = match r.stuck {
            Some((stuck, flew)) => {
                let Some(sticky) = r.data.sticky.as_ref() else {
                    continue;
                };
                // Stuck: it rides what it is in, its fuse burning down.
                let held = match stuck {
                    Stuck::Level => Some(at),
                    Stuck::Player(e, offset) => match players.get(e) {
                        Ok((_, t, p, local)) if !(p.dead_left > 0.0 || p.dying || p.waiting) => {
                            // (A jet's rocket shaken off as he changes form.)
                            if p.transform_left > 0.0
                                && let Some((detach_fx, detach_sound)) = sticky.shaken_off.as_ref()
                            {
                                if let Some(id) = fx.spawn(detach_fx, at, Vec3::Y, &[]) {
                                    detached.push((id, DETACH_FX_SECS));
                                }
                                if !detach_sound.is_empty() {
                                    sfx.play_at(detach_sound, t.translation);
                                }
                                if let Some(id) = r.flight.take() {
                                    fx.stop(id);
                                }
                                commands.entity(entity).despawn();
                                continue;
                            }
                            if local
                                && sticky.shaken_off.is_some()
                                && p.revive.downed.is_none()
                                && let Some(seat) = seats.of(e)
                            {
                                stuck_in_me[seat] = true;
                            }
                            Some(t.translation + t.rotation * offset)
                        }
                        _ => None,
                    },
                };
                match held {
                    None => Some(GoOff(at, Vec3::Y)),
                    Some(now) => {
                        transform.translation = now;
                        if let Some(id) = r.flight {
                            fx.place(id, now, flew, Vec3::ZERO);
                        }
                        let fuse = r.fuse.get_or_insert(sticky.fuse.0);
                        *fuse -= dt;
                        if *fuse <= 0.0 {
                            Some(GoOff(now, -flew))
                        } else {
                            if !sticky.fuse_sound.is_empty() {
                                sfx.hold(entity.to_bits(), sticky.fuse_sound, Some(now));
                            }
                            None
                        }
                    }
                }
            }
            None => match fly(
                entity,
                r,
                &mut transform,
                (&level, &colliders, &barriers),
                &bodies,
                (&players, &mut fx, &mut sfx),
                dt,
            ) {
                Flight::On => None,
                Flight::Off(off) => Some(off),
                Flight::Gone => {
                    if let Some(id) = r.flight.take() {
                        fx.stop(id);
                    }
                    commands.entity(entity).despawn();
                    continue;
                }
            },
        };
        let Some(GoOff(hit, n)) = off else {
            continue;
        };
        // Going off.
        commands.entity(entity).despawn();
        if let Some(id) = r.flight.take() {
            fx.stop(id);
        }
        let d = &*r.data;
        fx.spawn_scaled(fx_name(&d.explosion_fx), hit, n, d.explosion_scale, &[]);
        if !d.explosion_sound.is_empty() {
            sfx.play_at(d.explosion_sound, hit);
        }
        let centre = hit + n * RADIUS;
        let radius = d.damage_radius / ue::UU_PER_M;
        // (An exploding barrel in reach: barrels.rs.)
        barrels.blast(
            centre,
            radius,
            |reach| d.damage * d.radius_scale.value_at(reach * ue::UU_PER_M),
            crate::killfeed::damage_name(&d.damage_type),
            crate::barrels::By::Ai,
        );
        for (_, pt, mut player, local) in &mut players {
            let (body, height) = player.size();
            let mid = pt.translation + Vec3::Y * height * 0.5;
            // (Its camera shake, on the local player's view.)
            if local
                && let Some(felt) = d
                    .shake
                    .as_ref()
                    .and_then(|s| s.felt(mid.distance(centre), radius))
            {
                player.screen_shakes.push(felt);
            }
            let reach = (mid.distance(centre) - body).max(0.0);
            if reach > radius
                || (!d.ignore_hurt_radius_trace
                    && (level.blocked(centre, mid)
                        || crate::equipment::barrier_between(&barriers, centre, mid)))
            {
                continue;
            }
            let amount = d.damage * d.radius_scale.value_at(reach * ue::UU_PER_M);
            if amount <= 0.0 {
                continue;
            }
            let way = (mid - centre).normalize_or(Vec3::Y);
            let hit = Hit::new(Death::Explosive, mid, way)
                .of_type(crate::killfeed::damage_name(&d.damage_type));
            if crate::damage_player(&mut player, amount, hit) {
                // (Its shooter, for the kill feed should it kill him.)
                if let Ok(foe) = foes.get(r.by) {
                    player.last_attack.0 = crate::killfeed::Who::Enemy(foe.kind as u16);
                }
                if local {
                    player.hud_events.push(crate::hud::HudEvent::Damaged {
                        yaw: crate::heading(-way),
                        amount: amount / crate::HIT_INDICATOR_HEALTH,
                    });
                }
            }
        }
        // (And each barrier within its reach: TnBarrierSpawnable takes radius
        // damage.)
        for (e, boxes) in &barriers {
            let reach = crate::equipment::barrier_reach(boxes, centre);
            if reach <= radius
                && let Ok((_, mut b)) = standing.get_mut(*e)
            {
                b.damage(d.damage * d.radius_scale.value_at(reach * ue::UU_PER_M));
            }
        }
    }
    // (A jet's rocket in the local player: its line, with the key he changes form
    // with.)
    for (seat, stuck) in stuck_in_me.into_iter().enumerate() {
        let hud = huds.seat(seat);
        hud.sticky_rocket = stuck;
        if stuck {
            let pad = pads.seat(seat);
            let mode = crate::controls::Mode::Robot;
            let action = crate::controls::Action::ChangeForm;
            let key = crate::control_name(&controls, pad, mode, action);
            if hud.transform_key != key {
                hud.transform_key = key;
            }
            hud.transform_key_mouse = !pad.active
                && controls
                    .bind(mode, action)
                    .is_some_and(|b| !matches!(b, crate::controls::Bind::Key(_)));
        }
    }
}

/// A step of a round in flight: pulled (homing, gravity), it moves on, striking a
/// player (or a barrier, or the level) on its way: going off there, or a sticky one
/// stuck in it (none on a barrier: it goes off); or it closes on its target, or is
/// gone after its life.
#[allow(clippy::type_complexity)]
fn fly(
    entity: Entity,
    r: &mut AiProjectile,
    transform: &mut Transform,
    (level, colliders, barriers): (&Level, &Colliders, &Barriers),
    bodies: &[(Entity, Vec3, f32, f32, Vec3, bool)],
    (players, fx, sfx): (
        &Query<(Entity, &Transform, &mut Player, Has<LocalPlayer>), Without<AiProjectile>>,
        &mut FxSystems,
        &mut Sfx,
    ),
    dt: f32,
) -> Flight {
    let at = transform.translation;
    // Its pull: a homing one's at its target (Homing.ShouldStopHoming: not a
    // robot it may not lock on to), and gravity's share.
    let mut accel = Vec3::Y * GRAVITY * r.data.gravity_scale;
    if let Some(t) = r.target
        && let Some(&(_, mid, _, _, vel, robot)) = bodies.iter().find(|b| b.0 == t)
        && r.data
            .homing
            .as_ref()
            .is_none_or(|h| h.can_lock_on_to_robots || !robot)
    {
        let (pull, closing) = homing_pull(&r.data, at, mid, vel, r.age, r.closing.is_some());
        accel += pull;
        if closing {
            *r.closing.get_or_insert(0.0) += dt;
        }
    }
    r.velocity += accel * dt;
    let max = r.data.max_speed / ue::UU_PER_M;
    if max > 0.0 && r.velocity.length() > max {
        r.velocity = r.velocity.normalize() * max;
    }
    let motion = r.velocity * dt;
    let len = motion.length();
    let dir = r.velocity.normalize_or(Vec3::X);
    let pawn = bodies
        .iter()
        .filter(|b| b.0 != r.by)
        .filter_map(|b| touch(b.1, b.2, b.3, at, dir, len).map(|(s, n)| (s, n, b.0)))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    // (The level, or a barrier nearer: whether it was a barrier.)
    let wall = level
        .shot_surface(colliders, at, dir, len + RADIUS)
        .map(|(t, n)| (t, n, false));
    let wall = match (
        wall,
        crate::equipment::barrier_hit(barriers, at, dir, len + RADIUS),
    ) {
        (Some(w), Some((t, n, _))) if t < w.0 => Some((t, n, true)),
        (None, Some((t, n, _))) => Some((t, n, true)),
        (w, _) => w,
    };
    let struck = match (pawn, wall) {
        (Some((s, n, e)), w) if w.is_none_or(|(t, ..)| s <= (t - RADIUS).max(0.0)) => {
            Some((s, n, Some(e), false))
        }
        (_, Some((t, n, barrier))) => Some(((t - RADIUS).max(0.0), n, None, barrier)),
        _ => None,
    };
    // (Closing: it goes off ClosingTime after it began to close, facing up.)
    let closed = r
        .data
        .homing
        .as_ref()
        .zip(r.closing)
        .is_some_and(|(h, c)| h.closing_time > 0.0 && c >= h.closing_time);
    match struck {
        Some((s, normal, player, barrier)) => {
            let n = if normal.dot(dir) > 0.0 {
                -normal
            } else {
                normal
            };
            let hit = at + dir * s;
            // A sticky round sticks in what it struck (not a barrier).
            if let Some(sticky) = r.data.sticky.as_ref()
                && !barrier
            {
                let stuck = match player.and_then(|e| players.get(e).ok()) {
                    Some((e, t, ..)) => {
                        transform.translation = hit;
                        Stuck::Player(e, t.rotation.inverse() * (hit - t.translation))
                    }
                    None => {
                        transform.translation = hit + n * RADIUS;
                        Stuck::Level
                    }
                };
                r.stuck = Some((stuck, dir));
                r.velocity = Vec3::ZERO;
                // (Its fuse: FuseTime, at random between its two.)
                let (lo, hi) = sticky.fuse;
                let mut rng = ue::Rng::new(entity.to_bits());
                r.fuse = Some(lo + (hi - lo).max(0.0) * rng.frand());
                if let Some(id) = r.flight {
                    fx.place(id, transform.translation, dir, Vec3::ZERO);
                }
                if !sticky.fuse_sound.is_empty() {
                    sfx.hold(
                        entity.to_bits(),
                        sticky.fuse_sound,
                        Some(transform.translation),
                    );
                }
                return Flight::On;
            }
            Flight::Off(GoOff(hit, n))
        }
        None if closed => Flight::Off(GoOff(at, Vec3::Y)),
        None => {
            transform.translation = at + motion;
            if let Some(id) = r.flight {
                fx.place(id, transform.translation, dir, r.velocity);
            }
            if r.age > LIFE_SECS {
                return Flight::Gone;
            }
            if !r.data.flight_sound.is_empty() {
                sfx.hold(
                    entity.to_bits(),
                    r.data.flight_sound,
                    Some(transform.translation),
                );
            }
            Flight::On
        }
    }
}

/// Every projectile gone (a new match).
pub fn clear_projectiles(
    mut commands: Commands,
    mut fx: ResMut<FxSystems>,
    projectiles: Query<(Entity, &AiProjectile)>,
) {
    for (e, r) in &projectiles {
        if let Some(id) = r.flight {
            fx.stop(id);
        }
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::weapon::{FloatScaler, HomingData};

    #[test]
    fn rounds_touch_a_body_they_fly_at() {
        let mid = Vec3::new(0.0, 2.0, 0.0);
        let (s, n) = touch(mid, 0.8, 2.0, Vec3::new(-10.0, 2.0, 0.0), Vec3::X, 20.0).unwrap();
        assert!((s - (10.0 - 0.8 - RADIUS)).abs() < 1e-4);
        assert!((n - Vec3::NEG_X).length() < 1e-4);
        assert!(touch(mid, 0.8, 2.0, Vec3::new(-10.0, 5.0, 0.0), Vec3::X, 20.0).is_none());
        // Its full damage throughout its radius when its scaler has no Scales.
        assert_eq!(FloatScaler::default().value_at(700.0), 1.0);
    }

    /// The Guard's barrage (TnProjectileHeavyGuardianBlaster's data): pulled at
    /// its target by HomingForce from afar, then within ClosingDistance by
    /// ClosingForce, straight at him.
    #[test]
    fn homing_rounds_pull_then_close() {
        let data = ProjectileData {
            homing: Some(HomingData {
                force: 15000.0,
                closing_distance: 2000.0,
                closing_force: 10000.0,
                closing_time: 0.75,
                can_lock_on_to_robots: true,
                ..Default::default()
            }),
            ..ProjectileData::default()
        };
        let target = Vec3::new(30.0, 1.0, 0.0);
        let (pull, closing) = homing_pull(&data, Vec3::ZERO, target, Vec3::ZERO, 0.5, false);
        assert!(!closing);
        assert!((pull.length() - 150.0).abs() < 1e-3);
        assert!(pull.normalize().dot(target.normalize()) > 0.999);
        let near = Vec3::new(29.0, 1.0, 10.0);
        let (pull, closing) = homing_pull(&data, near, target, Vec3::ZERO, 0.5, false);
        assert!(closing);
        assert!((pull.length() - 100.0).abs() < 1e-3);
    }
}
