//! The class abilities a build can take (customize.txt's `ability` lines) besides
//! the Infiltrator's EMP Grenade (emp.rs): the Scout's Cloak, the Leader's
//! Diffraction Barrier and Hover Slam, the Soldier's Whirlwind and Flak Shield and
//! the Scientist's Heal Beam and Battle Sentry, as the game's scripts and data run
//! them (TransGame.xxx's TnAbility classes, mp_shared.xxx's and MP_Array_BASE_m.xxx's
//! TransContent ones, TransCustomization.ini's AbilityResourceDatas).
//!
//! This is what they share (TnAbility, TnAbilityManager):
//!
//! * **The resource.** Each ability has a meter (AbilityResourceDatas: the entry
//!   whose GameTypes has "ESC", else its first): full as he spawns and each time he
//!   takes up his character. Pressing Ability (Q, robot form only) starts it if the
//!   meter holds ResourceRequired, taking InitialResourceDrain; while it is going the
//!   meter drains at ResourceDrainRate (running out ends it), otherwise it refills at
//!   ResourceRefillRate. The Intercooler support item refills it faster and drains it
//!   slower (equipment.rs's cooldown_scale), and some upgrades scale the rates.
//! * **The key.** A press starts it; a press while it is going ends it (IsCancellable:
//!   all but the Heal Beam, which handles its presses itself), but not within
//!   SpamPreventionTime of the last press that did something (1 s; the Hover's 0.25 s).
//!   Letting go does nothing.
//! * **Ends.** Going down or dying ends what is going (RobotDowned's
//!   ForceEndActiveAbilities); the robot's own (the Whirlwind, the Flak Shield, the
//!   Heal Beam and the Hover) end as he changes form; the Cloak goes on into the
//!   vehicle; the barrier and the sentry stand on their own.
//! * **The HUD** (robotAbility_mc, hud.rs): its icon (AbilityIcon<name>; the Heal
//!   Beam's and the Hover's are movie clips), the meter, blue while in use or short of
//!   a use, the ready sound as it comes ready, and the cancel icon while the Cloak or
//!   the Barrier is going.
//! * **Online.** Each player's PC runs his abilities; what the others need goes in his
//!   snapshot (net::AbilitySnap: which ability, its upgrade, what is going and how far
//!   in, whom his beam heals, his sentry) and in Ability events for the moments their
//!   copies act on (a sweep's or a slam's damage to their copies of the enemies, a
//!   sentry's shot, a reward). The one-off sounds go as his other sounds do.
//!
//! The Diffraction Barrier is here (it is the support item's barrier, equipment.rs);
//! the others have modules of their own: cloak.rs, whirlwind.rs, healbeam.rs,
//! flakshield.rs, hoverslam.rs and battlesentry.rs.

use std::collections::HashMap;
use std::sync::RwLock;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::controls::{Action, Controls, Mode};
use crate::hud::{HudAbility, HudState};
use crate::levelfx::FxSystems;
use crate::net::{self, JazzEvent};
use crate::sound::Sfx;
use crate::{
    Form, GltfClips, JazzOf, LocalPlayer, Player, PlayerInput, RemotePlayer, RobotForm, choose,
};

// ---------------------------------------------------------------------------
// The abilities
// ---------------------------------------------------------------------------

/// A class ability (the build's: customize.txt's ids).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    None,
    Cloak,
    Barrier,
    Hover,
    Whirlwind,
    HealBeam,
    Sentry,
    Shield,
}

/// The abilities by their ids in the builds, in Kind's order.
const IDS: [(&str, Kind); 7] = [
    ("Cloaking", Kind::Cloak),
    ("Barrier", Kind::Barrier),
    ("Hover", Kind::Hover),
    ("Whirlwind", Kind::Whirlwind),
    ("HealBeam", Kind::HealBeam),
    ("SpawnSentry", Kind::Sentry),
    ("ShieldAbility", Kind::Shield),
];

/// Each ability's upgrades (customize.txt's `skill` lines, in their order: the
/// build's upgrade is 1, 2 or 3 of them, 0 none).
const UPGRADES: [(&str, Kind, u8); 21] = [
    ("CloakResourceDrainRate", Kind::Cloak, 1),
    ("CloakResourceRefillRate", Kind::Cloak, 2),
    ("CloakExposeDelay", Kind::Cloak, 3),
    ("BarrierDrainRate", Kind::Barrier, 1),
    ("BarrierSlow", Kind::Barrier, 2),
    ("BarrierOneWayView", Kind::Barrier, 3),
    ("HoverGroundPoundIncreasedRadius", Kind::Hover, 1),
    ("HoverIncreasedMovement", Kind::Hover, 2),
    ("HoverMovementBasedDrainRate", Kind::Hover, 3),
    ("IncreaseWhirlwindDamage", Kind::Whirlwind, 1),
    ("DecreaseDamageTakenDuringWhirlwind", Kind::Whirlwind, 2),
    ("IncreaseWhirlwindSpeed", Kind::Whirlwind, 3),
    ("HealBeamResourceDrainRate", Kind::HealBeam, 1),
    ("HealBeamSelfHealReward", Kind::HealBeam, 2),
    ("HealBeamIncreasedDamageReward", Kind::HealBeam, 3),
    ("IncreaseSentryPower", Kind::Sentry, 1),
    ("SentryResourceDrainRate", Kind::Sentry, 2),
    ("SlowSentry", Kind::Sentry, 3),
    ("ShieldIncreasedMovement", Kind::Shield, 1),
    ("ShieldDecreaseDamage", Kind::Shield, 2),
    ("ShieldHealOnDamage", Kind::Shield, 3),
];

