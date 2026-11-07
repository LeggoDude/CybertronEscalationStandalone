//! The Riot Cannon's rounds: the game's TnProjectileBazooka, as WEP_Bazooka_p and
//! TransGame.xxx have it (Escalation's weapon data, Bazooka_SV_WEPDATA, fires
//! TnProjectileBazooka, whose data for Escalation is its Data, Bazooka_SP_PROJDATA:
//! TnProjectile.GetProjectileData gives the Data of a projectile with no
//! SurvivalData).
//!
//! * **Flight** (Bazooka_SP_PROJDATA, Bazooka_PROJMESH): the round leaves the
//!   muzzle at 180 m/s and keeps that speed (InitialSpeed and MaxSpeed 18000) in a
//!   straight line (PHYS_Projectile: no gravity), for at most 4 s (TnProjectile's
//!   LifeSpan), after which it goes off where it is, facing back the way it flies
//!   (HmProjectile.OnWaitingForLifeSpanDestruction). It flies as Tracer_Bazooka_FX (an energy
//!   ball flickering with bursts and arcs, trailing smoke and a crackling
//!   distortion), with its light (Radius 800, Brightness 8, colour (100, 65, 109))
//!   and its flight loop (TRAIL_LP).
//! * **Striking** (TnProjectile.ProcessTouch, Explode): it goes off on the first
//!   thing in its way: the level, an arena pillar, a sentry or another player (the
//!   players stop rounds as any pawn does, TnPlayerPawn.StopsProjectile; it passes
//!   through the one who fired it, Projectile.Touch's ShouldIgnoreInstigator). Its
//!   blast hurts only the sentries and him.
//! * **Going off** (Projectile.Explode, HmProjectile.SpawnExplosionEffect,
//!   TnProjectile.PlayExplosion): RiotCannon_ImpactExp_FX along the surface's
//!   normal at DamageRadius / ExplosionEffectRadius (650 / 750) of its size, its
//!   blast (EXPL_IMPT_WORLD); its light goes out and its flight effect is let go
//!   (what is out of it lives out its life: bWaitForFlightEffect). Its damage is
//!   Projectile.HurtRadius's: 155 (Damage) to everything within 6.5 m (DamageRadius,
//!   reaching into what it overlaps: CollidingActors), whatever lies between
//!   (IgnoreHurtRadiusTrace), at full strength at any distance (its
//!   RadiusDamageScale has no Scales: FloatScaler.GetValueAtPoint is 1). It hurts
//!   the Jazz who fired it too, at TnGame's SelfDamageMultiplier (0.25:
//!   TnPawn.AdjustDamage), but not the other players.
//!
//! The Titan's tanks fire rounds too: their cannon's shells (TANK_SHELL: the
//! TankCannon's Escalation data fires TnProjectileTankShell, TankShell_SV_PROJDATA
//! and TankShell_PROJMESH), flying at 170 m/s as Tracer_HomRocket_FX with their own
//! flight loop, going off as TankCannon_ImpactExp_FX at three quarters of its size
//! (ExplosionEffectScale, and no ExplosionEffectRadius: HmProjectile.
//! SpawnExplosionEffect) for 300 within 12.5 m, at full strength out to 5 m and
//! less and less to nothing at 12.5 m (its RadiusDamageScale: a constant stretch,
//! a linear one, its max range; FloatScaler.GetValueAtPoint).
//!
//! The Scientist's jets fire homing rockets (JET_ROCKET: HomingRocketVehicle's
//! Escalation data fires TnProjectileHomingRocketVehicle, whose data for Escalation
//! is its Data, HomingRocketVh_SP_PROJDATA), flying at 100 m/s as
//! Tracer_HomRocket_FX; one fired with a sentry locked on (jet.rs's Lock) homes on
//! it (TnProjectileHoming: pushed towards it at HomingForce, 800 m/s^2, its speed
//! held to MaxSpeed; within ClosingDistance, 25 m, at ClosingForce, 800 m/s^2 too,
//! going off where it is if it hasn't struck in ClosingTime, a second), and goes off
//! as HomRocket_Explosion_FX for 75 within 5 m, at full strength throughout (its
//! RadiusDamageScale has no Scales).
//!
//! The Destroyer's trucks fire rockets that fly straight (TRUCK_ROCKET:
//! RocketVehicle's Escalation data fires TnProjectilePlaneRocketTemp, whose data for
//! Escalation is its Data, RocketVh_SP_PROJDATA, and Rocket_PROJMESH), at 120 m/s as
//! Tracer_HomRocketVH_FX with the jets' rockets' flight loop, going off as
//! Squib_HomRocket_01_FX at three quarters of its size for 55 within 15 m: at full
//! strength out to 7.5 m, less and less to a quarter at 14 m and a quarter beyond
//! (its RadiusDamageScale).
//!
//! A round's flight and explosion effects take its projectile mesh's Parameters
//! (HmProjectile.OnParticleSystemCreated): its TnParticleSystemParameterEnergonColor
//! (ParameterName Color) gives them the team's energon colour, cyan, and its
//! TnParticleSystemParameterTeamIndex the module channel of its shooter's side
//! (round_params).
//!
//! Rounds fired go to the other players' PCs (net.rs's Round), where their copies
//! fly and go off the same way (striking their sentries, as their copies of shots
//! do: remote_shot; a rocket's copy flies straight).

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use std::collections::HashMap;

use crate::enemies::{Death, Hit, Target};
use crate::hud::HudEvent;
use crate::levelfx::{FxId, FxSystems, MODULE_CHANNEL, MODULE_CHANNEL_A, MODULE_CHANNEL_B};
use crate::sound::Sfx;
use crate::{Colliders, Enemy, Form, HIT_INDICATOR_HEALTH, Level, LocalPlayer, Player};

/// The share of a blast's damage that hurts the Jazz whose round it is: the game's
/// SelfDamageMultiplier, which Escalation sets to nothing ([TransContent.
/// TnSurvivalGame] SelfDamageMultiplier=0, over TnGame's 0.25 for the other games):
/// a player's own blasts never hurt him.
pub(crate) const SELF_DAMAGE: f32 = 0.0;

/// A weapon's rounds (WeaponFireTypes EWFT_Projectile): its projectile's data and
/// mesh (TnProjectileData, TnProjectileMesh).
pub(crate) struct RoundKind {
    /// Metres a second (InitialSpeed, MaxSpeed) and the longest it flies (LifeSpan).
    pub speed: f32,
    pub life_secs: f32,
    /// Damage (Damage) within this many metres (DamageRadius), and the share of it
    /// that hurts its own Jazz (the game's SelfDamageMultiplier: SELF_DAMAGE).
    pub damage: f32,
    pub radius: f32,
    pub self_damage: f32,
    /// Its effects (fx/weapons_fx.txt): the weapon's muzzle flash
    /// (TnWeaponEffectPlayMuzzleFX), flying (FlightEffect) and going off
    /// (ExplosionEffect, at `explosion_scale` of its size).
    pub muzzle_fx: &'static str,
    pub flight_fx: &'static str,
    pub explosion_fx: &'static str,
    pub explosion_scale: f32,
    /// Its flight loop (FlightSound) and blast (ExplosionSound).
    pub flight_sound: &'static str,
    pub explosion_sound: &'static str,
    /// Its light (its PointLightComponent): colour, Brightness and Radius (metres);
    /// None without one.
    pub light: Option<([u8; 3], f32, f32)>,
    /// How its damage falls off (its RadiusDamageScale); None for full strength
    /// throughout.
    pub falloff: Option<Falloff>,
    /// How it homes on a target, if it does (TnProjectileDataHoming).
    pub homing: Option<Homing>,
    /// How strongly the world's gravity pulls it (GravityScale: 0 for a round
    /// that flies straight, PHYS_Projectile).
    pub gravity: f32,
    /// The size it is drawn at (its projectile mesh's DrawScale, which
    /// TnProjectile.UpdateMesh gives the round): its flight effect (and its path's
    /// preview), attached to it, is that many times its size.
    pub flight_scale: f32,
    /// A round that bounces off what it strikes and then sticks in it (the Gear
    /// Shredder's blades: Bounce); None for one that goes off on the first thing.
    pub bounce: Option<&'static Bounce>,
    /// It turns to face the way it flies (bRotationFollowsVelocity, TnProjectile's);
    /// else it keeps facing the way it was fired (the grenades': TnProjectileGrenadeBase),
    /// and its effects with it.
    pub follows_velocity: bool,
    /// Its mesh, drawn on it (its projectile mesh's StaticMesh: fx/weapons_fx.txt's
    /// system of it, tools/build_weapon_fx.py's MESH_SYSTEMS), "" for none.
    pub mesh: &'static str,
    /// The camera shake its blast gives the players near it (its data's
    /// TnCameraShakeProjectileExtension), if any.
    pub shake: Option<&'static ScreenShake>,
    /// A round that sticks in the first thing it strikes and goes off after its fuse
    /// (the A-4 Pulsar Cannon's: TnProjectileGrenadeLauncher).
    pub sticky: Option<&'static Sticky>,
    /// A round that falls, bounces and goes off after its fuse (TnProjectileGrenadeBase:
    /// the Chaos-Rift Combustor's clusters).
    pub grenade: Option<&'static Grenade>,
    /// A round that bursts into clusters as it goes off (the Chaos-Rift Combustor's
    /// bomb: TnClusterBombProjectileExtension).
    pub clusters: Option<&'static Clusters>,
}

/// The camera shake a round's blast gives the players near it (its data's
/// TnCameraShakeProjectileExtension, played by TnProjectile.PlayLocalCameraShake for
/// the players on that PC): a player further from the blast than its DamageRadius but
/// within `radius` metres (CameraShakeRadius) has his view shaken by its
/// ScreenShakeStruct (HmCameraModifierScreenShake), scaled down by his distance
/// (UseFalloff, the extension's default: all of it, its time, amplitudes and
/// frequencies, times 1 - distance / radius). Each wave of it runs at its frequency
/// (radians a second) from a random start (ESP_OffsetRandom), at its amplitude, both
/// fading out over `secs` (main.rs's camera plays it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScreenShake {
    pub radius: f32,
    pub secs: f32,
    /// The view's turn: pitch, yaw and roll (RotAmplitude, Unreal's units of 1/65536
    /// of a turn) and their frequencies (RotFrequency).
    pub rot_amp: Vec3,
    pub rot_freq: Vec3,
    /// The view's place: forward, right and up (LocAmplitude, centimetres) and their
    /// frequencies (LocFrequency).
    pub loc_amp: Vec3,
    pub loc_freq: Vec3,
    /// The field of view (FOVAmplitude, degrees) and its frequency (FOVFrequency).
    pub fov_amp: f32,
    pub fov_freq: f32,
}

impl ScreenShake {
    /// The shake a player `distance` metres from a blast of damage radius `reach`
    /// feels (GetCameraShake with UseFalloff), if any: none within the blast's reach
    /// or beyond the shake's radius.
    pub fn felt(&self, distance: f32, reach: f32) -> Option<ScreenShake> {
        if distance <= reach || distance > self.radius {
            return None;
        }
        let k = 1.0 - distance / self.radius.max(1e-3);
        Some(ScreenShake {
            radius: self.radius,
            secs: self.secs * k,
            rot_amp: self.rot_amp * k,
            rot_freq: self.rot_freq * k,
            loc_amp: self.loc_amp * k,
            loc_freq: self.loc_freq * k,
            fov_amp: self.fov_amp * k,
            fov_freq: self.fov_freq * k,
        })
    }
}

/// A round that sticks in the first thing it strikes (TnProjectileGrenadeLauncher's
/// OnHitThing: it stops, PHYS_None, and AttachToActor: the level holds it where it
/// touched; an enemy or another player on his closest bone, with no offset), its
/// flight's loop stopping and `sound` starting (TnProjectileGrenadeBase.HitThing:
/// StopFlightSound, PlayFuseSound, its FuseSound). It goes off once its fuse runs out
/// (`fuse` seconds, FuseTime: its LifeSpan from the strike), facing back the way it
/// was fired (HmProjectile.OnWaitingForLifeSpanDestruction: -Rotation), or at once,
/// facing up, as its shooter sets it off (the launcher's alt fire:
/// TnWeaponGrenadeLauncherBase.DetonateGrenade), fires one more than `most` (the
/// oldest goes: NumberOfActiveGrenades) or the enemy it is in is destroyed (its Tick:
/// ShouldDetach).
#[derive(Clone, Copy)]
pub(crate) struct Sticky {
    pub fuse: f32,
    pub most: usize,
    pub sound: &'static str,
}

/// A grenade (TnProjectileGrenadeBase, its TnProjectileDataGrenadeLauncher): it falls
/// (its GravityScale) and bounces off what it strikes (OnHitThing: its velocity
/// mirrored in the surface, keeping `keep` of it: BounceDampening; its BounceSound),
/// its fuse starting as it first strikes something (HitThing: its LifeSpan set to
/// FuseTime, at random between `fuse`'s two; its flight's loop stopping and its
/// FuseSound starting), and it stops once its speed over the last second is below a
/// metre a second (Tick: LowSpeedThreshold over its _DeltaPostionHistory). With
/// `explode_on_pawn` it goes off on the first enemy or player it strikes
/// (ExplodeWhenHittingPawn). Its fuse out, it goes off where it is, facing back the
/// way it was fired (OnWaitingForLifeSpanDestruction).
#[derive(Clone, Copy)]
pub(crate) struct Grenade {
    pub keep: f32,
    pub fuse: (f32, f32),
    pub explode_on_pawn: bool,
    pub sound_bounce: &'static str,
    pub sound_fuse: &'static str,
}

/// A round that bursts into clusters as it goes off, however it does
/// (TnProjectileTankShellArcable.Explode: its TnClusterBombProjectileExtension's
/// SpawnCluster): `count` rounds of `bomb` (NumProjectiles, ProjectileClass) from where
/// it went off, each along a way at random within `cone` degrees of its "normal"
/// (RandomVectorInCone; struck, the surface's normal; set off in the air, its
/// velocity), at `transfer` times that (VelocityTransferScale: a surface's unit normal
/// counts as a centimetre a second) plus `speed` metres a second (AdditionalVelocity,
/// at random between its two) along its own way.
#[derive(Clone, Copy)]
pub(crate) struct Clusters {
    pub count: u32,
    pub cone: f32,
    pub transfer: f32,
    pub speed: (f32, f32),
    pub bomb: &'static RoundKind,
}

/// How a blast's damage falls off with distance (its RadiusDamageScale, a FloatScaler
/// of a constant stretch, a linear one and its max range: FloatScaler.
/// GetValueAtPoint): full out to `full` metres, then less and less, in a straight
/// line, to `floor` of it at `reach`, and `floor` from there out to its radius.
#[derive(Clone, Copy)]
pub(crate) struct Falloff {
    pub full: f32,
    pub reach: f32,
    pub floor: f32,
}

/// A homing round's steering (TnProjectileHoming's Homing and Closing states): its
/// acceleration towards its target (HomingForce, m/s^2), and within this many
/// metres of it (ClosingDistance) its closing one (ClosingForce), going off where it
/// is after this many seconds of closing (ClosingTime).
#[derive(Clone, Copy)]
pub(crate) struct Homing {
    pub force: f32,
    pub closing_distance: f32,
    pub closing_force: f32,
    pub closing_secs: f32,
}

