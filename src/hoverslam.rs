//! The Leader's Hover Slam (MP_Array_BASE_m.xxx's TransContent.TnAbilityHoverMp, from
//! TransGame.TnAbilityHover, driving TnAcrobaticsManager's hover; the multiplayer
//! Destroyer's acrobatics, TR_Acrobatics_p.TruckMPAcrobatics), as the game runs it.
//! Escalation has no entry for it (its only resource entry is multiplayer's), so
//! these are the multiplayer game's numbers:
//!
//! * **The meter** (abilities.rs): a full bar of 20 to start, 10 taken as he goes up,
//!   2 a second while he hovers (5 s), refilling at 0.75 a second. Aerial Camper stops
//!   the drain while he hovers in place (TnSkillHoverMovementBasedDrainRate: under a
//!   tenth of his hover speed).
//! * **Up** (JumpingToHover): he jumps for 12.5 m (HoverJumpHeight) under twice the
//!   gravity (HoverJumpGravityMultiplier), and as he starts down he **hovers**: no
//!   gravity, moving at 6 m/s (HoverAirSpeed; Bug Out's 12 m/s,
//!   TnBuffHoverIncreasedMovement), aiming and shooting as he likes, his blows 1.5
//!   times as strong and those on him half as (TnBuffIncreaseDamageDuringHover,
//!   TnBuffDecreaseDamageTakenWhileHovering). HOVER_JUMP_START_LP plays meanwhile,
//!   HOVER_JUMP_END as it stops.
//! * **Ability again** while he hovers is the **slam** (HoverMeleeFalling): the rest
//!   of the meter goes, and he dives at (25, 5, -10) m/s in his frame
//!   (HoverMeleeVelocity, its 5 to the side mirrored with his gun in his left hand),
//!   under the world's gravity, with no steering. Landing, every enemy within 5 m
//!   (DamageRadius; Divebomb's 6.25 m) takes 150 (Damage, DoFullDamage: no falloff),
//!   with MEGATRON_HOVER_SMASH and the truck's heavy landing; he recovers as from a
//!   hard landing (Melee_HoverGroundSlam_Recover is the truck's skeleton's). That
//!   clip's notifies are the slam's blast, played here as the game plays them as it
//!   starts (mp_shared.xxx's Shared_Truck_ROBO_ANIM): the ground smash on his
//!   RootSocket (HighLeap_GroundSmash_FX, the Leapers' smash: a flash spreading over
//!   the ground, columns and a ring of thick smoke thrown up round him, a spray of
//!   sparks and the air bending round it), on his mesh as the notify puts it
//!   (bAttachToMesh), so lit by the light where he lands (his light environment:
//!   movefx.rs's set_lit, the smoke the tan of the game's videos rather than an
//!   unlit dark haze), and his own view shaking hard (its TnAnimNotify_CameraShake:
//!   Intensity 2.5, Roll 1.75 falling off over a second, the shake over 1.15 s).
//! * **Falling** instead: the meter running out, Jump or a blow while he hovers end it
//!   with no slam (CancelHover).
//! * **Where the slam would land** shows while he hovers or dives (his own view:
//!   TnProjectileHoverSimulation, Hover_Ability_FX at a scale of its radius over 10 m:
//!   the red rings round where it lands).
//!
//! The hover's own clips (Melee_Hover and its take-off and slam) are on the truck's
//! skeleton, so the Infiltrator's skeleton's Nav_Hover (Breakdown_ROBO_ANIM's) plays
//! while he hovers, his jump and fall clips round it. hover_jump_A_FX burns on his
//! boosters meanwhile (as the Whirlwind's clip has it there: the truck clips' own
//! effects weren't read).

use bevy::prelude::*;

use crate::abilities::{
    AERIAL_CAMPER, AbilityInbox, BUG_OUT, ClassAbility, DIVEBOMB, End, Kind, Sockets,
};
use crate::enemies::{Death, Hit};
use crate::levelfx::{FxId, FxSystems};
use crate::net::JazzEvent;
use crate::sound::Sfx;
use crate::{Colliders, Enemy, Level, LocalPlayer, Player, PlayerInput, RemotePlayer};

