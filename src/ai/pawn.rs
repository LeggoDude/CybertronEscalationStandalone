//! An enemy's pawn (TnHtnAiPawn and the engine's Pawn): where it is and how it
//! moves (UE3's walking and falling physics, its rotation towards its desired
//! rotation), and the services it carries (its stance, its acrobatics state, its
//! animator's custom animations, its weapon, its aimer).
//!
//! Everything is in Unreal's space (centimetres, z up, rotation units): the pawn's
//! location is its collision cylinder's middle (Pawn.Location), as the AI's script
//! measures from it.

use std::sync::Arc;

use bevy::prelude::*;

use super::anim::{AnimParams, Animator, Callback};
use super::data::Behaviour;
use super::motion::Locomotion;
use super::ue;
use super::weapon::{Aimer, Gun};
use super::world::WorldView;

/// UE3's EPhysics, as far as the AI's pawns use it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Physics {
    #[default]
    None = 0,
    Walking = 1,
    Falling = 2,
    /// PHYS_Flying: its acrobatics service flies it (TnAcrobaticsService.Fly: the
    /// Leaper's lunges and its leap smash's dive), no gravity on it.
    Flying = 4,
    /// PHYS_Interpolating: a spawn animation plays (TnHtnAiPawn.PostBeginPlay and
    /// StartSpawnAnimation), the pawn held where the animation puts it.
    Interpolating = 7,
    /// PHYS_Spider: on a wall (or a ceiling), moving along it (a Sniper's insect on
    /// its perch: SetPhysics(PHYS_Spider, its wall, its normal), sniper.rs).
    Spider = 8,
    /// PHYS_RigidBody: its vehicle form's rigid body moves it (a drone's plane
    /// form, hovering: drones.rs), no gravity on it.
    RigidBody = 10,
}

/// FoC's world gravity (WorldInfo's DefaultGravityZ).
pub const GRAVITY_Z: f32 = -2940.0;
/// The steepest floor a pawn walks on (Pawn.WalkableFloorZ).
pub const WALKABLE_FLOOR_Z: f32 = 0.7;
/// A pawn steps up and down ledges this high (the demo's STEP_HEIGHT, as the
/// players do).
pub const MAX_STEP_UU: f32 = 110.0;
/// How high a walking pawn's cylinder stands over the floor: UE3's physWalking
/// keeps it between MINFLOORDIST (1.9) and MAXFLOORDIST (2.4), putting one outside
/// them back at their middle. (Volumes are built to it: Ignition's smash volume's
/// floor is half a unit above the arena's.)
pub const FLOOR_DIST: f32 = 2.15;
/// TnAiLaserSightBlueprint's SmoothTargetLocationSpeed and RollDegreesPerSecond
/// (its class's: the Sharpshooter's doesn't set them).
const LASER_SMOOTH_SPEED: f32 = 10.0;
const LASER_ROLL_PER_SEC: f32 = 200.0;

/// TnPawn.CalcVelocity's least speed asked for when it accelerates at all.
const MIN_DESIRED_SPEED: f32 = 385.0;
/// TnPawn.CalculateMomentumPreservation: the acceleration counts as along the
/// velocity within 30 degrees of it.
const MOMENTUM_FORWARD_COS: f32 = 0.866;
/// How far the head's aimer turns it from the body (rotation units: 80 degrees).
const HEAD_TURN: f32 = 80.0 * ue::ROT_PER_DEG;

/// ETnStance (TnStanceService).
pub mod stance {
    pub const GUN_ARM_SHOOT_FROM_HIP: u8 = 11;
    pub const NAVIGATION: u8 = 19;
    pub const LOW_COVER_HIDE: u8 = 22;
    pub const LOW_COVER_PEEK: u8 = 23;
    pub const LOW_CORNER_COVER_HIDE: u8 = 24;
    pub const LOW_CORNER_COVER_PEEK: u8 = 25;
    pub const CORNER_COVER_HIDE: u8 = 26;
    pub const CORNER_COVER_PEEK: u8 = 27;
    pub const LEDGE_COVER_HIDE: u8 = 28;
    pub const LEDGE_COVER_PEEK: u8 = 29;
    pub const OPEN_COVER_HIDE: u8 = 30;
    pub const OPEN_COVER_PEEK: u8 = 31;
    pub const PATROL: u8 = 32;
    pub const NONE: u8 = 34;

    /// TnStanceService.ETnStance's names, in their order.
    const NAMES: [&str; 43] = [
        "STANCE_OneHanded",
        "STANCE_TwoHandedMelee",
        "STANCE_TwoHandedMeleeCharge",
        "STANCE_TwoHandedGun",
        "STANCE_Downed",
        "STANCE_HunkerDown",
        "STANCE_Stealth",
        "STANCE_Flamethrower",
        "STANCE_Shield",
        "STANCE_Grab",
        "STANCE_GunArmIdle",
        "STANCE_GunArmShootFromHip",
        "STANCE_GunArmFineAim",
        "STANCE_StealthFineAim",
        "STANCE_WallPerch",
        "STANCE_CeilingPerch",
        "STANCE_Customization",
        "STANCE_Driving",
        "STANCE_Siege",
        "STANCE_Navigation",
        "STANCE_RustStorm",
        "STANCE_CrouchNavigation",
        "STANCE_LowCoverHide",
        "STANCE_LowCoverPeek",
        "STANCE_LowCornerCoverHide",
        "STANCE_LowCornerCoverPeek",
        "STANCE_CornerCoverHide",
        "STANCE_CornerCoverPeek",
        "STANCE_LedgeCoverHide",
        "STANCE_LedgeCoverPeek",
        "STANCE_OpenCoverHide",
        "STANCE_OpenCoverPeek",
        "STANCE_Patrol",
        "STANCE_DualGunArm",
        "STANCE_None",
        "STANCE_Hover",
        "STANCE_HeavyGunArmIdle",
        "STANCE_HeavyGunArmShootFromHip",
        "STANCE_HeavyGunArmFineAim",
        "STANCE_DownedNoGun",
        "STANCE_Test1",
        "STANCE_Test2",
        "STANCE_Test3",
    ];

    /// A stance's number by its name (STANCE_None for an unknown one).
    pub fn number(name: &str) -> u8 {
        NAMES
            .iter()
            .position(|n| *n == name)
            .unwrap_or(NONE as usize) as u8
    }
}

/// TnStanceManager: the stances pushed over the weapon's, and the hand the pawn
/// holds its gun in (0 right, 1 left: ETnHandedness).
#[derive(Clone, Debug, Default)]
pub struct StanceService {
    pub overrides: Vec<u8>,
    pub handedness: u8,
    pub desired_handedness: u8,
}

impl StanceService {
    /// get_Stance: the top override, else the weapon's (its data's BaseStance: an
    /// AI's weapon never goes idle in a match).
    pub fn stance(&self, weapon_stance: u8) -> u8 {
        self.overrides.first().copied().unwrap_or(weapon_stance)
    }

    pub fn push(&mut self, s: u8) {
        self.overrides.insert(0, s);
    }

    pub fn pop(&mut self, s: u8) {
        self.overrides.retain(|&x| x != s);
    }

    pub fn clear(&mut self) {
        self.overrides.clear();
    }

    pub fn set_handedness(&mut self, h: u8) {
        self.handedness = h;
        self.desired_handedness = h;
    }
}

/// TnAcrobaticsService.EAcrobaticsState, as the AI reads it (WS_Acrobatics: its
/// operators that need the pawn on its feet ask for AS_OnTheGround, 10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Acrobatics {
    Jumping = 0,
    Falling = 5,
    OnTheGround = 10,
    Flying = 13,
    Interpolating = 14,
    Landing = 15,
    #[default]
    None = 18,
}

/// Its acrobatics service's landing animation's end (Landing.OnFinishLanding:
/// on the ground).
pub const CB_ACRO_LANDED: Callback = Callback { owner: 0, kind: 4 };