/// A round that bounces off what it strikes for a while, then sticks in the first
/// thing it strikes (TnProjectileSawblade: its TnSawBladeProjectileExtension, its
/// HitThing, Bounce, OnHitPawn and homing).
#[derive(Clone, Copy)]
pub(crate) struct Bounce {
    /// The level turns it back for `secs` after it is fired (BounceDuration), and
    /// it keeps `keep` of its speed each time (BounceSpeedPercent); after that, or
    /// once it has bounced `most` times (Bounce: its eleventh is none), it sticks in
    /// the first thing it strikes. The players' side turn it back at any age
    /// (OnHitPawn: OnSameTeamAs), the Jazz who fired it not at all (it passes
    /// through him: Projectile.Touch's ShouldIgnoreInstigator). The enemy it sticks
    /// in takes its Damage.
    pub secs: f32,
    pub keep: f32,
    pub most: u32,
    /// Once it has bounced it homes (TickHoming's AfterFirstBounce profile: its
    /// profile before its first bounce is empty, so it flies straight until then):
    /// on the enemy nearest its line (FindTargetInCone, by the distance from the
    /// enemy's body to the line: AI.CalculateRangeToPoint) within `reach` metres of
    /// it (MaxDistanceToTarget) and `off_path` metres of its line
    /// (MaxDistanceFromPath), inside a cone of `cone` degrees round the way it flies
    /// (MaxAngleToTarget), keeping it while it is ahead (UpdateHomingTarget); its
    /// velocity turned towards it by at most `turn` metres a second each second
    /// (HomingSpeedAfterFirstBounce: HomeTowardsTarget's MaxSpeedDelta). A bounce
    /// finds no new target (its Bounce profile is empty) but turns it straight at
    /// one still ahead (UpdateHomingVelocity with no limit).
    pub reach: f32,
    pub off_path: f32,
    pub cone: f32,
    pub turn: f32,
    /// Its collision cylinder (CollisionRadius, and CollisionHeight, half its
    /// height): what it strikes, its edge strikes first.
    pub radius: f32,
    pub half_height: f32,
    /// Its bounce's sound (the projectile mesh's BounceSound).
    pub sound: &'static str,
    /// Stuck, it shows its mesh where it stopped (fx/weapons_fx.txt's
    /// SawBlade_Stuck_FX: TnProjectileSawblade.Explode gives its MeshComp the
    /// projectile mesh's StaticMesh), for the 10 s it is left
    /// (DestroyProjectile's LifeSpan), or until the enemy it is in is destroyed
    /// (Tick: its Base gone).
    pub stuck_fx: &'static str,
    /// The path fine aim shows: its colour (GetTrajectoryColor's FlyingOnMiss,
    /// HomingOnMiss, FlyingOnHit and HomingOnHit, by whether the path shown before
    /// struck an enemy and whether the simulated round is homing), its alpha going
    /// from 255 to 0 over the simulation's `path_secs` (SimulationMaxTime).
    pub path_colors: [[u8; 3]; 4],
    pub path_secs: f32,
}

/// Whether two kinds of round are the same: by their effects, as a kind's constant
/// may be copied to more than one place.
fn same_kind(a: &RoundKind, b: &RoundKind) -> bool {
    std::ptr::eq(a, b)
        || (a.flight_fx == b.flight_fx && a.explosion_fx == b.explosion_fx && a.mesh == b.mesh)
}

impl RoundKind {
    /// What its blast strikes with (its round's damage type, as its death and the
    /// players' and enemies' deaths go: deaths.rs), its momentum (its data's
    /// MomentumTransfer, Unreal's units: TnProjectileTankShell's 300000,
    /// GrenadeLauncher_SP's 200000, ClusterCannon's 250000, Bazooka_SP's 100000 and
    /// its special's 350000, the homing rockets' 1) and its damage type's
    /// GibDistance (metres: TnDamageTypeHomingRocket's 7).
    /// Its damage type (the kill feed's: its weapon's), none for one not here.
    pub fn damage_type(&self) -> &'static str {
        if same_kind(self, &TANK_SHELL) {
            "TnDamageTypeTankShell"
        } else if same_kind(self, &PULSAR_ROUND) {
            "TnDamageTypeGrenadeLauncher"
        } else if same_kind(self, &COMBUSTOR_SHELL) || same_kind(self, &COMBUSTOR_CLUSTER) {
            "TnDamageTypeClusterBomb"
        } else if same_kind(self, &RIOT_ROUND) || same_kind(self, &PLANET_BUSTER) {
            "TnDamageTypeBazooka"
        } else if same_kind(self, &THERMO_ROCKET) {
            "TnDamageTypeHomingRocket"
        } else if same_kind(self, &JET_ROCKET) {
            "TnDamageTypePlaneRocket"
        } else if same_kind(self, &TRUCK_ROCKET) {
            "TnDamageTypeTruckRocket"
        } else if same_kind(self, &SLIME_ROUND) {
            "TnDamageTypeSlow"
        } else if same_kind(self, &SAW_BLADE) {
            "TnDamageTypeSawBlade"
        } else if same_kind(self, &crate::traps::SENTRY_ROCKET) {
            "TnDamageTypeSentryRocket"
        } else {
            ""
        }
    }

    pub fn blow(&self) -> (Death, f32, f32) {
        if same_kind(self, &TANK_SHELL) {
            (Death::Tank, 300000.0, 0.0)
        } else if same_kind(self, &PULSAR_ROUND) {
            (Death::Explosive, 200000.0, 0.0)
        } else if same_kind(self, &COMBUSTOR_SHELL) || same_kind(self, &COMBUSTOR_CLUSTER) {
            (Death::Explosive, 250000.0, 0.0)
        } else if same_kind(self, &RIOT_ROUND) {
            (Death::Explosive, 100000.0, 0.0)
        } else if same_kind(self, &PLANET_BUSTER) {
            (Death::Explosive, 350000.0, 0.0)
        } else if same_kind(self, &THERMO_ROCKET) || same_kind(self, &JET_ROCKET) {
            (Death::Explosive, 1.0, 7.0)
        } else if same_kind(self, &SLIME_ROUND) {
            (Death::Slow, 0.0, 0.0)
        } else if same_kind(self, &TRUCK_ROCKET) {
            // (TnDamageTypeTruckRocket names no death types: the ragdoll.)
            (Death::Plain, 1.0, 0.0)
        } else {
            (Death::Explosive, 1.0, 0.0)
        }
    }

    /// The share of its damage something this far from the blast takes.
    fn damage_scale(&self, distance: f32) -> f32 {
        match self.falloff {
            Some(f) if distance > f.full => {
                let t = ((distance - f.full) / (f.reach - f.full).max(1e-3)).clamp(0.0, 1.0);
                1.0 + (f.floor - 1.0) * t
            }
            _ => 1.0,
        }
    }
}

/// The Riot Cannon's (TnProjectileBazooka: Bazooka_SP_PROJDATA, Bazooka_PROJMESH).
pub(crate) const RIOT_ROUND: RoundKind = RoundKind {
    speed: 180.0,
    life_secs: 4.0,
    damage: 155.0,
    radius: 6.5,
    self_damage: SELF_DAMAGE,
    muzzle_fx: "RiotBazooka_MuzzleFlash_FX",
    flight_fx: "Tracer_Bazooka_FX",
    explosion_fx: "RiotCannon_ImpactExp_FX",
    explosion_scale: 650.0 / 750.0,
    flight_sound: "BL_WPN_GUN_FUSION_CANNON.TRAIL_LP",
    explosion_sound: "BL_WPN_GUN_FUSION_CANNON.EXPL_IMPT_WORLD",
    light: Some(([100, 65, 109], 8.0, 8.0)),
    falloff: None,
    homing: None,
    gravity: 0.0,
    flight_scale: 1.0,
    bounce: None,
    follows_velocity: true,
    mesh: "",
    shake: None,
    sticky: None,
    grenade: None,
    clusters: None,
};

/// The Riot Cannon's Planet Buster round, the last of a clip with that upgrade
/// (TnWeapon.FireAmmunition's special shot: fire mode 1, its WeaponProjectiles'
/// TnProjectileBazookaSpecial: BazookaSpecial_SP_PROJDATA, BazookaSpecial_SP_PROJMESH):
/// 140 m/s, 500 within 20 m at full strength throughout (its RadiusDamageScale has
/// no Scales), whatever lies between (IgnoreHurtRadiusTrace); flying as the
/// Enforcer's lightning sphere (LightningSphere_Tracer_FX) with the five-times
/// damage loop, going off as EXP_PlanetBuster_FX at its own size (no
/// ExplosionEffectRadius) with its blast; its light white (Radius 800, Brightness
/// 8); a two-second shake out to 300 m (its TnCameraShakeProjectileExtension). Its
/// muzzle flash is the Riot Cannon's own (fire mode 1 has no effects of its own:
/// TnWeaponMesh.GetFireModeEffectIndex falls back to mode 0's).
pub(crate) const PLANET_BUSTER: RoundKind = RoundKind {
    speed: 140.0,
    life_secs: 4.0,
    damage: 500.0,
    radius: 20.0,
    self_damage: SELF_DAMAGE,
    muzzle_fx: "RiotBazooka_MuzzleFlash_FX",
    flight_fx: "LightningSphere_Tracer_FX",
    explosion_fx: "EXP_PlanetBuster_FX",
    explosion_scale: 1.0,
    flight_sound: "BL_WPN_GUN_FUSION_CANNON.TRAIL_LP_5X_DMG",
    explosion_sound: "BL_WPN_GUN_FUSION_CANNON.EXPL_IMPT_WORLD_5x_DMG",
    light: Some(([255, 255, 255], 8.0, 8.0)),
    falloff: None,
    homing: None,
    gravity: 0.0,
    flight_scale: 1.0,
    bounce: None,
    follows_velocity: true,
    mesh: "",
    shake: Some(&PLANET_BUSTER_SHAKE),
    sticky: None,
    grenade: None,
    clusters: None,
};

/// The Planet Buster's blast's shake (BazookaSpecial_SP_PROJDATA's
/// TnCameraShakeProjectileExtension: CameraShakeRadius 30000, its ScreenShakeStruct).
pub(crate) const PLANET_BUSTER_SHAKE: ScreenShake = ScreenShake {
    radius: 300.0,
    secs: 2.0,
    rot_amp: Vec3::new(200.0, 150.0, 200.0),
    rot_freq: Vec3::new(100.0, 75.0, 50.0),
    loc_amp: Vec3::new(10.0, 0.0, 5.0),
    loc_freq: Vec3::new(35.0, 0.0, 20.0),
    fov_amp: 0.0,
    fov_freq: 0.0,
};

/// A tank's cannon's (TnProjectileTankShell: TankShell_SV_PROJDATA, TankShell_PROJMESH,
/// the TankCannon_Vehicle_WEPMESH's muzzle flash); it has no light of its own, and
/// is drawn at 1.5 times its size (the mesh's DrawScale), its flight effect with it.
pub(crate) const TANK_SHELL: RoundKind = RoundKind {
    speed: 170.0,
    life_secs: 4.0,
    damage: 300.0,
    radius: 12.5,
    self_damage: SELF_DAMAGE,
    muzzle_fx: "Tank_MuzzleFlash_FX",
    flight_fx: "Tracer_HomRocket_FX",
    explosion_fx: "TankCannon_ImpactExp_FX",
    explosion_scale: 0.75,
    flight_sound: "BL_WPN_GUN_TANK_CANNON.TRAIL_LP",
    explosion_sound: "BL_WPN_GUN_TANK_CANNON.EXPL_IMPT_WORLD",
    light: None,
    falloff: Some(Falloff {
        full: 5.0,
        reach: 12.5,
        floor: 0.0,
    }),
    homing: None,
    gravity: 0.0,
    flight_scale: 1.5,
    bounce: None,
    follows_velocity: true,
    mesh: "",
    shake: None,
    sticky: None,
    grenade: None,
    clusters: None,
};

/// A jet's homing rocket (TnProjectileHomingRocketVehicle: HomingRocketVh_SP_PROJDATA,
/// HomingRocketVh_PROJMESH, the HomingRocket_Vehicle_WEPMESH's muzzle flash): 75
/// within 5 m, full throughout; no light of its own; drawn at 1.25 times its size
/// (the mesh's DrawScale), its flight effect with it.
pub(crate) const JET_ROCKET: RoundKind = RoundKind {
    speed: 100.0,
    life_secs: 4.0,
    damage: 75.0,
    radius: 5.0,
    self_damage: SELF_DAMAGE,
    muzzle_fx: "MuzzleFlash_HomRocket_FX",
    flight_fx: "Tracer_HomRocket_FX",
    explosion_fx: "HomRocket_Explosion_FX",
    explosion_scale: 1.0,
    flight_sound: "BL_WPN_GUN_ROCKET.TRAIL_LP",
    explosion_sound: "BL_WPN_GUN_ROCKET.EXPL_IMPT_WORLD",
    light: None,
    falloff: None,
    homing: Some(Homing {
        force: 800.0,
        closing_distance: 25.0,
        closing_force: 800.0,
        closing_secs: 1.0,
    }),
    gravity: 0.0,
    flight_scale: 1.25,
    bounce: None,
    follows_velocity: true,
    mesh: "",
    shake: None,
    sticky: None,
    grenade: None,
    clusters: None,
};

/// A truck's rocket (TnProjectilePlaneRocketTemp: RocketVh_SP_PROJDATA,
/// Rocket_PROJMESH, the Rocket_Vehicle_WEPMESH's muzzle flash): 55 within 15 m, full
/// out to 7.5 m, less and less to a quarter at 14 m and a quarter beyond (its
/// RadiusDamageScale); no light of its own.
pub(crate) const TRUCK_ROCKET: RoundKind = RoundKind {
    speed: 120.0,
    life_secs: 4.0,
    damage: 55.0,
    radius: 15.0,
    self_damage: SELF_DAMAGE,
    muzzle_fx: "MuzzleFlash_Blaster_FX",
    flight_fx: "Tracer_HomRocketVH_FX",
    explosion_fx: "Squib_HomRocket_01_FX",
    explosion_scale: 0.75,
    flight_sound: "BL_WPN_GUN_ROCKET.TRAIL_LP",
    explosion_sound: "BL_WPN_GUN_ROCKET.VEH_EXPL_IMPT_WORLD",
    light: None,
    falloff: Some(Falloff {
        full: 7.5,
        reach: 14.0,
        floor: 0.25,
    }),
    homing: None,
    gravity: 0.0,
    flight_scale: 1.0,
    bounce: None,
    follows_velocity: true,
    mesh: "",
    shake: None,
    sticky: None,
    grenade: None,
    clusters: None,
};

/// The Corrosive Slime Cannon's round (TnProjectileSlow: its MultiplayerData,
/// Slow_MP_PROJDATA, the only data it has; AoeSlow_PROJMESH, the
/// AoeSlow_WEPMESH's muzzle flash): 70 m/s, falling under the world's gravity
/// (GravityScale 1), 125 within 8 m, at full strength out to 4 m and less and less
/// to half at the edge (its RadiusDamageScale: a constant stretch to 400, a linear
/// one to 800, its max range's 0.5); its light (Radius 800, Brightness 8, white);
/// drawn at 1.5 times its size (the mesh's DrawScale), its flight effect with it;
/// its blast at DamageRadius / ExplosionEffectRadius (800 / 1000) of its size.
/// What it hurts it slows for 3 s (TnDamageTypeSlow's TnBuffAoeSlow: enemies only,
/// not the player's own side), which does nothing to the demo's sentries, as they
/// hover in place.
pub(crate) const SLIME_ROUND: RoundKind = RoundKind {
    speed: 70.0,
    life_secs: 4.0,
    damage: 125.0,
    radius: 8.0,
    self_damage: SELF_DAMAGE,
    muzzle_fx: "Corrosive_Slime_Cannon_MuzzleFlash_FX",
    flight_fx: "Corrosive_Slime_Cannon_Projectile_FX",
    explosion_fx: "SlimeCannon_Exp_FX",
    explosion_scale: 0.8,
    flight_sound: "BL_WPN_GUN_SLIME_CANNON.PROJ_FLIGHT_LP",
    explosion_sound: "BL_WPN_GUN_SLIME_CANNON.PROJ_IMPT_WORLD",
    light: Some(([255, 255, 255], 8.0, 8.0)),
    falloff: Some(Falloff {
        full: 4.0,
        reach: 8.0,
        floor: 0.5,
    }),
    homing: None,
    gravity: 1.0,
    flight_scale: 1.5,
    bounce: None,
    follows_velocity: true,
    mesh: "",
    shake: None,
    sticky: None,
    grenade: None,
    clusters: None,
};

