//! The multiplayer Customize screen, from the main menu's "Customize".
//!
//! Fall of Cybertron's character customization is its Scaleform movie
//! UI_GFxCustomCharacter_p.CustomCharacter_GFX played over the garage level
//! UI_CustomCharacter_m, where four preview robots stand on platforms. This module
//! recreates both:
//!
//! * **The garage** (models/garage.glb, levels/garage_*.txt, built by
//!   tools/build_garage.py): its own camera and render layer ([`GARAGE_LAYER`]), its
//!   light maps and lights, the blue light beams and the glow over the focused class,
//!   the level's ambience, and the camera views of the game's
//!   CAM_UI_Strategies_p.Customization_STRATEGY (the line-up, each class, and close-ups
//!   of their parts).
//! * **Four preview robots**: the Infiltrator's characters stand in for all four
//!   classes (Infiltrator, Destroyer, Titan, Scientist) in the game's customization
//!   pose (Cust_Idle, models/robot_cust.glb), each wearing the build shown (its
//!   parts and paint, in the faction's colours: character.rs), and can change form
//!   into the vehicle its chest makes.
//! * **The class select**: the four class tags under the robots with their three
//!   build slots each, flying in as the movie's menu_SelectCharacter does.
//! * **The editor** (menu_ModifyCharacter): three pages in a turned 3D stack
//!   (Loadouts, Armor, Personality), their option lists and colour grid, the info
//!   panels, the button hints, Randomize, Change Form and the camera lock.
//!
//! The movie's 3D clips are drawn by flash.rs; customize_ui.rs draws this screen's
//! from the state kept here. The text and the game data the lists show come from
//! assets/customize/customize.txt (tools/build_customize.py), built into the program
//! ([`DATA`]); the builds are saved in builds.txt next to the assets folder.

use crate::flash::{Ease, Tween};
use crate::lighting::{self, MapLighting};
use crate::menu::MenuText;
use crate::sound::{Loudness, Sfx, SoundBank, Volumes};
use bevy::animation::graph::AnimationNodeIndex;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::{Tonemapping, TonemappingLuts};
use bevy::gltf::{GltfExtras, GltfMaterialExtras};
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::light::{AmbientLight, NotShadowCaster};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::render::view::Hdr;
use bevy::shader::ShaderRef;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Render layer of the garage, its robots and its camera.
pub const GARAGE_LAYER: usize = 2;
/// customize.txt (tools/build_customize.py writes it), built into the program as it
/// is compiled: what the lists offer, what each option needs (level, Prime Mode) and
/// costs, and the screen's text. The copy in the assets folder isn't read, so that
/// changing it can't open options early; a change to it (the tool run again) takes
/// with the next build (cargo builds again when it changes).
pub(crate) const DATA: &str = include_str!("../assets/customize/customize.txt");
const POINTS_FILE: &str = "levels/garage_points.txt";
pub(crate) const LIGHTS_FILE: &str = "levels/garage_lights.txt";
const GARAGE_MODEL: &str = "models/garage.glb";
const LIGHTMAP_DIR: &str = "levels/garage_lightmaps";
/// After the garage and the robots have loaded and the robots are dressed, a moment
/// more behind the black (their meshes are put on the graphics card and their
/// pipelines asked for), then until no pipeline is still being made for
/// READY_FRAMES frames running (PipelinesPending); and the longest it waits.
const SETTLE_SECS: f32 = 0.4;
const READY_FRAMES: u32 = 3;
const LOAD_TIMEOUT_SECS: f32 = 12.0;
const ROBOT_MODEL: &str = "models/robot.glb";
const VEHICLE_MODEL: &str = "models/vehicle.glb";
/// Jazz's customization pose (Cust_Idle) and his idle holding a gun (the Scout's
/// Nav_Idle_Shoot) for robot.glb's skeleton (tools/build_customize.py).
const POSE_MODEL: &str = "models/robot_cust.glb";
/// Given the weapon being looked at, the game's preview pawn is put in the
/// shoot-from-the-hip stance (TnCharacterScriptBinding.GiveWeaponNextFrame:
/// STANCE_GunArmShootFromHip), and the robot anim tree's idle (IdleStance, a
/// TnStanceAnimNodeSequence) turns from Cust_Idle to that stance's Nav_Idle_Shoot,
/// blending over its BlendTime; taking the gun away turns it back.
const WEAPON_POSE_BLEND_SECS: f32 = 0.25;
/// As main.rs draws Jazz: his robot meshes at 0.6 of their size (another class's at
/// its own, character.rs's Robot, dress_previews), the car (built 2.2 times its
/// size) at its own.
const ROBOT_SCALE: f32 = 0.6;
/// The middle of a preview robot, above his feet (metres): where the garage's
/// lights are matched to the game's (open_customize).
const ROBOT_MIDDLE: f32 = 1.3;
/// The lamps over the platforms, made to shine down (open_customize): their cone's
/// inner and outer half-angles (degrees), all of the robot and his platform in it.
const LAMP_CONE: (f32, f32) = (70.0, 85.0);
const VEHICLE_SCALE: f32 = 1.0 / 2.2;
/// robot.glb's clips used here (main.rs's ANIM_TO_VEHICLE / ANIM_TO_ROBOT: Jazz's
/// own, without the characters' clips.glb) and the car models' (car.rs's
/// CAR_DRIVE / CAR_TO_VEHICLE / CAR_TO_ROBOT).
const ROBOT_TO_VEHICLE: usize = 8;
const ROBOT_TO_ROBOT: usize = 9;
const CAR_DRIVE: usize = 0;
/// A walker (Snarl) parks standing, a tank hovering: its hover clip (car.rs walks
/// it hovering; a tank only hovers).
const CAR_STAND: usize = 3;
const CAR_TO_VEHICLE: usize = 1;
const CAR_TO_ROBOT: usize = 2;
const CAR_GUN_UNEQUIP: usize = 12;
/// car_node's clip holding its guns folded away (CAR_GUN_UNEQUIP's).
const CAR_GUNS_AWAY: usize = 3;
/// A tank's and a jet's hover bob (ADD_Nav_Hover_VEH, car.rs's CAR_HOVER_BOB), added
/// on top of their parked hover as in play: a jet's rocking and its body's rise and
/// fall, Swoop's wings flapping.
const CAR_HOVER_BOB: usize = 13;
/// The transformation is timed as car.rs times it, in clip time (the clips play
/// at main.rs's TRANSFORM_SPEED, as when Jazz transforms in play), by the
/// vehicle's clips (character.rs's Timing): the car appears as its fold-up's
/// ToggleHidden says and the robot is gone at his, and unfolding, the robot
/// appears and the car goes at theirs, the car drawn among his pieces and the two
/// carried as in play (car.rs's transform_heights). The half seen as it starts
/// blends in from the pose it was in (main.rs's TRANSFORM_BLEND_IN: the robot's
/// fold-up from the customization pose, the car's unfolding from where it is
/// parked). Each end blends into the pose the preview
/// is left in, as the game blends out of its transformation: the car settles from
/// its hover onto its wheels over the last CAR_SETTLE_SECS of its fold-up, and the
/// robot comes down from his leap into the customization pose rather than into the
/// crouch his unfold ends in, over ROBOT_LAND_SECS before its last frames.
const CAR_SETTLE_SECS: f32 = 0.533;
const ROBOT_LAND: (f32, f32) = (0.217, 0.017);

/// The classes in the movie's order (buttonArray, left to right on screen).
pub(crate) const SPECIALTIES: [&str; 4] = ["Scout", "Leader", "Soldier", "Scientist"];
pub(crate) const SLOTS: usize = 3;
/// Each class's starting loadout, multiplayer's (TR_MPPlayerCharacterData_p's
/// <class>_PCD_MP: its WeaponTypes, its first Abilities and its RobotVoice), all
/// open from level 0: a new build's.
const STARTING_LOADOUTS: [([&str; 2], &str, &str); 4] = [
    (["Shotgun", "HeavyPistol"], "Cloaking", "ScoutVoice2"),
    (["Bazooka", "HomingRocket"], "Barrier", "LeaderVoice1"),
    (["HeavyMG", "DrainGun"], "Whirlwind", "SoldierVoice2"),
    (
        ["AssaultRifle", "TracerDart"],
        "HealBeam",
        "ScientistVoice1",
    ),
];
// (The classes Jazz can play as, Choose Character, choose.rs, and whose weapons the
// preview robot can hold, are those whose characters the demo has: CustData::playable.)

/// The fade from and to black around the screen.
const FADE_SECS: f32 = 0.5;
/// TnSeqAct_DeresCustomizationPawn: a robot vanishes 0.2 s into its de-res and
/// shows 0.3 s into its re-res.
const DERES_HIDE_AT: f32 = 0.2;
const RERES_SHOW_AT: f32 = 0.3;
/// The previews' sounds are played as their own (Sfx::play_owned), each class's
/// this plus its number: far from any entity's bits, which the game's Jazzes use.
const PREVIEW_SOUNDS: u64 = u64::MAX - 8;
/// The voice preview's lines' owners (preview_voice: a new one for each line).
const PREVIEW_VOICE: u64 = u64::MAX - 4096;
/// Camera rotation while unlocked (radians per second for keys and stick, per
/// pixel for the mouse).
const CAMERA_TURN_SPEED: f32 = 1.6;
const CAMERA_TURN_PER_PIXEL: f32 = 0.006;

pub(crate) const SND_SELECT_IN: &str = "BL_HUD_INTERFACE.CUST_CHARACTER_SELECT_PRESS_IN";
pub(crate) const SND_SELECT_OUT: &str = "BL_HUD_INTERFACE.CUST_CHARACTER_SELECT_PRESS_OUT";
pub(crate) const SND_SELECT_LEFT: &str = "BL_HUD_INTERFACE.CUST_CHARACTER_SELECT_SCROLL_LEFT";
pub(crate) const SND_SELECT_RIGHT: &str = "BL_HUD_INTERFACE.CUST_CHARACTER_SELECT_SCROLL_RIGHT";
pub(crate) const SND_MENU_UP: &str = "BL_HUD_INTERFACE.CUST_SUBMENU_01_SCROLL_UP";
pub(crate) const SND_MENU_DOWN: &str = "BL_HUD_INTERFACE.CUST_SUBMENU_01_SCROLL_DOWN";
pub(crate) const SND_MENU_IN: &str = "BL_HUD_INTERFACE.CUST_SUBMENU_01_PRESS_IN";
pub(crate) const SND_MENU_OUT: &str = "BL_HUD_INTERFACE.CUST_SUBMENU_01_PRESS_OUT";
pub(crate) const SND_LIST_UP: &str = "BL_HUD_INTERFACE.CUST_SUBMENU_02_SCROLL_UP";
pub(crate) const SND_LIST_DOWN: &str = "BL_HUD_INTERFACE.CUST_SUBMENU_02_SCROLL_DOWN";
pub(crate) const SND_LIST_IN: &str = "BL_HUD_INTERFACE.CUST_SUBMENU_02_PRESS_IN";
pub(crate) const SND_LIST_OUT: &str = "BL_HUD_INTERFACE.CUST_SUBMENU_02_PRESS_OUT";
pub(crate) const SND_TAB: &str = "BL_HUD_INTERFACE.CUST_TAB_CHANGE";
pub(crate) const SND_LOCKED: &str = "BL_HUD_INTERFACE.BUTTON_LOCKED_ITEM";
/// The shop's: something bought, and too few shards to buy it.
const SND_MONEY_SUCCESS: &str = "BL_HUD_INTERFACE.UI_MONEY_DEPOSIT_SUCESS";
const SND_MONEY_FAIL: &str = "BL_HUD_INTERFACE.UI_MONEY_DEPOSIT_FAIL";
pub(crate) const SND_SLIDER: &str = "BL_HUD_INTERFACE.BUTTON_SLIDER_INC_DECREASE";
pub(crate) const SND_SLIDER_STEP: &str = "BL_HUD_INTERFACE.BUTTON_CALIBRATE";
pub(crate) const SND_BUTTON_OVER: &str = "BL_HUD_INTERFACE.BUTTON_UP";
pub(crate) const SND_BUTTON_CLICK: &str = "BL_HUD_INTERFACE.BUTTON_ACCEPT";
const SND_DERES: &str = "BL_DEATH.DERES_DISSOLVE_REMAINS";
/// The garage's ambience (its Kismet's Audio sequence): loops (their looping
/// layers), and one-shots now and then (cue, shortest and longest wait in
/// seconds). The computer screens' beeps (their cues' other layers) come round
/// with their cues' loops.
const AMBIENCE_LOOPS: [&str; 3] = [
    "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_AMB_BED",
    "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_EMIT_COMP_SCREEN_01",
    "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_EMIT_COMP_SCREEN_02",
];
const AMBIENCE_SHOTS: [(&str, f32, f32); 6] = [
    ("BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_PP_BEEPS", 3.0, 7.0),
    (
        "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_PP_MTL_WHISPER",
        3.0,
        7.0,
    ),
    (
        "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_PP_MACH_SERVO",
        5.0,
        10.0,
    ),
    ("BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_BYS_SYNTH", 3.0, 5.0),
    (
        "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_EMIT_COMP_SCREEN_01",
        1.225,
        1.225,
    ),
    (
        "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_EMIT_COMP_SCREEN_02",
        1.546,
        1.546,
    ),
];

// ---------------------------------------------------------------------------
// The game's data (customize.txt and garage_points.txt)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub(crate) struct ClassInfo {
    pub robot_speed: f32,
    pub vehicle_speed: f32,
    pub health: f32,
    /// The class's extra part: Wheels, Wings or Cannon.
    pub aux: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Swatch {
    pub id: String,
    /// The ini's values (0-255, linear).
    pub primary: [f32; 3],
    pub secondary: [f32; 3],
}

impl Swatch {
    /// How the movie shows a colour: 255 (v / 255)^0.4545 (sRGB 0-255).
    pub fn shown(c: [f32; 3]) -> [f32; 3] {
        c.map(|v| 255.0 * (v / 255.0).clamp(0.0, 1.0).powf(0.454_545))
    }
}

/// A weapon, weapon upgrade, ability or ability upgrade (skill).
#[derive(Clone, Debug)]
pub(crate) struct Item {
    pub id: String,
    /// Weapons and abilities: the classes that can have it; upgrades: the weapon
    /// or ability it goes with.
    pub owner: String,
    /// Weapons: 1 (primary-class) or 2 (heavy).
    pub kind: u8,
    pub level: u32,
    /// Weapons: damage, range, accuracy and rate of fire (0-1).
    pub ratings: [f32; 4],
    pub name: String,
    pub description: String,
}

/// A camera view of the customization camera behaviour.
#[derive(Clone, Copy, Debug)]
pub(crate) enum View {
    /// Position and forward direction.
    Absolute(Vec3, Vec3),
    /// The orbit point and the camera, from a preview robot's feet, in its own
    /// axes (forward, up, right).
    Orbit(Vec3, Vec3),
}

#[derive(Clone, Debug)]
pub(crate) struct Spot {
    pub at: Vec3,
    /// The way the robot faces (Bevy x, z).
    pub facing: Vec2,
}

/// A robot part (TnDataProvider_Part).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Part {
    pub id: String,
    /// Its part type (PART_TYPES', or Wings or Cannon for the aux slot).
    pub kind: String,
    pub name: String,
    /// The specialties it is for, comma-separated.
    pub classes: String,
    /// Its price in shards (RequiresCurrency; a DLC part's, its type's price as
    /// the game's own sets have it); 0 for a free part.
    pub price: u32,
    /// What else it needs (the bonus heads: a level or Prime Mode).
    pub needs: crate::xp::Needs,
}

impl Part {
    /// Its lock for a class as things stand (GetOptionLockStatus), None if it can
    /// be worn.
    pub fn lock(&self, progress: &crate::xp::Progress, class: usize) -> Option<crate::xp::Lock> {
        progress.part_lock(&self.id, self.price, self.needs, class)
    }
}

#[derive(Resource)]
pub(crate) struct CustData {
    pub text: MenuText,
    pub classes: [ClassInfo; 4],
    /// Every class's parts.
    pub parts: Vec<Part>,
    /// Armor sets (id, name, part ids, specialty), each class's first set first.
    pub armor_sets: Vec<(String, String, Vec<String>, String)>,
    /// The class whose parts each class offers: its own where the demo has its
    /// characters (character.rs), else the Infiltrator's, whose stand in for them.
    pub offered: [usize; 4],
    pub swatches: [Vec<Swatch>; 2],
    /// (class, id, name).
    pub voices: Vec<(String, String, String)>,
    pub weapons: Vec<Item>,
    pub weapon_upgrades: Vec<Item>,
    pub abilities: Vec<Item>,
    pub skills: Vec<Item>,
    pub spots: [Spot; 4],
    pub glows: [Vec3; 4],
    pub views: HashMap<String, View>,
    /// Horizontal field of view (degrees) and blend time between views.
    pub fov: f32,
    pub lerp: f32,
}

impl CustData {
    fn load(assets: &Path) -> Option<CustData> {
        let data = DATA;
        let points = std::fs::read_to_string(assets.join(POINTS_FILE)).ok()?;
        let text = MenuText::parse(DATA);
        let blank = ClassInfo {
            robot_speed: 0.0,
            vehicle_speed: 0.0,
            health: 0.0,
            aux: "Wheels".to_string(),
        };
        let mut d = CustData {
            text,
            classes: [blank.clone(), blank.clone(), blank.clone(), blank],
            parts: Vec::new(),
            armor_sets: Vec::new(),
            offered: std::array::from_fn(|c| {
                if crate::character::class_files_present(assets, c) {
                    c
                } else {
                    crate::character::INFILTRATOR
                }
            }),
            swatches: [Vec::new(), Vec::new()],
            voices: Vec::new(),
            weapons: Vec::new(),
            weapon_upgrades: Vec::new(),
            abilities: Vec::new(),
            skills: Vec::new(),
            spots: std::array::from_fn(|_| Spot {
                at: Vec3::ZERO,
                facing: Vec2::X,
            }),
            glows: [Vec3::ZERO; 4],
            views: HashMap::new(),
            fov: 50.0,
            lerp: 0.6,
        };
        let num = |s: &str| s.trim().parse::<f32>().unwrap_or(0.0);
        for line in data.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let item = |owner: usize, kind: Option<usize>, level: usize, name: usize| Item {
                id: f[1].to_string(),
                owner: f[owner].to_string(),
                kind: kind.map(|k| num(f[k]) as u8).unwrap_or(0),
                level: num(f[level]) as u32,
                ratings: [0.0; 4],
                name: f[name].to_string(),
                description: f.get(name + 1).unwrap_or(&"").to_string(),
            };
            match (f[0], f.len()) {
                ("class", 6) => {
                    if let Some(i) = SPECIALTIES.iter().position(|s| *s == f[1]) {
                        d.classes[i] = ClassInfo {
                            robot_speed: num(f[2]),
                            vehicle_speed: num(f[3]),
                            health: num(f[4]),
                            aux: f[5].to_string(),
                        };
                    }
                }
                // (An older file's lines are the Infiltrator's, and free.)
                ("part", 4..=7) => d.parts.push(Part {
                    id: f[1].into(),
                    kind: f[2].into(),
                    name: f[3].into(),
                    classes: f.get(4).unwrap_or(&"Scout").to_string(),
                    price: f.get(5).map_or(0.0, |s| num(s)) as u32,
                    needs: crate::xp::Needs::parse(f.get(6).unwrap_or(&"-")),
                }),
                ("armorset", 4 | 5) => d.armor_sets.push((
                    f[1].into(),
                    f[2].into(),
                    f[3].split(',').map(str::to_string).collect(),
                    f.get(4).unwrap_or(&"Scout").to_string(),
                )),
                ("swatch", 9) => {
                    let faction = (num(f[1]) as usize).min(1);
                    d.swatches[faction].push(Swatch {
                        id: f[2].to_string(),
                        primary: [num(f[3]), num(f[4]), num(f[5])],
                        secondary: [num(f[6]), num(f[7]), num(f[8])],
                    });
                }
                ("voice", 4) => d.voices.push((f[1].into(), f[2].into(), f[3].into())),
                ("weapon", n) if n >= 10 => {
                    let mut w = item(2, Some(3), 4, 9);
                    w.ratings = [num(f[5]), num(f[6]), num(f[7]), num(f[8])];
                    d.weapons.push(w);
                }
                ("wupgrade", n) if n >= 5 => d.weapon_upgrades.push(item(2, None, 3, 4)),
                ("ability", n) if n >= 5 => d.abilities.push(item(2, None, 3, 4)),
                ("skill", n) if n >= 5 => d.skills.push(item(2, None, 3, 4)),
                _ => {}
            }
        }
        let mut glows = 0;
        for line in points.lines() {
            let w: Vec<&str> = line.split_whitespace().collect();
            let v = |i: usize| w.get(i).map(|s| num(s)).unwrap_or(0.0);
            let class = |name: &str| SPECIALTIES.iter().position(|s| *s == name);
            match w.first().copied() {
                Some("preview") if w.len() >= 7 => {
                    if let Some(i) = class(w[1]) {
                        d.spots[i] = Spot {
                            at: Vec3::new(v(2), v(3), v(4)),
                            facing: Vec2::new(v(5), v(6)).normalize_or(Vec2::X),
                        };
                    }
                }
                Some("glow") if w.len() >= 5 => {
                    if let Some(i) = class(w[1]) {
                        d.glows[i] = Vec3::new(v(2), v(3), v(4));
                        glows += 1;
                    }
                }
                Some("camera") if w.len() >= 9 => {
                    let a = Vec3::new(v(3), v(4), v(5));
                    let b = Vec3::new(v(6), v(7), v(8));
                    let view = if w[2] == "absolute" {
                        View::Absolute(a, b.normalize_or(Vec3::NEG_X))
                    } else {
                        View::Orbit(a, b)
                    };
                    d.views.insert(w[1].to_string(), view);
                }
                Some("fov") if w.len() >= 4 => {
                    d.fov = v(1);
                    d.lerp = v(3);
                }
                _ => {}
            }
        }
        info!(
            "Customize: {} swatches, {} weapons, {} camera views, {} glows",
            d.swatches[0].len() + d.swatches[1].len(),
            d.weapons.len(),
            d.views.len(),
            glows
        );
        Some(d)
    }

    pub fn string(&self, key: &str, fallback: &str) -> String {
        self.text.get(key, fallback)
    }

    pub fn class_name(&self, class: usize) -> String {
        let fallback = ["Infiltrator", "Destroyer", "Titan", "Scientist"][class];
        self.string(
            &format!("CustomCharacter.Class{}", SPECIALTIES[class]),
            fallback,
        )
    }

    pub fn item<'a>(list: &'a [Item], id: &str) -> Option<&'a Item> {
        list.iter().find(|i| i.id == id)
    }

    /// A class's weapons: the primary-class ones, then the heavy ones, by level.
    pub fn weapons_for(&self, class: usize) -> Vec<&Item> {
        let spec = SPECIALTIES[class];
        let has = |w: &&Item| w.owner.split(',').any(|s| s == spec);
        let kind = |k: u8| {
            let mut v: Vec<&Item> = self
                .weapons
                .iter()
                .filter(|w| has(w) && w.kind == k)
                .collect();
            v.sort_by_key(|w| w.level);
            v
        };
        let mut list = kind(1);
        list.extend(kind(2));
        list
    }

    pub fn upgrades_for<'a>(list: &'a [Item], owner: &str) -> Vec<&'a Item> {
        let mut out: Vec<&Item> = list.iter().filter(|u| u.owner == owner).collect();
        out.sort_by_key(|u| u.level);
        out
    }

    pub fn abilities_for(&self, class: usize) -> Vec<&Item> {
        let spec = SPECIALTIES[class];
        let mut out: Vec<&Item> = self
            .abilities
            .iter()
            .filter(|a| a.owner.split(',').any(|s| s == spec))
            .collect();
        out.sort_by_key(|a| a.level);
        out
    }

    pub fn voices_for(&self, class: usize) -> Vec<&(String, String, String)> {
        self.voices
            .iter()
            .filter(|v| v.0 == SPECIALTIES[class])
            .collect()
    }

    /// Whether the demo has a class's own characters (it offers its parts and Jazz
    /// can play it).
    pub fn playable(&self, class: usize) -> bool {
        self.offered.get(class) == Some(&class)
    }

    /// The parts a class's robot part slot offers: its own of that type, or the
    /// Infiltrator's for a class without its characters (the aux slot takes the
    /// class's extra part whatever it is called: wheels, wings or a cannon).
    pub fn parts_for(&self, class: usize, index: usize) -> Vec<&Part> {
        let kind = PART_TYPES[index];
        let spec = SPECIALTIES[self.offered[class.min(3)]];
        self.parts
            .iter()
            .filter(|p| {
                (p.kind == kind
                    || (index == 5 && matches!(p.kind.as_str(), "Wheels" | "Wings" | "Cannon")))
                    && p.classes.split(',').any(|s| s == spec)
            })
            .collect()
    }

    /// The armor sets a class offers (its own, or the Infiltrator's for a class
    /// without its characters).
    pub fn armor_sets_for(&self, class: usize) -> Vec<&(String, String, Vec<String>, String)> {
        let spec = SPECIALTIES[self.offered[class.min(3)]];
        self.armor_sets.iter().filter(|s| s.3 == spec).collect()
    }

    /// A part by its id.
    pub fn part(&self, id: &str) -> Option<&Part> {
        self.parts.iter().find(|p| p.id == id)
    }

    /// An armor set's price for a class as things stand: its parts' that are
    /// still locked (the list's configArmorSetButton; 0 once all are worn).
    pub fn set_price(&self, parts: &[String], progress: &crate::xp::Progress, class: usize) -> u32 {
        parts
            .iter()
            .filter_map(|id| self.part(id))
            .filter(|p| p.lock(progress, class).is_some())
            .map(|p| p.price)
            .sum()
    }

    /// A random part of a build slot's type the class can wear as things stand
    /// (TnCustomCharacterOptions.GetRandomPart: none locked), or None if it has
    /// none.
    pub fn random_part(
        &self,
        class: usize,
        index: usize,
        progress: &crate::xp::Progress,
        rng: &mut Rng,
    ) -> Option<String> {
        let open: Vec<&Part> = self
            .parts_for(class, index)
            .into_iter()
            .filter(|p| p.lock(progress, class).is_none())
            .collect();
        (!open.is_empty()).then(|| open[rng.below(open.len())].id.clone())
    }
}