/// A landing animation of its acrobatics blueprint's (TnLandingAnimationParams):
/// for a landing at least so fast along its facing and from at least so high, its
/// stance picker's animation (its stance's own, else its default; "None" none),
/// its blend out, and whether its root motion moves the pawn (DisallowMovement:
/// RMM_Accel, else ignored and the pawn free to move).
#[derive(Clone, Debug)]
pub struct LandingAnim {
    pub min_speed: f32,
    pub min_height: f32,
    pub blend_out: f32,
    pub disallow_movement: bool,
    pub default: Arc<str>,
    pub custom: Vec<(u8, Arc<str>)>,
}

impl LandingAnim {
    /// Its chassis's acrobatics blueprint's LandingAnims (those with animations).
    pub fn for_type(b: &Behaviour) -> Vec<LandingAnim> {
        let key = "Chassis.AcrobaticsManagerBlueprint.LandingAnims";
        (0..b.n(key))
            .filter_map(|i| {
                let k = format!("{key}[{i}]");
                b.obj(&format!("{k}.Animations"))?;
                let picker = format!("{k}.Animations");
                let custom = (0..b.n(&format!("{picker}.CustomAnimations")))
                    .map(|j| {
                        let c = format!("{picker}.CustomAnimations[{j}]");
                        (
                            stance::number(b.get(&format!("{c}.Stance")).unwrap_or("STANCE_None")),
                            Arc::from(b.get(&format!("{c}.Animation")).unwrap_or("None")),
                        )
                    })
                    .collect();
                Some(LandingAnim {
                    min_speed: b.f(&format!("{k}.MinSpeed"), 0.0),
                    min_height: b.f(&format!("{k}.MinHeight"), 0.0),
                    blend_out: b.f(&format!("{k}.BlendOutTime"), 0.1),
                    disallow_movement: b.b(&format!("{k}.DisallowMovement"), false),
                    default: Arc::from(
                        b.get(&format!("{picker}.DefaultAnimation"))
                            .unwrap_or("None"),
                    ),
                    custom,
                })
            })
            .collect()
    }

    /// TnStanceAnimationPicker.Pick (PickBasedOffStance).
    fn pick(&self, stance: u8) -> Arc<str> {
        self.custom
            .iter()
            .find(|(s, _)| *s == stance)
            .map_or_else(|| self.default.clone(), |(_, a)| a.clone())
    }
}

/// A TnTransformerMomentumBlueprint (its chassis's MomentumBlueprint: the
/// Leaper's): how much of its speed over its top speed it keeps, on the ground
/// and in the air, accelerating along its way, not at all, or another way
/// (MomentumParams' Forward, Neutral and Backward). None keeps nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Momentum {
    pub on_ground: [f32; 3],
    pub in_air: [f32; 3],
}

/// What happened to the pawn this frame that its controller hears of.
#[derive(Clone, Debug, Default)]
pub struct PawnNotes {
    /// It landed (Landed).
    pub landed: bool,
    /// It touched another pawn (Bump: the other's entity).
    pub bumped: Vec<Entity>,
    /// It ran into a wall (HitWall: its normal).
    pub hit_wall: Option<Vec3>,
}

/// An enemy's pawn.
#[derive(Clone, Debug)]
pub struct Pawn {
    pub entity: Entity,
    // -- Its body --------------------------------------------------------------
    pub loc: Vec3,
    pub vel: Vec3,
    /// Pawn.Acceleration: the way the controller drives it (its size is ignored:
    /// AccelRate is how hard).
    pub accel: Vec3,
    pub physics: Physics,
    /// Rotation.Yaw and DesiredRotation.Yaw (rotation units).
    pub yaw: f32,
    pub desired_yaw: f32,
    /// Rotation.Pitch and Rotation.Roll (rotation units): none but a Sniper's on a
    /// wall, turned to it (sniper.rs), and its model turned with them.
    pub pitch: f32,
    pub roll: f32,
    /// The wall it crawls on (PHYS_Spider's Floor: its normal).
    pub spider_floor: Vec3,
    /// The controller's RotationRate.Yaw (rotation units a second).
    pub rotation_rate: f32,
    pub radius: f32,
    pub half_height: f32,
    /// BaseGroundSpeed (the locomotion's profile's, or slowed near a destination)
    /// and AccelRate.
    pub ground_speed: f32,
    pub accel_rate: f32,
    /// BaseAirSpeed (its chassis's AirSpeed; a jump of a set velocity makes it
    /// that velocity's size: TnAcrobaticsManager.CalculateJumpVelocity), its
    /// TerminalVelocity (its chassis's: TnPawnNative.GetTerminalVelocity) and its
    /// chassis's momentum blueprint.
    pub air_speed: f32,
    pub terminal_velocity: f32,
    pub momentum: Option<Momentum>,
    /// Its acrobatics service is in its Jumping state (its form's Jump), till it
    /// comes down (FallingFromJump: AS_Falling) or lands.
    pub acro_jumping: bool,
    /// Its acrobatics service's landings: its blueprint's landing animations,
    /// whether they may play (_LandingAnimationEnabled: a jump blend's landing and
    /// a skid turn them off), the one that plays (AS_Landing till it ends), the
    /// height its fall began at (_FallBaseHeight), and whether it is in its
    /// Falling state, and has landed for its next update (its acrobatics service
    /// takes up a landing the frame after: after the jump blend has had its say).
    pub landing_anims: Vec<LandingAnim>,
    pub landing_animation_enabled: bool,
    pub acro_landing: Option<Arc<str>>,
    pub fall_base_height: f32,
    pub acro_falling: bool,
    pub landed_pending: bool,
    /// It goes through the pawns that would block it (bIgnoreBlockingActors: the
    /// Leaper's leap smash).
    pub ignore_blockers: bool,
    /// It may walk off ledges (SetWalkOffLedge; behaviour flag 64 holds it back).
    pub walk_off_ledge: bool,
    // -- Its controller's view -------------------------------------------------
    /// bRotateTowardFocus (behaviour flag 4).
    pub rotate_toward_focus: bool,
    /// The focal point (FocusLocation) and the point the aimer aims at and the
    /// head looks at (SetLookAtPoint).
    pub focus: Vec3,
    pub look_at: Vec3,
    /// The IK it has on (behaviour flags 1, 16 and the aimer's arm).
    pub ik_head: bool,
    pub ik_weapon: bool,
    pub ik_arm: bool,
    // -- Its services ----------------------------------------------------------
    pub motion: Locomotion,
    pub stance: StanceService,
    pub anim: Animator,
    pub weapon: Gun,
    pub aimer: Aimer,
    // -- Its state -------------------------------------------------------------
    pub health: f32,
    pub health_max: f32,
    /// Its navigation cell (Anchor), if it has one.
    pub anchor: Option<u32>,
    /// A knockback's velocity to come (TakeDamage), and the jump the animation
    /// plays (the jump blend's horizontal and vertical trajectory and its time).
    pub knockback_velocity: Vec3,
    pub jump: Option<JumpInfo>,
    /// Seconds in the air.
    pub air_time: f32,
    /// A Sniper flying between its points (sniper.rs's FlyTo): the level's blocking
    /// volumes don't stop it, only its solid surfaces. (The air network the game's
    /// Snipers fly crosses one volume on any map: Ignition's AirStrike station's
    /// barrier, DynamicBlockingVolume_16006, which keeps the players off its
    /// platform until it is bought and holds the hub of nine of its flights,
    /// AirPathNode_5790, which its Kismet leaves open while it shuts the
    /// platform's cells. Taken as a wall, it held the Snipers headed through the
    /// hub against it.)
    pub through_volumes: bool,
    pub notes: PawnNotes,
    /// It is spawning (its spawn animation plays: _IsSpawning).
    pub spawning: bool,
    /// It blocks other pawns (bBlockActors: its path following turns it off while
    /// it moves, SetCollision).
    pub block_actors: bool,
    /// Its sight (Pawn.SightRadius, PeripheralVision: the cosine of half its cone,
    /// VisionHalfHeight: how far above or below its view it sees, 0 for any).
    pub sight_radius: f32,
    pub peripheral_vision: f32,
    pub vision_half_height: f32,
    /// A fire's burning it (its buffs' TnBuffDotFromFire).
    pub on_fire: bool,
    /// It is dying (its health gone: the death plays).
    pub dying: bool,
    /// Its voice: its sound sets, its dialog character and its dialog (audio.rs).
    pub voice: super::audio::Voice,
    /// Its melee manager.
    pub melee: super::melee::Melee,
    /// The speed multiplier its melee puts on it (SetSpeedMultiplier 'Melee').
    pub speed_multiplier: f32,
    /// Its gun's muzzle (Unreal space), and its sockets (its skeleton's, its
    /// gun's, the bones it reads) where its model's last pose put them: in its own
    /// frame (from its feet, its facing along x), so that they move with it as
    /// UE3's do between poses.
    pub muzzle: Vec3,
    pub muzzle_local: Option<Vec3>,
    pub sockets: Vec<PawnSocket>,
    /// The physics it goes back to after its spawn animation.
    pub post_spawn_physics: Physics,
    /// Its jumps so far (the anim tree's jump blend plays each new one).
    pub jumps: u32,
    /// What is left of its blind fire (TnAnimNotify_AIFireWeapon's duration).
    pub blind_fire_left: f32,
    /// Its mesh's scale (its MeshSet's MeshScale: the root motion's).
    pub mesh_scale: f32,
    /// Its anim tree, and the pose it made this frame (for its model).
    pub tree: super::tree::AnimTree,
    pub pose: super::tree::Pose,
    /// The SwitchHandedness timer its blueprint may set as it comes in (seconds
    /// left).
    pub switch_handedness_in: Option<f32>,
    /// Its cloak, if its blueprint gives it one (the Sharpshooter's).
    pub cloak: Cloak,
    /// Its gun's laser sight (the Sharpshooter's).
    pub laser: LaserSight,
    /// _CanPlayHitReaction (a reaction's operator holds its hit reactions off).
    pub can_play_hit_reaction: bool,
    /// Its health status effects (_HealthStatusFX: its blueprint's, by its health
    /// level), those playing on its sockets, and the damage type of the last blow
    /// it took (get_LastTakeHitInfo's: their choosers go by it).
    pub health_status_fx: Vec<HealthStatusFx>,
    pub playing_status_fx: Vec<(Arc<str>, Arc<str>)>,
    pub last_damage: Option<super::brain::DamageKind>,
    /// What it makes of blows (TnPawn._DamageMultiplier, HeadShotBoneNames and
    /// BoneDamageOverrides, and its buffs' more from behind: any blow's, a melee
    /// blow's on top (TnBuffIncreaseDamageTakenFromBehind and
    /// TnBuffIncreaseMeleeDamageTakenFromBehind, the Leaper's); its Enemy takes
    /// them up: enemies.rs), and the bone it sees from (_ViewBoneName: none, its
    /// cylinder's top) and the axis of it it looks along (_ViewBoneAxis: 0 its -z,
    /// 1 its y), as its blueprint and its forms set them (the Guard's, the
    /// Leaper's).
    pub damage_multiplier: f32,
    pub head_bones: Vec<Arc<str>>,
    pub bone_damage: Vec<(Arc<str>, f32)>,
    pub from_behind: (f32, f32),
    pub view_bone: Option<Arc<str>>,
    pub view_bone_axis: u8,
    /// Its Mass (its chassis's: a blow's momentum over it is the knockback's
    /// velocity, TnHtnAiPawn.TakeDamage).
    pub mass: f32,
    /// What a blow's momentum moves it by (_TakeDamageMomentumScale: a Sniper's
    /// none).
    pub momentum_scale: f32,
    /// Its aim animator's AimOffsetScale (TnAimAnimator.get_AimOffset scales the
    /// aim its anim tree's aim offset node turns it by: the Guard's).
    pub aim_offset_scale: f32,
    /// Its aim animator (TnPawn._AimAnimator) and the aim it gave this frame
    /// (TnPawn.AimOffsetPct: UpdateAimOffset's, what its anim tree's aim offset
    /// node reads).
    pub aim_animator: AimAnimator,
    pub aim_offset_pct: Vec2,
    /// Its anim tree's aim node on (TnAnimNodeAimer._IsEnabled: the Guard's is off
    /// while it changes form).
    pub aim_node_enabled: bool,
    /// The Guard's forms and scanner (its TnGuardForms service).
    pub guard: Option<Box<super::guard::GuardForms>>,
    /// A drone's plane form and its motion controller (TnAiHoverPlaneMotionController:
    /// drones.rs), and a support drone's aura (TnBuffAuraSupportDroneDamage).
    pub plane: Option<Box<super::drones::PlaneForm>>,
    pub drone_aura: bool,
    /// The shield its support drones put on it showing (TnBuffSupportDroneDamage's
    /// looping ApplyFx playing).
    pub shield_fx: bool,
    /// A transforming pawn's two forms (the Snipers', the jets': forms.rs).
    pub forms: Option<Box<super::forms::Forms>>,
}

