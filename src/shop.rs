//! Escalation's shop, beside main.rs's health and ammo stations: the purchase points
//! (TnPickupFactoryPurchasable) that sell weapons, heavy ammo and equipment, where the
//! map has them and at the game's prices (tools/build_map.py writes them into the
//! map's points file: its "weapon", "heavyammo" and "equipment" lines).
//!
//! * **Weapon stations** (PresetType PPT_Weapon and PPT_HeavyWeapon): the weapon over
//!   the map's pickup base (part of the map), held as the game holds it
//!   (TnWeapon.CreatePickupFactoryMesh: the weapon mesh at its PickupScale, 1,
//!   aligned by AlignPickupMeshToSocket so that its Pickup socket - the gun bone
//!   itself, every gun's - is at the factory, its barrel along the factory's -X and
//!   its top up), the factory standing 2 m over its base (FixupWeaponPickup's
//!   WeaponOffset), the weapon still (FixupWeaponPickup turns a weapon's
//!   OptionalMeshRotationRate off), in its pickup look (tools/build_assets.py's
//!   PICKUP_LOOKS: the heavy weapons' outline yellow). It sells at the weapon's own
//!   Escalation price (TnPickupFactoryPurchasable.GetCost:
//!   TnCustomCharacterOptions.GetPurchaseCostOfOption, the weapon's
//!   TnDataProvider_Weapon's PurchasePriceEsc in a single-team game), not the
//!   factory's Cost, and once bought is gone for the factory's RespawnTime.
//! * **Heavy ammo stations** (PPT_HeavyAmmo, selling TnAmmoCratePickupHeavy) are
//!   main.rs's ammo stations with the heavy crate (AmmoPickupHeavy_STAT, glowing
//!   yellow), serving the heavy weapons (its ValidWeaponTypes, WT_Secondary) as the
//!   ammo crate serves the rest (WT_Primary and the vehicles' guns).
//! * **Equipment stations** (PPT_UtilityEquipment, PPT_AssaultEquipment): the
//!   equipment's pickup (its PickupFactoryMesh) a metre over the base (its
//!   OptionalOffset: 1.5 m for the Diffraction Barrier), turning at the factory's
//!   OptionalMeshRotationRate (10000 rotator units a second). Bought, the equipment
//!   is his (equipment.rs uses it), in its slot (TnEquipmentPickup.ClientGivenTo:
//!   ability 4 + its EquipmentType, offensive or defensive), in place of what was
//!   there; the station's is gone for its RespawnTime.
//!
//! In Escalation weapons go by their type (TnSurvivalGame.RespectsWeaponTypes): each
//! weapon's TnDataProvider_Weapon in TransWeapons.ini gives it WeaponType 1
//! (WT_Primary: the regular guns, their names in cyan) or 2 (WT_Secondary: the heavy
//! weapons, in yellow, and the Orbital Pistol), and a weapon picked up or bought
//! takes the place of the carried one of its type (TnPickupManager.DropRobotWeapon,
//! the weapon dropped going away: TnSurvivalGame.ShouldDestroyWeaponsOnDrop). Jazz
//! carries two weapons whatever their types (his build's two), so a weapon whose
//! type neither carries takes the place of the one in hand. A weapon he already
//! has is sold again while it is short of ammo (HmWeapon.DenyPickupQuery: refused
//! once AmmoMaxed), and fills it.

use bevy::prelude::*;
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::OnceLock;

use crate::levelfx::{FxId, FxSystems};
use crate::{MatchClock, WEAPONS, assets_dir, weapon_index};

