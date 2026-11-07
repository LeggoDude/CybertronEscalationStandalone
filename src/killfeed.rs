//! The kill feed: who killed whom and with what, a line at a time in the HUD's game
//! messages at the top left, as the game's death messages show them.
//!
//! The game broadcasts a death message for every death of a player or an enemy
//! (GameInfo.Killed's BroadcastDeathMessage, TnDeathMessage): its killer's
//! name, its damage type's icon and its victim's name ("Sentry [Sentry] Soldier"),
//! or, for a death with no killer (or one's own), its damage type's suicide message
//! ("[Suicide]Soldier"); a headshot's is TnDeathMessage's own HeadShotString, the
//! headshot's icon in the weapon's place ("Default [Headshot] Soldier": TnGame.
//! GetDeathMessageSwitch's switch 2, when the victim's last blow struck its head,
//! WasLastHitHeadshot, by a damage type not ExemptFromAccuracyKills). The words come
//! from the damage type's DeathString or Suicide (TransGame.int, inherited down its
//! classes: DAMAGE_TYPES), the names
//! from the players' and the enemies' (a trap's sentries' "Sentry"), and each
//! name its colour by who is looking (TnMessageHelpers.GetColorForPRI: the player
//! at this PC, his teammates and their sentries, the enemies; the rest of the line
//! in the killer's: ParseMessageIntoColoredString). The HUD shows the icons in place
//! of their tokens (hud.rs's ICONS) and keeps five lines, each for five seconds.
//!
//! Online, each death is noticed at the PC it happens at (an enemy's where the blow
//! that killed it was struck: waves.rs; a player's at his own) and its line goes to
//! the others (JazzEvent::KillFeed), each showing it as its own player sees it. Every
//! PC plays the players' blows on its own copies of the enemies (and its own traps),
//! so an enemy's death is noticed at more than one PC at once: each line names the
//! enemy by its number in the waves, and a PC shows one line for each enemy's death,
//! the first it has (its own, told to the others, or one told to it), as the game's
//! server broadcasts one death message for each death.

use bevy::prelude::*;

use crate::enemies::{Death, Enemy, Foe, Kinds};
use crate::hud::{HudEvent, HudState};
use crate::net::{JazzEvent, Net};
use crate::{LocalPlayer, Player, PlayerSlot};

/// A trap's sentries' name (TransGame.int's [AICharacterNames] Sentry: their
/// controllers' PlayerReplicationInfo's).
pub const SENTRY_NAME: &str = "Sentry";

/// TnMessageTextColors' colours, as the HUD's getColorFromIndex takes them: the
/// player at this PC's, his teammates', the enemies', and nobody's.
pub const PLAYER_COLOR: u8 = 0;
pub const FRIENDLY_COLOR: u8 = 1;
pub const ENEMY_COLOR: u8 = 2;
pub const NEUTRAL_COLOR: u8 = 3;

/// Who a line names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Who {
    /// No one (a trap's fire or blast, a fall, a death by one's own hand).
    #[default]
    Nobody,
    /// A player, by his slot.
    Player(u8),
    /// An enemy, by its type (enemies.rs's Kinds).
    Enemy(u16),
    /// A trap's sentry (Downfall's Defensive Sentries, Ignition's gunship).
    Sentry,
}

impl Who {
    /// Its kind and number, as JazzEvent::KillFeed carries it.
    pub fn code(self) -> (u8, u16) {
        match self {
            Who::Nobody => (0, 0),
            Who::Player(slot) => (1, slot as u16),
            Who::Enemy(kind) => (2, kind),
            Who::Sentry => (3, 0),
        }
    }

    pub fn from_code(kind: u8, id: u16) -> Who {
        match kind {
            1 => Who::Player(id as u8),
            2 => Who::Enemy(id),
            3 => Who::Sentry,
            _ => Who::Nobody,
        }
    }
}

/// A death for the feed: who killed whom, by what damage type (its index in
/// DAMAGE_TYPES), the enemy that died by its number in the waves (waves.rs's
/// WaveFoe; 0 for none: a player's death, or an enemy the waves didn't bring in),
/// and whether it was a headshot (its line HEAD_SHOT_STRING).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KillNote {
    pub killer: Who,
    pub victim: Who,
    pub damage: u8,
    pub foe: u32,
    pub head: bool,
}

impl KillNote {
    pub fn new(killer: Who, victim: Who, damage: &str) -> KillNote {
        KillNote {
            killer,
            victim,
            damage: damage_index(damage),
            foe: 0,
            head: false,
        }
    }

    /// The same, the death of the waves' enemy numbered `id`.
    pub fn of_foe(self, id: u32) -> KillNote {
        KillNote { foe: id, ..self }
    }

    /// The same, a headshot or not.
    pub fn headshot(self, head: bool) -> KillNote {
        KillNote { head, ..self }
    }

    pub fn event(&self) -> JazzEvent {
        let (killer, killer_id) = self.killer.code();
        let (victim, victim_id) = self.victim.code();
        JazzEvent::KillFeed {
            killer,
            killer_id,
            victim,
            victim_id,
            damage: self.damage,
            foe: self.foe,
            head: self.head,
        }
    }

