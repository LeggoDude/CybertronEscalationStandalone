//! Jazz's car in motion: the game's car animations, its wheels and suspension, the
//! jump, the hover mode, the jets and boosters, and the car's half of the
//! transformation.
//!
//! The animations are the game's own (Jazz_VEH_ANIM, built into
//! assets/models/vehicle.glb by tools/build_assets.py), played the way the car's
//! animation tree, VEH_Proxy_p.VEH_Car_ANIMTREE, plays them:
//!
//! - The car hovers unless Jazz boosts (TnCarForm's Hovering and Driving states;
//!   the tree switches on its _BoostPosing). Hovering holds Nav_Hover_Pose_VEH, the
//!   wheels turned flat as jets, blended with the four directional hover poses by
//!   the car's velocity as the tree's TnAccelerationAnimBlend does, with the
//!   ADD_Nav_Hover_VEH bob on top. The transformation ends in the hover pose.
//! - Boosting drops the car onto its wheels: Nav_HoverToBoost_VEH, then the car's
//!   reference pose (Jazz's set has no Nav_Boost_VEH). Its SkelControlWheels turn,
//!   steer and spring the wheels; here that is done on the tyre meshes hung on the
//!   wheel bones. Letting go plays Nav_BoostToHover_VEH back up.
//! - The guns fold out with ADD_Weapon_Equip_VEH when Jazz fires and away again
//!   with ADD_Weapon_Unequip_VEH.
//! - Transform_ToVehicle_VEH and Transform_ToRobot_VEH play on the car together
//!   with the robot's clips, and the two models swap at the times of the clips'
//!   TnAnimNotify_ToggleHidden markers, so the car's parts fly together out of the
//!   folding robot and apart into the unfolding one. The car is drawn where the
//!   robot's pieces are as they swap, which is well above where its clips put it
//!   on its own (transform_heights): folding up it comes out of the robot up there
//!   and settles to where it floats; unfolding, both are carried up from where the
//!   car was to where the robot stands.
//!
//! The numbers are the game's: the car physics (VEH_SHARED_p.Car_Physics,
//! Escalation's), the hover car's (HoverCar_Physics), their suspensions, the scripts
//! that drive them
//! (TnCarForm, TnCarSimulation, TnHoverCarSimulation in TransGame.xxx), and each
//! vehicle's effects at its sockets: the game's particle systems (JazzVehicleForm's
//! CarHover_A_01_FX hover jets, Booster_Jazz_FX boosters, CarJump_01_FX and the
//! Vehland_*_FX landings), played as TnCarForm plays them (vehicle_effects); the
//! hover jets' and boosters' heat haze is their materials' distortion (levelfx.rs).
//!
//! Each chest makes its own vehicle (character.rs): its model
//! (models/infiltrator/vehicle_<name>.glb, its clips in the order below), its
//! wheels on its wheel sockets (the build's Wheels part), the timing of its half of
//! the transformation, its wheels' radius and where its jets are. Snarl's walks:
//! hovering is walking on its legs (its Nav_Hover clips) and boosting is running.
//!
//! The Titan's tanks (TnTankForm, TnHoverTankSimulation) only hover: boosting they
//! fly straight ahead (main.rs), still floating, in their Nav_Boost_VEH pose, the
//! boosters lit; their cannon follows the view's pitch (aim_cannons) and plays its
//! reload (ADD_TankShooting_Reload, in the car guns' place) after each shell.
//! Grimlock's dino walks as Snarl does, on a tank's physics. The walkers (Snarl,
//! Slug, Grimlock) play their jump clip from a jump until they come down, looping,
//! and their landing clip after half a second in the air: moving, the hard landing,
//! standing still, the idle one (TnCarForm's and TnTankForm's PlayAnimation of their
//! blueprints' HoverJumpAnim, HoverLandAnim and HoverIdleLandAnim).
//!
//! The Scientist's jets (TnPlaneForm) hover and fly (jet.rs), turned and banked as
//! their bodies are, as VEH_Plane_ANIMTREE plays them: hovering, the hover pose
//! blended with its directions and its climb (its TnAccelerationAnimBlend, its Up
//! child Nav_Hover_Ascent_VEH) and the bob on top; taking off, Nav_Hover_to_Boost_VEH
//! into flying, banked by their roll (TnRollAnimBlend: Nav_Boost_L_VEH and
//! Nav_Boost_R_VEH in by 45 degrees of it); dropping back, Nav_Boost_to_Hover_VEH,
//! or after a crash Nav_Collision_VEH (its BoostMalfunction). Their hover jets burn
//! while they hover and their afterburners and contrails while they fly
//! (vehicle_effects), and their engines and boosters sound as they do (main.rs).

use super::*;
use crate::character::{Ground, Hull};
use bevy::camera::primitives::Aabb;
use bevy::math::Affine3A;
use bevy::prelude::*;

// The car's clips in vehicle.glb, in the order tools/build_assets.py writes them.
const CAR_DRIVE: usize = 0;
const CAR_TO_VEHICLE: usize = 1;
const CAR_TO_ROBOT: usize = 2;
const CAR_HOVER: usize = 3;
const CAR_HOVER_F: usize = 4;
const CAR_HOVER_B: usize = 5;
const CAR_HOVER_L: usize = 6;
const CAR_HOVER_R: usize = 7;
const CAR_DRIVE_TO_HOVER: usize = 8;
const CAR_HOVER_TO_DRIVE: usize = 9;
// The guns' own clips (both guns), poses rather than additive.
const CAR_GUN_EQUIP: usize = 10;
const CAR_GUN_FIRE: usize = 11;
const CAR_GUN_UNEQUIP: usize = 12;
// Additive: the hover bob and the gun pods sliding out of the body and back.
const CAR_HOVER_BOB: usize = 13;
const CAR_GUNS_OUT: usize = 14;
const CAR_GUNS_IN: usize = 15;
/// A tank's cannon reloading (its ADD_TankShooting_Reload, in the gun pods' place:
/// tools/build_class.py's TANK_ADDITIVE).
const CAR_CANNON_RELOAD: usize = CAR_GUNS_OUT;
/// How long the cannon takes to turn to the view once a tank has come together
/// (its skeletal control's blend in).
const CANNON_BLEND_SECS: f32 = 0.25;
const CAR_FIRST_ADDITIVE: usize = CAR_HOVER_BOB;
const CAR_END_ADDITIVE: usize = CAR_GUNS_IN + 1;
/// A jet's own (tools/build_class.py's PLANE_CLIPS, after the additive ones):
/// banking left and right, climbing, and the crash out of flying.
const CAR_BANK_L: usize = 16;
const CAR_BANK_R: usize = 17;
const CAR_HOVER_UP: usize = 18;
const CAR_CRASH: usize = 19;
/// A walker's own in the jets' place (tools/build_class.py's WALKER_CLIPS: its
/// blueprint's HoverJumpAnim, HoverLandAnim and HoverIdleLandAnim, the same clips
/// as its BoostJumpAnim and BoostLandAnim): the jump, looping from the jump until
/// it comes down, and the landing, moving and standing still, once through.
const CAR_JUMP: usize = 16;
const CAR_LAND: usize = 17;
const CAR_LAND_IDLE: usize = 18;
const CAR_ANIM_COUNT: usize = 20;
/// A jet's TnAccelerationAnimBlend: MaxVelocity 3000 (the cars' 2000).
const JET_ANIM_MAX_SPEED: f32 = 30.0;
/// Its TnRollAnimBlend: banked all the way in by MaxAngle (45 degrees) of roll.
const JET_BANK_DEGREES: f32 = 45.0;

/// Whether a car clip is one of the additive ones (added on top of the rest).
fn is_additive(clip: usize) -> bool {
    (CAR_FIRST_ADDITIVE..CAR_END_ADDITIVE).contains(&clip)
}
/// TnAccelerationAnimBlend MaxVelocity (2000): the speed at which a hover pose is
/// fully in.
const HOVER_ANIM_MAX_SPEED: f32 = 20.0;
/// Weapon_Equip's length: the guns fire from it on.
const GUN_EQUIP_SECS: f32 = 0.4;

/// The hover transitions' length in the game, in seconds. (The transformation's
/// clips and the moments the car shows and goes, TnAnimNotify_ToggleHidden, are
/// each vehicle's: Player::kit's timing, character.rs.)
const HOVER_SWITCH_SECS: f32 = 0.533;

/// How each vehicle drives is its own (Kit::handling and Kit::drive, character.rs:
/// the car and hover physics blueprints Escalation gives it, the second of each its
/// form lists: TnSurvivalGame.GetSkillDataIndex; on its wheels drive.rs).
/// Jazz's car's, Car_Physics: JumpLinearVelocity (200, 0, 1500), 15 m/s up and
/// 2 m/s forward, with JumpAngularVelocity 0.4 lifting the nose, falling at the
/// rigid bodies' gravity; boosting on the wheels MaxSpeed 3100, MaxAcceleration
/// 1200. His hover car (HoverCar_Physics, the rest TnHoverCarSimulationBlueprint's
/// defaults): JumpLinearSpeed 1550, MaxLinearSpeed 1500, MaxLinearAcceleration
/// 3000, DriftDuration 0.5. After the wheels (and after transforming) it drifts: its grip
/// comes back as (1 - left / DriftDuration)^2 over DriftDuration
/// (TnHoverCarSimulation.CalculateDriftScale). Its JumpAngularSpeed -5: the hover
/// jump kicks the nose up at 5 rad/s while the hover suspension still reaches the
/// ground, i.e. until the car is 50 units higher (InAirRestingLength 200 against
/// RestingLength 150, the trucks' 300 against 250 alike: HOVER_KICK) ...
const HOVER_KICK: f32 = 0.5;
/// ... and in the air the hover car rights itself (AIR_HOLD). On the ground its four
/// springs (HoverCar_Supension: stiffness 8000, damping 4000, SuspensionRadius 100
/// round the centre of mass; the car's 1500 kg and pitch inertia 9.8e6) pitch it
/// about 0.6 times a second, critically damped (each vehicle's: Ground::
/// hover_pitch); a tank's, whose mass the game takes from its physics asset, pitch
/// and lean it so.
const HOVER_PITCH_SPRING: (f32, f32) = (16.0, 8.0);
/// The springs' pull and damping on the body when its vehicle's own aren't known
/// (character.rs's Ground: Jazz's car's wheels against its pitch, against its rise
/// and fall, and its hover's against its rise and fall).
const WHEEL_PITCH_SPRING: (f32, f32) = Ground::JAZZ.wheel_pitch;
const WHEEL_RISE_SPRING: (f32, f32) = Ground::JAZZ.wheel_rise;
const HOVER_RISE_SPRING: (f32, f32) = Ground::JAZZ.hover_rise;
/// In the air a hover car (and a tank) turns back towards level by 5% of what is
/// between them a frame (TnHoverCarSimulation.UpdateTurn with no contact,
/// get_ShouldUpright; TnHoverTankSimulation's TurnRate).
const AIR_HOLD: f32 = 0.05;
/// The most its body is pitched or leaned (radians: 34 degrees), on the ground or
/// in the air.
const MAX_TILT: f32 = 0.6;
/// How far above and below its origin the ground under each of its springs is
/// looked for: as far as it climbs or drops a step (main.rs's STEP_HEIGHT). A spring
/// that finds none there hangs that far down.
const GROUND_REACH: f32 = STEP_HEIGHT;
/// How fast the ground under it is taken to change at most (radians or metres a
/// second), for the dampers: the ground's own edges (a step under a wheel) change
/// it at once.
const GROUND_RATE_MAX: f32 = 6.0;
/// Both forms wait 0.3 s on the ground between jumps (TnCarForm kTimeBetweenJumps).
pub const TIME_BETWEEN_JUMPS: f32 = 0.3;
/// A jump on the wheels boosts the car for 0.7 s (TnCarSimulation's
/// JumpBoostDuration: its boosters' colour, vehicle_effects).
const JUMP_BOOST_SECS: f32 = 0.7;
/// A crash while boosting, within 30 degrees of head-on (kCosCollisionNormalThreshold
/// 0.866), drops the car back to hovering for a second (kCollisionCooldown).
pub const BOOST_CRASH_COS: f32 = 0.866;
pub const BOOST_CRASH_COOLDOWN: f32 = 1.0;
/// Landings: effects after half a second in the air (TnCarForm.UpdateInAir), the
/// low ones under LowLandingAirTimeThreshold (1 s: each vehicle's, Vehicle::
/// low_landing_secs, for its own effects). The hover car has only a low one
/// (Vehland_Low_XSm_FX); Jazz's HighLandingHoverFX is empty.
pub(crate) const LANDING_AIR_SECS: f32 = 0.5;
const LOW_LANDING_AIR_SECS: f32 = 1.0;
/// The hover jets' size (TnCarForm.Hovering.UpdateFx): the push each jet gives,
/// the body's acceleration against gravity along the jet, over
/// ThrusterLinearAccelerationLimit (3000), smoothed (10% a frame).
const THRUSTER_ACCEL_LIMIT: f32 = 30.0;
const THRUSTER_SMOOTHING: f32 = 6.0;
/// The turn a hover jet's push gives against the body's angular acceleration is
/// over ThrusterAngularAccelerationLimit (6 rad/s^2: TnCarForm's).
const THRUSTER_ANGULAR_LIMIT: f32 = 6.0;
/// (Hovering, the car floats about as high as it stands on its wheels: its four
/// hover springs (150 units at rest, round its centre of mass) and its four wheels'
/// (35, under its wheel mounts, less the wheels' radius) bear the same 1500 kg,
/// and it comes out 1.5 cm higher on its jets, the wheels turned flat under it;
/// the trucks 15.5 cm. Each vehicle's: Handling::hover_height.)
/// How fast the front wheels turn to the steering (a rate a second): the game sets
/// them to it each frame; here they follow it closely, so a frame's jolt of the mouse
/// doesn't flick them.
const STEER_EASE: f32 = 30.0;
/// (Car_Suspension: the wheels hang 35 cm further down in the air,
/// InAirRestingLength 70 against RestingLength 35, MaxVisualLength 35. Each
/// vehicle's: Handling::wheel_drop.)
/// The wheel arches sit right on the tyres, so a wheel never rises into its arch:
/// a landing only squashes the whole car (tyres and all) a few centimetres.
const SUSPENSION_SQUASH: f32 = 0.06;
/// Springs per unit of the car's 1500 kg mass: Car_Suspension (stiffness 30000,
/// damping 3000) and HoverCar_Supension (8000, 4000).
const WHEEL_SPRING: (f32, f32) = (20.0, 2.0);
pub(crate) const HOVER_SPRING: (f32, f32) = (5.33, 2.67);
/// The guns (a truck's launchers) fold away 3 s after the trigger is let go
/// (TnWeaponVehicle.StopFire: its _PutAwayDuration, then PutAway's
/// TnWeaponEventStopCharging), and at once as they reload (OnReload).
const GUNS_HOLD_SECS: f32 = 3.0;
/// How often the hover jets puff.
const HOVER_PUFF_SECS: f32 = 0.08;
/// Folding up, the car settles from where it came out of the robot to where it
/// floats this fast (critically damped, radians a second: most of the way in
/// half a second).
const FOLD_SETTLE_RATE: f32 = 6.0;
/// Unfolding, both models are carried from where the car was to where the robot
/// stands over this long (TnPawn.MoveToSafeTransformationLocation's mesh offset,
/// _TransformBlendTimeRemaining), eased in and out.
const UNFOLD_CARRY_SECS: f32 = 0.5;

