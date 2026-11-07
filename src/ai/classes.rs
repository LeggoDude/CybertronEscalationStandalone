//! Each enemy class's blueprint as it sets its AI up (Tn<Kind>CharacterBlueprint):
//! OnApplyCharacterProperties (its pawn's sight and health, its motion profiles
//! (CreateMotionController), its gun, its stance, its randomness and the like) and
//! OnPopulateSensorCollection (its sensors, each made as its Create*Sensor makes
//! it, in its update group). Escalation's own parts (IsEsc) are always in.

use std::sync::Arc;

use bevy::prelude::*;

use super::brain::{Brain, Ctx};
use super::data::Behaviour;
use super::motion::MotionProfile;
use super::pawn::{Pawn, stance};
use super::sensors::{
    self as s, ClearChannelSensor, CompetencySensor, EnemyStatusSensor, HearingSensor,
    LosSearchSensor, PainSensor, PathingSensor, Rpn, SearchSensor, SquadSensor, ThreatEval,
    ThreatSensor, VisionProfile, VisionSensor, WeaponSensor, register,
};
use super::shared::Shared;
use super::tactical::{TacticalAttack, TacticalEval as T};
use super::ue;
use super::weapon::GunData;

/// The blueprint classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Soldier,
    SharpShooter,
    Shotgunner,
    RocketTrooper,
    Scrapper,
    Swarmer,
    HeavyGunner,
    Titan,
    Guard,
    Leaper,
    /// The Titan's drones (their characters: TnSupportDroneCharacter and
    /// TnExplosiveDroneCharacter, drones.rs).
    SupportDrone,
    ExplosiveDrone,
    /// The Snipers (sniper.rs: their robot form flying, their insect on its perch).
    Sniper,
    /// The jets (jet.rs: their plane, their robot for their pulse attack).
    Jet,
}

impl Class {
    pub fn of(name: &str) -> Option<Class> {
        Some(match name {
            "TnSoldierCharacterBlueprint" => Class::Soldier,
            "TnSharpShooterCharacterBlueprint" => Class::SharpShooter,
            "TnShotgunnerCharacterBlueprint" => Class::Shotgunner,
            "TnRocketTrooperCharacterBlueprint" => Class::RocketTrooper,
            "TnScrapperCharacterBlueprint" => Class::Scrapper,
            "TnSwarmerCharacterBlueprint" => Class::Swarmer,
            "TnHeavyGunnerCharacterBlueprint" => Class::HeavyGunner,
            "TnTitanCharacterBlueprint" => Class::Titan,
            "TnGuardCharacterBlueprint" => Class::Guard,
            "TnLeaperCharacterBlueprint" => Class::Leaper,
            "TnSupportDroneCharacter" => Class::SupportDrone,
            "TnExplosiveDroneCharacter" => Class::ExplosiveDrone,
            "TnSniperCharacterBlueprint" => Class::Sniper,
            "TnJetCharacterBlueprint" => Class::Jet,
            _ => return None,
        })
    }

    /// The classes whose AI runs here.
    pub fn runs(self) -> bool {
        matches!(
            self,
            Class::Soldier
                | Class::SharpShooter
                | Class::Shotgunner
                | Class::RocketTrooper
                | Class::Scrapper
                | Class::Swarmer
                | Class::HeavyGunner
                | Class::Titan
                | Class::Guard
                | Class::Leaper
                | Class::SupportDrone
                | Class::ExplosiveDrone
                | Class::Sniper
                | Class::Jet
        )
    }
}

/// AI.MakeMotionProfile.
fn profile(speed: f32, accel: f32, turn: f32) -> MotionProfile {
    MotionProfile {
        max_linear_speed: speed as i32,
        max_linear_acceleration: accel as i32,
        max_angular_speed: turn as i32,
    }
}

/// AI.MakeVisionProfile (its half height none unless given).
fn vision_profile(fov: f32, distance: f32, dpd: f32, ppd: f32, half_height: f32) -> VisionProfile {
    VisionProfile {
        fov,
        distance,
        half_height,
        distance_perception_delay: dpd,
        peripheral_perception_delay: ppd,
    }
}

