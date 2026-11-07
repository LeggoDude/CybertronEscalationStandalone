//! The Soldier's Whirlwind (TransGame.TnAbilityWhirlwind, a TnAbilityMelee: his
//! multiplayer melee set's MELEE_Whirlwind, TR_Shared_MS_p.MPMeleeSet_Titan in
//! MP_Array_BASE_m.xxx), as the game runs it:
//!
//! * **The meter** (abilities.rs; "MP,ESC"): a full bar of 20 to start, 10 taken as
//!   it starts and 2.1 a second while he spins (4.76 s), refilling at 1 a second. A
//!   press ends it early; it starts on the ground only (AllowFlight False).
//! * **The spin**: Transform_Whirlwind_ROBO over his upper body, his legs walking
//!   and running as they would (GroundSpeedMultiplier 1): the Titan's own,
//!   TR_Iconic_Melee_ANM_p.Titan_ROBO_ANIM's (his robot definition's animation
//!   sets have it, and not Shared_SCOUT_ROBO_ANIM, whose 5.9 s spin the demo used
//!   to play). 1.67 s long: he draws his blade and winds up, then spins round and
//!   round, the clip going back to 1.36 s each time it reaches its end (its
//!   LoopOffsetTime: 0.307 s a turn).
//! * **Its notifies** (only those within the clip fire; its list carries the 5.9 s
//!   clip's later ones, which never do): at its start, TnAnimNotify_WeaponMelee (the
//!   Ion Blade comes out on his gun: melee.rs) and WHIRLWIND_START; the wind-up's
//!   WHIRLWIND_WHOOSH_SINGLE at 0.894 and 1.223 s and its sweeps at 0.896 and
//!   1.116 s; and each turn of the spin (from 1.36 s, again every 0.307 s) a
//!   sweep at 1.369 s, its two trails, each 3 m out along its socket (the blade's
//!   length: the trails its edge sweeps out round him), and a whoosh at 1.518 s.
//!   At 1.361 s on his WeaponTrailSocket Weapon_Trail_Melee_FX, a white-hot burst
//!   (Faction_Whirlwind_MAT, its colour hundreds of times over: a glare) refracting
//!   the scene, a fifth of a second long; at 1.38 s on his WeaponSocket_Melee
//!   Sword_Trail_Whirlwind_FX, a second's distorting smear that brightens what is
//!   behind it. A sweep (its TnAnimNotify_DamageSweep's DamageSweepInfo) strikes
//!   every enemy within a box round his middle (PositionSocket), 5.5 m out
//!   each way in the wind-up and 6.5 m in the spin, 2 m up and down, for 60
//!   (TnDamageTypeWhirlwind, a melee blow), once a sweep. With a full bar (4.76 s)
//!   14 land.
//! * **The camera** (TnAbilityMelee's MeleePCS, TnPCS_Whirlwind): his class's
//!   over-the-shoulder strategy's orbit for the state, 11.75 m out for the Titan's
//!   (Truck_OverTheShoulder_STRATEGY), against its usual 8.75 (robot_cam.rs).
//! * **Upgrades** (each only while he spins): P.o.K.E. Alpha 1.25 times the damage,
//!   Energon Absorbers 0.85 times the damage he takes, Flurry of Blows 1.15 times his
//!   speed.
//!
//! His sweeps go online (abilities.rs's Ability events): the other PCs' copies of
//! the enemies take them too. His spin's sounds and effects play on every PC.

use bevy::prelude::*;

use crate::abilities::{AbilityInbox, Kind, POKE_ALPHA, Sockets};
use crate::enemies::{Death, Hit};
use crate::levelfx::{FxId, FxSystems};
use crate::net::JazzEvent;
use crate::sound::Sfx;
use crate::{Enemy, Player, RemotePlayer};