/// One of its model's sockets (or bones) where its last pose put it, in its own
/// frame (from its feet, its facing along x): its place and its axes (x, y, z:
/// Unreal's).
#[derive(Clone, Debug)]
pub struct PawnSocket {
    pub name: Arc<str>,
    pub at: Vec3,
    pub axes: [Vec3; 3],
}

/// TnHtnAiPawn.HealthStatusFXInfo: an effect on a socket once its health comes
/// down to its level (the first of its choosers for the last blow's damage type,
/// or for any).
#[derive(Clone, Debug)]
pub struct HealthStatusFx {
    pub level: i32,
    pub socket: Arc<str>,
    pub choosers: Vec<(Option<super::brain::DamageKind>, Arc<str>)>,
}

/// A cloak ability (TnAbilityCloaking: TnBuffCloak on it; its resource
/// unlimited for the AI, so it lasts until something gives it away).
#[derive(Clone, Debug, Default)]
pub struct Cloak {
    pub has: bool,
    /// _IsCloaking (StartCloaking, StopCloaking).
    pub active: bool,
    /// How far into the cloak it is (0 seen, 1 cloaked: its materials' sweep).
    pub amount: f32,
    /// TnBuffDecloak's seconds left: no cloaking again till they are up.
    pub lockout: f32,
}

/// CloakActivateDuration (its materials' sweep in and out) and TnBuffDecloak's
/// time (cloak.rs's, the players').
pub const CLOAK_SWEEP_SECS: f32 = 1.5;
pub const DECLOAK_SECS: f32 = 1.5;

impl Cloak {
    /// TriggerAbility on its cloak: TnBuffCloak on it (StartCloaking), unless it
    /// is cloaked or a decloak keeps it from it.
    pub fn trigger(&mut self) -> bool {
        if !self.has || self.active || self.lockout > 0.0 {
            return false;
        }
        self.active = true;
        true
    }

    /// TnPawn.ExposeSelf: the cloak taken off (ExposeCloak) and TnBuffDecloak on.
    pub fn expose(&mut self) -> bool {
        if !self.active {
            return false;
        }
        self.active = false;
        self.lockout = DECLOAK_SECS;
        true
    }

    fn tick(&mut self, dt: f32) {
        self.lockout = (self.lockout - dt).max(0.0);
        let step = dt / CLOAK_SWEEP_SECS;
        self.amount = if self.active {
            (self.amount + step).min(1.0)
        } else {
            (self.amount - step).max(0.0)
        };
    }
}

/// A laser sight (TnWeaponLaserSight's TnAiLaserSight: the Sharpshooter's three
/// beams closing on what it scans): where it points (_TargetLocation, and the
/// point it eases towards it), how far its beams are spread (_NormalizedSpread:
/// all its spread at 1, none at 0) and their roll (degrees).
#[derive(Clone, Debug, Default)]
pub struct LaserSight {
    pub active: bool,
    pub target: Vec3,
    pub smoothed: Vec3,
    pub spread: f32,
    pub roll: f32,
}

impl LaserSight {
    /// Start (StartLaserSight).
    pub fn start(&mut self, target: Vec3) {
        self.active = true;
        self.target = target;
        self.smoothed = target;
    }

