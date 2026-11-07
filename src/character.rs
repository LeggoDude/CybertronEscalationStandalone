//! The classes' characters: every armor set, bonus head, wheel and cannon the game
//! offers each class in multiplayer (the Infiltrator's Bumblebee, Breakdown,
//! Sideswipe, Deadend, Jazz, Dragstrip, Wheeljack and Snarl; the Destroyer's Swindle,
//! Optimus Prime, Ratchet, Ironhide, Soundwave, Onslaught, Zeta Prime, G1 Optimus,
//! Ultra Magnus, Hound and Slug; the Titan's Megatron, Warpath, Brawl, Scattershot,
//! Demolisher, Quake, Perceptor, Hardshell and Grimlock; the Scientist's Jetfire,
//! Starscream, Air Raid, Vortex, Silverbolt, Shockwave, Blastoff, Sharpshot, Kickback
//! and Swoop), worn by the robot model and
//! painted in a build's colours, and each chest's vehicle (the Titan's hover tanks
//! and their cannons: [`Tank`]; the Scientist's jets: [`Plane`]).
//!
//! The game builds a character from parts on one skeleton (TR_Skeletons_p.
//! RB_Iconic_SKEL), the one robot.glb has with all of Jazz's clips: so the parts
//! (models/<class>/parts.glb, tools/build_class.py) are skinned to the bones of the
//! robot model they are put on, by name, in place of robot.glb's own Jazz meshes.
//! A class's sixth part hangs on sockets (the game's StaticMeshAttachment on every
//! socket whose name starts with the part's AttachmentSocket): the Infiltrator's
//! and the Destroyer's wheels on the parts' "Wheel" sockets and the vehicle's
//! wheels, the Titan's cannons on its chests' "Cannon" sockets and its tank's, the
//! Scientist's wings on "WingSocket" (two meshes: the robot's stand up on its back,
//! the jet's are its wings). The chest decides the vehicle (its
//! ImpliesAltPath), the vehicle's half of the transformation and the robot's.
//!
//! Every class shares that skeleton, but not its size or all of its moves: a class's
//! robot is drawn at its own size (its robot definition's RobotMeshMergeScale: the
//! Infiltrator 0.6, the Destroyer 0.8) and moves with its own animation sets where
//! they differ from the Infiltrator's (the Destroyer jogs, sprints, dashes, jumps
//! and walks in fine aim with Shared_Truck_ROBO_ANIM's heavy clips, and swings its
//! own sword): its [`Robot`] and its robot_clips.glb, which main.rs's animation graph
//! for the class plays in place of robot.glb's.
//!
//! A look ([`Look`]) is a class, a part of each type, the paint and the faction. The
//! paint is the game's customization: its texture sets keep a mask in Masks_NormY's
//! blue (half for unpainted, more for the primary colour, less for the secondary),
//! and the characters' materials multiply the colour by Cust_Color_A or
//! Cust_COLOR_B as far as the mask is from half. The parts' colour textures carry
//! that mask in their alpha, and [`PaintMaterial`] (assets/shaders/paint.wgsl) does
//! the same.
//!
//! The faction colours the glow. The parts and vehicles glow where their texture
//! sets' Masks_NormY red says (tools/build_class.py's GLOW_COVERAGE: the glow
//! texture's two masks): lights such as Jazz's visor in their own painted colour,
//! and the energon accents (his eyes, wheel rims and leg stripes, Deadend's
//! chevrons) in the faction's EnergonColor, red for the Autobots and purple for the
//! Decepticons ([`AUTOBOT_ENERGON`], [`DECEPTICON_ENERGON`]), pulsing as the game's
//! TR_Energon_MF pulses it. A build's looks come in both (customize.rs's
//! build_looks): a match wears its map's team's (main.rs), the Customize screen
//! the one Toggle Faction shows.
//!
//! The rest of a look (its clips, timings and handling: [`Kit`]) is for main.rs,
//! car.rs and customize.rs, which play them.

use bevy::gltf::{Gltf, GltfMaterialExtras, GltfMaterialName, GltfMesh, GltfNode, GltfSkin};
use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::mesh::skinning::SkinnedMesh;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::scene::{SceneInstance, SceneInstanceReady};
use bevy::shader::ShaderRef;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// The classes, in the game's order (customize.rs's SPECIALTIES: the Scout, Leader,
/// Soldier and Scientist): each one's folder under models/, with its <class>.txt and
/// models (tools/build_class.py).
pub const CLASSES: [&str; 4] = ["infiltrator", "destroyer", "titan", "scientist"];
pub const INFILTRATOR: usize = 0;
const PAINT_SHADER: &str = "shaders/paint.wgsl";
const PAINT_PREPASS_SHADER: &str = "shaders/paint_prepass.wgsl";

/// A class's files.
fn class_file(class: usize, file: &str) -> String {
    format!("models/{}/{file}", CLASSES[class.min(CLASSES.len() - 1)])
}

/// Its data (<class>.txt) and parts.
fn data_file(class: usize) -> String {
    class_file(
        class,
        &format!("{}.txt", CLASSES[class.min(CLASSES.len() - 1)]),
    )
}

fn parts_model(class: usize) -> String {
    class_file(class, "parts.glb")
}

/// The clips a class's parts and vehicles bring, on robot.glb's skeleton, and the
/// same mirrored (for the gun in the left hand).
pub fn clips_model(class: usize) -> String {
    class_file(class, "clips.glb")
}

pub fn clips_mirror_model(class: usize) -> String {
    class_file(class, "clips_mirror.glb")
}

/// A class's own versions of robot.glb's clips ([`Class::robot_clips`]), and mirrored.
pub fn robot_clips_model(class: usize) -> String {
    class_file(class, "robot_clips.glb")
}

pub fn robot_clips_mirror_model(class: usize) -> String {
    class_file(class, "robot_clips_mirror.glb")
}

/// A class's forearm sets' clips for the gun going away and coming back
/// (tools/build_arm_equip.py: each set's ADD_Weapon_Equip_ARM and
/// ADD_Weapon_Unequip_ARM), their ScaleBone notifies, and the clips mirrored.
pub fn arm_equip_model(class: usize) -> String {
    class_file(class, "arm_equip.glb")
}

pub fn arm_equip_mirror_model(class: usize) -> String {
    class_file(class, "arm_equip_mirror.glb")
}

fn arm_equip_file(class: usize) -> String {
    class_file(class, "arm_equip.txt")
}

/// The robot's graph has a slot for each clip a class's clips.glb brings: its
/// vehicles' transformations first (two each), from 0, its forearm and shoulder
/// sets' clips (additive) from TRANSFORM_SLOTS, then its arm_equip.glb's (additive),
/// EXTRA_SLOTS in all, so every class's graph has them in the same places.
pub const TRANSFORM_SLOTS: usize = 32;
pub const EXTRA_SLOTS: usize = 96;

/// A look's part types, in a build's order (customize.rs's PART_TYPES, less the
/// decals the demo doesn't draw).
pub const PART_TYPES: [&str; 6] = ["ArmLo", "ArmUp", "Chest", "Head", "Legs", "Wheels"];
/// The sixth part's type in each class: the cars' and trucks' wheels, the tanks'
/// cannons, the jets' wings (customize.rs's ClassInfo::aux).
const VEHICLE_PARTS: [&str; 3] = ["Wheels", "Cannon", "Wings"];

/// A part type's slot in a look ([`PART_TYPES`]; the sixth for any class's
/// [`VEHICLE_PARTS`]).
fn part_kind(kind: &str) -> Option<usize> {
    if VEHICLE_PARTS.contains(&kind) {
        return Some(WHEELS);
    }
    PART_TYPES.iter().position(|t| *t == kind)
}
pub const ARM_LO: usize = 0;
pub const ARM_UP: usize = 1;
pub const CHEST: usize = 2;
pub const WHEELS: usize = 5;
/// A look's part slot left to the default (its class's first armor set's part).
pub const DEFAULT_PART: u8 = u8::MAX;
/// The robot skeleton's top bone, and the node over it (robot.glb's Root): a
/// robot model is ready to dress once its scene has them.
const SKELETON_ROOT: &str = "C_Root_Reference_XR";
const MODEL_ROOT: &str = "Root";
/// The car skeleton's top bone.
const VEHICLE_ROOT: &str = "C_Reference_XR";

/// What a robot wears: its class (an index into [`CLASSES`]), a part of each type
/// ([`PART_TYPES`]; indices into its class's [`Class::parts`], or
/// [`DEFAULT_PART`]), its decal set, its paint and its faction (the colour of its
/// glow).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Look {
    pub class: u8,
    pub parts: [u8; 6],
    /// Its decal set: its number among decals.rs's sets (1 for the first), 0 for
    /// none.
    pub decals: u8,
    /// The build's colours (its swatch's primary and secondary, the ini's 0-255
    /// linear values) and its metallic slider (0-255).
    pub primary: [u8; 3],
    pub secondary: [u8; 3],
    pub metallic: u8,
    /// Worn as a Decepticon (its energon glowing purple), not an Autobot (red).
    pub decepticon: bool,
}

/// The teams' energon colours (TnFactionTeamAutobots' and TnFactionTeamDecepticons'
/// EnergonColor, linear): what the game gives a multiplayer character's
/// EnergonColor when it colours by faction (TnCharacterApplier.
/// GetEnergonColorFromTeamIndex), in an Escalation match and on the Customize
/// screen alike. (It colours by team instead, the player's own team cyan,
/// MpSameTeamEnergonColor, with Team Coloring Mode on Ally vs Enemy only in a team
/// game or a lobby: ShouldUseMpTeamColoring. Escalation is a single-team game.)
pub const AUTOBOT_ENERGON: Vec3 = Vec3::new(1.25, 0.05, 0.05);
pub const DECEPTICON_ENERGON: Vec3 = Vec3::new(0.2, 0.1, 1.5);
/// The colour of the player's own team where the game colours by team
/// (MpSameTeamEnergonColor).
pub const SAME_TEAM_ENERGON: Vec3 = Vec3::new(0.0, 1.0, 1.0);

impl Look {
    /// The Infiltrator's first armor set (Bumblebee's) in the first Autobot swatch
    /// (AutobotSwatch101): a player's look until his build is known.
    pub const DEFAULT: Look = Look {
        class: INFILTRATOR as u8,
        parts: [DEFAULT_PART; 6],
        decals: 0,
        primary: [240, 240, 247],
        secondary: [255, 0, 0],
        metallic: 0,
        decepticon: false,
    };
    pub const BYTES: usize = 15;
    /// The class byte's bit for the faction.
    const DECEPTICON_BIT: u8 = 0x80;

    pub fn to_bytes(self) -> [u8; Look::BYTES] {
        let mut b = [0u8; Look::BYTES];
        b[..6].copy_from_slice(&self.parts);
        b[6..9].copy_from_slice(&self.primary);
        b[9..12].copy_from_slice(&self.secondary);
        b[12] = self.metallic;
        b[13] = self.class
            | if self.decepticon {
                Look::DECEPTICON_BIT
            } else {
                0
            };
        b[14] = self.decals;
        b
    }

    pub fn from_bytes(b: [u8; Look::BYTES]) -> Look {
        Look {
            class: (b[13] & !Look::DECEPTICON_BIT).min(CLASSES.len() as u8 - 1),
            parts: [b[0], b[1], b[2], b[3], b[4], b[5]],
            decals: b[14],
            primary: [b[6], b[7], b[8]],
            secondary: [b[9], b[10], b[11]],
            metallic: b[12],
            decepticon: b[13] & Look::DECEPTICON_BIT != 0,
        }
    }

    /// Its energon's colour (its faction's).
    pub fn energon(&self) -> Vec3 {
        if self.decepticon {
            DECEPTICON_ENERGON
        } else {
            AUTOBOT_ENERGON
        }
    }

    pub fn class(&self) -> usize {
        (self.class as usize).min(CLASSES.len() - 1)
    }

    fn paint_key(&self) -> [u8; 8] {
        let (p, s) = (self.primary, self.secondary);
        [
            p[0],
            p[1],
            p[2],
            s[0],
            s[1],
            s[2],
            self.metallic,
            u8::from(self.decepticon),
        ]
    }
}

impl Default for Look {
    fn default() -> Self {
        Look::DEFAULT
    }
}

/// A part the game offers (a TnDataProvider_Part; Customize shows its name and
/// unlock from customize.txt).
#[derive(Clone, Debug)]
pub struct Part {
    pub id: String,
    /// Its type: an index into [`PART_TYPES`].
    pub kind: usize,
    pub meshes: Vec<String>,
    /// The mesh it hangs on sockets (a Wheels part's wheel, a Cannon's cannon, a
    /// Wings part's wing), and the start of their names (its AttachmentSocket, lower
    /// case: "wheel", "cannon", "wingsocket").
    pub wheel: Option<String>,
    pub socket: String,
    /// The one its vehicle hangs on its sockets: the same, but for a jet's wings
    /// (the robot's stand up on its back, RB_Airraid_STAT_Wing; the jet's are its
    /// wings, VH_Airraid_STAT_Wing).
    pub vehicle_wheel: Option<String>,
    /// The vehicle it makes (a chest), the forearm set it adds (forearms) and the
    /// shoulder set (upper arms): indices into the lists of [`Characters`].
    pub vehicle: Option<usize>,
    pub arm_set: Option<usize>,
    pub shoulder_set: Option<usize>,
    /// It leaves the sixth part off (Snarl's and Slug's chests: DisablePart Wheels;
    /// Grimlock's: Cannon).
    pub no_wheels: bool,
}

/// An armor set (a TnDataProvider_ArmorSet): a part of each type.
#[derive(Clone, Debug)]
pub struct ArmorSet {
    pub id: String,
    pub parts: Vec<String>,
}

/// A mesh's socket: on a bone, in its space (umodel's glTF axes, metres).
#[derive(Clone, Debug)]
pub struct Socket {
    pub name: String,
    pub bone: String,
    pub transform: Transform,
}

/// When things happen in a transformation, in seconds of its clips as the game
/// plays them (TnAnimNotify_ToggleHidden): the robot folding up vanishes at
/// `robot_hide` and the car appears at `car_show` of its own clip (`car_fold`
/// long); unfolding, the robot appears at `robot_show` of its clip
/// (`robot_unfold` long) and the car vanishes at `car_hide` (`car_unfold`).
/// While both show, the car is drawn `fold_lift` (folding up) or `unfold_lift`
/// (unfolding) metres higher than its clip puts it, among the robot's pieces it
/// comes from or goes into (car.rs's transform_heights; tools/transform_fit.py
/// measures them).
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub robot_hide: f32,
    pub robot_show: f32,
    pub robot_unfold: f32,
    pub car_show: f32,
    pub car_fold: f32,
    pub car_hide: f32,
    pub car_unfold: f32,
    pub fold_lift: f32,
    pub unfold_lift: f32,
}

impl Default for Timing {
    fn default() -> Self {
        Timing::JAZZ
    }
}

impl Timing {
    /// Jazz's, as the demo has timed his clips in robot.glb and vehicle.glb.
    pub const JAZZ: Timing = Timing {
        robot_hide: 1.23,
        robot_show: 0.0,
        robot_unfold: 1.17,
        car_show: 0.906,
        car_fold: 2.167,
        car_hide: 0.499,
        car_unfold: 1.2,
        fold_lift: 0.0,
        unfold_lift: 0.0,
    };
}

/// A class's robot (its robot definition, TR_Custom<...>_ROBODEF): its size and the
/// timings of its own clips.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Robot {
    /// Its parts are drawn at this size (RobotMeshMergeScale).
    pub scale: f32,
    /// Its collision cylinder's height and radius (metres: its CollisionHeight is
    /// half the height, in centimetres).
    pub height: f32,
    pub radius: f32,
    /// Metres it covers per footstep jogging, sprinting and walking in fine aim (its
    /// clips' strides at the demo's speeds: main.rs's footsteps).
    pub jog_stride: f32,
    pub sprint_stride: f32,
    pub aim_stride: f32,
    /// Its jump's take-off (Nav_TakeOff_01) and its melee clip, in seconds.
    pub takeoff_secs: f32,
    pub melee_secs: f32,
}

impl Robot {
    /// Jazz's (the Infiltrator's, as robot.glb has him).
    pub const JAZZ: Robot = Robot {
        scale: 0.6,
        height: 3.1,
        radius: 1.4,
        jog_stride: 4.2,
        sprint_stride: 7.0,
        aim_stride: 2.8,
        takeoff_secs: 0.67,
        melee_secs: 1.10,
    };

    /// The size the game draws the effects it attaches to this robot at: its
    /// RobotMeshMergeScale (the Infiltrator 0.6, the Scientist 0.7, the Destroyer 0.8,
    /// the Titan 0.9). TnTransformer sets the robot mesh's Scale3D to it, and an effect
    /// attached to the mesh takes on the mesh's scale, its particles' places, speeds
    /// and sizes with it: the buffs' effects (TnBuff.PlayEffect attaches ApplyFx,
    /// UnapplyFx and the looping ones to the robot mesh with a RelativeScale of 1: the
    /// Cloak's lightning, the Energon Recharger's and the Intercooler's glows, Speed
    /// Redirector's trail, the flame jets' fire on him, the Heal Beam on the one it
    /// heals) and the Heal Beam's rewards (TnAbilityHealBeamBase plays them on his
    /// mesh's socket). In his vehicle the game asks for the same size
    /// (TnBuff.GetVehicleFxScale: the robot mesh's Scale3D). The game's scripts count
    /// on it: a gun's effects undo their gun's scale
    /// (TnWeaponEffectPlayFXOnSocket.CalculateRelativeScale), and the Cloak's
    /// lightning matches the game's multiplayer videos at it.
    pub fn effect_scale(&self) -> f32 {
        self.scale.max(0.01)
    }
}

/// A class's camera on foot (its camera set's over-the-shoulder strategy,
/// tools/build_class.py's camera_line): how far out it orbits, and where it aims, by
/// the view's pitch: through its three offsets (ahead, side, up; metres from the
/// middle of the robot) from the bottom of its pitch range (degrees) to the middle
/// and on to the top; and the scope's camera (side, up and back from the middle of
/// the robot: the scoped guns', main.rs's WeaponStats::scope_camera).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub distance: f32,
    pub pitch_range: (f32, f32),
    pub offsets: [Vec3; 3],
    pub scope: Vec3,
    /// Its camera sprinting (robot_cam.rs).
    pub sprint: SprintCamera,
}

/// A class's camera while its robot sprints: its over-the-shoulder strategy's
/// entries for the sprint's camera state (TnPCS_Sprinting), metres, degrees and
/// seconds. How far out it orbits (TnLocationOffsetCameraBehavior's OrbitDistance)
/// and the seconds the change takes in and out (PCSTransitionInTime, -OutTime);
/// where it aims (TnScreenSpaceOffsetByPitchCameraBehavior's curve for the state:
/// ahead, side and up, the same at any pitch) and the seconds it eases into it
/// (that curve's SmoothTime) and back out (the default curve's); its field of
/// view and the seconds it eases into it (TnFovCameraBehavior); the view's pitch
/// range and a stick's look speeds (TnOrbitRotationCameraBehavior's LookSpeed:
/// across, up and down, its ramp's multiplier); and its wiggle (TnWiggler3Camera-
/// Behavior's profile: yaw, pitch and roll amplitudes, how many new spots a second
/// each heads for, the seconds a change eases over), the wiggle the rest of the
/// time (its DefaultProfile) and the speed it is full size from (m/s: its
/// FullWiggleSpeedThreshold, VelocityAdjustPercent 1). And the Whirlwind's camera
/// state (TnPCS_Whirlwind, the spin's MeleePCS): how far out it orbits (over the
/// same seconds in and out) and its field of view and the seconds it eases into
/// it; None for a strategy without one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SprintCamera {
    pub distance: f32,
    pub distance_secs: (f32, f32),
    pub offset: Vec3,
    pub offset_secs: (f32, f32),
    pub fov: (f32, f32),
    pub pitch_range: (f32, f32),
    pub look: (f32, f32, f32),
    pub wiggle: ([f32; 3], [f32; 3], f32),
    pub calm: ([f32; 3], [f32; 3], f32),
    pub full_wiggle_speed: f32,
    pub whirl: Option<(f32, (f32, f32))>,
}

impl SprintCamera {
    /// The Infiltrator's (CAM_Strategies_p.Car_MP_OverTheShoulder_STRATEGY: 550 out,
    /// aimed 100 ahead, 180 to the side and 100 up, 90 degrees wide, the view kept
    /// within 25 degrees of level, its wiggle 0.6 degrees each way; its default
    /// wiggle 0.1, full size from 1000 a second).
    pub const CAR: SprintCamera = SprintCamera {
        distance: 5.5,
        distance_secs: (0.5, 0.5),
        offset: Vec3::new(1.0, 1.8, 1.0),
        offset_secs: (0.5, 0.315),
        fov: (90.0, 0.4),
        pitch_range: (-25.0, 25.0),
        look: (20.0, 5.0, 1.5),
        wiggle: ([0.6, 0.6, 0.6], [5.0, 12.0, 8.0], 0.3),
        calm: ([0.1, 0.1, 0.1], [8.0, 18.0, 12.0], 1.0),
        full_wiggle_speed: 10.0,
        whirl: None,
    };
    /// The Destroyer's and the Titan's (Truck_OverTheShoulder_STRATEGY: 800 out from
    /// its 875, aimed 200 ahead, 295 to the side and 150 up; spinning, 1175 out and
    /// 90 degrees wide).
    pub const TRUCK: SprintCamera = SprintCamera {
        distance: 8.0,
        distance_secs: (0.5, 0.55),
        offset: Vec3::new(2.0, 2.95, 1.5),
        offset_secs: (0.5, 0.5),
        fov: (90.0, 0.4),
        pitch_range: (-25.0, 25.0),
        look: (16.0, 5.0, 1.5),
        wiggle: ([0.4, 0.25, 0.4], [12.0, 10.0, 12.0], 0.3),
        calm: ([0.1, 0.1, 0.1], [8.0, 18.0, 12.0], 0.3),
        full_wiggle_speed: 1.0,
        whirl: Some((11.75, (90.0, 0.5))),
    };
    /// The Scientist's (Plane_OverTheShoulder_STRATEGY: 625 out from its 650, aimed
    /// 125 ahead, 245 to the side and 95 up, its pitch range as ever).
    pub const PLANE: SprintCamera = SprintCamera {
        distance: 6.25,
        distance_secs: (0.5, 0.5),
        offset: Vec3::new(1.25, 2.45, 0.95),
        offset_secs: (0.35, 0.5),
        fov: (90.0, 0.4),
        pitch_range: (-60.0, 55.0),
        look: (18.0, 5.0, 1.5),
        wiggle: ([0.6, 0.6, 0.6], [5.0, 12.0, 8.0], 0.3),
        calm: ([0.1, 0.1, 0.1], [8.0, 18.0, 12.0], 0.3),
        full_wiggle_speed: 1.0,
        whirl: None,
    };

    /// A class's (CLASSES' order: its camera set's strategy).
    pub fn of_class(class: usize) -> SprintCamera {
        match CLASSES.get(class).copied() {
            Some("destroyer" | "titan") => SprintCamera::TRUCK,
            Some("scientist") => SprintCamera::PLANE,
            _ => SprintCamera::CAR,
        }
    }
}

impl Camera {
    /// Jazz's, as main.rs has always had it (his own over-the-shoulder camera,
    /// CAM_Strategies_p.OverTheShoulder_STRATEGY: 5.5 m out, aimed 1.65 m to the
    /// side, 1 m ahead and 1.5-1.95 m over the middle of his body, higher as the
    /// view comes up from looking down; the scope's from Car_OverTheShoulder_
    /// STRATEGY's fine aim, 400, 100, 200 at its orbit distance of 575).
    pub const JAZZ: Camera = Camera {
        distance: 5.5,
        pitch_range: (-60.0, 55.0),
        offsets: [
            Vec3::new(1.0, 1.65, 1.5),
            Vec3::new(1.0, 1.65, 1.95),
            Vec3::new(1.0, 1.65, 1.95),
        ],
        scope: Vec3::new(1.0, 2.0, 1.75),
        sprint: SprintCamera::CAR,
    };