// The car's sockets (VH_Jazz_SKEL), in the car's own space (metres; x right, y up,
// -z forward), from the rest pose.
/// Hover_LF / RF / LB / RB, on the wheels.
const HOVER_JETS: [Vec3; 4] = [
    Vec3::new(-0.40, 0.31, -1.13),
    Vec3::new(0.41, 0.31, -1.13),
    Vec3::new(-0.40, 0.31, 0.92),
    Vec3::new(0.41, 0.31, 0.92),
];
/// JumpBoostSocket_C, under the back of the body.
const JUMP_JET: Vec3 = Vec3::new(0.0, 0.1, 0.85);
/// BoostSocket_L / R, the exhausts at the back.
const BOOSTERS: [Vec3; 2] = [Vec3::new(-0.66, 0.44, 1.16), Vec3::new(0.67, 0.44, 1.16)];

/// Jazz's car's jumps and landings (each character's vehicle has its own:
/// character.rs).
const SND_HOVER_JUMP: &str = "BL_VEH_CAR.VEH_SPORT_01_DRIVE_JUMP_START";
const SND_BOOST_JUMP: &str = "BL_VEH_CAR.VEH_SPORT_01_BOOST_JUMP_START";
const SND_WHEELS_LAND_LIGHT: &str = "BL_VEH_CAR.VEH_SPORT_01_WHEELS_LAND_LIGHT";
const SND_WHEELS_LAND_HEAVY: &str = "BL_VEH_CAR.VEH_SPORT_01_WHEELS_LAND_HEAVY";
const SND_HOVER_LAND_LIGHT: &str = "BL_VEH_CAR.VEH_SPORT_01_HOVER_LAND_LIGHT";
const SND_HOVER_LAND_HEAVY: &str = "BL_VEH_CAR.VEH_SPORT_01_HOVER_LAND_HEAVY";

/// The guns on the car's flanks.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Guns {
    #[default]
    Stowed,
    Out,
    Folding,
}

/// What the car is doing, on the player.
#[derive(Component, Default)]
pub struct Car {
    /// Hovering (the default) rather than boosting on the wheels (Shift held), set
    /// by player_movement.
    pub hover: bool,
    /// The car's velocity while hovering (it strafes).
    pub hover_vel: Vec3,
    /// A jump started this frame (player_movement), and while in the air from one,
    /// whether it was the hover's (its JumpingGravityScale is the hover's then).
    pub jumped: bool,
    pub in_jump: Option<bool>,
    /// Seconds of hover drift left (its handling's drift_secs), until the next jump is
    /// allowed (TIME_BETWEEN_JUMPS), and until a crash lets the car boost again.
    pub drift: f32,
    pub jump_wait: f32,
    pub boost_cooldown: f32,
    /// A tank's cannon fired this frame (main.rs's fire): its reload plays, and the
    /// seconds since, while it does.
    pub cannon_fired: bool,
    cannon_reload: Option<f32>,
    /// How far a tank's cannon follows the view rather than its clips (0 to 1).
    cannon_aim: f32,
    /// It jumped or landed this frame: its own effects for that play
    /// (vehicle_effects), the landing's by how it landed.
    fx_jump: bool,
    fx_land: Option<crate::character::FxWhen>,
    /// Seconds left of the boost a jump on the wheels gives
    /// (TnCarSimulation._JumpTimeRemaining, from its JumpBoostDuration): its
    /// boosters burn yellow, fading back (vehicle_effects).
    jump_boost_left: f32,
    /// The body's velocity and spin last frame (world, metres and radians a second)
    /// and its smoothed acceleration and angular acceleration: how hard each hover
    /// jet pushes (vehicle_effects).
    fx_last_vel: Option<Vec3>,
    fx_last_heading: Option<f32>,
    fx_last_spin: Vec3,
    fx_accel: Vec3,
    fx_spin_accel: Vec3,
    /// The animation's idea of hovering (switches after a transition clip).
    hovering: bool,
    /// A transition or transformation clip playing, and its seconds left.
    action: Option<(usize, f32)>,
    /// A clip to start this frame, with its speed.
    start: Option<(usize, f32)>,
    /// The transformation clip playing on the car and its seconds so far.
    transforming: Option<(usize, f32)>,
    last_transform_left: f32,
    /// How much higher the car is drawn as it changes form (transform_heights),
    /// and how high it was as it began unfolding (its lift).
    transform_lift: f32,
    unfold_from: f32,
    /// Seconds into its unfolding clip's blend-in from the pose it was in
    /// (main.rs's TRANSFORM_BLEND_IN), while it blends in.
    blend_in: Option<f32>,
    guns: Guns,
    start_guns: Option<usize>,
    since_shot: f32,
    /// The vehicle weapon's meshes (the car guns, a truck's launchers) show: not
    /// from when it is equipped as the car forms (TnWeaponEffectHideWeapon on
    /// TnWeaponEventEquip) until they first come out to fire (TnWeaponVehicle.
    /// CheckForWeaponOut shows them).
    guns_shown: bool,
    /// Hover lift and wheel travel (metres) and their speeds.
    lift: f32,
    lift_vel: f32,
    travel: f32,
    travel_vel: f32,
    /// How much higher the body is drawn than its springs hold it so that none of
    /// its tyres, as its clips pose them, reaches under the ground (metres, easing
    /// at WHEEL_CLEAR_RATE to the deepest tyre_depth less the wheels' travel).
    wheel_clear: f32,
    /// Nose-up pitch and sideways lean (radians: the right side up), with their
    /// speeds, and the lean it is easing towards (drive.rs's bank).
    pitch: f32,
    pitch_vel: f32,
    roll: f32,
    roll_vel: f32,
    bank_target: f32,
    /// Front wheel steering (radians).
    steer: f32,
    was_grounded: bool,
    last_vertical: f32,
    air_time: f32,
    hover_puff: f32,
    /// Seconds left of the hover jump's nose-up kick.
    kick: f32,
    /// The body's vertical speed last frame and its smoothed acceleration (the hover
    /// jets' push).
    last_body_vel: f32,
    accel: f32,
    since_equip: f32,
    /// The ground under its springs last frame (ground_under), and how far the body
    /// is settled onto it (metres, up) and how fast.
    last_fit: Option<GroundFit>,
    settle: f32,
    settle_vel: f32,
    /// Where the body is this frame: raised (hover lift, suspension and settling)
    /// and tilted.
    raise: f32,
    tilt: Quat,
    /// A walker's jump or landing clip playing (CAR_JUMP, CAR_LAND, CAR_LAND_IDLE),
    /// and whether it starts this frame (animate_car), and where it was last frame,
    /// in seconds (below 0 as it starts: car_clip_sounds).
    leap: Option<usize>,
    start_leap: bool,
    leap_time: f32,
    /// The car model's clip weights (animate_car).
    weights: Vec<f32>,
    /// Where the looping clips that can play sounds (SOUND_CLIPS) were last frame,
    /// in seconds (car_clip_sounds).
    clip_times: [f32; SOUND_CLIPS.len()],
    /// A jet's flight (jet.rs).
    pub jet: crate::jet::JetState,
}

impl Car {
    /// A point on the car (its own space) where the body is now: lifted, sprung and
    /// tilted (the guns' muzzles).
    pub fn on_body(&self, local: Vec3) -> Vec3 {
        self.tilt * local + Vec3::Y * self.raise
    }

    /// How far the body is raised (hover lift and suspension).
    pub fn raised(&self) -> f32 {
        self.raise
    }

    /// How the body is turned against the player's heading (pitched and leaned; a
    /// jet's, as its body is).
    pub fn tilt(&self) -> Quat {
        self.tilt
    }
}

/// The car models' animation graphs: one node per clip. One per vehicle
/// (character.rs's order), or vehicle.glb's alone without the characters.
#[derive(Resource)]
pub struct CarAnimations {
    sets: Vec<CarClips>,
}

struct CarClips {
    graph: Handle<AnimationGraph>,
    nodes: Vec<AnimationNodeIndex>,
}

/// Marks the car model's AnimationPlayer, with the graph it plays.
#[derive(Component)]
pub struct CarAnimator(usize);

/// A tyre mesh hung on a wheel bone (character.rs's VehicleWheel, or vehicle.glb's
/// own `_Tyre` nodes): steered (front), spun and sprung here.
pub use crate::character::VehicleWheel as CarTyre;

/// A tank's cannon bone in its model (character.rs's Tank::cannon_bone), which
/// aim_cannons turns to the view.
#[derive(Component)]
pub struct CannonBone;

/// A bone of a tank's model its own effects play on (character.rs's
/// VehicleFx::bone).
#[derive(Component)]
pub struct FxBone;

/// The preview tanks' and jets' effects' Color on the Customize screen
/// (TnVehicleFxPlayer.Play sets it to the owner's EnergonColor): cyan, as the game's
/// default team colouring (MpColoringMode 0) gives a character there
/// MpSameTeamEnergonColor (TnCharacterApplier.GetEnergonColorForTeam). In a match
/// they take their owner's energon colour, his faction's (vehicle_effects).
pub(crate) const TANK_FX_COLOR: &[(&str, [u8; 4])] = &[("Color", [0, 255, 255, 255])];

/// One of the car guns' meshes on a vehicle model (its two copies of
/// WEP_AssaultRifleVH_SKEL on the weapon sockets: tools/build_class.py's
/// GunL_Mesh and GunR_Mesh), hidden while the vehicle weapon is put away
/// (car_guns_shown).
#[derive(Component)]
pub struct CarGunMesh;

/// How long into folding up the car's guns appear: the game puts the robot's gun
/// away as the change starts and equips the vehicle weapon that long after
/// (TnPawn.TransformingToAlt.StoreOffRobotWeapon: TransformToAltShootingDelay,
/// 1 s), while the car's pieces are still flying together.
pub const CAR_GUNS_DELAY: f32 = 1.0;

/// Whether the car's guns show as it changes form (`changing`: to the car or not,
/// and the seconds since the change began): not until CAR_GUNS_DELAY into folding
/// up, and not at all while unfolding (TransformingToRobot.StoreOffAltWeapon puts
/// the vehicle weapon away at once: UnEquipWeaponInstantly).
pub fn car_guns_shown(changing: Option<(bool, f32)>) -> bool {
    match changing {
        Some((true, secs)) => secs >= CAR_GUNS_DELAY,
        Some((false, _)) => false,
        None => true,
    }
}

/// Whether vehicle.glb carries the car's animations (older builds don't).
pub fn model_is_animated(path: &std::path::Path) -> bool {
    std::fs::read(path)
        .map(|bytes| {
            // The glTF JSON chunk comes first; its length is at byte 12.
            let len = bytes
                .get(12..16)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
                .unwrap_or(0);
            let json = bytes.get(20..20 + len).unwrap_or(&[]);
            json.windows(13).any(|w| w == b"\"animations\":")
        })
        .unwrap_or(false)
}

/// Builds the cars' animation graphs (each model's): the ordinary clips blended,
/// the additive ones (hover bob, guns) added on top, as for the robot.
pub fn animation_graphs(
    asset_server: &AssetServer,
    graphs: &mut Assets<AnimationGraph>,
    clips: &mut crate::GltfClips,
    models: &[String],
) -> CarAnimations {
    let sets = models
        .iter()
        .map(|model| {
            let mut graph = AnimationGraph::new();
            let layers = graph.add_additive_blend(1.0, graph.root);
            let base = graph.add_blend(1.0, layers);
            let mut pending = Vec::new();
            let nodes = (0..CAR_ANIM_COUNT)
                .map(|i| {
                    let parent = if is_additive(i) { layers } else { base };
                    crate::gltf_clip(&mut graph, &mut pending, i, parent)
                })
                .collect();
            let graph = graphs.add(graph);
            clips.wait(asset_server.load(model.clone()), graph.clone(), pending);
            CarClips { graph, nodes }
        })
        .collect();
    CarAnimations { sets }
}

