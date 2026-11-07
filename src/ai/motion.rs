//! The enemies' locomotion: TnAiLocomotionController (the soldiers' motion
//! controller: it steers the pawn's acceleration at a point, at one of three speed
//! profiles, slowing near its destination or matching the speed the next corner
//! wants, and jumps), with its obstacle avoidance (TnObstacleAvoidanceManager and
//! TnObstacleAvoidance: steering round the pawn it would soonest run into).

use bevy::prelude::*;

use super::brain::Brain;
use super::pawn::{GRAVITY_Z, JumpInfo, Pawn, Physics};
use super::ue;
use super::world::WorldView;

/// UE3's NavigationPoint's CylinderComponent CollisionHeight: a path node stands
/// this far over the floor (Jump takes it off the landing point).
pub const NAV_POINT_HEIGHT: f32 = 50.0;
/// Pathing's jump peaks over the higher end (Pathing.150 and .500 for a high jump).
pub const JUMP_PEAK: f32 = 150.0;
pub const HIGH_JUMP_PEAK: f32 = 500.0;

/// ELocomotionState.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LocoState {
    #[default]
    Stopped = 0,
    Walking = 1,
    Jumping = 2,
}

/// AI.EMotionFacing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Facing {
    #[default]
    MovementDirection = 0,
    Rotation = 1,
    EnemyLocation = 2,
    Focus = 3,
}

impl Facing {
    pub fn from_i32(v: i32) -> Facing {
        match v {
            1 => Facing::Rotation,
            2 => Facing::EnemyLocation,
            3 => Facing::Focus,
            _ => Facing::MovementDirection,
        }
    }
}

/// AI.MotionProfile.
#[derive(Clone, Copy, Debug, Default)]
pub struct MotionProfile {
    pub max_linear_speed: i32,
    pub max_linear_acceleration: i32,
    pub max_angular_speed: i32,
}

/// TnAiLocomotionController.
#[derive(Clone, Debug)]
pub struct Locomotion {
    pub state: LocoState,
    /// EMotionAction: 0 slow down approaching the destination, 1 match the
    /// destination's speed.
    pub action: u8,
    /// EMotionSpeed: 0 slow, 1 medium, 2 fast.
    pub speed: u8,
    pub facing: Facing,
    pub destination: Vec3,
    pub steering_point: Vec3,
    pub normal: Vec3,
    pub range: f32,
    pub destination_speed: f32,
    pub face_focus_range: f32,
    pub avoidance_enabled: bool,
    pub is_taking_off: bool,
    pub take_off_turn_rate: f32,
    pub cos_take_off_activation_angle: f32,
    pub cos_take_off_complete_angle: f32,
    pub slowdown_distance: f32,
    pub slowdown_speed_multiplier: f32,
    pub walk_off_ledge_disabled: bool,
    pub idle_max_angular_speed: i32,
    pub num_frames_stuck: u32,
    pub prev_location: Vec3,
    pub profiles: [MotionProfile; 3],
    /// The jump blend's landing animation is playing (TnJumpTrajectoryBlend.
    /// _PlayingJumpLanding: anim.rs keeps it).
    pub playing_jump_landing: bool,
    /// The obstacle avoidance's settings (TnObstacleAvoidanceManager).
    pub avoid_search_radius: f32,
    pub avoid_min_separation: f32,
    pub avoid_max_collision_time: f32,
    /// It has no motion controller (its blueprint makes none: the Sniper's, whose
    /// operators drive it themselves), so what would be asked of one does nothing
    /// (UnrealScript's calls on None): its speeds, acceleration and turn rate stay
    /// its own.
    pub none: bool,
}

impl Default for Locomotion {
    fn default() -> Self {
        Locomotion {
            state: LocoState::Stopped,
            action: 0,
            speed: 0,
            facing: Facing::MovementDirection,
            destination: Vec3::ZERO,
            steering_point: Vec3::ZERO,
            normal: Vec3::ZERO,
            range: 0.0,
            destination_speed: 0.0,
            face_focus_range: 1.0e6,
            avoidance_enabled: true,
            is_taking_off: false,
            take_off_turn_rate: 0.0,
            cos_take_off_activation_angle: 1.0,
            cos_take_off_complete_angle: 1.0,
            slowdown_distance: 0.0,
            slowdown_speed_multiplier: 0.0,
            walk_off_ledge_disabled: false,
            idle_max_angular_speed: 0,
            num_frames_stuck: 0,
            prev_location: Vec3::ZERO,
            profiles: [MotionProfile::default(); 3],
            playing_jump_landing: false,
            avoid_search_radius: 5000.0,
            avoid_min_separation: 50.0,
            avoid_max_collision_time: 0.75,
            none: false,
        }
    }
}

