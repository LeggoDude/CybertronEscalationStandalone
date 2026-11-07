//! Experience and class levels, as the game's multiplayer has them (TnXpManager and
//! its TnXpEventType classes: PC-TransGame.ini's [TransGame.TnXpManager] and the
//! events' XpAmount, TransGame.int's names for them).
//!
//! * **Levels.** Each class (the Customize builds' specialty: Scout, Leader, Soldier,
//!   Scientist) has its own experience, its level the number of LevelTable's steps
//!   it has reached (GetLevelForSpecialty): level 0 from nothing, level 1 at 500,
//!   up to the top, 25, at 355,000, past which it earns no more (AddXpForSpecialty
//!   stops there). A class's level unlocks its weapons, upgrades and abilities
//!   (their RequiresLevel: TnXpManager.GetOptionLockStatus, the Customize screen's
//!   locked rows), and two more builds at levels 5 and 10 (UnlockCharacterSlotLevels).
//!   Each level reached brings shards (CurrencyForLevelUp, 1000; level 1's 2000),
//!   which buy the Customize screen's armor parts and sets (the game's prices).
//! * **Prime Mode.** Once every class is at the top level, Prime Mode can be turned
//!   on (the lobby's Prime Mode): for good, it clears every class's experience,
//!   keeping the shards and what they bought, so the levels (and their shards)
//!   come again; it unlocks the Prime heads and shows a medal by the player's name.
//! * **Earning it.** The player at this PC earns it for the class he plays, from
//!   what he does in a match (each PC its own player's: TnPlayerController.
//!   ClientNotifyXpEvent): a kill (the demo's own by what was killed, kill_xp: 2
//!   for a drone up to 250 for a Titan, times 0.75 on Easy and 1.25 on Hard; the
//!   game's multiplayer gives 50) and the kill's awards (TnKillAwardManager's
//!   rules: the match's first kill, 3, 5 and 7 kills without dying, kills at the
//!   same moment, a kill while low on health or from far away, a melee kill of an
//!   enemy his EMP grenade disabled), an assist when an enemy he marked with the
//!   Orbital Pistol or disabled with his EMP grenade is killed by someone else (half
//!   that kill's experience, as the game's 25 is half its 50: assist_xp_at), the
//!   first death of the match, and his weapons' own (three marks out at once,
//!   500 of Fireblast's fire in one life). The game gives none in Escalation
//!   (TnGameReplicationInfoSurvival.CanGainXp); the demo gives multiplayer's in its
//!   matches, as the class levels need it.
//! * **Showing it.** Each award is a point transaction on the HUD (the movie's
//!   mc_pointTransaction: "+50", the award's name and description, hud.rs), a kill's
//!   awards in one transaction (ToggleXpTransaction); a new level, the level-up
//!   notice with its stinger (NotifyLevelUp, MP_LEVEL_UP_MX_STNG).
//!
//! The experience is kept in progress.txt next to the assets folder (as builds.txt),
//! signed so that it can't be changed by hand: one that has been is set aside, and
//! the progress starts over with the game's notice for a damaged profile (see
//! Progress::load).
//! A match's experience counts from the moment it is earned (its levels unlock as
//! they come) but is only kept once the match is over: it ends (the host's Return to
//! Lobby) or online play stops (the host leaving). Quitting it (Quit Game, or
//! closing the demo mid-match) loses it, as the game's multiplayer does
//! (LeaveLobbyXpConfirm: "All gained XP will be lost.").

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::hud::{HudEvent, HudState};
use crate::{Enemy, LocalPlayer, Player};

/// The experience each level starts at (LevelTable): level n from LEVEL_TABLE[n - 1],
/// level 0 from nothing, the top level (MAX_LEVEL) from the last.
pub(crate) const LEVEL_TABLE: [u32; 25] = [
    500, 1500, 3000, 5000, 7500, 11000, 15500, 21000, 27500, 35000, 44000, 54500, 66500, 80000,
    95000, 112000, 131000, 152000, 175000, 200000, 227000, 256000, 287000, 320000, 355000,
];
pub(crate) const MAX_LEVEL: u32 = LEVEL_TABLE.len() as u32;

/// The levels that open a class's second and third builds (TnCharacterCustomizationData's
/// UnlockCharacterSlotLevels).
pub(crate) const BUILD_UNLOCK_LEVELS: [u32; 2] = [5, 10];

/// A class's level for `xp` (GetLevelForSpecialty).
pub(crate) fn level_for(xp: u32) -> u32 {
    LEVEL_TABLE.iter().take_while(|&&t| xp >= t).count() as u32
}

/// The experience a level starts at (GetXpNeededForLevel), None past the top.
pub(crate) fn xp_for(level: u32) -> Option<u32> {
    match level {
        0 => Some(0),
        l if l <= MAX_LEVEL => Some(LEVEL_TABLE[l as usize - 1]),
        _ => None,
    }
}

/// The shards reaching a level brings (CurrencyForLevelUp, CurrencyLevelUpOverrides).
fn shards_for(level: u32) -> u32 {
    if level == 1 { 2000 } else { 1000 }
}

/// Whether an option needing `requires` is open at `level` (GetOptionLockStatus:
/// locked while its RequiresLevel is over the class's level).
pub(crate) fn unlocked(requires: u32, level: u32) -> bool {
    requires <= level
}

/// How many of a class's builds are open at `level`: the first always, the second
/// and third from BUILD_UNLOCK_LEVELS (the Customize screen's refreshBuilds).
pub(crate) fn builds_open(level: u32) -> usize {
    1 + BUILD_UNLOCK_LEVELS.iter().filter(|&&l| l <= level).count()
}

/// The level a build slot (0-based) opens at (its "Unlocks at Level `n").
pub(crate) fn build_unlock_level(slot: usize) -> u32 {
    slot.checked_sub(1)
        .and_then(|i| BUILD_UNLOCK_LEVELS.get(i).copied())
        .unwrap_or(0)
}

/// An award (a TnXpEventType the demo gives).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum XpEvent {
    Kill,
    FirstKill,
    FirstDeath,
    KillStreak3,
    KillStreak5,
    KillStreak7,
    MultiKill2,
    MultiKill3,
    MultiKill4,
    KillerLowHealth,
    LongRange,
    Assist,
    KillFlashBangMelee,
    TracerDartActiveLocks,
    FireShotgunDamage,
}

impl XpEvent {
    /// Its experience (XpAmount, times GlobalXpMultiplier 1; the demo's matches are
    /// team games, so never DeathmatchXpAmount).
    pub fn amount(self) -> u32 {
        use XpEvent::*;
        match self {
            Kill => 50,
            FirstKill => 100,
            FirstDeath => 75,
            KillStreak3 => 100,
            KillStreak5 => 150,
            KillStreak7 => 200,
            MultiKill2 => 100,
            MultiKill3 => 150,
            MultiKill4 => 200,
            KillerLowHealth => 75,
            LongRange => 50,
            Assist => 25,
            KillFlashBangMelee => 50,
            TracerDartActiveLocks => 50,
            FireShotgunDamage => 50,
        }
    }

