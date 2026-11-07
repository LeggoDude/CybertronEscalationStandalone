//! What the enemies share: their squad (TnAiSquadManager: the squad actions only
//! so many may claim at once, who is engaging whom, events broadcast to the squad
//! and the swarmers' turns at attacking), the tactical system (TnTacticalSystem:
//! the claims on the tactical nodes' groups, the enemies last seen, each node's
//! line of sight to each team as TnTacticalUpdater keeps it, its stink), and what
//! the brains do to each other and to the world this frame.

use std::collections::HashMap;

use bevy::prelude::*;

use super::brain::{Event, EventId};
use super::ue::{self, Rng};
use super::world::{AI_TEAM, WorldView};
use crate::nav::{Cover, NavMesh};

// ---------------------------------------------------------------------------
// Squad actions
// ---------------------------------------------------------------------------

/// AI.ESquadActionType.
pub const SAT_ADVANCE_ON_SUSPECTED_ENEMY: usize = 0;
pub const SAT_THROW_GRENADE: usize = 1;
pub const SAT_JET_ROBOT_ATTACK: usize = 2;
pub const SAT_ADVANCING: usize = 3;
pub const SAT_NUM: usize = 4;

/// AI.ESquadActionStatus: 0 succeeded, 1 failed.
pub const SAS_SUCCEEDED: u8 = 0;
pub const SAS_FAILED: u8 = 1;

/// AI.EAiCharacterType (the ones the squad counts).
pub const AICT_GUARD: i32 = 1;
pub const AICT_SWARMER: i32 = 2;
pub const AICT_SOLDIER: i32 = 4;
pub const AICT_LEAPER: i32 = 7;
pub const AICT_ROCKET_TROOPER: i32 = 13;
pub const AICT_SHARP_SHOOTER: i32 = 24;
pub const AICT_SCRAPPER: i32 = 25;
pub const AICT_NUM: usize = 26;

/// The type of a squad action (its TnSquadAction subclass).
#[derive(Clone, Debug)]
pub enum ActionKind {
    AdvanceOnSuspected {
        min_time_between: f32,
        release_time: f32,
        available: bool,
        number_hidden: i32,
    },
    ThrowGrenade {
        min_throw_interval: f32,
        last_throw_time: f32,
    },
    /// TnJetRobotAttackSquadAction: free at first, then once _MinAttackInterval
    /// has passed since the last was let go (its _LastAttackTime: OnRelease).
    JetRobotAttack {
        min_attack_interval: f32,
        last_attack_time: f32,
    },
    Advancing,
}

/// TnSquadAction.
#[derive(Clone, Debug)]
pub struct SquadAction {
    pub kind: ActionKind,
    pub enabled: bool,
    pub max_claimers: usize,
    pub always_available_to_claimers: bool,
    pub claimers: Vec<Entity>,
    /// Members it failed for, and when (IncapableInfo: ten seconds each).
    pub incapables: Vec<(Entity, f32)>,
}

impl SquadAction {
    fn new(kind: ActionKind, max_claimers: usize, always: bool) -> SquadAction {
        SquadAction {
            kind,
            enabled: true,
            max_claimers,
            always_available_to_claimers: always,
            claimers: Vec::new(),
            incapables: Vec::new(),
        }
    }

    pub fn is_claiming(&self, c: Entity) -> bool {
        self.claimers.contains(&c)
    }

    fn is_incapable(&self, c: Entity, now: f32) -> bool {
        self.incapables
            .iter()
            .find(|(e, _)| *e == c)
            .is_some_and(|(_, t)| t + 10.0 >= now)
    }

    fn add_to_incapables(&mut self, c: Entity, now: f32) {
        match self.incapables.iter_mut().find(|(e, _)| *e == c) {
            Some(x) => x.1 = now,
            None => self.incapables.insert(0, (c, now)),
        }
    }
}

/// What a squad action needs to know of the member it is asked about.
#[derive(Clone, Copy, Debug)]
pub struct MemberInfo {
    pub entity: Entity,
    pub ai_type: i32,
}

/// What a claimer gives a squad action's OnClaim (the grenade throw's: where it
/// would throw, as it can see to).
pub struct ClaimInfo<'a> {
    pub member: MemberInfo,
    pub suspected_enemy: Vec3,
    pub enemy_half_height: Option<f32>,
    pub eyes: Vec3,
    pub loc: Vec3,
    pub view: &'a WorldView<'a>,
    /// The grenade aim point it found (stored as LR_GrenadeAimPoint).
    pub grenade_aim_point: Option<Vec3>,
}

// ---------------------------------------------------------------------------
// The squad
// ---------------------------------------------------------------------------

/// A world state write a squad makes on one of its members (applied to it as soon
/// as the brain doing the squad's work is done).
#[derive(Clone, Copy, Debug)]
pub enum WsWrite {
    Int(usize, i32),
    Bit(usize, i32, bool),
}