/// HoverJumpHeight (metres) and HoverJumpGravityMultiplier.
const JUMP_HEIGHT: f32 = 12.5;
const JUMP_GRAVITY: f32 = 2.0;
/// HoverAirSpeed (m/s), Bug Out's multiplier, and Aerial Camper's "in place" (a share
/// of it).
const AIR_SPEED: f32 = 6.0;
const BUG_OUT_SPEED: f32 = 2.0;
const CAMPER_SPEED: f32 = 0.1;
/// HoverMeleeVelocity (forward, right, up: m/s).
const SLAM_VELOCITY: Vec3 = Vec3::new(25.0, 5.0, -10.0);
/// Damage, DamageRadius (metres) and Divebomb's multiplier on the radius.
const SLAM_DAMAGE: f32 = 150.0;
const SLAM_RADIUS: f32 = 5.0;
/// TnDamageTypeHoverGroundPound's GibDistance (metres), and the blast's momentum
/// (Unreal's units: the AI's ground pound's, TitanExplosion's, as the player's
/// ability data gives none of its own).
const SLAM_GIB_DISTANCE: f32 = 12.5;
const SLAM_MOMENTUM: f32 = 150000.0;
const DIVEBOMB_RADIUS: f32 = 1.25;
/// The damage buffs while he hovers.
pub const HOVER_DEALT: f32 = 1.5;
pub const HOVER_TAKEN: f32 = 0.5;
/// The prediction's steps (seconds) and how long it looks ahead at most.
const PREDICT_STEP: f32 = 0.05;
const PREDICT_SECS: f32 = 4.0;
/// SimulationEffectRadius (the effect's rings at scale 1: 10 m).
const MARKER_RADIUS: f32 = 10.0;

const FX_MARKER: &str = "Hover_Ability_FX";
const FX_BOOSTER: &str = "hover_jump_A_FX";
const FX_SLAM: &str = "HighLeap_GroundSmash_FX";
const SLAM_SOCKET: &str = "RootSocket";
/// Melee_HoverGroundSlam_Recover's TnAnimNotify_CameraShake (his own view:
/// TnLocalPlayerFinderOwner).
const SLAM_SHAKE: crate::CameraShake = crate::CameraShake {
    intensity: 2.5,
    roll: 1.75,
    roll_secs: 1.0,
    secs: 1.15,
};
const BOOSTERS: [&str; 2] = ["BoosterSocket_L", "BoosterSocket_R"];
const ENERGON: &[(&str, [u8; 4])] = &[("EnergonColor", [0, 255, 255, 255])];
const SND_LOOP: &str = "BL_TRANS_POWER.HOVER_JUMP_START_LP";
const SND_END: &str = "BL_TRANS_POWER.HOVER_JUMP_END";
const SND_SLAM: &str = "BL_TRANS_POWER.MEGATRON_HOVER_SMASH";
const SND_SLAM_LAND: &str = "BL_VEH_TRUCK.VEH_TRUCK_02_HOVER_LAND_HEAVY";

/// The Ability event of a slam landing (where, its radius).
pub const WHAT_SLAM: u8 = 4;

/// Where his hover is (TnAcrobaticsManager's states): none, going up
/// (JumpingToHover), hovering, or diving into the slam (HoverMeleeFalling).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    None = 0,
    Rising = 1,
    Hovering = 2,
    Slamming = 3,
}

impl Phase {
    pub fn from_bits(b: u8) -> Phase {
        match b {
            1 => Phase::Rising,
            2 => Phase::Hovering,
            3 => Phase::Slamming,
            _ => Phase::None,
        }
    }
}