    /// Its announcement and description (TransGame.int's: GetAnnouncement,
    /// GetDescription; a plain kill has neither).
    pub fn text(self) -> (&'static str, &'static str) {
        use XpEvent::*;
        match self {
            Kill => ("", ""),
            FirstKill => ("First Blood", "You got the first kill of the game."),
            FirstDeath => ("Rough Start", "You were the first to die this game."),
            KillStreak3 => ("3 Kill Streak", "You killed 3 enemies without dying."),
            KillStreak5 => ("5 Kill Streak", "You killed 5 enemies without dying."),
            KillStreak7 => ("7 Kill Streak", "You killed 7 enemies without dying."),
            MultiKill2 => ("Double Kill", "You killed 2 enemies in a row!"),
            MultiKill3 => ("Triple Kill", "You killed 3 enemies in a row!"),
            MultiKill4 => ("Quad Kill", "You killed 4 enemies in a row!"),
            KillerLowHealth => (
                "Just a Flesh Wound!",
                "You killed an enemy while low on health.",
            ),
            LongRange => ("Far and Away", "You killed an enemy at long range."),
            Assist => ("", "Assist"),
            KillFlashBangMelee => (
                "Bangers and Mash",
                "You killed an enemy with a melee attack after disabling them with the Emp Grenade.",
            ),
            TracerDartActiveLocks => (
                "Visionary",
                "You marked many enemies with your Orbital Pistol in a single life;",
            ),
            FireShotgunDamage => (
                "En Fuego",
                "You caused massive damage incinerating enemies with a Fireblast upgraded Scatter Blaster",
            ),
        }
    }
}

/// How a blow was struck, as the kill awards tell kills apart (the damage type: a
/// melee blow, TnDamageTypeMelee; a burn, TnDamageTypeFireDot, which is
/// ExemptFromAccuracyKills, so never a long-range kill).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Blow {
    #[default]
    Weapon,
    Melee,
    Burn,
}

/// Who has hurt an enemy (its damage history, as far as the awards need it): the
/// last Jazz to (its killer, when it dies) and how.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Hurt {
    pub last: Option<(Entity, Blow)>,
}

impl Hurt {
    pub fn by(&mut self, jazz: Option<Entity>, blow: Blow) {
        if let Some(j) = jazz {
            self.last = Some((j, blow));
        }
    }
}

/// What an option needs besides its price (TnDataProvider_Part's Requires* flags:
/// the bonus heads').
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Needs {
    #[default]
    Nothing,
    /// Its class at the top level, or Prime Mode (RequiresMaxLevelOrPrime).
    MaxLevelOrPrime,
    /// Prime Mode (RequiresPrime).
    Prime,
    /// Prime Mode and its class at the top level (RequiresMaxLevelAndPrime).
    MaxLevelAndPrime,
    /// Prime Mode and every class at the top level (RequiresAllMaxLevelAndPrime).
    AllMaxLevelAndPrime,
}

impl Needs {
    /// From customize.txt's lock column (tools/build_customize.py).
    pub fn parse(s: &str) -> Needs {
        match s {
            "max" => Needs::MaxLevelOrPrime,
            "prime" => Needs::Prime,
            "prime_max" => Needs::MaxLevelAndPrime,
            "prime_all" => Needs::AllMaxLevelAndPrime,
            _ => Needs::Nothing,
        }
    }
}

/// Why a part is locked: GetOptionLockStatus's answers past the level's (the DLC's,
/// 7, the demo sells for their price).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lock {
    /// Not bought yet: its price in shards (2).
    Price(u32),
    /// Its class at the top level, or Prime Mode (3: "25").
    MaxLevel,
    /// Prime Mode (4: "PRIME").
    Prime,
    /// Prime Mode and its class at the top level (5: "PRIME 25").
    PrimeMaxLevel,
    /// Prime Mode and every class at the top level (6: "PRIME 100").
    PrimeAllMaxLevels,
}

impl Lock {
    /// What the Customize list shows by a locked row (its RequiresLevel: "25", or
    /// UIText's Prime upper-cased, with " 25" or " 100"); a price shows as a price.
    pub fn label(self, prime: &str) -> Option<String> {
        let prime = prime.to_uppercase();
        match self {
            Lock::Price(_) => None,
            Lock::MaxLevel => Some(MAX_LEVEL.to_string()),
            Lock::Prime => Some(prime),
            Lock::PrimeMaxLevel => Some(format!("{prime} {MAX_LEVEL}")),
            Lock::PrimeAllMaxLevels => Some(format!("{prime} {}", 4 * MAX_LEVEL)),
        }
    }
}

/// Each class's experience, the shards earned, the parts bought with them and
/// Prime Mode (TnXpManager's, kept in the game's profile key archive), kept in
/// progress.txt, signed (Progress::load).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub(crate) struct Progress {
    pub xp: [u32; 4],
    pub shards: u32,
    /// The parts bought, by id (IsCurrencyOptionPurchased: each option's
    /// CurrencyUnlockKeyArchiveKey; whichever class buys a part, it is bought).
    pub owned: std::collections::BTreeSet<String>,
    /// Prime Mode is on (GetPrimeModeUnlocked), for good.
    pub prime: bool,
}

/// progress.txt was set aside as the demo started, changed by hand
/// (Progress::load): the main menu says so, once, with the game's notice (menu.rs).
#[derive(Resource, Default)]
pub(crate) struct ProgressSetAside(pub bool);

/// progress.txt's first line.
const HEADER: &str = "# foc_esc progress (src/xp.rs), signed: changed by hand, the file is \
     set aside and the progress starts over";
/// The key progress.txt is signed with (SipHash's 128 bits). Should it change,
/// every progress.txt signed with the one before is set aside.
const SIGNING_KEY: (u64, u64) = (0xf1e2_a732_8c2e_a1b9, 0xc054_e90a_3cf9_e0b1);
/// The first line the demo's earlier versions wrote (unsigned) as its SipHash-2-4
/// under SIGNING_KEY: a progress.txt that starts with it and has no signature is
/// one of theirs. (The line itself, in the tests below, isn't in the program, for
/// anyone to find and start an unsigned file of his own with; nor is it like the
/// demo's own first line now.)
const UNSIGNED_HEADER_HASH: u64 = 0x8366_8859_4e7d_bd03;
/// What a signature is of, ahead of the values it signs (so that it signs them as
/// progress.txt's and nothing else).
const SIGNED_AS: &str = "foc_esc progress.txt 1\n";
/// The name a progress.txt changed by hand is set aside as (progress.rejected.txt,
/// or progress.rejected.2.txt, .3 ... should that be there already).
const SET_ASIDE_AS: &str = "progress.rejected";