/// OnApplyCharacterProperties, with its CreateMotionController.
pub fn apply_properties(
    class: Class,
    brain: &mut Brain,
    pawn: &mut Pawn,
    shared: &mut Shared,
    gun: Arc<GunData>,
) {
    let b = brain.core.b.clone();
    let f = |k: &str, d: f32| b.f(k, d);
    // Its sight and health (the gunners' and the Leaper's sight radius their
    // vision distance; the others keep the pawn's).
    let gunner = matches!(
        class,
        Class::Soldier | Class::SharpShooter | Class::Shotgunner | Class::RocketTrooper
    );
    if gunner
        || matches!(
            class,
            Class::Scrapper | Class::Swarmer | Class::Leaper | Class::Sniper
        )
    {
        pawn.peripheral_vision = (f("VisionCone", 90.0) * 0.5 * ue::DEG_TO_RAD).cos();
    }
    if gunner || class == Class::Sniper {
        pawn.sight_radius = f("VisionDistance", 5000.0);
    }
    pawn.health_max = f("Health", pawn.health_max);
    // Its chassis's movement (TnPawn.ApplyTransformer, its physics service's):
    // its air speed, terminal velocity, mass and momentum blueprint.
    pawn.air_speed = f("Chassis.AirSpeed", pawn.air_speed);
    pawn.terminal_velocity = f("Chassis.TerminalVelocity", 8000.0);
    pawn.mass = f("Chassis.Mass", 100.0);
    pawn.landing_anims = super::pawn::LandingAnim::for_type(&b);
    if b.obj("Chassis.MomentumBlueprint").is_some() {
        let params = |k: &str| {
            ["Forward", "Neutral", "Backward"]
                .map(|p| f(&format!("Chassis.MomentumBlueprint.{k}.{p}"), 0.0))
        };
        pawn.momentum = Some(super::pawn::Momentum {
            on_ground: params("OnGround"),
            in_air: params("InAir"),
        });
    }
    let m = &mut pawn.motion;
    match class {
        Class::Soldier | Class::SharpShooter | Class::RocketTrooper => {
            let patrol = f("PatrolSpeed", 600.0);
            let tactical = f("TacticalSpeed", 1200.0);
            let sprint = f("SprintSpeed", 2100.0);
            m.set_profile(0, profile(patrol, patrol * 3.0, 180.0));
            m.set_profile(1, profile(tactical, tactical * 3.0, 180.0));
            m.set_profile(2, profile(sprint, sprint * 3.0, 270.0));
        }
        Class::Shotgunner => {
            let sprint = f("SprintSpeed", 1800.0);
            m.set_profile(0, profile(f("PatrolSpeed", 600.0), 1500.0, 90.0));
            m.set_profile(1, profile(sprint, 4000.0, 270.0));
            m.set_profile(2, profile(sprint, 4000.0, 270.0));
        }
        Class::Scrapper => {
            // (Its speeds and turn rate as AI.MakeMotionProfile's ints take them.)
            let ground = f("GroundSpeed", 1800.0).trunc();
            let fire = f("OnFireGroundSpeed", 3500.0).trunc();
            let yaw = f("YawRate", 360.0).trunc();
            m.set_profile(0, profile((ground * 0.4).trunc(), ground * 3.0, yaw));
            m.set_profile(1, profile(ground, ground * 3.0, yaw));
            m.set_profile(2, profile(fire, fire * 3.0, yaw));
            m.face_focus_range = f("FaceFocusRange", 3500.0);
        }
        Class::Swarmer => {
            // (Its ground speed at its walk and its run, its on-fire speed for its
            // sprint.)
            let ground = f("GroundSpeed", 2600.0).trunc();
            let fire = f("OnFireGroundSpeed", 3500.0).trunc();
            let yaw = f("YawRate", 360.0).trunc();
            m.set_profile(0, profile(ground, ground * 3.0, yaw));
            m.set_profile(1, profile(ground, ground * 3.0, yaw));
            m.set_profile(2, profile(fire, fire * 3.0, yaw));
            m.face_focus_range = f("FaceFocusRange", 3500.0);
        }
        Class::Guard => {
            // Its investigate form's speed, and its battle form's (both its
            // tactical and its sprint), each at three times that to get up to it,
            // turning twice as fast in battle form.
            let investigate = f("InvestigateSpeed", 500.0);
            let battle = f("BattleSpeed", 1600.0);
            let turn = f("MovingTurnRate", 60.0);
            m.idle_max_angular_speed = f("IdleTurnRate", 220.0) as i32;
            m.set_profile(0, profile(investigate, investigate * 3.0, turn));
            m.set_profile(1, profile(battle, battle * 3.0, turn * 2.0));
            m.set_profile(2, profile(battle, battle * 3.0, turn * 2.0));
            // SetSlowdownCondition (its multiplier as it is) and SetTakeOffCondition.
            m.slowdown_distance = f("SlowDownDistance", 0.0).trunc();
            m.slowdown_speed_multiplier = f("SlowDownSpeedMultiplier", 0.0);
            m.take_off_turn_rate = f("TakeOffTurnRate", 0.0);
            m.cos_take_off_activation_angle =
                (f("TakeOffActivationAngle", 0.0) * ue::DEG_TO_RAD).cos();
            m.cos_take_off_complete_angle = (f("TakeOffCompleteAngle", 0.0) * ue::DEG_TO_RAD).cos();
        }
        Class::HeavyGunner | Class::Titan => {
            // (Their walk at every speed.)
            let walk = f("WalkSpeed", 700.0).trunc();
            let turn = f("MovingTurnRate", 90.0).trunc();
            m.idle_max_angular_speed = f("IdleTurnRate", 120.0) as i32;
            for speed in 0..3 {
                m.set_profile(speed, profile(walk, (walk * 1.5).trunc(), turn));
            }
            // SetSlowdownCondition and SetTakeOffCondition.
            m.slowdown_distance = f("SlowDownDistance", 0.0).trunc();
            m.slowdown_speed_multiplier = f("SlowDownSpeed", 0.0) / walk;
            m.take_off_turn_rate = f("TakeOffTurnRate", 0.0);
            m.cos_take_off_activation_angle =
                (f("TakeOffActivationAngle", 0.0) * ue::DEG_TO_RAD).cos();
            m.cos_take_off_complete_angle = (f("TakeOffCompleteAngle", 0.0) * ue::DEG_TO_RAD).cos();
        }
        Class::Leaper => {
            // SetupRegularMotion: its one profile (its int GroundSpeed, three
            // times that to get up to it, 180 degrees a second), and it turns at
            // its YawRate.
            let ground = f("GroundSpeed", 1000.0).trunc();
            m.set_profile(0, profile(ground, ground * 3.0, 180.0));
        }
        Class::Sniper => {
            // Its own speeds (no motion profiles: its operators drive it): its
            // AccelRate, its flight's (FlyingSpeed: BaseAirSpeed) and its crawl's
            // (CrawlingSpeed: BaseGroundSpeed); turning 30° a second
            // (RotationRate), rolling to its desired rotation.
            pawn.accel_rate = f("AccelRate", 5000.0);
            pawn.air_speed = f("FlyingSpeed", 5000.0);
            pawn.ground_speed = f("CrawlingSpeed", 7500.0);
            pawn.rotation_rate = 30.0 * ue::ROT_PER_DEG;
            // (No motion controller: TnSniperCharacterBlueprint makes none, so a
            // behaviour taking motion away, which stops one, leaves these be. Its
            // motion profiles, none, once took its acceleration to nothing the
            // first time it left a wall: it flew on in a straight line, and slowed
            // to a crawl in the air by the first wall it touched, it hung there.)
            pawn.motion.none = true;
        }
        Class::Jet => super::jet::apply_properties(brain, pawn, &b),
        Class::SupportDrone | Class::ExplosiveDrone => {
            // CreateMotionController: a TnAiHoverPlaneMotionController (no
            // avoidance, MaxZOffset 300, no slowing as it stops), its profile its
            // HoverSpeed both ways (an int: cast44) at 180 degrees a second; its
            // controller's RotationRate none (its plane form's rigid body turns it).
            let speed = f("HoverSpeed", 4000.0).trunc();
            let at = pawn.loc;
            let yaw = pawn.yaw;
            pawn.plane = Some(Box::new(super::drones::PlaneForm::new(
                profile(speed, speed, 180.0),
                300.0,
                at,
                yaw,
            )));
            pawn.rotation_rate = 0.0;
            // (Its rigid body meets the level, not the pawns' cylinders: it
            // neither blocks them nor is pushed off them.)
            pawn.block_actors = false;
        }
    }
    if class == Class::Leaper {
        pawn.rotation_rate = f("YawRate", 180.0) * ue::ROT_PER_DEG;
    }
    // Its gun (the Rocket Trooper's taken in hand by its plans: its EquipWeapon
    // tasks; the melee kinds have none).
    if class == Class::RocketTrooper || b.get("Weapon.class").is_none() {
        pawn.weapon.data = gun;
        pawn.weapon.equipped = false;
    } else {
        pawn.weapon.equip(gun);
    }
    match class {
        Class::Soldier => {
            brain.core.combat_alert_level = 2;
            if let super::shared::ActionKind::ThrowGrenade {
                min_throw_interval, ..
            } = &mut shared.squad.actions[super::shared::SAT_THROW_GRENADE].kind
            {
                *min_throw_interval = f("GrenadeThrowInterval", *min_throw_interval);
            }
        }
        Class::SharpShooter => {
            brain.core.combat_alert_level = 2;
            // (Its cloak: its ability, unlimited.)
            pawn.cloak.has = true;
            // One in five switch hands after 1.2 s.
            if brain.core.rng.frand() < 0.2 {
                pawn.switch_handedness_in = Some(1.2);
            }
        }
        Class::RocketTrooper => {
            brain.core.combat_alert_level = 2;
            brain.core.competency = 1.0;
            if brain.core.rng.frand() < 0.2 {
                pawn.switch_handedness_in = Some(1.2);
            }
        }
        Class::HeavyGunner | Class::Titan => {
            // (Its look-at aim animator: TnAiLookAtAimAnimator.)
            pawn.aim_animator = super::pawn::AimAnimator::LookAt;
            brain.core.combat_alert_level = 1;
            if class == Class::HeavyGunner {
                pawn.health_status_fx = health_status_fx(&b);
            }
        }
        Class::Guard => {
            // Its forms (TnGuardForms) and its look-at aim animator; it fires
            // without holding back (UseUnrestrainedFireAlways); it starts in
            // investigate form (PrepareToActivateForm and ActivateForm 0), not yet
            // alert.
            pawn.guard = Some(Box::new(super::guard::GuardForms::new(&b)));
            pawn.aim_animator = super::pawn::AimAnimator::LookAt;
            pawn.aim_offset_scale = 1.0;
            brain.shooter.use_unrestrained_fire_always = true;
            brain.shooter.set_unrestrained_fire(true);
            brain.core.combat_alert_level = 2;
            super::guard::prepare_to_activate_form(brain, pawn, super::guard::FORM_INVESTIGATE);
            super::guard::activate_form(brain, pawn, super::guard::FORM_INVESTIGATE);
            brain.core.set_ws(super::htn::WS_ALERT_LEVEL, 0);
        }
        Class::Leaper => {
            // It sees from its lumbar's armour (C_Lumbar02Robo04_XT, along its y
            // axis); blows from behind it do three times the damage, a melee
            // blow's fifteen times that (its buffs: AddBuffs). (Its MaxFallSpeed
            // is a million: no fall hurts it; its shield bones block blows for
            // its hit effects only.)
            pawn.view_bone = Some(Arc::from("C_Lumbar02Robo04_XT"));
            pawn.view_bone_axis = 1;
            pawn.from_behind = (3.0, 15.0);
        }
        Class::SupportDrone => {
            // Its aura (TnBuffAuraSupportDroneDamage) on itself. (It takes every
            // blow in full: its AllowDamageFromTypes's DisallowedDamageModifier,
            // its DamageModifier, is 1.)
            pawn.drone_aura = true;
        }
        Class::ExplosiveDrone => {
            // Its flight's glow on its sockets. (Its SpawnDamageModifier, 0, holds
            // only while a spawn animation plays: its Titan's give it none.)
            super::drones::play_flight_effects(&b, brain.core.entity, pawn, shared);
        }
        Class::Sniper => {
            // Its data store (TnSniperAiDataStore) and aim animator
            // (TnSniperAimAnimator); no blow moves it (_TakeDamageMomentumScale 0).
            // (Its damage modifier ignores damage of TnDamageType itself, which no
            // blow is.)
            brain.sensors.sniper_store = Some(super::sniper::Store::default());
            pawn.aim_animator = super::pawn::AimAnimator::Sniper(Vec2::ZERO);
            pawn.momentum_scale = 0.0;
        }
        Class::Scrapper | Class::Swarmer => {
            // Its squad's turns at the enemy (the melee kinds' circle and the time
            // between their attacks), never alert.
            shared.squad.melee_range = f("KungFuCircleRange", 1500.0).trunc();
            shared.squad.time_between_swarmer_attacks = (
                f("TimeBetweenSwarmerAttacks.Min", 0.5),
                f("TimeBetweenSwarmerAttacks.Max", 5.0),
            );
            brain.core.combat_alert_level = 0;
            brain.core.set_ws(super::htn::WS_ALERT_LEVEL, 0);
        }
        _ => {}
    }
    // The gunners (and the Leaper) start in the patrol stance.
    if matches!(
        class,
        Class::Soldier
            | Class::SharpShooter
            | Class::Shotgunner
            | Class::RocketTrooper
            | Class::Leaper
    ) {
        pawn.stance.clear();
        pawn.stance.push(stance::PATROL);
    }
    brain.core.randomness = match class {
        Class::Scrapper => 0.3,
        _ => f("Randomness", 0.0),
    };
    pawn.melee = super::melee::Melee::for_type(&b);
    // A Swarmer's override (OnApplyCharacterBlueprintOverride: Escalation's
    // SwarmerLite) gives it TnBuffModifySwarmerMeleeDamage: its melee blows do
    // its FloatModifier's 0.14 of their damage (TnMeleeManager's
    // MeleeDamageModifier).
    if class == Class::Swarmer && b.get("BehaviorBlueprintOverride").is_some() {
        pawn.melee.damage_scale *= SWARMER_MELEE_DAMAGE;
    }
}

/// TnBuffModifySwarmerMeleeDamage's FloatModifier.
const SWARMER_MELEE_DAMAGE: f32 = 0.14;

