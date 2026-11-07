//! An enemy's senses: its HtnSensorCollection (the sensors by update frequency,
//! those updated before it plans, events handed to each in turn) and the
//! soldiers' sensors, each a port of its TnXxxSensor:
//!
//! * search (where to search next), hearing (noises, and alarms from its own side),
//!   pathing (the places it couldn't find a path to), path following (path.rs), vision
//!   (who it sees, as soon as its perception delays let it), threat (which of the
//!   threats it knows of is its enemy, by its evaluators' scores), pain (who hurt it
//!   and how), hit reaction (hits.rs), weapon (armed, aiming, loaded), enemy status
//!   (how far away, how hurt, in the air or not...), clear attack channel (a way
//!   open to melee its enemy), tactical attack (tactical.rs: the cover to fight
//!   from) and competency (how well it shoots);
//! * the threat data store (TnThreatDataStore: the threats it knows of, where each
//!   was last seen and where it is thought to be);
//! * the engine's own sight check that tells the controller it sees someone
//!   (SeeEnemy, EnemyNotVisible) and LinesOfSightManager's line of sight.

use bevy::prelude::*;

use super::brain::{Brain, Ctx, DamageKind, Event, EventId};
use super::htn;
use super::pawn::{Pawn, Physics};
use super::ue::{self, Rng};
use super::world::{ActorInfo, WorldView};

// AI.ESensor (the soldiers' and what the other types add).
pub const S_HEARING: u8 = 1;
pub const S_VISION: u8 = 2;
pub const S_ENEMY_STATUS: u8 = 3;
pub const S_SEARCH: u8 = 4;
pub const S_PAIN: u8 = 5;
pub const S_WEAPON: u8 = 6;
pub const S_CLEAR_ATTACK_CHANNEL: u8 = 8;
pub const S_PATHING: u8 = 10;
pub const S_TACTICAL_ATTACK: u8 = 11;
pub const S_SQUAD: u8 = 12;
pub const S_INJURED: u8 = 13;
pub const S_NAV_MESH: u8 = 14;
pub const S_LEAPER_SMASH_VOLUME: u8 = 15;
pub const S_HEALTH_LEVEL: u8 = 18;
pub const S_COMPETENCY: u8 = 20;
pub const S_HIT_REACTION: u8 = 22;
pub const S_THREAT: u8 = 23;
pub const S_SPECIAL_ATTACK: u8 = 24;
pub const S_PATH_FOLLOWING: u8 = 25;
pub const S_LOS_SEARCH: u8 = 27;
pub const S_GUARD: u8 = 28;
pub const S_ATTACK_POINT: u8 = 45;
pub const S_ENEMY_KITING: u8 = 48;
pub const MAX_SENSORS: usize = 64;

/// HtnSensorCollection.EUpdateFrequency's groups: every frame, 0.1 s, 0.5 s, 1 s,
/// an hour.
pub const GROUP_FREQUENCIES: [f32; 5] = [0.0, 0.1, 0.5, 1.0, 3600.0];

/// HtnSensorGroup.
#[derive(Clone, Debug, Default)]
pub struct Group {
    pub sensors: Vec<u8>,
    pub frequency: f32,
    pub time_before_next_update: f32,
}

// ---------------------------------------------------------------------------
// The threat data store
// ---------------------------------------------------------------------------

/// TnThreatDataStore.TnThreatInfo.
#[derive(Clone, Copy, Debug)]
pub struct ThreatInfo {
    pub threat: Entity,
    pub visible: bool,
    pub confirmed: Vec3,
    pub confirmed_ground: Vec3,
    pub suspected: Vec3,
    pub suspected_ground: Vec3,
    pub reachable: bool,
    pub time_since_unreachable: f32,
    pub damage: f32,
    pub last_damage_type: DamageKind,
    pub last_damaged_time: f32,
}

/// TnThreatDataStore.
#[derive(Clone, Debug, Default)]
pub struct ThreatStore {
    pub threats: Vec<ThreatInfo>,
    pub disallowed: Vec<Entity>,
}

impl ThreatStore {
    pub fn num(&self) -> usize {
        self.threats.len()
    }

    fn find(&self, e: Entity) -> Option<usize> {
        self.threats.iter().position(|t| t.threat == e)
    }

    pub fn info(&self, e: Entity) -> Option<&ThreatInfo> {
        self.threats.iter().find(|t| t.threat == e)
    }

    pub fn is_registered(&self, e: Entity) -> bool {
        self.find(e).is_some()
    }

    /// IsThreatAllowed.
    pub fn is_allowed(&self, e: Entity) -> bool {
        !self.disallowed.contains(&e)
    }

    /// AddThreat.
    pub fn add(&mut self, view: &WorldView, e: Entity, visible: bool) {
        if !self.is_allowed(e) || self.find(e).is_some() {
            return;
        }
        let Some(a) = view.actor(e) else { return };
        self.threats.push(ThreatInfo {
            threat: e,
            visible,
            confirmed: a.loc,
            confirmed_ground: a.feet,
            suspected: a.loc,
            suspected_ground: a.feet,
            reachable: true,
            time_since_unreachable: 0.0,
            damage: 0.0,
            last_damage_type: DamageKind::Other,
            last_damaged_time: 0.0,
        });
    }

    /// UpdateThreatPosition.
    pub fn update_position(&mut self, view: &WorldView, e: Entity) {
        let Some(i) = self.find(e) else {
            self.add(view, e, false);
            return;
        };
        if let Some(a) = view.actor(e) {
            let t = &mut self.threats[i];
            t.confirmed = a.loc;
            t.confirmed_ground = a.feet;
            t.suspected = t.confirmed;
            t.suspected_ground = t.confirmed_ground;
        }
    }

    /// UpdateThreatVisibility.
    pub fn update_visibility(&mut self, view: &WorldView, e: Entity, visible: bool) {
        match self.find(e) {
            None => self.add(view, e, visible),
            Some(i) => self.update_visibility_at(view, i, visible),
        }
    }

    /// UpdateThreatVisibilityAtIndex.
    pub fn update_visibility_at(&mut self, view: &WorldView, i: usize, visible: bool) {
        let Some(t) = self.threats.get_mut(i) else {
            return;
        };
        t.visible = visible;
        if visible && let Some(a) = view.actor(t.threat) {
            t.confirmed = a.loc;
            t.confirmed_ground = a.feet;
            t.suspected = t.confirmed;
            t.suspected_ground = t.confirmed_ground;
        }
    }

    /// SetReachability.
    pub fn set_reachability(&mut self, e: Entity, reachable: bool) {
        if let Some(i) = self.find(e) {
            self.threats[i].reachable = reachable;
            if !reachable {
                self.threats[i].time_since_unreachable = 0.0;
            }
        }
    }

    /// UpdateThreatReachability.
    pub fn update_reachability(&mut self, dt: f32, between: f32) {
        for t in self.threats.iter_mut() {
            if t.reachable {
                continue;
            }
            t.time_since_unreachable += dt;
            if t.time_since_unreachable > between {
                t.reachable = true;
            }
        }
    }

    /// UpdateSuspectedLocation: the nearest threat (unseen) is thought to be at
    /// `loc`; false if there is none.
    pub fn update_suspected(&mut self, view: &WorldView, loc: Vec3, ground: Vec3) -> bool {
        let mut best = None;
        let mut best_d = f32::MAX;
        for (i, t) in self.threats.iter().enumerate() {
            let Some(a) = view.actor(t.threat) else {
                continue;
            };
            let d = (a.loc - loc).length_squared();
            if d < best_d {
                best_d = d;
                best = Some(i);
            }
        }
        let Some(i) = best else { return false };
        if !self.threats[i].visible {
            self.threats[i].suspected = loc;
            self.threats[i].suspected_ground = ground;
        }
        true
    }

    /// UpdateDamageByThreat.
    pub fn update_damage(&mut self, e: Entity, damage: f32, kind: DamageKind, now: f32) {
        if let Some(i) = self.find(e) {
            let t = &mut self.threats[i];
            t.damage += damage;
            t.last_damage_type = kind;
            t.last_damaged_time = now;
        }
    }

    /// DisallowThreat.
    pub fn disallow(&mut self, e: Entity) {
        if self.disallowed.contains(&e) {
            return;
        }
        self.disallowed.push(e);
        if let Some(i) = self.find(e) {
            self.threats.remove(i);
        }
    }

    pub fn allow(&mut self, e: Entity) {
        self.disallowed.retain(|x| *x != e);
    }

    /// GetNumPotentialThreats: those not on `team`.
    pub fn num_potential(&self, view: &WorldView, team: u8) -> usize {
        self.threats
            .iter()
            .filter(|t| view.actor(t.threat).is_none_or(|a| a.team != team))
            .count()
    }
}

/// TnPatrolDataStore: the patrol route's points and the one it is at.
#[derive(Clone, Debug, Default)]
pub struct PatrolStore {
    pub route: Vec<(Vec3, f32)>,
    pub patrol_type: u8,
    pub index: i32,
}

// ---------------------------------------------------------------------------
// The sensors
// ---------------------------------------------------------------------------

/// TnSearchSensor.
#[derive(Clone, Debug)]
pub struct SearchSensor {
    pub distance_weighting: (f32, f32),
    pub age_weighting: (f32, f32),
    pub points: Vec<Vec3>,
    pub weights: Vec<f32>,
    /// TnSquadSearchDataStore's searches (each point's last search time).
    pub searched: Vec<(Vec3, f32)>,
}

impl Default for SearchSensor {
    fn default() -> Self {
        SearchSensor {
            distance_weighting: (100.0, 15000.0),
            age_weighting: (10.0, 30.0),
            points: Vec::new(),
            weights: Vec::new(),
            searched: Vec::new(),
        }
    }
}

impl SearchSensor {
    fn update(&mut self, b: &mut Brain) {
        b.core
            .set_ws_bool(htn::WS_SEARCH_AVAILABLE, !self.points.is_empty());
    }

    /// ChooseNewSearchPoint.
    pub fn choose_new_search_point(&mut self, b: &mut Brain, location: Vec3, now: f32) {
        if self.points.is_empty() {
            return;
        }
        self.weights = self
            .points
            .iter()
            .map(|p| {
                let distance = (*p - location).length();
                let dw = if distance < 100.0 {
                    0.0
                } else {
                    1.0 - ue::clamp_pct_by_range(
                        distance,
                        self.distance_weighting.0,
                        self.distance_weighting.1,
                    )
                };
                let last = self
                    .searched
                    .iter()
                    .find(|(q, _)| q == p)
                    .map_or(0.0, |(_, t)| *t);
                let aw =
                    ue::clamp_pct_by_range(now - last, self.age_weighting.0, self.age_weighting.1);
                dw + aw
            })
            .collect();
        let mut best = 0;
        for i in 1..self.points.len() {
            if self.weights[i] > self.weights[best] {
                best = i;
            }
        }
        let point = self.points[best];
        b.core
            .store(super::brain::LR_SEARCH_POINT, point, Vec3::ZERO);
        match self.searched.iter_mut().find(|(q, _)| *q == point) {
            Some(s) => s.1 = now,
            None => self.searched.push((point, now)),
        }
    }
}

/// TnHearingSensor.
#[derive(Clone, Debug)]
pub struct HearingSensor {
    pub recently_heard_threshold: f32,
    pub lose_heard_target_threshold: f32,
    pub time_since_last_state_change: f32,
    pub last_heard_type: Option<&'static str>,
    pub last_heard_location: Vec3,
}

impl HearingSensor {
    pub fn new(recently: f32, lose: f32) -> HearingSensor {
        HearingSensor {
            recently_heard_threshold: recently,
            lose_heard_target_threshold: lose,
            time_since_last_state_change: 3600.0,
            last_heard_type: None,
            last_heard_location: Vec3::ZERO,
        }
    }

    fn update(&mut self, b: &mut Brain, dt: f32) {
        if !b.core.ws_bool(htn::WS_HEARD_NOISE) {
            return;
        }
        if self.time_since_last_state_change > self.lose_heard_target_threshold {
            b.core.set_ws_bool(htn::WS_HEARD_NOISE, false);
            return;
        }
        self.update_recently_heard(b, dt);
    }

    fn update_recently_heard(&mut self, b: &mut Brain, dt: f32) {
        self.time_since_last_state_change += dt;
        let recent = self.time_since_last_state_change < self.recently_heard_threshold;
        b.core.set_ws_ctx(htn::WS_HEARD_NOISE, recent as i32);
    }

    fn handle_event(&mut self, b: &mut Brain, ctx: &mut Ctx, e: &Event) {
        let Some(noisy) = e.actor else { return };
        // IsRelevantNoiseEvent.
        let relevant = match e.id {
            EventId::Alarm | EventId::Bump => true,
            EventId::HeardEnemy => b.sensors.threats.is_allowed(noisy),
            _ => false,
        };
        if !relevant {
            return;
        }
        // ShouldAlertToEnemyPresence: a shot heard.
        if e.noise == Some("FireAmmunition") {
            b.sensors.threats.add(ctx.view, noisy, false);
        }
        self.handle_heard_enemy(b, ctx, noisy, e.noise);
    }