/// The Gear Shredder's blades (TnProjectileSawblade: its Data, SawBlade_SP_PROJDATA,
/// which Escalation flies, as it has no SurvivalData: TnProjectile.GetProjectileData;
/// SawBlade_PROJMESH): 140 m/s in a straight line, for at most 20 s (its LifeSpan),
/// bouncing and sticking (SAW_BOUNCE); 165 to the enemy it sticks in (Damage: with
/// no DamageRadius, Projectile.Explode gives the struck enemy its damage alone) and
/// nothing to anything else; SawBlade_Trail_FX flying, with its light (Radius 350,
/// Brightness 8, white) and its flight loop, going off as Squib_PlasmaCannon_Med_FX
/// at a fifth of its size (ExplosionEffectScale) with its blast (ExplosionSound);
/// drawn at its own size. (It has no muzzle flash: SawBlade_WEPMESH shows a light.)
pub(crate) const SAW_BLADE: RoundKind = RoundKind {
    speed: 140.0,
    life_secs: 20.0,
    damage: 165.0,
    radius: 0.0,
    self_damage: 0.0,
    muzzle_fx: "",
    flight_fx: "SawBlade_Trail_FX",
    explosion_fx: "Squib_PlasmaCannon_Med_FX",
    explosion_scale: 0.2,
    flight_sound: "BL_WPN_GUN_SAW_BLADE.PROJECTILE_LP",
    explosion_sound: "BL_WPN_GUN_SAW_BLADE.PROJECTILE_IMPT",
    light: Some(([255, 255, 255], 8.0, 3.5)),
    falloff: None,
    homing: None,
    gravity: 0.0,
    flight_scale: 1.0,
    bounce: Some(&SAW_BOUNCE),
    follows_velocity: false,
    mesh: "",
    shake: None,
    sticky: None,
    grenade: None,
    clusters: None,
};

/// Its TnSawBladeProjectileExtension_5502: bouncing for 1.25 s, at 0.85 of its
/// speed each time, 10 times at most; homing once it has bounced on the enemy
/// nearest its line within 10 m (and 15 m of its line, all round it: 360 degrees)
/// at 1000 m/s/s; a cylinder 30 units round and 20 high; its path's colours
/// SawBlade_SP_PROJDATA's extension's (stored blue, green, red, alpha, as UE3's
/// FColor is): blue while it would miss (FlyingOnMiss 67, 130, 207; HomingOnMiss
/// 76, 57, 203), red where it would strike an enemy (FlyingOnHit 196, 103, 76;
/// HomingOnHit 196, 26, 34), the trail's colour scale tripling them (bright cyan
/// and red, as the game's look), over its 1 s.
const SAW_BOUNCE: Bounce = Bounce {
    secs: 1.25,
    keep: 0.85,
    most: 10,
    reach: 10.0,
    off_path: 15.0,
    cone: 360.0,
    turn: 1000.0,
    radius: 0.3,
    half_height: 0.1,
    sound: "BL_WPN_GUN_SAW_BLADE.PROJECTILE_BOUNCE",
    stuck_fx: "SawBlade_Stuck_FX",
    path_colors: [[67, 130, 207], [76, 57, 203], [196, 103, 76], [196, 26, 34]],
    path_secs: 1.0,
};

/// Where a blade's projectile goes off when it sticks in the level: its middle as
/// it touches, out along the surface's normal (Projectile.HitWall: Location +
/// HitNormal * 15, in centimetres).
const STUCK_LIFT: f32 = 0.15;

/// The Thermo Rocket Cannon's rocket (TnProjectileHomingRocket: its Data,
/// RocketLauncher_SP_PROJDATA, which Escalation flies, as it has no SurvivalData;
/// RocketLauncher_PROJMESH): 70 m/s (InitialSpeed and MaxSpeed 7000) in a straight
/// line, homing on the enemy it was fired locked on to (TnProjectileHoming's Homing:
/// pushed towards it at HomingForce, 400 m/s/s; within ClosingDistance, 20 m, at
/// ClosingForce, 600 m/s/s, going off where it is after ClosingTime, a second);
/// 375 within 7 m (Damage, DamageRadius; through walls: IgnoreHurtRadiusTrace), full
/// out to 2.5 m and half beyond (its RadiusDamageScale: two DST_Constant scales, to
/// 250 and 550 units, at 1 and 0.5, and its max range's 0.5); its light (Radius 750,
/// Brightness 2, white); Tracer_RocketLauncher_FX flying (the rocket's mesh, its
/// flame and smoke), going off as Squib_RocketLauncher_FX at DamageRadius /
/// ExplosionEffectRadius (700 / 650) of its size, with the rockets' loop and blast;
/// and the shake its blast gives the players near it (ROCKET_SHAKE).
pub(crate) const THERMO_ROCKET: RoundKind = RoundKind {
    speed: 70.0,
    life_secs: 4.0,
    damage: 375.0,
    radius: 7.0,
    self_damage: SELF_DAMAGE,
    muzzle_fx: "MuzzleFlash_RocketLauncher_FX",
    flight_fx: "Tracer_RocketLauncher_FX",
    explosion_fx: "Squib_RocketLauncher_FX",
    explosion_scale: 700.0 / 650.0,
    flight_sound: "BL_WPN_GUN_ROCKET.TRAIL_LP",
    explosion_sound: "BL_WPN_GUN_ROCKET.EXPL_IMPT_WORLD",
    light: Some(([255, 255, 255], 2.0, 7.5)),
    falloff: Some(Falloff {
        full: 2.5,
        reach: 2.5,
        floor: 0.5,
    }),
    homing: Some(Homing {
        force: 400.0,
        closing_distance: 20.0,
        closing_force: 600.0,
        closing_secs: 1.0,
    }),
    gravity: 0.0,
    flight_scale: 1.0,
    bounce: None,
    follows_velocity: true,
    mesh: "",
    shake: Some(&ROCKET_SHAKE),
    sticky: None,
    grenade: None,
    clusters: None,
};

/// RocketLauncher_SP_PROJDATA's TnCameraShakeProjectileExtension_707: out to 150 m,
/// half a second.
const ROCKET_SHAKE: ScreenShake = ScreenShake {
    radius: 150.0,
    secs: 0.5,
    rot_amp: Vec3::new(150.0, 100.0, 75.0),
    rot_freq: Vec3::new(100.0, 75.0, 55.0),
    loc_amp: Vec3::new(5.0, 0.0, 2.5),
    loc_freq: Vec3::new(10.0, 0.0, 5.0),
    fov_amp: 0.0,
    fov_freq: 0.0,
};

/// The A-4 Pulsar Cannon's round (TnProjectileGrenadeLauncher: its Data,
/// GrenadeLauncher_SP_PROJDATA, which Escalation flies, as it has no SurvivalData;
/// GrenadeLauncher_PROJMESH): 45 m/s (InitialSpeed 4500), all but straight (its
/// GravityScale, 0.001), until it sticks in what it strikes (PULSAR_STICK); 225
/// within 7.5 m (through walls), full out to 3.5 m and half beyond (its
/// RadiusDamageScale); Trail_PlasmaCannon_Med_FX flying and stuck (a ball of plasma,
/// at half its size: the mesh's DrawScale, 0.5), going off as Squib_Pulsar_Lrg_FX at
/// 750 / 650 of its size, with its own loop (PULSAR_CANNON_SHOOT_TAIL_LP) and blast;
/// the shake its blast gives (PULSAR_SHAKE). (Its light's colour is black: it lights
/// nothing.) It flies on until it strikes something (its LifeSpan is none until then:
/// here 20 s, after which it goes off where it is).
pub(crate) const PULSAR_ROUND: RoundKind = RoundKind {
    speed: 45.0,
    life_secs: 20.0,
    damage: 225.0,
    radius: 7.5,
    self_damage: SELF_DAMAGE,
    muzzle_fx: "Pulsar_MuzzleFlash_FX",
    flight_fx: "Trail_PlasmaCannon_Med_FX",
    explosion_fx: "Squib_Pulsar_Lrg_FX",
    explosion_scale: 750.0 / 650.0,
    flight_sound: "BL_WPN_GUN_PLASMA_CANNON.PULSAR_CANNON_SHOOT_TAIL_LP",
    explosion_sound: "BL_WPN_GUN_PLASMA_CANNON.PULSAR_CANNON_PROJ_EXPL",
    light: None,
    falloff: Some(Falloff {
        full: 3.5,
        reach: 3.5,
        floor: 0.5,
    }),
    homing: None,
    gravity: 0.001,
    flight_scale: 0.5,
    bounce: None,
    follows_velocity: false,
    mesh: "",
    shake: Some(&PULSAR_SHAKE),
    sticky: Some(&PULSAR_STICK),
    grenade: None,
    clusters: None,
};

/// Its FuseTime (2 s: its Max, and its Min the class's default, 2), the launcher's
/// NumberOfActiveGrenades (3: TnWeaponDataGrenadeLauncher's default) and its
/// FuseSound.
const PULSAR_STICK: Sticky = Sticky {
    fuse: 2.0,
    most: 3,
    sound: "BL_WPN_GUN_PLASMA_CANNON.PULSAR_CANNON_PROJ_IMPT_STICK",
};

/// GrenadeLauncher_SP_PROJDATA's TnCameraShakeProjectileExtension_6001: out to 300 m,
/// for a second (the extension's default TimeDuration).
const PULSAR_SHAKE: ScreenShake = ScreenShake {
    radius: 300.0,
    secs: 1.0,
    rot_amp: Vec3::new(125.0, 100.0, 125.0),
    rot_freq: Vec3::new(75.0, 50.0, 25.0),
    loc_amp: Vec3::new(10.0, 0.0, 5.0),
    loc_freq: Vec3::new(15.0, 0.0, 10.0),
    fov_amp: 0.0,
    fov_freq: 0.0,
};

/// The Chaos-Rift Combustor's bomb (TnProjectileClusterCannon: its SurvivalData,
/// ClusterCannon_SV_PROJDATA; ClusterCannon_PROJMESH): lobbed at what the crosshair is
/// on (main.rs's toss: its weapon sets its velocity), falling under the world's
/// gravity (GravityScale 1, PHYS_Falling: it goes off on whatever it lands on or
/// strikes), its mesh (PROJMESH_GrenadeBall_STAT, a ball) turning as it flies; 150
/// within 4.5 m, full throughout (its RadiusDamageScale has no scales), through
/// walls; ClusterCannon_Tracer_FX flying, with its light (Radius 1500, Brightness 8,
/// white) and the tank shell's loop, going off as ClusterCannon_ExplosionPrimary_FX at
/// 450 / 375 of its size with the tank shell's blast, and bursting into its clusters
/// (COMBUSTOR_CLUSTERS) however it goes off: on what it strikes, or in the air as its
/// shooter pulls the trigger again (TnWeaponTankCannonArcable.RequestClusterBomb).
pub(crate) const COMBUSTOR_SHELL: RoundKind = RoundKind {
    speed: 60.0,
    life_secs: 4.0,
    damage: 150.0,
    radius: 4.5,
    self_damage: SELF_DAMAGE,
    muzzle_fx: "ClusterCannon_MuzzleFlash_FX",
    flight_fx: "ClusterCannon_Tracer_FX",
    explosion_fx: "ClusterCannon_ExplosionPrimary_FX",
    explosion_scale: 450.0 / 375.0,
    flight_sound: "BL_WPN_GUN_TANK_CANNON.TRAIL_LP",
    explosion_sound: "BL_WPN_GUN_TANK_CANNON.EXPL_IMPT_WORLD",
    light: Some(([255, 255, 255], 8.0, 15.0)),
    falloff: None,
    homing: None,
    gravity: 1.0,
    flight_scale: 1.0,
    bounce: None,
    follows_velocity: true,
    mesh: "ClusterCannon_Bomb_FX",
    shake: None,
    sticky: None,
    grenade: None,
    clusters: Some(&COMBUSTOR_CLUSTERS),
};

/// ClusterCannon_SV_PROJDATA's TnClusterBombProjectileExtension_7326: five clusters,
/// within 65 degrees of its normal, at its velocity plus 15 to 25 m/s.
const COMBUSTOR_CLUSTERS: Clusters = Clusters {
    count: 5,
    cone: 65.0,
    transfer: 1.0,
    speed: (15.0, 25.0),
    bomb: &COMBUSTOR_CLUSTER,
};

/// A Chaos-Rift Combustor's cluster (TnProjectileClusterCannonBomb: its Data,
/// Clusters_SP_PROJDATA, which Escalation flies; Clusters_PROJMESH): a grenade
/// (CLUSTER_BOUNCE) falling at 1.25 times the world's gravity (GravityScale), going off
/// on the first enemy or player it strikes or as its fuse runs out; 150 within 8.5 m,
/// full throughout, through walls (in Escalation every cluster hurts every enemy in
/// reach: ShouldDamageActor's one cluster an enemy is for team games); its mesh
/// (PROJMESH_Grenade_STAT, a canister) and ClusterCannon_Tracer_FX, both at 1.5 times
/// their size (the mesh's DrawScale), with its own loop, bounce, fuse and blast
/// (ClusterCannon_ExplosionSecondary_FX at 850 / 375 of its size); the shake its blast
/// gives (CLUSTER_SHAKE). Until its fuse is lit it has no LifeSpan (here 30 s).
pub(crate) const COMBUSTOR_CLUSTER: RoundKind = RoundKind {
    speed: 100.0,
    life_secs: 30.0,
    damage: 150.0,
    radius: 8.5,
    self_damage: SELF_DAMAGE,
    muzzle_fx: "",
    flight_fx: "ClusterCannon_Tracer_FX",
    explosion_fx: "ClusterCannon_ExplosionSecondary_FX",
    explosion_scale: 850.0 / 375.0,
    flight_sound: "BL_WPN_GUN_TANK_CANNON.CLUSTER_BOMB_PROJ_LP",
    explosion_sound: "BL_WPN_GUN_TANK_CANNON.CLUSTER_BOMB_PROJ_EXPL",
    light: None,
    falloff: None,
    homing: None,
    gravity: 1.25,
    flight_scale: 1.5,
    bounce: None,
    follows_velocity: false,
    mesh: "ClusterCannon_Cluster_FX",
    shake: Some(&CLUSTER_SHAKE),
    sticky: None,
    grenade: Some(&CLUSTER_BOUNCE),
    clusters: None,
};

/// Clusters_SP_PROJDATA: BounceDampening 0.55, FuseTime 0.5 to 1.75 s,
/// ExplodeWhenHittingPawn; Clusters_PROJMESH's BounceSound and FuseSound.
const CLUSTER_BOUNCE: Grenade = Grenade {
    keep: 0.55,
    fuse: (0.5, 1.75),
    explode_on_pawn: true,
    sound_bounce: "BL_WPN_GRENADE.GRENADE_FOLEY_SHELL_BOUNCE_HEAVY",
    sound_fuse: "BL_WPN_GUN_TANK_CANNON.CLUSTER_BOMB_FUSE",
};

/// Clusters_SP_PROJDATA's TnCameraShakeProjectileExtension_13772: out to 150 m, for
/// 0.75 s, with a slight widening of the view (FOVAmplitude -1 at 0.1).
const CLUSTER_SHAKE: ScreenShake = ScreenShake {
    radius: 150.0,
    secs: 0.75,
    rot_amp: Vec3::new(75.0, 50.0, 25.0),
    rot_freq: Vec3::new(45.0, 35.0, 25.0),
    loc_amp: Vec3::new(5.0, 0.0, 5.0),
    loc_freq: Vec3::new(10.0, 0.0, 5.0),
    fov_amp: -1.0,
    fov_freq: 0.1,
};

/// A grenade's LowSpeedThreshold (m/s) and the time its speed is averaged over (its
/// _DeltaPostionHistory's SampleTime).
const REST_SPEED: f32 = 1.0;
const REST_SAMPLE_SECS: f32 = 1.0;
/// Bounces slower than this make no sound (the demo's: a grenade settling would
/// tick away on the floor; as the EMP grenade's).
const BOUNCE_SOUND_SPEED: f32 = 2.0;
/// How far short of a surface a grenade stops (its collision cylinder's size).
const GRENADE_RADIUS: f32 = 0.1;