/// A Jazz's hover: its phase, the slam's velocity once it dives, whether he hovers
/// in place (Aerial Camper), the boosters' effects and the landing marker.
#[derive(Default)]
pub(crate) struct HoverState {
    pub phase: Phase,
    slam: Vec3,
    still: bool,
    boosters: Vec<(&'static str, FxId)>,
    marker: Option<FxId>,
}

/// The meter's drain while it is going (abilities.rs): Aerial Camper's none while he
/// hovers in place.
pub fn drain(a: &ClassAbility) -> f32 {
    if a.has(AERIAL_CAMPER) && a.hover.phase == Phase::Hovering && a.hover.still {
        0.0
    } else {
        a.kind.meter().drain
    }
}

/// His hover speed (Bug Out's twice).
fn air_speed(a: &ClassAbility) -> f32 {
    AIR_SPEED * if a.has(BUG_OUT) { BUG_OUT_SPEED } else { 1.0 }
}

/// The slam's radius (Divebomb's larger).
fn slam_radius(a: &ClassAbility) -> f32 {
    SLAM_RADIUS
        * if a.has(DIVEBOMB) {
            DIVEBOMB_RADIUS
        } else {
            1.0
        }
}

/// His movement through the hover (main.rs's move_jazz): his ground movement
/// `walk` (the way he moves at his own speed) becomes his air movement; returns what
/// he moves at across the ground and the gravity's multiplier, or None when he isn't
/// hovering.
pub fn movement(player: &mut Player, walk: Vec3) -> Option<(Vec3, f32)> {
    let a = &player.ability;
    if a.kind != Kind::Hover {
        return None;
    }
    match a.hover.phase {
        Phase::None => None,
        Phase::Rising => Some((walk.clamp_length_max(air_speed(a)), JUMP_GRAVITY)),
        Phase::Hovering => {
            player.vertical_velocity = 0.0;
            let speed = air_speed(a);
            let v = walk.normalize_or_zero() * speed;
            Some((v, 0.0))
        }
        Phase::Slamming => Some((a.hover.slam.with_y(0.0), 1.0)),
    }
}

/// The slam's velocity for a Jazz facing `heading` (radians), his gun in his left
/// hand or not.
fn slam_velocity(heading: f32, left: bool) -> Vec3 {
    let turn = Quat::from_rotation_y(heading);
    let forward = turn * Vec3::NEG_Z;
    let right = turn * Vec3::X;
    let side = if left {
        -SLAM_VELOCITY.y
    } else {
        SLAM_VELOCITY.y
    };
    forward * SLAM_VELOCITY.x + right * side + Vec3::Y * SLAM_VELOCITY.z
}

/// Where a dive from `from` at `velocity` lands (the first floor under its path), as
/// TnProjectileHoverSimulation predicts it.
fn landing(
    level: &Level,
    colliders: &Colliders,
    from: Vec3,
    mut velocity: Vec3,
    gravity: f32,
) -> Option<Vec3> {
    let mut at = from;
    let mut t = 0.0;
    while t < PREDICT_SECS {
        velocity.y += gravity * PREDICT_STEP;
        let step = velocity * PREDICT_STEP;
        let len = step.length();
        if len > 1e-5
            && let Some((d, _)) = level.shot_surface(colliders, at, step / len, len)
        {
            return Some(at + step / len * d);
        }
        at += step;
        t += PREDICT_STEP;
    }
    None
}

/// Every Jazz's hover: its phases (his PC's: up, hovering, the slam, the fall; the
/// others' from their snapshots), the slam's landing (his PC's blow; the others' from
/// their events), its effects and sounds.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn run_hover(
    (level, colliders): (Res<Level>, Res<Colliders>),
    lighting: Option<Res<crate::lighting::MapLighting>>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut inbox: ResMut<AbilityInbox>,
    mut sockets: Sockets,
    mut players: Query<(
        Entity,
        &Transform,
        &mut Player,
        &PlayerInput,
        Has<RemotePlayer>,
        Has<LocalPlayer>,
    )>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
) {
    for (jazz, at, _, radius) in inbox.take(WHAT_SLAM) {
        let lit = (&*level, lighting.as_deref());
        slam_effect(&mut fx, &mut sockets, lit, jazz, at);
        slam(&mut enemies, at, radius.clamp(0.0, 10.0), jazz);
    }
    for (entity, transform, mut player, input, remote, local) in &mut players {
        let player = &mut *player;
        let feet = transform.translation;
        if player.ability.kind != Kind::Hover {
            end_effects(&mut player.ability, &mut fx);
            continue;
        }
        let heading = player.heading;
        let left = player.left_handed;
        let grounded = player.grounded;
        let speed = player.velocity.length();
        if !remote {
            let a = &mut player.ability;
            if a.started {
                // Up: the jump for the hover's height under its own gravity.
                a.hover.phase = Phase::Rising;
                player.vertical_velocity =
                    (2.0 * -crate::GRAVITY * JUMP_GRAVITY * JUMP_HEIGHT).sqrt();
                player.grounded = false;
                player.jump_time = Some(0.0);
                player.start_clip = Some((crate::ANIM_JUMP, 1.0));
            }
            let a = &mut player.ability;
            match a.ended {
                // The slam: what is left of the meter goes, and he dives.
                Some(End::Cancelled) if a.hover.phase == Phase::Hovering => {
                    a.hover.phase = Phase::Slamming;
                    a.resource = 0.0;
                    a.hover.slam = slam_velocity(heading, left);
                    player.vertical_velocity = a.hover.slam.y;
                }
                // Run out, or ended otherwise: he falls.
                Some(_) => a.hover.phase = Phase::None,
                None => {}
            }
            let a = &mut player.ability;
            // Jump or a blow while he hovers: he falls.
            if a.hover.phase == Phase::Hovering && (input.jump || input.melee) {
                a.end(End::Forced);
                a.hover.phase = Phase::None;
            }
            // Over the top of the jump: he hovers.
            if a.hover.phase == Phase::Rising && player.vertical_velocity <= 0.0 && a.active {
                a.hover.phase = Phase::Hovering;
                player.vertical_velocity = 0.0;
            }
            // (Down on a floor as he hovers: it ends.)
            if a.hover.phase == Phase::Hovering && grounded && a.active {
                a.end(End::Forced);
                a.hover.phase = Phase::None;
            }
            a.hover.still = speed < air_speed(a) * CAMPER_SPEED;
            // The slam lands.
            if a.hover.phase == Phase::Slamming && grounded {
                a.hover.phase = Phase::None;
                let r = slam_radius(a);
                sfx.play(SND_SLAM);
                sfx.play(SND_SLAM_LAND);
                slam_effect(
                    &mut fx,
                    &mut sockets,
                    (&*level, lighting.as_deref()),
                    entity,
                    feet,
                );
                if local {
                    player.shake = Some(SLAM_SHAKE);
                }
                slam(&mut enemies, feet, r, entity);
                player.played.push(JazzEvent::Ability {
                    what: WHAT_SLAM,
                    at: feet,
                    dir: Vec3::Y,
                    value: r,
                });
                // (Unless his landing has started it already.)
                if !matches!(player.action, Some((crate::ANIM_LAND_HARD, _))) {
                    crate::start_action(
                        player,
                        crate::ANIM_LAND_HARD,
                        crate::LAND_HARD_SECS,
                        crate::LAND_ANIM_SPEED,
                    );
                }
            }
        }
        let a = &mut player.ability;
        let up = matches!(a.hover.phase, Phase::Rising | Phase::Hovering);
        if a.ended.is_some() && !remote && a.hover.phase != Phase::Slamming {
            sfx.play(SND_END);
        }
        // The loop while he is up, his boosters burning.
        if up {
            sfx.hold(
                entity.to_bits() ^ 0x484f56,
                SND_LOOP,
                remote.then_some(feet + Vec3::Y),
            );
            if a.hover.boosters.is_empty() {
                for socket in BOOSTERS {
                    if let Some((p, x, _)) = sockets.frame(entity, socket)
                        && let Some(id) = fx.spawn(FX_BOOSTER, p, x, ENERGON)
                    {
                        a.hover.boosters.push((socket, id));
                    }
                }
            }
            for (socket, id) in &a.hover.boosters {
                if let Some((p, x, y)) = sockets.frame(entity, socket) {
                    fx.place_turned(*id, p, x, y);
                }
            }
        } else {
            for (_, id) in a.hover.boosters.drain(..) {
                fx.stop(id);
            }
        }
        // Where the slam would land (his own view).
        let predicting = local && matches!(a.hover.phase, Phase::Hovering | Phase::Slamming);
        let landing_at = predicting
            .then(|| {
                let (v, g) = if a.hover.phase == Phase::Slamming {
                    (
                        a.hover.slam.with_y(player.vertical_velocity),
                        crate::GRAVITY,
                    )
                } else {
                    (slam_velocity(heading, left), crate::GRAVITY)
                };
                landing(&level, &colliders, feet + Vec3::Y * 0.5, v, g)
            })
            .flatten();
        match landing_at {
            Some(p) => {
                let scale = slam_radius(a) / MARKER_RADIUS;
                if a.hover.marker.is_none_or(|id| !fx.alive(id)) {
                    a.hover.marker = fx.spawn_scaled(FX_MARKER, p, Vec3::X, scale, &[]);
                }
                if let Some(id) = a.hover.marker {
                    fx.place(id, p, Vec3::X, Vec3::ZERO);
                }
            }
            None => {
                if let Some(id) = a.hover.marker.take() {
                    fx.kill(id);
                }
            }
        }
    }
}