    /// HandleHeardEnemy.
    fn handle_heard_enemy(
        &mut self,
        b: &mut Brain,
        ctx: &mut Ctx,
        noisy: Entity,
        kind: Option<&'static str>,
    ) {
        let Some(a) = ctx.view.actor(noisy) else {
            return;
        };
        self.last_heard_location = a.loc;
        self.last_heard_type = kind;
        self.time_since_last_state_change = 0.0;
        b.core.set_ws_bool(htn::WS_HEARD_NOISE, true);
        self.update_recently_heard(b, 0.0);
        let location = a.nav_near + Vec3::Z * a.half_height;
        let navigable = a.nav_near;
        if !b
            .sensors
            .threats
            .update_suspected(ctx.view, location, navigable)
        {
            b.core.set_ws_bool(htn::WS_ENEMY_LOCATION_SUSPICIOUS, true);
            b.core
                .store(super::brain::LR_SUSPECTED_ENEMY, location, navigable);
            b.core.store(
                super::brain::LR_NAVIGABLE_LOCATION_NEAR_ENEMY,
                location,
                navigable,
            );
        }
    }
}

/// TnPathingSensor.
#[derive(Clone, Debug)]
pub struct PathingSensor {
    pub time_between_pathing_attempts: f32,
    /// Each location reference's TnPathBlockedInfo: the time since its path was
    /// blocked and the time between attempts.
    pub blocked: [(f32, f32); super::brain::LR_NUM],
}

impl Default for PathingSensor {
    fn default() -> Self {
        PathingSensor {
            time_between_pathing_attempts: 2.0,
            blocked: [(0.0, 2.0); super::brain::LR_NUM],
        }
    }
}

impl PathingSensor {
    /// TnPathingSensor's constructor: its time between pathing attempts, each
    /// location reference's too (OnInitialize).
    pub fn new(time_between_pathing_attempts: f32) -> PathingSensor {
        PathingSensor {
            time_between_pathing_attempts,
            blocked: [(0.0, time_between_pathing_attempts); super::brain::LR_NUM],
        }
    }

    /// PathBlocked.
    pub fn path_blocked(&mut self, b: &mut Brain, p: &Pawn, lr: usize) {
        if !p.can_anchor(b.core.is_active()) || lr >= self.blocked.len() {
            return;
        }
        self.blocked[lr].0 = 0.0;
        b.core.set_ws_bit(htn::WS_PATH_BLOCKED, lr as i32, true);
        if let Some(enemy) = b.core.enemy
            && (lr == super::brain::LR_CONFIRMED_ENEMY
                || lr == super::brain::LR_NAVIGABLE_LOCATION_NEAR_ENEMY)
        {
            b.sensors.threats.set_reachability(enemy, false);
        }
    }

    fn update(&mut self, b: &mut Brain, dt: f32) {
        for (i, (since, between)) in self.blocked.iter_mut().enumerate() {
            *since += dt;
            if *since > *between {
                b.core.set_ws_bit(htn::WS_PATH_BLOCKED, i as i32, false);
            }
        }
        b.sensors
            .threats
            .update_reachability(dt, self.time_between_pathing_attempts);
    }
}

/// AI.VisionProfile.
#[derive(Clone, Copy, Debug, Default)]
pub struct VisionProfile {
    pub fov: f32,
    pub distance: f32,
    pub half_height: f32,
    pub distance_perception_delay: f32,
    pub peripheral_perception_delay: f32,
}

/// TnVisionSensor.
#[derive(Clone, Debug)]
pub struct VisionSensor {
    pub profiles: Vec<VisionProfile>,
    pub active_profile: i32,
    pub possible_threats: Vec<(Entity, f32)>,
    pub time_until_enemy_registered: f32,
}

impl Default for VisionSensor {
    fn default() -> Self {
        VisionSensor {
            profiles: vec![VisionProfile::default()],
            active_profile: -1,
            possible_threats: Vec::new(),
            time_until_enemy_registered: f32::MAX,
        }
    }
}

impl VisionSensor {
    pub fn set_profile(&mut self, awareness: usize, p: VisionProfile) {
        if self.profiles.len() < awareness + 1 {
            self.profiles
                .resize(awareness + 1, VisionProfile::default());
        }
        self.profiles[awareness] = p;
    }

    /// UpdateActiveProfile.
    fn update_active_profile(&mut self, b: &Brain, p: &mut Pawn) {
        let alert = b.core.ws(htn::WS_ALERT_LEVEL);
        if self.active_profile == alert || alert < 0 || alert as usize >= self.profiles.len() {
            return;
        }
        self.active_profile = alert;
        let prof = self.profiles[alert as usize];
        p.vision_half_height = prof.half_height;
        p.peripheral_vision = (prof.fov * ue::DEG_TO_RAD * 0.5).cos();
        p.sight_radius = prof.distance;
    }

    fn update(&mut self, b: &mut Brain, ctx: &mut Ctx, dt: f32) {
        self.update_active_profile(b, ctx.pawn);
        self.update_pending_threats(b, ctx, dt);
        for i in 0..b.sensors.threats.num() {
            let threat = b.sensors.threats.threats[i].threat;
            let visible = can_see_pawn(ctx, threat, false);
            b.sensors.threats.update_visibility_at(ctx.view, i, visible);
            if visible && Some(threat) == b.core.enemy {
                b.core.set_ws_bool(htn::WS_ENEMY_VISIBLE, visible);
                ctx.shared
                    .squad
                    .broadcast(EventId::BuddySawEnemy, Some(threat));
            }
        }
    }

    fn handle_event(&mut self, b: &mut Brain, ctx: &mut Ctx, e: &Event) {
        let Some(enemy) = e.actor else { return };
        match e.id {
            EventId::BuddySawEnemy => b.sensors.threats.update_position(ctx.view, enemy),
            EventId::SawEnemy => self.handle_saw_enemy(b, ctx, enemy),
            _ => {}
        }
    }

    /// RefreshLineOfSight.
    pub fn refresh_line_of_sight(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let Some(enemy) = b.core.enemy else { return };
        let visible = can_see_pawn(ctx, enemy, true);
        b.sensors
            .threats
            .update_visibility(ctx.view, enemy, visible);
    }

    /// HandleSawEnemy: the delay before it registers someone seen (longer the
    /// farther away and the further out of its view's middle).
    fn handle_saw_enemy(&mut self, b: &mut Brain, ctx: &mut Ctx, enemy: Entity) {
        if !is_targetable(b, enemy) || b.sensors.threats.is_registered(enemy) {
            return;
        }
        let Some(a) = ctx.view.actor(enemy) else {
            return;
        };
        let dir = a.loc - ctx.pawn.loc;
        let distance = dir.length().max(1e-3);
        let cos = (dir / distance).dot(ctx.pawn.look_direction());
        let prof = self.profiles[self.active_profile.max(0) as usize % self.profiles.len()];
        let periphery = 1.0 + (1.0 - cos) * prof.peripheral_perception_delay;
        let desired = prof
            .distance_perception_delay
            .max(prof.distance_perception_delay * distance * 0.001 * periphery);
        match self.possible_threats.iter_mut().find(|(e, _)| *e == enemy) {
            None => self.possible_threats.push((enemy, desired)),
            Some(t) => t.1 = t.1.min(desired),
        }
    }

    /// UpdatePendingThreats.
    fn update_pending_threats(&mut self, b: &mut Brain, ctx: &mut Ctx, dt: f32) {
        self.time_until_enemy_registered = f32::MAX;
        let mut i = self.possible_threats.len();
        while i > 0 {
            i -= 1;
            let (e, _) = self.possible_threats[i];
            let gone = ctx.view.actor(e).is_none_or(|a| a.dead);
            if gone || !can_see_pawn(ctx, e, false) {
                self.possible_threats.remove(i);
                continue;
            }
            self.possible_threats[i].1 -= dt;
            self.time_until_enemy_registered = self
                .time_until_enemy_registered
                .min(self.possible_threats[i].1);
            if self.possible_threats[i].1 > 0.0 {
                continue;
            }
            // RegisterEnemy.
            b.sensors.threats.update_visibility(ctx.view, e, true);
            self.possible_threats.remove(i);
        }
        if b.sensors.threats.num() > 0 {
            self.time_until_enemy_registered = 0.0;
        }
    }
}

/// TnVisionSensor.IsTargetable (a player's pawn: targetable unless hiding outside
/// a fight; none of the demo's players hide).
fn is_targetable(_b: &Brain, _e: Entity) -> bool {
    true
}

/// A threat evaluator (TnThreatEvaluator's subclasses the soldiers use) and its
/// weight.
#[derive(Clone, Debug)]
pub enum ThreatEval {
    Downed,
    /// The abilities that draw its fire (EscalationAgroAbilities: the shield).
    AbilityAgro,
    Cloaked {
        discard: bool,
    },
    Sentry {
        discard: bool,
    },
    DamagedBy,
    Flanked {
        range_sq: f32,
    },
    VisibleFromCover,
    PlayerEnemy {
        score: f32,
    },
    EnemyAttack,
    AttackEffectiveness,
    EnemyDistance {
        max_range: f32,
    },
    /// How few of its own type its squad has on the threat.
    EnemyTypeDistribution {
        ai_type: i32,
    },
    /// How few of its squad are on the threat.
    TargetDistribution,
    /// A threat in flight (a jet's): 0.
    Flying {
        discard: bool,
    },
    /// A threat it can't reach: 0.
    Reachable {
        discard: bool,
    },
    /// A threat out of sight: 0.
    Visible {
        discard: bool,
    },
    /// Its current enemy: its score.
    CurrentEnemy {
        score: f32,
    },
    /// A threat whose place on the navigation mesh is fresh: its score.
    NavigableLocationAge {
        score: f32,
        threshold: f32,
    },
}

/// TnNavigableLocationAgeThreatEvaluator's _RecentlyUpdatedThreshold (s).
pub const RECENTLY_UPDATED_THRESHOLD: f32 = 3.0;

/// AI.RPNInput: 1 a value, 2 add, 3 multiply.
#[derive(Clone, Copy, Debug)]
pub enum Rpn {
    Value(usize),
    Add,
    Mul,
}

/// Math.RPNEvaluate.
pub fn rpn_evaluate(inputs: &[Rpn], values: &[f32]) -> f32 {
    let mut stack: Vec<f32> = Vec::with_capacity(inputs.len());
    for i in inputs {
        match i {
            Rpn::Value(k) => stack.push(values.get(*k).copied().unwrap_or(0.0)),
            Rpn::Add | Rpn::Mul => {
                let r = stack.pop().unwrap_or(0.0);
                let l = stack.pop().unwrap_or(0.0);
                stack.push(if matches!(i, Rpn::Add) { l + r } else { l * r });
            }
        }
    }
    stack.first().copied().unwrap_or(0.0)
}

/// TnThreatSensor.
#[derive(Clone, Debug)]
pub struct ThreatSensor {
    pub evaluators: Vec<(ThreatEval, f32)>,
    pub eval_results: Vec<f32>,
    pub rpn: Vec<Rpn>,
    pub time_to_lose_enemies: f32,
    pub current_time_with_no_visible_enemies: f32,
    pub recently_changed_visibility_duration: f32,
    pub recently_changed_visibility_time_remaining: f32,
    pub best_threat: Option<Entity>,
    pub biggest_threat_override: Option<Entity>,
    pub biggest_threat: i32,
    /// The type's effective attack range and score (its blueprint's
    /// get_EffectiveAttackRange and get_EffectiveAttackScore).
    pub effective_attack_range: f32,
    pub effective_attack_score: f32,
}

impl ThreatSensor {
    pub fn new(time_to_lose: f32) -> ThreatSensor {
        ThreatSensor {
            evaluators: Vec::new(),
            eval_results: Vec::new(),
            rpn: Vec::new(),
            time_to_lose_enemies: time_to_lose,
            current_time_with_no_visible_enemies: 0.0,
            recently_changed_visibility_duration: 0.0,
            recently_changed_visibility_time_remaining: 0.0,
            best_threat: None,
            biggest_threat_override: None,
            biggest_threat: -1,
            effective_attack_range: 0.0,
            effective_attack_score: 1.0,
        }
    }

    pub fn add_rpn_evaluator(&mut self, e: ThreatEval, weight: f32) {
        self.evaluators.push((e, weight));
        self.eval_results.resize(self.evaluators.len(), 0.0);
        self.rpn.push(Rpn::Value(self.evaluators.len() - 1));
    }

    pub fn add_rpn_operator(&mut self, op: Rpn) {
        self.rpn.push(op);
    }

    /// AddEvaluator: one its scores are summed with (no RPN).
    pub fn add_evaluator(&mut self, e: ThreatEval, weight: f32) {
        self.evaluators.push((e, weight));
        self.eval_results.resize(self.evaluators.len(), 0.0);
    }

