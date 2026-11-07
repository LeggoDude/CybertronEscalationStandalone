//! An enemy's brain: TnHtnAiController (its world state, behaviour flags,
//! blackboard, events and tick) with its HtnPlanRunner (running a plan's operators
//! in turn) and HtnSensorCollection (its sensors by update frequency).
//!
//! The controller's tick (TnHtnAiController.Tick), each frame:
//! 1. the active task's symbol into WS_CurrentTask (its context: interruptible or
//!    not) and the acrobatics state into WS_Acrobatics;
//! 2. the sensors' update, each group at its frequency (every frame, 0.1 s, 0.5 s,
//!    1 s);
//! 3. a new plan looked for if the world state changed (an attribute the domain
//!    uses) or there is no plan running: the sensors that update prior to planning,
//!    the plan validated (a task's preconditions no longer holding clears it), the
//!    override goal planned for if there is one and the AI is active, else the
//!    goal; the new plan runs if it is better (by priority) and the active operator
//!    can be interrupted;
//! 4. the plan runner's update (the active operator's);
//! 5. the override goal finished, if its plan succeeded;
//! 6. the weapon controller's and the motion controller's updates.

use std::sync::Arc;

use bevy::prelude::*;

use super::anim::Callback;
use super::blueprint::Blueprint;
use super::data::Behaviour;
use super::htn::{self, Domain, Status, Task, WS_NUM, WorldState, Ws};
use super::ops::{self, Op};
use super::pawn::Pawn;
use super::sensors::{self, Sensors};
use super::shared::Shared;
use super::ue::Rng;
use super::weapon::{self, Shooter};
use super::world::WorldView;

// AI.ELocationReference.
pub const LR_NONE: usize = 0;
pub const LR_CONFIRMED_ENEMY: usize = 1;
pub const LR_SUSPECTED_ENEMY: usize = 2;
pub const LR_ATTACK_POINT: usize = 3;
pub const LR_DEFEND_POINT: usize = 4;
pub const LR_PATROL_POINT: usize = 5;
pub const LR_SEARCH_POINT: usize = 6;
pub const LR_DRIVING_CIRCLING_POINT: usize = 7;
pub const LR_KISMET_MOVE_TO_ACTOR: usize = 8;
pub const LR_KISMET_LOOK_AT_ACTOR: usize = 9;
pub const LR_FALLBACK_LOCATION: usize = 10;
pub const LR_NAVIGABLE_LOCATION_NEAR_ENEMY: usize = 11;
pub const LR_NAVIGABLE_LOCATION_NEAR_DEFEND_ACTOR: usize = 12;
pub const LR_ATTACK_FOCUS: usize = 13;
pub const LR_GRENADE_AIM_POINT: usize = 14;
pub const LR_GRENADE_LOCATION: usize = 15;
pub const LR_GRAV_BOMB_LOCATION: usize = 16;
pub const LR_TRAIL_ENEMY_POINT: usize = 17;
pub const LR_NUM: usize = 18;

// AI behaviour flags (AI.1 ... AI.64).
pub const BEHAVIOR_HEAD_TRACKING: u32 = 1;
pub const BEHAVIOR_AIMER: u32 = 2;
pub const BEHAVIOR_ROTATE_TO_FOCUS: u32 = 4;
pub const BEHAVIOR_SHOOTER: u32 = 8;
pub const BEHAVIOR_WEAPON_TRACKING: u32 = 16;
pub const BEHAVIOR_ALLOW_MOTION: u32 = 32;
pub const BEHAVIOR_LEDGE_FALL: u32 = 64;

/// AI.EEvent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventId {
    HeardEnemy = 0,
    SawEnemy = 1,
    BuddySawEnemy = 2,
    LostSightOfEnemy = 3,
    TookDamage = 4,
    Alarm = 5,
    HitSurface = 6,
    Bump = 7,
    HitWall = 8,
    AreaClear = 9,
    Died = 10,
    AnimNotify = 11,
    Cloak = 12,
    Uncloak = 13,
    Gravity = 14,
    GravityEnd = 15,
    GrappleAttached = 16,
    GrappleDetached = 17,
}