    pub fn of_event(event: &JazzEvent) -> Option<KillNote> {
        match *event {
            JazzEvent::KillFeed {
                killer,
                killer_id,
                victim,
                victim_id,
                damage,
                foe,
                head,
            } => Some(KillNote {
                killer: Who::from_code(killer, killer_id),
                victim: Who::from_code(victim, victim_id),
                damage,
                foe,
                head,
            }),
            _ => None,
        }
    }
}

/// The damage types the demo's deaths come by: their DeathString and Suicide
/// (TransGame.int, each inherited from the nearest of its classes that has one:
/// TnDamageType's own for the rest, and for any not here).
#[rustfmt::skip]
pub const DAMAGE_TYPES: &[(&str, &str, &str)] = &[
    ("TnDamageType", "`k destroyed `o", "[Suicide]`o"),
    ("TnDamageTypeAssaultRifle", "`k [AssRifle] `o", "[Suicide]`o[AssRifle]"),
    ("TnDamageTypeIonBlaster", "`k [IonBlast] `o", "[Suicide]`o[IonBlast]"),
    ("TnDamageTypeBurstRifle", "`k [BrstRifle] `o", "[Suicide]`o[BrstRifle]"),
    ("TnDamageTypeShotgun", "`k [Shotgun] `o", "[Suicide]`o[Shotgun]"),
    ("TnDamageTypeFireShotgun", "`k [FireShotgun] `o", "[Suicide]`o[FireShotgun]"),
    ("TnDamageTypeSniperRifle", "`k [SnprRifle] `o", "[Suicide]`o[SnprRifle]"),
    ("TnDamageTypeHeavyPistol", "`k [HvyPistol] `o", "[Suicide]`o[HvyPistol]"),
    ("TnDamageTypeBazooka", "`k [Bazooka] `o", "[Suicide]`o[Bazooka]"),
    ("TnDamageTypeTracerDart", "`k [TracerDart] `o", "[Suicide]`o"),
    ("TnDamageTypeHeavyMG", "`k [HvyMG] `o", "[Suicide]`o[HvyMG]"),
    ("TnDamageTypeDrainGun", "`k [DrainGun] `o", "[Suicide]`o[DrainGun]"),
    ("TnDamageTypeChainBeam", "`k [ChainBeam] `o", "[Suicide]`o[ChainBeam]"),
    ("TnDamageTypeSlow", "`k [Slow] `o", "[Suicide]`o[Slow]"),
    ("TnDamageTypeSawBlade", "`k [SawBlade] `o", "[Suicide]`o[SawBlade]"),
    ("TnDamageTypeHomingRocket", "`k [HmgRocket] `o", "[Suicide]`o[HmgRocket]"),
    ("TnDamageTypeGrenadeLauncher", "`k [GrenLaunch] `o", "[Suicide]`o[GrenLaunch]"),
    ("TnDamageTypeClusterBomb", "`k [ClusterBomb] `o", "[Suicide]`o[ClusterBomb]"),
    ("TnDamageTypeCarMachineGun", "`k [Car] `o", "[Suicide]`o[Car]"),
    ("TnDamageTypeTruckRocket", "`k [Truck] `o", "[Suicide]`o[Truck]"),
    ("TnDamageTypeTankShell", "`k [Tank] `o", "[Suicide]`o[Tank]"),
    ("TnDamageTypePlaneRocket", "`k [Jet] `o", "[Suicide]`o[Jet]"),
    ("TnDamageTypeMelee", "`k [Melee] `o", "[Suicide]`o[Melee]"),
    ("TnDamageTypeWhirlwind", "`k [Melee] `o", "[Suicide]`o[Melee]"),
    ("TnDamageTypeLeapSmash", "`k [Melee] `o", "[Suicide]`o[Melee]"),
    ("TnDamageTypeTitanMelee", "`k [Melee] `o", "[Suicide]`o[Melee]"),
    ("TnDamageTypeShoulderSlam", "`k [Melee] `o", "[Suicide]`o[Melee]"),
    ("TnDamageTypeBreakShield", "`k [Melee] `o", "[Suicide]`o[Melee]"),
    ("TnDamageTypeHoverGroundPound", "`k [GroundPound] `o", "[Suicide]`o[GroundPound]"),
    ("TnDamageTypeHoverGroundPoundAi", "`k [GroundPound] `o", "[Suicide]`o[GroundPound]"),
    ("TnDamageTypeFireDot", "`k [FireDot] `o", "[Suicide]`o[FireDot]"),
    ("TnDamageTypeKamikazeMine", "`k [KamiMine] `o", "[Suicide]`o[KamiMine]"),
    ("TnDamageTypeMartyrGrenade", "`k [KamiMine] `o", "[Suicide]`o[KamiMine]"),
    ("TnDamageTypeGravityBomb", "`k [GravBomb] `o", "[Suicide] `o [GravBomb]"),
    ("TnDamageTypeFlakGrenade", "`k [FragGren] `o", "[Suicide]`o[FragGren]"),
    ("TnDamageTypeShockwave", "`k [Shockwave] `o", "[Suicide]`o[Shockwave]"),
    ("TnDamageTypeSentryGun", "`k [Sentry] `o", "[Suicide]`o[Sentry]"),
    ("TnDamageTypeSentryRocket", "`k [Sentry] `o", "[Suicide]`o[Sentry]"),
    ("TnDamageTypeRocketTemp", "`k [Rocket] `o", "[Suicide]`o[Rocket]"),
    ("TnDamageTypeMangaRocketSoldier", "`k [Rocket] `o", "[Suicide]`o"),
    ("TnDamageTypeHeavyRocketTurret", "`k [HvyTurret] `o", "[Suicide]`o[HvyTurret]"),
    ("TnDamageTypeGatlingGun", "`k [HvyTurret] `o", "[Suicide]`o[HvyTurret]"),
    ("TnDamageTypeHeavyIonBlaster", "`k [HvyTurret] `o", "[Suicide]`o[HvyTurret]"),
    ("TnDamageTypeEmpShotgun", "`k [EMPShotgun] `o", "[Suicide]`o[EMPShotgun]"),
    ("TnDamageTypeRammed", "`k [Ram] `o", "[Suicide]`o[Ram]"),
    ("TnDamageTypeDrain", "`k [Drain] `o", "[Suicide]`o[Drain]"),
    ("TnDamageTypeCorrosion", "`k [Corrosion] `o", "[Suicide]`o"),
    ("TnDamageTypeExplodeOnDeath", "`k [Suicide] `o", "[Suicide]`o"),
    ("TnDamageTypeDownedDrain", "[Suicide]`o", "[Suicide]`o"),
    ("DmgType_Suicided", "[Suicide]`o", "[Suicide]`o"),
    ("TnDamageTypeEnvironment", "`k destroyed `o", "[Suicide] `o"),
    // (Oblivion's exploding barrel's blast: barrels.rs. TnDamageType's words.)
    ("TnDamageTypeBruticusFire", "`k destroyed `o", "[Suicide]`o"),
];

