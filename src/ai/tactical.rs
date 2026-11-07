//! TnTacticalAttackSensor: which tactical node (a place to fight from, behind cover
//! or in the open) a soldier takes. Its picker (TnTacticalPicker) scores the
//! nodes its collection (TnTacticalCollection: the nodes on the cells within
//! 5000 uu of it, and further out when none has a line of sight to its enemy)
//! holds by its evaluators, combined as its blueprint's RPN has them; the best
//! scoring above 0 is claimed (its claim group: no other may take one of the
//! group), stored as LR_AttackPoint and made WS_AttackPointAvailable (its cover in
//! the context). It may change node only when it has been at one too long, has
//! waited too long without a line of sight to its enemy, or its node no longer
//! scores.

use bevy::prelude::*;

use super::brain::{Brain, Ctx, Event, EventId};
use super::htn;
use super::sensors::{Rpn, rpn_evaluate};
use super::shared::{self, TacticalSystem};
use super::ue;
use crate::nav::{Cover, NavMesh};

/// AI.ActorSighting: who, where, and its team.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sighting {
    pub entity: Option<Entity>,
    pub location: Vec3,
    pub team: u8,
}

/// The tactical evaluators the soldiers use (TnXxxTacticalEvaluator).
#[derive(Clone, Debug)]
pub enum TacticalEval {
    BattleLine,
    Stink {
        duration: f32,
    },
    MinEngagementDistance {
        min_distance_sq: f32,
    },
    Advance {
        min_advance_distance: f32,
        distance_to_enemy: f32,
        running_los_count: i32,
        evals_without_los: i32,
        should_advance: bool,
    },
    Occlusion,
    LineOfSight,
    /// (No actor to defend: a score of 1.)
    DefendLocation,
    CloseOnEnemy {
        min_advance_distance: f32,
        distance_to_enemy: f32,
        enabled: bool,
    },
}

impl TacticalEval {
    pub fn advance(min_advance_distance: f32) -> TacticalEval {
        TacticalEval::Advance {
            min_advance_distance,
            distance_to_enemy: 0.0,
            running_los_count: 0,
            evals_without_los: 30,
            should_advance: false,
        }
    }

    pub fn close_on_enemy(min_advance_distance: f32, enabled: bool) -> TacticalEval {
        TacticalEval::CloseOnEnemy {
            min_advance_distance,
            distance_to_enemy: 0.0,
            enabled,
        }
    }

    /// The class of TnTacticalEvaluator it is (FindEvaluator's).
    pub fn class(&self) -> &'static str {
        match self {
            TacticalEval::BattleLine => "TnBattleLineTacticalEvaluator",
            TacticalEval::Stink { .. } => "TnStinkTacticalEvaluator",
            TacticalEval::MinEngagementDistance { .. } => {
                "TnMinEngagementDistanceTacticalEvaluator"
            }
            TacticalEval::Advance { .. } => "TnAdvanceTacticalEvaluator",
            TacticalEval::Occlusion => "TnOcclusionTacticalEvaluator",
            TacticalEval::LineOfSight => "TnLineOfSightTacticalEvaluator",
            TacticalEval::DefendLocation => "TnDefendLocationTacticalEvaluator",
            TacticalEval::CloseOnEnemy { .. } => "TnCloseOnEnemyTacticalEvaluator",
        }
    }

    /// set_Enabled.
    pub fn set_enabled(&mut self, on: bool) {
        if let TacticalEval::CloseOnEnemy { enabled, .. } = self {
            *enabled = on;
        }
    }
}

/// A tactical node's fixed and changing state, as the evaluators read it.
#[derive(Clone, Copy, Debug)]
pub struct TacticalNode {
    pub point: u32,
    pub location: Vec3,
    pub yaw: f32,
    pub cover: Cover,
    pub occlusion: u32,
    pub claim_group: i32,
    pub state: shared::TacticalState,
}