/// A small random number generator (xorshift32): the builds' random parts and
/// colours (the game's Random.RandomInt and FRand).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed.max(1))
    }

    /// Seeded from the clock.
    pub fn from_time() -> Rng {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32).rotate_left(16))
            .unwrap_or(0x2545_f491);
        Rng::new(nanos)
    }

    /// 0 up to (not including) 1.
    pub fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// 0 up to (not including) n.
    pub fn below(&mut self, n: usize) -> usize {
        ((self.next() * n as f32) as usize).min(n.saturating_sub(1))
    }
}

/// The part types of a build's seven parts (CharacterData's robotParts order).
pub(crate) const PART_TYPES: [&str; 7] = [
    "ArmLo", "ArmUp", "Chest", "Head", "Legs", "Wheels", "DecalSet",
];

// ---------------------------------------------------------------------------
// Builds
// ---------------------------------------------------------------------------

/// One character build (the game's CharacterData).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Build {
    pub name: String,
    pub parts: [String; 7],
    /// Autobot and Decepticon colour swatches.
    pub swatches: [String; 2],
    pub metallic: f32,
    pub voice: String,
    pub weapons: [String; 2],
    pub weapon_upgrades: [String; 2],
    pub ability: String,
    pub ability_upgrade: String,
}

impl Build {
    /// A new build (ResetCharacterFromName): the class's name and number, its
    /// starting weapons, ability and voice (GetDefaultCharacter: STARTING_LOADOUTS;
    /// where the data lacks one, the first it has, the weapons its lowest-level
    /// two), a random part of each type it can wear (GetRandomTransformerParts:
    /// none it would have to buy or unlock), random colours for both factions
    /// (GetRandomColorSwatch) and a random metallicness (FRand). The game has no
    /// armor set a new build starts in.
    pub(crate) fn new(
        data: &CustData,
        class: usize,
        slot: usize,
        progress: &crate::xp::Progress,
        rng: &mut Rng,
    ) -> Build {
        let (weapons, ability, voice) = STARTING_LOADOUTS[class.min(3)];
        let first = weapon_for(data, class, weapons[0], "", 0);
        let second = weapon_for(data, class, weapons[1], &first, 0);
        let voices = data.voices_for(class);
        let abilities = data.abilities_for(class);
        Build {
            name: format!("{} {}", data.class_name(class), slot + 1),
            parts: std::array::from_fn(|i| {
                data.random_part(class, i, progress, rng)
                    .unwrap_or_default()
            }),
            swatches: [0, 1].map(|f| {
                let n = data.swatches[f].len();
                if n > 0 {
                    data.swatches[f][rng.below(n)].id.clone()
                } else {
                    String::new()
                }
            }),
            metallic: rng.next(),
            voice: voices
                .iter()
                .find(|v| v.1 == voice)
                .or(voices.first())
                .map(|v| v.1.clone())
                .unwrap_or_default(),
            weapons: [first, second],
            weapon_upgrades: [String::new(), String::new()],
            ability: abilities
                .iter()
                .find(|a| a.id == ability)
                .or(abilities.first())
                .map(|a| a.id.clone())
                .unwrap_or_default(),
            ability_upgrade: String::new(),
        }
    }

    /// The build with nothing its class's `level` hasn't opened (one saved at a
    /// higher level, or before the demo had levels; the game's builds never hold
    /// such options): a locked weapon gives way to the class's starting one for
    /// its slot (or its lowest-level open one), taking its upgrade with it; a
    /// locked ability to the starting one; a locked upgrade or ability upgrade, or
    /// one for another weapon or ability, to none.
    pub(crate) fn sanitized(mut self, data: &CustData, class: usize, level: u32) -> Build {
        let open = |list: &[Item], id: &str| {
            CustData::item(list, id).is_none_or(|i| crate::xp::unlocked(i.level, level))
        };
        let (weapons, ability, _) = STARTING_LOADOUTS[class.min(3)];
        for slot in 0..2 {
            if !open(&data.weapons, &self.weapons[slot]) {
                let other = self.weapons[1 - slot].clone();
                self.weapons[slot] = weapon_for(data, class, weapons[slot], &other, level);
                self.weapon_upgrades[slot].clear();
            }
            let upgrade = &self.weapon_upgrades[slot];
            if !upgrade.is_empty()
                && CustData::item(&data.weapon_upgrades, upgrade).is_none_or(|u| {
                    u.owner != self.weapons[slot] || !crate::xp::unlocked(u.level, level)
                })
            {
                self.weapon_upgrades[slot].clear();
            }
        }
        if !open(&data.abilities, &self.ability) {
            let abilities = data.abilities_for(class);
            self.ability = abilities
                .iter()
                .find(|a| a.id == ability)
                .or(abilities
                    .iter()
                    .find(|a| crate::xp::unlocked(a.level, level)))
                .map(|a| a.id.clone())
                .unwrap_or_default();
            self.ability_upgrade.clear();
        }
        let upgrade = &self.ability_upgrade;
        if !upgrade.is_empty()
            && CustData::item(&data.skills, upgrade)
                .is_none_or(|u| u.owner != self.ability || !crate::xp::unlocked(u.level, level))
        {
            self.ability_upgrade.clear();
        }
        self
    }

    fn line(&self, class: usize, slot: usize) -> String {
        [
            "build".to_string(),
            SPECIALTIES[class].to_string(),
            (slot + 1).to_string(),
            self.name.replace('\t', " "),
            self.parts.join(","),
            self.swatches.join(","),
            format!("{:.2}", self.metallic),
            self.voice.clone(),
            self.weapons.join(","),
            self.weapon_upgrades.join(","),
            self.ability.clone(),
            self.ability_upgrade.clone(),
        ]
        .join("\t")
    }
}

/// A class's build's look (character.rs) in each faction's colours: its Autobot
/// swatch and glow, then its Decepticon ones (a class without its characters
/// wears the Infiltrator's).
pub(crate) fn build_looks(
    data: &CustData,
    chars: &crate::character::Characters,
    class: usize,
    b: &Build,
) -> [crate::character::Look; 2] {
    std::array::from_fn(|f| {
        let swatch = data.swatches[f]
            .iter()
            .find(|s| s.id == b.swatches[f])
            .or(data.swatches[f].first());
        let (primary, secondary) = swatch.map_or(([240.0, 240.0, 247.0], [255.0, 0.0, 0.0]), |s| {
            (s.primary, s.secondary)
        });
        crate::character::Look {
            decepticon: f == 1,
            ..chars.look(
                data.offered[class.min(3)],
                &b.parts,
                primary,
                secondary,
                b.metallic,
            )
        }
    })
}

/// A weapon for a build's slot: `want` if the class has it, open at `level`, and it
/// isn't `other` (the other slot's), else the class's lowest-level open weapon that
/// isn't.
fn weapon_for(data: &CustData, class: usize, want: &str, other: &str, level: u32) -> String {
    let list = data.weapons_for(class);
    let fits = |w: &&&Item| w.id != other && crate::xp::unlocked(w.level, level);
    list.iter()
        .filter(fits)
        .find(|w| w.id == want)
        .or_else(|| list.iter().filter(fits).min_by_key(|w| w.level))
        .map(|w| w.id.clone())
        .unwrap_or_default()
}

fn builds_path() -> PathBuf {
    let assets = crate::assets_dir();
    assets
        .parent()
        .map(|p| p.join("builds.txt"))
        .unwrap_or_else(|| PathBuf::from("builds.txt"))
}

/// Every class's three builds, from builds.txt, kept as the game keeps its
/// characters: a slot builds.txt has no build for gets a new one (FillCharacterSlots:
/// Build::new, random parts and colours); a part its class can't wear as things
/// stand (one not bought, or needing a level or Prime Mode, or not its class's)
/// gives way to a random one of its type it can wear (as
/// ResetPartsThatAreNoLongerAvailable does for the game's DLC); and nothing in them
/// their class's level hasn't opened (Build::sanitized). New builds and changed
/// parts are saved straight away (every screen then shows the same builds).
pub(crate) fn load_builds(data: &CustData, progress: &crate::xp::Progress) -> [[Build; SLOTS]; 4] {
    let mut rng = Rng::from_time();
    let mut saved: [[Option<Build>; SLOTS]; 4] = Default::default();
    let text = std::fs::read_to_string(builds_path()).unwrap_or_default();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 12 || f[0] != "build" {
            continue;
        }
        let (Some(class), Ok(slot)) = (
            SPECIALTIES.iter().position(|s| *s == f[1]),
            f[2].parse::<usize>(),
        ) else {
            continue;
        };
        if !(1..=SLOTS).contains(&slot) {
            continue;
        }
        let pair = |s: &str| {
            let mut it = s.split(',').map(str::to_string);
            [it.next().unwrap_or_default(), it.next().unwrap_or_default()]
        };
        let mut b = empty_build();
        b.name = f[3].chars().take(RENAME_MAX).collect();
        let parts: Vec<String> = f[4].split(',').map(str::to_string).collect();
        if parts.len() == 7 {
            b.parts = std::array::from_fn(|i| parts[i].clone());
        }
        b.swatches = pair(f[5]);
        b.metallic = f[6].parse::<f32>().unwrap_or(0.0).clamp(0.0, 1.0);
        b.voice = f[7].to_string();
        b.weapons = pair(f[8]);
        b.weapon_upgrades = pair(f[9]);
        b.ability = f[10].to_string();
        b.ability_upgrade = f[11].to_string();
        saved[class][slot - 1] = Some(b);
    }
    let mut changed = false;
    let mut builds: [[Build; SLOTS]; 4] = std::array::from_fn(|c| {
        std::array::from_fn(|s| match saved[c][s].take() {
            Some(b) => b,
            None => {
                changed = true;
                Build::new(data, c, s, progress, &mut rng)
            }
        })
    });
    for (class, slots) in builds.iter_mut().enumerate() {
        for b in slots.iter_mut() {
            changed |= wear_what_can_be_worn(b, data, class, progress, &mut rng);
        }
        let level = progress.level(class);
        for b in slots.iter_mut() {
            *b = std::mem::replace(b, empty_build()).sanitized(data, class, level);
        }
    }
    if changed {
        save_builds(&builds);
    }
    builds
}

/// Swaps each part of a build its class can't wear as things stand (not bought,
/// needing a level or Prime Mode, or not one its class offers in that slot) for a
/// random one of the slot's type it can (GetRandomPart); whether any changed.
pub(crate) fn wear_what_can_be_worn(
    b: &mut Build,
    data: &CustData,
    class: usize,
    progress: &crate::xp::Progress,
    rng: &mut Rng,
) -> bool {
    let mut changed = false;
    for i in 0..PART_TYPES.len() {
        let wearable = data
            .parts_for(class, i)
            .iter()
            .any(|p| p.id == b.parts[i] && p.lock(progress, class).is_none());
        if !wearable && let Some(id) = data.random_part(class, i, progress, rng) {
            b.parts[i] = id;
            changed = true;
        }
    }
    changed
}

/// Every build made anew, its name kept (Prime Mode's ResetCharacters:
/// ResetCharacterFromName for each), and saved: the class's starting loadout and
/// random parts and colours, as things stand (the parts bought still worn).
pub(crate) fn reset_builds(data: &CustData, progress: &crate::xp::Progress) {
    let old = load_builds(data, progress);
    let mut rng = Rng::from_time();
    let builds: [[Build; SLOTS]; 4] = std::array::from_fn(|c| {
        std::array::from_fn(|s| Build {
            name: old[c][s].name.clone(),
            ..Build::new(data, c, s, progress, &mut rng)
        })
    });
    save_builds(&builds);
}

/// Customize.WriteCustomizationFile.
fn save_builds(builds: &[[Build; SLOTS]; 4]) {
    let mut lines = vec!["# The Customize screen's character builds (class, slot, name, parts, \
                          colours, metallic, voice, weapons, weapon upgrades, ability, ability upgrade)"
        .to_string()];
    for (c, class) in builds.iter().enumerate() {
        for (s, b) in class.iter().enumerate() {
            lines.push(b.line(c, s));
        }
    }
    if let Err(e) = std::fs::write(builds_path(), lines.join("\n") + "\n") {
        warn!("Couldn't save the builds: {e}");
    }
}

/// Most characters a build's name can have (the rename prompt's MaxLength).
pub(crate) const RENAME_MAX: usize = 15;

// ---------------------------------------------------------------------------
// The screen's state
// ---------------------------------------------------------------------------

/// Between the menus (the main menu's CUSTOMIZE, the Escalation lobby's Customize)
/// and this screen.
#[derive(Resource, Default)]
pub struct CustomizeLink {
    /// The screen's data is there (tools/build_customize.py and build_garage.py).
    pub available: bool,
    /// Set by the menu once it has faded to black: open the screen.
    pub open: bool,
    /// The screen is up and drawing its own fade: the menu's black can go.
    pub showing: bool,
    /// Set here once the screen has closed and faded to black.
    pub closed: bool,
    /// Set by the menu: close the screen (opened from the Escalation lobby, for
    /// the lobby to show the match starting, or the game gone).
    pub close: bool,
}

/// What a button hint does (the PC hint bar's buttons).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Hint {
    ToggleFaction,
    Close,
    CameraLock,
    Randomize,
    ChangeForm,
    CyclePage,
    Back,
}

impl Hint {
    /// The controller button the hint is on, as the picture the bar shows beside
    /// its label while a controller plays (the movie's own button icons,
    /// UI_GFxButtonIcons_p's Gamepad_<id>): the buttons the movie gives its hints
    /// (setButtonHint's X, Y, LS and DRIGHT; its Back and Close on B, the menus'
    /// close), and Lock Camera, which the game leaves to the mouse, on the right
    /// stick's click (the stick that turns the camera).
    pub(crate) fn pad_button(self) -> &'static str {
        match self {
            Hint::ToggleFaction => "Gamepad_X",
            Hint::Close | Hint::Back => "Gamepad_B",
            Hint::CameraLock => "Gamepad_R3",
            Hint::Randomize => "Gamepad_Y",
            Hint::ChangeForm => "Gamepad_L3",
            Hint::CyclePage => "Gamepad_DRIGHT",
        }
    }
}

/// Something the mouse can point at (placed by customize_ui.rs as it draws).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Target {
    /// A class's build slot.
    Slot(usize, usize),
    /// A class tag.
    Class(usize),
    Hint(Hint),
    /// A row of the editor's front page.
    Row(usize),
    /// An option of the open list, or a colour of the open grid.
    Option(usize),
    Slider,
    PromptOk,
    PromptCancel,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Hit {
    pub target: Target,
    pub matrix: crate::flash::M3,
    pub rect: [f32; 4],
}

/// A class tag and its slots (sprite 124 and its classDisplay_mc).
#[derive(Clone, Debug)]
pub(crate) struct ClassUi {
    /// resizeButtons: the instant x shift and scale.
    pub x_offset: f32,
    pub scale: f32,
    /// classDisplay_mc's place: 0 collapsed (30, 78), 1 expanded (0, 0) (0.4 s).
    pub display: Tween,
    /// Its parts: 0 collapsed, 1 expanded (0.5 s).
    pub parts: Tween,
    /// The slots: 0 folded onto (30, 99), 1 out (0.25 s).
    pub slots: Tween,
    /// Flying to the editor's header (1.2 s) and back (0.4 s).
    pub fly: Tween,
    /// lockClassDisplay: in the editor, staying expanded with the build's name.
    pub locked: bool,
    pub hidden: bool,
}

impl ClassUi {
    fn new() -> ClassUi {
        ClassUi {
            x_offset: 0.0,
            scale: 1.0,
            display: Tween::at(0.0),
            parts: Tween::at(0.0),
            slots: Tween::at(0.0),
            fly: Tween::at(0.0),
            locked: false,
            hidden: false,
        }
    }
}

/// The class select (menu_SelectCharacter).
pub(crate) struct SelectUi {
    /// The fly-in (0.7 s).
    pub intro: Tween,
    pub focus: Option<usize>,
    pub slot: [usize; 4],
    pub classes: [ClassUi; 4],
    /// A slot held down (its "down" look), until released.
    pub pressed: Option<(usize, usize)>,
}

/// What an option list or the colour grid changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    Part(usize),
    ArmorSet,
    Swatch(usize),
    Voice,
    Weapon(usize),
    WeaponUpgrade(usize),
    Ability,
    AbilityUpgrade,
}

/// A row of an option list (3DListItemRenderer).
#[derive(Clone, Debug)]
pub(crate) struct OptionRow {
    pub id: String,
    pub label: String,
    /// Shown locked, with this text left of the row (e.g. "In Use").
    pub locked: Option<String>,
    /// Not bought yet: its price in shards, shown left of the row (price_mc);
    /// choosing it buys it (0: nothing to buy).
    pub price: u32,
}

/// An open option list (menu_ModifyCharacter_OptionSelect) or colour grid
/// (_SwatchSelect).
pub(crate) struct SubMenu {
    pub field: Field,
    pub swatches: bool,
    pub title: String,
    pub rows: Vec<OptionRow>,
    pub selected: usize,
    /// The first row (list) or grid row shown.
    pub scroll: usize,
    /// Its _z, from the page's -800 to -1600 (0.4 s).
    pub z: Tween,
    /// The build as it was when the list opened (cancel goes back to it).
    pub before: Build,
    /// Its camera close-up, if any.
    pub camera: Option<String>,
    /// Each row's pop-out toward the camera (0.2 s).
    pub pops: Vec<Tween>,
    /// A locked row's lock blinking since then.
    pub blink: Option<(usize, f32)>,
    /// When it opened (the rows' locks and prices slide in: lock_mc's and
    /// price_mc's ten frames).
    pub opened: f32,
}

/// One of the editor's info panels (mc_weaponInfo / mc_partInfo).
pub(crate) struct InfoPanel {
    /// The page it describes (0 Loadouts, 1 Armor).
    pub page: usize,
    pub born: f32,
    /// Fading out since then (removed 0.6 s later).
    pub leaving: Option<f32>,
    /// The Armor panel hides while a part list is open (0.4 s).
    pub alpha: Tween,
    /// The weapon panel: weapon details (1) or ability details (0) (0.4 s).
    pub weapon_mode: Tween,
    /// When its description last changed (fades in over 0.4 s).
    pub described_at: f32,
    pub described: String,
}

/// The rename prompt.
pub(crate) struct Rename {
    pub text: String,
    pub opened: f32,
}

/// The editor (menu_ModifyCharacter).
pub(crate) struct EditorUi {
    pub class: usize,
    pub slot: usize,
    /// The class's level (what its lists open).
    pub level: u32,
    pub build: Build,
    /// Its swing-in (1.2 s).
    pub intro: Tween,
    /// The pages (0 Loadouts, 1 Armor, 2 Personality) front to back.
    pub ring: [usize; 3],
    /// Each page's slot place: x, y, z (twips) and alpha (0.4 s).
    pub page_x: [Tween; 3],
    pub page_y: [Tween; 3],
    pub page_z: [Tween; 3],
    pub page_alpha: [Tween; 3],
    /// Pages that have had focus (their page menu exists).
    pub page_seen: [bool; 3],
    /// Each page's focused row.
    pub row: [usize; 3],
    /// Each page's rows' pop-out toward the camera.
    pub pops: [[Tween; 8]; 3],
    pub sub: Option<SubMenu>,
    pub info: Vec<InfoPanel>,
    /// When the front page's info panel is due.
    pub info_due: Option<f32>,
    pub camera_locked: bool,
    pub randomize_ready: f32,
    pub spinner: Option<f32>,
    pub rename: Option<Rename>,
    pub dragging_slider: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Phase {
    Closed,
    /// The garage spawned then, waiting for it to load (behind the menu's black).
    Loading(f32),
    Open,
    /// Fading out since then.
    Closing(f32),
}

/// A preview robot.
#[derive(Default)]
struct Preview {
    root: Option<Entity>,
    robot: Option<Entity>,
    car: Option<Entity>,
    robot_player: Option<Entity>,
    car_player: Option<Entity>,
    /// The look it wears (dress_previews) and the car model its car shows (index
    /// into PreviewAnimations::cars).
    look: Option<crate::character::Look>,
    car_set: usize,
    /// Its look's vehicle (character.rs's) and the transformation's timing.
    vehicle: Option<usize>,
    timing: crate::character::Timing,
    /// Shown (or hidden) as of then: the de-res / re-res.
    shown: bool,
    shown_at: f32,
    /// 0 robot, 1 car.
    form: u8,
    changing: Option<Change>,
    /// Change Form pressed during a change: the change back follows it.
    queued: bool,
    /// Straight back to the customization pose (a fold cancelled before the car
    /// showed).
    repose: bool,
    /// How much higher its car is drawn: a walker's to stand on its feet (Kit::lift),
    /// and a tank's float (its hover suspension at CustomizationSuspensionLength,
    /// the same as in play: Kit::handling's hover height; a jet's on its hover jets
    /// alike), which it rises to once
    /// it has come together and sinks from as it unfolds, sprung as car.rs springs
    /// it in play (raise and its speed).
    lift: f32,
    float: f32,
    raise: f32,
    raise_vel: f32,
    /// How much higher still its car is drawn as it changes form (car.rs's
    /// transform_heights).
    transform_lift: f32,
}

/// A preview robot changing form.
#[derive(Clone, Copy, Debug)]
struct Change {
    /// When it began.
    at: f32,
    to_car: bool,
    /// Its clips have been started.
    started: bool,
    /// Unfolding, how high its car was as it began (transform_heights).
    from: f32,
}

impl Change {
    fn new(at: f32, to_car: bool) -> Option<Change> {
        Some(Change {
            at,
            to_car,
            started: false,
            from: 0.0,
        })
    }
}

/// `clip` blending in on `player` over TRANSFORM_BLEND_IN from what it was
/// playing, `from` to `to` of the way in this frame: its weight rising evenly as
/// the others' fall (all but `keep`), which stop once it is in.
fn blend_in(
    player: &mut AnimationPlayer,
    clip: AnimationNodeIndex,
    from: f32,
    to: f32,
    keep: Option<AnimationNodeIndex>,
) {
    let mut gone = Vec::new();
    for (node, active) in player.playing_animations_mut() {
        if *node == clip {
            active.set_weight(to);
        } else if Some(*node) != keep {
            if to >= 1.0 {
                gone.push(*node);
            } else if from < 1.0 {
                let w = active.weight() * (1.0 - to) / (1.0 - from);
                active.set_weight(w);
            }
        }
    }
    for node in gone {
        player.stop(node);
    }
}

#[derive(Resource)]
pub(crate) struct Customize {
    pub phase: Phase,
    pub opened: f32,
    /// The black over everything.
    pub fade: Tween,
    pub select: SelectUi,
    pub editor: Option<EditorUi>,
    pub builds: [[Build; SLOTS]; 4],
    /// The classes' experience and levels, and the shards (xp.rs's, as the screen
    /// opened).
    pub progress: crate::xp::Progress,
    /// Toggle Faction: the previews in Autobot (0) or Decepticon (1) colours.
    pub faction: usize,
    pub hits: Vec<Hit>,
    pub hover: Option<Target>,
    pub hint_hover: HashMap<Hint, Tween>,
    /// The hint bar shows the controller's buttons beside its labels
    /// (`pad_hints_after`): set as the screen takes its first input (None till then).
    pub pad_hints: Option<bool>,
    /// The camera: its view, and the pose it is blending from.
    view: String,
    blend_from: Option<(Vec3, Vec3, f32)>,
    pose: (Vec3, Vec3),
    /// The unlocked camera's turn about its subject.
    yaw: f32,
    previews: [Preview; 4],
    garage: Vec<Entity>,
    ambience: Vec<Entity>,
    ambience_next: [f32; AMBIENCE_SHOTS.len()],
    rng: Rng,
    dragging_camera: bool,
    /// A voice the Select Voice list has just moved to: its line to play
    /// (preview_voice).
    voice_preview: Option<String>,
    /// The shop's notices (purchaseAlert_mc).
    pub notices: Notices,
    /// Too few shards to buy something: the currency sign's warning since then
    /// (showInsufficientFunds: currencySign_mc's "animate").
    pub funds_at: Option<f32>,
    /// Something was bought here: xp.rs's progress to update and save.
    progress_changed: bool,
    /// Opening: since when everything has loaded and the robots are dressed, and
    /// how many frames running no pipeline has been in the making.
    ready_since: Option<f32>,
    idle_frames: u32,
}

/// One of the shop's notices (mc_notifyChallenge): its title (Acquired), what was
/// bought ("Head : Rogue") and its price.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Notice {
    pub title: String,
    pub description: String,
    pub cost: u32,
    /// When it came up, and when it started fading out (fadeOut: 0.5 s).
    pub at: f32,
    pub fade_at: Option<f32>,
}