/// TnDeathMessage's HeadShotString (TransGame.int): a headshot's line, whatever its
/// damage type.
pub const HEAD_SHOT_STRING: &str = "`k [Headshot] `o";

/// The damage types whose kills are never accuracy kills (their
/// ExemptFromAccuracyKills, inherited down their classes: TnDamageTypeMelee's and
/// its subclasses', the burns', the Kamikaze Mines' and the trap sentries'): a blow
/// of theirs to the head is no headshot (TnGame.GetDeathMessageSwitch,
/// TnKillAwardRuleHeadshot).
pub fn exempt_from_accuracy_kills(name: &str) -> bool {
    matches!(
        name,
        "TnDamageTypeMelee"
            | "TnDamageTypeWhirlwind"
            | "TnDamageTypeLeapSmash"
            | "TnDamageTypeTitanMelee"
            | "TnDamageTypeShoulderSlam"
            | "TnDamageTypeBreakShield"
            | "TnDamageTypeSawbladeMelee"
            | "TnDamageTypeFireDot"
            | "TnDamageTypeKamikazeMine"
            | "TnDamageTypeSentryGun"
            | "TnDamageTypeSentryRocket"
    )
}

/// A damage type's index in DAMAGE_TYPES (TnDamageType's, 0, for one not there).
pub fn damage_index(name: &str) -> u8 {
    DAMAGE_TYPES
        .iter()
        .position(|(n, _, _)| *n == name)
        .unwrap_or(0) as u8
}

/// A damage type as DAMAGE_TYPES has it (TnDamageType for one not there, whose
/// words are its too: the AI's Thrown, VehicleExplosive, GuardKnockback...).
pub fn damage_name(name: &str) -> &'static str {
    damage_of_index(damage_index(name))
}

/// The damage type at an index in DAMAGE_TYPES (TnDamageType past its end).
pub fn damage_of_index(index: u8) -> &'static str {
    DAMAGE_TYPES
        .get(index as usize)
        .unwrap_or(&DAMAGE_TYPES[0])
        .0
}

/// The damage types the trap sentries' rounds are (theirs the sentries' kills).
pub fn sentry_damage(name: &str) -> bool {
    matches!(name, "TnDamageTypeSentryGun" | "TnDamageTypeSentryRocket")
}

/// The damage type of a Jazz's weapon in hand (or his vehicle's): its TnWeapon's
/// projectile's or instant hit's.
pub fn weapon_damage(id: &str) -> &'static str {
    match id {
        "AssaultRifle" => "TnDamageTypeAssaultRifle",
        "IonBlaster" => "TnDamageTypeIonBlaster",
        "BurstRifle" => "TnDamageTypeBurstRifle",
        "Shotgun" => "TnDamageTypeShotgun",
        "SniperRifle" => "TnDamageTypeSniperRifle",
        "HeavyPistol" => "TnDamageTypeHeavyPistol",
        "Bazooka" => "TnDamageTypeBazooka",
        "TracerDart" => "TnDamageTypeTracerDart",
        "HeavyMG" => "TnDamageTypeHeavyMG",
        "DrainGun" => "TnDamageTypeDrainGun",
        "ChainBeam" => "TnDamageTypeChainBeam",
        "AoESlow" => "TnDamageTypeSlow",
        "SawBlade" => "TnDamageTypeSawBlade",
        "HomingRocket" => "TnDamageTypeHomingRocket",
        "GrenadeLauncher" => "TnDamageTypeGrenadeLauncher",
        "ClusterCannon" => "TnDamageTypeClusterBomb",
        "Car" => "TnDamageTypeCarMachineGun",
        "Truck" => "TnDamageTypeTruckRocket",
        "Tank" => "TnDamageTypeTankShell",
        "Jet" => "TnDamageTypePlaneRocket",
        _ => "TnDamageType",
    }
}