/// The damage an event carries (TnTakeDamageInfo).
#[derive(Clone, Debug, Default)]
pub struct DamageInfo {
    /// Who did it (a player's pawn), if anyone.
    pub instigator: Option<Entity>,
    pub hit_location: Vec3,
    pub damage: f32,
    pub kind: crate::enemies::Death,
    /// The damage type's own (TnDamageType): its class name's tag.
    pub damage_type: DamageKind,
    pub momentum: Vec3,
    /// The bone it struck (HitInfo.BoneName), if known.
    pub bone: Option<Arc<str>>,
}

/// A TnAnimNotify_HtnEvent an animation reached (AnimEvent: 0 a grenade's
/// release, 2 a melee's end of tracking...), with its socket and the mirrored
/// one.
#[derive(Clone, Debug, Default)]
pub struct Notify {
    pub event: u8,
    pub socket: Arc<str>,
    pub mirrored_socket: Arc<str>,
}

/// The damage types the AI's script tells apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DamageKind {
    #[default]
    Other,
    Melee,
    Stun,
    LeapSmash,
    VehicleExplosive,
    PlaneRocket,
    Rammed,
    Explosive,
    Fire,
}

/// An event and what it is about.
#[derive(Clone, Debug)]
pub struct Event {
    pub id: EventId,
    /// The actor it is about (an enemy seen, a noise's maker...).
    pub actor: Option<Entity>,
    pub damage: Option<DamageInfo>,
    /// A noise's name (HearNoise's NoiseType: 'Death'...).
    pub noise: Option<&'static str>,
    pub notify: Option<Notify>,
}

impl Event {
    pub fn about(id: EventId, actor: Option<Entity>) -> Event {
        Event {
            id,
            actor,
            damage: None,
            noise: None,
            notify: None,
        }
    }
}

/// The blackboard (TnAiBlackboard): a location (and its ground location) for each
/// location reference.
#[derive(Clone, Debug)]
pub struct Blackboard {
    pub locations: [Vec3; LR_NUM],
    pub ground: [Vec3; LR_NUM],
}

impl Default for Blackboard {
    fn default() -> Self {
        Blackboard {
            locations: [Vec3::ZERO; LR_NUM],
            ground: [Vec3::ZERO; LR_NUM],
        }
    }
}

/// The plan runner (HtnPlanRunner).
#[derive(Default)]
pub struct Runner {
    pub plan: Vec<Task>,
    pub index: usize,
    pub goal: Task,
    pub priority: i32,
    pub benefit: i32,
    pub pending: Vec<Task>,
    pub pending_goal: Task,
    pub pending_priority: i32,
    pub status: Status,
    /// The operator running, and those of the plan started before it (PlanFinish
    /// goes to every one).
    pub active: Option<Op>,
    pub finished: Vec<Op>,
    pub plans_run: u32,
    pub plans_requested: u32,
    /// The world state attribute whose precondition last invalidated a plan.
    pub invalid_precondition: usize,
    /// A recent history of plans, for the debug view: (goal, plan, priority).
    pub history: Vec<(i32, Vec<i32>, i32, Status)>,
}

impl Runner {
    pub fn running(&self) -> bool {
        !self.plan.is_empty()
    }

    pub fn active_symbol(&self) -> i32 {
        if self.index < self.plan.len() {
            self.plan[self.index].symbol
        } else {
            -1
        }
    }

    /// HtnPlanRunner.get_Interruptible.
    pub fn interruptible(&self) -> bool {
        self.active.as_ref().is_none_or(|o| o.is_interruptible())
    }

    pub fn succeeded(&self, goal_symbol: i32) -> bool {
        self.status == Status::Succeeded && self.goal.symbol == goal_symbol
    }
}

/// The controller's own state (TnHtnAiController and what it owns but the sensors
/// and the plan runner).
pub struct Core {
    pub entity: Entity,
    /// The enemy type (enemies.rs's Kinds) and its AI.EAiCharacterType.
    pub kind: usize,
    pub ai_type: i32,
    /// The serial number the next operator gets.
    pub next_serial: u32,
    pub b: Arc<Behaviour>,
    pub d: Arc<Domain>,
    pub ws: WorldState,
    pub ws_changed: bool,
    pub goal: Task,
    pub override_goal: Task,
    pub override_completes_on_success: bool,
    pub scripted: bool,
    pub waiting_for_plan_abort: bool,
    pub behavior: u32,
    pub blackboard: Blackboard,
    /// Controller.Enemy: the player it is after.
    pub enemy: Option<Entity>,
    pub competency: f32,
    pub randomness: f32,
    pub combat_alert_level: i32,
    pub grenade_danger_range_sq: f32,
    pub brain_dead_frames: u32,
    pub team: u8,
    pub sub_squad: i32,
    pub rng: Rng,
    /// Events that came in since its last tick.
    pub inbox: Vec<Event>,
    /// The Kismet-style override's completion, for the alerter (none here).
    pub override_done: bool,
}