    fn member(b: &Brain) -> super::shared::MemberInfo {
        super::shared::MemberInfo {
            entity: b.core.entity,
            ai_type: b.core.ai_type,
        }
    }

    /// ResetThreats.
    pub fn reset_threats(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        self.biggest_threat = -1;
        self.clear_enemy(b, ctx);
        self.update_enemy_visibility(b, -1, 0.0);
        b.core.set_ws_bool(htn::WS_ALERT_TO_ENEMY_PRESENCE, false);
        b.core.set_ws_bool(htn::WS_ENEMY_LOCATION_SUSPICIOUS, false);
        b.core
            .store(super::brain::LR_CONFIRMED_ENEMY, Vec3::ZERO, Vec3::ZERO);
        b.core
            .store(super::brain::LR_SUSPECTED_ENEMY, Vec3::ZERO, Vec3::ZERO);
    }

    fn update(&mut self, b: &mut Brain, ctx: &mut Ctx, dt: f32) {
        let mut highest = -f32::MAX;
        if let Some(over) = self.biggest_threat_override {
            ctx.shared
                .squad
                .engaging_target(Self::member(b), Some(over), self.best_threat);
            self.best_threat = Some(over);
            b.core.enemy = Some(over);
            if let Some(a) = ctx.view.actor(over) {
                b.core
                    .store(super::brain::LR_CONFIRMED_ENEMY, a.loc, a.feet);
                b.core
                    .store(super::brain::LR_SUSPECTED_ENEMY, a.loc, a.feet);
            }
            return;
        }
        self.biggest_threat = -1;
        // PruneThreats.
        b.sensors
            .threats
            .threats
            .retain(|t| ctx.view.actor(t.threat).is_some_and(|a| !a.dead));
        let mut any_visible = false;
        for i in 0..b.sensors.threats.num() {
            let info = b.sensors.threats.threats[i];
            if ctx.view.same_team(ctx.pawn.entity, info.threat) {
                continue;
            }
            any_visible |= info.visible;
            for k in 0..self.evaluators.len() {
                let (score, discard) = self.evaluate(k, b, ctx, &info);
                self.eval_results[k] = score;
                if discard {
                    self.eval_results[k] = -f32::MAX;
                    break;
                }
            }
            let level = self.sum_scores();
            if level > highest {
                highest = level;
                self.biggest_threat = i as i32;
            }
        }
        ctx.shared.squad.update_threats_visible(any_visible);
        if self.biggest_threat != -1 {
            self.update_current_enemy(b, ctx);
        } else {
            self.clear_enemy(b, ctx);
        }
        self.update_enemy_visibility(b, self.biggest_threat, dt);
        let potential = b.sensors.threats.num_potential(ctx.view, b.core.team);
        b.core
            .set_ws_bool(htn::WS_ALERT_TO_ENEMY_PRESENCE, potential != 0);
        self.current_time_with_no_visible_enemies = if any_visible {
            self.time_to_lose_enemies
        } else {
            self.current_time_with_no_visible_enemies - dt
        };
        let ctxv = if self.current_time_with_no_visible_enemies > 0.0 {
            0
        } else {
            1
        };
        b.core.set_ws_ctx(htn::WS_ALERT_TO_ENEMY_PRESENCE, ctxv);
    }

    /// SumEvaluatorScores.
    fn sum_scores(&self) -> f32 {
        if !self.rpn.is_empty() {
            return rpn_evaluate(&self.rpn, &self.eval_results);
        }
        self.eval_results.iter().sum()
    }

    /// The evaluator's CalculateThreatLevel (its weight times its own score), and
    /// whether it discards the threat.
    fn evaluate(&self, k: usize, b: &Brain, ctx: &Ctx, t: &ThreatInfo) -> (f32, bool) {
        let (eval, weight) = &self.evaluators[k];
        let player = ctx.view.player(t.threat);
        let p = &*ctx.pawn;
        let mut discard = false;
        let score = match eval {
            ThreatEval::Downed => player.map_or(1.0, |pl| if pl.downed { 0.0 } else { 1.0 }),
            ThreatEval::AbilityAgro => player.map_or(1.0, |pl| if pl.shielded { 2.0 } else { 1.0 }),
            ThreatEval::Cloaked { discard: d } => match player {
                None => 1.0,
                Some(pl) => {
                    discard = *d && pl.cloaked;
                    if pl.cloaked { 0.0 } else { 1.0 }
                }
            },
            // (No sentries among the threats: the players' are no TnSentryPawn the
            // AI hunts.)
            ThreatEval::Sentry { .. } => 1.0,
            ThreatEval::DamagedBy => {
                let time_scale = 1.0 - (1.0f32).min((ctx.view.now - t.last_damaged_time) / 5.0);
                t.damage / p.health_max.max(1.0) * time_scale
            }
            ThreatEval::Flanked { range_sq } => {
                let to = t.suspected - p.loc;
                if p.vel != Vec3::ZERO {
                    0.0
                } else if to.length_squared() < *range_sq && ue::normal(to).dot(p.forward()) < 0.707
                {
                    1.0
                } else {
                    0.0
                }
            }
            ThreatEval::VisibleFromCover => {
                if b.core.ws(htn::WS_COVER_USAGE) != 2 {
                    1.0
                } else if t.visible {
                    2.0
                } else {
                    0.0
                }
            }
            ThreatEval::PlayerEnemy { score } => {
                if player.is_none() {
                    0.0
                } else {
                    let mut n = ctx.shared.squad.total_engaging(t.threat, 0);
                    if Some(t.threat) == b.core.enemy {
                        n -= 1;
                    }
                    if n <= 0 { *score } else { 0.0 }
                }
            }
            // (A player's pawn is no TnHtnAiPawn.)
            ThreatEval::EnemyAttack => {
                if player.is_some() {
                    1.0
                } else {
                    0.0
                }
            }
            ThreatEval::AttackEffectiveness => {
                let Some(a) = ctx.view.actor(t.threat) else {
                    return (0.0, false);
                };
                if (p.loc - a.loc).length_squared()
                    > self.effective_attack_range * self.effective_attack_range
                {
                    0.0
                } else {
                    self.effective_attack_score
                }
            }
            ThreatEval::EnemyDistance { max_range } => {
                let Some(a) = ctx.view.actor(t.threat) else {
                    return (0.0, false);
                };
                (1.0 - (p.loc - a.loc).length() / max_range).max(0.0)
            }
            ThreatEval::EnemyTypeDistribution { ai_type } => {
                let squad = &ctx.shared.squad;
                let mut n = squad.total_engaging(t.threat, *ai_type as usize) as f32;
                if Some(t.threat) == b.core.enemy {
                    n -= 1.0;
                }
                let of_type = squad.number_of_type(*ai_type).max(1) as f32;
                (1.0 - n / of_type).max(0.0) + 1.0
            }
            ThreatEval::TargetDistribution => {
                let squad = &ctx.shared.squad;
                let mut n = squad.total_engaging(t.threat, 0) as f32;
                if Some(t.threat) == b.core.enemy {
                    n -= 1.0;
                }
                (1.0 - n / squad.size().max(1) as f32).max(0.0)
            }
            ThreatEval::Flying { discard: d } => {
                // (A player's jet: in its other form, a plane.)
                let flying = player.is_some_and(|pl| pl.form == 2);
                discard = *d && flying;
                if flying { 0.0 } else { 1.0 }
            }
            ThreatEval::Reachable { discard: d } => {
                discard = *d && !t.reachable;
                if t.reachable { 1.0 } else { 0.0 }
            }
            ThreatEval::Visible { discard: d } => {
                discard = *d && !t.visible;
                if t.visible { 1.0 } else { 0.0 }
            }
            ThreatEval::CurrentEnemy { score } => {
                if Some(t.threat) == b.core.enemy {
                    *score
                } else {
                    0.0
                }
            }
            ThreatEval::NavigableLocationAge { score, threshold } => match player {
                None => 0.0,
                Some(pl) => {
                    if ctx.view.now - pl.nav_near_time <= *threshold {
                        *score
                    } else {
                        0.0
                    }
                }
            },
        };
        (weight * score, discard)
    }

    fn handle_event(&mut self, b: &mut Brain, e: &Event) {
        if e.id == EventId::AreaClear {
            b.sensors.threats.threats.clear();
            let any = b.sensors.threats.num() != 0;
            b.core.set_ws_bool(htn::WS_ALERT_TO_ENEMY_PRESENCE, any);
        }
    }

    fn clear_enemy(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        ctx.shared
            .squad
            .engaging_target(Self::member(b), None, self.best_threat);
        self.best_threat = None;
        b.core.enemy = None;
    }

    fn update_current_enemy(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let i = self
            .biggest_threat
            .clamp(0, b.sensors.threats.num() as i32 - 1) as usize;
        let best = b.sensors.threats.threats[i];
        ctx.shared
            .squad
            .engaging_target(Self::member(b), Some(best.threat), self.best_threat);
        self.best_threat = Some(best.threat);
        b.core.enemy = Some(best.threat);
        b.core.store(
            super::brain::LR_CONFIRMED_ENEMY,
            best.confirmed,
            best.confirmed_ground,
        );
        b.core.store(
            super::brain::LR_SUSPECTED_ENEMY,
            best.suspected,
            best.suspected_ground,
        );
        // AI.IsSuspicious: thought to be more than 1000 uu from where it was seen.
        let suspicious = (best.suspected - best.confirmed).length() > 1000.0;
        b.core
            .set_ws_bool(htn::WS_ENEMY_LOCATION_SUSPICIOUS, suspicious);
    }

    /// UpdateEnemyVisibility.
    fn update_enemy_visibility(&mut self, b: &mut Brain, index: i32, dt: f32) {
        let visible = index >= 0
            && b.sensors
                .threats
                .threats
                .get(index as usize)
                .is_some_and(|t| t.visible);
        let was = b.core.ws_bool(htn::WS_ENEMY_VISIBLE);
        if visible != was {
            self.recently_changed_visibility_time_remaining =
                self.recently_changed_visibility_duration;
        }
        self.recently_changed_visibility_time_remaining -= dt;
        let ctxv = if self.recently_changed_visibility_time_remaining > 0.0 {
            1
        } else {
            0
        };
        b.core.set_ws_bool(htn::WS_ENEMY_VISIBLE, visible);
        b.core.set_ws_ctx(htn::WS_ENEMY_VISIBLE, ctxv);
    }

    /// OnShutdown.
    pub fn shutdown(&mut self, b: &Brain, ctx: &mut Ctx) {
        ctx.shared
            .squad
            .engaging_target(Self::member(b), None, self.best_threat);
    }
}

/// TnPainSensor.
#[derive(Clone, Debug)]
pub struct PainSensor {
    pub allow_friendly_fire: bool,
    pub time_since_last_hurt: f32,
    pub last_hurt_location: Vec3,
}

impl PainSensor {
    pub fn new(allow_friendly_fire: bool) -> PainSensor {
        PainSensor {
            allow_friendly_fire,
            time_since_last_hurt: 3600.0,
            last_hurt_location: Vec3::ZERO,
        }
    }

    /// get_RecentlyHurt.
    pub fn recently_hurt(&self) -> bool {
        self.time_since_last_hurt < 3.0
    }

    fn update(&mut self, b: &mut Brain, p: &Pawn, dt: f32) {
        self.time_since_last_hurt += dt;
        b.core
            .set_ws_bool(htn::WS_HURT_BY_ENEMY_RECENTLY, self.recently_hurt());
        b.core.set_ws_bool(htn::WS_ON_FIRE, p.on_fire);
    }

    fn handle_event(&mut self, b: &mut Brain, ctx: &mut Ctx, e: &Event) {
        if !matches!(
            e.id,
            EventId::TookDamage | EventId::Gravity | EventId::GravityEnd
        ) {
            return;
        }
        let Some(tdi) = &e.damage else { return };
        let sender = tdi.instigator;
        let friendly = sender.is_some_and(|s| ctx.view.same_team(ctx.pawn.entity, s));
        if !self.allow_friendly_fire && friendly {
            return;
        }
        self.time_since_last_hurt = 0.0;
        b.core.set_ws_bool(htn::WS_HURT_BY_ENEMY_RECENTLY, true);
        match e.id {
            EventId::Gravity => {
                b.core.set_ws_bit(htn::WS_DANGER_CLOSE, 4, true);
                b.core.store(
                    super::brain::LR_GRAV_BOMB_LOCATION,
                    tdi.hit_location,
                    Vec3::ZERO,
                );
            }
            EventId::GravityEnd => b.core.set_ws_bit(htn::WS_DANGER_CLOSE, 4, false),
            _ => handle_pain_world_state(b, tdi.damage_type),
        }
        let Some(sender) = sender else { return };
        if friendly {
            return;
        }
        let Some(a) = ctx.view.actor(sender) else {
            return;
        };
        self.last_hurt_location = a.loc;
        if !b.sensors.threats.update_suspected(ctx.view, a.loc, a.feet) {
            b.core.set_ws_bool(htn::WS_ENEMY_LOCATION_SUSPICIOUS, true);
            b.core
                .store(super::brain::LR_SUSPECTED_ENEMY, a.loc, a.feet);
            b.core.store(
                super::brain::LR_NAVIGABLE_LOCATION_NEAR_ENEMY,
                a.loc,
                a.feet,
            );
        }
        b.sensors
            .threats
            .update_damage(sender, tdi.damage, tdi.damage_type, ctx.view.now);
    }
}