/// OnPopulateSensorCollection.
pub fn populate(class: Class, b: &mut Brain, ctx: &mut Ctx) {
    match class {
        Class::Soldier => soldier(b, ctx),
        Class::SharpShooter => sharp_shooter(b, ctx),
        Class::Shotgunner => shotgunner(b, ctx),
        Class::RocketTrooper => rocket_trooper(b, ctx),
        Class::Scrapper => scrapper(b, ctx),
        Class::Swarmer => swarmer(b, ctx),
        Class::HeavyGunner | Class::Titan => heavy(class, b, ctx),
        Class::Guard => guard(b, ctx),
        Class::Leaper => leaper(b, ctx),
        Class::SupportDrone => support_drone(b, ctx),
        Class::ExplosiveDrone => explosive_drone(b, ctx),
        Class::Sniper => sniper(b, ctx),
        Class::Jet => super::jet::populate(b, ctx),
    }
}

/// A behaviour's name kept for as long as the game runs (the sensors' animation
/// names are static).
fn intern(name: &str) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static NAMES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut names = NAMES.get_or_init(Default::default).lock().expect("names");
    if let Some(n) = names.get(name) {
        return n;
    }
    let n: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.insert(name.to_string(), n);
    n
}

/// TnSniperCharacterBlueprint: it hears a little, sees far all round, goes for the
/// players (PlayerThreatScore) and whoever hurts it, keeps its sniper points
/// (TnSniperPointSensor), and is knocked off balance by a blow of half its health
/// (its injured sensor's reaction: its HitReaction).
fn sniper(b: &mut Brain, ctx: &mut Ctx) {
    let bp: Arc<Behaviour> = b.core.b.clone();
    let f = |k: &str, d: f32| bp.f(k, d);
    let cone = f("VisionCone", 360.0);
    let distance = f("VisionDistance", 15000.0);
    b.sensors.hearing = Some(HearingSensor::new(0.0, 5.0));
    register(b, ctx, s::S_HEARING, 1, false);
    let mut vision = VisionSensor::default();
    vision.set_profile(0, vision_profile(cone, distance, 0.0, 0.0, 0.0));
    vision.set_profile(1, vision_profile(cone, distance, 0.0, 0.0, 0.0));
    b.sensors.vision = Some(vision);
    register(b, ctx, s::S_VISION, 0, false);
    // CreateThreatSensor (Escalation's).
    b.sensors.threat = Some(rpn_threat(
        0.0,
        vec![
            (ThreatEval::Downed, 1.0),
            (ThreatEval::AbilityAgro, 1.0),
            (ThreatEval::Cloaked { discard: false }, 1.0),
            (ThreatEval::Sentry { discard: false }, 1.0),
            (
                ThreatEval::EnemyTypeDistribution {
                    ai_type: b.core.ai_type,
                },
                1.0,
            ),
            (ThreatEval::DamagedBy, 3.0),
            (
                ThreatEval::PlayerEnemy {
                    score: f("PlayerThreatScore", 0.0),
                },
                1.0,
            ),
            (ThreatEval::TargetDistribution, 0.1),
            (
                ThreatEval::EnemyDistance {
                    max_range: distance,
                },
                1.0,
            ),
        ],
        &[
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
        ],
    ));
    register(b, ctx, s::S_THREAT, 0, false);
    b.sensors.weapon = Some(WeaponSensor {
        check_for_aiming: true,
        check_for_pawns_in_lof: true,
        check_for_los: true,
    });
    register(b, ctx, s::S_WEAPON, 2, true);
    b.sensors.status = Some(EnemyStatusSensor::default());
    register(b, ctx, s::S_ENEMY_STATUS, 2, true);
    b.sensors.sniper_point = Some(super::sniper::Sensor::default());
    register(b, ctx, super::sniper::S_SNIPER_POINT, 2, true);
    b.sensors.squad = Some(SquadSensor);
    register(b, ctx, s::S_SQUAD, 2, false);
    // CreateInjuredSensor: its HitReaction from every side.
    let mut injured = s::InjuredSensor::new(
        f("Health", 200.0) * f("InjuredDamagePercentOfHealthForReaction", 0.5),
    );
    let reaction = intern(bp.get("HitReaction.AnimName").unwrap_or("None"));
    injured.from = [reaction; 4];
    injured.blend_in = f("HitReaction.BlendIn", 0.1);
    injured.blend_out = f("HitReaction.BlendOut", 0.1);
    b.sensors.injured = Some(injured);
    register(b, ctx, s::S_INJURED, 2, false);
    b.sensors.pain = Some(PainSensor::new(false));
    register(b, ctx, s::S_PAIN, 1, false);
}

/// The drones' threat sensor (CreateThreatSensor): the players first, the nearer
/// the better, none cloaked (summed: its evaluators aren't RPN's); the explosive
/// drone's whoever hurts it most, spread among its squad, sentries too.
fn drone_threat(explosive: bool) -> ThreatSensor {
    let mut t = ThreatSensor::new(0.0);
    t.add_evaluator(ThreatEval::PlayerEnemy { score: 1.0 }, 1.0);
    t.add_evaluator(
        ThreatEval::EnemyDistance {
            max_range: 100000.0,
        },
        1.0,
    );
    t.add_evaluator(ThreatEval::Cloaked { discard: true }, 1.0);
    if explosive {
        t.add_evaluator(ThreatEval::DamagedBy, 3.0);
        t.add_evaluator(ThreatEval::TargetDistribution, 0.1);
        t.add_evaluator(ThreatEval::Sentry { discard: false }, 1.0);
    }
    t
}

/// The drones' vision (CreateVisionSensor): all round, 1 km out.
fn drone_vision() -> VisionSensor {
    let mut v = VisionSensor::default();
    v.set_profile(0, vision_profile(360.0, 100000.0, 0.0, 0.0, 0.0));
    v
}

/// TnSupportDroneCharacter.OnPopulateSensorCollection: its threats and sight, its
/// path following HeightAboveGround over its corners (a height in its range),
/// its squad, and its post DistanceFromDefendLocation from its Titan
/// (TnHoverDefendActorSensor).
fn support_drone(b: &mut Brain, ctx: &mut Ctx) {
    let bp: Arc<Behaviour> = b.core.b.clone();
    b.sensors.threat = Some(drone_threat(false));
    register(b, ctx, s::S_THREAT, 1, false);
    b.sensors.vision = Some(drone_vision());
    register(b, ctx, s::S_VISION, 1, false);
    b.sensors.pathing = Some(PathingSensor::new(0.5));
    register(b, ctx, s::S_PATHING, 0, false);
    let mut range = |k: &str| {
        let (lo, hi) = (bp.i(&format!("{k}.Min"), 0), bp.i(&format!("{k}.Max"), 0));
        b.core.rng.int_in_range(lo, hi.max(lo))
    };
    let height = range("HeightAboveGround") as f32;
    let distance = range("DistanceFromDefendLocation") as f32;
    b.sensors.drone_path = Some(super::drones::DronePathFollowing::new(height));
    register(b, ctx, super::drones::S_DRONE_PATH_FOLLOWING, 0, false);
    squad(b, ctx);
    b.sensors.hover_defend = Some(super::drones::HoverDefendActor { distance, height });
    register(b, ctx, super::drones::S_DEFEND_ACTOR, 1, false);
}

/// TnExplosiveDroneCharacter.OnPopulateSensorCollection: its enemy within its
/// TriggerExplosionRadius or not, its threats, sight and hearing, its path
/// following HoverHeight over its corners, its Titan's ground
/// (TnDefendActorStatusSensor), its squad and its LifespanTime.
fn explosive_drone(b: &mut Brain, ctx: &mut Ctx) {
    let bp: Arc<Behaviour> = b.core.b.clone();
    b.sensors.status = Some(EnemyStatusSensor {
        ranges: vec![bp.i("TriggerExplosionRadius", 500) as f32, 100000.0],
        ..Default::default()
    });
    register(b, ctx, s::S_ENEMY_STATUS, 1, false);
    b.sensors.pathing = Some(PathingSensor::new(0.5));
    register(b, ctx, s::S_PATHING, 0, false);
    b.sensors.threat = Some(drone_threat(true));
    register(b, ctx, s::S_THREAT, 1, false);
    b.sensors.vision = Some(drone_vision());
    register(b, ctx, s::S_VISION, 1, false);
    b.sensors.hearing = Some(HearingSensor::new(0.0, 0.0));
    register(b, ctx, s::S_HEARING, 2, false);
    b.sensors.drone_path = Some(super::drones::DronePathFollowing::new(
        bp.i("HoverHeight", 256) as f32,
    ));
    register(b, ctx, super::drones::S_DRONE_PATH_FOLLOWING, 0, false);
    b.sensors.defend_status = Some(super::drones::DefendActorStatus::default());
    register(b, ctx, super::drones::S_DEFEND_ACTOR, 1, false);
    squad(b, ctx);
    b.sensors.lifespan = Some(super::drones::Lifespan {
        left: bp.f("LifespanTime", 60.0),
        active: true,
    });
    register(b, ctx, super::drones::S_LIFESPAN, 3, false);
}

