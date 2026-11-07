//! The jets (TnJetCharacterBlueprint: Escalation's Flyer_Aut and Flyer_Dec, robots
//! that fight as planes), as TransGame's script runs them (their planning domain is
//! cooked: ai.txt's TnJetCharacterBlueprint):
//!
//! * **Their forms** (forms.rs): the plane they start as and come in as (TF_Alt),
//!   and the robot they land as for their pulse attack. As a plane its
//!   TnAiHoverPlaneMotionController drives its plane form (drones.rs's PlaneForm:
//!   hovering at its HoveringSpeed, flying at its FlyingSpeed or boosting at its
//!   BoostFlyingSpeed, the flight TnPlaneSimulation's, its obstacle avoidance on); as
//!   a robot, a TnAiLocomotionController (its SprintSpeed).
//! * **Its flights** (TnPlaneHoverToOperator: TnPlanePathFollowingSensor): straight
//!   to its destination if nothing of the level's is in the way, else along the
//!   level's air network (its AirPathNodes), node by node, the last to its range;
//!   given up if the destination moves out of that range from where it was, or if
//!   the way ahead is blocked (looked at every 0.9 s).
//! * **Its attack on the ground**: to an attack point ahead of its enemy and above
//!   him (TnAirAttackPointSensor), where it hovers turned to face him
//!   (TnHoverToFaceOperator), taunts him and fires its rockets for 3 s; hurt there,
//!   it slips aside (TnPlaneDashOperator); after an enemy beyond its pursuit range
//!   at its boost (TnAerialPursuitSensor).
//! * **Its pulse attack** (once its special attack's cooldown, 8 to 12 s, is up;
//!   one jet at a time, 10 s between: its squad's SAT_JetRobotAttack): to a place by
//!   its enemy and over it (TnChooseJetRobotAttackPointOperator), where it changes
//!   into its robot, falls and lands (Nav_DescentToLand) and blasts all round it
//!   (Ability_PulseAttack, TnAnimatedAttackOperator); then it jumps and changes back.
//! * **Dogfights**: against a player in a jet, it circles him at its boost
//!   (TnBestFlyingCirclingPointSensor) and fires at him head on while each faces
//!   the other (TnShootAtEnemyHeadOnOperator), or round him while he flanks it
//!   (TnShootAtEnemyFireForEffectOperator).
//!
//! The level's traces with an extent are its box swept (drones.rs's sweep_box); the
//! game's also stop at pawns, which these don't.

use std::sync::Arc;

use bevy::prelude::*;

use super::brain::{
    BEHAVIOR_AIMER, BEHAVIOR_ALLOW_MOTION, BEHAVIOR_HEAD_TRACKING, BEHAVIOR_SHOOTER,
    BEHAVIOR_WEAPON_TRACKING, Brain, Ctx, LR_CONFIRMED_ENEMY,
};
use super::data::Behaviour;
use super::drones::{FlightValues, HoverValues, PlaneForm, sweep_box};
use super::forms;
use super::htn::{self, Status, Task};
use super::motion::{self, Facing, MotionProfile};
use super::pawn::{Pawn, Physics};
use super::sensors::with_sensor;
use super::ue;
use super::world::WorldView;

/// The sensors' identifiers (their classes' _Identifier).
pub const S_FLYING_CIRCLING: u8 = 7;
pub const S_AIR_ATTACK_POINT: u8 = 32;
pub const S_AERIAL_PURSUIT: u8 = 35;
pub const S_PLANE_PATH_FOLLOWING: u8 = 39;

// ---------------------------------------------------------------------------
// The plane form
// ---------------------------------------------------------------------------

/// The jets' hover (TR_FlyerAut_VEHDEF_p.Flyer_HoverPlane_Physics, and its class's
/// AiMotionPhysicsScale): its braking distance its blueprint's
/// UseStoppingLinearDampingDistance (set as it comes in).
const FLYER_HOVER: HoverValues = HoverValues {
    max_angular_speed: 18.0,
    max_angular_acceleration: 24.0,
    motion_physics_scale: 2.0,
    stopping_distance_sq: 0.0,
};

/// The jets' flight (TR_FlyerAut_VEHDEF_p.Flyer_Plane_Physics: MaxAiTurnSpeed and
/// MaxAiTurnAcceleration 45, its class's YawDueToYawValue, RollDueToYawValue and
/// ExtraRotationLerpValue; TnPlaneForm's _AIForwardSteeringTarget; its plane form
/// blueprint's CollisionReactionTime).
const FLYER_FLIGHT: FlightValues = FlightValues {
    max_turn_speed: 45.0,
    max_turn_acceleration: 45.0,
    yaw_due_to_yaw: 16.0,
    roll_due_to_yaw: 77.0,
    extra_rotation_lerp: 0.1,
    forward_steering: 2000.0,
    collision_reaction_time: 1.0,
};

/// The plane's rigid body (VH_FlyerAut_SKEL_Physics's hull, round its middle: its
/// half width and half height), its mesh's middle in its own axes (forward, right,
/// up: CalculateCylinderBounds puts it on the Location) and the cylinder those
/// bounds give the pawn as a plane (half their width, their half height: its
/// blueprint's _CollisionRadius, 445).
const BODY: (f32, f32) = (210.0, 183.0);
const MESH_CENTRE: Vec3 = Vec3::new(27.0, 0.0, 151.0);
pub const PLANE_RADIUS: f32 = 445.0;
pub const PLANE_HALF_HEIGHT: f32 = 227.0;

/// TnAiHoverPlaneMotionController's _SlowDownFactor (its default).
const SLOW_DOWN_FACTOR: f32 = 0.5;

fn profile(speed: f32, accel: f32, turn: i32) -> MotionProfile {
    MotionProfile {
        max_linear_speed: speed as i32,
        max_linear_acceleration: accel as i32,
        max_angular_speed: turn,
    }
}

/// OnApplyCharacterProperties: its two motion controllers (CreateJetMotionController:
/// its hover, flight and boost profiles, the last two boosting; its
/// CreateRobotMotionController: its sprint), its plane form's braking distance, its
/// sight, its aim's threshold; and its form's world state.
pub fn apply_properties(brain: &mut Brain, pawn: &mut Pawn, b: &Behaviour) {
    let f = |k: &str, d: f32| b.f(k, d);
    // The robot's locomotion (its SprintSpeed, three times that to get up to it,
    // 180 degrees a second; 270 at its fastest).
    let sprint = f("SprintSpeed", 1800.0).trunc();
    pawn.motion
        .set_profile(0, profile(sprint, (sprint * 3.0).trunc(), 180));
    pawn.motion
        .set_profile(1, profile(sprint, (sprint * 3.0).trunc(), 180));
    pawn.motion
        .set_profile(2, profile(sprint, (sprint * 3.0).trunc(), 270));
    // The plane's (its angular speeds packed floats: its turn modifiers).
    let hover = f("HoveringSpeed", 2000.0).trunc();
    let flying = f("FlyingSpeed", 4000.0).trunc();
    let boost = f("BoostFlyingSpeed", 6000.0).trunc();
    let turn = f("BoostFlyingTurnModifier", 0.75);
    let mut plane = PlaneForm::new(
        profile(hover, hover * 3.0, htn::pack_float(1.0)),
        f("_MaxZOffset", 800.0),
        pawn.loc,
        pawn.yaw,
    );
    plane.profiles = [
        profile(hover, hover * 3.0, htn::pack_float(1.0)),
        profile(flying, flying * 3.0, htn::pack_float(1.0)),
        profile(boost, flying * 3.0, htn::pack_float(turn)),
    ];
    plane.turn_modifiers = [1.0, 1.0, htn::pack_float(turn) as f32 * 0.001];
    plane.boost = [false, true, true];
    plane.slow_down_factor = SLOW_DOWN_FACTOR;
    plane.avoidance = true;
    plane.avoid_min_separation = f("_AvoidanceSeparation", 250.0);
    let stopping = f("UseStoppingLinearDampingDistance", 600.0);
    plane.hover = HoverValues {
        stopping_distance_sq: stopping * stopping,
        ..FLYER_HOVER
    };
    plane.flight = Some(FLYER_FLIGHT);
    plane.body = BODY;
    plane.mesh_centre = MESH_CENTRE;
    plane.looks = Some(Arc::new(VehicleLooks::from_behaviour(b)));
    plane.set_speed(0);
    pawn.plane = Some(Box::new(plane));
    pawn.peripheral_vision = (f("VisionCone", 180.0) * 0.5 * ue::DEG_TO_RAD).cos();
    pawn.sight_radius = f("VisionDistance", 20000.0);
    pawn.aimer.threshold = f("AimingThreshold", 0.906);
    // (Its pawn's cylinder as a plane: its vehicle form's bounds'.)
    if let Some(forms) = pawn.forms.as_mut() {
        forms.caps[forms::ALT as usize] = forms::FormCaps {
            radius: PLANE_RADIUS,
            half_height: PLANE_HALF_HEIGHT,
        };
    }
    let alt = forms::form(pawn) == forms::ALT;
    if alt {
        pawn.radius = PLANE_RADIUS;
        pawn.half_height = PLANE_HALF_HEIGHT;
        activate_plane(pawn);
    }
    brain.core.set_ws(htn::WS_CURRENT_FORM, alt as i32);
}