/// TnPainSensor.HandlePainWorldState: WS_IsInPain's bit for the damage's type.
fn handle_pain_world_state(b: &mut Brain, kind: DamageKind) {
    let current = b.core.ws(htn::WS_IS_IN_PAIN);
    let bit = match kind {
        DamageKind::Stun => 4,
        DamageKind::LeapSmash => 16,
        DamageKind::VehicleExplosive | DamageKind::PlaneRocket => 32,
        DamageKind::Rammed => 8,
        DamageKind::Melee => 2,
        _ => 1,
    };
    b.core.set_ws(htn::WS_IS_IN_PAIN, current | bit);
}

/// TnWeaponSensor.
#[derive(Clone, Debug)]
pub struct WeaponSensor {
    pub check_for_aiming: bool,
    pub check_for_pawns_in_lof: bool,
    pub check_for_los: bool,
}

impl WeaponSensor {
    fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let p = &*ctx.pawn;
        b.core
            .set_ws_bool(htn::WS_WEAPON_EQUIPPED, p.weapon.is_equipped());
        let clear = self.has_clear_line_of_fire(b, ctx);
        b.core
            .set_ws_bool(htn::WS_WEAPON_HAS_CLEAR_LINE_OF_FIRE, clear);
        b.core.set_ws_bool(
            htn::WS_HAS_AMMO,
            ctx.pawn.weapon.equipped && ctx.pawn.weapon.has_ammo(),
        );
    }

    /// HasClearLineOfFire.
    fn has_clear_line_of_fire(&self, b: &Brain, ctx: &Ctx) -> bool {
        let p = &*ctx.pawn;
        if !p.weapon.equipped || !p.aimer.active {
            return false;
        }
        let muzzle = muzzle_location(p);
        if self.check_for_aiming && !super::weapon::is_aiming_at_target(p, muzzle) {
            return false;
        }
        let aim = super::weapon::aim_location(p);
        if self.check_for_pawns_in_lof && pawn_inside_shooting_corridor(b, ctx, aim) {
            return false;
        }
        if self.check_for_los && !ctx.view.fast_trace(muzzle, aim) {
            return false;
        }
        true
    }
}

/// The muzzle's place (the weapon's GetMuzzleLocation: its socket on the gun
/// arm, as the animation holds it; the tree works it out each frame).
pub fn muzzle_location(p: &Pawn) -> Vec3 {
    p.muzzle
}

/// TnWeaponSensor.PawnInsideShootingCorridor: someone it should not shoot
/// through between it and where it aims.
fn pawn_inside_shooting_corridor(b: &Brain, ctx: &Ctx, aim: Vec3) -> bool {
    let p = &*ctx.pawn;
    let radius = (aim - p.loc).length() * 0.5;
    let centre = (aim + p.loc) * 0.5;
    let from = p.eyes();
    for a in &ctx.view.ais {
        if !a.alive || a.entity == p.entity || (a.loc - centre).length() > radius + a.radius {
            continue;
        }
        // ShouldAvoidAimingTowards: its own side's (not its enemy, nor itself).
        if a.team != b.core.team || Some(a.entity) == b.core.enemy {
            continue;
        }
        if in_line_of_fire(from, aim, a.loc, a.radius, a.half_height) {
            return true;
        }
    }
    false
}

/// TnWeaponSensor.InLineOfFire.
fn in_line_of_fire(begin: Vec3, end: Vec3, at: Vec3, radius: f32, height: f32) -> bool {
    let dir = ue::normal(end - begin);
    let distance = ue::dot2d(at - begin, dir);
    let t = distance / ue::vsize2d(end - begin).max(1e-3);
    if !(0.0..=1.0).contains(&t) {
        return false;
    }
    let closest = begin + (end - begin) * t;
    ue::vsize2d(closest - at) <= radius && (closest.z - at.z).abs() <= height
}

/// TnEnemyStatusSensor.
#[derive(Clone, Debug)]
pub struct EnemyStatusSensor {
    pub update_navigable_location_near_enemy: bool,
    pub flanking_half_angle: f32,
    pub ranges: Vec<f32>,
    pub health_levels: Vec<f32>,
}

impl Default for EnemyStatusSensor {
    fn default() -> Self {
        EnemyStatusSensor {
            update_navigable_location_near_enemy: false,
            flanking_half_angle: 50.0,
            ranges: Vec::new(),
            health_levels: Vec::new(),
        }
    }
}

impl EnemyStatusSensor {
    fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let enemy = b.core.enemy.and_then(|e| ctx.view.actor(e));
        let pl = b.core.enemy.and_then(|e| ctx.view.player(e));
        let range = self.calculate_range(ctx.pawn, enemy.as_ref());
        b.core.set_ws(htn::WS_ENEMY_RANGE, range);
        let health = match pl {
            None => 0,
            Some(pl) => {
                let pct = pl.health / pl.health_max.max(1.0);
                self.health_levels
                    .iter()
                    .position(|l| pct < *l)
                    .unwrap_or(self.health_levels.len()) as i32
            }
        };
        b.core.set_ws(htn::WS_ENEMY_HEALTH, health);
        b.core.set_ws_bool(
            htn::WS_ENEMY_AIRBORNE,
            pl.is_some_and(|p| p.jumping || p.airborne),
        );
        b.core.set_ws(htn::WS_ENEMY_FORM, pl.map_or(0, |p| p.form));
        if b.core.d.is_used(htn::WS_ENEMY_WEAKENED) {
            b.core
                .set_ws_bool(htn::WS_ENEMY_WEAKENED, pl.is_some_and(|p| p.weakened));
        }
        if b.core.d.is_used(htn::WS_ENEMY_MOVEMENT) {
            let flanked = enemy.is_some_and(|a| self.is_flanked(ctx.pawn, &a));
            b.core.set_ws_bit(htn::WS_ENEMY_MOVEMENT, 0, flanked);
            let (mut advancing, mut retreating) = (false, false);
            if !flanked && let Some(a) = enemy {
                let from = ue::normal(ctx.pawn.loc - a.loc);
                let speed_sq = a.vel.length_squared();
                if speed_sq > 400.0 * 400.0 {
                    let vdir = a.vel / speed_sq.sqrt();
                    advancing = self.flanking_half_angle
                        > ue::radian_angle_between_unit(from, vdir).to_degrees();
                    retreating = !advancing
                        && self.flanking_half_angle
                            > ue::radian_angle_between_unit(-from, vdir).to_degrees();
                }
            }
            b.core.set_ws_bit(htn::WS_ENEMY_MOVEMENT, 1, advancing);
            b.core.set_ws_bit(htn::WS_ENEMY_MOVEMENT, 2, retreating);
        }
        match (pl, enemy) {
            (Some(p), Some(a)) => {
                b.core
                    .set_ws_bool(htn::WS_ENEMY_TRANSFORMING, p.transforming);
                b.core
                    .set_ws_ctx(htn::WS_ENEMY_TRANSFORMING, p.transforming as i32);
                if self.update_navigable_location_near_enemy {
                    b.core.store(
                        super::brain::LR_NAVIGABLE_LOCATION_NEAR_ENEMY,
                        a.nav_near,
                        Vec3::ZERO,
                    );
                }
                b.core.set_ws(htn::WS_ENEMY_PHYSICS, p.physics);
            }
            _ => {
                b.core.set_ws_bool(htn::WS_ENEMY_TRANSFORMING, false);
                b.core.set_ws_ctx(htn::WS_ENEMY_TRANSFORMING, 0);
                b.core.set_ws(htn::WS_ENEMY_PHYSICS, 0);
                if self.update_navigable_location_near_enemy
                    && let Some(a) = enemy
                {
                    b.core.store(
                        super::brain::LR_NAVIGABLE_LOCATION_NEAR_ENEMY,
                        a.loc,
                        Vec3::ZERO,
                    );
                }
            }
        }
    }

    /// IsEnemyFlanked: it is out of its enemy's view's half angle.
    fn is_flanked(&self, p: &Pawn, a: &ActorInfo) -> bool {
        let from = ue::normal(p.loc - a.loc);
        self.flanking_half_angle
            < ue::radian_angle_between_unit(from, ue::yaw_vec(a.yaw)).to_degrees()
    }

    /// CalculateRange: the first range its enemy is within (AI.CalculateRangeToActor).
    fn calculate_range(&self, p: &Pawn, enemy: Option<&ActorInfo>) -> i32 {
        let Some(a) = enemy else {
            return self.ranges.len() as i32;
        };
        let range = ue::range_point_to_point(
            p.loc,
            p.radius,
            a.loc,
            a.radius,
            p.half_height,
            a.half_height,
        );
        self.ranges
            .iter()
            .position(|r| range < *r)
            .unwrap_or(self.ranges.len()) as i32
    }

    fn handle_event(&mut self, b: &mut Brain, ctx: &mut Ctx, e: &Event) {
        let Some(cloaker) = e.actor else { return };
        if ctx.view.same_team(ctx.pawn.entity, cloaker) {
            return;
        }
        if e.id == EventId::Cloak && b.core.enemy == Some(cloaker) {
            b.core.set_ws_bit(htn::WS_DANGER_CLOSE, 2, true);
        } else if e.id == EventId::Uncloak
            && let Some(a) = ctx.view.actor(cloaker)
            && point_inside_vision_cone(ctx.pawn, a.loc)
            && line_of_sight(ctx, cloaker, false)
        {
            b.core.set_ws_bit(htn::WS_DANGER_CLOSE, 3, true);
        }
    }
}

/// TnClearAttackChannelSensor.
#[derive(Clone, Debug)]
pub struct ClearChannelSensor {
    pub trace_extent: Vec3,
    pub walk_line_only_test: bool,
    /// The way it last side stepped (0 none, 1 left, 2 right).
    pub last_move_direction: u8,
    /// The demo's own (the Scrappers'): the way round when the straight way is cut
    /// off.
    pub way_round: WayRound,
}

impl Default for ClearChannelSensor {
    fn default() -> Self {
        ClearChannelSensor {
            trace_extent: Vec3::new(300.0, 300.0, 0.0),
            walk_line_only_test: false,
            last_move_direction: 0,
            way_round: WayRound::default(),
        }
    }
}

/// The demo's own, not the game's, for the Scrappers: the game's channel is clear
/// only along a straight walk across the navigation mesh to its enemy, so a
/// Scrapper below a player on a ledge (Downfall's trench under the Photon Burst
/// Rifle's walkway) side steps for a clear channel (SCRAPPER_MoveToClearAttackChannel)
/// for ever, where the way up is a jump away. After `after_side_steps` side steps
/// in a row that leave the straight way still cut off, a path to its enemy on
/// the mesh (jumps and all) counts as a clear channel: its kung fu circle's first
/// method takes it, and its move there (SCRAPPER_MoveTo, which follows paths)
/// brings it up to him. It counts until the straight way is open (the game's
/// channel takes over again), its enemy changes or the path is gone.
#[derive(Clone, Debug, Default)]
pub struct WayRound {
    /// Side steps before a path counts (0: never, the game's).
    pub after_side_steps: u8,
    /// Failed side steps in a row with the straight way cut off.
    pub failed_side_steps: u8,
    /// The enemy they were for.
    pub enemy: Option<Entity>,
    /// Whether the straight way was cut off at the last update.
    pub cut_off: bool,
    /// The last search for a path: when, and whether it found one.
    pub searched_at: Option<f32>,
    pub path: bool,
}

/// How often a way round is searched for again, in seconds.
pub const WAY_ROUND_SEARCH_SECS: f32 = 0.5;

impl WayRound {
    /// A Scrapper's: a path counts after two failed side steps.
    pub fn after(side_steps: u8) -> WayRound {
        WayRound {
            after_side_steps: side_steps,
            ..Default::default()
        }
    }

    /// Whether a path counts as a clear channel now.
    pub fn open(&self) -> bool {
        self.after_side_steps > 0
            && self.cut_off
            && self.failed_side_steps >= self.after_side_steps
            && self.path
    }
}