    /// Where it aims (ahead, side, up) with the view at a pitch (radians).
    pub fn offset(&self, pitch: f32) -> Vec3 {
        let (lo, hi) = self.pitch_range;
        let t = if hi > lo {
            ((pitch.to_degrees() - lo) / (hi - lo)).clamp(0.0, 1.0)
        } else {
            0.5
        };
        if t < 0.5 {
            self.offsets[0].lerp(self.offsets[1], t * 2.0)
        } else {
            self.offsets[1].lerp(self.offsets[2], t * 2.0 - 1.0)
        }
    }
}

/// A vehicle's camera (Escalation's camera set for its character, or its class's,
/// its driving and hovering strategies, and their own for the camera state its form
/// puts the player in: build_class.py's vehicle_camera_line): how far behind it is
/// and how high over the vehicle's middle it aims with the view level (metres), on
/// its wheels and hovering, and their fields of view (degrees; none from a class
/// built before them: CAMERA_FOV). A car's or truck's follows its strategies in full
/// (`strategies`: driving, hovering, from its vcam lines; vehicle_cam.rs); a tank's
/// and a jet's only these (a jet's fields of view are its vplane line's).
///
/// Every one of them is anchored over the vehicle's middle (`middle`: how high over
/// the ground it stands on and how far ahead of its origin, metres; its vmiddle
/// line, the middle of its mesh's bounds, where the game's pawn is and so where its
/// cameras anchor: build_class.py's vehicle_middle), or a class built before it
/// over half its height (a car's or truck's over its strategies' origin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleCamera {
    pub drive_distance: f32,
    pub drive_up: f32,
    pub hover_distance: f32,
    pub hover_up: f32,
    pub fov: Option<(f32, f32)>,
    pub strategies: Option<[CamStrategy; 2]>,
    pub middle: Option<(f32, f32)>,
}

impl VehicleCamera {
    /// Jazz's car's without the characters' files: its strategies are
    /// [`jazz_car_camera`]'s.
    pub const JAZZ: VehicleCamera = VehicleCamera {
        drive_distance: 5.0,
        drive_up: 1.5,
        hover_distance: 5.0,
        hover_up: 1.5,
        fov: None,
        strategies: None,
        middle: None,
    };
}

/// The camera states a vehicle's camera follows (vehicle_cam.rs puts the player in
/// them; the game's TnPCS_*), as bits.
pub mod cam_state {
    pub const HIGH_SPEED: u8 = 1;
    pub const TURNING_LEFT: u8 = 2;
    pub const TURNING_RIGHT: u8 = 4;
    pub const JUMPING: u8 = 8;
    pub const HOVER_STRAFING: u8 = 16;
    pub const BOOSTING: u8 = 32;
    pub const SUPER_BOOSTING: u8 = 64;

    /// A vcam line's states ("-" for none; HighSpeed,TurningLeft...), or None for
    /// one the demo doesn't know.
    pub fn parse(s: &str) -> Option<u8> {
        if s == "-" {
            return Some(0);
        }
        s.split(',').try_fold(0, |bits, name| {
            Some(
                bits | match name {
                    "HighSpeed" => HIGH_SPEED,
                    "TurningLeft" => TURNING_LEFT,
                    "TurningRight" => TURNING_RIGHT,
                    "Jumping" => JUMPING,
                    "HoverStrafing" => HOVER_STRAFING,
                    "Boosting" => BOOSTING,
                    "SuperBoosting" => SUPER_BOOSTING,
                    _ => return None,
                },
            )
        })
    }
}

/// A camera strategy's values by camera state, in the game's order: the first whose
/// states are all on applies (its default, for none, last).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ByState<T: Copy> {
    list: [(u8, T); 8],
    len: usize,
}

impl<T: Copy + Default> Default for ByState<T> {
    fn default() -> Self {
        ByState {
            list: [(0, T::default()); 8],
            len: 0,
        }
    }
}

impl<T: Copy + Default> ByState<T> {
    fn push(&mut self, states: u8, value: T) {
        if self.len < self.list.len() {
            self.list[self.len] = (states, value);
            self.len += 1;
        }
    }

    /// Its value with these states on (`T::default()` when it has none).
    pub fn get(&self, on: u8) -> T {
        self.list[..self.len]
            .iter()
            .find(|(states, _)| states & on == *states)
            .map_or_else(T::default, |(_, v)| *v)
    }
}

/// A camera strategy's offsets by the view's pitch (TnCameraOffsetCurve: ahead,
/// side and up, metres, in the view's axes), at the bottom, middle and top of its
/// pitch range (degrees: one offset at any pitch when it has none), eased over
/// `secs` when it changes. The game runs a curve through them (HmCurveVector: keys
/// at 0, a half and 1 of the way through the range, Hermite between them, level at
/// the ends and through the middle one at half the slope from the first to the
/// last), not straight lines.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OffsetCurve {
    pub offsets: [Vec3; 3],
    pub pitch: (f32, f32),
    pub secs: f32,
}

impl OffsetCurve {
    /// Its offset with the view at `pitch` (degrees, up), as the game evaluates it
    /// (TnScreenSpaceOffsetByPitchCameraBehavior.GetCameraOffset: UE3's
    /// FInterpCurve, its tangents by the keys' spacing).
    pub fn at(&self, pitch: f32) -> Vec3 {
        let (lo, hi) = self.pitch;
        if hi <= lo {
            return self.offsets[0];
        }
        let t = ((pitch - lo) / (hi - lo)).clamp(0.0, 1.0);
        let [first, middle, last] = self.offsets;
        let slope = (last - first) * 0.25;
        let (p0, p1, m0, m1, a) = if t < 0.5 {
            (first, middle, Vec3::ZERO, slope, t * 2.0)
        } else {
            (middle, last, slope, Vec3::ZERO, t * 2.0 - 1.0)
        };
        let (a2, a3) = (a * a, a * a * a);
        p0 * (2.0 * a3 - 3.0 * a2 + 1.0)
            + m0 * (a3 - 2.0 * a2 + a)
            + p1 * (3.0 * a2 - 2.0 * a3)
            + m1 * (a3 - a2)
    }
}

/// One of a vehicle's camera strategies, its driving or its hovering one
/// (tools/build_class.py's vehicle_cam_lines: the game's HmCameraStrategy and its
/// behaviors), metres, degrees and seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CamStrategy {
    /// How long the camera takes to blend into it (TransitionTime), and its motion
    /// blur (MotionBlur_Amount; 0 for none).
    pub blend: f32,
    pub blur: f32,
    /// Its anchor's height over the vehicle's middle, in the view's axes when
    /// `anchor_turns` (OffsetUsesCameraRotation), and how high that middle is over
    /// the ground the vehicle stands on (VehicleCamera::middle's, which a class built
    /// before it lacks: its rigid body's bone's then).
    pub anchor: f32,
    pub anchor_turns: bool,
    pub origin: f32,
    /// The view's pitch range, how fast its pitch follows the vehicle's
    /// (MatchRotationSpeed: 0 for one the player turns; the driving strategy's yaw is
    /// the car's own, vehicle_cam.rs) and how long its turn is smoothed over (its
    /// orbit smoother's SmoothTime).
    pub pitch_range: (f32, f32),
    pub follow: f32,
    pub smooth: f32,
    /// Its field of view and how long it eases into it, its orbit distance and the
    /// seconds a change of it takes in and out, its offsets, its roll and how long
    /// that eases, and its wiggle (yaw, pitch and roll amplitudes, degrees, how
    /// many new spots a second each heads for, and the seconds a change to it
    /// eases over: vehicle_cam's Wiggler): by camera state.
    pub fov: ByState<(f32, f32)>,
    pub distance: ByState<f32>,
    pub distance_secs: (f32, f32),
    pub curve: ByState<OffsetCurve>,
    pub roll: ByState<(f32, f32)>,
    pub wiggle: ByState<([f32; 3], [f32; 3], f32)>,
}

/// Reads a vcam line (`f`: vcam <form> <drive|hover> <property> <states> <values>)
/// into a vehicle's strategies.
fn read_vcam(strategies: &mut [CamStrategy; 2], f: &[&str]) {
    let s = match f[2] {
        "drive" => &mut strategies[0],
        "hover" => &mut strategies[1],
        _ => return,
    };
    let Some(states) = cam_state::parse(f[4]) else {
        return;
    };
    let v: Vec<f32> = f[5..]
        .iter()
        .map(|x| x.trim().parse().unwrap_or(0.0))
        .collect();
    let at = |i: usize| v.get(i).copied().unwrap_or(0.0);
    match f[3] {
        "blend" => s.blend = at(0),
        "blur" => s.blur = at(0),
        "anchor" => {
            s.anchor = at(0);
            s.anchor_turns = at(1) > 0.5;
        }
        "origin" => s.origin = at(0),
        "pitch" => s.pitch_range = (at(0), at(1)),
        "match" => s.follow = at(0),
        "smooth" => s.smooth = at(0),
        "dist_secs" => s.distance_secs = (at(0), at(1)),
        "fov" => s.fov.push(states, (at(0), at(1))),
        "dist" => s.distance.push(states, at(0)),
        "curve" => {
            let o = |i: usize| Vec3::new(at(i), at(i + 1), at(i + 2));
            s.curve.push(
                states,
                OffsetCurve {
                    offsets: [o(0), o(3), o(6)],
                    pitch: (at(9), at(10)),
                    secs: at(11),
                },
            );
        }
        "roll" => s.roll.push(states, (at(0), at(1))),
        "wiggle" => s.wiggle.push(
            states,
            ([at(0), at(1), at(2)], [at(3), at(4), at(5)], at(6)),
        ),
        _ => {}
    }
}

/// Jazz's car's camera without the characters' files: his class's Car_MP_CAMSET
/// (its Car_MP_New_STRATEGY driving, HoverCar_MP_STRATEGY hovering), as
/// tools/build_class.py's vehicle_cam_lines write it for JazzVehicleForm.
pub fn jazz_car_camera() -> [CamStrategy; 2] {
    static CAMERA: std::sync::OnceLock<[CamStrategy; 2]> = std::sync::OnceLock::new();
    *CAMERA.get_or_init(|| {
        let mut strategies = [CamStrategy::default(); 2];
        for line in JAZZ_CAR_CAMERA.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() >= 6 {
                read_vcam(&mut strategies, &f);
            }
        }
        strategies
    })
}

/// (vcam lines, the form left out as "-".)
const JAZZ_CAR_CAMERA: &str = "\
vcam - drive blend - 2.5
vcam - drive blur - 1
vcam - drive anchor - 1.5 1
vcam - drive origin - 0.49
vcam - drive pitch - -20 25
vcam - drive match - 3
vcam - drive smooth - 0.25
vcam - drive dist_secs - 2.5 4
vcam - drive fov SuperBoosting 100 2.5
vcam - drive fov TurningLeft 95 1
vcam - drive fov TurningRight 95 1
vcam - drive fov - 90 1
vcam - drive dist - 5
vcam - drive curve SuperBoosting 0 0 0 -0.4 0 -0.7 0 0 0 -25 25 1
vcam - drive curve HighSpeed,TurningLeft 0 0 0 -0.15 -0.85 -0.65 0 0 0 -25 25 1
vcam - drive curve HighSpeed,TurningRight 0 0 0 -0.15 0.85 -0.65 0 0 0 -25 25 1
vcam - drive curve HighSpeed 0 0 0 -0.15 0 -0.65 0 0 0 -25 25 1
vcam - drive curve TurningLeft 0 0 0 0.35 -0.75 -0.5 0 0 0 -25 25 1
vcam - drive curve TurningRight 0 0 0 0.35 0.75 -0.5 0 0 0 -25 25 1
vcam - drive curve - 0 0 0 0 0 -0.5 0 0 0 -25 25 1
vcam - drive roll HighSpeed,TurningRight 2.5 0.5
vcam - drive roll HighSpeed,TurningLeft -2.5 0.5
vcam - drive roll TurningRight 2.5 0.5
vcam - drive roll TurningLeft -2.5 0.5
vcam - drive roll - 0 0.5
vcam - drive wiggle Jumping 0.2 0.15 0.2 10 9 12 0.1
vcam - drive wiggle SuperBoosting 0.25 0.2 0.25 10 9 12 0.5
vcam - drive wiggle HighSpeed 0.25 0.2 0.25 12 10 14 0.5
vcam - drive wiggle TurningLeft 0.25 0.25 0.25 10 12 14 0.5
vcam - drive wiggle TurningRight 0.25 0.25 0.25 10 12 14 0.5
vcam - drive wiggle - 0.15 0.1 0.15 10 9 12 0.5
vcam - hover blend - 1.5
vcam - hover blur - 0
vcam - hover anchor - 1.25 1
vcam - hover origin - 0.49
vcam - hover pitch - -20 30
vcam - hover match - 0
vcam - hover smooth - 0.1
vcam - hover dist_secs - 5 5
vcam - hover fov Jumping 80 0.1
vcam - hover fov - 80 0.5
vcam - hover dist HoverStrafing 5.25
vcam - hover dist - 5
vcam - hover curve Jumping 1 0 0 0.1 0 0.15 1 0 0 -25 25 0.5
vcam - hover curve - 0 0 0 0 0 -0.35 0 0 0 -25 30 1
vcam - hover wiggle Jumping 0.125 0.125 0.125 8 12 5 0.25
vcam - hover wiggle HoverStrafing 0.1 0.075 0.1 10 7 8 0.25
vcam - hover wiggle - 0.125 0.1 0.125 8 5 7 0.25
";

/// How a car or truck drives on its wheels beyond its [`Handling`] (its vdrive line:
/// its multiplayer car physics, TnCarSimulation and its TnWheelAssemblies; main.rs's
/// drive_velocity): metres, seconds, radians unless said.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drive {
    /// Its AngularDamping (per second, less as it steers).
    pub angular_damping: f32,
    /// Its front wheels ahead of its centre of mass and its back ones behind, and
    /// the square of its turning radius of gyration (inertia over mass).
    pub ahead: f32,
    pub behind: f32,
    pub gyration: f32,
    /// Its front wheels' steering lock (degrees) and its tyres' grip, front and back
    /// (TireFrictionCoefficient: a share of the wheel's load per m/s it slides
    /// sideways, up to twice its load).
    pub max_steer: f32,
    pub grip_front: f32,
    pub grip_back: f32,
    /// Its lean out of turns at full steering (TurningBankValue, degrees), the
    /// share of the way to it it eases a frame (TurningBankSpeed), and its
    /// suspension's pull and damping against the lean (per s², per s).
    pub bank: f32,
    pub bank_speed: f32,
    pub roll_pull: f32,
    pub roll_damping: f32,
    /// Its air control: turn (rad/s²) and strafe (m/s²) by the steering.
    pub air_turn: f32,
    pub air_strafe: f32,
    /// The camera shakes its jumps and landings start (intensity, roll, roll
    /// falloff, falloff).
    pub jump_shake: [f32; 4],
    pub land_shake: [f32; 4],
}

impl Drive {
    /// Jazz's car's (VEH_SHARED_p.Car_Physics, Escalation's, and JazzVehicleForm's
    /// shakes).
    pub const JAZZ: Drive = Drive {
        angular_damping: 2.0,
        ahead: 0.83,
        behind: 0.93,
        gyration: 0.84667,
        max_steer: 15.0,
        grip_front: 0.17,
        grip_back: 0.2,
        bank: 20.0,
        bank_speed: 0.5,
        roll_pull: 120.54,
        roll_damping: 12.054,
        air_turn: 2.0,
        air_strafe: 7.5,
        jump_shake: [0.175, 0.15, 0.125, 0.15],
        land_shake: [0.275, 0.2, 0.25, 0.275],
    };
}

/// How a car's, truck's or tank's body follows the ground under it (its vground
/// line, tools/build_class.py's ground_line: its rigid body on its springs, as its
/// car and hover physics have them; car.rs's GroundFit): metres, per second
/// squared and per second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ground {
    /// Its wheels (TnWheelAssembly's LocalPosition): across it (to the right) and
    /// ahead of its origin, front left, front right, back left, back right; all
    /// zero for a tank, which has none.
    pub wheels: [Vec2; 4],
    /// The pull and damping of its wheels' springs against its pitch, and against
    /// its rise and fall.
    pub wheel_pitch: (f32, f32),
    pub wheel_rise: (f32, f32),
    /// Its hover's four springs: their distance round its centre of mass
    /// (SuspensionRadius, at 45 degrees off its length), and that centre, across
    /// and ahead.
    pub hover_radius: f32,
    pub hover_centre: Vec2,
    /// Their pull and damping against its pitch, its roll and its rise (zero for a
    /// tank, whose rigid body's mass the game takes from its physics asset: car.rs's
    /// own then).
    pub hover_pitch: (f32, f32),
    pub hover_roll: (f32, f32),
    pub hover_rise: (f32, f32),
    /// The share of its roll its hover holds level a frame on the ground (a hover
    /// car's TnHoverCarSimulation.UpdateTurn: 5%; a tank's none).
    pub hover_hold: f32,
}

impl Ground {
    /// Jazz's car's (Car_Physics' wheels and HoverCar_Physics' springs).
    pub const JAZZ: Ground = Ground {
        wheels: [
            Vec2::new(-0.75, 0.95),
            Vec2::new(0.75, 0.95),
            Vec2::new(-0.75, -0.81),
            Vec2::new(0.75, -0.81),
        ],
        wheel_pitch: (95.131, 9.5131),
        wheel_rise: (80.0, 8.0),
        hover_radius: 1.0,
        hover_centre: Vec2::new(0.0, 0.12),
        hover_pitch: (16.327, 8.1633),
        hover_roll: (28.571, 14.286),
        hover_rise: (21.333, 10.667),
        hover_hold: 0.05,
    };

    /// Where its wheels touch the ground (across, ahead), if it has wheels.
    pub fn wheel_points(&self) -> Option<[Vec2; 4]> {
        self.wheels
            .iter()
            .any(|w| *w != Vec2::ZERO)
            .then_some(self.wheels)
    }

    /// Where its hover's springs reach down (across, ahead): front left, front
    /// right, back left, back right (TnHoverCarSimulation.CalculateSuspensionLocation).
    pub fn hover_points(&self) -> [Vec2; 4] {
        let r = self.hover_radius * std::f32::consts::FRAC_1_SQRT_2;
        let c = self.hover_centre;
        [
            c + Vec2::new(-r, r),
            c + Vec2::new(r, r),
            c + Vec2::new(-r, -r),
            c + Vec2::new(r, -r),
        ]
    }
}

/// How a vehicle drives (its car and hover physics blueprints, tools/build_class.py's
/// handling_line): metres, seconds and radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handling {
    /// On its wheels: top speed, acceleration, the jump's speed up and ahead and its
    /// nose-up turn (rad/s), and gravity's share while it is in the air from a jump.
    pub max_speed: f32,
    pub accel: f32,
    pub jump_speed: f32,
    pub jump_forward: f32,
    pub jump_pitch: f32,
    pub jump_gravity: f32,
    /// Hovering: top speed, acceleration, the jump's speed and nose-up kick, the
    /// drift after the wheels and gravity's share in the air from a jump.
    pub hover_speed: f32,
    pub hover_accel: f32,
    pub hover_jump_speed: f32,
    pub hover_jump_pitch: f32,
    pub drift_secs: f32,
    pub hover_jump_gravity: f32,
    /// How much higher it floats hovering than on its wheels (where its hover
    /// springs hold it against where its wheels' do: build_class.py's hover_rise),
    /// and how far its wheels drop in the air.
    pub hover_height: f32,
    pub wheel_drop: f32,
}

impl Handling {
    /// Jazz's car's in Escalation (Car_Physics, HoverCar_Physics: the second of
    /// each its form lists, as TnSurvivalGame's GetSkillDataIndex picks them; a
    /// multiplayer match picks the first, Car_MP_Physics and HoverCar_MP_Physics).
    pub const JAZZ: Handling = Handling {
        max_speed: 31.0,
        accel: 12.0,
        jump_speed: 15.0,
        jump_forward: 2.0,
        jump_pitch: 0.4,
        jump_gravity: 1.0,
        hover_speed: 15.0,
        hover_accel: 30.0,
        hover_jump_speed: 15.5,
        hover_jump_pitch: 5.0,
        drift_secs: 0.5,
        hover_jump_gravity: 1.0,
        hover_height: 0.015,
        wheel_drop: 0.35,
    };
}

/// A vehicle's body against walls: its half width and how far it reaches ahead of
/// its origin and behind it (metres).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub half_width: f32,
    pub ahead: f32,
    pub behind: f32,
}

/// A vehicle's rigid body as it strikes the world: the box round its physics asset's
/// shapes (tools/build_class.py's vhull: a jet's rotor or wings are no part of it),
/// how far it reaches ahead of its root and behind it, its half width, and its
/// bottom and top over the root, metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hull {
    pub ahead: f32,
    pub behind: f32,
    pub half_width: f32,
    pub bottom: f32,
    pub top: f32,
}

impl Hull {
    /// The spheres its box is tested against the walls as: half as big round as it is
    /// high (no rounder than it is wide), in columns set at most one radius apart
    /// along its length and across its width, in its own frame (x across, z ahead:
    /// -Z forward as the vehicles face).
    pub fn sphere_radius(&self) -> f32 {
        ((self.top - self.bottom) * 0.5).clamp(0.2, self.half_width.max(0.2))
    }

    pub fn columns(&self) -> Vec<Vec2> {
        let r = self.sphere_radius();
        let line = |from: f32, to: f32| -> Vec<f32> {
            if to <= from {
                return vec![(from + to) * 0.5];
            }
            let n = ((to - from) / r).ceil().max(1.0) as usize;
            (0..=n)
                .map(|i| from + (to - from) * i as f32 / n as f32)
                .collect()
        };
        let along = line(-self.behind + r, self.ahead - r);
        let across = line(-self.half_width + r, self.half_width - r);
        along
            .iter()
            .flat_map(|&z| across.iter().map(move |&x| Vec2::new(x, z)))
            .collect()
    }
}

impl Body {
    /// Jazz's car's (vehicle.glb's).
    pub const JAZZ: Body = Body {
        half_width: 0.93,
        ahead: 2.01,
        behind: 1.43,
    };
}