/// Its plane form taken (TnPlaneForm.OnActivate: flying, its rigid body put where the
/// pawn is, turned as it is; its velocity no more than 3500 uu a second:
/// ActivateSkeletalMeshComponent).
pub fn activate_plane(p: &mut Pawn) {
    let (loc, rotation) = (p.loc, Vec3::new(0.0, p.yaw, 0.0));
    p.vel = p.vel.clamp_length_max(3500.0);
    p.physics = Physics::RigidBody;
    p.pitch = 0.0;
    p.roll = 0.0;
    if let Some(f) = p.plane.as_mut() {
        f.activate(loc, rotation);
    }
}

/// Its robot form taken in the air (TnRobotForm's: falling, upright, as the plane
/// faced).
pub fn activate_robot(p: &mut Pawn) {
    if let Some(f) = p.plane.as_ref() {
        p.yaw = f.rotation.y;
        p.desired_yaw = p.yaw;
    }
    p.pitch = 0.0;
    p.roll = 0.0;
    if p.physics != Physics::Walking {
        p.acrobatics_fall();
    }
}

// ---------------------------------------------------------------------------
// Its flights: TnPlanePathFollowingSensor
// ---------------------------------------------------------------------------

/// TnPlanePathFollowingSensor.kPathCheckFrequency (ClearPathTo's) and the range at
/// which a node on the way counts as reached.
const PATH_CHECK_FREQUENCY: f32 = 0.9;
const REACHED_INTERMEDIATE_POINT: f32 = 500.0;

/// TnPlanePathFollowingSensor: its destination (its location reference, where that
/// was as it set off, its range), its channel along the air network (the nodes'
/// places) and the node it makes for, and the time till it next looks at the way
/// ahead.
#[derive(Clone, Debug, Default)]
pub struct PlanePathFollowing {
    pub moving: bool,
    pub destination_reference: usize,
    pub initial_destination: Vec3,
    pub range: i32,
    pub channel: Vec<Vec3>,
    pub index: usize,
    pub time_till_next_path_check: f32,
}

impl PlanePathFollowing {
    /// get_CurrentDestination.
    fn current_destination(&self) -> Vec3 {
        self.channel
            .get(self.index)
            .copied()
            .unwrap_or(self.initial_destination)
    }

    /// MoveToLocation: off to the reference's place, straight there if it can be,
    /// else along a channel of the air network (its path blocked, and the move
    /// failed, with neither); its motion's speed and facing; WS_Navigation the
    /// reference, WS_Location none.
    pub fn move_to_location(
        &mut self,
        b: &mut Brain,
        ctx: &mut Ctx,
        reference: usize,
        range: i32,
        speed: u8,
        facing: i32,
    ) -> bool {
        self.destination_reference = reference;
        self.initial_destination = b.core.fetch(reference);
        self.range = range;
        self.channel.clear();
        self.index = 0;
        let dest = self.initial_destination;
        if !self.clear_path_to(ctx, dest) && !self.find_channel(ctx, dest) {
            self.set_path_blocked(b, ctx, reference);
            return false;
        }
        motion::set_speed(ctx.pawn, speed);
        ctx.pawn.motion.facing = Facing::from_i32(facing);
        b.core.set_ws(htn::WS_NAVIGATION, reference as i32);
        b.core.set_ws(htn::WS_LOCATION, 0);
        self.prune_start_point(ctx);
        self.move_to_point(ctx);
        true
    }

    /// Pathing.FindChannelToPoint for a flyer: through the air network's nodes
    /// (its AirPathNodes) by the links it fits, from the nearest it can see and fly
    /// on from (or the nearest it can fly on from), to the one nearest its
    /// destination (one the destination can see, if any) of those it can reach.
    fn find_channel(&mut self, ctx: &Ctx, dest: Vec3) -> bool {
        let Some(nav) = ctx.view.nav else {
            return false;
        };
        let p = &*ctx.pawn;
        let moves =
            crate::nav::Moves::air(p.radius / ue::UU_PER_M, p.half_height * 2.0 / ue::UU_PER_M);
        let flies_on = |k: u32| nav.links_of(k).iter().any(|l| moves.supports(l));
        let air = |k: u32| {
            let n = &nav.points[k as usize];
            n.kind == crate::nav::PointKind::Air && !n.blocked
        };
        let nearest = |at: Vec3, ok: &dyn Fn(u32) -> bool| {
            let mut best: Option<(f32, u32)> = None;
            for k in 0..nav.points.len() as u32 {
                if !air(k) || !ok(k) {
                    continue;
                }
                let d = ue::to_ue(nav.points[k as usize].at).distance_squared(at);
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, k));
                }
            }
            best.map(|(_, k)| k)
        };
        let seen = |from: Vec3, k: u32| {
            ctx.view
                .fast_trace(from, ue::to_ue(nav.points[k as usize].at))
        };
        let Some(start) = nearest(p.loc, &|k| flies_on(k) && seen(p.loc, k))
            .or_else(|| nearest(p.loc, &flies_on))
        else {
            return false;
        };
        // (The nodes it can reach from there.)
        let mut reach = vec![false; nav.points.len()];
        reach[start as usize] = true;
        let mut open = vec![start];
        while let Some(k) = open.pop() {
            for l in nav.links_of(k) {
                if moves.supports(l) && !reach[l.to as usize] && air(l.to) {
                    reach[l.to as usize] = true;
                    open.push(l.to);
                }
            }
        }
        let reachable = |k: u32| reach[k as usize];
        let Some(goal) =
            nearest(dest, &|k| reachable(k) && seen(dest, k)).or_else(|| nearest(dest, &reachable))
        else {
            return false;
        };
        let goal_at = nav.points[goal as usize].at;
        let Some(points) = nav.find_channel_to(start, goal, goal_at, &moves) else {
            return false;
        };
        self.channel = points
            .iter()
            .map(|&k| ue::to_ue(nav.points[k as usize].at))
            .collect();
        !self.channel.is_empty()
    }

    /// PruneStartPoint: its first node left out if it lies behind it and the
    /// second can be flown to straight.
    fn prune_start_point(&mut self, ctx: &Ctx) {
        if self.channel.len() <= 1 {
            return;
        }
        let p = &*ctx.pawn;
        if ue::dot2d(self.channel[0] - p.loc, self.channel[1] - p.loc) >= 0.0 {
            return;
        }
        let extent = Vec3::new(p.radius, p.radius, p.half_height);
        if sweep_box(ctx.view, p.loc, self.channel[1], extent).is_none() {
            self.channel.remove(0);
        }
    }

    /// MoveToPoint: its motion controller to the node it makes for (on to the next
    /// within 500 uu), or on its last leg to its range there.
    fn move_to_point(&mut self, ctx: &mut Ctx) {
        let destination = self.current_destination();
        let last = self.channel.is_empty() || self.index + 1 == self.channel.len();
        let (action, range) = if last {
            (0, self.range as f32)
        } else {
            (1, REACHED_INTERMEDIATE_POINT)
        };
        motion::move_to_point(ctx.pawn, destination, action, range);
        self.moving = true;
    }

    /// MoveToNextPoint: on to the next node, if there is one.
    fn move_to_next_point(&mut self, ctx: &mut Ctx) -> bool {
        self.index += 1;
        if self.channel.is_empty() || self.index >= self.channel.len() {
            return false;
        }
        self.move_to_point(ctx);
        true
    }

    /// ClearPathTo: a trace from it to the place meets nothing (the next look at
    /// the way ahead put off).
    fn clear_path_to(&mut self, ctx: &Ctx, at: Vec3) -> bool {
        self.time_till_next_path_check = PATH_CHECK_FREQUENCY;
        ctx.view.fast_trace(ctx.pawn.loc, at)
    }

    /// SetPathBlocked: its pathing sensor told, and its move over.
    fn set_path_blocked(&mut self, b: &mut Brain, ctx: &mut Ctx, reference: usize) {
        let p = &*ctx.pawn;
        with_sensor!(b, pathing, s => s.path_blocked(b, p, reference));
        self.complete_move(b, ctx, false);
    }

    /// CompleteMove: stopped; WS_Location where it got to (none if it didn't),
    /// WS_Navigation off.
    fn complete_move(&mut self, b: &mut Brain, ctx: &mut Ctx, succeeded: bool) {
        motion::stop(ctx.pawn);
        let reference = if succeeded {
            self.destination_reference
        } else {
            0
        };
        b.core.set_ws(htn::WS_LOCATION, reference as i32);
        b.core.set_ws(htn::WS_NAVIGATION, 0);
        self.moving = false;
    }

    /// OnUpdate: there (its motion controller there, its last leg flown) or given
    /// up (its destination gone out of its range of where it was, or the way to its
    /// node blocked).
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx, dt: f32) {
        if !self.moving {
            return;
        }
        let now = b.core.fetch(self.destination_reference);
        if motion::reached_destination(ctx.pawn) && !self.move_to_next_point(ctx) {
            self.complete_move(b, ctx, true);
            return;
        }
        if !self.destination_is_valid(ctx, now) {
            self.complete_move(b, ctx, false);
            return;
        }
        self.time_till_next_path_check -= dt;
    }

    /// DestinationIsValid.
    fn destination_is_valid(&mut self, ctx: &Ctx, now: Vec3) -> bool {
        let from_initial = ue::range_point_to_point(
            self.initial_destination,
            ctx.pawn.radius,
            now,
            0.0,
            0.0,
            0.0,
        );
        if from_initial > self.range as f32 {
            return false;
        }
        if self.time_till_next_path_check < 0.0 {
            let at = self.current_destination();
            if !self.clear_path_to(ctx, at) {
                return false;
            }
        }
        true
    }
}