/// The world's gravity (WorldInfo's DefaultGravityZ, -2940 cm/s/s), which pulls a
/// round by its GravityScale.
const WORLD_GRAVITY: f32 = -29.4;

/// A round's simulated flight (TnProjectile.RunPhysicsSimulation, for its weapon's
/// TnSimulateProjectileExtension): a place every SimulationTimeStep (0.05 s:
/// TnProjectileData's default) for at most SimulationMaxTime (3 s), until it would
/// strike something.
pub(crate) const SIMULATION_STEP: f32 = 0.05;
const SIMULATION_SECS: f32 = 3.0;

/// The path a round of `kind` would take fired from `origin` along `dir` (unit
/// length): its places SIMULATION_STEP apart, ending where it would strike the level,
/// a sentry (`sentries`' middles) or a Jazz other than `shooter` (`bodies`).
pub(crate) fn simulate_round(
    kind: &RoundKind,
    (level, colliders): (&Level, &Colliders),
    (sentries, bodies): (&[Target], &Bodies),
    shooter: Option<Entity>,
    origin: Vec3,
    dir: Vec3,
) -> Vec<Vec3> {
    let velocity = dir.normalize_or(Vec3::NEG_Z) * kind.speed;
    let secs = SIMULATION_SECS.min(kind.life_secs);
    let world = (level, colliders);
    simulate_flight(
        kind,
        world,
        (sentries, bodies),
        shooter,
        (origin, velocity),
        secs,
    )
    .0
}

/// The path a round of `kind` would take leaving `origin` at `velocity` (as
/// simulate_round: TnProjectile.RunPhysicsSimulation, a place every SimulationTimeStep
/// for `secs` at most, until it would strike something), and the normal of the
/// surface it would strike there (LastHitWallNormal), if it would.
pub(crate) fn simulate_flight(
    kind: &RoundKind,
    (level, colliders): (&Level, &Colliders),
    (sentries, bodies): (&[Target], &Bodies),
    shooter: Option<Entity>,
    (origin, velocity): (Vec3, Vec3),
    secs: f32,
) -> (Vec<Vec3>, Option<Vec3>) {
    let mut at = origin;
    let mut velocity = velocity;
    let mut path = vec![at];
    let mut t = 0.0;
    while t < secs - 1e-4 {
        t += SIMULATION_STEP;
        velocity.y += WORLD_GRAVITY * kind.gravity * SIMULATION_STEP;
        let motion = velocity * SIMULATION_STEP;
        if let Some((f, normal, _)) =
            strike(level, colliders, (sentries, bodies), shooter, at, motion)
        {
            path.push(at + motion * f);
            return (path, Some(normal));
        }
        at += motion;
        path.push(at);
    }
    (path, None)
}

/// The velocity that carries a round from `from` to `to` in `secs` under `gravity`
/// times the world's (Core's Math.CalculateTossVelocity: straight across at the
/// distance over the time, and up at what makes it fall to there by then).
pub(crate) fn toss_velocity(from: Vec3, to: Vec3, secs: f32, gravity: f32) -> Vec3 {
    let d = to - from;
    let t = secs.max(1e-3);
    let mut v = d.normalize_or_zero() * (d.length() / t);
    v.y = (d.y - 0.5 * WORLD_GRAVITY * gravity * t * t) / t;
    v
}

/// The effects' EnergonColor: a team's own effects are cyan in a match (as the EMP
/// grenade's, emp.rs).
const ENERGON: &[(&str, [u8; 4])] = &[("EnergonColor", [0, 255, 255, 255])];
/// A round's flight and explosion effects' parameters (its projectile mesh's
/// Parameters, set on them by HmProjectile.OnParticleSystemCreated): its
/// TnParticleSystemParameterEnergonColor (every round's here), whose ParameterName is
/// Color, the team's energon colour, cyan; and the TnParticleSystemParameterTeamIndex
/// of the newer ones (the Gear Shredder's, the Destroyer's), the module channel of the
/// shooter's side (levelfx::MODULE_CHANNEL: A for the Autobots, B for the
/// Decepticons), which their High Moon colour modules switch on.
fn round_params(channel: [u8; 4]) -> [(&'static str, [u8; 4]); 2] {
    [("Color", [0, 255, 255, 255]), (MODULE_CHANNEL, channel)]
}

/// The module channel of a side's effects (A the Autobots', B the Decepticons').
fn channel_of(decepticon: bool) -> [u8; 4] {
    if decepticon {
        MODULE_CHANNEL_B
    } else {
        MODULE_CHANNEL_A
    }
}

/// Who fired a round: the Jazz on this PC who fired it (its blast hurts him too and
/// his HUD shows its hits; None for another PC's round), the Jazz who fired it on any
/// PC (it passes through him, and his weapon sets it off), and whether his side is the
/// Decepticons (its effects' module channel).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Firer {
    pub owner: Option<Entity>,
    pub shooter: Option<Entity>,
    pub decepticon: bool,
    /// What the upgrades on the weapon that fired it do to it.
    pub mods: RoundMods,
}

/// What a weapon's upgrades do to its rounds (upgrades.rs: its rounds' buffs read
/// through their _FiringWeapon): their speed (TnProjectile.Init: InitialSpeed and
/// MaxSpeed times TnBuffWeaponProjectileSpeed's), their blast's reach and its
/// effect's size with it (GetDamageRadiusModifier, which GetDamageRadius and so
/// SpawnExplosionEffect's scale go by), the clusters added to a burst
/// (TnBuffWeaponNumClusters) and their slow's time (TnBuffIncreaseAoeSlowDuration).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RoundMods {
    pub speed: f32,
    pub radius: f32,
    pub clusters: u32,
    pub slow: f32,
}

impl RoundMods {
    pub const NONE: RoundMods = RoundMods {
        speed: 1.0,
        radius: 1.0,
        clusters: 0,
        slow: 1.0,
    };

    pub fn of(m: &crate::upgrades::Mods) -> RoundMods {
        RoundMods {
            speed: m.projectile_speed,
            radius: m.projectile_radius,
            clusters: m.clusters,
            slow: m.slow,
        }
    }
}

impl Default for RoundMods {
    fn default() -> Self {
        RoundMods::NONE
    }
}

/// The order rounds are fired in (Round::seq).
static NEXT_ROUND: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// A round's random draws, the same on every PC for the same shot: from where it
/// left and its way.
pub(crate) fn seed_of(origin: Vec3, dir: Vec3) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for v in [origin.x, origin.y, origin.z, dir.x, dir.y, dir.z] {
        h = (h ^ v.to_bits()).wrapping_mul(0x0100_0193);
        h ^= h >> 15;
    }
    h
}

/// The next of a seed's random numbers, from 0 to 1.
fn draw(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed >> 8) as f32 / (1u32 << 24) as f32
}

/// A way at random within `cone` radians of unit `dir` (Core's
/// Random.RandomVectorInCone: off it by an angle at random up to the cone's, round it
/// by one at random all the way).
fn random_in_cone(dir: Vec3, cone: f32, seed: &mut u32) -> Vec3 {
    let phi = draw(seed) * cone;
    let theta = draw(seed) * std::f32::consts::TAU;
    let side = if dir.y.abs() < 0.99 { Vec3::Y } else { Vec3::X };
    let a = dir.cross(side).normalize_or(Vec3::X);
    let b = dir.cross(a);
    (dir * phi.cos() + (a * theta.cos() + b * theta.sin()) * phi.sin()).normalize_or(dir)
}

/// The blast is centred this far out of the surface struck (Projectile.Explode's
/// HitLocation + HitNormal * 0.02, in centimetres).
const BLAST_LIFT: f32 = 0.0002;
/// A Jazz's body against the blast, as the sentries' shots see him (sentry_fire):
/// an upright cylinder, its radius and height.
const ROBOT_BODY: (f32, f32) = (1.0, 3.4);
const CAR_BODY: (f32, f32) = (1.3, 1.15);

/// A Jazz's body (ROBOT_BODY or CAR_BODY, by his form).
pub(crate) fn body_of(player: &Player) -> (f32, f32) {
    match player.form {
        Form::Robot => ROBOT_BODY,
        Form::Vehicle => CAR_BODY,
    }
}

pub struct RiotPlugin;

impl Plugin for RiotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(crate::menu::AppState::Game), clear_rounds)
            .add_systems(
                Update,
                (
                    fly_rounds,
                    stuck_blades,
                    follow_muzzle_flashes,
                    follow_cannon_flashes,
                    follow_vehicle_flashes,
                )
                    .after(crate::fire)
                    .before(crate::health_stations)
                    .run_if(in_state(crate::menu::AppState::Game))
                    .before(crate::hud::HudSystems),
            );
    }
}

/// A round in flight.
#[derive(Component)]
pub(crate) struct Round {
    kind: &'static RoundKind,
    velocity: Vec3,
    age: f32,
    flight_fx: Option<FxId>,
    /// The Jazz on this PC who fired it (its blast hurts him too); None for another
    /// PC's.
    owner: Option<Entity>,
    /// The sentry a homing round homes on, and the seconds it has been closing on it.
    target: Option<Entity>,
    closing: Option<f32>,
    /// A bouncing round's flight (RoundKind::bounce).
    blade: Option<Blade>,
    /// The Jazz who fired it, on any PC (Firer::shooter), and its effects' module
    /// channel (his side's).
    shooter: Option<Entity>,
    channel: [u8; 4],
    /// Its place in the order rounds are fired (a launcher's oldest goes first).
    seq: u64,
    /// The way it faces (RoundKind::follows_velocity: the way it flies, or the way it
    /// was fired), and its mesh's effect (RoundKind::mesh).
    facing: Vec3,
    mesh_fx: Option<FxId>,
    /// A sticky round's or a grenade's fuse, once lit: seconds left.
    fuse: Option<f32>,
    /// What a sticky round is stuck in.
    stuck: Option<StuckIn>,
    /// A grenade at rest (PHYS_None), and its speeds over the last second (age,
    /// metres a second).
    resting: bool,
    speeds: Vec<(f32, f32)>,
    /// Its shooter has set it off (his launcher's alt fire, a Combustor's trigger:
    /// Player::set_off), or fired one too many after it (Sticky::most).
    set_off: bool,
    /// Its random draws (a grenade's fuse, its clusters' ways and speeds).
    seed: u32,
    /// What its weapon's upgrades do to it (Firer::mods).
    mods: RoundMods,
    /// A grenade's warning to the enemies near has gone out
    /// (_PawnsNotifiedOfGrenade: crate::ai::NearGrenades).
    warned: bool,
}

/// What a sticky round is stuck in (TnProjectileGrenadeLauncher.AttachToActor): the
/// level, where it touched; an enemy or another Jazz, on the bone of its nearest
/// where it struck (HitMesh.FindClosestBone, SetBase on it with no offset: it rides
/// the bone as it moves, animates and falls), or, while the bones aren't found yet,
/// on the enemy's middle or the point of his body's axis this far over his feet.
#[derive(Clone, Copy, Debug)]
enum StuckIn {
    Level,
    Sentry(Entity),
    Jazz(Entity, f32),
    Bone(Entity, Entity),
    /// An enemy's bone: it stays on it while its body is there, dead or alive.
    EnemyBone(Entity, Entity),
}

/// Each Jazz's bones where they are this frame: the bone, whose it is, and where
/// (JointOf).
pub(crate) type Joints = [(Entity, Entity, Vec3)];

/// The bone of a Jazz's nearest `at` (FindClosestBone), if his model has any.
fn nearest_bone(joints: &Joints, jazz: Entity, at: Vec3) -> Option<Entity> {
    joints
        .iter()
        .filter(|j| j.1 == jazz)
        .min_by(|a, b| {
            a.2.distance_squared(at)
                .total_cmp(&b.2.distance_squared(at))
        })
        .map(|j| j.0)
}

/// A bouncing round in flight: how it flies (BladeFlight), the way it was fired
/// (its Rotation, which it keeps, bRotationFollowsVelocity being off: its flight
/// effect and its stuck mesh are turned by it), the Jazz who fired it (on any PC:
/// it passes through him) and his side's module channel for its effects
/// (levelfx::MODULE_CHANNEL).
struct Blade {
    flight: BladeFlight,
    facing: Vec3,
    shooter: Option<Entity>,
    channel: [u8; 4],
}

/// A bouncing round's state as it flies (TnProjectileSawblade's): where it is, its
/// velocity, its age (_TimeSinceCreation), its bounces (_NumBounces, _HasBounced),
/// whether it is past bouncing (its eleventh: _TimeSinceCreation set to the
/// largest float), and the enemy it homes on (_HomingTarget).
#[derive(Clone, Copy, Debug)]
pub(crate) struct BladeFlight {
    pub at: Vec3,
    pub velocity: Vec3,
    pub age: f32,
    pub bounces: u32,
    pub bounced: bool,
    pub spent: bool,
    pub target: Option<Entity>,
}

impl BladeFlight {
    pub(crate) fn new(at: Vec3, dir: Vec3, speed: f32) -> BladeFlight {
        BladeFlight {
            at,
            velocity: dir.normalize_or(Vec3::NEG_Z) * speed,
            age: 0.0,
            bounces: 0,
            bounced: false,
            spent: false,
            target: None,
        }
    }
}

/// What befell a blade in a step of its flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum BladeEvent {
    Flew,
    /// It bounced off the level or a Jazz of the players' side.
    Bounced,
    /// It went off and stuck: where (its explosion's place and the surface's normal)
    /// and in which enemy, if one.
    Stuck {
        at: Vec3,
        normal: Vec3,
        sentry: Option<Entity>,
    },
}

/// What a blade's cylinder strikes first moving from `from` by `motion`.
enum BladeHit {
    Level {
        t: f32,
        normal: Vec3,
    },
    Sentry {
        t: f32,
        entity: Entity,
        normal: Vec3,
    },
    Jazz {
        t: f32,
        normal: Vec3,
    },
}

/// The players a blade can strike: each one's entity, feet and body (an upright
/// cylinder's radius and height).
pub(crate) type Bodies = [(Entity, Vec3, (f32, f32))];

/// What a blade (`b`'s cylinder) moving from `from` by `motion` strikes first: the
/// level (its middle's line against the level, the surface held off by its edge),
/// a live sentry (`sentries`' middles, their spheres grown by its radius) or a Jazz
/// of `jazzes` other than `shooter` (his cylinder grown by its size), as it comes
/// on to them.
fn blade_hit(
    b: &Bounce,
    level: &Level,
    colliders: &Colliders,
    sentries: &[Target],
    jazzes: &Bodies,
    shooter: Option<Entity>,
    (from, motion): (Vec3, Vec3),
) -> Option<BladeHit> {
    let len = motion.length();
    if len < 1e-6 {
        return None;
    }
    let dir = motion / len;
    let mut best: Option<BladeHit> = None;
    let t_of = |h: &BladeHit| match h {
        BladeHit::Level { t, .. } | BladeHit::Sentry { t, .. } | BladeHit::Jazz { t, .. } => *t,
    };
    if let Some((t, n)) = level.shot_surface(colliders, from, dir, len) {
        let n = if n.dot(dir) > 0.0 { -n } else { n };
        best = Some(BladeHit::Level { t, normal: n });
    }
    for target in sentries {
        let (entity, c) = (target.entity, target.at);
        if let Some(t) = target.body.ray_wide(from, dir, len, b.radius)
            && best.as_ref().is_none_or(|h| t < t_of(h))
        {
            let normal = (from + dir * t - c).normalize_or(-dir);
            if normal.dot(dir) < 0.0 || t > 0.0 {
                best = Some(BladeHit::Sentry { t, entity, normal });
            }
        }
    }
    for &(entity, feet, (radius, height)) in jazzes {
        if Some(entity) == shooter {
            continue;
        }
        let r = radius + b.radius;
        let Some(t) = crate::ray_cylinder(
            from,
            dir,
            Vec2::new(feet.x, feet.z),
            r,
            feet.y - b.half_height,
            feet.y + height + b.half_height,
        ) else {
            continue;
        };
        if t > len || best.as_ref().is_some_and(|h| t >= t_of(h)) {
            continue;
        }
        let at = from + dir * t;
        let flat = Vec2::new(at.x - feet.x, at.z - feet.z);
        let normal = if flat.length() >= r - 1e-3 {
            Vec3::new(flat.x, 0.0, flat.y).normalize_or(-dir)
        } else if at.y > feet.y + height * 0.5 {
            Vec3::Y
        } else {
            Vec3::NEG_Y
        };
        // (Inside him and on the way out, as after a bounce: nothing.)
        if normal.dot(dir) < 0.0 {
            best = Some(BladeHit::Jazz { t, normal });
        }
    }
    best
}