/// A chest's vehicle (its TnCarFormBlueprint or TnTruckFormBlueprint).
#[derive(Clone, Debug)]
pub struct Vehicle {
    pub form: String,
    /// The class it is one of.
    pub class: usize,
    /// Its model (vehicle_<name>.glb: car.rs's clips, in its order).
    pub model: String,
    /// The robot's halves of the transformation (slots of the robot's graph:
    /// [`TRANSFORM_SLOTS`]).
    pub to_vehicle_clip: usize,
    pub to_robot_clip: usize,
    pub timing: Timing,
    /// Its wheels' radius (VisualWheels), its height, and whether it walks on legs
    /// instead (Snarl, Slug): its model is then drawn `lift` higher (lower,
    /// negative) to stand on its feet.
    pub wheel_radius: f32,
    pub height: f32,
    pub legs: bool,
    pub lift: f32,
    pub handling: Handling,
    /// How it drives on its wheels (a car's or truck's vdrive line).
    pub drive: Drive,
    /// How its body follows the ground (its vground line; Jazz's car's for a class's
    /// file without one).
    pub ground: Ground,
    pub body: Body,
    /// Its rigid body's box (None for a class's file without one).
    pub hull: Option<Hull>,
    pub camera: VehicleCamera,
    /// The sockets its class's sixth part hangs on (its wheels, a tank's cannons).
    pub wheel_sockets: Vec<Socket>,
    /// Its decal sockets (a decal set's decals hang on them: decals.rs).
    pub decal_sockets: Vec<Socket>,
    /// A tank's cannon (the Titan's vehicles, Grimlock's dino too).
    pub tank: Option<Tank>,
    /// A jet's flight (the Scientist's vehicles).
    pub plane: Option<Plane>,
    /// It fires rockets from the launchers on its weapon sockets (its class's vehicle
    /// weapon, RocketVehicle: the Destroyer's trucks), its muzzles theirs.
    pub rockets: bool,
    /// Its own effects on their sockets (the game's particle systems: its hover
    /// jets, boosters, jumps and landings).
    pub effects: Vec<VehicleFx>,
    /// A car's or truck's landings are its low ones under this long in the air
    /// (LowLandingAirTimeThreshold: 1 s, Optimus Prime's, Optimus G1's and Ultra
    /// Magnus's 0.25).
    pub low_landing_secs: f32,
    /// Where its effects come from (the car's own space: x right, y up, -z
    /// ahead): the hover jets, the jump jets and the boosters (none for a walker),
    /// and where its guns' shots leave from (left, right, left, right).
    pub hover_jets: Vec<Vec3>,
    pub jump_jets: Vec<Vec3>,
    pub boosters: Vec<Vec3>,
    pub muzzles: Vec<Vec3>,
    /// Its sounds: the cue for each event of its form's sound event set (the game's
    /// event names: Auto_Boost_Start..., and Death).
    sounds: HashMap<String, &'static str>,
    /// The transformation's sounds, to the car [0] and back [1]: (seconds into the
    /// clips as the game plays them, cue).
    transform_sounds: [Vec<(f32, &'static str)>; 2],
    /// Its footsteps, the same way: (seconds, the robot's sound event: his class's
    /// set gives its cue): his landing as he comes out of it, his jump into it.
    transform_steps: [Vec<(f32, String)>; 2],
    /// The sounds its clips play, by clip (car.rs's CAR_*): (seconds into the clip
    /// as it plays, cue). The Dinobots' footsteps as they walk and run, Snarl's and
    /// Slug's tails and growls standing, and their stamps as they jump and land; the
    /// cars' clips play none.
    clip_sounds: HashMap<usize, Vec<(f32, &'static str)>>,
    /// The sound events its audio component plays (its vaudio line), where the
    /// class file names them: of COMPONENT_EVENTS it plays no other (the Dinobots'
    /// plays no engine, jump or landing, and only Grimlock's a boost's start).
    audio_events: Option<Vec<String>>,
}

/// The sound events a vehicle's audio component plays, of those the demo plays for
/// it: its engine's loops, its boost's start, loops and end, its wheels as it
/// boosts, its jumps and its landings, and a jet's climbs and dives and their stops
/// and its hover's start and stop (HmPlayerVehicleAudioComponent's DriveSounds,
/// BoostSound, BoostSounds, BoostWheelsSound, BoostStopSound, BoostJumpSound,
/// AscendSound, HoverLandSound, BoostLandSound, AscendStopSound, DescendSound,
/// DescendStopSound and its loops' one-shots). A vehicle whose
/// component doesn't name one plays none for it, whatever its sound event set has:
/// the Dinobots' VEH_DINOBOT set has tank engines, jumps and landings the game never
/// plays for them; no jet but Vortex sounds its dive's stop, and Kickback no stop
/// of his hover (Sharpshot does).
const COMPONENT_EVENTS: [&str; 18] = [
    SOUND_ENGINE,
    SOUND_BOOST_START,
    SOUND_BOOST_END,
    SOUND_BOOST_LOOP,
    SOUND_BOOST_LOOP_EXHAUST,
    SOUND_BOOST_WHEELS,
    SOUND_HOVER_JUMP,
    SOUND_BOOST_JUMP,
    SOUND_LAND_WHEELS_LIGHT,
    SOUND_LAND_WHEELS_HEAVY,
    SOUND_LAND_HOVER_LIGHT,
    SOUND_LAND_HOVER_HEAVY,
    SOUND_ASCEND,
    SOUND_ASCEND_STOP,
    SOUND_DESCEND,
    SOUND_DESCEND_STOP,
    SOUND_MOVE_OFF,
    SOUND_STOP,
];

/// A tank's cannon (TnTankFormBlueprint): the bone that aims it (CannonBoneName,
/// which TnTankForm.UpdateCannonRotation turns to the view's pitch, the body's
/// yaw), and where its shells leave from (the TankCannon's WeaponSocket_TankCannon,
/// on that bone).
#[derive(Clone, Debug)]
pub struct Tank {
    pub cannon_bone: String,
    pub muzzle: Socket,
}

/// A jet (TnPlaneFormBlueprint: its hover's TnHoverPlaneSimulationBlueprint, its
/// flight's TnPlaneSimulationBlueprint and its class's camera set's hovering and
/// flying strategies; tools/build_class.py's plane_line): how it climbs and dives,
/// its flight's drag and lead into turns, how it leans hovering, and its cameras.
/// Metres, seconds and degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    /// Ascend and Descend (VerticalDashSpeed, VerticalDashDuration: each held frame
    /// keeps it going that much longer), and the dash (DashSpeed, DashDuration).
    pub climb_speed: f32,
    pub climb_secs: f32,
    pub dash_speed: f32,
    pub dash_secs: f32,
    /// Flying: DragCoefficient (Unreal's), on its speed across its nose.
    pub drag: f32,
    /// Hovering: how far it leans into its moves (MotionPhysicsScale).
    pub lean: f32,
    /// Flying, its lead into turns at full pitch and yaw input (PitchDueToPitchValue,
    /// YawDueToYawValue, RollDueToYawValue), the share of it taken in a frame
    /// (ExtraRotationLerpValue), and the pitch of its body over which its pitch lead
    /// fades out (FullPitchThreshold to MaxPitchValue).
    pub pitch_lead: f32,
    pub yaw_lead: f32,
    pub roll_lead: f32,
    pub lead_lerp: f32,
    pub full_pitch: f32,
    pub max_pitch: f32,
    /// How far over the view it hovers pitched (PitchOffsetFromCamera: Vortex's 5).
    pub hover_pitch: f32,
    /// Its cameras' fields of view and the view's pitch range, hovering and flying.
    pub hover_fov: f32,
    pub fly_fov: f32,
    pub hover_pitch_range: (f32, f32),
    pub fly_pitch_range: (f32, f32),
    /// Its flying camera's turn (TnFlyingFollowCameraBehavior): the view's top yaw
    /// speed, acceleration and damping, the same for its pitch (degrees a second),
    /// and its roll per degree a second it yaws (RollDueToYawVelocity).
    pub turn_speed: f32,
    pub turn_accel: f32,
    pub turn_damping: f32,
    pub pitch_speed: f32,
    pub pitch_accel: f32,
    pub pitch_damping: f32,
    pub camera_roll: f32,
    /// Its clips' lengths: taking off into flight (Nav_Hover_to_Boost_VEH), dropping
    /// back to hovering (Nav_Boost_to_Hover_VEH) and the crash (Nav_Collision_VEH).
    pub to_fly_secs: f32,
    pub to_hover_secs: f32,
    pub crash_secs: f32,
}

/// When a vehicle's effect plays: a tank's or a jet's (its form's HoverFX, BoostFx,
/// JumpFX, LandFX), a car's or a truck's (HoverFX, BoostFx, JumpFX and its landings:
/// LowLandingFX and HighLandingFX on its wheels, LowLandingHoverFX and
/// HighLandingHoverFX hovering, the low ones after less than its
/// `low_landing_secs` in the air).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxWhen {
    Hover,
    Boost,
    Jump,
    Land,
    LandLow,
    LandHigh,
    HoverLandLow,
    HoverLandHigh,
}

impl FxWhen {
    /// Played once as it happens (a jump or a landing), not kept going.
    pub fn once(self) -> bool {
        !matches!(self, FxWhen::Hover | FxWhen::Boost)
    }