impl ClearChannelSensor {
    fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let mut clear = self.is_attack_channel_clear(b, ctx);
        if self.way_round.after_side_steps > 0 {
            self.update_way_round(b, ctx, clear);
            clear = clear || self.way_round.open();
        }
        b.core.set_ws_bool(htn::WS_CLEAR_ATTACK_CHANNEL, clear);
    }

    /// A side step (SCRAPPER_MoveToClearAttackChannel) that left the channel
    /// blocked: one more toward the way round while the straight way is cut off.
    pub fn side_step_failed(&mut self) {
        if self.way_round.cut_off {
            self.way_round.failed_side_steps = self.way_round.failed_side_steps.saturating_add(1);
        }
    }

    /// WayRound: whether the straight way to its enemy is cut off and, once
    /// enough side steps have failed, whether a path leads to him (searched
    /// for again every half second). In the air (a jump on the way) it keeps what
    /// it had.
    fn update_way_round(&mut self, b: &Brain, ctx: &Ctx, clear: bool) {
        let enemy = b.core.enemy;
        let w = &mut self.way_round;
        if enemy != w.enemy {
            *w = WayRound {
                enemy,
                ..WayRound::after(w.after_side_steps)
            };
        }
        if ctx.pawn.physics != Physics::Walking || !b.core.is_active() {
            return;
        }
        let target = enemy
            .and_then(|e| ctx.view.actor(e))
            .filter(|a| !a.dead && !clear);
        let Some(target) = target else {
            w.cut_off = false;
            w.failed_side_steps = 0;
            return;
        };
        let reach = calculate_reach_location(ctx.pawn, &target);
        w.cut_off = !super::motion::is_reachable(ctx.pawn, ctx.view, reach);
        if !w.cut_off {
            w.failed_side_steps = 0;
            return;
        }
        if w.failed_side_steps < w.after_side_steps
            || w.searched_at
                .is_some_and(|t| ctx.view.now - t < WAY_ROUND_SEARCH_SECS)
        {
            return;
        }
        w.searched_at = Some(ctx.view.now);
        let p = &*ctx.pawn;
        w.path = match (ctx.view.nav, p.anchor) {
            (Some(nav), Some(cell)) => {
                let moves = crate::nav::Moves::ground(
                    p.radius / ue::UU_PER_M,
                    p.half_height * 2.0 / ue::UU_PER_M,
                );
                nav.find_channel(cell, ue::from_ue(target.nav_near), &moves)
                    .is_some()
            }
            _ => false,
        };
    }

    /// IsAttackChannelClear.
    pub fn is_attack_channel_clear(&self, b: &Brain, ctx: &Ctx) -> bool {
        if !b.core.is_active() || ctx.pawn.physics != Physics::Walking {
            return false;
        }
        let Some(enemy) = b.core.enemy.and_then(|e| ctx.view.actor(e)) else {
            return false;
        };
        if enemy.dead {
            return false;
        }
        let reach = calculate_reach_location(ctx.pawn, &enemy);
        if !super::motion::is_reachable(ctx.pawn, ctx.view, reach) {
            return false;
        }
        if self.walk_line_only_test {
            return true;
        }
        self.can_trace(ctx, &enemy, reach)
    }

    /// CanTrace: a box as wide as the trace extent from it to where it would
    /// strike, nothing but its enemy in the way.
    fn can_trace(&self, ctx: &Ctx, enemy: &ActorInfo, at: Vec3) -> bool {
        let p = &*ctx.pawn;
        let end = at + Vec3::Z * p.half_height;
        let start = p.loc;
        if ctx.view.sweep_blocked(start, end, self.trace_extent.x) {
            return false;
        }
        for a in &ctx.view.ais {
            if !a.alive || a.entity == p.entity || a.entity == enemy.entity {
                continue;
            }
            if in_line_of_fire(
                start,
                end,
                a.loc,
                a.radius + self.trace_extent.x,
                a.half_height + self.trace_extent.z,
            ) {
                return false;
            }
        }
        true
    }

    /// CalculateMoveToDestination: a side step of 500 uu across its enemy's
    /// direction it can walk and nothing blocks (the way it went last, else either
    /// at random).
    pub fn calculate_move_to_destination(
        &mut self,
        ctx: &Ctx,
        enemy_dir: Vec3,
        rng: &mut Rng,
    ) -> Option<Vec3> {
        let d = ue::normal2d(enemy_dir);
        let y = ue::vec_of(0.0, ue::yaw_of(d) + 16384.0);
        let go_left = match self.last_move_direction {
            1 => true,
            2 => false,
            _ => rng.frand() < 0.5,
        };
        self.can_side_step(ctx, y, go_left)
            .or_else(|| self.can_side_step(ctx, y, !go_left))
    }

    fn can_side_step(&mut self, ctx: &Ctx, dir: Vec3, go_left: bool) -> Option<Vec3> {
        let p = &*ctx.pawn;
        let dir = if go_left { -dir } else { dir };
        let destination = p.feet() + dir * 500.0;
        if can_walk_line(p, ctx.view, destination, true).0 {
            let start = ue::normal(destination - p.loc) * self.trace_extent.x + p.loc;
            if !ctx
                .view
                .sweep_blocked(start, destination, self.trace_extent.x)
            {
                self.last_move_direction = if go_left { 1 } else { 2 };
                return Some(destination);
            }
        }
        self.last_move_direction = 0;
        None
    }
}

/// TnClearAttackChannelSensor.CalculateReachLocation: just short of its enemy,
/// on the navigation mesh, on its own side.
pub fn calculate_reach_location(p: &Pawn, enemy: &ActorInfo) -> Vec3 {
    let push_back = (enemy.radius + p.radius) * 1.05;
    let dir = ue::normal(p.loc - enemy.nav_near);
    dir * push_back + enemy.nav_near
}

/// AI.CanWalkLine: whether it can walk straight from its feet to `destination`
/// over the mesh (with its radius or none), and where it would stop.
pub fn can_walk_line(
    p: &Pawn,
    view: &WorldView,
    destination: Vec3,
    use_radius: bool,
) -> (bool, Vec3) {
    let start = p.feet();
    let (Some(nav), Some(cell)) = (view.nav, p.anchor) else {
        return (
            view.nav.is_none(),
            if view.nav.is_none() {
                destination
            } else {
                start
            },
        );
    };
    let radius = if use_radius { p.radius } else { 0.0 } / ue::UU_PER_M;
    let (ok, end) = nav.walk_line(cell, ue::from_ue(start), ue::from_ue(destination), radius);
    (ok, ue::to_ue(end))
}

/// AI.CanWalkLineFrom: whether the pawn (its radius) can walk the straight line
/// from `start` (a point at its middle's height) to `destination`, and where it
/// would stop (from the cell under the start, else its anchor's).
pub fn can_walk_line_from(
    p: &Pawn,
    view: &WorldView,
    start: Vec3,
    destination: Vec3,
) -> (bool, Vec3) {
    let Some(nav) = view.nav else {
        return (true, destination);
    };
    let feet = start - Vec3::Z * p.half_height;
    let Some(cell) = nav.cell_at(ue::from_ue(feet)).or(p.anchor) else {
        return (false, start);
    };
    let (ok, end) = nav.walk_line(
        cell,
        ue::from_ue(start),
        ue::from_ue(destination),
        p.radius / ue::UU_PER_M,
    );
    (ok, ue::to_ue(end))
}

/// TnCompetencySensor.
#[derive(Clone, Debug)]
pub struct CompetencySensor {
    pub incompetent_distance: f32,
    pub max_competence_lower_bound: f32,
    pub min_competency_upper_bound: f32,
    pub loss_in_competence_per_buddy_alive: f32,
    pub flanking_half_angle: f32,
    pub flanking_in_competence: f32,
    pub flanking_in_competence_time: f32,
    pub time_flanking: f32,
    pub enemy_health: f32,
    pub distance_to_enemy: f32,
    pub frequency: f32,
}

impl Default for CompetencySensor {
    fn default() -> Self {
        CompetencySensor {
            incompetent_distance: 7000.0,
            max_competence_lower_bound: 0.5,
            min_competency_upper_bound: 0.3,
            loss_in_competence_per_buddy_alive: 0.25,
            flanking_half_angle: 90.0,
            flanking_in_competence: 0.1,
            flanking_in_competence_time: 0.0,
            time_flanking: 0.0,
            enemy_health: 0.0,
            distance_to_enemy: 0.0,
            frequency: 0.1,
        }
    }
}

impl CompetencySensor {
    fn update(&mut self, b: &mut Brain, ctx: &mut Ctx, dt: f32) {
        if let Some(e) = b.core.enemy
            && let Some(a) = ctx.view.actor(e)
        {
            self.enemy_health = ctx.view.player(e).map_or(1.0, |pl| {
                // GetNormalizedHealth (a downed player's against its downed health).
                pl.health / pl.health_max.max(1.0)
            });
            self.distance_to_enemy = (a.loc - ctx.pawn.loc).length();
            // UpdateFlanking.
            let from = ue::normal(ctx.pawn.loc - a.loc);
            let flanking = self.flanking_half_angle
                < ue::radian_angle_between_unit(from, ue::yaw_vec(a.yaw)).to_degrees();
            if flanking {
                self.time_flanking += dt;
            } else {
                self.time_flanking = 0.0;
            }
        } else if b.core.enemy.is_none() {
            self.time_flanking = 0.0;
        }
        b.core.competency = self.desired(b, ctx);
    }

    fn flanking_competency(&self) -> f32 {
        if self.time_flanking == 0.0
            || self.time_flanking > self.flanking_in_competence_time.max(self.frequency)
        {
            1.0
        } else {
            self.flanking_in_competence
        }
    }

    /// CalculateDesiredCompetency.
    fn desired(&self, b: &Brain, ctx: &Ctx) -> f32 {
        let Some(enemy) = b.core.enemy else {
            return 0.0;
        };
        // CountNumFriendliesAttackingEnemy.
        let friends = ctx
            .view
            .ais
            .iter()
            .filter(|a| a.entity != b.core.entity && a.alive && a.enemy == Some(enemy))
            .count() as f32;
        let min = (1.0 - self.loss_in_competence_per_buddy_alive * friends)
            .clamp(0.0, self.min_competency_upper_bound)
            * self.flanking_competency();
        let h = self.enemy_health;
        let max = self
            .max_competence_lower_bound
            .max(1.0 - (1.0 - h) * (1.0 - h));
        if max < min {
            return (max + min) * 0.5;
        }
        let d = self.distance_to_enemy;
        let blend =
            1.0 - (1.0f32).min(d * d / (self.incompetent_distance * self.incompetent_distance));
        ue::lerp(min, max, blend * self.flanking_competency())
    }
}

// ---------------------------------------------------------------------------
// The collection
// ---------------------------------------------------------------------------

/// An enemy's sensors (its HtnSensorCollection) and the data stores they keep.
#[derive(Clone, Debug, Default)]
pub struct Sensors {
    pub groups: Vec<Group>,
    pub prior: Vec<u8>,
    /// Every sensor registered, by its id (AllSensors), and each one's last update.
    pub all: Vec<u8>,
    pub last_update: Vec<f32>,
    pub frequency: Vec<f32>,
    pub update_while_asleep: bool,
    // -- The sensors -----------------------------------------------------------
    pub search: Option<SearchSensor>,
    pub hearing: Option<HearingSensor>,
    pub pathing: Option<PathingSensor>,
    pub path: Option<super::path::PathFollowing>,
    pub vision: Option<VisionSensor>,
    pub threat: Option<ThreatSensor>,
    pub pain: Option<PainSensor>,
    pub hit: Option<super::hits::HitReactionSensor>,
    pub weapon: Option<WeaponSensor>,
    pub status: Option<EnemyStatusSensor>,
    pub channel: Option<ClearChannelSensor>,
    pub tactical: Option<super::tactical::TacticalAttack>,
    pub competency: Option<CompetencySensor>,
    pub squad: Option<SquadSensor>,
    pub los_search: Option<LosSearchSensor>,
    pub injured: Option<InjuredSensor>,
    pub health_level: Option<HealthLevelSensor>,
    pub attack_point: Option<AttackPointSensor>,
    pub guard: Option<GuardSensor>,
    pub special_attack: Option<SpecialAttackCooldownSensor>,
    pub nav_mesh: Option<NavMeshSensor>,
    pub leap_smash: Option<LeapSmashSensor>,
    pub kiting: Option<EnemyKitingSensor>,
    /// The Titan's drones, and the drones' own (drones.rs).
    pub drone: Option<super::drones::DroneSensor>,
    pub drone_path: Option<super::drones::DronePathFollowing>,
    pub hover_defend: Option<super::drones::HoverDefendActor>,
    pub defend_status: Option<super::drones::DefendActorStatus>,
    pub lifespan: Option<super::drones::Lifespan>,
    /// The Snipers' (sniper.rs).
    pub sniper_point: Option<super::sniper::Sensor>,
    /// The jets' (jet.rs): their flights, their attack, pursuit and circling
    /// points.
    pub plane_path: Option<super::jet::PlanePathFollowing>,
    pub air_attack: Option<super::jet::AirAttackPoint>,
    pub aerial_pursuit: Option<super::jet::AerialPursuit>,
    pub flying_circling: Option<super::jet::FlyingCircling>,
    // -- The data stores --------------------------------------------------------
    /// TnSniperAiDataStore (a Sniper's).
    pub sniper_store: Option<super::sniper::Store>,
    pub threats: ThreatStore,
    pub patrol: PatrolStore,
    /// TnDefendActorDataStore's DefendActor (a drone's Titan).
    pub defend_actor: Option<Entity>,
    // -- The engine's sight check (AController's SightCounter) -----------------
    pub sight_counter: f32,
}

