//! An enemy's melee (its TnMeleeManager, as the AI drives it through its
//! TnMeleeService): an attack from its melee set (TnMeleeSet: the soldiers'
//! Soldier_MS, its weapon attack and its combo, both AI_Melee_01) played through
//! the animator, the pawn slowed while it swings, and the blow its animation's
//! damage sweep notify makes (a box ahead of it striking each enemy once, for the
//! attack's damage; the pawn stops as it reaches its victim).

use std::sync::Arc;

use bevy::prelude::*;

use super::anim::{AnimParams, Callback};
use super::brain::{Brain, Ctx};
use super::data::Behaviour;
use super::pawn::Pawn;
use super::shared::PlayerDamage;
use super::ue;

/// TnMeleeSet.ETnMeleeType: the combo, the weapon attack, and any (an attack of
/// any type answers it, and an attack of type any answers any request).
pub const MELEE_COMBO: u8 = 0;
pub const MELEE_WEAPON_ATTACK: u8 = 1;
pub const MELEE_ANY: u8 = 10;

/// ETnMeleeType's names, in their order.
const MELEE_TYPES: [&str; 12] = [
    "MELEE_Combo",
    "MELEE_WeaponAttack",
    "MELEE_ShoulderSlam",
    "MELEE_Whirlwind",
    "MELEE_BerzerkAttack",
    "MELEE_SlamAttack",
    "MELEE_Stampede",
    "MELEE_Stampede_Miss",
    "MELEE_Stealth",
    "MELEE_None",
    "MELEE_Any",
    "MELEE_SuperMelee",
];

/// The melee manager's callbacks (its MeleeAnimationComplete, kind 2): their
/// owner this bit (apart from the operators' serial numbers and the pawn's own 0)
/// with the number of the attack whose animation it is.
pub const MELEE_CALLBACK: u32 = 1 << 31;

/// The callback for attack number `serial`'s animation.
fn melee_done(serial: u32) -> Callback {
    Callback {
        owner: MELEE_CALLBACK | serial,
        kind: 2,
    }
}

/// A melee set's attack (TnMeleeAttack).
#[derive(Clone, Debug)]
pub struct MeleeAttack {
    pub melee_type: u8,
    /// Its MeleeWeaponTypes include MWT_Any.
    pub any_weapon: bool,
    pub anim: AnimParams,
    pub damage: f32,
    pub damage_kind: super::brain::DamageKind,
    pub forward_momentum: f32,
    pub vertical_momentum: f32,
    pub ground_speed_multiplier: f32,
    pub disable_root_motion_after_contact: bool,
    pub abort_after_hit: bool,
}

/// A damage sweep (TnAnimNotify_DamageSweep's SweepInfo): when in its
/// animation it starts and for how long, and its box ahead of the pawn (its
/// centre from the pawn's middle and its half sizes, uu, in the pawn's axes).
#[derive(Clone, Copy, Debug)]
pub struct SweepNotify {
    pub at: f32,
    pub secs: f32,
    pub offset: Vec3,
    pub half: Vec3,
}

/// The melee manager's state.
#[derive(Clone, Debug, Default)]
pub struct Melee {
    pub attacks: Vec<MeleeAttack>,
    pub attacking: Option<usize>,
    /// The attack's number (each attack its own: its animation's callbacks carry
    /// it, animation_complete).
    pub serial: u32,
    pub last_type: u8,
    /// The attack's time and its sweep's victims so far.
    pub time: f32,
    pub struck: Vec<Entity>,
    pub contact: bool,
    /// The attack's sweeps (its animation's notifies).
    pub sweeps: Vec<SweepNotify>,
    /// Its attack struck a foe (get_LastMeleeHitPawn), and hurt it
    /// (_LastMeleeHurtPawn: its blow not stopped by his shield).
    pub hit_pawn: bool,
    pub hurt_pawn: bool,
    /// Its impact damage sweep, while one runs (its _ImpactDamageSweeper: an
    /// animation's TnAnimNotify_ImpactDamageSweep, the Leaper's booster vent and
    /// its spawn's landing), whether it attacks or not.
    pub impact: Option<ImpactSweep>,
    /// Its attacks' damage scale (TnMeleeManager's MeleeDamageModifier: the
    /// product of its buffs that modify it, an Escalation Swarmer's
    /// TnBuffModifySwarmerMeleeDamage).
    pub damage_scale: f32,
}

