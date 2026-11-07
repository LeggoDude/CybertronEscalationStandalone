//! The Titan's drones, as TransGame's script runs them: the Titan sends them out
//! (TnDroneSensor and TnSpawnDronesOperator), and each one flies (its plane form
//! hovering: TnPlaneForm, TnHoverPlaneSimulation and HoverPhysics, under its
//! TnAiHoverPlaneMotionController) on a planning domain its character builds in
//! script (TnSupportDroneCharacter and TnExplosiveDroneCharacter's
//! OnBuildPlanningDomain: their domains aren't cooked).
//!
//! * **The Titan's waves** (its blueprint's Drones, a TnDroneSensorBlueprint): its
//!   health waves come as its health falls to each one's HealthThreshold (two
//!   support drones at 67%, two more at 27%: WS_SpecialAttackAvailable's bit 0),
//!   its unreachable wave (three explosive drones) at once the first time it can't
//!   get at its enemy and every 20 s after (SecondsBetweenUnreachableWaves: bit 1).
//!   Its SpawnDrones animation plays while each drone comes out of its next spawn
//!   socket (DroneSpawnSocket01...: SecondsUntilFirstDrone, then
//!   SecondsBetweenDrones apart), flung out round it (the n-th of m at n/m of a
//!   turn from its facing, SpawnVelocity out and as much up: falling till it comes
//!   down, then hovering). As it dies, its drones die with it
//!   (DeactivateAllDrones).
//! * **A support drone** hovers at its post: DistanceFromDefendLocation (20 m) out
//!   from the Titan and HeightAboveGround (2.56 m) over the floor
//!   (TnHoverDefendActorSensor), turned away from the other drones defending it
//!   (MoveAngleAwayFromDefenders), flying there (TnDronePathFollowingSensor) and
//!   holding it (TnHoverAtLocationOperator). Its aura (TnBuffAuraSupportDroneDamage)
//!   puts TnBuffSupportDroneDamage on the Titan, whose FloatModifier (0) is in every
//!   pawn's AllDamageModifierSelf: while any of its support drones lives, nothing
//!   hurts it. Its Titan dead, it dies too.
//! * **An explosive drone** flies at the enemy it sees (its Titan's enemy as it
//!   came out), and once within TriggerExplosionRadius (5 m) stops and plays its
//!   suicide animation for SecondBeforeExplosion (2 s), then blows up
//!   (TnExplodeOperator: HurtRadius, Damage 50 to all within DamageRadius, 10 m,
//!   in full) and dies. Out of sight of its enemy it goes where it was last seen,
//!   then back to its Titan. It lives LifespanTime (60 s) at most
//!   (TnLifespanSensor).

use std::sync::Arc;

use bevy::prelude::*;

use super::anim::AnimParams;
use super::brain::{
    BEHAVIOR_ALLOW_MOTION, Brain, Ctx, LR_ATTACK_POINT, LR_CONFIRMED_ENEMY,
    LR_NAVIGABLE_LOCATION_NEAR_DEFEND_ACTOR, LR_NONE,
};
use super::data::Behaviour;
use super::htn::{self, Domain, DomainBuilder, Status};
use super::motion::{Facing, MotionProfile};
use super::pawn::{Pawn, Physics};
use super::sensors::with_sensor;
use super::ue;
use super::world::WorldView;
use crate::enemies::Death;
use crate::nav::{Corner, CornerType, Moves, Portal};

// Htn.EHtnComparisonOperator.
const EQUAL: i32 = 0;
const NOT_EQUAL: i32 = 1;
const BIT_IS_NOT_SET: i32 = 7;
const BITS_ARE_SET: i32 = 8;
/// WS_Asleep's bits (Htn.ConvertToBit(0) | ConvertToBit(1)).
const ASLEEP: i32 = 0b11;

/// The sensors' identifiers (their classes' _Identifier): the Titan's drone
/// sensor, the drones' path following, the support drone's post and the
/// explosive drone's word of its Titan (both 17), its lifespan.
pub const S_DRONE: u8 = 26;
pub const S_DRONE_PATH_FOLLOWING: u8 = 38;
pub const S_DEFEND_ACTOR: u8 = 17;
pub const S_LIFESPAN: u8 = 41;

/// WS_SpecialAttackAvailable's bits (TnDroneSensor.EDroneFlags).
pub const DF_HEALTH_WAVE: i32 = 0;
pub const DF_UNREACHABLE_WAVE: i32 = 1;

// ---------------------------------------------------------------------------
// Their planning domains
// ---------------------------------------------------------------------------

/// TnSupportDroneCharacter.ESupportDroneTaskSymbol.
const SUPPORT_TASKS: [&str; 6] = [
    "SDTS_SupportDrone",
    "SDTS_Asleep",
    "SDTS_ChooseNewSupportPoint",
    "SDTS_MoveTo",
    "SDTS_IdleWhileMoving",
    "SDTS_IdleWhileStationary",
];

/// TnExplosiveDroneCharacter.EExplosiveDroneTaskSymbol.
const EXPLOSIVE_TASKS: [&str; 12] = [
    "EDTS_ExplosiveDrone",
    "EDTS_Asleep",
    "EDTS_MoveToVisibleEnemy",
    "EDTS_MoveToSuspectedEnemy",
    "EDTS_MoveToDefendActor",
    "EDTS_MoveTo",
    "EDTS_IdleWhileMoving",
    "EDTS_PlayAnimation",
    "EDTS_Explode",
    "EDTS_IdleWhileStationary",
    "EDTS_ChooseDefendLocation",
    "EDTS_ShouldReturnToDefendActor",
];

/// The domain a character builds in script, for the classes that build their own.
pub fn script_domain(b: &Behaviour) -> Option<Domain> {
    match b.class.as_str() {
        "TnSupportDroneCharacter" => Some(support_drone_domain()),
        "TnExplosiveDroneCharacter" => Some(explosive_drone_domain(b)),
        _ => None,
    }
}

/// The asleep task (an idle while WS_Asleep has both its bits).
fn asleep(d: &mut DomainBuilder, symbol: usize) {
    d.begin_task(symbol);
    d.begin_operator("TnIdleOperator");
    d.pre(htn::WS_ASLEEP, ASLEEP, BITS_ARE_SET);
    d.end_operator();
    d.end_task();
}

/// The idle while it moves (its motion allowed while WS_Navigation is on).
fn idle_while_moving(d: &mut DomainBuilder, symbol: usize) {
    d.begin_task(symbol);
    d.begin_operator("TnIdleOperator");
    d.pre(htn::WS_NAVIGATION, 0, NOT_EQUAL);
    d.behavior(BEHAVIOR_ALLOW_MOTION);
    d.end_operator();
    d.end_task();
}

/// TnSupportDroneCharacter.OnBuildPlanningDomain.
fn support_drone_domain() -> Domain {
    let mut d = DomainBuilder::new("TnSupportDroneCharacter", &SUPPORT_TASKS);
    d.debug_flags(1);
    asleep(&mut d, 1);
    d.begin_task(3);
    d.declare("Destination", 0);
    d.declare("Range", 10);
    d.declare("Speed", 0);
    d.begin_operator("TnDroneMoveToOperator");
    d.end_operator();
    d.end_task();
    idle_while_moving(&mut d, 4);
    d.begin_task(5);
    d.declare("LocationReference", 0);
    d.declare("Range", 50);
    d.begin_operator("TnHoverAtLocationOperator");
    d.eff(htn::WS_LOCATION, 0);
    d.end_operator();
    d.end_task();
    // The drone: asleep; at its post, holding it; on its way, idling; else off to
    // it.
    d.begin_task(0);
    d.begin_method();
    d.pre(htn::WS_ASLEEP, ASLEEP, BITS_ARE_SET);
    d.sub(1);
    d.end_method();
    d.begin_method();
    d.pre(htn::WS_LOCATION, LR_ATTACK_POINT as i32, EQUAL);
    d.begin_sub(5);
    d.set_param("LocationReference", LR_ATTACK_POINT as i32);
    d.end_sub();
    d.end_method();
    d.begin_method();
    d.pre(htn::WS_NAVIGATION, LR_ATTACK_POINT as i32, EQUAL);
    d.sub(4);
    d.end_method();
    d.begin_method();
    d.pre(htn::WS_PATH_BLOCKED, LR_ATTACK_POINT as i32, BIT_IS_NOT_SET);
    d.begin_sub(3);
    d.set_param("Destination", LR_ATTACK_POINT as i32);
    d.end_sub();
    d.end_method();
    d.end_task();
    d.build()
}