    pub fn stop(&mut self) {
        self.active = false;
    }
}

/// A pawn's aim animator (TnAimAnimator: the aim its anim tree's aim offset node
/// turns it by, a quarter turn each way its whole aim): TnPawnAimAnimator's
/// (the default: its base aim rotation's turn from its rotation),
/// TnAiLookAtAimAnimator's (the Guard's, the Heavy Gunners' and the Titans': where
/// its aimer aims from where it stands, from its rotation) or
/// TnSniperAimAnimator's (the Snipers': where it aims from its perch's wall, eased
/// in; its last aim kept).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AimAnimator {
    #[default]
    Pawn,
    LookAt,
    Sniper(Vec2),
}

/// The jump blend's trajectory (TnJumpBlendInfo): the jump's horizontal and
/// vertical distances (uu) and its time.
#[derive(Clone, Copy, Debug, Default)]
pub struct JumpInfo {
    pub x: f32,
    pub y: f32,
    pub time: f32,
    pub elapsed: f32,
}

impl Pawn {
    pub fn new(
        entity: Entity,
        feet: Vec3,
        yaw: f32,
        radius: f32,
        half_height: f32,
        health: f32,
    ) -> Pawn {
        Pawn {
            entity,
            loc: feet + Vec3::Z * half_height,
            vel: Vec3::ZERO,
            accel: Vec3::ZERO,
            physics: Physics::Walking,
            yaw,
            desired_yaw: yaw,
            pitch: 0.0,
            roll: 0.0,
            spider_floor: Vec3::Z,
            rotation_rate: 180.0 * ue::ROT_PER_DEG,
            radius,
            half_height,
            ground_speed: 600.0,
            accel_rate: 1800.0,
            air_speed: 1200.0,
            terminal_velocity: 10000.0,
            momentum: None,
            acro_jumping: false,
            landing_anims: Vec::new(),
            landing_animation_enabled: true,
            acro_landing: None,
            fall_base_height: feet.z + half_height,
            acro_falling: false,
            landed_pending: false,
            ignore_blockers: false,
            walk_off_ledge: true,
            rotate_toward_focus: false,
            focus: Vec3::ZERO,
            look_at: Vec3::ZERO,
            ik_head: false,
            ik_weapon: false,
            ik_arm: false,
            motion: Locomotion::default(),
            stance: StanceService::default(),
            anim: Animator::default(),
            weapon: Gun::default(),
            aimer: Aimer::default(),
            health,
            health_max: health,
            anchor: None,
            knockback_velocity: Vec3::ZERO,
            jump: None,
            air_time: 0.0,
            through_volumes: false,
            notes: PawnNotes::default(),
            spawning: false,
            block_actors: true,
            sight_radius: 50000.0,
            peripheral_vision: 0.0,
            vision_half_height: 0.0,
            on_fire: false,
            dying: false,
            voice: Default::default(),
            melee: super::melee::Melee::default(),
            speed_multiplier: 1.0,
            muzzle: feet + Vec3::Z * half_height,
            muzzle_local: None,
            sockets: Vec::new(),
            post_spawn_physics: Physics::Walking,
            jumps: 0,
            blind_fire_left: 0.0,
            mesh_scale: 1.0,
            tree: Default::default(),
            pose: Default::default(),
            switch_handedness_in: None,
            cloak: Cloak::default(),
            laser: LaserSight {
                spread: 1.0,
                ..Default::default()
            },
            can_play_hit_reaction: true,
            health_status_fx: Vec::new(),
            playing_status_fx: Vec::new(),
            last_damage: None,
            damage_multiplier: 1.0,
            head_bones: crate::enemies::HEAD_BONES
                .iter()
                .map(|b| Arc::from(*b))
                .collect(),
            bone_damage: crate::enemies::HEAD_BONES
                .iter()
                .map(|b| (Arc::from(*b), crate::enemies::HEAD_DAMAGE))
                .collect(),
            from_behind: (1.0, 1.0),
            view_bone: None,
            view_bone_axis: 0,
            mass: 100.0,
            momentum_scale: 1.0,
            aim_offset_scale: 1.0,
            aim_animator: AimAnimator::Pawn,
            aim_offset_pct: Vec2::ZERO,
            aim_node_enabled: true,
            guard: None,
            plane: None,
            drone_aura: false,
            shield_fx: false,
            forms: None,
        }
    }

    /// PlayHealthStatusFX: the effect of the health level come to played on its
    /// socket (ChooseHealthFXTemplate's), those of the levels under it stopped.
    pub fn play_health_status_fx(&mut self, level: i32) {
        for info in &self.health_status_fx {
            if info.level == level {
                let chosen = info
                    .choosers
                    .iter()
                    .find(|(kind, _)| kind.is_none() || *kind == self.last_damage)
                    .map(|(_, t)| t.clone());
                if let Some(t) = chosen {
                    self.playing_status_fx.push((info.socket.clone(), t));
                }
            } else if info.level < level {
                for (_, t) in &info.choosers {
                    self.playing_status_fx
                        .retain(|(s, p)| !(*s == info.socket && p == t));
                }
            }
        }
    }

    /// A socket's place (Mesh.GetSocketWorldLocation), as its model's last pose
    /// put it, else its hand's height ahead of it.
    pub fn socket_location(&self, name: &str) -> Vec3 {
        self.socket_frame(name)
            .map(|(at, _)| at)
            .unwrap_or(self.loc + self.forward() * self.radius + Vec3::Z * self.half_height * 0.4)
    }

    /// GetSocketWorldLocationAndRotation: a socket's place and axes (x, y, z), if
    /// its model has it.
    pub fn socket_frame(&self, name: &str) -> Option<(Vec3, [Vec3; 3])> {
        self.sockets.iter().find(|s| &*s.name == name).map(|s| {
            (
                self.feet() + ue::rotate_yaw(s.at, self.yaw),
                s.axes.map(|a| ue::rotate_yaw(a, self.yaw)),
            )
        })
    }

    /// TnHtnAiPawn.StartSpawnAnimation: held where the animation puts it, its
    /// controller asleep, until it ends (none: it simply stands).
    pub fn start_spawn_animation(&mut self, anim: &str) {
        if anim.is_empty() || anim == "None" {
            return;
        }
        self.spawning = true;
        self.post_spawn_physics = self.physics;
        self.physics = Physics::Interpolating;
        let p = super::anim::AnimParams::make(anim, 0.0, 0.3, super::anim::RMM_TRANSLATE);
        self.anim.enqueue(
            p,
            Some(super::anim::CB_SPAWN_DONE),
            Some(super::anim::CB_SPAWN_DONE),
        );
    }

    /// SpawnAnimationComplete.
    pub fn spawn_complete(&mut self) {
        self.spawning = false;
        if self.dying {
            return;
        }
        self.physics = self.post_spawn_physics;
    }

    /// TnHtnAiController.GetLookDirection: where its head (its view bone) looks:
    /// along the bone's axis (its -z, or its y: _ViewBoneAxis) where its model's
    /// last pose put it; without the bone, at what it looks at while its head
    /// tracks it (as far as the head turns), else where it faces.
    pub fn look_direction(&self) -> Vec3 {
        if let Some((_, axes)) = self.view_bone.as_deref().and_then(|b| self.socket_frame(b)) {
            let look = if self.view_bone_axis == 1 {
                axes[1]
            } else {
                -axes[2]
            };
            if look != Vec3::ZERO {
                return ue::normal(look);
            }
        }
        if self.ik_head && self.look_at != Vec3::ZERO {
            let to = self.look_at - self.eyes();
            let yaw = ue::yaw_of(to);
            let turn = ue::wrap_rot(yaw - self.yaw).clamp(-HEAD_TURN, HEAD_TURN);
            let pitch = ue::pitch_of(to).clamp(-HEAD_TURN, HEAD_TURN);
            return ue::vec_of(pitch, self.yaw + turn);
        }
        if self.aimer.active {
            return super::weapon::gun_direction(self);
        }
        self.forward()
    }

