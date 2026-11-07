//! The enemies' guns and how they use them: TnAiWeaponController (bursts as long as
//! the competency makes them, pauses between them), TnAiWeaponAimer (aiming at a
//! point or in a direction, and its bad aim point for an incompetent shot),
//! TnAiTargeter (aiming at the target's eyes), and the gun itself as TnWeapon fires
//! for an AI (its fire interval, its spread climbing with each shot and cooling,
//! its clip and reload, its instant hits with their damage falling off with range:
//! the weapon's survival data, AiSurvivalData).

use bevy::prelude::*;

use super::brain::{Brain, Ctx};
use super::data::Behaviour;
use super::pawn::Pawn;
use super::ue::{self, Rng};

/// FloatScaler (its DistanceScale segments: their type, end point, start value and
/// ease exponent).
#[derive(Clone, Debug, Default)]
pub struct FloatScaler {
    pub scales: Vec<(u8, f32, f32, f32)>,
}

impl FloatScaler {
    /// GetValueAtPoint.
    pub fn value_at(&self, point: f32) -> f32 {
        if self.scales.is_empty() {
            return 1.0;
        }
        let mut start_point = 0.0;
        let mut start_value = self.scales[0].2;
        for (i, &(kind, end_point, _, factor)) in self.scales.iter().enumerate() {
            if kind == 0 {
                // DST_MaxRange.
                return self.scales[i].2;
            }
            let end_value = self.scales.get(i + 1).map_or(0.0, |s| s.2);
            if point <= end_point {
                let a = (point - start_point) / (end_point - start_point).max(1e-6);
                return match kind {
                    1 => start_value,
                    2 => ue::lerp(start_value, end_value, a),
                    3 => ue::lerp(start_value, end_value, a.max(0.0).powf(factor)),
                    4 => ue::lerp(
                        start_value,
                        end_value,
                        1.0 - (1.0 - a).max(0.0).powf(factor),
                    ),
                    5 => {
                        let e = if a < 0.5 {
                            0.5 * (2.0 * a).max(0.0).powf(factor)
                        } else {
                            1.0 - 0.5 * (2.0 * (1.0 - a)).max(0.0).powf(factor)
                        };
                        ue::lerp(start_value, end_value, e)
                    }
                    _ => start_value,
                };
            }
            start_point = end_point;
            start_value = end_value;
        }
        0.0
    }
}

/// A gun's AI data (its TnWeaponData for survival games).
#[derive(Clone, Debug)]
pub struct GunData {
    pub class: String,
    /// FireIntervals[0]: its interval's range and ramp time (an HmValueLerper).
    pub interval: (f32, f32, f32),
    /// Spreads[0]: starting high or low, its range, its change each shot and its
    /// cool down time.
    pub spread_start_high: bool,
    pub spread: (f32, f32),
    pub spread_per_shot: f32,
    pub spread_cooldown: f32,
    /// CompetentBurst and IncompetentBurst: the rounds in a burst and the pause
    /// after it.
    pub competent_burst: (i32, i32),
    pub competent_pause: (f32, f32),
    pub incompetent_burst: (i32, i32),
    pub incompetent_pause: (f32, f32),
    pub clip_size: u32,
    pub unlimited_reserve: bool,
    pub reload_time: f32,
    pub equip_time: f32,
    pub damage: f32,
    pub range: f32,
    pub range_scale: FloatScaler,
    /// ShotsPerBurst (0: not a burst weapon) and the delay after a burst.
    pub shots_per_burst: u32,
    pub burst_delay: f32,
    /// The Guardian Blaster's ChargeDuration (TnWeaponGuardianBlaster: each firing
    /// charges this long before its first round; none for every other gun).
    pub charge: f32,
    /// A round's speed for a projectile gun (none for an instant hit one).
    pub projectile_speed: f32,
    /// NumShotsToFire: the traces a round fires (the shotgun's pellets), and the
    /// pattern they spread in (SpreadPattern: each one's offset right and up,
    /// scaled by how far the spread has grown from its start; none for a random
    /// spread).
    pub shots_to_fire: u32,
    pub spread_pattern: Vec<(f32, f32)>,
    /// Its stance while it is in hand (BaseStance).
    pub base_stance: u8,
    /// A projectile gun's round (its first fire mode's projectile class's AI
    /// data), none for an instant hit one.
    pub projectile: Option<std::sync::Arc<ProjectileData>>,
    /// Its instant hits' damage type (InstantHitDamageTypes[0]: what its kills die
    /// of) and momentum (InstantHitMomentum[0], Unreal's units).
    pub damage_type: String,
    pub momentum: f32,
}