/// TnExplosiveDroneCharacter.OnBuildPlanningDomain: its blueprint's trigger radius,
/// time before it blows and blast in its tasks' parameters.
fn explosive_drone_domain(b: &Behaviour) -> Domain {
    let mut d = DomainBuilder::new("TnExplosiveDroneCharacter", &EXPLOSIVE_TASKS);
    d.debug_flags(1);
    asleep(&mut d, 1);
    // MoveTo: done within the trigger radius, its enemy expected in range then
    // (EER_Explosive), and ready to return to its Titan.
    d.begin_task(5);
    d.declare("Destination", 0);
    d.declare("Range", b.i("TriggerExplosionRadius", 500));
    d.declare("Speed", 0);
    d.begin_operator("TnDroneMoveToOperator");
    d.expected_eff(htn::WS_ENEMY_RANGE, 0);
    d.eff(htn::WS_TIRED, 0);
    d.end_operator();
    d.end_task();
    idle_while_moving(&mut d, 6);
    // IdleWhileStationary: a second, then off its place, and back to its Titan
    // (WS_Tired, its ShouldReturnToDefendActor).
    d.begin_task(9);
    d.declare("Duration", htn::pack_float(1.0));
    d.begin_operator("TnIdleForDurationOperator");
    d.eff(htn::WS_LOCATION, 0);
    d.eff(htn::WS_TIRED, 1);
    d.end_operator();
    d.end_task();
    // PlayAnimation: its explosion animation for SecondBeforeExplosion (an int of
    // seconds: cast44).
    d.begin_task(7);
    d.declare("AnimationReference", 0);
    d.declare("Duration", b.f("SecondBeforeExplosion", 2.0) as i32);
    d.add_operator("TnReactForDurationOperator");
    d.end_task();
    d.begin_task(8);
    d.declare("Damage", b.i("Damage", 50));
    d.declare("DamageRadius", b.i("DamageRadius", 1000));
    d.add_operator("TnExplodeOperator");
    d.end_task();
    d.begin_task(11);
    d.declare("Enable", 0);
    d.begin_operator("HtnNoOpOperator");
    d.variable_eff(htn::WS_TIRED, "Enable");
    d.end_operator();
    d.end_task();
    d.begin_task(10);
    d.add_operator("TnChooseNewDefendActorLocationOperator");
    d.end_task();
    // MoveToVisibleEnemy: there and in range, it blows; on its way, idling; else
    // off to it.
    d.begin_task(2);
    d.begin_method();
    d.pre(htn::WS_LOCATION, LR_CONFIRMED_ENEMY as i32, EQUAL);
    d.pre(htn::WS_ENEMY_RANGE, 0, EQUAL);
    d.sub(7);
    d.sub(8);
    d.end_method();
    d.begin_method();
    d.pre(htn::WS_NAVIGATION, LR_CONFIRMED_ENEMY as i32, EQUAL);
    d.sub(6);
    d.end_method();
    d.begin_method();
    d.pre(
        htn::WS_PATH_BLOCKED,
        LR_CONFIRMED_ENEMY as i32,
        BIT_IS_NOT_SET,
    );
    d.begin_sub(5);
    d.set_param("Destination", LR_CONFIRMED_ENEMY as i32);
    d.end_sub();
    d.end_method();
    d.end_task();
    // MoveToSuspectedEnemy: there, a moment's idle; on its way, idling; else off
    // to it.
    let suspected = super::brain::LR_SUSPECTED_ENEMY as i32;
    d.begin_task(3);
    d.begin_method();
    d.pre(htn::WS_LOCATION, suspected, EQUAL);
    d.sub(9);
    d.end_method();
    d.begin_method();
    d.pre(htn::WS_NAVIGATION, suspected, EQUAL);
    d.sub(6);
    d.end_method();
    d.begin_method();
    d.pre(htn::WS_PATH_BLOCKED, suspected, BIT_IS_NOT_SET);
    d.begin_sub(5);
    d.set_param("Destination", suspected);
    d.end_sub();
    d.end_method();
    d.end_task();
    // MoveToDefendActor: the same with its Titan's place (LR_AttackPoint), chosen
    // afresh before it sets off.
    let defend = LR_ATTACK_POINT as i32;
    d.begin_task(4);
    d.begin_method();
    d.pre(htn::WS_LOCATION, defend, EQUAL);
    d.sub(9);
    d.end_method();
    d.begin_method();
    d.pre(htn::WS_NAVIGATION, defend, EQUAL);
    d.sub(6);
    d.end_method();
    d.begin_method();
    d.pre(htn::WS_PATH_BLOCKED, defend, BIT_IS_NOT_SET);
    d.sub(10);
    d.begin_sub(5);
    d.set_param("Destination", defend);
    d.end_sub();
    d.end_method();
    d.end_task();
    // The drone: asleep; its enemy in sight; its enemy's last place, unless it
    // has been and is going back; its Titan.
    d.begin_task(0);
    d.begin_method();
    d.pre(htn::WS_ASLEEP, ASLEEP, BITS_ARE_SET);
    d.sub(1);
    d.end_method();
    d.begin_method();
    d.pre(htn::WS_ENEMY_VISIBLE, 1, EQUAL);
    d.sub(2);
    d.end_method();
    d.begin_method();
    d.pre(htn::WS_TIRED, 0, EQUAL);
    d.sub(3);
    d.end_method();
    d.begin_method();
    d.sub(4);
    d.end_method();
    d.end_task();
    d.build()
}

// ---------------------------------------------------------------------------
// The Titan's drone sensor
// ---------------------------------------------------------------------------

/// TnDroneSensorBlueprint.TnDroneWave: its drones (their enemy types), the health
/// it comes at, its timing, the sockets they come out of and how hard they're
/// flung out.
#[derive(Clone, Debug, Default)]
pub struct DroneWave {
    pub drones: Vec<String>,
    pub health_threshold: f32,
    pub seconds_until_first_drone: f32,
    pub seconds_between_drones: f32,
    pub spawn_animation: String,
    pub spawn_sockets: Vec<Arc<str>>,
    pub spawn_velocity: f32,
}

impl DroneWave {
    fn read(b: &Behaviour, key: &str) -> DroneWave {
        let k = |f: &str| format!("{key}.{f}");
        let drones = (0..b.n(&k("Drones")))
            .filter_map(|i| b.obj(&format!("{key}.Drones[{i}]")))
            .map(drone_type)
            .collect();
        DroneWave {
            drones,
            health_threshold: b.f(&k("HealthThreshold"), 0.0),
            seconds_until_first_drone: b.f(&k("SecondsUntilFirstDrone"), 0.0),
            seconds_between_drones: b.f(&k("SecondsBetweenDrones"), 0.0),
            spawn_animation: b.get(&k("SpawnAnimation")).unwrap_or("None").to_string(),
            spawn_sockets: b
                .get(&k("SpawnSockets"))
                .map(|s| s.split_whitespace().map(Arc::from).collect())
                .unwrap_or_default(),
            spawn_velocity: b.f(&k("SpawnVelocity"), 0.0),
        }
    }
}

/// An AiDescription's enemy type (TR_Titan_AI_p.SupportDrone_CHARACTER: SupportDrone).
fn drone_type(name: &str) -> String {
    name.strip_suffix("_CHARACTER").unwrap_or(name).to_string()
}

/// TnDroneSensor.
#[derive(Clone, Debug, Default)]
pub struct DroneSensor {
    pub waves: Vec<DroneWave>,
    pub unreachable_wave: DroneWave,
    pub seconds_between_unreachable_waves: f32,
    /// The health wave due (-1 before the first), and whether one is coming out.
    pub desired_wave: i32,
    pub is_spawning: bool,
    /// The drones it sent out that live.
    pub drones: Vec<Entity>,
    pub seconds_until_unreachable_wave: f32,
    /// It had its support drones' shield, and the dialog it says as it goes
    /// (ShieldDeactivedDialog).
    pub has_shield: bool,
    pub shield_deactivated_dialog: Option<String>,
}

impl DroneSensor {
    /// CreateDroneSensor: its blueprint's Drones.
    pub fn new(b: &Behaviour) -> DroneSensor {
        DroneSensor {
            waves: (0..b.n("Drones.Waves"))
                .map(|i| DroneWave::read(b, &format!("Drones.Waves[{i}]")))
                .collect(),
            unreachable_wave: DroneWave::read(b, "Drones.UnreachableEnemyWave"),
            seconds_between_unreachable_waves: b.f("Drones.SecondsBetweenUnreachableWaves", 20.0),
            desired_wave: -1,
            shield_deactivated_dialog: b.get("ShieldDeactivedDialog").map(str::to_string),
            ..Default::default()
        }
    }

    /// OnInitialize: its unreachable wave ready from the start.
    pub fn initialize(&mut self, b: &mut Brain) {
        b.core
            .set_ws_bit(htn::WS_SPECIAL_ATTACK_AVAILABLE, DF_UNREACHABLE_WAVE, true);
    }

    /// get_DesiredWave (none before the first: an empty wave).
    pub fn desired(&self) -> DroneWave {
        usize::try_from(self.desired_wave)
            .ok()
            .and_then(|i| self.waves.get(i))
            .cloned()
            .unwrap_or_default()
    }

    /// EndSpawning.
    pub fn end_spawning(&mut self, b: &mut Brain, p: &Pawn) {
        self.is_spawning = false;
        self.update_desired_wave(b, p);
    }

    /// OnUpdate.
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx, dt: f32) {
        self.remove_dead_drones(ctx.view);
        self.update_desired_wave(b, ctx.pawn);
        self.update_unreachable_wave(b, dt);
        self.update_shield_dialog(b, ctx);
    }

    /// UpdateDesiredWave: the next wave due as its health falls to its threshold.
    fn update_desired_wave(&mut self, b: &mut Brain, p: &Pawn) {
        if self.is_spawning {
            return;
        }
        let next = (self.desired_wave + 1) as usize;
        let Some(wave) = self.waves.get(next) else {
            return;
        };
        if p.health / p.health_max.max(1.0) > wave.health_threshold {
            return;
        }
        b.core
            .set_ws_bit(htn::WS_SPECIAL_ATTACK_AVAILABLE, DF_HEALTH_WAVE, true);
        self.desired_wave += 1;
    }

    /// UpdateUnreachableWave: once used, ready again after its time.
    fn update_unreachable_wave(&mut self, b: &mut Brain, dt: f32) {
        if b.core
            .ws_bit(htn::WS_SPECIAL_ATTACK_AVAILABLE, DF_UNREACHABLE_WAVE)
        {
            return;
        }
        if self.seconds_until_unreachable_wave <= 0.0 {
            self.seconds_until_unreachable_wave = self.seconds_between_unreachable_waves;
        }
        self.seconds_until_unreachable_wave -= dt;
        if self.seconds_until_unreachable_wave > 0.0 {
            return;
        }
        b.core
            .set_ws_bit(htn::WS_SPECIAL_ATTACK_AVAILABLE, DF_UNREACHABLE_WAVE, true);
    }

    /// UpdateShieldDialog: a word as its support drones' shield goes.
    fn update_shield_dialog(&mut self, b: &Brain, ctx: &mut Ctx) {
        let has = shielded(ctx.shared, b.core.entity);
        if !has
            && self.has_shield
            && let Some(event) = &self.shield_deactivated_dialog
        {
            ctx.shared
                .dialog
                .push(super::audio::DialogRequest::say(b.core.entity, event));
        }
        self.has_shield = has;
    }

    /// RemoveDeadDrones.
    fn remove_dead_drones(&mut self, view: &WorldView) {
        self.drones.retain(|d| view.ai(*d).is_some_and(|a| a.alive));
    }

    /// DroneSpawned (its defend actor and threat are set as it comes in:
    /// DroneLaunch).
    pub fn drone_spawned(&mut self, drone: Entity) {
        self.drones.push(drone);
    }

    /// OnShutdown: its drones die with it (Kill, TnDamageTypeAssassination).
    pub fn shutdown(&mut self, ctx: &mut Ctx) {
        self.remove_dead_drones(ctx.view);
        for d in self.drones.drain(..) {
            ctx.shared.kills.push((d, Death::Plain));
        }
    }
}

/// Whether a pawn has its support drones' shield on it (TnBuffSupportDroneDamage).
pub fn shielded(shared: &super::shared::Shared, e: Entity) -> bool {
    shared.shields.iter().any(|(on, _)| *on == e)
}

// ---------------------------------------------------------------------------
// The Titan sending them out
// ---------------------------------------------------------------------------