    /// Its acrobatics service's state (TnAcrobaticsManager's): on the ground while
    /// it walks, jumping in the air till it comes down, then falling, flying while
    /// it flies, interpolating while its spawn animation holds it.
    pub fn acrobatics(&self) -> Acrobatics {
        match self.physics {
            Physics::Walking if self.acro_landing.is_some() => Acrobatics::Landing,
            // (Its service still in the air till its next update.)
            Physics::Walking if self.landed_pending => Acrobatics::Falling,
            // (A Sniper's insect crawling on its wall: on its ground.)
            Physics::Walking | Physics::Spider => Acrobatics::OnTheGround,
            Physics::Falling if self.acro_jumping => Acrobatics::Jumping,
            Physics::Falling => Acrobatics::Falling,
            Physics::Flying => Acrobatics::Flying,
            Physics::Interpolating => Acrobatics::Interpolating,
            // (A vehicle form's rigid body: its robot form's acrobatics idle.)
            Physics::None | Physics::RigidBody => Acrobatics::None,
        }
    }

    /// Its form's Jump with a velocity of its own, forced (TnRobotForm.Jump, then
    /// TnAcrobaticsManager.Jump: on the ground its Jumping state, off it with that
    /// velocity, its BaseAirSpeed that velocity's size; in the air a double jump,
    /// which needs a buff no enemy has, so nothing). Whether it jumped.
    pub fn form_jump(&mut self, velocity: Vec3) -> bool {
        if self.physics != Physics::Walking {
            return false;
        }
        self.physics = Physics::Falling;
        self.acro_jumping = true;
        self.vel = velocity;
        if velocity != Vec3::ZERO {
            self.air_speed = velocity.length();
        }
        true
    }

    /// TnAcrobaticsService.Fall: its Falling state (PHYS_Falling).
    pub fn acrobatics_fall(&mut self) {
        self.physics = Physics::Falling;
        self.acro_jumping = false;
    }

    /// TnAcrobaticsService.Fly: its Flying state (PHYS_Flying).
    pub fn acrobatics_fly(&mut self) {
        self.physics = Physics::Flying;
        self.acro_jumping = false;
    }

    /// Pawn.CanAnchor (TnHtnAiPawn's): not while it falls or its controller sleeps.
    pub fn can_anchor(&self, active: bool) -> bool {
        active && self.physics != Physics::Falling
    }

    /// Its plane form while it is in it (a drone's always; a jet's while it is a
    /// plane: its TnAiTransformerMotionController's active motion controller the
    /// hover plane's).
    pub fn active_plane(&self) -> Option<&super::drones::PlaneForm> {
        if self
            .forms
            .as_ref()
            .is_some_and(|f| f.current != super::forms::ALT)
        {
            return None;
        }
        self.plane.as_deref()
    }

    pub fn active_plane_mut(&mut self) -> Option<&mut super::drones::PlaneForm> {
        if self
            .forms
            .as_ref()
            .is_some_and(|f| f.current != super::forms::ALT)
        {
            return None;
        }
        self.plane.as_deref_mut()
    }

    /// GetLocationAtFeet.
    pub fn feet(&self) -> Vec3 {
        self.loc - Vec3::Z * self.half_height
    }

    /// GetViewLocation (its eyes: its view bone, if it has one (TnHtnAiPawn's
    /// _ViewBoneName), else the top of its cylinder less a little, as BaseEyeHeight
    /// puts them).
    pub fn eyes(&self) -> Vec3 {
        self.view_bone
            .as_deref()
            .and_then(|b| self.socket_frame(b))
            .map(|(at, _)| at)
            .unwrap_or(self.loc + Vec3::Z * (self.half_height * 0.8))
    }

    /// Its facing (Rotation as a vector).
    pub fn forward(&self) -> Vec3 {
        ue::yaw_vec(self.yaw)
    }

    /// Its stance (TnStanceManager.get_Stance: the top one pushed, else its
    /// weapon's (its data's BaseStance), else STANCE_Navigation with none in hand).
    pub fn current_stance(&self) -> u8 {
        let weapon = if self.weapon.equipped {
            self.weapon.data.base_stance
        } else {
            stance::NAVIGATION
        };
        self.stance.stance(weapon)
    }

    pub fn is_jumping(&self) -> bool {
        self.physics == Physics::Falling && self.jump.is_some()
    }

    /// The controller's SetFocalPoint: rotating towards it if it rotates to focus.
    pub fn set_focal_point(&mut self, p: Vec3) {
        self.focus = p;
        if self.rotate_toward_focus {
            self.desired_yaw = ue::yaw_of(p - self.loc);
        }
    }

    pub fn set_focal_rotation(&mut self, yaw: f32) {
        let p = self.loc + ue::yaw_vec(yaw) * 10000.0;
        self.set_focal_point(p);
    }

    /// SetLookAtPoint: the aimer aims at it (and the head looks at it).
    pub fn set_look_at_point(&mut self, p: Vec3) {
        self.look_at = p;
        self.aimer.aim_at_location(p);
    }

    pub fn set_look_at_rotation(&mut self, pitch: f32, yaw: f32) {
        let p = self.loc + ue::vec_of(pitch, yaw) * 10000.0;
        self.set_look_at_point(p);
    }

    /// The motion controller's Stop.
    pub fn motion_stop(&mut self) {
        super::motion::stop(self);
    }

    /// TnHtnAiPawn.EstimateAirTime.
    pub fn estimate_air_time(v: Vec3) -> f32 {
        2.0 * v.z / -GRAVITY_Z
    }

    /// TnHtnAiPawn.DoKnockBack: falls with the knockback's velocity if it would
    /// be in the air long enough; the air time.
    pub fn do_knockback(&mut self) -> f32 {
        let air = Pawn::estimate_air_time(self.knockback_velocity);
        if air > 0.5 {
            self.acrobatics_fall();
            self.vel = self.knockback_velocity;
            self.accel = ue::normal(self.knockback_velocity);
        }
        self.knockback_velocity = Vec3::ZERO;
        air
    }
}

// ---------------------------------------------------------------------------
// Physics
// ---------------------------------------------------------------------------

/// TnPawn.CalcVelocity, the event UE3's walking, falling and flying physics ask
/// before their own (which every TnPawn takes over): the velocity the
/// acceleration asks for (its way, at its size (up to 1) of `max_speed`, but at
/// least MIN_DESIRED_SPEED; without one, standing still on the ground and keeping
/// its way in the air), come to by at most AccelRate along each of the pawn's own
/// axes (less by the momentum it keeps over its top speed); in the ground plane
/// walking or falling (a fall's gravity added to it, down to its terminal
/// velocity), in any way flying.
fn calc_velocity(p: &mut Pawn, dt: f32, max_speed: f32) {
    if dt <= 0.0 {
        return;
    }
    let falling = p.physics == Physics::Falling;
    let mask = match p.physics {
        Physics::Walking | Physics::Falling => Vec3::new(1.0, 1.0, 0.0),
        _ => Vec3::ONE,
    };
    let previous = p.vel * mask;
    let wanted = (p.accel * mask).clamp_length_max(1.0);
    // CalculateDesiredFlatVelocity.
    let desired = if wanted == Vec3::ZERO {
        if falling { previous } else { Vec3::ZERO }
    } else {
        wanted.normalize() * (wanted.length() * max_speed).max(MIN_DESIRED_SPEED)
    };
    let max_accel = p.accel_rate / (1.0 + momentum_preservation(p, wanted, previous, max_speed));
    let local = ue::rotate_yaw((desired - previous) / dt, -p.yaw)
        .clamp(Vec3::splat(-max_accel), Vec3::splat(max_accel));
    let old_z = p.vel.z;
    p.vel = previous + ue::rotate_yaw(local, p.yaw) * dt;
    if falling {
        p.vel.z = (old_z + GRAVITY_Z * dt).max(-p.terminal_velocity);
    }
}

