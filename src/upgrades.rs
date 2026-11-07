//! Weapon upgrades, as the game's multiplayer has them: each weapon of a Customize
//! build carries one of its upgrades (the build's weapon upgrades, the character's
//! WeaponUpgradePairs: the Customize screen's "Select Weapon Upgrade", customize.rs),
//! which is installed on the weapon as it is given to him (TnWeapon.GivenTo:
//! AttachUpgradesFromInstigator, InstallModification). Each upgrade is a
//! TnWeaponUpgrade class (TransGame.xxx) named by a TnDataProvider_WeaponUpgrade in
//! TransWeapons.ini (its UniqueId, WeaponRestriction and RequiresLevel; the
//! customization lists those for multiplayer, SupportedGames MP: three a weapon).
//!
//! Installing one (TnWeapon.InstallModification) does two things:
//!
//! * **Its buff** (the class's BuffToApply) goes on the weapon. Most are a
//!   TnBuffWeaponFloatModifier whose modifier is picked for the weapon's class from
//!   its FloatModifiers in a multiplayer match (TnBuffWeaponFloatModifier.Apply:
//!   GetCurrentSkillDataIndex 0, TnMultiplayerGame's; Escalation's, 1, would take
//!   FloatModifiersSp), first match by class name (ChooseModifier; the ini spells
//!   some names in other cases, TnWeaponAoeSlow, TnWeaponAOESlow, which match);
//!   the weapon's code then reads them through TnValuePlusSumBuffModifier (added:
//!   the clip, the reserve, the clusters) or TnValueTimesProductBuffModifier
//!   (multiplied: everything else). The rest have a single FloatModifier
//!   (TnBuffFloatModifier: the drain, the saw blade's charge, the slime's slow, the
//!   Orbital Pistol's marks) or none at all, the weapon's script asking whether it
//!   has the upgrade (HasModification: the special shots, the Nucleon Charge Rifle's
//!   Deadeye, the Photon Burst Rifle's target info).
//! * **Its part** (StaticMeshAttachment, a WEP_WepUpgrades_p mesh) is put on the
//!   weapon's mesh at its socket (MeshAttachmentSocket: every weapon mesh has the
//!   sockets of its upgrades, on its moving parts' bones), painted in the
//!   character's colours (TnWeaponMesh.InstallMeshAttachmentModification:
//!   ApplyColorsToMesh) and, chosen in Customize, glowing in its energon colour as
//!   it goes on (ShouldHighlight: TnWeaponMesh.UpdateModificationHighlights).
//!
//! The weapon keeps its upgrades when it is dropped (TnWeapon.DropFrom keeps its
//! WeaponModifications) and gains its new owner's as he picks it up (GivenTo again).

use crate::WEAPONS;