/// A drone a Titan asked for (TnLatentAISpawner.BeginSpawn): its type, where it
/// comes in (its middle, Unreal space) and facing, the velocity it is flung out
/// with (OnSpawnComplete's), whose it is and whom to go after.
#[derive(Clone, Debug)]
pub struct DroneSpawn {
    pub owner: Entity,
    pub kind: String,
    pub at: Vec3,
    pub yaw: f32,
    pub velocity: Vec3,
    pub enemy: Option<Entity>,
}

/// On a drone come in, till its AI takes it up: what its Titan gave it.
#[derive(Component, Clone, Debug)]
pub struct DroneLaunch {
    pub owner: Entity,
    pub velocity: Vec3,
    pub enemy: Option<Entity>,
}

/// TnSpawnDronesOperator's state.
#[derive(Clone, Debug, Default)]
pub struct SpawnDrones {
    pub wave: DroneWave,
    pub seconds_until_next_drone: f32,
    pub index: usize,
    pub animation: Arc<str>,
    /// The spawner's at work on a drone (_Spawner._Active).
    pub spawning: bool,
}

impl SpawnDrones {
    /// OnStart: the wave (its unreachable one if the task says), its animation
    /// (its blueprint's, high priority, its root motion ignored), its first drone's
    /// time.
    pub fn start(&mut self, b: &mut Brain, ctx: &mut Ctx, task: &htn::Task) -> bool {
        let unreachable = task.int(1) == 1;
        let wave = with_sensor!(b, drone, s => {
            s.is_spawning = true;
            if unreachable { s.unreachable_wave.clone() } else { s.desired() }
        });
        let Some(wave) = wave else {
            return false;
        };
        self.wave = wave;
        self.index = 0;
        self.spawning = false;
        b.set_behavior(ctx, 0, false);
        let mut p: AnimParams = b.blueprint.anim(task.int(0), super::pawn::stance::NONE);
        p.priority = 1;
        p.override_mode = super::anim::OVERRIDE_IF_HIGHER_OR_EQUAL;
        p.root_motion = super::anim::RMM_IGNORE;
        self.animation = p.name.clone();
        ctx.pawn.anim.enqueue(p, None, None);
        self.seconds_until_next_drone = self.wave.seconds_until_first_drone;
        self.update_spawning(b, ctx, 0.0);
        true
    }

    /// OnUpdate: on while drones are coming and its animation plays.
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) -> Status {
        self.update_spawning(b, ctx, ctx.dt);
        if self.seconds_until_next_drone > 0.0 || self.spawning {
            return Status::Running;
        }
        if ctx.pawn.anim.is_playing_named(&self.animation) {
            return Status::Running;
        }
        Status::Succeeded
    }

    /// OnFinish.
    pub fn finish(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let p = &*ctx.pawn;
        with_sensor!(b, drone, s => s.end_spawning(b, p));
    }

    /// UpdateSpawning: the next drone once its time comes (the spawner taking one
    /// at a time).
    fn update_spawning(&mut self, b: &mut Brain, ctx: &mut Ctx, dt: f32) {
        self.seconds_until_next_drone -= dt;
        if self.index < self.wave.drones.len()
            && self.seconds_until_next_drone <= 0.0
            && !self.spawning
        {
            self.spawn_next_drone(b, ctx);
        }
    }

    /// SpawnNextDrone: out of its next socket (or over its cylinder), turned as
    /// the Titan is, flung out at its share of a turn round it.
    fn spawn_next_drone(&mut self, b: &Brain, ctx: &mut Ctx) {
        let p = &*ctx.pawn;
        let sockets = &self.wave.spawn_sockets;
        let at = if sockets.is_empty() {
            None
        } else {
            p.socket_frame(&sockets[self.index % sockets.len()])
                .map(|(at, _)| at)
        }
        .unwrap_or(p.loc + Vec3::Z * p.half_height);
        let n = self.wave.drones.len().max(1) as f32;
        let angle = self.index as f32 / n * std::f32::consts::TAU;
        let v = self.wave.spawn_velocity;
        let out = Vec3::new(v * angle.cos(), v * angle.sin(), v);
        ctx.shared.drone_spawns.push(DroneSpawn {
            owner: b.core.entity,
            kind: self.wave.drones[self.index].clone(),
            at,
            yaw: p.yaw,
            velocity: ue::rotate_yaw(out, p.yaw),
            enemy: b.core.enemy,
        });
        self.spawning = true;
    }

    /// OnSpawnComplete: one more out (or none could come), the next after its
    /// time.
    pub fn spawn_complete(&mut self, b: &mut Brain, drone: Option<Entity>) {
        self.spawning = false;
        if let Some(d) = drone
            && let Some(s) = b.sensors.drone.as_mut()
        {
            s.drone_spawned(d);
        }
        self.index += 1;
        self.seconds_until_next_drone = self.wave.seconds_between_drones;
    }
}

/// The drones the Titans asked for, brought in (TnLatentAISpawner, its collision
/// unchecked: enemies.rs's spawn, their feet under their middles), each with what
/// its Titan gave it for its AI to take up (its AI's attach tells the Titan it
/// came; one that couldn't come is told of at once).
pub fn spawn_drones(
    mut commands: Commands,
    mut world: ResMut<super::AiWorld>,
    kinds: Res<crate::enemies::Kinds>,
    asset_server: Res<AssetServer>,
    mut waves: ResMut<crate::waves::Waves>,
    numbered: Query<&crate::waves::WaveFoe>,
) {
    if world.shared.drone_spawns.is_empty() {
        return;
    }
    for s in std::mem::take(&mut world.shared.drone_spawns) {
        let kind = kinds.kinds.iter().position(|k| k.id == s.kind);
        let seed = world.shared.rng.next_u32();
        // (Its model's middle where it comes in: its vehicle form's mesh is
        // centred on it, mod.rs's attach takes its feet from it.)
        let (at, yaw) = (ue::from_ue(s.at), ue::heading(s.yaw));
        let spawned = kind.and_then(|k| {
            crate::enemies::spawn(
                &mut commands,
                &asset_server,
                &kinds,
                k,
                (at, yaw),
                seed,
                // (The players' buffs on what they do to it: waves.rs's done.)
                crate::waves::done(),
            )
            .map(|e| (e, k))
        });
        match spawned {
            Some((e, k)) => {
                // (Numbered with the waves' enemies, for the other PCs to bring it
                // in as it came in here: sync.rs.)
                let snap = crate::net::EnemySnap {
                    id: 0,
                    kind: k as u8,
                    at,
                    yaw,
                    seed,
                    to: at,
                    leap: false,
                    owner: numbered.get(s.owner).map_or(0, |w| w.id),
                };
                let foe = waves.adopt(e, snap);
                commands.entity(e).insert((
                    DroneLaunch {
                        owner: s.owner,
                        velocity: s.velocity,
                        enemy: s.enemy,
                    },
                    foe,
                ));
            }
            None => world.shared.drones_spawned.push((s.owner, None)),
        }
    }
}

/// A drone come in (or none that could), to the Titan that asked for it: its
/// spawn operator's, or, that gone, its drone sensor's all the same.
pub fn deliver_spawned(b: &mut Brain, drone: Option<Entity>) {
    let mut op = b.runner.active.take();
    match op.as_mut().map(|o| &mut o.kind) {
        Some(super::ops::Kind::SpawnDrones(s)) => s.spawn_complete(b, drone),
        _ => {
            if let (Some(d), Some(s)) = (drone, b.sensors.drone.as_mut()) {
                s.drone_spawned(d);
            }
        }
    }
    b.runner.active = op;
}

/// A drone's body as its rigid body turns it (yaw, then pitch, then roll), for its
/// model (the demo's axes: its facing along its heading's -z, its right +x).
pub fn body_rotation(f: &PlaneForm) -> Quat {
    let k = std::f32::consts::TAU / 65536.0;
    Quat::from_rotation_y(ue::heading(f.rotation.y))
        * Quat::from_rotation_x(f.rotation.x * k)
        * Quat::from_rotation_z(-f.rotation.z * k)
}

/// A drone's AI as it comes in: flung out (SetPhysics(PHYS_Falling) and its
/// velocity), its Titan its defend actor and its Titan's enemy its threat
/// (DroneSpawned).
pub fn launch(brain: &mut Brain, pawn: &mut Pawn, view: &WorldView, l: &DroneLaunch) {
    pawn.physics = Physics::Falling;
    pawn.vel = l.velocity;
    pawn.accel = Vec3::ZERO;
    brain.sensors.defend_actor = Some(l.owner);
    if let Some(e) = l.enemy {
        brain.sensors.threats.add(view, e, false);
    }
}

// ---------------------------------------------------------------------------
// The plane form, hovering, and its motion controller
// ---------------------------------------------------------------------------

/// A plane form's hover's AI values (its TnHoverPlaneSimulationBlueprint's): its
/// top turn speed (radians a second) and acceleration, the lean its moves give it
/// (AiMotionPhysicsScale), and how near where it goes it brakes hard (its plane
/// form's UseStoppingLinearDampingDistance, squared).
#[derive(Clone, Copy, Debug)]
pub struct HoverValues {
    pub max_angular_speed: f32,
    pub max_angular_acceleration: f32,
    pub motion_physics_scale: f32,
    pub stopping_distance_sq: f32,
}

/// The drones' hover (TR_Sentinel_VEHDEF_p.HoverPlane_Physics).
pub const SENTINEL_HOVER: HoverValues = HoverValues {
    max_angular_speed: 12.0,
    max_angular_acceleration: 24.0,
    motion_physics_scale: 1.0,
    stopping_distance_sq: 10_000.0,
};

/// A plane form's flight as its AI flies it (TnPlaneForm's Flying state:
/// TnPlaneSimulation.UpdateFromLocationAndRotation, its TnPlaneSimulationBlueprint's
/// values): its top turn speed and acceleration (MaxAiTurnSpeed and
/// MaxAiTurnAcceleration, as the hover's), the extra turn and roll a turn gives it
/// (YawDueToYawValue and RollDueToYawValue, degrees, eased in by
/// ExtraRotationLerpValue a frame), how far ahead it steers (TnPlaneForm's
/// _AIForwardSteeringTarget) and how long a collision keeps it from flying again
/// (its plane form blueprint's CollisionReactionTime).
#[derive(Clone, Copy, Debug)]
pub struct FlightValues {
    pub max_turn_speed: f32,
    pub max_turn_acceleration: f32,
    pub yaw_due_to_yaw: f32,
    pub roll_due_to_yaw: f32,
    pub extra_rotation_lerp: f32,
    pub forward_steering: f32,
    pub collision_reaction_time: f32,
}

