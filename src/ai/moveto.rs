//! TnMoveToOperator: a move the operator runs itself (the melee kinds' and the
//! Leaper's), as the robots' path following sensor does for theirs (path.rs):
//! its channel of navigation points to the destination, its portals and the
//! corners pulled from them one by one, walked to with the motion controller or
//! jumped to once it faces the landing. It fails as the destination moves 200 uu
//! from where its path ends, or turns out to be out of reach; it succeeds within
//! its range of it, with a straight walk there.

use bevy::prelude::*;

use super::brain::{
    BEHAVIOR_AIMER, BEHAVIOR_ALLOW_MOTION, BEHAVIOR_HEAD_TRACKING, BEHAVIOR_ROTATE_TO_FOCUS, Brain,
    Ctx,
};
use super::htn::{self, Status, Task};
use super::motion;
use super::pawn::Physics;
use super::sensors::with_sensor;
use super::ue;
use crate::nav::{Corner, CornerType, Moves, Portal};

/// EMotionState.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Walking = 0,
    PreparingToJump = 1,
    PreparingToHighJump = 2,
    Jumping = 3,
    ReturningToNavMesh = 4,
}

/// The operator's state.
#[derive(Clone, Debug, Default)]
pub struct MoveTo {
    pub destination: usize,
    pub range: f32,
    pub look_at: usize,
    pub stop_at_destination: bool,
    pub state: State,
    pub channel: Vec<u32>,
    pub portals: Vec<Portal>,
    pub start_portal_index: usize,
    pub next_corner: Corner,
    pub path_begin: Vec3,
    pub path_end: Vec3,
    pub reachable: bool,
    pub unreachable_context: &'static str,
    /// In a jump (its controller's and its pawn's): not to be interrupted.
    pub jumping: bool,
}

impl MoveTo {
    fn corner_ue(&self) -> Vec3 {
        ue::to_ue(self.next_corner.at)
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

    /// OnStart.
    pub fn start(&mut self, task: &Task, b: &mut Brain, ctx: &mut Ctx) -> bool {
        motion::set_speed(ctx.pawn, task.int(2) as u8);
        self.destination = task.int(0).max(0) as usize;
        self.range = task.int(1) as f32;
        self.look_at = task.int(3).max(0) as usize;
        ctx.pawn.motion.facing = if self.look_at != 0 {
            motion::Facing::Focus
        } else {
            motion::Facing::MovementDirection
        };
        self.stop_at_destination = true;
        if task.n > 4 {
            self.stop_at_destination = task.float(4) == 1.0;
        }
        self.path_begin = ctx.pawn.loc;
        self.path_end = b.core.fetch_ground(self.destination);
        let p = &*ctx.pawn;
        let channel = match (ctx.view.nav, p.anchor) {
            (Some(nav), Some(cell)) => {
                let moves =
                    Moves::ground(p.radius / ue::UU_PER_M, p.half_height * 2.0 / ue::UU_PER_M);
                nav.find_channel(cell, ue::from_ue(self.path_end), &moves)
            }
            _ => None,
        };
        let Some(channel) = channel else {
            let dest = self.destination;
            with_sensor!(b, pathing, s => s.path_blocked(b, ctx.pawn, dest));
            return false;
        };
        self.channel = channel;
        self.portals.clear();
        self.start_portal_index = 0;
        self.next_corner = Corner::new(CornerType::Point, 0, Vec3::ZERO);
        self.reachable = true;
        if self.move_on_to_nav_mesh(b, ctx) {
            self.move_to_next_corner(b, ctx);
        }
        ctx.pawn.block_actors = false;
        b.core.set_ws(htn::WS_NAVIGATION, self.destination as i32);
        true
    }

    /// OnUpdate.
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) -> Status {
        self.jumping = ctx.pawn.motion.is_jumping() && ctx.pawn.is_jumping();
        self.set_look_at_point(b, ctx);
        if self.state != State::Jumping {
            if !self.path_ends_in_range_of_destination(b, ctx) {
                return Status::Failed;
            }
            if !self.reachable {
                return Status::Failed;
            }
        }
        if self.reached_destination(b, ctx) {
            return Status::Succeeded;
        }
        match self.state {
            State::ReturningToNavMesh => {
                if self.move_on_to_nav_mesh(b, ctx) {
                    self.move_to_next_corner(b, ctx);
                }
            }
            State::Walking => {
                if motion::reached_destination(ctx.pawn)
                    || !motion::is_reachable(ctx.pawn, ctx.view, self.corner_ue())
                {
                    self.move_to_next_corner(b, ctx);
                }
            }
            State::PreparingToJump | State::PreparingToHighJump => {
                self.update_preparing_to_jump(b, ctx)
            }
            State::Jumping => {
                let reached = motion::reached_destination(ctx.pawn);
                if reached {
                    self.move_to_next_corner(b, ctx);
                }
                if !reached && ctx.pawn.physics == Physics::Walking {
                    self.unreachable("Jump");
                    // (Down short of its landing it walks again, so its next update
                    // fails the move. The game's operator stays Jumping, which
                    // skips that check, and its move runs on where it stands for
                    // good: the enemy freezes there.)
                    self.state = State::Walking;
                }
            }
        }
        Status::Running
    }