/// What an upgrade does to its weapon, with the game's multiplayer numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Effect {
    /// Rounds added to the clip (TnBuffLargerClip: GetMaxAmmoClipCount); the reserve's
    /// most loses as many (GetMaxReserveAmmoCount takes the clip's growth off it).
    LargerClip(u32),
    /// Rounds added to the reserve's most (TnBuffReserveAmmo: GetMaxReserveAmmoCount).
    ReserveAmmo(u32),
    /// Reloads this many times as fast (TnBuffFastReload: GetReloadSpeed, which the
    /// reload's time is divided by: ModifyReloadTime).
    FastReload(f32),
    /// The spread times this (TnBuffWeaponSpread: GetCurrentSpreadModifier).
    Spread(f32),
    /// The spread's settling time times this (TnBuffWeaponSpreadCooldown: CooldownSpread).
    SpreadCooldown(f32),
    /// The fire interval times this (TnBuffWeaponFireInterval: ScaleFireInterval).
    FireInterval(f32),
    /// A ramping fire interval runs this many times as fast while the gun fires
    /// (TnBuffWeaponFireIntervalRamp: UpdateFireInterval; back at its own rate).
    FireIntervalRamp(f32),
    /// The range times this, and the damage's falloff stretched as far
    /// (TnBuffWeaponRange: GetTraceRange, GetRangeDamageModifier).
    Range(f32),
    /// Jazz's speed while it fires times this (TnBuffWeaponFiringGroundSpeed:
    /// OnBeginStateWeaponFiring's FiringGroundSpeedMultiplier).
    FiringGroundSpeed(f32),
    /// The Gear Shredder's: Jazz's speed while it charges times this
    /// (TnBuffIncreaseFineAimFootSpeed: TnWeaponSawblade.AddBlades'
    /// ChargingGroundSpeedMultiplier).
    FineAimFootSpeed(f32),
    /// The camera's kick (its pitch) times this (TnBuffReducedRecoil:
    /// GetRecoilCameraParams).
    ReducedRecoil(f32),
    /// Bursts follow one another while the trigger is held (TnBuffWeaponAutoFire:
    /// GetAutoFire), their wait times this (BurstDelaying: BurstDelay).
    AutoFire(f32),
    /// The lock-on's time times this (TnBuffWeaponLockOnTime: TnWeaponHoming.LockOnTime).
    LockOnTime(f32),
    /// Its rounds fly this many times as fast (TnBuffWeaponProjectileSpeed:
    /// TnProjectile.Init).
    ProjectileSpeed(f32),
    /// Its rounds' blast reaches this many times as far (TnBuffWeaponProjectileRadius:
    /// TnProjectile.GetDamageRadiusModifier).
    ProjectileRadius(f32),
    /// Clusters added to each burst (TnBuffWeaponNumClusters:
    /// TnClusterBombProjectileExtension.SpawnCluster).
    NumClusters(u32),
    /// The health each wounding pellet gives back times this
    /// (TnBuffDrainGunIncreaseDrain: TnWeaponDrainGun.ProcessInstantHit's HealthPerHit).
    IncreaseDrain(f32),
    /// The time between blades loading times this (TnBuffSawbladeChargeTime:
    /// TnWeaponSawblade.Charging.Tick's SecondsPerAdditionalSawBlade).
    SawChargeTime(f32),
    /// Its slow lasts this many times as long (TnBuffIncreaseAoeSlowDuration:
    /// TnBuffAoeSlow.GetBuffTime).
    SlowDuration(f32),
    /// Its marks last this many times as long (TnBuffIncreasedHardLockDuration: the
    /// marks' InstigatorBuffTimeModifier).
    HardLockDuration(f32),
    /// Its marks hurt more: each is put on at its next level (TnBuffImprovedHardLock:
    /// TnAbilityHardLock.ServerHardLock, the marks' FloatModifier[1]).
    ImprovedHardLock,
    /// The Nucleon Charge Rifle's Deadeye: a round that strikes an enemy speeds its
    /// charge, its ChargeDelay times `delay`, for `secs` (TnWeaponSniperRifle.
    /// ProcessInstantHit: TnBuffSniperRifleSpeedUpCharge, multiplayer's
    /// FloatModifier and BuffTime).
    HitsChargeFaster { delay: f32, secs: f32 },
    /// The last round of each clip is the special one (TnWeapon.FireAmmunition:
    /// _SpecialShotInClip 1, ClientInstallModification): its other fire mode
    /// (GetFiringModeForSpecialShot).
    LastShotSpecial,
    /// Each shot is the special one by the weapon's RandomShotSpecialPct
    /// (TnWeapon.FireAmmunition).
    RandomShotSpecial,
    /// The Photon Burst Rifle's target info in its scope (TnWeaponBurstRifle.
    /// ToggleWeaponSpecificHud: its WeaponSpecificHudObservers, which a team match
    /// shows only with this upgrade).
    ShowInfo,
}

/// A weapon upgrade (a TnDataProvider_WeaponUpgrade that multiplayer offers).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Upgrade {
    /// Its UniqueId (the Customize builds name it by this).
    pub id: &'static str,
    /// The weapon it is for (WeaponRestriction: main.rs's WeaponStats::id).
    pub weapon: &'static str,
    pub effect: Effect,
    /// Its part (StaticMeshAttachment: WEP_WepUpgrades_p.StatMesh.<part>) and the
    /// weapon mesh's socket it goes on (MeshAttachmentSocket), as the game has
    /// them; the demo places the parts from tools/build_upgrades.py's upgrades.txt,
    /// which the tests hold the parts to.
    #[cfg_attr(not(test), allow(dead_code))]
    pub part: &'static str,
    #[allow(dead_code)]
    pub socket: &'static str,
    /// The class level it unlocks at (RequiresLevel; none: 0). The Customize
    /// screen's lists lock it by customize.txt's, which the tests hold it to.
    #[cfg_attr(not(test), allow(dead_code))]
    pub level: u32,
}