impl Melee {
    /// The type's melee set (its chassis' MeleeSet: Melee.Attacks[i]); the
    /// soldiers' (Soldier_MS) for one the data doesn't give.
    pub fn for_type(b: &Behaviour) -> Melee {
        let n = b.n("Melee.Attacks");
        let attacks = if n > 0 {
            (0..n).map(|i| attack_from_props(b, i)).collect()
        } else if b.class == "TnSoldierCharacterBlueprint" {
            soldier_attacks()
        } else {
            Vec::new()
        };
        Melee {
            attacks,
            damage_scale: 1.0,
            ..Default::default()
        }
    }

    pub fn is_attacking(&self) -> bool {
        self.attacking.is_some()
    }
}

/// A melee set's attack from the data (TnMeleeManager.PlayMeleeAttack's params:
/// its animation's name, blends, root motion, mask, priority and looping).
fn attack_from_props(b: &Behaviour, i: usize) -> MeleeAttack {
    let k = |f: &str| format!("Melee.Attacks[{i}].{f}");
    let s = |f: &str| b.get(&k(f));
    let melee_type = s("MeleeType")
        .and_then(|t| MELEE_TYPES.iter().position(|n| *n == t))
        .unwrap_or(MELEE_ANY as usize) as u8;
    let anim = AnimParams {
        name: Arc::from(s("AnimName").unwrap_or("None")),
        blend_in: b.f(&k("BlendInTime"), 0.15),
        blend_out: b.f(&k("BlendOutTime"), 0.15),
        root_motion: match s("RootMotionMode") {
            Some("RMM_Translate") => super::anim::RMM_TRANSLATE,
            Some("RMM_Velocity") => super::anim::RMM_VELOCITY,
            Some("RMM_Accel") => super::anim::RMM_ACCEL,
            _ => super::anim::RMM_IGNORE,
        },
        mask: match s("AnimationMaskType") {
            Some("CAMT_UpperBody") => super::anim::MASK_UPPER_BODY,
            Some("CAMT_LeftArm") => super::anim::MASK_LEFT_ARM,
            Some("CAMT_Weapon") => super::anim::MASK_WEAPON,
            Some("CAMT_Shoulder") => super::anim::MASK_SHOULDER,
            _ => super::anim::MASK_FULL_BODY,
        },
        priority: if s("AnimationPriority") == Some("CAPR_LowPriority") {
            0
        } else {
            1
        },
        looping: b.b(&k("Looping"), false),
        ..Default::default()
    };
    MeleeAttack {
        melee_type,
        any_weapon: (0..b.n(&k("MeleeWeaponTypes")).max(1)).any(|j| {
            b.get(&format!("Melee.Attacks[{i}].MeleeWeaponTypes[{j}]"))
                .or_else(|| s("MeleeWeaponTypes"))
                == Some("MWT_Any")
        }),
        anim,
        damage: b.f(&k("Damage"), 25.0),
        damage_kind: super::brain::DamageKind::Melee,
        forward_momentum: b.f(&k("HitMomentum.ForwardMomentum"), 0.0),
        vertical_momentum: b.f(&k("HitMomentum.VerticalMomentum"), 0.0),
        ground_speed_multiplier: b.f(&k("GroundSpeedMultiplier"), 1.0),
        disable_root_motion_after_contact: b.b(&k("DisableRootMotionAfterContact"), false),
        abort_after_hit: b.b(&k("AbortAfterHit"), false),
    }
}

