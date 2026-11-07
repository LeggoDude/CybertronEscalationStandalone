//! The Leaper's own operators (TnLeapSmashOperator, TnMeleeLungeOperator and
//! TnAnimatedSkidOperator): its leap from a smash volume, high over its enemy
//! and down in a dive on the place it claimed (its smash hurting and throwing all
//! round where it lands), its lunge straight at him in flight (its sweep striking
//! the first it meets), and its skid to a stop after a lunge (a miss's sweeping
//! on as it slides). Each is the operator's state with its OnStart, OnUpdate,
//! OnHandleEvent and OnFinish, which ops.rs runs.

use std::sync::Arc;

use bevy::prelude::*;

use super::anim::{AnimParams, Callback, RMM_IGNORE};
use super::blueprint::{self, Asset};
use super::brain::{BEHAVIOR_AIMER, BEHAVIOR_HEAD_TRACKING, Brain, Ctx, Event, EventId};
use super::htn::{self, Status, Task};
use super::melee::TraceSweeper;
use super::pawn::{Acrobatics, GRAVITY_Z, Physics, stance};
use super::shared::PlayerDamage;
use super::ue;
use super::world::PlayerView;

/// STANCE_TwoHandedMeleeCharge: the stance it leaps in.
const LEAP_STANCE: u8 = 2;
/// How far past its landing it looks as it leaps (UpdateFocusLocation), and how
/// far ahead it looks as it lunges (kMeleeLungeLookAheadDistance).
const LEAP_LOOK_AHEAD: f32 = 1000.0;
const LUNGE_LOOK_AHEAD: f32 = 75000.0;
/// Its dive aims this far under its landing (UpdateSmashDescent).
const DIVE_UNDER: f32 = 500.0;

/// TnLeapSmashOperator: off the ground at its enemy with the jump that tops out
/// its LeapApexHeight over the higher end (its form's Jump), its transform into
/// the dive played as it nears the top, then (falling) flying down at its
/// blueprint's AirSpeed on the landing it retargeted to, till it strikes the
/// ground (falling the last bit, on a surface it meets: HitSurface); there its
/// smash hurts all round it (HurtRadius). It goes through pawns as it leaps,
/// takes its blueprint's DamageMultiplier the while, and comes down Tired (for
/// its recovery).
#[derive(Clone, Debug, Default)]
pub struct LeapSmash {
    pub bp: Option<Arc<blueprint::LeapSmash>>,
    pub transform: AnimParams,
    pub impact: Vec3,
    pub focus: Vec3,
    pub leaping: bool,
    pub played_transform: bool,
    pub old_accel_rate: f32,
    pub old_air_speed: f32,
    pub old_damage_multiplier: f32,
    pub hurt_enemy: bool,
}

impl LeapSmash {
    /// OnStart.
    pub fn start(&mut self, b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
        let bp = b.blueprint.clone();
        // (ResolveAnimationParameters, its own priority.)
        self.transform = bp.anim(task.int(0), stance::NONE);
        let Asset::LeapSmash(lb) = bp.asset(task.int(1)) else {
            return false;
        };
        let Some(sensor) = b.sensors.leap_smash.as_mut() else {
            return false;
        };
        self.impact = sensor.claim_impact_location(&mut b.core.rng);
        let apex = sensor.apex_height;
        self.update_focus(ctx.pawn.loc);
        let p = &mut *ctx.pawn;
        let jump = ue::jump_velocity(p.loc, self.impact, apex, GRAVITY_Z)
            .map_or(Vec3::ZERO, |(v, _, _)| v);
        p.form_jump(jump);
        self.leaping = true;
        self.old_accel_rate = p.accel_rate;
        self.old_air_speed = p.air_speed;
        b.set_behavior(ctx, 0, false);
        let p = &mut *ctx.pawn;
        p.stance.push(LEAP_STANCE);
        p.ignore_blockers = true;
        self.old_damage_multiplier = p.damage_multiplier;
        p.damage_multiplier *= lb.damage_multiplier;
        self.bp = Some(lb);
        true
    }

    /// UpdateFocusLocation: past the landing, the way it leaps.
    fn update_focus(&mut self, from: Vec3) {
        self.focus = self.impact + ue::normal2d(self.impact - from) * LEAP_LOOK_AHEAD;
    }

