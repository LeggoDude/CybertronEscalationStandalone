//! How an enemy reacts to being hit: TnHitReactionSensor (the damage of the last
//! seconds making a reaction likelier: its interruptive hit conditions, met,
//! make WS_Damaged for the planner's hit reaction tasks; else one of its full body
//! reactions may be chosen; an additive flinch plays on every hit with a push),
//! and TnHitReactionPicker (the reaction for the octant the hit came from, the
//! bone it struck, how the pawn moves).

use std::sync::Arc;

use bevy::prelude::*;

use super::anim::AnimParams;
use super::brain::{Brain, Ctx, DamageInfo, DamageKind, Event, EventId};
use super::data::Behaviour;
use super::htn;
use super::pawn::Pawn;
use super::ue;

/// TnHitReactionPicker.TnHitReactionAnim.
#[derive(Clone, Debug, Default)]
pub struct HitReactionAnim {
    pub reaction: AnimParams,
    /// The octants it is for (GetOctantMask's bits: 1 front right, 2 front left,
    /// 4 back right, 8 back left, 32 right front, 64 right back, 128 left front,
    /// 256 left back).
    pub octants: u32,
    pub any_bone: bool,
    pub required_bones: Vec<Arc<str>>,
    pub requires_speed: bool,
    /// ETnMotionDirection (0 any) and EMotionSpeed it needs.
    pub motion_direction: u8,
    pub speed: u8,
}

/// TnHitReactionPicker.
#[derive(Clone, Debug, Default)]
pub struct HitReactionPicker {
    pub reactions: Vec<HitReactionAnim>,
}

/// ETnMotionDirection by name.
fn motion_direction(name: &str) -> u8 {
    match name {
        "MD_Forward" => 1,
        "MD_Right" => 2,
        "MD_Backward" => 3,
        "MD_Left" => 4,
        "MD_None" => 5,
        _ => 0,
    }
}

/// EMotionSpeed by name.
fn motion_speed(name: &str) -> u8 {
    match name {
        "MS_Medium" => 1,
        "MS_Fast" => 2,
        _ => 0,
    }
}

impl HitReactionPicker {
    /// A picker from the behaviour's values under `key` (an object reference's
    /// flattened values).
    pub fn from_props(b: &Behaviour, key: &str) -> Option<HitReactionPicker> {
        let n = b.n(&format!("{key}.Reactions"));
        if n == 0 {
            return None;
        }
        let base = AnimParams {
            priority: 1,
            ..Default::default()
        };
        let reactions = (0..n)
            .map(|i| {
                let k = |f: &str| format!("{key}.Reactions[{i}].{f}");
                let flag = |f: &str| b.b(&k(f), false);
                let all_front = flag("All_Front");
                let all_back = flag("All_Back");
                let bits = [
                    (flag("Front_Right") || all_front, 1),
                    (flag("Front_Left") || all_front, 2),
                    (flag("Back_Right") || all_back, 4),
                    (flag("Back_Left") || all_back, 8),
                    (flag("Right_Front") || all_front, 32),
                    (flag("Right_Back") || all_back, 64),
                    (flag("Left_Front") || all_front, 128),
                    (flag("Left_Back") || all_back, 256),
                ];
                HitReactionAnim {
                    reaction: AnimParams::from_props(b, &k("HitReaction"), &base),
                    octants: bits.iter().filter(|(on, _)| *on).map(|(_, v)| v).sum(),
                    any_bone: flag("AnyBone"),
                    required_bones: b
                        .get(&k("RequiredBoneNames"))
                        .unwrap_or("")
                        .split_whitespace()
                        .map(Arc::from)
                        .collect(),
                    requires_speed: flag("RequiresSpeed"),
                    motion_direction: motion_direction(
                        b.get(&k("RequredMotionDirection")).unwrap_or("MD_Any"),
                    ),
                    speed: motion_speed(b.get(&k("RequredSpeed")).unwrap_or("MS_Slow")),
                }
            })
            .collect();
        Some(HitReactionPicker { reactions })
    }