impl Locomotion {
    /// AI.MakeMotionProfile's three for a type: SetProfile.
    pub fn set_profile(&mut self, speed: usize, p: MotionProfile) {
        if speed < 3 {
            self.profiles[speed] = p;
        }
    }

    fn profile(&self) -> MotionProfile {
        self.profiles[(self.speed as usize).min(2)]
    }

    pub fn stopped(&self) -> bool {
        self.state == LocoState::Stopped
    }

    pub fn is_jumping(&self) -> bool {
        self.state == LocoState::Jumping
    }

    /// get_IsInMotion.
    pub fn is_in_motion(&self) -> bool {
        self.state != LocoState::Stopped && !self.playing_jump_landing
    }

    /// get_IsTurning (the locomotion controller has none: the pawn turns as it
    /// goes).
    pub fn is_turning(&self) -> bool {
        false
    }
}

/// ApplyMotionProperties (none without a motion controller: Locomotion's `none`).
fn apply_motion_properties(p: &mut Pawn) {
    if p.motion.none {
        return;
    }
    let prof = p.motion.profile();
    p.ground_speed = prof.max_linear_speed as f32;
    p.accel_rate = prof.max_linear_acceleration as f32;
    p.rotation_rate = prof.max_angular_speed as f32 * ue::ROT_PER_DEG;
}

/// get_ReachedDestination (a plane form's hover motion controller's own: a
/// drone's, a jet's while it is a plane).
pub fn reached_destination(p: &Pawn) -> bool {
    if let Some(f) = p.active_plane() {
        return f.reached(p.loc);
    }
    let m = &p.motion;
    if m.normal.length_squared() == 0.0 {
        return true;
    }
    ue::dot2d(p.loc - m.destination, m.normal) > -m.range && p.physics == Physics::Walking
}

/// set_Speed.
pub fn set_speed(p: &mut Pawn, speed: u8) {
    if let Some(f) = p.active_plane_mut() {
        f.set_speed(speed);
        return;
    }
    p.motion.speed = speed;
    apply_motion_properties(p);
}

/// set_DestinationSpeedMultiplier.
pub fn set_destination_speed_multiplier(p: &mut Pawn, value: f32) {
    p.motion.destination_speed =
        value * p.motion.profiles[(p.motion.speed as usize).min(2)].max_linear_speed as f32;
}

/// MoveToPoint.
pub fn move_to_point(p: &mut Pawn, point: Vec3, action: u8, range: f32) {
    if p.motion.none {
        return;
    }
    let loc = p.loc;
    if let Some(f) = p.active_plane_mut() {
        f.move_to_point(loc, point, range);
        return;
    }
    p.motion.destination = point;
    p.motion.state = LocoState::Walking;
    p.motion.action = action;
    p.motion.range = range;
    p.motion.normal = ue::normal2d(point - p.loc);
    apply_motion_properties(p);
    p.motion.is_taking_off = should_take_off(p);
    if p.motion.is_taking_off {
        p.rotation_rate = p.motion.take_off_turn_rate * ue::ROT_PER_DEG;
        p.desired_yaw = ue::yaw_of(p.motion.normal);
        p.accel = Vec3::ZERO;
    }
}

/// ShouldTakeOff.
fn should_take_off(p: &Pawn) -> bool {
    let m = &p.motion;
    if m.take_off_turn_rate <= 0.0 {
        return false;
    }
    if p.vel.length_squared() > 0.0
        && m.normal.dot(ue::normal(p.vel)) >= m.cos_take_off_activation_angle
    {
        return false;
    }
    if m.normal.dot(p.forward()) >= m.cos_take_off_activation_angle {
        return false;
    }
    true
}

/// Jump: the jump to `landing` (a path node's place: its height over the floor
/// taken off, the pawn's half height put on), peaking 150 uu (500 for a high
/// jump) over its higher end.
pub fn jump(p: &mut Pawn, landing: Vec3, range: f32, high: bool) {
    let mut landing = landing;
    landing.z -= NAV_POINT_HEIGHT;
    landing.z += p.half_height;
    p.motion.destination = landing;
    let peak = if high { HIGH_JUMP_PEAK } else { JUMP_PEAK };
    let (vel, time) = match ue::jump_velocity(p.loc, landing, peak, GRAVITY_Z) {
        Some((v, t, _)) => (v, t),
        None => (Vec3::ZERO, 0.0),
    };
    // PlayJumpBlend: the jump's horizontal and vertical reach for the animation.
    let d = landing - p.loc;
    p.jump = Some(JumpInfo {
        x: ue::vsize2d(d),
        y: d.z,
        time,
        elapsed: 0.0,
    });
    p.jumps = p.jumps.wrapping_add(1);
    // The form's Jump: off the ground with the velocity.
    p.form_jump(vel);
    p.accel = Vec3::ZERO;
    apply_motion_properties(p);
    p.motion.state = LocoState::Jumping;
    p.motion.range = range;
    p.motion.normal = ue::normal2d(landing - p.loc);
}