/// A projectile's data (its TnProjectileData: AiSurvivalData, else AiData, else
/// Data, as Escalation's AI fires it) and its projectile mesh's looks
/// (TnProjectileMesh).
#[derive(Clone, Debug, Default)]
pub struct ProjectileData {
    pub class: String,
    /// InitialSpeed and MaxSpeed (none: as fast as it goes), and its share of the
    /// world's gravity (GravityScale).
    pub speed: f32,
    pub max_speed: f32,
    pub gravity_scale: f32,
    /// Its damage, the radius it does it in (its RadiusDamageScale's scale of it by
    /// distance), its damage type, its MomentumTransfer, and whether its blast
    /// hurts without a clear line from it (IgnoreHurtRadiusTrace).
    pub damage: f32,
    pub damage_radius: f32,
    pub radius_scale: FloatScaler,
    pub damage_type: String,
    pub momentum_transfer: f32,
    pub ignore_hurt_radius_trace: bool,
    /// A TnProjectileHoming's pull towards its target.
    pub homing: Option<HomingData>,
    /// Its looks: its flight effect (FlightEffect, the particle system's name),
    /// its blast (ExplosionEffect, ExplosionEffectScale), and its sounds
    /// (FlightSound's loop, ExplosionSound).
    pub flight_fx: String,
    pub explosion_fx: String,
    pub explosion_scale: f32,
    pub flight_sound: &'static str,
    pub explosion_sound: &'static str,
    /// Its light (its mesh's PointLightComponent: radius, brightness), if any.
    pub light: Option<(f32, f32)>,
    /// A sticky round's (TnProjectileGrenadeLauncher's: the jets' rockets).
    pub sticky: Option<StickyData>,
    /// Its blast's camera shake (its TnCameraShakeProjectileExtension: out to its
    /// CameraShakeRadius, its ScreenShakeStruct; metres).
    pub shake: Option<crate::riot::ScreenShake>,
    /// Its grenade marker on the HUD (TnProjectileDataGrenadeLauncher.ShowMarker:
    /// TnObjectiveMarkerTypeGrenade, from UpdateData to its going off).
    pub marker: bool,
}

/// A sticky round (TnProjectileGrenadeLauncher: OnHitThing stops it and
/// AttachToActor sticks it in what it struck), its fuse lit as it strikes
/// (HitThing: its LifeSpan its FuseTime, at random between its two; its flight's
/// loop let go for its FuseSound); a TnProjectileFlyerRocket stuck in a player is
/// shaken off by his changing form (its Tick: disabled, its
/// TnDetachFXProjectileExtension's DetachFXTemplate where it was and its
/// DetachAudioCue at him, gone without going off), its contextual command
/// (TnContextualCommandFlyerRocket) on his HUD meanwhile.
#[derive(Clone, Debug, Default)]
pub struct StickyData {
    pub fuse: (f32, f32),
    pub fuse_sound: &'static str,
    pub shaken_off: Option<(String, &'static str)>,
}

/// TnProjectileDataHoming, as a TnProjectileHoming flies with it: pulled towards
/// its target (its targetable place, led by its speed: LeadTargetSpeed,
/// MaxLeadDistance) by HomingForce (or its HomingForceScale by its time in the
/// air, UseHomingForceScale), then within ClosingDistance of it straight at it by
/// ClosingForce, going off ClosingTime after (none: when it strikes).
#[derive(Clone, Debug, Default)]
pub struct HomingData {
    pub force: f32,
    pub force_scale: Option<FloatScaler>,
    pub closing_distance: f32,
    pub closing_force: f32,
    pub closing_time: f32,
    pub lead_target_speed: (f32, f32),
    pub max_lead_distance: f32,
    pub can_lock_on_to_robots: bool,
}

/// A FloatScaler's segments (`<key>.Scales[i]`).
fn read_scaler(b: &Behaviour, key: &str) -> FloatScaler {
    let n = b.n(&format!("{key}.Scales"));
    FloatScaler {
        scales: (0..n)
            .map(|i| {
                let s = |f: &str| format!("{key}.Scales[{i}].{f}");
                (
                    scale_kind(b.get(&s("ScaleType")).unwrap_or("DST_Constant")),
                    b.f(&s("EndPoint"), 0.0),
                    b.f(&s("StartValue"), 0.0),
                    b.f(&s("ScaleFactor"), 1.0),
                )
            })
            .collect(),
    }
}

/// A name that lives as long as the program (the sound cues the data names: the
/// AI's are few, read once).
fn leak(s: &str) -> &'static str {
    if s.is_empty() || s == "None" {
        ""
    } else {
        Box::leak(s.to_string().into_boxed_str())
    }
}