/// The upgrades by name (ClassAbility::upgrade's numbers).
pub const SPECTER: u8 = 1;
pub const CLOAK_INTERCOOLER: u8 = 2;
pub const SHADOW_STALKER: u8 = 3;
pub const FORTIFY: u8 = 1;
pub const DIVEBOMB: u8 = 1;
pub const BUG_OUT: u8 = 2;
pub const AERIAL_CAMPER: u8 = 3;
pub const POKE_ALPHA: u8 = 1;
pub const ENERGON_ABSORBERS: u8 = 2;
pub const FLURRY_OF_BLOWS: u8 = 3;
pub const EMPOWERED_BEAM: u8 = 1;
pub const ENERGON_RECHARGE: u8 = 2;
pub const BATTLE_MEDIC: u8 = 3;
pub const POWER_CORE: u8 = 1;
pub const RESOURCE_RECYCLER: u8 = 2;
pub const RUST_SENTRY: u8 = 3;
pub const BASHER: u8 = 1;
pub const BRUTE_FORCE: u8 = 2;
pub const BLASTPROOF: u8 = 3;

/// An ability's meter (its AbilityResourceDatas entry: ResourceRequired, ResourceMax,
/// InitialResourceDrain, ResourceDrainRate and ResourceRefillRate, a second).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meter {
    pub required: f32,
    pub max: f32,
    pub initial: f32,
    pub drain: f32,
    pub refill: f32,
}

/// The number of the upgrade with skill id `id` for `kind` (0: none).
pub fn upgrade_of(kind: Kind, id: &str) -> u8 {
    UPGRADES
        .iter()
        .find(|(i, k, _)| *i == id && *k == kind)
        .map_or(0, |(_, _, u)| *u)
}

impl Kind {
    /// The ability with this id in the builds (None for the EMP grenade's, or none).
    pub fn of_id(id: &str) -> Kind {
        IDS.iter()
            .find(|(i, _)| *i == id)
            .map_or(Kind::None, |(_, k)| *k)
    }

    /// Its number online (0 none).
    pub fn index(self) -> u8 {
        IDS.iter()
            .position(|(_, k)| *k == self)
            .map_or(0, |i| i as u8 + 1)
    }

    pub fn from_index(i: u8) -> Kind {
        IDS.get((i as usize).wrapping_sub(1))
            .map_or(Kind::None, |(_, k)| *k)
    }

    /// Its meter. The ESC entries where the ability has one ("CP,ESC", "MP,ESC",
    /// "ESC"); the Hover's only entry, MP's; and the Barrier's MP entry, a reusable
    /// class ability (its ESC entry, 1/1/1/0/0, is the purchasable item's single use:
    /// equipment.rs).
    pub fn meter(self) -> Meter {
        let m = |required, max, initial, drain, refill| Meter {
            required,
            max,
            initial,
            drain,
            refill,
        };
        match self {
            Kind::None => m(1.0, 1.0, 0.0, 0.0, 0.0),
            Kind::Cloak => m(10.0, 14.0, 2.0, 1.0, 1.0),
            Kind::Barrier => m(20.0, 20.0, 0.0, 2.0, 1.0),
            Kind::Hover => m(20.0, 20.0, 10.0, 2.0, 0.75),
            Kind::Whirlwind => m(20.0, 20.0, 10.0, 2.1, 1.0),
            Kind::HealBeam => m(1.0, 15.0, 0.0, 1.0, 2.0),
            Kind::Sentry => m(30.0, 30.0, 5.0, 0.5, 1.5),
            Kind::Shield => m(1.0, 4.5, 0.0, 0.25, 0.5),
        }
    }

    /// Its icon on the HUD (hud.txt's AbilityIcon<name>, or the movie clip the HUD's
    /// checkForComplexIcon swaps in: the Heal Beam's and the Hover's).
    pub fn icon(self) -> &'static str {
        match self {
            Kind::None => "AbilityIconNone",
            Kind::Cloak => "AbilityIconCloaking",
            Kind::Barrier => "AbilityIconBarrier",
            Kind::Hover => "mc_abilityIconHover",
            Kind::Whirlwind => "AbilityIconWhirlwind",
            Kind::HealBeam => "mc_abilityIconHealBeam",
            Kind::Sentry => "AbilityIconSpawnSentry",
            Kind::Shield => "AbilityIconShield",
        }
    }

    /// SpamPreventionTime (TnAbility's 1 s; TnAbilityHover's 0.25 s).
    fn spam_secs(self) -> f32 {
        if self == Kind::Hover { 0.25 } else { 1.0 }
    }

    /// IsCancellable: a press while it is going ends it (the Heal Beam's presses
    /// pick its target instead: healbeam.rs).
    fn cancellable(self) -> bool {
        !matches!(self, Kind::None | Kind::HealBeam)
    }

    /// The HUD shows its cancel icon while it is going (robotAbility_mc's
    /// updateShouldShowCancelIcon: TnAbilityCloaking and TnAbilityBarrier; its
    /// 'TnAbilitySentry' names no class, so the sentry never shows it).
    fn cancel_icon(self) -> bool {
        matches!(self, Kind::Cloak | Kind::Barrier)
    }

    /// The robot's own (GetRobotService): it ends as he changes form.
    pub fn robot_only(self) -> bool {
        matches!(
            self,
            Kind::Whirlwind | Kind::Shield | Kind::HealBeam | Kind::Hover
        )
    }
}

/// How an ability ended (the game's ForceEndAbilityReason): its meter ran out
/// (FEAR 0), its key was pressed again (FEAR 1), or something else ended it (his
/// going down, changing form, a cloak exposed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    RanOut,
    Cancelled,
    Forced,
}