/// Stop.
pub fn stop(p: &mut Pawn) {
    if p.motion.none {
        return;
    }
    let (loc, vel) = (p.loc, p.vel);
    if let Some(f) = p.active_plane_mut() {
        f.stop(loc, vel);
        return;
    }
    p.accel = Vec3::ZERO;
    apply_motion_properties(p);
    if p.motion.idle_max_angular_speed > 0 {
        p.rotation_rate = p.motion.idle_max_angular_speed as f32 * ue::ROT_PER_DEG;
    }
    p.motion.state = LocoState::Stopped;
}

/// The brain's motion update (TnHtnAiController.Tick's MotionController.Update).
pub fn update(brain: &mut Brain, ctx: &mut super::brain::Ctx) {
    if ctx.pawn.active_plane().is_some() {
        super::drones::motion_update(brain, ctx.pawn, ctx.view);
        return;
    }
    let enemy = brain.core.enemy;
    let walk_off = locomotion_update(ctx.pawn, ctx.view, enemy);
    if let Some(off) = walk_off {
        // Behaviour flag 64 holds the pawn back from ledges while it lands.
        if off {
            brain.add_behavior(ctx, super::brain::BEHAVIOR_LEDGE_FALL);
        } else {
            brain.remove_behavior(ctx, super::brain::BEHAVIOR_LEDGE_FALL);
        }
    }
}

/// TnAiLocomotionController.Update; a change to the ledge behaviour flag for the
/// controller to make (Some(true) to add it, Some(false) to remove it).
fn locomotion_update(p: &mut Pawn, view: &WorldView, enemy: Option<Entity>) -> Option<bool> {
    if p.motion.state == LocoState::Stopped {
        return None;
    }
    let mut ledge = None;
    if p.motion.playing_jump_landing {
        p.desired_yaw = p.yaw;
        p.motion.walk_off_ledge_disabled = true;
        return Some(true);
    }
    if p.motion.walk_off_ledge_disabled {
        p.motion.walk_off_ledge_disabled = false;
        ledge = Some(false);
    }
    if p.motion.is_taking_off {
        p.motion.is_taking_off =
            p.motion.normal.dot(p.forward()) < p.motion.cos_take_off_complete_angle;
        return ledge;
    }
    update_steering_point(p, view, enemy);
    if p.motion.state == LocoState::Walking {
        update_walking(p);
    }
    if p.motion.facing == Facing::MovementDirection && p.vel != Vec3::ZERO {
        p.desired_yaw = ue::yaw_of(p.vel);
    }
    if p.motion.facing == Facing::Focus && p.focus != Vec3::ZERO {
        let dir = p.focus - p.loc;
        if ue::vsize_sq2d(dir) < p.motion.face_focus_range * p.motion.face_focus_range {
            p.desired_yaw = ue::yaw_of(dir);
        } else if p.vel != Vec3::ZERO {
            p.desired_yaw = ue::yaw_of(p.vel);
        }
    }
    ledge
}

/// UpdateWalking.
fn update_walking(p: &mut Pawn) {
    p.accel = ue::normal((p.motion.steering_point - p.loc) * Vec3::new(1.0, 1.0, 0.0));
    check_stuck(p);
    match p.motion.action {
        0 => slow_down_approaching_destination(p),
        1 => match_destination_speed(p),
        _ => {}
    }
}

/// CheckStuck: thirty frames within 20 uu of where it was, and it stops.
fn check_stuck(p: &mut Pawn) {
    if (p.motion.prev_location - p.loc).length_squared() < 20.0 * 20.0 {
        p.motion.num_frames_stuck += 1;
        if p.motion.num_frames_stuck > 30 {
            p.motion.state = LocoState::Stopped;
        }
    } else {
        p.motion.prev_location = p.loc;
        p.motion.num_frames_stuck = 0;
    }
}

