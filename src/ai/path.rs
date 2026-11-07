//! TnRobotPathFollowingSensor: how a ground enemy gets somewhere. Asked to move
//! to a location reference (MoveToLocation), it finds a channel of navigation
//! points to it, builds its portals and pulls the corners from them one by one
//! (nav.rs's ports of PortalBuilder and StringPulling), walking to each with the
//! motion controller (or jumping, at a jump's corner, once it faces the landing),
//! and keeps WS_Navigation (the reference it is going to, and its context:
//! starting, under way, ending, jumping) and WS_Location (where it arrived)
//! for the planner. Its path is found again whenever the destination moves
//! more than 100 uu from where the path ends.

use bevy::prelude::*;

use super::brain::{BEHAVIOR_LEDGE_FALL, Brain, Ctx};
use super::htn;
use super::motion;
use super::pawn::Physics;
use super::ue;
use crate::nav::{Corner, CornerType, Moves, Portal};

/// ERobotMotionState.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MotionState {
    #[default]
    Idle = 0,
    Walking = 1,
    PreparingToJump = 2,
    PreparingToHighJump = 3,
    Jumping = 4,
    ReturningToNavMesh = 5,
}

/// ERobotMoveCompleteStatus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveStatus {
    Succeeded = 0,
    Failed = 1,
    PathBlocked = 2,
    Aborted = 3,
}

/// MoveToParams.
#[derive(Clone, Copy, Debug, Default)]
pub struct MoveParams {
    pub destination: usize,
    pub range: f32,
    pub speed: u8,
    pub look_at: usize,
}

/// TnRobotPathFollowingSensor.
#[derive(Clone, Debug, Default)]
pub struct PathFollowing {
    /// The current move's parameters, and a new move's (kPendingParams) until it
    /// is taken up.
    pub params: MoveParams,
    pub pending: Option<MoveParams>,
    pub state: MotionState,
    /// The path: its channel of points, its portals (the demo's space, metres)
    /// and the corner it heads for.
    pub channel: Vec<u32>,
    pub portals: Vec<Portal>,
    pub start_portal_index: usize,
    pub next_corner: Corner,
    pub path_begin: Vec3,
    pub path_end: Vec3,
    pub started_in_motion: f32,
    pub start_distance: f32,
    pub end_distance: f32,
    pub slow_turn_speed_multiplier: f32,
    pub cos_fast_turn_angle: f32,
    pub cos_slow_turn_angle: f32,
    pub min_jump_velocity_for_fx: f32,
    /// The last move's end (OnRobotMoveComplete: whether it succeeded), for the
    /// operator that asked for it.
    pub completed: Option<bool>,
    /// A jump's launch this frame, for its dust (the jump particle's socket).
    pub jumped_fx: bool,
}

impl PathFollowing {
    pub fn set_start_and_end_distances(&mut self, start: f32, end: f32) {
        self.start_distance = start;
        self.end_distance = end;
    }

    /// SetSlowTurnCondition.
    pub fn set_slow_turn_condition(&mut self, multiplier: f32, angles: (f32, f32)) {
        self.slow_turn_speed_multiplier = multiplier;
        self.cos_fast_turn_angle = (angles.0 * ue::DEG_TO_RAD).cos();
        self.cos_slow_turn_angle = (angles.1 * ue::DEG_TO_RAD).cos();
    }

    /// get_IsMoving.
    pub fn is_moving(b: &Brain) -> bool {
        b.core.ws(htn::WS_NAVIGATION) != 0
    }

    /// get_IsJumping.
    pub fn is_jumping(ctx: &Ctx) -> bool {
        ctx.pawn.motion.is_jumping() && ctx.pawn.is_jumping()
    }

    /// set_LookAtReference.
    pub fn set_look_at_reference(&mut self, r: usize) {
        self.params.look_at = r;
    }

    fn next_corner_is_destination(&self) -> bool {
        !self.portals.is_empty() && self.next_corner.portal == self.portals.len() - 1
    }

    fn next_corner_is_jump(&self) -> bool {
        matches!(
            self.next_corner.kind,
            CornerType::Jump | CornerType::HighJump
        )
    }