/// TnPlaneHoverToOperator.OnStart: its motion allowed, its plane path following off
/// to the task's destination (Destination, Range, Speed, Facing); its OnUpdate
/// succeeds at once (the flight goes on: WS_Navigation).
pub fn plane_hover_to(b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
    if b.sensors.plane_path.is_none() {
        return false;
    }
    b.set_behavior(ctx, BEHAVIOR_ALLOW_MOTION, false);
    let reference = task.int(0).clamp(0, super::brain::LR_NUM as i32 - 1) as usize;
    let (range, speed, facing) = (task.int(1), task.int(2).clamp(0, 2) as u8, task.int(3));
    with_sensor!(b, plane_path, s => s.move_to_location(b, ctx, reference, range, speed, facing))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Its attack points: TnAirAttackPointSensor, TnAerialPursuitSensor,
// TnBestFlyingCirclingPointSensor
// ---------------------------------------------------------------------------

/// TnAirAttackPointSensor's kPathNodeRadiusCheck and
/// kMinDistanceFromEnemyForPathNodeSq (its fallback nodes'), and its
/// _VehicleHeightOffset (over an enemy that has a vehicle form: the players).
const PATH_NODE_RADIUS_CHECK: f32 = 1000.0;
const MIN_DISTANCE_FROM_ENEMY_FOR_PATH_NODE_SQ: f32 = 250_000.0;
const VEHICLE_HEIGHT_OFFSET: f32 = 200.0;

/// TnAirAttackPointSensor: where to hover and fire from, ahead of its enemy (by his
/// facing) and above him.
#[derive(Clone, Debug)]
pub struct AirAttackPoint {
    pub cone_half_angle: f32,
    pub vertical_aim_offset: f32,
    pub horizontal_offsets: [f32; 5],
    pub fallback_horizontal_offsets: Vec<f32>,
    pub current_horizontal_offset_index: i32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub min_change_distance_sq: f32,
    pub min_height_above_ground: f32,
    pub min_height_below_roof: f32,
    pub attack_location: Vec3,
}

impl AirAttackPoint {
    /// Its constructor: its cone (degrees), its aim's offsets up and either side
    /// (degrees), its distances from the enemy, the least move to a new point, its
    /// heights over the ground and under a roof (OnInitialize: its pawn's collision
    /// height added).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cone: f32,
        vertical: f32,
        horizontal: f32,
        min_distance: f32,
        max_distance: f32,
        min_change: f32,
        above_ground: f32,
        below_roof: f32,
        collision_height: f32,
    ) -> AirAttackPoint {
        AirAttackPoint {
            cone_half_angle: cone * ue::DEG_TO_RAD,
            vertical_aim_offset: vertical,
            horizontal_offsets: [
                -horizontal,
                -horizontal * 0.5,
                0.0,
                horizontal * 0.5,
                horizontal,
            ],
            fallback_horizontal_offsets: vec![90.0, 180.0, -90.0, 0.0],
            current_horizontal_offset_index: -1,
            min_distance,
            max_distance,
            min_change_distance_sq: min_change * min_change,
            min_height_above_ground: above_ground + collision_height,
            min_height_below_roof: below_roof + collision_height,
            attack_location: Vec3::ZERO,
        }
    }

    /// ChooseNewAttackPoint: from a random one of its offsets on round them, the
    /// first place it finds (FindPossibleAttackLocation); else its fallback offsets';
    /// else an air network node near its enemy (UseFallbackNodes). Into
    /// LR_AttackPoint. (Its data store's points, the level's: none in Escalation.)
    pub fn choose_new_attack_point(&mut self, b: &mut Brain, ctx: &Ctx) {
        let Some(enemy) = b.core.enemy.and_then(|e| ctx.view.actor(e)) else {
            return;
        };
        let n = self.horizontal_offsets.len() as i32;
        self.current_horizontal_offset_index = b.core.rng.rand(n);
        let height = enemy_starting_height_offset(&enemy);
        for _ in 0..n {
            let offset = self.horizontal_offsets[self.current_horizontal_offset_index as usize];
            if let Some(at) = self.find_possible_attack_location(b, ctx, &enemy, offset, height) {
                self.attack_location = at;
                b.core.store(super::brain::LR_ATTACK_POINT, at, Vec3::ZERO);
                return;
            }
            self.current_horizontal_offset_index = (self.current_horizontal_offset_index + 1) % n;
        }
        for k in 0..self.fallback_horizontal_offsets.len() {
            let offset = self.fallback_horizontal_offsets[k];
            if let Some(at) = self.find_possible_attack_location(b, ctx, &enemy, offset, height) {
                self.attack_location = at;
                b.core.store(super::brain::LR_ATTACK_POINT, at, Vec3::ZERO);
                return;
            }
        }
        self.use_fallback_nodes(b, ctx, &enemy);
    }

    /// FindPossibleAttackLocation: along a way (its vertical offset up, the
    /// horizontal offset round from the enemy's flat facing), somewhere in the cone
    /// round it from his eyes (and the height over him), out between its least and
    /// greatest distances, no nearer than the wall that way allows; not too near
    /// where it is; lifted off the ground, or let down from a roof (not both).
    fn find_possible_attack_location(
        &self,
        b: &mut Brain,
        ctx: &Ctx,
        enemy: &super::world::ActorInfo,
        horizontal_offset: f32,
        height: f32,
    ) -> Option<Vec3> {
        let p = &*ctx.pawn;
        let yaw = enemy.yaw + horizontal_offset * ue::ROT_PER_DEG;
        let direction = ue::vec_of(self.vertical_aim_offset * ue::ROT_PER_DEG, yaw);
        let start = enemy_eyes(ctx.view, enemy) + Vec3::Z * height;
        let aim = b.core.rng.vector_in_cone(direction, self.cone_half_angle);
        let mut end = start + aim * self.max_distance;
        let mut max = self.max_distance;
        let extent = Vec3::new(p.radius, p.radius, p.half_height);
        if let Some((hit, _)) = sweep_box(ctx.view, start, end, extent) {
            end = hit - aim * p.radius * 2.0;
            max = (enemy.loc - end).length();
        }
        if (end - start).length_squared() < self.min_distance * self.min_distance {
            return None;
        }
        let distance = b.core.rng.range(self.min_distance, max);
        let mut at = start + aim * distance;
        if (at - p.loc).length_squared() < self.min_change_distance_sq {
            return None;
        }
        let mut at_min_height = false;
        let below = Vec3::Z * self.min_height_above_ground;
        if let Some((hit, _)) = sweep_box(ctx.view, at, at - below, extent) {
            at = hit + below;
            at_min_height = true;
        }
        let above = Vec3::Z * self.min_height_below_roof;
        if let Some((hit, _)) = sweep_box(ctx.view, at, at + above, extent) {
            if at_min_height {
                return None;
            }
            at = hit - above;
        }
        Some(at)
    }

    /// UseFallbackNodes: an air network node within 10 m of its enemy (not within
    /// 5 m) he can see and with room under and over it.
    fn use_fallback_nodes(&mut self, b: &mut Brain, ctx: &Ctx, enemy: &super::world::ActorInfo) {
        let Some(nav) = ctx.view.nav else { return };
        let p = &*ctx.pawn;
        let extent = Vec3::new(p.radius, p.radius, p.half_height);
        let eyes = enemy_eyes(ctx.view, enemy);
        for n in nav.points.iter() {
            if n.blocked || n.kind != crate::nav::PointKind::Air {
                continue;
            }
            let at = ue::to_ue(n.at);
            let d = (at - enemy.loc).length_squared();
            if !(MIN_DISTANCE_FROM_ENEMY_FOR_PATH_NODE_SQ
                ..=PATH_NODE_RADIUS_CHECK * PATH_NODE_RADIUS_CHECK)
                .contains(&d)
            {
                continue;
            }
            let clear = sweep_box(ctx.view, eyes, at, extent).is_none()
                && sweep_box(
                    ctx.view,
                    at,
                    at - Vec3::Z * self.min_height_above_ground,
                    extent,
                )
                .is_none()
                && sweep_box(
                    ctx.view,
                    at,
                    at + Vec3::Z * self.min_height_below_roof,
                    extent,
                )
                .is_none();
            if clear {
                self.attack_location = at;
                b.core.store(super::brain::LR_ATTACK_POINT, at, Vec3::ZERO);
                return;
            }
        }
    }
}