fn progress_path() -> PathBuf {
    let assets = crate::assets_dir();
    assets
        .parent()
        .map(|p| p.join("progress.txt"))
        .unwrap_or_else(|| PathBuf::from("progress.txt"))
}

/// What a progress.txt holds.
#[derive(Debug, PartialEq)]
enum Saved {
    /// Nothing (none, or an empty one): nothing earned yet.
    Nothing,
    /// The progress, as the demo signed it.
    Signed(Progress),
    /// The progress from an earlier version of the demo (its first line, and no
    /// signature): taken as it is, and signed from then on.
    Unsigned(Progress),
    /// Changed since the demo signed it (its values not the ones signed, or the
    /// signature gone): not to be taken.
    Changed,
}

/// A progress.txt's text read: its values, a line each (`xp<Tab>Scout<Tab>600`,
/// `shards<Tab>2000`, `owned<Tab>ScoutHead6` for each part bought and
/// `prime<Tab>1`), and whether they are the ones its signature (`sig<Tab>` and 16
/// hex digits, the last line) is of. Only the values are signed: a file opened and
/// saved again unchanged, even with its line ends changed, is still the demo's.
fn read_saved(text: &str) -> Saved {
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Saved::Nothing;
    }
    let mut p = Progress::default();
    let mut signature = None;
    for line in text.lines() {
        let f: Vec<&str> = line.trim_end().split('\t').collect();
        match (f.first().copied(), f.len()) {
            (Some("xp"), 3) => {
                if let (Some(c), Ok(xp)) = (
                    crate::customize::SPECIALTIES
                        .iter()
                        .position(|s| *s == f[1]),
                    f[2].parse::<u32>(),
                ) {
                    p.xp[c] = xp.min(LEVEL_TABLE[LEVEL_TABLE.len() - 1]);
                }
            }
            (Some("shards"), 2) => p.shards = f[1].parse().unwrap_or(0),
            (Some("owned"), 2) if !f[1].is_empty() => {
                p.owned.insert(f[1].to_string());
            }
            (Some("prime"), 2) => p.prime = f[1] == "1",
            (Some("sig"), 2) => signature = Some(u64::from_str_radix(f[1], 16).ok()),
            _ => {}
        }
    }
    let first = text.lines().next().map(str::trim_end).unwrap_or("");
    match signature {
        Some(Some(s)) if s == sign(&p.values()) => Saved::Signed(p),
        None if siphash24(SIGNING_KEY, first.as_bytes()) == UNSIGNED_HEADER_HASH => {
            Saved::Unsigned(p)
        }
        _ => Saved::Changed,
    }
}

/// The signature of progress.txt's values: the SipHash-2-4 of SIGNED_AS and them
/// under SIGNING_KEY.
fn sign(values: &str) -> u64 {
    siphash24(
        SIGNING_KEY,
        &[SIGNED_AS.as_bytes(), values.as_bytes()].concat(),
    )
}

/// SipHash-2-4 (Aumasson and Bernstein's): a 64-bit hash of `data` that can't be
/// made, or matched by other data, without its 128-bit key.
fn siphash24(key: (u64, u64), data: &[u8]) -> u64 {
    fn round(v: &mut [u64; 4]) {
        v[0] = v[0].wrapping_add(v[1]);
        v[1] = v[1].rotate_left(13) ^ v[0];
        v[0] = v[0].rotate_left(32);
        v[2] = v[2].wrapping_add(v[3]);
        v[3] = v[3].rotate_left(16) ^ v[2];
        v[0] = v[0].wrapping_add(v[3]);
        v[3] = v[3].rotate_left(21) ^ v[0];
        v[2] = v[2].wrapping_add(v[1]);
        v[1] = v[1].rotate_left(17) ^ v[2];
        v[2] = v[2].rotate_left(32);
    }
    let (k0, k1) = key;
    let mut v = [
        k0 ^ 0x736f_6d65_7073_6575,
        k1 ^ 0x646f_7261_6e64_6f6d,
        k0 ^ 0x6c79_6765_6e65_7261,
        k1 ^ 0x7465_6462_7974_6573,
    ];
    let mut take = |m: u64| {
        v[3] ^= m;
        round(&mut v);
        round(&mut v);
        v[0] ^= m;
    };
    let mut words = data.chunks_exact(8);
    for word in &mut words {
        take(u64::from_le_bytes(word.try_into().unwrap()));
    }
    // The bytes left over, with the length's lowest byte on top.
    let mut last = [0u8; 8];
    last[..words.remainder().len()].copy_from_slice(words.remainder());
    last[7] = data.len() as u8;
    take(u64::from_le_bytes(last));
    v[2] ^= 0xff;
    for _ in 0..4 {
        round(&mut v);
    }
    v[0] ^ v[1] ^ v[2] ^ v[3]
}

/// Moves a progress.txt changed by hand aside, beside it, as SET_ASIDE_AS (the first
/// of its names not taken); the name it got.
fn set_aside(path: &Path) -> std::io::Result<PathBuf> {
    let dir = path.parent().unwrap_or(Path::new(""));
    let to = (1..1000)
        .map(|n| match n {
            1 => dir.join(format!("{SET_ASIDE_AS}.txt")),
            n => dir.join(format!("{SET_ASIDE_AS}.{n}.txt")),
        })
        .find(|to| !to.exists())
        .ok_or_else(|| std::io::Error::other("every name taken"))?;
    std::fs::rename(path, &to)?;
    Ok(to)
}

/// Writes a file whole: into a file beside it, then moved over it, so that a crash
/// or a power cut mid-save leaves the one before (half a progress.txt wouldn't
/// match its signature, and would be set aside); written in place instead should
/// that fail (a file held open elsewhere, say).
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let write = |to: &Path| -> std::io::Result<()> {
        let mut file = std::fs::File::create(to)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()
    };
    let mut temp = path.as_os_str().to_owned();
    temp.push(format!(".{}.tmp", std::process::id()));
    let temp = PathBuf::from(temp);
    if write(&temp)
        .and_then(|()| std::fs::rename(&temp, path))
        .is_err()
    {
        let _ = std::fs::remove_file(&temp);
        write(path)?;
    }
    Ok(())
}

impl Progress {
    /// From progress.txt (nothing earned without it), and whether it was set aside.
    /// The demo signs it as it saves it: one whose values aren't the ones signed
    /// (changed by hand, or its signature taken off) isn't taken but moved aside, to
    /// progress.rejected.txt (where it can be looked at, or put back as it was and
    /// moved back), and the progress starts over, as the game's does with a damaged
    /// profile (TnPlayerController.OnReadProfileSettingsComplete: the profile back
    /// to its defaults, and its notice, which the main menu shows). One from an
    /// earlier version of the demo, unsigned, is taken as it is and signed there and
    /// then.
    pub fn load() -> (Progress, bool) {
        Progress::load_from(&progress_path())
    }