    /// ChooseHitReaction: the first reaction for the hit's octant, bone and the
    /// pawn's motion (none: empty params).
    pub fn choose(&self, p: &Pawn, tdi: &DamageInfo) -> AnimParams {
        let octant = find_octant(-tdi.momentum, p.yaw);
        let dir = get_motion_direction(p);
        let speed = p.motion.speed;
        let bone = tdi.bone.as_deref().unwrap_or("");
        let mirror = mirror_bone(bone);
        let mirrored = p.stance.handedness == 1;
        for r in &self.reactions {
            if !can_use_for_bone(bone, &mirror, mirrored, r) {
                continue;
            }
            if r.motion_direction != dir && r.motion_direction != 0 {
                continue;
            }
            if r.requires_speed && r.speed != speed {
                continue;
            }
            if octant & r.octants == octant {
                return r.reaction.clone();
            }
        }
        AnimParams::default()
    }
}

/// The skeleton's mirrored bone (FindBonesMirror: its left for its right).
fn mirror_bone(bone: &str) -> String {
    if let Some(rest) = bone.strip_prefix("L_") {
        format!("R_{rest}")
    } else if let Some(rest) = bone.strip_prefix("R_") {
        format!("L_{rest}")
    } else {
        bone.to_string()
    }
}

/// CanUseReactionForBone.
fn can_use_for_bone(bone: &str, mirror: &str, mirrored: bool, r: &HitReactionAnim) -> bool {
    if r.any_bone {
        return true;
    }
    let want = if mirrored && r.reaction.mirroring_allowed {
        mirror
    } else {
        bone
    };
    r.required_bones.iter().any(|b| &**b == want)
}

/// GetMotionDirection: none standing still, else the way it moves relative to
/// its facing.
fn get_motion_direction(p: &Pawn) -> u8 {
    if p.vel == Vec3::ZERO {
        return 5;
    }
    let local = super::weapon::to_local(ue::normal(p.vel * Vec3::new(1.0, 1.0, 0.0)), 0.0, p.yaw);
    if local.x.abs() > local.y.abs() {
        if local.x > 0.0 { 1 } else { 3 }
    } else if local.y > 0.0 {
        2
    } else {
        4
    }
}

/// FindOctantFromDirection.
fn find_octant(dir: Vec3, yaw: f32) -> u32 {
    let local = super::weapon::to_local(ue::normal(dir * Vec3::new(1.0, 1.0, 0.0)), 0.0, yaw);
    let front = local.x > 0.0;
    let right = local.y > 0.0;
    if local.x.abs() > local.y.abs() {
        if front {
            if right { 1 } else { 2 }
        } else if right {
            4
        } else {
            8
        }
    } else if right {
        if front { 32 } else { 64 }
    } else if front {
        128
    } else {
        256
    }
}

/// A hit reaction of the sensor's blueprint (TnHitReactionBase's subclasses).
#[derive(Clone, Debug)]
pub enum HitReaction {
    /// TnHitReactionRequiredDamageType: by the direction the hit came from, for
    /// one damage type.
    RequiredDamageType {
        damage: DamageKind,
        front: AnimParams,
        back: AnimParams,
        left: AnimParams,
        right: AnimParams,
    },
    /// TnHitReactionFrontQuadrant: the front's four quarters, or the back.
    FrontQuadrant {
        top_left: AnimParams,
        top_right: AnimParams,
        bottom_left: AnimParams,
        bottom_right: AnimParams,
        back: AnimParams,
    },
}

impl HitReaction {
    /// ChooseHitReaction.
    fn choose(&self, p: &Pawn, tdi: &DamageInfo, chance: f32, roll: f32) -> Option<AnimParams> {
        let params = match self {
            HitReaction::RequiredDamageType {
                damage,
                front,
                back,
                left,
                right,
            } => {
                if tdi.damage_type != *damage {
                    return None;
                }
                let to_hit = ue::normal2d(tdi.hit_location - p.loc);
                let (x, y, _) = super::weapon::axes(0.0, p.yaw);
                let d = to_hit.dot(x);
                if d > 0.7 {
                    front.clone()
                } else if d < -0.7 {
                    back.clone()
                } else if to_hit.dot(y) > 0.0 {
                    right.clone()
                } else {
                    left.clone()
                }
            }
            HitReaction::FrontQuadrant {
                top_left,
                top_right,
                bottom_left,
                bottom_right,
                back,
            } => {
                if roll > chance {
                    return None;
                }
                if tdi.momentum.dot(p.forward()) < 0.0 {
                    let (_, y, _) = super::weapon::axes(0.0, p.yaw);
                    if tdi.hit_location.z >= p.loc.z {
                        // GetFrontTopReaction: about the chest.
                        let chest = p.loc + Vec3::Z * p.half_height * 0.45;
                        let dir = ue::normal(tdi.hit_location - chest);
                        if dir.dot(y) > 0.0 {
                            top_right.clone()
                        } else if dir.dot(-y) > 0.0 {
                            top_left.clone()
                        } else {
                            AnimParams::default()
                        }
                    } else {
                        let dir = ue::normal(tdi.hit_location - p.loc);
                        if dir.dot(y) > 0.0 {
                            bottom_right.clone()
                        } else if dir.dot(-y) > 0.0 {
                            bottom_left.clone()
                        } else {
                            AnimParams::default()
                        }
                    }
                } else {
                    back.clone()
                }
            }
        };
        (!params.is_none()).then_some(params)
    }
}

