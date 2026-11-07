//! His reaction to an enemy's melee blow, as Escalation's has it.
//!
//! A blow from an enemy's melee (its TnMeleeManager: the Scrappers' fist attack,
//! the soldiers' and the Shotgunners' melee, ai/melee.rs; TnDamageTypeMelee) makes
//! the one struck react (ServerProcessMeleeHit: ShouldPlayHitReaction, then his
//! melee service's PlayHitReaction). Escalation's characters' melee set
//! (TR_Shared_MS_p.SharedMeleeSet) has three reactions (TnMeleeSet.FindReaction,
//! by where the one who struck stands from where he faces): struck from the front
//! left (AngleCenter 315, HalfAngle 45), the front right (45, 45) or behind (180,
//! 90), mp_shared.xxx's HitReactions_ROBO_ANIM.ADD_Hit_Reaction_Melee_FL, _FR and
//! _B, each 1.03 s, on his upper body (CAMT_UpperBody), blending in over 0.1 s
//! (TnAnimationParams' BlendInTime) and out over 0.2 s (the reaction's
//! BlendOutTime), his legs going on as they were. The clips are additive (made
//! against Nav_Idle_Shoot): they throw his upper body and arms over whatever he is
//! doing.
//!
//! The set's reactions aren't AdditiveAnim (multiplayer's sets' are): his melee
//! manager goes into its Reacting state, which turns his shooting, his reload and
//! his ability off (DisableShooting, DisableReload, DisableAbility) until the clip
//! completes (TnCustomAnimationPlayer.CheckForCompletion: its BlendOutTime from its
//! end, 0.83 s in). Holding the trigger, his gun stops and fires again as he
//! recovers (TnPlayerController.CheckDeferredFire). Another blow starts it over.
//! His melee cuts it short (the set's MeleeCancelsHitReaction, its default:
//! StopAnimation(0.1)), and so does a dodge (TnAcrobaticsManager.Dodge: the
//! animator's StopAnimation, over the reaction's BlendOutTime).
//!
//! He doesn't react (TnPawn.get_CanPlayHitReaction, ShouldPlayHitReaction,
//! PlayHitReaction) down or reviving, transforming, giving a revive, dodging,
//! swinging his sword or punching (IsAttacking) or spinning his Whirlwind
//! (IsMeleeing(MELEE_Whirlwind)), nor with his Flak Shield up (every reaction's
//! RestrictedShieldState is SHIELD_Down); a vehicle has no melee service.
//! main.rs's damage_player starts it; animate_jazz plays the clips
//! (robot_extras.glb's, tools/build_robot_extras.py).

use bevy::prelude::*;

use crate::Player;

/// The damage type whose blows make him react: every enemy melee set's attacks'
/// (Scrapper_MS, Soldier_MS, Shotgunner_MS), and the reactions'
/// ValidForThisDamageType.
pub const DAMAGE_TYPE: &str = "TnDamageTypeMelee";

/// The reactions' roles in robot_extras.txt, in SharedMeleeSet's Reactions' order
/// (FindReaction takes the first that fits): struck from the front left, the front
/// right, behind.
pub const ROLES: [&str; 3] = ["melee_react_fl", "melee_react_fr", "melee_react_b"];

/// Each reaction's AngleCenter and HalfAngle (degrees, the way to the one who
/// struck from where he faces, clockwise seen from above: to his right positive).
const SECTORS: [(f32, f32); 3] = [(315.0, 45.0), (45.0, 45.0), (180.0, 90.0)];

/// The clips' SequenceLength (31 frames at 30 a second).
pub const CLIP_SECS: f32 = 31.0 / 30.0;
/// Blending in (TnAnimationParams' default BlendInTime) and out (the reactions'
/// BlendOutTime).
pub const BLEND_IN: f32 = 0.1;
pub const BLEND_OUT: f32 = 0.2;
/// His melee stopping it (TnMeleeManager.Reacting.StartMeleeAttack:
/// StopAnimation(0.1)).
pub const MELEE_STOP: f32 = 0.1;
/// A dodge stopping it (StopAnimation with no time: the reaction's BlendOutTime).
pub const DODGE_STOP: f32 = BLEND_OUT;

/// A reaction's clip playing: which (ROLES), seconds into it, and when it was
/// stopped (seconds into it) and over how long it blends out then.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Playing {
    pub clip: usize,
    pub t: f32,
    pub stop: Option<(f32, f32)>,
}

impl Playing {
    /// Its weight: blending in from its start, out over BLEND_OUT before its end
    /// (the slot's own blend out), or out from where it was stopped.
    pub fn weight(&self) -> f32 {
        let mut w =
            (self.t / BLEND_IN).min(1.0) * ((CLIP_SECS - self.t) / BLEND_OUT).clamp(0.0, 1.0);
        if let Some((at, blend)) = self.stop {
            w *= (1.0 - (self.t - at) / blend.max(1e-3)).clamp(0.0, 1.0);
        }
        w
    }

    fn over(&self) -> bool {
        self.t >= CLIP_SECS || self.stop.is_some_and(|(at, blend)| self.t - at >= blend)
    }
}

/// His reactions to melee blows: the one playing, and those still blending out
/// (one cut short by the next blow blends out as that one blends in, as a slot's
/// custom animations hand over).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeleeReact {
    pub playing: Vec<Playing>,
}

impl MeleeReact {
    /// In his melee manager's Reacting state: a reaction playing, not stopped, and
    /// short of its completion (its BlendOutTime from its end). Meanwhile he can't
    /// shoot, reload or use his ability.
    pub fn reacting(&self) -> bool {
        self.playing
            .last()
            .is_some_and(|p| p.stop.is_none() && p.t < CLIP_SECS - BLEND_OUT)
    }

