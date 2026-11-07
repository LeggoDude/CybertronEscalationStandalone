//! The Scientist's Heal Beam (mp_shared.xxx's TransContent.TnAbilityHealBeam, from
//! TransGame.TnAbilityHealBeamBase; TransContent.TnBuffHealTarget on the one healed),
//! as the game runs it in Escalation:
//!
//! * **The meter** (abilities.rs; "ESC": 1 needed of 15): it drains 1 a second while
//!   the beam is linked (15 s from full; Empowered Beam halves it,
//!   TnBuffHealBeamResourceDrainRate) and refills at 2 a second.
//! * **Ability** picks the teammate under the crosshair (TnPlayerTargeting's repair
//!   pick: taken as the one nearest the crosshair within 10 degrees of it, in sight)
//!   within 45 m (MaximumInitialRange) and links the beam to him. With nobody there it
//!   plays HEAL_FAIL and nothing else happens. Pressed while linked it switches to
//!   another teammate under the crosshair, or ends the beam.
//! * **Linked**, he is healed 75 a second (HealRate: his health, then his shield),
//!   on his own PC (each PC keeps its own Jazz's health). The beam breaks beyond 60 m
//!   (MaximumEngagedRange), out of sight for 1.25 s (LOSHysteresis), with him dead or
//!   down, or with the meter empty.
//! * **Juice**: while the one healed is below full, the healer's juice rises 0.33 a
//!   second; full, it starts over and rewards him (HEAL_SUCCESS and an effect at his
//!   free hand): with Energon Recharge he is healed 100 himself (SelfHeal_FX), with
//!   Battle Medic his damage is 1.15 times for 10 s
//!   (TnBuffIncreaseDamageHealBeamReward, HealDoubleDmg_FX), otherwise HealBeacon_FX.
//!   The HUD's icon (mc_abilityIconHealBeam) fills with the juice.
//! * **Its look**: Skill_Heal over his left arm (his hand coming up, then held up:
//!   round and round from its LoopOffsetTime, clip_time), and HealAbility_Beam_FX on
//!   the one healed (at his middle, PositionSocket), its beams reaching to the
//!   healer's free hand (MeleeSocket_LeftHand, the right one with his gun in his left).
//!   HEAL_INITIALIZE_LP loops at the healer, HEAL_ACTIVE_LP on the one healed;
//!   HEAL_RETRACT as it ends.
//!
//! Only teammates can be healed, so on his own he can't use it (HEAL_FAIL). The game
//! also outlines the one healed (TR_OverShield_Outline_MATINST); the demo doesn't.

use bevy::prelude::*;

use crate::abilities::{
    AbilityInbox, BATTLE_MEDIC, ClassAbility, ENERGON_RECHARGE, End, Kind, Sockets,
};
use crate::levelfx::{FxId, FxSystems};
use crate::net::JazzEvent;
use crate::sound::Sfx;
use crate::{Level, LocalPlayer, Player, PlayerInput, PlayerSlot, RemotePlayer};

/// MaximumInitialRange, MaximumEngagedRange (metres) and LOSHysteresis (seconds).
const PICK_RANGE: f32 = 45.0;
const LINK_RANGE: f32 = 60.0;
const SIGHT_GRACE: f32 = 1.25;
/// The crosshair's reach for a pick (degrees off the view: assumed).
const PICK_CONE: f32 = 10.0;
/// TnBuffHealTarget's HealRate (a second).
const HEAL_RATE: f32 = 75.0;
/// The juice a second while the one healed is hurt (to 1, the reward).
const JUICE_RATE: f32 = 0.33;
/// Energon Recharge's self heal, and Battle Medic's damage buff and its time.
const SELF_HEAL: f32 = 100.0;
pub const MEDIC_DEALT: f32 = 1.15;
const MEDIC_SECS: f32 = 10.0;

/// Skill_Heal (all_shared.xxx's Shared_ROBO_ANIM): its length (31 frames, its
/// SequenceLength), and its LoopOffsetTime, where it goes back to as it reaches its
/// end: his hand comes up once, then stays up, the clip going round from there.
/// (robot_extras.glb has it with the pose there added at its end:
/// build_robot_extras.py's LOOPS.)
pub const CLIP_SECS: f32 = 31.0 / 30.0;
pub const LOOP_FROM: f32 = 0.54;