/// The slam's blast (Melee_HoverGroundSlam_Recover's HmAnimNotify_PlayEffect): the
/// ground smash on `jazz`'s RootSocket, turned as it is (at `feet`, upright, without
/// his model), lit by the level's light there as his mesh's light environment
/// lights it (the sun only where it reaches: movefx.rs's in_sun). He stays where he
/// lands while it plays (the hard landing), so it is left there.
fn slam_effect(
    fx: &mut FxSystems,
    sockets: &mut Sockets,
    (level, lighting): (&Level, Option<&crate::lighting::MapLighting>),
    jazz: Entity,
    feet: Vec3,
) {
    let (at, x, y) = sockets
        .frame(jazz, SLAM_SOCKET)
        .unwrap_or((feet, Vec3::X, Vec3::Z));
    let Some(id) = fx.spawn(FX_SLAM, at, x, &[]) else {
        return;
    };
    fx.place_turned(id, at, x, y);
    if crate::movefx::in_sun(Some(level), lighting, at) {
        fx.set_lit(id);
    } else {
        fx.set_lit_shaded(id);
    }
}

/// His hover's effects gone (another ability, or none).
fn end_effects(a: &mut ClassAbility, fx: &mut FxSystems) {
    for (_, id) in a.hover.boosters.drain(..) {
        fx.stop(id);
    }
    if let Some(id) = a.hover.marker.take() {
        fx.kill(id);
    }
    a.hover.phase = Phase::None;
}