/// TnHoverPlaneSimulation's rigid body damping (kDefaultLinearDamping, within
/// _UseStoppingLinearDampingDistance of where it goes kStopLinearDamping;
/// kDefaultAngularDamping) and its top spin (kMaxAngularVelocity).
const DEFAULT_LINEAR_DAMPING: f32 = 0.1;
const STOP_LINEAR_DAMPING: f32 = 10.0;
const ANGULAR_DAMPING: f32 = 6.0;
const MAX_ANGULAR_VELOCITY: f32 = 50.0;
/// TnPlaneSimulation's (kDefaultLinearDamping and kDefaultAngularDamping, times
/// their modifiers: 1).
const FLIGHT_LINEAR_DAMPING: f32 = 0.1;
const FLIGHT_ANGULAR_DAMPING: f32 = 8.0;
/// TnAiHoverPlaneMotionController.kNoBrakesThreshold: it brakes as it stops only
/// going faster than this.
const NO_BRAKES_THRESHOLD: f32 = 100.0;
/// AddMotionPhysics's smoothing of the body's local acceleration and spin.
const SMOOTH_ACCEL: f32 = 0.1;
const SMOOTH_SPIN: f32 = 0.9;

/// TnAiHoverPlaneMotionController and the plane form it drives (TnPlaneForm's
/// desired location and rotation, its Hovering or Flying state, its
/// simulations' AI values and state).
#[derive(Clone, Debug)]
pub struct PlaneForm {
    pub profiles: [MotionProfile; 3],
    /// The profiles' turn modifiers (MaxAngularSpeed, a packed float: the
    /// flight's MaxAiFlyingTurningModifier) and whether each boosts (EnableBoost).
    pub turn_modifiers: [f32; 3],
    pub boost: [bool; 3],
    pub speed: u8,
    pub destination: Vec3,
    pub normal: Vec3,
    pub range: f32,
    pub max_z_offset: f32,
    pub slow_down_factor: f32,
    pub moving: bool,
    /// Its obstacle avoidance (TnObstacleAvoidanceManager: on, and its
    /// MinSeparation).
    pub avoidance: bool,
    pub avoid_min_separation: f32,
    /// The plane form's _DesiredLocation and _DesiredRotation (pitch, yaw, roll).
    pub desired_location: Vec3,
    pub desired_rotation: Vec3,
    /// SetAIFlyingValues: its top speed, acceleration and turn modifier.
    pub max_speed: f32,
    pub max_accel: f32,
    pub turn_modifier: f32,
    pub hover: HoverValues,
    pub flight: Option<FlightValues>,
    /// Its boost input (_InputState._Boosting: its profile's), its Flying state
    /// (_Boosting), the hover's drift as it comes back into it (Drift: it can't
    /// boost again till its next frame) and the time a collision keeps it from
    /// flying (_AiCollisionCountdown).
    pub boost_input: bool,
    pub boosting: bool,
    pub drifting: bool,
    pub collision_countdown: f32,
    /// TnPlaneSimulation's _PreviousRotationDelta (pitch, yaw, roll).
    pub previous_rotation_delta: Vec3,
    /// AddMotionPhysics's smoothed local acceleration and spin.
    pub smooth_accel: Vec3,
    pub smooth_spin: Vec3,
    /// Its rigid body's turn (pitch, yaw, roll: rotation units), its spin about
    /// those (radians a second) and its last local acceleration.
    pub rotation: Vec3,
    pub spin: Vec3,
    pub accel: Vec3,
    /// Its rigid body as the cylinder it meets the level with (its half width and
    /// half height, uu), and its mesh's middle in its own axes (forward, right,
    /// up: the vehicle form puts it on the pawn's Location, CalculateCylinderBounds).
    pub body: (f32, f32),
    pub mesh_centre: Vec3,
    /// A jet's plane's effects and engine (jet.rs's VehicleLooks; none for the
    /// drones: their flight's own EffectTemplate and engine), and its engine's
    /// state last frame (none: its form wasn't out; else whether it was flying).
    pub looks: Option<std::sync::Arc<super::jet::VehicleLooks>>,
    pub sounded: Option<bool>,
}

impl PlaneForm {
    /// A TnAiHoverPlaneMotionController (its MaxZOffset; no slowing, no avoidance:
    /// the drones') with one profile, its plane form hovering where it is.
    pub fn new(profile: MotionProfile, max_z_offset: f32, at: Vec3, yaw: f32) -> PlaneForm {
        let mut f = PlaneForm {
            profiles: [profile; 3],
            turn_modifiers: [1.0; 3],
            boost: [false; 3],
            speed: 0,
            destination: at,
            normal: Vec3::ZERO,
            range: 0.0,
            max_z_offset,
            slow_down_factor: 0.0,
            moving: false,
            avoidance: false,
            avoid_min_separation: 0.0,
            desired_location: at,
            desired_rotation: Vec3::new(0.0, yaw, 0.0),
            max_speed: 0.0,
            max_accel: 0.0,
            turn_modifier: 1.0,
            hover: SENTINEL_HOVER,
            flight: None,
            boost_input: false,
            boosting: false,
            drifting: false,
            collision_countdown: 0.0,
            previous_rotation_delta: Vec3::ZERO,
            smooth_accel: Vec3::ZERO,
            smooth_spin: Vec3::ZERO,
            rotation: Vec3::new(0.0, yaw, 0.0),
            spin: Vec3::ZERO,
            accel: Vec3::ZERO,
            body: (BODY_RADIUS, BODY_HALF_HEIGHT),
            mesh_centre: Vec3::ZERO,
            looks: None,
            sounded: None,
        };
        f.apply_motion_properties();
        f
    }

    /// ApplyMotionProperties (SetAIFlyingValues, and its boost input).
    fn apply_motion_properties(&mut self) {
        let k = (self.speed as usize).min(2);
        let p = self.profiles[k];
        self.max_speed = p.max_linear_speed as f32;
        self.max_accel = p.max_linear_acceleration as f32;
        self.turn_modifier = self.turn_modifiers[k];
        self.boost_input = self.boost[k];
    }

    /// set_Speed.
    pub fn set_speed(&mut self, speed: u8) {
        self.speed = speed;
        self.apply_motion_properties();
    }

    /// MoveToPoint.
    pub fn move_to_point(&mut self, loc: Vec3, point: Vec3, range: f32) {
        self.destination = point;
        self.moving = true;
        self.range = range;
        self.normal = ue::normal2d(point - loc);
    }

    /// Stop (SetStopLocation): it comes to rest where it is (braking, past
    /// kNoBrakesThreshold, by its velocity times its SlowDownFactor).
    pub fn stop(&mut self, loc: Vec3, vel: Vec3) {
        self.moving = false;
        let brake = vel.length() > NO_BRAKES_THRESHOLD;
        self.desired_location = loc
            + if brake {
                vel * self.slow_down_factor
            } else {
                Vec3::ZERO
            };
    }

    /// get_ReachedDestination: at or past it along the way it set off, and near
    /// its height.
    pub fn reached(&self, loc: Vec3) -> bool {
        ue::dot2d(loc - self.destination, self.normal) > -self.range
            && (self.destination.z - loc.z).abs() < self.max_z_offset
    }

    /// Its form taken (TnPlaneForm.OnActivate and its vehicle form's): where it
    /// is and its way as it starts (its rigid body put on the pawn's Rotation:
    /// SetRBRotation), hovering unless its boost input has it fly.
    pub fn activate(&mut self, loc: Vec3, rotation: Vec3) {
        self.desired_location = loc;
        self.desired_rotation = rotation;
        self.rotation = rotation;
        self.spin = Vec3::ZERO;
        self.smooth_accel = Vec3::ZERO;
        self.smooth_spin = Vec3::ZERO;
        self.previous_rotation_delta = Vec3::ZERO;
        self.moving = false;
        self.boosting = self.flight.is_some() && self.boost_input;
        self.drifting = !self.boosting;
    }
}

/// TnAiHoverPlaneMotionController.Update: its rotation to match (by its facing),
/// and while it isn't there its destination for the plane form (SetMovementLocation:
/// a way round the pawn it would run into, if it has avoidance, facing that way
/// while it flies).
pub fn motion_update(b: &Brain, p: &mut Pawn, view: &WorldView) {
    let loc = p.loc;
    let focus = p.focus;
    let enemy = b.core.fetch(LR_CONFIRMED_ENEMY);
    let facing = p.motion.facing;
    let toward = |at: Vec3| {
        let d = at - loc;
        Vec3::new(ue::pitch_of(d), ue::yaw_of(d), 0.0)
    };
    // (Its avoidance: TnObstacleAvoidanceManager.ShouldAvoid, stopping.)
    let safe = p.plane.as_ref().and_then(|f| {
        (f.moving && !f.reached(loc) && f.avoidance)
            .then(|| {
                super::motion::plane_should_avoid(p, view, f.destination, f.avoid_min_separation)
            })
            .flatten()
    });
    let Some(f) = p.plane.as_mut() else { return };
    f.desired_rotation = match facing {
        Facing::MovementDirection if !f.reached(loc) => toward(f.destination),
        Facing::EnemyLocation => toward(enemy),
        Facing::Focus => toward(focus),
        _ => f.rotation,
    };
    if f.moving && !f.reached(loc) {
        match safe {
            Some(at) => {
                f.desired_location = at;
                if f.boosting {
                    f.desired_rotation = toward(at);
                }
            }
            None => f.desired_location = f.destination,
        }
    }
}

/// HoverPhysics.MoveToTargetLocation / RotateToTargetOrientation's control: the
/// rate wanted to close the gap this step (no faster than it could still stop
/// from), and the change to it no more than the acceleration allows.
fn approach(gap: f32, rate: f32, dt: f32, max_speed: f32, max_accel: f32) -> f32 {
    let cap = (max_accel * gap.abs()).sqrt().min(max_speed);
    let want = (gap / dt).clamp(-cap, cap);
    rate + ((want - rate) / dt).clamp(-max_accel, max_accel) * dt
}

/// HoverPhysics.MoveToTargetLocation: its velocity steered at `target`, no faster
/// than it could still stop from (nor its top speed), changed no faster than its
/// acceleration.
fn move_to_target(
    vel: Vec3,
    from: Vec3,
    target: Vec3,
    dt: f32,
    max_speed: f32,
    max_accel: f32,
) -> Vec3 {
    let to = target - from;
    let distance = to.length();
    let want = (to / dt).clamp_length_max((max_accel * distance).sqrt().min(max_speed));
    let change = ((want - vel) / dt).clamp_length_max(max_accel);
    vel + change * dt
}