/// GetEnemyStartingHeightOffset: over an enemy with a vehicle form (every
/// player), its _VehicleHeightOffset.
fn enemy_starting_height_offset(enemy: &super::world::ActorInfo) -> f32 {
    if enemy.is_player {
        VEHICLE_HEIGHT_OFFSET
    } else {
        0.0
    }
}

/// Its enemy's eyes (GetViewLocation).
fn enemy_eyes(view: &WorldView, enemy: &super::world::ActorInfo) -> Vec3 {
    view.player(enemy.entity)
        .map_or(enemy.loc + Vec3::Z * enemy.half_height * 0.8, |p| p.eyes)
}

/// TnChooseNewAirAttackPointOperator.OnStart: its sensor's choice, done at once.
pub fn choose_new_air_attack_point(b: &mut Brain, ctx: &mut Ctx) -> bool {
    with_sensor!(b, air_attack, s => s.choose_new_attack_point(b, ctx));
    true
}

/// TnAerialPursuitSensor: where to make for after an enemy too far off, over
/// him (its height offset) and on beyond him from where it is (its leading
/// offset).
#[derive(Clone, Debug, Default)]
pub struct AerialPursuit {
    pub offset: f32,
    pub leading_offset: f32,
    pub attack_location: Vec3,
}

impl AerialPursuit {
    /// ChooseNewPursuitPoint: its height over its enemy (no higher than a roof lets
    /// it), then its leading offset on past him (short of a wall), into
    /// LR_SearchPoint.
    pub fn choose_new_pursuit_point(&mut self, b: &mut Brain, ctx: &Ctx) {
        let p = &*ctx.pawn;
        let enemy = b.core.fetch(LR_CONFIRMED_ENEMY);
        let mut start = enemy + Vec3::Z * self.offset;
        if let Some((hit, _)) = ctx.view.trace(enemy, start) {
            start = hit - Vec3::Z * p.half_height;
        }
        let mut end = start + ue::normal2d(start - p.loc) * self.leading_offset;
        if let Some((hit, _)) = ctx.view.trace(start, end) {
            let back = ue::normal(start - end);
            end = hit + back * p.radius;
        }
        self.attack_location = end;
        b.core.store(super::brain::LR_SEARCH_POINT, end, Vec3::ZERO);
    }
}

/// TnChooseNewAerialPursuitPointOperator.OnStart.
pub fn choose_new_aerial_pursuit_point(b: &mut Brain, ctx: &mut Ctx) -> bool {
    with_sensor!(b, aerial_pursuit, s => s.choose_new_pursuit_point(b, ctx));
    true
}

/// TnBestFlyingCirclingPointSensor's kHitOffsetDistance (back off a wall it would
/// circle beyond).
const HIT_OFFSET_DISTANCE: f32 = 1000.0;

/// TnBestFlyingCirclingPointSensor: the places it circles a jet to, either side of
/// him in turn.
#[derive(Clone, Debug, Default)]
pub struct FlyingCircling {
    pub min_offset_distance: f32,
    pub max_offset_distance: f32,
    pub cone_half_angle: f32,
    pub min_height: f32,
    pub offset_index: usize,
    pub current: Vec3,
}

impl FlyingCircling {
    pub fn new(min: f32, max: f32, cone: f32, min_height: f32) -> FlyingCircling {
        FlyingCircling {
            min_offset_distance: min,
            max_offset_distance: max,
            cone_half_angle: cone * ue::DEG_TO_RAD,
            min_height,
            ..Default::default()
        }
    }

    /// RequestNewLocation: for a reference, a place round its enemy
    /// (GetEnemySurroundingPosition), else where it is; into LR_DrivingCirclingPoint;
    /// the other side next time.
    pub fn request_new_location(&mut self, b: &mut Brain, ctx: &Ctx, reference: usize) {
        self.current = if reference != 0 {
            self.enemy_surrounding_position(b, ctx)
        } else {
            ctx.pawn.loc
        };
        b.core.store(
            super::brain::LR_DRIVING_CIRCLING_POINT,
            self.current,
            Vec3::ZERO,
        );
        self.offset_index = (self.offset_index + 1) % 2;
    }

    /// GetEnemySurroundingPosition: ahead of its enemy or behind him (by his facing,
    /// along the ground), somewhere in the cone that way, out between its
    /// distances short of a wall (backed off it), lifted off the ground; the last
    /// place if none.
    fn enemy_surrounding_position(&mut self, b: &mut Brain, ctx: &Ctx) -> Vec3 {
        let Some(enemy) = b.core.enemy.and_then(|e| ctx.view.actor(e)) else {
            return self.current;
        };
        let offset = if self.offset_index == 0 { 1.0 } else { -1.0 };
        let direction = ue::yaw_vec(enemy.yaw) * offset;
        let start = enemy_eyes(ctx.view, &enemy);
        let aim = b.core.rng.vector_in_cone(direction, self.cone_half_angle);
        let mut end = start + aim * self.max_offset_distance;
        if let Some((hit, _)) = ctx.view.trace(start, end) {
            end = hit - aim * ctx.pawn.radius;
        }
        end -= aim * HIT_OFFSET_DISTANCE;
        let max = (enemy.loc - end).length();
        if (end - start).length_squared() < self.min_offset_distance * self.min_offset_distance {
            return self.current;
        }
        let distance = b.core.rng.range(self.min_offset_distance, max);
        let mut at = start + aim * distance;
        if let Some((hit, _)) = ctx.view.trace(at, at - Vec3::Z * self.min_height) {
            at = hit + Vec3::Z * self.min_height;
        }
        at
    }
}

/// TnChooseNewFlyingCirclingPointOperator.OnStart: its sensor's new place for the
/// task's reference.
pub fn choose_new_flying_circling_point(b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
    if task.n != 1 {
        return false;
    }
    b.set_behavior(ctx, 0, false);
    let reference = task.int(0).max(0) as usize;
    with_sensor!(b, flying_circling, s => s.request_new_location(b, ctx, reference)).is_some()
}

/// TnChooseNewFlyingInvestigateLocationOperator.OnStart: the air network node
/// nearest the reference's place (raised by HeightOffset) within SearchDistance (or
/// that place itself), into OffsetLocationReference.
pub fn choose_new_flying_investigate_location(b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
    let height = task.float(2);
    let base = b.core.fetch(task.int(0).max(0) as usize) + Vec3::Z * height;
    let offset_ref = task.int(1).max(0) as usize;
    let search = task.float(3);
    let at = ctx
        .view
        .nav
        .and_then(|nav| {
            nav.points
                .iter()
                .filter(|n| n.kind == crate::nav::PointKind::Air)
                .map(|n| ue::to_ue(n.at))
                .filter(|at| (*at - base).length_squared() <= search * search)
                .min_by(|a, c| {
                    (*a - base)
                        .length_squared()
                        .total_cmp(&(*c - base).length_squared())
                })
        })
        .unwrap_or(base);
    b.core.store(offset_ref, at, Vec3::ZERO);
    true
}

// ---------------------------------------------------------------------------
// Its pulse attack: TnChooseJetRobotAttackPointOperator
// ---------------------------------------------------------------------------

/// TnChooseJetRobotAttackPointOperatorBlueprint (its jet's RobotTransform* values:
/// SetUpRobotAttackBlueprint).
#[derive(Clone, Copy, Debug, Default)]
pub struct RobotAttackPoint {
    pub forward_offset: f32,
    pub height_offset: f32,
    pub min_distance_from_enemy: f32,
    pub min_distance_above_ground: f32,
    pub min_distance_below_roof: f32,
}