/// The soldiers' Soldier_MS: its weapon attack (over its upper body) and its
/// combo (all of it), both AI_Melee_01.
fn soldier_attacks() -> Vec<MeleeAttack> {
    let anim = |mask: u8| AnimParams {
        mask,
        name: Arc::from("AI_Melee_01"),
        blend_in: 0.15,
        blend_out: 0.35,
        root_motion: super::anim::RMM_IGNORE,
        priority: 1,
        ..Default::default()
    };
    let attack = |melee_type: u8, mask: u8| MeleeAttack {
        melee_type,
        any_weapon: true,
        anim: anim(mask),
        damage: 25.0,
        damage_kind: super::brain::DamageKind::Melee,
        forward_momentum: 0.0,
        vertical_momentum: 0.0,
        ground_speed_multiplier: 0.75,
        disable_root_motion_after_contact: true,
        abort_after_hit: false,
    };
    vec![
        attack(MELEE_WEAPON_ATTACK, super::anim::MASK_UPPER_BODY),
        attack(MELEE_COMBO, super::anim::MASK_FULL_BODY),
    ]
}

/// TnMeleeSet.FindAttack: the attack answering the request (its type any, or the
/// request's) for the pawn's weapon's melee type (an AI's: any, MWT_Any), the
/// last of those for any weapon if none is for its own.
pub fn find_attack(attacks: &[MeleeAttack], melee_type: u8) -> Option<usize> {
    let mut backup = None;
    for (i, a) in attacks.iter().enumerate() {
        if a.melee_type != MELEE_ANY && a.melee_type != melee_type {
            continue;
        }
        if a.any_weapon {
            backup = Some(i);
        }
    }
    backup
}

/// TnRobotMeleeService.IsMeleeing: attacking, with an attack of the type asked
/// for (any, for MELEE_Any).
pub fn is_meleeing(p: &Pawn, melee_type: u8) -> bool {
    let m = &p.melee;
    match m.attacking {
        None => false,
        Some(_) => melee_type == MELEE_ANY || m.last_type == melee_type,
    }
}

/// StartMeleeAttack (Idle's: the set's attack of the type, played).
pub fn start_attack(b: &mut Brain, ctx: &mut Ctx, melee_type: u8) {
    let p = &mut *ctx.pawn;
    if p.melee.attacking.is_some() {
        return;
    }
    let Some(i) = find_attack(&p.melee.attacks, melee_type) else {
        return;
    };
    // (DoMeleeAttack: an attack gives a cloaked pawn away, ExposeSelf.)
    p.cloak.expose();
    p.melee.attacking = Some(i);
    p.melee.serial = p.melee.serial.wrapping_add(1) & !MELEE_CALLBACK;
    p.melee.last_type = melee_type;
    p.melee.time = 0.0;
    p.melee.struck.clear();
    p.melee.contact = false;
    p.melee.hit_pawn = false;
    p.melee.hurt_pawn = false;
    // Attacking.PlayMeleeAttack.
    p.anim.stop(0.0, None);
    let attack = p.melee.attacks[i].clone();
    p.anim.enqueue(
        attack.anim.clone(),
        Some(melee_done(p.melee.serial)),
        Some(melee_done(p.melee.serial)),
    );
    p.speed_multiplier = attack.ground_speed_multiplier;
    let _ = b;
}

/// AbortMeleeAttack.
pub fn abort_attack(_b: &mut Brain, ctx: &mut Ctx) {
    let p = &mut *ctx.pawn;
    if p.melee.attacking.is_none() {
        return;
    }
    p.anim.stop(0.0, None);
    end_attack(p);
}

/// Attacking.EndState.
fn end_attack(p: &mut Pawn) {
    if let Some(i) = p.melee.attacking.take() {
        let name = p.melee.attacks[i].anim.name.clone();
        p.anim.stop(0.0, Some(&name));
    }
    p.speed_multiplier = 1.0;
    p.melee.sweeps.clear();
}