/// A Jazz's class ability (Player::ability): which it is and its upgrade, its meter,
/// whether it is going, and each ability's own state. On his PC `use_abilities` runs
/// it; on the others' PCs his snapshot (`follow`) says what his copy shows.
#[derive(Default)]
pub(crate) struct ClassAbility {
    pub kind: Kind,
    pub upgrade: u8,
    pub resource: f32,
    /// Going (TnAbility.IsActive).
    pub active: bool,
    /// Started or ended this frame (and how): the abilities' systems act on it.
    pub started: bool,
    pub ended: Option<End>,
    /// The key pressed this frame while it was going, for an ability that takes it
    /// itself (the Heal Beam's next target, the Hover's slam), or to start one that
    /// checks first (the Heal Beam's target).
    pub pressed: bool,
    /// Seconds the key does nothing (SpamPreventionCooldowns), and the key last frame.
    guard: f32,
    button: bool,
    /// Ready last frame (the HUD's ready sound plays as it comes ready).
    ready: bool,
    /// Seconds into what is going (a snapshot's clock for the copies), and since it
    /// last ended (its clip blending out).
    pub clock: f32,
    pub since_end: f32,
    pub cloak: crate::cloak::CloakState,
    pub whirl: crate::whirlwind::WhirlState,
    pub heal: crate::healbeam::HealState,
    pub shield: crate::flakshield::ShieldState,
    pub hover: crate::hoverslam::HoverState,
    pub sentry: crate::battlesentry::SentryLink,
    /// His Diffraction Barrier standing (equipment.rs's Barrier).
    pub barrier: Option<Entity>,
}

impl ClassAbility {
    /// A new one of `kind` with `upgrade` (its meter full), as he takes up his
    /// character.
    pub fn new(kind: Kind, upgrade: u8) -> ClassAbility {
        ClassAbility {
            kind,
            upgrade,
            resource: kind.meter().max,
            ready: true,
            ..default()
        }
    }

    /// Ends it now (ForceEndAbility): its systems clean up this frame.
    pub fn end(&mut self, how: End) {
        if self.active {
            self.active = false;
            self.ended = Some(how);
        }
    }

    /// Its upgrade is `u`.
    pub fn has(&self, u: u8) -> bool {
        self.upgrade == u && u != 0
    }

    /// He can't shoot or strike (the Whirlwind spinning: it is his melee).
    pub fn blocks_attacks(&self) -> bool {
        self.kind == Kind::Whirlwind && self.active
    }

    /// The clip over his upper body or arm (main.rs's ability_clips): its role in
    /// robot_extras.txt, seconds into it, and its share of what plays there, blended
    /// in and out over the ability's BlendInTime and BlendOutTime (the Whirlwind's
    /// MELEE_Whirlwind: 0.15 and 0.5 s; the Heal Beam's HealAnimParams: 0.2 and 0.3 s;
    /// the shield's break, as a hit reaction, quickly). The Heal Beam's arm goes on
    /// round its loop as it blends out (healbeam.rs's clip_time: his hand held up).
    pub fn overlay(&self) -> Option<(&'static str, f32, f32)> {
        let blend = |role: &'static str, at: f32, (fade_in, fade_out): (f32, f32)| {
            if self.active {
                Some((role, at, (self.clock / fade_in).min(1.0)))
            } else if self.clock > 0.0 && self.since_end < fade_out {
                Some((role, at + self.since_end, 1.0 - self.since_end / fade_out))
            } else {
                None
            }
        };
        match self.kind {
            Kind::Whirlwind => blend(
                "whirlwind",
                crate::whirlwind::clip_time(self.clock),
                (0.15, 0.5),
            ),
            Kind::HealBeam => {
                let since = if self.active { 0.0 } else { self.since_end };
                blend(
                    "heal_beam",
                    crate::healbeam::clip_time(self.clock + since) - since,
                    (0.2, 0.3),
                )
            }
            Kind::Shield => self.shield.broke.map(|t| {
                let k = (t / 0.1).min(1.0) * ((crate::flakshield::BREAK_SECS - t) / 0.3).min(1.0);
                ("shield_break", t, k.max(0.0))
            }),
            _ => None,
        }
    }

    /// Fills its meter (the Heal Beam's reward, a sentry hit draining it...): by
    /// `amount` (negative drains), kept within it.
    pub fn add(&mut self, amount: f32) {
        self.resource = (self.resource + amount).clamp(0.0, self.kind.meter().max);
    }

    /// What goes in his snapshot (net::AbilitySnap).
    pub fn snap(&self) -> net::AbilitySnap {
        let mut flags = 0u8;
        if self.active {
            flags |= net::AbilitySnap::ACTIVE;
        }
        flags |= (self.hover.phase as u8 & 3) << 1;
        if self.shield.broken_left > 0.0 {
            flags |= net::AbilitySnap::SHIELD_BROKEN;
        }
        if self.heal.medic_left > 0.0 {
            flags |= net::AbilitySnap::MEDIC;
        }
        net::AbilitySnap {
            kind: self.kind.index(),
            upgrade: self.upgrade,
            flags,
            clock: self.clock,
            meter: (self.resource / self.kind.meter().max.max(1e-6) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8,
            target: self.heal.target_slot.unwrap_or(u8::MAX),
            sentry: self.sentry.snap(),
        }
    }

    /// Another player's copy: what his snapshot says (`fresh`: a new one), with the
    /// starts and ends it brings.
    pub fn follow(&mut self, s: &net::AbilitySnap, fresh: bool, dt: f32) {
        if !fresh {
            if self.active {
                self.clock += dt;
            }
            return;
        }
        let kind = Kind::from_index(s.kind);
        if kind != self.kind || s.upgrade != self.upgrade {
            // (A new character: what the old ability had going ends first.)
            if self.active {
                self.end(End::Forced);
            }
            self.kind = kind;
            self.upgrade = s.upgrade;
        }
        let active = s.flags & net::AbilitySnap::ACTIVE != 0;
        if active && !self.active {
            self.active = true;
            self.started = true;
        } else if !active && self.active {
            // (A shield ending broken broke.)
            let broke = s.flags & net::AbilitySnap::SHIELD_BROKEN != 0;
            self.end(if broke { End::RanOut } else { End::Forced });
        }
        self.clock = s.clock;
        // (His meter as his PC has it: the cloak runs low by it.)
        self.resource = s.meter as f32 / 255.0 * self.kind.meter().max;
        self.hover.phase = crate::hoverslam::Phase::from_bits((s.flags >> 1) & 3);
        self.shield.broken_left = if s.flags & net::AbilitySnap::SHIELD_BROKEN != 0 {
            self.shield.broken_left.max(0.1)
        } else {
            0.0
        };
        self.heal.medic_left = if s.flags & net::AbilitySnap::MEDIC != 0 {
            self.heal.medic_left.max(0.1)
        } else {
            0.0
        };
        self.heal.target_slot = (s.target != u8::MAX).then_some(s.target);
        self.sentry.follow(&s.sentry);
    }
}