/// The node at navigation point `point`, if it is one.
pub fn node(nav: &NavMesh, sys: &TacticalSystem, point: u32) -> Option<TacticalNode> {
    let t = nav.tactical(point)?;
    let index = nav.tactical_index(point)?;
    let p = &nav.points[point as usize];
    Some(TacticalNode {
        point,
        location: ue::to_ue(nav.tactical_at(t)),
        yaw: ue::yaw_of_heading(p.yaw),
        cover: t.cover,
        occlusion: t.occlusion,
        claim_group: t.claim_group,
        state: sys.state(index),
    })
}

/// TnTacticalCollection.
#[derive(Clone, Debug, Default)]
pub struct Collection {
    pub tactical: Vec<u32>,
    pub cells: Vec<u32>,
    pub source: Vec3,
    pub target: Vec3,
    pub fixed: bool,
    pub searches_without_tactical: i32,
}

impl Collection {
    /// RequiresUpdate: none held, or the source or target moved 100 uu.
    fn requires_update(&self, source: Vec3, target: Vec3) -> bool {
        self.tactical.is_empty()
            || ue::vsize_sq2d(source - self.source) > 100.0 * 100.0
            || ue::vsize_sq2d(target - self.target) > 100.0 * 100.0
    }

    /// Update: the cells within 5000 uu (5000 more for each search without a
    /// tactical, to 15000) of the source, none within 1000 of the target, and
    /// their tactical nodes.
    pub fn update(&mut self, nav: &NavMesh, root: u32, source: Vec3, target: Vec3, expand: bool) {
        if self.fixed {
            return;
        }
        let range = (5000 * (1 + self.searches_without_tactical)).min(15000) as f32;
        if !self.requires_update(source, target) && (!expand || range >= 15000.0) {
            return;
        }
        self.source = source;
        self.target = target;
        self.cells = nav.cells_in_radius(
            root,
            ue::from_ue(source),
            range / ue::UU_PER_M,
            ue::from_ue(target),
            1000.0 / ue::UU_PER_M,
            false,
        );
        self.tactical = nav.tactical_in_cells(&self.cells);
        self.searches_without_tactical = if expand {
            self.searches_without_tactical + 1
        } else {
            0
        };
    }
}

/// TnTacticalPicker.
#[derive(Clone, Debug, Default)]
pub struct Picker {
    pub evaluators: Vec<(TacticalEval, f32)>,
    pub eval_results: Vec<f32>,
    pub rpn: Vec<Rpn>,
    pub searches_with_no_los: i32,
    pub claimed: Option<u32>,
    pub target: Sighting,
    pub source: Sighting,
    pub last_target: Option<Entity>,
    pub root: Option<u32>,
    pub nearby: Collection,
}

impl Picker {
    pub fn add_rpn_evaluator(&mut self, e: TacticalEval, weight: f32) {
        self.evaluators.push((e, weight));
        self.eval_results.resize(self.evaluators.len(), 0.0);
        self.rpn.push(Rpn::Value(self.evaluators.len() - 1));
    }

    pub fn add_rpn_operator(&mut self, op: Rpn) {
        self.rpn.push(op);
    }

    /// AddEvaluator: one its scores are summed with (no RPN).
    pub fn add_evaluator(&mut self, e: TacticalEval, weight: f32) {
        self.evaluators.push((e, weight));
        self.eval_results.resize(self.evaluators.len(), 0.0);
    }

    /// FindEvaluator.
    pub fn find_evaluator(&mut self, class: &str) -> Option<&mut TacticalEval> {
        self.evaluators
            .iter_mut()
            .find(|(e, _)| e.class() == class)
            .map(|(e, _)| e)
    }

    /// UpdateSource.
    pub fn update_source(&mut self, entity: Entity, loc: Vec3, team: u8) {
        self.source = Sighting {
            entity: Some(entity),
            location: loc,
            team,
        };
    }

    /// UpdateTarget: a target seen at `location` (registered with the tactical
    /// system), or with none its latest known place.
    pub fn update_target(
        &mut self,
        sys: &mut TacticalSystem,
        target: Option<(Entity, Vec3, u8)>,
        alive: bool,
    ) {
        match target {
            None => {
                if let Some(last) = self.last_target
                    && alive
                    && let Some(loc) = sys.latest_enemy_location(last)
                {
                    self.target.location = loc;
                }
            }
            Some((e, loc, team)) => {
                self.target = Sighting {
                    entity: Some(e),
                    location: loc,
                    team,
                };
                sys.saw_enemy(e, team, loc);
                self.last_target = Some(e);
            }
        }
    }