    /// A landing's (any of them).
    pub fn landing(self) -> bool {
        !matches!(self, FxWhen::Hover | FxWhen::Boost | FxWhen::Jump)
    }
}

/// One of a vehicle's effects: the game's particle system (levelfx.rs's, from
/// fx/weapons_fx.txt), on a socket: its bone, its place there (glTF axes, metres),
/// its rotation (Unreal's rotator) and its scale (Unreal's RelativeScale: hover
/// jets' size goes by it, car.rs's vehicle_effects).
#[derive(Clone, Debug)]
pub struct VehicleFx {
    pub when: FxWhen,
    pub system: String,
    pub bone: String,
    pub at: Vec3,
    pub rotation: [i32; 3],
    pub scale: Vec3,
}

/// Sound events of a vehicle (its form's sound event set) the demo plays.
pub const SOUND_ENGINE: &str = "Auto_Engine_Gear_1_OnLoad";
pub const SOUND_BOOST_START: &str = "Auto_Boost_Start";
pub const SOUND_BOOST_END: &str = "Auto_Boost_End";
/// Its engine while it boosts (HmPlayerVehicleAudioComponent's BoostSounds: its
/// BoostLoops, played by its Boosting state in place of its engine's others), and
/// its wheels on the ground as it starts to boost (BoostWheelsSound).
pub const SOUND_BOOST_LOOP: &str = "Auto_Boost_Loop";
pub const SOUND_BOOST_LOOP_EXHAUST: &str = "Auto_Boost_Loop_Exhaust";
pub const SOUND_BOOST_WHEELS: &str = "Auto_Boost_Wheels";
pub const SOUND_TIRE_SQUEAL: &str = "Auto_Tire_Squeal_Default";
pub const SOUND_IMPACT_HEAVY: &str = "BF_DEFAULT_ROBOT_HEAVY";
pub const SOUND_IMPACT_LIGHT: &str = "BF_DEFAULT_ROBOT_LIGHT";
/// Its jumps: hovering (TnCarForm's Hovering state, a tank always: PlayAscendSound,
/// its audio component's AscendSound, Auto_Jump_Start for every car, truck and tank)
/// and boosting on its wheels (the Driving state: PlayBoostJumpSound, its
/// BoostJumpSound, the class default's Auto_Boost_Jump).
pub const SOUND_HOVER_JUMP: &str = "Auto_Jump_Start";
pub const SOUND_BOOST_JUMP: &str = "Auto_Boost_Jump";
pub const SOUND_LAND_WHEELS_LIGHT: &str = "Auto_Land_Wheels_Light";
pub const SOUND_LAND_WHEELS_HEAVY: &str = "Auto_Land_Wheels_Heavy";
pub const SOUND_LAND_HOVER_LIGHT: &str = "Auto_Land_Hover_Light";
pub const SOUND_LAND_HOVER_HEAVY: &str = "Auto_Land_Hover_Heavy";
pub const SOUND_DEATH: &str = "Death";
/// A jet's: climbing and diving and their stops, and its engine's start as it moves
/// off in its hover and its stop as it comes to rest.
pub const SOUND_ASCEND: &str = "Auto_Engine_Hover_Ascend";
pub const SOUND_ASCEND_STOP: &str = "Auto_Engine_Hover_Ascend_Stop";
pub const SOUND_DESCEND: &str = "Auto_Engine_Hover_Descend";
pub const SOUND_DESCEND_STOP: &str = "Auto_Engine_Hover_Descend_Stop";
pub const SOUND_MOVE_OFF: &str = "Auto_Engine_Gear_1_OnLoad_Oneshot";
pub const SOUND_STOP: &str = "Auto_Engine_Gear_1_OffLoad_Oneshot";
/// Events a set may give no cue, and its own event to play in their place: Snarl's
/// walker has no light crash (only its heavy one).
const SOUND_STAND_INS: [(&str, &str); 1] = [(SOUND_IMPACT_LIGHT, SOUND_IMPACT_HEAVY)];

/// The robot's sound events (its class's sound event set) the demo plays: its
/// footsteps, jumps and landings.
pub const ROBOT_JOG: &str = "FS_DEFAULT_JOG";
pub const ROBOT_RUN: &str = "FS_DEFAULT_RUN";
pub const ROBOT_WALK: &str = "FS_DEFAULT_WALK";
pub const ROBOT_JUMP: &str = "FS_DEFAULT_JUMP";
pub const ROBOT_JUMP_CHARGED: &str = "FS_DEFAULT_JUMP_CHARGED";
pub const ROBOT_LAND: &str = "FS_DEFAULT_LAND";
pub const ROBOT_LAND_HIGH: &str = "FS_DEFAULT_LAND_HIGH_FALL";

/// A cue name for the sound queue, which keeps them for good (the characters'
/// are read once, as the demo starts).
fn cue_name(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

/// A bone's scale as a forearm set's ADD_Weapon_MirrorReverse_ARM sets it
/// (TnAnimNotify_ScaleBone): from `from` to `to` over `ramp` seconds from `time`.
#[derive(Clone, Copy, Debug)]
struct ScaleEvent {
    time: f32,
    from: f32,
    to: f32,
    ramp: f32,
}

impl ScaleEvent {
    fn at(&self, t: f32) -> f32 {
        if self.ramp <= 0.0 {
            return self.to;
        }
        let k = ((t - self.time) / self.ramp).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * k
    }
}

/// A forearm set: its switching-hands clip (a slot of the robot's graph) and the
/// scales of its bones meanwhile; and its clips for the gun going away (the
/// forearm unfolding on the CloudA arm bones: ADD_Weapon_Equip_ARM) and coming back
/// (it folding away on the CloudB ones: ADD_Weapon_Unequip_ARM), slots too, with
/// their bones' scales (arm_equip.glb, tools/build_arm_equip.py).
#[derive(Clone, Debug, Default)]
pub struct ArmSet {
    pub name: String,
    pub clip: usize,
    schedule: HashMap<String, Vec<ScaleEvent>>,
    pub away_clip: Option<usize>,
    pub back_clip: Option<usize>,
    away: HashMap<String, Vec<ScaleEvent>>,
    back: HashMap<String, Vec<ScaleEvent>>,
}

/// A shoulder set: its pads' clips (slots of the robot's graph) while switching
/// hands and switching weapons.
#[derive(Clone, Debug)]
pub struct ShoulderSet {
    pub name: String,
    pub change_hands_clip: usize,
    pub equip_clip: usize,
}

/// What a look brings to the robot: its class, robot and camera, its vehicle and how
/// that drives and is followed, its clips (slots of the robot's graph, after
/// robot.glb's) and the transformation's timing.
#[derive(Clone, Copy, Debug)]
pub struct Kit {
    pub class: usize,
    pub robot: Robot,
    pub camera: Camera,
    pub vehicle: Option<usize>,
    pub to_vehicle: Option<usize>,
    pub to_robot: Option<usize>,
    pub arm_switch: Option<usize>,
    pub shoulder_switch: Option<usize>,
    pub shoulder_equip: Option<usize>,
    /// Its forearm set's clips for the gun going away and coming back
    /// (ArmSet::away_clip, back_clip).
    pub arm_away: Option<usize>,
    pub arm_back: Option<usize>,
    pub arm_set: Option<usize>,
    pub timing: Timing,
    /// The vehicle's wheel radius and height, and whether it walks (drawn `lift`
    /// higher, to stand on its feet).
    pub wheel_radius: f32,
    pub vehicle_height: f32,
    pub legs: bool,
    pub lift: f32,
    pub handling: Handling,
    pub drive: Drive,
    /// How its vehicle's body follows the ground (Vehicle::ground).
    pub ground: Ground,
    pub body: Body,
    /// Its vehicle's rigid body's box (Vehicle::hull), which a jet and a tank meet
    /// the walls with.
    pub hull: Option<Hull>,
    pub vehicle_camera: VehicleCamera,
    /// Its vehicle is a tank (the Titan's): it only hovers, boosts straight ahead and
    /// fires its cannon (Vehicle::tank).
    pub tank: bool,
    /// Its vehicle is a jet (the Scientist's): it hovers and flies, without gravity,
    /// and fires homing rockets (Vehicle::plane).
    pub plane: Option<Plane>,
    /// Its vehicle fires rockets from launchers (the Destroyer's trucks:
    /// Vehicle::rockets).
    pub rockets: bool,
}

impl Kit {
    /// Without the characters' files: Jazz's own clips and timing.
    pub const JAZZ: Kit = Kit {
        class: INFILTRATOR,
        robot: Robot::JAZZ,
        camera: Camera::JAZZ,
        vehicle: None,
        to_vehicle: None,
        to_robot: None,
        arm_switch: None,
        shoulder_switch: None,
        shoulder_equip: None,
        arm_away: None,
        arm_back: None,
        arm_set: None,
        timing: Timing::JAZZ,
        wheel_radius: 0.3,
        vehicle_height: 1.1,
        legs: false,
        lift: 0.0,
        handling: Handling::JAZZ,
        drive: Drive::JAZZ,
        ground: Ground::JAZZ,
        body: Body::JAZZ,
        hull: None,
        vehicle_camera: VehicleCamera::JAZZ,
        tank: false,
        plane: None,
        rockets: false,
    };
}

impl Kit {
    /// Its vehicle is a car or a truck (TnCarForm: hovering, and driving on its
    /// wheels as drive.rs has it), not a tank or a jet.
    pub fn wheeled(&self) -> bool {
        !self.tank && self.plane.is_none()
    }
}

impl Default for Kit {
    fn default() -> Self {
        Kit::JAZZ
    }
}

/// A class's characters (its <class>.txt and parts model).
#[derive(Default)]
pub struct Class {
    pub parts: Vec<Part>,
    pub sets: Vec<ArmorSet>,
    sockets: HashMap<String, Vec<Socket>>,
    /// The default set's part of each type.
    default_parts: [Option<usize>; 6],
    pub robot: Option<Robot>,
    /// Its camera on foot (Jazz's where its file gives none).
    pub camera: Option<Camera>,
    /// Its robot's sounds: its sound event set's cue for each event (ROBOT_*).
    sounds: HashMap<String, &'static str>,
    /// robot.glb's clips it has its own of: (robot.glb's clip, robot_clips.glb's).
    pub robot_clips: Vec<(usize, usize)>,
    /// The sounds those play, by robot.glb's clip: (seconds in, cue).
    robot_clip_sounds: HashMap<usize, Vec<(f32, &'static str)>>,
    /// Its own idle holding a gun, for Customize's preview (a robot_clips.glb clip;
    /// robot_cust.glb's, the Scout's, without).
    pub cust_weapon_clip: Option<usize>,
    /// How many clips its clips.glb has, and the first additive one (its
    /// vehicles' transformations come first, two each).
    pub clip_count: usize,
    pub first_additive_clip: usize,
    /// How many clips its arm_equip.glb has (their slots follow clips.glb's).
    pub equip_clip_count: usize,
    pub gltf: Handle<Gltf>,
    /// Its files are there (built by tools/build_class.py).
    pub available: bool,
}

impl Class {
    /// The slot of the robot's graph a clip of its clips.glb plays in.
    pub fn slot(&self, clip: usize) -> usize {
        if clip < self.first_additive_clip {
            clip
        } else {
            TRANSFORM_SLOTS + clip - self.first_additive_clip
        }
    }

    /// The clip of its clips.glb a slot of the robot's graph plays, if any.
    pub fn clip_in_slot(&self, slot: usize) -> Option<usize> {
        let clip = if slot < TRANSFORM_SLOTS {
            (slot < self.first_additive_clip).then_some(slot)?
        } else {
            self.first_additive_clip + slot - TRANSFORM_SLOTS
        };
        (clip < self.clip_count).then_some(clip)
    }

    /// The slot of the robot's graph a clip of its arm_equip.glb plays in: after
    /// its clips.glb's.
    pub fn equip_slot(&self, clip: usize) -> usize {
        TRANSFORM_SLOTS + self.clip_count.saturating_sub(self.first_additive_clip) + clip
    }

    /// The clip of its arm_equip.glb a slot plays, if any.
    pub fn equip_clip_in_slot(&self, slot: usize) -> Option<usize> {
        let first = self.equip_slot(0);
        (slot >= first && slot - first < self.equip_clip_count).then(|| slot - first)
    }
}

/// Every class's characters ([`CLASSES`]), and what they share: the vehicles, the
/// forearm and shoulder sets (each class's, one list), and robot.glb's painted guns.
#[derive(Resource, Default)]
pub struct Characters {
    pub classes: Vec<Class>,
    pub vehicles: Vec<Vehicle>,
    pub arm_sets: Vec<ArmSet>,
    pub shoulder_sets: Vec<ShoulderSet>,
    /// Every bone a forearm set's switch scales.
    switch_bones: HashSet<String>,
    /// Every vehicle's file, kept loaded from the start: the models are taken from
    /// them (dress_cars). Asked for by its scene's label instead, a file that isn't
    /// in is loaded once more for each asking, and Bevy then puts every model of it
    /// in again (losing its paint), or fails when it has been let go meanwhile.
    vehicle_files: Vec<Handle<Gltf>>,
    /// robot.glb's guns and the arm built into them, painted too (the game puts
    /// the character's colours on the weapon in hand: TnCharacterApplier.
    /// ApplyColorsToCharacter): each material there, its colour texture with the
    /// paint mask, and that texture once loaded.
    gun_paint: Vec<(String, String)>,
    gun_paint_textures: Vec<Handle<Image>>,
    /// The weapon upgrades' parts (models/weapons/upgrades.txt and .glb, made by
    /// tools/build_upgrades.py): where each upgrade's part goes on its gun's right-arm
    /// and left-arm meshes, and the parts' model.
    pub upgrade_parts: Vec<UpgradePlacement>,
    upgrades_gltf: Handle<Gltf>,
    /// The Infiltrator's characters are there (Jazz wears robot.glb's own parts
    /// without them).
    pub available: bool,
    /// The decal sets (decals.rs), every class's.
    pub decals: crate::decals::DecalData,
}

fn floats<const N: usize>(s: &str) -> Option<[f32; N]> {
    let v: Vec<f32> = s
        .split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect();
    (v.len() == N).then(|| std::array::from_fn(|i| v[i]))
}

fn socket(f: &[&str]) -> Option<Socket> {
    let t = floats::<3>(f[4])?;
    let q = floats::<4>(f[5])?;
    let s = floats::<3>(f[6])?;
    Some(Socket {
        name: f[2].to_string(),
        bone: f[3].to_string(),
        transform: Transform {
            translation: Vec3::from(t),
            rotation: Quat::from_array(q).normalize(),
            scale: Vec3::from(s),
        },
    })
}

/// The weapon upgrades' parts' model and where they go (tools/build_upgrades.py).
const UPGRADES_MODEL: &str = "models/weapons/upgrades.glb";
const UPGRADES_FILE: &str = "models/weapons/upgrades.txt";

/// Where an upgrade's part goes on a gun (TnWeaponMesh.InstallMeshAttachmentModification:
/// its StaticMeshAttachment on the gun's weapon mesh's MeshAttachmentSocket): the
/// upgrade (an index into upgrades.rs's UPGRADES), the gun (an index into main.rs's
/// WEAPONS) and its arm (left: true), the part's mesh (a node of upgrades.glb) and the
/// socket's bone and place on it (umodel's glTF axes, metres; often mirrored).
#[derive(Clone, Debug)]
pub struct UpgradePlacement {
    pub upgrade: usize,
    pub weapon: usize,
    pub left: bool,
    pub part: String,
    pub bone: String,
    pub transform: Transform,
}

/// An upgrade's part as spawned onto a robot model (hidden: main.rs's
/// show_upgrade_parts and customize.rs show it while its gun shows and the upgrade
/// is on it): which upgrade, on which gun and arm, and whether it is the copy on the
/// CloudB bones (the gun folding away rides those).
#[derive(Component, Clone, Copy, Debug)]
pub struct UpgradePart {
    pub upgrade: usize,
    pub weapon: usize,
    pub left: bool,
    pub cloud_b: bool,
}

/// models/weapons/upgrades.txt's lines (`part <upgrade> <weapon mesh> <part mesh>
/// <bone> <t> <q> <s>`), for the demo's upgrades and guns.
fn upgrade_placements(assets: &Path) -> Vec<UpgradePlacement> {
    let Ok(text) = std::fs::read_to_string(assets.join(UPGRADES_FILE)) else {
        info!("{UPGRADES_FILE} not found (tools/build_upgrades.py makes it): no upgrade parts");
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 8 || f[0] != "part" {
            continue;
        }
        let Some(upgrade) = crate::upgrades::index(f[1]) else {
            continue;
        };
        let Some((weapon, left)) = crate::WEAPONS.iter().enumerate().find_map(|(i, w)| {
            (w.mesh == f[2])
                .then_some((i, false))
                .or((w.mesh_left == f[2]).then_some((i, true)))
        }) else {
            continue;
        };
        let (Some(t), Some(q), Some(sc)) =
            (floats::<3>(f[5]), floats::<4>(f[6]), floats::<3>(f[7]))
        else {
            continue;
        };
        out.push(UpgradePlacement {
            upgrade,
            weapon,
            left,
            part: f[3].to_string(),
            bone: f[4].to_string(),
            transform: Transform {
                translation: Vec3::from(t),
                rotation: Quat::from_array(q).normalize(),
                scale: Vec3::from(sc),
            },
        });
    }
    out
}

/// Whether a class's characters' files are there (tools/build_class.py makes them):
/// its data, parts and clips (Characters::load checks the rest).
pub fn class_files_present(assets: &Path, class: usize) -> bool {
    class < CLASSES.len()
        && [data_file(class), parts_model(class), clips_model(class)]
            .iter()
            .all(|f| assets.join(f).exists())
}

/// Whether the Infiltrator's characters are there (Jazz wears robot.glb's own
/// parts without them).
static AVAILABLE: AtomicBool = AtomicBool::new(false);

pub fn available() -> bool {
    AVAILABLE.load(Ordering::Relaxed)
}

/// The bone on the other side (L_* and R_* swap; C_* bones are their own).
pub fn partner(name: &str) -> String {
    if let Some(rest) = name.strip_prefix("L_") {
        format!("R_{rest}")
    } else if let Some(rest) = name.strip_prefix("R_") {
        format!("L_{rest}")
    } else {
        name.to_string()
    }
}

impl Characters {
    /// The models the characters are made from (the classes' parts, the vehicles
    /// and the weapon upgrades' parts), for what waits for them to load.
    pub fn models(&self) -> impl Iterator<Item = bevy::asset::UntypedAssetId> + '_ {
        self.classes
            .iter()
            .filter(|c| c.available)
            .map(|c| c.gltf.id().untyped())
            .chain(self.vehicle_files.iter().map(|h| h.id().untyped()))
            .chain((!self.upgrade_parts.is_empty()).then(|| self.upgrades_gltf.id().untyped()))
    }

    /// Reads every class's <class>.txt (a class without one: none of its own, and
    /// unavailable).
    pub fn load(assets: &Path) -> Characters {
        let mut c = Characters::default();
        for class in 0..CLASSES.len() {
            let class_data = c.load_class(assets, class);
            c.classes.push(class_data);
        }
        c.available = c.classes[INFILTRATOR].available;
        c.upgrade_parts = upgrade_placements(assets);
        c.decals = crate::decals::DecalData::load(assets);
        if !c.available {
            warn!("The Infiltrator's characters are incomplete: Jazz wears robot.glb's own parts");
        }
        AVAILABLE.store(c.available, Ordering::Relaxed);
        c
    }

    /// One class's file: its own data, and its vehicles, forearm and shoulder sets
    /// added to the shared lists.
    fn load_class(&mut self, assets: &Path, class: usize) -> Class {
        let mut k = Class::default();
        let file = data_file(class);
        let Ok(text) = std::fs::read_to_string(assets.join(&file)) else {
            info!("{file} not found (tools/build_class.py makes it)");
            return k;
        };
        let num = |s: &str| s.trim().parse::<f32>().unwrap_or(0.0);
        let idx = |s: &str| s.trim().parse::<usize>().unwrap_or(0);
        // This class's vehicles and sets go after those read so far.
        let first_vehicle = self.vehicles.len();
        let first_arm = self.arm_sets.len();
        let first_shoulder = self.shoulder_sets.len();
        // Parts name their vehicle and sets by name: resolved once all are read.
        let mut raw_parts = Vec::new();
        let mut vsockets: Vec<(String, Socket)> = Vec::new();
        let mut vdecals: Vec<(String, Socket)> = Vec::new();
        let mut tanks: Vec<(String, Tank)> = Vec::new();
        let mut planes: Vec<(String, Plane)> = Vec::new();
        let mut effects: Vec<(String, VehicleFx)> = Vec::new();
        let mut low_landings: Vec<(String, f32)> = Vec::new();
        let mut vpoints: Vec<(String, String, Vec3)> = Vec::new();
        let mut vsounds: Vec<(String, String, String)> = Vec::new();
        let mut vtsounds: Vec<(String, bool, f32, String)> = Vec::new();
        let mut vcsounds: Vec<(String, usize, f32, String)> = Vec::new();
        let mut vaudio: Vec<(String, Vec<String>)> = Vec::new();
        let mut vtsteps: Vec<(String, bool, f32, String)> = Vec::new();
        let mut handlings: Vec<(String, Handling)> = Vec::new();
        let mut transform_lifts: Vec<(String, f32, f32)> = Vec::new();
        let mut bodies: Vec<(String, Body)> = Vec::new();
        let mut hulls: Vec<(String, Hull)> = Vec::new();
        let mut cameras: Vec<(String, VehicleCamera)> = Vec::new();
        let mut middles: Vec<(String, (f32, f32))> = Vec::new();
        let mut drives: Vec<(String, Drive)> = Vec::new();
        let mut grounds: Vec<(String, Ground)> = Vec::new();
        let mut strategies: HashMap<String, [CamStrategy; 2]> = HashMap::new();
        let mut switches: Vec<(String, String, ScaleEvent)> = Vec::new();
        // (Clip numbers are clips.glb's: made slots once all are counted.)
        let mut vehicles: Vec<Vehicle> = Vec::new();
        // The weapons its vehicles carry (TransWeapons.ini's unique ids).
        let mut vehicle_weapons: Vec<String> = Vec::new();
        let mut arm_sets: Vec<ArmSet> = Vec::new();
        let mut shoulder_sets: Vec<ShoulderSet> = Vec::new();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            match (f[0], f.len()) {
                // class <name> <scale> <half height> <radius> <strides x3> <take-off> <melee>
                ("class", 10) => {
                    k.robot = Some(Robot {
                        scale: num(f[2]),
                        height: 2.0 * num(f[3]) * 0.01,
                        radius: num(f[4]) * 0.01,
                        jog_stride: num(f[5]),
                        sprint_stride: num(f[6]),
                        aim_stride: num(f[7]),
                        takeoff_secs: num(f[8]),
                        melee_secs: num(f[9]),
                    })
                }
                // camera <distance> <pitch min> <pitch max> <offsets x3> <scope>
                ("camera", 8) => {
                    let v = |s: &str| floats::<3>(s).map(Vec3::from).unwrap_or_default();
                    k.camera = Some(Camera {
                        distance: num(f[1]),
                        pitch_range: (num(f[2]), num(f[3])),
                        offsets: [v(f[4]), v(f[5]), v(f[6])],
                        scope: v(f[7]),
                        sprint: SprintCamera::of_class(class),
                    });
                }
                ("csound", 3) => {
                    k.sounds.insert(f[1].to_string(), cue_name(f[2]));
                }
                ("vweapon", 2) => vehicle_weapons.push(f[1].to_string()),
                ("robotclip", 3) => k.robot_clips.push((idx(f[1]), idx(f[2]))),
                ("rcsound", 4) => k
                    .robot_clip_sounds
                    .entry(idx(f[1]))
                    .or_default()
                    .push((num(f[2]), cue_name(f[3]))),
                ("custpose", 3) if f[1] == "weapon" => k.cust_weapon_clip = Some(idx(f[2])),
                ("part", 11..=13) => {
                    raw_parts.push(f.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                }
                // set <id> <name> <dlc> <parts>
                ("set", 5) => k.sets.push(ArmorSet {
                    id: f[1].into(),
                    parts: f[4].split(',').map(str::to_string).collect(),
                }),
                ("socket", 7) => {
                    if let Some(s) = socket(&f) {
                        k.sockets.entry(f[1].to_string()).or_default().push(s);
                    }
                }
                ("vehicle", 17) => vehicles.push(Vehicle {
                    form: f[1].into(),
                    class,
                    model: f[2].into(),
                    to_vehicle_clip: idx(f[3]),
                    to_robot_clip: idx(f[4]),
                    // f[6]: the robot's fold clip's length (it hides first).
                    timing: Timing {
                        robot_hide: num(f[5]),
                        robot_show: num(f[7]),
                        robot_unfold: num(f[8]),
                        car_show: num(f[9]),
                        car_fold: num(f[10]),
                        car_hide: num(f[11]),
                        car_unfold: num(f[12]),
                        fold_lift: 0.0,
                        unfold_lift: 0.0,
                    },
                    wheel_radius: num(f[13]),
                    height: num(f[14]),
                    legs: f[15] == "1",
                    lift: num(f[16]),
                    handling: Handling::JAZZ,
                    drive: Drive::JAZZ,
                    ground: Ground::JAZZ,
                    body: Body::JAZZ,
                    hull: None,
                    camera: VehicleCamera::JAZZ,
                    wheel_sockets: Vec::new(),
                    decal_sockets: Vec::new(),
                    tank: None,
                    plane: None,
                    rockets: false,
                    effects: Vec::new(),
                    low_landing_secs: 1.0,
                    hover_jets: Vec::new(),
                    jump_jets: Vec::new(),
                    boosters: Vec::new(),
                    muzzles: Vec::new(),
                    sounds: HashMap::new(),
                    transform_sounds: [Vec::new(), Vec::new()],
                    transform_steps: [Vec::new(), Vec::new()],
                    clip_sounds: HashMap::new(),
                    audio_events: None,
                }),
                ("vhandling", 16) => {
                    let v: Vec<f32> = f[2..].iter().map(|s| num(s)).collect();
                    handlings.push((
                        f[1].to_string(),
                        Handling {
                            max_speed: v[0],
                            accel: v[1],
                            jump_speed: v[2],
                            jump_forward: v[3],
                            jump_pitch: v[4],
                            jump_gravity: v[5],
                            hover_speed: v[6],
                            hover_accel: v[7],
                            hover_jump_speed: v[8],
                            hover_jump_pitch: v[9],
                            drift_secs: v[10],
                            hover_jump_gravity: v[11],
                            hover_height: v[12],
                            wheel_drop: v[13],
                        },
                    ));
                }
                // vcamera <form> <drive orbit> <drive height> <hover orbit> <hover
                // height> [<drive fov> <hover fov>]
                ("vcamera", 6 | 8) => cameras.push((
                    f[1].to_string(),
                    VehicleCamera {
                        drive_distance: num(f[2]),
                        drive_up: num(f[3]),
                        hover_distance: num(f[4]),
                        hover_up: num(f[5]),
                        fov: (f.len() == 8).then(|| (num(f[6]), num(f[7]))),
                        strategies: None,
                        middle: None,
                    },
                )),
                // vmiddle <form> <up> <ahead>
                ("vmiddle", 4) => middles.push((f[1].to_string(), (num(f[2]), num(f[3])))),
                // vdrive <form> <angular damping> <ahead> <behind> <gyration²> <steer>
                // <grips x2> <bank> <bank speed> <roll pull> <roll damping> <air turn>
                // <air strafe> <jump shake x4> <landing shake x4>
                ("vdrive", 23) => {
                    let v: Vec<f32> = f[2..].iter().map(|s| num(s)).collect();
                    drives.push((
                        f[1].to_string(),
                        Drive {
                            angular_damping: v[0],
                            ahead: v[1],
                            behind: v[2],
                            gyration: v[3].max(0.01),
                            max_steer: v[4],
                            grip_front: v[5],
                            grip_back: v[6],
                            bank: v[7],
                            bank_speed: v[8],
                            roll_pull: v[9],
                            roll_damping: v[10],
                            air_turn: v[11],
                            air_strafe: v[12],
                            jump_shake: [v[13], v[14], v[15], v[16]],
                            land_shake: [v[17], v[18], v[19], v[20]],
                        },
                    ));
                }
                // vground <form> <wheel across ahead>x4 <pitch pull damping> <rise pull
                // damping> <hover radius> <hover centre across ahead> <hover pitch,
                // roll and rise pull damping> <hover roll hold>
                ("vground", 24) => {
                    let v: Vec<f32> = f[2..].iter().map(|s| num(s)).collect();
                    let pair = |i: usize| (v[i], v[i + 1]);
                    grounds.push((
                        f[1].to_string(),
                        Ground {
                            wheels: [
                                Vec2::new(v[0], v[1]),
                                Vec2::new(v[2], v[3]),
                                Vec2::new(v[4], v[5]),
                                Vec2::new(v[6], v[7]),
                            ],
                            wheel_pitch: pair(8),
                            wheel_rise: pair(10),
                            hover_radius: v[12],
                            hover_centre: Vec2::new(v[13], v[14]),
                            hover_pitch: pair(15),
                            hover_roll: pair(17),
                            hover_rise: pair(19),
                            hover_hold: v[21],
                        },
                    ));
                }
                // vcam <form> <drive|hover> <property> <states> <values...>
                ("vcam", n) if n >= 6 => read_vcam(
                    strategies
                        .entry(f[1].to_string())
                        .or_insert([CamStrategy::default(); 2]),
                    &f,
                ),
                // vtransform <form> <folding up lift> <unfolding lift>
                ("vtransform", 4) => transform_lifts.push((f[1].to_string(), num(f[2]), num(f[3]))),
                ("vbody", 5) => bodies.push((
                    f[1].to_string(),
                    Body {
                        half_width: num(f[2]),
                        ahead: num(f[3]),
                        behind: num(f[4]),
                    },
                )),
                ("vhull", 7) => hulls.push((
                    f[1].to_string(),
                    Hull {
                        ahead: num(f[2]),
                        behind: num(f[3]),
                        half_width: num(f[4]),
                        bottom: num(f[5]),
                        top: num(f[6]),
                    },
                )),
                ("vsound", 4) => {
                    vsounds.push((f[1].to_string(), f[2].to_string(), f[3].to_string()))
                }
                ("vtsound", 5) => {
                    vtsounds.push((
                        f[1].to_string(),
                        f[2] == "to_robot",
                        num(f[3]),
                        f[4].to_string(),
                    ));
                }
                ("vcsound", 5) => {
                    vcsounds.push((f[1].to_string(), idx(f[2]), num(f[3]), f[4].to_string()));
                }
                ("vtstep", 5) => {
                    vtsteps.push((
                        f[1].to_string(),
                        f[2] == "to_robot",
                        num(f[3]),
                        f[4].to_string(),
                    ));
                }
                ("vaudio", 3) => {
                    let events = f[2]
                        .split(',')
                        .filter(|e| !e.is_empty() && *e != "-")
                        .map(str::to_string)
                        .collect();
                    vaudio.push((f[1].to_string(), events));
                }
                ("vsocket", 7) => {
                    if let Some(s) = socket(&f) {
                        vsockets.push((f[1].to_string(), s));
                    }
                }
                ("vdecal", 7) => {
                    if let Some(s) = socket(&f) {
                        vdecals.push((f[1].to_string(), s));
                    }
                }
                // vfx <form> <kind> <system> <bone> <t> <rotator> [<scale>]
                ("vfx", 7 | 8) => {
                    let when = match f[2] {
                        "hover" => Some(FxWhen::Hover),
                        "boost" => Some(FxWhen::Boost),
                        "jump" => Some(FxWhen::Jump),
                        "land" => Some(FxWhen::Land),
                        "land_low" => Some(FxWhen::LandLow),
                        "land_high" => Some(FxWhen::LandHigh),
                        "hover_land_low" => Some(FxWhen::HoverLandLow),
                        "hover_land_high" => Some(FxWhen::HoverLandHigh),
                        _ => None,
                    };
                    let rot: Vec<i32> = f[6]
                        .split_whitespace()
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    if let (Some(when), Some(at), [p, y, r]) =
                        (when, floats::<3>(f[5]), rot.as_slice())
                    {
                        effects.push((
                            f[1].to_string(),
                            VehicleFx {
                                when,
                                system: f[3].to_string(),
                                bone: f[4].to_string(),
                                at: Vec3::from(at),
                                rotation: [*p, *y, *r],
                                scale: f
                                    .get(7)
                                    .and_then(|s| floats::<3>(s))
                                    .map_or(Vec3::ONE, Vec3::from),
                            },
                        ));
                    }
                }
                // vfxland <form> <secs>
                ("vfxland", 3) => {
                    if let Ok(secs) = f[2].parse::<f32>() {
                        low_landings.push((f[1].to_string(), secs));
                    }
                }
                // vtank <form> <cannon bone> <muzzle bone> <t> <q> <s>
                ("vtank", 7) => {
                    // (socket() reads the bone and transform from f[3..]; f[2] is the
                    // cannon's bone.)
                    if let Some(s) = socket(&f) {
                        tanks.push((
                            f[1].to_string(),
                            Tank {
                                cannon_bone: f[2].to_string(),
                                muzzle: Socket {
                                    name: "WeaponSocket_TankCannon".into(),
                                    ..s
                                },
                            },
                        ));
                    }
                }
                // vplane <form> <29 numbers: Plane's, in its order>
                ("vplane", 31) => {
                    let v: Vec<f32> = f[2..].iter().map(|s| num(s)).collect();
                    planes.push((
                        f[1].to_string(),
                        Plane {
                            climb_speed: v[0],
                            climb_secs: v[1],
                            dash_speed: v[2],
                            dash_secs: v[3],
                            drag: v[4],
                            lean: v[5],
                            pitch_lead: v[6],
                            yaw_lead: v[7],
                            roll_lead: v[8],
                            lead_lerp: v[9],
                            full_pitch: v[10],
                            max_pitch: v[11],
                            hover_pitch: v[12],
                            hover_fov: v[13],
                            fly_fov: v[14],
                            hover_pitch_range: (v[15], v[16]),
                            fly_pitch_range: (v[17], v[18]),
                            turn_speed: v[19],
                            turn_accel: v[20],
                            turn_damping: v[21],
                            pitch_speed: v[22],
                            pitch_accel: v[23],
                            pitch_damping: v[24],
                            camera_roll: v[25],
                            to_fly_secs: v[26],
                            to_hover_secs: v[27],
                            crash_secs: v[28],
                        },
                    ));
                }
                ("vpoint", 4) => {
                    if let Some(p) = floats::<3>(f[3]) {
                        vpoints.push((f[1].to_string(), f[2].to_string(), Vec3::from(p)));
                    }
                }
                ("armset", 3) => arm_sets.push(ArmSet {
                    name: f[1].into(),
                    clip: idx(f[2]),
                    ..Default::default()
                }),
                ("switch", 7) => switches.push((
                    f[1].to_string(),
                    f[2].to_string(),
                    ScaleEvent {
                        time: num(f[3]),
                        from: num(f[4]),
                        to: num(f[5]),
                        ramp: num(f[6]),
                    },
                )),
                ("shoulderset", 4) => shoulder_sets.push(ShoulderSet {
                    name: f[1].into(),
                    change_hands_clip: idx(f[2]),
                    equip_clip: idx(f[3]),
                }),
                ("gunpaint" | "gunarm", 3) => {
                    if !self.gun_paint.iter().any(|(m, _)| m == f[1]) {
                        self.gun_paint.push((f[1].to_string(), f[2].to_string()));
                    }
                }
                _ => {}
            }
        }
        let find = |list: &[Vehicle], form: &str| -> Option<usize> {
            list.iter().position(|v| v.form == form)
        };
        for (form, s) in vsockets {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].wheel_sockets.push(s);
            }
        }
        for (form, s) in vdecals {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].decal_sockets.push(s);
            }
        }
        let rockets = vehicle_weapons.iter().any(|w| w == "RocketVehicle");
        for v in &mut vehicles {
            v.rockets = rockets;
        }
        for (form, t) in tanks {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].tank = Some(t);
            }
        }
        for (form, p) in planes {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].plane = Some(p);
            }
        }
        for (form, e) in effects {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].effects.push(e);
            }
        }
        for (form, secs) in low_landings {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].low_landing_secs = secs;
            }
        }
        for (form, h) in handlings {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].handling = h;
            }
        }
        for (form, fold, unfold) in transform_lifts {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].timing.fold_lift = fold;
                vehicles[i].timing.unfold_lift = unfold;
            }
        }
        for (form, b) in bodies {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].body = b;
            }
        }
        for (form, h) in hulls {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].hull = Some(h);
            }
        }
        for (form, c) in cameras {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].camera = c;
            }
        }
        for (form, d) in drives {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].drive = d;
            }
        }
        for (form, g) in grounds {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].ground = g;
            }
        }
        for (form, s) in strategies {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].camera.strategies = Some(s);
            }
        }
        for (form, m) in middles {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].camera.middle = Some(m);
            }
        }
        for (form, event, cue) in vsounds {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].sounds.insert(event, cue_name(&cue));
            }
        }
        for (form, to_robot, t, cue) in vtsounds {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].transform_sounds[usize::from(to_robot)].push((t, cue_name(&cue)));
            }
        }
        for (form, clip, t, cue) in vcsounds {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i]
                    .clip_sounds
                    .entry(clip)
                    .or_default()
                    .push((t, cue_name(&cue)));
            }
        }
        for (form, to_robot, t, event) in vtsteps {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].transform_steps[usize::from(to_robot)].push((t, event));
            }
        }
        for (form, events) in vaudio {
            if let Some(i) = find(&vehicles, &form) {
                vehicles[i].audio_events = Some(events);
            }
        }
        for (form, kind, p) in vpoints {
            if let Some(i) = find(&vehicles, &form) {
                let v = &mut vehicles[i];
                match kind.as_str() {
                    "hover" => v.hover_jets.push(p),
                    "jump" => v.jump_jets.push(p),
                    "boost" => v.boosters.push(p),
                    "muzzle" => v.muzzles.push(p),
                    _ => {}
                }
            }
        }
        for (set, bone, e) in switches {
            if let Some(a) = arm_sets.iter_mut().find(|a| a.name == set) {
                self.switch_bones.insert(bone.clone());
                a.schedule.entry(bone).or_default().push(e);
            }
        }
        for a in &mut arm_sets {
            for events in a.schedule.values_mut() {
                events.sort_by(|x, y| x.time.total_cmp(&y.time));
            }
        }
        // Its clips.glb: the transformations first, then the arm and shoulder clips,
        // each put in its slot.
        k.clip_count = 2 * vehicles.len() + arm_sets.len() + 2 * shoulder_sets.len();
        k.first_additive_clip = 2 * vehicles.len();
        for v in &mut vehicles {
            v.to_vehicle_clip = k.slot(v.to_vehicle_clip);
            v.to_robot_clip = k.slot(v.to_robot_clip);
        }
        for a in &mut arm_sets {
            a.clip = k.slot(a.clip);
        }
        // Its forearm sets' clips for the gun going away and coming back, when built
        // (tools/build_arm_equip.py), in the slots after clips.glb's.
        if let Ok(text) = std::fs::read_to_string(assets.join(arm_equip_file(class)))
            && assets.join(arm_equip_model(class)).exists()
            && assets.join(arm_equip_mirror_model(class)).exists()
        {
            let mut count = 0;
            for line in text.lines() {
                let f: Vec<&str> = line.split('\t').collect();
                let Some(a) = f
                    .get(1)
                    .and_then(|set| arm_sets.iter_mut().find(|a| a.name == *set))
                else {
                    continue;
                };
                match (f[0], f.len()) {
                    ("armequip", 4) => {
                        let (away, back) = (idx(f[2]), idx(f[3]));
                        count = count.max(away + 1).max(back + 1);
                        a.away_clip = Some(away);
                        a.back_clip = Some(back);
                    }
                    ("equipscale" | "unequipscale", 7) => {
                        let e = ScaleEvent {
                            time: num(f[3]),
                            from: num(f[4]),
                            to: num(f[5]),
                            ramp: num(f[6]),
                        };
                        let schedule = if f[0] == "equipscale" {
                            &mut a.away
                        } else {
                            &mut a.back
                        };
                        self.switch_bones.insert(f[2].to_string());
                        schedule.entry(f[2].to_string()).or_default().push(e);
                    }
                    _ => {}
                }
            }
            k.equip_clip_count = count;
            for a in &mut arm_sets {
                a.away_clip = a.away_clip.map(|c| k.equip_slot(c));
                a.back_clip = a.back_clip.map(|c| k.equip_slot(c));
                for events in a.away.values_mut().chain(a.back.values_mut()) {
                    events.sort_by(|x, y| x.time.total_cmp(&y.time));
                }
            }
        }
        for s in &mut shoulder_sets {
            s.change_hands_clip = k.slot(s.change_hands_clip);
            s.equip_clip = k.slot(s.equip_clip);
        }
        for f in raw_parts {
            let Some(kind) = part_kind(&f[2]) else {
                continue;
            };
            // part <id> <type> <name> <dlc> <meshes> <wheel> <vehicle> <arm set>
            //      <shoulder set> <disabled types> [<attachment socket>
            //      [<vehicle's wheel>]]
            let opt = |s: &str| (s != "-").then(|| s.to_string());
            k.parts.push(Part {
                id: f[1].clone(),
                kind,
                meshes: opt(&f[5])
                    .map(|m| m.split(',').map(str::to_string).collect())
                    .unwrap_or_default(),
                wheel: opt(&f[6]),
                socket: f
                    .get(11)
                    .filter(|s| s.as_str() != "-")
                    .map_or("wheel".to_string(), |s| s.to_ascii_lowercase()),
                vehicle_wheel: f.get(12).map_or_else(|| opt(&f[6]), |s| opt(s)),
                vehicle: vehicles
                    .iter()
                    .position(|v| v.form == f[7])
                    .map(|i| first_vehicle + i),
                arm_set: arm_sets
                    .iter()
                    .position(|a| a.name == f[8])
                    .map(|i| first_arm + i),
                shoulder_set: shoulder_sets
                    .iter()
                    .position(|s| s.name == f[9])
                    .map(|i| first_shoulder + i),
                no_wheels: f[10].split(',').any(|t| VEHICLE_PARTS.contains(&t)),
            });
        }
        // Empty slots wear the class's first armor set's parts (the game's first:
        // Bumblebee's for the Infiltrator, free to all).
        let default = k.sets.first().cloned();
        if let Some(set) = default {
            for id in &set.parts {
                if let Some(i) = k.parts.iter().position(|p| &p.id == id) {
                    k.default_parts[k.parts[i].kind] = Some(i);
                }
            }
        }
        let own_clips = !k.robot_clips.is_empty() || k.cust_weapon_clip.is_some();
        k.available = !k.parts.is_empty()
            && k.equip_slot(k.equip_clip_count) <= EXTRA_SLOTS
            && k.first_additive_clip <= TRANSFORM_SLOTS
            && assets.join(parts_model(class)).exists()
            && assets.join(clips_model(class)).exists()
            && assets.join(clips_mirror_model(class)).exists()
            && (!own_clips
                || [robot_clips_model(class), robot_clips_mirror_model(class)]
                    .iter()
                    .all(|f| assets.join(f).exists()))
            && vehicles.iter().all(|v| assets.join(&v.model).exists());
        if k.available {
            self.vehicles.extend(vehicles);
            self.arm_sets.extend(arm_sets);
            self.shoulder_sets.extend(shoulder_sets);
        } else {
            warn!("{}'s characters are incomplete", CLASSES[class]);
            k.parts.clear();
        }
        info!(
            "Characters ({}): {} parts, {} sets (empty slots wear {}'s), {} clips, {} of \
             robot.glb's clips its own",
            CLASSES[class],
            k.parts.len(),
            k.sets.len(),
            k.sets.first().map_or("no set", |s| s.id.as_str()),
            k.clip_count,
            k.robot_clips.len()
        );
        k
    }

    /// A class's characters (an empty, unavailable one if it has none).
    pub fn class(&self, class: usize) -> &Class {
        static NONE: std::sync::OnceLock<Class> = std::sync::OnceLock::new();
        self.classes
            .get(class)
            .unwrap_or_else(|| NONE.get_or_init(Class::default))
    }

    /// Whether a class can be played (its characters are there).
    pub fn playable(&self, class: usize) -> bool {
        self.class(class).available
    }

    /// A class's robot (Jazz's where its file gives none).
    pub fn robot(&self, class: usize) -> Robot {
        self.class(class).robot.unwrap_or(Robot::JAZZ)
    }

    /// A class's part by its id (as builds keep them).
    pub fn part_index(&self, class: usize, id: &str) -> Option<u8> {
        self.class(class)
            .parts
            .iter()
            .position(|p| p.id == id)
            .and_then(|i| u8::try_from(i).ok())
    }

    /// The part a look wears of a type (its own, or its class's default set's).
    pub fn part(&self, look: &Look, kind: usize) -> Option<&Part> {
        let class = self.class(look.class());
        let own = class
            .parts
            .get(look.parts[kind] as usize)
            .filter(|p| p.kind == kind);
        own.or_else(|| class.default_parts[kind].map(|i| &class.parts[i]))
    }

    /// A build's look: its class, its parts (by id, in a build's order) and colours.
    pub fn look(
        &self,
        class: usize,
        part_ids: &[String],
        primary: [f32; 3],
        secondary: [f32; 3],
        metallic: f32,
    ) -> Look {
        let byte = |v: f32| v.round().clamp(0.0, 255.0) as u8;
        let parts = &self.class(class).parts;
        Look {
            class: class.min(CLASSES.len() - 1) as u8,
            parts: std::array::from_fn(|i| {
                part_ids
                    .get(i)
                    .and_then(|id| self.part_index(class, id))
                    .filter(|&p| parts[p as usize].kind == i)
                    .unwrap_or(DEFAULT_PART)
            }),
            // (The build's seventh part: customize.rs's PART_TYPES.)
            decals: part_ids
                .get(PART_TYPES.len())
                .map_or(0, |id| self.decals.number(id)),
            primary: primary.map(byte),
            secondary: secondary.map(byte),
            metallic: byte(metallic * 255.0),
            decepticon: false,
        }
    }

    /// The mesh a look's robot hangs on its sockets (its wheels, cannon or wings)
    /// and the start of the sockets' names, none if a part leaves them off.
    pub fn wheel(&self, look: &Look) -> Option<(&str, &str)> {
        let part = self.wheel_part(look)?;
        Some((part.wheel.as_deref()?, part.socket.as_str()))
    }

    /// The same for its vehicle (a jet's wings are its own: [`Part::vehicle_wheel`]).
    pub fn vehicle_wheel(&self, look: &Look) -> Option<(&str, &str)> {
        let part = self.wheel_part(look)?;
        Some((part.vehicle_wheel.as_deref()?, part.socket.as_str()))
    }

    fn wheel_part(&self, look: &Look) -> Option<&Part> {
        let off = (0..PART_TYPES.len()).any(|k| self.part(look, k).is_some_and(|p| p.no_wheels));
        if off {
            return None;
        }
        self.part(look, WHEELS)
    }

    /// A sound of a look's vehicle (`vehicle`: its kit's): the cue its form's sound
    /// event set gives `event` (SOUND_*), or the one it gives the event's stand-in
    /// (SOUND_STAND_INS), None where it gives neither (Snarl's tyres don't squeal)
    /// or its audio component never plays it (COMPONENT_EVENTS: a Dinobot's engine,
    /// jumps and landings); Jazz's car's, `jazz`, without the characters' sounds.
    pub fn car_sound(
        &self,
        vehicle: Option<usize>,
        event: &str,
        jazz: &'static str,
    ) -> Option<&'static str> {
        match vehicle.and_then(|v| self.vehicles.get(v)) {
            Some(v) if !v.sounds.is_empty() => {
                let plays = |e: &str| {
                    !COMPONENT_EVENTS.contains(&e)
                        || v.audio_events
                            .as_ref()
                            .is_none_or(|events| events.iter().any(|x| x == e))
                };
                if !plays(event) {
                    return None;
                }
                v.sounds.get(event).copied().or_else(|| {
                    let (_, stand_in) = SOUND_STAND_INS.iter().find(|(e, _)| *e == event)?;
                    v.sounds.get(*stand_in).copied().filter(|_| plays(stand_in))
                })
            }
            _ => Some(jazz),
        }
    }

    /// Whether a vehicle has an engine its audio component plays (a Dinobot has
    /// none: its footsteps and growls are all it makes walking and running).
    pub fn car_has_engine(&self, vehicle: Option<usize>) -> bool {
        match vehicle.and_then(|v| self.vehicles.get(v)) {
            Some(v) if !v.sounds.is_empty() => v
                .audio_events
                .as_ref()
                .is_none_or(|events| events.iter().any(|e| e == SOUND_ENGINE)),
            _ => true,
        }
    }

    /// A sound of a class's robot: the cue its sound event set gives `event`
    /// (ROBOT_*), or Jazz's, `jazz`.
    pub fn robot_sound(&self, class: usize, event: &str, jazz: &'static str) -> &'static str {
        self.class(class).sounds.get(event).copied().unwrap_or(jazz)
    }

    /// The sounds a class's own version of one of robot.glb's clips plays (its
    /// melee's swing: (seconds in, cue)), if it has its own.
    pub fn robot_clip_sounds(&self, class: usize, clip: usize) -> Option<&[(f32, &'static str)]> {
        self.class(class)
            .robot_clip_sounds
            .get(&clip)
            .map(Vec::as_slice)
    }

    /// The sounds of a vehicle's transformation, to the car or back: (seconds into
    /// its clips as the game plays them, cue); None without the characters' sounds.
    pub fn transform_sounds(
        &self,
        vehicle: Option<usize>,
        to_car: bool,
    ) -> Option<&[(f32, &'static str)]> {
        let v = vehicle.and_then(|v| self.vehicles.get(v))?;
        let sounds = &v.transform_sounds[usize::from(!to_car)];
        (!sounds.is_empty()).then_some(sounds.as_slice())
    }

    /// The footsteps of a vehicle's transformation, to the car or back: (seconds into
    /// its clips as the game plays them, the robot's sound event); none without the
    /// characters'.
    pub fn transform_steps(&self, vehicle: Option<usize>, to_car: bool) -> &[(f32, String)] {
        vehicle
            .and_then(|v| self.vehicles.get(v))
            .map_or(&[], |v| v.transform_steps[usize::from(!to_car)].as_slice())
    }

    /// The sounds a vehicle's looping clip plays (`clip`: car.rs's CAR_*): (seconds
    /// into the clip, cue); none for most (Snarl's and Slug's walk, run and stand play
    /// their footsteps, tails and growls).
    pub fn car_clip_sounds(&self, vehicle: Option<usize>, clip: usize) -> &[(f32, &'static str)] {
        vehicle
            .and_then(|v| self.vehicles.get(v))
            .and_then(|v| v.clip_sounds.get(&clip))
            .map_or(&[], Vec::as_slice)
    }

    /// The vehicle of a class whose transformation clip (a slot of the robot's
    /// graph) this is.
    pub fn vehicle_of_clip(&self, class: usize, slot: usize) -> Option<usize> {
        self.vehicles.iter().position(|v| {
            v.class == class && (v.to_vehicle_clip == slot || v.to_robot_clip == slot)
        })
    }

    /// The vehicle a look's chest makes.
    pub fn vehicle(&self, look: &Look) -> Option<usize> {
        self.part(look, CHEST).and_then(|p| p.vehicle)
    }

    /// What a look brings: its class's robot, its vehicle, clips and timing (Jazz's
    /// without the files).
    pub fn kit(&self, look: &Look) -> Kit {
        let class = look.class();
        if !self.playable(class) {
            return Kit::JAZZ;
        }
        let vehicle = self.vehicle(look);
        let v = vehicle.map(|i| &self.vehicles[i]);
        let arm_set = self.part(look, ARM_LO).and_then(|p| p.arm_set);
        let shoulder = self
            .part(look, ARM_UP)
            .and_then(|p| p.shoulder_set)
            .map(|i| &self.shoulder_sets[i]);
        Kit {
            class,
            robot: self.robot(class),
            camera: self.class(class).camera.unwrap_or(Camera::JAZZ),
            vehicle,
            to_vehicle: v.map(|v| v.to_vehicle_clip),
            to_robot: v.map(|v| v.to_robot_clip),
            arm_switch: arm_set.map(|i| self.arm_sets[i].clip),
            arm_away: arm_set.and_then(|i| self.arm_sets[i].away_clip),
            arm_back: arm_set.and_then(|i| self.arm_sets[i].back_clip),
            shoulder_switch: shoulder.map(|s| s.change_hands_clip),
            shoulder_equip: shoulder.map(|s| s.equip_clip),
            arm_set,
            timing: v.map_or(Timing::JAZZ, |v| v.timing),
            wheel_radius: v.map_or(0.3, |v| v.wheel_radius.max(0.05)),
            vehicle_height: v.map_or(1.1, |v| v.height),
            legs: v.is_some_and(|v| v.legs),
            lift: v.map_or(0.0, |v| v.lift),
            handling: v.map_or(Handling::JAZZ, |v| v.handling),
            drive: v.map_or(Drive::JAZZ, |v| v.drive),
            ground: v.map_or(Ground::JAZZ, |v| v.ground),
            body: v.map_or(Body::JAZZ, |v| v.body),
            hull: v.and_then(|v| v.hull),
            vehicle_camera: v.map_or(VehicleCamera::JAZZ, |v| v.camera),
            tank: v.is_some_and(|v| v.tank.is_some()),
            plane: v.and_then(|v| v.plane),
            rockets: v.is_some_and(|v| v.rockets),
        }
    }

    /// Whether a tank's effect plays on this bone (VehicleFx::bone).
    pub fn is_fx_bone(&self, name: &str) -> bool {
        self.vehicles
            .iter()
            .any(|v| v.effects.iter().any(|e| e.bone == name))
    }

    /// Whether a bone is a tank's cannon bone (Tank::cannon_bone).
    pub fn is_cannon_bone(&self, name: &str) -> bool {
        self.vehicles
            .iter()
            .any(|v| v.tank.as_ref().is_some_and(|t| t.cannon_bone == name))
    }

    /// Whether a forearm set's switch scales this bone.
    pub fn is_switch_bone(&self, name: &str) -> bool {
        self.switch_bones.contains(name)
    }

    /// A bone's scale `t` seconds into switching hands (ADD_Weapon_MirrorReverse_ARM's
    /// ScaleBone notifies). The clip moves the gun to the right arm; to the left
    /// (`to_left`), it plays mirrored, each bone as its partner. A held size lasts
    /// until the bone's next notify; the ramps that have started all count, the
    /// smallest winning (the pieces fold away one after another, then whatever is
    /// left goes at once).
    pub fn switch_scale(&self, arm_set: Option<usize>, bone: &str, to_left: bool, t: f32) -> f32 {
        let Some(set) = arm_set.and_then(|i| self.arm_sets.get(i)) else {
            return 1.0;
        };
        scheduled_scale(&set.schedule, bone, to_left, t)
    }

    /// A bone's scale `t` seconds into the gun going away (`away`: the forearm on its
    /// arm unfolding, ADD_Weapon_Equip_ARM's ScaleBone notifies) or coming back (it
    /// folding away, ADD_Weapon_Unequip_ARM's), as switch_scale: the clips are the
    /// gun arm's on the right; with the gun on the left (`left`), each bone plays
    /// its partner's. 1 for a bone they don't scale.
    pub fn equip_scale(
        &self,
        arm_set: Option<usize>,
        bone: &str,
        left: bool,
        away: bool,
        t: f32,
    ) -> f32 {
        let Some(set) = arm_set.and_then(|i| self.arm_sets.get(i)) else {
            return 1.0;
        };
        scheduled_scale(if away { &set.away } else { &set.back }, bone, left, t)
    }
}

/// A bone's scale `t` seconds into a clip by its ScaleBone notifies (`schedule`, by
/// bone, in order): mirrored (`mirrored`), each bone takes its partner's. A held size
/// lasts until the bone's next notify; the ramps that have started all count, the
/// smallest winning (Characters::switch_scale).
fn scheduled_scale(
    schedule: &HashMap<String, Vec<ScaleEvent>>,
    bone: &str,
    mirrored: bool,
    t: f32,
) -> f32 {
    let key;
    let name = if mirrored {
        key = partner(bone);
        key.as_str()
    } else {
        bone
    };
    let Some(events) = schedule.get(name) else {
        return 1.0;
    };
    let mut scale = 1.0f32;
    for (i, e) in events.iter().enumerate() {
        if e.time > t {
            break;
        }
        let held = e.from == e.to;
        let replaced = events[i + 1..]
            .iter()
            .any(|n| n.time > e.time && n.time <= t);
        if held && replaced {
            continue;
        }
        scale = scale.min(e.at(t));
    }
    scale
}

// ---------------------------------------------------------------------------
// The paint
// ---------------------------------------------------------------------------

/// The build's colours, as the game's character materials take them
/// (Cust_Color_A, Cust_COLOR_B: linear, 0-1), and its metallic slider in the
/// primary's w; its faction's energon colour (EnergonColor), with 1 in w when the
/// material's emissive texture is its glow masks (tools/build_class.py's
/// GLOW_COVERAGE: the part's own light in red, its energon in green), which the
/// shader lights (0: the material's own emissive, as it is).
#[derive(Clone, Copy, Debug, ShaderType)]
pub struct PaintColors {
    pub primary: Vec4,
    pub secondary: Vec4,
    pub energon: Vec4,
    /// An upgrade's part glowing as it goes on (ENV_BRDF_WEP_ResIn_MAT's ResInColor,
    /// linear, and ResInValue in w: TnWeaponMesh.UpdateModificationHighlights).
    pub res_in: Vec4,
    /// A dead robot dissolving (the death sets' TnDeresser drawing its overlay over
    /// it: enemies.rs's dissolve_meshes): where the overlay comes out from in the
    /// world (ObjectWorldPosition: the body's middle) and in w how far it reaches
    /// (ObjectRadius, metres; below 0, none: the robot whole).
    pub deres: Vec4,
    /// The dissolve's DefragValue (x) and past which the robot is drawn no more, only
    /// its overlay (y: NormalizedTimeToMakeMeshInvisible); its overlay's noise
    /// multiplier (z: Energon Fractal Alpha Multiplier) and in w 1 if that noise is
    /// ElectricArcs, not fractalMaps.
    pub deres_fx: Vec4,
    /// The overlay's look (enemies.rs's [`crate::enemies::DefragLook`]): its rim's
    /// colour times its intensity, its exponent in w (Fresnel Color, Intensity and
    /// Exponent); its front's colour and in w its intensity (Defrag Lead Color, Lead
    /// Intensity); its burn-out colour, and in w the glow round its middle (Burn
    /// Out Color, Core Intensity); its lines' intensity and contrast, the core's
    /// exponent and the tiling (Energon Intensity, Energon Contrast, Core Exponent,
    /// MaterialTiling).
    pub deres_fresnel: Vec4,
    pub deres_lead: Vec4,
    pub deres_burn: Vec4,
    pub deres_energon: Vec4,
    /// The part's material values ([`PaintValues`]): Roughness Power, Roughness
    /// Offset, SmoothStep_Min, SmoothStep_Max.
    pub gloss: Vec4,
    /// DiffuseIntensityScalar, DiffuseIntensityAdd, and the last mip level of the
    /// cube map it reflects (below 0: none, [`reflect_surroundings`]).
    pub shade: Vec4,
    /// CoefficientColor_C: the unpainted metal's reflection coefficient.
    pub coefficient: Vec4,
    /// One of a cloaking Scout's copies (cloak.rs): his elbow's joint in the world,
    /// where the cloak comes out from, and in w the overlay's reach (metres: his
    /// bounds' radius times 1.2; below 0, not a copy).
    pub cloak: Vec4,
    /// What the copy draws (cloak.rs's [`crate::cloak::Look`]): in x 1 for the
    /// overlay, 2 for the cloak; in y the overlay's DefragValue, in z the cloak's
    /// Fade_In.
    pub cloak_fx: Vec4,
}

impl PaintColors {
    /// No dissolve.
    pub const WHOLE: Vec4 = Vec4::new(0.0, 0.0, 0.0, -1.0);

    /// A part's paint: `primary` (its w the metallic slider), `secondary` and
    /// `energon` (its w 1 when the material's emissive is its glow masks), shaded
    /// with its material's `values`, reflecting a cube map with `reflection_lod` as
    /// its last mip level (below 0: none).
    pub fn new(
        primary: Vec4,
        secondary: Vec4,
        energon: Vec4,
        values: &PaintValues,
        reflection_lod: f32,
    ) -> PaintColors {
        PaintColors {
            primary,
            secondary,
            energon,
            res_in: Vec4::ZERO,
            deres: PaintColors::WHOLE,
            deres_fx: Vec4::ZERO,
            deres_fresnel: Vec4::ZERO,
            deres_lead: Vec4::ZERO,
            deres_burn: Vec4::ZERO,
            deres_energon: Vec4::ZERO,
            gloss: values.gloss,
            shade: Vec4::new(values.diffuse.x, values.diffuse.y, reflection_lod, 0.0),
            coefficient: values.coefficient.extend(0.0),
            cloak: PaintColors::WHOLE,
            cloak_fx: Vec4::ZERO,
        }
    }
}

/// The part materials' values (tools/build_paint.py): the game's character
/// material instances', which paint.wgsl shades the parts with as the game does.
pub const PAINT_VALUES: &str = "models/paint.txt";

/// A part material's values (its game material instance's): how glossy its gloss
/// mask makes it ([Roughness Power, Roughness Offset, SmoothStep_Min,
/// SmoothStep_Max]), how bright its colour is lit ([DiffuseIntensityScalar,
/// DiffuseIntensityAdd]) and how much its unpainted metal reflects
/// (CoefficientColor_C).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintValues {
    pub gloss: Vec4,
    pub diffuse: Vec2,
    pub coefficient: Vec3,
}

impl PaintValues {
    /// TR_BRDF_CustChar_TEXSET_MAT's own: a material paint.txt doesn't have, without
    /// its "default" line.
    pub const DEFAULT: PaintValues = PaintValues {
        gloss: Vec4::new(600.0, 0.05, 0.0, 0.7),
        diffuse: Vec2::new(1.0, 0.0),
        coefficient: Vec3::splat(0.72),
    };

    fn parse(words: &[&str]) -> Option<PaintValues> {
        let v = words
            .iter()
            .map(|w| w.parse::<f32>().ok())
            .collect::<Option<Vec<f32>>>()?;
        (v.len() == 9).then(|| PaintValues {
            gloss: Vec4::new(v[0], v[1], v[2], v[3]),
            diffuse: Vec2::new(v[4], v[5]),
            coefficient: Vec3::new(v[6], v[7], v[8]),
        })
    }
}

/// paint.txt: each material's values, by the name of the texture set (or material
/// instance) its colour texture is named after (`<name>_c.ktx2`).
#[derive(Default)]
pub struct PaintTable {
    by_name: HashMap<String, PaintValues>,
    default: Option<PaintValues>,
}

impl PaintTable {
    pub fn read(text: &str) -> PaintTable {
        let mut table = PaintTable::default();
        for line in text.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.as_slice() {
                ["default", rest @ ..] => table.default = PaintValues::parse(rest),
                ["brdf", name, rest @ ..] => {
                    if let Some(v) = PaintValues::parse(rest) {
                        table.by_name.insert(name.to_string(), v);
                    }
                }
                _ => {}
            }
        }
        table
    }

    /// The values of the material with the colour texture `texture` (an asset path).
    pub fn values(&self, texture: Option<&std::path::Path>) -> PaintValues {
        texture
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .map(|s| s.strip_suffix("_c").unwrap_or(s))
            .and_then(|name| self.by_name.get(name))
            .or(self.default.as_ref())
            .copied()
            .unwrap_or(PaintValues::DEFAULT)
    }

    /// The values of the material (texture set or material instance) `name`, if
    /// paint.txt has them.
    pub fn named(&self, name: &str) -> Option<PaintValues> {
        self.by_name.get(name).copied()
    }

    /// A part material's values (by its colour texture).
    fn for_material(&self, m: &StandardMaterial) -> PaintValues {
        self.values(
            m.base_color_texture
                .as_ref()
                .and_then(|h| h.path())
                .map(|p| p.path()),
        )
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct Paint {
    #[uniform(100)]
    pub colors: PaintColors,
    /// The energon's pulse runs over this (the game's fractalMaps).
    #[texture(101)]
    #[sampler(102)]
    pub noise: Option<Handle<Image>>,
    /// The cube map it reflects: its surroundings' ([`reflect_surroundings`]).
    #[texture(103, dimension = "cube")]
    #[sampler(104)]
    pub reflection: Option<Handle<Image>>,
    /// The cloak's textures, on its copies only (cloak.rs's CloakTextures: the
    /// overlay's WEP_Defrag_Alphas and its two gradients, the cloak's
    /// DiffClouds_CLR, bubbles and Pixel_4_NORM_DXT1; all repeating, sampled with
    /// the first's sampler but the gradients).
    #[texture(105)]
    #[sampler(106)]
    pub cloak_alphas: Option<Handle<Image>>,
    #[texture(107)]
    #[sampler(108)]
    pub cloak_pulse: Option<Handle<Image>>,
    #[texture(109)]
    pub cloak_clouds: Option<Handle<Image>>,
    #[texture(110)]
    pub cloak_bubbles: Option<Handle<Image>>,
    #[texture(111)]
    pub cloak_pixels: Option<Handle<Image>>,
    /// The dissolve's own, on a dissolving robot's materials only (with the
    /// overlay's three above): IaconMask_NORM's lines and ElectricArcs (cloak.rs's
    /// CloakTextures; sampled with cloak_alphas' sampler).
    #[texture(112)]
    pub defrag_lines: Option<Handle<Image>>,
    #[texture(113)]
    pub defrag_arcs: Option<Handle<Image>>,
}

impl Paint {
    /// A paint (no cloak's textures).
    pub fn new(
        colors: PaintColors,
        noise: Option<Handle<Image>>,
        reflection: Option<Handle<Image>>,
    ) -> Paint {
        Paint {
            colors,
            noise,
            reflection,
            cloak_alphas: None,
            cloak_pulse: None,
            cloak_clouds: None,
            cloak_bubbles: None,
            cloak_pixels: None,
            defrag_lines: None,
            defrag_arcs: None,
        }
    }
}

/// The energon pulse's noise: the game's fractalMaps, as the energon cube's model
/// has it (its texture 0: tools/build_assets.py, src/energon.rs), in colour and
/// repeating, as the game samples it.
const ENERGON_NOISE_MODEL: &str = "models/props/energon.glb";

impl MaterialExtension for Paint {
    fn fragment_shader() -> ShaderRef {
        PAINT_SHADER.into()
    }

    /// The depth prepass and the shadows take the dissolve's holes too
    /// (paint_prepass.wgsl: a dissolving robot's materials are masked, so their
    /// prepass runs it: once the robot is drawn no more, only its overlay is left).
    fn prepass_fragment_shader() -> ShaderRef {
        PAINT_PREPASS_SHADER.into()
    }
}

/// A painted part: its own material (textures, normals, glow) with the paint.
pub type PaintMaterial = ExtendedMaterial<StandardMaterial, Paint>;

/// The paint materials made so far: one per part material, colours and culling
/// (a mesh hung mirrored, as the left wheels are, has its faces culled the other
/// way round: Bevy's glTF loader does the same for its mirrored nodes).
#[derive(Resource, Default)]
pub struct PaintCache {
    paints: HashMap<(AssetId<StandardMaterial>, [u8; 8], bool), Handle<PaintMaterial>>,
    flipped: HashMap<AssetId<StandardMaterial>, Handle<StandardMaterial>>,
    /// robot.glb's gun and gun arm materials on their textures with the paint mask.
    gun_paint: HashMap<AssetId<StandardMaterial>, Handle<StandardMaterial>>,
    /// The energon's noise ([`ENERGON_NOISE_MODEL`]).
    noise: Handle<Image>,
    /// The paints made in colours of their own ([`PaintCache::colored`]).
    colored: HashMap<(AssetId<StandardMaterial>, [[u32; 3]; 3], bool), Handle<PaintMaterial>>,
    /// The weapon pickups' paints ([`PaintCache::pickup`]), by their base and their
    /// material instance.
    pickups: HashMap<(AssetId<StandardMaterial>, String), Handle<PaintMaterial>>,
    /// The part materials' values ([`PAINT_VALUES`]).
    table: PaintTable,
    /// The cube map the paints reflect and its last mip level
    /// ([`reflect_surroundings`]).
    reflection: Option<(Handle<Image>, f32)>,
}

/// Whether a transform mirrors what it places.
fn mirrors(t: &Transform) -> bool {
    t.scale.is_negative_bitmask().count_ones() & 1 == 1
}

fn flip_culling(m: &mut StandardMaterial) {
    if m.cull_mode.is_some() {
        m.cull_mode = Some(bevy::render::render_resource::Face::Front);
    }
}

impl PaintCache {
    /// A part material in a look's paint. `glow`: its emissive texture, if it has
    /// one, is its glow masks (a part's or vehicle's texture set), lit in the look's
    /// faction's energon colour (not robot.glb's guns, whose glow is their own).
    pub fn get(
        &mut self,
        material: &Handle<StandardMaterial>,
        look: &Look,
        flip: bool,
        glow: bool,
        standard: &Assets<StandardMaterial>,
        paints: &mut Assets<PaintMaterial>,
    ) -> Option<Handle<PaintMaterial>> {
        let key = (material.id(), look.paint_key(), flip);
        if let Some(h) = self.paints.get(&key) {
            return Some(h.clone());
        }
        let mut base = standard.get(material)?.clone();
        if flip {
            flip_culling(&mut base);
        }
        // (The masks come through as the emissive, which the shader then lights.)
        let masks = glow && base.emissive_texture.is_some();
        if glow {
            base.emissive = if masks {
                LinearRgba::WHITE
            } else {
                LinearRgba::BLACK
            };
        }
        let c = |v: [u8; 3]| Vec3::from(v.map(|x| x as f32 / 255.0));
        let values = self.table.for_material(&base);
        let colors = PaintColors::new(
            c(look.primary).extend(look.metallic as f32 / 255.0),
            c(look.secondary).extend(0.0),
            look.energon().extend(if masks { 1.0 } else { 0.0 }),
            &values,
            self.reflection_lod(),
        );
        let handle = paints.add(self.paint(base, colors));
        self.paints.insert(key, handle.clone());
        Some(handle)
    }

    /// The paint's material, with the energon's noise and the cube map it reflects.
    fn paint(&self, base: StandardMaterial, colors: PaintColors) -> PaintMaterial {
        PaintMaterial {
            base,
            extension: Paint::new(
                colors,
                Some(self.noise.clone()),
                self.reflection.as_ref().map(|(h, _)| h.clone()),
            ),
        }
    }

    /// The last mip level of the cube map the paints reflect (-1: none).
    fn reflection_lod(&self) -> f32 {
        self.reflection.as_ref().map_or(-1.0, |(_, lod)| *lod)
    }

    /// A part material painted in `primary` and `secondary` (linear) with its glow
    /// in `energon` (linear), as an enemy is (enemies.rs: its colour pair and its
    /// type's EnergonColor); `glow` as for [`PaintCache::get`].
    pub fn colored(
        &mut self,
        material: &Handle<StandardMaterial>,
        (primary, secondary, energon): (Vec3, Vec3, Vec3),
        glow: bool,
        standard: &Assets<StandardMaterial>,
        paints: &mut Assets<PaintMaterial>,
    ) -> Option<Handle<PaintMaterial>> {
        let bits = |v: Vec3| v.to_array().map(f32::to_bits);
        let key = (
            material.id(),
            [bits(primary), bits(secondary), bits(energon)],
            glow,
        );
        if let Some(h) = self.colored.get(&key) {
            return Some(h.clone());
        }
        let mut base = standard.get(material)?.clone();
        let masks = glow && base.emissive_texture.is_some();
        if glow {
            base.emissive = if masks {
                LinearRgba::WHITE
            } else {
                LinearRgba::BLACK
            };
        }
        let values = self.table.for_material(&base);
        let colors = PaintColors::new(
            primary.extend(0.0),
            secondary.extend(0.0),
            energon.extend(if masks { 1.0 } else { 0.0 }),
            &values,
            self.reflection_lod(),
        );
        let handle = paints.add(self.paint(base, colors));
        self.colored.insert(key, handle.clone());
        Some(handle)
    }

    /// A weapon pickup's gun or cuff (paint_pickups) in its pickup paint `paint`:
    /// `base` (its material on the colour texture with the paint mask) painted grey
    /// with the metallic slider at its MetallicOffset, shaded with its material
    /// instance's values (paint.txt), keeping its own glow.
    pub fn pickup(
        &mut self,
        base: &Handle<StandardMaterial>,
        paint: &PickupPaint,
        standard: &Assets<StandardMaterial>,
        paints: &mut Assets<PaintMaterial>,
    ) -> Option<Handle<PaintMaterial>> {
        let key = (base.id(), paint.brdf.clone());
        if let Some(h) = self.pickups.get(&key) {
            return Some(h.clone());
        }
        let base = standard.get(base)?.clone();
        let values = self
            .table
            .named(&paint.brdf)
            .unwrap_or_else(|| self.table.for_material(&base));
        let grey = Vec3::splat(paint.paint);
        let colors = PaintColors::new(
            grey.extend(paint.metallic),
            grey.extend(0.0),
            Vec4::ZERO,
            &values,
            self.reflection_lod(),
        );
        let handle = paints.add(self.paint(base, colors));
        self.pickups.insert(key, handle.clone());
        Some(handle)
    }

    /// The cube map the paints reflect, if there is one ([`reflect_surroundings`]).
    pub fn reflection_cube(&self) -> Option<Handle<Image>> {
        self.reflection.as_ref().map(|(h, _)| h.clone())
    }

    /// An unpainted material, culled the other way round when `flip`.
    pub fn plain(
        &mut self,
        material: &Handle<StandardMaterial>,
        flip: bool,
        standard: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        if !flip {
            return material.clone();
        }
        if let Some(h) = self.flipped.get(&material.id()) {
            return h.clone();
        }
        let Some(mut m) = standard.get(material).cloned() else {
            return material.clone();
        };
        flip_culling(&mut m);
        let h = standard.add(m);
        self.flipped.insert(material.id(), h.clone());
        h
    }

    /// robot.glb's gun (or gun arm) material `material`, named `name`, on its
    /// colour texture with the paint mask (Characters::gun_paint): the base its
    /// paint is made from (made once, kept), or None for one without (or without
    /// the characters).
    pub fn gun_base(
        &mut self,
        chars: &Characters,
        name: &str,
        material: &Handle<StandardMaterial>,
        standard: &mut Assets<StandardMaterial>,
    ) -> Option<Handle<StandardMaterial>> {
        let i = chars.gun_paint.iter().position(|(n, _)| n == name)?;
        let texture = chars.gun_paint_textures.get(i).cloned();
        let base = self.gun_paint.entry(material.id()).or_insert_with(|| {
            let mut gun = standard.get(material).cloned().unwrap_or_default();
            gun.base_color_texture = texture;
            standard.add(gun)
        });
        Some(base.clone())
    }
}

pub(crate) fn is_paint(extras: Option<&str>) -> bool {
    extras.is_some_and(|e| e.contains("\"paint\""))
}

// ---------------------------------------------------------------------------
// Dressing the models
// ---------------------------------------------------------------------------

/// On a robot model (a robot.glb scene): the look it wears ([`dress_robots`] puts
/// the parts on it).
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct Wear(pub Look);

/// The look a robot model has on, and what was spawned for it.
#[derive(Component)]
pub struct Worn {
    pub look: Look,
    parts: Vec<Entity>,
}

/// A part of a look as spawned onto a robot model: for a forearm (or a wheel on
/// one) its side (left: true) and whether it is the copy on the CloudB arm bones
/// (shown while switching hands, as the game's forearms ride those bones then).
#[derive(Component, Clone, Copy, Debug)]
pub struct WornPart {
    pub forearm: Option<bool>,
    pub cloud_b: bool,
}

/// On a vehicle model: the look whose vehicle it is (its scene is swapped for the
/// look's vehicle; its paint and wheels follow the look).
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct CarWear(pub Look);

/// What a vehicle model has on: its vehicle (index into [`Characters::vehicles`]),
/// the look, and the wheels spawned for it.
#[derive(Component)]
pub struct CarWorn {
    pub vehicle: usize,
    look: Option<Look>,
    /// What it has hung on its sockets (its wheels, cannons or wings, and its
    /// decals), taken off when its look changes.
    hung: Vec<Entity>,
}

/// A wheel hung on one of a vehicle's wheel sockets (car.rs turns, steers and
/// springs it): `rest` is its place on the bone; `axle` and `up`, in the bone's
/// frame, the wheel bone's Z and Y (the vehicle's across and up), which it turns
/// round and is steered and sprung by: the bone's own for every vehicle but
/// Swindle, whose wheels hang on bones of their own (his L_ and R_Robo07, 08, 10 and
/// 11, under the wheel bones), turned another way (wheel_axes).
#[derive(Component, Clone, Copy, Debug)]
pub struct VehicleWheel {
    pub front: bool,
    pub rest: Transform,
    pub axle: Vec3,
    pub up: Vec3,
}

impl VehicleWheel {
    /// One on a wheel bone itself.
    pub fn on_wheel_bone(front: bool, rest: Transform) -> Self {
        VehicleWheel {
            front,
            rest,
            axle: Vec3::Z,
            up: Vec3::Y,
        }
    }
}

/// Whether a vehicle model's bone is one of its wheel bones (`L_Wheel01_XB` and
/// the like: their Z the vehicle's across, their Y its up, in every vehicle).
pub fn is_wheel_bone(name: &str) -> bool {
    name.contains("_Wheel") && name.ends_with("_XB")
}

/// The wheel bone's axle (its Z) and up (its Y) in the frame of a bone under it,
/// from that bone's turn in the wheel bone's frame (`turn`).
pub fn wheel_axes(turn: Quat) -> (Vec3, Vec3) {
    let back = turn.inverse();
    (
        (back * Vec3::Z).normalize_or(Vec3::Z),
        (back * Vec3::Y).normalize_or(Vec3::Y),
    )
}

/// A wheel socket's bone's wheel bone (itself, or the one it hangs under), as the
/// vehicle model has them at rest (its nodes' own transforms, not where its clips
/// have them: Swindle's hover pose turns his wheels' bones flat): whether it is a
/// front one (Wheel01), and its axle and up in the socket bone's frame (wheel_axes).
fn wheel_frame(gltf: &Gltf, nodes: &Assets<GltfNode>, bone: &str) -> Option<(bool, Vec3, Vec3)> {
    let mut at = gltf.named_nodes.get(bone)?.clone();
    let mut turn = Quat::IDENTITY;
    // (Never more than a bone or two up.)
    for _ in 0..4 {
        let node = nodes.get(&at)?;
        if is_wheel_bone(&node.name) {
            let (axle, up) = wheel_axes(turn);
            return Some((node.name.contains("Wheel01"), axle, up));
        }
        turn = node.transform.rotation * turn;
        at = gltf
            .nodes
            .iter()
            .find(|h| nodes.get(*h).is_some_and(|n| n.children.contains(&at)))?
            .clone();
    }
    None
}

/// A part hung on one of a vehicle model's sockets that isn't a wheel (a jet's
/// wings, a tank's cannons), and the model: shadows.rs fits the model's shadow to
/// its pieces, these among them.
#[derive(Component, Clone, Copy, Debug)]
pub struct HungPart(pub Entity);

/// The scene the vehicle model shows (set as its SceneRoot).
#[derive(Component)]
struct CarScene(usize);

/// A painted mesh of a vehicle model: the material it is painted from.
#[derive(Component)]
struct PaintBase(Handle<StandardMaterial>);

/// On a robot or vehicle model: its energon glowing in this colour (linear) rather
/// than its look's faction's (customize.rs gives its robots the colour the game
/// gives them on its Custom Character screen).
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct GlowOverride(pub Vec3);

/// A painted mesh under a GlowOverride: the paint it has of its own, and the one
/// it shows.
#[derive(Component)]
struct GlowSwapped {
    own: Handle<PaintMaterial>,
    shown: AssetId<PaintMaterial>,
}

/// The paints made for GlowOverrides: by paint and colour (its bits).
type GlowCopies = HashMap<(AssetId<PaintMaterial>, [u32; 3]), Handle<PaintMaterial>>;

/// Each painted mesh under a GlowOverride shows a copy of its paint with its
/// energon in the override's colour (one copy made for each paint and colour),
/// or its own paint when that is its colour. A paint another system has put on
/// it in the colour already (as customize.rs's upgrade glow does, from the copy)
/// is left on.
fn override_glows(
    mut commands: Commands,
    models: Query<(Entity, &GlowOverride)>,
    children: Query<&Children>,
    meshes: Query<(&MeshMaterial3d<PaintMaterial>, Option<&GlowSwapped>)>,
    mut paints: ResMut<Assets<PaintMaterial>>,
    mut copies: Local<GlowCopies>,
) {
    if models.is_empty() {
        copies.clear();
        return;
    }
    for (model, glow) in &models {
        let color = glow.0;
        for e in children.iter_descendants(model) {
            let Ok((material, swapped)) = meshes.get(e) else {
                continue;
            };
            let current = material.0.id();
            let energon = |paints: &Assets<PaintMaterial>, id: AssetId<PaintMaterial>| {
                paints
                    .get(id)
                    .map(|m| m.extension.colors.energon.truncate())
            };
            // Its own paint: as swapped here, or a new one put on since.
            let own = match swapped {
                Some(s) if s.shown == current => s.own.clone(),
                _ => {
                    if energon(&paints, current) == Some(color) {
                        continue;
                    }
                    material.0.clone()
                }
            };
            let Some(own_energon) = paints.get(&own).map(|m| m.extension.colors.energon) else {
                continue;
            };
            let wanted = if own_energon.truncate() == color {
                own.clone()
            } else {
                let key = (own.id(), color.to_array().map(f32::to_bits));
                match copies.get(&key) {
                    Some(h) => h.clone(),
                    None => {
                        let Some(mut copy) = paints.get(&own).cloned() else {
                            continue;
                        };
                        copy.extension.colors.energon = color.extend(own_energon.w);
                        let h = paints.add(copy);
                        copies.insert(key, h.clone());
                        h
                    }
                }
            };
            if wanted.id() != current || swapped.is_none() {
                let shown = wanted.id();
                commands
                    .entity(e)
                    .try_insert((MeshMaterial3d(wanted), GlowSwapped { own, shown }));
            }
        }
    }
}

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        let chars = Characters::load(&crate::assets_dir());
        app.insert_resource(chars)
            .init_resource::<PaintCache>()
            .add_plugins(MaterialPlugin::<PaintMaterial>::default())
            .add_systems(Startup, load_parts)
            .add_systems(
                Update,
                (
                    reflect_surroundings,
                    (dress_robots, dress_cars),
                    override_glows,
                    gild_level_parts,
                    paint_pickups,
                )
                    .chain(),
            )
            .add_systems(Last, take_off)
            .add_observer(redress_respawned);
    }
}

/// A part coming off a model (robot.glb's own, or one the old look put on):
/// despawned at the end of the frame, after what other systems did to it this
/// frame (main.rs and customize.rs mark a model's new parts as it comes in).
#[derive(Component)]
struct TakenOff;

fn take_off(mut commands: Commands, off: Query<Entity, With<TakenOff>>) {
    for e in &off {
        commands.entity(e).try_despawn();
    }
}

/// A robot or vehicle model whose scene has been put in afresh (Bevy does that
/// when its file is loaded again, as robot.glb can be while the game starts) has
/// lost what it had on, so it is dressed again.
fn redress_respawned(
    ready: On<SceneInstanceReady>,
    mut commands: Commands,
    worn: Query<&Worn>,
    mut car_worn: Query<&mut CarWorn>,
) {
    let model = ready.entity;
    if let Ok(w) = worn.get(model) {
        for &e in &w.parts {
            commands.entity(e).try_insert(TakenOff);
        }
        commands.entity(model).try_remove::<Worn>();
    }
    if let Ok(mut w) = car_worn.get_mut(model) {
        w.look = None;
        w.hung.clear();
    }
}

fn load_parts(
    asset_server: Res<AssetServer>,
    (mut parts, images): (ResMut<crate::gltffiles::GltfParts>, Res<Assets<Image>>),
    mut chars: ResMut<Characters>,
    mut cache: ResMut<PaintCache>,
) {
    // (The energon cube's picture: its file read once, gltffiles.rs.)
    cache.noise = parts.texture(&asset_server, &images, ENERGON_NOISE_MODEL, 0);
    match std::fs::read_to_string(crate::assets_dir().join(PAINT_VALUES)) {
        Ok(text) => cache.table = PaintTable::read(&text),
        Err(_) => warn!(
            "No {PAINT_VALUES} (tools/build_paint.py makes it): the parts are shaded with \
             TR_BRDF_CustChar_TEXSET_MAT's own values"
        ),
    }
    for (i, class) in chars.classes.iter_mut().enumerate() {
        if class.available {
            class.gltf = asset_server.load(parts_model(i));
        }
    }
    chars.vehicle_files = chars
        .vehicles
        .iter()
        .map(|v| asset_server.load(v.model.clone()))
        .collect();
    if !chars.upgrade_parts.is_empty() {
        chars.upgrades_gltf = asset_server.load(UPGRADES_MODEL);
    }
    // (Wrapping as robot.glb's own textures do: glTF's default.)
    chars.gun_paint_textures = chars
        .gun_paint
        .iter()
        .map(|(_, texture)| {
            asset_server.load_with_settings(texture.clone(), |s: &mut ImageLoaderSettings| {
                s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    ..ImageSamplerDescriptor::linear()
                });
            })
        })
        .collect();
}