/// HoverPhysics.RotateToTargetOrientation (about each of its axes) with its
/// rigid body's angular damping and top spin: its rotation and spin a step on.
fn rotate_to_target(
    f: &mut PlaneForm,
    target: Vec3,
    dt: f32,
    max_speed: f32,
    max_accel: f32,
    damping: f32,
) {
    let k = std::f32::consts::TAU / 65536.0;
    for axis in 0..3 {
        let gap = ue::wrap_rot(target[axis] - f.rotation[axis]) * k;
        let mut s = approach(gap, f.spin[axis], dt, max_speed, max_accel);
        s /= 1.0 + damping * dt;
        f.spin[axis] = s.clamp(-MAX_ANGULAR_VELOCITY, MAX_ANGULAR_VELOCITY);
        f.rotation[axis] = ue::wrap_rot(f.rotation[axis] + f.spin[axis] * dt / k);
    }
}

/// The plane form's frame (TnPlaneForm.OnUpdate for an AI: OnUpdateAi in its
/// state): hovering (its Hovering state's TnHoverPlaneSimulation), or, for a form
/// that can, flying (its Flying state's TnPlaneSimulation); between them as its
/// boost input asks (UpdateBoosting: not while it drifts back into its hover, nor
/// for a while after a collision as it flew: OnRigidBodyCollision).
pub fn plane_update(p: &mut Pawn, view: &WorldView, dt: f32) {
    let Some(f) = p.plane.as_mut() else { return };
    if f.flight.is_some() {
        if f.boosting {
            // Flying.UpdateBoosting.
            if !f.boost_input {
                f.boosting = false;
                f.drifting = true;
            }
        } else {
            // Hovering.OnUpdateAi: its collision countdown, UpdateBoosting.
            f.collision_countdown -= dt;
            if f.collision_countdown <= 0.0 && f.boost_input && !f.drifting {
                f.boosting = true;
                f.previous_rotation_delta = Vec3::ZERO;
            }
        }
    }
    if f.boosting {
        fly(p, view, dt);
    } else {
        hover(p, view, dt);
    }
}

/// A rigid body hovering a step (the plane form's Hovering.OnUpdateAi:
/// TnHoverPlaneSimulation.UpdateFromLocationAndRotation, no gravity): its velocity
/// steered at its desired location (MoveToTargetLocation, damped the harder near
/// it) and its body turned to its desired rotation leaning into its moves
/// (AddMotionPhysics, RotateToTargetOrientation), then moved against the level,
/// sliding along what it meets.
pub fn hover(p: &mut Pawn, view: &WorldView, dt: f32) {
    let Some(mut f) = p.plane.take() else { return };
    // (Its drift over: UpdateFromLocationAndRotation's.)
    f.drifting = false;
    let old = p.vel;
    let mut v = move_to_target(
        p.vel,
        p.loc,
        f.desired_location,
        dt,
        f.max_speed,
        f.max_accel,
    );
    // The rigid body's damping.
    let damping = if (f.desired_location - p.loc).length_squared() < f.hover.stopping_distance_sq {
        STOP_LINEAR_DAMPING
    } else {
        DEFAULT_LINEAR_DAMPING
    };
    v /= 1.0 + damping * dt;
    // AddMotionPhysics: its lean from its smoothed local acceleration and spin.
    let scale = f.hover.motion_physics_scale;
    let local_accel = ue::rotate_yaw((v - old) / dt, -f.rotation.y);
    f.smooth_accel = f.smooth_accel.lerp(local_accel, SMOOTH_ACCEL);
    f.smooth_spin = f.smooth_spin.lerp(f.spin, SMOOTH_SPIN);
    let mut target = f.desired_rotation;
    target.z += (f.smooth_accel.y * scale + f.smooth_spin.y * 1000.0 * scale).trunc();
    target.x -= (f.smooth_accel.x * 0.5 * scale).trunc();
    // RotateToTargetOrientation, its damping and spin limit.
    let (speed, accel) = (f.hover.max_angular_speed, f.hover.max_angular_acceleration);
    rotate_to_target(&mut f, target, dt, speed, accel, ANGULAR_DAMPING);
    f.accel = local_accel;
    p.yaw = f.rotation.y;
    p.desired_yaw = p.yaw;
    p.plane = Some(f);
    p.vel = v;
    super::pawn::move_rigid_body(p, view, dt);
}

/// A plane flying a step (Flying.OnUpdateAi: TnPlaneSimulation's
/// UpdateFromLocationAndRotation): it steers at a point ahead along its nose (as
/// far as its desired location, no farther than its forward steering target; the
/// desired location itself while it changes form), turning to its desired rotation
/// with the extra turn and the roll a turn gives it (CalculateFinalRotationToMatch,
/// CalculateAIYawValue); a collision drops it back to its hover for its
/// CollisionReactionTime.
pub fn fly(p: &mut Pawn, view: &WorldView, dt: f32) {
    let Some(mut f) = p.plane.take() else { return };
    let Some(fv) = f.flight else {
        p.plane = Some(f);
        return hover(p, view, dt);
    };
    let [forward, ..] = ue::rotator_axes(f.rotation.x, f.rotation.y, f.rotation.z);
    let target = if super::forms::is_transforming(p) {
        f.desired_location
    } else {
        let distance = (p.loc - f.desired_location)
            .length()
            .min(fv.forward_steering);
        p.loc + forward * distance
    };
    let mut v = move_to_target(p.vel, p.loc, target, dt, f.max_speed, f.max_accel);
    v /= 1.0 + FLIGHT_LINEAR_DAMPING * dt;
    let final_rotation = final_rotation_to_match(&mut f, &fv);
    let speed = fv.max_turn_speed * f.turn_modifier;
    let accel = fv.max_turn_acceleration * f.turn_modifier;
    rotate_to_target(
        &mut f,
        final_rotation,
        dt,
        speed,
        accel,
        FLIGHT_ANGULAR_DAMPING,
    );
    p.yaw = f.rotation.y;
    p.desired_yaw = p.yaw;
    p.plane = Some(f);
    p.vel = v;
    super::pawn::move_rigid_body(p, view, dt);
    // Flying.OnRigidBodyCollision (an AI's): back to its hover for a while.
    if p.notes.hit_wall.is_some()
        && let Some(f) = p.plane.as_mut()
    {
        f.collision_countdown = fv.collision_reaction_time;
        f.boosting = false;
        f.drifting = true;
    }
}

/// TnPlaneSimulation.CalculateFinalRotationToMatch for an AI (no pitch input; its
/// yaw value CalculateAIYawValue's: how far round it has yet to turn, by which
/// way): its desired rotation turned on by YawDueToYawValue and rolled by
/// RollDueToYawValue in its own axes, by the square of the yaw value, eased in
/// from the last (ExtraRotationLerpValue).
fn final_rotation_to_match(f: &mut PlaneForm, fv: &FlightValues) -> Vec3 {
    let flat = |r: Vec3| ue::normal(ue::vec_of(r.x, r.y) * Vec3::new(1.0, 1.0, 0.0));
    let facing = flat(f.rotation);
    let desired = flat(f.desired_rotation);
    let yaw_value = (1.0 - facing.dot(desired)) * desired.dot(ue::rotate_yaw(facing, 16384.0));
    let yaw_squared = yaw_value.abs() * yaw_value;
    let delta = Vec3::new(
        0.0,
        yaw_squared * fv.yaw_due_to_yaw * ue::ROT_PER_DEG,
        yaw_squared * fv.roll_due_to_yaw * ue::ROT_PER_DEG,
    );
    let delta =
        f.previous_rotation_delta + (delta - f.previous_rotation_delta) * fv.extra_rotation_lerp;
    f.previous_rotation_delta = delta;
    // (TransformRotator: the delta in the desired rotation's own axes.)
    ue::quat_to_rotator(ue::quat_from_rotator(f.desired_rotation) * ue::quat_from_rotator(delta))
}

// ---------------------------------------------------------------------------
// The drones' path following
// ---------------------------------------------------------------------------

/// TnDronePathFollowingSensor.kAboveDestinationOffset, kPathStartOffset and
/// kRepathMoveDistance.
const ABOVE_DESTINATION: f32 = 500.0;
const PATH_START_DOWN: f32 = 1000.0;
const REPATH_MOVE_DISTANCE: f32 = 50.0;
/// How far from a place its best anchor may be (Pawn.GetBestAnchor's search:
/// metres).
const ANCHOR_REACH: f32 = 10.0;

/// EDronePathFollowingState.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DroneState {
    #[default]
    Idle = 0,
    MovingToDestination = 1,
    MovingAboveDestination = 2,
    FollowingPath = 3,
    MovingOntoNavMesh = 4,
}

/// TnDronePathFollowingSensor: straight to its destination if it can fly there,
/// else to over it, else along a channel of the navigation mesh (its corners
/// pulled with its radius, HoverHeight over them), else back to the last place
/// on its path; WS_Navigation and WS_Location for the planner.
#[derive(Clone, Debug, Default)]
pub struct DronePathFollowing {
    pub hover_height: f32,
    pub state: DroneState,
    pub destination_reference: usize,
    pub range: i32,
    pub current_destination: Vec3,
    pub move_to_location: Vec3,
    pub channel: Vec<u32>,
    /// Its portals (the demo's space, metres) and the one it is at.
    pub portals: Vec<Portal>,
    pub portal_index: usize,
    pub last_location_on_path: Vec3,
    pub last_location_on_path_valid: bool,
}

impl DronePathFollowing {
    pub fn new(hover_height: f32) -> DronePathFollowing {
        DronePathFollowing {
            hover_height,
            ..Default::default()
        }
    }

    /// get_IsMoving.
    fn is_moving(b: &Brain) -> bool {
        b.core.ws(htn::WS_NAVIGATION) != 0
    }

    /// MoveToLocation: done if it is in range already; else under way (WS_Navigation
    /// the reference, WS_Location none) if it finds a way.
    pub fn move_to_location(
        &mut self,
        b: &mut Brain,
        ctx: &mut Ctx,
        reference: usize,
        range: i32,
        speed: u8,
    ) -> bool {
        self.destination_reference = reference;
        self.range = range;
        let desired = b.core.fetch(reference);
        if range_to(ctx.pawn, desired) <= range as f32 {
            self.complete_move(b, ctx, true);
            return true;
        }
        if let Some(f) = ctx.pawn.plane.as_mut() {
            f.set_speed(speed);
        }
        b.core.set_ws(htn::WS_NAVIGATION, reference as i32);
        b.core.set_ws(htn::WS_LOCATION, 0);
        if !self.move_to_destination(ctx, desired) {
            self.complete_move(b, ctx, false);
            return false;
        }
        true
    }