impl ProjectileData {
    /// A projectile's values under `p` (tools/build_ai.py's: a gun's round's under
    /// `Weapon.proj.`, an animation's projectile's with none), none if they have no
    /// class.
    pub fn from_keys(b: &Behaviour, p: &str) -> Option<ProjectileData> {
        let k = |f: &str| format!("{p}{f}");
        let class = b.get(&k("class"))?.to_string();
        let homing = b
            .get(&k("supers"))
            .is_some_and(|s| s.split_whitespace().any(|c| c == "TnProjectileHoming"))
            .then(|| HomingData {
                force: b.f(&k("HomingForce"), 0.0),
                force_scale: b
                    .b(&k("UseHomingForceScale"), false)
                    .then(|| read_scaler(b, &k("HomingForceScale"))),
                closing_distance: b.f(&k("ClosingDistance"), 0.0),
                closing_force: b.f(&k("ClosingForce"), 0.0),
                closing_time: b.f(&k("ClosingTime"), 0.0),
                lead_target_speed: (
                    b.f(&k("LeadTargetSpeed.Min"), 0.0),
                    b.f(&k("LeadTargetSpeed.Max"), 0.0),
                ),
                max_lead_distance: b.f(&k("MaxLeadDistance"), 0.0),
                can_lock_on_to_robots: b.b(&k("CanLockOnToRobots"), true),
            });
        let light = b.get(&k("mesh.Light.class")).is_some().then(|| {
            (
                b.f(&k("mesh.Light.Radius"), 1024.0),
                b.f(&k("mesh.Light.Brightness"), 1.0),
            )
        });
        let supers = b.get(&k("supers")).unwrap_or("");
        let is = |c: &str| supers.split_whitespace().any(|s| s == c);
        // Its extensions (Extensions[i], each its class).
        let mut shake = None;
        let mut detach = None;
        for i in 0..b.n(&k("Extensions")) {
            let e = |f: &str| k(&format!("Extensions[{i}].{f}"));
            let v3 = |f: &str| {
                let v: Vec<f32> = b
                    .get(&e(f))
                    .unwrap_or("")
                    .split_whitespace()
                    .filter_map(|x| x.parse().ok())
                    .collect();
                if v.len() >= 3 {
                    Vec3::new(v[0], v[1], v[2])
                } else {
                    Vec3::ZERO
                }
            };
            match b.get(&e("class")) {
                Some("TnCameraShakeProjectileExtension") => {
                    shake = Some(crate::riot::ScreenShake {
                        radius: b.f(&e("CameraShakeRadius"), 0.0) / super::ue::UU_PER_M,
                        secs: b.f(&e("CameraShake.TimeDuration"), 1.0),
                        rot_amp: v3("CameraShake.RotAmplitude"),
                        rot_freq: v3("CameraShake.RotFrequency"),
                        loc_amp: v3("CameraShake.LocAmplitude"),
                        loc_freq: v3("CameraShake.LocFrequency"),
                        fov_amp: b.f(&e("CameraShake.FOVAmplitude"), 0.0),
                        fov_freq: b.f(&e("CameraShake.FOVFrequency"), 0.0),
                    });
                }
                Some("TnDetachFXProjectileExtension") => {
                    let fx = b.obj(&e("DetachFXTemplate")).unwrap_or("");
                    detach = Some((
                        fx.rsplit('.').next().unwrap_or(fx).to_string(),
                        leak(b.get(&e("DetachAudioCue")).unwrap_or("")),
                    ));
                }
                _ => {}
            }
        }
        let sticky = is("TnProjectileGrenadeLauncher").then(|| StickyData {
            fuse: (
                b.f(&k("FuseTime.Min"), 0.0),
                b.f(&k("FuseTime.Max"), b.f(&k("FuseTime.Min"), 0.0)),
            ),
            fuse_sound: leak(b.get(&k("mesh.FuseSound")).unwrap_or("")),
            shaken_off: if is("TnProjectileFlyerRocket") {
                detach
            } else {
                None
            },
        });
        Some(ProjectileData {
            class,
            speed: b.f(&k("InitialSpeed"), 1000.0),
            max_speed: b.f(&k("MaxSpeed"), 0.0),
            gravity_scale: b.f(&k("GravityScale"), 0.0),
            damage: b.f(&k("Damage"), 0.0),
            damage_radius: b.f(&k("DamageRadius"), 0.0),
            radius_scale: read_scaler(b, &k("RadiusDamageScale")),
            damage_type: b.obj(&k("DamageType")).unwrap_or("None").to_string(),
            momentum_transfer: b.f(&k("MomentumTransfer"), 0.0),
            ignore_hurt_radius_trace: b.b(&k("IgnoreHurtRadiusTrace"), false),
            homing,
            flight_fx: b.obj(&k("mesh.FlightEffect")).unwrap_or("").to_string(),
            explosion_fx: b.obj(&k("mesh.ExplosionEffect")).unwrap_or("").to_string(),
            explosion_scale: b.f(&k("mesh.ExplosionEffectScale"), 1.0),
            flight_sound: leak(b.get(&k("mesh.FlightSound")).unwrap_or("")),
            explosion_sound: leak(b.get(&k("mesh.ExplosionSound")).unwrap_or("")),
            light,
            sticky,
            shake,
            marker: b.b(&k("ShowMarker"), false),
        })
    }
}

impl Default for GunData {
    fn default() -> Self {
        // TnWeaponBurstRifleAI (BurstRifle_SV_WEPDATA), the soldiers' gun.
        GunData {
            class: "TnWeaponBurstRifleAI".into(),
            interval: (0.11, 0.13, 0.0),
            spread_start_high: false,
            spread: (0.01, 0.05),
            spread_per_shot: 0.005,
            spread_cooldown: 5.0,
            competent_burst: (16, 20),
            competent_pause: (0.0, 0.0),
            incompetent_burst: (5, 8),
            incompetent_pause: (0.0, 0.0),
            clip_size: 8,
            unlimited_reserve: true,
            reload_time: 3.0,
            equip_time: 0.5,
            damage: 4.5,
            range: 20000.0,
            range_scale: FloatScaler {
                scales: vec![
                    (1, 5000.0, 1.0, 1.0),
                    (3, 10000.0, 1.0, 1.0),
                    (0, 0.0, 0.5, 1.0),
                ],
            },
            shots_per_burst: 0,
            burst_delay: 0.0,
            charge: 0.0,
            projectile_speed: 0.0,
            shots_to_fire: 1,
            spread_pattern: Vec::new(),
            base_stance: super::pawn::stance::GUN_ARM_SHOOT_FROM_HIP,
            projectile: None,
            damage_type: "TnDamageTypeBurstRifle".into(),
            momentum: 0.0,
        }
    }
}