/// The shop's notices as purchaseAlert_mc keeps them (its ChallengeQueue): one up
/// at a time for 3 s (ChallengeDisplayTime), the next 0.2 s after it starts fading
/// (ChallengeDisplayInterval); the same kind of thing bought again within 3 s of
/// the last notice brings none (lastAlertMessage).
#[derive(Default)]
pub(crate) struct Notices {
    pub shown: Option<Notice>,
    pub fading: Vec<Notice>,
    queue: std::collections::VecDeque<Notice>,
    queued: bool,
    next: Option<f32>,
    last: Option<(String, f32)>,
}

/// purchaseAlert_mc's ChallengeDisplayTime, ChallengeDisplayInterval and the
/// notice's fadeOut.
pub(crate) const NOTICE_SECS: f32 = 3.0;
const NOTICE_GAP_SECS: f32 = 0.2;
pub(crate) const NOTICE_FADE_SECS: f32 = 0.5;

impl Notices {
    /// showAlert: `category` is the page button the list opened from ("Head").
    pub fn show(&mut self, title: &str, category: &str, description: &str, cost: u32, now: f32) {
        if self
            .last
            .as_ref()
            .is_some_and(|(c, at)| c == category && now - at < NOTICE_SECS)
        {
            return;
        }
        self.last = Some((category.to_string(), now));
        let description = if description.is_empty() {
            category.to_string()
        } else {
            format!("{category} : {description}")
        };
        self.queue.push_back(Notice {
            title: title.to_string(),
            description,
            cost,
            at: now,
            fade_at: None,
        });
        if !self.queued {
            self.step(now);
        }
    }

    /// UpdateChallenges: the next notice up (after the one up fades), or none.
    fn step(&mut self, now: f32) {
        self.next = None;
        if self.queue.is_empty() {
            self.queued = false;
            self.fade(now);
            return;
        }
        self.queued = true;
        if self.shown.is_some() {
            self.fade(now);
            self.next = Some(now + NOTICE_GAP_SECS);
            return;
        }
        if let Some(mut n) = self.queue.pop_front() {
            n.at = now;
            self.shown = Some(n);
            self.next = Some(now + NOTICE_SECS);
        }
    }

    fn fade(&mut self, now: f32) {
        if let Some(mut n) = self.shown.take() {
            n.fade_at = Some(now);
            self.fading.push(n);
        }
    }

    /// Each frame: the queue's timer, and the faded notices gone.
    pub fn update(&mut self, now: f32) {
        if self.next.is_some_and(|t| now >= t) {
            self.step(now);
        }
        self.fading
            .retain(|n| n.fade_at.is_some_and(|t| now - t < NOTICE_FADE_SECS));
    }
}

impl Customize {
    /// Whether a class's build slot is open at its level (refreshBuilds: the others
    /// show "Unlocks at Level n", disabled).
    pub(crate) fn slot_open(&self, class: usize, slot: usize) -> bool {
        slot < crate::xp::builds_open(self.progress.level(class))
    }

    /// The build a class's preview shows: the selected slot's, or, on a locked one,
    /// the last open slot's (updateSelectedBuildPreview leaves it be on a disabled
    /// slot, and the slots open in order).
    pub(crate) fn shown_slot(&self, class: usize) -> usize {
        let open = crate::xp::builds_open(self.progress.level(class));
        self.select.slot[class]
            .min(open.saturating_sub(1))
            .min(SLOTS - 1)
    }

    fn new() -> Customize {
        Customize {
            phase: Phase::Closed,
            opened: 0.0,
            fade: Tween::at(1.0),
            select: SelectUi {
                intro: Tween::at(0.0),
                focus: None,
                slot: [0; 4],
                classes: std::array::from_fn(|_| ClassUi::new()),
                pressed: None,
            },
            editor: None,
            builds: std::array::from_fn(|_| std::array::from_fn(|_| empty_build())),
            progress: Default::default(),
            faction: 0,
            hits: Vec::new(),
            hover: None,
            hint_hover: HashMap::new(),
            pad_hints: None,
            view: "Lineup".to_string(),
            blend_from: None,
            pose: (Vec3::new(19.5, 4.0, 0.0), Vec3::new(0.0, 2.3, 0.0)),
            yaw: 0.0,
            previews: Default::default(),
            garage: Vec::new(),
            ambience: Vec::new(),
            ambience_next: [0.0; AMBIENCE_SHOTS.len()],
            rng: Rng::from_time(),
            dragging_camera: false,
            voice_preview: None,
            notices: Notices::default(),
            funds_at: None,
            progress_changed: false,
            ready_since: None,
            idle_frames: 0,
        }
    }

    fn random(&mut self) -> f32 {
        self.rng.next()
    }

    /// The hints the bar shows now, in the bar's order.
    pub(crate) fn hints(&self) -> Vec<Hint> {
        let Some(ed) = &self.editor else {
            return vec![Hint::ToggleFaction, Hint::Close];
        };
        let mut out = vec![Hint::CameraLock];
        match &ed.sub {
            None => out.extend([Hint::Randomize, Hint::ChangeForm, Hint::CyclePage]),
            // A list with a close-up keeps Change Form only for the chest (the car).
            Some(sub) if sub.camera.is_none() || sub.field == Field::Part(2) => {
                out.push(Hint::ChangeForm)
            }
            Some(_) => {}
        }
        out.push(Hint::Back);
        out
    }

    /// The camera view `id` for class `class`'s preview.
    fn set_view(&mut self, id: &str, now: f32, cut: bool) {
        if self.view == id && !cut {
            return;
        }
        self.view = id.to_string();
        self.blend_from = if cut {
            None
        } else {
            Some((self.pose.0, self.pose.1, now))
        };
    }
}

fn empty_build() -> Build {
    Build {
        name: String::new(),
        parts: Default::default(),
        swatches: Default::default(),
        metallic: 0.0,
        voice: String::new(),
        weapons: Default::default(),
        weapon_upgrades: Default::default(),
        ability: String::new(),
        ability_upgrade: String::new(),
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct CustomizePlugin;

impl Plugin for CustomizePlugin {
    fn build(&self, app: &mut App) {
        let assets = crate::assets_dir();
        let data = CustData::load(&assets).filter(|_| assets.join(GARAGE_MODEL).exists());
        let available = data.is_some();
        match data {
            Some(data) => {
                app.insert_resource(data);
            }
            None => {
                info!("No customize data (tools/build_customize.py and build_garage.py make it)")
            }
        }
        // (The render world's count of the pipelines it is still making, for the
        // screen to wait for: PipelinesPending.)
        let pending = PipelinesPending::default();
        if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
            render_app
                .insert_resource(pending.clone())
                .add_systems(
                    bevy::render::Render,
                    count_pending_pipelines.in_set(bevy::render::RenderSystems::Cleanup),
                )
                .add_systems(
                    bevy::render::Render,
                    warm_blit_pipelines.in_set(bevy::render::RenderSystems::Queue),
                );
        }
        app.insert_resource(pending);
        app.add_plugins(MaterialPlugin::<GarageFx>::default())
            .add_plugins(MaterialPlugin::<GarageFog>::default())
            .add_plugins(bevy::app::HierarchyPropagatePlugin::<RenderLayers>::new(
                PostUpdate,
            ))
            .insert_resource(CustomizeLink {
                available,
                ..default()
            })
            .insert_resource(Customize::new())
            .add_systems(
                Startup,
                (
                    load_preview_animations,
                    preload_garage,
                    crate::customize_ui::load_images,
                ),
            )
            .add_systems(
                Update,
                (
                    open_customize,
                    garage_scene,
                    customize_input,
                    customize_update,
                    crate::customize_ui::draw_customize,
                )
                    .chain()
                    .before(crate::flash::FlashSystems)
                    .before(crate::menu::MenuSystems),
            )
            .add_systems(
                Update,
                (
                    attach_preview_animators,
                    hide_preview_guns,
                    garage_effects,
                    (dress_previews, preview_glows)
                        .chain()
                        .after(customize_update),
                    preview_voice.after(customize_input),
                    preview_tank_jets.after(customize_update),
                    follow_display_gamma,
                    keep_purchases
                        .after(customize_input)
                        .before(crate::customize_ui::draw_customize),
                ),
            )
            .add_systems(
                Update,
                (show_preview_gun, show_preview_upgrades)
                    .after(customize_update)
                    .after(hide_preview_guns),
            );
    }
}

/// What was bought here kept (TnCharacterScriptBinding.PurchaseOptionWithCurrency
/// publishes the key archive straight away): the shards and the parts bought into
/// xp.rs's progress, saved to progress.txt. And the shop's notices' timer.
fn keep_purchases(
    time: Res<Time>,
    mut cust: ResMut<Customize>,
    mut progress: ResMut<crate::xp::Progress>,
) {
    let now = time.elapsed_secs();
    cust.notices.update(now);
    if std::mem::take(&mut cust.progress_changed) {
        progress.shards = cust.progress.shards;
        progress.owned = cust.progress.owned.clone();
        progress.save();
    }
}

/// The garage and its light maps, loaded as the demo starts so the screen opens
/// without a wait (the handles keep them loaded).
#[derive(Resource)]
struct GaragePreload {
    scene: Handle<Scene>,
    pages: Vec<Handle<Image>>,
}

fn preload_garage(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    data: Option<Res<CustData>>,
) {
    if data.is_none() {
        return;
    }
    let scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset(GARAGE_MODEL));
    let dir = crate::assets_dir().join(LIGHTMAP_DIR);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".png"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let pages = names
        .iter()
        .map(|n| asset_server.load(format!("{LIGHTMAP_DIR}/{n}")))
        .collect();
    commands.insert_resource(GaragePreload { scene, pages });
}

/// How many render pipelines were still being made (compiled) as of the render
/// world's last frame. The Customize screen opens once its own are ready, or the
/// garage and the robots would show as they come: black shapes, then textured, a
/// piece at a time (each pipeline is made in turn, behind every other the game
/// has asked for).
#[derive(Resource, Clone, Default)]
pub struct PipelinesPending(std::sync::Arc<std::sync::atomic::AtomicU32>);