    /// OnUpdate.
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        if !Self::is_moving(b) {
            return;
        }
        let desired = b.core.fetch(self.destination_reference);
        let moved = (desired - self.current_destination).length();
        if range_to(ctx.pawn, desired) <= self.range as f32 {
            self.complete_move(b, ctx, true);
            return;
        }
        // Its destination moved on: a new way there, if it can; where it was
        // making for reached: on to the next leg, if there is one.
        let reached = |ctx: &Ctx| {
            ctx.pawn
                .plane
                .as_ref()
                .is_some_and(|f| f.reached(ctx.pawn.loc))
        };
        let failed = (moved > REPATH_MOVE_DISTANCE && !self.move_to_destination(ctx, desired))
            || (reached(ctx) && !self.update_state_when_destination_reached(ctx, desired));
        if failed {
            self.complete_move(b, ctx, false);
        }
    }

    /// UpdateStateWhenDestinationReached: on from over its destination down to
    /// it, to the next corner of its path, or from the last place on its path
    /// along a new one.
    fn update_state_when_destination_reached(&mut self, ctx: &mut Ctx, desired: Vec3) -> bool {
        match self.state {
            DroneState::MovingAboveDestination => {
                self.state = DroneState::MovingToDestination;
                self.current_destination = desired;
                self.move_to(ctx, desired, true)
            }
            DroneState::FollowingPath => self.move_to_next_portal(ctx, false),
            DroneState::MovingOntoNavMesh => {
                self.state = DroneState::FollowingPath;
                self.current_destination = desired;
                self.find_channel(ctx, desired) && self.move_to_next_portal(ctx, true)
            }
            _ => true,
        }
    }

    /// MoveToDestination.
    fn move_to_destination(&mut self, ctx: &mut Ctx, destination: Vec3) -> bool {
        self.current_destination = destination;
        if self.move_to(ctx, destination, true) {
            self.state = DroneState::MovingToDestination;
            return true;
        }
        if self.move_to(ctx, destination + Vec3::Z * ABOVE_DESTINATION, true) {
            self.state = DroneState::MovingAboveDestination;
            return true;
        }
        if self.find_channel(ctx, destination) && self.move_to_next_portal(ctx, true) {
            self.state = DroneState::FollowingPath;
            return true;
        }
        if self.last_location_on_path_valid {
            let last = self.last_location_on_path;
            if self.move_to(ctx, last, true) {
                self.state = DroneState::MovingOntoNavMesh;
                return true;
            }
        }
        false
    }

    /// FindChannel: from the anchor under it to its destination's, through the
    /// links it can take (its form's movement: walking and jumping anything).
    fn find_channel(&mut self, ctx: &mut Ctx, destination: Vec3) -> bool {
        let p = &*ctx.pawn;
        let Some(nav) = ctx.view.nav else {
            return false;
        };
        let (start_location, _) = can_reach(p, ctx.view, p.loc - Vec3::Z * PATH_START_DOWN);
        let Some((start, _)) = nav.nearest_cell(ue::from_ue(start_location), ANCHOR_REACH) else {
            return false;
        };
        let Some((end, _)) = nav
            .nearest_cell(ue::from_ue(destination), ANCHOR_REACH)
            .or_else(|| {
                let (down, _) = can_reach_from(
                    ctx.view,
                    destination,
                    destination - Vec3::Z * PATH_START_DOWN,
                    p,
                );
                nav.nearest_cell(ue::from_ue(down), ANCHOR_REACH)
            })
        else {
            return false;
        };
        let moves = Moves::ground(p.radius / ue::UU_PER_M, p.half_height * 2.0 / ue::UU_PER_M);
        let Some(channel) = nav.find_channel_to(start, end, ue::from_ue(destination), &moves)
        else {
            return false;
        };
        self.portals = nav.build_portals(ue::from_ue(p.loc), ue::from_ue(destination), &channel);
        self.channel = channel;
        if self.portals.is_empty() {
            return false;
        }
        self.portal_index = 0;
        true
    }

    /// MoveToNextPortal: the next corner (StringPulling.FunnelWithRadius from the
    /// portal it is at), HoverHeight over it; the middle of the first one it
    /// passes kept as the last place on its path.
    fn move_to_next_portal(&mut self, ctx: &mut Ctx, check_reachability: bool) -> bool {
        let p = &*ctx.pawn;
        let start = Corner::new(CornerType::Point, self.portal_index, ue::from_ue(p.loc));
        let next = crate::nav::funnel_with_radius(start, p.radius / ue::UU_PER_M, &self.portals);
        let at = ue::to_ue(next.at);
        if at == p.loc {
            return false;
        }
        if !self.move_to(ctx, at + Vec3::Z * self.hover_height, check_reachability) {
            return false;
        }
        if self.portal_index != 0
            && let Some(portal) = self.portals.get(next.portal)
        {
            self.last_location_on_path = ue::to_ue((portal.left + portal.right) * 0.5);
            self.last_location_on_path_valid = true;
        }
        self.portal_index = next.portal;
        true
    }

    /// MoveTo: if it can fly there (or isn't asked to check), its motion
    /// controller off to it (MoveToPoint, not stopping there, within half its
    /// range).
    fn move_to(&mut self, ctx: &mut Ctx, location: Vec3, check_reachability: bool) -> bool {
        if check_reachability && !can_reach(ctx.pawn, ctx.view, location).1 {
            return false;
        }
        self.move_to_location = location;
        let loc = ctx.pawn.loc;
        if let Some(f) = ctx.pawn.plane.as_mut() {
            f.move_to_point(loc, location, 0.5 * self.range as f32);
        }
        true
    }

    /// CompleteMove: stopped; WS_Location where it got to (none if it didn't),
    /// WS_Navigation off, and a failed one's path blocked (TnPathingSensor).
    fn complete_move(&mut self, b: &mut Brain, ctx: &mut Ctx, succeeded: bool) {
        self.state = DroneState::Idle;
        let (loc, vel) = (ctx.pawn.loc, ctx.pawn.vel);
        if let Some(f) = ctx.pawn.plane.as_mut() {
            f.stop(loc, vel);
        }
        let reference = if succeeded {
            self.destination_reference
        } else {
            LR_NONE
        };
        b.core.set_ws(htn::WS_LOCATION, reference as i32);
        b.core.set_ws(htn::WS_NAVIGATION, 0);
        if !succeeded {
            let lr = self.destination_reference;
            let p = &*ctx.pawn;
            with_sensor!(b, pathing, s => s.path_blocked(b, p, lr));
        }
    }
}

/// AI.CalculateRangeToPoint: from its cylinder to the point.
pub fn range_to(p: &Pawn, point: Vec3) -> f32 {
    ue::range_point_to_point(p.loc, p.radius, point, 0.0, p.half_height, 0.0)
}

/// The level stopping a box of `extent` (half sizes: x and y across, z up) swept
/// from `from` to `to` (a Trace with an extent): where its middle is as it meets
/// what stops it, and the normal there. Rays from its middle and its sides; each
/// one's stop drawn back by as much of the box's reach towards what it struck
/// (its extent along the normal) as lies ahead of the ray, as far as the way it
/// goes closes on that.
pub fn sweep_box(view: &WorldView, from: Vec3, to: Vec3, extent: Vec3) -> Option<(Vec3, Vec3)> {
    let way = to - from;
    let len = way.length();
    if len < 1e-3 {
        return None;
    }
    let dir = way / len;
    let flat = Vec3::new(way.x, way.y, 0.0);
    let side = if flat.length_squared() > 1e-6 {
        Vec3::new(-flat.y, flat.x, 0.0).normalize() * extent.x
    } else {
        Vec3::X * extent.x
    };
    let up = Vec3::Z * extent.z;
    let mut best: Option<(f32, Vec3)> = None;
    for o in [Vec3::ZERO, side, -side, up, -up] {
        let t = view.sweep_time(from + o, to + o);
        if t >= 1.0 {
            continue;
        }
        let normal = view.trace(from + o, to + o).map_or(-dir, |(_, n)| n);
        let reach =
            extent.x * normal.x.abs() + extent.y * normal.y.abs() + extent.z * normal.z.abs();
        let closing = -dir.dot(normal);
        let back = if closing > 0.05 {
            (reach + o.dot(normal)).max(0.0) / (closing * len)
        } else {
            0.0
        };
        let t = (t - back).max(0.0);
        if best.is_none_or(|(b, _)| t < b) {
            best = Some((t, normal));
        }
    }
    best.map(|(t, n)| (from + way * t, n))
}

/// TnDronePathFollowingSensor.CanReach: a trace of half its extent from where it
/// is clear to `to` (where it stops, and whether it got there).
fn can_reach(p: &Pawn, view: &WorldView, to: Vec3) -> (Vec3, bool) {
    can_reach_from(view, p.loc, to, p)
}

fn can_reach_from(view: &WorldView, from: Vec3, to: Vec3, p: &Pawn) -> (Vec3, bool) {
    let extent = Vec3::new(p.radius, p.radius, p.half_height) * 0.5;
    match sweep_box(view, from, to, extent) {
        Some((at, _)) => (at, false),
        None => (to, true),
    }
}

// ---------------------------------------------------------------------------
// The support drone's post, the explosive drone's word of its Titan, its lifespan
// ---------------------------------------------------------------------------

/// TnHoverDefendActorSensor: its post by its defend actor (LR_AttackPoint): its
/// DefendDistance out from it, away from the others defending it, over the floor
/// by its DefendHeight.
#[derive(Clone, Debug)]
pub struct HoverDefendActor {
    pub distance: f32,
    pub height: f32,
}

