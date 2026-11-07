//! Cybertron demo: a small third-person shooter in Bevy using models extracted
//! from your own copy of Transformers: Fall of Cybertron.
//!
//! Models are loaded from `assets/models`. Anything missing is replaced by a
//! placeholder made of simple shapes, so the demo always runs.
//!
//! Controls: the game's Default keyboard layout (Settings > Controls picks another
//! or changes any key; see controls.rs).
//!   Mouse         look (click the window first to capture the mouse); boosting a
//!                 car on its wheels, left and right steer it (drive.rs)
//!   W A S D       move (robot) / strafe the hovering car (vehicle; it faces where
//!                 the mouse looks); the arrow keys too
//!   Shift         dash (tap) / sprint (hold, without dashing) (robot)
//!   Right click   fine aim (robot, hold) / boost: down onto the wheels, flat out,
//!                 steered with the mouse, the camera following it round (vehicle,
//!                 hold; or press to start and stop, with Toggle Vehicle Boost)
//!   Space         jump
//!   F             transform between robot and vehicle
//!   Left click    fire
//!   Middle click  melee
//!   R             reload
//!   C             switch the gun to the other hand (and the camera to the other
//!                 shoulder)
//!   Mouse wheel   swap between the two weapons Jazz carries (he starts with the
//!                 Neutron Assault Rifle and the Scatter Blaster)
//!   E (hold)      pick up the weapon in front of you, in place of the one in hand
//!                 (the Photon Burst Rifle and the Subsonic Repeater lie next to the
//!                 start); at a health station, buy health
//!   Esc           the pause menu
//!
//! Sound effects come from `assets/sounds` (made by `tools/build_sounds.py`); without
//! them the demo runs silent.
//!
//! The fight takes place on one of the game's Escalation maps (MAPS): Downfall
//! (`assets/models/downfall.glb` and `assets/levels/downfall_*`), Oblivion
//! (`oblivion.glb` and `oblivion_*`), Ancients (`ancients.glb` and `ancients_*`)
//! or Ignition (`ignition.glb` and `ignition_*`), all made by
//! `tools/build_map.py`, whichever the host picks in the Escalation lobby (Select
//! Map) from those that have been built. Without any, it takes place in a small
//! arena built from props.

mod abilities;
mod ai;
mod appicon;
mod arsenal;
mod barrels;
mod battlesentry;
mod car;
mod character;
mod choose;
mod clipboard;
mod cloak;
mod collision;
mod controls;
mod customize;
mod customize_ui;
mod deaths;
mod decals;
mod dof;
mod doors;
mod downed;
mod drive;
mod emp;
mod enemies;
mod energon;
mod equipment;
mod flakshield;
mod flash;
mod fx;
mod gibs;
mod gltffiles;
mod graphics;
mod healbeam;
mod hitreact;
mod hologram;
mod hoverslam;
mod hud;
mod jet;
mod killfeed;
mod levelfx;
mod lighting;
mod lightshafts;
mod live;
mod mapmotion;
mod melee;
mod menu;
mod merge;
mod mixer;
mod movefx;
mod nav;
mod net;
mod orbital;
mod outline;
mod pad;
mod padlayout;
mod pickupbase;
mod pregame;
mod ragdoll;
mod recreator;
mod revive;
mod riot;
mod robot_aim;
mod robot_cam;
mod scoreboard;
mod shadows;
mod shieldfx;
mod shop;
mod sound;
mod specials;
mod split;
mod standin;
mod traps;
mod upgrades;
mod vehicle_cam;
mod voice;
mod waves;
mod whirlwind;
mod xp;

use bevy::animation::graph::{AnimationNodeIndex, AnimationNodeType};
use bevy::audio::{AddAudioSource, AudioSinkPlayback, Volume};
use bevy::camera::visibility::VisibilityRange;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::core_pipeline::tonemapping::TonemappingLuts;
use bevy::ecs::system::SystemParam;
use bevy::gltf::{Gltf, GltfLoaderSettings};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::light::{GlobalAmbientLight, NotShadowCaster, NotShadowReceiver};
use bevy::post_process::motion_blur::MotionBlur;
use bevy::prelude::*;
use bevy::render::view::Hdr;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use collision::LevelCollision;
use controls::{Action, Bind, Controls, Inputs, Mode};
use enemies::Enemy;
use fx::{Effect, FxQueue, TracerKind};
use hud::{
    HudAlly, HudEvent, HudGrenade, HudLockOn, HudRevealed, HudState, HudWeapon, Prompt, Scope,
};
use shop::ShopSpot;
use sound::{Sfx, SoundBank};
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Settings you are most likely to change
// ---------------------------------------------------------------------------

/// Paths are relative to the `assets` folder.
const ROBOT_MODEL: &str = "models/robot.glb";
/// Every clip of robot.glb mirrored left for right, in the same order
/// (tools/mirror_anims.py), for Jazz holding his gun in his left hand.
const ROBOT_MIRROR_MODEL: &str = "models/robot_mirror.glb";
const VEHICLE_MODEL: &str = "models/vehicle.glb";
const FLOOR_MODEL: &str = "models/floor.glb";
const SKY_MODEL: &str = "models/sky.glb";
const WALL_MODEL: &str = "models/props/wall.glb";
const PISTON_MODEL: &str = "models/props/piston.glb";
const PIPES_MODEL: &str = "models/props/pipes.glb";
const VENT_MODEL: &str = "models/props/vent.glb";
const ENERGON_MODEL: &str = "models/props/energon.glb";
const HEALTH_BASE_MODEL: &str = "models/props/health_base.glb";

/// One of the game's Escalation maps, as tools/build_map.py builds them:
/// `models/<stem>.glb` (with its textures in `models/<stem>/`),
/// `levels/<stem>_col.bin` (collision), `levels/<stem>_points.txt` (spawn points),
/// and its lighting (lighting.rs).
pub struct MapInfo {
    /// Its files' name.
    pub stem: &'static str,
    /// Its name: the game's text (the map info's FriendlyName), and the English.
    pub name_key: &'static str,
    pub name: &'static str,
    /// Its picture in the Escalation lobby (in assets/menu/img).
    pub picture: &'static str,
    /// For a map without a sky of its own: where SKY_MODEL's dome stands (metres)
    /// and how it is turned (degrees).
    pub sky: Option<([f32; 3], f32)>,
    /// Escalation on it is played by the Decepticons (Downfall and Oblivion, the
    /// Decepticon maps), not the Autobots: the lobby lists the players under the
    /// Decepticons' name and colours, and their characters wear their Decepticon
    /// colours and glow purple.
    pub decepticons: bool,
}

/// The maps the lobby offers, those of them that have been built (`built_maps`).
pub const MAPS: [MapInfo; 4] = [
    // Downfall (ESC_Iacon, the game's MapId 1603): its sunset sky is part of the map.
    MapInfo {
        stem: "downfall",
        name_key: "TransGame.ESC_Iacon_BASE_m.FriendlyName",
        name: "Downfall",
        picture: "lobby_map_downfall.png",
        sky: None,
        decepticons: true,
    },
    // Oblivion (ESC_Ark, MapId 1600): its space backdrop is part of the map.
    MapInfo {
        stem: "oblivion",
        name_key: "TransGame.ESC_Ark_BASE_m.FriendlyName",
        name: "Oblivion",
        picture: "lobby_map_oblivion.png",
        sky: None,
        decepticons: true,
    },
    // Ancients (ESC_Temple, MapId 1602): its sky dome is part of the map.
    MapInfo {
        stem: "ancients",
        name_key: "TransGame.ESC_Temple_BASE_m.FriendlyName",
        name: "Ancients",
        picture: "lobby_map_ancients.png",
        sky: None,
        decepticons: false,
    },
    // Ignition (ESC_Gladiator, MapId 1601): its sky is part of the map.
    MapInfo {
        stem: "ignition",
        name_key: "TransGame.ESC_Gladiator_BASE_m.FriendlyName",
        name: "Ignition",
        picture: "lobby_map_ignition.png",
        sky: None,
        decepticons: false,
    },
];

impl MapInfo {
    fn model(&self) -> String {
        format!("models/{}.glb", self.stem)
    }

    fn collision(&self) -> String {
        format!("levels/{}_col.bin", self.stem)
    }

    fn points(&self) -> String {
        format!("levels/{}_points.txt", self.stem)
    }

    fn pods(&self) -> String {
        format!("levels/{}_pods.txt", self.stem)
    }
}

/// The maps that have been built (indices into MAPS, in its order), found once.
/// Without any, the fight is in the prop arena.
pub fn built_maps() -> &'static [usize] {
    static BUILT: std::sync::OnceLock<Vec<usize>> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| {
        let dir = assets_dir();
        (0..MAPS.len())
            .filter(|&i| {
                dir.join(MAPS[i].model()).exists() && dir.join(MAPS[i].collision()).exists()
            })
            .collect()
    })
}

/// The maps' backdrops are drawn (Oblivion's space dome, 4 km out; Downfall's and
/// Ancients' sky domes, up to 16 and 8 km out, are never culled, as the camera is
/// always inside them; Ignition's sky is within 2 km), so the camera sees that far
/// (Bevy's depth has no far plane: this only culls).
const CAMERA_FAR: f32 = 12_000.0;
/// Ledges up to this high are stepped onto instead of blocking.
const STEP_HEIGHT: f32 = 1.1;
/// Falling below this puts Jazz back where he last stood, on a map whose
/// TnWorldInfo sets no KillZ of its own (Oblivion) and in the prop arena.
const KILL_Y: f32 = -60.0;
/// While Jazz is on the ground, where he stands is noted this often; if he falls
/// out of the map he is put back where it was noted the time before last (half a
/// second to a second back along his way: not on the edge he went over).
/// `Player::safe_spot`.
const SAFE_GROUND_SECS: f32 = 0.5;
/// ... if it is no further away than this (m): else the start (he was moved there
/// some other way: a respawn, another map).
const SAFE_SPOT_RANGE: f32 = 80.0;
// Sizes are the game's own, at its scale of 1 unit = 1 cm (the map is imported
// that way). Jazz's collision cylinder (Jazz_ROBODEF's CollisionHeight 155 and
// CollisionRadius 140) is 3.1 m tall with a 1.4 m radius; the car is about 1.1 m tall.
const ROBOT_HEIGHT: f32 = 3.1;
const VEHICLE_HEIGHT: f32 = 1.15;

/// The converted models are in metres and already face forward (-Z). Jazz's robot
/// meshes are drawn at 0.6 of their size in the game (the MeshScale of his
/// RB_Jazz_MESHSET), which makes him about 3.6 m tall.
const ROBOT_SCALE: f32 = 0.6;
/// vehicle.glb is built 2.2 times its size (tools/build_assets.py, so that the
/// robot's wheel mesh fits it); the game draws the car at its own size, 3.4 m long,
/// with the 0.3 m wheels of its car physics (VEH_SHARED_p.Car_Physics).
const VEHICLE_SCALE: f32 = 1.0 / 2.2;
const MODEL_YAW_OFFSET: f32 = 0.0;

const ARENA_HALF_SIZE: f32 = 100.0;
/// Energon is Escalation's money: each player starts a match with
/// TnSurvivalSpawnList's StartingMoney, and each kill pays (enemies.rs).
const STARTING_ENERGON: u32 = 250;

/// Escalation's health stations (TnPickupFactoryPurchasable with PresetType
/// PPT_Health, selling TnHealthPickup), where the map has them: a pickup base
/// (ESC_PickupBase_Health_STAT) under a hovering energon cube (TnHealthPickup's
/// HealthPickup_FX: PROP_NEU_EnergonCube_STAT at 0.6 times its size, a metre up,
/// tumbling, and turning at the factory's OptionalMeshRotationRate). Standing at one
/// (in its touch cylinder: 7.68 m round, 4 m high, with Jazz's own) with health or
/// shield to make up shows "Hold E to purchase Health for *200"
/// (ContextualCommands.PickupPurchasable); holding E for PICKUP_HOLD_SECS buys it
/// for HEALTH_COST energon, healing him fully (AddedHealth 1000, healed base first,
/// then shield: TnHealTypeHealthPickup). The cube is then gone for
/// HEALTH_RESPAWN_SECS (RespawnTime). Without enough energon, the purchase is
/// refused with the game's error beep (TnPickupManager's InsufficientFundsSound).
const HEALTH_COST: u32 = 200;
const HEALTH_RESPAWN_SECS: f32 = 40.0;
/// The touch cylinder of the game's pickup factories (TnPickupFactory's
/// CollisionCylinder: 7.68 m round, 4 m high), the health and ammo stations'.
const STATION_TOUCH_RADIUS: f32 = 7.68;
const STATION_TOUCH_HEIGHT: f32 = 4.0;
const HEALTH_CUBE_LIFT: f32 = 1.0;
const HEALTH_CUBE_SCALE: f32 = 0.6;
/// The cube's turn (10000 rotator units a second, in radians) and tumble (a tenth of
/// a turn a second about two axes).
const HEALTH_CUBE_SPIN: f32 = 10000.0 / 65536.0 * TAU;
const HEALTH_CUBE_TUMBLE: f32 = 0.1 * TAU;
/// The station's name on the command line (TnHealthPickup's ItemName).
const HEALTH_ITEM_NAME: &str = "Health";
const SND_HEALTH: &str = "BL_HUD_INTERFACE.HEALTH_PU_ENERGON";
const SND_NO_FUNDS: &str = "BL_LVL_MP.ESC_INSUFFICIENT_FUNDS";

/// Escalation's ammo stations (TnPickupFactoryPurchasable with PresetType PPT_Ammo,
/// selling TnAmmoCratePickup), where the map has them (their bases are part of the
/// map): the ammo crate (AmmoPickupLight_STAT) a metre over the base (its PickupMesh's
/// 100-unit offset), turning at the factory's OptionalMeshRotationRate (10000 rotator
/// units a second). Standing in its touch cylinder while one of Jazz's guns is short
/// of a full reserve (TnAmmoCratePickup.PickupAllowed) shows "Hold E to purchase Ammo
/// for *100" (its ItemName and the station's Cost); holding E for PICKUP_HOLD_SECS
/// buys it (fill_ammo), with the crate's pickup sound. The crate is then gone for
/// the station's RespawnTime (10 s on Downfall, 20 s on Oblivion, Ancients and
/// Ignition).
/// Without enough energon, the purchase is refused with the error beep, as at the
/// health stations.
const AMMO_CRATE_MODEL: &str = "models/props/ammo_crate.glb";
/// The heavy ammo stations' crate (shop.rs): AmmoPickupHeavy_STAT, glowing yellow
/// (HeavyAmmoPickup_MATINST's Emissive_Color and OutlineColor, 1, 0.396, 0), its
/// purchase sound HEALTH_PU_AMMO_HEAVY (TnAmmoCratePickupHeavy's PickupSound) and
/// its name Heavy Ammo.
const HEAVY_AMMO_CRATE_MODEL: &str = "models/props/ammo_crate_heavy.glb";
const HEAVY_AMMO_ITEM_NAME: &str = "Heavy Ammo";
const SND_HEAVY_AMMO: &str = "BL_HUD_INTERFACE.HEALTH_PU_AMMO_HEAVY";
const AMMO_CRATE_LIFT: f32 = 1.0;
const AMMO_CRATE_SPIN: f32 = 10000.0 / 65536.0 * TAU;
const AMMO_ITEM_NAME: &str = "Ammo";
const SND_AMMO: &str = "BL_HUD_INTERFACE.HEALTH_PU_AMMO";

const GRAVITY: f32 = -35.0;
const ROBOT_SPEED: f32 = 12.0;
const ROBOT_SPRINT: f32 = 20.0;
/// Jazz jumps 6.5 m high (JumpHeight 650 in his acrobatics,
/// TR_Acrobatics_p.Bumblebee_JazzAcrobatics); the jump launches him at the speed
/// that reaches it against GRAVITY.
const JUMP_HEIGHT: f32 = 6.5;
const ROBOT_RADIUS: f32 = 1.4;
/// Jazz's car moves as the game's TnCarForm does in Escalation (Car_Physics,
/// HoverCar_Physics). It hovers (car::HOVER_*): W A S D strafe it relative to
/// where the camera looks, up to 15 m/s, and it faces that way. Holding Speed Boost
/// (right click) drops it onto its wheels and drives it flat out, the mouse
/// steering it and the camera following it round (drive.rs, vehicle_cam.rs).
/// Fastest the player can go (a car dash or a fall carried along).
const VEHICLE_TOP_SPEED: f32 = 45.0;
/// Jazz's car is about 3.4 m long and 1.9 m wide; its wheels are the game's 30 cm
/// (WheelRadius in Car_Physics).
const VEHICLE_RADIUS: f32 = 1.2;
/// Against walls a vehicle is its real shape, not a circle: vehicle.glb's body runs
/// from 1.43 m behind its origin to 2.01 m ahead and is 1.86 m wide (each
/// character's vehicle has its own: Kit::body), so it is covered by three
/// overlapping circles of its half width along its length (body_circles).
const VEHICLE_HALF_WIDTH: f32 = 0.93;

/// The circles covering a vehicle's body: their offsets ahead of its origin (its
/// front and back ones just inside its ends, one in its middle; metres).
fn body_circles(body: character::Body) -> [f32; 3] {
    [
        body.ahead - body.half_width,
        (body.ahead - body.behind) * 0.5,
        body.half_width - body.behind,
    ]
}
const WHEEL_RADIUS: f32 = 0.3;

/// Where shots leave Jazz's gun when his model isn't loaded (model space).
const ROBOT_MUZZLE: Vec3 = Vec3::new(1.04, 2.07, -1.05);
/// The car's guns (WEP_AssaultRifleVH_SKEL on its two weapon sockets) when folded
/// out: each has two barrels (sockets MuzzleFlash1 and MuzzleFlash2), fired in turn
/// (TnWeaponEffectIncrementMuzzleSocket). Car space, from tools/build_assets.py's
/// vehicle.glb with the guns out.
pub(crate) const VEHICLE_MUZZLES: [Vec3; 4] = [
    Vec3::new(-0.71, 0.86, -1.04),
    Vec3::new(0.72, 0.86, -1.04),
    Vec3::new(-0.88, 0.92, -0.87),
    Vec3::new(0.89, 0.92, -0.87),
];

/// Jazz's robot-mode weapons. Each is an arm mesh inside robot.glb (the game swaps
/// Jazz's right forearm for the weapon). Fire interval and damage are the game's own
/// values from the weapons' TnWeaponData (WEP_AssaultRifleArm_p, WEP_IonBlaster_p,
/// and all_shared's BurstRifle_SV_WEPDATA for the Photon Burst Rifle).
struct WeaponStats {
    /// The game's id for it (TransWeapons.ini: the Customize screen's builds name
    /// their weapons by it).
    id: &'static str,
    /// Mesh node name in robot.glb, and its left-handed version (on his left arm).
    mesh: &'static str,
    mesh_left: &'static str,
    /// Seconds between shots (the game's FireIntervals).
    fire_interval: f32,
    /// Damage per hit (the game's InstantHitDamage).
    damage: f32,
    /// How far a shot reaches, in metres (the game's Range). Shots are instant
    /// hits, as in the game; the tracer's speed is part of its effect (fx.rs).
    range: f32,
    /// Distance from Jazz's hand to the end of the barrel.
    muzzle_reach: f32,
    /// The weapon's sound cues (its TnWeaponEffectSound in the game): letting go of
    /// the trigger, switching to and away from it, and the impacts (the shots are
    /// in `gun`).
    sound_end: &'static str,
    sound_equip: &'static str,
    sound_holster: &'static str,
    sound_impact_world: &'static str,
    sound_impact_enemy: &'static str,
    /// Raising and lowering the sights (fine aim).
    sound_aim_start: &'static str,
    sound_aim_end: &'static str,
    /// The gun's additive animations (ANIM_*): looped while the trigger is held
    /// (arm recoil plus the gun's moving parts), looped while it is out, and played
    /// once when the trigger is let go.
    fire_anim: usize,
    idle_anim: usize,
    cooldown_anim: Option<(usize, f32)>,
    /// Firing effects (the weapon's muzzle flash, tracer and impact particle systems).
    muzzle_fx: Effect,
    tracer: TracerKind,
    impact_fx: Effect,
    /// Ammo, reloading, spread and the low-ammo shot sound (`Gun`).
    gun: Gun,
    /// The reload's additive animation (ANIM_*) and its length in seconds; it is
    /// stretched to the gun's reload time.
    reload_anim: (usize, f32),
    /// Switching to and from it (TnWeaponEventEquip / UnEquip, additive): the gun
    /// unfolding out of Jazz's forearm on the R_CloudA_* gun bones, and folding back
    /// in on the R_CloudB_* ones, where its second mesh (`<mesh>_CloudB`) is skinned.
    equip_anim: usize,
    unequip_anim: usize,
    /// Switching hands (TnWeaponEventChangeHandedness): its sound.
    sound_change_hands: &'static str,
    /// Shown by the HUD: the weapon's name and its ItemWireframe icon.
    hud_label: &'static str,
    hud_icon: &'static str,
    /// Its reticle in the HUD movie (the game picks it by weapon class), "" for none.
    hud_crosshair: &'static str,
    /// Semi-automatic burst fire (None: fires for as long as the trigger is held).
    burst: Option<Burst>,
    /// Horizontal field of view while fine aiming (the over-the-shoulder camera's
    /// TnFovCameraBehavior: 50 degrees, 30 for the Burst Rifle's 5x scope, 25 for
    /// the Nucleon Charge Rifle's).
    fine_aim_fov: f32,
    /// The scope fine aim brings up on the HUD (the tactical one's range finder and
    /// compass, the sniper one's charge).
    scope: Scope,
    /// Fine aim takes the camera in close behind Jazz's head (his class's
    /// character::Camera::scope): the scoped guns', the Photon Burst Rifle and the
    /// Nucleon Charge Rifle.
    scope_camera: bool,
    /// Jazz's stance while fine aiming (TnWeaponData FineAimStance, as `stance`
    /// numbers them): 2, the fine-aim stance, walking; 1 for the Nucleon Charge
    /// Rifle, which he holds from the hip as he jogs.
    fine_aim_stance: usize,
    /// A charging weapon's charge (None for the others).
    charge: Option<Charge>,
    /// Pulling the trigger with no ammo at all (its NoAmmoFire sound).
    sound_dry: Option<&'static str>,
    /// The weapon lying on the ground (tools/build_assets.py, models/weapons).
    pickup_model: &'static str,
    /// A shotgun: every shot fires a pellet along each of these offsets (the game's
    /// SpreadPattern: to the right and up, against the aim direction's length,
    /// scaled by the spread's size against its starting size). None: one round,
    /// thrown off at random within the spread.
    pellets: &'static [Vec2],
    /// Damage by distance (the game's RangeDamageScale): a multiplier at a number of
    /// metres, joined by straight lines and held past either end. None: full damage
    /// at any range.
    falloff: &'static [(f32, f32)],
    /// A one-off clip the gun plays with every shot, started over by each (ANIM_*,
    /// and its length in seconds), instead of looping `fire_anim` while the trigger
    /// is held.
    shot_anim: Option<(usize, f32)>,
    /// The camera's kick with every shot, from the hip and in fine aim (None: the
    /// game's is zero). The car's guns have none either.
    recoil: Option<(CameraRecoil, CameraRecoil)>,
    /// The camera's shake with every shot, from the hip and in fine aim alike
    /// (TnWeaponData CameraShakes; the car's guns' is VEHICLE_SHAKE).
    shake: Option<CameraShake>,
    /// Its muzzles and shell port on Jazz's elbow bone (the weapon mesh's sockets).
    /// None: shots leave `muzzle_reach` out from his hand towards the aim point.
    sockets: Option<GunSockets>,
    /// Its spent shells.
    shell: GunShell,
    /// The steam that rises from it as it reloads (its reload's
    /// TnWeaponEffectPlayFXOnSocket effects, on GunSockets::steam), if any.
    reload_steam: Option<Effect>,
    /// A weapon whose rounds fly (WeaponFireTypes EWFT_Projectile: the Riot
    /// Cannon's, riot.rs) rather than strike at once: its rounds. Its tracer,
    /// impacts and their sounds then go unused, and `damage` is its rounds'.
    projectile: Option<&'static riot::RoundKind>,
    /// The game's own particle systems its shots play in place of the demo's muzzle
    /// flash (its light is still `muzzle_fx`'s), `tracer` and `impact_fx`, and the
    /// one it plays while it is ready (orbital.rs: the Orbital Pistol's). None: the
    /// demo's own.
    level_fx: Option<&'static orbital::ShotFx>,
    /// The game's own muzzle flash its shots play on the barrel that fired (kept
    /// there while it plays, at the robot's size: arsenal.rs's
    /// play_flash_on_socket), and on its other flash socket if it has one
    /// (GunSockets::flash), in place of the demo's flash for `muzzle_fx`, whose light
    /// it keeps: the Neutron Assault Rifle's (fx/weapons_fx.txt's
    /// NeutronRifle_MuzzleFlash_FX), the Subsonic Repeater's, the Photon Burst
    /// Rifle's, the Scatter Blaster's, the Nucleon Charge Rifle's and the Path
    /// Blaster's (their TnWeaponEffectPlayMuzzleFX). None: `muzzle_fx` whole (or
    /// `level_fx`'s).
    muzzle_system: Option<&'static str>,
    /// Its rounds mark the sentries they strike (orbital.rs's Mark: the Orbital
    /// Pistol's), and its falloff goes by the distance the game measures, from
    /// Jazz's centre to the sentry's (TnWeapon.ModifyDamage: the other guns measure
    /// from the muzzle to where the round struck).
    marks: bool,
    /// Switching hands (TnWeaponEventChangeHandedness): the gun's MirrorReverse clip
    /// (ANIM_*, additive).
    change_hands_anim: usize,
    /// How fast Jazz moves with it, times his speed: in his hand (its
    /// GroundSpeedMultiplier, set as it is equipped: TnWeapon.WeaponEquipped) and
    /// while it is firing (FiringGroundSpeedMultiplier, for its WeaponFiring state:
    /// TnWeapon.OnBeginStateWeaponFiring; TnPawn multiplies them).
    ground_speed: f32,
    firing_speed: f32,
    /// A heavy weapon (BaseStance and FineAimStance STANCE_HeavyGunArmShootFromHip):
    /// shooting and fine aiming, Jazz stands, walks and jogs in the heavy-gun stance
    /// (ANIM_HEAVY_*).
    heavy: bool,
    /// Its fire interval eases from `fire_interval` to another as it fires.
    ramp: Option<FireRamp>,
    /// What the fine-aim button does with it.
    alt: AltFire,
    /// Seconds from taking it out to its first shot (EquipTime).
    equip_secs: f32,
    /// What the Titan's, Scientist's and Destroyer's guns do beyond the others
    /// (arsenal.rs); None for the eight before them.
    gear: Option<&'static arsenal::Gear>,
}

/// A fire interval that eases from the gun's `fire_interval` to `end` over `secs`
/// of firing (FireIntervals' IntervalRange over its RampTime: an HmValueLerper,
/// in a straight line; TnWeapon.UpdateFireInterval runs it on while the gun is in
/// its firing state and back at the same rate out of it), and a gun that only warms
/// up while its interval is over `warm_until` (the X18 Scrapmaker's
/// TnWeaponDataExtensionHeavyMG: MinFireIntervalToStartFiring; TnWeaponHeavyMG.
/// FireAmmunition sends StartCharging and fires no round).
#[derive(Clone, Copy)]
struct FireRamp {
    end: f32,
    secs: f32,
    warm_until: f32,
}

/// What the fine-aim button does with a weapon: TnPlayerController.PlayerWalking.
/// FineAim asks the weapon first (TryAltFirePress), and fine aims only if it says no.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AltFire {
    /// Fine aim (TnWeapon.TryAltFirePress: false).
    FineAim,
    /// Shows where its round would go while held (TnWeaponAoESlow's
    /// TnSimulateProjectileExtension: arsenal.rs's trajectories).
    Trajectory,
    /// Its press sets off the launcher's rounds (TnWeaponGrenadeLauncherBase.
    /// TryAltFirePress: AttemptToDetonate; arsenal.rs's Detonate).
    Detonate,
    /// Its press, while the weapon is ready, starts showing the lob (TnWeaponTank-
    /// CannonArcable.TryAltFirePress: _IsAiming), until it is let go (arsenal.rs's Arc).
    Arc,
}

/// A gun's spent shells: its shell effect on ShellEject with every shot
/// (TnWeaponEffectPlayFXOnSocket), whose casing comes out for `chance` of them and
/// whose steam rises from the shell port for `steam` of them (the SpawnProbability
/// of its Shell and SmokeSteam emitters).
#[derive(Clone, Copy)]
struct GunShell {
    /// Effect::Shell or Effect::MedShell.
    effect: Effect,
    chance: f32,
    steam: f32,
}

/// Where a gun's barrels end and its shells come out (its weapon mesh's sockets,
/// on Jazz's R_ or L_Arm03_Elbow_XB): positions in the elbow bone's own space in
/// robot.glb, in metres before Jazz's ROBOT_SCALE (the game's centimetres, its
/// socket's y and z swapped as umodel turns them), for the gun on his right arm
/// and on his left.
#[derive(Clone, Copy)]
struct GunSockets {
    /// Its barrels (MuzzleFlash, and MuzzleFlash2 if it has two): each shot's
    /// flash, tracer and rounds leave from the next in turn
    /// (TnWeaponEffectIncrementMuzzleSocket).
    muzzles: &'static [[Vec3; 2]],
    /// ShellEject: spent shells and their steam.
    shell: [Vec3; 2],
    /// Where the steam comes out as it reloads (at its own size, not its socket's:
    /// levelfx's spawn_on_socket).
    steam: [Vec3; 2],
    /// Another flash that plays with every shot (TnWeaponEffectPlayFXOnSocket): its
    /// effect and socket.
    flash: Option<(Effect, [Vec3; 2])>,
}

/// The Neutron Assault Rifle's (RB_AssaultRifleR_SKEL and RB_AssaultRifleL_SKEL):
/// its two barrels, half a metre apart at Jazz's size, and its shell port behind
/// his wrist, which steams as it reloads.
const RIFLE_SOCKETS: GunSockets = GunSockets {
    muzzles: &[
        [Vec3::new(-2.65, 0.15, 0.05), Vec3::new(2.65, -0.15, 0.05)],
        [Vec3::new(-2.50, -0.25, -0.65), Vec3::new(2.50, 0.25, 0.65)],
    ],
    shell: [Vec3::new(-0.452, 0.0, 0.0), Vec3::new(0.452, 0.0, 0.0)],
    steam: [Vec3::new(-0.452, 0.0, 0.0), Vec3::new(0.452, 0.0, 0.0)],
    flash: None,
};

/// The Scatter Blaster's (RB_ScatterblasterR_SKEL and RB_ScatterblasterL_SKEL): its
/// one barrel, its shell port on top halfway along, and its ReloadFX socket by his
/// wrist (on the left-handed gun, its own spot rather than the mirror of the
/// right's).
const SCATTER_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[Vec3::new(-2.50, 0.10, -0.15), Vec3::new(2.50, -0.10, 0.15)]],
    shell: [Vec3::new(-1.32, 0.0, 0.0), Vec3::new(1.32, 0.0, 0.0)],
    steam: [
        Vec3::new(-0.474, -0.061, -0.036),
        Vec3::new(0.269, 0.0, 0.215),
    ],
    flash: None,
};

/// The Subsonic Repeater's (RB_IonBlasterR_SKEL and RB_IonBlasterL_SKEL): every
/// round leaves its upper barrel (MuzzleFlash: it has no
/// TnWeaponEffectIncrementMuzzleSocket), while the flash plays at its lower one too
/// (MuzzleFlash2); its shell port under the drum, and its ReloadFX socket, which is
/// on a bone of its moving parts (R_ and L_CloudA_ElbowRobo01_XT), where that lies
/// in Jazz's pose here (robot.glb's).
const SUBSONIC_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[Vec3::new(-2.00, -0.10, -0.15), Vec3::new(2.00, 0.10, 0.15)]],
    shell: [
        Vec3::new(-0.996, -0.934, -0.175),
        Vec3::new(0.996, 0.934, 0.175),
    ],
    steam: [
        Vec3::new(0.53, -0.48, -0.114),
        Vec3::new(-0.104, 0.509, 0.192),
    ],
    flash: Some((
        Effect::SubsonicFlash,
        [Vec3::new(-2.00, -0.50, -0.15), Vec3::new(2.00, 0.50, 0.15)],
    )),
};

/// The Photon Burst Rifle's (RB_BurstRifleR_SKEL and RB_BurstRifleL_SKEL): its one
/// barrel 295 units down the forearm (MuzzleFlash; on the left-handed gun at the
/// same height, not the mirror of the right's), its shell port behind his wrist
/// (ShellEject, at the same spot on both guns' elbows) and its ReloadFX socket on
/// the elbow, under the gun.
const PHOTON_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[Vec3::new(-2.95, -0.20, 0.0), Vec3::new(2.95, -0.20, 0.0)]],
    shell: [Vec3::new(0.45, 0.0, 0.0), Vec3::new(0.45, 0.0, 0.0)],
    steam: [
        Vec3::new(-0.6836, 0.0557, -0.5557),
        Vec3::new(0.7287, 0.0345, 0.6269),
    ],
    flash: None,
};

/// The Nucleon Charge Rifle's (RB_SniperRifleR_SKEL and RB_SniperRifleL_SKEL): its
/// one barrel 445 units down the forearm, its shell port (ShellSocket, at the same
/// spot on both guns' elbows, so on the left-handed one not the mirror of the
/// right's), and its ReloadFX socket on a bone of its moving parts
/// (R_ and L_CloudA_ElbowRobo11_XT), where that lies in Jazz's pose here.
const NUCLEON_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[Vec3::new(-4.45, -0.15, 0.0), Vec3::new(4.45, 0.15, 0.0)]],
    shell: [Vec3::new(0.55, -0.25, 0.60), Vec3::new(0.55, -0.25, 0.60)],
    steam: [
        Vec3::new(-1.248, -0.256, -0.413),
        Vec3::new(1.27, 0.256, 0.413),
    ],
    flash: None,
};

/// The Path Blaster's (RB_HeavyPistolR_SKEL and RB_HeavyPistolL_SKEL): its one
/// barrel 401 units down the forearm (MuzzleFlash, its only muzzle socket; on the
/// left-handed gun 409 units down, its own spot rather than the mirror of the
/// right's), its shell port behind his wrist (ShellEject, as the rifle's) and its
/// ReloadFX socket on the elbow, under the gun.
const PATH_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[
        Vec3::new(-4.0117, 0.1536, -0.10),
        Vec3::new(4.0901, -0.3235, 0.10),
    ]],
    shell: [Vec3::new(-0.452, 0.0, 0.0), Vec3::new(0.452, 0.0, 0.0)],
    steam: [
        Vec3::new(-1.416, -0.2855, -0.1791),
        Vec3::new(1.416, 0.2855, 0.1791),
    ],
    flash: None,
};

/// The Riot Cannon's (RB_BazookaR_SKEL and RB_BazookaL_SKEL): its one barrel 250
/// units down the forearm (MuzzleFlash), and its ReloadFX socket under the gun by
/// the elbow (on the left-handed gun, its own spot rather than the mirror of the
/// right's). It throws no shells (nor has a shell port: ShellEject is the elbow).
const RIOT_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[Vec3::new(-2.50, -0.35, 0.0), Vec3::new(2.50, 0.35, 0.0)]],
    shell: [Vec3::ZERO, Vec3::ZERO],
    steam: [
        Vec3::new(-0.3897, -0.8618, 0.2617),
        Vec3::new(0.2792, 0.6928, 0.2617),
    ],
    flash: None,
};

/// The Orbital Pistol's (RB_SysCorruptorR_SKEL and RB_SysCorruptorL_SKEL): its one
/// barrel 341 units down the forearm (MuzzleFlash; on the left-handed gun 334 units
/// down, its own spot rather than the mirror of the right's). It throws no shells:
/// its ShellEject socket, under the gun, lets off its steam while it is ready
/// (orbital.rs). It steams as it reloads from its Pickup socket, the elbow itself
/// (the left-handed gun has none: the same spot).
const ORBITAL_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[
        Vec3::new(-3.4073, -0.1003, -0.1878),
        Vec3::new(3.3402, -0.0028, 0.2367),
    ]],
    shell: [
        Vec3::new(-0.3190, -0.9507, -0.8874),
        Vec3::new(0.2913, 0.9579, 0.9360),
    ],
    steam: [Vec3::ZERO, Vec3::ZERO],
    flash: None,
};

/// The X18 Scrapmaker's (RB_HeavyMGR_SKEL and RB_HeavyMGL_SKEL): its two barrels,
/// 320 and 365 units down the forearm (MuzzleFlash and MuzzleFlash2: every round
/// leaves the next in turn, TnWeaponEffectIncrementMuzzleSocket), and its shell
/// port under the drum (ShellEject). It has no ReloadFX socket.
const SCRAPMAKER_SOCKETS: GunSockets = GunSockets {
    muzzles: &[
        [Vec3::new(-3.20, 0.35, -0.15), Vec3::new(3.20, -0.35, 0.15)],
        [Vec3::new(-3.65, -0.55, -0.15), Vec3::new(3.65, 0.55, 0.15)],
    ],
    shell: [
        Vec3::new(-0.9695, -0.4064, -0.5752),
        Vec3::new(0.9695, 0.4064, 0.5752),
    ],
    steam: [Vec3::ZERO, Vec3::ZERO],
    flash: None,
};

/// The Energon Harvester's (RB_EmpShotgunR_SKEL and RB_EmpShotgunL_SKEL: its one
/// barrel, MuzzleFlash, 228 units down the forearm and out to the side; on the
/// left-handed gun its own spot rather than the mirror of the right's), and its
/// ShellEject socket on the elbow, where it steams as it reloads.
const HARVESTER_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[
        Vec3::new(-2.2809, -0.0524, -0.5817),
        Vec3::new(2.235, 0.0181, -0.2389),
    ]],
    shell: [Vec3::new(-1.32, 0.0, 0.0), Vec3::new(1.32, 0.0, 0.0)],
    steam: [Vec3::new(-1.32, 0.0, 0.0), Vec3::new(1.32, 0.0, 0.0)],
    flash: None,
};

/// The E.D.K. TechVolt's (RB_RepairBeamR_SKEL and RB_RepairBeamL_SKEL): its emitter,
/// 150 units down the forearm (MuzzleFlash, and MuzzleFlash2 in the same spot), its
/// ShellEject socket, and its ReloadFX socket under the gun.
const TECHVOLT_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[
        Vec3::new(-1.4978, -0.1522, -0.1416),
        Vec3::new(1.4978, 0.1522, 0.1416),
    ]],
    shell: [Vec3::new(-0.4522, 0.0, 0.0), Vec3::new(0.4522, 0.0, 0.0)],
    steam: [
        Vec3::new(-1.1169, 0.0, -1.2412),
        Vec3::new(1.1169, 0.0, 1.1882),
    ],
    flash: None,
};

/// The Corrosive Slime Cannon's (RB_RustCannonR_SKEL and RB_RustCannonL_SKEL): its
/// barrel, 250 units down the forearm (MuzzleFlash), and its ReloadFX socket by the
/// elbow. It throws no shells.
const SLIME_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[Vec3::new(-2.50, 0.0, 0.0), Vec3::new(2.50, 0.0, 0.0)]],
    shell: [Vec3::ZERO, Vec3::ZERO],
    steam: [
        Vec3::new(-0.9979, 0.0, -0.1006),
        Vec3::new(1.0463, 0.0, -0.1036),
    ],
    flash: None,
};

/// The Gear Shredder's (RB_SawbladeR_SKEL and RB_SawbladeL_SKEL): its three muzzles
/// in fire-mode order (SawBlade_WEPMESH's MuzzleSockets: MuzzleFlashCenter for the
/// first blade, MuzzleFlashRight for the second, MuzzleFlashLeft for the third, 200
/// and 275 units down the forearm; on the left-handed gun its own spots rather than
/// the mirror of the right's), each blade's round leaving its own (arsenal.rs's
/// Blades), and its ShellSocket, which steams as it reloads (arsenal.rs). It throws
/// no shells.
const SHREDDER_SOCKETS: GunSockets = GunSockets {
    muzzles: &[
        [Vec3::new(-2.00, 0.70, 0.0), Vec3::new(2.00, 0.15, 0.0)],
        [Vec3::new(-2.75, 0.65, -0.55), Vec3::new(2.75, 0.20, 0.55)],
        [Vec3::new(-2.75, 0.65, 0.55), Vec3::new(2.75, 0.20, -0.55)],
    ],
    shell: [
        Vec3::new(-0.7306, 0.0, -0.1005),
        Vec3::new(0.7306, 0.0, 0.1005),
    ],
    steam: [Vec3::ZERO, Vec3::ZERO],
    flash: None,
};

/// The Thermo Rocket Cannon's (RB_RocketLauncherR_SKEL and RB_RocketLauncherL_SKEL):
/// its MuzzleFlash, 315 units down the forearm; its ShellEject (it throws no shells;
/// its exhaust and steam are arsenal.rs's).
const THERMO_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[Vec3::new(-3.15, -0.35, 0.0), Vec3::new(3.15, 0.35, 0.0)]],
    shell: [
        Vec3::new(0.8560, -0.2211, -0.3043),
        Vec3::new(0.9550, -0.6819, 0.4698),
    ],
    steam: [Vec3::ZERO, Vec3::ZERO],
    flash: None,
};

/// The A-4 Pulsar Cannon's (RB_GrenadeLauncherR_SKEL and RB_GrenadeLauncherL_SKEL):
/// its MuzzleFlash, 220 units down the forearm; its ShellEject (it throws no shells).
const PULSAR_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[Vec3::new(-2.20, 0.0, -0.65), Vec3::new(2.20, 0.0, 0.65)]],
    shell: [
        Vec3::new(-0.2238, -0.5433, -0.7600),
        Vec3::new(0.4462, 0.4941, 0.8949),
    ],
    steam: [Vec3::ZERO, Vec3::ZERO],
    flash: None,
};

/// The Chaos-Rift Combustor's (RB_GrenadeArmR_SKEL and RB_GrenadeArmL_SKEL, the
/// "GrenadeArm"): its MuzzleFlash, 125 units down the forearm, and its ShellEject
/// (its shells are the game's Lrg_Shell1_FX: arsenal.rs).
const COMBUSTOR_SOCKETS: GunSockets = GunSockets {
    muzzles: &[[
        Vec3::new(-1.2514, 0.0, -0.0943),
        Vec3::new(1.1909, 0.0, 0.1576),
    ]],
    shell: [
        Vec3::new(-0.2238, -0.5433, -0.7600),
        Vec3::new(0.4522, 0.4941, 0.8949),
    ],
    steam: [Vec3::ZERO, Vec3::ZERO],
    flash: None,
};

/// The X18 Scrapmaker's shots (HeavyMG_WEPMESH): X18_Scrapmaker_MuzzleFlash_FX on
/// the barrel it fired from (arsenal.rs keeps it there), Scrapmaker_Tracer_FX with
/// every round and X18_Scrapmaker_Squib_FX where it strikes. (Its MuzzleFlash and
/// MuzzleFlash2 sockets are at a tenth, RelativeScale 0.1, which its flash doesn't
/// take: levelfx's spawn_on_socket.)
const SCRAPMAKER_FX: orbital::ShotFx = orbital::ShotFx {
    muzzle: "X18_Scrapmaker_MuzzleFlash_FX",
    tracer: "Scrapmaker_Tracer_FX",
    tracer_at: orbital::TracerAt::EveryShot,
    impact: "X18_Scrapmaker_Squib_FX",
    idle: None,
};

/// The Energon Harvester's (DrainGun_WEPMESH): EMPShotgun_MuzzleFlash_FX with every
/// shot, EMPShotgun_Tracer_FX once a burst (StartFire), EMPShotgun_Squib_FX where
/// each pellet strikes, and while it is ready the arcs round its muzzle
/// (EMPShotgun_Idle_FX on MuzzleFlash, from ActiveStart to ActiveStop).
const HARVESTER_FX: orbital::ShotFx = orbital::ShotFx {
    muzzle: "EMPShotgun_MuzzleFlash_FX",
    tracer: "EMPShotgun_Tracer_FX",
    tracer_at: orbital::TracerAt::StartFire,
    impact: "EMPShotgun_Squib_FX",
    idle: Some(orbital::IdleFx {
        system: "EMPShotgun_Idle_FX",
        socket: [
            (Vec3::new(-2.2809, -0.0524, -0.5817), [0, 32768, 0]),
            (Vec3::new(2.235, 0.0181, -0.2389), [32768, 32768, 0]),
        ],
    }),
};

/// The E.D.K. TechVolt's (ChainBeam_WEPMESH): its beam, flash and all
/// (TechvoltEmitter_MuzzleFlashAndTracer_FX, held from StartFire to StopFire on
/// the muzzle: arsenal.rs), TechvoltEmitter_Squib_FX where each round strikes, and
/// while it is ready the arcs on its emitter (TechvoltEmitter_Idle_FX on
/// MuzzleFlash).
const TECHVOLT_FX: orbital::ShotFx = orbital::ShotFx {
    muzzle: "",
    tracer: "TechvoltEmitter_MuzzleFlashAndTracer_FX",
    tracer_at: orbital::TracerAt::Held,
    impact: "TechvoltEmitter_Squib_FX",
    idle: Some(orbital::IdleFx {
        system: "TechvoltEmitter_Idle_FX",
        socket: [
            (Vec3::new(-1.4978, -0.1522, -0.1416), [0, 32768, 0]),
            (Vec3::new(1.4978, 0.1522, 0.1416), [0, 0, 0]),
        ],
    }),
};

/// The Corrosive Slime Cannon's (AoeSlow_WEPMESH): while it is ready, the dark
/// steam off it (SlimeCannon_Idle_FX on MeleeSocket, the elbow; its flash, flight
/// and blast are its round's, riot.rs).
const SLIME_FX: orbital::ShotFx = orbital::ShotFx {
    muzzle: "",
    tracer: "",
    tracer_at: orbital::TracerAt::EveryShot,
    impact: "",
    idle: Some(orbital::IdleFx {
        system: "SlimeCannon_Idle_FX",
        // (Its MeleeSocket is at 0.7 on both guns, which the steam doesn't take.)
        socket: [(Vec3::ZERO, [0, 32768, 0]), (Vec3::ZERO, [0, 0, 0])],
    }),
};

/// The A-4 Pulsar Cannon's (GrenadeLauncher_WEPMESH): while it is ready, the glow on
/// its IdleFXSocket (Pulsar_Idle_FX, from ActiveStart to ActiveStop; its flash,
/// flight and blast are its round's, riot.rs): its ball of plasma 50 units along the
/// effect's -X, which the sockets' turns point down the barrel on either arm, so it
/// sits on the gun's front. (The sockets' RelativeScale, -0.7, -0.7, 0.7 on the
/// right gun and 0.7, 0.7, -0.7 on the left, is none of the glow's: levelfx's
/// spawn_on_socket.)
const PULSAR_FX: orbital::ShotFx = orbital::ShotFx {
    muzzle: "",
    tracer: "",
    tracer_at: orbital::TracerAt::EveryShot,
    impact: "",
    idle: Some(orbital::IdleFx {
        system: "Pulsar_Idle_FX",
        socket: [
            (Vec3::new(-1.2937, -0.1103, -0.1089), [0, 0, 0]),
            (Vec3::new(1.2937, 0.1103, 0.1089), [32768, 0, 0]),
        ],
    }),
};

/// A gun's camera shake (TnWeaponData CameraShakes, played by the camera's
/// TnShakeCameraBehavior and its TnCameraShaker). The camera's shake waves
/// (ShakeWaves) run all the time; a shot sets them playing at `intensity`, fading
/// out over `secs` (ShakeFalloffDuration, which the shaker counts down a frame at a
/// time from the shot's frame on), and rolls the view by `roll` degrees, fading by
/// the square of what is left of `roll_secs`. At 60 frames a second that is 0.7, 0.4
/// and 0.1 of the shake on the shot's frame and the two after it, as the game plays
/// it (Shaker).
#[derive(Clone, Copy, Debug, PartialEq)]
struct CameraShake {
    intensity: f32,
    roll: f32,
    roll_secs: f32,
    secs: f32,
}

impl CameraShake {
    /// How strongly the shake plays `since` seconds after the shot: the scale of
    /// the waves and the roll (degrees, clockwise). The shaker counts its falloff
    /// down before it plays a frame, so on the shot's frame `since` is that frame's
    /// length.
    fn strength(&self, since: f32) -> Vec2 {
        let left = |secs: f32| {
            if secs > 0.0 {
                (1.0 - since / secs).max(0.0)
            } else {
                0.0
            }
        };
        let r = left(self.roll_secs);
        Vec2::new(
            self.intensity * left(self.secs),
            self.intensity * self.roll * r * r,
        )
    }
}

/// A car's or truck's jump's or landing's shake (its form's JumpingShakeParams or
/// LandingShakeParams: Intensity, Roll, RollFalloffDuration, ShakeFalloffDuration;
/// character::Drive's).
fn vehicle_shake([intensity, roll, roll_secs, secs]: [f32; 4]) -> CameraShake {
    CameraShake {
        intensity,
        roll,
        roll_secs,
        secs,
    }
}

/// A camera's shake waves (its TnShakeCameraBehavior's YawSpectrum and
/// PitchSpectrum, played by HmMultiFrequencyWaveGenerator): amplitude in degrees
/// and frequency (a second) of the yaw and of the pitch.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ShakeWaves {
    yaw: (f32, f32),
    pitch: (f32, f32),
}

impl ShakeWaves {
    /// The view's turn at the shaker's clock `time` (pitch and yaw in degrees, up
    /// and right) at full strength.
    fn at(&self, time: f32) -> Vec3 {
        let wave = |(amplitude, hz): (f32, f32)| amplitude * (time * hz * TAU).sin();
        Vec3::new(wave(self.pitch), wave(self.yaw), 0.0)
    }
}

/// On foot: Jazz's over-the-shoulder camera, CAM_Strategies_p.Car_OverTheShoulder_
/// STRATEGY (his Car_Jazz_CAMSET's, as for the car, tank, truck and plane classes'
/// robots): yaw 0.75 degrees at 1.5 a second, pitch 2 degrees at 4 a second. (The
/// plain OverTheShoulder_STRATEGY, with 3 at 5 and 5 at 4, only plays as he
/// transforms back into a robot.) The game's videos show these: a Subsonic
/// Repeater's shot turns the view by up to 0.35 degrees on its frame.
const ROBOT_SHAKE_WAVES: ShakeWaves = ShakeWaves {
    yaw: (0.75, 1.5),
    pitch: (2.0, 4.0),
};
/// In the car: Car_Jazz_STRATEGY's and HoverCar_Jazz_STRATEGY's, yaw 3 degrees at
/// 5 a second, pitch 5 degrees at 4 a second.
const VEHICLE_SHAKE_WAVES: ShakeWaves = ShakeWaves {
    yaw: (3.0, 5.0),
    pitch: (5.0, 4.0),
};
/// The shaker's clock wraps round every 2 s, a whole number of every wave above.
const SHAKE_CLOCK_SECS: f32 = 2.0;
/// The game's frame at 60 frames a second, over which a shot's shake comes in here
/// when frames are shorter (Shaker::step).
const SHAKE_FRAME: f32 = 1.0 / 60.0;

/// The camera's shaker (TnCameraShaker): the last shot's shake, the seconds since
/// its shot (counted as CameraShake::strength takes them), the strength that was
/// playing when it came, and this frame's.
#[derive(Clone, Copy, Debug, Default)]
struct Shaker {
    shake: Option<CameraShake>,
    since: f32,
    from: Vec2,
    now: Vec2,
}

impl Shaker {
    /// A frame of `dt` seconds, `shot` being the shake of a shot this frame: it takes
    /// over from the one playing, as the game's shaker restarts its falloff. At 60
    /// frames a second or fewer the strength is the game's, frame for frame; with
    /// shorter frames a shot's shake comes in over the game's first frame
    /// (SHAKE_FRAME) from what was playing, rather than all at once.
    fn step(&mut self, shot: Option<CameraShake>, dt: f32) -> Vec2 {
        if let Some(shake) = shot {
            self.shake = Some(shake);
            self.since = 0.0;
            self.from = self.now;
        }
        self.since = (self.since + dt).min(60.0);
        self.now = match self.shake {
            None => Vec2::ZERO,
            Some(s) if self.since < SHAKE_FRAME => self
                .from
                .lerp(s.strength(SHAKE_FRAME), self.since / SHAKE_FRAME),
            Some(s) => s.strength(self.since),
        };
        self.now
    }
}

/// A round's blast's camera shake playing (riot.rs's ScreenShake, as
/// HmCameraModifierScreenShake plays it: UE3's UpdateScreenShake): its time left and
/// where each of its waves has got to (radians: rotation pitch, yaw and roll;
/// location forward, right and up; the field of view).
#[derive(Clone, Debug)]
struct PlayingShake {
    shake: riot::ScreenShake,
    left: f32,
    rot: Vec3,
    loc: Vec3,
    fov: f32,
}

impl PlayingShake {
    /// It starts (InitializeShake): each wave from a place at random
    /// (ESP_OffsetRandom: FRand * 2 pi), drawn from `seed`.
    fn new(shake: riot::ScreenShake, seed: &mut u32) -> PlayingShake {
        let mut next = || {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 17;
            *seed ^= *seed << 5;
            (*seed >> 8) as f32 / (1u32 << 24) as f32 * TAU
        };
        PlayingShake {
            shake,
            left: shake.secs,
            rot: Vec3::new(next(), next(), next()),
            loc: Vec3::new(next(), next(), next()),
            fov: next(),
        }
    }

    /// A frame of `dt`: its view's turn (pitch up, yaw right and roll clockwise, in
    /// degrees), its place's offset (forward, right and up, in metres) and its change
    /// to the field of view (degrees); None once it is over. Its strength eases out
    /// over its time (smoothstep of the time left over its TimeDuration), each wave
    /// moving on at its frequency times that.
    fn step(&mut self, dt: f32) -> Option<(Vec3, Vec3, f32)> {
        self.left -= dt;
        if self.left <= 0.0 {
            return None;
        }
        let s = &self.shake;
        let pct = (self.left / s.secs.max(1e-4)).clamp(0.0, 1.0);
        let pct = pct * pct * (3.0 - 2.0 * pct);
        let mut loc = Vec3::ZERO;
        let mut rot = Vec3::ZERO;
        for i in 0..3 {
            if s.loc_amp[i] != 0.0 {
                self.loc[i] += dt * s.loc_freq[i] * pct;
                loc[i] = s.loc_amp[i] * self.loc[i].sin() * pct;
            }
            if s.rot_amp[i] != 0.0 {
                self.rot[i] += dt * s.rot_freq[i] * pct;
                rot[i] = s.rot_amp[i] * self.rot[i].sin() * pct;
            }
        }
        let mut fov = 0.0;
        if s.fov_amp != 0.0 {
            self.fov += dt * s.fov_freq * pct;
            fov = pct * s.fov_amp * self.fov.sin();
        }
        Some((rot * (360.0 / 65536.0), loc * 0.01, fov))
    }
}

/// How the camera kicks when the gun fires (TnWeaponData CameraRecoils, played by
/// TnRecoilCameraBehavior): every shot pitches the view up by `pitch` degrees for
/// good (the player brings it back down), and changes the field of view by `fov`
/// degrees (negative widens it), which eases back (smoothstep) over `secs`, `delay`
/// seconds after the shot. The game turns the view in one frame; here both ease
/// in (camera_follow, RECOIL_EASE).
#[derive(Clone, Copy, Debug, PartialEq)]
struct CameraRecoil {
    pitch: f32,
    fov: f32,
    secs: f32,
    delay: f32,
}

impl CameraRecoil {
    /// How much of the field-of-view change is left `since` seconds after the shot.
    fn fov_left(&self, since: f32) -> f32 {
        if self.secs <= 0.0 {
            return 0.0;
        }
        let t = ((since - self.delay) / self.secs).clamp(0.0, 1.0);
        1.0 - t * t * (3.0 - 2.0 * t)
    }
}

/// A burst-fire weapon (AutoFire false): each pull of the trigger fires `shots`
/// rounds `fire_interval` apart, then waits `delay` (BurstDelay) before the next.
/// `anim` is the additive clip holding a whole burst's recoil and its length. Its
/// shot sound plays once a burst (a whole burst's recording, on StartFire), or with
/// every round when `sound_each` (on FireShot: the Energon Harvester's).
#[derive(Clone, Copy)]
struct Burst {
    shots: u32,
    delay: f32,
    anim: (usize, f32),
    sound_each: bool,
}

/// The Nucleon Charge Rifle's charge (TnWeaponSniperRifle, its
/// TnSniperRifleExtension): while Jazz fine aims and the rifle is ready, it charges
/// up a level every `delay` seconds (ChargeDelay), as far as its last `damage`
/// (InstantHitDamage, by charge level: the shot's fire mode) while it has a round
/// in the clip. It starts over as fine aim starts or ends, and as the rifle comes
/// back to the ready after a shot, a reload or a swap (its Active.BeginState's
/// ResetFireMode). Its scope holds three looping cues on (weapon_loops): `scope`
/// while it is up (FineAimBegin to FineAimEnd), `charging` while it charges
/// (StartCharged to StopCharged) and `charged` once it is fully charged
/// (StartFullyCharged to StopFullyCharged).
#[derive(Clone, Copy)]
struct Charge {
    delay: f32,
    damage: &'static [f32],
    sound_scope: &'static str,
    sound_charging: &'static str,
    sound_charged: &'static str,
}

impl Charge {
    /// The highest level it can charge to with `clip` rounds in the clip
    /// (GetMaxChargeLevel: every level's shot costs one round).
    fn max_level(&self, clip: u32) -> usize {
        if clip >= 1 {
            self.damage.len().saturating_sub(1)
        } else {
            0
        }
    }

    /// The level reached after `timer` seconds of charging (GetChargeLevel).
    fn level(&self, timer: f32, clip: u32) -> usize {
        let max = self.max_level(clip);
        (0..=max)
            .rev()
            .find(|&l| timer >= l as f32 * self.delay)
            .unwrap_or(0)
    }

    /// How far it has charged, 0..1 (GetNormalizedChargeLevel; the HUD shows it).
    fn fraction(&self, timer: f32, clip: u32) -> f32 {
        let max = self.max_level(clip);
        if max == 0 {
            0.0
        } else {
            (timer / (max as f32 * self.delay)).clamp(0.0, 1.0)
        }
    }
}

/// What the HUD and the ammo rules need of a gun (the game's TnWeaponData and
/// TnWeaponEffectLowAmmoSound): clip and reserve sizes, reload time, the shot
/// sound swapped in when the clip runs low, and spread.
#[derive(Clone, Copy)]
struct Gun {
    clip_size: u32,
    reserve_max: u32,
    reload_secs: f32,
    sound_reload: &'static str,
    sound_shot: &'static str,
    sound_shot_low: &'static str,
    /// LowAmmoThreshold: the low-ammo shot plays with this many rounds or fewer left.
    low_ammo: u32,
    spread: Spread,
}

/// A gun's Spreads: it starts at `min` and opens by `per_shot` with every shot up
/// to `max`, all the while settling back at the rate that takes it from `max` to
/// `min` in `cooldown` seconds; fine aim multiplies it by `fine_aim`
/// (FineAimSpreadModifier). Shots leave up to spread/2 off the aim.
#[derive(Clone, Copy)]
struct Spread {
    min: f32,
    max: f32,
    per_shot: f32,
    cooldown: f32,
    fine_aim: f32,
}

const WEAPONS: [WeaponStats; 16] = [
    WeaponStats {
        id: "AssaultRifle",
        mesh: "RB_AssaultRifleR_SKEL",
        mesh_left: "RB_AssaultRifleL_SKEL",
        fire_interval: 0.10, // 600 rounds per minute
        damage: 18.0,
        range: 300.0,
        muzzle_reach: 1.38,
        sound_end: "BL_WPN_GUN_NEUTRON_RIFLE.GUNARM_SHOOT_END",
        sound_equip: "BL_WPN_GUN_NEUTRON_RIFLE.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_NEUTRON_RIFLE.ANIM_HOLSTER",
        sound_impact_world: "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_WORLD",
        sound_impact_enemy: "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_DMG_AI",
        sound_aim_start: "BL_WPN_GUN_NEUTRON_RIFLE.FINE_AIM_START",
        sound_aim_end: "BL_WPN_GUN_NEUTRON_RIFLE.FINE_AIM_END",
        fire_anim: ANIM_RIFLE_FIRE,
        idle_anim: ANIM_RIFLE_IDLE,
        cooldown_anim: Some((ANIM_RIFLE_COOLDOWN, RIFLE_COOLDOWN_SECS)),
        muzzle_fx: Effect::RifleMuzzle,
        tracer: TracerKind::Bullet,
        impact_fx: Effect::BulletImpact,
        gun: Gun {
            clip_size: 36,
            reserve_max: 216,
            reload_secs: 1.5,
            sound_reload: "BL_WPN_GUN_NEUTRON_RIFLE.ANIM_RELOAD",
            sound_shot: "BL_WPN_GUN_NEUTRON_RIFLE.GUNARM_SHOOT",
            sound_shot_low: "BL_WPN_GUN_NEUTRON_RIFLE.GUNARM_SHOOT_LOWAMMO",
            low_ammo: 9,
            spread: Spread {
                min: 0.05,
                max: 0.10,
                per_shot: 0.005,
                cooldown: 2.0,
                fine_aim: 0.5,
            },
        },
        reload_anim: (ANIM_RIFLE_RELOAD, 61.0 / 30.0),
        equip_anim: ANIM_EQUIP + 0,
        unequip_anim: ANIM_EQUIP + 1,
        sound_change_hands: "BL_WPN_GUN_NEUTRON_RIFLE.ANIM_CHANGE_HANDS",
        hud_label: "Neutron Assault Rifle",
        hud_icon: "ItemWireframeAssaultRifle",
        hud_crosshair: "mc_crosshairRifle",
        burst: None,
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        fine_aim_stance: 2,
        charge: None,
        sound_dry: None,
        pickup_model: "models/weapons/assault_rifle.glb",
        pellets: &[],
        // RangeDamageScale (DistanceScaler_11452): 1.15x to 25 m, down to 0.25x at
        // 150 m, 0.25x out to the end of its range.
        falloff: &[(25.0, 1.15), (150.0, 0.25)],
        shot_anim: None,
        // AssaultRifleArm_SV_WEPDATA: 0.4 degrees a shot from the hip; in fine aim
        // 0.1, with the field of view a degree wider for a moment.
        recoil: Some((
            CameraRecoil {
                pitch: 0.4,
                fov: 0.0,
                secs: 0.3,
                delay: 0.1,
            },
            CameraRecoil {
                pitch: 0.1,
                fov: -1.0,
                secs: 0.3,
                delay: 0.1,
            },
        )),
        // Its CameraShakes: intensity 0.2, falling off over 0.05 s.
        shake: Some(CameraShake {
            intensity: 0.2,
            roll: 0.05,
            roll_secs: 0.05,
            secs: 0.05,
        }),
        sockets: Some(RIFLE_SOCKETS),
        // Sm_Shell3_FX: a shell for one shot in four, steam for one in five.
        shell: GunShell {
            effect: Effect::Shell,
            chance: 0.25,
            steam: 0.2,
        },
        reload_steam: Some(Effect::ReloadSteam),
        projectile: None,
        level_fx: None,
        // AssaultRifleArm_WEPMESH's TnWeaponEffectPlayMuzzleFX: its flame cap, black
        // smoke cap, spark bolts, haze and glow (RIFLE_MUZZLE is its light).
        muzzle_system: Some("NeutronRifle_MuzzleFlash_FX"),
        marks: false,
        change_hands_anim: ANIM_HAND_SWITCH,
        ground_speed: 1.0,
        firing_speed: 1.0,
        heavy: false,
        ramp: None,
        alt: AltFire::FineAim,
        equip_secs: EQUIP_SECS,
        gear: None,
    },
    // The Subsonic Repeater (TnWeaponIonBlaster, the game's "IonBlaster" inside:
    // WEP_IonBlaster_p, IonBlaster_SP_WEPDATA, whose ammo, 60 and 300, the game's
    // Escalation uses): the fastest-firing gun, 12-damage instant hits every 0.065 s,
    // worth 1.35 times that up close and falling off to a quarter by 100 m.
    WeaponStats {
        id: "IonBlaster",
        mesh: "RB_IonBlasterR_SKEL",
        mesh_left: "RB_IonBlasterL_SKEL",
        fire_interval: 0.065,
        damage: 12.0,
        range: 150.0,
        // Its MuzzleFlash socket is 200 units down the forearm (SUBSONIC_SOCKETS).
        muzzle_reach: 0.96,
        sound_end: "BL_WPN_GUN_ION_BLASTER.SHOOT_TAIL",
        sound_equip: "BL_WPN_GUN_ION_BLASTER.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_ION_BLASTER.ANIM_HOLSTER",
        sound_impact_world: "BL_WPN_GUN__IMPT_BYS.IMPT_ENERGY_WORLD",
        sound_impact_enemy: "BL_WPN_GUN__IMPT_BYS.IMPT_ENERGY_DMG_AI",
        sound_aim_start: "BL_WPN_GUN_ION_BLASTER.FINE_AIM_START",
        sound_aim_end: "BL_WPN_GUN_ION_BLASTER.FINE_AIM_END",
        fire_anim: ANIM_ION_FIRE,
        idle_anim: ANIM_ION_IDLE,
        cooldown_anim: None,
        muzzle_fx: Effect::SubsonicMuzzle,
        tracer: TracerKind::Subsonic,
        impact_fx: Effect::SubsonicImpact,
        gun: Gun {
            clip_size: 60,
            reserve_max: 300,
            reload_secs: 1.25,
            sound_reload: "BL_WPN_GUN_ION_BLASTER.ANIM_RELOAD_01",
            sound_shot: "BL_WPN_GUN_ION_BLASTER.SHOOT",
            sound_shot_low: "BL_WPN_GUN_ION_BLASTER.SHOOT_LOW_AMMO",
            low_ammo: 5,
            spread: Spread {
                min: 0.11,
                max: 0.11,
                per_shot: 0.0,
                cooldown: 0.0,
                fine_aim: 0.65,
            },
        },
        reload_anim: (ANIM_ION_RELOAD, 38.0 / 30.0),
        equip_anim: ANIM_EQUIP + 2,
        unequip_anim: ANIM_EQUIP + 3,
        sound_change_hands: "BL_WPN_GUN_ION_BLASTER.ANIM_CHANGE_HANDS",
        hud_label: "Subsonic Repeater",
        hud_icon: "ItemWireframeIonBlaster",
        hud_crosshair: "mc_crosshairRifle",
        burst: None,
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        fine_aim_stance: 2,
        charge: None,
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_ELECTRICITY"),
        pickup_model: "models/weapons/ion_blaster.glb",
        pellets: &[],
        // DistanceScaler_10471: 1.35x to 20 m, down to 0.25x at 100 m, 0.25x out to
        // the end of its range.
        falloff: &[(20.0, 1.35), (100.0, 0.25)],
        shot_anim: None,
        // (IonBlaster_SP_WEPDATA's camera recoil is all zero: no kick.)
        recoil: None,
        // IonBlaster_SP_WEPDATA's CameraShakes: intensity 0.25, falling off over 0.055 s.
        shake: Some(CameraShake {
            intensity: 0.25,
            roll: 0.2,
            roll_secs: 0.045,
            secs: 0.055,
        }),
        sockets: Some(SUBSONIC_SOCKETS),
        // Sm_Shell3_FX, as the rifle's: a shell for one shot in four, steam for one
        // in five.
        shell: GunShell {
            effect: Effect::Shell,
            chance: 0.25,
            steam: 0.2,
        },
        // ReloadSteam_FX on its ReloadFX socket.
        reload_steam: Some(Effect::SubsonicReloadSteam),
        projectile: None,
        level_fx: None,
        // IonBlaster_WEPMESH's TnWeaponEffectPlayMuzzleFX, and again on its
        // MuzzleFlash2 socket (SUBSONIC_SOCKETS' flash): SUBSONIC_MUZZLE is its light.
        muzzle_system: Some("Subsonic_Repeater_MuzzleFlash_FX"),
        marks: false,
        change_hands_anim: ANIM_HAND_SWITCH + 1,
        // IonBlaster_SP_WEPDATA's GroundSpeedMultiplier: "faster footspeed when
        // equipped".
        ground_speed: 1.05,
        firing_speed: 1.0,
        heavy: false,
        ramp: None,
        alt: AltFire::FineAim,
        equip_secs: EQUIP_SECS,
        gear: None,
    },
    // The Photon Burst Rifle (TnWeaponBurstRifle, BurstRifle_SV_WEPDATA): three-round
    // bursts of 32-damage instant hits, a 5x scope.
    WeaponStats {
        id: "BurstRifle",
        mesh: "RB_BurstRifleR_SKEL",
        mesh_left: "RB_BurstRifleL_SKEL",
        fire_interval: 0.1,
        damage: 32.0,
        range: 300.0,
        // Its MuzzleFlash socket is 295 units down the forearm (the rifle's 265,
        // the Subsonic Repeater's 200).
        muzzle_reach: 1.57,
        // StartFire plays the whole burst's sound, StopFire its tail.
        sound_end: "BL_WPN_GUN_PHOTON_ELIMINATOR.SHOOT_TAIL",
        sound_equip: "BL_WPN_GUN_PHOTON_ELIMINATOR.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_PHOTON_ELIMINATOR.ANIM_EQUIP",
        sound_impact_world: "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_WORLD",
        sound_impact_enemy: "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_DMG_AI",
        sound_aim_start: "BL_WPN_GUN_PHOTON_ELIMINATOR.FINE_AIM_START",
        sound_aim_end: "BL_WPN_GUN_PHOTON_ELIMINATOR.FINE_AIM_END",
        fire_anim: ANIM_BURST_FIRE,
        idle_anim: ANIM_BURST_IDLE,
        cooldown_anim: None,
        muzzle_fx: Effect::PhotonMuzzle,
        tracer: TracerKind::Photon,
        impact_fx: Effect::PhotonImpact,
        gun: Gun {
            clip_size: 15,
            reserve_max: 105,
            reload_secs: 2.0,
            sound_reload: "BL_WPN_GUN_PHOTON_ELIMINATOR.ANIM_RELOAD",
            // One sound per burst, and no low-ammo version.
            sound_shot: "BL_WPN_GUN_PHOTON_ELIMINATOR.SHOOT",
            sound_shot_low: "BL_WPN_GUN_PHOTON_ELIMINATOR.SHOOT",
            low_ammo: 0,
            spread: Spread {
                min: 0.06,
                max: 0.12,
                per_shot: 0.01,
                cooldown: 2.0,
                fine_aim: 0.25,
            },
        },
        reload_anim: (ANIM_BURST_RELOAD, 75.0 / 30.0),
        equip_anim: ANIM_EQUIP + 4,
        unequip_anim: ANIM_EQUIP + 5,
        sound_change_hands: "BL_WPN_GUN_PHOTON_ELIMINATOR.ANIM_CHANGE_HANDEDNESS",
        hud_label: "Photon Burst Rifle",
        hud_icon: "ItemWireframeBurstRifle",
        hud_crosshair: "mc_crosshairTactical",
        burst: Some(Burst {
            shots: 3,
            delay: 0.325,
            anim: (ANIM_BURST_FIRE, 26.0 / 30.0),
            sound_each: false,
        }),
        fine_aim_fov: 30.0,
        scope: Scope::Tactical,
        // (Scoped, the view goes in behind his head and leaves him out of it, as
        // the Nucleon Charge Rifle's does.)
        scope_camera: true,
        fine_aim_stance: 2,
        charge: None,
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_BALLISTIC"),
        pickup_model: "models/weapons/burst_rifle.glb",
        pellets: &[],
        falloff: &[],
        shot_anim: None,
        // (BurstRifle_SV_WEPDATA's camera recoil is all zero: no kick.)
        recoil: None,
        // BurstRifle_SV_WEPDATA's: intensity 0.35 with every round, over 0.125 s.
        shake: Some(CameraShake {
            intensity: 0.35,
            roll: 0.15,
            roll_secs: 0.1,
            secs: 0.125,
        }),
        sockets: Some(PHOTON_SOCKETS),
        shell: GunShell {
            effect: Effect::Shell,
            chance: 1.0,
            steam: 0.0,
        },
        reload_steam: None,
        projectile: None,
        level_fx: None,
        // (BurstRifle_WEPMESH's TnWeaponEffectPlayMuzzleFX: PHOTON_MUZZLE is its light.)
        muzzle_system: Some("Photon_BurstRifle_MuzzleFlash_Fx"),
        marks: false,
        change_hands_anim: ANIM_HAND_SWITCH + 2,
        ground_speed: 1.0,
        firing_speed: 1.0,
        heavy: false,
        ramp: None,
        alt: AltFire::FineAim,
        equip_secs: EQUIP_SECS,
        gear: None,
    },
    // The Scatter Blaster (TnWeaponShotgun, all_shared's Shotgun_SV_WEPDATA): an
    // automatic shotgun, seven 12-damage pellets a shot in a fixed pattern, worth
    // double up close and falling off to a quarter by 50 m.
    WeaponStats {
        id: "Shotgun",
        mesh: "RB_ScatterblasterR_SKEL",
        mesh_left: "RB_ScatterblasterL_SKEL",
        fire_interval: 0.425,
        damage: 12.0,
        range: 65.0,
        // Its MuzzleFlash socket is 250 units down the forearm (SCATTER_SOCKETS).
        muzzle_reach: 1.29,
        sound_end: "BL_WPN_GUN_SCATTER_BLASTER.AUTO_SHOTGUN_SHOOT_TAIL",
        sound_equip: "BL_WPN_GUN_SCATTER_BLASTER.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_SCATTER_BLASTER.ANIM_HOLSTER",
        sound_impact_world: "BL_WPN_GUN__IMPT_BYS.IMPT_SHOTGUN_WORLD",
        sound_impact_enemy: "BL_WPN_GUN__IMPT_BYS.IMPT_SHOTGUN_DMG_AI",
        sound_aim_start: "BL_WPN_GUN_SHOTGUN.FINE_AIM_START",
        sound_aim_end: "BL_WPN_GUN_SHOTGUN.FINE_AIM_END",
        fire_anim: ANIM_SCATTER_FIRE,
        idle_anim: ANIM_SCATTER_IDLE,
        cooldown_anim: None,
        muzzle_fx: Effect::ScatterMuzzle,
        tracer: TracerKind::Scatter,
        impact_fx: Effect::ScatterImpact,
        gun: Gun {
            clip_size: 8,
            reserve_max: 40,
            reload_secs: 2.5,
            sound_reload: "BL_WPN_GUN_SCATTER_BLASTER.ANIM_RELOAD",
            sound_shot: "BL_WPN_GUN_SCATTER_BLASTER.AUTO_SHOTGUN_SHOOT",
            sound_shot_low: "BL_WPN_GUN_SCATTER_BLASTER.AUTO_SHOTGUN_SHOOT_LOW_AMMO",
            low_ammo: 2,
            // A fixed spread: it only scales the pellet pattern (to 0.75 in fine aim).
            spread: Spread {
                min: 0.15,
                max: 0.15,
                per_shot: 0.0,
                cooldown: 0.0,
                fine_aim: 0.75,
            },
        },
        reload_anim: (ANIM_SCATTER_RELOAD, 75.0 / 30.0),
        equip_anim: ANIM_EQUIP + 6,
        unequip_anim: ANIM_EQUIP + 7,
        sound_change_hands: "BL_WPN_GUN_SCATTER_BLASTER.ANIM_CHANGE_HANDS",
        hud_label: "Scatter Blaster",
        hud_icon: "ItemWireframeShotgun",
        hud_crosshair: "mc_crosshairShotgun",
        burst: None,
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        fine_aim_stance: 2,
        charge: None,
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_PLASMA_02"),
        pickup_model: "models/weapons/scatter_blaster.glb",
        pellets: &[
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, -0.0625),
            Vec2::new(0.0, 0.08),
            Vec2::new(0.065, 0.0325),
            Vec2::new(-0.075, -0.0325),
            Vec2::new(0.075, -0.0325),
            Vec2::new(-0.065, 0.0325),
        ],
        // DistanceScaler_2833: 2x to 20 m, down to 0.25x at 50 m, 0.25x out to the
        // end of its range.
        falloff: &[(20.0, 2.0), (50.0, 0.25)],
        // Weapon_Recoil_Light_S at 1.667x with its Fire_WEP (18 frames).
        shot_anim: Some((ANIM_SCATTER_FIRE, 18.0 / 30.0)),
        recoil: Some((
            CameraRecoil {
                pitch: 2.75,
                fov: -1.0,
                secs: 0.4,
                delay: 0.05,
            },
            CameraRecoil {
                pitch: 1.375,
                fov: -1.0,
                secs: 0.625,
                delay: 0.1,
            },
        )),
        // Shotgun_SV_WEPDATA's: intensity 0.35, over 0.35 s: the hardest jolt.
        shake: Some(CameraShake {
            intensity: 0.35,
            roll: 0.25,
            roll_secs: 0.25,
            secs: 0.35,
        }),
        sockets: Some(SCATTER_SOCKETS),
        // Med_Shell2_FX: a big shell, and its steam, with every shot.
        shell: GunShell {
            effect: Effect::MedShell,
            chance: 1.0,
            steam: 1.0,
        },
        // ReloadSteam_Med_FX and ReloadSteam_Med02_FX on its ReloadFX socket.
        reload_steam: Some(Effect::ScatterReloadSteam),
        projectile: None,
        level_fx: None,
        // (Shotgun_WEPMESH's TnWeaponEffectPlayMuzzleFX: SCATTER_MUZZLE is its light.)
        muzzle_system: Some("Scatterblaster_MuzzleFlash_FX"),
        marks: false,
        change_hands_anim: ANIM_HAND_SWITCH + 3,
        // Shotgun_SV_WEPDATA's FiringGroundSpeedMultiplier: a little slower while it
        // fires.
        ground_speed: 1.0,
        firing_speed: 0.85,
        heavy: false,
        ramp: None,
        alt: AltFire::FineAim,
        equip_secs: EQUIP_SECS,
        gear: None,
    },
    // The Nucleon Charge Rifle (TnWeaponSniperRifle, WEP_SniperRifle_p's
    // SniperRifle_WEPDATA, the data Escalation uses: it has no other): a
    // semi-automatic rifle firing an instant hit every 1.25 s at most, 75 damage
    // from the hip; in fine aim its 25-degree scope charges it up for 150 and then
    // 285 (Charge). Four rounds a clip, 20 in reserve.
    WeaponStats {
        id: "SniperRifle",
        mesh: "RB_SniperRifleR_SKEL",
        mesh_left: "RB_SniperRifleL_SKEL",
        fire_interval: 1.25,
        damage: 75.0,
        range: 300.0,
        // Its MuzzleFlash socket is 445 units down the forearm (NUCLEON_SOCKETS).
        muzzle_reach: 2.3,
        // (It has no sound for letting go of the trigger.)
        sound_end: "",
        sound_equip: "BL_WPN_GUN_NULLRAY.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_NULLRAY.ANIM_HOLSTER",
        sound_impact_world: "BL_WPN_GUN__IMPT_BYS.IMPT_SNIPER_WORLD",
        sound_impact_enemy: "BL_WPN_GUN__IMPT_BYS.IMPT_SNIPER_DMG_AI",
        // Its scope's ZOOM_LP is held for as long as it is up (weapon_loops);
        // ZOOM_OUT plays as it comes down.
        sound_aim_start: "",
        sound_aim_end: "BL_WPN_GUN_NULLRAY.ZOOM_OUT",
        fire_anim: ANIM_SNIPER_FIRE,
        idle_anim: ANIM_SNIPER_IDLE,
        cooldown_anim: None,
        muzzle_fx: Effect::NucleonMuzzle,
        tracer: TracerKind::Nucleon,
        impact_fx: Effect::NucleonImpact,
        gun: Gun {
            clip_size: 4,
            reserve_max: 20,
            reload_secs: 2.0,
            sound_reload: "BL_WPN_GUN_NULLRAY.ANIM_RELOAD",
            sound_shot: "BL_WPN_GUN_NULLRAY.SHOOT",
            sound_shot_low: "BL_WPN_GUN_NULLRAY.SHOOT_LOW_AMMO",
            low_ammo: 2,
            // From the hip 0.08, opening by 0.04 a shot up to 0.16 and settling back
            // over 3 s; in fine aim none (it has no FineAimSpreadModifier: 0).
            spread: Spread {
                min: 0.08,
                max: 0.16,
                per_shot: 0.04,
                cooldown: 3.0,
                fine_aim: 0.0,
            },
        },
        reload_anim: (ANIM_SNIPER_RELOAD, 60.0 / 30.0),
        equip_anim: ANIM_EQUIP + 8,
        unequip_anim: ANIM_EQUIP + 9,
        sound_change_hands: "BL_WPN_GUN_NULLRAY.ANIM_CHANGE_HANDS",
        hud_label: "Nucleon Charge Rifle",
        hud_icon: "ItemWireframeSniperRifle",
        // No reticle, from the hip or scoped (its ShouldShowReticule and
        // ShouldShowReticuleWhileZoomed are false): the scope has its own lines.
        hud_crosshair: "",
        // Semi-automatic (AutoFire 0): a burst of one. A pull of the trigger fires a
        // round, the next no sooner than 1.25 s later (its FireIntervals); one
        // pulled meanwhile fires as soon as it can. Each plays the gun's Fire_WEP
        // with the arm's recoil (Weapon_Recoil_HeavyLight_Small_01 at twice its
        // speed), 37 frames.
        burst: Some(Burst {
            shots: 1,
            delay: 1.25,
            anim: (ANIM_SNIPER_FIRE, 37.0 / 30.0),
            sound_each: false,
        }),
        fine_aim_fov: 25.0,
        scope: Scope::Sniper,
        scope_camera: true,
        // FineAimStance STANCE_GunArmShootFromHip.
        fine_aim_stance: 1,
        // TnSniperRifleExtension ChargeDelay 0.75 s; InstantHitDamage 75, 150, 285.
        charge: Some(Charge {
            delay: 0.75,
            damage: &[75.0, 150.0, 285.0],
            sound_scope: "BL_WPN_GUN_NULLRAY.ZOOM_LP",
            sound_charging: "BL_WPN_GUN_NULLRAY.ZOOM_CHARGE_LP",
            sound_charged: "BL_WPN_GUN_NULLRAY.ZOOM_CHARGE_COMPLETE_LP",
        }),
        sound_dry: Some("BL_WPN_GUN_NULLRAY.DRY_FIRE"),
        pickup_model: "models/weapons/sniper_rifle.glb",
        pellets: &[],
        // RangeDamageScale (DistanceScaler_3733): full damage to 150 m, down to 0.15x
        // at 250 m, 0.15x out to the end of its range.
        falloff: &[(150.0, 1.0), (250.0, 0.15)],
        shot_anim: None,
        // SniperRifle_WEPDATA's CameraRecoils: the view kicks up 1.5 degrees with
        // every shot, from the hip and in fine aim alike (it has no
        // FineAimCameraRecoils), with no change to the field of view.
        recoil: Some((
            CameraRecoil {
                pitch: 1.5,
                fov: 0.0,
                secs: 0.0,
                delay: 0.0,
            },
            CameraRecoil {
                pitch: 1.5,
                fov: 0.0,
                secs: 0.0,
                delay: 0.0,
            },
        )),
        // Its CameraShakes: intensity 0.55, rolling 0.35, falling off over 0.45 s.
        shake: Some(CameraShake {
            intensity: 0.55,
            roll: 0.35,
            roll_secs: 0.45,
            secs: 0.45,
        }),
        sockets: Some(NUCLEON_SOCKETS),
        // Med_Shell2_FX on ShellSocket, as the Scatter Blaster's: a big shell, and its
        // steam, with every shot.
        shell: GunShell {
            effect: Effect::MedShell,
            chance: 1.0,
            steam: 1.0,
        },
        // ReloadSteam_FX on its ReloadFX socket, as the Subsonic Repeater's.
        reload_steam: Some(Effect::SubsonicReloadSteam),
        projectile: None,
        level_fx: None,
        // (SniperRifle_WEPMESH's TnWeaponEffectPlayMuzzleFX: NUCLEON_MUZZLE is its light.)
        muzzle_system: Some("NucleonRifle_MuzzleFlash_FX"),
        marks: false,
        change_hands_anim: ANIM_HAND_SWITCH + 4,
        ground_speed: 1.0,
        firing_speed: 1.0,
        heavy: false,
        ramp: None,
        alt: AltFire::FineAim,
        equip_secs: EQUIP_SECS,
        gear: None,
    },
    // The Path Blaster (TnWeaponHeavyPistol, the game's "HeavyPistol" inside:
    // WEP_HeavyPistol_p, whose HeavyPistol_SV_WEPDATA Escalation uses): an automatic
    // blaster firing a 67-damage instant hit every 0.385 s, worth 1.15 times that up
    // to 20 m and falling off to a quarter by 150 m, ten rounds a clip and 40 in
    // reserve. Each shot kicks the view up hard (1.5 degrees from the hip, 0.75 in
    // fine aim) and throws its spread open by 0.02, from 0.04 up to 0.1. (Its
    // shockwave rounds, fire modes 1 and 2, come with the game's upgrades: left out.)
    WeaponStats {
        id: "HeavyPistol",
        mesh: "RB_HeavyPistolR_SKEL",
        mesh_left: "RB_HeavyPistolL_SKEL",
        fire_interval: 0.385,
        damage: 67.0,
        range: 300.0,
        // Its MuzzleFlash socket is 401 units down the forearm (PATH_SOCKETS).
        muzzle_reach: 2.09,
        // (It has no sound for letting go of the trigger, nor for lowering its
        // sights.)
        sound_end: "",
        sound_equip: "BL_WPN_GUN_BATTLE_PISTOL.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_BATTLE_PISTOL.ANIM_HOLSTER",
        sound_impact_world: "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_WORLD",
        sound_impact_enemy: "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_DMG_AI",
        sound_aim_start: "BL_WPN_GUN_BATTLE_PISTOL.ZOOM_IN",
        sound_aim_end: "",
        fire_anim: ANIM_PATH_FIRE,
        idle_anim: ANIM_PATH_IDLE,
        cooldown_anim: None,
        muzzle_fx: Effect::PathMuzzle,
        tracer: TracerKind::Path,
        impact_fx: Effect::PathImpact,
        gun: Gun {
            clip_size: 10,
            reserve_max: 40,
            reload_secs: 2.5,
            sound_reload: "BL_WPN_GUN_BATTLE_PISTOL.ANIM_RELOAD",
            sound_shot: "BL_WPN_GUN_BATTLE_PISTOL.SHOOT",
            sound_shot_low: "BL_WPN_GUN_BATTLE_PISTOL.SHOOT_LOW_AMMO",
            low_ammo: 2,
            // From 0.04, opening by 0.02 a shot up to 0.1 and settling back over
            // 2.5 s; half that in fine aim (FineAimSpreadModifier 0.5).
            spread: Spread {
                min: 0.04,
                max: 0.10,
                per_shot: 0.02,
                cooldown: 2.5,
                fine_aim: 0.5,
            },
        },
        reload_anim: (ANIM_PATH_RELOAD, 75.0 / 30.0),
        equip_anim: ANIM_EQUIP + 10,
        unequip_anim: ANIM_EQUIP + 11,
        sound_change_hands: "BL_WPN_GUN_BATTLE_PISTOL.ANIM_CHANGE_HANDEDNESS",
        hud_label: "Path Blaster",
        hud_icon: "ItemWireframeHeavyPistol",
        hud_crosshair: "mc_crosshairPistol",
        // Automatic (AutoFire 1): it fires for as long as the trigger is held.
        burst: None,
        // (It has no fine-aim camera of its own: the usual over-the-shoulder one.)
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        fine_aim_stance: 2,
        charge: None,
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_BALLISTIC"),
        pickup_model: "models/weapons/path_blaster.glb",
        pellets: &[],
        // RangeDamageScale (DistanceScaler_15120): 1.15x to 20 m, down to 0.25x at
        // 150 m, 0.25x out to the end of its range.
        falloff: &[(20.0, 1.15), (150.0, 0.25)],
        // Weapon_Recoil_Light_S at 2.25x with its Fire_WEP (14 frames).
        shot_anim: Some((ANIM_PATH_FIRE, 14.0 / 30.0)),
        // HeavyPistol_SV_WEPDATA's CameraRecoils and FineAimCameraRecoils: the view
        // kicks up 1.5 degrees with every shot from the hip and 0.75 in fine aim,
        // the field of view a degree wider for a moment, easing back over half a
        // second.
        recoil: Some((
            CameraRecoil {
                pitch: 1.5,
                fov: -1.0,
                secs: 0.55,
                delay: 0.01,
            },
            CameraRecoil {
                pitch: 0.75,
                fov: -1.0,
                secs: 0.5,
                delay: 0.01,
            },
        )),
        // Its CameraShakes (fire mode 0's): intensity 0.35, rolling 0.25, falling
        // off over 0.325 s.
        shake: Some(CameraShake {
            intensity: 0.35,
            roll: 0.25,
            roll_secs: 0.25,
            secs: 0.325,
        }),
        sockets: Some(PATH_SOCKETS),
        // Med_Shell2_FX on ShellEject, as the Scatter Blaster's: a big shell, and
        // its steam, with every shot.
        shell: GunShell {
            effect: Effect::MedShell,
            chance: 1.0,
            steam: 1.0,
        },
        // ReloadSteam_Med02_FX on its ReloadFX socket.
        reload_steam: Some(Effect::PathReloadSteam),
        projectile: None,
        level_fx: None,
        // (HeavyPistol_WEPMESH's TnWeaponEffectPlayMuzzleFX: PATH_MUZZLE is its light.)
        muzzle_system: Some("PathBlaster_MuzzleFlash_FX"),
        marks: false,
        change_hands_anim: ANIM_HAND_SWITCH + 5,
        ground_speed: 1.0,
        firing_speed: 1.0,
        heavy: false,
        ramp: None,
        alt: AltFire::FineAim,
        equip_secs: EQUIP_SECS,
        gear: None,
    },
    // The Riot Cannon (TnWeaponBazooka, the game's "Bazooka" inside: WEP_Bazooka_p,
    // whose Bazooka_SV_WEPDATA Escalation uses): an automatic rocket launcher, its
    // rounds flying at 180 m/s and going off for 155 damage to everything within
    // 6.5 m (riot.rs), one every 0.85 s for as long as the trigger is held. Four
    // rounds a clip, 24 in reserve. Each shot kicks the view up 1.5 degrees from the
    // hip (the field of view a degree wider for a moment) and 0.75 in fine aim, and
    // shakes it hard; its spread is a steady 0.04, half that in fine aim. (Its
    // second fire mode, TnProjectileBazookaSpecial, comes with the game's Planet
    // Buster upgrade: left out.)
    WeaponStats {
        id: "Bazooka",
        mesh: "RB_BazookaR_SKEL",
        mesh_left: "RB_BazookaL_SKEL",
        fire_interval: 0.85,
        damage: 155.0,
        // Range 30000: as far as the aim reaches.
        range: 300.0,
        // Its MuzzleFlash socket is 250 units down the forearm (RIOT_SOCKETS).
        muzzle_reach: 1.3,
        // (It has no sound for letting go of the trigger; its rounds bring their
        // own blast, riot.rs.)
        sound_end: "",
        sound_equip: "BL_WPN_GUN_FUSION_CANNON.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_FUSION_CANNON.ANIM_HOLSTER",
        sound_impact_world: "",
        sound_impact_enemy: "",
        sound_aim_start: "BL_WPN_GUN_FUSION_CANNON.FINE_AIM_START",
        sound_aim_end: "BL_WPN_GUN_FUSION_CANNON.FINE_AIM_END",
        fire_anim: ANIM_RIOT_FIRE,
        idle_anim: ANIM_RIOT_IDLE,
        cooldown_anim: None,
        // The shot's light (its TnWeaponEffectLight); the flash itself is the game's
        // RiotBazooka_MuzzleFlash_FX (riot.rs).
        muzzle_fx: Effect::RiotMuzzle,
        // (Unused: its rounds fly, riot.rs.)
        tracer: TracerKind::Bullet,
        impact_fx: Effect::BulletImpact,
        gun: Gun {
            clip_size: 4,
            reserve_max: 24,
            reload_secs: 2.5,
            sound_reload: "BL_WPN_GUN_FUSION_CANNON.ANIM_RELOAD",
            // Its TnWeaponEffectLowAmmoSound swaps in the same shot (threshold 1).
            sound_shot: "BL_WPN_GUN_FUSION_CANNON.SHOOT",
            sound_shot_low: "BL_WPN_GUN_FUSION_CANNON.SHOOT",
            low_ammo: 1,
            // A steady 0.04 (Modifier 0.04 to 0.04, none more a shot), half that in
            // fine aim (FineAimSpreadModifier 0.5).
            spread: Spread {
                min: 0.04,
                max: 0.04,
                per_shot: 0.0,
                cooldown: 1.0,
                fine_aim: 0.5,
            },
        },
        // Reload_S (75 frames, the gun's 2.5 s reload: WeaponReloadAnimTime).
        reload_anim: (ANIM_RIOT_RELOAD, 75.0 / 30.0),
        equip_anim: ANIM_EQUIP + 12,
        unequip_anim: ANIM_EQUIP + 13,
        sound_change_hands: "BL_WPN_GUN_FUSION_CANNON.ANIM_CHANGE_HANDEDNESS",
        hud_label: "Riot Cannon",
        hud_icon: "ItemWireframeBazooka",
        // updateCrosshair's for TnWeaponBazooka.
        hud_crosshair: "mc_crosshairCannon",
        // Automatic (AutoFire 1): it fires for as long as the trigger is held.
        burst: None,
        // (It has no fine-aim camera of its own: the usual over-the-shoulder one, as
        // the game's video shows: the view 1.8 times as close in fine aim.)
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        // FineAimStance: TnWeaponData's STANCE_GunArmFineAim.
        fine_aim_stance: 2,
        charge: None,
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_PLASMA_02"),
        pickup_model: "models/weapons/riot_cannon.glb",
        pellets: &[],
        // (Its rounds strike as hard at any range: RangeDamageScale has no Scales.)
        falloff: &[],
        // Weapon_Recoil_HeavyLight_S at 1.8x with its RiotCannon_Fire (30 frames:
        // a second).
        shot_anim: Some((ANIM_RIOT_FIRE, 1.0)),
        // Bazooka_SV_WEPDATA's CameraRecoils and FineAimCameraRecoils: 1.5 degrees
        // a shot from the hip, the field of view a degree wider, easing back over
        // 0.35 s after 0.1 s; 0.75 degrees in fine aim, the field of view as it is.
        recoil: Some((
            CameraRecoil {
                pitch: 1.5,
                fov: -1.0,
                secs: 0.35,
                delay: 0.1,
            },
            CameraRecoil {
                pitch: 0.75,
                fov: 0.0,
                secs: 0.35,
                delay: 0.1,
            },
        )),
        // Its CameraShakes: intensity 0.65, rolling 0.55 over 0.35 s, falling off
        // over 0.4 s: the hardest of the guns'.
        shake: Some(CameraShake {
            intensity: 0.65,
            roll: 0.55,
            roll_secs: 0.35,
            secs: 0.4,
        }),
        sockets: Some(RIOT_SOCKETS),
        // No shells.
        shell: GunShell {
            effect: Effect::Shell,
            chance: 0.0,
            steam: 0.0,
        },
        // ReloadSteam_FX and ReloadSteam_Med02_FX on its ReloadFX socket.
        reload_steam: Some(Effect::RiotReloadSteam),
        projectile: Some(&riot::RIOT_ROUND),
        level_fx: None,
        muzzle_system: None,
        marks: false,
        change_hands_anim: ANIM_HAND_SWITCH + 6,
        ground_speed: 1.0,
        firing_speed: 1.0,
        heavy: false,
        ramp: None,
        alt: AltFire::FineAim,
        equip_secs: EQUIP_SECS,
        gear: None,
    },
    // The Orbital Pistol (TnWeaponTracerDart, the game's "TracerDart" inside:
    // WEP_TracerDart_p, its multiplayer TracerDart_MP_WEPDATA; a Scientist's gun,
    // the campaign's SysCorruptor): an automatic pistol firing a 100-damage instant
    // hit every 0.85 s, at full damage out to 200 m and half that beyond (its
    // falloff measured from Jazz's centre to the sentry's), six rounds a clip and 24
    // in reserve. It doesn't kick the view at all (its CameraRecoils are 0), only
    // shakes it a little; its spread opens from 0.02 by 0.25 a shot (straight to its
    // most, 0.15) and settles back in a second, half that in fine aim. Its rounds
    // mark the sentries they strike, which then take more damage and show on the
    // players' HUDs (orbital.rs). (Its upgrades are left out.)
    WeaponStats {
        id: "TracerDart",
        mesh: "RB_SysCorruptorR_SKEL",
        mesh_left: "RB_SysCorruptorL_SKEL",
        fire_interval: 0.85,
        damage: 100.0,
        // Range 30000.
        range: 300.0,
        // Its MuzzleFlash socket is 341 units down the forearm (ORBITAL_SOCKETS).
        muzzle_reach: 1.77,
        // (It has no sound for letting go of the trigger.)
        sound_end: "",
        sound_equip: "BL_WPN_GUN_TRACER_DART.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_TRACER_DART.ANIM_HOLSTER",
        // Its impact's DefaultSound; on a sentry, its energy hit.
        sound_impact_world: "BL_WPN_GUN__IMPT_BYS.IMPT_ENERGY_WORLD",
        sound_impact_enemy: "BL_WPN_GUN__IMPT_BYS.IMPT_ENERGY_DMG_AI",
        sound_aim_start: "BL_WPN_GUN_TRACER_DART.FINE_AIM_START",
        sound_aim_end: "BL_WPN_GUN_TRACER_DART.FINE_AIM_END",
        fire_anim: ANIM_ORBITAL_FIRE,
        idle_anim: ANIM_ORBITAL_IDLE,
        cooldown_anim: None,
        // The shot's light (its TnWeaponEffectLight); the flash itself is the game's
        // TracerDartMuzzle_FX (orbital.rs).
        muzzle_fx: Effect::OrbitalMuzzle,
        // (Unused: its tracer and impact are the game's, orbital.rs.)
        tracer: TracerKind::Bullet,
        impact_fx: Effect::BulletImpact,
        gun: Gun {
            clip_size: 6,
            reserve_max: 24,
            reload_secs: 1.25,
            sound_reload: "BL_WPN_GUN_TRACER_DART.ANIM_RELOAD_01",
            // Its TnWeaponEffectLowAmmoSound swaps in the same shot (threshold 2).
            sound_shot: "BL_WPN_GUN_TRACER_DART.SHOOT",
            sound_shot_low: "BL_WPN_GUN_TRACER_DART.SHOOT",
            low_ammo: 2,
            // From 0.02, opening by 0.25 a shot up to 0.15 (DWWS_StartLow) and
            // settling back over a second; half that in fine aim
            // (FineAimSpreadModifier 0.5).
            spread: Spread {
                min: 0.02,
                max: 0.15,
                per_shot: 0.25,
                cooldown: 1.0,
                fine_aim: 0.5,
            },
        },
        // Reload_S_WEP (30 frames at 30 a second: 1 s), stretched to the gun's
        // 1.25 s (WeaponReloadAnimTime).
        reload_anim: (ANIM_ORBITAL_RELOAD, 1.0),
        equip_anim: ANIM_EQUIP + 14,
        unequip_anim: ANIM_EQUIP + 15,
        sound_change_hands: "BL_WPN_GUN_TRACER_DART.ANIM_CHANGE_HANDS",
        hud_label: "Orbital Pistol",
        // Its ItemWireframeTracerDart: the weapon icon's checkLinkageId shows the
        // SysCorruptor's.
        hud_icon: "ItemWireframeSysCorruptor",
        // (updateCrosshair has no case for TnWeaponTracerDart: the rifle's.)
        hud_crosshair: "mc_crosshairRifle",
        // Automatic (AutoFire 1): it fires for as long as the trigger is held.
        burst: None,
        // (It has no fine-aim camera of its own: the usual over-the-shoulder one.)
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        // FineAimStance: TnWeaponData's STANCE_GunArmFineAim.
        fine_aim_stance: 2,
        charge: None,
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_BALLISTIC"),
        pickup_model: "models/weapons/orbital_pistol.glb",
        pellets: &[],
        // RangeDamageScale (DistanceScaler_4425): 1x to 200 m (its two DST_Constant
        // scales, to 10000 and 20000 units), 0.5x beyond (its DST_MaxRange).
        falloff: &[(200.0, 1.0), (200.0, 0.5)],
        // Weapon_Recoil_Light_S at 2x with its SysCorruptor_Fire_WEP (15 frames).
        shot_anim: Some((ANIM_ORBITAL_FIRE, 15.0 / 30.0)),
        // TracerDart_MP_WEPDATA's CameraRecoils: no kick, no change of the field of
        // view (and no FineAimCameraRecoils).
        recoil: None,
        // Its CameraShakes: intensity 0.05, rolling 0.15 over 0.15 s, falling off
        // over 0.15 s: a light tremor.
        shake: Some(CameraShake {
            intensity: 0.05,
            roll: 0.15,
            roll_secs: 0.15,
            secs: 0.15,
        }),
        sockets: Some(ORBITAL_SOCKETS),
        // No shells.
        shell: GunShell {
            effect: Effect::Shell,
            chance: 0.0,
            steam: 0.0,
        },
        // ReloadSteam_Med02_FX on its Pickup socket (as the Path Blaster's).
        reload_steam: Some(Effect::PathReloadSteam),
        projectile: None,
        level_fx: Some(&orbital::ORBITAL_FX),
        muzzle_system: None,
        marks: true,
        change_hands_anim: ANIM_HAND_SWITCH + 7,
        ground_speed: 1.0,
        firing_speed: 1.0,
        heavy: false,
        ramp: None,
        alt: AltFire::FineAim,
        equip_secs: EQUIP_SECS,
        gear: None,
    },
    // The X18 Scrapmaker (TnWeaponHeavyMG, all_shared's HeavyMG_SV_WEPDATA): a
    // Gatling gun whose barrels spin up before it fires (arsenal.rs). Its fire
    // interval eases from 0.35 s down to 0.065 s over a second of the trigger held
    // and back as it rests, and until it is down to 0.15 s the gun only warms up:
    // then 27-damage instant hits from its two barrels in turn, full damage to 50 m
    // and down to a tenth at 100 m, out of a 200-round drum with no reserve (the
    // data has no MaxReserveAmmoCount). Jazz holds it in the heavy-gun stance, and
    // moves at three quarters of his speed while it fires.
    WeaponStats {
        id: "HeavyMG",
        mesh: "RB_HeavyMGR_SKEL",
        mesh_left: "RB_HeavyMGL_SKEL",
        // (FireIntervals' first: `ramp` eases it down.)
        fire_interval: 0.35,
        damage: 27.0,
        // Range 30000: as far as the aim reaches.
        range: 300.0,
        // Its MuzzleFlash socket is 320 units down the forearm (SCRAPMAKER_SOCKETS).
        muzzle_reach: 1.66,
        // StopFire: its tail.
        sound_end: "BL_WPN_GUN_PULSE_RIFLE.SHOOT_TAIL",
        sound_equip: "BL_WPN_GUN_PULSE_RIFLE.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_PULSE_RIFLE.ANIM_HOLSTER",
        // Its impact's DefaultSound; on a sentry, the ballistic hit (as the rifle's).
        sound_impact_world: "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_WORLD",
        sound_impact_enemy: "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_DMG_AI",
        sound_aim_start: "BL_WPN_GUN_PULSE_RIFLE.FINE_AIM_START",
        sound_aim_end: "BL_WPN_GUN_PULSE_RIFLE.FINE_AIM_END",
        // ADD_Weapon_HeavyMG_Fire_WEP with the arm's Weapon_Recoil_Rapid_Heavy_S,
        // looped from its first round (StartFire) to StopFire.
        fire_anim: ANIM_NEW_GUNS,
        idle_anim: ANIM_NEW_GUNS + 1,
        // ADD_Weapon_HeavyMG_CoolDown_WEP (29 frames) at StopFire, and at
        // StopCharging (let go while it was still warming up).
        cooldown_anim: Some((ANIM_HEAVYMG_COOLDOWN, 29.0 / 30.0)),
        // The shot's light (Brightness 15); its flash is the game's (arsenal.rs).
        muzzle_fx: Effect::ScrapmakerMuzzle,
        // (Unused: its tracer and impact are the game's.)
        tracer: TracerKind::Bullet,
        impact_fx: Effect::BulletImpact,
        gun: Gun {
            clip_size: 200,
            reserve_max: 0,
            reload_secs: 2.5,
            sound_reload: "BL_WPN_GUN_PULSE_RIFLE.ANIM_RELOAD",
            sound_shot: "BL_WPN_GUN_PULSE_RIFLE.SHOOT",
            sound_shot_low: "BL_WPN_GUN_PULSE_RIFLE.SHOOT_LOW_AMMO",
            low_ammo: 10,
            // From 0.07, opening by 0.0025 a round up to 0.14 and settling back over
            // 4 s; 0.85 of that in fine aim.
            spread: Spread {
                min: 0.07,
                max: 0.14,
                per_shot: 0.0025,
                cooldown: 4.0,
                fine_aim: 0.85,
            },
        },
        // ADD_Weapon_HeavyMG_Reload_S_WEP (104 frames), stretched to its 2.5 s
        // (WeaponReloadAnimTime), should it ever reload.
        reload_anim: (ANIM_NEW_GUNS + 2, 104.0 / 30.0),
        equip_anim: ANIM_NEW_GUNS + 3,
        unequip_anim: ANIM_NEW_GUNS + 4,
        sound_change_hands: "BL_WPN_GUN_PULSE_RIFLE.ANIM_CHANGE_HANDEDNESS",
        hud_label: "X18 Scrapmaker",
        hud_icon: "ItemWireframeHeavyMG",
        // (updateCrosshair has no case for TnWeaponHeavyMG: the rifle's.)
        hud_crosshair: "mc_crosshairRifle",
        // Automatic (AutoFire 1).
        burst: None,
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        // (FineAimStance STANCE_HeavyGunArmShootFromHip: `heavy`.)
        fine_aim_stance: 2,
        charge: None,
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_PLASMA_02"),
        pickup_model: "models/weapons/scrapmaker.glb",
        pellets: &[],
        // RangeDamageScale (DistanceScaler_9584): full damage to 50 m, down to 0.1x
        // at 100 m, 0.1x out to the end of its range.
        falloff: &[(50.0, 1.0), (100.0, 0.1)],
        shot_anim: None,
        // HeavyMG_SV_WEPDATA's CameraRecoils and FineAimCameraRecoils: a tenth of a
        // degree a round from the hip, half that in fine aim.
        recoil: Some((
            CameraRecoil {
                pitch: 0.1,
                fov: 0.0,
                secs: 0.05,
                delay: 0.01,
            },
            CameraRecoil {
                pitch: 0.05,
                fov: 0.0,
                secs: 0.05,
                delay: 0.01,
            },
        )),
        // Its CameraShakes: intensity 0.15, rolling 0.1, falling off over 0.1 s.
        shake: Some(CameraShake {
            intensity: 0.15,
            roll: 0.1,
            roll_secs: 0.1,
            secs: 0.1,
        }),
        sockets: Some(SCRAPMAKER_SOCKETS),
        // (Its shells are the game's Shell4_FX on ShellEject: arsenal.rs.)
        shell: GunShell {
            effect: Effect::Shell,
            chance: 0.0,
            steam: 0.0,
        },
        reload_steam: None,
        projectile: None,
        level_fx: Some(&SCRAPMAKER_FX),
        muzzle_system: None,
        marks: false,
        change_hands_anim: ANIM_NEW_GUNS + 5,
        ground_speed: 1.0,
        firing_speed: 0.75,
        heavy: true,
        ramp: Some(FireRamp {
            end: 0.065,
            secs: 1.0,
            warm_until: 0.15,
        }),
        alt: AltFire::FineAim,
        equip_secs: 0.2,
        gear: Some(&arsenal::SCRAPMAKER),
    },
    // The Energon Harvester (TnWeaponDrainGun, all_shared's DrainGun_MP_WEPDATA): a
    // shotgun firing bursts of two shots 0.2 s apart (ShotsPerBurst), 0.6 s between
    // bursts (BurstDelay), each shot nine 20-damage pellets in a fixed pattern, full
    // damage to 20 m and none by 40 m. Every pellet that wounds a sentry gives Jazz
    // 15 health back (arsenal.rs). Six rounds a clip, 24 in reserve.
    WeaponStats {
        id: "DrainGun",
        mesh: "RB_EmpShotgunR_SKEL",
        mesh_left: "RB_EmpShotgunL_SKEL",
        fire_interval: 0.2,
        damage: 20.0,
        range: 40.0,
        // Its MuzzleFlash socket is 228 units down the forearm (HARVESTER_SOCKETS).
        muzzle_reach: 1.19,
        // (It has no sound for letting go of the trigger.)
        sound_end: "",
        sound_equip: "BL_WPN_GUN_SHOTGUN.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_SHOTGUN.ANIM_HOLSTER",
        sound_impact_world: "BL_WPN_GUN__IMPT_BYS.IMPT_SHOTGUN_WORLD",
        sound_impact_enemy: "BL_WPN_GUN__IMPT_BYS.IMPT_SHOTGUN_DMG_AI",
        sound_aim_start: "BL_WPN_GUN_SHOTGUN.FINE_AIM_START",
        sound_aim_end: "BL_WPN_GUN_SHOTGUN.FINE_AIM_END",
        // A burst's: ADD_Weapon_EMPShotgun_Fire with the arm's EMPShotgun_Recoil_S
        // for each of its two shots (`burst`).
        fire_anim: ANIM_NEW_GUNS + 6,
        idle_anim: ANIM_NEW_GUNS + 7,
        cooldown_anim: None,
        // The shot's light (the default Brightness, 25, its colour (106, 181, 245)).
        muzzle_fx: Effect::HarvesterMuzzle,
        // (Unused: its tracer and impacts are the game's.)
        tracer: TracerKind::Scatter,
        impact_fx: Effect::ScatterImpact,
        gun: Gun {
            clip_size: 6,
            reserve_max: 24,
            reload_secs: 2.5,
            sound_reload: "BL_WPN_GUN_SHOTGUN.ANIM_RELOAD",
            sound_shot: "BL_WPN_GUN_SHOTGUN.SHOOT",
            sound_shot_low: "BL_WPN_GUN_SHOTGUN.SHOOT_LOW_AMMO",
            low_ammo: 1,
            // A steady 0.12: it only scales the pellet pattern (0.8 of it in fine
            // aim).
            spread: Spread {
                min: 0.12,
                max: 0.12,
                per_shot: 0.0,
                cooldown: 0.0,
                fine_aim: 0.8,
            },
        },
        // ADD_Weapon_EMPShotgun_Reload_S (74 frames), stretched to 2.5 s.
        reload_anim: (ANIM_NEW_GUNS + 8, 74.0 / 30.0),
        equip_anim: ANIM_NEW_GUNS + 9,
        unequip_anim: ANIM_NEW_GUNS + 10,
        sound_change_hands: "BL_WPN_GUN_SHOTGUN.ANIM_CHANGE_HANDS",
        hud_label: "Energon Harvester",
        hud_icon: "ItemWireframeDrainGun",
        // updateCrosshair's for TnWeaponDrainGun, as for the shotguns.
        hud_crosshair: "mc_crosshairShotgun",
        // AutoFire 0: each pull of the trigger fires a burst of two.
        burst: Some(Burst {
            shots: 2,
            delay: 0.6,
            anim: (ANIM_NEW_GUNS + 6, 24.0 / 30.0),
            // (Its shot sound is a TnWeaponEffectLowAmmoSound on FireShot: each
            // round's, the second the low-ammo one when the clip is down to one.)
            sound_each: true,
        }),
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        fine_aim_stance: 2,
        charge: None,
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_PLASMA_01"),
        pickup_model: "models/weapons/harvester.glb",
        // SpreadPattern (DrainGun_MP_WEPDATA's): a plus and an X, 0.05 out.
        pellets: &[
            Vec2::new(0.0, 0.0),
            Vec2::new(0.05, 0.0),
            Vec2::new(-0.05, 0.0),
            Vec2::new(0.0, 0.05),
            Vec2::new(0.0, -0.05),
            Vec2::new(0.0353, 0.0353),
            Vec2::new(-0.0353, 0.0353),
            Vec2::new(-0.0353, -0.0353),
            Vec2::new(0.0353, -0.0353),
        ],
        // RangeDamageScale (DistanceScaler_15590): full damage to 20 m, down to none
        // at 40 m, the end of its range.
        falloff: &[(20.0, 1.0), (40.0, 0.0)],
        shot_anim: None,
        // DrainGun_MP_WEPDATA's CameraRecoils and FineAimCameraRecoils: 2.5 degrees
        // a shot from the hip and 1.25 in fine aim, the field of view a degree wider
        // for a moment.
        recoil: Some((
            CameraRecoil {
                pitch: 2.5,
                fov: -1.0,
                secs: 0.7,
                delay: 0.2,
            },
            CameraRecoil {
                pitch: 1.25,
                fov: -1.0,
                secs: 0.35,
                delay: 0.1,
            },
        )),
        // Its CameraShakes: intensity 0.8, rolling 0.5 over 0.4 s, falling off over
        // 0.5 s: the hardest of any gun's.
        shake: Some(CameraShake {
            intensity: 0.8,
            roll: 0.5,
            roll_secs: 0.4,
            secs: 0.5,
        }),
        sockets: Some(HARVESTER_SOCKETS),
        // No shells.
        shell: GunShell {
            effect: Effect::Shell,
            chance: 0.0,
            steam: 0.0,
        },
        // (Its steam is the game's ReloadSteam_Harvester_FX: arsenal.rs.)
        reload_steam: None,
        projectile: None,
        level_fx: Some(&HARVESTER_FX),
        muzzle_system: None,
        marks: false,
        change_hands_anim: ANIM_NEW_GUNS + 11,
        ground_speed: 1.0,
        firing_speed: 1.0,
        heavy: false,
        ramp: None,
        alt: AltFire::FineAim,
        equip_secs: 0.75,
        gear: Some(&arsenal::HARVESTER),
    },
    // The E.D.K. TechVolt (TnWeaponChainBeam, all_shared's ChainBeam_SV_WEPDATA): a
    // beam out to 27.5 m doing 120 a second to what it strikes, chaining to every
    // other sentry within 14 m of it for 80 a second each (arsenal.rs). A round
    // every 0.07 s, 50 a clip (3.5 s of beam), 200 in reserve; Jazz moves at three
    // quarters of his speed while it fires.
    WeaponStats {
        id: "ChainBeam",
        mesh: "RB_RepairBeamR_SKEL",
        mesh_left: "RB_RepairBeamL_SKEL",
        fire_interval: 0.07,
        // DamagePerSecond times a round's 0.07 s (TnWeaponBeam.ProcessBeamHit).
        damage: 120.0 * 0.07,
        range: 27.5,
        // Its MuzzleFlash socket is 150 units down the forearm (TECHVOLT_SOCKETS).
        muzzle_reach: 0.78,
        // StopFire: its last shot.
        sound_end: "BL_WPN_GUN_TECH_VOLT.SHOOT_LAST_SHOT",
        sound_equip: "BL_WPN_GUN_TECH_VOLT.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_TECH_VOLT.ANIM_HOLSTER",
        // Its impact's DefaultSound; a sentry it strikes crackles instead (its
        // HitEnemy sound: arsenal.rs).
        sound_impact_world: "BL_WPN_GUN_REPAIR_WPN.IMPT_WORLD",
        sound_impact_enemy: "",
        sound_aim_start: "BL_WPN_GUN_REPAIR_WPN.FINE_AIM_START",
        sound_aim_end: "BL_WPN_GUN_REPAIR_WPN.FINE_AIM_END",
        // ADD_Weapon_RepairBeam_Fire_WEP with the arm's Weapon_Recoil_Rapid_Light_S
        // at 0.8 its speed, looped from StartFire to StopFire.
        fire_anim: ANIM_NEW_GUNS + 12,
        idle_anim: ANIM_NEW_GUNS + 13,
        // ADD_Weapon_RepairBeam_CoolDown_WEP (19 frames) at StopFire.
        cooldown_anim: Some((ANIM_TECHVOLT_COOLDOWN, 19.0 / 30.0)),
        // (No light with each round: it holds one while it fires, arsenal.rs.)
        muzzle_fx: Effect::NoFlash,
        // (Unused: its beam and impacts are the game's.)
        tracer: TracerKind::Bullet,
        impact_fx: Effect::BulletImpact,
        gun: Gun {
            clip_size: 50,
            reserve_max: 200,
            reload_secs: 2.5,
            sound_reload: "BL_WPN_GUN_TECH_VOLT.ANIM_RELOAD",
            // (Its rounds have no sound of their own: its loop is held while it
            // fires.)
            sound_shot: "",
            sound_shot_low: "",
            low_ammo: 0,
            // From 0.02, opening by 0.02 a round (straight to its most, 0.04) and
            // settling back in a second; half that in fine aim.
            spread: Spread {
                min: 0.02,
                max: 0.04,
                per_shot: 0.02,
                cooldown: 1.0,
                fine_aim: 0.5,
            },
        },
        // ADD_Weapon_RepairBeam_Reload_S_WEP (74 frames), stretched to 2.5 s.
        reload_anim: (ANIM_NEW_GUNS + 14, 74.0 / 30.0),
        equip_anim: ANIM_NEW_GUNS + 15,
        unequip_anim: ANIM_NEW_GUNS + 16,
        sound_change_hands: "BL_WPN_GUN_TECH_VOLT.ANIM_CHANGE_HANDEDNESS",
        hud_label: "E.D.K.TechVolt",
        hud_icon: "ItemWireframeChainBeam",
        // updateCrosshair's for TnWeaponChainBeam.
        hud_crosshair: "mc_crosshairChainBeam",
        // Automatic (AutoFire 1).
        burst: None,
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        fine_aim_stance: 2,
        charge: None,
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_ELECTRICITY"),
        pickup_model: "models/weapons/techvolt.glb",
        pellets: &[],
        // (Its RangeDamageScale has no Scales: full damage at any range.)
        falloff: &[],
        shot_anim: None,
        // (ChainBeam_SV_WEPDATA's camera recoil is all zero.)
        recoil: None,
        // Its CameraShakes: intensity 0.1, rolling 0.25 over 0.1 s, falling off over
        // 0.15 s, with every round.
        shake: Some(CameraShake {
            intensity: 0.1,
            roll: 0.25,
            roll_secs: 0.1,
            secs: 0.15,
        }),
        sockets: Some(TECHVOLT_SOCKETS),
        shell: GunShell {
            effect: Effect::Shell,
            chance: 0.0,
            steam: 0.0,
        },
        // (Its steam is the game's ReloadSteam_Tech_FX: arsenal.rs.)
        reload_steam: None,
        projectile: None,
        level_fx: Some(&TECHVOLT_FX),
        muzzle_system: None,
        marks: false,
        change_hands_anim: ANIM_NEW_GUNS + 17,
        ground_speed: 1.0,
        firing_speed: 0.75,
        heavy: false,
        ramp: None,
        alt: AltFire::FineAim,
        equip_secs: 0.2,
        gear: Some(&arsenal::TECHVOLT),
    },
    // The Corrosive Slime Cannon (TnWeaponAoESlow, all_shared's AoeSlow_WEPDATA): a
    // round a second (the default FireIntervals) that falls as it flies and goes
    // off for 125 within 8 m, slowing what it hurts (riot.rs's SLIME_ROUND). Five
    // rounds a clip, ten in reserve. Fine aim's button shows its round's path
    // instead of fine aiming (arsenal.rs).
    WeaponStats {
        id: "AoESlow",
        mesh: "RB_RustCannonR_SKEL",
        mesh_left: "RB_RustCannonL_SKEL",
        fire_interval: 1.0,
        damage: 125.0,
        range: 60.0,
        // Its MuzzleFlash socket is 250 units down the forearm (SLIME_SOCKETS).
        muzzle_reach: 1.3,
        sound_end: "",
        sound_equip: "BL_WPN_GUN_SLIME_CANNON.ANIM_EQUIP",
        // (Its ANIM_HOLSTER plays as it is dropped, TnWeaponEventDrop, which the
        // demo's weapons never are: putting it away is silent.)
        sound_holster: "",
        // (Its round brings its own blast: riot.rs.)
        sound_impact_world: "",
        sound_impact_enemy: "",
        // (It doesn't fine aim.)
        sound_aim_start: "",
        sound_aim_end: "",
        // ADD_Weapon_RustCannon_Fire with the arm's Weapon_Recoil_Light_S at twice
        // its speed, with every shot (`shot_anim`).
        fire_anim: ANIM_NEW_GUNS + 18,
        idle_anim: ANIM_NEW_GUNS + 19,
        cooldown_anim: None,
        // The shot's light (Brightness 3); its flash is the round's (riot.rs).
        muzzle_fx: Effect::SlimeMuzzle,
        // (Unused: its rounds fly, riot.rs.)
        tracer: TracerKind::Bullet,
        impact_fx: Effect::BulletImpact,
        gun: Gun {
            clip_size: 5,
            reserve_max: 10,
            reload_secs: 2.5,
            sound_reload: "BL_WPN_GUN_SLIME_CANNON.ANIM_RELOAD",
            // Its TnWeaponEffectLowAmmoSound swaps in the same shot (threshold 1).
            sound_shot: "BL_WPN_GUN_SLIME_CANNON.SHOOT",
            sound_shot_low: "BL_WPN_GUN_SLIME_CANNON.SHOOT",
            low_ammo: 1,
            // None (a Modifier of 0 to 0).
            spread: Spread {
                min: 0.0,
                max: 0.0,
                per_shot: 0.0,
                cooldown: 2.5,
                fine_aim: 0.75,
            },
        },
        // ADD_Weapon_RustCannon_Reload_S (74 frames), stretched to 2.5 s.
        reload_anim: (ANIM_NEW_GUNS + 20, 74.0 / 30.0),
        equip_anim: ANIM_NEW_GUNS + 21,
        unequip_anim: ANIM_NEW_GUNS + 22,
        sound_change_hands: "BL_WPN_GUN_SLIME_CANNON.ANIM_CHANGE_HANDEDNESS",
        hud_label: "Corrosive Slime Cannon",
        hud_icon: "ItemWireframeAoESlow",
        // updateCrosshair's for TnWeaponAoESlow.
        hud_crosshair: "mc_crosshairMortar",
        // Automatic (AutoFire 1).
        burst: None,
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        fine_aim_stance: 2,
        charge: None,
        // Its NoAmmoFire sounds: the dry click, with the grenade launcher's
        // (arsenal.rs).
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_BALLISTIC"),
        pickup_model: "models/weapons/slime_cannon.glb",
        pellets: &[],
        // (Its round's blast has its own falloff: riot.rs.)
        falloff: &[],
        // ADD_Weapon_RustCannon_Fire and the arm's recoil (29 frames).
        shot_anim: Some((ANIM_NEW_GUNS + 18, 29.0 / 30.0)),
        // (AoeSlow_WEPDATA's camera recoil is all zero.)
        recoil: None,
        // Its CameraShakes: intensity 0.15, rolling 0.1 over 0.15 s, falling off
        // over 0.2 s.
        shake: Some(CameraShake {
            intensity: 0.15,
            roll: 0.1,
            roll_secs: 0.15,
            secs: 0.2,
        }),
        sockets: Some(SLIME_SOCKETS),
        shell: GunShell {
            effect: Effect::Shell,
            chance: 0.0,
            steam: 0.0,
        },
        // (Its steam is the game's ReloadSteam_Slime_FX: arsenal.rs.)
        reload_steam: None,
        projectile: Some(&riot::SLIME_ROUND),
        // (Only its steam while it is ready: its round brings its flash.)
        level_fx: Some(&SLIME_FX),
        muzzle_system: None,
        marks: false,
        change_hands_anim: ANIM_NEW_GUNS + 23,
        ground_speed: 1.0,
        firing_speed: 1.0,
        heavy: false,
        ramp: None,
        alt: AltFire::Trajectory,
        equip_secs: 0.75,
        gear: Some(&arsenal::SLIME_CANNON),
    },
    // The Gear Shredder (TnWeaponSawblade, all_shared's SawBlade_SV_WEPDATA): held,
    // its trigger loads blades onto it, one at once and another every 0.65 s, three
    // at most and no more than the clip holds; let go, they all fly (riot.rs's
    // SAW_BLADE: bouncing off the level for 1.25 s, homing once they have bounced,
    // sticking in what they strike after that for 165), the side ones 5 degrees out
    // (arsenal.rs). Six rounds a clip, 12 in reserve, a round a blade. Jazz moves at
    // 0.85 of his speed while it charges, and can't sprint.
    WeaponStats {
        id: "SawBlade",
        mesh: "RB_SawbladeR_SKEL",
        mesh_left: "RB_SawbladeL_SKEL",
        // (Its FireIntervals: the wait after a shot before it can charge again.)
        fire_interval: 0.5,
        damage: 165.0,
        // Range 30000: as far as the aim reaches.
        range: 300.0,
        // Its MuzzleFlashCenter socket is 200 units down the forearm
        // (SHREDDER_SOCKETS).
        muzzle_reach: 1.04,
        sound_end: "",
        sound_equip: "BL_WPN_GUN_NEUTRON_RIFLE.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_NEUTRON_RIFLE.ANIM_HOLSTER",
        // (Its blades bring their own blasts and bounces: riot.rs.)
        sound_impact_world: "",
        sound_impact_enemy: "",
        sound_aim_start: "BL_WPN_GUN_NEUTRON_RIFLE.FINE_AIM_START",
        sound_aim_end: "BL_WPN_GUN_NEUTRON_RIFLE.FINE_AIM_END",
        // ADD_Weapon_Sawblade_Charging_WEP, looped while it charges (StartCharging
        // to StopCharging: Jazz's `firing` while it charges); its shot's clips are
        // arsenal.rs's (by its blades).
        fire_anim: ANIM_SHREDDER_CHARGING,
        idle_anim: ANIM_NEW_GUNS + 25,
        cooldown_anim: None,
        // The shot's light (Brightness 8, colour (164, 232, 255)); it has no flash.
        muzzle_fx: Effect::ShredderMuzzle,
        // (Unused: its rounds fly, riot.rs.)
        tracer: TracerKind::Bullet,
        impact_fx: Effect::BulletImpact,
        gun: Gun {
            clip_size: 6,
            reserve_max: 12,
            reload_secs: 2.0,
            sound_reload: "BL_WPN_GUN_SAW_BLADE.ANIM_RELOAD",
            // (Its shots' sounds go by its blades: arsenal.rs. Its
            // TnWeaponEffectSoundPerChargeLevel's low-ammo one, the dry click, would
            // play with no rounds left over: never, as it fires with some.)
            sound_shot: "BL_WPN_GUN_SAW_BLADE.SHOOT",
            sound_shot_low: "BL_WPN_GUN_SAW_BLADE.SHOOT",
            low_ammo: 0,
            // None (a Modifier of 0 to 0).
            spread: Spread {
                min: 0.0,
                max: 0.0,
                per_shot: 0.0,
                cooldown: 2.0,
                fine_aim: 0.5,
            },
        },
        // ADD_Weapon_Sawblade_Reload_S_WEP (74 frames), stretched to its 2 s
        // (WeaponReloadAnimTime).
        reload_anim: (ANIM_NEW_GUNS + 26, 74.0 / 30.0),
        equip_anim: ANIM_NEW_GUNS + 27,
        unequip_anim: ANIM_NEW_GUNS + 28,
        sound_change_hands: "BL_WPN_GUN_NEUTRON_RIFLE.ANIM_CHANGE_HANDS",
        hud_label: "Gear Shredder",
        hud_icon: "ItemWireframeSawBlade",
        // updateCrosshair's for TnWeaponSawblade.
        hud_crosshair: "mc_crosshairSawblade",
        // (A pull charges it, and letting go fires: arsenal.rs.)
        burst: None,
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        fine_aim_stance: 2,
        charge: None,
        // (SawBlade_WEPMESH has nothing for NoAmmoFire.)
        sound_dry: None,
        pickup_model: "models/weapons/gear_shredder.glb",
        pellets: &[],
        // (Its blades do their damage alone: riot.rs.)
        falloff: &[],
        // (By its blades: arsenal.rs's Blades::shot_anims.)
        shot_anim: None,
        // SawBlade_SV_WEPDATA's CameraRecoils: no pitch, the field of view 1.5
        // degrees wider at once, back over 0.7 s; in fine aim 1 degree, back over
        // 0.65 s after 0.1.
        recoil: Some((
            CameraRecoil {
                pitch: 0.0,
                fov: -1.5,
                secs: 0.7,
                delay: 0.01,
            },
            CameraRecoil {
                pitch: 0.0,
                fov: -1.0,
                secs: 0.65,
                delay: 0.1,
            },
        )),
        // Its CameraShakes: intensity 0.5, rolling 0.4 over 0.45 s, falling off over
        // 0.5 s.
        shake: Some(CameraShake {
            intensity: 0.5,
            roll: 0.4,
            roll_secs: 0.45,
            secs: 0.5,
        }),
        sockets: Some(SHREDDER_SOCKETS),
        shell: GunShell {
            effect: Effect::Shell,
            chance: 0.0,
            steam: 0.0,
        },
        // (Its steam is the game's ReloadSteam_Med_FX and _Med02_FX: arsenal.rs.)
        reload_steam: None,
        projectile: Some(&riot::SAW_BLADE),
        level_fx: None,
        muzzle_system: None,
        marks: false,
        change_hands_anim: ANIM_NEW_GUNS + 29,
        ground_speed: 1.0,
        // (ChargingGroundSpeedMultiplier, while it charges.)
        firing_speed: 0.85,
        heavy: false,
        ramp: None,
        alt: AltFire::FineAim,
        equip_secs: 0.2,
        gear: Some(&arsenal::GEAR_SHREDDER),
    },
    // The Thermo Rocket Cannon (TnWeaponRocketLauncher, the "HomingRocket", all_shared's
    // RocketLauncher_SV_WEPDATA): a rocket a pull of the trigger (riot.rs's
    // THERMO_ROCKET: 375 within 7 m), homing on the sentry it was locked on to (kept
    // under the crosshair for 0.75 s: arsenal.rs's THERMO_ROCKET), its clip of one
    // reloading at once. Five in reserve. Jazz holds it in the heavy-gun stance.
    WeaponStats {
        id: "HomingRocket",
        mesh: "RB_RocketLauncherR_SKEL",
        mesh_left: "RB_RocketLauncherL_SKEL",
        // FireIntervals 0: a rocket and it reloads (its clip holds one).
        fire_interval: 0.0,
        damage: 375.0,
        // Range 30000: as far as the aim reaches.
        range: 300.0,
        // Its MuzzleFlash socket is 315 units down the forearm (THERMO_SOCKETS).
        muzzle_reach: 1.64,
        sound_end: "",
        sound_equip: "BL_WPN_GUN_ROCKET.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_ROCKET.ANIM_HOLSTER",
        // (Its rockets bring their own blasts: riot.rs.)
        sound_impact_world: "",
        sound_impact_enemy: "",
        // FINE_AIM_START as fine aim begins and as it ends (RocketLauncher_WEPMESH
        // has it for both).
        sound_aim_start: "BL_WPN_GUN_ROCKET.FINE_AIM_START",
        sound_aim_end: "BL_WPN_GUN_ROCKET.FINE_AIM_START",
        // ADD_Weapon_RocketLauncher_Fire_WEP with the arm's Weapon_Recoil_Heavy_S at
        // 1.5 times its speed (its burst's clip).
        fire_anim: ANIM_NEW_GUNS + 30,
        idle_anim: ANIM_NEW_GUNS + 31,
        cooldown_anim: None,
        // The shot's light (its TnWeaponEffectLight's defaults: Brightness 25, colour
        // (245, 174, 122)); its flash is the rocket's (riot.rs).
        muzzle_fx: Effect::ThermoMuzzle,
        // (Unused: its rounds fly, riot.rs.)
        tracer: TracerKind::Bullet,
        impact_fx: Effect::BulletImpact,
        gun: Gun {
            clip_size: 1,
            reserve_max: 5,
            reload_secs: 2.0,
            sound_reload: "BL_WPN_GUN_ROCKET.ANIM_RELOAD",
            sound_shot: "BL_WPN_GUN_ROCKET.SHOOT",
            sound_shot_low: "BL_WPN_GUN_ROCKET.SHOOT",
            low_ammo: 0,
            // 0.04 to 0.08, 0.02 more a shot, back over 3 s; half in fine aim.
            spread: Spread {
                min: 0.04,
                max: 0.08,
                per_shot: 0.02,
                cooldown: 3.0,
                fine_aim: 0.5,
            },
        },
        // ADD_Weapon_RocketLauncher_Reload_S_WEP (60 frames), its 2 s
        // (WeaponReloadAnimTime).
        reload_anim: (ANIM_NEW_GUNS + 32, 60.0 / 30.0),
        equip_anim: ANIM_NEW_GUNS + 33,
        unequip_anim: ANIM_NEW_GUNS + 34,
        sound_change_hands: "BL_WPN_GUN_ROCKET.ANIM_CHANGE_HANDEDNESS",
        hud_label: "Thermo Rocket Cannon",
        hud_icon: "ItemWireframeHomingRocket",
        // updateCrosshair's for TnWeaponRocketLauncher.
        hud_crosshair: "mc_crosshairRocket",
        // Semi-automatic (AutoFire 0): a burst of one, its Fire_WEP and the arm's
        // recoil (20 frames) with each.
        burst: Some(Burst {
            shots: 1,
            delay: 0.0,
            anim: (ANIM_NEW_GUNS + 30, 20.0 / 30.0),
            sound_each: false,
        }),
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        // (FineAimStance STANCE_HeavyGunArmShootFromHip: `heavy`.)
        fine_aim_stance: 2,
        charge: None,
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_BALLISTIC"),
        pickup_model: "models/weapons/thermo_rocket.glb",
        pellets: &[],
        // (Its rockets' blasts have their own falloff: riot.rs.)
        falloff: &[],
        shot_anim: None,
        // CameraRecoils: the view kicked up 0.25 degrees, with no change to the field
        // of view; the same in fine aim (it has no FineAimCameraRecoils:
        // GetRecoilCameraParams).
        recoil: Some((
            CameraRecoil {
                pitch: 0.25,
                fov: 0.0,
                secs: 0.6,
                delay: 0.25,
            },
            CameraRecoil {
                pitch: 0.25,
                fov: 0.0,
                secs: 0.6,
                delay: 0.25,
            },
        )),
        // Its CameraShakes: intensity 0.6, rolling 0.4 over 0.4 s, falling off over
        // 0.5 s.
        shake: Some(CameraShake {
            intensity: 0.6,
            roll: 0.4,
            roll_secs: 0.4,
            secs: 0.5,
        }),
        sockets: Some(THERMO_SOCKETS),
        shell: GunShell {
            effect: Effect::Shell,
            chance: 0.0,
            steam: 0.0,
        },
        // (Its smoke and steam are the game's: arsenal.rs.)
        reload_steam: None,
        projectile: Some(&riot::THERMO_ROCKET),
        level_fx: None,
        muzzle_system: None,
        marks: false,
        change_hands_anim: ANIM_NEW_GUNS + 35,
        ground_speed: 1.0,
        firing_speed: 1.0,
        heavy: true,
        ramp: None,
        alt: AltFire::FineAim,
        equip_secs: 0.2,
        gear: Some(&arsenal::THERMO_ROCKET),
    },
    // The A-4 Pulsar Cannon (TnWeaponGrenadeLauncher, all_shared's
    // GrenadeLauncher_WEPDATA, the data a match gives it: Escalation has none of its
    // own for it): a round a pull of the trigger, a second apart at the most (the
    // default FireIntervals), each sticking in what it strikes and going off two
    // seconds later (riot.rs's PULSAR_ROUND: 225 within 7.5 m), three out at most;
    // fine aim's button sets them all off (arsenal.rs's PULSAR_CANNON). Three rounds a
    // clip, nine in reserve.
    WeaponStats {
        id: "GrenadeLauncher",
        mesh: "RB_GrenadeLauncherR_SKEL",
        mesh_left: "RB_GrenadeLauncherL_SKEL",
        fire_interval: 1.0,
        damage: 225.0,
        range: 65.0,
        // Its MuzzleFlash socket is 220 units down the forearm (PULSAR_SOCKETS).
        muzzle_reach: 1.14,
        sound_end: "",
        sound_equip: "BL_WPN_GUN_PLASMA_CANNON.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_PLASMA_CANNON.ANIM_HOLSTER",
        // (Its rounds bring their own blasts: riot.rs.)
        sound_impact_world: "",
        sound_impact_enemy: "",
        // (It doesn't fine aim: SupportsFineAim is off.)
        sound_aim_start: "",
        sound_aim_end: "",
        // ADD_Weapon_PulsarCannon_Fire_WEP (StartFire) with the arm's
        // Weapon_Recoil_HeavyLight_S at 1.5 times its speed (FireShot): its burst's
        // clip.
        fire_anim: ANIM_NEW_GUNS + 36,
        idle_anim: ANIM_NEW_GUNS + 37,
        cooldown_anim: None,
        // The shot's light (Brightness 25, colour (121, 75, 108)); its flash is its
        // round's (riot.rs).
        muzzle_fx: Effect::PulsarMuzzle,
        // (Unused: its rounds fly, riot.rs.)
        tracer: TracerKind::Bullet,
        impact_fx: Effect::BulletImpact,
        gun: Gun {
            clip_size: 3,
            reserve_max: 9,
            reload_secs: 2.5,
            sound_reload: "BL_WPN_GUN_PLASMA_CANNON.ANIM_RELOAD",
            // (Its TnWeaponEffectLowAmmoSound has only its shot.)
            sound_shot: "BL_WPN_GUN_PLASMA_CANNON.PULSAR_CANNON_SHOOT",
            sound_shot_low: "BL_WPN_GUN_PLASMA_CANNON.PULSAR_CANNON_SHOOT",
            low_ammo: 0,
            // 0.01 throughout.
            spread: Spread {
                min: 0.01,
                max: 0.01,
                per_shot: 0.0,
                cooldown: 0.0,
                fine_aim: 0.75,
            },
        },
        // ADD_Weapon_PulsarCannon_Reload_S_WEP (75 frames), its 2.5 s.
        reload_anim: (ANIM_NEW_GUNS + 38, 75.0 / 30.0),
        equip_anim: ANIM_NEW_GUNS + 39,
        unequip_anim: ANIM_NEW_GUNS + 40,
        sound_change_hands: "BL_WPN_GUN_PLASMA_CANNON.CHANGE_HANDEDNESS",
        hud_label: "A-4 Pulsar Cannon",
        hud_icon: "ItemWireframeGrenadeLauncher",
        // updateCrosshair's for TnWeaponGrenadeLauncher.
        hud_crosshair: "mc_crosshairCannon",
        // Semi-automatic (AutoFire 0): a burst of one, a second before the next.
        burst: Some(Burst {
            shots: 1,
            delay: 1.0,
            anim: (ANIM_NEW_GUNS + 36, 27.0 / 30.0),
            sound_each: false,
        }),
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        fine_aim_stance: 2,
        charge: None,
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_BALLISTIC"),
        pickup_model: "models/weapons/pulsar_cannon.glb",
        pellets: &[],
        // (Its rounds' blasts have their own falloff: riot.rs.)
        falloff: &[],
        shot_anim: None,
        // (GrenadeLauncher_WEPDATA's camera recoil is zero.)
        recoil: None,
        // Its CameraShakes: intensity 0.7, rolling 0.5 over 0.5 s, falling off over
        // 0.4 s.
        shake: Some(CameraShake {
            intensity: 0.7,
            roll: 0.5,
            roll_secs: 0.5,
            secs: 0.4,
        }),
        sockets: Some(PULSAR_SOCKETS),
        shell: GunShell {
            effect: Effect::Shell,
            chance: 0.0,
            steam: 0.0,
        },
        // (Its steam is the game's: arsenal.rs.)
        reload_steam: None,
        projectile: Some(&riot::PULSAR_ROUND),
        // (Only its glow while it is ready: its round brings its flash.)
        level_fx: Some(&PULSAR_FX),
        muzzle_system: None,
        marks: false,
        change_hands_anim: ANIM_NEW_GUNS + 41,
        ground_speed: 1.0,
        firing_speed: 1.0,
        heavy: false,
        ramp: None,
        alt: AltFire::Detonate,
        equip_secs: 0.75,
        gear: Some(&arsenal::PULSAR_CANNON),
    },
    // The Chaos-Rift Combustor (TnWeaponClusterCannon, all_shared's
    // ClusterCannon_SV_WEPDATA): a bomb lobbed at what the crosshair is on (riot.rs's
    // COMBUSTOR_SHELL: 150 within 4.5 m), bursting into five clusters as it goes off
    // (150 within 8.5 m each), or in the air as the trigger is pulled again; its clip
    // of one reloading after each. Fine aim's button shows its lob and where it would
    // land (arsenal.rs's COMBUSTOR). Five in reserve.
    WeaponStats {
        id: "ClusterCannon",
        mesh: "RB_GrenadeArmR_SKEL",
        mesh_left: "RB_GrenadeArmL_SKEL",
        fire_interval: 0.5,
        damage: 150.0,
        // Range 5000: its lob's reach (arsenal.rs's Arc).
        range: 50.0,
        // Its MuzzleFlash socket is 125 units down the forearm (COMBUSTOR_SOCKETS).
        muzzle_reach: 0.65,
        sound_end: "",
        sound_equip: "BL_WPN_GUN_GRENADE_LAUNCHER.ANIM_EQUIP",
        sound_holster: "BL_WPN_GUN_GRENADE_LAUNCHER.ANIM_HOLSTER",
        // (Its bombs bring their own blasts: riot.rs.)
        sound_impact_world: "",
        sound_impact_enemy: "",
        // (It doesn't fine aim: SupportsFineAim is off.)
        sound_aim_start: "",
        sound_aim_end: "",
        // ADD_Weapon_GrenadeLauncher_Fire_WEP with the arm's Weapon_Recoil_HeavyLight_S
        // at twice its speed, with every shot (`shot_anim`).
        fire_anim: ANIM_NEW_GUNS + 42,
        idle_anim: ANIM_NEW_GUNS + 43,
        cooldown_anim: None,
        // The shot's light (its TnWeaponEffectLight's defaults); its flash is its
        // bomb's (riot.rs).
        muzzle_fx: Effect::CombustorMuzzle,
        // (Unused: its rounds fly, riot.rs.)
        tracer: TracerKind::Bullet,
        impact_fx: Effect::BulletImpact,
        gun: Gun {
            clip_size: 1,
            reserve_max: 5,
            reload_secs: 2.5,
            sound_reload: "BL_WPN_GUN_GRENADE_LAUNCHER.ANIM_RELOAD",
            // Its TnWeaponEffectLowAmmoSound's shot and low-ammo shot are the same,
            // SHOOT_LOW_AMMO.
            sound_shot: "BL_WPN_GUN_GRENADE_LAUNCHER.SHOOT_LOW_AMMO",
            sound_shot_low: "BL_WPN_GUN_GRENADE_LAUNCHER.SHOOT_LOW_AMMO",
            low_ammo: 1,
            // 0 to 0.04, 0.04 more a shot, back over 2 s.
            spread: Spread {
                min: 0.0,
                max: 0.04,
                per_shot: 0.04,
                cooldown: 2.0,
                fine_aim: 1.0,
            },
        },
        // ADD_Weapon_GrenadeLauncher_Reload_S_WEP (75 frames), its 2.5 s.
        reload_anim: (ANIM_NEW_GUNS + 44, 75.0 / 30.0),
        equip_anim: ANIM_NEW_GUNS + 45,
        unequip_anim: ANIM_NEW_GUNS + 46,
        sound_change_hands: "BL_WPN_GUN_GRENADE_LAUNCHER.ANIM_CHANGE_HANDS",
        hud_label: "Chaos-Rift Combustor",
        hud_icon: "ItemWireframeClusterCannon",
        // updateCrosshair's for TnWeaponClusterCannon (its _ShouldShowReticule is on).
        hud_crosshair: "mc_crosshairMortar",
        // Automatic (AutoFire 1).
        burst: None,
        fine_aim_fov: FINE_AIM_FOV,
        scope: Scope::None,
        scope_camera: false,
        fine_aim_stance: 2,
        charge: None,
        sound_dry: Some("BL_WPN_FOLEY.SHOOT_DRY_FIRE_PLASMA_01"),
        pickup_model: "models/weapons/combustor.glb",
        pellets: &[],
        // (Its bombs' blasts do their damage: riot.rs.)
        falloff: &[],
        // ADD_Weapon_GrenadeLauncher_Fire_WEP and the arm's recoil (20 frames).
        shot_anim: Some((ANIM_NEW_GUNS + 42, 20.0 / 30.0)),
        // (ClusterCannon_SV_WEPDATA's camera recoil is zero.)
        recoil: None,
        // Its CameraShakes: intensity 0.3, rolling 0.65 over 0.85 s, falling off over
        // 0.8 s.
        shake: Some(CameraShake {
            intensity: 0.3,
            roll: 0.65,
            roll_secs: 0.85,
            secs: 0.8,
        }),
        sockets: Some(COMBUSTOR_SOCKETS),
        shell: GunShell {
            effect: Effect::Shell,
            chance: 0.0,
            steam: 0.0,
        },
        // (Its shells and steam are the game's: arsenal.rs.)
        reload_steam: None,
        projectile: Some(&riot::COMBUSTOR_SHELL),
        level_fx: None,
        muzzle_system: None,
        marks: false,
        change_hands_anim: ANIM_NEW_GUNS + 47,
        ground_speed: 1.0,
        firing_speed: 1.0,
        heavy: false,
        ramp: None,
        alt: AltFire::Arc,
        equip_secs: 0.2,
        gear: Some(&arsenal::COMBUSTOR),
    },
];
/// The two weapons (indices into WEAPONS) Jazz starts with, and the one in hand:
/// the Neutron Assault Rifle and the Scatter Blaster.
const START_LOADOUT: [usize; 2] = [0, 3];
const START_WEAPON: usize = START_LOADOUT[0];

/// The weapon the game calls `id` (its index in WEAPONS), if the demo has it.
pub(crate) fn weapon_index(id: &str) -> Option<usize> {
    WEAPONS.iter().position(|w| w.id == id)
}

/// The guns the Customize screen can show on its preview robots: each one's id,
/// its mesh on Jazz's right arm and its idle clip (robot.glb).
pub(crate) fn preview_guns() -> Vec<(&'static str, &'static str, usize)> {
    WEAPONS
        .iter()
        .map(|w| (w.id, w.mesh, w.idle_anim))
        .collect()
}

/// The upgrades of the character the player chose (choose.rs): his build's weapon
/// upgrades (its WeaponUpgradePairs), those of them the demo has.
fn chosen_pairs(chosen: &choose::Chosen) -> upgrades::Installed {
    match chosen.weapons.as_ref() {
        Some(weapons) => upgrades::pairs(weapons, &chosen.weapon_upgrades),
        None => 0,
    }
}

/// The two weapons of the character the player chose (choose.rs) as indices into
/// WEAPONS: the starting one in its slot stands in for one the demo hasn't got.
fn chosen_loadout(chosen: &choose::Chosen) -> Option<[usize; 2]> {
    let ids = chosen.weapons.as_ref()?;
    Some(std::array::from_fn(|i| {
        weapon_index(&ids[i]).unwrap_or(START_LOADOUT[i])
    }))
}
/// The vehicles' weapons are as in the game's multiplayer (each weapon's
/// MultiplayerData), their names on the HUD its data providers' (TransGame.int's
/// FriendlyName); their rounds' flight and damage stay Escalation's.
///
/// The car's twin guns, the M1 Tachyon Repeater (AssaultRifleVehicle:
/// AssaultRifle_Vehicle_WEPDATA): instant hits out to 200 m (Range 20000), a shot
/// every 0.075 s.
const VEHICLE_RANGE: f32 = 200.0;
const VEHICLE_FIRE_DELAY: f32 = 0.075;
const VEHICLE_LABEL: &str = "M1 Tachyon Repeater";
/// Their camera shake (CameraShakes: intensity 0.05, over 0.1 s): a slight tremor,
/// with the car's cameras' waves (VEHICLE_SHAKE_WAVES).
const VEHICLE_SHAKE: CameraShake = CameraShake {
    intensity: 0.05,
    roll: 0.025,
    roll_secs: 0.1,
    secs: 0.1,
};
/// The car's guns as a `Gun` (a 60-round clip, 360 in reserve, a 2 s reload).
const VEHICLE_GUN: Gun = Gun {
    clip_size: 60,
    reserve_max: 360,
    reload_secs: 2.0,
    sound_reload: "BL_WPN_GUN_NEUTRON_RIFLE.VEH_ANIM_RELOAD",
    sound_shot: SND_VEHICLE_SHOT,
    sound_shot_low: "BL_WPN_GUN_NEUTRON_RIFLE.VEH_SHOOT_LOW_AMMO",
    low_ammo: 10,
    spread: Spread {
        min: 0.02,
        max: 0.08,
        per_shot: 0.008,
        cooldown: 1.25,
        fine_aim: 0.5,
    },
};
/// A tank's cannon, the Mega-Concussion Cannon (the Titan's vehicles' weapon,
/// TnWeaponTankCannon: TankCannon_WEPDATA): a shell at a time (ClipSize 1) and 9
/// more in reserve (InitialReserveAmmoCount, MaxReserveAmmoCount), the next loaded
/// in 2.25 s
/// (WeaponReloadAnimTime) as its reload clip plays (TnWeaponEffectPlayAnimation's
/// ADD_TankShooting_Reload, car.rs); its shot always the low-ammo one, a shell
/// being all it holds (TnWeaponEffectLowAmmoSound, LowAmmoThreshold 1).
const TANK_GUN: Gun = Gun {
    clip_size: 1,
    reserve_max: 9,
    reload_secs: 2.25,
    sound_reload: "BL_WPN_GUN_TANK_CANNON.ANIM_RELOAD",
    sound_shot: "BL_WPN_GUN_TANK_CANNON.SHOOT",
    sound_shot_low: "BL_WPN_GUN_TANK_CANNON.SHOOT_LOW_AMMO",
    low_ammo: 1,
    spread: Spread {
        min: 0.02,
        max: 0.08,
        per_shot: 0.06,
        cooldown: 2.5,
        fine_aim: 1.0,
    },
};
/// Its camera shake as it fires (CameraShakes: intensity 0.38, roll 0.65 settling
/// over 0.86 s, the shake over 0.79 s).
const TANK_SHAKE: CameraShake = CameraShake {
    intensity: 0.38,
    roll: 0.65,
    roll_secs: 0.86,
    secs: 0.79,
};
/// How far its shells' aim reaches (Range 30000).
const TANK_RANGE: f32 = 300.0;
const TANK_LABEL: &str = "Mega-Concussion Cannon";
/// A tank's shell as a JazzEvent::Round's weapon (the robot weapons are numbered
/// from 0).
const ROUND_TANK_SHELL: u8 = 254;
/// Pulling the trigger with the cannon empty (TnWeaponEffectSound on
/// TnWeaponEventNoAmmoFire).
const SND_TANK_DRY: &str = "BL_WPN_FOLEY.SHOOT_DRY_FIRE_PLASMA_01";
/// A jet's homing rockets, the Thermo Burst Rockets (the Scientist's vehicles'
/// weapon, TnWeaponHomingRocketVehicle: HomingRocket_Vehicle_WEPDATA): one to a clip
/// and 11 in reserve, the next loaded in 1.5 s; its shot is the same cue with the
/// clip running low (TnWeaponEffectLowAmmoSound's SHOOT_VEH, at 2 and under).
const JET_GUN: Gun = Gun {
    clip_size: 1,
    reserve_max: 11,
    reload_secs: 1.5,
    sound_reload: "BL_WPN_GUN_ROCKET.ANIM_RELOAD_veh",
    sound_shot: "BL_WPN_GUN_ROCKET.SHOOT_VEH",
    sound_shot_low: "BL_WPN_GUN_ROCKET.SHOOT_VEH",
    low_ammo: 2,
    spread: Spread {
        min: 0.02,
        max: 0.02,
        per_shot: 0.0,
        cooldown: 4.0,
        fine_aim: 0.5,
    },
};
/// Its camera shake (CameraShakes: intensity 0.3, roll 0.2 over 0.15 s, the shake
/// over 0.2 s) and recoil (CameraRecoils: the field of view 1 degree narrower, as
/// TnRecoilCameraBehavior takes its FovAdjustment off, coming back over 0.6 s after
/// 0.25).
const JET_SHAKE: CameraShake = CameraShake {
    intensity: 0.3,
    roll: 0.2,
    roll_secs: 0.15,
    secs: 0.2,
};
const JET_RECOIL: CameraRecoil = CameraRecoil {
    pitch: 0.0,
    fov: 1.0,
    secs: 0.6,
    delay: 0.25,
};
/// How far its rockets' aim reaches (Range 100000).
const JET_RANGE: f32 = 1000.0;
const JET_LABEL: &str = "Thermo Burst Rockets";
/// A jet's rocket as a JazzEvent::Round's weapon.
const ROUND_JET_ROCKET: u8 = 253;
/// The tone held on while a jet is locked on (TnWeaponEventStartLockOn to
/// StopLockOn), and the empty click of its rockets and a truck's
/// (TnWeaponEventNoAmmoFire).
const SND_JET_LOCK: &str = "BL_HUD_INTERFACE.HUD_TARGET_LOCK";
const SND_ROCKET_DRY: &str = "BL_WPN_FOLEY.SHOOT_DRY_FIRE_BALLISTIC";
/// A truck's rockets, the Eclipse Rockets (the Destroyer's vehicles' weapon,
/// TnWeaponRocketVehicle: Rocket_Vehicle_WEPDATA): 8 to a clip and 24 in reserve,
/// the clip reloaded in 2 s; its cues are a jet's (Rocket_Vehicle_WEPMESH's shot,
/// the same with the clip running low at 2 and under, its reload).
const TRUCK_GUN: Gun = Gun {
    clip_size: 8,
    reserve_max: 24,
    reload_secs: 2.0,
    sound_reload: "BL_WPN_GUN_ROCKET.ANIM_RELOAD_veh",
    sound_shot: "BL_WPN_GUN_ROCKET.SHOOT_VEH",
    sound_shot_low: "BL_WPN_GUN_ROCKET.SHOOT_VEH",
    low_ammo: 2,
    spread: Spread {
        min: 0.01,
        max: 0.01,
        per_shot: 0.01,
        cooldown: 1.0,
        fine_aim: 0.5,
    },
};
/// Its camera shake (CameraShakes: intensity 0.2, roll 0.2 over 0.2 s, the shake
/// over 0.3 s); its recoil (CameraRecoils) neither pitches nor narrows the view.
const TRUCK_SHAKE: CameraShake = CameraShake {
    intensity: 0.2,
    roll: 0.2,
    roll_secs: 0.2,
    secs: 0.3,
};
/// How far its rockets' aim reaches (Range 100000).
const TRUCK_RANGE: f32 = 1000.0;
const TRUCK_LABEL: &str = "Eclipse Rockets";
/// A truck's rocket as a JazzEvent::Round's weapon.
const ROUND_TRUCK_ROCKET: u8 = 252;
/// A JazzEvent::Round's weapon for a Riot Cannon's Planet Buster (specials.rs,
/// riot.rs's PLANET_BUSTER).
const ROUND_PLANET_BUSTER: u8 = 251;

/// A vehicle's rockets (fire_rockets): a jet's homing ones or a truck's.
struct Rockets {
    /// Its ammo slot and gun.
    slot: usize,
    gun: &'static Gun,
    /// Its rounds (riot.rs), and what a JazzEvent::Round calls them.
    kind: &'static riot::RoundKind,
    round: u8,
    /// Each pull of the trigger fires a burst of `burst` rockets (ShotsPerBurst, 1
    /// when unset) `interval` apart (FireIntervals), from its muzzles by turns
    /// (TnWeaponEffectIncrementMuzzleSocket); the next burst waits `burst_delay` more
    /// (BurstDelay) after the refire check that ends this one, an interval after
    /// its last rocket. `auto` (AutoFire): held, the trigger fires burst after burst.
    burst: u32,
    interval: f32,
    burst_delay: f32,
    auto: bool,
    shake: CameraShake,
    recoil: Option<CameraRecoil>,
    /// How far its aim reaches (Range).
    range: f32,
    /// It locks on to the sentry under the crosshair, its rockets homing on it
    /// (TnWeaponHoming: a jet's).
    locks_on: bool,
}

/// A jet's: a rocket for each pull of the trigger (no FireIntervals, ShotsPerBurst
/// or BurstDelay), its clip of one then reloading.
const JET_ROCKETS: Rockets = Rockets {
    slot: JET_SLOT,
    gun: &JET_GUN,
    kind: &riot::JET_ROCKET,
    round: ROUND_JET_ROCKET,
    burst: 1,
    interval: 0.0,
    burst_delay: 0.0,
    auto: false,
    shake: JET_SHAKE,
    recoil: Some(JET_RECOIL),
    range: JET_RANGE,
    locks_on: true,
};

/// A truck's: bursts of two 0.15 s apart, 0.55 s more between them, for as long
/// as the trigger is held.
const TRUCK_ROCKETS: Rockets = Rockets {
    slot: TRUCK_SLOT,
    gun: &TRUCK_GUN,
    kind: &riot::TRUCK_ROCKET,
    round: ROUND_TRUCK_ROCKET,
    burst: 2,
    interval: 0.15,
    burst_delay: 0.55,
    auto: true,
    shake: TRUCK_SHAKE,
    recoil: None,
    range: TRUCK_RANGE,
    locks_on: false,
};

/// Ammo slots: the robot weapons (WEAPONS), the car's guns, a tank's cannon, a jet's
/// rockets and a truck's.
const VEHICLE_SLOT: usize = WEAPONS.len();
const TANK_SLOT: usize = WEAPONS.len() + 1;
const JET_SLOT: usize = WEAPONS.len() + 2;
const TRUCK_SLOT: usize = WEAPONS.len() + 3;
const GUN_SLOTS: usize = WEAPONS.len() + 4;

/// Weapons lying on the ground (TnDroppedPickupWeapon): standing within PICKUP_REACH
/// of one and holding E for PICKUP_HOLD_SECS takes it in place of the weapon in hand
/// ("Hold <<Pickup>> to pickup", ContextualCommands.PickupWeapon), which is gone.
const PICKUP_REACH: f32 = 3.0;
const PICKUP_HOLD_SECS: f32 = 0.5;
/// The equip time (EquipTime) of the eight guns before the Titan's, Scientist's and
/// Destroyer's (WeaponStats::equip_secs).
const EQUIP_SECS: f32 = 0.2;
/// Swap Weapons on the mouse wheel: a turn of it swaps once, and turning on swaps
/// again no sooner than this.
const SWAP_REPEAT_SECS: f32 = 0.3;
const SND_PICKUP: &str = "BL_WPN_FOLEY.PICK_UP";

fn gun(slot: usize) -> &'static Gun {
    match WEAPONS.get(slot) {
        Some(w) => &w.gun,
        None if slot == TANK_SLOT => &TANK_GUN,
        None if slot == JET_SLOT => &JET_GUN,
        None if slot == TRUCK_SLOT => &TRUCK_GUN,
        None => &VEHICLE_GUN,
    }
}
const VEHICLE_DAMAGE: f32 = 15.0;

/// Sound cues (see tools/build_sounds.py) for everything other than Jazz's guns.
const SND_VEHICLE_SHOT: &str = "BL_WPN_GUN_NEUTRON_RIFLE.VEH_SHOOT";
const SND_VEHICLE_IMPACT: &str = "BL_WPN_GUN__IMPT_BYS.IMPT_ENERGY_WORLD";
const SND_VEHICLE_HIT: &str = "BL_WPN_GUN__IMPT_BYS.IMPT_ENERGY_DMG_AI";
const SND_BOT2VEH: &str = "BL_TRANSFORM.JAZZ_BOT2VEH";
const SND_VEH2BOT: &str = "BL_TRANSFORM.JAZZ_VEH2BOT";
const SND_STEP_JOG: &str = "BL_FS_SML_BOT.FS_JOG_DEFAULT";
const SND_STEP_RUN: &str = "BL_FS_SML_BOT.FS_RUN_DEFAULT";
const SND_STEP_WALK: &str = "BL_FS_SML_BOT.FS_WALK_DEFAULT";
const SND_SPRINT_START: &str = "BL_FS_SML_BOT.FS_SPRINT_START";
const SND_JUMP: &str = "BL_FS_SML_BOT.FS_JUMP";
const SND_BOOST: &str = "BL_FS_SML_BOT.FS_JUMP_CHARGED";
const SND_LAND: &str = "BL_FS_SML_BOT.FS_LAND_DEFAULT";
const SND_LAND_HIGH: &str = "BL_FS_SML_BOT.FS_LAND_HIGH_FALL";
const SND_BASH_SERVO: &str = "BL_CHR_GRIMLOCK.ROBO_ATTACK_SHIELD_BASH_SERVO";
const SND_BASH_SWING: &str = "BL_CHR_GRIMLOCK.ROBO_ATTACK_SHIELD_BASH_SWING";
const SND_BASH_SETTLE: &str = "BL_CHR_GRIMLOCK.ROBO_FS_DEFAULT_SHIFT_MTL";
const SND_MELEE_SWING: &str = "BL_WPN_MELEE.SWORD_SWING_LT_02";
const SND_MELEE_HIT: &str = "BL_WPN_MELEE.MELEE_IMPT_DMG_TO_AI";
const SND_GRENADE_THROW: &str = "BL_WPN_GRENADE.MAGMA_DEPLOY";
const SND_HIT_REACT: &str = "BL_FS_MED_BOT.FOLEY_SERVO_HIT_RX";
const SND_PLAYER_HIT: &str = "BL_WPN_GUN__IMPT_BYS.IMPT_ENERGY_DMG";
/// The shield running out and filling up again (TnShieldHealth's ShieldDrainedSound
/// and ShieldFullSound, played for its owner only).
const SND_SHIELD_DOWN: &str = "BL_HUD_INTERFACE.OVERSHIELD_POWER_DEPLETED";
const SND_SHIELD_FULL: &str = "BL_HUD_INTERFACE.OVERSHIELD_POWER_RECHARGED";
const SND_DEATH: &str = "BL_LVL_HUD_INTERFACE.HUD_DEATH_POWER_DOWN";
const SND_BODYFALL: &str = "BL_BF_BODYFALLS.BF_DEFAULT_BODY_MEDIUM";
const SND_CAR_BOOST_START: &str = "BL_VEH_CAR.VEH_SPORT_01_BOOST_START";
const SND_CAR_BOOST_END: &str = "BL_VEH_CAR.VEH_SPORT_01_BOOST_END";
const SND_CAR_IMPACT_HEAVY: &str = "BL_VEH_CAR.VEH_SPORT_01_IMPT_HEAVY";
const SND_CAR_IMPACT_LIGHT: &str = "BL_VEH_CAR.VEH_SPORT_01_IMPT_LIGHT";
/// Jazz's car's DeathSound (each character's vehicle has its own: character.rs).
const SND_CAR_DEATH: &str = "BL_VEH_CAR.VEH_SPORT_01_DEATH";
/// Jazz's car's engine and tyre squeal cues (each character's vehicle has its own:
/// character.rs; ENGINES and SQUEALS have their loops).
const SND_ENGINE: &str = "BL_VEH_CAR.VEH_SPORT_01_DRIVE_ONLOAD";
const SND_TIRE_SQUEAL: &str = "BL_VEH_CAR.VEH_SPORT_01_TIRE_SQUEAL";

/// What one of a vehicle's engine loops follows (engine_sound): the idle, fading as
/// the car speeds up; the engine under load, coming in with speed; a bed under both;
/// an engine that plays at rest and under load alike, louder as it speeds up (the
/// trucks', whose idle and load cues play the same loops: `rest` dB at rest against
/// its level flat out); the tyres squealing in hard turns. A jet's
/// (HmPlayerVehicleAudioComponentImpl's engine states): its hover's loops while it
/// hovers (OffLoad and ForwardOnLoad, which its cues play alike), the whine on top
/// while it moves ahead (ForwardOnLoad's Auto_Onload_Whine), its hover jets' wash as
/// hard as they push (Auto_Hover_Boosters, by its Jet_Hover_Boosters parameter), and
/// while it flies its boost loops (Boosting: Auto_Boost_Loop, Auto_Boost_Whine), each
/// with its layer's level and pitch (semitones) over the seconds it has flown (its
/// VolumeCurve and PitchCurve).
#[derive(Clone, Copy, PartialEq, Debug)]
enum EngineRole {
    Idle,
    Load,
    Bed,
    Throttle {
        rest: f32,
    },
    Squeal,
    Hover,
    Whine,
    Boosters,
    Fly {
        level: &'static [(f32, f32)],
        pitch: &'static [(f32, f32)],
    },
}

/// One of a vehicle's engine loops: its wave, its level (dB) and its pitch (times
/// Jazz's car's at the same speed).
#[derive(Clone, Copy, Debug)]
struct EngineLoop {
    wave: &'static str,
    db: f32,
    pitch: f32,
    role: EngineRole,
}

impl EngineLoop {
    const fn new(role: EngineRole, wave: &'static str, db: f32, pitch: f32) -> Self {
        EngineLoop {
            wave,
            db,
            pitch,
            role,
        }
    }
    const fn idle(wave: &'static str, db: f32, pitch: f32) -> Self {
        Self::new(EngineRole::Idle, wave, db, pitch)
    }
    const fn load(wave: &'static str, db: f32, pitch: f32) -> Self {
        Self::new(EngineRole::Load, wave, db, pitch)
    }
    const fn bed(wave: &'static str, db: f32, pitch: f32) -> Self {
        Self::new(EngineRole::Bed, wave, db, pitch)
    }
    const fn throttle(wave: &'static str, db: f32, rest: f32, pitch: f32) -> Self {
        Self::new(EngineRole::Throttle { rest }, wave, db, pitch)
    }
    const fn squeal(db: f32, pitch: f32) -> Self {
        Self::squeal_of("MECH_TIRE_SQUEAL_HEAVY_LP", db, pitch)
    }
    const fn squeal_of(wave: &'static str, db: f32, pitch: f32) -> Self {
        Self::new(EngineRole::Squeal, wave, db, pitch)
    }
    const fn hover(wave: &'static str, db: f32, pitch: f32) -> Self {
        Self::new(EngineRole::Hover, wave, db, pitch)
    }
    const fn whine(wave: &'static str, db: f32, pitch: f32) -> Self {
        Self::new(EngineRole::Whine, wave, db, pitch)
    }
    const fn boosters(wave: &'static str, db: f32, pitch: f32) -> Self {
        Self::new(EngineRole::Boosters, wave, db, pitch)
    }
    const fn fly(
        wave: &'static str,
        db: f32,
        pitch: f32,
        level: &'static [(f32, f32)],
        bend: &'static [(f32, f32)],
    ) -> Self {
        Self::new(EngineRole::Fly { level, pitch: bend }, wave, db, pitch)
    }
}

/// A curve's value at `t` (its keys' straight lines between them, held past its
/// ends; `default` for none).
fn curve_at(keys: &[(f32, f32)], t: f32, default: f32) -> f32 {
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
        return default;
    };
    if t <= first.0 {
        return first.1;
    }
    for pair in keys.windows(2) {
        let ((t0, v0), (t1, v1)) = (pair[0], pair[1]);
        if t <= t1 {
            return v0 + (v1 - v0) * ((t - t0) / (t1 - t0).max(1e-6));
        }
    }
    last.1
}

/// The jets' boost loops' curves over the seconds they have flown (their cues'
/// layers' VolumeCurve and PitchCurve): VEH_JET_01_BOOST_LOOP's engine, settling
/// over 25 s and rising in pitch over 20 (its second, lower layer falling), its
/// whine coming in over a second and a half and dipping after three, and the
/// Insecticons' beetle loop rising.
const BOOST_LEVEL: &[(f32, f32)] = &[(3.0, 1.0), (15.0, 0.501), (25.0, 0.355)];
const BOOST_RISE: &[(f32, f32)] = &[(1.0, 0.0), (20.0, 3.0)];
const BOOST_FALL: &[(f32, f32)] = &[(1.0, 0.0), (20.0, -3.0)];
const BOOST_WHINE_LEVEL: &[(f32, f32)] = &[
    (0.0, 0.0),
    (1.5, 1.0),
    (3.0, 1.0),
    (6.0, 0.251),
    (15.0, 0.708),
];
const BOOST_WHINE_RISE: &[(f32, f32)] = &[(0.25, 0.0), (3.0, 1.0)];
const INSECT_LEVEL: &[(f32, f32)] = &[(0.0, 0.633), (1.28, 1.0)];
const INSECT_RISE: &[(f32, f32)] = &[(0.0, 0.0), (0.68, 2.0)];
const FLAT: &[(f32, f32)] = &[];

/// The vehicles' engines by their engine cue (their sound set's
/// Auto_Engine_Gear_1_OnLoad, VEH_*_DRIVE_ONLOAD): the loops of the cue the demo
/// mixes by speed. Jazz's car's (VEH_SPORT_01, his and Sideswipe's) is set by ear.
/// The others are pitched against it as their cues' and layers' pitches are against
/// his (+2 semitones), and levelled so that, against his, each is as loud as in the
/// game at rest (its DRIVE_OFFLOAD cue) and flat out (DRIVE_ONLOAD's loop), the
/// recordings' own loudness counted in.
const ENGINES: [(&str, &[EngineLoop]); 10] = [
    (
        SND_ENGINE,
        &[
            EngineLoop::idle("AUTO_SPORT_01_ENGINE_MAIN_LOW_LP_01", -13.0, 1.0),
            EngineLoop::load("AUTO_SPORT_01_ENGINE_MAIN_MID_LP_02", -14.0, 1.0),
        ],
    ),
    // Bumblebee's, Dragstrip's and Wheeljack's.
    (
        "BL_VEH_CAR.VEH_SPORT_02_DRIVE_ONLOAD",
        &[
            EngineLoop::idle("AUTO_SPORT_02_ENGINE_IDLE_LP", -16.5, 0.944),
            EngineLoop::load("AUTO_SPORT_02_ENGINE_MAIN_LP", -17.5, 0.667),
            EngineLoop::bed("ELEC_ENERGY_COMP_PITCHADJ_LP_01", -23.0, 0.667),
        ],
    ),
    // Breakdown's and Deadend's.
    (
        "BL_VEH_CAR.VEH_MUSCLE_01_DRIVE_ONLOAD",
        &[
            EngineLoop::idle("AUTO_MUSCLE_01_ENGINE_IDLE_LP", -19.5, 0.891),
            EngineLoop::load("AUTO_MUSCLE_01_ENGINE_MAIN_LP", -20.5, 0.891),
            EngineLoop::bed("ELEC_ENERGY_COMP_PITCHADJ_LP_01", -23.0, 0.63),
        ],
    ),
    // The Titan's tanks' (VEH_TANK_01): their treads and turret only under load (its
    // DRIVE_OFFLOAD has neither). (The Dinobots' VEH_DINOBOT set names the same
    // loops, but their audio components play no engine: car_engine.)
    (
        "BL_VEH_TANK.VEH_TANK_01_DRIVE_ONLOAD",
        &[
            EngineLoop::idle("MECH_ENGINE_MOD_LOW_LP_01", -14.5, 0.891),
            EngineLoop::bed("MECH_TANK_ENGINE_COMP_LP", -15.0, 0.891),
            EngineLoop::bed("ELEC_TANK_ENERGY_COMP_LP", -15.0, 0.891),
            EngineLoop::load("TANK_TREAD_FAST_LP", -23.5, 0.749),
            EngineLoop::load("TANK_TURRET_LP_01", -19.0, 0.794),
        ],
    ),
    // Swindle's and Hound's jeeps' (VEH_JEEP_01): its idle and load cues play the
    // same loops, louder under load (DRIVE_OFFLOAD's -8 dB against DRIVE_ONLOAD's 0),
    // and much louder than Jazz's car, as are the trucks' below.
    (
        "BL_VEH_TRUCK.VEH_JEEP_01_DRIVE_ONLOAD",
        &[
            EngineLoop::throttle("AUTO_JEEP_01_ENGINE_HOVER_LP_01", -1.5, -9.0, 0.891),
            EngineLoop::throttle("JET_TURBO_WHINE_LP", -17.5, -9.0, 0.891),
        ],
    ),
    // Optimus Prime's, G1 Optimus's and Ultra Magnus's trucks' (VEH_TRUCK_01).
    (
        "BL_VEH_TRUCK.VEH_TRUCK_01_DRIVE_ONLOAD",
        &[
            EngineLoop::throttle("AUTO_TRUCK_01_ENGINE_IDLE_LP", -13.5, -3.5, 0.891),
            EngineLoop::throttle("AUTO_TRUCK_01_ENGINE_HOVER_LP", -13.5, -3.5, 0.891),
        ],
    ),
    // Ratchet's, Ironhide's, Soundwave's, Onslaught's and Zeta Prime's (VEH_TRUCK_02).
    (
        "BL_VEH_TRUCK.VEH_TRUCK_02_DRIVE_ONLOAD",
        &[
            EngineLoop::throttle("AUTO_TRUCK_02_ENGINE_MAIN_LP", -11.5, -5.0, 0.891),
            EngineLoop::throttle("SYNTH_ENERGY_MOD_PAN_LP", -23.5, -5.0, 0.891),
        ],
    ),
    // The Scientist's jets (VEH_JET_01: Jetfire's, Starscream's, Air Raid's,
    // Silverbolt's, Shockwave's, Blastoff's and Swoop's): its cues' layers at their
    // levels, 1.5 dB down as the trucks' (HOVER_FLIGHT_ONLOAD, _ONLOAD_WHINE,
    // HOVER_BOOSTERS, BOOST_LOOP and BOOST_LOOP_WHINE), each at its layer's pitch.
    (
        "BL_VEH_JET.VEH_JET_01_HOVER_FLIGHT_ONLOAD",
        &[
            EngineLoop::hover("JET_HOVER_LP", -10.5, 1.0),
            EngineLoop::hover("JET_ENGINE_WASH_LP", -13.5, 1.0),
            EngineLoop::whine("JET_TURBO_WHINE_LP", -16.5, 1.0),
            EngineLoop::boosters("WHSH_JET_WASH_BOOSTERS_LP", -1.5, 1.0),
            EngineLoop::fly(
                "JET_FLYER_01_ENGINE_BOOST_LP",
                -13.5,
                0.944,
                BOOST_LEVEL,
                BOOST_RISE,
            ),
            EngineLoop::fly(
                "JET_FLYER_01_ENGINE_BOOST_LP",
                -19.5,
                0.667,
                BOOST_LEVEL,
                BOOST_FALL,
            ),
            EngineLoop::fly(
                "JET_TURBO_WHINE_LP",
                -13.5,
                1.335,
                BOOST_WHINE_LEVEL,
                BOOST_WHINE_RISE,
            ),
        ],
    ),
    // Vortex's helicopter (VEH_JET_GUNSHIP_01): its rotors and engine, hovering and
    // flying (no whine or hover jets).
    (
        "BL_VEH_JET.VEH_JET_GUNSHIP_01_HOVER_FLIGHT_ONLOAD",
        &[
            EngineLoop::hover("JET_HELI_01_ENGINE_BOOST_LOW_LP", -8.5, 1.0),
            EngineLoop::hover("JET_HELI_01_ENGINE_BOOST_LP", -8.5, 1.0),
            EngineLoop::hover("JET_HELI_01_ENGINE_MAIN_LP", -8.5, 1.0),
            EngineLoop::fly("JET_HELI_01_ENGINE_BOOST_LOW_LP", -11.5, 1.0, FLAT, FLAT),
            EngineLoop::fly("JET_HELI_01_ENGINE_BOOST_LP", -10.5, 1.0, FLAT, FLAT),
        ],
    ),
    // Sharpshot's and Kickback's beetles (VEH_INSECTICON): their wings' buzz,
    // hovering, and flying the jets' boost loops with their beetle loops.
    (
        "BL_VEH_JET.VEH_INSECTICON_FLYING_LP",
        &[
            EngineLoop::hover("VEH_INSECT_FLY_LP_02", -8.5, 1.0),
            EngineLoop::hover("VEH_INSECT_IDLE_LP", -13.5, 1.0),
            EngineLoop::hover("VEH_INSECT_LP_LOW", -8.5, 1.0),
            EngineLoop::boosters("WHSH_JET_WASH_BOOSTERS_LP", -1.5, 1.0),
            EngineLoop::fly(
                "JET_FLYER_01_ENGINE_BOOST_LP",
                -13.5,
                0.944,
                BOOST_LEVEL,
                BOOST_RISE,
            ),
            EngineLoop::fly(
                "JET_FLYER_01_ENGINE_BOOST_LP",
                -16.5,
                0.841,
                BOOST_LEVEL,
                BOOST_FALL,
            ),
            EngineLoop::fly(
                "JET_TURBO_WHINE_LP",
                -12.5,
                1.335,
                BOOST_WHINE_LEVEL,
                BOOST_WHINE_RISE,
            ),
            EngineLoop::fly("VEH_INSECT_LP_01", -13.5, 0.841, INSECT_LEVEL, INSECT_RISE),
            EngineLoop::fly("VEH_INSECT_LP_LOW", -10.5, 0.841, FLAT, FLAT),
        ],
    ),
];

/// The tyre squeals by their cue (Auto_Tire_Squeal_Default's): the one loop, as loud
/// and as high against Jazz's as the game's cues play it. (Snarl's and Slug's walkers
/// have none; the jeeps skid on dirt.)
const SQUEALS: [(&str, EngineLoop); 6] = [
    (SND_TIRE_SQUEAL, EngineLoop::squeal(-15.0, 1.0)),
    (
        "BL_VEH_CAR.VEH_SPORT_02_TIRE_SQUEAL",
        EngineLoop::squeal(-11.0, 0.841),
    ),
    (
        "BL_VEH_CAR.VEH_MUSCLE_01_TIRE_SQUEAL",
        EngineLoop::squeal(-8.0, 1.0),
    ),
    (
        "BL_VEH_TRUCK.VEH_JEEP_01_TIRE_SQUEAL",
        EngineLoop::squeal_of("AUTO_JEEP_01_TIRE_SKID_DIRT_LP", -14.0, 0.841),
    ),
    (
        "BL_VEH_TRUCK.VEH_TRUCK_01_TIRE_SQUEAL",
        EngineLoop::squeal(-10.0, 0.841),
    ),
    (
        "BL_VEH_TRUCK.VEH_TRUCK_02_TIRE_SQUEAL",
        EngineLoop::squeal(-16.0, 0.944),
    ),
];

/// A vehicle's engine loops and its tyre squeal, if any, by their cues (Jazz's car's
/// for cues ENGINES or SQUEALS doesn't have).
fn engine_loops(engine: Option<&str>, squeal: Option<&str>) -> Vec<EngineLoop> {
    // (None: a vehicle without an engine, a Dinobot.)
    let loops = engine.map_or(&[][..], |engine| {
        ENGINES
            .iter()
            .find(|(cue, _)| *cue == engine)
            .map_or(ENGINES[0].1, |(_, loops)| *loops)
    });
    let squeal = squeal.map(|s| {
        SQUEALS
            .iter()
            .find(|(cue, _)| *cue == s)
            .map_or(SQUEALS[0].1, |(_, l)| *l)
    });
    loops.iter().copied().chain(squeal).collect()
}
// (The metres a robot covers per footstep, jogging, sprinting and walking in fine
// aim, are its class's: Kit::robot, character.rs.)

/// Cover pillars (piston towers): position on the ground and radius for collision.
const PISTON_SCALE: f32 = 1.6;
const PISTON_RADIUS: f32 = 1.6;
const PISTON_HEIGHT: f32 = 5.4 * PISTON_SCALE;

/// Animation clips inside robot.glb, in the order tools/build_assets.py writes them.
const ANIM_IDLE: usize = 0;
const ANIM_JOG_FL: usize = 1;
// 2..=6: jogging FR, L, R, BL, BR (each stance's moving clips use this order).
const ANIM_SPRINT: usize = 7;
const ANIM_TO_VEHICLE: usize = 8;
const ANIM_TO_ROBOT: usize = 9;
const ANIM_LAND: usize = 10;
const ANIM_BOOST_F: usize = 11;
const ANIM_BOOST_B: usize = 12;
const ANIM_BOOST_L: usize = 13;
const ANIM_BOOST_R: usize = 14;
const ANIM_MELEE: usize = 15;
const ANIM_MELEE_BOOST: usize = 16;
const ANIM_AIR_BOOST_L: usize = 17;
const ANIM_AIR_BOOST_R: usize = 18;
const ANIM_LAND_HARD: usize = 19;
const ANIM_THROW: usize = 20;
const ANIM_HIT: usize = 21;
const ANIM_DEATH: usize = 22;
const ANIM_AIR_BOOST_F: usize = 23;
const ANIM_AIR_BOOST_B: usize = 24;
/// Weapon stances: idle, then moving FL, FR, L, R, BL, BR (the same order as the
/// ANIM_JOG_* clips). Shooting raises the gun from the hip; fine aim brings it up
/// to the sights, and Jazz walks instead of jogging.
const ANIM_SHOOT_IDLE: usize = 25;
const ANIM_SHOOT_MOVE: usize = 26;
const ANIM_AIM_IDLE: usize = 32;
const ANIM_AIM_MOVE: usize = 33;
/// Jumping and falling: Jazz's acrobatics (TR_Acrobatics_p.Bumblebee_JazzAcrobatics)
/// jump with Nav_TakeOff_01 into the Nav_Jump_Descent loop, and fall (stepping off
/// a ledge) with Nav_Jump_Descent_02.
const ANIM_JUMP: usize = 39;
const ANIM_JUMP_DESCENT: usize = 40;
const ANIM_FALL: usize = 41;
/// Nav_TakeOff_01 starts with a crouch held for 0.1 s; Jazz is already leaving the
/// ground then, so it plays from just after it. The descent takes over at its end.
const JUMP_TAKEOFF_SKIP: f32 = 0.1;
/// (The take-off plays its class's Nav_TakeOff_01 to its end: Robot::takeoff_secs,
/// Jazz's 0.67 s.)
fn jump_takeoff_secs(kit: &character::Kit) -> f32 {
    kit.robot.takeoff_secs - JUMP_TAKEOFF_SKIP
}
/// Off the ground this long without jumping before the falling clip plays.
const FALL_DELAY: f32 = 0.2;
/// Additive clips, layered on top of whatever else plays (see setup_player).
const ANIM_RIFLE_FIRE: usize = 42;
const ANIM_RIFLE_COOLDOWN: usize = 43;
const ANIM_RIFLE_IDLE: usize = 44;
const ANIM_ION_FIRE: usize = 45;
const ANIM_ION_IDLE: usize = 46;
const ANIM_RIFLE_RELOAD: usize = 47;
const ANIM_ION_RELOAD: usize = 48;
/// The Photon Burst Rifle's: one burst's recoil (played once per burst), its idle
/// loop and its reload.
const ANIM_BURST_FIRE: usize = 49;
const ANIM_BURST_IDLE: usize = 50;
const ANIM_BURST_RELOAD: usize = 51;
/// The Scatter Blaster's: one shot's recoil (restarted by every shot), its idle loop
/// and its reload.
const ANIM_SCATTER_FIRE: usize = 52;
const ANIM_SCATTER_IDLE: usize = 53;
const ANIM_SCATTER_RELOAD: usize = 54;
/// The Nucleon Charge Rifle's: one shot's recoil (the gun's Fire_WEP with the arm's
/// recoil, 37 frames), its idle loop and its reload.
const ANIM_SNIPER_FIRE: usize = 55;
const ANIM_SNIPER_IDLE: usize = 56;
const ANIM_SNIPER_RELOAD: usize = 57;
/// The Path Blaster's: one shot's recoil (the gun's Fire_WEP with the arm's recoil,
/// 14 frames, restarted by every shot), its idle loop and its reload.
const ANIM_PATH_FIRE: usize = 58;
const ANIM_PATH_IDLE: usize = 59;
const ANIM_PATH_RELOAD: usize = 60;
/// The Riot Cannon's: one shot's recoil (the gun's RiotCannon_Fire with the arm's
/// recoil, 30 frames, restarted by every shot), its idle loop and its reload.
const ANIM_RIOT_FIRE: usize = 61;
const ANIM_RIOT_IDLE: usize = 62;
const ANIM_RIOT_RELOAD: usize = 63;
/// The Orbital Pistol's: one shot's recoil (the gun's SysCorruptor_Fire_WEP with the
/// arm's recoil, 15 frames, restarted by every shot), its idle loop and its reload.
const ANIM_ORBITAL_FIRE: usize = 64;
const ANIM_ORBITAL_IDLE: usize = 65;
const ANIM_ORBITAL_RELOAD: usize = 66;
const ANIM_FIRST_ADDITIVE: usize = ANIM_RIFLE_FIRE;
const ANIM_END_ADDITIVE: usize = 67;
/// The rest of each stance's strafe set (Shared_SCOUT_ROBO_ANIM): four clips per
/// stance (relaxed jog, shooting jog, fine-aim walk, in that order): straight
/// forward, straight back, and left / right with the hips turned back (LB, RB).
const ANIM_STRAFE_EXTRA: usize = 67;
const STRAFE_F: usize = 0;
const STRAFE_B: usize = 1;
const STRAFE_LB: usize = 2;
const STRAFE_RB: usize = 3;
/// Turning on the spot (Shared_ROBO_NAV_ANIM's IdlePivot clips): a 90-degree turn
/// left, then right, per stance (relaxed, shooting, fine aim), then the shooting
/// stance's 180-degree turns, left and right.
const ANIM_PIVOT: usize = 79;
const ANIM_PIVOT_180: usize = 85;
/// Additive: the upper body turned towards the camera, left then right per stance
/// (from the game's aim offset poses, Aim_<stance>_L_C / _R_C, a full turn each).
/// No longer played: robot_aim.rs puts the game's own aim offset node on his bones
/// (up and down too); they keep their place so that the clips after them keep
/// their numbers.
const ANIM_TWIST: usize = 87;
/// Additive: switching weapons, equip then unequip for each weapon in WEAPONS order
/// (see WeaponStats::equip_anim).
const ANIM_EQUIP: usize = 93;
/// Additive: switching hands (the gun's MirrorReverse_WEP), one per weapon in
/// WEAPONS order.
const ANIM_HAND_SWITCH: usize = 109;
/// Additive: Jazz's forearms changing over as he switches hands (his arm parts'
/// ADD_Weapon_MirrorReverse_ARM), played with the gun's (see fold_forearms).
const ANIM_ARM_SWITCH: usize = 117;
/// Additive: the Titan's, Scientist's and Destroyer's guns' clips, six each in
/// WEAPONS order from the X18 Scrapmaker on (tools/build_assets.py's NEW_WEAPONS):
/// firing (its loop, a shot's or a burst's), idle, reload, equip, unequip and
/// switching hands (its MirrorReverse); then their one-offs: the X18 Scrapmaker's
/// cool-down and warm-up, the TechVolt's cool-down, the Gear Shredder's charged
/// shot and its charging loop.
const ANIM_NEW_GUNS: usize = 118;
const ANIM_HEAVYMG_COOLDOWN: usize = 166;
const ANIM_HEAVYMG_WARMUP: usize = 167;
const ANIM_TECHVOLT_COOLDOWN: usize = 168;
/// The Gear Shredder's charged shot (ChargedFire_WEP with the arm's recoil) and its
/// charging loop (Charging_WEP, from StartCharging to StopCharging).
const ANIM_SHREDDER_CHARGED_FIRE: usize = 169;
const ANIM_SHREDDER_CHARGING: usize = 170;
/// The heavy-gun stance (STANCE_HeavyGunArmShootFromHip: Shared_SCOUT_ROBO_ANIM's
/// HeavyWep clips, or a class's own): standing, then jogging and walking in the ten
/// strafe directions each (FL, FR, L, R, BL, BR, F, B, LB, RB: the game's
/// WalkJogBlend blends the two by speed), then its turns on the spot (90 degrees
/// left and right, 180 left and right).
const ANIM_HEAVY_IDLE: usize = 171;
const ANIM_HEAVY_JOG: usize = 172;
const ANIM_HEAVY_WALK: usize = 182;
const ANIM_HEAVY_PIVOT: usize = 192;
const ANIM_HEAVY_PIVOT_180: usize = 194;
const ANIM_COUNT: usize = 196;
/// The clips the characters bring (models/<class>/clips.glb, character.rs), after
/// robot.glb's: each vehicle's halves of the transformation (to the vehicle, to the
/// robot) from the first, then the forearm and shoulder sets' clips (additive) from
/// character::TRANSFORM_SLOTS on.
const ANIM_EXTRA: usize = character::EXTRA_SLOTS;
const ANIM_TOTAL: usize = ANIM_COUNT + ANIM_EXTRA;

/// A clips.glb clip in the robot's graph.
fn extra_clip(clip: usize) -> usize {
    ANIM_COUNT + clip.min(ANIM_EXTRA - 1)
}

/// Whether a clip folds the robot up into the vehicle (Jazz's own or a
/// character's), or unfolds him.
fn is_to_vehicle(clip: usize) -> bool {
    clip == ANIM_TO_VEHICLE
        || (ANIM_COUNT..ANIM_COUNT + character::TRANSFORM_SLOTS).contains(&clip)
            && (clip - ANIM_COUNT) % 2 == 0
}

fn is_to_robot(clip: usize) -> bool {
    clip == ANIM_TO_ROBOT
        || (ANIM_COUNT..ANIM_COUNT + character::TRANSFORM_SLOTS).contains(&clip)
            && (clip - ANIM_COUNT) % 2 == 1
}
/// The guns' Equip_WEP and Unequip_WEP clips both last a second (30 frames).
const EQUIP_ANIM_SECS: f32 = 1.0;
/// The forearm sets' ADD_Weapon_Equip_ARM and Unequip_ARM, played as the gun goes
/// away and comes back (holsters), last a second too.
const HOLSTER_ARM_SECS: f32 = 1.0;

/// Whether a Jazz's gun is put away: as the game's TnWeapon.ShouldHolster has it,
/// while he sprints (his acrobatics' Sprinting state: running on the ground, not
/// jumping or falling, TnAcrobaticsService.IsSprinting) or gives a teammate a
/// revive (IsGivingRevive); only in robot form, alive. (In the game a reload keeps
/// the gun where it is; here a reload brings it out, to be seen reloading.) A gun
/// put away (TnWeapon.Holster, its Active state's Tick) plays its UnEquip effects
/// (its Unequip_WEP on the CloudB gun bones, its holster sound) and hides; the
/// forearm on its arm comes out (TnArmAttachmentManager: no visible weapon, so its
/// forearm parts show and play their ADD_Weapon_Equip_ARM). Taken out (Unholster)
/// it shows and plays its Equip effects, the forearm folding away on the CloudB arm
/// bones (ADD_Weapon_Unequip_ARM) and hiding as that ends. None outside robot
/// form, through a transformation or dead: the gun is his form's business then
/// (back in hand at once, as it comes back after a transformation).
fn holsters(player: &Player) -> Option<bool> {
    let robot = player.form == Form::Robot
        && player.transform_left <= 0.0
        && player.pending_form.is_none()
        && player.dead_left <= 0.0
        && !player.dying;
    let on_the_ground =
        player.jump_time.is_none() && (player.grounded || player.air_time <= FALL_DELAY);
    let sprinting = on_the_ground && sprint_run(player.sprinting, player.move_input);
    robot.then_some(player.reload_left <= 0.0 && (sprinting || player.revive.healing.is_some()))
}
/// Switching hands (C, the game's ChangeHandedness): Jazz's whole animation is
/// mirrored left for right (TnAnimNodeHandednessMirror), blending over BlendTime
/// (0.5 s) to the mirrored pose, while the gun folds into one forearm and unfolds out
/// of the other (MirrorReverse_WEP) over SwitchingHandsTime (1 s), when it can't fire.
/// The camera moves over to the other shoulder (OverTheShoulder_STRATEGY: the offset's
/// side flips, smoothed over 0.5 s).
const MIRROR_BLEND_SECS: f32 = 0.5;
const HAND_SWITCH_SECS: f32 = 1.0;
const SHOULDER_SMOOTH_SECS: f32 = 0.5;

/// The clip that is the mirror image of a clip: the left and right versions of a
/// move swap (a strafe to the right, mirrored, is a strafe to the left); the rest
/// are their own.
fn mirror_clip(clip: usize) -> usize {
    let pair = |base: usize| {
        if (clip - base) % 2 == 0 {
            clip + 1
        } else {
            clip - 1
        }
    };
    match clip {
        // FL / FR, L / R, BL / BR of each stance's moves.
        ANIM_JOG_FL..=6 => pair(ANIM_JOG_FL),
        c if (ANIM_SHOOT_MOVE..ANIM_SHOOT_MOVE + 6).contains(&c) => pair(ANIM_SHOOT_MOVE),
        c if (ANIM_AIM_MOVE..ANIM_AIM_MOVE + 6).contains(&c) => pair(ANIM_AIM_MOVE),
        ANIM_BOOST_L => ANIM_BOOST_R,
        ANIM_BOOST_R => ANIM_BOOST_L,
        ANIM_AIR_BOOST_L => ANIM_AIR_BOOST_R,
        ANIM_AIR_BOOST_R => ANIM_AIR_BOOST_L,
        // The hips-back sides of the strafe set (LB / RB).
        c if (ANIM_STRAFE_EXTRA..ANIM_PIVOT).contains(&c) => match (c - ANIM_STRAFE_EXTRA) % 4 {
            STRAFE_LB => c + 1,
            STRAFE_RB => c - 1,
            _ => c,
        },
        // Turning on the spot and the upper-body turns: left / right pairs.
        c if (ANIM_PIVOT..ANIM_EQUIP).contains(&c) => pair(ANIM_PIVOT),
        // The heavy-gun stance's jog and walk: FL / FR, L / R and BL / BR pairs, F and
        // B their own, LB / RB; and its turns, left / right pairs.
        c if (ANIM_HEAVY_JOG..ANIM_HEAVY_PIVOT).contains(&c) => match (c - ANIM_HEAVY_JOG) % 10 {
            6 | 7 => c,
            k if k % 2 == 0 => c + 1,
            _ => c - 1,
        },
        c if (ANIM_HEAVY_PIVOT..ANIM_COUNT).contains(&c) => pair(ANIM_HEAVY_PIVOT),
        c => c,
    }
}
/// The pivot clips' lengths and how far each turns Jazz (left is positive; the
/// game's 180-degree right turn stops a little short). The turn is even over the
/// clip, as the clips turn their reference bone.
const PIVOT_SECS: f32 = 18.0 / 30.0;
const PIVOT_180_SECS: f32 = 22.0 / 30.0;
const PIVOT_TURN: f32 = FRAC_PI_2;
const PIVOT_180_TURN_L: f32 = PI;
const PIVOT_180_TURN_R: f32 = -172.3 * PI / 180.0;
/// Standing still, Jazz's feet stay put while the camera turns until it is this far
/// round from where he faces (radians); then he steps round with a pivot clip. In
/// the shooting stance, past PIVOT_180_START he turns right round.
const PIVOT_START: f32 = 75.0 * PI / 180.0;
const PIVOT_180_START: f32 = 135.0 * PI / 180.0;
/// While the camera is still swinging round faster than this (radians a second),
/// Jazz waits to see how far it goes (up to PIVOT_180_START) before he turns, so a
/// quick swing right round gets one big turn rather than a 90 that falls short.
const PIVOT_WAIT_SWING: f32 = 2.0;
/// How fast Jazz's facing catches up with the camera when he isn't standing still
/// (moving, jumping, a move playing): the fraction of the gap closed per second, as
/// an exponential rate.
const FACE_CAMERA_RATE: f32 = 20.0;
/// Strafing turns the hips back (to the LB / RB clips) when the movement swings past
/// this many degrees from straight ahead, and forward again under HIP_FLIP_FRONT
/// (the game's TnStraferAnimBlend flips the hips the same way; the gap keeps them
/// from flipping back and forth around the side).
const HIP_FLIP_BACK: f32 = 110.0;
const HIP_FLIP_FRONT: f32 = 70.0;

fn is_additive(clip: usize) -> bool {
    (ANIM_FIRST_ADDITIVE..ANIM_END_ADDITIVE).contains(&clip)
        || (ANIM_TWIST..ANIM_HEAVY_IDLE).contains(&clip)
        || clip >= ANIM_COUNT + character::TRANSFORM_SLOTS
}

/// Jazz turning on the spot: the clip, the facing he started from, how far it
/// turns him, the seconds into it and its playback speed.
#[derive(Clone, Copy)]
struct Pivot {
    clip: usize,
    from: f32,
    turn: f32,
    secs: f32,
    t: f32,
    speed: f32,
}

/// Which weapon stance Jazz is in: 1 shooting from the hip, 2 fine aim (in fine
/// aim, his weapon's FineAimStance: the Nucleon Charge Rifle is held from the hip).
/// Never 0, the relaxed stance (STANCE_GunArmIdle, his gun lowered): the game's
/// weapons only take their IdleStance after a while unused in the campaign
/// (TnWeapon.ShouldUseIdleStance is false outside it), so in Escalation his gun is
/// always up, ready, in its BaseStance (STANCE_GunArmShootFromHip, a heavy
/// weapon's STANCE_HeavyGunArmShootFromHip).
fn stance(player: &Player) -> usize {
    if player.aiming {
        WEAPONS.get(player.weapon).map_or(2, |w| w.fine_aim_stance)
    } else {
        1
    }
}

/// The game's stance (ETnStance) Jazz's robot anim tree's aim offset node takes its
/// profile by (robot_aim.rs): his weapon's (a heavy weapon's BaseStance and
/// FineAimStance are both STANCE_HeavyGunArmShootFromHip), or the hover's.
fn game_stance(player: &Player, heavy: bool) -> u8 {
    let hovering = player.ability.kind == abilities::Kind::Hover
        && player.ability.hover.phase == hoverslam::Phase::Hovering;
    if hovering {
        robot_aim::HOVER
    } else if heavy {
        robot_aim::HEAVY_GUN_ARM_SHOOT_FROM_HIP
    } else if stance(player) == 2 {
        robot_aim::GUN_ARM_FINE_AIM
    } else {
        robot_aim::GUN_ARM_SHOOT_FROM_HIP
    }
}

/// The signed angle from `from` round to `to` (radians, -PI..PI; positive is left).
fn angle_to(from: f32, to: f32) -> f32 {
    (to - from + PI).rem_euclid(TAU) - PI
}
/// Length of the rifle's barrel spin-down after a burst.
const RIFLE_COOLDOWN_SECS: f32 = 1.03;
/// The speed the transformation animations play at: their own, as the game's.
/// TnTransformation.Execute plays both halves at a Rate of 1 (4 only for a downed
/// Transformer's quick change), in a match and on the Customize screen alike. How
/// long they take, and when the robot and the car show and vanish, is each
/// vehicle's (Player::kit's timing: the clips' ToggleHidden notifies; character.rs).
const TRANSFORM_SPEED: f32 = 1.0;
/// The half of a transformation seen as it starts (the robot's folding up, the
/// car's unfolding) blends in from the pose it was in over this long
/// (TnTransformation's BlendInTime); the other half, hidden as it starts, cuts in.
const TRANSFORM_BLEND_IN: f32 = 0.115;
/// The half of a transformation unfolding into the robot blends out over its last
/// this long into his own clips (TnTransformation's BlendOutTime).
const TRANSFORM_BLEND_OUT: f32 = 0.25;
/// How quickly the car's speed bleeds off while Jazz unfolds back into a robot.
const UNFOLD_SLOWDOWN: f32 = 2.5;
/// How far into a transformation the weapons of the form he is becoming come out,
/// and every weapon he carries is reloaded (TnPawn's TransformToAltShootingDelay
/// and TransformToRobotShootingDelay, 1 s each: TransformingToAlt's and
/// TransformingToRobot's SwitchToFormWeapon, on that timer, or as the change ends
/// if it ends sooner; reload_all_weapons). Every vehicle's change takes longer.
const FORM_WEAPON_SECS: f32 = 1.0;

/// A character's halves of the transformation, as robot graph clips (folding up,
/// unfolding): his vehicle's (clips.glb), or Jazz's own in robot.glb.
fn transform_clips(kit: &character::Kit) -> (usize, usize) {
    match (kit.to_vehicle, kit.to_robot) {
        (Some(v), Some(r)) => (extra_clip(v), extra_clip(r)),
        _ => (ANIM_TO_VEHICLE, ANIM_TO_ROBOT),
    }
}

/// Fine aim (hold the right mouse button). The game's over-the-shoulder camera
/// narrows from 80 to 50 degrees (horizontal field of view); scoped weapons zoom
/// further (WeaponStats::fine_aim_fov). Its TnFovCameraBehavior eases in to the fine
/// aim's field of view over SmoothTime 0.1 and back out over its default's 0.15
/// (Car_OverTheShoulder_STRATEGY), with HmC2Smoother: a critically damped spring at
/// a rate of 4 / SmoothTime (smooth_damp with half the SmoothTime). The game's
/// videos have the Nucleon Charge Rifle's scope halfway in after 0.04 s and all
/// but in after 0.17 s, as that has it.
const CAMERA_FOV: f32 = 80.0;
/// The camera's pitch as a match starts (radians: a little down).
const START_PITCH: f32 = -0.18;
/// How far the view looks down and up on foot (radians): every class's over-the-
/// shoulder strategy's (Car_MP_, Truck_ and Plane_OverTheShoulder_STRATEGY's
/// TnOrbitRotationCameraBehavior: its PitchRange, 60 degrees down to 55 up, in fine
/// aim too). (In a vehicle the demo keeps its own: VEHICLE_PITCH_RANGE.)
const ROBOT_PITCH_RANGE: (f32, f32) = (-60.0 * PI / 180.0, 55.0 * PI / 180.0);
const VEHICLE_PITCH_RANGE: (f32, f32) = (-1.2, 0.6);
/// How far a tank's view looks down and up (radians): its camera strategy's
/// (MP_Tank_STRATEGY's TnOrbitRotationCameraBehavior: PitchRange, 30 degrees down
/// to 30 up, from the tank's own pitch, PitchRelativeToTargetActor). Its cannon fires
/// where the view looks, so it can't be aimed at the ground at its feet, as the
/// demo's tanks could (down to 69 degrees, VEHICLE_PITCH_RANGE), firing into their
/// own blast.
const TANK_PITCH_RANGE: (f32, f32) = (-30.0 * PI / 180.0, 30.0 * PI / 180.0);
const FINE_AIM_FOV: f32 = 50.0;
const FOV_SMOOTH_TIME: f32 = 0.1;
const FOV_SMOOTH_OUT: f32 = 0.15;
/// The Nucleon Charge Rifle's scoped camera (WeaponStats::scope_camera, the class's
/// character::Camera::scope): Jazz's Car_OverTheShoulder_STRATEGY's offsets for its
/// fine aim (400, 100, 200 at its orbit distance of 575, the same at any pitch),
/// 1.75 m behind the middle of his body, 1 m out to the side and 2 m up: close
/// behind his head, which at 25 degrees leaves him out of the picture (as the game's
/// videos show; bigger armor would reach into it, so fade_near_camera fades him
/// out); the Destroyer's, its Truck_OverTheShoulder_STRATEGY's (500, 155, 350 at
/// 875). It moves in with the scope's offsets' SmoothTime (0.1) and back out with
/// the default offsets' (0.5), on the same smoother.
const SCOPE_IN_SECS: f32 = 0.1;
/// The vehicle's camera moves between where it follows it driving and hovering
/// over about this long (the hover strategies' TransitionTime, 1.5 s; on the
/// camera's smoother, as the field of view).
const HOVER_CAMERA_SECS: f32 = 1.5;
/// Changing form, the camera moves from where it was to the new form's place over
/// this long, easing in and out (the transformations' camera strategies'
/// TransitionTime, HoverCar_Bumblebee_STRATEGY's, HoverTruck_Optimus_STRATEGY's and
/// OverTheShoulder_STRATEGY's).
const FORM_CAMERA_SECS: f32 = 1.5;
const SCOPE_OUT_SECS: f32 = 0.5;
/// Jazz walks while aiming: the fine-aim walk cycle covers 0.41 times the ground
/// of the jog per second.
const FINE_AIM_SPEED: f32 = 5.0;
/// The heavy-gun stance's WalkJogBlend (the robot anim tree's TnVelocityAnimBlend):
/// all walk at this speed and slower (450 units a second), all jog from this one
/// (1200), the two blended between.
const HEAVY_WALK_SPEED: f32 = 4.5;
const HEAVY_JOG_SPEED: f32 = 12.0;

/// Dodge (Dash/Sprint, tapped: the game's Dodge, TnAcrobaticsManager's
/// DodgingOnGround and Dodging states): for DodgeTime Jazz moves at DodgeSpeed one
/// of four ways from where he faces (forward or back when the movement keys push
/// more that way than to a side, else left or right; forward with none:
/// TnPlayerInput.GetMovementDirection), in place of his run; in the air he flies
/// level (PHYS_Flying), once until he lands. Every class's multiplayer acrobatics
/// (TR_Acrobatics_p's Car-, Plane-, Tank- and TruckMPAcrobatics) have the same,
/// 2000 units a second for 0.95 s: 19 m. Its clip plays over it at its own pace.
const DASH_SPEED: f32 = 20.0;
const DASH_TIME: f32 = 0.95;
const DASH_ANIM_SPEED: f32 = 1.0;
const DASH_ACTION_SECS: f32 = DASH_TIME;
/// Dash/Sprint is one key: held this long, Jazz sprints, and doesn't dash. Let go
/// sooner, it was a tap, and he dashes. (The demo's own timing.)
const SPRINT_HOLD_SECS: f32 = 0.2;
/// Sprinting, Jazz runs forward, or forward and to a side: the movement within 60
/// degrees of straight ahead (this is its share of the input, cos 60). The game has
/// one sprint clip, Nav_Sprint_F (its acrobatics play it for the left and right
/// sprint leans too, TnSprintAnim), and its robots always sprint the way they face,
/// so Jazz turns to face the way he runs (`run_turn`) and plays it, upper body and
/// all. Held the other ways, Dash/Sprint leaves him jogging at his usual speed.
const SPRINT_MIN_FORWARD: f32 = 0.5;

/// Whether Jazz is sprinting with this movement input (see SPRINT_MIN_FORWARD).
fn sprint_run(sprinting: bool, input: Vec2) -> bool {
    sprinting && input.length() > 0.1 && input.normalize().y > SPRINT_MIN_FORWARD
}

/// The way Jazz runs as a turn from where the camera looks (radians about the up
/// axis, positive to the left, as headings turn), for movement input `input`.
fn run_turn(input: Vec2) -> f32 {
    (-input.x).atan2(input.y)
}

/// Melee (middle click): the class's sword swing, as in multiplayer (its melee set's
/// MELEE_WeaponAttack, Melee_Sword_Infiltrator... at its own rate, as long as its
/// clip, Robot::melee_secs; the Ion Blade and the blow's sweep are melee.rs's), or a
/// charging punch when sprinting forward (Jazz's own, quickened).
const MELEE_ANIM_SPEED: f32 = 1.3;
const MELEE_BOOST_SECS: f32 = 1.17 / MELEE_ANIM_SPEED;
const MELEE_LUNGE_SPEED: f32 = 30.0;
const MELEE_LUNGE_TIME: f32 = 0.35;
/// Time from the charging punch to its blow landing.
const MELEE_HIT_DELAY: f32 = 0.35;
/// How far the punch's blow reaches from Jazz's chest (it sends out a shockwave).
const MELEE_RANGE: f32 = 6.5;
/// The sword swing's blow's damage. Escalation's melee set (every Escalation
/// character's MeleeSet: TR_Shared_MS_p.SharedMeleeSet, not multiplayer's
/// MPMeleeSet_<class>, whose sword does 135 with a tenth of the push): its
/// MELEE_WeaponAttack, Melee_EnergonSword_01, Damage 75.
const MELEE_DAMAGE: f32 = 75.0;
/// The charging punch's: the set's sprint attack (the Melee_Boost restricted to
/// AS_Sprinting: TnMeleeManager.UsingSprintMelee), Damage 150.
const MELEE_SPRINT_DAMAGE: f32 = 150.0;

const LAND_ANIM_SPEED: f32 = 1.2;
const LAND_SECS: f32 = 0.53 / LAND_ANIM_SPEED;
/// Hard landings, as Jazz's LandingAnims pick them: after falling 12 m while moving
/// at 12 m/s or more, or after falling 45 m. Lighter falls of 2.5 m or more get the
/// ordinary landing.
const HARD_LAND_FALL: f32 = 12.0;
const HARD_LAND_SPEED: f32 = 12.0;
const HARD_LAND_FALL_ANY: f32 = 45.0;
const LAND_FALL: f32 = 2.5;
const LAND_HARD_SECS: f32 = 1.0 / LAND_ANIM_SPEED;

/// The camera's shake as a robot lands and as he jumps: the TnAnimNotify_CameraShake
/// each landing clip and the take-off clip play on their first frame (all_shared's
/// Shared_ROBO_NAV_ANIM, every class's; their TnLocalPlayerFinderOwner shakes the
/// lander's own view only): Nav_Land's, Nav_Land_02's and Nav_Land_Hard_01's
/// Intensity 0.85 and Roll 0.7, the roll falling off over 0.4, 0.35 and 0.4 s and the
/// shake over 0.35, 0.4 and 0.45 s; Nav_TakeOff_01's a light one, 0.15 and 0.25 over
/// 0.35 and 0.3 s.
const LAND_SHAKE: CameraShake = CameraShake {
    intensity: 0.85,
    roll: 0.7,
    roll_secs: 0.4,
    secs: 0.35,
};
const LAND_02_SHAKE: CameraShake = CameraShake {
    intensity: 0.85,
    roll: 0.7,
    roll_secs: 0.35,
    secs: 0.4,
};
const LAND_HARD_SHAKE: CameraShake = CameraShake {
    intensity: 0.85,
    roll: 0.7,
    roll_secs: 0.4,
    secs: 0.45,
};
const JUMP_SHAKE: CameraShake = CameraShake {
    intensity: 0.15,
    roll: 0.25,
    roll_secs: 0.35,
    secs: 0.3,
};

/// The shake of the landing clip the game picks after a fall of `fall` metres at
/// `speed` (m/s): the first of the multiplayer robots' LandingAnims (CarMPAcrobatics'
/// and every class's alike: TnAcrobaticsManager.FindLandingAnimationParams) that
/// the fall matches. 12 m at 12 m/s or more, or 45 m: Nav_Land_03, which the cooked
/// sets lack, the demo playing the hard landing in its place (and its shake); 10 m
/// at 12 m/s: Nav_Land; 5 m: Nav_Land_02; 2.5 m: Nav_Land; less, no landing clip.
fn landing_shake(fall: f32, speed: f32) -> Option<CameraShake> {
    if (fall >= HARD_LAND_FALL && speed >= HARD_LAND_SPEED) || fall >= HARD_LAND_FALL_ANY {
        Some(LAND_HARD_SHAKE)
    } else if fall >= 10.0 && speed >= HARD_LAND_SPEED {
        Some(LAND_SHAKE)
    } else if fall >= 5.0 {
        Some(LAND_02_SHAKE)
    } else if fall >= LAND_FALL {
        Some(LAND_SHAKE)
    } else {
        None
    }
}
const AIR_DASH_ANIM_SPEED: f32 = 1.0;

/// Each class's health, in character::CLASSES' order, as the multiplayer game gives
/// it (its HealthBlueprint, TR_Health_p.MP_<class>_Health's BaseHealth): the
/// Infiltrator and the Scientist 300, the Destroyer 400, the Titan (MP_Soldier_Health)
/// 500. (Escalation's own, Shared_Health_ES, gives every class 100.)
const CLASS_MAX_HEALTH: [f32; 4] = [300.0, 400.0, 500.0, 300.0];
/// The shield over it, every class's (ShieldHealth, the same in both): it takes
/// damage first (TnShieldHealth).
const PLAYER_MAX_SHIELD: f32 = 100.0;
/// What the HUD's damage indicator measures a blow against (its glow's strength
/// is the blow's share of it): Escalation's health, 100.
const HIT_INDICATOR_HEALTH: f32 = 100.0;

/// A class's full health (CLASS_MAX_HEALTH).
pub(crate) fn max_health(class: usize) -> f32 {
    CLASS_MAX_HEALTH
        .get(class)
        .copied()
        .unwrap_or(CLASS_MAX_HEALTH[0])
}
/// The shield recharges once Jazz has gone this long without damage, at this many
/// points a second (TnHealthRegener with the form's HealthRegenParameters: the
/// robot's, Jazz_ROBODEF's, and the car's, JazzVehicleForm's). Its heal reaches the
/// shield only (TnHealTypeRegen, SH_HealShieldOnly): his own health doesn't come
/// back until he respawns.
const SHIELD_REGEN_DELAY: f32 = 4.0;
const SHIELD_REGEN_RATE: f32 = 50.0;
const CAR_SHIELD_REGEN_DELAY: f32 = 7.0;
const CAR_SHIELD_REGEN_RATE: f32 = 12.0;
const RESPAWN_SECS: f32 = 4.0;
/// What's left of the wait while his body still lies (downed.rs): down, but due.
const DEAD_HOLD: f32 = 1e-3;

/// Another player's copy's wait to come back, run on `dt` between his snapshots:
/// dead, he stays dead till his PC says he is back, his wait stopping at the hold,
/// as his own does while he is out till the round ends or his body still lies.
/// (Run down to nothing, a copy out till the next wave came back on every frame
/// without a snapshot: the marker over where he died flickered on till the wave.)
fn remote_dead_left(left: f32, dt: f32) -> f32 {
    if left > 0.0 {
        (left - dt).max(DEAD_HOLD)
    } else {
        0.0
    }
}
const SPAWN_PROTECTION: f32 = 2.0;
const HIT_SECS: f32 = 0.33;

// ---------------------------------------------------------------------------

/// The IO threads' stacks (see main).
const IO_THREAD_STACK: usize = 32 << 20;

/// crash.txt, by settings.txt: each time the demo stops on an error in its code
/// (a panic, which closes it, and the window it was printed in with it), when (UTC),
/// the thread, where in the code, what went wrong and the stack, for the crash to
/// be looked into. Begun afresh once past CRASH_LOG_MAX bytes.
const CRASH_LOG: &str = "crash.txt";
const CRASH_LOG_MAX: u64 = 512 * 1024;

/// Panics written to CRASH_LOG as well as printed (Bevy's log adds its own).
fn log_panics() {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let what = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        let at = info.location().map(|l| l.to_string()).unwrap_or_default();
        let thread = std::thread::current();
        let stack = std::backtrace::Backtrace::force_capture();
        let text = format!(
            "{} foc_esc {}: thread '{}' panicked at {at}:\n{what}\n{stack}\n",
            net::utc_now(),
            env!("CARGO_PKG_VERSION"),
            thread.name().unwrap_or("?"),
        );
        let path = assets_dir()
            .parent()
            .map(|p| p.join(CRASH_LOG))
            .unwrap_or_else(|| PathBuf::from(CRASH_LOG));
        let fresh = std::fs::metadata(&path).is_ok_and(|m| m.len() > CRASH_LOG_MAX);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(!fresh)
            .truncate(fresh)
            .open(&path)
        {
            use std::io::Write;
            let _ = file.write_all(text.as_bytes());
        }
        before(info);
    }));
}

/// A copy installed by the installer for players (tools/installer) whose game
/// files aren't built yet, or were built with other build tools than the ones it
/// has now (an update that changed them): its builder builds them, and starts the
/// demo when they're done, instead of the demo starting without them. It knows
/// by its build tools' id (tools/installer/pipeline.txt) and the one its game
/// files were built with (game_files.txt, which the builder writes when all of
/// them are built). A copy without the first, such as the project's own folder,
/// starts as always.
fn needs_builder(dir: &std::path::Path) -> bool {
    let Ok(want) =
        std::fs::read_to_string(dir.join("tools").join("installer").join("pipeline.txt"))
    else {
        return false;
    };
    let have = std::fs::read_to_string(dir.join("game_files.txt")).unwrap_or_default();
    have.trim() != want.trim()
}

/// Runs the builder (needs_builder), its window showing (pythonw.exe, the
/// installed copy's own Python): true if it started.
fn start_builder() -> bool {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
    else {
        return false;
    };
    if !needs_builder(&dir) {
        return false;
    }
    let python = dir.join("python").join("pythonw.exe");
    let builder = dir.join("tools").join("installer").join("builder.py");
    if !python.is_file() || !builder.is_file() {
        return false;
    }
    std::process::Command::new(python)
        .args(["-E", "-s"])
        .arg(builder)
        .arg("--play")
        .current_dir(&dir)
        .spawn()
        .is_ok()
}

fn main() {
    if start_builder() {
        return;
    }
    log_panics();
    // The models load on the IO threads, where Bevy's glTF loader waits for a
    // model's textures (a task scope) and meanwhile runs other loads on the same
    // thread: with every class's models loading as the demo starts, those can nest
    // deep enough to overflow a thread's usual 2 MB stack. So the IO pool is built
    // here as Bevy would build it (a quarter of the cores, 1 to 4 threads), with
    // more room (TaskPoolPlugin keeps a pool that is already there).
    bevy::tasks::IoTaskPool::get_or_init(|| {
        let cores = bevy::tasks::available_parallelism();
        let threads = ((cores as f32 * 0.25).round() as usize)
            .min(cores)
            .clamp(1, 4);
        bevy::tasks::TaskPoolBuilder::default()
            .num_threads(threads)
            .thread_name("IO Task Pool".to_string())
            .stack_size(IO_THREAD_STACK)
            .build()
    });
    // The maps are mostly indoors: a roof would put everything in the sun's shadow,
    // so on a map without its own lighting the light is flatter and shadow-free.
    let on_map = !built_maps().is_empty();
    App::new()
        // A command or system that fails (a change to an entity something else
        // has just despawned, say) is logged as a warning instead of stopping
        // the game, as Bevy does by default.
        .set_error_handler(bevy::ecs::error::warn)
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "foc_esc".into(),
                        // The settings' Framerate Cap sets the pace (menu.rs), not
                        // the display: frames show as soon as they are done, without
                        // tearing where the driver can (Mailbox; otherwise
                        // Immediate, or vsync as a last resort).
                        present_mode: bevy::window::PresentMode::Mailbox,
                        ..default()
                    }),
                    ..default()
                })
                // Bevy loads from the same assets folder the game reads itself
                // (assets_dir: the project's, however the game was started).
                .set(AssetPlugin {
                    file_path: assets_dir().to_string_lossy().into_owned(),
                    ..default()
                })
                // (The standard materials not bindless: graphics.rs's wgpu_settings.)
                .set(bevy::render::RenderPlugin {
                    render_creation: bevy::render::settings::RenderCreation::Automatic(
                        graphics::wgpu_settings(),
                    ),
                    ..default()
                }),
        )
        // The program's icon on its window and the taskbar (Windows).
        .add_systems(Update, appicon::set_window_icon)
        .insert_resource(ClearColor(Color::srgb(0.20, 0.16, 0.15)))
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(1.0, 0.85, 0.75),
            brightness: if on_map { 900.0 } else { 350.0 },
            ..default()
        })
        .init_resource::<Score>()
        .init_resource::<ScreenEffects>()
        .init_resource::<Colliders>()
        .init_resource::<Sfx>()
        // (The game's mixer presets on: mixer.rs.)
        .init_resource::<mixer::Mix>()
        // (Recordings decoded into memory the first time they play: sound.rs's Pcm.)
        .add_audio_source::<sound::Pcm>()
        .init_resource::<sound::Decoded>()
        .add_systems(Update, sound::finish_decoding)
        .init_resource::<HeldInput>()
        .init_resource::<MatchClock>()
        .init_resource::<CurrentMap>()
        // Animation clips put in their graphs as their files come in.
        .init_resource::<GltfClips>()
        .add_systems(PreUpdate, fill_gltf_clips)
        // (Models' scenes, meshes and pictures from their files, each file read once.)
        .add_plugins(gltffiles::GltfFilesPlugin)
        // Online play (net.rs): packets in before the game's systems, out after.
        .init_resource::<net::Net>()
        .add_systems(PreUpdate, net::net_receive)
        .add_systems(PostUpdate, net::net_send)
        // The camera fades the player's own Jazz when it comes close (after the
        // transforms are in place: the fade is measured from them; before Bevy
        // checks the meshes' visibility ranges, which the fade sets).
        // A tank's cannon follows the view, once its clips have posed it.
        .add_systems(
            PostUpdate,
            car::aim_cannons
                .after(bevy::app::AnimationSystems)
                .before(bevy::transform::TransformSystems::Propagate),
        )
        // A Jazz's upper body and gun arm turned to where he aims (robot_aim.rs),
        // once his clips have posed them, before his ragdoll takes them.
        .add_systems(
            PostUpdate,
            robot_aim::apply_robot_aim
                .after(bevy::app::AnimationSystems)
                .before(ragdoll::drive_rigs)
                .before(bevy::transform::TransformSystems::Propagate),
        )
        // A dead Jazz's bones his death scales (downed.rs), once his clips have
        // posed them, before his ragdoll poses its own.
        .add_systems(
            PostUpdate,
            downed::scale_bones
                .after(bevy::app::AnimationSystems)
                .before(ragdoll::drive_rigs),
        )
        .add_systems(
            PostUpdate,
            fade_near_camera
                .after(bevy::transform::TransformSystems::Propagate)
                .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
        )
        // The lobby's map comes into play as the countdown to the match starts (it
        // loads meanwhile), or else as the match does, before it is set up.
        .add_systems(Update, follow_map_choice)
        .add_systems(
            OnEnter(menu::AppState::Game),
            (follow_map_choice, pregame::enter, start_match).chain(),
        )
        // Leaving the match: its sounds stop with it (sound.rs's stop_match_sounds).
        .add_systems(
            OnExit(menu::AppState::Game),
            (quiet_engine, sound::stop_match_sounds),
        )
        .init_resource::<FxQueue>()
        .init_resource::<fx::SpriteLooks>()
        .insert_resource(fx::FxDir::new(&assets_dir()))
        // The game's HUD movie, played from assets/hud.
        .add_plugins(hud::HudPlugin {
            assets: assets_dir(),
        })
        // The main menu, shown first; the game runs once it is left.
        .add_plugins(menu::MenuPlugin)
        // Its Customize screen: the garage and its Scaleform-style 3D menus.
        .add_plugins(flash::FlashPlugin)
        .add_plugins(customize::CustomizePlugin)
        // A match's Choose Character screen (after Customize: it uses its data).
        .add_plugins(choose::ChoosePlugin)
        // The start of a match: the map's view and the countdown to it.
        .add_plugins(pregame::PreGamePlugin)
        // The scoreboard in a match (Tab), and the numbers it shows.
        .add_plugins(scoreboard::ScoreboardPlugin)
        // A game controller playing Jazz (Active Game Controls: Controller).
        .add_plugins(pad::PadPlugin)
        // A second player on this PC, the window shared (split screen), and his
        // Jazz.
        .add_plugins(split::SplitPlugin)
        .add_systems(Update, guest_jazz)
        // The pause menu's hologram view of the world.
        .add_plugins(hologram::HologramPlugin)
        // The map's depth of field (its plugin after the hologram's: its pass runs
        // just before the hologram's).
        .add_plugins(dof::GameDofPlugin)
        // The sun's light shafts (their plugin after the depth of field's: their
        // pass runs just before it).
        .add_plugins(lightshafts::LightShaftsPlugin)
        // The purchase points' bases in the game's own unlit shading.
        .add_plugins(pickupbase::PickupBasePlugin)
        // The pickups' outlines round their silhouettes only, as the game's.
        .add_plugins(outline::OutlinePlugin)
        // The Graphics settings' level (graphics.rs).
        .add_plugins(graphics::GraphicsPlugin)
        // The characters' shadows from the lights other than the sun (shadows.rs).
        .add_plugins(shadows::ShadowsPlugin)
        // The map's baked lighting, lights, fog and colour grade (from the game).
        .add_plugins(lighting::MapLightingPlugin)
        // The energon cubes' material.
        .add_plugins(energon::EnergonPlugin)
        // The multiplayer melee's Ion Blade and sword swings.
        .add_plugins(melee::MeleePlugin)
        // The boosters and dust under the robots' feet as they dash, jump and land.
        .add_plugins(movefx::MoveFxPlugin)
        // The maps' effects: light shafts, glow spheres, particles and lens flares.
        .add_plugins(levelfx::LevelFxPlugin)
        // The map's parts joined into fewer meshes once it is in (to draw quickly).
        .add_plugins(merge::MergePlugin)
        // The Infiltrator's EMP grenade (his build's ability).
        .add_plugins(emp::EmpPlugin)
        // The other class abilities a build can take.
        .add_plugins(abilities::AbilitiesPlugin)
        .add_plugins(hitreact::HitReactPlugin)
        .add_plugins(mapmotion::MapMotionPlugin)
        // The materials whose glow or colour changes in their shader: the map's
        // pulsing glows, the consoles' and pods' outlines, the gun effects' sprites.
        .add_plugins(live::LivePlugin)
        // Escalation's support items in use (bought at the equipment stations).
        .add_plugins(equipment::EquipmentPlugin)
        // Escalation's downed players, revives, spectating and the team's lives.
        .add_plugins(revive::RevivePlugin)
        // Escalation's Armory Recreators: pods that make a heavy weapon.
        .add_plugins(recreator::RecreatorPlugin)
        // Escalation's locked doors and the consoles that unlock them.
        .add_plugins(doors::DoorsPlugin)
        .add_plugins(barrels::BarrelsPlugin)
        // The maps' traps their consoles set off: the flame jets, the combustion
        // chamber, the sentries and the gunship's guns.
        .add_plugins(traps::TrapsPlugin)
        .add_plugins(nav::NavPlugin)
        // The Riot Cannon's rounds.
        .add_plugins(riot::RiotPlugin)
        .add_plugins(orbital::OrbitalPlugin)
        .add_plugins(specials::SpecialsPlugin)
        .add_plugins(xp::XpPlugin)
        .add_plugins(killfeed::KillFeedPlugin)
        .add_plugins(voice::VoicePlugin)
        // Escalation's enemies (without their AI), and the ragdolls the dead fall
        // as (theirs and the players').
        .add_plugins(enemies::EnemiesPlugin)
        // Escalation's waves: the countdowns, the enemies they bring in, their ends.
        .add_plugins(waves::WavesPlugin)
        // The enemies' AI (once they have come in).
        .add_plugins(ai::AiPlugin)
        .add_plugins(ragdoll::RagdollPlugin)
        .add_plugins(gibs::GibsPlugin)
        // The Titan's, Scientist's and Destroyer's weapons' own workings.
        .add_plugins(arsenal::ArsenalPlugin)
        // The Infiltrator's characters: the parts a build wears, painted, and its
        // vehicle.
        .add_plugins(character::CharacterPlugin)
        .add_plugins(decals::DecalsPlugin)
        // The robots' bones posed through small stand-in graphs of the clips that
        // count (their big graphs walked bone by bone cost most of a frame).
        .add_plugins(standin::StandInPlugin)
        .configure_sets(
            Update,
            hud::HudSystems.run_if(in_state(menu::AppState::Game)),
        )
        .add_systems(
            Startup,
            // Chained so each step can use what the previous one created.
            (
                setup_world,
                setup_level,
                setup_player,
                setup_camera,
                setup_sounds,
                fx::setup_fx,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                // The pause menu first (the scoreboard's Esc before it), then the
                // Choose Character screen: while either is up Jazz takes no orders,
                // nor before the match has begun for him (the countdown,
                // pregame.rs). As it begins, the match is set up and he takes up
                // the character chosen as it started.
                (
                    scoreboard::scoreboard_input,
                    menu::pause_input,
                    choose::follow_classes,
                    choose::choose_input,
                    pregame::countdown,
                    pause_game_input,
                    pad::read_pad,
                    start_match.run_if(pregame::match_begins),
                    arm_chosen,
                )
                    .chain(),
                (
                    // Who plays which Jazz, and what each asks for this frame.
                    (
                        cursor_grab,
                        attach_robot_animator,
                        car::attach_car_animator,
                        tag_jazz_parts,
                        tag_jazz_joints,
                        downed::rig_robots,
                        follow_looks,
                        follow_robot_class,
                        net_puppets,
                        read_local_input,
                        net_apply,
                        hide_waiting_jazz,
                    )
                        .chain(),
                    // Every Jazz moves, animates and fights from his input.
                    (
                        // (His death plays out as his health says: downed.rs.)
                        (
                            player_health,
                            downed::start_deaths,
                            downed::play_deaths,
                            downed::end_deaths,
                        )
                            .chain(),
                        toggle_form,
                        weapon_pickups,
                        switch_weapon,
                        switch_hands,
                        animate_transform_pop,
                        player_movement,
                        player_sounds,
                        animate_robot,
                        (show_weapon_meshes, show_upgrade_parts, melee::show_blades),
                        fold_forearms,
                        spin_wheels,
                        camera_follow,
                        level_camera_view,
                        reload_and_spread,
                        fire,
                        car::car_state,
                        car::animate_car,
                        car::vehicle_effects,
                    )
                        .chain(),
                )
                    .chain(),
                (
                    weapon_loops,
                    melee_hits,
                    melee_sweeps,
                    screen_effects,
                    health_stations,
                    shieldfx::shield_effects,
                    ammo_stations,
                    shop::shop_stations,
                    feed_hud,
                    feed_hud_allies,
                    // (What befell this PC's players, to their HUDs.)
                    deliver_hud_events,
                    engine_sound,
                    cut_hover_engines,
                    voice::speak,
                    net_capture,
                    sound::play_sounds.in_set(sound::PlaySounds),
                    fx::spawn_fx,
                    fx::update_fx,
                    restore_game_input,
                )
                    .chain(),
            )
                .chain()
                .run_if(in_state(menu::AppState::Game))
                // The HUD reads what these systems leave in HudState.
                .before(hud::HudSystems),
        )
        .run();
}

// ---------------------------------------------------------------------------
// Components and resources
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Robot,
    Vehicle,
}

#[derive(Component)]
struct Player {
    form: Form,
    vertical_velocity: f32,
    grounded: bool,
    /// Facing angle around the vertical axis (radians).
    heading: f32,
    /// In robot form: the camera's angle from that facing (positive to the left),
    /// which his upper body turns to follow, and a turn on the spot playing.
    twist: f32,
    /// The view's pitch (radians, up positive): a tank's cannon follows it
    /// (car::aim_cannons).
    aim_pitch: f32,
    pivot: Option<Pivot>,
    /// Forward speed, used while in vehicle form.
    drive_speed: f32,
    /// Sideways speed (positive to the right) and turning speed (radians a second,
    /// positive to the left), while in vehicle form.
    strafe_speed: f32,
    yaw_rate: f32,
    /// The car's or truck's wheels' steering (-1 to 1, right: drive.rs's
    /// wheel_steering), which they turn and the body leans by (car.rs).
    steer: f32,
    fire_cooldown: f32,
    next_vehicle_gun: usize,
    /// The two weapons carried (indices into WEAPONS), the one in hand (0 or 1),
    /// and that weapon (`loadout[hand]`).
    loadout: [usize; 2],
    /// The equipment bought (shop.rs), offensive and defensive, and what it has going
    /// (equipment.rs: its uses, his barrier and mines, the buffs on him).
    equipment: [Option<shop::Equipment>; 2],
    support: equipment::Support,
    /// Down, getting up, reviving another, dead till the round ends (revive.rs).
    revive: revive::Revive,
    /// His character's class ability (abilities.rs; the EMP grenade is emp.rs's).
    ability: abilities::ClassAbility,
    hand: usize,
    weapon: usize,
    /// Rounds left in the burst being fired, and a burst asked for (the trigger
    /// pulled) that starts once the gun is ready.
    burst_left: u32,
    burst_queued: bool,
    wheel_angle: f32,
    /// Movement input relative to where the robot faces (x = right, y = forward).
    move_input: Vec2,
    sprinting: bool,
    /// Seconds left of a transformation animation; the player can't move meanwhile.
    transform_left: f32,
    /// Form to switch to when the robot-to-vehicle animation finishes.
    pending_form: Option<Form>,
    /// Animation to (re)start this frame, with its playback speed.
    start_clip: Option<(usize, f32)>,
    /// The robot's clip last started (animate_jazz): it, how far into it it is
    /// (seconds of the clip) and its speed, for the effects its notifies play
    /// (movefx.rs).
    move_clip: Option<(usize, f32, f32)>,
    /// A one-off move being played (dash, melee, landing) and the seconds left.
    action: Option<(usize, f32)>,
    /// His reaction to an enemy's melee blow (hitreact.rs): his upper body thrown,
    /// and no shooting, reloading or ability till it completes.
    melee_react: hitreact::MeleeReact,
    /// Velocity from a dodge (DASH_SPEED the way it goes, in place of his run, for
    /// as long as it lasts: `dodge`) or a melee lunge (extra, fading out over
    /// `dash_total`).
    dash_vel: Vec3,
    dash_left: f32,
    dash_total: f32,
    dodge: bool,
    /// The dodge began in the air (the game's Dodging state, PHYS_Flying: level, no
    /// gravity while it lasts). One begun on the ground (DodgingOnGround, walking)
    /// falls if it runs off a ledge, and a jump ends it.
    air_dodge: bool,
    /// Counts down to the moment a charging punch's blow lands (Melee while
    /// sprinting).
    melee_hit_in: Option<f32>,
    /// His sword swing's blow (melee.rs; melee_sweeps).
    melee_sweep: melee::Sweep,
    /// His sword swing's lunge at the enemy under his crosshair (melee.rs's Lunge),
    /// and whether the swing has met an enemy (its lunge reached it, or its blow
    /// landed): then he stands till the swing is over (TnMeleeManager.Attacking.
    /// DisableMotionOnContact: its speed multiplier 0).
    lunge: Option<melee::Lunge>,
    melee_contact: bool,
    /// Seconds spent off the ground in the current jump.
    air_time: f32,
    /// Seconds since Jazz jumped, while he is in the air from a jump (not a fall).
    jump_time: Option<f32>,
    /// The highest point of the current time in the air (for the landing).
    air_peak: f32,
    /// How long he has been on the ground since where he stands was last noted
    /// (seconds), and the places noted (every SAFE_GROUND_SECS on the ground): the
    /// one before last, where he is put back if he falls below the map's KillZ
    /// (`Level::kill_y`), and the last.
    ground_secs: f32,
    safe_spot: Option<Vec3>,
    safe_next: Option<Vec3>,
    /// Dodged since he was last on the ground with no dodge going (the game's
    /// _LandedSinceLastDodge, false: TnAcrobaticsManager.CanDodge): no other dodge
    /// till then.
    dodged: bool,
    health: f32,
    /// The shield over his health (PLAYER_MAX_SHIELD when full).
    shield: f32,
    /// Seconds since he last took damage (the shield's recharge waits on it).
    since_hit: f32,
    /// Set when health runs out; the death sequence starts next frame.
    dying: bool,
    /// Seconds until respawn while dead (0 = alive; held above 0 until his body has
    /// dissolved: downed.rs).
    dead_left: f32,
    /// How he was last struck (what his death goes by: downed.rs).
    last_hit: enemies::Hit,
    /// His shield's effects: its return, its breaking and its ripples (shieldfx.rs).
    shield_fx: shieldfx::ShieldFx,
    /// His death (downed.rs): its clip to start this frame (robot_deaths.glb's
    /// number, and its speed) and the one playing; his robot fallen limp (a
    /// ragdoll: its clips' weights left at nothing); where the camera looks while
    /// he is down (as if he stood there); and, for another player's copy, word of
    /// how he died from his PC.
    start_death_clip: Option<(usize, f32)>,
    death_clip: Option<usize>,
    limp: bool,
    death_view: Option<Vec3>,
    death_news: Option<downed::DeathNews>,
    /// Who last hurt him and by what damage type (the kill feed's killer should it
    /// kill him: killfeed.rs). Nobody for a blow of no one's (a trap's, his own).
    last_attack: (killfeed::Who, &'static str),
    /// Not in the match yet (pregame.rs: his player is choosing his character, or
    /// counting down to the match): his Jazz isn't seen, and the sentries leave him
    /// be.
    waiting: bool,
    protection: f32,
    /// Horizontal velocity last frame.
    velocity: Vec3,
    /// Speed kept through a transformation: Jazz keeps rolling while he changes.
    carry_vel: Vec3,
    /// Which clip the current transformation is playing.
    transform_clip: usize,
    /// Fine aim held (right mouse button).
    aiming: bool,
    /// Trigger held in robot form.
    firing: bool,
    /// A one-off gun animation to start this frame (the rifle's cool-down, a
    /// reload: clip, seconds, speed), and the one playing with its seconds left.
    start_weapon_clip: Option<(usize, f32, f32)>,
    weapon_clip: Option<(usize, f32)>,
    /// The camera's kick from shots (WeaponStats::recoil): pitch still to add to the
    /// view (radians), and the last shot's recoil with the seconds since it.
    camera_kick: f32,
    recoil: Option<(CameraRecoil, f32)>,
    /// A weapon switch to start this frame (the weapon taken out, and the one put
    /// away, if any: none when a picked-up weapon replaces the one in hand), and
    /// the weapons unfolding and folding away with their seconds left.
    start_equip: Option<(usize, Option<usize>)>,
    equipping: Option<(usize, f32)>,
    unequipping: Option<(usize, f32)>,
    /// The gun put away while he sprints or revives (holsters), and the change to
    /// that under way: seconds into the gun folding away and the forearm coming out
    /// on its arm (to holstered), or the gun unfolding and the forearm folding away
    /// (back out).
    holstered: bool,
    holster_change: Option<f32>,
    /// The gun in his left hand (C switches), how far his animation is mirrored to
    /// match (0 to 1, MIRROR_BLEND_SECS), and a hand switch: one to start this frame,
    /// and the gun moving over with its seconds left.
    left_handed: bool,
    mirror: f32,
    start_hand_switch: bool,
    hand_switch: Option<(usize, f32)>,
    /// Rounds in the clip and in reserve, per gun slot (WEAPONS, then the car).
    ammo: [(u32, u32); GUN_SLOTS],
    /// The upgrades on the weapons he carries (upgrades.rs: the game's
    /// WeaponModifications, each weapon's own), and those his character puts on a
    /// weapon given to him (his build's: its WeaponUpgradePairs).
    upgrades: upgrades::Installed,
    upgrade_pairs: upgrades::Installed,
    /// Seconds left of a reload (0 = not reloading), and the slot reloading.
    reload_left: f32,
    reload_slot: usize,
    /// A transformation's reload still to come: seconds left until every weapon
    /// he carries is reloaded (FORM_WEAPON_SECS, reload_all_weapons).
    form_weapon_left: Option<f32>,
    /// The current gun's spread, and the slot it belongs to.
    spread: f32,
    spread_slot: usize,
    /// The spread's multiplier while he is in the air (TnWeaponSpreadModifier):
    /// up to AIRBORNE_SPREAD.
    airborne_spread: f32,
    /// Shots fired from a gun with several barrels: the next leaves from barrel
    /// `muzzle` (modulo their number).
    muzzle: u32,
    /// A shot's camera shake for the camera to play (the player at this PC).
    shake: Option<CameraShake>,
    /// His blow landed this frame: energon splashes across his screen
    /// (screen_effects).
    screen_struck: bool,
    /// A reload of a gun that steams (WeaponStats::reload_steam): seconds since it
    /// began.
    steam: Option<f32>,
    /// The steam rising from the shell port after shots whose shells steam
    /// (GunShell::steam): seconds since each shot, and the weapon (WEAPONS) fired.
    shell_steam: Vec<(f32, usize)>,
    /// The end-of-burst sound of the gun being fired, played when the trigger is
    /// let go.
    burst_end: Option<&'static str>,
    /// The Nucleon Charge Rifle's charge (WeaponStats::charge, update_charge):
    /// seconds charging (its _ChargeTimer) and the level reached (its fire mode: the
    /// next shot's damage); whether the rifle was ready and Jazz fine aiming last
    /// frame (it starts over when either changes: ResetFireMode); its two states as
    /// its weapon events have them (StartCharged to StopCharged, charging, and
    /// StartFullyCharged to StopFullyCharged, fully charged); and seconds to the
    /// next ring of the glow round its barrel while fully charged.
    charge_timer: f32,
    charge_level: usize,
    charge_ready: bool,
    charge_aiming: bool,
    charging: bool,
    charged: bool,
    charge_ring: f32,
    /// Seconds left of Deadeye's faster charge (TnBuffSniperRifleSpeedUpCharge on
    /// the Nucleon Charge Rifle, put on afresh as a round of it strikes an enemy).
    deadeye_left: f32,
    /// The fire his Fireblast pellets have done this life (TnPlayerPawn's
    /// AccumulateFireShotgunDamageForXpEvent) and whether its award is given (xp.rs).
    fire_damage: f32,
    fire_awarded: bool,
    /// Seconds E has been held at a weapon on the ground, and whether E is still
    /// down from a swap (so holding it doesn't swap back once the dropped weapon
    /// can be taken).
    pickup_held: f32,
    pickup_swapped: bool,
    /// Seconds left of the EMP grenade's Speed Redirector (emp.rs): he runs faster.
    speed_boost: f32,
    /// Seconds left of TnBuffAbilityJammed on him (a Guard's scanner or its blast:
    /// abilities.rs's jam): his ability can't be used and doesn't refill meanwhile,
    /// and the HUD shows it jammed.
    jammed_left: f32,
    /// A blow's push across the ground (its momentum over his mass, for a damage
    /// type that asks for it: the Guard's blast), the demo's m/s: he flies with it
    /// till he lands (Pawn.AddVelocity: falling).
    knock_vel: Vec3,
    /// He has just taken up his chosen character's loadout (`arm`): emp.rs gives
    /// him its ability too.
    rearmed: bool,
    /// What he did this frame that his copies on the other players' PCs must do
    /// too (moves and gun animations started, shots, grenades, blows): sent by
    /// net_capture for this PC's Jazz, cleared for every Jazz.
    played: Vec<net::JazzEvent>,
    /// What his HUD is to show of what befell him this frame (his damage, his
    /// pickups...): this PC's player's, handed to his HUD (deliver_hud_events: split
    /// screen's guest's to his own); another player's, dropped.
    hud_events: Vec<HudEvent>,
    /// The character he plays as (character.rs): the parts his robot wears, their
    /// paint and the vehicle his chest makes, and what that brings (its clips
    /// and transformation timing). Taken up with his chosen character (`arm`).
    look: character::Look,
    kit: character::Kit,
    /// A homing weapon's lock on a sentry (jet.rs's Lock: a jet's rockets', the
    /// Thermo Rocket Cannon's).
    lock: jet::Lock,
    /// The new guns' state (arsenal.rs): the X18 Scrapmaker's spin, a firing's first
    /// round, a beam's end, the fine-aim button held for a gun that doesn't fine
    /// aim.
    gear: arsenal::GearState,
    /// His rounds of this kind to go off now, wherever they are (riot.rs's
    /// fly_rounds): the A-4 Pulsar Cannon's as its alt fire sets them off, the
    /// Chaos-Rift Combustor's bomb as its trigger bursts it.
    set_off: Option<&'static riot::RoundKind>,
    /// The camera shakes rounds' blasts have given the player at this PC this frame
    /// (riot.rs's ScreenShake), for the camera to play.
    screen_shakes: Vec<riot::ScreenShake>,
}

impl Player {
    /// His full health: his class's (CLASS_MAX_HEALTH).
    fn max_health(&self) -> f32 {
        max_health(self.kit.class)
    }

    /// His collision cylinder in the form he is in: radius and height (his class's
    /// robot's, or his vehicle's: as wide as its body and as tall as it is, the
    /// smallest the car's).
    fn size(&self) -> (f32, f32) {
        match self.form {
            Form::Robot => (self.kit.robot.radius, self.kit.robot.height),
            Form::Vehicle => (
                VEHICLE_RADIUS.max(self.kit.body.half_width + VEHICLE_RADIUS - VEHICLE_HALF_WIDTH),
                VEHICLE_HEIGHT.max(self.kit.vehicle_height),
            ),
        }
    }

    /// The middle of his collision cylinder, standing at `feet` (the place the game
    /// measures from, TnWeapon.ModifyDamage's Instigator.Location).
    fn centre(&self, feet: Vec3) -> Vec3 {
        feet + Vec3::Y * (self.size().1 * 0.5)
    }

    /// The middle of his robot standing at `feet`.
    fn robot_centre(&self, feet: Vec3) -> Vec3 {
        feet + Vec3::Y * (self.kit.robot.height * 0.5)
    }

    /// Swinging his sword or punching (Melee), until its blow comes.
    fn melee_winding_up(&self) -> bool {
        self.melee_hit_in.is_some()
            || (matches!(self.action, Some((ANIM_MELEE, _))) && !self.melee_sweep.begun)
    }

    /// In the middle of a sword swing or a charging punch (his gun doesn't fire).
    fn meleeing(&self) -> bool {
        matches!(self.action, Some((ANIM_MELEE | ANIM_MELEE_BOOST, _)))
    }

    /// What the upgrades on his weapon in `slot` do to it (upgrades.rs; the
    /// vehicles' weapons have none).
    fn mods(&self, slot: usize) -> upgrades::Mods {
        if slot < WEAPONS.len() {
            upgrades::Mods::of(self.upgrades, slot)
        } else {
            upgrades::Mods::default()
        }
    }

    /// The gun in `slot` as its upgrades make it: its clip and reserve
    /// (GetMaxAmmoClipCount, GetMaxReserveAmmoCount), its reload's time
    /// (ModifyReloadTime: divided by GetReloadSpeed) and its spread (the spread a
    /// shot leaves with, GetCurrentSpreadModifier, times the upgrade's, which the
    /// spread's every step scaled by as much gives; its settling time,
    /// CooldownSpread's, times the other's).
    fn gun(&self, slot: usize) -> Gun {
        let mut g = *gun(slot);
        let m = self.mods(slot);
        (g.clip_size, g.reserve_max) = m.ammo(g.clip_size, g.reserve_max);
        g.reload_secs /= m.reload_speed;
        g.spread.min *= m.spread;
        g.spread.max *= m.spread;
        g.spread.per_shot *= m.spread;
        g.spread.cooldown *= m.spread_cooldown;
        g
    }

    /// The ammo a weapon in `slot` comes with as it is given to him, upgraded
    /// (HmWeapon.SetupInitialAmmo, which TnBuffLargerClip and TnBuffReserveAmmo
    /// call as they go on): its clip full, its reserve its data's
    /// InitialReserveAmmoCount (the reserve's most before any upgrade).
    fn initial_ammo(&self, slot: usize) -> (u32, u32) {
        (self.gun(slot).clip_size, gun(slot).reserve_max)
    }
}

/// Up to four players fight together, as in the game's Escalation.
const MAX_PLAYERS: usize = 4;

/// Which of the four players a Jazz is (0-3): it picks his spawn point.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct PlayerSlot(usize);

/// The Jazz this machine plays: the camera follows him, the HUD shows him, and
/// his sounds play as the camera hears them.
#[derive(Component)]
struct LocalPlayer;

/// The Jazz of a player on another PC (online, net.rs): here he only does what his
/// PC says (net_apply), deciding nothing himself; his sounds come from there too.
#[derive(Component)]
struct RemotePlayer;

/// When the match started (game time): the sentries and energon cubes bob from it,
/// so they bob together on every PC.
#[derive(Resource, Default)]
struct MatchClock(f32);

/// What a player asks for this frame: the keys, buttons and view of whoever plays
/// this Jazz. The local player's are read from this machine's keyboard and mouse
/// (`read_local_input`); every system that moves or fights reads only these, so
/// any Jazz can be played from anywhere.
#[derive(Component, Clone, Default)]
struct PlayerInput {
    /// The movement keys (x to the right, y forward), each -1, 0 or 1.
    movement: Vec2,
    /// The game has the player's mouse (not while it is free for a menu): his guns
    /// fire only then.
    control: bool,
    /// Held: sprint (the robot: Dash/Sprint held SPRINT_HOLD_SECS) or boost (the
    /// car), the trigger (only while the game has the mouse), fine aim and pick
    /// up. (The keys are the layout's: see read_local_input.)
    boost: bool,
    fire: bool,
    aim: bool,
    pick_up: bool,
    /// This frame: jump, dash (a tap of Dash/Sprint, as it's let go), melee,
    /// transform, reload, switch hands, the trigger, and swap to the other
    /// weapon.
    jump: bool,
    dash: bool,
    melee: bool,
    transform: bool,
    reload: bool,
    switch_hands: bool,
    fire_pressed: bool,
    swap_weapon: bool,
    /// Held: the Ability button (the EMP grenade's: emp.rs).
    ability: bool,
    /// Held: the equipment keys, Assault T.E.C.H. and Utility T.E.C.H. (the
    /// offensive and defensive slots' items: equipment.rs), robot form's only.
    tech: [bool; 2],
    /// Held: Jump/Ascend and Descend (a jet climbs and dives on them: jet.rs).
    ascend: bool,
    descend: bool,
    /// How fast the mouse turns the view (degrees a second: yaw to the right, pitch
    /// up), which a jet's flight reads as its yaw and pitch input (jet.rs).
    steer: Vec2,
    /// The controller's right stick (x to the right, y up, the Jet Camera's way
    /// round), while the controller plays: a jet's flight takes it as its yaw and
    /// pitch input as it is instead (SetLocalInputs: GetNormalizedTurn,
    /// GetNormalizedLookUp).
    stick: Option<Vec2>,
    /// The mouse's steering of a car or truck on its wheels (-1 to 1, right:
    /// vehicle_cam.rs's Rig::steer).
    wheel: f32,
    /// Where the player looks: the camera's heading (radians), how fast it is
    /// swinging round (radians a second), and the camera's view (where it is and
    /// the way it looks, unit length): shots and grenades go where it points.
    yaw: f32,
    swing: f32,
    view_origin: Vec3,
    view_dir: Vec3,
}

/// Which Jazz a part of his model belongs to (his robot and car models, their
/// animation players, his hands, guns, forearms and tyres), so each player's
/// systems find their own. Parts of no player's model (a weapon lying on the
/// ground) get `Entity::PLACEHOLDER`.
#[derive(Component, Clone, Copy)]
struct JazzOf(Entity);

/// A Jazz's animation blending (animate_robot): each clip's weight, the air clip
/// playing (descent or fall), and whether his hips are turned back (strafing; see
/// HIP_FLIP_BACK); his death's clip playing and its weight, and how many frames
/// his robot has been limp (downed.rs); the downed clips' weights and times
/// (revive.rs: blend_downed); and his aim offset node (robot_aim.rs puts it on his
/// bones).
#[derive(Component, Default)]
struct RobotAnimState {
    weights: Vec<f32>,
    /// His folding up's clip and the seconds into its blend-in from the pose he was
    /// in (TRANSFORM_BLEND_IN), while it blends in.
    transform_blend: Option<(usize, f32)>,
    air_clip: Option<usize>,
    hips_back: bool,
    death: Option<(AnimationNodeIndex, f32)>,
    limp_frames: u8,
    downed: [(f32, f32); DOWNED_CLIPS],
    ability: AbilityAnim,
    aim: robot_aim::AimNode,
}

/// A Jazz's class ability's clips as they blend (ability_clips): the stance of his
/// raised Flak Shield and his hover's pose (how far each has taken over from his
/// own clips), and the clip over his upper body or arm (its role and weight).
#[derive(Default)]
struct AbilityAnim {
    shield: f32,
    hover: f32,
    overlay: Option<(&'static str, f32)>,
    shield_clips: Vec<(&'static str, f32)>,
}

/// The downed robot's clips (robot_extras.glb's, revive.rs: the game's RobotDowned
/// tree, Shared_ROBO_NAV_Downed_ANIM's), in blend_downed's order: going down
/// (Revive_Fall), the nine aim poses lying (Aim_Downed_<F/L/R>_<C/D/U>, row by
/// row: ahead, left, right; level, down, up), the turns over lying
/// (Nav_Downed_IdlePivot90L/R), getting up (Revive_Getup) and reviving another
/// (Revive_Heal): each one's node and length.
const DOWNED_ROLES: [&str; DOWNED_CLIPS] = [
    "downed_fall",
    "downed_aim_f_c",
    "downed_aim_f_d",
    "downed_aim_f_u",
    "downed_aim_l_c",
    "downed_aim_l_d",
    "downed_aim_l_u",
    "downed_aim_r_c",
    "downed_aim_r_d",
    "downed_aim_r_u",
    "downed_pivot_l",
    "downed_pivot_r",
    "downed_getup",
    "revive_heal",
];
const DOWNED_CLIPS: usize = 14;
const DOWNED_FALL: usize = 0;
const DOWNED_AIM: usize = 1;
const DOWNED_PIVOT_L: usize = 10;
const DOWNED_PIVOT_R: usize = 11;
const DOWNED_GETUP: usize = 12;
const DOWNED_HEAL: usize = 13;
/// The aim poses' reach (the game's AnimNodeAimOffset takes the aim a quarter turn
/// to a pose: its full left, right, up and down).
const DOWNED_AIM_REACH: f32 = FRAC_PI_2;

/// Where a player's sounds come from: the local player's play as the camera hears
/// them (None), the others' where they are.
fn voice_at(local: bool, at: Vec3) -> Option<Vec3> {
    (!local).then_some(at + Vec3::Y)
}

/// The robot's animation graphs, one per class (character::CLASSES): one node per
/// clip, indexed by the ANIM_* constants, in the same places in every class's (so a
/// Jazz who changes class keeps playing as he was), each class's own clips in
/// robot.glb's places (robot_graph).
#[derive(Resource)]
struct RobotAnimations {
    graphs: Vec<Handle<AnimationGraph>>,
    nodes: Vec<AnimationNodeIndex>,
    /// The mirrored clips (robot_mirror.glb), when the file is there.
    mirrored: Option<Vec<AnimationNodeIndex>>,
    /// The players' death clips (robot_deaths.glb's, in its order: downed.rs).
    deaths: Vec<AnimationNodeIndex>,
    /// The robot's extra clips (robot_extras.glb's: tools/build_robot_extras.py),
    /// by their role: the support items' use's (equipment.rs), its node and length;
    /// the downed ones (DOWNED_ROLES' order), when they are all there.
    trigger: Option<(AnimationNodeIndex, f32)>,
    downed: Option<[(AnimationNodeIndex, f32); DOWNED_CLIPS]>,
    /// His reactions to melee blows (hitreact.rs's ROLES, additive), when they are
    /// all there.
    melee_react: Option<[(AnimationNodeIndex, f32); 3]>,
    /// The class abilities' clips (ABILITY_ROLES), by role: node and length.
    ability: Vec<(&'static str, AnimationNodeIndex, f32)>,
    /// The robots' anim tree's aim offset node (robot_extras.txt's: robot_aim.rs),
    /// when it is built.
    aim: Option<ai::clips::AimOffsetData>,
}

/// The class abilities' clips in robot_extras.glb (tools/build_robot_extras.py), by
/// role: the Whirlwind's spin and the Heal Beam's arm, the Flak Shield's stance (its
/// idle, turns, and jog and walk in ten directions) and break, and the hover.
const ABILITY_ROLES: [&str; 26] = [
    "whirlwind",
    "heal_beam",
    "shield_idle",
    "shield_pivot_l",
    "shield_pivot_r",
    "shield_jog_f",
    "shield_jog_fl",
    "shield_jog_fr",
    "shield_jog_l",
    "shield_jog_r",
    "shield_jog_b",
    "shield_jog_bl",
    "shield_jog_br",
    "shield_jog_lb",
    "shield_jog_rb",
    "shield_walk_f",
    "shield_walk_fl",
    "shield_walk_fr",
    "shield_walk_l",
    "shield_walk_r",
    "shield_walk_b",
    "shield_walk_bl",
    "shield_walk_br",
    "shield_walk_lb",
    "shield_walk_rb",
    "shield_break",
];
/// The hover's pose (Nav_Hover), the last of them.
const ABILITY_HOVER: &str = "hover";
/// The shield stance's walk below this speed and jog above that (m/s).
const SHIELD_WALK_SPEED: f32 = 4.0;
const SHIELD_JOG_SPEED: f32 = 8.0;

/// The robot's extra clips (tools/build_robot_extras.py) and what main.rs reads of
/// them: each one's role, its number in the file, its length, whether it plays
/// on his upper body alone and whether it is additive (his reactions to melee
/// blows: hitreact.rs).
const ROBOT_EXTRAS_MODEL: &str = "models/robot_extras.glb";
const ROBOT_EXTRAS_FILE: &str = "models/robot_extras.txt";

fn robot_extras() -> Vec<(String, usize, f32, bool, bool)> {
    let Ok(text) = std::fs::read_to_string(assets_dir().join(ROBOT_EXTRAS_FILE)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            (f.len() >= 5 && f[0] == "clip").then(|| {
                (
                    f[1].to_string(),
                    f[2].parse().unwrap_or(0),
                    f[3].parse().unwrap_or(1.0),
                    f[4] == "upper" || f[4] == "add",
                    f[4] == "add",
                )
            })
        })
        .collect()
}

/// A class's robot graph: one node per clip in the ANIM_* order, its weights set
/// every frame. The ordinary clips are blended together; the additive ones (gun
/// recoil and moving parts) are then added on top: an additive blend node whose
/// first child is the ordinary blend, as the game layers its ADD_* animations. Each
/// clip is robot.glb's, or the class's own version of it (its robot_clips.glb); after
/// them come the class's characters' (its clips.glb, in its slots: ANIM_EXTRA). With
/// `mirrored`, the mirrored set too (robot_mirror.glb and the classes' mirrored
/// files), blended in for the gun in his left hand: its nodes. Last, the players'
/// `deaths` death clips (robot_deaths.glb's: downed.rs), ordinary ones: theirs;
/// then the robot's extra clips (robot_extras.glb's), ordinary but for those
/// `extras` marks additive.
fn robot_graph(
    class: usize,
    chars: &character::Characters,
    asset_server: &AssetServer,
    (graphs, clips): (&mut Assets<AnimationGraph>, &mut GltfClips),
    mirrored: bool,
    (deaths, extras): (usize, &[bool]),
) -> RobotGraph {
    let own = chars.class(class);
    let playable = chars.playable(class);
    let mut graph = AnimationGraph::new();
    let layers = graph.add_additive_blend(1.0, graph.root);
    let base = graph.add_blend(1.0, layers);
    // Each file's clips, by node: robot.glb's, the class's own robot clips and its
    // characters' clips.
    type Pending = Vec<(AnimationNodeIndex, usize)>;
    // (Robot.glb's, the class's own robot clips, its characters' clips.glb and their
    // arm_equip.glb: the forearms as the gun goes away and comes back.)
    let mut add_clips = |[robot, own_robot, extra, equip]: [&mut Pending; 4]| {
        (0..ANIM_TOTAL)
            .map(|i| {
                let parent = if !is_additive(i) { base } else { layers };
                let node = graph.add_clip(Handle::default(), 1.0, parent);
                if i < ANIM_COUNT {
                    match own.robot_clips.iter().find(|(r, _)| *r == i) {
                        Some(&(_, mine)) if playable => own_robot.push((node, mine)),
                        _ => robot.push((node, i)),
                    }
                } else if let Some(k) = own.clip_in_slot(i - ANIM_COUNT).filter(|_| playable) {
                    extra.push((node, k));
                } else if let Some(k) = own.equip_clip_in_slot(i - ANIM_COUNT).filter(|_| playable)
                {
                    equip.push((node, k));
                }
                node
            })
            .collect::<Vec<_>>()
    };
    let mut files: Vec<(String, Pending)> = Vec::new();
    let mut pending: [Pending; 4] = Default::default();
    let [a, b, c, d] = &mut pending;
    let nodes = add_clips([a, b, c, d]);
    let [robot, own_robot, extra, equip] = pending;
    files.push((ROBOT_MODEL.to_string(), robot));
    files.push((character::robot_clips_model(class), own_robot));
    files.push((character::clips_model(class), extra));
    files.push((character::arm_equip_model(class), equip));
    let mirror_nodes = mirrored.then(|| {
        let mut pending: [Pending; 4] = Default::default();
        let [a, b, c, d] = &mut pending;
        let nodes = add_clips([a, b, c, d]);
        let [robot, own_robot, extra, equip] = pending;
        files.push((ROBOT_MIRROR_MODEL.to_string(), robot));
        files.push((character::robot_clips_mirror_model(class), own_robot));
        files.push((character::clips_mirror_model(class), extra));
        files.push((character::arm_equip_mirror_model(class), equip));
        nodes
    });
    let mut pending = Vec::new();
    let death_nodes = (0..deaths)
        .map(|i| gltf_clip(&mut graph, &mut pending, i, base))
        .collect();
    files.push((downed::DEATHS_MODEL.to_string(), pending));
    let mut pending = Vec::new();
    let extra_nodes = extras
        .iter()
        .enumerate()
        .map(|(i, &add)| gltf_clip(&mut graph, &mut pending, i, if add { layers } else { base }))
        .collect();
    files.push((ROBOT_EXTRAS_MODEL.to_string(), pending));
    let graph = graphs.add(graph);
    for (file, pending) in files {
        if !pending.is_empty() && asset_exists(&file) {
            clips.wait(asset_server.load(file), graph.clone(), pending);
        }
    }
    RobotGraph {
        graph,
        nodes,
        mirrored: mirror_nodes,
        deaths: death_nodes,
        extras: extra_nodes,
    }
}

/// A class's robot graph and its nodes (robot_graph).
#[derive(Clone)]
struct RobotGraph {
    graph: Handle<AnimationGraph>,
    nodes: Vec<AnimationNodeIndex>,
    mirrored: Option<Vec<AnimationNodeIndex>>,
    deaths: Vec<AnimationNodeIndex>,
    extras: Vec<AnimationNodeIndex>,
}

impl RobotAnimations {
    /// A class's graph (the Infiltrator's for one that has none).
    fn graph(&self, class: usize) -> Handle<AnimationGraph> {
        self.graphs.get(class).unwrap_or(&self.graphs[0]).clone()
    }

    /// A clip's graph node and, with the mirrored set, the node of its mirror image's
    /// mirrored copy (which looks like the clip done with the other hand).
    fn pair(&self, clip: usize) -> impl Iterator<Item = AnimationNodeIndex> + '_ {
        std::iter::once(self.nodes[clip])
            .chain(self.mirrored.as_ref().map(|m| m[mirror_clip(clip)]))
    }

    /// Starts a clip (and its mirrored copy) over from the beginning at `speed`,
    /// `skip` seconds in.
    fn restart(&self, player: &mut AnimationPlayer, clip: usize, speed: f32, skip: f32) {
        for node in self.pair(clip) {
            let active = player.play(node);
            active.replay();
            active.set_speed(speed);
            if skip > 0.0 {
                active.seek_to(skip);
            }
        }
    }
}

/// Marks the robot model's AnimationPlayer once it has loaded.
#[derive(Component)]
struct RobotAnimator;

/// A model that plays its own clips (an Armory Recreator's, a Thermo Mine's, an
/// enemy's): the animation players under it are not robots' (attach_robot_animator
/// leaves them).
#[derive(Component)]
pub struct OwnAnimation;

#[derive(Component)]
struct RobotForm;

#[derive(Component)]
struct VehicleForm;

/// The normal size of a form (placeholders are scaled up to match the real models).
#[derive(Component)]
struct BaseScale(f32);

/// The robot model, drawn at its Jazz's class's size (Kit::robot, follow_looks).
#[derive(Component)]
struct ClassSized;

/// Plays a quick scale-up when a form becomes visible.
#[derive(Component)]
struct TransformPop(Timer);

/// Jazz's hand bones (his weapon is built onto the forearm), used as the muzzle:
/// the right one, or the left (true) with the gun in his left hand.
#[derive(Component)]
struct MuzzleBone(bool);

/// Jazz's elbow bones, which carry his gun's sockets (GunSockets): the right one,
/// or the left (true).
#[derive(Component)]
struct GunBone(bool);

/// One of Jazz's weapon meshes (index into WEAPONS, and whether it's the left-handed
/// one); only the one in use is shown (both while it moves to the other hand).
#[derive(Component)]
struct WeaponMesh(usize, bool);

/// A weapon's second mesh, on the CloudB gun bones: shown while it folds away.
#[derive(Component)]
struct WeaponMeshCloudB(usize, bool);

/// Jazz's forearm (and the wheel on it) on the left (true) or right side: shown on
/// the arm without the gun.
#[derive(Component)]
struct Forearm(bool);

/// The copies of Jazz's forearms (and their wheels) on his CloudB arm bones, on the
/// left (true) or right, shown while he switches hands in place of his forearms:
/// the game's forearms ride those bones, and its ADD_Weapon_MirrorReverse_ARM
/// (ANIM_ARM_SWITCH) folds one away while unfolding the other (see fold_forearms);
/// and the one on the gun's arm as it comes back out (holsters: the forearm set's
/// ADD_Weapon_Unequip_ARM folds it away there).
#[derive(Component)]
struct ForearmCloudB(bool);

/// A bone that switching hands shrinks or grows (fold_forearms): one of the CloudB
/// arm bones or a hand, on the left (true) or right arm, by name (a character's
/// forearm set says how: character.rs; Jazz's own, robot.glb's, by `part`).
#[derive(Component, Clone)]
struct SwitchBone {
    name: String,
    left: bool,
    part: Option<SwitchPart>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SwitchPart {
    /// CloudB_ElbowRobo<n>_XT.
    Elbow(u8),
    /// CloudB_ShoulderRobo<n>_XT.
    Shoulder(u8),
    /// Arm04_Hand_XB.
    Hand,
}

impl SwitchBone {
    /// The bone a robot.glb node name stands for, if switching hands scales it (as
    /// a character's forearms do, `named`, or Jazz's).
    fn from_name(name: &str, named: bool) -> Option<SwitchBone> {
        let (left, rest) = match name.split_at_checked(2)? {
            ("L_", rest) => (true, rest),
            ("R_", rest) => (false, rest),
            _ => return None,
        };
        let part = Self::jazz_part(rest);
        (named || part.is_some()).then(|| SwitchBone {
            name: name.to_string(),
            left,
            part,
        })
    }

    /// Which of Jazz's forearm bones a bone is (its side's prefix off).
    fn jazz_part(rest: &str) -> Option<SwitchPart> {
        let number = |s: &str, prefix: &str| -> Option<u8> {
            s.strip_prefix(prefix)?.strip_suffix("_XT")?.parse().ok()
        };
        if rest == "Arm04_Hand_XB" {
            Some(SwitchPart::Hand)
        } else if let Some(n) = number(rest, "CloudB_ElbowRobo") {
            Some(SwitchPart::Elbow(n))
        } else {
            number(rest, "CloudB_ShoulderRobo").map(SwitchPart::Shoulder)
        }
    }
}

/// Names of the bones shots come from while they are animated.
const MUZZLE_BONE: &str = "R_Arm04_Hand_XB";
const MUZZLE_BONE_LEFT: &str = "L_Arm04_Hand_XB";
/// And of the bones the guns' sockets are on.
const GUN_BONE: &str = "R_Arm03_Elbow_XB";
const GUN_BONE_LEFT: &str = "L_Arm03_Elbow_XB";

#[derive(Component)]
struct OrbitCamera {
    yaw: f32,
    pitch: f32,
    /// How far the vehicle's camera has moved from where it follows it driving to
    /// where it follows it hovering (character::VehicleCamera: 0 driving, 1
    /// hovering), and how fast it is moving.
    hover: f32,
    hover_vel: f32,
    /// Moving over to a new form's place (FORM_CAMERA_SECS): the form it is for, the
    /// offsets it set out from (rig_offsets'), how far it has come (0 to 1), and the
    /// offsets it was at last frame.
    rig_form: Option<Form>,
    rig_from: Vec4,
    rig_blend: f32,
    rig_last: Vec4,
    /// Current horizontal field of view in degrees (narrows for fine aim), and how
    /// fast it is changing.
    fov: f32,
    fov_vel: f32,
    /// How far the camera has moved in to a scope's close position (the class's
    /// character::Camera::scope: 0 out, 1 in), and how fast it is moving.
    scope: f32,
    scope_vel: f32,
    /// Which shoulder the camera is over (1 right, -1 left, in between while it
    /// moves across) and how fast it is moving.
    shoulder: f32,
    shoulder_vel: f32,
    /// The camera shaker's clock (ShakeWaves), the shaker, and this frame's shake
    /// (pitch, yaw and roll in degrees), which the HUD follows.
    shake_time: f32,
    shaker: Shaker,
    shake: Vec3,
    /// The rounds' blasts' shakes playing (Player::screen_shakes), and the random
    /// places their waves start from.
    screen_shakes: Vec<PlayingShake>,
    screen_seed: u32,
    /// Shots' recoil easing in (settle): the pitch kick still to come (radians, as
    /// a negative error) and the field of view's change shown (degrees), each with
    /// how fast it is changing.
    kick: (f32, f32),
    fov_kick: (f32, f32),
    /// How fast the mouse turns the view (degrees a second: yaw to the right, pitch
    /// up; PlayerInput::steer), and a flying jet's camera (jet.rs's view_turn and
    /// flying_anchor): how fast it turns the view (degrees a second, the same way),
    /// its anchor's offset (metres, the view's axes) and the view's roll (radians).
    mouse_rate: Vec2,
    fly_spin: Vec2,
    fly_anchor: Vec3,
    roll: f32,
    /// The heading the view looks along (the player's, `yaw`, but for a car's or
    /// truck's camera's smoothing of it, and the car's own just after it boosts:
    /// Rig::facing), which the player faces, and that camera (vehicle_cam.rs).
    view_yaw: f32,
    vcam: vehicle_cam::Rig,
    /// On foot, the camera sprinting (robot_cam.rs).
    robot: robot_cam::RobotCam,
    /// How far it has turned to look down at the player's body while he is down
    /// (0 to 1: DOWN_PITCH, over his middle), and how fast it is turning.
    down: f32,
    down_vel: f32,
    /// How far it has moved to the downed camera (revive.rs: 0 his own, 1 the
    /// downed one's, over DOWNED_CAMERA_SECS each way).
    downed: f32,
    /// Watching while he is out (revive.rs): whom (a slot; None the level's camera),
    /// and the glide there: where the camera set out from (its place, turn and field
    /// of view) and what it was on then (his body, or the player watched before;
    /// None the level's camera), how far it has come (0 to 1, over SPECTATE_SECS),
    /// and where it was last frame and what on.
    watching: Option<Option<usize>>,
    watch_from: (Vec3, Quat, f32),
    watch_from_on: Option<Vec3>,
    watch_blend: f32,
    watch_last: (Vec3, Quat, f32),
    watch_on: Option<Vec3>,
    /// The player dead last frame (revive.rs): coming back, the camera is his own
    /// again at once.
    was_dead: bool,
    /// The controller's right stick turning it (pad.rs).
    stick: pad::StickLook,
}

/// The downed camera (CAM_Strategies_p.Downed_Robot_STRATEGY, placed as the
/// game's own video shows it: in close over his right shoulder, a little behind,
/// 3 m over where he lies), its 60-degree view, and its TransitionTime there and
/// back.
const DOWNED_CAMERA: Vec4 = Vec4::new(1.4, 3.0, 1.3, 0.0);
const DOWNED_FOV: f32 = 60.0;
const DOWNED_CAMERA_SECS: f32 = 0.75;
/// Dead after being down, his death's camera (CAM_Strategies_p.Death_STRATEGY):
/// on his body as it lies (its anchor his lumbar bone), back out from the downed
/// camera's place to his own camera's round where he lay, looking as it was, as
/// the game's video shows it (his body at the bottom left), with a 90-degree view,
/// over its TransitionTime; and no turning it (it has no orbit rotation).
const DEATH_FOV: f32 = 90.0;
const DEATH_CAMERA_SECS: f32 = 1.25;
/// The spectating camera (CAM_Strategies_p.SpectateOverTheShoulder_STRATEGY): an
/// orbit the mouse turns round its anchor 2 m over the player watched
/// (HmOffsetAnchorPointRelativeToActorCameraBehavior), starting from his view
/// (TnSpectateCameraInitialViewCameraBehavior); its aim's rise by the view's pitch
/// (TnScreenSpaceOffsetByPitch: -0.35 m level, 1.5 m at the ends of its -75 to 75
/// degrees), as far back as the game's video shows it, an 80-degree view, and its
/// TransitionTime gliding from one player to another.
const SPECTATE_UP: f32 = 2.0;
const SPECTATE_RISE: (f32, f32) = (-0.35, 1.5);
const SPECTATE_PITCH_RANGE: f32 = 75.0;
const SPECTATE_BACK: f32 = 7.0;
const SPECTATE_FOV: f32 = 80.0;
const SPECTATE_SECS: f32 = 1.5;

/// While the player is down (downed.rs), the camera looks down at his body from
/// this pitch (radians, or lower if the view already looks lower), moving there
/// over this long.
const DOWN_PITCH: f32 = -0.55;
const DOWN_CAMERA_SECS: f32 = 0.8;

/// How quickly a shot's recoil eases in (CameraRecoil): the rate (per second) of
/// the critically damped spring that brings in its pitch kick and field-of-view
/// change, most of the way in a tenth of a second and all of it in about a fifth.
const RECOIL_EASE: f32 = 25.0;

/// One step of `dt` seconds of a critically damped spring pulling `x` (moving at
/// `v`) back to zero at rate `omega`: exact for any frame time. Returns the new
/// position and velocity.
fn settle(x: f32, v: f32, omega: f32, dt: f32) -> (f32, f32) {
    let decay = (-omega * dt).exp();
    let a = v + omega * x;
    ((x + a * dt) * decay, (v - omega * a * dt) * decay)
}

#[derive(Resource)]
struct Score {
    /// The enemies killed this match.
    destroyed: u32,
    /// Energon to spend (see STARTING_ENERGON).
    energon: u32,
}

impl Default for Score {
    fn default() -> Self {
        Score {
            destroyed: 0,
            energon: STARTING_ENERGON,
        }
    }
}

/// Solid round obstacles on the ground: (centre on the XZ plane, radius).
#[derive(Resource, Default)]
struct Colliders(Vec<(Vec2, f32)>);

/// The map in play (an index into MAPS), or None for the prop arena.
#[derive(Resource, Default)]
struct CurrentMap(Option<usize>);

/// Belongs to the map in play: its model, health stations, lights and sky, which
/// leave with it.
#[derive(Component)]
struct MapPart;

/// Where the fight takes place: the imported map (with triangle collision) or,
/// when `collision` is None, the flat prop arena.
#[derive(Resource, Default)]
struct Level {
    collision: Option<LevelCollision>,
    player_start: Vec3,
    /// The way the players face at the start (radians about the up axis, as
    /// headings and the camera's yaw turn).
    start_yaw: f32,
    /// Each player's spawn point (see `find_player_starts`).
    player_starts: Vec<Vec3>,
    /// The purchase points (shop.rs): the health stations, the ammo and heavy ammo
    /// stations, the weapon stations (with their weapons) and the equipment
    /// stations (with their equipment).
    health_stations: Vec<ShopSpot>,
    ammo_stations: Vec<ShopSpot>,
    heavy_ammo_stations: Vec<ShopSpot>,
    weapon_stations: Vec<(usize, ShopSpot)>,
    equipment_stations: Vec<(shop::Equipment, ShopSpot)>,
    /// The level camera: the map as the players see it before they come into the
    /// match (see `pre_match_view`).
    camera: Option<LevelCamera>,
    /// Its Armory Recreators (recreator.rs).
    pods: Vec<recreator::PodSpot>,
    /// Its locked doors and their unlock consoles (doors.rs).
    doors: doors::DoorsData,
    /// Its exploding barrels (barrels.rs).
    barrels: barrels::BarrelsData,
    /// The height below which the game takes a player out of the map (its
    /// TnWorldInfo's KillZ: the points file's "killz" line; KILL_Y without one): the
    /// space under the map, off its edges and down its pits.
    kill_y: Option<f32>,
}

/// The CameraActor a level names as its LevelCamera (tools/build_map.py): where it
/// stands, the way it looks (a heading as the start's, and its pitch, up positive:
/// radians) and its field of view across (degrees).
#[derive(Clone, Copy, Debug)]
struct LevelCamera {
    at: Vec3,
    heading: f32,
    pitch: f32,
    fov: f32,
}

/// The prop arena has no level camera: the start's view from behind and above.
const ARENA_VIEW_BACK: f32 = 9.0;
const ARENA_VIEW_UP: f32 = 4.0;
const ARENA_VIEW_PITCH: f32 = -0.25;
/// A CameraActor's field of view (degrees across) unless the level sets it.
const LEVEL_CAMERA_FOV: f32 = 90.0;

impl Level {
    /// Where the view is before the match begins (pregame.rs): through the level's
    /// camera (TnSpectateTargetChooser.GetLevelCamera: the players watch the map
    /// from it until they spawn), or for the arena from behind the start. Its
    /// place, its turn and its field of view across (degrees).
    fn pre_match_view(&self) -> (Vec3, Quat, f32) {
        match self.camera {
            Some(c) => (
                c.at,
                Quat::from_rotation_y(c.heading) * Quat::from_rotation_x(c.pitch),
                c.fov,
            ),
            None => {
                let turn = Quat::from_rotation_y(self.start_yaw);
                (
                    self.player_start + turn * Vec3::new(0.0, ARENA_VIEW_UP, ARENA_VIEW_BACK),
                    turn * Quat::from_rotation_x(ARENA_VIEW_PITCH),
                    LEVEL_CAMERA_FOV,
                )
            }
        }
    }

    /// Where player `slot` starts the match and comes back after being downed.
    fn start(&self, slot: usize) -> Vec3 {
        self.player_starts
            .get(slot)
            .copied()
            .unwrap_or(self.player_start)
    }

    /// Highest floor under a body of `radius` at `pos` between `top` and `bottom`.
    /// Samples the centre and four points around it so edges hold you up.
    fn ground(&self, pos: Vec3, radius: f32, top: f32, bottom: f32) -> Option<f32> {
        self.ground_for(pos, radius, top, bottom, collision::Mover::Player)
    }

    /// The same for `who` (an AI's pawn stands on no players-only wall).
    pub fn ground_for(
        &self,
        pos: Vec3,
        radius: f32,
        top: f32,
        bottom: f32,
        who: collision::Mover,
    ) -> Option<f32> {
        match &self.collision {
            Some(c) => {
                let r = radius * 0.5;
                [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)]
                    .iter()
                    .filter_map(|(dx, dz)| c.ground_for(pos.x + dx, pos.z + dz, top, bottom, who))
                    .reduce(f32::max)
            }
            None => (top >= 0.0 && bottom <= 0.0).then_some(0.0),
        }
    }

    /// The highest floor right under (x, z) between `top` and `bottom` (a vehicle's
    /// spring's: car.rs's ground_under), an overhang's underside only up to just
    /// over `feet`, as for `ground_moving`.
    pub fn ground_at(&self, x: f32, z: f32, top: f32, bottom: f32, feet: f32) -> Option<f32> {
        match &self.collision {
            Some(c) => c.ground_lift(x, z, top, bottom, collision::Mover::Player, feet + 0.05),
            None => (top >= 0.0 && bottom <= 0.0).then_some(0.0),
        }
    }

    /// The floor a player's body comes onto as he moves (`ground`), his feet having
    /// been at `feet` before: the underside of a rock's overhang (or of anything) is
    /// not a floor for him above that, so he is never lifted onto it from below and
    /// into the rock (collision.rs's ground_lift).
    fn ground_moving(
        &self,
        pos: Vec3,
        radius: f32,
        top: f32,
        bottom: f32,
        feet: f32,
    ) -> Option<f32> {
        let Some(c) = &self.collision else {
            return self.ground(pos, radius, top, bottom);
        };
        let r = radius * 0.5;
        [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)]
            .iter()
            .filter_map(|(dx, dz)| {
                c.ground_lift(
                    pos.x + dx,
                    pos.z + dz,
                    top,
                    bottom,
                    collision::Mover::Player,
                    feet + 0.05,
                )
            })
            .reduce(f32::max)
    }

    /// Highest floor under the whole of a flat-bottomed body of `radius` at `pos`
    /// (an AI's pawn's cylinder: UE3 finds its floor by sweeping it down, so it
    /// rests on any floor under its disc, a ledge's lip too) for `who`, between
    /// `top` and `bottom`: its centre and rings at half its radius and at its rim.
    pub fn ground_under_disc(
        &self,
        pos: Vec3,
        radius: f32,
        top: f32,
        bottom: f32,
        who: collision::Mover,
    ) -> Option<f32> {
        let Some(c) = &self.collision else {
            return self.ground_for(pos, radius, top, bottom, who);
        };
        let mut best = c.ground_for(pos.x, pos.z, top, bottom, who);
        for ring in [0.5f32, 0.98] {
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let (dx, dz) = (a.cos() * radius * ring, a.sin() * radius * ring);
                if let Some(y) = c.ground_for(pos.x + dx, pos.z + dz, top, bottom, who) {
                    best = Some(best.map_or(y, |b: f32| b.max(y)));
                }
            }
        }
        best
    }

    /// Lowest ceiling over a body of `radius` at `pos` between `bottom` and `top`
    /// (the blocking volumes' lids too), sampled as `ground` is.
    fn ceiling(&self, pos: Vec3, radius: f32, bottom: f32, top: f32) -> Option<f32> {
        self.ceiling_for(pos, radius, bottom, top, collision::Mover::Player)
    }

    /// The same for `who` (an AI's pawn passes under the players-only lids).
    pub fn ceiling_for(
        &self,
        pos: Vec3,
        radius: f32,
        bottom: f32,
        top: f32,
        who: collision::Mover,
    ) -> Option<f32> {
        let c = self.collision.as_ref()?;
        let r = radius * 0.5;
        [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)]
            .iter()
            .filter_map(|(dx, dz)| c.ceiling_for(pos.x + dx, pos.z + dz, bottom, top, who))
            .reduce(f32::min)
    }

    /// Sideways push that moves a body of `radius` and `height` (feet at pos.y)
    /// out of the level's walls.
    fn push_out(&self, pos: Vec3, radius: f32, height: f32) -> Option<Vec2> {
        self.push_out_for(pos, radius, height, collision::Mover::Player)
    }

    /// The same for `who` (an AI's pawn walks through the players-only walls: the
    /// enemies' spawn walls).
    pub fn push_out_for(
        &self,
        pos: Vec3,
        radius: f32,
        height: f32,
        who: collision::Mover,
    ) -> Option<Vec2> {
        let c = self.collision.as_ref()?;
        // Contacts lower than STEP_HEIGHT are ledges to step onto, so the test
        // spheres must sit above it; otherwise a body as low as the car (1.15 m)
        // would find every wall's nearest point below that and pass through.
        let low = (pos.y + radius.min(height * 0.5)).max(pos.y + STEP_HEIGHT + 0.05);
        let high = (pos.y + height - radius).max(low);
        c.push_out_for(pos.to_array(), radius, low, high, pos.y + STEP_HEIGHT, who)
            .map(|[x, z]| Vec2::new(x, z))
    }

    /// A part of a vehicle's body (a test sphere or circle of `radius` and `height`,
    /// its feet at `at`) on the far side of a wall from the vehicle's origin
    /// (`from`): the push that brings it back to the origin's side, its centre
    /// `radius` short of the wall; None while no wall stands between them. A body
    /// that comes into being overlapping a thin wall (a car or jet unfolding with
    /// its back to a spawn door) is then moved out on its own side, as the game
    /// finds it room (TnPawn.MoveToSafeTransformationLocation's
    /// FindSpotAwayFromPawns), rather than pushed on through the wall by the parts
    /// already past it.
    fn back_from_wall(&self, from: Vec3, at: Vec3, radius: f32, height: f32) -> Option<Vec2> {
        let flat = Vec2::new(at.x - from.x, at.z - from.z);
        let length = flat.length();
        if length < 1e-3 {
            return None;
        }
        let dir = flat / length;
        // (At the heights its walls are tested at: above a step, under its top.)
        let low = (at.y + radius.min(height * 0.5)).max(at.y + STEP_HEIGHT + 0.05);
        let high = (at.y + height - radius).max(low);
        [low, high]
            .into_iter()
            .filter_map(|y| {
                let origin = Vec3::new(from.x, y, from.z);
                self.ray_hit_for(
                    origin,
                    Vec3::new(dir.x, 0.0, dir.y),
                    length,
                    collision::Mover::Player,
                )
                .map(|(t, _)| t)
            })
            .reduce(f32::min)
            .map(|t| -dir * (length - t + radius))
    }

    /// The same for a vehicle: its body (body_circles along `forward`, a horizontal
    /// unit vector) pushed out of the walls together.
    fn push_out_car(
        &self,
        pos: Vec3,
        forward: Vec3,
        height: f32,
        body: character::Body,
    ) -> Option<Vec2> {
        let mut total = Vec2::ZERO;
        let mut touched = false;
        // Twice round, as pushing one end out can press the other into a wall.
        for _ in 0..2 {
            for ahead in body_circles(body) {
                let from = pos + Vec3::new(total.x, 0.0, total.y);
                let at = from + forward * ahead;
                if let Some(back) = self.back_from_wall(from, at, body.half_width, height) {
                    total += back;
                    touched = true;
                    continue;
                }
                if let Some(push) = self.push_out(at, body.half_width, height) {
                    total += push;
                    touched = true;
                }
            }
        }
        touched.then_some(total)
    }

    /// The same for a jet or a tank: its rigid body's box (character::Hull, its root
    /// `lift` over `pos`, facing along `forward`, a horizontal unit vector), from
    /// the floor under it (as its hover holds it over the floor) up to the box's top,
    /// as columns of spheres no taller than that (Hull::columns), pushed out
    /// together. (The vehicle body's circles are as wide as the vehicle: a wide, low
    /// one's would reach far over it, a jet's into the arches it flies under.)
    fn push_out_hull(
        &self,
        pos: Vec3,
        forward: Vec3,
        hull: &character::Hull,
        lift: f32,
    ) -> Option<Vec2> {
        let side = forward.cross(Vec3::Y);
        let height = (lift + hull.top).max(0.4);
        let hull = character::Hull {
            bottom: 0.0,
            top: height,
            ..*hull
        };
        let radius = hull.sphere_radius();
        let columns = hull.columns();
        let mut total = Vec2::ZERO;
        let mut touched = false;
        // Twice round, as pushing one end out can press the other into a wall.
        for _ in 0..2 {
            for c in &columns {
                let from = pos + Vec3::new(total.x, 0.0, total.y);
                let at = from + forward * c.y + side * c.x;
                if let Some(back) = self.back_from_wall(from, at, radius, height) {
                    total += back;
                    touched = true;
                    continue;
                }
                if let Some(push) = self.push_out(at, radius, height) {
                    total += push;
                    touched = true;
                }
            }
        }
        touched.then_some(total)
    }

    /// Distance to the first level surface along `dir` (unit length).
    fn ray(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        self.ray_hit(origin, dir, max).map(|(t, _)| t)
    }

    /// One of the rays of a trace with an extent for `who`: the walls that stop its
    /// movement stop it too (the blocking volumes, BlockNonZeroExtent).
    pub fn ray_hit_for(
        &self,
        origin: Vec3,
        dir: Vec3,
        max: f32,
        who: collision::Mover,
    ) -> Option<(f32, Vec3)> {
        match &self.collision {
            Some(c) => c
                .raycast_hit_for(origin.to_array(), dir.to_array(), max, who)
                .map(|(t, n)| (t, Vec3::from_array(n))),
            None => self.ray_hit(origin, dir, max),
        }
    }

    /// The same, with the surface's normal there (facing back against the ray).
    fn ray_hit(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<(f32, Vec3)> {
        match &self.collision {
            Some(c) => c
                .raycast_hit(origin.to_array(), dir.to_array(), max)
                .map(|(t, n)| (t, Vec3::from_array(n))),
            None => {
                // The arena floor.
                (dir.y < -1e-4 && origin.y >= 0.0)
                    .then(|| origin.y / -dir.y)
                    .filter(|t| *t <= max)
                    .map(|t| (t, Vec3::Y))
            }
        }
    }

    /// The level's surfaces (the triangles rays hit) reaching into the box from
    /// `min` to `max`, as their corners: what a decal there lies on. None without a
    /// level (the arena, whose floor is a plane).
    fn surfaces_in(&self, min: Vec3, max: Vec3) -> Option<Vec<[Vec3; 3]>> {
        let c = self.collision.as_ref()?;
        let mut tris = Vec::new();
        c.solid_triangles_in(min.to_array(), max.to_array(), &mut tris);
        Some(tris.iter().map(|t| t.map(Vec3::from_array)).collect())
    }

    /// First hit on the segment from `a` to `b`, as a point.
    fn segment_hit(&self, a: Vec3, b: Vec3) -> Option<Vec3> {
        let d = b - a;
        let l = d.length();
        if l < 1e-5 {
            return None;
        }
        let dir = d / l;
        self.ray(a, dir, l).map(|t| a + dir * t)
    }

    /// True if level geometry blocks the straight line between two points.
    fn blocked(&self, a: Vec3, b: Vec3) -> bool {
        self.collision.is_some() && self.segment_hit(a, b).is_some()
    }

    /// Distance along a shot (unit `dir`) to the first wall, floor or arena
    /// pillar it strikes within `max`.
    fn shot_hit(&self, colliders: &Colliders, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        self.shot_surface(colliders, origin, dir, max)
            .map(|(t, _)| t)
    }

    /// The same, with the struck surface's normal (facing back towards the shot):
    /// impacts play along it, and bullet holes lie on it.
    fn shot_surface(
        &self,
        colliders: &Colliders,
        origin: Vec3,
        dir: Vec3,
        max: f32,
    ) -> Option<(f32, Vec3)> {
        let mut nearest = self.ray_hit(origin, dir, max);
        for (centre, radius) in &colliders.0 {
            if let Some(t) = ray_cylinder(origin, dir, *centre, *radius, 0.0, PISTON_HEIGHT) {
                if t <= nearest.map_or(max, |(n, _)| n) {
                    // Its side, or its top.
                    let p = origin + dir * t;
                    let out = Vec3::new(p.x - centre.x, 0.0, p.z - centre.y);
                    let normal = if p.y >= PISTON_HEIGHT - 1e-3 {
                        Vec3::Y
                    } else {
                        out.normalize_or(-dir)
                    };
                    nearest = Some((t, normal));
                }
            }
        }
        nearest
    }
}

/// Distance along a ray (unit `dir`) to where it enters an upright cylinder standing
/// on `centre` (XZ plane) from height `bottom` to `top`; 0 if it starts inside.
fn ray_cylinder(
    origin: Vec3,
    dir: Vec3,
    centre: Vec2,
    radius: f32,
    bottom: f32,
    top: f32,
) -> Option<f32> {
    let o = Vec2::new(origin.x - centre.x, origin.z - centre.y);
    let d = Vec2::new(dir.x, dir.z);
    let r2 = radius * radius;
    let height_ok = |t: f32| {
        let y = origin.y + dir.y * t;
        y >= bottom && y <= top
    };
    if o.length_squared() <= r2 && height_ok(0.0) {
        return Some(0.0);
    }
    let mut nearest: Option<f32> = None;
    let mut keep = |t: f32| {
        if t >= 0.0 && nearest.is_none_or(|n| t < n) {
            nearest = Some(t);
        }
    };
    // The side.
    let a = d.length_squared();
    if a > 1e-8 {
        let b = o.dot(d);
        let disc = b * b - a * (o.length_squared() - r2);
        if disc >= 0.0 {
            let t = (-b - disc.sqrt()) / a;
            if height_ok(t) {
                keep(t);
            }
        }
    }
    // The top and bottom.
    if dir.y.abs() > 1e-6 {
        for h in [bottom, top] {
            let t = (h - origin.y) / dir.y;
            if (o + d * t).length_squared() <= r2 {
                keep(t);
            }
        }
    }
    nearest
}

/// Loads a map's collision and spawn points. None if the files are missing.
fn load_level_data(map: &MapInfo) -> Option<Level> {
    let dir = assets_dir();
    let collision = match LevelCollision::load(&dir.join(map.collision())) {
        Ok(c) => c,
        Err(e) => {
            warn!("{} could not be loaded ({e})", map.collision());
            return None;
        }
    };
    let text = std::fs::read_to_string(dir.join(map.points())).unwrap_or_default();
    let mut start = None;
    let mut start_yaw = 0.0;
    let mut camera = None;
    let mut health = Vec::new();
    let mut ammo = Vec::new();
    let mut heavy_ammo = Vec::new();
    let mut weapons = Vec::new();
    let mut equipment = Vec::new();
    let mut kill_y = None;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let kind = parts.next().unwrap_or("");
        // (A weapon or equipment station names what it sells first.)
        let class = if matches!(kind, "weapon" | "equipment") {
            parts.next().unwrap_or("")
        } else {
            ""
        };
        let v: Vec<f32> = parts.filter_map(|p| p.parse().ok()).collect();
        // killz y (metres): the map's KillZ.
        if kind == "killz" {
            kill_y = v.first().copied();
            continue;
        }
        if v.len() < 3 {
            continue;
        }
        let p = Vec3::new(v[0], v[1], v[2]);
        match kind {
            // x y z, and the way to face in degrees.
            "start" => {
                start = Some(p);
                start_yaw = v.get(3).copied().unwrap_or(0.0).to_radians();
            }
            // x y z, its heading and pitch (degrees), its field of view across.
            "camera" => {
                camera = Some(LevelCamera {
                    at: p,
                    heading: v.get(3).copied().unwrap_or(0.0).to_radians(),
                    pitch: v.get(4).copied().unwrap_or(0.0).to_radians(),
                    fov: v.get(5).copied().unwrap_or(LEVEL_CAMERA_FOV),
                });
            }
            // (The enemy spawn points are the waves' own: waves.rs.)
            // The purchase points: x y z, the turn in degrees, the price and the
            // respawn time (a weapon's or equipment's class first).
            "health" => health.extend(ShopSpot::parse(&v, HEALTH_COST, HEALTH_RESPAWN_SECS)),
            "ammo" => ammo.extend(ShopSpot::parse(&v, 100, 10.0)),
            "heavyammo" => heavy_ammo.extend(ShopSpot::parse(&v, 100, 40.0)),
            "weapon" => weapons.extend(shop::parse_weapon(class, &v)),
            "equipment" => equipment.extend(shop::parse_equipment(class, &v)),
            _ => {}
        }
    }
    // Its locked doors and unlock consoles, their collision put in (doors.rs: the
    // enemy spawn clusters behind them are off as gameplay starts, waves.rs).
    let (doors, collision) = doors::DoorsData::load(&dir, map.stem, collision);
    // Its exploding barrels, their collision put in (barrels.rs).
    let (barrels, collision) = barrels::BarrelsData::load(&dir, map.stem, collision);
    // The start on the floor below it.
    let snap = |p: Vec3| {
        collision
            .ground(p.x, p.z, p.y + 1.0, p.y - 8.0)
            .map(|y| Vec3::new(p.x, y, p.z))
    };
    // (The stations stand where the map puts them, on the floor already.)
    let health_stations = health;
    let player_start = start.and_then(snap).or(start).unwrap_or(Vec3::ZERO);
    info!(
        "{}: {} collision triangles, {} health, {} ammo, {} heavy ammo, {} weapon and {} \
         equipment stations",
        map.name,
        collision.triangle_count(),
        health_stations.len(),
        ammo.len(),
        heavy_ammo.len(),
        weapons.len(),
        equipment.len()
    );
    // Its Armory Recreators, and a box for each where the map has no collision for
    // them (recreator.rs).
    let (pods, pods_solid) = recreator::load_pods(&dir.join(map.pods()));
    let collision = if pods_solid {
        collision.with_solids(&recreator::pod_boxes(&pods))
    } else {
        collision
    };
    Some(Level {
        collision: Some(collision),
        player_start,
        start_yaw,
        player_starts: Vec::new(),
        health_stations,
        ammo_stations: ammo,
        heavy_ammo_stations: heavy_ammo,
        weapon_stations: weapons,
        equipment_stations: equipment,
        camera,
        pods,
        doors,
        barrels,
        kill_y,
    })
}

/// A map has one player start (its TnTeamPlayerStart); the four players start
/// there and on open floor around it: the first places 3 m away (then 5 m) that
/// have floor at about the same height, no wall in the way from the start, and room
/// to stand.
fn find_player_starts(level: &Level, colliders: &Colliders) -> Vec<Vec3> {
    let start = level.player_start;
    let mut starts = vec![start];
    for ring in [3.0, 5.0] {
        for k in 0..8 {
            if starts.len() == MAX_PLAYERS {
                return starts;
            }
            let angle = k as f32 * TAU / 8.0;
            let mut spot = start + Vec3::new(angle.sin(), 0.0, angle.cos()) * ring;
            let Some(floor) = level.ground(spot, ROBOT_RADIUS, start.y + 1.5, start.y - 1.5) else {
                continue;
            };
            spot.y = floor;
            let walled = level.push_out(spot, ROBOT_RADIUS, ROBOT_HEIGHT).is_some()
                || level.blocked(start + Vec3::Y, spot + Vec3::Y)
                || colliders
                    .0
                    .iter()
                    .any(|(c, r)| Vec2::new(spot.x, spot.z).distance(*c) < r + ROBOT_RADIUS + 0.5);
            let crowded = starts.iter().any(|s| s.distance(spot) < 2.5);
            if !walled && !crowded {
                starts.push(spot);
            }
        }
    }
    // (Nowhere else to stand: the rest share the start.)
    starts.resize(MAX_PLAYERS, start);
    starts
}

#[derive(Resource)]
struct DemoAssets {
    /// How the health stations' energon cubes are drawn.
    energon: energon::CubeLook,
}

// ---------------------------------------------------------------------------
// Asset helpers
// ---------------------------------------------------------------------------

/// Finds the `assets` folder (and Bevy is told to load from it too, in main): the
/// one in the folder BEVY_ASSET_ROOT names, or in the project folder under `cargo
/// run`; otherwise the nearest one next to the program or in a folder above it, so
/// that `target\debug\foc_esc.exe` started by itself (double-clicked, or by an
/// IDE) finds the project's own two folders up. Worked out once.
fn assets_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        for var in ["BEVY_ASSET_ROOT", "CARGO_MANIFEST_DIR"] {
            if let Ok(root) = std::env::var(var) {
                return PathBuf::from(root).join("assets");
            }
        }
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.to_path_buf()));
        let Some(exe_dir) = exe_dir else {
            return PathBuf::from("assets");
        };
        exe_dir
            .ancestors()
            .take(4)
            .map(|dir| dir.join("assets"))
            .find(|assets| assets.is_dir())
            .unwrap_or_else(|| exe_dir.join("assets"))
    })
    .clone()
}

fn asset_exists(relative: &str) -> bool {
    let found = assets_dir().join(relative).exists();
    if !found {
        info!("{relative} not found, using a placeholder");
    }
    found
}

fn gltf_scene(asset_server: &AssetServer, path: &'static str) -> SceneRoot {
    SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(path)))
}

/// Animation clips from glTF files, for animation graphs. Asked for one by one by
/// their labels (`robot.glb#Animation7`) before its file is in, Bevy loads the whole
/// file again for every clip (the robot has a hundred, each load with all its
/// textures): so a file is loaded once, its clips go into a graph as placeholders
/// (`gltf_clip`), and they are put in place when it is in (`fill_gltf_clips`). The
/// files are kept (let go and asked for again, by another graph, a file would be read
/// again: the enemies' npc_clips.glb, each type's).
#[derive(Resource, Default)]
pub struct GltfClips(Vec<PendingClips>, Vec<Handle<Gltf>>);

struct PendingClips {
    file: Handle<Gltf>,
    graph: Handle<AnimationGraph>,
    /// Each graph node, and the number of the file's clip it plays.
    nodes: Vec<(AnimationNodeIndex, usize)>,
}

impl GltfClips {
    /// `graph`'s clip nodes in `nodes` get their clips from `file` when it is in.
    pub fn wait(
        &mut self,
        file: Handle<Gltf>,
        graph: Handle<AnimationGraph>,
        nodes: Vec<(AnimationNodeIndex, usize)>,
    ) {
        self.0.push(PendingClips { file, graph, nodes });
    }
}

/// A clip node for clip `index` of a glTF file (in place once the file is in: it is
/// noted in `pending`, for `GltfClips::wait`).
pub fn gltf_clip(
    graph: &mut AnimationGraph,
    pending: &mut Vec<(AnimationNodeIndex, usize)>,
    index: usize,
    parent: AnimationNodeIndex,
) -> AnimationNodeIndex {
    let node = graph.add_clip(Handle::default(), 1.0, parent);
    pending.push((node, index));
    node
}

/// Puts the clips of the files that have come in into their graphs.
fn fill_gltf_clips(
    mut clips: ResMut<GltfClips>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    asset_server: Res<AssetServer>,
) {
    if clips.0.is_empty() {
        return;
    }
    let clips = &mut *clips;
    let kept = &mut clips.1;
    clips.0.retain(|pending| {
        let Some(gltf) = gltfs.get(&pending.file) else {
            // (Gone if it can't be read.)
            return !matches!(
                asset_server.load_state(&pending.file),
                bevy::asset::LoadState::Failed(_)
            );
        };
        if let Some(graph) = graphs.get_mut(&pending.graph) {
            for &(node, index) in &pending.nodes {
                if let (Some(node), Some(clip)) = (graph.get_mut(node), gltf.animations.get(index))
                {
                    node.node_type = AnimationNodeType::Clip(clip.clone());
                }
            }
        }
        if !kept.contains(&pending.file) {
            kept.push(pending.file.clone());
        }
        false
    });
}

/// Cheap repeatable pseudo-random number in 0..1, so we need no extra crates.
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

/// Light for a place without lighting of its own (lighting.rs): a low warm sunset
/// key light, plus a cool fill from the other side. On a map it is weaker and
/// casts no shadows (see main). Returns the two lights.
fn spawn_plain_lights(commands: &mut Commands, on_map: bool) -> [Entity; 2] {
    [
        commands
            .spawn((
                DirectionalLight {
                    illuminance: if on_map { 6_000.0 } else { 9_000.0 },
                    shadows_enabled: !on_map,
                    color: Color::srgb(1.0, 0.78, 0.58),
                    ..default()
                },
                Transform::from_xyz(-45.0, 70.0, 30.0).looking_at(Vec3::ZERO, Vec3::Y),
            ))
            .id(),
        commands
            .spawn((
                DirectionalLight {
                    illuminance: 1_800.0,
                    shadows_enabled: false,
                    color: Color::srgb(0.55, 0.65, 1.0),
                    ..default()
                },
                Transform::from_xyz(50.0, 40.0, -50.0).looking_at(Vec3::ZERO, Vec3::Y),
            ))
            .id(),
    ]
}

/// The sky dome from the game (SKY_MODEL), drawn unlit and outside the fog, ten
/// times its size, standing at `at` turned `yaw` degrees. None if it isn't there.
/// (Its mesh and picture from its file read once: gltffiles.rs.)
fn spawn_sky(
    commands: &mut Commands,
    (asset_server, parts, meshes, images): (
        &AssetServer,
        &mut gltffiles::GltfParts,
        &Assets<Mesh>,
        &Assets<Image>,
    ),
    materials: &mut Assets<StandardMaterial>,
    at: Vec3,
    yaw: f32,
) -> Option<Entity> {
    if !asset_exists(SKY_MODEL) {
        return None;
    }
    let mesh = parts.mesh(asset_server, meshes, SKY_MODEL, 0, 0);
    let texture = parts.texture(asset_server, images, SKY_MODEL, 0);
    let material = materials.add(StandardMaterial {
        base_color_texture: Some(texture),
        unlit: true,
        fog_enabled: false,
        cull_mode: None,
        ..default()
    });
    Some(
        commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::from_translation(at)
                    .with_rotation(Quat::from_rotation_y(yaw.to_radians()))
                    .with_scale(Vec3::splat(10.0)),
                NotShadowCaster,
            ))
            .id(),
    )
}

fn setup_world(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    (mut parts, images): (ResMut<gltffiles::GltfParts>, Res<Assets<Image>>),
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cube_materials: ResMut<Assets<energon::EnergonCubeMaterial>>,
) {
    let energon = energon::CubeLook::new(
        asset_exists(ENERGON_MODEL).then_some(ENERGON_MODEL),
        (&asset_server, &mut parts, &images),
        &mut meshes,
        &mut materials,
        &mut cube_materials,
    );
    commands.insert_resource(DemoAssets { energon });
}

/// What putting a map in play takes (see `load`).
#[derive(SystemParam)]
struct MapLoader<'w, 's> {
    commands: Commands<'w, 's>,
    asset_server: Res<'w, AssetServer>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    luts: Option<Res<'w, TonemappingLuts>>,
    grade: ResMut<'w, lighting::Grade>,
    pages: ResMut<'w, lighting::LightmapPages>,
    current: ResMut<'w, CurrentMap>,
    assets: Res<'w, DemoAssets>,
    parts: Query<'w, 's, Entity, With<MapPart>>,
    cameras: Query<'w, 's, Entity, With<OrbitCamera>>,
    gltf_parts: ResMut<'w, gltffiles::GltfParts>,
}

impl MapLoader<'_, '_> {
    /// Puts map `index` (of MAPS) in play in place of the one there: its model,
    /// collision and spawn points (the Level), health stations, lighting (with the
    /// camera's fog, bloom and colour grade) and sky. False, changing nothing, if
    /// its collision or spawn points can't be read.
    fn load(&mut self, index: usize) -> bool {
        let map = &MAPS[index];
        let Some(mut level) = load_level_data(map) else {
            return false;
        };
        // (The prop arena's pillars aren't on a map.)
        level.player_starts = find_player_starts(&level, &Colliders::default());
        for part in &self.parts {
            self.commands.entity(part).despawn();
        }
        self.pages.clear();
        let commands = &mut self.commands;
        let scene = self.asset_server.load_with_settings(
            GltfAssetLabel::Scene(0).from_asset(map.model()),
            // (Filtered as the Graphics level the demo started at has the maps'
            // textures filtered: graphics.rs.)
            |settings: &mut GltfLoaderSettings| {
                settings.default_sampler = Some(graphics::map_sampler());
            },
        );
        // (Hidden until its parts are joined: merge.rs.)
        commands.spawn((
            MapPart,
            merge::MapScene,
            SceneRoot(scene),
            Transform::default(),
            Visibility::Hidden,
        ));
        for station in spawn_health_stations(
            commands,
            &self.asset_server,
            &mut self.meshes,
            &mut self.materials,
            &self.assets.energon,
            &level.health_stations,
        ) {
            commands.entity(station).insert(MapPart);
        }
        for (stations, heavy) in [
            (&level.ammo_stations, false),
            (&level.heavy_ammo_stations, true),
        ] {
            for station in spawn_ammo_stations(
                commands,
                &self.asset_server,
                &mut self.meshes,
                &mut self.materials,
                stations,
                heavy,
            ) {
                commands.entity(station).insert(MapPart);
            }
        }
        let shop =
            shop::spawn_weapon_stations(commands, &self.asset_server, &level.weapon_stations)
                .into_iter()
                .chain(shop::spawn_equipment_stations(
                    commands,
                    &self.asset_server,
                    &level.equipment_stations,
                ));
        for station in shop.collect::<Vec<_>>() {
            commands.entity(station).insert(MapPart);
        }
        for pod in recreator::spawn_pods(commands, &self.asset_server, &level.pods) {
            commands.entity(pod).insert(MapPart);
        }
        for part in doors::spawn(commands, &self.asset_server, &level.doors) {
            commands.entity(part).insert(MapPart);
        }
        commands.insert_resource(doors::DoorsState::new(level.doors.clone()));
        for barrel in barrels::spawn(commands, &self.asset_server, &level.barrels) {
            commands.entity(barrel).insert(MapPart);
        }
        commands.insert_resource(barrels::Barrels::new(level.barrels.clone()));
        // Its navigation for the enemies' AI (nav.rs).
        commands.insert_resource(nav::Nav::new(nav::NavMesh::load(&assets_dir(), map.stem)));
        // (Its spinning actors and pulsing glows: mapmotion.rs.)
        commands.insert_resource(mapmotion::MapMotion::load(&assets_dir(), map.stem));
        match lighting::MapLighting::load(&assets_dir(), map.stem) {
            Some(lights) => {
                let luts = self.luts.as_deref();
                for light in lights.light(commands, &mut self.grade, luts, &mut self.images) {
                    commands.entity(light).insert(MapPart);
                }
                for camera in &self.cameras {
                    lights.camera_effects(&mut commands.entity(camera));
                }
                commands.insert_resource(lights);
            }
            None => {
                for light in spawn_plain_lights(commands, true) {
                    commands.entity(light).insert(MapPart);
                }
                commands.insert_resource(GlobalAmbientLight {
                    color: Color::srgb(1.0, 0.85, 0.75),
                    brightness: 900.0,
                    ..default()
                });
                self.grade.set(None, self.luts.as_deref(), &mut self.images);
                commands.remove_resource::<lighting::MapLighting>();
            }
        }
        if let Some((at, yaw)) = map.sky
            && let Some(sky) = spawn_sky(
                commands,
                (
                    &self.asset_server,
                    &mut self.gltf_parts,
                    &self.meshes,
                    &self.images,
                ),
                &mut self.materials,
                Vec3::from_array(at),
                yaw,
            )
        {
            commands.entity(sky).insert(MapPart);
        }
        commands.insert_resource(level);
        self.current.0 = Some(index);
        info!("Map in play: {}", map.name);
        true
    }
}

/// Puts the lobby's map (the host's choice) in play once the countdown to the match
/// starts, so that it loads meanwhile, and at the latest as the match starts.
fn follow_map_choice(net: Res<net::Net>, mut loader: MapLoader, mut failed: Local<Option<usize>>) {
    let wanted = net.map();
    if loader.current.0 == Some(wanted)
        || *failed == Some(wanted)
        || !(net.countdown().is_some() || net.in_match())
    {
        return;
    }
    // (A map that hasn't been built here, or can't be read: the match is played on
    // the one in play.)
    if !built_maps().contains(&wanted) {
        let name = MAPS.get(wanted).map_or("The host's map", |m| m.name);
        warn!("{name} hasn't been built here (see the README): playing on the map in play");
        *failed = Some(wanted);
    } else if !loader.load(wanted) {
        *failed = Some(wanted);
    }
}

/// Puts the first map that has been built in play (the lobby can pick another), or
/// builds the arena from the game's props. Edit the lists here to change its layout.
fn setup_level(mut loader: MapLoader, mut colliders: ResMut<Colliders>) {
    if built_maps().iter().any(|&map| loader.load(map)) {
        return;
    }
    let MapLoader {
        mut commands,
        asset_server,
        mut meshes,
        mut materials,
        images,
        mut gltf_parts,
        ..
    } = loader;

    // (A map brings its own light and sky.)
    spawn_plain_lights(&mut commands, false);
    spawn_sky(
        &mut commands,
        (&asset_server, &mut gltf_parts, &meshes, &images),
        &mut materials,
        Vec3::new(0.0, -40.0, 0.0),
        0.0,
    );

    // Floor.
    if asset_exists(FLOOR_MODEL) {
        commands.spawn((gltf_scene(&asset_server, FLOOR_MODEL), Transform::default()));
    } else {
        let size = ARENA_HALF_SIZE * 2.6;
        commands.spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(size, size))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.19, 0.22),
                metallic: 0.8,
                perceptual_roughness: 0.55,
                ..default()
            })),
        ));
    }

    // Perimeter walls: 7 wall sections per side, facing inwards.
    let wall_scale = 0.75;
    let wall_width = 40.96 * wall_scale;
    let edge = ARENA_HALF_SIZE + 6.0;
    let sides = [
        (Vec3::new(0.0, 0.0, -edge), 0.0, Vec3::X),
        (Vec3::new(0.0, 0.0, edge), PI, Vec3::X),
        (Vec3::new(-edge, 0.0, 0.0), FRAC_PI_2, Vec3::Z),
        (Vec3::new(edge, 0.0, 0.0), -FRAC_PI_2, Vec3::Z),
    ];
    let mut wall_spots = Vec::new();
    for (centre, yaw, along) in sides {
        for k in -3..=3 {
            wall_spots.push((centre + along * (k as f32 * wall_width), yaw, wall_scale));
        }
    }
    let have_walls = spawn_props(&mut commands, &asset_server, WALL_MODEL, &wall_spots);
    if !have_walls {
        let wall_mesh = meshes.add(Cuboid::new(wall_width, 12.0, 2.0));
        let wall_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.28, 0.3, 0.35),
            metallic: 0.9,
            perceptual_roughness: 0.4,
            ..default()
        });
        for (pos, yaw, _) in &wall_spots {
            commands.spawn((
                Mesh3d(wall_mesh.clone()),
                MeshMaterial3d(wall_mat.clone()),
                Transform::from_translation(*pos + Vec3::Y * 6.0)
                    .with_rotation(Quat::from_rotation_y(*yaw)),
            ));
        }
    }

    // Vent grates and pipe runs along the walls (decoration only).
    let vent_spots: Vec<(Vec3, f32, f32)> = [-60.0, -20.0, 20.0, 60.0]
        .iter()
        .flat_map(|x| {
            [
                (Vec3::new(*x, 2.4, -ARENA_HALF_SIZE - 2.5), 0.0, 1.5),
                (Vec3::new(*x, 2.4, ARENA_HALF_SIZE + 2.5), PI, 1.5),
            ]
        })
        .collect();
    spawn_props(&mut commands, &asset_server, VENT_MODEL, &vent_spots);
    let pipe_spots: Vec<(Vec3, f32, f32)> = [-50.0, 0.0, 50.0]
        .iter()
        .flat_map(|z| {
            [
                (Vec3::new(-ARENA_HALF_SIZE - 2.0, 0.0, *z), 0.0, 1.5),
                (Vec3::new(ARENA_HALF_SIZE + 2.0, 0.0, *z), PI, 1.5),
            ]
        })
        .collect();
    spawn_props(&mut commands, &asset_server, PIPES_MODEL, &pipe_spots);

    // Piston towers used as cover. These block movement and shots.
    let mut pillars = Vec::new();
    for i in 0..10u32 {
        let angle = i as f32 / 10.0 * TAU + 0.3;
        let radius = if i % 2 == 0 { 38.0 } else { 70.0 };
        pillars.push(Vec2::new(angle.cos(), angle.sin()) * radius);
    }
    pillars.extend([
        Vec2::new(18.0, 12.0),
        Vec2::new(-16.0, -20.0),
        Vec2::new(-24.0, 22.0),
    ]);
    let piston_spots: Vec<(Vec3, f32, f32)> = pillars
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                Vec3::new(p.x, 0.0, p.y),
                hash01(i as u32) * TAU,
                PISTON_SCALE,
            )
        })
        .collect();
    if !spawn_props(&mut commands, &asset_server, PISTON_MODEL, &piston_spots) {
        let mesh = meshes.add(Cylinder::new(PISTON_RADIUS, PISTON_HEIGHT));
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.4, 0.28, 0.22),
            metallic: 0.8,
            perceptual_roughness: 0.6,
            ..default()
        });
        for p in &pillars {
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_xyz(p.x, PISTON_HEIGHT / 2.0, p.y),
            ));
        }
    }
    colliders.0 = pillars.iter().map(|p| (*p, PISTON_RADIUS)).collect();
    let mut level = Level::default();
    level.player_starts = find_player_starts(&level, &colliders);
    commands.insert_resource(level);
}

/// Spawns one model at each (position, yaw, scale). Returns false if the model is missing.
fn spawn_props(
    commands: &mut Commands,
    asset_server: &AssetServer,
    path: &'static str,
    spots: &[(Vec3, f32, f32)],
) -> bool {
    if !asset_exists(path) {
        return false;
    }
    let scene: Handle<Scene> = asset_server.load(GltfAssetLabel::Scene(0).from_asset(path));
    for (pos, yaw, scale) in spots {
        commands.spawn((
            SceneRoot(scene.clone()),
            Transform::from_translation(*pos)
                .with_rotation(Quat::from_rotation_y(*yaw))
                .with_scale(Vec3::splat(*scale)),
        ));
    }
    true
}

fn setup_player(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    level: Res<Level>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut clips: ResMut<GltfClips>,
    chars: Res<character::Characters>,
) {
    // The robot's animation graphs, one per class, shared by every Jazz of it, with
    // the players' deaths (downed.rs) when they are built.
    if asset_exists(ROBOT_MODEL) {
        let mirrored = asset_exists(ROBOT_MIRROR_MODEL);
        let deaths = downed::PlayerDeaths::load(&assets_dir());
        let death_clips = deaths.as_ref().map_or(0, |d| d.count);
        match deaths {
            Some(d) => commands.insert_resource(d),
            None => info!(
                "{} not built: the players die in the old pose",
                downed::DEATHS_FILE
            ),
        }
        // (The robot's extra clips, when they are built.)
        let extras = if asset_exists(ROBOT_EXTRAS_MODEL) {
            robot_extras()
        } else {
            Vec::new()
        };
        let extra_clips = extras.iter().map(|e| e.1 + 1).max().unwrap_or(0);
        let mut additive = vec![false; extra_clips];
        for e in extras.iter().filter(|e| e.4) {
            additive[e.1] = true;
        }
        let mut made: Vec<RobotGraph> = Vec::new();
        for class in 0..character::CLASSES.len() {
            // (A class without its characters plays the Infiltrator's.)
            if class > 0 && !chars.playable(class) {
                let first = made[0].clone();
                made.push(first);
                continue;
            }
            made.push(robot_graph(
                class,
                &chars,
                &asset_server,
                (&mut graphs, &mut clips),
                mirrored,
                (death_clips, &additive),
            ));
        }
        let first = made[0].clone();
        commands.insert_resource(RobotAnimations {
            graphs: made.into_iter().map(|g| g.graph).collect(),
            nodes: first.nodes,
            mirrored: first.mirrored,
            deaths: first.deaths,
            trigger: extras
                .iter()
                .find(|e| e.0 == "trigger")
                .and_then(|e| first.extras.get(e.1).map(|&node| (node, e.2))),
            downed: {
                let clip = |role: &str| {
                    let e = extras.iter().find(|e| e.0 == role)?;
                    first.extras.get(e.1).map(|&node| (node, e.2))
                };
                let found: Vec<_> = DOWNED_ROLES.iter().filter_map(|r| clip(r)).collect();
                <[(AnimationNodeIndex, f32); DOWNED_CLIPS]>::try_from(found).ok()
            },
            melee_react: {
                let found: Vec<_> = hitreact::ROLES
                    .iter()
                    .filter_map(|role| {
                        let e = extras.iter().find(|e| e.0 == *role && e.4)?;
                        first.extras.get(e.1).map(|&node| (node, e.2))
                    })
                    .collect();
                <[(AnimationNodeIndex, f32); 3]>::try_from(found).ok()
            },
            ability: ABILITY_ROLES
                .iter()
                .chain([&ABILITY_HOVER])
                .filter_map(|role| {
                    let e = extras.iter().find(|e| e.0 == *role)?;
                    first.extras.get(e.1).map(|&node| (*role, node, e.2))
                })
                .collect(),
            aim: robot_aim::load(),
        });
    }
    // The cars' own animations (car.rs): each character vehicle's, or vehicle.glb's
    // when it has them.
    if chars.available {
        let models: Vec<String> = chars.vehicles.iter().map(|v| v.model.clone()).collect();
        commands.insert_resource(car::animation_graphs(
            &asset_server,
            &mut graphs,
            &mut clips,
            &models,
        ));
    } else if asset_exists(VEHICLE_MODEL) {
        if car::model_is_animated(&assets_dir().join(VEHICLE_MODEL)) {
            commands.insert_resource(car::animation_graphs(
                &asset_server,
                &mut graphs,
                &mut clips,
                &[VEHICLE_MODEL.to_string()],
            ));
        } else {
            info!("{VEHICLE_MODEL} has no animations (rebuild it with tools/build_assets.py)");
        }
    }

    // The player on this machine (the first: split screen's guest has his own,
    // guest_jazz).
    let local = spawn_jazz(
        &mut commands,
        &asset_server,
        &mut meshes,
        &mut materials,
        0,
        level.start(0),
        level.start_yaw,
        false,
    );
    commands.entity(local).insert((LocalPlayer, split::Seat(0)));
}

/// A Jazz as he starts a match: in robot form, healed, with his two starting
/// weapons and full ammo.
fn new_player() -> Player {
    Player {
        form: Form::Robot,
        vertical_velocity: 0.0,
        grounded: true,
        heading: 0.0,
        twist: 0.0,
        aim_pitch: 0.0,
        pivot: None,
        drive_speed: 0.0,
        strafe_speed: 0.0,
        yaw_rate: 0.0,
        steer: 0.0,
        fire_cooldown: 0.0,
        next_vehicle_gun: 0,
        loadout: START_LOADOUT,
        equipment: [None; 2],
        support: equipment::Support::default(),
        revive: revive::Revive::default(),
        ability: abilities::ClassAbility::default(),
        hand: 0,
        weapon: START_WEAPON,
        burst_left: 0,
        burst_queued: false,
        wheel_angle: 0.0,
        move_input: Vec2::ZERO,
        sprinting: false,
        transform_left: 0.0,
        pending_form: None,
        start_clip: None,
        melee_react: hitreact::MeleeReact::default(),
        move_clip: None,
        transform_clip: ANIM_TO_VEHICLE,
        action: None,
        dash_vel: Vec3::ZERO,
        dash_left: 0.0,
        dash_total: DASH_TIME,
        dodge: false,
        air_dodge: false,
        melee_hit_in: None,
        melee_sweep: melee::Sweep::default(),
        lunge: None,
        melee_contact: false,
        air_time: 0.0,
        jump_time: None,
        air_peak: 0.0,
        ground_secs: 0.0,
        safe_spot: None,
        safe_next: None,
        dodged: false,
        health: max_health(character::Kit::JAZZ.class),
        shield: PLAYER_MAX_SHIELD,
        since_hit: 0.0,
        dying: false,
        waiting: false,
        dead_left: 0.0,
        last_hit: enemies::Hit::default(),
        shield_fx: shieldfx::ShieldFx::default(),
        start_death_clip: None,
        death_clip: None,
        limp: false,
        death_view: None,
        death_news: None,
        last_attack: (killfeed::Who::Nobody, ""),
        protection: 0.0,
        velocity: Vec3::ZERO,
        carry_vel: Vec3::ZERO,
        aiming: false,
        firing: false,
        start_weapon_clip: None,
        start_equip: None,
        equipping: None,
        unequipping: None,
        holstered: false,
        holster_change: None,
        left_handed: false,
        mirror: 0.0,
        start_hand_switch: false,
        hand_switch: None,
        weapon_clip: None,
        camera_kick: 0.0,
        recoil: None,
        ammo: full_ammo(),
        upgrades: 0,
        upgrade_pairs: 0,
        reload_left: 0.0,
        reload_slot: START_WEAPON,
        form_weapon_left: None,
        spread: gun(START_WEAPON).spread.min,
        spread_slot: START_WEAPON,
        airborne_spread: 1.0,
        muzzle: 0,
        shake: None,
        screen_struck: false,
        steam: None,
        shell_steam: Vec::new(),
        burst_end: None,
        charge_timer: 0.0,
        deadeye_left: 0.0,
        fire_damage: 0.0,
        fire_awarded: false,
        charge_level: 0,
        charge_ready: false,
        charge_aiming: false,
        charging: false,
        charged: false,
        charge_ring: 0.0,
        pickup_held: 0.0,
        pickup_swapped: false,
        speed_boost: 0.0,
        jammed_left: 0.0,
        knock_vel: Vec3::ZERO,
        rearmed: false,
        played: Vec::new(),
        hud_events: Vec::new(),
        look: character::Look::DEFAULT,
        kit: character::Kit::JAZZ,
        lock: jet::Lock::default(),
        gear: arsenal::GearState::default(),
        set_off: None,
        screen_shakes: Vec::new(),
    }
}

/// Jazz takes up a character's two weapons as he spawns (the game gives a pawn its
/// character's weapons afresh): the first in hand, both full, nothing of the guns
/// he had still playing.
fn arm(player: &mut Player, loadout: [usize; 2]) {
    player.rearmed = true;
    player.loadout = loadout;
    player.hand = 0;
    player.weapon = loadout[0];
    player.ammo = full_ammo();
    // His character's upgrades go on its weapons as they are given to him
    // (TnWeapon.GivenTo: AttachUpgradesFromInstigator), which fill them as their
    // clip and reserve upgrades go on (SetupInitialAmmo).
    let pairs = player.upgrade_pairs;
    player.upgrades = loadout
        .iter()
        .fold(0, |set, &w| set | (pairs & upgrades::of_weapon(w)));
    for w in loadout {
        player.ammo[w] = player.initial_ammo(w);
    }
    player.reload_left = 0.0;
    player.reload_slot = loadout[0];
    player.spread = player.gun(loadout[0]).spread.min;
    player.spread_slot = loadout[0];
    player.burst_left = 0;
    player.burst_queued = false;
    player.start_weapon_clip = None;
    player.weapon_clip = None;
    player.start_equip = None;
    player.equipping = None;
    player.unequipping = None;
    player.holstered = false;
    player.holster_change = None;
    player.steam = None;
}

/// The character chosen as the match starts (choose.rs): Jazz, who hasn't played
/// yet, takes up its weapons there and then, and puts on its look.
fn arm_chosen(
    mut chosen: split::BySeat<choose::Chosen>,
    current_map: Res<CurrentMap>,
    mut players: Query<(&mut Player, Option<&split::Seat>), With<LocalPlayer>>,
) {
    // (Each of this PC's players his own: split screen's guest chooses after the
    // first player.)
    let arm_now = chosen.each().map(|c| std::mem::take(&mut c.arm_now));
    for (mut player, seat) in &mut players {
        let seat = split::seat_of(seat);
        let chosen = chosen.seat(seat);
        if !arm_now[seat] {
            continue;
        }
        let Some(loadout) = chosen_loadout(chosen) else {
            continue;
        };
        player.upgrade_pairs = chosen_pairs(chosen);
        arm(&mut player, loadout);
        if let Some(look) = chosen_look(chosen, current_map.0) {
            player.look = look;
        }
    }
}

/// The look of the character chosen (choose.rs) in its faction's colours: the
/// build's Decepticon swatch and glow on a Decepticon map, its Autobot ones
/// elsewhere.
fn chosen_look(chosen: &choose::Chosen, map: Option<usize>) -> Option<character::Look> {
    let looks = chosen.looks?;
    Some(looks[usize::from(decepticon_map(map))])
}

/// Whether Escalation on the map is the Decepticons' (MapInfo::decepticons).
fn decepticon_map(map: Option<usize>) -> bool {
    map.and_then(|m| MAPS.get(m)).is_some_and(|m| m.decepticons)
}

/// What a Jazz's models have on: the look they were last given.
#[derive(Component)]
struct LookApplied(character::Look);

/// Each Jazz's models wear his look (character.rs dresses them: the parts on the
/// robot, the chest's vehicle in its paint), and he takes up what the look brings
/// (his class's robot at its size, its clips and the transformation's timing:
/// Player::kit).
#[allow(clippy::type_complexity)]
fn follow_looks(
    mut commands: Commands,
    chars: Res<character::Characters>,
    mut players: Query<(Entity, &mut Player, Option<&LookApplied>)>,
    mut forms: Query<
        (
            Entity,
            &JazzOf,
            Has<RobotForm>,
            Has<ClassSized>,
            &mut BaseScale,
            &mut Transform,
        ),
        Or<(With<RobotForm>, With<VehicleForm>)>,
    >,
) {
    for (owner, mut player, applied) in &mut players {
        if applied.is_some_and(|a| a.0 == player.look) {
            continue;
        }
        let look = player.look;
        let was = player.max_health();
        player.kit = chars.kit(&look);
        // (A new class's health: as full as he was of the old one's.)
        let full = player.max_health();
        if full != was && player.health > 0.0 {
            player.health = (player.health / was * full).min(full);
        }
        let scale = player.kit.robot.scale;
        for (entity, of, is_robot, sized, mut base, mut transform) in &mut forms {
            if of.0 != owner {
                continue;
            }
            if is_robot {
                // His class's size (not the placeholder's, without robot.glb).
                if sized && base.0 != scale {
                    base.0 = scale;
                    transform.scale = Vec3::splat(scale);
                }
                if chars.available {
                    commands.entity(entity).insert(character::Wear(look));
                }
            } else if chars.available {
                commands.entity(entity).insert(character::CarWear(look));
            }
        }
        commands.entity(owner).insert(LookApplied(look));
    }
}

/// A new Jazz in robot form at `at` for player `slot`, with his robot and car
/// models (the car hidden); returns him. Every system that moves or fights works
/// on each Jazz from his PlayerInput. `waiting`: not in the match yet (Player::
/// waiting), as another player's Jazz is until his PC says otherwise.
#[allow(clippy::too_many_arguments)]
fn spawn_jazz(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    slot: usize,
    at: Vec3,
    facing: f32,
    waiting: bool,
) -> Entity {
    let model_rotation = Quat::from_rotation_y(MODEL_YAW_OFFSET);
    let player = commands
        .spawn((
            Player {
                heading: facing,
                waiting,
                ..new_player()
            },
            PlayerSlot(slot),
            PlayerInput {
                view_dir: Vec3::NEG_Z,
                ..default()
            },
            RobotAnimState::default(),
            SoundTrack::default(),
            car::Car::default(),
            Transform::from_translation(at).with_rotation(Quat::from_rotation_y(facing)),
            Visibility::default(),
        ))
        .id();

    // Robot form.
    let robot = if asset_exists(ROBOT_MODEL) {
        commands
            .spawn((
                // (Its scene from the file its clips come from: gltffiles.rs.)
                gltffiles::SceneOf::load(asset_server, ROBOT_MODEL),
                Transform::from_scale(Vec3::splat(ROBOT_SCALE)).with_rotation(model_rotation),
                BaseScale(ROBOT_SCALE),
                ClassSized,
            ))
            .id()
    } else {
        let e = placeholder_robot(commands, meshes, materials);
        commands
            .entity(e)
            .insert((BaseScale(1.5), Transform::from_scale(Vec3::splat(1.5))));
        e
    };
    commands
        .entity(robot)
        .insert((RobotForm, JazzOf(player), Visibility::Inherited));

    // Vehicle form, hidden at the start (the characters' vehicles: the one his
    // look makes, which character.rs puts in, follow_looks).
    let vehicle = if character::available() {
        commands
            .spawn((
                Transform::from_scale(Vec3::splat(VEHICLE_SCALE)).with_rotation(model_rotation),
                BaseScale(VEHICLE_SCALE),
            ))
            .id()
    } else if asset_exists(VEHICLE_MODEL) {
        commands
            .spawn((
                gltf_scene(asset_server, VEHICLE_MODEL),
                Transform::from_scale(Vec3::splat(VEHICLE_SCALE)).with_rotation(model_rotation),
                BaseScale(VEHICLE_SCALE),
            ))
            .id()
    } else {
        let e = placeholder_vehicle(commands, meshes, materials);
        commands
            .entity(e)
            .insert((BaseScale(0.85), Transform::from_scale(Vec3::splat(0.85))));
        e
    };
    commands
        .entity(vehicle)
        .insert((VehicleForm, JazzOf(player), Visibility::Hidden));

    commands.entity(player).add_children(&[robot, vehicle]);
    player
}

/// Marks the parts of each Jazz's models that his systems look for (see JazzOf)
/// with the Jazz they belong to, as they appear (the models' insides come in as
/// their scenes load).
#[allow(clippy::type_complexity)]
fn tag_jazz_parts(
    mut commands: Commands,
    parts: Query<
        Entity,
        (
            Or<(
                With<RobotAnimator>,
                With<car::CarAnimator>,
                With<MuzzleBone>,
                With<GunBone>,
                With<arsenal::CloudBone>,
                With<WeaponMesh>,
                With<WeaponMeshCloudB>,
                With<Forearm>,
                With<ForearmCloudB>,
                With<SwitchBone>,
                With<car::CarTyre>,
                With<car::CarGunMesh>,
                With<car::CannonBone>,
                With<car::FxBone>,
                Or<(With<character::UpgradePart>, With<melee::Blade>)>,
            )>,
            Without<JazzOf>,
        ),
    >,
    parents: Query<&ChildOf>,
    players: Query<(), With<Player>>,
) {
    for entity in &parts {
        let owner = parents
            .iter_ancestors(entity)
            .find(|e| players.contains(*e))
            .unwrap_or(Entity::PLACEHOLDER);
        commands.entity(entity).insert(JazzOf(owner));
    }
}

/// A bone of a Jazz's models (a joint of their skinned meshes) and the Jazz it is
/// his: what a sticky round stuck in him rides (riot.rs: the game's
/// FindClosestBone of his mesh, TnProjectileGrenadeLauncher.AttachToActor).
#[derive(Component, Clone, Copy)]
pub(crate) struct JointOf(pub Entity);

/// Marks each Jazz's bones (the joints of his models' skinned meshes) with whose
/// they are, as his models' scenes spawn them. (Not his Ion Blades': melee.rs.)
#[allow(clippy::type_complexity)]
fn tag_jazz_joints(
    mut commands: Commands,
    meshes: Query<
        (Entity, &bevy::mesh::skinning::SkinnedMesh),
        (
            Added<bevy::mesh::skinning::SkinnedMesh>,
            Without<melee::BladePart>,
        ),
    >,
    parents: Query<&ChildOf>,
    players: Query<(), With<Player>>,
) {
    for (entity, skinned) in &meshes {
        let Some(owner) = parents
            .iter_ancestors(entity)
            .find(|e| players.contains(*e))
        else {
            continue;
        };
        for &joint in &skinned.joints {
            commands.entity(joint).try_insert(JointOf(owner));
        }
    }
}

/// A weapon lying on the ground, with the ammo it holds (clip, reserve) and the
/// upgrades on it (a dropped weapon keeps its owner's: TnWeapon.DropFrom).
#[derive(Component)]
struct WeaponPickup {
    weapon: usize,
    ammo: (u32, u32),
    upgrades: upgrades::Installed,
    /// The floor under it.
    spot: Vec3,
    /// Seconds before it can be picked up (a weapon just dropped).
    delay: f32,
}

/// The name of what `action` is on in `mode` for the HUD's prompts: its key in the
/// keyboard layout, or its button on the controller while that plays (pad.rs).
fn control_name(controls: &Controls, pad: &pad::Pad, mode: Mode, action: Action) -> String {
    match pad.control_name(mode, action).filter(|_| pad.active) {
        Some(name) => name.to_string(),
        None => controls.key_name(mode, action),
    }
}

/// Standing at a weapon on the ground and holding E takes it in place of the
/// weapon in hand (shop::slot_for), which is gone (as in the game's
/// Escalation: nothing is left behind); at a health station, it buys health (see
/// HEALTH_COST), at an ammo or heavy ammo station ammo (see fill_ammo), at a weapon
/// station its weapon and at an equipment station its equipment (shop.rs). Of the
/// things in reach, the nearest is offered (TnPickupManager.GetClosestPickup). The
/// HUD shows "Hold E to pickup <weapon>" or "Hold E to purchase <it> for *<price>"
/// meanwhile (to the local player; E, or the key Interact/Pick Up is bound to).
#[allow(clippy::too_many_arguments)]
fn weapon_pickups(
    mut commands: Commands,
    time: Res<Time>,
    mut players: Query<
        (
            &Transform,
            &mut Player,
            &PlayerInput,
            Has<LocalPlayer>,
            Option<&split::Seat>,
        ),
        Without<RemotePlayer>,
    >,
    mut pickups: Query<(Entity, &mut WeaponPickup)>,
    mut stations: Query<(Entity, &mut HealthStation)>,
    mut ammo_stations: Query<(Entity, &mut AmmoStation)>,
    (mut weapon_stations, mut equipment_stations): (
        Query<(Entity, &mut shop::WeaponStation)>,
        Query<(Entity, &mut shop::EquipmentStation)>,
    ),
    mut scores: split::BySeat<Score>,
    mut sfx: ResMut<Sfx>,
    mut huds: split::BySeat<HudState>,
    (controls, pads): (Res<Controls>, pad::Pads),
    mut voice: ResMut<voice::VoiceLines>,
    mut pod_loot: Query<(Entity, &mut recreator::PodLoot)>,
) {
    #[derive(Clone, Copy)]
    enum Target {
        Weapon(Entity),
        /// What an Armory Recreator made (recreator.rs), and its LOOT number.
        Loot(Entity, usize),
        /// A health station, and its price.
        Station(Entity, u32),
        /// An ammo station, its price, and whether it is a heavy ammo one.
        Ammo(Entity, u32, bool),
        /// A weapon station, and its weapon.
        Shop(Entity, usize),
        /// An equipment station, its equipment and its price.
        Gear(Entity, shop::Equipment, u32),
    }
    let dt = time.delta_secs();
    for (_, mut pickup) in &mut pickups {
        pickup.delay = (pickup.delay - dt).max(0.0);
    }
    for (_, mut loot) in &mut pod_loot {
        loot.delay = (loot.delay - dt).max(0.0);
    }
    // (A weapon taken this frame can't be taken again by the next player.)
    let mut taken = Vec::new();
    for (player_transform, mut jazz, input, local, seat) in &mut players {
        // (Each of this PC's players with his own HUD, energon and controller.)
        let seat = split::seat_of(seat);
        let (hud, score, pad) = (huds.seat(seat), scores.seat(seat), pads.seat(seat));
        if !input.pick_up {
            jazz.pickup_swapped = false;
        }
        if local {
            // The key Interact/Pick Up is on (the controller's button while it
            // plays), every frame: the other lines on it take it from here too (a
            // console's, revive.rs's, an Armory Recreator's), offered where there
            // is nothing to pick up or buy. (Kept up only while there was, a
            // controller's player saw a keyboard's E at a door till he had stood
            // at a weapon.)
            let key = control_name(&controls, &pad, Mode::Robot, Action::Interact);
            if hud.pickup_key != key {
                hud.pickup_key = key;
            }
            hud.pickup_key_mouse = !pad.active
                && controls
                    .bind(Mode::Robot, Action::Interact)
                    .is_some_and(|b| !matches!(b, Bind::Key(_)));
            // The keys the equipment is on (the purchase line's and the equipment
            // slots'; the controller's buttons while it plays).
            for (i, action) in [Action::AssaultTech, Action::UtilityTech]
                .into_iter()
                .enumerate()
            {
                let key = control_name(&controls, &pad, Mode::Robot, action);
                let mouse = !pad.active
                    && controls
                        .bind(Mode::Robot, action)
                        .is_some_and(|b| !matches!(b, Bind::Key(_)));
                if hud.equipment_keys[i].0 != key || hud.equipment_keys[i].1 != mouse {
                    hud.equipment_keys[i] = (key, mouse);
                }
            }
            // What he has bought, in the equipment slots: red once used, while its
            // effect lasts.
            let spent = jazz.support.spent;
            let mut shown: [Option<hud::HudEquipment>; 2] = [None, None];
            for (slot, item) in jazz.equipment.iter().enumerate() {
                shown[slot] = item.map(|kind| hud::HudEquipment {
                    icon: kind.hud_icon(),
                    ready: !spent[slot],
                });
            }
            if hud.equipment != shown {
                hud.equipment = shown;
            }
        }
        let at = player_transform.translation;
        let mut nearest: Option<(f32, Entity)> = None;
        for (entity, pickup) in &pickups {
            let flat = Vec2::new(pickup.spot.x - at.x, pickup.spot.z - at.z).length();
            let rise = pickup.spot.y - at.y;
            if flat < PICKUP_REACH
                && (-2.0..2.0).contains(&rise)
                && pickup.delay <= 0.0
                && !taken.contains(&entity)
                && !jazz.loadout.contains(&pickup.weapon)
                && nearest.is_none_or(|(d, _)| flat < d)
            {
                nearest = Some((flat, entity));
            }
        }
        // A station whose cylinder he touches (its factory's), with what it sells
        // there to buy: health when he has some to make up (TnHealthPickup.
        // PickupAllowed), its ammo crate when a gun it serves is short of ammo
        // (ammo_short), a weapon he hasn't got or has short of ammo
        // (HmWeapon.DenyPickupQuery), or equipment.
        let (robot_radius, robot_height) = (jazz.kit.robot.radius, jazz.kit.robot.height);
        let touching = |spot: Vec3| {
            let flat = Vec2::new(spot.x - at.x, spot.z - at.z).length();
            let rise = spot.y - at.y;
            (flat < STATION_TOUCH_RADIUS + robot_radius
                && (-STATION_TOUCH_HEIGHT..robot_height).contains(&rise))
            .then_some(flat)
        };
        let hurt = jazz.health < jazz.max_health() || jazz.shield < PLAYER_MAX_SHIELD;
        let mut station: Option<(f32, Target)> = None;
        let mut offer = |flat: Option<f32>, target: Target| {
            if let Some(flat) = flat
                && station.is_none_or(|(d, _)| flat < d)
            {
                station = Some((flat, target));
            }
        };
        for (entity, st) in &stations {
            offer(
                touching(st.spot).filter(|_| hurt && st.away <= 0.0),
                Target::Station(entity, st.cost),
            );
        }
        let short = [ammo_short(&jazz, false), ammo_short(&jazz, true)];
        for (entity, st) in &ammo_stations {
            offer(
                touching(st.spot).filter(|_| short[st.heavy as usize] && st.away <= 0.0),
                Target::Ammo(entity, st.cost, st.heavy),
            );
        }
        for (entity, st) in &weapon_stations {
            let wanted = !jazz.loadout.contains(&st.weapon) || !ammo_maxed(&jazz, st.weapon);
            offer(
                touching(st.spot).filter(|_| wanted && st.away <= 0.0),
                Target::Shop(entity, st.weapon),
            );
        }
        for (entity, st) in &equipment_stations {
            offer(
                touching(st.spot).filter(|_| st.away <= 0.0),
                Target::Gear(entity, st.kind, st.cost),
            );
        }
        // What an Armory Recreator made, taken from where its buyer stood once it
        // can be (its pickup cylinder twice a dropped weapon's: recreator.rs); a
        // weapon only if he hasn't got it.
        for (entity, loot) in &pod_loot {
            let flat = Vec2::new(loot.spot.x - at.x, loot.spot.z - at.z).length();
            let rise = loot.spot.y - at.y;
            let size = recreator::LOOT_SIZE;
            let carried = match recreator::LOOT.get(loot.item).map(|l| l.0) {
                Some(recreator::Loot::Weapon(id)) => {
                    weapon_index(id).is_none_or(|w| jazz.loadout.contains(&w))
                }
                Some(recreator::Loot::Gear(_)) => false,
                None => true,
            };
            offer(
                (flat < PICKUP_REACH * size
                    && (-2.0 * size..2.0 * size).contains(&rise)
                    && loot.delay <= 0.0
                    && !carried
                    && !taken.contains(&entity))
                .then_some(flat),
                Target::Loot(entity, loot.item),
            );
        }
        // (Not down, getting up or reviving, nor with a downed teammate to revive
        // on the same key: revive.rs.)
        let able = jazz.form == Form::Robot
            && jazz.hand_switch.is_none()
            && jazz.dead_left <= 0.0
            && !jazz.dying
            && jazz.transform_left <= 0.0
            && jazz.pending_form.is_none()
            && !jazz.revive.locked()
            && jazz.revive.hold_target.is_none();
        let target = match (nearest, station) {
            (Some((dw, w)), Some((ds, s))) => Some(if ds < dw { s } else { Target::Weapon(w) }),
            (Some((_, w)), None) => Some(Target::Weapon(w)),
            (None, Some((_, s))) => Some(s),
            (None, None) => None,
        }
        .filter(|_| able);
        let Some(target) = target else {
            if local {
                hud.prompt = None;
            }
            jazz.pickup_held = 0.0;
            continue;
        };
        if local {
            hud.prompt = Some(match target {
                Target::Weapon(entity) => match pickups.get(entity) {
                    Ok((_, pickup)) => Prompt::Pickup(WEAPONS[pickup.weapon].hud_label),
                    Err(_) => continue,
                },
                Target::Loot(_, item) => match recreator::LOOT.get(item).map(|l| l.0) {
                    Some(recreator::Loot::Weapon(id)) => match weapon_index(id) {
                        Some(w) => Prompt::Pickup(WEAPONS[w].hud_label),
                        None => continue,
                    },
                    Some(recreator::Loot::Gear(kind)) => Prompt::Pickup(kind.friendly_name()),
                    None => continue,
                },
                Target::Station(_, cost) => Prompt::Purchase(HEALTH_ITEM_NAME, cost),
                Target::Ammo(_, cost, heavy) => Prompt::Purchase(
                    if heavy {
                        HEAVY_AMMO_ITEM_NAME
                    } else {
                        AMMO_ITEM_NAME
                    },
                    cost,
                ),
                Target::Shop(_, weapon) => {
                    Prompt::Purchase(shop::purchase_name(weapon), shop::price(weapon))
                }
                Target::Gear(_, kind, cost) => Prompt::Purchase(kind.name(), cost),
            });
        }
        jazz.pickup_held = if input.pick_up && !jazz.pickup_swapped {
            jazz.pickup_held + dt
        } else {
            0.0
        };
        if jazz.pickup_held < PICKUP_HOLD_SECS {
            continue;
        }
        jazz.pickup_held = 0.0;
        jazz.pickup_swapped = true;
        // What it costs; without enough energon, the error beep and nothing bought.
        let cost = match target {
            Target::Weapon(_) | Target::Loot(..) => 0,
            Target::Station(_, cost) | Target::Ammo(_, cost, _) | Target::Gear(_, _, cost) => cost,
            Target::Shop(_, weapon) => shop::price(weapon),
        };
        if score.energon < cost {
            sfx.play_from(SND_NO_FUNDS, voice_at(local, at));
            continue;
        }
        score.energon -= cost;
        if local {
            hud.prompt = None;
        }
        let entity = match target {
            Target::Weapon(entity) => entity,
            Target::Loot(entity, item) => {
                let pod = pod_loot.get(entity).map(|(_, l)| l.pod);
                match recreator::LOOT.get(item).map(|l| l.0) {
                    Some(recreator::Loot::Weapon(id)) => {
                        let Some(weapon) = weapon_index(id) else {
                            continue;
                        };
                        // (As a weapon station's: its ammo as a player's would be, no
                        // upgrades of its own.)
                        let ammo = jazz.initial_ammo(weapon);
                        let slot = shop::slot_for(&jazz.loadout, jazz.hand, weapon);
                        give_weapon(&mut jazz, slot, weapon, ammo, 0);
                        sfx.play_from(SND_PICKUP, voice_at(local, at));
                        if local {
                            voice.say_for(seat, voice::Line::WeaponPickup);
                        }
                    }
                    Some(recreator::Loot::Gear(kind)) => {
                        // (As a support item bought, Target::Gear.)
                        jazz.equipment[kind.slot()] = Some(kind);
                        jazz.support.bought.push(kind);
                        sfx.play_from(kind.pickup_sound(), voice_at(local, at));
                        if local {
                            hud.events.push(HudEvent::ItemUnlocked {
                                image: kind.wireframe(),
                                name: kind.friendly_name(),
                            });
                        }
                    }
                    None => continue,
                }
                // (Gone from the others too.)
                if let Ok(pod) = pod {
                    jazz.played.push(net::JazzEvent::PodLoot { pod: pod as u8 });
                }
                taken.push(entity);
                commands.entity(entity).despawn();
                continue;
            }
            Target::Station(entity, _) => {
                if let Ok((_, mut st)) = stations.get_mut(entity) {
                    st.away = st.respawn;
                }
                jazz.health = jazz.max_health();
                jazz.shield = PLAYER_MAX_SHIELD;
                sfx.play_from(SND_HEALTH, voice_at(local, at));
                if local {
                    voice.say_for(seat, voice::Line::HealthPickup);
                }
                continue;
            }
            Target::Ammo(entity, _, heavy) => {
                if let Ok((_, mut st)) = ammo_stations.get_mut(entity) {
                    st.away = st.respawn;
                }
                let hud_events = local.then_some(&mut *hud);
                fill_ammo(&mut jazz, heavy, &mut sfx, hud_events, voice_at(local, at));
                sfx.play_from(
                    if heavy { SND_HEAVY_AMMO } else { SND_AMMO },
                    voice_at(local, at),
                );
                if local {
                    voice.say_for(seat, voice::Line::AmmoPickup);
                }
                continue;
            }
            Target::Shop(entity, weapon) => {
                if let Ok((_, mut st)) = weapon_stations.get_mut(entity) {
                    st.away = st.respawn;
                }
                if jazz.loadout.contains(&weapon) {
                    // One he has: its ammo filled.
                    let hud_events = local.then_some(&mut *hud);
                    top_up(&mut jazz, weapon, hud_events);
                } else {
                    // A weapon from the factory comes with its ammo as a player's
                    // would (TnWeapon.PostInstantiateFromPickupFactory) and no
                    // upgrades of its own.
                    let ammo = jazz.initial_ammo(weapon);
                    let slot = shop::slot_for(&jazz.loadout, jazz.hand, weapon);
                    give_weapon(&mut jazz, slot, weapon, ammo, 0);
                }
                sfx.play_from(SND_PICKUP, voice_at(local, at));
                if local {
                    voice.say_for(seat, voice::Line::WeaponPickup);
                }
                continue;
            }
            Target::Gear(entity, kind, _) => {
                if let Ok((_, mut st)) = equipment_stations.get_mut(entity) {
                    st.away = st.respawn;
                }
                // In its slot, in place of what was there (TnEquipmentPickup.
                // ClientGivenTo: ReplaceAbility; what it had going ends, equipment.rs),
                // shown bought at the top right (NotifyStoreItemUnlocked).
                jazz.equipment[kind.slot()] = Some(kind);
                jazz.support.bought.push(kind);
                sfx.play_from(kind.pickup_sound(), voice_at(local, at));
                if local {
                    hud.events.push(HudEvent::ItemUnlocked {
                        image: kind.wireframe(),
                        name: kind.friendly_name(),
                    });
                }
                continue;
            }
        };
        let Ok((_, pickup)) = pickups.get(entity) else {
            continue;
        };
        let (weapon, ammo) = (pickup.weapon, pickup.ammo);
        let carried_upgrades = pickup.upgrades & upgrades::of_weapon(weapon);
        if local {
            voice.say_for(seat, voice::Line::WeaponPickup);
        }
        let slot = shop::slot_for(&jazz.loadout, jazz.hand, weapon);
        give_weapon(&mut jazz, slot, weapon, ammo, carried_upgrades);
        sfx.play_from(SND_PICKUP, voice_at(local, at));
        taken.push(entity);
        commands.entity(entity).despawn();
    }
}

/// A weapon taken or bought (`ammo` its clip and reserve, `carried_upgrades` the
/// upgrades on it) into `slot` of the two Jazz carries, in place of the weapon there,
/// which is gone; it goes into his hand: unfolding out of the arm in place of the one
/// there, or, in the other slot, as he switches to it.
fn give_weapon(
    jazz: &mut Player,
    slot: usize,
    weapon: usize,
    ammo: (u32, u32),
    carried_upgrades: upgrades::Installed,
) {
    let slot = slot.min(1);
    let replaced = jazz.loadout[slot];
    let in_hand = jazz.weapon;
    // (A grenade launcher let go sets off its rounds out: TnWeaponGrenade-
    // LauncherBase.DropFrom's ServerDetonateGrenades.)
    if WEAPONS[replaced].gear.is_some_and(|g| g.detonate.is_some()) {
        jazz.set_off = WEAPONS[replaced].projectile;
        jazz.played.push(net::JazzEvent::SetOff {
            weapon: replaced as u8,
        });
    }
    let switching = slot != jazz.hand.min(1);
    jazz.loadout[slot] = weapon;
    jazz.hand = slot;
    jazz.weapon = weapon;
    jazz.ammo[weapon] = ammo;
    // It keeps the upgrades it came with and gains his character's for it
    // (GivenTo: AttachUpgradesFromInstigator); a clip or reserve upgrade new to
    // it fills it as it goes on (TnBuffLargerClip, TnBuffReserveAmmo:
    // SetupInitialAmmo).
    let gained = jazz.upgrade_pairs & upgrades::of_weapon(weapon) & !carried_upgrades;
    jazz.upgrades = (jazz.upgrades & !upgrades::of_weapon(replaced) & !upgrades::of_weapon(weapon))
        | carried_upgrades
        | gained;
    let fills = upgrades::on_weapon(gained, weapon).any(|u| {
        matches!(
            u.effect,
            upgrades::Effect::LargerClip(_) | upgrades::Effect::ReserveAmmo(_)
        )
    });
    if fills {
        jazz.ammo[weapon] = jazz.initial_ammo(weapon);
    }
    jazz.reload_left = 0.0;
    jazz.start_weapon_clip = None;
    // (The old gun's one-off clip, a reload say, is cut off by the switch:
    // animate_robot.)
    jazz.burst_left = 0;
    jazz.burst_queued = false;
    jazz.fire_cooldown = jazz.fire_cooldown.max(WEAPONS[weapon].equip_secs);
    // The new weapon unfolds out of the arm; the one replaced in hand is dropped as
    // it is, the one in hand from the other slot folds away.
    jazz.start_equip = Some((weapon, switching.then_some(in_hand)));
}

/// A health station (see HEALTH_COST).
#[derive(Component)]
struct HealthStation {
    /// The floor at its centre.
    spot: Vec3,
    /// Its price and how long its health takes to come back once bought (the
    /// factory's Cost and RespawnTime: 20 s on Downfall and Ancients, 40 s on
    /// Oblivion and Ignition).
    cost: u32,
    respawn: f32,
    /// Seconds until its health is back after a purchase (0: there to buy).
    away: f32,
}

/// A health station's energon cube, shown while its health can be bought.
#[derive(Component)]
struct HealthCube;

/// The map's health stations: each a base with the cube hovering over it, lit by
/// the cube's glow. (A plain disc stands in for the base, and a block for the
/// cube, when their models haven't been built.)
fn spawn_health_stations(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    cube_look: &energon::CubeLook,
    stations: &[ShopSpot],
) -> Vec<Entity> {
    let mut spawned = Vec::new();
    if stations.is_empty() {
        return spawned;
    }
    let base_model = asset_exists(HEALTH_BASE_MODEL);
    let base_mesh = meshes.add(Cylinder::new(1.55, 0.3));
    let base_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.45, 0.47, 0.5),
        metallic: 0.6,
        perceptual_roughness: 0.5,
        ..default()
    });
    for s in stations {
        let mut station = commands.spawn((
            HealthStation {
                spot: s.spot,
                cost: s.cost,
                respawn: s.respawn,
                away: 0.0,
            },
            Transform::from_translation(s.spot).with_rotation(Quat::from_rotation_y(s.yaw)),
            Visibility::default(),
        ));
        if base_model {
            station.insert(gltf_scene(asset_server, HEALTH_BASE_MODEL));
        } else {
            station.with_child((
                Mesh3d(base_mesh.clone()),
                MeshMaterial3d(base_material.clone()),
                Transform::from_xyz(0.0, 0.15, 0.0),
            ));
        }
        let light = (
            PointLight {
                color: energon::glow_color(),
                intensity: 67_500.0,
                range: 8.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::default(),
        );
        let cube = (
            HealthCube,
            Transform::from_xyz(0.0, HEALTH_CUBE_LIFT, 0.0)
                .with_scale(Vec3::splat(HEALTH_CUBE_SCALE)),
            Visibility::default(),
        );
        station.with_children(|parent| {
            cube_look.attach(&mut parent.spawn((cube, children![light])));
        });
        spawned.push(station.id());
    }
    spawned
}

/// The health stations' cubes turn and tumble while there, and come back
/// HEALTH_RESPAWN_SECS after a purchase.
fn health_stations(
    time: Res<Time>,
    clock: Res<MatchClock>,
    mut stations: Query<&mut HealthStation>,
    mut cubes: Query<(&ChildOf, &mut Transform, &mut Visibility), With<HealthCube>>,
) {
    let dt = time.delta_secs();
    for mut station in &mut stations {
        if station.away > 0.0 {
            station.away = (station.away - dt).max(0.0);
        }
    }
    let t = time.elapsed_secs() - clock.0;
    let turn = Quat::from_rotation_y(t * HEALTH_CUBE_SPIN)
        * Quat::from_rotation_x(t * HEALTH_CUBE_TUMBLE)
        * Quat::from_rotation_z(-t * HEALTH_CUBE_TUMBLE);
    for (child_of, mut transform, mut visibility) in &mut cubes {
        let there = stations
            .get(child_of.parent())
            .is_ok_and(|station| station.away <= 0.0);
        let want = if there {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != want {
            *visibility = want;
        }
        if there {
            transform.rotation = turn;
        }
    }
}

/// An ammo station (see AMMO_CRATE_MODEL), or a heavy ammo station (shop.rs).
#[derive(Component)]
struct AmmoStation {
    /// The floor at its centre.
    spot: Vec3,
    /// Its crate is the heavy one (TnAmmoCratePickupHeavy), for the heavy weapons.
    heavy: bool,
    cost: u32,
    /// Seconds its crate takes to come back after a purchase, and seconds until it
    /// does (0: there to buy).
    respawn: f32,
    away: f32,
}

/// An ammo station's crate, shown while its ammo can be bought.
#[derive(Component)]
struct AmmoCrate;

/// The map's ammo stations (or its heavy ammo stations): each the ammo crate (the
/// heavy crate) hovering over the base the map has there. (A glowing block stands in
/// for the crate when its model hasn't been built.)
fn spawn_ammo_stations(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    stations: &[ShopSpot],
    heavy: bool,
) -> Vec<Entity> {
    let mut spawned = Vec::new();
    if stations.is_empty() {
        return spawned;
    }
    let crate_model = if heavy {
        HEAVY_AMMO_CRATE_MODEL
    } else {
        AMMO_CRATE_MODEL
    };
    let model = asset_exists(crate_model);
    let block_mesh = meshes.add(Cuboid::new(0.46, 1.4, 1.27));
    let block_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.3, 0.32, 0.34),
        emissive: if heavy {
            LinearRgba::rgb(1.0, 0.396, 0.0)
        } else {
            LinearRgba::rgb(0.0, 1.0, 0.94)
        },
        ..default()
    });
    for station in stations {
        let mut entity = commands.spawn((
            AmmoStation {
                spot: station.spot,
                heavy,
                cost: station.cost,
                respawn: station.respawn,
                away: 0.0,
            },
            Transform::from_translation(station.spot)
                .with_rotation(Quat::from_rotation_y(station.yaw)),
            Visibility::default(),
        ));
        entity.with_children(|parent| {
            let mut crate_mesh = parent.spawn((
                AmmoCrate,
                Transform::from_xyz(0.0, AMMO_CRATE_LIFT, 0.0),
                Visibility::default(),
            ));
            if model {
                crate_mesh.insert(gltf_scene(asset_server, crate_model));
            } else {
                crate_mesh.with_child((
                    Mesh3d(block_mesh.clone()),
                    MeshMaterial3d(block_material.clone()),
                    Transform::from_xyz(0.0, 0.7, 0.0),
                ));
            }
        });
        spawned.push(entity.id());
    }
    spawned
}

/// The ammo stations' crates turn while there, and come back the station's
/// respawn time after a purchase.
fn ammo_stations(
    time: Res<Time>,
    clock: Res<MatchClock>,
    mut stations: Query<&mut AmmoStation>,
    mut crates: Query<(&ChildOf, &mut Transform, &mut Visibility), With<AmmoCrate>>,
) {
    let dt = time.delta_secs();
    for mut station in &mut stations {
        if station.away > 0.0 {
            station.away = (station.away - dt).max(0.0);
        }
    }
    // (The game's positive yaw turns the other way round in Bevy's axes.)
    let turn = Quat::from_rotation_y(-(time.elapsed_secs() - clock.0) * AMMO_CRATE_SPIN);
    for (child_of, mut transform, mut visibility) in &mut crates {
        let there = stations
            .get(child_of.parent())
            .is_ok_and(|station| station.away <= 0.0);
        let want = if there {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != want {
            *visibility = want;
        }
        if there {
            transform.rotation = turn;
        }
    }
}

/// The guns an ammo crate serves of those Jazz carries (its ValidWeaponTypes): the
/// ammo crate his weapons of WT_Primary and his vehicle's guns (WT_AltForm,
/// WT_AltFormSecondary), the heavy ammo crate (`heavy`) his heavy weapons
/// (WT_Secondary: shop::is_heavy).
fn served_slots(jazz: &Player, heavy: bool) -> Vec<usize> {
    let mut slots: Vec<usize> = jazz
        .loadout
        .into_iter()
        .filter(|&w| shop::is_heavy(w) == heavy)
        .collect();
    slots.dedup();
    if !heavy {
        slots.push(vehicle_slot(jazz));
    }
    slots
}

/// A gun's ammo is full (HmWeapon.AmmoMaxed): its reserve, or for a gun with no
/// reserve its clip.
fn ammo_maxed(jazz: &Player, slot: usize) -> bool {
    let g = jazz.gun(slot);
    match g.reserve_max {
        0 => jazz.ammo[slot].0 >= g.clip_size,
        max => jazz.ammo[slot].1 >= max,
    }
}

/// Whether an ammo crate (the heavy one: `heavy`) has anything for Jazz
/// (TnAmmoCratePickup.PickupAllowed): a gun he carries that it serves short of a
/// full reserve.
fn ammo_short(jazz: &Player, heavy: bool) -> bool {
    served_slots(jazz, heavy)
        .into_iter()
        .any(|slot| !ammo_maxed(jazz, slot))
}

/// A weapon he has bought again: its clip and reserve filled, with the HUD's note.
fn top_up(jazz: &mut Player, weapon: usize, hud: Option<&mut HudState>) {
    let g = jazz.gun(weapon);
    let (clip, reserve) = jazz.ammo[weapon];
    let added = g.clip_size.saturating_sub(clip) + g.reserve_max.saturating_sub(reserve);
    jazz.ammo[weapon] = (clip.max(g.clip_size), reserve.max(g.reserve_max));
    if let (Some(hud), true) = (hud, added > 0) {
        hud.events.push(HudEvent::AmmoAdded {
            icon: Some(ammo_icon(weapon)).filter(|i| !i.is_empty()),
            amount: added,
            current: weapon == gun_slot(jazz),
        });
    }
}

/// An ammo crate's ammo for Jazz (TnAmmoCratePickup.GivenTo): every gun he carries
/// that it serves gets its reserve filled (FillReserveAmmo: AddAmmo(-1)), the clip
/// as it was; a gun whose clip is empty then reloads, the one in hand with its
/// reload, the others at once (HmWeapon.CheckForReload: Reload or InstantReload).
/// The small icon the HUD lists ammo picked up for a gun with, over the ammo count
/// (the movie's updateAmmoAdded: its getLinkageId turns the weapon's class,
/// TnWeapon<X>, into the kill feed's GameMessageIcon<X>: the Orbital Pistol's
/// TnWeaponTracerDart into the System Corruptor's, the Thermo Rocket Cannon's
/// TnWeaponRocketLauncher into its own; the vehicles' weapons into their vehicles').
fn ammo_icon(slot: usize) -> &'static str {
    match slot {
        TANK_SLOT => "GameMessageIconTank",
        JET_SLOT => "GameMessageIconJet",
        TRUCK_SLOT => "GameMessageIconTruck",
        VEHICLE_SLOT => "GameMessageIconCar",
        _ => match WEAPONS.get(slot).map(|w| w.id) {
            Some("TracerDart") => "GameMessageIconSysCorruptor",
            Some("HomingRocket") => "GameMessageIconRocketLauncher",
            Some("AssaultRifle") => "GameMessageIconAssaultRifle",
            Some("IonBlaster") => "GameMessageIconIonBlaster",
            Some("BurstRifle") => "GameMessageIconBurstRifle",
            Some("Shotgun") => "GameMessageIconShotgun",
            Some("SniperRifle") => "GameMessageIconSniperRifle",
            Some("HeavyPistol") => "GameMessageIconHeavyPistol",
            Some("Bazooka") => "GameMessageIconBazooka",
            Some("HeavyMG") => "GameMessageIconHeavyMG",
            Some("DrainGun") => "GameMessageIconDrainGun",
            Some("ChainBeam") => "GameMessageIconChainBeam",
            Some("AoESlow") => "GameMessageIconAoESlow",
            Some("SawBlade") => "GameMessageIconSawBlade",
            Some("GrenadeLauncher") => "GameMessageIconGrenadeLauncher",
            Some("ClusterCannon") => "GameMessageIconClusterCannon",
            _ => "",
        },
    }
}

/// The HUD lists what his weapons got (TnWeapon.OnAmmoAdded), his vehicle's too. The
/// heavy ammo crate (`heavy`) serves his heavy weapons instead.
fn fill_ammo(
    jazz: &mut Player,
    heavy: bool,
    sfx: &mut Sfx,
    mut hud: Option<&mut HudState>,
    at: Option<Vec3>,
) {
    let current = gun_slot(jazz);
    for slot in served_slots(jazz, heavy) {
        let g = jazz.gun(slot);
        let (clip, reserve) = jazz.ammo[slot];
        // A gun with no reserve (the X18 Scrapmaker) is filled in its clip
        // (HmWeapon.AddAmmo: a MaxReserveAmmoCount of 0 takes the ammo in the clip).
        if g.reserve_max == 0 {
            let added = g.clip_size.saturating_sub(clip);
            jazz.ammo[slot] = (g.clip_size, 0);
            if let (Some(hud), true) = (hud.as_deref_mut(), added > 0) {
                hud.events.push(HudEvent::AmmoAdded {
                    icon: Some(ammo_icon(slot)).filter(|i| !i.is_empty()),
                    amount: added,
                    current: slot == current,
                });
            }
            continue;
        }
        let added = g.reserve_max.saturating_sub(reserve);
        let reserve = reserve.max(g.reserve_max);
        jazz.ammo[slot] = (clip, reserve);
        if let (Some(hud), true) = (hud.as_deref_mut(), added > 0) {
            hud.events.push(HudEvent::AmmoAdded {
                icon: Some(ammo_icon(slot)).filter(|i| !i.is_empty()),
                amount: added,
                current: slot == current,
            });
        }
        if clip > 0 || reserve == 0 {
            continue;
        }
        if slot == current {
            if jazz.reload_left <= 0.0 {
                start_reload(jazz, slot, sfx, hud.as_deref_mut(), at);
            }
        } else {
            let take = g.clip_size.min(reserve);
            jazz.ammo[slot] = (take, reserve - take);
        }
    }
}

/// A generic blocky robot made of primitives (original design).
fn placeholder_robot(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    let armor = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.58, 0.62),
        metallic: 0.9,
        perceptual_roughness: 0.3,
        ..default()
    });
    let trim = materials.add(StandardMaterial {
        base_color: Color::srgb(0.1, 0.45, 0.45),
        metallic: 0.7,
        perceptual_roughness: 0.4,
        ..default()
    });
    let visor = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 1.0, 0.8),
        emissive: LinearRgba::rgb(1.0, 6.0, 4.0),
        ..default()
    });

    let part = |size: Vec3| Cuboid::new(size.x, size.y, size.z);
    let pieces: [(Vec3, Vec3, &Handle<StandardMaterial>); 9] = [
        (Vec3::new(0.0, 1.55, 0.0), Vec3::new(1.1, 0.9, 0.6), &armor), // chest
        (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.7, 0.4, 0.45), &trim),  // waist
        (Vec3::new(0.0, 2.25, 0.0), Vec3::new(0.4, 0.4, 0.4), &armor), // head
        (
            Vec3::new(0.0, 2.27, -0.21),
            Vec3::new(0.32, 0.08, 0.02),
            &visor,
        ),
        (Vec3::new(-0.75, 1.5, 0.0), Vec3::new(0.3, 0.9, 0.3), &trim), // arms
        (Vec3::new(0.75, 1.5, 0.0), Vec3::new(0.3, 0.9, 0.3), &trim),
        (
            Vec3::new(-0.28, 0.4, 0.0),
            Vec3::new(0.32, 0.8, 0.35),
            &armor,
        ), // legs
        (
            Vec3::new(0.28, 0.4, 0.0),
            Vec3::new(0.32, 0.8, 0.35),
            &armor,
        ),
        (
            Vec3::new(0.85, 1.25, -0.35),
            Vec3::new(0.18, 0.18, 0.7),
            &trim,
        ), // blaster
    ];

    let root = commands
        .spawn((Transform::default(), Visibility::default()))
        .id();
    for (pos, size, mat) in pieces {
        let child = commands
            .spawn((
                Mesh3d(meshes.add(part(size))),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(pos),
            ))
            .id();
        commands.entity(root).add_child(child);
    }
    root
}

/// A generic hover-car made of primitives (original design).
fn placeholder_vehicle(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Entity {
    let body = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.58, 0.62),
        metallic: 0.95,
        perceptual_roughness: 0.25,
        ..default()
    });
    let glow = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 1.0, 0.8),
        emissive: LinearRgba::rgb(1.0, 6.0, 4.0),
        ..default()
    });
    let wheel = materials.add(StandardMaterial {
        base_color: Color::srgb(0.08, 0.08, 0.1),
        perceptual_roughness: 0.8,
        ..default()
    });

    let root = commands
        .spawn((Transform::default(), Visibility::default()))
        .id();
    let mut add = |mesh: Mesh, mat: &Handle<StandardMaterial>, t: Transform| {
        let child = commands
            .spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), t))
            .id();
        commands.entity(root).add_child(child);
    };

    add(
        Cuboid::new(1.8, 0.5, 4.0).into(),
        &body,
        Transform::from_xyz(0.0, 0.55, 0.0),
    );
    add(
        Cuboid::new(1.4, 0.45, 1.6).into(),
        &body,
        Transform::from_xyz(0.0, 1.0, 0.3),
    );
    add(
        Cuboid::new(1.6, 0.08, 0.1).into(),
        &glow,
        Transform::from_xyz(0.0, 0.7, -2.02),
    );
    let wheel_mesh: Mesh = Cylinder::new(0.4, 0.35).into();
    for (x, z) in [(-0.95, -1.3), (0.95, -1.3), (-0.95, 1.3), (0.95, 1.3)] {
        add(
            wheel_mesh.clone(),
            &wheel,
            Transform::from_xyz(x, 0.4, z).with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
        );
    }
    root
}

/// A game camera as it starts (setup_camera; the split screen guest's as he
/// joins, split.rs): looking the way the map's players start.
pub(crate) fn fresh_orbit(level: &Level) -> OrbitCamera {
    OrbitCamera {
        yaw: level.start_yaw,
        pitch: START_PITCH,
        hover: 1.0,
        hover_vel: 0.0,
        rig_form: None,
        rig_from: Vec4::ZERO,
        rig_blend: 1.0,
        rig_last: Vec4::ZERO,
        fov: CAMERA_FOV,
        fov_vel: 0.0,
        scope: 0.0,
        scope_vel: 0.0,
        shoulder: 1.0,
        shoulder_vel: 0.0,
        shake_time: 0.0,
        shaker: Shaker::default(),
        shake: Vec3::ZERO,
        screen_shakes: Vec::new(),
        screen_seed: 0x5eed_1234,
        kick: (0.0, 0.0),
        fov_kick: (0.0, 0.0),
        mouse_rate: Vec2::ZERO,
        fly_spin: Vec2::ZERO,
        fly_anchor: Vec3::ZERO,
        roll: 0.0,
        view_yaw: level.start_yaw,
        vcam: vehicle_cam::Rig::default(),
        robot: robot_cam::RobotCam::default(),
        down: 0.0,
        down_vel: 0.0,
        downed: 0.0,
        watching: None,
        watch_from: (Vec3::ZERO, Quat::IDENTITY, CAMERA_FOV),
        watch_from_on: None,
        watch_blend: 1.0,
        watch_last: (Vec3::ZERO, Quat::IDENTITY, CAMERA_FOV),
        watch_on: None,
        was_dead: false,
        stick: pad::StickLook::default(),
    }
}

fn setup_camera(
    mut commands: Commands,
    map_lighting: Option<Res<lighting::MapLighting>>,
    level: Res<Level>,
) {
    let mut camera = commands.spawn((
        Camera3d {
            // The pause menu's hologram reads the depth buffer.
            depth_texture_usages: (bevy::render::render_resource::TextureUsages::RENDER_ATTACHMENT
                | bevy::render::render_resource::TextureUsages::TEXTURE_BINDING)
                .into(),
            ..default()
        },
        hologram::Hologram::default(),
        // A depth prepass, so the maps' soft effects (levelfx.rs) can fade where they
        // meet the scene, as the game's DepthBiasedAlpha does.
        DepthPrepass,
        // The vehicles' motion blur (camera_follow opens its shutter while a car or
        // truck boosts; the Graphics level may take it off: graphics.rs).
        graphics::camera_motion_blur(),
        // (camera_follow sets the field of view.)
        Projection::from(PerspectiveProjection {
            far: CAMERA_FAR,
            ..default()
        }),
        // HDR, so bright light can bloom. The HUD and menu cameras draw over this
        // one and are HDR too (cameras sharing a window must agree).
        Hdr,
        fresh_orbit(&level),
        // (The first player's: split.rs; the world's layer and his view's own.)
        split::Seat(0),
        split::view_layers(0),
        Transform::from_xyz(0.0, 3.5, 5.5),
        DistanceFog {
            color: Color::srgb(0.36, 0.30, 0.28),
            falloff: FogFalloff::Linear {
                start: 70.0,
                end: 360.0,
            },
            ..default()
        },
    ));
    // The map's own fog, bloom and colour grade.
    if let Some(map) = map_lighting {
        map.camera_effects(&mut camera);
    }
}

// ---------------------------------------------------------------------------
// Update systems
// ---------------------------------------------------------------------------

/// A click takes the mouse back if it got away (the pause menu, on Esc, frees and
/// takes it itself).
fn cursor_grab(
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pause: Res<menu::Pause>,
) {
    if !pause.open
        && !pause.choosing
        && mouse.just_pressed(MouseButton::Left)
        && cursor.grab_mode == CursorGrabMode::None
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

/// The keys and buttons as they really are, put aside while the game's systems
/// run with none (see `pause_game_input`).
#[derive(Resource, Default)]
struct HeldInput(Option<(ButtonInput<KeyCode>, ButtonInput<MouseButton>)>);

/// While the pause menu is up the game goes on, but Jazz takes no orders: the
/// keys, buttons and wheel are put aside before the game's systems read them
/// (and given back after them, see `restore_game_input`, so the menus keep seeing
/// what is held). The same goes for the frame the menu closes. Also tells the
/// pause menu where Jazz is, starts the match over when the player quits to the
/// main menu (every Jazz back at his start, healed and rearmed, the other
/// players' Jazzes gone, the sentry count back to zero, the energon back
/// to what a match starts with, every health station stocked), and starts keeping
/// this PC's Jazz's sounds for the other players (see `net_capture`).
#[allow(clippy::too_many_arguments)]
fn pause_game_input(
    mut commands: Commands,
    mut pause: ResMut<menu::Pause>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut scroll: ResMut<AccumulatedMouseScroll>,
    mut held: ResMut<HeldInput>,
    mut score: split::BySeat<Score>,
    level: Res<Level>,
    mut sfx: ResMut<Sfx>,
    mut players: Query<(
        Entity,
        &mut Transform,
        &mut Player,
        &PlayerSlot,
        Has<LocalPlayer>,
        Has<RemotePlayer>,
        Option<&split::Seat>,
    )>,
    mut forms: Query<
        (Entity, &mut Visibility, Has<RobotForm>, &JazzOf),
        Or<(With<RobotForm>, With<VehicleForm>)>,
    >,
    mut stations: Query<&mut HealthStation>,
    (mut ammo_stations, mut weapon_stations, mut equipment_stations): (
        Query<&mut AmmoStation>,
        Query<&mut shop::WeaponStation>,
        Query<&mut shop::EquipmentStation>,
    ),
    pre: Res<pregame::PreMatch>,
) {
    // From here until net_capture, the sounds asked for near the camera are this
    // PC's Jazz's (the pause menu's own come before).
    sfx.begin_capture();
    if pause.reset_match {
        pause.reset_match = false;
        for (entity, mut transform, mut player, slot, _, remote, _) in &mut players {
            if remote {
                commands.entity(entity).despawn();
                continue;
            }
            if player.form == Form::Vehicle {
                switch_form(
                    &mut commands,
                    entity,
                    &mut player,
                    &mut forms,
                    Form::Robot,
                    false,
                );
            }
            player.pending_form = None;
            player.transform_left = 0.0;
            player.form_weapon_left = None;
            player.dying = false;
            player.dead_left = 0.0;
            player.health = player.max_health();
            player.shield = PLAYER_MAX_SHIELD;
            player.ammo = full_ammo();
            player.equipment = [None; 2];
            player.support = equipment::Support::default();
            player.action = None;
            player.protection = SPAWN_PROTECTION;
            player.vertical_velocity = 0.0;
            player.jammed_left = 0.0;
            player.knock_vel = Vec3::ZERO;
            transform.translation = level.start(slot.0);
        }
        for score in score.each() {
            score.destroyed = 0;
            score.energon = STARTING_ENERGON;
        }
        for mut station in &mut stations {
            station.away = 0.0;
        }
        for mut station in &mut ammo_stations {
            station.away = 0.0;
        }
        for mut station in &mut weapon_stations {
            station.away = 0.0;
        }
        for mut station in &mut equipment_stations {
            station.away = 0.0;
        }
    }
    // (Whoever opened the pause menu: split screen's guest his own.)
    if let Some((_, transform, ..)) = players
        .iter()
        .find(|p| p.4 && split::seat_of(p.6) == pause.seat)
        .or_else(|| players.iter().find(|p| p.4))
    {
        pause.player_at = transform.translation + Vec3::Y;
    }
    // (Nor are they his before the match has begun for him.)
    if pause.open || pause.choosing || pause.just_closed || pre.waiting {
        pause.just_closed = false;
        held.0 = Some((keys.clone(), mouse.clone()));
        keys.reset_all();
        mouse.reset_all();
        scroll.delta = Vec2::ZERO;
    }
}

/// Gives back the keys and buttons put aside while the pause menu is up.
fn restore_game_input(
    mut held: ResMut<HeldInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    if let Some((k, m)) = held.0.take() {
        *keys = k;
        *mouse = m;
    }
}

/// Leaving the game for the main menu: the car's engine loops go quiet (they keep
/// playing, silent, until the game is back, and come up again from silence).
fn quiet_engine(mut voices: Query<(&mut EngineVoice, Option<&mut AudioSink>)>) {
    for (mut voice, sink) in &mut voices {
        voice.level = 0.0;
        if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(0.0));
        }
    }
}

fn toggle_form(
    mut commands: Commands,
    time: Res<Time>,
    mut players: Query<(Entity, &mut Player, &PlayerInput), Without<RemotePlayer>>,
    animators: Query<&JazzOf, With<RobotAnimator>>,
    mut forms: Query<
        (Entity, &mut Visibility, Has<RobotForm>, &JazzOf),
        Or<(With<RobotForm>, With<VehicleForm>)>,
    >,
) {
    for (owner, mut player, input) in &mut players {
        // FORM_WEAPON_SECS into a transformation every weapon he carries is
        // reloaded.
        if let Some(left) = player.form_weapon_left {
            let left = left - time.delta_secs();
            if left <= 0.0 {
                reload_all_weapons(&mut player);
            } else {
                player.form_weapon_left = Some(left);
            }
        }
        // A transformation is playing: wait for it, then finish the switch.
        if player.transform_left > 0.0 {
            player.transform_left -= time.delta_secs();
            if player.transform_left <= 0.0 {
                player.transform_left = 0.0;
                if let Some(form) = player.pending_form.take() {
                    switch_form(&mut commands, owner, &mut player, &mut forms, form, false);
                }
            }
            continue;
        }
        // Change Form transforms on the ground or in mid-air, as in the game.
        if !input.transform || !can_transform(&player) {
            continue;
        }
        // (His robot model animates once it has loaded.)
        let animated = animators.iter().any(|of| of.0 == owner);
        // His character's halves of the transformation (Jazz's own without).
        let (to_vehicle, to_robot) = transform_clips(&player.kit);
        let timing = player.kit.timing;
        match player.form {
            Form::Robot if animated => {
                // Fold up first, then swap in the car model (as the robot
                // vanishes: his clip's ToggleHidden).
                player.transform_left = timing.robot_hide / TRANSFORM_SPEED;
                player.pending_form = Some(Form::Vehicle);
                player.start_clip = Some((to_vehicle, TRANSFORM_SPEED));
                player.action = None;
                player.dash_left = 0.0;
                player.transform_clip = to_vehicle;
                player.carry_vel = player.velocity;
                player.form_weapon_left = Some(FORM_WEAPON_SECS);
            }
            Form::Robot => {
                switch_form(
                    &mut commands,
                    owner,
                    &mut player,
                    &mut forms,
                    Form::Vehicle,
                    true,
                );
                // (A change with no animation is over at once.)
                reload_all_weapons(&mut player);
            }
            Form::Vehicle => {
                // Swap in the robot at once and let it unfold, still rolling (the
                // car's parts fly apart around it for a moment, car.rs).
                player.carry_vel = player.velocity;
                switch_form(
                    &mut commands,
                    owner,
                    &mut player,
                    &mut forms,
                    Form::Robot,
                    !animated,
                );
                if animated {
                    player.transform_left = timing.robot_unfold / TRANSFORM_SPEED;
                    player.start_clip = Some((to_robot, TRANSFORM_SPEED));
                    player.transform_clip = to_robot;
                    player.form_weapon_left = Some(FORM_WEAPON_SECS);
                } else {
                    reload_all_weapons(&mut player);
                }
            }
        }
    }
}

/// Every weapon Jazz carries reloaded at once, silently, from its reserve, as a
/// transformation brings out the weapons of his new form (TnInventoryManager.
/// ReloadAllWeapons: each weapon's InstantReload, HmWeapon.RefillClip): his two
/// guns and his vehicle's weapon, each clip filled as far as its reserve allows.
/// A reload under way is overtaken by it.
fn reload_all_weapons(player: &mut Player) {
    player.form_weapon_left = None;
    for slot in [player.loadout[0], player.loadout[1], vehicle_slot(player)] {
        player.ammo[slot] = refilled(player.ammo[slot], player.gun(slot).clip_size);
    }
    player.reload_left = 0.0;
}

/// A clip filled from its reserve (HmWeapon.RefillClip, its MaxAmountThatCanBeRefilled
/// unset): `(clip, reserve)` with as many rounds as the clip has room for, or as
/// the reserve holds.
fn refilled((clip, reserve): (u32, u32), clip_size: u32) -> (u32, u32) {
    let take = clip_size.saturating_sub(clip).min(reserve);
    (clip + take, reserve - take)
}

/// Whether Change Form would start a change now (toggle_form): not changing already,
/// not dead, not switching hands.
fn can_transform(player: &Player) -> bool {
    player.transform_left <= 0.0
        && player.dead_left <= 0.0
        && player.hand_switch.is_none()
        && !player.start_hand_switch
}

/// Jazz (`owner`) changes form: his robot or car model shows.
fn switch_form(
    commands: &mut Commands,
    owner: Entity,
    player: &mut Player,
    forms: &mut Query<
        (Entity, &mut Visibility, Has<RobotForm>, &JazzOf),
        Or<(With<RobotForm>, With<VehicleForm>)>,
    >,
    form: Form,
    // A quick scale-up for the form that appears (when there is no animation).
    pop: bool,
) {
    player.form = form;
    // The car picks up the speed Jazz had while folding up.
    let facing = Quat::from_rotation_y(player.heading);
    let (drive, strafe) = if form == Form::Vehicle {
        (
            player.carry_vel.dot(facing * Vec3::NEG_Z).max(0.0),
            player.carry_vel.dot(facing * Vec3::X),
        )
    } else {
        (0.0, 0.0)
    };
    player.drive_speed = drive;
    player.strafe_speed = strafe;
    player.yaw_rate = 0.0;
    player.pivot = None;
    for (entity, mut visibility, is_robot, of) in forms.iter_mut() {
        if of.0 != owner {
            continue;
        }
        let show = is_robot == (form == Form::Robot);
        *visibility = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if show && pop {
            commands
                .entity(entity)
                .insert(TransformPop(Timer::from_seconds(0.25, TimerMode::Once)));
        }
    }
}

/// When the robot model has loaded, hook its AnimationPlayer up to our graph and
/// start every looping clip (their weights decide what is actually seen).
#[allow(clippy::type_complexity)]
fn attach_robot_animator(
    mut commands: Commands,
    anims: Option<Res<RobotAnimations>>,
    mut new_players: Query<
        (Entity, &mut AnimationPlayer),
        (Added<AnimationPlayer>, Without<standin::StandIn>),
    >,
    parents: Query<&ChildOf>,
    vehicles: Query<(), With<VehicleForm>>,
    previews: Query<(), With<customize::PreviewModel>>,
    own: Query<(), With<OwnAnimation>>,
) {
    let Some(anims) = anims else {
        return;
    };
    for (entity, mut animation_player) in &mut new_players {
        // The car model has its own (car.rs), the Customize screen's robots theirs,
        // and the pods and mines theirs.
        if car::in_vehicle(entity, &parents, &vehicles)
            || customize::in_preview(entity, &parents, &previews)
            || parents.iter_ancestors(entity).any(|e| own.contains(e))
        {
            continue;
        }
        // (The Infiltrator's graph first: follow_robot_class gives it its Jazz's
        // class's once it knows whose it is.)
        commands.entity(entity).insert((
            AnimationGraphHandle(anims.graph(character::INFILTRATOR)),
            RobotAnimator,
            RobotGraphClass(character::INFILTRATOR),
        ));
        for i in 0..ANIM_COUNT {
            let looping = i < ANIM_TO_VEHICLE
                || (ANIM_SHOOT_IDLE..ANIM_JUMP).contains(&i)
                || (ANIM_STRAFE_EXTRA..ANIM_PIVOT).contains(&i)
                // The heavy-gun stance's standing, jogging and walking.
                || (ANIM_HEAVY_IDLE..ANIM_HEAVY_PIVOT).contains(&i)
                // Every gun's idle motion, and the firing loop of those that loop one
                // while the trigger is held (a burst's or a shot's recoil is a one-off
                // that `fire` starts).
                || WEAPONS.iter().any(|w| {
                    i == w.idle_anim
                        || (i == w.fire_anim && w.burst.is_none() && w.shot_anim.is_none())
                });
            if looping {
                for (k, node) in anims.pair(i).enumerate() {
                    let weight = if i == ANIM_SHOOT_IDLE && k == 0 {
                        1.0
                    } else {
                        0.0
                    };
                    animation_player.play(node).repeat().set_weight(weight);
                }
            }
        }
    }
}

/// The class whose graph a robot model's AnimationPlayer plays (RobotAnimations).
#[derive(Component)]
struct RobotGraphClass(usize);

/// Each Jazz's robot plays his class's graph: its own clips where it has them (its
/// heavy jog and sprint, its sword), the same nodes in the same places, so what he
/// is doing goes on as it was.
fn follow_robot_class(
    mut commands: Commands,
    anims: Option<Res<RobotAnimations>>,
    players: Query<&Player>,
    mut animators: Query<(Entity, &JazzOf, &mut RobotGraphClass), With<RobotAnimator>>,
) {
    let Some(anims) = anims else {
        return;
    };
    for (entity, of, mut playing) in &mut animators {
        let Ok(player) = players.get(of.0) else {
            continue;
        };
        if playing.0 != player.kit.class {
            playing.0 = player.kit.class;
            commands
                .entity(entity)
                .insert(AnimationGraphHandle(anims.graph(player.kit.class)));
        }
    }
}

/// Blends idle, the six jog directions and sprint from the movement input (in the
/// shooting or fine-aim stance), plays the one-off moves and the transformation
/// clips, and layers the gun's own animations on top: for each Jazz, on his robot
/// model's animation player. (robot_aim.rs then turns his upper body and gun arm
/// to where he aims, from his RobotAnimState's aim offset node.)
fn animate_robot(
    time: Res<Time>,
    anims: Option<Res<RobotAnimations>>,
    mut players: Query<(&mut Player, &mut RobotAnimState)>,
    mut animators: Query<(&mut AnimationPlayer, &JazzOf), With<RobotAnimator>>,
) {
    let Some(anims) = anims else {
        return;
    };
    let dt = time.delta_secs();
    for (mut animation_player, of) in &mut animators {
        let Ok((mut player, mut state)) = players.get_mut(of.0) else {
            continue;
        };
        animate_jazz(&anims, dt, &mut animation_player, &mut player, &mut state);
    }
}

/// animate_robot for one Jazz.
fn animate_jazz(
    anims: &RobotAnimations,
    dt: f32,
    animation_player: &mut AnimationPlayer,
    player: &mut Player,
    state: &mut RobotAnimState,
) {
    let RobotAnimState {
        weights,
        transform_blend,
        air_clip,
        hips_back,
        death,
        limp_frames,
        downed,
        ability,
        aim: state_aim,
    } = state;
    if weights.len() != ANIM_TOTAL {
        weights.resize(ANIM_TOTAL, 0.0);
    }
    // His death (downed.rs): its clip cutting in over the rest; then, as his robot
    // falls as a ragdoll, none of them (its bones are the ragdoll's); as he comes
    // back, his own clips again, from where they have got to.
    if let Some((k, speed)) = player.start_death_clip.take()
        && let Some(&node) = anims.deaths.get(k)
    {
        if let Some((old, _)) = death.take() {
            animation_player.stop(old);
        }
        let active = animation_player.play(node);
        active.replay();
        active.set_speed(speed).set_weight(0.0);
        *death = Some((node, 0.0));
    }
    if player.limp {
        // (A frame on, so the ragdoll takes over from the pose as it moved.)
        *limp_frames = limp_frames.saturating_add(1);
        if *limp_frames >= 2 {
            weights.fill(0.0);
            for (_, active) in animation_player.playing_animations_mut() {
                active.set_weight(0.0);
            }
        }
        return;
    }
    *limp_frames = 0;
    let dead = player.dead_left > 0.0 || player.dying;
    if !dead && let Some((node, _)) = death.take() {
        animation_player.stop(node);
    }
    // Down, getting up or reviving another (revive.rs): the downed clips play, full
    // body, in place of his own (blend_downed), with his gun's own layers on them.
    let downed_pose = !dead && player.revive.locked() && anims.downed.is_some();
    let mut target = [0.0f32; ANIM_TOTAL];
    // The clips his character's arms bring: the forearms changing over and the
    // shoulder pads moving as he switches hands, the pads as a gun comes out
    // (Jazz's forearms' own without the characters).
    let arm_switch = player.kit.arm_switch.map_or(ANIM_ARM_SWITCH, extra_clip);
    let shoulder_switch = player.kit.shoulder_switch.map(extra_clip);
    let shoulder_equip = player.kit.shoulder_equip.map(extra_clip);
    // The gun's one-off animation (the rifle's barrels spinning down).
    if let Some((clip, secs, speed)) = player.start_weapon_clip.take() {
        anims.restart(animation_player, clip, speed, 0.0);
        player.weapon_clip = Some((clip, secs));
        // A new gun's reload: its steam on its sockets (arsenal.rs plays it).
        if WEAPONS.get(player.weapon).is_some_and(|w| {
            w.reload_anim.0 == clip && w.gear.is_some_and(|g| !g.on_reload.is_empty())
        }) {
            player.gear.reloading = true;
        }
        player.played.push(net::JazzEvent::WeaponClip {
            clip: clip as u8,
            secs,
            speed,
        });
    }
    // Switching weapons: the new gun's Equip clip and the old one's Unequip start at
    // full weight (the new gun starts folded up inside the forearm, the old one
    // whole), cutting off the clips they replace and any one-off clip of the old gun.
    let start_equip = player.start_equip.take();
    if let Some((new, old)) = start_equip {
        player.played.push(net::JazzEvent::Equip {
            new: new as u8,
            old: old.map_or(u8::MAX, |w| w as u8),
        });
    }
    // (With the gun put away, the new one goes straight into the forearm: the
    // game's AttachWeapon holsters it at once, ShouldHolster, playing nothing.)
    if let Some((new, old)) = start_equip.filter(|_| !player.holstered) {
        let mut cut = vec![];
        cut.extend(player.equipping.take().map(|(w, _)| WEAPONS[w].equip_anim));
        cut.extend(player.weapon_clip.take().map(|(clip, _)| clip));
        if let Some(gone) = old {
            cut.extend(
                player
                    .unequipping
                    .take()
                    .map(|(w, _)| WEAPONS[w].unequip_anim),
            );
            // (Its idle and firing loop too, which the new gun's Equip replaces on
            // the same bones.)
            cut.extend([WEAPONS[gone].idle_anim, WEAPONS[gone].fire_anim]);
        }
        for clip in cut {
            weights[clip] = 0.0;
        }
        // The shoulder pads move as the gun comes out.
        if let Some(clip) = shoulder_equip {
            anims.restart(animation_player, clip, 1.0, 0.0);
            weights[clip] = 1.0;
        }
        // (As the gun comes back out after a sprint or a revive, the forearm folds
        // away on the CloudB bones, holsters below: the gun it was bringing out
        // goes at once, rather than folding away on them too, under the forearm's
        // clip, which sent its parts flying off his arm and back.)
        let coming_back = player.holster_change.is_some();
        let mut start = vec![(WEAPONS[new].equip_anim, new, false)];
        start.extend(
            old.filter(|_| !coming_back)
                .map(|w| (WEAPONS[w].unequip_anim, w, true)),
        );
        for (clip, weapon, away) in start {
            anims.restart(animation_player, clip, 1.0, 0.0);
            weights[clip] = 1.0;
            if away {
                player.unequipping = Some((weapon, EQUIP_ANIM_SECS));
            } else {
                player.equipping = Some((weapon, EQUIP_ANIM_SECS));
            }
        }
    }
    // The gun going away as he sprints or revives, and coming back (holsters): its
    // Unequip_WEP folding it away on the CloudB gun bones (shown on the copy there,
    // as when swapping) and the forearm set's Equip_ARM unfolding the forearm on its
    // arm; or its Equip_WEP unfolding it out of the forearm, the forearm set's
    // Unequip_ARM folding that away on the CloudB arm bones. The shoulder pads move
    // either way (the game's ShoulderAnimation, as the visible weapon changes). Not
    // while a weapon change or a hand switch starts, or one is under way: it waits.
    let arm_away = player.kit.arm_away.map(extra_clip);
    let arm_back = player.kit.arm_back.map(extra_clip);
    let want = holsters(player);
    if want.is_none() {
        // (Out of robot form, transforming or dead: back in hand at once.)
        player.holstered = false;
        player.holster_change = None;
    }
    if let Some(want) = want
        && want != player.holstered
        && start_equip.is_none()
        && !player.start_hand_switch
        && player.hand_switch.is_none()
    {
        player.holstered = want;
        player.holster_change = Some(0.0);
        let weapon = player.weapon.min(WEAPONS.len() - 1);
        let w = &WEAPONS[weapon];
        // (Its idle, firing loop and one-off clips stop; the arm clip it replaces.)
        let mut cut = vec![w.idle_anim, w.fire_anim];
        cut.extend(player.weapon_clip.take().map(|(clip, _)| clip));
        cut.extend(arm_away.into_iter().chain(arm_back));
        if want {
            cut.extend(player.equipping.take().map(|(g, _)| WEAPONS[g].equip_anim));
        } else {
            cut.extend(
                player
                    .unequipping
                    .take()
                    .map(|(g, _)| WEAPONS[g].unequip_anim),
            );
        }
        for clip in cut {
            weights[clip] = 0.0;
        }
        let (gun_clip, arm_clip) = if want {
            player.unequipping = Some((weapon, EQUIP_ANIM_SECS));
            (w.unequip_anim, arm_away)
        } else {
            player.equipping = Some((weapon, EQUIP_ANIM_SECS));
            (w.equip_anim, arm_back)
        };
        for clip in [Some(gun_clip), arm_clip, shoulder_equip]
            .into_iter()
            .flatten()
        {
            anims.restart(animation_player, clip, 1.0, 0.0);
            weights[clip] = 1.0;
        }
    }
    if let Some(t) = player.holster_change {
        let t = t + dt;
        player.holster_change = (t < HOLSTER_ARM_SECS).then_some(t);
    }
    match (player.holster_change, player.holstered) {
        (Some(_), true) => {
            if let Some(clip) = arm_away {
                target[clip] = 1.0;
            }
        }
        (Some(_), false) => {
            if let Some(clip) = arm_back {
                target[clip] = 1.0;
            }
        }
        // (Done: at once, the forearm out or folded away and hidden.)
        (None, _) => {
            for clip in arm_away.into_iter().chain(arm_back) {
                weights[clip] = 0.0;
            }
        }
    }
    // Switching hands: the gun's MirrorReverse clip, at full weight from the start
    // (it starts with the gun whole on the arm it leaves and folded up in the other).
    if std::mem::take(&mut player.start_hand_switch) {
        player.played.push(net::JazzEvent::HandSwitch);
        let clip = WEAPONS[player.weapon.min(WEAPONS.len() - 1)].change_hands_anim;
        if let Some((w, _)) = player.hand_switch.take() {
            weights[WEAPONS[w].change_hands_anim] = 0.0;
        }
        // (In place of the gun's idle.)
        weights[WEAPONS[player.weapon.min(WEAPONS.len() - 1)].idle_anim] = 0.0;
        // The gun's clip, the forearms' and the shoulder pads' together.
        for clip in [Some(clip), Some(arm_switch), shoulder_switch]
            .into_iter()
            .flatten()
        {
            anims.restart(animation_player, clip, 1.0, 0.0);
            weights[clip] = 1.0;
        }
        player.hand_switch = Some((player.weapon, HAND_SWITCH_SECS));
    }
    let p: &mut Player = player;
    for slot in [&mut p.equipping, &mut p.unequipping, &mut p.hand_switch] {
        if let Some((weapon, left)) = *slot {
            let left = left - dt;
            *slot = (left > 0.0).then_some((weapon, left));
        }
    }
    if let Some((weapon, _)) = player.equipping {
        target[WEAPONS[weapon].equip_anim] = 1.0;
    }
    // (The shoulder pads' as a gun comes out, or goes away or back: holsters.)
    if let Some(clip) = shoulder_equip {
        if player.equipping.is_some() || player.holster_change.is_some() {
            target[clip] = 1.0;
        } else {
            weights[clip] = 0.0;
        }
    }
    if let Some((weapon, _)) = player.hand_switch {
        target[WEAPONS[weapon].change_hands_anim] = 1.0;
        target[arm_switch] = 1.0;
        if let Some(clip) = shoulder_switch {
            target[clip] = 1.0;
        }
    } else {
        for clip in WEAPONS
            .iter()
            .map(|w| w.change_hands_anim)
            .chain([ANIM_ARM_SWITCH, arm_switch])
            .chain(shoulder_switch)
        {
            weights[clip] = 0.0;
        }
    }
    if let Some((weapon, _)) = player.unequipping {
        target[WEAPONS[weapon].unequip_anim] = 1.0;
    } else {
        // Folded away and hidden: done at once.
        for w in &WEAPONS {
            weights[w.unequip_anim] = 0.0;
        }
    }
    let in_hand = &WEAPONS[player.weapon.min(WEAPONS.len() - 1)];
    // The gun's firing loop: while it fires (a new gun's from its first round,
    // StartFire: the X18 Scrapmaker's not while it warms up).
    let fire_loop = player.firing && (in_hand.gear.is_none() || player.gear.fired);
    // A burst's recoil, or a shot's, plays on while firing (other one-off gun clips
    // stop for the firing loop).
    let shot_clip = in_hand
        .burst
        .map(|b| b.anim.0)
        .or(in_hand.shot_anim.map(|a| a.0));
    if let Some((clip, left)) = player.weapon_clip {
        let left = left - dt;
        if left > 0.0 && (!fire_loop || Some(clip) == shot_clip) {
            player.weapon_clip = Some((clip, left));
            // Fade out over the last 0.2 s so the parts settle back smoothly.
            target[clip] = (left / 0.2).min(1.0);
        } else {
            player.weapon_clip = None;
        }
    }
    if let Some((clip, speed)) = player.start_clip.take() {
        player.played.push(net::JazzEvent::Clip {
            clip: clip as u8,
            speed,
        });
        let skip = if clip == ANIM_JUMP {
            JUMP_TAKEOFF_SKIP
        } else {
            0.0
        };
        anims.restart(animation_player, clip, speed, skip);
        player.move_clip = Some((clip, skip, speed));
        *transform_blend = None;
        if is_to_vehicle(clip) {
            // Folding up blends in from the pose he was in (below; it isn't
            // mirrored, as the car's half of it isn't).
            weights[clip] = 0.0;
            *transform_blend = Some((clip, 0.0));
        } else if is_to_robot(clip) {
            // Unfolding cuts in (he is hidden as it starts).
            weights.fill(0.0);
            weights[clip] = 1.0;
            player.mirror = 0.0;
        }
    }

    // The unfolding blends out over its last TRANSFORM_BLEND_OUT into his own clips
    // (the game's BlendOutTime), his moves taking over as it ends.
    let unfold_out = (death.is_none()
        && !downed_pose
        && player.transform_left > 0.0
        && player.transform_left < TRANSFORM_BLEND_OUT
        && is_to_robot(player.transform_clip))
    .then(|| player.transform_left / TRANSFORM_BLEND_OUT);
    if death.is_some() || downed_pose {
        // (Only his death's clip, or the downed ones.)
    } else if player.transform_left > 0.0 && unfold_out.is_none() {
        target[player.transform_clip] = 1.0;
    } else if let Some((clip, _)) = player.action {
        target[clip] = 1.0;
    } else {
        let input = player.move_input;
        // Shooting from the hip (his gun always up: `stance`), or aiming down the
        // sights; or, with a heavy weapon, its heavy-gun stance.
        let stance = stance(player);
        let heavy = stance > 0 && in_hand.heavy;
        let (idle, moving) = if heavy {
            (ANIM_HEAVY_IDLE, ANIM_HEAVY_JOG)
        } else {
            [
                (ANIM_IDLE, ANIM_JOG_FL),
                (ANIM_SHOOT_IDLE, ANIM_SHOOT_MOVE),
                (ANIM_AIM_IDLE, ANIM_AIM_MOVE),
            ][stance]
        };
        let extra = ANIM_STRAFE_EXTRA + stance * 4;
        let (f, b, lb, rb) = if heavy {
            (
                ANIM_HEAVY_JOG + 6,
                ANIM_HEAVY_JOG + 7,
                ANIM_HEAVY_JOG + 8,
                ANIM_HEAVY_JOG + 9,
            )
        } else {
            (
                extra + STRAFE_F,
                extra + STRAFE_B,
                extra + STRAFE_LB,
                extra + STRAFE_RB,
            )
        };
        // The heavy-gun stance's walk and jog by his speed (WalkJogBlend, a
        // TnVelocityAnimBlend: all walk at 450 units a second and slower, all jog
        // from 1200): the share of the jog.
        let jog = ((player.velocity.length() - HEAVY_WALK_SPEED)
            / (HEAVY_JOG_SPEED - HEAVY_WALK_SPEED))
            .clamp(0.0, 1.0);
        // A direction's clip at weight `w` (a heavy jog's, its walk's too).
        let add = |target: &mut [f32; ANIM_TOTAL], clip: usize, w: f32| {
            if heavy && (ANIM_HEAVY_JOG..ANIM_HEAVY_WALK).contains(&clip) {
                target[clip] += w * jog;
                target[clip + (ANIM_HEAVY_WALK - ANIM_HEAVY_JOG)] += w * (1.0 - jog);
            } else {
                target[clip] += w;
            }
        };
        // Directions within a stance's moving clips (FL, FR, L, R, BL, BR).
        let (fl, fr, l, r, bl, br) = (
            moving,
            moving + 1,
            moving + 2,
            moving + 3,
            moving + 4,
            moving + 5,
        );
        if let (false, Some(t)) = (player.grounded, player.jump_time) {
            // Jumping: the take-off, then the descent loop.
            if t < jump_takeoff_secs(&player.kit) {
                target[ANIM_JUMP] = 1.0;
            } else {
                target[ANIM_JUMP_DESCENT] = 1.0;
            }
        } else if !player.grounded && player.air_time > FALL_DELAY {
            // Stepped off a ledge.
            target[ANIM_FALL] = 1.0;
        } else if input.length() < 0.1 || !player.grounded {
            // Standing, or turning on the spot.
            match player.pivot {
                Some(pivot) if player.grounded => target[pivot.clip] = 1.0,
                _ => target[idle] = 1.0,
            }
        } else if sprint_run(player.sprinting, input) {
            // (Turned to face the way he runs: move_jazz.)
            target[ANIM_SPRINT] = 1.0;
        } else {
            // Angle of travel: 0 = forward, 90 = right, 180 = back, -90 = left.
            let angle = input.x.atan2(input.y).to_degrees();
            // The game's ten strafe directions: forward, back, the four diagonals,
            // and the sides twice, hips forward (L, R) for the front half and
            // hips back (LB, RB) for the back half, switching over around the side.
            if *hips_back {
                if angle.abs() < HIP_FLIP_FRONT {
                    *hips_back = false;
                }
            } else if angle.abs() > HIP_FLIP_BACK {
                *hips_back = true;
            }
            let (dirs, at) = if *hips_back {
                // The back half, measured round from the right (90) to the left (270).
                let a = if angle < 0.0 { angle + 360.0 } else { angle };
                (
                    [
                        (90.0, rb),
                        (135.0, br),
                        (180.0, b),
                        (225.0, bl),
                        (270.0, lb),
                    ],
                    a.clamp(90.0, 270.0),
                )
            } else {
                (
                    [(-90.0, l), (-45.0, fl), (0.0, f), (45.0, fr), (90.0, r)],
                    angle.clamp(-90.0, 90.0),
                )
            };
            for pair in dirs.windows(2) {
                let (a0, c0) = pair[0];
                let (a1, c1) = pair[1];
                if at >= a0 && at <= a1 {
                    let k = (at - a0) / (a1 - a0);
                    add(&mut target, c0, 1.0 - k);
                    add(&mut target, c1, k);
                    break;
                }
            }
        }
    }

    if let Some(k) = unfold_out {
        for (i, t) in target.iter_mut().enumerate() {
            if !is_additive(i) {
                *t *= 1.0 - k;
            }
        }
        target[player.transform_clip] = k;
    }

    // The descent and fall clips play from their start each time they come in.
    let air = [ANIM_JUMP_DESCENT, ANIM_FALL]
        .into_iter()
        .find(|&clip| target[clip] > 0.0);
    if air != *air_clip {
        if let Some(clip) = air {
            anims.restart(animation_player, clip, 1.0, 0.0);
        }
        *air_clip = air;
    }

    // The gun's additive layers: its idle motion whenever it is out, the firing
    // loop (recoil and moving parts) while the trigger is held. (A burst weapon's
    // recoil is a one-off per burst, and the Scatter Blaster's one per shot, started
    // by `fire`.) The gun's own clips take turns, as in the game's WeaponCustom slot
    // (TnAnimNodeWeaponSlot: its idle, TnWeaponEffectIdleAnimation's, plays only
    // while none of the gun's other clips does, all of them CAMT_Weapon): the idle
    // stops for the firing loop, a shot's or a burst's clip, the reload or another
    // one-off, the gun coming out and switching hands, rather than adding to them
    // (with both, the Orbital Pistol's orbiting parts, which its idle turns, fly off
    // the gun).
    let weapon = in_hand;
    if player.form == Form::Robot && player.transform_left <= 0.0 && player.dead_left <= 0.0 {
        let looping = fire_loop && weapon.burst.is_none() && weapon.shot_anim.is_none();
        if looping {
            target[weapon.fire_anim] = 1.0;
        }
        let busy = looping
            || player.weapon_clip.is_some()
            || player.equipping.is_some()
            || player.hand_switch.is_some()
            || player.holstered;
        if !busy {
            target[weapon.idle_anim] = 1.0;
        }
    }

    // The class abilities' poses (abilities.rs) take over from his own clips: his
    // raised Flak Shield's stance on the ground (not through a move of his own), his
    // hover's pose while he hovers (ability_clips plays them).
    let free_pose = death.is_none()
        && !downed_pose
        && player.transform_left <= 0.0
        && player.form == Form::Robot
        && !dead;
    let shield_want = if free_pose && player.grounded && player.action.is_none() {
        player.ability.shield.stance
    } else {
        0.0
    };
    let hover_want = if free_pose
        && player.ability.kind == abilities::Kind::Hover
        && player.ability.hover.phase == hoverslam::Phase::Hovering
        && player.action.is_none()
    {
        1.0
    } else {
        0.0
    };
    let ease = (dt * 10.0).min(1.0);
    ability.shield += (shield_want - ability.shield) * ease;
    ability.hover += (hover_want - ability.hover) * (dt / 0.3).min(1.0);
    let over = (ability.shield + ability.hover).min(1.0);
    if over > 1e-3 {
        for (i, t) in target.iter_mut().enumerate() {
            if !is_additive(i) {
                *t *= 1.0 - over;
            } else if (ANIM_FIRST_ADDITIVE..ANIM_END_ADDITIVE).contains(&i) {
                // (The shield's stance holds his gun its own way, its arm on the
                // shield: his gun's layers stay out of it. Hovering, he aims and
                // shoots as ever.)
                *t *= 1.0 - ability.shield.min(1.0);
            }
        }
    }

    // Ease towards the target weights so changes blend smoothly.
    // One-off moves blend in faster so they feel responsive, the gun layers faster still
    // (a death's clip as fast).
    let rate = if player.action.is_some() || death.is_some() {
        20.0
    } else {
        10.0
    };
    if let Some((node, w)) = death.as_mut() {
        *w += (1.0 - *w) * (dt * rate).min(1.0);
        if let Some(active) = animation_player.animation_mut(*node) {
            active.set_weight(*w);
        }
    }
    // The mirrored set: the body blends over to it (MIRROR_BLEND_SECS); the gun's
    // own clips switch at once (the game's mirror node leaves the gun bones out of
    // its blend).
    let mirror_target =
        if player.left_handed && player.transform_left <= 0.0 && anims.mirrored.is_some() {
            1.0
        } else {
            0.0
        };
    let step = dt / MIRROR_BLEND_SECS;
    player.mirror += (mirror_target - player.mirror).clamp(-step, step);
    let left = if player.left_handed && anims.mirrored.is_some() {
        1.0
    } else {
        0.0
    };
    // A gun's idle starts again from its first frame when it comes back after
    // another of the gun's clips (TryToPlayIdleAnimation's PlayCustomAnim), which
    // ended where the idle starts.
    for w in &WEAPONS {
        if target[w.idle_anim] > 0.0 && weights[w.idle_anim] < 0.01 {
            anims.restart(animation_player, w.idle_anim, 1.0, 0.0);
        }
    }
    // His folding up's blend-in: its weight rising evenly over TRANSFORM_BLEND_IN as
    // the clips it takes over from go (the game's BlendInTime).
    let transforming = player.transform_left > 0.0 && death.is_none() && !downed_pose;
    let blend_in = match *transform_blend {
        Some((clip, e)) if transforming => {
            let to = e + dt;
            *transform_blend = (to < TRANSFORM_BLEND_IN).then_some((clip, to));
            Some((
                clip,
                (e / TRANSFORM_BLEND_IN).min(1.0),
                (to / TRANSFORM_BLEND_IN).min(1.0),
            ))
        }
        _ => {
            *transform_blend = None;
            None
        }
    };
    let mut mirrored = [0.0f32; ANIM_TOTAL];
    for (i, node) in anims.nodes.iter().enumerate() {
        // A pivot clip comes in quickly too: Jazz starts turning with it at once, and
        // the standing pose would slide its feet round meanwhile.
        let r = if is_additive(i) || (ANIM_PIVOT..ANIM_TWIST).contains(&i) {
            30.0
        } else {
            rate
        };
        let blend = (dt * r).min(1.0);
        match blend_in {
            Some((clip, _, to)) if i == clip => weights[i] = to,
            Some((_, from, to)) if !is_additive(i) => {
                if from < 1.0 {
                    weights[i] *= (1.0 - to) / (1.0 - from);
                }
            }
            _ => weights[i] += (target[i] - weights[i]) * blend,
        }
        // (The transformation's never mirrored: the car's half of it isn't.)
        let k = if is_additive(i) {
            left
        } else if is_to_vehicle(i) || is_to_robot(i) {
            0.0
        } else {
            player.mirror
        };
        mirrored[mirror_clip(i)] = weights[i] * k;
        if let Some(active) = animation_player.animation_mut(*node) {
            active.set_weight(weights[i] * (1.0 - k));
        }
    }
    if let Some(nodes) = &anims.mirrored {
        for (i, node) in nodes.iter().enumerate() {
            if let Some(active) = animation_player.animation_mut(*node) {
                active.set_weight(mirrored[i]);
            }
        }
    }
    if let Some(clips) = &anims.downed {
        blend_downed(clips, dt, animation_player, player, downed, downed_pose);
    }
    // A support item's use (equipment.rs: TnAbility's OnTriggerAnimParams): its clip
    // over his upper body, blended in over its first TRIGGER_BLEND_IN and out over
    // its last TRIGGER_BLEND_OUT; given the weight that makes it that share of what
    // his upper body plays (its bones are his own clips' too), his legs going on as
    // they were. Not while he changes form or falls.
    let mut trigger_share = 0.0;
    if let Some((node, secs)) = anims.trigger
        && let Some(t) = player.support.trigger_anim
    {
        let robot = player.form == Form::Robot && player.transform_left <= 0.0;
        if t >= secs || dead || !robot {
            player.support.trigger_anim = None;
            animation_player.stop(node);
        } else {
            if t == 0.0 {
                animation_player.play(node).replay();
            }
            let share = ((t / TRIGGER_BLEND_IN).min(1.0)
                * ((secs - t) / TRIGGER_BLEND_OUT).min(1.0))
            .clamp(0.0, TRIGGER_MOST);
            trigger_share = share;
            let others: f32 = (0..ANIM_TOTAL)
                .filter(|&i| !is_additive(i))
                .map(|i| weights[i])
                .sum();
            if let Some(active) = animation_player.animation_mut(node) {
                active.set_weight(others.max(0.1) * share / (1.0 - share));
            }
            player.support.trigger_anim = Some(t + dt);
        }
    }
    let others: f32 = (0..ANIM_TOTAL)
        .filter(|&i| !is_additive(i))
        .map(|i| weights[i])
        .sum();
    ability_clips(
        anims,
        dt,
        animation_player,
        player,
        (&mut *ability, *hips_back),
        others,
    );
    melee_react_clips(anims, animation_player, player);
    // The aim offset node (robot_aim.rs: the game's, over his clips): his upper
    // body and gun arm to where he aims, from where he faces (left of it is the
    // twist's positive, the game's aim's negative) and level, a quarter turn each
    // way; its profile his stance's. On in his robot form, standing (not down: the
    // downed aim poses aim him there); the moves that play in the custom slots over
    // it in the game's tree (a blow, a throw, a hit reaction) and the clip over his
    // upper body (a support item's use, the Whirlwind's spin, the shield's break or
    // bash: not the Heal Beam's, on his other arm) taking it off by their share.
    if let Some(node) = anims.aim.as_ref() {
        let on =
            player.form == Form::Robot && player.transform_left <= 0.0 && !dead && !downed_pose;
        let over = [ANIM_MELEE, ANIM_MELEE_BOOST, ANIM_THROW, ANIM_HIT]
            .iter()
            .map(|&i| weights[i])
            .sum::<f32>()
            .min(1.0);
        let overlay = ability
            .overlay
            .filter(|(role, _)| *role != "heal_beam")
            .map_or(0.0, |(_, share)| share);
        let share = f32::from(u8::from(on))
            * (1.0 - over)
            * (1.0 - trigger_share)
            * (1.0 - overlay.clamp(0.0, 1.0));
        let want = Vec2::new(-player.twist, player.aim_pitch) / FRAC_PI_2;
        let mirror = if anims.mirrored.is_some() {
            player.mirror
        } else {
            0.0
        };
        state_aim.update(
            node,
            want,
            game_stance(player, in_hand.heavy),
            (share, left > 0.0, mirror),
            dt,
        );
    }
}

/// The class abilities' clips (robot_extras.glb's, ABILITY_ROLES) for a Jazz: the
/// Flak Shield's stance by how he moves (its walk and jog blended by his speed, in
/// the ten directions his own strafe uses, its idle and turns standing) and the
/// hover's pose, as far as they have taken over (animate_jazz eased them in); and the
/// clip over his upper body or arm (abilities.rs's ClassAbility::overlay: the
/// Whirlwind's spin, the Heal Beam's arm, the shield's break), its weight the share
/// of what plays there that the trigger clip's is (`others`: his own clips' weight).
fn ability_clips(
    anims: &RobotAnimations,
    dt: f32,
    animation_player: &mut AnimationPlayer,
    player: &Player,
    (state, hips_back): (&mut AbilityAnim, bool),
    others: f32,
) {
    if anims.ability.is_empty() {
        return;
    }
    let node = |role: &str| {
        anims
            .ability
            .iter()
            .find(|(r, _, _)| *r == role)
            .map(|(_, n, s)| (*n, *s))
    };
    // The shield's stance: each of its clips' share.
    let mut want: Vec<(&'static str, f32)> = Vec::new();
    if state.shield > 1e-3 {
        let input = player.move_input;
        if input.length() < 0.1 {
            let role = match player.pivot {
                Some(p) if p.turn > 0.0 => "shield_pivot_l",
                Some(p) if p.turn < 0.0 => "shield_pivot_r",
                _ => "shield_idle",
            };
            want.push((role, 1.0));
        } else {
            let jog = ((player.velocity.length() - SHIELD_WALK_SPEED)
                / (SHIELD_JOG_SPEED - SHIELD_WALK_SPEED))
                .clamp(0.0, 1.0);
            let angle = input.x.atan2(input.y).to_degrees();
            let dirs: [(f32, &str); 5] = if hips_back {
                [
                    (90.0, "rb"),
                    (135.0, "br"),
                    (180.0, "b"),
                    (225.0, "bl"),
                    (270.0, "lb"),
                ]
            } else {
                [
                    (-90.0, "l"),
                    (-45.0, "fl"),
                    (0.0, "f"),
                    (45.0, "fr"),
                    (90.0, "r"),
                ]
            };
            let at = if hips_back {
                let a = if angle < 0.0 { angle + 360.0 } else { angle };
                a.clamp(90.0, 270.0)
            } else {
                angle.clamp(-90.0, 90.0)
            };
            for pair in dirs.windows(2) {
                let (a0, d0) = pair[0];
                let (a1, d1) = pair[1];
                if at >= a0 && at <= a1 {
                    let k = (at - a0) / (a1 - a0);
                    for (d, w) in [(d0, 1.0 - k), (d1, k)] {
                        for (gait, g) in [("jog", jog), ("walk", 1.0 - jog)] {
                            let role = ABILITY_ROLES
                                .iter()
                                .find(|r| **r == format!("shield_{gait}_{d}"))
                                .copied();
                            if let Some(role) = role {
                                want.push((role, w * g));
                            }
                        }
                    }
                    break;
                }
            }
        }
    }
    // (Each eases to its share of the stance; those it leaves fade out.)
    let ease = (dt * 10.0).min(1.0);
    for (role, w) in &mut state.shield_clips {
        let to = want
            .iter()
            .find(|(r, _)| r == role)
            .map_or(0.0, |(_, w)| *w);
        *w += (to - *w) * ease;
    }
    for (role, w) in &want {
        if !state.shield_clips.iter().any(|(r, _)| r == role) {
            state.shield_clips.push((role, *w * ease));
        }
    }
    state.shield_clips.retain(|(role, w)| {
        let keep = *w > 1e-3 || want.iter().any(|(r, _)| r == role);
        if !keep && let Some((n, _)) = node(role) {
            animation_player.stop(n);
        }
        keep
    });
    for (role, w) in &state.shield_clips {
        if let Some((n, _)) = node(role) {
            if !animation_player.is_playing_animation(n) {
                animation_player.play(n).repeat();
            }
            if let Some(active) = animation_player.animation_mut(n) {
                active.set_weight(w * state.shield);
            }
        }
    }
    // The hover's pose.
    if let Some((n, _)) = node(ABILITY_HOVER) {
        if state.hover > 1e-3 {
            if !animation_player.is_playing_animation(n) {
                animation_player.play(n).repeat();
            }
            if let Some(active) = animation_player.animation_mut(n) {
                active.set_weight(state.hover);
            }
        } else if animation_player.is_playing_animation(n) {
            animation_player.stop(n);
        }
    }
    // The clip over his upper body or arm: blended in and out (the ability's
    // BlendInTime and BlendOutTime), held each frame at the time the ability's clock
    // puts it at (the Whirlwind's spin goes round from its LoopOffsetTime, not its
    // start: whirlwind::clip_time; playing on by itself, the clip went back to its
    // wind-up each time round, the Titan standing still for most of a second between
    // turns while his trails, whooshes and sweeps carried on).
    let wanted = player.ability.overlay();
    if let Some((old, _)) = state.overlay
        && wanted.is_none_or(|(role, _, _)| role != old)
        && let Some((n, _)) = node(old)
    {
        animation_player.stop(n);
        state.overlay = None;
    }
    if let Some((role, t, share)) = wanted
        && let Some((n, secs)) = node(role)
    {
        state.overlay = Some((role, share));
        let share = share.clamp(0.0, TRIGGER_MOST);
        animation_player
            .play(n)
            .set_speed(0.0)
            .seek_to(overlay_time(t, secs))
            .set_weight(others.max(0.1) * share / (1.0 - share));
    }
}

/// His reactions to melee blows (hitreact.rs): each clip held at its time, added
/// over his upper body at its weight (blending in, and out as it completes or is
/// cut short); those not playing stopped.
fn melee_react_clips(
    anims: &RobotAnimations,
    animation_player: &mut AnimationPlayer,
    player: &Player,
) {
    let Some(nodes) = anims.melee_react else {
        return;
    };
    for ((node, secs), now) in nodes.into_iter().zip(player.melee_react.clips()) {
        match now {
            Some((t, w)) if w > 0.0 => {
                animation_player
                    .play(node)
                    .set_speed(0.0)
                    .seek_to(t.clamp(0.0, (secs - 1.0 / 30.0 - 1e-3).max(0.0)))
                    .set_weight(w);
            }
            _ => {
                if animation_player.is_playing_animation(node) {
                    animation_player.stop(node);
                }
            }
        }
    }
}

/// Where an ability's clip over his upper body is held for its clock's time `t`
/// (Ability::overlay's: the Whirlwind's already round and round its spin, the Heal
/// Beam's arm round its loop): within the clip, `secs` long, round again past its
/// end, and never at its very end (a clip seeked there wraps round to its start).
fn overlay_time(t: f32, secs: f32) -> f32 {
    let secs = secs.max(1e-3);
    // (A time a hair past its end is its end: robot_extras.txt keeps a clip's
    // length to four figures, Skill_Heal's 1.0333 s as 1.033, and the Heal Beam's
    // clip_time goes on to its true end.)
    let t = if (secs..secs + 1e-3).contains(&t) {
        secs
    } else {
        t.rem_euclid(secs)
    };
    t.min((secs - 1e-3).max(0.0))
}

/// The downed clips (DOWNED_ROLES) for where a Jazz is in revive.rs's states, each
/// held at the time his state's clock puts it at (from his snapshot, for another
/// player's Jazz): going down, Revive_Fall; lying, the aim poses blended by where
/// he aims (the game's AnimNodeAimOffset: the four poses round the aim, a pose a
/// quarter turn from ahead, left positive, and from level), or, turning over where
/// he lies, the turn's clip; getting up, Revive_Getup; reviving another,
/// Revive_Heal. Their weights ease in and out as his own clips' do (the base blend
/// shares the pose out between them), each one going holding its last time.
fn blend_downed(
    clips: &[(AnimationNodeIndex, f32); DOWNED_CLIPS],
    dt: f32,
    animation_player: &mut AnimationPlayer,
    player: &Player,
    blend: &mut [(f32, f32); DOWNED_CLIPS],
    on: bool,
) {
    let r = &player.revive;
    let mut target = [None::<f32>; DOWNED_CLIPS];
    let mut weight = [0.0f32; DOWNED_CLIPS];
    let mut play = |i: usize, t: f32, w: f32| {
        target[i] = Some(t);
        weight[i] = w;
    };
    if on {
        if let Some(d) = r.downed {
            if d.age < clips[DOWNED_FALL].1 {
                play(DOWNED_FALL, d.age, 1.0);
            } else if let Some(turn) = d.pivot {
                let i = if turn.left {
                    DOWNED_PIVOT_L
                } else {
                    DOWNED_PIVOT_R
                };
                play(i, turn.t, 1.0);
            } else {
                // Across (ahead, left, right) and up (level, down, up): each pose's
                // share of the four round the aim.
                let x = (player.twist / DOWNED_AIM_REACH).clamp(-1.0, 1.0);
                let y = (player.aim_pitch / DOWNED_AIM_REACH).clamp(-1.0, 1.0);
                let across = [1.0 - x.abs(), x.max(0.0), (-x).max(0.0)];
                let up = [1.0 - y.abs(), (-y).max(0.0), y.max(0.0)];
                for (c, a) in across.iter().enumerate() {
                    for (v, u) in up.iter().enumerate() {
                        if a * u > 0.0 {
                            play(DOWNED_AIM + c * 3 + v, 0.0, a * u);
                        }
                    }
                }
            }
        } else if let Some(t) = r.getting_up {
            play(DOWNED_GETUP, t, 1.0);
        } else if let Some(h) = r.healing {
            play(DOWNED_HEAL, h.t, 1.0);
        }
    }
    let k = (dt * 10.0).min(1.0);
    for (i, &(node, secs)) in clips.iter().enumerate() {
        let (w, t) = &mut blend[i];
        if let Some(at) = target[i] {
            // (Held just short of its end: a clip seeked to its very end wraps round
            // to its start.)
            *t = at.clamp(0.0, (secs - 1e-3).max(0.0));
        }
        *w += (weight[i] - *w) * k;
        if *w > 1e-3 {
            animation_player
                .play(node)
                .set_speed(0.0)
                .seek_to(*t)
                .set_weight(*w);
        } else if animation_player.is_playing_animation(node) {
            *w = 0.0;
            animation_player.stop(node);
        }
    }
}

/// The support items' use's clip's blend in and out (OnTriggerAnimParams'
/// BlendInTime and BlendOutTime), and the most of his upper body it takes (all but
/// a trace: its weight is finite).
const TRIGGER_BLEND_IN: f32 = 0.2;
const TRIGGER_BLEND_OUT: f32 = 0.4;
const TRIGGER_MOST: f32 = 0.995;

fn start_action(player: &mut Player, clip: usize, secs: f32, speed: f32) {
    player.action = Some((clip, secs));
    player.start_clip = Some((clip, speed));
}

/// When a charging punch's blow lands, it strikes every enemy in reach in front of
/// Jazz.
fn melee_hits(
    time: Res<Time>,
    mut players: Query<(Entity, &Transform, &mut Player), Without<RemotePlayer>>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    mut sfx: ResMut<Sfx>,
    mut barrels: ResMut<barrels::Barrels>,
) {
    for (jazz, player_transform, mut player) in &mut players {
        let Some(left) = player.melee_hit_in else {
            continue;
        };
        let left = left - time.delta_secs();
        if left > 0.0 {
            player.melee_hit_in = Some(left);
            continue;
        }
        player.melee_hit_in = None;

        let chest = player_transform.translation + Vec3::Y * 2.2;
        let forward = *player_transform.forward();
        player
            .played
            .push(net::JazzEvent::Melee { at: chest, forward });
        if melee_blow(
            &mut enemies,
            (&mut sfx, &mut barrels),
            (chest, forward),
            jazz,
        ) {
            struck_by_melee(&mut player);
        }
    }
}

/// A charging punch's blow from `at` along `forward` by `jazz` (his, or another
/// player's as his PC reports it): the sprint attack's Damage (150,
/// TnDamageTypeMelee, MELEE_SPRINT_DAMAGE) and HitMomentum (MELEE_MOMENTUM) on
/// every live enemy whose body is in reach in front of him (and an exploding barrel
/// there: barrels.rs). Whether it did any damage.
fn melee_blow(
    enemies: &mut Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    (sfx, barrels): (&mut Sfx, &mut barrels::Barrels),
    (at, forward): (Vec3, Vec3),
    jazz: Entity,
) -> bool {
    let mut struck_any = false;
    let in_reach: Vec<usize> = barrels
        .standing_ones()
        .filter(|&b| {
            let near = barrels.data.spots[b].shape.nearest(at);
            barrels.reach(b, at) < MELEE_RANGE - 0.9
                && (near - at).normalize_or_zero().dot(forward) > 0.2
        })
        .collect();
    for b in in_reach {
        barrels.strike(
            b,
            MELEE_SPRINT_DAMAGE,
            "TnDamageTypeMelee",
            barrels::By::Player(jazz),
        );
        sfx.play_at(SND_MELEE_HIT, barrels.data.spots[b].shape.nearest(at));
        struck_any = true;
    }
    for (_, enemy_transform, mut enemy) in enemies.iter_mut() {
        if !enemy.alive() {
            continue;
        }
        let centre = enemy.centre();
        let to_enemy = centre - at;
        // (MELEE_RANGE was to a drone's middle, 0.9 m in from its surface.)
        if enemy.reach(at) < MELEE_RANGE - 0.9 && to_enemy.normalize_or_zero().dot(forward) > 0.2 {
            let struck =
                at + to_enemy.clamp_length_max((to_enemy.length() - enemy.radius() * 0.5).max(0.0));
            struck_any |= enemy.take(
                MELEE_SPRINT_DAMAGE,
                Some(jazz),
                xp::Blow::Melee,
                melee_hit(struck, enemy_transform.translation - at, forward),
            ) > 0.0;
            sfx.play_at(SND_MELEE_HIT, centre);
        }
    }
    struck_any
}

/// A sword swing's blow (melee.rs's Sweep), as the game's sweep strikes: its class's
/// box (melee.txt's: ahead of him from his MeleeSocket_SmallRobot, to the side his
/// gun is on) through its time in the swing, striking each live enemy whose body it
/// meets once, for the attack's Damage (75, TnDamageTypeMelee) and HitMomentum
/// (MELEE_MOMENTUM: enough to knock a soldier off its feet). A local Jazz's
/// follows him by his swing's clock and is told to the other PCs as it begins
/// (JazzEvent::Sweep); another player's is where his PC said, for as long as his
/// class's lasts.
fn melee_sweeps(
    time: Res<Time>,
    data: Res<melee::MeleeData>,
    mut players: Query<(Entity, &Transform, &mut Player, Has<RemotePlayer>)>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    mut sfx: ResMut<Sfx>,
    mut barrels: ResMut<barrels::Barrels>,
) {
    let dt = time.delta_secs();
    for (jazz, transform, mut player, remote) in &mut players {
        let player = &mut *player;
        let swing = data.swing(player.kit.class);
        let scale = player.kit.robot.scale;
        let sweep_box = if remote {
            let Some((centre, heading, since)) = player.melee_sweep.given else {
                continue;
            };
            player.melee_sweep.given =
                (since + dt < swing.sweep_secs).then_some((centre, heading, since + dt));
            melee::SweepBox {
                centre,
                rotation: Quat::from_rotation_y(heading),
                half: swing.half,
            }
        } else {
            let Some((ANIM_MELEE, left)) = player.action else {
                continue;
            };
            // (Its time in the swing; at least once, should a long frame step over it.)
            let t = player.kit.robot.melee_secs - left;
            let begun = player.melee_sweep.begun;
            if t < swing.sweep_at || (begun && t >= swing.sweep_at + swing.sweep_secs) {
                continue;
            }
            let turn = Quat::from_rotation_y(player.heading);
            let b = swing.sweep_box(transform.translation, turn, scale, player.left_handed);
            if !begun {
                player.melee_sweep.begun = true;
                player.played.push(net::JazzEvent::Sweep {
                    centre: b.centre,
                    heading: player.heading,
                });
            }
            b
        };
        // (Struck from his chest: the box's socket's height.)
        let at = transform.translation + Vec3::Y * (swing.socket.y * scale);
        let forward = sweep_box.rotation * Vec3::NEG_Z;
        // An exploding barrel the box meets, once a swing (barrels.rs).
        let met: Vec<usize> = barrels
            .standing_ones()
            .filter(|&b| {
                let shape = barrels.data.spots[b].shape;
                shape.reach(sweep_box.nearest(shape.centre())) <= 1e-3
            })
            .collect();
        for b in met {
            if player.melee_sweep.barrels.contains(&b) {
                continue;
            }
            player.melee_sweep.barrels.push(b);
            barrels.strike(
                b,
                MELEE_DAMAGE,
                "TnDamageTypeMelee",
                barrels::By::Player(jazz),
            );
            let near = barrels.data.spots[b].shape.nearest(at);
            sfx.play_at(SND_MELEE_HIT, near);
            if !remote {
                struck_by_melee(player);
            }
        }
        for (entity, enemy_transform, mut enemy) in enemies.iter_mut() {
            if !enemy.alive() || player.melee_sweep.struck.contains(&entity) {
                continue;
            }
            let centre = enemy.centre();
            if enemy.reach(sweep_box.nearest(centre)) > 1e-3 {
                continue;
            }
            player.melee_sweep.struck.push(entity);
            let to_enemy = centre - at;
            let struck =
                at + to_enemy.clamp_length_max((to_enemy.length() - enemy.radius() * 0.5).max(0.0));
            let dealt = enemy.take(
                MELEE_DAMAGE,
                Some(jazz),
                xp::Blow::Melee,
                melee_hit(
                    struck,
                    enemy_transform.translation - transform.translation,
                    forward,
                ),
            );
            sfx.play_at(SND_MELEE_HIT, centre);
            if !remote {
                // (His blow has met it: he stops, his lunge with him: ProcessMeleeHit's
                // DisableMotionOnContact.)
                player.melee_contact = true;
                if let Some(lunge) = player.lunge.as_mut() {
                    lunge.reached = true;
                }
                if dealt > 0.0 {
                    struck_by_melee(player);
                }
            }
        }
    }
}

/// The striker's own blow landing (TnPlayerController.ClientNotifyCausedDamage, for
/// damage he has done: TnDamageTypeMelee's AttackerCameraShakeParams and
/// AttackerCameraEffect): his view shakes, and energon splashes across his screen.
fn struck_by_melee(player: &mut Player) {
    player.shake = Some(MELEE_SHAKE);
    player.screen_struck = true;
}

/// TnDamageTypeMelee's AttackerCameraShakeParams: Intensity 1.25, Roll 1 over 0.35 s,
/// falling off over 0.4 s.
const MELEE_SHAKE: CameraShake = CameraShake {
    intensity: 1.25,
    roll: 1.0,
    roll_secs: 0.35,
    secs: 0.4,
};

/// TnDamageTypeMelee's AttackerCameraEffect, the energon splashed across the
/// striker's screen (FX_Screen_p's Screen_EnergonBloodsplatter_FX: drips and
/// splats that bend the picture behind them as they run).
const SCREEN_SPLATTER: &str = "Screen_EnergonBloodsplatter_FX";
/// TnCamera's EmitterDistance (85 uu) and DisableDuplicateEmitterDelay.
const SCREEN_FX_DISTANCE: f32 = 0.85;
const SCREEN_FX_REPEAT: f32 = 0.4;

/// The effects on this PC's players' screens (TnCamera's Emitters), each showing
/// with when it started (by seat: split screen's guest has his own).
#[derive(Resource, Default)]
struct ScreenEffects {
    live: [Vec<(crate::levelfx::FxId, f32)>; split::SEATS],
}

/// The screen's effects (TnCamera.SpawnEmitter and UpdateEmitters): started
/// EmitterDistance in front of the camera, facing it (SCREEN_FX_DISTANCE), unless
/// the same is still showing from less than DisableDuplicateEmitterDelay ago
/// (IsPlayingEffect), and kept there as the camera moves.
fn screen_effects(
    time: Res<Time>,
    mut screen: ResMut<ScreenEffects>,
    fx: Option<ResMut<crate::levelfx::FxSystems>>,
    cameras: Query<(&Transform, &split::Seat), With<OrbitCamera>>,
    mut players: Query<(&mut Player, &split::Seat), With<LocalPlayer>>,
) {
    let Some(mut fx) = fx else {
        for (mut player, _) in &mut players {
            player.screen_struck = false;
        }
        return;
    };
    let now = time.elapsed_secs();
    for live in &mut screen.live {
        live.retain(|(id, _)| fx.alive(*id));
    }
    // (Each of this PC's players' in front of his own camera.)
    for (mut player, seat) in &mut players {
        let struck = std::mem::take(&mut player.screen_struck);
        let Some((camera, _)) = cameras.iter().find(|(_, s)| *s == seat) else {
            continue;
        };
        let live = &mut screen.live[seat.0.min(split::SEATS - 1)];
        let ahead = camera.translation + *camera.forward() * SCREEN_FX_DISTANCE;
        let facing = camera.translation - ahead;
        if struck
            && !live.iter().any(|(_, at)| now - at <= SCREEN_FX_REPEAT)
            && let Some(id) = fx.spawn(SCREEN_SPLATTER, ahead, facing, &[])
        {
            // (On his screen alone: split screen's other view doesn't show it.)
            fx.see_only(id, seat.0);
            live.push((id, now));
        }
        for (id, _) in live.iter() {
            fx.place(*id, ahead, facing, Vec3::ZERO);
        }
    }
}

fn animate_transform_pop(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Transform, &mut TransformPop, &BaseScale)>,
) {
    for (entity, mut transform, mut pop, base) in &mut query {
        pop.0.tick(time.delta());
        let t = pop.0.fraction();
        if t >= 1.0 {
            transform.scale = Vec3::splat(base.0);
            commands.entity(entity).remove::<TransformPop>();
        } else {
            // Settle in from slightly smaller with a small overshoot.
            let s = 0.85 + 0.15 * t + 0.06 * (t * PI).sin();
            transform.scale = Vec3::splat(s * base.0);
        }
    }
}

/// Where Jazz faces in robot form. On the move he faces `camera_yaw`: where the
/// camera looks as he strafes, or, sprinting, the way he runs. Standing still, his
/// feet stay put while the camera turns and his upper body follows it
/// (animate_robot), until it has turned PIVOT_START from his facing: then he steps
/// round with one of the game's pivot clips, turning as the clip does.
fn face_camera(player: &mut Player, camera_yaw: f32, swing: f32, input: Vec2, free: bool, dt: f32) {
    let standing = free
        && player.grounded
        && input.length() < 0.1
        && player.action.is_none()
        && player.dash_left <= 0.0;
    if !standing {
        player.pivot = None;
        let off = angle_to(player.heading, camera_yaw);
        player.heading += off * (1.0 - (-FACE_CAMERA_RATE * dt).exp());
    } else if let Some(mut pivot) = player.pivot {
        pivot.t += dt * pivot.speed;
        let k = (pivot.t / pivot.secs).min(1.0);
        player.heading = pivot.from + pivot.turn * k;
        player.pivot = (k < 1.0).then_some(pivot);
    }
    let off = angle_to(player.heading, camera_yaw);
    let settled = swing.abs() < PIVOT_WAIT_SWING || off.abs() > PIVOT_180_START;
    if standing && player.pivot.is_none() && off.abs() > PIVOT_START && settled {
        let stance = stance(player);
        // (A heavy weapon's stance has its own turns: as the shooting stance's,
        // 90 and 180 degrees each way.)
        let heavy = stance > 0 && WEAPONS.get(player.weapon).is_some_and(|w| w.heavy);
        let side = usize::from(off < 0.0);
        let (clip, turn, secs) = if (stance == 1 || heavy) && off.abs() > PIVOT_180_START {
            // (Left-handed, each turn is the other side's clip mirrored.)
            let turn = match (side == 0, player.left_handed) {
                (true, false) => PIVOT_180_TURN_L,
                (false, false) => PIVOT_180_TURN_R,
                (true, true) => -PIVOT_180_TURN_R,
                (false, true) => -PIVOT_180_TURN_L,
            };
            let first = if heavy {
                ANIM_HEAVY_PIVOT_180
            } else {
                ANIM_PIVOT_180
            };
            (first + side, turn, PIVOT_180_SECS)
        } else {
            let clip = if heavy {
                ANIM_HEAVY_PIVOT + side
            } else {
                ANIM_PIVOT + stance * 2 + side
            };
            (clip, PIVOT_TURN.copysign(off), PIVOT_SECS)
        };
        // Quicker when the camera has swung well past the turn.
        let speed = (off.abs() / turn.abs()).clamp(1.0, 1.5);
        player.pivot = Some(Pivot {
            clip,
            from: player.heading,
            turn,
            secs,
            t: 0.0,
            speed,
        });
        player.start_clip = Some((clip, speed));
    }
    player.heading = player.heading.rem_euclid(TAU);
    player.twist = angle_to(player.heading, camera_yaw);
}

/// The cylinder a jet or a tank meets the map with (its hull's spheres, as high as
/// its box reaches over him, hovering `hover_height` up): radius and height.
fn hull_cylinder(h: &character::Hull, hover_height: f32) -> (f32, f32) {
    let height = (hover_height + h.top).max(0.4);
    ((height * 0.5).min(h.half_width.max(0.2)), height)
}

/// Moves every Jazz from his input: running, jumping, dashing and melee in robot
/// form, hovering and driving in the car; then walls, floors and falls. (Melee looks
/// for the enemy under his crosshair to lunge at, and a lunge follows its enemy:
/// melee.rs.)
#[allow(clippy::type_complexity)]
fn player_movement(
    time: Res<Time>,
    colliders: Res<Colliders>,
    level: Res<Level>,
    kinds: Res<enemies::Kinds>,
    mut players: Query<
        (
            &mut Transform,
            &mut Player,
            &mut car::Car,
            &PlayerInput,
            &PlayerSlot,
        ),
        Without<RemotePlayer>,
    >,
    enemies: Query<(Entity, &Transform, &Enemy, &enemies::Foe), Without<Player>>,
) {
    let dt = time.delta_secs();
    let assist = |e: Entity, t: &Transform, enemy: &Enemy, foe: &enemies::Foe| {
        let kind = kinds.kinds.get(foe.kind);
        melee::AssistTarget {
            entity: e,
            aim: enemy.centre(),
            middle: t.translation + Vec3::Y * kind.map_or(1.0, |k| k.half_height),
            radius: kind.map_or(0.5, |k| k.radius),
            pick: melee::assist_radius(kind.map_or(enemies::Form::Robot, |k| k.form)),
        }
    };
    for (mut transform, mut player, mut car, keys, slot) in &mut players {
        // (The enemy under his crosshair as he swings, if he does.)
        let pick = if keys.melee && player.form == Form::Robot {
            let targets: Vec<melee::AssistTarget> = enemies
                .iter()
                .filter(|(_, _, enemy, _)| enemy.alive())
                .map(|(e, t, enemy, foe)| assist(e, t, enemy, foe))
                .collect();
            let me = player.centre(transform.translation);
            let facing = Quat::from_rotation_y(player.heading) * Vec3::NEG_Z;
            melee::assist_target(
                (keys.view_origin, keys.view_dir),
                (me, facing),
                &targets,
                |from, to| {
                    let way = to - from;
                    level
                        .shot_hit(&colliders, from, way.normalize_or_zero(), way.length())
                        .is_some_and(|wall| wall < way.length() - 0.05)
                },
            )
        } else {
            None
        };
        // (A lunge flies at where its enemy is now.)
        if let Some(lunge) = player.lunge.as_mut()
            && let Some(target) = lunge.target
            && let Ok((e, t, enemy, foe)) = enemies.get(target)
        {
            let now = assist(e, t, enemy, foe);
            lunge.toward = now.middle;
            lunge.radius = now.radius;
        }
        move_jazz(
            dt,
            &colliders,
            &level,
            &mut transform,
            &mut player,
            &mut car,
            (keys, pick),
            level.start(slot.0),
        );
    }
}

/// player_movement for one Jazz (`keys` is what his player asks for; `pick` the
/// enemy his sword swing lunges at, should he swing).
#[allow(clippy::too_many_arguments)]
fn move_jazz(
    dt: f32,
    colliders: &Colliders,
    level: &Level,
    transform: &mut Transform,
    player: &mut Player,
    car: &mut car::Car,
    (keys, pick): (&PlayerInput, Option<melee::AssistTarget>),
    start: Vec3,
) {
    // Fallen limp, he stays where he fell: his body is the ragdoll's (downed.rs),
    // which his robot's bones are posed from.
    if player.limp {
        player.move_input = Vec2::ZERO;
        player.velocity = Vec3::ZERO;
        player.vertical_velocity = 0.0;
        player.sprinting = false;
        player.aiming = false;
        return;
    }
    let mut input = keys.movement;
    let boost = keys.boost;
    // Down, getting up or reviving another (revive.rs), he can't move at all. Nor
    // while he transforms, but for the robot unfolding out of his vehicle, which is
    // the player's to move from the start (see where the unfolding keeps his speed,
    // below).
    let locked = player.revive.locked();
    let unfolding = player.transform_left > 0.0
        && player.form == Form::Robot
        && is_to_robot(player.transform_clip);
    if (player.transform_left > 0.0 && !unfolding) || player.dead_left > 0.0 || locked {
        input = Vec2::ZERO;
    }
    if locked {
        player.velocity = Vec3::ZERO;
        player.dash_vel = Vec3::ZERO;
        player.dash_left = 0.0;
        player.sprinting = false;
    }
    player.move_input = input;
    player.aim_pitch = keys.view_dir.y.clamp(-1.0, 1.0).asin();
    // Fine aim: hold the right mouse button (robot form only), with a gun that fine
    // aims; with one that does something else with it (the Corrosive Slime Cannon
    // shows its round's path), that instead.
    let alt = WEAPONS
        .get(player.weapon)
        .map_or(AltFire::FineAim, |w| w.alt);
    let held = keys.aim
        && player.form == Form::Robot
        && player.transform_left <= 0.0
        && player.dead_left <= 0.0
        && player.hand_switch.is_none();
    player.aiming = held && alt == AltFire::FineAim;
    player.gear.alt = held && alt != AltFire::FineAim;
    // Jazz can't sprint while aiming or shooting; the car boosts regardless.
    let sprint = match player.form {
        Form::Robot => boost && !player.aiming && !player.firing,
        Form::Vehicle => boost,
    };
    player.sprinting = sprint;
    // Sprinting forward (or forward and to a side) in robot form.
    let running = player.form == Form::Robot && sprint_run(sprint, input);

    // Count down the current one-off move.
    if let Some((clip, left)) = player.action {
        let left = left - dt;
        player.action = if left > 0.0 { Some((clip, left)) } else { None };
    }
    let in_melee = matches!(player.action, Some((ANIM_MELEE | ANIM_MELEE_BOOST, _)));
    // (A lunge is its swing's: over with it.)
    if !matches!(player.action, Some((ANIM_MELEE, _))) {
        player.lunge = None;
        player.melee_contact = false;
    }
    let was_airborne = !player.grounded;
    if was_airborne {
        player.air_time += dt;
        if let Some(t) = player.jump_time.as_mut() {
            *t += dt;
        }
    }

    let horizontal = match player.form {
        Form::Robot => {
            // Strafe relative to where the camera looks, always face the aim direction.
            let yaw_rot = Quat::from_rotation_y(keys.yaw);
            let forward = yaw_rot * Vec3::NEG_Z;
            let right = yaw_rot * Vec3::X;
            let dir = (forward * input.y + right * input.x).normalize_or_zero();
            let free = player.transform_left <= 0.0 && player.dead_left <= 0.0 && !locked;
            // Sprinting, he turns to the way he runs; otherwise he faces the camera's way.
            // (Down, as he lies: revive.rs.)
            let facing = if running {
                keys.yaw + run_turn(input)
            } else {
                keys.yaw
            };
            if locked {
                revive::face_downed(player, keys.yaw, dt);
            } else {
                face_camera(player, facing, keys.swing, input, free, dt);
            }
            if keys.jump && player.grounded && free && !in_melee {
                // A jump ends a dodge on the ground: he leaves it at his run's speed
                // and falls as from any jump, and can't dodge again till he lands.
                // (DodgingOnGround is one of OnTheGroundBase's states, which jump;
                // its EndState clamps his speed to GetMaxSpeedAfterDodge, the
                // ground's; _LandedSinceLastDodge stays false.)
                if player.dodge && player.dash_left > 0.0 {
                    player.dash_left = 0.0;
                    if matches!(
                        player.action,
                        Some((ANIM_BOOST_F | ANIM_BOOST_B | ANIM_BOOST_L | ANIM_BOOST_R, _))
                    ) {
                        player.action = None;
                    }
                }
                player.vertical_velocity = (2.0 * -GRAVITY * JUMP_HEIGHT).sqrt();
                player.grounded = false;
                player.jump_time = Some(0.0);
                player.start_clip = Some((ANIM_JUMP, 1.0));
                // (Its take-off clip's light shake.)
                player.shake = Some(JUMP_SHAKE);
            }

            // Dodge one of four ways from where he faces (DASH_SPEED): on the ground
            // once the last one is over, in the air once until he lands.
            if keys.dash && !player.dodged && free && !in_melee {
                let local = if input.y.abs() >= input.x.abs() {
                    if input.y < 0.0 { Vec2::NEG_Y } else { Vec2::Y }
                } else if input.x < 0.0 {
                    Vec2::NEG_X
                } else {
                    Vec2::X
                };
                let facing = Quat::from_rotation_y(player.heading);
                let world = facing * Vec3::NEG_Z * local.y + facing * Vec3::X * local.x;
                player.dash_vel = world * DASH_SPEED;
                player.dash_left = DASH_TIME;
                player.dash_total = DASH_TIME;
                player.dodge = true;
                player.dodged = true;
                // (It ends a reaction to a melee blow: hitreact.rs.)
                player.melee_react.stop(hitreact::DODGE_STOP);
                player.air_dodge = !player.grounded;
                if player.grounded {
                    let clip = if local.y >= local.x.abs() {
                        ANIM_BOOST_F
                    } else if local.y <= -local.x.abs() {
                        ANIM_BOOST_B
                    } else if local.x < 0.0 {
                        ANIM_BOOST_L
                    } else {
                        ANIM_BOOST_R
                    };
                    start_action(player, clip, DASH_ACTION_SECS, DASH_ANIM_SPEED);
                } else {
                    player.vertical_velocity = 0.0;
                    let clip = if local.y >= local.x.abs() {
                        ANIM_AIR_BOOST_F
                    } else if local.y <= -local.x.abs() {
                        ANIM_AIR_BOOST_B
                    } else if local.x < 0.0 {
                        ANIM_AIR_BOOST_L
                    } else {
                        ANIM_AIR_BOOST_R
                    };
                    start_action(player, clip, DASH_ACTION_SECS, AIR_DASH_ANIM_SPEED);
                }
            }

            // Melee: a charging punch when sprinting (lunging the way he runs),
            // otherwise his class's sword swing at its own rate (its blow:
            // melee_sweeps). (Not while he spins his Whirlwind; a blow gives a cloak
            // away and lowers a shield: abilities.rs.)
            if keys.melee
                && player.grounded
                && free
                && !in_melee
                && !player.ability.blocks_attacks()
            {
                abilities::attacked(player);
                // (It cuts a reaction to a melee blow short: hitreact.rs.)
                player.melee_react.stop(hitreact::MELEE_STOP);
                if running {
                    player.dash_vel = dir * MELEE_LUNGE_SPEED;
                    player.dash_left = MELEE_LUNGE_TIME;
                    player.dash_total = MELEE_LUNGE_TIME;
                    player.dodge = false;
                    start_action(player, ANIM_MELEE_BOOST, MELEE_BOOST_SECS, MELEE_ANIM_SPEED);
                    player.melee_hit_in = Some(MELEE_HIT_DELAY);
                } else {
                    let secs = player.kit.robot.melee_secs;
                    start_action(player, ANIM_MELEE, secs, 1.0);
                    player.melee_sweep = melee::Sweep::default();
                    // His lunge (melee.rs): at the enemy under his crosshair, or with
                    // none straight ahead, the way he looks (his yaw: his body
                    // turns to it as he swings).
                    let ahead = Vec3::new(keys.view_dir.x, 0.0, keys.view_dir.z);
                    let ahead = if ahead.length_squared() > 1e-4 {
                        ahead
                    } else {
                        Quat::from_rotation_y(player.heading) * Vec3::NEG_Z
                    };
                    player.lunge = Some(melee::Lunge::new(pick, ahead));
                    player.melee_contact = false;
                }
            }

            // Jazz barely moves while swinging, and not at all once his swing has
            // met an enemy.
            let locked = matches!(player.action, Some((ANIM_LAND_HARD | ANIM_DEATH, _)));
            let slow = if locked || (in_melee && player.melee_contact) {
                0.0
            } else if in_melee {
                0.25
            } else {
                1.0
            };
            // (Fine aiming in the fine-aim stance he walks; in another, he jogs.)
            let speed = if player.aiming && stance(player) == 2 {
                FINE_AIM_SPEED
            } else if running {
                ROBOT_SPRINT
            } else {
                ROBOT_SPEED
            };
            // (Faster for a while after an EMP grenade with Speed Redirector struck,
            // and with the Intercooler's buff on him.)
            let boost = if player.speed_boost > 0.0 {
                emp::SPEED_BOOST
            } else {
                1.0
            } * player.support.speed_scale();
            // (Faster with the Subsonic Repeater in hand, slower while the Scatter
            // Blaster, the X18 Scrapmaker or the TechVolt fires: their
            // GroundSpeedMultiplier and FiringGroundSpeedMultiplier.)
            // (An upgrade speeds him while it fires: the TechVolt's Zero-G Casing, the
            // Gear Shredder's Tachyon Transmitter while it charges.)
            let gun = WEAPONS.get(player.weapon).map_or(1.0, |w| {
                let m = player.mods(player.weapon);
                w.ground_speed
                    * if player.firing {
                        w.firing_speed * m.firing_speed * m.charging_speed
                    } else {
                        1.0
                    }
            });
            // (A raised Flak Shield slows him, the Whirlwind's Flurry of Blows speeds
            // him: abilities.rs.)
            let multiplier = boost * gun * abilities::speed_scale(player);
            let run = dir * speed * slow * multiplier;
            // His sword swing's lunge: flying at its enemy (or straight ahead), at
            // its speed times his speed multipliers, till it reaches it or its time
            // is out; then he stops dead (melee.rs).
            match player.lunge {
                Some(mut lunge) if lunge.left > 0.0 => {
                    lunge.left -= dt;
                    let me = player.centre(transform.translation);
                    let facing = Quat::from_rotation_y(player.heading) * Vec3::NEG_Z;
                    let air_speed = melee::DASH_SPEED * melee::SWING_SPEED_MULTIPLIER * multiplier;
                    let velocity = if lunge.reached {
                        None
                    } else {
                        melee::lunge_velocity(&lunge, me, facing, dir, player.size().0, air_speed)
                    };
                    if velocity.is_none() && !lunge.reached {
                        lunge.reached = true;
                        player.melee_contact = true;
                    }
                    player.lunge = Some(lunge);
                    let velocity = velocity.unwrap_or(Vec3::ZERO);
                    player.vertical_velocity = velocity.y;
                    Vec3::new(velocity.x, 0.0, velocity.z)
                }
                _ => run,
            }
        }
        // A jet hovers and flies, gravity and all its own (jet.rs).
        Form::Vehicle if player.kit.plane.is_some() => {
            let free = player.transform_left <= 0.0 && player.dead_left <= 0.0;
            car.boost_cooldown = (car.boost_cooldown - dt).max(0.0);
            let velocity = jet::fly(player, car, keys, free, dt);
            player.vertical_velocity = velocity.y;
            Vec3::new(velocity.x, 0.0, velocity.z)
        }
        Form::Vehicle => {
            let free = player.transform_left <= 0.0 && player.dead_left <= 0.0;
            // The car hovers; holding Speed Boost drops it onto its wheels, unless a
            // crash has just knocked it out of boosting (car.rs animates both). A
            // tank only hovers: boosting, it flies straight ahead faster, and it
            // can't start to while it drifts, after boosting or firing its cannon
            // (TnTankForm.UpdateBoosting).
            car.boost_cooldown = (car.boost_cooldown - dt).max(0.0);
            let tank = player.kit.tank;
            let drifting = tank && car.drift > 0.0;
            let boosting = free && boost && car.boost_cooldown <= 0.0 && !drifting;
            if !boosting && !car.hover {
                // Off the wheels: the hover car drifts for a moment.
                car.drift = player.kit.handling.drift_secs;
            }
            car.hover = !boosting;
            // Jump (Space), 0.3 s after landing at the soonest: the hover car's
            // JumpLinearSpeed, or the car physics' JumpLinearVelocity.
            if player.grounded {
                car.jump_wait = (car.jump_wait - dt).max(0.0);
            }
            if keys.jump && player.grounded && free && car.jump_wait <= 0.0 {
                // (Its vehicle's: Kit::handling. A tank's jump is the same boosting:
                // TnHoverTankSimulation.Jump.)
                let handling = player.kit.handling;
                if car.hover || tank {
                    player.vertical_velocity = handling.hover_jump_speed;
                } else {
                    player.vertical_velocity = handling.jump_speed;
                    player.drive_speed += handling.jump_forward;
                }
                player.grounded = false;
                car.jumped = true;
                car.in_jump = Some(car.hover);
                car.jump_wait = car::TIME_BETWEEN_JUMPS;
                player.played.push(net::JazzEvent::CarJump);
                // (A car's or truck's jump shakes the camera: its JumpingShakeParams,
                // TnCarForm.OnBeginJump.)
                if !tank {
                    player.shake = Some(vehicle_shake(player.kit.drive.jump_shake));
                }
            }
            let handling = player.kit.handling;
            if car.hover {
                player.steer = 0.0;
                hover_velocity(player, car, input, handling.hover_speed, keys.yaw, free, dt)
            } else if tank {
                // (TnHoverTankSimulation.UpdateStrafe: straight ahead, at its boost speed.)
                hover_velocity(player, car, Vec2::Y, handling.max_speed, keys.yaw, free, dt)
            } else {
                // On its wheels the mouse steers it (drive.rs).
                let wheel = if free { keys.wheel } else { 0.0 };
                let velocity = drive::drive_velocity(player, wheel, dt);
                car.hover_vel = velocity;
                velocity
            }
        }
    };

    // The Hover Slam's air movement and gravity (hoverslam.rs).
    let (horizontal, hover_gravity) = match player.form {
        Form::Robot => match hoverslam::movement(player, horizontal) {
            Some((h, g)) => (h, Some(g)),
            None => (horizontal, None),
        },
        Form::Vehicle => (horizontal, None),
    };

    // A Scientist folding up into his jet is in its hands from the start, as the
    // game's is (jet.rs): it hovers, strafes, climbs and flies on his keys while he
    // changes, starting from his own speed, with no falling; out of it, its flight
    // starts over.
    let jet = player.kit.plane.is_some()
        && player.dead_left <= 0.0
        && (player.form == Form::Vehicle
            || (player.transform_left > 0.0 && is_to_vehicle(player.transform_clip)));
    if jet && player.form == Form::Robot {
        if !car.jet.ready {
            car.hover_vel = Vec3::new(
                player.carry_vel.x,
                player.vertical_velocity,
                player.carry_vel.z,
            );
        }
        car.boost_cooldown = (car.boost_cooldown - dt).max(0.0);
        let velocity = jet::fly(player, car, keys, !locked, dt);
        player.carry_vel = Vec3::new(velocity.x, 0.0, velocity.z);
        player.vertical_velocity = velocity.y;
    } else if !jet {
        car.jet.ready = false;
    }
    // While transforming, Jazz keeps the speed he had (slowing down when unfolding).
    // Unfolding, he is in the player's hands too, as the game's robot is from the
    // start of its change (its controller's input is only taken away for a
    // character whose blueprint asks, DisableTransformationControl: TnPlayerPawn.
    // OnBeginTransformation; no class's does): his own move takes over as the
    // vehicle's speed falls to it (a slow one's at once: the Titans' tanks, which
    // stood on the spot while the robot unfolded, over a second).
    let horizontal = if player.transform_left > 0.0 && player.dead_left <= 0.0 {
        if is_to_robot(player.transform_clip) {
            player.carry_vel *= (-UNFOLD_SLOWDOWN * dt).exp();
            if player.form == Form::Robot
                && horizontal.length_squared() > player.carry_vel.length_squared()
            {
                player.carry_vel = horizontal;
            }
        }
        player.carry_vel
    } else {
        horizontal
    };

    // A dodge in place of his run (one begun in the air, level: no gravity while it
    // lasts), or a lunge's burst, fading out.
    let mut horizontal = horizontal;
    // A blow's push (the Guard's blast: abilities.rs's knock): he flies with it
    // till he is down on the ground again.
    if player.knock_vel != Vec3::ZERO {
        if player.grounded && player.vertical_velocity <= 0.0 {
            player.knock_vel = Vec3::ZERO;
        } else {
            horizontal += player.knock_vel;
        }
    }
    // (Flying too while his sword's lunge lasts: TnFlier's PHYS_Flying.)
    let flying = (player.dodge && player.air_dodge && player.dash_left > 0.0 && !player.grounded)
        || player.lunge.is_some_and(|l| l.left > 0.0);
    if player.dash_left > 0.0 {
        if player.dodge {
            horizontal = player.dash_vel;
        } else {
            horizontal += player.dash_vel * (player.dash_left / player.dash_total).clamp(0.0, 1.0);
        }
        player.dash_left -= dt;
    }

    // (A jet or a tank meets the map with its rigid body: its hull's spheres, as high
    // as its box reaches over him. Push out of walls, a ceiling and the floor by
    // them, not by its circles' width.)
    let hull = player
        .kit
        .hull
        .filter(|_| player.form == Form::Vehicle && (jet || player.kit.tank));
    let (radius, height) = match &hull {
        Some(h) => hull_cylinder(h, player.kit.handling.hover_height),
        None => player.size(),
    };
    let old = transform.translation;
    // (A vehicle in the air from its jump falls more gently: its JumpingGravityScale.)
    if player.grounded {
        car.in_jump = None;
    }
    // (A car or truck falls at the rigid bodies' gravity, drive.rs's.)
    let body = if player.kit.wheeled() {
        drive::VEHICLE_GRAVITY / GRAVITY
    } else {
        1.0
    };
    let gravity = match (player.form, car.in_jump) {
        // (A jet has none: TnPlaneForm.OnActivate's SetGravityFactor(0); nor has a
        // robot dodging in the air: PHYS_Flying.)
        _ if jet || flying => 0.0,
        (Form::Vehicle, Some(true)) => body * player.kit.handling.hover_jump_gravity,
        (Form::Vehicle, Some(false)) => body * player.kit.handling.jump_gravity,
        (Form::Vehicle, None) => body,
        _ => hover_gravity.unwrap_or(1.0),
    };
    player.vertical_velocity += GRAVITY * gravity * dt;
    let mut pos = old + horizontal * dt;
    pos.y += player.vertical_velocity * dt;

    // Walls: slide along them. Hitting one in the car kills most of your speed.
    let push = match player.form {
        Form::Robot => level.push_out(pos, radius, height),
        Form::Vehicle => {
            let forward = Quat::from_rotation_y(player.heading) * Vec3::NEG_Z;
            match &hull {
                Some(h) => level.push_out_hull(pos, forward, h, player.kit.handling.hover_height),
                None => level.push_out_car(pos, forward, height, player.kit.body),
            }
        }
    };
    if let Some(push) = push {
        pos.x += push.x;
        pos.z += push.y;
        if jet {
            // (A jet loses its speed into it, and crashes out of flying: jet.rs.)
            let normal = Vec3::new(push.x, 0.0, push.y).normalize_or_zero();
            let speed = (-car.hover_vel.dot(normal)).max(0.0);
            jet::struck(player, car, normal, speed);
        } else if push.length() > 0.05 {
            player.drive_speed *= 0.6;
            player.strafe_speed *= 0.6;
            car_hit_obstacle(player, car, push);
        }
    }
    // (A jet climbing: the ceiling over it, the lids over the map too.)
    if jet && player.vertical_velocity > 0.0 {
        let head = old.y + height;
        if let Some(lid) = level.ceiling(pos, radius, head - 0.05, pos.y + height + 0.05) {
            jet::struck(player, car, Vec3::NEG_Y, player.vertical_velocity);
            player.vertical_velocity = 0.0;
            pos.y = (lid - height - 0.01).min(pos.y).max(old.y.min(pos.y));
        }
    }
    // Head bump on ceilings when jumping.
    if player.vertical_velocity > 0.0 {
        let head = old + Vec3::Y * (height - 0.5);
        if level
            .ray(head, Vec3::Y, player.vertical_velocity * dt + 0.6)
            .is_some()
        {
            if jet {
                jet::struck(player, car, Vec3::NEG_Y, player.vertical_velocity);
            }
            player.vertical_velocity = 0.0;
            pos.y = pos.y.min(old.y);
        }
    }

    // Floor: land when falling onto it; while walking, follow steps and slopes
    // up to STEP_HEIGHT up or down.
    let floor = if player.vertical_velocity <= 0.0 {
        // (A jet stays where it hovers: it doesn't follow the ground down.)
        let snap_down = if player.grounded && !jet {
            STEP_HEIGHT
        } else {
            0.0
        };
        level.ground_moving(
            pos,
            radius,
            old.y.max(pos.y) + STEP_HEIGHT,
            pos.y.min(old.y) - snap_down - 0.05,
            old.y,
        )
    } else {
        None
    };
    // The top of the jump or fall (from where Jazz left the ground).
    player.air_peak = if was_airborne {
        player.air_peak.max(pos.y)
    } else {
        pos.y
    };
    if let Some(floor_y) = floor.filter(|y| *y >= pos.y - STEP_HEIGHT - 0.05) {
        pos.y = floor_y;
        if jet && player.vertical_velocity < 0.0 {
            jet::struck(player, car, Vec3::Y, -player.vertical_velocity);
        }
        player.vertical_velocity = 0.0;
        player.grounded = true;
        // (A car or truck landing after half a second in the air shakes the camera:
        // its LandingShakeParams, TnCarForm.UpdateInAir's OnLandComplete.)
        if was_airborne
            && player.form == Form::Vehicle
            && player.kit.wheeled()
            && player.air_time > car::LANDING_AIR_SECS
            && player.dead_left <= 0.0
        {
            player.shake = Some(vehicle_shake(player.kit.drive.land_shake));
        }
        let fall = player.air_peak - floor_y;
        let speed = player.velocity.length();
        // (No landing move while a mid-air transformation is still playing.)
        if was_airborne
            && player.form == Form::Robot
            && player.dead_left <= 0.0
            && player.transform_left <= 0.0
        {
            // The landing's shake, whether or not he is moving off (the game plays
            // the landing clip, its notify with it, either way; not swinging his
            // sword, TnAcrobaticsManager.CanPlayLandingAnimation, and not as a
            // Hover Slam lands: hoverslam.rs shakes it its own way).
            if !in_melee
                && player.ability.hover.phase != hoverslam::Phase::Slamming
                && let Some(shake) = landing_shake(fall, speed)
            {
                player.shake = Some(shake);
            }
            let air_dashing = matches!(
                player.action,
                Some((
                    ANIM_AIR_BOOST_L | ANIM_AIR_BOOST_R | ANIM_AIR_BOOST_F | ANIM_AIR_BOOST_B,
                    _
                ))
            );
            let hard =
                (fall >= HARD_LAND_FALL && speed >= HARD_LAND_SPEED) || fall >= HARD_LAND_FALL_ANY;
            if hard && (player.action.is_none() || air_dashing) {
                // Long fall: heavy landing, Jazz can't move until he recovers.
                start_action(player, ANIM_LAND_HARD, LAND_HARD_SECS, LAND_ANIM_SPEED);
            } else if fall >= LAND_FALL
                && player.action.is_none()
                && player.move_input.length() < 0.1
            {
                // Light landing, unless already moving off.
                start_action(player, ANIM_LAND, LAND_SECS, LAND_ANIM_SPEED);
            }
        }
        player.air_time = 0.0;
        player.jump_time = None;
        // (On the ground with the dodge over: OnTheGroundBase.BeginState.)
        if !(player.dodge && player.dash_left > 0.0) {
            player.dodged = false;
        }
    } else {
        player.grounded = false;
    }

    // Where he is put back if he falls out of the map: where he stood a moment
    // before he last left the ground.
    if player.grounded {
        player.ground_secs += dt;
        if player.ground_secs >= SAFE_GROUND_SECS {
            player.ground_secs -= SAFE_GROUND_SECS;
            player.safe_spot = player.safe_next.or(Some(pos));
            player.safe_next = Some(pos);
        }
    } else {
        player.ground_secs = 0.0;
    }
    // Fell out of the map (below its KillZ: the game takes him out there,
    // Actor.FellOutOfWorld): back where he stood a moment before (the start if that
    // was far from here).
    if pos.y < level.kill_y.unwrap_or(KILL_Y) {
        let fell = pos;
        pos = player
            .safe_spot
            .filter(|s| s.xz().distance(pos.xz()) < SAFE_SPOT_RANGE)
            .unwrap_or(start);
        info!(
            "Fell out of the map at {fell:.1} (its KillZ {:.1} m): back at {pos:.1}",
            level.kill_y.unwrap_or(KILL_Y)
        );
        player.safe_spot = None;
        player.safe_next = None;
        player.ground_secs = 0.0;
        player.vertical_velocity = 0.0;
        player.drive_speed = 0.0;
        player.strafe_speed = 0.0;
        player.carry_vel = Vec3::ZERO;
        player.knock_vel = Vec3::ZERO;
    }

    // Prop arena: pillars and the arena edge.
    for (centre, r) in &colliders.0 {
        let flat = Vec2::new(pos.x, pos.z);
        let offset = flat - *centre;
        let min = r + radius;
        if offset.length_squared() < min * min && pos.y < PISTON_HEIGHT {
            let push = offset.normalize_or(Vec2::X) * min;
            pos.x = centre.x + push.x;
            pos.z = centre.y + push.y;
            if jet {
                let normal = Vec3::new(offset.x, 0.0, offset.y).normalize_or(Vec3::X);
                let speed = (-car.hover_vel.dot(normal)).max(0.0);
                jet::struck(player, car, normal, speed);
            } else {
                player.drive_speed *= 0.3;
                player.strafe_speed *= 0.3;
                car_hit_obstacle(player, car, offset);
            }
        }
    }
    if level.collision.is_none() {
        let limit = ARENA_HALF_SIZE - radius;
        let side = |v: f32| {
            if v > limit {
                -1.0
            } else if v < -limit {
                1.0
            } else {
                0.0
            }
        };
        let edge = Vec3::new(side(pos.x), 0.0, side(pos.z));
        if edge != Vec3::ZERO {
            if jet {
                let normal = edge.normalize();
                let speed = (-car.hover_vel.dot(normal)).max(0.0);
                jet::struck(player, car, normal, speed);
            } else {
                player.drive_speed *= 0.5;
                player.strafe_speed *= 0.5;
            }
        }
        pos.x = pos.x.clamp(-limit, limit);
        pos.z = pos.z.clamp(-limit, limit);
    }

    if player.form == Form::Vehicle && !car.hover {
        player.wheel_angle += player.drive_speed * dt / WHEEL_RADIUS;
    }
    if dt > 0.0 {
        let v = (pos - old) / dt;
        player.velocity = Vec3::new(v.x, 0.0, v.z).clamp_length_max(VEHICLE_TOP_SPEED);
    }
    transform.translation = pos;
    transform.rotation = Quat::from_rotation_y(player.heading);
}

/// Swap Weapons (the mouse wheel) changes which of the two weapons Jazz carries is
/// in hand; the matching arm mesh is shown and the others hidden.
fn switch_weapon(mut players: Query<(&mut Player, &PlayerInput), Without<RemotePlayer>>) {
    for (mut player, input) in &mut players {
        if !input.swap_weapon {
            continue;
        }
        if player.dead_left <= 0.0 && player.hand_switch.is_none() {
            swap_weapon(&mut player);
        }
    }
}

/// The other weapon he carries to hand.
fn swap_weapon(player: &mut Player) {
    let hand = 1 - player.hand.min(1);
    let old = player.weapon;
    player.hand = hand;
    player.weapon = player.loadout[hand];
    player.burst_left = 0;
    player.burst_queued = false;
    // The weapon's equip time (EquipTime).
    player.fire_cooldown = player
        .fire_cooldown
        .max(WEAPONS[player.weapon.min(WEAPONS.len() - 1)].equip_secs);
    // The new gun unfolds while the old one folds away (animate_robot).
    player.start_equip = Some((player.weapon, Some(old)));
}

/// Going down (revive.rs), a heavy weapon in hand gives way to the other gun he
/// carries (TnPawn's ForceEquipRobotWeapon), as Swap Weapons would bring it.
fn put_away_heavy(player: &mut Player) {
    let other = player.loadout[1 - player.hand.min(1)];
    let heavy = |w: usize| WEAPONS.get(w).is_some_and(|w| w.heavy);
    if heavy(player.weapon) && !heavy(other) {
        swap_weapon(player);
    }
}

/// C moves the gun to Jazz's other hand (TnPlayerController.ChangeHandedness): in
/// robot form, when he isn't dashing, meleeing, transforming or already switching
/// guns or hands (TnStanceManager.CanSwitchHandedness, and the weapon must be ready:
/// TnWeapon.Active). He can't fire or fine aim until the gun is across. Unlike the
/// game, he can switch while sprinting (and keeps sprinting).
fn switch_hands(
    anims: Option<Res<RobotAnimations>>,
    mut players: Query<
        (&Transform, &mut Player, &PlayerInput, Has<LocalPlayer>),
        Without<RemotePlayer>,
    >,
    mut sfx: ResMut<Sfx>,
) {
    if !anims.is_some_and(|a| a.mirrored.is_some()) {
        return;
    }
    for (transform, mut player, input, local) in &mut players {
        if input.switch_hands {
            switch_jazz_hands(
                &mut player,
                &mut sfx,
                voice_at(local, transform.translation),
            );
        }
    }
}

/// switch_hands for one Jazz (his sounds come from `at`, see voice_at).
fn switch_jazz_hands(player: &mut Player, sfx: &mut Sfx, at: Option<Vec3>) {
    let busy = matches!(
        player.action,
        Some((
            ANIM_BOOST_F
                | ANIM_BOOST_B
                | ANIM_BOOST_L
                | ANIM_BOOST_R
                | ANIM_MELEE
                | ANIM_MELEE_BOOST
                | ANIM_AIR_BOOST_F
                | ANIM_AIR_BOOST_B
                | ANIM_AIR_BOOST_L
                | ANIM_AIR_BOOST_R
                | ANIM_DEATH,
            _
        ))
    );
    if player.form != Form::Robot
        || player.transform_left > 0.0
        || player.pending_form.is_some()
        || player.dead_left > 0.0
        || player.dying
        || busy
        || player.hand_switch.is_some()
        || player.start_hand_switch
        || player.reload_left > 0.0
        || player.equipping.is_some()
        || player.unequipping.is_some()
        || player.start_equip.is_some()
        || player.holstered
        || player.holster_change.is_some()
    {
        return;
    }
    player.left_handed = !player.left_handed;
    player.start_hand_switch = true;
    player.burst_left = 0;
    player.burst_queued = false;
    player.fire_cooldown = player.fire_cooldown.max(HAND_SWITCH_SECS);
    let weapon = &WEAPONS[player.weapon.min(WEAPONS.len() - 1)];
    sfx.play_from(weapon.sound_change_hands, at);
}

/// Whether a Jazz's gun is on his left arm this frame (while it moves across, the
/// arm it goes to). (The transformations aren't mirrored: through one, and until
/// the body has blended back over to the mirrored pose after it, the gun is on the
/// right.)
fn gun_on_left(player: &Player) -> bool {
    player.left_handed
        && (player.hand_switch.is_some() || (player.transform_left <= 0.0 && player.mirror > 0.5))
}

/// Shows the guns, forearms and folding guns that should be seen this frame: the
/// gun in hand on its arm (and on the other arm too while it moves across), the
/// gun being put away folding back in on the CloudB bones, and a bare forearm on
/// the arm without a gun. Runs right after animate_robot, so each shows for exactly
/// as long as its clip plays.
#[allow(clippy::type_complexity)]
fn show_weapon_meshes(
    mut commands: Commands,
    players: Query<&Player>,
    guns: Query<(Entity, &WeaponMesh, &JazzOf, Option<&Visibility>)>,
    folding: Query<(Entity, &WeaponMeshCloudB, &JazzOf, Option<&Visibility>), Without<WeaponMesh>>,
    forearms: Query<
        (Entity, &Forearm, &JazzOf, Option<&Visibility>),
        (Without<WeaponMesh>, Without<WeaponMeshCloudB>),
    >,
    forearms_b: Query<(Entity, &ForearmCloudB, &JazzOf, Option<&Visibility>)>,
) {
    // Each Jazz's: the gun on the left arm or not, and whether it is moving across.
    let sides = |owner: Entity| {
        let player = players.get(owner).ok()?;
        Some((player, gun_on_left(player), player.hand_switch.is_some()))
    };
    for (entity, mesh, of, visibility) in &guns {
        let Some((player, left, switching)) = sides(of.0) else {
            continue;
        };
        // (Put away: none, but the copy folding away below.)
        let on = (mesh.1 == left || switching) && !player.holstered;
        if let Some(want) = shown(visibility, mesh.0 == player.weapon && on) {
            commands.entity(entity).insert(want);
        }
    }
    for (entity, mesh, of, visibility) in &folding {
        let Some((player, left, _)) = sides(of.0) else {
            continue;
        };
        let away = player.unequipping.map(|(w, _)| w);
        if let Some(want) = shown(visibility, Some(mesh.0) == away && mesh.1 == left) {
            commands.entity(entity).insert(want);
        }
    }
    // (The gun put away, its arm's forearm out too: unfolding as it goes away,
    // fold_forearms growing its pieces back.)
    for (entity, arm, of, visibility) in &forearms {
        let Some((player, left, switching)) = sides(of.0) else {
            continue;
        };
        let show = !(arm.0 == left || switching) || (player.holstered && !switching);
        if let Some(want) = shown(visibility, show) {
            commands.entity(entity).insert(want);
        }
    }
    // While switching hands, the forearms on their CloudB bones stand in for them
    // (fold_forearms shows and hides their parts); as the gun comes back out, the
    // one on its arm, folding away there.
    for (entity, arm, of, visibility) in &forearms_b {
        let Some((player, left, switching)) = sides(of.0) else {
            continue;
        };
        let coming_back = !player.holstered && player.holster_change.is_some() && arm.0 == left;
        if let Some(want) = shown(visibility, switching || coming_back) {
            commands.entity(entity).insert(want);
        }
    }
}

/// Shows the upgrades' parts (character.rs's UpgradePart) on the guns that show:
/// each with its gun's mesh on its arm (show_weapon_meshes' rules; the copy on the
/// CloudB bones with the gun folding away) while the upgrade is on that Jazz's gun
/// (Player::upgrades, a remote Jazz's from his snapshot).
fn show_upgrade_parts(
    mut commands: Commands,
    players: Query<&Player>,
    parts: Query<(
        Entity,
        &character::UpgradePart,
        &JazzOf,
        Option<&Visibility>,
    )>,
) {
    for (entity, part, of, visibility) in &parts {
        let Ok(player) = players.get(of.0) else {
            continue;
        };
        let switching = player.hand_switch.is_some();
        let left = gun_on_left(player);
        let gun_shows = if part.cloud_b {
            player.unequipping.map(|(w, _)| w) == Some(part.weapon) && part.left == left
        } else {
            part.weapon == player.weapon && (part.left == left || switching) && !player.holstered
        };
        let on = player.upgrades & (1 << part.upgrade) != 0;
        if let Some(want) = shown(visibility, gun_shows && on) {
            commands.entity(entity).insert(want);
        }
    }
}

/// Switching hands, as the game's ADD_Weapon_MirrorReverse_ARM does with its
/// ScaleBone notifies (TR_Iconic_Car_ANM_p.Jazz_Arm_ROBO_ANIM): the forearm on the
/// arm the gun leaves grows back from nothing on its CloudB bones as it unfolds
/// (ANIM_ARM_SWITCH), its wheel last, and the one on the arm the gun goes to shrinks
/// away piece by piece as it folds up, its hand drawing in, until it is gone. Each
/// bone's scale over the second the switch takes is the game's (switch_bone_scale).
/// Otherwise the bones are left at their full size.
fn fold_forearms(
    chars: Res<character::Characters>,
    players: Query<&Player>,
    mut bones: Query<(&SwitchBone, &JazzOf, &mut Transform)>,
) {
    for (bone, of, mut transform) in &mut bones {
        let scale = match players.get(of.0) {
            Ok(player) => match (player.hand_switch, player.kit.arm_set, bone.part) {
                // A character's forearms: their set's ScaleBone notifies (the gun
                // going to the left hand plays them mirrored).
                (Some((_, left)), Some(set), _) => chars.switch_scale(
                    Some(set),
                    &bone.name,
                    player.left_handed,
                    HAND_SWITCH_SECS - left,
                ),
                // Jazz's own: the forearm comes back on the arm the gun leaves (the
                // left one when it moves to the right hand).
                (Some((_, left)), None, Some(part)) => switch_bone_scale(
                    part,
                    bone.left != player.left_handed,
                    HAND_SWITCH_SECS - left,
                ),
                // The gun going away or coming back (holsters): the forearm on its
                // arm unfolding or folding away, by its set's ADD_Weapon_Equip_ARM or
                // Unequip_ARM (mirrored with the gun on the left).
                (None, set, _) => player.holster_change.map_or(1.0, |t| {
                    chars.equip_scale(set, &bone.name, gun_on_left(player), player.holstered, t)
                }),
                _ => 1.0,
            },
            Err(_) => 1.0,
        };
        if transform.scale.x != scale {
            transform.scale = Vec3::splat(scale);
        }
    }
}

/// The scale of a forearm bone `t` seconds into switching hands, on the arm the
/// forearm comes back on (`returning`) or the one it leaves. These are the
/// ScaleBone notifies of ADD_Weapon_MirrorReverse_ARM: each lerps a bone from one
/// size to another (0.001 for gone) over a time, from a moment in the clip.
fn switch_bone_scale(part: SwitchPart, returning: bool, t: f32) -> f32 {
    const GONE: f32 = 0.001;
    // From `from` to `to` over `secs`, starting at `start`.
    let lerp = |start: f32, secs: f32, from: f32, to: f32| {
        let k = ((t - start) / secs).clamp(0.0, 1.0);
        from + (to - from) * k
    };
    if returning {
        match part {
            // The wheel stays away until the forearm has nearly unfolded.
            SwitchPart::Elbow(16) => {
                if t < 0.383 {
                    GONE
                } else {
                    lerp(0.383, 0.3, GONE, 1.0)
                }
            }
            SwitchPart::Elbow(4) | SwitchPart::Shoulder(1) => lerp(0.0, 0.4, GONE, 1.0),
            _ => lerp(0.0, 0.2, GONE, 1.0),
        }
    } else {
        // Its pieces go one after another, then whatever is left at once.
        let rest = lerp(0.524, 0.05, 1.0, GONE);
        match part {
            SwitchPart::Hand => lerp(0.428, 0.15, 1.0, GONE),
            SwitchPart::Elbow(16) => rest.min(lerp(0.275, 0.3, 1.0, GONE)),
            SwitchPart::Elbow(4) => rest.min(lerp(0.311, 0.3, 1.0, GONE)),
            SwitchPart::Elbow(19) => rest.min(lerp(0.336, 0.3, 1.0, GONE)),
            SwitchPart::Shoulder(1) => rest.min(lerp(0.442, 0.3, 1.0, GONE)),
            _ => rest,
        }
    }
}

/// The visibility to give something shown or hidden, if it isn't already.
fn shown(visibility: Option<&Visibility>, show: bool) -> Option<Visibility> {
    let want = if show {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    (visibility != Some(&want)).then_some(want)
}

/// Hovering (TnHoverCarSimulation; a tank's TnHoverTankSimulation): the keys strafe
/// the car relative to the camera, which it faces at once, up to `speed` (its hover
/// speed; a boosting tank's, straight ahead) at its hover acceleration (its
/// vehicle's: Kit::handling), on the ground or in the air. Its grip comes back
/// gradually after the wheels (the drift).
fn hover_velocity(
    player: &mut Player,
    car: &mut car::Car,
    input: Vec2,
    speed: f32,
    yaw: f32,
    free: bool,
    dt: f32,
) -> Vec3 {
    let yaw_rot = Quat::from_rotation_y(yaw);
    let dir = (yaw_rot * Vec3::NEG_Z * input.y + yaw_rot * Vec3::X * input.x).normalize_or_zero();
    // (A wreck no longer turns with the camera.)
    if free {
        player.heading = yaw;
    }
    let handling = player.kit.handling;
    let grip = (1.0 - car.drift / handling.drift_secs.max(0.01))
        .clamp(0.0, 1.0)
        .powi(2);
    car.drift = (car.drift - dt).max(0.0);
    let target = dir * speed;
    let delta = target - car.hover_vel;
    let step = handling.hover_accel * grip * dt;
    car.hover_vel += if delta.length() <= step {
        delta
    } else {
        delta.normalize() * step
    };
    // (So the wheels carry on at the same speed when the car comes down.)
    let facing = Quat::from_rotation_y(player.heading);
    player.drive_speed = car.hover_vel.dot(facing * Vec3::NEG_Z);
    player.strafe_speed = car.hover_vel.dot(facing * Vec3::X);
    player.yaw_rate = 0.0;
    car.hover_vel
}

/// The car ran into a wall or pillar; `push` is the way it was pushed back out. The
/// hovering car loses its speed into the obstacle. Boosting into one within 30
/// degrees of head-on knocks the car off its wheels back to hovering, and it can't
/// boost again for a second (TnCarForm.Driving.OnRigidBodyCollision).
fn car_hit_obstacle(player: &Player, car: &mut car::Car, push: Vec2) {
    if player.form != Form::Vehicle {
        return;
    }
    let normal = Vec3::new(push.x, 0.0, push.y).normalize_or_zero();
    car.hover_vel -= normal * car.hover_vel.dot(normal).min(0.0);
    let forward = Quat::from_rotation_y(player.heading) * Vec3::NEG_Z;
    // (A tank's crashes don't stop its boost: TnTankForm has no Driving state.)
    if !car.hover && !player.kit.tank && normal.dot(forward).abs() > car::BOOST_CRASH_COS {
        car.boost_cooldown = car::BOOST_CRASH_COOLDOWN;
    }
}

/// Jazz's forearms and their wheels (on their sockets, tools/build_assets.py), left
/// (true) and right. The forearm holding the gun is replaced by it, wheel and all.
const FOREARMS: [(&str, bool); 4] = [
    ("RB_Jazz_CHARSKEL_ArmLoL", true),
    ("L_CloudA_ElbowRobo16_XT_Wheel", true),
    ("RB_Jazz_CHARSKEL_ArmLoR", false),
    ("R_CloudA_ElbowRobo16_XT_Wheel", false),
];
/// Their copies on the CloudB arm bones (ForearmCloudB).
const FOREARMS_CLOUD_B: [&str; 4] = [
    "RB_Jazz_CHARSKEL_ArmLoL_CloudB",
    "L_CloudB_ElbowRobo16_XT_Wheel",
    "RB_Jazz_CHARSKEL_ArmLoR_CloudB",
    "R_CloudB_ElbowRobo16_XT_Wheel",
];

fn spin_wheels(
    mut commands: Commands,
    chars: Res<character::Characters>,
    new_names: Query<(Entity, &Name), Added<Name>>,
    new_parts: Query<(Entity, &character::WornPart), Added<character::WornPart>>,
) {
    // A character's forearms and their wheels (character.rs), as Jazz's below.
    for (entity, part) in &new_parts {
        match (part.forearm, part.cloud_b) {
            (Some(left), false) => {
                commands.entity(entity).insert(Forearm(left));
            }
            (Some(left), true) => {
                commands
                    .entity(entity)
                    .insert((ForearmCloudB(left), Visibility::Hidden));
            }
            _ => {}
        }
    }
    for (entity, name) in &new_names {
        let n = name.as_str();
        // Each forearm and its wheel: shown on the arm without the gun.
        if let Some(left) = FOREARMS
            .iter()
            .find(|(name, _)| *name == n)
            .map(|(_, left)| *left)
        {
            commands.entity(entity).insert(Forearm(left));
        }
        if let Some(i) = FOREARMS_CLOUD_B.iter().position(|b| *b == n) {
            // (The left one and its wheel first.)
            commands
                .entity(entity)
                .insert((ForearmCloudB(i < 2), Visibility::Hidden));
        }
        if let Some(bone) = SwitchBone::from_name(n, chars.is_switch_bone(n)) {
            commands.entity(entity).insert(bone);
        }
        // A pickup's outline shell (models/weapons, models/props: PickupOutline, the
        // heavy weapons' PickupOutlineHeavy) glows; it casts no shadow.
        if n.contains(".PickupOutline") {
            commands
                .entity(entity)
                .insert((NotShadowCaster, NotShadowReceiver));
        }
        if n == MUZZLE_BONE || n == MUZZLE_BONE_LEFT {
            commands
                .entity(entity)
                .insert(MuzzleBone(n == MUZZLE_BONE_LEFT));
        }
        if n == GUN_BONE || n == GUN_BONE_LEFT {
            commands.entity(entity).insert(GunBone(n == GUN_BONE_LEFT));
        }
        // The guns' moving parts' bones, which some of their sockets ride.
        if let Some(bone) = arsenal::CloudBone::from_name(n) {
            commands.entity(entity).insert(bone);
        }
        let gun = |n: &str| {
            WEAPONS.iter().enumerate().find_map(|(i, w)| {
                (w.mesh == n)
                    .then_some((i, false))
                    .or((w.mesh_left == n).then_some((i, true)))
            })
        };
        if let Some((i, left)) = gun(n) {
            commands
                .entity(entity)
                .insert((WeaponMesh(i, left), Visibility::Hidden));
        }
        if let Some((i, left)) = n.strip_suffix("_CloudB").and_then(gun) {
            commands
                .entity(entity)
                .insert((WeaponMeshCloudB(i, left), Visibility::Hidden));
        }
    }
}

/// Critically damped smoothing towards `target`, settling in about `smooth_time`.
fn smooth_damp(x: &mut f32, v: &mut f32, target: f32, smooth_time: f32, dt: f32) {
    let omega = 2.0 / smooth_time.max(1e-4);
    let k = omega * dt;
    let decay = 1.0 / (1.0 + k + 0.48 * k * k + 0.235 * k * k * k);
    let change = *x - target;
    let temp = (*v + omega * change) * dt;
    *v = (*v - omega * temp) * decay;
    *x = target + (change + temp) * decay;
}

/// Where a player's camera sits against him: out to the side, up over his feet, back
/// along the view and up in the view's own axes (rig_offsets' Vec4: a jet's camera
/// anchor is over it in the view's axes), as his class's cameras have it
/// (character::Camera and VehicleCamera: Jazz's are the game's CAM_Strategies_p.
/// OverTheShoulder_STRATEGY and CAM_Driving_Strategies_p.Car_Jazz_STRATEGY, the
/// Destroyer's its Truck_MP_CAMSET's; in a vehicle, its Escalation character's camera
/// set's: character::VehicleCamera). On foot: out as far as it orbits (Jazz's
/// 5.5 m), aimed at a point to the side of him (`shoulder`: 1 right, -1 left), ahead
/// and over the middle of his body as its offsets have it for the view's pitch
/// (Jazz's 1.65 m to the side, 1 m ahead and 1.5-1.95 m up, higher as the view comes
/// up from looking down), both as `robot` has them (robot_cam's frame: sprinting,
/// moving to the sprint's), moving in to the scope's position as `scope` goes from 0
/// to 1. In the vehicle: as far out and as high over its middle as its camera has
/// it, driving or hovering (`hover`, 0 to 1: Jazz's car 5 m out and 1.5 m up either
/// way), the middle the game's pawn is at (VehicleCamera::middle: the middle of its
/// mesh's bounds, which may be ahead of its origin; half its height for a class
/// built before it). A jet's camera is behind its anchor over the jet's middle, the
/// anchor that far up in the view's axes (HmOffsetAnchorPointRelativeToActorCamera-
/// Behavior's OffsetUsesCameraRotation; flying, TnFlyingFollowCameraBehavior's, out
/// to the side of a turn: `fly_anchor`).
fn rig_offsets(
    player: &Player,
    car: &car::Car,
    hover: f32,
    shoulder: f32,
    scope: f32,
    fly_anchor: Vec3,
    robot: robot_cam::Frame,
) -> Vec4 {
    if player.form == Form::Vehicle && player.kit.plane.is_some() {
        let camera = &player.kit.vehicle_camera;
        let h = hover.clamp(0.0, 1.0);
        let anchor = fly_anchor.lerp(Vec3::Y * camera.hover_up, h);
        let back = camera.drive_distance + (camera.hover_distance - camera.drive_distance) * h;
        let (up, ahead) = camera.middle.unwrap_or((0.0, 0.0));
        return Vec4::new(anchor.x, up + car.raised(), back - ahead, anchor.y);
    }
    let (side, height, back) = match player.form {
        Form::Robot => {
            let camera = &player.kit.camera;
            // (Its aim for the view's pitch and its orbit, sprinting moved toward the
            // sprint's: robot_cam.rs.)
            let aim = robot.aim;
            let s = scope.clamp(0.0, 1.0);
            let side = aim.y * shoulder;
            let half = player.kit.robot.height * 0.5;
            let height = half + aim.z;
            let back = robot.distance - aim.x;
            (
                side + (camera.scope.x * shoulder - side) * s,
                height + (half + camera.scope.y - height) * s,
                back + (camera.scope.z - back) * s,
            )
        }
        // (Following the body up when it hovers; over a tall vehicle, Snarl's,
        // higher.)
        Form::Vehicle => {
            let camera = &player.kit.vehicle_camera;
            let h = hover.clamp(0.0, 1.0);
            let up = camera.drive_up + (camera.hover_up - camera.drive_up) * h;
            let back = camera.drive_distance + (camera.hover_distance - camera.drive_distance) * h;
            let (middle, ahead) = camera
                .middle
                .unwrap_or((VEHICLE_HEIGHT.max(player.kit.vehicle_height) * 0.5, 0.0));
            (0.0, middle + up + car.raised(), back - ahead)
        }
    };
    Vec4::new(side, height, back, 0.0)
}

/// The camera at `offsets` (rig_offsets') from a player at `at`, looking the way
/// `yaw` and `pitch` say, kept on his side of walls and ceilings: its position and
/// rotation.
fn camera_rig(level: &Level, at: Vec3, (yaw, pitch): (f32, f32), offsets: Vec4) -> (Vec3, Quat) {
    let (side, height, back) = (offsets.x, offsets.y, offsets.z);
    let rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    let target = at + Vec3::Y * height + rotation * Vec3::Y * offsets.w;
    let mut position = target + rotation * Vec3::new(side, 0.0, back);
    if level.collision.is_some() {
        // Keep the camera on Jazz's side of walls and ceilings.
        let offset = position - target;
        let dist = offset.length();
        if dist > 0.01 {
            let dir = offset / dist;
            if let Some(hit) = level.ray(target, dir, dist + 0.6) {
                position = target + dir * (hit - 0.6).max(0.3);
            }
        }
    } else {
        position.y = position.y.max(0.5);
    }
    (position, rotation)
}

/// Before the match has begun for this player (Choose Character, the countdown:
/// pregame.rs) the view is the level camera's, not his (whose camera keeps
/// following his waiting Jazz meanwhile, unseen).
fn level_camera_view(
    pre: Res<pregame::PreMatch>,
    level: Res<Level>,
    split: Res<split::Split>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<OrbitCamera>>,
) {
    if !pre.waiting {
        return;
    }
    let (at, turn, fov) = level.pre_match_view();
    for (mut transform, mut projection) in &mut cameras {
        *transform = Transform::from_translation(at).with_rotation(turn);
        if let Projection::Perspective(perspective) = &mut *projection {
            // (Across, as the game's; Bevy's is up and down.)
            let aspect = split::fov_aspect(perspective.aspect_ratio.max(0.1), split.active());
            perspective.fov = 2.0 * ((fov.to_radians() * 0.5).tan() / aspect).atan();
        }
    }
}

/// A Jazz not in the match yet (this PC's before the countdown runs out, another
/// player's whose snapshot says he is waiting) isn't seen.
fn hide_waiting_jazz(
    pre: Res<pregame::PreMatch>,
    mut players: Query<(&mut Player, &mut Visibility, Has<LocalPlayer>)>,
) {
    for (mut player, mut visibility, local) in &mut players {
        if local && player.waiting != pre.waiting {
            player.waiting = pre.waiting;
        }
        visibility.set_if_neq(if player.waiting {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
}

/// The local player's camera: the mouse turns it (as fast as Mouse Sensitivity
/// says, its up and down the other way round with Robot or Vehicle Camera
/// Inverted), or the controller's right stick (pad.rs), it follows him and zooms
/// for fine aim, and it gives him his view to aim along. (Where it heads and pitches
/// is where he looks; read_local_input passes that on the next frame, as Jazz has
/// always turned with the camera a frame behind.) A car's or truck's camera is its
/// strategies' (vehicle_cam.rs): boosting, the mouse steers it and the view follows
/// it round, the picture blurring as it moves.
///
/// Each of this PC's players has his own (split screen, split.rs: the guest's
/// turned by his controller, the mouse only ever the first player's).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn camera_follow(
    time: Res<Time>,
    motion: Res<AccumulatedMouseMotion>,
    (controls, pads, split): (Res<Controls>, pad::Pads, Res<split::Split>),
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    level: Res<Level>,
    mut cameras: Query<
        (
            &split::Seat,
            &mut Transform,
            &mut OrbitCamera,
            &mut Projection,
            Option<&mut MotionBlur>,
        ),
        Without<Player>,
    >,
    mut players: Query<
        (
            &split::Seat,
            &Transform,
            &mut Player,
            &car::Car,
            &mut PlayerInput,
            &PlayerSlot,
        ),
        With<LocalPlayer>,
    >,
    (spectate, others): (
        split::BySeatRef<revive::Spectate>,
        Query<(&PlayerSlot, &Transform, &Player), (With<RemotePlayer>, Without<LocalPlayer>)>,
    ),
) {
    // Whom a player out till the round ends can watch: the others online and this
    // PC's other player (split screen).
    let mut watchable: Vec<Watched> = others
        .iter()
        .map(|(s, t, p)| Watched::of(s.0, t, p))
        .collect();
    watchable.extend(
        players
            .iter()
            .map(|(_, t, p, _, _, s)| Watched::of(s.0, t, p)),
    );
    for (seat, cam_transform, orbit, projection, blur) in &mut cameras {
        let Some((_, player_transform, player, car, input, _)) =
            players.iter_mut().find(|(s, ..)| *s == seat)
        else {
            continue;
        };
        let first = seat.0 == 0;
        let ctx = Follow {
            dt: time.delta_secs(),
            mouse: if first { motion.delta } else { Vec2::ZERO },
            grabbed: first && cursor.grab_mode != CursorGrabMode::None,
            controls: &controls,
            pad: pads.seat(seat.0),
            split: split.active(),
            level: &level,
        };
        follow_view(
            &ctx,
            (cam_transform, orbit, projection, blur),
            (player_transform, player, car, input),
            (spectate.seat(seat.0), &watchable),
        );
    }
}

/// A player a dead one's camera can watch (spectating): his slot, where he is, and
/// where he looks.
#[derive(Clone, Copy)]
struct Watched {
    slot: usize,
    at: Vec3,
    yaw: f32,
    pitch: f32,
}

impl Watched {
    fn of(slot: usize, t: &Transform, p: &Player) -> Self {
        Watched {
            slot,
            at: t.translation,
            yaw: p.heading + p.twist,
            pitch: p.aim_pitch,
        }
    }
}

/// What a player's camera follows him by this frame: the frame's time, the mouse's
/// move (none for the guest), whether the game has the mouse, the controls, his
/// controller, whether the window is split, and the map.
struct Follow<'a> {
    dt: f32,
    mouse: Vec2,
    grabbed: bool,
    controls: &'a Controls,
    pad: &'a pad::Pad,
    split: bool,
    level: &'a Level,
}

/// One player's camera following him (camera_follow).
#[allow(clippy::type_complexity)]
fn follow_view(
    ctx: &Follow,
    (mut cam_transform, mut orbit, mut projection, blur): (
        Mut<Transform>,
        Mut<OrbitCamera>,
        Mut<Projection>,
        Option<Mut<MotionBlur>>,
    ),
    (player_transform, mut player, car, mut input): (
        &Transform,
        Mut<Player>,
        &car::Car,
        Mut<PlayerInput>,
    ),
    (spectate, watchable): (&revive::Spectate, &[Watched]),
) {
    let (controls, pad, level) = (ctx.controls, ctx.pad, ctx.level);
    // Dead (revive.rs): after being down, his death's camera (DEATH_*), held still
    // till he spectates; back from the dead, his own camera at once.
    let r = &player.revive;
    let dead = player.dead_left > 0.0 || player.dying || r.out;
    let spectating = r.out && r.dead_for >= revive::DEATH_VIEW_SECS;
    let died_down = dead && r.died_downed;
    let held = died_down && !spectating;
    let came_back = orbit.was_dead && !dead;
    orbit.was_dead = dead;

    let in_hand = WEAPONS
        .get(player.weapon)
        .filter(|_| player.form == Form::Robot);
    // (A jet's cameras have their own: hovering 80 degrees eased in over half a
    // second, flying 90 over a second, TnFovCameraBehavior's.)
    let jet = player
        .kit
        .plane
        .filter(|_| player.form == Form::Vehicle && player.dead_left <= 0.0);
    let flying = jet.is_some() && !car.hover;
    // A car's or truck's camera strategies (driving, hovering).
    let wheeled = vehicle_cam::strategies(&player.kit).filter(|_| player.form == Form::Vehicle);
    // (Boosting on its wheels the mouse steers it rather than turning the view.)
    let steering = wheeled.is_some() && !car.hover;
    let scoped = player.aiming && in_hand.is_some_and(|w| w.scope_camera);
    let (scope_target, scope_secs) = if scoped {
        (1.0, SCOPE_IN_SECS)
    } else {
        (0.0, SCOPE_OUT_SECS)
    };
    let dt = ctx.dt;
    // Sprinting on foot, the camera is in the sprint's camera state (robot_cam.rs:
    // the game's TnPCS_Sprinting).
    let sprint_camera = player.form == Form::Robot
        && !dead
        && player.revive.downed.is_none()
        && !player.aiming
        && sprint_run(player.sprinting, player.move_input);
    // Spinning (the Whirlwind), in its camera state (TnPCS_Whirlwind: robot_cam.rs).
    let whirl_camera = player.form == Form::Robot
        && !dead
        && player.ability.kind == abilities::Kind::Whirlwind
        && player.ability.active;
    let o = &mut *orbit;
    smooth_damp(
        &mut o.scope,
        &mut o.scope_vel,
        scope_target,
        scope_secs * 0.5,
        dt,
    );
    // A shot's recoil: the view kicks up and the field of view changes for a moment,
    // both easing in (RECOIL_EASE) rather than jumping. The kick still to come is
    // an error that settles to zero; what it closes this frame turns the view.
    let (left, speed) = orbit.kick;
    let left = left - std::mem::take(&mut player.camera_kick);
    let (now_left, speed) = settle(left, speed, RECOIL_EASE, dt);
    // (The view's pitch range: a jet's cameras' own, hovering and flying; eased into
    // a narrower one rather than cut to it. A car's or truck's keeps its own:
    // vehicle_cam.rs.)
    let (lo, hi) = match jet {
        Some(plane) => {
            let (lo, hi) = if flying {
                plane.fly_pitch_range
            } else {
                plane.hover_pitch_range
            };
            (lo.to_radians(), hi.to_radians())
        }
        // (On foot its strategy's, sprinting its sprint's, blending from one to the
        // other: robot_cam.rs.)
        None if player.form == Form::Robot => {
            orbit
                .robot
                .pitch_range(&player.kit.camera, ROBOT_PITCH_RANGE, sprint_camera, dt)
        }
        // (A tank's: its camera's, from its body's own pitch, so a tank can't aim
        // down at the ground beside it.)
        None if player.form == Form::Vehicle && player.kit.tank => {
            let ahead = (player_transform.rotation * car.tilt()) * Vec3::NEG_Z;
            let body = ahead.y.clamp(-1.0, 1.0).asin();
            (body + TANK_PITCH_RANGE.0, body + TANK_PITCH_RANGE.1)
        }
        None => VEHICLE_PITCH_RANGE,
    };
    let ease = if jet.is_some() {
        1.5 * dt
    } else {
        f32::INFINITY
    };
    let clamp_pitch = |p: f32| {
        if p > hi {
            (p - ease).max(hi)
        } else if p < lo {
            (p + ease).min(lo)
        } else {
            p
        }
    };
    orbit.pitch = clamp_pitch(orbit.pitch + now_left - left);
    orbit.kick = (now_left, speed);

    let grabbed = ctx.grabbed;
    // (As the game picks it by form: TnPlayerController.UpdateInvertMouseBy-
    // VehicleForm gives a jet the Jet Camera's, GetInvertY_Plane.)
    let inverted = match player.form {
        Form::Robot => controls.invert[0],
        Form::Vehicle if player.kit.plane.is_some() => controls.invert[2],
        Form::Vehicle => controls.invert[1],
    };
    if pad.active {
        // The controller's right stick turns it instead, as fast as its camera
        // strategy's look speeds say for a stick (pad.rs's StickLook), its up and
        // down the other way round Inverted; flying a jet, it is the flying
        // camera's stick (view_turn: the turn that settles at its full speed).
        let stick = Vec2::new(
            pad.right.x,
            if inverted { -pad.right.y } else { pad.right.y },
        );
        let (speeds, ramp_secs) = pad_look(&player);
        let turn = orbit
            .stick
            .turn(stick, speeds, ramp_secs, controls.camera_sensitivity, dt);
        orbit.mouse_rate = match jet {
            Some(plane) if flying => Vec2::new(
                stick.x * plane.turn_speed / (1.0 + plane.turn_damping),
                stick.y * plane.pitch_speed / (1.0 + plane.pitch_damping),
            ),
            _ => Vec2::ZERO,
        };
        if !flying && !held {
            if !steering {
                orbit.yaw -= turn.x.to_radians();
            }
            orbit.pitch = clamp_pitch(orbit.pitch + turn.y.to_radians());
        }
        // On its wheels a car or truck steers by the stick across as it is
        // (PlayerInCarForm.SetLocalInputs: GetNormalizedTurn).
        orbit.vcam.steer = if wheeled.is_some() { pad.right.x } else { 0.0 };
    } else if grabbed {
        orbit.stick.rest();
        // Slower turning while zoomed in, so aiming stays steady.
        let sensitivity = 0.003 * controls.mouse_scale() * orbit.fov / CAMERA_FOV;
        let rise = if inverted { -ctx.mouse.y } else { ctx.mouse.y };
        // (Flying a jet, the mouse sets how fast the view should turn and the
        // flying camera turns it: jet.rs's view_turn.)
        let mut turn = Vec2::new(-ctx.mouse.x, -rise) * sensitivity;
        orbit.mouse_rate = if dt > 0.0 {
            Vec2::new(-turn.x.to_degrees(), turn.y.to_degrees()) / dt
        } else {
            Vec2::ZERO
        };
        // (In a vehicle, the mouse turns the view as the game's camera turns a
        // vehicle's under the mouse: as fast as the mouse goes, up to its strategy's
        // top speed, pad.rs's vehicle_mouse_turn; hovering, or a tank boosting.
        // Flying, a jet's flying camera turns it; boosting on its wheels, a car or
        // truck steers by the mouse.)
        if player.form == Form::Vehicle && !flying && !steering {
            let fov_now = orbit.fov;
            let degrees = pad::vehicle_mouse_turn(
                Vec2::new(ctx.mouse.x, -rise),
                pad_look(&player).0,
                !car.hover,
                controls.mouse_scale(),
                fov_now,
                controls.camera_sensitivity,
                dt,
            );
            turn = Vec2::new(-degrees.x.to_radians(), degrees.y.to_radians());
        }
        if !flying && !held {
            if !steering {
                orbit.yaw += turn.x;
            }
            orbit.pitch = clamp_pitch(orbit.pitch + turn.y);
        }
    } else {
        orbit.stick.rest();
        orbit.mouse_rate = Vec2::ZERO;
    }
    // The mouse's steering of a car or truck (read always, used on its wheels).
    if !pad.active {
        let dx = if grabbed && wheeled.is_some() {
            ctx.mouse.x
        } else {
            0.0
        };
        let fov_now = orbit.fov;
        orbit
            .vcam
            .read_steering(dx, controls.mouse_scale(), fov_now, dt);
    }
    let roll = match jet {
        Some(plane) if flying => {
            let o = &mut *orbit;
            let (turn, roll) = jet::view_turn(&mut o.fly_spin, &plane, o.mouse_rate, dt);
            o.yaw -= turn.x;
            o.pitch = clamp_pitch(o.pitch + turn.y);
            roll
        }
        _ => {
            orbit.fly_spin = Vec2::ZERO;
            0.0
        }
    };
    // (The roll eases in and out with the strategies' blend.)
    orbit.roll += (roll - orbit.roll) * (dt * 4.0).min(1.0);
    let fly_up = player.kit.vehicle_camera.drive_up;
    if flying {
        let o = &mut *orbit;
        jet::flying_anchor(&mut o.fly_anchor, o.fly_spin, fly_up, o.pitch, dt);
    } else {
        orbit.fly_anchor = Vec3::Y * fly_up;
    }
    // A car's or truck's camera this frame: the view followed round after it and
    // smoothed, where its strategies put it, its field of view, roll, wiggle and
    // blur (vehicle_cam.rs).
    let shot = match wheeled {
        Some(strategies) => {
            let o = &mut *orbit;
            Some(
                o.vcam
                    .frame(&strategies, &player, car, &mut o.yaw, &mut o.pitch, dt),
            )
        }
        None => {
            // (Out of the car the view carries on from where its camera had it.)
            if let Some((yaw, pitch)) = orbit.vcam.view_if_active() {
                orbit.yaw = yaw;
                orbit.pitch = pitch;
            }
            let (yaw, pitch) = (orbit.yaw, orbit.pitch);
            orbit.vcam.reset(yaw, pitch);
            None
        }
    };
    let view = if shot.is_some() {
        orbit.vcam.view
    } else {
        (orbit.yaw, orbit.pitch)
    };
    // (A car or truck faces its camera's view, but off its wheels for a moment the
    // way it was going: vehicle_cam's Rig::facing.)
    orbit.view_yaw = if shot.is_some() {
        orbit.vcam.facing(orbit.yaw)
    } else {
        view.0
    };

    // Zoom in for fine aim the way the game does: a narrower field of view, eased in
    // and out on the camera's smoother (FOV_SMOOTH_TIME, FOV_SMOOTH_OUT); a scope
    // that takes the camera in close (the class camera's scope) moves it in and out
    // with it. (A car's or truck's is its strategies'.)
    let (target_fov, fov_secs) = if player.aiming {
        (
            in_hand.map_or(FINE_AIM_FOV, |w| w.fine_aim_fov),
            FOV_SMOOTH_TIME,
        )
    } else if died_down {
        (DEATH_FOV, DEATH_CAMERA_SECS)
    } else if player.revive.downed.is_some() {
        (DOWNED_FOV, DOWNED_CAMERA_SECS)
    } else if let Some(plane) = jet {
        if flying {
            (plane.fly_fov, 1.0)
        } else {
            (plane.hover_fov, 0.5)
        }
    } else if let Some(shot) = &shot {
        (shot.fov, FOV_SMOOTH_OUT)
    } else if player.form == Form::Vehicle
        && let Some((drive, hover)) = player.kit.vehicle_camera.fov
    {
        // (A tank's: its camera's, driving or hovering.)
        let h = orbit.hover.clamp(0.0, 1.0);
        (drive + (hover - drive) * h, FOV_SMOOTH_OUT)
    } else if sprint_camera {
        player.kit.camera.sprint.fov
    } else if let Some((_, fov)) = player.kit.camera.sprint.whirl.filter(|_| whirl_camera) {
        fov
    } else {
        (CAMERA_FOV, FOV_SMOOTH_OUT)
    };
    let o = &mut *orbit;
    if came_back {
        (o.fov, o.fov_vel, o.downed) = (target_fov, 0.0, 0.0);
    }
    smooth_damp(&mut o.fov, &mut o.fov_vel, target_fov, fov_secs * 0.5, dt);
    let mut fov_target = 0.0;
    if let Some((recoil, since)) = player.recoil {
        let since = since + dt;
        fov_target = recoil.fov * recoil.fov_left(since);
        player.recoil = (since <= recoil.delay + recoil.secs).then_some((recoil, since));
    }
    let (shown, speed) = orbit.fov_kick;
    let (off, speed) = settle(shown - fov_target, speed, RECOIL_EASE, dt);
    orbit.fov_kick = (fov_target + off, speed);
    // The rounds' blasts' shakes (PlayingShake), added up: the view's turn and place,
    // and the field of view.
    let o = &mut *orbit;
    for shake in player.screen_shakes.drain(..) {
        o.screen_shakes
            .push(PlayingShake::new(shake, &mut o.screen_seed));
    }
    let (mut screen_rot, mut screen_loc, mut screen_fov) = (Vec3::ZERO, Vec3::ZERO, 0.0);
    o.screen_shakes.retain_mut(|s| match s.step(dt) {
        Some((rot, loc, fov)) => {
            screen_rot += rot;
            screen_loc += loc;
            screen_fov += fov;
            true
        }
        None => false,
    });
    let fov = orbit.fov - orbit.fov_kick.0 + screen_fov;
    if let Projection::Perspective(perspective) = &mut *projection {
        // Bevy's field of view is vertical; the game's is horizontal.
        let aspect = split::fov_aspect(perspective.aspect_ratio.max(0.1), ctx.split);
        perspective.fov = 2.0 * ((fov.to_radians() * 0.5).tan() / aspect).atan();
    }

    // The shoulder side follows the hand the gun is in (FlipYAxisWhenLeftHanded),
    // moving across smoothly (HmC2Smoother, SHOULDER_SMOOTH_SECS).
    let side = if player.left_handed { -1.0 } else { 1.0 };
    let o = &mut *orbit;
    smooth_damp(
        &mut o.shoulder,
        &mut o.shoulder_vel,
        side,
        SHOULDER_SMOOTH_SECS,
        dt,
    );
    // A tank's or jet's camera moves between its driving and hovering places over
    // its strategies' blend (HOVER_CAMERA_SECS).
    let o = &mut *orbit;
    smooth_damp(
        &mut o.hover,
        &mut o.hover_vel,
        if car.hover { 1.0 } else { 0.0 },
        HOVER_CAMERA_SECS * 0.5,
        dt,
    );
    // The robot camera's aim and orbit for the view's pitch, the sprint's camera
    // state eased in or out, and its wiggle (robot_cam.rs).
    let o = &mut *orbit;
    let robot = o.robot.frame(
        &player.kit.camera,
        (sprint_camera, whirl_camera),
        o.pitch,
        player.velocity.length(),
        dt,
    );
    let offsets = match &shot {
        Some(shot) => shot.offsets,
        None => rig_offsets(
            &player,
            car,
            orbit.hover,
            orbit.shoulder,
            orbit.scope,
            orbit.fly_anchor,
            robot,
        ),
    };
    // Changing form, the camera moves over to the other form's place as the game's
    // blends into its new strategy (FORM_CAMERA_SECS), rather than jumping there.
    let o = &mut *orbit;
    if o.rig_form != Some(player.form) {
        o.rig_from = if o.rig_form.is_some() {
            o.rig_last
        } else {
            offsets
        };
        o.rig_form = Some(player.form);
        o.rig_blend = 0.0;
    }
    o.rig_blend = (o.rig_blend + dt / FORM_CAMERA_SECS).min(1.0);
    let k = o.rig_blend * o.rig_blend * (3.0 - 2.0 * o.rig_blend);
    let offsets = o.rig_from.lerp(offsets, k);
    o.rig_last = offsets;
    // Down (revive.rs), the camera moves in close over his shoulder (the downed
    // strategy's place), and back out as he gets up, or more slowly as he dies
    // (DEATH_*).
    let want = if player.revive.downed.is_some() {
        1.0
    } else {
        0.0
    };
    let secs = if died_down {
        DEATH_CAMERA_SECS
    } else {
        DOWNED_CAMERA_SECS
    };
    let step = dt / secs;
    o.downed += (want - o.downed).clamp(-step, step);
    let k = o.downed * o.downed * (3.0 - 2.0 * o.downed);
    let offsets = offsets.lerp(DOWNED_CAMERA, k);
    // Down, the camera keeps on his body (downed.rs), turning to look down at it
    // over his middle; it is back behind him as he comes back.
    let o = &mut *orbit;
    if player.death_view.is_some() {
        smooth_damp(
            &mut o.down,
            &mut o.down_vel,
            1.0,
            DOWN_CAMERA_SECS * 0.5,
            dt,
        );
    } else {
        (o.down, o.down_vel) = (0.0, 0.0);
    }
    // (Dead after being down, it goes back to his own camera's place, round his
    // body's middle as it lies, as if he stood there, looking as it was: DEATH_*.)
    let k = if died_down { 0.0 } else { orbit.down };
    let rig_view = (view.0, view.1 + (view.1.min(DOWN_PITCH) - view.1) * k);
    let offsets = Vec4::new(offsets.x * (1.0 - k), offsets.y, offsets.z, offsets.w);
    // (Its target, over his feet as he stands, comes down onto his body's middle;
    // dead after being down, it moves there from where he lay as the camera comes
    // out of the downed one.)
    let at = match player.death_view {
        Some(feet) if died_down => {
            let d = orbit.downed * orbit.downed * (3.0 - 2.0 * orbit.downed);
            feet.lerp(player_transform.translation, d)
        }
        Some(feet) => feet + Vec3::Y * ((player.kit.robot.height * 0.5 - offsets.y) * k),
        None => player_transform.translation,
    };
    let (position, rotation) = camera_rig(&level, at, rig_view, offsets);
    // A shot's shake turns the view (not the aim): the camera's shake waves run on
    // all the time, at the strength the last shot's shake has left (Shaker).
    orbit.shake_time = (orbit.shake_time + dt) % SHAKE_CLOCK_SECS;
    let strength = orbit.shaker.step(player.shake.take(), dt);
    let waves = if matches!(player.form, Form::Robot) {
        ROBOT_SHAKE_WAVES
    } else {
        VEHICLE_SHAKE_WAVES
    };
    let shake = waves.at(orbit.shake_time) * strength.x + Vec3::Z * strength.y;
    orbit.shake = shake;
    // (A car's or truck's roll and wiggle turn the view too: its strategies'; on
    // foot, its over-the-shoulder strategy's wiggle: robot_cam.rs.)
    let (dutch, wiggle) = match &shot {
        Some(s) => (s.roll, s.wiggle),
        None if player.form == Form::Robot && !dead => (0.0, robot.wiggle),
        None => (0.0, Vec3::ZERO),
    };
    // (A blast's shake moves the camera along its axes, then turns it.)
    cam_transform.translation =
        position + rotation * Vec3::new(screen_loc.y, screen_loc.z, -screen_loc.x);
    cam_transform.rotation = rotation
        * Quat::from_rotation_z(orbit.roll + dutch)
        * Quat::from_rotation_y(-(shake.y + wiggle.y + screen_rot.y).to_radians())
        * Quat::from_rotation_x((shake.x + wiggle.x + screen_rot.x).to_radians())
        * Quat::from_rotation_z(-(shake.z + wiggle.z + screen_rot.z).to_radians());
    input.view_origin = position;
    input.view_dir = rotation * Vec3::NEG_Z;
    // Out till the round ends, past his death's view (revive.rs): the camera
    // watches the teammate he spectates, or with nobody to watch is the level's.
    if spectating {
        let fov_now = match &*projection {
            Projection::Perspective(p) => {
                let aspect = split::fov_aspect(p.aspect_ratio.max(0.1), ctx.split);
                2.0 * ((p.fov * 0.5).tan() * aspect).atan().to_degrees()
            }
            _ => CAMERA_FOV,
        };
        let watched = spectate
            .target
            .and_then(|slot| watchable.iter().find(|w| w.slot == slot));
        let whom = watched.map(|w| w.slot);
        let o = &mut *orbit;
        if o.watching != Some(whom) {
            // (From where the camera is now, and what it is on: his body, or the
            // player watched before; a player watched starts from his view.)
            (o.watch_from, o.watch_from_on) = match o.watching {
                None => (
                    (cam_transform.translation, cam_transform.rotation, fov_now),
                    Some(at),
                ),
                Some(_) => (o.watch_last, o.watch_on),
            };
            o.watching = Some(whom);
            o.watch_blend = 0.0;
            if let Some(w) = watched {
                o.yaw = w.yaw;
                o.pitch = w.pitch.clamp(ROBOT_PITCH_RANGE.0, ROBOT_PITCH_RANGE.1);
            }
        }
        o.watch_blend = (o.watch_blend + dt / SPECTATE_SECS).min(1.0);
        let (spot, turn, fov, on) = match watched {
            Some(w) => {
                let (lo, hi) = SPECTATE_RISE;
                let k = (o.pitch.to_degrees().abs() / SPECTATE_PITCH_RANGE).min(1.0);
                let rig = Vec4::new(0.0, SPECTATE_UP + lo + (hi - lo) * k, SPECTATE_BACK, 0.0);
                let (spot, turn) = camera_rig(level, w.at, (o.yaw, o.pitch), rig);
                (spot, turn, SPECTATE_FOV, Some(w.at))
            }
            None => {
                let (spot, turn, fov) = level.pre_match_view();
                (spot, turn, fov, None)
            }
        };
        let (from, from_turn, from_fov) = o.watch_from;
        let k = o.watch_blend * o.watch_blend * (3.0 - 2.0 * o.watch_blend);
        let (place, turned) =
            spectate_place((from, from_turn, o.watch_from_on), (spot, turn, on), k);
        o.watch_on = on;
        let fov = from_fov + (fov - from_fov) * k;
        o.watch_last = (place, turned, fov);
        cam_transform.translation = place;
        cam_transform.rotation = turned;
        if let Projection::Perspective(perspective) = &mut *projection {
            let aspect = split::fov_aspect(perspective.aspect_ratio.max(0.1), ctx.split);
            perspective.fov = 2.0 * ((fov.to_radians() * 0.5).tan() / aspect).atan();
        }
    } else {
        orbit.watching = None;
    }
    // Boosting blurs the picture as it moves (the driving strategy's), easing in and
    // out with the strategies' blend.
    if let Some(mut blur) = blur {
        let shutter = shot.as_ref().map_or(0.0, |s| s.shutter);
        if blur.shutter_angle != shutter {
            blur.shutter_angle = shutter;
        }
    }
}

/// The spectating camera `k` (0 to 1) of the way from where it set out (its place,
/// turn, and what it was on: his body or the player watched before) to where it is
/// going (the same, on the player watched now). The game's strategies blend round
/// what they are on: going from one to another, the camera cuts to the new one,
/// placed off him as it was off the last, and swings in round him from there as
/// its view turns (each camera's offset from him in its own view's axes, blended),
/// as the game's video shows. To or from the level's camera (on nobody), it glides
/// from where it was.
fn spectate_place(
    (from, from_turn, was): (Vec3, Quat, Option<Vec3>),
    (spot, turn, now): (Vec3, Quat, Option<Vec3>),
    k: f32,
) -> (Vec3, Quat) {
    let turned = from_turn.slerp(turn, k);
    let place = match (was, now) {
        (Some(was), Some(now)) => {
            let set_out = from_turn.inverse() * (from - was);
            let end = turn.inverse() * (spot - now);
            now + turned * set_out.lerp(end, k)
        }
        _ => from.lerp(spot, k),
    };
    (place, turned)
}

/// The camera fades the player's own Jazz (robot or car) as it comes close, as the
/// game's does (TnCamera.UpdatePawnTransparencies, UpdateTransparencyForPawn): his
/// collision cylinder, padded by the camera strategy's PlayerCylinderRadiusPadding
/// and PlayerCylinderHeightPadding (30 each, HmCameraStrategy's), is taken as a
/// capsule, and how far the camera is from it, over its PlayerFadeStartDistance
/// (200), is how opaque he is (0.001 to 1): with the camera pushed in against a
/// wall, he fades. Scoped (the Nucleon Charge Rifle takes the camera in close
/// behind his head) he is gone altogether: the game's scope leaves Jazz out of the
/// picture, but bigger armor (Snarl's) reaches into it, so he fades out as the
/// camera moves in to the scope and back in as it leaves.
const PAWN_FADE_PADDING: f32 = 0.3;
const PAWN_FADE_START: f32 = 2.0;
/// Half the height of a downed Jazz's cylinder as the fade takes it, lying (m).
const LYING_HALF_HEIGHT: f32 = 0.6;
/// Under this opacity (the dither's last step) a mesh isn't drawn at all.
const PAWN_FADE_GONE: f32 = 1.0 / 32.0;

/// How opaque a Jazz whose collision cylinder (`radius`, `half_height`) stands
/// round `centre` is to a camera at `camera` (PAWN_FADE_*).
fn pawn_opacity(camera: Vec3, centre: Vec3, radius: f32, half_height: f32) -> f32 {
    let r = radius + PAWN_FADE_PADDING;
    let h = (half_height + PAWN_FADE_PADDING - r).max(0.0);
    let mut offset = centre - camera;
    offset.y = (offset.y.abs() - h).max(0.0);
    ((offset.length() - r) / PAWN_FADE_START).clamp(0.001, 1.0)
}

/// Draws pawn_opacity on the player's own Jazz the way the game draws a pawn
/// semi-transparent: a dithered pattern over each of his meshes, as much of him
/// drawn as he is opaque (Bevy's visibility-range dither, its range set each frame
/// so that the camera's distance from each mesh falls where the dither is his
/// opacity). For Bevy to draw it:
/// - A mesh is given its dithered shaders only when it is specialized with a
///   crossfading range, and that happens when its mesh or material changes, not
///   its range: so his ranges always have a crossfade (behind the camera while he
///   is fully drawn), and a mesh is marked changed as it gets its first.
/// - The depth prepass draws opaque meshes without their fragment shader, so
///   without the dither: the dithered-out pixels would show nothing behind them
///   (a dark shape). His materials are made alpha-masked instead, with a cutoff
///   of 0 (drawn just the same), whose prepass runs the shader and dithers.
/// - Bevy numbers the ranges anew whenever one changes, and a mesh takes its new
///   number only when its range is marked changed: so all his ranges are set
///   together.
///
/// Fully faded, a mesh's range starts past the camera, and it isn't drawn at all.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn fade_near_camera(
    mut commands: Commands,
    camera: Query<(&GlobalTransform, &OrbitCamera, &split::Seat)>,
    players: Query<(Entity, &Transform, &Player, &split::Seat), With<LocalPlayer>>,
    models: Query<(Entity, &JazzOf), Or<(With<RobotForm>, With<VehicleForm>)>>,
    children: Query<&Children>,
    mut meshes: Query<(
        &GlobalTransform,
        Option<&mut VisibilityRange>,
        &mut Mesh3d,
        Option<&MeshMaterial3d<StandardMaterial>>,
        Option<&MeshMaterial3d<character::PaintMaterial>>,
        Has<decals::Decal>,
    )>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut paints: ResMut<Assets<character::PaintMaterial>>,
    mut wanted: Local<Vec<(Entity, VisibilityRange)>>,
) {
    // (Each of this PC's players by his own camera: split.rs.)
    for (camera, orbit, seat) in &camera {
        let Some((player_entity, transform, player, _)) = players.iter().find(|(.., s)| *s == seat)
        else {
            continue;
        };
        fade_from(
            (camera, orbit),
            (player_entity, transform, player),
            (&models, &children),
            (&mut meshes, &mut standard, &mut paints),
            &mut wanted,
            &mut commands,
        );
    }
}

/// One player's Jazz faded by his camera (fade_near_camera).
#[allow(clippy::type_complexity)]
fn fade_from(
    (camera, orbit): (&GlobalTransform, &OrbitCamera),
    (player_entity, transform, player): (Entity, &Transform, &Player),
    (models, children): (
        &Query<(Entity, &JazzOf), Or<(With<RobotForm>, With<VehicleForm>)>>,
        &Query<&Children>,
    ),
    (meshes, standard, paints): (
        &mut Query<(
            &GlobalTransform,
            Option<&mut VisibilityRange>,
            &mut Mesh3d,
            Option<&MeshMaterial3d<StandardMaterial>>,
            Option<&MeshMaterial3d<character::PaintMaterial>>,
            Has<decals::Decal>,
        )>,
        &mut Assets<StandardMaterial>,
        &mut Assets<character::PaintMaterial>,
    ),
    wanted: &mut Vec<(Entity, VisibilityRange)>,
    commands: &mut Commands,
) {
    const FAR: f32 = 1.0e9;
    let eye = camera.translation();
    let (radius, height) = player.size();
    // Lying down under the downed camera (revive.rs), his cylinder is taken as
    // lying with him, low over the floor (LYING_HALF_HEIGHT): the camera over him
    // leaves him whole, his gun in the picture, as the game's video shows him,
    // unless it comes in close; it stands again as the camera leaves.
    let k = orbit.downed * orbit.downed * (3.0 - 2.0 * orbit.downed);
    let half_height = height * 0.5 + (LYING_HALF_HEIGHT.min(height * 0.5) - height * 0.5) * k;
    let opacity = pawn_opacity(
        eye,
        transform.translation + Vec3::Y * half_height,
        radius,
        half_height,
    )
    .min(1.0 - orbit.scope);
    wanted.clear();
    let mut changed = false;
    for (model, of) in models.iter() {
        if of.0 != player_entity {
            continue;
        }
        for e in children.iter_descendants(model) {
            let Ok((at, range, mut mesh, plain, painted, decal)) = meshes.get_mut(e) else {
                continue;
            };
            // (Effects on him, in their own materials, are left as they are; his
            // decals fade with him, their shader dithering as the paint's does.)
            let opaque = |mode: &AlphaMode| matches!(mode, AlphaMode::Opaque);
            if let Some(m) = plain {
                if standard.get(&m.0).is_some_and(|m| opaque(&m.alpha_mode))
                    && let Some(m) = standard.get_mut(&m.0)
                {
                    m.alpha_mode = AlphaMode::Mask(0.0);
                }
            } else if let Some(m) = painted {
                if paints.get(&m.0).is_some_and(|m| opaque(&m.base.alpha_mode))
                    && let Some(m) = paints.get_mut(&m.0)
                {
                    m.base.alpha_mode = AlphaMode::Mask(0.0);
                }
            } else if !decal {
                continue;
            }
            // (The dither level is (distance - start) / (end - start) of 16 steps.)
            let d = at.translation().distance(eye);
            let start = if opacity >= 1.0 {
                -2.0
            } else if opacity < PAWN_FADE_GONE {
                d + 1.0
            } else {
                d - opacity
            };
            let want = VisibilityRange {
                start_margin: start..start + 1.0,
                end_margin: FAR..FAR,
                use_aabb: false,
            };
            match range {
                Some(r) => changed |= *r != want,
                None => {
                    changed = true;
                    mesh.set_changed();
                }
            }
            wanted.push((e, want));
        }
    }
    if !changed {
        return;
    }
    for (e, want) in wanted.drain(..) {
        match meshes.get_mut(e) {
            Ok((_, Some(mut r), ..)) => *r = want,
            _ => {
                commands.entity(e).try_insert(want);
            }
        }
    }
}

/// The look speeds the controller's right stick turns the view at in the form he is
/// in (the camera strategy's, pad.rs), and how long its ramp takes: the robot's
/// over-the-shoulder camera's, sprinting or fine aiming (the Nucleon Charge Rifle's
/// and Photon Burst Rifle's scopes their own), a tank's, or a hovering vehicle's.
fn pad_look(player: &Player) -> (pad::LookSpeed, f32) {
    match player.form {
        Form::Robot => {
            let look = if sprint_run(player.sprinting, player.move_input) {
                // (Its class's strategy's sprint's: character::SprintCamera.)
                let (yaw, pitch, ramp) = player.kit.camera.sprint.look;
                pad::LookSpeed { yaw, pitch, ramp }
            } else if player.aiming {
                match WEAPONS.get(player.weapon).map(|w| w.id) {
                    Some("SniperRifle") => pad::SNIPER_LOOK,
                    Some("BurstRifle") => pad::BURST_LOOK,
                    _ => pad::FINE_AIM_LOOK,
                }
            } else {
                pad::ROBOT_LOOK
            };
            (look, pad::ROBOT_RAMP_SECS)
        }
        Form::Vehicle if player.kit.tank => (pad::TANK_LOOK, pad::VEHICLE_RAMP_SECS),
        // (A jet's hovering camera, HoverPlane_MP_STRATEGY: its own.)
        Form::Vehicle if player.kit.plane.is_some() => (jet::HOVER_LOOK, jet::HOVER_RAMP_SECS),
        Form::Vehicle => (pad::HOVER_LOOK, pad::VEHICLE_RAMP_SECS),
    }
}

/// Dash/Sprint, one key for both as the game's tables have it (Dash(Press)/
/// Sprint(Hold)): a tap dashes as the key is let go, within SPRINT_HOLD_SECS;
/// held longer, Jazz sprints from then on and doesn't dash. `down_since` is when
/// the key went down (None while it's up). A key already down when it reaches the
/// game (back from the pause menu, or out of the car) has been held all along,
/// and one taken away (to the pause menu, or into the car) wasn't let go: no
/// dash. Returns (dash, sprint) for this frame.
fn dash_or_sprint(
    down_since: &mut Option<f32>,
    pressed: bool,
    held: bool,
    released: bool,
    now: f32,
) -> (bool, bool) {
    if down_since.is_none() {
        if pressed {
            *down_since = Some(now);
        } else if held {
            *down_since = Some(f32::NEG_INFINITY);
        }
    }
    let mut dash = false;
    if !held && let Some(since) = down_since.take() {
        dash = released && now - since < SPRINT_HOLD_SECS;
    }
    let sprint = down_since.is_some_and(|since| now - since >= SPRINT_HOLD_SECS);
    (dash, sprint)
}

/// The local player's input, read from this machine's keyboard and mouse (the
/// pause menu has put them aside while it is up) through the keyboard layout in
/// use (Settings > Controls): the robot's keys, or the vehicle's in the car; or,
/// with Active Game Controls on Controller, from the controller by the game's
/// controller layout (pad.rs), the keyboard and mouse playing no part. Where he
/// looks is where his camera heads and pitches (camera_follow turns it with the
/// mouse or the right stick, and gives him his view). Split screen's guest
/// (split.rs) plays by his own controller only.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn read_local_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    (controls, pads): (Res<Controls>, pad::Pads),
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    cameras: Query<(&OrbitCamera, &split::Seat)>,
    mut locals: Query<(&mut PlayerInput, &Player, &car::Car, &split::Seat), With<LocalPlayer>>,
    mut boost_latch: Local<[bool; split::SEATS]>,
    mut last_swap: Local<[f32; split::SEATS]>,
    mut dash_down: Local<[Option<f32>; split::SEATS]>,
) {
    for (input, player, car, seat) in &mut locals {
        let Some((camera, _)) = cameras.iter().find(|(_, s)| *s == seat) else {
            continue;
        };
        let i = seat.0.min(split::SEATS - 1);
        let first = i == 0;
        let pad = pads.seat(i);
        // (The guest has no keyboard and mouse: only his controller plays him.)
        let by_pad = pad.active || !first;
        let grabbed = by_pad || cursor.grab_mode != CursorGrabMode::None;
        let inputs = Inputs {
            keys: &keys,
            mouse: &mouse,
            wheel: scroll.delta.y,
        };
        read_seat_input(
            (&time, &controls, pad, by_pad, grabbed, &inputs),
            (input, player, car, camera),
            (&mut boost_latch[i], &mut last_swap[i], &mut dash_down[i]),
        );
    }
}

/// One player's input (read_local_input): from his controller (`by_pad`), or the
/// keyboard and mouse; `grabbed`: the trigger counts (the game has the mouse, or
/// the controller plays).
#[allow(clippy::type_complexity)]
fn read_seat_input(
    (time, controls, pad, by_pad, grabbed, inputs): (
        &Time,
        &Controls,
        &pad::Pad,
        bool,
        bool,
        &Inputs,
    ),
    (mut input, player, car, camera): (Mut<PlayerInput>, &Player, &car::Car, &OrbitCamera),
    (boost_latch, last_swap, dash_down): (&mut bool, &mut f32, &mut Option<f32>),
) {
    let now = time.elapsed_secs();
    let keys = inputs.keys;
    // What the layout's actions ask for this frame: the keys' and mouse's, or the
    // controller's.
    let held = |mode, action| {
        if by_pad {
            pad.action_held(mode, action)
        } else {
            controls.held(mode, action, inputs)
        }
    };
    let pressed = |mode, action| {
        if by_pad {
            pad.action_pressed(mode, action)
        } else {
            controls.pressed(mode, action, inputs)
        }
    };
    let released = |mode, action| {
        if by_pad {
            pad.action_released(mode, action)
        } else {
            controls.released(mode, action, inputs)
        }
    };
    let form_mode = match player.form {
        Form::Robot => Mode::Robot,
        Form::Vehicle => Mode::Vehicle,
    };
    // A Scientist's jet has his keys from the frame he starts to change into it (the
    // game's controller goes over to the jet's then, before the change plays: jet.rs),
    // so Boost held as he changes has it flying at once. (Change Form itself is read
    // with his form's keys.)
    let into_jet = player.kit.plane.is_some()
        && player.form == Form::Robot
        && (player.pending_form == Some(Form::Vehicle)
            || (can_transform(player) && pressed(Mode::Robot, Action::ChangeForm)));
    let mode = if into_jet { Mode::Vehicle } else { form_mode };
    // (Only the robot has Dash/Sprint: in the car it reads as up.)
    let (dash, sprint) = dash_or_sprint(
        dash_down,
        pressed(mode, Action::DashSprint),
        held(mode, Action::DashSprint),
        released(mode, Action::DashSprint),
        now,
    );
    // Toggle Vehicle Boost: Hold boosts while Speed Boost is held; Toggle starts
    // and stops boosting with a press (and stops out of the car, or on a crash).
    if mode != Mode::Vehicle || car.boost_cooldown > 0.0 || controls.hold_boost {
        *boost_latch = false;
    } else if pressed(mode, Action::SpeedBoost) {
        *boost_latch = !*boost_latch;
    }
    let boost = match mode {
        Mode::Vehicle if controls.hold_boost => held(mode, Action::SpeedBoost),
        Mode::Vehicle => *boost_latch,
        _ => sprint,
    };
    // (A turn of the wheel can come over several frames: one swap for it.)
    let on_wheel = !by_pad
        && matches!(
            controls.bind(mode, Action::SwapWeapons),
            Some(Bind::Wheel | Bind::WheelUp | Bind::WheelDown)
        );
    let swap_weapon =
        pressed(mode, Action::SwapWeapons) && (!on_wheel || now - *last_swap >= SWAP_REPEAT_SECS);
    if swap_weapon {
        *last_swap = now;
    }
    // The controller's right stick for a jet's flight, up and down the Jet Camera's
    // way round (as camera_follow turns the view with it).
    let stick = by_pad.then(|| {
        let inverted = match player.form {
            Form::Robot => controls.invert[0],
            Form::Vehicle if player.kit.plane.is_some() => controls.invert[2],
            Form::Vehicle => controls.invert[1],
        };
        let up = if inverted { -1.0 } else { 1.0 };
        Vec2::new(pad.right.x, pad.right.y * up)
    });
    let swing = angle_to(input.yaw, camera.view_yaw) / time.delta_secs().max(1e-4);
    let (view_origin, view_dir) = (input.view_origin, input.view_dir);
    *input = PlayerInput {
        // (The left stick past its dead zone moves him as the keys do, at his
        // speed whichever way it points.)
        movement: if by_pad {
            pad.left.normalize_or_zero()
        } else {
            controls.movement(mode, keys)
        },
        control: grabbed,
        boost,
        fire: grabbed && held(mode, Action::Fire),
        aim: held(mode, Action::FineAim),
        pick_up: held(mode, Action::Interact),
        // (The controller's jump and a jet's climb are its PressJump and Ascend,
        // apart with the Seeker scheme: pad.rs.)
        jump: if by_pad {
            pressed(mode, Action::Jump)
        } else {
            pressed(mode, Action::Jump) || pressed(mode, Action::JumpAscend)
        },
        dash,
        melee: pressed(mode, Action::Melee),
        transform: pressed(form_mode, Action::ChangeForm),
        reload: pressed(mode, Action::Reload),
        switch_hands: pressed(mode, Action::SwitchCamera),
        fire_pressed: grabbed && pressed(mode, Action::Fire),
        swap_weapon,
        ability: grabbed && held(mode, Action::Ability),
        tech: if mode == Mode::Robot {
            [
                grabbed && held(mode, Action::AssaultTech),
                grabbed && held(mode, Action::UtilityTech),
            ]
        } else {
            [false; 2]
        },
        ascend: if by_pad {
            held(mode, Action::JumpAscend)
        } else {
            held(mode, Action::JumpAscend) || held(mode, Action::Jump)
        },
        descend: held(mode, Action::Descend),
        steer: camera.mouse_rate,
        stick,
        wheel: camera.vcam.steer,
        yaw: camera.view_yaw,
        swing,
        view_origin,
        view_dir,
    };
}

// ---------------------------------------------------------------------------
// Matches, and online play (net.rs): the other players' Jazzes, and this one's
// for them
// ---------------------------------------------------------------------------

/// How fast another player's Jazz closes on where his PC has him (the fraction of
/// the gap closed a second, as an exponential rate) and turns to its heading, and
/// how far off he jumps straight there (a respawn, or packets lost for a while).
const REMOTE_SMOOTH_RATE: f32 = 15.0;
const REMOTE_TURN_RATE: f32 = 20.0;
const REMOTE_SNAP_DISTANCE: f32 = 4.0;
/// He is moved on by his velocity for at most this long after his last snapshot.
const REMOTE_EXTRAPOLATE_SECS: f32 = 0.25;

/// A match starts (the Escalation lobby has started it): the world as every match
/// starts it, the same on every PC (no enemies or weapons lying about: the waves
/// bring the enemies in, waves.rs; the energon at what a match starts with, every
/// purchase point stocked), and this PC's Jazz fresh at his slot's start. Any other
/// Jazzes go (other players' from an earlier match: net_puppets brings this
/// match's in).
///
/// This runs twice: as the game is entered, while the player hasn't come into the
/// match (Choose Character, the countdown: pregame.rs), when it leaves the map
/// empty and his Jazz waiting; and again as the countdown runs out and the match
/// begins for him, when it all starts (the match's clock from then).
#[allow(clippy::too_many_arguments)]
fn start_match(
    mut commands: Commands,
    time: Res<Time>,
    (net, pre, current_map): (Res<net::Net>, Res<pregame::PreMatch>, Res<CurrentMap>),
    mut clock: ResMut<MatchClock>,
    mut score: split::BySeat<Score>,
    level: Res<Level>,
    things: Query<Entity, Or<(With<enemies::Foe>, With<WeaponPickup>)>>,
    mut players: Query<(
        Entity,
        &mut Transform,
        &mut Player,
        &mut PlayerSlot,
        &mut car::Car,
        Has<LocalPlayer>,
        Option<&split::Seat>,
    )>,
    mut forms: Query<
        (Entity, &mut Visibility, Has<RobotForm>, &JazzOf),
        Or<(With<RobotForm>, With<VehicleForm>)>,
    >,
    mut stations: Query<&mut HealthStation>,
    mut ammo_stations: Query<&mut AmmoStation>,
    (mut weapon_stations, mut equipment_stations): (
        Query<&mut shop::WeaponStation>,
        Query<&mut shop::EquipmentStation>,
    ),
    mut cameras: Query<&mut OrbitCamera>,
) {
    clock.0 = time.elapsed_secs();
    for entity in &things {
        commands.entity(entity).despawn();
    }
    for score in score.each() {
        *score = Score::default();
    }
    for mut station in &mut stations {
        station.away = 0.0;
    }
    for mut station in &mut ammo_stations {
        station.away = 0.0;
    }
    for mut station in &mut weapon_stations {
        station.away = 0.0;
    }
    for mut station in &mut equipment_stations {
        station.away = 0.0;
    }
    // (The enemies come in waves once the match has begun: waves.rs.)
    let me = net.my_slot().unwrap_or(0);
    for (entity, mut transform, mut player, mut slot, mut car, local, seat) in &mut players {
        // (Split screen's guest at his own slot.)
        let me = match net.guest_slot() {
            Some(guest) if split::seat_of(seat) == 1 => guest,
            _ => me,
        };
        if !local {
            // (As the countdown runs out the other players are this match's.)
            if !pre.begins {
                commands.entity(entity).despawn();
            }
            continue;
        }
        if player.form == Form::Vehicle {
            switch_form(
                &mut commands,
                entity,
                &mut player,
                &mut forms,
                Form::Robot,
                false,
            );
        }
        *player = new_player();
        // (Until a character is put on him, he glows in his map's team's colour.)
        player.look.decepticon = decepticon_map(current_map.0);
        player.heading = level.start_yaw;
        player.protection = SPAWN_PROTECTION;
        player.waiting = pre.waiting;
        *car = car::Car::default();
        slot.0 = me;
        transform.translation = level.start(me);
        transform.rotation = Quat::from_rotation_y(level.start_yaw);
    }
    // The camera looks the way he faces, with no recoil or shake left over (nor a
    // change of form to follow).
    for mut orbit in &mut cameras {
        orbit.yaw = level.start_yaw;
        orbit.pitch = START_PITCH;
        orbit.view_yaw = level.start_yaw;
        orbit.vcam.reset(level.start_yaw, START_PITCH);
        orbit.kick = (0.0, 0.0);
        orbit.fov_kick = (0.0, 0.0);
        orbit.shaker = Shaker::default();
        orbit.shake = Vec3::ZERO;
        orbit.rig_form = None;
    }
}

/// Online: the other players' Jazzes come and go with the lobby (one for each of
/// their slots while the match is on). Another player's Jazz comes in out of the
/// match (waiting, unseen) until his PC's first word on him: as the match starts
/// he has no pawn on the map yet, and until his PC says he is in the match he
/// mustn't count as a living player (the host's count of the team's lives, and
/// the waves, start from the first living player: revive.rs's team_lives, waves.rs).
#[allow(clippy::too_many_arguments)]
fn net_puppets(
    mut commands: Commands,
    net: Res<net::Net>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    level: Res<Level>,
    puppets: Query<(Entity, &PlayerSlot), With<RemotePlayer>>,
) {
    // (The host has just ended the match: they stay as they are while the game
    // fades back to the lobby, which sets the match up anew.)
    if net.status() == net::NetStatus::Lobby {
        return;
    }
    let wanted: Vec<usize> = if net.in_match() {
        net.players()
            .map(|(slot, _)| slot)
            .filter(|slot| Some(*slot) != net.my_slot())
            .collect()
    } else {
        Vec::new()
    };
    for (entity, slot) in &puppets {
        if !wanted.contains(&slot.0) {
            commands.entity(entity).despawn();
        }
    }
    for slot in wanted {
        if puppets.iter().all(|(_, s)| s.0 != slot) {
            let jazz = spawn_jazz(
                &mut commands,
                &asset_server,
                &mut meshes,
                &mut materials,
                slot,
                level.start(slot),
                level.start_yaw,
                true,
            );
            commands.entity(jazz).insert(RemotePlayer);
        }
    }
}

/// What befell each of this PC's players this frame goes to his HUD (split screen's
/// guest's to his own); another player's is dropped.
fn deliver_hud_events(
    mut huds: split::BySeat<HudState>,
    mut players: Query<(&mut Player, Has<LocalPlayer>, Option<&split::Seat>)>,
) {
    for (mut player, local, seat) in &mut players {
        if player.hud_events.is_empty() {
            continue;
        }
        let events = std::mem::take(&mut player.hud_events);
        if local {
            huds.seat(split::seat_of(seat)).events.extend(events);
        }
    }
}

/// Split screen's guest's Jazz (split.rs): his own while he is in (from the lobby,
/// as he joins), waiting at his slot's start (start_match puts him in the match as
/// it puts the first player's), gone as he leaves.
fn guest_jazz(
    mut commands: Commands,
    net: Res<net::Net>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    level: Res<Level>,
    locals: Query<(Entity, &split::Seat), With<LocalPlayer>>,
) {
    let guest = locals.iter().find(|(_, s)| s.0 == 1).map(|(e, _)| e);
    match (net.guest_slot(), guest) {
        (Some(slot), None) => {
            let jazz = spawn_jazz(
                &mut commands,
                &asset_server,
                &mut meshes,
                &mut materials,
                slot,
                level.start(slot),
                level.start_yaw,
                true,
            );
            commands.entity(jazz).insert((LocalPlayer, split::Seat(1)));
            info!("Split screen: the second player's Jazz {jazz:?}, slot {slot}");
        }
        (None, Some(jazz)) => {
            commands.entity(jazz).despawn();
        }
        _ => {}
    }
}

/// Online: each other player's Jazz here does what his PC says (net::Remote). He
/// closes on where his last snapshot puts him (moved on by his velocity since),
/// turns to its heading, and takes its form, stance, moves and their timings; his
/// events start his animations and car jumps, fire his shots here (their effects,
/// and their damage to the sentries here), throw his grenades, land his blows and
/// play his sounds where he is.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn net_apply(
    mut commands: Commands,
    time: Res<Time>,
    mut net: ResMut<net::Net>,
    bank: Option<Res<SoundBank>>,
    mut sfx: ResMut<Sfx>,
    mut fxq: ResMut<FxQueue>,
    mut puppets: Query<
        (
            Entity,
            &mut Transform,
            &mut Player,
            &mut car::Car,
            &mut PlayerInput,
            &PlayerSlot,
        ),
        With<RemotePlayer>,
    >,
    mut forms: Query<
        (Entity, &mut Visibility, Has<RobotForm>, &JazzOf),
        Or<(With<RobotForm>, With<VehicleForm>)>,
    >,
    mut enemies: Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    mut weapon_fx: ResMut<levelfx::FxSystems>,
    (mut remote_gear, mut revive_inbox, mut pod_inbox, mut door_inbox, mut ability_inbox): (
        ResMut<equipment::RemoteGear>,
        ResMut<revive::ReviveInbox>,
        ResMut<recreator::PodInbox>,
        ResMut<doors::DoorInbox>,
        ResMut<abilities::AbilityInbox>,
    ),
    (mut wave_inbox, waves, mut kill_feed, mut barrels): (
        ResMut<waves::WaveInbox>,
        Res<waves::Waves>,
        ResMut<killfeed::KillFeed>,
        ResMut<barrels::Barrels>,
    ),
) {
    use net::{JazzEvent, flag};
    let dt = time.delta_secs();
    let clock = net.clock();
    for (entity, mut transform, mut player, mut car, mut input, slot) in &mut puppets {
        let Some(remote) = net.remote_mut(slot.0) else {
            continue;
        };
        let events = std::mem::take(&mut remote.events);
        let fresh = std::mem::take(&mut remote.fresh);
        let age = (clock - remote.received_at).clamp(0.0, REMOTE_EXTRAPOLATE_SECS);
        let Some(snap) = remote.snapshot else {
            continue;
        };

        // His form (the car picks up his speed from carry_vel, as his did).
        let form = if snap.has(flag::VEHICLE) {
            Form::Vehicle
        } else {
            Form::Robot
        };
        if player.form != form {
            player.carry_vel = snap.carry_vel;
            switch_form(&mut commands, entity, &mut player, &mut forms, form, false);
        }

        // His state: from a new snapshot, or run on since the last.
        if fresh {
            let weapon = |w: u8| (w as usize).min(WEAPONS.len() - 1);
            let clip = |c: u8| ((c as usize) < ANIM_COUNT).then_some(c as usize);
            let heading = player.heading;
            player.vertical_velocity = snap.vertical_velocity;
            player.grounded = snap.has(flag::GROUNDED);
            player.velocity = snap.velocity;
            player.twist = snap.twist.clamp(-PI, PI);
            player.aim_pitch = snap.aim_pitch.clamp(-FRAC_PI_2, FRAC_PI_2);
            // (Only the clip matters: his PC turns him.)
            player.pivot = clip(snap.pivot_clip).map(|clip| Pivot {
                clip,
                from: heading,
                turn: 0.0,
                secs: 1.0,
                t: 0.0,
                speed: 1.0,
            });
            player.drive_speed = snap.drive_speed;
            player.strafe_speed = snap.strafe_speed;
            player.steer = snap.steer;
            player.yaw_rate = snap.yaw_rate;
            player.wheel_angle = snap.wheel_angle;
            player.loadout = snap.loadout.map(weapon);
            // (Upgrades for weapons the demo knows: the bits past UPGRADES' are none.)
            player.upgrades = snap.upgrades & ((1u64 << upgrades::UPGRADES.len()) - 1);
            player.hand = (snap.hand as usize).min(1);
            player.weapon = weapon(snap.weapon);
            player.move_input = snap.move_input.clamp_length_max(1.5);
            player.sprinting = snap.has(flag::SPRINTING);
            player.transform_left = snap.transform_left.clamp(0.0, 5.0);
            // (The clip as his PC plays it: his character's, which this PC has too.)
            let (to_vehicle, to_robot) = transform_clips(&player.kit);
            let c = snap.transform_clip as usize;
            player.transform_clip = if is_to_robot(c) { to_robot } else { to_vehicle };
            player.look = character::Look::from_bytes(snap.look);
            player.action = clip(snap.action_clip).map(|c| (c, snap.action_left.clamp(0.0, 10.0)));
            player.firing = snap.has(flag::FIRING);
            player.aiming = snap.has(flag::AIMING);
            player.charging = snap.has(flag::CHARGING);
            player.charged = snap.has(flag::CHARGED);
            // A new gun's warm-up (its wind-up heard from its start) and its firing
            // after it (StartFire: its loop and beam); a beam ends with the firing.
            let warming = snap.has(flag::WARMING);
            if warming && !player.gear.warming {
                player.gear.windup = arsenal::WINDUP_HOLD_SECS;
            }
            player.gear.warming = warming;
            player.gear.fired = player.firing && !warming;
            if player.gear.fired {
                player.gear.windup = 0.0;
            }
            if !player.firing {
                player.gear.beam_end = None;
            }
            // The blades loaded onto his Gear Shredder (their effects on his gun, and
            // its charge's loop).
            player.gear.blades =
                u8::from(snap.has(flag::BLADES_LOW)) + 2 * u8::from(snap.has(flag::BLADES_HIGH));
            player.left_handed = snap.has(flag::LEFT_HANDED);
            player.dying = snap.has(flag::DYING);
            player.waiting = snap.has(flag::WAITING);
            player.dead_left = snap.dead_left.clamp(0.0, RESPAWN_SECS);
            let heading = player.heading;
            player.revive.follow(&snap.revive, heading);
            player.health = snap.health;
            player.shield = snap.shield;
            player.carry_vel = snap.carry_vel;
            player.air_time = snap.air_time.max(0.0);
            player.jump_time = (snap.jump_time >= 0.0).then_some(snap.jump_time);
            car.hover = snap.has(flag::HOVER);
            car.hover_vel = snap.hover_vel;
            input.fire = snap.has(flag::TRIGGER);
        } else {
            player.transform_left = (player.transform_left - dt).max(0.0);
            if let Some((c, left)) = player.action {
                player.action = (left > dt).then_some((c, left - dt));
            }
            player.dead_left = remote_dead_left(player.dead_left, dt);
            player.revive.run_on(dt);
            if !player.grounded {
                player.air_time += dt;
                if let Some(t) = player.jump_time.as_mut() {
                    *t += dt;
                }
            }
        }

        // His class ability as his snapshot has it (abilities.rs).
        player.ability.follow(&snap.ability, fresh, dt);

        // Where he is: closing on his snapshot, moved on by its velocity.
        let mut target = snap.pos + snap.velocity * age;
        if !snap.has(flag::GROUNDED) {
            target.y += snap.vertical_velocity * age + 0.5 * GRAVITY * age * age;
        }
        let gap = target - transform.translation;
        if gap.length() > REMOTE_SNAP_DISTANCE {
            transform.translation = target;
        } else {
            transform.translation += gap * (1.0 - (-REMOTE_SMOOTH_RATE * dt).exp());
        }
        let turn = angle_to(player.heading, snap.heading);
        player.heading =
            (player.heading + turn * (1.0 - (-REMOTE_TURN_RATE * dt).exp())).rem_euclid(TAU);
        transform.rotation = Quat::from_rotation_y(player.heading);

        // What he did.
        let at = transform.translation;
        let side = *transform.right() * if player.left_handed { -1.0 } else { 1.0 };
        // (A new gun's effect as his rounds wound plays once a frame, as does the
        // Gear Shredder's shot's light.)
        let mut hit_fx = false;
        let mut blade_light = false;
        for event in events {
            // (His gunfire, for the enemies here to hear: the host's AI, ai/mod.rs's
            // player_noises.)
            if matches!(
                event,
                JazzEvent::Shot { .. }
                    | JazzEvent::Round { .. }
                    | JazzEvent::HomingRound { .. }
                    | JazzEvent::Lob { .. }
            ) {
                player.played.push(event.clone());
            }
            match event {
                JazzEvent::Clip { clip, speed } => {
                    if (clip as usize) < ANIM_TOTAL {
                        player.start_clip = Some((clip as usize, speed.clamp(0.05, 10.0)));
                    }
                }
                JazzEvent::WeaponClip { clip, secs, speed } => {
                    if (clip as usize) < ANIM_COUNT {
                        player.start_weapon_clip = Some((
                            clip as usize,
                            secs.clamp(0.0, 10.0),
                            speed.clamp(0.05, 10.0),
                        ));
                    }
                }
                JazzEvent::Equip { new, old } => {
                    if (new as usize) < WEAPONS.len() {
                        let old = ((old as usize) < WEAPONS.len()).then_some(old as usize);
                        player.start_equip = Some((new as usize, old));
                    }
                }
                JazzEvent::HandSwitch => player.start_hand_switch = true,
                JazzEvent::CarJump => car.jumped = true,
                JazzEvent::Sound { cue, delay, scale } => {
                    if let Some(bank) = bank.as_ref() {
                        sfx.play_remote(bank, &cue, at + Vec3::Y, delay, scale, entity.to_bits());
                    }
                }
                JazzEvent::Shot {
                    weapon,
                    origin,
                    dir,
                    distance,
                    struck,
                    enemy,
                } => {
                    // (A new gun's, and one whose flash is the game's own (the
                    // rifle's): the barrel the shot left, taking turns as his do,
                    // and a beam's end where it struck.)
                    let gun = WEAPONS
                        .get(weapon as usize)
                        .filter(|w| w.gear.is_some() || w.muzzle_system.is_some());
                    let mut barrel = None;
                    if let (Some(w), true) = (gun, struck & SHOT_PELLET == 0)
                        && let Some(sockets) = w.sockets
                    {
                        barrel =
                            Some(sockets.muzzles[player.muzzle as usize % sockets.muzzles.len()]);
                        player.muzzle = player.muzzle.wrapping_add(1);
                    }
                    if gun.and_then(|w| w.gear).is_some_and(|g| g.beam.is_some()) {
                        let d = distance.clamp(0.0, 1000.0);
                        player.gear.beam_end = Some(origin + dir.normalize_or_zero() * d);
                    }
                    remote_shot(
                        &mut commands,
                        &mut sfx,
                        (&mut fxq, &mut weapon_fx),
                        &mut enemies,
                        (&level, &colliders, &mut barrels),
                        weapon,
                        (origin, dir, distance),
                        (
                            struck,
                            (enemy != 0)
                                .then(|| waves.entity_of(enemy as u32))
                                .flatten(),
                        ),
                        side,
                        (entity, at, player.robot_centre(at)),
                        (barrel, &mut hit_fx),
                        player.mods(weapon as usize),
                    )
                }
                JazzEvent::EmpGrenade {
                    start,
                    velocity,
                    fuse,
                } => emp::spawn_emp_grenade(
                    &mut commands,
                    &mut weapon_fx,
                    start,
                    velocity.clamp_length_max(200.0),
                    fuse,
                    *transform.forward(),
                    None,
                ),
                // His round, flying here as on his PC (remote_round).
                JazzEvent::Round {
                    weapon,
                    origin,
                    dir,
                } => remote_round(
                    &mut commands,
                    (&mut fxq, &mut weapon_fx),
                    &enemies,
                    weapon,
                    (origin, dir, None),
                    None,
                    (entity, player.look.decepticon, round_mods(&player, weapon)),
                    &mut blade_light,
                ),
                JazzEvent::HomingRound {
                    weapon,
                    origin,
                    dir,
                    target,
                } => remote_round(
                    &mut commands,
                    (&mut fxq, &mut weapon_fx),
                    &enemies,
                    weapon,
                    (origin, dir, None),
                    Some(target),
                    (entity, player.look.decepticon, round_mods(&player, weapon)),
                    &mut blade_light,
                ),
                JazzEvent::Lob {
                    weapon,
                    origin,
                    velocity,
                } => remote_round(
                    &mut commands,
                    (&mut fxq, &mut weapon_fx),
                    &enemies,
                    weapon,
                    (origin, velocity, Some(velocity.clamp_length_max(500.0))),
                    None,
                    (entity, player.look.decepticon, round_mods(&player, weapon)),
                    &mut blade_light,
                ),
                // His weapon's rounds set off (his A-4 Pulsar Cannon's alt fire, his
                // Chaos-Rift Combustor's trigger): riot.rs sets off their copies here.
                JazzEvent::SetOff { weapon } => {
                    player.set_off = WEAPONS.get(weapon as usize).and_then(|w| w.projectile);
                }
                JazzEvent::Melee { at, forward } => {
                    // (As melee_hits: every enemy in reach in front of him.)
                    melee_blow(
                        &mut enemies,
                        (&mut sfx, &mut barrels),
                        (at, forward),
                        entity,
                    );
                }
                // His sword swing's blow begins: melee_sweeps sweeps its box here.
                JazzEvent::Sweep { centre, heading } => {
                    player.melee_sweep = melee::Sweep {
                        begun: true,
                        given: Some((centre, heading, 0.0)),
                        ..default()
                    };
                }
                // How he died: his copy falls the same way (downed.rs).
                death @ JazzEvent::Death { .. } => {
                    player.death_news = downed::DeathNews::of_event(&death);
                }
                // His support items: their copies here (equipment.rs).
                gear @ (JazzEvent::Gear { .. }
                | JazzEvent::Mine { .. }
                | JazzEvent::GearEnd { .. }) => {
                    remote_gear.0.push((entity, gear));
                }
                // His revive of a downed teammate: for that teammate's PC (revive.rs).
                revive @ JazzEvent::Revive { .. } => {
                    revive_inbox.0.push((slot.0, revive));
                }
                // His Armory Recreator used, or what one made taken (recreator.rs).
                pod @ (JazzEvent::Pod { .. } | JazzEvent::PodLoot { .. }) => {
                    pod_inbox.0.push(pod);
                }
                // His deposit into an unlock console, or its purchase (doors.rs).
                door @ (JazzEvent::Deposit { .. } | JazzEvent::Unlock { .. }) => {
                    door_inbox.0.push(door);
                }
                // A moment of his class ability (abilities.rs and its modules).
                ability @ JazzEvent::Ability { .. } => {
                    ability_inbox.0.push((entity, ability));
                }
                // One of the waves' enemies died at his PC: here too (waves.rs).
                JazzEvent::EnemyDeath {
                    id,
                    death,
                    flags,
                    at,
                    momentum,
                } => {
                    wave_inbox.0.push((id, death, flags, at, momentum));
                }
                // A death noticed at his PC: the kill feed here (killfeed.rs).
                kill @ JazzEvent::KillFeed { .. } => {
                    kill_feed.heard.extend(killfeed::KillNote::of_event(&kill));
                }
                // An exploding barrel blown up at his PC: here too (barrels.rs).
                JazzEvent::BarrelBlown { barrel, ai } => {
                    barrels.heard_blown(entity, barrel, ai);
                }
            }
        }
    }
}

/// How far from where a homing round's target was on its shooter's PC the sentry here
/// it homes on may be (the sentries here are his, but needn't be quite in step).
const REMOTE_TARGET_REACH: f32 = 3.0;

/// What the upgrades on a Jazz's weapon `weapon` (a JazzEvent's: WEAPONS', or a
/// vehicle's round, which has none) do to its rounds.
fn round_mods(player: &Player, weapon: u8) -> riot::RoundMods {
    riot::RoundMods::of(&player.mods(weapon as usize))
}

/// Another player's round (his JazzEvent::Round, HomingRound or Lob), flying here as on
/// his PC (it hurts the sentries here, not him: his PC sees to that), with its light
/// and muzzle flash, kept on his gun's muzzle: `weapon` (a tank's shell, a jet's or a
/// truck's rocket, or WEAPONS'), where it left and the way it went, at its kind's
/// speed or, lobbed, at `velocity`; homing on the sentry here nearest where his was
/// (`target_at`), if it homes; fired by `jazz` (it passes through him), of his side.
/// A Gear Shredder's blade bounces off the players here but him, his shot's light
/// once a frame however many blades (`blade_light`).
#[allow(clippy::too_many_arguments)]
fn remote_round(
    commands: &mut Commands,
    (fxq, weapon_fx): (&mut FxQueue, &mut levelfx::FxSystems),
    enemies: &Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    weapon: u8,
    (origin, dir, velocity): (Vec3, Vec3, Option<Vec3>),
    target_at: Option<Vec3>,
    (jazz, decepticon, mods): (Entity, bool, riot::RoundMods),
    blade_light: &mut bool,
) {
    let w = weapon as usize;
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }
    let firer = riot::Firer {
        owner: None,
        shooter: Some(jazz),
        decepticon,
        mods,
    };
    let target = target_at.and_then(|at| {
        enemies
            .iter()
            .filter(|(_, _, e)| e.alive())
            .map(|(e, _, en)| (e, en.centre().distance(at)))
            .filter(|(_, d)| *d < REMOTE_TARGET_REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(e, _)| e)
    });
    if weapon == ROUND_PLANET_BUSTER {
        // (A Riot Cannon's Planet Buster: its flash kept on his cannon's muzzle.)
        let kind = &riot::PLANET_BUSTER;
        let cannon = WEAPONS.iter().position(|w| w.id == "Bazooka");
        riot::muzzle_flash(
            commands,
            weapon_fx,
            kind,
            origin,
            dir,
            cannon.map(|w| (jazz, w)),
        );
        riot::fire_round(commands, weapon_fx, kind, origin, dir, firer);
    } else if weapon == ROUND_TANK_SHELL {
        // (A tank's shell: its flash kept on his cannon.)
        riot::cannon_flash(commands, weapon_fx, origin, dir, jazz);
        riot::fire_round(commands, weapon_fx, &riot::TANK_SHELL, origin, dir, firer);
    } else if let Some(rockets) = [&JET_ROCKETS, &TRUCK_ROCKETS]
        .into_iter()
        .find(|r| r.round == weapon)
    {
        // (A jet's rocket or a truck's: its flash kept on the muzzle it left.)
        let kind = rockets.kind;
        riot::vehicle_flash(commands, weapon_fx, kind, (origin, dir), jazz, None);
        riot::fire_homing_round(commands, weapon_fx, kind, origin, dir, firer, target);
    } else if let Some(stats) = WEAPONS.get(w)
        && let Some(kind) = stats.projectile
    {
        if kind.bounce.is_some() {
            if !*blade_light {
                *blade_light = true;
                fxq.spawn(stats.muzzle_fx, origin, dir);
            }
            riot::fire_blade(commands, weapon_fx, kind, origin, dir, firer);
            return;
        }
        fxq.spawn(stats.muzzle_fx, origin, dir);
        riot::muzzle_flash(commands, weapon_fx, kind, origin, dir, Some((jazz, w)));
        match velocity {
            Some(v) => riot::launch(
                commands,
                weapon_fx,
                kind,
                (origin, v, dir),
                firer,
                None,
                riot::seed_of(origin, dir),
            ),
            None => riot::fire_homing_round(commands, weapon_fx, kind, origin, dir, firer, target),
        }
    }
}

/// Another player's shot (his JazzEvent::Shot: the weapon, or the car's guns; where
/// it left, its way and how far it went; what it struck, and the enemy here it struck
/// by its number) as it plays out here: its flash, shell, tracer and impact, and when
/// it struck an enemy, its damage to that one (or, none numbered here, to the one
/// nearest where it struck). A shotgun's further pellets (SHOT_PELLET) bring no flash or
/// shell of their own; a Nucleon Charge Rifle's round strikes as hard as its
/// charge level (SHOT_LEVEL_MASK) says. `side` is the way his shells fly; `jazz`
/// is the Jazz who fired, where his feet are (his shells land there, and a gun
/// with the game's own shot effects keeps its muzzle flash on his gun's muzzle) and
/// the middle of his robot (where a gun's falloff is measured from).
/// An Orbital Pistol's round marks the sentry here too (orbital.rs). `mods` are his
/// weapon's upgrades (a range upgrade's longer falloff, the pistol's marks).
#[allow(clippy::too_many_arguments)]
fn remote_shot(
    commands: &mut Commands,
    sfx: &mut Sfx,
    (fxq, weapon_fx): (&mut FxQueue, &mut levelfx::FxSystems),
    enemies: &mut Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    (level, colliders, barrels): (&Level, &Colliders, &mut barrels::Barrels),
    weapon: u8,
    (origin, dir, distance): (Vec3, Vec3, f32),
    (struck, numbered): (u8, Option<Entity>),
    side: Vec3,
    (jazz, feet, centre): (Entity, Vec3, Vec3),
    (barrel, hit_fx): (Option<[Vec3; 2]>, &mut bool),
    mods: upgrades::Mods,
) {
    let floor = feet.y;
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }
    let distance = distance.clamp(0.0, 1000.0);
    let hit = origin + dir * distance;
    let first = struck & SHOT_PELLET == 0;
    let start_fire = struck & SHOT_START != 0;
    let charge_level = ((struck & SHOT_LEVEL_MASK) >> SHOT_LEVEL_SHIFT) as usize;
    let stats = WEAPONS.get(weapon as usize);
    // A special shot (specials.rs): its own effects, and what it does.
    let special = (struck & SHOT_SPECIAL != 0)
        .then(|| stats.and_then(|w| specials::Special::of(w.id)))
        .flatten();
    let special_fx = special.and_then(|k| k.fx());
    let struck = struck & !(SHOT_PELLET | SHOT_LEVEL_MASK | SHOT_START | SHOT_SPECIAL);
    let level_fx = stats.and_then(|w| w.level_fx);
    let marks = stats.is_some_and(|w| w.marks);
    let gear = stats.and_then(|w| w.gear);
    // A new gun's flash on the barrel it fired from and its effects on its sockets
    // with every shot, as on his PC.
    if let (Some(gear), true) = (gear, first) {
        if let (Some(fx), Some(at)) = (level_fx, barrel)
            && !fx.muzzle.is_empty()
        {
            arsenal::play_flash_on_socket(
                commands,
                jazz,
                weapon as usize,
                &arsenal::barrel_fx(fx.muzzle, at),
            );
        }
        for on_shot in gear.on_shot {
            arsenal::play_on_socket(commands, jazz, weapon as usize, on_shot);
        }
    }
    let mut struck_entity = None;
    let (impact_world, impact_enemy, impact_fx, tracer, damage) = match stats {
        Some(w) => {
            if first && special_fx.is_none() {
                fxq.spawn(w.muzzle_fx, origin, dir);
                // (The game's own flash, on the barrel it left, and on the gun's
                // other flash socket if it has one: the Subsonic Repeater's
                // MuzzleFlash2.)
                if let Some(system) = w.muzzle_system {
                    let other = w.sockets.and_then(|s| s.flash).map(|(_, at)| at);
                    for at in [barrel, other].into_iter().flatten() {
                        arsenal::play_flash_on_socket(
                            commands,
                            jazz,
                            weapon as usize,
                            &arsenal::barrel_fx(system, at),
                        );
                    }
                }
                if fxq.chance(w.shell.chance) {
                    fxq.shell(
                        w.shell.effect,
                        origin - dir * w.muzzle_reach * 0.7,
                        side,
                        Some(floor),
                    );
                }
            }
            (
                w.sound_impact_world,
                w.sound_impact_enemy,
                w.impact_fx,
                w.tracer,
                match special {
                    // (Lights Out's round does no damage of its own.)
                    Some(specials::Special::LightsOut) => 0.0,
                    _ => shot_damage(w, charge_level, distance / mods.range),
                },
            )
        }
        None => {
            // (A car's guns: their light, and the game's flash kept on the gun
            // nearest where it left.)
            fxq.spawn(Effect::CarMuzzle, origin, dir);
            riot::car_gun_flash(commands, weapon_fx, (origin, dir), jazz, None);
            (
                SND_VEHICLE_IMPACT,
                SND_VEHICLE_HIT,
                Effect::BulletImpact,
                TracerKind::Bullet,
                VEHICLE_DAMAGE,
            )
        }
    };
    let mut impact = None;
    // (A special shot's impact sound comes with its effects.)
    let own_impact_sound = special_fx.is_none_or(|f| f.sound_impact.is_empty());
    match struck {
        2 => {
            if level_fx.is_none() && special_fx.is_none() {
                fxq.spawn(impact_fx, hit, -dir);
            }
            impact = Some((hit, -dir));
            if own_impact_sound {
                sfx.play_at(impact_enemy, hit);
            }
            let nearest = numbered
                .filter(|e| enemies.get(*e).is_ok_and(|(_, _, en)| en.alive()))
                .or_else(|| {
                    enemies
                        .iter()
                        .filter(|(_, _, e)| e.alive())
                        .map(|(entity, _, e)| (entity, e.reach(hit)))
                        .filter(|(_, d)| *d < 2.0)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(entity, _)| entity)
                });
            struck_entity = nearest;
            // (A new gun's effect as its rounds wound, once a frame: the Energon
            // Harvester's heal glow.)
            if let Some(on_hit) = gear.and_then(|g| g.on_hit.as_ref())
                && !*hit_fx
            {
                arsenal::play_on_socket(commands, jazz, weapon as usize, on_hit);
                *hit_fx = true;
            }
            if let Some(entity) = nearest
                && let Ok((_, _, mut enemy)) = enemies.get_mut(entity)
            {
                let at = enemy.centre();
                // (A gun that measures its falloff as the game does: from his centre
                // to the enemy's.)
                let damage = match stats {
                    Some(w) if marks => {
                        shot_damage(w, charge_level, centre.distance(at) / mods.range)
                    }
                    _ => damage,
                };
                if damage > 0.0 && special == Some(specials::Special::Fireblast) {
                    enemy.burn.ignite(jazz);
                }
                // (Its weapon's damage type, momentum and gib distance; a Fireblast
                // pellet's TnDamageTypeFireShotgun.)
                let (death, momentum, gib) = match stats {
                    _ if special == Some(specials::Special::Fireblast) => (
                        enemies::Death::Fire,
                        enemies::weapon_momentum("Shotgun"),
                        0.0,
                    ),
                    Some(w) => (
                        enemies::Death::of_weapon(w.id),
                        enemies::weapon_momentum(w.id),
                        enemies::gib_reach(w.id),
                    ),
                    None => (enemies::Death::Gunfire, 0.0, 0.0),
                };
                let damage_type = match stats {
                    _ if special == Some(specials::Special::Fireblast) => "TnDamageTypeFireShotgun",
                    Some(w) => killfeed::weapon_damage(w.id),
                    None => "",
                };
                enemy.take(
                    damage,
                    Some(jazz),
                    xp::Blow::Weapon,
                    enemies::Hit::new(death, hit, dir)
                        .with_momentum(momentum)
                        .gibs_within(gib, centre, at)
                        .of_type(damage_type),
                );
                if marks {
                    enemy.mark.hit(jazz, &mods);
                }
            }
        }
        1 => {
            // The wall's normal there, as the shooter's game found it.
            let normal = level
                .shot_surface(colliders, origin, dir, distance + 0.5)
                .filter(|(t, _)| (t - distance).abs() < 0.5)
                .map_or(-dir, |(_, n)| n);
            impact = Some((hit, normal));
            // An exploding barrel it struck: here too (barrels.rs).
            if let Some(b) = barrels.struck_at(hit) {
                let damage = match stats {
                    Some(w) if marks && special != Some(specials::Special::LightsOut) => {
                        shot_damage(w, charge_level, centre.distance(hit) / mods.range)
                    }
                    _ => damage,
                };
                let kind = match stats {
                    _ if special == Some(specials::Special::Fireblast) => "TnDamageTypeFireShotgun",
                    Some(w) => killfeed::weapon_damage(w.id),
                    None => "TnDamageTypeCarMachineGun",
                };
                barrels.strike(b, damage, kind, barrels::By::Player(jazz));
            }
            if level_fx.is_none() && special_fx.is_none() {
                fxq.spawn(impact_fx, hit, normal);
                if let Some(hole) = fx::hole_for(impact_fx) {
                    fxq.spawn(hole, hit, normal);
                }
            }
            if own_impact_sound {
                sfx.play_at(impact_world, hit);
            }
        }
        _ => {}
    }
    // Lights Out's shockwave where it struck (or at its range's end), here too.
    if special == Some(specials::Special::LightsOut) && first {
        specials::shockwave(enemies, (hit, Some(jazz)));
    }
    match (special_fx, level_fx) {
        (Some(kind), _) => {
            // (Its shell from about where his gun's port is.)
            let port = stats.map(|w| (origin - dir * w.muzzle_reach * 0.7, side));
            specials::play_fx(
                commands,
                (weapon_fx, sfx),
                kind,
                (origin, dir),
                (dir, distance),
                impact,
                first.then_some((jazz, weapon as usize)),
                (first, port),
            )
        }
        (None, Some(kind)) => {
            let tracer = first
                && match kind.tracer_at {
                    orbital::TracerAt::EveryShot => true,
                    orbital::TracerAt::StartFire => start_fire,
                    orbital::TracerAt::Held => false,
                };
            orbital::shot_fx(
                commands,
                weapon_fx,
                kind,
                (origin, dir),
                (dir, distance),
                impact,
                first.then_some((jazz, weapon as usize)),
                (first && gear.is_none(), tracer),
            )
        }
        (None, None) => fxq.tracer(tracer, origin, dir, distance),
    }
    // A beam's chains, to the enemies here round its line.
    if let Some(beam) = gear.and_then(|g| g.beam) {
        let interval = stats.map_or(0.07, |w| w.fire_interval);
        arsenal::chain_round(
            &beam,
            interval,
            (origin, hit),
            (struck_entity, Some(jazz)),
            level,
            colliders,
            enemies,
        );
    }
}

/// Online: hands this PC's Jazz over for the other players (his snapshot, and what
/// he did this frame with the sounds he made), and clears every Jazz's events.
fn net_capture(
    mut net: ResMut<net::Net>,
    mut sfx: ResMut<Sfx>,
    mut players: Query<(
        &Transform,
        &mut Player,
        &car::Car,
        &PlayerInput,
        Has<LocalPlayer>,
    )>,
) {
    let sounds = sfx.end_capture();
    let in_match = net.in_match();
    for (transform, mut player, car, input, local) in &mut players {
        let played = std::mem::take(&mut player.played);
        if !local || !in_match {
            continue;
        }
        net.set_local(snapshot_of(transform, &player, car, input));
        for event in played {
            net.push_event(event);
        }
        for (cue, delay, scale) in &sounds {
            net.push_event(net::JazzEvent::Sound {
                cue: cue.to_string(),
                delay: *delay,
                scale: *scale,
            });
        }
    }
}

/// A Jazz as the other players' PCs need him (net::JazzSnapshot).
fn snapshot_of(
    transform: &Transform,
    p: &Player,
    car: &car::Car,
    input: &PlayerInput,
) -> net::JazzSnapshot {
    use net::flag;
    let flags = [
        (flag::GROUNDED, p.grounded),
        (flag::VEHICLE, p.form == Form::Vehicle),
        (flag::SPRINTING, p.sprinting),
        (flag::AIMING, p.aiming),
        (flag::FIRING, p.firing),
        (flag::LEFT_HANDED, p.left_handed),
        (flag::DYING, p.dying),
        (flag::HOVER, car.hover),
        (flag::TRIGGER, input.fire),
        (flag::CHARGING, p.charging),
        (flag::CHARGED, p.charged),
        (flag::WARMING, p.gear.warming),
        (flag::BLADES_LOW, p.gear.blades & 1 != 0),
        (flag::BLADES_HIGH, p.gear.blades & 2 != 0),
        (flag::WAITING, p.waiting),
    ]
    .into_iter()
    .filter(|(_, on)| *on)
    .fold(0, |bits, (bit, _)| bits | bit);
    net::JazzSnapshot {
        pos: transform.translation,
        heading: p.heading,
        velocity: p.velocity,
        vertical_velocity: p.vertical_velocity,
        flags,
        transform_left: p.transform_left,
        transform_clip: p.transform_clip as u8,
        move_input: p.move_input,
        twist: p.twist,
        aim_pitch: p.aim_pitch,
        pivot_clip: p.pivot.map_or(u8::MAX, |pivot| pivot.clip as u8),
        action_clip: p.action.map_or(u8::MAX, |(clip, _)| clip as u8),
        action_left: p.action.map_or(0.0, |(_, left)| left),
        jump_time: p.jump_time.unwrap_or(-1.0),
        air_time: p.air_time,
        weapon: p.weapon as u8,
        hand: p.hand as u8,
        loadout: [p.loadout[0] as u8, p.loadout[1] as u8],
        upgrades: p.upgrades,
        dead_left: p.dead_left,
        health: p.health,
        shield: p.shield,
        hover_vel: car.hover_vel,
        wheel_angle: p.wheel_angle,
        yaw_rate: p.yaw_rate,
        drive_speed: p.drive_speed,
        strafe_speed: p.strafe_speed,
        steer: p.steer,
        carry_vel: p.carry_vel,
        look: p.look.to_bytes(),
        // (net.rs puts in the class the player has chosen and his match so far.)
        class: u8::MAX,
        stats: net::MatchStats::default(),
        revive: p.revive.snap(),
        ability: p.ability.snap(),
    }
}

/// Every Jazz's trigger: his gun (or the car's) fires along his view, striking
/// the first sentry or wall in the way.
#[allow(clippy::too_many_arguments)]
fn fire(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    mut players: Query<
        (
            Entity,
            &Transform,
            &mut Player,
            &mut car::Car,
            &PlayerInput,
            Has<LocalPlayer>,
            Option<&split::Seat>,
        ),
        Without<RemotePlayer>,
    >,
    hands: Query<(&GlobalTransform, &MuzzleBone, &JazzOf)>,
    gun_bones: Query<(&GlobalTransform, &GunBone, &JazzOf)>,
    cannons: CannonBones,
    colliders: Res<Colliders>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    mut sfx: ResMut<Sfx>,
    mut fxq: ResMut<FxQueue>,
    mut huds: split::BySeat<HudState>,
    mut weapon_fx: ResMut<levelfx::FxSystems>,
    (chars, numbered, mut barrels): (
        Res<character::Characters>,
        Query<&waves::WaveFoe>,
        ResMut<barrels::Barrels>,
    ),
) {
    for (owner, player_transform, mut player, mut car, input, local, seat) in &mut players {
        // (His HUD: split screen's guest's his own.)
        let hud = huds.seat(split::seat_of(seat));
        let voice = voice_at(local, player_transform.translation);
        // A jet's rockets and a truck's fire their own way (a jet's locking on as
        // they do).
        if let Some(rockets) = vehicle_rockets(&player) {
            player.fire_cooldown -= time.delta_secs();
            let sentries = arsenal::live_targets(enemies.iter().map(|(e, _, en)| (e, en)));
            let shot = Cannon {
                owner,
                at: player_transform,
                input,
                local,
            };
            fire_rockets(
                &mut commands,
                &shot,
                &mut player,
                (&car, rockets),
                (&level, &colliders, &chars, &sentries),
                (&mut sfx, &mut *hud, &mut weapon_fx),
                (time.elapsed_secs(), time.delta_secs()),
            );
            continue;
        }
        // (A lock is a homing weapon's in his hand: the Thermo Rocket Cannon's, below;
        // putting it away or changing form drops it: WeaponPuttingDown, Inactive.)
        let homing_gun = WEAPONS
            .get(player.weapon)
            .and_then(|w| w.gear)
            .and_then(|g| g.lock)
            .filter(|_| player.form == Form::Robot);
        if homing_gun.is_none() {
            player.lock.stop();
        }
        // A tank's cannon fires its own way.
        if player.form == Form::Vehicle && player.kit.tank {
            player.fire_cooldown -= time.delta_secs();
            let shot = Cannon {
                owner,
                at: player_transform,
                input,
                local,
            };
            fire_cannon(
                &mut commands,
                &shot,
                &mut player,
                &mut car,
                (&level, &cannons, &chars),
                (&mut sfx, &mut *hud, &mut weapon_fx),
                time.elapsed_secs(),
            );
            continue;
        }
        // The elbow carrying his gun (its sockets), right or left.
        let elbow = gun_bones
            .iter()
            .find(|(_, b, of)| b.0 == player.left_handed && of.0 == owner)
            .map(|(t, _, _)| *t);
        let arm = player.left_handed as usize;
        let dt = time.delta_secs();
        // While a gun reloads it steams out of its socket for that, where that is
        // now: a puff every quarter of a second through its first 1.5 s (the
        // steam's EmitterDuration), stopping if the reload does (each of its
        // emitters letting out no more puffs than its cooked peak holds: fx.rs's
        // STEAM).
        if let Some(since) = player.steam {
            let weapon = WEAPONS.get(player.reload_slot);
            if player.reload_left <= 0.0 || player.form != Form::Robot || since > STEAM_SECS {
                player.steam = None;
            } else {
                player.steam = Some(since + dt);
                if let (Some(sockets), Some(steam), Some(elbow)) = (
                    weapon.and_then(|w| w.sockets),
                    weapon.and_then(|w| w.reload_steam),
                    elbow,
                ) {
                    for puff in steam_puffs(since, since + dt) {
                        let at = elbow.transform_point(sockets.steam[arm]);
                        fxq.spawn_puff(steam, at, Vec3::Y, puff);
                    }
                }
            }
        }
        // And each shot whose shell steams lets steam out of the shell port for
        // 1.5 s, while that gun stays in hand: a wisp at a time (the Scatter
        // Blaster's bigger shells two), the next as the last goes.
        if !player.shell_steam.is_empty() {
            let (form, weapon) = (player.form, player.weapon);
            let port = WEAPONS
                .get(weapon)
                .and_then(|w| w.sockets)
                .zip(elbow)
                .map(|(sockets, elbow)| elbow.transform_point(sockets.shell[arm]));
            let steam = match WEAPONS.get(weapon).map(|w| w.shell.effect) {
                Some(Effect::MedShell) => Effect::MedShellSteam,
                _ => Effect::ShellSteam,
            };
            player
                .shell_steam
                .retain(|&(since, w)| form == Form::Robot && w == weapon && since <= STEAM_SECS);
            for (since, _) in player.shell_steam.iter_mut() {
                if let Some(port) = port {
                    for _ in 0..shell_steam_wisps(*since, *since + dt) {
                        fxq.spawn(steam, port, Vec3::Y);
                    }
                }
                *since += dt;
            }
        }
        player.fire_cooldown -= dt;
        update_charge(&mut player, dt);
        // (A new gun's state is its own: it starts over as he changes weapons, but for
        // his rounds out, which riot.rs counts.)
        if player.gear.weapon != player.weapon {
            player.gear = arsenal::GearState {
                weapon: player.weapon,
                alt: player.gear.alt,
                alt_was: player.gear.alt_was,
                grenades: player.gear.grenades,
                in_flight: player.gear.in_flight,
                ..default()
            };
        }
        let slot = gun_slot(&player);
        // (Not while he spins his Whirlwind, abilities.rs, nor while he swings his
        // sword or punches: the gun carries the blade; nor while he reacts to a
        // melee blow, hitreact.rs: his shooting is off till it completes, the
        // trigger held firing again then.)
        let ready = input.control
            && player.transform_left <= 0.0
            && player.dead_left <= 0.0
            && player.reload_left <= 0.0
            && !player.ability.blocks_attacks()
            && !player.meleeing()
            && !player.melee_react.reacting();
        // The fine-aim button's press for a gun that does something else with it
        // (TryAltFirePress).
        let alt_pressed = player.gear.alt && !player.gear.alt_was;
        player.gear.alt_was = player.gear.alt;
        let in_hand_gear = WEAPONS
            .get(player.weapon)
            .and_then(|w| w.gear)
            .filter(|_| player.form == Form::Robot);
        // Ready, as the game's Active state is: not firing (its refire wait), reloading,
        // put away (sprinting: TnWeapon.Holster), coming out or switching hands.
        let active = ready
            && player.fire_cooldown <= 0.0
            && !player.holstered
            && player.equipping.is_none()
            && player.hand_switch.is_none();
        // A launcher's alt fire sets off its rounds (AttemptToDetonate): its sound for
        // some out, its click for none; every one of them goes off (riot.rs).
        if let Some(d) = in_hand_gear.and_then(|g| g.detonate)
            && alt_pressed
        {
            let cue = if player.gear.grenades > 0 {
                d.sound
            } else {
                d.sound_none
            };
            sfx.play_from(cue, voice);
            set_off_rounds(&mut player);
        }
        // An arcing weapon: its lob shown from the fine-aim button's press while it is
        // ready to its release, or its leaving Active (TryAltFirePress,
        // Active.EndState's TryAltFireRelease); and a pull of its trigger bursts its
        // round in the air (StartFire: RequestClusterBomb, whatever its state).
        if in_hand_gear.and_then(|g| g.arc).is_some() {
            if alt_pressed && active {
                player.gear.arc = true;
            }
            if !player.gear.alt || !active {
                player.gear.arc = false;
            }
            if input.fire_pressed && input.control && player.gear.in_flight {
                set_off_rounds(&mut player);
            }
        } else {
            player.gear.arc = false;
        }
        // A homing weapon's lock (TnWeaponHoming.Tick: UpdateLockOnTarget while it is
        // Active, with a round in its clip: CanLockOn), its tone held on for the player
        // at this PC while it is locked on.
        if let Some(l) = homing_gun {
            if active {
                let live: Vec<_> = arsenal::live_targets(enemies.iter().map(|(e, _, en)| (e, en)))
                    .into_iter()
                    .filter(|t| l.robots || !t.robot)
                    .collect();
                let view = input.view_dir.normalize_or(*player_transform.forward());
                let picked = if player.ammo[slot].0 > 0 {
                    jet::pick_target(&level, &colliders, (input.view_origin, view), &live)
                } else {
                    None
                };
                // (The Omega Pulse Charge shortens its lock-on: TnBuffWeaponLockOnTime.)
                let lock_secs = l.secs * player.mods(player.weapon).lock_on;
                player.lock.update(
                    picked,
                    |e| live.iter().any(|t| t.entity == e),
                    (lock_secs, l.hold),
                    dt,
                );
            } else if !active && player.fire_cooldown <= 0.0 && player.reload_left <= 0.0 {
                // (Coming out or switching hands: Inactive, or no Active state.)
                player.lock.stop();
            }
            if local && player.lock.locked {
                sfx.hold(owner.to_bits() ^ 0x10c4, l.sound, None);
            }
        }
        // A launcher of blades (the Gear Shredder) charges and fires its own way.
        if player.form == Form::Robot
            && let Some(weapon) = WEAPONS.get(player.weapon)
            && let (Some(blades), Some(kind)) =
                (weapon.gear.and_then(|g| g.blades), weapon.projectile)
        {
            let live = arsenal::live_targets(enemies.iter().map(|(e, _, en)| (e, en)));
            let shot = Cannon {
                owner,
                at: player_transform,
                input,
                local,
            };
            fire_blades(
                &mut commands,
                &shot,
                &mut player,
                (weapon, &blades, kind),
                (&level, &colliders, &live, elbow),
                (&mut sfx, &mut *hud, &mut weapon_fx, &mut fxq),
                (ready, dt),
            );
            continue;
        }
        let burst = match player.form {
            Form::Robot => WEAPONS[player.weapon.min(WEAPONS.len() - 1)].burst,
            Form::Vehicle => None,
        };
        let held = match burst {
            // Automatic: fires while the trigger is held (and no burst is left over
            // from the weapon or form in use before).
            None => {
                player.burst_left = 0;
                player.burst_queued = false;
                ready && input.fire
            }
            // Burst fire: each pull of the trigger fires one burst, starting as soon as
            // the gun is ready (a pull during the wait between bursts counts); with
            // the Photon Accelerator, bursts follow one another while it is held
            // (TnBuffWeaponAutoFire: GetAutoFire).
            Some(b) => {
                let auto =
                    player.form == Form::Robot && player.mods(player.weapon).auto_fire.is_some();
                if !ready {
                    player.burst_left = 0;
                    player.burst_queued = false;
                } else if input.fire_pressed || (auto && input.fire) {
                    player.burst_queued = true;
                }
                if player.burst_left == 0 && player.burst_queued && player.fire_cooldown <= 0.0 {
                    player.burst_queued = false;
                    if player.ammo[slot].0 > 0 {
                        player.burst_left = b.shots;
                    } else {
                        // Nothing in the clip: a reload (below), or a dry click.
                        player.burst_queued = player.ammo[slot].1 > 0;
                    }
                }
                ready && (player.burst_left > 0 || player.burst_queued)
            }
        };
        // Pulling the trigger with no ammo left at all.
        if ready && input.fire_pressed && player.ammo[slot] == (0, 0) && player.form == Form::Robot
        {
            let w = &WEAPONS[player.weapon.min(WEAPONS.len() - 1)];
            if let Some(dry) = w.sound_dry {
                sfx.play_from(dry, voice);
            }
            if let Some(gear) = w.gear
                && !gear.sound_dry.is_empty()
            {
                sfx.play_from(gear.sound_dry, voice);
            }
        }
        // An empty clip reloads by itself (bAutoReload: HmWeapon.TryReload), held
        // or not: a fire interval after its last round, when the refire check finds
        // it empty (WeaponFiring.RefireCheckTimer: a burst's wait after a burst), or
        // as soon as the gun is ready with it empty (Active.BeginState).
        if ready && player.ammo[slot].0 == 0 && player.fire_cooldown <= 0.0 {
            start_reload(
                &mut player,
                slot,
                &mut sfx,
                local.then_some(&mut *hud),
                voice,
            );
        }
        let held = held && player.ammo[slot].0 > 0 && player.reload_left <= 0.0;
        // (Shooting gives a cloak away and lowers a shield: abilities.rs.)
        if held && local {
            abilities::attacked(&mut player);
        }
        // Robot-form shooting drives the stance and the gun's animations.
        let was_firing = player.firing;
        player.firing = held && player.form == Form::Robot && player.hand_switch.is_none();
        let in_hand = &WEAPONS[player.weapon.min(WEAPONS.len() - 1)];
        // A ramping fire interval runs on while the gun fires and back while it rests
        // (TnWeapon.UpdateFireInterval).
        // (The Dropshot runs it faster while it fires: TnBuffWeaponFireIntervalRamp.)
        if let (Some(r), Form::Robot) = (in_hand.ramp, player.form) {
            let step = if player.firing {
                dt * player.mods(player.weapon).ramp_rate
            } else {
                -dt
            };
            player.gear.ramp = (player.gear.ramp + step).clamp(0.0, r.secs);
        }
        if !player.firing && was_firing && player.form == Form::Robot {
            // The firing stops (its WeaponFiring state ends): a new gun's StopFire if
            // it fired, its StopCharging if it was still warming up.
            let (fired, warming) = (player.gear.fired, player.gear.warming);
            player.gear.fired = false;
            player.gear.warming = false;
            // Let go of the trigger: the rifle's barrels spin down, and the X18
            // Scrapmaker's (at StopFire and StopCharging alike) and the TechVolt's
            // (StopFire), unless a reload just took over the gun.
            let cools = in_hand.gear.is_none() || fired || warming;
            if cools
                && player.reload_left <= 0.0
                && let Some((clip, secs)) = in_hand.cooldown_anim
            {
                player.start_weapon_clip = Some((clip, secs, 1.0));
            }
        }
        if !held {
            if let Some(end) = player.burst_end.take() {
                sfx.play_from(end, voice);
            }
            continue;
        }
        if player.fire_cooldown > 0.0 {
            continue;
        }

        // Where the crosshair points: the first wall along the player's view, or the
        // point 150 m away (as far as the gun reaches, if further: the Nucleon Charge
        // Rifle's 300 m). (Only a gun with no muzzle socket needs it: its muzzle is
        // put out from his hand towards it.)
        let weapon = &WEAPONS[player.weapon.min(WEAPONS.len() - 1)];
        // Its fire interval now (a ramping one's where its ramp has got to:
        // GetFireInterval).
        let ramp = weapon.ramp.filter(|_| player.form == Form::Robot);
        let interval_now = match ramp {
            Some(r) => {
                let k = (player.gear.ramp / r.secs.max(1e-4)).min(1.0);
                weapon.fire_interval + (r.end - weapon.fire_interval) * k
            }
            None => weapon.fire_interval,
        };
        // (A fire interval upgrade, Sonic Blast, scales it: ScaleFireInterval.)
        let mods = match player.form {
            Form::Robot => player.mods(player.weapon),
            Form::Vehicle => upgrades::Mods::default(),
        };
        let interval_now = interval_now * mods.fire_interval;
        // Still spinning up: the X18 Scrapmaker fires no round while its interval is
        // over MinFireIntervalToStartFiring, only warms up (TnWeaponHeavyMG.
        // FireAmmunition: StartCharging as it starts, its wind-up and WarmUp clip),
        // and tries again an interval later (its refire timer).
        if let Some(r) = ramp
            && interval_now > r.warm_until
        {
            if !player.gear.warming {
                player.gear.warming = true;
                player.gear.windup = arsenal::WINDUP_HOLD_SECS;
                if let Some((clip, secs)) = weapon.gear.and_then(|g| g.warmup_anim) {
                    player.start_weapon_clip = Some((clip, secs, 1.0));
                }
            }
            let late = if player.fire_cooldown > -dt {
                player.fire_cooldown
            } else {
                0.0
            };
            player.fire_cooldown = (late + interval_now).max(interval_now * 0.5);
            continue;
        }
        // A round: the firing's first is its StartFire (the warm-up is over: no
        // StopCharging; StartFire stops the wind-up).
        let start_fire = player.form == Form::Robot && !player.gear.fired;
        if player.form == Form::Robot {
            player.gear.warming = false;
            player.gear.fired = true;
            if start_fire {
                player.gear.windup = 0.0;
            }
        }
        let view = input.view_dir.normalize_or(*player_transform.forward());
        let reach = match player.form {
            Form::Robot => (weapon.range * mods.range).max(150.0),
            Form::Vehicle => 150.0,
        };
        let aim_dist = level.ray(input.view_origin, view, reach).unwrap_or(reach);
        let aim_point = input.view_origin + view * aim_dist;
        // (The barrel the round leaves, on each arm: a new gun's flash plays on it;
        // in a vehicle, which of its guns: the car's guns' flash plays on it.)
        let mut barrel_used: Option<[Vec3; 2]> = None;
        let mut vehicle_gun: Option<usize> = None;
        let origin = match player.form {
            // With animations the weapon moves, so fire from the end of the barrel:
            // its next barrel's socket (taking turns), or else nudged forward from the
            // hand that holds it. Without them, use a fixed point.
            Form::Robot => match (weapon.sockets, elbow) {
                (Some(sockets), Some(elbow)) => {
                    let barrel = player.muzzle as usize % sockets.muzzles.len();
                    player.muzzle = player.muzzle.wrapping_add(1);
                    barrel_used = Some(sockets.muzzles[barrel]);
                    elbow.transform_point(sockets.muzzles[barrel][arm])
                }
                _ => match hands
                    .iter()
                    .find(|(_, m, of)| m.0 == player.left_handed && of.0 == owner)
                    .map(|(t, _, _)| t)
                {
                    Some(hand) => {
                        hand.translation()
                            + (aim_point - hand.translation()).normalize_or_zero()
                                * weapon.muzzle_reach
                    }
                    None => player_transform.transform_point(ROBOT_MUZZLE),
                },
            },
            Form::Vehicle => {
                // (Its own vehicle's guns, or Jazz's car's.)
                let muzzles = player
                    .kit
                    .vehicle
                    .and_then(|v| chars.vehicles.get(v))
                    .map(|v| v.muzzles.as_slice())
                    .filter(|m| !m.is_empty())
                    .unwrap_or(&VEHICLE_MUZZLES);
                let k = player.next_vehicle_gun % muzzles.len();
                let gun = muzzles[k];
                vehicle_gun = Some(k);
                player.next_vehicle_gun += 1;
                // Wherever the body is (raised while hovering, pitched in a jump).
                player_transform.transform_point(car.on_body(gun))
            }
        };
        // Rounds go where the crosshair points, as the game fires them
        // (Weapon.InstantFire and ProjectileFire): traced along the camera's line,
        // with the gun's spread, from the point of it level with him (shot_start), so
        // they strike whatever is under the crosshair however far his gun is from that
        // line. What is seen still sets off from the muzzle: each round's tracer runs
        // from there to where its trace struck, and the Riot Cannon's round flies
        // there. The car's guns too: the game's trace starts the same way for
        // whatever weapon he has (TnPlayerPawn.GetWeaponStartTraceLocation, level
        // with the pawn, which in the car is the car), so its rounds go up, down and
        // across with the view, as far as the camera turns.
        let centre = match player.form {
            Form::Robot => player.robot_centre(player_transform.translation),
            Form::Vehicle => player_transform.transform_point(car.on_body(Vec3::ZERO)),
        };
        let start = shot_start(
            &level,
            &colliders,
            (input.view_origin, view),
            centre,
            origin,
        );
        let aim = view;
        // The gun's spread (tighter in fine aim, wider in the air), opening up with
        // every shot.
        let g = player.gun(slot);
        let spread = spread_now(&player, &g.spread);
        let seed = (time.elapsed_secs() * 7919.0) as u32
            ^ (player.ammo[slot].0.wrapping_mul(2_654_435_761));
        // A shotgun's pellets fly in its pattern around the aim, the pattern scaled
        // by the spread; other guns' rounds go off at random within it.
        let pellets = match player.form {
            Form::Robot => weapon.pellets,
            Form::Vehicle => &[],
        };
        let direction = if pellets.is_empty() {
            spread_shot(aim, spread, seed)
        } else {
            aim
        };
        let shots: Vec<Vec3> = if pellets.is_empty() {
            vec![direction]
        } else {
            pellet_dirs(aim, pellets, spread / g.spread.min.max(1e-4)).collect()
        };
        player.spread = (player.spread + g.spread.per_shot).min(g.spread.max);
        // One round from the clip; the shot sounds thinner when the clip runs low.
        let left = player.ammo[slot].0;
        player.ammo[slot].0 = left.saturating_sub(1);
        // The special shot (specials.rs: TnWeapon.FireAmmunition), fired in the
        // weapon's special fire mode: the clip's last round with Lights Out or
        // Planet Buster, a quarter of the shots with Fireblast.
        let special = (player.form == Form::Robot
            && ((mods.last_shot_special && left == 1)
                || (mods.random_shot_special
                    && hash01(seed.wrapping_add(0x5EED)) < specials::FIREBLAST_CHANCE)))
            .then(|| specials::Special::of(weapon.id))
            .flatten();
        let special_fx = special.and_then(|k| k.fx());
        let shot_sound = match special_fx {
            Some(f) if !f.sound_shot.is_empty() => f.sound_shot,
            _ if left <= g.low_ammo => g.sound_shot_low,
            _ => g.sound_shot,
        };
        // A burst: its sound and recoil start with the first round; after the last
        // comes the wait before the next burst.
        let mut first_of_burst = true;
        let mut burst_done = false;
        if let Some(b) = burst {
            first_of_burst = player.burst_left == b.shots;
            if first_of_burst {
                player.start_weapon_clip = Some((b.anim.0, b.anim.1, 1.0));
                // The rounds of a burst keep their spacing however long the gun rested.
                player.fire_cooldown = player.fire_cooldown.max(0.0);
            }
            player.burst_left = player.burst_left.saturating_sub(1);
            // The burst also ends when the clip runs dry.
            if player.ammo[slot].0 == 0 {
                player.burst_left = 0;
            }
            burst_done = player.burst_left == 0;
        }
        // Keep the leftover time so the fire rate stays exact at any frame rate: how
        // late this round is, if the wait for it ran out during this frame. (A gun
        // that was ready before isn't late: its next round waits the whole interval,
        // as the game's refire timer does, rather than catching up on the rest.)
        let interval = match (player.form, burst) {
            // (The Photon Accelerator's bursts wait its share: BurstDelaying.)
            (Form::Robot, Some(b)) if burst_done => b.delay * mods.auto_fire.unwrap_or(1.0),
            (Form::Robot, _) => interval_now,
            (Form::Vehicle, _) => VEHICLE_FIRE_DELAY,
        };
        let late = if player.fire_cooldown > -dt {
            player.fire_cooldown
        } else {
            0.0
        };
        player.fire_cooldown = (late + interval).max(interval * 0.5);
        // (A range upgrade reaches further: GetTraceRange.)
        let range = match player.form {
            Form::Robot => weapon.range * mods.range,
            Form::Vehicle => VEHICLE_RANGE,
        };
        // What each round strikes (each pellet of a shotgun's), and the line the shot
        // is seen along: from the muzzle to where its first round struck.
        let traced: Vec<Traced> = shots
            .iter()
            .map(|&dir| trace_shot(&level, &colliders, &enemies, start, dir, range))
            .collect();
        let seen = traced.first().map_or(direction, |t| t.seen_from(origin).0);
        // (Where a special shot's shell leaves from, and the way it is thrown.)
        let mut special_port = None;
        let (impact_world, impact_enemy, impact_fx, tracer) = match player.form {
            Form::Robot => {
                if first_of_burst || burst.is_some_and(|b| b.sound_each) {
                    sfx.play_from(shot_sound, voice);
                }
                // The gun's one-off clip for every shot, and the camera's kick (for
                // the player at this PC).
                if let Some((clip, secs)) = weapon.shot_anim {
                    player.start_weapon_clip = Some((clip, secs, 1.0));
                }
                // (The Quantum Stabilizer takes the kick's pitch down:
                // GetRecoilCameraParams' TnBuffReducedRecoil.)
                // (A special shot kicks as from the hip, fine aiming or not: there is
                // no FineAimCameraRecoils entry for its fire mode; and shakes by its
                // own, if it has one.)
                if local && let Some((hip, fine)) = weapon.recoil {
                    let mut r = if player.aiming && special.is_none() {
                        fine
                    } else {
                        hip
                    };
                    r.pitch *= mods.recoil;
                    player.camera_kick += r.pitch.to_radians();
                    player.recoil = Some((r, 0.0));
                }
                let shake = special.and_then(|k| k.shake()).or(weapon.shake);
                if local && shake.is_some() {
                    player.shake = shake;
                }
                player.burst_end = (!weapon.sound_end.is_empty()).then_some(weapon.sound_end);
                // Muzzle flash and light at the barrel, now and then a spent shell out
                // to the side of the hand holding the gun (from its shell port), and
                // the shell's steam. (A special shot with effects of its own plays
                // them instead, below: specials.rs.)
                let side = if player.left_handed { -1.0 } else { 1.0 };
                let port = match (weapon.sockets, elbow) {
                    (Some(sockets), Some(elbow)) => elbow.transform_point(sockets.shell[arm]),
                    _ => origin - seen * weapon.muzzle_reach * 0.7,
                };
                special_port = Some((port, *player_transform.right() * side));
                if special_fx.is_none() {
                    fxq.spawn(weapon.muzzle_fx, origin, seen);
                    // (The game's own flash, on the barrel it fired from.)
                    if let (Some(system), Some(at)) = (weapon.muzzle_system, barrel_used) {
                        arsenal::play_flash_on_socket(
                            &mut commands,
                            owner,
                            player.weapon,
                            &arsenal::barrel_fx(system, at),
                        );
                    }
                    if let (Some((flash, at)), Some(elbow)) =
                        (weapon.sockets.and_then(|s| s.flash), elbow)
                    {
                        fxq.spawn(flash, elbow.transform_point(at[arm]), seen);
                        // (A gun whose flash is the game's own plays it there too:
                        // the Subsonic Repeater's on its MuzzleFlash2.)
                        if let Some(system) = weapon.muzzle_system {
                            arsenal::play_flash_on_socket(
                                &mut commands,
                                owner,
                                player.weapon,
                                &arsenal::barrel_fx(system, at),
                            );
                        }
                    }
                    if fxq.chance(weapon.shell.chance) {
                        fxq.shell(
                            weapon.shell.effect,
                            port,
                            *player_transform.right() * side,
                            Some(player_transform.translation.y),
                        );
                    }
                    if weapon.shell.steam > 0.0 && fxq.chance(weapon.shell.steam) {
                        let fired = player.weapon;
                        player.shell_steam.push((0.0, fired));
                    }
                }
                // A new gun's flash on the barrel it fired from, and its effects on
                // its sockets with every shot (arsenal.rs keeps them there).
                if let Some(gear) = weapon.gear {
                    if let (Some(fx), Some(at)) = (weapon.level_fx, barrel_used)
                        && !fx.muzzle.is_empty()
                    {
                        arsenal::play_flash_on_socket(
                            &mut commands,
                            owner,
                            player.weapon,
                            &arsenal::barrel_fx(fx.muzzle, at),
                        );
                    }
                    for on_shot in gear.on_shot {
                        arsenal::play_on_socket(&mut commands, owner, player.weapon, on_shot);
                    }
                }
                (
                    weapon.sound_impact_world,
                    weapon.sound_impact_enemy,
                    weapon.impact_fx,
                    weapon.tracer,
                )
            }
            Form::Vehicle => {
                sfx.play_from(shot_sound, voice);
                if local {
                    player.shake = Some(VEHICLE_SHAKE);
                }
                // (Its light; the flash is the game's own, kept on the gun it left.)
                fxq.spawn(Effect::CarMuzzle, origin, seen);
                let nose = player_transform.rotation * car.tilt() * Vec3::NEG_Z;
                riot::car_gun_flash(
                    &mut commands,
                    &mut weapon_fx,
                    (origin, nose),
                    owner,
                    vehicle_gun,
                );
                (
                    SND_VEHICLE_IMPACT,
                    SND_VEHICLE_HIT,
                    Effect::BulletImpact,
                    TracerKind::Bullet,
                )
            }
        };

        // A weapon whose rounds fly (the Riot Cannon's): the round sets off from the
        // muzzle (riot.rs) towards what the crosshair is on (ProjectileFire turns it
        // to where its trace struck), the muzzle flash with it, kept on the muzzle as
        // the gun recoils.
        if let (Form::Robot, Some(kind)) = (player.form, weapon.projectile) {
            // (The Planet Buster: the special shot's round.)
            let special_round = special.and_then(|k| k.round());
            let kind = special_round.unwrap_or(kind);
            let firer = riot::Firer {
                owner: Some(owner),
                shooter: Some(owner),
                decepticon: player.look.decepticon,
                mods: riot::RoundMods::of(&mods),
            };
            let fired = player.weapon.min(WEAPONS.len() - 1) as u8;
            let event = match weapon.gear.and_then(|g| g.arc) {
                // An arcing weapon's lob (ServerSpawnedProjectile: CalculateTossVelocity).
                Some(arc) => {
                    let velocity = lob_velocity(
                        &level,
                        &colliders,
                        &enemies,
                        (origin, seen),
                        (weapon.range, arc.flight_secs),
                        kind.gravity,
                    );
                    let dir = velocity.normalize_or(seen);
                    riot::launch(
                        &mut commands,
                        &mut weapon_fx,
                        kind,
                        (origin, velocity, dir),
                        firer,
                        None,
                        riot::seed_of(origin, dir),
                    );
                    net::JazzEvent::Lob {
                        weapon: fired,
                        origin,
                        velocity,
                    }
                }
                // A homing weapon's round takes its lock (ServerSpawnedProjectile: its
                // target), which the shot drops (WeaponFiring.BeginState: StopLockOn).
                None if weapon.gear.is_some_and(|g| g.lock.is_some()) => {
                    let target = player.lock.homing();
                    player.lock.stop();
                    riot::fire_homing_round(
                        &mut commands,
                        &mut weapon_fx,
                        kind,
                        origin,
                        seen,
                        firer,
                        target,
                    );
                    let target_at = target
                        .and_then(|t| enemies.get(t).ok())
                        .map(|(_, t, _)| t.translation);
                    match target_at {
                        Some(target) => net::JazzEvent::HomingRound {
                            weapon: fired,
                            origin,
                            dir: seen,
                            target,
                        },
                        None => net::JazzEvent::Round {
                            weapon: fired,
                            origin,
                            dir: seen,
                        },
                    }
                }
                None => {
                    riot::fire_round(&mut commands, &mut weapon_fx, kind, origin, seen, firer);
                    net::JazzEvent::Round {
                        weapon: if special_round.is_some() {
                            ROUND_PLANET_BUSTER
                        } else {
                            fired
                        },
                        origin,
                        dir: seen,
                    }
                }
            };
            let barrel = muzzle_socket(&player, owner, &gun_bones).map_or(seen, |(_, d)| d);
            riot::muzzle_flash(
                &mut commands,
                &mut weapon_fx,
                kind,
                origin,
                barrel,
                Some((owner, player.weapon)),
            );
            player.played.push(event);
            continue;
        }

        // The Nucleon Charge Rifle's round strikes as hard as it had charged.
        let charge_level = player.charge_level;
        // A gun playing the game's own shot effects (the Orbital Pistol's: orbital.rs)
        // has its muzzle flash kept on its muzzle, turned along the barrel.
        let level_fx = match player.form {
            Form::Robot => weapon.level_fx,
            Form::Vehicle => None,
        };
        let barrel = muzzle_socket(&player, owner, &gun_bones).map_or(seen, |(_, d)| d);
        let marks = player.form == Form::Robot && weapon.marks;
        let centre = player.centre(player_transform.translation);
        // A new gun's: what it does as its rounds wound (the Energon Harvester's heal,
        // the TechVolt's crackle), and a beam's chains.
        let gear = match player.form {
            Form::Robot => weapon.gear,
            Form::Vehicle => None,
        };
        let mut wounded = 0u32;
        // (Where a Lights Out shot went off: its shockwave, after its rounds.)
        let mut shockwave_at = None;
        // Every gun in the demo is an instant-hit weapon in the game (WeaponFireTypes
        // EWFT_InstantHit): each round (each pellet of a shotgun's) strikes the first
        // thing along its line at once, with its own impact and tracer
        // (CreateTracerForEachImpact).
        for (pellet, shot) in traced.into_iter().enumerate() {
            let wall = shot.surface.map(|(t, _)| t);
            let struck = shot.struck;
            let hit = shot.hit();
            // The round as it is seen, from the muzzle to where it struck: its tracer,
            // and what the other players' PCs are told.
            let (line, distance) = shot.seen_from(origin);
            // Where it struck and the surface's normal there (the impact plays along
            // it: EffectOrientation HitNormal).
            let mut impact = None;
            if let Some(entity) = struck {
                if let Ok((_, _, mut enemy)) = enemies.get_mut(entity) {
                    let at = enemy.centre();
                    // (Its falloff by the game's distance, for a gun that measures it;
                    // the others' from the muzzle.)
                    let reach = if marks { centre.distance(at) } else { distance };
                    // (A range upgrade stretches the falloff as far as the reach:
                    // GetRangeDamageModifier, the distance divided by it. Lights Out's
                    // round does no damage of its own: its shockwave does it.)
                    let damage = match player.form {
                        _ if special == Some(specials::Special::LightsOut) => 0.0,
                        Form::Robot => shot_damage(weapon, charge_level, reach / mods.range),
                        Form::Vehicle => VEHICLE_DAMAGE,
                    };
                    // A marked enemy takes more; then the round marks it. (The car
                    // guns' rounds kill as gunfire.)
                    let wounds = damage > 0.0 && enemy.alive();
                    // (Its weapon's damage type, momentum and gib distance; a
                    // Fireblast pellet's TnDamageTypeFireShotgun; the car guns' data
                    // gives them no momentum.)
                    let (death, momentum, gib) = match player.form {
                        Form::Robot if special == Some(specials::Special::Fireblast) => (
                            enemies::Death::Fire,
                            enemies::weapon_momentum("Shotgun"),
                            0.0,
                        ),
                        Form::Robot => (
                            enemies::Death::of_weapon(weapon.id),
                            enemies::weapon_momentum(weapon.id),
                            enemies::gib_reach(weapon.id),
                        ),
                        Form::Vehicle => (enemies::Death::Gunfire, 0.0, 0.0),
                    };
                    let damage_type = match player.form {
                        Form::Robot if special == Some(specials::Special::Fireblast) => {
                            "TnDamageTypeFireShotgun"
                        }
                        Form::Robot => killfeed::weapon_damage(weapon.id),
                        Form::Vehicle => held_damage(&player),
                    };
                    enemy.take(
                        damage,
                        Some(owner),
                        xp::Blow::Weapon,
                        enemies::Hit::new(death, hit, line)
                            .with_momentum(momentum)
                            .gibs_within(gib, centre, at)
                            .of_type(damage_type),
                    );
                    if wounds {
                        wounded += 1;
                        // (A Fireblast pellet sets it alight: TnDamageTypeFireShotgun.)
                        if special == Some(specials::Special::Fireblast) {
                            enemy.burn.ignite(owner);
                        }
                    }
                    if marks {
                        enemy.mark.hit(owner, &mods);
                    }
                    // (Deadeye: a round striking an enemy speeds the rifle's charge.)
                    if let Some((_, secs)) = mods.hits_charge {
                        player.deadeye_left = secs;
                    }
                    if local {
                        hud.events.push(HudEvent::Hit);
                    }
                    if special_fx.is_none_or(|f| f.sound_impact.is_empty()) {
                        sfx.play_at(impact_enemy, hit);
                    }
                    let normal = (hit - at).normalize_or(-line);
                    impact = Some((hit, normal));
                    if level_fx.is_none() && special_fx.is_none() {
                        fxq.spawn(impact_fx, hit, normal);
                    }
                }
            } else if let Some((_, normal)) = shot.surface {
                impact = Some((hit, normal));
                // An exploding barrel it struck takes its damage, as an enemy would
                // (barrels.rs).
                if let Some(b) = barrels.struck_at(hit) {
                    let reach = if marks {
                        centre.distance(hit)
                    } else {
                        distance
                    };
                    let (damage, kind) = match player.form {
                        _ if special == Some(specials::Special::LightsOut) => (0.0, ""),
                        Form::Robot if special == Some(specials::Special::Fireblast) => (
                            shot_damage(weapon, charge_level, reach / mods.range),
                            "TnDamageTypeFireShotgun",
                        ),
                        Form::Robot => (
                            shot_damage(weapon, charge_level, reach / mods.range),
                            killfeed::weapon_damage(weapon.id),
                        ),
                        Form::Vehicle => (VEHICLE_DAMAGE, held_damage(&player)),
                    };
                    barrels.strike(b, damage, kind, barrels::By::Player(owner));
                }
                if level_fx.is_none() && special_fx.is_none() {
                    fxq.spawn(impact_fx, hit, normal);
                    // The rifle's, the Subsonic Repeater's and the car guns' rounds
                    // leave bullet holes, the Path Blaster's scorch marks.
                    if let Some(hole) = fx::hole_for(impact_fx) {
                        fxq.spawn(hole, hit, normal);
                    }
                }
                if special_fx.is_none_or(|f| f.sound_impact.is_empty()) {
                    sfx.play_at(impact_world, hit);
                }
            }
            if special == Some(specials::Special::LightsOut) && pellet == 0 {
                shockwave_at = Some(hit);
            }
            match (special_fx, level_fx) {
                // A special shot's own flash, shell, tracer and impact.
                (Some(kind), _) => specials::play_fx(
                    &mut commands,
                    (&mut weapon_fx, &mut sfx),
                    kind,
                    (origin, barrel),
                    (line, distance),
                    impact,
                    Some((owner, player.weapon)),
                    (pellet == 0, special_port),
                ),
                // The game's muzzle flash (a new gun's is on its barrel: above),
                // tracer (out to where the shot's first round struck; a beam's is
                // held: arsenal.rs) and impact.
                (None, Some(kind)) => {
                    let flash = pellet == 0 && gear.is_none();
                    let tracer = pellet == 0
                        && match kind.tracer_at {
                            orbital::TracerAt::EveryShot => true,
                            orbital::TracerAt::StartFire => start_fire,
                            orbital::TracerAt::Held => false,
                        };
                    orbital::shot_fx(
                        &mut commands,
                        &mut weapon_fx,
                        kind,
                        (origin, barrel),
                        (line, distance),
                        impact,
                        Some((owner, player.weapon)),
                        (flash, tracer),
                    )
                }
                // The tracer flies out at its own speed and stops where the shot
                // struck.
                (None, None) => fxq.tracer(tracer, origin, line, distance),
            }
            // A beam: its end is where this round struck (the beam reaches there
            // until the next), and it chains to the sentries round it.
            if let Some(beam) = gear.and_then(|g| g.beam) {
                player.gear.beam_end = Some(origin + line * distance);
                wounded += arsenal::chain_round(
                    &beam,
                    interval_now,
                    (origin, origin + line * distance),
                    (struck, Some(owner)),
                    &level,
                    &colliders,
                    &mut enemies,
                );
            }
            // (For the other players: the weapon, or the car's guns, and what it
            // struck, the shotgun's pellets after the first marked as such, and the
            // Nucleon Charge Rifle's charge level. Worked out before the push, which
            // borrows the Jazz mutably.)
            let shot_weapon = match player.form {
                Form::Robot => player.weapon.min(WEAPONS.len() - 1) as u8,
                Form::Vehicle => u8::MAX,
            };
            let what = if struck.is_some() {
                2
            } else if wall.is_some() {
                1
            } else {
                0
            };
            let what = what | (((charge_level as u8) << SHOT_LEVEL_SHIFT) & SHOT_LEVEL_MASK);
            let what = if start_fire { what | SHOT_START } else { what };
            let what = if special.is_some() {
                what | SHOT_SPECIAL
            } else {
                what
            };
            let shot_struck = if pellet > 0 { what | SHOT_PELLET } else { what };
            // (The enemy it struck by its number: the same one on every PC.)
            let enemy = struck
                .and_then(|e| numbered.get(e).ok())
                .map_or(0, |w| w.id as u16);
            player.played.push(net::JazzEvent::Shot {
                weapon: shot_weapon,
                origin,
                dir: line,
                distance,
                struck: shot_struck,
                enemy,
            });
        }
        // Lights Out's shockwave where its round struck (or at its range's end).
        if let Some(at) = shockwave_at
            && specials::shockwave(&mut enemies, (at, Some(owner))) > 0
            && local
        {
            hud.events.push(HudEvent::Hit);
        }
        // Its rounds that wounded: the Energon Harvester's heal (HealthPerHit for each,
        // TnPawn.HealDamage: up to his full health) and its glow on his gun (HitEnemy),
        // the TechVolt's crackle.
        if let (Some(gear), true) = (gear, wounded > 0) {
            if gear.heal > 0.0 {
                // (The Reaper drains more: TnBuffDrainGunIncreaseDrain.)
                let heal = gear.heal * player.mods(player.weapon).drain;
                player.health = (player.health + heal * wounded as f32).min(player.max_health());
            }
            if let Some(on_hit) = gear.on_hit.as_ref() {
                arsenal::play_on_socket(&mut commands, owner, player.weapon, on_hit);
            }
            if !gear.sound_hit.is_empty() {
                sfx.play_from(gear.sound_hit, voice);
            }
        }
    }
}

/// A Jazz's weapon sets off its rounds that are out (his A-4 Pulsar Cannon's alt fire,
/// his Chaos-Rift Combustor's trigger): riot.rs sets them off on this PC, and the other
/// players' PCs are told.
fn set_off_rounds(player: &mut Player) {
    let Some(kind) = WEAPONS.get(player.weapon).and_then(|w| w.projectile) else {
        return;
    };
    player.set_off = Some(kind);
    player.played.push(net::JazzEvent::SetOff {
        weapon: player.weapon.min(WEAPONS.len() - 1) as u8,
    });
}

/// The velocity an arcing weapon lobs its round at, fired from `origin` along `aim`
/// (TnWeaponTankCannonArcable.CalculateTossVelocity, from ServerSpawnedProjectile):
/// to what it would strike along its aim within its `range` (CalcWeaponFire: the
/// level or a sentry; nothing, the range's end), in a flight time from
/// `flight_secs.0` to `.1` by how far that is over the range (LerpFloatRange), under
/// `gravity` times the world's.
pub(crate) fn lob_velocity(
    level: &Level,
    colliders: &Colliders,
    enemies: &Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    (origin, aim): (Vec3, Vec3),
    (range, flight_secs): (f32, (f32, f32)),
    gravity: f32,
) -> Vec3 {
    let sentries: Vec<enemies::Target> = enemies
        .iter()
        .filter(|(_, _, e)| e.alive())
        .map(|(entity, _, e)| e.target(entity))
        .collect();
    lob_velocity_among(
        level,
        colliders,
        &sentries,
        (origin, aim),
        (range, flight_secs),
        gravity,
    )
}

/// lob_velocity with the enemies given.
pub(crate) fn lob_velocity_among(
    level: &Level,
    colliders: &Colliders,
    sentries: &[enemies::Target],
    (origin, aim): (Vec3, Vec3),
    (range, flight_secs): (f32, (f32, f32)),
    gravity: f32,
) -> Vec3 {
    let aim = aim.normalize_or(Vec3::NEG_Z);
    let mut reach = level
        .shot_surface(colliders, origin, aim, range)
        .map_or(range, |(t, _)| t);
    for target in sentries {
        if let Some(t) = target.body.ray(origin, aim, reach) {
            reach = t;
        }
    }
    let k = (reach / range.max(1e-3)).clamp(0.0, 1.0);
    let secs = flight_secs.0 + (flight_secs.1 - flight_secs.0) * k;
    riot::toss_velocity(origin, origin + aim * reach, secs, gravity)
}

/// A launcher of blades in a Jazz's hand (the Gear Shredder: TnWeaponSawblade,
/// arsenal.rs's Blades). A pull of the trigger while it is ready (Active: its fire
/// interval and equip time over, not reloading) starts it charging, a blade loaded
/// at once (Charging.BeginState: SetNumberOfBlades(1); AddBlades: StartCharging, its
/// clip and loop, and AmmoLoaded's sound), another every 0.65 s the trigger is held,
/// up to three and no more than the clip holds (Charging.Tick); letting go
/// (Charging.StopFire) fires them all (rounds from their fire modes' muzzles, the
/// last loaded first) for a round each, the shot's sound and clip by how many
/// (StartFire, FireShot), its light, the camera's kick and shake, then its fire
/// interval passes before it is ready again. With the clip empty a pull does
/// nothing (NoAmmoFire: SawBlade_WEPMESH has nothing for it), and an empty clip
/// reloads by itself, as the other guns'. A pull while it isn't ready is lost (its
/// StartFire does nothing out of Active). A reload drops the blades (WeaponReloading.
/// BeginState), as does Jazz going down (HolderDied) and, here, the gun leaving his
/// hand in any way (the game keeps them loaded through a weapon swap:
/// Charging.TryPutDown puts it down without dropping them). While it charges Jazz
/// is `firing`: the shooting stance, its charging clip, 0.85 of his speed
/// (ChargingGroundSpeedMultiplier, WeaponStats::firing_speed) and no sprinting
/// (get_IsSprintDisabled).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn fire_blades(
    commands: &mut Commands,
    shot: &Cannon,
    player: &mut Player,
    (weapon, blades, kind): (&WeaponStats, &arsenal::Blades, &'static riot::RoundKind),
    (level, colliders, sentries, elbow): (
        &Level,
        &Colliders,
        &[enemies::Target],
        Option<GlobalTransform>,
    ),
    (sfx, hud, weapon_fx, fxq): (
        &mut Sfx,
        &mut HudState,
        &mut levelfx::FxSystems,
        &mut FxQueue,
    ),
    (ready, dt): (bool, f32),
) {
    let (input, local) = (shot.input, shot.local);
    let feet = shot.at.translation;
    let voice = voice_at(local, feet);
    let slot = gun_slot(player);
    let can = ready && player.hand_switch.is_none();
    if !can && player.gear.blades > 0 {
        player.gear.blades = 0;
        player.gear.charge = 0.0;
    }
    if can
        && player.gear.blades == 0
        && input.fire_pressed
        && player.fire_cooldown <= 0.0
        && player.ammo[slot].0 > 0
    {
        player.gear.blades = 1;
        player.gear.charge = 0.0;
        sfx.play_from(blades.sound_loaded[0], voice);
    }
    if player.gear.blades > 0 {
        if input.fire {
            player.gear.charge += dt;
            // (The Torque Ripper loads them faster: TnBuffSawbladeChargeTime.)
            let saw = player.mods(player.weapon).saw_charge;
            let n = blades.loaded(player.gear.charge / saw.max(1e-3), player.ammo[slot].0);
            if n > player.gear.blades {
                player.gear.blades = n;
                sfx.play_from(blades.sound_loaded[n as usize - 1], voice);
            }
        } else {
            shoot_blades(
                commands,
                shot,
                player,
                (weapon, blades, kind),
                (level, colliders, sentries, elbow),
                (sfx, weapon_fx, fxq),
            );
        }
    }
    // (As the other guns', an empty clip reloads itself once the gun is ready.)
    if ready && player.gear.blades == 0 && player.ammo[slot].0 == 0 && player.fire_cooldown <= 0.0 {
        start_reload(player, slot, sfx, local.then_some(hud), voice);
    }
    let charging = player.gear.blades > 0;
    player.firing = charging;
    player.gear.fired = charging;
    player.burst_left = 0;
    player.burst_queued = false;
}

/// A launcher of blades' shot as its trigger is let go (TnWeaponSawblade.
/// FireAmmunition, then SetNumberOfBlades(0): StopCharging): the shot's sound and
/// clip by how many blades (StartFire, FireShot, the fire mode the last loaded
/// blade's), its light on the first muzzle, the camera's kick and shake (for the
/// player at this PC), then a round for each blade from its fire mode's muzzle, the
/// last loaded first (GetProjectileFireParameters, then SingleFireAction taking the
/// fire mode down a step), aimed as arsenal.rs's blade_aim has it; as many rounds
/// out of the clip (ShotCost), and the fire interval to wait.
#[allow(clippy::type_complexity)]
fn shoot_blades(
    commands: &mut Commands,
    shot: &Cannon,
    player: &mut Player,
    (weapon, blades, kind): (&WeaponStats, &arsenal::Blades, &'static riot::RoundKind),
    (level, colliders, sentries, elbow): (
        &Level,
        &Colliders,
        &[enemies::Target],
        Option<GlobalTransform>,
    ),
    (sfx, weapon_fx, fxq): (&mut Sfx, &mut levelfx::FxSystems, &mut FxQueue),
) {
    let (owner, input, local) = (shot.owner, shot.input, shot.local);
    let feet = shot.at.translation;
    let voice = voice_at(local, feet);
    let slot = gun_slot(player);
    let n = (player.gear.blades as usize).clamp(1, 3);
    let mode = n - 1;
    sfx.play_from(blades.sound_shot[mode], voice);
    let (clip, secs) = blades.shot_anims[mode.min(blades.shot_anims.len() - 1)];
    player.start_weapon_clip = Some((clip, secs, 1.0));
    if local && let Some((hip, fine)) = weapon.recoil {
        let r = if player.aiming { fine } else { hip };
        player.camera_kick += r.pitch.to_radians();
        player.recoil = Some((r, 0.0));
    }
    if local && weapon.shake.is_some() {
        player.shake = weapon.shake;
    }
    let view = input.view_dir.normalize_or(*shot.at.forward());
    let centre = player.robot_centre(feet);
    let arm = player.left_handed as usize;
    let muzzle = |k: usize| {
        match (weapon.sockets, elbow) {
            (Some(sockets), Some(elbow)) => sockets
                .muzzles
                .get(k)
                .map(|m| elbow.transform_point(m[arm])),
            _ => None,
        }
        .unwrap_or_else(|| shot.at.transform_point(ROBOT_MUZZLE))
    };
    let barrel = match elbow {
        Some(elbow) => (muzzle(0) - elbow.translation()).normalize_or(view),
        None => view,
    };
    fxq.spawn(weapon.muzzle_fx, muzzle(0), barrel);
    let fired = player.weapon.min(WEAPONS.len() - 1) as u8;
    for k in (0..n).rev() {
        let origin = muzzle(k);
        let dir = arsenal::blade_aim(
            blades,
            k,
            (input.view_origin, view),
            centre,
            origin,
            weapon.range,
            (level, colliders),
            sentries,
        );
        let firer = riot::Firer {
            owner: Some(owner),
            shooter: Some(owner),
            decepticon: player.look.decepticon,
            mods: riot::RoundMods::of(&player.mods(player.weapon)),
        };
        riot::fire_blade(commands, weapon_fx, kind, origin, dir, firer);
        player.played.push(net::JazzEvent::Round {
            weapon: fired,
            origin,
            dir,
        });
    }
    player.ammo[slot].0 = player.ammo[slot].0.saturating_sub(n as u32);
    player.fire_cooldown = weapon.fire_interval;
    player.gear.blades = 0;
    player.gear.charge = 0.0;
}

/// The tanks' cannon bones (car::CannonBone), by whose tank they are.
type CannonBones<'w, 's> =
    Query<'w, 's, (&'static GlobalTransform, &'static JazzOf), With<car::CannonBone>>;

/// The Jazz firing a tank's cannon (fire_cannon): his entity, where he is, what he
/// asks for, and whether he plays at this PC.
struct Cannon<'a> {
    owner: Entity,
    at: &'a Transform,
    input: &'a PlayerInput,
    local: bool,
}

/// Where a tank's shells leave from: the end of its barrel (its muzzle socket on the
/// cannon bone, as car::aim_cannons turned it), or else its muzzle point (the
/// vehicle's vpoint, where the body is).
fn cannon_muzzle(
    player: &Player,
    owner: Entity,
    at: &Transform,
    car: &car::Car,
    cannons: &CannonBones,
    chars: &character::Characters,
) -> Option<Vec3> {
    let vehicle = player.kit.vehicle.and_then(|v| chars.vehicles.get(v))?;
    let bone = cannons.iter().find(|(_, of)| of.0 == owner).map(|(g, _)| g);
    match (vehicle.tank.as_ref(), bone) {
        (Some(tank), Some(bone)) => Some(bone.transform_point(tank.muzzle.transform.translation)),
        _ => vehicle
            .muzzles
            .first()
            .map(|&m| at.transform_point(car.on_body(m))),
    }
}

/// A tank's cannon (TnWeaponTankCannon): a shell for each pull of the trigger
/// (AutoFire 0) while one is loaded, from the end of its barrel towards what the
/// crosshair is on (out to its Range) within its spread, flying as the Riot
/// Cannon's rounds do (riot.rs's TANK_SHELL); its muzzle flash kept on the barrel,
/// its shot (always the low-ammo one) and the camera's shake; then the next shell
/// loads (its reload clip playing on the tank, car.rs). Firing stops a boost and
/// sets the tank drifting (TnTankForm.OnWeaponFired: StopBoost, Recoil, Drift; its
/// RecoilVelocity is 0). With none loaded the trigger reloads, with none at all it
/// clicks.
fn fire_cannon(
    commands: &mut Commands,
    shot: &Cannon,
    player: &mut Player,
    car: &mut car::Car,
    (level, cannons, chars): (&Level, &CannonBones, &character::Characters),
    (sfx, hud, weapon_fx): (&mut Sfx, &mut HudState, &mut levelfx::FxSystems),
    now: f32,
) {
    let voice = voice_at(shot.local, shot.at.translation);
    let ready = shot.input.control
        && player.transform_left <= 0.0
        && player.dead_left <= 0.0
        && player.reload_left <= 0.0;
    player.firing = false;
    if !(ready && shot.input.fire_pressed) {
        return;
    }
    let slot = TANK_SLOT;
    let (clip, reserve) = player.ammo[slot];
    if clip == 0 {
        if reserve == 0 {
            sfx.play_from(SND_TANK_DRY, voice);
        } else {
            start_reload(player, slot, sfx, shot.local.then_some(&mut *hud), voice);
        }
        return;
    }
    let Some(origin) = cannon_muzzle(player, shot.owner, shot.at, car, cannons, chars) else {
        return;
    };
    let view = shot.input.view_dir.normalize_or(*shot.at.forward());
    let reach = level
        .ray(shot.input.view_origin, view, TANK_RANGE)
        .unwrap_or(TANK_RANGE);
    let aim = (shot.input.view_origin + view * reach - origin).normalize_or(view);
    let seed = (now * 7919.0) as u32 ^ reserve.wrapping_mul(2_654_435_761);
    let dir = spread_shot(aim, player.spread, seed);
    player.spread = (player.spread + TANK_GUN.spread.per_shot).min(TANK_GUN.spread.max);
    player.ammo[slot].0 = clip - 1;
    let sound = if clip <= TANK_GUN.low_ammo {
        TANK_GUN.sound_shot_low
    } else {
        TANK_GUN.sound_shot
    };
    sfx.play_from(sound, voice);
    if shot.local {
        player.shake = Some(TANK_SHAKE);
    }
    let firer = riot::Firer {
        owner: Some(shot.owner),
        shooter: Some(shot.owner),
        decepticon: player.look.decepticon,
        mods: riot::RoundMods::NONE,
    };
    riot::fire_round(commands, weapon_fx, &riot::TANK_SHELL, origin, dir, firer);
    riot::cannon_flash(commands, weapon_fx, origin, dir, shot.owner);
    player.played.push(net::JazzEvent::Round {
        weapon: ROUND_TANK_SHELL,
        origin,
        dir,
    });
    car.cannon_fired = true;
    car.drift = player.kit.handling.drift_secs;
    start_reload(player, slot, sfx, shot.local.then_some(hud), voice);
}

/// A vehicle's rockets (`rockets`): a jet's homing ones (TnWeaponHomingRocket-
/// Vehicle), which while in hand lock on to the sentry under the crosshair (jet.rs's
/// Lock, the lock tone held on for the player at this PC), or a truck's
/// (TnWeaponRocketVehicle). Each pull of the trigger fires a burst of them from its
/// muzzles by turns towards what the crosshair is on (out to its Range) within its
/// spread (a jet's homing on the sentry locked on: riot.rs's JET_ROCKET; a truck's
/// flying straight: TRUCK_ROCKET), each with its muzzle flash kept on its muzzle,
/// its shot and the camera's shake (and a jet's recoil). An empty clip reloads by
/// itself; with no rockets at all the trigger clicks.
#[allow(clippy::too_many_arguments)]
fn fire_rockets(
    commands: &mut Commands,
    shot: &Cannon,
    player: &mut Player,
    (car, rockets): (&car::Car, &Rockets),
    (level, colliders, chars, sentries): (
        &Level,
        &Colliders,
        &character::Characters,
        &[enemies::Target],
    ),
    (sfx, hud, weapon_fx): (&mut Sfx, &mut HudState, &mut levelfx::FxSystems),
    (now, dt): (f32, f32),
) {
    let voice = voice_at(shot.local, shot.at.translation);
    let together = player.transform_left <= 0.0 && player.dead_left <= 0.0;
    let ready = shot.input.control && together && player.reload_left <= 0.0;
    player.firing = false;
    let slot = rockets.slot;
    // A jet's lock (TnWeaponHoming.Tick: while it stands ready, Active, and while it
    // fires, UpdateLockOnWhileFiring; held as it is while it reloads or waits out a
    // burst's delay, BurstDelaying; none with its clip empty, CanLockOn).
    let delaying = player.burst_left == 0
        && player.fire_cooldown > 0.0
        && player.fire_cooldown <= rockets.burst_delay;
    let view = shot.input.view_dir.normalize_or(*shot.at.forward());
    if together && rockets.locks_on && player.reload_left <= 0.0 && !delaying {
        let picked = if player.ammo[slot].0 > 0 {
            jet::pick_target(level, colliders, (shot.input.view_origin, view), sentries)
        } else {
            None
        };
        player.lock.update(
            picked,
            |e| sentries.iter().any(|t| t.entity == e),
            jet::JET_LOCK,
            dt,
        );
    }
    if !together || !rockets.locks_on {
        player.lock.stop();
    }
    if shot.local && player.lock.locked {
        sfx.hold(shot.owner.to_bits() ^ 0x10c4, SND_JET_LOCK, None);
    }
    // A burst for each pull of the trigger, as soon as it is ready.
    if !ready {
        player.burst_left = 0;
        player.burst_queued = false;
        return;
    }
    // The clip emptied reloads by itself (bAutoReload: HmWeapon.TryReload), a fire
    // interval after its last rocket (WeaponFiring.RefireCheckTimer), or as soon as
    // the vehicle is ready with it empty (Active.BeginState).
    let (clip, reserve) = player.ammo[slot];
    if clip == 0 && reserve > 0 && player.fire_cooldown <= rockets.burst_delay {
        player.burst_left = 0;
        start_reload(player, slot, sfx, shot.local.then_some(&mut *hud), voice);
        return;
    }
    if rocket_burst_starts(player, rockets, shot.input.fire_pressed, shot.input.fire) {
        let (clip, reserve) = player.ammo[slot];
        if clip == 0 {
            if reserve == 0 {
                sfx.play_from(SND_ROCKET_DRY, voice);
            } else {
                start_reload(player, slot, sfx, shot.local.then_some(&mut *hud), voice);
            }
            return;
        }
        player.burst_left = rockets.burst;
        // (Its rockets keep their spacing however long it rested.)
        player.fire_cooldown = player.fire_cooldown.max(0.0);
    }
    if player.burst_left == 0 || player.fire_cooldown > 0.0 {
        return;
    }
    let Some(vehicle) = player.kit.vehicle.and_then(|v| chars.vehicles.get(v)) else {
        return;
    };
    if vehicle.muzzles.is_empty() {
        return;
    }
    let muzzle = player.next_vehicle_gun % vehicle.muzzles.len();
    player.next_vehicle_gun += 1;
    let origin = shot
        .at
        .transform_point(car.on_body(vehicle.muzzles[muzzle]));
    let reach = level
        .ray(shot.input.view_origin, view, rockets.range)
        .unwrap_or(rockets.range);
    let aim = (shot.input.view_origin + view * reach - origin).normalize_or(view);
    let clip = player.ammo[slot].0;
    let seed = (now * 7919.0) as u32 ^ clip.wrapping_mul(2_654_435_761);
    let gun = rockets.gun;
    let dir = spread_shot(aim, spread_now(player, &gun.spread), seed);
    player.spread = (player.spread + gun.spread.per_shot).min(gun.spread.max);
    player.ammo[slot].0 = clip - 1;
    let sound = if clip <= gun.low_ammo {
        gun.sound_shot_low
    } else {
        gun.sound_shot
    };
    sfx.play_from(sound, voice);
    if shot.local {
        player.shake = Some(rockets.shake);
        if let Some(recoil) = rockets.recoil {
            player.recoil = Some((recoil, 0.0));
        }
    }
    let target = if rockets.locks_on {
        player.lock.homing()
    } else {
        None
    };
    let firer = riot::Firer {
        owner: Some(shot.owner),
        shooter: Some(shot.owner),
        decepticon: player.look.decepticon,
        mods: riot::RoundMods::NONE,
    };
    riot::fire_homing_round(
        commands,
        weapon_fx,
        rockets.kind,
        origin,
        dir,
        firer,
        target,
    );
    riot::vehicle_flash(
        commands,
        weapon_fx,
        rockets.kind,
        (origin, dir),
        shot.owner,
        Some(muzzle),
    );
    // (Homing, its copies home on the enemy nearest where it is there.)
    let target_at = target
        .and_then(|t| sentries.iter().find(|s| s.entity == t))
        .map(|s| s.at);
    player.played.push(match target_at {
        Some(target) => net::JazzEvent::HomingRound {
            weapon: rockets.round,
            origin,
            dir,
            target,
        },
        None => net::JazzEvent::Round {
            weapon: rockets.round,
            origin,
            dir,
        },
    });
    rocket_fired(player, rockets, dt);
}

/// A vehicle's rocket gone (fire_rockets): one less of its burst (none once its
/// clip is empty), and the wait for the next, its leftover time kept, as fire's:
/// the rate stays exact at any frame rate. After a burst's last rocket the refire
/// check still comes a fire interval later, and only then does the burst's delay
/// start (WeaponFiring.RefireCheckTimer, LeaveWeaponFireState, BurstDelaying).
fn rocket_fired(player: &mut Player, rockets: &Rockets, dt: f32) {
    player.burst_left = player.burst_left.saturating_sub(1);
    if player.ammo[rockets.slot].0 == 0 {
        player.burst_left = 0;
    }
    let interval = if player.burst_left == 0 {
        rockets.interval + rockets.burst_delay
    } else {
        rockets.interval
    };
    let late = if player.fire_cooldown > -dt {
        player.fire_cooldown
    } else {
        0.0
    };
    player.fire_cooldown = (late + interval).max(interval * 0.5);
}

/// Whether a vehicle's rockets start a burst now (fire_rockets): once the last
/// burst and its delay are done, for a pull of the trigger (one during the wait
/// counts) or, with AutoFire, the trigger still held then (the refire check's
/// PendingFire). A trigger held is no pull of its own: held through a burst and let
/// go before the next is due, it fires no more. (Latched while held, a click that
/// lasted into the first rocket's interval set a second burst going: four rockets
/// for one pull.)
fn rocket_burst_starts(player: &mut Player, rockets: &Rockets, pressed: bool, held: bool) -> bool {
    if pressed {
        player.burst_queued = true;
    }
    let wanted = player.burst_queued || (rockets.auto && held);
    if player.burst_left == 0 && wanted && player.fire_cooldown <= 0.0 {
        player.burst_queued = false;
        return true;
    }
    false
}

/// How far ahead of the muzzle, along the view, has to be clear for a robot's shot
/// to be traced from the camera's line (TnPlayerPawn.GetWeaponStartTraceLocation's
/// 250 units).
const MUZZLE_CLEAR: f32 = 2.5;

/// Where a robot's shot is traced from (TnPlayerPawn.GetWeaponStartTraceLocation):
/// the point of the camera's line (`view`: where the camera is and the way it
/// looks) level with him (his middle, `centre`, projected onto it), so the rounds go
/// where the crosshair points and nothing between the camera and him is struck; but
/// from the `muzzle` when the 2.5 m ahead of it along the view are blocked, so he
/// can't shoot through a wall his gun is up against. (The game skips that check
/// while the gun is in its idle stance; Jazz always has it up as he fires.)
fn shot_start(
    level: &Level,
    colliders: &Colliders,
    (eye, view): (Vec3, Vec3),
    centre: Vec3,
    muzzle: Vec3,
) -> Vec3 {
    if level
        .shot_surface(colliders, muzzle, view, MUZZLE_CLEAR)
        .is_some()
    {
        return muzzle;
    }
    eye + view * (centre - eye).dot(view)
}

/// A round's trace: its line (from the shot's start along `dir`, unit length), how
/// far along it struck something (its range if nothing), the sentry it struck, and
/// the wall's surface (how far along and its normal) if it reached one.
struct Traced {
    start: Vec3,
    dir: Vec3,
    distance: f32,
    struck: Option<Entity>,
    surface: Option<(f32, Vec3)>,
}

impl Traced {
    fn hit(&self) -> Vec3 {
        self.start + self.dir * self.distance
    }

    /// The way from `muzzle` to where it struck, and how far: the round's line as
    /// it is seen (its tracer's).
    fn seen_from(&self, muzzle: Vec3) -> (Vec3, f32) {
        let to = self.hit() - muzzle;
        let length = to.length();
        if length > 1e-4 {
            (to / length, length)
        } else {
            (self.dir, 0.0)
        }
    }
}

/// What a round traced from `start` along `dir` strikes within `range` (the game's
/// CalcWeaponFire): the first of an enemy (its body) and a wall.
fn trace_shot(
    level: &Level,
    colliders: &Colliders,
    enemies: &Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    start: Vec3,
    dir: Vec3,
    range: f32,
) -> Traced {
    let surface = level.shot_surface(colliders, start, dir, range);
    let mut distance = surface.map_or(range, |(t, _)| t);
    let mut struck = None;
    for (entity, _, enemy) in enemies {
        if let Some(t) = enemy.ray(start, dir, distance) {
            distance = t;
            struck = Some(entity);
        }
    }
    Traced {
        start,
        dir,
        distance,
        struck,
        surface,
    }
}

/// Where the muzzle of the gun in `jazz`'s hand is and the way its barrel points
/// (from his elbow out to its first MuzzleFlash socket, GunSockets), if he holds
/// one in robot form, not moving it to his other hand, with his model in.
fn muzzle_socket(
    player: &Player,
    jazz: Entity,
    gun_bones: &Query<(&GlobalTransform, &GunBone, &JazzOf)>,
) -> Option<(Vec3, Vec3)> {
    muzzle_socket_at(player, jazz, gun_bones, 0)
}

/// The same for its muzzle `k` (GunSockets::muzzles: a launcher of blades' fire
/// mode k's), the barrel's way still out to its first.
fn muzzle_socket_at(
    player: &Player,
    jazz: Entity,
    gun_bones: &Query<(&GlobalTransform, &GunBone, &JazzOf)>,
    k: usize,
) -> Option<(Vec3, Vec3)> {
    if player.form != Form::Robot || player.hand_switch.is_some() {
        return None;
    }
    let sockets = WEAPONS.get(player.weapon)?.sockets?;
    let elbow = gun_bones
        .iter()
        .find(|(_, b, of)| b.0 == player.left_handed && of.0 == jazz)
        .map(|(t, _, _)| *t)?;
    let side = player.left_handed as usize;
    let first = elbow.transform_point(sockets.muzzles.first()?[side]);
    let at = elbow.transform_point(sockets.muzzles.get(k)?[side]);
    let dir = (first - elbow.translation()).normalize_or_zero();
    (dir != Vec3::ZERO).then_some((at, dir))
}

/// Marks a JazzEvent::Shot as a further pellet of the shot before it (its `struck`
/// flag): no muzzle flash or shell of its own.
const SHOT_PELLET: u8 = 0x80;
/// Marks a JazzEvent::Shot as its firing's first (its StartFire: the Energon
/// Harvester's tracer plays with it).
const SHOT_START: u8 = 0x40;
/// A JazzEvent::Shot's charge level (the Nucleon Charge Rifle's, which sets its
/// damage), in these bits of its `struck` flags.
const SHOT_LEVEL_SHIFT: u8 = 4;
const SHOT_LEVEL_MASK: u8 = 0x30;
/// Marks a JazzEvent::Shot as a special shot (specials.rs: Lights Out's, a
/// Fireblast's).
const SHOT_SPECIAL: u8 = 0x08;

/// The directions of a shotgun's pellets around `aim` (WeaponStats::pellets: right
/// and up, as the game's TnWeaponHud.DrawSpreadPattern lays them out), the pattern
/// `scale` times its size.
fn pellet_dirs(aim: Vec3, pattern: &[Vec2], scale: f32) -> impl Iterator<Item = Vec3> + '_ {
    let right = aim.cross(Vec3::Y).normalize_or(Vec3::X);
    let up = right.cross(aim).normalize_or(Vec3::Y);
    pattern
        .iter()
        .map(move |p| (aim + (right * p.x + up * p.y) * scale).normalize_or(aim))
}

/// A round's damage when it strikes `distance` metres out: the weapon's damage (a
/// charging weapon's at the charge `level` it was fired at) times its falloff there
/// (WeaponStats::falloff).
fn shot_damage(weapon: &WeaponStats, level: usize, distance: f32) -> f32 {
    let points = weapon.falloff;
    let scale = match points.iter().position(|&(d, _)| distance < d) {
        None => points.last().map_or(1.0, |p| p.1),
        Some(0) => points[0].1,
        Some(i) => {
            let (d0, s0) = points[i - 1];
            let (d1, s1) = points[i];
            s0 + (s1 - s0) * (distance - d0) / (d1 - d0).max(1e-4)
        }
    };
    let damage = weapon
        .charge
        .and_then(|c| c.damage.get(level).copied())
        .unwrap_or(weapon.damage);
    damage * scale
}

/// The ammo slot of the gun in use: the robot's weapon, or his vehicle's.
fn gun_slot(player: &Player) -> usize {
    match player.form {
        Form::Robot => player.weapon.min(WEAPONS.len() - 1),
        Form::Vehicle => vehicle_slot(player),
    }
}

/// The ammo slot of his vehicle's weapon: the car's guns, a tank's cannon, a jet's
/// rockets or a truck's.
fn vehicle_slot(player: &Player) -> usize {
    if player.kit.tank {
        TANK_SLOT
    } else if player.kit.plane.is_some() {
        JET_SLOT
    } else if player.kit.rockets {
        TRUCK_SLOT
    } else {
        VEHICLE_SLOT
    }
}

/// The damage type of the gun he has in hand, or his vehicle's (the kill feed's
/// words for a kill whose blow named none: killfeed.rs).
pub(crate) fn held_damage(player: &Player) -> &'static str {
    match player.form {
        Form::Robot => killfeed::weapon_damage(WEAPONS[player.weapon.min(WEAPONS.len() - 1)].id),
        Form::Vehicle => killfeed::weapon_damage(match vehicle_slot(player) {
            TANK_SLOT => "Tank",
            JET_SLOT => "Jet",
            TRUCK_SLOT => "Truck",
            _ => "Car",
        }),
    }
}

/// The DialogDamageCategory of the gun in use's damage type, which picks his line
/// for a kill (voice.rs): the guns' (DDC_Gun, the tanks' TnDamageTypeTankShell and
/// the Gear Shredder's TnDamageTypeSawBlade among them), the rockets' (the Thermo
/// Rocket Cannon's TnDamageTypeHomingRocket, a jet's and a truck's,
/// TnDamageTypeDeathBlossomRocket), the explosives' (the A-4 Pulsar Cannon's
/// TnDamageTypeGrenadeLauncher, the Chaos-Rift Combustor's TnDamageTypeClusterBomb,
/// the Corrosive Slime Cannon's TnDamageTypeSlow), and none (DDC_None: the Riot
/// Cannon's TnDamageTypeBazooka, the E.D.K. TechVolt's TnDamageTypeChainBeam).
fn dialog_category(player: &Player) -> voice::DialogCategory {
    use voice::DialogCategory as C;
    match player.form {
        Form::Robot => match WEAPONS[player.weapon].id {
            "HomingRocket" => C::Rocket,
            "GrenadeLauncher" | "ClusterCannon" | "AoESlow" => C::Explosive,
            "Bazooka" | "ChainBeam" => C::None,
            _ => C::Gun,
        },
        Form::Vehicle if vehicle_rockets(player).is_some() => C::Rocket,
        Form::Vehicle => C::Gun,
    }
}

/// His vehicle's rockets, if it fires them (a jet's or a truck's).
fn vehicle_rockets(player: &Player) -> Option<&'static Rockets> {
    match player.form {
        Form::Vehicle if player.kit.plane.is_some() => Some(&JET_ROCKETS),
        Form::Vehicle if player.kit.rockets => Some(&TRUCK_ROCKETS),
        _ => None,
    }
}

/// Every gun with a full clip and full reserve.
fn full_ammo() -> [(u32, u32); GUN_SLOTS] {
    std::array::from_fn(|slot| (gun(slot).clip_size, gun(slot).reserve_max))
}

/// The spread a shot leaves with now (the game's GetSpread): the gun's, in robot
/// form tightened in fine aim (FineAimSpreadModifier) and widened in the air
/// (TnWeaponSpreadModifier: AIRBORNE_SPREAD).
fn spread_now(player: &Player, spread: &Spread) -> f32 {
    match player.form {
        Form::Robot => {
            player.spread
                * player.airborne_spread
                * if player.aiming { spread.fine_aim } else { 1.0 }
        }
        Form::Vehicle => player.spread,
    }
}

/// A shot spread around `aim` the way Unreal's instant-hit weapons do it: up to
/// spread/2 to the side, and up or down within a circle.
fn spread_shot(aim: Vec3, spread: f32, seed: u32) -> Vec3 {
    if spread <= 0.0 {
        return aim;
    }
    let side = hash01(seed) - 0.5;
    let lift = (0.5 - side * side).sqrt() * (hash01(seed.wrapping_add(7)) - 0.5);
    let right = aim.cross(Vec3::Y).normalize_or(Vec3::X);
    let up = right.cross(aim);
    (aim + (right * side + up * lift) * spread).normalize_or(aim)
}

/// The charge of the gun in Jazz's hand, if it charges: its ChargeDelay shorter
/// while Deadeye's buff is on it (TnWeaponSniperRifle.GetChargeDelay: times the
/// buff's FloatModifier, multiplayer's 0.3), the levels it has reached and the HUD's
/// fraction going by it as it is now.
fn charged_by(player: &Player) -> Option<Charge> {
    let charge = match player.form {
        Form::Robot => WEAPONS.get(player.weapon).and_then(|w| w.charge),
        Form::Vehicle => None,
    }?;
    match player.mods(player.weapon).hits_charge {
        Some((delay, _)) if player.deadeye_left > 0.0 => Some(Charge {
            delay: charge.delay * delay,
            ..charge
        }),
        _ => Some(charge),
    }
}

/// The Nucleon Charge Rifle's charge through a frame of `dt` seconds
/// (TnWeaponSniperRifle's Active.Tick and ResetFireMode): it charges while Jazz
/// fine aims with the rifle ready (not in the wait after a shot, reloading or
/// switching), a level every ChargeDelay up to what its clip allows, and is fully
/// charged on reaching the top; it starts over (charging again if he fine aims) as
/// fine aim starts or ends and as the rifle comes back to the ready after a shot,
/// a reload, a swap or a switch of hands. Other weapons have none.
fn update_charge(player: &mut Player, dt: f32) {
    // (Deadeye's buff runs its time on the rifle wherever it is.)
    player.deadeye_left = (player.deadeye_left - dt).max(0.0);
    let charge = charged_by(player);
    let aiming = player.aiming;
    let ready = charge.is_some()
        && player.fire_cooldown <= 0.0
        && player.reload_left <= 0.0
        && player.hand_switch.is_none()
        && player.dead_left <= 0.0
        && player.transform_left <= 0.0;
    let restart = (ready && !player.charge_ready) || aiming != player.charge_aiming;
    player.charge_ready = ready;
    player.charge_aiming = aiming;
    let Some(charge) = charge else {
        player.charge_timer = 0.0;
        player.charge_level = 0;
        player.charging = false;
        player.charged = false;
        return;
    };
    if restart {
        player.charge_timer = 0.0;
        player.charge_level = 0;
        player.charging = aiming;
        player.charged = false;
    }
    if ready && aiming {
        player.charge_timer += dt;
        let clip = player.ammo[player.weapon].0;
        let level = charge.level(player.charge_timer, clip);
        if level != player.charge_level {
            if level == charge.max_level(clip) {
                player.charging = false;
                player.charged = true;
            }
            player.charge_level = level;
        }
    }
}

/// How far the gun in Jazz's hand has charged, as the HUD shows it (0..1): none
/// while it fires (in the wait after a shot), or for a gun that doesn't charge.
fn charge_shown(player: &Player) -> f32 {
    let charge = charged_by(player);
    match charge {
        Some(c) if player.fire_cooldown <= 0.0 => {
            c.fraction(player.charge_timer, player.ammo[player.weapon].0)
        }
        _ => 0.0,
    }
}

/// Starts reloading `slot` if it has rounds in reserve and room in the clip: the
/// reload sound, the gun's reload animation (stretched to the reload time) and
/// the HUD's reload ring.
fn start_reload(
    player: &mut Player,
    slot: usize,
    sfx: &mut Sfx,
    hud: Option<&mut HudState>,
    at: Option<Vec3>,
) {
    let g = player.gun(slot);
    let (clip, reserve) = player.ammo[slot];
    // (Nor reacting to a melee blow: HmPawn.ReloadWeapon, its reload turned off.)
    if player.reload_left > 0.0
        || reserve == 0
        || clip >= g.clip_size
        || player.dead_left > 0.0
        || player.hand_switch.is_some()
        || player.melee_react.reacting()
    {
        return;
    }
    player.reload_left = g.reload_secs;
    player.reload_slot = slot;
    // Its steam, if it has any: fire() lets it out at the gun's socket for that.
    player.steam = (player.form == Form::Robot
        && WEAPONS.get(slot).is_some_and(|w| w.reload_steam.is_some()))
    .then_some(0.0);
    sfx.play_from(g.sound_reload, at);
    if let (Form::Robot, Some(w)) = (player.form, WEAPONS.get(slot)) {
        let (clip, length) = w.reload_anim;
        player.start_weapon_clip = Some((clip, g.reload_secs, length / g.reload_secs));
    }
    if let Some(hud) = hud {
        hud.events.push(HudEvent::Reload(g.reload_secs));
    }
}

/// R reloads; reloads finish (or stop when Jazz changes weapon, transforms or is
/// downed); spread settles back, and widens in the air.
#[allow(clippy::type_complexity)]
fn reload_and_spread(
    time: Res<Time>,
    mut players: Query<
        (
            &Transform,
            &mut Player,
            &PlayerInput,
            Has<LocalPlayer>,
            Option<&split::Seat>,
        ),
        Without<RemotePlayer>,
    >,
    mut sfx: ResMut<Sfx>,
    mut huds: split::BySeat<HudState>,
) {
    let dt = time.delta_secs();
    for (transform, mut player, input, local, seat) in &mut players {
        let at = voice_at(local, transform.translation);
        // (His HUD: split screen's guest's his own.)
        let hud = huds.seat(split::seat_of(seat));
        reload_jazz(dt, &mut player, input, &mut sfx, local.then_some(hud), at);
    }
}

/// reload_and_spread for one Jazz.
fn reload_jazz(
    dt: f32,
    player: &mut Player,
    input: &PlayerInput,
    sfx: &mut Sfx,
    hud: Option<&mut HudState>,
    at: Option<Vec3>,
) {
    let slot = gun_slot(player);
    if player.reload_left > 0.0 {
        if player.reload_slot != slot || player.transform_left > 0.0 || player.dead_left > 0.0 {
            player.reload_left = 0.0;
        } else {
            player.reload_left -= dt;
            if player.reload_left <= 0.0 {
                player.reload_left = 0.0;
                let (clip, reserve) = player.ammo[slot];
                let take = (player.gun(slot).clip_size - clip).min(reserve);
                player.ammo[slot] = (clip + take, reserve - take);
            }
        }
    } else if input.reload && player.transform_left <= 0.0 {
        start_reload(player, slot, sfx, hud, at);
    }
    // The spread settles back all the time, firing or not (TnWeapon.Tick's
    // CooldownSpread), so held down it opens up by half a step a shot.
    let s = player.gun(slot).spread;
    if player.spread_slot != slot {
        player.spread_slot = slot;
        player.spread = s.min;
    } else if s.cooldown > 0.0 {
        player.spread = (player.spread - (s.max - s.min) / s.cooldown * dt).max(s.min);
    }
    // In the air the spread widens (TnWeaponSpreadModifier.Tick), and narrows back
    // on landing; it keeps as it is in the car.
    if player.form == Form::Robot {
        let (target, secs) = if player.grounded {
            (1.0, LAND_SPREAD_SECS)
        } else {
            (AIRBORNE_SPREAD, AIRBORNE_SPREAD_SECS)
        };
        let step = (AIRBORNE_SPREAD - 1.0) / secs * dt;
        player.airborne_spread += (target - player.airborne_spread).clamp(-step, step);
    }
}

/// ReloadSteam_Med_FX's steam, and a steaming shell's: 1.5 s of it (its
/// EmitterDuration), a puff of five wisps every quarter of a second (its burst of
/// five, then 20 a second), as many as its peak holds (fx.rs's STEAM).
const STEAM_SECS: f32 = 1.5;
const STEAM_PUFF_SECS: f32 = 0.25;

/// The puffs of such steam due between `since` and `now` seconds into it (by
/// number, 0 the first): one at the start, then one every STEAM_PUFF_SECS up to
/// STEAM_SECS.
fn steam_puffs(since: f32, now: f32) -> impl Iterator<Item = u32> {
    let first = (since / STEAM_PUFF_SECS).ceil() as u32;
    let last = (now / STEAM_PUFF_SECS).ceil() as u32;
    (first..last).filter(|&puff| puff as f32 * STEAM_PUFF_SECS <= STEAM_SECS)
}

/// When a steaming shell lets out its wisps (seconds in): its steam's emitter keeps
/// its cooked peak of one at a time (two for the Scatter Blaster's: fx.rs's
/// SHELL_STEAM), its rate letting out the next as the last one goes, so one at
/// the start and the next once it has lived its middling life (0.75 s; they live
/// 0.5-1 s), within the steam's 1.5 s.
const SHELL_STEAM_WISPS: [f32; 2] = [0.0, 0.75];

/// How many of a steaming shell's wisps are due between `since` and `now` seconds
/// into its steam.
fn shell_steam_wisps(since: f32, now: f32) -> usize {
    SHELL_STEAM_WISPS
        .iter()
        .filter(|&&at| at >= since && at < now)
        .count()
}

/// TnWeaponSpreadModifier: in the air, guns spread 1.25x as much, the spread
/// widening over a quarter of a second and narrowing back over a quarter of a
/// second on landing (its AirborneMultiplier, AirborneRampUpTime, LandRampDownTime).
const AIRBORNE_SPREAD: f32 = 1.25;
const AIRBORNE_SPREAD_SECS: f32 = 0.25;
const LAND_SPREAD_SECS: f32 = 0.25;

/// Returns whether the hit did any damage. The shield takes it first, the rest
/// comes off his health (TnShieldHealth.OnTakeDamage); either way the shield's
/// recharge starts waiting again (TnHealthRegener.InterruptRegen). How it struck
/// him is kept (`hit`: the death he dies, if it kills him: downed.rs). Only a
/// melee blow staggers him (the hit reaction); shots and blasts don't.
/// A blow's push on him (Pawn.TakeDamage's momentum over his mass, as
/// HandleMomentum's AddVelocity adds it, for a damage type whose
/// RequestRespectForcesApplied asks for it: the Guard's blast), `velocity` in the
/// demo's m/s: he leaves the ground with it (PHYS_Falling), flying with its push
/// across the ground till he lands (move_jazz).
pub(crate) fn knock(player: &mut Player, velocity: Vec3) {
    if player.dead_left > 0.0 || player.dying || player.revive.down() {
        return;
    }
    player.knock_vel += Vec3::new(velocity.x, 0.0, velocity.z);
    player.vertical_velocity = if player.grounded {
        velocity.y
    } else {
        player.vertical_velocity + velocity.y
    };
    if velocity.y > 0.0 {
        player.grounded = false;
    }
}

/// His sword's or his punch's blow on an enemy at `struck`, going `forward`
/// (TnDamageTypeMelee and the attack's HitMomentum: 75000 forward, 75000 up,
/// Unreal's units). A melee blow struck at `struck`
/// (TnMeleeManager.ServerProcessMeleeHit; its SidewaysMomentum goes unused): its
/// momentum along the ground from the striker to the one struck (`toward`, the way
/// from his place to its; along his `forward` should they stand at one place)
/// times the attack's ForwardMomentum, and its VerticalMomentum up.
fn melee_hit(struck: Vec3, toward: Vec3, forward: Vec3) -> enemies::Hit {
    let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or(Vec3::NEG_Z);
    let away = Vec3::new(toward.x, 0.0, toward.z).normalize_or(forward);
    enemies::Hit::new(enemies::Death::Melee, struck, away)
        .with_momentum_vec(away * MELEE_MOMENTUM.0 + Vec3::Y * MELEE_MOMENTUM.1)
}

/// The HitMomentum of Escalation's melee set's sword swing (Melee_EnergonSword_01)
/// and sprint punch (Melee_Boost) alike: forward and up. Up, 75000 over a
/// soldier's Mass of 100 throws it up at 750 uu/s, in the air for 0.51 s by
/// TnHtnAiPawn.EstimateAirTime: past the half second that knocks it back
/// (TnHtnAiPawn.TakeDamage's KnockbackVelocity; its planner's STS_Knockedback, the
/// take off, flight and landing of its blueprint's Knockedback animations: ai/).
/// Multiplayer's 10000 up is 0.07 s: no knockback.
const MELEE_MOMENTUM: (f32, f32) = (75000.0, 75000.0);

fn damage_player(player: &mut Player, amount: f32, hit: enemies::Hit) -> bool {
    if player.dead_left > 0.0 || player.protection > 0.0 || player.dying {
        return false;
    }
    // (The difficulty's and the wave's damage taken: waves.rs.)
    let amount = amount * waves::taken();
    player.last_hit = hit;
    // (Its striker's, if an enemy's: who calls this says, after.)
    player.last_attack = (killfeed::Who::Nobody, hit.damage);
    // Down (revive.rs): it comes off his downed health, after his first seconds.
    if player.revive.downed.is_some() {
        return revive::hurt_downed(player, amount);
    }
    // What his class ability makes of it (abilities.rs: his Flak Shield stopping it,
    // his buffs, his cloak given away).
    let amount = abilities::hurt(player, amount, &hit, None);
    if amount <= 0.0 {
        return true;
    }
    let absorbed = amount.min(player.shield);
    player.shield -= absorbed;
    player.health -= amount - absorbed;
    player.since_hit = 0.0;
    player.shield_fx.struck(hit);
    if player.health <= 0.0 {
        player.health = 0.0;
        player.shield = 0.0;
        // With others in the game he goes down rather than dying (revive.rs).
        if player.revive.downable {
            revive::go_down(player);
        } else {
            player.dying = true;
        }
    } else if hit.damage == hitreact::DAMAGE_TYPE && matches!(hit.death, enemies::Death::Melee) {
        // An enemy's melee blow: his melee set's reaction, by the side it came from
        // (hitreact.rs).
        if can_react(player) {
            let forward = Quat::from_rotation_y(player.heading) * Vec3::NEG_Z;
            player
                .melee_react
                .start(hitreact::reaction(forward, -hit.dir));
        }
    } else if matches!(hit.death, enemies::Death::Melee)
        && player.form == Form::Robot
        && player.action.is_none()
        && player.transform_left <= 0.0
    {
        start_action(player, ANIM_HIT, HIT_SECS, 1.0);
    }
    true
}

/// Whether a melee blow makes him react (TnMeleeManager.ShouldPlayHitReaction,
/// TnPawn.get_CanPlayHitReaction, PlayHitReaction and FindReaction): in robot form,
/// not changing form, down, reviving another, dodging, swinging his sword or
/// punching, spinning his Whirlwind, nor with his Flak Shield up (the reactions
/// need it down).
fn can_react(player: &Player) -> bool {
    player.form == Form::Robot
        && player.transform_left <= 0.0
        && player.pending_form.is_none()
        && player.revive.downed.is_none()
        && player.revive.healing.is_none()
        && !(player.dodge && player.dash_left > 0.0)
        && !player.meleeing()
        && player.melee_hit_in.is_none()
        && !player.ability.blocks_attacks()
        && !(player.ability.kind == abilities::Kind::Shield && player.ability.active)
}

/// The shield's recharge, death and respawn, for every Jazz.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn player_health(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    chosen: split::BySeatRef<choose::Chosen>,
    current_map: Res<CurrentMap>,
    mut tally: split::BySeat<scoreboard::Tally>,
    mut kill_feed: ResMut<killfeed::KillFeed>,
    mut players: Query<
        (
            Entity,
            &mut Transform,
            &mut Player,
            &PlayerSlot,
            Has<LocalPlayer>,
            Option<&downed::Downed>,
            Option<&split::Seat>,
        ),
        Without<RemotePlayer>,
    >,
    mut forms: Query<
        (Entity, &mut Visibility, Has<RobotForm>, &JazzOf),
        Or<(With<RobotForm>, With<VehicleForm>)>,
    >,
) {
    let dt = time.delta_secs();
    for (owner, mut transform, mut player, slot, local, downed, seat) in &mut players {
        // (Each of this PC's players by his own character and scoreboard.)
        let seat = split::seat_of(seat);
        let chosen = chosen.seat(seat);
        player.since_hit += dt;
        player.protection -= dt;

        // Down in a vehicle, he changes back to his robot at once (TnVehicleForm.
        // ClientStartDowned), whatever change was under way. That change, or the
        // one cut short, ends there, and its reload with it (the transforming
        // state's EndState runs its SwitchToFormWeapon).
        if player.revive.downed.is_some()
            && (player.form == Form::Vehicle
                || player.pending_form.is_some()
                || player.transform_left > 0.0
                || player.form_weapon_left.is_some())
        {
            player.pending_form = None;
            player.transform_left = 0.0;
            if player.form == Form::Vehicle {
                switch_form(
                    &mut commands,
                    owner,
                    &mut player,
                    &mut forms,
                    Form::Robot,
                    false,
                );
                reload_all_weapons(&mut player);
            } else if player.form_weapon_left.is_some() {
                reload_all_weapons(&mut player);
            }
        }

        if player.dying {
            // (The scoreboard counts this PC's player's deaths.)
            if local {
                tally.seat(seat).died();
            }
            // (The kill feed's line for it: his last blow's striker and damage type.)
            kill_feed
                .noticed
                .push(killfeed::player_death(&player, slot.0 as u8));
            // Escalation's dead come back as the team's round ends, not on a timer
            // (revive.rs).
            player.revive.out = true;
            player.revive.dead_for = 0.0;
            // Jazz always falls in robot form (his death: downed.rs).
            player.dying = false;
            player.pending_form = None;
            player.transform_left = 0.0;
            if player.form == Form::Vehicle {
                switch_form(
                    &mut commands,
                    owner,
                    &mut player,
                    &mut forms,
                    Form::Robot,
                    false,
                );
            }
            player.dash_left = 0.0;
            player.melee_hit_in = None;
            player.melee_sweep = melee::Sweep::default();
            player.dead_left = RESPAWN_SECS;
            continue;
        }

        if player.dead_left > 0.0 {
            player.dead_left -= dt;
            // (Not before his body has gone, nor while he is out till the round
            // ends; the round's end brings him back at once: revive.rs.)
            let waits = downed.is_some_and(|d| !d.gone()) && !player.revive.back;
            if player.dead_left <= 0.0 && (waits || player.revive.out) {
                player.dead_left = DEAD_HOLD;
            }
            if player.dead_left <= 0.0 {
                // Back in the fight where he started the match, as the character
                // he has chosen (choose.rs; one chosen since he died is used now).
                player.dead_left = 0.0;
                player.health = player.max_health();
                player.shield = PLAYER_MAX_SHIELD;
                player.ammo = full_ammo();
                if local && let Some(loadout) = chosen_loadout(chosen) {
                    player.upgrade_pairs = chosen_pairs(chosen);
                    arm(&mut player, loadout);
                    if let Some(look) = chosen_look(chosen, current_map.0) {
                        player.look = look;
                    }
                } else {
                    // (The weapons he carried, with their upgrades, come back full.)
                    for w in player.loadout {
                        player.ammo[w] = player.initial_ammo(w);
                    }
                }
                player.action = None;
                player.protection = SPAWN_PROTECTION;
                player.revive = revive::Revive {
                    downable: player.revive.downable,
                    ..default()
                };
                transform.translation = level.start(slot.0);
                player.vertical_velocity = 0.0;
                player.jammed_left = 0.0;
                player.knock_vel = Vec3::ZERO;
            }
            continue;
        }

        // Down, nothing recharges (TnPawn.Tick skips his HealthRegener).
        if player.revive.downed.is_some() {
            continue;
        }
        // The shield recharges, for the time past the delay (TnHealthRegener.Update).
        let (delay, rate) = match player.form {
            Form::Robot => (SHIELD_REGEN_DELAY, SHIELD_REGEN_RATE),
            Form::Vehicle => (CAR_SHIELD_REGEN_DELAY, CAR_SHIELD_REGEN_RATE),
        };
        if player.support.recharge_left > 0.0 {
            // The Energon Recharger's buff: no wait, his health and then his shield
            // (TnHealTypeRegenAll: SH_HealBaseThenShield), at its share of the rate.
            let mut heal = rate * equipment::RECHARGE_SCALE * dt;
            let to_health = heal.min(player.max_health() - player.health).max(0.0);
            player.health += to_health;
            heal -= to_health;
            player.shield = (player.shield + heal).min(PLAYER_MAX_SHIELD);
        } else {
            let past = player.since_hit - delay;
            if past > 0.0 {
                player.shield = (player.shield + rate * dt.min(past)).min(PLAYER_MAX_SHIELD);
            }
        }
    }
}

/// Compass heading of a direction on the ground: 0 = -Z, pi/2 = +X (the HUD's
/// damage arrows turn by it).
fn heading(dir: Vec3) -> f32 {
    dir.x.atan2(-dir.z)
}

/// How far over a Jazz's head (or his car's roof) his marker goes.
const MARKER_LIFT: f32 = 0.4;
/// The top of a downed Jazz as he lies propped up (his downed clips' head, about
/// 2.1 m up), which his marker goes over.
const DOWNED_MARKER_HEIGHT: f32 = 2.1;
/// An enemy grenade shows its marker within 18 m of him (TnObjectiveMarkerType-
/// Grenade's DrawCutoffDistance, 1800), lifted a twentieth of the screen's height
/// (its MarkerSSOffset, -0.05 of the height: a tenth of the -1..1 the normalized
/// device coordinates span).
const GRENADE_MARKER_REACH: f32 = 18.0;
const GRENADE_MARKER_LIFT: f32 = 0.1;

/// The other players, for the markers over them on the HUD: each one's name (as
/// the lobby has it: his Display Name), his health and shield and where his head
/// is on the screen; and the markers on the enemies and their grenades.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn feed_hud_allies(
    mut huds: split::BySeat<HudState>,
    net: Res<net::Net>,
    cameras: Query<(&Camera, &Transform, &split::Seat), With<OrbitCamera>>,
    others: Query<(&PlayerSlot, &Player, &Transform), (With<RemotePlayer>, Without<OrbitCamera>)>,
    locals: Query<
        (&PlayerSlot, &Player, &Transform, &split::Seat),
        (With<LocalPlayer>, Without<OrbitCamera>),
    >,
    enemies: Query<(Entity, &Transform, &Enemy), (Without<Player>, Without<OrbitCamera>)>,
    grenades: Query<
        (Entity, &Transform),
        (
            Or<(With<ai::grenade::AiGrenade>, With<ai::projectile::Marked>)>,
            Without<Player>,
            Without<OrbitCamera>,
        ),
    >,
    (waves, pause): (Res<waves::Waves>, Res<menu::Pause>),
) {
    // Each of this PC's players' HUD by his own camera, his teammates the others
    // online and his split screen partner.
    for (camera, view, seat) in &cameras {
        let me = locals
            .iter()
            .find(|l| l.3 == seat)
            .map(|(_, p, t, _)| (p, t));
        let mates: Vec<(&PlayerSlot, &Player, &Transform)> = others
            .iter()
            .chain(
                locals
                    .iter()
                    .filter(|l| l.3 != seat)
                    .map(|(slot, p, t, _)| (slot, p, t)),
            )
            .collect();
        allies_for(
            huds.seat(seat.0),
            &net,
            (camera, view),
            me,
            &mates,
            (&enemies, &grenades),
            (&waves, &pause),
        );
    }
}

/// One player's HUD's allies, markers and locks (feed_hud_allies), by his camera:
/// his own Jazz `me`, his teammates `mates`.
#[allow(clippy::type_complexity)]
fn allies_for(
    hud: &mut HudState,
    net: &net::Net,
    (camera, view): (&Camera, &Transform),
    me: Option<(&Player, &Transform)>,
    mates: &[(&PlayerSlot, &Player, &Transform)],
    (enemies, grenades): (
        &Query<(Entity, &Transform, &Enemy), (Without<Player>, Without<OrbitCamera>)>,
        &Query<
            (Entity, &Transform),
            (
                Or<(With<ai::grenade::AiGrenade>, With<ai::projectile::Marked>)>,
                Without<Player>,
                Without<OrbitCamera>,
            ),
        >,
    ),
    (waves, pause): (&waves::Waves, &menu::Pause),
) {
    // (The camera as it is now: its GlobalTransform is a frame behind.)
    let view = GlobalTransform::from(*view);
    // In front of the camera (Bevy's reversed depth: 0 far, 1 near).
    let screen = |at: Vec3| {
        camera
            .world_to_ndc(&view, at)
            .filter(|p| p.z > 0.0 && p.z <= 1.0)
            .map(|p| p.truncate())
    };
    // A homing weapon's lock (TnObjectiveMarkerTypeLockOn: from its target's picking
    // to the lock's end), on the sentry it is for: a jet's rockets', the Thermo
    // Rocket Cannon's.
    hud.lock_on = me.and_then(|(p, _)| {
        let secs = match p.form {
            Form::Vehicle => jet::JET_LOCK.0,
            Form::Robot => WEAPONS.get(p.weapon)?.gear?.lock?.secs,
        };
        let target = p.lock.target?;
        let (_, _, e) = enemies.get(target).ok()?;
        e.alive().then(|| HudLockOn {
            id: target.to_bits(),
            at: screen(e.centre()),
            progress: p.lock.progress(secs),
        })
    });
    // The enemies the Orbital Pistol has revealed (orbital.rs), and every one as a
    // wave's last show (waves.rs: TnBuffSeeEnemyObjectiveMarkers), wherever they
    // are, each with its marker just over it.
    let all = waves.markers();
    hud.revealed = enemies
        .iter()
        .filter(|(_, _, e)| e.alive() && (all || e.mark.revealed()))
        .map(|(entity, _, e)| HudRevealed {
            id: entity.to_bits(),
            mark: e.mark.level,
            at: screen(e.top() + Vec3::Y * MARKER_LIFT),
        })
        .collect();
    // The enemies' grenades (and the jets' sticky rockets) within 18 m of him
    // (TnObjectiveMarkerTypeGrenade:
    // ShouldDisplayMarker's DrawCutoffDistance, from his middle; their data's
    // ShowMarker), each projected (Canvas.Project, in front of the camera or not)
    // and lifted a twentieth of the screen's height (MarkerSSOffset). None while he
    // has no pawn (dead, or waiting for the next wave) or the HUD is down
    // (TnHUD.UpdateObjectiveMarker: no PlayerOwner.Pawn, HudVisible or a pause).
    hud.grenades.clear();
    let mine = me.filter(|(p, _)| {
        !(p.dead_left > 0.0 || p.dying || p.waiting || pause.open || pause.choosing)
    });
    if let Some((p, t)) = mine {
        let middle = t.translation + Vec3::Y * p.size().1 * 0.5;
        for (entity, g) in grenades.iter() {
            if g.translation.distance_squared(middle) > GRENADE_MARKER_REACH * GRENADE_MARKER_REACH
            {
                continue;
            }
            let Some(ndc) = camera.world_to_ndc(&view, g.translation) else {
                continue;
            };
            hud.grenades.push(HudGrenade {
                id: entity.to_bits(),
                at: ndc.truncate() + Vec2::Y * GRENADE_MARKER_LIFT,
                in_front: ndc.z > 0.0,
            });
        }
    }
    hud.allies.clear();
    for &(slot, player, at) in mates {
        let Some((_, lobby)) = net.players().find(|(s, _)| *s == slot.0) else {
            continue;
        };
        // (Down, over his body as it lies.)
        let height = if player.revive.down() {
            DOWNED_MARKER_HEIGHT
        } else {
            player.size().1
        };
        let head = at.translation + Vec3::Y * (height + MARKER_LIFT);
        // In front of the camera (Bevy's reversed depth: 0 far, 1 near), and up.
        // (None while he has no pawn: dying, dead, out till the round ends or
        // waiting for the next match.)
        let down = player.dead_left > 0.0 || player.dying || player.waiting || player.revive.out;
        let screen = camera
            .world_to_ndc(&view, head)
            .filter(|p| p.z > 0.0 && p.z <= 1.0 && !down)
            .map(|p| p.truncate());
        hud.allies.push(HudAlly {
            id: slot.0,
            name: lobby.name.clone(),
            health: (player.health / player.max_health()).clamp(0.0, 1.0),
            shield: Some((player.shield / PLAYER_MAX_SHIELD).clamp(0.0, 1.0)),
            downed: player.revive.downed_fraction(),
            at: screen,
        });
    }
}

/// Hands the game's state to the HUD (src/hud.rs), which shows it the way Fall of
/// Cybertron's HUD movie does.
#[allow(clippy::too_many_arguments)]
///
/// Each of this PC's players' HUD shows him (split screen's guest's his own).
#[allow(clippy::type_complexity)]
fn feed_hud(
    mut huds: split::BySeat<HudState>,
    scores: split::BySeatRef<Score>,
    (level, colliders): (Res<Level>, Res<Colliders>),
    players: Query<(&Player, &Transform, &PlayerInput, &split::Seat), With<LocalPlayer>>,
    orbits: Query<(&OrbitCamera, &split::Seat)>,
    (enemies, kinds): (Query<&Enemy, Without<Player>>, Res<enemies::Kinds>),
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    pause: Res<menu::Pause>,
    (controls, pads): (Res<Controls>, pad::Pads),
    (display, current_map, barrels): (
        Res<menu::DisplaySettings>,
        Res<CurrentMap>,
        Res<barrels::Barrels>,
    ),
) {
    for (player, player_pos, input, seat) in &players {
        let Some((orbit, _)) = orbits.iter().find(|(_, s)| *s == seat) else {
            continue;
        };
        let first = seat.0 == 0;
        let pad = pads.seat(seat.0);
        // (The guest plays by his controller alone; the first player's mouse
        // free, how to play.)
        let by_pad = pad.active || !first;
        let mouse_free = first && cursor.grab_mode == CursorGrabMode::None && !by_pad;
        feed_seat_hud(
            huds.seat(seat.0),
            scores.seat(seat.0),
            (&level, &colliders),
            (player, player_pos, input),
            orbit,
            (&enemies, &kinds),
            &pause,
            (&controls, by_pad, mouse_free),
            (&display, &current_map, &barrels),
        );
    }
}

/// One player's HUD's state (feed_hud).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn feed_seat_hud(
    hud: &mut HudState,
    score: &Score,
    (level, colliders): (&Level, &Colliders),
    (player, player_pos, input): (&Player, &Transform, &PlayerInput),
    orbit: &OrbitCamera,
    (enemies, kinds): (&Query<&Enemy, Without<Player>>, &enemies::Kinds),
    pause: &menu::Pause,
    (controls, by_pad, mouse_free): (&Controls, bool, bool),
    (display, current_map, barrels): (&menu::DisplaySettings, &CurrentMap, &barrels::Barrels),
) {
    // (The controller's buttons in its prompts while it plays: pad.rs.)
    if hud.pad != by_pad {
        hud.pad = by_pad;
    }
    // The HUD jolts with the camera's shake (TnHudDataObserverCameraShake); what it
    // shows of the aim goes by where the view points without it.
    hud.camera_shake = orbit.shake;
    // (Its messages' colours: Team Coloring Mode, and whose side the players are.)
    hud.faction_colors = display.faction_colors;
    hud.decepticons = decepticon_map(current_map.0);
    // The pause menu (and the Choose Character screen) takes the HUD's place, and
    // there is none before the match has begun for him (pregame.rs).
    hud.hidden = pause.open || pause.choosing || player.waiting;
    // (Markers kept on the screen keep inside the safe frame.)
    hud.safe_frame = display.safe_frame;
    let slot = gun_slot(player);
    let g = player.gun(slot);
    hud.health = player.health;
    hud.max_health = player.max_health();
    hud.shield = player.shield;
    hud.max_shield = PLAYER_MAX_SHIELD;
    hud.weapon = match WEAPONS.get(slot) {
        Some(w) if player.form == Form::Robot => HudWeapon {
            id: slot as u32,
            label: w.hud_label,
            icon: Some(w.hud_icon),
            crosshair: w.hud_crosshair,
        },
        // The vehicles' weapons show their names but no icon (there are none for
        // them); a tank's cannon has the Riot Cannon's crosshair, a jet's rockets and a
        // truck's the rocket one (updateCrosshair's for TnWeaponTankCannon, and for
        // TnWeaponHomingRocketVehicle and TnWeaponRocketVehicle).
        _ => HudWeapon {
            id: slot as u32,
            label: match slot {
                TANK_SLOT => TANK_LABEL,
                JET_SLOT => JET_LABEL,
                TRUCK_SLOT => TRUCK_LABEL,
                _ => VEHICLE_LABEL,
            },
            icon: None,
            crosshair: match slot {
                TANK_SLOT => "mc_crosshairCannon",
                JET_SLOT | TRUCK_SLOT => "mc_crosshairRocket",
                _ => "mc_crosshairRifle",
            },
        },
    };
    let (clip, reserve) = player.ammo[slot];
    hud.clip = clip;
    hud.reserve = reserve;
    hud.clip_size = g.clip_size;
    hud.reserve_max = g.reserve_max;
    hud.fine_aim = player.aiming && player.form == Form::Robot;
    hud.spread = spread_now(player, &g.spread);
    hud.vehicle = player.form == Form::Vehicle;
    hud.reloading = player.reload_left > 0.0;
    hud.energon = score.energon;
    let view = input.view_dir;
    let eye = input.view_origin;
    hud.view_yaw = heading(view);
    hud.dead = player.dead_left > 0.0 || player.dying;
    // Red crosshair: an enemy under it, in range and in sight (or an exploding
    // barrel, a target too: barrels.rs).
    let range = WEAPONS.get(slot).map_or(
        match slot {
            TANK_SLOT => TANK_RANGE,
            JET_SLOT => JET_RANGE,
            TRUCK_SLOT => TRUCK_RANGE,
            _ => VEHICLE_RANGE,
        },
        |w| w.range,
    );
    let wall_hit = level.shot_hit(colliders, eye, view, range);
    let wall = wall_hit.unwrap_or(range);
    let on_barrel = wall_hit.is_some_and(|d| barrels.struck_at(eye + view * d).is_some());
    // (The nearest, its health and its name.)
    let target_hit = enemies
        .iter()
        .filter_map(|e| e.ray(eye, view, wall).map(|d| (d, e)))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    let target = target_hit.map(|(d, _)| d);
    // (A jet's rockets locked on show it the same way.)
    hud.on_target = target.is_some() || player.lock.locked || on_barrel;
    // The weapon in hand's rounds out to set off, or its bomb in the air to burst
    // (HudData's ActiveGrenadeCount and CanClusterBomb: the weapon's own).
    let gear = WEAPONS
        .get(slot)
        .and_then(|w| w.gear)
        .filter(|_| player.form == Form::Robot);
    hud.grenades_out = gear.is_some_and(|g| g.detonate.is_some()) && player.gear.grenades > 0;
    hud.can_cluster_bomb = gear.is_some_and(|g| g.arc.is_some()) && player.gear.in_flight;
    hud.southpaw = controls.layout == controls::SOUTHPAW;
    // The scope (fine aim with the Photon Burst Rifle or the Nucleon Charge Rifle):
    // the tactical one's range finder reads how far from Jazz the crosshair's target
    // is, nothing beyond the weapon's range; the sniper one shows the charge.
    hud.scope = match WEAPONS.get(slot) {
        Some(w) if hud.fine_aim => w.scope,
        _ => Scope::None,
    };
    hud.charge = charge_shown(player);
    // The upgrades' parts of the HUD: the special shot's round (the clip's last:
    // its first segment, _SpecialShotInClip 1 less one), Deadeye's lights in the
    // Nucleon Charge Rifle's scope (all three while it charges faster), and the
    // tactical scope's target info - its range finder, the target's name and
    // health - which multiplayer gives the Photon Burst Rifle only with the Cyber
    // Tactical Scope (TnWeaponBurstRifle.ToggleWeaponSpecificHud: without it its
    // TnHudDataObserverTargetDistance, TargetName and TargetHealthNormalized are
    // off, the range finder reading nothing).
    let mods = match player.form {
        Form::Robot => player.mods(slot),
        Form::Vehicle => upgrades::Mods::default(),
    };
    hud.special_shot = mods.last_shot_special.then_some(0);
    hud.sniper_upgrade = mods
        .hits_charge
        .map(|_| if player.deadeye_left > 0.0 { 3 } else { 0 });
    let info = mods.show_info;
    let aimed = target.unwrap_or(wall);
    hud.target_distance = if info && aimed < range {
        (eye + view * aimed).distance(player.robot_centre(player_pos.translation))
    } else {
        -1.0
    };
    hud.target_health = match target_hit {
        Some((_, e)) if info => (e.health / e.max_health.max(1.0)).clamp(0.0, 1.0),
        _ => -1.0,
    };
    if info && let Some((_, e)) = target_hit {
        hud.target_name = enemies::name_of(kinds, e).to_string();
    }
    // Nothing to pick up or buy dead (out till the round ends, the respawn screen
    // over the HUD: revive.rs). How to play while the mouse is free.
    if hud.dead {
        hud.prompt = None;
    }
    if mouse_free {
        hud.announcement = "CLICK TO PLAY".to_string();
        hud.hint = controls_hint(controls);
    } else {
        hud.announcement.clear();
        hud.hint.clear();
    }
}

/// The keys to play with, shown while the mouse is free: the robot's in the
/// keyboard layout in use (Settings > Controls), and the car's boost.
fn controls_hint(controls: &Controls) -> String {
    let robot = |action| controls.short_key_name(Mode::Robot, action);
    format!(
        "{} MOVE   {} DASH (TAP), SPRINT (HOLD)   {} JUMP   {} TRANSFORM\n\
         {} FIRE   {} AIM   {} MELEE   {} RELOAD   {} (HOLD) PICK UP   ESC PAUSE\n\
         {} SWAP WEAPONS   {} SWITCH HANDS   {} BOOST (CAR)",
        robot(Action::Move),
        robot(Action::DashSprint),
        robot(Action::Jump),
        robot(Action::ChangeForm),
        robot(Action::Fire),
        robot(Action::FineAim),
        robot(Action::Melee),
        robot(Action::Reload),
        robot(Action::Interact),
        robot(Action::SwapWeapons),
        robot(Action::SwitchCamera),
        controls.short_key_name(Mode::Vehicle, Action::SpeedBoost),
    )
}

// ---------------------------------------------------------------------------
// Sound
// ---------------------------------------------------------------------------

/// One of the local player's vehicle's looping engine sounds (engine_sound), and
/// how loud it is now.
#[derive(Component)]
struct EngineVoice {
    sound: EngineLoop,
    level: f32,
    /// Whose (split screen's seat: each of this PC's players hears his own).
    seat: usize,
}

/// Loads the sound cues.
fn setup_sounds(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(SoundBank::load(&assets_dir().join("sounds"), &asset_server));
}

/// A transformation's sounds: its armor set's (the cues of the robot's and the
/// car's clips' notifies), Jazz's without the characters, played for `owner` (the
/// Jazz's entity's bits; None in Customize). The clips play `speed` times as fast as
/// in the game, so the sounds come sooner and their parts closer together, to stay
/// in step.
pub(crate) fn transform_sounds(
    sfx: &mut Sfx,
    chars: &character::Characters,
    vehicle: Option<usize>,
    to_car: bool,
    speed: f32,
    at: Option<Vec3>,
    owner: Option<u64>,
) {
    let speed = speed.max(0.01);
    // (A Jazz's own, so that his vehicle's engine, which some start, can be cut
    // short: cut_hover_engines.)
    let mut play = |cue: &'static str, delay: f32| match owner {
        Some(owner) => sfx.play_owned(owner, cue, delay, 1.0 / speed, at),
        None => sfx.play_scaled_after_from(cue, delay, 1.0 / speed, at),
    };
    match chars.transform_sounds(vehicle, to_car) {
        Some(sounds) => {
            for &(t, cue) in sounds {
                play(cue, t / speed);
            }
        }
        None => play(if to_car { SND_BOT2VEH } else { SND_VEH2BOT }, 0.0),
    }
}

/// The sounds the game's animations trigger (their AnimNotify_Sound markers),
/// as (seconds into the clip, cue).
fn clip_sounds(
    sfx: &mut Sfx,
    chars: &character::Characters,
    kit: &character::Kit,
    clip: usize,
    speed: f32,
    at: Option<Vec3>,
    owner: Option<u64>,
) {
    let speed = speed.max(0.01);
    // His class's robot's footsteps, jumps and landings (Jazz's small robot's for
    // the Infiltrator: BL_FS_SML_BOT, the Destroyer's BL_FS_LRG_BOT).
    let class = kit.class;
    let step = chars.robot_sound(class, character::ROBOT_WALK, SND_STEP_WALK);
    let boost = chars.robot_sound(class, character::ROBOT_JUMP_CHARGED, SND_BOOST);
    let land = chars.robot_sound(class, character::ROBOT_LAND, SND_LAND);
    let land_high = chars.robot_sound(class, character::ROBOT_LAND_HIGH, SND_LAND_HIGH);
    // A big robot falls heavily (BF_DEFAULT_ROBOT_HEAVY).
    let bodyfall = if kit.robot.height > 4.0 {
        chars.robot_sound(class, character::SOUND_IMPACT_HEAVY, SND_BODYFALL)
    } else {
        SND_BODYFALL
    };
    let notifies: Vec<(f32, &'static str)> = match clip {
        // A transformation: its armor set's sounds (the clip is its vehicle's).
        c if is_to_vehicle(c) || is_to_robot(c) => {
            let vehicle = c
                .checked_sub(ANIM_COUNT)
                .and_then(|k| chars.vehicle_of_clip(class, k));
            transform_sounds(sfx, chars, vehicle, is_to_vehicle(c), speed, at, owner);
            return;
        }
        ANIM_BOOST_F | ANIM_BOOST_B | ANIM_BOOST_L | ANIM_BOOST_R | ANIM_AIR_BOOST_F
        | ANIM_AIR_BOOST_B | ANIM_AIR_BOOST_L | ANIM_AIR_BOOST_R => vec![(0.0, boost)],
        // His class's own melee's (its sword's swing), or the shield bash's.
        ANIM_MELEE => match chars.robot_clip_sounds(class, ANIM_MELEE) {
            Some(own) => own.to_vec(),
            None => vec![
                (0.114, SND_BASH_SERVO),
                (0.131, SND_BASH_SWING),
                (0.62, SND_BASH_SETTLE),
            ],
        },
        ANIM_MELEE_BOOST => vec![(0.008, SND_MELEE_SWING)],
        ANIM_LAND => vec![(0.0, land)],
        ANIM_LAND_HARD => vec![(0.0, land_high)],
        ANIM_THROW => vec![(0.0, SND_GRENADE_THROW)],
        ANIM_HIT => vec![(0.0, SND_HIT_REACT)],
        // Turning on the spot: a step as each foot comes down (from the clips' foot
        // heights; the relaxed right turn takes three steps).
        // (The heavy-gun stance's as the shooting stance's.)
        c if c == ANIM_PIVOT
            || c == ANIM_PIVOT + 2
            || c == ANIM_PIVOT + 4
            || c == ANIM_HEAVY_PIVOT =>
        {
            vec![(0.233, step), (0.5, step)]
        }
        c if c == ANIM_PIVOT + 1 => vec![(0.167, step), (0.367, step), (0.533, step)],
        c if c == ANIM_PIVOT + 3 || c == ANIM_PIVOT + 5 || c == ANIM_HEAVY_PIVOT + 1 => {
            vec![(0.4, step), (0.6, step)]
        }
        ANIM_PIVOT_180 | ANIM_HEAVY_PIVOT_180 => vec![(0.3, step), (0.6, step)],
        c if c == ANIM_PIVOT_180 + 1 || c == ANIM_HEAVY_PIVOT_180 + 1 => {
            vec![(0.2, step), (0.433, step), (0.667, step)]
        }
        // Jazz powers down, then hits the floor about a second in.
        ANIM_DEATH => vec![(0.0, SND_DEATH), (1.0, bodyfall)],
        _ => Vec::new(),
    };
    for (t, cue) in notifies {
        sfx.play_after_from(cue, t / speed, at);
    }
}

/// What player_sounds saw of a Jazz last frame.
#[derive(Component, Default)]
struct SoundTrack {
    ready: bool,
    clip: Option<(usize, f32)>,
    grounded: bool,
    vehicle: bool,
    weapon: usize,
    holstered: bool,
    walking: bool,
    sprinting: bool,
    aiming: bool,
    boosting: bool,
    drive_speed: f32,
    air_time: f32,
    stride: f32,
    impact_cooldown: f32,
    shield: f32,
    alive: bool,
    /// A jet: climbing, diving, and moving off ahead in its hover (its engine's
    /// ForwardOnLoad), last frame.
    climbing: bool,
    diving: bool,
    ahead: bool,
    /// A transformation's footsteps to come: (seconds until each, its cue).
    steps: Vec<(f32, &'static str)>,
}

/// Each Jazz's own sounds: the moves he starts, footsteps, jumps and landings,
/// weapon swaps, the car's boost and crashes (the local player's as the camera
/// hears them, the others' where they are), and the local player's shield.
#[allow(clippy::type_complexity)]
fn player_sounds(
    time: Res<Time>,
    mut players: Query<
        (
            Entity,
            &Transform,
            &Player,
            &car::Car,
            &mut SoundTrack,
            Has<LocalPlayer>,
        ),
        Without<RemotePlayer>,
    >,
    chars: Res<character::Characters>,
    mut sfx: ResMut<Sfx>,
) {
    let dt = time.delta_secs();
    for (jazz, transform, p, car, mut last, local) in &mut players {
        let at = voice_at(local, transform.translation);
        let vehicle = p.form == Form::Vehicle;
        let alive = p.dead_left <= 0.0;
        // His look's vehicle's sounds (Jazz's car's without the characters).
        let car_sound =
            |event: &str, jazz: &'static str| chars.car_sound(p.kit.vehicle, event, jazz);
        if !last.ready {
            last.ready = true;
            last.grounded = p.grounded;
            last.vehicle = vehicle;
            last.weapon = p.weapon;
            last.shield = p.shield;
            last.alive = alive;
            continue;
        }

        // His own shield running out, and filling up again (not as he respawns).
        if local && alive && last.alive {
            if last.shield > 0.0 && p.shield <= 0.0 {
                sfx.play_from(SND_SHIELD_DOWN, at);
            } else if last.shield < PLAYER_MAX_SHIELD && p.shield >= PLAYER_MAX_SHIELD {
                sfx.play_from(SND_SHIELD_FULL, at);
            }
        }
        // Destroyed in his vehicle: its own death sound (its form's DeathSound), as
        // he falls out of it.
        if last.alive
            && !alive
            && last.vehicle
            && let Some(cue) = car_sound(character::SOUND_DEATH, SND_CAR_DEATH)
        {
            sfx.play_from(cue, at);
        }
        last.shield = p.shield;
        last.alive = alive;

        // A move (dash, melee, throw, landing, transformation...) started this frame.
        if let Some((clip, speed)) = p.start_clip {
            if p.start_clip != last.clip {
                clip_sounds(
                    &mut sfx,
                    &chars,
                    &p.kit,
                    clip,
                    speed,
                    at,
                    Some(jazz.to_bits()),
                );
                // (A transformation's footsteps, each as its clips reach it.)
                last.steps.clear();
                if is_to_vehicle(clip) || is_to_robot(clip) {
                    let vehicle = clip
                        .checked_sub(ANIM_COUNT)
                        .and_then(|k| chars.vehicle_of_clip(p.kit.class, k));
                    let speed = speed.max(0.01);
                    for (t, event) in chars.transform_steps(vehicle, is_to_vehicle(clip)) {
                        let cue = chars.robot_sound(p.kit.class, event, "");
                        if !cue.is_empty() {
                            last.steps.push((t / speed, cue));
                        }
                    }
                }
            }
        }
        last.clip = p.start_clip;
        // A transformation's footsteps (its clips' AnimNotify_Footstep: his landing
        // as he comes out of his vehicle, his jump into it), each as it comes, where
        // his foot finds the ground (the game's FindPhysicalMaterial: none in the
        // air, out of a jet in flight).
        if !alive {
            last.steps.clear();
        }
        for step in last.steps.iter_mut() {
            step.0 -= dt;
            if step.0 <= 0.0 && p.grounded {
                sfx.play_from(step.1, at);
            }
        }
        last.steps.retain(|s| s.0 > 0.0);

        // Jumping and landing (his class's robot's).
        let robot_sound =
            |event: &str, jazz: &'static str| chars.robot_sound(p.kit.class, event, jazz);
        if last.grounded && !p.grounded && p.vertical_velocity > 0.0 && !vehicle {
            sfx.play_from(robot_sound(character::ROBOT_JUMP, SND_JUMP), at);
        }
        if !p.grounded {
            last.air_time += dt;
        }
        // (The car's landings are car.rs's.)
        if !last.grounded
            && p.grounded
            && alive
            && last.air_time > 0.15
            && !vehicle
            && !matches!(p.start_clip, Some((ANIM_LAND | ANIM_LAND_HARD, _)))
        {
            // Landing on the move (the landing clip brings its own sound).
            sfx.play_from(robot_sound(character::ROBOT_LAND, SND_LAND), at);
        }
        if p.grounded {
            last.air_time = 0.0;
        }
        last.grounded = p.grounded;

        // Footsteps, one per stride of the jog or sprint cycle.
        let walking = !vehicle
            && alive
            && p.grounded
            && p.action.is_none()
            && p.transform_left <= 0.0
            && p.move_input.length() > 0.1;
        let sprinting = walking && sprint_run(p.sprinting, p.move_input);
        let robot = p.kit.robot;
        let (stride, step) = if sprinting {
            (
                robot.sprint_stride,
                robot_sound(character::ROBOT_RUN, SND_STEP_RUN),
            )
        } else if p.aiming && stance(p) == 2 {
            (
                robot.aim_stride,
                robot_sound(character::ROBOT_WALK, SND_STEP_WALK),
            )
        } else {
            (
                robot.jog_stride,
                robot_sound(character::ROBOT_JOG, SND_STEP_JOG),
            )
        };
        if walking {
            if !last.walking {
                // The first foot comes down soon after setting off.
                last.stride = stride * 0.6;
            }
            last.stride += p.velocity.length() * dt;
            if last.stride >= stride {
                last.stride = (last.stride - stride).min(stride * 0.5);
                sfx.play_from(step, at);
            }
        }
        if sprinting && !last.sprinting {
            sfx.play_from(SND_SPRINT_START, at);
        }
        last.walking = walking;
        last.sprinting = sprinting;

        // Raising and lowering the sights (the Nucleon Charge Rifle's scope holds its
        // hum on instead of a start sound: weapon_loops).
        if p.aiming != last.aiming {
            if let Some(weapon) = WEAPONS.get(p.weapon) {
                let cue = if p.aiming {
                    weapon.sound_aim_start
                } else {
                    weapon.sound_aim_end
                };
                if !cue.is_empty() {
                    sfx.play_from(cue, at);
                }
            }
            last.aiming = p.aiming;
        }

        // Weapon swap: put one away, bring the other up.
        if p.weapon != last.weapon {
            if let Some(old) = WEAPONS.get(last.weapon) {
                sfx.play_from(old.sound_holster, at);
            }
            if let Some(new) = WEAPONS.get(p.weapon) {
                sfx.play_after_from(new.sound_equip, 0.1, at);
            }
            last.weapon = p.weapon;
        }
        // The gun put away as he sprints or revives, and taken out again (holsters:
        // its UnEquip and Equip events' sounds; not when it is simply back in hand
        // after a transformation or a death).
        if p.holstered != last.holstered {
            if p.holster_change.is_some()
                && let Some(gun) = WEAPONS.get(p.weapon)
            {
                let cue = if p.holstered {
                    gun.sound_holster
                } else {
                    gun.sound_equip
                };
                sfx.play_from(cue, at);
            }
            last.holstered = p.holstered;
        }

        // The car: boost on and off (down onto the wheels and back up), and crashing
        // into walls.
        let boosting = vehicle && alive && !car.hover && p.transform_left <= 0.0;
        let boost_sound = if boosting && !last.boosting {
            car_sound(character::SOUND_BOOST_START, SND_CAR_BOOST_START)
        } else if !boosting && last.boosting && vehicle {
            car_sound(character::SOUND_BOOST_END, SND_CAR_BOOST_END)
        } else {
            None
        };
        if let Some(cue) = boost_sound {
            sfx.play_from(cue, at);
        }
        last.boosting = boosting;
        // A jet's climb and dive starting and stopping (Hovering.UpdateVerticalDashing:
        // PlayAscendSound, PlayAscendStopSound...), and its engine's start as it moves
        // off and its stop as it comes to rest (its engine's OnLoad and OffLoad
        // oneshots, not out of flying: the Insecticons').
        if p.kit.plane.is_some() {
            use character::{
                SOUND_ASCEND, SOUND_ASCEND_STOP, SOUND_DESCEND, SOUND_DESCEND_STOP, SOUND_MOVE_OFF,
                SOUND_STOP,
            };
            let in_jet = vehicle && alive && p.transform_left <= 0.0;
            let (climbing, diving) = (in_jet && car.jet.climbing, in_jet && car.jet.diving);
            let ahead = in_jet && car.hover && p.move_input.y > 0.01;
            for (now, was, start, stop) in [
                (climbing, last.climbing, SOUND_ASCEND, SOUND_ASCEND_STOP),
                (diving, last.diving, SOUND_DESCEND, SOUND_DESCEND_STOP),
                (ahead, last.ahead, SOUND_MOVE_OFF, SOUND_STOP),
            ] {
                let event = match (now, was) {
                    (true, false) => Some(start),
                    // (Not when it takes off: the engine's Boosting state takes over.)
                    (false, true) if in_jet && car.hover => Some(stop),
                    _ => None,
                };
                if let Some(cue) = event.and_then(|e| chars.car_sound(p.kit.vehicle, e, ""))
                    && !cue.is_empty()
                {
                    sfx.play_from(cue, at);
                }
            }
            last.climbing = climbing;
            last.diving = diving;
            last.ahead = ahead;
        }
        last.impact_cooldown -= dt;
        // (By how fast the car really moves: hovering, its heading follows the camera.)
        let speed = p.velocity.length();
        if vehicle
            && last.vehicle
            && last.impact_cooldown <= 0.0
            && last.drive_speed > 10.0
            && speed < last.drive_speed * 0.8
        {
            let impact = if last.drive_speed > 28.0 {
                car_sound(character::SOUND_IMPACT_HEAVY, SND_CAR_IMPACT_HEAVY)
            } else {
                car_sound(character::SOUND_IMPACT_LIGHT, SND_CAR_IMPACT_LIGHT)
            };
            if let Some(cue) = impact {
                sfx.play_from(cue, at);
            }
            last.impact_cooldown = 0.6;
        }
        last.drive_speed = speed;
        last.vehicle = vehicle;
    }
}

/// The fully charged rifle's glow lets out a ring every this many seconds
/// (NucleonRifle_Charged_FX Ring02: 20 a second).
const CHARGED_RING_SECS: f32 = 0.05;

/// A charging weapon's looping sounds and glow, for every Jazz (the others' as
/// their PCs report them): its scope's hum held on while it is up, the charging
/// loop while it charges and the charged one once it is fully charged
/// (Charge::sound_scope, sound_charging, sound_charged), heard where he is (the
/// local player's by the camera); and while it is fully charged, the glow's rings
/// at its muzzle (Effect::NucleonCharged on MuzzleFlash, CHARGED_RING_SECS apart),
/// carried along as he moves (the game's are in the gun's own space).
fn weapon_loops(
    time: Res<Time>,
    mut players: Query<(Entity, &Transform, &mut Player, Has<LocalPlayer>)>,
    gun_bones: Query<(&GlobalTransform, &GunBone, &JazzOf)>,
    mut sfx: ResMut<Sfx>,
    mut fxq: ResMut<FxQueue>,
) {
    let dt = time.delta_secs();
    for (owner, transform, mut player, local) in &mut players {
        let weapon = WEAPONS.get(player.weapon);
        let charge = weapon.and_then(|w| w.charge);
        let (Some(weapon), Some(charge), Form::Robot, true) = (
            weapon,
            charge,
            player.form,
            player.dead_left <= 0.0 && !player.dying,
        ) else {
            player.charge_ring = 0.0;
            continue;
        };
        let at = voice_at(local, transform.translation);
        let key = owner.to_bits();
        if player.aiming {
            sfx.hold(key, charge.sound_scope, at);
        }
        if player.charging && player.aiming {
            sfx.hold(key, charge.sound_charging, at);
        }
        if !player.charged {
            player.charge_ring = 0.0;
            continue;
        }
        sfx.hold(key, charge.sound_charged, at);
        player.charge_ring -= dt;
        if player.charge_ring > 0.0 {
            continue;
        }
        player.charge_ring = (player.charge_ring + CHARGED_RING_SECS).max(0.0);
        let elbow = gun_bones
            .iter()
            .find(|(_, b, of)| b.0 == player.left_handed && of.0 == owner)
            .map(|(t, _, _)| *t);
        if let (Some(sockets), Some(elbow)) = (weapon.sockets, elbow) {
            let arm = player.left_handed as usize;
            let muzzle = elbow.transform_point(sockets.muzzles[0][arm]);
            // (Along the barrel: from the elbow out to the muzzle.)
            let along = (muzzle - elbow.translation()).normalize_or(*transform.forward());
            fxq.spawn_moving(Effect::NucleonCharged, muzzle, along, player.velocity);
        }
    }
}

/// A vehicle's engine cue (its sound event set's Auto_Engine_Gear_1_OnLoad, Jazz's
/// car's where it gives none), or None for one whose audio component plays no
/// engine: the Dinobots walk and run to their footsteps alone.
fn car_engine(chars: &character::Characters, vehicle: Option<usize>) -> Option<&'static str> {
    chars.car_has_engine(vehicle).then(|| {
        chars
            .car_sound(vehicle, character::SOUND_ENGINE, SND_ENGINE)
            .unwrap_or(SND_ENGINE)
    })
}

/// The recordings of a vehicle's hover sound (its engine's loops that play while it
/// hovers: ENGINES' Hover, Whine and Boosters; none but the jets' have them), by
/// name.
pub(crate) fn hover_waves(
    chars: &character::Characters,
    vehicle: Option<usize>,
) -> impl Iterator<Item = &'static str> {
    engine_loops(car_engine(&chars, vehicle), None)
        .into_iter()
        .filter(|l| {
            matches!(
                l.role,
                EngineRole::Hover | EngineRole::Whine | EngineRole::Boosters
            )
        })
        .map(|l| l.wave)
}

/// A vehicle's hover sound stops as its hovering does. Vortex's transformation into
/// his helicopter starts its main engine's loop as the helicopter comes together
/// (VORTEX_BOT2VEH plays JET_HELI_01_ENGINE_MAIN_LP, 5.3 s of it, from 0.8 s in),
/// before its engine's own loops take over: that recording is cut short (in a
/// tenth of a second) as soon as he flies off (boosting), turns back into a robot or
/// dies, rather than playing out over the boost or the robot. Each Jazz's, the other
/// players' copies too.
fn cut_hover_engines(
    chars: Res<character::Characters>,
    mut sfx: ResMut<Sfx>,
    players: Query<(Entity, &Player, &car::Car)>,
) {
    for (jazz, player, car) in &players {
        let alive = player.dead_left <= 0.0 && !player.dying;
        let becoming = player.transform_left > 0.0 && is_to_vehicle(player.transform_clip);
        let hovering = player.form == Form::Vehicle && car.hover;
        if alive && (becoming || hovering) {
            continue;
        }
        for wave in hover_waves(&chars, player.kit.vehicle) {
            sfx.cut(jazz.to_bits(), wave);
        }
    }
}

/// The owner the hover cue's one-off layers are held for (engine_sound), apart from
/// the Jazz's other held sounds.
const HOVER_ONE_OFFS: u64 = 0x484f_5645_524f_4e45;
/// The owners a car's, truck's or tank's boost loops and its wheels' are held for
/// (engine_sound), and Jazz's car's cues for them (each character's vehicle has its
/// own: character.rs).
const BOOST_LOOPS: [u64; 3] = [
    0x424f_4f53_545f_4c50,
    0x424f_4f53_545f_4558,
    0x424f_4f53_545f_5748,
];
const SND_CAR_BOOST_LOOP: &str = "BL_VEH_CAR.VEH_SPORT_01_BOOST_LOOP";
const SND_CAR_BOOST_LOOP_EXHAUST: &str = "BL_VEH_CAR.VEH_SPORT_01_BOOST_LOOP_EXHAUST";
const SND_CAR_BOOST_WHEELS: &str = "BL_VEH_CAR.VEH_SPORT_01_BOOST_WHEELS";
/// How long into a boost its wheels' sound plays on regardless of the ground under
/// them; past it, once off the ground, it stops till the next boost
/// (HmVehicleAudioComponent's BoostWheelsGroundCheckDelay, UpdateBoostWheelsSound).
const BOOST_WHEELS_GROUND_CHECK: f32 = 0.25;

/// The tyres squeal sliding more than this many degrees off the way the car points,
/// going faster than this (m/s).
const SQUEAL_SLIP: f32 = 6.0;
const SQUEAL_SPEED: f32 = 8.0;
/// How fast the vehicle's loops die away once he is out of it (a share a second:
/// gone within a few frames, no audible tail into robot form).
const ENGINE_STOP_RATE: f32 = 40.0;

/// The local player's vehicle's engine note follows its speed, and its tyres squeal
/// in hard turns: its own engine's loops and squeal (ENGINES, SQUEALS; Jazz's car's
/// without the characters), started afresh as he takes another vehicle. A car,
/// truck or tank boosting plays its boost loops instead of its engine's (its audio
/// component's Boosting state: HmPlayerVehicleAudioComponentImpl.Boosting plays its
/// BoostSounds' BoostLoops, Auto_Boost_Loop and Auto_Boost_Loop_Exhaust, as the
/// other states' loops stop), and its wheels' sound as it starts to boost on the
/// ground (BoostWheelsSound, Auto_Boost_Wheels: PlayBoostSound).
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
fn engine_sound(
    mut commands: Commands,
    time: Res<Time>,
    (volumes, mix): (Res<sound::Volumes>, Res<mixer::Mix>),
    chars: Res<character::Characters>,
    bank: Res<SoundBank>,
    mut sfx: ResMut<Sfx>,
    players: Query<(Entity, &Player, &car::Car, &split::Seat), With<LocalPlayer>>,
    mut voices: Query<(Entity, &mut EngineVoice, Option<&mut AudioSink>)>,
    mut playing: Local<[Option<(Option<&'static str>, Option<&'static str>)>; split::SEATS]>,
    mut flown: Local<[f32; split::SEATS]>,
    mut boost: Local<[(bool, f32, bool); split::SEATS]>,
) {
    // (Each of this PC's players his own: split screen's guest has his.)
    for (jazz, player, car, seat) in &players {
        let i = seat.0.min(split::SEATS - 1);
        engine_for(
            (
                &mut commands,
                &time,
                &volumes,
                &mix,
                &chars,
                &bank,
                &mut sfx,
            ),
            (jazz, player, car, i),
            &mut voices,
            (&mut playing[i], &mut flown[i], &mut boost[i]),
        );
    }
    // (A player gone, his engine with him.)
    for (voice, v, _) in &voices {
        if !players.iter().any(|p| p.3.0 == v.seat) {
            commands.entity(voice).despawn();
            playing[v.seat.min(split::SEATS - 1)] = None;
        }
    }
}

/// One of this PC's players' vehicle's engine (engine_sound).
#[allow(clippy::type_complexity)]
fn engine_for(
    (commands, time, volumes, mix, chars, bank, sfx): (
        &mut Commands,
        &Time,
        &sound::Volumes,
        &mixer::Mix,
        &character::Characters,
        &SoundBank,
        &mut Sfx,
    ),
    (jazz, player, car, seat): (Entity, &Player, &car::Car, usize),
    voices: &mut Query<(Entity, &mut EngineVoice, Option<&mut AudioSink>)>,
    (playing, flown, boost): (
        &mut Option<(Option<&'static str>, Option<&'static str>)>,
        &mut f32,
        &mut (bool, f32, bool),
    ),
) {
    let vehicle = player.kit.vehicle;
    let engine = car_engine(chars, vehicle);
    let squeal = chars.car_sound(vehicle, character::SOUND_TIRE_SQUEAL, SND_TIRE_SQUEAL);
    if *playing != Some((engine, squeal)) {
        *playing = Some((engine, squeal));
        for (voice, v, _) in voices.iter() {
            if v.seat == seat {
                commands.entity(voice).despawn();
            }
        }
        for sound in engine_loops(engine, squeal) {
            if let Some(handle) = bank.wave(sound.wave) {
                commands.spawn((
                    AudioPlayer::new(handle),
                    PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
                    EngineVoice {
                        sound,
                        level: 0.0,
                        seat,
                    },
                ));
            }
        }
        return;
    }
    let driving = player.form == Form::Vehicle && player.dead_left <= 0.0;
    let f = (player.drive_speed.abs() / player.kit.handling.max_speed.max(1.0)).clamp(0.0, 1.0);
    // A jet: hovering or flying, and how long it has flown (its boost loops' curves
    // start over with each flight).
    let jet = player.kit.plane.is_some();
    let flying = jet && driving && !car.hover && player.transform_left <= 0.0;
    let hovering = jet && driving && !flying;
    *flown = if flying {
        *flown + time.delta_secs()
    } else {
        0.0
    };
    let ahead = hovering && player.move_input.y > 0.01;
    // A car, truck or tank boosting (down on its wheels, or a tank's boost): its
    // boost loops, from the start of each boost, in place of its engine's; its
    // wheels' as it starts to boost on the ground, until it leaves the ground.
    let boosting = !jet && driving && !car.hover && player.transform_left <= 0.0;
    let (was_boosting, boost_secs, wheels) = boost;
    if boosting && !*was_boosting {
        *boost_secs = 0.0;
        *wheels = player.grounded;
    } else if boosting {
        *boost_secs += time.delta_secs();
        if *boost_secs >= BOOST_WHEELS_GROUND_CHECK && !player.grounded {
            *wheels = false;
        }
    }
    *was_boosting = boosting;
    if boosting {
        use character::{SOUND_BOOST_LOOP, SOUND_BOOST_LOOP_EXHAUST, SOUND_BOOST_WHEELS};
        let loops = [
            (SOUND_BOOST_LOOP, SND_CAR_BOOST_LOOP, true),
            (SOUND_BOOST_LOOP_EXHAUST, SND_CAR_BOOST_LOOP_EXHAUST, true),
            (SOUND_BOOST_WHEELS, SND_CAR_BOOST_WHEELS, *wheels),
        ];
        for ((event, jazz_cue, on), owner) in loops.into_iter().zip(BOOST_LOOPS) {
            if on && let Some(cue) = chars.car_sound(vehicle, event, jazz_cue) {
                sfx.hold_through(jazz.to_bits() ^ owner, cue, None);
            }
        }
    }
    // (Its hover cue's one-off layers round the loops, again each time round its
    // loop, while it hovers: the Insecticons' wings' buzzes and rustles, as their
    // VEH_INSECTICON_FLYING_LP plays them every 0.31 s, a few passes silent.)
    if let Some(engine) = engine
        && hovering
        && bank.loops_one_offs(engine)
    {
        sfx.hold_one_offs(jazz.to_bits() ^ HOVER_ONE_OFFS, engine, None);
    }
    // The tyres squeal as they slide: the car's slip angle (its sideways speed
    // against its speed ahead: TnCarForm.Driving.UpdateSounds' WheelSlipAngle) past
    // SQUEAL_SLIP at speed.
    let squealing = driving
        && player.grounded
        && !car.hover
        && player.drive_speed > SQUEAL_SPEED
        && player.strafe_speed.abs().atan2(player.drive_speed) > SQUEAL_SLIP.to_radians();
    let gain = |db: f32| 10f32.powf((db + sound::SFX_VOLUME_DB) / 20.0);
    // (The Audio settings' volumes for the engine's cue's class, effects, as its
    // tyres' cue's is; and its cue's mixer category's volume, its tyres' theirs:
    // the game's presets turn them down, a jump the engine 10 dB.)
    let engine_cue = engine.unwrap_or(SND_ENGINE);
    let settings_gain = volumes.gain(bank.class(engine_cue));
    let engine_mix = mix.level(bank.category(engine_cue));
    let squeal_mix = mix.level(squeal.and_then(|c| bank.category(c)));
    // (Easing in and out as he drives; out of the vehicle, its loops go at once,
    // in a few hundredths of a second, as its form's sounds stop with it.)
    let rate = if driving { 6.0 } else { ENGINE_STOP_RATE };
    let k = (time.delta_secs() * rate).min(1.0);
    for (_, mut voice, sink) in voices.iter_mut() {
        if voice.seat != seat {
            continue;
        }
        let sound = voice.sound;
        // (How loud, against its level, and how fast it plays, against its pitch.)
        // (Boosting, its engine's loops stop for its boost's.)
        let engine = if boosting { 0.0 } else { 1.0 };
        let (level, speed) = match sound.role {
            EngineRole::Idle => (engine * (1.0 - 0.75 * f), 0.85 + 0.6 * f),
            EngineRole::Load => (engine * (0.15 + f).min(1.0), 0.8 + 0.5 * f),
            EngineRole::Bed => (engine, 0.9 + 0.3 * f),
            EngineRole::Throttle { rest } => {
                let quiet = 10f32.powf(rest / 20.0);
                (engine * (quiet + (1.0 - quiet) * f), 0.85 + 0.5 * f)
            }
            EngineRole::Squeal => (if squealing { 1.0 } else { 0.0 }, 0.95 + 0.15 * f),
            EngineRole::Hover => (hovering as u8 as f32, 1.0),
            EngineRole::Whine => (ahead as u8 as f32, 1.0),
            EngineRole::Boosters => (if hovering { car.jet.thrust } else { 0.0 }, 1.0),
            EngineRole::Fly { level, pitch } => {
                let level = if flying {
                    curve_at(level, *flown, 1.0)
                } else {
                    0.0
                };
                (level, 2f32.powf(curve_at(pitch, *flown, 0.0) / 12.0))
            }
        };
        let target = if driving { gain(sound.db) * level } else { 0.0 };
        voice.level += (target - voice.level) * k;
        if let Some(mut sink) = sink {
            let mixed = if matches!(sound.role, EngineRole::Squeal) {
                squeal_mix
            } else {
                engine_mix
            };
            sink.set_volume(Volume::Linear(voice.level * settings_gain * mixed));
            sink.set_speed(sound.pitch * speed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An installed copy starts its builder while its game files aren't built with
    /// its build tools; the project's own folder never does.
    #[test]
    fn an_installed_copy_builds_its_game_files_first() {
        let dir = std::env::temp_dir().join(format!("foc_builder_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tools").join("installer")).unwrap();
        assert!(
            !needs_builder(&dir),
            "no build tools' id: not an installed copy"
        );
        let id = dir.join("tools").join("installer").join("pipeline.txt");
        std::fs::write(&id, "02c73cfe4c71c208\r\n").unwrap();
        assert!(needs_builder(&dir), "not built yet");
        std::fs::write(dir.join("game_files.txt"), "02c73cfe4c71c208\n").unwrap();
        assert!(!needs_builder(&dir), "built with these tools");
        std::fs::write(&id, "9d1e0a77b2c34f10\n").unwrap();
        assert!(needs_builder(&dir), "built with other tools");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Whirlwind's clip is held where its clock puts it: once through, then
    /// round and round its spin (from LOOP_FROM), never back to its wind-up however
    /// long he spins, nor at its very end (which would show its start); and so is
    /// the Heal Beam's arm, round from its loop point, his hand held up (its length
    /// as robot_extras.txt keeps it, 1.033 s, a hair short of the clip's).
    #[test]
    fn the_whirlwind_clip_stays_in_its_spin() {
        let secs = whirlwind::CLIP_SECS;
        assert!((overlay_time(0.5, secs) - 0.5).abs() < 1e-6);
        let mut t = 0.0;
        while t < 6.0 {
            let at = overlay_time(whirlwind::clip_time(t), secs);
            assert!(at < secs, "{t}: {at}");
            if t >= secs {
                assert!(at >= whirlwind::LOOP_FROM - 1e-4, "{t}: {at}");
            }
            t += 1.0 / 60.0;
        }
        assert!((overlay_time(2.5, 2.0) - 0.5).abs() < 1e-5);
        let secs = 1.033;
        let mut t = 0.0;
        while t < 8.0 {
            let at = overlay_time(healbeam::clip_time(t), secs);
            assert!(at < secs, "{t}: {at}");
            if t >= healbeam::CLIP_SECS {
                assert!(at >= healbeam::LOOP_FROM - 1e-4, "{t}: {at}");
            }
            t += 1.0 / 240.0;
        }
    }

    /// Each class's health is the multiplayer game's: the Infiltrator and the
    /// Scientist 300, the Destroyer 400, the Titan 500.
    #[test]
    fn class_health() {
        let by_name =
            |name: &str| max_health(character::CLASSES.iter().position(|c| *c == name).unwrap());
        assert_eq!(by_name("infiltrator"), 300.0);
        assert_eq!(by_name("scientist"), 300.0);
        assert_eq!(by_name("destroyer"), 400.0);
        assert_eq!(by_name("titan"), 500.0);
        // (A new player starts full.)
        let p = new_player();
        assert_eq!(p.health, p.max_health());
    }

    /// A melee blow is Escalation's (SharedMeleeSet): the sword's 75, the sprint
    /// punch's 150, both pushing 75000 along the ground from the striker and 75000
    /// up (ServerProcessMeleeHit), which over a soldier's Mass of 100 would have it
    /// in the air for over half a second: knocked back (TnHtnAiPawn.TakeDamage),
    /// where multiplayer's sword (20000 and 10000) only nudged it.
    #[test]
    fn a_melee_blow_knocks_a_soldier_back() {
        assert_eq!(MELEE_DAMAGE, 75.0);
        assert_eq!(MELEE_SPRINT_DAMAGE, 150.0);
        let hit = melee_hit(
            Vec3::new(3.0, 2.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::NEG_Z,
        );
        // (Away from him along the ground, whatever way he faces.)
        let along = Vec3::new(hit.momentum.x, 0.0, hit.momentum.z);
        assert!(along.normalize().dot(Vec3::X) > 0.999, "{}", hit.momentum);
        assert!((along.length() - 75000.0).abs() < 1.0);
        assert!((hit.momentum.y - 75000.0).abs() < 1.0);
        let soldier =
            ai::pawn::Pawn::new(Entity::PLACEHOLDER, Vec3::ZERO, 0.0, 150.0, 220.0, 100.0);
        let v = ai::ue::dir_to_ue(hit.momentum) / soldier.mass;
        let air = ai::pawn::Pawn::estimate_air_time(v);
        assert!(air > 0.5 && air < 0.52, "{air}");
        let mp = ai::ue::dir_to_ue(Vec3::new(20000.0, 10000.0, 0.0)) / soldier.mass;
        assert!(ai::pawn::Pawn::estimate_air_time(mp) < 0.5);
    }

    /// Changing form either way, every weapon he carries is reloaded from its reserve
    /// 1 s into the change (TransformTo*ShootingDelay: SwitchToFormWeapon's
    /// ReloadAllWeapons), not before: his two guns and his vehicle's weapon, a clip
    /// filled only as far as its reserve allows, a gun he hasn't got left as it is;
    /// a change with no animation (his model not in yet) reloads them at once.
    #[test]
    fn transforming_reloads_every_weapon() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let mut p = new_player();
        let [a, b] = p.loadout;
        let v = vehicle_slot(&p);
        let other = (0..WEAPONS.len()).find(|w| !p.loadout.contains(w)).unwrap();
        for s in [a, v, other] {
            p.ammo[s] = (0, gun(s).reserve_max);
        }
        // (Fewer in reserve than the clip has room for.)
        p.ammo[b] = (1, 3);
        let owner = world
            .spawn((
                p,
                PlayerInput {
                    transform: true,
                    ..default()
                },
            ))
            .id();
        let animator = world.spawn((RobotAnimator, JazzOf(owner))).id();
        let step = |world: &mut World, secs: f32| {
            world
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(secs));
            world.run_system_once(toggle_form).unwrap();
            world.get_mut::<PlayerInput>(owner).unwrap().transform = false;
        };
        let ammo = |world: &World, s: usize| world.get::<Player>(owner).unwrap().ammo[s];
        let full = |s: usize| (gun(s).clip_size, gun(s).reserve_max - gun(s).clip_size);

        // To the vehicle: nothing yet 0.9 s in, all reloaded by 1 s.
        step(&mut world, 0.0);
        assert!(world.get::<Player>(owner).unwrap().pending_form == Some(Form::Vehicle));
        step(&mut world, 0.9);
        assert_eq!(ammo(&world, a), (0, gun(a).reserve_max));
        step(&mut world, 0.15);
        assert_eq!(ammo(&world, a), full(a));
        assert_eq!(ammo(&world, b), (4, 0));
        assert_eq!(ammo(&world, v), full(v));
        assert_eq!(ammo(&world, other), (0, gun(other).reserve_max));
        assert!(
            world
                .get::<Player>(owner)
                .unwrap()
                .form_weapon_left
                .is_none()
        );

        // Back to the robot (once the change is over), the same 1 s in.
        step(&mut world, 3.0);
        assert!(world.get::<Player>(owner).unwrap().form == Form::Vehicle);
        world.get_mut::<Player>(owner).unwrap().ammo[a] = (2, 50);
        world.get_mut::<PlayerInput>(owner).unwrap().transform = true;
        step(&mut world, 0.0);
        assert!(world.get::<Player>(owner).unwrap().form == Form::Robot);
        step(&mut world, 0.5);
        assert_eq!(ammo(&world, a), (2, 50));
        step(&mut world, 0.55);
        let clip = gun(a).clip_size;
        assert_eq!(ammo(&world, a), (clip, 52 - clip));

        // No animation: at once.
        step(&mut world, 3.0);
        world.despawn(animator);
        world.get_mut::<Player>(owner).unwrap().ammo[a] = (0, 50);
        world.get_mut::<PlayerInput>(owner).unwrap().transform = true;
        step(&mut world, 0.0);
        assert!(world.get::<Player>(owner).unwrap().form == Form::Vehicle);
        assert_eq!(ammo(&world, a), (clip, 50 - clip));
    }

    /// The key the contextual command line shows for Interact (a console's, a
    /// revive's, an Armory Recreator's line take it too) is kept up with nothing in
    /// reach to pick up or buy: with a controller its X, else the keyboard's key.
    /// (It was kept up only at a weapon or a sale: at a door a controller's player
    /// saw the keyboard's E.)
    #[test]
    fn the_interact_key_is_shown_with_nothing_to_pick_up() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Score>()
            .init_resource::<Sfx>()
            .init_resource::<HudState>()
            .init_resource::<Controls>()
            .init_resource::<voice::VoiceLines>()
            .init_resource::<pad::Pad>()
            // (Split screen's guest's, there whether or not he is.)
            .init_resource::<split::Second<Score>>()
            .init_resource::<split::Second<HudState>>()
            .init_resource::<pad::GuestPad>();
        let world = app.world_mut();
        world.resource_mut::<pad::Pad>().active = true;
        // Jazz with nothing in reach to pick up or buy (at a door's console, say).
        world.spawn((
            Transform::default(),
            new_player(),
            PlayerInput::default(),
            LocalPlayer,
        ));
        world.run_system_once(weapon_pickups).unwrap();
        let hud = world.resource::<HudState>();
        assert!(hud.prompt.is_none());
        assert_eq!(hud.pickup_key, "X");
        assert!(!hud.pickup_key_mouse);
        // And back on the keyboard, its key.
        world.resource_mut::<pad::Pad>().active = false;
        world.run_system_once(weapon_pickups).unwrap();
        let key = world
            .resource::<Controls>()
            .key_name(Mode::Robot, Action::Interact);
        assert_eq!(world.resource::<HudState>().pickup_key, key);
    }

    /// Online, another player's Jazz comes in out of the match until his PC says he
    /// is in it: while the match is starting (the players choosing, the countdown)
    /// the host sees nobody living, however long it takes, so the team's count of
    /// its lives doesn't start (the other player's Jazz counted as living until his
    /// first word, so the three lives ran out in the 9 s after it and the game
    /// ended before the match began); once they are in, nobody up for 3 s takes a
    /// life, as before.
    #[test]
    fn the_team_count_waits_for_the_players_to_come_in() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<Scene>()
            .init_asset::<bevy::gltf::Gltf>()
            .insert_resource(net::host_in_match(&[1]))
            .init_resource::<Level>()
            .init_resource::<revive::Team>();
        let world = app.world_mut();
        // This PC's Jazz, waiting as the match starts (start_match), and the other
        // player's, as net_puppets brings it in before his PC has said a word.
        let mine = world
            .spawn(Player {
                waiting: true,
                ..new_player()
            })
            .id();
        world.run_system_once(net_puppets).unwrap();
        let mut others = world.query_filtered::<Entity, With<RemotePlayer>>();
        let other = others.single(world).expect("the other player's Jazz");
        assert!(world.get::<Player>(other).unwrap().waiting);
        let step = |world: &mut World, secs: f32| {
            for _ in 0..(secs / 0.05).round() as usize {
                world
                    .resource_mut::<Time>()
                    .advance_by(std::time::Duration::from_secs_f32(0.05));
                world.run_system_once(revive::team_lives).unwrap();
            }
        };
        let lives = |world: &World| world.resource::<revive::Team>().lives;

        // A second before his PC's first word (still loading the map), then its
        // word that he is choosing (net_apply takes his snapshot's WAITING), and
        // the choosing and the countdown for 20 s: no life goes.
        step(world, 1.0);
        world.get_mut::<Player>(other).unwrap().waiting = true;
        step(world, 20.0);
        assert_eq!(lives(world), revive::LIVES);
        assert!(!world.resource::<revive::Team>().over);

        // In the match (his PC says so, net_apply), then both dead: 3 s, a life.
        for e in [mine, other] {
            world.get_mut::<Player>(e).unwrap().waiting = false;
        }
        step(world, 1.0);
        for e in [mine, other] {
            world.get_mut::<Player>(e).unwrap().dead_left = 30.0;
        }
        step(world, 2.9);
        assert_eq!(lives(world), revive::LIVES);
        step(world, 0.2);
        assert_eq!(lives(world), revive::LIVES - 1);
    }

    /// The gun goes away as he sprints and comes back as he stops, as the game's
    /// TnWeapon.ShouldHolster has it: sprinting forward on the ground, the gun folds
    /// away on its CloudB copy (its Unequip_WEP) while his forearm set's Equip_ARM
    /// unfolds the forearm on its arm (a second), and its idle stops; stopping, it
    /// unfolds again (Equip_WEP) as the forearm folds away (Unequip_ARM). A jump out
    /// of the sprint brings it back (the acrobatics' Jumping state isn't
    /// Sprinting), as does a reload; a swap while it is away plays nothing; and a
    /// transformation puts it back in hand at once, playing nothing.
    /// An enemy's melee blow (TnDamageTypeMelee: a Scrapper's fist, ai/melee.rs)
    /// makes him react by the side it came from (hitreact.rs), his gun, reload and
    /// ability off till the reaction completes; a Leaper's smash (its own blow, not
    /// a melee set's) plays his old hit reaction instead, and he doesn't react with
    /// his Flak Shield up or in his vehicle.
    #[test]
    fn a_melee_blow_staggers_him() {
        // (He faces -Z; the blow goes from where it was struck from.)
        let blow = |from: Vec3, damage: &'static str| {
            enemies::Hit::new(enemies::Death::Melee, Vec3::ZERO, -from).of_type(damage)
        };
        let ahead = Vec3::NEG_Z;
        for (from, clip) in [(ahead - Vec3::X, 0), (ahead + Vec3::X, 1), (-ahead, 2)] {
            let mut p = new_player();
            assert!(damage_player(&mut p, 25.0, blow(from, "TnDamageTypeMelee")));
            assert!(p.melee_react.reacting());
            assert_eq!(p.melee_react.playing.last().map(|r| r.clip), Some(clip));
            assert!(p.action.is_none(), "not the old hit reaction");
        }
        let mut p = new_player();
        damage_player(&mut p, 25.0, blow(ahead, "TnDamageTypeLeapSmash"));
        assert!(!p.melee_react.reacting());
        assert_eq!(p.action.map(|a| a.0), Some(ANIM_HIT));
        let mut p = new_player();
        p.ability.kind = abilities::Kind::Shield;
        p.ability.active = true;
        damage_player(&mut p, 25.0, blow(-ahead, "TnDamageTypeMelee"));
        assert!(!p.melee_react.reacting());
        let mut p = new_player();
        p.form = Form::Vehicle;
        damage_player(&mut p, 25.0, blow(ahead, "TnDamageTypeMelee"));
        assert!(!p.melee_react.reacting());
        // Swinging his sword, he doesn't (PlayHitReaction: IsAttacking).
        let mut p = new_player();
        start_action(&mut p, ANIM_MELEE, 1.0, 1.0);
        damage_player(&mut p, 25.0, blow(ahead, "TnDamageTypeMelee"));
        assert!(!p.melee_react.reacting());
    }

    #[test]
    fn the_gun_goes_away_while_he_sprints() {
        let assets = assets_dir();
        let chars = character::Characters::load(&assets);
        let anims = RobotAnimations {
            graphs: Vec::new(),
            nodes: (0..ANIM_TOTAL).map(AnimationNodeIndex::new).collect(),
            mirrored: None,
            deaths: Vec::new(),
            trigger: None,
            downed: None,
            melee_react: None,
            ability: Vec::new(),
            aim: None,
        };
        let mut animation_player = AnimationPlayer::default();
        let mut state = RobotAnimState::default();
        let mut p = new_player();
        p.kit = chars.kit(&character::Look::DEFAULT);
        let gun = &WEAPONS[p.weapon];
        let arm_away = p.kit.arm_away.map(extra_clip);
        let arm_back = p.kit.arm_back.map(extra_clip);
        if chars.available {
            assert!(
                arm_away.is_some() && arm_back.is_some(),
                "{:?}",
                p.kit.arm_set
            );
        }
        let dt = 1.0 / 60.0;
        let mut run = |p: &mut Player, state: &mut RobotAnimState, secs: f32| {
            for _ in 0..(secs / dt).round() as usize {
                animate_jazz(&anims, dt, &mut animation_player, p, state);
            }
        };
        run(&mut p, &mut state, 0.5);
        assert!(!p.holstered && p.holster_change.is_none());
        assert!(state.weights[gun.idle_anim] > 0.5);
        // Sprinting forward.
        p.sprinting = true;
        p.move_input = Vec2::new(0.0, 1.0);
        assert_eq!(holsters(&p), Some(true));
        run(&mut p, &mut state, 0.3);
        assert!(p.holstered);
        assert!(p.unequipping.is_some_and(|(w, _)| w == p.weapon));
        assert!(state.weights[gun.unequip_anim] > 0.9);
        assert!(state.weights[gun.idle_anim] < 0.5);
        if let Some(clip) = arm_away {
            assert!(state.weights[clip] > 0.9);
        }
        run(&mut p, &mut state, 1.0);
        assert!(p.holstered && p.holster_change.is_none() && p.unequipping.is_none());
        assert_eq!(state.weights[gun.idle_anim], 0.0);
        if let Some(clip) = arm_away {
            assert_eq!(state.weights[clip], 0.0);
        }
        // A swap while it is away: the new gun goes straight into the forearm.
        let other = p.loadout[1 - p.hand];
        p.hand = 1 - p.hand;
        p.weapon = other;
        p.start_equip = Some((other, Some(gun_index(gun))));
        run(&mut p, &mut state, 0.2);
        assert!(p.holstered && p.equipping.is_none() && p.unequipping.is_none());
        assert!(state.weights[WEAPONS[other].equip_anim] < 0.01);
        // Stopping: it comes back out.
        p.sprinting = false;
        run(&mut p, &mut state, 0.3);
        assert!(!p.holstered && p.holster_change.is_some());
        assert!(p.equipping.is_some_and(|(w, _)| w == other));
        if let Some(clip) = arm_back {
            assert!(state.weights[clip] > 0.9);
        }
        run(&mut p, &mut state, 1.0);
        assert!(p.holster_change.is_none());
        // A jump out of a sprint, or a reload, brings it back.
        p.sprinting = true;
        run(&mut p, &mut state, 1.2);
        assert!(p.holstered);
        p.jump_time = Some(0.0);
        p.grounded = false;
        assert_eq!(holsters(&p), Some(false));
        p.jump_time = None;
        p.grounded = true;
        p.reload_left = 1.0;
        assert_eq!(holsters(&p), Some(false));
        p.reload_left = 0.0;
        // Sideways it is a jog, not a sprint.
        p.move_input = Vec2::new(1.0, 0.0);
        assert_eq!(holsters(&p), Some(false));
        p.move_input = Vec2::new(0.0, 1.0);
        // A transformation: back at once, nothing played.
        p.transform_left = 1.0;
        assert_eq!(holsters(&p), None);
        run(&mut p, &mut state, dt);
        assert!(!p.holstered && p.holster_change.is_none());
    }

    /// The index of a weapon in WEAPONS.
    fn gun_index(gun: &WeaponStats) -> usize {
        WEAPONS.iter().position(|w| std::ptr::eq(w, gun)).unwrap()
    }

    /// Another player's copy, out till the next wave (his snapshots saying he is
    /// dead, his wait at the hold), stays dead between his snapshots, so his marker
    /// stays off; his death's wait still runs down, and he is back only when his PC
    /// says so (a snapshot's 0).
    #[test]
    fn a_dead_copy_stays_dead_between_snapshots() {
        let mut left = DEAD_HOLD;
        for _ in 0..10 {
            left = remote_dead_left(left, 1.0 / 60.0);
            assert!(left > 0.0);
        }
        assert!((remote_dead_left(RESPAWN_SECS, 1.0) - (RESPAWN_SECS - 1.0)).abs() < 1e-6);
        assert_eq!(remote_dead_left(0.0, 1.0 / 60.0), 0.0);
    }

    /// Going from his body to a teammate, the spectating camera cuts to the
    /// teammate as far off him as it was off the body, swings round him (never
    /// passing through him, as a straight glide from in front of him to behind him
    /// would) and ends behind him; to the level's camera it glides straight there.
    #[test]
    fn spectating_swings_round_the_player_watched() {
        let body = Vec3::ZERO;
        let mate = Vec3::new(10.0, 0.0, 0.0);
        // The death camera 5 m behind his body (looking down -Z), 2 m up; the
        // spectating camera 7 m behind the teammate, looking the other way.
        let from = (body + Vec3::new(0.0, 2.0, 5.0), Quat::IDENTITY, Some(body));
        let turn = Quat::from_rotation_y(PI);
        let to = (mate + Vec3::new(0.0, 2.0, -7.0), turn, Some(mate));
        let (at, _) = spectate_place(from, to, 0.0);
        assert!((at - (mate + Vec3::new(0.0, 2.0, 5.0))).length() < 1e-4);
        let (at, turned) = spectate_place(from, to, 1.0);
        assert!((at - to.0).length() < 1e-4);
        assert!(turned.angle_between(turn) < 1e-4);
        for i in 0..=20 {
            let (at, _) = spectate_place(from, to, i as f32 / 20.0);
            assert!((at - mate).length() > 5.0, "{i}: {at}");
        }
        let (at, _) = spectate_place(from, (to.0, turn, None), 0.5);
        assert!((at - from.0.lerp(to.0, 0.5)).length() < 1e-4);
    }

    /// A truck's Eclipse Rockets fire one burst of two for a click, however long the
    /// click lasts into the burst, and burst after burst while the trigger is held.
    #[test]
    fn a_truck_fires_one_burst_for_a_click_and_bursts_while_held() {
        // (fire_rockets' cadence: the trigger held for `held` seconds of a 3 s run at
        // 60 frames a second; the rockets' times.)
        let run = |held: f32| {
            let mut p = new_player();
            p.ammo[TRUCK_SLOT] = (8, 24);
            let r = &TRUCK_ROCKETS;
            let dt = 1.0 / 60.0;
            let mut fired = Vec::new();
            for frame in 0..180 {
                let t = frame as f32 * dt;
                p.fire_cooldown -= dt;
                if rocket_burst_starts(&mut p, r, frame == 0, t < held) {
                    p.burst_left = r.burst;
                    p.fire_cooldown = p.fire_cooldown.max(0.0);
                }
                if p.burst_left > 0 && p.fire_cooldown <= 0.0 {
                    fired.push(t);
                    p.ammo[TRUCK_SLOT].0 -= 1;
                    rocket_fired(&mut p, r, dt);
                }
            }
            fired
        };
        // A click (held a tenth of a second or a quarter, into the burst): its two.
        for held in [0.1, 0.25] {
            let fired = run(held);
            assert_eq!(fired.len(), 2, "held {held} s: {fired:?}");
            assert!((fired[1] - fired[0] - 0.15).abs() < 0.02, "{fired:?}");
        }
        // Held: burst after burst while it is, each 0.7 s after the last's second
        // rocket.
        let fired = run(1.0);
        assert_eq!(fired.len(), 4, "{fired:?}");
        assert!((fired[2] - fired[1] - 0.7).abs() < 0.02, "{fired:?}");
    }

    /// A jet flies under an arch's beam its rigid body's box clears (Vortex's: its
    /// rotor no part of it), where the vehicle body's circles, as wide as its rotor,
    /// would reach up into the beam; it is still stopped by the beam's pillars.
    #[test]
    fn a_jet_flies_under_an_arch() {
        // A floor, and a beam across x from -10 to 10 at z = 0, its face 5 m to 8 m up,
        // on a pillar at x = 10.
        let tris: Vec<[f32; 9]> = vec![
            [-40.0, 0.0, -40.0, -40.0, 0.0, 40.0, 40.0, 0.0, 40.0],
            [-40.0, 0.0, -40.0, 40.0, 0.0, 40.0, 40.0, 0.0, -40.0],
            [-10.0, 5.0, 0.0, 10.0, 5.0, 0.0, 10.0, 8.0, 0.0],
            [-10.0, 5.0, 0.0, 10.0, 8.0, 0.0, -10.0, 8.0, 0.0],
            [10.0, 0.0, -1.0, 10.0, 0.0, 1.0, 10.0, 8.0, 1.0],
            [10.0, 0.0, -1.0, 10.0, 8.0, 1.0, 10.0, 8.0, -1.0],
        ];
        let level = Level {
            collision: Some(LevelCollision::from_parts(&tris, &[], 4.0)),
            ..default()
        };
        let vortex = character::Hull {
            ahead: 2.463,
            behind: 5.562,
            half_width: 2.067,
            bottom: 0.022,
            top: 3.165,
        };
        let rotor = character::Body {
            half_width: 4.315,
            ahead: 3.527,
            behind: 5.699,
        };
        let (lift, forward) = (0.7268, Vec3::NEG_Z);
        // Hovering at the floor under the beam, nose through the arch.
        let at = Vec3::new(0.0, 0.0, 1.0);
        assert!(level.push_out_hull(at, forward, &vortex, lift).is_none());
        // (The rotor-wide circles are stopped by the beam over it.)
        assert!(level.push_out_car(at, forward, 3.06, rotor).is_some());
        // Higher up its box reaches the beam's face.
        let high = Vec3::new(0.0, 2.5, 1.0);
        assert!(level.push_out_hull(high, forward, &vortex, lift).is_some());
        // Its side against the pillar.
        let beside = Vec3::new(8.6, 0.0, 0.0);
        assert!(
            level
                .push_out_hull(beside, forward, &vortex, lift)
                .is_some()
        );
    }

    /// A jet (Vortex's) unfolding with its back to a thin door, its tail through
    /// it, is moved out on its own side of the door, not pushed on through it by
    /// the parts already past it; and the same for a car.
    #[test]
    fn a_vehicle_unfolding_against_a_door_stays_on_its_side() {
        // A floor, and a door 10 cm thick across x from -6 to 6 at z = 0, 8 m tall.
        let mut tris: Vec<[f32; 9]> = vec![
            [-40.0, 0.0, -40.0, -40.0, 0.0, 40.0, 40.0, 0.0, 40.0],
            [-40.0, 0.0, -40.0, 40.0, 0.0, 40.0, 40.0, 0.0, -40.0],
        ];
        for z in [-0.05f32, 0.05] {
            tris.push([-6.0, 0.0, z, 6.0, 0.0, z, 6.0, 8.0, z]);
            tris.push([-6.0, 0.0, z, 6.0, 8.0, z, -6.0, 8.0, z]);
        }
        let level = Level {
            collision: Some(LevelCollision::from_parts(&tris, &[], 4.0)),
            ..default()
        };
        let vortex = character::Hull {
            ahead: 2.463,
            behind: 5.562,
            half_width: 2.067,
            bottom: 0.022,
            top: 3.165,
        };
        let lift = 0.7268;
        // He stood 0.6 m in front of the door, facing away from it (+z).
        let at = Vec3::new(0.0, 0.0, 0.6);
        let push = level
            .push_out_hull(at, Vec3::Z, &vortex, lift)
            .expect("its tail is in the door");
        let moved = at + Vec3::new(push.x, 0.0, push.y);
        assert!(moved.z > 3.0, "{moved}");
        assert!(
            level.push_out_hull(moved, Vec3::Z, &vortex, lift).is_none(),
            "{moved}"
        );
        // A car (Jazz's body) the same.
        let body = character::Body {
            half_width: 0.9,
            ahead: 2.2,
            behind: 2.1,
        };
        let push = level
            .push_out_car(at, Vec3::Z, 1.15, body)
            .expect("its back is in the door");
        let moved = at + Vec3::new(push.x, 0.0, push.y);
        assert!(moved.z > 1.0, "{moved}");
        assert!(
            level.push_out_car(moved, Vec3::Z, 1.15, body).is_none(),
            "{moved}"
        );
    }

    /// The sword swing with a Soldier under the crosshair 6 m ahead (the videos'):
    /// Jazz flies at it at the lunge's speed (25 m/s by the swing's 0.75) and stops
    /// dead as he reaches it (their two collision radii apart), well inside the lunge's
    /// quarter of a second, and stands there while the swing lasts; swinging at
    /// nothing, he lunges all the same, straight ahead the way he looks, for the
    /// whole quarter of a second (4.7 m), then stands.
    #[test]
    fn a_sword_swing_lunges_at_the_enemy_under_the_crosshair() {
        let tris: Vec<[f32; 9]> = vec![
            [-40.0, 0.0, -40.0, -40.0, 0.0, 40.0, 40.0, 0.0, 40.0],
            [-40.0, 0.0, -40.0, 40.0, 0.0, 40.0, 40.0, 0.0, -40.0],
        ];
        let level = Level {
            collision: Some(LevelCollision::from_parts(&tris, &[], 4.0)),
            ..default()
        };
        let colliders = Colliders::default();
        let swing = |pick: Option<melee::AssistTarget>| {
            let mut player = new_player();
            player.grounded = true;
            let mut transform = Transform::default();
            let mut car = car::Car::default();
            let mut keys = PlayerInput {
                melee: true,
                view_origin: Vec3::new(0.6, 2.5, 3.0),
                view_dir: Vec3::NEG_Z,
                ..default()
            };
            let dt = 1.0 / 60.0;
            let mut path = Vec::new();
            for frame in 0..60 {
                move_jazz(
                    dt,
                    &colliders,
                    &level,
                    &mut transform,
                    &mut player,
                    &mut car,
                    (&keys, if frame == 0 { pick } else { None }),
                    Vec3::ZERO,
                );
                keys.melee = false;
                path.push((frame as f32 * dt, transform.translation, player.lunge));
            }
            path
        };
        let soldier = melee::AssistTarget {
            entity: Entity::from_raw_u32(7).unwrap(),
            aim: Vec3::new(0.0, 1.6, -6.0),
            middle: Vec3::new(0.0, 1.2, -6.0),
            radius: 0.6,
            pick: 3.75,
        };
        let path = swing(Some(soldier));
        let radii = 0.6 + new_player().size().0;
        let (_, after_one, _) = path[0];
        // (A frame in: 25 x 0.75 m/s for 1/60 s.)
        assert!((-after_one.z - 18.75 / 60.0).abs() < 0.05, "{after_one}");
        let stopped = path
            .iter()
            .position(|(_, _, l)| l.is_some_and(|l| l.reached))
            .expect("he reached it");
        let (t, at, _) = path[stopped];
        assert!(t < melee::DASH_SECS, "reached at {t} s");
        assert!(
            (-6.0 - at.z - -radii).abs() < 0.4 && at.x.abs() < 0.05,
            "{at}, radii {radii}"
        );
        // Standing there for the rest of the swing.
        let (_, end, _) = path[path.len() - 1];
        assert!(end.distance(at) < 1e-3, "{at} -> {end}");
        // With nothing under the crosshair: straight ahead, 18.75 m/s for 0.25 s.
        let path = swing(None);
        let (_, lunged, _) = path[59];
        assert!(
            (-lunged.z - 18.75 * melee::DASH_SECS).abs() < 0.35 && lunged.x.abs() < 1e-3,
            "{lunged}"
        );
        let (_, done, _) = path[20];
        assert!(done.distance(lunged) < 0.05, "{done} -> {lunged}");
    }

    /// The guns' flashes are the game's own particle systems as fx/weapons_fx.txt
    /// has them: the Neutron Assault Rifle's AssaultRifleArm_MuzzleFlash_FX (its
    /// flame and black smoke caps on the cap mesh, spark bolts, haze and glow, and not
    /// the dark smoke the game never draws), Subsonic_Repeater_MuzzleFlash_FX (its
    /// squiggly caps, its Ion cores, glows and starburst; on its MuzzleFlash2 socket
    /// too), the Photon Burst Rifle's, the Scatter Blaster's (not its dark smoke
    /// either), the Nucleon Charge Rifle's and the Path Blaster's, each on its barrel
    /// (the Photon Burst Rifle's now has its socket).
    #[test]
    fn the_rifles_flash_is_the_games() {
        let rifle = WEAPONS.iter().find(|w| w.id == "AssaultRifle").unwrap();
        assert_eq!(rifle.muzzle_system, Some("NeutronRifle_MuzzleFlash_FX"));
        let subsonic = WEAPONS.iter().find(|w| w.id == "IonBlaster").unwrap();
        assert_eq!(
            subsonic.muzzle_system,
            Some("Subsonic_Repeater_MuzzleFlash_FX")
        );
        // (On its MuzzleFlash2 socket too.)
        assert!(subsonic.sockets.and_then(|s| s.flash).is_some());
        for (id, system) in [
            ("BurstRifle", "Photon_BurstRifle_MuzzleFlash_Fx"),
            ("Shotgun", "Scatterblaster_MuzzleFlash_FX"),
            ("SniperRifle", "NucleonRifle_MuzzleFlash_FX"),
            ("HeavyPistol", "PathBlaster_MuzzleFlash_FX"),
        ] {
            let w = WEAPONS.iter().find(|w| w.id == id).unwrap();
            assert_eq!(w.muzzle_system, Some(system), "{id}");
            // (Played on its barrel: it has its sockets.)
            assert!(w.sockets.is_some(), "{id}");
        }
        assert_eq!(
            WEAPONS.iter().filter(|w| w.muzzle_system.is_some()).count(),
            6
        );
        let Ok(text) = std::fs::read_to_string(assets_dir().join("fx").join("weapons_fx.txt"))
        else {
            return;
        };
        let emitters = |system: &str| {
            let mut names = Vec::new();
            let mut inside = false;
            for line in text.lines() {
                if line.starts_with("system ") {
                    inside = line.split_whitespace().nth(2) == Some(system);
                } else if inside && let Some(name) = line.strip_prefix("emitter ") {
                    names.push(name.to_string());
                }
            }
            names
        };
        assert_eq!(
            emitters("NeutronRifle_MuzzleFlash_FX"),
            [
                "Flash_Black",
                "Flash_Blast",
                "Spark_Bolts",
                "SMOKE",
                "MOD_Glow_Flash_Export"
            ]
        );
        assert_eq!(
            emitters("Subsonic_Repeater_MuzzleFlash_FX"),
            [
                "Flash_Cap",
                "Flash_Blast",
                "MOD_Glow",
                "MOD_Glow_Flash",
                "MOD_Glow_Flash_Dup",
                "BlastCap_glow_Export"
            ]
        );
        assert_eq!(
            emitters("Photon_BurstRifle_MuzzleFlash_Fx"),
            [
                "Streak",
                "BlastCap",
                "BlastCap_chunkybits",
                "BlastCap_glow",
                "ParticleEmitter_7738",
                "ModGlow",
                "Streak_Dup"
            ]
        );
        assert_eq!(
            emitters("Scatterblaster_MuzzleFlash_FX"),
            [
                "SmokeFront",
                "DistortionFlash",
                "SmokeBlendLite",
                "SmokeBlendFB",
                "DistortionRings",
                "MuzzleCore",
                "SparksSM",
                "SparksLG",
                "ModFlash"
            ]
        );
        assert_eq!(
            emitters("NucleonRifle_MuzzleFlash_FX"),
            [
                "Flash_Blast_Export",
                "SmokeBlendLite_Export",
                "SmokeFront_Export"
            ]
        );
        assert_eq!(
            emitters("PathBlaster_MuzzleFlash_FX"),
            [
                "MuzzleCap_Dup",
                "MuzzleCore_Export_Export",
                "MuzzleCap_Export_Export",
                "Flash_Blast_Export",
                "SmokeBlendLite_Export",
                "SmokeFront_Export"
            ]
        );
    }

    /// Over 1.5 s of frames, a reload's steam is due its seven puffs, numbered 0 to
    /// 6 (each of its emitters letting out only as many as its peak holds), and a
    /// steaming shell's its two wisps, at the start and 0.75 s on.
    #[test]
    fn steam_comes_out_puff_by_puff() {
        let dt = 1.0 / 60.0;
        let mut puffs = Vec::new();
        let mut wisps = Vec::new();
        let mut t = 0.0;
        while t <= STEAM_SECS + 0.1 {
            puffs.extend(steam_puffs(t, t + dt));
            if t <= STEAM_SECS {
                for _ in 0..shell_steam_wisps(t, t + dt) {
                    wisps.push(t);
                }
            }
            t += dt;
        }
        assert_eq!(puffs, (0..7).collect::<Vec<u32>>());
        assert_eq!(wisps.len(), 2);
        assert!(wisps[0] < dt && (wisps[1] - 0.75).abs() <= dt, "{wisps:?}");
    }

    /// A jump shakes the view lightly as he takes off (Nav_TakeOff_01's shake) and
    /// harder as he lands, by the landing clip the fall picks (Nav_Land_02's for a
    /// jump's 6.5 m), running or standing; a step down shakes nothing.
    #[test]
    fn landing_from_a_jump_shakes_the_view() {
        let tris: Vec<[f32; 9]> = vec![
            [-40.0, 0.0, -40.0, -40.0, 0.0, 40.0, 40.0, 0.0, 40.0],
            [-40.0, 0.0, -40.0, 40.0, 0.0, 40.0, 40.0, 0.0, -40.0],
        ];
        let level = Level {
            collision: Some(LevelCollision::from_parts(&tris, &[], 4.0)),
            ..default()
        };
        let colliders = Colliders::default();
        for run in [Vec2::ZERO, Vec2::Y] {
            let mut player = new_player();
            player.grounded = true;
            let mut transform = Transform::default();
            let mut car = car::Car::default();
            let mut keys = PlayerInput {
                jump: true,
                movement: run,
                view_dir: Vec3::NEG_Z,
                ..default()
            };
            let dt = 1.0 / 60.0;
            let mut shakes = Vec::new();
            for frame in 0..180 {
                move_jazz(
                    dt,
                    &colliders,
                    &level,
                    &mut transform,
                    &mut player,
                    &mut car,
                    (&keys, None),
                    Vec3::ZERO,
                );
                keys.jump = false;
                if let Some(shake) = player.shake.take() {
                    shakes.push((frame, shake, player.grounded));
                }
            }
            assert_eq!(shakes.len(), 2, "{run}: {shakes:?}");
            assert_eq!((shakes[0].0, shakes[0].1), (0, JUMP_SHAKE));
            let (frame, shake, grounded) = shakes[1];
            assert!(
                grounded && shake == LAND_02_SHAKE && frame > 30,
                "{shakes:?}"
            );
        }
        assert_eq!(landing_shake(1.0, 20.0), None);
        assert_eq!(landing_shake(3.0, 0.0), Some(LAND_SHAKE));
        assert_eq!(landing_shake(11.0, 15.0), Some(LAND_SHAKE));
        assert_eq!(landing_shake(13.0, 15.0), Some(LAND_HARD_SHAKE));
        assert_eq!(landing_shake(50.0, 0.0), Some(LAND_HARD_SHAKE));
    }

    /// A blast's shake plays for its time and no longer, within its amplitudes,
    /// strongest at its start (it eases out: smoothstep of the time left).
    #[test]
    fn screen_shake_plays_out() {
        let shake = riot::ScreenShake {
            radius: 150.0,
            secs: 0.5,
            rot_amp: Vec3::new(150.0, 100.0, 75.0),
            rot_freq: Vec3::new(100.0, 75.0, 55.0),
            loc_amp: Vec3::new(5.0, 0.0, 2.5),
            loc_freq: Vec3::new(10.0, 0.0, 5.0),
            fov_amp: -1.0,
            fov_freq: 0.1,
        };
        let mut seed = 7;
        let mut playing = PlayingShake::new(shake, &mut seed);
        let dt = 1.0 / 60.0;
        let (mut frames, mut first, mut last) = (0, 0.0f32, 0.0f32);
        while let Some((rot, loc, fov)) = playing.step(dt) {
            let deg = shake.rot_amp * (360.0 / 65536.0);
            assert!(rot.abs().cmple(deg + 1e-4).all(), "{rot:?}");
            assert!(
                loc.abs().cmple(shake.loc_amp * 0.01 + 1e-5).all(),
                "{loc:?}"
            );
            assert!(fov.abs() <= 1.0 + 1e-5);
            assert_eq!(loc.y, 0.0);
            let size = rot.length() / deg.length();
            if frames < 3 {
                first = first.max(size);
            }
            last = size;
            frames += 1;
        }
        assert!((29..=30).contains(&frames), "{frames}");
        assert!(last < 0.05 && last <= first, "{first} {last}");
    }

    /// Every gun, the vehicles' too, has its small icon for ammo picked up, and the
    /// HUD's images have it.
    #[test]
    fn ammo_icons_are_the_huds() {
        let hud = std::fs::read_to_string(assets_dir().join("hud/hud.txt")).unwrap();
        for slot in 0..GUN_SLOTS {
            let icon = ammo_icon(slot);
            assert!(icon.starts_with("GameMessageIcon"), "slot {slot}");
            let line = format!("image {icon} img/{icon}.png ");
            assert!(hud.contains(&line), "{icon} isn't in hud.txt");
        }
    }
}