/// TnAiSquadManager.
#[derive(Clone, Debug)]
pub struct Squad {
    pub members: Vec<MemberInfo>,
    pub type_counts: [i32; AICT_NUM],
    pub actions: [SquadAction; SAT_NUM],
    /// Who engages each target: its sums, overall (index 0) and by type.
    pub engagements: HashMap<Entity, [i32; AICT_NUM]>,
    /// Events broadcast to the squad this frame (each once).
    pub events: Vec<(EventId, Option<Entity>)>,
    // The swarmers' turns at attacking.
    pub melee_range: f32,
    pub chance_for_another_immediate_attack: f32,
    pub time_between_swarmer_attacks: (f32, f32),
    pub time_before_attack_available: f32,
    pub last_attacker_index: usize,
    pub time_between_leap_smash_attacks: (f32, f32),
    pub time_before_leap_smash_available: f32,
}

impl Default for Squad {
    fn default() -> Self {
        Squad {
            members: Vec::new(),
            type_counts: [0; AICT_NUM],
            actions: [
                SquadAction::new(
                    ActionKind::AdvanceOnSuspected {
                        min_time_between: 5.0,
                        release_time: 0.0,
                        available: false,
                        number_hidden: 0,
                    },
                    1,
                    false,
                ),
                SquadAction::new(
                    ActionKind::ThrowGrenade {
                        min_throw_interval: 10.0,
                        last_throw_time: -1.0,
                    },
                    1,
                    true,
                ),
                SquadAction::new(
                    ActionKind::JetRobotAttack {
                        min_attack_interval: 10.0,
                        last_attack_time: -1.0,
                    },
                    1,
                    true,
                ),
                SquadAction::new(ActionKind::Advancing, 10, false),
            ],
            engagements: HashMap::new(),
            events: Vec::new(),
            melee_range: 1500.0,
            chance_for_another_immediate_attack: 0.4,
            time_between_swarmer_attacks: (0.5, 5.0),
            time_before_attack_available: 0.0,
            last_attacker_index: 0,
            time_between_leap_smash_attacks: (1.0, 2.0),
            time_before_leap_smash_available: 0.0,
        }
    }
}

/// AI.GetSquadActionMask.
pub fn action_mask(action: usize) -> i32 {
    1 << action
}

impl Squad {
    pub fn size(&self) -> usize {
        self.members.len()
    }

    pub fn number_of_type(&self, t: i32) -> i32 {
        self.type_counts.get(t as usize).copied().unwrap_or(0)
    }

    /// EngagingTarget.
    pub fn engaging_target(
        &mut self,
        member: MemberInfo,
        new: Option<Entity>,
        prev: Option<Entity>,
    ) {
        self.add_engagement(member, prev, -1);
        self.add_engagement(member, new, 1);
    }

    fn add_engagement(&mut self, member: MemberInfo, target: Option<Entity>, n: i32) {
        let Some(t) = target else { return };
        let sums = self.engagements.entry(t).or_insert([0; AICT_NUM]);
        sums[0] = (sums[0] + n).max(0);
        let k = (member.ai_type as usize).min(AICT_NUM - 1);
        sums[k] = (sums[k] + n).max(0);
    }

    /// GetTotalEngagingTarget.
    pub fn total_engaging(&self, target: Entity, ai_type: usize) -> i32 {
        self.engagements
            .get(&target)
            .map_or(0, |s| s[ai_type.min(AICT_NUM - 1)])
    }

    /// TnSquadAction.IsAvailableTo (with its OnIsAvailable).
    fn is_available_to(&self, action: usize, c: MemberInfo, now: f32) -> bool {
        let a = &self.actions[action];
        if !a.enabled {
            return false;
        }
        let claiming = a.is_claiming(c.entity);
        if a.claimers.len() >= a.max_claimers && !claiming {
            return false;
        }
        if a.is_incapable(c.entity, now) {
            return false;
        }
        let on = match &a.kind {
            ActionKind::AdvanceOnSuspected { available, .. } => *available,
            ActionKind::ThrowGrenade {
                min_throw_interval,
                last_throw_time,
            } => *last_throw_time >= 0.0 && now - last_throw_time > *min_throw_interval,
            ActionKind::JetRobotAttack {
                min_attack_interval,
                last_attack_time,
            } => *last_attack_time < 0.0 || now - last_attack_time > *min_attack_interval,
            ActionKind::Advancing => {
                let soldiers = self.number_of_type(AICT_SOLDIER)
                    + self.number_of_type(AICT_ROCKET_TROOPER)
                    + self.number_of_type(AICT_SHARP_SHOOTER);
                // (bMemberIsClaiming, or not Escalation: unavailable.)
                !claiming && (a.claimers.len() as f32) < (soldiers as f32 * 0.5).max(1.0)
            }
        };
        on || (a.always_available_to_claimers && claiming)
    }