/// TnLeaperCharacterBlueprint: it hears well and sees 100 m round most of it (a
/// 270 degree cone), goes for the shield and for whoever flanks it, knows its
/// enemy as within its striking, melee, lunge or leap smash range, leaps from the
/// smash volumes (TnLeaperSmashVolumeSensor), lunges every 3 to 5 s (bit 1 of its
/// special attacks) and vents its boosters at an enemy who keeps close
/// (TnEnemyKitingSensor). No squad sensor: its squad's leap smash turns are its
/// squad manager's.
fn leaper(b: &mut Brain, ctx: &mut Ctx) {
    let bp: Arc<Behaviour> = b.core.b.clone();
    let f = |k: &str, d: f32| bp.f(k, d);
    // (Its VisionDistance: its pawn's SightRadius when its properties were
    // applied, before its vision's profile.)
    let vision_distance = ctx.pawn.sight_radius;
    b.sensors.hearing = Some(HearingSensor::new(10.0, 10.0));
    register(b, ctx, s::S_HEARING, 1, false);
    let cone = f("VisionCone", 270.0);
    let mut vision = VisionSensor::default();
    vision.set_profile(0, vision_profile(cone, 10000.0, 0.0, 0.0, 0.0));
    vision.set_profile(1, vision_profile(cone, 10000.0, 0.0, 0.0, 0.0));
    b.sensors.vision = Some(vision);
    register(b, ctx, s::S_VISION, 0, false);
    // CreateThreatSensor (Escalation's).
    b.sensors.threat = Some(rpn_threat(
        0.0,
        vec![
            (ThreatEval::AbilityAgro, 1.0),
            (ThreatEval::Downed, 1.0),
            (
                ThreatEval::EnemyTypeDistribution {
                    ai_type: b.core.ai_type,
                },
                1.0,
            ),
            (ThreatEval::Sentry { discard: false }, 1.0),
            (ThreatEval::Reachable { discard: true }, 1.0),
            (ThreatEval::Flying { discard: false }, 1.0),
            (ThreatEval::Cloaked { discard: true }, 1.0),
            (ThreatEval::EnemyAttack, 1.0),
            (ThreatEval::DamagedBy, 3.0),
            (
                ThreatEval::EnemyDistance {
                    max_range: vision_distance,
                },
                4.0,
            ),
            (ThreatEval::TargetDistribution, 0.1),
            (
                ThreatEval::Flanked {
                    range_sq: f("MinFlankedFocusDistance", 1000.0).powi(2),
                },
                2.0,
            ),
            (ThreatEval::Visible { discard: false }, 0.1),
        ],
        &[
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
        ],
    ));
    register(b, ctx, s::S_THREAT, 0, false);
    b.sensors.pain = Some(PainSensor::new(false));
    register(b, ctx, s::S_PAIN, 1, false);
    // CreateEnemyStatusSensor: its ranges (ints).
    let lunge = f("MeleeLungeRange", 3500.0).trunc();
    let leap = f("LeapSmashRange", 7000.0).trunc();
    b.sensors.status = Some(EnemyStatusSensor {
        update_navigable_location_near_enemy: true,
        ranges: vec![
            0.0,
            f("StrikingRange", 500.0).trunc(),
            f("MeleeRange", 1100.0).trunc(),
            lunge,
            leap,
        ],
        ..Default::default()
    });
    register(b, ctx, s::S_ENEMY_STATUS, 0, false);
    b.sensors.pathing = Some(PathingSensor::default());
    register(b, ctx, s::S_PATHING, 0, false);
    // CreateLeaperSmashVolumeSensor.
    b.sensors.leap_smash = Some(s::LeapSmashSensor {
        apex_height: f("LeapSmashApexHeight", 2500.0),
        impact_offset: f("LeapSmashLocationOffset", 500.0),
        retarget_max_distance: f("LeapSmashRetargetInAirMaxDistance", 1500.0),
        cooldown_range: (
            f("LeapSmashCooldownRange.Min", 3.0),
            f("LeapSmashCooldownRange.Max", 5.0),
        ),
        range: (lunge, leap),
        ..Default::default()
    });
    register(b, ctx, s::S_LEAPER_SMASH_VOLUME, 0, false);
    b.sensors.channel = Some(ClearChannelSensor {
        trace_extent: bp.v3("ClearAttackChannelTraceExtent", Vec3::ZERO),
        walk_line_only_test: false,
        last_move_direction: 0,
        way_round: s::WayRound::default(),
    });
    register(b, ctx, s::S_CLEAR_ATTACK_CHANNEL, 1, false);
    b.sensors.nav_mesh = Some(s::NavMeshSensor::default());
    register(b, ctx, s::S_NAV_MESH, 1, false);
    // (Its blueprint's reactions, with no interruptive conditions: made with
    // `new` rather than a CreateHitReactionSensor.)
    let mut hit = super::hits::HitReactionSensor::new(&bp, false);
    hit.conditions.clear();
    b.sensors.hit = Some(hit);
    register(b, ctx, s::S_HIT_REACTION, 3, false);
    // Its lunge (special attack 1) every LungeCooldownRange seconds.
    b.sensors.special_attack = Some(s::SpecialAttackCooldownSensor::new(
        (
            f("LungeCooldownRange.Min", 3.0),
            f("LungeCooldownRange.Max", 5.0),
        ),
        1,
    ));
    register(b, ctx, s::S_SPECIAL_ATTACK, 1, false);
    b.sensors.kiting = Some(s::EnemyKitingSensor {
        range: f("EnemyKitingRange", 1200.0),
        duration: f("EnemyKitingDuration", 2.8),
        current: 0.0,
    });
    register(b, ctx, s::S_ENEMY_KITING, 1, false);
}

/// TnGuardCharacterBlueprint: it sees far (200 m, a 160° cone), hears little,
/// tracks its kind's battle form (TnGuardSensor), searches by line of sight, and
/// its special attack comes ready every SpecialAttackCooldown seconds.
fn guard(b: &mut Brain, ctx: &mut Ctx) {
    let bp: Arc<Behaviour> = b.core.b.clone();
    let f = |k: &str, d: f32| bp.f(k, d);
    let cone = f("VisionCone", 160.0);
    let distance = f("VisionDistance", 20000.0);
    b.sensors.update_while_asleep = false;
    b.sensors.search = Some(SearchSensor::default());
    register(b, ctx, s::S_SEARCH, 3, true);
    b.sensors.hearing = Some(HearingSensor::new(0.2, 0.2));
    register(b, ctx, s::S_HEARING, 1, false);
    // CreateVisionSensor: its investigate form's (in patrol, its vision's height
    // its PatrolVisionHeight), its battle form's.
    let mut vision = VisionSensor::default();
    let investigate_delay = f("InvestigatePerceptionDelay", 0.0);
    vision.set_profile(
        0,
        vision_profile(
            cone,
            distance,
            investigate_delay,
            0.0,
            bp.i("PatrolVisionHeight", 0) as f32,
        ),
    );
    vision.set_profile(
        1,
        vision_profile(cone, distance, investigate_delay, 0.0, 0.0),
    );
    vision.set_profile(
        2,
        vision_profile(cone, distance, f("BattlePerceptionDelay", 0.0), 0.0, 0.0),
    );
    b.sensors.vision = Some(vision);
    register(b, ctx, s::S_VISION, 1, false);
    // CreateThreatSensor (Escalation's).
    b.sensors.threat = Some(rpn_threat(
        0.0,
        vec![
            (ThreatEval::AbilityAgro, 1.0),
            (ThreatEval::Downed, 1.0),
            (
                ThreatEval::EnemyTypeDistribution {
                    ai_type: b.core.ai_type,
                },
                1.0,
            ),
            (ThreatEval::Sentry { discard: false }, 1.0),
            (ThreatEval::Cloaked { discard: true }, 1.0),
            (ThreatEval::EnemyAttack, 1.0),
            (ThreatEval::DamagedBy, 3.0),
            (
                ThreatEval::EnemyDistance {
                    max_range: distance,
                },
                4.0,
            ),
            (ThreatEval::TargetDistribution, 0.1),
            (ThreatEval::Flying { discard: false }, 1.0),
            (ThreatEval::Visible { discard: false }, 0.1),
        ],
        &[
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
        ],
    ));
    register(b, ctx, s::S_THREAT, 1, false);
    b.sensors.pain = Some(PainSensor::new(false));
    register(b, ctx, s::S_PAIN, 1, false);
    // (TnWeaponSensor's defaults: every check.)
    b.sensors.weapon = Some(WeaponSensor {
        check_for_aiming: true,
        check_for_pawns_in_lof: true,
        check_for_los: true,
    });
    register(b, ctx, s::S_WEAPON, 2, true);
    // CreateEnemyStatusSensor: in melee, shooting and scanning range, or out.
    b.sensors.status = Some(EnemyStatusSensor {
        ranges: vec![
            f("MeleeRange", 0.0).trunc(),
            f("ShootRange", 0.0).trunc(),
            0.9 * f("InvestigateScanDistance", 0.0).trunc(),
            100000.0,
        ],
        ..Default::default()
    });
    register(b, ctx, s::S_ENEMY_STATUS, 0, true);
    squad(b, ctx);
    // (Its blueprint's reactions, with no interruptive conditions: made with
    // `new` rather than a CreateHitReactionSensor.)
    let mut hit = super::hits::HitReactionSensor::new(&bp, false);
    hit.conditions.clear();
    b.sensors.hit = Some(hit);
    register(b, ctx, s::S_HIT_REACTION, 3, false);
    b.sensors.pathing = Some(PathingSensor::default());
    register(b, ctx, s::S_PATHING, 1, true);
    b.sensors.guard = Some(s::GuardSensor {
        battle_radius: f("BattleFormNearFriendliesRange", 0.0).trunc(),
    });
    register(b, ctx, s::S_GUARD, 2, true);
    // CreatePathFollowingSensor: slower round its turns.
    let mut path = path_following(&bp, None);
    path.set_slow_turn_condition(
        f("SlowTurnSpeedMultiplier", 0.0),
        (f("SlowTurnAngles.Min", 0.0), f("SlowTurnAngles.Max", 0.0)),
    );
    b.sensors.path = Some(path);
    register(b, ctx, s::S_PATH_FOLLOWING, 0, false);
    b.sensors.los_search = Some(LosSearchSensor::new(0.0, Vec3::ZERO, -1.0));
    register(b, ctx, s::S_LOS_SEARCH, 2, false);
    // CreateSpecialAttackSensor: its special attack (index 0) every
    // SpecialAttackCooldown seconds.
    let cooldown = f("SpecialAttackCooldown", 0.0);
    b.sensors.special_attack = Some(s::SpecialAttackCooldownSensor::new((cooldown, cooldown), 0));
    register(b, ctx, s::S_SPECIAL_ATTACK, 2, false);
}