    fn load_from(path: &Path) -> (Progress, bool) {
        let saved = match std::fs::read(path) {
            Ok(bytes) => String::from_utf8(bytes).map_or(Saved::Changed, |t| read_saved(&t)),
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    warn!("Couldn't read {}: {e}", path.display());
                }
                Saved::Nothing
            }
        };
        match saved {
            Saved::Nothing => (Progress::default(), false),
            Saved::Signed(p) => (p, false),
            Saved::Unsigned(p) => {
                info!(
                    "{} is from an earlier version of the demo: signed now",
                    path.display()
                );
                p.save_to(path);
                (p, false)
            }
            Saved::Changed => {
                match set_aside(path) {
                    Ok(to) => warn!(
                        "{} was changed by hand: set aside as {}, and the progress starts over",
                        path.display(),
                        to.display()
                    ),
                    Err(e) => warn!(
                        "{} was changed by hand and couldn't be set aside ({e}): the progress \
                         starts over",
                        path.display()
                    ),
                }
                (Progress::default(), true)
            }
        }
    }

    /// Into progress.txt, signed.
    pub(crate) fn save(&self) {
        self.save_to(&progress_path());
    }

    fn save_to(&self, path: &Path) {
        if let Err(e) = write_whole(path, &self.file_text()) {
            warn!("Couldn't save the experience: {e}");
        }
    }

    /// Its values as progress.txt holds them, a line each: what is signed.
    fn values(&self) -> String {
        let mut s = String::new();
        for (c, xp) in self.xp.iter().enumerate() {
            let _ = writeln!(s, "xp\t{}\t{xp}", crate::customize::SPECIALTIES[c]);
        }
        let _ = writeln!(s, "shards\t{}", self.shards);
        for id in &self.owned {
            let _ = writeln!(s, "owned\t{id}");
        }
        if self.prime {
            s.push_str("prime\t1\n");
        }
        s
    }

    /// progress.txt's text: its first line, the values and their signature.
    fn file_text(&self) -> String {
        let values = self.values();
        format!("{HEADER}\n{values}sig\t{:016x}\n", sign(&values))
    }

    /// Whether a class is at the top level (IsLevelMaxed).
    pub fn is_maxed(&self, class: usize) -> bool {
        self.xp(class) >= LEVEL_TABLE[LEVEL_TABLE.len() - 1]
    }

    /// Whether every class is at the top level (AllLevelsMaxed).
    pub fn all_maxed(&self) -> bool {
        (0..self.xp.len()).all(|c| self.is_maxed(c))
    }

    /// Prime Mode can be turned on: not yet, and every class at the top level
    /// (TnCharacterScriptBinding.IsPrimeModeAvailable).
    pub fn prime_mode_available(&self) -> bool {
        !self.prime && self.all_maxed()
    }

    /// A part's lock for a class (TnXpManager.GetOptionLockStatus: not bought,
    /// then what it needs besides), or None if it can be worn.
    pub fn part_lock(&self, id: &str, price: u32, needs: Needs, class: usize) -> Option<Lock> {
        if price > 0 && !self.owned.contains(id) {
            return Some(Lock::Price(price));
        }
        let maxed = self.is_maxed(class);
        match needs {
            Needs::MaxLevelOrPrime if !maxed && !self.prime => Some(Lock::MaxLevel),
            Needs::Prime if !self.prime => Some(Lock::Prime),
            Needs::MaxLevelAndPrime if !maxed || !self.prime => Some(Lock::PrimeMaxLevel),
            Needs::AllMaxLevelAndPrime if !self.all_maxed() || !self.prime => {
                Some(Lock::PrimeAllMaxLevels)
            }
            _ => None,
        }
    }

    /// Buys a part with shards (PurchaseCurrencyOption: kept as bought, its price
    /// taken from the shards); nothing for a free part or one already bought.
    pub fn buy(&mut self, id: &str, price: u32) -> bool {
        if price == 0 || self.owned.contains(id) {
            return false;
        }
        self.owned.insert(id.to_string());
        self.shards = self.shards.saturating_sub(price);
        true
    }

    /// Prime Mode turned on (OnPrimeModeUnlocked): for good, with every class's
    /// experience cleared (ClearXp); the shards and the parts bought are kept, and
    /// the levels pay their shards again as they are earned again.
    pub fn unlock_prime_mode(&mut self) {
        self.prime = true;
        self.xp = [0; 4];
    }

    /// A class's level.
    pub fn level(&self, class: usize) -> u32 {
        level_for(self.xp(class))
    }

    /// A class's experience (GetXpEarnedForSpecialty).
    pub fn xp(&self, class: usize) -> u32 {
        self.xp.get(class).copied().unwrap_or(0)
    }

    /// Adds experience to a class, no further than the top (AddXpForSpecialty); the
    /// experience it took.
    fn add(&mut self, class: usize, xp: u32) -> u32 {
        let top = LEVEL_TABLE[LEVEL_TABLE.len() - 1];
        let Some(have) = self.xp.get_mut(class) else {
            return 0;
        };
        let added = xp.min(top - (*have).min(top));
        *have += added;
        added
    }
}

/// This match's awards so far, as the game keeps them for the player at this PC
/// (TnPlayerReplicationInfo's kill streak and multi-kill detector, the GRI's first
/// kill, the Orbital Pistol's and Fireblast's counts), and the awards to hand out.
#[derive(Resource, Default)]
pub(crate) struct MatchXp {
    /// The match's first kill (or death) has been awarded (HasAwardedFirstKill).
    first_kill: bool,
    /// Kills since he last died (_CurrentKillStreak).
    streak: u32,
    /// Kills at the same moment (TnMultiKillDetector: its TimeBetweenMultiKills is
    /// 0), and that moment.
    multi: u32,
    multi_at: f32,
    /// Marks of his out at once, as last counted, and whether three out has been
    /// awarded since he last had just one (CheckForActiveLocksXpEvent).
    marks_out: u32,
    gave_marks: bool,
    /// Kills to award (the dead enemies he had a hand in), and awards to hand out:
    /// each with its transaction (a kill's awards share one).
    pending_kills: Vec<Kill>,
    /// Each award, its experience (a kill's by the enemy: kill_xp) and its
    /// transaction.
    awards: Vec<(XpEvent, u32, i32)>,
    next_transaction: i32,
    /// He was down last frame (his death awards once).
    was_down: bool,
}