/// A part of the map shaded as the characters are (glTF material extras
/// "brdf:<values>", tools/build_map.py's gilded_flag): coloured metal, Ignition's gold
/// statue of Starscream (TR_Starscream_*_MAT, ENV_BRDF_A_MAT with CoefficientColor_C
/// (2, 1, 0.11)), which the game's shading makes shine gold and reflect the arena.
/// Its own textures, unpainted (paint.shade.w 1: the level's texture sets keep their
/// gloss mask as build_level.py's metal, 0.2 + 0.5 x green), its material's values
/// in the paint's place: Roughness Power, Roughness Offset, SmoothStep_Min and _Max,
/// DiffuseIntensityScalar and _Add, CoefficientColor_C, MetallicOffset (as the
/// metallic slider) and the reflection's strength. It keeps its own glow.
#[allow(clippy::type_complexity)]
fn gild_level_parts(
    mut commands: Commands,
    parts: Query<
        (
            Entity,
            &GltfMaterialExtras,
            &MeshMaterial3d<StandardMaterial>,
        ),
        Added<GltfMaterialExtras>,
    >,
    standard: Res<Assets<StandardMaterial>>,
    mut paints: ResMut<Assets<PaintMaterial>>,
    cache: Res<PaintCache>,
    mut made: Local<HashMap<AssetId<StandardMaterial>, Handle<PaintMaterial>>>,
) {
    for (entity, extras, material) in &parts {
        let Some(values) = gilded_values(&extras.value) else {
            continue;
        };
        let paint = match made.get(&material.id()) {
            Some(p) => p.clone(),
            None => {
                let Some(base) = standard.get(material.id()) else {
                    continue;
                };
                let mut base = base.clone();
                // (The shader reads the gloss mask from the metal channel.)
                base.metallic = 1.0;
                let p = paints.add(PaintMaterial {
                    base,
                    extension: Paint::new(
                        gilded_colors(&values, cache.reflection_lod()),
                        Some(cache.noise.clone()),
                        cache.reflection.as_ref().map(|(h, _)| h.clone()),
                    ),
                });
                made.insert(material.id(), p.clone());
                info!("A part of the map shaded as the characters are: {values:?}");
                p
            }
        };
        commands
            .entity(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(paint));
    }
}