    /// Claim.
    pub fn claim(&mut self, sys: &mut TacticalSystem, n: &TacticalNode) {
        sys.claim(n.claim_group);
        self.claimed = Some(n.point);
    }

    /// Release.
    pub fn release(&mut self, sys: &mut TacticalSystem, n: &TacticalNode) {
        sys.release(n.claim_group);
        self.claimed = None;
    }

    /// CanClaimTactical.
    fn can_claim(
        &self,
        sys: &TacticalSystem,
        claimed: Option<&TacticalNode>,
        n: &TacticalNode,
    ) -> bool {
        if let Some(c) = claimed
            && c.claim_group == n.claim_group
        {
            return true;
        }
        !n.state.disabled && !sys.is_claimed(n.claim_group)
    }

    /// PreEvaluatePass.
    fn pre_evaluate(&mut self) {
        let (s, t) = (self.source, self.target);
        for (e, _) in self.evaluators.iter_mut() {
            match e {
                TacticalEval::Advance {
                    min_advance_distance,
                    distance_to_enemy,
                    ..
                }
                | TacticalEval::CloseOnEnemy {
                    min_advance_distance,
                    distance_to_enemy,
                    ..
                } => {
                    *distance_to_enemy = (s.location - t.location).length() - *min_advance_distance
                }
                _ => {}
            }
        }
    }

    /// PostEvaluatePass.
    fn post_evaluate(&mut self) {
        for (e, _) in self.evaluators.iter_mut() {
            if let TacticalEval::Advance {
                running_los_count,
                evals_without_los,
                should_advance,
                ..
            } = e
            {
                *evals_without_los = if *running_los_count > 0 {
                    0
                } else {
                    *evals_without_los + 1
                };
                *should_advance = *evals_without_los >= 30;
                *running_los_count = 0;
            }
        }
    }

    /// CalculateScore: -1 if any evaluator rules it out, else the RPN of their
    /// scores.
    pub fn calculate_score(&mut self, n: &TacticalNode, now: f32) -> f32 {
        if n.state.disabled {
            return -1.0;
        }
        let mut total = 0.0;
        for k in 0..self.evaluators.len() {
            let w = self.evaluators[k].1;
            let s = w * self.evaluate(k, n, now);
            self.eval_results[k] = s;
            if s < 0.0 {
                return -1.0;
            }
            total += s;
        }
        if !self.rpn.is_empty() {
            return rpn_evaluate(&self.rpn, &self.eval_results);
        }
        total
    }

    fn evaluate(&mut self, k: usize, n: &TacticalNode, now: f32) -> f32 {
        let (s, t) = (self.source, self.target);
        let team = t.team as i32;
        match &mut self.evaluators[k].0 {
            TacticalEval::BattleLine => {
                if n.state.controlling_team == 255 || n.state.controlling_team != s.team {
                    0.1
                } else {
                    1.0
                }
            }
            TacticalEval::Stink { duration } => {
                if now - n.state.stink_time < *duration {
                    0.0
                } else {
                    1.0
                }
            }
            TacticalEval::MinEngagementDistance { min_distance_sq } => {
                if (n.location - t.location).length_squared() > *min_distance_sq {
                    1.0
                } else {
                    0.0
                }
            }
            TacticalEval::Advance {
                min_advance_distance,
                distance_to_enemy,
                running_los_count,
                should_advance,
                ..
            } => {
                if has_los_to_team(n, team) {
                    *running_los_count += 1;
                }
                if !*should_advance {
                    return 1.0;
                }
                let range = (n.location - t.location).length() - *min_advance_distance;
                if range < 0.0 {
                    return 0.0;
                }
                (1.0 - range / *distance_to_enemy + 1.0).max(0.0)
            }
            TacticalEval::Occlusion => occlusion_score(n, t.location),
            TacticalEval::LineOfSight => los_to_team(n, team),
            TacticalEval::DefendLocation => 1.0,
            TacticalEval::CloseOnEnemy {
                min_advance_distance,
                distance_to_enemy,
                enabled,
            } => {
                let range = (n.location - t.location).length() - *min_advance_distance;
                if !*enabled || range < 0.0 || *distance_to_enemy <= 0.0 {
                    return 0.0;
                }
                (1.0 - range / *distance_to_enemy + 1.0).max(0.0)
            }
        }
    }

