//! The laser sights the Sharpshooters and the Snipers scan with (their guns'
//! TnWeaponLaserSightExtension: TnWeaponLaserSight's TnAiLaserSight, started,
//! aimed and spread by the scan, ops.rs's ScanSniperEnemy, and eased by the pawn:
//! pawn.rs's LaserSight), as the game draws them:
//!
//! * **Its effect** (the blueprint's Effect, Laser_Sight_FX, on its EffectSocket,
//!   LaserSightSocket: the Sharpshooter's gun's, the Sniper's insect's head): three
//!   beams, one an emitter (kNumberOfBeams; SetBeamTargetPoint by emitter), each
//!   from the socket out along where the sight points turned off it by its spread
//!   (its NormalizedSpread of the blueprint's Spread, a degree) and rolled round it
//!   a third of a turn from the next (its roll running at RollDegreesPerSecond),
//!   as far as its gun reaches (GetTraceRange) or to what it strikes: one beam
//!   traced every kTraceInterval in turn, the others kept at the reach of their
//!   last (GetTraceLocation), all of them as it starts; coloured by its wielder's
//!   energon (its parameter collection's TnParticleSystemParameterEnergonColor,
//!   the effect's Color).
//! * **Its sound** (the blueprint's Sound, the scan's loop): heard from the point of
//!   the beam nearest the listener, eased there (UpdateAudio: four tenths of the
//!   way each frame), let go over SoundFadeOutTime as it stops.
//!
//! Its light (a PointLight on the socket that lights only the players: its
//! template's PlayerOnly lighting channel) isn't drawn, as a Bevy light lights the
//! level as well; nor does its sound's start fade in (SoundFadeInTime) or its
//! spread play on it (SoundParameter): the sounds have no such controls.

use std::collections::HashMap;

use bevy::prelude::*;

use super::{Ai, ue};
use crate::enemies::{Enemy, Kinds};
use crate::levelfx::{FxId, FxSystems};

/// The blueprints' (TnAiLaserSightBlueprint's: the Sharpshooters' and the
/// Snipers' are the same): its effect, its socket, its spread at its widest
/// (degrees), its sound and its sound's fade out.
const EFFECT: &str = "Laser_Sight_FX";
pub const SOCKET: &str = "LaserSightSocket";
const SPREAD_DEGREES: f32 = 1.0;
const SOUND: &str = "BL_WPN_GUN_AI_SNIPER.AI_LASER_SNIPER_SCAN_LP";
const SOUND_FADE_OUT: f32 = 0.5;
/// TnAiLaserSight's kNumberOfBeams, kTraceInterval (seconds) and its beams'
/// length without a gun (GetBeamLength, uu).
const BEAMS: usize = 3;
const TRACE_INTERVAL: f32 = 0.0666;
const NO_GUN_LENGTH: f32 = 10000.0;
/// UpdateAudio: the sound's place moves this share of the way to the beam's nearest
/// point each frame.
const SOUND_EASE: f32 = 0.4;

/// A sight that is on: its effect, its traces (the beam it traces next, when it
/// last traced and how far each beam reached at its last), where its sound is.
pub struct Sight {
    fx: Option<FxId>,
    next_trace: usize,
    last_trace: f32,
    reach: [f32; BEAMS],
    sound_at: Vec3,
}

/// GetBeamTargetLocation: the way a beam goes, the sight's way (`dir`) turned off
/// by `spread` (radians) and rolled `roll` degrees round it.
pub fn beam_way(dir: Vec3, spread: f32, roll: f32) -> Vec3 {
    let theta = roll * ue::DEG_TO_RAD;
    let local = Vec3::new(
        spread.cos(),
        theta.cos() * spread.sin(),
        theta.sin() * spread.sin(),
    )
    .normalize();
    let [x, y, z] = ue::rotator_axes(ue::pitch_of(dir), ue::yaw_of(dir), 0.0);
    x * local.x + y * local.y + z * local.z
}