/// TnPawn.CalculateMomentumPreservation: how much of its speed over its top speed
/// it keeps (its momentum blueprint's, in the air while it falls, by the way it
/// accelerates against the way it goes; none at or under its top speed).
fn momentum_preservation(p: &Pawn, wanted: Vec3, velocity: Vec3, max_speed: f32) -> f32 {
    let Some(m) = p.momentum else {
        return 0.0;
    };
    if velocity.length() < max_speed + 0.1 {
        return 0.0;
    }
    let along = if wanted == Vec3::ZERO {
        0.0
    } else {
        ue::normal(wanted).dot(ue::normal(velocity))
    };
    let params = if p.physics == Physics::Falling {
        m.in_air
    } else {
        m.on_ground
    };
    if along > MOMENTUM_FORWARD_COS {
        params[0]
    } else if wanted == Vec3::ZERO {
        params[1]
    } else {
        params[2]
    }
}

/// Turns `yaw` towards `desired` by at most `rate * dt` (physicsRotation's fixed
/// turn).
pub fn turn_toward(yaw: f32, desired: f32, rate: f32, dt: f32) -> f32 {
    let delta = ue::wrap_rot(desired - yaw);
    let step = rate * dt;
    if delta.abs() <= step {
        desired
    } else {
        ue::wrap_rot(yaw + step * delta.signum())
    }
}

/// One frame of the pawn's physics against the level (walking on its floors,
/// falling, its rotation), with the other pawns kept out of it: the world's view
/// gives the level and where the others are; the clips, its full body animation's
/// root motion.
pub fn step(p: &mut Pawn, view: &WorldView, clips: &super::clips::Clips, dt: f32) {
    p.notes = PawnNotes::default();
    if dt <= 0.0 {
        return;
    }
    p.cloak.tick(dt);
    // TnAiLaserSight.Update: its point eased towards its target
    // (SmoothTargetLocationSpeed), its beams rolling (RollDegreesPerSecond).
    if p.laser.active {
        p.laser.smoothed = ue::vinterp_to(p.laser.smoothed, p.laser.target, dt, LASER_SMOOTH_SPEED);
        p.laser.roll += LASER_ROLL_PER_SEC * dt;
    }
    // TnHtnAiPawn.SwitchHandedness: its other hand (not while it spawns).
    if let Some(left) = p.switch_handedness_in.as_mut() {
        *left -= dt;
        if *left <= 0.0 {
            p.switch_handedness_in = None;
            if !p.spawning {
                let h = if p.stance.handedness == 0 { 1 } else { 0 };
                p.stance.set_handedness(h);
            }
        }
    }
    acrobatics_update(p);
    // (A vehicle form come down from a fall goes back to its rigid body: a drone
    // flung out of its Titan, hovering once it lands.)
    if p.active_plane().is_some() && p.physics == Physics::Walking {
        p.physics = Physics::RigidBody;
    }
    // Rotation (physicsRotation; a spider keeps the turn its wall gave it).
    if !matches!(p.physics, Physics::Interpolating | Physics::Spider) {
        p.yaw = turn_toward(p.yaw, p.desired_yaw, p.rotation_rate.max(0.0), dt);
    }
    match p.physics {
        Physics::Walking => {
            let root = root_motion_velocity(p, clips, dt);
            walk(p, view, dt, root);
        }
        Physics::Falling => fall(p, view, dt),
        Physics::Flying => fly(p, view, dt),
        Physics::Spider => spider(p, dt),
        Physics::RigidBody => super::drones::plane_update(p, view, dt),
        _ => {}
    }
    // TnAcrobaticsManager.Jumping.OnUpdate: on the ground, or coming down
    // (IsDescending: FallingFromJump, AS_Falling).
    if p.acro_jumping && (p.physics != Physics::Falling || p.vel.z < 0.0) {
        p.acro_jumping = false;
    }
    if p.notes.landed {
        p.landed_pending = true;
    }
    if !p.ignore_blockers && p.active_plane().is_none() {
        separate(p, view);
    }
    p.anchor = view
        .nav
        .and_then(|n| n.cell_at(ue::from_ue(p.feet())).or(p.anchor));
}

/// TnAcrobaticsManager.Update (its pawn's tick, before the physics): into its
/// Falling state (off a ledge, over the top of a jump, let fall), the height it
/// falls from (out of any landing); back on the ground (its OnTheGround state),
/// its landing animation.
fn acrobatics_update(p: &mut Pawn) {
    let falling = p.physics == Physics::Falling && !p.acro_jumping;
    if falling && !p.acro_falling {
        p.fall_base_height = p.loc.z;
        p.acro_landing = None;
    }
    p.acro_falling = falling;
    if p.landed_pending {
        p.landed_pending = false;
        if p.physics == Physics::Walking {
            play_landing_animation(p);
            p.fall_base_height = p.loc.z;
        }
    }
}

/// OnTheGroundBase.PlayLandingAnimation: the first of its landing animations
/// for its speed along its facing and the height it fell (CanPlayLandingAnimation:
/// not while turned off or while it melees), high priority, till it ends
/// (AS_Landing).
fn play_landing_animation(p: &mut Pawn) {
    if !p.landing_animation_enabled || p.melee.is_attacking() {
        return;
    }
    let speed = p.vel.dot(p.forward()).abs();
    let fall = p.fall_base_height - p.loc.z;
    let Some(l) = p
        .landing_anims
        .iter()
        .find(|l| speed >= l.min_speed && fall >= l.min_height)
    else {
        return;
    };
    let name = l.pick(p.current_stance());
    if &*name == "None" {
        return;
    }
    let params = AnimParams {
        name: name.clone(),
        blend_out: l.blend_out,
        priority: 1,
        override_mode: super::anim::OVERRIDE_IF_HIGHER_OR_EQUAL,
        root_motion: if l.disallow_movement {
            super::anim::RMM_ACCEL
        } else {
            super::anim::RMM_IGNORE
        },
        ..Default::default()
    };
    p.anim
        .enqueue(params, Some(CB_ACRO_LANDED), Some(CB_ACRO_LANDED));
    p.acro_landing = Some(name);
}

/// The velocity the full body's animation's root motion gives the pawn this
/// frame (its slot's RootMotionMode other than RMM_Ignore: the custom animation
/// slots extract the root's translation and discard its turn), its slot's weight
/// of it: none while no such animation plays.
fn root_motion_velocity(p: &Pawn, clips: &super::clips::Clips, dt: f32) -> Option<Vec3> {
    let (_, _, anim, t, weight) = p.anim.root_motion()?;
    let clip = clips.find(&anim).and_then(|c| clips.get(c));
    let rate = clip.map_or(1.0, |c| {
        c.rate * p.anim.players[0].current().map_or(1.0, |a| a.play_rate)
    });
    let delta = match clip.and_then(|c| c.root.as_ref()) {
        Some(r) => {
            let a = r.at(t);
            let b = r.at(t + dt * rate);
            Vec2::new(b[0] - a[0], b[1] - a[1])
        }
        None => Vec2::ZERO,
    };
    let f = ue::yaw_vec(p.yaw);
    let r = ue::yaw_vec(p.yaw + 16384.0);
    Some((f * delta.x + r * delta.y) * (weight * p.mesh_scale / dt))
}

fn walk(p: &mut Pawn, view: &WorldView, dt: f32, root: Option<Vec3>) {
    p.air_time = 0.0;
    match root {
        // (RMM_Accel: the animation's motion is the pawn's.)
        Some(v) => p.vel = Vec3::new(v.x, v.y, 0.0),
        // (Its GroundSpeed: its base ground speed times its speed multipliers.)
        None => calc_velocity(p, dt, p.ground_speed * p.speed_multiplier),
    }
    let start = p.feet();
    let mut to = start + Vec3::new(p.vel.x, p.vel.y, 0.0) * dt;
    if let Some(push) = view.push_out(to, p.radius, p.half_height * 2.0) {
        to += push;
        if push.length_squared() > 1.0 {
            p.notes.hit_wall = Some(push.normalize_or_zero());
        }
    }
    match view.ground_at(to, p.radius, MAX_STEP_UU, MAX_STEP_UU) {
        Some(z) => {
            to.z = z + FLOOR_DIST;
            p.loc = to + Vec3::Z * p.half_height;
        }
        None => {
            if p.walk_off_ledge {
                p.loc = to + Vec3::Z * p.half_height;
                p.physics = Physics::Falling;
            } else {
                // (Held at the ledge: bCanWalkOffLedges off.)
                p.vel = Vec3::ZERO;
            }
        }
    }
    // The floor's height under it when it walked straight off nothing (a level
    // without collision: the start's height).
    if view.level.is_none() {
        p.loc.z = start.z + p.half_height;
    }
}