/// The damage type a death of `death` came by where its blow named none: the
/// deaths that only one damage type gives (a sword's, a burn's, the Energon
/// Harvester's...), or else the killer's weapon's (`weapon`, a gun or a vehicle's
/// round), or TnDamageType's.
pub fn death_damage(death: Death, burn: bool, weapon: Option<&'static str>) -> &'static str {
    if burn {
        return "TnDamageTypeFireDot";
    }
    match death {
        Death::Melee => "TnDamageTypeMelee",
        Death::Fire => "TnDamageTypeFireDot",
        Death::Drain => "TnDamageTypeDrainGun",
        Death::Dismember => "TnDamageTypeSawBlade",
        Death::Energy => "TnDamageTypeChainBeam",
        Death::Slow => "TnDamageTypeSlow",
        Death::Tank => "TnDamageTypeTankShell",
        // (The Path Blaster's Lights Out: TnDamageTypeShockwaveGun, TnDamageType's
        // words.)
        Death::Shockwave => "TnDamageType",
        Death::SelfDestruct => "TnDamageTypeExplodeOnDeath",
        Death::Gunfire | Death::SingleShot | Death::Explosive | Death::Plain => {
            weapon.unwrap_or("TnDamageType")
        }
    }
}

/// The icons the game's HUD shows in a message in place of these words (sprite
/// 785's GameMessageIconBitmaps: setImageSubstitutions, at 80% of their size).
pub const ICONS: &[(&str, &str)] = &[
    ("[Bazooka]", "GameMessageIconBazooka"),
    ("[HvyPistol]", "GameMessageIconHeavyPistol"),
    ("[GrenLaunch]", "GameMessageIconGrenadeLauncher"),
    ("[HvyMG]", "GameMessageIconHeavyMG"),
    ("[AssRifle]", "GameMessageIconAssaultRifle"),
    ("[IonBlast]", "GameMessageIconIonBlaster"),
    ("[BrstRifle]", "GameMessageIconBurstRifle"),
    ("[HmgRocket]", "GameMessageIconRocketLauncher"),
    ("[Shotgun]", "GameMessageIconShotgun"),
    ("[FireShotgun]", "GameMessageIconShotgun"),
    ("[RepairRay]", "GameMessageIconChainBeam"),
    ("[SnprRifle]", "GameMessageIconSniperRifle"),
    ("[ChainBeam]", "GameMessageIconChainBeam"),
    ("[SawBlade]", "GameMessageIconSawBlade"),
    ("[SawBladeMelee]", "GameMessageIconSawBlade"),
    ("[DrainGun]", "GameMessageIconDrainGun"),
    ("[TracerDart]", "GameMessageIconSysCorruptor"),
    ("[Melee]", "GameMessageIconMelee"),
    ("[Car]", "GameMessageIconCar"),
    ("[Truck]", "GameMessageIconTruck"),
    ("[Jet]", "GameMessageIconJet"),
    ("[Tank]", "GameMessageIconTank"),
    ("[Ram]", "GameMessageIconRam"),
    ("[FragGren]", "GameMessageIconGrenadeLauncher"),
    ("[KamiMine]", "GameMessageIconKamikazeMines"),
    ("[Decoy]", "GameMessageIconDecoyTrap"),
    ("[RollSphere]", "GameMessageIconRollerSphere"),
    ("[TransDisrupt]", "GameMessageIconTransformDisrupt"),
    ("[Shockwave]", "GameMessageIconShockwave"),
    ("[Drain]", "GameMessageIconDrain"),
    ("[Sentry]", "GameMessageIconSentry"),
    ("[Rocket]", "GameMessageIconRocket"),
    ("[HvyTurret]", "GameMessageIconClusterCannon"),
    ("[Suicide]", "GameMessageIconSuicide"),
    ("[Headshot]", "GameMessageIconHeadshot"),
    ("[GroundPound]", "GameMessageIconGroundPound"),
    ("[Slow]", "GameMessageIconAoESlow"),
    ("[Corrosion]", "GameMessageIconAoESlow"),
    ("[ClusterBomb]", "GameMessageIconClusterCannon"),
    ("[FireDot]", "GameMessageIconFireDot"),
    ("[GravBomb]", "GameMessageIconGravBomb"),
    ("[Autobot]", "GameMessageIconAutobot"),
    ("[Decepticon]", "GameMessageIconDecepticon"),
];

/// A piece of a message's text: words, or an icon (its image's name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    Text(&'a str),
    Icon(&'static str),
}

/// A message part's text in pieces: its icons' words out, their images in their
/// place (in the order they come; the first word matching at each place).
pub fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    'outer: while !rest.is_empty() {
        let mut from = 0;
        while let Some(i) = rest[from..].find('[') {
            let at = from + i;
            if let Some((word, image)) = ICONS.iter().find(|(w, _)| rest[at..].starts_with(*w)) {
                if at > 0 {
                    out.push(Piece::Text(&rest[..at]));
                }
                out.push(Piece::Icon(image));
                rest = &rest[at + word.len()..];
                continue 'outer;
            }
            from = at + 1;
        }
        out.push(Piece::Text(rest));
        break;
    }
    out
}