/// How far a blade's cylinder reaches out along unit `normal` from its middle
/// (Bevy's axes, Y up): how far off a surface its middle is as it touches it.
fn blade_reach(b: &Bounce, normal: Vec3) -> f32 {
    let up = normal.y.abs().min(1.0);
    b.radius * (1.0 - up * up).sqrt() + b.half_height * up
}

/// The enemy a homing blade at `at` flying along `velocity` would take
/// (FindTargetInCone): the live one nearest its line, within its reach, its cone
/// and its distance from the line (from the enemy's body: AI.CalculateRangeToPoint,
/// a sentry's body its sphere).
fn blade_target(b: &Bounce, at: Vec3, velocity: Vec3, sentries: &[Target]) -> Option<Entity> {
    if b.cone <= 0.0 || b.reach <= 0.0 || b.off_path <= 0.0 {
        return None;
    }
    let dir = velocity.normalize_or_zero();
    let cos = (0.5 * b.cone).to_radians().cos();
    let mut best = None;
    let mut best_off = b.off_path;
    for target in sentries {
        let (entity, c) = (target.entity, target.at);
        let to = c - at;
        let distance = to.length();
        if distance > b.reach {
            continue;
        }
        if dir.dot(to.normalize_or_zero()) < cos {
            continue;
        }
        let closest = at + dir * to.dot(dir);
        let off = target.body.reach(closest);
        if off > best_off {
            continue;
        }
        best = Some(entity);
        best_off = off;
    }
    best
}

/// Turns a blade's `velocity` towards `target` (HomeTowardsTarget): the velocity
/// straight at it, at its speed, reached by a change of at most `most` (none if
/// None).
fn home_towards(velocity: Vec3, at: Vec3, target: Vec3, most: Option<f32>) -> Vec3 {
    let wanted = (target - at).normalize_or_zero() * velocity.length();
    let change = wanted - velocity;
    velocity
        + match most {
            Some(m) => change.clamp_length_max(m.max(0.0)),
            None => change,
        }
}

/// Keeps a homing blade's target while it is ahead of it, else looks for another
/// with `find` (UpdateHomingTarget); the target's middle, if it has one.
fn keep_target(
    flight: &mut BladeFlight,
    sentries: &[Target],
    find: impl FnOnce(&BladeFlight) -> Option<Entity>,
) -> Option<Vec3> {
    let ahead = flight
        .target
        .and_then(|t| sentries.iter().find(|s| s.entity == t))
        .filter(|s| (s.at - flight.at).dot(flight.velocity) >= 0.0)
        .map(|s| s.at);
    if ahead.is_some() {
        return ahead;
    }
    flight.target = find(flight);
    flight
        .target
        .and_then(|t| sentries.iter().find(|s| s.entity == t))
        .map(|s| s.at)
}

/// A step of `dt` of a blade's flight (TnProjectileSawblade.Tick: its age, its
/// homing once it has bounced, then its move, PHYS_Projectile with bBounce: a
/// bounce's move goes on with what is left of the step), striking the level, the
/// sentries and the players as blade_hit finds them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn blade_step(
    b: &Bounce,
    flight: &mut BladeFlight,
    dt: f32,
    (level, colliders): (&Level, &Colliders),
    sentries: &[Target],
    jazzes: &Bodies,
    shooter: Option<Entity>,
) -> BladeEvent {
    flight.age += dt;
    if flight.bounced {
        let target = keep_target(flight, sentries, |f| {
            blade_target(b, f.at, f.velocity, sentries)
        });
        if let Some(c) = target {
            flight.velocity = home_towards(flight.velocity, flight.at, c, Some(b.turn * dt));
        }
    }
    let mut event = BladeEvent::Flew;
    let mut left = dt;
    for _ in 0..4 {
        let motion = flight.velocity * left;
        let Some(hit) = blade_hit(
            b,
            level,
            colliders,
            sentries,
            jazzes,
            shooter,
            (flight.at, motion),
        ) else {
            flight.at += motion;
            return event;
        };
        let len = motion.length().max(1e-6);
        let dir = motion / len;
        match hit {
            BladeHit::Sentry { t, entity, normal } => {
                flight.at += dir * t;
                flight.velocity = Vec3::ZERO;
                return BladeEvent::Stuck {
                    at: flight.at,
                    normal,
                    sentry: Some(entity),
                };
            }
            BladeHit::Level { t, normal } => {
                let touch = flight.at + dir * t + normal * blade_reach(b, normal);
                if flight.age >= b.secs || flight.spent {
                    flight.at = touch;
                    flight.velocity = Vec3::ZERO;
                    return BladeEvent::Stuck {
                        at: touch + normal * STUCK_LIFT,
                        normal,
                        sentry: None,
                    };
                }
                if !bounce_off(b, flight, sentries, touch, normal) {
                    // (Its eleventh: no bounce. It goes on into the surface, which it
                    // strikes again past bouncing.)
                    continue;
                }
                left *= 1.0 - t / len;
                event = BladeEvent::Bounced;
            }
            BladeHit::Jazz { t, normal } => {
                let touch = flight.at + dir * t;
                if flight.spent || !bounce_off(b, flight, sentries, touch, normal) {
                    // (Past bouncing it passes through the players' side.)
                    flight.at += motion;
                    return event;
                }
                left *= 1.0 - t / len;
                event = BladeEvent::Bounced;
            }
        }
    }
    event
}

/// A blade's bounce off a surface of unit `normal` as it touches it at `touch`
/// (TnProjectileSawblade.Bounce): there, its velocity mirrored in the surface at
/// BounceSpeedPercent of its speed, its target dropped unless it is still ahead
/// (then it turns straight at it). False for its eleventh, which it doesn't make
/// (it is past bouncing from then on).
fn bounce_off(
    b: &Bounce,
    flight: &mut BladeFlight,
    sentries: &[Target],
    touch: Vec3,
    normal: Vec3,
) -> bool {
    flight.bounced = true;
    flight.bounces += 1;
    if flight.bounces > b.most {
        flight.spent = true;
        return false;
    }
    flight.at = touch;
    let v = flight.velocity;
    flight.velocity = (v - normal * 2.0 * v.dot(normal)) * b.keep;
    if let Some(c) = keep_target(flight, sentries, |_| None) {
        flight.velocity = home_towards(flight.velocity, flight.at, c, None);
    }
    true
}

/// The path a blade of `kind` fired from `origin` along `dir` would take, as the
/// path fine aim shows (TnWeaponSawblade.SimulateSingleTrajectory: TnProjectile.
/// RunPhysicsSimulation for its SimulationMaxTime, a place every SimulationTimeStep,
/// TnProjectileSawblade's OnSimulationStepped homing it), bouncing and homing as it
/// would, ending where it would stick in an enemy: its places, the colour of each
/// (GetTrajectoryColor, by whether the path shown before struck an enemy, `struck_before`,
/// and whether it is homing there), and whether it struck one.
#[allow(clippy::too_many_arguments)]
pub(crate) fn simulate_blade(
    kind: &RoundKind,
    world: (&Level, &Colliders),
    sentries: &[Target],
    jazzes: &Bodies,
    shooter: Option<Entity>,
    (origin, dir): (Vec3, Vec3),
    struck_before: bool,
) -> (Vec<Vec3>, Vec<[u8; 4]>, bool) {
    let Some(b) = kind.bounce else {
        return (vec![origin], vec![[0, 0, 0, 255]], false);
    };
    let mut flight = BladeFlight::new(origin, dir, kind.speed);
    let color = |f: &BladeFlight| {
        let homing = f.target.is_some() as usize;
        let [r, g, bl] = b.path_colors[struck_before as usize * 2 + homing];
        let a = 255.0 - 255.0 * (f.age / b.path_secs.max(1e-3)).clamp(0.0, 1.0);
        [r, g, bl, a as u8]
    };
    let mut path = vec![origin];
    let mut colors = vec![color(&flight)];
    let mut struck = false;
    let mut stopped = false;
    let mut t = 0.0;
    while t < b.path_secs - 1e-4 {
        t += SIMULATION_STEP;
        if !stopped
            && let BladeEvent::Stuck { sentry, .. } = blade_step(
                b,
                &mut flight,
                SIMULATION_STEP,
                world,
                sentries,
                jazzes,
                shooter,
            )
        {
            // (Stuck, it stops: the rest of the simulation leaves it there.)
            stopped = true;
            struck = sentry.is_some();
        } else if stopped {
            flight.age += SIMULATION_STEP;
        }
        path.push(flight.at);
        colors.push(color(&flight));
    }
    (path, colors, struck)
}