/// The Heavy Gunner's AddHealthStatusFX: its light damage effect once its health
/// is under its first threshold (level 1), its heavy damage effect under its
/// second (level 0), each that names a template.
fn health_status_fx(b: &Behaviour) -> Vec<super::pawn::HealthStatusFx> {
    let mut out = Vec::new();
    for (key, level) in [("LightDamageFx", 1), ("HeavyDamageFx", 0)] {
        if b.obj(&format!("{key}.FXTemplate")).is_none() {
            continue;
        }
        let choosers = (0..b.n(&format!("{key}.ChooserValues")))
            .filter_map(|i| {
                let k = format!("{key}.ChooserValues[{i}]");
                let template = b.obj(&format!("{k}.FXTemplate"))?;
                let required = b
                    .obj(&format!("{k}.RequiredDamageType"))
                    .map(super::hits::damage_kind);
                Some((required, Arc::from(template)))
            })
            .collect();
        out.push(super::pawn::HealthStatusFx {
            level,
            socket: Arc::from(b.get(&format!("{key}.AttachSocket")).unwrap_or("None")),
            choosers,
        });
    }
    out
}

/// CreatePathFollowingSensor: a TnRobotPathFollowingSensor (its start and end
/// distances, if it sets them), its jumps' effects from its blueprint.
fn path_following(bp: &Behaviour, start_end: Option<(f32, f32)>) -> super::path::PathFollowing {
    let mut path = super::path::PathFollowing::default();
    if let Some((start, end)) = start_end {
        path.set_start_and_end_distances(start, end);
    }
    path.min_jump_velocity_for_fx = bp.f("MinJumpVelocityForFx", 0.0);
    path
}

/// A threat sensor of RPN evaluators and operators.
fn rpn_threat(time_to_lose: f32, evals: Vec<(ThreatEval, f32)>, ops: &[Rpn]) -> ThreatSensor {
    let mut t = ThreatSensor::new(time_to_lose);
    for (e, w) in evals {
        t.add_rpn_evaluator(e, w);
    }
    for op in ops {
        t.add_rpn_operator(*op);
    }
    t
}

/// CreateCompetencySensor (the soldiers' kinds': its blueprint's values).
fn competency(bp: &Behaviour) -> CompetencySensor {
    let f = |k: &str, d: f32| bp.f(k, d);
    CompetencySensor {
        incompetent_distance: f("IncompetentDistance", 7000.0),
        max_competence_lower_bound: f("MaxCompetenceLowerBound", 0.5),
        min_competency_upper_bound: f("MinCompetencyUpperBound", 0.3),
        loss_in_competence_per_buddy_alive: f("LossInCompetencePerBuddyAlive", 0.25),
        flanking_half_angle: f("FlankingHalfAngle", 90.0),
        flanking_in_competence: f("FlankingInCompetence", 0.1),
        flanking_in_competence_time: f("FlankingInCompetenceTime", 0.0),
        ..Default::default()
    }
}

/// TnSoldierCharacterBlueprint.
fn soldier(b: &mut Brain, ctx: &mut Ctx) {
    let bp: Arc<Behaviour> = b.core.b.clone();
    let f = |k: &str, d: f32| bp.f(k, d);
    b.sensors.search = Some(SearchSensor::default());
    register(b, ctx, s::S_SEARCH, 3, true);
    b.sensors.hearing = Some(HearingSensor::new(1.0, 10.0));
    register(b, ctx, s::S_HEARING, 1, false);
    b.sensors.pathing = Some(PathingSensor::default());
    register(b, ctx, s::S_PATHING, 0, false);
    b.sensors.path = Some(path_following(&bp, Some((1000.0, 1500.0))));
    register(b, ctx, s::S_PATH_FOLLOWING, 0, false);
    // CreateVisionSensor.
    let cone = f("VisionCone", 90.0);
    let distance = f("VisionDistance", 5000.0);
    let mut vision = VisionSensor::default();
    vision.set_profile(
        0,
        vision_profile(
            cone * 0.5,
            distance * 0.5,
            f("DistancePerceptionDelay", 0.0),
            f("PeripheralPerceptionDelay", 0.0),
            f("PatrolVisionHeight", 0.0),
        ),
    );
    vision.set_profile(
        1,
        vision_profile(
            cone,
            distance,
            f("DistancePerceptionDelay_Prepared", 0.0),
            f("PeripheralPerceptionDelay_Prepared", 0.0),
            0.0,
        ),
    );
    vision.set_profile(
        2,
        vision_profile(
            cone,
            distance,
            f("DistancePerceptionDelay_Aware", 0.0),
            f("PeripheralPerceptionDelay_Aware", 0.0),
            0.0,
        ),
    );
    b.sensors.vision = Some(vision);
    register(b, ctx, s::S_VISION, 0, false);
    // CreateThreatSensor.
    let flank = f("MinFlankedFocusDistance", 0.0);
    let mut threat = rpn_threat(
        f("TimeToLoseEnemies", 5.0),
        vec![
            (ThreatEval::Downed, 1.0),
            (ThreatEval::AbilityAgro, 1.0),
            (ThreatEval::Cloaked { discard: false }, 1.0),
            (ThreatEval::Sentry { discard: false }, 1.0),
            (ThreatEval::DamagedBy, 3.0),
            (
                ThreatEval::Flanked {
                    range_sq: flank * flank,
                },
                4.0,
            ),
            (ThreatEval::VisibleFromCover, 1.0),
            (
                ThreatEval::PlayerEnemy {
                    score: f("PlayerThreatScore", 0.0),
                },
                1.0,
            ),
            (ThreatEval::EnemyAttack, 1.0),
            (ThreatEval::AttackEffectiveness, 1.0),
            (
                ThreatEval::EnemyDistance {
                    max_range: distance,
                },
                1.0,
            ),
        ],
        &[
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Mul,
            Rpn::Add,
            Rpn::Add,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
        ],
    );
    threat.recently_changed_visibility_duration = 4.0;
    threat.effective_attack_range = f("EffectiveThreatRange", 0.0);
    b.sensors.threat = Some(threat);
    register(b, ctx, s::S_THREAT, 1, false);
    b.sensors.pain = Some(PainSensor::new(false));
    register(b, ctx, s::S_PAIN, 1, false);
    b.sensors.hit = Some(super::hits::HitReactionSensor::new(&bp, true));
    register(b, ctx, s::S_HIT_REACTION, 0, false);
    b.sensors.weapon = Some(WeaponSensor {
        check_for_aiming: true,
        check_for_pawns_in_lof: false,
        check_for_los: false,
    });
    register(b, ctx, s::S_WEAPON, 1, false);
    b.sensors.status = Some(EnemyStatusSensor {
        ranges: vec![
            300.0,
            f("MeleeRange", 0.0),
            f("NavigatingShootingRange", 0.0),
            f("MinGrenadeRange", 0.0),
            f("StrafeShootingRange", 0.0),
            100000.0,
        ],
        health_levels: vec![0.2, 1.0],
        ..Default::default()
    });
    register(b, ctx, s::S_ENEMY_STATUS, 0, false);
    b.sensors.channel = Some(ClearChannelSensor::default());
    register(b, ctx, s::S_CLEAR_ATTACK_CHANNEL, 3, false);
    // CreateTacticalAttackSensor.
    let randomness = f("Randomness", 0.0);
    let max_duration = ue::randomize(
        f("MaxDurationAtTactical", 20.0),
        randomness,
        &mut b.core.rng,
    );
    let max_wait = ue::randomize(f("MaxWaitWithNoLOS", 0.0), randomness, &mut b.core.rng);
    let mut tac = TacticalAttack::new(max_duration, max_wait);
    let min_advance = f("MinAdvanceDistance", 0.0);
    let min_tac = f("MinTacticalDistance", 0.0);
    tac.picker.add_rpn_evaluator(T::BattleLine, 1.0);
    tac.picker.add_rpn_evaluator(
        T::Stink {
            duration: f("StinkDuration", 0.0),
        },
        1.0,
    );
    tac.picker.add_rpn_evaluator(
        T::MinEngagementDistance {
            min_distance_sq: min_tac * min_tac,
        },
        1.0,
    );
    tac.picker.add_rpn_evaluator(T::advance(min_advance), 1.0);
    tac.picker.add_rpn_evaluator(T::Occlusion, 1.0);
    tac.picker.add_rpn_evaluator(T::LineOfSight, 1.0);
    tac.picker.add_rpn_evaluator(T::DefendLocation, 1.0);
    tac.picker
        .add_rpn_evaluator(T::close_on_enemy(min_advance, false), 1.0);
    for op in [
        Rpn::Add,
        Rpn::Add,
        Rpn::Mul,
        Rpn::Mul,
        Rpn::Mul,
        Rpn::Mul,
        Rpn::Mul,
    ] {
        tac.picker.add_rpn_operator(op);
    }
    b.sensors.tactical = Some(tac);
    register(b, ctx, s::S_TACTICAL_ATTACK, 3, false);
    b.sensors.competency = Some(competency(&bp));
    register(b, ctx, s::S_COMPETENCY, 1, false);
}