const fn up(
    id: &'static str,
    weapon: &'static str,
    effect: Effect,
    part: &'static str,
    socket: &'static str,
    level: u32,
) -> Upgrade {
    Upgrade {
        id,
        weapon,
        effect,
        part,
        socket,
        level,
    }
}

use Effect::*;

/// Every upgrade multiplayer offers for the demo's weapons, three a weapon, with
/// their multiplayer numbers (TransWeapons.ini's buffs' FloatModifiers and
/// FloatModifier) and parts (TransGame.xxx's TnWeaponUpgrade* defaults).
pub(crate) const UPGRADES: [Upgrade; 48] = [
    // The Nucleon Charge Rifle (SniperRifle).
    up(
        "SniperRifleHitsCharge",
        "SniperRifle",
        HitsChargeFaster {
            delay: 0.3,
            secs: 5.0,
        },
        "WEP_SniperRifleHitsSpeedUpCharge_STAT",
        "SniperRifleHitsSpeedUpCharge",
        7,
    ),
    up(
        "SniperRifleFastReload",
        "SniperRifle",
        FastReload(1.5),
        "WEP_FastReload_STAT",
        "FastReload",
        0,
    ),
    up(
        "SniperRifleLargerClip",
        "SniperRifle",
        LargerClip(2),
        "WEP_LargerClip_STAT",
        "LargerClip",
        14,
    ),
    // The Orbital Pistol (TracerDart): both its mark upgrades go on one socket
    // (TnWeaponUpgradeImprovedHardLock's MeshAttachmentSocket).
    up(
        "TracerDartIncreaseDuration",
        "TracerDart",
        HardLockDuration(1.5),
        "WEP_IncreasedHardLockDuration_STAT",
        "IncreasedHardLockDuration",
        3,
    ),
    up(
        "TracerDartFireInterval",
        "TracerDart",
        FireInterval(0.75),
        "WEP_FireInterval_STAT",
        "FireInterval",
        13,
    ),
    up(
        "TracerDartIncreaseDamage",
        "TracerDart",
        ImprovedHardLock,
        "WEP_ImprovedHardLock_STAT",
        "IncreasedHardLockDuration",
        19,
    ),
    // The Corrosive Slime Cannon (AoESlow).
    up(
        "AoESlowProjectileRadius",
        "AoESlow",
        ProjectileRadius(1.15),
        "WEP_ProjectileDamageRadius_STAT",
        "ProjectileDamageRadius",
        0,
    ),
    up(
        "AoESlowFireInterval",
        "AoESlow",
        FireInterval(0.6),
        "WEP_FireInterval_STAT",
        "FireInterval",
        20,
    ),
    up(
        "AoESlowIncreaseDuration",
        "AoESlow",
        SlowDuration(2.0),
        "WEP_PuddlesLingerLonger_STAT",
        "PuddlesLingerLonger",
        24,
    ),
    // The Energon Harvester (DrainGun).
    up(
        "DrainGunRange",
        "DrainGun",
        Range(1.25),
        "WEP_Range_STAT",
        "Range",
        13,
    ),
    up(
        "DrainGunLargerClip",
        "DrainGun",
        LargerClip(4),
        "WEP_LargerClip_STAT",
        "LargerClip",
        19,
    ),
    up(
        "DrainGunIncreaseDrain",
        "DrainGun",
        IncreaseDrain(1.5),
        "WEP_DrainGunIncreaseDrain_STAT",
        "DrainGunIncreaseDrain",
        3,
    ),
    // The Neutron Assault Rifle (AssaultRifle).
    up(
        "AssaultRifleLargerClip",
        "AssaultRifle",
        LargerClip(14),
        "WEP_LargerClip_STAT",
        "LargerClip",
        2,
    ),
    up(
        "AssaultRifleAccuracy",
        "AssaultRifle",
        Spread(0.65),
        "WEP_Spread_STAT",
        "Spread",
        9,
    ),
    up(
        "AssaultRifleReduceRecoil",
        "AssaultRifle",
        ReducedRecoil(0.1),
        "WEP_ReducedRecoil_STAT",
        "ReducedRecoil",
        18,
    ),
    // The Subsonic Repeater (IonBlaster).
    up(
        "IonBlasterIncreaseAccuracy",
        "IonBlaster",
        Spread(0.65),
        "WEP_Spread_STAT",
        "Spread",
        16,
    ),
    up(
        "IonBlasterLargerClip",
        "IonBlaster",
        LargerClip(20),
        "WEP_LargerClip_STAT",
        "LargerClip",
        0,
    ),
    up(
        "IonBlasterFastReload",
        "IonBlaster",
        FastReload(1.5),
        "WEP_FastReload_STAT",
        "FastReload",
        20,
    ),
    // The Path Blaster (HeavyPistol).
    up(
        "HeavyPistolSpreadCooldown",
        "HeavyPistol",
        SpreadCooldown(0.4),
        "WEP_SpreadCooldown_STAT",
        "SpreadCooldown",
        3,
    ),
    up(
        "HeavyPistolFireInterval",
        "HeavyPistol",
        FireInterval(0.7),
        "WEP_FireInterval_STAT",
        "FireInterval",
        13,
    ),
    up(
        "HeavyPistolLastShotInClip",
        "HeavyPistol",
        LastShotSpecial,
        "WEP_LastShotInClipSpecial_STAT",
        "LastShotInClipSpecial",
        19,
    ),
    // The Photon Burst Rifle (BurstRifle).
    up(
        "BurstRifleLargerClip",
        "BurstRifle",
        LargerClip(6),
        "WEP_LargerClip_STAT",
        "LargerClip",
        7,
    ),
    up(
        "BurstRifleShowInfo",
        "BurstRifle",
        ShowInfo,
        "WEP_BurstRifleShowInfo_STAT",
        "BurstRifleShowInfo",
        14,
    ),
    up(
        "BurstRifleAutoFire",
        "BurstRifle",
        AutoFire(1.05),
        "WEP_AutoFire_STAT",
        "AutoFire",
        21,
    ),
    // The X18 Scrapmaker (HeavyMG).
    up(
        "HeavyMGIncreaseAccuracy",
        "HeavyMG",
        Spread(0.65),
        "WEP_Spread_STAT",
        "Spread",
        2,
    ),
    up(
        "HeavyMGLargerClip",
        "HeavyMG",
        LargerClip(50),
        "WEP_LargerClip_STAT",
        "LargerClip",
        9,
    ),
    up(
        "HeavyMGWindUpSpeed",
        "HeavyMG",
        FireIntervalRamp(2.5),
        "WEP_ReduceWindUpSpeed_STAT",
        "ReduceWindUpSpeed",
        18,
    ),
    // The A-4 Pulsar Cannon (GrenadeLauncher).
    up(
        "GrenadeLauncherLargerClip",
        "GrenadeLauncher",
        LargerClip(2),
        "WEP_LargerClip_STAT",
        "LargerClip",
        7,
    ),
    up(
        "GrenadeLauncherProjectileSpeed",
        "GrenadeLauncher",
        ProjectileSpeed(1.5),
        "WEP_ProjectileSpeedIncrease_STAT",
        "ProjectileSpeedIncrease",
        14,
    ),
    up(
        "GrenadeLauncherProjectileRadius",
        "GrenadeLauncher",
        ProjectileRadius(1.15),
        "WEP_ProjectileDamageRadius_STAT",
        "ProjectileDamageRadius",
        21,
    ),
    // The Riot Cannon (Bazooka).
    up(
        "BazookaFireInterval",
        "Bazooka",
        FireInterval(0.65),
        "WEP_FireInterval_STAT",
        "FireInterval",
        2,
    ),
    up(
        "BazookaLargerClip",
        "Bazooka",
        LargerClip(3),
        "WEP_LargerClip_STAT",
        "LargerClip",
        9,
    ),
    up(
        "BazookaLastShotInClip",
        "Bazooka",
        LastShotSpecial,
        "WEP_LastShotInClipSpecial_STAT",
        "LastShotInClipSpecial",
        18,
    ),
    // The E.D.K. TechVolt (ChainBeam).
    up(
        "ChainBeamLargerClip",
        "ChainBeam",
        LargerClip(30),
        "WEP_LargerClip_STAT",
        "LargerClip",
        7,
    ),
    up(
        "ChainBeamFiringGroundSpeed",
        "ChainBeam",
        FiringGroundSpeed(1.5),
        "WEP_FiringGroundSpeed_STAT",
        "FiringGroundSpeed",
        14,
    ),
    up(
        "ChainBeamRange",
        "ChainBeam",
        Range(1.15),
        "WEP_Range_STAT",
        "Range",
        21,
    ),
    // The Gear Shredder (SawBlade).
    up(
        "SawBladeChargeTime",
        "SawBlade",
        SawChargeTime(0.5),
        "WEP_SawbladeChargeTime_STAT",
        "SawbladeChargeTime",
        16,
    ),
    up(
        "SawBladeFastReload",
        "SawBlade",
        FastReload(1.5),
        "WEP_FastReload_STAT",
        "FastReload",
        20,
    ),
    up(
        "SawBladeFineAimFootSpeed",
        "SawBlade",
        FineAimFootSpeed(1.2),
        "WEP_IncreaseFineAimFootSpeed_STAT",
        "IncreaseFineAimFootSpeed",
        24,
    ),
    // The Scatter Blaster (Shotgun).
    up(
        "ShotgunLargerClip",
        "Shotgun",
        LargerClip(4),
        "WEP_LargerClip_STAT",
        "LargerClip",
        2,
    ),
    up(
        "ShotgunFireInterval",
        "Shotgun",
        FireInterval(0.75),
        "WEP_FireInterval_STAT",
        "FireInterval",
        9,
    ),
    up(
        "ShotgunRandomSpecial",
        "Shotgun",
        RandomShotSpecial,
        "WEP_RandomShotSpecial_STAT",
        "RandomShotSpecial",
        18,
    ),
    // The Thermo Rocket Cannon (HomingRocket).
    up(
        "HomingRocketReserveAmmo",
        "HomingRocket",
        ReserveAmmo(6),
        "WEP_ReserveAmmo_STAT",
        "ReserveAmmo",
        3,
    ),
    up(
        "HomingRocketFastReload",
        "HomingRocket",
        FastReload(1.5),
        "WEP_FastReload_STAT",
        "FastReload",
        13,
    ),
    up(
        "HomingRocketLockOnTime",
        "HomingRocket",
        LockOnTime(0.35),
        "WEP_WeaponLockOnTime_STAT",
        "WeaponLockOnTime",
        19,
    ),
    // The Chaos-Rift Combustor (ClusterCannon).
    up(
        "ClusterCannonFastReload",
        "ClusterCannon",
        FastReload(1.5),
        "WEP_FastReload_STAT",
        "FastReload",
        16,
    ),
    up(
        "ClusterCannonReserveAmmo",
        "ClusterCannon",
        ReserveAmmo(6),
        "WEP_ReserveAmmo_STAT",
        "ReserveAmmo",
        20,
    ),
    up(
        "ClusterCannonNumClusters",
        "ClusterCannon",
        NumClusters(2),
        "WEP_NumClusters_STAT",
        "NumClusters",
        24,
    ),
];