    /// OnUpdate.
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) -> Status {
        let Some(lb) = self.bp.clone() else {
            return Status::Failed;
        };
        self.focus.z = ctx.pawn.loc.z;
        ctx.pawn.set_focal_point(self.focus);
        ctx.pawn.set_look_at_point(self.focus);
        // PlayTransform: near the top of its jump (that many seconds from it), the
        // transform into its dive, looping, its root motion ignored.
        if !self.played_transform
            && ctx.pawn.acrobatics() == Acrobatics::Jumping
            && (ctx.pawn.vel.z / GRAVITY_Z).abs() < lb.start_transform_anim_time
        {
            let mut t = self.transform.clone();
            t.root_motion = RMM_IGNORE;
            t.looping = true;
            ctx.pawn.anim.enqueue(t, None, None);
            self.played_transform = true;
        }
        // StartSmashDescent: over the top, it flies at the landing its sensor
        // retargeted to, its dive's AccelRate and AirSpeed.
        if self.leaping && ctx.pawn.acrobatics() == Acrobatics::Falling {
            self.leaping = false;
            ctx.pawn.accel_rate = lb.accel_rate;
            ctx.pawn.air_speed = lb.air_speed;
            if let Some(s) = &b.sensors.leap_smash {
                self.impact = s.retargeted_impact;
            }
            ctx.pawn.acrobatics_fly();
            self.update_focus(ctx.pawn.loc);
        }
        // UpdateSmashDescent: at the landing and down (never up).
        if ctx.pawn.acrobatics() == Acrobatics::Flying {
            let mut a = ue::normal(self.impact - Vec3::Z * DIVE_UNDER - ctx.pawn.loc);
            a.z = -a.z.abs();
            ctx.pawn.accel = a;
        }
        if ctx.pawn.physics == Physics::Walking {
            self.hurt_radius(b, ctx, &lb);
            return Status::Succeeded;
        }
        Status::Running
    }

    /// OnHandleEvent: a surface struck (HitSurface) ends its flight: it falls.
    pub fn handle_event(&mut self, ctx: &mut Ctx, e: &Event) {
        if e.id != EventId::HitSurface {
            return;
        }
        self.restore_speed(ctx);
        ctx.pawn.acrobatics_fall();
        self.leaping = false;
    }

    /// RestoreSpeed.
    fn restore_speed(&self, ctx: &mut Ctx) {
        ctx.pawn.accel_rate = self.old_accel_rate;
        ctx.pawn.air_speed = self.old_air_speed;
    }

    /// HurtRadius (Actor's, full damage): each player whose body is within its
    /// DamageRadius of it, not more than its DamageHeightLimit over its middle
    /// (IsRadiusDamageWithinHeightLimit), and in its sight across the level, takes
    /// its SmashDamage and its DamageMomentum away from it (TnDamageTypeLeapSmash:
    /// a melee blow's, which pushes).
    fn hurt_radius(&mut self, b: &Brain, ctx: &mut Ctx, lb: &blueprint::LeapSmash) {
        let origin = ctx.pawn.loc;
        for pl in &ctx.view.players {
            if !pl.alive || pl.team == b.core.team {
                continue;
            }
            if !within_radius(pl, origin, lb.damage_radius) {
                continue;
            }
            if lb.damage_height_limit > 0.0
                && lb.damage_height_limit <= lb.damage_radius
                && pl.loc.z - origin.z >= lb.damage_height_limit
            {
                continue;
            }
            if let Some((hit, _)) = ctx.view.trace(origin, pl.loc)
                && (hit - origin).length() + pl.radius < (pl.loc - origin).length()
            {
                continue;
            }
            let dir = ue::normal(pl.loc - origin);
            let momentum = dir * lb.damage_momentum;
            if Some(pl.entity) == b.core.enemy {
                self.hurt_enemy = true;
            }
            ctx.shared.player_damage.push(PlayerDamage {
                player: pl.entity,
                by: ctx.pawn.entity,
                amount: lb.smash_damage,
                at: pl.loc - dir * (0.5 * (pl.radius + pl.half_height)),
                dir: momentum,
                melee: true,
                momentum: Some(momentum),
                jam: false,
                shot: None,
                damage: "TnDamageTypeLeapSmash",
            });
        }
    }

    /// OnFinish: its speeds, stance, collision and damage multiplier back, still;
    /// Tired (context 2: its smash recovery's), and more vulnerable for a miss.
    pub fn finish(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        self.restore_speed(ctx);
        let p = &mut *ctx.pawn;
        p.accel = Vec3::ZERO;
        p.vel = Vec3::ZERO;
        p.stance.pop(LEAP_STANCE);
        p.ignore_blockers = false;
        if self.bp.is_some() {
            p.damage_multiplier = self.old_damage_multiplier;
        }
        b.core.set_ws_bool(htn::WS_TIRED, true);
        b.core.set_ws_ctx(htn::WS_TIRED, 2);
        let missed = self.bp.as_ref().map_or(0, |lb| lb.missed_vulnerable_amount);
        if !self.hurt_enemy && missed > 0 {
            let v = b.core.ws(htn::WS_VULNERABILITY);
            b.core.set_ws(htn::WS_VULNERABILITY, v + missed);
        }
    }
}