impl Sensors {
    pub fn new() -> Sensors {
        Sensors {
            groups: GROUP_FREQUENCIES
                .iter()
                .map(|f| Group {
                    sensors: Vec::new(),
                    frequency: *f,
                    time_before_next_update: 0.0,
                })
                .collect(),
            last_update: vec![0.0; MAX_SENSORS],
            frequency: vec![0.0; MAX_SENSORS],
            update_while_asleep: true,
            ..Default::default()
        }
    }

    pub fn is_registered(&self, id: u8) -> bool {
        self.all.contains(&id)
    }
}

/// Takes a sensor out of its slot for the call (so it can use the brain), and
/// puts it back.
macro_rules! with_sensor {
    ($b:expr, $field:ident, $s:ident => $body:expr) => {{
        if let Some(mut $s) = $b.sensors.$field.take() {
            let r = $body;
            if $b.sensors.$field.is_none() {
                $b.sensors.$field = Some($s);
            }
            Some(r)
        } else {
            None
        }
    }};
}
pub(crate) use with_sensor;

/// HtnSensorCollection.RegisterSensor: into its group (and those updated before
/// planning), initialized (its first update, forced).
pub fn register(b: &mut Brain, ctx: &mut Ctx, id: u8, group: usize, prior: bool) {
    if b.sensors.is_registered(id) {
        return;
    }
    let group = group.min(GROUP_FREQUENCIES.len() - 1);
    b.sensors.all.push(id);
    b.sensors.groups[group].sensors.push(id);
    b.sensors.frequency[id as usize] = GROUP_FREQUENCIES[group];
    if prior {
        b.sensors.prior.push(id);
    }
    initialize(b, ctx, id);
    b.sensors.last_update[id as usize] = ctx.view.now;
    update_sensor(b, ctx, id, true);
}

/// A sensor's OnInitialize.
fn initialize(b: &mut Brain, ctx: &mut Ctx, id: u8) {
    match id {
        S_VISION => {
            with_sensor!(b, vision, s => s.update_active_profile(b, ctx.pawn));
        }
        S_TACTICAL_ATTACK => {
            with_sensor!(b, tactical, s => s.initialize(b, ctx));
        }
        S_HIT_REACTION => {
            with_sensor!(b, hit, s => s.initialize(b, ctx));
        }
        S_COMPETENCY => {
            let f = b.sensors.frequency[id as usize];
            if let Some(s) = b.sensors.competency.as_mut() {
                s.frequency = f;
            }
        }
        S_INJURED => {
            let health = ctx.pawn.health;
            if let Some(s) = b.sensors.injured.as_mut() {
                s.trigger_value = health - s.damage_amount_for_trigger;
            }
            b.core.set_ws_bool(htn::WS_BALANCED, true);
        }
        S_SPECIAL_ATTACK => {
            with_sensor!(b, special_attack, s => s.initialize(b));
        }
        S_NAV_MESH => {
            let at = ctx.pawn.loc;
            if let Some(s) = b.sensors.nav_mesh.as_mut() {
                s.previous = at;
            }
        }
        super::drones::S_DRONE => {
            with_sensor!(b, drone, s => s.initialize(b));
        }
        super::sniper::S_SNIPER_POINT => {
            if let Some(s) = b.sensors.sniper_point.as_mut() {
                s.initialize(ctx);
            }
        }
        _ => {}
    }
}

/// HtnSensor.Update: OnUpdate with the time since its last update (none if no
/// time has passed, unless forced).
fn update_sensor(b: &mut Brain, ctx: &mut Ctx, id: u8, force: bool) {
    let now = ctx.view.now;
    let dt = now - b.sensors.last_update[id as usize];
    if !force && dt == 0.0 {
        return;
    }
    match id {
        S_SEARCH => {
            with_sensor!(b, search, s => s.update(b));
        }
        S_HEARING => {
            with_sensor!(b, hearing, s => s.update(b, dt));
        }
        S_PATHING => {
            with_sensor!(b, pathing, s => s.update(b, dt));
        }
        S_PATH_FOLLOWING => {
            with_sensor!(b, path, s => s.update(b, ctx));
        }
        S_VISION => {
            with_sensor!(b, vision, s => s.update(b, ctx, dt));
        }
        S_THREAT => {
            with_sensor!(b, threat, s => s.update(b, ctx, dt));
        }
        S_PAIN => {
            with_sensor!(b, pain, s => s.update(b, ctx.pawn, dt));
        }
        S_HIT_REACTION => {
            with_sensor!(b, hit, s => s.update(ctx.view.now));
        }
        S_WEAPON => {
            with_sensor!(b, weapon, s => s.update(b, ctx));
        }
        S_ENEMY_STATUS => {
            with_sensor!(b, status, s => s.update(b, ctx));
        }
        S_CLEAR_ATTACK_CHANNEL => {
            with_sensor!(b, channel, s => s.update(b, ctx));
        }
        S_TACTICAL_ATTACK => {
            with_sensor!(b, tactical, s => s.update(b, ctx, dt));
        }
        S_COMPETENCY => {
            with_sensor!(b, competency, s => s.update(b, ctx, dt));
        }
        S_SQUAD => {
            with_sensor!(b, squad, s => s.update(b, ctx));
        }
        S_LOS_SEARCH => {
            with_sensor!(b, los_search, s => s.update(b, ctx));
        }
        S_INJURED => {
            with_sensor!(b, injured, s => s.update(b, dt));
        }
        S_HEALTH_LEVEL => {
            with_sensor!(b, health_level, s => s.update(b, ctx.pawn));
        }
        S_ATTACK_POINT => {
            // (The level script's attack points: SeqAct_TnAssignAttackPoints.
            // Escalation's maps assign none.)
            b.core.set_ws_bool(htn::WS_ATTACK_POINT_AVAILABLE, false);
        }
        S_GUARD => {
            with_sensor!(b, guard, s => s.update(b, ctx));
        }
        S_SPECIAL_ATTACK => {
            with_sensor!(b, special_attack, s => s.update(b, ctx.pawn, dt));
        }
        S_NAV_MESH => {
            with_sensor!(b, nav_mesh, s => s.update(b, ctx, dt));
        }
        S_LEAPER_SMASH_VOLUME => {
            with_sensor!(b, leap_smash, s => s.update(b, ctx, dt));
        }
        S_ENEMY_KITING => {
            with_sensor!(b, kiting, s => s.update(b, ctx, dt));
        }
        super::drones::S_DRONE => {
            with_sensor!(b, drone, s => s.update(b, ctx, dt));
        }
        super::drones::S_DRONE_PATH_FOLLOWING => {
            with_sensor!(b, drone_path, s => s.update(b, ctx));
        }
        super::drones::S_DEFEND_ACTOR => {
            with_sensor!(b, hover_defend, s => s.update(b, ctx));
            with_sensor!(b, defend_status, s => s.update(b, ctx));
        }
        super::drones::S_LIFESPAN => {
            with_sensor!(b, lifespan, s => s.update(b, ctx, dt));
        }
        super::sniper::S_SNIPER_POINT => {
            with_sensor!(b, sniper_point, s => s.update(b, ctx));
        }
        super::jet::S_PLANE_PATH_FOLLOWING => {
            with_sensor!(b, plane_path, s => s.update(b, ctx, dt));
        }
        _ => {}
    }
    b.sensors.last_update[id as usize] = now;
}

/// HtnSensorCollection.Update: each group whose time has come.
pub fn update(b: &mut Brain, ctx: &mut Ctx) {
    if !b.sensors.update_while_asleep && !b.core.is_active() {
        return;
    }
    for g in 0..b.sensors.groups.len() {
        b.sensors.groups[g].time_before_next_update -= ctx.dt;
        if b.sensors.groups[g].time_before_next_update > 0.0 {
            continue;
        }
        let ids = b.sensors.groups[g].sensors.clone();
        for id in ids {
            update_sensor(b, ctx, id, false);
        }
        b.sensors.groups[g].time_before_next_update = b.sensors.groups[g].frequency;
    }
}

/// UpdatePriorToPlanning.
pub fn update_prior_to_planning(b: &mut Brain, ctx: &mut Ctx) {
    for id in b.sensors.prior.clone() {
        update_sensor(b, ctx, id, false);
    }
}

/// NotifyEventOccurred: every sensor, in the order of their ids.
pub fn notify_event(b: &mut Brain, ctx: &mut Ctx, e: &Event) {
    let mut ids = b.sensors.all.clone();
    ids.sort_unstable();
    for id in ids {
        match id {
            S_HEARING => {
                with_sensor!(b, hearing, s => s.handle_event(b, ctx, e));
            }
            S_VISION => {
                with_sensor!(b, vision, s => s.handle_event(b, ctx, e));
            }
            S_ENEMY_STATUS => {
                with_sensor!(b, status, s => s.handle_event(b, ctx, e));
            }
            S_PAIN => {
                with_sensor!(b, pain, s => s.handle_event(b, ctx, e));
            }
            S_TACTICAL_ATTACK => {
                with_sensor!(b, tactical, s => s.handle_event(b, ctx, e));
            }
            S_HIT_REACTION => {
                with_sensor!(b, hit, s => s.handle_event(b, ctx, e));
            }
            S_THREAT => {
                with_sensor!(b, threat, s => s.handle_event(b, e));
            }
            S_INJURED => {
                with_sensor!(b, injured, s => s.handle_event(b, ctx, e));
            }
            S_GUARD => {
                with_sensor!(b, guard, s => s.handle_event(b, e));
            }
            _ => {}
        }
    }
}

/// TnGuardSensor: a Guard in investigate form grows suspicious
/// (WS_Suspicion) of a Guard of its side within BattleFormNearFriendliesRange that
/// has gone into battle form after an enemy (ShouldRaiseSuspicion), and of a
/// buddy's sighting or an alarm (BuddySawEnemy, Alarm); an area cleared clears it.
#[derive(Clone, Debug)]
pub struct GuardSensor {
    pub battle_radius: f32,
}

impl GuardSensor {
    fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        if b.core.ws(htn::WS_CURRENT_FORM) == 1 || b.core.ws_bool(htn::WS_SUSPICION) {
            return;
        }
        let me = b.core.entity;
        let suspicion = ctx.view.ais.iter().any(|a| {
            a.alive
                && a.entity != me
                && a.team == b.core.team
                && (a.loc - ctx.pawn.loc).length() <= self.battle_radius
                && a.raises_suspicion
        });
        b.core.set_ws_bool(htn::WS_SUSPICION, suspicion);
    }

    fn handle_event(&mut self, b: &mut Brain, e: &Event) {
        match e.id {
            EventId::BuddySawEnemy | EventId::Alarm => b.core.set_ws_bool(htn::WS_SUSPICION, true),
            EventId::AreaClear => b.core.set_ws_bool(htn::WS_SUSPICION, false),
            _ => {}
        }
    }
}

/// TnSpecialAttackCooldownSensor: each special attack (its index's bit of
/// WS_SpecialAttackAvailable) comes ready once its cooldown (a time in its range,
/// drawn afresh each time) has run out, and its cooldown starts over once it has
/// been used (its bit cleared by its task) or while the pawn jumps.
#[derive(Clone, Debug, Default)]
pub struct SpecialAttackCooldownSensor {
    /// Each attack: its cooldown's range, its index and the cooldown left.
    pub attacks: Vec<SpecialAttackCooldown>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SpecialAttackCooldown {
    pub range: (f32, f32),
    pub index: i32,
    pub left: f32,
}

impl SpecialAttackCooldownSensor {
    pub fn new(range: (f32, f32), index: i32) -> SpecialAttackCooldownSensor {
        SpecialAttackCooldownSensor {
            attacks: vec![SpecialAttackCooldown {
                range,
                index,
                left: 0.0,
            }],
        }
    }

    fn initialize(&mut self, b: &mut Brain) {
        for a in &mut self.attacks {
            a.left = b.core.rng.range(a.range.0, a.range.1);
        }
    }