impl GunData {
    /// A gun's data from the behaviour's weapon values (tools/build_ai.py's: its
    /// PrimaryWeapon's, `Weapon.<key>`, else the first other weapon it names,
    /// `Weapon@<its key>.<key>`: the Guard's BattleModeWeapon, a jet's
    /// AirToGroundWeapon), the soldiers' rifle's where it has none.
    pub fn from_behaviour(b: &Behaviour) -> GunData {
        GunData::all_from_behaviour(b)
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    /// Every gun the behaviour names, in from_behaviour's order (the one it
    /// starts with first: a jet's AirToGroundWeapon, then its DogfightWeapon), as
    /// the inventory its EquipWeapon finds or creates them in.
    pub fn all_from_behaviour(b: &Behaviour) -> Vec<GunData> {
        let mut out = Vec::new();
        if b.get("Weapon.class").is_some() {
            out.push(GunData::from_prefix(b, "Weapon."));
        }
        let mut others: Vec<&str> = b
            .props
            .keys()
            .filter(|k| {
                k.starts_with("Weapon@") && k.ends_with(".class") && k.matches('.').count() == 1
            })
            .map(|k| k.as_str())
            .collect();
        others.sort_unstable();
        out.extend(
            others
                .iter()
                .map(|k| GunData::from_prefix(b, &k[..k.len() - "class".len()])),
        );
        out
    }

    /// A gun's data from its values under `p` (`Weapon.`, `Weapon@<key>.`).
    pub fn from_prefix(b: &Behaviour, p: &str) -> GunData {
        let mut g = GunData::default();
        let k = |s: &str| format!("{p}{s}");
        if b.get(&k("class")).is_none() {
            return g;
        }
        g.class = b.get(&k("class")).unwrap_or_default().to_string();
        g.interval = (
            b.f(&k("FireIntervals[0].IntervalRange.Min"), g.interval.0),
            b.f(&k("FireIntervals[0].IntervalRange.Max"), g.interval.1),
            b.f(&k("FireIntervals[0].RampTime"), g.interval.2),
        );
        g.spread_start_high = b.get(&k("Spreads[0].Start")) == Some("DWWS_StartHigh");
        g.spread = (
            b.f(&k("Spreads[0].Modifier.Min"), 0.0),
            b.f(&k("Spreads[0].Modifier.Max"), 0.0),
        );
        g.spread_per_shot = b.f(&k("Spreads[0].ModifierChangePerShot"), 0.0);
        g.spread_cooldown = b.f(&k("Spreads[0].Cooldown"), 0.0);
        g.competent_burst = (
            b.i(&k("CompetentBurst.Burst.Min"), 0),
            b.i(&k("CompetentBurst.Burst.Max"), 0),
        );
        g.competent_pause = (
            b.f(&k("CompetentBurst.PauseDuration.Min"), 0.0),
            b.f(&k("CompetentBurst.PauseDuration.Max"), 0.0),
        );
        g.incompetent_burst = (
            b.i(&k("IncompetentBurst.Burst.Min"), 0),
            b.i(&k("IncompetentBurst.Burst.Max"), 0),
        );
        g.incompetent_pause = (
            b.f(&k("IncompetentBurst.PauseDuration.Min"), 0.0),
            b.f(&k("IncompetentBurst.PauseDuration.Max"), 0.0),
        );
        g.clip_size = b.i(&k("ClipSize"), 1).max(1) as u32;
        g.unlimited_reserve = b.b(&k("bHasUnlimitedReserveAmmo"), true);
        g.reload_time = b.f(&k("WeaponReloadAnimTime"), 1.0);
        g.equip_time = b.f(&k("EquipTime"), 0.5);
        // (Arrays of floats as their values in a row: the first fire mode's.)
        let first = |key: &str, default: f32| {
            b.get(&k(key))
                .and_then(|v| v.split_whitespace().next())
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(default)
        };
        g.damage = first("InstantHitDamage", 0.0);
        g.momentum = first("InstantHitMomentum", 0.0);
        if let Some(t) = b.obj(&k("InstantHitDamageTypes[0]")) {
            g.damage_type = t.to_string();
        }
        g.range = first("Range", 20000.0);
        g.range_scale = read_scaler(b, &k("RangeDamageScale"));
        g.shots_per_burst = b.i(&k("ShotsPerBurst"), 0).max(0) as u32;
        g.burst_delay = b.f(&k("BurstDelay"), 0.0);
        if g.class == "TnWeaponGuardianBlaster" {
            g.charge = b.f(&k("def.ChargeDuration"), 0.0);
        }
        g.projectile_speed = b.f(&k("ProjectileSpeed"), 0.0);
        g.shots_to_fire = b.i(&k("NumShotsToFire"), 1).max(1) as u32;
        let pattern: Vec<f32> = b
            .get(&k("SpreadPattern"))
            .map(|v| {
                v.split_whitespace()
                    .filter_map(|x| x.parse::<f32>().ok())
                    .collect()
            })
            .unwrap_or_default();
        g.spread_pattern = pattern.chunks_exact(2).map(|c| (c[0], c[1])).collect();
        if let Some(st) = b.get(&k("BaseStance")) {
            g.base_stance = super::pawn::stance::number(st);
        }
        // A projectile fire mode's round (WeaponFireTypes[0] EWFT_Projectile).
        let projectile_fire = b
            .get(&k("WeaponFireTypes"))
            .and_then(|v| v.split_whitespace().next())
            == Some("EWFT_Projectile");
        if projectile_fire && let Some(proj) = ProjectileData::from_keys(b, &k("proj.")) {
            g.projectile_speed = proj.speed;
            g.projectile = Some(std::sync::Arc::new(proj));
        }
        g
    }

    /// GetInitialSpread: where its spread starts.
    pub fn initial_spread(&self) -> f32 {
        if self.spread_start_high {
            self.spread.1
        } else {
            self.spread.0
        }
    }
}

/// FloatScaler.EDistanceScaleType's number, by its name.
fn scale_kind(name: &str) -> u8 {
    match name {
        "DST_MaxRange" => 0,
        "DST_Constant" => 1,
        "DST_Linear" => 2,
        "DST_ExponentialEaseIn" => 3,
        "DST_ExponentialEaseOut" => 4,
        "DST_ExponentialEaseInOut" => 5,
        _ => 1,
    }
}

/// The gun's state (TnWeapon's, as an AI's fires).
#[derive(Clone, Debug, Default)]
pub struct Gun {
    pub data: std::sync::Arc<GunData>,
    pub equipped: bool,
    pub equip_left: f32,
    /// StartFire held (PendingFire) and the time to its next round.
    pub pending_fire: bool,
    pub firing: bool,
    pub refire_left: f32,
    /// Rounds fired in this firing (its BurstCount) and a burst weapon's delay.
    pub burst_count: u32,
    pub burst_delay_left: f32,
    pub clip: u32,
    pub reload_left: f32,
    pub spread: f32,
    /// The fire interval lerper's time.
    pub interval_time: f32,
    /// Where it is in its spread pattern (SpreadPatternIndex).
    pub spread_pattern_index: usize,
    /// Rounds fired this frame (for the controller's OnWeaponFired and the
    /// shots' effects), and a reload begun since its effects last looked.
    pub fired_now: u32,
    pub reload_started: bool,
    /// TnWeapon's bIsFiring (its effects' firing: the first round's
    /// TnWeaponEventStartFire, StopFireEffects' TnWeaponEventStopFire), and whether
    /// the round being fired started it and whether it stopped this frame.
    pub effects_firing: bool,
    pub start_now: bool,
    pub stopped_now: bool,
    /// The Guardian Blaster's charge: seconds left of it while it charges
    /// (_IsCharging), and whether it began (StartCharging's
    /// TnWeaponEventStartCharging) or ended (StopCharging's) this frame.
    pub charge_left: f32,
    pub charging: bool,
    pub charge_started: bool,
    pub charge_stopped: bool,
    /// A vehicle weapon's meshes shown (the jets' dogfight gun's launchers: hidden
    /// as it is equipped, TnWeaponEffectHideWeapon on TnWeaponEventEquip, and out
    /// from its first StartFire with ammo on, TnWeaponVehicle.CheckForWeaponOut),
    /// and the muzzle its next round leaves from (TnWeaponEffectIncrementMuzzleSocket:
    /// a jet's two by turns).
    pub out: bool,
    pub muzzle_index: usize,
}

impl Gun {
    pub fn equip(&mut self, data: std::sync::Arc<GunData>) {
        self.out = false;
        self.clip = data.clip_size;
        self.spread = if data.spread_start_high {
            data.spread.1
        } else {
            data.spread.0
        };
        self.equip_left = data.equip_time;
        self.data = data;
        self.equipped = true;
    }