    /// OnFinish.
    pub fn finish(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        ctx.pawn.block_actors = true;
        b.core.set_ws(htn::WS_NAVIGATION, 0);
    }

    fn unreachable(&mut self, context: &'static str) {
        self.reachable = false;
        self.unreachable_context = context;
    }

    /// get_PathEndsInRangeOfDestination: within 200 uu of where the path ends.
    fn path_ends_in_range_of_destination(&self, b: &Brain, ctx: &Ctx) -> bool {
        let dest = b.core.fetch_ground(self.destination);
        ue::range_point_to_point(self.path_end, 0.0, dest, 0.0, ctx.pawn.half_height, 0.0) <= 200.0
    }

    /// ReachedDestination: within its range across the ground, and a straight
    /// walk to it.
    fn reached_destination(&self, b: &Brain, ctx: &Ctx) -> bool {
        let dest = b.core.fetch_ground(self.destination);
        if ue::vsize_sq2d(dest - ctx.pawn.loc) > self.range * self.range {
            return false;
        }
        super::sensors::can_walk_line(ctx.pawn, ctx.view, dest, false).0
    }

    /// MoveOnToNavMesh: true when it is on the mesh; else it steps back on.
    fn move_on_to_nav_mesh(&mut self, b: &mut Brain, ctx: &mut Ctx) -> bool {
        let p = &*ctx.pawn;
        let dir = match (ctx.view.nav, p.anchor) {
            (Some(nav), Some(cell)) => ue::dir_to_ue(nav.safe_direction_back(
                cell,
                ue::from_ue(p.loc),
                p.radius / ue::UU_PER_M,
            )),
            _ => Vec3::ZERO,
        };
        if dir == Vec3::ZERO {
            return true;
        }
        let destination = p.loc + dir * p.radius;
        motion::move_to_point(ctx.pawn, destination, 1, 10.0);
        b.set_behavior(
            ctx,
            BEHAVIOR_HEAD_TRACKING | BEHAVIOR_AIMER | BEHAVIOR_ALLOW_MOTION,
            false,
        );
        self.state = State::ReturningToNavMesh;
        false
    }

    /// UpdatePreparingToJump: turns to face the landing, then jumps.
    fn update_preparing_to_jump(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let corner = self.corner_ue();
        let p = &*ctx.pawn;
        let ready =
            ue::dot2d(p.forward(), ue::normal2d(corner - p.loc)) > 0.984 && !p.motion.is_turning();
        if !ready {
            return;
        }
        let range = if self.next_corner_is_destination() {
            self.range
        } else {
            ctx.pawn.radius
        };
        b.set_behavior(
            ctx,
            BEHAVIOR_HEAD_TRACKING | BEHAVIOR_AIMER | BEHAVIOR_ALLOW_MOTION,
            false,
        );
        let high = self.state == State::PreparingToHighJump;
        motion::jump(ctx.pawn, corner, range, high);
        self.state = State::Jumping;
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
        self.find_next_corner(b, ctx);
        match ct {
            CornerType::Jump | CornerType::HighJump => {
                // JumpToNextCorner.
                b.set_behavior(ctx, BEHAVIOR_ROTATE_TO_FOCUS, false);
                ctx.pawn.set_focal_point(self.corner_ue());
                self.state = if ct == CornerType::HighJump {
                    State::PreparingToHighJump
                } else {
                    State::PreparingToJump
                };
            }
            _ => self.walk_to_next_corner(b, ctx),
        }
    }