// ---------------------------------------------------------------------------
// What the abilities do to his damage and speed
// ---------------------------------------------------------------------------

/// Each Jazz's damage dealt multiplier from his ability (set every frame by
/// `ability_buffs`, read by enemies.rs's Enemy::take for the blows he deals): the
/// Hover's TnBuffIncreaseDamageDuringHover and Battle Medic's
/// TnBuffIncreaseDamageHealBeamReward.
static DEALT: RwLock<Vec<(Entity, f32)>> = RwLock::new(Vec::new());

/// The damage `by` deals scaled by his buffs (1 for anyone else).
pub fn dealt_scale(by: Option<Entity>) -> f32 {
    let Some(by) = by else {
        return 1.0;
    };
    DEALT
        .read()
        .ok()
        .and_then(|d| d.iter().find(|(e, _)| *e == by).map(|(_, s)| *s))
        .unwrap_or(1.0)
}

/// What damage to him is scaled by (TnBuffFloatModifier DamageTaken): the
/// Whirlwind's Energon Absorbers while he spins (0.85), the Hover's
/// TnBuffDecreaseDamageTakenWhileHovering while he hovers (0.5).
pub fn taken_scale(player: &Player) -> f32 {
    let a = &player.ability;
    let mut k = 1.0;
    if a.kind == Kind::Whirlwind && a.active && a.has(ENERGON_ABSORBERS) {
        k *= crate::whirlwind::ABSORBERS_TAKEN;
    }
    if a.kind == Kind::Hover && a.hover.phase == crate::hoverslam::Phase::Hovering {
        k *= crate::hoverslam::HOVER_TAKEN;
    }
    k
}

/// His ground speed's multiplier from his ability: the Flak Shield raised
/// (BlockGroundSpeedMultiplier, not while sprinting), the Whirlwind's Flurry of
/// Blows.
pub fn speed_scale(player: &Player) -> f32 {
    let a = &player.ability;
    match a.kind {
        Kind::Shield if a.shield.up && !player.sprinting => crate::flakshield::speed(a),
        Kind::Whirlwind if a.active && a.has(FLURRY_OF_BLOWS) => crate::whirlwind::FLURRY_SPEED,
        _ => 1.0,
    }
}

/// He attacks (a shot, a blow, a grenade): his cloak gives him away
/// (TnPawn.ExposeSelf, after Shadow Stalker's delay) and his shield goes down
/// (AutoStopShieldWhenAttacking).
pub fn attacked(player: &mut Player) {
    let a = &mut player.ability;
    match a.kind {
        Kind::Cloak if a.active => crate::cloak::expose(a, false),
        Kind::Shield if a.shield.up => a.end(End::Forced),
        _ => {}
    }
}

/// TnBuffAbilityJammed's BuffTime.
pub const JAM_SECS: f32 = 10.0;

/// TnBuffAbilityJammed put on him (a Guard's scanner caught him, or its blast's
/// TnDamageTypeGuardKnockback struck him: ai/guard.rs), or put on afresh: for its
/// BuffTime his ability can't be used (TnPawn.get_CanUseAbilities) nor refill
/// (Apply's SetCooldownMultiplier(0)), and the HUD shows it jammed; going on, the
/// cloak comes off him (BuffsToRemoveWhenJammed: TnBuffCloak) and the ability
/// going is ended (ForceEndAbilityByClass). The EMP grenade's likewise (emp.rs).
pub fn jam(player: &mut Player) {
    player.jammed_left = JAM_SECS;
    player.ability.end(End::Forced);
}

/// TnBuffAbilityJammed on him.
pub fn jammed(player: &Player) -> bool {
    player.jammed_left > 0.0
}

/// TnBuffAbilityJammed's time running out on every Jazz (his copies too: the AI
/// here jams them, and takes them for weakened meanwhile).
fn count_jams(time: Res<Time>, mut players: Query<&mut Player>) {
    let dt = time.delta_secs();
    for mut player in &mut players {
        if player.jammed_left > 0.0 {
            player.jammed_left = (player.jammed_left - dt).max(0.0);
        }
    }
}

/// A blow to him (main.rs's damage_player): `amount` as his ability takes it (what
/// gets past his shield, scaled by his buffs), his cloak giving him away. Returns
/// the damage left for his shield and health.
pub fn hurt(
    player: &mut Player,
    amount: f32,
    hit: &crate::enemies::Hit,
    feet: Option<Vec3>,
) -> f32 {
    let amount = amount * taken_scale(player);
    crate::cloak::expose(&mut player.ability, true);
    if player.ability.kind == Kind::Shield && player.ability.shield.up {
        return crate::flakshield::block(player, amount, hit, feet);
    }
    amount
}

// ---------------------------------------------------------------------------
// The robot's sockets and the abilities' models
// ---------------------------------------------------------------------------

/// The robot's sockets (robot_extras.txt's socket lines: tools/build_robot_extras.py,
/// RB_Iconic_SKEL's): each one's bone and place on it (umodel's glTF axes, metres).
#[derive(Resource, Default)]
pub(crate) struct RobotSockets(HashMap<String, (String, Transform)>);

impl RobotSockets {
    fn load() -> RobotSockets {
        let path = crate::assets_dir().join("models/robot_extras.txt");
        let text = std::fs::read_to_string(path).unwrap_or_default();
        RobotSockets(read_sockets(&text, 0))
    }
}