    /// Put down (SetCurrentWeapon(None): the AI's guns put down at once).
    pub fn unequip(&mut self) {
        self.pending_fire = false;
        self.firing = false;
        self.stop_charging();
        self.equip_left = 0.0;
        self.equipped = false;
    }

    /// TnWeaponGuardianBlaster.StartCharging: the charge begun (its effects' start
    /// charging event), its shot fired once it is over (FireChargedShot's timer).
    fn start_charging(&mut self) {
        if !self.charging {
            self.charging = true;
            self.charge_started = true;
        }
        self.charge_left = self.data.charge;
    }

    /// StopCharging: the charge over (its effects' stop charging event).
    fn stop_charging(&mut self) {
        if self.charging {
            self.charging = false;
            self.charge_stopped = true;
        }
        self.charge_left = 0.0;
    }

    pub fn is_equipping(&self) -> bool {
        self.equipped && self.equip_left > 0.0
    }

    pub fn is_equipped(&self) -> bool {
        self.equipped && self.equip_left <= 0.0
    }

    /// HasAmmo(0).
    pub fn has_ammo(&self) -> bool {
        self.clip > 0 && self.reload_left <= 0.0
    }

    pub fn reloading(&self) -> bool {
        self.reload_left > 0.0
    }

    /// GetFireInterval (the lerper's value).
    fn fire_interval(&self) -> f32 {
        let (lo, hi, ramp) = self.data.interval;
        if ramp == 0.0 {
            lo
        } else {
            lo + (hi - lo) * (self.interval_time / ramp).min(1.0)
        }
    }

    /// GetSpread (the current spread: no difficulty multipliers on the AI's
    /// guns).
    pub fn current_spread(&self) -> f32 {
        self.spread
    }

    pub fn start_fire(&mut self) {
        self.pending_fire = true;
        if self.has_ammo() {
            self.out = true;
        }
    }

    pub fn stop_fire(&mut self) {
        self.pending_fire = false;
    }