/// The clip's length (50 frames at 30 a second) and where it goes back to at its
/// end (LoopOffsetTime): a turn of the spin is the time between.
pub const CLIP_SECS: f32 = 50.0 / 30.0;
pub const LOOP_FROM: f32 = 1.36;
/// Its sweeps: when (in the clip), how far out each way (DamageSweepInfo's Extent,
/// half sizes) and for how long (its Duration).
const SWEEPS: [(f32, f32, f32); 3] = [(0.896, 5.5, 0.2), (1.116, 5.5, 0.2), (1.369, 6.5, 0.25)];
/// The sweep's half height (Extent Z, 200 UU).
const SWEEP_HEIGHT: f32 = 2.0;
/// MELEE_Whirlwind's Damage and P.o.K.E. Alpha's multiplier (TnBuffFloatModifier).
const DAMAGE: f32 = 60.0;
const POKE_DAMAGE: f32 = 1.25;
/// Energon Absorbers' damage taken, Flurry of Blows' speed (while he spins).
pub const ABSORBERS_TAKEN: f32 = 0.85;
pub const FLURRY_SPEED: f32 = 1.15;
/// Off the ground longer than this, the spin stops (AllowFlight False).
const AIR_GRACE: f32 = 0.25;

/// The spin's two trails (its HmAnimNotify_PlayEffects, each turn): their effects,
/// sockets and times in the clip. The white-hot one on his hand
/// (Weapon_Trail_Melee_FX, written as Weapon_Trail_Melee_Whirlwind_FX:
/// tools/build_weapon_fx.py) and the distorting smear along the blade.
const TRAILS: [(&str, &str, f32); 2] = [
    (
        "Weapon_Trail_Melee_Whirlwind_FX",
        "WeaponTrailSocket",
        1.361,
    ),
    ("Sword_Trail_Whirlwind_FX", "WeaponSocket_Melee", 1.38),
];
/// How far out along its socket's Z each plays (RelativeLocation, 300 UU: metres of
/// the robot's model, as big as he is drawn).
const TRAIL_OUT: f32 = 3.0;
/// The effects' EnergonColor (a team's own, as emp.rs's).
const ENERGON: &[(&str, [u8; 4])] = &[("EnergonColor", [0, 255, 255, 255])];

const SND_START: &str = "BL_TRANS_POWER.WHIRLWIND_START";
const SND_WHOOSH: &str = "BL_TRANS_POWER.WHIRLWIND_WHOOSH_SINGLE";
const WHOOSHES: [f32; 3] = [0.894, 1.223, 1.518];

/// The Ability event of a sweep (at his middle, the way he faces, its reach).
pub const WHAT_SWEEP: u8 = 1;

/// A Jazz's spin: how long it had gone at the last frame (its clock then: the
/// notifies between that and now fire), its trails out (which of TRAILS each is)
/// and seconds off the ground.
#[derive(Default)]
pub(crate) struct WhirlState {
    last: Option<f32>,
    fx: Vec<(usize, FxId)>,
    airborne: f32,
}

/// The seconds into the clip a spin `t` seconds long is at: through it once, then
/// round and round from LOOP_FROM.
pub fn clip_time(t: f32) -> f32 {
    if t < CLIP_SECS {
        t.max(0.0)
    } else {
        LOOP_FROM + (t - CLIP_SECS).rem_euclid(CLIP_SECS - LOOP_FROM)
    }
}

/// How many times a spin passes the clip's time `at` between `from` and `to` seconds
/// in (from excluded): once for a time before LOOP_FROM, at it; every turn for one
/// in the spin (at `at`, then a turn later, and so on).
fn passes(at: f32, from: f32, to: f32) -> u32 {
    if to <= from {
        return 0;
    }
    if at < LOOP_FROM {
        return u32::from(from < at && at <= to);
    }
    let turn = CLIP_SECS - LOOP_FROM;
    // (The passes at at + k turns, k from 0, within (from, to].)
    let first = ((from - at) / turn).floor() as i64 + 1;
    let last = ((to - at) / turn).floor() as i64;
    (last.max(-1) - first.max(0) + 1).max(0) as u32
}