const FX_BEAM: &str = "HealAbility_Beam_FX";
const FX_REWARD: [&str; 3] = ["HealBeacon_FX", "SelfHeal_FX", "HealDoubleDmg_FX"];
const SND_FAIL: &str = "BL_TRANS_POWER.HEAL_FAIL";
const SND_SUCCESS: &str = "BL_TRANS_POWER.HEAL_SUCCESS";
const SND_LOOP: &str = "BL_TRANS_POWER.HEAL_INITIALIZE_LP";
const SND_TARGET_LOOP: &str = "BL_TRANS_POWER.HEAL_ACTIVE_LP";
const SND_RETRACT: &str = "BL_TRANS_POWER.HEAL_RETRACT";

/// The Ability event of a reward (its effect at the healer's hand: FX_REWARD's).
pub const WHAT_REWARD: u8 = 2;

/// A Jazz's beam: whom it heals (his Jazz here and his slot online), seconds he has
/// been out of sight, the juice, Battle Medic's buff left, and its effect.
#[derive(Default)]
pub(crate) struct HealState {
    pub target: Option<Entity>,
    pub target_slot: Option<u8>,
    lost: f32,
    pub juice: f32,
    pub medic_left: f32,
    beam: Option<FxId>,
}

/// The off hand's socket (the one without the gun).
fn hand_socket(player: &Player) -> &'static str {
    if player.left_handed {
        "MeleeSocket_RightHand"
    } else {
        "MeleeSocket_LeftHand"
    }
}

/// Whether `p` can be healed (alive, in the match, not down).
fn healable(p: &Player) -> bool {
    p.dead_left <= 0.0 && !p.dying && !p.waiting && !p.revive.locked()
}

/// The teammate under the crosshair (within PICK_CONE of the view and PICK_RANGE,
/// in sight), nearest it.
fn pick(
    level: &Level,
    input: &PlayerInput,
    me: Entity,
    others: &[(Entity, Vec3, u8, bool)],
) -> Option<(Entity, u8)> {
    let view = input.view_dir.normalize_or(Vec3::NEG_Z);
    let cos = PICK_CONE.to_radians().cos();
    others
        .iter()
        .filter(|(e, _, _, ok)| *e != me && *ok)
        .filter_map(|(e, at, slot, _)| {
            let d = *at - input.view_origin;
            let dist = d.length();
            let c = d.dot(view) / dist.max(1e-3);
            (dist <= PICK_RANGE && c >= cos && !level.blocked(input.view_origin, *at))
                .then_some((c, *e, *slot))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, e, slot)| (e, slot))
}