impl RobotAttackPoint {
    pub fn from_behaviour(b: &Behaviour) -> RobotAttackPoint {
        RobotAttackPoint {
            forward_offset: b.f("RobotTransformForwardOffset", 3000.0),
            height_offset: b.f("RobotTransformHeightOffset", 1500.0),
            min_distance_from_enemy: b.f("RobotTransformMinDistanceFromEnemy", 500.0),
            min_distance_above_ground: b.f("RobotTransformMinHeightAboveGround", 1500.0),
            min_distance_below_roof: b.f("RobotTransformMinHeightBelowRoof", 500.0),
        }
    }
}

/// TnChooseJetRobotAttackPointOperator's _OffsetsInDegrees.
const ROBOT_ATTACK_OFFSETS: [f32; 3] = [0.0, 45.0, -45.0];

/// TnChooseJetRobotAttackPointOperator.OnStart: a place by its enemy to change over
/// (GetAttackLocation), into the task's reference; its path blocked (and
/// WS_Location none) if there is none.
pub fn choose_jet_robot_attack_point(b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
    if task.n != 2 {
        return false;
    }
    let Some(enemy) = b.core.enemy.and_then(|e| ctx.view.actor(e)) else {
        return false;
    };
    let reference = task.int(0).max(0) as usize;
    let bp = match b.blueprint.asset(task.int(1)) {
        super::blueprint::Asset::RobotAttackPoint(r) => r,
        _ => RobotAttackPoint::from_behaviour(&b.core.b),
    };
    match robot_attack_location(ctx, &enemy, &bp) {
        Some(at) => {
            b.core.store(reference, at, Vec3::ZERO);
            true
        }
        None => {
            let p = &*ctx.pawn;
            with_sensor!(b, pathing, s => s.path_blocked(b, p, reference));
            b.core.set_ws(htn::WS_LOCATION, 0);
            false
        }
    }
}

/// GetAttackLocation: for each offset round the enemy's facing, as far as he could
/// walk that way (to its ForwardOffset: AI.CanWalkLine) unless too near him, else
/// at its least distance from him; the first with room (IsValidAttackLocation).
fn robot_attack_location(
    ctx: &Ctx,
    enemy: &super::world::ActorInfo,
    bp: &RobotAttackPoint,
) -> Option<Vec3> {
    let nav = ctx.view.nav;
    let cell = ctx.view.cell_at(enemy.feet);
    for offset in ROBOT_ATTACK_OFFSETS {
        let direction = ue::yaw_vec(enemy.yaw + offset * ue::ROT_PER_DEG);
        let offset_location = enemy.loc + direction * bp.forward_offset;
        let walked = match (nav, cell) {
            (Some(n), Some(c)) => {
                let (_, end) = n.walk_line(
                    c,
                    ue::from_ue(enemy.feet),
                    ue::from_ue(offset_location - Vec3::Z * enemy.half_height),
                    enemy.radius / ue::UU_PER_M,
                );
                ue::to_ue(end) + Vec3::Z * enemy.half_height
            }
            _ => offset_location,
        };
        let min = bp.min_distance_from_enemy;
        if (enemy.loc - walked).length_squared() >= min * min
            && let Some(at) = valid_robot_attack_location(ctx, walked, bp)
        {
            return Some(at);
        }
        if let Some(at) = valid_robot_attack_location(ctx, enemy.loc + direction * min, bp) {
            return Some(at);
        }
    }
    None
}

/// IsValidAttackLocation: its HeightOffset over the place, lifted off the ground or
/// let down from a roof (not both).
fn valid_robot_attack_location(ctx: &Ctx, at: Vec3, bp: &RobotAttackPoint) -> Option<Vec3> {
    let p = &*ctx.pawn;
    let extent = Vec3::new(p.radius, p.radius, p.half_height);
    let mut offset = at + Vec3::Z * bp.height_offset;
    let below = Vec3::Z * bp.min_distance_above_ground;
    let above = Vec3::Z * bp.min_distance_below_roof;
    let mut at_min_height = false;
    if let Some((hit, _)) = sweep_box(ctx.view, offset, offset - below, extent) {
        offset = hit + below;
        at_min_height = true;
    }
    if let Some((hit, _)) = sweep_box(ctx.view, offset, offset + above, extent) {
        if at_min_height {
            return None;
        }
        offset = hit - above;
    }
    Some(offset)
}

// ---------------------------------------------------------------------------
// Its hovers and dashes: TnHoverToFaceOperator, TnPlaneDashOperator
// ---------------------------------------------------------------------------

/// TnHoverToFaceOperator._MaxDuration (its default).
const HOVER_TO_FACE_MAX_DURATION: f32 = 5.0;

/// TnHoverToFaceOperator: to a place at its hover's speed, turned to face it or its
/// enemy (Facing: 0 or 1 the place, 2 its enemy); done once within its range and
/// facing that way, failed after its _MaxDuration.
#[derive(Clone, Debug, Default)]
pub struct HoverToFace {
    reference: usize,
    destination: Vec3,
    facing: i32,
    range: f32,
    max_duration: f32,
}

impl HoverToFace {
    pub fn start(&mut self, b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
        if task.n != 3 {
            return false;
        }
        self.max_duration = HOVER_TO_FACE_MAX_DURATION;
        self.reference = task.int(0).max(0) as usize;
        b.set_behavior(ctx, 0, false);
        self.destination = b.core.fetch(self.reference);
        self.facing = task.int(1);
        let radius = ctx.pawn.radius;
        motion::move_to_point(ctx.pawn, self.destination, 0, radius);
        ctx.pawn.motion.facing = Facing::from_i32(self.facing);
        motion::set_speed(ctx.pawn, 0);
        self.range = task.float(2);
        let range = super::drones::range_to(ctx.pawn, b.core.fetch(self.reference));
        self.settle(ctx.pawn, range);
        true
    }

    /// IsFacing (CalculateDesiredFacing).
    fn is_facing(&self, b: &Brain, p: &Pawn) -> bool {
        let to = match self.facing {
            2 => b.core.fetch(LR_CONFIRMED_ENEMY),
            _ => self.destination,
        };
        let desired = ue::normal2d(to - p.loc);
        // (Right over or under him there is no way across the ground to face him,
        // and it would wait out its 5 s, again and again, as long as he stood
        // still: a hover settles on the very spot here, where the game's jet, never
        // quite still, always had some way to face. Any way does.)
        if desired == Vec3::ZERO {
            return true;
        }
        let facing = match p.active_plane() {
            Some(f) => ue::normal2d(ue::yaw_vec(f.rotation.y)),
            None => p.forward(),
        };
        desired.dot(facing) > 0.9
    }

    /// Its plane put on its way to the place, as long as it isn't within the
    /// operator's range of it while its motion controller counts it as there.
    /// (The controller steers it only till then (ReachedDestination: within its
    /// radius of the place across the ground, or past it, and within its
    /// MaxZOffset, 800 uu, of its height), but the operator wants its cylinder over
    /// the place (its Range, 0.2 uu: packed). Begun within that radius, come in fast
    /// enough to sail on to the stop its flight's end set (its velocity's half
    /// second on), or over or under the place by more than its half height (227
    /// uu), it was left where it was and failed after its 5 s, again and again until
    /// its robot attack came round: a jet that hung in the air not firing.)
    fn settle(&self, p: &mut Pawn, range: f32) {
        if range < self.range {
            return;
        }
        let loc = p.loc;
        if let Some(f) = p.active_plane_mut()
            && f.moving
            && f.reached(loc)
        {
            f.desired_location = self.destination;
        }
    }

    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) -> Status {
        self.max_duration -= ctx.dt;
        if self.max_duration <= 0.0 {
            return Status::Failed;
        }
        let range = super::drones::range_to(ctx.pawn, b.core.fetch(self.reference));
        if range < self.range && self.is_facing(b, ctx.pawn) {
            return Status::Succeeded;
        }
        self.settle(ctx.pawn, range);
        Status::Running
    }

    /// OnFinish: its motion facing as it turns (Rotation).
    pub fn finish(&mut self, ctx: &mut Ctx) {
        ctx.pawn.motion.facing = Facing::Rotation;
    }
}

/// TnPlaneDashOperator's kTraceDistance (the room either side it looks for) and
/// kEndLocationDistance (how far aside it puts its reference).
const DASH_TRACE_DISTANCE: f32 = 1500.0;
const DASH_END_LOCATION_DISTANCE: f32 = 1200.0;