impl Core {
    // -----------------------------------------------------------------------
    // The world state
    // -----------------------------------------------------------------------

    pub fn ws(&self, attr: usize) -> i32 {
        self.ws[attr].value
    }

    pub fn ws_bool(&self, attr: usize) -> bool {
        self.ws[attr].value != 0
    }

    pub fn ws_ctx(&self, attr: usize) -> i32 {
        self.ws[attr].context
    }

    pub fn ws_bit(&self, attr: usize, bit: i32) -> bool {
        htn::Cmp::BitIsSet.test(self.ws[attr].value, bit)
    }

    /// TnHtnAiController.OnWorldStateChanged.
    fn changed(&mut self, attr: usize) {
        if self.d.is_used(attr) {
            self.ws_changed = true;
        }
    }

    /// SetWorldStateIntValue.
    pub fn set_ws(&mut self, attr: usize, value: i32) {
        if self.ws[attr].value == value {
            return;
        }
        self.ws[attr].value = value;
        self.changed(attr);
    }

    pub fn set_ws_bool(&mut self, attr: usize, value: bool) {
        self.set_ws(attr, value as i32);
    }

    /// SetWorldStateContext.
    pub fn set_ws_ctx(&mut self, attr: usize, ctx: i32) {
        if self.ws[attr].context == ctx {
            return;
        }
        self.ws[attr].context = ctx;
        self.changed(attr);
    }

    /// SetWorldStateBitValue.
    pub fn set_ws_bit(&mut self, attr: usize, bit: i32, enable: bool) {
        if self.ws_bit(attr, bit) == enable {
            return;
        }
        self.ws[attr].value = htn::set_bit(self.ws[attr].value, bit, enable);
        self.changed(attr);
    }

    // -----------------------------------------------------------------------
    // The blackboard
    // -----------------------------------------------------------------------

    /// StoreLocation (a zero ground location stores the location as its own).
    pub fn store(&mut self, r: usize, loc: Vec3, ground: Vec3) {
        if r >= LR_NUM {
            return;
        }
        self.blackboard.locations[r] = loc;
        self.blackboard.ground[r] = if ground == Vec3::ZERO { loc } else { ground };
    }

    pub fn fetch(&self, r: usize) -> Vec3 {
        self.blackboard
            .locations
            .get(r)
            .copied()
            .unwrap_or_default()
    }

    pub fn fetch_ground(&self, r: usize) -> Vec3 {
        self.blackboard.ground.get(r).copied().unwrap_or_default()
    }

    /// IsActive: not asleep.
    pub fn is_active(&self) -> bool {
        self.ws(htn::WS_ASLEEP) == 0
    }

    /// IsInCombat.
    pub fn in_combat(&self) -> bool {
        self.ws(htn::WS_ALERT_LEVEL) >= self.combat_alert_level
    }

    /// IsAwareOfThreats.
    pub fn aware_of_threats(&self) -> bool {
        self.ws_bool(htn::WS_ALERT_TO_ENEMY_PRESENCE)
    }
}

/// An enemy's brain.
pub struct Brain {
    pub core: Core,
    pub sensors: Sensors,
    pub runner: Runner,
    /// Its weapon controller (TnAiWeaponController).
    pub shooter: Shooter,
    /// Its blueprint's resolved references.
    pub blueprint: Arc<Blueprint>,
}

/// What a brain's tick works with besides the brain: its pawn, the frame's view of
/// the world and what the AI shares.
pub struct Ctx<'a, 'w> {
    pub dt: f32,
    pub pawn: &'a mut Pawn,
    pub view: &'a WorldView<'w>,
    pub shared: &'a mut Shared,
}