impl HoverDefendActor {
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let Some(defend) = b.sensors.defend_actor.and_then(|e| ctx.view.actor(e)) else {
            return;
        };
        let at = self.find_defend_location(b, ctx, &defend);
        b.core.store(LR_ATTACK_POINT, at, Vec3::ZERO);
    }

    /// FindDefendLocation.
    fn find_defend_location(&self, b: &Brain, ctx: &Ctx, defend: &super::world::ActorInfo) -> Vec3 {
        let p = &*ctx.pawn;
        let angle = defend_angle(p.loc, defend.loc);
        let angle = self.move_angle_away_from_defenders(b, ctx, defend, angle);
        let dir = ue::yaw_vec(angle);
        self.adjust_location_around_geometry(ctx, defend.loc, defend.loc + dir * self.distance)
    }

    /// MoveAngleAwayFromDefenders: midway between the nearest defenders either
    /// side of it.
    fn move_angle_away_from_defenders(
        &self,
        b: &Brain,
        ctx: &Ctx,
        defend: &super::world::ActorInfo,
        angle: f32,
    ) -> f32 {
        let p = &*ctx.pawn;
        let (mut min, mut max) = (-32767.0f32, 32767.0f32);
        for a in &ctx.view.ais {
            if a.entity == b.core.entity
                || a.defend != Some(defend.entity)
                || (a.loc - p.loc).length_squared() > (2.0 * self.distance).powi(2)
            {
                continue;
            }
            let local = normalize_rot_axis(defend_angle(a.loc, defend.loc) - angle);
            if local < 0.0 && local > min {
                min = local;
            } else if local > 0.0 && local < max {
                max = local;
            }
        }
        (angle + 0.5 * (min + max)).trunc()
    }

    /// AdjustLocationAroundGeometry: short of a wall in the way (its radius off
    /// it), and its height over the floor under it.
    fn adjust_location_around_geometry(&self, ctx: &Ctx, from: Vec3, desired: Vec3) -> Vec3 {
        let p = &*ctx.pawn;
        let extent = Vec3::new(p.radius, p.radius, p.half_height);
        let mut at = desired;
        if let Some((hit, n)) = sweep_box(ctx.view, from, at, extent) {
            at = hit + n * p.radius;
        }
        if let Some((hit, _)) = sweep_box(
            ctx.view,
            at + Vec3::Z * 1000.0,
            at - Vec3::Z * 1000.0,
            extent,
        ) {
            at = hit + Vec3::Z * self.height;
        }
        at
    }
}

/// GetDefendAngle: its yaw round its defend actor (rotation units, -32768..32767).
fn defend_angle(at: Vec3, defend: Vec3) -> f32 {
    normalize_rot_axis(ue::yaw_of(at - defend).round())
}

/// Core.Math.NormalizeRotAxis.
fn normalize_rot_axis(a: f32) -> f32 {
    let mut a = (a as i32) & 0xFFFF;
    if a > 32767 {
        a -= 65536;
    }
    a as f32
}

/// TnDefendActorStatusSensor (its ranges none: WS_DefendActorRange 0) and the
/// ground under its defend actor (LR_NavigableLocationNearDefendActor).
#[derive(Clone, Debug, Default)]
pub struct DefendActorStatus {
    pub ranges: Vec<f32>,
}

impl DefendActorStatus {
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let defend = b.sensors.defend_actor.and_then(|e| ctx.view.actor(e));
        // (Its LeadingOffset: none.)
        let range = match defend {
            None => 0,
            Some(d) => {
                let r = range_to(ctx.pawn, d.loc);
                self.ranges
                    .iter()
                    .position(|x| r < *x)
                    .unwrap_or(self.ranges.len()) as i32
            }
        };
        b.core.set_ws(htn::WS_DEFEND_ACTOR_RANGE, range);
        let near = match defend {
            None => b.core.fetch(LR_NAVIGABLE_LOCATION_NEAR_DEFEND_ACTOR),
            Some(d) => ctx
                .view
                .trace(d.loc, d.loc - Vec3::Z * 10000.0)
                .map_or_else(
                    || b.core.fetch(LR_NAVIGABLE_LOCATION_NEAR_DEFEND_ACTOR),
                    |(at, _)| at,
                ),
        };
        b.core
            .store(LR_NAVIGABLE_LOCATION_NEAR_DEFEND_ACTOR, near, Vec3::ZERO);
    }
}

/// TnLifespanSensor: once its time is up it destroys itself
/// (TnDamageTypeSelfDestruct).
#[derive(Clone, Debug)]
pub struct Lifespan {
    pub left: f32,
    pub active: bool,
}

impl Lifespan {
    pub fn update(&mut self, b: &Brain, ctx: &mut Ctx, dt: f32) {
        if !self.active {
            return;
        }
        self.left -= dt;
        if self.left > 0.0 {
            return;
        }
        ctx.shared.kills.push((b.core.entity, Death::SelfDestruct));
        self.active = false;
    }
}

// ---------------------------------------------------------------------------
// The support drone's aura
// ---------------------------------------------------------------------------

/// TnBuffAuraSupportDroneDamage.TickBuff: its shield on its defend actor while
/// that lives (OnEnterAura: one of the appliers of its TnBuffSupportDroneDamage),
/// else off it (OnExitAura) and the drone dead (Kill, TnDamageTypeAssassination).
pub fn tick_aura(b: &Brain, ctx: &mut Ctx) {
    let me = b.core.entity;
    let defend = b.sensors.defend_actor;
    let alive = defend
        .and_then(|e| ctx.view.actor(e))
        .is_some_and(|a| !a.dead);
    let shields = &mut ctx.shared.shields;
    match defend {
        Some(d) if alive => {
            if !shields.iter().any(|&(on, by)| on == d && by == me) {
                shields.push((d, me));
            }
        }
        _ => {
            shields.retain(|&(_, by)| by != me);
            if !ctx.pawn.dying {
                ctx.shared.kills.push((me, Death::Plain));
            }
        }
    }
}

/// OnUnapplyOrOwnerDied: a dead drone's shield off its defend actor.
pub fn drone_died(shared: &mut super::shared::Shared, e: Entity) {
    shared.shields.retain(|&(_, by)| by != e);
}

// ---------------------------------------------------------------------------
// Their effects and sounds
// ---------------------------------------------------------------------------

/// The shield on a Titan (TnBuffSupportDroneDamage's ApplyFx, looping, on its
/// FxSocket).
pub const SHIELD_FX: &str = "ShieldHeal_FX";
const SHIELD_FX_SOCKET: &str = "shieldfxsocket";
/// A support drone's beam (TnBuffAuraSupportDroneDamage's _Beam: its Template), to
/// its Titan's chest (UpdateHealBeam's socket).
const BEAM_FX: &str = "ChainBeam_MainBeam_FX";
const BEAM_SOCKET: &str = "Chest_FX";
/// Their engine (their vehicle form's HmPlayerVehicleAudioComponent: its drive
/// and off-load loops, both VEH_SENTINEL's SENTINEL_ENGINE_LP), held while they
/// fly, and the time it fades out over once they are gone.
pub const ENGINE_EVENT: &str = "Auto_Engine_Gear_1_OnLoad";
pub const ENGINE_FADE: f32 = 0.25;
/// Their rigid body (TR_Sentinel_VEHDEF_p.VH_Sentinel_PHYSSYS: a box 329.5 by
/// 329.2 by 358.3 on C_Reference_XR, which the vehicle form centres on the pawn's
/// Location, CalculateCylinderBounds), as the cylinder they meet the level with:
/// its half width and half height.
pub const BODY_RADIUS: f32 = 164.8;
pub const BODY_HALF_HEIGHT: f32 = 179.2;

/// TnExplosiveDroneCharacter.OnApplyCharacterProperties's effects: its
/// EffectTemplate played on each of its EffectSockets (PlayEffectOnSocket: held on
/// it), its flight's glow.
pub fn play_flight_effects(
    b: &Behaviour,
    by: Entity,
    p: &Pawn,
    shared: &mut super::shared::Shared,
) {
    let Some(template) = b.get("EffectTemplate").filter(|t| *t != "None") else {
        return;
    };
    let system: Arc<str> = Arc::from(template.rsplit('.').next().unwrap_or(template));
    for socket in b.get("EffectSockets").unwrap_or("").split_whitespace() {
        let (at, axes) = p.socket_frame(socket).unwrap_or_else(|| {
            let (x, y, z) = super::weapon::axes(0.0, p.yaw);
            (p.loc, [x, y, z])
        });
        shared.effects.push(super::shared::Effect {
            by,
            system: system.clone(),
            socket: Arc::from(socket),
            at,
            axes,
            offset: Vec3::ZERO,
            turn: Vec3::ZERO,
            attached: true,
            only_once: false,
            restart: false,
            params: None,
            auto_off: None,
        });
    }
}