/// A "brdf:" flag's values (gild_level_parts), all eleven.
fn gilded_values(extras: &str) -> Option<[f32; 11]> {
    let flag = crate::lighting::parse_flags(extras)?
        .split_whitespace()
        .find_map(|f| f.strip_prefix("brdf:"))?;
    let v: Vec<f32> = flag.split(':').filter_map(|x| x.parse().ok()).collect();
    <[f32; 11]>::try_from(v).ok()
}

/// The paint of a gilded part of the map (gild_level_parts) from its values.
fn gilded_colors(v: &[f32; 11], reflection_lod: f32) -> PaintColors {
    PaintColors {
        primary: Vec4::new(1.0, 1.0, 1.0, v[9]),
        secondary: Vec4::ONE.with_w(0.0),
        energon: Vec4::ZERO,
        res_in: Vec4::ZERO,
        deres: PaintColors::WHOLE,
        deres_fx: Vec4::ZERO,
        deres_fresnel: Vec4::ZERO,
        deres_lead: Vec4::ZERO,
        deres_burn: Vec4::ZERO,
        deres_energon: Vec4::ZERO,
        gloss: Vec4::new(v[0], v[1], v[2], v[3]),
        shade: Vec4::new(v[4], v[5], reflection_lod, 1.0),
        coefficient: Vec4::new(v[6], v[7], v[8], v[10]),
        cloak: PaintColors::WHOLE,
        cloak_fx: Vec4::ZERO,
    }
}

/// A weapon pickup's paint (tools/build_assets.py's build_pickup: the "pickup_paint"
/// extras on its gun's and cuff's materials): the robot.glb material whose colour
/// texture with the paint mask it is painted on (Characters' gunpaint lines), the
/// material instance whose values shade it (paint.txt: the gun's PICKUP_*_MATINST,
/// the cuff's own RB_ArmUp_Cust_MatINST_INST), its paint (Cust_Color_A and _B: a
/// grey) and its MetallicOffset.
#[derive(Clone, Debug, PartialEq)]
pub struct PickupPaint {
    pub gunpaint: String,
    pub brdf: String,
    pub paint: f32,
    pub metallic: f32,
}

