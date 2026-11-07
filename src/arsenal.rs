//! The Titan's, Scientist's and Destroyer's weapons: what they do beyond the eight
//! guns before them, as all_shared.xxx (their WEP_*_p packages), TransGame.xxx and
//! TransContent's scripts have it. Their weapon data is main.rs's WEAPONS entries
//! (each weapon's Escalation data where the game has one, SurvivalData, else its
//! MultiplayerData: the Energon Harvester's DrainGun_MP_WEPDATA, the Corrosive Slime
//! Cannon's AoeSlow_WEPDATA, the A-4 Pulsar Cannon's GrenadeLauncher_WEPDATA, the
//! data a match would give it; their campaign data, PlayerData, is the campaign's
//! version of the weapon). Their effects are the game's own particle systems
//! (fx/weapons_fx.txt, tools/build_weapon_fx.py), which levelfx.rs runs.
//!
//! * **The X18 Scrapmaker** (TnWeaponHeavyMG, HeavyMG_SV_WEPDATA): its barrels spin
//!   up before it fires. Its fire interval eases from 0.35 s down to 0.065 s over a
//!   second of the trigger held (FireIntervals: an HmValueLerper, which runs back
//!   at the same rate when the trigger is let go: TnWeapon.UpdateFireInterval), and
//!   while it is over 0.15 s (TnWeaponDataExtensionHeavyMG's
//!   MinFireIntervalToStartFiring) it only warms up (TnWeaponHeavyMG.FireAmmunition:
//!   StartCharging, its wind-up sound and WarmUp clip; no round): its first round
//!   leaves about 0.78 s after the pull. Then 27-damage instant hits, full damage
//!   to 50 m and down to a tenth at 100 m, from its two barrels in turn, a
//!   200-round drum and no reserve (it has no MaxReserveAmmoCount: ammo comes from
//!   ammo stations and pickups). Jazz moves at three quarters of his speed while
//!   it fires (FiringGroundSpeedMultiplier), and holds it in the heavy-gun stance
//!   (STANCE_HeavyGunArmShootFromHip, main.rs's ANIM_HEAVY_*).
//! * **The Energon Harvester** (TnWeaponDrainGun, DrainGun_MP_WEPDATA): a shotgun
//!   firing bursts of two shots 0.2 s apart, 0.6 s between bursts, each shot nine
//!   20-damage pellets in a plus and an X (SpreadPattern), full damage to 20 m and
//!   none at 40 m. Every pellet that wounds an enemy gives Jazz 15 health back
//!   (TnWeaponDrainGun.ProcessInstantHit: HealthPerHit), the heal's glow on his gun
//!   (Harvester_PlayerHeal_FX on MeleeSocket, at HitEnemy).
//! * **The E.D.K. TechVolt** (TnWeaponChainBeam, ChainBeam_SV_WEPDATA): a beam out to
//!   27.5 m, doing 120 a second to what it strikes (TnWeaponBeam.ProcessBeamHit:
//!   DamagePerSecond times each round's 0.07 s), that chains to every other enemy
//!   within 14 m of it, inside 55 degrees of its line from the muzzle and in sight of
//!   the beam, for 80 a second each (TnWeaponChainBeam.UpdateChainBeam: its
//!   TnWeaponDataExtensionChainBeam), each chain an arc from the enemy to the
//!   nearest point of the beam (TechvoltEmitter_ExtensionTracer_FX). 50 rounds a
//!   clip (3.5 s of beam), 200 in reserve; Jazz moves at three quarters of his speed
//!   while it fires.
//! * **The Corrosive Slime Cannon** (TnWeaponAoESlow, AoeSlow_WEPDATA, firing
//!   TnProjectileSlow with its Slow_MP_PROJDATA): a round a second, flying at 70 m/s
//!   and falling under the world's gravity (GravityScale 1), going off where it
//!   strikes for 125 to everything within 8 m (full to 4 m, half at the edge: its
//!   RadiusDamageScale), and slowing what it hurts for 3 s (TnDamageTypeSlow's
//!   TnBuffAoeSlow: a robot at 0.75 of its speed, a vehicle at 0.65). Fine aim's
//!   button shows where its round would go instead of fine aiming
//!   (TnWeaponAoESlow.TryAltFirePress: TnSimulateProjectileExtension, the round's
//!   Slime_Trajectory_FX run along a simulated shot).

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use std::collections::HashMap;

use crate::enemies::{Death, Hit, Target};
use crate::levelfx::{FxId, FxSystems};
use crate::sound::Sfx;
use crate::{Colliders, Enemy, Form, GunBone, JazzOf, Level, LocalPlayer, Player, WEAPONS};

/// A bone of a gun's moving parts that a socket rides (Jazz's R_ or L_
/// CloudA_ElbowRobo<n>_XT in robot.glb): on his left arm or right, and `n`.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CloudBone {
    pub left: bool,
    pub n: u8,
}

impl CloudBone {
    /// The bone a robot.glb node name stands for, if it is one.
    pub fn from_name(name: &str) -> Option<CloudBone> {
        let (left, rest) = match name.strip_prefix("L_CloudA_ElbowRobo") {
            Some(r) => (true, r),
            None => (false, name.strip_prefix("R_CloudA_ElbowRobo")?),
        };
        let n = rest.strip_suffix("_XT")?.parse::<u8>().ok()?;
        Some(CloudBone { left, n })
    }
}

/// A socket of a gun's weapon mesh (its SkeletalMeshSocket), for the gun on Jazz's
/// right arm and on his left: the bone it is on (0 his elbow, R_ / L_Arm03_Elbow_XB;
/// else the CloudA bone of that number), where on it (as main.rs's GunSockets, in
/// the bone's space: the game's centimetres over 100, its y and z swapped) and its
/// turn (RelativeRotation: pitch, yaw and roll in Unreal's units). Its RelativeScale
/// is no part of the effects on it, signs and all (levelfx's spawn_on_socket).
#[derive(Clone, Copy)]
pub(crate) struct Socket {
    pub bone: [u8; 2],
    pub at: [Vec3; 2],
    pub rot: [[i32; 3]; 2],
}

/// An effect a gun plays on one of its sockets (TnWeaponEffectPlayFXOnSocket).
/// `restart`: its PlayCondition is PEC_RestartIfAlreadyPlaying (the class's
/// default), so played again while it still plays, the one playing starts over
/// (HmParticleEffectManagerComponent.PlayEffect: one effect per socket and template,
/// StartFromBeginning) rather than another joining it (PEC_PlayAlways).
#[derive(Clone, Copy)]
pub(crate) struct SocketFx {
    pub system: &'static str,
    pub socket: Socket,
    pub restart: bool,
}

/// A beam weapon's damage and chains (TnWeaponDataBeam, TnWeaponDataExtensionChainBeam).
#[derive(Clone, Copy)]
pub(crate) struct Beam {
    /// (Its DamagePerSecond, 120, is main.rs's WeaponStats::damage over the fire
    /// interval: each round does that times the interval.)
    ///
    /// How far an enemy may be from the beam (MaxChainLength), the cone round its
    /// line from the muzzle it must be in (BeamConeAngle, degrees), and what each
    /// chain does a second (ChainDamagePerSecond).
    pub chain_reach: f32,
    pub chain_cone: f32,
    pub chain_dps: f32,
    /// Each chain's arc, from the enemy to the beam (ChainTemplate).
    pub chain_fx: &'static str,
}

/// A light held on the muzzle while the gun fires (a TnWeaponEffectLight from
/// StartFire to StopFire): its colour, Brightness and Radius (metres), and how long
/// it takes to come up and to go down (RampInTime, RampOutTime).
#[derive(Clone, Copy)]
pub(crate) struct HeldLight {
    pub color: [u8; 3],
    pub brightness: f32,
    pub radius: f32,
    pub ramp_in: f32,
    pub ramp_out: f32,
}