impl Brain {
    /// A brain for a type's behaviour (TnHtnAiController.Initialize: its world state
    /// as it starts, its sensors, its planner).
    pub fn new(
        entity: Entity,
        kind: usize,
        b: Arc<Behaviour>,
        blueprint: Arc<Blueprint>,
        seed: u64,
    ) -> Option<Brain> {
        let d = b.domain.clone()?;
        let mut ws: WorldState = [Ws::default(); WS_NUM];
        // InitializeWorldState.
        ws[htn::WS_AREA_CLEAR].value = 1;
        ws[htn::WS_BALANCED].value = 1;
        let goal = Task::new(b.goal, &[]);
        let ai_type = sensors::ai_type_number(b.get("_AiCharacterType").unwrap_or(""));
        let core = Core {
            entity,
            kind,
            ai_type,
            next_serial: 1,
            d,
            ws,
            ws_changed: true,
            goal,
            override_goal: Task::new(-1, &[]),
            override_completes_on_success: true,
            scripted: false,
            waiting_for_plan_abort: false,
            behavior: 0,
            blackboard: Blackboard::default(),
            enemy: None,
            // (TnHtnAiController's defaults: its blueprint may change them.)
            competency: 1.0,
            randomness: 0.0,
            combat_alert_level: 1,
            // (TnHtnAiController's: 20 m.)
            grenade_danger_range_sq: 4_000_000.0,
            brain_dead_frames: 0,
            team: super::world::AI_TEAM,
            sub_squad: 0,
            rng: Rng::new(seed),
            inbox: Vec::new(),
            override_done: false,
            b,
        };
        Some(Brain {
            core,
            sensors: Sensors::new(),
            runner: Runner {
                priority: htn::INVALID_PLAN,
                pending_priority: htn::INVALID_PLAN,
                goal: Task::new(-1, &[]),
                pending_goal: Task::new(-1, &[]),
                ..Default::default()
            },
            shooter: Shooter::default(),
            blueprint,
        })
    }

    // -----------------------------------------------------------------------
    // Behaviours
    // -----------------------------------------------------------------------

    /// SetBehavior: the behaviour flags, starting and stopping what they turn on.
    pub fn set_behavior(&mut self, ctx: &mut Ctx, flags: u32, force_all: bool) {
        let diff = if force_all {
            flags
        } else {
            self.core.behavior ^ flags
        };
        let rotate = flags & BEHAVIOR_ROTATE_TO_FOCUS != 0;
        ctx.pawn.rotate_toward_focus = rotate;
        if rotate {
            ctx.pawn.motion.facing = super::motion::Facing::Focus;
        }
        if diff == 0 {
            return;
        }
        if diff & BEHAVIOR_HEAD_TRACKING != 0 {
            ctx.pawn.ik_head = flags & BEHAVIOR_HEAD_TRACKING != 0;
        }
        if diff & BEHAVIOR_WEAPON_TRACKING != 0 {
            ctx.pawn.ik_weapon = flags & BEHAVIOR_WEAPON_TRACKING != 0;
        }
        if diff & BEHAVIOR_AIMER != 0 {
            if flags & BEHAVIOR_AIMER != 0 {
                ctx.pawn.aimer.activate_with(&mut self.core.rng);
                ctx.pawn.ik_arm = true;
            } else {
                ctx.pawn.aimer.deactivate();
                ctx.pawn.ik_arm = false;
            }
        }
        if diff & BEHAVIOR_SHOOTER != 0 {
            if flags & BEHAVIOR_SHOOTER != 0 {
                weapon::activate(self);
            } else {
                weapon::deactivate(self, ctx.pawn);
            }
        }
        if diff & BEHAVIOR_ALLOW_MOTION != 0 && flags & BEHAVIOR_ALLOW_MOTION == 0 {
            ctx.pawn.motion_stop();
            sensors::abort_move(self, ctx);
        }
        if diff & BEHAVIOR_LEDGE_FALL != 0 {
            ctx.pawn.walk_off_ledge = flags & BEHAVIOR_LEDGE_FALL != BEHAVIOR_LEDGE_FALL;
        }
        self.core.behavior = flags;
    }

    pub fn add_behavior(&mut self, ctx: &mut Ctx, flags: u32) {
        let f = self.core.behavior | flags;
        self.set_behavior(ctx, f, false);
    }

    pub fn remove_behavior(&mut self, ctx: &mut Ctx, flags: u32) {
        let f = self.core.behavior & !flags;
        self.set_behavior(ctx, f, false);
    }

    // -----------------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------------