    /// FindBestTactical: the best node of its collection (its own while it may
    /// not change, or one of its claim group's), if any scores above 0.
    pub fn find_best_tactical(
        &mut self,
        nav: &NavMesh,
        sys: &TacticalSystem,
        root: u32,
        center: Vec3,
        limit_to_claim_group: bool,
        enemy_team: Option<u8>,
        now: f32,
    ) -> Option<u32> {
        let expand = enemy_team.is_some() && self.searches_with_no_los > 2;
        let target = self.target.location;
        self.nearby.update(nav, root, center, target, expand);
        self.root = Some(root);
        let claimed = self.claimed.and_then(|p| node(nav, sys, p));
        let mut best_score = match &claimed {
            None => 0.0,
            Some(c) => self.calculate_score(c, now),
        };
        let mut best = self.claimed;
        self.pre_evaluate();
        let mut group_has_los = false;
        for i in 0..self.nearby.tactical.len() {
            let p = self.nearby.tactical[i];
            if Some(p) == self.claimed {
                continue;
            }
            let Some(n) = node(nav, sys, p) else { continue };
            if let Some(team) = enemy_team {
                group_has_los |= has_los_to_team(&n, team as i32);
            }
            if limit_to_claim_group
                && let Some(c) = &claimed
                && n.claim_group != c.claim_group
            {
                continue;
            }
            if !self.can_claim(sys, claimed.as_ref(), &n) {
                continue;
            }
            let score = self.calculate_score(&n, now);
            if score >= 0.0 && score > best_score {
                best = Some(p);
                best_score = score;
            }
        }
        self.searches_with_no_los = if self.claimed.is_some() && group_has_los {
            0
        } else {
            self.searches_with_no_los + 1
        };
        self.post_evaluate();
        if best_score > 0.0 { best } else { None }
    }
}

/// TnTactical.HasLosToTeam.
fn has_los_to_team(n: &TacticalNode, team: i32) -> bool {
    (0..3).contains(&team) && n.state.los_to_team[team as usize] == 255
}

/// TnTactical.GetLosToTeam.
fn los_to_team(n: &TacticalNode, team: i32) -> f32 {
    if (0..3).contains(&team) {
        n.state.los_to_team[team as usize] as f32 * 0.00392
    } else {
        0.0
    }
}

/// TnOcclusionTacticalEvaluator: 1 if the node's cover hides it from the enemy's
/// direction, -1 if not (a corner's half covered side 0.5, its open side or
/// behind it -1; a ledge with the enemy above it -1).
fn occlusion_score(n: &TacticalNode, enemy: Vec3) -> f32 {
    let index = shared::occlusion_index(enemy - n.location);
    let required = 1u32 << index;
    let left = 1u32 << ((index + 1).rem_euclid(32));
    let right = 1u32 << ((index - 1).rem_euclid(32));
    let has = shared::occlusion_compatible(required, n.occlusion);
    let partial = shared::occlusion_compatible(left, n.occlusion)
        || shared::occlusion_compatible(right, n.occlusion);
    let local = super::weapon::to_local(enemy - n.location, 0.0, n.yaw);
    match n.cover {
        Cover::LeftCorner => {
            if local.x < 0.0 {
                return -1.0;
            }
            if local.y > 0.0 && !has && partial {
                return 0.5;
            }
            if local.y < 0.0 {
                return -1.0;
            }
        }
        Cover::RightCorner => {
            if local.x < 0.0 {
                return -1.0;
            }
            if local.y < 0.0 && !has && partial {
                return 0.5;
            }
            if local.y > 0.0 {
                return -1.0;
            }
        }
        Cover::Ledge => {
            if local.z > 0.0 {
                return -1.0;
            }
        }
        _ => {}
    }
    if has { 1.0 } else { -1.0 }
}