/// How much of a pawn's radius it lands on in a jump to a landing (the share of its
/// disc a floor must be under). The demo's: landing anywhere under its disc, a
/// Scrapper jumping down Oblivion's 10 m drop by path node 1194 caught the deck's
/// lip as it came back down to it, was held there (bCanWalkOffLedges off) and
/// stayed on the deck. Its middle has to come down over a floor now.
const JUMP_LANDING_RADIUS: f32 = 0.25;

fn fall(p: &mut Pawn, view: &WorldView, dt: f32) {
    p.air_time += dt;
    let land_radius = if p.jump.is_some() {
        p.radius * JUMP_LANDING_RADIUS
    } else {
        p.radius
    };
    // (Its AirSpeed: its base air speed times its speed multipliers.)
    calc_velocity(p, dt, p.air_speed * p.speed_multiplier);
    let mut v = p.vel;
    // (APawn::physFalling holds its speed to its terminal velocity too.)
    if v.length() > p.terminal_velocity {
        v = v.normalize() * p.terminal_velocity;
    }
    let start = p.feet();
    let n = sub_steps(v * dt, p.radius);
    let h = dt / n as f32;
    let mut at = start;
    for _ in 0..n {
        let mut to = at + v * h;
        if let Some(push) = view.push_out(to, p.radius, p.half_height * 2.0) {
            to += push;
            let w = push.normalize_or_zero();
            // The wall takes the velocity into it.
            let into = v.x * w.x + v.y * w.y;
            if into < 0.0 {
                v.x -= into * w.x;
                v.y -= into * w.y;
            }
            p.notes.hit_wall = Some(w);
        }
        // Risen into a ceiling: its head stops under it, the climb gone from its
        // velocity (physFalling's slide along what it struck, HitWall its normal).
        if let Some(top) = ceiling_over(p, view, at, to.z) {
            to.z = top;
            v.z = v.z.min(0.0);
            p.notes.hit_wall = Some(Vec3::NEG_Z);
        }
        if v.z <= 0.0 {
            let drop = (at.z - to.z).max(0.0);
            if let Some(z) = view
                .ground_at(to, land_radius, 10.0 + drop, 0.0)
                .or_else(|| {
                    view.ground_at(Vec3::new(to.x, to.y, at.z), land_radius, 10.0, drop + 1.0)
                })
            {
                to.z = z + FLOOR_DIST;
                p.loc = to + Vec3::Z * p.half_height;
                v.z = 0.0;
                p.vel = v;
                p.physics = Physics::Walking;
                p.jump = None;
                p.notes.landed = true;
                return;
            }
        }
        at = to;
    }
    p.vel = v;
    p.loc = at + Vec3::Z * p.half_height;
}

/// APawn::physFlying: TnPawn.CalcVelocity at its AirSpeed (no gravity), the move
/// made against the level: a wall pushes it back out (HitWall, its normal) and it
/// slides along it, a floor it comes down onto stops it there (HitWall too,
/// processHitWall's, with the floor's normal), one no higher than a step it
/// climbs (stepUp), and its velocity is the move it made (along what it slid on).
fn fly(p: &mut Pawn, view: &WorldView, dt: f32) {
    calc_velocity(p, dt, p.air_speed * p.speed_multiplier);
    let start = p.feet();
    let mut v = p.vel;
    let n = sub_steps(v * dt, p.radius);
    let h = dt / n as f32;
    let mut at = start;
    let mut slid = false;
    // (A Sniper flying between its points meets the level's solid surfaces alone:
    // through_volumes.)
    let who = if p.through_volumes {
        crate::collision::Mover::AiFlying
    } else {
        crate::collision::Mover::Ai
    };
    for _ in 0..n {
        let mut to = at + v * h;
        if let Some(push) = view.push_out_by(to, p.radius, p.half_height * 2.0, who) {
            to += push;
            if push.length_squared() > 1.0 {
                let w = push.normalize_or_zero();
                p.notes.hit_wall = Some(w);
                let into = v.x * w.x + v.y * w.y;
                if into < 0.0 {
                    v.x -= into * w.x;
                    v.y -= into * w.y;
                }
                slid = true;
            }
        }
        let rise = to.z - at.z;
        let ceiling = (rise > 0.0)
            .then(|| {
                let height = 2.0 * p.half_height;
                view.ceiling_at_by(at + Vec3::Z * height, p.radius, rise, who)
                    .map(|c| (c - height).max(at.z))
            })
            .flatten();
        if let Some(top) = ceiling {
            to.z = top;
            v.z = v.z.min(0.0);
            p.notes.hit_wall = Some(Vec3::NEG_Z);
            slid = true;
        }
        let up = MAX_STEP_UU.max(at.z - to.z + 10.0);
        let mut struck = false;
        if let Some(z) = view.ground_at_by(to, p.radius, up, 0.0, who)
            && z > to.z
        {
            if v.z < 0.0 && z <= at.z + 1.0 {
                p.notes.hit_wall = Some(Vec3::Z);
                struck = true;
            }
            to.z = z;
        }
        at = to;
        if struck {
            break;
        }
    }
    p.vel = if slid { v } else { (at - start) / dt };
    p.loc = at + Vec3::Z * p.half_height;
}

/// physSpider: its velocity brought towards its ground speed the way it is driven
/// (its acceleration), along its wall, and its move along it at that (a Sniper's
/// perch is clear: nothing in its way on its patch of wall).
fn spider(p: &mut Pawn, dt: f32) {
    calc_velocity(p, dt, p.ground_speed * p.speed_multiplier);
    let n = p.spider_floor;
    p.vel -= n * p.vel.dot(n);
    p.loc += p.vel * dt;
}

/// A rigid body's move a step at its velocity (no gravity), against the level: a
/// wall pushes it back out and takes the velocity into it (HitWall, its normal), a
/// ceiling stops its rise and a floor its fall, each taking the velocity into
/// it (RigidBodyCollision: the drones glance off what they meet). A drone's body
/// is its physics asset's box round its middle, not its pawn's cylinder.
pub fn move_rigid_body(p: &mut Pawn, view: &WorldView, dt: f32) {
    let (radius, half) = match p.active_plane() {
        Some(f) => f.body,
        None => (p.radius, p.half_height),
    };
    let start = p.loc - Vec3::Z * half;
    let mut v = p.vel;
    let n = sub_steps(v * dt, radius);
    let h = dt / n as f32;
    let mut at = start;
    for _ in 0..n {
        let mut to = at + v * h;
        if let Some(push) = view.push_out(to, radius, half * 2.0) {
            to += push;
            if push.length_squared() > 1.0 {
                let w = push.normalize_or_zero();
                p.notes.hit_wall = Some(w);
                let into = v.x * w.x + v.y * w.y;
                if into < 0.0 {
                    v.x -= into * w.x;
                    v.y -= into * w.y;
                }
            }
        }
        if let Some(top) = ceiling_over_body(view, at, to.z, radius, 2.0 * half) {
            to.z = top;
            v.z = v.z.min(0.0);
            p.notes.hit_wall = Some(Vec3::NEG_Z);
        }
        if v.z < 0.0
            && let Some(z) = view.ground_at(to, radius, (at.z - to.z).max(0.0) + 1.0, 0.0)
            && to.z < z
        {
            to.z = z;
            v.z = 0.0;
            p.notes.hit_wall = Some(Vec3::Z);
        }
        at = to;
    }
    p.vel = v;
    p.loc = at + Vec3::Z * half;
}