/// A muzzle flash that stays on the gun's muzzle while it plays (the game attaches
/// it to the weapon's MuzzleFlash socket): the Jazz whose gun it is, and the gun
/// (WEAPONS). It is drawn at his robot's size, as the game draws an effect on a gun
/// (the gun is drawn at its robot's size, the effect attached to it at its own:
/// TnWeaponEffectPlayFXOnSocket.CalculateRelativeScale undoes only a scale over 1),
/// so 0.8 of it on the Destroyer's Riot Cannon, 0.9 on the Titan's Corrosive Slime
/// Cannon (arsenal.rs's GunFx::robot_sized, for the flashes on the guns' sockets).
#[derive(Component)]
pub(crate) struct MuzzleFlash {
    /// Its effect, once started (follow_muzzle_flashes starts it, where it knows
    /// his robot).
    fx: Option<FxId>,
    /// The effect to start, where and which way (the muzzle and the barrel as it was
    /// fired), till it is started.
    start: Option<(&'static str, Vec3, Vec3)>,
    jazz: Entity,
    weapon: usize,
    /// Seconds since it started.
    age: f32,
}

/// A tank's muzzle flash, kept on the end of its barrel while it plays (the game
/// attaches it to the TankCannon's muzzle socket): the Jazz whose tank it is.
#[derive(Component)]
pub(crate) struct CannonFlash {
    fx: FxId,
    jazz: Entity,
}

/// A jet's or a truck's muzzle flash, kept on the muzzle it fired from while it
/// plays: the Jazz whose vehicle it is, and which of its muzzles (Vehicle::muzzles;
/// for another player's, the one nearest where it was fired, `from`).
#[derive(Component)]
pub(crate) struct VehicleFlash {
    fx: FxId,
    jazz: Entity,
    muzzle: Option<usize>,
    from: Vec3,
}

/// Fires a round of `kind` from `origin` along `dir` (unit length), fired by `firer`.
pub(crate) fn fire_round(
    commands: &mut Commands,
    fx: &mut FxSystems,
    kind: &'static RoundKind,
    origin: Vec3,
    dir: Vec3,
    firer: Firer,
) {
    fire_homing_round(commands, fx, kind, origin, dir, firer, None);
}

/// Fires a round as fire_round does, homing on `target` if its kind homes.
pub(crate) fn fire_homing_round(
    commands: &mut Commands,
    fx: &mut FxSystems,
    kind: &'static RoundKind,
    origin: Vec3,
    dir: Vec3,
    firer: Firer,
    target: Option<Entity>,
) {
    let dir = dir.normalize_or(Vec3::NEG_Z);
    let seed = seed_of(origin, dir);
    launch(
        commands,
        fx,
        kind,
        (origin, dir * kind.speed * firer.mods.speed, dir),
        firer,
        target,
        seed,
    );
}

/// Sends a round of `kind` from `origin` at `velocity`, facing `facing` (unit
/// length: the way it was fired), fired by `firer`, homing on `target` if its kind
/// homes, its random draws from `seed`.
pub(crate) fn launch(
    commands: &mut Commands,
    fx: &mut FxSystems,
    kind: &'static RoundKind,
    (origin, velocity, facing): (Vec3, Vec3, Vec3),
    firer: Firer,
    target: Option<Entity>,
    seed: u32,
) {
    let facing = facing.normalize_or(Vec3::NEG_Z);
    let channel = channel_of(firer.decepticon);
    let params = round_params(channel);
    let flight_fx = fx.spawn_scaled(kind.flight_fx, origin, facing, kind.flight_scale, &params);
    let mesh_fx = if kind.mesh.is_empty() {
        None
    } else {
        fx.spawn_scaled(kind.mesh, origin, facing, kind.flight_scale, &params)
    };
    let mut round = commands.spawn((
        Round {
            kind,
            velocity,
            age: 0.0,
            flight_fx,
            owner: firer.owner,
            target: target.filter(|_| kind.homing.is_some()),
            closing: None,
            blade: None,
            shooter: firer.shooter,
            channel,
            seq: NEXT_ROUND.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            facing,
            mesh_fx,
            fuse: None,
            stuck: None,
            resting: false,
            speeds: Vec::new(),
            set_off: false,
            seed: seed | 1,
            mods: firer.mods,
            warned: false,
        },
        Transform::from_translation(origin),
        Visibility::default(),
    ));
    if let Some(([r, g, b], brightness, radius)) = kind.light {
        round.with_child((
            crate::graphics::LiveLight,
            PointLight {
                color: Color::srgb_u8(r, g, b),
                intensity: brightness * crate::fx::LIGHT_LUMENS,
                range: radius,
                shadows_enabled: false,
                ..default()
            },
            NotShadowCaster,
            Transform::default(),
        ));
    }
}

/// Fires a bouncing round (a Gear Shredder's blade: RoundKind::bounce) from `origin`
/// along `dir`, fired by `firer` (it passes through his shooter, on whichever PC;
/// his side the Decepticons' gives its effects module channel B, the Autobots' A).
pub(crate) fn fire_blade(
    commands: &mut Commands,
    fx: &mut FxSystems,
    kind: &'static RoundKind,
    origin: Vec3,
    dir: Vec3,
    firer: Firer,
) {
    let dir = dir.normalize_or(Vec3::NEG_Z);
    let channel = channel_of(firer.decepticon);
    let params = round_params(channel);
    let flight_fx = fx.spawn_scaled(kind.flight_fx, origin, dir, kind.flight_scale, &params);
    let mut round = commands.spawn((
        Round {
            kind,
            velocity: dir * kind.speed,
            age: 0.0,
            flight_fx,
            owner: firer.owner,
            target: None,
            closing: None,
            blade: Some(Blade {
                flight: BladeFlight::new(origin, dir, kind.speed),
                facing: dir,
                shooter: firer.shooter,
                channel,
            }),
            shooter: firer.shooter,
            channel,
            seq: NEXT_ROUND.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            facing: dir,
            mesh_fx: None,
            fuse: None,
            stuck: None,
            resting: false,
            speeds: Vec::new(),
            set_off: false,
            seed: seed_of(origin, dir) | 1,
            mods: firer.mods,
            warned: false,
        },
        Transform::from_translation(origin),
        Visibility::default(),
    ));
    if let Some(([r, g, b], brightness, radius)) = kind.light {
        round.with_child((
            crate::graphics::LiveLight,
            PointLight {
                color: Color::srgb_u8(r, g, b),
                intensity: brightness * crate::fx::LIGHT_LUMENS,
                range: radius,
                shadows_enabled: false,
                ..default()
            },
            NotShadowCaster,
            Transform::default(),
        ));
    }
}

/// A blade stuck where it stopped (RoundKind::bounce's stuck_fx), in the enemy it
/// struck if it struck one: on its bone nearest where it struck (TnProjectileSawblade.
/// AttachToActor, SetBase on the bone and no offset), found as its bones are there,
/// and goes with it, its body falling too, until it is gone.
#[derive(Component)]
pub(crate) struct StuckBlade {
    fx: FxId,
    sentry: Option<Entity>,
    bone: Option<Entity>,
    at: Vec3,
    facing: Vec3,
}

/// The weapon's muzzle flash at `at` along the barrel `dir`, kept on the muzzle of
/// the gun (WEAPONS) in a Jazz's hand while it plays, if given
/// (follow_muzzle_flashes; else it plays where it started).
pub(crate) fn muzzle_flash(
    commands: &mut Commands,
    fx: &mut FxSystems,
    kind: &RoundKind,
    at: Vec3,
    dir: Vec3,
    gun: Option<(Entity, usize)>,
) {
    flash_on_muzzle(commands, fx, kind.muzzle_fx, at, dir, gun);
}

/// Muzzle flash `system` (any gun's) at `at` along the barrel `dir`, kept on the
/// muzzle of the gun in a Jazz's hand while it plays, at his robot's size, if given
/// (as muzzle_flash: follow_muzzle_flashes starts it); else where it is, at its own
/// size.
pub(crate) fn flash_on_muzzle(
    commands: &mut Commands,
    fx: &mut FxSystems,
    system: &'static str,
    at: Vec3,
    dir: Vec3,
    gun: Option<(Entity, usize)>,
) {
    match gun {
        Some((jazz, weapon)) => {
            commands.spawn(MuzzleFlash {
                fx: None,
                start: Some((system, at, dir)),
                jazz,
                weapon,
                age: 0.0,
            });
        }
        None => {
            fx.spawn(system, at, dir, ENERGON);
        }
    }
}

/// A tank's muzzle flash (TANK_SHELL's) at `at` along `dir`, kept on the barrel of
/// the tank of `jazz` while it plays (follow_cannon_flashes).
pub(crate) fn cannon_flash(
    commands: &mut Commands,
    fx: &mut FxSystems,
    at: Vec3,
    dir: Vec3,
    jazz: Entity,
) {
    if let Some(id) = fx.spawn(TANK_SHELL.muzzle_fx, at, dir, ENERGON) {
        commands.spawn(CannonFlash { fx: id, jazz });
    }
}

/// A jet's or a truck's muzzle flash's colour: its weapon mesh's parameters
/// (HomingRocket_Vehicle_WEPMESH's, Rocket_Vehicle_WEPMESH's: a
/// TnParticleSystemParameterEnergonColor, whose ParameterName is Color) give it the
/// team's energon colour, cyan.
const VEHICLE_FLASH_COLOR: &[(&str, [u8; 4])] = &[("Color", [0, 255, 255, 255])];

/// A jet's or a truck's muzzle flash (its rounds', `kind`'s) at `at` along `dir`,
/// kept on its muzzle `muzzle` of the vehicle of `jazz` (the one nearest `at` if
/// None) while it plays (follow_vehicle_flashes).
pub(crate) fn vehicle_flash(
    commands: &mut Commands,
    fx: &mut FxSystems,
    kind: &RoundKind,
    (at, dir): (Vec3, Vec3),
    jazz: Entity,
    muzzle: Option<usize>,
) {
    flash_on_vehicle(
        commands,
        fx,
        (kind.muzzle_fx, VEHICLE_FLASH_COLOR),
        (at, dir),
        jazz,
        muzzle,
    );
}

/// The car's guns' muzzle flash: the game's own (AssaultRifle_VEH_MuzzleFlash_FX,
/// the vehicle rifle's: fx.rs's CAR_MUZZLE is its light), kept on the gun that fired
/// as the jets' and trucks' are.
const CAR_GUN_FLASH: &str = "AssaultRifle_VEH_MuzzleFlash_FX";

/// A car's guns' muzzle flash at `at` along `dir`, kept on its gun `muzzle` of the
/// vehicle of `jazz` (the one nearest `at` if None) while it plays, as vehicle_flash
/// keeps a jet's or a truck's.
pub(crate) fn car_gun_flash(
    commands: &mut Commands,
    fx: &mut FxSystems,
    (at, dir): (Vec3, Vec3),
    jazz: Entity,
    muzzle: Option<usize>,
) {
    flash_on_vehicle(commands, fx, (CAR_GUN_FLASH, &[]), (at, dir), jazz, muzzle);
}

/// Muzzle flash `system` (with its colour parameters) at `at` along `dir`, kept on
/// muzzle `muzzle` of the vehicle of `jazz` (follow_vehicle_flashes).
fn flash_on_vehicle(
    commands: &mut Commands,
    fx: &mut FxSystems,
    (system, params): (&str, &[(&str, [u8; 4])]),
    (at, dir): (Vec3, Vec3),
    jazz: Entity,
    muzzle: Option<usize>,
) {
    if let Some(id) = fx.spawn(system, at, dir, params) {
        commands.spawn(VehicleFlash {
            fx: id,
            jazz,
            muzzle,
            from: at,
        });
    }
}

/// Keeps the jets', the trucks' and the cars' guns' muzzle flashes on their
/// muzzles (where the body is, along its nose) until they are over, or the vehicle
/// is gone.
fn follow_vehicle_flashes(
    mut commands: Commands,
    mut fx: ResMut<FxSystems>,
    chars: Res<crate::character::Characters>,
    flashes: Query<(Entity, &VehicleFlash)>,
    players: Query<(&Transform, &Player, &crate::car::Car)>,
) {
    for (entity, flash) in &flashes {
        if !fx.alive(flash.fx) {
            commands.entity(entity).despawn();
            continue;
        }
        let muzzle = players.get(flash.jazz).ok().and_then(|(at, player, car)| {
            // (Its own vehicle's, or Jazz's car's: as main.rs's fire finds them.)
            let muzzles = player
                .kit
                .vehicle
                .and_then(|v| chars.vehicles.get(v))
                .map(|v| v.muzzles.as_slice())
                .filter(|m| !m.is_empty())
                .unwrap_or(&crate::VEHICLE_MUZZLES);
            let place = |m: Vec3| at.transform_point(car.on_body(m));
            let m = match flash.muzzle {
                Some(i) => *muzzles.get(i % muzzles.len())?,
                None => *muzzles.iter().min_by(|a, b| {
                    let (a, b) = (place(**a), place(**b));
                    a.distance_squared(flash.from)
                        .total_cmp(&b.distance_squared(flash.from))
                })?,
            };
            (player.form == Form::Vehicle).then(|| {
                let dir = at.rotation * car.tilt() * Vec3::NEG_Z;
                (place(m), dir)
            })
        });
        match muzzle {
            Some((at, dir)) => fx.place(flash.fx, at, dir, Vec3::ZERO),
            // (Changed form, or gone: it plays on where it was.)
            None => {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Keeps the tanks' muzzle flashes on the end of their barrels (the muzzle socket on
/// the cannon bone, along the barrel) until they are over, or the tank is gone.
fn follow_cannon_flashes(
    mut commands: Commands,
    mut fx: ResMut<FxSystems>,
    chars: Res<crate::character::Characters>,
    flashes: Query<(Entity, &CannonFlash)>,
    players: Query<&Player>,
    cannons: Query<(&GlobalTransform, &crate::JazzOf), With<crate::car::CannonBone>>,
) {
    for (entity, flash) in &flashes {
        if !fx.alive(flash.fx) {
            commands.entity(entity).despawn();
            continue;
        }
        let tank = players
            .get(flash.jazz)
            .ok()
            .filter(|p| p.form == Form::Vehicle)
            .and_then(|p| p.kit.vehicle)
            .and_then(|v| chars.vehicles.get(v))
            .and_then(|v| v.tank.as_ref());
        let bone = cannons
            .iter()
            .find(|(_, of)| of.0 == flash.jazz)
            .map(|(g, _)| g);
        match (tank, bone) {
            (Some(tank), Some(bone)) => {
                let at = bone.transform_point(tank.muzzle.transform.translation);
                let dir = bone
                    .affine()
                    .transform_vector3(Vec3::X)
                    .normalize_or(Vec3::NEG_Z);
                fx.place(flash.fx, at, dir, Vec3::ZERO);
            }
            // (Transformed or gone: it plays on where it was.)
            _ => {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// A new match: no rounds left over from the last.
#[allow(clippy::type_complexity)]
fn clear_rounds(
    mut commands: Commands,
    mut fx: ResMut<FxSystems>,
    rounds: Query<(Entity, &Round)>,
    stuck: Query<(Entity, &StuckBlade)>,
    flashes: Query<Entity, Or<(With<MuzzleFlash>, With<CannonFlash>, With<VehicleFlash>)>>,
) {
    for (entity, round) in &rounds {
        for id in [round.flight_fx, round.mesh_fx].into_iter().flatten() {
            fx.kill(id);
        }
        commands.entity(entity).despawn();
    }
    for (entity, blade) in &stuck {
        fx.kill(blade.fx);
        commands.entity(entity).despawn();
    }
    for entity in &flashes {
        commands.entity(entity).despawn();
    }
}

/// Keeps the muzzle flashes on their guns' muzzles (where main.rs's GunSockets put
/// it, turned along the barrel) until they are over, or the gun is put away: then it
/// is turned off where it is (what is out of it lives out its life, the emitters
/// killed on deactivation going at once), as the game's flash goes with the gun. A
/// flash with an emitter that would loop forever is turned off once that emitter's
/// first loop is done (FxSystems::forever_loop_secs: the A-4 Pulsar Cannon's burning
/// ball, which otherwise kept burning on the muzzle, shot after shot, and stayed
/// hanging in the air where the gun was put away).
fn follow_muzzle_flashes(
    mut commands: Commands,
    time: Res<Time>,
    mut fx: ResMut<FxSystems>,
    mut flashes: Query<(Entity, &mut MuzzleFlash)>,
    players: Query<&Player>,
    gun_bones: Query<(&GlobalTransform, &crate::GunBone, &crate::JazzOf)>,
) {
    for (entity, mut flash) in &mut flashes {
        let player = players.get(flash.jazz).ok();
        if let Some((system, at, dir)) = flash.start.take() {
            // (Started at his robot's size: MuzzleFlash.)
            let scale = player.map_or(1.0, |p| p.kit.robot.effect_scale());
            flash.fx = fx.spawn_scaled(system, at, dir, scale, ENERGON);
        }
        let Some(id) = flash.fx.filter(|&id| fx.alive(id)) else {
            commands.entity(entity).despawn();
            continue;
        };
        flash.age += time.delta_secs();
        let Some(player) = player else {
            fx.stop(id);
            commands.entity(entity).despawn();
            continue;
        };
        let muzzle = (player.weapon == flash.weapon)
            .then(|| crate::muzzle_socket(player, flash.jazz, &gun_bones))
            .flatten();
        match muzzle {
            Some((at, dir)) => {
                fx.place(id, at, dir, Vec3::ZERO);
                if fx
                    .forever_loop_secs(id)
                    .is_some_and(|secs| flash.age >= secs)
                {
                    fx.stop(id);
                }
            }
            // (Put away, or the car: turned off where it was.)
            None => {
                fx.stop(id);
                commands.entity(entity).despawn();
            }
        }
    }
}

/// What a round strikes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Struck {
    Level,
    Sentry(Entity),
    Jazz(Entity),
}

/// What a round strikes moving from `from` by `motion` this frame: how far along
/// (0..1 of the move), the surface's normal there (facing back at the round) and what
/// it is: the level, a sentry (`sentries`' middles) or a Jazz other than `shooter`
/// (`bodies`: Projectile.Touch, the players stopping rounds as any pawn does,
/// TnPlayerPawn.StopsProjectile; it passes through the Jazz who fired it,
/// ShouldIgnoreInstigator).
fn strike(
    level: &Level,
    colliders: &Colliders,
    (sentries, bodies): (&[Target], &Bodies),
    shooter: Option<Entity>,
    from: Vec3,
    motion: Vec3,
) -> Option<(f32, Vec3, Struck)> {
    let len = motion.length();
    if len < 1e-6 {
        return None;
    }
    let dir = motion / len;
    let mut hit = level
        .shot_surface(colliders, from, dir, len)
        .map(|(t, n)| (t, n, Struck::Level));
    for target in sentries {
        let (entity, c) = (target.entity, target.at);
        // (Leaving an enemy it is on, as the clusters thrown out of the surface of the
        // one their bomb struck, it strikes nothing: it is on its way out.)
        let out = from - c;
        if target.body.reach(from) <= 0.01 && out.dot(dir) >= 0.0 {
            continue;
        }
        if let Some(t) = target.body.ray(from, dir, hit.map_or(len, |(h, _, _)| h)) {
            hit = Some((
                t,
                (from + dir * t - c).normalize_or(-dir),
                Struck::Sentry(entity),
            ));
        }
    }
    for &(entity, feet, (radius, height)) in bodies {
        if Some(entity) == shooter {
            continue;
        }
        let Some(t) = crate::ray_cylinder(
            from,
            dir,
            Vec2::new(feet.x, feet.z),
            radius,
            feet.y,
            feet.y + height,
        ) else {
            continue;
        };
        if t > hit.map_or(len, |(h, _, _)| h) {
            continue;
        }
        let at = from + dir * t;
        let flat = Vec2::new(at.x - feet.x, at.z - feet.z);
        let normal = if flat.length() >= radius - 1e-3 {
            Vec3::new(flat.x, 0.0, flat.y).normalize_or(-dir)
        } else if at.y > feet.y + height * 0.5 {
            Vec3::Y
        } else {
            Vec3::NEG_Y
        };
        // (Starting inside him, on the way out: nothing.)
        if normal.dot(dir) < 0.0 {
            hit = Some((t, normal, Struck::Jazz(entity)));
        }
    }
    hit.map(|(t, n, what)| (t / len, if n.dot(dir) > 0.0 { -n } else { n }, what))
}

/// The sentries rounds strike (fly_rounds).
type RoundTargets<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Transform, &'static mut Enemy),
    (Without<Player>, Without<Round>),
>;

/// A bouncing round's frame (fly_rounds): its flight (blade_step), its bounces'
/// sound, and as it sticks its blast (Explode: its ExplosionEffect along the
/// surface's normal, its sound, its light out and its flight effect let go), its
/// damage to the sentry it struck and its stuck mesh. Its LifeSpan over in flight,
/// it goes off where it is, pointing up (HmProjectile.Destroyed), and is gone.
#[allow(clippy::too_many_arguments)]
fn fly_blade(
    commands: &mut Commands,
    (level, colliders): (&Level, &Colliders),
    (fx, sfx): (&mut FxSystems, &mut Sfx),
    hits: &mut Vec<Entity>,
    (entity, transform, round): (Entity, &mut Transform, &mut Round),
    enemies: &mut RoundTargets<'_, '_>,
    (sentries, jazzes): (&[Target], &Bodies),
    dt: f32,
) {
    let kind = round.kind;
    let local = round.owner.is_some();
    let (Some(b), Some(blade)) = (kind.bounce, round.blade.as_mut()) else {
        return;
    };
    round.age += dt;
    let event = blade_step(
        b,
        &mut blade.flight,
        dt,
        (level, colliders),
        sentries,
        jazzes,
        blade.shooter,
    );
    round.velocity = blade.flight.velocity;
    transform.translation = blade.flight.at;
    let params = round_params(blade.channel);
    let (at, normal, sentry) = match event {
        BladeEvent::Flew | BladeEvent::Bounced => {
            let at = blade.flight.at;
            if event == BladeEvent::Bounced {
                sfx.play_at(b.sound, at);
            }
            if let Some(id) = round.flight_fx {
                fx.place(id, at, blade.facing, blade.flight.velocity);
            }
            if round.age < kind.life_secs {
                sfx.hold(entity.to_bits(), kind.flight_sound, Some(at));
                return;
            }
            (at, Vec3::Y, None)
        }
        BladeEvent::Stuck { at, normal, sentry } => (at, normal, sentry),
    };
    // It goes off.
    if let Some(id) = round.flight_fx {
        fx.place(id, at, blade.facing, Vec3::ZERO);
        fx.stop(id);
    }
    commands.entity(entity).despawn();
    fx.spawn_scaled(kind.explosion_fx, at, normal, kind.explosion_scale, &params);
    sfx.play_at(kind.explosion_sound, at);
    if round.age >= kind.life_secs && !matches!(event, BladeEvent::Stuck { .. }) {
        return;
    }
    // The enemy it struck takes its damage (a marked one more: orbital.rs; the Gear
    // Shredder's TnDamageTypeSawBlade kills as Dismember) and holds it on its bone
    // (stuck_blades); one it kills isn't left holding it.
    let mut stuck_at = Some(at);
    let mut stuck_in = None;
    if let Some(e) = sentry {
        stuck_at = None;
        if let Ok((_, _, mut enemy)) = enemies.get_mut(e)
            && enemy.alive()
        {
            enemy.take(
                kind.damage,
                round.shooter,
                crate::xp::Blow::Weapon,
                Hit::new(Death::Dismember, at, blade.flight.velocity)
                    .with_momentum(crate::enemies::weapon_momentum("SawBlade"))
                    .of_type("TnDamageTypeSawBlade"),
            );
            // (The hit ping for its Jazz, one of this PC's players: fly_rounds.)
            if local && let Some(owner) = round.owner {
                hits.push(owner);
            }
            if enemy.alive() {
                stuck_at = Some(enemy.centre());
                stuck_in = Some(e);
            }
        }
    }
    if let Some(stuck_at) = stuck_at
        && let Some(id) = fx.spawn(b.stuck_fx, stuck_at, blade.facing, &params)
    {
        commands.spawn(StuckBlade {
            fx: id,
            sentry: stuck_in,
            bone: None,
            at,
            facing: blade.facing,
        });
    }
}

/// Keeps the blades stuck in enemies on them (on their bones), and takes them away
/// as those are gone; the others stay where they are until their time is up.
fn stuck_blades(
    mut commands: Commands,
    mut fx: ResMut<FxSystems>,
    mut blades: Query<(Entity, &mut StuckBlade)>,
    enemies: Query<(&Transform, &Enemy), Without<Player>>,
    foes: Query<(), With<crate::enemies::Foe>>,
    bones: Query<(Entity, &crate::JointOf, &GlobalTransform)>,
) {
    for (entity, mut blade) in &mut blades {
        if !fx.alive(blade.fx) {
            commands.entity(entity).despawn();
            continue;
        }
        let Some(sentry) = blade.sentry else {
            continue;
        };
        if !foes.contains(sentry) {
            fx.kill(blade.fx);
            commands.entity(entity).despawn();
            continue;
        }
        if blade.bone.is_none() {
            let at = blade.at;
            blade.bone = bones
                .iter()
                .filter(|(_, of, _)| of.0 == sentry)
                .min_by(|a, b| {
                    a.2.translation()
                        .distance_squared(at)
                        .total_cmp(&b.2.translation().distance_squared(at))
                })
                .map(|(b, _, _)| b);
        }
        let on = blade
            .bone
            .and_then(|b| bones.get(b).ok())
            .map(|(_, _, g)| g.translation())
            .or_else(|| enemies.get(sentry).ok().map(|(_, e)| e.centre()));
        if let Some(at) = on {
            fx.place(blade.fx, at, blade.facing, Vec3::ZERO);
        }
    }
}

/// How far a body (an upright cylinder: feet at `feet`, `radius` and `height`) is
/// from `at`, 0 inside it.
pub(crate) fn distance_to_body(at: Vec3, feet: Vec3, (radius, height): (f32, f32)) -> f32 {
    let flat = Vec2::new(at.x - feet.x, at.z - feet.z).length();
    let out = (flat - radius).max(0.0);
    let up = if at.y < feet.y {
        feet.y - at.y
    } else if at.y > feet.y + height {
        at.y - feet.y - height
    } else {
        0.0
    };
    Vec2::new(out, up).length()
}

/// A round going off: where, the way its blast faces (its HitNormal), and the
/// "normal" its clusters fly along (TnClusterBombProjectileExtension.SpawnCluster's
/// SpawnVelocity, metres a second: a struck surface's unit normal, which in the game's
/// units is a centimetre a second; set off in the air, its velocity).
struct GoOff {
    at: Vec3,
    normal: Vec3,
    spawn_velocity: Vec3,
}

impl GoOff {
    /// Going off on a surface of unit `normal`, or facing that way.
    fn on(at: Vec3, normal: Vec3) -> GoOff {
        GoOff {
            at,
            normal,
            spawn_velocity: normal * 0.01,
        }
    }
}

/// Where a round strikes the level, it goes off this far out of it
/// (Projectile.HitWall: Location + HitNormal * 15, in centimetres); striking an
/// enemy or a player (Touch), where it touches.
const WALL_LIFT: f32 = 0.15;

/// Keeps a round's effects on it: its flight effect and its mesh, facing its way.
fn place_round(fx: &mut FxSystems, round: &Round, at: Vec3) {
    if let Some(id) = round.flight_fx {
        fx.place(id, at, round.facing, round.velocity);
    }
    if let Some(id) = round.mesh_fx {
        fx.place(id, at, round.facing, round.velocity);
    }
}

/// A step of `dt` of a round in flight (PHYS_Projectile, or PHYS_Falling for one with
/// a GravityScale): homing on its target, falling, and striking the first thing in
/// its way (the level, a sentry, a Jazz but its shooter). Where it goes off, if it
/// does: on what it strikes; where it is as its shooter sets it off (a launcher's
/// grenade, facing up: DetonateGrenade; a Combustor's bomb along its velocity:
/// ServerSpawnCluster), as it has closed on its target too long (Closing.OnTick:
/// facing up), or as its LifeSpan runs out (facing back the way it faces:
/// HmProjectile.OnWaitingForLifeSpanDestruction, its flight effect being alive). A
/// sticky round sticks in what it strikes instead (stick).
#[allow(clippy::too_many_arguments)]
fn flight_step(
    round: &mut Round,
    transform: &mut Transform,
    entity: Entity,
    (level, colliders): (&Level, &Colliders),
    (sentries, bodies): (&[Target], &Bodies),
    enemies: &RoundTargets<'_, '_>,
    (fx, sfx): (&mut FxSystems, &mut Sfx),
    dt: f32,
) -> Option<GoOff> {
    round.age += dt;
    let from = transform.translation;
    if round.set_off {
        return Some(match round.kind.clusters {
            Some(_) => GoOff {
                at: from,
                normal: round.velocity.normalize_or(Vec3::Y),
                spawn_velocity: round.velocity,
            },
            None => GoOff::on(from, Vec3::Y),
        });
    }
    // A homing round steers for its target while it is there (Homing.OnTick,
    // Closing.OnTick), its speed held to its MaxSpeed (BoundProjectileVelocity);
    // closed on it too long, it goes off where it is.
    if let (Some(homing), Some(target)) = (round.kind.homing, round.target) {
        match enemies.get(target) {
            Ok((_, _, e)) if e.alive() => {
                let to = e.centre() - from;
                if round.closing.is_none() && to.length() <= homing.closing_distance {
                    round.closing = Some(0.0);
                }
                let force = match round.closing {
                    Some(_) => homing.closing_force,
                    None => homing.force,
                };
                round.velocity += to.normalize_or_zero() * force * dt;
                round.velocity = round
                    .velocity
                    .clamp_length_max(round.kind.speed * round.mods.speed);
                if let Some(t) = round.closing.as_mut() {
                    *t += dt;
                    if *t >= homing.closing_secs {
                        return Some(GoOff::on(from, Vec3::Y));
                    }
                }
            }
            _ => {
                round.target = None;
                round.closing = None;
            }
        }
    }
    // (A round with a GravityScale falls.)
    round.velocity.y += WORLD_GRAVITY * round.kind.gravity * dt;
    if round.kind.follows_velocity {
        round.facing = round.velocity.normalize_or(round.facing);
    }
    let motion = round.velocity * dt;
    let world = (level, colliders);
    let Some((f, normal, what)) = strike(
        world.0,
        world.1,
        (sentries, bodies),
        round.shooter,
        from,
        motion,
    ) else {
        let at = from + motion;
        transform.translation = at;
        place_round(fx, round, at);
        if round.age >= round.kind.life_secs {
            return Some(GoOff::on(at, -round.facing));
        }
        sfx.hold(entity.to_bits(), round.kind.flight_sound, Some(at));
        return None;
    };
    let at = from + motion * f;
    if let Some(s) = round.kind.sticky {
        stick(round, transform, (at, normal, what), s, bodies);
        place_round(fx, round, transform.translation);
        return None;
    }
    Some(match what {
        Struck::Level => GoOff::on(at + normal * WALL_LIFT, normal),
        _ => GoOff::on(at, normal),
    })
}

/// A sticky round sticks in what it struck at `at` (TnProjectileGrenadeLauncher.
/// OnHitThing: it stops, and AttachToActor), its fuse lit (HitThing: its LifeSpan its
/// FuseTime) and its flight's loop let go for its fuse's sound (stuck_step holds it).
fn stick(
    round: &mut Round,
    transform: &mut Transform,
    (at, normal, what): (Vec3, Vec3, Struck),
    s: &Sticky,
    bodies: &Bodies,
) {
    round.velocity = Vec3::ZERO;
    round.fuse = Some(s.fuse);
    round.stuck = Some(match what {
        Struck::Level => {
            transform.translation = at + normal * GRENADE_RADIUS;
            StuckIn::Level
        }
        Struck::Sentry(e) => {
            transform.translation = at;
            StuckIn::Sentry(e)
        }
        Struck::Jazz(e) => {
            transform.translation = at;
            let (feet, height) = bodies
                .iter()
                .find(|b| b.0 == e)
                .map_or((at, 0.0), |b| (b.1, b.2.1));
            StuckIn::Jazz(e, (at.y - feet.y).clamp(0.0, height))
        }
    });
}

/// A step of a sticky round stuck (TnProjectileGrenadeLauncher): it stays on what it is
/// in (on a sentry's middle, on a Jazz's body as he moves), its fuse's sound held
/// (FuseSound, from the strike to its going off), until it goes off: its fuse out
/// (facing back the way it was fired), set off by its shooter (facing up), or what it
/// is in gone (its Tick: ShouldDetach, its Base gone; facing up).
#[allow(clippy::too_many_arguments)]
fn stuck_step(
    round: &mut Round,
    transform: &mut Transform,
    entity: Entity,
    enemies: &RoundTargets<'_, '_>,
    (bodies, joints): (&Bodies, &Joints),
    (fx, sfx): (&mut FxSystems, &mut Sfx),
    dt: f32,
) -> Option<GoOff> {
    round.age += dt;
    let s = round.kind.sticky?;
    // (A round just stuck in a Jazz or an enemy takes the bone of its nearest where
    // it struck.)
    match round.stuck {
        Some(StuckIn::Jazz(e, _)) => {
            if let Some(bone) = nearest_bone(joints, e, transform.translation) {
                round.stuck = Some(StuckIn::Bone(e, bone));
            }
        }
        Some(StuckIn::Sentry(e)) => {
            if let Some(bone) = nearest_bone(joints, e, transform.translation) {
                round.stuck = Some(StuckIn::EnemyBone(e, bone));
            }
        }
        _ => {}
    }
    let held = match round.stuck {
        Some(StuckIn::Level) => Some(transform.translation),
        Some(StuckIn::Sentry(e)) => enemies
            .get(e)
            .ok()
            .filter(|(_, _, en)| en.alive())
            .map(|(_, _, en)| en.centre()),
        Some(StuckIn::EnemyBone(_, bone)) => joints.iter().find(|j| j.0 == bone).map(|j| j.2),
        Some(StuckIn::Jazz(e, up)) => bodies.iter().find(|b| b.0 == e).map(|b| b.1 + Vec3::Y * up),
        // (Gone with him: dead, or out of the match.)
        Some(StuckIn::Bone(e, bone)) => bodies
            .iter()
            .any(|b| b.0 == e)
            .then(|| joints.iter().find(|j| j.0 == bone).map(|j| j.2))
            .flatten(),
        None => None,
    };
    let Some(at) = held else {
        return Some(GoOff::on(transform.translation, Vec3::Y));
    };
    transform.translation = at;
    place_round(fx, round, at);
    if round.set_off {
        return Some(GoOff::on(at, Vec3::Y));
    }
    let fuse = round.fuse.get_or_insert(s.fuse);
    *fuse -= dt;
    if *fuse <= 0.0 {
        return Some(GoOff::on(at, -round.facing));
    }
    sfx.hold(entity.to_bits(), s.sound, Some(at));
    None
}

/// A step of a grenade (TnProjectileGrenadeBase): it falls and bounces off what it
/// strikes (going off on an enemy or a player, if it does that), its fuse lit as it
/// first strikes something, coming to rest as it slows (below LowSpeedThreshold
/// over its last second), and going off as its fuse runs out (facing back the way it
/// was fired: OnWaitingForLifeSpanDestruction). Its flight's loop is held until its
/// fuse is lit, its fuse's after.
#[allow(clippy::too_many_arguments)]
fn grenade_step(
    round: &mut Round,
    transform: &mut Transform,
    entity: Entity,
    (level, colliders): (&Level, &Colliders),
    (sentries, bodies): (&[Target], &Bodies),
    (fx, sfx): (&mut FxSystems, &mut Sfx),
    near: &mut Vec<Vec3>,
    dt: f32,
) -> Option<GoOff> {
    let g = round.kind.grenade?;
    round.age += dt;
    let from = transform.translation;
    let mut at = from;
    if !round.resting {
        round.velocity.y += WORLD_GRAVITY * round.kind.gravity * dt;
        let motion = round.velocity * dt;
        match strike(
            level,
            colliders,
            (sentries, bodies),
            round.shooter,
            from,
            motion,
        ) {
            Some((f, normal, what)) => {
                let touch = from + motion * f;
                if g.explode_on_pawn && what != Struck::Level {
                    return Some(GoOff::on(touch, normal));
                }
                at = touch + normal * GRENADE_RADIUS;
                let speed = round.velocity.length();
                if round.fuse.is_none() {
                    let k = draw(&mut round.seed);
                    round.fuse = Some(g.fuse.0 + (g.fuse.1 - g.fuse.0) * k);
                }
                let v = round.velocity;
                round.velocity = (v - normal * 2.0 * v.dot(normal)) * g.keep;
                if speed > BOUNCE_SOUND_SPEED {
                    sfx.play_at(g.sound_bounce, at);
                }
                // (OnHitThing's NotifyPawnsOfGrenade: once it bounces slowly enough
                // across the ground, the enemies near take cover from it.)
                let across = Vec2::new(round.velocity.x, round.velocity.z).length();
                if !round.warned && across <= crate::ai::GRENADE_WARN_SPEED {
                    round.warned = true;
                    near.push(at);
                }
            }
            None => at = from + motion,
        }
        transform.translation = at;
        // (Its speed over the last second: its _DeltaPostionHistory.)
        let speed = (at - from).length() / dt.max(1e-4);
        round.speeds.push((round.age, speed));
        let since = round.age - REST_SAMPLE_SECS;
        round.speeds.retain(|(t, _)| *t >= since);
        let mean =
            round.speeds.iter().map(|(_, v)| v).sum::<f32>() / round.speeds.len().max(1) as f32;
        if mean < REST_SPEED {
            round.resting = true;
            round.velocity = Vec3::ZERO;
        }
    }
    place_round(fx, round, at);
    match round.fuse.as_mut() {
        Some(fuse) => {
            *fuse -= dt;
            if *fuse <= 0.0 {
                return Some(GoOff::on(at, -round.facing));
            }
            sfx.hold(entity.to_bits(), g.sound_fuse, Some(at));
        }
        None => {
            if round.age >= round.kind.life_secs {
                return Some(GoOff::on(at, -round.facing));
            }
            sfx.hold(entity.to_bits(), round.kind.flight_sound, Some(at));
        }
    }
    None
}

/// A round's clusters as it goes off (Clusters: SpawnCluster): each along its way at
/// random in the cone round `spawn_velocity`'s way, fired by the round's shooter.
fn spawn_clusters(
    commands: &mut Commands,
    fx: &mut FxSystems,
    c: &'static Clusters,
    (at, spawn_velocity): (Vec3, Vec3),
    parent: &Round,
) {
    let dir = spawn_velocity.normalize_or(Vec3::Y);
    let mut seed = parent.seed ^ 0x2545_f491;
    // (The bombs' clusters are the weapon's rounds too, with its upgrades: the
    // Chaos Fuse adds to them, TnBuffWeaponNumClusters.)
    let firer = Firer {
        owner: parent.owner,
        shooter: parent.shooter,
        decepticon: parent.channel == MODULE_CHANNEL_B,
        mods: RoundMods {
            clusters: 0,
            ..parent.mods
        },
    };
    for _ in 0..c.count + parent.mods.clusters {
        let way = random_in_cone(dir, c.cone.to_radians(), &mut seed);
        let speed = c.speed.0 + (c.speed.1 - c.speed.0) * draw(&mut seed);
        let velocity = spawn_velocity * c.transfer + way * speed;
        let own = seed ^ 0x9e37_79b9;
        draw(&mut seed);
        launch(commands, fx, c.bomb, (at, velocity, way), firer, None, own);
    }
}

/// The players rounds hurt and shake (fly_rounds).
type RoundPlayers<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Transform,
        &'static mut Player,
        Has<LocalPlayer>,
    ),
    Without<Round>,
>;

/// A round going off (`go`): its flight effect let go and its mesh gone, its blast
/// (Explode: its ExplosionEffect along its normal, at its size, and its sound), its
/// damage to every sentry in reach (Projectile.HurtRadius; an exploding barrel too:
/// barrels.rs) and to its own Jazz if he is (at SelfDamageMultiplier), its shake for
/// each of this PC's players (PlayLocalCameraShake, by his middle) and its clusters.
fn go_off(
    commands: &mut Commands,
    (fx, sfx, barrels): (&mut FxSystems, &mut Sfx, &mut crate::barrels::Barrels),
    (entity, round): (Entity, &Round),
    go: GoOff,
    enemies: &mut RoundTargets<'_, '_>,
    players: &mut RoundPlayers<'_, '_>,
) {
    let kind = round.kind;
    let params = round_params(round.channel);
    let normal = go.normal.normalize_or(Vec3::Y);
    // (Where its Jazz is: what it kills near him it gibs, by its damage type's
    // GibDistance.)
    let shooter_at = round
        .owner
        .and_then(|o| players.get(o).ok())
        .map(|(_, t, p, _)| p.robot_centre(t.translation));
    if let Some(id) = round.flight_fx {
        fx.place(id, go.at, round.facing, Vec3::ZERO);
        fx.stop(id);
    }
    if let Some(id) = round.mesh_fx {
        fx.kill(id);
    }
    commands.entity(entity).despawn();
    // (A radius upgrade reaches further, its blast's effect as much bigger:
    // GetDamageRadius over ExplosionEffectRadius.)
    let radius = kind.radius * round.mods.radius;
    fx.spawn_scaled(
        kind.explosion_fx,
        go.at,
        normal,
        kind.explosion_scale * round.mods.radius,
        &params,
    );
    sfx.play_at(kind.explosion_sound, go.at);
    let centre = go.at + normal * BLAST_LIFT;
    let mut struck = false;
    for (_, _, mut e) in enemies.iter_mut() {
        let reach = e.reach(centre);
        if !e.alive() || reach > radius {
            continue;
        }
        // (An enemy the Orbital Pistol has marked takes more: orbital.rs. A tank's
        // shell kills as TnDamageTypeTankShell: Explosive and Tank; its momentum
        // falls off as its damage does: TakeRadiusDamage's DamageScale.)
        let at = e.centre();
        let (death, momentum, gib) = kind.blow();
        let scale = kind.damage_scale(reach);
        let from = shooter_at.unwrap_or(centre);
        e.take(
            kind.damage * scale,
            round.shooter,
            crate::xp::Blow::Weapon,
            Hit::new(death, at, at - centre)
                .with_momentum(momentum * scale)
                .gibs_within(gib, from, at)
                .of_type(kind.damage_type()),
        );
        struck = true;
    }
    if let Some(by) = round.shooter {
        barrels.blast(
            centre,
            radius,
            |reach| kind.damage * kind.damage_scale(reach),
            kind.damage_type(),
            crate::barrels::By::Player(by),
        );
    }
    if let Some(c) = kind.clusters {
        spawn_clusters(commands, fx, c, (go.at, go.spawn_velocity), round);
    }
    if let Some(shake) = kind.shake {
        for (_, t, mut player, local) in players.iter_mut() {
            let at = player.centre(t.translation);
            if local && let Some(felt) = shake.felt(at.distance(go.at), radius) {
                player.screen_shakes.push(felt);
            }
        }
    }
    // Its own Jazz, if he is in the blast.
    let Some(owner) = round.owner else {
        return;
    };
    let Ok((_, owner_transform, mut player, local)) = players.get_mut(owner) else {
        return;
    };
    if local && struck {
        player.hud_events.push(HudEvent::Hit);
    }
    let body = body_of(&player);
    let reach = distance_to_body(centre, owner_transform.translation, body);
    if reach > radius {
        return;
    }
    let damage = kind.damage * kind.damage_scale(reach) * kind.self_damage;
    let mid = player.robot_centre(owner_transform.translation);
    let (death, momentum, _) = kind.blow();
    // (By his own hand: the kill feed's suicide, with its weapon's icon.)
    let hit = Hit::new(death, mid, mid - centre)
        .with_momentum(momentum * kind.damage_scale(reach))
        .of_type(kind.damage_type());
    if crate::damage_player(&mut player, damage, hit) {
        sfx.play_from(
            crate::SND_PLAYER_HIT,
            crate::voice_at(local, owner_transform.translation),
        );
        if local {
            // The HUD's damage arrow points back at the blast.
            player.hud_events.push(HudEvent::Damaged {
                yaw: crate::heading(centre - owner_transform.translation),
                amount: damage / HIT_INDICATOR_HEALTH,
            });
        }
    }
}

/// Flies the rounds and sets them off: as they strike, as their fuses run out, or as
/// their shooters set them off (Player::set_off: his launcher's alt fire, a
/// Combustor's trigger; and the oldest of a launcher's rounds once he has more out
/// than it keeps, Sticky::most). Each Jazz's status follows from his rounds: his
/// sticky rounds out (GearState::grenades, the launcher's ActiveGrenadeCount) and
/// whether a round of his that bursts into clusters is in the air
/// (GearState::in_flight, TnWeaponTankCannonArcable's _IsProjectileInFlight).
#[allow(clippy::too_many_arguments)]
fn fly_rounds(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut rounds: Query<(Entity, &mut Transform, &mut Round)>,
    mut enemies: RoundTargets,
    mut players: RoundPlayers,
    (bones, mut near): (
        Query<(Entity, &crate::JointOf, &GlobalTransform)>,
        ResMut<crate::ai::NearGrenades>,
    ),
    mut barrels: ResMut<crate::barrels::Barrels>,
) {
    let dt = time.delta_secs();
    // (The bones of the Jazzes rounds are stuck in, where they are.)
    let stuck_in: Vec<Entity> = rounds
        .iter()
        .filter_map(|(_, _, r)| match r.stuck {
            Some(
                StuckIn::Jazz(e, _)
                | StuckIn::Bone(e, _)
                | StuckIn::Sentry(e)
                | StuckIn::EnemyBone(e, _),
            ) => Some(e),
            _ => None,
        })
        .collect();
    let joints: Vec<(Entity, Entity, Vec3)> = if stuck_in.is_empty() {
        Vec::new()
    } else {
        bones
            .iter()
            .filter(|(_, of, _)| stuck_in.contains(&of.0))
            .map(|(e, of, t)| (e, of.0, t.translation()))
            .collect()
    };
    let live = crate::arsenal::live_targets(enemies.iter().map(|(e, _, en)| (e, en)));
    // (The players rounds strike and blades bounce off: every Jazz in the match, up.)
    let jazzes: Vec<(Entity, Vec3, (f32, f32))> = players
        .iter()
        .filter(|(_, _, p, _)| p.dead_left <= 0.0 && !p.dying && !p.waiting)
        .map(|(e, t, p, _)| (e, t.translation, body_of(p)))
        .collect();
    // (The Jazzes, of this PC's players, whose blades hit this frame.)
    let mut hits = Vec::new();
    let set_off: Vec<(Entity, &'static RoundKind)> = players
        .iter_mut()
        .filter_map(|(e, _, mut p, _)| p.set_off.take().map(|k| (e, k)))
        .collect();
    // A launcher's rounds past the most it keeps out: its oldest go
    // (TnWeaponGrenadeLauncherBase.ServerSpawnedProjectile: DetonateGrenade).
    let mut out: Vec<(u64, u64, Entity, usize)> = rounds
        .iter()
        .filter(|(_, _, r)| !r.set_off)
        .filter_map(|(e, _, r)| Some((r.shooter?.to_bits(), r.seq, e, r.kind.sticky?.most)))
        .collect();
    out.sort_unstable();
    let mut too_many = Vec::new();
    for group in out.chunk_by(|a, b| a.0 == b.0) {
        let most = group[0].3;
        if group.len() > most {
            too_many.extend(group[..group.len() - most].iter().map(|g| g.2));
        }
    }
    let mut status: HashMap<Entity, (u8, bool)> = HashMap::new();
    for (entity, mut transform, mut round) in &mut rounds {
        let round = &mut *round;
        let by_shooter = round.shooter.is_some_and(|s| {
            set_off
                .iter()
                .any(|(e, k)| *e == s && same_kind(k, round.kind))
        });
        if by_shooter || too_many.contains(&entity) {
            round.set_off = true;
        }
        if round.blade.is_some() {
            fly_blade(
                &mut commands,
                (&level, &colliders),
                (&mut fx, &mut sfx),
                &mut hits,
                (entity, &mut transform, round),
                &mut enemies,
                (&live, &jazzes),
                dt,
            );
            continue;
        }
        let world = (&*level, &*colliders);
        let event = if round.kind.grenade.is_some() {
            grenade_step(
                round,
                &mut transform,
                entity,
                world,
                (&live, &jazzes),
                (&mut fx, &mut sfx),
                &mut near.0,
                dt,
            )
        } else if round.stuck.is_some() {
            stuck_step(
                round,
                &mut transform,
                entity,
                &enemies,
                (&jazzes, &joints),
                (&mut fx, &mut sfx),
                dt,
            )
        } else {
            flight_step(
                round,
                &mut transform,
                entity,
                world,
                (&live, &jazzes),
                &enemies,
                (&mut fx, &mut sfx),
                dt,
            )
        };
        match event {
            None => {
                if let Some(shooter) = round.shooter {
                    let s = status.entry(shooter).or_default();
                    if round.kind.sticky.is_some() {
                        s.0 = s.0.saturating_add(1);
                    }
                    if round.kind.clusters.is_some() {
                        s.1 = true;
                    }
                }
            }
            Some(go) => go_off(
                &mut commands,
                (&mut fx, &mut sfx, &mut barrels),
                (entity, round),
                go,
                &mut enemies,
                &mut players,
            ),
        }
    }
    for owner in hits {
        if let Ok((_, _, mut player, true)) = players.get_mut(owner) {
            player.hud_events.push(HudEvent::Hit);
        }
    }
    for (jazz, _, mut player, _) in &mut players {
        let (grenades, in_flight) = status.get(&jazz).copied().unwrap_or_default();
        if player.gear.grenades != grenades {
            player.gear.grenades = grenades;
        }
        if player.gear.in_flight != in_flight {
            player.gear.in_flight = in_flight;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A toss lands where it is thrown, when it is thrown to, under its gravity.
    #[test]
    fn toss_lands_on_time() {
        let from = Vec3::new(1.0, 2.0, 3.0);
        let to = Vec3::new(-20.0, 0.5, -30.0);
        for (secs, gravity) in [(0.1, 1.0), (0.6, 1.0), (1.1, 1.25)] {
            let v = toss_velocity(from, to, secs, gravity);
            let landed = from + v * secs + 0.5 * Vec3::Y * WORLD_GRAVITY * gravity * secs * secs;
            assert!(landed.distance(to) < 1e-3, "{secs} {gravity}: {landed:?}");
        }
    }

    /// A blast's shake: none within its reach or beyond its radius, and between them
    /// all of it scaled down by the distance (UseFalloff).
    #[test]
    fn shake_falls_off() {
        let s = ROCKET_SHAKE;
        assert!(s.felt(5.0, 7.0).is_none());
        assert!(s.felt(151.0, 7.0).is_none());
        let half = s.felt(75.0, 7.0).expect("felt at 75 m");
        assert!((half.secs - 0.25).abs() < 1e-5);
        assert!((half.rot_amp - s.rot_amp * 0.5).length() < 1e-4);
        assert!((half.rot_freq - s.rot_freq * 0.5).length() < 1e-4);
        assert!((half.loc_amp - s.loc_amp * 0.5).length() < 1e-4);
        let near = CLUSTER_SHAKE.felt(15.0, 8.5).expect("felt at 15 m");
        assert!((near.fov_amp - CLUSTER_SHAKE.fov_amp * 0.9).abs() < 1e-5);
    }

    /// A shot's draws are the same on every PC (from where it left and its way), and
    /// each between 0 and 1.
    #[test]
    fn draws_repeat() {
        let (origin, dir) = (
            Vec3::new(3.5, 2.25, -7.0),
            Vec3::new(0.1, 0.3, -0.9).normalize(),
        );
        assert_eq!(seed_of(origin, dir), seed_of(origin, dir));
        assert_ne!(seed_of(origin, dir), seed_of(origin + Vec3::X * 0.01, dir));
        let (mut a, mut b) = (seed_of(origin, dir) | 1, seed_of(origin, dir) | 1);
        for _ in 0..1000 {
            let (x, y) = (draw(&mut a), draw(&mut b));
            assert_eq!(x.to_bits(), y.to_bits());
            assert!((0.0..1.0).contains(&x), "{x}");
        }
    }

    /// The clusters' ways stay within their cone, all round it.
    #[test]
    fn cone_ways() {
        let dir = Vec3::new(0.2, 0.9, -0.1).normalize();
        let cone = COMBUSTOR_CLUSTERS.cone.to_radians();
        let mut seed = 0x1234_5679;
        let mut widest: f32 = 0.0;
        for _ in 0..2000 {
            let way = random_in_cone(dir, cone, &mut seed);
            assert!((way.length() - 1.0).abs() < 1e-4);
            let angle = way.dot(dir).clamp(-1.0, 1.0).acos();
            assert!(angle <= cone + 1e-3, "{angle}");
            widest = widest.max(angle);
        }
        assert!(widest > cone * 0.95, "{widest}");
        // (Straight up too, where the cone's axes come from another side.)
        let way = random_in_cone(Vec3::Y, cone, &mut seed);
        assert!(way.dot(Vec3::Y) >= cone.cos() - 1e-4);
    }

    /// Kinds of round are told apart by what they are, not where their constants sit.
    #[test]
    fn kinds_compare() {
        let copy = RoundKind { ..COMBUSTOR_SHELL };
        assert!(same_kind(&copy, &COMBUSTOR_SHELL));
        assert!(!same_kind(&COMBUSTOR_SHELL, &COMBUSTOR_CLUSTER));
        assert!(!same_kind(&PULSAR_ROUND, &THERMO_ROCKET));
    }

    /// A rocket's damage: full to 2.5 m, half from there on (its RadiusDamageScale).
    #[test]
    fn rocket_falloff() {
        assert_eq!(THERMO_ROCKET.damage_scale(0.0), 1.0);
        assert_eq!(THERMO_ROCKET.damage_scale(2.5), 1.0);
        assert_eq!(THERMO_ROCKET.damage_scale(2.6), 0.5);
        assert_eq!(THERMO_ROCKET.damage_scale(7.0), 0.5);
        assert_eq!(COMBUSTOR_CLUSTER.damage_scale(8.0), 1.0);
    }
}