/// The experience a kill of each enemy type brings (by its AI type, AICT_*): the
/// demo's own, as the game's Escalation gives none (its multiplayer gives 50 a
/// kill): what each is to fight, a tougher one more, scaled so the levels come at
/// multiplayer's pace. Unknown types bring a Soldier's.
pub(crate) fn kill_xp(ai_type: &str) -> u32 {
    match ai_type {
        "AICT_ExplosiveDrone" | "AICT_SupportDrone" => 2,
        "AICT_Scrapper" | "AICT_Swarmer" => 10,
        "AICT_RocketTrooper" => 30,
        "AICT_Sniper" | "AICT_SharpShooter" | "AICT_Jet" => 40,
        "AICT_Shotgunner" => 50,
        "AICT_HeavyGunner" => 100,
        "AICT_Guard" => 150,
        "AICT_Leaper" => 200,
        "AICT_Titan" => 250,
        _ => 20,
    }
}

/// A kill's experience at a difficulty (0 easy, 1 normal, 2 hard): its type's
/// times 0.75, 1 or 1.25, as the game scales Escalation's rewards, rounded down.
pub(crate) fn kill_xp_at(ai_type: &str, difficulty: usize) -> u32 {
    let base = kill_xp(ai_type);
    match difficulty {
        0 => base * 3 / 4,
        1 => base,
        _ => base * 5 / 4,
    }
}

/// An assist's experience for a kill of that type at a difficulty: the kill's
/// (kill_xp_at), scaled as the game's multiplayer gives an assist to a kill
/// (Assist's 25 to Kill's 50: half), rounded down, and never none.
pub(crate) fn assist_xp_at(ai_type: &str, difficulty: usize) -> u32 {
    (kill_xp_at(ai_type, difficulty) * XpEvent::Assist.amount() / XpEvent::Kill.amount()).max(1)
}

/// An enemy that has died: what it was, where, who killed it and how (and the
/// death its last blow gave it, a headshot or not: the killer's line, voice.rs),
/// and whom its marks and EMP were from.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Kill {
    /// Its type (enemies.rs's Kinds).
    pub kind: usize,
    pub at: Vec3,
    pub killer: Option<(Entity, Blow)>,
    pub death: crate::enemies::Death,
    pub head: bool,
    pub marked_by: Option<Entity>,
    pub emp_by: Option<Entity>,
}

/// The awards' distances and thresholds: a long-range kill beyond 80 m
/// (TnKillAwardRuleLongRange's LongDistance), a low-health kill under 50 health
/// (TnKillAwardRuleKillerLowHealth), Fireblast's 500 of fire in a life
/// (TnBuffDotFromFireShotgun.OnDamage's FireDamageXpEventThreshold), three marks out
/// (kActiveLockXpThreshold).
const LONG_RANGE: f32 = 80.0;
const LOW_HEALTH: f32 = 50.0;
pub(crate) const FIRE_DAMAGE_AWARD: f32 = 500.0;
const ACTIVE_MARKS: u32 = 3;

pub struct XpPlugin;