/// TnBuffSupportDroneDamage's looping ApplyFx (IsApplyFxLooping:
/// StartLoopingEffects as it is applied, StopLoopingEffects as it comes off): the
/// shield's effect on each AI its support drones shield, while they do.
pub fn shield_effects(shared: &mut super::shared::Shared, slots: &mut [super::Slot<'_>]) {
    for s in slots.iter_mut() {
        let on = shielded(shared, s.entity) && !s.ai.pawn.dying;
        let p = &mut s.ai.pawn;
        if on == p.shield_fx {
            continue;
        }
        p.shield_fx = on;
        if !on {
            shared
                .effects_stopped
                .push((s.entity, Arc::from(SHIELD_FX)));
            continue;
        }
        let (at, axes) = p.socket_frame(SHIELD_FX_SOCKET).unwrap_or_else(|| {
            let (x, y, z) = super::weapon::axes(0.0, p.yaw);
            (p.loc, [x, y, z])
        });
        shared.effects.push(super::shared::Effect {
            by: s.entity,
            system: Arc::from(SHIELD_FX),
            socket: Arc::from(SHIELD_FX_SOCKET),
            at,
            axes,
            offset: Vec3::ZERO,
            turn: Vec3::ZERO,
            attached: true,
            only_once: true,
            restart: false,
            params: None,
            auto_off: None,
        });
    }
}

/// The support drones' beams, each its effect's id by its drone's.
#[derive(Resource, Default)]
pub struct DroneBeams(Vec<(Entity, crate::levelfx::FxId)>);

/// Draws each living support drone's beam to its Titan (its aura's _Beam: from
/// the drone, attached to it, to its Titan's Chest_FX where this frame's pose put
/// it, turned along the way: UpdateHealBeam, each tick), after read_sockets; the
/// beam of a drone that died or is gone is let go.
pub fn draw_beams(
    ais: Query<(Entity, &super::Ai)>,
    mut fx: ResMut<crate::levelfx::FxSystems>,
    mut beams: ResMut<DroneBeams>,
) {
    let mut drawn = Vec::new();
    for (e, ai) in &ais {
        let p = &ai.pawn;
        if !p.drone_aura || p.dying {
            continue;
        }
        let Some((_, titan)) = ai.brain.sensors.defend_actor.and_then(|t| ais.get(t).ok()) else {
            continue;
        };
        let t = &titan.pawn;
        let to = t.socket_frame(BEAM_SOCKET).map_or(t.loc, |(at, _)| at);
        let (from, to) = (ue::from_ue(p.loc), ue::from_ue(to));
        let dir = (to - from).normalize_or(Vec3::X);
        let id = match beams.0.iter().find(|(b, id)| *b == e && fx.alive(*id)) {
            Some(&(_, id)) => id,
            None => {
                let Some(id) = fx.spawn(BEAM_FX, from, dir, &[]) else {
                    continue;
                };
                beams.0.push((e, id));
                id
            }
        };
        fx.place(id, from, dir, Vec3::ZERO);
        fx.set_beam_end(id, to);
        drawn.push(e);
    }
    beams.0.retain(|&(e, id)| {
        let keep = drawn.contains(&e) && fx.alive(id);
        if !keep {
            fx.stop(id);
        }
        keep
    });
}

/// Every beam let go (a new match).
pub fn clear_beams(mut fx: ResMut<crate::levelfx::FxSystems>, mut beams: ResMut<DroneBeams>) {
    for (_, id) in beams.0.drain(..) {
        fx.stop(id);
    }
}

// ---------------------------------------------------------------------------
// Their operators
// ---------------------------------------------------------------------------

/// TnDroneMoveToOperator.OnStart: its motion allowed and its path following off
/// to the task's destination (done at once: its plan idles while it moves).
pub fn drone_move_to(b: &mut Brain, ctx: &mut Ctx, task: &htn::Task) -> bool {
    if b.sensors.drone_path.is_none() {
        return false;
    }
    b.add_behavior(ctx, BEHAVIOR_ALLOW_MOTION);
    let destination = task.int(0).max(0) as usize;
    let range = task.int(1);
    let speed = task.int(2).clamp(0, 2) as u8;
    with_sensor!(b, drone_path, s => s.move_to_location(b, ctx, destination, range, speed))
        .unwrap_or(false)
}

/// TnHoverAtLocationOperator's state: the reference it holds, the place it went
/// for, and how far it may drift (Range) before going again or failing (twice
/// that).
#[derive(Clone, Debug, Default)]
pub struct HoverAtLocation {
    pub reference: usize,
    pub location: Vec3,
    pub update_range_sq: f32,
    pub fail_range_sq: f32,
}

impl HoverAtLocation {
    pub fn start(&mut self, b: &Brain, ctx: &mut Ctx, task: &htn::Task) -> bool {
        if ctx.pawn.plane.is_none() {
            return false;
        }
        self.reference = task.int(0).max(0) as usize;
        let range = task.int(1) as f32;
        self.update_range_sq = range * range;
        self.fail_range_sq = (2.0 * range) * (2.0 * range);
        self.move_to(ctx, b.core.fetch(self.reference));
        true
    }

    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) -> Status {
        let location = b.core.fetch(self.reference);
        if (ctx.pawn.loc - self.location).length_squared() > self.fail_range_sq {
            b.core.set_ws(htn::WS_LOCATION, 0);
            return Status::Failed;
        }
        if (location - self.location).length_squared() > self.update_range_sq {
            self.move_to(ctx, location);
        }
        Status::Running
    }

    /// MoveTo: its motion controller to the place (stopping there, within 50 uu).
    fn move_to(&mut self, ctx: &mut Ctx, location: Vec3) {
        self.location = location;
        let loc = ctx.pawn.loc;
        if let Some(f) = ctx.pawn.plane.as_mut() {
            f.move_to_point(loc, location, 50.0);
        }
    }
}

/// TnExplodeOperator.OnStart: its blast (HurtRadius: the task's damage to every
/// player within its radius it has a line to, in full, no momentum), and its
/// death (Kill, its blueprint's DamageType: TnDamageTypeExplodeOnDeath).
pub fn explode(b: &Brain, ctx: &mut Ctx, task: &htn::Task) -> bool {
    let damage = task.int(0) as f32;
    let radius = task.int(1) as f32;
    let at = ctx.pawn.loc;
    for pl in &ctx.view.players {
        if !pl.alive {
            continue;
        }
        let reach = ue::range_point_to_point(at, 0.0, pl.loc, pl.radius, 0.0, pl.half_height);
        if reach > radius || !ctx.view.fast_trace(at, pl.loc) {
            continue;
        }
        ctx.shared.player_damage.push(super::shared::PlayerDamage {
            player: pl.entity,
            by: b.core.entity,
            amount: damage,
            at: pl.loc,
            dir: ue::normal(pl.loc - at),
            melee: false,
            momentum: None,
            jam: false,
            shot: None,
            damage: "TnDamageTypeExplodeOnDeath",
        });
    }
    ctx.shared.kills.push((b.core.entity, Death::SelfDestruct));
    true
}

/// TnChooseNewDefendActorLocationOperator.OnStart: its Titan's ground
/// (LR_NavigableLocationNearDefendActor; where it is, with none) its place to go
/// (LR_AttackPoint).
pub fn choose_new_defend_actor_location(b: &mut Brain, ctx: &Ctx) -> bool {
    let at = if b.sensors.defend_actor.is_none() {
        ctx.pawn.loc
    } else {
        b.core.fetch(LR_NAVIGABLE_LOCATION_NEAR_DEFEND_ACTOR)
    };
    b.core.store(LR_ATTACK_POINT, at, Vec3::ZERO);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::htn::{Task, WS_NUM, Ws, build_plan};

    fn ws() -> [Ws; WS_NUM] {
        let mut w = [Ws::default(); WS_NUM];
        w[htn::WS_AREA_CLEAR].value = 1;
        w[htn::WS_BALANCED].value = 1;
        w
    }

    fn plan(d: &Domain, w: &[Ws; WS_NUM]) -> Vec<String> {
        build_plan(d, Task::new(0, &[]), w)
            .plan
            .iter()
            .map(|t| d.name(t.symbol).to_string())
            .collect()
    }

    /// The support drone's domain as its script builds it: off to its post, idling
    /// on the way, holding it once there.
    #[test]
    fn a_support_drone_goes_to_its_post_and_holds_it() {
        let d = support_drone_domain();
        assert_eq!(d.debug_flags, 1);
        let mut w = ws();
        assert_eq!(plan(&d, &w), ["SDTS_MoveTo"]);
        let b = build_plan(&d, Task::new(0, &[]), &w);
        assert_eq!(b.plan[0].int(0), LR_ATTACK_POINT as i32);
        assert_eq!(b.plan[0].int(1), 10);
        w[htn::WS_NAVIGATION].value = LR_ATTACK_POINT as i32;
        assert_eq!(plan(&d, &w), ["SDTS_IdleWhileMoving"]);
        w[htn::WS_NAVIGATION].value = 0;
        w[htn::WS_LOCATION].value = LR_ATTACK_POINT as i32;
        let b = build_plan(&d, Task::new(0, &[]), &w);
        assert_eq!(d.name(b.plan[0].symbol), "SDTS_IdleWhileStationary");
        assert_eq!(b.plan[0].int(0), LR_ATTACK_POINT as i32);
        assert_eq!(b.plan[0].int(1), 50);
        assert!(d.is_used(htn::WS_PATH_BLOCKED) && d.is_used(htn::WS_LOCATION));
        assert!(!d.is_used(htn::WS_ENEMY_VISIBLE));
    }

    /// The explosive drone's: at its enemy, its fuse and its blast; out of sight,
    /// its enemy's last place, then back to its Titan.
    #[test]
    fn an_explosive_drone_flies_at_its_enemy_and_blows() {
        let b = Behaviour {
            id: "ExplosiveDrone".into(),
            class: "TnExplosiveDroneCharacter".into(),
            props: [
                ("TriggerExplosionRadius", "500"),
                ("SecondBeforeExplosion", "2"),
                ("Damage", "50"),
                ("DamageRadius", "1000"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            ..Default::default()
        };
        let d = script_domain(&b).unwrap();
        let mut w = ws();
        w[htn::WS_ENEMY_VISIBLE].value = 1;
        w[htn::WS_ENEMY_RANGE].value = 1;
        let built = build_plan(&d, Task::new(0, &[]), &w);
        assert_eq!(d.name(built.plan[0].symbol), "EDTS_MoveTo");
        assert_eq!(built.plan[0].int(0), LR_CONFIRMED_ENEMY as i32);
        assert_eq!(built.plan[0].int(1), 500);
        // Its expected effect is the sensors' to set (WillBeSetExternally).
        let op = d.op(built.plan[0].symbol).unwrap();
        assert!(
            op.eff
                .iter()
                .any(|e| e.attr == htn::WS_ENEMY_RANGE && e.external)
        );
        w[htn::WS_LOCATION].value = LR_CONFIRMED_ENEMY as i32;
        w[htn::WS_ENEMY_RANGE].value = 0;
        let built = build_plan(&d, Task::new(0, &[]), &w);
        let names: Vec<&str> = built.plan.iter().map(|t| d.name(t.symbol)).collect();
        assert_eq!(names, ["EDTS_PlayAnimation", "EDTS_Explode"]);
        assert_eq!(built.plan[0].int(1), 2);
        assert_eq!((built.plan[1].int(0), built.plan[1].int(1)), (50, 1000));
        // Out of sight: its enemy's last place; been there, its Titan.
        let mut w = ws();
        assert_eq!(plan(&d, &w), ["EDTS_MoveTo"]);
        w[htn::WS_LOCATION].value = super::super::brain::LR_SUSPECTED_ENEMY as i32;
        assert_eq!(plan(&d, &w), ["EDTS_IdleWhileStationary"]);
        w[htn::WS_LOCATION].value = 0;
        w[htn::WS_TIRED].value = 1;
        assert_eq!(plan(&d, &w), ["EDTS_ChooseDefendLocation", "EDTS_MoveTo"]);
        // (ShouldReturnToDefendActor's effect: its Enable parameter's value.)
        let op = d.op(11).unwrap();
        assert_eq!(op.eff[0].value, -1);
    }

    /// The effects the drones play (their flight, beam, shield, fuse and deaths)
    /// are among the built ones (fx/weapons_fx.txt).
    #[test]
    fn their_effects_are_built() {
        let Ok(text) = std::fs::read_to_string(crate::assets_dir().join("fx/weapons_fx.txt"))
        else {
            return;
        };
        let built: std::collections::HashSet<&str> = text
            .lines()
            .filter_map(|l| {
                let mut w = l.split_whitespace();
                (w.next() == Some("system")).then(|| w.nth(1)).flatten()
            })
            .collect();
        for s in [
            "DroneFlight_FX",
            "TitanDrone_Attack_FX",
            BEAM_FX,
            SHIELD_FX,
            "Electrical_Death_FX",
            "Enforcer_Deathbeam_Squib_FX",
            "Exp_TitanDrone_FX",
        ] {
            assert!(built.contains(s), "{s} isn't built");
        }
    }
}