    /// A blow: reaction `clip` from its start, the one playing handing over to it.
    pub fn start(&mut self, clip: usize) {
        self.stop(BLEND_IN);
        self.playing.push(Playing {
            clip: clip.min(ROLES.len() - 1),
            t: 0.0,
            stop: None,
        });
    }

    /// Cut short, blending out over `blend`.
    pub fn stop(&mut self, blend: f32) {
        for p in &mut self.playing {
            if p.stop.is_none() {
                p.stop = Some((p.t, blend));
            }
        }
    }

    /// Gone at once (he changed form, went down or died).
    pub fn clear(&mut self) {
        self.playing.clear();
    }

    /// `dt` on.
    pub fn step(&mut self, dt: f32) {
        for p in &mut self.playing {
            p.t += dt;
        }
        self.playing.retain(|p| !p.over());
    }

    /// Each clip's time and weight now (the latest of each clip's: one node a
    /// clip), for animate_jazz.
    pub fn clips(&self) -> [Option<(f32, f32)>; 3] {
        let mut out = [None; 3];
        for p in &self.playing {
            let w = p.weight();
            out[p.clip] = Some(match out[p.clip] {
                Some((_, before)) => (p.t, f32::max(before, w)),
                None => (p.t, w),
            });
        }
        out
    }
}

/// The reaction for a blow from whoever stands the way `toward` (along the
/// ground) from one facing `forward` (TnMeleeSet.FindReaction: the first of the
/// set's reactions whose sector holds the way to him).
pub fn reaction(forward: Vec3, toward: Vec3) -> usize {
    let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or(Vec3::NEG_Z);
    let right = Vec3::new(-forward.z, 0.0, forward.x);
    let toward = Vec3::new(toward.x, 0.0, toward.z);
    let yaw = toward.dot(right).atan2(toward.dot(forward)).to_degrees();
    SECTORS
        .iter()
        .position(|&(centre, half)| {
            let off = (yaw - centre + 180.0).rem_euclid(360.0) - 180.0;
            off.abs() <= half
        })
        .unwrap_or(ROLES.len() - 1)
}

/// The reactions' clocks, for every Jazz (his copies too: a blow to another
/// player's copy plays his reaction here); gone with his robot form or his life.
fn step_reactions(time: Res<Time>, mut players: Query<&mut Player>) {
    let dt = time.delta_secs();
    for mut player in &mut players {
        if player.melee_react.playing.is_empty() {
            continue;
        }
        let gone = player.form != crate::Form::Robot
            || player.transform_left > 0.0
            || player.pending_form.is_some()
            || player.dead_left > 0.0
            || player.dying
            || player.revive.downed.is_some();
        if gone {
            player.melee_react.clear();
        } else {
            player.melee_react.step(dt);
        }
    }
}

pub struct HitReactPlugin;

impl Plugin for HitReactPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, step_reactions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FindReaction over SharedMeleeSet's sectors: a blow from ahead and to his
    /// left is the front-left reaction, ahead and to his right the front-right
    /// one, from beside him or behind the one from behind; straight ahead is the
    /// front left's (the first that fits), and he faces where he faces (here east).
    #[test]
    fn the_reaction_is_the_side_struck_from() {
        let ahead = Vec3::NEG_Z;
        let right = Vec3::X;
        assert_eq!(reaction(ahead, ahead - right), 0);
        assert_eq!(reaction(ahead, ahead + right), 1);
        assert_eq!(reaction(ahead, ahead), 0);
        assert_eq!(reaction(ahead, -ahead), 2);
        assert_eq!(reaction(ahead, -ahead + right * 0.5), 2);
        assert_eq!(reaction(ahead, right * 2.0 - ahead * 0.1), 2);
        assert_eq!(reaction(ahead, -right + ahead * 0.1), 0);
        // Facing east (+X): north (-Z) is to his left.
        assert_eq!(reaction(Vec3::X, Vec3::X + Vec3::NEG_Z), 0);
        assert_eq!(reaction(Vec3::X, Vec3::X + Vec3::Z), 1);
    }

    /// The Reacting state: from the blow until 0.83 s in (its BlendOutTime from the
    /// clip's end), the clip blending in over 0.1 s and out over its last 0.2 s;
    /// another blow starts it over (the first blending out as the second blends
    /// in); his melee or a dodge ends it at once, the clip blending out.
    #[test]
    fn a_reaction_holds_his_gun_till_it_completes() {
        let mut r = MeleeReact::default();
        assert!(!r.reacting());
        r.start(1);
        assert!(r.reacting());
        r.step(0.05);
        let (t, w) = r.clips()[1].unwrap();
        assert!((t - 0.05).abs() < 1e-6 && (w - 0.5).abs() < 1e-4);
        r.step(0.75);
        assert!(r.reacting());
        r.step(0.04);
        assert!(!r.reacting(), "complete 0.83 s in");
        let w = r.clips()[1].unwrap().1;
        assert!(w > 0.9 && w < 1.0, "blending out: {w}");
        r.step(0.2);
        assert!(r.playing.is_empty());

        r.start(0);
        r.step(0.5);
        r.start(2);
        assert!(r.reacting());
        r.step(0.05);
        let c = r.clips();
        assert!((c[0].unwrap().1 - 0.5).abs() < 1e-4 && (c[2].unwrap().1 - 0.5).abs() < 1e-4);
        r.step(0.7);
        assert!(r.reacting(), "the second blow's reaction runs its own time");
        r.stop(MELEE_STOP);
        assert!(!r.reacting());
        r.step(0.05);
        assert!((r.clips()[2].unwrap().1 - 0.5).abs() < 1e-3);
        r.step(0.06);
        assert!(r.playing.is_empty());
    }
}