/// The weapons' Escalation types and prices (TransWeapons.ini: each one's
/// TnDataProvider_Weapon's WeaponType for Escalation, the third, and its
/// PurchasePriceEsc), their names on the purchase line (TransContent.int ItemName,
/// coloured as the game's are: cyan for WT_Primary, yellow for WT_Secondary), and
/// the weapon class the maps' factories name (InventoryType, without TnWeapon), by
/// their ids in WEAPONS. The Orbital Pistol (TracerDart) is no factory's and has no
/// Escalation price.
const SHOP_WEAPONS: [(&str, bool, u32, &str, &str); 16] = [
    (
        "AssaultRifle",
        false,
        500,
        "<font color='#00FFFF'>Neutron Assault Rifle</font>",
        "AssaultRifle",
    ),
    (
        "IonBlaster",
        false,
        400,
        "<font color='#00FFFF'>Subsonic Repeater</font>",
        "IonBlaster",
    ),
    (
        "BurstRifle",
        false,
        500,
        "<font color='#00FFFF'>Photon Burst Rifle</font>",
        "BurstRifle",
    ),
    (
        "Shotgun",
        false,
        500,
        "<font color='#00FFFF'>Scatter Blaster</font>",
        "Shotgun",
    ),
    (
        "SniperRifle",
        false,
        700,
        "<font color='#00FFFF'>Nucleon Charge Rifle</font>",
        "SniperRifle",
    ),
    (
        "HeavyPistol",
        false,
        400,
        "<font color='#00FFFF'>Path Blaster</font>",
        "HeavyPistol",
    ),
    (
        "Bazooka",
        false,
        600,
        "<font color='#00FFFF'>Riot Cannon</font>",
        "Bazooka",
    ),
    (
        "TracerDart",
        true,
        0,
        "<font color='#FFCC00'>Orbital Pistol</font>",
        "TracerDart",
    ),
    (
        "HeavyMG",
        true,
        1500,
        "<font color='#FFCC00'>X18 Scrapmaker</font>",
        "HeavyMG",
    ),
    (
        "DrainGun",
        true,
        1000,
        "<font color='#FFCC00'>Energon Harvester</font>",
        "DrainGun",
    ),
    (
        "ChainBeam",
        false,
        750,
        "<font color='#00FFFF'>E.D.K. Techvolt</font>",
        "ChainBeam",
    ),
    (
        "AoESlow",
        true,
        1400,
        "<font color='#FFCC00'>Corrosive Slime Cannon</font>",
        "AoESlow",
    ),
    (
        "SawBlade",
        true,
        1000,
        "<font color='#FFCC00'>Gear Shredder</font>",
        "Sawblade",
    ),
    (
        "HomingRocket",
        true,
        900,
        "<font color='#FFCC00'>Thermo Rocket Cannon</font>",
        "RocketLauncher",
    ),
    (
        "GrenadeLauncher",
        true,
        1400,
        "<font color='#FFCC00'>A-4 Pulsar Cannon</font>",
        "GrenadeLauncher",
    ),
    (
        "ClusterCannon",
        true,
        1600,
        "<font color='#FFCC00'>Chaos-Rift Combustor</font>",
        "ClusterCannon",
    ),
];

fn shop_entry(
    weapon: usize,
) -> Option<&'static (&'static str, bool, u32, &'static str, &'static str)> {
    let id = WEAPONS.get(weapon)?.id;
    SHOP_WEAPONS.iter().find(|e| e.0 == id)
}

/// A heavy weapon (WT_Secondary in Escalation): its ammo is the heavy crate's, and
/// it takes the place of the other WT_Secondary one carried.
pub(crate) fn is_heavy(weapon: usize) -> bool {
    shop_entry(weapon).is_some_and(|e| e.1)
}

/// The weapon's price at a weapon station (PurchasePriceEsc).
pub(crate) fn price(weapon: usize) -> u32 {
    shop_entry(weapon).map_or(0, |e| e.2)
}

/// The weapon's name on the purchase line, in its type's colour.
pub(crate) fn purchase_name(weapon: usize) -> &'static str {
    shop_entry(weapon).map_or("", |e| e.3)
}

/// The weapon a factory sells by its InventoryType's class (without TnWeapon).
fn weapon_for_class(class: &str) -> Option<usize> {
    let id = SHOP_WEAPONS.iter().find(|e| e.4 == class)?.0;
    weapon_index(id)
}

/// Where a weapon bought (or picked up) goes among the two carried, `hand` the one
/// in hand: always the place of the one in hand, whatever the types, as the base
/// game does (the user's videos: holding the Energon Harvester and buying the Path
/// Blaster must replace the Harvester; it used to take the place of the carried
/// weapon of its own type, the stored one).
pub(crate) fn slot_for(_loadout: &[usize; 2], hand: usize, _weapon: usize) -> usize {
    hand.min(1)
}