/// The graph of a car model (its vehicle's, by the model's CarWorn).
fn car_set(
    entity: Entity,
    parents: &Query<&ChildOf>,
    worn: &Query<&crate::character::CarWorn>,
) -> usize {
    parents
        .iter_ancestors(entity)
        .find_map(|e| worn.get(e).ok())
        .map_or(0, |w| w.vehicle)
}

/// Whether `entity` is part of the car model.
pub fn in_vehicle(
    entity: Entity,
    parents: &Query<&ChildOf>,
    vehicles: &Query<(), With<VehicleForm>>,
) -> bool {
    parents.iter_ancestors(entity).any(|e| vehicles.contains(e))
}

/// Hooks the car model's AnimationPlayer up to its graph once it has loaded, and
/// starts the looping clips (their weights decide what is seen).
pub fn attach_car_animator(
    mut commands: Commands,
    anims: Option<Res<CarAnimations>>,
    mut new_players: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    vehicles: Query<(), With<VehicleForm>>,
    worn: Query<&crate::character::CarWorn>,
) {
    let Some(anims) = anims else {
        return;
    };
    for (entity, mut animation_player) in &mut new_players {
        if !in_vehicle(entity, &parents, &vehicles) {
            continue;
        }
        let set = car_set(entity, &parents, &worn).min(anims.sets.len().saturating_sub(1));
        let Some(clips) = anims.sets.get(set) else {
            continue;
        };
        commands
            .entity(entity)
            .insert((AnimationGraphHandle(clips.graph.clone()), CarAnimator(set)));
        let anims = clips;
        // The guns start folded away (the end of Weapon_UnEquip).
        animation_player
            .play(anims.nodes[CAR_GUN_UNEQUIP])
            .seek_to(1.0)
            .set_weight(1.0);
        // (A jet's banks and climb too; the cars' models have none of those clips.)
        for clip in [
            CAR_DRIVE,
            CAR_HOVER,
            CAR_HOVER_F,
            CAR_HOVER_B,
            CAR_HOVER_L,
            CAR_HOVER_R,
            CAR_HOVER_BOB,
            CAR_BANK_L,
            CAR_BANK_R,
            CAR_HOVER_UP,
        ] {
            let weight = if clip == CAR_DRIVE { 1.0 } else { 0.0 };
            animation_player
                .play(anims.nodes[clip])
                .repeat()
                .set_weight(weight);
        }
    }
}

/// The ground under a vehicle's springs, as its body sits on it: the plane through
/// where each spring finds the ground, its pitch (nose up) and lean (the right side
/// up) under the vehicle, radians, and its height at the vehicle's origin, over
/// the origin (metres), never lower than `floor`: the lowest the body may settle,
/// so pitched and leaned, without sinking into the ground under its springs or
/// under the middle of it, where it stands (ground_under).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundFit {
    pub pitch: f32,
    pub roll: f32,
    pub height: f32,
    pub floor: f32,
}

impl Default for GroundFit {
    fn default() -> Self {
        Self {
            pitch: 0.0,
            roll: 0.0,
            height: 0.0,
            floor: -GROUND_REACH,
        }
    }
}

/// How high over a vehicle's origin a point of its body (`across`, `ahead` of the
/// origin) is when it is pitched and leaned (as car.rs turns it).
fn tilted_height(p: Vec2, pitch: f32, roll: f32) -> f32 {
    (Quat::from_rotation_x(pitch) * Quat::from_rotation_z(roll) * Vec3::new(p.x, 0.0, -p.y)).y
}

/// The plane (least squares) through the ground at `points` (across, ahead of the
/// origin: character.rs's Ground) at `heights` over the origin, and the body's
/// pitch and lean on it (pitched about its own across, then leaned about its own
/// length, as car.rs turns it), each at most MAX_TILT.
pub fn fit_ground(points: &[Vec2; 4], heights: &[f32; 4]) -> GroundFit {
    // h = a + b across + c ahead.
    let mut m = Mat3::ZERO;
    let mut r = Vec3::ZERO;
    for (p, &h) in points.iter().zip(heights) {
        let row = Vec3::new(1.0, p.x, p.y);
        m += Mat3::from_cols(row * row.x, row * row.y, row * row.z);
        r += row * h;
    }
    let (a, b, c) = if m.determinant().abs() > 1e-6 {
        let s = m.inverse() * r;
        (s.x, s.y, s.z)
    } else {
        (r.x / 4.0, 0.0, 0.0)
    };
    let pitch = c.atan().clamp(-MAX_TILT, MAX_TILT);
    GroundFit {
        pitch,
        roll: (b * pitch.cos()).atan().clamp(-MAX_TILT, MAX_TILT),
        height: a.clamp(-GROUND_REACH, GROUND_REACH),
        ..default()
    }
}

/// The ground under a vehicle at `at` (facing -Z by its turn) at its springs'
/// `points` (across, ahead), as fit_ground lays it: each spring finds the highest
/// floor under it within GROUND_REACH of the origin (none: it hangs that far down),
/// an overhang's underside only below the origin (main.rs's ground_moving).
///
/// The plane is no lower than lets the body clear the ground at its springs and
/// where the vehicle stands, `radius` its (ground_moving's five points round its
/// origin), and its rigid body's box (`hull`, its root `ride` over the plane: the
/// hover's lift) clear the ground under its corners and sides: a spring over lower
/// ground beside it (a walkway's edge, a gap between floor plates) pulls the plane
/// down, but the body rests on what is under it, as the game's rigid body does,
/// its hover jets or wheels never in the floor.
pub fn ground_under(
    level: &Level,
    at: &Transform,
    points: &[Vec2; 4],
    radius: f32,
    hull: Option<&Hull>,
    ride: f32,
) -> GroundFit {
    let y = at.translation.y;
    let ground = |p: Vec2| {
        let w = at.translation + at.rotation * Vec3::new(p.x, 0.0, -p.y);
        level
            .ground_at(w.x, w.z, y + GROUND_REACH, y - GROUND_REACH, y)
            .map(|g| g - y)
    };
    let heights = points.map(|p| ground(p).unwrap_or(-GROUND_REACH));
    let mut fit = fit_ground(points, &heights);
    // (ground_moving's points are round the origin along the map's axes.)
    let r = radius * 0.5;
    let middle = [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)].map(|(dx, dz)| {
        let local = at.rotation.inverse() * Vec3::new(dx, 0.0, dz);
        Vec2::new(local.x, -local.z)
    });
    // How low it may settle, pitched and leaned as the ground is, for each point
    // of it that finds the ground `clear` under it.
    let lowest = |points: &mut dyn Iterator<Item = (Vec2, f32, f32)>| {
        points
            .map(|(p, h, clear)| h - clear - tilted_height(p, fit.pitch, fit.roll))
            .fold(-GROUND_REACH, f32::max)
            .min(GROUND_REACH)
    };
    // Its springs (that find the ground) and where it stands, at once: the jets or
    // wheels are never in the floor, nor its middle under where it stands (no
    // higher: the origin is on the highest of them, but for the frame it steps up
    // a stair while the next step is in reach).
    let mut held = points
        .iter()
        .zip(heights)
        .filter(|(_, h)| *h > -GROUND_REACH)
        .map(|(&p, h)| (p, h, 0.0))
        .chain(
            middle
                .iter()
                .filter_map(|&p| ground(p).map(|h| (p, h.min(0.0), 0.0))),
        );
    fit.floor = lowest(&mut held);
    // Its box's corners and the middles of its sides, its bottom that far over its
    // root (no lower than the root: a car's box takes in its wheels), its springs
    // settling it onto what they reach (not at once, as it passes a floor plate's
    // edge).
    let mut boxed = hull.into_iter().flat_map(|h| {
        let (w, side) = (h.half_width, (h.ahead - h.behind) * 0.5);
        let clear = (ride + h.bottom).max(0.0);
        [
            Vec2::new(0.0, h.ahead),
            Vec2::new(0.0, -h.behind),
            Vec2::new(-w, side),
            Vec2::new(w, side),
        ]
        .into_iter()
        .filter_map(move |p| ground(p).map(|g| (p, g, clear)))
    });
    let rests = lowest(&mut boxed);
    fit.height = fit.height.max(fit.floor).max(rests);
    fit
}

/// A step of `dt` of a body's pitch, lean or rise `x` (speed `v`) on its springs
/// and its hover: the springs (`pull`: their pull and damping, None off the ground)
/// pulling it to the ground's `(target, target_rate)` and damping it against how
/// the ground moves, and the hover setting its speed, once a frame at the game's 60,
/// to turn back `hold` of the way to level (TnHoverCarSimulation.UpdateTurn).
pub fn tilt_step(
    x: &mut f32,
    v: &mut f32,
    (target, target_rate): (f32, f32),
    pull: Option<(f32, f32)>,
    hold: f32,
    dt: f32,
) {
    const FRAME: f32 = 1.0 / 60.0;
    let steps = (dt / 0.005).ceil().max(1.0) as u32;
    let h = dt / steps as f32;
    for _ in 0..steps {
        let mut accel = 0.0;
        if let Some((k, c)) = pull {
            accel += k * (target - *x) - c * (*v - target_rate);
        }
        if hold > 0.0 {
            accel += (hold * -*x / FRAME - *v) / FRAME;
        }
        *v += accel * h;
        *x += *v * h;
    }
}

/// How far under a vehicle's origin one of its tyres reaches (metres; nothing for
/// one wholly over it): the tyre mesh's bounds (`center`, `half` its half extents,
/// in its own space, the axle its thinnest way) placed by `to_car` (its own space
/// into the car model's, as its clips pose it, with its socket, spin and steering),
/// `scale` the car model's metres a unit. It is measured as the disc it is, not by
/// its box, so a turning wheel reaches as deep at every turn: its centre less its
/// radius across the up, less half its width along its axle's tilt.
pub fn tyre_depth(center: Vec3, half: Vec3, to_car: &Affine3A, scale: f32) -> f32 {
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    let thin = (0..3)
        .min_by(|&a, &b| half[a].total_cmp(&half[b]))
        .unwrap_or(1);
    let c = to_car.transform_point3(center);
    let axle = to_car.transform_vector3(axes[thin] * half[thin]);
    let radius = (0..3)
        .filter(|&k| k != thin)
        .map(|k| to_car.transform_vector3(axes[k] * half[k]).length())
        .fold(0.0, f32::max);
    let width = axle.length();
    let tilt = if width > 0.0 {
        (axle.y / width).abs()
    } else {
        0.0
    };
    let lowest = c.y - radius * (1.0 - tilt * tilt).max(0.0).sqrt() - width * tilt;
    (-lowest * scale).max(0.0)
}

/// How fast the body's tyre clearance follows its tyres (a rate a second): as the
/// clips turn the wheels flat under a car taking to its hover, and back down onto
/// them, it rises and settles with them.
const WHEEL_CLEAR_RATE: f32 = 12.0;

/// A damped spring step: moves `x` towards `target`.
pub(crate) fn spring(x: &mut f32, v: &mut f32, target: f32, (k, c): (f32, f32), dt: f32) {
    // Small steps keep the stiff wheel spring stable at low frame rates.
    let steps = (dt / 0.01).ceil().max(1.0) as u32;
    let h = dt / steps as f32;
    for _ in 0..steps {
        *v += (-k * (*x - target) - c * *v) * h;
        *x += *v * h;
    }
}

/// How much higher than usual the car and the robot are drawn `secs` into a
/// transformation (folding up if `to_car`). While both are seen, the car is
/// `timing`'s lift (character.rs's Timing) above where its clip puts it: among the
/// robot's pieces it comes out of or goes into, where the game's forms hang their
/// meshes (tools/transform_fit.py). Folding up, it holds there until the robot has
/// gone, then settles to `rest`, where it floats as it comes out hovering (car.rs's
/// lift takes over from there). Unfolding, the robot is drawn from where the car
/// was (`from` up: its lift then) and the car that lift above him, and both are
/// carried to where he stands.
pub(crate) fn transform_heights(
    timing: &crate::character::Timing,
    to_car: bool,
    secs: f32,
    rest: f32,
    from: f32,
) -> (f32, f32) {
    if to_car {
        let clip_time = secs * TRANSFORM_SPEED;
        let swap = timing.car_show.max(timing.robot_hide);
        let lift = timing.fold_lift;
        if clip_time <= swap {
            return (lift, 0.0);
        }
        let x = (clip_time - swap) / TRANSFORM_SPEED * FOLD_SETTLE_RATE;
        (rest + (lift - rest) * (1.0 + x) * (-x).exp(), 0.0)
    } else {
        let k = (secs / UNFOLD_CARRY_SECS).clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k);
        let robot = (from - timing.unfold_lift) * (1.0 - k);
        (robot + timing.unfold_lift, robot)
    }
}