/// TnTacticalAttackSensor.
#[derive(Clone, Debug, Default)]
pub struct TacticalAttack {
    pub picker: Picker,
    pub tactical: Option<u32>,
    pub max_duration_at_tactical: f32,
    pub max_wait_to_advance: f32,
    pub time_at_tactical: f32,
    pub time_at_tactical_no_los_to_enemy: f32,
    pub time_since_picked_tactical: f32,
    pub reason: &'static str,
}

impl TacticalAttack {
    pub fn new(max_duration: f32, max_wait: f32) -> TacticalAttack {
        TacticalAttack {
            max_duration_at_tactical: max_duration,
            max_wait_to_advance: max_wait,
            ..Default::default()
        }
    }

    /// OnInitialize (the picker's: the AI is on the battle line).
    pub fn initialize(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        ctx.shared.tactical.add_battle_line_pawn(b.core.entity);
        self.picker.nearby = Collection::default();
    }

    /// OnShutdown.
    pub fn shutdown(&mut self, ctx: &mut Ctx) {
        if let (Some(nav), Some(c)) = (ctx.view.nav, self.picker.claimed)
            && let Some(n) = node(nav, &ctx.shared.tactical, c)
        {
            self.picker.release(&mut ctx.shared.tactical, &n);
        }
        ctx.shared.tactical.remove_battle_line_pawn(ctx.pawn.entity);
    }