    fn next_jump_destination(&self) -> Vec3 {
        self.portals
            .get(self.next_corner.portal + 1)
            .map_or(self.next_corner.at, |p| p.left)
    }

    /// The next corner in Unreal's space.
    fn corner_ue(&self) -> Vec3 {
        ue::to_ue(self.next_corner.at)
    }

    /// MoveToLocation: a new move, taken up on the sensor's next update unless it
    /// may start at once.
    pub fn move_to_location(
        &mut self,
        b: &mut Brain,
        ctx: &mut Ctx,
        destination: usize,
        range: i32,
        speed: u8,
        look_at: usize,
    ) {
        self.pending = Some(MoveParams {
            destination,
            range: range as f32,
            speed,
            look_at,
        });
        b.core.set_ws(htn::WS_NAVIGATION, destination as i32);
        self.try_move_to_location(b, ctx);
    }

    /// ShouldRepath: a pending move is taken up unless it is in the air (or
    /// jumping).
    fn should_repath(&self, ctx: &Ctx) -> bool {
        if self.pending.is_none() {
            return false;
        }
        match ctx.pawn.physics {
            Physics::Falling | Physics::Interpolating | Physics::Flying => return false,
            _ => {}
        }
        self.state != MotionState::Jumping
    }

    /// TryMoveToLocation.
    fn try_move_to_location(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        if !self.should_repath(ctx) {
            return;
        }
        self.params = self.pending.take().unwrap_or_default();
        motion::set_speed(ctx.pawn, self.params.speed);
        ctx.pawn.motion.facing = if self.params.look_at != 0 {
            motion::Facing::Focus
        } else {
            motion::Facing::MovementDirection
        };
        self.started_in_motion = if ctx.pawn.motion.is_in_motion() {
            0.0
        } else {
            1.0
        };
        self.path_begin = ctx.pawn.loc;
        self.path_end = b.core.fetch_ground(self.params.destination);
        self.clear_move_variables();
        let p = &*ctx.pawn;
        let range =
            ue::range_point_to_point(p.loc, p.radius, self.path_end, 0.0, p.half_height, 0.0);
        if range <= self.params.range {
            self.complete_move(b, ctx, MoveStatus::Succeeded);
            return;
        }
        let channel = match (ctx.view.nav, p.anchor) {
            (Some(nav), Some(cell)) => {
                let moves =
                    Moves::ground(p.radius / ue::UU_PER_M, p.half_height * 2.0 / ue::UU_PER_M);
                nav.find_channel(cell, ue::from_ue(self.path_end), &moves)
            }
            _ => None,
        };
        let Some(channel) = channel else {
            self.complete_move(b, ctx, MoveStatus::PathBlocked);
            return;
        };
        self.channel = channel;
        b.core.set_ws(htn::WS_LOCATION, 0);
        ctx.pawn.block_actors = false;
        self.update_returning_to_nav_mesh(b, ctx);
    }

    fn clear_move_variables(&mut self) {
        self.channel.clear();
        self.portals.clear();
        self.start_portal_index = 0;
        self.next_corner = Corner::new(CornerType::Point, 0, Vec3::ZERO);
    }