/// A file's `socket` lines: name, bone, translation, rotation (x y z w) and scale,
/// tab-separated after the word and `skip` fields of the file's own (abilities.txt's
/// model name: 1).
fn read_sockets(text: &str, skip: usize) -> HashMap<String, (String, Transform)> {
    let floats = |s: &str| -> Vec<f32> {
        s.split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect()
    };
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            if f.first() != Some(&"socket") || f.len() < 6 + skip {
                return None;
            }
            let f = &f[skip..];
            let (t, q, sc) = (floats(f[3]), floats(f[4]), floats(f[5]));
            (t.len() == 3 && q.len() == 4 && sc.len() == 3).then(|| {
                (
                    f[1].to_string(),
                    (
                        f[2].to_string(),
                        Transform {
                            translation: Vec3::new(t[0], t[1], t[2]),
                            rotation: Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize(),
                            scale: Vec3::new(sc[0], sc[1], sc[2]),
                        },
                    ),
                )
            })
        })
        .collect()
}

/// Each Jazz's robot's bones by name, as they are found.
#[derive(Resource, Default)]
pub(crate) struct JazzBones(HashMap<(Entity, String), Entity>);

/// A Jazz's robot's bones and sockets in the world.
#[derive(SystemParam)]
pub(crate) struct Sockets<'w, 's> {
    table: Res<'w, RobotSockets>,
    bones: ResMut<'w, JazzBones>,
    models: Query<'w, 's, (Entity, &'static JazzOf), With<RobotForm>>,
    children: Query<'w, 's, &'static Children>,
    names: Query<'w, 's, &'static Name>,
    globals: Query<'w, 's, &'static GlobalTransform>,
}

impl Sockets<'_, '_> {
    /// Bone `bone` of `jazz`'s robot.
    pub fn bone(&mut self, jazz: Entity, bone: &str) -> Option<Entity> {
        let key = (jazz, bone.to_string());
        if let Some(&e) = self.bones.0.get(&key)
            && self.globals.contains(e)
        {
            return Some(e);
        }
        let found = self
            .models
            .iter()
            .filter(|(_, of)| of.0 == jazz)
            .flat_map(|(m, _)| self.children.iter_descendants(m))
            .find(|e| self.names.get(*e).is_ok_and(|n| n.as_str() == bone))?;
        self.bones.0.insert(key, found);
        Some(found)
    }

    /// Socket `socket` of `jazz`'s robot: its bone and its place on it.
    pub fn socket(&mut self, jazz: Entity, socket: &str) -> Option<(Entity, Transform)> {
        let (bone, local) = self.table.0.get(socket).cloned()?;
        Some((self.bone(jazz, &bone)?, local))
    }

    /// Where socket `socket` of `jazz`'s robot is, and its X and Y (Unreal's, the
    /// way an effect on it faces), in the world.
    pub fn frame(&mut self, jazz: Entity, socket: &str) -> Option<(Vec3, Vec3, Vec3)> {
        let (bone, local) = self.socket(jazz, socket)?;
        let g = self.globals.get(bone).ok()?;
        let at = g.transform_point(local.translation);
        // (umodel's glTF axes are Unreal's X, Z and Y.)
        let x = g
            .affine()
            .transform_vector3(local.rotation * Vec3::X)
            .normalize_or(Vec3::X);
        let y = g
            .affine()
            .transform_vector3(local.rotation * Vec3::Z)
            .normalize_or(Vec3::Z);
        Some((at, x, y))
    }

    /// The same with its Z (Unreal's: the way the socket's up points): its place,
    /// its X, Y and Z, and how big the robot is drawn there (his class's size: a
    /// metre of the model's is that many in the world), in the world.
    pub fn frame3(&mut self, jazz: Entity, socket: &str) -> Option<(Vec3, [Vec3; 3], f32)> {
        let (bone, local) = self.socket(jazz, socket)?;
        let g = self.globals.get(bone).ok()?;
        let at = g.transform_point(local.translation);
        let axis = |v: Vec3, or: Vec3| {
            g.affine()
                .transform_vector3(local.rotation * v)
                .normalize_or(or)
        };
        let size = g
            .affine()
            .transform_vector3(local.rotation * Vec3::X)
            .length();
        // (umodel's glTF axes are Unreal's X, Z and Y.)
        Some((
            at,
            [
                axis(Vec3::X, Vec3::X),
                axis(Vec3::Z, Vec3::Z),
                axis(Vec3::Y, Vec3::Y),
            ],
            size,
        ))
    }

    /// Where `entity` is in the world.
    pub fn global(&self, entity: Entity) -> Option<GlobalTransform> {
        self.globals.get(entity).ok().copied()
    }
}

/// The abilities' models (tools/build_abilities.py's models/abilities/): the Flak
/// Shield and its clips (equip, drop), the Battle Sentry and the Rust Sentry and
/// theirs (idle, activate, deactivate). (Their scenes from the files their clips
/// come from: gltffiles.rs.)
#[derive(Resource)]
pub(crate) struct AbilityModels {
    pub shield: Option<crate::gltffiles::SceneOf>,
    pub shield_graph: Handle<AnimationGraph>,
    pub shield_clips: [AnimationNodeIndex; 2],
    pub sentry: [Option<crate::gltffiles::SceneOf>; 2],
    pub sentry_graph: [Handle<AnimationGraph>; 2],
    pub sentry_clips: [[AnimationNodeIndex; 3]; 2],
}

const SHIELD_MODEL: &str = "models/abilities/flak_shield.glb";
const SENTRY_MODELS: [&str; 2] = [
    "models/abilities/sentry.glb",
    "models/abilities/sentry_slow.glb",
];

fn load_ability_models(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut clips: ResMut<GltfClips>,
) {
    let mut graph_of = |file: &'static str, n: usize| {
        let mut graph = AnimationGraph::new();
        let mut pending = Vec::new();
        let root = graph.root;
        let nodes: Vec<AnimationNodeIndex> = (0..n)
            .map(|i| crate::gltf_clip(&mut graph, &mut pending, i, root))
            .collect();
        let handle = graphs.add(graph);
        let exists = crate::asset_exists(file);
        let scene = exists.then(|| crate::gltffiles::SceneOf::load(&asset_server, file));
        if let Some(scene) = &scene {
            clips.wait(scene.file.clone(), handle.clone(), pending);
        }
        (scene, handle, nodes)
    };
    let (shield, shield_graph, s) = graph_of(SHIELD_MODEL, 2);
    let (sentry0, graph0, c0) = graph_of(SENTRY_MODELS[0], 3);
    let (sentry1, graph1, c1) = graph_of(SENTRY_MODELS[1], 3);
    if shield.is_none() || sentry0.is_none() {
        info!(
            "models/abilities not built: the Flak Shield and the Battle Sentry show no models (tools/build_abilities.py)"
        );
    }
    commands.insert_resource(AbilityModels {
        shield,
        shield_graph,
        shield_clips: [s[0], s[1]],
        sentry: [sentry0, sentry1],
        sentry_graph: [graph0, graph1],
        sentry_clips: [[c0[0], c0[1], c0[2]], [c1[0], c1[1], c1[2]]],
    });
}