impl PipelinesPending {
    pub fn get(&self) -> u32 {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

fn count_pending_pipelines(
    cache: Option<Res<bevy::render::render_resource::PipelineCache>>,
    pending: Res<PipelinesPending>,
) {
    let n = cache.map_or(0, |c| c.waiting_pipelines().count());
    pending
        .0
        .store(n as u32, std::sync::atomic::Ordering::Relaxed);
}

/// Whether the opening screen shows now: everything loaded and the robots dressed
/// SETTLE_SECS ago (`ready_since`) and no pipeline in the making for READY_FRAMES
/// frames running (`idle_frames`), or LOAD_TIMEOUT_SECS after it was asked for
/// (`since`) whatever is left.
fn ready_to_show(ready_since: Option<f32>, idle_frames: u32, since: f32, now: f32) -> bool {
    let ready =
        ready_since.is_some_and(|at| now - at >= SETTLE_SECS && idle_frames >= READY_FRAMES);
    ready || now - since >= LOAD_TIMEOUT_SECS
}

/// Whether the screen waits for the pipelines (the test build can say not).
fn wait_for_pipelines() -> bool {
    true
}

/// The pipelines that copy a camera's picture to the window (or to an image), made
/// as the game starts. Bevy makes one the first time a camera needs it and holds
/// up the frame until it is made, behind every other pipeline still waiting: the
/// first time the Customize screen's cameras came on (its text's, drawing into an
/// image), while the game was still making its first few hundred, the game stood
/// still for seconds. Made first, they are ready by then: for the images' formats
/// at once, and the window's as soon as it is known.
fn warm_blit_pipelines(
    mut warmed: Local<Vec<bevy::render::render_resource::TextureFormat>>,
    windows: Res<bevy::render::view::ExtractedWindows>,
    cache: Res<bevy::render::render_resource::PipelineCache>,
    blit: Option<Res<bevy::core_pipeline::blit::BlitPipeline>>,
    mut pipelines: ResMut<
        bevy::render::render_resource::SpecializedRenderPipelines<
            bevy::core_pipeline::blit::BlitPipeline,
        >,
    >,
) {
    use bevy::render::render_resource::{BlendState, TextureFormat};
    let Some(blit) = blit else {
        return;
    };
    let windows = windows.windows.values().filter_map(|w| {
        w.swap_chain_texture_view_format
            .or(w.swap_chain_texture_format)
    });
    let formats = [TextureFormat::Bgra8UnormSrgb, TextureFormat::Rgba8UnormSrgb]
        .into_iter()
        .chain(windows);
    for format in formats {
        if warmed.contains(&format) {
            continue;
        }
        warmed.push(format);
        for blend_state in [None, Some(BlendState::ALPHA_BLENDING)] {
            let key = bevy::core_pipeline::blit::BlitPipelineKey {
                texture_format: format,
                blend_state,
                samples: 1,
            };
            pipelines.specialize(&cache, &blit, key);
        }
    }
}

/// Whether an asset is done loading (loaded, or failed: nothing more to wait for).
fn settled(asset_server: &AssetServer, id: impl Into<bevy::asset::UntypedAssetId>) -> bool {
    use bevy::asset::RecursiveDependencyLoadState as S;
    matches!(
        asset_server.recursive_dependency_load_state(id),
        S::Loaded | S::Failed(_)
    )
}

/// Marks a preview robot's models (and so everything in them).
#[derive(Component)]
pub struct PreviewModel {
    pub class: usize,
    pub car: bool,
}

/// Whether `entity` is part of a preview robot (main.rs leaves those alone).
pub fn in_preview(
    entity: Entity,
    parents: &Query<&ChildOf>,
    previews: &Query<(), With<PreviewModel>>,
) -> bool {
    previews.contains(entity) || parents.iter_ancestors(entity).any(|e| previews.contains(e))
}

#[derive(Component)]
pub(crate) struct GarageCamera;

#[derive(Component)]
struct GarageRoot;

/// A light effect of garage.glb: a beam, or the glow over a class.
#[derive(Component)]
struct GlowOf(usize);

/// The preview robots' animation graphs: (robot: pose, Jazz's to car and to robot,
/// the pose holding a gun, each class's own of that, each vehicle's, and each gun's
/// idle motion added on top), (each car: drive, to car, to robot, guns folded away).
#[derive(Resource)]
struct PreviewAnimations {
    robot: Handle<AnimationGraph>,
    robot_nodes: [AnimationNodeIndex; 4],
    /// Each class's pose holding a gun: its own (its robot_clips.glb's), or the
    /// Scout's (robot_nodes' last).
    gun_poses: [AnimationNodeIndex; 4],
    /// Each vehicle's halves of the transformation (its class's clips.glb,
    /// character.rs's order): to car, to robot.
    vehicle_nodes: Vec<[AnimationNodeIndex; 2]>,
    /// The guns a preview robot can hold (main.rs's weapons): id and idle clip.
    guns: Vec<(&'static str, AnimationNodeIndex)>,
    /// Each car model's graph: the characters' vehicles (character.rs's order), or
    /// vehicle.glb's alone.
    cars: Vec<PreviewCar>,
}

/// A preview car model to load: its file, its parked clip, and whether it bobs on
/// its hover (a tank or a jet).
type CarModel = (String, usize, bool);

/// A preview car model's graph, its clips (parked, to car, to robot, guns folded
/// away) and, for a tank or a jet, its hover bob.
type PreviewCar = (
    Handle<AnimationGraph>,
    [AnimationNodeIndex; 4],
    Option<AnimationNodeIndex>,
);

impl PreviewAnimations {
    /// The car's guns stay folded away whatever else it plays (the end of their
    /// unequip clip, as car.rs starts them).
    fn car_guns_away(&self, car: usize, player: &mut AnimationPlayer) {
        if let Some((_, nodes, _)) = self.cars.get(car) {
            player.play(nodes[CAR_GUNS_AWAY]).seek_to(1.0);
        }
    }

    /// A car model's clip (drive 0, to car 1, to robot 2, its guns folded away 3).
    fn car_node(&self, car: usize, clip: usize) -> Option<AnimationNodeIndex> {
        self.cars.get(car).map(|(_, nodes, _)| nodes[clip])
    }

    /// A tank's or a jet's hover bob.
    fn car_bob(&self, car: usize) -> Option<AnimationNodeIndex> {
        self.cars.get(car).and_then(|(_, _, bob)| *bob)
    }

    /// Parks car model `car`: its transformation's clips stopped, its guns folded
    /// away and its parked clip on (drive 0), with a tank's or a jet's hover bob on
    /// top as car.rs plays it for one hovering. Those carry on where they are if they
    /// were already blending in.
    fn park_car(&self, car: usize, player: &mut AnimationPlayer) {
        let Some((_, nodes, bob)) = self.cars.get(car) else {
            return;
        };
        player.stop(nodes[1]);
        player.stop(nodes[2]);
        player.play(nodes[CAR_GUNS_AWAY]).seek_to(1.0);
        player.play(nodes[0]).repeat().set_weight(1.0);
        if let Some(bob) = bob {
            player.play(*bob).repeat().set_weight(1.0);
        }
    }

    /// The robot's half of a vehicle's transformation (Jazz's without one).
    fn robot_transform(&self, vehicle: Option<usize>, to_car: bool) -> AnimationNodeIndex {
        let k = usize::from(!to_car);
        match vehicle.and_then(|v| self.vehicle_nodes.get(v)) {
            Some(nodes) => nodes[k],
            None => self.robot_nodes[1 + k],
        }
    }
}

fn load_preview_animations(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut clips: ResMut<crate::GltfClips>,
    chars: Res<crate::character::Characters>,
) {
    let assets = crate::assets_dir();
    if !assets.join(POSE_MODEL).exists() {
        return;
    }
    // (Each file's clips come in with the file: see GltfClips in main.rs.)
    let (mut pose_clips, mut robot_clips) = (Vec::new(), Vec::new());
    // The pose and the transformation are blended; a gun's idle motion (an
    // additive clip, as main.rs plays it) goes on top.
    let mut robot = AnimationGraph::new();
    let layers = robot.add_additive_blend(1.0, robot.root);
    let base = robot.add_blend(1.0, layers);
    let robot_nodes = [
        crate::gltf_clip(&mut robot, &mut pose_clips, 0, base),
        crate::gltf_clip(&mut robot, &mut robot_clips, ROBOT_TO_VEHICLE, base),
        crate::gltf_clip(&mut robot, &mut robot_clips, ROBOT_TO_ROBOT, base),
        crate::gltf_clip(&mut robot, &mut pose_clips, 1, base),
    ];
    let guns = crate::preview_guns()
        .into_iter()
        .map(|(id, _, idle)| {
            (
                id,
                crate::gltf_clip(&mut robot, &mut robot_clips, idle, layers),
            )
        })
        .collect();
    // Each class's files' clips: its own robot clips (its pose holding a gun) and
    // its characters' (the transformations: clips.glb).
    let classes = crate::character::CLASSES.len();
    let mut own_clips = vec![Vec::new(); classes];
    let mut extra_clips = vec![Vec::new(); classes];
    let gun_poses = std::array::from_fn(|class| {
        match chars
            .class(class)
            .cust_weapon_clip
            .filter(|_| chars.playable(class))
        {
            Some(clip) => crate::gltf_clip(&mut robot, &mut own_clips[class], clip, base),
            None => robot_nodes[3],
        }
    });
    // The characters' transformations, and their cars.
    let (vehicle_nodes, models): (Vec<[AnimationNodeIndex; 2]>, Vec<CarModel>) = if chars.available
    {
        chars
            .vehicles
            .iter()
            .map(|v| {
                let own = chars.class(v.class);
                let nodes = [v.to_vehicle_clip, v.to_robot_clip].map(|slot| {
                    let node = robot.add_clip(Handle::default(), 1.0, base);
                    if let Some(clip) = own.clip_in_slot(slot) {
                        extra_clips[v.class].push((node, clip));
                    }
                    node
                });
                // (A tank and a jet in their hover poses, floating on their jets,
                // and bobbing there.)
                let parked = if v.legs || v.tank.is_some() || v.plane.is_some() {
                    CAR_STAND
                } else {
                    CAR_DRIVE
                };
                let bobs = v.tank.is_some() || v.plane.is_some();
                (nodes, (v.model.clone(), parked, bobs))
            })
            .unzip()
    } else {
        (
            Vec::new(),
            vec![(VEHICLE_MODEL.to_string(), CAR_DRIVE, false)],
        )
    };
    let cars = models
        .iter()
        .map(|(model, parked, bobs)| {
            // (As car.rs builds them: the clips blended, the hover bob added on top.)
            let mut car = AnimationGraph::new();
            let layers = car.add_additive_blend(1.0, car.root);
            let base = car.add_blend(1.0, layers);
            let mut car_clips = Vec::new();
            // (Its parked clip in the drive one's place.)
            let nodes = [*parked, CAR_TO_VEHICLE, CAR_TO_ROBOT, CAR_GUN_UNEQUIP]
                .map(|i| crate::gltf_clip(&mut car, &mut car_clips, i, base));
            let bob =
                bobs.then(|| crate::gltf_clip(&mut car, &mut car_clips, CAR_HOVER_BOB, layers));
            let car = graphs.add(car);
            clips.wait(asset_server.load(model.clone()), car.clone(), car_clips);
            (car, nodes, bob)
        })
        .collect();
    let robot = graphs.add(robot);
    clips.wait(asset_server.load(POSE_MODEL), robot.clone(), pose_clips);
    clips.wait(asset_server.load(ROBOT_MODEL), robot.clone(), robot_clips);
    for (class, (own, extra)) in own_clips.into_iter().zip(extra_clips).enumerate() {
        for (file, pending) in [
            (crate::character::robot_clips_model(class), own),
            (crate::character::clips_model(class), extra),
        ] {
            if !pending.is_empty() {
                clips.wait(asset_server.load(file), robot.clone(), pending);
            }
        }
    }
    commands.insert_resource(PreviewAnimations {
        robot,
        robot_nodes,
        gun_poses,
        vehicle_nodes,
        guns,
        cars,
    });
}

// ---------------------------------------------------------------------------
// The garage's light effects
// ---------------------------------------------------------------------------

/// The light beams and glow spheres: added to what is behind them, fading towards
/// their outlines (assets/shaders/garage_fx.wgsl).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GarageFx {
    #[uniform(0)]
    color: Vec4,
    #[uniform(1)]
    params: Vec4,
}

impl Material for GarageFx {
    fn fragment_shader() -> ShaderRef {
        "shaders/garage_fx.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// Both sides: the glow spheres are two-sided, the beams are seen from inside.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// The garage's height fog: its HeightFog layers (lighting.rs's HeightFogLayer, up
/// to four), drawn over the whole picture from the depth prepass
/// (assets/shaders/garage_fog.wgsl).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GarageFog {
    #[uniform(0)]
    layers: FogLayers,
}

#[derive(Clone, Copy, Debug, Default, ShaderType)]
struct FogLayers {
    /// rgb: a layer's light (the game's units); w: its density per metre.
    color: [Vec4; 4],
    /// x: its top (m); y: where it starts from the camera (m); z: 1 if it is there.
    shape: [Vec4; 4],
}

impl GarageFog {
    fn new(layers: &[lighting::HeightFogLayer]) -> GarageFog {
        let mut out = FogLayers::default();
        for (k, f) in layers.iter().take(4).enumerate() {
            let c = f.color.to_linear();
            out.color[k] = Vec4::new(
                c.red * f.brightness,
                c.green * f.brightness,
                c.blue * f.brightness,
                f.density,
            );
            out.shape[k] = Vec4::new(f.height, f.start, 1.0, 0.0);
        }
        GarageFog { layers: out }
    }
}

const FOG_SHADER: &str = "shaders/garage_fog.wgsl";

impl Material for GarageFog {
    fn vertex_shader() -> ShaderRef {
        FOG_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        FOG_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// Its triangle's corners only (in clip space), never culled.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![
            layout
                .0
                .get_layout(&[Mesh::ATTRIBUTE_POSITION.at_shader_location(0)])?,
        ];
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// A triangle covering the whole screen (clip space x and y).
fn screen_triangle() -> Mesh {
    Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[-1.0f32, -1.0, 0.0], [3.0, -1.0, 0.0], [-1.0, 3.0, 0.0]],
    )
    .with_inserted_indices(bevy::mesh::Indices::U32(vec![0, 1, 2]))
}

/// The level effects (levelfx.rs) are the garage's while the screen is up, from the
/// moment it is put in (behind the black) until it has gone.
fn garage_scene(cust: Res<Customize>, mut scene: ResMut<crate::levelfx::GarageScene>) {
    let up = !matches!(cust.phase, Phase::Closed);
    if scene.0 != up {
        scene.0 = up;
    }
}

/// Garage.glb's effect meshes as they come in: the glows hidden until their class
/// has focus. They are drawn with their game materials by levelfx.rs (tools/
/// build_garage.py marks them so); a garage built without those gets the demo's
/// stand-ins here: the beams (blubeams_MATINST: (0.257, 0.522, 0.745) at 0.125) and
/// the glows (ccGlow_MAT_INST: (0.201, 0.494, 0.932) at 0.337).
fn garage_effects(
    mut commands: Commands,
    nodes: Query<(Entity, &GltfExtras, Option<&Children>), Added<GltfExtras>>,
    meshes: Query<Option<&GltfMaterialExtras>, With<Mesh3d>>,
    mut materials: ResMut<Assets<GarageFx>>,
    mut cache: Local<Option<(Handle<GarageFx>, Handle<GarageFx>)>>,
) {
    for (entity, extras, children) in &nodes {
        let value = &extras.value;
        if !value.contains("\"effect\"") {
            continue;
        }
        let (beam, glow) = cache
            .get_or_insert_with(|| {
                (
                    materials.add(GarageFx {
                        color: Vec4::new(0.257, 0.522, 0.745, 0.0) * 0.125 * 2.0,
                        params: Vec4::new(1.5, 0.0, 0.0, 0.0),
                    }),
                    materials.add(GarageFx {
                        color: Vec4::new(0.201, 0.494, 0.932, 0.0) * 0.337 * 2.5,
                        params: Vec4::new(2.0, 0.0, 0.0, 0.0),
                    }),
                )
            })
            .clone();
        let is_glow = value.contains("\"glow\"");
        let material = if is_glow { glow } else { beam };
        let mut parts = vec![entity];
        if let Some(children) = children {
            parts.extend_from_slice(children);
        }
        for part in parts {
            match meshes.get(part) {
                // (levelfx.rs's.)
                Ok(Some(m)) if m.value.contains("fx:") => {}
                Ok(_) => {
                    commands
                        .entity(part)
                        .remove::<MeshMaterial3d<StandardMaterial>>()
                        .insert((MeshMaterial3d(material.clone()), NotShadowCaster));
                }
                Err(_) => {}
            }
        }
        if is_glow
            && let Some(class) = SPECIALTIES
                .iter()
                .position(|s| value.contains(&format!("\"{s}\"")))
        {
            commands
                .entity(entity)
                .insert((GlowOf(class), Visibility::Hidden));
        }
    }
}

// ---------------------------------------------------------------------------
// Opening and closing
// ---------------------------------------------------------------------------

/// The garage camera's tone mapping (Blender Filmic, which no other camera uses):
/// its table becomes the game's filmic curve at the display gamma, made again as
/// that changes. The game sends its picture to the screen through its filmic
/// curve (lighting::game_filmic) and its display gamma: 2.2 unless Settings >
/// Display > Brightness has moved it (menu.rs's DisplaySettings::gamma). Footage of
/// the game's garage matches the demo's at 2.2.
fn follow_display_gamma(
    luts: Option<Res<TonemappingLuts>>,
    display: Res<crate::menu::DisplaySettings>,
    mut images: ResMut<Assets<Image>>,
    mut made: Local<Option<f32>>,
) {
    let Some(luts) = luts else {
        return;
    };
    let gamma = display.gamma();
    if *made == Some(gamma) {
        return;
    }
    *made = Some(gamma);
    if images
        .insert(luts.blender_filmic.id(), lighting::filmic_lut(gamma))
        .is_err()
    {
        warn!("Couldn't install the garage's tone curve");
    }
}

/// Opens the screen when the menu asks: the garage, its camera, lights and robots
/// are spawned behind the menu's black; once they have loaded the screen fades in,
/// the robots re-res and the class select flies in.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn open_customize(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    data: Option<Res<CustData>>,
    preload: Option<Res<GaragePreload>>,
    mut link: ResMut<CustomizeLink>,
    mut cust: ResMut<Customize>,
    mut sfx: ResMut<Sfx>,
    bank: Option<Res<SoundBank>>,
    volumes: Res<Volumes>,
    progress: Res<crate::xp::Progress>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fogs: ResMut<Assets<GarageFog>>,
    (chars, pending, worn, mut garage_cameras): (
        Res<crate::character::Characters>,
        Res<PipelinesPending>,
        Query<(), With<crate::character::Worn>>,
        Query<&mut Camera, With<GarageCamera>>,
    ),
) {
    let now = time.elapsed_secs();
    let cust = &mut *cust;
    // The menu wants the screen gone (the lobby's: CustomizeLink::close): still
    // loading, at once; open, as Back closes it.
    if std::mem::take(&mut link.close) {
        match cust.phase {
            Phase::Loading(_) => {
                close_now(&mut commands, cust, &mut link);
                return;
            }
            Phase::Open => close_screen(cust, now, &mut sfx),
            Phase::Closed | Phase::Closing(_) => {}
        }
    }
    if let Phase::Loading(since) = cust.phase
        && let Some(data) = data.as_deref()
    {
        // The garage, its light maps and the characters' models (with their
        // textures) loaded, and the robots dressed.
        let loaded = preload.as_deref().is_none_or(|p| {
            settled(&asset_server, &p.scene) && p.pages.iter().all(|h| settled(&asset_server, h))
        }) && chars.models().all(|id| settled(&asset_server, id));
        let dressed = !chars.available
            || cust
                .previews
                .iter()
                .all(|p| p.robot.is_none_or(|r| worn.contains(r)));
        if loaded && dressed {
            cust.ready_since.get_or_insert(now);
        } else {
            cust.ready_since = None;
        }
        // Then everything it draws ready to be drawn: no pipeline still being made.
        if pending.get() == 0 || !wait_for_pipelines() {
            cust.idle_frames += 1;
        } else {
            cust.idle_frames = 0;
        }
        if ready_to_show(cust.ready_since, cust.idle_frames, since, now) {
            let sound = bank.as_deref().map(|bank| (bank, &*volumes));
            start_screen(&mut commands, cust, data, sound, &mut sfx, now);
            link.showing = true;
            // Now it shows (drawn all along, unseen), and the robots go for their
            // re-res.
            for mut camera in &mut garage_cameras {
                camera.output_mode = bevy::camera::CameraOutputMode::default();
            }
            for p in &cust.previews {
                for e in [p.root, p.car].into_iter().flatten() {
                    commands.entity(e).insert(Visibility::Hidden);
                }
            }
        }
        return;
    }
    if !link.open {
        return;
    }
    link.open = false;
    let Some(data) = data else {
        link.closed = true;
        return;
    };
    let assets = crate::assets_dir();
    // (The classes' levels as they stand: none is earned here.)
    cust.progress = progress.clone();
    cust.builds = load_builds(&data, &cust.progress);
    cust.phase = Phase::Loading(now);
    cust.ready_since = None;
    cust.idle_frames = 0;
    // Black until it starts (the UI is drawn under it meanwhile, so its images,
    // text and shaders are ready when it fades in).
    cust.fade = Tween::at(1.0);
    cust.editor = None;
    cust.notices = Notices::default();
    cust.funds_at = None;
    cust.faction = 0;
    cust.yaw = 0.0;
    cust.hits.clear();
    cust.hover = None;
    cust.dragging_camera = false;
    cust.select = SelectUi {
        intro: Tween::at(0.0),
        focus: None,
        slot: [0; 4],
        classes: std::array::from_fn(|_| ClassUi::new()),
        pressed: None,
    };
    cust.set_view("SceneIntro", now, true);
    cust.pose = view_pose(&data, &cust.view, None, 0.0).unwrap_or(cust.pose);

    // The garage.
    let layer = RenderLayers::layer(GARAGE_LAYER);
    let mut spawned = Vec::new();
    spawned.push(
        commands
            .spawn((
                GarageRoot,
                SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(GARAGE_MODEL))),
                Transform::default(),
                Visibility::default(),
                bevy::app::Propagate(layer.clone()),
            ))
            .id(),
    );
    let lights = MapLighting::load_file(&assets, LIGHTS_FILE);
    let mut camera = commands.spawn((
        GarageCamera,
        Camera3d::default(),
        Camera {
            // Over the game's world (built behind the menu), under the menus.
            order: 5,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            // Drawn but not shown while it loads (the menu's black stays in
            // front), so that everything in it is ready when it does show:
            // open_customize.
            output_mode: bevy::camera::CameraOutputMode::Skip,
            ..default()
        },
        Projection::from(PerspectiveProjection {
            fov: 0.5,
            near: 0.1,
            far: 2000.0,
            ..default()
        }),
        // The game's tone curve (follow_display_gamma: the level has no colour
        // grade of its own).
        Tonemapping::BlenderFilmic,
        Hdr,
        // A depth prepass, so its soft effects (levelfx.rs: the beams, the steam, the
        // lens flares) fade where they meet the scene, as the game's DepthBiasedAlpha
        // does.
        bevy::core_pipeline::prepass::DepthPrepass,
        // Light for the robots (the garage's own is in its light maps).
        AmbientLight {
            color: Color::srgb(0.82, 0.86, 1.0),
            brightness: 0.22 * lighting::unit(),
            affects_lightmapped_meshes: false,
        },
        Transform::from_translation(cust.pose.0).looking_at(cust.pose.1, Vec3::Y),
        layer.clone(),
    ));
    // Its bloom, as the game camera gets the map's. Not its fog: the garage's
    // height fogs lie below the platforms' tops (under 0.94 m) and fill only what is
    // under them, which the camera looks over (the demo's fog is everywhere).
    if let Some(map) = &lights {
        map.camera_effects(&mut camera);
        camera.remove::<DistanceFog>();
    }
    // Its own fog instead, as the game draws its height fogs: each fills what is
    // below its top, which here is the pit under the platforms, seen from above
    // as a teal haze. (Far in front of the camera, so the effects are drawn after
    // it.)
    if let Some(map) = lights.as_ref().filter(|m| !m.height_fogs.is_empty()) {
        camera.with_child((
            Mesh3d(meshes.add(screen_triangle())),
            MeshMaterial3d(fogs.add(GarageFog::new(&map.height_fogs))),
            Transform::from_xyz(0.0, 0.0, -1500.0),
            bevy::camera::visibility::NoFrustumCulling,
            NotShadowCaster,
            layer.clone(),
        ));
    }
    spawned.push(camera.id());
    if let Some(map) = &lights {
        for light in &map.lights {
            // A light that reaches the robots is as strong on them as the game's
            // (at the middle of the nearest; its own falloff, not Bevy's, decides
            // that: see lighting::spawn_light_at): the lamps over the platforms
            // and the room's fills.
            let nearest = data
                .spots
                .iter()
                .map(|s| (s.at + Vec3::Y * ROBOT_MIDDLE).distance(light.position))
                .fold(f32::INFINITY, f32::min);
            // The lamps over the platforms light the characters and nothing else
            // (LAC_USER_SELECTED, neither BSP nor Static). Bevy's lights shine
            // their highlights on everything (the garage's own light is in its
            // light maps), and these hang just under the lamp fixtures, which they
            // turned white; so they shine down, on the robots and their platforms.
            let shining_down;
            let light = if light.dynamic_only && light.spot.is_none() {
                shining_down = lighting::MapLight {
                    spot: Some((Vec3::NEG_Y, LAMP_CONE.0, LAMP_CONE.1)),
                    ..*light
                };
                &shining_down
            } else {
                light
            };
            let e = if nearest < light.radius {
                lighting::spawn_light_at(&mut commands, light, nearest)
            } else {
                lighting::spawn_light(&mut commands, light)
            };
            commands.entity(e).insert(layer.clone());
            spawned.push(e);
        }
    }

    // The four robots, hidden from their re-res on (drawn while the screen loads,
    // unseen, robot and car, so that they are ready to be drawn by then).
    for class in 0..4 {
        let spot = &data.spots[class];
        let yaw = (-spot.facing.x).atan2(-spot.facing.y);
        let root = commands
            .spawn((
                Transform::from_translation(spot.at).with_rotation(Quat::from_rotation_y(yaw)),
                Visibility::Inherited,
                bevy::app::Propagate(layer.clone()),
            ))
            .id();
        let robot = commands
            .spawn((
                PreviewModel { class, car: false },
                // (Its scene from the file its clips come from: gltffiles.rs.)
                crate::gltffiles::SceneOf::load(&asset_server, ROBOT_MODEL),
                Transform::from_scale(Vec3::splat(ROBOT_SCALE)),
                Visibility::Inherited,
                ChildOf(root),
            ))
            .id();
        // (The characters' vehicles: the one the build's chest makes, which
        // character.rs puts in, dress_previews.)
        let mut car = commands.spawn((
            PreviewModel { class, car: true },
            Transform::from_scale(Vec3::splat(VEHICLE_SCALE)),
            Visibility::Inherited,
            ChildOf(root),
        ));
        if !crate::character::available() {
            car.insert(SceneRoot(
                asset_server.load(GltfAssetLabel::Scene(0).from_asset(VEHICLE_MODEL)),
            ));
        }
        let car = car.id();
        cust.previews[class] = Preview {
            root: Some(root),
            robot: Some(robot),
            car: Some(car),
            robot_player: None,
            car_player: None,
            look: None,
            car_set: 0,
            vehicle: None,
            timing: crate::character::Timing::JAZZ,
            shown: false,
            shown_at: f32::MIN,
            form: 0,
            changing: None,
            queued: false,
            repose: false,
            lift: 0.0,
            float: 0.0,
            raise: 0.0,
            raise_vel: 0.0,
            transform_lift: 0.0,
        };
        spawned.push(root);
    }
    cust.garage = spawned;
}

/// The garage is ready: the screen fades in from black, the robots re-res, the
/// camera moves from the intro view to the line-up, the class select flies in and
/// the ambience starts (UNHIDE_PreviewAll).
fn start_screen(
    commands: &mut Commands,
    cust: &mut Customize,
    data: &CustData,
    sound: Option<(&SoundBank, &Volumes)>,
    sfx: &mut Sfx,
    now: f32,
) {
    cust.phase = Phase::Open;
    cust.opened = now;
    cust.pad_hints = None;
    cust.fade
        .from_to(now, 1.0, 0.0, FADE_SECS, Ease::RegularOut);
    // The menu flies in (0.7 s): from 4000 deep, tilted back, transparent.
    cust.select
        .intro
        .from_to(now, 0.0, 1.0, 0.7, Ease::RegularOut);
    // SceneIntro, cut; then the blend to the line-up.
    cust.set_view("SceneIntro", now, true);
    cust.pose = view_pose(data, &cust.view, None, 0.0).unwrap_or(cust.pose);
    cust.set_view("Lineup", now, false);
    for p in cust.previews.iter_mut() {
        p.shown = true;
        p.shown_at = now;
    }
    sfx.play(SND_SELECT_IN);
    sfx.play(SND_DERES);

    if let Some((bank, volumes)) = sound {
        for cue in AMBIENCE_LOOPS {
            let Some(music) = bank.music(cue) else {
                continue;
            };
            for layer in music.layers.iter().filter(|l| l.looping) {
                let loudness = Loudness {
                    gain: 10f32.powf(layer.volume_db / 20.0),
                    class: music.class,
                };
                cust.ambience.push(
                    commands
                        .spawn((
                            AudioPlayer::new(layer.wave.clone()),
                            PlaybackSettings::LOOP.with_volume(loudness.volume(volumes)),
                            loudness,
                        ))
                        .id(),
                );
            }
        }
    }
    for i in 0..AMBIENCE_SHOTS.len() {
        let (_, lo, hi) = AMBIENCE_SHOTS[i];
        let r = cust.random();
        cust.ambience_next[i] = now + lo + (hi - lo) * r;
    }
}