/// Each AI's laser sight while its scan has it on: its effect on its socket with
/// its beams cut where they strike, and its loop (after the sockets are read,
/// before the effects run).
#[allow(clippy::too_many_arguments)]
pub fn draw_lasers(
    ais: Query<(Entity, &Ai, &Enemy)>,
    players: Query<(&Transform, &crate::Player)>,
    listener: Query<&GlobalTransform, (With<Camera3d>, With<crate::OrbitCamera>)>,
    kinds: Res<Kinds>,
    level: Res<crate::Level>,
    colliders: Res<crate::Colliders>,
    time: Res<Time>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<crate::sound::Sfx>,
    mut sights: Local<HashMap<Entity, Sight>>,
) {
    let now = time.elapsed_secs();
    // (With two players on one screen, each camera.)
    let ears: Vec<Vec3> = listener
        .iter()
        .map(|t| ue::to_ue(t.translation()))
        .collect();
    // (What a beam strikes: the level, a player or another enemy.)
    let cylinders: Vec<(Entity, Vec3, f32, f32)> = players
        .iter()
        .filter(|(_, p)| crate::revive::up(p))
        .map(|(t, p)| {
            let (radius, height) = p.size();
            let half = height * 0.5 * ue::UU_PER_M;
            (
                Entity::PLACEHOLDER,
                ue::to_ue(t.translation) + Vec3::Z * half,
                radius * ue::UU_PER_M,
                half,
            )
        })
        .chain(ais.iter().filter(|(_, _, e)| e.alive()).map(|(e, ai, _)| {
            let p = &ai.pawn;
            (e, p.loc, p.radius, p.half_height)
        }))
        .collect();
    let mut on = Vec::new();
    for (entity, ai, enemy) in &ais {
        let p = &ai.pawn;
        if !p.laser.active || !enemy.alive() {
            continue;
        }
        on.push(entity);
        let source = p.socket_frame(SOCKET).map_or(p.muzzle, |(at, _)| at);
        let dir = ue::normal(p.laser.smoothed - source);
        if dir == Vec3::ZERO {
            continue;
        }
        let sight = sights.entry(entity).or_insert_with(|| {
            let energon = kinds.kinds.get(enemy.kind).map_or(Vec3::X, |k| k.energon);
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let colour = [byte(energon.x), byte(energon.y), byte(energon.z), 255];
            Sight {
                fx: fx.spawn(
                    EFFECT,
                    ue::from_ue(source),
                    ue::dir_from_ue(dir),
                    &[("Color", colour)],
                ),
                next_trace: 0,
                last_trace: f32::NEG_INFINITY,
                reach: [0.0; BEAMS],
                sound_at: p.laser.target,
            }
        });
        let fresh = sight.last_trace == f32::NEG_INFINITY;
        let length = if p.weapon.data.range > 0.0 {
            p.weapon.data.range
        } else {
            NO_GUN_LENGTH
        };
        let spread = p.laser.spread * SPREAD_DEGREES * ue::DEG_TO_RAD;
        for i in 0..BEAMS {
            let way = beam_way(
                dir,
                spread,
                p.laser.roll + i as f32 * (360.0 / BEAMS as f32),
            );
            let target = source + way * length;
            // UpdateBeamTargetLocation: this beam traced in its turn (each at the
            // start), else kept at its last reach.
            let end =
                if !fresh && (sight.next_trace != i || now - sight.last_trace < TRACE_INTERVAL) {
                    source + ue::normal(target - source) * sight.reach[i]
                } else {
                    let mut far = level
                        .shot_surface(
                            &colliders,
                            ue::from_ue(source),
                            ue::dir_from_ue(way),
                            length / ue::UU_PER_M,
                        )
                        .map_or(length, |(t, _)| t * ue::UU_PER_M);
                    for &(e, at, radius, half) in &cylinders {
                        if e == entity {
                            continue;
                        }
                        if let Some(t) = super::ray_cylinder(source, way, at, radius, half)
                            && t < far
                        {
                            far = t;
                        }
                    }
                    sight.reach[i] = far;
                    sight.last_trace = now;
                    sight.next_trace = (i + 1) % BEAMS;
                    source + way * far
                };
            if let Some(id) = sight.fx {
                fx.set_emitter_beam_end(id, i, ue::from_ue(end));
            }
        }
        if let Some(id) = sight.fx {
            fx.place(id, ue::from_ue(source), ue::dir_from_ue(dir), Vec3::ZERO);
        }
        // Its loop from the beam's point nearest the listener, eased there.
        let along = p.laser.target - source;
        let nearest = ears
            .iter()
            .map(|&ear| {
                let t =
                    ((ear - source).dot(along) / along.length_squared().max(1e-6)).clamp(0.0, 1.0);
                let point = source + along * t;
                (point.distance_squared(ear), point)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, nearest)) = nearest {
            sight.sound_at = nearest * SOUND_EASE + sight.sound_at * (1.0 - SOUND_EASE);
        }
        sfx.hold_fading(
            entity.to_bits() ^ 0x4c41_5345,
            SOUND,
            Some(ue::from_ue(sight.sound_at)),
            SOUND_FADE_OUT,
        );
    }
    // The sights turned off (or their AIs gone): their effects stop.
    sights.retain(|e, s| {
        let keep = on.contains(e);
        if !keep && let Some(id) = s.fx {
            fx.stop(id);
        }
        keep
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sight with no spread points its beams along it; spread, each beam leaves
    /// it by the spread, rolled a third of a turn from the next.
    #[test]
    fn beams_spread_round_the_sight() {
        let dir = ue::normal(Vec3::new(1.0, 1.0, 0.2));
        assert!(beam_way(dir, 0.0, 37.0).dot(dir) > 0.99999);
        let spread = 2.0 * ue::DEG_TO_RAD;
        let ways: Vec<Vec3> = (0..3)
            .map(|i| beam_way(dir, spread, 10.0 + 120.0 * i as f32))
            .collect();
        for w in &ways {
            assert!((w.dot(dir).acos() - spread).abs() < 1e-3);
        }
        // (Each a third of a turn round from the next.)
        let across = |w: Vec3| ue::normal(w - dir * w.dot(dir));
        let a = across(ways[0]).dot(across(ways[1])).acos();
        assert!((a - std::f32::consts::TAU / 3.0).abs() < 1e-2);
    }
}