/// SlowDownApproachingDestination.
fn slow_down_approaching_destination(p: &mut Pawn) {
    let prof = p.motion.profile();
    let max_speed = prof.max_linear_speed as f32;
    let mut slow_down_distance = if p.motion.slowdown_distance > 0.0 {
        p.motion.slowdown_distance
    } else {
        max_speed * max_speed / (2 * prof.max_linear_acceleration.max(1)) as f32
    };
    slow_down_distance += p.motion.range;
    let distance = ue::vsize2d(p.motion.destination - p.loc);
    if distance > slow_down_distance {
        return;
    }
    let percent = ue::clamp_pct_by_range(distance, p.motion.range, slow_down_distance);
    p.ground_speed = ue::lerp(
        p.motion.slowdown_speed_multiplier * max_speed,
        max_speed,
        percent,
    );
}

/// MatchDestinationSpeed.
fn match_destination_speed(p: &mut Pawn) {
    if p.motion.destination_speed <= 0.0 {
        return;
    }
    let distance = ue::vsize2d(p.motion.destination - p.loc);
    if distance > 2000.0 {
        return;
    }
    let max_destination_speed = p
        .motion
        .destination_speed
        .min(p.motion.profiles[2].max_linear_speed as f32 * 1.2);
    let max = p.motion.profile().max_linear_speed as f32;
    p.ground_speed = ue::lerp(max_destination_speed, max, distance / (2000.0 * 2.0));
}

/// UpdateSteeringPoint: the destination, or a way round the pawn it would run
/// into.
fn update_steering_point(p: &mut Pawn, view: &WorldView, enemy: Option<Entity>) {
    let mut steer = p.motion.destination;
    let m = &p.motion;
    let avoid = Avoid {
        search_radius: m.avoid_search_radius,
        min_separation: m.avoid_min_separation,
        max_collision_time: m.avoid_max_collision_time,
    };
    if p.motion.avoidance_enabled
        && let Some(safe) = should_avoid(p, view, p.motion.destination, enemy, true, &avoid)
    {
        steer = safe;
    }
    p.motion.steering_point = steer;
}

/// A TnObstacleAvoidanceManager's settings: its _SearchRadius, _MinSeparation and
/// _MaxCollisionTime.
pub struct Avoid {
    pub search_radius: f32,
    pub min_separation: f32,
    pub max_collision_time: f32,
}

/// A plane form's motion controller's avoidance (TnAiHoverPlaneMotionController's
/// SetMovementLocation: ShouldAvoid on the way to its destination, stopping there;
/// its manager's defaults but for its MinSeparation): the place to steer for, if
/// it must go round another pawn.
pub fn plane_should_avoid(
    p: &Pawn,
    view: &WorldView,
    destination: Vec3,
    min_separation: f32,
) -> Option<Vec3> {
    let avoid = Avoid {
        search_radius: 5000.0,
        min_separation,
        max_collision_time: 0.75,
    };
    should_avoid(p, view, destination, None, true, &avoid)
}

// ---------------------------------------------------------------------------
// Obstacle avoidance
// ---------------------------------------------------------------------------

/// TnObstacleAvoidanceManager.ShouldAvoid (on the navigation mesh): the place to
/// steer for round the pawn it would soonest run into (EAvoid 1), or None (none
/// in its way, 0, or no way round, 2: it keeps on to its destination).
fn should_avoid(
    p: &Pawn,
    view: &WorldView,
    destination: Vec3,
    ignore: Option<Entity>,
    stopping: bool,
    avoid: &Avoid,
) -> Option<Vec3> {
    let (primary, secondary, distance) =
        attempt_avoidance(p, view, destination, ignore, stopping, avoid)?;
    let primary_steer = p.loc + primary * distance;
    if is_reachable(p, view, primary_steer) {
        return Some(primary_steer);
    }
    let secondary_steer = p.loc + secondary * distance;
    if is_reachable(p, view, secondary_steer) {
        return Some(secondary_steer);
    }
    None
}

/// AI.IsReachable: the pawn could walk straight there over the navigation mesh.
pub fn is_reachable(p: &Pawn, view: &WorldView, to: Vec3) -> bool {
    let (Some(nav), Some(cell)) = (view.nav, p.anchor) else {
        return true;
    };
    nav.is_reachable_from_cell(
        cell,
        ue::from_ue(p.loc),
        ue::from_ue(to),
        p.radius / ue::UU_PER_M,
    )
    .is_ok()
}