/// The equipment the shop sells (Escalation's TnEquipmentPickups), each an ability in
/// an equipment slot (TnEquipmentPickup.AbilityType, equipment.rs).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub(crate) enum Equipment {
    /// TnEquipmentPickupMinePooper: TnKillstreakMinePooper.
    MinePayload,
    /// TnEquipmentPickupBarrier: TnAbilityBarrier.
    Barrier,
    /// TnEquipmentPickupHealthRegen: TnKillStreakHealthRegen.
    EnergonRecharger,
    /// TnEquipmentPickupFastAbilityCooldown: TnKillstreakFastAbilityCooldown.
    Intercooler,
    /// TnEquipmentPickupRollerSphereGravityAura: TnAbilityRollerSphereGravityAura.
    Decimator,
}

impl Equipment {
    pub(crate) const ALL: [Equipment; 5] = [
        Equipment::MinePayload,
        Equipment::Barrier,
        Equipment::EnergonRecharger,
        Equipment::Intercooler,
        Equipment::Decimator,
    ];

    /// The factory's InventoryType (without TnEquipmentPickup).
    fn from_class(class: &str) -> Option<Equipment> {
        Some(match class {
            "MinePooper" => Equipment::MinePayload,
            "Barrier" => Equipment::Barrier,
            "HealthRegen" => Equipment::EnergonRecharger,
            "FastAbilityCooldown" => Equipment::Intercooler,
            "RollerSphereGravityAura" => Equipment::Decimator,
            _ => return None,
        })
    }

    /// Its name on the purchase line (TransContent.int ItemName: Escalation's, with
    /// the key of its slot, <<OffensiveEquipment>> or <<DefensiveEquipment>>, marked
    /// up by the HUD).
    pub(crate) fn name(self) -> &'static str {
        match self {
            Equipment::MinePayload => "Thermo Mine Payload (<<OffensiveEquipment>> Heat-Seeking)",
            Equipment::Barrier => "Diffraction Barrier (<<DefensiveEquipment>> Shield)",
            Equipment::EnergonRecharger => {
                "Energon Recharger (<<DefensiveEquipment>> Health Regen)"
            }
            Equipment::Intercooler => "Intercooler (<<DefensiveEquipment>> Ability Speed)",
            Equipment::Decimator => "Dimensional Decimator (<<OffensiveEquipment>> Gravity Bomb)",
        }
    }

    /// Its slot: 0 offensive (Assault T.E.C.H., key 1), 1 defensive (Utility
    /// T.E.C.H., key 2): its EquipmentType (the Thermo Mine Payload's and the
    /// Dimensional Decimator's the class's 0).
    pub(crate) fn slot(self) -> usize {
        match self {
            Equipment::MinePayload | Equipment::Decimator => 0,
            _ => 1,
        }
    }

    /// Its pickup over the station (tools/build_assets.py's STATION_PROPS).
    pub(crate) fn model(self) -> &'static str {
        match self {
            Equipment::MinePayload => "models/props/pickup_mine_payload.glb",
            Equipment::Barrier => "models/props/pickup_barrier.glb",
            Equipment::EnergonRecharger => "models/props/pickup_energon_recharger.glb",
            Equipment::Intercooler => "models/props/pickup_intercooler.glb",
            Equipment::Decimator => "models/props/pickup_decimator.glb",
        }
    }

    /// How far over the factory it stands (OptionalOffset, metres).
    fn lift(self) -> f32 {
        match self {
            Equipment::Barrier | Equipment::Decimator => 1.5,
            _ => 1.0,
        }
    }

    /// Its icon in the HUD's equipment slot (updateEquipment: its ability's class
    /// with TnAbility / TnKillstreak turned into AbilityIcon).
    pub(crate) fn hud_icon(self) -> &'static str {
        match self {
            Equipment::MinePayload => "AbilityIconMinePooper",
            Equipment::Barrier => "AbilityIconBarrier",
            Equipment::EnergonRecharger => "AbilityIconHealthRegen",
            Equipment::Intercooler => "AbilityIconFastAbilityCooldown",
            Equipment::Decimator => "AbilityIconRollerSphereGravityAura",
        }
    }

    /// Its picture as the HUD shows it bought (mc_itemUnlocked's sprite 714:
    /// ItemWireframe and its UniqueId, MinePooperStreak, Barrier, HealthRegenStreak,
    /// FastAbilityCooldownStreak or RollerSphereGravityAura, less its "Streak") and
    /// its name there (its ability's FriendlyName, <TnMenuItems:Abilities>).
    pub(crate) fn wireframe(self) -> &'static str {
        match self {
            Equipment::MinePayload => "ItemWireframeMinePooper",
            Equipment::Barrier => "ItemWireframeBarrier",
            Equipment::EnergonRecharger => "ItemWireframeHealthRegen",
            Equipment::Intercooler => "ItemWireframeFastAbilityCooldown",
            Equipment::Decimator => "ItemWireframeRollerSphereGravityAura",
        }
    }

    pub(crate) fn friendly_name(self) -> &'static str {
        match self {
            Equipment::MinePayload => "Thermo Mine Payload",
            Equipment::Barrier => "Diffraction Barrier",
            Equipment::EnergonRecharger => "Energon Recharger",
            Equipment::Intercooler => "Intercooler",
            Equipment::Decimator => "Dimensional Decimator",
        }
    }

    /// Its number online (net.rs: ALL's order), and the item numbered `i`.
    pub(crate) fn index(self) -> u8 {
        Equipment::ALL.iter().position(|&e| e == self).unwrap_or(0) as u8
    }

    pub(crate) fn from_index(i: u8) -> Option<Equipment> {
        Equipment::ALL.get(i as usize).copied()
    }

    /// The sound of buying it (its PickupSound).
    pub(crate) fn pickup_sound(self) -> &'static str {
        match self {
            Equipment::MinePayload | Equipment::Barrier => "BL_TRANS_POWER.BARRIER_PU",
            Equipment::EnergonRecharger => "BL_TRANS_POWER.ENERGON_RECHARGER_PU",
            Equipment::Intercooler => "BL_TRANS_POWER.INTERCOOLER_PU",
            Equipment::Decimator => "BL_WPN_FOLEY.WPN_GRENADE_PICKUP",
        }
    }
}