/// Closes the screen once it has faded out: everything it made goes, and the menu
/// takes over.
fn close_now(commands: &mut Commands, cust: &mut Customize, link: &mut CustomizeLink) {
    for e in cust.garage.drain(..).chain(cust.ambience.drain(..)) {
        commands.entity(e).try_despawn();
    }
    for p in cust.previews.iter_mut() {
        *p = Preview::default();
    }
    cust.phase = Phase::Closed;
    cust.editor = None;
    cust.hits.clear();
    link.showing = false;
    link.closed = true;
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// The player's input this frame, as the movie's navigation (keyboard with key
/// repeat, gamepad, and the mouse through the UI's hit areas).
#[derive(Default)]
struct Nav {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    page_up: bool,
    page_down: bool,
    home: bool,
    end: bool,
    accept: bool,
    back: bool,
    /// X: Toggle Faction.
    faction: bool,
    /// Y: Randomize.
    randomize: bool,
    /// Change Form.
    change_form: bool,
    /// Unlock / Lock Camera.
    camera_lock: bool,
    cycle_page: bool,
    /// Turning the camera (radians this frame).
    turn: f32,
    /// The mouse on the stage, and what it is over.
    cursor: Option<Vec2>,
    moved: bool,
    clicked: Option<Target>,
    pressed: Option<Target>,
    /// The left button held down.
    held: bool,
    /// Where along the metallic slider the mouse is (0-1).
    slider_value: Option<f32>,
    wheel: f32,
    /// Typed characters (the rename prompt).
    typed: Vec<char>,
    backspace: bool,
}

#[allow(clippy::too_many_arguments)]
fn read_nav(
    cust: &Customize,
    keys: &ButtonInput<KeyCode>,
    events: &[KeyboardInput],
    mouse: &ButtonInput<MouseButton>,
    motion: &AccumulatedMouseMotion,
    scroll: &AccumulatedMouseScroll,
    gamepads: &Query<&Gamepad>,
    window: &Window,
    view: &crate::flash::FlashView,
    dt: f32,
    typing: bool,
) -> Nav {
    let mut nav = Nav::default();
    // The rename prompt takes a paste too (Ctrl+V or Shift+Insert: the clipboard's
    // first line, clipboard.rs), as typing; the shortcut's V isn't typed.
    if typing
        && crate::clipboard::paste_pressed(keys)
        && let Some(line) = crate::clipboard::line()
    {
        nav.typed.extend(line.chars().filter(|c| !c.is_control()));
    }
    for e in events {
        if e.state != ButtonState::Pressed {
            continue;
        }
        if typing && crate::clipboard::is_paste_key(e, keys) {
            continue;
        }
        if typing {
            match e.key_code {
                KeyCode::Enter | KeyCode::NumpadEnter if !e.repeat => nav.accept = true,
                KeyCode::Escape if !e.repeat => nav.back = true,
                KeyCode::Backspace => nav.backspace = true,
                _ => {
                    if let bevy::input::keyboard::Key::Character(s) = &e.logical_key {
                        nav.typed.extend(s.chars().filter(|c| !c.is_control()));
                    } else if e.logical_key == bevy::input::keyboard::Key::Space {
                        nav.typed.push(' ');
                    }
                }
            }
            continue;
        }
        match e.key_code {
            KeyCode::ArrowUp | KeyCode::KeyW => nav.up = true,
            KeyCode::ArrowDown | KeyCode::KeyS => nav.down = true,
            KeyCode::ArrowLeft | KeyCode::KeyA => nav.left = true,
            KeyCode::ArrowRight | KeyCode::KeyD => nav.right = true,
            KeyCode::PageUp => nav.page_up = true,
            KeyCode::PageDown => nav.page_down = true,
            KeyCode::Home => nav.home = true,
            KeyCode::End => nav.end = true,
            _ if e.repeat => {}
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => nav.accept = true,
            KeyCode::Escape | KeyCode::Backspace => nav.back = true,
            KeyCode::KeyX => nav.faction = true,
            KeyCode::KeyY | KeyCode::KeyR => nav.randomize = true,
            KeyCode::KeyC | KeyCode::KeyF => nav.change_form = true,
            KeyCode::KeyL => nav.camera_lock = true,
            KeyCode::Tab => nav.cycle_page = true,
            _ => {}
        }
    }
    let _ = keys;
    if mouse.just_released(MouseButton::Right) && !typing {
        nav.back = true;
    }
    for pad in gamepads.iter() {
        pad_nav(&mut nav, pad, dt);
    }
    if !typing {
        if keys.pressed(KeyCode::KeyQ) {
            nav.turn -= CAMERA_TURN_SPEED * dt;
        }
        if keys.pressed(KeyCode::KeyE) {
            nav.turn += CAMERA_TURN_SPEED * dt;
        }
    }
    nav.moved = motion.delta != Vec2::ZERO;
    nav.wheel = scroll.delta.y;
    if let Some(p) = window.cursor_position() {
        let at = view.to_stage(p);
        nav.cursor = Some(at);
        let over = target_at(cust, view, at);
        if mouse.just_pressed(MouseButton::Left) {
            nav.pressed = over;
        }
        if mouse.just_released(MouseButton::Left) {
            nav.clicked = over;
        }
        nav.held = mouse.pressed(MouseButton::Left);
        // The slider's value under the mouse (TransSlider: its bar runs from 32 px
        // in to 12 px from its far end).
        if let Some(h) = cust.hits.iter().find(|h| h.target == Target::Slider)
            && let Some(local) = crate::flash::unproject([at.x, at.y], view.focal, &h.matrix)
        {
            let (x0, x1) = (h.rect[0] + 32.0, h.rect[2] - 12.0);
            nav.slider_value = Some(((local[0] - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0));
        }
        // Dragging the camera round (while unlocked), from anywhere but the menus.
        if cust.dragging_camera && mouse.pressed(MouseButton::Left) {
            nav.turn += motion.delta.x * CAMERA_TURN_PER_PIXEL;
        }
    }
    nav
}

/// A controller's part of the frame's input: the D-pad moves, A takes, B or Back
/// goes back, and the hint bar's buttons (Hint::pad_button): X, Y, the sticks'
/// clicks, D-pad right (and RT) for Cycle Page; the right stick turns the camera.
fn pad_nav(nav: &mut Nav, pad: &Gamepad, dt: f32) {
    nav.up |= pad.just_pressed(GamepadButton::DPadUp);
    nav.down |= pad.just_pressed(GamepadButton::DPadDown);
    nav.left |= pad.just_pressed(GamepadButton::DPadLeft);
    nav.right |= pad.just_pressed(GamepadButton::DPadRight);
    nav.accept |= pad.just_pressed(GamepadButton::South);
    nav.back |= pad.just_pressed(GamepadButton::East) || pad.just_pressed(GamepadButton::Select);
    nav.faction |= pad.just_pressed(GamepadButton::West);
    nav.randomize |= pad.just_pressed(GamepadButton::North);
    nav.change_form |= pad.just_pressed(GamepadButton::LeftThumb);
    nav.camera_lock |= pad.just_pressed(GamepadButton::RightThumb);
    nav.cycle_page |= pad.just_pressed(GamepadButton::RightTrigger);
    let x = pad.get(GamepadAxis::RightStickX).unwrap_or(0.0);
    if x.abs() > 0.2 {
        nav.turn += x * CAMERA_TURN_SPEED * dt;
    }
}

/// What the stage point `at` is over (the last drawn first: it is on top).
pub(crate) fn target_at(
    cust: &Customize,
    view: &crate::flash::FlashView,
    at: Vec2,
) -> Option<Target> {
    cust.hits.iter().rev().find_map(|h| {
        let p = crate::flash::unproject([at.x, at.y], view.focal, &h.matrix)?;
        let [x0, y0, x1, y1] = h.rect;
        (p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1).then_some(h.target)
    })
}

#[allow(clippy::too_many_arguments)]
fn customize_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut key_events: MessageReader<KeyboardInput>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    gamepads: Query<&Gamepad>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    view: Res<crate::flash::FlashView>,
    data: Option<Res<CustData>>,
    mut cust: ResMut<Customize>,
    mut sfx: ResMut<Sfx>,
) {
    let events: Vec<KeyboardInput> = key_events.read().cloned().collect();
    let (Some(window), Some(data)) = (window, data) else {
        return;
    };
    if cust.phase != Phase::Open {
        return;
    }
    let now = time.elapsed_secs();
    let typing = cust.editor.as_ref().is_some_and(|e| e.rename.is_some());
    let nav = read_nav(
        &cust,
        &keys,
        &events,
        &mouse,
        &motion,
        &scroll,
        &gamepads,
        &window,
        &view,
        time.delta_secs(),
        typing,
    );
    let cust = &mut *cust;
    // Which the hint bar shows the buttons of: the controller or the keyboard and
    // mouse, whichever was used last.
    let pad_used = gamepads.iter().any(|pad| {
        pad.get_just_pressed().next().is_some()
            || pad.left_stick().length() > PAD_USED_STICK
            || pad.right_stick().length() > PAD_USED_STICK
    });
    let desk_used = events.iter().any(|e| e.state == ButtonState::Pressed)
        || mouse.get_just_pressed().next().is_some()
        || motion.delta.length() > DESK_USED_MOTION
        || scroll.delta != Vec2::ZERO;
    cust.pad_hints = Some(pad_hints_after(
        cust.pad_hints,
        !gamepads.is_empty(),
        pad_used,
        desk_used,
    ));
    // Still fading in: nothing yet.
    if now - cust.opened < FADE_SECS * 0.5 {
        return;
    }

    // What the mouse is over (the hint buttons light up under it).
    let over = nav.cursor.and_then(|p| target_at(cust, &view, p));
    cust.hover = over;
    for hint in cust.hints() {
        let on = over == Some(Target::Hint(hint)) && !typing;
        let t = cust.hint_hover.entry(hint).or_insert(Tween::at(0.0));
        let goal = if on { 1.0 } else { 0.0 };
        if t.target() != goal {
            if on {
                sfx.play(SND_BUTTON_OVER);
            }
            t.to(now, goal, 0.2, Ease::RegularOut);
        }
    }
    // (Under the rename prompt the hints don't answer.)
    if let Some(Target::Hint(hint)) = nav.clicked
        && !typing
    {
        sfx.play(SND_BUTTON_CLICK);
        hint_action(cust, &data, hint, now, &mut sfx);
        return;
    }
    // Camera dragging starts on empty space.
    if nav.pressed.is_some() || mouse.just_released(MouseButton::Left) {
        cust.dragging_camera = false;
    }
    if mouse.just_pressed(MouseButton::Left) && nav.pressed.is_none() && !typing {
        cust.dragging_camera = true;
    }

    if cust.editor.is_some() {
        editor_input(cust, &data, &nav, now, &mut sfx);
    } else {
        select_input(cust, &data, &nav, now, &mut sfx);
    }
}

/// A stick pushed this far (of 1) is the controller being used; the mouse moved
/// this far in a frame (its own pixels) is the mouse.
const PAD_USED_STICK: f32 = 0.5;
const DESK_USED_MOTION: f32 = 2.0;

/// Whether the hint bar shows the controller's buttons, after a frame: while a
/// controller is connected, from when one is used (a button, a stick) till the
/// keyboard or mouse is (a key, a click, the mouse moved); as the screen opens
/// (`was` None), whenever one is connected.
fn pad_hints_after(was: Option<bool>, connected: bool, pad_used: bool, desk_used: bool) -> bool {
    if !connected {
        false
    } else if pad_used {
        true
    } else if desk_used {
        false
    } else {
        was.unwrap_or(true)
    }
}

/// A hint button's action (also its key or gamepad button).
fn hint_action(cust: &mut Customize, data: &CustData, hint: Hint, now: f32, sfx: &mut Sfx) {
    match hint {
        Hint::ToggleFaction => cust.faction = 1 - cust.faction,
        Hint::Close => close_select(cust, now, sfx),
        Hint::CameraLock => {
            if let Some(ed) = cust.editor.as_mut() {
                ed.camera_locked = !ed.camera_locked;
            }
        }
        Hint::Randomize => randomize(cust, data, now),
        Hint::ChangeForm => change_form(cust, now),
        Hint::CyclePage => cycle_page(cust, now, sfx, true),
        // In a list: the list closes, as Esc does.
        Hint::Back if cust.editor.as_ref().is_some_and(|e| e.sub.is_some()) => {
            close_sub(cust, data, false, now, sfx)
        }
        Hint::Back => close_editor(cust, now, sfx),
    }
}

// ---------------------------------------------------------------------------
// Class select
// ---------------------------------------------------------------------------

/// The x offsets resizeButtons gives the classes for each focused one.
const RESIZE: [[f32; 4]; 4] = [
    [0.0, 40.0, 20.0, 0.0],
    [0.0, -20.0, 20.0, 0.0],
    [0.0, -20.0, -40.0, 0.0],
    [0.0, -20.0, -40.0, -60.0],
];

/// Focus moves to class `class` (the old one's list loses focus).
fn focus_class(cust: &mut Customize, class: usize, now: f32) {
    if let Some(old) = cust.select.focus
        && old != class
    {
        collapse(&mut cust.select.classes[old], now);
    }
    cust.select.focus = Some(class);
    // resizeButtons: instant.
    for i in 0..4 {
        cust.select.classes[i].x_offset = RESIZE[class][i];
        cust.select.classes[i].scale = if i == class { 1.0 } else { 0.95 };
    }
    expand(&mut cust.select.classes[class], now);
}

fn expand(c: &mut ClassUi, now: f32) {
    c.locked = false;
    c.display.to(now, 1.0, 0.4, Ease::RegularOut);
    c.parts.to(now, 1.0, 0.5, Ease::RegularOut);
    c.slots.to(now, 1.0, 0.25, Ease::RegularOut);
}

fn collapse(c: &mut ClassUi, now: f32) {
    if !c.locked {
        c.display.to(now, 0.0, 0.4, Ease::RegularOut);
        c.parts.to(now, 0.0, 0.5, Ease::RegularOut);
    }
    c.slots.to(now, 0.0, 0.25, Ease::RegularOut);
}

fn select_input(cust: &mut Customize, data: &CustData, nav: &Nav, now: f32, sfx: &mut Sfx) {
    // setInitialFocus: the scout's list, a tick after the menu opens.
    let Some(focus) = cust.select.focus else {
        focus_class(cust, 0, now);
        return;
    };
    if nav.faction {
        cust.faction = 1 - cust.faction;
    }
    if nav.back {
        close_select(cust, now, sfx);
        return;
    }
    // The mouse: over a class's tag or slot focuses it; over a slot selects it.
    if nav.moved {
        match cust.hover {
            // (A locked slot is a disabled button: the mouse passes it by.)
            Some(Target::Slot(c, s)) if cust.slot_open(c, s) => {
                if c != focus {
                    focus_class(cust, c, now);
                }
                if cust.select.slot[c] != s {
                    sfx.play(if s > cust.select.slot[c] {
                        SND_MENU_DOWN
                    } else {
                        SND_MENU_UP
                    });
                    cust.select.slot[c] = s;
                }
            }
            Some(Target::Class(c)) if c != focus => focus_class(cust, c, now),
            _ => {}
        }
    }
    let focus = cust.select.focus.unwrap_or(0);
    if nav.left && focus > 0 {
        focus_class(cust, focus - 1, now);
        sfx.play(SND_SELECT_LEFT);
    } else if nav.right && focus < 3 {
        focus_class(cust, focus + 1, now);
        sfx.play(SND_SELECT_RIGHT);
    }
    let focus = cust.select.focus.unwrap_or(0);
    let slot = &mut cust.select.slot[focus];
    if nav.up && *slot > 0 {
        *slot -= 1;
        sfx.play(SND_MENU_UP);
    } else if nav.down && *slot + 1 < SLOTS {
        *slot += 1;
        sfx.play(SND_MENU_DOWN);
    }
    // A slot held down shows pressed until the button comes up (a locked one,
    // disabled, neither presses nor opens).
    if let Some(Target::Slot(c, s)) = nav.pressed
        && cust.slot_open(c, s)
    {
        cust.select.pressed = Some((c, s));
    }
    if !nav.held {
        cust.select.pressed = None;
    }
    let clicked = match nav.clicked {
        Some(Target::Slot(c, s)) if cust.slot_open(c, s) => {
            cust.select.slot[c] = s;
            Some(c)
        }
        _ => None,
    };
    if nav.accept || clicked.is_some() {
        let class = clicked.unwrap_or(focus);
        let slot = cust.select.slot[class];
        if cust.slot_open(class, slot) {
            open_editor(cust, data, class, slot, now, sfx);
        }
    }
}

/// The screen closed from wherever it is (the menu's CustomizeLink::close): the
/// editor's build kept as far as Back would keep it (commit's: an open list's
/// choice not taken) and saved, as closing the editor saves it; then closeMenu.
fn close_screen(cust: &mut Customize, now: f32, sfx: &mut Sfx) {
    if cust.editor.take().is_some() {
        save_builds(&cust.builds);
    }
    close_select(cust, now, sfx);
}

/// closeMenu: the robots de-res, the glows go out, and the screen fades to black.
fn close_select(cust: &mut Customize, now: f32, sfx: &mut Sfx) {
    sfx.play(SND_SELECT_OUT);
    cust.phase = Phase::Closing(now);
    cust.fade.to(now, 1.0, FADE_SECS, Ease::RegularIn);
    cust.select.focus = None;
    for p in cust.previews.iter_mut() {
        if p.shown {
            p.shown = false;
            p.shown_at = now;
        }
    }
    sfx.play(SND_DERES);
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

/// The pages' places in the stack: x, y, z (twips) and alpha of slots 0-2.
const PAGE_SLOTS: [(f32, f32, f32, f32); 3] = [
    (0.0, 0.0, 0.0, 1.0),
    (35.0, -55.0, 2000.0, 1.0 / 3.0),
    (45.0, -85.0, 2800.0, 1.0 / 6.0),
];
/// Rows per page: Loadouts 6, Armor 8, Personality 5 (its third is the slider).
pub(crate) const PAGE_ROWS: [usize; 3] = [6, 8, 5];

/// characterSelected / modifyCharacter: the editor opens on a build.
fn open_editor(
    cust: &mut Customize,
    data: &CustData,
    class: usize,
    slot: usize,
    now: f32,
    sfx: &mut Sfx,
) {
    sfx.play(SND_MENU_IN);
    let build = cust.builds[class][slot].clone();
    let mut ed = EditorUi {
        class,
        slot,
        level: cust.progress.level(class),
        build,
        intro: Tween::at(0.0),
        ring: [0, 1, 2],
        page_x: [Tween::at(0.0); 3],
        page_y: [Tween::at(0.0); 3],
        page_z: [Tween::at(0.0); 3],
        page_alpha: [Tween::at(0.0); 3],
        page_seen: [false; 3],
        row: [0; 3],
        pops: [[Tween::at(0.0); 8]; 3],
        sub: None,
        info: Vec::new(),
        info_due: Some(now + 0.5),
        camera_locked: true,
        randomize_ready: now,
        spinner: None,
        rename: None,
        dragging_slider: false,
    };
    ed.intro.from_to(now, 0.0, 1.0, 1.2, Ease::StrongOut);
    place_pages(&mut ed, now);
    focus_page(&mut ed, now);
    // The class's tag stays expanded and flies to the header; the others hide.
    let c = &mut cust.select.classes[class];
    c.locked = true;
    c.slots.to(now, 0.0, 0.25, Ease::RegularOut);
    c.fly.to(now, 1.0, 1.2, Ease::StrongOut);
    for (i, c) in cust.select.classes.iter_mut().enumerate() {
        c.hidden = i != class;
    }
    // HIDE_PreviewAll, UNHIDE_<pawn>: every robot de-reses, this one comes back.
    for (i, p) in cust.previews.iter_mut().enumerate() {
        if p.shown {
            p.shown = false;
            p.shown_at = now;
        }
        if i == class {
            p.shown = true;
            p.shown_at = now + DERES_HIDE_AT;
        }
    }
    sfx.play(SND_DERES);
    cust.set_view(SPECIALTIES[class], now, false);
    cust.yaw = 0.0;
    cust.editor = Some(ed);
    let _ = data;
}

/// updateRendererPositioning: each page tweens to its slot (0.4 s).
fn place_pages(ed: &mut EditorUi, now: f32) {
    for (slot, &page) in ed.ring.iter().enumerate() {
        let (x, y, z, a) = PAGE_SLOTS[slot];
        ed.page_x[page].to(now, x, 0.4, Ease::RegularInOut);
        ed.page_y[page].to(now, y, 0.4, Ease::RegularInOut);
        ed.page_z[page].to(now, z, 0.4, Ease::RegularInOut);
        ed.page_alpha[page].to(now, a, 0.4, Ease::RegularInOut);
    }
}

/// updatePageFocus: the front page takes focus; the info panel follows 0.5 s later.
fn focus_page(ed: &mut EditorUi, now: f32) {
    let front = ed.ring[0];
    ed.page_seen[front] = true;
    for p in ed.info.iter_mut() {
        if p.leaving.is_none() {
            p.leaving = Some(now);
        }
    }
    ed.info_due = Some(now + 0.5);
    let row = ed.row[front];
    pop_row(ed, front, row, now);
}

/// The focused row of a page pops out toward the camera; the others go back.
fn pop_row(ed: &mut EditorUi, page: usize, row: usize, now: f32) {
    for (i, t) in ed.pops[page].iter_mut().enumerate() {
        let goal = if i == row { 1.0 } else { 0.0 };
        if t.target() != goal {
            t.to(now, goal, 0.2, Ease::RegularOut);
        }
    }
}

fn cycle_page(cust: &mut Customize, now: f32, sfx: &mut Sfx, right: bool) {
    let Some(ed) = cust.editor.as_mut() else {
        return;
    };
    if ed.sub.is_some() || ed.rename.is_some() {
        return;
    }
    sfx.play(SND_TAB);
    if right {
        ed.ring.rotate_left(1);
    } else {
        ed.ring.rotate_right(1);
    }
    place_pages(ed, now);
    focus_page(ed, now);
}

fn editor_input(cust: &mut Customize, data: &CustData, nav: &Nav, now: f32, sfx: &mut Sfx) {
    let Some(ed) = cust.editor.as_mut() else {
        return;
    };
    // The rename prompt takes everything.
    if let Some(rename) = ed.rename.as_mut() {
        for c in &nav.typed {
            if rename.text.chars().count() < RENAME_MAX {
                rename.text.push(*c);
            }
        }
        if nav.backspace {
            rename.text.pop();
        }
        let ok = nav.accept || nav.clicked == Some(Target::PromptOk);
        let cancel = nav.back || nav.clicked == Some(Target::PromptCancel);
        if ok {
            // KeyboardDone: a name with something besides spaces is taken.
            let name = rename.text.trim().to_string();
            if !name.is_empty() {
                ed.build.name = name;
                commit(cust);
            }
            sfx.play(SND_BUTTON_CLICK);
            if let Some(ed) = cust.editor.as_mut() {
                ed.rename = None;
            }
        } else if cancel {
            ed.rename = None;
            sfx.play(SND_BUTTON_CLICK);
        }
        return;
    }
    if nav.camera_lock {
        ed.camera_locked = !ed.camera_locked;
    }
    if nav.randomize && ed.sub.is_none() {
        randomize(cust, data, now);
        return;
    }
    if nav.change_form && cust.hints().contains(&Hint::ChangeForm) {
        change_form(cust, now);
    }
    let Some(ed) = cust.editor.as_mut() else {
        return;
    };
    if !ed.camera_locked && nav.turn != 0.0 {
        cust.yaw += nav.turn;
    }
    let ed = cust.editor.as_mut().unwrap();
    if ed.sub.is_some() {
        sub_input(cust, data, nav, now, sfx);
        return;
    }
    let front = ed.ring[0];
    let rows = PAGE_ROWS[front];
    // The slider (Personality's third row) takes Left / Right while focused.
    let slider_focused = front == 2 && ed.row[front] == 2;
    if slider_focused && (nav.left || nav.right) {
        let step = if nav.right { 0.05 } else { -0.05 };
        let v = (ed.build.metallic + step).clamp(0.0, 1.0);
        sfx.play(SND_SLIDER_STEP);
        if v != ed.build.metallic {
            ed.build.metallic = v;
            sfx.play(SND_SLIDER);
            commit(cust);
        }
        return;
    }
    // Dragging the slider: the value follows the mouse along it.
    if nav.pressed == Some(Target::Slider) {
        ed.dragging_slider = true;
    }
    if ed.dragging_slider {
        if !nav.held {
            ed.dragging_slider = false;
        } else if let Some(v) = nav.slider_value {
            if (v - ed.build.metallic).abs() > 1e-4 {
                ed.build.metallic = v;
                sfx.play(SND_SLIDER);
                commit(cust);
            }
            return;
        }
    }
    if nav.left {
        cycle_page(cust, now, sfx, false);
        return;
    }
    if nav.right || nav.cycle_page {
        cycle_page(cust, now, sfx, true);
        return;
    }
    if nav.back {
        close_editor(cust, now, sfx);
        return;
    }
    let ed = cust.editor.as_mut().unwrap();
    let mut row = ed.row[front];
    if nav.up && row > 0 {
        row -= 1;
        sfx.play(SND_MENU_UP);
    } else if nav.down && row + 1 < rows {
        row += 1;
        sfx.play(SND_MENU_DOWN);
    }
    if nav.moved
        && let Some(Target::Row(r)) = cust.hover
        && r != row
        && r < rows
    {
        sfx.play(if r > row { SND_MENU_DOWN } else { SND_MENU_UP });
        row = r;
    }
    let ed = cust.editor.as_mut().unwrap();
    if row != ed.row[front] {
        ed.row[front] = row;
        pop_row(ed, front, row, now);
    }
    let clicked_row = match nav.clicked {
        Some(Target::Row(r)) if r < rows => Some(r),
        _ => None,
    };
    if nav.accept || clicked_row.is_some() {
        let row = clicked_row.unwrap_or(row);
        activate_row(cust, data, front, row, now, sfx);
    }
}

/// A page row chosen: its list opens (or the prompt, or nothing for the slider).
fn activate_row(
    cust: &mut Customize,
    data: &CustData,
    page: usize,
    row: usize,
    now: f32,
    sfx: &mut Sfx,
) {
    let field = match (page, row) {
        (0, 0) => Field::Weapon(0),
        (0, 1) => Field::WeaponUpgrade(0),
        (0, 2) => Field::Weapon(1),
        (0, 3) => Field::WeaponUpgrade(1),
        (0, 4) => Field::Ability,
        (0, 5) => Field::AbilityUpgrade,
        // Head, Chest/Vehicle, Shoulders, Arms, Legs, aux, Decals, Armor Set.
        (1, 0) => Field::Part(3),
        (1, 1) => Field::Part(2),
        (1, 2) => Field::Part(1),
        (1, 3) => Field::Part(0),
        (1, 4) => Field::Part(4),
        (1, 5) => Field::Part(5),
        (1, 6) => Field::Part(6),
        (1, 7) => Field::ArmorSet,
        (2, 0) => Field::Swatch(0),
        (2, 1) => Field::Swatch(1),
        (2, 3) => Field::Voice,
        (2, 4) => {
            if let Some(ed) = cust.editor.as_mut() {
                ed.rename = Some(Rename {
                    text: String::new(),
                    opened: now,
                });
                sfx.play(SND_BUTTON_CLICK);
            }
            return;
        }
        _ => return,
    };
    open_sub(cust, data, field, now, sfx);
}

/// The rows a list offers for a field, and which one the build has. Weapons,
/// upgrades, abilities and ability upgrades the class's level hasn't opened show
/// locked with the level they need (the list's GetOptionLockStatus and its
/// RequiresLevel by the lock); the weapon in the other slot, "In Use". A part not
/// bought yet shows its price, an armor set the price of its parts not bought
/// (configArmorSetButton); a bonus head needing a level or Prime Mode shows locked
/// with what it needs ("25", "PRIME", "PRIME 25", "PRIME 100").
pub(crate) fn field_rows(
    data: &CustData,
    ed: &EditorUi,
    progress: &crate::xp::Progress,
    field: Field,
) -> (Vec<OptionRow>, usize) {
    let none = data.string("Shared.None", "None");
    let row = |id: &str, label: String| OptionRow {
        id: id.to_string(),
        label,
        locked: None,
        price: 0,
    };
    let level = ed.level;
    let item_row = |i: &Item| OptionRow {
        id: i.id.clone(),
        label: i.name.clone(),
        locked: (!crate::xp::unlocked(i.level, level)).then(|| i.level.to_string()),
        price: 0,
    };
    let prime = data.string("CustomCharacter.Prime", "Prime");
    let b = &ed.build;
    let rows: Vec<OptionRow> = match field {
        Field::Part(i) => data
            .parts_for(ed.class, i)
            .iter()
            .map(|p| {
                let lock = p.lock(progress, ed.class);
                OptionRow {
                    locked: lock.and_then(|l| l.label(&prime)),
                    price: match lock {
                        Some(crate::xp::Lock::Price(price)) => price,
                        _ => 0,
                    },
                    ..row(&p.id, p.name.clone())
                }
            })
            .collect(),
        Field::ArmorSet => data
            .armor_sets_for(ed.class)
            .iter()
            .map(|s| OptionRow {
                price: data.set_price(&s.2, progress, ed.class),
                ..row(&s.2.join(","), s.1.clone())
            })
            .collect(),
        Field::Swatch(f) => data.swatches[f]
            .iter()
            .map(|s| row(&s.id, String::new()))
            .collect(),
        // (And None, the demo's own: no voice at all, voice.rs.)
        Field::Voice => std::iter::once(row(crate::voice::NONE, none.clone()))
            .chain(
                data.voices_for(ed.class)
                    .iter()
                    .map(|v| row(&v.1, v.2.clone())),
            )
            .collect(),
        Field::Weapon(slot) => {
            let other = &b.weapons[1 - slot];
            data.weapons_for(ed.class)
                .iter()
                .map(|w| {
                    let mut r = item_row(w);
                    if w.id == *other {
                        r.locked = Some(data.string("CustomCharacter.InUse", "In Use"));
                    }
                    r
                })
                .collect()
        }
        Field::WeaponUpgrade(slot) => {
            let mut v = vec![row("", none.clone())];
            v.extend(
                CustData::upgrades_for(&data.weapon_upgrades, &b.weapons[slot])
                    .iter()
                    .map(|u| item_row(u)),
            );
            v
        }
        Field::Ability => data
            .abilities_for(ed.class)
            .iter()
            .map(|a| item_row(a))
            .collect(),
        Field::AbilityUpgrade => {
            let mut v = vec![row("", none)];
            v.extend(
                CustData::upgrades_for(&data.skills, &b.ability)
                    .iter()
                    .map(|u| item_row(u)),
            );
            v
        }
    };
    let current = field_value(b, field);
    let selected = rows.iter().position(|r| r.id == current).unwrap_or(0);
    (rows, selected)
}

/// The build's value for a field (the armor set: its parts, comma-separated).
fn field_value(b: &Build, field: Field) -> String {
    match field {
        Field::Part(i) => b.parts[i].clone(),
        Field::ArmorSet => b.parts.join(","),
        Field::Swatch(f) => b.swatches[f].clone(),
        Field::Voice => b.voice.clone(),
        Field::Weapon(s) => b.weapons[s].clone(),
        Field::WeaponUpgrade(s) => b.weapon_upgrades[s].clone(),
        Field::Ability => b.ability.clone(),
        Field::AbilityUpgrade => b.ability_upgrade.clone(),
    }
}

fn set_field(b: &mut Build, field: Field, value: &str) {
    match field {
        Field::Part(i) => b.parts[i] = value.to_string(),
        Field::ArmorSet => {
            let parts: Vec<&str> = value.split(',').collect();
            if parts.len() == 7 {
                b.parts = std::array::from_fn(|i| parts[i].to_string());
            }
        }
        Field::Swatch(f) => b.swatches[f] = value.to_string(),
        Field::Voice => b.voice = value.to_string(),
        Field::Weapon(s) => b.weapons[s] = value.to_string(),
        Field::WeaponUpgrade(s) => b.weapon_upgrades[s] = value.to_string(),
        Field::Ability => b.ability = value.to_string(),
        Field::AbilityUpgrade => b.ability_upgrade = value.to_string(),
    }
}

/// The list's title (UIText Select*), upper-cased by the movie.
fn field_title(data: &CustData, class: usize, field: Field) -> String {
    let (key, fallback) = match field {
        Field::Part(0) => ("SelectLowerArm", "Select Arms"),
        Field::Part(1) => ("SelectUpperArm", "Select Shoulders"),
        Field::Part(2) => ("SelectChest", "Select Chest/Vehicle"),
        Field::Part(3) => ("SelectHead", "Select Head"),
        Field::Part(4) => ("SelectLeg", "Select Legs"),
        Field::Part(5) => match data.classes[class].aux.as_str() {
            "Wings" => ("SelectWing", "Select Wings"),
            "Cannon" => ("SelectCannon", "Select Cannon"),
            _ => ("SelectWheel", "Select Wheels"),
        },
        Field::Part(_) => ("SelectDecals", "Select Decals"),
        Field::ArmorSet => ("SelectArmorSet", "Select Armor Set"),
        Field::Swatch(0) => ("SelectColorAutobot", "Select Autobot Colors"),
        Field::Swatch(_) => ("SelectColorDecepticon", "Select Decepticon Colors"),
        Field::Voice => ("SelectVoice", "Select Voice"),
        Field::Weapon(0) => ("SelectPrimaryWeapon", "Select Primary Weapon"),
        Field::Weapon(_) => ("SelectSecondaryWeapon", "Select Secondary Weapon"),
        Field::WeaponUpgrade(_) => ("SelectWeaponUpgrade", "Select Weapon Upgrade"),
        Field::Ability => ("SelectAbility", "Select Ability"),
        Field::AbilityUpgrade => ("SelectAbilityUpgrade", "Select Ability Upgrade"),
    };
    data.string(&format!("CustomCharacter.{key}"), fallback)
        .to_uppercase()
}

/// The close-up a list asks for (parts: the part; weapons: the weapon arm).
fn field_camera(class: usize, field: Field) -> Option<String> {
    let spec = SPECIALTIES[class];
    let part = match field {
        Field::Part(0) => "ArmLo",
        Field::Part(1) => "ArmUp",
        Field::Part(2) => "Chest",
        Field::Part(3) => "Head",
        Field::Part(4) => "Leg",
        Field::Weapon(_) => "ArmWeapon",
        _ => return None,
    };
    Some(format!("{spec}{part}"))
}

/// Opens a field's option list (or colour grid) over the front page.
fn open_sub(cust: &mut Customize, data: &CustData, field: Field, now: f32, sfx: &mut Sfx) {
    let Some(ed) = cust.editor.as_mut() else {
        return;
    };
    let (rows, selected) = field_rows(data, ed, &cust.progress, field);
    // An upgrade list with nothing but "None" (a weapon without upgrades) doesn't open.
    if rows.is_empty()
        || (matches!(field, Field::WeaponUpgrade(_) | Field::AbilityUpgrade) && rows.len() <= 1)
    {
        return;
    }
    sfx.play(SND_LIST_IN);
    let swatches = matches!(field, Field::Swatch(_));
    let camera = field_camera(ed.class, field);
    let mut sub = SubMenu {
        field,
        swatches,
        title: field_title(data, ed.class, field),
        pops: vec![Tween::at(0.0); rows.len()],
        rows,
        selected,
        scroll: 0,
        z: Tween::at(-800.0),
        before: ed.build.clone(),
        camera: camera.clone(),
        blink: None,
        opened: now,
    };
    sub.z.to(now, -1600.0, 0.4, Ease::RegularOut);
    scroll_to(&mut sub);
    if let Some(t) = sub.pops.get_mut(selected) {
        t.to(now, 1.0, 0.2, Ease::RegularOut);
    }
    // The Armor page's info panel hides while a part list is open.
    if matches!(field, Field::Part(_) | Field::ArmorSet) {
        for p in ed.info.iter_mut().filter(|p| p.leaving.is_none()) {
            p.alpha.to(now, 0.0, 0.4, Ease::RegularOut);
        }
    }
    ed.sub = Some(sub);
    if let Some(cam) = camera {
        // A close-up shows the robot.
        to_robot_form(cust, now);
        cust.set_view(&cam, now, false);
    }
    // Opening applies the current value (and sounds the list).
    sfx.play(SND_LIST_DOWN);
}

/// Keeps the selection in the list's window (8 rows; the grid, 10 rows of 3).
fn scroll_to(sub: &mut SubMenu) {
    if sub.swatches {
        let row = sub.selected / 3;
        if row < sub.scroll {
            sub.scroll = row;
        } else if row >= sub.scroll + 10 {
            sub.scroll = row - 9;
        }
    } else if sub.selected < sub.scroll {
        sub.scroll = sub.selected;
    } else if sub.selected >= sub.scroll + 8 {
        sub.scroll = sub.selected - 7;
    }
}

fn sub_input(cust: &mut Customize, data: &CustData, nav: &Nav, now: f32, sfx: &mut Sfx) {
    let hover = cust.hover;
    let ed = cust.editor.as_mut().unwrap();
    let sub = ed.sub.as_mut().unwrap();
    let n = sub.rows.len();
    let old = sub.selected;
    let mut sel = old as i64;
    if sub.swatches {
        if nav.up {
            sel -= 3;
        }
        if nav.down {
            sel += 3;
        }
        if nav.left && sel % 3 > 0 {
            sel -= 1;
        }
        if nav.right && sel % 3 < 2 {
            sel += 1;
        }
        if nav.page_up {
            sel -= 30;
        }
        if nav.page_down {
            sel += 30;
        }
        if sel < 0 || sel >= n as i64 {
            // Past the ends: nothing happens (not consumed).
            sel = old as i64;
        }
    } else {
        if nav.up {
            sel -= 1;
        }
        if nav.down {
            sel += 1;
        }
        if nav.page_up {
            sel = (sel - 8).max(0);
        }
        if nav.page_down {
            sel = (sel + 8).min(n as i64 - 1);
        }
        sel = sel.clamp(0, n as i64 - 1);
    }
    if nav.home {
        sel = 0;
    }
    if nav.end {
        sel = n as i64 - 1;
    }
    if nav.moved
        && let Some(Target::Option(i)) = hover
        && i < n
    {
        sel = i as i64;
    }
    // The mouse wheel scrolls the list (8 rows) or the grid (10 rows of 3).
    let (window, per_row) = if sub.swatches { (10, 3) } else { (8, 1) };
    let rows = n.div_ceil(per_row);
    if nav.wheel != 0.0 && rows > window {
        let s = sub.scroll as f32 - nav.wheel.signum();
        sub.scroll = (s.max(0.0) as usize).min(rows - window);
    }
    let sel = sel.clamp(0, n.max(1) as i64 - 1) as usize;
    if sel != old {
        sub.selected = sel;
        scroll_to(sub);
        sfx.play(if sel > old {
            SND_LIST_DOWN
        } else {
            SND_LIST_UP
        });
        for (i, t) in sub.pops.iter_mut().enumerate() {
            let goal = if i == sel { 1.0 } else { 0.0 };
            if t.target() != goal {
                t.to(now, goal, 0.2, Ease::RegularOut);
            }
        }
        // updateOption: the robot shows it straight away (the colour grid commits too),
        // and a voice says its line (preview_voice).
        let value = sub.rows[sel].id.clone();
        let field = sub.field;
        set_field(&mut ed.build, field, &value);
        if field == Field::Voice {
            cust.voice_preview = Some(value);
        }
        if field_commits_live(field) {
            commit(cust);
        }
    }
    let ed = cust.editor.as_mut().unwrap();
    let sub = ed.sub.as_mut().unwrap();
    let clicked = match nav.clicked {
        Some(Target::Option(i)) if i < n => Some(i),
        _ => None,
    };
    if nav.accept || clicked.is_some() {
        let i = clicked.unwrap_or(sub.selected);
        if sub.rows[i].locked.is_some() {
            // saveAndClose on a locked row: its lock blinks.
            sub.blink = Some((i, now));
            sfx.play(SND_LOCKED);
            return;
        }
        let value = sub.rows[i].id.clone();
        let field = sub.field;
        let (price, label, class) = (sub.rows[i].price, sub.rows[i].label.clone(), ed.class);
        if (field == Field::ArmorSet || price > 0)
            && !buy(cust, data, field, class, &value, price, &label, now, sfx)
        {
            return;
        }
        let ed = cust.editor.as_mut().unwrap();
        set_field(&mut ed.build, field, &value);
        close_sub(cust, data, true, now, sfx);
    } else if nav.back {
        close_sub(cust, data, false, now, sfx);
    }
}

/// saveAndClose's purchase, for an armor set (whatever it costs) or a part not
/// bought yet: with too few shards (hasSufficientFunds), the currency sign's
/// warning and its sound (showInsufficientFunds), and nothing more (false: the
/// list stays open). Else the Acquired notice and its sound if it costs anything
/// (showPurchaseAlert, with the page button it was chosen from), and the part
/// bought, or each of the set's parts still locked (purchaseArmorSet, which sounds
/// BUTTON_LOCKED_ITEM for each part already worn, as the game's does); true.
#[allow(clippy::too_many_arguments)]
fn buy(
    cust: &mut Customize,
    data: &CustData,
    field: Field,
    class: usize,
    value: &str,
    price: u32,
    label: &str,
    now: f32,
    sfx: &mut Sfx,
) -> bool {
    if price > cust.progress.shards {
        cust.funds_at = Some(now);
        sfx.play(SND_MONEY_FAIL);
        return false;
    }
    if price > 0 {
        let title = data.string("CustomCharacter.Aquired", "Acquired");
        let category = armor_labels(data, class)[armor_row(field)].clone();
        cust.notices.show(&title, &category, label, price, now);
        sfx.play(SND_MONEY_SUCCESS);
    }
    let ids: Vec<&str> = if field == Field::ArmorSet {
        value.split(',').collect()
    } else {
        vec![value]
    };
    let mut worn = false;
    for id in ids {
        match data.part(id) {
            Some(part) if part.lock(&cust.progress, class).is_some() => {
                cust.progress.buy(&part.id, part.price);
            }
            _ => worn = true,
        }
    }
    if worn && field == Field::ArmorSet {
        sfx.play(SND_LOCKED);
    }
    cust.progress_changed = true;
    true
}

/// The Armor page's buttons' labels (the lists' parentButtonLabel): Head,
/// Chest/Vehicle, Shoulders, Arms, Legs, the class's aux part, Decals, Armor Set.
pub(crate) fn armor_labels(data: &CustData, class: usize) -> [String; 8] {
    let s = |k: &str, f: &str| data.string(&format!("CustomCharacter.{k}"), f);
    let aux = match data.classes[class.min(3)].aux.as_str() {
        "Wings" => s("PartWing", "Wings"),
        "Cannon" => s("PartCannon", "Cannon"),
        _ => s("PartWheel", "Wheels"),
    };
    [
        s("PartHead", "Head"),
        s("PartChest", "Chest/Vehicle"),
        s("PartUpperArm", "Shoulders"),
        s("PartLowerArm", "Arms"),
        s("PartLeg", "Legs"),
        aux,
        s("PartDecal", "Decals"),
        s("PartArmorSet", "Armor Set"),
    ]
}

/// The Armor page's button a part list or the armor sets open from.
fn armor_row(field: Field) -> usize {
    match field {
        Field::Part(3) => 0,
        Field::Part(2) => 1,
        Field::Part(1) => 2,
        Field::Part(0) => 3,
        Field::Part(4) => 4,
        Field::Part(5) => 5,
        Field::Part(_) => 6,
        _ => 7,
    }
}

/// The colour grid commits every change (it doesn't hold its commits like the lists).
fn field_commits_live(field: Field) -> bool {
    matches!(field, Field::Swatch(_))
}

/// closeMenu of the list: accept or go back to the build as it was, restore the
/// camera, and chain to the upgrade list after a weapon or ability.
fn close_sub(cust: &mut Customize, data: &CustData, accept: bool, now: f32, sfx: &mut Sfx) {
    let Some(ed) = cust.editor.as_mut() else {
        return;
    };
    let Some(sub) = ed.sub.take() else {
        return;
    };
    sfx.play(SND_LIST_OUT);
    if !accept {
        // Back to the build as it was (after a weapon or ability was chosen,
        // that choice is part of it).
        ed.build = sub.before.clone();
    } else {
        // Another weapon's or ability's upgrade doesn't carry over (browsing
        // the list leaves it alone, so going back to the same one keeps it).
        let b = &mut ed.build;
        match sub.field {
            Field::Weapon(s) if b.weapons[s] != sub.before.weapons[s] => {
                b.weapon_upgrades[s].clear()
            }
            Field::Ability if b.ability != sub.before.ability => b.ability_upgrade.clear(),
            _ => {}
        }
    }
    if matches!(sub.field, Field::Part(_) | Field::ArmorSet) {
        for p in ed.info.iter_mut().filter(|p| p.leaving.is_none()) {
            p.alpha.to(now, 1.0, 0.4, Ease::RegularOut);
        }
    }
    let front = ed.ring[0];
    let row = ed.row[front];
    pop_row(ed, front, row, now);
    let class = ed.class;
    if sub.camera.is_some() {
        cust.set_view(SPECIALTIES[class], now, false);
    }
    commit(cust);
    if accept {
        let next = match sub.field {
            Field::Weapon(s) => Some(Field::WeaponUpgrade(s)),
            Field::Ability => Some(Field::AbilityUpgrade),
            _ => None,
        };
        if let Some(next) = next {
            open_sub(cust, data, next, now, sfx);
        }
    }
}

/// CharacterData.commit: the build is kept (and saved when the editor closes).
fn commit(cust: &mut Customize) {
    if let Some(ed) = &cust.editor {
        cust.builds[ed.class][ed.slot] = ed.build.clone();
    }
}

/// The editor's closeMenu: back to the class select (the tag flies home).
fn close_editor(cust: &mut Customize, now: f32, sfx: &mut Sfx) {
    if cust.editor.is_none() {
        return;
    }
    // Its robot back on its feet for the line-up (while the editor still says
    // which one).
    to_robot_form(cust, now);
    let Some(ed) = cust.editor.take() else {
        return;
    };
    sfx.play(SND_MENU_OUT);
    cust.builds[ed.class][ed.slot] = ed.build.clone();
    save_builds(&cust.builds);
    cust.set_view("Lineup", now, false);
    cust.yaw = 0.0;
    // UNHIDE_PreviewAll.
    for p in cust.previews.iter_mut() {
        if !p.shown {
            p.shown = true;
            p.shown_at = now;
        }
    }
    sfx.play(SND_DERES);
    // handleFocusIn: every class shows, this one flies back and expands again.
    for c in cust.select.classes.iter_mut() {
        c.hidden = false;
    }
    let c = &mut cust.select.classes[ed.class];
    c.fly.to(now, 0.0, 0.4, Ease::RegularOut);
    expand(c, now);
}

/// Randomize (ResetCharacter, TnCharacterScriptBinding's): a random part of each
/// type the class has (GetRandomTransformerParts: any it can wear as things stand,
/// none it would have to buy or unlock), random colours for both factions
/// (GetRandomColorSwatch) and a random metallicness (FRand); its loadout and voice
/// stay as they are. With the movie's pulse; 0.5 s between goes.
fn randomize(cust: &mut Customize, data: &CustData, now: f32) {
    let Some(class) = cust.editor.as_ref().map(|e| e.class) else {
        return;
    };
    if cust
        .editor
        .as_ref()
        .is_some_and(|e| now < e.randomize_ready)
    {
        return;
    }
    let pick = |n: usize, cust: &mut Customize| {
        ((cust.random() * n as f32) as usize).min(n.saturating_sub(1))
    };
    let mut b = cust.editor.as_ref().unwrap().build.clone();
    for i in 0..b.parts.len() {
        if let Some(id) = data.random_part(class, i, &cust.progress, &mut cust.rng) {
            b.parts[i] = id;
        }
    }
    for f in 0..2 {
        let n = data.swatches[f].len();
        if n > 0 {
            let i = pick(n, cust);
            b.swatches[f] = data.swatches[f][i].id.clone();
        }
    }
    b.metallic = cust.random();
    let ed = cust.editor.as_mut().unwrap();
    ed.build = b;
    ed.randomize_ready = now + 0.5;
    ed.spinner = Some(now);
    commit(cust);
}

/// Change Form: the preview robot folds into its car, or back.
fn change_form(cust: &mut Customize, now: f32) {
    let Some(class) = cust.editor.as_ref().map(|e| e.class) else {
        return;
    };
    let p = &mut cust.previews[class];
    if p.changing.is_some() {
        // Mid-change: it changes back as soon as this one is over.
        p.queued = true;
        return;
    }
    let to_car = p.form == 0;
    p.changing = Change::new(now, to_car);
}

/// Lists with a close-up (and closing the editor) put the robot back on its feet:
/// the editor's robot (or, without the editor, every one).
fn to_robot_form(cust: &mut Customize, now: f32) {
    let classes = match cust.editor.as_ref() {
        Some(ed) => ed.class..ed.class + 1,
        None => 0..4,
    };
    for p in &mut cust.previews[classes] {
        p.queued = false;
        match p.changing {
            // Not folded far enough for the car to show: straight back.
            Some(Change {
                at, to_car: true, ..
            }) if (now - at) * crate::TRANSFORM_SPEED < p.timing.car_show => {
                p.changing = None;
                p.form = 0;
                p.repose = true;
            }
            Some(Change { to_car: true, .. }) => p.changing = Change::new(now, false),
            None if p.form == 1 => p.changing = Change::new(now, false),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Each frame
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn customize_update(
    mut commands: Commands,
    time: Res<Time>,
    data: Option<Res<CustData>>,
    anims: Option<Res<PreviewAnimations>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut link: ResMut<CustomizeLink>,
    mut cust: ResMut<Customize>,
    mut sfx: ResMut<Sfx>,
    chars: Res<crate::character::Characters>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<GarageCamera>>,
    mut visibility: Query<&mut Visibility, Without<GlowOf>>,
    mut glows: Query<(&GlowOf, &mut Visibility)>,
    mut players: Query<&mut AnimationPlayer>,
    car_guns: Query<(Entity, &PreviewCarGun)>,
    mut car_places: Query<&mut Transform, Without<GarageCamera>>,
) {
    let Some(data) = data else {
        return;
    };
    let now = time.elapsed_secs();
    let dt = time.delta_secs();
    let cust = &mut *cust;
    match cust.phase {
        Phase::Closed | Phase::Loading(_) => return,
        Phase::Closing(at) if now - at >= FADE_SECS => {
            close_now(&mut commands, cust, &mut link);
            return;
        }
        _ => {}
    }

    // The camera.
    if let Ok((mut transform, mut projection)) = cameras.single_mut() {
        let class = cust.editor.as_ref().map(|e| e.class);
        let locked = cust.editor.as_ref().is_none_or(|e| e.camera_locked);
        if locked && cust.yaw != 0.0 {
            // Back to the view's own angle.
            cust.yaw *= (1.0 - 6.0 * time.delta_secs()).max(0.0);
            if cust.yaw.abs() < 1e-3 {
                cust.yaw = 0.0;
            }
        }
        let target = view_pose(&data, &cust.view, class, cust.yaw).unwrap_or(cust.pose);
        let pose = match cust.blend_from {
            Some((pos, look, at)) if data.lerp > 0.0 && now - at < data.lerp => {
                let k = ((now - at) / data.lerp).clamp(0.0, 1.0);
                let k = k * k * (3.0 - 2.0 * k);
                (pos.lerp(target.0, k), look.lerp(target.1, k))
            }
            _ => {
                cust.blend_from = None;
                target
            }
        };
        cust.pose = pose;
        *transform = Transform::from_translation(pose.0).looking_at(pose.1, Vec3::Y);
        if let Some(window) = &window
            && let Projection::Perspective(p) = &mut *projection
        {
            // The game's field of view is across the frame.
            let aspect = (window.width() / window.height().max(1.0)).max(0.1);
            let fov = 2.0 * ((data.fov.to_radians() * 0.5).tan() / aspect).atan();
            if (p.fov - fov).abs() > 1e-5 {
                p.fov = fov;
            }
        }
    }

    // The glow over the focused class (FOCUS_<class>In / Out).
    let lit = if cust.editor.is_none() && cust.phase == Phase::Open {
        cust.select.focus
    } else {
        None
    };
    for (of, mut v) in &mut glows {
        let want = if lit == Some(of.0) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        v.set_if_neq(want);
    }

    // The robots: shown and hidden by their de-res (one re-resing keeps showing
    // until its de-res is due), and changing form.
    for (class, p) in cust.previews.iter_mut().enumerate() {
        let Some(root) = p.root else {
            continue;
        };
        let visible = if p.shown {
            now < p.shown_at || now >= p.shown_at + RERES_SHOW_AT
        } else {
            now < p.shown_at + DERES_HIDE_AT
        };
        if let Ok(mut v) = visibility.get_mut(root) {
            v.set_if_neq(if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
        let (Some(anims), Some(robot), Some(car)) = (&anims, p.robot, p.car) else {
            continue;
        };
        // A tank (and a jet) floats once it has come together.
        let target = if p.form == 1 && p.changing.is_none() {
            p.float
        } else {
            0.0
        };
        crate::car::spring(
            &mut p.raise,
            &mut p.raise_vel,
            target,
            crate::car::HOVER_SPRING,
            dt,
        );
        p.raise = p.raise.max(0.0);
        // Changing form, the car among the robot's pieces, the robot carried up from
        // where the car was (car.rs's transform_heights); before its clips start,
        // the car where it was.
        let (car_up, robot_up) = match p.changing {
            Some(c) if c.started => {
                crate::car::transform_heights(&p.timing, c.to_car, now - c.at, p.float, c.from)
            }
            Some(_) => (p.transform_lift, 0.0),
            None => (0.0, 0.0),
        };
        p.transform_lift = car_up;
        for (entity, y) in [(car, p.lift + p.raise + car_up), (robot, robot_up)] {
            if let Ok(mut place) = car_places.get_mut(entity)
                && (place.translation.y - y).abs() > 1e-5
            {
                place.translation.y = y;
            }
        }
        let mut show = |entity: Entity, on: bool| {
            if let Ok(mut v) = visibility.get_mut(entity) {
                v.set_if_neq(if on {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        };
        // Its car's guns (a truck's launchers): hidden, as the game hides the vehicle
        // weapon until it first fires (car.rs's guns_shown), which it never does here.
        for (e, gun) in &car_guns {
            if gun.0 == class {
                show(e, false);
            }
        }
        let timing = p.timing;
        if p.repose {
            p.repose = false;
            if let Some(e) = p.robot_player
                && let Ok(mut player) = players.get_mut(e)
            {
                player.stop_all();
                player.play(anims.robot_nodes[0]).repeat();
            }
            if let Some(e) = p.car_player
                && let Ok(mut player) = players.get_mut(e)
            {
                player.stop_all();
                anims.park_car(p.car_set, &mut player);
            }
            show(robot, true);
            show(car, false);
        }
        let Some(mut change) = p.changing else {
            continue;
        };
        // Clip time.
        let t = (now - change.at) * crate::TRANSFORM_SPEED;
        let (show_robot, show_car, done) = if change.to_car {
            (
                t < timing.robot_hide,
                t >= timing.car_show,
                t >= timing.car_fold,
            )
        } else {
            (
                t >= timing.robot_show,
                t < timing.car_hide,
                t >= timing.robot_unfold,
            )
        };
        // Each clip starts once, as the change begins, and its sounds with it (its
        // armor set's, as in play).
        if !change.started {
            change.started = true;
            let to_car = change.to_car;
            if !to_car {
                // (From where its car is: settling still if it was folding up.)
                change.from = p.raise + p.transform_lift;
                p.raise = 0.0;
                p.raise_vel = 0.0;
            }
            // (Its own, so that the engine its vehicle's transformation starts,
            // Vortex's helicopter's, stops as it turns back into the robot, as in
            // play: main.rs's cut_hover_engines.)
            let owner = PREVIEW_SOUNDS + class as u64;
            if !to_car {
                for wave in crate::hover_waves(&chars, p.vehicle) {
                    sfx.cut(owner, wave);
                }
            }
            crate::transform_sounds(
                &mut sfx,
                &chars,
                p.vehicle,
                to_car,
                crate::TRANSFORM_SPEED,
                None,
                Some(owner),
            );
            // (And its footsteps, the robot on the garage's floor: his landing as he
            // comes out of it, his jump into it.)
            for (t, event) in chars.transform_steps(p.vehicle, to_car) {
                let cue = chars.robot_sound(class, event, "");
                if !cue.is_empty() {
                    sfx.play_owned(owner, cue, t / crate::TRANSFORM_SPEED, 1.0, None);
                }
            }
            // (The half seen as it starts blends in from where it was, below; the
            // other cuts in.)
            if let Some(e) = p.robot_player
                && let Ok(mut player) = players.get_mut(e)
            {
                if !to_car {
                    player.stop_all();
                }
                player
                    .start(anims.robot_transform(p.vehicle, to_car))
                    .set_speed(crate::TRANSFORM_SPEED)
                    .set_weight(if to_car { 0.0 } else { 1.0 });
            }
            if let Some(e) = p.car_player
                && let Ok(mut player) = players.get_mut(e)
                && let Some(clip) = anims.car_node(
                    p.car_set,
                    if to_car { CAR_TO_VEHICLE } else { CAR_TO_ROBOT },
                )
            {
                if to_car {
                    player.stop_all();
                }
                anims.car_guns_away(p.car_set, &mut player);
                player
                    .start(clip)
                    .set_speed(crate::TRANSFORM_SPEED)
                    .set_weight(if to_car { 1.0 } else { 0.0 });
            }
        }
        // The blend-in.
        let since = now - change.at;
        let (from, to) = (
            ((since - dt) / crate::TRANSFORM_BLEND_IN).clamp(0.0, 1.0),
            (since / crate::TRANSFORM_BLEND_IN).clamp(0.0, 1.0),
        );
        if from < 1.0 {
            let (who, clip, keep) = if change.to_car {
                (
                    p.robot_player,
                    Some(anims.robot_transform(p.vehicle, true)),
                    None,
                )
            } else {
                (
                    p.car_player,
                    anims.car_node(p.car_set, CAR_TO_ROBOT),
                    anims.car_node(p.car_set, CAR_GUNS_AWAY),
                )
            };
            if let Some(e) = who
                && let Some(clip) = clip
                && let Ok(mut player) = players.get_mut(e)
            {
                blend_in(&mut player, clip, from, to, keep);
            }
        }
        // Blending into the pose it is left in: the car's drive pose or the
        // robot's customization pose.
        let (settle, (from, to), (start, end)) = if change.to_car {
            (
                p.car_player,
                (
                    anims.car_node(p.car_set, CAR_TO_VEHICLE),
                    anims.car_node(p.car_set, CAR_DRIVE),
                ),
                (timing.car_fold - CAR_SETTLE_SECS, timing.car_fold),
            )
        } else {
            (
                p.robot_player,
                (
                    Some(anims.robot_transform(p.vehicle, false)),
                    Some(anims.robot_nodes[0]),
                ),
                (
                    timing.robot_unfold - ROBOT_LAND.0,
                    timing.robot_unfold - ROBOT_LAND.1,
                ),
            )
        };
        let w = Ease::RegularInOut.apply((t - start) / (end - start));
        if w > 0.0
            && !done
            && let Some(e) = settle
            && let (Some(from), Some(to)) = (from, to)
            && let Ok(mut player) = players.get_mut(e)
        {
            if let Some(clip) = player.animation_mut(from) {
                clip.set_weight(1.0 - w);
            }
            player.play(to).repeat().set_weight(w);
            if change.to_car
                && let Some(bob) = anims.car_bob(p.car_set)
            {
                player.play(bob).repeat().set_weight(w);
            }
        }
        show(robot, show_robot);
        show(car, show_car);
        p.changing = Some(change);
        if done {
            p.changing = None;
            p.form = u8::from(change.to_car);
            // (Its float takes over from where the change left its car.)
            if change.to_car {
                p.raise = (p.raise + p.transform_lift).max(0.0);
                p.raise_vel = 0.0;
            }
            p.transform_lift = 0.0;
            if !change.to_car {
                // Back in the customization pose.
                if let Some(e) = p.robot_player
                    && let Ok(mut player) = players.get_mut(e)
                {
                    player.stop_all();
                    player.play(anims.robot_nodes[0]).repeat();
                }
            } else if let Some(e) = p.car_player
                && let Ok(mut player) = players.get_mut(e)
            {
                anims.park_car(p.car_set, &mut player);
            }
            // Change Form pressed meanwhile: straight on into the change back.
            if std::mem::take(&mut p.queued) {
                p.changing = Change::new(now, !change.to_car);
            }
        }
    }

    // Ambience one-shots.
    if cust.phase == Phase::Open {
        for i in 0..AMBIENCE_SHOTS.len() {
            if now >= cust.ambience_next[i] {
                let (cue, lo, hi) = AMBIENCE_SHOTS[i];
                sfx.play(cue);
                let r = cust.random();
                cust.ambience_next[i] = now + lo + (hi - lo) * r;
            }
        }
    }

    // The editor's info panels and the end of its spinner.
    if let Some(ed) = cust.editor.as_mut() {
        ed.info
            .retain(|p| p.leaving.is_none_or(|at| now - at < 0.6));
        if let Some(due) = ed.info_due
            && now >= due
        {
            ed.info_due = None;
            let front = ed.ring[0];
            if front < 2 {
                let alpha = if ed
                    .sub
                    .as_ref()
                    .is_some_and(|s| matches!(s.field, Field::Part(_) | Field::ArmorSet))
                {
                    0.0
                } else {
                    1.0
                };
                ed.info.push(InfoPanel {
                    page: front,
                    born: now,
                    leaving: None,
                    alpha: Tween::at(alpha),
                    weapon_mode: Tween::at(if ed.row[0] < 4 { 1.0 } else { 0.0 }),
                    described_at: now,
                    described: String::new(),
                });
            }
        }
        // The weapon panel follows the focused row (weapons or ability).
        let weapon_rows = match &ed.sub {
            Some(sub) => !matches!(sub.field, Field::Ability | Field::AbilityUpgrade),
            None => ed.row[0] < 4,
        };
        for p in ed.info.iter_mut().filter(|p| p.page == 0) {
            let goal = if weapon_rows { 1.0 } else { 0.0 };
            if p.weapon_mode.target() != goal {
                p.weapon_mode.to(now, goal, 0.4, Ease::StrongOut);
            }
        }
        if ed.spinner.is_some_and(|at| now - at > 0.7) {
            ed.spinner = None;
        }
    }
}

/// Where the camera is and what it looks at, for view `id` (a class's views are
/// around its robot), turned `yaw` about its subject.
fn view_pose(data: &CustData, id: &str, class: Option<usize>, yaw: f32) -> Option<(Vec3, Vec3)> {
    let view = data.views.get(id)?;
    match *view {
        View::Absolute(pos, dir) => Some((pos, pos + dir * 10.0)),
        View::Orbit(orbit, offset) => {
            let class = class.or_else(|| SPECIALTIES.iter().position(|s| id.starts_with(s)))?;
            let spot = &data.spots[class];
            let f = Vec3::new(spot.facing.x, 0.0, spot.facing.y);
            let r = Vec3::new(-spot.facing.y, 0.0, spot.facing.x);
            let local = |v: Vec3| f * v.x + Vec3::Y * v.y + r * v.z;
            let look = spot.at + local(orbit);
            let arm = Quat::from_rotation_y(yaw) * local(offset);
            Some((look + arm, look))
        }
    }
}

/// A preview robot's gun on his right arm (where the game's preview pawn is given
/// its weapon), shown while it is the weapon being looked at (show_preview_gun).
#[derive(Component)]
struct PreviewGun {
    class: usize,
    id: &'static str,
}

/// An upgrade's part on one of a preview robot's guns (character.rs's UpgradePart),
/// of the class's robot: shown on the gun being looked at while it is the build's
/// upgrade for that weapon (show_preview_upgrades).
#[derive(Component)]
struct PreviewUpgrade(usize);

/// An upgrade's part glowing as it goes on (TnWeaponMesh.UpdateModificationHighlights,
/// ShouldHighlight: chosen in Customize): its own copy of its paint (the part's
/// material, kept to put back), how far the glow has come (LerpVal, 0 to 1) and
/// which way it is going (in, then out).
#[derive(Component)]
struct UpgradeGlow {
    paint: Handle<crate::character::PaintMaterial>,
    glowing: Handle<crate::character::PaintMaterial>,
    lerp: f32,
    rising: bool,
}

/// The glow's times in and out (Default__TnWeaponMesh's ModificationHighlightInTime
/// and ModificationHighlightOutTime), and its sound
/// (WeaponModificatonHighlightSound, played as a glow starts).
const UPGRADE_GLOW_IN: f32 = 0.3;
const UPGRADE_GLOW_OUT: f32 = 5.0;
const SND_UPGRADE_GLOW: &str = "BL_HUD_INTERFACE.WPN_UPGRADE_GLOW";

/// One of a preview car's gun meshes (car.rs's CarGunMesh), of the class's car:
/// hidden, as in a match until the car first fires.
#[derive(Component)]
struct PreviewCarGun(usize);

/// A preview robot's right forearm, or the wheel on it: put away while he holds a
/// gun (the gun is built onto the forearm, as main.rs shows it).
#[derive(Component)]
struct PreviewForearm(usize);

/// Jazz's right forearm and its wheel (main.rs's FOREARMS), as robot.glb has them
/// (without the characters' parts).
const RIGHT_FOREARM: [&str; 2] = ["RB_Jazz_CHARSKEL_ArmLoR", "R_CloudA_ElbowRobo16_XT_Wheel"];

/// robot.glb carries all of Jazz's guns on both arms (main.rs shows the one in
/// hand, in the game); the preview robots hold none but the one being looked at.
/// Nor do they show the copies of the forearms and their wheels on the CloudB arm
/// bones (for switching hands). Each is put away whole, by its node: its meshes
/// (named after it, `RB_HeavyPistolR_SKEL.<material>`) go with it, so the gun
/// shown comes with them, the arm it is built on included.
fn hide_preview_guns(
    mut commands: Commands,
    names: Query<(Entity, &Name, Has<crate::character::WornPart>, Has<Mesh3d>), Added<Name>>,
    parts: Query<(Entity, &crate::character::WornPart), Added<crate::character::WornPart>>,
    upgrade_parts: Query<Entity, Added<crate::character::UpgradePart>>,
    parents: Query<&ChildOf>,
    previews: Query<&PreviewModel>,
) {
    let model_of = |entity: Entity| {
        std::iter::once(entity)
            .chain(parents.iter_ancestors(entity))
            .find_map(|e| previews.get(e).ok())
    };
    // The upgrades' parts on its guns (character.rs), shown with the gun being
    // looked at (show_preview_upgrades).
    for entity in &upgrade_parts {
        if let Some(model) = model_of(entity)
            && !model.car
        {
            commands
                .entity(entity)
                .try_insert(PreviewUpgrade(model.class));
        }
    }
    // A character's parts (character.rs): their right forearm and its wheel.
    for (entity, part) in &parts {
        if let Some(model) = model_of(entity)
            && !model.car
            && part.forearm == Some(false)
            && !part.cloud_b
        {
            commands
                .entity(entity)
                .try_insert(PreviewForearm(model.class));
        }
    }
    if names.is_empty() {
        return;
    }
    let guns = crate::preview_guns();
    for (entity, name, worn, mesh) in &names {
        // (The parts are dressed as their own kind says, above; a node's meshes
        // show or hide with it.)
        if worn || mesh {
            continue;
        }
        let Some(model) = model_of(entity) else {
            continue;
        };
        let n = name.as_str();
        if model.car && (n == "GunL_Mesh" || n == "GunR_Mesh") {
            commands
                .entity(entity)
                .try_insert(PreviewCarGun(model.class));
        }
        let cloud_b = n.ends_with("_CloudB") || (n.contains("_CloudB_") && n.ends_with("_Wheel"));
        if (n.starts_with("RB_") && !n.starts_with("RB_Jazz_")) || cloud_b {
            commands.entity(entity).try_insert(Visibility::Hidden);
            if let Some(&(id, _, _)) = guns.iter().find(|g| g.1 == n).filter(|_| !model.car) {
                commands.entity(entity).try_insert(PreviewGun {
                    class: model.class,
                    id,
                });
            }
        }
        if !model.car && RIGHT_FOREARM.contains(&n) {
            commands
                .entity(entity)
                .try_insert(PreviewForearm(model.class));
        }
    }
}

/// The gun the preview robot being edited holds: the build's weapon while its list
/// is open (the one picked out of it, as it is picked) or its upgrades' list is
/// (CharacterData.previewWeapon; the game gives the pawn it at once:
/// InstantlyGiveWeapon), while he stands in robot form: a class's whose
/// characters the demo has, and of the weapons Jazz has.
fn preview_weapon(cust: &Customize, data: &CustData) -> Option<(usize, String)> {
    let ed = cust.editor.as_ref()?;
    if !data.playable(ed.class) {
        return None;
    }
    let sub = ed.sub.as_ref()?;
    let slot = match sub.field {
        Field::Weapon(s) | Field::WeaponUpgrade(s) => s,
        _ => return None,
    };
    let p = &cust.previews[ed.class];
    if p.form != 0 || p.changing.is_some() {
        return None;
    }
    let id = ed.build.weapons[slot].clone();
    crate::weapon_index(&id).map(|_| (ed.class, id))
}

/// The upgrade on the gun the preview robot being edited holds (preview_weapon):
/// the build's for that weapon (the one picked out of the upgrades' list as it is
/// picked), by class and upgrade (an index into upgrades.rs's UPGRADES).
fn preview_upgrade(cust: &Customize, data: &CustData) -> Option<(usize, usize)> {
    let (class, _) = preview_weapon(cust, data)?;
    let ed = cust.editor.as_ref()?;
    let slot = match ed.sub.as_ref()?.field {
        Field::Weapon(s) | Field::WeaponUpgrade(s) => s,
        _ => return None,
    };
    let upgrade = crate::upgrades::index(&ed.build.weapon_upgrades[slot])?;
    Some((class, upgrade))
}

/// Shows the build's upgrade's part on the preview robot's gun (on its right arm,
/// as he holds it there), as the game installs it on the gun it gives him to look
/// at; one put on while the upgrades' list is open glows in the faction's energon
/// colour as it goes on (ShouldHighlight: ENV_BRDF_WEP_ResIn_MAT's ResInValue, the
/// square root of a lerp up over 0.3 s and back down over 5, ResInColor the
/// character's energon colour) with its sound.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn show_preview_upgrades(
    mut commands: Commands,
    cust: Res<Customize>,
    data: Option<Res<CustData>>,
    time: Res<Time>,
    mut sfx: ResMut<Sfx>,
    mut paints: ResMut<Assets<crate::character::PaintMaterial>>,
    mut parts: Query<(
        Entity,
        &PreviewUpgrade,
        &crate::character::UpgradePart,
        &mut Visibility,
        Option<&MeshMaterial3d<crate::character::PaintMaterial>>,
        Option<&mut UpgradeGlow>,
    )>,
    mut last: Local<[Option<usize>; 4]>,
) {
    let Some(data) = data else {
        return;
    };
    if cust.phase == Phase::Closed {
        *last = [None; 4];
        return;
    }
    let shown = preview_weapon(&cust, &data);
    let upgrade = preview_upgrade(&cust, &data);
    // (A part put on while browsing the upgrades' list glows; one there as the gun
    // comes out doesn't.)
    let browsing = cust
        .editor
        .as_ref()
        .and_then(|ed| ed.sub.as_ref())
        .is_some_and(|sub| matches!(sub.field, Field::WeaponUpgrade(_)));
    let mut glow_starts = [false; 4];
    for (class, prev) in last.iter_mut().enumerate() {
        let now = upgrade.filter(|(c, _)| *c == class).map(|(_, u)| u);
        if now != *prev {
            glow_starts[class] = browsing && now.is_some();
            *prev = now;
        }
    }
    let dt = time.delta_secs();
    let mut sounded = false;
    for (entity, preview, part, mut visibility, paint, glow) in &mut parts {
        let class = preview.0;
        let on = !part.left
            && !part.cloud_b
            && shown.as_ref().is_some_and(|(c, id)| {
                *c == class
                    && crate::WEAPONS
                        .get(part.weapon)
                        .is_some_and(|w| w.id == id.as_str())
            })
            && upgrade == Some((class, part.upgrade));
        visibility.set_if_neq(if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        // Its glow: started afresh as it goes on, run in and out, then its own
        // paint back.
        let starts = on && glow_starts[class];
        match glow {
            Some(mut g) => {
                if starts {
                    g.lerp = 0.0;
                    g.rising = true;
                }
                if g.lerp == 0.0 && g.rising && on && !sounded {
                    sfx.play(SND_UPGRADE_GLOW);
                    sounded = true;
                }
                let secs = if g.rising {
                    UPGRADE_GLOW_IN
                } else {
                    UPGRADE_GLOW_OUT
                };
                let step = dt / secs;
                g.lerp = if g.rising {
                    g.lerp + step
                } else {
                    g.lerp - step
                }
                .clamp(0.0, 1.0);
                if g.lerp >= 1.0 {
                    g.rising = false;
                }
                let value = g.lerp.sqrt();
                if let Some(m) = paints.get_mut(&g.glowing) {
                    m.extension.colors.res_in.w = value;
                }
                if (!g.rising && g.lerp <= 0.0) || !on {
                    commands
                        .entity(entity)
                        .try_insert(MeshMaterial3d(g.paint.clone()))
                        .try_remove::<UpgradeGlow>();
                    paints.remove(&g.glowing);
                }
            }
            None if starts => {
                let Some(paint) = paint.map(|m| m.0.clone()) else {
                    continue;
                };
                let Some(mut copy) = paints.get(&paint).cloned() else {
                    continue;
                };
                let energon = copy.extension.colors.energon.truncate();
                copy.extension.colors.res_in = energon.extend(0.0);
                let glowing = paints.add(copy);
                commands.entity(entity).try_insert((
                    MeshMaterial3d(glowing.clone()),
                    UpgradeGlow {
                        paint,
                        glowing,
                        lerp: 0.0,
                        rising: true,
                    },
                ));
                if !sounded {
                    sfx.play(SND_UPGRADE_GLOW);
                    sounded = true;
                }
            }
            None => {}
        }
    }
}

/// Shows the preview robot's gun (and puts his forearm away), stands him holding
/// it (WEAPON_POSE_BLEND_SECS) and plays its idle motion, as `preview_weapon` says.
#[allow(clippy::too_many_arguments)]
fn show_preview_gun(
    cust: Res<Customize>,
    data: Option<Res<CustData>>,
    time: Res<Time>,
    anims: Option<Res<PreviewAnimations>>,
    mut guns: Query<(&PreviewGun, &mut Visibility), Without<PreviewForearm>>,
    mut forearms: Query<(&PreviewForearm, &mut Visibility), Without<PreviewGun>>,
    mut players: Query<&mut AnimationPlayer>,
    mut armed: Local<[f32; 4]>,
) {
    let Some(data) = data else {
        return;
    };
    if cust.phase == Phase::Closed {
        return;
    }
    let shown = preview_weapon(&cust, &data);
    let held = |class: usize| shown.as_ref().filter(|(c, _)| *c == class);
    for (gun, mut v) in &mut guns {
        let on = held(gun.class).is_some_and(|(_, id)| id == gun.id);
        v.set_if_neq(if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    for (arm, mut v) in &mut forearms {
        v.set_if_neq(if held(arm.0).is_some() {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
    let Some(anims) = anims else {
        return;
    };
    let step = time.delta_secs() / WEAPON_POSE_BLEND_SECS;
    for (class, p) in cust.previews.iter().enumerate() {
        let Some(mut player) = p.robot_player.and_then(|e| players.get_mut(e).ok()) else {
            continue;
        };
        // The stance: from the customization pose to the one holding a gun and
        // back (not while changing form, which plays its own clips and lands in
        // the customization pose).
        let (pose, gun_pose) = (anims.robot_nodes[0], anims.gun_poses[class]);
        if p.changing.is_some() || p.form != 0 {
            armed[class] = 0.0;
        } else {
            let target = if held(class).is_some() { 1.0 } else { 0.0 };
            let w = &mut armed[class];
            *w = if target > *w {
                (*w + step).min(target)
            } else {
                (*w - step).max(target)
            };
            let w = *w;
            if w > 0.0 {
                player.play(gun_pose).repeat().set_weight(w);
            } else if player.animation(gun_pose).is_some() {
                player.stop(gun_pose);
            }
            if let Some(clip) = player.animation_mut(pose) {
                clip.set_weight(1.0 - w);
            }
        }
        for (id, node) in &anims.guns {
            if held(class).is_some_and(|(_, held)| held == id) {
                player.play(*node).repeat().set_weight(1.0);
            } else if player.animation(*node).is_some() {
                player.stop(*node);
            }
        }
    }
}

/// Hooks up the preview robots' animation players: the robot in its pose, the car
/// in its driving pose.
fn attach_preview_animators(
    mut commands: Commands,
    anims: Option<Res<PreviewAnimations>>,
    mut new_players: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    models: Query<(&PreviewModel, Option<&crate::character::CarWorn>)>,
    mut cust: ResMut<Customize>,
) {
    let Some(anims) = anims else {
        return;
    };
    for (entity, mut player) in &mut new_players {
        let mut e = entity;
        let mut model = None;
        for _ in 0..64 {
            if let Ok((m, worn)) = models.get(e) {
                model = Some((m.class, m.car, worn.map_or(0, |w| w.vehicle)));
                break;
            }
            match parents.get(e) {
                Ok(p) => e = p.parent(),
                Err(_) => break,
            }
        }
        let Some((class, car, set)) = model else {
            continue;
        };
        let set = set.min(anims.cars.len().saturating_sub(1));
        let graph = if car {
            let Some((graph, _, _)) = anims.cars.get(set) else {
                continue;
            };
            graph.clone()
        } else {
            anims.robot.clone()
        };
        commands.entity(entity).insert(AnimationGraphHandle(graph));
        if car {
            anims.park_car(set, &mut player);
        } else {
            player.play(anims.robot_nodes[0]).repeat();
        }
        let p = &mut cust.previews[class];
        if car {
            p.car_player = Some(entity);
            p.car_set = set;
        } else {
            p.robot_player = Some(entity);
        }
    }
}

/// The look a class's preview robot wears: the build being edited, or the one
/// its class shows, in the faction's colours (Toggle Faction).
fn preview_look(
    cust: &Customize,
    data: &CustData,
    chars: &crate::character::Characters,
    class: usize,
) -> crate::character::Look {
    let build = match cust.editor.as_ref() {
        Some(ed) if ed.class == class => &ed.build,
        _ => &cust.builds[class][cust.shown_slot(class)],
    };
    build_looks(data, chars, class, build)[cust.faction.min(1)]
}

/// Dresses the preview robots and their cars in their builds' looks as they
/// change (character.rs puts the parts on, the chest's vehicle in).
fn dress_previews(
    mut commands: Commands,
    data: Option<Res<CustData>>,
    chars: Res<crate::character::Characters>,
    mut cust: ResMut<Customize>,
) {
    let Some(data) = data else {
        return;
    };
    if !chars.available || matches!(cust.phase, Phase::Closed) {
        return;
    }
    for class in 0..4 {
        let look = preview_look(&cust, &data, &chars, class);
        let p = &mut cust.previews[class];
        if p.look == Some(look) {
            continue;
        }
        let vehicle_changed = p.look.and_then(|l| chars.vehicle(&l)) != chars.vehicle(&look);
        p.look = Some(look);
        let kit = chars.kit(&look);
        p.vehicle = kit.vehicle;
        p.timing = kit.timing;
        if let Some(robot) = p.robot {
            // (At its class's size: the Destroyer's 0.8.)
            commands.entity(robot).insert((
                crate::character::Wear(look),
                Transform::from_scale(Vec3::splat(kit.robot.scale)),
            ));
        }
        p.lift = kit.lift;
        p.float = if (kit.tank || kit.plane.is_some()) && !kit.legs {
            kit.handling.hover_height.max(0.0)
        } else {
            0.0
        };
        if let Some(car) = p.car {
            // (A walker's model moved to stand on its feet, as car.rs does; a tank's
            // floating: customize_update.)
            commands.entity(car).insert((
                crate::character::CarWear(look),
                Transform::from_xyz(0.0, kit.lift + p.raise, 0.0)
                    .with_scale(Vec3::splat(VEHICLE_SCALE)),
            ));
            // (A new vehicle: its model's own animation player comes with it.)
            if vehicle_changed {
                p.car_player = None;
            }
        }
    }
}

/// The preview robots' glow, as the game colours them on its Customize screen: in
/// their faction's colour, the Autobots' red and the Decepticons' purple (Toggle
/// Faction), whatever the Team Coloring Mode. (TnCharacterApplier.
/// GetEnergonColorForTeam gives the player's own team's cyan with Ally vs Enemy
/// only in a team game or in a lobby, IsInCustomCharacterScreen: the game's
/// Customize screen, as your game shows it, is in neither.)
fn preview_glows(
    mut commands: Commands,
    cust: Res<Customize>,
    glows: Query<&crate::character::GlowOverride>,
) {
    if matches!(cust.phase, Phase::Closed) {
        return;
    }
    for p in &cust.previews {
        let Some(look) = p.look else {
            continue;
        };
        let color = look.energon();
        for model in [p.robot, p.car].into_iter().flatten() {
            if glows.get(model).ok().map(|g| g.0) != Some(color) {
                commands
                    .entity(model)
                    .try_insert(crate::character::GlowOverride(color));
            }
        }
    }
}

/// The voice preview (TnCharacterScriptBinding's PreviewSound): as the Select Voice
/// list moves to a voice the preview robot says that voice's line for a gun kill
/// (TargetKilledByGunDialog), in the set of the side its colours show (Toggle
/// Faction), cutting off the line before; None says nothing.
fn preview_voice(
    mut cust: ResMut<Customize>,
    voices: Res<crate::voice::VoiceData>,
    mut sfx: ResMut<Sfx>,
    mut lines: Local<u64>,
) {
    let Some(voice) = cust.voice_preview.take() else {
        return;
    };
    // (Each line its own owner, so the cut leaves the new one be.)
    sfx.silence(PREVIEW_VOICE + *lines);
    *lines = (*lines + 1) % 1024;
    if let Some(cue) = crate::voice::preview_cue(&voices, &voice, cust.faction == 1) {
        sfx.play_owned(PREVIEW_VOICE + *lines, cue, 0.0, 1.0, None);
    }
}

/// A preview tank's hover jet: its class, its vehicle and its place in the
/// vehicle's effects.
type PreviewJet = (usize, usize, usize);

/// The hover jets of the preview robots' tanks and jets: the game's garage shows
/// them floating on them, as in a match (car.rs's vehicle_effects plays its form's
/// HoverFX on its sockets; here the same on the preview's model, drawn in the
/// garage), from when it has come together until it begins to unfold, what is out
/// of them living out its life.
#[allow(clippy::too_many_arguments)]
fn preview_tank_jets(
    cust: Res<Customize>,
    chars: Res<crate::character::Characters>,
    mut fx: ResMut<crate::levelfx::FxSystems>,
    shown: Query<&InheritedVisibility>,
    bones: Query<(&GlobalTransform, &Name)>,
    children: Query<&Children>,
    mut found: Local<HashMap<(Entity, String), Entity>>,
    mut playing: Local<Vec<(PreviewJet, crate::levelfx::FxId)>>,
) {
    use crate::character::FxWhen;
    let before = std::mem::take(&mut *playing);
    if !matches!(cust.phase, Phase::Open | Phase::Closing(_)) {
        for (_, id) in before {
            fx.kill(id);
        }
        found.clear();
        return;
    }
    for (class, p) in cust.previews.iter().enumerate() {
        let (Some(car), Some(index)) = (p.car, p.vehicle) else {
            continue;
        };
        let Some(vehicle) = chars.vehicles.get(index) else {
            continue;
        };
        // (A car's or a truck's play none in the garage: TnCarForm.Hovering.UpdateFx
        // plays nothing in CustomizationMode.)
        if vehicle.tank.is_none() && vehicle.plane.is_none() {
            continue;
        }
        let floating = p.form == 1 && p.changing.is_none() && shown.get(car).is_ok_and(|v| v.get());
        if !floating {
            continue;
        }
        for (k, e) in vehicle.effects.iter().enumerate() {
            if e.when != FxWhen::Hover {
                continue;
            }
            // The socket's bone in the model (looked up once).
            let named = |b: Entity| bones.get(b).is_ok_and(|(_, n)| n.as_str() == e.bone);
            let key = (car, e.bone.clone());
            let bone = match found.get(&key).copied().filter(|&b| named(b)) {
                Some(b) => Some(b),
                None => {
                    let b = children.iter_descendants(car).find(|&b| named(b));
                    if let Some(b) = b {
                        found.insert(key, b);
                    }
                    b
                }
            };
            let Some((bone, _)) = bone.and_then(|b| bones.get(b).ok()) else {
                continue;
            };
            let (x, y) = crate::orbital::socket_axes(e.rotation);
            let at = bone.transform_point(e.at);
            let x = bone.affine().transform_vector3(x).normalize_or(Vec3::X);
            let y = bone.affine().transform_vector3(y).normalize_or(Vec3::Y);
            let jet = (class, index, k);
            let id = match before.iter().find(|(j, _)| *j == jet) {
                Some(&(_, id)) if fx.alive(id) => Some(id),
                _ => {
                    let id = fx.spawn(&e.system, at, x, crate::car::TANK_FX_COLOR);
                    if let Some(id) = id {
                        // (A jet's burn at their full size here: in play their Size
                        // is how hard each pushes (car.rs's vehicle_effects), but
                        // nothing sets it in the garage, so it stays at its
                        // effect's own (its parameter's Constant, 1), as the game's
                        // garage shows them.)
                        fx.in_garage(id);
                    }
                    id
                }
            };
            if let Some(id) = id {
                fx.place_turned(id, at, x, y);
                playing.push((jet, id));
            }
        }
    }
    for (jet, id) in before {
        if !playing.iter().any(|(j, _)| *j == jet) {
            fx.stop(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The screen shows once everything has loaded, the robots are dressed and no
    /// pipeline has been in the making for a few frames, a moment after; while any
    /// is, it waits, up to LOAD_TIMEOUT_SECS from when it was asked for.
    #[test]
    fn the_screen_shows_once_its_pipelines_are_ready() {
        let asked = 10.0;
        // Not loaded yet: it waits, to the timeout.
        assert!(!ready_to_show(None, 100, asked, asked + 5.0));
        assert!(ready_to_show(None, 0, asked, asked + LOAD_TIMEOUT_SECS));
        // Loaded at 11: a moment more, and the pipelines idle READY_FRAMES frames.
        let at = Some(asked + 1.0);
        let early = asked + 1.0 + SETTLE_SECS * 0.5;
        let settled = asked + 1.0 + SETTLE_SECS + 0.01;
        assert!(!ready_to_show(at, READY_FRAMES, asked, early));
        assert!(!ready_to_show(at, READY_FRAMES - 1, asked, settled));
        assert!(!ready_to_show(at, 0, asked, asked + 6.0));
        assert!(ready_to_show(at, READY_FRAMES, asked, settled));
    }

    /// customize.txt is the copy built into the program: one edited in the assets
    /// folder (every class ability's level set to 0, to have them early) changes
    /// nothing, and Hover Slam still needs level 11.
    #[test]
    fn an_edited_customize_txt_opens_nothing_early() {
        let Ok(points) = std::fs::read_to_string(crate::assets_dir().join(POINTS_FILE)) else {
            assert!(
                std::env::var_os("BEVY_ASSET_ROOT").is_none(),
                "no {POINTS_FILE}"
            );
            return;
        };
        let edited: String = DATA
            .lines()
            .map(|line| {
                let mut f: Vec<&str> = line.split('\t').collect();
                if f[0] == "ability" && f.len() > 3 {
                    f[3] = "0";
                }
                f.join("\t") + "\n"
            })
            .collect();
        assert_ne!(edited.replace('\r', ""), DATA.replace('\r', ""));
        let dir = std::env::temp_dir().join(format!("foc_esc_edited_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("levels")).unwrap();
        std::fs::create_dir_all(dir.join("customize")).unwrap();
        std::fs::write(dir.join(POINTS_FILE), points).unwrap();
        std::fs::write(dir.join("customize/customize.txt"), edited).unwrap();
        let data = CustData::load(&dir);
        std::fs::remove_dir_all(&dir).ok();
        let data = data.expect("the Customize data");
        let hover = CustData::item(&data.abilities, "Hover").expect("Hover Slam");
        assert_eq!(hover.level, 11);
    }

    /// The Customize data the tools exported (None without it: the tests skip,
    /// unless BEVY_ASSET_ROOT says where it is).
    fn data() -> Option<CustData> {
        let d = CustData::load(&crate::assets_dir());
        assert!(
            d.is_some() || std::env::var_os("BEVY_ASSET_ROOT").is_none(),
            "no Customize data in {:?}",
            crate::assets_dir()
        );
        d
    }

    /// A controller reaches every button of the hint bar, on the button its picture
    /// shows: in the editor, the right stick's click locks and unlocks the camera,
    /// the left stick's changes form, Y randomizes, D-pad right cycles the page and
    /// B goes back; at the class select, X toggles the faction and B closes.
    #[test]
    fn a_controller_has_every_hint_on_its_button() {
        let pressing = |button: GamepadButton| {
            let mut pad = Gamepad::default();
            pad.digital_mut().press(button);
            let mut nav = Nav::default();
            pad_nav(&mut nav, &pad, 1.0 / 60.0);
            nav
        };
        let on = |hint: Hint| match hint.pad_button() {
            "Gamepad_X" => GamepadButton::West,
            "Gamepad_Y" => GamepadButton::North,
            "Gamepad_B" => GamepadButton::East,
            "Gamepad_L3" => GamepadButton::LeftThumb,
            "Gamepad_R3" => GamepadButton::RightThumb,
            "Gamepad_DRIGHT" => GamepadButton::DPadRight,
            other => panic!("{hint:?} on {other}"),
        };
        let did = |hint: Hint, nav: &Nav| match hint {
            Hint::ToggleFaction => nav.faction,
            Hint::Close | Hint::Back => nav.back,
            Hint::CameraLock => nav.camera_lock,
            Hint::Randomize => nav.randomize,
            Hint::ChangeForm => nav.change_form,
            Hint::CyclePage => nav.right || nav.cycle_page,
        };
        let all = [
            Hint::ToggleFaction,
            Hint::Close,
            Hint::CameraLock,
            Hint::Randomize,
            Hint::ChangeForm,
            Hint::CyclePage,
            Hint::Back,
        ];
        for hint in all {
            assert!(did(hint, &pressing(on(hint))), "{hint:?}");
            // (And nothing else of the bar's.)
            for other in all {
                if on(other) != on(hint) {
                    assert!(!did(other, &pressing(on(hint))), "{hint:?} does {other:?}");
                }
            }
        }

        // In the editor, as it plays.
        let Some(data) = data() else {
            return;
        };
        let mut cust = Customize::new();
        let mut sfx = Sfx::default();
        open_editor(&mut cust, &data, 0, 0, 0.0, &mut sfx);
        assert!(cust.hints().contains(&Hint::CameraLock));
        let locked = |c: &Customize| c.editor.as_ref().unwrap().camera_locked;
        assert!(locked(&cust));
        editor_input(
            &mut cust,
            &data,
            &pressing(GamepadButton::RightThumb),
            1.0,
            &mut sfx,
        );
        assert!(!locked(&cust));
        // Unlocked, the right stick turns it.
        let mut pad = Gamepad::default();
        pad.analog_mut().set(GamepadAxis::RightStickX, 1.0);
        let mut nav = Nav::default();
        pad_nav(&mut nav, &pad, 0.5);
        editor_input(&mut cust, &data, &nav, 1.1, &mut sfx);
        assert!(cust.yaw > 0.0);
        editor_input(
            &mut cust,
            &data,
            &pressing(GamepadButton::LeftThumb),
            1.2,
            &mut sfx,
        );
        assert!(cust.previews[0].changing.as_ref().is_some_and(|c| c.to_car));
        let page = cust.editor.as_ref().unwrap().ring[0];
        editor_input(
            &mut cust,
            &data,
            &pressing(GamepadButton::DPadRight),
            1.3,
            &mut sfx,
        );
        assert_ne!(cust.editor.as_ref().unwrap().ring[0], page);
        // (B closes the editor, which saves the builds: not here.)
    }

    /// The bar shows the controller's buttons while one is connected and used last:
    /// from the start if one is connected, and after a button or stick, till a key,
    /// a click or the mouse moves.
    #[test]
    fn the_hint_bar_shows_the_buttons_of_whatever_plays() {
        assert!(pad_hints_after(None, true, false, false));
        assert!(!pad_hints_after(None, false, false, false));
        assert!(!pad_hints_after(Some(true), false, true, false));
        assert!(!pad_hints_after(Some(true), true, false, true));
        assert!(!pad_hints_after(Some(false), true, false, false));
        assert!(pad_hints_after(Some(false), true, true, false));
        assert!(pad_hints_after(Some(true), true, false, false));
    }

    #[test]
    fn new_builds_start_with_the_games_loadouts() {
        let Some(data) = data() else {
            return;
        };
        for (class, (weapons, ability, voice)) in STARTING_LOADOUTS.iter().enumerate() {
            let b = Build::new(&data, class, 0, &Default::default(), &mut Rng::new(7));
            assert_eq!(
                b.weapons,
                weapons.map(str::to_string),
                "{}",
                SPECIALTIES[class]
            );
            assert_eq!(b.ability, *ability);
            assert_eq!(b.voice, *voice);
            // (All open from the first level.)
            for w in &b.weapons {
                assert_eq!(CustData::item(&data.weapons, w).map(|i| i.level), Some(0));
            }
        }
    }

    #[test]
    fn locked_options_give_way() {
        let Some(data) = data() else {
            return;
        };
        // An Infiltrator build of the higher levels' (the Nucleon Charge Rifle at 6
        // with Deadeye at 7, the Subsonic Repeater at 15, the EMP grenade at 11).
        let mut b = Build::new(&data, 0, 0, &Default::default(), &mut Rng::new(7));
        b.weapons = ["SniperRifle".into(), "IonBlaster".into()];
        b.weapon_upgrades = [
            "SniperRifleHitsCharge".into(),
            "IonBlasterLargerClip".into(),
        ];
        b.ability = "FlashBangAbility".into();
        // At level 25 nothing changes.
        assert_eq!(b.clone().sanitized(&data, 0, 25).weapons, b.weapons);
        // At 6 the rifle stays but Deadeye goes; the Repeater gives way to the
        // starting secondary (and its upgrade with it); the grenade to the cloak.
        let s = b.clone().sanitized(&data, 0, 6);
        assert_eq!(
            s.weapons,
            ["SniperRifle".to_string(), "HeavyPistol".to_string()]
        );
        assert_eq!(s.weapon_upgrades, [String::new(), String::new()]);
        assert_eq!(s.ability, "Cloaking");
        // At 0: the starting primary too.
        let s = b.sanitized(&data, 0, 0);
        assert_eq!(
            s.weapons,
            ["Shotgun".to_string(), "HeavyPistol".to_string()]
        );
        // An upgrade for the other weapon is no upgrade.
        let mut b = Build::new(&data, 0, 0, &Default::default(), &mut Rng::new(7));
        b.weapon_upgrades[0] = "HeavyPistolSpreadCooldown".into();
        assert_eq!(b.sanitized(&data, 0, 25).weapon_upgrades[0], "");
    }

    /// New builds (and Randomize) wear only parts open as things stand: at the
    /// start, the classes' two free sets' parts and DecalSetNone.
    #[test]
    fn new_builds_wear_free_parts() {
        let Some(data) = data() else {
            return;
        };
        let progress = crate::xp::Progress::default();
        let mut rng = Rng::new(12345);
        for class in 0..4 {
            for _ in 0..40 {
                let b = Build::new(&data, class, 0, &progress, &mut rng);
                for (i, id) in b.parts.iter().enumerate() {
                    let part = data.part(id).unwrap_or_else(|| panic!("{id}"));
                    assert_eq!(part.price, 0, "{id}");
                    assert_eq!(part.lock(&progress, class), None, "{id}");
                    assert!(data.parts_for(class, i).iter().any(|p| p.id == *id), "{id}");
                }
            }
        }
    }

    /// The game's prices: a part as its RequiresCurrency, the DLC's as their
    /// type's; a set as its parts still locked; the bonus heads' locks.
    #[test]
    fn decal_sets_are_sold_to_every_class_and_worn() {
        let Some(data) = data() else {
            return;
        };
        let chars = crate::character::Characters::load(&crate::assets_dir());
        let mut progress = crate::xp::Progress::default();
        for class in 0..4 {
            // DecalSetNone and the eight the game sells, 400 shards each.
            let decals = data.parts_for(class, 6);
            assert_eq!(decals.len(), 9, "class {class}");
            for p in &decals {
                let price = if p.id == "DecalSetNone" { 0 } else { 400 };
                assert_eq!(p.price, price, "{}", p.id);
            }
            // A new build wears none (the sets are bought).
            let mut rng = Rng::new(7);
            for _ in 0..20 {
                let part = data.random_part(class, 6, &progress, &mut rng);
                assert_eq!(part.as_deref(), Some("DecalSetNone"));
            }
        }
        // Bought and worn, a set is the look's (in both factions' colours).
        progress.shards = 400;
        assert!(progress.buy("DecalSet03", 400));
        let mut b = Build::new(&data, 0, 0, &progress, &mut Rng::new(1));
        b.parts[6] = "DecalSet03".into();
        let looks = build_looks(&data, &chars, 0, &b);
        assert_eq!(looks.map(|l| l.decals), [3, 3]);
        b.parts[6] = "DecalSetNone".into();
        assert_eq!(build_looks(&data, &chars, 0, &b)[0].decals, 0);
    }

    #[test]
    fn parts_and_sets_cost_what_the_game_asks() {
        let Some(data) = data() else {
            return;
        };
        let mut progress = crate::xp::Progress::default();
        let set = |id: &str| -> Vec<String> {
            data.armor_sets
                .iter()
                .find(|s| s.0 == id)
                .unwrap()
                .2
                .clone()
        };
        assert_eq!(data.set_price(&set("ScoutArmorSet1"), &progress, 0), 0);
        assert_eq!(data.set_price(&set("ScoutArmorSet6"), &progress, 0), 5400);
        // (Snarl, the DLC's: its wheels are the free Bumblebee's.)
        assert_eq!(data.set_price(&set("ScoutArmorSet13"), &progress, 0), 4900);
        assert_eq!(data.part("ScoutChest7").map(|p| p.price), Some(1500));
        progress.shards = 1000;
        assert!(progress.buy("ScoutHead6", 1000));
        assert!(!progress.buy("ScoutHead6", 1000));
        assert_eq!(progress.shards, 0);
        assert_eq!(data.set_price(&set("ScoutArmorSet6"), &progress, 0), 4400);
        let head = |p: &crate::xp::Progress, id: &str| data.part(id).unwrap().lock(p, 0);
        use crate::xp::Lock;
        assert_eq!(head(&progress, "ScoutHead9"), Some(Lock::MaxLevel));
        assert_eq!(head(&progress, "ScoutHead11"), Some(Lock::Prime));
        assert_eq!(head(&progress, "ScoutHead10"), Some(Lock::PrimeMaxLevel));
        assert_eq!(
            head(&progress, "ScoutHead12"),
            Some(Lock::PrimeAllMaxLevels)
        );
        assert_eq!(
            Lock::PrimeAllMaxLevels.label("Prime").as_deref(),
            Some("PRIME 100")
        );
        progress.xp[0] = 355_000;
        assert_eq!(head(&progress, "ScoutHead9"), None);
        assert_eq!(head(&progress, "ScoutHead10"), Some(Lock::PrimeMaxLevel));
        progress.prime = true;
        assert_eq!(head(&progress, "ScoutHead10"), None);
        assert_eq!(
            head(&progress, "ScoutHead12"),
            Some(Lock::PrimeAllMaxLevels)
        );
        progress.xp = [355_000; 4];
        assert_eq!(head(&progress, "ScoutHead12"), None);
    }

    /// A saved build's parts its class can't wear give way to random ones it can.
    #[test]
    fn locked_parts_give_way() {
        let Some(data) = data() else {
            return;
        };
        let jazz = data
            .armor_sets
            .iter()
            .find(|s| s.0 == "ScoutArmorSet6")
            .unwrap()
            .2
            .clone();
        let mut progress = crate::xp::Progress::default();
        let mut rng = Rng::new(99);
        let mut b = Build::new(&data, 0, 0, &progress, &mut rng);
        b.parts = std::array::from_fn(|i| jazz[i].clone());
        let mut swapped = b.clone();
        assert!(wear_what_can_be_worn(
            &mut swapped,
            &data,
            0,
            &progress,
            &mut rng
        ));
        for (i, id) in swapped.parts.iter().enumerate() {
            assert_eq!(data.part(id).map(|p| p.price), Some(0), "{id}");
            // (DecalSetNone is free: it stays.)
            if i == 6 {
                assert_eq!(id, "DecalSetNone");
            }
        }
        // Bought, they stay.
        for id in &jazz {
            progress.owned.insert(id.clone());
        }
        let mut kept = b.clone();
        assert!(!wear_what_can_be_worn(
            &mut kept, &data, 0, &progress, &mut rng
        ));
        assert_eq!(kept.parts, b.parts);
    }

    /// purchaseAlert_mc's queue: 3 s each, the next 0.2 s after one fades, and
    /// none for the same kind bought again within 3 s.
    #[test]
    fn notices_queue_up() {
        let mut n = Notices::default();
        n.show("Acquired", "Head", "Rogue", 1000, 0.0);
        n.show("Acquired", "Head", "Ghost", 1000, 1.0);
        n.show("Acquired", "Legs", "Rogue", 800, 1.5);
        assert_eq!(
            n.shown.as_ref().map(|s| s.description.as_str()),
            Some("Head : Rogue")
        );
        n.update(2.9);
        assert!(n.fading.is_empty());
        n.update(3.0);
        assert!(n.shown.is_none());
        assert_eq!(n.fading.len(), 1);
        n.update(3.2);
        assert_eq!(
            n.shown.as_ref().map(|s| (s.description.as_str(), s.cost)),
            Some(("Legs : Rogue", 800))
        );
        n.update(3.6);
        assert!(n.fading.is_empty());
        n.update(6.2);
        assert!(n.shown.is_none());
        // (Long enough after: the same kind brings one again.)
        n.show("Acquired", "Head", "Ghost", 1000, 7.0);
        assert!(n.shown.is_some());
    }

    #[test]
    fn weapon_for_skips_the_other_slot() {
        let Some(data) = data() else {
            return;
        };
        // The Titan at level 0 with the Scrapmaker in the other slot: the Energon
        // Harvester (his other open weapon), not the Scrapmaker again.
        assert_eq!(weapon_for(&data, 2, "HeavyMG", "HeavyMG", 0), "DrainGun");
        // Nothing open and free: none.
        assert_eq!(weapon_for(&data, 0, "Nothing", "HeavyPistol", 0), "Shotgun");
    }
}