/// The slam's blow at `at` (HurtRadius, DoFullDamage): every live enemy whose body is
/// within `radius` takes SLAM_DAMAGE, a melee blow by `jazz` (not 1.5 times: the
/// hover's buff ended as he dove).
fn slam(
    enemies: &mut Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    at: Vec3,
    radius: f32,
    jazz: Entity,
) {
    for (_, _, mut enemy) in enemies.iter_mut() {
        if !enemy.alive() || enemy.reach(at) > radius {
            continue;
        }
        let c = enemy.centre();
        let out = (c - at).normalize_or(Vec3::Y);
        let struck = c - out * enemy.radius() * 0.5;
        // (Its TnDamageTypeHoverGroundPound kills as Explosive, gibbing within its
        // GibDistance, 12.5 m; its momentum the ground pound's blast's.)
        enemy.take(
            SLAM_DAMAGE,
            Some(jazz),
            crate::xp::Blow::Melee,
            Hit::new(Death::Explosive, struck, out)
                .with_momentum(SLAM_MOMENTUM)
                .gibs_within(SLAM_GIB_DISTANCE, at, c)
                .of_type("TnDamageTypeHoverGroundPound"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slam_goes_ahead_down_and_to_the_side() {
        let v = slam_velocity(0.0, false);
        assert!((v - Vec3::new(5.0, -10.0, -25.0)).length() < 1e-4);
        let v = slam_velocity(0.0, true);
        assert!((v.x + 5.0).abs() < 1e-4);
        assert_eq!(Phase::from_bits(Phase::Slamming as u8), Phase::Slamming);
    }
}