/// Attack number `serial`'s animation finished or was stopped (Attacking's
/// MeleeAnimationComplete: back to idle), if that attack is the one going on (Idle's
/// is nothing). The game's animator calls it back as the animation stops, before
/// another attack can start; here the callback comes the next frame, by when the
/// next attack may have begun: the attack's own number keeps one attack's
/// callback from ending the next. (Without it, an attack given up mid-swing, its
/// plan dropped for a better one as its enemy came nearer, ended the next attack
/// the frame after it began, which stopped that one's animation as the next began,
/// and so on: the soldier stood restarting its swing every other frame, striking
/// nothing and never firing, for as long as its enemy stayed near.)
pub fn animation_complete(p: &mut Pawn, serial: u32) {
    if p.melee.attacking.is_some() && p.melee.serial == serial {
        p.melee.attacking = None;
        p.speed_multiplier = 1.0;
        p.melee.sweeps.clear();
    }
}

/// The attack's frame: its sweeps striking, and it stopping at its victim
/// (Attacking.OnUpdate, DisableMotionOnContact); and its impact sweep's (Update:
/// its _ImpactDamageSweeper's, attacking or not).
pub fn update(
    p: &mut Pawn,
    ctx_view: &super::world::WorldView,
    enemy: Option<Entity>,
    dt: f32,
    out: &mut Vec<PlayerDamage>,
) {
    update_impact(p, ctx_view, dt, out);
    let Some(i) = p.melee.attacking else { return };
    let attack = p.melee.attacks[i].clone();
    let t0 = p.melee.time;
    p.melee.time += dt;
    // Contact with its target: no more root motion or movement.
    if attack.disable_root_motion_after_contact
        && let Some(v) = enemy.and_then(|e| ctx_view.actor(e))
    {
        let in_front = (v.loc - p.loc).dot(p.forward());
        if in_front < v.radius + p.radius {
            p.vel = Vec3::ZERO;
            p.accel = Vec3::ZERO;
            p.speed_multiplier = 0.0;
            p.melee.contact = true;
        }
    }
    let sweeps = p.melee.sweeps.clone();
    for s in sweeps {
        if p.melee.time < s.at || t0 > s.at + s.secs {
            continue;
        }
        let centre = p.loc + super::weapon::from_local(s.offset, 0.0, p.yaw);
        for pl in &ctx_view.players {
            if !pl.alive || p.melee.struck.contains(&pl.entity) {
                continue;
            }
            // The box (its axes the pawn's) against the player's cylinder.
            let local = super::weapon::to_local(pl.loc - centre, 0.0, p.yaw);
            let near = local.clamp(-s.half, s.half);
            let d = local - near;
            if ue::vsize2d(d) > pl.radius || d.z.abs() > pl.half_height {
                continue;
            }
            p.melee.struck.push(pl.entity);
            p.melee.hit_pawn = true;
            // ServerProcessMeleeHit: pushed away along the ground, up a little.
            let away = ue::normal((pl.loc - p.loc) * Vec3::new(1.0, 1.0, 0.0));
            let mut momentum = away * attack.forward_momentum.max(1.0);
            momentum.z = attack.vertical_momentum.max(1.0);
            // His health goes down unless his Flak Shield is up and the blow comes
            // at him from in front (flakshield.rs' block).
            if !(pl.shielded && away.dot(ue::yaw_vec(pl.yaw)) < 0.0) && pl.alive {
                p.melee.hurt_pawn = true;
            }
            out.push(PlayerDamage {
                player: pl.entity,
                by: p.entity,
                amount: attack.damage * p.melee.damage_scale,
                at: centre + super::weapon::from_local(near, 0.0, p.yaw),
                dir: momentum,
                melee: true,
                momentum: None,
                jam: false,
                shot: None,
                damage: "TnDamageTypeMelee",
            });
            if attack.abort_after_hit {
                end_attack(p);
                return;
            }
        }
    }
}

/// TnTraceSweeper: a box of its extent swept, each update, along the way its
/// socket went (from where it was to where it is; a step ahead if it hardly
/// moved), meeting each player once a sweep, as long as nothing of the level
/// stands between him and its owner (HasClearDirectTrace), for its time.
#[derive(Clone, Debug, Default)]
pub struct TraceSweeper {
    pub active: bool,
    pub socket: Arc<str>,
    pub extent: Vec3,
    pub duration: f32,
    pub last: Vec3,
    pub hit: Vec<Entity>,
}