/// An interruptive hit condition (TnInterruptiveHitCondition's subclasses): met
/// (with the weighted chance, unless it ignores it), WS_Damaged is set with its
/// context.
#[derive(Clone, Debug)]
pub enum HitCondition {
    /// TnInCoverHitCondition: hiding in cover that is cover.
    InCover { context: i32 },
    /// TnRequiredDamageTypeHitCondition (None: any of the game's damage types).
    RequiredDamageType {
        damage: Option<DamageKind>,
        context: i32,
    },
}

/// TnHitReactionSensor.
#[derive(Clone, Debug, Default)]
pub struct HitReactionSensor {
    pub conditions: Vec<HitCondition>,
    pub reactions: Vec<HitReaction>,
    pub additive: Option<HitReactionPicker>,
    pub weighted_chance: f32,
    /// The damage history (HmFloatValueHistory: each hit's damage and time) and
    /// how long it remembers.
    pub history: Vec<(f32, f32)>,
    pub sample_time: f32,
    pub chance_upper_bound: f32,
    /// The reaction chosen (get_HitReactionParams) and the hit (get_TakeDamageInfo).
    pub params: AnimParams,
    pub tdi: Option<DamageInfo>,
}

impl HitReactionSensor {
    /// The soldiers' (CreateHitReactionSensor: its blueprint's reactions, the in
    /// cover condition and the any damage one).
    pub fn soldier(b: &Behaviour) -> HitReactionSensor {
        HitReactionSensor::new(b, true)
    }

    /// A type's CreateHitReactionSensor: its blueprint's reactions and its
    /// interruptive conditions (in cover, if `in_cover`; any damage).
    pub fn new(b: &Behaviour, in_cover: bool) -> HitReactionSensor {
        let key = "HitReactionBlueprint";
        let mut reactions = Vec::new();
        let base = AnimParams {
            priority: 1,
            ..Default::default()
        };
        for i in 0..b.n(&format!("{key}.HitReactions")) {
            let k = format!("{key}.HitReactions[{i}]");
            let p = |f: &str, name: &str| {
                let mut d = base.clone();
                d.name = Arc::from(name);
                AnimParams::from_props(b, &format!("{k}.{f}"), &d)
            };
            match b.get(&format!("{k}.class")) {
                Some("TnHitReactionRequiredDamageTypeBlueprint") => {
                    reactions.push(HitReaction::RequiredDamageType {
                        damage: damage_kind(b.get(&format!("{k}.DamageType")).unwrap_or("")),
                        front: p("HitFromFrontAnimParams", "None"),
                        back: p("HitFromBackAnimParams", "None"),
                        left: p("HitFromLeftAnimParams", "None"),
                        right: p("HitFromRightAnimParams", "None"),
                    })
                }
                Some("TnHitReactionFrontQuadrantBlueprint") => {
                    reactions.push(HitReaction::FrontQuadrant {
                        top_left: p("FrontTopLeftReaction", "HitReaction_FrontTopLeft"),
                        top_right: p("FrontTopRightReaction", "HitReaction_FrontTopRight"),
                        bottom_left: p("FrontBottomLeftReaction", "HitReaction_FrontBottomLeft"),
                        bottom_right: p("FrontBottomRightReaction", "HitReaction_FrontBottomRight"),
                        back: p("BackReaction", "HitReaction_Back"),
                    })
                }
                _ => {}
            }
        }
        let mut conditions = Vec::new();
        if in_cover {
            conditions.push(HitCondition::InCover { context: 0 });
        }
        conditions.push(HitCondition::RequiredDamageType {
            damage: None,
            context: 0,
        });
        HitReactionSensor {
            conditions,
            reactions,
            additive: HitReactionPicker::from_props(b, &format!("{key}.AdditiveHitReactions")),
            sample_time: b.f(&format!("{key}.DamageHistorySampleTime"), 3.0),
            chance_upper_bound: b.f(&format!("{key}.DamageHistoryChanceToPlayUpperBound"), 150.0),
            params: AnimParams::default(),
            ..Default::default()
        }
    }