    fn update(&mut self, b: &mut Brain, p: &Pawn, dt: f32) {
        for a in &mut self.attacks {
            if !b.core.ws_bit(htn::WS_SPECIAL_ATTACK_AVAILABLE, a.index) && a.left <= 0.0 {
                a.left = b.core.rng.range(a.range.0, a.range.1);
            }
            if p.motion.is_jumping() {
                a.left = b.core.rng.range(a.range.0, a.range.1);
            }
            a.left -= dt;
            b.core
                .set_ws_bit(htn::WS_SPECIAL_ATTACK_AVAILABLE, a.index, a.left <= 0.0);
        }
    }
}

/// TnNavMeshSensor: whether it is on the navigation mesh (WS_OnNavMesh; off it
/// once it has been off for the time it allows, none unless made with one; its
/// context whether it is, was, or never was), and the last place it was on it
/// (LR_FallbackLocation).
#[derive(Clone, Debug, Default)]
pub struct NavMeshSensor {
    pub time_allowed_before_off: f32,
    pub time_since_on: f32,
    pub on: bool,
    pub has_been_on: bool,
    pub previous: Vec3,
}

impl NavMeshSensor {
    fn update(&mut self, b: &mut Brain, ctx: &mut Ctx, dt: f32) {
        // (IsOnNavMesh: a cell under it.)
        if ctx.view.cell_at(ctx.pawn.feet()).is_some() {
            self.on = true;
            self.has_been_on = true;
            self.previous = ctx.pawn.loc;
            self.time_since_on = 0.0;
        } else {
            self.time_since_on += dt;
            if self.time_since_on >= self.time_allowed_before_off {
                self.on = false;
            }
        }
        b.core.set_ws_bool(htn::WS_ON_NAV_MESH, self.on);
        let context = if self.on {
            0
        } else if self.has_been_on {
            1
        } else {
            2
        };
        b.core.set_ws_ctx(htn::WS_ON_NAV_MESH, context);
        b.core.store(
            super::brain::LR_FALLBACK_LOCATION,
            self.previous,
            Vec3::ZERO,
        );
    }
}

/// TnLeaperSmashVolumeSensor: its leap smash (bit 0 of
/// WS_SpecialAttackAvailable) is ready while, on the ground, its cooldown over
/// and its squad's next leap smash due, it leaps at an enemy it sees: it and
/// where it would land (by its enemy's LastNavigableNearbyLocation, its
/// ImpactLocationOffset towards itself) both inside the smash volume it stands in
/// (its TnLeaperSmashVolumeDataStore's), the enemy within its range (and not one
/// it could walk straight to inside the near end of it), the landing walkable
/// from the enemy and its leap's arc clear (its last place on the ground up to
/// the top of the arc, and down to the landing). Each landing it finds is its
/// last valid one; while it leaps (one claimed), one near enough to the claimed
/// one is where it dives to (RetargetImpactMaxDistance).
#[derive(Clone, Debug, Default)]
pub struct LeapSmashSensor {
    pub apex_height: f32,
    pub impact_offset: f32,
    pub retarget_max_distance: f32,
    pub cooldown_range: (f32, f32),
    /// Its LeapSmashRange (min, max).
    pub range: (f32, f32),
    pub cooldown: f32,
    pub last_on_ground: Vec3,
    pub last_valid_impact: Vec3,
    pub claimed_impact: Vec3,
    pub retargeted_impact: Vec3,
}

impl LeapSmashSensor {
    fn update(&mut self, b: &mut Brain, ctx: &mut Ctx, dt: f32) {
        self.cooldown -= dt;
        let p = &*ctx.pawn;
        let walking = p.physics == Physics::Walking;
        if walking {
            self.last_on_ground = p.loc;
        }
        let enemy = b.core.enemy.and_then(|e| ctx.view.actor(e));
        let valid = enemy.and_then(|e| {
            let impact = self.impact_location(p, &e);
            let nav = ctx.view.nav?;
            // (Its data store's volume: the one it touches.)
            let volume = nav
                .leap_volumes
                .iter()
                .find(|v| v.encompasses(ue::from_ue(p.loc)))?;
            let ok = b.core.ws_bool(htn::WS_ENEMY_VISIBLE)
                && encompasses_box(volume, p.loc, Vec3::new(p.radius, p.radius, p.half_height))
                && encompasses_box(
                    volume,
                    impact + Vec3::Z * e.half_height,
                    Vec3::new(e.radius, e.radius, e.half_height),
                )
                && self.passes_range_conditions(p, ctx.view, &e)
                && can_walk_line_from_enemy(ctx.view, &e, impact)
                && self.can_trace_leap_path(p, ctx.view, &e, impact);
            ok.then_some(impact)
        });
        if let Some(impact) = valid {
            self.last_valid_impact = impact;
            if ue::vsize_sq2d(self.claimed_impact - impact)
                < self.retarget_max_distance * self.retarget_max_distance
            {
                self.retargeted_impact = impact;
            }
            if walking
                && self.cooldown <= 0.0
                && ctx.shared.squad.time_before_leap_smash_available <= 0.0
            {
                b.core.set_ws_bit(htn::WS_SPECIAL_ATTACK_AVAILABLE, 0, true);
                return;
            }
        }
        b.core
            .set_ws_bit(htn::WS_SPECIAL_ATTACK_AVAILABLE, 0, false);
    }

    /// GetImpactLocation.
    fn impact_location(&self, p: &Pawn, e: &ActorInfo) -> Vec3 {
        e.nav_near + ue::normal2d(p.loc - e.loc) * self.impact_offset
    }

    /// PassesRangeConditions: within its range's far end; inside the near end only
    /// if it couldn't walk there (WalkLine from its feet, on its anchor's cell).
    fn passes_range_conditions(&self, p: &Pawn, view: &WorldView, e: &ActorInfo) -> bool {
        let d2 = (e.loc - p.loc).length_squared();
        if d2 > self.range.1 * self.range.1 {
            return false;
        }
        let (Some(nav), Some(cell)) = (view.nav, p.anchor) else {
            return false;
        };
        !(d2 < self.range.0 * self.range.0
            && nav
                .walk_line(cell, ue::from_ue(p.feet()), ue::from_ue(e.nav_near), 0.0)
                .0)
    }

    /// CanTraceLeapPath: its box swept from its last place on the ground to the
    /// top of the arc (half way across, its LeapApexHeight over the higher end)
    /// meets nothing, and on to where it would stand at the landing meets nothing
    /// but its enemy.
    fn can_trace_leap_path(&self, p: &Pawn, view: &WorldView, e: &ActorInfo, impact: Vec3) -> bool {
        let launch = self.last_on_ground;
        let d = impact - launch;
        let peak = self.apex_height + d.z.max(0.0);
        let apex = launch + Vec3::new(d.x * 0.5, d.y * 0.5, peak);
        let extent = Vec3::new(p.radius, p.radius, p.half_height);
        if box_trace(view, launch, apex, extent, p.entity).is_some() {
            return false;
        }
        let end = impact + Vec3::Z * extent.z;
        match box_trace(view, apex, end, extent, p.entity) {
            None => true,
            Some(hit) => hit == Some(e.entity),
        }
    }

    /// ClaimImpactLocation: its cooldown begun, its last valid landing the one it
    /// leaps at.
    pub fn claim_impact_location(&mut self, rng: &mut Rng) -> Vec3 {
        self.cooldown = rng.range(self.cooldown_range.0, self.cooldown_range.1);
        self.claimed_impact = self.last_valid_impact;
        self.retargeted_impact = self.last_valid_impact;
        self.last_valid_impact
    }
}

/// TnLeaperSmashVolumeSensor.EncompassesAxisAlignedBox: every corner of the box
/// inside the volume.
pub fn encompasses_box(volume: &crate::nav::LeapVolume, at: Vec3, extent: Vec3) -> bool {
    [-1.0f32, 1.0].iter().all(|&sx| {
        [-1.0f32, 1.0].iter().all(|&sy| {
            [-1.0f32, 1.0].iter().all(|&sz| {
                let corner = at + Vec3::new(sx * extent.x, sy * extent.y, sz * extent.z);
                volume.encompasses(ue::from_ue(corner))
            })
        })
    })
}

/// TnLeaperSmashVolumeSensor.CanWalkLineFromEnemyToImpactLocation: WalkLine on
/// the enemy's anchor's cell (the cell under it, or under its last navigable
/// place while it is in the air) from its LastNavigableNearbyLocation.
fn can_walk_line_from_enemy(view: &WorldView, e: &ActorInfo, impact: Vec3) -> bool {
    let Some(nav) = view.nav else {
        return false;
    };
    let Some(cell) = nav
        .cell_at(ue::from_ue(e.feet))
        .or_else(|| nav.cell_at(ue::from_ue(e.nav_near)))
    else {
        return false;
    };
    nav.walk_line(cell, ue::from_ue(e.nav_near), ue::from_ue(impact), 0.0)
        .0
}

/// A box of `extent` swept from `from` to `to` (Unreal space): Trace with an
/// extent and its actors, those that block (TRACEFLAG_Blocking): the first thing
/// in its way, the level (Some(None)) or a pawn (Some(Some)), other than `me`.
/// (The level, its blocking volumes too, by rays at its middle and its corners, a
/// little in from its faces so that what it starts and ends against doesn't stop
/// it; each pawn by its cylinder grown by the box.)
pub fn box_trace(
    view: &WorldView,
    from: Vec3,
    to: Vec3,
    extent: Vec3,
    me: Entity,
) -> Option<Option<Entity>> {
    let inset = (extent - Vec3::splat(10.0)).max(Vec3::ZERO);
    let mut best = 1.0f32;
    let mut hit = None;
    let mut offsets = vec![Vec3::ZERO];
    for sx in [-1.0f32, 1.0] {
        for sy in [-1.0f32, 1.0] {
            for sz in [-1.0f32, 1.0] {
                offsets.push(Vec3::new(sx * inset.x, sy * inset.y, sz * inset.z));
            }
        }
    }
    for o in offsets {
        let t = view.sweep_time(from + o, to + o);
        if t < best {
            best = t;
            hit = Some(None);
        }
    }
    let grow = extent.x.max(extent.y);
    let pawns = view
        .players
        .iter()
        .filter(|pl| pl.alive)
        .map(|pl| (pl.entity, pl.loc, pl.radius, pl.half_height))
        .chain(
            view.ais
                .iter()
                .filter(|a| a.alive && a.blocks && a.entity != me)
                .map(|a| (a.entity, a.loc, a.radius, a.half_height)),
        );
    for (e, loc, radius, half) in pawns {
        if let Some(t) = ue::segment_cylinder(from, to, loc, radius + grow, half + extent.z)
            && t < best
        {
            best = t;
            hit = Some(Some(e));
        }
    }
    hit
}

/// TnEnemyKitingSensor: an enemy that keeps within its kiting range for its
/// kiting duration sets WS_Custom (1, else 0), unless its booster attack has just
/// gone off (2: its effect), which starts the time over.
#[derive(Clone, Debug, Default)]
pub struct EnemyKitingSensor {
    pub range: f32,
    pub duration: f32,
    pub current: f32,
}

impl EnemyKitingSensor {
    fn update(&mut self, b: &mut Brain, ctx: &mut Ctx, dt: f32) {
        let close = b
            .core
            .enemy
            .and_then(|e| ctx.view.actor(e))
            .is_some_and(|e| (e.loc - ctx.pawn.loc).length_squared() <= self.range * self.range);
        if close && b.core.ws(htn::WS_CUSTOM) != 2 {
            self.current += dt;
        } else {
            self.current = 0.0;
        }
        b.core
            .set_ws(htn::WS_CUSTOM, i32::from(self.current >= self.duration));
    }
}

/// TnVisionSensor.SeeInstantly: `e` registered as seen at once (the Guard's
/// scanner).
pub fn see_instantly(b: &mut Brain, view: &WorldView, e: Entity) {
    if b.sensors.vision.is_some() {
        b.sensors.threats.update_visibility(view, e, true);
    }
}

/// TnInjuredSensor: each time its health has dropped by its trigger amount it
/// stumbles (WS_Balanced off) away from where the blow came from, steady again
/// once a moment has passed.
#[derive(Clone, Debug)]
pub struct InjuredSensor {
    pub damage_amount_for_trigger: f32,
    pub ignore_injury_threshold: f32,
    pub trigger_value: f32,
    pub time_since_injury: f32,
    pub current: &'static str,
    /// Its stumbles by the side struck (front, back, left, right), and their
    /// blends.
    pub from: [&'static str; 4],
    pub blend_in: f32,
    pub blend_out: f32,
}

impl InjuredSensor {
    pub fn new(damage_amount_for_trigger: f32) -> InjuredSensor {
        InjuredSensor {
            damage_amount_for_trigger,
            ignore_injury_threshold: 0.5,
            trigger_value: 0.0,
            time_since_injury: 0.0,
            current: "None",
            from: ["Stumble_B", "Stumble_F", "Stumble_R", "Stumble_L"],
            blend_in: 0.0,
            blend_out: 0.0,
        }
    }

    /// get_InjuredReactionAnimation (Animation.MakeAnimationParams').
    pub fn reaction_animation(&self) -> super::anim::AnimParams {
        super::anim::AnimParams::make(
            self.current,
            self.blend_in,
            self.blend_out,
            super::anim::RMM_ACCEL,
        )
    }

    fn update(&mut self, b: &mut Brain, dt: f32) {
        if b.core.ws_bool(htn::WS_BALANCED) {
            return;
        }
        self.time_since_injury += dt;
        if self.time_since_injury > self.ignore_injury_threshold {
            b.core.set_ws_bool(htn::WS_BALANCED, true);
        }
    }