/// Every Jazz's spin: its sweeps (his PC's; the others' from their events), its
/// trails on his sockets and its sounds (his PC's go to the others as his), each as
/// the clip passes its notify.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_whirlwind(
    time: Res<Time>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut inbox: ResMut<AbilityInbox>,
    mut sockets: Sockets,
    mut players: Query<(Entity, &Transform, &mut Player, Has<RemotePlayer>)>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
) {
    let dt = time.delta_secs();
    // The other players' sweeps, on the enemies here.
    for (jazz, at, facing, reach) in inbox.take(WHAT_SWEEP) {
        let damage = players
            .get(jazz)
            .map_or(DAMAGE, |(_, _, p, _)| sweep_damage(&p.ability));
        sweep(
            &mut enemies,
            &mut sfx,
            (at, facing, reach.clamp(0.0, 10.0)),
            damage,
            jazz,
        );
    }
    for (entity, transform, mut player, remote) in &mut players {
        let player = &mut *player;
        if player.ability.kind != Kind::Whirlwind {
            if !player.ability.whirl.fx.is_empty() {
                for (_, id) in player.ability.whirl.fx.drain(..) {
                    fx.stop(id);
                }
            }
            continue;
        }
        let grounded = player.grounded;
        let centre = player.robot_centre(transform.translation);
        let facing = transform.forward().as_vec3();
        let a = &mut player.ability;
        if a.started {
            a.whirl = WhirlState::default();
        }
        if a.active && !remote {
            // (Off the ground for more than a moment, it stops.)
            a.whirl.airborne = if grounded { 0.0 } else { a.whirl.airborne + dt };
            if a.whirl.airborne > AIR_GRACE {
                a.end(crate::abilities::End::Forced);
            }
        }
        // The trails out follow their sockets (each lives out its time by itself).
        a.whirl.fx.retain(|(_, id)| fx.alive(*id));
        let at = TRAILS.map(|(_, socket, _)| {
            sockets
                .frame3(entity, socket)
                .map(|(p, [x, y, z], size)| (p + z * TRAIL_OUT * size, x, y))
        });
        for &(k, id) in &a.whirl.fx {
            if let Some((p, x, y)) = at[k] {
                fx.place_turned(id, p, x, y);
            }
        }
        let a = &mut player.ability;
        if a.active {
            // The notifies the clip has passed since the last frame (the first frame
            // from before its start: the ones at its start too).
            let now = a.clock;
            let last = a.whirl.last.replace(now).unwrap_or(-1.0);
            let start = last < 0.0;
            for (k, (effect, _, when)) in TRAILS.into_iter().enumerate() {
                for _ in 0..passes(when, last, now) {
                    if let Some((p, x, y)) = at[k]
                        && let Some(id) = fx.spawn(effect, p, x, ENERGON)
                    {
                        fx.place_turned(id, p, x, y);
                        a.whirl.fx.push((k, id));
                    }
                }
            }
            if !remote {
                if start {
                    sfx.play_owned(entity.to_bits(), SND_START, 0.0, 1.0, None);
                }
                for at in WHOOSHES {
                    for _ in 0..passes(at, last, now) {
                        sfx.play(SND_WHOOSH);
                    }
                }
                // The sweeps.
                for (at, reach, _) in SWEEPS {
                    for _ in 0..passes(at, last, now) {
                        let damage = sweep_damage(&player.ability);
                        sweep(
                            &mut enemies,
                            &mut sfx,
                            (centre, facing, reach),
                            damage,
                            entity,
                        );
                        player.played.push(JazzEvent::Ability {
                            what: WHAT_SWEEP,
                            at: centre,
                            dir: facing,
                            value: reach,
                        });
                    }
                }
            }
        }
        let a = &mut player.ability;
        if a.ended.is_some() {
            a.whirl.last = None;
        }
    }
}

/// A sweep's damage: MELEE_Whirlwind's, with P.o.K.E. Alpha's.
fn sweep_damage(a: &crate::abilities::ClassAbility) -> f32 {
    DAMAGE * if a.has(POKE_ALPHA) { POKE_DAMAGE } else { 1.0 }
}