/// AttemptAvoidance: the two ways round the pawn to avoid (the better first) and
/// how far.
fn attempt_avoidance(
    p: &Pawn,
    view: &WorldView,
    destination: Vec3,
    ignore: Option<Entity>,
    stopping: bool,
    m: &Avoid,
) -> Option<(Vec3, Vec3, f32)> {
    let current_speed = (p.vel.length() as i32).max(1) as f32;
    let desired_velocity = ue::normal2d(destination - p.loc) * current_speed;
    let time_on_target = if stopping {
        ue::vsize2d(destination - p.loc) / current_speed
    } else {
        m.max_collision_time
    };
    let (other_loc, other_vel, other_radius, time) = find_best_pawn_to_avoid(
        p,
        view,
        ignore,
        desired_velocity,
        time_on_target.min(m.max_collision_time),
        m,
    )?;
    let p0 = p.loc * Vec3::new(1.0, 1.0, 0.0);
    let r0 = p.radius + 0.5 * m.min_separation;
    let p1 = (other_loc + other_vel * time) * Vec3::new(1.0, 1.0, 0.0);
    let r1 = other_radius + 0.5 * m.min_separation;
    let (d0, d1) = match find_steering_directions(p0, r0, p1, r1) {
        Some(d) => d,
        None => {
            let d = ue::normal2d(p0 - p1);
            (d, d)
        }
    };
    let distance = (p0 - p1).dot(d0).abs();
    let relative = desired_velocity - other_vel;
    let best_is_d0 = ue::dot2d(d0, relative) > ue::dot2d(d1, relative);
    Some(if best_is_d0 {
        (d0, d1, distance)
    } else {
        (d1, d0, distance)
    })
}

/// FindSteeringDirections: the tangents from p0 to the circle round p1.
fn find_steering_directions(p0: Vec3, r0: f32, p1: Vec3, r1: f32) -> Option<(Vec3, Vec3)> {
    let radius1 = r0 + r1;
    let dist_sq = ue::vsize_sq2d(p1 - p0);
    if dist_sq < radius1 * radius1 {
        return None;
    }
    let radius0 = (dist_sq - radius1 * radius1).sqrt();
    let (t1, t2) = crate::nav::intersect_circles(
        Vec3::new(p0.x, 0.0, p0.y),
        radius0,
        Vec3::new(p1.x, 0.0, p1.y),
        radius1,
    )?;
    let t1 = Vec3::new(t1.x, t1.z, 0.0);
    let t2 = Vec3::new(t2.x, t2.z, 0.0);
    Some((ue::normal2d(t1 - p0), ue::normal2d(t2 - p0)))
}

/// TnObstacleAvoidance.FindBestPawnToAvoid: the pawn within the search radius it
/// would soonest run into within `max_time` (its place, velocity, radius and the
/// time).
fn find_best_pawn_to_avoid(
    p: &Pawn,
    view: &WorldView,
    goal: Option<Entity>,
    desired_velocity: Vec3,
    max_time: f32,
    m: &Avoid,
) -> Option<(Vec3, Vec3, f32, f32)> {
    let mut best: Option<(Vec3, Vec3, f32, f32)> = None;
    let mut best_time = max_time;
    let search_sq = m.search_radius * m.search_radius;
    let mut consider = |e: Entity, loc: Vec3, vel: Vec3, radius: f32| {
        if e == p.entity || Some(e) == goal {
            return;
        }
        if (loc - p.loc).length_squared() > search_sq {
            return;
        }
        let rel_loc = loc - p.loc;
        let rel_vel = vel - desired_velocity;
        let t = next_collision_time(rel_loc, rel_vel, p.radius + radius + m.min_separation);
        if t < 0.0 {
            return;
        }
        if t < best_time {
            best_time = t;
            best = Some((loc, vel, radius, t));
        }
    };
    for a in &view.ais {
        if a.alive {
            consider(a.entity, a.loc, a.vel, a.radius);
        }
    }
    for pl in &view.players {
        if pl.alive {
            consider(pl.entity, pl.loc, pl.vel, pl.radius);
        }
    }
    best
}

/// TnObstacleAvoidance.GetNextCollisionTime.
fn next_collision_time(rel_loc: Vec3, rel_vel: Vec3, combined_radii: f32) -> f32 {
    let a = rel_vel.dot(rel_vel);
    let b = 2.0 * rel_vel.dot(rel_loc);
    let c = rel_loc.dot(rel_loc) - combined_radii * combined_radii;
    // Core.Math.SolveQuadratic.
    if a.abs() < 1e-6 {
        return -1.0;
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return -1.0;
    }
    let s = disc.sqrt();
    let x1 = (-b - s) / (2.0 * a);
    let x2 = (-b + s) / (2.0 * a);
    if x1 < 0.0 && x2 < 0.0 {
        return -1.0;
    }
    if x1.signum() != x2.signum() {
        return 0.0;
    }
    x1.min(x2)
}