/// Every Jazz's beam: his presses (his PC's: picking, switching, ending), the link
/// breaking, the juice and its rewards; its effect, clip and sounds on every PC; and
/// this PC's Jazz healed by the beams on him.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn run_heal_beam(
    time: Res<Time>,
    level: Res<Level>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut inbox: ResMut<AbilityInbox>,
    mut sockets: Sockets,
    mut players: Query<(
        Entity,
        &Transform,
        &mut Player,
        &PlayerInput,
        &PlayerSlot,
        Has<RemotePlayer>,
        Has<LocalPlayer>,
    )>,
) {
    let dt = time.delta_secs();
    // Everyone's middle and slot, and whether he can be healed.
    let others: Vec<(Entity, Vec3, u8, bool)> = players
        .iter()
        .map(|(e, t, p, _, s, _, _)| (e, p.centre(t.translation), s.0 as u8, healable(p)))
        .collect();
    let by_slot = |slot: u8| others.iter().find(|o| o.2 == slot).map(|o| o.0);
    // The size of the effects on each (his robot's: Robot::effect_scale).
    let sizes: Vec<(Entity, f32)> = players
        .iter()
        .map(|(e, _, p, ..)| (e, p.kit.robot.effect_scale()))
        .collect();
    let size_of = |e: Entity| sizes.iter().find(|s| s.0 == e).map_or(1.0, |s| s.1);
    // The rewards' effects for the other players' beams.
    for (jazz, at, dir, kind) in inbox.take(WHAT_REWARD) {
        let size = size_of(jazz);
        fx.spawn_scaled(FX_REWARD[(kind as usize).min(2)], at, dir, size, &[]);
    }
    let mut healing_me = Vec::new();
    for (entity, transform, mut player, input, _, remote, _) in &mut players {
        let player = &mut *player;
        if player.ability.kind != Kind::HealBeam {
            let h = &mut player.ability.heal;
            if let Some(id) = h.beam.take() {
                fx.stop(id);
            }
            h.medic_left = 0.0;
            continue;
        }
        let me = player.centre(transform.translation);
        if !remote {
            player.ability.heal.medic_left = (player.ability.heal.medic_left - dt).max(0.0);
            let a = &mut player.ability;
            if a.pressed {
                // Picking (TryLocalTriggerAbility): a teammate under the crosshair
                // links the beam (or switches it to him); nobody, it fails or ends.
                let picked = pick(&level, input, entity, &others)
                    .filter(|(e, _)| Some(*e) != a.heal.target || !a.active);
                match picked {
                    Some((target, slot)) if a.active || a.resource >= a.kind.meter().required => {
                        a.heal.target = Some(target);
                        a.heal.target_slot = Some(slot);
                        a.heal.lost = 0.0;
                        if !a.active {
                            a.active = true;
                            a.started = true;
                            a.clock = 0.0;
                        }
                    }
                    _ if a.active => a.end(End::Cancelled),
                    _ => sfx.play(SND_FAIL),
                }
            }
            // The link: in range, in sight (or not out of it for long), and him fit.
            if a.active {
                let target = a.heal.target.and_then(|t| others.iter().find(|o| o.0 == t));
                match target {
                    Some(&(_, at, _, ok)) if ok && at.distance(me) <= LINK_RANGE => {
                        let eye = me + Vec3::Y * 0.5;
                        a.heal.lost = if level.blocked(eye, at) {
                            a.heal.lost + dt
                        } else {
                            0.0
                        };
                        if a.heal.lost >= SIGHT_GRACE {
                            a.end(End::Forced);
                        }
                    }
                    _ => a.end(End::Forced),
                }
            }
        }
        let a = &mut player.ability;
        // (Another player's beam: whom his snapshot says it heals.)
        if remote {
            a.heal.target = a.heal.target_slot.and_then(by_slot);
        }
        if a.ended.is_some() || !a.active {
            if let Some(id) = a.heal.beam.take() {
                fx.stop(id);
            }
            if a.ended.is_some() {
                a.heal.juice = 0.0;
                if !remote {
                    sfx.play(SND_RETRACT);
                }
            }
            if !a.active {
                a.heal.target = None;
                a.heal.target_slot = None;
            }
            continue;
        }
        let Some(target) = a.heal.target else {
            continue;
        };
        let Some(&(_, target_at, target_slot, _)) = others.iter().find(|o| o.0 == target) else {
            continue;
        };
        // The beam: on him, reaching back to the healer's free hand.
        let hand = sockets
            .frame(entity, hand_socket(player))
            .map_or(me + Vec3::Y, |(p, _, _)| p);
        let a = &mut player.ability;
        let dir = (hand - target_at).normalize_or(Vec3::Y);
        if a.heal.beam.is_none_or(|id| !fx.alive(id)) {
            // (At his robot's size: the game attaches it to his mesh, TnBuffHealTarget.)
            let size = size_of(target);
            a.heal.beam = fx.spawn_scaled(FX_BEAM, target_at, dir, size, &[]);
        }
        if let Some(id) = a.heal.beam {
            fx.place(id, target_at, dir, Vec3::ZERO);
            fx.set_beam_end(id, hand);
        }
        let owner = entity.to_bits() ^ 0x4845414c;
        sfx.hold(owner, SND_LOOP, remote.then_some(me));
        sfx.hold(owner ^ 1, SND_TARGET_LOOP, Some(target_at));
        healing_me.push((target_slot, entity));
    }
    // The juice: his PC's, as the one he heals is hurt (that one's health as it
    // reaches here); full, the reward.
    let hurt: Vec<(Entity, bool)> = players
        .iter()
        .map(|(e, _, p, _, _, _, _)| {
            (
                e,
                p.health < p.max_health() || p.shield < crate::PLAYER_MAX_SHIELD,
            )
        })
        .collect();
    for (entity, _, mut player, _, _, remote, _) in &mut players {
        let player = &mut *player;
        let a = &player.ability;
        if remote || a.kind != Kind::HealBeam || !a.active {
            continue;
        }
        let wounded = a
            .heal
            .target
            .and_then(|t| hurt.iter().find(|h| h.0 == t))
            .is_some_and(|h| h.1);
        if !wounded {
            continue;
        }
        let a = &mut player.ability;
        a.heal.juice += JUICE_RATE * dt;
        if a.heal.juice >= 1.0 {
            a.heal.juice = 0.0;
            reward(player, entity, &mut sockets, &mut fx, &mut sfx);
        }
    }
    // This PC's Jazz healed by the beams on him (75 a second each: his health, then
    // his shield).
    let healers: Vec<u8> = healing_me.iter().map(|(slot, _)| *slot).collect();
    for (_, _, mut player, _, slot, _, local) in &mut players {
        if !local || !healable(&player) {
            continue;
        }
        let n = healers.iter().filter(|s| **s == slot.0 as u8).count();
        if n > 0 {
            heal(&mut player, HEAL_RATE * n as f32 * dt);
        }
    }
}