    pub fn initialize(&mut self, _b: &mut Brain, _ctx: &mut Ctx) {}

    /// OnUpdate: the damage history's old hits forgotten.
    pub fn update(&mut self, now: f32) {
        let keep = self.sample_time;
        self.history.retain(|(_, t)| now - t <= keep);
    }

    pub fn handle_event(&mut self, b: &mut Brain, ctx: &mut Ctx, e: &Event) {
        if e.id != EventId::TookDamage {
            return;
        }
        let Some(tdi) = e.damage.clone() else { return };
        self.history.push((tdi.damage, ctx.view.now));
        // CalculateWeightedChance.
        let sum: f32 = self.history.iter().map(|(d, _)| d).sum();
        self.weighted_chance = (sum / self.chance_upper_bound.max(1e-3)).clamp(0.0, 1.0);
        self.tdi = Some(tdi.clone());
        let Some(by) = tdi.instigator else { return };
        if ctx.view.same_team(ctx.pawn.entity, by) {
            return;
        }
        if let Some(add) = &self.additive
            && tdi.momentum != Vec3::ZERO
        {
            let params = add.choose(ctx.pawn, &tdi);
            ctx.pawn.anim.enqueue_additive(params, None, None);
        }
        // CanPlayFullBodyHitReaction (every damage type of the game's causes one).
        if !b.runner.interruptible()
            || !ctx.pawn.can_play_hit_reaction
            || ctx.pawn.dying
            || ctx.pawn.spawning
        {
            return;
        }
        if self.should_interrupt(b, &tdi) {
            return;
        }
        if !self.choose_hit_reaction(b, ctx, &tdi) {
            return;
        }
        b.core.set_ws_bool(htn::WS_BALANCED, false);
    }

    /// ShouldInterrupt: the first condition met.
    fn should_interrupt(&self, b: &mut Brain, tdi: &DamageInfo) -> bool {
        for c in &self.conditions {
            let roll = b.core.rng.frand();
            let (met, context) = match c {
                HitCondition::InCover { context } => (
                    b.core.ws(htn::WS_COVER_USAGE) == 1
                        && b.core.ws_ctx(htn::WS_ATTACK_POINT_AVAILABLE) != 0,
                    *context,
                ),
                HitCondition::RequiredDamageType { damage, context } => {
                    (damage.is_none_or(|d| d == tdi.damage_type), *context)
                }
            };
            if roll < self.weighted_chance && met {
                b.core.set_ws_bool(htn::WS_DAMAGED, true);
                b.core.set_ws_ctx(htn::WS_DAMAGED, context);
                return true;
            }
        }
        false
    }

    /// ChooseHitReaction.
    fn choose_hit_reaction(&mut self, b: &mut Brain, ctx: &Ctx, tdi: &DamageInfo) -> bool {
        for r in &self.reactions {
            let roll = b.core.rng.frand();
            if let Some(p) = r.choose(ctx.pawn, tdi, self.weighted_chance, roll) {
                self.params = p;
                return true;
            }
        }
        false
    }
}

/// A damage type by its class name.
pub fn damage_kind(class: &str) -> DamageKind {
    match class {
        "TnDamageTypeMelee" => DamageKind::Melee,
        "TnDamageTypeStun" => DamageKind::Stun,
        "TnDamageTypeLeapSmash" => DamageKind::LeapSmash,
        "TnDamageTypeVehicleExplosive" => DamageKind::VehicleExplosive,
        "TnDamageTypePlaneRocket" => DamageKind::PlaneRocket,
        "TnDamageTypeRammed" => DamageKind::Rammed,
        _ => DamageKind::Other,
    }
}