/// Its move this frame in steps no longer than half its radius: a fast one (a
/// Leaper's dive) meets a wall as it comes to it, not deep inside it (MoveActor
/// sweeps its cylinder to what it first touches).
fn sub_steps(d: Vec3, radius: f32) -> usize {
    let step = (radius * 0.5).max(20.0);
    ((d.length() / step).ceil() as usize).clamp(1, 16)
}

/// Where its feet stop rising from `start` towards the height `to_z` under the
/// lowest ceiling over its head on the way (none if it isn't rising or meets none).
fn ceiling_over(p: &Pawn, view: &WorldView, start: Vec3, to_z: f32) -> Option<f32> {
    ceiling_over_body(view, start, to_z, p.radius, 2.0 * p.half_height)
}

/// The same for a body `radius` wide and `height` tall, its bottom at `start`.
fn ceiling_over_body(
    view: &WorldView,
    start: Vec3,
    to_z: f32,
    radius: f32,
    height: f32,
) -> Option<f32> {
    let rise = to_z - start.z;
    if rise <= 0.0 {
        return None;
    }
    view.ceiling_at(start + Vec3::Z * height, radius, rise)
        .map(|c| (c - height).max(start.z))
}

/// The pawns' cylinders block each other: this one is pushed out of those that
/// block (bBlockActors: the players always, the AIs unless they follow a path),
/// a Bump for each it touches. Two AIs that both block share the push.
fn separate(p: &mut Pawn, view: &WorldView) {
    let mut push = Vec3::ZERO;
    let mut bumped = Vec::new();
    let mut test = |e: Entity, loc: Vec3, radius: f32, half: f32, share: bool| {
        if e == p.entity {
            return;
        }
        if (loc.z - p.loc.z).abs() > half + p.half_height {
            return;
        }
        let d = Vec3::new(p.loc.x - loc.x, p.loc.y - loc.y, 0.0);
        let min = radius + p.radius;
        let len = d.length();
        if len < min {
            let n = if len > 1e-3 {
                d / len
            } else {
                ue::yaw_vec(p.yaw) * -1.0
            };
            push += n * (min - len) * if share { 0.5 } else { 1.0 };
            bumped.push(e);
        }
    };
    let blocks = p.block_actors;
    for a in &view.ais {
        if a.alive && a.blocks {
            test(a.entity, a.loc, a.radius, a.half_height, blocks);
        }
    }
    for pl in &view.players {
        if pl.alive {
            test(pl.entity, pl.loc, pl.radius, pl.half_height, false);
        }
    }
    if push != Vec3::ZERO {
        let to = p.feet() + push;
        let to = match view.push_out(to, p.radius, p.half_height * 2.0) {
            Some(back) => to + back,
            None => to,
        };
        p.loc = Vec3::new(to.x, to.y, p.loc.z);
    }
    p.notes.bumped = bumped;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pawn(feet: Vec3) -> Pawn {
        Pawn::new(Entity::PLACEHOLDER, feet, 0.0, 300.0, 350.0, 100.0)
    }

    #[test]
    fn velocity_comes_to_what_its_acceleration_asks() {
        let mut p = pawn(Vec3::ZERO);
        p.accel = Vec3::X;
        calc_velocity(&mut p, 0.1, 600.0);
        assert!((p.vel.x - 180.0).abs() < 1e-3, "{}", p.vel);
        for _ in 0..10 {
            calc_velocity(&mut p, 0.1, 600.0);
        }
        assert!((p.vel.x - 600.0).abs() < 1e-3);
        // Along each of its own axes by its rate: turned 45 degrees, faster.
        p.vel = Vec3::ZERO;
        p.accel = Vec3::new(1.0, 1.0, 0.0);
        calc_velocity(&mut p, 0.1, 6000.0);
        assert!((p.vel.x - 180.0).abs() < 1e-3 && (p.vel.y - 180.0).abs() < 1e-3);
        // Let go, it stops on the ground at its rate and keeps going in the air.
        p.vel = Vec3::X * 600.0;
        p.accel = Vec3::ZERO;
        calc_velocity(&mut p, 0.1, 600.0);
        assert!((p.vel.x - 420.0).abs() < 1e-3);
        p.physics = Physics::Falling;
        calc_velocity(&mut p, 0.1, 600.0);
        assert!((p.vel.x - 420.0).abs() < 1e-3 && (p.vel.z - GRAVITY_Z * 0.1).abs() < 1e-3);
        // A slow ask is at least 385.
        p.physics = Physics::Walking;
        p.vel = Vec3::ZERO;
        p.accel = Vec3::X * 0.1;
        for _ in 0..20 {
            calc_velocity(&mut p, 0.1, 600.0);
        }
        assert!((p.vel.x - MIN_DESIRED_SPEED).abs() < 1e-3);
        // Flying, up and down too; a momentum blueprint keeps half its speed over
        // the top while it coasts (the Leaper's: Neutral 1).
        p.physics = Physics::Flying;
        p.vel = Vec3::ZERO;
        p.accel = -Vec3::Z;
        calc_velocity(&mut p, 0.1, 1200.0);
        assert!((p.vel.z + 180.0).abs() < 1e-3);
        p.momentum = Some(Momentum {
            on_ground: [0.0, 1.0, 0.0],
            in_air: [0.0, 1.0, 0.0],
        });
        p.physics = Physics::Walking;
        p.vel = Vec3::X * 2000.0;
        p.accel = Vec3::ZERO;
        calc_velocity(&mut p, 0.1, 600.0);
        assert!((p.vel.x - 1910.0).abs() < 1e-3);
    }

    #[test]
    fn a_jump_falls_from_its_top_and_a_dive_strikes_the_floor() {
        let Some(map) = crate::MAPS.iter().find(|m| m.stem == "ignition") else {
            return;
        };
        let Some(level) = crate::load_level_data(map) else {
            eprintln!("ignition's level not built: skipped");
            return;
        };
        let view = WorldView {
            level: Some(&level),
            ..Default::default()
        };
        let clips = super::super::clips::Clips::default();
        let start = ue::to_ue(level.player_start);
        let mut p = pawn(start);
        p.accel_rate = 3000.0;
        let dt = 1.0 / 60.0;
        step(&mut p, &view, &clips, dt);
        assert_eq!(p.acrobatics(), Acrobatics::OnTheGround);
        assert!(p.form_jump(Vec3::new(0.0, 0.0, 1500.0)));
        assert!((p.air_speed - 1500.0).abs() < 1e-3);
        assert!(!p.form_jump(Vec3::Z * 100.0), "no jump in the air");
        let mut states = Vec::new();
        for _ in 0..180 {
            step(&mut p, &view, &clips, dt);
            let s = p.acrobatics();
            if states.last() != Some(&s) {
                states.push(s);
            }
        }
        assert_eq!(
            states,
            [
                Acrobatics::Jumping,
                Acrobatics::Falling,
                Acrobatics::OnTheGround
            ]
        );
        // Flown up and dived back down: it strikes the floor (HitWall with the
        // floor's normal) and stands on it.
        let floor = p.feet().z;
        p.acrobatics_fly();
        assert_eq!(p.acrobatics(), Acrobatics::Flying);
        p.accel_rate = 30000.0;
        p.air_speed = 10000.0;
        p.accel = Vec3::Z;
        for _ in 0..10 {
            step(&mut p, &view, &clips, dt);
        }
        // (30000 a second each second for a sixth of a second: 458.)
        assert!(
            (p.feet().z - floor - 458.3).abs() < 1.0,
            "{}",
            p.feet().z - floor
        );
        p.accel = -Vec3::Z;
        let mut struck = None;
        for k in 0..120 {
            step(&mut p, &view, &clips, dt);
            if let Some(n) = p.notes.hit_wall {
                struck = Some((k, n));
                break;
            }
        }
        let (_, n) = struck.expect("it strikes the floor");
        assert!(n.z > WALKABLE_FLOOR_Z);
        assert!((p.feet().z - floor).abs() < 5.0, "{}", p.feet().z - floor);
        p.acrobatics_fall();
        step(&mut p, &view, &clips, dt);
        assert_eq!(p.physics, Physics::Walking);
    }
}