    fn update_target(&mut self, b: &Brain, ctx: &mut Ctx) {
        let p = &*ctx.pawn;
        self.picker.update_source(p.entity, p.loc, b.core.team);
        let suspected = b.core.fetch(super::brain::LR_SUSPECTED_ENEMY);
        let target = b
            .core
            .enemy
            .and_then(|e| ctx.view.actor(e))
            .map(|a| (a.entity, suspected, a.team));
        let alive = self
            .picker
            .last_target
            .and_then(|e| ctx.view.actor(e))
            .is_some_and(|a| !a.dead);
        self.picker
            .update_target(&mut ctx.shared.tactical, target, alive);
    }

    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx, dt: f32) {
        self.update_target(b, ctx);
        if b.core.ws(htn::WS_LOCATION) == super::brain::LR_ATTACK_POINT as i32 {
            self.time_at_tactical += dt;
        }
        if self.current_has_los_to_enemy_team(b, ctx) {
            self.time_at_tactical_no_los_to_enemy = 0.0;
        } else {
            self.time_at_tactical_no_los_to_enemy += dt;
        }
        self.time_since_picked_tactical += dt;
        b.core.store(
            super::brain::LR_ATTACK_FOCUS,
            self.picker.target.location,
            Vec3::ZERO,
        );
        if b.core.enemy.is_none() && self.picker.last_target.is_none() {
            return;
        }
        self.find_new_tactical(b, ctx);
    }

    pub fn handle_event(&mut self, b: &mut Brain, ctx: &mut Ctx, e: &Event) {
        if e.id == EventId::Died
            && let (Some(nav), Some(t)) = (ctx.view.nav, self.tactical)
            && let Some(index) = nav.tactical_index(t)
        {
            ctx.shared
                .tactical
                .stinkify(index, nav, ctx.pawn.loc, ctx.view.now);
        }
        if e.id != EventId::BuddySawEnemy && e.id != EventId::Alarm {
            return;
        }
        let Some(sender) = e.actor else { return };
        if !b.sensors.threats.is_allowed(sender) {
            return;
        }
        if b.core.enemy.is_some() && Some(sender) != b.core.enemy {
            return;
        }
        if Some(sender) == b.core.enemy {
            return;
        }
        let Some(a) = ctx.view.actor(sender) else {
            return;
        };
        if b.core.enemy.is_some() || e.id == EventId::BuddySawEnemy {
            b.core
                .store(super::brain::LR_CONFIRMED_ENEMY, a.loc, a.feet);
            b.core
                .store(super::brain::LR_SUSPECTED_ENEMY, a.loc, a.feet);
            b.core.set_ws_bool(htn::WS_ENEMY_LOCATION_SUSPICIOUS, false);
        }
        let p = &*ctx.pawn;
        self.picker.update_source(p.entity, p.loc, b.core.team);
        self.picker.update_target(
            &mut ctx.shared.tactical,
            Some((sender, a.loc, a.team)),
            true,
        );
    }

    /// DoesCurrentTacticalHaveLOSToEnemyTeam.
    fn current_has_los_to_enemy_team(&self, b: &Brain, ctx: &Ctx) -> bool {
        let (Some(t), Some(enemy)) = (self.tactical, b.core.enemy) else {
            return true;
        };
        let Some(nav) = ctx.view.nav else { return true };
        let team = ctx.view.actor(enemy).map_or(0, |a| a.team) as i32;
        node(nav, &ctx.shared.tactical, t).is_some_and(|n| has_los_to_team(&n, team))
    }

    /// AllowedToChangeTactical.
    fn allowed_to_change(&mut self, b: &Brain, ctx: &Ctx) -> bool {
        let Some(t) = self.tactical else { return true };
        if self.time_at_tactical > self.max_duration_at_tactical {
            self.reason = "At same tactical for too long";
            return true;
        }
        if self.time_at_tactical_no_los_to_enemy > self.max_wait_to_advance {
            self.reason = "No line of sight to the enemy for too long";
            return true;
        }
        if self.time_since_picked_tactical < 0.5 {
            return true;
        }
        if b.core.enemy.is_some()
            && let Some(nav) = ctx.view.nav
            && let Some(n) = node(nav, &ctx.shared.tactical, t)
            && self.picker.calculate_score(&n, ctx.view.now) <= 0.0
        {
            self.reason = "Tactical had invalid score";
            return true;
        }
        false
    }

    /// FindNewTactical.
    fn find_new_tactical(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let Some(nav) = ctx.view.nav else { return };
        let Some(root) = ctx.pawn.anchor else { return };
        let limit = !self.allowed_to_change(b, ctx);
        let enemy_team = b.core.enemy.and_then(|e| ctx.view.actor(e)).map(|a| a.team);
        let best = self.picker.find_best_tactical(
            nav,
            &ctx.shared.tactical,
            root,
            ctx.pawn.loc,
            limit,
            enemy_team,
            ctx.view.now,
        );
        self.claim_tactical(b, ctx, best);
    }

    /// ClaimTactical.
    fn claim_tactical(&mut self, b: &mut Brain, ctx: &mut Ctx, new: Option<u32>) {
        if new == self.tactical {
            return;
        }
        let Some(nav) = ctx.view.nav else { return };
        self.clear_current(ctx);
        self.tactical = new;
        let n = new.and_then(|p| node(nav, &ctx.shared.tactical, p));
        if let Some(n) = &n {
            self.picker.claim(&mut ctx.shared.tactical, n);
            b.core
                .store(super::brain::LR_ATTACK_POINT, n.location, Vec3::ZERO);
        }
        if b.core.ws(htn::WS_LOCATION) == super::brain::LR_ATTACK_POINT as i32 {
            b.core.set_ws(htn::WS_LOCATION, 0);
        }
        b.core
            .set_ws_bool(htn::WS_ATTACK_POINT_AVAILABLE, n.is_some());
        b.core.set_ws_ctx(
            htn::WS_ATTACK_POINT_AVAILABLE,
            n.map_or(0, |n| n.cover as i32),
        );
    }

    /// ClearCurrentTactical.
    fn clear_current(&mut self, ctx: &mut Ctx) {
        if let (Some(t), Some(nav)) = (self.tactical, ctx.view.nav)
            && let Some(n) = node(nav, &ctx.shared.tactical, t)
        {
            self.picker.release(&mut ctx.shared.tactical, &n);
        }
        self.tactical = None;
        self.time_at_tactical = 0.0;
        self.time_at_tactical_no_los_to_enemy = 0.0;
        self.time_since_picked_tactical = 0.0;
    }

    /// The node it holds (for the cover operators).
    pub fn current(&self, ctx: &Ctx) -> Option<TacticalNode> {
        let nav = ctx.view.nav?;
        node(nav, &ctx.shared.tactical, self.tactical?)
    }
}