    /// HandleEvent: the sensors' and the active operator's.
    pub fn handle_event(&mut self, ctx: &mut Ctx, e: Event) {
        sensors::notify_event(self, ctx, &e);
        if let Some(mut op) = self.runner.active.take() {
            ops::handle_event(&mut op, self, ctx, &e);
            if !self.runner.running() || self.runner.active.is_some() {
                ops::finish(&mut op, self, ctx, Status::Aborted);
                ops::plan_finish(&mut op, self, ctx, Status::Aborted);
            } else {
                self.runner.active = Some(op);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Planning and running plans
    // -----------------------------------------------------------------------

    /// TnHtnAiController.FindNewPlan.
    pub fn find_new_plan(&mut self, ctx: &mut Ctx) {
        sensors::update_prior_to_planning(self, ctx);
        self.validate_plan(ctx);
        let goal = if self.core.override_goal.symbol != -1 && self.core.is_active() {
            self.core.override_goal
        } else {
            self.core.goal
        };
        // HtnPlanRunner.FindNewPlan.
        let built = htn::build_plan(&self.core.d, goal, &self.core.ws);
        self.runner.pending_goal = goal;
        self.runner.pending = built.plan;
        self.runner.pending_priority = built.priority;
        if !self.pending_is_better() {
            self.runner.pending.clear();
            self.core.d.reset_world_states(&mut self.core.ws);
        }
        self.runner.plans_requested += 1;
    }

    /// get_PendingPlanIsBetterThanCurrentPlan.
    fn pending_is_better(&self) -> bool {
        let r = &self.runner;
        if r.plan.is_empty() || r.pending_goal.symbol != r.goal.symbol {
            return true;
        }
        if self.core.d.debug_flags & 1 != 0 {
            r.pending_priority < r.priority
        } else {
            self.core.d.benefit(&r.pending) > r.benefit
        }
    }

    /// HtnPlanRunner.ValidatePlan.
    pub fn validate_plan(&mut self, ctx: &mut Ctx) -> bool {
        if let Some((symbol, attr)) = self.find_invalid_task() {
            self.runner.invalid_precondition = attr;
            self.clear_plan(ctx, Status::Invalid, symbol);
            return false;
        }
        true
    }

    /// FindInvalidTask (none while the active operator can't be interrupted).
    fn find_invalid_task(&self) -> Option<(i32, usize)> {
        if !self.runner.interruptible() || !self.runner.running() {
            return None;
        }
        htn::find_invalid_task(
            &self.core.d,
            &self.runner.plan,
            self.runner.index,
            &self.core.ws,
        )
    }

    /// HtnPlanRunner.Update.
    pub fn runner_update(&mut self, ctx: &mut Ctx) {
        if !self.runner.pending.is_empty() && self.runner.interruptible() {
            self.run_pending_plan(ctx);
        }
        if !self.runner.running() {
            return;
        }
        let Some(mut op) = self.runner.active.take() else {
            return;
        };
        let status = ops::update(&mut op, self, ctx);
        if !self.runner.running() || self.runner.active.is_some() {
            // (The plan was cleared, or another started, while it ran.)
            ops::finish(&mut op, self, ctx, Status::Aborted);
            ops::plan_finish(&mut op, self, ctx, Status::Aborted);
            return;
        }
        self.runner.active = Some(op);
        match status {
            Status::Running => {}
            Status::Failed => {
                let s = self.runner.active_symbol();
                self.clear_plan(ctx, Status::Failed, s);
            }
            Status::Succeeded => {
                self.finish_active_operator(ctx);
            }
            other => {
                let s = self.runner.active_symbol();
                self.clear_plan(ctx, other, s);
            }
        }
    }

    fn run_pending_plan(&mut self, ctx: &mut Ctx) {
        let s = self.runner.active_symbol();
        self.clear_plan(ctx, Status::Obsolete, s);
        let r = &mut self.runner;
        r.goal = r.pending_goal;
        r.plan = std::mem::take(&mut r.pending);
        r.priority = r.pending_priority;
        r.status = Status::Running;
        r.index = 0;
        r.benefit = self.core.d.benefit(&r.plan);
        r.plans_run += 1;
        self.start_next_operator(ctx);
    }

    /// StartNextOperator.
    fn start_next_operator(&mut self, ctx: &mut Ctx) -> bool {
        if self.runner.index >= self.runner.plan.len() {
            return false;
        }
        let task = self.runner.plan[self.runner.index];
        let class = self
            .core
            .d
            .op(task.symbol)
            .map(|o| o.class.clone())
            .unwrap_or_default();
        let flags = self.core.d.op(task.symbol).map_or(0, |o| o.flags);
        let serial = self.core.next_serial;
        self.core.next_serial = self.core.next_serial.wrapping_add(1).max(1);
        let mut op = Op::new(&class, task, serial);
        // (The rest of the plan checked against the world as it is now, unless the
        // new operator can't be interrupted: the script makes it the active operator
        // first, and FindInvalidTask finds nothing invalid while the active operator
        // runs to its end. A Sniper that saw its enemy as its search flight ended
        // went on to take its insect form on the wall, as in the game, and fought
        // from there; checked before AttachToWall, its plan was dropped in the air
        // in its robot form, where its domain has nothing for it to do, and it hung
        // there for good.)
        if op.is_interruptible()
            && let Some((symbol, attr)) = htn::find_invalid_task(
                &self.core.d,
                &self.runner.plan,
                self.runner.index,
                &self.core.ws,
            )
        {
            self.runner.invalid_precondition = attr;
            self.clear_plan(ctx, Status::Invalid, symbol);
            return false;
        }
        // HtnOperator.Start: OnStart, then its behaviour flags (OnStartBehavior: a
        // TnBehaviorOperator sets its own and the task's, any other adds the
        // task's).
        let started = ops::start(&mut op, self, ctx);
        if !started {
            self.runner.finished.push(op);
            self.clear_plan(ctx, Status::Failed, task.symbol);
            return false;
        }
        match ops::behaviour_class_flags(&op, self, ctx) {
            Some(own) => self.set_behavior(ctx, own | flags, false),
            None => {
                if flags != 0 {
                    self.add_behavior(ctx, flags);
                }
            }
        }
        self.runner.active = Some(op);
        true
    }

    /// FinishActiveOperator.
    fn finish_active_operator(&mut self, ctx: &mut Ctx) -> bool {
        let last = self.runner.active_symbol();
        self.clear_active_operator(ctx, Status::Succeeded);
        if self.runner.plan.is_empty() {
            return false;
        }
        let task = self.runner.plan[self.runner.index];
        htn::apply_effects(&self.core.d, &task, &mut self.core.ws, false);
        // (The effects' attributes count as changes, as the script's direct writes
        // to the world state don't: no replan of their own.)
        self.runner.index += 1;
        if self.runner.index == self.runner.plan.len() {
            self.clear_plan(ctx, Status::Succeeded, last);
            return false;
        }
        self.start_next_operator(ctx)
    }

    /// ClearActiveOperator.
    fn clear_active_operator(&mut self, ctx: &mut Ctx, status: Status) {
        if let Some(mut op) = self.runner.active.take() {
            ops::finish(&mut op, self, ctx, status);
            self.runner.finished.push(op);
        }
    }

    /// HtnPlanRunner.ClearPlan.
    pub fn clear_plan(&mut self, ctx: &mut Ctx, status: Status, symbol: i32) {
        if self.runner.plan.is_empty() {
            return;
        }
        self.clear_active_operator(ctx, status);
        let goal = self.runner.goal.symbol;
        let plan: Vec<i32> = self.runner.plan.iter().map(|t| t.symbol).collect();
        let priority = self.runner.priority;
        self.runner.history.push((goal, plan, priority, status));
        if self.runner.history.len() > 6 {
            self.runner.history.remove(0);
        }
        let _ = symbol;
        let mut ops = std::mem::take(&mut self.runner.finished);
        for op in ops.iter_mut() {
            ops::plan_finish(op, self, ctx, status);
        }
        self.runner.status = status;
        self.runner.plan.clear();
        self.runner.priority = htn::INVALID_PLAN;
        self.runner.benefit = 0;
    }

    /// AbortPlan (TnHtnAiController's: the runner's, and the override goal
    /// cleared; or once the active operator can be interrupted).
    pub fn abort_plan(&mut self, ctx: &mut Ctx, wait_until_interruptible: bool) {
        if wait_until_interruptible {
            self.core.waiting_for_plan_abort = true;
            return;
        }
        if self.runner.active.is_some() {
            let s = self.runner.active_symbol();
            self.clear_plan(ctx, Status::Aborted, s);
        }
        self.clear_override_goal();
    }

    /// RequestOverridePlan.
    pub fn request_override_plan(&mut self, ctx: &mut Ctx, goal: Task, completes_on_success: bool) {
        self.core.override_goal = goal;
        self.core.scripted = true;
        self.core.override_completes_on_success = completes_on_success;
        self.find_new_plan(ctx);
    }

    /// ClearOverrideGoal.
    pub fn clear_override_goal(&mut self) {
        self.core.override_goal.symbol = -1;
        self.core.scripted = false;
        self.core.override_done = true;
    }

    /// HasOverrideGoalCompleted.
    fn override_goal_completed(&self) -> bool {
        if !self.core.override_completes_on_success {
            return false;
        }
        !self.runner.running() && self.runner.succeeded(self.core.override_goal.symbol)
    }

    // -----------------------------------------------------------------------
    // The tick
    // -----------------------------------------------------------------------

    /// The animations' callbacks of the last update, to their operators (the
    /// active one's: the others are done with theirs) and the pawn's own.
    pub fn deliver_callbacks(&mut self, ctx: &mut Ctx) {
        let fired: Vec<Callback> = std::mem::take(&mut ctx.pawn.anim.fired);
        for cb in fired {
            // The melee manager's, for the attack it names.
            if cb.owner & super::melee::MELEE_CALLBACK != 0 {
                super::melee::animation_complete(
                    ctx.pawn,
                    cb.owner & !super::melee::MELEE_CALLBACK,
                );
                continue;
            }
            if cb.owner == 0 {
                match cb {
                    // TnJumpTrajectoryBlend.LandingAnimationComplete (its acrobatics
                    // service's landings back on).
                    super::tree::CB_JUMP_LANDED => {
                        ctx.pawn.motion.playing_jump_landing = false;
                        ctx.pawn.landing_animation_enabled = true;
                    }
                    // Landing.OnFinishLanding.
                    super::pawn::CB_ACRO_LANDED => ctx.pawn.acro_landing = None,
                    super::anim::CB_SPAWN_DONE => {
                        ctx.pawn.spawn_complete();
                        // WakeUp.
                        self.core.set_ws(htn::WS_ASLEEP, 0);
                    }
                    _ => {}
                }
                continue;
            }
            if self
                .runner
                .active
                .as_ref()
                .is_some_and(|o| o.serial == cb.owner)
                && let Some(mut op) = self.runner.active.take()
            {
                ops::anim_callback(&mut op, self, ctx, cb.kind);
                if !self.runner.running() || self.runner.active.is_some() {
                    ops::finish(&mut op, self, ctx, Status::Aborted);
                    ops::plan_finish(&mut op, self, ctx, Status::Aborted);
                } else {
                    self.runner.active = Some(op);
                }
            }
        }
    }

    /// TnHtnAiController.Tick.
    pub fn tick(&mut self, ctx: &mut Ctx) {
        for e in std::mem::take(&mut self.core.inbox) {
            self.handle_event(ctx, e);
        }
        sensors::sight_check(self, ctx);
        if self.core.waiting_for_plan_abort && self.runner.interruptible() {
            self.abort_plan(ctx, false);
            self.core.waiting_for_plan_abort = false;
        }
        let symbol = self.runner.active_symbol();
        self.core.ws[htn::WS_CURRENT_TASK].value = symbol;
        self.core.ws[htn::WS_CURRENT_TASK].context =
            if self.runner.interruptible() { 0 } else { 1 };
        let acro = ctx.pawn.acrobatics() as i32;
        self.core.set_ws(htn::WS_ACROBATICS, acro);
        sensors::update(self, ctx);
        if ctx.dt > 0.0 && (self.core.ws_changed || !self.runner.running()) {
            self.find_new_plan(ctx);
        }
        self.runner_update(ctx);
        self.core.ws_changed = false;
        if self.override_goal_completed() {
            self.clear_override_goal();
        }
        let competency = self.core.competency;
        weapon::update_controller(self, ctx, competency);
        super::motion::update(self, ctx);
        // UpdateBrainDead.
        if ctx.dt > 0.0 {
            self.core.brain_dead_frames = if self.runner.running() {
                0
            } else {
                self.core.brain_dead_frames + 1
            };
        }
    }
}