    /// The gun's tick: equipping, reloading, the spread cooling, the fire interval
    /// ramping, and the rounds it fires this frame (each a call to `shoot`).
    pub fn tick(&mut self, dt: f32, mut shoot: impl FnMut(&mut Gun)) {
        self.fired_now = 0;
        if self.equip_left > 0.0 {
            self.equip_left -= dt;
        }
        // CooldownSpread.
        let d = &self.data;
        if d.spread_cooldown > 0.0 {
            let mag = (d.spread.1 - d.spread.0) * dt / d.spread_cooldown;
            if d.spread_start_high {
                self.spread += mag;
            } else {
                self.spread -= mag;
            }
            self.spread = self.spread.clamp(d.spread.0, d.spread.1);
        }
        // UpdateFireInterval: on while firing, back while not.
        let ramp = d.interval.2;
        self.interval_time =
            (self.interval_time + if self.firing { dt } else { -dt }).clamp(0.0, ramp.max(0.0));
        if self.reload_left > 0.0 {
            self.reload_left -= dt;
            if self.reload_left <= 0.0 {
                self.clip = self.data.clip_size;
                self.reload_left = 0.0;
            }
            self.firing = false;
            self.stop_charging();
            return;
        }
        if self.burst_delay_left > 0.0 {
            self.burst_delay_left -= dt;
        }
        self.refire_left -= dt;
        if !self.is_equipped() {
            self.firing = false;
            self.stop_charging();
            return;
        }
        if !self.firing {
            // Active.BeginFire: a round at once, if the last one's interval has
            // passed and a burst weapon's delay is over; the Guardian Blaster's
            // WeaponFiring.BeginState charges first instead.
            if self.pending_fire && self.refire_left <= 0.0 && self.burst_delay_left <= 0.0 {
                if self.clip == 0 {
                    self.begin_reload();
                    return;
                }
                self.firing = true;
                self.burst_count = 0;
                if self.data.charge > 0.0 {
                    self.start_charging();
                    return;
                }
                self.fire(&mut shoot);
                self.refire_left = self.fire_interval();
            }
            return;
        }
        if self.data.charge > 0.0 {
            // The Guardian Blaster's WeaponFiring.EndFire: let go, it stops
            // (charging or firing) and goes back to Active at once.
            if !self.pending_fire {
                self.firing = false;
                self.stop_charging();
                return;
            }
            // FireChargedShot: charged, its first round (StopCharging,
            // FireAmmunition, TimeWeaponFiring).
            if self.charging {
                self.charge_left -= dt;
                if self.charge_left > 0.0 {
                    return;
                }
                self.stop_charging();
                self.fire(&mut shoot);
                self.refire_left = self.fire_interval();
                return;
            }
        }
        // WeaponFiring: a round each interval while it should refire.
        while self.refire_left <= 0.0 {
            let refire = if self.data.shots_per_burst > 0 {
                self.burst_count < self.data.shots_per_burst
            } else {
                self.pending_fire
            };
            if !refire || self.clip == 0 {
                self.firing = false;
                if self.data.shots_per_burst > 0 {
                    self.burst_delay_left = self.data.burst_delay;
                }
                if self.clip == 0 {
                    self.begin_reload();
                }
                return;
            }
            self.fire(&mut shoot);
            self.refire_left += self.fire_interval();
        }
    }

    /// The frame's end for its effects: StopFireEffects once it has stopped
    /// firing (a round an animation fired alone stops at once).
    pub fn end_frame(&mut self) {
        if self.effects_firing && !self.firing {
            self.effects_firing = false;
            self.stopped_now = true;
        }
    }

    /// Weapon.FireAmmunition: a round at once (an animation's notify), off the
    /// clip, apart from any burst.
    pub fn fire_round(&mut self, mut shoot: impl FnMut(&mut Gun)) {
        let count = self.burst_count;
        self.fire(&mut shoot);
        self.burst_count = count;
    }

    /// HmWeapon.Reload (the pawn's ReloadWeapon): in hand and not full, its clip
    /// refilled over its reload time.
    pub fn reload(&mut self) {
        if self.is_equipped() && self.reload_left <= 0.0 && self.clip < self.data.clip_size {
            self.firing = false;
            self.begin_reload();
        }
    }

    fn begin_reload(&mut self) {
        if self.data.unlimited_reserve {
            self.reload_left = self.data.reload_time.max(0.01);
            // (Its TnWeaponEventReload, for its effects.)
            self.reload_started = true;
        }
    }