    /// FindNextCorner.
    fn find_next_corner(&mut self, b: &Brain, ctx: &Ctx) {
        let here = ue::from_ue(ctx.pawn.loc);
        let start = Corner::new(CornerType::Point, self.start_portal_index, here);
        self.next_corner =
            crate::nav::funnel_with_radius(start, ctx.pawn.radius / ue::UU_PER_M, &self.portals);
        if !self.next_corner_is_destination() && self.next_corner.at == here {
            self.unreachable("Funnel");
            return;
        }
        if self.next_corner_is_destination() {
            let dest = b.core.fetch_ground(self.destination);
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
                Some(r) => self.next_corner.at = ue::from_ue(r),
                None => self.unreachable("Destination"),
            }
        }
    }

    /// WalkToNextCorner.
    fn walk_to_next_corner(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let action = self.next_motion_action(ctx);
        let range = if self.next_corner_is_destination() {
            self.range
        } else {
            ctx.pawn.radius
        };
        let corner = self.corner_ue();
        motion::move_to_point(ctx.pawn, corner, action, range);
        ctx.pawn.motion.facing = if self.look_at != 0 {
            motion::Facing::Focus
        } else {
            motion::Facing::MovementDirection
        };
        b.set_behavior(
            ctx,
            BEHAVIOR_HEAD_TRACKING | BEHAVIOR_AIMER | BEHAVIOR_ALLOW_MOTION,
            false,
        );
        self.state = State::Walking;
    }

    /// NextMotionAction: slow to a stop at the destination (if it is to stop
    /// there), and for a jump unless it runs straight into it at the jump's
    /// speed.
    fn next_motion_action(&self, ctx: &mut Ctx) -> u8 {
        let mut action = if self.stop_at_destination && self.next_corner_is_destination() {
            0
        } else {
            1
        };
        ctx.pawn.motion.destination_speed = 0.0;
        if self.next_corner_is_jump() {
            action = 0;
            let jump_to = self
                .portals
                .get(self.next_corner.portal + 1)
                .map(|p| p.left);
            if let Some(to) = jump_to {
                let jump_dir = crate::nav::normal2d(to - self.next_corner.at);
                let run_dir = crate::nav::normal2d(self.next_corner.at - ue::from_ue(ctx.pawn.loc));
                if crate::nav::dot2d(jump_dir, run_dir) > 0.707
                    && let Some((_, _, horizontal)) = ue::jump_velocity(
                        self.corner_ue(),
                        ue::to_ue(to),
                        motion::JUMP_PEAK,
                        super::pawn::GRAVITY_Z,
                    )
                {
                    ctx.pawn.motion.destination_speed = horizontal;
                    action = 1;
                }
            }
        }
        action
    }

    /// SetLookAtPoint: what it is to look at (and face, unless it is jumping),
    /// else 3000 uu on past its next corner's apex.
    fn set_look_at_point(&self, b: &Brain, ctx: &mut Ctx) {
        if self.look_at != 0 {
            let at = b.core.fetch(self.look_at);
            ctx.pawn.set_look_at_point(at);
            if !matches!(
                self.state,
                State::PreparingToJump | State::PreparingToHighJump | State::Jumping
            ) {
                ctx.pawn.set_focal_point(at);
            }
            return;
        }
        let apex = match self.next_corner.kind {
            CornerType::Right => self
                .portals
                .get(self.next_corner.portal)
                .map_or(self.next_corner.at, |p| p.right),
            CornerType::Left => self
                .portals
                .get(self.next_corner.portal)
                .map_or(self.next_corner.at, |p| p.left),
            _ => self.next_corner.at,
        };
        let apex = ue::to_ue(apex);
        let look = apex + ue::normal((apex - ctx.pawn.loc) * Vec3::new(1.0, 1.0, 0.0)) * 3000.0;
        ctx.pawn.set_look_at_point(look);
    }
}