/// A purchase point as the map has it: the factory's place (the floor at its centre,
/// but a weapon station's, 2 m over its base), the way it is turned (radians about
/// the up axis), its price and how long what it sells takes to come back once
/// bought.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShopSpot {
    pub spot: Vec3,
    pub yaw: f32,
    pub cost: u32,
    pub respawn: f32,
}

impl ShopSpot {
    /// A points-file line's numbers: x y z, its turn in degrees, its price and its
    /// respawn time (`cost` and `respawn` when they are missing).
    pub(crate) fn parse(v: &[f32], cost: u32, respawn: f32) -> Option<ShopSpot> {
        (v.len() >= 3).then(|| ShopSpot {
            spot: Vec3::new(v[0], v[1], v[2]),
            yaw: v.get(3).copied().unwrap_or(0.0).to_radians(),
            cost: v.get(4).map_or(cost, |&c| c.max(0.0) as u32),
            respawn: v.get(5).copied().unwrap_or(respawn).max(0.0),
        })
    }
}

/// A points-file "weapon" or "equipment" line, past its kind: its class, then the
/// spot's numbers.
pub(crate) fn parse_weapon(class: &str, v: &[f32]) -> Option<(usize, ShopSpot)> {
    Some((weapon_for_class(class)?, ShopSpot::parse(v, 0, 30.0)?))
}

pub(crate) fn parse_equipment(class: &str, v: &[f32]) -> Option<(Equipment, ShopSpot)> {
    Some((Equipment::from_class(class)?, ShopSpot::parse(v, 0, 60.0)?))
}

/// A weapon station.
#[derive(Component)]
pub(crate) struct WeaponStation {
    pub weapon: usize,
    /// The factory (2 m over its base).
    pub spot: Vec3,
    pub respawn: f32,
    /// Seconds until its weapon is back (0: there to buy).
    pub away: f32,
}