    fn fire(&mut self, shoot: &mut impl FnMut(&mut Gun)) {
        self.burst_count += 1;
        self.clip = self.clip.saturating_sub(1);
        self.fired_now += 1;
        // OnPlayFireEffects: StartFire with the first round.
        self.start_now = !self.effects_firing;
        self.effects_firing = true;
        shoot(self);
        self.start_now = false;
        // InstantFire's IncrementSpread.
        let d = &self.data;
        if d.spread_cooldown > 0.0 {
            let step = if d.spread_start_high { -1.0 } else { 1.0 };
            self.spread = (self.spread + d.spread_per_shot * step).clamp(d.spread.0, d.spread.1);
        }
    }
}

/// TnWeapon.AddSpread with a spread pattern: the aim moved right and up by the
/// pattern's offset (scaled by its spread over its initial spread).
pub fn add_pattern_spread(aim: Vec3, offset: (f32, f32), modifier: f32) -> Vec3 {
    let x = aim.normalize_or_zero();
    let to_rad = std::f32::consts::TAU / 65536.0;
    let (sp, cp) = (ue::pitch_of(x) * to_rad).sin_cos();
    let (sy, cy) = (ue::yaw_of(x) * to_rad).sin_cos();
    let y = Vec3::new(-sy, cy, 0.0);
    let z = Vec3::new(-cy * sp, -sy * sp, cp);
    (x + y * offset.0 * modifier + z * offset.1 * modifier).normalize_or_zero()
}

/// UE3's Weapon.AddSpread: the aim thrown off at random within the spread.
pub fn add_spread(aim: Vec3, spread: f32, rng: &mut Rng) -> Vec3 {
    if spread == 0.0 {
        return aim;
    }
    let x = aim.normalize_or_zero();
    let up = if x.z.abs() > 0.99 { Vec3::X } else { Vec3::Z };
    let y = up.cross(x).normalize_or_zero() * -1.0;
    let z = x.cross(y).normalize_or_zero();
    let rand_y = rng.frand() - 0.5;
    let rand_z = (0.5 - rand_y * rand_y).max(0.0).sqrt() * (rng.frand() - 0.5);
    (x + y * rand_y * spread + z * rand_z * spread).normalize_or_zero()
}

/// TnAiWeaponController's state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShooterState {
    #[default]
    Inactive = 0,
    Waiting = 1,
    Equipping = 2,
    Shooting = 3,
    Unequipping = 4,
}

/// TnAiWeaponController.
#[derive(Clone, Debug, Default)]
pub struct Shooter {
    pub state: ShooterState,
    pub use_unrestrained_fire_always: bool,
    pub use_unrestrained_fire: bool,
    pub time_before_next_burst: f32,
    pub burst_shots_remaining: i32,
}

impl Shooter {
    pub fn set_unrestrained_fire(&mut self, v: bool) {
        self.use_unrestrained_fire = self.use_unrestrained_fire_always || v;
    }
}

/// TnAiWeaponAimer.
#[derive(Clone, Debug)]
pub struct Aimer {
    /// ETnAimingMode: 0 at a location, 1 in a direction (relative to the pawn), 2
    /// along the weapon.
    pub mode: u8,
    pub target_location: Vec3,
    /// The direction in the pawn's space (pitch, yaw rotation units).
    pub target_direction: (f32, f32),
    pub active: bool,
    pub threshold: f32,
    /// The direction a bad shot goes off the target (in its frame).
    pub offset_direction: Vec3,
    /// Where the gun points now: the aim offset's turn towards the aim direction
    /// (pitch, yaw, rotation units, relative to the pawn).
    pub current: (f32, f32),
}

impl Default for Aimer {
    fn default() -> Self {
        Aimer {
            mode: 1,
            target_location: Vec3::ZERO,
            target_direction: (0.0, 0.0),
            active: false,
            threshold: 0.939,
            offset_direction: Vec3::Y,
            current: (0.0, 0.0),
        }
    }
}

impl Aimer {
    pub fn aim_at_location(&mut self, p: Vec3) {
        self.mode = 0;
        self.target_location = p;
    }

    pub fn aim_in_direction(&mut self, pitch: f32, yaw: f32) {
        self.mode = 1;
        self.target_direction = (pitch, yaw);
    }

    /// Activate: a new way to miss.
    pub fn activate_with(&mut self, rng: &mut Rng) {
        let angle = rng.frand() * std::f32::consts::TAU;
        self.offset_direction = Vec3::new(0.0, angle.sin(), angle.cos());
        self.active = true;
    }

    pub fn activate(&mut self) {
        self.active = true;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }
}

/// The aimer's AimDirection (a unit vector) for a pawn.
pub fn aim_direction(p: &Pawn, muzzle: Vec3) -> Vec3 {
    match p.aimer.mode {
        0 => ue::normal(p.aimer.target_location - muzzle),
        1 => ue::vec_of(
            p.aimer.target_direction.0,
            p.yaw + p.aimer.target_direction.1,
        ),
        _ => gun_direction(p),
    }
}

/// The aimer's AimLocation.
pub fn aim_location(p: &Pawn) -> Vec3 {
    match p.aimer.mode {
        0 => p.aimer.target_location,
        1 => {
            p.loc
                + ue::vec_of(
                    p.aimer.target_direction.0,
                    p.yaw + p.aimer.target_direction.1,
                ) * 1000.0
        }
        _ => p.loc + gun_direction(p) * 1000.0,
    }
}

/// Where the gun points: its muzzle's rotation (the pawn's facing turned by the
/// aim offset the animation plays).
pub fn gun_direction(p: &Pawn) -> Vec3 {
    ue::vec_of(p.aimer.current.0, p.yaw + p.aimer.current.1)
}

/// IsAiming: the gun points within the threshold of where it should.
pub fn is_aiming_at_target(p: &Pawn, muzzle: Vec3) -> bool {
    if !p.aimer.active {
        return false;
    }
    aim_direction(p, muzzle).dot(gun_direction(p)) > p.aimer.threshold
}