    /// RefreshSquadActionAvailability: each member's WS_AvailableSquadActions bit.
    fn refresh_availability(&self, action: usize, now: f32, writes: &mut Vec<(Entity, WsWrite)>) {
        for m in &self.members {
            let on = self.is_available_to(action, *m, now);
            writes.push((
                m.entity,
                WsWrite::Bit(super::htn::WS_AVAILABLE_SQUAD_ACTIONS, action as i32, on),
            ));
        }
    }

    /// ClaimSquadAction.
    pub fn claim(
        &mut self,
        claim: &mut ClaimInfo,
        action: usize,
        now: f32,
        writes: &mut Vec<(Entity, WsWrite)>,
    ) -> bool {
        let c = claim.member;
        if self.actions[action].is_claiming(c.entity) {
            return true;
        }
        let unavailable = (
            c.entity,
            WsWrite::Bit(super::htn::WS_AVAILABLE_SQUAD_ACTIONS, action as i32, false),
        );
        if !self.is_available_to(action, c, now) {
            writes.push(unavailable);
            return false;
        }
        // Claim (OnClaim).
        let ok = match &self.actions[action].kind {
            ActionKind::ThrowGrenade { .. } => find_aim_point(claim),
            ActionKind::Advancing => c.ai_type == AICT_SOLDIER,
            _ => true,
        };
        if !ok {
            self.actions[action].add_to_incapables(c.entity, now);
            writes.push(unavailable);
            return false;
        }
        self.actions[action].claimers.push(c.entity);
        self.refresh_availability(action, now, writes);
        true
    }

    /// ReleaseSquadAction.
    pub fn release(
        &mut self,
        c: Entity,
        action: usize,
        status: u8,
        now: f32,
        writes: &mut Vec<(Entity, WsWrite)>,
    ) {
        if !self.actions[action].is_claiming(c) {
            return;
        }
        // OnRelease.
        match &mut self.actions[action].kind {
            ActionKind::ThrowGrenade {
                last_throw_time, ..
            } => {
                if status == SAS_SUCCEEDED {
                    *last_throw_time = now;
                }
            }
            ActionKind::AdvanceOnSuspected { release_time, .. } => *release_time = now,
            ActionKind::JetRobotAttack {
                last_attack_time, ..
            } => *last_attack_time = now,
            _ => {}
        }
        self.actions[action].claimers.retain(|e| *e != c);
        self.refresh_availability(action, now, writes);
    }

    pub fn is_claiming(&self, c: Entity, action: usize) -> bool {
        self.actions[action].is_claiming(c)
    }

    /// MakeIncapableOfSquadAction.
    pub fn make_incapable(
        &mut self,
        c: Entity,
        action: usize,
        now: f32,
        writes: &mut Vec<(Entity, WsWrite)>,
    ) {
        if self.actions[action].is_claiming(c) {
            self.release(c, action, SAS_FAILED, now, writes);
        }
        self.actions[action].add_to_incapables(c, now);
        self.refresh_availability(action, now, writes);
    }

    /// AddSquadMember.
    pub fn add_member(&mut self, m: MemberInfo, now: f32, writes: &mut Vec<(Entity, WsWrite)>) {
        if self.members.iter().any(|x| x.entity == m.entity) {
            return;
        }
        self.members.push(m);
        if let Some(c) = self.type_counts.get_mut(m.ai_type as usize) {
            *c += 1;
        }
        for a in 0..SAT_NUM {
            if self.is_available_to(a, m, now) {
                writes.push((
                    m.entity,
                    WsWrite::Bit(super::htn::WS_AVAILABLE_SQUAD_ACTIONS, a as i32, true),
                ));
            }
        }
    }

    /// RemoveSquadMember.
    pub fn remove_member(&mut self, c: Entity, now: f32, writes: &mut Vec<(Entity, WsWrite)>) {
        let Some(i) = self.members.iter().position(|x| x.entity == c) else {
            return;
        };
        for a in 0..SAT_NUM {
            self.release(c, a, SAS_FAILED, now, writes);
        }
        let m = self.members.remove(i);
        if let Some(n) = self.type_counts.get_mut(m.ai_type as usize) {
            *n = (*n - 1).max(0);
        }
        // Its engagements go with it.
        for sums in self.engagements.values_mut() {
            let _ = sums;
        }
    }

    /// The squad's BroadcastEvent (deduplicated, delivered on its next update).
    pub fn broadcast(&mut self, id: EventId, context: Option<Entity>) {
        if !self.events.contains(&(id, context)) {
            self.events.push((id, context));
        }
    }