/// TnSharpShooterCharacterBlueprint: the soldiers' senses, its threats' and
/// tactical nodes' evaluators summed (no RPN), no competency.
fn sharp_shooter(b: &mut Brain, ctx: &mut Ctx) {
    let bp: Arc<Behaviour> = b.core.b.clone();
    let f = |k: &str, d: f32| bp.f(k, d);
    b.sensors.search = Some(SearchSensor::default());
    register(b, ctx, s::S_SEARCH, 3, true);
    b.sensors.hearing = Some(HearingSensor::new(1.0, 10.0));
    register(b, ctx, s::S_HEARING, 1, false);
    b.sensors.pathing = Some(PathingSensor::default());
    register(b, ctx, s::S_PATHING, 0, false);
    b.sensors.path = Some(path_following(&bp, Some((1000.0, 1500.0))));
    register(b, ctx, s::S_PATH_FOLLOWING, 0, false);
    // CreateVisionSensor (the soldiers').
    let cone = f("VisionCone", 90.0);
    let distance = f("VisionDistance", 5000.0);
    let mut vision = VisionSensor::default();
    vision.set_profile(
        0,
        vision_profile(
            cone * 0.5,
            distance * 0.5,
            f("DistancePerceptionDelay", 0.0),
            f("PeripheralPerceptionDelay", 0.0),
            f("PatrolVisionHeight", 0.0),
        ),
    );
    vision.set_profile(
        1,
        vision_profile(
            cone,
            distance,
            f("DistancePerceptionDelay_Prepared", 0.0),
            f("PeripheralPerceptionDelay_Prepared", 0.0),
            0.0,
        ),
    );
    vision.set_profile(
        2,
        vision_profile(
            cone,
            distance,
            f("DistancePerceptionDelay_Aware", 0.0),
            f("PeripheralPerceptionDelay_Aware", 0.0),
            0.0,
        ),
    );
    b.sensors.vision = Some(vision);
    register(b, ctx, s::S_VISION, 0, false);
    // CreateThreatSensor: its evaluators summed.
    let mut threat = ThreatSensor::new(f("TimeToLoseEnemies", 5.0));
    let flank = f("MinFlankedFocusDistance", 0.0);
    for (e, w) in [
        (
            ThreatEval::PlayerEnemy {
                score: f("PlayerThreatScore", 0.0),
            },
            1.0,
        ),
        (ThreatEval::EnemyAttack, 1.0),
        (ThreatEval::AttackEffectiveness, 1.0),
        (ThreatEval::Cloaked { discard: true }, 1.0),
        (ThreatEval::VisibleFromCover, 1.0),
        (ThreatEval::DamagedBy, 3.0),
        (
            ThreatEval::Flanked {
                range_sq: flank * flank,
            },
            4.0,
        ),
        (
            ThreatEval::EnemyTypeDistribution {
                ai_type: b.core.ai_type,
            },
            1.0,
        ),
        (ThreatEval::TargetDistribution, 0.1),
        (ThreatEval::Sentry { discard: false }, 1.0),
        (ThreatEval::AbilityAgro, 2.0),
        (ThreatEval::Downed, 2.0),
    ] {
        threat.add_evaluator(e, w);
    }
    threat.effective_attack_range = f("EffectiveThreatRange", 0.0);
    b.sensors.threat = Some(threat);
    register(b, ctx, s::S_THREAT, 1, false);
    b.sensors.pain = Some(PainSensor::new(false));
    register(b, ctx, s::S_PAIN, 1, false);
    b.sensors.hit = Some(super::hits::HitReactionSensor::new(&bp, true));
    register(b, ctx, s::S_HIT_REACTION, 0, false);
    b.sensors.weapon = Some(WeaponSensor {
        check_for_aiming: true,
        check_for_pawns_in_lof: true,
        check_for_los: false,
    });
    register(b, ctx, s::S_WEAPON, 1, false);
    b.sensors.status = Some(EnemyStatusSensor {
        ranges: vec![300.0, f("MeleeRange", 0.0), 100000.0],
        health_levels: vec![0.2, 1.0],
        ..Default::default()
    });
    register(b, ctx, s::S_ENEMY_STATUS, 0, false);
    b.sensors.channel = Some(ClearChannelSensor::default());
    register(b, ctx, s::S_CLEAR_ATTACK_CHANNEL, 3, false);
    // CreateTacticalAttackSensor: its evaluators summed.
    let randomness = f("Randomness", 0.0);
    let max_duration = ue::randomize(
        f("MaxDurationAtTactical", 20.0),
        randomness,
        &mut b.core.rng,
    );
    let max_wait = ue::randomize(f("MaxWaitWithNoLOS", 0.0), randomness, &mut b.core.rng);
    let mut tac = TacticalAttack::new(max_duration, max_wait);
    let min_tac = f("MinTacticalDistance", 0.0);
    tac.picker.add_evaluator(T::Occlusion, 0.1);
    tac.picker.add_evaluator(T::LineOfSight, 1.0);
    tac.picker.add_evaluator(T::BattleLine, 2.0);
    tac.picker.add_evaluator(
        T::Stink {
            duration: f("StinkDuration", 0.0),
        },
        1.0,
    );
    tac.picker.add_evaluator(
        T::MinEngagementDistance {
            min_distance_sq: min_tac * min_tac,
        },
        1.0,
    );
    tac.picker
        .add_evaluator(T::advance(f("MinAdvanceDistance", 0.0)), 2.0);
    b.sensors.tactical = Some(tac);
    register(b, ctx, s::S_TACTICAL_ATTACK, 3, false);
}

/// TnShotgunnerCharacterBlueprint.
fn shotgunner(b: &mut Brain, ctx: &mut Ctx) {
    let bp: Arc<Behaviour> = b.core.b.clone();
    let f = |k: &str, d: f32| bp.f(k, d);
    let shoot_range = f("ShootRange", 2500.0);
    b.sensors.search = Some(SearchSensor::default());
    register(b, ctx, s::S_SEARCH, 3, true);
    b.sensors.path = Some(path_following(&bp, None));
    register(b, ctx, s::S_PATH_FOLLOWING, 0, false);
    b.sensors.hearing = Some(HearingSensor::new(1.0, 10.0));
    register(b, ctx, s::S_HEARING, 1, false);
    // CreateVisionSensor.
    let cone = f("VisionCone", 90.0);
    let distance = f("VisionDistance", 5000.0);
    let dpd = f("DistancePerceptionDelay", 0.0);
    let ppd = f("PeripheralPerceptionDelay", 0.0);
    let mut vision = VisionSensor::default();
    vision.set_profile(0, vision_profile(cone * 0.5, distance, dpd, ppd, 0.0));
    vision.set_profile(1, vision_profile(cone, distance, dpd, ppd, 0.0));
    vision.set_profile(2, vision_profile(cone, distance, dpd, ppd, 0.0));
    b.sensors.vision = Some(vision);
    register(b, ctx, s::S_VISION, 0, false);
    // CreateThreatSensor.
    let mut threat = rpn_threat(
        0.0,
        vec![
            (ThreatEval::AbilityAgro, 1.0),
            (ThreatEval::Downed, 1.0),
            (
                ThreatEval::EnemyTypeDistribution {
                    ai_type: b.core.ai_type,
                },
                1.0,
            ),
            (ThreatEval::Sentry { discard: false }, 1.0),
            (ThreatEval::Flying { discard: false }, 1.0),
            (ThreatEval::Cloaked { discard: true }, 1.0),
            (ThreatEval::Reachable { discard: true }, 1.0),
            (ThreatEval::EnemyAttack, 1.0),
            (ThreatEval::AttackEffectiveness, 1.0),
            (ThreatEval::DamagedBy, 3.0),
            (
                ThreatEval::EnemyDistance {
                    max_range: shoot_range + 10000.0,
                },
                4.0,
            ),
            (ThreatEval::TargetDistribution, 0.1),
            (ThreatEval::Visible { discard: false }, 0.1),
        ],
        &[
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
        ],
    );
    // (get_EffectiveAttackRange and get_EffectiveAttackScore.)
    threat.effective_attack_range = shoot_range * 1.1;
    threat.effective_attack_score = 2.0;
    b.sensors.threat = Some(threat);
    register(b, ctx, s::S_THREAT, 1, false);
    b.sensors.pain = Some(PainSensor::new(false));
    register(b, ctx, s::S_PAIN, 1, false);
    b.sensors.hit = Some(super::hits::HitReactionSensor::new(&bp, true));
    register(b, ctx, s::S_HIT_REACTION, 0, false);
    b.sensors.weapon = Some(WeaponSensor {
        check_for_aiming: true,
        check_for_pawns_in_lof: true,
        check_for_los: false,
    });
    register(b, ctx, s::S_WEAPON, 1, false);
    b.sensors.status = Some(EnemyStatusSensor {
        update_navigable_location_near_enemy: true,
        ranges: vec![f("DashBackRange", 1000.0), shoot_range, 100000.0],
        flanking_half_angle: f("EnemyIsFlankedHalfAngle", 50.0),
        ..Default::default()
    });
    register(b, ctx, s::S_ENEMY_STATUS, 0, false);
    b.sensors.channel = Some(ClearChannelSensor::default());
    register(b, ctx, s::S_CLEAR_ATTACK_CHANNEL, 3, false);
    b.sensors.competency = Some(competency(&bp));
    register(b, ctx, s::S_COMPETENCY, 1, false);
    b.sensors.pathing = Some(PathingSensor::default());
    register(b, ctx, s::S_PATHING, 0, false);
    b.sensors.los_search = Some(LosSearchSensor::new(
        0.0,
        Vec3::new(75.0, 75.0, 50.0),
        shoot_range,
    ));
    register(b, ctx, s::S_LOS_SEARCH, 0, false);
}