/// What a new gun does beyond what main.rs's WeaponStats says (its
/// TnWeaponMesh's effects and its weapon class's script).
pub(crate) struct Gear {
    /// Effects with every shot on its sockets (FireShot: its spent shells, the X18
    /// Scrapmaker's barrel smoke).
    pub on_shot: &'static [SocketFx],
    /// Effects as it reloads (Reload: its steam).
    pub on_reload: &'static [SocketFx],
    /// An effect on Jazz's gun as a round wounds an enemy (HitEnemy: the Energon
    /// Harvester's heal).
    pub on_hit: Option<SocketFx>,
    /// A cue held while it fires (StartFire to StopFire: a looping
    /// TnWeaponEffectSound), fading out over the seconds given (LoopingFadeOutTime;
    /// the demo's 0.1 where the game gives none).
    pub firing_loop: Option<(&'static str, f32)>,
    /// Its wind-up (StartCharging to StartFire).
    pub windup: Option<&'static str>,
    /// Its warm-up's one-off clip (StartCharging: ANIM_*, its length in seconds).
    pub warmup_anim: Option<(usize, f32)>,
    /// A sound with every round that wounds an enemy (HitEnemy), "" for none.
    pub sound_hit: &'static str,
    /// A second sound as the trigger is pulled with no ammo at all (NoAmmoFire: the
    /// Corrosive Slime Cannon's), "" for none.
    pub sound_dry: &'static str,
    /// Health each wounding round gives Jazz back (HealthPerHit), 0 for none.
    pub heal: f32,
    /// A beam weapon's damage and chains.
    pub beam: Option<Beam>,
    /// A light held while it fires.
    pub firing_light: Option<HeldLight>,
    /// Where its rounds would go, shown while the fine-aim button is held (its
    /// projectile mesh's TrajectoryPreviewEffect, run along a simulated shot:
    /// TnSimulateProjectileExtension), or for a launcher of blades while Jazz fine
    /// aims with it (TnWeaponSawblade.ShouldSimulateTrajectory).
    pub trajectory: Option<&'static str>,
    /// A launcher that loads blades while its trigger is held and fires them as it
    /// is let go (the Gear Shredder's).
    pub blades: Option<Blades>,
    /// A homing weapon's lock on the sentry kept under the crosshair (the Thermo
    /// Rocket Cannon's: TnWeaponHoming).
    pub lock: Option<LockOn>,
    /// A launcher whose alt fire sets off its rounds (the A-4 Pulsar Cannon's:
    /// TnWeaponGrenadeLauncherBase.TryAltFirePress).
    pub detonate: Option<Detonate>,
    /// An arcing weapon (the Chaos-Rift Combustor's: TnWeaponTankCannonArcable): its
    /// rounds lobbed at what the crosshair is on, its lob shown while the fine-aim
    /// button is held.
    pub arc: Option<Arc>,
}

/// A homing weapon's lock (TnWeaponHoming, its TnWeaponDataHoming): a sentry kept
/// under the crosshair (jet.rs's pick_target) for `secs` (LockOnTime) is locked on to,
/// and stays so for `hold` seconds after it leaves the crosshair (HoldLockOnTime),
/// `sound` held on meanwhile for the player at that PC (TnWeaponEventStartLockOn to
/// StopLockOn). The lock is looked for only while the weapon is ready (its Active
/// state: UpdateLockOnWhileFiring is off) with a round in the clip (CanLockOn); its
/// shot takes it (ServerSpawnedProjectile: the rocket's target) and drops it
/// (WeaponFiring.BeginState: StopLockOn), as does putting the weapon away. Only
/// enemies in robot form are passed over, unless `robots` (CanLockOnToRobots).
#[derive(Clone, Copy)]
pub(crate) struct LockOn {
    pub secs: f32,
    pub hold: f32,
    pub sound: &'static str,
    pub robots: bool,
}

/// A launcher's setting off of its rounds by its alt fire (AttemptToDetonate): with
/// some out, `sound` (StartFire of fire mode 1), else `sound_none` (NoAmmoFire of fire
/// mode 1); then every one of them goes off where it is (ServerDetonateGrenades).
#[derive(Clone, Copy)]
pub(crate) struct Detonate {
    pub sound: &'static str,
    pub sound_none: &'static str,
}

/// An arcing weapon (TnWeaponTankCannonArcable, its TnTankCannonArcableExtension):
/// each round is lobbed to what it would strike from its muzzle along its aim out to
/// the weapon's Range (or the end of that), in a flight time from `flight_secs.0` to
/// `flight_secs.1` by that distance over the Range (ServerSpawnedProjectile:
/// CalculateTossVelocity). While the fine-aim button is held (TryAltFirePress, as it
/// is ready: _IsAiming, until it is let go or the weapon leaves its Active state) the
/// lob is simulated every frame (SimulateTrajectory: its round's
/// TrajectoryPreviewEffect, Gear::trajectory, along it) and `marker` shows where it
/// would land, turned to the surface there (its _ImpactMarker, Sphere_03_MESH ten
/// times its size in MarkTarget_Depth_MAT, along the struck surfaces' normals averaged
/// over 0.25 s: _NormalHistory).
#[derive(Clone, Copy)]
pub(crate) struct Arc {
    pub flight_secs: (f32, f32),
    pub marker: &'static str,
}

/// A launcher of blades (TnWeaponSawblade, its TnSawBladeExtension): a pull of the
/// trigger (Active.StartFire) starts it charging (the Charging state), a blade
/// loaded at once and one more every `secs_per_blade` the trigger is held
/// (Charging.Tick: SecondsPerAdditionalSawBlade), up to `most` (SetNumberOfBlades'
/// clamp) and no more than the clip holds; letting go (Charging.StopFire) fires them
/// all, a round each (FireAmmunition: GetNumShotsToFire), costing as many rounds
/// (ShotCost by fire mode: 1, 2, 3). Blade n is fire mode n - 1 (ConvertBladesToFireMode):
/// its round leaves that mode's muzzle socket (GetMuzzleSocketName: main.rs's
/// GunSockets::muzzles in mode order) turned `yaw` off the aim (AddSpread:
/// DesignPerBlade's Spread, in Unreal's units), the last loaded first
/// (SingleFireAction takes the fire mode down a step after each).
#[derive(Clone, Copy)]
pub(crate) struct Blades {
    pub secs_per_blade: f32,
    pub most: u8,
    pub yaw: [i32; 3],
    /// Each blade's effect while it is loaded (SawBladeEffect, attached to its fire
    /// mode's muzzle socket by AddBlades and taken off, gone at once, by
    /// RemoveBlades), on `sockets` (by fire mode).
    pub effect: &'static str,
    pub sockets: [Socket; 3],
    /// Its charge's loop, held from the first blade loaded to the shot (StartCharging
    /// to StopCharging: its fade), a blade's sound as it loads (AmmoLoaded, by the
    /// fire mode reached) and the shot's (StartFire, by the shot's fire mode).
    pub sound_loop: (&'static str, f32),
    pub sound_loaded: [&'static str; 3],
    pub sound_shot: [&'static str; 3],
    /// The shot's one-off clip (FireShot's TnWeaponEffectPlayAnimationPerChargeLevel,
    /// by fire mode, its last for the rest: Fire_WEP for one blade, ChargedFire_WEP
    /// for more, each with the arm's recoil), ANIM_* and its length in seconds.
    pub shot_anims: [(usize, f32); 2],
}

impl Blades {
    /// The blades loaded after `secs` of charging with `clip` rounds in the clip
    /// (Charging.Tick: 1 + secs / SecondsPerAdditionalSawBlade, at most `most` and the
    /// clip).
    pub fn loaded(&self, secs: f32, clip: u32) -> u8 {
        let n = 1 + (secs / self.secs_per_blade.max(1e-3)).max(0.0) as u32;
        n.min(self.most as u32).min(clip) as u8
    }
}

/// A socket on the elbow (bone 0), the same spot as `at` for both hands, mirrored:
/// the left-handed gun's is the right's with every coordinate negated, as the game's
/// mirrored meshes have most of theirs.
const fn elbow(right: Vec3, left: Vec3, rot: [[i32; 3]; 2]) -> Socket {
    Socket {
        bone: [0, 0],
        at: [right, left],
        rot,
    }
}

/// Facing along the barrel on each arm (the right gun's sockets turned round:
/// yaw 32768).
const ALONG_BARREL: [[i32; 3]; 2] = [[0, 32768, 0], [0, 0, 0]];

/// A muzzle flash `system` on a barrel's muzzle socket (`at`: main.rs's
/// GunSockets::muzzles entry, on the elbow), facing along the barrel; each barrel's
/// starts over with its next shot (TnWeaponEffectPlayMuzzleFX, a
/// TnWeaponEffectPlayFXOnSocket with its default PlayCondition). It plays at its own
/// size, whatever the socket's scale (the X18 Scrapmaker's are a tenth): levelfx's
/// spawn_on_socket.
pub(crate) fn barrel_fx(system: &'static str, at: [Vec3; 2]) -> SocketFx {
    SocketFx {
        system,
        socket: Socket {
            bone: [0, 0],
            at,
            rot: ALONG_BARREL,
        },
        restart: true,
    }
}

// ---------------------------------------------------------------------------
// The X18 Scrapmaker (WEP_HeavyMG_p)
// ---------------------------------------------------------------------------

/// Its ShellEject socket (RB_HeavyMGR_SKEL's and RB_HeavyMGL_SKEL's).
const SCRAPMAKER_SHELL: Socket = elbow(
    Vec3::new(-0.9695, -0.4064, -0.5752),
    Vec3::new(0.9695, 0.4064, 0.5752),
    [[0, -16384, 0], [0, 16384, 0]],
);

/// HeavyMG_WEPMESH: with every shot its spent shell (Shell4_FX on ShellEject) and
/// the smoke off its spinning barrels (X18_Scrapmaker_Idle_FX on its IdleFX socket,
/// on a bone of the barrels: R_CloudA_ElbowRobo14_XT on the right-handed gun,
/// L_CloudA_ElbowRobo13_XT on the left); held from StartFire to StopFire its
/// shooting loop, and from StartCharging to StartFire its wind-up.
pub(crate) const SCRAPMAKER: Gear = Gear {
    on_shot: &[
        SocketFx {
            system: "Shell4_FX",
            socket: SCRAPMAKER_SHELL,
            restart: false,
        },
        SocketFx {
            system: "X18_Scrapmaker_Idle_FX",
            socket: Socket {
                bone: [14, 13],
                at: [
                    Vec3::new(0.9201, -0.8289, 0.7783),
                    Vec3::new(-0.4816, 1.0087, -0.7983),
                ],
                rot: [[-8996, 5589, 28377], [8589, -28114, -29295]],
            },
            restart: false,
        },
    ],
    on_reload: &[],
    on_hit: None,
    firing_loop: Some(("BL_WPN_GUN_PULSE_RIFLE.SHOOT_LP", HELD_FADE)),
    windup: Some("BL_WPN_GUN_PULSE_RIFLE.SHOOT_WINDUP"),
    warmup_anim: Some((crate::ANIM_HEAVYMG_WARMUP, 44.0 / 30.0)),
    sound_hit: "",
    sound_dry: "",
    heal: 0.0,
    beam: None,
    firing_light: None,
    trajectory: None,
    blades: None,
    lock: None,
    detonate: None,
    arc: None,
};

// ---------------------------------------------------------------------------
// The Energon Harvester (WEP_DrainGun_p, its mesh WEP_EMPShotgun_p's)
// ---------------------------------------------------------------------------

/// Its ShellEject socket (RB_EmpShotgunR_SKEL's and RB_EmpShotgunL_SKEL's).
const HARVESTER_SHELL: Socket = elbow(
    Vec3::new(-1.32, 0.0, 0.0),
    Vec3::new(1.32, 0.0, 0.0),
    [[0, -16384, 0], [0, 16384, 0]],
);

/// DrainGun_WEPMESH: the heal's crosses on MeleeSocket as a shot's pellets wound an
/// enemy (HitEnemy, and Kill: one effect, starting over with each shot that heals),
/// and as it reloads its steam on ShellEject (ReloadSteam_Harvester_FX).
pub(crate) const HARVESTER: Gear = Gear {
    on_shot: &[],
    on_reload: &[SocketFx {
        system: "ReloadSteam_Harvester_FX",
        socket: HARVESTER_SHELL,
        restart: false,
    }],
    on_hit: Some(SocketFx {
        system: "Harvester_PlayerHeal_FX",
        socket: elbow(
            Vec3::new(-1.6975, 0.0806, -0.1295),
            Vec3::new(1.4826, -0.0806, 0.1295),
            [[0, 0, 0], [0, 0, 0]],
        ),
        restart: true,
    }),
    firing_loop: None,
    windup: None,
    warmup_anim: None,
    sound_hit: "",
    sound_dry: "",
    heal: 15.0,
    beam: None,
    firing_light: None,
    trajectory: None,
    blades: None,
    lock: None,
    detonate: None,
    arc: None,
};

// ---------------------------------------------------------------------------
// The E.D.K. TechVolt (WEP_ChainBeam_p, its mesh WEP_RepairBeam_p's)
// ---------------------------------------------------------------------------

/// ChainBeam_WEPMESH: its steam as it reloads (ReloadSteam_Tech_FX on ReloadFX);
/// held from StartFire to StopFire its beam's loop (fading out over a quarter of a
/// second) and its light (Brightness 30, Radius 500, colour (96, 131, 245), coming
/// up and going down over half a second); a crackle with every round that strikes
/// an enemy (HIT_ENEMY).
pub(crate) const TECHVOLT: Gear = Gear {
    on_shot: &[],
    on_reload: &[SocketFx {
        system: "ReloadSteam_Tech_FX",
        socket: elbow(
            Vec3::new(-1.1169, 0.0, -1.2412),
            Vec3::new(1.1169, 0.0, 1.1882),
            ALONG_BARREL,
        ),
        restart: false,
    }],
    on_hit: None,
    firing_loop: Some(("BL_WPN_GUN_TECH_VOLT.SHOOT_LP", 0.25)),
    windup: None,
    warmup_anim: None,
    sound_hit: "BL_WPN_GUN_TECH_VOLT.HIT_ENEMY",
    sound_dry: "",
    heal: 0.0,
    beam: Some(Beam {
        chain_reach: 14.0,
        chain_cone: 55.0,
        chain_dps: 80.0,
        chain_fx: "TechvoltEmitter_ExtensionTracer_FX",
    }),
    firing_light: Some(HeldLight {
        color: [96, 131, 245],
        brightness: 30.0,
        radius: 5.0,
        ramp_in: 0.5,
        ramp_out: 0.5,
    }),
    trajectory: None,
    blades: None,
    lock: None,
    detonate: None,
    arc: None,
};

// ---------------------------------------------------------------------------
// The Corrosive Slime Cannon (WEP_AoeSlow_p, its mesh RB_RustCannon*)
// ---------------------------------------------------------------------------

/// AoeSlow_WEPMESH: its steam as it reloads (ReloadSteam_Slime_FX on ReloadFX); its
/// round's path while fine aim's button is held (AoeSlow_PROJMESH's
/// TrajectoryPreviewEffect, Slime_Trajectory_FX).
pub(crate) const SLIME_CANNON: Gear = Gear {
    on_shot: &[],
    on_reload: &[SocketFx {
        system: "ReloadSteam_Slime_FX",
        // (Its ReloadFX socket is at 0.7 on both guns, which the steam doesn't take.)
        socket: elbow(
            Vec3::new(-0.9979, 0.0, -0.1006),
            Vec3::new(1.0463, 0.0, -0.1036),
            [[16384, 0, 0], [-16384, 0, 0]],
        ),
        restart: false,
    }],
    on_hit: None,
    firing_loop: None,
    windup: None,
    warmup_anim: None,
    sound_hit: "",
    sound_dry: "BL_WPN_GUN_GRENADE_LAUNCHER.DETONATE_REMOTE_DRYFIRE",
    heal: 0.0,
    beam: None,
    firing_light: None,
    trajectory: Some("Slime_Trajectory_FX"),
    blades: None,
    lock: None,
    detonate: None,
    arc: None,
};

// ---------------------------------------------------------------------------
// The Gear Shredder (WEP_Sawblade_p)
// ---------------------------------------------------------------------------

/// Its muzzle sockets by fire mode (SawBlade_WEPMESH's MuzzleSockets: mode 0
/// MuzzleFlashCenter, 1 MuzzleFlashRight, 2 MuzzleFlashLeft), on RB_SawbladeR_SKEL's
/// and RB_SawbladeL_SKEL's elbows (the left-handed gun's its own spots), facing along
/// the barrel; main.rs's SHREDDER_SOCKETS has the same places.
pub(crate) const SHREDDER_MUZZLES: [Socket; 3] = [
    Socket {
        bone: [0, 0],
        at: [Vec3::new(-2.00, 0.70, 0.0), Vec3::new(2.00, 0.15, 0.0)],
        rot: ALONG_BARREL,
    },
    Socket {
        bone: [0, 0],
        at: [Vec3::new(-2.75, 0.65, -0.55), Vec3::new(2.75, 0.20, 0.55)],
        rot: ALONG_BARREL,
    },
    Socket {
        bone: [0, 0],
        at: [Vec3::new(-2.75, 0.65, 0.55), Vec3::new(2.75, 0.20, -0.55)],
        rot: ALONG_BARREL,
    },
];

/// SawBlade_WEPMESH: as it reloads its steam on ShellSocket (ReloadSteam_Med_FX) and
/// on ReserveAmmo, a socket on a bone of its moving parts (R_ and
/// L_CloudA_ElbowRobo11_XT: ReloadSteam_Med02_FX), each a new one with every reload
/// (PEC_PlayAlways); its blades while it charges (TnSawBladeExtension_13904:
/// SawBlade_Weapon_FX, a blade a fire mode, 0.65 s apart, three at most, the side
/// ones 5 degrees out, right and left: DesignPerBlade's Spread, yaw 910 and -910),
/// its charge's loop (CHARGE_LP from StartCharging to StopCharging, fading in and out
/// over 0.1 s), a blade's sound as it loads (AmmoLoaded: CHARGE_MECH_01 for the first
/// and second, CHARGE_MECH_02 for the third), the shot's by its blades (StartFire:
/// SHOOT, SHOOT_CHRG_02_SAWS, SHOOT_CHRG_03_SAWS) and its clip (FireShot: Fire_WEP
/// for one blade, ChargedFire_WEP for more); the path its blades would take while
/// Jazz fine aims (SawBlade_PROJMESH's TrajectoryPreviewEffect,
/// SawBlade_Trajectory_FX: TnWeaponSawblade.UpdateTrajectorySimulation).
pub(crate) const GEAR_SHREDDER: Gear = Gear {
    on_shot: &[],
    on_reload: &[
        SocketFx {
            system: "ReloadSteam_Med_FX",
            socket: elbow(
                Vec3::new(-0.7306, 0.0, -0.1005),
                Vec3::new(0.7306, 0.0, 0.1005),
                [[0, 32768, 32768], [0, 0, 32768]],
            ),
            restart: false,
        },
        SocketFx {
            system: "ReloadSteam_Med02_FX",
            socket: Socket {
                bone: [11, 11],
                at: [
                    Vec3::new(-0.0114, 0.6879, -0.0725),
                    Vec3::new(0.0067, -0.6968, 0.0725),
                ],
                rot: [[-6967, -32768, 19570], [-6967, -32768, 19570]],
            },
            restart: false,
        },
    ],
    on_hit: None,
    firing_loop: None,
    windup: None,
    warmup_anim: None,
    sound_hit: "",
    sound_dry: "",
    heal: 0.0,
    beam: None,
    firing_light: None,
    trajectory: Some("SawBlade_Trajectory_FX"),
    blades: Some(Blades {
        secs_per_blade: 0.65,
        most: 3,
        yaw: [0, 910, -910],
        effect: "SawBlade_Weapon_FX",
        sockets: SHREDDER_MUZZLES,
        sound_loop: ("BL_WPN_GUN_SAW_BLADE.CHARGE_LP", 0.1),
        sound_loaded: [
            "BL_WPN_GUN_SAW_BLADE.CHARGE_MECH_01",
            "BL_WPN_GUN_SAW_BLADE.CHARGE_MECH_01",
            "BL_WPN_GUN_SAW_BLADE.CHARGE_MECH_02",
        ],
        sound_shot: [
            "BL_WPN_GUN_SAW_BLADE.SHOOT",
            "BL_WPN_GUN_SAW_BLADE.SHOOT_CHRG_02_SAWS",
            "BL_WPN_GUN_SAW_BLADE.SHOOT_CHRG_03_SAWS",
        ],
        shot_anims: [
            (crate::ANIM_NEW_GUNS + 24, 17.0 / 30.0),
            (crate::ANIM_SHREDDER_CHARGED_FIRE, 29.0 / 30.0),
        ],
    }),
    lock: None,
    detonate: None,
    arc: None,
};

// ---------------------------------------------------------------------------
// The Thermo Rocket Cannon (WEP_RocketLauncher_p)
// ---------------------------------------------------------------------------

/// RocketLauncher_WEPMESH: with every shot the flame out of its back
/// (RocketLauncherExhaust_FX on MuzzleExhaust, starting over with each:
/// TnWeaponEffectPlayFXOnSocket's default PlayCondition); as it reloads the dark smoke
/// out of its exhaust (ReloadSteam_DarkSmke_FX on ReloadExhaust) and steam
/// (ReloadSteam_FX on ReloadSocket), each a new one with every reload (PEC_PlayAlways);
/// its lock (RocketLauncher_SV_WEPDATA: LockOnTime 0.75 s, HoldLockOnTime 2 s; the
/// HUD's lock tone, HUD_TARGET_LOCK).
pub(crate) const THERMO_ROCKET: Gear = Gear {
    on_shot: &[SocketFx {
        system: "RocketLauncherExhaust_FX",
        // (The right-handed gun's MuzzleExhaust, ReloadExhaust and ReloadSocket are
        // at 0.7, the left's at 1; the effects play at their own size.)
        socket: Socket {
            bone: [0, 0],
            at: [
                Vec3::new(0.8041, -0.1031, -0.1046),
                Vec3::new(-0.8052, 0.1910, 0.0635),
            ],
            rot: [[0, 1512, 32768], [-1504, -31997, -32768]],
        },
        restart: true,
    }],
    on_reload: &[
        SocketFx {
            system: "ReloadSteam_DarkSmke_FX",
            socket: Socket {
                bone: [0, 0],
                at: [
                    Vec3::new(1.0580, -0.1031, -0.0676),
                    Vec3::new(-1.1144, 0.1460, 0.0407),
                ],
                rot: [[0, 1512, 32768], [-1504, -31997, -32768]],
            },
            restart: false,
        },
        SocketFx {
            system: "ReloadSteam_FX",
            socket: Socket {
                bone: [0, 0],
                at: [
                    Vec3::new(-1.0182, 0.0, -0.2851),
                    Vec3::new(0.9550, 0.2013, 0.4698),
                ],
                rot: [[32768, 16384, 0], [-32768, -16384, 0]],
            },
            restart: false,
        },
    ],
    on_hit: None,
    firing_loop: None,
    windup: None,
    warmup_anim: None,
    sound_hit: "",
    sound_dry: "",
    heal: 0.0,
    beam: None,
    firing_light: None,
    trajectory: None,
    blades: None,
    lock: Some(LockOn {
        secs: 0.75,
        hold: 2.0,
        sound: "BL_HUD_INTERFACE.HUD_TARGET_LOCK",
        // (Off in its data: it locks on to the jets, drones and swarmers.)
        robots: false,
    }),
    detonate: None,
    arc: None,
};

// ---------------------------------------------------------------------------
// The A-4 Pulsar Cannon (WEP_GrenadeLauncher_p)
// ---------------------------------------------------------------------------

/// GrenadeLauncher_WEPMESH: as it reloads its steam on ReloadSocket
/// (ReloadSteam_Med02_FX and ReloadSteam_Med_FX, each a new one with every reload);
/// its alt fire setting off its rounds (fire mode 1's StartFire,
/// PULSAR_CANNON_REMOTE_DETONATE, with some out; its NoAmmoFire,
/// DETONATE_REMOTE_DRYFIRE, with none).
pub(crate) const PULSAR_CANNON: Gear = Gear {
    on_shot: &[],
    on_reload: &[
        SocketFx {
            system: "ReloadSteam_Med02_FX",
            socket: PULSAR_RELOAD,
            restart: false,
        },
        SocketFx {
            system: "ReloadSteam_Med_FX",
            socket: PULSAR_RELOAD,
            restart: false,
        },
    ],
    on_hit: None,
    firing_loop: None,
    windup: None,
    warmup_anim: None,
    sound_hit: "",
    sound_dry: "",
    heal: 0.0,
    beam: None,
    firing_light: None,
    trajectory: None,
    blades: None,
    lock: None,
    detonate: Some(Detonate {
        sound: "BL_WPN_GUN_PLASMA_CANNON.PULSAR_CANNON_REMOTE_DETONATE",
        sound_none: "BL_WPN_GUN_GRENADE_LAUNCHER.DETONATE_REMOTE_DRYFIRE",
    }),
    arc: None,
};

/// Its ReloadSocket (RB_GrenadeLauncherR_SKEL's and RB_GrenadeLauncherL_SKEL's).
const PULSAR_RELOAD: Socket = Socket {
    bone: [0, 0],
    at: [
        Vec3::new(0.3454, -0.5433, -0.6077),
        Vec3::new(-0.3595, 0.5630, 0.5473),
    ],
    rot: [[32768, 32768, 32768], [0, 32768, 32768]],
};

// ---------------------------------------------------------------------------
// The Chaos-Rift Combustor (WEP_ClusterCannon_p, its mesh WEP_GrenadeArm_p's)
// ---------------------------------------------------------------------------

/// ClusterCannon_WEPMESH: with every shot its spent shell (Lrg_Shell1_FX on
/// ShellEject, a new one each time: PEC_PlayAlways); as it reloads its steam
/// (ReloadSteam_FX on ReloadFX); its lob (ClusterCannon_SV_WEPDATA's
/// TnTankCannonArcableExtension_12849: FlightTime 0.1 to 1.1 s) shown along
/// ClusterCannon_PROJMESH's TrajectoryPreviewEffect, Cluster_Trajectory_FX, with its
/// impact marker (tools/build_weapon_fx.py's ClusterCannon_Marker_FX).
pub(crate) const COMBUSTOR: Gear = Gear {
    on_shot: &[SocketFx {
        system: "Lrg_Shell1_FX",
        socket: elbow(
            Vec3::new(-0.2238, -0.5433, -0.7600),
            Vec3::new(0.4522, 0.4941, 0.8949),
            [[0, -16384, 0], [0, 16384, 0]],
        ),
        restart: false,
    }],
    on_reload: &[SocketFx {
        system: "ReloadSteam_FX",
        socket: elbow(
            Vec3::new(-1.0369, -0.2686, -0.5763),
            Vec3::new(1.1020, 0.2684, 0.6487),
            ALONG_BARREL,
        ),
        restart: false,
    }],
    on_hit: None,
    firing_loop: None,
    windup: None,
    warmup_anim: None,
    sound_hit: "",
    sound_dry: "",
    heal: 0.0,
    beam: None,
    firing_light: None,
    trajectory: Some("Cluster_Trajectory_FX"),
    blades: None,
    lock: None,
    detonate: None,
    arc: Some(Arc {
        flight_secs: (0.1, 1.1),
        marker: "ClusterCannon_Marker_FX",
    }),
};

/// The fade of a held cue the game gives no LoopingFadeOutTime (the demo's usual).
const HELD_FADE: f32 = 0.1;

/// The effects' energon colour: a team's own effects are cyan in a match (their
/// EnergonColor, as the other guns'), and the weapon's and its round's meshes give
/// their effects the same in their Color parameter (their Parameters'
/// TnParticleSystemParameterEnergonColor, whose ParameterName is Color: the
/// Scrapmaker's shells and barrel smoke, the steam, the slime's path).
const ENERGON: &[(&str, [u8; 4])] = &[
    ("EnergonColor", [0, 255, 255, 255]),
    ("Color", [0, 255, 255, 255]),
];

/// A Jazz's new-gun state (main.rs's Player::gear): what the game's TnWeapon and
/// its subclasses keep between frames.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GearState {
    /// The weapon (WEAPONS) it is for: it starts over as he changes weapons.
    pub weapon: usize,
    /// The fire interval's ramp (WeaponStats::ramp): seconds into it.
    pub ramp: f32,
    /// It has fired since the trigger was pulled (StartFire): its StopFire sound and
    /// clip play as the firing stops.
    pub fired: bool,
    /// Warming up (WarmingUp: from StartCharging to the first round, or to
    /// StopCharging as the trigger is let go).
    pub warming: bool,
    /// Its wind-up's cue: seconds it may still play (it plays on after the trigger
    /// is let go, to its end: its StopEvent is StartFire alone).
    pub windup: f32,
    /// A beam's end: where its latest round struck, or the end of its range.
    pub beam_end: Option<Vec3>,
    /// The fine-aim button held with a gun that does something else with it
    /// (WeaponStats::alt).
    pub alt: bool,
    /// Its reload has just started (main.rs's animate_jazz, as the reload's clip
    /// starts): its steam (Gear::on_reload) is to play.
    pub reloading: bool,
    /// A launcher of blades' blades loaded (Gear::blades: TnWeaponSawblade's
    /// _NumberOfBlades, 0 when it isn't charging), and the seconds it has charged
    /// (since _StartChargeTimestamp).
    pub blades: u8,
    pub charge: f32,
    /// The fine-aim button held last frame (a press is its going down: the
    /// weapon's TryAltFirePress).
    pub alt_was: bool,
    /// Aiming an arcing weapon's lob (Gear::arc: TnWeaponTankCannonArcable's _IsAiming,
    /// from the fine-aim button's press while it is ready to its release or the
    /// weapon's leaving its Active state).
    pub arc: bool,
    /// His sticky rounds out (riot.rs: the grenade launcher's ActiveGrenadeCount), and
    /// whether a round of his that bursts into clusters is in the air (its
    /// _IsProjectileInFlight).
    pub grenades: u8,
    pub in_flight: bool,
}

/// How long a wind-up's cue is held after its StartCharging at most (its recordings
/// are one-offs, GUN_HV_MG_WINDUP the longest: after that the hold plays nothing).
pub(crate) const WINDUP_HOLD_SECS: f32 = 3.0;

pub struct ArsenalPlugin;

impl Plugin for ArsenalPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(crate::menu::AppState::Game), clear_gun_fx)
            .add_systems(
                Update,
                (
                    reload_fx,
                    follow_gun_fx,
                    gear_loops,
                    blade_fx,
                    beams,
                    trajectories,
                )
                    .after(crate::fire)
                    .before(crate::hud::HudSystems)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

/// An effect on a socket of a Jazz's gun (TnWeaponEffectPlayFXOnSocket, attached to
/// it): started at the socket where it is as follow_gun_fx first sees it, then kept
/// there while the gun is in his hand (when he puts it away, changes form or
/// switches hands, it plays out where it is).
#[derive(Component)]
pub(crate) struct GunFx {
    system: &'static str,
    jazz: Entity,
    weapon: usize,
    socket: Socket,
    restart: bool,
    fx: Option<FxId>,
    /// Drawn at his robot's size (Robot::effect_scale), as the game draws an effect
    /// on a gun: the gun is drawn at its robot's size, the effect attached to it at
    /// its own (TnWeaponEffectPlayFXOnSocket.CalculateRelativeScale undoes only a
    /// scale over 1), so 0.6 of it on the Infiltrator's gun, 0.9 on the Titan's.
    /// (The muzzle flashes main.rs's muzzle_system plays.)
    robot_sized: bool,
}

/// Plays `system` on `socket` of the gun (WEAPONS) in `jazz`'s hand.
pub(crate) fn play_on_socket(commands: &mut Commands, jazz: Entity, weapon: usize, fx: &SocketFx) {
    commands.spawn(GunFx {
        system: fx.system,
        jazz,
        weapon,
        socket: fx.socket,
        restart: fx.restart,
        fx: None,
        robot_sized: false,
    });
}

/// As `play_on_socket`, at his robot's size (GunFx::robot_sized): a gun's muzzle
/// flash that is the game's own (main.rs's muzzle_system).
pub(crate) fn play_flash_on_socket(
    commands: &mut Commands,
    jazz: Entity,
    weapon: usize,
    fx: &SocketFx,
) {
    commands.spawn(GunFx {
        system: fx.system,
        jazz,
        weapon,
        socket: fx.socket,
        restart: fx.restart,
        fx: None,
        robot_sized: true,
    });
}

/// The bones the guns' sockets ride: Jazz's elbows (main.rs's GunBone) and the gun
/// parts' bones (CloudBone).
pub(crate) type Elbows<'w, 's> =
    Query<'w, 's, (&'static GlobalTransform, &'static GunBone, &'static JazzOf)>;
pub(crate) type CloudBones<'w, 's> = Query<
    'w,
    's,
    (
        &'static GlobalTransform,
        &'static CloudBone,
        &'static JazzOf,
    ),
>;

/// Where `socket` of the gun on `jazz`'s left arm (or right) is now: its place and
/// its X and Y axes.
pub(crate) fn socket_frame(
    socket: &Socket,
    left: bool,
    jazz: Entity,
    elbows: &Elbows,
    clouds: &CloudBones,
) -> Option<(Vec3, Vec3, Vec3)> {
    let side = left as usize;
    let bone = match socket.bone[side] {
        0 => elbows
            .iter()
            .find(|(_, b, of)| b.0 == left && of.0 == jazz)
            .map(|(t, _, _)| *t),
        n => clouds
            .iter()
            .find(|(_, b, of)| b.left == left && b.n == n && of.0 == jazz)
            .map(|(t, _, _)| *t),
    }?;
    let (x, y) = crate::orbital::socket_axes(socket.rot[side]);
    let at = bone.transform_point(socket.at[side]);
    let x = bone.affine().transform_vector3(x).normalize_or(Vec3::X);
    let y = bone.affine().transform_vector3(y).normalize_or(Vec3::Y);
    Some((at, x, y))
}

/// Keeps the gun effects on their sockets.
fn follow_gun_fx(
    mut commands: Commands,
    mut fx: ResMut<FxSystems>,
    players: Query<&Player>,
    elbows: Elbows,
    clouds: CloudBones,
    mut effects: Query<(Entity, &mut GunFx)>,
) {
    // One playing at a time of those that start over (SocketFx::restart): a new
    // one ends the one before it on the same socket of the same gun.
    let fresh: Vec<(Entity, Entity, &'static str, Vec3)> = effects
        .iter()
        .filter(|(_, g)| g.restart && g.fx.is_none())
        .map(|(e, g)| (e, g.jazz, g.system, g.socket.at[0]))
        .collect();
    let mut replaced = Vec::new();
    if !fresh.is_empty() {
        for (entity, mut g) in &mut effects {
            let older = fresh.iter().any(|&(e, jazz, system, at)| {
                e != entity && g.jazz == jazz && g.system == system && g.socket.at[0] == at
            });
            if older && let Some(id) = g.fx.take() {
                fx.kill(id);
                commands.entity(entity).despawn();
                replaced.push(entity);
            }
        }
    }
    for (entity, mut g) in &mut effects {
        if replaced.contains(&entity) {
            continue;
        }
        let held = players
            .get(g.jazz)
            .ok()
            .filter(|p| p.form == Form::Robot && p.weapon == g.weapon && p.hand_switch.is_none());
        let frame =
            held.and_then(|p| socket_frame(&g.socket, p.left_handed, g.jazz, &elbows, &clouds));
        match (g.fx, frame) {
            (None, Some((at, x, y))) => {
                g.fx = match held.filter(|_| g.robot_sized) {
                    Some(p) => {
                        fx.spawn_scaled(g.system, at, x, p.kit.robot.effect_scale(), ENERGON)
                    }
                    None => fx.spawn_on_socket(g.system, at, x, ENERGON),
                };
                match g.fx {
                    Some(id) => fx.place_turned(id, at, x, y),
                    None => commands.entity(entity).despawn(),
                }
            }
            (Some(id), Some((at, x, y))) if fx.alive(id) => fx.place_turned(id, at, x, y),
            // (Never started, over, or let go: it plays on where it is.)
            _ => commands.entity(entity).despawn(),
        }
    }
}

/// A new gun's reload, for every Jazz (the others' as their reload clips start
/// here): its effects on its sockets (Gear::on_reload: its steam).
fn reload_fx(mut commands: Commands, mut players: Query<(Entity, &mut Player)>) {
    for (jazz, mut player) in &mut players {
        if !std::mem::take(&mut player.gear.reloading) {
            continue;
        }
        if let Some(gear) = WEAPONS.get(player.weapon).and_then(|w| w.gear) {
            for fx in gear.on_reload {
                play_on_socket(&mut commands, jazz, player.weapon, fx);
            }
        }
    }
}

/// A new match: no gun effects left over from the last.
fn clear_gun_fx(mut commands: Commands, effects: Query<Entity, With<GunFx>>) {
    for entity in &effects {
        commands.entity(entity).despawn();
    }
}

/// The new guns' held cues, for every Jazz (the others' as their PCs report them):
/// a gun's firing loop while it fires (from its first round, StartFire, until the
/// firing stops), and its wind-up from StartCharging until the first round (or its
/// end), heard where he is (the local player's by the camera).
fn gear_loops(
    time: Res<Time>,
    mut players: Query<(Entity, &Transform, &mut Player, Has<LocalPlayer>)>,
    mut sfx: ResMut<Sfx>,
) {
    let dt = time.delta_secs();
    for (owner, transform, mut player, local) in &mut players {
        let gear = WEAPONS.get(player.weapon).and_then(|w| w.gear);
        let alive = player.dead_left <= 0.0 && !player.dying && player.form == Form::Robot;
        let at = crate::voice_at(local, transform.translation);
        let key = owner.to_bits();
        player.gear.windup = (player.gear.windup - dt).max(0.0);
        let Some(gear) = gear.filter(|_| alive) else {
            player.gear.windup = 0.0;
            continue;
        };
        if let Some((cue, fade)) = gear.firing_loop
            && player.firing
            && player.gear.fired
        {
            sfx.hold_fading(key, cue, at, fade);
        }
        if let Some(cue) = gear.windup
            && player.gear.windup > 0.0
            && !player.gear.fired
        {
            sfx.hold_fading(key, cue, at, HELD_FADE);
        }
        if let Some(b) = gear.blades
            && player.gear.blades > 0
        {
            sfx.hold_fading(key, b.sound_loop.0, at, b.sound_loop.1);
        }
    }
}

/// The blades loaded onto a launcher of blades, for every Jazz (the others' as
/// their PCs report them): each one's effect on its fire mode's muzzle socket
/// while it is loaded (AddBlades: attached to the socket), gone at once as the
/// blades go (RemoveBlades: detached), or the gun leaves his hand.
fn blade_fx(
    mut fx: ResMut<FxSystems>,
    players: Query<(Entity, &Player)>,
    elbows: Elbows,
    clouds: CloudBones,
    mut shown: Local<HashMap<Entity, [Option<FxId>; 3]>>,
) {
    for (jazz, player) in &players {
        let blades = WEAPONS
            .get(player.weapon)
            .and_then(|w| w.gear)
            .and_then(|g| g.blades)
            .filter(|_| player.form == Form::Robot && player.hand_switch.is_none());
        let ids = shown.entry(jazz).or_default();
        for (k, id) in ids.iter_mut().enumerate() {
            let frame = blades
                .filter(|_| k < player.gear.blades as usize)
                .and_then(|b| {
                    socket_frame(&b.sockets[k], player.left_handed, jazz, &elbows, &clouds)
                        .map(|f| (b.effect, f))
                });
            match (frame, *id) {
                (Some((effect, (at, x, y))), None) => {
                    *id = fx.spawn_on_socket(effect, at, x, ENERGON);
                    if let Some(i) = *id {
                        fx.place_turned(i, at, x, y);
                    }
                }
                (Some((_, (at, x, y))), Some(i)) => fx.place_turned(i, at, x, y),
                (None, Some(i)) => {
                    fx.kill(i);
                    *id = None;
                }
                (None, None) => {}
            }
        }
    }
    shown.retain(|jazz, ids| {
        let keep = players.contains(*jazz);
        if !keep {
            for i in ids.iter_mut().filter_map(|i| i.take()) {
                fx.kill(i);
            }
        }
        keep
    });
}

/// The enemies a beam from `start` to `end` chains to (TnWeaponChainBeam.
/// UpdateChainBeam): every live one, other than the one it strikes (`struck`), whose
/// middle is within the chain's reach of the beam's line, inside its cone round
/// that line from the start, and in sight of the nearest point of the line; each
/// with that point (the chain's arc ends there).
pub(crate) fn chain_targets(
    beam: &Beam,
    (start, end): (Vec3, Vec3),
    struck: Option<Entity>,
    sentries: &[Target],
    level: &Level,
    colliders: &Colliders,
) -> Vec<(Entity, Vec3)> {
    let ray = end - start;
    let length = ray.length();
    if length < 1e-3 {
        return Vec::new();
    }
    let dir = ray / length;
    let cone = beam.chain_cone.to_radians().cos();
    sentries
        .iter()
        .filter(|t| Some(t.entity) != struck)
        .filter_map(|target| {
            let (e, at) = (target.entity, target.at);
            let t = (at - start).dot(dir).clamp(0.0, length);
            let closest = start + dir * t;
            if closest.distance(at) > beam.chain_reach {
                return None;
            }
            let to = (at - start).normalize_or_zero();
            if to.dot(dir) < cone {
                return None;
            }
            // (TraceSight from the enemy to the beam: the level in the way blocks
            // it; the enemy's own body doesn't.)
            let gap = closest - at;
            let reach = gap.length();
            let clear = target.body.reach(closest) <= 0.05
                || level
                    .shot_surface(colliders, at, gap / reach, reach)
                    .is_none_or(|(d, _)| d >= reach - 0.05);
            clear.then_some((e, closest))
        })
        .collect()
}

/// A beam weapon's round's chains (TnWeaponChainBeam.ProcessBeamHit's
/// UpdateChainBeam, every `interval` seconds of firing, the round's own damage done
/// as any round's): each chain's damage to an enemy chained to the beam from `start`
/// to `end` (ChainDamagePerSecond times the interval; a marked one takes more:
/// orbital.rs; TnDamageTypeChainBeam's death types, Energy). How many it wounded.
pub(crate) fn chain_round(
    beam: &Beam,
    interval: f32,
    (start, end): (Vec3, Vec3),
    (struck, by): (Option<Entity>, Option<Entity>),
    level: &Level,
    colliders: &Colliders,
    enemies: &mut Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
) -> u32 {
    let sentries = live_targets(enemies.iter().map(|(e, _, en)| (e, en)));
    let mut wounded = 0;
    for (entity, closest) in chain_targets(beam, (start, end), struck, &sentries, level, colliders)
    {
        if let Ok((_, _, mut e)) = enemies.get_mut(entity)
            && e.alive()
        {
            let at = e.centre();
            e.take(
                beam.chain_dps * interval,
                by,
                crate::xp::Blow::Weapon,
                Hit::new(Death::Energy, at, at - closest).of_type("TnDamageTypeChainBeam"),
            );
            wounded += 1;
        }
    }
    wounded
}

/// The live enemies as rounds and chains find them.
pub(crate) fn live_targets<'a>(enemies: impl Iterator<Item = (Entity, &'a Enemy)>) -> Vec<Target> {
    enemies
        .filter(|(_, e)| e.alive())
        .map(|(entity, e)| e.target(entity))
        .collect()
}

/// A Jazz's beam as it shows: the beam's effect, each chain's, and the light.
#[derive(Default)]
struct BeamShow {
    fx: Option<FxId>,
    chains: Vec<(Entity, FxId)>,
    light: Option<Entity>,
    /// How far the light is up (0 to 1: its ramp).
    level: f32,
}

/// The beams of the beam weapons, for every Jazz (the others' as their PCs report
/// them): while a Jazz fires one, its looping tracer from the muzzle (on the socket,
/// turned along the barrel) to where its latest round struck
/// (TnWeaponEffectPlayTracerFX Looping: CreateLoopingTracer, UpdateLoopingTracer),
/// each chain's arc from an enemy chained to the nearest point of the beam, and its
/// light coming up on the muzzle; as he stops, the beam and its chains are turned
/// off (what is out of them lives out its life) and the light goes down.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn beams(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    mut fx: ResMut<FxSystems>,
    players: Query<(Entity, &Player)>,
    gun_bones: Elbows,
    enemies: Query<(Entity, &Transform, &Enemy), bevy::ecs::query::Without<Player>>,
    mut lights: Query<(&mut PointLight, &mut Transform), bevy::ecs::query::Without<Enemy>>,
    mut shows: Local<HashMap<Entity, BeamShow>>,
) {
    let dt = time.delta_secs();
    let sentries = live_targets(enemies.iter().map(|(e, _, en)| (e, en)));
    for (jazz, player) in &players {
        let weapon = WEAPONS.get(player.weapon);
        let gear = weapon.and_then(|w| w.gear);
        let Some(beam) = gear.and_then(|g| g.beam) else {
            continue;
        };
        let show = shows.entry(jazz).or_default();
        let muzzle = crate::muzzle_socket(player, jazz, &gun_bones);
        let on = player.firing
            && player.gear.fired
            && player.form == Form::Robot
            && player.dead_left <= 0.0
            && !player.dying;
        match (on, muzzle) {
            (true, Some((at, dir))) => {
                let range = weapon.map_or(27.5, |w| w.range);
                let end = player.gear.beam_end.unwrap_or(at + dir * range);
                let tracer = weapon
                    .and_then(|w| w.level_fx)
                    .map(|f| f.tracer)
                    .unwrap_or("");
                if show.fx.is_none_or(|id| !fx.alive(id)) {
                    show.fx = fx.spawn(tracer, at, dir, ENERGON);
                }
                if let Some(id) = show.fx {
                    fx.place(id, at, dir, Vec3::ZERO);
                    fx.set_beam_end(id, end);
                }
                // The chains: the struck enemy has none (the beam itself is on it).
                let struck = sentries
                    .iter()
                    .find(|t| t.body.reach(end) < 0.1)
                    .map(|t| t.entity);
                let chained =
                    chain_targets(&beam, (at, end), struck, &sentries, &level, &colliders);
                let mut kept = Vec::new();
                for (enemy, closest) in chained {
                    let Some(c) = sentries.iter().find(|t| t.entity == enemy).map(|t| t.at) else {
                        continue;
                    };
                    let id = match show.chains.iter().position(|(e, _)| *e == enemy) {
                        Some(i) => Some(show.chains.swap_remove(i).1),
                        None => fx.spawn(beam.chain_fx, c, closest - c, ENERGON),
                    };
                    if let Some(id) = id {
                        fx.place(id, c, closest - c, Vec3::ZERO);
                        fx.set_beam_end(id, closest);
                        kept.push((enemy, id));
                    }
                }
                for (_, id) in std::mem::replace(&mut show.chains, kept) {
                    fx.stop(id);
                }
                if let Some(light) = gear.and_then(|g| g.firing_light) {
                    show.level = (show.level + dt / light.ramp_in.max(1e-3)).min(1.0);
                    let lit = show.light.and_then(|e| lights.get_mut(e).ok());
                    match lit {
                        Some((mut l, mut t)) => {
                            l.intensity = light.brightness * crate::fx::LIGHT_LUMENS * show.level;
                            t.translation = at;
                        }
                        None => {
                            let [r, g, b] = light.color;
                            show.light = Some(
                                commands
                                    .spawn((
                                        crate::graphics::LiveLight,
                                        PointLight {
                                            color: Color::srgb_u8(r, g, b),
                                            intensity: light.brightness
                                                * crate::fx::LIGHT_LUMENS
                                                * show.level,
                                            range: light.radius,
                                            shadows_enabled: false,
                                            ..default()
                                        },
                                        NotShadowCaster,
                                        Transform::from_translation(at),
                                    ))
                                    .id(),
                            );
                        }
                    }
                }
            }
            _ => {
                if let Some(id) = show.fx.take() {
                    fx.stop(id);
                }
                for (_, id) in show.chains.drain(..) {
                    fx.stop(id);
                }
                let ramp_out = gear
                    .and_then(|g| g.firing_light)
                    .map_or(0.5, |l| l.ramp_out);
                show.level = (show.level - dt / ramp_out.max(1e-3)).max(0.0);
                if let Some(e) = show.light {
                    match lights.get_mut(e) {
                        Ok((mut l, mut t)) if show.level > 0.0 => {
                            let brightness = gear
                                .and_then(|g| g.firing_light)
                                .map_or(0.0, |l| l.brightness);
                            l.intensity = brightness * crate::fx::LIGHT_LUMENS * show.level;
                            if let Some((at, _)) = muzzle {
                                t.translation = at;
                            }
                        }
                        _ => {
                            commands.entity(e).try_despawn();
                            show.light = None;
                        }
                    }
                }
            }
        }
    }
    // Jazzes gone (or with another gun): their beams go too.
    shows.retain(|jazz, show| {
        let keep = players
            .get(*jazz)
            .ok()
            .and_then(|(_, p)| WEAPONS.get(p.weapon))
            .and_then(|w| w.gear)
            .is_some_and(|g| g.beam.is_some());
        if !keep {
            if let Some(id) = show.fx.take() {
                fx.stop(id);
            }
            for (_, id) in show.chains.drain(..) {
                fx.stop(id);
            }
            if let Some(e) = show.light.take() {
                commands.entity(e).try_despawn();
            }
        }
        keep
    });
}

/// A Jazz's trajectory previews: their effects (one a blade for a launcher of
/// blades), and whether each blade's path last struck an enemy
/// (TnProjectileSawblade's _LastSimulationHitTarget: its path's colour); an arcing
/// weapon's impact marker, and the normals its simulated lob last struck, with their
/// times (its _NormalHistory: the marker turns to their average).
#[derive(Default)]
struct Preview {
    fx: [Option<FxId>; 3],
    struck: [bool; 3],
    marker: Option<FxId>,
    normals: Vec<(f32, Vec3)>,
    last_normal: Option<Vec3>,
}

/// How long an arcing weapon's impact marker averages the normals it is turned by
/// (_NormalHistory.SetSampleTime(0.25)).
const MARKER_NORMAL_SECS: f32 = 0.25;
/// How long its lob is simulated (its round's SimulationMaxTime, TnProjectileData's
/// default).
const ARC_SIMULATION_SECS: f32 = 3.0;

/// The way blade mode `k`'s round leaves `muzzle` (TnWeaponSawblade's
/// GetProjectileFireParameters with that fire mode): the view turned by the mode's
/// yaw (AddSpread: DesignPerBlade's Spread on the aim's rotator, Unreal's yaw turning
/// it to the right), traced from the camera's line (main.rs's shot_start, from
/// level with `centre`) to the first sentry or wall within `range` (as fire traces
/// its rounds), and from the muzzle to there.
#[allow(clippy::too_many_arguments)]
pub(crate) fn blade_aim(
    blades: &Blades,
    k: usize,
    (eye, view): (Vec3, Vec3),
    centre: Vec3,
    muzzle: Vec3,
    range: f32,
    (level, colliders): (&Level, &Colliders),
    sentries: &[Target],
) -> Vec3 {
    let yaw = blades.yaw.get(k).copied().unwrap_or(0) as f32 * std::f32::consts::TAU / 65536.0;
    let aim = (Quat::from_rotation_y(-yaw) * view).normalize_or(view);
    let start = crate::shot_start(level, colliders, (eye, aim), centre, muzzle);
    let mut distance = level
        .shot_surface(colliders, start, aim, range)
        .map_or(range, |(t, _)| t);
    for target in sentries {
        if let Some(t) = target.body.ray(start, aim, distance) {
            distance = t;
        }
    }
    (start + aim * distance - muzzle).normalize_or(aim)
}

/// The rounds' paths of the guns that show them (Gear::trajectory), for this PC's
/// Jazz (the game simulates them only for the locally controlled pawn): every frame
/// its round fired now is simulated (TnProjectile.RunPhysicsSimulation, a place
/// every SimulationTimeStep, 0.05 s, for at most SimulationMaxTime, or until it
/// strikes something) and the projectile mesh's TrajectoryPreviewEffect is run
/// along it. The Corrosive Slime Cannon's while the fine-aim button is held
/// (TnSimulateProjectileExtension: 3 s); a launcher of blades' while Jazz fine aims
/// with it ready or charging (TnWeaponSawblade.ShouldSimulateTrajectory), a path for
/// each blade loaded, one at least, each from its fire mode's muzzle, bouncing and
/// homing as a blade would (riot.rs's simulate_blade), its colour set at each place
/// (OnSimulationStepped), turned the way the blade is fired (it keeps its
/// Rotation).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn trajectories(
    time: Res<Time>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    mut fx: ResMut<FxSystems>,
    players: Query<(Entity, &Transform, &Player, &crate::PlayerInput), With<LocalPlayer>>,
    everyone: Query<(Entity, &Transform, &Player)>,
    gun_bones: Elbows,
    enemies: Query<(Entity, &Transform, &Enemy), bevy::ecs::query::Without<Player>>,
    mut preview: Local<Preview>,
) {
    let live = live_targets(enemies.iter().map(|(e, _, en)| (e, en)));
    let bodies: Vec<(Entity, Vec3, (f32, f32))> = everyone
        .iter()
        .filter(|(_, _, p)| p.dead_left <= 0.0 && !p.dying && !p.waiting)
        .map(|(e, t, p)| (e, t.translation, crate::riot::body_of(p)))
        .collect();
    let now = time.elapsed_secs();
    let mut shown = 0;
    let mut marked = false;
    for (jazz, transform, player, input) in &players {
        let Some(weapon) = WEAPONS.get(player.weapon) else {
            continue;
        };
        let (Some(system), Some(kind)) =
            (weapon.gear.and_then(|g| g.trajectory), weapon.projectile)
        else {
            continue;
        };
        let free =
            player.form == Form::Robot && player.dead_left <= 0.0 && player.transform_left <= 0.0;
        // (Where the shot would go, as fire aims it: at what the crosshair is on.)
        let view = input.view_dir.normalize_or(*transform.forward());
        if let Some(blades) = weapon.gear.and_then(|g| g.blades) {
            let ready = free
                && player.aiming
                && player.fire_cooldown <= 0.0
                && player.reload_left <= 0.0
                && player.hand_switch.is_none();
            if !ready {
                continue;
            }
            let centre = player.robot_centre(transform.translation);
            let count = (player.gear.blades.max(1) as usize).min(preview.fx.len());
            for k in 0..count {
                let Some((muzzle, _)) = crate::muzzle_socket_at(player, jazz, &gun_bones, k) else {
                    continue;
                };
                let dir = blade_aim(
                    &blades,
                    k,
                    (input.view_origin, view),
                    centre,
                    muzzle,
                    weapon.range,
                    (&level, &colliders),
                    &live,
                );
                let (path, colors, struck) = crate::riot::simulate_blade(
                    kind,
                    (&level, &colliders),
                    &live,
                    &bodies,
                    Some(jazz),
                    (muzzle, dir),
                    preview.struck[k],
                );
                preview.struck[k] = struck;
                if preview.fx[k].is_none_or(|id| !fx.alive(id)) {
                    preview.fx[k] =
                        fx.spawn_scaled(system, muzzle, dir, kind.flight_scale, ENERGON);
                }
                if let Some(id) = preview.fx[k] {
                    let steps: Vec<(Vec3, Vec3)> = path.iter().map(|&p| (p, dir)).collect();
                    fx.replay_colored(id, &steps, crate::riot::SIMULATION_STEP, "Color", colors);
                }
            }
            shown = count;
            continue;
        }
        if let Some(arc) = weapon.gear.and_then(|g| g.arc) {
            // An arcing weapon's lob (SimulateTrajectory, while _IsAiming): its round
            // tossed from the muzzle to what the camera's line strikes within the
            // weapon's Range (CalculateTossVelocity from GetProjectileFireParameters'
            // trace: its flight time by that distance from the muzzle over the trace's
            // far end's), run until it would strike something.
            let ready = player.gear.arc && free;
            let Some((muzzle, _)) =
                crate::muzzle_socket(player, jazz, &gun_bones).filter(|_| ready)
            else {
                continue;
            };
            let centre = player.robot_centre(transform.translation);
            let start = crate::shot_start(
                &level,
                &colliders,
                (input.view_origin, view),
                centre,
                muzzle,
            );
            let end = start + view * weapon.range;
            let mut reach = level
                .shot_surface(&colliders, start, view, weapon.range)
                .map_or(weapon.range, |(t, _)| t);
            for target in &live {
                if let Some(t) = target.body.ray(start, view, reach) {
                    reach = t;
                }
            }
            let hit = start + view * reach;
            let k = (muzzle.distance(hit) / muzzle.distance(end).max(1e-3)).clamp(0.0, 1.0);
            let secs = arc.flight_secs.0 + (arc.flight_secs.1 - arc.flight_secs.0) * k;
            let velocity = crate::riot::toss_velocity(muzzle, hit, secs, kind.gravity);
            let (path, struck) = crate::riot::simulate_flight(
                kind,
                (&level, &colliders),
                (&live, &bodies),
                Some(jazz),
                (muzzle, velocity),
                ARC_SIMULATION_SECS,
            );
            let dir = velocity.normalize_or(view);
            if preview.fx[0].is_none_or(|id| !fx.alive(id)) {
                preview.fx[0] = fx.spawn_scaled(system, muzzle, dir, kind.flight_scale, ENERGON);
            }
            if let Some(id) = preview.fx[0] {
                let steps: Vec<(Vec3, Vec3)> = path
                    .windows(2)
                    .map(|w| (w[0], (w[1] - w[0]).normalize_or(dir)))
                    .chain(path.last().map(|&p| (p, dir)))
                    .collect();
                fx.replay(id, &steps, crate::riot::SIMULATION_STEP);
            }
            // Its marker where the lob ends, turned into the surfaces it has struck
            // lately (their normals averaged: SetRotation(-AverageNormal)).
            if let Some(n) = struck {
                preview.last_normal = Some(n);
            }
            if let Some(n) = preview.last_normal {
                preview.normals.push((now, n));
            }
            preview
                .normals
                .retain(|(t, _)| now - *t <= MARKER_NORMAL_SECS);
            let average = preview
                .normals
                .iter()
                .fold(Vec3::ZERO, |sum, (_, n)| sum + *n)
                .normalize_or(Vec3::Y);
            let end_at = path.last().copied().unwrap_or(muzzle);
            if preview.marker.is_none_or(|id| !fx.alive(id)) {
                preview.marker = fx.spawn(arc.marker, end_at, -average, ENERGON);
            }
            if let Some(id) = preview.marker {
                fx.place(id, end_at, -average, Vec3::ZERO);
            }
            marked = true;
            shown = 1;
            continue;
        }
        let ready = player.gear.alt && free;
        let Some((muzzle, _)) = crate::muzzle_socket(player, jazz, &gun_bones).filter(|_| ready)
        else {
            continue;
        };
        let reach = weapon.range.max(150.0);
        let aim_at =
            input.view_origin + view * level.ray(input.view_origin, view, reach).unwrap_or(reach);
        let dir = (aim_at - muzzle).normalize_or(view);
        let path = crate::riot::simulate_round(
            kind,
            (&level, &colliders),
            (&live, &bodies),
            Some(jazz),
            muzzle,
            dir,
        );
        // (On the simulated round, at its size: its mesh's DrawScale.)
        if preview.fx[0].is_none_or(|id| !fx.alive(id)) {
            preview.fx[0] = fx.spawn_scaled(system, muzzle, dir, kind.flight_scale, ENERGON);
        }
        if let Some(id) = preview.fx[0] {
            let steps: Vec<(Vec3, Vec3)> = path
                .windows(2)
                .map(|w| (w[0], (w[1] - w[0]).normalize_or(dir)))
                .chain(path.last().map(|&p| (p, dir)))
                .collect();
            fx.replay(id, &steps, crate::riot::SIMULATION_STEP);
        }
        shown = 1;
    }
    // (Those no longer simulated go: StopSimulatingTrajectory; an arcing weapon's
    // marker with its lob: KillSimulationProjectile's DetachImpactMarker.)
    for k in shown..preview.fx.len() {
        if let Some(id) = preview.fx[k].take() {
            fx.kill(id);
        }
        preview.struck[k] = false;
    }
    if !marked {
        if let Some(id) = preview.marker.take() {
            fx.kill(id);
        }
        preview.normals.clear();
        preview.last_normal = None;
    }
}