/// The line as the PC whose player has slot `me` shows it, in coloured parts
/// (TnDeathMessage.GetColoredString): a headshot's HEAD_SHOT_STRING, else the damage
/// type's DeathString, or for a death with no killer or by his own hand its Suicide
/// (TnDeathMessage.GetUnparsedMessage by GetDeathMessageSwitch's switch: the
/// headshot first), `k the killer's name and `o the victim's, each in its own
/// colour and the rest in the killer's (ParseMessageIntoColoredString: word by word,
/// each followed by a space).
pub fn line(note: &KillNote, me: Option<u8>, name: impl Fn(Who) -> String) -> Vec<(u8, String)> {
    let (_, death, suicide) = DAMAGE_TYPES
        .get(note.damage as usize)
        .copied()
        .unwrap_or(DAMAGE_TYPES[0]);
    let by_none = note.killer == Who::Nobody || note.killer == note.victim;
    let unparsed = if note.head {
        HEAD_SHOT_STRING
    } else if by_none {
        suicide
    } else {
        death
    };
    let color = |w: Who| match w {
        Who::Nobody => NEUTRAL_COLOR,
        Who::Player(slot) if Some(slot) == me => PLAYER_COLOR,
        Who::Player(_) | Who::Sentry => FRIENDLY_COLOR,
        Who::Enemy(_) => ENEMY_COLOR,
    };
    let killer_color = if by_none {
        NEUTRAL_COLOR
    } else {
        color(note.killer)
    };
    let replacements = [
        ("`k", name(note.killer), killer_color),
        ("`o", name(note.victim), color(note.victim)),
    ];
    let mut parts: Vec<(u8, String)> = Vec::new();
    let mut current = String::new();
    for word in unparsed.split(' ').filter(|w| !w.is_empty()) {
        match replacements.iter().find(|(from, _, _)| word.contains(from)) {
            Some((from, to, c)) => {
                if !current.is_empty() {
                    parts.push((killer_color, std::mem::take(&mut current)));
                }
                parts.push((*c, format!("{} ", word.replace(from, to))));
            }
            None => {
                current.push_str(word);
                current.push(' ');
            }
        }
    }
    if !current.is_empty() {
        parts.push((killer_color, current));
    }
    parts
}

/// A player's death at his PC, for the feed: the enemy whose blow killed him (or
/// nobody: a trap's, a blast's, his own round's, his bleeding out), by its damage
/// type, or his death's.
pub fn player_death(player: &Player, slot: u8) -> KillNote {
    let (killer, damage) = player.last_attack;
    let damage = if damage.is_empty() {
        death_damage(player.last_hit.death, false, None)
    } else {
        damage
    };
    KillNote::new(killer, Who::Player(slot), damage)
}

/// The deaths to show: those noticed here (to tell the others too) and those the
/// others told of; and the waves' enemies whose deaths have been shown this match.
#[derive(Resource, Default)]
pub struct KillFeed {
    pub noticed: Vec<KillNote>,
    pub heard: Vec<KillNote>,
    shown: std::collections::HashSet<u32>,
}

impl KillFeed {
    /// The deaths to show now, those heard of and those noticed here (to tell the
    /// others too), each enemy's death once: a death another PC told of first, or
    /// one shown already, isn't shown again (each PC notices the deaths its copies
    /// of the enemies die, which the players' blows deal on every PC).
    pub fn fresh(&mut self) -> (Vec<KillNote>, Vec<KillNote>) {
        let heard = std::mem::take(&mut self.heard);
        let noticed = std::mem::take(&mut self.noticed);
        let shown = &mut self.shown;
        let mut first = |n: &KillNote| n.foe == 0 || shown.insert(n.foe);
        let heard: Vec<KillNote> = heard.into_iter().filter(|n| first(n)).collect();
        let noticed: Vec<KillNote> = noticed.into_iter().filter(|n| first(n)).collect();
        (heard, noticed)
    }
}

pub struct KillFeedPlugin;