impl PickupPaint {
    /// The paint in a material's extras (glTF JSON), if it has one.
    pub fn parse(extras: &str) -> Option<PickupPaint> {
        let at = extras.find("\"pickup_paint\"")?;
        let rest = &extras[at..];
        let text = |key: &str| -> Option<String> {
            let after = &rest[rest.find(&format!("\"{key}\""))? + key.len() + 2..];
            let start = after.find('"')? + 1;
            let end = start + after[start..].find('"')?;
            Some(after[start..end].to_string())
        };
        let number = |key: &str| -> Option<f32> {
            let after = &rest[rest.find(&format!("\"{key}\""))? + key.len() + 2..];
            let after = after.trim_start().strip_prefix(':')?.trim_start();
            let end = after
                .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | 'e' | 'E' | '+')))
                .unwrap_or(after.len());
            after[..end].parse().ok()
        };
        Some(PickupPaint {
            gunpaint: text("gunpaint")?,
            brdf: text("brdf")?,
            paint: number("paint")?,
            metallic: number("metallic")?,
        })
    }
}

/// The weapon pickups (models/weapons/*.glb: on the weapon stations, in the pods),
/// shaded as the game shades them: their gun's TnWeaponMesh's PickupMaterialOverride
/// (a PICKUP_*_MATINST) is an instance of the characters' material, and the cuff the
/// gun is mounted on keeps its own (RB_ArmUp_Cust_MatINST_INST), so each is drawn as
/// a gun in hand is (paint.wgsl), painted the pickups' grey (PickupPaint). Without
/// the characters' files they keep their plain look.
#[allow(clippy::type_complexity)]
fn paint_pickups(
    mut commands: Commands,
    parts: Query<
        (
            Entity,
            &GltfMaterialExtras,
            &MeshMaterial3d<StandardMaterial>,
        ),
        Added<GltfMaterialExtras>,
    >,
    chars: Res<Characters>,
    mut cache: ResMut<PaintCache>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut paints: ResMut<Assets<PaintMaterial>>,
) {
    for (entity, extras, material) in &parts {
        let Some(paint) = PickupPaint::parse(&extras.value) else {
            continue;
        };
        let Some(base) = cache.gun_base(&chars, &paint.gunpaint, &material.0, &mut standard) else {
            continue;
        };
        if let Some(p) = cache.pickup(&base, &paint, &standard, &mut paints) {
            commands
                .entity(entity)
                .try_remove::<MeshMaterial3d<StandardMaterial>>()
                .try_insert(MeshMaterial3d(p));
        }
    }
}

/// The cube map the characters reflect (paint.wgsl): the map in play's (its
/// lighting file's "reflect" line: the level's captured cube map, or the game's
/// default, rommCubeMap), or the Customize garage's while that screen is up (it
/// covers the map). Every paint is given it when that changes.
fn reflect_surroundings(
    garage: Option<Res<crate::levelfx::GarageScene>>,
    map: Option<Res<crate::lighting::MapLighting>>,
    asset_server: Res<AssetServer>,
    mut cache: ResMut<PaintCache>,
    mut paints: ResMut<Assets<PaintMaterial>>,
    mut garage_cube: Local<Option<Option<(String, u32)>>>,
    mut shown: Local<Option<Option<String>>>,
) {
    let wanted = if garage.is_some_and(|g| g.0) {
        garage_cube
            .get_or_insert_with(|| {
                crate::lighting::MapLighting::load_file(
                    &crate::assets_dir(),
                    crate::customize::LIGHTS_FILE,
                )
                .and_then(|l| l.reflect)
            })
            .clone()
    } else {
        map.and_then(|m| m.reflect.clone())
    };
    let key = wanted.as_ref().map(|(path, _)| path.clone());
    if shown.as_ref() == Some(&key) {
        return;
    }
    *shown = Some(key);
    cache.reflection = wanted.map(|(path, mips)| {
        let cube = asset_server.load_with_settings(path, |s: &mut ImageLoaderSettings| {
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                ..default()
            });
        });
        (cube, mips.saturating_sub(1) as f32)
    });
    let cube = cache.reflection.as_ref().map(|(h, _)| h.clone());
    let lod = cache.reflection_lod();
    for (_, paint) in paints.iter_mut() {
        paint.extension.reflection = cube.clone();
        paint.extension.colors.shade.z = lod;
    }
}

/// Every named entity under `root`.
fn named_descendants(
    root: Entity,
    children: &Query<&Children>,
    names: &Query<&Name>,
) -> HashMap<String, Entity> {
    let mut out = HashMap::new();
    for e in children.iter_descendants(root) {
        if let Ok(n) = names.get(e) {
            out.entry(n.as_str().to_string()).or_insert(e);
        }
    }
    out
}

/// A part's material: painted in the look's colours, or as it is.
enum Mat {
    Paint(Handle<PaintMaterial>),
    Plain(Handle<StandardMaterial>),
}

fn part_material(
    prim: &bevy::gltf::GltfPrimitive,
    look: &Look,
    flip: bool,
    cache: &mut PaintCache,
    standard: &mut Assets<StandardMaterial>,
    paints: &mut Assets<PaintMaterial>,
) -> Option<Mat> {
    let m = prim.material.as_ref()?;
    let paint = is_paint(prim.material_extras.as_ref().map(|e| e.value.as_str()));
    Some(
        match paint
            .then(|| cache.get(m, look, flip, true, standard, paints))
            .flatten()
        {
            Some(p) => Mat::Paint(p),
            None => Mat::Plain(cache.plain(m, flip, standard)),
        },
    )
}

fn insert_material(e: &mut EntityCommands, m: Option<Mat>) {
    match m {
        Some(Mat::Paint(p)) => {
            e.insert(MeshMaterial3d(p));
        }
        Some(Mat::Plain(p)) => {
            e.insert(MeshMaterial3d(p));
        }
        None => {}
    }
}

/// robot.glb's own Jazz: his part meshes, their CloudB copies and his wheels (the
/// parts model's take their place).
fn is_builtin_part(name: &str) -> bool {
    name.starts_with("RB_Jazz_CHARSKEL_") || (name.ends_with("_Wheel") && !name.starts_with("RB_"))
}

/// Puts each robot model's look on it once its scene is in (and again when the
/// look changes): the parts' meshes skinned to its bones, the forearms also on the
/// CloudB bones, the wheels on the parts' sockets, all in the look's paint.
#[allow(clippy::too_many_arguments)]
fn dress_robots(
    mut commands: Commands,
    chars: Res<Characters>,
    gltfs: Res<Assets<Gltf>>,
    gltf_nodes: Res<Assets<GltfNode>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_skins: Res<Assets<GltfSkin>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut paints: ResMut<Assets<PaintMaterial>>,
    mut cache: ResMut<PaintCache>,
    robots: Query<(Entity, &Wear, Option<&Worn>)>,
    children: Query<&Children>,
    names: Query<&Name>,
    gun_arms: Query<(&GltfMaterialName, &MeshMaterial3d<StandardMaterial>)>,
    bases: Query<&PaintBase>,
    decal_assets: Res<crate::decals::DecalAssets>,
) {
    for (robot, wear, worn) in &robots {
        let look = wear.0;
        if worn.is_some_and(|w| w.look == look) {
            continue;
        }
        // (A look with decals waits for their model.)
        if look.decals != 0 && !crate::decals::ready(&decal_assets, &gltfs) {
            continue;
        }
        // Its class's parts (once that file is in).
        let class = chars.class(look.class());
        if !class.available {
            continue;
        }
        let Some(gltf) = gltfs.get(&class.gltf) else {
            continue;
        };
        // (And the upgrades' parts, once their model is in.)
        let upgrades_gltf = gltfs.get(&chars.upgrades_gltf);
        if !chars.upgrade_parts.is_empty() && upgrades_gltf.is_none() {
            continue;
        }
        let named = named_descendants(robot, &children, &names);
        let (Some(_), Some(&model_root)) = (named.get(SKELETON_ROOT), named.get(MODEL_ROOT)) else {
            continue;
        };
        // Off with the old.
        if let Some(worn) = worn {
            for &e in &worn.parts {
                commands.entity(e).try_insert(TakenOff);
            }
        }
        for (name, &e) in &named {
            if is_builtin_part(name) {
                commands.entity(e).try_insert(TakenOff);
            }
        }
        let mut spawned = Vec::new();
        // The parts' skin is the robot skeleton: its joints are the model's bones.
        let joints_for = |skin: &GltfSkin, cloud_b: bool| -> Option<Vec<Entity>> {
            skin.joints
                .iter()
                .map(|h| {
                    let name = &gltf_nodes.get(h)?.name;
                    let name = if cloud_b {
                        name.replace("_CloudA_", "_CloudB_")
                    } else {
                        name.clone()
                    };
                    named.get(&name).copied()
                })
                .collect()
        };
        let (wheel, wheel_socket) = match chars.wheel(&look) {
            Some((mesh, socket)) => (gltf.named_nodes.get(mesh).cloned(), socket.to_string()),
            None => (None, String::new()),
        };
        for kind in 0..PART_TYPES.len() {
            let Some(part) = chars.part(&look, kind) else {
                continue;
            };
            for mesh_name in &part.meshes {
                let Some(node) = gltf
                    .named_nodes
                    .get(mesh_name.as_str())
                    .and_then(|h| gltf_nodes.get(h))
                else {
                    warn!("{mesh_name} isn't in {}", parts_model(look.class()));
                    continue;
                };
                let (Some(mesh), Some(skin)) = (
                    node.mesh.as_ref().and_then(|h| gltf_meshes.get(h)),
                    node.skin.as_ref().and_then(|h| gltf_skins.get(h)),
                ) else {
                    continue;
                };
                // A forearm's side, from its name (..._ArmLoL / ..._ArmLoR).
                let forearm = (kind == ARM_LO).then(|| mesh_name.ends_with('L'));
                for cloud_b in [false, true] {
                    if cloud_b && forearm.is_none() {
                        break;
                    }
                    let Some(joints) = joints_for(skin, cloud_b) else {
                        warn!("{mesh_name}: the robot model lacks some of its bones");
                        break;
                    };
                    for prim in &mesh.primitives {
                        let mut e = commands.spawn((
                            Mesh3d(prim.mesh.clone()),
                            SkinnedMesh {
                                inverse_bindposes: skin.inverse_bind_matrices.clone(),
                                joints: joints.clone(),
                            },
                            Transform::default(),
                            if cloud_b {
                                Visibility::Hidden
                            } else {
                                Visibility::Inherited
                            },
                            Name::new(if cloud_b {
                                format!("{mesh_name}_CloudB")
                            } else {
                                mesh_name.clone()
                            }),
                            WornPart { forearm, cloud_b },
                            ChildOf(model_root),
                        ));
                        insert_material(
                            &mut e,
                            part_material(
                                prim,
                                &look,
                                false,
                                &mut cache,
                                &mut standard,
                                &mut paints,
                            ),
                        );
                        spawned.push(e.id());
                    }
                }
                // The wheels on its sockets.
                let Some(wheel_mesh) = wheel
                    .as_ref()
                    .and_then(|h| gltf_nodes.get(h))
                    .and_then(|n| n.mesh.as_ref())
                    .and_then(|h| gltf_meshes.get(h))
                else {
                    continue;
                };
                for s in class.sockets.get(mesh_name.as_str()).into_iter().flatten() {
                    if !s.name.to_ascii_lowercase().starts_with(&wheel_socket) {
                        continue;
                    }
                    let copies: &[bool] = if forearm.is_some() && s.bone.contains("_CloudA_") {
                        &[false, true]
                    } else {
                        &[false]
                    };
                    for &cloud_b in copies {
                        let bone = if cloud_b {
                            s.bone.replace("_CloudA_", "_CloudB_")
                        } else {
                            s.bone.clone()
                        };
                        let Some(&bone_entity) = named.get(&bone) else {
                            continue;
                        };
                        for prim in &wheel_mesh.primitives {
                            let mut e = commands.spawn((
                                Mesh3d(prim.mesh.clone()),
                                s.transform,
                                if cloud_b {
                                    Visibility::Hidden
                                } else {
                                    Visibility::Inherited
                                },
                                Name::new(format!("{bone}_Wheel")),
                                WornPart { forearm, cloud_b },
                                ChildOf(bone_entity),
                            ));
                            insert_material(
                                &mut e,
                                part_material(
                                    prim,
                                    &look,
                                    mirrors(&s.transform),
                                    &mut cache,
                                    &mut standard,
                                    &mut paints,
                                ),
                            );
                            spawned.push(e.id());
                        }
                    }
                }
            }
        }
        // The decal set's decals on the parts' decal sockets (the chests have
        // them): each decal's quad on every socket whose name starts with its own.
        if look.decals != 0 {
            for kind in 0..PART_TYPES.len() {
                let Some(part) = chars.part(&look, kind) else {
                    continue;
                };
                for mesh_name in &part.meshes {
                    spawned.extend(crate::decals::hang(
                        &mut commands,
                        &chars,
                        &decal_assets,
                        (&*gltfs, &*gltf_nodes, &*gltf_meshes),
                        look.decals,
                        look.decepticon,
                        class.sockets.get(mesh_name.as_str()).into_iter().flatten(),
                        &named,
                    ));
                }
            }
        }
        // robot.glb's guns and the arm in them, in the paint too (each on its own
        // texture with the paint mask; its first material kept as PaintBase).
        if !chars.gun_paint.is_empty() {
            for e in children.iter_descendants(robot) {
                let base = match (bases.get(e), gun_arms.get(e)) {
                    (Ok(b), _) => b.0.clone(),
                    (Err(_), Ok((name, m))) => {
                        let Some(base) = cache.gun_base(&chars, &name.0, &m.0, &mut standard)
                        else {
                            continue;
                        };
                        commands.entity(e).try_insert(PaintBase(base.clone()));
                        base
                    }
                    _ => continue,
                };
                if let Some(p) = cache.get(&base, &look, false, false, &standard, &mut paints) {
                    commands
                        .entity(e)
                        .try_remove::<MeshMaterial3d<StandardMaterial>>()
                        .try_insert(MeshMaterial3d(p));
                }
            }
        }
        // The upgrades' parts on every gun's sockets, each hidden until its gun
        // shows with the upgrade on it (a part on a CloudA bone also on the CloudB
        // one, for the gun folding away), in the look's paint (the game paints them:
        // ApplyColorsToMesh), culled the other way round where the socket mirrors it.
        for place in &chars.upgrade_parts {
            let Some(mesh) = upgrades_gltf
                .and_then(|g| g.named_nodes.get(place.part.as_str()))
                .and_then(|h| gltf_nodes.get(h))
                .and_then(|n| n.mesh.as_ref())
                .and_then(|h| gltf_meshes.get(h))
            else {
                continue;
            };
            let copies: &[bool] = if place.bone.contains("_CloudA_") {
                &[false, true]
            } else {
                &[false]
            };
            for &cloud_b in copies {
                let bone = if cloud_b {
                    place.bone.replace("_CloudA_", "_CloudB_")
                } else {
                    place.bone.clone()
                };
                let Some(&bone_entity) = named.get(&bone) else {
                    continue;
                };
                for prim in &mesh.primitives {
                    let mut e = commands.spawn((
                        Mesh3d(prim.mesh.clone()),
                        place.transform,
                        Visibility::Hidden,
                        Name::new(format!("{}_Upgrade", place.part)),
                        UpgradePart {
                            upgrade: place.upgrade,
                            weapon: place.weapon,
                            left: place.left,
                            cloud_b,
                        },
                        ChildOf(bone_entity),
                    ));
                    insert_material(
                        &mut e,
                        part_material(
                            prim,
                            &look,
                            mirrors(&place.transform),
                            &mut cache,
                            &mut standard,
                            &mut paints,
                        ),
                    );
                    spawned.push(e.id());
                }
            }
        }
        commands.entity(robot).try_insert(Worn {
            look,
            parts: spawned,
        });
    }
}