/// A sweep from `centre` (his middle), facing `facing`, `reach` out each way: every
/// live enemy whose body is in the box takes `damage`, a melee blow by `jazz`.
fn sweep(
    enemies: &mut Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    sfx: &mut Sfx,
    (centre, facing, reach): (Vec3, Vec3, f32),
    damage: f32,
    jazz: Entity,
) {
    let facing = facing.with_y(0.0).normalize_or(Vec3::NEG_Z);
    let right = facing.cross(Vec3::Y);
    for (_, _, mut enemy) in enemies.iter_mut() {
        if !enemy.alive() {
            continue;
        }
        let c = enemy.centre();
        let d = c - centre;
        // (Its body within the box: its middle, give or take half its size.)
        let r = enemy.radius() * 0.5;
        if d.dot(facing).abs() > reach + r
            || d.dot(right).abs() > reach + r
            || d.y.abs() > SWEEP_HEIGHT + r
        {
            continue;
        }
        let out = d.with_y(0.0).normalize_or(facing);
        let struck = c - out * r;
        enemy.take(
            damage,
            Some(jazz),
            crate::xp::Blow::Melee,
            // (TnDamageTypeWhirlwind, its MELEE_Whirlwind's HitMomentum: 2000 out.)
            Hit::new(Death::Melee, struck, out)
                .with_momentum(2000.0)
                .of_type("TnDamageTypeWhirlwind"),
        );
        sfx.play_at(crate::SND_MELEE_HIT, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clip runs through once, then round and round from 1.36 s: a turn every
    /// 0.307 s.
    #[test]
    fn the_spin_goes_round_from_its_loop_offset() {
        assert!((clip_time(1.0) - 1.0).abs() < 1e-5);
        assert!((clip_time(CLIP_SECS + 0.1) - (LOOP_FROM + 0.1)).abs() < 1e-4);
        let turn = CLIP_SECS - LOOP_FROM;
        assert!((clip_time(CLIP_SECS + 3.0 * turn + 0.05) - (LOOP_FROM + 0.05)).abs() < 1e-4);
        // The wind-up's notifies pass once, the spin's every turn.
        assert_eq!(passes(0.896, -1.0, 10.0), 1);
        assert_eq!(passes(1.369, -1.0, 1.369 + 2.0 * turn + 0.01), 3);
        assert_eq!(passes(1.369, 1.369, 1.369 + turn - 0.01), 0);
        assert_eq!(passes(1.369, 1.0, 1.5), 1);
    }

    /// Both trails are in the weapons' effects (tools/build_weapon_fx.py), on
    /// sockets the robot skeleton has (robot_extras.txt).
    #[test]
    fn the_trails_are_built() {
        let assets = crate::assets_dir();
        let (Ok(fx), Ok(extras)) = (
            std::fs::read_to_string(assets.join("fx/weapons_fx.txt")),
            std::fs::read_to_string(assets.join("models/robot_extras.txt")),
        ) else {
            eprintln!("fx/weapons_fx.txt or models/robot_extras.txt not built: skipped");
            return;
        };
        for (effect, socket, _) in TRAILS {
            assert!(
                fx.lines().any(
                    |l| l.starts_with("system ") && l.split_whitespace().nth(2) == Some(effect)
                ),
                "{effect}"
            );
            assert!(
                extras
                    .lines()
                    .any(|l| l.split('\t').take(2).eq(["socket", socket])),
                "{socket}"
            );
        }
    }

    /// A full bar's spin (4.76 s) lands its wind-up's two sweeps and one a turn of
    /// the spin from 1.369 s (the twelfth at 4.74 s): 14.
    #[test]
    fn a_full_bar_lands_fourteen_sweeps() {
        let m = Kind::Whirlwind.meter();
        let secs = (m.max - m.initial) / m.drain;
        let n: u32 = SWEEPS.iter().map(|(t, _, _)| passes(*t, -1.0, secs)).sum();
        assert_eq!(n, 14);
        // Frame by frame, the same.
        let mut last = -1.0;
        let mut count = 0;
        let mut t = 0.0;
        while t <= secs {
            count += SWEEPS
                .iter()
                .map(|(at, _, _)| passes(*at, last, t))
                .sum::<u32>();
            last = t;
            t += 1.0 / 60.0;
        }
        assert_eq!(count, n);
    }
}