impl Plugin for KillFeedPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KillFeed>()
            .add_observer(enemy_killed)
            // (A match starts with none: no word of the last one's.)
            .add_systems(OnEnter(crate::menu::AppState::Game), forget_kills)
            .add_systems(
                Update,
                show_kills
                    .before(crate::hud::HudSystems)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

fn forget_kills(mut feed: ResMut<KillFeed>) {
    feed.noticed.clear();
    feed.heard.clear();
    feed.shown.clear();
}

/// An enemy that has died here (as it goes: its death not one the others told of,
/// waves.rs), for the feed: its killer the sentry whose round killed it, or the Jazz
/// whose blow did, or nobody, by the damage type its blow named, or its death's, or
/// his weapon's; a headshot when that blow struck its head (Enemy::headshot) by a
/// damage type not exempt from accuracy kills.
fn enemy_killed(
    gone: On<Remove, Enemy>,
    enemies: Query<(&Enemy, &Foe, Option<&crate::waves::WaveFoe>)>,
    players: Query<(&PlayerSlot, &Player)>,
    mut feed: ResMut<KillFeed>,
) {
    let Ok((enemy, foe, wave)) = enemies.get(gone.entity) else {
        return;
    };
    if enemy.health > 0.0 || wave.is_some_and(|w| w.told()) {
        return;
    }
    let hit = enemy.last_hit;
    let jazz = enemy.last_by.and_then(|j| players.get(j).ok());
    // (A sentry's kill is its own, as its controller's: the line names the sentry,
    // not the Jazz it shoots for.)
    let killer = match jazz {
        _ if sentry_damage(hit.damage) => Who::Sentry,
        Some((slot, _)) => Who::Player(slot.0 as u8),
        None => Who::Nobody,
    };
    let damage = if !hit.damage.is_empty() {
        hit.damage
    } else {
        death_damage(
            hit.death,
            enemy.last_blow == crate::xp::Blow::Burn,
            jazz.map(|(_, p)| crate::held_damage(p)),
        )
    };
    let head = enemy.headshot() && !exempt_from_accuracy_kills(damage);
    feed.noticed.push(
        KillNote::new(killer, Who::Enemy(foe.kind as u16), damage)
            .of_foe(wave.map_or(0, |w| w.id))
            .headshot(head),
    );
}

/// The deaths noticed and heard of, into the HUD's game messages as this PC's
/// player sees them (each enemy's once: KillFeed::fresh); those noticed here told
/// to the others too.
fn show_kills(
    mut feed: ResMut<KillFeed>,
    mut huds: crate::split::BySeat<HudState>,
    net: Res<Net>,
    kinds: Res<Kinds>,
    mut me: Query<(&mut Player, &PlayerSlot, Option<&crate::split::Seat>), With<LocalPlayer>>,
) {
    if feed.noticed.is_empty() && feed.heard.is_empty() {
        return;
    }
    let (heard, noticed) = feed.fresh();
    let online = net.in_match();
    // Each of this PC's players' HUD as he sees it (split screen's guest's his own);
    // the others told by the first.
    for (mut player, slot, seat) in &mut me {
        let seat = crate::split::seat_of(seat);
        let my_slot = Some(slot.0 as u8);
        let name = |w: Who| -> String {
            match w {
                Who::Nobody => String::new(),
                Who::Player(slot) => net
                    .players()
                    .find(|(s, _)| *s == slot as usize)
                    .map(|(_, p)| p.name.clone())
                    .or_else(|| (Some(slot) == my_slot).then(|| net.name().to_string()))
                    .unwrap_or_default(),
                Who::Enemy(kind) => kinds
                    .kinds
                    .get(kind as usize)
                    .map(|k| k.name.clone())
                    .unwrap_or_default(),
                Who::Sentry => SENTRY_NAME.to_string(),
            }
        };
        let hud = huds.seat(seat);
        for note in &noticed {
            hud.events
                .push(HudEvent::GameMessage(line(note, my_slot, name)));
            if online && seat == 0 {
                player.played.push(note.event());
            }
        }
        for note in &heard {
            hud.events
                .push(HudEvent::GameMessage(line(note, my_slot, name)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(w: Who) -> String {
        match w {
            Who::Nobody => String::new(),
            Who::Player(0) => "LegoDude".into(),
            Who::Player(_) => "Bumblebee".into(),
            Who::Enemy(_) => "Soldier".into(),
            Who::Sentry => SENTRY_NAME.into(),
        }
    }

    /// A trap's sentry's kill as the videos show it: "Sentry", the sentry icon in
    /// the sentry's colour, "Soldier" in the enemies'.
    #[test]
    fn a_sentry_kill_reads_as_the_game_has_it() {
        let note = KillNote::new(Who::Sentry, Who::Enemy(0), "TnDamageTypeSentryGun");
        assert_eq!(
            line(&note, Some(0), names),
            vec![
                (FRIENDLY_COLOR, "Sentry ".to_string()),
                (FRIENDLY_COLOR, "[Sentry] ".to_string()),
                (ENEMY_COLOR, "Soldier ".to_string()),
            ]
        );
    }

    /// The player's own kill is in his colour; a teammate's in the friendly one.
    #[test]
    fn a_kill_is_coloured_by_who_looks() {
        let note = KillNote::new(Who::Player(1), Who::Enemy(3), "TnDamageTypeTankShell");
        let mine = line(&note, Some(1), names);
        assert_eq!(mine[0].0, PLAYER_COLOR);
        assert_eq!(mine[1], (PLAYER_COLOR, "[Tank] ".to_string()));
        let theirs = line(&note, Some(0), names);
        assert_eq!(theirs[0], (FRIENDLY_COLOR, "Bumblebee ".to_string()));
    }

    /// A death with no killer: its damage type's Suicide, the icon on the name.
    #[test]
    fn a_death_by_nobody_is_a_suicide() {
        let fire = KillNote::new(Who::Nobody, Who::Enemy(0), "TnDamageTypeFireDot");
        assert_eq!(
            line(&fire, Some(0), names),
            vec![(ENEMY_COLOR, "[Suicide]Soldier[FireDot] ".to_string())]
        );
        let flame = KillNote::new(Who::Nobody, Who::Enemy(0), "TnDamageTypeFlameZone");
        assert_eq!(
            line(&flame, Some(0), names),
            vec![(ENEMY_COLOR, "[Suicide]Soldier ".to_string())]
        );
        let bled = KillNote::new(Who::Nobody, Who::Player(0), "TnDamageTypeDownedDrain");
        assert_eq!(
            line(&bled, Some(0), names),
            vec![(PLAYER_COLOR, "[Suicide]LegoDude ".to_string())]
        );
        // (An enemy's kill of the player: its name and icon in the enemies' colour.)
        let shot = KillNote::new(Who::Enemy(0), Who::Player(0), "TnDamageTypeAssaultRifle");
        assert_eq!(
            line(&shot, Some(0), names),
            vec![
                (ENEMY_COLOR, "Soldier ".to_string()),
                (ENEMY_COLOR, "[AssRifle] ".to_string()),
                (PLAYER_COLOR, "LegoDude ".to_string()),
            ]
        );
    }

    #[test]
    fn icons_take_their_words_places() {
        assert_eq!(
            pieces("[Suicide]Soldier[FireDot] "),
            vec![
                Piece::Icon("GameMessageIconSuicide"),
                Piece::Text("Soldier"),
                Piece::Icon("GameMessageIconFireDot"),
                Piece::Text(" "),
            ]
        );
        assert_eq!(pieces("Sentry "), vec![Piece::Text("Sentry ")]);
        assert_eq!(pieces("[Odd] word"), vec![Piece::Text("[Odd] word")]);
    }

    #[test]
    fn notes_go_online_as_they_are() {
        let note = KillNote::new(Who::Player(2), Who::Enemy(7), "TnDamageTypeSentryRocket");
        assert_eq!(KillNote::of_event(&note.event()), Some(note));
        let numbered = note.of_foe(41);
        assert_eq!(KillNote::of_event(&numbered.event()), Some(numbered));
        let head = numbered.headshot(true);
        assert_eq!(KillNote::of_event(&head.event()), Some(head));
    }

    /// A headshot as the game's screenshot shows it: "Default [Headshot] Soldier",
    /// TnDeathMessage's HeadShotString with the headshot's icon in the weapon's
    /// place, in the killer's colour.
    #[test]
    fn a_headshot_reads_as_the_game_has_it() {
        let note =
            KillNote::new(Who::Player(0), Who::Enemy(3), "TnDamageTypeSniperRifle").headshot(true);
        assert_eq!(
            line(&note, Some(0), names),
            vec![
                (PLAYER_COLOR, "LegoDude ".to_string()),
                (PLAYER_COLOR, "[Headshot] ".to_string()),
                (ENEMY_COLOR, "Soldier ".to_string()),
            ]
        );
        assert_eq!(
            pieces("[Headshot] "),
            vec![Piece::Icon("GameMessageIconHeadshot"), Piece::Text(" ")]
        );
    }

    /// An enemy's death is a headshot when its last blow was a round on its head
    /// (Enemy::headshot) by a damage type that isn't exempt from accuracy kills: a
    /// rifle's round there is; the same round on its chest, a trap sentry's round on
    /// its head (ExemptFromAccuracyKills) and a blast there aren't.
    #[test]
    fn a_kill_by_a_round_to_the_head_is_a_headshot() {
        use crate::enemies::{Body, Death, Hit};
        let body = Body::standing_robot();
        let head = Vec3::new(0.0, 2.9, 0.4);
        let chest = Vec3::new(0.0, 1.0, 0.5);
        let killed = |death: Death, at: Vec3, damage: &'static str| -> KillNote {
            let mut w = World::new();
            w.init_resource::<KillFeed>();
            w.add_observer(enemy_killed);
            let jazz = w.spawn((crate::new_player(), PlayerSlot(0))).id();
            let soldier = w
                .spawn((
                    crate::enemies::Enemy {
                        kind: 3,
                        robot: true,
                        health: 100.0,
                        max_health: 100.0,
                        damage_scale: 1.0,
                        ai_damage_scale: 1.0,
                        head_bones: Vec::new(),
                        bone_damage: Vec::new(),
                        from_behind: (1.0, 1.0),
                        facing: Vec3::NEG_Z,
                        emp_left: 0.0,
                        emp_fx: None,
                        mark: Default::default(),
                        burn: Default::default(),
                        emp_by: None,
                        hurt: Default::default(),
                        last_hit: Hit::default(),
                        last_by: None,
                        last_blow: crate::xp::Blow::Weapon,
                        damage_by: Vec::new(),
                        body: body.clone(),
                        hits: Vec::new(),
                        motion: Default::default(),
                    },
                    Foe {
                        kind: 3,
                        spot: Vec3::ZERO,
                        seed: 0,
                    },
                ))
                .id();
            let mut enemy = w.get_mut::<crate::enemies::Enemy>(soldier).unwrap();
            enemy.take(
                500.0,
                Some(jazz),
                crate::xp::Blow::Weapon,
                Hit::new(death, at, -Vec3::Z).of_type(damage),
            );
            w.entity_mut(soldier).remove::<crate::enemies::Enemy>();
            let (_, noticed) = w.resource_mut::<KillFeed>().fresh();
            assert_eq!(noticed.len(), 1);
            noticed[0]
        };
        let rifle = "TnDamageTypeAssaultRifle";
        let shot = killed(Death::Gunfire, head, rifle);
        assert!(shot.head && shot.killer == Who::Player(0), "{shot:?}");
        assert!(!killed(Death::Gunfire, chest, rifle).head);
        assert!(!killed(Death::Gunfire, head, "TnDamageTypeSentryGun").head);
        assert!(!killed(Death::Explosive, head, "TnDamageTypeBazooka").head);
    }

    /// Each enemy's death is one line, the first come: this PC's own (told to the
    /// others), or one told to it (then its own isn't shown or told), however many
    /// PCs tell of it; another enemy's death, and each of a player's deaths (no
    /// number), each its own line.
    #[test]
    fn each_enemys_death_is_shown_once() {
        let kill = |foe| {
            KillNote::new(Who::Player(0), Who::Enemy(3), "TnDamageTypeAssaultRifle").of_foe(foe)
        };
        let mut feed = KillFeed::default();
        // Its own first, the two others' after.
        feed.noticed.push(kill(7));
        assert_eq!(feed.fresh(), (vec![], vec![kill(7)]));
        feed.heard.extend([kill(7), kill(7)]);
        assert_eq!(feed.fresh(), (vec![], vec![]));
        // Two others' first (in the same frame), its own after.
        feed.heard.extend([kill(8), kill(8)]);
        assert_eq!(feed.fresh(), (vec![kill(8)], vec![]));
        feed.noticed.push(kill(8));
        assert_eq!(feed.fresh(), (vec![], vec![]));
        // Its own and another's in the same frame: the other's.
        feed.noticed.push(kill(9));
        feed.heard.push(kill(9));
        assert_eq!(feed.fresh(), (vec![kill(9)], vec![]));
        // Two deaths of a player, each shown.
        let shot = KillNote::new(Who::Enemy(3), Who::Player(1), "TnDamageTypeAssaultRifle");
        feed.heard.extend([shot, shot]);
        assert_eq!(feed.fresh().0.len(), 2);
    }

    /// The videos' match: three players, LegoDude (slot 0) shoots a Soldier dead.
    /// His round strikes it on every PC (main.rs's remote_shot), so each PC's copy
    /// dies and each PC notices the death and tells the others; each shows one line,
    /// "LegoDude [AssRifle] Soldier", not one for each PC.
    #[test]
    fn a_kill_seen_on_three_pcs_is_one_line_on_each() {
        use crate::enemies::{Death, Hit};
        let mut pcs: Vec<World> = (0..3)
            .map(|_| {
                let mut w = World::new();
                w.init_resource::<KillFeed>();
                w.add_observer(enemy_killed);
                w
            })
            .collect();
        let foe_id = pcs
            .iter_mut()
            .map(|w| {
                let jazzes: Vec<Entity> = (0..3)
                    .map(|slot| w.spawn((crate::new_player(), PlayerSlot(slot))).id())
                    .collect();
                let soldier = w
                    .spawn((
                        crate::enemies::Enemy {
                            kind: 3,
                            robot: true,
                            health: 100.0,
                            max_health: 100.0,
                            damage_scale: 1.0,
                            ai_damage_scale: 1.0,
                            head_bones: Vec::new(),
                            bone_damage: Vec::new(),
                            from_behind: (1.0, 1.0),
                            facing: Vec3::NEG_Z,
                            emp_left: 0.0,
                            emp_fx: None,
                            mark: Default::default(),
                            burn: Default::default(),
                            emp_by: None,
                            hurt: Default::default(),
                            last_hit: Hit::default(),
                            last_by: None,
                            last_blow: crate::xp::Blow::Weapon,
                            damage_by: Vec::new(),
                            body: Default::default(),
                            hits: Vec::new(),
                            motion: Default::default(),
                        },
                        Foe {
                            kind: 3,
                            spot: Vec3::ZERO,
                            seed: 0,
                        },
                    ))
                    .id();
                let wave = crate::waves::Waves::default().adopt(soldier, Default::default());
                let id = wave.id;
                w.entity_mut(soldier).insert(wave);
                // (LegoDude's round, here as on every PC.)
                let mut enemy = w.get_mut::<crate::enemies::Enemy>(soldier).unwrap();
                enemy.take(
                    500.0,
                    Some(jazzes[0]),
                    crate::xp::Blow::Weapon,
                    Hit::new(Death::Gunfire, Vec3::Y, Vec3::Z).of_type("TnDamageTypeAssaultRifle"),
                );
                assert!(!enemy.alive());
                w.entity_mut(soldier).remove::<crate::enemies::Enemy>();
                id
            })
            .collect::<Vec<u32>>()[0];
        // Each PC's frame: what it shows, its own told to the others; then the next
        // frame, what it heard.
        let mut shown = vec![Vec::new(); 3];
        let mut told = Vec::new();
        for (i, w) in pcs.iter_mut().enumerate() {
            let (heard, noticed) = w.resource_mut::<KillFeed>().fresh();
            shown[i].extend(heard.into_iter().chain(noticed.iter().copied()));
            told.extend(noticed.into_iter().map(|n| (i, n)));
        }
        for &(from, note) in &told {
            for (i, w) in pcs.iter_mut().enumerate() {
                if i != from {
                    w.resource_mut::<KillFeed>().heard.push(note);
                }
            }
        }
        for (i, w) in pcs.iter_mut().enumerate() {
            let (heard, noticed) = w.resource_mut::<KillFeed>().fresh();
            shown[i].extend(heard.into_iter().chain(noticed));
        }
        let line =
            KillNote::new(Who::Player(0), Who::Enemy(3), "TnDamageTypeAssaultRifle").of_foe(foe_id);
        for lines in &shown {
            assert_eq!(lines, &vec![line]);
        }
    }
}