    /// TnAdvanceOnSuspectedSquadAction.UpdateThreatsVisible.
    pub fn update_threats_visible(&mut self, visible: bool) {
        if let ActionKind::AdvanceOnSuspected { number_hidden, .. } =
            &mut self.actions[SAT_ADVANCE_ON_SUSPECTED_ENEMY].kind
        {
            *number_hidden = if visible { 0 } else { *number_hidden + 1 };
        }
    }

    /// Update: the squad's events to its members, the actions' ticks, the
    /// engagements pruned, the swarmers' turns, the actions' availability.
    pub fn update(
        &mut self,
        dt: f32,
        now: f32,
        alive: &dyn Fn(Entity) -> bool,
        swarmers: &mut dyn FnMut(&Squad) -> Vec<(Entity, SwarmerInfo)>,
        rng: &mut Rng,
        deliver: &mut Vec<(Entity, Event)>,
        writes: &mut Vec<(Entity, WsWrite)>,
    ) {
        // FlushSquadEvents.
        for (id, ctx) in std::mem::take(&mut self.events) {
            for m in &self.members {
                if alive(m.entity) {
                    deliver.push((m.entity, Event::about(id, ctx)));
                }
            }
        }
        // UpdateActions.
        let size = self.size();
        for a in self.actions.iter_mut() {
            match &mut a.kind {
                ActionKind::AdvanceOnSuspected {
                    min_time_between,
                    release_time,
                    available,
                    number_hidden,
                } => {
                    *available =
                        now - *release_time >= *min_time_between && *number_hidden >= size as i32;
                }
                ActionKind::ThrowGrenade {
                    last_throw_time, ..
                } => {
                    if size == 0 || *last_throw_time == -1.0 {
                        *last_throw_time = now;
                    }
                }
                _ => {}
            }
        }
        // PruneEngagements.
        self.engagements.retain(|t, _| alive(*t));
        if self.members.is_empty() {
            return;
        }
        self.update_swarmers(dt, swarmers, rng, writes);
        // RefreshActions.
        for a in 0..SAT_NUM {
            self.refresh_availability(a, now, writes);
        }
        self.time_before_leap_smash_available -= dt;
    }

    /// UpdateSwarmers: the swarmers and scrappers take turns at their enemy.
    fn update_swarmers(
        &mut self,
        dt: f32,
        swarmers: &mut dyn FnMut(&Squad) -> Vec<(Entity, SwarmerInfo)>,
        rng: &mut Rng,
        writes: &mut Vec<(Entity, WsWrite)>,
    ) {
        self.time_before_attack_available -= dt;
        let n = self.members.len();
        if self.last_attacker_index >= n {
            self.last_attacker_index = 0;
        }
        let infos = swarmers(self);
        let last_attacker = self.members[self.last_attacker_index].entity;
        let offset = self.last_attacker_index + 1;
        let mut current = self.last_attacker_index;
        for index in 0..n {
            let k = (index + offset) % n;
            let m = self.members[k];
            if m.ai_type != AICT_SWARMER && m.ai_type != AICT_SCRAPPER {
                continue;
            }
            let Some((_, info)) = infos.iter().find(|(e, _)| *e == m.entity) else {
                continue;
            };
            if !info.alive {
                continue;
            }
            if info.enemy_is_ai {
                writes.push((m.entity, WsWrite::Int(super::htn::WS_SQUAD_COMMAND, 1)));
                continue;
            }
            // SwarmerSquadCommand.
            if info.has_enemy
                && self.time_before_attack_available <= 0.0
                && info.clear_channel
                && info.range < self.melee_range
            {
                writes.push((m.entity, WsWrite::Int(super::htn::WS_SQUAD_COMMAND, 1)));
                if m.entity != last_attacker
                    && rng.frand() < self.chance_for_another_immediate_attack
                {
                    self.time_before_attack_available = 0.0;
                } else {
                    let (lo, hi) = self.time_between_swarmer_attacks;
                    self.time_before_attack_available = rng.range(lo, hi);
                }
                current = k;
            }
        }
        self.last_attacker_index = current;
    }

    /// ClaimLeapSmash.
    pub fn claim_leap_smash(&mut self, rng: &mut Rng, writes: &mut Vec<(Entity, WsWrite)>) {
        let (lo, hi) = self.time_between_leap_smash_attacks;
        self.time_before_leap_smash_available = rng.range(lo, hi);
        for m in &self.members {
            if m.ai_type == AICT_LEAPER {
                writes.push((
                    m.entity,
                    WsWrite::Bit(super::htn::WS_SPECIAL_ATTACK_AVAILABLE, 0, false),
                ));
            }
        }
    }
}

/// What the swarmers' turns need of a member.
#[derive(Clone, Copy, Debug, Default)]
pub struct SwarmerInfo {
    pub alive: bool,
    pub has_enemy: bool,
    /// Its enemy is no player (another AI: attack at once).
    pub enemy_is_ai: bool,
    pub clear_channel: bool,
    pub range: f32,
}