/// TnPlaneDashOperator: a dash to its right or left (whichever it has room for,
/// either if both), MinDashDistance to MaxDashDistance aside, for no longer than
/// MaxDashTime; its reference put 1200 uu that way as it ends.
#[derive(Clone, Debug, Default)]
pub struct PlaneDash {
    reference: usize,
    max_dash_time: f32,
    dash_location: Vec3,
    end_location: Vec3,
}

impl PlaneDash {
    pub fn start(&mut self, b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
        self.reference = task.int(0).max(0) as usize;
        let min = task.int(1) as f32;
        let max = task.int(2) as f32;
        self.max_dash_time = task.float(3);
        let side = dash_direction(b, ctx);
        let p = &*ctx.pawn;
        let dash = ue::yaw_vec(p.yaw + 16384.0) * side;
        self.dash_location = p.loc + dash * b.core.rng.range(min, max);
        self.end_location = p.loc + dash * DASH_END_LOCATION_DISTANCE;
        if side == 0.0 {
            return true;
        }
        motion::set_speed(ctx.pawn, 0);
        ctx.pawn.motion.facing = Facing::Rotation;
        motion::move_to_point(ctx.pawn, self.dash_location, 0, 50.0);
        true
    }

    pub fn update(&mut self, ctx: &mut Ctx) -> Status {
        self.max_dash_time -= ctx.dt;
        if self.max_dash_time <= 0.0 {
            return Status::Succeeded;
        }
        if super::drones::range_to(ctx.pawn, self.dash_location) > 50.0 {
            Status::Running
        } else {
            Status::Succeeded
        }
    }

    pub fn finish(&mut self, b: &mut Brain) {
        b.core.store(self.reference, self.end_location, Vec3::ZERO);
    }
}