/// An equipment station.
#[derive(Component)]
pub(crate) struct EquipmentStation {
    pub kind: Equipment,
    /// The floor at its centre.
    pub spot: Vec3,
    pub cost: u32,
    pub respawn: f32,
    pub away: f32,
}

/// What a station shows while it is there to buy: its weapon (still) or its
/// equipment (turning).
#[derive(Component)]
pub(crate) struct Goods {
    spins: bool,
}

/// The equipment's turn (OptionalMeshRotationRate: 10000 rotator units a second).
const SPIN: f32 = 10000.0 / 65536.0 * TAU;

/// The Diffraction Barrier pickup's shield: its mesh's second section in its own
/// material (MP_Barrier_Distortion_MATINST: see-through, its hexagons lit round the
/// rim and a band of them sweeping up it), drawn as an effect
/// (tools/build_weapon_fx.py's MESH_SYSTEMS) turning with the pickup.
const BARRIER_SHIELD_FX: &str = "Barrier_Pickup_Shield_FX";

/// The weapon pickups' Pickup sockets (models/weapons/sockets.txt, by model name).
fn sockets() -> &'static HashMap<String, Vec3> {
    static SOCKETS: OnceLock<HashMap<String, Vec3>> = OnceLock::new();
    SOCKETS.get_or_init(|| {
        let text = std::fs::read_to_string(assets_dir().join("models/weapons/sockets.txt"))
            .unwrap_or_default();
        text.lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let mut p = l.split_whitespace();
                let name = p.next()?.to_string();
                let v: Vec<f32> = p.filter_map(|x| x.parse().ok()).collect();
                (v.len() == 3).then(|| (name, Vec3::new(v[0], v[1], v[2])))
            })
            .collect()
    })
}

/// How a weapon's pickup model sits on its station: turned so its barrel (the
/// model's -Z) lies along the factory's -X, its Pickup socket at the factory.
pub(crate) fn goods_transform(model: &str) -> Transform {
    let name = model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .trim_end_matches(".glb");
    let socket = sockets().get(name).copied().unwrap_or(Vec3::ZERO);
    let turn = Quat::from_rotation_y(FRAC_PI_2);
    Transform::from_translation(-(turn * socket)).with_rotation(turn)
}

/// The map's weapon stations, each its weapon over the base the map has there.
pub(crate) fn spawn_weapon_stations(
    commands: &mut Commands,
    asset_server: &AssetServer,
    stations: &[(usize, ShopSpot)],
) -> Vec<Entity> {
    let mut spawned = Vec::new();
    for &(weapon, s) in stations {
        let model = WEAPONS[weapon].pickup_model;
        let mut station = commands.spawn((
            WeaponStation {
                weapon,
                spot: s.spot,
                respawn: s.respawn,
                away: 0.0,
            },
            Transform::from_translation(s.spot).with_rotation(Quat::from_rotation_y(s.yaw)),
            Visibility::default(),
        ));
        if assets_dir().join(model).exists() {
            station.with_child((
                Goods { spins: false },
                SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(model))),
                goods_transform(model),
                Visibility::default(),
            ));
        }
        spawned.push(station.id());
    }
    spawned
}

/// The map's equipment stations, each its equipment over the base.
pub(crate) fn spawn_equipment_stations(
    commands: &mut Commands,
    asset_server: &AssetServer,
    stations: &[(Equipment, ShopSpot)],
) -> Vec<Entity> {
    let mut spawned = Vec::new();
    for &(kind, s) in stations {
        let mut station = commands.spawn((
            EquipmentStation {
                kind,
                spot: s.spot,
                cost: s.cost,
                respawn: s.respawn,
                away: 0.0,
            },
            Transform::from_translation(s.spot).with_rotation(Quat::from_rotation_y(s.yaw)),
            Visibility::default(),
        ));
        if assets_dir().join(kind.model()).exists() {
            station.with_child((
                Goods { spins: true },
                SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(kind.model()))),
                Transform::from_xyz(0.0, kind.lift(), 0.0),
                Visibility::default(),
            ));
        }
        spawned.push(station.id());
    }
    spawned
}