/// TnThrowGrenadeSquadAction.FindAimPoint: the suspected enemy's place, or 750
/// either side of it, that the claimer can see to.
fn find_aim_point(c: &mut ClaimInfo) -> bool {
    let ideal = c.suspected_enemy;
    let can_throw = |c: &ClaimInfo, point: Vec3| {
        let mut p = point;
        if let Some(h) = c.enemy_half_height {
            p += Vec3::Z * h;
        }
        c.view.fast_trace(c.eyes, p)
    };
    if can_throw(c, ideal) {
        c.grenade_aim_point = Some(ideal);
        return true;
    }
    let yaw = ue::yaw_of(c.loc - ideal);
    let pitch = ue::pitch_of(c.loc - ideal);
    let right = ideal + super::weapon::from_local(Vec3::new(0.0, 750.0, 0.0), pitch, yaw);
    if can_throw(c, right) {
        c.grenade_aim_point = Some(right);
        return true;
    }
    let left = ideal + super::weapon::from_local(Vec3::new(0.0, -750.0, 0.0), pitch, yaw);
    if can_throw(c, left) {
        c.grenade_aim_point = Some(left);
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// The tactical system
// ---------------------------------------------------------------------------

/// A tactical node's changing state (TnTactical's: its claims are the system's).
#[derive(Clone, Copy, Debug, Default)]
pub struct TacticalState {
    pub disabled: bool,
    /// Its line of sight to each team (0-255: 255 sees it fully).
    pub los_to_team: [u8; 3],
    /// The team whose pawn is nearest (255 for none).
    pub controlling_team: u8,
    pub stink_time: f32,
    pub waiting_for_update: bool,
}

/// TacticalOcclusion's TraceOffsets by cover (where the node looks from: forward,
/// right, up of the way it looks, in uu).
pub fn trace_offset(cover: Cover) -> Vec3 {
    match cover {
        Cover::None => Vec3::ZERO,
        Cover::Low => Vec3::new(100.0, 0.0, 200.0),
        Cover::LowRightCorner => Vec3::new(0.0, -155.0, 200.0),
        Cover::LowLeftCorner => Vec3::new(0.0, 155.0, 200.0),
        Cover::RightCorner => Vec3::new(0.0, -155.0, 200.0),
        Cover::LeftCorner => Vec3::new(0.0, 155.0, 200.0),
        Cover::Ledge => Vec3::new(100.0, 0.0, 200.0),
    }
}

/// TacticalOcclusion.GetOcclusionIndex: the 32nd of a turn a direction is in.
pub fn occlusion_index(dir: Vec3) -> i32 {
    let n = dir.normalize_or_zero();
    let index = (n.x.clamp(-1.0, 1.0).acos() / (2.0 * std::f32::consts::PI) * 32.0).floor() as i32;
    if dir.y < 0.0 { 32 - 1 - index } else { index }
}

/// OcclusionIsCompatible.
pub fn occlusion_compatible(required: u32, existing: u32) -> bool {
    ((required & existing) ^ required) == 0
}

/// An enemy the AI has seen (TnTacticalSystem's registered sightings).
#[derive(Clone, Copy, Debug)]
pub struct Sighting {
    pub entity: Entity,
    pub team: u8,
    pub location: Vec3,
}

/// TnTacticalSystem.
#[derive(Clone, Debug, Default)]
pub struct TacticalSystem {
    /// The claimed groups (by ClaimGroup).
    pub claims: std::collections::HashSet<i32>,
    pub sightings: Vec<Sighting>,
    /// The pawns that make the battle line (the AIs': their places and teams are
    /// the frame's view's).
    pub battle_line: Vec<Entity>,
    /// Each tactical node's state (by its index in the mesh's tacticals).
    pub states: Vec<TacticalState>,
    /// The update queue (TnTacticalUpdater's), and the nodes the AIs' collections
    /// hold (each brain's, this frame).
    pub queue: std::collections::VecDeque<u32>,
}

impl TacticalSystem {
    pub fn reset(&mut self, nav: Option<&NavMesh>) {
        self.claims.clear();
        self.sightings.clear();
        self.battle_line.clear();
        self.queue.clear();
        self.states = vec![
            TacticalState {
                controlling_team: 255,
                stink_time: -1.0e9,
                ..Default::default()
            };
            nav.map_or(0, |n| n.tacticals.len())
        ];
    }

    pub fn claim(&mut self, group: i32) {
        self.claims.insert(group);
    }

    pub fn release(&mut self, group: i32) {
        self.claims.remove(&group);
    }

    pub fn is_claimed(&self, group: i32) -> bool {
        self.claims.contains(&group)
    }

    /// SawEnemy.
    pub fn saw_enemy(&mut self, enemy: Entity, team: u8, location: Vec3) {
        match self.sightings.iter_mut().find(|s| s.entity == enemy) {
            Some(s) => s.location = location,
            None => self.sightings.push(Sighting {
                entity: enemy,
                team,
                location,
            }),
        }
    }

    pub fn latest_enemy_location(&self, enemy: Entity) -> Option<Vec3> {
        self.sightings
            .iter()
            .find(|s| s.entity == enemy)
            .map(|s| s.location)
    }

    /// RemoveDeadEnemies (the players' pawns that are gone: a dead player's).
    pub fn remove_dead_enemies(&mut self, alive: impl Fn(Entity) -> bool) {
        self.sightings.retain(|s| alive(s.entity));
    }

    /// AddBattleLinePawn.
    pub fn add_battle_line_pawn(&mut self, e: Entity) {
        if !self.battle_line.contains(&e) {
            self.battle_line.push(e);
        }
    }

    /// RemoveBattleLinePawn.
    pub fn remove_battle_line_pawn(&mut self, e: Entity) {
        self.battle_line.retain(|x| *x != e);
    }

    /// The node's state.
    pub fn state(&self, t: u32) -> TacticalState {
        self.states
            .get(t as usize)
            .copied()
            .unwrap_or(TacticalState {
                controlling_team: 255,
                stink_time: -1.0e9,
                ..Default::default()
            })
    }

    /// HasLosToTeam.
    pub fn has_los_to_team(&self, t: u32, team: i32) -> bool {
        (0..3).contains(&team) && self.state(t).los_to_team[team as usize] == 255
    }

    /// GetLosToTeam.
    pub fn los_to_team(&self, t: u32, team: i32) -> f32 {
        if (0..3).contains(&team) {
            self.state(t).los_to_team[team as usize] as f32 * 0.00392
        } else {
            0.0
        }
    }

    /// Stinkify: a death within 600 uu of the node.
    pub fn stinkify(&mut self, t: u32, nav: &NavMesh, dead: Vec3, now: f32) {
        // (`t` is the node's index among the mesh's tactical nodes.)
        let Some(tac) = nav.tacticals.get(t as usize) else {
            return;
        };
        let at = ue::to_ue(nav.tactical_at(tac));
        if ue::vsize_sq2d(dead - at) > 360000.0 {
            return;
        }
        if let Some(s) = self.states.get_mut(t as usize) {
            s.stink_time = now;
        }
    }

    /// The nodes a collection holds (their points) join the update queue
    /// (AddTacticalToUpdateQueue).
    pub fn enqueue(&mut self, nav: &NavMesh, points: &[u32]) {
        for &p in points {
            let Some(t) = nav.tactical_index(p) else {
                continue;
            };
            if let Some(s) = self.states.get_mut(t as usize)
                && !s.waiting_for_update
                && self.queue.len() < 800
            {
                s.waiting_for_update = true;
                self.queue.push_back(t);
            }
        }
    }

    /// TacticalHelpers.Update: the queued nodes' caches (their line of sight to
    /// each team's last seen pawns, their controlling team), as many as the
    /// frame's budget allows (`most` of them).
    pub fn update(&mut self, nav: &NavMesh, view: &WorldView, most: usize) {
        let line: Vec<(Vec3, u8)> = self
            .battle_line
            .iter()
            .filter_map(|e| view.actor(*e))
            .map(|a| (a.loc, a.team))
            .collect();
        let all_same_team = !line.is_empty() && line.iter().all(|(_, t)| *t == line[0].1);
        for _ in 0..most {
            let Some(t) = self.queue.pop_front() else {
                break;
            };
            let Some(tac) = nav.tacticals.get(t as usize).copied() else {
                continue;
            };
            let at = ue::to_ue(nav.tactical_at(&tac));
            let mut los = [0u8; 3];
            for s in &self.sightings {
                if s.team >= 3 || los[s.team as usize] == 255 {
                    continue;
                }
                let distance = (s.location - at).length();
                let hit_time = check_los(view, at, tac.cover, tac.occlusion, s.location);
                let visible = distance * hit_time;
                let hidden = distance * (1.0 - hit_time);
                let mut score = 1.0 - hidden * 5.0e-5;
                if visible < 400.0 {
                    score = 0.0;
                }
                let k = s.team as usize;
                los[k] = los[k].max((score * 255.0).clamp(0.0, 255.0) as u8);
            }
            let st = &mut self.states[t as usize];
            st.los_to_team = los;
            // UpdateControllingTeam.
            if all_same_team {
                st.controlling_team = line[0].1;
            } else {
                st.controlling_team = 255;
                let mut best = f32::MAX;
                for (loc, team) in &line {
                    if *team == 255 {
                        continue;
                    }
                    let d = (*loc - at).length();
                    if d < best {
                        best = d;
                        st.controlling_team = *team;
                    }
                }
            }
            st.waiting_for_update = false;
        }
    }
}

/// TnTactical.CheckLOS: how far along the line to the target the level is
/// struck (its cover's occlusion hiding the target: 0). The occlusion's
/// directions are the world's.
fn check_los(view: &WorldView, at: Vec3, cover: Cover, occlusion: u32, target: Vec3) -> f32 {
    if cover != Cover::None && !occlusion_compatible(1 << occlusion_index(target - at), occlusion) {
        return 0.0;
    }
    let offset = trace_offset(cover);
    let end = target + Vec3::Z * offset.z;
    let dir_yaw = ue::yaw_of(target - at);
    let dir_pitch = ue::pitch_of(target - at);
    let start = at + super::weapon::from_local(offset, dir_pitch, dir_yaw);
    view.trace_time(start, end)
}

// ---------------------------------------------------------------------------
// Everything shared
// ---------------------------------------------------------------------------

/// A shot an AI fired this frame (for its effects: the muzzle, the tracer, the
/// impact) and what it struck.
#[derive(Clone, Debug)]
pub struct Shot {
    pub shooter: Entity,
    pub from: Vec3,
    pub to: Vec3,
    pub hit_player: Option<Entity>,
    pub hit_normal: Option<Vec3>,
    pub tracer: bool,
    pub gun: String,
    /// The round's first trace (its muzzle flash: TnWeaponEventFireShot's), and
    /// whether it is a trace at all (none for a projectile's round: its flash
    /// only).
    pub first: bool,
    pub traced: bool,
    /// Its damage where it stopped (its range's scale of it) and its damage type,
    /// for what it struck that isn't a player (an exploding barrel: barrels.rs); none
    /// for a projectile's round, or the host's shot at a client's PC.
    pub damage: f32,
    pub damage_type: &'static str,
}

/// A projectile an AI fired this frame (its gun's round, TnWeapon.ProjectileFire;
/// an animation's, TnProjectileSpawner.SpawnProjectile): who fired it, from where,
/// which way, its data, and what it homes in on (a homing round's target: its
/// shooter's enemy).
#[derive(Clone, Debug)]
pub struct Launch {
    pub by: Entity,
    pub from: Vec3,
    pub dir: Vec3,
    pub data: std::sync::Arc<super::weapon::ProjectileData>,
    pub target: Option<Entity>,
}

/// Damage an AI does to a player this frame (its blow's way in `dir`), and what
/// its damage type does besides: a push he takes (its momentum, Unreal's units:
/// TnDamageTypeGuardKnockback's RequestRespectForcesApplied) and a buff it puts
/// on him (its VictimBuffs: the Guard's area attack jams him).
#[derive(Clone, Debug)]
pub struct PlayerDamage {
    pub player: Entity,
    pub by: Entity,
    pub amount: f32,
    pub at: Vec3,
    pub dir: Vec3,
    pub melee: bool,
    pub momentum: Option<Vec3>,
    pub jam: bool,
    /// A gun's round's kill: its damage type's death, its momentum (Unreal's units,
    /// along `dir`) and whether it gibs (its GibDistance from the shooter); None for
    /// the rest (a blow's, a blast's).
    pub shot: Option<(crate::enemies::Death, f32, bool)>,
    /// Its damage type (the kill feed's: killfeed.rs's DAMAGE_TYPES).
    pub damage: &'static str,
}

/// The gravity a thrown grenade falls with (the world's: TnProjectileGrenade's
/// GetGravityZ).
pub const GRENADE_GRAVITY_Z: f32 = -2940.0;

/// A grenade an AI threw (its kind, where from, its velocity).
#[derive(Clone, Debug)]
pub struct Throw {
    pub by: Entity,
    pub class: String,
    pub from: Vec3,
    pub velocity: Vec3,
}

/// What the brains share and what they did this frame.
#[derive(Default)]
pub struct Shared {
    pub now: f32,
    pub squad: Squad,
    pub tactical: TacticalSystem,
    pub rng: Rng,
    /// World state writes on other brains (by entity), applied after the brain
    /// that made them.
    pub writes: Vec<(Entity, WsWrite)>,
    /// Events to other brains (TnHtnAiController.BroadcastEvent: to every one but
    /// the sender; and the squad's, delivered on its update).
    pub broadcasts: Vec<(Entity, Event)>,
    pub deliver: Vec<(Entity, Event)>,
    // -- Its effects on the world --------------------------------------------
    pub shots: Vec<Shot>,
    pub player_damage: Vec<PlayerDamage>,
    /// The damage the AIs' rounds did to the players' barriers (each barrier's
    /// entity: equipment.rs's Barrier takes it).
    pub barrier_damage: Vec<(Entity, f32)>,
    pub throws: Vec<Throw>,
    pub launches: Vec<Launch>,
    /// Sounds to play at a place (an AI's gun's...), and loops held this frame.
    pub sounds: Vec<(Entity, &'static str, Vec3)>,
    /// (Each loop with the time it fades out over once it is let go.)
    pub loops: Vec<(Entity, &'static str, Vec3, f32)>,
    /// The lines of dialog the AIs asked for this frame (their incidental
    /// dialog's, their animations'), and the dialog manager that lets them speak.
    pub dialog: Vec<super::audio::DialogRequest>,
    pub dialog_manager: super::audio::DialogManager,
    /// LinesOfSightManager's answers: each pair's line of sight and when it was
    /// traced.
    pub los: std::collections::HashMap<(Entity, Entity), (bool, f32)>,
    /// The players the Guards' scanners caught this frame (exposed, jammed).
    pub scanned: Vec<super::guard::Scanned>,
    /// The effects the AIs played this frame (their particle systems on their
    /// sockets: their animations' HmAnimNotify_PlayEffect, their guns' muzzle
    /// effects), and the held ones they stopped (by whom and which: the Guardian
    /// Blaster's charge as it ends).
    pub effects: Vec<Effect>,
    pub effects_stopped: Vec<(Entity, std::sync::Arc<str>)>,
    /// The sniper points' claims (the Snipers': sniper.rs).
    pub sniper_claims: super::sniper::Claims,
    /// The projectiles the animations spawn (TnAnimNotify_SpawnProjectile), by
    /// class: their data (ai.txt's, read once).
    pub notify_projectiles: std::sync::Arc<
        std::collections::HashMap<String, std::sync::Arc<super::weapon::ProjectileData>>,
    >,
    /// The support drones' shields (TnBuffSupportDroneDamage): on whom, and each
    /// drone that puts it there (drones.rs).
    pub shields: Vec<(Entity, Entity)>,
    /// The drones the Titans asked for this frame, and those come in since (each
    /// to its Titan; none where one couldn't come).
    pub drone_spawns: Vec<super::drones::DroneSpawn>,
    pub drones_spawned: Vec<(Entity, Option<Entity>)>,
    /// The deaths the AIs' script dealt them this frame (who, and how): a drone's
    /// blast, its lifespan out, its Titan dead.
    pub kills: Vec<(Entity, crate::enemies::Death)>,
}

/// A particle system an AI started on a socket (its animation's
/// HmAnimNotify_PlayEffect, its gun's TnWeaponEffectPlayMuzzleFX): where and which
/// way (its socket's frame, Unreal's space), its place and turn in that frame
/// (`offset`, uu; `turn`: pitch, yaw and roll), and on whom, if it is held on its
/// socket (bAttachToMesh: it moves with him), and whether it starts only if his
/// last one of it has ended (PEC_PlayOnlyIfNotAlreadyPlaying) or starts that one
/// over (`restart`, PEC_RestartIfAlreadyPlaying: its emitters rewound, what is out
/// living on). `params`: its
/// notify's Parameters (HmAnimNotify_PlayEffect.Notify applies them as it starts
/// it): colour parameters set to his EnergonColor
/// (TnParticleSystemParameterEnergonColor.Apply: his pawn's, an sRGB Color, alpha
/// and all), and "@team" for his side's module channel
/// (TnParticleSystemParameterTeamIndex: an AI has no team, so his faction's).
/// `auto_off`: an animation's effect turned off with its clip (bAutoDeactivate),
/// this many seconds after it starts (clips.rs's ClipNotify::Effect); one held
/// on its socket that is still on plays on rather than another starting by it
/// (HmParticleEffectManagerComponent.PlayEffect's PlayCondition 1).
#[derive(Clone, Debug)]
pub struct Effect {
    pub by: Entity,
    pub system: std::sync::Arc<str>,
    pub socket: std::sync::Arc<str>,
    pub at: Vec3,
    pub axes: [Vec3; 3],
    pub offset: Vec3,
    pub turn: Vec3,
    pub attached: bool,
    pub only_once: bool,
    pub restart: bool,
    pub params: Option<std::sync::Arc<[std::sync::Arc<str>]>>,
    pub auto_off: Option<f32>,
}

impl Default for Rng {
    fn default() -> Self {
        Rng::new(0x5EED)
    }
}

impl Shared {
    /// TnHtnAiController.BroadcastEvent (static: to every AI controller but the
    /// sender).
    pub fn broadcast(&mut self, sender: Entity, id: EventId, context: Option<Entity>) {
        self.broadcasts.push((sender, Event::about(id, context)));
    }

    pub fn is_team(&self, team: u8) -> bool {
        team == AI_TEAM
    }
}