/// TnRocketTrooperCharacterBlueprint.
fn rocket_trooper(b: &mut Brain, ctx: &mut Ctx) {
    let bp: Arc<Behaviour> = b.core.b.clone();
    let f = |k: &str, d: f32| bp.f(k, d);
    b.sensors.pathing = Some(PathingSensor::default());
    register(b, ctx, s::S_PATHING, 0, false);
    b.sensors.search = Some(SearchSensor::default());
    register(b, ctx, s::S_SEARCH, 3, true);
    b.sensors.hearing = Some(HearingSensor::new(1.0, 10.0));
    register(b, ctx, s::S_HEARING, 1, false);
    b.sensors.path = Some(path_following(&bp, None));
    register(b, ctx, s::S_PATH_FOLLOWING, 0, false);
    // CreateVisionSensor.
    let cone = f("VisionCone", 90.0);
    let distance = f("VisionDistance", 5000.0);
    let dpd = f("DistancePerceptionDelay", 0.0);
    let ppd = f("PeripheralPerceptionDelay", 0.0);
    let mut vision = VisionSensor::default();
    vision.set_profile(
        0,
        vision_profile(
            cone * 0.5,
            distance * 0.5,
            dpd,
            ppd,
            f("PatrolVisionHeight", 0.0),
        ),
    );
    vision.set_profile(1, vision_profile(cone, distance, dpd, ppd, 0.0));
    vision.set_profile(2, vision_profile(cone, distance, dpd, ppd, 0.0));
    b.sensors.vision = Some(vision);
    register(b, ctx, s::S_VISION, 0, false);
    // CreateThreatSensor.
    let mut threat = rpn_threat(
        0.0,
        vec![
            (ThreatEval::Downed, 1.0),
            (ThreatEval::AbilityAgro, 1.0),
            (ThreatEval::Cloaked { discard: false }, 1.0),
            (ThreatEval::Sentry { discard: false }, 1.0),
            (ThreatEval::DamagedBy, 3.0),
            (
                ThreatEval::EnemyTypeDistribution {
                    ai_type: b.core.ai_type,
                },
                1.0,
            ),
            (ThreatEval::VisibleFromCover, 1.0),
            (
                ThreatEval::PlayerEnemy {
                    score: f("PlayerThreatScore", 0.0),
                },
                1.0,
            ),
            (ThreatEval::EnemyAttack, 1.0),
            (ThreatEval::AttackEffectiveness, 1.0),
            (ThreatEval::TargetDistribution, 1.0),
        ],
        &[
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Add,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
        ],
    );
    threat.effective_attack_range = f("EffectiveThreatRange", 0.0);
    b.sensors.threat = Some(threat);
    register(b, ctx, s::S_THREAT, 1, false);
    b.sensors.pain = Some(PainSensor::new(false));
    register(b, ctx, s::S_PAIN, 1, false);
    b.sensors.hit = Some(super::hits::HitReactionSensor::new(&bp, true));
    register(b, ctx, s::S_HIT_REACTION, 3, false);
    b.sensors.weapon = Some(WeaponSensor {
        check_for_aiming: true,
        check_for_pawns_in_lof: true,
        check_for_los: false,
    });
    register(b, ctx, s::S_WEAPON, 1, false);
    b.sensors.status = Some(EnemyStatusSensor {
        ranges: vec![300.0, f("MeleeRange", 0.0), 100000.0],
        ..Default::default()
    });
    register(b, ctx, s::S_ENEMY_STATUS, 0, false);
    b.sensors.channel = Some(ClearChannelSensor::default());
    register(b, ctx, s::S_CLEAR_ATTACK_CHANNEL, 3, false);
    // CreateTacticalAttackSensor (Escalation's: no distance to itself).
    let randomness = f("Randomness", 0.0);
    let max_duration = ue::randomize(
        f("MaxDurationAtTactical", 20.0),
        randomness,
        &mut b.core.rng,
    );
    let max_wait = ue::randomize(f("MaxWaitToAdvance", 0.0), randomness, &mut b.core.rng);
    let mut tac = TacticalAttack::new(max_duration, max_wait);
    let min_tac = f("MinTacticalDistance", 0.0);
    tac.picker.add_rpn_evaluator(T::BattleLine, 1.0);
    tac.picker.add_rpn_evaluator(
        T::Stink {
            duration: f("StinkDuration", 0.0),
        },
        1.0,
    );
    tac.picker.add_rpn_evaluator(
        T::MinEngagementDistance {
            min_distance_sq: min_tac * min_tac,
        },
        1.0,
    );
    tac.picker
        .add_rpn_evaluator(T::advance(f("MinAdvanceDistance", 0.0)), 1.0);
    tac.picker.add_rpn_evaluator(T::Occlusion, 1.0);
    tac.picker.add_rpn_evaluator(T::LineOfSight, 1.0);
    tac.picker.add_rpn_evaluator(T::DefendLocation, 1.0);
    for op in [Rpn::Add, Rpn::Mul, Rpn::Mul, Rpn::Mul, Rpn::Mul, Rpn::Mul] {
        tac.picker.add_rpn_operator(op);
    }
    b.sensors.tactical = Some(tac);
    register(b, ctx, s::S_TACTICAL_ATTACK, 3, false);
    b.sensors.competency = Some(competency(&bp));
    register(b, ctx, s::S_COMPETENCY, 1, false);
}

/// TnScrapperCharacterBlueprint: it hears, sees all round to 100 m, goes for the
/// nearest player it can reach, and takes its turn with its squad.
fn scrapper(b: &mut Brain, ctx: &mut Ctx) {
    let bp: Arc<Behaviour> = b.core.b.clone();
    let f = |k: &str, d: f32| bp.f(k, d);
    let kung_fu = f("KungFuCircleRange", 1500.0).trunc();
    b.sensors.hearing = Some(HearingSensor::new(10.0, 10.0));
    register(b, ctx, s::S_HEARING, 1, false);
    // CreateVisionSensor: its one profile.
    let mut vision = VisionSensor::default();
    vision.set_profile(
        0,
        vision_profile(f("VisionCone", 90.0), 10000.0, 0.0, 0.0, 0.0),
    );
    b.sensors.vision = Some(vision);
    register(b, ctx, s::S_VISION, 1, false);
    // CreateThreatSensor.
    b.sensors.threat = Some(rpn_threat(
        0.0,
        vec![
            (ThreatEval::AbilityAgro, 1.0),
            (ThreatEval::Downed, 1.0),
            (ThreatEval::Sentry { discard: false }, 1.0),
            (ThreatEval::Flying { discard: false }, 1.0),
            (ThreatEval::Cloaked { discard: true }, 1.0),
            (ThreatEval::Reachable { discard: true }, 1.0),
            (ThreatEval::CurrentEnemy { score: 0.3 }, 1.0),
            (
                ThreatEval::NavigableLocationAge {
                    score: 1.0,
                    threshold: s::RECENTLY_UPDATED_THRESHOLD,
                },
                1.0,
            ),
            (ThreatEval::DamagedBy, 3.0),
            (
                ThreatEval::EnemyDistance {
                    max_range: kung_fu + 10000.0,
                },
                4.0,
            ),
            (ThreatEval::Visible { discard: false }, 0.1),
        ],
        &[
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
        ],
    ));
    register(b, ctx, s::S_THREAT, 1, false);
    b.sensors.pain = Some(PainSensor::new(false));
    register(b, ctx, s::S_PAIN, 1, false);
    b.sensors.status = Some(EnemyStatusSensor {
        update_navigable_location_near_enemy: true,
        ranges: vec![0.0, f("MoveAwayRange", 0.0).trunc(), kung_fu],
        ..Default::default()
    });
    register(b, ctx, s::S_ENEMY_STATUS, 0, false);
    squad(b, ctx);
    let r = ctx.pawn.radius * 0.7;
    b.sensors.channel = Some(ClearChannelSensor {
        trace_extent: Vec3::new(r, r, 0.0),
        walk_line_only_test: false,
        last_move_direction: 0,
        // (The demo's own: up to its enemy by a path after two failed side
        // steps, where the straight way is cut off.)
        way_round: s::WayRound::after(2),
    });
    register(b, ctx, s::S_CLEAR_ATTACK_CHANNEL, 0, false);
    b.sensors.pathing = Some(PathingSensor::default());
    register(b, ctx, s::S_PATHING, 0, false);
    b.sensors.hit = Some(super::hits::HitReactionSensor::new(&bp, true));
    register(b, ctx, s::S_HIT_REACTION, 0, false);
}