/// A set of installed upgrades: bit i is UPGRADES[i].
pub(crate) type Installed = u64;

/// The upgrade named `id` (an index into UPGRADES).
pub(crate) fn index(id: &str) -> Option<usize> {
    UPGRADES.iter().position(|u| u.id == id)
}

/// The upgrades of weapon `w` (an index into WEAPONS), as a set.
pub(crate) fn of_weapon(w: usize) -> Installed {
    let Some(stats) = WEAPONS.get(w) else {
        return 0;
    };
    UPGRADES
        .iter()
        .enumerate()
        .filter(|(_, u)| u.weapon == stats.id)
        .fold(0, |set, (i, _)| set | (1 << i))
}

/// The upgrades of `set` that are weapon `w`'s.
pub(crate) fn on_weapon(set: Installed, w: usize) -> impl Iterator<Item = &'static Upgrade> {
    let mine = set & of_weapon(w);
    UPGRADES
        .iter()
        .enumerate()
        .filter(move |(i, _)| mine & (1 << i) != 0)
        .map(|(_, u)| u)
}

/// What the upgrades of `set` do to weapon `w`, put together as the game's buff
/// modifiers put them together (sums for the plus kinds, products for the times
/// kinds; a weapon has one upgrade from its own build, but may carry another a
/// previous owner put on it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mods {
    pub clip: u32,
    pub reserve: u32,
    pub reload_speed: f32,
    pub spread: f32,
    pub spread_cooldown: f32,
    pub fire_interval: f32,
    pub ramp_rate: f32,
    pub range: f32,
    pub firing_speed: f32,
    pub charging_speed: f32,
    pub recoil: f32,
    /// AutoFire: the burst's wait times this (None: not automatic).
    pub auto_fire: Option<f32>,
    pub lock_on: f32,
    pub projectile_speed: f32,
    pub projectile_radius: f32,
    pub clusters: u32,
    pub drain: f32,
    pub saw_charge: f32,
    pub slow: f32,
    pub mark_secs: f32,
    pub improved_marks: bool,
    pub hits_charge: Option<(f32, f32)>,
    pub last_shot_special: bool,
    pub random_shot_special: bool,
    pub show_info: bool,
}