/// CollidingActors' overlap: the player's cylinder within `radius` of `origin`.
fn within_radius(pl: &PlayerView, origin: Vec3, radius: f32) -> bool {
    let d = pl.loc - origin;
    let across = (ue::vsize2d(d) - pl.radius).max(0.0);
    let up = (d.z.abs() - pl.half_height).max(0.0);
    across * across + up * up <= radius * radius
}

/// TargetIsDodgingAway (the lunge's and the skid's): a player dodging across its
/// way (not along it, nor towards it) gets clear.
fn dodging_away(p_loc: Vec3, facing: Vec3, pl: &PlayerView) -> bool {
    let Some(dodge) = pl.dodge else {
        return false;
    };
    if facing == Vec3::ZERO || dodge == Vec3::ZERO {
        return false;
    }
    if facing.dot(dodge).abs() > 0.707 {
        return false;
    }
    dodge.dot(pl.loc - p_loc) >= 0.0
}

/// A blow of the Leaper's (TnDamageTypeLeapSmash: a melee blow's, which pushes)
/// on each player of the other side its sweep met, unless he dodges clear: its
/// damage, and its momentum the way it faces. Whether it struck anyone.
fn strike(b: &Brain, ctx: &mut Ctx, met: Vec<(Entity, Vec3)>, damage: f32, momentum: f32) -> bool {
    let mut struck = false;
    let facing = ctx.pawn.forward();
    for (e, at) in met {
        let Some(pl) = ctx.view.player(e) else {
            continue;
        };
        if pl.team == b.core.team || dodging_away(ctx.pawn.loc, facing, pl) {
            continue;
        }
        let m = facing * momentum;
        ctx.shared.player_damage.push(PlayerDamage {
            player: e,
            by: ctx.pawn.entity,
            amount: damage,
            at,
            dir: m,
            melee: true,
            momentum: Some(m),
            jam: false,
            shot: None,
            damage: "TnDamageTypeLeapSmash",
        });
        struck = true;
    }
    struck
}

/// TnMeleeLungeOperator: it flies the way it faces (its blueprint's AirSpeed and
/// AccelRate), its lunge animation looping, its damage sweep on its socket
/// striking the first foe it meets, till it has struck one, run into a wall or
/// its time is up; then it falls, Tired (context 0 for a hit, 1 for a miss: which
/// skid it recovers with).
#[derive(Clone, Debug, Default)]
pub struct MeleeLunge {
    pub bp: Option<Arc<blueprint::MeleeLunge>>,
    pub time: f32,
    pub old_accel_rate: f32,
    pub old_air_speed: f32,
    pub hit_wall: bool,
    pub hit_pawn: bool,
    pub sweeper: TraceSweeper,
}

impl MeleeLunge {
    /// OnStart.
    pub fn start(&mut self, b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
        let bp = b.blueprint.clone();
        let Asset::MeleeLunge(lb) = bp.asset(task.int(1)) else {
            return false;
        };
        let p = &mut *ctx.pawn;
        self.old_accel_rate = p.accel_rate;
        self.old_air_speed = p.air_speed;
        p.air_speed = lb.air_speed;
        p.accel_rate = lb.accel_rate;
        b.set_behavior(ctx, BEHAVIOR_HEAD_TRACKING | BEHAVIOR_AIMER, false);
        let p = &mut *ctx.pawn;
        p.acrobatics_fly();
        let look = p.forward();
        p.accel = look;
        let ahead = p.loc + look * LUNGE_LOOK_AHEAD;
        p.set_focal_point(ahead);
        p.set_look_at_point(ahead);
        // StartDamageSweep: for as long as it lasts.
        self.sweeper
            .begin(p, lb.socket.clone(), lb.extent, 1_000_000.0);
        let mut a = bp.anim(task.int(0), stance::NONE);
        a.priority = 1;
        a.root_motion = RMM_IGNORE;
        a.looping = true;
        p.anim.enqueue(a, None, None);
        self.bp = Some(lb);
        true
    }