    /// OnUpdate.
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        self.jumped_fx = false;
        self.try_move_to_location(b, ctx);
        if !Self::is_moving(b) {
            return;
        }
        if self.reached_destination(b, ctx) {
            self.complete_move(b, ctx, MoveStatus::Succeeded);
            return;
        }
        match self.state {
            MotionState::ReturningToNavMesh => {
                self.update_returning_to_nav_mesh(b, ctx);
                self.update_focal_point(b, ctx);
            }
            MotionState::Walking => {
                self.update_walking(b, ctx);
                self.update_focal_point(b, ctx);
            }
            MotionState::PreparingToJump | MotionState::PreparingToHighJump => {
                self.update_preparing_to_jump(b, ctx)
            }
            MotionState::Jumping => self.update_jumping(b, ctx),
            MotionState::Idle => {}
        }
        if !self.path_ends_in_range_of_destination(b, ctx) {
            let p = self.params;
            let speed = ctx.pawn.motion.speed;
            self.move_to_location(b, ctx, p.destination, p.range as i32, speed, p.look_at);
            return;
        }
        self.update_navigation_context(b, ctx);
    }

    /// get_PathEndsInRangeOfDestination: the destination hasn't moved 100 uu from
    /// where the path ends.
    fn path_ends_in_range_of_destination(&self, b: &Brain, ctx: &Ctx) -> bool {
        let dest = b.core.fetch_ground(self.params.destination);
        ue::range_point_to_point(self.path_end, 0.0, dest, 0.0, ctx.pawn.half_height, 0.0) <= 100.0
    }

    /// UpdateNavigationContext: WS_Navigation's context (0 starting, 1 under way,
    /// 2 ending, 3 jumping).
    fn update_navigation_context(&self, b: &mut Brain, ctx: &Ctx) {
        let p = &*ctx.pawn;
        match self.state {
            MotionState::ReturningToNavMesh | MotionState::Walking => {
                let c = if ue::vsize2d(self.path_begin - p.loc)
                    < self.start_distance * self.started_in_motion
                {
                    0
                } else if ue::vsize2d(self.path_end - p.loc) < self.end_distance {
                    2
                } else {
                    1
                };
                b.core.set_ws_ctx(htn::WS_NAVIGATION, c);
            }
            MotionState::PreparingToJump
            | MotionState::PreparingToHighJump
            | MotionState::Jumping => {
                b.core.set_ws_ctx(htn::WS_NAVIGATION, 3);
            }
            MotionState::Idle => {}
        }
    }

    /// UpdateFocalPoint: what it looks at, or 3000 uu on past its next corner.
    fn update_focal_point(&self, b: &Brain, ctx: &mut Ctx) {
        let focal = if self.params.look_at != 0 {
            b.core.fetch(self.params.look_at)
        } else {
            let c = self.corner_ue();
            c + ue::normal((c - ctx.pawn.loc) * Vec3::new(1.0, 1.0, 0.0)) * 3000.0
        };
        ctx.pawn.set_focal_point(focal);
    }

    /// ReachedDestination: within range of it, and a straight walk to it.
    fn reached_destination(&self, b: &Brain, ctx: &Ctx) -> bool {
        let dest = b.core.fetch_ground(self.params.destination);
        let p = &*ctx.pawn;
        if ue::range_point_to_point(p.loc, p.radius, dest, 0.0, p.half_height, 0.0)
            > self.params.range
        {
            return false;
        }
        super::sensors::can_walk_line(p, ctx.view, dest, false).0
    }

    /// MoveOnToNavMesh: true when it is on the mesh; else it steps back on.
    fn move_on_to_nav_mesh(&mut self, ctx: &mut Ctx) -> bool {
        let p = &mut *ctx.pawn;
        let dir = match (ctx.view.nav, p.anchor) {
            (Some(nav), Some(cell)) => {
                let d = nav.safe_direction_back(cell, ue::from_ue(p.loc), p.radius / ue::UU_PER_M);
                ue::dir_to_ue(d)
            }
            _ => Vec3::ZERO,
        };
        if dir == Vec3::ZERO {
            return true;
        }
        let destination = p.loc + dir * p.radius;
        motion::move_to_point(p, destination, 1, 10.0);
        self.state = MotionState::ReturningToNavMesh;
        false
    }

    fn update_returning_to_nav_mesh(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        if self.move_on_to_nav_mesh(ctx) {
            self.move_to_next_corner(b, ctx);
        }
    }

    /// UpdateWalking.
    fn update_walking(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        if ctx.pawn.motion.stopped() {
            self.complete_move(b, ctx, MoveStatus::Failed);
            return;
        }
        if motion::reached_destination(ctx.pawn)
            || !motion::is_reachable(ctx.pawn, ctx.view, self.corner_ue())
        {
            self.move_to_next_corner(b, ctx);
        }
    }

    /// UpdatePreparingToJump: turns to face the landing, then jumps.
    fn update_preparing_to_jump(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let corner = self.corner_ue();
        ctx.pawn.set_focal_point(corner);
        ctx.pawn.set_look_at_point(corner);
        b.add_behavior(ctx, BEHAVIOR_LEDGE_FALL);
        let p = &*ctx.pawn;
        let ready =
            ue::dot2d(p.forward(), ue::normal2d(corner - p.loc)) > 0.939 && !p.motion.is_turning();
        if !ready {
            return;
        }
        b.remove_behavior(ctx, BEHAVIOR_LEDGE_FALL);
        let range = if self.next_corner_is_destination() {
            self.params.range
        } else {
            ctx.pawn.radius
        };
        let high = self.state == MotionState::PreparingToHighJump;
        motion::jump(ctx.pawn, corner, range, high);
        self.state = MotionState::Jumping;
        let v = ctx.pawn.vel.length_squared();
        if self.min_jump_velocity_for_fx > 0.0
            && v > self.min_jump_velocity_for_fx * self.min_jump_velocity_for_fx
        {
            self.jumped_fx = true;
        }
    }

    /// UpdateJumping.
    fn update_jumping(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        if motion::reached_destination(ctx.pawn) {
            self.move_to_next_corner(b, ctx);
        } else if ctx.pawn.physics == Physics::Walking {
            self.complete_move(b, ctx, MoveStatus::Failed);
        }
    }

    /// MoveToNextCorner.
    fn move_to_next_corner(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let mut ct = CornerType::None;
        if self.portals.is_empty()
            && let Some(nav) = ctx.view.nav
        {
            self.portals = nav.build_portals(
                ue::from_ue(ctx.pawn.loc),
                ue::from_ue(self.path_end),
                &self.channel,
            );
        }
        if motion::reached_destination(ctx.pawn) {
            ct = self.next_corner.kind;
            self.start_portal_index = self.next_corner.portal;
        }
        if !self.find_next_corner(b, ctx) {
            return;
        }
        match ct {
            CornerType::Jump | CornerType::HighJump => {
                self.jump_to_next_corner(ctx, ct == CornerType::HighJump)
            }
            _ => self.walk_to_next_corner(ctx),
        }
    }

    /// FindNextCorner: the next corner (the destination moved back off the
    /// mesh's border if it has to be); false if there is none.
    fn find_next_corner(&mut self, b: &mut Brain, ctx: &mut Ctx) -> bool {
        let here = ue::from_ue(ctx.pawn.loc);
        let start = Corner::new(CornerType::Point, self.start_portal_index, here);
        self.next_corner =
            crate::nav::funnel_with_radius(start, ctx.pawn.radius / ue::UU_PER_M, &self.portals);
        let dest_ref = self.params.destination;
        let enemyish = matches!(
            dest_ref,
            super::brain::LR_NAVIGABLE_LOCATION_NEAR_ENEMY
                | super::brain::LR_SUSPECTED_ENEMY
                | super::brain::LR_CONFIRMED_ENEMY
        );
        if self.next_corner.at == here {
            let status = if enemyish {
                MoveStatus::PathBlocked
            } else {
                MoveStatus::Failed
            };
            self.complete_move(b, ctx, status);
            return false;
        }
        if self.next_corner_is_destination() {
            let dest = b.core.fetch_ground(dest_ref);
            let reachable = match (ctx.view.nav, ctx.pawn.anchor) {
                (Some(nav), Some(cell)) => nav
                    .reachable_destination(
                        cell,
                        here,
                        ue::from_ue(dest),
                        ctx.pawn.radius / ue::UU_PER_M,
                    )
                    .map(ue::to_ue),
                _ => Some(dest),
            };
            match reachable {
                None => {
                    let status = if matches!(
                        dest_ref,
                        super::brain::LR_NAVIGABLE_LOCATION_NEAR_ENEMY
                            | super::brain::LR_SUSPECTED_ENEMY
                    ) {
                        MoveStatus::PathBlocked
                    } else {
                        MoveStatus::Failed
                    };
                    self.complete_move(b, ctx, status);
                    return false;
                }
                Some(r) => self.next_corner.at = ue::from_ue(r),
            }
        }
        true
    }

    /// WalkToNextCorner.
    fn walk_to_next_corner(&mut self, ctx: &mut Ctx) {
        let action = self.next_motion_action(ctx);
        let range = if self.next_corner_is_destination() {
            self.params.range
        } else {
            ctx.pawn.radius
        };
        let corner = self.corner_ue();
        motion::move_to_point(ctx.pawn, corner, action, range);
        ctx.pawn.motion.facing = if self.params.look_at != 0 {
            motion::Facing::Focus
        } else {
            motion::Facing::MovementDirection
        };
        if action == 1 {
            self.set_destination_speed(ctx);
        }
        self.state = MotionState::Walking;
    }

    /// SetDestinationSpeed: a jump's corner wants the jump's speed across, any
    /// other the speed its turn allows.
    fn set_destination_speed(&self, ctx: &mut Ctx) {
        if self.next_corner_is_jump() {
            let from = self.corner_ue();
            let to = ue::to_ue(self.next_jump_destination());
            if let Some((_, _, horizontal)) =
                ue::jump_velocity(from, to, motion::JUMP_PEAK, super::pawn::GRAVITY_Z)
            {
                ctx.pawn.motion.destination_speed = horizontal;
            }
        } else {
            let m = self.destination_speed_multiplier_from_corner(ctx);
            motion::set_destination_speed_multiplier(ctx.pawn, m);
        }
    }

    /// GetDestinationSpeedMultiplierFromCorner.
    fn destination_speed_multiplier_from_corner(&self, ctx: &Ctx) -> f32 {
        if self.slow_turn_speed_multiplier <= 0.0 {
            return 0.0;
        }
        let next_next = crate::nav::funnel_with_radius(
            self.next_corner,
            ctx.pawn.radius / ue::UU_PER_M,
            &self.portals,
        );
        if next_next.at == self.next_corner.at {
            return 0.0;
        }
        let cos =
            cos_angle_between_corners(ue::from_ue(ctx.pawn.loc), self.next_corner.at, next_next.at);
        let pct =
            1.0 - ue::clamp_pct_by_range(cos, self.cos_fast_turn_angle, self.cos_slow_turn_angle);
        ue::lerp(self.slow_turn_speed_multiplier, 1.0, pct)
    }

    /// GetNextMotionAction: slow to a stop at the destination or before a sharp
    /// turn into a jump; else keep the speed the next corner wants.
    fn next_motion_action(&self, ctx: &Ctx) -> u8 {
        if self.next_corner_is_destination() {
            return 0;
        }
        if self.next_corner_is_jump()
            && cos_angle_between_corners(
                ue::from_ue(ctx.pawn.loc),
                self.next_corner.at,
                self.next_jump_destination(),
            ) <= 0.707
        {
            return 0;
        }
        1
    }

    /// JumpToNextCorner.
    fn jump_to_next_corner(&mut self, ctx: &mut Ctx, high: bool) {
        let corner = self.corner_ue();
        ctx.pawn.set_focal_point(corner);
        self.state = if high {
            MotionState::PreparingToHighJump
        } else {
            MotionState::PreparingToJump
        };
    }

    /// AbortMove.
    pub fn abort_move(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        if self.state != MotionState::Idle {
            self.complete_move(b, ctx, MoveStatus::Aborted);
        }
    }

    /// CompleteMove.
    pub fn complete_move(&mut self, b: &mut Brain, ctx: &mut Ctx, status: MoveStatus) {
        if status == MoveStatus::PathBlocked {
            let dest = self.params.destination;
            super::sensors::with_sensor!(b, pathing, s => s.path_blocked(b, ctx.pawn, dest));
        }
        motion::stop(ctx.pawn);
        ctx.pawn.block_actors = true;
        let location = if status == MoveStatus::Succeeded {
            self.params.destination as i32
        } else {
            0
        };
        b.core.set_ws(htn::WS_LOCATION, location);
        b.core.set_ws(htn::WS_NAVIGATION, 0);
        self.state = MotionState::Idle;
        self.completed = Some(status == MoveStatus::Succeeded);
    }
}

/// GetCosAngleBetweenCorners (the demo's space: x and z the ground plane).
fn cos_angle_between_corners(pawn: Vec3, next: Vec3, next_next: Vec3) -> f32 {
    crate::nav::dot2d(
        crate::nav::normal2d(next_next - next),
        crate::nav::normal2d(next - pawn),
    )
}