impl TraceSweeper {
    /// BeginSweep (its hits cleared, from where its socket is).
    pub fn begin(&mut self, p: &Pawn, socket: Arc<str>, extent: Vec3, duration: f32) {
        self.socket = socket;
        self.extent = extent;
        self.duration = duration;
        self.hit.clear();
        self.active = true;
        self.last = self.location(p);
    }

    /// EndSweep.
    pub fn end(&mut self) {
        self.active = false;
    }

    /// GetAttachmentLocation: its socket's place, else the pawn's middle.
    fn location(&self, p: &Pawn) -> Vec3 {
        if self.socket.is_empty() || &*self.socket == "None" {
            p.loc
        } else {
            p.socket_location(&self.socket)
        }
    }

    /// Update: the players its box met this update (each with where it met him),
    /// its time run down (its sweep over as it runs out).
    pub fn update(
        &mut self,
        p: &Pawn,
        view: &super::world::WorldView,
        dt: f32,
    ) -> Vec<(Entity, Vec3)> {
        if !self.active {
            return Vec::new();
        }
        let mut current = self.location(p);
        if (self.last - current).length() < 0.1 {
            current = self.last + p.forward();
        }
        let grow = self.extent.x.max(self.extent.y);
        let mut met = Vec::new();
        for pl in &view.players {
            if !pl.alive || self.hit.contains(&pl.entity) {
                continue;
            }
            let Some(t) = ue::segment_cylinder(
                self.last,
                current,
                pl.loc,
                pl.radius + grow,
                pl.half_height + self.extent.z,
            ) else {
                continue;
            };
            if !view.fast_trace(p.loc, pl.loc) {
                continue;
            }
            self.hit.push(pl.entity);
            met.push((pl.entity, self.last + (current - self.last) * t));
        }
        self.last = current;
        self.duration -= dt;
        if self.duration <= 0.0 {
            self.active = false;
        }
        met
    }
}

/// An impact damage sweep (TnAnimNotify_ImpactDamageSweep, through the melee
/// manager's BeginImpactSweep): its sweeper, and what it does to each player it
/// meets (ProcessImpactHit): its damage (DamageOverride), whether its damage
/// type is a melee one, whether it pushes (RequestRespectForcesApplied), and the
/// push: forward from the pawn along the ground, to the side away from its
/// facing, and up (HitMomentum).
#[derive(Clone, Debug, Default)]
pub struct ImpactSweep {
    pub sweeper: TraceSweeper,
    pub damage: f32,
    /// Its damage type (the kill feed's).
    pub damage_type: &'static str,
    pub melee: bool,
    pub push: bool,
    pub forward: f32,
    pub sideways: f32,
    pub vertical: f32,
}

/// The impact sweep's frame: each player it meets takes its damage, and its push.
fn update_impact(
    p: &mut Pawn,
    view: &super::world::WorldView,
    dt: f32,
    out: &mut Vec<PlayerDamage>,
) {
    let Some(mut imp) = p.melee.impact.take() else {
        return;
    };
    for (e, at) in imp.sweeper.update(p, view, dt) {
        let Some(pl) = view.player(e) else { continue };
        if pl.team == super::world::AI_TEAM {
            continue;
        }
        let along = ue::normal2d(pl.loc - p.loc);
        let mut side = along.cross(Vec3::Z);
        if super::weapon::to_local(along, 0.0, p.yaw).y > 0.0 {
            side = -side;
        }
        let mut momentum = along * imp.forward.max(1.0) + side * imp.sideways;
        momentum.z = imp.vertical.max(1.0);
        out.push(PlayerDamage {
            player: e,
            by: p.entity,
            amount: imp.damage,
            at,
            dir: momentum,
            melee: imp.melee,
            momentum: imp.push.then_some(momentum),
            jam: false,
            shot: None,
            damage: imp.damage_type,
        });
    }
    if imp.sweeper.active {
        p.melee.impact = Some(imp);
    }
}