    /// OnUpdate.
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) -> Status {
        let Some(lb) = self.bp.clone() else {
            return Status::Failed;
        };
        self.time += ctx.dt;
        let met = self.sweeper.update(ctx.pawn, ctx.view, ctx.dt);
        if strike(b, ctx, met, lb.damage, lb.damage_momentum) {
            self.hit_pawn = true;
        }
        if self.hit_pawn || self.hit_wall || self.time >= lb.duration {
            Status::Succeeded
        } else {
            Status::Running
        }
    }

    /// OnHandleEvent: it ran into a wall (HitWall).
    pub fn handle_event(&mut self, e: &Event) {
        if e.id == EventId::HitWall {
            self.hit_wall = true;
        }
    }

    /// OnFinish.
    pub fn finish(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let p = &mut *ctx.pawn;
        p.acrobatics_fall();
        p.anim.stop(0.0, None);
        self.sweeper.end();
        p.accel_rate = self.old_accel_rate;
        p.air_speed = self.old_air_speed;
        b.core.set_ws_bool(htn::WS_TIRED, true);
        b.core
            .set_ws_ctx(htn::WS_TIRED, if self.hit_pawn { 0 } else { 1 });
    }
}

/// TnAnimatedSkidOperator: its recovery's animation as it slides to a stop (its
/// ground speed eased from what it came in with to none over SkidDuration, along
/// the way it was going; a wall stops it; no landing animation over it), its
/// sweep striking for less the slower it goes; done once its animation has ended
/// and its skid's time is up.
#[derive(Clone, Debug, Default)]
pub struct Skid {
    pub bp: Option<Arc<blueprint::Skid>>,
    pub anim: AnimParams,
    pub starting_speed: f32,
    pub old_base_ground_speed: f32,
    pub time: f32,
    pub anim_complete: bool,
    pub lunge_hit_pawn: bool,
    pub sweeper: TraceSweeper,
}

impl Skid {
    /// OnStart.
    pub fn start(
        &mut self,
        b: &mut Brain,
        ctx: &mut Ctx,
        task: &Task,
        complete: Option<Callback>,
    ) -> bool {
        let bp = b.blueprint.clone();
        let p = &mut *ctx.pawn;
        self.old_base_ground_speed = p.ground_speed;
        self.starting_speed = p.vel.length();
        if self.starting_speed > 0.0 {
            p.accel = p.vel / self.starting_speed;
        }
        p.ground_speed = self.starting_speed;
        let mut a = bp.anim(task.int(0), stance::NONE);
        a.priority = 1;
        a.root_motion = RMM_IGNORE;
        let Asset::Skid(sb) = bp.asset(task.int(1)) else {
            return false;
        };
        p.anim.enqueue(a.clone(), complete, complete);
        self.anim = a;
        p.landing_animation_enabled = false;
        // StartDamageSweep.
        self.sweeper
            .begin(p, sb.socket.clone(), sb.extent, sb.sweep_duration);
        self.bp = Some(sb);
        true
    }

    /// OnUpdate.
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) -> Status {
        let Some(sb) = self.bp.clone() else {
            return Status::Failed;
        };
        self.time += ctx.dt;
        let met = self.sweeper.update(ctx.pawn, ctx.view, ctx.dt);
        // CalculateDamage: the more of its speed it has left.
        let scale = if self.starting_speed > 0.0 {
            ctx.pawn.vel.length() / self.starting_speed
        } else {
            0.0
        };
        if strike(b, ctx, met, scale * sb.damage, sb.damage_momentum) {
            self.lunge_hit_pawn = true;
        }
        if self.anim_complete && self.time >= sb.skid_duration {
            return Status::Succeeded;
        }
        // (Lerp: past its skid's time, on below none.)
        ctx.pawn.ground_speed = ue::lerp(
            self.starting_speed,
            0.0,
            self.time / sb.skid_duration.max(1e-3),
        );
        Status::Running
    }

    /// OnHandleEvent: a wall run into stops it pushing on (HitWall).
    pub fn handle_event(&mut self, ctx: &mut Ctx, e: &Event) {
        if e.id == EventId::HitWall {
            ctx.pawn.accel = Vec3::ZERO;
        }
    }

    /// OnFinish: its animation stopped if it didn't run its course, its ground
    /// speed back.
    pub fn finish(&mut self, ctx: &mut Ctx, status: Status) {
        if status != Status::Succeeded {
            ctx.pawn.anim.stop(0.0, Some(&self.anim.name));
        }
        ctx.pawn.landing_animation_enabled = true;
        ctx.pawn.ground_speed = self.old_base_ground_speed;
    }
}