/// Where Skill_Heal is `t` seconds into the beam: through the clip once (his hand
/// coming up), then round and round from LOOP_FROM to its end, his hand held up.
pub fn clip_time(t: f32) -> f32 {
    if t < CLIP_SECS {
        t.max(0.0)
    } else {
        LOOP_FROM + (t - CLIP_SECS).rem_euclid(CLIP_SECS - LOOP_FROM)
    }
}

/// Heals `player` by `amount`: his health, then his shield (TnHealTypeHealAbility).
pub fn heal(player: &mut Player, amount: f32) {
    let to_health = amount.min(player.max_health() - player.health).max(0.0);
    player.health += to_health;
    player.shield = (player.shield + amount - to_health).min(crate::PLAYER_MAX_SHIELD);
}

/// The juice's reward (TnAbilityHealBeamBase.FillJuiceBar): its sound and effect at
/// the healer's free hand, and his upgrade's own.
fn reward(
    player: &mut Player,
    entity: Entity,
    sockets: &mut Sockets,
    fx: &mut FxSystems,
    sfx: &mut Sfx,
) {
    let a: &ClassAbility = &player.ability;
    let kind = if a.has(ENERGON_RECHARGE) {
        1
    } else if a.has(BATTLE_MEDIC) {
        2
    } else {
        0
    };
    sfx.play(SND_SUCCESS);
    let (at, x) = sockets
        .frame(entity, hand_socket(player))
        .map_or((Vec3::ZERO, Vec3::X), |(p, x, _)| (p, x));
    if at != Vec3::ZERO {
        // (At his robot's size: the game plays it on his mesh's socket.)
        fx.spawn_scaled(FX_REWARD[kind], at, x, player.kit.robot.effect_scale(), &[]);
        player.played.push(JazzEvent::Ability {
            what: WHAT_REWARD,
            at,
            dir: x,
            value: kind as f32,
        });
    }
    match kind {
        1 => heal(player, SELF_HEAL),
        2 => player.ability.heal.medic_left = MEDIC_SECS,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Skill_Heal plays through once, then goes round from its LoopOffsetTime: never
    /// back to its start (his hand at his waist) while the beam is on.
    #[test]
    fn the_arm_stays_up_while_healing() {
        assert_eq!(clip_time(0.0), 0.0);
        assert_eq!(clip_time(0.5), 0.5);
        for i in 0..2000 {
            let t = CLIP_SECS + i as f32 * 0.0137;
            let at = clip_time(t);
            assert!((LOOP_FROM..CLIP_SECS).contains(&at), "{t}: {at}");
        }
        // Past its end it carries on from its loop point, a loop's length on.
        assert!((clip_time(CLIP_SECS + 0.1) - (LOOP_FROM + 0.1)).abs() < 1e-5);
        assert!((clip_time(2.0 * CLIP_SECS - LOOP_FROM + 0.1) - (LOOP_FROM + 0.1)).abs() < 1e-4);
    }

    #[test]
    fn heals_health_then_shield() {
        let mut p = crate::new_player();
        let full = p.max_health();
        p.health = full - 50.0;
        p.shield = 0.0;
        heal(&mut p, 75.0);
        assert_eq!((p.health, p.shield), (full, 25.0));
    }
}