/// The car's state each frame: the transformation's timing, hover, guns, wheels,
/// suspension, body motion, jets and their sounds.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn car_state(
    mut commands: Commands,
    time: Res<Time>,
    chars: Res<crate::character::Characters>,
    level: Res<Level>,
    mut players: Query<(
        Entity,
        &Transform,
        &Player,
        &mut Car,
        &PlayerInput,
        Has<LocalPlayer>,
        Has<RemotePlayer>,
    )>,
    mut vehicles: Query<
        (&mut Transform, &mut Visibility, &JazzOf),
        (
            With<VehicleForm>,
            Without<Player>,
            Without<CarTyre>,
            Without<RobotForm>,
        ),
    >,
    // (Their Transforms: carried up as they unfold, transform_heights.)
    mut robots: Query<
        (&mut Visibility, &mut Transform, &JazzOf),
        (
            With<RobotForm>,
            Without<VehicleForm>,
            Without<Player>,
            Without<CarTyre>,
        ),
    >,
    // (Not the car model itself, whose Transform `vehicles` moves, nor the robot's.)
    new_names: Query<
        (Entity, &Name, &Transform),
        (
            Added<Name>,
            Without<CarTyre>,
            Without<VehicleForm>,
            Without<RobotForm>,
        ),
    >,
    mut tyres: Query<(&mut Transform, &CarTyre, &JazzOf), (Without<Player>, Without<VehicleForm>)>,
    mut car_guns: Query<
        (&mut Visibility, &JazzOf),
        (
            With<CarGunMesh>,
            Without<VehicleForm>,
            Without<RobotForm>,
            Without<Player>,
        ),
    >,
    parents: Query<&ChildOf>,
    vehicle_models: Query<(), With<VehicleForm>>,
    mut fxq: ResMut<FxQueue>,
    mut sfx: ResMut<Sfx>,
    // (Where its tyres were drawn last frame, and the car model: tyre_depth.)
    (tyre_bounds, model_places): (
        Query<(&Aabb, &GlobalTransform, &JazzOf), With<CarTyre>>,
        Query<(&GlobalTransform, &JazzOf), With<VehicleForm>>,
    ),
) {
    let dt = time.delta_secs();

    // vehicle.glb's tyre meshes (tools/build_assets.py hangs one on each *_Wheel*_XB
    // bone; the characters' vehicles get theirs from character.rs), and the tanks'
    // cannon bones.
    for (entity, name, transform) in &new_names {
        let n = name.as_str();
        if chars.is_cannon_bone(n) && in_vehicle(entity, &parents, &vehicle_models) {
            commands.entity(entity).insert(CannonBone);
        }
        if chars.is_fx_bone(n) && in_vehicle(entity, &parents, &vehicle_models) {
            commands.entity(entity).insert(FxBone);
        }
        if n == "GunL_Mesh" || n == "GunR_Mesh" {
            commands.entity(entity).insert(CarGunMesh);
        }
        if n.contains("_Wheel") && n.ends_with("_Tyre") {
            commands.entity(entity).insert(CarTyre::on_wheel_bone(
                n.contains("Wheel01"),
                // The tyre mesh's axle (its Y) turned onto the bone's Z.
                Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                    .with_scale(transform.scale),
            ));
        }
    }

    // (Another player's car makes no sounds here: they come from his PC.)
    for (owner, player_transform, player, mut car, input, local, remote) in &mut players {
        // His car model.
        let Some((mut model, mut visibility, _)) =
            vehicles.iter_mut().find(|(_, _, of)| of.0 == owner)
        else {
            continue;
        };
        let at = voice_at(local, player_transform.translation);
        let timing = player.kit.timing;
        let vehicle = player.kit.vehicle.and_then(|v| chars.vehicles.get(v));
        // (Its jets, boosters, jumps and landings are its own effects, the game's:
        // vehicle_effects. vehicle.glb's Jazz, without the characters' data, has the
        // demo's own, fx.rs's.)
        let game_fx = vehicle.is_some_and(|v| !v.effects.is_empty());

        let in_car = player.form == Form::Vehicle;
        let alive = player.dead_left <= 0.0 && !player.dying;

        // The car's half of the transformation starts with the robot's.
        if player.transform_left > 0.0 && car.last_transform_left <= 0.0 {
            let clip = if is_to_vehicle(player.transform_clip) {
                Some((CAR_TO_VEHICLE, timing.car_fold))
            } else if is_to_robot(player.transform_clip) {
                Some((CAR_TO_ROBOT, timing.car_unfold))
            } else {
                None
            };
            if let Some((clip, secs)) = clip {
                car.start = Some((clip, TRANSFORM_SPEED));
                car.action = Some((clip, secs / TRANSFORM_SPEED));
                car.transforming = Some((clip, 0.0));
                // The car comes out of the transformation hovering (the clip ends in
                // the hover pose), keeping Jazz's speed and drifting at first
                // (TnCarForm.Hovering.BeginState). (A jet's flight is jet.rs's from
                // the change's start: its speed, and whether it hovers or already
                // flies, TnPlaneForm.OnActivate.)
                car.hovering = clip == CAR_TO_VEHICLE;
                let jet = player.kit.plane.is_some();
                if !(jet && car.hovering) {
                    car.hover = car.hovering;
                    let up = if jet { player.vertical_velocity } else { 0.0 };
                    car.hover_vel = Vec3::new(player.carry_vel.x, up, player.carry_vel.z);
                    car.drift = player.kit.handling.drift_secs;
                }
                car.guns = Guns::Stowed;
                car.guns_shown = false;
                // (Unfolding, the robot comes from where the car was: still settling
                // if it had only just come together, or held up on its tyres.)
                car.unfold_from = car.lift + car.transform_lift + car.wheel_clear;
                car.wheel_clear = 0.0;
                car.transform_lift = 0.0;
                car.lift = 0.0;
                car.lift_vel = 0.0;
                car.kick = 0.0;
            }
        }
        car.last_transform_left = player.transform_left;
        if let Some((clip, t)) = car.transforming {
            let t = t + dt;
            let clip_time = t * TRANSFORM_SPEED;
            // Where the two are drawn: the car among the robot's pieces, settling
            // to where it floats as it comes out hovering (a walker on its feet).
            let rest = if player.kit.legs {
                0.0
            } else {
                player.kit.handling.hover_height.max(0.0)
            };
            let (car_up, robot_up) =
                transform_heights(&timing, clip == CAR_TO_VEHICLE, t, rest, car.unfold_from);
            car.transform_lift = car_up;
            if let Some((_, mut robot, _)) = robots.iter_mut().find(|(_, _, of)| of.0 == owner)
                && robot.translation.y != robot_up
            {
                robot.translation.y = robot_up;
            }
            if !alive {
                car.transforming = None;
                car.action = None;
            } else if !in_car {
                // Folding up: the car appears before the robot is gone. Unfolding: the
                // car stays until the robot has stepped out of it, and the robot
                // shows once it is coming out (its clip's ToggleHidden).
                let show = match clip {
                    CAR_TO_VEHICLE => clip_time >= timing.car_show,
                    _ => clip_time < timing.car_hide,
                };
                *visibility = if show {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if clip == CAR_TO_ROBOT
                    && let Some((mut robot, _, _)) =
                        robots.iter_mut().find(|(_, _, of)| of.0 == owner)
                {
                    let want = if clip_time >= timing.robot_show {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                    robot.set_if_neq(want);
                }
            }
            let length = if clip == CAR_TO_VEHICLE {
                timing.car_fold
            } else {
                timing.car_unfold
            };
            if car.transforming.is_some() {
                car.transforming = (clip_time < length).then_some((clip, t));
            }
            if car.transforming.is_none() {
                // Done (or cut short): the car's lift takes over where it is, the
                // robot stands where he is.
                if clip == CAR_TO_VEHICLE {
                    car.lift = (car.lift + car.transform_lift).max(0.0);
                    car.lift_vel = 0.0;
                }
                car.transform_lift = 0.0;
                if let Some((_, mut robot, _)) = robots.iter_mut().find(|(_, _, of)| of.0 == owner)
                    && robot.translation.y != 0.0
                {
                    robot.translation.y = 0.0;
                }
            }
            if car.transforming.is_none() && !in_car {
                *visibility = Visibility::Hidden;
                if let Some((mut robot, _, _)) = robots.iter_mut().find(|(_, _, of)| of.0 == owner)
                {
                    robot.set_if_neq(Visibility::Inherited);
                }
            }
        }
        // His car's guns, put away while it changes form and hidden until they
        // first come out.
        let guns = car.guns_shown
            && car_guns_shown(
                car.transforming
                    .map(|(clip, t)| (clip == CAR_TO_VEHICLE, t)),
            );
        for (mut v, of) in &mut car_guns {
            if of.0 == owner {
                v.set_if_neq(if guns {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
        if let Some((clip, left)) = car.action {
            let left = left - dt;
            car.action = (left > 0.0).then_some((clip, left));
        }

        // Hovering, or down on the wheels while boosting (a jet: flying, its clips
        // as long as they are, and the crash's after one).
        let hover = in_car && car.hover && player.transform_left <= 0.0 && alive;
        if in_car && car.transforming.is_none() && hover != car.hovering {
            let (clip, secs) = match (hover, player.kit.plane) {
                (true, Some(p)) if car.jet.crashed => (CAR_CRASH, p.crash_secs),
                (true, Some(p)) => (CAR_DRIVE_TO_HOVER, p.to_hover_secs),
                (false, Some(p)) => (CAR_HOVER_TO_DRIVE, p.to_fly_secs),
                (true, None) => (CAR_DRIVE_TO_HOVER, HOVER_SWITCH_SECS),
                (false, None) => (CAR_HOVER_TO_DRIVE, HOVER_SWITCH_SECS),
            };
            car.start = Some((clip, 1.0));
            car.action = Some((clip, secs));
            car.hovering = hover;
        }
        if !in_car && car.transforming.is_none() {
            car.hovering = false;
        }

        // A tank's cannon reloads after each shell (its clip, one shot long).
        if std::mem::take(&mut car.cannon_fired) {
            car.cannon_reload = Some(0.0);
            car.start_guns = Some(CAR_CANNON_RELOAD);
        }
        if let Some(t) = car.cannon_reload.as_mut() {
            *t += dt;
        }
        // The cannon turns to the view once the tank is together (not a walker's,
        // whose head it is in: Dinobot_ANIMTREE has no cannon control).
        let aims = in_car
            && player.kit.tank
            && !player.kit.legs
            && alive
            && car.transforming.is_none()
            && player.transform_left <= 0.0;
        let step = dt / CANNON_BLEND_SECS;
        car.cannon_aim = (car.cannon_aim + if aims { step } else { -step }).clamp(0.0, 1.0);

        // Guns out while firing, folded away a while after the last shot or as they
        // reload (a tank and a jet have none).
        let reloading = player.reload_left > 0.0;
        let firing = in_car
            && alive
            && player.transform_left <= 0.0
            && input.fire
            && !reloading
            && !player.kit.tank
            && player.kit.plane.is_none();
        car.since_equip += dt;
        if firing {
            car.since_shot = 0.0;
            car.guns_shown = true;
            if car.guns != Guns::Out {
                car.guns = Guns::Out;
                car.start_guns = Some(CAR_GUNS_OUT);
                car.since_equip = 0.0;
            }
        } else {
            car.since_shot += dt;
            if car.guns == Guns::Out && (car.since_shot > GUNS_HOLD_SECS || !in_car || reloading) {
                car.guns = Guns::Folding;
                car.start_guns = Some(CAR_GUNS_IN);
            }
        }

        // Steering: the front wheels turn as far as the car steers (positive to the
        // left: the game's visual wheels take the physics wheels' steering angle).
        let wheeled = player.kit.wheeled();
        let steer_target = if in_car && !car.hovering && alive && wheeled {
            -player.steer * player.kit.drive.max_steer.to_radians()
        } else {
            0.0
        };
        car.steer += (steer_target - car.steer) * (dt * STEER_EASE).min(1.0);

        // Suspension: the wheels hang down in the air and the body sinks onto them
        // when it lands.
        let grounded = player.grounded;
        let landed = in_car && grounded && !car.was_grounded;
        if !grounded {
            car.air_time += dt;
        }
        // A walker's jump stops as it comes down (OnRigidBodyCollision's
        // StopAnimation), and after half a second in the air it lands
        // (OnLandComplete): on the move (boosting, or the stick pushed) its
        // HoverLandAnim, standing its HoverIdleLandAnim.
        if landed && player.kit.legs {
            car.leap = None;
            if car.air_time > LANDING_AIR_SECS && alive && car.transforming.is_none() {
                let moving = !car.hover
                    || player.move_input != Vec2::ZERO
                    || (remote && player.velocity.xz().length() > 0.5);
                car.leap = Some(if moving { CAR_LAND } else { CAR_LAND_IDLE });
                car.start_leap = true;
            }
        }
        if !in_car || !alive || car.transforming.is_some() {
            car.leap = None;
        }
        if landed {
            let fall = (-car.last_vertical).max(0.0);
            car.travel = car.travel.min(0.0);
            car.travel_vel -= fall * 0.05;
            let heavy = car.air_time >= LOW_LANDING_AIR_SECS;
            // The landing's own effect, after half a second in the air
            // (UpdateInAir): a car's or a truck's by how it lands (Driving's and
            // Hovering's OnLandComplete: on its wheels its LowLandingFX, or its
            // HighLandingFX after its LowLandingAirTimeThreshold or more in the air;
            // hovering its LowLandingHoverFX or HighLandingHoverFX), a tank's its
            // LandFX (TnTankForm.UpdateInAir). (Without the game's, the demo's own:
            // on the wheels Vehland_Low_Med_FX, or Vehland_High_XSm_FX after a long
            // time in the air; hovering Vehland_Low_XSm_FX, only after a short one.)
            if car.air_time > LANDING_AIR_SECS && alive && game_fx {
                use crate::character::FxWhen;
                let low =
                    car.air_time < vehicle.map_or(LOW_LANDING_AIR_SECS, |v| v.low_landing_secs);
                car.fx_land = Some(if player.kit.tank || player.kit.plane.is_some() {
                    FxWhen::Land
                } else {
                    match (car.hover, low) {
                        (false, true) => FxWhen::LandLow,
                        (false, false) => FxWhen::LandHigh,
                        (true, true) => FxWhen::HoverLandLow,
                        (true, false) => FxWhen::HoverLandHigh,
                    }
                });
            } else if car.air_time > LANDING_AIR_SECS && alive {
                let effect = match (car.hovering, heavy) {
                    (true, false) => Some(Effect::CarLandHover),
                    (true, true) => None,
                    (false, false) => Some(Effect::CarLandLow),
                    (false, true) => Some(Effect::CarLandHigh),
                };
                if let Some(effect) = effect {
                    fxq.spawn(
                        effect,
                        player_transform.translation + Vec3::Y * 0.1,
                        Vec3::Y,
                    );
                }
            }
            if car.air_time > 0.15 && alive && !remote {
                use crate::character::{
                    SOUND_LAND_HOVER_HEAVY, SOUND_LAND_HOVER_LIGHT, SOUND_LAND_WHEELS_HEAVY,
                    SOUND_LAND_WHEELS_LIGHT,
                };
                let (event, jazz) = match (car.hovering, heavy) {
                    (true, true) => (SOUND_LAND_HOVER_HEAVY, SND_HOVER_LAND_HEAVY),
                    (true, false) => (SOUND_LAND_HOVER_LIGHT, SND_HOVER_LAND_LIGHT),
                    (false, true) => (SOUND_LAND_WHEELS_HEAVY, SND_WHEELS_LAND_HEAVY),
                    (false, false) => (SOUND_LAND_WHEELS_LIGHT, SND_WHEELS_LAND_LIGHT),
                };
                // (His vehicle's own landing, where its audio component plays one: a
                // Dinobot's stamp is its landing clip's, car_clip_sounds.)
                if let Some(cue) = chars.car_sound(player.kit.vehicle, event, jazz) {
                    sfx.play_from(cue, at);
                }
            }
        }
        if grounded {
            car.air_time = 0.0;
        }
        let handling = player.kit.handling;
        let travel_target = if grounded || car.hovering {
            0.0
        } else {
            handling.wheel_drop
        };
        let (mut travel, mut travel_vel) = (car.travel, car.travel_vel);
        spring(
            &mut travel,
            &mut travel_vel,
            travel_target,
            WHEEL_SPRING,
            dt,
        );
        car.travel = travel.clamp(-SUSPENSION_SQUASH, handling.wheel_drop.max(0.0));
        car.travel_vel = travel_vel;
        let (mut lift, mut lift_vel) = (car.lift, car.lift_vel);
        // (Not while the transformation is still putting the car together; a
        // walker walks on its legs.)
        // (A tank floats boosting too, and a jet always.)
        let floats = car.hovering || (in_car && (player.kit.tank || player.kit.plane.is_some()));
        let lift_target = if floats && car.transforming.is_none() && !player.kit.legs {
            handling.hover_height.max(0.0)
        } else {
            0.0
        };
        spring(&mut lift, &mut lift_vel, lift_target, HOVER_SPRING, dt);
        car.lift = lift.max(0.0);
        car.lift_vel = lift_vel;

        // The jump lifts the nose: a little on the wheels, settling back on landing; a
        // sharp kick hovering, which the hover car rights in the air. On the wheels it
        // boosts the car for a moment (TnCarSimulation.Jump: _JumpTimeRemaining), its
        // boosters burning yellow.
        car.jump_boost_left = (car.jump_boost_left - dt).max(0.0);
        if car.jumped {
            car.jumped = false;
            // (A walker's jump clip, looping: TnCarForm's and TnTankForm's
            // UpdateJumping.)
            if in_car && player.kit.legs && alive && car.transforming.is_none() {
                car.leap = Some(CAR_JUMP);
                car.start_leap = true;
            }
            if in_car {
                if car.hovering || player.kit.tank {
                    car.pitch_vel = handling.hover_jump_pitch;
                    car.kick = HOVER_KICK / handling.hover_jump_speed.max(1.0);
                } else {
                    car.pitch_vel = handling.jump_pitch;
                    car.jump_boost_left = JUMP_BOOST_SECS;
                }
                let jets = match vehicle {
                    Some(v) => v.jump_jets.as_slice(),
                    None => &[JUMP_JET],
                };
                if game_fx {
                    car.fx_jump = true;
                } else {
                    for &jet in jets {
                        let jet = player_transform.transform_point(jet + Vec3::Y * car.lift);
                        fxq.spawn_moving(Effect::CarJump, jet, Vec3::NEG_Y, player.velocity);
                    }
                }
                if !remote {
                    let (event, jazz) = jump_sound(car.hovering || player.kit.tank);
                    if let Some(cue) = chars.car_sound(player.kit.vehicle, event, jazz) {
                        sfx.play_from(cue, at);
                    }
                }
            }
        }
        let hovering_now = in_car && (car.hovering || player.kit.tank);

        // Its tyres as its clips pose them (the wheels turned flat under a car or
        // truck as it hovers reach 5 to 37 cm under its origin, as Breakdown's and
        // Soundwave's do on their wheels), less their hanging in the air: the body
        // is drawn that much higher, so that none of them goes into the ground,
        // the hover's lift on top. Folding up, only as far as the car hasn't yet
        // settled down to its hover (transform_heights), so that it comes down onto
        // its tyres and the hover takes over from there. (Not unfolding, a jet nor
        // a walker.)
        let folding = matches!(car.transforming, Some((CAR_TO_VEHICLE, _)));
        let tyres_down = if in_car
            && (car.transforming.is_none() || folding)
            && player.kit.plane.is_none()
            && !player.kit.legs
        {
            model_places
                .iter()
                .find(|(_, of)| of.0 == owner)
                .map_or(0.0, |(place, _)| {
                    let to_model = place.affine().inverse();
                    tyre_bounds
                        .iter()
                        .filter(|(_, _, of)| of.0 == owner)
                        .map(|(bounds, tyre, _)| {
                            tyre_depth(
                                bounds.center.into(),
                                bounds.half_extents.into(),
                                &(to_model * tyre.affine()),
                                model.scale.y,
                            )
                        })
                        .fold(0.0, f32::max)
                })
        } else {
            0.0
        };
        let still_high = if folding {
            (car.transform_lift - player.kit.handling.hover_height.max(0.0)).max(0.0)
        } else {
            0.0
        };
        let clear_target = (tyres_down - car.travel.max(0.0) - still_high).max(0.0);
        car.wheel_clear += (clear_target - car.wheel_clear) * (dt * WHEEL_CLEAR_RATE).min(1.0);

        // The ground under its wheels (or, hovering, its hover's springs; a tank's
        // always), as their springs find it: the body pitches, leans and settles to
        // it as the game's rigid body does on them. Not a jet, nor in the air.
        let ground = player.kit.ground;
        let fit = (in_car && grounded && player.kit.plane.is_none()).then(|| {
            let points = match ground.wheel_points() {
                Some(wheels) if !hovering_now => wheels,
                _ => ground.hover_points(),
            };
            // (Where it stands: main.rs's player movement, a tank by its hull.)
            let radius = match player.kit.hull.filter(|_| player.kit.tank) {
                Some(h) => crate::hull_cylinder(&h, handling.hover_height).0,
                None => player.size().0,
            };
            let hull = player.kit.hull.as_ref();
            ground_under(
                &level,
                player_transform,
                &points,
                radius,
                hull,
                car.lift + car.wheel_clear,
            )
        });
        // (How fast it changes under the car, for the springs' dampers: they damp
        // the body's motion against the ground's.)
        let rate = match (fit, car.last_fit) {
            (Some(f), Some(l)) if dt > 0.0 => GroundFit {
                pitch: ((f.pitch - l.pitch) / dt).clamp(-GROUND_RATE_MAX, GROUND_RATE_MAX),
                roll: ((f.roll - l.roll) / dt).clamp(-GROUND_RATE_MAX, GROUND_RATE_MAX),
                height: ((f.height - l.height) / dt).clamp(-GROUND_RATE_MAX, GROUND_RATE_MAX),
                ..default()
            },
            _ => GroundFit::default(),
        };
        car.last_fit = fit;
        // (A tank's rigid body's mass is its physics asset's: its springs pull as a
        // hover car's do.)
        let or_hover =
            |pull: (f32, f32), default: (f32, f32)| if pull.0 > 0.0 { pull } else { default };
        let hover_pitch = or_hover(ground.hover_pitch, HOVER_PITCH_SPRING);
        let hover_roll = or_hover(ground.hover_roll, HOVER_PITCH_SPRING);
        let hover_rise = or_hover(ground.hover_rise, HOVER_RISE_SPRING);
        let wheel_pitch = or_hover(ground.wheel_pitch, WHEEL_PITCH_SPRING);
        let wheel_rise = or_hover(ground.wheel_rise, WHEEL_RISE_SPRING);

        if car.kick > 0.0 && hovering_now {
            car.kick -= dt;
            car.pitch = (car.pitch + car.pitch_vel * dt).clamp(-MAX_TILT, MAX_TILT);
        } else if !in_car {
            car.kick = 0.0;
            let (mut pitch, mut pitch_vel) = (car.pitch, car.pitch_vel);
            spring(&mut pitch, &mut pitch_vel, 0.0, WHEEL_PITCH_SPRING, dt);
            car.pitch = pitch;
            car.pitch_vel = pitch_vel;
        } else if let Some(f) = fit {
            // On the ground its pitch is its springs' (the hover's leave it free:
            // TnHoverCarSimulation.UpdateTurn, TnHoverTankSimulation's), to the
            // ground's.
            car.kick = 0.0;
            let pull = if hovering_now {
                hover_pitch
            } else {
                wheel_pitch
            };
            let c = &mut *car;
            tilt_step(
                &mut c.pitch,
                &mut c.pitch_vel,
                (f.pitch, rate.pitch),
                Some(pull),
                0.0,
                dt,
            );
        } else if hovering_now {
            // In the air the hover rights it (UpdateTurn's 5% a frame).
            let c = &mut *car;
            tilt_step(
                &mut c.pitch,
                &mut c.pitch_vel,
                (0.0, 0.0),
                None,
                AIR_HOLD,
                dt,
            );
        } else {
            car.pitch = (car.pitch + car.pitch_vel * dt).clamp(-MAX_TILT, MAX_TILT);
        }
        // Its lean. On the wheels, out of turns as far as the car steers (drive.rs's
        // bank: Jazz's car about 15 degrees at full lock), its springs leaning it to
        // the ground against the banking's pull towards the view's level. Hovering
        // (a car's or truck's, a tank's), its hover's springs lean it to the ground,
        // a car's hover holding it towards level (UpdateTurn's 5% a frame, as in the
        // air), a tank's on the ground not at all.
        let g_roll = fit.map_or((0.0, 0.0), |f| (f.roll, rate.roll));
        if in_car && wheeled && alive && !car.hovering {
            let c = &mut *car;
            crate::drive::bank(
                &player.kit.drive,
                player.steer,
                grounded,
                g_roll,
                &mut c.roll,
                &mut c.roll_vel,
                &mut c.bank_target,
                dt,
            );
        } else if in_car && alive && hovering_now && player.kit.plane.is_none() {
            let hold = if fit.is_some() {
                ground.hover_hold
            } else {
                AIR_HOLD
            };
            let pull = fit.is_some().then_some(hover_roll);
            let c = &mut *car;
            tilt_step(&mut c.roll, &mut c.roll_vel, g_roll, pull, hold, dt);
            c.bank_target = 0.0;
        } else {
            car.roll += (0.0 - car.roll) * (dt * 5.0).min(1.0);
            car.roll_vel = 0.0;
            car.bank_target = 0.0;
        }
        car.pitch = car.pitch.clamp(-MAX_TILT, MAX_TILT);
        car.roll = car.roll.clamp(-MAX_TILT, MAX_TILT);
        car.was_grounded = grounded;
        car.last_vertical = player.vertical_velocity;
        // Its rise and fall onto the ground under its springs (the body stands where
        // the ground under them is at its origin, which may be lower than the ground
        // its origin was put on: main.rs's ground takes the highest under it), never
        // sinking into what is under it (GroundFit::floor: its rigid body stops on
        // it).
        let (settle, settle_rate) = fit.map_or((0.0, 0.0), |f| (f.height, rate.height));
        let pull = if hovering_now { hover_rise } else { wheel_rise };
        let c = &mut *car;
        tilt_step(
            &mut c.settle,
            &mut c.settle_vel,
            (settle, settle_rate),
            Some(pull),
            0.0,
            dt,
        );
        if let Some(f) = fit
            && c.settle < f.floor
        {
            c.settle = f.floor;
            c.settle_vel = c.settle_vel.max(0.0);
        }

        // Place the body: lifted while hovering, sunk or raised by the suspension,
        // settled onto the ground, pitched and leaned (a jet turned as its body is:
        // jet.rs). The wheels stay where the ground is.
        let body = if grounded { car.travel } else { 0.0 };
        let tilt = match player.kit.plane {
            Some(_) if in_car => car.jet.tilt,
            _ => Quat::from_rotation_x(car.pitch) * Quat::from_rotation_z(car.roll),
        };
        let raise = car.lift + body + car.transform_lift + car.settle + car.wheel_clear;
        car.raise = raise;
        car.tilt = tilt;
        // (A walker's model is moved to stand on its feet.)
        model.translation = Vec3::Y * (raise + player.kit.lift);
        model.rotation = tilt * Quat::from_rotation_y(MODEL_YAW_OFFSET);
        // The wheels turn as far as the ground they cover over their own radius
        // (player.wheel_angle is over WHEEL_RADIUS), round their wheel bones' axles,
        // the front ones steered round its up (Swindle's on bones of their own,
        // turned another way: VehicleWheel).
        let roll = crate::WHEEL_RADIUS / player.kit.wheel_radius.max(0.05);
        for (mut transform, tyre, of) in &mut tyres {
            if of.0 != owner {
                continue;
            }
            let steer = Quat::from_axis_angle(tyre.up, if tyre.front { car.steer } else { 0.0 });
            let spin = if car.hovering {
                0.0
            } else {
                player.wheel_angle * roll
            };
            transform.rotation =
                steer * Quat::from_axis_angle(tyre.axle, -spin) * tyre.rest.rotation;
            // Hanging down out of the arch, never up into it.
            transform.translation = steer * tyre.rest.translation + tyre.up * -car.travel.max(0.0);
        }

        // Jets: the hover jets under the wheels, sized by how hard they push (they fade
        // out falling and flare on take-off and landing), and the boosters while on the
        // wheels.
        let world = |local: Vec3| player_transform.transform_point(tilt * local + Vec3::Y * raise);
        let body_vel = player.vertical_velocity + car.lift_vel;
        if dt > 0.0 {
            let accel = (body_vel - car.last_body_vel) / dt;
            car.accel += (accel - car.accel) * (THRUSTER_SMOOTHING * dt).min(1.0);
        }
        car.last_body_vel = body_vel;
        let push = ((car.accel - GRAVITY) / THRUSTER_ACCEL_LIMIT).clamp(0.0, 1.0);
        if in_car && floats && car.transforming.is_none() && !game_fx {
            car.hover_puff -= dt;
            if car.hover_puff <= 0.0 && push > 0.05 {
                car.hover_puff = HOVER_PUFF_SECS;
                let jets = match vehicle {
                    Some(v) => v.hover_jets.as_slice(),
                    None => &HOVER_JETS,
                };
                for &jet in jets {
                    fxq.spawn_scaled(
                        Effect::CarHover,
                        world(jet),
                        Vec3::NEG_Y,
                        player.velocity,
                        push,
                    );
                }
            }
        }
        let boosting = in_car && alive && !car.hovering && player.transform_left <= 0.0;
        if boosting && !game_fx {
            let back = *player_transform.back();
            let boosters = match vehicle {
                Some(v) => v.boosters.as_slice(),
                None => &BOOSTERS,
            };
            for &booster in boosters {
                fxq.spawn_moving(Effect::CarBoost, world(booster), back, player.velocity);
            }
        }
    }
}

/// Weights the car's clips each frame: a transformation or hover transition when
/// one is playing, else hovering (blended by direction) or driving, with the hover
/// bob and the guns added on top; and plays the sounds of its clips.
#[allow(clippy::type_complexity)]
pub fn animate_car(
    time: Res<Time>,
    anims: Option<Res<CarAnimations>>,
    chars: Res<crate::character::Characters>,
    mut sfx: ResMut<Sfx>,
    mut players: Query<(
        &Transform,
        &Player,
        &mut Car,
        Has<LocalPlayer>,
        Has<RemotePlayer>,
    )>,
    mut animators: Query<(&mut AnimationPlayer, &JazzOf, &CarAnimator)>,
) {
    let Some(anims) = anims else {
        return;
    };
    let dt = time.delta_secs();
    for (mut animation_player, of, set) in &mut animators {
        let Ok((transform, player, mut car, local, remote)) = players.get_mut(of.0) else {
            continue;
        };
        let Some(anims) = anims.sets.get(set.0) else {
            continue;
        };
        animate_one_car(
            anims,
            dt,
            &mut animation_player,
            transform,
            player,
            &mut car,
        );
        // (Another player's come from his PC.)
        if !remote {
            let at = voice_at(local, transform.translation);
            car_clip_sounds(
                &chars,
                &mut sfx,
                anims,
                &animation_player,
                player,
                &mut car,
                at,
            );
        }
    }
}

/// A vehicle's jump's sound event, and Jazz's car's cue for it: hovering (a tank
/// always), its hover jump's (TnCarForm.Hovering.UpdateJumping's and TnTankForm's
/// PlayAscendSound: Auto_Jump_Start, the DRIVE_JUMP_START cues); boosting on its
/// wheels, its boost jump's (TnCarForm.Driving.UpdateJumping's PlayBoostJumpSound:
/// Auto_Boost_Jump, BOOST_JUMP_START). (The demo had them the other way round.)
fn jump_sound(hovering: bool) -> (&'static str, &'static str) {
    use crate::character::{SOUND_BOOST_JUMP, SOUND_HOVER_JUMP};
    if hovering {
        (SOUND_HOVER_JUMP, SND_HOVER_JUMP)
    } else {
        (SOUND_BOOST_JUMP, SND_BOOST_JUMP)
    }
}

/// The car's looping clips that can play sounds (their sound notifies): driving,
/// hovering and its directions (Snarl's run, stand and walk).
const SOUND_CLIPS: [usize; 6] = [
    CAR_DRIVE,
    CAR_HOVER,
    CAR_HOVER_F,
    CAR_HOVER_B,
    CAR_HOVER_L,
    CAR_HOVER_R,
];

/// The sounds of the car's looping clips (character.rs's, from their sound notifies:
/// the Dinobots' footsteps, Snarl's and Slug's tails and growls), each as the clip's
/// playback passes it: the clip most in view's (the directions are blended, and the
/// stand under them), while he is in the car and alive and no transformation or
/// hover transition plays. A walker's jump or landing plays its own over them (the
/// tree's slot: from its first frame, its weight still coming in), and they none.
fn car_clip_sounds(
    chars: &crate::character::Characters,
    sfx: &mut Sfx,
    anims: &CarClips,
    animation_player: &AnimationPlayer,
    player: &Player,
    car: &mut Car,
    at: Option<Vec3>,
) {
    let shown = player.form == Form::Vehicle
        && player.dead_left <= 0.0
        && !player.dying
        && car.action.is_none();
    let weight = |clip: usize| car.weights.get(clip).copied().unwrap_or(0.0);
    // (Past `t` since last frame, `was`, or over the end and round again.)
    let passed = |was: f32, now: f32, t: f32| {
        if now >= was {
            was < t && t <= now
        } else {
            t > was || t <= now
        }
    };
    if let Some(clip) = car.leap
        && let Some(active) = animation_player.animation(anims.nodes[clip])
    {
        let (was, now) = (car.leap_time, active.seek_time());
        car.leap_time = now;
        if shown {
            for &(t, cue) in chars.car_clip_sounds(player.kit.vehicle, clip) {
                if passed(was, now, t) {
                    sfx.play_from(cue, at);
                }
            }
        }
    }
    let main = SOUND_CLIPS
        .into_iter()
        .max_by(|a, b| weight(*a).total_cmp(&weight(*b)))
        .filter(|&clip| shown && car.leap.is_none() && weight(clip) >= 0.5);
    for (k, clip) in SOUND_CLIPS.into_iter().enumerate() {
        let Some(active) = animation_player.animation(anims.nodes[clip]) else {
            continue;
        };
        let (was, now) = (car.clip_times[k], active.seek_time());
        car.clip_times[k] = now;
        if main != Some(clip) {
            continue;
        }
        for &(t, cue) in chars.car_clip_sounds(player.kit.vehicle, clip) {
            if passed(was, now, t) {
                sfx.play_from(cue, at);
            }
        }
    }
}

/// animate_car for one Jazz's car.
fn animate_one_car(
    anims: &CarClips,
    dt: f32,
    animation_player: &mut AnimationPlayer,
    transform: &Transform,
    player: &Player,
    car: &mut Car,
) {
    let weights = &mut car.weights;
    if weights.len() != CAR_ANIM_COUNT {
        weights.resize(CAR_ANIM_COUNT, 0.0);
        weights[CAR_DRIVE] = 1.0;
    }
    if let Some((clip, speed)) = car.start.take() {
        let active = animation_player.play(anims.nodes[clip]);
        active.replay();
        active.set_speed(speed);
        car.blend_in = None;
        if clip == CAR_TO_ROBOT {
            // Unfolding blends in from the pose it was in (below).
            weights[clip] = 0.0;
            car.blend_in = Some(0.0);
        } else if clip == CAR_TO_VEHICLE {
            // Folding up cuts in (it is hidden as it starts).
            for (i, w) in weights.iter_mut().enumerate() {
                if !is_additive(i) {
                    *w = 0.0;
                }
            }
            weights[clip] = 1.0;
        }
    }
    // The unfolding's blend-in: its weight rising evenly over TRANSFORM_BLEND_IN as
    // the clips it takes over from go.
    let blend_in = match car.blend_in {
        Some(e) => {
            let to = e + dt;
            car.blend_in = (to < TRANSFORM_BLEND_IN && car.transforming.is_some()).then_some(to);
            Some((
                (e / TRANSFORM_BLEND_IN).min(1.0),
                (to / TRANSFORM_BLEND_IN).min(1.0),
            ))
        }
        None => None,
    };
    // A walker's jump (looping) or landing (once through, then back to walking).
    if std::mem::take(&mut car.start_leap)
        && let Some(clip) = car.leap
    {
        let active = animation_player.play(anims.nodes[clip]);
        active.replay();
        active.set_repeat(if clip == CAR_JUMP {
            bevy::animation::RepeatAnimation::Forever
        } else {
            bevy::animation::RepeatAnimation::Never
        });
        // (Its sounds from its first frame on: a landing's stamp is on it.)
        car.leap_time = -1.0;
    }
    if let Some(clip) = car.leap
        && clip != CAR_JUMP
        && animation_player
            .animation(anims.nodes[clip])
            .is_none_or(|a| a.is_finished())
    {
        car.leap = None;
    }
    let mut target = [0.0f32; CAR_ANIM_COUNT];
    let jet = player.kit.plane.is_some();
    if let Some((clip, _)) = car.action {
        target[clip] = 1.0;
    } else if let Some(clip) = car.leap {
        target[clip] = 1.0;
    } else if jet && car.hovering {
        // A jet's TnAccelerationAnimBlend: as the car's, with its MaxVelocity and its
        // Up child (the climb; Down is the hover pose), by its velocity in its own
        // axes, pitch and all.
        let vel = car.hover_vel;
        let turn = transform.rotation * car.tilt;
        let local = turn.inverse() * vel.clamp_length_max(JET_ANIM_MAX_SPEED);
        let upright = (car.tilt * Vec3::Y).y.max(0.0);
        let motion = (Vec3::new(-local.z, local.x, local.y) / JET_ANIM_MAX_SPEED * upright)
            .clamp_length_max(1.0);
        let length = motion.length();
        if length < 1e-4 {
            target[CAR_HOVER] = 1.0;
        } else {
            let w = motion.signum() * motion * motion / length;
            target[CAR_HOVER] = 1.0 - length + (-w.z).max(0.0);
            target[CAR_HOVER_UP] = w.z.max(0.0);
            target[CAR_HOVER_F] = w.x.max(0.0);
            target[CAR_HOVER_B] = (-w.x).max(0.0);
            target[CAR_HOVER_R] = w.y.max(0.0);
            target[CAR_HOVER_L] = (-w.y).max(0.0);
        }
    } else if jet {
        // Flying, its TnRollAnimBlend: banked left or right by its roll (Unreal's,
        // its right wing down positive), all the way at JET_BANK_DEGREES.
        let (_, _, roll) = car.tilt.to_euler(EulerRot::YXZ);
        let roll = -roll.to_degrees();
        let bank = (roll.abs() / JET_BANK_DEGREES).clamp(0.0, 1.0);
        target[CAR_DRIVE] = 1.0 - bank;
        if roll < 0.0 {
            target[CAR_BANK_L] = bank;
        } else {
            target[CAR_BANK_R] = bank;
        }
    } else if car.hovering {
        // TnAccelerationAnimBlend: the car's velocity in its own frame over
        // MaxVelocity (2000), each axis squared over the length (the sign kept),
        // weights the forward / back / right / left poses, with the rest on the
        // hover pose (its up and down children are the hover pose too), all scaled
        // by how upright the car is.
        let vel = car.hover_vel + Vec3::Y * player.vertical_velocity;
        let local = transform.rotation.inverse() * vel.clamp_length_max(HOVER_ANIM_MAX_SPEED);
        let upright = (car.tilt * Vec3::Y).y.max(0.0);
        let motion = (Vec3::new(-local.z, local.x, local.y) / HOVER_ANIM_MAX_SPEED * upright)
            .clamp_length_max(1.0);
        let length = motion.length();
        if length < 1e-4 {
            target[CAR_HOVER] = 1.0;
        } else {
            let w = motion.signum() * motion * motion / length;
            target[CAR_HOVER] = 1.0 - length + w.z.abs();
            target[CAR_HOVER_F] = w.x.max(0.0);
            target[CAR_HOVER_B] = (-w.x).max(0.0);
            target[CAR_HOVER_R] = w.y.max(0.0);
            target[CAR_HOVER_L] = (-w.y).max(0.0);
        }
    } else {
        target[CAR_DRIVE] = 1.0;
    }
    // (A tank bobs whenever it is together: it always hovers, some barely off the
    // ground; a jet while it hovers: VEH_Plane_ANIMTREE's Hovering state.)
    target[CAR_HOVER_BOB] = if jet {
        (car.transforming.is_none()
            && player.form == Form::Vehicle
            && car.hovering
            && car.action.is_none()) as u8 as f32
    } else if player.kit.tank {
        (car.transforming.is_none() && player.form == Form::Vehicle) as u8 as f32
    } else {
        // (A car's or truck's while it hovers: its tree's hover branch.)
        (car.transforming.is_none() && player.form == Form::Vehicle && car.hovering) as u8 as f32
    };

    // The guns' clips switch at once: each starts where the other ends. Out: the
    // pods slide out of the body (additive) while the guns unfold (Weapon_Equip),
    // then spin while firing (Weapon_Fire); in: the reverse.
    let tank = player.kit.tank;
    if let Some(clip) = car.start_guns.take() {
        animation_player.play(anims.nodes[clip]).replay();
        let gun = if clip == CAR_GUNS_OUT {
            CAR_GUN_EQUIP
        } else {
            CAR_GUN_UNEQUIP
        };
        animation_player.play(anims.nodes[gun]).replay();
        if clip == CAR_GUNS_OUT {
            animation_player.play(anims.nodes[CAR_GUN_FIRE]).repeat();
        }
    }
    let out = car.guns == Guns::Out;
    target[CAR_GUNS_OUT] = out as u8 as f32;
    target[CAR_GUNS_IN] = (car.guns == Guns::Folding) as u8 as f32;
    if tank {
        // (Its reload, once through.)
        let reloading = car.cannon_reload.is_some()
            && animation_player
                .animation(anims.nodes[CAR_CANNON_RELOAD])
                .is_some_and(|a| !a.is_finished());
        if !reloading {
            car.cannon_reload = None;
        }
        target[CAR_CANNON_RELOAD] = reloading as u8 as f32;
        target[CAR_GUNS_IN] = 0.0;
    }
    let gun = match car.guns {
        Guns::Out if car.since_equip >= GUN_EQUIP_SECS => CAR_GUN_FIRE,
        Guns::Out => CAR_GUN_EQUIP,
        _ => CAR_GUN_UNEQUIP,
    };
    target[gun] = 1.0;

    // Blend like the tree's transitions (0.05-0.1 s).
    let blend = (dt * 12.0).min(1.0);
    for (i, node) in anims.nodes.iter().enumerate() {
        if (CAR_GUN_EQUIP..=CAR_GUN_UNEQUIP).contains(&i) || i == CAR_GUNS_OUT || i == CAR_GUNS_IN {
            weights[i] = target[i];
        } else if let Some((from, to)) = blend_in
            && !is_additive(i)
        {
            if i == CAR_TO_ROBOT {
                weights[i] = to;
            } else if from < 1.0 {
                weights[i] *= (1.0 - to) / (1.0 - from);
            }
        } else {
            weights[i] += (target[i] - weights[i]) * blend;
        }
        if let Some(active) = animation_player.animation_mut(*node) {
            active.set_weight(weights[i]);
        }
    }
}

/// Turns each tank's cannon to the view (TnTankForm.UpdateCannonRotation: its
/// skeletal control sets the cannon bone's rotation in the world to the view's
/// pitch and the body's yaw, whatever the body's tilt), once its clips have posed
/// it (before the transforms are propagated): blended in as the tank comes
/// together (Car::cannon_aim). The bone lies along the mesh's axes at rest, so
/// that rotation is the model's own turned by the pitch.
#[allow(clippy::type_complexity)]
pub fn aim_cannons(
    players: Query<(&Transform, &Player, &Car), Without<CannonBone>>,
    bones: Query<(Entity, &JazzOf), With<CannonBone>>,
    transforms: Query<&Transform, (Without<Player>, Without<CannonBone>)>,
    mut cannons: Query<&mut Transform, (With<CannonBone>, Without<Player>)>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
) {
    for (bone, of) in &bones {
        let Ok((player_transform, player, car)) = players.get(of.0) else {
            continue;
        };
        if car.cannon_aim <= 0.0 {
            continue;
        }
        // Its parent's rotation in the world this frame, and the model's Root node's
        // (the turn from the mesh's axes to the demo's).
        let mut parent = Quat::IDENTITY;
        let mut root = None;
        let mut e = bone;
        while let Ok(child_of) = parents.get(e) {
            e = child_of.parent();
            if e == of.0 {
                break;
            }
            let Ok(t) = transforms.get(e) else {
                break;
            };
            parent = t.rotation * parent;
            if root.is_none() && names.get(e).is_ok_and(|n| n.as_str() == "Root") {
                root = Some(t.rotation);
            }
        }
        let (Some(root), Ok(mut transform)) = (root, cannons.get_mut(bone)) else {
            continue;
        };
        let parent = player_transform.rotation * parent;
        let aimed = player_transform.rotation * Quat::from_rotation_x(player.aim_pitch) * root;
        let local = parent.inverse() * aimed;
        transform.rotation = transform.rotation.slerp(local, car.cannon_aim);
    }
}

/// The vehicles' own effects (the game's particle systems, levelfx.rs), each on its
/// socket as the vehicle moves (TnVehicleFxPlayer.Play: PlayEffectOnSocket):
///
/// - a car's or truck's hover jets while it hovers (TnCarForm.Hovering.PlayHoverFx)
///   and its boosters and trails while it drives on its wheels (Driving.PlayBoostFx),
///   from the moment its state changes; a tank's hover jets while it floats and its
///   boosters while it boosts (TnTankForm.UpdateFx); a jet's hover jets while it hovers
///   and its afterburners and contrails while it flies (TnPlaneForm). Each starts once
///   as its state does: one that plays itself out (a truck's ram flash as it starts to
///   boost) isn't started again until the state is; the rest are stopped as it ends,
///   what is out of them living out its life (Stop, not killing them);
/// - the jump's as it jumps (OnBeginJump) and the landing's as it comes down after half
///   a second in the air (OnLandComplete: a car's or truck's by how it lands, car_state).
///
/// What is left of them goes as the robot comes out of the vehicle (changing back, as
/// his pieces show) or the vehicle is hidden: a boost's glow and trails are let out
/// into the world as they go (Trail_SatHomingRocket_FX's big glow lives a second or
/// two, its particles left where they came out, and its emitters aren't killed on
/// deactivation), and they hung in the air where the vehicle had been after the robot
/// had stood up.
///
/// Their Color is the owner's energon colour (Play: SetColorParameter, the
/// character's EnergonColor, kept as an sRGB Color); a car's or truck's boosters' is
/// that turned towards yellow while a jump on its wheels boosts it, and more opaque
/// (Driving.UpdateBoostFx: ColorLerp to Yellow by NormalizedJumpTimeRemaining, alpha
/// 100 to 255).
///
/// Hover jets are as big as each pushes (Hovering.UpdateFx: their Size,
/// CalculateBoosterFxAmount times the socket's scale): the share of
/// ThrusterLinearAccelerationLimit (30 m/s^2) the body's smoothed acceleration less
/// gravity has along the way the jet pushes (against its flame), and of
/// ThrusterAngularAccelerationLimit (6 rad/s^2) the turn it would give against the
/// body's smoothed angular acceleration (HoverPhysics.CalculateThrusterLinear- and
/// AngularContribution), at most 1: a car's or truck's against gravity, so at rest
/// they burn full and they die down as it falls; a jet's with none (a jet has none),
/// so at rest they die down.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn vehicle_effects(
    mut fx: ResMut<crate::levelfx::FxSystems>,
    chars: Res<crate::character::Characters>,
    time: Res<Time>,
    mut players: Query<(Entity, &Player, &mut Car, &Transform)>,
    bones: Query<(&GlobalTransform, &Name, &JazzOf), With<FxBone>>,
    level: Res<Level>,
    // (The vehicle models and the robots'.)
    models: Query<(Entity, &Visibility, &JazzOf), Or<(With<VehicleForm>, With<RobotForm>)>>,
    robot_models: Query<(), With<RobotForm>>,
    mut playing: Local<Vec<(Entity, String, crate::levelfx::FxId)>>,
    // (Those stopped or played once, still living out their lives.)
    mut fading: Local<Vec<(Entity, crate::levelfx::FxId)>>,
) {
    use crate::character::FxWhen;
    let dt = time.delta_secs();
    let before = std::mem::take(&mut *playing);
    for (jazz, player, mut car, transform) in &mut players {
        let jumped = std::mem::take(&mut car.fx_jump);
        let landed = std::mem::take(&mut car.fx_land);
        let Some(vehicle) = player.kit.vehicle.and_then(|v| chars.vehicles.get(v)) else {
            continue;
        };
        let plane = player.kit.plane.is_some();
        let tank = player.kit.tank;
        let together = player.form == Form::Vehicle
            && player.dead_left <= 0.0
            && !player.dying
            && car.transforming.is_none()
            && player.transform_left <= 0.0;
        // (A jet's hover jets burn while it hovers and its afterburners while it
        // flies, from the moment it changes: TnPlaneForm's Hovering.EndState and
        // Flying.UpdateFx. A tank floats boosting too.)
        let (floating, boosting) = if plane {
            (together && car.hover, together && !car.hover)
        } else if tank {
            (together, together && !car.hovering)
        } else {
            (together && car.hover, together && !car.hover)
        };
        // The body's smoothed acceleration and angular acceleration (a car's or
        // truck's: TnCarForm.Hovering.UpdateFx, VLerp by 0.1 a frame), for its hover
        // jets' size.
        if !plane && !tank && dt > 0.0 {
            let vel = Vec3::new(
                player.velocity.x,
                player.vertical_velocity + car.lift_vel,
                player.velocity.z,
            );
            let yaw = Quat::from_rotation_y(player.heading);
            let turned = car.fx_last_heading.map_or(0.0, |h| {
                let d = player.heading - h;
                (d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
            });
            car.fx_last_heading = Some(player.heading);
            let spin = Vec3::Y * (turned / dt)
                + yaw * Vec3::X * car.pitch_vel
                + yaw * Quat::from_rotation_x(car.pitch) * Vec3::Z * car.roll_vel;
            let k = 1.0 - (1.0 - HOVER_FX_SMOOTHING).powf(dt * 60.0);
            let c = &mut *car;
            if let Some(last) = c.fx_last_vel {
                let accel = (vel - last) / dt;
                let spin_accel = (spin - c.fx_last_spin) / dt;
                c.fx_accel += (accel - c.fx_accel) * k;
                c.fx_spin_accel += (spin_accel - c.fx_spin_accel) * k;
            }
            c.fx_last_vel = Some(vel);
            c.fx_last_spin = spin;
        }
        let energon = energon_srgb(player.look.energon());
        let color = [("Color", effect_color(energon, 255))];
        // (Driving.UpdateBoostFx's, every frame.)
        let t = (car.jump_boost_left / JUMP_BOOST_SECS).clamp(0.0, 1.0);
        let boost_rgb = [0, 1, 2].map(|c| {
            let (a, b) = (energon[c] as f32, [255.0, 255.0, 0.0][c]);
            (a + (b - a) * t) as u8
        });
        let boost_color = [("Color", effect_color(boost_rgb, (100.0 + 155.0 * t) as u8))];
        // (Its hover jets this frame: each with where it is, the way its flame points
        // and its socket's scale, for their size below.)
        let mut hover_jets: Vec<(crate::levelfx::FxId, Vec3, Vec3, Vec3)> = Vec::new();
        for (i, e) in vehicle.effects.iter().enumerate() {
            let on = match e.when {
                FxWhen::Hover => floating,
                FxWhen::Boost => boosting,
                FxWhen::Jump => jumped,
                kind => landed == Some(kind),
            };
            if !on {
                continue;
            }
            let Some((bone, _, _)) = bones
                .iter()
                .find(|(_, n, of)| of.0 == jazz && n.as_str() == e.bone)
            else {
                continue;
            };
            let (x, y) = crate::orbital::socket_axes(e.rotation);
            let mut at = bone.transform_point(e.at);
            // A Dinobot's landing on the floor under its socket: the sockets their
            // blueprints name for it are up inside their bodies (Grimlock's LandFX is
            // the tanks' list: its TargetingSocket, his lock-on point in his chest,
            // and two sockets his model hasn't got; Slug's and Snarl's are 1.5 m up
            // their bodies' bone), and the game's landing effects are bursts of
            // sparks and rings of dust off the ground.
            if e.when.landing() && player.kit.legs {
                let feet = transform.translation.y;
                at.y = level
                    .ground(at, 0.0, at.y.max(feet + 0.5), feet - 1.0)
                    .unwrap_or(feet);
            }
            let x = bone.affine().transform_vector3(x).normalize_or(Vec3::X);
            let y = bone.affine().transform_vector3(y).normalize_or(Vec3::Y);
            let params: &[(&str, [u8; 4])] = if e.when == FxWhen::Boost && !plane && !tank {
                &boost_color
            } else {
                &color
            };
            if e.when.once() {
                if let Some(id) = fx.spawn(&e.system, at, x, params) {
                    fx.place_turned(id, at, x, y);
                    fading.push((jazz, id));
                }
                continue;
            }
            // (Kept going while it stays on: one per socket, by its place in the list;
            // one that has played out stays out.)
            let key = format!("{i}:{}", e.system);
            let id = match before.iter().find(|(j, k, _)| *j == jazz && *k == key) {
                Some(&(_, _, id)) => Some(id),
                None => fx.spawn(&e.system, at, x, params),
            };
            if let Some(id) = id {
                if fx.alive(id) {
                    fx.place_turned(id, at, x, y);
                    if e.when == FxWhen::Boost {
                        fx.set_color(id, "Color", params[0].1);
                    }
                }
                playing.push((jazz, key, id));
                if e.when == FxWhen::Hover && !tank {
                    hover_jets.push((id, at, x, e.scale));
                }
            }
        }
        if hover_jets.is_empty() {
            if plane {
                car.jet.thrust = 0.0;
            }
            continue;
        }
        // (Where each is from the middle of them.)
        let centre = hover_jets.iter().map(|j| j.1).sum::<Vec3>() / hover_jets.len() as f32;
        let mut largest = 0.0f32;
        for &(id, at, x, scale) in &hover_jets {
            let push = if plane {
                let body = car.jet.rot.inverse();
                crate::jet::hover_jet_push(&car.jet, body * (at - centre), body * -x)
            } else {
                hover_push(car.fx_accel, car.fx_spin_accel, at - centre, -x)
            };
            largest = largest.max(push);
            fx.set_vector(id, "Size", scale * push);
        }
        // (The hardest push sets a jet's boosters' sound.)
        if plane {
            car.jet.thrust = largest;
        }
    }
    for (jazz, key, id) in before {
        if !playing.iter().any(|(j, k, _)| *j == jazz && *k == key) {
            fx.stop(id);
            fading.push((jazz, id));
        }
    }
    // Those of a vehicle the robot has come out of, or that has been hidden, go.
    let gone = |jazz: Entity| {
        models.iter().any(|(model, v, of)| {
            let shown = *v != Visibility::Hidden;
            of.0 == jazz && shown == robot_models.contains(model)
        })
    };
    fading.retain(|&(jazz, id)| {
        if !fx.alive(id) {
            return false;
        }
        if gone(jazz) {
            fx.kill(id);
            return false;
        }
        true
    });
}

/// How smoothly a car's hover jets follow its acceleration (TnCarForm.Hovering.
/// UpdateFx: VLerp by 0.1 a frame).
const HOVER_FX_SMOOTHING: f32 = 0.1;

/// How hard a car's or truck's hover jet pushes (Hovering.CalculateBoosterFxAmount,
/// 0..1): `at`, where it is from the middle of the jets, and `push`, the way it
/// pushes (against its flame), in the world's axes, against the body's smoothed
/// acceleration less gravity (CalculateThrusterLinearContribution, of 30 m/s^2) and
/// its smoothed angular acceleration (CalculateThrusterAngularContribution, the turn
/// it would give, of 6 rad/s^2).
fn hover_push(accel: Vec3, spin_accel: Vec3, at: Vec3, push: Vec3) -> f32 {
    let linear = ((accel - Vec3::Y * GRAVITY).dot(push) / THRUSTER_ACCEL_LIMIT).clamp(0.0, 1.0);
    let angular = (at.normalize_or_zero().cross(push).dot(spin_accel) / THRUSTER_ANGULAR_LIMIT)
        .clamp(0.0, 1.0);
    (linear + angular).min(1.0)
}

/// A character's energon colour as the game keeps it for its effects (its
/// EnergonColor, TnCharacterApplier.SaveColors: LinearColorToSRGBColor, 2.2 gamma,
/// 8 bits).
pub(crate) fn energon_srgb(linear: Vec3) -> [u8; 3] {
    linear
        .to_array()
        .map(|c| (c.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.999) as u8)
}

/// A Color parameter for an effect's particles: the game's Color, its alpha with it
/// (levelfx.rs takes colour parameters as 255ths, as the demo's effects have matched
/// the game's videos with: the hover jets' and boosters' glow salmon pink, as in
/// them, for an Autobot's red).
pub(crate) fn effect_color(srgb: [u8; 3], alpha: u8) -> [u8; 4] {
    [srgb[0], srgb[1], srgb[2], alpha]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Timing;

    /// Bumblebee's, as tools/transform_fit.py measures them.
    const BUMBLEBEE: Timing = Timing {
        robot_hide: 1.0276,
        robot_show: 0.1666,
        robot_unfold: 1.4667,
        car_show: 0.9089,
        car_fold: 2.5,
        car_hide: 0.3563,
        car_unfold: 1.5,
        fold_lift: 1.298,
        unfold_lift: 0.703,
    };

    /// A car's jump sounds as the game plays them: hovering its hover jump's
    /// (Auto_Jump_Start, Jazz's DRIVE_JUMP_START), boosting on its wheels its boost
    /// jump's (Auto_Boost_Jump, BOOST_JUMP_START).
    #[test]
    fn a_hover_jump_and_a_boost_jump_sound_as_the_game_has_them() {
        assert_eq!(
            jump_sound(true),
            (
                "Auto_Jump_Start",
                "BL_VEH_CAR.VEH_SPORT_01_DRIVE_JUMP_START"
            )
        );
        assert_eq!(
            jump_sound(false),
            (
                "Auto_Boost_Jump",
                "BL_VEH_CAR.VEH_SPORT_01_BOOST_JUMP_START"
            )
        );
    }

    #[test]
    fn the_car_is_drawn_among_the_robots_pieces_as_they_swap() {
        let t = BUMBLEBEE;
        let rest = 1.15;
        // Folding up: the car comes out of the robot up among his pieces and stays
        // there until he is gone; the robot isn't moved.
        for secs in [0.0, t.car_show, t.robot_hide] {
            assert_eq!(
                transform_heights(&t, true, secs, rest, 0.0),
                (t.fold_lift, 0.0)
            );
        }
        // Then it settles, without a jump, to where it floats by the clip's end.
        let mut last = t.fold_lift;
        let mut secs = t.robot_hide;
        while secs < t.car_fold {
            secs += 1.0 / 60.0;
            let (car, robot) = transform_heights(&t, true, secs, rest, 0.0);
            assert_eq!(robot, 0.0);
            assert!((car - last).abs() < 0.05, "{secs}: {last} to {car}");
            assert!(car <= last + 1e-6 && car >= rest - 1e-6);
            last = car;
        }
        assert!((last - rest).abs() < 0.01, "{last}");
        // Unfolding from the car's hover (or its wheels): the robot is drawn where
        // the car was and the car that lift above him all along, both carried to
        // where he stands.
        for from in [0.0, rest] {
            let (car, robot) = transform_heights(&t, false, 0.0, rest, from);
            assert!((car - from).abs() < 1e-6 && (robot - (from - t.unfold_lift)).abs() < 1e-6);
            let mut secs = 0.0;
            while secs < t.robot_unfold {
                secs += 1.0 / 60.0;
                let (car, robot) = transform_heights(&t, false, secs, rest, from);
                assert!((car - robot - t.unfold_lift).abs() < 1e-5);
            }
            let (car, robot) = transform_heights(&t, false, UNFOLD_CARRY_SECS, rest, from);
            assert!((car - t.unfold_lift).abs() < 1e-6 && robot.abs() < 1e-6);
        }
    }

    /// The ground under the springs: a slope up ahead pitches the body nose up by its
    /// angle and a slope up to the right leans it right side up, whatever the
    /// footprint (the wheels', or the hover's off centre); its height at the origin
    /// is the plane's there, a spring finding no ground hanging it lower.
    #[test]
    fn the_body_lies_on_the_plane_under_its_springs() {
        let g = Ground::JAZZ;
        for points in [g.wheels, g.hover_points()] {
            let plane = |p: Vec2, a: f32, b: f32, c: f32| a + b * p.x + c * p.y;
            let up = 15f32.to_radians().tan();
            let fit = fit_ground(&points, &points.map(|p| plane(p, -0.1, 0.0, up)));
            assert!((fit.pitch - 15f32.to_radians()).abs() < 1e-4, "{fit:?}");
            assert!(
                fit.roll.abs() < 1e-4 && (fit.height + 0.1).abs() < 1e-4,
                "{fit:?}"
            );
            let fit = fit_ground(&points, &points.map(|p| plane(p, 0.0, up, 0.0)));
            assert!((fit.roll - 15f32.to_radians()).abs() < 1e-4 && fit.pitch.abs() < 1e-4);
            // (Both: leaned on the pitched body as car.rs turns it, its up the
            // plane's.)
            let fit = fit_ground(&points, &points.map(|p| plane(p, 0.0, 0.3, -0.4)));
            let tilt = Quat::from_rotation_x(fit.pitch) * Quat::from_rotation_z(fit.roll);
            let normal = Vec3::new(-0.3, 1.0, -0.4).normalize();
            assert!((tilt * Vec3::Y).dot(normal) > 0.9999, "{fit:?}");
        }
        // Off a ledge: the front springs find nothing and hang, the nose down.
        let fit = fit_ground(&g.wheels, &[-GROUND_REACH, -GROUND_REACH, 0.0, 0.0]);
        assert!(fit.pitch < -0.5 && fit.pitch >= -MAX_TILT, "{fit:?}");
    }

    /// A map of flat floors: (x from, x to, z from, z to, height).
    fn floors(rects: &[(f32, f32, f32, f32, f32)]) -> Level {
        let tris: Vec<[f32; 9]> = rects
            .iter()
            .flat_map(|&(x0, x1, z0, z1, h)| {
                [
                    [x0, h, z0, x0, h, z1, x1, h, z1],
                    [x0, h, z0, x1, h, z1, x1, h, z0],
                ]
            })
            .collect();
        Level {
            collision: Some(crate::collision::LevelCollision::from_parts(
                &tris,
                &[],
                4.0,
            )),
            ..default()
        }
    }

    /// The body rests on what is under it: a truck on a walkway narrower than its
    /// hover's springs, which reach down past its edges to the floor beside it,
    /// stays up on the walkway where it stands (its springs no lower than its
    /// jets clear: it doesn't sink into it), and a car whose nose is over a floor
    /// A tyre reaches as far under the car's origin as its disc does, however far it
    /// has turned on its axle (its box's corners would reach further at a turn of
    /// 45 degrees, and the car would bob as it drove); tipped 50 degrees, as a car's
    /// wheels are turning flat to hover, its lowest edge reaches its radius across
    /// the up and half its width along the axle's tilt under its middle. Jazz's
    /// car's: 0.7 across and 0.38 thick in its own units, at 0.45 on its socket, its
    /// middle 0.314 m up (just over the ground) and, tipped, 0.2 m.
    #[test]
    fn a_tyre_reaches_under_the_car_as_its_disc_does() {
        use std::f32::consts::FRAC_PI_2;
        let (centre, half) = (Vec3::ZERO, Vec3::new(0.7, 0.38, 0.7));
        // (The car model's units: 2.2 a metre.)
        let units = 2.2;
        let tyre = |tip: f32, spin: f32, up: f32| {
            Affine3A::from_scale_rotation_translation(
                Vec3::splat(0.45 * units),
                Quat::from_rotation_z(-FRAC_PI_2 + tip) * Quat::from_rotation_y(spin),
                Vec3::new(0.75 * units, up * units, -1.1 * units),
            )
        };
        for spin in [0.0, 0.3, std::f32::consts::FRAC_PI_4, 2.0] {
            let upright = tyre_depth(centre, half, &tyre(0.0, spin, 0.314), 1.0 / units);
            assert!((upright - 0.001).abs() < 1e-4, "{spin}: {upright}");
            let tipped = tyre_depth(
                centre,
                half,
                &tyre(50f32.to_radians(), spin, 0.2),
                1.0 / units,
            );
            let (tilt, radius, width) = (50f32.to_radians().sin(), 0.315, 0.171);
            let want = radius * (1.0 - tilt * tilt).sqrt() + width * tilt - 0.2;
            assert!((tipped - want).abs() < 1e-4, "{spin}: {tipped} {want}");
        }
        // One wholly over the ground reaches nowhere under it.
        assert_eq!(
            tyre_depth(centre, half, &tyre(0.0, 0.0, 0.5), 1.0 / units),
            0.0
        );
    }

    /// plate settles with its rigid body's front on it; a truck's box rides higher
    /// over its jets, over the plate.
    #[test]
    fn the_body_rests_on_what_is_under_it() {
        let truck = Ground {
            hover_radius: 1.85,
            hover_centre: Vec2::new(0.0, -0.47),
            ..Ground::JAZZ
        };
        let level = floors(&[
            (-20.0, 20.0, -20.0, 20.0, 0.0),
            (-1.0, 1.0, -20.0, 20.0, 0.25),
        ]);
        let at = Transform::from_xyz(0.0, 0.25, 0.0);
        let fit = ground_under(&level, &at, &truck.hover_points(), 1.75, None, 0.155);
        assert!(fit.height.abs() < 1e-4 && fit.floor.abs() < 1e-4, "{fit:?}");
        assert!(fit.pitch.abs() < 1e-4 && fit.roll.abs() < 1e-4, "{fit:?}");
        // (Its springs alone would have it 25 cm down, its jets in the walkway.)
        let plain = fit_ground(&truck.hover_points(), &[-0.25; 4]);
        assert!((plain.height + 0.25).abs() < 1e-4, "{plain:?}");

        let level = floors(&[
            (-20.0, 20.0, -20.0, 20.0, 0.0),
            (-5.0, 5.0, -3.0, -2.0, 0.1),
        ]);
        let at = Transform::from_xyz(0.0, 0.0, 0.0);
        let jazz = Hull {
            ahead: 2.096,
            behind: 1.730,
            half_width: 1.083,
            bottom: -0.015,
            top: 0.843,
        };
        let points = Ground::JAZZ.hover_points();
        let fit = ground_under(&level, &at, &points, 1.21, Some(&jazz), 0.015);
        assert!(
            (fit.height - 0.1).abs() < 1e-4 && fit.floor.abs() < 1e-4,
            "{fit:?}"
        );
        let fit = ground_under(&level, &at, &points, 1.21, None, 0.015);
        assert!(fit.height.abs() < 1e-4, "{fit:?}");
        let truck_box = Hull {
            bottom: 0.456,
            ..jazz
        };
        let fit = ground_under(&level, &at, &points, 1.21, Some(&truck_box), 0.155);
        assert!(fit.height.abs() < 1e-4, "{fit:?}");
    }

    /// Settled on a slope: on its wheels and its hover it pitches all the way to the
    /// ground's (the hover leaves its pitch free); leaned across one, a hover car
    /// holds most of its lean back (UpdateTurn's 5% a frame against its springs:
    /// about a seventh of the ground's), a tank none (it follows the ground).
    #[test]
    fn hovering_it_pitches_to_the_ground_and_holds_its_lean() {
        let g = Ground::JAZZ;
        let slope = 0.3;
        let settle = |pull: (f32, f32), hold: f32| {
            let (mut x, mut v) = (0.0, 0.0);
            for _ in 0..600 {
                tilt_step(&mut x, &mut v, (slope, 0.0), Some(pull), hold, 1.0 / 60.0);
            }
            x / slope
        };
        assert!((settle(g.wheel_pitch, 0.0) - 1.0).abs() < 1e-3);
        assert!((settle(g.hover_pitch, 0.0) - 1.0).abs() < 1e-3);
        let share = settle(g.hover_roll, g.hover_hold);
        assert!(share > 0.1 && share < 0.2, "{share}");
        assert!((settle(HOVER_PITCH_SPRING, 0.0) - 1.0).abs() < 1e-3);
        // In the air the hover rights it, 5% a frame.
        let (mut x, mut v) = (0.3, 0.0);
        tilt_step(&mut x, &mut v, (0.0, 0.0), None, AIR_HOLD, 1.0 / 60.0);
        assert!((x - 0.3 * 0.95).abs() < 0.01, "{x}");
    }
}