/// TnSwarmerCharacterBlueprint: the Scrapper's sensors but for its threats' last
/// (whether its enemy is a player: TnPlayerEnemyThreatEvaluator, its
/// PlayerThreatScore), its pain sensor's (friendly fire too, its default), its
/// attack channel's trace (300 uu across) and no hit reactions of its own (its
/// pain's world state picks its reactions).
fn swarmer(b: &mut Brain, ctx: &mut Ctx) {
    let bp: Arc<Behaviour> = b.core.b.clone();
    let f = |k: &str, d: f32| bp.f(k, d);
    let kung_fu = f("KungFuCircleRange", 2000.0).trunc();
    b.sensors.hearing = Some(HearingSensor::new(10.0, 10.0));
    register(b, ctx, s::S_HEARING, 1, false);
    let mut vision = VisionSensor::default();
    vision.set_profile(
        0,
        vision_profile(f("VisionCone", 270.0), 10000.0, 0.0, 0.0, 0.0),
    );
    b.sensors.vision = Some(vision);
    register(b, ctx, s::S_VISION, 1, false);
    b.sensors.threat = Some(rpn_threat(
        0.0,
        vec![
            (ThreatEval::AbilityAgro, 1.0),
            (ThreatEval::Downed, 1.0),
            (ThreatEval::Sentry { discard: false }, 1.0),
            (ThreatEval::Flying { discard: false }, 1.0),
            (ThreatEval::Cloaked { discard: true }, 1.0),
            (ThreatEval::Reachable { discard: true }, 1.0),
            (ThreatEval::CurrentEnemy { score: 0.3 }, 1.0),
            (
                ThreatEval::NavigableLocationAge {
                    score: 1.0,
                    threshold: s::RECENTLY_UPDATED_THRESHOLD,
                },
                1.0,
            ),
            (ThreatEval::DamagedBy, 3.0),
            (
                ThreatEval::EnemyDistance {
                    max_range: kung_fu + 10000.0,
                },
                4.0,
            ),
            (
                ThreatEval::PlayerEnemy {
                    score: f("PlayerThreatScore", 0.1),
                },
                1.0,
            ),
        ],
        &[
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Add,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
            Rpn::Mul,
        ],
    ));
    register(b, ctx, s::S_THREAT, 1, false);
    b.sensors.pain = Some(PainSensor::new(true));
    register(b, ctx, s::S_PAIN, 1, false);
    b.sensors.status = Some(EnemyStatusSensor {
        update_navigable_location_near_enemy: true,
        ranges: vec![0.0, f("MoveAwayRange", 0.0).trunc(), kung_fu],
        ..Default::default()
    });
    register(b, ctx, s::S_ENEMY_STATUS, 0, false);
    squad(b, ctx);
    b.sensors.channel = Some(ClearChannelSensor {
        trace_extent: Vec3::new(300.0, 300.0, 0.0),
        walk_line_only_test: false,
        last_move_direction: 0,
        way_round: s::WayRound::default(),
    });
    register(b, ctx, s::S_CLEAR_ATTACK_CHANNEL, 0, false);
    b.sensors.pathing = Some(PathingSensor::default());
    register(b, ctx, s::S_PATHING, 0, false);
}

/// TnHeavyGunnerCharacterBlueprint and TnTitanCharacterBlueprint: the soldiers'
/// senses without cover, an attack point sensor, a line of sight search 4 m out;
/// the Heavy Gunner's health levels (its damage effects), the Titan's drones.
fn heavy(class: Class, b: &mut Brain, ctx: &mut Ctx) {
    let bp: Arc<Behaviour> = b.core.b.clone();
    let f = |k: &str, d: f32| bp.f(k, d);
    let distance = f("VisionDistance", 20000.0);
    let walk = f("WalkSpeed", 700.0).trunc();
    b.sensors.update_while_asleep = false;
    b.sensors.search = Some(SearchSensor::default());
    register(b, ctx, s::S_SEARCH, 3, true);
    // (Its hearing's thresholds its class's defaults: none.)
    b.sensors.hearing = Some(HearingSensor::new(0.0, 0.0));
    register(b, ctx, s::S_HEARING, 1, false);
    // CreateVisionSensor: the same profile ignorant and alert.
    let cone = f("VisionCone", 180.0);
    let dpd = f("DistancePerceptionDelay", 0.0);
    let ppd = f("PeripheralPerceptionDelay", 0.0);
    let mut vision = VisionSensor::default();
    vision.set_profile(0, vision_profile(cone, distance, dpd, ppd, 0.0));
    vision.set_profile(1, vision_profile(cone, distance, dpd, ppd, 0.0));
    b.sensors.vision = Some(vision);
    register(b, ctx, s::S_VISION, 1, false);
    // CreateThreatSensor (the Titan's without the visibility evaluator).
    let mut evals = vec![
        (ThreatEval::AbilityAgro, 1.0),
        (ThreatEval::Downed, 1.0),
        (
            ThreatEval::EnemyTypeDistribution {
                ai_type: b.core.ai_type,
            },
            1.0,
        ),
        (ThreatEval::Sentry { discard: false }, 1.0),
        (ThreatEval::Cloaked { discard: true }, 1.0),
        (ThreatEval::EnemyAttack, 1.0),
        (ThreatEval::AttackEffectiveness, 1.0),
        (ThreatEval::DamagedBy, 3.0),
        (
            ThreatEval::EnemyDistance {
                max_range: distance,
            },
            4.0,
        ),
        (ThreatEval::TargetDistribution, 0.1),
    ];
    let mut ops = vec![Rpn::Add, Rpn::Add, Rpn::Add, Rpn::Add];
    if class == Class::HeavyGunner {
        evals.push((ThreatEval::Visible { discard: false }, 1.0));
        ops.push(Rpn::Add);
    }
    ops.extend([Rpn::Mul; 5]);
    let mut threat = rpn_threat(0.0, evals, &ops);
    if class == Class::HeavyGunner {
        // (get_EffectiveAttackRange: the Heavy Gunner's own.)
        threat.effective_attack_range = f("EffectiveThreatRange", 0.0);
    }
    b.sensors.threat = Some(threat);
    register(b, ctx, s::S_THREAT, 1, false);
    b.sensors.pain = Some(PainSensor::new(false));
    register(b, ctx, s::S_PAIN, 1, false);
    // CreateWeaponSensor: its class's checks, all of them.
    b.sensors.weapon = Some(WeaponSensor {
        check_for_aiming: true,
        check_for_pawns_in_lof: true,
        check_for_los: true,
    });
    register(b, ctx, s::S_WEAPON, 0, true);
    b.sensors.status = Some(EnemyStatusSensor {
        ranges: vec![f("MeleeRange", 0.0).trunc(), 100000.0],
        health_levels: vec![0.3, 1.0],
        ..Default::default()
    });
    register(b, ctx, s::S_ENEMY_STATUS, 2, true);
    squad(b, ctx);
    b.sensors.pathing = Some(PathingSensor::default());
    register(b, ctx, s::S_PATHING, 1, true);
    // CreatePathFollowingSensor: slower round its turns.
    let mut path = path_following(&bp, None);
    path.set_slow_turn_condition(
        f("SlowTurnSpeed", 0.0) / walk,
        (f("SlowTurnAngles.Min", 0.0), f("SlowTurnAngles.Max", 0.0)),
    );
    b.sensors.path = Some(path);
    register(b, ctx, s::S_PATH_FOLLOWING, 0, false);
    // The Titan's CreateDroneSensor: its blueprint's Drones.
    if class == Class::Titan {
        b.sensors.drone = Some(super::drones::DroneSensor::new(&bp));
        register(b, ctx, super::drones::S_DRONE, 0, true);
    }
    // CreateCompetencySensor: incompetent out past what it sees.
    b.sensors.competency = Some(CompetencySensor {
        incompetent_distance: distance,
        ..Default::default()
    });
    register(b, ctx, s::S_COMPETENCY, 0, false);
    b.sensors.los_search = Some(LosSearchSensor::new(
        400.0,
        Vec3::new(50.0, 50.0, 50.0),
        -1.0,
    ));
    register(b, ctx, s::S_LOS_SEARCH, 2, false);
    b.sensors.attack_point = Some(s::AttackPointSensor);
    register(b, ctx, s::S_ATTACK_POINT, 2, false);
    // CreateHitReactionSensor: no cover condition.
    b.sensors.hit = Some(super::hits::HitReactionSensor::new(&bp, false));
    register(b, ctx, s::S_HIT_REACTION, 0, false);
    if class == Class::HeavyGunner {
        b.sensors.health_level = Some(s::HealthLevelSensor {
            thresholds: vec![
                f("PlayHeavyDamageFxHealthThreshold", 0.0),
                f("PlayLightDamageFxHealthThreshold", 0.0),
            ],
        });
        register(b, ctx, s::S_HEALTH_LEVEL, 0, false);
    }
}

/// The squad sensor (the melee kinds' and the heavies').
fn squad(b: &mut Brain, ctx: &mut Ctx) {
    b.sensors.squad = Some(SquadSensor);
    register(b, ctx, s::S_SQUAD, 3, true);
}