/// CalculateBadAimPoint: a point off the target's cylinder (1.4 times its size)
/// the way the aimer misses.
pub fn bad_aim_point(
    p: &Pawn,
    good: Vec3,
    target_loc: Vec3,
    target_radius: f32,
    target_height: f32,
) -> Vec3 {
    let frame_yaw = ue::yaw_of(target_loc - p.loc);
    let frame_pitch = ue::pitch_of(target_loc - p.loc);
    let local = to_local(good - target_loc, frame_pitch, frame_yaw) * Vec3::new(0.0, 1.0, 1.0);
    let radius = target_radius * 1.4;
    let height = target_height * 1.4;
    let o = p.aimer.offset_direction;
    let big = f32::MAX;
    let ty1 = if o.y == 0.0 {
        big
    } else {
        (radius - local.y) / o.y
    };
    let ty2 = if o.y == 0.0 {
        big
    } else {
        (-radius - local.y) / o.y
    };
    let tz1 = if o.z == 0.0 {
        big
    } else {
        (height - local.z) / o.z
    };
    let tz2 = if o.z == 0.0 {
        big
    } else {
        (-height - local.z) / o.z
    };
    let ty = ty1.max(ty2);
    let tz = tz1.max(tz2);
    let t = if (ty / radius).abs() < (tz / height).abs() {
        ty
    } else {
        tz
    };
    let local_bad = local + o * t;
    target_loc + from_local(local_bad, frame_pitch, frame_yaw)
}

/// A vector into a frame turned by (pitch, yaw): Vector << Rotator.
pub fn to_local(v: Vec3, pitch: f32, yaw: f32) -> Vec3 {
    let (x, y, z) = axes(pitch, yaw);
    Vec3::new(v.dot(x), v.dot(y), v.dot(z))
}

/// Vector >> Rotator.
pub fn from_local(v: Vec3, pitch: f32, yaw: f32) -> Vec3 {
    let (x, y, z) = axes(pitch, yaw);
    x * v.x + y * v.y + z * v.z
}

/// GetAxes of a rotator (pitch, yaw; no roll).
pub fn axes(pitch: f32, yaw: f32) -> (Vec3, Vec3, Vec3) {
    let x = ue::vec_of(pitch, yaw);
    let y = ue::vec_of(0.0, yaw + 16384.0);
    (x, y, x.cross(y))
}

/// TnHtnAiController.Tick's WeaponController.Update(DeltaTime, Competency).
pub fn update_controller(brain: &mut Brain, ctx: &mut Ctx, competency: f32) {
    let p = &mut *ctx.pawn;
    let rng = &mut brain.core.rng;
    let s = &mut brain.shooter;
    match s.state {
        ShooterState::Inactive => {}
        ShooterState::Waiting => {
            // UpdateWaiting.
            if !p.weapon.is_equipped() {
                s.state = ShooterState::Equipping;
                return;
            }
            s.time_before_next_burst -= ctx.dt;
            if s.time_before_next_burst > 0.0 && !s.use_unrestrained_fire {
                return;
            }
            p.weapon.start_fire();
            let d = &p.weapon.data;
            let incompetent = rand_int(rng, d.incompetent_burst) as f32;
            let competent = rand_int(rng, d.competent_burst) as f32;
            s.burst_shots_remaining = ue::lerp(incompetent, competent, competency) as i32;
            s.state = ShooterState::Shooting;
        }
        ShooterState::Shooting => {
            // UpdateShooting.
            p.weapon.start_fire();
            if s.use_unrestrained_fire {
                return;
            }
            if s.burst_shots_remaining > 0 {
                return;
            }
            p.weapon.stop_fire();
            let d = &p.weapon.data;
            let a = rng.range(d.incompetent_pause.0, d.incompetent_pause.1);
            let b = rng.range(d.competent_pause.0, d.competent_pause.1);
            s.time_before_next_burst = ue::lerp(a, b, competency);
            s.state = ShooterState::Waiting;
        }
        ShooterState::Equipping => {
            if p.weapon.equipped {
                s.state = ShooterState::Waiting;
            }
        }
        ShooterState::Unequipping => {
            if !p.weapon.equipped {
                s.state = ShooterState::Inactive;
            }
        }
    }
}

/// RandomIntInRange.
fn rand_int(rng: &mut Rng, (lo, hi): (i32, i32)) -> i32 {
    if hi <= lo {
        return lo;
    }
    lo + rng.rand(hi - lo + 1)
}

/// TnAiWeaponController.EquipWeapon: its gun in hand (its burst data with it);
/// another class of gun put down first (SetCurrentWeapon).
pub fn equip(p: &mut Pawn, data: std::sync::Arc<GunData>) {
    if p.weapon.equipped && std::sync::Arc::ptr_eq(&p.weapon.data, &data) {
        return;
    }
    if p.weapon.equipped && p.weapon.data.class != data.class {
        p.weapon.unequip();
    }
    p.weapon.equip(data);
}

/// UnEquipWeapon: its gun put away.
pub fn unequip(brain: &mut Brain, p: &mut Pawn) {
    p.weapon.unequip();
    brain.shooter.state = ShooterState::Unequipping;
}

/// get_WeaponEquipped / get_WeaponUnequipped.
pub fn weapon_equipped(p: &Pawn) -> bool {
    p.weapon.equipped && !p.weapon.is_equipping()
}

/// The controller's Activate (behaviour flag 8 on).
pub fn activate(brain: &mut Brain) {
    brain.shooter.state = ShooterState::Waiting;
}

/// Deactivate (behaviour flag 8 off).
pub fn deactivate(brain: &mut Brain, p: &mut Pawn) {
    p.weapon.stop_fire();
    brain.shooter.set_unrestrained_fire(false);
    brain.shooter.state = ShooterState::Inactive;
}