impl Plugin for XpPlugin {
    fn build(&self, app: &mut App) {
        let (progress, set_aside) = Progress::load();
        app.insert_resource(progress)
            .insert_resource(ProgressSetAside(set_aside))
            .init_resource::<MatchXp>()
            .init_resource::<MatchStart>()
            .add_observer(enemy_died)
            .add_systems(OnEnter(crate::menu::AppState::Game), new_match)
            .add_systems(OnExit(crate::menu::AppState::Game), keep_or_lose)
            .add_systems(
                Update,
                (award_kills, award_lives, give_awards)
                    .chain()
                    .after(crate::fire)
                    .before(crate::hud::HudSystems)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

/// The experience as the match began, to go back to should its player quit it.
#[derive(Resource, Default)]
struct MatchStart(Option<Progress>);

/// A new match: no awards yet, and the experience as it begins.
fn new_match(mut m: ResMut<MatchXp>, progress: Res<Progress>, mut start: ResMut<MatchStart>) {
    let next = m.next_transaction;
    *m = MatchXp {
        next_transaction: next,
        ..default()
    };
    start.0 = Some(progress.clone());
}

/// The match is left: quit (Quit Game), the experience it earned is lost (back to
/// what it began with); otherwise it is kept, in progress.txt.
fn keep_or_lose(
    mut progress: ResMut<Progress>,
    mut start: ResMut<MatchStart>,
    mut pause: ResMut<crate::menu::Pause>,
) {
    let before = start.0.take();
    if std::mem::take(&mut pause.forfeit_xp) {
        if let Some(before) = before {
            *progress = before;
        }
        return;
    }
    if before.as_ref() != Some(&*progress) {
        progress.save();
    }
}

/// An enemy that has died (as it goes): its kill, for award_kills.
fn enemy_died(
    gone: On<Remove, Enemy>,
    mut m: ResMut<MatchXp>,
    enemies: Query<(&Transform, &Enemy)>,
) {
    let Ok((transform, enemy)) = enemies.get(gone.entity) else {
        return;
    };
    if enemy.health > 0.0 {
        return;
    }
    let hit = enemy.last_hit;
    m.pending_kills.push(Kill {
        kind: enemy.kind,
        at: transform.translation,
        killer: enemy.hurt.last,
        death: hit.death,
        // (As its death is picked: enemies.rs.)
        head: matches!(
            hit.death,
            crate::enemies::Death::Gunfire | crate::enemies::Death::SingleShot
        ) && enemy.body.head_at(hit.at),
        marked_by: enemy.mark.marker(),
        emp_by: (enemy.emp_left > 0.0).then_some(enemy.emp_by).flatten(),
    });
}

/// The kills of this frame, awarded as TnKillAwardManager.AwardKill does, for the
/// player at this PC: his own kills (the kill, then each rule's award: his kill
/// streak, the match's first kill, kills at the same moment, low health, long range,
/// a melee kill of an enemy his EMP grenade disabled), and an assist for an enemy he
/// marked or disabled that another killed (KilledOffensiveBuffCheck's AssistXpEvent).
/// His kills have him say his line for them (voice.rs), as split screen's guest's
/// have him.
fn award_kills(
    time: Res<Time>,
    mut m: ResMut<MatchXp>,
    players: Query<(Entity, &Transform, &Player, Option<&crate::split::Seat>), With<LocalPlayer>>,
    mut voice: ResMut<crate::voice::VoiceLines>,
    kinds: Option<Res<crate::enemies::Kinds>>,
    net: Res<crate::net::Net>,
) {
    if m.pending_kills.is_empty() {
        return;
    }
    let kills = std::mem::take(&mut m.pending_kills);
    // (The first player's: split screen's guest earns none, xp.rs's docs.)
    let Some((me, transform, player, _)) = players.iter().find(|p| crate::split::seat_of(p.3) == 0)
    else {
        // (The match's first kill is still the first, whoever made it.)
        if kills.iter().any(|k| k.killer.is_some()) {
            m.first_kill = true;
        }
        return;
    };
    // (Split screen's guest: he says his lines for his kills, though the
    // experience isn't his.)
    let guest = players
        .iter()
        .find(|p| crate::split::seat_of(p.3) == 1)
        .map(|p| (p.0, p.2));
    let now = time.elapsed_secs();
    for kill in kills {
        let Some((killer, blow)) = kill.killer else {
            continue;
        };
        // (What was killed: its kill's and its assist's experience.)
        let ai_type = kinds
            .as_deref()
            .and_then(|k| k.kinds.get(kill.kind))
            .map_or("", |k| k.ai_type.as_str());
        if killer != me {
            m.first_kill = true;
            if let Some((guest, guest_player)) = guest
                && killer == guest
                && let Some(line) = crate::voice::Line::for_kill(
                    kill.death,
                    kill.head,
                    crate::dialog_category(guest_player),
                )
            {
                voice.say_for(1, line);
            }
            // (Marks and EMP grenades: an assist for him when another kills it, by
            // what was killed and the difficulty, as a kill is.)
            if kill.marked_by == Some(me) || kill.emp_by == Some(me) {
                let t = m.next_transaction;
                m.next_transaction += 1;
                m.awards
                    .push((XpEvent::Assist, assist_xp_at(ai_type, net.difficulty()), t));
            }
            continue;
        }
        if let Some(line) =
            crate::voice::Line::for_kill(kill.death, kill.head, crate::dialog_category(player))
        {
            voice.say(line);
        }
        // (TnPlayerReplicationInfo.AddKills: the streak and the multi-kill.)
        m.streak += 1;
        if now - m.multi_at <= 0.0 {
            m.multi += 1;
        } else {
            m.multi = 1;
        }
        m.multi_at = now;
        let t = m.next_transaction;
        m.next_transaction += 1;
        let mut give = vec![XpEvent::Kill];
        match m.streak {
            3 => give.push(XpEvent::KillStreak3),
            5 => give.push(XpEvent::KillStreak5),
            7 => give.push(XpEvent::KillStreak7),
            _ => {}
        }
        if kill.emp_by == Some(me) && blow == Blow::Melee {
            give.push(XpEvent::KillFlashBangMelee);
        }
        if !m.first_kill {
            m.first_kill = true;
            give.push(XpEvent::FirstKill);
        }
        if player.health < LOW_HEALTH {
            give.push(XpEvent::KillerLowHealth);
        }
        // (A burn is ExemptFromAccuracyKills: never a long-range kill.)
        if blow != Blow::Burn && transform.translation.distance(kill.at) > LONG_RANGE {
            give.push(XpEvent::LongRange);
        }
        match m.multi {
            2 => give.push(XpEvent::MultiKill2),
            3 => give.push(XpEvent::MultiKill3),
            4 => give.push(XpEvent::MultiKill4),
            _ => {}
        }
        // (The kill itself by what was killed and the difficulty; its awards as
        // the game's.)
        let kill_xp = kill_xp_at(ai_type, net.difficulty());
        m.awards.extend(give.into_iter().map(|e| {
            let amount = if e == XpEvent::Kill {
                kill_xp
            } else {
                e.amount()
            };
            (e, amount, t)
        }));
    }
}

/// The player at this PC's life: his death ends his kill streak (KillStreakEnded)
/// and, the match's first, is its first death (FirstKillAndFirstDeath's FirstDeath);
/// his weapons' awards for a life (three Orbital Pistol marks out at once,
/// Fireblast's fire) come as he earns them.
fn award_lives(
    mut m: ResMut<MatchXp>,
    mut players: Query<(Entity, &mut Player, Option<&crate::split::Seat>), With<LocalPlayer>>,
    enemies: Query<&Enemy>,
) {
    // (The first player's.)
    let Some((me, mut player, _)) = players.iter_mut().find(|p| crate::split::seat_of(p.2) == 0)
    else {
        return;
    };
    let down = player.dying || player.dead_left > 0.0;
    if down && !m.was_down {
        m.streak = 0;
        m.multi = 0;
        player.fire_damage = 0.0;
        if !m.first_kill {
            m.first_kill = true;
            let t = m.next_transaction;
            m.next_transaction += 1;
            m.awards
                .push((XpEvent::FirstDeath, XpEvent::FirstDeath.amount(), t));
        }
    }
    m.was_down = down;
    // Three of his marks out at once (counted as a mark goes on: one starts it over).
    let marks = enemies
        .iter()
        .filter(|e| e.health > 0.0 && e.mark.marker() == Some(me))
        .count() as u32;
    if marks > m.marks_out {
        if marks == 1 {
            m.gave_marks = false;
        }
        if marks >= ACTIVE_MARKS && !m.gave_marks {
            m.gave_marks = true;
            let t = m.next_transaction;
            m.next_transaction += 1;
            m.awards.push((
                XpEvent::TracerDartActiveLocks,
                XpEvent::TracerDartActiveLocks.amount(),
                t,
            ));
        }
    }
    m.marks_out = marks;
    // Fireblast's fire: 500 of it in one life (crossed once).
    if player.fire_damage >= FIRE_DAMAGE_AWARD && !player.fire_awarded {
        player.fire_awarded = true;
        let t = m.next_transaction;
        m.next_transaction += 1;
        m.awards.push((
            XpEvent::FireShotgunDamage,
            XpEvent::FireShotgunDamage.amount(),
            t,
        ));
    }
    if !down && player.fire_damage < FIRE_DAMAGE_AWARD {
        player.fire_awarded = false;
    }
}

/// Hands out the awards to the class he plays (NotifyXpEvent): its experience,
/// each on the HUD in its transaction, and a level reached (CheckForLevelUp: the
/// HUD's level-up notice and its stinger, and its shards); kept in progress.txt
/// once the match is over (keep_or_lose). The class is the one he has on now, not
/// one chosen from the pause menu for his next life (choose.rs: it only comes in
/// as he respawns).
fn give_awards(
    mut m: ResMut<MatchXp>,
    mut progress: ResMut<Progress>,
    mut hud: ResMut<HudState>,
    mut sfx: ResMut<crate::sound::Sfx>,
    chosen: Res<crate::choose::Chosen>,
    me: Query<(&crate::Player, Option<&crate::split::Seat>), With<crate::LocalPlayer>>,
) {
    if m.awards.is_empty() {
        return;
    }
    // (The first player's: split screen's guest earns none.)
    let class = me
        .iter()
        .find(|(_, s)| crate::split::seat_of(*s) == 0)
        .map_or(chosen.build.map_or(0, |(c, _)| c), |(p, _)| p.kit.class);
    let before = progress.level(class);
    for (event, amount, transaction) in std::mem::take(&mut m.awards) {
        // (The HUD shows the award's amount, GetAwardAmount, even at the top level
        // where it adds nothing.)
        progress.add(class, amount);
        let (announcement, description) = event.text();
        hud.events.push(HudEvent::Points {
            amount,
            announcement: announcement.to_string(),
            description: description.to_string(),
            transaction,
        });
    }
    let after = progress.level(class);
    if after > before {
        for level in before + 1..=after {
            progress.shards += shards_for(level);
        }
        hud.events.push(HudEvent::LevelUp {
            level: after,
            class,
        });
        sfx.play(SND_LEVEL_UP);
    }
}

/// The level-up stinger (TnPlayerController's LevelUpMusic; the HUD's NotifyLevelUp
/// plays the same).
const SND_LEVEL_UP: &str = "BL_LVL_MP.MP_LEVEL_UP_MX_STNG";

#[cfg(test)]
mod tests {
    use super::*;

    /// The first line the demo's earlier versions wrote (UNSIGNED_HEADER_HASH's).
    const UNSIGNED_HEADER: &str =
        "# Each class's experience, the shards earned, the parts bought and Prime Mode (src/xp.rs)";

    #[test]
    fn levels_follow_the_table() {
        assert_eq!(level_for(0), 0);
        assert_eq!(level_for(499), 0);
        assert_eq!(level_for(500), 1);
        assert_eq!(level_for(1499), 1);
        assert_eq!(level_for(1500), 2);
        assert_eq!(level_for(354_999), 24);
        assert_eq!(level_for(355_000), 25);
        assert_eq!(xp_for(0), Some(0));
        assert_eq!(xp_for(1), Some(500));
        assert_eq!(xp_for(25), Some(355_000));
        assert_eq!(xp_for(26), None);
    }

    #[test]
    fn builds_open_with_levels() {
        assert_eq!(builds_open(0), 1);
        assert_eq!(builds_open(4), 1);
        assert_eq!(builds_open(5), 2);
        assert_eq!(builds_open(9), 2);
        assert_eq!(builds_open(10), 3);
        assert_eq!(builds_open(MAX_LEVEL), 3);
        assert_eq!(build_unlock_level(0), 0);
        assert_eq!(build_unlock_level(1), 5);
        assert_eq!(build_unlock_level(2), 10);
        assert!(unlocked(0, 0) && unlocked(6, 6) && !unlocked(7, 6));
    }

    /// Quit Game loses what the match earned: the experience and shards go back to
    /// what the match began with (and nothing is saved).
    #[test]
    fn quitting_loses_the_match_experience() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let before = Progress {
            xp: [600, 0, 0, 0],
            shards: 2000,
            ..default()
        };
        world.insert_resource(Progress {
            xp: [1600, 0, 0, 0],
            shards: 3000,
            ..default()
        });
        world.insert_resource(MatchStart(Some(before.clone())));
        world.insert_resource(crate::menu::Pause {
            forfeit_xp: true,
            ..default()
        });
        world.run_system_once(keep_or_lose).unwrap();
        assert_eq!(*world.resource::<Progress>(), before);
        assert!(!world.resource::<crate::menu::Pause>().forfeit_xp);
        assert!(world.resource::<MatchStart>().0.is_none());
    }

    /// A kill's experience goes to the class he is playing, not to one chosen from
    /// the pause menu for his next life.
    #[test]
    fn awards_go_to_the_class_in_play() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.insert_resource(Progress::default());
        world.insert_resource(HudState::default());
        world.insert_resource(crate::sound::Sfx::default());
        let mut chosen = crate::choose::Chosen::default();
        chosen.build = Some((0, 0));
        world.insert_resource(chosen);
        let mut m = MatchXp::default();
        m.awards.push((XpEvent::Kill, 600, 0));
        world.insert_resource(m);
        let mut player = crate::new_player();
        player.kit.class = 2;
        world.spawn((player, crate::LocalPlayer));
        world.run_system_once(give_awards).unwrap();
        let progress = world.resource::<Progress>();
        assert_eq!((progress.xp(0), progress.xp(2)), (0, 600));
    }

    /// Prime Mode: open once every class is at the top; it clears the experience
    /// and keeps the shards and what was bought.
    #[test]
    fn prime_mode_clears_the_experience_only() {
        let mut p = Progress {
            xp: [355_000, 355_000, 355_000, 354_999],
            shards: 4321,
            ..default()
        };
        p.owned.insert("ScoutHead6".into());
        assert!(!p.prime_mode_available());
        p.xp[3] = 355_000;
        assert!(p.all_maxed() && p.prime_mode_available());
        p.unlock_prime_mode();
        assert!(p.prime && !p.prime_mode_available());
        assert_eq!(p.xp, [0; 4]);
        assert_eq!(p.shards, 4321);
        assert!(p.owned.contains("ScoutHead6"));
    }

    /// A kill's experience by what was killed, scaled by the difficulty and
    /// rounded down.
    #[test]
    fn kills_bring_their_enemys_experience() {
        assert_eq!(kill_xp_at("AICT_Titan", 1), 250);
        assert_eq!(kill_xp_at("AICT_Titan", 0), 187);
        assert_eq!(kill_xp_at("AICT_Titan", 2), 312);
        assert_eq!(kill_xp_at("AICT_ExplosiveDrone", 0), 1);
        assert_eq!(kill_xp_at("AICT_SupportDrone", 2), 2);
        assert_eq!(kill_xp_at("AICT_Soldier", 1), 20);
        assert_eq!(kill_xp_at("AICT_Leaper", 2), 250);
        // (An assist: half the kill's, at least 1.)
        assert_eq!(assist_xp_at("AICT_Soldier", 1), 10);
        assert_eq!(assist_xp_at("AICT_Titan", 0), 93);
        assert_eq!(assist_xp_at("AICT_Titan", 2), 156);
        assert_eq!(assist_xp_at("AICT_ExplosiveDrone", 0), 1);
    }

    #[test]
    fn experience_stops_at_the_top() {
        let mut p = Progress::default();
        assert_eq!(p.add(1, 50), 50);
        p.xp[1] = 354_990;
        assert_eq!(p.add(1, 50), 10);
        assert_eq!(p.add(1, 50), 0);
        assert_eq!(p.level(1), MAX_LEVEL);
    }

    /// SipHash-2-4 as its paper has it (its appendix's example, the key 00 ... 0f
    /// and the 15 bytes 00 ... 0e) and its reference vectors (the same key, the
    /// empty message and the 8 bytes 00 ... 07, a whole word and its length's).
    #[test]
    fn siphash_matches_its_paper() {
        let key = (0x0706_0504_0302_0100, 0x0f0e_0d0c_0b0a_0908);
        let bytes: Vec<u8> = (0..15).collect();
        assert_eq!(siphash24(key, &bytes), 0xa129_ca61_49be_45e5);
        assert_eq!(siphash24(key, &[]), 0x726f_db47_dd0e_0e31);
        assert_eq!(siphash24(key, &bytes[..8]), 0x93f5_f579_9a93_2462);
    }

    fn earned() -> Progress {
        let mut p = Progress {
            xp: [600, 0, 15_500, 355_000],
            shards: 2000,
            ..default()
        };
        p.owned.insert("ScoutHead6".into());
        p.owned.insert("LeaderArmorSet7".into());
        p
    }

    /// What the demo saves it takes back, signed; and opened and saved again
    /// unchanged (its line ends made CRLF, a byte-order mark put in front, a
    /// comment added), it still does.
    #[test]
    fn signed_progress_comes_back() {
        let p = earned();
        let text = p.file_text();
        assert!(text.starts_with(HEADER) && text.lines().last().unwrap().starts_with("sig\t"));
        assert_eq!(read_saved(&text), Saved::Signed(p.clone()));
        let crlf = text.replace('\n', "\r\n");
        assert_eq!(read_saved(&crlf), Saved::Signed(p.clone()));
        assert_eq!(
            read_saved(&format!("\u{feff}{text}")),
            Saved::Signed(p.clone())
        );
        assert_eq!(
            read_saved(&format!("{text}# looked at\n")),
            Saved::Signed(p.clone())
        );
        let prime = Progress { prime: true, ..p };
        assert_eq!(read_saved(&prime.file_text()), Saved::Signed(prime));
    }

    /// Changed by hand, it isn't taken: a number changed, a line added or taken
    /// out, the signature changed or taken off (unsigned, only the earlier
    /// versions' first line passes, and the demo's own isn't that).
    #[test]
    fn changes_by_hand_are_refused() {
        let text = earned().file_text();
        let changed = |from: &str, to: &str| {
            assert!(text.contains(from), "{from}");
            read_saved(&text.replacen(from, to, 1))
        };
        assert_eq!(changed("shards\t2000", "shards\t99999"), Saved::Changed);
        assert_eq!(
            changed("xp\tScout\t600", "xp\tScout\t355000"),
            Saved::Changed
        );
        assert_eq!(changed("owned\tScoutHead6\n", ""), Saved::Changed);
        assert_eq!(
            changed("shards\t2000\n", "shards\t2000\nowned\tSoldierHead9\n"),
            Saved::Changed
        );
        assert_eq!(
            changed("shards\t2000\n", "shards\t2000\nprime\t1\n"),
            Saved::Changed
        );
        // (A second shards line: the last one counts.)
        assert_eq!(
            changed("shards\t2000\n", "shards\t2000\nshards\t50000\n"),
            Saved::Changed
        );
        let sig = text.lines().last().unwrap();
        let mut forged = sig.to_string();
        let last = forged.pop().unwrap();
        forged.push(if last == '0' { '1' } else { '0' });
        assert_eq!(changed(sig, &forged), Saved::Changed);
        assert_eq!(changed(sig, "sig\tnot hex"), Saved::Changed);
        assert_eq!(changed(&format!("{sig}\n"), ""), Saved::Changed);
        let unsigned = text.replacen(&format!("{sig}\n"), "", 1);
        assert_eq!(
            read_saved(&unsigned.replacen(HEADER, "# foc_esc progress (src/xp.rs)", 1)),
            Saved::Changed
        );
        assert_ne!(HEADER, UNSIGNED_HEADER);
    }

    /// A progress.txt from before the demo signed it (its first line, no
    /// signature) is taken as it is; nothing in one, nothing earned.
    #[test]
    fn earlier_versions_progress_is_taken() {
        assert_eq!(
            siphash24(SIGNING_KEY, UNSIGNED_HEADER.as_bytes()),
            UNSIGNED_HEADER_HASH
        );
        let old = format!(
            "{UNSIGNED_HEADER}\nxp\tScout\t600\nxp\tLeader\t0\nxp\tSoldier\t15500\n\
             xp\tScientist\t355000\nshards\t2000\nowned\tLeaderArmorSet7\n\
             owned\tScoutHead6\n"
        );
        assert_eq!(read_saved(&old), Saved::Unsigned(earned()));
        assert_eq!(
            read_saved(&old.replace('\n', "\r\n")),
            Saved::Unsigned(earned())
        );
        // (Without that first line, it is no earlier version's.)
        let headless = old.replacen(&format!("{UNSIGNED_HEADER}\n"), "", 1);
        assert_eq!(read_saved(&headless), Saved::Changed);
        assert_eq!(read_saved(""), Saved::Nothing);
        assert_eq!(read_saved(" \r\n\n"), Saved::Nothing);
    }

    /// Loading, from a folder of the test's own: a changed progress.txt is moved
    /// aside (to progress.rejected.txt, then .2 ...) and nothing is taken from it;
    /// an earlier version's is taken and signed where it is; what is saved comes
    /// back, with nothing left beside it.
    #[test]
    fn loading_sets_changed_progress_aside() {
        let dir = std::env::temp_dir().join(format!("foc_esc_xp_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("progress.txt");
        let files = || {
            let mut names: Vec<String> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };

        assert_eq!(Progress::load_from(&path), (Progress::default(), false));

        let p = earned();
        p.save_to(&path);
        assert_eq!(files(), ["progress.txt"]);
        assert_eq!(Progress::load_from(&path), (p.clone(), false));

        let tampered = p.file_text().replace("shards\t2000", "shards\t99999");
        std::fs::write(&path, &tampered).unwrap();
        assert_eq!(Progress::load_from(&path), (Progress::default(), true));
        assert_eq!(files(), ["progress.rejected.txt"]);
        assert_eq!(
            std::fs::read_to_string(dir.join("progress.rejected.txt")).unwrap(),
            tampered
        );
        std::fs::write(&path, &tampered).unwrap();
        assert_eq!(Progress::load_from(&path), (Progress::default(), true));
        assert_eq!(
            files(),
            ["progress.rejected.2.txt", "progress.rejected.txt"]
        );

        let old = p.file_text().replacen(HEADER, UNSIGNED_HEADER, 1);
        let old: String = old
            .lines()
            .filter(|l| !l.starts_with("sig\t"))
            .map(|l| format!("{l}\n"))
            .collect();
        std::fs::write(&path, old).unwrap();
        assert_eq!(Progress::load_from(&path), (p.clone(), false));
        let signed = std::fs::read_to_string(&path).unwrap();
        assert_eq!(signed, p.file_text());
        assert_eq!(Progress::load_from(&path), (p, false));
        assert_eq!(
            files(),
            [
                "progress.rejected.2.txt",
                "progress.rejected.txt",
                "progress.txt"
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