// ---------------------------------------------------------------------------
// The plugin
// ---------------------------------------------------------------------------

/// The other players' Ability events (net_apply's), for the abilities' systems:
/// whose (his Jazz here) and what.
#[derive(Resource, Default)]
pub(crate) struct AbilityInbox(pub Vec<(Entity, JazzEvent)>);

impl AbilityInbox {
    /// Those of `what` (taken out).
    pub fn take(&mut self, what: u8) -> Vec<(Entity, Vec3, Vec3, f32)> {
        let mut out = Vec::new();
        self.0.retain(|(e, ev)| match ev {
            JazzEvent::Ability {
                what: w,
                at,
                dir,
                value,
            } if *w == what => {
                out.push((*e, *at, *dir, *value));
                false
            }
            _ => true,
        });
        out
    }
}

/// The abilities' systems run in this set (after the support items' keys, before
/// he moves).
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct AbilitySystems;

pub struct AbilitiesPlugin;

impl Plugin for AbilitiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AbilityInbox>()
            .init_resource::<JazzBones>()
            .insert_resource(RobotSockets::load())
            .add_systems(Startup, load_ability_models)
            .add_plugins((crate::cloak::CloakPlugin, crate::battlesentry::SentryPlugin))
            .add_systems(OnEnter(crate::menu::AppState::Game), clear_abilities)
            .configure_sets(
                Update,
                AbilitySystems
                    .after(crate::weapon_pickups)
                    .before(crate::player_movement)
                    .run_if(in_state(crate::menu::AppState::Game)),
            )
            .add_systems(
                Update,
                (
                    equip_abilities,
                    count_jams,
                    use_abilities,
                    run_barrier,
                    crate::cloak::run_cloak,
                    crate::whirlwind::run_whirlwind,
                    crate::healbeam::run_heal_beam,
                    crate::flakshield::run_shield,
                    crate::hoverslam::run_hover,
                    crate::battlesentry::run_sentry_ability,
                    ability_buffs,
                    finish_abilities,
                )
                    .chain()
                    .in_set(AbilitySystems),
            )
            .add_systems(
                Update,
                // (After net_capture: the HUD's sound is this PC's own. After emp.rs's,
                // which feeds the EMP grenade's.)
                feed_ability_hud
                    .after(crate::net_capture)
                    .after(crate::emp::feed_ability_hud)
                    .before(crate::sound::PlaySounds)
                    .before(crate::hud::HudSystems)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

/// Gives this PC's Jazz his chosen character's class ability as he takes up its
/// loadout (match start, respawn: Player::rearmed, which emp.rs takes after this),
/// its meter full; what the old one had going ends.
fn equip_abilities(
    chosen: crate::split::BySeatRef<choose::Chosen>,
    mut players: Query<(&mut Player, Option<&crate::split::Seat>), With<LocalPlayer>>,
) {
    for (mut player, seat) in &mut players {
        if !player.rearmed {
            continue;
        }
        // (Split screen's guest's from his own.)
        let chosen = chosen.seat(crate::split::seat_of(seat));
        let kind = Kind::of_id(&chosen.ability);
        let upgrade = upgrade_of(kind, &chosen.ability_upgrade);
        let a = &mut player.ability;
        if a.active {
            a.end(End::Forced);
        }
        let ended = a.ended;
        let old = std::mem::take(a);
        let mut new = ClassAbility::new(kind, upgrade);
        // (What the old one had out goes with it: its systems see it end this frame.)
        new.ended = ended;
        new.barrier = old.barrier;
        new.sentry = old.sentry;
        new.cloak = old.cloak;
        new.shield = old.shield;
        new.heal = old.heal;
        new.whirl = old.whirl;
        new.hover = old.hover;
        player.ability = new;
    }
}

/// The Ability key and the meter, for this PC's Jazz (the other players' PCs run
/// theirs): starting, ending, draining and refilling (TnAbilityManager.TriggerAbility
/// and Tick, TnAbility's resource).
fn use_abilities(
    time: Res<Time>,
    mut players: Query<(&mut Player, &PlayerInput), With<LocalPlayer>>,
) {
    let dt = time.delta_secs();
    for (mut player, input) in &mut players {
        let player = &mut *player;
        let kind = player.ability.kind;
        if kind == Kind::None {
            continue;
        }
        let down = player.dead_left > 0.0 || player.dying;
        let downed = player.revive.locked();
        let robot = player.form == Form::Robot
            && player.transform_left <= 0.0
            && player.pending_form.is_none();
        let cooldown = player.support.cooldown_scale();
        let sprinting = player.sprinting || (player.dodge && player.dash_left > 0.0);
        let grounded = player.grounded;
        let boosting = player.form == Form::Vehicle && player.sprinting;
        let jammed = player.jammed_left > 0.0;
        let reacting = player.melee_react.reacting();
        let a = &mut player.ability;
        a.guard = (a.guard - dt).max(0.0);
        if a.active {
            a.clock += dt;
        }

        // Dead or down: what is going ends (dead, his meter comes back full with
        // him).
        if down || downed {
            a.end(End::Forced);
            if down {
                a.resource = kind.meter().max;
            }
            a.button = input.ability;
            continue;
        }
        // The robot's own end as he changes form.
        if a.active && kind.robot_only() && !robot {
            a.end(End::Forced);
        }

        // The key (TriggerAbility): in robot form only, and not while he is jammed
        // (get_CanUseAbilities) or reacting to a melee blow (his ability turned
        // off: IsAbilityEnabled, hitreact.rs).
        let pressed = input.ability && !a.button && input.control;
        a.button = input.ability;
        if pressed && robot && a.guard <= 0.0 && !jammed && !reacting {
            if a.active && kind.cancellable() {
                // (The Hover only once it hovers: a press as it rises does nothing.)
                if kind != Kind::Hover || a.hover.phase == crate::hoverslam::Phase::Hovering {
                    a.end(End::Cancelled);
                }
            } else if kind == Kind::HealBeam {
                // (healbeam.rs picks the target, or switches, or ends it.)
                a.pressed = true;
                a.guard = kind.spam_secs();
            } else if !a.active && a.resource >= kind.meter().required && can_start(a, grounded) {
                a.resource -= kind.meter().initial;
                a.active = true;
                a.started = true;
                a.clock = 0.0;
                a.guard = kind.spam_secs();
            }
        }

        // The meter: draining while it is going (its own rate, with its upgrades and
        // the Intercooler's), refilling while it isn't.
        let meter = kind.meter();
        if a.active {
            let drain = drain_rate(a, sprinting, boosting) / cooldown;
            a.resource -= drain * dt;
            if a.resource <= 0.0 && drain > 0.0 {
                a.resource = 0.0;
                a.end(End::RanOut);
            }
        } else if !jammed {
            // (Jammed, it doesn't refill: SetCooldownMultiplier(0).)
            let refill = meter.refill * refill_scale(a) * cooldown;
            a.resource = (a.resource + refill * dt).min(meter.max);
        }
    }
}

/// Whether it may start now: the Cloak not while TnBuffDecloak is on him, the Flak
/// Shield not while it is knocked out, the Whirlwind on the ground only, the Hover not
/// while he dives into a slam.
fn can_start(a: &ClassAbility, grounded: bool) -> bool {
    match a.kind {
        Kind::Cloak => crate::cloak::can_start(a),
        Kind::Shield => crate::flakshield::can_start(a),
        Kind::Whirlwind => grounded,
        Kind::Hover => a.hover.phase == crate::hoverslam::Phase::None,
        _ => true,
    }
}

/// The meter's drain a second while it is going, with its upgrades.
fn drain_rate(a: &ClassAbility, sprinting: bool, boosting: bool) -> f32 {
    let meter = a.kind.meter();
    match a.kind {
        // (Faster while he sprints or dodges, or boosts the vehicle:
        // CloakResourceDatas[1]'s SprintDrainRate and BoostDrainRate; Specter halves it,
        // TnBuffCloakResourceDrainRate.)
        Kind::Cloak => {
            let rate = if sprinting || boosting {
                crate::cloak::SPRINT_DRAIN
            } else {
                meter.drain
            };
            rate * if a.has(SPECTER) { 0.5 } else { 1.0 }
        }
        Kind::Barrier => meter.drain * if a.has(FORTIFY) { 0.5 } else { 1.0 },
        Kind::Hover => crate::hoverslam::drain(a),
        Kind::HealBeam => meter.drain * if a.has(EMPOWERED_BEAM) { 0.5 } else { 1.0 },
        Kind::Sentry => {
            if a.has(RESOURCE_RECYCLER) {
                0.0
            } else {
                meter.drain
            }
        }
        _ => meter.drain,
    }
}

/// The meter's refill's multiplier from its upgrade (Cloak Intercooler's
/// TnBuffCloakResourceRefillRate).
fn refill_scale(a: &ClassAbility) -> f32 {
    if a.kind == Kind::Cloak && a.has(CLOAK_INTERCOOLER) {
        2.0
    } else {
        1.0
    }
}

/// The Diffraction Barrier as the Leader's ability (equipment.rs's barrier): raised
/// as it starts (10 m ahead, its trigger clip and its deploy sound), let down as it
/// ends (its retract), and its end as the barrier goes by itself. One barrier each:
/// the support item's goes as this one goes up (and the other way about,
/// equipment.rs).
#[allow(clippy::too_many_arguments)]
fn run_barrier(
    mut commands: Commands,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut players: Query<(
        Entity,
        &Transform,
        &mut Player,
        &PlayerInput,
        Has<RemotePlayer>,
    )>,
    mut barriers: Query<&mut crate::equipment::Barrier>,
) {
    for (entity, transform, mut player, input, remote) in &mut players {
        let player = &mut *player;
        if player.ability.kind != Kind::Barrier || remote {
            continue;
        }
        // (Gone by itself: worn away, or with him.)
        if player
            .ability
            .barrier
            .is_some_and(|b| barriers.get(b).is_err())
        {
            player.ability.barrier = None;
            player.ability.end(End::Forced);
        }
        if player.ability.started {
            crate::equipment::lower_support_barrier(player, &mut barriers);
            let (barrier, at, facing) = crate::equipment::raise_barrier(
                &mut commands,
                &mut fx,
                &mut sfx,
                entity,
                player,
                transform.translation,
                input,
            );
            player.ability.barrier = Some(barrier);
            player.support.trigger_anim = Some(0.0);
            player.played.push(JazzEvent::Gear {
                item: crate::shop::Equipment::Barrier.index(),
                at,
                facing,
            });
        }
        if player.ability.ended.is_some()
            && let Some(b) = player.ability.barrier.take()
        {
            crate::equipment::lower_barrier(&mut barriers, b);
            player.played.push(JazzEvent::GearEnd {
                item: crate::shop::Equipment::Barrier.index(),
            });
        }
    }
}

/// Each Jazz's damage multiplier for the blows he deals (`DEALT`), from his ability
/// as it is this frame (his copy's from his snapshot).
fn ability_buffs(players: Query<(Entity, &Player)>) {
    let mut out = Vec::new();
    for (e, player) in &players {
        let a = &player.ability;
        let mut k = 1.0;
        if a.kind == Kind::Hover && a.hover.phase == crate::hoverslam::Phase::Hovering {
            k *= crate::hoverslam::HOVER_DEALT;
        }
        if a.heal.medic_left > 0.0 {
            k *= crate::healbeam::MEDIC_DEALT;
        }
        if k != 1.0 {
            out.push((e, k));
        }
    }
    if let Ok(mut d) = DEALT.write() {
        *d = out;
    }
}

/// The frame's starts, ends and presses have been acted on.
fn finish_abilities(time: Res<Time>, mut players: Query<&mut Player>) {
    let dt = time.delta_secs();
    for mut player in &mut players {
        let a = &mut player.ability;
        a.started = false;
        a.ended = None;
        a.pressed = false;
        a.since_end = if a.active { 0.0 } else { a.since_end + dt };
    }
}

/// A new match: nobody's ability going, nothing of the last match's left about.
fn clear_abilities(mut players: Query<&mut Player>, mut inbox: ResMut<AbilityInbox>) {
    inbox.0.clear();
    for mut player in &mut players {
        let a = &mut player.ability;
        a.end(End::Forced);
    }
}

/// The HUD's ability (robotAbility_mc) for this PC's Jazz's class ability, and its
/// ready sound as it comes ready (the EMP grenade's is emp.rs's).
#[allow(clippy::type_complexity)]
fn feed_ability_hud(
    mut huds: crate::split::BySeat<HudState>,
    controls: Res<Controls>,
    mut sfx: ResMut<Sfx>,
    mut players: Query<
        (
            &mut Player,
            Has<crate::emp::EmpAbility>,
            Option<&crate::split::Seat>,
        ),
        With<LocalPlayer>,
    >,
) {
    // (Each of this PC's players' HUD: split screen's guest's his own.)
    for (player, emp, seat) in &mut players {
        let hud = huds.seat(crate::split::seat_of(seat));
        feed_one(hud, &controls, &mut sfx, player, emp);
    }
}

/// One of this PC's players' class ability on his HUD (feed_ability_hud).
fn feed_one(
    hud: &mut HudState,
    controls: &Controls,
    sfx: &mut Sfx,
    mut player: Mut<Player>,
    emp: bool,
) {
    if emp {
        return;
    }
    let alive = player.dead_left <= 0.0;
    let jammed = player.jammed_left > 0.0;
    let a = &mut player.ability;
    if a.kind == Kind::None {
        if hud.ability.is_some() {
            hud.ability = None;
        }
        return;
    }
    let meter = a.kind.meter();
    // (In use: its meter draining, TnAbility's cooldown running backwards.)
    let in_use = a.active;
    let ready = a.resource >= meter.required && !in_use && !jammed;
    if ready && !a.ready && alive {
        sfx.play(crate::emp::SND_READY);
    }
    a.ready = ready;
    let shown = HudAbility {
        icon: a.kind.icon(),
        meter: a.resource / meter.max,
        cost: meter.required / meter.max,
        in_use,
        key: controls.short_key_name(Mode::Robot, Action::Ability),
        cancel: a.kind.cancel_icon() && a.active,
        active: a.active,
        juice: a.heal.juice,
        jammed,
    };
    if hud.ability.as_ref() != Some(&shown) {
        hud.ability = Some(shown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_upgrades() {
        assert_eq!(Kind::of_id("Cloaking"), Kind::Cloak);
        assert_eq!(Kind::of_id("FlashBangAbility"), Kind::None);
        for k in IDS.map(|(_, k)| k) {
            assert_eq!(Kind::from_index(k.index()), k);
            assert_eq!(UPGRADES.iter().filter(|(_, u, _)| *u == k).count(), 3);
        }
        assert_eq!(Kind::from_index(0), Kind::None);
    }

    #[test]
    fn meters_last_as_the_game_says() {
        // (Cloak: 12 s walking, 4 s sprinting from full; Whirlwind 4.76 s; Hover 5 s;
        // the Sentry's 50 s untouched; the Shield's 18 s held.)
        let walk = |k: Kind| {
            let m = k.meter();
            (m.max - m.initial) / m.drain
        };
        assert!((walk(Kind::Cloak) - 12.0).abs() < 1e-4);
        assert!((walk(Kind::Whirlwind) - 4.7619).abs() < 1e-3);
        assert!((walk(Kind::Hover) - 5.0).abs() < 1e-4);
        assert!((walk(Kind::Sentry) - 50.0).abs() < 1e-4);
        assert!((walk(Kind::Shield) - 18.0).abs() < 1e-4);
        let a = ClassAbility::new(Kind::Cloak, SPECTER);
        assert!((drain_rate(&a, true, false) - 1.5).abs() < 1e-5);
    }

    #[test]
    fn sockets_read() {
        let robot = "socket\tShieldSocket_L\tL_Arm03_Wrist_XB\t0 0.1 0\t0 0 0 1\t1 1 1\n";
        let read = read_sockets(robot, 0);
        let (bone, at) = &read["ShieldSocket_L"];
        assert_eq!(bone, "L_Arm03_Wrist_XB");
        assert_eq!(at.translation, Vec3::new(0.0, 0.1, 0.0));
        let model = "socket\tsentry\tMuzzleFlash\tC_Robo06_XT\t0 0 0.5\t0 0 0 1\t1 1 1\n";
        assert_eq!(read_sockets(model, 1)["MuzzleFlash"].0, "C_Robo06_XT");
        assert!(read_sockets(model, 2).is_empty());
    }
}
