//! What the AI knows of the world each frame (UE3's actors as the script reads
//! them): the players (their pawns), the other AI pawns, the navigation and the
//! level's collision, in Unreal's space.

use bevy::prelude::*;

use crate::nav::NavMesh;

use super::ue;

/// A player's pawn as the AI sees it (Unreal space).
#[derive(Clone, Debug)]
pub struct PlayerView {
    pub entity: Entity,
    /// The collision cylinder's middle (Pawn.Location) and its feet
    /// (GetLocationAtFeet).
    pub loc: Vec3,
    pub feet: Vec3,
    pub vel: Vec3,
    /// Its facing (Rotation.Yaw).
    pub yaw: f32,
    pub radius: f32,
    pub half_height: f32,
    pub health: f32,
    pub health_max: f32,
    /// In the match and not dead (a downed player is alive).
    pub alive: bool,
    pub downed: bool,
    pub cloaked: bool,
    /// Its form (AI.EEnemyForm: 1 robot, 2 flying vehicle, 3 ground vehicle).
    pub form: i32,
    /// Off the ground (IsJumping, falling, a jet flying).
    pub airborne: bool,
    pub jumping: bool,
    /// Physics as UE3 numbers it (1 walking, 2 falling, 4 flying).
    pub physics: i32,
    pub transforming: bool,
    /// Its class ability's shield up (the soldiers' EscalationAgroAbilities).
    pub shielded: bool,
    /// Its eyes (GetViewLocation).
    pub eyes: Vec3,
    /// The last place on the navigation mesh near it (LastNavigableNearbyLocation:
    /// its feet), and when it was last there (the AI's clock).
    pub nav_near: Vec3,
    pub nav_near_time: f32,
    pub team: u8,
    /// Its abilities are out of use (an EMP: get_CanUseAbilities false).
    pub weakened: bool,
    /// The way he dodges while he does (his TnDodgeService's get_IsDodging and
    /// get_DodgeDirectionVector: a dash), Unreal space.
    pub dodge: Option<Vec3>,
    /// What an instant hit's trace strikes of him: his robot's physics asset's
    /// shapes where his bones are this frame, or his vehicle's box (the demo's
    /// space, metres). TnPawnCollisionController lets such traces through his
    /// collision cylinder (SetTraceBlocking(false, true)) and has his robot's mesh
    /// (in his vehicle, its mesh) take them; none where his bones aren't known yet,
    /// when the cylinder stands in.
    pub body: Option<std::sync::Arc<[crate::ragdoll::Shape]>>,
}

/// Another AI pawn as the AIs see each other.
#[derive(Clone, Debug)]
pub struct AiView {
    pub entity: Entity,
    pub loc: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub radius: f32,
    pub half_height: f32,
    pub alive: bool,
    pub team: u8,
    /// Who it is after (Controller.Enemy).
    pub enemy: Option<Entity>,
    pub kind: usize,
    /// It blocks other pawns (bBlockActors: off while it follows a path).
    pub blocks: bool,
    /// Its anchor cell's place under it (its LastNavigableNearbyLocation).
    pub nav_near: Vec3,
    /// A Guard in battle form, alerted to the enemy, its search and unreachable
    /// counters clear (TnGuardSensor.ShouldRaiseSuspicion of its controller).
    pub raises_suspicion: bool,
    /// The pawn it defends (its TnDefendActorDataStore's: a drone's Titan).
    pub defend: Option<Entity>,
}

/// An actor the AI's script asks about (a player's pawn or another AI's): what
/// the sensors read of it.
#[derive(Clone, Copy, Debug)]
pub struct ActorInfo {
    pub entity: Entity,
    pub loc: Vec3,
    pub feet: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    /// LastNavigableNearbyLocation.
    pub nav_near: Vec3,
    pub radius: f32,
    pub half_height: f32,
    pub team: u8,
    pub is_player: bool,
    /// IsDead() or IsDying(): no more a threat.
    pub dead: bool,
}

/// The players' team (the AI is team 1 in Escalation's single-team game).
pub const PLAYER_TEAM: u8 = 0;
pub const AI_TEAM: u8 = 1;

/// The frame's view.
#[derive(Default)]
pub struct WorldView<'a> {
    pub now: f32,
    pub players: Vec<PlayerView>,
    pub ais: Vec<AiView>,
    pub nav: Option<&'a NavMesh>,
    pub level: Option<&'a crate::Level>,
    /// The players' Diffraction Barriers (equipment.rs), each with its boxes (the
    /// demo's axes): world geometry to the enemies' rounds.
    pub barriers: Vec<(Entity, [crate::equipment::BarrierBox; 10])>,
}