impl Default for Mods {
    fn default() -> Self {
        Mods {
            clip: 0,
            reserve: 0,
            reload_speed: 1.0,
            spread: 1.0,
            spread_cooldown: 1.0,
            fire_interval: 1.0,
            ramp_rate: 1.0,
            range: 1.0,
            firing_speed: 1.0,
            charging_speed: 1.0,
            recoil: 1.0,
            auto_fire: None,
            lock_on: 1.0,
            projectile_speed: 1.0,
            projectile_radius: 1.0,
            clusters: 0,
            drain: 1.0,
            saw_charge: 1.0,
            slow: 1.0,
            mark_secs: 1.0,
            improved_marks: false,
            hits_charge: None,
            last_shot_special: false,
            random_shot_special: false,
            show_info: false,
        }
    }
}

impl Mods {
    /// The upgrades of `set` on weapon `w`, put together.
    pub fn of(set: Installed, w: usize) -> Mods {
        let mut m = Mods::default();
        for u in on_weapon(set, w) {
            match u.effect {
                LargerClip(n) => m.clip += n,
                ReserveAmmo(n) => m.reserve += n,
                FastReload(k) => m.reload_speed *= k,
                Spread(k) => m.spread *= k,
                SpreadCooldown(k) => m.spread_cooldown *= k,
                FireInterval(k) => m.fire_interval *= k,
                FireIntervalRamp(k) => m.ramp_rate *= k,
                Range(k) => m.range *= k,
                FiringGroundSpeed(k) => m.firing_speed *= k,
                FineAimFootSpeed(k) => m.charging_speed *= k,
                ReducedRecoil(k) => m.recoil *= k,
                AutoFire(k) => m.auto_fire = Some(m.auto_fire.unwrap_or(1.0) * k),
                LockOnTime(k) => m.lock_on *= k,
                ProjectileSpeed(k) => m.projectile_speed *= k,
                ProjectileRadius(k) => m.projectile_radius *= k,
                NumClusters(n) => m.clusters += n,
                IncreaseDrain(k) => m.drain *= k,
                SawChargeTime(k) => m.saw_charge *= k,
                SlowDuration(k) => m.slow *= k,
                HardLockDuration(k) => m.mark_secs *= k,
                ImprovedHardLock => m.improved_marks = true,
                HitsChargeFaster { delay, secs } => m.hits_charge = Some((delay, secs)),
                LastShotSpecial => m.last_shot_special = true,
                RandomShotSpecial => m.random_shot_special = true,
                ShowInfo => m.show_info = true,
            }
        }
        m
    }