    fn handle_event(&mut self, b: &mut Brain, ctx: &mut Ctx, e: &Event) {
        if e.id != EventId::TookDamage || ctx.pawn.health > self.trigger_value {
            return;
        }
        self.trigger_value -= self.damage_amount_for_trigger;
        // ComputeInjuredReaction: by where the last blow struck.
        let p = &*ctx.pawn;
        let at = e.damage.as_ref().map_or(p.loc, |d| d.hit_location);
        let to_hit = ue::normal((at - p.loc) * Vec3::new(1.0, 1.0, 0.0));
        let (x, y, _) = super::weapon::axes(0.0, p.yaw);
        let d = to_hit.dot(x);
        self.current = if d > 0.7 {
            self.from[0]
        } else if d < -0.7 {
            self.from[1]
        } else if to_hit.dot(y) > 0.0 {
            self.from[2]
        } else {
            self.from[3]
        };
        b.core.set_ws_bool(htn::WS_BALANCED, false);
        self.time_since_injury = 0.0;
    }
}

/// TnHealthLevelSensor: which of its health thresholds its health is under
/// (WS_Health: the first, else past the last), its health status effects
/// following it (TnHtnAiController.SetWorldStateIntValue's PlayHealthStatusFX).
#[derive(Clone, Debug, Default)]
pub struct HealthLevelSensor {
    pub thresholds: Vec<f32>,
}

impl HealthLevelSensor {
    fn update(&mut self, b: &mut Brain, p: &mut Pawn) {
        let normalized = if p.health_max > 0.0 {
            p.health / p.health_max
        } else {
            0.0
        };
        let level = self
            .thresholds
            .iter()
            .position(|t| normalized < *t)
            .unwrap_or(self.thresholds.len()) as i32;
        if b.core.ws(htn::WS_HEALTH) != level {
            b.core.set_ws(htn::WS_HEALTH, level);
            p.play_health_status_fx(level);
        }
    }
}

/// The path following sensor's AbortMove (behaviour flag 32 off).
pub fn abort_move(b: &mut Brain, ctx: &mut Ctx) {
    with_sensor!(b, path, s => s.abort_move(b, ctx));
}

/// The sensors' shutdown (the controller's Cleanup as its pawn dies).
pub fn shutdown(b: &mut Brain, ctx: &mut Ctx) {
    with_sensor!(b, sniper_point, s => s.shutdown(b, ctx));
    with_sensor!(b, threat, s => s.shutdown(b, ctx));
    with_sensor!(b, tactical, s => s.shutdown(ctx));
    with_sensor!(b, drone, s => s.shutdown(ctx));
}

/// TnAttackPointSensor: the attack points its level gives it
/// (TnAttackPointDataStore: none on Escalation's maps), WS_AttackPointAvailable.
#[derive(Clone, Debug, Default)]
pub struct AttackPointSensor;

/// TnSquadSensor: while it sees its enemy, it tells its side (an Alarm about
/// its enemy: BroadcastTeamEvent).
#[derive(Clone, Debug, Default)]
pub struct SquadSensor;

impl SquadSensor {
    fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let Some(enemy) = b.core.enemy else { return };
        if !b.core.ws_bool(htn::WS_ENEMY_VISIBLE) {
            return;
        }
        ctx.shared
            .broadcast(b.core.entity, EventId::Alarm, Some(enemy));
    }
}

/// TnLineOfSightSearchSensor: once it has lost sight of its enemy, a search out
/// across the navigation mesh from where it stands, a point a frame, for one it
/// would see the enemy's suspected place from (within its desired distance, if
/// it has one; else the nearest of those it sees it from): WS_LOSSearchAvailable
/// and LR_SearchLocation.
#[derive(Clone, Debug)]
pub struct LosSearchSensor {
    /// _State: searching (or idle).
    pub searching: bool,
    pub trace_offset: Vec3,
    pub trace_extents: Vec3,
    pub desired_distance: f32,
    pub searched: Vec<u32>,
    pub index: usize,
    pub point_with_los: Option<u32>,
    pub fallback: Option<u32>,
    pub fallback_distance_sq: f32,
}

impl LosSearchSensor {
    pub fn new(trace_height: f32, trace_extents: Vec3, desired_distance: f32) -> LosSearchSensor {
        LosSearchSensor {
            searching: false,
            trace_offset: Vec3::Z * trace_height,
            trace_extents,
            desired_distance,
            searched: Vec::new(),
            index: 0,
            point_with_los: None,
            fallback: None,
            fallback_distance_sq: f32::MAX,
        }
    }

    fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        if self.searching {
            self.update_searching(b, ctx);
        } else {
            let visible = b.core.ws_bool(htn::WS_ENEMY_VISIBLE);
            if !visible && !self.has_los(b, ctx, self.point_with_los) {
                self.restart(b, ctx);
            }
        }
    }

    fn update_searching(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        if b.core.ws_bool(htn::WS_ENEMY_VISIBLE) {
            self.complete(b, ctx, None);
            return;
        }
        let Some(nav) = ctx.view.nav else {
            self.complete(b, ctx, None);
            return;
        };
        let p = &*ctx.pawn;
        let moves =
            crate::nav::Moves::ground(p.radius / ue::UU_PER_M, p.half_height * 2.0 / ue::UU_PER_M);
        match nav.breadth_search_step(&mut self.searched, &mut self.index, &moves) {
            None => {
                let f = self.fallback;
                self.complete(b, ctx, f);
            }
            Some(point) => {
                if self.has_los(b, ctx, Some(point)) {
                    self.complete(b, ctx, Some(point));
                }
            }
        }
    }

    /// DoesPointHaveLOS (and keeps the nearest that does, as its fallback).
    fn has_los(&mut self, b: &Brain, ctx: &mut Ctx, point: Option<u32>) -> bool {
        let (Some(point), Some(nav)) = (point, ctx.view.nav) else {
            return false;
        };
        let Some(np) = nav.points.get(point as usize) else {
            return false;
        };
        let start = ue::to_ue(np.at) + self.trace_offset;
        let end = b.core.fetch(super::brain::LR_SUSPECTED_ENEMY) + self.trace_offset;
        let d_sq = (end - start).length_squared();
        let sight = ctx.pawn.sight_radius;
        if d_sq > sight * sight {
            return false;
        }
        // (FastTrace with its extents: the level's walls, a box's width across.)
        if ctx.view.sweep_blocked(end, start, self.trace_extents.x) {
            return false;
        }
        if d_sq < self.fallback_distance_sq {
            self.fallback = Some(point);
            self.fallback_distance_sq = d_sq;
        }
        self.desired_distance <= 0.0 || d_sq < self.desired_distance * self.desired_distance
    }

    /// RestartSearch: from its anchor.
    fn restart(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        self.index = 0;
        self.searched.clear();
        self.fallback_distance_sq = f32::MAX;
        if let Some(root) = ctx.pawn.anchor {
            self.searched.push(root);
        }
        if b.core.ws(htn::WS_LOCATION) == super::brain::LR_SEARCH_POINT as i32 {
            b.core.set_ws(htn::WS_LOCATION, 0);
        }
        b.core.set_ws_bool(htn::WS_LOS_SEARCH_AVAILABLE, false);
        self.searching = true;
    }

    /// SearchComplete.
    fn complete(&mut self, b: &mut Brain, ctx: &mut Ctx, point: Option<u32>) {
        self.searching = false;
        let (Some(point), Some(nav)) = (point, ctx.view.nav) else {
            return;
        };
        self.point_with_los = Some(point);
        self.fallback = Some(point);
        self.fallback_distance_sq = f32::MAX;
        b.core.set_ws(htn::WS_LOCATION, 0);
        if let Some(np) = nav.points.get(point as usize) {
            b.core
                .store(super::brain::LR_SEARCH_POINT, ue::to_ue(np.at), Vec3::ZERO);
        }
        b.core.set_ws_bool(htn::WS_LOS_SEARCH_AVAILABLE, true);
    }
}

// ---------------------------------------------------------------------------
// Sight
// ---------------------------------------------------------------------------

/// AI.PointInsideVisionCone: within its view's cone (and its vision's height of
/// its view's plane, if it has one).
pub fn point_inside_vision_cone(p: &Pawn, point: Vec3) -> bool {
    let view_dir = p.look_direction();
    if p.vision_half_height > 0.0 {
        let local =
            super::weapon::to_local(point - p.loc, ue::pitch_of(view_dir), ue::yaw_of(view_dir));
        if local.z.abs() > p.vision_half_height {
            return false;
        }
    }
    ue::normal(point - p.loc).dot(view_dir) >= p.peripheral_vision
}

/// LinesOfSightManager.StaticHasLineOfSight: nothing of the level's between its
/// eyes and the other's eyes or middle (each pair's answer kept a moment, as
/// the manager spreads its traces over frames, unless forced).
pub fn line_of_sight(ctx: &mut Ctx, other: Entity, force: bool) -> bool {
    let key = (ctx.pawn.entity, other);
    let now = ctx.view.now;
    if !force
        && let Some((v, t)) = ctx.shared.los.get(&key)
        && now - t < LOS_REFRESH
    {
        return *v;
    }
    let Some(a) = ctx.view.actor(other) else {
        return false;
    };
    let eyes = ctx.pawn.eyes();
    let other_eyes = ctx
        .view
        .player(other)
        .map_or(a.loc + Vec3::Z * a.half_height * 0.8, |p| p.eyes);
    let v = ctx.view.fast_trace(eyes, other_eyes) || ctx.view.fast_trace(eyes, a.loc);
    ctx.shared.los.insert(key, (v, now));
    v
}

/// How long a line of sight's answer holds.
const LOS_REFRESH: f32 = 0.2;

/// AI.CanSeePawn.
pub fn can_see_pawn(ctx: &mut Ctx, other: Entity, force: bool) -> bool {
    let Some(a) = ctx.view.actor(other) else {
        return false;
    };
    let cloaked = ctx.view.player(other).is_some_and(|p| p.cloaked);
    !cloaked && point_inside_vision_cone(ctx.pawn, a.loc) && line_of_sight(ctx, other, force)
}

/// The engine's sight check (AController::SeePawn from the seen pawns' ShowSelf,
/// every 0.15 to 0.25 s): each player it can see makes SeeEnemy (SawEnemy, or
/// LostSightOfEnemy for its enemy cloaked); its enemy unseen makes
/// EnemyNotVisible.
pub fn sight_check(b: &mut Brain, ctx: &mut Ctx) {
    b.sensors.sight_counter -= ctx.dt;
    if b.sensors.sight_counter >= 0.0 {
        return;
    }
    b.sensors.sight_counter += 0.15 + 0.1 * b.core.rng.frand();
    if !b.core.is_active() || ctx.pawn.dying {
        return;
    }
    let players: Vec<(Entity, Vec3, bool, bool)> = ctx
        .view
        .players
        .iter()
        .map(|p| (p.entity, p.loc, p.alive, p.cloaked))
        .collect();
    let mut events = Vec::new();
    let mut enemy_seen = false;
    for (e, loc, alive, cloaked) in players {
        if !alive {
            continue;
        }
        // SeePawn: within its sight radius (far ones sometimes skipped), its view's
        // cone, and in its line of sight.
        let max = ctx.pawn.sight_radius;
        let dist = (loc - ctx.pawn.loc).length();
        if dist > max {
            continue;
        }
        if b.core.rng.frand() * dist > 0.1 * max {
            continue;
        }
        if !point_inside_vision_cone(ctx.pawn, loc) || !line_of_sight(ctx, e, false) {
            continue;
        }
        if Some(e) == b.core.enemy {
            enemy_seen = true;
        }
        if !cloaked {
            events.push(Event::about(EventId::SawEnemy, Some(e)));
        } else if Some(e) == b.core.enemy {
            events.push(Event::about(EventId::LostSightOfEnemy, Some(e)));
        }
    }
    // CheckEnemyVisible.
    if let Some(enemy) = b.core.enemy
        && !enemy_seen
        && !line_of_sight(ctx, enemy, false)
    {
        events.push(Event::about(EventId::LostSightOfEnemy, Some(enemy)));
    }
    for e in events {
        b.handle_event(ctx, e);
    }
}

/// The type's AI.EAiCharacterType number, from its name.
pub fn ai_type_number(name: &str) -> i32 {
    const NAMES: [&str; 26] = [
        "AICT_Invalid",
        "AICT_Guard",
        "AICT_Swarmer",
        "AICT_Skirmisher",
        "AICT_Soldier",
        "AICT_Sniper",
        "AICT_Gnat",
        "AICT_Leaper",
        "AICT_Enforcer",
        "AICT_Bruiser",
        "AICT_Shotgunner",
        "AICT_Jet",
        "AICT_Titan",
        "AICT_RocketTrooper",
        "AICT_Rumble",
        "AICT_Laserbeak",
        "AICT_OptimusPrime",
        "AICT_Megatron",
        "AICT_Bruticus",
        "AICT_ScriptableDuder",
        "AICT_AttackDrone",
        "AICT_ExplosiveDrone",
        "AICT_SupportDrone",
        "AICT_HeavyGunner",
        "AICT_SharpShooter",
        "AICT_Scrapper",
    ];
    NAMES.iter().position(|n| *n == name).unwrap_or(0) as i32
}