/// Keeps each vehicle model in its look's vehicle: the scene swapped when the
/// chest's vehicle changes, its painted materials in the look's paint, the look's
/// wheels on its wheel sockets.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dress_cars(
    mut commands: Commands,
    chars: Res<Characters>,
    scene_spawner: Res<SceneSpawner>,
    gltfs: Res<Assets<Gltf>>,
    gltf_nodes: Res<Assets<GltfNode>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut paints: ResMut<Assets<PaintMaterial>>,
    mut cache: ResMut<PaintCache>,
    mut cars: Query<(
        Entity,
        &CarWear,
        Option<&mut CarWorn>,
        Option<&CarScene>,
        Option<&SceneInstance>,
    )>,
    children: Query<&Children>,
    names: Query<&Name>,
    painted: Query<(&MeshMaterial3d<StandardMaterial>, &GltfMaterialExtras)>,
    bases: Query<&PaintBase>,
    decal_assets: Res<crate::decals::DecalAssets>,
) {
    for (car, wear, worn, scene, instance) in &mut cars {
        let look = wear.0;
        let Some(vehicle) = chars.vehicle(&look) else {
            continue;
        };
        // (A look with decals waits for their model.)
        if look.decals != 0 && !crate::decals::ready(&decal_assets, &gltfs) {
            continue;
        }
        // The look's vehicle model (once its file is in), but not while the one
        // it shows is still being put in: Bevy would put that in as well, beside
        // the new one, and never take it out.
        if scene.map(|s| s.0) != Some(vehicle) {
            if scene.is_some() && !instance.is_some_and(|i| scene_spawner.instance_is_ready(**i)) {
                continue;
            }
            let Some(model) = chars
                .vehicle_files
                .get(vehicle)
                .and_then(|h| gltfs.get(h))
                .and_then(|g| g.scenes.first())
            else {
                continue;
            };
            commands.entity(car).try_insert((
                SceneRoot(model.clone()),
                CarScene(vehicle),
                CarWorn {
                    vehicle,
                    look: None,
                    hung: Vec::new(),
                },
            ));
            continue;
        }
        let Some(mut worn) = worn else {
            continue;
        };
        if worn.look == Some(look) {
            continue;
        }
        let named = named_descendants(car, &children, &names);
        if !named.contains_key(VEHICLE_ROOT) {
            continue;
        }
        // (The wheels are its class's parts'.)
        let Some(gltf) = gltfs.get(&chars.class(look.class()).gltf) else {
            continue;
        };
        // Paint: each painted material in the look's colours (from its own
        // material, kept as PaintBase once painted).
        for e in children.iter_descendants(car) {
            let base = match (bases.get(e), painted.get(e)) {
                (Ok(b), _) => b.0.clone(),
                (Err(_), Ok((m, extras))) if is_paint(Some(&extras.value)) => {
                    commands.entity(e).try_insert(PaintBase(m.0.clone()));
                    m.0.clone()
                }
                _ => continue,
            };
            if let Some(p) = cache.get(&base, &look, false, true, &standard, &mut paints) {
                commands
                    .entity(e)
                    .try_remove::<MeshMaterial3d<StandardMaterial>>()
                    .try_insert(MeshMaterial3d(p));
            }
        }
        // Wheels.
        for &e in &worn.hung {
            commands.entity(e).try_insert(TakenOff);
        }
        worn.hung.clear();
        let wheel = chars.vehicle_wheel(&look);
        let wheel_mesh = wheel
            .and_then(|(w, _)| gltf.named_nodes.get(w))
            .and_then(|h| gltf_nodes.get(h))
            .and_then(|n| n.mesh.as_ref())
            .and_then(|h| gltf_meshes.get(h));
        let socket = wheel.map_or("", |(_, s)| s);
        let vehicle_gltf = chars.vehicle_files.get(vehicle).and_then(|h| gltfs.get(h));
        if let Some(mesh) = wheel_mesh {
            for s in &chars.vehicles[vehicle].wheel_sockets {
                // (Its own kind's sockets: a tank's cannons don't turn like wheels.)
                let name = s.name.to_ascii_lowercase();
                if !name.starts_with(socket) {
                    continue;
                }
                let Some(&bone) = named.get(&s.bone) else {
                    continue;
                };
                for prim in &mesh.primitives {
                    let mut e = commands.spawn((
                        Mesh3d(prim.mesh.clone()),
                        s.transform,
                        Visibility::Inherited,
                        ChildOf(bone),
                    ));
                    if socket == "wheel" {
                        let (front, axle, up) = vehicle_gltf
                            .and_then(|g| wheel_frame(g, &gltf_nodes, &s.bone))
                            .unwrap_or((s.bone.contains("Wheel01"), Vec3::Z, Vec3::Y));
                        e.insert((
                            Name::new(format!("{}_Tyre", s.name)),
                            VehicleWheel {
                                front,
                                rest: s.transform,
                                axle,
                                up,
                            },
                        ));
                    } else {
                        e.insert((Name::new(format!("{}_Part", s.name)), HungPart(car)));
                    }
                    insert_material(
                        &mut e,
                        part_material(
                            prim,
                            &look,
                            mirrors(&s.transform),
                            &mut cache,
                            &mut standard,
                            &mut paints,
                        ),
                    );
                    worn.hung.push(e.id());
                }
            }
        }
        // The decal set's decals on its decal sockets.
        let decals = crate::decals::hang(
            &mut commands,
            &chars,
            &decal_assets,
            (&*gltfs, &*gltf_nodes, &*gltf_meshes),
            look.decals,
            look.decepticon,
            &chars.vehicles[vehicle].decal_sockets,
            &named,
        );
        worn.hung.extend(decals);
        worn.look = Some(look);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The wheels turn round their wheel bones' axles whatever bone they hang on:
    /// on a wheel bone, its own Z and Y, as always; on Swindle's (his L_ and
    /// R_Robo08_XT as his truck has them at rest under L_ and R_Wheel01_XB, their
    /// sockets turning the tyre's axle, its Y, onto their X), their -X, and their Y
    /// on the left and -Y on the right. Turned as far, they roll as a wheel bone's.
    #[test]
    fn swindles_wheels_roll_round_his_trucks_axles() {
        let close = |a: Vec3, b: Vec3| (a - b).length() < 5e-2;
        assert_eq!(wheel_axes(Quat::IDENTITY), (Vec3::Z, Vec3::Y));
        assert!(is_wheel_bone("R_Wheel02_XB") && !is_wheel_bone("L_Robo08_XT"));
        let left = Quat::from_xyzw(0.008, 0.707, -0.008, 0.707).normalize();
        let right = Quat::from_xyzw(0.707, -0.008, -0.707, -0.008).normalize();
        let socket = Quat::from_xyzw(0.0, 0.0, 0.707107, 0.707107);
        for (turn, up) in [(left, Vec3::Y), (right, Vec3::NEG_Y)] {
            let (axle, wheel_up) = wheel_axes(turn);
            assert!(close(axle, Vec3::NEG_X), "{axle}");
            assert!(close(wheel_up, up), "{wheel_up}");
            assert!(close(turn * axle, Vec3::Z) && close(turn * wheel_up, Vec3::Y));
            // (The tyre's own axle along it, so that it turns round its middle.)
            assert!((socket * Vec3::Y).dot(axle).abs() > 0.999);
            for v in [Vec3::X, Vec3::Y, Vec3::new(0.3, -0.5, 0.8)] {
                let rolled = turn * (Quat::from_axis_angle(axle, -0.7) * (turn.inverse() * v));
                assert!(close(rolled, Quat::from_rotation_z(-0.7) * v), "{rolled}");
            }
        }
    }

    /// A weapon pickup's paint comes from its material's extras, whatever order the
    /// glTF loader gives the keys in; other extras have none.
    #[test]
    fn a_pickup_reads_its_paint_from_its_material() {
        let want = PickupPaint {
            gunpaint: "WEP_Shotgun_MatINST".into(),
            brdf: "PICKUP_Shotgun_MatINST".into(),
            paint: 0.3961,
            metallic: 0.4,
        };
        let written = r#"{"pickup_paint": {"gunpaint": "WEP_Shotgun_MatINST", "brdf": "PICKUP_Shotgun_MatINST", "paint": 0.3961, "metallic": 0.4}}"#;
        let sorted = r#"{"pickup_paint":{"brdf":"PICKUP_Shotgun_MatINST","gunpaint":"WEP_Shotgun_MatINST","metallic":0.4,"paint":0.3961}}"#;
        assert_eq!(PickupPaint::parse(written), Some(want.clone()));
        assert_eq!(PickupPaint::parse(sorted), Some(want));
        assert_eq!(PickupPaint::parse(r#"{"foc":"noshadow nofog"}"#), None);
        // The pickups' values are in the paint table by their material instance.
        let table =
            PaintTable::read("brdf PICKUP_Shotgun_MatINST 110 0.06 0.05 0.74 1 0 0.72 0.72 0.72\n");
        let v = table.named("PICKUP_Shotgun_MatINST").unwrap();
        assert_eq!(v.gloss, Vec4::new(110.0, 0.06, 0.05, 0.74));
        assert!(table.named("PICKUP_Nothing").is_none());
    }

    #[test]
    fn a_part_takes_its_material_values_by_its_colour_texture() {
        let table = PaintTable::read(
            "# comment\n\
             default 600 0.05 0 0.7 1 0 0.72 0.72 0.72\n\
             brdf RB_Jazz_Chest_TEXSET 200 0.04 0 0.5 0.75 0.1 0.75 0.75 0.75\n\
             brdf Broken 1 2 3\n",
        );
        let jazz = table.values(Some(Path::new(
            "models/infiltrator/textures/RB_Jazz_Chest_TEXSET_c.ktx2",
        )));
        assert_eq!(jazz.gloss, Vec4::new(200.0, 0.04, 0.0, 0.5));
        assert_eq!(jazz.diffuse, Vec2::new(0.75, 0.1));
        assert_eq!(jazz.coefficient, Vec3::splat(0.75));
        // One the table doesn't have (or a broken line's), or no texture: the
        // default line's.
        let other = table.values(Some(Path::new("textures/Something_c.ktx2")));
        assert_eq!(other.gloss, Vec4::new(600.0, 0.05, 0.0, 0.7));
        assert_eq!(table.values(None), other);
        assert_eq!(table.values(Some(Path::new("Broken_c.ktx2"))), other);
        // Without one: the game's character material's own.
        assert_eq!(PaintTable::read("").values(None), PaintValues::DEFAULT);
    }

    #[test]
    fn the_gilded_statue_reads_its_material_values() {
        // (tools/build_map.py's gilded_flag for Ignition's TR_Starscream_*_MAT.)
        let extras = r#"{"foc":"brdf:300:0.21961:0.07451:1:0.15686:0:2:1:0.10985:0.68628:2"}"#;
        let v = gilded_values(extras).expect("eleven values");
        let colors = gilded_colors(&v, 7.0);
        assert_eq!(colors.gloss, Vec4::new(300.0, 0.21961, 0.07451, 1.0));
        assert_eq!(colors.shade, Vec4::new(0.15686, 0.0, 7.0, 1.0));
        assert_eq!(colors.coefficient, Vec4::new(2.0, 1.0, 0.10985, 2.0));
        assert_eq!(colors.primary, Vec4::new(1.0, 1.0, 1.0, 0.68628));
        assert_eq!(colors.deres.w, -1.0);
        // Other flags beside it are passed over; a short or missing one is no gilding.
        assert!(
            gilded_values(r#"{"foc":"noshadow brdf:300:0.2:0:1:0.15:0:2:1:0.1:0.7:2"}"#).is_some()
        );
        assert!(gilded_values(r#"{"foc":"brdf:300:0.2:0:1"}"#).is_none());
        assert!(gilded_values(r#"{"foc":"nofog"}"#).is_none());
    }

    /// Each vehicle's car is drawn up among its robot's pieces as they swap
    /// (vtransform: tools/transform_fit.py), never lower than its clips put it;
    /// The vehicles' cameras are Escalation's (its characters' camera sets:
    /// build_class.py's vehicle_camera_line, class_data.vehicle_camera_set): Optimus
    /// Prime's truck hovers with Truck_Optimus_CAMSET's camera, 7 m out with its aim
    /// 0.35 m down in the middle of its pitch range (multiplayer's Truck_MP_CAMSET
    /// had it 9 m out) and the view kept within 25 degrees of level, and Ironhide's
    /// (no Escalation character) the same over his own truck; Swindle's with
    /// Truck_ALT_CAMSET's, 7.25 m;
    /// G1 Optimus keeps the multiplayer set's camera made for him, 6.5 m. Warpath's
    /// tank hovers with Tank_Megatron_CAMSET's, 8.25 m back and 2.1 m up with the
    /// view level, 75 degrees wide; Brawl's with Tank_SP_ALT_CAMSET's, 8.5 m back.
    /// Starscream's jet hovers at 75 degrees, its view free within 80 degrees of
    /// level, and flies at 100. Each is anchored over its vehicle's middle, where the
    /// game's pawn is (the middle of its mesh's bounds: Optimus's truck's 1.45 m up,
    /// not its rigid body's bone's 0.42 m), and its aim's offsets run on the game's
    /// curve (level, Optimus's 0.26 m down, not 0.18 m on straight lines).
    #[test]
    fn the_vehicles_cameras_are_escalations() {
        let assets = crate::assets_dir();
        if !(0..CLASSES.len()).all(|c| class_files_present(&assets, c)) {
            return;
        }
        let chars = Characters::load(&assets);
        let form = |name: &str| {
            chars
                .vehicles
                .iter()
                .find(|v| v.form == name)
                .unwrap_or_else(|| panic!("{name}"))
        };
        let hover = |name: &str| form(name).camera.strategies.expect(name)[1];
        let optimus = hover("OptimusPrimeTruckForm");
        assert_eq!(optimus.distance.get(0), 7.0);
        assert_eq!(optimus.distance.get(cam_state::HOVER_STRAFING), 8.5);
        assert!((optimus.curve.get(0).at(5.0).z + 0.35).abs() < 1e-4);
        assert!((optimus.curve.get(0).at(0.0).z + 0.2616).abs() < 1e-3);
        assert!((optimus.curve.get(0).at(-20.0).z - 0.5).abs() < 1e-4);
        assert_eq!(optimus.pitch_range, (-25.0, 25.0));
        let middle = |name: &str| form(name).camera.middle.expect(name);
        let close =
            |(a, b): (f32, f32), (x, y): (f32, f32)| (a - x).abs() < 0.01 && (b - y).abs() < 0.01;
        assert!(close(middle("OptimusPrimeTruckForm"), (1.454, -0.044)));
        assert!((optimus.origin - 1.454).abs() < 0.01, "{}", optimus.origin);
        assert!(close(middle("StarScreamVehicleForm"), (1.362, 0.126)));
        assert!(close(middle("WarpathVehicleForm"), (1.459, 0.235)));
        assert!(close(middle("BumbleBeeVehicleForm"), (0.671, 0.020)));
        // (But for its own height over the ground.)
        let ironhide = hover("IronhideTruckForm");
        assert_eq!(
            CamStrategy {
                origin: optimus.origin,
                ..ironhide
            },
            optimus
        );
        assert_eq!(hover("SwindleTruckForm").distance.get(0), 7.25);
        assert_eq!(hover("OptimusG1TruckForm").distance.get(0), 6.5);
        let warpath = form("WarpathVehicleForm").camera;
        assert!((warpath.hover_distance - 8.25).abs() < 1e-3, "{warpath:?}");
        assert!((warpath.hover_up - 2.102).abs() < 1e-3, "{warpath:?}");
        assert_eq!(warpath.fov, Some((75.0, 75.0)));
        assert!((form("BrawlVehicleForm").camera.hover_distance - 8.5).abs() < 1e-3);
        let starscream = form("StarScreamVehicleForm").plane.as_ref().expect("a jet");
        assert_eq!((starscream.hover_fov, starscream.fly_fov), (75.0, 100.0));
        assert_eq!(starscream.hover_pitch_range, (-80.0, 80.0));
    }

    /// Bumblebee's well over a metre up as he folds (the car appeared below him).
    #[test]
    fn every_vehicle_has_its_transformation_lifts() {
        let assets = crate::assets_dir();
        if !(0..CLASSES.len()).all(|c| class_files_present(&assets, c)) {
            return;
        }
        let chars = Characters::load(&assets);
        assert!(chars.available);
        assert!(chars.vehicles.len() >= 38, "{}", chars.vehicles.len());
        for v in &chars.vehicles {
            let t = v.timing;
            assert!(t.fold_lift >= 0.0 && t.fold_lift < 3.0, "{}", v.form);
            assert!(t.unfold_lift >= 0.0 && t.unfold_lift < 3.0, "{}", v.form);
        }
        let bumblebee = chars
            .vehicles
            .iter()
            .find(|v| v.form == "BumbleBeeVehicleForm")
            .expect("Bumblebee's car");
        assert!(bumblebee.timing.fold_lift > 1.0, "{:?}", bumblebee.timing);
        assert!(bumblebee.timing.unfold_lift > 0.5, "{:?}", bumblebee.timing);
    }

    /// Each car's, truck's and tank's body follows the ground on its own springs (its
    /// vground line): Jazz's car's are Ground::JAZZ, Optimus Prime's truck's wheels
    /// and hover reach wider, a tank's hover reaches 2.5 m round it and holds nothing
    /// level, and it has no wheels; a jet has none of its own.
    #[test]
    fn every_ground_vehicle_has_its_springs() {
        let assets = crate::assets_dir();
        if !(0..CLASSES.len()).all(|c| class_files_present(&assets, c)) {
            return;
        }
        let chars = Characters::load(&assets);
        let get = |form: &str| {
            chars
                .vehicles
                .iter()
                .find(|v| v.form == form)
                .unwrap_or_else(|| panic!("{form}"))
        };
        assert_eq!(get("JazzVehicleForm").ground, Ground::JAZZ);
        let optimus = get("OptimusPrimeTruckForm").ground;
        assert!(
            optimus
                .wheel_points()
                .is_some_and(|w| w[0].x < -1.0 && w[3].y < -1.5)
        );
        assert!((optimus.hover_radius - 1.85).abs() < 1e-3 && optimus.hover_hold == 0.05);
        let megatron = get("MegatronVehicleForm").ground;
        assert!(megatron.wheel_points().is_none());
        assert!((megatron.hover_radius - 2.5).abs() < 1e-3 && megatron.hover_hold == 0.0);
        assert_eq!(get("JetfireVehicleForm").ground, Ground::JAZZ);
    }

    /// The Dinobots sound as their audio components play them: no engine, no jump
    /// or landing from their sound event set (its tank's), no boost's start or end
    /// but Grimlock's start; their jump and landing clips' own (Snarl's stamp as he
    /// lands, at the landing's first frame, and twice landing hard on the move); and
    /// their runs' footfalls at the clips' times as they play (1.2 to 1.4 times as
    /// fast), both within the clip. Jazz's car keeps its engine, jumps and landings.
    #[test]
    fn the_dinobots_play_only_their_own_sounds() {
        let assets = crate::assets_dir();
        if !(0..CLASSES.len()).all(|c| class_files_present(&assets, c)) {
            return;
        }
        let chars = Characters::load(&assets);
        let at = |form: &str| chars.vehicles.iter().position(|v| v.form == form);
        let (Some(snarl), Some(slug), Some(grimlock), Some(jazz)) = (
            at("SnarlVehicleForm"),
            at("SlugTruckForm"),
            at("GrimlockMultiplayerVehicleForm"),
            at("JazzVehicleForm"),
        ) else {
            panic!("the Dinobots and Jazz's car");
        };
        // (Each one's run as it plays: Nav_Boost_VEH at its RateScale, 1.35, 1.2 and
        // 1.4 times as fast.)
        for (dino, run_secs) in [(snarl, 0.5432), (slug, 0.6111), (grimlock, 0.7143)] {
            let v = Some(dino);
            assert!(!chars.car_has_engine(v), "{}", chars.vehicles[dino].form);
            for event in [
                SOUND_ENGINE,
                SOUND_HOVER_JUMP,
                SOUND_BOOST_JUMP,
                SOUND_LAND_HOVER_LIGHT,
                SOUND_LAND_HOVER_HEAVY,
                SOUND_LAND_WHEELS_LIGHT,
                SOUND_LAND_WHEELS_HEAVY,
                SOUND_BOOST_END,
            ] {
                assert_eq!(chars.car_sound(v, event, "x"), None, "{event}");
            }
            // (Its run's two footfalls, both within the clip as it plays.)
            let run = chars.car_clip_sounds(v, 0);
            assert_eq!(run.len(), 2, "{}", chars.vehicles[dino].form);
            assert!(run.iter().all(|&(t, _)| t < run_secs), "{run:?}");
            // (Its jump's, and its landings' stamps.)
            for clip in [16, 17, 18] {
                assert!(!chars.car_clip_sounds(v, clip).is_empty(), "{clip}");
            }
        }
        assert_eq!(chars.car_sound(Some(snarl), SOUND_BOOST_START, "x"), None);
        assert!(
            chars
                .car_sound(Some(grimlock), SOUND_BOOST_START, "x")
                .is_some()
        );
        let land = chars.car_clip_sounds(Some(snarl), 17);
        assert_eq!(land.len(), 2);
        assert_eq!(land[0], (0.0, "BL_VEH_TANK.SNARL_FS_LAND"));
        assert_eq!(chars.car_clip_sounds(Some(snarl), 18)[0].0, 0.0);
        let j = Some(jazz);
        assert!(chars.car_has_engine(j));
        for event in [
            SOUND_ENGINE,
            SOUND_HOVER_JUMP,
            SOUND_BOOST_JUMP,
            SOUND_LAND_HOVER_HEAVY,
        ] {
            assert!(chars.car_sound(j, event, "x").is_some(), "{event}");
        }
    }

    /// The Insecticons sound as their audio components play them: Sharpshot's and
    /// Kickback's hover starts with its one-shot, Sharpshot's alone stops with one;
    /// no jet's dive stops with a sound but Vortex's (each one's set has one for
    /// it); their engine, boost and climbs are their VEH_INSECTICON set's.
    #[test]
    fn the_insecticons_play_their_own_sounds() {
        let assets = crate::assets_dir();
        if !(0..CLASSES.len()).all(|c| class_files_present(&assets, c)) {
            return;
        }
        let chars = Characters::load(&assets);
        let at = |form: &str| chars.vehicles.iter().position(|v| v.form == form);
        let (Some(sharpshot), Some(kickback), Some(vortex), Some(jetfire)) = (
            at("SharpshotVehicleForm"),
            at("KickbackVehicleForm"),
            at("VortexVehicleForm"),
            at("JetfireVehicleForm"),
        ) else {
            panic!("the Insecticons, Vortex and Jetfire");
        };
        let cue = |v: usize, e: &str| chars.car_sound(Some(v), e, "");
        for bug in [sharpshot, kickback] {
            assert_eq!(
                cue(bug, SOUND_ENGINE),
                Some("BL_VEH_JET.VEH_INSECTICON_FLYING_LP")
            );
            assert_eq!(
                cue(bug, SOUND_BOOST_START),
                Some("BL_VEH_JET.VEH_INSECTICON_BOOST_START")
            );
            assert_eq!(
                cue(bug, SOUND_MOVE_OFF),
                Some("BL_VEH_JET.VEH_INSECTICON_FLIGHT_START_ONESHOT")
            );
            assert!(cue(bug, SOUND_ASCEND).is_some());
        }
        assert_eq!(
            cue(sharpshot, SOUND_STOP),
            Some("BL_VEH_JET.VEH_INSECTICON_FLIGHT_STOP_ONESHOT")
        );
        assert_eq!(cue(kickback, SOUND_STOP), None);
        for jet in [sharpshot, kickback, jetfire] {
            assert_eq!(cue(jet, SOUND_DESCEND_STOP), None);
        }
        assert!(cue(vortex, SOUND_DESCEND_STOP).is_some());
    }

    #[test]
    fn the_transformations_play_their_footsteps() {
        let assets = crate::assets_dir();
        if !(0..CLASSES.len()).all(|c| class_files_present(&assets, c)) {
            return;
        }
        let chars = Characters::load(&assets);
        let at = |form: &str| chars.vehicles.iter().position(|v| v.form == form);
        let steps = |form: &str, to_car: bool| -> Vec<(f32, String)> {
            chars.transform_steps(at(form), to_car).to_vec()
        };
        // Jazz lands twice as he comes out of his car, and jumps into none of it.
        let jazz = steps("JazzVehicleForm", false);
        assert_eq!(jazz.len(), 2);
        assert!((jazz[0].0 - 0.7763).abs() < 1e-3 && jazz[0].1 == "FS_DEFAULT_HARD_LAND");
        assert!((jazz[1].0 - 1.0806).abs() < 1e-3);
        assert!(steps("JazzVehicleForm", true).is_empty());
        // Air Raid's jump into his jet, Jetfire's landing out of his, Ironhide's two
        // running steps, Ultra Magnus's softer landing.
        let air_raid = steps("AirRaidVehicleForm", true);
        assert_eq!(air_raid.len(), 1);
        assert_eq!(air_raid[0].1, "FS_DEFAULT_JUMP");
        assert_eq!(
            steps("JetfireVehicleForm", false)[0].1,
            "FS_DEFAULT_HARD_LAND"
        );
        assert_eq!(steps("IronhideTruckForm", false).len(), 2);
        assert_eq!(steps("UltraMagnusTruckForm", false)[0].1, "FS_DEFAULT_LAND");
        // Each one's class gives every one of them a cue (only those are listed),
        // and each comes in the clips' time.
        let mut any = 0;
        for (i, v) in chars.vehicles.iter().enumerate() {
            for to_car in [true, false] {
                for (t, event) in chars.transform_steps(Some(i), to_car) {
                    assert!(
                        !chars.robot_sound(v.class, event, "").is_empty(),
                        "{} {event}",
                        v.form
                    );
                    assert!((0.0..3.0).contains(t), "{} {t}", v.form);
                    any += 1;
                }
            }
        }
        assert!(any >= 40, "{any}");
    }

    /// Every class's forearm sets have their clips for the gun going away and coming
    /// back (arm_equip.glb), in slots of the robot's graph after the class's clips.glb
    /// ones and within EXTRA_SLOTS, and their pieces' sizes meanwhile: putting the
    /// gun away, the forearm's wheel (CloudA_ElbowRobo16) stays gone until it has
    /// nearly unfolded and its other pieces grow back over a fifth of a second;
    /// taking it out, the pieces on the CloudB bones fold away, all gone by 0.65 s
    /// (Jazz's set's ScaleBone notifies); with the gun on the left, the left arm's.
    #[test]
    fn the_forearms_come_out_as_the_gun_goes_away() {
        let assets = crate::assets_dir();
        if !(0..CLASSES.len())
            .all(|c| class_files_present(&assets, c) && assets.join(arm_equip_model(c)).exists())
        {
            return;
        }
        let chars = Characters::load(&assets);
        for (c, class) in chars.classes.iter().enumerate() {
            assert!(class.available, "{}", CLASSES[c]);
            assert!(class.equip_clip_count > 0, "{}", CLASSES[c]);
            assert!(class.equip_slot(class.equip_clip_count) <= EXTRA_SLOTS);
            // (Its first slot follows its last clips.glb one.)
            let last = class.slot(class.clip_count - 1);
            assert_eq!(class.equip_slot(0), last + 1, "{}", CLASSES[c]);
            assert_eq!(class.clip_in_slot(class.equip_slot(0)), None);
            assert_eq!(class.equip_clip_in_slot(class.equip_slot(1)), Some(1));
        }
        for set in &chars.arm_sets {
            let (Some(away), Some(back)) = (set.away_clip, set.back_clip) else {
                panic!("{} has no gun-away clips", set.name);
            };
            assert_ne!(away, back);
            assert!(away >= TRANSFORM_SLOTS && back >= TRANSFORM_SLOTS);
        }
        let jazz = chars
            .arm_sets
            .iter()
            .position(|a| a.name == "Jazz_Arm_ROBO_ANIM")
            .expect("Jazz's forearm set");
        let scale = |bone: &str, left: bool, away: bool, t: f32| {
            chars.equip_scale(Some(jazz), bone, left, away, t)
        };
        let wheel = "R_CloudA_ElbowRobo16_XT";
        assert!(scale(wheel, false, true, 0.2) < 0.01);
        assert!(scale(wheel, false, true, 0.9) > 0.99);
        assert!(scale("R_CloudA_ElbowRobo01_XT", false, true, 0.1) < 0.6);
        assert!(scale("R_CloudA_ElbowRobo01_XT", false, true, 0.3) > 0.99);
        assert!(scale("R_CloudB_ElbowRobo01_XT", false, false, 0.3) > 0.99);
        assert!(scale("R_CloudB_ElbowRobo01_XT", false, false, 0.7) < 0.01);
        // (Bones they don't scale stay whole; the left arm's play the right's.)
        assert_eq!(scale("R_CloudB_ElbowRobo01_XT", false, true, 0.5), 1.0);
        assert!(scale("L_CloudA_ElbowRobo16_XT", true, true, 0.2) < 0.01);
        assert_eq!(scale("L_CloudA_ElbowRobo16_XT", false, true, 0.2), 1.0);
        assert!(chars.is_switch_bone(wheel));
    }

    /// The Scientist's robots wear their wings standing up on their backs (the
    /// game's RB_*_PART_Wing: RB_Airraid_STAT_Wing...), their jets the jets' own
    /// (VH_*_PART_Wing); every other sixth part is the robot's and the vehicle's.
    #[test]
    fn a_robot_wears_its_own_wings_and_its_jet_the_jets() {
        let assets = crate::assets_dir();
        if !(0..CLASSES.len()).all(|c| class_files_present(&assets, c)) {
            return;
        }
        let chars = Characters::load(&assets);
        let mut wings = 0;
        for (c, class) in chars.classes.iter().enumerate() {
            for p in class.parts.iter().filter(|p| p.wheel.is_some()) {
                if CLASSES[c] == "scientist" {
                    let (robot, jet) = (p.wheel.as_deref(), p.vehicle_wheel.as_deref());
                    assert!(robot.is_some_and(|m| m.starts_with("RB_")), "{robot:?}");
                    assert!(jet.is_some_and(|m| m.starts_with("VH_")), "{jet:?}");
                    assert_eq!(p.socket, "wingsocket");
                    wings += 1;
                } else {
                    assert_eq!(p.vehicle_wheel, p.wheel, "{}", p.id);
                }
            }
        }
        assert_eq!(wings, 6);
    }

    #[test]
    fn a_look_keeps_its_class_and_faction_over_the_network() {
        for class in 0..CLASSES.len() as u8 {
            for decepticon in [false, true] {
                let look = Look {
                    class,
                    decepticon,
                    primary: [1, 2, 3],
                    // (Its decal set too.)
                    decals: class + 1,
                    ..Look::DEFAULT
                };
                assert_eq!(Look::from_bytes(look.to_bytes()), look);
            }
        }
        let purple = Look {
            decepticon: true,
            ..Look::DEFAULT
        };
        assert_eq!(purple.energon(), DECEPTICON_ENERGON);
        assert_eq!(Look::DEFAULT.energon(), AUTOBOT_ENERGON);
        assert_ne!(purple.paint_key(), Look::DEFAULT.paint_key());
    }

    #[test]
    fn a_glow_override_repaints_the_energon_and_gives_the_paint_back() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<PaintMaterial>()
            .add_systems(Update, override_glows);
        let own = app
            .world_mut()
            .resource_mut::<Assets<PaintMaterial>>()
            .add(PaintMaterial {
                base: StandardMaterial::default(),
                extension: Paint::new(
                    PaintColors::new(
                        Vec4::ZERO,
                        Vec4::ZERO,
                        AUTOBOT_ENERGON.extend(1.0),
                        &PaintValues::DEFAULT,
                        -1.0,
                    ),
                    None,
                    None,
                ),
            });
        let model = app.world_mut().spawn(GlowOverride(DECEPTICON_ENERGON)).id();
        let part = app
            .world_mut()
            .spawn((MeshMaterial3d(own.clone()), ChildOf(model)))
            .id();
        let shown = |app: &App| {
            let world = app.world();
            let handle = &world.get::<MeshMaterial3d<PaintMaterial>>(part).unwrap().0;
            (
                handle.id(),
                world
                    .resource::<Assets<PaintMaterial>>()
                    .get(handle)
                    .unwrap()
                    .extension
                    .colors
                    .energon,
            )
        };
        // The other colour, its glow masks kept.
        app.update();
        let (copy, energon) = shown(&app);
        assert_ne!(copy, own.id());
        assert_eq!(energon, DECEPTICON_ENERGON.extend(1.0));
        // Steady: the same copy.
        app.update();
        assert_eq!(shown(&app).0, copy);
        // Its faction's colour again: its own paint back.
        app.world_mut()
            .entity_mut(model)
            .insert(GlowOverride(AUTOBOT_ENERGON));
        app.update();
        assert_eq!(shown(&app), (own.id(), AUTOBOT_ENERGON.extend(1.0)));
    }
}