/// CalculateDashDirection: 1 to its right, -1 to its left, whichever a trace
/// 1500 uu that way finds clear (either if both: RandomSign), 0 if neither.
fn dash_direction(b: &mut Brain, ctx: &Ctx) -> f32 {
    let p = &*ctx.pawn;
    let right = ue::yaw_vec(p.yaw + 16384.0) * DASH_TRACE_DISTANCE;
    let can_right = ctx.view.fast_trace(p.loc, p.loc + right);
    let can_left = ctx.view.fast_trace(p.loc, p.loc - right);
    match (can_right, can_left) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        (false, false) => 0.0,
        (true, true) => {
            if b.core.rng.frand() < 0.5 {
                -1.0
            } else {
                1.0
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Its changes of form and its jump: TnTransformOperator, TnJumpOperator
// ---------------------------------------------------------------------------

/// TnTransformOperator: a change of form (TargetForm), its momentum kept or not
/// (MaintainMomentum), done as the change ends or after MaxDuration.
#[derive(Clone, Debug, Default)]
pub struct Transform {
    has_max_duration: bool,
    duration: f32,
}

impl Transform {
    pub fn start(&mut self, b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
        b.set_behavior(ctx, 0, false);
        let target = task.int(0).clamp(0, 1) as u8;
        if task.n > 1 && task.int(1) != 1 {
            // HandleMomentum: stopped.
            ctx.pawn.vel = Vec3::ZERO;
        }
        if task.n > 2 {
            self.has_max_duration = true;
            self.duration = task.float(2);
        }
        // (Its plane's rigid body put on the pawn's Rotation: SetRBRotation.)
        if target == forms::ALT {
            let yaw = ctx.pawn.yaw;
            if let Some(f) = ctx.pawn.plane.as_mut() {
                f.rotation = Vec3::new(0.0, yaw, 0.0);
            }
        }
        target == forms::form(ctx.pawn) || forms::transform(ctx.pawn, target, None)
    }

    pub fn update(&mut self, ctx: &mut Ctx) -> Status {
        self.duration -= ctx.dt;
        if self.has_max_duration && self.duration <= 0.0 {
            return Status::Succeeded;
        }
        if forms::is_transforming(ctx.pawn) {
            Status::Running
        } else {
            Status::Succeeded
        }
    }
}

/// TnJumpOperator: its form's jump (its acrobatics' JumpHeight), done at its top
/// (FinishAtTop) or as it lands.
#[derive(Clone, Debug, Default)]
pub struct Jump {
    finish_at_top: bool,
}

impl Jump {
    pub fn start(&mut self, b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
        b.set_behavior(ctx, 0, false);
        self.finish_at_top = task.int(0) == 1;
        let height = b
            .core
            .b
            .f("Other.AcrobaticsManagerBlueprint.JumpHeight", 850.0);
        let speed = (2.0 * -super::pawn::GRAVITY_Z * height).sqrt();
        ctx.pawn.form_jump(Vec3::Z * speed);
        true
    }

    pub fn update(&mut self, ctx: &mut Ctx) -> Status {
        let p = &*ctx.pawn;
        let going = if self.finish_at_top {
            p.vel.z > 0.0
        } else {
            p.physics == Physics::Falling
        };
        if going {
            Status::Running
        } else {
            Status::Succeeded
        }
    }
}

// ---------------------------------------------------------------------------
// Its dogfights: TnShootAtEnemyHeadOnOperator, TnShootAtEnemyFireForEffectOperator
// ---------------------------------------------------------------------------

/// TnShootAtEnemyHeadOnOperator (`fire_for_effect` none) and
/// TnShootAtEnemyFireForEffectOperator (its targets' distances round its enemy):
/// its gun on its enemy, shooting while it can (head on: while it faces him within
/// FacingEnemyTolerance and he faces it within EnemyFacingTolerance; for effect:
/// at a point round him, while its line of fire is clear); its battle cry as it
/// opens fire. Running till its enemy is dead.
#[derive(Clone, Debug, Default)]
pub struct Dogfight {
    facing_enemy_tolerance: f32,
    enemy_facing_pawn_tolerance: f32,
    fire_for_effect: Option<(f32, f32)>,
    sound: i32,
}

impl Dogfight {
    pub fn start(
        &mut self,
        b: &mut Brain,
        ctx: &mut Ctx,
        task: &Task,
        fire_for_effect: bool,
    ) -> bool {
        if b.core.enemy.is_none() {
            return false;
        }
        if fire_for_effect {
            self.fire_for_effect = Some((task.float(0), task.float(1)));
        } else {
            self.facing_enemy_tolerance = (task.float(0) * ue::DEG_TO_RAD).cos();
            self.enemy_facing_pawn_tolerance = (task.float(1) * ue::DEG_TO_RAD).cos();
        }
        b.set_behavior(
            ctx,
            BEHAVIOR_HEAD_TRACKING
                | BEHAVIOR_AIMER
                | BEHAVIOR_WEAPON_TRACKING
                | BEHAVIOR_ALLOW_MOTION,
            false,
        );
        self.sound = if task.n > 2 { task.int(2) } else { -1 };
        true
    }

    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) -> Status {
        let Some(e) = b.core.enemy else {
            return Status::Failed;
        };
        let Some(enemy) = ctx.view.actor(e) else {
            return Status::Failed;
        };
        if enemy.dead {
            return Status::Succeeded;
        }
        let mut aim = super::ops::find_aim_location_on_target(b, ctx, e);
        let shoot = match self.fire_for_effect {
            Some((min, max)) => {
                aim += b.core.rng.unit_vector() * b.core.rng.range(min, max);
                b.core.ws_bool(htn::WS_WEAPON_HAS_CLEAR_LINE_OF_FIRE)
            }
            None => self.can_shoot(b, ctx.pawn, &enemy),
        };
        if shoot {
            b.add_behavior(ctx, BEHAVIOR_SHOOTER);
            if self.sound >= 0
                && let super::blueprint::Asset::Dialog(event) = b.blueprint.asset(self.sound)
            {
                ctx.shared
                    .dialog
                    .push(super::audio::DialogRequest::say(ctx.pawn.entity, &event));
            }
        } else {
            b.remove_behavior(ctx, BEHAVIOR_SHOOTER);
        }
        ctx.pawn.set_focal_point(aim);
        ctx.pawn.set_look_at_point(aim);
        Status::Running
    }

    /// CanShoot: it faces its enemy, he faces it, and its line of fire is clear.
    fn can_shoot(&self, b: &Brain, p: &Pawn, enemy: &super::world::ActorInfo) -> bool {
        let to_enemy = ue::normal(enemy.loc - p.loc);
        let facing = match p.active_plane() {
            Some(f) => ue::vec_of(f.rotation.x, f.rotation.y),
            None => p.forward(),
        };
        if to_enemy.dot(facing) < self.facing_enemy_tolerance {
            return false;
        }
        if (-to_enemy).dot(ue::yaw_vec(enemy.yaw)) < self.enemy_facing_pawn_tolerance {
            return false;
        }
        b.core.ws_bool(htn::WS_WEAPON_HAS_CLEAR_LINE_OF_FIRE)
    }
}

// ---------------------------------------------------------------------------
// Its plane's looks: TnPlaneForm's effects, HmPlayerVehicleAudioComponent
// ---------------------------------------------------------------------------

/// A jet's plane's effects and engine (its chassis' TnPlaneFormBlueprint and its
/// AudioComp, an HmPlayerVehicleAudioComponent), as TnPlaneForm plays them:
///
/// * hovering (its Hovering state), its hover jets (HoverFX: each a particle system
///   on a socket), each as big as it pushes (Hovering.UpdateFx: its Size,
///   CalculateBoosterFxAmount: the share along its push of 30 m/s^2 of the body's
///   smoothed acceleration, a jet having no gravity, and of 6 rad/s^2 of the turn it
///   would give against its smoothed angular acceleration, at most 1, times its
///   socket's scale); flying (Flying), its afterburners and contrail (BoostFx); each
///   in the jet's energon colour (TnVehicleFxPlayer.Play), started as its state
///   begins and let go as it ends (Stop: what is out of them living out its life);
/// * its engine: hovering, its off-load loops (an AI's engine load is never on:
///   only a player's Hovering.UpdateSounds changes it); flying, its boost loops
///   (PlayBoostSound: faded in over BoostFadeInTime, out over BoostFadeOutTime),
///   with its boost's one-shots as it takes off and its off-load one-shots (its
///   boost's end) as it drops back to hovering; and flying, its flyby as it passes
///   within FlybyTriggerDistance of the listener (Flying.BeginState:
///   _AllowFlybySound).
///
/// Its events are its sound event set's (VEH_JET_FLYER_01's: ai_sounds.txt).
#[derive(Clone, Debug, Default)]
pub struct VehicleLooks {
    pub hover_fx: Vec<(Arc<str>, Arc<str>)>,
    pub boost_fx: Vec<(Arc<str>, Arc<str>)>,
    pub offload_loops: Vec<String>,
    pub offload_oneshots: Vec<String>,
    pub boost_loops: Vec<String>,
    pub boost_oneshots: Vec<String>,
    pub boost_fade: (f32, f32),
    pub flyby: Option<(String, f32)>,
}

impl VehicleLooks {
    pub fn from_behaviour(b: &Behaviour) -> VehicleLooks {
        // (A particle system or a sound event by its name, its package left off.)
        let short = |v: &str| v.rsplit('.').next().unwrap_or(v).to_string();
        let fx = |key: &str| -> Vec<(Arc<str>, Arc<str>)> {
            (0..b.n(key))
                .filter_map(|i| {
                    let system = b.get(&format!("{key}[{i}].ParticleSystemTemplate"))?;
                    let socket = b.get(&format!("{key}[{i}].Socket"))?;
                    (system != "None" && socket != "None")
                        .then(|| (Arc::from(short(system).as_str()), Arc::from(socket)))
                })
                .collect()
        };
        let events = |key: &str| -> Vec<String> {
            (0..b.n(key))
                .filter_map(|i| b.get(&format!("{key}[{i}].Sound")))
                .filter(|v| *v != "None")
                .map(short)
                .collect()
        };
        let a = "Chassis.AudioComp.";
        VehicleLooks {
            hover_fx: fx("Chassis.HoverFX"),
            boost_fx: fx("Chassis.BoostFx"),
            offload_loops: events(&format!("{a}OffLoadSound.OffloadLoops")),
            offload_oneshots: events(&format!("{a}OffLoadSound.OffloadOneshots")),
            boost_loops: events(&format!("{a}BoostSounds.BoostLoops")),
            boost_oneshots: events(&format!("{a}BoostSounds.BoostOneshots")),
            boost_fade: (
                b.f(&format!("{a}BoostFadeInTime"), 0.0),
                b.f(&format!("{a}BoostFadeOutTime"), 0.0),
            ),
            flyby: b
                .get(&format!("{a}FlybySound"))
                .filter(|v| *v != "None")
                .map(|v| (short(v), b.f(&format!("{a}FlybyTriggerDistance"), 0.0))),
        }
    }
}

/// How its engine's loops fade out once they are let go (its off-load's: the
/// drones' engine's).
const ENGINE_FADE: f32 = super::drones::ENGINE_FADE;

/// Its engine this frame (VehicleLooks): its loops held for the state it is in
/// (none while its plane isn't out, or as it dies), its one-shots as it changes
/// (`oneshots`: not on a client's copy of the host's jet, whose come from the host's,
/// sync.rs).
pub fn engine_sounds(
    p: &mut Pawn,
    sounds: &super::audio::AiSounds,
    shared: &mut super::shared::Shared,
    oneshots: bool,
) {
    let out = p.active_plane().is_some()
        && !p.dying
        && p.forms.as_ref().is_none_or(|f| f.change.is_none());
    let Some(f) = p.plane.as_mut() else { return };
    let Some(looks) = f.looks.clone() else { return };
    let now = out.then_some(f.boosting);
    let was = std::mem::replace(&mut f.sounded, now);
    let (entity, at) = (p.entity, p.loc);
    let cue = |event: &str| p.voice.find_cue(sounds, event).map(|(c, _)| c);
    let oneshots = match (was, now) {
        _ if !oneshots => &[][..],
        (Some(false) | None, Some(true)) => &looks.boost_oneshots[..],
        (Some(true), Some(false)) => &looks.offload_oneshots[..],
        _ => &[][..],
    };
    for e in oneshots {
        if let Some(c) = cue(e) {
            shared.sounds.push((entity, c, at));
        }
    }
    let (loops, fade) = match now {
        Some(false) => (&looks.offload_loops[..], ENGINE_FADE),
        Some(true) => (&looks.boost_loops[..], looks.boost_fade.1),
        None => (&[][..], 0.0),
    };
    for e in loops {
        if let Some(c) = cue(e) {
            shared.loops.push((entity, c, at, fade));
        }
    }
}

/// A jet's plane's effects as they play (plane_effects): each hover jet's and
/// booster's (its list, 0 hover or 1 boost, and its place in it), its body's
/// velocity and turn last frame (the demo's space), its angular velocity in its
/// own axes, and its smoothed acceleration and angular acceleration in them
/// (Hovering.UpdateFx: _SmoothLocalLinearAcceleration and
/// _SmoothLocalAngularAcceleration, VLerp by 0.3 a frame); and whether it was within
/// its flyby's reach of the listener.
#[derive(Default)]
pub struct PlaneFx {
    playing: Vec<(usize, usize, crate::levelfx::FxId)>,
    last: Option<(Vec3, Quat)>,
    spin: Vec3,
    accel: Vec3,
    spin_accel: Vec3,
    near: bool,
}

/// The hover jets' smoothing a frame (VLerp's 0.3, at 60 frames a second).
const FX_ACCEL_SHARE: f32 = 0.3;
/// CalculateBoosterFxAmount's limits: HoverPhysics' ThrusterLinearAccelerationLimit
/// (3000 uu/s^2) and ThrusterAngularAccelerationLimit (6 rad/s^2).
const THRUSTER_ACCEL: f32 = 30.0;
const THRUSTER_SPIN_ACCEL: f32 = 6.0;

/// Each jet's plane's effects and flyby (VehicleLooks) where its model's sockets
/// are: hovering its hover jets, as big as each pushes; flying its afterburners and
/// contrail; each in its energon colour.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn plane_effects(
    time: Res<Time>,
    kinds: Res<crate::enemies::Kinds>,
    world: Res<super::AiWorld>,
    ais: Query<(
        Entity,
        &super::Ai,
        &crate::enemies::Enemy,
        &crate::enemies::Model,
    )>,
    globals: Query<&GlobalTransform>,
    listener: Query<&GlobalTransform, With<crate::OrbitCamera>>,
    mut fx: ResMut<crate::levelfx::FxSystems>,
    mut sfx: ResMut<crate::Sfx>,
    mut states: Local<std::collections::HashMap<Entity, PlaneFx>>,
) {
    let dt = time.delta_secs();
    // (With two players on one screen, each camera.)
    let ears: Vec<Vec3> = listener.iter().map(|g| g.translation()).collect();
    let mut seen = Vec::new();
    for (e, ai, enemy, model) in &ais {
        let p = &ai.pawn;
        let Some(f) = p.plane.as_ref() else { continue };
        let Some(looks) = f.looks.as_ref() else {
            continue;
        };
        seen.push(e);
        let st = states.entry(e).or_default();
        let out = enemy.alive()
            && !p.dying
            && p.active_plane().is_some()
            && p.forms.as_ref().is_none_or(|f| f.change.is_none());
        let (floating, boosting) = (out && !f.boosting, out && f.boosting);
        // Its body's smoothed acceleration and angular acceleration, in its own axes.
        let rot = super::drones::body_rotation(f);
        let vel = ue::dir_from_ue(p.vel) / ue::UU_PER_M;
        if dt > 0.0 {
            if let Some((last_vel, last_rot)) = st.last {
                let accel = rot.inverse() * ((vel - last_vel) / dt);
                let (axis, angle) = (rot * last_rot.inverse()).to_axis_angle();
                let angle = (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                let spin = rot.inverse() * (axis * angle / dt);
                let spin_accel = (spin - st.spin) / dt;
                st.spin = spin;
                let k = 1.0 - (1.0 - FX_ACCEL_SHARE).powf(dt * 60.0);
                st.accel += (accel - st.accel) * k;
                st.spin_accel += (spin_accel - st.spin_accel) * k;
            }
            st.last = Some((vel, rot));
        }
        let kind = kinds.kinds.get(enemy.kind);
        let color = [("Color", kind.map_or([255; 4], |k| k.energon_color()))];
        let centre = ue::from_ue(p.loc);
        for (group, list, on) in [
            (0, &looks.hover_fx, floating),
            (1, &looks.boost_fx, boosting),
        ] {
            for (i, (system, socket)) in list.iter().enumerate() {
                let have = st.playing.iter().position(|x| x.0 == group && x.1 == i);
                if !on {
                    if let Some(j) = have {
                        fx.stop(st.playing.remove(j).2);
                    }
                    continue;
                }
                let Some((at, x, y)) = model.socket_world(socket, &globals) else {
                    continue;
                };
                let id = match have {
                    Some(j) => st.playing[j].2,
                    None => {
                        let Some(id) = fx.spawn(system, at, x, &color) else {
                            continue;
                        };
                        st.playing.push((group, i, id));
                        id
                    }
                };
                // (One that has played out stays out till its state comes again.)
                if !fx.alive(id) {
                    continue;
                }
                fx.place_turned(id, at, x, y);
                if group == 0 {
                    // CalculateBoosterFxAmount: its push (against its flame) and
                    // where it is, in the body's axes; no gravity.
                    let push = rot.inverse() * -x;
                    let from = rot.inverse() * (at - centre);
                    let linear = (st.accel.dot(push) / THRUSTER_ACCEL).clamp(0.0, 1.0);
                    let angular = (from.normalize_or_zero().cross(push).dot(st.spin_accel)
                        / THRUSTER_SPIN_ACCEL)
                        .clamp(0.0, 1.0);
                    let scale = kind
                        .and_then(|k| k.socket_scales.iter().find(|s| *s.0 == **socket))
                        .map_or(Vec3::ONE, |s| s.1);
                    fx.set_vector(id, "Size", scale * (linear + angular).min(1.0));
                }
            }
        }
        // Its flyby: flying, as it comes within its reach of a listener.
        if let Some((event, reach)) = looks.flyby.as_ref()
            && !ears.is_empty()
        {
            let near = boosting
                && ears
                    .iter()
                    .any(|ear| ear.distance(centre) < reach / ue::UU_PER_M);
            if near
                && !st.near
                && let Some((cue, _)) = p.voice.find_cue(&world.sounds, event)
            {
                sfx.play_at(cue, centre);
            }
            st.near = near;
        }
    }
    // (Those gone: their effects let go.)
    states.retain(|e, st| {
        if seen.contains(e) {
            return true;
        }
        for (_, _, id) in st.playing.drain(..) {
            fx.stop(id);
        }
        false
    });
}

// ---------------------------------------------------------------------------
// Its sensors
// ---------------------------------------------------------------------------

/// OnPopulateSensorCollection: its sight, enemy status, threats, weapon, pathing
/// (6: 10 s between attempts, 2: 5 s, 3: 3 s), hearing, attack points, pursuit,
/// special attack (its robot's pulse: RobotWeaponCooldownRange), circling, pain and
/// plane path following.
pub fn populate(b: &mut Brain, ctx: &mut Ctx) {
    use super::sensors::{
        self as s, EnemyStatusSensor, HearingSensor, PainSensor, PathingSensor,
        SpecialAttackCooldownSensor, ThreatEval, ThreatSensor, VisionProfile, VisionSensor,
        WeaponSensor, register,
    };
    let bp: Arc<Behaviour> = b.core.b.clone();
    let f = |k: &str, d: f32| bp.f(k, d);
    let cone = f("VisionCone", 180.0);
    let distance = f("VisionDistance", 20000.0);
    let mut vision = VisionSensor::default();
    let profile = VisionProfile {
        fov: cone,
        distance,
        ..Default::default()
    };
    vision.set_profile(0, profile);
    vision.set_profile(1, profile);
    b.sensors.vision = Some(vision);
    register(b, ctx, s::S_VISION, 1, false);
    b.sensors.status = Some(EnemyStatusSensor {
        ranges: vec![f("VehiclePursuitRange", 10000.0), 1_000_000.0],
        flanking_half_angle: f("DogfightEnemyFacingPawnTolerance", 35.0),
        ..Default::default()
    });
    register(b, ctx, s::S_ENEMY_STATUS, 1, false);
    // CreateThreatSensor (its evaluators summed; Escalation's two more).
    let mut threat = ThreatSensor::new(0.0);
    threat.add_evaluator(
        ThreatEval::PlayerEnemy {
            score: f("PlayerThreatScore", 0.2),
        },
        1.0,
    );
    threat.add_evaluator(
        ThreatEval::EnemyDistance {
            max_range: distance,
        },
        1.0,
    );
    threat.add_evaluator(ThreatEval::Sentry { discard: true }, 1.0);
    threat.add_evaluator(ThreatEval::DamagedBy, 3.0);
    threat.add_evaluator(ThreatEval::TargetDistribution, 0.1);
    threat.add_evaluator(ThreatEval::AbilityAgro, 2.0);
    threat.add_evaluator(ThreatEval::Downed, 2.0);
    b.sensors.threat = Some(threat);
    register(b, ctx, s::S_THREAT, 1, false);
    b.sensors.weapon = Some(WeaponSensor {
        check_for_aiming: false,
        check_for_pawns_in_lof: true,
        check_for_los: true,
    });
    register(b, ctx, s::S_WEAPON, 0, false);
    let mut pathing = PathingSensor::default();
    for (lr, secs) in [(6, 10.0), (2, 5.0), (3, 3.0)] {
        pathing.blocked[lr].1 = secs;
    }
    b.sensors.pathing = Some(pathing);
    register(b, ctx, s::S_PATHING, 0, false);
    b.sensors.hearing = Some(HearingSensor::new(1.0, 10.0));
    register(b, ctx, s::S_HEARING, 1, false);
    let collision_height = ctx.pawn.half_height;
    b.sensors.air_attack = Some(AirAttackPoint::new(
        f("AttackPointConeHalfAngle", 5.0),
        f("AttackPointVerticalOffset", 25.0),
        f("AttackPointHorizontalOffset", 50.0),
        f("MinAttackDistance", 3000.0),
        f("MaxAttackDistance", 8000.0),
        f("MinAttackChangeDistance", 1000.0),
        f("MinAttackHeightAboveGround", 1500.0),
        f("MinAttackHeightBelowRoof", 500.0),
        collision_height,
    ));
    register(b, ctx, S_AIR_ATTACK_POINT, 3, false);
    b.sensors.aerial_pursuit = Some(AerialPursuit {
        offset: f("PursuitHeightOffset", 1500.0),
        leading_offset: f("PursuitLeadingOffset", 4000.0),
        ..Default::default()
    });
    register(b, ctx, S_AERIAL_PURSUIT, 3, false);
    b.sensors.special_attack = Some(SpecialAttackCooldownSensor::new(
        (
            f("RobotWeaponCooldownRange.Min", 8.0),
            f("RobotWeaponCooldownRange.Max", 12.0),
        ),
        0,
    ));
    register(b, ctx, s::S_SPECIAL_ATTACK, 1, false);
    b.sensors.flying_circling = Some(FlyingCircling::new(
        f("MinDogfightDistance", 4000.0),
        f("MaxDogfightDistance", 8000.0),
        f("DogfightOffsetConeHalfAngle", 35.0),
        f("MinAttackHeightAboveGround", 1500.0),
    ));
    register(b, ctx, S_FLYING_CIRCLING, 3, false);
    b.sensors.pain = Some(PainSensor::new(false));
    register(b, ctx, s::S_PAIN, 1, false);
    b.sensors.plane_path = Some(PlanePathFollowing::default());
    register(b, ctx, S_PLANE_PATH_FOLLOWING, 0, false);
}