impl WorldView<'_> {
    pub fn player(&self, e: Entity) -> Option<&PlayerView> {
        self.players.iter().find(|p| p.entity == e)
    }

    pub fn ai(&self, e: Entity) -> Option<&AiView> {
        self.ais.iter().find(|a| a.entity == e)
    }

    /// The actor `e` (a player's pawn or an AI's), if it is still about.
    pub fn actor(&self, e: Entity) -> Option<ActorInfo> {
        if let Some(p) = self.player(e) {
            return Some(ActorInfo {
                entity: e,
                loc: p.loc,
                feet: p.feet,
                vel: p.vel,
                yaw: p.yaw,
                nav_near: p.nav_near,
                radius: p.radius,
                half_height: p.half_height,
                team: p.team,
                is_player: true,
                dead: !p.alive,
            });
        }
        self.ai(e).map(|a| ActorInfo {
            entity: e,
            loc: a.loc,
            feet: a.loc - Vec3::Z * a.half_height,
            vel: a.vel,
            yaw: a.yaw,
            nav_near: a.nav_near,
            radius: a.radius,
            half_height: a.half_height,
            team: a.team,
            is_player: false,
            dead: !a.alive,
        })
    }

    /// Pawn.OnSameTeamAs for two actors (an unknown one is on no team).
    pub fn same_team(&self, a: Entity, b: Entity) -> bool {
        match (self.actor(a), self.actor(b)) {
            (Some(x), Some(y)) => x.team == y.team,
            _ => false,
        }
    }

    /// FastTrace: nothing of the level's between two points (Unreal space).
    pub fn fast_trace(&self, from: Vec3, to: Vec3) -> bool {
        self.trace_time(from, to) >= 1.0
    }

    /// The first thing of the level's from `from` to `to`: where (Unreal space)
    /// and its normal.
    pub fn trace(&self, from: Vec3, to: Vec3) -> Option<(Vec3, Vec3)> {
        let level = self.level?;
        let a = ue::from_ue(from);
        let b = ue::from_ue(to);
        let d = b - a;
        let len = d.length();
        if len < 1e-4 {
            return None;
        }
        let n = d / len;
        level
            .ray_hit(a, n, len)
            .map(|(t, normal)| (ue::to_ue(a + n * t), ue::dir_to_ue(normal)))
    }

    /// The first of the players' barriers a round from `from` along unit `dir`
    /// strikes within `max` (Unreal space): how far along, its face's normal and
    /// which barrier.
    pub fn barrier_hit(&self, from: Vec3, dir: Vec3, max: f32) -> Option<(f32, Vec3, Entity)> {
        if self.barriers.is_empty() {
            return None;
        }
        let a = ue::from_ue(from);
        let d = ue::dir_from_ue(dir).normalize_or_zero();
        crate::equipment::barrier_hit(&self.barriers, a, d, max / ue::UU_PER_M)
            .map(|(t, n, e)| (t * ue::UU_PER_M, ue::dir_to_ue(n), e))
    }

    /// (The port's) Whether a flier of `radius` (uu) has room along a straight
    /// flight from `from` to `to` (Unreal space) against the level's own solid
    /// surfaces, as far as rays along it tell (the air network's flights are
    /// measured the same way: NavMesh::measure_flights).
    pub fn flight_fits(&self, from: Vec3, to: Vec3, radius: f32) -> bool {
        let Some(c) = self.level.and_then(|l| l.collision.as_ref()) else {
            return true;
        };
        c.own_tube_clear(
            ue::from_ue(from).to_array(),
            ue::from_ue(to).to_array(),
            radius / ue::UU_PER_M,
        )
    }

    /// A trace's HitInfo.Time: how far along it the level is struck (1 for nothing).
    pub fn trace_time(&self, from: Vec3, to: Vec3) -> f32 {
        let Some(level) = self.level else {
            return 1.0;
        };
        let a = ue::from_ue(from);
        let b = ue::from_ue(to);
        let d = b - a;
        let len = d.length();
        if len < 1e-4 {
            return 1.0;
        }
        match level.ray_hit(a, d / len, len) {
            Some((t, _)) => (t / len).clamp(0.0, 1.0),
            None => 1.0,
        }
    }

    /// One of the rays of a trace with an extent (a box swept, for one of the AI's
    /// pawns): how far along the level stops it (1 for nothing). The blocking
    /// volumes stop it too (BlockNonZeroExtent), the players-only ones don't.
    pub fn sweep_time(&self, from: Vec3, to: Vec3) -> f32 {
        let Some(level) = self.level else {
            return 1.0;
        };
        let a = ue::from_ue(from);
        let d = ue::from_ue(to) - a;
        let len = d.length();
        if len < 1e-4 {
            return 1.0;
        }
        match level.ray_hit_for(a, d / len, len, crate::collision::Mover::Ai) {
            Some((t, _)) => (t / len).clamp(0.0, 1.0),
            None => 1.0,
        }
    }

    /// A cylinder of `radius` swept from `from` to `to` (a Trace with an extent):
    /// whether the level stops it (rays at its middle and either side; with no
    /// extent, a line's).
    pub fn sweep_blocked(&self, from: Vec3, to: Vec3, radius: f32) -> bool {
        if radius <= 0.0 {
            return !self.fast_trace(from, to);
        }
        let d = Vec3::new(to.x - from.x, to.y - from.y, 0.0);
        let side = Vec3::new(-d.y, d.x, 0.0).normalize_or_zero() * radius;
        [Vec3::ZERO, side, -side]
            .iter()
            .any(|o| self.sweep_time(from + *o, to + *o) < 1.0)
    }

    /// The highest floor under a body of `radius` whose feet are at `feet` (Unreal
    /// space), from `up` above them to `down` below: its height (anywhere under its
    /// disc: a pawn's cylinder rests on a ledge's lip).
    pub fn ground_at(&self, feet: Vec3, radius: f32, up: f32, down: f32) -> Option<f32> {
        self.ground_at_by(feet, radius, up, down, crate::collision::Mover::Ai)
    }

    /// The same for `who` (push_out_by).
    pub fn ground_at_by(
        &self,
        feet: Vec3,
        radius: f32,
        up: f32,
        down: f32,
        who: crate::collision::Mover,
    ) -> Option<f32> {
        let Some(level) = self.level else {
            return Some(feet.z);
        };
        let b = ue::from_ue(feet);
        level
            .ground_under_disc(
                b,
                radius / ue::UU_PER_M,
                b.y + up / ue::UU_PER_M,
                b.y - down / ue::UU_PER_M,
                who,
            )
            .map(|y| y * ue::UU_PER_M)
    }

    /// The lowest ceiling over a body of `radius` whose head is at `head`, no more
    /// than `up` over it (Unreal space; an AI's pawn's: the players-only lids let it
    /// by): its height.
    pub fn ceiling_at(&self, head: Vec3, radius: f32, up: f32) -> Option<f32> {
        self.ceiling_at_by(head, radius, up, crate::collision::Mover::Ai)
    }

    /// The same for `who` (push_out_by).
    pub fn ceiling_at_by(
        &self,
        head: Vec3,
        radius: f32,
        up: f32,
        who: crate::collision::Mover,
    ) -> Option<f32> {
        let level = self.level?;
        let b = ue::from_ue(head);
        level
            .ceiling_for(b, radius / ue::UU_PER_M, b.y, b.y + up / ue::UU_PER_M, who)
            .map(|y| y * ue::UU_PER_M)
    }

    /// The push that moves a body of `radius` and `height` standing at `feet` out
    /// of the level's walls (in the ground plane), if it is in one.
    pub fn push_out(&self, feet: Vec3, radius: f32, height: f32) -> Option<Vec3> {
        self.push_out_by(feet, radius, height, crate::collision::Mover::Ai)
    }

    /// The same for `who`: a Sniper flying between its points is pushed out of the
    /// level's solid surfaces alone (Mover::AiFlying: no blocking volume stops it,
    /// pawn.rs's fly).
    pub fn push_out_by(
        &self,
        feet: Vec3,
        radius: f32,
        height: f32,
        who: crate::collision::Mover,
    ) -> Option<Vec3> {
        let level = self.level?;
        let b = ue::from_ue(feet);
        // (The players-only walls let it through: HmPlayerOnlyBlockingVolume's
        // IgnoreBlockingBy, the enemies' spawn walls.)
        level
            .push_out_for(b, radius / ue::UU_PER_M, height / ue::UU_PER_M, who)
            .map(|v| Vec3::new(v.x, v.y, 0.0) * ue::UU_PER_M)
    }

    /// The navigation cell under feet at `feet` (Unreal space): GetBestAnchor.
    pub fn cell_at(&self, feet: Vec3) -> Option<u32> {
        self.nav?.cell_at(ue::from_ue(feet))
    }
}