    /// A clip of `clip` rounds and a reserve of at most `reserve`, upgraded: the clip
    /// grows by its rounds, and the reserve's most by its own, less the clip's
    /// growth (TnWeapon.GetMaxReserveAmmoCount), never below nothing.
    pub fn ammo(&self, clip: u32, reserve: u32) -> (u32, u32) {
        let most = (reserve + self.reserve).saturating_sub(self.clip);
        (clip + self.clip, most)
    }
}

/// The upgrades a build's two weapon upgrades stand for (the character's
/// WeaponUpgradePairs), as a set: those of the demo's, and for its weapons.
pub(crate) fn pairs(weapons: &[String; 2], upgrades: &[String; 2]) -> Installed {
    let mut set = 0;
    for slot in 0..2 {
        if let Some(i) = index(&upgrades[slot])
            && UPGRADES[i].weapon == weapons[slot]
        {
            set |= 1 << i;
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_upgrades_a_weapon() {
        for (w, stats) in WEAPONS.iter().enumerate() {
            assert_eq!(of_weapon(w).count_ones(), 3, "{}", stats.id);
        }
        // Every upgrade is for one of the demo's weapons.
        let all = (0..WEAPONS.len()).fold(0, |s, w| s | of_weapon(w));
        assert_eq!(all.count_ones() as usize, UPGRADES.len());
    }

    #[test]
    fn larger_clip_takes_from_the_reserve() {
        // The A-4 Pulsar Cannon's Ion Cartridge: 3 + 2 a clip, 9 - 2 in reserve.
        let w = crate::weapon_index("GrenadeLauncher").unwrap();
        let set = 1 << index("GrenadeLauncherLargerClip").unwrap();
        assert_eq!(Mods::of(set, w).ammo(3, 9), (5, 7));
        // The Thermo Rocket Cannon's Amplifier Pak: 1 a clip, 5 + 6 in reserve.
        let w = crate::weapon_index("HomingRocket").unwrap();
        let set = 1 << index("HomingRocketReserveAmmo").unwrap();
        assert_eq!(Mods::of(set, w).ammo(1, 5), (1, 11));
        // The X18 Scrapmaker has no reserve to lose.
        let w = crate::weapon_index("HeavyMG").unwrap();
        let set = 1 << index("HeavyMGLargerClip").unwrap();
        assert_eq!(Mods::of(set, w).ammo(200, 0), (250, 0));
    }

    #[test]
    fn another_weapons_upgrades_do_nothing() {
        let rifle = crate::weapon_index("AssaultRifle").unwrap();
        let set = 1 << index("ShotgunLargerClip").unwrap();
        assert_eq!(Mods::of(set, rifle), Mods::default());
    }

    /// The table against what the tools exported: each upgrade's level as
    /// customize.txt lists it (tools/build_customize.py, from the game's ini; the
    /// copy built into the program) and its part as upgrades.txt places it
    /// (tools/build_upgrades.py, skipped without it).
    #[test]
    fn table_matches_the_exports() {
        let assets = crate::assets_dir();
        {
            let text = crate::customize::DATA;
            let mut seen = 0;
            for line in text.lines() {
                let f: Vec<&str> = line.split('\t').collect();
                if f.first() != Some(&"wupgrade") || f.len() < 4 {
                    continue;
                }
                if let Some(u) = UPGRADES.iter().find(|u| u.id == f[1]) {
                    assert_eq!(u.weapon, f[2], "{}'s weapon", u.id);
                    assert_eq!(u.level.to_string(), f[3], "{}'s level", u.id);
                    seen += 1;
                }
            }
            assert!(seen > 0, "no upgrade of the table in customize.txt");
        }
        if let Ok(text) = std::fs::read_to_string(assets.join("models/weapons/upgrades.txt")) {
            for line in text.lines() {
                let f: Vec<&str> = line.split('\t').collect();
                if f.first() != Some(&"part") || f.len() < 4 {
                    continue;
                }
                let u = UPGRADES
                    .iter()
                    .find(|u| u.id == f[1])
                    .expect("an upgrade of the table");
                assert_eq!(u.part, f[3], "{}'s part", u.id);
            }
        }
    }

    #[test]
    fn build_pairs() {
        let weapons = ["AssaultRifle".to_string(), "Shotgun".to_string()];
        // An upgrade for another weapon than its slot's is no pair.
        let upgrades = [
            "AssaultRifleAccuracy".to_string(),
            "BazookaLargerClip".to_string(),
        ];
        assert_eq!(
            pairs(&weapons, &upgrades),
            1 << index("AssaultRifleAccuracy").unwrap()
        );
        let none = [String::new(), String::new()];
        assert_eq!(pairs(&weapons, &none), 0);
    }
}