/// The stations' goods come back their respawn time after a purchase; the
/// equipment turns while it is there (the Diffraction Barrier's with its shield).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn shop_stations(
    time: Res<Time>,
    clock: Res<MatchClock>,
    mut weapons: Query<&mut WeaponStation>,
    mut equipment: Query<&mut EquipmentStation>,
    stations: Query<&Transform, Without<Goods>>,
    mut goods: Query<(Entity, &ChildOf, &Goods, &mut Transform, &mut Visibility)>,
    mut fx: ResMut<FxSystems>,
    mut shields: Local<HashMap<Entity, FxId>>,
) {
    let dt = time.delta_secs();
    for mut s in &mut weapons {
        s.away = (s.away - dt).max(0.0);
    }
    for mut s in &mut equipment {
        s.away = (s.away - dt).max(0.0);
    }
    // (The game's positive yaw turns the other way round in Bevy's axes.)
    let turn = Quat::from_rotation_y(-(time.elapsed_secs() - clock.0) * SPIN);
    let mut seen = Vec::new();
    for (entity, child_of, g, mut transform, mut visibility) in &mut goods {
        let parent = child_of.parent();
        let there = weapons
            .get(parent)
            .map(|s| s.away <= 0.0)
            .ok()
            .or_else(|| equipment.get(parent).map(|s| s.away <= 0.0).ok());
        let want = if there.unwrap_or(true) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != want {
            *visibility = want;
        }
        if g.spins && want == Visibility::Inherited {
            transform.rotation = turn;
        }
        // The barrier's shield, while the barrier is there to buy.
        let barrier = equipment
            .get(parent)
            .is_ok_and(|s| s.kind == Equipment::Barrier);
        if !barrier || want != Visibility::Inherited {
            continue;
        }
        seen.push(entity);
        let id = match shields.get(&entity) {
            Some(&id) if fx.alive(id) => Some(id),
            _ => fx.spawn(BARRIER_SHIELD_FX, Vec3::ZERO, Vec3::X, &[]),
        };
        let (Some(id), Ok(station)) = (id, stations.get(parent)) else {
            continue;
        };
        shields.insert(entity, id);
        let at = station.mul_transform(*transform);
        fx.place_turned(
            id,
            at.translation,
            at.rotation * Vec3::X,
            at.rotation * Vec3::Z,
        );
    }
    // (A shield whose barrier has been bought, or whose map has gone, goes.)
    shields.retain(|entity, id| {
        let keep = seen.contains(entity);
        if !keep {
            fx.kill(*id);
        }
        keep
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_weapon_has_its_shop_entry() {
        for (i, w) in WEAPONS.iter().enumerate() {
            assert!(shop_entry(i).is_some(), "{} has no shop entry", w.id);
        }
        // The maps' factories' classes name weapons the demo has.
        for class in [
            "Shotgun",
            "Bazooka",
            "SniperRifle",
            "IonBlaster",
            "HeavyPistol",
            "BurstRifle",
            "AssaultRifle",
            "ChainBeam",
            "HeavyMG",
            "DrainGun",
            "ClusterCannon",
            "RocketLauncher",
            "GrenadeLauncher",
            "AoESlow",
            "Sawblade",
        ] {
            assert!(weapon_for_class(class).is_some(), "{class}");
        }
    }

    #[test]
    fn a_weapon_takes_the_place_of_the_one_in_hand() {
        let rifle = weapon_index("AssaultRifle").unwrap();
        let shotgun = weapon_index("Shotgun").unwrap();
        let mg = weapon_index("HeavyMG").unwrap();
        let rocket = weapon_index("HomingRocket").unwrap();
        let harvester = weapon_index("DrainGun").unwrap();
        let path = weapon_index("HeavyPistol").unwrap();
        // Whatever the types, the one in hand goes.
        assert_eq!(slot_for(&[rifle, mg], 0, rocket), 0);
        assert_eq!(slot_for(&[rifle, shotgun], 1, mg), 1);
        assert_eq!(slot_for(&[mg, rifle], 0, shotgun), 0);
        assert_eq!(slot_for(&[harvester, mg], 0, path), 0);
        assert_eq!(slot_for(&[mg, harvester], 1, path), 1);
    }
}
