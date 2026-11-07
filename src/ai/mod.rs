//! The enemies' AI: Fall of Cybertron's HTN AI (TnHtnAiController and its planner,
//! sensors, operators, motion and weapons), as the game's script runs it, on the
//! game's own planning domains and behaviour values (models/enemies/ai.txt).
//!
//! Each enemy the waves bring in gets its controller (a [`brain::Brain`]) and its
//! pawn (a [`pawn::Pawn`]: where it is and how it moves, in Unreal's space) once it
//! stands on the ground, as TnHtnAiController.Initialize sets them up (its
//! blueprint's properties, its squad, its sensors, its planner). Each frame, in the
//! order the game ticks them:
//!
//! * what happened to it since its last frame comes in as events: the blows it took
//!   (NotifyTakeHit: TookDamage), the players' gunfire it heard (their weapons'
//!   MakeNoise and NotifyNearlyShot: HeardEnemy), its buddies' deaths (the death
//!   set's 'Death' noise: Alarm), the squad's and its buddies' broadcasts, and
//!   Escalation's alerter (TnSurvivalGoalModifyAIBehavior's DoAlert: every 5 s, and
//!   as each comes in, it is sent at the first player, TnAIAlerter.Alert);
//! * its controller's tick (its sensors, its plans and their operators, its weapon
//!   controller and its motion controller) and its pawn's (its physics, its
//!   animations, its gun, its melee);
//! * what it did to the world: its shots (traced at the players and the level,
//!   their damage falling off with range), its blows, its grenades, its dialogue.
//!
//! The squad (TnAiSquadManager) and the tactical system (TnTacticalSystem and its
//! updater) are every AI's, and update once a frame.

// The port keeps the game's script whole (its enums' values, its helpers and
// fields), some of it for the enemy types still to come: the compiler's
// unused-code warnings are off for it.
#![allow(dead_code)]

pub mod aimoffset;
pub mod anim;
pub mod audio;
pub mod blueprint;
pub mod brain;
pub mod classes;
pub mod clips;
pub mod data;
pub mod drones;
pub mod forms;
pub mod graph;
pub mod grenade;
pub mod guard;
pub mod hits;
pub mod htn;
pub mod jet;
pub mod laser;
pub mod leaper;
pub mod melee;
pub mod motion;
pub mod moveto;
pub mod ops;
pub mod path;
pub mod pawn;
pub mod projectile;
pub mod scanner;
pub mod sensors;
pub mod shared;
pub mod sniper;
pub mod sync;
pub mod tactical;
pub mod tree;
pub mod ue;
pub mod weapon;
pub mod world;

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

use self::blueprint::Blueprint;
use self::brain::{Brain, Ctx, DamageInfo, DamageKind, Event, EventId};
use self::clips::{ClipNotify, Clips};
use self::data::{AiData, Behaviour};
use self::pawn::Pawn;
use self::shared::{MemberInfo, Shared, WsWrite};
use self::weapon::GunData;
use self::world::{AiView, PlayerView, WorldView};
use crate::enemies::{Death, Enemy, Foe, Kinds};
use crate::nav::{Nav, NavMesh};

/// TnSurvivalGoalModifyAIBehavior's AlertFreq: the AIs are sent at the first player
/// this often.
const ALERT_SECS: f32 = 5.0;
/// TnDeathActionMakeNoise's radii in the soldiers' death set (Basic_DEATHSET).
const DEATH_NOISE: (f32, f32) = (3500.0, 3000.0);
/// The most tactical nodes TnTacticalUpdater refreshes a frame (its 1.5 ms).
const TACTICAL_UPDATES: usize = 24;
/// A frame's longest step (UE3's MaxDeltaTime).
const MAX_DT: f32 = 0.1;
/// A held loop's fade once it is let go (a gun's: the sound system's own).
const LOOP_FADE: f32 = 0.1;

/// An enemy's AI: its controller, its pawn and its anim tree's clips.
#[derive(Component)]
pub struct Ai {
    pub brain: Brain,
    pub pawn: Pawn,
    pub clips: Arc<Clips>,
}

/// An enemy whose type has no AI here (it stands as it came in).
#[derive(Component)]
pub struct NoAi;

/// The AI's data and what every AI shares.
#[derive(Resource, Default)]
pub struct AiWorld {
    pub data: Arc<AiData>,
    /// The types' sound sets and the dialog rules.
    pub sounds: Arc<audio::AiSounds>,
    /// The projectiles the animations spawn, by class (their data, read once).
    notify_projectiles: Arc<HashMap<String, Arc<weapon::ProjectileData>>>,
    loaded: bool,
    blueprints: HashMap<String, Arc<Blueprint>>,
    guns: HashMap<String, Arc<GunData>>,
    pub shared: Shared,
    /// The alerter's countdown (TnSurvivalGoalModifyAIBehavior._AlertFreqCountdown).
    alert_left: f32,
    /// The navigation the tactical system's states were made for (its points).
    tactical_for: usize,
    /// Each player's last place on the navigation mesh (LastNavigableNearbyLocation).
    player_nav: HashMap<Entity, (Vec3, f32)>,
}

impl AiWorld {
    /// The type's resolved references (built once).
    fn blueprint(&mut self, b: &Behaviour) -> Arc<Blueprint> {
        self.blueprints
            .entry(b.id.clone())
            .or_insert_with(|| Arc::new(Blueprint::new(b)))
            .clone()
    }

    /// The type's gun (the one it starts with: its blueprint's first).
    fn gun(&mut self, b: &Behaviour) -> Arc<GunData> {
        let bp = self.blueprint(b);
        self.guns
            .entry(b.id.clone())
            .or_insert_with(|| bp.guns[0].clone())
            .clone()
    }
}

/// A new match: nobody in the squad, every tactical node as the map starts.
fn reset(mut world: ResMut<AiWorld>, nav: Res<Nav>) {
    world.shared = Shared::default();
    world.shared.notify_projectiles = world.notify_projectiles.clone();
    world.shared.tactical.reset(nav.mesh.as_ref());
    world.tactical_for = nav.mesh.as_ref().map_or(0, |m| m.points.len());
    world.alert_left = 0.0;
    world.player_nav.clear();
}

/// The AI's data, once.
fn load(mut world: ResMut<AiWorld>) {
    if world.loaded {
        return;
    }
    world.loaded = true;
    let dir = crate::assets_dir();
    world.data = Arc::new(AiData::load(&dir));
    world.sounds = Arc::new(audio::AiSounds::load(&dir));
    world.notify_projectiles = Arc::new(
        world
            .data
            .projectiles
            .iter()
            .filter_map(|(class, b)| {
                weapon::ProjectileData::from_keys(b, "").map(|d| (class.clone(), Arc::new(d)))
            })
            .collect(),
    );
    world.shared.notify_projectiles = world.notify_projectiles.clone();
}

// ---------------------------------------------------------------------------
// An enemy's controller, as it is set up
// ---------------------------------------------------------------------------

/// A Swarmer's own coming in (Swarmer_ALT_ANIM's): up out of the ground.
const GROUND_SPAWN: &str = "Nav_Spawn_Ground";

/// The animation an enemy comes in with: its spawn point's, if its clips have it.
/// A point that asks for one its clips lack plays its own climb out of the ground
/// instead, if it has one: Ancients' alcove by the AoE Slow station
/// (TnSpawnClusterSurvival_9813) asks for CINE_ReadytoRide_s01_Swarmer, a campaign
/// cinematic's, in no animation set the Swarmers have (nor any Escalation
/// loads), and the Swarmers there came in standing.
pub fn spawn_animation<'a>(clips: &Clips, asked: Option<&'a str>) -> Option<&'a str> {
    let asked = asked?;
    if clips.find(asked).is_some() {
        return Some(asked);
    }
    clips.find(GROUND_SPAWN).map(|_| GROUND_SPAWN)
}

/// The enemies that have come in (and come down: those the waves fly or leap in
/// are given theirs as they land) get their controllers.
#[allow(clippy::type_complexity)]
fn attach(
    mut commands: Commands,
    mut world: ResMut<AiWorld>,
    kinds: Res<Kinds>,
    (nav, level, net): (Res<Nav>, Res<crate::Level>, Res<crate::net::Net>),
    foes: Query<
        (
            Entity,
            &Foe,
            &Transform,
            &Enemy,
            Option<&crate::waves::WaveFoe>,
            Option<&drones::DroneLaunch>,
        ),
        (Without<Ai>, Without<NoAi>),
    >,
    players: Query<(Entity, &crate::Player, &Transform), Without<Foe>>,
) {
    if !world.loaded {
        return;
    }
    let world = &mut *world;
    if nav.mesh.as_ref().map_or(0, |m| m.points.len()) != world.tactical_for {
        world.shared.tactical.reset(nav.mesh.as_ref());
        world.tactical_for = nav.mesh.as_ref().map_or(0, |m| m.points.len());
    }
    let view = WorldView {
        now: world.shared.now,
        nav: nav.mesh.as_ref(),
        players: players
            .iter()
            .map(|(e, p, t)| {
                let none = HitBody::default();
                player_view(
                    e,
                    p,
                    t,
                    None,
                    None,
                    &mut HashMap::new(),
                    world.shared.now,
                    none,
                )
            })
            .collect(),
        ais: Vec::new(),
        level: Some(&*level),
        barriers: Vec::new(),
    };
    for (entity, foe, transform, enemy, wave, launch) in &foes {
        if !enemy.alive() || wave.is_some_and(|w| !w.landed()) {
            continue;
        }
        // (A drone that gets no AI is its Titan's all the same: none came.)
        if let Some(l) = launch {
            commands.entity(entity).remove::<drones::DroneLaunch>();
            world.shared.drones_spawned.push((l.owner, Some(entity)));
        }
        let Some(kind) = kinds.kinds.get(foe.kind) else {
            commands.entity(entity).insert(NoAi);
            continue;
        };
        let behaviour = world.data.behaviour(&kind.id).filter(|b| {
            classes::Class::of(&b.class).is_some_and(|c| c.runs()) && b.domain.is_some()
        });
        let Some(b) = behaviour else {
            commands.entity(entity).insert(NoAi);
            continue;
        };
        // (Its model stands on its feet; a drone's is its middle.)
        let mut feet = ue::to_ue(transform.translation);
        if kind.form == crate::enemies::Form::Orb {
            feet.z -= kind.half_height * ue::UU_PER_M;
        }
        let heading = transform.rotation.to_euler(EulerRot::YXZ).0;
        let body = Body {
            feet,
            yaw: ue::yaw_of_heading(heading),
            radius: kind.radius * ue::UU_PER_M,
            half_height: kind.half_height * ue::UU_PER_M,
            health: enemy.health,
            mesh_scale: kind.scale,
            forms: forms_of(&kinds, kind),
        };
        let seed = (foe.seed as u64) << 1 | 1;
        let Some(mut ai) = new_ai(world, &view, entity, foe.kind, b, body, seed) else {
            commands.entity(entity).insert(NoAi);
            continue;
        };
        // A drone flung out of its Titan (TnSpawnDronesOperator.OnSpawnComplete).
        if let Some(l) = launch {
            let Ai { brain, pawn, .. } = &mut ai;
            drones::launch(brain, pawn, &view, l);
        }
        sniper_came_in(&mut ai, &view, transform.translation);
        // Its spawn point's animation (its SpawnAnimationName: Ancients' Swarmers
        // come up out of the ground; spawn_animation): held where it plays, its
        // controller asleep, till it is over (TnHtnAiPawn.StartSpawnAnimation).
        if let Some(anim) = spawn_animation(&ai.clips, wave.and_then(|w| w.spawn_anim.as_deref())) {
            ai.pawn.start_spawn_animation(anim);
            ai.brain.core.set_ws(htn::WS_ASLEEP, 1);
        }
        commands.entity(entity).insert(ai);
        // (A client's copy of the host's: its dialog and alerts are the host's.)
        if sync::following(&net) {
            continue;
        }
        // TnPawn.InitializeState's incidental dialog: its own as it comes in, and
        // its close teammates' word of it.
        world
            .shared
            .dialog
            .push(audio::DialogRequest::say(entity, audio::SPAWNED));
        world.shared.dialog.push(audio::DialogRequest::teammates(
            entity,
            feet,
            audio::TEAMMATE_SPAWNED,
        ));
        // (TnSurvivalDataStoreModifyAIBehavior.NewAI: sent at the players at once.)
        world.alert_left = world.alert_left.min(0.0);
    }
}

/// How near its air spawn point a Sniper comes in (m).
const SPAWN_POINT_REACH: f32 = 3.0;

/// A Sniper comes in flying at its air spawn point (an AirPathNode on the air
/// network: its SpawnPoint, where its first flight starts), `at` in the demo's
/// space.
fn sniper_came_in(ai: &mut Ai, view: &WorldView, at: Vec3) {
    let Some(store) = ai.brain.sensors.sniper_store.as_mut() else {
        return;
    };
    store.spawn_point = view
        .nav
        .and_then(|n| n.nearest_point(at, |p| p.kind == crate::nav::PointKind::Air))
        .filter(|&p| {
            view.nav
                .is_some_and(|n| n.points[p as usize].at.distance(at) < SPAWN_POINT_REACH)
        });
    ai.pawn.physics = pawn::Physics::Flying;
}

/// Where a new AI's pawn stands and what it is: its feet, its facing, its
/// collision cylinder and its health (Unreal's units).
#[derive(Clone, Copy, Debug, Default)]
pub struct Body {
    pub feet: Vec3,
    pub yaw: f32,
    pub radius: f32,
    pub half_height: f32,
    pub health: f32,
    /// Its mesh's scale (the root motion's).
    pub mesh_scale: f32,
    /// A transforming type's forms (its type's and its other's: forms.rs).
    pub forms: Option<forms::Setup>,
}

/// A transforming type's forms (its type's own and its other form's: Kind.other),
/// if it changes form.
pub fn forms_of(kinds: &Kinds, kind: &crate::enemies::Kind) -> Option<forms::Setup> {
    let other = kinds.kinds.get(kind.other?)?;
    let start = if kind.form == crate::enemies::Form::Robot {
        forms::ROBOT
    } else {
        forms::ALT
    };
    let cap = |k: &crate::enemies::Kind| forms::FormCaps {
        radius: k.radius * ue::UU_PER_M,
        half_height: k.half_height * ue::UU_PER_M,
    };
    let mut caps = [cap(kind), cap(other)];
    if start == forms::ALT {
        caps.swap(0, 1);
    }
    Some(forms::Setup { start, caps })
}

/// A new enemy's AI: its controller and its pawn, set up as
/// TnHtnAiController.Initialize does.
pub fn new_ai(
    world: &mut AiWorld,
    view: &WorldView,
    entity: Entity,
    kind: usize,
    b: Arc<Behaviour>,
    body: Body,
    seed: u64,
) -> Option<Ai> {
    let clips = clips::for_type(&b.id)?;
    let bp = world.blueprint(&b);
    let gun = world.gun(&b);
    // Its sound sets, and the dialog character it speaks as.
    let voice = world
        .sounds
        .voice(&b.id, &mut ue::Rng::new(seed ^ 0xD1A1_06E5));
    let brain = Brain::new(entity, kind, b, bp, seed)?;
    let mut pawn = Pawn::new(
        entity,
        body.feet,
        body.yaw,
        body.radius,
        body.half_height,
        body.health,
    );
    pawn.mesh_scale = body.mesh_scale;
    pawn.voice = voice;
    // (The custom slots its anim tree has.)
    pawn.anim.set_slots(clips.slots);
    // A transforming type's forms, its other form's clips the set its other's.
    if let Some(setup) = body.forms
        && let Some(other) = clips::for_type(&format!("{}~other", brain.core.b.id))
    {
        pawn.forms = Some(Box::new(forms::Forms::new(setup.start, setup.caps, other)));
    }
    let mut ai = Ai { brain, pawn, clips };
    initialize(&mut ai, &mut world.shared, view, gun);
    Some(ai)
}

/// TnHtnAiController.Initialize: its blueprint's planning domain, its squad, its
/// properties (ApplyCharacterProperties), its sensors, its planner, its motion,
/// and no behaviour yet.
fn initialize(ai: &mut Ai, shared: &mut Shared, view: &WorldView, gun: Arc<GunData>) {
    let Ai { brain, pawn, .. } = ai;
    let b = brain.core.b.clone();
    // InitializeWeapons: no IK tracking yet.
    pawn.ik_head = false;
    pawn.ik_weapon = false;
    // InitializeSquadManager.
    let member = MemberInfo {
        entity: brain.core.entity,
        ai_type: brain.core.ai_type,
    };
    let mut writes = Vec::new();
    shared.squad.add_member(member, shared.now, &mut writes);
    // Its blueprint's OnApplyCharacterProperties.
    let class = classes::Class::of(&b.class).unwrap_or(classes::Class::Soldier);
    classes::apply_properties(class, brain, pawn, shared, gun);
    // PopulateSensorCollection, then InitializePlanner (a first update of the
    // sensors, with no time passed) and SetBehavior(0, true).
    let mut ctx = Ctx {
        dt: 0.0,
        pawn,
        view,
        shared,
    };
    apply_writes(brain, brain.core.entity, &mut writes);
    classes::populate(class, brain, &mut ctx);
    sensors::update(brain, &mut ctx);
    sensors::update_prior_to_planning(brain, &mut ctx);
    brain.set_behavior(&mut ctx, 0, true);
    let mut writes = std::mem::take(&mut ctx.shared.writes);
    apply_writes(brain, brain.core.entity, &mut writes);
}

/// The squad's world state writes on `e`, applied to its brain.
fn apply_writes(brain: &mut Brain, e: Entity, writes: &mut Vec<(Entity, WsWrite)>) {
    writes.retain(|(target, w)| {
        if *target != e {
            return true;
        }
        match *w {
            WsWrite::Int(attr, v) => brain.core.set_ws(attr, v),
            WsWrite::Bit(attr, bit, on) => brain.core.set_ws_bit(attr, bit, on),
        }
        false
    });
}

// ---------------------------------------------------------------------------
// The world as the AI sees it
// ---------------------------------------------------------------------------

/// The demo's player height its eyes are at (waves.rs's EYES).
const EYES: f32 = 0.9;

/// How far down from a player in the air FindNavMeshLocation traces for the
/// ground (metres): Default__TnTransformer's NavigableLocationCheckDistance,
/// 2500 uu.
const NAVIGABLE_CHECK_M: f32 = 25.0;

/// How far above the enemies' floor a player's floor may be and still be theirs
/// (metres). A floor only the players stand on (the lid over Oblivion's ramp out
/// of the enemies' sunken spawn area, an HmPlayerOnlyBlockingVolume the enemies
/// pass through) isn't.
const OWN_FLOOR: f32 = 0.3;

/// How far under a player's floor the cell under him may lie and still be his
/// (metres). The cells lie about 0.6 m over the floors they cover; one further
/// down is a lower floor's (a corridor under a walkway), not his.
const CELL_UNDER_FLOOR: f32 = 0.5;

/// TnPawn.FindNavMeshLocation: the place on the navigation mesh where a player is,
/// if he is on it (Bevy's axes, the floor under his feet). That is his feet, or,
/// in the air (jumping, falling, a jet flying), the ground straight under him
/// within NavigableLocationCheckDistance (the game's trace), over an open cell of
/// his own floor (IsOnNavMesh), a floor the enemies stand on too.
fn navigable_location(
    nav: &NavMesh,
    level: Option<&crate::Level>,
    at: Vec3,
    radius: f32,
    trace: bool,
) -> Option<Vec3> {
    use crate::collision::Mover;
    let floor = match level {
        Some(l) if trace => l.ground_for(
            at,
            radius,
            at.y + 0.1,
            at.y - NAVIGABLE_CHECK_M,
            Mover::Player,
        )?,
        _ => at.y,
    };
    let feet = Vec3::new(at.x, floor, at.z);
    // (The enemies' floor there, which they stand on: a players-only lid isn't.
    // Its cell is the ramp under it: taken as his, it sent them to stand on the
    // ramp beneath him, under the floor and out of his reach.)
    if let Some(l) = level {
        let theirs = l.ground_for(feet, radius, floor + OWN_FLOOR, floor - 3.0, Mover::Ai);
        if theirs.is_none_or(|g| floor - g > OWN_FLOOR) {
            return None;
        }
    }
    let cell = nav.cell_at(feet)?;
    let own = nav.height_at(cell, feet.x, feet.z) >= floor - CELL_UNDER_FLOOR;
    (own && !nav.points[cell as usize].blocked).then_some(feet)
}

/// A player's pawn as the AI reads it (`body`: his body where his bones are this
/// frame, hit_body).
#[allow(clippy::too_many_arguments)]
fn player_view(
    e: Entity,
    p: &crate::Player,
    t: &Transform,
    nav: Option<&NavMesh>,
    level: Option<&crate::Level>,
    near: &mut HashMap<Entity, (Vec3, f32)>,
    now: f32,
    body: HitBody,
) -> PlayerView {
    let (radius, height) = p.size();
    // (On the ground, where a pawn's feet stand: over it by UE3's floor distance.)
    let feet = ue::to_ue(t.translation) + Vec3::Z * if p.grounded { pawn::FLOOR_DIST } else { 0.0 };
    let half = height * 0.5 * ue::UU_PER_M;
    let robot = p.form == crate::Form::Robot;
    let jet = !robot && p.kit.plane.is_some();
    // LastNavigableNearbyLocation (TnPawn.UpdateNavigableNearbyLocation, each
    // frame): where he is on the mesh (FindNavMeshLocation), held while he is off
    // it (on cover or a crate, a ledge, a blocked cell, a jet high in the air), as
    // the game holds it.
    if let Some(n) = nav
        && let Some(at) = navigable_location(n, level, t.translation, radius, !p.grounded || jet)
    {
        near.insert(e, (ue::to_ue(at) + Vec3::Z * pawn::FLOOR_DIST, now));
    }
    let up = crate::revive::up(p);
    let cloaked = p.ability.kind == crate::abilities::Kind::Cloak && p.ability.active;
    PlayerView {
        entity: e,
        loc: feet + Vec3::Z * half,
        feet,
        vel: ue::dir_to_ue(p.velocity + Vec3::Y * p.vertical_velocity) * ue::UU_PER_M,
        yaw: ue::yaw_of_heading(p.heading),
        radius: radius * ue::UU_PER_M,
        half_height: half,
        health: p.health + p.shield,
        health_max: p.max_health() + crate::PLAYER_MAX_SHIELD,
        alive: up,
        downed: p.revive.downed.is_some(),
        cloaked,
        form: if robot {
            1
        } else if jet {
            2
        } else {
            3
        },
        airborne: !p.grounded,
        jumping: p.jump_time.is_some(),
        physics: if jet {
            4
        } else if p.grounded {
            1
        } else {
            2
        },
        transforming: p.transform_left > 0.0,
        shielded: p.ability.kind == crate::abilities::Kind::Shield && p.ability.active,
        // (His robot's GetViewLocation: its view bone where it is, but cloaked;
        // else, and in his vehicle, high on his cylinder.)
        eyes: match body.view {
            Some(at) if !cloaked => ue::to_ue(at),
            _ => feet + Vec3::Z * (height * EYES * ue::UU_PER_M),
        },
        nav_near: near.get(&e).map_or(feet, |n| n.0),
        nav_near_time: near.get(&e).map_or(f32::MIN, |n| n.1),
        team: world::PLAYER_TEAM,
        // (TnEnemyStatusSensor: get_CanUseAbilities false, down or reviving or
        // jammed.)
        weakened: p.revive.locked() || crate::abilities::jammed(p),
        dodge: (p.dodge && p.dash_left > 0.0)
            .then(|| ue::dir_to_ue(p.dash_vel).normalize_or_zero()),
        body: body.shapes,
    }
}

/// TnPawn's _ViewBoneName (Default__TnPawn's, every player's robot's): the bone
/// its GetViewLocation is, which the enemies aim at (TnAiTargeter).
const VIEW_BONE: &str = "C_Spine04_Head_XB";

/// A player's body as the enemies find it this frame (the demo's space): what their
/// instant hits strike (PlayerView::body) and, in his robot, where they aim (his
/// view bone).
#[derive(Default)]
struct HitBody {
    shapes: Option<Arc<[crate::ragdoll::Shape]>>,
    view: Option<Vec3>,
}

/// A player's HitBody: in his robot, its physics asset's shapes (RB_Shared_PHYS,
/// downed.rs's: his robot's rig) where his bones are, and his view bone; in his
/// vehicle, its box (its hull's, or its body's reach and width, as tall as his
/// cylinder) turned his way. Nothing for a robot whose rig isn't in yet.
fn hit_body(
    p: &crate::Player,
    t: &Transform,
    rig: Option<&crate::ragdoll::Rig>,
    globals: &Query<&GlobalTransform>,
) -> HitBody {
    match p.form {
        crate::Form::Robot => {
            let Some(rig) = rig else {
                return HitBody::default();
            };
            let bone = |name: &str| {
                rig.names
                    .iter()
                    .position(|n| n == name)
                    .and_then(|i| globals.get(rig.bones[i]).ok())
            };
            let mut shapes = Vec::new();
            for body in &rig.asset.bodies {
                if let Some(g) = bone(&body.bone) {
                    let m = g.affine();
                    shapes.extend(body.shapes.iter().map(|s| s.placed(&m)));
                }
            }
            HitBody {
                shapes: (!shapes.is_empty()).then(|| Arc::from(shapes)),
                view: bone(VIEW_BONE).map(|g| g.translation()),
            }
        }
        crate::Form::Vehicle => {
            let (_, height) = p.size();
            let (ahead, behind, half_width) = match p.kit.hull {
                Some(h) => (h.ahead, h.behind, h.half_width),
                None => (p.kit.body.ahead, p.kit.body.behind, p.kit.body.half_width),
            };
            let turn = Quat::from_rotation_y(p.heading);
            let forward = turn * Vec3::NEG_Z;
            HitBody {
                shapes: Some(Arc::from([crate::ragdoll::Shape::Box {
                    c: p.centre(t.translation) + forward * ((ahead - behind) * 0.5),
                    axes: [turn * Vec3::X, Vec3::Y, forward],
                    half: Vec3::new(half_width, height * 0.5, (ahead + behind) * 0.5),
                }])),
                view: None,
            }
        }
    }
}

/// Where an instant hit's trace (from `o` along unit `dir`, Unreal space) strikes a
/// player (uu along it): his body's shapes, or his cylinder where they aren't known.
fn player_trace(pl: &PlayerView, o: Vec3, dir: Vec3) -> Option<f32> {
    match &pl.body {
        Some(shapes) => {
            let (o, dir) = (ue::from_ue(o), ue::dir_from_ue(dir));
            shapes
                .iter()
                .filter_map(|s| s.ray(o, dir, f32::MAX))
                .reduce(f32::min)
                .map(|t| t * ue::UU_PER_M)
        }
        None => ray_cylinder(o, dir, pl.loc, pl.radius, pl.half_height),
    }
}

/// An AI pawn as the others see it.
fn ai_view(ai: &Ai, alive: bool) -> AiView {
    let p = &ai.pawn;
    AiView {
        entity: p.entity,
        loc: p.loc,
        vel: p.vel,
        yaw: p.yaw,
        radius: p.radius,
        half_height: p.half_height,
        alive,
        team: ai.brain.core.team,
        enemy: ai.brain.core.enemy,
        kind: ai.brain.core.kind,
        blocks: p.block_actors,
        nav_near: p.feet(),
        raises_suspicion: guard_raises_suspicion(&ai.brain),
        defend: ai.brain.sensors.defend_actor,
    }
}

/// TnGuardSensor.ShouldRaiseSuspicion: a Guard (AICT_Guard) in battle form, aware
/// of the enemy (WS_AlertToEnemyPresence), its search and unreachable suspected
/// enemy counters (the Guard's WS 24 and 23) clear.
fn guard_raises_suspicion(b: &Brain) -> bool {
    let c = &b.core;
    c.ai_type == shared::AICT_GUARD
        && c.ws(htn::WS_CURRENT_FORM) == guard::FORM_BATTLE
        && c.ws_bool(htn::WS_ALERT_TO_ENEMY_PRESENCE)
        && c.ws(htn::WS_TIRED) == 0
        && c.ws(htn::WS_FRUSTRATED) == 0
}

/// A player weapon's noise (its data's SoundRadius and OccludedSoundRadius,
/// HmWeapon.FireAmmunition's MakeNoise) and how near its rounds pass a pawn to
/// make it flinch (NearlyShotDistance: NotifyPawnNearlyShot), by the demo's
/// weapon ids (the data Escalation gives each).
fn weapon_noise(id: &str) -> (f32, f32, f32) {
    match id {
        // AssaultRifleArm_SV_WEPDATA.
        "AssaultRifle" => (30000.0, 20000.0, 800.0),
        // IonBlaster_SP_WEPDATA.
        "IonBlaster" => (15000.0, 10000.0, 1000.0),
        "BurstRifle" | "HeavyPistol" | "Bazooka" | "DrainGun" | "AoESlow" => (6000.0, 0.0, 1000.0),
        "Shotgun" | "SniperRifle" => (15000.0, 10000.0, 1000.0),
        "HeavyMG" => (6000.0, 0.0, 1200.0),
        "ChainBeam" => (0.0, 0.0, 1000.0),
        "SawBlade" => (30000.0, 20000.0, 800.0),
        "HomingRocket" | "GrenadeLauncher" | "ClusterCannon" => (10000.0, 0.0, 1000.0),
        // TracerDart_MP_WEPDATA: none.
        _ => (0.0, 0.0, 0.0),
    }
}

/// A noise a pawn made this frame (MakeNoise): who, where, its radii, its name.
pub struct Noise {
    maker: Entity,
    at: Vec3,
    radius: f32,
    occluded: f32,
    name: &'static str,
}

/// A round's path past the pawns (NotifyPawnNearlyShot): its shooter, from where
/// to where, and how near it must pass.
pub struct ShotPath {
    shooter: Entity,
    from: Vec3,
    to: Vec3,
    near: f32,
}

/// What the players did this frame that the AI hears: their weapons' noises and
/// their rounds' paths.
fn player_noises(
    players: &[(Entity, &crate::Player, Vec3)],
    noises: &mut Vec<Noise>,
    paths: &mut Vec<ShotPath>,
) {
    for (e, p, feet) in players {
        let at = ue::to_ue(*feet) + Vec3::Z * p.size().1 * 50.0;
        for ev in &p.played {
            let weapon = match ev {
                crate::net::JazzEvent::Shot { weapon, .. }
                | crate::net::JazzEvent::Round { weapon, .. }
                | crate::net::JazzEvent::HomingRound { weapon, .. }
                | crate::net::JazzEvent::Lob { weapon, .. } => *weapon,
                _ => continue,
            };
            let id = crate::WEAPONS.get(weapon as usize).map_or("", |w| w.id);
            let (radius, occluded, near) = weapon_noise(id);
            if radius > 0.0 || occluded > 0.0 {
                noises.push(Noise {
                    maker: *e,
                    at,
                    radius,
                    occluded,
                    name: "FireAmmunition",
                });
            }
            if let crate::net::JazzEvent::Shot {
                origin,
                dir,
                distance,
                ..
            } = ev
                && near > 0.0
            {
                let from = ue::to_ue(*origin);
                paths.push(ShotPath {
                    shooter: *e,
                    from,
                    to: from + ue::dir_to_ue(*dir) * distance * ue::UU_PER_M,
                    near,
                });
            }
        }
    }
}

/// UE3's noise hearing (APawn::CheckNoiseHearing with FoC's two radii): within its
/// radius in the open, within its occluded radius through the level.
fn hears(view: &WorldView, p: &Pawn, n: &Noise) -> bool {
    let d = (n.at - p.loc).length();
    if d <= n.occluded {
        return true;
    }
    d <= n.radius && view.fast_trace(n.at, p.eyes())
}

/// The damage type the AI's script tells a blow apart by.
fn damage_kind(death: Death, blow: crate::xp::Blow) -> DamageKind {
    if matches!(blow, crate::xp::Blow::Melee) {
        return DamageKind::Melee;
    }
    match death {
        Death::Melee => DamageKind::Melee,
        Death::Tank => DamageKind::VehicleExplosive,
        Death::Explosive => DamageKind::Explosive,
        Death::Fire => DamageKind::Fire,
        _ => DamageKind::Other,
    }
}

// ---------------------------------------------------------------------------
// The frame
// ---------------------------------------------------------------------------

/// What the AI did this frame for the world to show (the players' damage is
/// theirs to take: `effects`).
#[derive(Resource, Default)]
pub struct AiOutputs {
    pub shots: Vec<shared::Shot>,
    pub player_damage: Vec<shared::PlayerDamage>,
    /// The damage their rounds did to the players' barriers (equipment.rs).
    pub barrier_damage: Vec<(Entity, f32)>,
    pub throws: Vec<shared::Throw>,
    pub launches: Vec<shared::Launch>,
    pub sounds: Vec<(Entity, &'static str, Vec3)>,
    /// Looping sounds held this frame (a gun's while it fires, the Guard's
    /// scanner's), each with the time it fades out over once let go.
    pub loops: Vec<(Entity, &'static str, Vec3, f32)>,
    /// The lines of dialog said and cut short.
    pub says: Vec<audio::Say>,
    /// The players the Guards' scanners caught (exposed and jammed).
    pub scanned: Vec<guard::Scanned>,
    /// The effects they played on their sockets, and the held ones they stopped.
    pub effects: Vec<shared::Effect>,
    pub effects_stopped: Vec<(Entity, Arc<str>)>,
}

/// The players' grenades that came down near where they bounced this frame (where
/// they were, the demo's space): TnProjectileGrenadeBase.NotifyPawnsOfGrenade, as a
/// grenade first bounces to under 1.5 m/s across the ground (an EMP grenade, a
/// Chaos-Rift Combustor's cluster), each AI's NotifyNearGrenade.
#[derive(Resource, Default)]
pub struct NearGrenades(pub Vec<Vec3>);

/// Its 2D speed (m/s, the ground's plane) at or under which a bouncing grenade
/// warns those near it (NotifyPawnsOfGrenade: VSizeSq2D(Velocity) over 150 cm/s
/// squared returns).
pub const GRENADE_WARN_SPEED: f32 = 1.5;

/// TnHtnAiController.NotifyNearGrenade: a grenade of the other side within its
/// danger range across the ground (_GrenadeDangerRangeSq) puts it in danger
/// (WS_DangerClose's DC_Grenade bit: the domain's grenade reaction) and tells it
/// where (LR_GrenadeLocation). `at` in Unreal's space.
fn notify_near_grenade(brain: &mut Brain, pawn: &Pawn, at: Vec3) {
    let d = at - pawn.loc;
    if d.x * d.x + d.y * d.y > brain.core.grenade_danger_range_sq {
        return;
    }
    brain
        .core
        .set_ws_bit(htn::WS_DANGER_CLOSE, htn::DC_GRENADE, true);
    brain.core.store(brain::LR_GRENADE_LOCATION, at, Vec3::ZERO);
}

/// An AI in the frame: its entity, its AI, and whether an EMP holds it.
pub struct Slot<'a> {
    pub entity: Entity,
    pub ai: &'a mut Ai,
    pub dormant: bool,
}

/// Every AI's frame.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn think(
    time: Res<Time>,
    mut world: ResMut<AiWorld>,
    mut outputs: ResMut<AiOutputs>,
    (nav, level, mut near_grenades): (Res<Nav>, Res<crate::Level>, ResMut<NearGrenades>),
    players: Query<(Entity, &crate::Player, &Transform), (Without<Ai>, Without<Foe>)>,
    mut ais: Query<
        (
            Entity,
            &mut Ai,
            &mut Transform,
            &mut Foe,
            Option<&mut Enemy>,
            Option<&mut crate::enemies::Forms>,
        ),
        Without<crate::waves::Withdrawn>,
    >,
    mut withdrawn: Query<(Entity, &mut Ai), With<crate::waves::Withdrawn>>,
    barriers: Query<(Entity, &crate::equipment::Barrier)>,
    (robots, globals): (
        Query<(&crate::JazzOf, &crate::ragdoll::Rig), With<crate::RobotForm>>,
        Query<&GlobalTransform>,
    ),
    mut commands: Commands,
) {
    let dt = time.delta_secs().min(MAX_DT);
    if dt <= 0.0 {
        return;
    }
    let world = &mut *world;
    world.shared.now += dt;
    let now = world.shared.now;
    let nav_mesh = nav.mesh.as_ref();
    // The players, and what they did that the AI hears.
    let mut near = std::mem::take(&mut world.player_nav);
    let mut view = WorldView {
        now,
        players: players
            .iter()
            .map(|(e, p, t)| {
                let rig = robots.iter().find(|(j, _)| j.0 == e).map(|(_, r)| r);
                let body = hit_body(p, t, rig, &globals);
                player_view(e, p, t, nav_mesh, Some(&*level), &mut near, now, body)
            })
            .collect(),
        ais: Vec::new(),
        nav: nav_mesh,
        level: Some(&*level),
        barriers: crate::equipment::barrier_shapes(barriers.iter()),
    };
    world.player_nav = near;
    let player_list: Vec<(Entity, &crate::Player, Vec3)> = players
        .iter()
        .map(|(e, p, t)| (e, p, t.translation))
        .collect();
    let mut noises = Vec::new();
    let mut paths = Vec::new();
    player_noises(&player_list, &mut noises, &mut paths);

    // Those the waves took out of play (not killed: waves.rs's Withdrawn): let go as
    // the dead are (out of their squad and the battle line, their plans and claims
    // given up), without a death's noise, and gone.
    for (entity, mut ai) in &mut withdrawn {
        pawn_died(entity, &mut ai, &mut world.shared, &view);
        commands.entity(entity).despawn();
    }
    // The dead first (PawnDied).
    let mut list: Vec<_> = ais.iter_mut().collect();
    let mut dead = Vec::new();
    // (Their voices, for the death cries: their AIs go.)
    let mut dead_voices = Vec::new();
    for (entity, ai, _, _, enemy, model_forms) in list.iter_mut() {
        let alive = enemy.as_ref().is_some_and(|e| e.alive());
        if alive && !ai.pawn.dying {
            continue;
        }
        let first = !ai.pawn.dying;
        if let Some(n) = pawn_died(*entity, ai, &mut world.shared, &view) {
            noises.push(n);
        }
        // (Its models as it dies: the form it was in, or taking.)
        if let Some(mf) = model_forms.as_mut() {
            show_forms(&ai.pawn, mf);
        }
        // TnPawn.RobotDying.BeginState's incidental dialog: its death cries (none
        // for a headshot) and its close teammates' word of its death.
        if first && !enemy.as_ref().is_some_and(|e| e.headshot()) {
            let d = &mut world.shared.dialog;
            d.push(audio::DialogRequest::say(*entity, audio::DEATH_VERBAL));
            d.push(audio::DialogRequest::say(*entity, audio::DEATH_NON_VERBAL));
            d.push(audio::DialogRequest::teammates(
                *entity,
                ai.pawn.loc,
                audio::TEAMMATE_KILLED,
            ));
        }
        dead_voices.push((*entity, std::mem::take(&mut ai.pawn.voice), ai.pawn.loc));
        dead.push(*entity);
        commands.entity(*entity).remove::<Ai>();
    }
    list.retain(|(e, ..)| !dead.contains(e));
    // The blows each took since its last frame.
    for (_, ai, _, _, enemy, _) in list.iter_mut() {
        if let Some(enemy) = enemy.as_mut() {
            ai.pawn.health = enemy.health;
            // (Each with the bone a round's trace struck, if it struck one.)
            let mut hits = std::mem::take(&mut enemy.hits);
            for h in hits.drain(..) {
                let bone = h.part.and_then(|p| enemy.body.bone(p));
                took_hit(ai, &h, bone);
            }
            enemy.hits = hits;
        }
    }
    // The players' grenades that came down near them.
    for at in near_grenades.0.drain(..) {
        let at = ue::to_ue(at);
        for (_, ai, ..) in list.iter_mut() {
            let Ai { brain, pawn, .. } = &mut **ai;
            notify_near_grenade(brain, pawn, at);
        }
    }
    // Escalation's alerter: every AI sent at the first player in the match.
    world.alert_left -= dt;
    let alert_at = view
        .players
        .iter()
        .find(|p| p.alive)
        .map(|p| (p.entity, p.loc));
    let alerting = (world.alert_left <= 0.0).then_some(alert_at).flatten();
    if alerting.is_some() {
        world.alert_left = ALERT_SECS;
    }
    {
        let mut slots: Vec<Slot> = list
            .iter_mut()
            .map(|(e, ai, _, _, enemy, _)| Slot {
                entity: *e,
                dormant: enemy.as_ref().is_some_and(|x| x.emp_left > 0.0),
                ai: &mut **ai,
            })
            .collect();
        view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
        hear(&view, &mut slots, &noises, &paths);
        run_frame(
            &mut world.shared,
            &world.sounds,
            &mut view,
            &mut slots,
            dt,
            alerting,
        );
    }
    // Their dialog this frame, as the dialog manager lets them speak.
    {
        let shared = &mut world.shared;
        shared.dialog_manager.tick(dt);
        let requests = std::mem::take(&mut shared.dialog);
        if !requests.is_empty() {
            let mut speakers: Vec<audio::Speaker> = list
                .iter_mut()
                .map(|(e, ai, ..)| {
                    let at = ai.pawn.loc;
                    let alive = !ai.pawn.dying;
                    audio::Speaker {
                        entity: *e,
                        voice: &mut ai.pawn.voice,
                        at,
                        alive,
                    }
                })
                .collect();
            speakers.extend(dead_voices.iter_mut().map(|(e, voice, at)| audio::Speaker {
                entity: *e,
                voice,
                at: *at,
                alive: false,
            }));
            let mut rng = shared.rng.clone();
            audio::speak(
                &mut shared.dialog_manager,
                requests,
                &world.sounds,
                &mut speakers,
                now,
                &mut rng,
                &mut outputs.says,
            );
            shared.rng = rng;
        }
    }
    // The deaths their script dealt them (a drone's blast, its lifespan out, its
    // Titan dead).
    for (e, death) in std::mem::take(&mut world.shared.kills) {
        if let Some((_, _, _, _, Some(enemy), _)) = list.iter_mut().find(|(x, ..)| *x == e)
            && enemy.alive()
        {
            enemy.last_hit = crate::enemies::Hit::new(death, enemy.centre(), Vec3::ZERO);
            enemy.health = 0.0;
        }
    }
    // Where the models go, and what their pawns make of blows now (the Guard's
    // forms change it; its support drones' shield, nothing).
    for (e, ai, transform, foe, enemy, model_forms) in list.iter_mut() {
        place_models(&ai.pawn, transform, foe, model_forms.as_deref_mut());
        if let Some(enemy) = enemy.as_mut() {
            let shield = if drones::shielded(&world.shared, *e) {
                0.0
            } else {
                1.0
            };
            sync_blows(enemy, &ai.pawn, shield);
        }
    }
    // What they did, for the world.
    let shared = &mut world.shared;
    outputs.shots.append(&mut shared.shots);
    outputs.player_damage.append(&mut shared.player_damage);
    outputs.barrier_damage.append(&mut shared.barrier_damage);
    outputs.throws.append(&mut shared.throws);
    outputs.launches.append(&mut shared.launches);
    outputs.sounds.append(&mut shared.sounds);
    outputs.loops.append(&mut shared.loops);
    outputs.scanned.append(&mut shared.scanned);
    outputs.effects.append(&mut shared.effects);
    outputs.effects_stopped.append(&mut shared.effects_stopped);
}

/// Its models where its pawn is (its Foe's spot with them): a transforming pawn's
/// each on its form's cylinder, its type's own the enemy's, its other form's placed
/// on it (and each form's model hidden or shown, as the pawn's forms say).
fn place_models(
    p: &Pawn,
    transform: &mut Transform,
    foe: &mut Foe,
    model_forms: Option<&mut crate::enemies::Forms>,
) {
    let place = match (p.forms.as_ref(), model_forms) {
        (Some(f), Some(mf)) => {
            // (A jet's plane on its middle, its robot on its feet.)
            let vehicle = |form: u8| form == forms::ALT && p.plane.is_some();
            let (main, other) = (f.main, 1 - f.main);
            let own = model_place(p, f.caps[main as usize].half_height, vehicle(main));
            let theirs = model_place(p, f.caps[other as usize].half_height, vehicle(other));
            mf.local = Transform::from_matrix(own.to_matrix().inverse() * theirs.to_matrix());
            show_forms(p, mf);
            own
        }
        _ => model_place(p, p.half_height, true),
    };
    (transform.translation, transform.rotation) = (place.translation, place.rotation);
    foe.spot = transform.translation;
}

/// Where its model goes (the demo's space), on a cylinder of `half_height` (uu): a
/// plane form's (`vehicle`: a drone's, a jet's plane's) on its middle (its vehicle
/// form centres its mesh on its Location: CalculateCylinderBounds' mesh
/// translation, its bounds' middle), turned as its body is; the others' on their
/// feet, turned to its facing.
fn model_place(p: &Pawn, half_height: f32, vehicle: bool) -> Transform {
    match p.plane.as_ref().filter(|_| vehicle) {
        Some(f) => {
            let rotation = drones::body_rotation(f);
            let [x, y, z] = ue::rotator_axes(f.rotation.x, f.rotation.y, f.rotation.z);
            let c = f.mesh_centre;
            let origin = p.loc - (x * c.x + y * c.y + z * c.z);
            Transform::from_translation(ue::from_ue(origin)).with_rotation(rotation)
        }
        // (Turned as its Rotation is: a Sniper's insect on a wall stands on it.)
        None => {
            let k = std::f32::consts::TAU / 65536.0;
            let rotation = Quat::from_rotation_y(ue::heading(p.yaw))
                * Quat::from_rotation_x(p.pitch * k)
                * Quat::from_rotation_z(-p.roll * k);
            let up = ue::rotator_axes(p.pitch, p.yaw, p.roll)[2];
            Transform::from_translation(ue::from_ue(p.loc - up * half_height))
                .with_rotation(rotation)
        }
    }
}

/// A transforming pawn's forms for its models (enemies.rs's Forms, by model: its
/// type's own, its other form's): the one it is in the enemy's, each hidden or
/// shown.
fn show_forms(p: &Pawn, mf: &mut crate::enemies::Forms) {
    let Some(f) = p.forms.as_ref() else {
        return;
    };
    let main = f.main as usize;
    mf.want = usize::from(f.current != f.main);
    mf.hidden = [f.hidden[main], f.hidden[1 - main]];
}

/// What its pawn makes of blows, for its Enemy (enemies.rs's take): its damage
/// multiplier, its head's bones and its bones' damage, what its buffs make of
/// blows from behind and its facing to tell them by.
fn sync_blows(enemy: &mut Enemy, p: &Pawn, shield: f32) {
    // (TnPawn's AllDamageModifierSelf: TnBuffSupportDroneDamage's 0.)
    let scale = p.damage_multiplier * shield;
    if enemy.ai_damage_scale != scale {
        enemy.ai_damage_scale = scale;
    }
    enemy.from_behind = p.from_behind;
    enemy.facing = ue::dir_from_ue(p.forward());
    // (What its death is made of, should it die: TnDeathContextFactory's.)
    enemy.motion = crate::enemies::Motion {
        at: Some(ue::from_ue(p.loc)),
        velocity: ue::dir_from_ue(p.vel) / ue::UU_PER_M,
        mass: p.mass,
        airborne: p.physics == pawn::Physics::Falling,
        boosting: p.active_plane().map(|f| f.boosting),
        up: ue::dir_from_ue(ue::rotator_axes(p.pitch, p.yaw, p.roll)[2]),
    };
    if enemy.head_bones != p.head_bones {
        enemy.head_bones = p.head_bones.clone();
    }
    if enemy.bone_damage != p.bone_damage {
        enemy.bone_damage = p.bone_damage.clone();
    }
}

/// TnHtnAiController.PawnDied (as its pawn's health runs out): Died, its plan
/// aborted, its sensors shut down, gone from the squad and the battle line; its
/// death set's noise for its buddies (once).
pub fn pawn_died(
    entity: Entity,
    ai: &mut Ai,
    shared: &mut Shared,
    view: &WorldView,
) -> Option<Noise> {
    if ai.pawn.dying {
        return None;
    }
    ai.pawn.dying = true;
    // (A change of form cut short: it dies in the form it was taking.)
    forms::died(&mut ai.pawn);
    let Ai { brain, pawn, .. } = ai;
    let mut ctx = Ctx {
        dt: 0.0,
        pawn,
        view,
        shared,
    };
    brain.handle_event(&mut ctx, Event::about(EventId::Died, None));
    brain.abort_plan(&mut ctx, false);
    sensors::shutdown(brain, &mut ctx);
    let mut writes = std::mem::take(&mut shared.writes);
    let now = shared.now;
    shared.squad.remove_member(entity, now, &mut writes);
    shared.writes = writes;
    shared.tactical.remove_battle_line_pawn(entity);
    // (A support drone's shield goes with it: its aura's OnUnapplyOrOwnerDied.)
    drones::drone_died(shared, entity);
    Some(Noise {
        maker: entity,
        at: ai.pawn.loc,
        radius: DEATH_NOISE.0,
        occluded: DEATH_NOISE.1,
        name: "Death",
    })
}

/// HmPawn's JumpZ (default.JumpZ: the soldiers' pawn class keeps it).
const JUMP_Z: f32 = 1000.0;

/// Whether a blow of the kind moves it (TnPawn.ShouldIgnoreForces: TnPawn's
/// bIgnoreForces holds every force off but a damage type's that asks for its own,
/// RequestRespectForcesApplied: TnDamageTypeMelee's, TnDamageTypeStun's,
/// TnDamageTypeRammed's; a round's or a blast's moves it not at all).
fn respects_forces(kind: DamageKind) -> bool {
    matches!(
        kind,
        DamageKind::Melee | DamageKind::Stun | DamageKind::Rammed
    )
}

/// The push a blow that respects forces gives it while it lives (Pawn.TakeDamage's
/// AddVelocity of its momentum over its Mass, `v`): none while an animation's root
/// motion moves it (TnPawn.ShouldIgnoreForces: its mesh's RootMotionMode not
/// RMM_Ignore); off the ground if it would be in the air for more than 0.2 s; its
/// rise halved if it already rises faster than it jumps. On the ground its own
/// acceleration takes the push back off it (TnPawn's CalcVelocity) over the next
/// moments.
fn add_velocity(p: &mut Pawn, v: Vec3) {
    if v == Vec3::ZERO || p.anim.root_motion().is_some() {
        return;
    }
    let mut v = v;
    if p.physics == pawn::Physics::Walking && Pawn::estimate_air_time(v) > 0.2 {
        p.physics = pawn::Physics::Falling;
    }
    if p.vel.z > JUMP_Z && v.z > 0.0 {
        v.z *= 0.5;
    }
    p.vel += v;
}

/// A blow it took (TnHtnAiPawn.TakeDamage, then its controller's NotifyTakeHit:
/// TookDamage), `bone` the bone it struck if it names one (a round's trace).
pub fn took_hit(ai: &mut Ai, h: &crate::enemies::TakenHit, bone: Option<Arc<str>>) {
    // (A blow from the other side gives it away: ExposeSelf.)
    ai.pawn.cloak.expose();
    ai.pawn.last_damage = Some(damage_kind(h.hit.death, h.blow));
    // (Its momentum: the weapon's InstantHitMomentum, a blast's MomentumTransfer,
    // a melee blow's HitMomentum.)
    let mom = ue::dir_to_ue(h.hit.momentum) * ai.pawn.momentum_scale;
    // A blow that would throw it into the air for long enough knocks it back.
    let v = mom / ai.pawn.mass;
    if Pawn::estimate_air_time(v) > 0.5 {
        ai.pawn.knockback_velocity = v;
    }
    if respects_forces(damage_kind(h.hit.death, h.blow)) {
        add_velocity(&mut ai.pawn, v);
    }
    ai.brain.core.inbox.push(Event {
        id: EventId::TookDamage,
        actor: h.by,
        damage: Some(DamageInfo {
            instigator: h.by,
            hit_location: ue::to_ue(h.hit.at),
            damage: h.amount,
            kind: h.hit.death,
            damage_type: damage_kind(h.hit.death, h.blow),
            momentum: mom,
            bone,
        }),
        noise: None,
        notify: None,
    });
}

/// The frame's noises and near misses, to each AI that hears them (HearNoise: an
/// enemy's noise is heard of, a buddy's death is an alarm; NotifyNearlyShot).
pub fn hear(view: &WorldView, slots: &mut [Slot], noises: &[Noise], paths: &[ShotPath]) {
    for s in slots.iter_mut() {
        let ai = &mut *s.ai;
        for n in noises {
            if n.maker == s.entity || !hears(view, &ai.pawn, n) {
                continue;
            }
            let id = if !view.same_team(n.maker, s.entity) {
                EventId::HeardEnemy
            } else if n.name == "Death" {
                EventId::Alarm
            } else {
                continue;
            };
            ai.brain.core.inbox.push(Event {
                id,
                actor: Some(n.maker),
                damage: None,
                noise: Some(n.name),
                notify: None,
            });
        }
        for p in paths {
            let closest = closest_point_on_segment(p.from, p.to, ai.pawn.loc);
            if (ai.pawn.loc - closest).length() < p.near {
                ai.brain.core.inbox.push(Event {
                    id: EventId::HeardEnemy,
                    actor: Some(p.shooter),
                    damage: None,
                    noise: Some("FireAmmunition"),
                    notify: None,
                });
            }
        }
    }
}

/// Every AI's controller and pawn a frame on, then the squad and the tactical
/// system (`alert`: the alerter's target this frame, if it alerts).
pub fn run_frame(
    shared: &mut Shared,
    sounds: &audio::AiSounds,
    view: &mut WorldView,
    slots: &mut [Slot],
    dt: f32,
    alert_at: Option<(Entity, Vec3)>,
) {
    let now = shared.now;
    for i in 0..slots.len() {
        let entity = slots[i].entity;
        let dormant = slots[i].dormant;
        {
            let ai = &mut *slots[i].ai;
            let Ai {
                brain,
                pawn,
                clips: form_clips,
            } = ai;
            let mut ctx = Ctx {
                dt,
                pawn,
                view,
                shared,
            };
            if let Some((target, at)) = alert_at {
                alert(brain, &mut ctx, target, at);
            }
            brain.deliver_callbacks(&mut ctx);
            // Its drones come in (its spawner's: TnSpawnDronesOperator.OnSpawnComplete).
            if ctx.shared.drones_spawned.iter().any(|(o, _)| *o == entity) {
                let mut came = Vec::new();
                ctx.shared.drones_spawned.retain(|&(o, d)| {
                    if o == entity {
                        came.push(d);
                    }
                    o != entity
                });
                for d in came {
                    drones::deliver_spawned(brain, d);
                }
            }
            if dormant {
                // (SetDormant: it neither thinks nor fires while the EMP lasts.)
                ctx.pawn.weapon.stop_fire();
                ctx.pawn.accel = Vec3::ZERO;
            } else {
                brain.tick(&mut ctx);
            }
            // A support drone's aura (its buff's tick).
            if ctx.pawn.drone_aura {
                drones::tick_aura(brain, &mut ctx);
            }
            // The Guard's scanner (its component's tick), its loop held while it
            // is on.
            if ctx.pawn.guard.is_some() {
                guard::tick_scanner(brain, &mut ctx);
                let p = &*ctx.pawn;
                if let Some(g) = p.guard.as_ref()
                    && g.scanner.attached
                    && let Some((cue, _)) = p.voice.find_cue(sounds, guard::SCANNER_LOOP_EVENT)
                {
                    let at = g.scanner.at;
                    ctx.shared
                        .loops
                        .push((p.entity, cue, at, guard::SCANNER_LOOP_FADE));
                }
            }
            // A drone's engine, held while it flies; a jet's plane's engine.
            if ctx.pawn.plane.is_some() && !ctx.pawn.dying {
                let p = &*ctx.pawn;
                if let Some((cue, _)) = p.voice.find_cue(sounds, drones::ENGINE_EVENT) {
                    ctx.shared
                        .loops
                        .push((p.entity, cue, p.loc, drones::ENGINE_FADE));
                }
            }
            jet::engine_sounds(ctx.pawn, sounds, ctx.shared, true);
            // The pawn's frame: a change of form it was asked for begun (its forms'
            // anim trees and clips swapped: forms.rs), its physics, its animations
            // (its custom animations, then its anim tree's pose; the form it leaves
            // too, as it changes form), their notifies, its gun, its melee.
            let p = &mut *ctx.pawn;
            let sh = &mut *ctx.shared;
            forms::begin(p, form_clips);
            let clips = &**form_clips;
            pawn::step(p, view, clips, dt);
            let mut rng = brain.core.rng.clone();
            p.anim.update(dt, clips, &mut || rng.frand());
            update_aim_offset(brain, p, view, dt);
            let mut tree = std::mem::take(&mut p.tree);
            p.pose = tree.update(clips, p, dt, &mut || rng.frand());
            p.tree = tree;
            let left = forms::update(p, dt, &mut || rng.frand());
            brain.core.rng = rng;
            update_aim(p, dt);
            // (The form it leaves: its half's sounds, at the pawn.)
            for n in left {
                if let ClipNotify::Sound { cue, .. } = n {
                    sh.sounds.push((entity, cue, p.loc));
                }
            }
            let rounds = anim_notifies(brain, p, dt, sounds, sh);
            fire(
                brain,
                p,
                view,
                dt,
                rounds,
                (
                    &mut sh.shots,
                    &mut sh.player_damage,
                    &mut sh.barrier_damage,
                    &mut sh.sounds,
                    &mut sh.launches,
                    &mut sh.loops,
                ),
                (&mut sh.effects, &mut sh.effects_stopped),
            );
            melee::update(p, view, brain.core.enemy, dt, &mut sh.player_damage);
            notes_to_events(brain, p, view);
            // Its collection's tactical nodes, for the updater.
            if let (Some(nav), Some(t)) = (view.nav, brain.sensors.tactical.as_ref()) {
                sh.tactical.enqueue(nav, &t.picker.nearby.tactical);
            }
        }
        view.ais[i] = ai_view(slots[i].ai, true);
        // The squad's world state writes and the events to the others.
        let writes = std::mem::take(&mut shared.writes);
        apply_all(slots, writes);
        for (sender, e) in std::mem::take(&mut shared.broadcasts) {
            for s in slots.iter_mut() {
                if s.entity != sender {
                    s.ai.brain.core.inbox.push(e.clone());
                }
            }
        }
        let _ = entity;
    }
    // (Drones come in for a Titan no longer about: nobody's.)
    shared
        .drones_spawned
        .retain(|(o, _)| slots.iter().any(|s| s.entity == *o));
    // The shields the support drones put on and took off this frame, shown.
    drones::shield_effects(shared, slots);
    // The squad and the tactical system.
    let alive = |e: Entity| {
        view.players.iter().any(|p| p.entity == e && p.alive)
            || view.ais.iter().any(|a| a.entity == e)
    };
    let mut deliver = Vec::new();
    let mut writes = Vec::new();
    let mut rng = shared.rng.clone();
    // What the swarmers' turns need of each (AI.CalculateRangeToActor for the
    // range to its enemy).
    let mut swarmers = Some(
        slots
            .iter()
            .map(|s| {
                let (b, p) = (&s.ai.brain, &s.ai.pawn);
                let enemy = b.core.enemy;
                let a = enemy.and_then(|e| view.actor(e));
                let info = shared::SwarmerInfo {
                    alive: !p.dying && p.health > 0.0,
                    has_enemy: enemy.is_some(),
                    enemy_is_ai: a.as_ref().is_some_and(|a| !a.is_player),
                    clear_channel: b.core.ws_bool(htn::WS_CLEAR_ATTACK_CHANNEL),
                    range: a.map_or(f32::MAX, |a| {
                        ue::range_point_to_point(
                            p.loc,
                            p.radius,
                            a.loc,
                            a.radius,
                            p.half_height,
                            a.half_height,
                        )
                    }),
                };
                (s.entity, info)
            })
            .collect::<Vec<_>>(),
    );
    shared.squad.update(
        dt,
        now,
        &alive,
        &mut |_| swarmers.take().unwrap_or_default(),
        &mut rng,
        &mut deliver,
        &mut writes,
    );
    shared.rng = rng;
    apply_all(slots, writes);
    for (target, e) in deliver {
        if let Some(s) = slots.iter_mut().find(|s| s.entity == target) {
            s.ai.brain.core.inbox.push(e);
        }
    }
    shared
        .tactical
        .remove_dead_enemies(|e| view.players.iter().any(|p| p.entity == e && p.alive));
    if let Some(nav) = view.nav {
        shared.tactical.update(nav, view, TACTICAL_UPDATES);
    }
    shared.los.retain(|_, (_, t)| now - *t < 1.0);
}

/// World state writes, each to its brain.
fn apply_all(slots: &mut [Slot], writes: Vec<(Entity, WsWrite)>) {
    for (target, w) in writes {
        if let Some(s) = slots.iter_mut().find(|s| s.entity == target) {
            let mut one = vec![(target, w)];
            apply_writes(&mut s.ai.brain, target, &mut one);
        }
    }
}

/// TnAIAlerter.Alert (and DoAlert's BuddySawEnemy): a soldier unaware of the
/// players goes where the alert's target is; every one hears the alarm.
fn alert(brain: &mut Brain, ctx: &mut Ctx, target: Entity, at: Vec3) {
    let symbol = match brain.core.ai_type {
        shared::AICT_SOLDIER => Some(96),
        shared::AICT_ROCKET_TROOPER => Some(76),
        _ => None,
    };
    if !brain.core.aware_of_threats()
        && let Some(symbol) = symbol
    {
        let goal = brain.core.d.make_task(
            symbol,
            &[
                ("Destination", brain::LR_KISMET_MOVE_TO_ACTOR as i32),
                ("Range", 100),
                ("Speed", 2),
                ("LookAt", 0),
            ],
        );
        brain
            .core
            .store(brain::LR_KISMET_MOVE_TO_ACTOR, at, Vec3::ZERO);
        brain.core.set_ws(htn::WS_LOCATION, 0);
        brain.request_override_plan(ctx, goal, false);
    }
    brain.handle_event(ctx, Event::about(EventId::Alarm, Some(target)));
    brain.handle_event(ctx, Event::about(EventId::BuddySawEnemy, Some(target)));
}

/// The pawn's notes for its controller: a player bumped (NotifyBump: WS_DangerClose's
/// bit 1 and Bump), a wall run into (NotifyHitWall: HitWall if it is no floor, and
/// HitSurface).
fn notes_to_events(brain: &mut Brain, p: &Pawn, view: &WorldView) {
    for e in &p.notes.bumped {
        if view.player(*e).is_some() {
            brain.core.set_ws_bit(htn::WS_DANGER_CLOSE, 1, true);
            brain.core.inbox.push(Event::about(EventId::Bump, Some(*e)));
        }
    }
    if let Some(n) = p.notes.hit_wall {
        if n.z < pawn::WALKABLE_FLOOR_Z {
            brain.core.inbox.push(Event::about(EventId::HitWall, None));
        }
        brain
            .core
            .inbox
            .push(Event::about(EventId::HitSurface, None));
    }
}

/// Where an animation's effect plays (HmAnimNotify_PlayEffect.StartEffect): its
/// socket's frame. On a model that hasn't that socket it doesn't play at all: the
/// game's PlayEffectOnSocket fails to attach it (an unattached effect isn't drawn)
/// and GetSocketWorldMatrix to find it for one left in the world. (The Scrappers'
/// fist attack, the Leaper's, plays the Leaper's boosters and arm trails on
/// sockets the Scrappers' meshes haven't: played at the Scrapper's middle instead,
/// the boosters' endless glow had hung in the air where each attack began, a lilac
/// orb.) The pawn's own place and facing while its model's sockets aren't read yet
/// (its first frames) or for a model with none read (the Swarmers').
fn effect_frame(p: &Pawn, socket: &str) -> Option<(Vec3, [Vec3; 3])> {
    match p.socket_frame(socket) {
        Some(frame) => Some(frame),
        None if p.sockets.is_empty() => {
            let (x, y, z) = weapon::axes(0.0, p.yaw);
            Some((p.loc, [x, y, z]))
        }
        None => None,
    }
}

/// The notifies the pawn's animations passed this frame: an HTN event to its
/// controller (TnAnimNotify_HtnEvent: AnimNotify, a grenade's release), a melee
/// blow's sweep begun (TnAnimNotify_DamageSweep: its melee service's), a round
/// (TnAnimNotify_AIFireRound: the aimer on for it, along the gun if it says) or the
/// gun fired for a while (TnAnimNotify_AIFireWeapon: blind fire, the aimer along
/// the gun and the weapon controller shooting unrestrained until it ends); and its
/// sounds, each only if its node weighs at least its MinWeight in the tree: a
/// footstep (HmFootstepComponent.PlayFootStep: its type's FS_DEFAULT_ event, the
/// floors' material's, through its sound set, at its feet), a cue
/// (HmAnimNotify_Sound) or a sound event's (HmAnimNotify_SoundEvent) at its socket,
/// a line of dialog for the dialog manager (HmAnimNotify_Dialog). The rounds to
/// fire at once, and whether along the gun.
fn anim_notifies(
    brain: &mut Brain,
    p: &mut Pawn,
    dt: f32,
    sounds: &audio::AiSounds,
    sh: &mut Shared,
) -> Vec<bool> {
    let mut rounds = Vec::new();
    let entity = brain.core.entity;
    let at_socket = |p: &Pawn, socket: &str| {
        if socket.is_empty() {
            p.loc
        } else {
            p.socket_location(socket)
        }
    };
    for (_, n, weight) in std::mem::take(&mut p.anim.notifies) {
        if weight + 1e-4 < n.min_weight() {
            continue;
        }
        match n {
            ClipNotify::Htn {
                event,
                socket,
                mirrored,
                ..
            } => {
                brain.core.inbox.push(Event {
                    id: EventId::AnimNotify,
                    actor: None,
                    damage: None,
                    noise: None,
                    notify: Some(brain::Notify {
                        event,
                        socket,
                        mirrored_socket: mirrored,
                    }),
                });
            }
            ClipNotify::Sweep {
                secs, offset, half, ..
            } => {
                if p.melee.is_attacking() {
                    let at = p.melee.time;
                    p.melee.sweeps.push(melee::SweepNotify {
                        at,
                        secs,
                        offset,
                        half,
                    });
                }
            }
            // TnAnimNotify_ImpactDamageSweep: its melee service's impact sweep
            // begun (BeginImpactSweep): its damage type's a melee one's (a melee
            // blow's death), and pushes if it respects forces (the shockwave's,
            // the melee's).
            ClipNotify::Impact {
                secs,
                socket,
                half,
                damage,
                damage_type,
                forward,
                sideways,
                vertical,
                ..
            } => {
                let melee = matches!(&*damage_type, "TnDamageTypeMelee" | "TnDamageTypeLeapSmash");
                let push = melee || &*damage_type == "TnDamageTypeShockwave";
                let mut sweeper = melee::TraceSweeper::default();
                sweeper.begin(p, socket, half, secs);
                p.melee.impact = Some(melee::ImpactSweep {
                    sweeper,
                    damage,
                    damage_type: crate::killfeed::damage_name(&damage_type),
                    melee,
                    push,
                    forward,
                    sideways,
                    vertical,
                });
            }
            ClipNotify::Round { along_gun, .. } => rounds.push(along_gun),
            ClipNotify::Burst { secs, .. } => {
                p.aimer.activate();
                p.aimer.mode = 2;
                weapon::activate(brain);
                brain.shooter.set_unrestrained_fire(true);
                brain.shooter.state = weapon::ShooterState::Shooting;
                p.weapon.start_fire();
                p.blind_fire_left = secs.max(dt);
            }
            ClipNotify::Step { kind, .. } => {
                let event = audio::FOOTSTEP_EVENTS
                    .get(kind as usize)
                    .copied()
                    .unwrap_or("FS_DEFAULT_WALK");
                if let Some((cue, _)) = p.voice.find_cue(sounds, event) {
                    sh.sounds.push((entity, cue, p.feet()));
                }
            }
            ClipNotify::Sound { cue, socket, .. } => {
                sh.sounds.push((entity, cue, at_socket(p, &socket)))
            }
            ClipNotify::Event { event, socket, .. } => {
                if let Some((cue, _)) = p.voice.find_cue(sounds, &event) {
                    sh.sounds.push((entity, cue, at_socket(p, &socket)));
                }
            }
            ClipNotify::Dialog { event, death, .. } => sh.dialog.push(audio::DialogRequest::Say {
                speaker: entity,
                event,
                last: death,
            }),
            ClipNotify::Voice { cue, .. } => sh.dialog.push(audio::DialogRequest::Cue {
                speaker: entity,
                cue,
            }),
            // HmAnimNotify_PlayEffect: its particle system on its socket (none on a
            // model without that socket: effect_frame).
            ClipNotify::Effect {
                system,
                socket,
                attached,
                offset,
                turn,
                params,
                auto_off,
                ..
            } => {
                let Some((at, axes)) = effect_frame(p, &socket) else {
                    continue;
                };
                sh.effects.push(shared::Effect {
                    by: entity,
                    system,
                    socket,
                    at,
                    axes,
                    offset,
                    turn,
                    attached,
                    only_once: false,
                    restart: false,
                    params,
                    auto_off,
                });
            }
            // A change of form's half: the form it is in hidden or shown, its alt
            // form's effects allowed or not (forms.rs).
            ClipNotify::Toggle { hide, .. } => forms::toggle(p, hide),
            ClipNotify::VehicleFx { allowed, .. } => forms::vehicle_fx(p, allowed),
            // TnAnimNotify_SpawnProjectile: its rounds (TnProjectileSpawner).
            ClipNotify::Projectile {
                socket,
                class,
                cone,
                count,
                ..
            } => {
                let data = sh.notify_projectiles.get(&*class).cloned();
                for _ in 0..count {
                    if let Some(data) = data.clone() {
                        spawn_projectile(brain, p, &socket, cone, data, sh);
                    }
                }
            }
        }
    }
    // The blind fire's end (the notify's NotifyEnd).
    if p.blind_fire_left > 0.0 {
        p.blind_fire_left -= dt;
        if p.blind_fire_left <= 0.0 {
            p.aimer.deactivate();
            weapon::deactivate(brain, p);
        }
    }
    rounds
}

/// TnProjectileSpawner.SpawnProjectile: a projectile from a socket (its place, and
/// its facing turned at random up to half `cone` degrees either way:
/// FindAimDirection), homing in on its shooter's enemy (FindTarget,
/// SetHomingTarget) if it homes. None without the socket.
fn spawn_projectile(
    brain: &mut Brain,
    p: &Pawn,
    socket: &str,
    cone: f32,
    data: Arc<weapon::ProjectileData>,
    sh: &mut Shared,
) {
    let Some((from, axes)) = p.socket_frame(socket) else {
        return;
    };
    let yaw = (brain.core.rng.frand() - 0.5) * cone * ue::ROT_PER_DEG;
    let local = ue::vec_of(0.0, yaw);
    let dir = ue::normal(axes[0] * local.x + axes[1] * local.y + axes[2] * local.z);
    sh.launches.push(shared::Launch {
        by: p.entity,
        from,
        dir,
        data,
        target: brain.core.enemy,
    });
}

/// TnPawn.UpdateAimOffset: the aim its aim animator gives (get_AimOffset: its
/// AimOffsetScale's share, within a whole aim each way: a quarter turn), what its
/// anim tree's aim offset node reads (AimOffsetPct).
fn update_aim_offset(brain: &Brain, p: &mut Pawn, view: &WorldView, dt: f32) {
    // (A rotation's turn from the pawn's, Normalize(R - Rotation): yaw and pitch
    // over a quarter turn.)
    let turn_from = |p: &Pawn, dir: Vec3| {
        Vec2::new(
            ue::wrap_rot(ue::yaw_of(dir) - p.yaw) / 16384.0,
            ue::wrap_rot(ue::pitch_of(dir) - p.pitch) / 16384.0,
        )
    };
    let offset = match p.aim_animator {
        // TnPawnAimAnimator: its base aim rotation (its aimer's way while it aims,
        // else its controller's, its own) from its rotation.
        pawn::AimAnimator::Pawn => {
            if p.aimer.active {
                turn_from(p, weapon::aim_direction(p, p.muzzle))
            } else {
                Vec2::ZERO
            }
        }
        // TnAiLookAtAimAnimator: where its aimer aims (GetAimLocation) from where
        // it stands (its weapon's place), from its rotation.
        pawn::AimAnimator::LookAt => turn_from(p, weapon::aim_location(p) - p.loc),
        pawn::AimAnimator::Sniper(last) => {
            let o = sniper::aim_offset(brain, p, view.nav, last, dt);
            p.aim_animator = pawn::AimAnimator::Sniper(o);
            o
        }
    };
    p.aim_offset_pct = (offset * p.aim_offset_scale).clamp(Vec2::splat(-1.0), Vec2::splat(1.0));
}

/// The aim offset's turn towards where the aimer aims (TnAnimNodeAimer: the gun
/// follows the aim at its rate), and back to the pawn's facing when it isn't
/// aiming.
fn update_aim(p: &mut Pawn, dt: f32) {
    const RATE: f32 = 270.0 * ue::ROT_PER_DEG;
    let (want_pitch, want_yaw) = if p.aimer.active {
        let dir = weapon::aim_direction(p, p.muzzle);
        (ue::pitch_of(dir), ue::wrap_rot(ue::yaw_of(dir) - p.yaw))
    } else {
        (0.0, 0.0)
    };
    let step = RATE * dt;
    let turn = |cur: f32, want: f32| {
        let d = ue::wrap_rot(want - cur);
        if d.abs() <= step {
            want
        } else {
            cur + step * d.signum()
        }
    };
    p.aimer.current = (
        turn(p.aimer.current.0, want_pitch),
        turn(p.aimer.current.1, want_yaw),
    );
    // The muzzle: its gun's MuzzleFlash socket, as its model's last pose put it
    // (read_sockets), else ahead of its chest.
    p.muzzle = match p.muzzle_local {
        Some(at) => p.feet() + ue::rotate_yaw(at, p.yaw),
        None => p.loc + Vec3::Z * p.half_height * 0.45 + weapon::gun_direction(p) * p.radius * 1.1,
    };
}

/// The gun's frame: the rounds it fires (InstantFire: from its muzzle along the
/// aim with its spread, striking the first pawn or wall, its damage scaled by the
/// range; a fifth of a burst round's tracers shown).
#[allow(clippy::type_complexity)]
fn fire(
    brain: &mut Brain,
    p: &mut Pawn,
    view: &WorldView,
    dt: f32,
    rounds: Vec<bool>,
    (shots, damage, barrier_damage, sounds, launches, loops): (
        &mut Vec<shared::Shot>,
        &mut Vec<shared::PlayerDamage>,
        &mut Vec<(Entity, f32)>,
        &mut Vec<(Entity, &'static str, Vec3)>,
        &mut Vec<shared::Launch>,
        &mut Vec<(Entity, &'static str, Vec3, f32)>,
    ),
    (effects, stopped): (&mut Vec<shared::Effect>, &mut Vec<(Entity, Arc<str>)>),
) {
    let muzzle = p.muzzle;
    let fx = gun_fx(&p.weapon.data.class);
    let aim = weapon::aim_direction(p, muzzle);
    let along_gun = weapon::gun_direction(p);
    let spread = p.weapon.current_spread();
    let range = p.weapon.data.range;
    let entity = p.entity;
    let enemy = brain.core.enemy;
    let rng = &mut brain.core.rng;
    let gun = p.weapon.data.class.clone();
    let tracer_chance = fx.tracer_chance;
    // One trace of a round, from the muzzle it leaves (its barrel's): what it
    // strikes first (a wall, a player's body, another enemy), its damage to a
    // player (its range's scale of it), its shot. (A pellet that only passes
    // through a player's collision cylinder, beside his body, misses him, as the
    // game's traces do: PlayerView::body.)
    let mut shoot_trace = |data: &weapon::GunData,
                           first: bool,
                           muzzle: Vec3,
                           dir: Vec3,
                           rng: &mut ue::Rng|
     -> shared::Shot {
        let end = muzzle + dir * range;
        let wall = view.trace(muzzle, end);
        let mut far = wall.map_or(range, |(at, _)| (at - muzzle).length());
        let mut normal = wall.map(|(_, n)| n);
        // (The players' barriers are world geometry to it: TnBarrierSpawnable.)
        let mut barrier = None;
        if let Some((t, n, b)) = view.barrier_hit(muzzle, dir, far)
            && t < far
        {
            far = t;
            normal = Some(n);
            barrier = Some(b);
        }
        let mut struck = None;
        for pl in &view.players {
            if !pl.alive {
                continue;
            }
            if let Some(t) = player_trace(pl, muzzle, dir)
                && t < far
            {
                far = t;
                struck = Some(pl.entity);
                barrier = None;
            }
        }
        for a in &view.ais {
            if a.entity == entity || !a.alive {
                continue;
            }
            if let Some(t) = ray_cylinder(muzzle, dir, a.loc, a.radius, a.half_height)
                && t < far
            {
                far = t;
                struck = None;
                barrier = None;
            }
        }
        let to = muzzle + dir * far;
        // (A barrier it strikes takes its damage, as a pawn would: TakeDamage.)
        if let Some(b) = barrier {
            barrier_damage.push((b, data.damage * data.range_scale.value_at(far)));
        }
        if let Some(pl) = struck {
            let scale = data.range_scale.value_at(far);
            // (What its round kills with: its damage type's death and gib
            // distance, its InstantHitMomentum.)
            let gib = crate::enemies::gib_distance_of(&data.damage_type);
            damage.push(shared::PlayerDamage {
                player: pl,
                by: entity,
                amount: data.damage * scale,
                at: to,
                dir,
                melee: false,
                momentum: None,
                jam: false,
                shot: Some((
                    crate::enemies::death_of_damage_type(&data.damage_type),
                    data.momentum,
                    gib > 0.0 && far <= gib * ue::UU_PER_M,
                )),
                damage: crate::killfeed::damage_name(&data.damage_type),
            });
        }
        shared::Shot {
            shooter: entity,
            from: muzzle,
            to,
            hit_player: struck,
            hit_normal: if struck.is_none() { normal } else { None },
            tracer: rng.frand() < tracer_chance,
            gun: gun.clone(),
            first,
            traced: true,
            damage: data.damage * data.range_scale.value_at(far),
            damage_type: crate::killfeed::damage_name(&data.damage_type),
        }
    };
    // A round: each of its traces (the shotgun's pellets: SingleFireAction, each
    // its spread pattern's next offset).
    // (Its muzzle socket's frame; before its model has posed it, the muzzle along
    // the gun.)
    let muzzle_frame = p.socket_frame(MUZZLE_SOCKET).unwrap_or_else(|| {
        let (x, y, z) = weapon::axes(p.aimer.current.0, p.yaw + p.aimer.current.1);
        (muzzle, [x, y, z])
    });
    // (A gun with muzzles by turns, on its wielder's model (a jet's) or on the gun
    // in hand (the heavies' two barrels): each socket's frame and the aim from it;
    // each round leaves from the next.)
    let names = match pawn_muzzles(&gun) {
        [] => fx.muzzles,
        names => names,
    };
    let turns: Vec<(&'static str, Vec3, [Vec3; 3], Vec3)> = match names {
        names if names.len() > 1 => names
            .iter()
            .filter_map(|n| {
                p.socket_frame(n)
                    .map(|(at, axes)| (*n, at, axes, weapon::aim_direction(p, at)))
            })
            .collect(),
        _ => Vec::new(),
    };
    // (Its flash on a socket of its own with every round: that socket's frame.)
    let (flash_socket, flash) = fx.socket_flash;
    let flash_frame = if flash.is_empty() {
        None
    } else {
        p.socket_frame(flash_socket)
    };
    let mut shoot = |g: &mut weapon::Gun, aim: Vec3| {
        let data = g.data.clone();
        let (socket, from, frame, aim) = match turns.get(g.muzzle_index % turns.len().max(1)) {
            Some(&(name, at, axes, aim)) => {
                g.muzzle_index = (g.muzzle_index + 1) % turns.len();
                (name, at, (at, axes), aim)
            }
            None => (MUZZLE_SOCKET, muzzle, muzzle_frame, aim),
        };
        // Its sound: its StartFire's (the burst's, with its first round), or
        // each round's (FireShot's).
        if (g.start_now || fx.shoot_each_round) && !fx.shoot.is_empty() {
            sounds.push((entity, fx.shoot, from));
        }
        // Its flash held on its muzzle (the rest leave theirs with the shot).
        if fx.muzzle_on_socket && !fx.muzzle.is_empty() {
            let (at, axes) = frame;
            effects.push(shared::Effect {
                by: entity,
                system: Arc::from(fx.muzzle),
                socket: Arc::from(socket),
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
        if let Some((at, axes)) = flash_frame {
            effects.push(shared::Effect {
                by: entity,
                system: Arc::from(flash),
                socket: Arc::from(flash_socket),
                at,
                axes,
                offset: Vec3::ZERO,
                turn: Vec3::ZERO,
                attached: true,
                only_once: false,
                restart: true,
                params: None,
                auto_off: None,
            });
        }
        if let Some(proj) = data.projectile.as_ref() {
            // ProjectileFire: its round from the muzzle the aim's way (with its
            // spread), and its flash.
            for _ in 0..data.shots_to_fire.max(1) {
                let dir = weapon::add_spread(aim, spread, rng);
                launches.push(shared::Launch {
                    by: entity,
                    from,
                    dir,
                    data: proj.clone(),
                    target: enemy,
                });
            }
            shots.push(shared::Shot {
                shooter: entity,
                from,
                to: from + aim,
                hit_player: None,
                hit_normal: None,
                tracer: false,
                gun: gun.clone(),
                first: true,
                traced: false,
                damage: 0.0,
                damage_type: "",
            });
            return;
        }
        for pellet in 0..data.shots_to_fire.max(1) {
            let dir = if data.spread_pattern.is_empty() {
                weapon::add_spread(aim, spread, rng)
            } else {
                let initial = data.initial_spread();
                let modifier = if initial == 0.0 {
                    1.0
                } else {
                    spread / initial
                };
                let n = data.spread_pattern.len();
                let offset = data.spread_pattern[g.spread_pattern_index % n];
                g.spread_pattern_index = (g.spread_pattern_index + 1) % n;
                weapon::add_pattern_spread(aim, offset, modifier)
            };
            let shot = shoot_trace(&data, pellet == 0, from, dir, rng);
            shots.push(shot);
        }
    };
    // (The rounds the animations fire: Weapon.FireAmmunition, the aimer's way or
    // along the gun.)
    p.weapon.stopped_now = false;
    for along in rounds {
        p.weapon
            .fire_round(|g| shoot(g, if along { along_gun } else { aim }));
    }
    let aim_now = if p.aimer.mode == 2 { along_gun } else { aim };
    p.weapon.tick(dt, |g| shoot(g, aim_now));
    // Its charge (the Guardian Blaster's): its glow held on its muzzle from its
    // StartCharging to its StopCharging, and its loop while it charges.
    let began = std::mem::take(&mut p.weapon.charge_started);
    let ended = std::mem::take(&mut p.weapon.charge_stopped);
    if !fx.charge.is_empty() {
        if began && !ended {
            let (at, axes) = muzzle_frame;
            effects.push(shared::Effect {
                by: entity,
                system: Arc::from(fx.charge),
                socket: Arc::from(MUZZLE_SOCKET),
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
        if ended && !began {
            stopped.push((entity, Arc::from(fx.charge)));
        }
    }
    if p.weapon.charging && !fx.charge_loop.is_empty() {
        loops.push((entity, fx.charge_loop, p.muzzle, fx.charge_loop_fade));
    }
    // (StopFire's sound, the burst's tail, once it stops.)
    p.weapon.end_frame();
    if p.weapon.stopped_now && !fx.tail.is_empty() {
        sounds.push((entity, fx.tail, p.muzzle));
    }
    // (Its loop, held while it fires.)
    if p.weapon.effects_firing && !fx.shoot_loop.is_empty() {
        loops.push((entity, fx.shoot_loop, p.muzzle, LOOP_FADE));
    }
    // Its reload's effects: its sound, and its wielder's animation.
    if std::mem::take(&mut p.weapon.reload_started) {
        if !fx.reload_sound.is_empty() {
            sounds.push((entity, fx.reload_sound, p.muzzle));
        }
        if !fx.reload_anim.is_empty() {
            p.anim.enqueue(
                anim::AnimParams {
                    name: Arc::from(fx.reload_anim),
                    mask: anim::MASK_UPPER_BODY,
                    blend_in: fx.reload_blend,
                    blend_out: fx.reload_blend,
                    priority: 1,
                    ..Default::default()
                },
                None,
                None,
            );
        }
    }
    // TnAiWeaponController.OnWeaponFired: each round off the burst.
    let fired = p.weapon.fired_now as i32;
    if fired > 0 {
        brain.shooter.burst_shots_remaining -= fired;
        // (TnWeapon.PreServerFire: a shot gives a cloaked pawn away, ExposeSelf.)
        p.cloak.expose();
    }
}

/// Where a ray (unit `dir`) enters an upright cylinder (its middle `c`, its radius
/// and half height), if it does.
fn ray_cylinder(o: Vec3, dir: Vec3, c: Vec3, radius: f32, half: f32) -> Option<f32> {
    let ox = o.x - c.x;
    let oy = o.y - c.y;
    let a = dir.x * dir.x + dir.y * dir.y;
    let b = 2.0 * (ox * dir.x + oy * dir.y);
    let cc = ox * ox + oy * oy - radius * radius;
    let in_height = |t: f32| ((o.z + dir.z * t) - c.z).abs() <= half;
    if cc <= 0.0 && (o.z - c.z).abs() <= half {
        return Some(0.0);
    }
    if a > 1e-8 {
        let disc = b * b - 4.0 * a * cc;
        if disc >= 0.0 {
            let t = (-b - disc.sqrt()) / (2.0 * a);
            if t >= 0.0 && in_height(t) {
                return Some(t);
            }
        }
    }
    // Its top or bottom.
    if dir.z.abs() > 1e-6 {
        for z in [c.z + half, c.z - half] {
            let t = (z - o.z) / dir.z;
            if t >= 0.0 {
                let x = ox + dir.x * t;
                let y = oy + dir.y * t;
                if x * x + y * y <= radius * radius {
                    return Some(t);
                }
            }
        }
    }
    None
}

/// Core.Math.ClosestPointToLineSegment.
fn closest_point_on_segment(a: Vec3, b: Vec3, p: Vec3) -> Vec3 {
    let d = b - a;
    let len = d.length_squared();
    if len < 1e-6 {
        return a;
    }
    a + d * ((p - a).dot(d) / len).clamp(0.0, 1.0)
}

/// The players' pawns' Mass (Pawn's default, TnPawn's too), a blow's momentum's
/// divisor (Pawn.TakeDamage).
const PLAYER_MASS: f32 = 100.0;

/// What the AI did to the world this frame: the players' damage (their shield then
/// their health, as any blow), its shots' tracers and impacts and its sounds.
#[allow(clippy::too_many_arguments)]
fn effects(
    mut outputs: ResMut<AiOutputs>,
    mut players: Query<
        (
            Entity,
            &Transform,
            &mut crate::Player,
            Has<crate::LocalPlayer>,
        ),
        (Without<Ai>, Without<Foe>),
    >,
    mut fx: ResMut<crate::levelfx::FxSystems>,
    mut sfx: ResMut<crate::Sfx>,
    mut barriers: Query<&mut crate::equipment::Barrier>,
    foes: Query<&Foe>,
    mut barrels: ResMut<crate::barrels::Barrels>,
) {
    for (b, amount) in outputs.barrier_damage.drain(..) {
        if let Ok(mut barrier) = barriers.get_mut(b) {
            barrier.damage(amount);
        }
    }
    for d in outputs.player_damage.drain(..) {
        let Ok((_, transform, mut player, local)) = players.get_mut(d.player) else {
            continue;
        };
        let at = ue::from_ue(d.at);
        let dir = ue::dir_from_ue(d.dir);
        let hit = match d.shot {
            // (A gun's round: its damage type's death, momentum and gib.)
            Some((death, momentum, gib)) => crate::enemies::Hit {
                gib,
                ..crate::enemies::Hit::new(death, at, dir).with_momentum(momentum)
            },
            None if d.melee => {
                let hit = crate::enemies::Hit::new(Death::Melee, at, dir);
                match d.momentum {
                    Some(m) => hit.with_momentum_vec(ue::dir_from_ue(m)),
                    None => hit,
                }
            }
            None => match d.momentum {
                Some(m) => crate::enemies::Hit::new(Death::Explosive, at, dir)
                    .with_momentum_vec(ue::dir_from_ue(m)),
                None => crate::enemies::Hit::new(Death::Gunfire, at, dir),
            },
        }
        .of_type(d.damage);
        if crate::damage_player(&mut player, d.amount, hit) {
            // (Its striker, for the kill feed should it kill him.)
            if let Ok(foe) = foes.get(d.by) {
                player.last_attack.0 = crate::killfeed::Who::Enemy(foe.kind as u16);
            }
            sfx.play_from(
                crate::SND_PLAYER_HIT,
                crate::voice_at(local, transform.translation),
            );
            if local {
                player.hud_events.push(crate::hud::HudEvent::Damaged {
                    yaw: crate::heading(-dir),
                    amount: d.amount / crate::HIT_INDICATOR_HEALTH,
                });
            }
            // Its damage type's push (Pawn.TakeDamage: bExtraMomentumZ lifting a
            // walking pawn by at least 0.4 of it, over his Mass) and its victim
            // buff (the Guard's blast jams him).
            if let Some(m) = d.momentum {
                let mut m = m;
                if player.grounded {
                    m.z = m.z.max(0.4 * m.length());
                }
                crate::knock(
                    &mut player,
                    ue::dir_from_ue(m) / (PLAYER_MASS * ue::UU_PER_M),
                );
            }
            if d.jam {
                crate::abilities::jam(&mut player);
            }
        }
    }
    // The players the Guards' scanners caught (FireDecloakBeam): their cloaks
    // given away at once (ExposeSelf), the decloak sound on each as it is caught,
    // and jammed (TnBuffAbilityJammed, put on afresh while it holds him).
    for s in outputs.scanned.drain(..) {
        let Ok((_, transform, mut player, _)) = players.get_mut(s.player) else {
            continue;
        };
        if player.dead_left > 0.0 || player.dying {
            continue;
        }
        crate::cloak::expose(&mut player.ability, true);
        if !crate::abilities::jammed(&player) {
            let (_, height) = player.size();
            sfx.play_at(
                guard::DECLOAK_CUE,
                transform.translation + Vec3::Y * height * 0.5,
            );
        }
        crate::abilities::jam(&mut player);
    }
    // The local player's middle, while he lives: the rounds passing close by him are
    // heard flying past (HmWeapon.InstantFire's NotifyPawnNearlyShot, each round's
    // path; HmPawn.NotifyNearlyShot, the local player's alone).
    let target = players
        .iter()
        .find(|(_, _, p, local)| *local && p.dead_left <= 0.0 && !p.dying)
        .map(|(_, t, p, _)| t.translation + Vec3::Y * p.size().1 * 0.5);
    for s in outputs.shots.drain(..) {
        let gun = gun_fx(&s.gun);
        let from = ue::from_ue(s.from);
        let to = ue::from_ue(s.to);
        // (A round that stopped on an exploding barrel: its damage, an AI's.)
        if s.hit_player.is_none() && s.hit_normal.is_some() {
            barrels.shot(to, s.damage, s.damage_type, crate::barrels::By::Ai);
        }
        let way = to - from;
        let dir = way.normalize_or(Vec3::X);
        if s.first && !gun.muzzle.is_empty() && !gun.muzzle_on_socket {
            fx.spawn(gun.muzzle, from, dir, &[]);
        }
        if !s.traced {
            continue;
        }
        // (Its fly-by, where its path comes closest to him: within its near-miss
        // distance, a round that strikes him too.)
        if s.first
            && !gun.bullet_by.is_empty()
            && let Some(c) = target
        {
            let closest = closest_point_on_segment(from, to, c);
            if closest.distance(c) < gun.nearly_shot {
                sfx.play_at(gun.bullet_by, closest);
            }
        }
        if s.tracer && !gun.tracer.is_empty() {
            fx.spawn_tracer(gun.tracer, from, dir, way.length());
        }
        if let Some(n) = s.hit_normal {
            if !gun.impact.is_empty() {
                fx.spawn(gun.impact, to, ue::dir_from_ue(n), &[]);
            }
            sfx.play_at(gun.impact_sound, to);
        }
    }
    for (_, cue, at) in outputs.sounds.drain(..) {
        sfx.play_at(cue, ue::from_ue(at));
    }
    for (e, cue, at, fade) in outputs.loops.drain(..) {
        sfx.hold_fading(e.to_bits() ^ 0x4c4f_4f50, cue, Some(ue::from_ue(at)), fade);
    }
    // Their dialog: each line as its own owner's, so the speaker's next can cut it
    // short.
    for say in outputs.says.drain(..) {
        match say {
            audio::Say::Line {
                owner,
                cut,
                cue,
                at,
            } => {
                if let Some(c) = cut {
                    sfx.silence(c);
                }
                sfx.play_owned(owner, cue, 0.0, 1.0, Some(ue::from_ue(at)));
            }
            audio::Say::Hush { owner } => sfx.silence(owner),
        }
    }
}

/// The effects held on the AIs' sockets (their animations' attached
/// HmAnimNotify_PlayEffect, their guns' muzzle effects): each one carried with its
/// socket, at its place and turn there, until it is done, or the model it rides is
/// gone (its AI dead, or changed to its other form: an attached component goes
/// with its mesh).
#[derive(Resource, Default)]
pub struct SocketFx(Vec<SocketEffect>);

struct SocketEffect {
    id: crate::levelfx::FxId,
    by: Entity,
    system: Arc<str>,
    socket: Arc<str>,
    offset: Vec3,
    turn: [Vec3; 3],
    /// The form its AI was in as it started (a transforming AI's: forms.rs), whose
    /// model it rides.
    form: Option<u8>,
}

/// Whether a held effect has lost the model it rides: its AI changed to its other
/// form since it started (the jets' pulse attack's tell, Flyer_EMPAttack_Tell_FX,
/// held on its robot's WhirlwindSocket, would otherwise hang where the robot was
/// as the plane flies off). (A socket missing for a moment, its model's bones not
/// in yet or its hand's gun put away, only leaves it where it was meanwhile.)
fn effect_orphaned(form: Option<u8>, pawn: &pawn::Pawn) -> bool {
    match (form, pawn.forms.as_ref()) {
        (Some(was), Some(f)) => f.current != was,
        _ => false,
    }
}

/// An effect's frame on its socket (Unreal's space): the socket's place and axes
/// (`at`, `axes`), moved by `offset` and turned by `turn` (a rotator's axes) in
/// that frame, as an attached component's RelativeLocation and RelativeRotation.
fn on_socket(at: Vec3, axes: [Vec3; 3], offset: Vec3, turn: [Vec3; 3]) -> (Vec3, [Vec3; 3]) {
    let world = |v: Vec3| axes[0] * v.x + axes[1] * v.y + axes[2] * v.z;
    (at + world(offset), turn.map(world))
}

/// The animations' effects turned off with their clips (bAutoDeactivate), each
/// with the seconds left till it is (HmAnimNotify_PlayEffect's
/// AutoDeactivateParticleSystems).
#[derive(Resource, Default)]
pub struct EffectsOff(Vec<(crate::levelfx::FxId, f32)>);

/// Starts the effects the AIs played this frame (each at its socket, turned as it
/// is: its X and Y axes; the attached ones carried from now on), after stopping
/// the held ones they stopped and turning off those whose clips' time is up
/// (EffectsOff: DeactivateParticleSystemsBasedOnTime). One turned off with its
/// clip that is held on a socket where the same one is still on isn't started
/// again: that one plays on till the later of their times
/// (HmParticleEffectManagerComponent.PlayEffect's PlayCondition 1, FindEffectIndex
/// finding it). (The demo didn't turn them off at all: the Leaper's booster glow,
/// Leaper_Boosters_LowBurn_FX, which loops for good, started on four sockets with
/// each of its fist attacks, smash recoveries and hit reactions and burned on for
/// as long as it lived, its distortion and all: tens of them on each Leaper in a
/// fight, the frame rate falling with every attack.)
#[allow(clippy::too_many_arguments)]
fn play_effects(
    time: Res<Time>,
    mut outputs: ResMut<AiOutputs>,
    mut fx: ResMut<crate::levelfx::FxSystems>,
    mut carried: ResMut<SocketFx>,
    mut off: ResMut<EffectsOff>,
    enemies: Query<&crate::enemies::Enemy>,
    kinds: Res<crate::enemies::Kinds>,
    ais: Query<&Ai>,
) {
    let dt = time.delta_secs();
    off.0.retain_mut(|(id, left)| {
        *left -= dt;
        if *left <= 0.0 {
            fx.stop(*id);
        }
        *left > 0.0 && fx.alive(*id)
    });
    for (by, system) in outputs.effects_stopped.drain(..) {
        carried.0.retain(|c| {
            if c.by == by && c.system == system {
                fx.stop(c.id);
                false
            } else {
                true
            }
        });
    }
    for e in outputs.effects.drain(..) {
        if e.only_once
            && carried
                .0
                .iter()
                .any(|c| c.by == e.by && c.system == e.system && fx.alive(c.id))
        {
            continue;
        }
        // (Started over: the last one on its socket spawns no more, what it has out
        // still carried with the socket, and a new one starts from the beginning.)
        if e.restart {
            for c in &carried.0 {
                if c.by == e.by && c.system == e.system && c.socket == e.socket {
                    fx.stop(c.id);
                }
            }
        }
        let turn = ue::rotator_axes(e.turn.x, e.turn.y, e.turn.z);
        if let Some(secs) = e.auto_off
            && e.attached
            && let Some(c) = carried.0.iter().find(|c| {
                c.by == e.by
                    && c.system == e.system
                    && c.socket == e.socket
                    && c.offset == e.offset
                    && c.turn == turn
                    && fx.is_on(c.id)
            })
        {
            match off.0.iter_mut().find(|(id, _)| *id == c.id) {
                Some((_, left)) => *left = left.max(secs),
                None => off.0.push((c.id, secs)),
            }
            continue;
        }
        let (at, axes) = on_socket(e.at, e.axes, e.offset, turn);
        let (place, x, y) = (
            ue::from_ue(at),
            ue::dir_from_ue(axes[0]),
            ue::dir_from_ue(axes[1]),
        );
        // (Its notify's parameters: his EnergonColor, his side's channel.)
        let mut params: Vec<(&str, [u8; 4])> = Vec::new();
        if let Some(names) = e.params.as_ref()
            && let Some(kind) = enemies.get(e.by).ok().and_then(|m| kinds.kinds.get(m.kind))
        {
            for name in names.iter() {
                if &**name == "@team" {
                    params.push((crate::levelfx::MODULE_CHANNEL, kind.module_channel()));
                } else {
                    params.push((&**name, kind.energon_color()));
                }
            }
        }
        let Some(id) = fx.spawn(&e.system, place, x, &params) else {
            continue;
        };
        fx.place_turned(id, place, x, y);
        if let Some(secs) = e.auto_off {
            off.0.push((id, secs));
        }
        if e.attached {
            let form = ais
                .get(e.by)
                .ok()
                .and_then(|ai| ai.pawn.forms.as_ref().map(|f| f.current));
            carried.0.push(SocketEffect {
                id,
                by: e.by,
                system: e.system,
                socket: e.socket,
                offset: e.offset,
                turn,
                form,
            });
        }
    }
}

/// Carries the held effects with their sockets (where this frame's pose put them,
/// read_sockets), before the effects run; one whose model is gone (its AI dead or
/// in its other form: effect_orphaned) goes with it.
fn carry_effects(
    ais: Query<&Ai>,
    mut fx: ResMut<crate::levelfx::FxSystems>,
    mut carried: ResMut<SocketFx>,
) {
    carried.0.retain(|c| {
        if !fx.alive(c.id) {
            return false;
        }
        let Ok(ai) = ais.get(c.by) else {
            fx.kill(c.id);
            return false;
        };
        if effect_orphaned(c.form, &ai.pawn) {
            fx.kill(c.id);
            return false;
        }
        if let Some((at, axes)) = ai.pawn.socket_frame(&c.socket) {
            let (at, axes) = on_socket(at, axes, c.offset, c.turn);
            fx.place_turned(
                c.id,
                ue::from_ue(at),
                ue::dir_from_ue(axes[0]),
                ue::dir_from_ue(axes[1]),
            );
        }
        true
    });
}

/// Every held effect let go (a new match).
fn clear_effects(
    mut fx: ResMut<crate::levelfx::FxSystems>,
    mut carried: ResMut<SocketFx>,
    mut off: ResMut<EffectsOff>,
) {
    for c in carried.0.drain(..) {
        fx.stop(c.id);
    }
    off.0.clear();
}

/// An AI gun's effects (its weapon mesh's TnWeaponEffects): with each round its
/// muzzle flash (FireShot), each trace's tracer (its ChanceOfPlaying of them, or
/// all: CreateTracerForEachImpact) and where it strikes its impact and its sound;
/// its firing sound (StartFire's, the burst's; or FireShot's, each round's), its
/// StopFire's, and its equip and holster sounds (Equip, UnEquip).
pub struct GunFx {
    pub muzzle: &'static str,
    /// Its flash held on its muzzle socket (TnWeaponEffectPlayMuzzleFX attaches
    /// it), not started again while its last one plays
    /// (PEC_PlayOnlyIfNotAlreadyPlaying); else left where the round left.
    pub muzzle_on_socket: bool,
    /// The muzzle sockets on the gun in hand its rounds leave from by turns (its
    /// MuzzleSockets, with TnWeaponEffectIncrementMuzzleSocket moving on to the
    /// next after each round: its flash, its tracer and the round itself from the
    /// one it is on); none: MuzzleFlash alone.
    pub muzzles: &'static [&'static str],
    /// A flash on a socket of the gun in hand with every round besides
    /// (TnWeaponEffectPlayFXOnSocket: held on the socket, PEC_RestartIfAlreadyPlaying
    /// starting it over each round): the socket and the effect, if it has one.
    pub socket_flash: (&'static str, &'static str),
    pub tracer: &'static str,
    pub tracer_chance: f32,
    pub impact: &'static str,
    pub impact_sound: &'static str,
    pub shoot: &'static str,
    pub shoot_each_round: bool,
    /// Its loop while it fires (StartFire to StopFire), if it has one.
    pub shoot_loop: &'static str,
    pub tail: &'static str,
    pub equip: &'static str,
    pub holster: &'static str,
    /// Its reload's (TnWeaponEventReload's) sound, and the animation it plays on
    /// its wielder's upper body (high priority, its blends).
    pub reload_sound: &'static str,
    pub reload_anim: &'static str,
    pub reload_blend: f32,
    /// What it shows while it charges (TnWeaponEventStartCharging to
    /// TnWeaponEventStopCharging: the Guardian Blaster's): an effect on its
    /// muzzle, and a loop let go over its fade.
    pub charge: &'static str,
    pub charge_loop: &'static str,
    pub charge_loop_fade: f32,
    /// Its rounds flying past a player (its mesh's BulletBySound, played where a
    /// round's path comes closest to him: HmPawn.NotifyNearlyShot), and how close
    /// one comes to be heard (its data's NearlyShotDistance, metres;
    /// HmWeapon.NotifyPawnNearlyShot).
    pub bullet_by: &'static str,
    pub nearly_shot: f32,
}

/// The soldiers' rifle (TnWeaponBurstRifleAI: WEP_BurstRifle_AI_p.BurstRifle_AI_WEPMESH).
/// Its shot is the Photon Eliminator's SHOOT, as the Photon Burst Rifle's is, but a
/// soldier's sounds with every round, each at a pitch of its own (the game's
/// recordings of them: one every 0.11 to 0.13 s through a burst, each a little
/// higher or lower), and away from the camera it is the cue's distant shot
/// (sounds.txt's distance curves: its close-up burst, the Photon Burst Rifle's own
/// sound, only within 5 to 7.5 m).
const BURST_RIFLE_FX: GunFx = GunFx {
    muzzle: "AssaultRifleArm_MuzzleFlash_FX",
    muzzle_on_socket: false,
    muzzles: &[],
    socket_flash: ("", ""),
    tracer: "AssaultRifle_Tracer_FX",
    tracer_chance: 0.4,
    impact: "AssaultRifleArm_Squib_FX",
    impact_sound: "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_WORLD",
    shoot: "BL_WPN_GUN_PHOTON_ELIMINATOR.SHOOT",
    shoot_each_round: true,
    shoot_loop: "",
    tail: "BL_WPN_GUN_PHOTON_ELIMINATOR.SHOOT_TAIL",
    equip: "",
    holster: "",
    reload_sound: "",
    reload_anim: "",
    reload_blend: 0.0,
    charge: "",
    charge_loop: "",
    charge_loop_fade: 0.0,
    // (BurstRifle_SV_WEPDATA's NearlyShotDistance: 1000 units.)
    bullet_by: "BL_WPN_GUN__IMPT_BYS.BY_BALLISTIC",
    nearly_shot: 10.0,
};

/// The shotgunners' (TnWeaponShotgunAI: WEP_Shotgun_AI_p.Shotgun_AI_WEPMESH): a
/// tracer for each pellet.
const SHOTGUN_FX: GunFx = GunFx {
    muzzle: "TurretArm_MuzzleFlash_FX",
    muzzle_on_socket: false,
    muzzles: &[],
    socket_flash: ("", ""),
    tracer: "Trail_ScatterBlaster_SP_FX",
    tracer_chance: 1.0,
    impact: "Squib_ScatterBlaster_FX",
    impact_sound: "BL_WPN_GUN__IMPT_BYS.IMPT_SHOTGUN_WORLD",
    shoot: "BL_WPN_GUN_SCATTER_BLASTER.AUTO_SHOTGUN_SHOOT",
    shoot_each_round: true,
    shoot_loop: "",
    tail: "BL_WPN_GUN_SCATTER_BLASTER.AUTO_SHOTGUN_SHOOT_TAIL",
    equip: "",
    holster: "",
    reload_sound: "BL_WPN_GUN_SCATTER_BLASTER.ANIM_RELOAD",
    reload_anim: "",
    reload_blend: 0.0,
    charge: "",
    charge_loop: "",
    charge_loop_fade: 0.0,
    bullet_by: "",
    nearly_shot: 0.0,
};

/// The rocket troopers' launcher (TnWeaponRocketLauncherAI:
/// WEP_RocketLauncher_AI_p.RocketLauncher_AI_WEPMESH; its rounds' own effects:
/// rocket.rs).
const ROCKET_LAUNCHER_FX: GunFx = GunFx {
    muzzle: "RocketLauncherAI_MuzzleFlash_FX",
    muzzle_on_socket: false,
    muzzles: &[],
    socket_flash: ("", ""),
    tracer: "",
    tracer_chance: 0.0,
    impact: "",
    impact_sound: "",
    shoot: "BL_WPN_GUN_ROCKET.SHOOT",
    shoot_each_round: true,
    shoot_loop: "",
    tail: "",
    equip: "BL_WPN_GUN_ROCKET.ANIM_EQUIP",
    holster: "BL_WPN_GUN_ROCKET.ANIM_HOLSTER",
    reload_sound: "BL_WPN_GUN_ROCKET.ANIM_RELOAD",
    reload_anim: "",
    reload_blend: 0.0,
    charge: "",
    charge_loop: "",
    charge_loop_fade: 0.0,
    bullet_by: "",
    nearly_shot: 0.0,
};

/// The sharpshooters' (TnWeaponSharpShot: WEP_SharpShot_p.SharpShot_WEPMESH) and the
/// Snipers' (TnWeaponSniper: WEP_Sniper_p.Sniper_WEPMESH, its effects the same):
/// its flash, its tracer (a bolt along the shot with lightning riding it: levelfx.rs's
/// LocationEmitterDirect) and its impact, its shot, and its round flying past a
/// player, a laser's whoosh (BY_SNIPER).
const SHARP_SHOT_FX: GunFx = GunFx {
    muzzle: "MuzzleFlash_Sniper_AI_FX",
    muzzle_on_socket: false,
    muzzles: &[],
    socket_flash: ("", ""),
    tracer: "Tracer_Sniper_AI_FX",
    tracer_chance: 1.0,
    impact: "Sniper_Squib_FX",
    impact_sound: "BL_WPN_GUN__IMPT_BYS.IMPT_SNIPER_WORLD",
    shoot: "BL_WPN_GUN_AI_SNIPER.AI_LASER_SNIPER_SHOOT",
    shoot_each_round: true,
    shoot_loop: "",
    tail: "",
    equip: "",
    holster: "",
    reload_sound: "",
    reload_anim: "",
    reload_blend: 0.0,
    charge: "",
    charge_loop: "",
    charge_loop_fade: 0.0,
    // (Sniper_AI_WEPDATA's and SharpShot_AI_WEPDATA's NearlyShotDistance: 1000 units.)
    bullet_by: "BL_WPN_GUN__IMPT_BYS.BY_SNIPER",
    nearly_shot: 10.0,
};

/// The heavies' machine guns (TnWeaponHeavyGunnerMG, TnWeaponTitanHeavyMG: both
/// TR_Titan_AI_p.TitanHeavyMG_WEPMESH, the Heavy Gunner's from the Titan's class):
/// its two barrels by turns, a flash on the second with every round besides, a
/// tracer each round (a spray of sparks streaking from the barrel to where it
/// struck), its shot each round over its loop while it fires.
const HEAVY_MG_FX: GunFx = GunFx {
    muzzle: "WEP_TitanMG_MuzzleFlash_FX",
    muzzle_on_socket: false,
    muzzles: &["MuzzleFlash", "MuzzleFlash2"],
    socket_flash: ("MuzzleFlash2", "WEP_TitanMG_MuzzleFlash_FX"),
    tracer: "WEP_TitanMG_Tracer_FX",
    tracer_chance: 1.0,
    impact: "X18_Scrapmaker_Squib_FX",
    impact_sound: "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_WORLD",
    shoot: "BL_WPN_GUN_AI_TITAN.SHOOT",
    shoot_each_round: true,
    shoot_loop: "BL_WPN_GUN_AI_TITAN.SHOOT_LP",
    tail: "BL_WPN_GUN_AI_TITAN.SHOOT_TAIL",
    equip: "",
    holster: "",
    reload_sound: "BL_CHR_AI_TITAN.TITAN_GUN_OVERHEAT",
    reload_anim: "Gun_Overheat",
    reload_blend: 0.3,
    charge: "",
    charge_loop: "",
    charge_loop_fade: 0.0,
    bullet_by: "",
    nearly_shot: 0.0,
};

/// The socket a gun's muzzle effects are held on (its weapon mesh's MuzzleSockets:
/// MuzzleFlash, its in-hand mesh's).
const MUZZLE_SOCKET: &str = "MuzzleFlash";

/// The guns that are their wielders' own forms (WT_AltForm: the Snipers' rifle is
/// the insect, the jets' guns are their planes'), firing from sockets of its model
/// (their weapon meshes' MuzzleSockets: the Sniper's LaserSightSocket, on its
/// insect's head; a jet's rockets' two, WeaponSocket_Primary and Primary2, by
/// turns: TnWeaponEffectIncrementMuzzleSocket; its dogfight gun's launchers', on
/// those two, their MuzzleFlash, by turns too: the model's vgun sockets).
const PAWN_MUZZLES: &[(&str, &[&str])] = &[
    ("TnWeaponSniper", &["LaserSightSocket"]),
    (
        "TnWeaponFlyerRocket",
        &["WeaponSocket_Primary", "WeaponSocket_Primary2"],
    ),
    (
        "TnWeaponRocketVehicle",
        &["vgun0:MuzzleFlash", "vgun1:MuzzleFlash"],
    ),
];

/// A gun's muzzle sockets on its wielder's own model (PAWN_MUZZLES), none for a
/// gun in hand.
fn pawn_muzzles(class: &str) -> &'static [&'static str] {
    PAWN_MUZZLES
        .iter()
        .find(|(c, _)| *c == class)
        .map_or(&[], |(_, s)| *s)
}

/// The jets' rockets (TnWeaponFlyerRocket: WEP_FlyerRockets_AI_p.
/// FlyerRockets_AI_WEPMESH): with each round its flash held on the muzzle it leaves
/// from and its shot; its rounds' own effects: projectile.rs.
const FLYER_ROCKETS_FX: GunFx = GunFx {
    muzzle: "MuzzleFlash_HomRocket_FX",
    muzzle_on_socket: true,
    muzzles: &[],
    socket_flash: ("", ""),
    tracer: "",
    tracer_chance: 0.0,
    impact: "",
    impact_sound: "",
    shoot: "BL_WPN_GUN_AI_PLANE.FLYER_ROCKETS_SHOOT",
    shoot_each_round: true,
    shoot_loop: "",
    tail: "",
    equip: "",
    holster: "",
    reload_sound: "",
    reload_anim: "",
    reload_blend: 0.0,
    charge: "",
    charge_loop: "",
    charge_loop_fade: 0.0,
    bullet_by: "",
    nearly_shot: 0.0,
};

/// The jets' dogfight gun (TnWeaponRocketVehicle: WEP_RocketVh_p.
/// Rocket_Vehicle_WEPMESH, the trucks' launchers): with each round its flash held
/// on the launcher's muzzle it leaves from and its shot (TnWeaponEffectLowAmmoSound:
/// the same cue low on ammo, which it never is).
const ROCKET_VEHICLE_FX: GunFx = GunFx {
    muzzle: "MuzzleFlash_Blaster_FX",
    muzzle_on_socket: true,
    muzzles: &[],
    socket_flash: ("", ""),
    tracer: "",
    tracer_chance: 0.0,
    impact: "",
    impact_sound: "",
    shoot: "BL_WPN_GUN_ROCKET.SHOOT_VEH",
    shoot_each_round: true,
    shoot_loop: "",
    tail: "",
    equip: "",
    holster: "",
    reload_sound: "",
    reload_anim: "",
    reload_blend: 0.0,
    charge: "",
    charge_loop: "",
    charge_loop_fade: 0.0,
    bullet_by: "",
    nearly_shot: 0.0,
};

/// The Guard's Guardian Blaster (TnWeaponGuardianBlaster:
/// WEP_GuardianBlaster_p.GuardianBlaster_WEPMESH): while it charges, the glow on
/// its muzzle and its loop (StartCharging to StopCharging); with each round its
/// flash on its muzzle and its shot (FireShot's TnWeaponEffectLowAmmoSound, its
/// WeaponSound every round); its rounds' own effects: projectile.rs.
const GUARDIAN_BLASTER_FX: GunFx = GunFx {
    muzzle: "GuardianBlaster_MuzzleFlash_Fx",
    muzzle_on_socket: true,
    muzzles: &[],
    socket_flash: ("", ""),
    tracer: "",
    tracer_chance: 0.0,
    impact: "",
    impact_sound: "",
    shoot: "BL_WPN_GUN_AI_SENTRY.AI_GUARDIAN_BLASTER_SHOOT",
    shoot_each_round: true,
    shoot_loop: "",
    tail: "",
    equip: "",
    holster: "",
    reload_sound: "",
    reload_anim: "",
    reload_blend: 0.0,
    charge: "Guardian_Arm_Cannon_Charge_FX",
    charge_loop: "BL_CHR_AI_GUARD.GUARD_CHARGE_SHOOT_LP",
    charge_loop_fade: 0.25,
    bullet_by: "",
    nearly_shot: 0.0,
};

/// The guns' effects by their classes. (The Snipers' rifle, TnWeaponSniper's
/// WEP_Sniper_p.Sniper_WEPMESH, plays the sharpshooters' muzzle flash, tracer,
/// impact and shot.)
const GUN_FX: &[(&str, &GunFx)] = &[
    ("TnWeaponGuardianBlaster", &GUARDIAN_BLASTER_FX),
    ("TnWeaponBurstRifleAI", &BURST_RIFLE_FX),
    ("TnWeaponShotgunAI", &SHOTGUN_FX),
    ("TnWeaponRocketLauncherAI", &ROCKET_LAUNCHER_FX),
    ("TnWeaponSharpShot", &SHARP_SHOT_FX),
    ("TnWeaponSniper", &SHARP_SHOT_FX),
    ("TnWeaponHeavyGunnerMG", &HEAVY_MG_FX),
    ("TnWeaponTitanHeavyMG", &HEAVY_MG_FX),
    ("TnWeaponFlyerRocket", &FLYER_ROCKETS_FX),
    ("TnWeaponRocketVehicle", &ROCKET_VEHICLE_FX),
];

/// A gun's effects by its class (the soldiers' rifle's for one without its own).
pub fn gun_fx(class: &str) -> &'static GunFx {
    GUN_FX
        .iter()
        .find(|(c, _)| *c == class)
        .map_or(&BURST_RIFLE_FX, |(_, f)| *f)
}

/// Each AI's pose on its model (graph.rs), while it lives: the form it is in's (its
/// Enemy's type is the form's); as it changes form, the form it leaves plays its
/// half on that form's model (forms.rs).
fn animate(
    kinds: Res<Kinds>,
    ais: Query<(
        &Ai,
        &Enemy,
        &crate::enemies::Model,
        Option<&crate::enemies::Forms>,
    )>,
    mut players: Query<&mut AnimationPlayer>,
) {
    for (ai, enemy, model, model_forms) in &ais {
        if !enemy.alive() || model_forms.is_some_and(|mf| mf.active != mf.want) {
            continue;
        }
        if let Some(nodes) = kinds.kinds.get(enemy.kind).and_then(|k| k.ai.as_ref())
            && let Some(Ok(mut player)) = model.animator().map(|a| players.get_mut(a))
        {
            graph::apply(&ai.pawn.pose, nodes, &mut player);
        }
        if let (Some(f), Some(mf)) = (ai.pawn.forms.as_ref(), model_forms)
            && f.change.is_some()
            && let Some(parked) = mf.parked.as_ref()
            && let Some(nodes) = kinds
                .kinds
                .get(mf.kinds[1 - mf.active])
                .and_then(|k| k.ai.as_ref())
            && let Some(Ok(mut player)) = parked.animator().map(|a| players.get_mut(a))
        {
            graph::apply(&f.other.pose, nodes, &mut player);
        }
    }
}

/// Each AI's sockets where its model's pose put them this frame (its gun's muzzle,
/// its hands': the next frame's AI reads them, as UE3's GetSocketWorldLocation
/// reads the last pose), in its own frame; and its gun shown in the hand it holds
/// it in (the left's mesh once its handedness mirror is more than half way over)
/// while it has it out (TnWeapon attaches its meshes as it is equipped and
/// detaches them as it is put away: the Guard's arm cannons, its battle form's).
/// A gun hung on both arms (the Guard's: a cannon on each of its weapon's
/// attachment sockets) shows on both, whichever hand it aims with.
fn read_sockets(
    mut ais: Query<(&mut Ai, &crate::enemies::Model)>,
    globals: Query<&GlobalTransform>,
    mut shown: Query<&mut Visibility>,
) {
    for (mut ai, model) in &mut ais {
        let p = &mut ai.pawn;
        let hand = usize::from(p.pose.mirror >= 0.5);
        for (h, gun) in model.guns().iter().enumerate() {
            if let Some(g) = gun
                && let Ok(mut v) = shown.get_mut(*g)
            {
                let want = if (h == hand || model.guns_together()) && p.weapon.equipped {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if *v != want {
                    *v = want;
                }
            }
        }
        // Its vehicle gun's meshes (a jet's dogfight gun's launchers), shown while
        // it has that gun out (TnWeaponVehicle: hidden as it is equipped, out from
        // its first StartFire: Gun.out) and put away with it.
        for (e, class) in model.vguns() {
            let want = if p.weapon.equipped && p.weapon.out && **class == *p.weapon.data.class {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if let Ok(mut v) = shown.get_mut(*e)
                && *v != want
            {
                *v = want;
            }
        }
        let (feet, yaw) = (p.feet(), p.yaw);
        p.sockets.clear();
        p.muzzle_local = None;
        let pawn_muzzle = pawn_muzzles(&p.weapon.data.class).first().copied();
        for s in model.ai_sockets() {
            if s.hand.is_some_and(|h| h != hand) {
                continue;
            }
            let Ok(bone) = globals.get(s.bone) else {
                continue;
            };
            let local = ue::rotate_yaw(ue::to_ue(bone.transform_point(s.at)) - feet, -yaw);
            if (s.hand.is_some() && &*s.name == MUZZLE_SOCKET)
                || (s.hand.is_none() && pawn_muzzle == Some(&*s.name))
            {
                p.muzzle_local = Some(local);
            }
            // Its axes: Unreal's x, y and z are glTF's x, z and y (a mirror, which
            // the swap back to Unreal's space undoes).
            let turn = bone.rotation() * s.turn;
            let axes =
                [Vec3::X, Vec3::Z, Vec3::Y].map(|a| ue::rotate_yaw(ue::dir_to_ue(turn * a), -yaw));
            p.sockets.push(pawn::PawnSocket {
                name: s.name.clone(),
                at: local,
                axes,
            });
        }
    }
}

/// This PC's AI plays the enemies: it hosts, or plays alone (a client's follow the
/// host's: sync.rs).
fn hosting(net: Res<crate::net::Net>) -> bool {
    crate::waves::hosting(&net)
}

/// The AI's systems.
pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiWorld>()
            .init_resource::<AiOutputs>()
            .init_resource::<sync::Puppets>()
            .init_resource::<SocketFx>()
            .init_resource::<EffectsOff>()
            .init_resource::<drones::DroneBeams>()
            .init_resource::<NearGrenades>()
            .init_resource::<graph::MaskJobs>()
            .add_systems(
                OnEnter(crate::menu::AppState::Game),
                (
                    reset,
                    sync::reset,
                    grenade::clear_grenades,
                    projectile::clear_projectiles,
                    clear_effects,
                    scanner::clear_beams,
                    drones::clear_beams,
                ),
            )
            .add_systems(Update, graph::fill_mask_clips)
            .add_systems(
                Update,
                (
                    load,
                    attach,
                    // (Online, the host's AI thinks; a client's enemies follow it.)
                    think.run_if(hosting),
                    sync::follow_host.run_if(not(hosting)),
                    sync::capture,
                    crate::enemies::sync_forms,
                    drones::spawn_drones,
                    animate,
                    effects,
                    play_effects,
                    grenade::spawn_grenades,
                    grenade::fly_grenades,
                    projectile::spawn_projectiles,
                    projectile::fly_projectiles,
                )
                    .chain()
                    .after(crate::waves::WaveSystems)
                    .after(crate::melee_sweeps)
                    .before(crate::net_capture)
                    .run_if(in_state(crate::menu::AppState::Game)),
            )
            .add_systems(
                PostUpdate,
                aimoffset::apply_aim_offsets
                    .after(bevy::app::AnimationSystems)
                    .before(crate::ragdoll::drive_rigs)
                    .before(bevy::transform::TransformSystems::Propagate)
                    .run_if(in_state(crate::menu::AppState::Game)),
            )
            .add_systems(
                PostUpdate,
                (
                    read_sockets,
                    carry_effects,
                    jet::plane_effects,
                    scanner::draw_beams,
                    drones::draw_beams,
                    laser::draw_lasers,
                )
                    .chain()
                    .after(bevy::transform::TransformSystems::Propagate)
                    .before(crate::levelfx::SpawnedFxRun)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every Swarmer comes in with an animation: a point's own where its clips
    /// have it, its climb out of the ground where the point asks for one they lack
    /// (Ancients' alcove's campaign cinematic); none where none is asked for, nor
    /// for an enemy without a ground spawn of its own.
    #[test]
    fn every_swarmer_climbs_in() {
        let dir = crate::assets_dir();
        let (Some(swarmer), Some(soldier)) = (
            clips::Clips::load(&dir, "swarmer"),
            clips::Clips::load(&dir, "soldier"),
        ) else {
            eprintln!("the clips not built: skipped");
            return;
        };
        let cine = "CINE_ReadytoRide_s01_Swarmer";
        assert!(swarmer.find(cine).is_none());
        assert_eq!(spawn_animation(&swarmer, Some(cine)), Some(GROUND_SPAWN));
        assert_eq!(
            spawn_animation(&swarmer, Some("Nav_Spawn_Ground")),
            Some("Nav_Spawn_Ground")
        );
        assert_eq!(spawn_animation(&swarmer, None), None);
        assert!(soldier.find(GROUND_SPAWN).is_none());
        assert_eq!(spawn_animation(&soldier, Some(cine)), None);
        // (Ancients' points: each Swarmer cluster's asks for one.)
        let Ok(text) = std::fs::read_to_string(dir.join("levels/ancients_waves.txt")) else {
            return;
        };
        let d = crate::waves::WaveData::parse(&text);
        for c in d.clusters.iter().filter(|c| c.request == 2) {
            for p in &c.points {
                let asked = p
                    .anim
                    .and_then(|i| d.spawn_anims.get(i as usize))
                    .map(|a| &**a);
                assert!(
                    spawn_animation(&swarmer, asked).is_some(),
                    "{} at {:?} asks for {asked:?}",
                    c.name,
                    p.at
                );
            }
        }
    }

    /// Every AI gun's sounds (its shot, its impact, its round flying past a player,
    /// its loops, its reload's and its charge's) are in the sound bank: the
    /// Snipers' fly-by, BY_SNIPER, among them.
    #[test]
    fn the_guns_sounds_are_in_the_bank() {
        let Ok(bank) = std::fs::read_to_string(crate::assets_dir().join("sounds/sounds.txt"))
        else {
            return;
        };
        let cues: std::collections::HashSet<&str> = bank
            .lines()
            .filter_map(|l| l.strip_prefix("cue "))
            .filter_map(|l| l.split_whitespace().next())
            .collect();
        for (class, fx) in GUN_FX {
            for cue in [
                fx.shoot,
                fx.impact_sound,
                fx.bullet_by,
                fx.shoot_loop,
                fx.tail,
                fx.equip,
                fx.holster,
                fx.reload_sound,
                fx.charge_loop,
            ] {
                assert!(
                    cue.is_empty() || cues.contains(cue),
                    "{class}: {cue} not in the bank"
                );
            }
        }
        assert!(cues.contains(gun_fx("TnWeaponSniper").bullet_by));
    }

    /// An animation's effect plays on its socket, not at all on a model without
    /// it (the Scrappers' fist attack's Leaper boosters, which had hung in the air
    /// as a lilac orb), and at the pawn's place until its sockets are read.
    /// An animation's effect turned off with its clip goes off on time, and one
    /// held on a socket where the same one is still on isn't started again (the
    /// Leaper's booster glow, with each hit it takes); one the game leaves on
    /// stays on.
    #[test]
    fn effects_turned_off_with_their_clips_go_off_and_dont_pile_up() {
        use bevy::ecs::system::RunSystemOnce;
        if !crate::assets_dir().join("fx/weapons_fx.txt").exists() {
            return;
        }
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(AiOutputs::default());
        world.insert_resource(crate::levelfx::FxSystems::default());
        world.insert_resource(SocketFx::default());
        world.insert_resource(EffectsOff::default());
        world.insert_resource(crate::enemies::Kinds::default());
        let leaper = world.spawn_empty().id();
        let glow = |socket: &str, auto_off: Option<f32>, attached: bool| shared::Effect {
            by: leaper,
            system: Arc::from("Leaper_Boosters_LowBurn_FX"),
            socket: Arc::from(socket),
            at: Vec3::ZERO,
            axes: [Vec3::X, Vec3::Y, Vec3::Z],
            offset: Vec3::ZERO,
            turn: Vec3::ZERO,
            attached,
            only_once: false,
            restart: false,
            params: None,
            auto_off,
        };
        let frame = |world: &mut World, effects: Vec<shared::Effect>| {
            world.resource_mut::<AiOutputs>().effects.extend(effects);
            world
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(100));
            world.run_system_once(play_effects).unwrap();
        };
        let on = |world: &World| {
            let fx = world.resource::<crate::levelfx::FxSystems>();
            world
                .resource::<SocketFx>()
                .0
                .iter()
                .filter(|c| fx.is_on(c.id))
                .count()
        };
        // Hit after hit, a tenth of a second apart: its four sockets' glow, each
        // for half a second.
        let sockets = [
            "FX_Booster_Back_R",
            "FX_Booster_Back_L",
            "FX_Booster_Elbow_R",
            "FX_Booster_Elbow_L",
        ];
        for _ in 0..20 {
            frame(
                &mut world,
                sockets.iter().map(|s| glow(s, Some(0.5), true)).collect(),
            );
            assert_eq!(on(&world), 4);
        }
        assert_eq!(world.resource::<SocketFx>().0.len(), 4);
        // The hits over: off half a second after the last.
        for _ in 0..4 {
            frame(&mut world, Vec::new());
        }
        assert_eq!(on(&world), 4);
        frame(&mut world, Vec::new());
        frame(&mut world, Vec::new());
        assert_eq!(on(&world), 0);
        // Left where it starts, each one starts and goes off by itself.
        let loose: Vec<_> = (0..3)
            .map(|_| glow("FX_Booster_Back_R", Some(0.15), false))
            .collect();
        frame(&mut world, loose);
        assert_eq!(world.resource::<EffectsOff>().0.len(), 3);
        frame(&mut world, Vec::new());
        frame(&mut world, Vec::new());
        assert!(world.resource::<EffectsOff>().0.is_empty());
        // One the game leaves on (no auto off) stays on.
        frame(&mut world, vec![glow("FX_Booster_Back_R", None, true)]);
        for _ in 0..30 {
            frame(&mut world, Vec::new());
        }
        assert_eq!(on(&world), 1);
    }

    #[test]
    fn effects_on_sockets_a_model_lacks_dont_play() {
        let mut p = pawn::Pawn::new(
            Entity::PLACEHOLDER,
            Vec3::new(0.0, 0.0, 100.0),
            0.0,
            150.0,
            220.0,
            100.0,
        );
        // (Its sockets not read yet: its place.)
        let (at, _) = effect_frame(&p, "FX_Booster_Back_R").expect("its place");
        assert_eq!(at, p.loc);
        p.sockets.push(pawn::PawnSocket {
            name: Arc::from("ReactionSocket_Chest"),
            at: Vec3::new(10.0, 0.0, 150.0),
            axes: [Vec3::X, Vec3::Y, Vec3::Z],
        });
        let (at, _) = effect_frame(&p, "ReactionSocket_Chest").expect("its socket");
        assert!(at.distance(p.feet() + Vec3::new(10.0, 0.0, 150.0)) < 1e-3);
        assert!(effect_frame(&p, "FX_Booster_Back_R").is_none());
        // The Scrappers' own data: their fist attack's boosters and arm trails are
        // on sockets their meshes haven't (where the data is built).
        let dir = crate::assets_dir();
        let Some(clips) = clips::Clips::load(&dir, "scrapper") else {
            eprintln!("the scrappers' clips not built: skipped");
            return;
        };
        let Ok(text) = std::fs::read_to_string(dir.join("models/enemies/enemies.txt")) else {
            eprintln!("enemies.txt not built: skipped");
            return;
        };
        let sockets: Vec<&str> = text
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f.len() > 2 && f[0] == "socket" && f[1] == "ScrapperAut").then(|| f[2])
            })
            .collect();
        assert!(sockets.contains(&"ReactionSocket_Chest"));
        let lacking: Vec<&str> = anim::ClipBook::notifies(&clips, "Melee_FistAttack")
            .iter()
            .filter_map(|n| match n {
                ClipNotify::Effect { socket, .. } => Some(&**socket),
                _ => None,
            })
            .filter(|s| !sockets.contains(s))
            .collect();
        assert_eq!(lacking.len(), 6, "{lacking:?}");
    }

    /// A player's pawn standing at `feet` (Unreal space).
    fn standing_player(entity: Entity, feet: Vec3) -> PlayerView {
        PlayerView {
            entity,
            loc: feet + Vec3::Z * 110.0,
            feet,
            vel: Vec3::ZERO,
            yaw: 0.0,
            radius: 50.0,
            half_height: 110.0,
            health: 200.0,
            health_max: 200.0,
            alive: true,
            downed: false,
            cloaked: false,
            form: 1,
            airborne: false,
            jumping: false,
            physics: 1,
            transforming: false,
            shielded: false,
            eyes: feet + Vec3::Z * 198.0,
            nav_near: feet,
            nav_near_time: 0.0,
            team: world::PLAYER_TEAM,
            weakened: false,
            dodge: None,
            body: None,
        }
    }

    /// What came of a fight (fight_on_ignition).
    struct Fight {
        shots: usize,
        launches: usize,
        damage: f32,
        moved: bool,
        took_enemy: bool,
        /// The tasks each one ran (by name).
        ran: Vec<std::collections::HashSet<String>>,
        /// The effects their animations and guns played (by system), the held ones
        /// they stopped, and the most rounds any fired in a row without charging.
        effects: Vec<String>,
        effects_stopped: usize,
        /// The forms each one was in (the Guard's), in order.
        forms: Vec<Vec<i32>>,
    }

    /// A fight on Ignition: `n` of a type brought in on cells `near` (uu) from a
    /// player standing at the map's start, run for `secs` (none if the level, its
    /// navigation or the AI data isn't built).
    fn fight_on_ignition(
        id: &str,
        n: usize,
        near: std::ops::Range<f32>,
        secs: f32,
    ) -> Option<Fight> {
        fight_on_ignition_as(id, n, near, secs, false)
    }

    /// The same, the player's Flak Shield up if `shielded`.
    fn fight_on_ignition_as(
        id: &str,
        n: usize,
        near: std::ops::Range<f32>,
        secs: f32,
        shielded: bool,
    ) -> Option<Fight> {
        fight_on_ignition_with(
            id,
            n,
            near,
            secs,
            Setup {
                shielded,
                ..Default::default()
            },
        )
    }

    /// How a fight is set up: the map (Ignition for none), the player's Flak
    /// Shield up, where he stands (his feet, Unreal space; the map's start for
    /// none) and whether those brought in stand wholly inside a smash volume (the
    /// Leapers', to leap from).
    #[derive(Default)]
    struct Setup {
        map: Option<&'static str>,
        shielded: bool,
        player_at: Option<Vec3>,
        in_leap_volume: bool,
        /// Only cells with a way to him (none behind the doors still locked).
        reachable: bool,
    }

    /// The same, set up so.
    fn fight_on_ignition_with(
        id: &str,
        n: usize,
        near: std::ops::Range<f32>,
        secs: f32,
        setup: Setup,
    ) -> Option<Fight> {
        let shielded = setup.shielded;
        let dir = crate::assets_dir();
        let stem = setup.map.unwrap_or("ignition");
        let map = crate::MAPS.iter().find(|m| m.stem == stem)?;
        let (Some(level), Some(nav)) = (crate::load_level_data(map), NavMesh::load(&dir, stem))
        else {
            eprintln!("{stem}'s level or navigation not built: skipped");
            return None;
        };
        let data = AiData::load(&dir);
        let Some(b) = data.behaviour(id) else {
            eprintln!("ai.txt not built: skipped");
            return None;
        };
        // Its type's body (enemies.txt's), else a soldier's.
        let kinds = crate::enemies::Kinds::load(&dir);
        let (radius, half_height, mesh_scale) =
            kinds
                .kinds
                .iter()
                .find(|k| k.id == id)
                .map_or((150.0, 220.0, 0.8), |k| {
                    (
                        k.radius * ue::UU_PER_M,
                        k.half_height * ue::UU_PER_M,
                        k.scale,
                    )
                });
        let mut ecs = bevy::ecs::world::World::new();
        let mut world = AiWorld {
            data: Arc::new(data.clone()),
            loaded: true,
            ..Default::default()
        };
        world.shared.tactical.reset(Some(&nav));
        let player = ecs.spawn_empty().id();
        // (Stood on the floor under the place: the cells are over it.)
        let grounded = |at: Vec3, radius: f32| {
            let floor = WorldView {
                level: Some(&level),
                ..Default::default()
            }
            .ground_at(at, radius, pawn::MAX_STEP_UU, 300.0);
            floor.map_or(at, |z| Vec3::new(at.x, at.y, z + pawn::FLOOR_DIST))
        };
        let start = grounded(
            setup.player_at.unwrap_or(ue::to_ue(level.player_start)),
            50.0,
        );
        let mut view = WorldView {
            now: 0.0,
            players: vec![PlayerView {
                shielded,
                ..standing_player(player, start)
            }],
            ais: Vec::new(),
            nav: Some(&nav),
            level: Some(&level),
            barriers: Vec::new(),
        };
        // On cells the distance asked from him, spread round him.
        let mut spots: Vec<Vec3> = Vec::new();
        for (i, p) in nav.points.iter().enumerate() {
            if !matches!(p.kind, crate::nav::PointKind::Cell) {
                continue;
            }
            let at = grounded(ue::to_ue(p.at), radius);
            let d = ue::vsize2d(at - start);
            if !near.contains(&d) || (at.z - start.z).abs() > 300.0 {
                continue;
            }
            if setup.reachable
                && nav
                    .find_channel(
                        i as u32,
                        ue::from_ue(start),
                        &crate::nav::Moves::ground(
                            radius / ue::UU_PER_M,
                            2.0 * half_height / ue::UU_PER_M,
                        ),
                    )
                    .is_none()
            {
                continue;
            }
            if setup.in_leap_volume
                && !nav.leap_volumes.iter().any(|v| {
                    sensors::encompasses_box(
                        v,
                        at + Vec3::Z * half_height,
                        Vec3::new(radius, radius, half_height),
                    )
                })
            {
                continue;
            }
            if spots.iter().all(|s| ue::vsize2d(*s - at) > 1500.0) {
                spots.push(at);
            }
            if spots.len() == n {
                break;
            }
        }
        assert!(!spots.is_empty(), "no cells near the start");
        let mut ais = Vec::new();
        for (k, at) in spots.iter().enumerate() {
            let e = ecs.spawn_empty().id();
            let body = Body {
                feet: *at,
                yaw: ue::yaw_of(start - *at),
                radius,
                half_height,
                health: 150.0,
                mesh_scale,
                forms: None,
            };
            let ai = new_ai(&mut world, &view, e, 0, b.clone(), body, 7 + k as u64).expect("an AI");
            ais.push((e, ai));
        }
        let d = b.domain.clone().unwrap();
        let dt = 1.0 / 30.0;
        let mut damage = 0.0;
        let mut shots = 0;
        let mut launches = 0;
        let mut alert_left = 0.0;
        let mut last: Vec<(Vec<i32>, Option<u32>, usize)> = Vec::new();
        let mut ran = vec![std::collections::HashSet::new(); ais.len()];
        let mut effects = Vec::new();
        let mut effects_stopped = 0;
        let mut forms = vec![Vec::new(); ais.len()];
        for frame in 0..(30.0 * secs) as usize {
            world.shared.now += dt;
            view.now = world.shared.now;
            alert_left -= dt;
            let alerting = (alert_left <= 0.0).then(|| (player, view.players[0].loc));
            if alerting.is_some() {
                alert_left = ALERT_SECS;
            }
            let mut slots: Vec<Slot> = ais
                .iter_mut()
                .map(|(e, ai)| Slot {
                    entity: *e,
                    ai,
                    dormant: false,
                })
                .collect();
            view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
            let sounds = world.sounds.clone();
            run_frame(
                &mut world.shared,
                &sounds,
                &mut view,
                &mut slots,
                dt,
                alerting,
            );
            shots += world.shared.shots.iter().filter(|s| s.traced).count();
            world.shared.shots.clear();
            launches += world.shared.launches.len();
            world.shared.launches.clear();
            world.shared.loops.clear();
            for h in world.shared.player_damage.drain(..) {
                damage += h.amount;
            }
            effects.extend(world.shared.effects.drain(..).map(|e| e.system.to_string()));
            effects_stopped += world.shared.effects_stopped.drain(..).count();
            for (k, (_, ai)) in ais.iter().enumerate() {
                if let Some(op) = ai.brain.runner.active.as_ref() {
                    ran[k].insert(d.name(op.task.symbol).to_string());
                }
                let form = guard::form(&ai.brain);
                if forms[k].last() != Some(&form) {
                    forms[k].push(form);
                }
            }
            // Every plan change, and every new tactical node.
            for (k, (e, ai)) in ais.iter().enumerate() {
                let plan: Vec<i32> = ai.brain.runner.plan.iter().map(|t| t.symbol).collect();
                let tac = ai.brain.sensors.tactical.as_ref().and_then(|t| t.tactical);
                let key = (plan.clone(), tac, ai.brain.runner.history.len());
                if last.get(k) != Some(&key) {
                    let names: Vec<&str> = plan.iter().map(|s| d.name(*s)).collect();
                    let hist: Vec<String> = ai
                        .brain
                        .runner
                        .history
                        .iter()
                        .rev()
                        .take(1)
                        .map(|(g, p, _, st)| {
                            format!(
                                "{} {:?} -> {}",
                                d.name(*g),
                                p.iter().map(|s| d.name(*s)).collect::<Vec<_>>(),
                                st
                            )
                        })
                        .collect();
                    eprintln!(
                        "  {:5.2} {:?} plan {:?} tac {:?} ({}) last {:?} loc {} cover {} ap {}/{}",
                        world.shared.now,
                        e,
                        names,
                        tac,
                        ai.brain.sensors.tactical.as_ref().map_or("", |t| t.reason),
                        hist,
                        ai.brain.core.ws(htn::WS_LOCATION),
                        ai.brain.core.ws(htn::WS_COVER_USAGE),
                        ai.brain.core.ws(htn::WS_ATTACK_POINT_AVAILABLE),
                        ai.brain.core.ws_ctx(htn::WS_ATTACK_POINT_AVAILABLE),
                    );
                    if last.len() <= k {
                        last.push(key);
                    } else {
                        last[k] = key;
                    }
                }
            }
            if frame % 30 == 0 {
                // What each one's model plays.
                for (e, ai) in &ais {
                    let mut pose: Vec<_> = ai
                        .pawn
                        .pose
                        .clips
                        .iter()
                        .filter(|c| c.weight > 0.05)
                        .collect();
                    pose.sort_by(|a, b| {
                        (a.layer as u8, -a.weight)
                            .partial_cmp(&(b.layer as u8, -b.weight))
                            .unwrap()
                    });
                    let list: Vec<String> = pose
                        .iter()
                        .map(|c| {
                            format!(
                                "{:?}:{}@{:.2}x{:.2}",
                                c.layer, ai.clips.clips[c.clip as usize].name, c.time, c.weight
                            )
                        })
                        .collect();
                    eprintln!(
                        "     {:?} stance {} (gun {}) pose [{}] shares {:?} mirror {:.1}",
                        e,
                        ai.pawn.current_stance(),
                        ai.pawn.weapon.equipped,
                        list.join(" "),
                        ai.pawn.pose.shares.map(|x| (x * 100.0).round() / 100.0),
                        ai.pawn.pose.mirror
                    );
                }
                for (e, ai) in &ais {
                    let c = &ai.brain.core;
                    let plan: Vec<&str> = ai
                        .brain
                        .runner
                        .plan
                        .iter()
                        .map(|t| d.name(t.symbol))
                        .collect();
                    eprintln!(
                        "t={:4.1} {:?} at ({:6.0},{:6.0},{:5.0}) alert {} aware {} visible {} range {} attack point {} \
                         location {} cover {} enemy {:?} plan {:?} #{}",
                        world.shared.now,
                        e,
                        ai.pawn.loc.x,
                        ai.pawn.loc.y,
                        ai.pawn.loc.z,
                        c.ws(htn::WS_ALERT_LEVEL),
                        c.ws(htn::WS_ALERT_TO_ENEMY_PRESENCE),
                        c.ws(htn::WS_ENEMY_VISIBLE),
                        c.ws(htn::WS_ENEMY_RANGE),
                        c.ws(htn::WS_ATTACK_POINT_AVAILABLE),
                        c.ws(htn::WS_LOCATION),
                        c.ws(htn::WS_COVER_USAGE),
                        c.enemy.is_some(),
                        plan,
                        ai.brain.runner.index,
                    );
                }
                for (e, ai) in &ais {
                    let p = &ai.pawn;
                    let pf = ai.brain.sensors.path.as_ref();
                    eprintln!(
                        "     {:?} path {:?} channel {} motion {:?} dest ({:.0},{:.0}) speed {:.0} vel {:.0} accel {:?} op {:?} beh {:#x} anchor {:?} phys {:?}",
                        e,
                        pf.map(|f| f.state),
                        pf.map_or(0, |f| f.channel.len()),
                        p.motion.state,
                        p.motion.destination.x,
                        p.motion.destination.y,
                        p.ground_speed,
                        p.vel.length(),
                        p.accel,
                        ai.brain
                            .runner
                            .active
                            .as_ref()
                            .map(|o| o.task.symbol)
                            .map(|s| d.name(s)),
                        ai.brain.core.behavior,
                        p.anchor,
                        p.physics,
                    );
                }
                eprintln!("   shots {shots}, damage to the player {damage:.1}");
            }
        }
        let moved = ais
            .iter()
            .zip(&spots)
            .any(|((_, ai), at)| ue::vsize2d(ai.pawn.loc - *at) > 300.0);
        let took_enemy = ais
            .iter()
            .any(|(_, ai)| ai.brain.core.enemy == Some(player));
        Some(Fight {
            shots,
            launches,
            damage,
            moved,
            took_enemy,
            ran,
            effects,
            effects_stopped,
            forms,
        })
    }

    /// Ignition's Decepticon soldiers, brought in 25 to 45 m from a player standing
    /// at the map's start: they hear of him (the alerter), come for him, take
    /// their places, and shoot.
    #[test]
    fn soldiers_fight_on_ignition() {
        let Some(f) = fight_on_ignition("SoldierDec_Kaon", 3, 2500.0..4500.0, 40.0) else {
            return;
        };
        assert!(f.moved, "no soldier moved");
        assert!(f.took_enemy, "none took the player for its enemy");
        assert!(f.shots > 0, "none fired");
    }

    /// Shotgunners brought in 15 to 30 m from him: they close in and fire their
    /// shotguns (eight pellets a round), stepping aside and dashing back.
    #[test]
    fn shotgunners_fight_on_ignition() {
        let Some(f) = fight_on_ignition("ShotgunnerDec", 2, 1500.0..3000.0, 30.0) else {
            return;
        };
        eprintln!("shotgunners ran {:?}", f.ran);
        assert!(f.took_enemy, "none took the player for its enemy");
        assert!(f.shots > 0, "none fired");
        assert_eq!(f.shots % 8, 0, "a round's pellets are 8");
        assert!(f.damage > 0.0, "no pellet struck him");
        assert!(f.moved, "none moved");
    }

    /// Rocket troopers brought in 25 to 45 m from him: they take their launchers in
    /// hand (EquipWeapon) and fire rockets at him.
    #[test]
    fn rocket_troopers_fight_on_ignition() {
        let Some(f) = fight_on_ignition("RocketTrooper_DEC", 2, 2500.0..4500.0, 40.0) else {
            return;
        };
        eprintln!("rocket troopers ran {:?}", f.ran);
        assert!(f.took_enemy, "none took the player for its enemy");
        assert!(
            f.ran.iter().any(|r| r.contains("RTTS_EquipWeapon")),
            "none took its launcher in hand"
        );
        assert!(f.launches > 0, "none fired a rocket");
        assert_eq!(f.shots, 0, "its rounds are rockets, not traces");
    }

    /// Sharpshooters brought in 25 to 45 m from him: they cloak, scan him with
    /// their laser sights and fire.
    #[test]
    fn sharpshooters_fight_on_ignition() {
        let Some(f) = fight_on_ignition("SharpShooterAut", 2, 2500.0..4500.0, 40.0) else {
            return;
        };
        eprintln!("sharpshooters ran {:?}", f.ran);
        assert!(f.took_enemy, "none took the player for its enemy");
        assert!(
            f.ran
                .iter()
                .any(|r| r.contains("SHARPSHOOTER_CloakAbility")),
            "none cloaked"
        );
        assert!(f.shots > 0, "none fired");
    }

    /// Scrappers brought in 20 to 40 m from him close in on him, take their turns
    /// at him (WS_SquadCommand) and strike him.
    #[test]
    fn scrappers_fight_on_ignition() {
        let Some(f) = fight_on_ignition("ScrapperDec", 3, 2000.0..4000.0, 30.0) else {
            return;
        };
        eprintln!("scrappers ran {:?}", f.ran);
        assert!(f.took_enemy, "none took the player for its enemy");
        assert!(
            f.ran.iter().any(|r| r.contains("SCRAPPER_MeleeEnemy")),
            "none attacked"
        );
        assert!(f.damage > 0.0, "none struck him");
    }

    /// Swarmers brought in on Ancients 10 to 30 m from him (where they can reach
    /// him: not behind the start's locked doors) come at him as a squad
    /// (their kung fu circle, each one's turn in the attack channel: SwarmerLite's
    /// no wait between turns) and bite (SWTS_MeleeEnemy: their insect form's melee
    /// set, its blows at 0.14 of their damage: TnBuffModifySwarmerMeleeDamage), a
    /// strafing run as they go.
    #[test]
    fn swarmers_swarm_on_ancients() {
        let Some(f) = fight_on_ignition_with(
            "SwarmerLite",
            4,
            1000.0..3000.0,
            30.0,
            Setup {
                map: Some("ancients"),
                reachable: true,
                ..Default::default()
            },
        ) else {
            return;
        };
        eprintln!("swarmers ran {:?}, damage {}", f.ran, f.damage);
        assert!(f.took_enemy, "none took the player for its enemy");
        assert!(f.moved, "none moved");
        assert!(
            f.ran.iter().any(|r| r.contains("SWTS_MeleeEnemy")),
            "none attacked"
        );
        assert!(f.damage > 0.0, "none bit him");
        // (A bite's 150 at 0.14.)
        assert!(
            f.damage < 150.0 * 0.14 * 60.0,
            "they bit too hard: {}",
            f.damage
        );
    }

    /// What came of a Titan's fight (titan_fight): the damage to the player, the
    /// drones it sent out (their types), the dialog asked for, the tasks the
    /// Titan ran, the effects played (by system) and stopped, the loops held and
    /// the deaths the script dealt.
    struct TitanFight {
        damage: f32,
        sent: Vec<String>,
        dialog: Vec<String>,
        ran: std::collections::HashSet<String>,
        effects: Vec<String>,
        stopped: Vec<String>,
        loops: std::collections::HashSet<&'static str>,
        kills: Vec<crate::enemies::Death>,
        /// Where the Titan's traced rounds left from, in its own frame (from its
        /// feet, its facing along x) as the frame ended, and whether each showed
        /// its tracer.
        rounds: Vec<(Vec3, bool)>,
        /// Its effects started on sockets: the effect, the socket, and whether it
        /// starts its last over.
        socket_effects: Vec<(String, String, bool)>,
    }

    /// A Titan on Downfall (its map) 15 to 30 m from a player at the map's start,
    /// `player_up` over the floor (out of its reach, up there), run `secs`: its
    /// drones brought in as it sends them out (their AIs taken up as the game's
    /// attach does, flung out: DroneLaunch), the dead gone (PawnDied), the deaths
    /// the script dealt (its kills) dealt; `hook` each frame (the time, the AIs,
    /// the AI's world). None if the map or the AI data isn't built.
    fn titan_fight(
        player_up: f32,
        secs: f32,
        mut hook: impl FnMut(f32, &mut Vec<(Entity, Ai)>, &mut AiWorld),
    ) -> Option<TitanFight> {
        let dir = crate::assets_dir();
        let map = crate::MAPS.iter().find(|m| m.stem == "downfall")?;
        let (Some(level), Some(nav)) =
            (crate::load_level_data(map), NavMesh::load(&dir, "downfall"))
        else {
            eprintln!("downfall's level or navigation not built: skipped");
            return None;
        };
        let data = AiData::load(&dir);
        let (Some(titan), Some(_)) = (data.behaviour("Titan"), data.behaviour("SupportDrone"))
        else {
            eprintln!("ai.txt not built: skipped");
            return None;
        };
        let kinds = crate::enemies::Kinds::load(&dir);
        let body_of = |id: &str, feet: Vec3, yaw: f32| {
            let k = kinds.kinds.iter().find(|k| k.id == id).expect("its kind");
            Body {
                feet,
                yaw,
                radius: k.radius * ue::UU_PER_M,
                half_height: k.half_height * ue::UU_PER_M,
                health: k.health,
                mesh_scale: k.scale,
                forms: forms_of(&kinds, k),
            }
        };
        if clips::for_type("SupportDrone").is_none() {
            eprintln!("the drones' clips aren't built: skipped");
            return None;
        }
        let mut ecs = bevy::ecs::world::World::new();
        let mut world = AiWorld {
            data: Arc::new(data.clone()),
            sounds: Arc::new(audio::AiSounds::load(&dir)),
            loaded: true,
            ..Default::default()
        };
        world.shared.tactical.reset(Some(&nav));
        let floor = |at: Vec3, radius: f32| {
            WorldView {
                level: Some(&level),
                ..Default::default()
            }
            .ground_at(at, radius, pawn::MAX_STEP_UU, 300.0)
            .map_or(at, |z| Vec3::new(at.x, at.y, z + pawn::FLOOR_DIST))
        };
        let start = floor(ue::to_ue(level.player_start), 50.0);
        let player = ecs.spawn_empty().id();
        let feet = start + Vec3::Z * player_up;
        let mut view = WorldView {
            players: vec![standing_player(player, feet)],
            nav: Some(&nav),
            level: Some(&level),
            ..Default::default()
        };
        let probe = body_of("Titan", Vec3::ZERO, 0.0);
        let moves = crate::nav::Moves::ground(
            probe.radius / ue::UU_PER_M,
            2.0 * probe.half_height / ue::UU_PER_M,
        );
        let spot = nav
            .points
            .iter()
            .enumerate()
            .filter(|(_, p)| matches!(p.kind, crate::nav::PointKind::Cell))
            .map(|(i, p)| (i, floor(ue::to_ue(p.at), probe.radius)))
            .find(|(i, at)| {
                (1500.0..3000.0).contains(&ue::vsize2d(*at - start))
                    && (at.z - start.z).abs() < 300.0
                    && nav
                        .find_channel(*i as u32, ue::from_ue(start), &moves)
                        .is_some()
            })
            .map(|(_, at)| at)
            .expect("a cell for the Titan");
        let t = ecs.spawn_empty().id();
        let ai = new_ai(
            &mut world,
            &view,
            t,
            0,
            titan.clone(),
            body_of("Titan", spot, ue::yaw_of(start - spot)),
            11,
        )
        .expect("the Titan's AI");
        let mut ais = vec![(t, ai)];
        let mut out = TitanFight {
            damage: 0.0,
            sent: Vec::new(),
            dialog: Vec::new(),
            ran: Default::default(),
            effects: Vec::new(),
            stopped: Vec::new(),
            loops: Default::default(),
            kills: Vec::new(),
            rounds: Vec::new(),
            socket_effects: Vec::new(),
        };
        let dt = 1.0 / 30.0;
        let mut sent_before = 0;
        for _ in 0..(30.0 * secs) as usize {
            world.shared.now += dt;
            view.now = world.shared.now;
            let mut slots: Vec<Slot> = ais
                .iter_mut()
                .map(|(e, ai)| Slot {
                    entity: *e,
                    ai,
                    dormant: false,
                })
                .collect();
            view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
            let sounds = world.sounds.clone();
            run_frame(&mut world.shared, &sounds, &mut view, &mut slots, dt, None);
            if let Some((_, ai)) = ais.iter().find(|(e, _)| *e == t) {
                let (feet, yaw) = (ai.pawn.feet(), ai.pawn.yaw);
                out.rounds.extend(
                    world
                        .shared
                        .shots
                        .iter()
                        .filter(|s| s.shooter == t && s.traced)
                        .map(|s| (ue::rotate_yaw(s.from - feet, -yaw), s.tracer)),
                );
            }
            world.shared.shots.clear();
            world.shared.launches.clear();
            world.shared.sounds.clear();
            out.loops
                .extend(world.shared.loops.drain(..).map(|(_, cue, ..)| cue));
            out.socket_effects.extend(
                world
                    .shared
                    .effects
                    .iter()
                    .filter(|e| e.by == t)
                    .map(|e| (e.system.to_string(), e.socket.to_string(), e.restart)),
            );
            out.effects
                .extend(world.shared.effects.drain(..).map(|e| e.system.to_string()));
            out.stopped.extend(
                world
                    .shared
                    .effects_stopped
                    .drain(..)
                    .map(|(_, s)| s.to_string()),
            );
            for h in world.shared.player_damage.drain(..) {
                out.damage += h.amount;
            }
            for r in world.shared.dialog.drain(..) {
                if let audio::DialogRequest::Say { event, .. } = r {
                    out.dialog.push(event.to_string());
                }
            }
            if let Some((_, ai)) = ais.iter().find(|(e, _)| *e == t)
                && let Some(op) = ai.brain.runner.active.as_ref()
            {
                out.ran
                    .insert(ai.brain.core.d.name(op.task.symbol).to_string());
            }
            // The script's deaths, and the dead gone.
            for (e, death) in std::mem::take(&mut world.shared.kills) {
                if let Some((_, ai)) = ais.iter_mut().find(|(x, _)| *x == e) {
                    ai.pawn.health = 0.0;
                    out.kills.push(death);
                }
            }
            for (e, ai) in ais.iter_mut() {
                if ai.pawn.health <= 0.0 {
                    pawn_died(*e, ai, &mut world.shared, &view);
                }
            }
            ais.retain(|(_, ai)| !ai.pawn.dying);
            // The drones asked for, come in.
            for s in std::mem::take(&mut world.shared.drone_spawns) {
                let b = world.data.behaviour(&s.kind).expect("the drone's type");
                let e = ecs.spawn_empty().id();
                let k = body_of(&s.kind, Vec3::ZERO, s.yaw);
                let body = Body {
                    feet: s.at - Vec3::Z * k.half_height,
                    ..k
                };
                let mut ai = new_ai(
                    &mut world,
                    &view,
                    e,
                    1,
                    b,
                    body,
                    100 + out.sent.len() as u64,
                )
                .expect("the drone's AI");
                let launch = drones::DroneLaunch {
                    owner: s.owner,
                    velocity: s.velocity,
                    enemy: s.enemy,
                };
                let Ai { brain, pawn, .. } = &mut ai;
                drones::launch(brain, pawn, &view, &launch);
                world.shared.drones_spawned.push((s.owner, Some(e)));
                out.sent.push(s.kind.clone());
                ais.push((e, ai));
            }
            hook(world.shared.now, &mut ais, &mut world);
            // (Each one's state, a second apart.)
            let now = world.shared.now;
            if ((now / dt).round() as i64) % 30 == 0 {
                for (e, ai) in &ais {
                    let b = &ai.brain;
                    let p = &ai.pawn;
                    let plan: Vec<&str> = b
                        .runner
                        .plan
                        .iter()
                        .map(|t| b.core.d.name(t.symbol))
                        .collect();
                    let dp = b.sensors.drone_path.as_ref();
                    eprintln!(
                        "t={now:4.1} {e:?} {} at ({:.0},{:.0},{:.0}) vel {:.0} phys {:?} health {:.0} plan {plan:?} nav {} loc {} blocked {:#x} post ({:.0},{:.0},{:.0}) path {:?} desired {:?} enemy {:?} visible {} range {}",
                        b.core.b.id,
                        p.loc.x,
                        p.loc.y,
                        p.loc.z,
                        p.vel.length(),
                        p.physics,
                        p.health,
                        b.core.ws(htn::WS_NAVIGATION),
                        b.core.ws(htn::WS_LOCATION),
                        b.core.ws(htn::WS_PATH_BLOCKED),
                        b.core.fetch(brain::LR_ATTACK_POINT).x,
                        b.core.fetch(brain::LR_ATTACK_POINT).y,
                        b.core.fetch(brain::LR_ATTACK_POINT).z,
                        dp.map(|d| d.state),
                        p.plane
                            .as_ref()
                            .map(|f| (f.desired_location.round(), f.moving)),
                        b.core.enemy.is_some(),
                        b.core.ws(htn::WS_ENEMY_VISIBLE),
                        b.core.ws(htn::WS_ENEMY_RANGE),
                    );
                }
            }
            for s in &out.sent[sent_before..] {
                eprintln!("t={:4.1} sent out a {s}", world.shared.now);
            }
            sent_before = out.sent.len();
        }
        Some(out)
    }

    /// What came of a Sniper's fight (sniper_fight): the tasks it ran (in order, each
    /// once as it began), its forms over time (when it took each), the effects
    /// played and stopped, the damage to the player and the shots fired.
    #[derive(Default)]
    struct SniperFight {
        ran: Vec<String>,
        forms: Vec<(f32, u8)>,
        hidden: Vec<(f32, [bool; 2])>,
        effects: Vec<String>,
        stopped: Vec<String>,
        damage: f32,
        shots: usize,
        on_wall: Option<(f32, Vec3, u32)>,
        claims: Vec<u32>,
        /// Its laser sight on at some time, and the furthest its aim offset node
        /// aimed it (its insect's).
        laser_on: bool,
        max_aim: f32,
    }

    /// A Sniper on Downfall coming in at the air spawn point nearest its start
    /// (its first flight's start), a player at the map's start; run `secs`, `hook`
    /// each frame.
    fn sniper_fight(
        secs: f32,
        mut hook: impl FnMut(f32, &mut Ai, &WorldView, &mut Vec3),
    ) -> Option<SniperFight> {
        let dir = crate::assets_dir();
        let map = crate::MAPS.iter().find(|m| m.stem == "downfall")?;
        let (Some(level), Some(nav)) =
            (crate::load_level_data(map), NavMesh::load(&dir, "downfall"))
        else {
            eprintln!("downfall's level or navigation not built: skipped");
            return None;
        };
        let data = AiData::load(&dir);
        let Some(sniper) = data.behaviour("Sniper_AUT") else {
            eprintln!("ai.txt not built: skipped");
            return None;
        };
        if clips::for_type("Sniper_AUT~other").is_none() {
            eprintln!("the Snipers' clips aren't built: skipped");
            return None;
        }
        let kinds = crate::enemies::Kinds::load(&dir);
        let k = kinds.kinds.iter().find(|k| k.id == "Sniper_AUT")?;
        let mut ecs = bevy::ecs::world::World::new();
        let mut world = AiWorld {
            data: Arc::new(data.clone()),
            sounds: Arc::new(audio::AiSounds::load(&dir)),
            loaded: true,
            ..Default::default()
        };
        world.shared.tactical.reset(Some(&nav));
        let floor = |at: Vec3, radius: f32| {
            WorldView {
                level: Some(&level),
                ..Default::default()
            }
            .ground_at(at, radius, pawn::MAX_STEP_UU, 300.0)
            .map_or(at, |z| Vec3::new(at.x, at.y, z + pawn::FLOOR_DIST))
        };
        let start = floor(ue::to_ue(level.player_start), 50.0);
        let player = ecs.spawn_empty().id();
        let mut player_at = start;
        let mut view = WorldView {
            players: vec![standing_player(player, start)],
            nav: Some(&nav),
            level: Some(&level),
            ..Default::default()
        };
        // (Its air spawn point: the nearest to the start.)
        let spawn = nav
            .points
            .iter()
            .filter(|p| {
                p.kind == crate::nav::PointKind::Air
                    && p.name.starts_with("TnSurvivalAirSpawnPoint")
            })
            .min_by(|a, b| {
                a.at.distance(ue::from_ue(start))
                    .total_cmp(&b.at.distance(ue::from_ue(start)))
            })
            .map(|p| p.at)
            .expect("an air spawn point");
        let e = ecs.spawn_empty().id();
        let body = Body {
            feet: ue::to_ue(spawn),
            yaw: ue::yaw_of(start - ue::to_ue(spawn)),
            radius: k.radius * ue::UU_PER_M,
            half_height: k.half_height * ue::UU_PER_M,
            health: k.health,
            mesh_scale: k.scale,
            forms: forms_of(&kinds, k),
        };
        let mut ai = new_ai(&mut world, &view, e, 0, sniper, body, 21).expect("the Sniper's AI");
        sniper_came_in(&mut ai, &view, spawn);
        assert!(
            ai.brain
                .sensors
                .sniper_store
                .as_ref()
                .unwrap()
                .spawn_point
                .is_some(),
            "its spawn point on the air network"
        );
        let mut out = SniperFight::default();
        let dt = 1.0 / 30.0;
        let mut last_task = String::new();
        let mut last_form = 0;
        let mut last_hidden = [false, true];
        for _ in 0..(30.0 * secs) as usize {
            world.shared.now += dt;
            view.now = world.shared.now;
            view.players[0] = standing_player(player, player_at);
            {
                let mut slots = vec![Slot {
                    entity: e,
                    ai: &mut ai,
                    dormant: false,
                }];
                view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
                let sounds = world.sounds.clone();
                run_frame(&mut world.shared, &sounds, &mut view, &mut slots, dt, None);
            }
            let now = world.shared.now;
            out.shots += world.shared.shots.len();
            world.shared.shots.clear();
            world.shared.launches.clear();
            world.shared.sounds.clear();
            world.shared.dialog.clear();
            world.shared.loops.clear();
            out.effects
                .extend(world.shared.effects.drain(..).map(|e| e.system.to_string()));
            out.stopped.extend(
                world
                    .shared
                    .effects_stopped
                    .drain(..)
                    .map(|(_, s)| s.to_string()),
            );
            for h in world.shared.player_damage.drain(..) {
                out.damage += h.amount;
            }
            if let Some(op) = ai.brain.runner.active.as_ref() {
                let name = ai.brain.core.d.name(op.task.symbol).to_string();
                if name != last_task {
                    eprintln!("t={now:4.1} {name}");
                    out.ran.push(name.clone());
                    last_task = name;
                }
            }
            if let Some(f) = ai.pawn.forms.as_ref() {
                if f.current != last_form {
                    out.forms.push((now, f.current));
                    last_form = f.current;
                }
                if f.hidden != last_hidden {
                    out.hidden.push((now, f.hidden));
                    last_hidden = f.hidden;
                }
            }
            let store = ai.brain.sensors.sniper_store.as_ref().unwrap();
            if out.on_wall.is_none()
                && ai.pawn.physics == pawn::Physics::Spider
                && let Some(c) = store.current
            {
                out.on_wall = Some((now, ai.pawn.loc, c));
            }
            for (p, c) in &world.shared.sniper_claims.0 {
                if c.claimed && !out.claims.contains(p) {
                    out.claims.push(*p);
                }
            }
            out.laser_on |= ai.pawn.laser.active;
            if let Some((_, aim)) = ai.pawn.pose.aim_offset {
                out.max_aim = out.max_aim.max(aim.length());
            }
            if ((now / dt).round() as i64) % 30 == 0 {
                let p = &ai.pawn;
                let b = &ai.brain;
                eprintln!(
                    "   ws form {} frustrated {} blocked {} balanced {} attack point {} alert {} plan {:?}",
                    b.core.ws(htn::WS_CURRENT_FORM),
                    b.core.ws(htn::WS_FRUSTRATED),
                    b.core.ws(htn::WS_PATH_BLOCKED),
                    b.core.ws(htn::WS_BALANCED),
                    b.core.ws(htn::WS_ATTACK_POINT_AVAILABLE),
                    b.core.ws(htn::WS_ALERT_LEVEL),
                    b.runner
                        .plan
                        .iter()
                        .map(|t| b.core.d.name(t.symbol))
                        .collect::<Vec<_>>(),
                );
                eprintln!(
                    "t={now:4.1} at ({:.0},{:.0},{:.0}) vel {:.0} phys {:?} rot ({:.0},{:.0},{:.0}) form {:?} path {:?} current {:?} enemy {:?} visible {} health {:.0} aim {:?} laser {}",
                    p.loc.x,
                    p.loc.y,
                    p.loc.z,
                    p.vel.length(),
                    p.physics,
                    p.pitch,
                    p.yaw,
                    p.roll,
                    p.forms.as_ref().map(|f| (f.current, f.change.is_some())),
                    store.path,
                    store.current,
                    b.core.enemy.is_some(),
                    b.core.ws(htn::WS_ENEMY_VISIBLE),
                    p.health,
                    p.pose.aim_offset,
                    p.laser.active,
                );
            }
            hook(now, &mut ai, &view, &mut player_at);
        }
        Some(out)
    }

    /// A Sniper searching for a player it can't see (him in Downfall's start hall)
    /// goes on from perch to perch for minutes: no flight lasts more than a few
    /// seconds, its acceleration stays its own (no motion controller to take it
    /// away: a perch left once had it flying on unsteered, and crawling through
    /// the air where it touched a wall), and it never perches beyond a locked door
    /// (the points Kismet blocks till the door is bought count as claimed:
    /// TnSniperPoint.get_IsClaimed is bBlocked).
    #[test]
    fn a_searching_sniper_never_hangs_in_the_air() {
        let mut flying_since: Option<f32> = None;
        let mut longest = 0.0f32;
        let mut perches = Vec::new();
        let mut accel_rates = Vec::new();
        let mut blocked_perch = None;
        let Some(_f) = sniper_fight(120.0, |now, ai, view, _player_at| {
            let flying = matches!(
                ai.brain.runner.active.as_ref().map(|o| &o.kind),
                Some(ops::Kind::FlyToSniperPoint(_))
            );
            match (flying, flying_since) {
                (true, None) => flying_since = Some(now),
                (true, Some(t)) => longest = longest.max(now - t),
                (false, _) => flying_since = None,
            }
            accel_rates.push(ai.pawn.accel_rate);
            if let Some(c) = ai
                .brain
                .sensors
                .sniper_store
                .as_ref()
                .and_then(|s| s.current)
                && perches.last() != Some(&c)
            {
                perches.push(c);
                if view.nav.is_some_and(|n| n.points[c as usize].blocked) {
                    blocked_perch = Some(c);
                }
            }
        }) else {
            return;
        };
        eprintln!("perches {perches:?}, longest flight {longest:.1} s");
        assert!(longest < 8.0, "a flight lasted {longest:.1} s");
        assert!(
            perches.len() >= 10,
            "it perched only {} times",
            perches.len()
        );
        assert!(
            accel_rates.iter().all(|a| *a > 0.0),
            "its acceleration was taken away"
        );
        assert!(
            blocked_perch.is_none(),
            "it perched on {blocked_perch:?}, beyond a locked door"
        );
    }

    /// A Sniper comes in at an air spawn point and flies (its robot form, its
    /// boosters lit) along the air network to a sniper point, takes its insect form
    /// onto the wall there (the robot hidden and the insect shown partway through the
    /// change), takes its gun in hand, and scans and shoots at the player.
    #[test]
    fn a_sniper_perches_and_shoots() {
        // (Once it is on its first perch, the player stands where it sees him:
        // the floor 20 to 40 m out in front of its wall.)
        let mut moved = false;
        let Some(f) = sniper_fight(30.0, |_, ai, view, player_at| {
            let Some(nav) = view.nav else { return };
            let Some(current) = ai
                .brain
                .sensors
                .sniper_store
                .as_ref()
                .and_then(|s| s.current)
                .and_then(|c| sniper::Point::get(nav, c))
            else {
                return;
            };
            if moved {
                return;
            }
            let eye = current.centre + current.normal * 200.0;
            let spot = nav
                .points
                .iter()
                .filter(|p| p.kind == crate::nav::PointKind::Cell)
                .map(|p| ue::to_ue(p.at) + Vec3::Z * pawn::FLOOR_DIST)
                .filter(|at| {
                    let d = *at - eye;
                    (2000.0..4000.0).contains(&d.length())
                        && d.normalize().dot(current.normal) > 0.3
                        && view.fast_trace(eye, *at + Vec3::Z * 150.0)
                })
                .min_by(|a, b| (*a - eye).length().total_cmp(&(*b - eye).length()));
            if let Some(at) = spot {
                eprintln!("the player stands at {at} (the perch's eye {eye})");
                *player_at = at;
                moved = true;
            }
        }) else {
            return;
        };
        eprintln!("ran {:?}", f.ran);
        eprintln!("forms {:?} hidden {:?}", f.forms, f.hidden);
        eprintln!(
            "effects {:?} stopped {:?} damage {} shots {} claims {:?} on wall {:?}",
            f.effects, f.stopped, f.damage, f.shots, f.claims, f.on_wall
        );
        let first = |t: &str| f.ran.iter().position(|r| r == t);
        let (Some(fly), Some(attach)) =
            (first("SNIPERTASK_FlyTo"), first("SNIPERTASK_AttachToWall"))
        else {
            panic!("it didn't fly to and attach to a point: {:?}", f.ran);
        };
        assert!(fly < attach);
        assert!(
            f.effects.iter().any(|e| e == "Boosters_Med_FX"),
            "no boosters"
        );
        assert!(
            f.stopped.iter().any(|e| e == "Boosters_Med_FX"),
            "boosters never out"
        );
        assert_eq!(
            f.forms.first().map(|x| x.1),
            Some(forms::ALT),
            "it never took its insect form"
        );
        assert!(f.on_wall.is_some(), "never on a wall");
        assert!(
            f.hidden.contains(&(f.hidden[0].0, [true, false]))
                || f.hidden.iter().any(|h| h.1 == [true, false])
        );
        assert!(
            f.ran.iter().any(|r| r == "SNIPERTASK_ScanEnemy"),
            "it never scanned for the player: {:?}",
            f.ran
        );
        // (Its rifle's 75 a round, a round each scan: a scan's lock-on and hold and
        // the shot's half second.)
        assert!(f.shots >= 3, "it fired {} shots", f.shots);
        assert!(f.damage >= 75.0 * 3.0, "it did {} damage", f.damage);
        // (Its laser sight on as it scans, its insect turned to aim at him.)
        assert!(f.laser_on, "its laser sight never came on");
        assert!(f.max_aim > 0.05, "its insect never aimed: {}", f.max_aim);
    }

    /// What came of a jet's fight (jet_fight): the tasks it ran (in order, each once
    /// as it began), its forms over time, its rockets fired, the damage to the
    /// player, the furthest it got from where it came in, its boosting at some
    /// time, the highest and lowest it flew (uu).
    #[derive(Default)]
    struct JetFight {
        ran: Vec<String>,
        forms: Vec<(f32, u8)>,
        launches: usize,
        /// Its rounds' classes (each once, as it first fired one), and the guns it
        /// had in hand (each once, as it took it).
        rounds: Vec<String>,
        guns: Vec<String>,
        /// Every task its plans held (the ones over within a frame too: its point
        /// choosers).
        planned: std::collections::HashSet<String>,
        /// The loops it held (its engine's).
        loops: std::collections::HashSet<String>,
        damage: f32,
        travelled: f32,
        boosted: bool,
        heights: (f32, f32),
        landed_as_robot: bool,
        sounds: Vec<String>,
    }

    /// A jet (Flyer_Aut) on Downfall coming in at the air spawn point nearest the
    /// map's start, a player there (`player_view`: his pawn as the AI sees it, from
    /// where he is: standing_player, jet_player); run `secs`, `hook` each frame.
    fn jet_fight(
        secs: f32,
        player_view: impl Fn(Entity, Vec3) -> PlayerView,
        mut hook: impl FnMut(f32, &mut Ai, &WorldView, &mut Vec3),
    ) -> Option<JetFight> {
        let dir = crate::assets_dir();
        let map = crate::MAPS.iter().find(|m| m.stem == "downfall")?;
        let (Some(level), Some(nav)) =
            (crate::load_level_data(map), NavMesh::load(&dir, "downfall"))
        else {
            eprintln!("downfall's level or navigation not built: skipped");
            return None;
        };
        let data = AiData::load(&dir);
        let Some(jet) = data.behaviour("Flyer_Aut") else {
            eprintln!("ai.txt not built: skipped");
            return None;
        };
        if clips::for_type("Flyer_Aut").is_none() || clips::for_type("Flyer_Aut~other").is_none() {
            eprintln!("the jets' clips aren't built: skipped");
            return None;
        }
        let kinds = crate::enemies::Kinds::load(&dir);
        let k = kinds.kinds.iter().find(|k| k.id == "Flyer_Aut")?;
        let mut ecs = bevy::ecs::world::World::new();
        let mut world = AiWorld {
            data: Arc::new(data.clone()),
            sounds: Arc::new(audio::AiSounds::load(&dir)),
            loaded: true,
            ..Default::default()
        };
        world.shared.tactical.reset(Some(&nav));
        let floor = |at: Vec3, radius: f32| {
            WorldView {
                level: Some(&level),
                ..Default::default()
            }
            .ground_at(at, radius, pawn::MAX_STEP_UU, 300.0)
            .map_or(at, |z| Vec3::new(at.x, at.y, z + pawn::FLOOR_DIST))
        };
        let map_start = floor(ue::to_ue(level.player_start), 50.0);
        let spawn = nav
            .points
            .iter()
            .filter(|p| {
                p.kind == crate::nav::PointKind::Air
                    && p.name.starts_with("TnSurvivalAirSpawnPoint")
            })
            .min_by(|a, b| {
                a.at.distance(ue::from_ue(map_start))
                    .total_cmp(&b.at.distance(ue::from_ue(map_start)))
            })
            .map(|p| ue::to_ue(p.at))
            .expect("an air spawn point");
        // (The player in the open, on the floor nearest it that it can see from
        // there.)
        let sight = WorldView {
            level: Some(&level),
            ..Default::default()
        };
        let start = nav
            .points
            .iter()
            .filter(|p| p.kind == crate::nav::PointKind::Cell)
            .map(|p| ue::to_ue(p.at) + Vec3::Z * pawn::FLOOR_DIST)
            .filter(|at| sight.fast_trace(spawn, *at + Vec3::Z * 150.0))
            .min_by(|a, b| ue::vsize2d(*a - spawn).total_cmp(&ue::vsize2d(*b - spawn)))
            .unwrap_or(map_start);
        let player = ecs.spawn_empty().id();
        let mut player_at = start;
        let mut view = WorldView {
            players: vec![player_view(player, start)],
            nav: Some(&nav),
            level: Some(&level),
            ..Default::default()
        };
        let e = ecs.spawn_empty().id();
        let body = Body {
            feet: spawn,
            yaw: ue::yaw_of(start - spawn),
            radius: k.radius * ue::UU_PER_M,
            half_height: k.half_height * ue::UU_PER_M,
            health: k.health,
            mesh_scale: k.scale,
            forms: forms_of(&kinds, k),
        };
        let mut ai = new_ai(&mut world, &view, e, 0, jet, body, 33).expect("the jet's AI");
        assert_eq!(
            ai.pawn.physics,
            pawn::Physics::RigidBody,
            "it comes in flying"
        );
        let mut out = JetFight {
            heights: (f32::MAX, f32::MIN),
            ..Default::default()
        };
        let dt = 1.0 / 30.0;
        let mut last_task = String::new();
        let mut last_form = forms::ALT;
        // (Escalation's alerter sends it at the player as it comes in, and every
        // 5 s.)
        let mut alert_left = 0.0;
        for _ in 0..(30.0 * secs) as usize {
            world.shared.now += dt;
            view.now = world.shared.now;
            view.players[0] = player_view(player, player_at);
            alert_left -= dt;
            let alerting = (alert_left <= 0.0).then(|| (player, view.players[0].loc));
            if alerting.is_some() {
                alert_left = ALERT_SECS;
            }
            {
                let mut slots = vec![Slot {
                    entity: e,
                    ai: &mut ai,
                    dormant: false,
                }];
                view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
                let sounds = world.sounds.clone();
                run_frame(
                    &mut world.shared,
                    &sounds,
                    &mut view,
                    &mut slots,
                    dt,
                    alerting,
                );
            }
            let now = world.shared.now;
            out.launches += world.shared.launches.len();
            for l in world.shared.launches.drain(..) {
                if !out.rounds.contains(&l.data.class) {
                    out.rounds.push(l.data.class.clone());
                }
            }
            world.shared.shots.clear();
            if ai.pawn.weapon.equipped && !out.guns.contains(&ai.pawn.weapon.data.class) {
                out.guns.push(ai.pawn.weapon.data.class.clone());
            }
            out.sounds
                .extend(world.shared.sounds.drain(..).map(|(_, s, _)| s.to_string()));
            world.shared.dialog.clear();
            out.loops
                .extend(world.shared.loops.drain(..).map(|(_, c, ..)| c.to_string()));
            world.shared.effects.clear();
            world.shared.effects_stopped.clear();
            for h in world.shared.player_damage.drain(..) {
                out.damage += h.amount;
            }
            if let Some(op) = ai.brain.runner.active.as_ref() {
                let name = ai.brain.core.d.name(op.task.symbol).to_string();
                if name != last_task {
                    eprintln!("t={now:4.1} {name}");
                    out.ran.push(name.clone());
                    last_task = name;
                }
            }
            for t in &ai.brain.runner.plan {
                out.planned
                    .insert(ai.brain.core.d.name(t.symbol).to_string());
            }
            let p = &ai.pawn;
            if let Some(f) = p.forms.as_ref()
                && f.current != last_form
            {
                out.forms.push((now, f.current));
                last_form = f.current;
            }
            out.travelled = out.travelled.max((p.loc - spawn).length());
            out.boosted |= p.active_plane().is_some_and(|f| f.boosting);
            if p.active_plane().is_some() {
                out.heights.0 = out.heights.0.min(p.loc.z);
                out.heights.1 = out.heights.1.max(p.loc.z);
            }
            out.landed_as_robot |=
                forms::form(p) == forms::ROBOT && p.physics == pawn::Physics::Walking;
            hook(now, &mut ai, &view, &mut player_at);
        }
        Some(out)
    }

    /// (By hand: FOC_JET="map:kind:secs:x,y,z" — a jet brought in at the air spawn
    /// point nearest a player standing at x,y,z (by default Ignition's Flyer_Dec, 120 s,
    /// by the health station near the start). Each half second: its task and plan,
    /// where it is, its attack point and how far, whether it faces him, and its
    /// rockets.)
    #[test]
    #[ignore]
    fn jet_probe() {
        let arg = std::env::var("FOC_JET").unwrap_or_default();
        let f: Vec<&str> = arg.split(':').collect();
        let stem = f
            .first()
            .filter(|s| !s.is_empty())
            .copied()
            .unwrap_or("ignition");
        let id = f
            .get(1)
            .filter(|s| !s.is_empty())
            .copied()
            .unwrap_or("Flyer_Dec");
        let secs: f32 = f.get(2).and_then(|s| s.parse().ok()).unwrap_or(120.0);
        let him: Vec3 = f
            .get(3)
            .map(|s| {
                let v: Vec<f32> = s.split(',').filter_map(|x| x.parse().ok()).collect();
                Vec3::new(v[0], v[1], v[2])
            })
            .unwrap_or(Vec3::new(27.0, 0.0, -12.0));
        let seed: u64 = std::env::var("FOC_JET_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(33);
        let dir = crate::assets_dir();
        let map = crate::MAPS
            .iter()
            .find(|m| m.stem == stem)
            .expect("the map");
        let (Some(level), Some(mut nav)) = (crate::load_level_data(map), NavMesh::load(&dir, stem))
        else {
            return;
        };
        if let Some(c) = level.collision.as_ref() {
            while !nav.flights_measured() {
                nav.measure_flights(c, std::time::Duration::from_secs(1));
            }
        }
        let data = AiData::load(&dir);
        let jet = data.behaviour(id).expect("its behaviour");
        let kinds = crate::enemies::Kinds::load(&dir);
        let k = kinds.kinds.iter().find(|k| k.id == id).expect("its kind");
        let mut ecs = bevy::ecs::world::World::new();
        let mut world = AiWorld {
            data: Arc::new(data.clone()),
            sounds: Arc::new(audio::AiSounds::load(&dir)),
            loaded: true,
            ..Default::default()
        };
        world.shared.tactical.reset(Some(&nav));
        let feet = ue::to_ue(him) + Vec3::Z * pawn::FLOOR_DIST;
        let spawn = nav
            .points
            .iter()
            .filter(|p| {
                p.kind == crate::nav::PointKind::Air
                    && p.name.starts_with("TnSurvivalAirSpawnPoint")
            })
            .min_by(|a, b| a.at.distance(him).total_cmp(&b.at.distance(him)))
            .map(|p| ue::to_ue(p.at))
            .expect("an air spawn point");
        let player = ecs.spawn_empty().id();
        let mut view = WorldView {
            players: vec![PlayerView {
                radius: 175.0,
                half_height: 270.0,
                eyes: feet + Vec3::Z * 486.0,
                ..standing_player(player, feet)
            }],
            nav: Some(&nav),
            level: Some(&level),
            ..Default::default()
        };
        view.players[0].loc = feet + Vec3::Z * 270.0;
        let e = ecs.spawn_empty().id();
        let body = Body {
            feet: spawn,
            yaw: ue::yaw_of(feet - spawn),
            radius: k.radius * ue::UU_PER_M,
            half_height: k.half_height * ue::UU_PER_M,
            health: k.health,
            mesh_scale: k.scale,
            forms: forms_of(&kinds, k),
        };
        let mut ai = new_ai(&mut world, &view, e, 0, jet, body, seed).expect("the jet's AI");
        let dt = 1.0 / 30.0;
        let mut alert_left = 0.0;
        let mut launches = 0;
        for frame in 0..(30.0 * secs) as usize {
            world.shared.now += dt;
            view.now = world.shared.now;
            alert_left -= dt;
            let alerting = (alert_left <= 0.0).then(|| (player, view.players[0].loc));
            if alerting.is_some() {
                alert_left = ALERT_SECS;
            }
            {
                let mut slots = vec![Slot {
                    entity: e,
                    ai: &mut ai,
                    dormant: false,
                }];
                view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
                let sounds = world.sounds.clone();
                run_frame(
                    &mut world.shared,
                    &sounds,
                    &mut view,
                    &mut slots,
                    dt,
                    alerting,
                );
            }
            launches += world.shared.launches.len();
            world.shared.launches.clear();
            world.shared.shots.clear();
            world.shared.sounds.clear();
            world.shared.dialog.clear();
            world.shared.loops.clear();
            world.shared.effects.clear();
            world.shared.effects_stopped.clear();
            world.shared.player_damage.clear();
            if frame % 15 != 0 {
                continue;
            }
            let core = &ai.brain.core;
            let run = &ai.brain.runner;
            let p = &ai.pawn;
            let op = run
                .active
                .as_ref()
                .map(|o| core.d.name(o.task.symbol).to_string())
                .unwrap_or_default();
            let plan: Vec<&str> = run.plan.iter().map(|t| core.d.name(t.symbol)).collect();
            let attack = core.fetch(super::brain::LR_ATTACK_POINT);
            let at = ue::from_ue(p.loc);
            let ap = ue::from_ue(attack);
            let to_him = ue::normal2d(view.players[0].loc - p.loc);
            let facing = match p.active_plane() {
                Some(f) => ue::normal2d(ue::yaw_vec(f.rotation.y)),
                None => p.forward(),
            };
            let hist: Vec<String> = run
                .history
                .iter()
                .rev()
                .take(2)
                .map(|(_, ts, _, st)| {
                    format!(
                        "{}={st:?}",
                        ts.iter()
                            .map(|s| core.d.name(*s))
                            .collect::<Vec<_>>()
                            .join("+")
                    )
                })
                .collect();
            eprintln!(
                "{:5.1} {op} {plan:?} at {:.1} {:.1} {:.1} {:?} speed {:.0} | attack point {:.1} {:.1} {:.1} range {:.0} | faces him {:.2} | form {} rockets {launches} | {hist:?}",
                world.shared.now,
                at.x,
                at.y,
                at.z,
                p.physics,
                p.vel.length(),
                ap.x,
                ap.y,
                ap.z,
                super::drones::range_to(p, attack),
                to_him.dot(facing),
                forms::form(p),
            );
        }
    }

    /// A jet comes in at its air spawn point flying, sees the player, flies to an
    /// attack point ahead of and over him (boosting on its way) and fires its
    /// rockets at him; its special attack up, it changes into its robot over him,
    /// lands and makes its pulse attack, then changes back.
    #[test]
    fn a_jet_attacks_from_the_air_and_as_a_robot() {
        let Some(f) = jet_fight(40.0, standing_player, |_, _, _, _| {}) else {
            return;
        };
        eprintln!("ran {:?}", f.ran);
        eprintln!(
            "forms {:?} launches {} damage {} travelled {} boosted {} heights {:?} landed {} sounds {:?}",
            f.forms,
            f.launches,
            f.damage,
            f.travelled,
            f.boosted,
            f.heights,
            f.landed_as_robot,
            f.sounds
        );
        let first = |t: &str| f.ran.iter().position(|r| r == t);
        assert!(first("JTS_FlyTo").is_some(), "it never flew: {:?}", f.ran);
        assert!(f.travelled > 2000.0, "it went no more than {}", f.travelled);
        assert!(
            first("JTS_HoverToFaceEnemy").is_some(),
            "it never hovered to face him"
        );
        assert!(f.launches >= 2, "it fired {} rockets", f.launches);
        assert!(f.boosted, "it never flew boosting");
        assert!(
            f.forms.iter().any(|x| x.1 == forms::ROBOT),
            "it never took its robot form"
        );
        assert!(f.landed_as_robot, "its robot never landed");
        assert!(first("JTS_RobotAttack").is_some(), "no pulse attack");
        // Its plane's engine (VEH_JET_FLYER_01): its hover's off-load loop, its
        // boost's loop, and its boost's start as it takes off.
        for cue in [
            "BL_VEH_JET.VEH_JET_FLYER_01_HOVER_FLIGHT_OFFLOAD",
            "BL_VEH_JET.VEH_JET_FLYER_01_BOOST_LOOP",
        ] {
            assert!(f.loops.contains(cue), "no {cue}: {:?}", f.loops);
        }
        assert!(
            f.sounds
                .iter()
                .any(|s| s == "BL_VEH_JET.VEH_JET_FLYER_01_BOOST_START"),
            "no boost start"
        );
        assert!(
            f.sounds
                .iter()
                .any(|s| s == "BL_WPN_GUN_AI_PLANE.FLYER_ROCKETS_SHOOT"),
            "no rocket shot"
        );
        assert!(
            f.forms.iter().filter(|x| x.1 == forms::ALT).count() >= 1,
            "it never changed back"
        );
    }

    /// The point the hook below gives the jet at its first stop before a hover to
    /// face its enemy (`at`: from where it is and where it takes him to be), the
    /// time it did and when (and how far from the point) it went on to its gun with
    /// the point still its attack point.
    fn jet_hover_from(
        at: impl Fn(&mut Ai, &WorldView) -> Option<Vec3>,
    ) -> Option<(f32, Option<(f32, f32)>)> {
        let mut given: Option<(f32, Vec3)> = None;
        let mut armed: Option<(f32, f32)> = None;
        jet_fight(30.0, standing_player, |now, ai, view, _| {
            let op = ai
                .brain
                .runner
                .active
                .as_ref()
                .map(|o| ai.brain.core.d.name(o.task.symbol).to_string())
                .unwrap_or_default();
            let hovers = ai
                .brain
                .runner
                .plan
                .iter()
                .any(|t| ai.brain.core.d.name(t.symbol) == "JTS_HoverToFaceEnemy");
            match given {
                None if op == "JTS_StopMovement" && hovers && ai.pawn.active_plane().is_some() => {
                    if let Some(point) = at(ai, view) {
                        ai.brain
                            .core
                            .store(brain::LR_ATTACK_POINT, point, Vec3::ZERO);
                        given = Some((now, point));
                    }
                }
                Some((_, point))
                    if armed.is_none()
                        && (op == "JTS_EquipWeapon" || op == "JTS_ShootAtEnemy")
                        && ai.brain.core.fetch(brain::LR_ATTACK_POINT) == point =>
                {
                    armed = Some((now, drones::range_to(&ai.pawn, point)));
                }
                _ => {}
            }
        })?;
        let (t0, _) = given.expect("it never stopped to hover to an attack point");
        Some((t0, armed.map(|(t, r)| (t - t0, r))))
    }

    /// A jet whose hover to its attack point begins already within its motion
    /// controller's radius of the place across the ground but well under it (as
    /// one come in steeply, or fast enough to sail on past, can) goes on up onto the
    /// place, faces its enemy and takes its gun, as the hover's range asks (its
    /// cylinder over the place). Its controller, counting it as there, had stopped
    /// steering it: it hung where it was, failing its hover every 5 s, not firing,
    /// till its robot attack came round.
    #[test]
    fn a_jet_comes_onto_an_attack_point_it_begins_its_hover_beside() {
        let Some((t0, armed)) = jet_hover_from(|ai, view| {
            let loc = ai.pawn.loc;
            let point = [-1.0, 1.0]
                .into_iter()
                .map(|up: f32| loc + Vec3::new(150.0, 0.0, 550.0 * up))
                .find(|&point| {
                    view.fast_trace(loc, point)
                        && view.fast_trace(point, point - Vec3::Z * 800.0)
                        && view.fast_trace(point, point + Vec3::Z * 800.0)
                })?;
            // (Come to rest there.)
            ai.pawn.vel = Vec3::ZERO;
            if let Some(f) = ai.pawn.active_plane_mut() {
                f.desired_location = loc;
            }
            Some(point)
        }) else {
            return;
        };
        eprintln!("given its point at {t0:.1} s: armed {armed:?}");
        let (after, range) = armed.expect("it never took its gun at the point");
        assert!(after < 5.0, "it took {after:.1} s to settle onto the point");
        assert!(range < 0.2, "it was {range:.0} uu off the point");
    }

    /// Right over its enemy, with no way across the ground to face him, a jet's
    /// hover counts it as facing him whichever way it is turned. Settled on the
    /// very spot over a player standing still, it had waited out its hover's 5 s
    /// over and over, not firing.
    #[test]
    fn a_jet_right_over_its_enemy_faces_him_whichever_way_it_is_turned() {
        let Some((t0, armed)) = jet_hover_from(|ai, view| {
            let him = ai.brain.core.fetch(brain::LR_CONFIRMED_ENEMY);
            let point = Vec3::new(him.x, him.y, ai.pawn.loc.z);
            if !view.fast_trace(him, point) {
                return None;
            }
            ai.pawn.loc = point;
            ai.pawn.vel = Vec3::ZERO;
            if let Some(f) = ai.pawn.active_plane_mut() {
                f.desired_location = point;
            }
            Some(point)
        }) else {
            return;
        };
        eprintln!("put over him at {t0:.1} s: armed {armed:?}");
        let (after, _) = armed.expect("it never took its gun over him");
        assert!(after < 1.0, "it took {after:.1} s to count as facing him");
    }

    /// The jets' guns as their blueprints have them: their rockets
    /// (TnWeaponFlyerRocket, which they start with: AirToGroundWeapon), a sticky
    /// round (TnProjectileFlyerRocket: 60 within 10 m, its fuse 3 s, shaken off by a
    /// change of form, its camera shake and its grenade marker), and their
    /// dogfight gun (DogfightWeapon, TnWeaponRocketVehicle from mp_shared:
    /// Rocket_Vehicle_AI_WEPDATA, a rocket every 0.75 s: TnProjectilePlaneRocketTemp,
    /// RocketVh_AI_PROJDATA, 18 within 5 m at 120 m/s).
    #[test]
    fn the_jets_have_their_rockets_and_their_dogfight_gun() {
        let data = AiData::load(&crate::assets_dir());
        let Some(jet) = data.behaviour("Flyer_Aut") else {
            eprintln!("ai.txt not built: skipped");
            return;
        };
        let bp = blueprint::Blueprint::new(&jet);
        let classes: Vec<&str> = bp.guns.iter().map(|g| g.class.as_str()).collect();
        assert_eq!(classes, ["TnWeaponFlyerRocket", "TnWeaponRocketVehicle"]);
        let rockets = &bp.guns[0];
        assert!(
            (rockets.interval.0 - 3.0).abs() < 1e-4,
            "{:?}",
            rockets.interval
        );
        let round = rockets.projectile.as_ref().expect("its rocket");
        assert_eq!(round.class, "TnProjectileFlyerRocket");
        assert!((round.damage - 60.0).abs() < 1e-4 && (round.damage_radius - 1000.0).abs() < 1e-3);
        let sticky = round.sticky.as_ref().expect("a sticky round");
        assert_eq!(sticky.fuse, (3.0, 3.0));
        assert_eq!(sticky.fuse_sound, "BL_WPN_GUN_AI_PLANE.FLYER_ROCKETS_FUSE");
        assert_eq!(
            sticky
                .shaken_off
                .as_ref()
                .map(|(fx, cue)| (fx.as_str(), *cue)),
            Some((
                "FlyersSticky_Detach_FX",
                "BL_WPN_GUN_AI_PLANE.FLYER_ROCKETS_detach"
            ))
        );
        let shake = round.shake.expect("its camera shake");
        assert!((shake.secs - 0.5).abs() < 1e-4 && shake.radius > 9000.0);
        assert!(round.marker, "no grenade marker");
        let dogfight = &bp.guns[1];
        assert!((dogfight.interval.0 - 0.75).abs() < 1e-4);
        let round = dogfight.projectile.as_ref().expect("its rocket");
        assert_eq!(round.class, "TnProjectilePlaneRocketTemp");
        assert!((round.speed - 12000.0).abs() < 1e-3 && (round.damage - 18.0).abs() < 1e-4);
        assert!((round.damage_radius - 500.0).abs() < 1e-3);
        assert!(round.sticky.is_none() && round.shake.is_none() && !round.marker);
        // Their EquipWeapon's references: 0 the rockets, 1 the dogfight gun.
        for (r, class) in [(0, "TnWeaponFlyerRocket"), (1, "TnWeaponRocketVehicle")] {
            let blueprint::Asset::Weapon(w) = bp.asset(r) else {
                panic!("no weapon reference {r}");
            };
            assert_eq!(bp.gun(&w).map(|g| g.class.clone()).as_deref(), Some(class));
        }
    }

    /// The jets' planes' looks (jet.rs's VehicleLooks, their FlyerAut_VEHDEF's):
    /// ten hover jets (HighLeap_Boosters_Thick_Short_FX on HoverSocket_L, _R and
    /// _C1 to _C8), four boosters (two afterburners' kinds and the contrail on the
    /// BoostSockets), and their engine's events: the off-load loop, the boost's
    /// loops (the whine's slot empty), its start and end, and the flyby within 80 m.
    #[test]
    fn the_jets_planes_look_and_sound_as_the_games() {
        let data = AiData::load(&crate::assets_dir());
        let Some(b) = data.behaviour("Flyer_Aut") else {
            eprintln!("ai.txt not built: skipped");
            return;
        };
        let looks = jet::VehicleLooks::from_behaviour(&b);
        assert_eq!(looks.hover_fx.len(), 10);
        assert!(
            looks
                .hover_fx
                .iter()
                .all(|(s, k)| &**s == "HighLeap_Boosters_Thick_Short_FX"
                    && k.starts_with("HoverSocket_"))
        );
        let boost: Vec<(&str, &str)> = looks.boost_fx.iter().map(|(s, k)| (&**s, &**k)).collect();
        assert_eq!(
            boost,
            [
                ("Afterburner_A_FX", "BoostSocket_C"),
                ("Afterburner_Sm_AI_Jet_FX", "BoostSocket_L"),
                ("Afterburner_Sm_AI_Jet_FX", "BoostSocket_R"),
                ("Trail_FlyersBoost_FX", "BoostSocket_C"),
            ]
        );
        assert_eq!(looks.offload_loops, ["Auto_Engine_Gear_1_OffLoad"]);
        assert_eq!(looks.offload_oneshots, ["Auto_Boost_End"]);
        assert_eq!(
            looks.boost_loops,
            [
                "Auto_Boost_Loop",
                "Auto_Boost_Loop_Exhaust",
                "Auto_Boost_Whine"
            ]
        );
        assert_eq!(looks.boost_oneshots, ["Auto_Boost_Start"]);
        assert_eq!(looks.boost_fade, (0.0, 0.6));
        assert_eq!(looks.flyby, Some(("Auto_Flyby".to_string(), 8000.0)));
    }

    /// A player in his jet (AI.EEnemyForm 2, a flying vehicle; flying physics)
    /// hovering 15 m over `at`, his plane's middle there.
    fn jet_player(entity: Entity, at: Vec3) -> PlayerView {
        let loc = at + Vec3::Z * 1500.0;
        PlayerView {
            loc,
            feet: loc - Vec3::Z * 150.0,
            radius: 300.0,
            half_height: 150.0,
            form: 2,
            airborne: true,
            physics: 4,
            eyes: loc,
            ..standing_player(entity, at)
        }
    }

    /// Against a player in his jet the jet dogfights (JTS_AttackPlane): it flies at
    /// its boost to a point round him (its circling point,
    /// TnBestFlyingCirclingPointSensor's), takes its dogfight gun
    /// (TnWeaponRocketVehicle, for its rockets) and fires at him, head on or for
    /// effect; it never comes down for its pulse attack (not against a jet).
    #[test]
    fn a_jet_dogfights_a_player_in_a_jet() {
        let Some(f) = jet_fight(30.0, jet_player, |_, _, _, _| {}) else {
            return;
        };
        eprintln!("ran {:?}", f.ran);
        eprintln!(
            "forms {:?} launches {} rounds {:?} guns {:?} damage {} boosted {} sounds {:?}",
            f.forms, f.launches, f.rounds, f.guns, f.damage, f.boosted, f.sounds
        );
        let ran = |t: &str| f.ran.iter().any(|r| r == t);
        assert!(
            f.planned.contains("JTS_ChooseNewFlyingCirclingPoint"),
            "it never circled: {:?}",
            f.planned
        );
        assert!(f.boosted, "it never flew boosting");
        assert!(
            f.guns.iter().any(|g| g == "TnWeaponRocketVehicle"),
            "it never took its dogfight gun: {:?}",
            f.guns
        );
        assert!(
            ran("JTS_DogfightShootHeadOn") || ran("JTS_DogfightFireForEffect"),
            "it never dogfought: {:?}",
            f.ran
        );
        assert!(
            f.rounds.iter().any(|r| r == "TnProjectilePlaneRocketTemp"),
            "no dogfight rockets: {:?}",
            f.rounds
        );
        assert!(
            f.sounds.iter().any(|s| s == "BL_WPN_GUN_ROCKET.SHOOT_VEH"),
            "no shot sounds"
        );
        assert!(
            !f.forms.iter().any(|x| x.1 == forms::ROBOT),
            "it came down as a robot against a jet"
        );
    }

    /// A soldier struck as it moves (STS_NavigationHitReaction: its
    /// NavigatingHitReaction, TR_AI_Navigation_HitReactionPicker): backing away at
    /// a jog and struck in the body from the front, it stumbles backward
    /// (HitReact_StumbleBackward, one of three, its root motion carrying it); not
    /// by a sword's blow, which names no bone (the sweep's box meets its
    /// collision, not its body's bones), nor standing still. The stumbles are
    /// among its set's clips.
    #[test]
    fn a_soldier_struck_while_it_moves_stumbles() {
        let dir = crate::assets_dir();
        let data = AiData::load(&dir);
        let Some(b) = data.behaviour("SoldierAut_Prime") else {
            eprintln!("ai.txt not built: skipped");
            return;
        };
        let bp = blueprint::Blueprint::new(&b);
        let blueprint::Asset::Picker(picker) = bp.asset(15) else {
            panic!("no navigation hit reactions");
        };
        assert_eq!(picker.reactions.len(), 12);
        let mut p = pawn::Pawn::new(Entity::PLACEHOLDER, Vec3::ZERO, 0.0, 150.0, 220.0, 100.0);
        // (Facing +X, backing away along -X at a jog.)
        p.vel = Vec3::new(-400.0, 0.0, 0.0);
        p.motion.speed = 1;
        let hit = |bone: Option<&str>| DamageInfo {
            instigator: None,
            hit_location: Vec3::new(100.0, 0.0, 100.0),
            damage: 135.0,
            kind: crate::enemies::Death::Gunfire,
            damage_type: DamageKind::Other,
            momentum: Vec3::new(-20000.0, 0.0, 0.0),
            bone: bone.map(Arc::from),
        };
        let stumble = picker.choose(&p, &hit(Some("C_Spine02_Lumbar02_XB")));
        assert_eq!(&*stumble.name, "HitReact_StumbleBackward");
        assert_eq!(stumble.root_motion, anim::RMM_ACCEL);
        assert!(picker.choose(&p, &hit(None)).is_none());
        p.vel = Vec3::ZERO;
        assert!(
            picker
                .choose(&p, &hit(Some("C_Spine02_Lumbar02_XB")))
                .is_none()
        );
        let Some(clips) = clips::Clips::load(&dir, "soldier") else {
            eprintln!("the soldiers' clips not built: skipped");
            return;
        };
        let group = clips
            .group("HitReact_StumbleBackward")
            .expect("no stumbles");
        assert_eq!(group.len(), 3);
        for name in ["HitReact_StumbleLeft", "HitReact_StumbleRight"] {
            assert!(clips.group(name).is_some(), "no {name}");
        }
    }

    /// A blow's push on one that lives (Pawn.AddVelocity): a light one (2 and 1
    /// m/s: multiplayer's sword's HitMomentum, 20000 along the ground and 10000 up,
    /// over its Mass of 100) moves a soldier standing on the ground 2 m/s away,
    /// still on it (in the air for only 0.07 s). A push that would throw it up for
    /// over 0.2 s has it fall (the demo's sword, Escalation's, throws it up for
    /// 0.51 s: knocked back). Only a melee blow (and a stun's, a ram's) pushes: rounds and
    /// blasts don't (TnPawn's bIgnoreForces).
    #[test]
    fn a_blow_pushes_one_that_lives() {
        assert!(respects_forces(damage_kind(
            Death::Melee,
            crate::xp::Blow::Weapon
        )));
        assert!(respects_forces(damage_kind(
            Death::Gunfire,
            crate::xp::Blow::Melee
        )));
        assert!(!respects_forces(damage_kind(
            Death::Gunfire,
            crate::xp::Blow::Weapon
        )));
        assert!(!respects_forces(damage_kind(
            Death::Explosive,
            crate::xp::Blow::Weapon
        )));
        let mut p = pawn::Pawn::new(Entity::PLACEHOLDER, Vec3::ZERO, 0.0, 150.0, 220.0, 100.0);
        add_velocity(&mut p, Vec3::new(200.0, 0.0, 100.0));
        assert_eq!(p.physics, pawn::Physics::Walking);
        assert!((p.vel.x - 200.0).abs() < 1e-3, "{}", p.vel);
        let mut r = pawn::Pawn::new(Entity::PLACEHOLDER, Vec3::ZERO, 0.0, 150.0, 220.0, 100.0);
        add_velocity(&mut r, Vec3::new(0.0, 0.0, 400.0));
        assert_eq!(r.physics, pawn::Physics::Falling);
        assert!((r.vel.z - 400.0).abs() < 1e-3);
    }

    /// Ignition's AirStrike station's barrier (DynamicBlockingVolume_16006, up
    /// till its console is bought) holds walkers off the platform but not a
    /// Sniper in flight: one that comes down on the platform flies out through the
    /// barrier to a perch and goes on from perch to perch, never held in the air
    /// (it hung against the barrier on its way to the air hub in it).
    #[test]
    fn a_sniper_flies_through_the_airstrike_barrier() {
        use crate::collision::Mover;
        let dir = crate::assets_dir();
        let Some(map) = crate::MAPS.iter().find(|m| m.stem == "ignition") else {
            return;
        };
        let (Some(level), Some(nav)) =
            (crate::load_level_data(map), NavMesh::load(&dir, "ignition"))
        else {
            eprintln!("ignition's level or navigation not built: skipped");
            return;
        };
        let data = AiData::load(&dir);
        let Some(b) = data.behaviour("Sniper") else {
            eprintln!("ai.txt not built: skipped");
            return;
        };
        let kinds = crate::enemies::Kinds::load(&dir);
        let k = kinds
            .kinds
            .iter()
            .find(|k| k.id == "Sniper")
            .expect("its kind");
        let hub = nav
            .points
            .iter()
            .find(|p| p.name == "AirPathNode_5790")
            .expect("the air hub in the barrier")
            .at;
        // (The barrier stands: a walker astride its west face at the hub's height
        // is pushed out of it, a Sniper in flight isn't.)
        let astride = Vec3::new(154.9, hub.y - 1.0, hub.z);
        let pushed = |who| level.push_out_for(astride, 0.6, 2.0, who);
        assert!(pushed(Mover::Ai).is_some(), "the barrier is gone");
        assert!(
            pushed(Mover::AiFlying).is_none(),
            "the barrier stops a flight"
        );
        let mut ecs = bevy::ecs::world::World::new();
        let sounds = Arc::new(audio::AiSounds::load(&dir));
        let mut world = AiWorld {
            data: Arc::new(data.clone()),
            sounds: sounds.clone(),
            loaded: true,
            ..Default::default()
        };
        world.shared.tactical.reset(Some(&nav));
        let player = ecs.spawn_empty().id();
        let feet = ue::to_ue(Vec3::new(138.24, 0.0, -17.92));
        let mut view = WorldView {
            players: vec![standing_player(player, feet)],
            nav: Some(&nav),
            level: Some(&level),
            ..Default::default()
        };
        // (On the platform, in the barrier.)
        let at = Vec3::new(156.5, 6.7, 5.1);
        let at = level
            .ground_for(at + Vec3::Y, k.radius, at.y + 1.0, at.y - 8.0, Mover::Ai)
            .map_or(at, |y| Vec3::new(at.x, y, at.z));
        let body = Body {
            feet: ue::to_ue(at) + Vec3::Z * pawn::FLOOR_DIST,
            yaw: ue::yaw_of(feet - ue::to_ue(at)),
            radius: k.radius * ue::UU_PER_M,
            half_height: k.half_height * ue::UU_PER_M,
            health: k.health,
            mesh_scale: k.scale,
            forms: forms_of(&kinds, k),
        };
        let e = ecs.spawn_empty().id();
        let mut ai = new_ai(&mut world, &view, e, 0, b, body, 77).expect("the Sniper's AI");
        let dt = 1.0 / 30.0;
        let mut first_perch = None;
        let mut perches = Vec::new();
        let mut held_since: Option<(f32, Vec3)> = None;
        let mut longest_held = 0.0f32;
        for _ in 0..(30.0 * 20.0) as usize {
            world.shared.now += dt;
            view.now = world.shared.now;
            {
                let mut slots = vec![Slot {
                    entity: e,
                    ai: &mut ai,
                    dormant: false,
                }];
                view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
                run_frame(&mut world.shared, &sounds, &mut view, &mut slots, dt, None);
            }
            world.shared.shots.clear();
            world.shared.launches.clear();
            world.shared.sounds.clear();
            world.shared.dialog.clear();
            world.shared.loops.clear();
            world.shared.player_damage.clear();
            world.shared.effects.clear();
            world.shared.effects_stopped.clear();
            let now = world.shared.now;
            let p = &ai.pawn;
            if p.physics == pawn::Physics::Spider
                && let Some(c) = ai
                    .brain
                    .sensors
                    .sniper_store
                    .as_ref()
                    .and_then(|s| s.current)
                && perches.last() != Some(&c)
            {
                perches.push(c);
                first_perch.get_or_insert(now);
            }
            // (Held: flying, and no more than 0.25 m from where it was.)
            if p.physics == pawn::Physics::Flying {
                match held_since {
                    Some((t, there)) if p.loc.distance(there) < 25.0 => {
                        longest_held = longest_held.max(now - t);
                    }
                    _ => held_since = Some((now, p.loc)),
                }
            } else {
                held_since = None;
            }
        }
        eprintln!("perches {perches:?}, first at {first_perch:?} s, held {longest_held:.1} s");
        assert!(
            first_perch.is_some_and(|t| t < 5.0),
            "it perched first at {first_perch:?} s"
        );
        assert!(
            perches.len() >= 3,
            "it perched only {} times",
            perches.len()
        );
        assert!(longest_held < 1.5, "it hung in the air {longest_held:.1} s");
    }

    /// A Sniper coming in at each of `stem`'s air spawn points in turn (its `kind`),
    /// the player standing somewhere else on the map's floor every 15 s (picked by
    /// `seed`), Escalation's alerter sending it at him every five seconds as a
    /// match does, shot on its perch every `hurt` seconds if given (0: as each crawl
    /// on its perch begins), each run `secs`: how often each perched, and every
    /// time one went wrong, where and what it was doing: held in the air (flying,
    /// and no more than 0.4 m from where it was, for 2 s), loose in the air (flying
    /// or falling with no flight of its own for 1.5 s), off its perch (on its wall
    /// but a metre past its patch, or 3 m off it) or over the map (10 m over its
    /// highest navigation point).
    fn sniper_survey(
        stem: &str,
        kind: &str,
        secs: f32,
        seed: u32,
        hurt: Option<f32>,
    ) -> Option<(Vec<usize>, Vec<String>)> {
        let dir = crate::assets_dir();
        let map = crate::MAPS.iter().find(|m| m.stem == stem)?;
        let (Some(level), Some(mut nav)) = (crate::load_level_data(map), NavMesh::load(&dir, stem))
        else {
            eprintln!("{stem}'s level or navigation not built: skipped");
            return None;
        };
        // (Its flights measured against the map, as the match's start has them.)
        if let Some(c) = level.collision.as_ref() {
            while !nav.flights_measured() {
                nav.measure_flights(c, std::time::Duration::from_secs(1));
            }
        }
        let data = AiData::load(&dir);
        let Some(b) = data.behaviour(kind) else {
            eprintln!("ai.txt not built: skipped");
            return None;
        };
        let kinds = crate::enemies::Kinds::load(&dir);
        let k = kinds.kinds.iter().find(|k| k.id == kind)?;
        let spawns: Vec<Vec3> = nav
            .points
            .iter()
            .filter(|p| {
                p.kind == crate::nav::PointKind::Air
                    && p.name.starts_with("TnSurvivalAirSpawnPoint")
            })
            .map(|p| p.at)
            .collect();
        let floor: Vec<Vec3> = nav
            .points
            .iter()
            .filter(|p| p.kind == crate::nav::PointKind::Cell)
            .map(|p| p.at)
            .collect();
        let sounds = Arc::new(audio::AiSounds::load(&dir));
        let (mut perched, mut holds) = (Vec::new(), Vec::new());
        for (si, &spawn) in spawns.iter().enumerate() {
            let mut ecs = bevy::ecs::world::World::new();
            let mut world = AiWorld {
                data: Arc::new(data.clone()),
                sounds: sounds.clone(),
                loaded: true,
                ..Default::default()
            };
            world.shared.tactical.reset(Some(&nav));
            let mut rng = 0x9e37_79b9u32.wrapping_mul(si as u32 + 1 + seed * 97);
            let mut pick = || {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                ue::to_ue(floor[rng as usize % floor.len()]) + Vec3::Z * pawn::FLOOR_DIST
            };
            let player = ecs.spawn_empty().id();
            let mut feet = pick();
            let mut view = WorldView {
                players: vec![standing_player(player, feet)],
                nav: Some(&nav),
                level: Some(&level),
                ..Default::default()
            };
            let body = Body {
                feet: ue::to_ue(spawn),
                yaw: ue::yaw_of(feet - ue::to_ue(spawn)),
                radius: k.radius * ue::UU_PER_M,
                half_height: k.half_height * ue::UU_PER_M,
                health: k.health,
                mesh_scale: k.scale,
                forms: forms_of(&kinds, k),
            };
            let e = ecs.spawn_empty().id();
            let seed = 31 + si as u64 + seed as u64 * 1000;
            let mut ai = new_ai(&mut world, &view, e, 0, b.clone(), body, seed)?;
            sniper_came_in(&mut ai, &view, spawn);
            let dt = 1.0 / 30.0;
            let mut held_since: Option<(f32, Vec3)> = None;
            let mut reported = false;
            let (mut perches, mut last_perch) = (0, None);
            let full_health = ai.pawn.health;
            let mut last_hurt = 0.0;
            let mut drift_reported = None;
            let (mut loose_since, mut loose_reported, mut above_reported) = (None, false, false);
            let top = nav.points.iter().map(|p| p.at.y).fold(f32::MIN, f32::max);
            for frame in 0..(30.0 * secs) as usize {
                if frame > 0 && frame % (30 * 15) == 0 {
                    feet = pick();
                }
                world.shared.now += dt;
                view.now = world.shared.now;
                view.players[0] = standing_player(player, feet);
                // (Shot at on its perch every `hurt` seconds, as a player does: 60
                // damage a time, its health topped up once under 40%, so that it
                // reels now and then and fights on.)
                // (`hurt` 0: shot once as each crawl along its perch begins.)
                let crawling = matches!(
                    ai.brain.runner.active.as_ref().map(|o| &o.kind),
                    Some(ops::Kind::CrawlOnSniperPoint(_))
                );
                if let Some(every) = hurt
                    && ai.pawn.physics == pawn::Physics::Spider
                    && if every > 0.0 {
                        view.now - last_hurt >= every
                    } else {
                        crawling && view.now - last_hurt > 0.5
                    }
                {
                    last_hurt = view.now;
                    if ai.pawn.health < 0.4 * full_health {
                        ai.pawn.health = full_health;
                    }
                    ai.pawn.health -= 60.0;
                    ai.brain.core.inbox.push(Event {
                        id: EventId::TookDamage,
                        actor: Some(player),
                        damage: Some(DamageInfo {
                            instigator: Some(player),
                            hit_location: ai.pawn.loc,
                            damage: 60.0,
                            kind: crate::enemies::Death::Gunfire,
                            damage_type: DamageKind::Other,
                            momentum: Vec3::ZERO,
                            bone: None,
                        }),
                        noise: None,
                        notify: None,
                    });
                }
                {
                    let mut slots = vec![Slot {
                        entity: e,
                        ai: &mut ai,
                        dormant: false,
                    }];
                    view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
                    // (Escalation's alerter, as a match has it: every ALERT_SECS.)
                    let alerting = (frame > 0 && frame % (30.0 * ALERT_SECS) as usize == 0)
                        .then(|| (player, view.players[0].loc));
                    run_frame(
                        &mut world.shared,
                        &sounds,
                        &mut view,
                        &mut slots,
                        dt,
                        alerting,
                    );
                }
                world.shared.shots.clear();
                world.shared.launches.clear();
                world.shared.sounds.clear();
                world.shared.dialog.clear();
                world.shared.loops.clear();
                world.shared.player_damage.clear();
                world.shared.effects.clear();
                world.shared.effects_stopped.clear();
                let now = world.shared.now;
                let p = &ai.pawn;
                let store = ai.brain.sensors.sniper_store.as_ref()?;
                // (Loose in the air: flying or falling with no flight of its own
                // (FlyTo, or DetachFromWall's take-off) for over a second and a
                // half, or 10 m over the map's highest point.)
                let flight_op = matches!(
                    ai.brain.runner.active.as_ref().map(|o| &o.kind),
                    Some(ops::Kind::FlyToSniperPoint(_) | ops::Kind::DetachFromWall(_))
                );
                if matches!(p.physics, pawn::Physics::Flying | pawn::Physics::Falling) && !flight_op
                {
                    let since = *loose_since.get_or_insert(now);
                    if now - since > 1.5 && !loose_reported {
                        loose_reported = true;
                        holds.push(format!(
                            "spawn {si} at {now:.1} s: loose in the air ({:?}) for {:.1} s at {:?}, vel {:?} accel {:?}, its task {:?}, form {:?}",
                            p.physics,
                            now - since,
                            ue::from_ue(p.loc),
                            p.vel.round(),
                            p.accel.round(),
                            ai.brain.runner.active.as_ref().map(|o| ai.brain.core.d.name(o.task.symbol)),
                            p.forms.as_ref().map(|f| f.current),
                        ));
                    }
                } else {
                    loose_since = None;
                    loose_reported = false;
                }
                if ue::from_ue(p.loc).y > top + 10.0 && !above_reported {
                    above_reported = true;
                    holds.push(format!(
                        "spawn {si} at {now:.1} s: above the map at {:?} ({:?}), vel {:?} accel {:?}, its task {:?}",
                        ue::from_ue(p.loc),
                        p.physics,
                        p.vel.round(),
                        p.accel.round(),
                        ai.brain.runner.active.as_ref().map(|o| ai.brain.core.d.name(o.task.symbol)),
                    ));
                }
                // (Off its perch: crawling on its wall but more than a metre past
                // its patch's edges, or 3 m off the wall: it has crawled off the
                // wall, into the air.)
                if p.physics == pawn::Physics::Spider
                    && let Some(c) = store.current.and_then(|c| sniper::Point::get(&nav, c))
                    && drift_reported != Some(c.id)
                {
                    let [x, y, z] = c.axes();
                    let d = p.loc - c.centre;
                    let (out, across, up) = (d.dot(x), d.dot(y).abs(), d.dot(z).abs());
                    if out.abs() > 300.0 || across > c.extent.0 + 100.0 || up > c.extent.1 + 100.0 {
                        drift_reported = Some(c.id);
                        let task = ai
                            .brain
                            .runner
                            .active
                            .as_ref()
                            .map(|o| ai.brain.core.d.name(o.task.symbol).to_string());
                        let core = &ai.brain.core;
                        let hist: Vec<String> = ai
                            .brain
                            .runner
                            .history
                            .iter()
                            .rev()
                            .take(4)
                            .map(|(_, ts, _, st)| {
                                let names: Vec<&str> = ts.iter().map(|s| core.d.name(*s)).collect();
                                format!("{}={st:?}", names.join("+"))
                            })
                            .collect();
                        holds.push(format!(
                            "spawn {si} at {now:.0} s: off its perch {} ({out:.0} uu out, {across:.0} across, \
                             {up:.0} up; its patch {:.0} x {:.0}) at {:?}, vel {:?} accel {:?}, its task {task:?}, before {hist:?}",
                            nav.points[c.id as usize].name,
                            c.extent.0,
                            c.extent.1,
                            ue::from_ue(p.loc),
                            p.vel.round(),
                            p.accel.round(),
                        ));
                    }
                }
                // (FOC_SNIPER_TRACE="spawn:from:to": that spawn's Sniper every
                // frame from `from` to `to` seconds.)
                if let Some((ts, t0, t1)) = std::env::var("FOC_SNIPER_TRACE").ok().and_then(|v| {
                    let f: Vec<f32> = v.split(':').filter_map(|x| x.parse().ok()).collect();
                    (f.len() == 3).then(|| (f[0] as usize, f[1], f[2]))
                }) && ts == si
                    && (t0..=t1).contains(&now)
                {
                    let fly = match ai.brain.runner.active.as_ref().map(|o| &o.kind) {
                        Some(ops::Kind::FlyToSniperPoint(f)) => {
                            format!(
                                "state {} avoiding {} approach {:?}",
                                f.state,
                                f.avoiding,
                                ue::from_ue(f.approach)
                            )
                        }
                        _ => "-".to_string(),
                    };
                    eprintln!(
                        "{now:6.2} {:?} at {:?} vel {:?} accel {:?} hit {:?} next {:?} anchor {:?} dest {:?} {fly}",
                        ai.brain.runner.active.as_ref().map(|o| ai
                            .brain
                            .core
                            .d
                            .name(o.task.symbol)),
                        ue::from_ue(p.loc),
                        p.vel.round(),
                        p.accel.round(),
                        p.notes.hit_wall,
                        store
                            .next_point()
                            .map(|d| nav.points[d as usize].name.clone()),
                        store.anchor.map(|d| nav.points[d as usize].name.clone()),
                        store
                            .destination()
                            .map(|d| nav.points[d as usize].name.clone()),
                    );
                    let core = &ai.brain.core;
                    let run = &ai.brain.runner;
                    let hist: Vec<String> = run
                        .history
                        .iter()
                        .rev()
                        .take(3)
                        .map(|(_, ts, _, st)| {
                            let names: Vec<&str> = ts.iter().map(|s| core.d.name(*s)).collect();
                            format!("{}={st:?}", names.join("+"))
                        })
                        .collect();
                    let plan: Vec<&str> = run.plan.iter().map(|t| core.d.name(t.symbol)).collect();
                    eprintln!(
                        "        plan {plan:?} status {:?} invalid {} ws vis {} apa {} frus {} blocked {} bal {} form {} alert {} search {} navmesh {} | {hist:?}",
                        run.status,
                        run.invalid_precondition,
                        core.ws(htn::WS_ENEMY_VISIBLE),
                        core.ws(htn::WS_ATTACK_POINT_AVAILABLE),
                        core.ws(htn::WS_FRUSTRATED),
                        core.ws(htn::WS_PATH_BLOCKED),
                        core.ws(htn::WS_BALANCED),
                        core.ws(htn::WS_CURRENT_FORM),
                        core.ws(htn::WS_ALERT_LEVEL),
                        core.ws(htn::WS_SEARCH_AVAILABLE),
                        core.ws(htn::WS_ON_NAV_MESH),
                    );
                }
                if p.physics == pawn::Physics::Spider && store.current != last_perch {
                    last_perch = store.current;
                    perches += 1;
                }
                if p.physics != pawn::Physics::Flying {
                    held_since = None;
                    continue;
                }
                match held_since {
                    Some((t, there)) if p.loc.distance(there) < 40.0 => {
                        if now - t > 2.0 && !reported {
                            reported = true;
                            let task = ai
                                .brain
                                .runner
                                .active
                                .as_ref()
                                .map(|o| ai.brain.core.d.name(o.task.symbol).to_string());
                            holds.push(format!(
                                "spawn {si} at {:.0} s: held at {:?} (its task {task:?}, headed for {:?})",
                                now,
                                ue::from_ue(p.loc),
                                store
                                    .destination()
                                    .map(|d| nav.points[d as usize].name.clone()),
                            ));
                        }
                    }
                    _ => {
                        held_since = Some((now, p.loc));
                        reported = false;
                    }
                }
            }
            perched.push(perches);
        }
        Some((perched, holds))
    }

    /// Ignition's Snipers, from each of its air spawn points, go on from perch to
    /// perch with the player moving about the map, never held in the air. (A
    /// Sniper that saw the player as its search flight ended had its plan
    /// checked before it took to the wall: in its robot form, in the air, with
    /// nothing in its domain to do there, it hung where it was for good, drifting
    /// into the walls and ceilings round it. Of eight, five perched no more than
    /// five times in two and a half minutes.)
    #[test]
    fn ignitions_snipers_never_hang_in_the_air() {
        let Some((perched, holds)) = sniper_survey("ignition", "Sniper", 60.0, 0, None) else {
            return;
        };
        eprintln!("perches {perched:?}");
        assert!(holds.is_empty(), "{holds:#?}");
        assert!(
            perched.iter().all(|&n| n >= 5),
            "perches from each spawn point: {perched:?}"
        );
    }

    /// Ancients' Snipers, from each of its air spawn points, go on from perch to
    /// perch, never held in the air. (Sent straight from a perch to one beside it,
    /// on flights the scaffolding round the map's middle leaves too little room
    /// for (TnSniperPoint_10859 and TnSniperPoint_7942 each way, TnSniperPoint_14899
    /// to TnSniperPoint_2368), they hung against it for the rest of the run: of 16,
    /// seven perched only once in a minute.)
    #[test]
    fn ancients_snipers_never_hang_in_the_air() {
        let Some((perched, holds)) = sniper_survey("ancients", "Sniper", 60.0, 0, None) else {
            return;
        };
        eprintln!("perches {perched:?}");
        assert!(holds.is_empty(), "{holds:#?}");
        assert!(
            perched.iter().all(|&n| n >= 5),
            "perches from each spawn point: {perched:?}"
        );
    }

    /// Downfall's Snipers, from each of its air spawn points with the doors still
    /// locked, find a perch they can go on from and go on from perch to perch.
    /// (Their search from four of the spawn points finding none, they flew to the
    /// nearest free perch they could see, TnSniperPoint_9893, which has no flights
    /// to or from it, and clung there for good. From two others the game's search
    /// takes them round 46 points of the air network to their first perch, nearly a
    /// minute's flight: hence the minute and a half.) Shot at on their perches, and
    /// sent at the player every five seconds as a match does, none crawls off its
    /// wall. (The one from the third spawn point, crawling up its wall on
    /// TnSniperPoint_4152 to its take-off spot as an alert changed its plan, kept
    /// crawling up the wall and off its top into the sky, still clinging: its
    /// crawl, cut short, left it pushed along the wall.)
    #[test]
    fn downfalls_snipers_go_on_from_their_first_perch() {
        let Some((perched, holds)) = sniper_survey("downfall", "Sniper_AUT", 90.0, 0, Some(1.5))
        else {
            return;
        };
        eprintln!("perches {perched:?}");
        assert!(holds.is_empty(), "{holds:#?}");
        assert!(
            perched.iter().all(|&n| n >= 3),
            "perches from each spawn point: {perched:?}"
        );
    }

    /// (By hand: FOC_SNIPERS="map:secs:seeds" — the survey above on any map, each
    /// seed in turn, printing the holds and perches.)
    #[test]
    #[ignore]
    fn sniper_hunt() {
        let arg = std::env::var("FOC_SNIPERS").unwrap_or_default();
        let f: Vec<&str> = arg.split(':').collect();
        let stem = f
            .first()
            .filter(|s| !s.is_empty())
            .copied()
            .unwrap_or("ancients");
        let secs: f32 = f.get(1).and_then(|s| s.parse().ok()).unwrap_or(150.0);
        let seeds: u32 = f.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
        let hurt: Option<f32> = f.get(3).and_then(|s| s.parse().ok());
        for seed in 0..seeds {
            let kind = if stem == "downfall" {
                "Sniper_AUT"
            } else {
                "Sniper"
            };
            let Some((perched, holds)) = sniper_survey(stem, kind, secs, seed, hurt) else {
                return;
            };
            eprintln!("seed {seed}: perches {perched:?} holds {holds:#?}");
        }
    }

    /// A Sniper on its perch hurt past half its health (its injured sensor's
    /// threshold: InjuredDamagePercentOfHealthForReaction) reels (WS_Balanced
    /// off: SNIPERTASK_PlayHitReaction, one of its insect's HitReactions on its
    /// full body), then fights on.
    #[test]
    fn a_sniper_reels_when_hurt() {
        let mut hurt_at = None;
        let mut reeled = None;
        let Some(f) = sniper_fight(20.0, |now, ai, view, player_at| {
            let Some(nav) = view.nav else { return };
            let on_wall = ai.pawn.physics == pawn::Physics::Spider
                && ai
                    .pawn
                    .forms
                    .as_ref()
                    .is_some_and(|f| f.current == forms::ALT && f.change.is_none());
            if hurt_at.is_none() && on_wall {
                // (In view of its perch first, as a_sniper_perches_and_shoots.)
                if let Some(c) = ai
                    .brain
                    .sensors
                    .sniper_store
                    .as_ref()
                    .and_then(|s| s.current)
                    .and_then(|c| sniper::Point::get(nav, c))
                {
                    let eye = c.centre + c.normal * 200.0;
                    if let Some(at) = nav
                        .points
                        .iter()
                        .filter(|p| p.kind == crate::nav::PointKind::Cell)
                        .map(|p| ue::to_ue(p.at) + Vec3::Z * pawn::FLOOR_DIST)
                        .filter(|at| {
                            let d = *at - eye;
                            (2000.0..4000.0).contains(&d.length())
                                && d.normalize().dot(c.normal) > 0.3
                                && view.fast_trace(eye, *at + Vec3::Z * 150.0)
                        })
                        .min_by(|a, b| (*a - eye).length().total_cmp(&(*b - eye).length()))
                    {
                        *player_at = at;
                    }
                }
                if ai.pawn.laser.active {
                    hurt_at = Some(now);
                    ai.pawn.health -= 120.0;
                    let by = view.players[0].entity;
                    ai.brain.core.inbox.push(Event {
                        id: EventId::TookDamage,
                        actor: Some(by),
                        damage: Some(DamageInfo {
                            instigator: Some(by),
                            hit_location: ai.pawn.loc + Vec3::X * 50.0,
                            damage: 120.0,
                            kind: crate::enemies::Death::Gunfire,
                            damage_type: DamageKind::Other,
                            momentum: Vec3::ZERO,
                            bone: None,
                        }),
                        noise: None,
                        notify: None,
                    });
                }
            }
            if hurt_at.is_some() && reeled.is_none() {
                let full = &ai.pawn.anim.players[anim::MASK_FULL_BODY as usize].slot;
                if full
                    .channels
                    .iter()
                    .any(|c| c.anim.starts_with("HitReaction_"))
                {
                    reeled = Some(now);
                }
            }
        }) else {
            return;
        };
        let (Some(hurt), Some(reeled)) = (hurt_at, reeled) else {
            panic!("hurt at {hurt_at:?}, reeled at {reeled:?}: {:?}", f.ran);
        };
        eprintln!("hurt at {hurt:.2}, reeled at {reeled:.2}; ran {:?}", f.ran);
        assert!(
            reeled - hurt < 0.5,
            "it reeled {:.2} s after",
            reeled - hurt
        );
        assert!(f.ran.iter().any(|r| r == "SNIPERTASK_PlayHitReaction"));
        // (It went back to scanning and shooting after.)
        let reel = f
            .ran
            .iter()
            .rposition(|r| r == "SNIPERTASK_PlayHitReaction")
            .unwrap();
        assert!(
            f.ran[reel..].iter().any(|r| r == "SNIPERTASK_ScanEnemy"),
            "it didn't fight on: {:?}",
            f.ran
        );
    }

    /// The Titan's health falling to 60% (under its first wave's 67%): it says
    /// so (SpawningDronesDialog) and sends out two support drones, flung out of its
    /// back, that fly to their posts 20 m out round it, 2.56 m over the floor, and
    /// shield it (nothing hurts it while they live: its Enemy takes nothing); both
    /// shot down, the shield goes (ShieldDeactivatedDialog). Dead, its drones die
    /// with it.
    #[test]
    fn the_titan_sends_out_support_drones() {
        let mut posts: Vec<(f32, f32, f32)> = Vec::new();
        let mut shielded_at = None;
        let mut unshielded_at = None;
        let mut drones_after_its_death = None;
        let titan_id = std::cell::Cell::new(None);
        let Some(f) = titan_fight(0.0, 30.0, |t, ais, world| {
            let titan = match titan_id.get() {
                Some(t) => t,
                None => {
                    titan_id.set(Some(ais[0].0));
                    ais[0].0
                }
            };
            // At 3 s its health down to 60%.
            if (t - 3.0).abs() < 0.02
                && let Some((_, ai)) = ais.iter_mut().find(|(e, _)| *e == titan)
            {
                ai.pawn.health = ai.pawn.health_max * 0.6;
            }
            let shielded = drones::shielded(&world.shared, titan);
            if shielded && shielded_at.is_none() {
                shielded_at = Some(t);
            }
            // At 15 s, each drone's place round it; then both shot down.
            if (t - 15.0).abs() < 0.02 {
                let at = ais
                    .iter()
                    .find(|(e, _)| *e == titan)
                    .map(|(_, a)| a.pawn.loc);
                for (e, ai) in ais.iter_mut() {
                    if *e == titan {
                        continue;
                    }
                    let p = &ai.pawn;
                    let d = at.map_or(0.0, |a| ue::vsize2d(p.loc - a));
                    posts.push((d, p.loc.z - p.half_height, p.vel.length()));
                    assert_eq!(p.physics, pawn::Physics::RigidBody, "a drone still falling");
                    ai.pawn.health = 0.0;
                }
            }
            if t > 15.0 && !shielded && unshielded_at.is_none() {
                unshielded_at = Some(t);
            }
            // At 20 s its health down to 20%: two more; then it dies.
            if (t - 20.0).abs() < 0.02
                && let Some((_, ai)) = ais.iter_mut().find(|(e, _)| *e == titan)
            {
                ai.pawn.health = ai.pawn.health_max * 0.2;
            }
            if (t - 27.0).abs() < 0.02
                && let Some((_, ai)) = ais.iter_mut().find(|(e, _)| *e == titan)
            {
                ai.pawn.health = 0.0;
            }
            if (t - 28.0).abs() < 0.02 {
                drones_after_its_death = Some(ais.len());
            }
        }) else {
            return;
        };
        eprintln!(
            "the Titan sent {:?}, ran {:?}, said {:?}; posts {posts:?}; shielded at {shielded_at:?}, off at {unshielded_at:?}",
            f.sent, f.ran, f.dialog
        );
        assert!(
            f.ran.contains("TITAN_SpawnHealthDrones"),
            "it sent none out"
        );
        assert_eq!(
            f.sent.iter().filter(|s| *s == "SupportDrone").count(),
            4,
            "two support drones a wave"
        );
        assert!(f.dialog.iter().any(|d| d.contains("SpawningDronesDialog")));
        assert_eq!(posts.len(), 2, "the first two weren't about at 15 s");
        for (d, _, _) in &posts {
            assert!((d - 2000.0).abs() < 400.0, "a drone {d} uu off its Titan");
        }
        assert!(
            shielded_at.is_some_and(|t| t < 15.0),
            "its drones didn't shield it"
        );
        assert!(unshielded_at.is_some_and(|t| t < 15.5), "the shield stayed");
        assert!(
            f.dialog
                .iter()
                .any(|d| d.contains("ShieldDeactivatedDialog"))
        );
        assert_eq!(drones_after_its_death, Some(0), "its drones outlived it");
        // Its shield shown while they lived (TnBuffSupportDroneDamage's looping
        // ApplyFx), taken off with them; their engines heard.
        assert!(
            f.effects.iter().any(|s| s == drones::SHIELD_FX),
            "no shield shown"
        );
        assert!(
            f.stopped.iter().any(|s| s == drones::SHIELD_FX),
            "the shield stayed shown"
        );
        assert!(
            f.loops.contains("BL_CHR_AI_SENTINEL.SENTINEL_ENGINE_LP"),
            "no engine heard"
        );
    }

    /// The player out of the Titan's reach (12 m up over its floor): it sends its
    /// explosive drones at him (TITAN_SpawnUnreachableDrones: three), which fly up
    /// to him and blow up by him (50 each), or their lifespans run out.
    #[test]
    fn the_titan_sends_explosive_drones_at_an_unreachable_enemy() {
        let Some(f) = titan_fight(1200.0, 25.0, |_, _, _| {}) else {
            return;
        };
        eprintln!(
            "the Titan sent {:?}, ran {:?}; damage {}",
            f.sent, f.ran, f.damage
        );
        assert!(
            f.ran.contains("TITAN_SpawnUnreachableDrones"),
            "it sent no drones at him"
        );
        assert!(f.sent.iter().filter(|s| *s == "ExplosiveDrone").count() >= 3);
        assert!(f.damage >= 50.0, "none blew up by him");
        // Each glowing as it flies (its EffectTemplate on its EffectSockets), and
        // blowing itself up (TnDamageTypeExplodeOnDeath: its self destruct death).
        assert!(
            f.effects.iter().filter(|s| *s == "DroneFlight_FX").count() >= 3,
            "a drone without its glow"
        );
        // (Its fuse: its suicide animation's notify.)
        assert!(
            f.effects.iter().any(|s| s == "TitanDrone_Attack_FX"),
            "no fuse lit"
        );
        assert!(
            f.kills.contains(&crate::enemies::Death::SelfDestruct),
            "none blew itself up"
        );
    }

    /// Scrappers whose plain blows his raised shield stops are frustrated
    /// (WS_Frustrated): they play their frustration (SCRAPPER_FrustratedReaction).
    #[test]
    fn scrappers_are_frustrated_by_his_shield() {
        let Some(f) = fight_on_ignition_as("ScrapperDec", 3, 2000.0..4000.0, 30.0, true) else {
            return;
        };
        eprintln!("scrappers ran {:?}", f.ran);
        assert!(
            f.ran
                .iter()
                .any(|r| r.contains("SCRAPPER_FrustratedReaction")),
            "none was frustrated"
        );
    }

    /// The heavies' machine gun (the Titan's here, on Downfall) fires from its two
    /// barrels by turns (its MuzzleSockets, MuzzleFlash and MuzzleFlash2, with
    /// TnWeaponEffectIncrementMuzzleSocket), every round with its tracer, and
    /// flashes on its second barrel with every round besides, started over each
    /// time (TnWeaponEffectPlayFXOnSocket: PEC_RestartIfAlreadyPlaying).
    #[test]
    fn the_heavy_machine_gun_fires_from_both_barrels_by_turns() {
        // (Its gun's sockets where a pose would put them, in its own frame.)
        let first = Vec3::new(320.0, 35.0, 150.0);
        let second = Vec3::new(365.0, -55.0, 150.0);
        let Some(f) = titan_fight(0.0, 20.0, |_, ais, _| {
            if let Some((_, ai)) = ais.first_mut()
                && ai.pawn.sockets.is_empty()
            {
                for (name, at) in [("MuzzleFlash", first), ("MuzzleFlash2", second)] {
                    ai.pawn.sockets.push(pawn::PawnSocket {
                        name: Arc::from(name),
                        at,
                        axes: [Vec3::X, Vec3::Y, Vec3::Z],
                    });
                }
            }
        }) else {
            return;
        };
        eprintln!("the Titan fired {} rounds", f.rounds.len());
        assert!(f.rounds.len() >= 6, "it hardly fired");
        // (Each round's barrel: the one its frame's end is nearest; it moves a
        // little between firing and then.)
        let barrel = |at: Vec3| usize::from(at.distance(second) < at.distance(first));
        for (k, w) in f.rounds.windows(2).enumerate() {
            assert!(
                w.iter()
                    .all(|(at, _)| at.distance(first).min(at.distance(second)) < 40.0),
                "round {k} left from neither barrel: {:?}",
                w[0].0
            );
            assert_ne!(
                barrel(w[0].0),
                barrel(w[1].0),
                "rounds {k} and {} from one barrel",
                k + 1
            );
        }
        assert!(
            f.rounds.iter().all(|(_, tracer)| *tracer),
            "a round without its tracer"
        );
        let flashes = f
            .socket_effects
            .iter()
            .filter(|(fx, socket, restart)| {
                fx == "WEP_TitanMG_MuzzleFlash_FX" && socket == "MuzzleFlash2" && *restart
            })
            .count();
        assert_eq!(
            flashes,
            f.rounds.len(),
            "not a flash on its second barrel with every round"
        );
    }

    /// A Heavy Gunner brought in 25 to 45 m from him closes in, shooting while it
    /// walks and while it stands (HEAVY_ShootWhileMoving, HEAVY_ShootWhileStanding).
    #[test]
    fn heavy_gunners_fight_on_ignition() {
        let Some(f) = fight_on_ignition("HeavyGunnerAut", 1, 2500.0..4500.0, 30.0) else {
            return;
        };
        eprintln!("the heavy gunner ran {:?}", f.ran);
        assert!(f.took_enemy, "it never took the player for its enemy");
        assert!(
            f.ran[0].contains("HEAVY_ShootWhileMoving")
                || f.ran[0].contains("HEAVY_ShootWhileStanding"),
            "it never shot at him"
        );
        assert!(f.shots > 0, "it never fired");
        assert!(f.damage > 0.0, "it never hurt him");
    }

    /// A Guard brought in 25 to 45 m from him: told of him (the alerter), it changes
    /// to its battle form (TnChangeRobotFormOperator: its arm cannon in hand), charges
    /// its Guardian Blaster (its glow on its muzzle, stopped as it fires) and fires
    /// its homing rounds at him.
    #[test]
    fn guards_fight_on_ignition() {
        let Some(f) = fight_on_ignition("Guard", 1, 2500.0..4500.0, 40.0) else {
            return;
        };
        eprintln!("the guard ran {:?} forms {:?}", f.ran, f.forms);
        let mut fx: Vec<&String> = f.effects.iter().collect();
        fx.dedup();
        eprintln!(
            "its effects {fx:?}, {} stopped, {} rounds",
            f.effects_stopped, f.launches
        );
        assert!(f.took_enemy, "it never took the player for its enemy");
        assert!(
            f.forms[0].contains(&guard::FORM_BATTLE),
            "it never changed to its battle form"
        );
        assert!(
            f.effects
                .iter()
                .any(|e| e == "Guardian_Arm_Cannon_Charge_FX"),
            "it never charged"
        );
        assert!(f.effects_stopped > 0, "its charge never ended");
        assert!(f.launches > 0, "it never fired");
        assert!(
            f.effects
                .iter()
                .any(|e| e == "GuardianBlaster_MuzzleFlash_Fx"),
            "no muzzle flash"
        );
    }

    /// Leapers brought in 35 to 65 m from a player standing in Ignition's smash
    /// volume (and wholly inside it themselves): they leap high at him and dive
    /// down by him, their smash hurting him; close by they strike him, lunge at
    /// him and vent their boosters at him as he keeps close.
    #[test]
    fn leapers_leap_and_smash_on_ignition() {
        let dir = crate::assets_dir();
        let Some(nav) = NavMesh::load(&dir, "ignition") else {
            return;
        };
        let Some(v) = nav.leap_volumes.first() else {
            eprintln!("ignition's smash volume not built: skipped");
            return;
        };
        // The cell inside it nearest the middle of those inside it.
        let inside: Vec<Vec3> = nav
            .points
            .iter()
            .filter(|p| matches!(p.kind, crate::nav::PointKind::Cell))
            .map(|p| p.at)
            .filter(|at| v.encompasses(*at + Vec3::Y * 0.5))
            .collect();
        assert!(!inside.is_empty(), "no cells in the smash volume");
        let mid = inside.iter().copied().sum::<Vec3>() / inside.len() as f32;
        let at = inside
            .iter()
            .copied()
            .min_by(|a, b| a.distance(mid).total_cmp(&b.distance(mid)))
            .unwrap();
        let Some(f) = fight_on_ignition_with(
            "Leaper",
            2,
            3500.0..6500.0,
            40.0,
            Setup {
                player_at: Some(ue::to_ue(at)),
                in_leap_volume: true,
                ..Default::default()
            },
        ) else {
            return;
        };
        eprintln!("leapers ran {:?}, damage {}", f.ran, f.damage);
        assert!(f.took_enemy, "none took the player for its enemy");
        assert!(
            f.ran.iter().any(|r| r.contains("LPTS_LeapSmash")),
            "none leapt"
        );
        assert!(f.damage > 0.0, "none hurt him");
    }

    /// A shotgunner brought in within 10 m of him dashes back from him (firing as
    /// it goes) or moves away.
    #[test]
    fn shotgunners_back_off_when_close() {
        let Some(f) = fight_on_ignition("ShotgunnerAut", 1, 200.0..1200.0, 8.0) else {
            return;
        };
        eprintln!("the shotgunner ran {:?}", f.ran);
        assert!(
            f.ran[0].contains("SHTS_DashBack") || f.ran[0].contains("SHTS_MoveAwayFromEnemy"),
            "it never backed off"
        );
        assert!(f.shots > 0, "it never fired");
    }

    /// An enemy's round strikes a player's body, not his collision cylinder: a
    /// pellet passing beside his chest, inside his cylinder (which
    /// TnPawnCollisionController lets such traces through), misses him; one at his
    /// chest strikes it where it is.
    #[test]
    fn rounds_strike_the_body_not_the_cylinder() {
        let mut pl = standing_player(Entity::PLACEHOLDER, Vec3::ZERO);
        pl.radius = 140.0;
        pl.half_height = 155.0;
        pl.loc = Vec3::Z * 155.0;
        // His chest: 0.5 m round, from 1.5 to 2.5 m up (the demo's space).
        let chest = crate::ragdoll::Shape::Capsule {
            a: Vec3::new(0.0, 1.5, 0.0),
            b: Vec3::new(0.0, 2.5, 0.0),
            r: 0.5,
        };
        // From 10 m off, 2 m up (Unreal's space).
        let from = Vec3::new(-1000.0, 0.0, 200.0);
        let beside = (Vec3::new(0.0, 100.0, 200.0) - from).normalize();
        assert!(
            player_trace(&pl, from, beside).is_some(),
            "his cylinder takes it"
        );
        pl.body = Some(Arc::from([chest]));
        assert!(
            player_trace(&pl, from, beside).is_none(),
            "it passes a metre beside his chest"
        );
        let t = player_trace(&pl, from, Vec3::X).expect("his chest takes it");
        assert!((t - 950.0).abs() < 1.0, "struck {t} uu along");
    }

    /// A player's grenade coming down 25 m from a soldier leaves it be; one 5 m
    /// off puts it in danger (WS_DangerClose's DC_Grenade), and it plans its
    /// reaction (STS_Reactions: STS_ReactToGrenade, towards where it came down),
    /// the danger cleared as the reaction is done.
    #[test]
    fn soldiers_react_to_grenades_near_them() {
        let dir = crate::assets_dir();
        let Some(map) = crate::MAPS.iter().find(|m| m.stem == "ignition") else {
            return;
        };
        let (Some(level), Some(nav)) =
            (crate::load_level_data(map), NavMesh::load(&dir, "ignition"))
        else {
            eprintln!("ignition's level or navigation not built: skipped");
            return;
        };
        let data = AiData::load(&dir);
        let Some(b) = data.behaviour("SoldierDec_Kaon") else {
            eprintln!("ai.txt not built: skipped");
            return;
        };
        let mut ecs = bevy::ecs::world::World::new();
        let mut world = AiWorld {
            data: Arc::new(data.clone()),
            loaded: true,
            ..Default::default()
        };
        world.shared.tactical.reset(Some(&nav));
        // The player far off (out of its sight and hearing), the soldier on a cell
        // near the map's start.
        let player = ecs.spawn_empty().id();
        let start = ue::to_ue(level.player_start);
        let mut view = WorldView {
            now: 0.0,
            players: vec![standing_player(
                player,
                start + Vec3::new(0.0, 0.0, 100_000.0),
            )],
            ais: Vec::new(),
            nav: Some(&nav),
            level: Some(&level),
            barriers: Vec::new(),
        };
        let at = nav
            .points
            .iter()
            .filter(|p| matches!(p.kind, crate::nav::PointKind::Cell))
            .map(|p| ue::to_ue(p.at))
            .min_by(|a, c| ue::vsize2d(*a - start).total_cmp(&ue::vsize2d(*c - start)))
            .expect("a cell");
        let e = ecs.spawn_empty().id();
        let body = Body {
            feet: at,
            yaw: 0.0,
            radius: 150.0,
            half_height: 220.0,
            health: 150.0,
            mesh_scale: 0.8,
            forms: None,
        };
        let mut ai = new_ai(&mut world, &view, e, 0, b.clone(), body, 11).expect("a soldier's AI");
        let d = b.domain.clone().unwrap();
        let sounds = world.sounds.clone();
        fn frame(
            world: &mut AiWorld,
            view: &mut WorldView,
            ai: &mut Ai,
            e: Entity,
            sounds: &audio::AiSounds,
        ) {
            let dt = 1.0 / 30.0;
            world.shared.now += dt;
            view.now = world.shared.now;
            let mut slots = vec![Slot {
                entity: e,
                ai,
                dormant: false,
            }];
            view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
            run_frame(&mut world.shared, sounds, view, &mut slots, dt, None);
        }
        for _ in 0..60 {
            frame(&mut world, &mut view, &mut ai, e, &sounds);
        }
        let danger = |ai: &Ai| ai.brain.core.ws_bit(htn::WS_DANGER_CLOSE, htn::DC_GRENADE);
        // 25 m off: nothing.
        let far = ai.pawn.loc + Vec3::new(2500.0, 0.0, 0.0);
        notify_near_grenade(&mut ai.brain, &ai.pawn, far);
        assert!(!danger(&ai), "a grenade 25 m off");
        // 5 m off, beside it.
        let near = ai.pawn.loc + Vec3::new(0.0, 500.0, 0.0);
        notify_near_grenade(&mut ai.brain, &ai.pawn, near);
        assert!(danger(&ai), "a grenade 5 m off");
        assert_eq!(ai.brain.core.fetch(brain::LR_GRENADE_LOCATION), near);
        let mut reacted = false;
        for _ in 0..(30 * 5) {
            frame(&mut world, &mut view, &mut ai, e, &sounds);
            let plan: Vec<&str> = ai
                .brain
                .runner
                .plan
                .iter()
                .map(|t| d.name(t.symbol))
                .collect();
            reacted |= plan.contains(&"STS_ReactToGrenade");
        }
        assert!(reacted, "no reaction planned");
        assert!(!danger(&ai), "the danger never cleared");
    }

    /// Soldiers brought in at `spots` (the demo's space, on the floor), sent at a
    /// player standing at the level's start by the alerter for `secs`: where each
    /// one's feet were every half second.
    fn soldiers_run(
        level: &crate::Level,
        nav: &NavMesh,
        data: &AiData,
        b: &Arc<Behaviour>,
        spots: &[Vec3],
        secs: f32,
    ) -> Vec<Vec<Vec3>> {
        let mut ecs = bevy::ecs::world::World::new();
        let mut world = AiWorld {
            data: Arc::new(data.clone()),
            loaded: true,
            ..Default::default()
        };
        world.shared.tactical.reset(Some(nav));
        let player = ecs.spawn_empty().id();
        let start = ue::to_ue(level.player_start);
        let mut view = WorldView {
            now: 0.0,
            players: vec![standing_player(player, start)],
            ais: Vec::new(),
            nav: Some(nav),
            level: Some(level),
            barriers: Vec::new(),
        };
        let mut ais = Vec::new();
        for (k, at) in spots.iter().enumerate() {
            let e = ecs.spawn_empty().id();
            let feet = ue::to_ue(*at);
            let body = Body {
                feet,
                yaw: ue::yaw_of(start - feet),
                radius: 150.0,
                half_height: 220.0,
                health: 150.0,
                mesh_scale: 0.8,
                forms: None,
            };
            let ai = new_ai(&mut world, &view, e, 0, b.clone(), body, 21 + k as u64)
                .expect("a soldier's AI");
            ais.push((e, ai));
        }
        let sounds = world.sounds.clone();
        let dt = 1.0 / 30.0;
        let mut alert_left = 0.0;
        let mut trails = vec![Vec::new(); spots.len()];
        for frame in 0..(secs * 30.0) as usize {
            world.shared.now += dt;
            view.now = world.shared.now;
            alert_left -= dt;
            let alerting = (alert_left <= 0.0).then(|| (player, view.players[0].loc));
            if alerting.is_some() {
                alert_left = ALERT_SECS;
            }
            let mut slots: Vec<Slot> = ais
                .iter_mut()
                .map(|(e, ai)| Slot {
                    entity: *e,
                    ai,
                    dormant: false,
                })
                .collect();
            view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
            run_frame(
                &mut world.shared,
                &sounds,
                &mut view,
                &mut slots,
                dt,
                alerting,
            );
            world.shared.shots.clear();
            world.shared.launches.clear();
            world.shared.loops.clear();
            world.shared.player_damage.clear();
            if frame % 15 == 0 {
                for (k, (_, ai)) in ais.iter().enumerate() {
                    trails[k].push(ue::from_ue(ai.pawn.loc - Vec3::Z * ai.pawn.half_height));
                }
            }
        }
        trails
    }

    /// Where a player is on Oblivion's navigation mesh (FindNavMeshLocation): on
    /// the hall's floor, his feet; jumping over it, the floor under him; a jet 30 m
    /// over it, the walkway 15 m up (the first ground the trace finds); out of the
    /// map, nowhere; on the lid over the ramp out of the enemies' sunken spawn
    /// area (players only, the ramp 1.9 m under it), nowhere, though the ramp's
    /// cell is under his feet.
    #[test]
    fn players_are_on_the_mesh_where_the_enemies_walk() {
        let dir = crate::assets_dir();
        let Some(map) = crate::MAPS.iter().find(|m| m.stem == "oblivion") else {
            return;
        };
        let (Some(level), Some(nav)) =
            (crate::load_level_data(map), NavMesh::load(&dir, "oblivion"))
        else {
            eprintln!("Oblivion's level or navigation not built: skipped");
            return;
        };
        let hall = Vec3::new(-6.9, -1.2151, -98.0);
        let on =
            navigable_location(&nav, Some(&level), hall, 0.6, false).expect("on the hall's floor");
        assert!(on.distance(hall) < 1e-4);
        let jumping = navigable_location(&nav, Some(&level), hall + Vec3::Y * 3.0, 0.6, true)
            .expect("over the hall's floor");
        assert!(jumping.distance(hall) < 0.01, "{jumping}");
        let jet = navigable_location(&nav, Some(&level), hall + Vec3::Y * 30.0, 0.6, true)
            .expect("over the walkway");
        assert!(jet.y > 10.0 && jet.y < 20.0, "{jet}");
        assert!(
            navigable_location(&nav, Some(&level), Vec3::new(900.0, 30.0, 0.0), 0.6, true)
                .is_none()
        );
        let lid = Vec3::new(-146.0, 0.0035, 100.0);
        assert!(nav.cell_at(lid).is_some());
        assert!(navigable_location(&nav, Some(&level), lid, 0.6, false).is_none());
        // (Without the floors to check, the ramp's cell is still too far under his.)
        assert!(navigable_location(&nav, None, lid, 0.6, false).is_none());
        assert!(navigable_location(&nav, None, hall, 0.6, false).is_some());
        // (Past the lid, on the arena's floor the ramp comes up to.)
        let arena = Vec3::new(-158.0, 0.0035, 100.0);
        assert!(navigable_location(&nav, Some(&level), arena, 0.6, false).is_some());
    }

    /// The maps' spawn walls (their HmPlayerOnlyBlockingVolumes: walls to the
    /// players alone, IgnoreBlockingBy letting the AI's pawns through) keep none
    /// of the soldiers in. Along the cells the navigation takes from each spawn
    /// point to the players' start, a soldier is stopped nowhere a player isn't
    /// but for those walls; and soldiers brought in behind them come out past
    /// them. (FOC_OLD_COL, a folder of old <map>_col.bin files, and FOC_OLD_DIR, a
    /// folder whose levels folder has the old doors: the same soldiers with the
    /// walls as everyone's, to compare.)
    #[test]
    fn soldiers_leave_their_spawns() {
        use crate::collision::{LevelCollision, Mover};
        use crate::nav::Moves;
        use std::path::Path;
        let dir = crate::assets_dir();
        let data = AiData::load(&dir);
        // The soldiers' size (enemies.txt's: 1.5 m round, 4.4 m tall).
        let (radius, height) = (1.5, 4.4);
        let moves = Moves::ground(radius, height);
        let mut walls_found = 0;
        for (stem, soldier) in [
            ("oblivion", "SoldierAut_Prime"),
            ("downfall", "SoldierAut_Prime"),
            ("ancients", "SoldierDec_Rust_Sea"),
            ("ignition", "SoldierDec_Kaon"),
        ] {
            let Some(map) = crate::MAPS.iter().find(|m| m.stem == stem) else {
                continue;
            };
            let (Some(level), Some(nav)) = (crate::load_level_data(map), NavMesh::load(&dir, stem))
            else {
                eprintln!("{stem}'s level or navigation not built: skipped");
                continue;
            };
            let Some(b) = data.behaviour(soldier) else {
                eprintln!("ai.txt not built: skipped");
                return;
            };
            let goal = level.player_start;
            let floor = |at: Vec3, down: f32| {
                level
                    .ground_for(at, radius, at.y + 1.0, at.y - down, Mover::Ai)
                    .map_or(at, |y| Vec3::new(at.x, y, at.z))
            };
            // The way to the start from a place (through its cells' middles).
            let way = |from: Vec3| -> Option<Vec<Vec3>> {
                let cell = nav.cell_at(from)?;
                let channel = nav.find_channel(cell, goal, &moves)?;
                let mut out = vec![from];
                out.extend(channel.iter().map(|&i| nav.points[i as usize].at));
                out.push(goal);
                Some(out)
            };
            let length = |w: &[Vec3]| w.windows(2).map(|s| (s[1] - s[0]).length()).sum::<f32>();
            let stops = |at: Vec3, who: Mover| {
                level
                    .push_out_for(floor(at, 3.0), radius, height, who)
                    .is_some_and(|v| v.length() > 0.05)
            };
            // Its spawn points (the waves' ground points: cluster, x y z).
            let text = std::fs::read_to_string(dir.join(format!("levels/{stem}_waves.txt")))
                .unwrap_or_default();
            let points: Vec<(usize, Vec3)> = text
                .lines()
                .map(|l| l.split('\t').collect::<Vec<_>>())
                .filter(|f| f[0] == "point" && f.len() >= 5)
                .filter_map(|f| {
                    let v: Vec<f32> = f[1..5].iter().filter_map(|x| x.parse().ok()).collect();
                    (v.len() == 4).then(|| (v[0] as usize, floor(Vec3::new(v[1], v[2], v[3]), 8.0)))
                })
                .collect();
            // Each one's way out (its first 40 m): where a player is stopped and
            // the soldier isn't (a spawn wall: how far along, and the way left
            // from there), and whether anything stops the soldier.
            let mut walled: Vec<(usize, Vec3, f32, f32)> = Vec::new();
            let mut ways = 0;
            for &(cluster, at) in &points {
                let Some(w) = way(at) else {
                    continue;
                };
                ways += 1;
                let total = length(&w);
                let mut along = 0.0;
                let mut wall = None;
                let mut ai_stops = 0;
                'way: for s in w.windows(2) {
                    let d = (s[1] - s[0]).length();
                    let steps = (d / 0.25).ceil().max(1.0) as usize;
                    for k in 0..steps {
                        let a = along + d * k as f32 / steps as f32;
                        if a > 40.0 {
                            break 'way;
                        }
                        let p = s[0].lerp(s[1], k as f32 / steps as f32);
                        let ai = stops(p, Mover::Ai);
                        ai_stops += ai as usize;
                        if !ai && wall.is_none() && stops(p, Mover::Player) {
                            wall = Some(a);
                        }
                    }
                    along += d;
                }
                eprintln!(
                    "  {stem} cluster {cluster:2} at ({:7.2},{:6.2},{:7.2}): way {total:6.1} m, \
                     spawn wall {:?}, the soldier stopped at {ai_stops} places in its first 40 m",
                    at.x,
                    at.y,
                    at.z,
                    wall.map(|a| (a * 10.0).round() / 10.0)
                );
                if let Some(a) = wall {
                    walled.push((cluster, at, a, total - a));
                }
            }
            eprintln!(
                "{stem}: {} spawn points, {ways} with a way to the start, {} behind a spawn wall",
                points.len(),
                walled.len()
            );
            if walled.is_empty() {
                continue;
            }
            walls_found += 1;
            // Soldiers at the walled points of four clusters, the nearest the start.
            walled.sort_by(|a, b| a.3.total_cmp(&b.3));
            let mut chosen: Vec<(usize, Vec3, f32, f32)> = Vec::new();
            for w in &walled {
                if chosen.len() < 4 && chosen.iter().all(|c| c.0 != w.0) {
                    chosen.push(*w);
                }
            }
            let spots: Vec<Vec3> = chosen.iter().map(|c| c.1).collect();
            // Out: nearer the start along the way than the wall is, or further
            // from its spawn point than the wall is along the way.
            let escaped = |trails: &[Vec<Vec3>], label: &str| -> usize {
                let mut out = 0;
                for (c, trail) in chosen.iter().zip(trails) {
                    let left = trail
                        .iter()
                        .filter_map(|p| way(*p).map(|w| length(&w)))
                        .fold(f32::INFINITY, f32::min);
                    let moved = trail
                        .iter()
                        .map(|p| (*p - c.1).length())
                        .fold(0.0, f32::max);
                    let past = left < c.3 - 5.0 || moved > c.2 + 3.0;
                    out += past as usize;
                    eprintln!(
                        "  {label}: {stem} cluster {:2}: wall {:.1} m out, {:.1} m from the start \
                         there; the soldier went {moved:.1} m, {left:.1} m from the start at best: {}",
                        c.0,
                        c.2,
                        c.3,
                        if past { "came out" } else { "held in" }
                    );
                }
                out
            };
            let trails = soldiers_run(&level, &nav, &data, &b, &spots, 30.0);
            let out = escaped(&trails, "now");
            // The old collision, if named: the walls everyone's.
            let old = std::env::var("FOC_OLD_COL").ok().and_then(|col| {
                let path = Path::new(&col).join(format!("{stem}_col.bin"));
                let c = LevelCollision::load(&path).ok()?;
                let doors = std::env::var("FOC_OLD_DIR").ok()?;
                let (_, c) = crate::doors::DoorsData::load(Path::new(&doors), stem, c);
                let (pods, solid) = crate::recreator::load_pods(&dir.join(map.pods()));
                let c = if solid {
                    c.with_solids(&crate::recreator::pod_boxes(&pods))
                } else {
                    c
                };
                let mut l = crate::load_level_data(map)?;
                l.collision = Some(c);
                Some(l)
            });
            if let Some(old) = &old {
                let trails = soldiers_run(old, &nav, &data, &b, &spots, 30.0);
                let before = escaped(&trails, "before");
                eprintln!(
                    "{stem}: came out {out} of {} now, {before} before",
                    spots.len()
                );
            }
            assert!(
                out * 3 >= spots.len() * 2,
                "{stem}: only {out} of {} soldiers came out past their spawn walls",
                spots.len()
            );
        }
        eprintln!("{walls_found} maps with spawn walls on the soldiers' ways");
    }

    /// A soldier's rounds at a player behind a Diffraction Barrier stop at the
    /// barrier (world geometry to them: TnBarrierSpawnable), which takes their
    /// damage; without it they strike him.
    #[test]
    fn soldiers_rounds_stop_at_barriers() {
        let dir = crate::assets_dir();
        let data = AiData::load(&dir);
        let Some(b) = data.behaviour("SoldierDec_Kaon") else {
            eprintln!("ai.txt not built: skipped");
            return;
        };
        let mut ecs = bevy::ecs::world::World::new();
        let mut world = AiWorld {
            data: Arc::new(data.clone()),
            loaded: true,
            ..Default::default()
        };
        let player = ecs.spawn_empty().id();
        let shield = ecs.spawn_empty().id();
        // The player at the origin, the soldier 20 m off along +X (Unreal's: the
        // demo's +X too).
        let mut view = WorldView {
            players: vec![standing_player(player, Vec3::ZERO)],
            ..Default::default()
        };
        let e = ecs.spawn_empty().id();
        let body = Body {
            feet: Vec3::new(2000.0, 0.0, 0.0),
            yaw: std::f32::consts::PI,
            radius: 150.0,
            half_height: 220.0,
            health: 150.0,
            mesh_scale: 0.8,
            forms: None,
        };
        let mut ai = new_ai(&mut world, &view, e, 0, b.clone(), body, 5).expect("a soldier's AI");
        let volley = |view: &WorldView, ai: &mut Ai| {
            ai.pawn.muzzle = Vec3::new(1900.0, 0.0, 300.0);
            ai.pawn.aimer.mode = 0;
            ai.pawn.aimer.target_location = view.players[0].loc;
            let (mut shots, mut damage, mut hits, mut sounds, mut launches, mut loops) = (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            fire(
                &mut ai.brain,
                &mut ai.pawn,
                view,
                1.0 / 30.0,
                vec![false; 20],
                (
                    &mut shots,
                    &mut damage,
                    &mut hits,
                    &mut sounds,
                    &mut launches,
                    &mut loops,
                ),
                (&mut Vec::new(), &mut Vec::new()),
            );
            (shots, damage, hits)
        };
        let (shots, damage, hits) = volley(&view, &mut ai);
        assert_eq!(shots.len(), 20);
        assert!(!damage.is_empty(), "no round struck him");
        assert!(hits.is_empty());
        // A barrier 8 m out from him, facing the soldier.
        view.barriers = vec![(
            shield,
            crate::equipment::barrier_boxes(Vec3::new(8.0, 0.0, 0.0), Vec3::X, Vec3::Z, 1.0),
        )];
        let (shots, damage, hits) = volley(&view, &mut ai);
        assert!(damage.is_empty(), "rounds went through the barrier");
        assert_eq!(hits.len(), 20);
        assert!(hits.iter().all(|(b, amount)| *b == shield && *amount > 0.0));
        // (They stop at its face, 10.1 m out: its impacts there.)
        for s in &shots {
            assert!((s.to.x - 1010.0).abs() < 5.0, "{:?}", s.to);
            assert!(s.hit_normal.is_some_and(|n| n.x > 0.99));
        }
    }

    /// (A report, run by hand: FOC_STUCK="map:kinds:secs:x,y,z" — the kinds
    /// comma-separated, the player's feet in the demo's space; by default
    /// Downfall's fourth wave's ground enemies against a player at each heavy
    /// ammo station.) Enemies brought in at the waves' spawn points nearest the
    /// player, run for the seconds; each time one is held within 0.5 m for 3 s
    /// while it walks (its path follower walking or stepping back on to the mesh),
    /// where, what it was doing and what it walked to. (FOC_STUCK_BOUGHT="143,9844":
    /// those consoles' purchases' navigation toggles made first.) A Sniper's place
    /// and task are traced too (FOC_STUCK_TRACE).
    #[test]
    #[ignore]
    fn stuck_hunt() {
        let arg = std::env::var("FOC_STUCK").unwrap_or_default();
        let f: Vec<&str> = arg.split(':').collect();
        let stem = f
            .first()
            .filter(|s| !s.is_empty())
            .copied()
            .unwrap_or("downfall");
        let ids: Vec<&str> = f
            .get(1)
            .filter(|s| !s.is_empty())
            .map(|s| s.split(',').collect())
            .unwrap_or_else(|| {
                vec![
                    "RocketTrooper_AUT",
                    "ShotgunnerAut",
                    "ScrapperAut",
                    "SoldierAut_Prime",
                ]
            });
        let secs: f32 = f.get(2).and_then(|s| s.parse().ok()).unwrap_or(60.0);
        let dir = crate::assets_dir();
        let map = crate::MAPS
            .iter()
            .find(|m| m.stem == stem)
            .expect("the map");
        let (Some(level), Some(mut nav)) = (crate::load_level_data(map), NavMesh::load(&dir, stem))
        else {
            eprintln!("{stem}: not built");
            return;
        };
        for console in std::env::var("FOC_STUCK_BOUGHT")
            .unwrap_or_default()
            .split(',')
        {
            if console.is_empty() {
                continue;
            }
            for t in nav.bought(&format!("TnInteractCurrencyReceptacle_{console}")) {
                nav.switch(&t);
            }
        }
        // (The air network's flights measured against the map, as the game does as
        // the match starts: FOC_STUCK_UNMEASURED, as they were.)
        if std::env::var_os("FOC_STUCK_UNMEASURED").is_none()
            && let Some(c) = level.collision.as_ref()
        {
            while !nav.flights_measured() {
                nav.measure_flights(c, std::time::Duration::from_secs(1));
            }
        }
        let text = std::fs::read_to_string(dir.join(format!("levels/{stem}_points.txt")))
            .unwrap_or_default();
        let players: Vec<Vec3> = match f.get(3) {
            Some(at) => {
                let v: Vec<f32> = at.split(',').filter_map(|x| x.parse().ok()).collect();
                vec![Vec3::new(v[0], v[1], v[2])]
            }
            None => text
                .lines()
                .map(|l| l.split_whitespace().collect::<Vec<_>>())
                .filter(|f| f.first() == Some(&"heavyammo"))
                .map(|f| {
                    Vec3::new(
                        f[1].parse().unwrap(),
                        f[2].parse().unwrap(),
                        f[3].parse().unwrap(),
                    )
                })
                .collect(),
        };
        let waves = std::fs::read_to_string(dir.join(format!("levels/{stem}_waves.txt")))
            .unwrap_or_default();
        let spawns: Vec<Vec3> = waves
            .lines()
            .map(|l| l.split('\t').collect::<Vec<_>>())
            .filter(|f| f[0] == "point" && f.len() >= 5)
            .filter_map(|f| {
                let v: Vec<f32> = f[2..5].iter().filter_map(|x| x.parse().ok()).collect();
                (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
            })
            .collect();
        let data = AiData::load(&dir);
        let kinds = crate::enemies::Kinds::load(&dir);
        let sounds = Arc::new(audio::AiSounds::load(&dir));
        let mut held = 0;
        for (round, &player_at) in players.iter().enumerate() {
            let mut ecs = bevy::ecs::world::World::new();
            let mut world = AiWorld {
                data: Arc::new(data.clone()),
                sounds: sounds.clone(),
                loaded: true,
                ..Default::default()
            };
            world.shared.tactical.reset(Some(&nav));
            let player = ecs.spawn_empty().id();
            let feet = ue::to_ue(player_at);
            let mut view = WorldView {
                players: vec![standing_player(player, feet)],
                nav: Some(&nav),
                level: Some(&level),
                ..Default::default()
            };
            // The spawn points nearest him, two of each kind (or the places
            // named, x,y,z;x,y,z, one of each kind in turn at each).
            let named: Vec<Vec3> = f
                .get(4)
                .map(|s| {
                    s.split(';')
                        .filter_map(|p| {
                            let v: Vec<f32> = p.split(',').filter_map(|x| x.parse().ok()).collect();
                            (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut near: Vec<Vec3> = spawns.clone();
            near.sort_by(|a, b| a.distance(player_at).total_cmp(&b.distance(player_at)));
            let count = if named.is_empty() {
                ids.len() * 2
            } else {
                named.len()
            };
            if !named.is_empty() {
                near = named.clone();
            }
            let mut ais: Vec<(Entity, Ai, &str)> = Vec::new();
            for (n, at) in near.iter().take(count).enumerate() {
                let id = ids[n % ids.len()];
                let Some(b) = data.behaviour(id) else {
                    eprintln!("no behaviour for {id}");
                    continue;
                };
                let k = kinds.kinds.iter().find(|k| k.id == id).expect("its kind");
                let at = level
                    .ground_for(
                        *at + Vec3::Y,
                        k.radius,
                        at.y + 1.0,
                        at.y - 8.0,
                        crate::collision::Mover::Ai,
                    )
                    .map_or(*at, |y| Vec3::new(at.x, y, at.z));
                let body = Body {
                    feet: ue::to_ue(at) + Vec3::Z * pawn::FLOOR_DIST,
                    yaw: ue::yaw_of(feet - ue::to_ue(at)),
                    radius: k.radius * ue::UU_PER_M,
                    half_height: k.half_height * ue::UU_PER_M,
                    health: k.health,
                    mesh_scale: k.scale,
                    forms: forms_of(&kinds, k),
                };
                let e = ecs.spawn_empty().id();
                if let Some(ai) = new_ai(&mut world, &view, e, 0, b.clone(), body, 77 + n as u64) {
                    ais.push((e, ai, id));
                }
            }
            eprintln!(
                "player at {:.1} {:.1} {:.1}: {} enemies",
                player_at.x,
                player_at.y,
                player_at.z,
                ais.len()
            );
            let dt = 1.0 / 30.0;
            let mut alert_left = 0.0;
            // Each one's place each half second, and whether it was walking.
            let mut track: Vec<Vec<(Vec3, bool)>> = vec![Vec::new(); ais.len()];
            let mut reported = vec![-100.0f32; ais.len()];
            for frame in 0..(secs * 30.0) as usize {
                world.shared.now += dt;
                view.now = world.shared.now;
                alert_left -= dt;
                let alerting = (alert_left <= 0.0).then(|| (player, view.players[0].loc));
                if alerting.is_some() {
                    alert_left = ALERT_SECS;
                }
                let mut slots: Vec<Slot> = ais
                    .iter_mut()
                    .map(|(e, ai, _)| Slot {
                        entity: *e,
                        ai,
                        dormant: false,
                    })
                    .collect();
                view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
                run_frame(
                    &mut world.shared,
                    &sounds,
                    &mut view,
                    &mut slots,
                    dt,
                    alerting,
                );
                world.shared.shots.clear();
                world.shared.launches.clear();
                world.shared.loops.clear();
                world.shared.player_damage.clear();
                world.shared.effects.clear();
                world.shared.effects_stopped.clear();
                // (FOC_STUCK_MOVE="dx,dz": each moved so far a second in, as a
                // shove would move it.)
                if frame == 30
                    && let Ok(m) = std::env::var("FOC_STUCK_MOVE")
                {
                    let v: Vec<f32> = m.split(',').filter_map(|x| x.parse().ok()).collect();
                    for (_, ai, _) in ais.iter_mut() {
                        ai.pawn.loc += ue::dir_to_ue(Vec3::new(v[0], 0.0, v[1])) * ue::UU_PER_M;
                    }
                }
                if frame % 15 != 0 {
                    continue;
                }
                let now = world.shared.now;
                for (k, (_, ai, id)) in ais.iter().enumerate() {
                    let state = ai.brain.sensors.path.as_ref().map(|p| p.state);
                    let walking = matches!(
                        state,
                        Some(path::MotionState::Walking | path::MotionState::ReturningToNavMesh)
                    ) || ai.pawn.motion.is_in_motion();
                    let at = ue::from_ue(ai.pawn.loc);
                    track[k].push((at, walking));
                    // (FOC_STUCK_TRACE: each one's place, physics and task every 2 s.)
                    if std::env::var_os("FOC_STUCK_TRACE").is_some() && frame % 60 == 0 {
                        let op = ai
                            .brain
                            .runner
                            .active
                            .as_ref()
                            .map(|o| ai.brain.core.d.name(o.task.symbol).to_string())
                            .unwrap_or_default();
                        let next = ai.brain.sensors.sniper_store.as_ref().and_then(|s| {
                            s.next_point()
                                .map(|p| (p, s.path.len(), nav.points[p as usize].at))
                        });
                        eprintln!(
                            "  t={now:5.1} {id} #{k} at {:.1} {:.1} {:.1} {:?} {op} next {next:?}",
                            at.x, at.y, at.z, ai.pawn.physics
                        );
                    }
                    let t = &track[k];
                    if t.len() < 7 || now - reported[k] < 5.0 {
                        continue;
                    }
                    let last = &t[t.len() - 7..];
                    let still = last.iter().all(|(p, w)| *w && (*p - at).length() < 0.5);
                    if !still {
                        continue;
                    }
                    reported[k] = now;
                    held += 1;
                    let op = ai
                        .brain
                        .runner
                        .active
                        .as_ref()
                        .map(|o| ai.brain.core.d.name(o.task.symbol).to_string())
                        .unwrap_or_default();
                    let p = ai.brain.sensors.path.as_ref().unwrap();
                    let cell_here = nav.cell_at(at);
                    eprintln!(
                        "  t={now:5.1} {id} #{k} held at {:.2} {:.2} {:.2} ({:.1} m from him): {:?}, \
                         {op}, anchor {:?} (cell here {:?}), corner {:.2} {:.2} {:.2}, path end {:.1} {:.1} {:.1}, push {:?}",
                        at.x,
                        at.y,
                        at.z,
                        at.distance(player_at),
                        state,
                        ai.pawn.anchor,
                        cell_here,
                        p.next_corner.at.x,
                        p.next_corner.at.y,
                        p.next_corner.at.z,
                        ue::from_ue(p.path_end).x,
                        ue::from_ue(p.path_end).y,
                        ue::from_ue(p.path_end).z,
                        level.push_out_for(
                            at - Vec3::Y * (ai.pawn.half_height / ue::UU_PER_M),
                            ai.pawn.radius / ue::UU_PER_M,
                            2.0 * ai.pawn.half_height / ue::UU_PER_M,
                            crate::collision::Mover::Ai
                        ),
                    );
                }
            }
            for (k, (_, ai, id)) in ais.iter().enumerate() {
                let t = &track[k];
                let start = t.first().map_or(Vec3::ZERO, |p| p.0);
                let walked: f32 = t.windows(2).map(|w| (w[1].0 - w[0].0).length()).sum();
                let at = ue::from_ue(ai.pawn.loc);
                eprintln!(
                    "  {id} #{k}: from {:.1} {:.1} {:.1} ({:.1} m from him) walked {walked:.1} m, \
                     now {:.1} {:.1} {:.1} ({:.1} m), walking {} of {} half seconds",
                    start.x,
                    start.y,
                    start.z,
                    start.distance(player_at),
                    at.x,
                    at.y,
                    at.z,
                    at.distance(player_at),
                    t.iter().filter(|p| p.1).count(),
                    t.len()
                );
                if walked < 1.0 {
                    let op = ai
                        .brain
                        .runner
                        .active
                        .as_ref()
                        .map(|o| ai.brain.core.d.name(o.task.symbol).to_string())
                        .unwrap_or_default();
                    let plan: Vec<&str> = ai
                        .brain
                        .runner
                        .plan
                        .iter()
                        .map(|t| ai.brain.core.d.name(t.symbol))
                        .collect();
                    let hist: Vec<String> = ai
                        .brain
                        .runner
                        .history
                        .iter()
                        .rev()
                        .take(4)
                        .map(|(g, p, _, st)| {
                            format!(
                                "{} {:?} -> {}",
                                ai.brain.core.d.name(*g),
                                p.iter()
                                    .map(|s| ai.brain.core.d.name(*s))
                                    .collect::<Vec<_>>(),
                                st
                            )
                        })
                        .collect();
                    eprintln!(
                        "    still: {op} of {plan:?}; anchor {:?}; last plans {hist:?}",
                        ai.pawn.anchor
                    );
                }
            }
            let _ = round;
        }
        eprintln!("{held} times held");
    }

    /// (A report, run by hand: FOC_IDLE="map:kinds:secs:x,y,z:metres" — the kinds
    /// comma-separated, the place the player walks round (the demo's space) and how
    /// far from it; by default Ignition's soldiers round each of its health stations
    /// on the ground floor, 8 m. FOC_IDLE_COUNT: how many come in (6), FOC_IDLE_SEEDS:
    /// how many runs at each place (2), FOC_IDLE_CIRCLE=<metres>: he walks round the
    /// nearest of them he sees within 20 m that far from it instead, while there is
    /// one.) Enemies brought in at the waves' spawn points nearest the place, the
    /// player walking round it from one open spot to the next; each time one has
    /// stood within a metre for 6 s in his sight, within 35 m, without a round or a
    /// blow, where, what it was doing and what it knew of him.
    #[test]
    #[ignore]
    fn idle_hunt() {
        use crate::collision::Mover;
        let arg = std::env::var("FOC_IDLE").unwrap_or_default();
        let f: Vec<&str> = arg.split(':').collect();
        let stem = f
            .first()
            .filter(|s| !s.is_empty())
            .copied()
            .unwrap_or("ignition");
        let ids: Vec<&str> = f
            .get(1)
            .filter(|s| !s.is_empty())
            .map(|s| s.split(',').collect())
            .unwrap_or_else(|| vec!["SoldierDec_Kaon"]);
        let secs: f32 = f.get(2).and_then(|s| s.parse().ok()).unwrap_or(120.0);
        let metres: f32 = f.get(4).and_then(|s| s.parse().ok()).unwrap_or(8.0);
        let env_n = |k: &str, d: usize| {
            std::env::var(k)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(d)
        };
        let count = env_n("FOC_IDLE_COUNT", 6);
        let seeds = env_n("FOC_IDLE_SEEDS", 2);
        let dir = crate::assets_dir();
        let map = crate::MAPS
            .iter()
            .find(|m| m.stem == stem)
            .expect("the map");
        let (Some(level), Some(mut nav)) = (crate::load_level_data(map), NavMesh::load(&dir, stem))
        else {
            eprintln!("{stem}: not built");
            return;
        };
        if let Some(c) = level.collision.as_ref() {
            while !nav.flights_measured() {
                nav.measure_flights(c, std::time::Duration::from_secs(1));
            }
        }
        let text = std::fs::read_to_string(dir.join(format!("levels/{stem}_points.txt")))
            .unwrap_or_default();
        let centres: Vec<Vec3> = match f.get(3).filter(|s| !s.is_empty()) {
            Some(at) => {
                let v: Vec<f32> = at.split(',').filter_map(|x| x.parse().ok()).collect();
                vec![Vec3::new(v[0], v[1], v[2])]
            }
            None => text
                .lines()
                .map(|l| l.split_whitespace().collect::<Vec<_>>())
                .filter(|f| f.first() == Some(&"health"))
                .map(|f| {
                    Vec3::new(
                        f[1].parse().unwrap(),
                        f[2].parse().unwrap(),
                        f[3].parse().unwrap(),
                    )
                })
                .filter(|p| p.y < 5.0)
                .collect(),
        };
        let waves = std::fs::read_to_string(dir.join(format!("levels/{stem}_waves.txt")))
            .unwrap_or_default();
        let spawns: Vec<Vec3> = waves
            .lines()
            .map(|l| l.split('\t').collect::<Vec<_>>())
            .filter(|f| f[0] == "point" && f.len() >= 5)
            .filter_map(|f| {
                let v: Vec<f32> = f[2..5].iter().filter_map(|x| x.parse().ok()).collect();
                (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
            })
            .collect();
        let data = AiData::load(&dir);
        let kinds = crate::enemies::Kinds::load(&dir);
        let sounds = Arc::new(audio::AiSounds::load(&dir));
        let mut idle = 0;
        for &centre in &centres {
            // His round: spots that far from the place every 30 degrees, open (on
            // the floor, on the mesh, out of the walls), walked one to the next.
            let mut ring: Vec<Vec3> = Vec::new();
            for k in 0..12 {
                let a = k as f32 * std::f32::consts::TAU / 12.0;
                let p = centre + Vec3::new(a.cos(), 0.0, a.sin()) * metres;
                let Some(y) =
                    level.ground_for(p + Vec3::Y, 0.5, p.y + 1.5, p.y - 3.0, Mover::Player)
                else {
                    continue;
                };
                let p = Vec3::new(p.x, y, p.z);
                if nav.cell_at(p).is_none()
                    || level.push_out_for(p, 0.6, 2.2, Mover::Player).is_some()
                {
                    continue;
                }
                ring.push(p);
            }
            if ring.is_empty() {
                ring.push(centre);
            }
            eprintln!(
                "place {:.1} {:.1} {:.1}: his round {} spots",
                centre.x,
                centre.y,
                centre.z,
                ring.len()
            );
            for seed in 0..seeds as u64 {
                let mut ecs = bevy::ecs::world::World::new();
                let mut world = AiWorld {
                    data: Arc::new(data.clone()),
                    sounds: sounds.clone(),
                    loaded: true,
                    ..Default::default()
                };
                world.shared.tactical.reset(Some(&nav));
                let player = ecs.spawn_empty().id();
                let mut at = ring[0];
                let mut view = WorldView {
                    players: vec![standing_player(player, ue::to_ue(at))],
                    nav: Some(&nav),
                    level: Some(&level),
                    ..Default::default()
                };
                let mut near: Vec<Vec3> = spawns.clone();
                near.sort_by(|a, b| a.distance(centre).total_cmp(&b.distance(centre)));
                let mut ais: Vec<(Entity, Ai, &str)> = Vec::new();
                for (n, sp) in near.iter().take(count).enumerate() {
                    let id = ids[n % ids.len()];
                    let Some(b) = data.behaviour(id) else {
                        eprintln!("no behaviour for {id}");
                        continue;
                    };
                    let k = kinds.kinds.iter().find(|k| k.id == id).expect("its kind");
                    let sp = level
                        .ground_for(*sp + Vec3::Y, k.radius, sp.y + 1.0, sp.y - 8.0, Mover::Ai)
                        .map_or(*sp, |y| Vec3::new(sp.x, y, sp.z));
                    let body = Body {
                        feet: ue::to_ue(sp) + Vec3::Z * pawn::FLOOR_DIST,
                        yaw: ue::yaw_of(ue::to_ue(centre) - ue::to_ue(sp)),
                        radius: k.radius * ue::UU_PER_M,
                        half_height: k.half_height * ue::UU_PER_M,
                        health: k.health,
                        mesh_scale: k.scale,
                        forms: forms_of(&kinds, k),
                    };
                    let e = ecs.spawn_empty().id();
                    if let Some(ai) = new_ai(
                        &mut world,
                        &view,
                        e,
                        0,
                        b.clone(),
                        body,
                        1000 * seed + 77 + n as u64,
                    ) {
                        ais.push((e, ai, id));
                    }
                }
                let dt = 1.0 / 30.0;
                let mut alert_left = 0.0;
                let mut next = 1 % ring.len();
                let circle: Option<f32> = std::env::var("FOC_IDLE_CIRCLE")
                    .ok()
                    .and_then(|v| v.parse().ok());
                let mut spin = 1.0f32;
                // Each one's rounds so far, and its samples each half second: the
                // time, its place, his sight of it, how far, its rounds.
                let mut fired = vec![0usize; ais.len()];
                type Sample = (f32, Vec3, bool, f32, usize);
                let mut track: Vec<Vec<Sample>> = vec![Vec::new(); ais.len()];
                let mut reported = vec![-100.0f32; ais.len()];
                for frame in 0..(secs * 30.0) as usize {
                    world.shared.now += dt;
                    view.now = world.shared.now;
                    // He walks his round at 4 m/s (straight on to the next spot if a
                    // wall is between them); with FOC_IDLE_CIRCLE=<metres>, round the
                    // nearest enemy he sees within 20 m that far from it instead,
                    // turning back where a wall stops him.
                    let target = ais
                        .iter()
                        .filter(|(_, a, _)| !a.pawn.dying)
                        .map(|(_, a, _)| ue::from_ue(a.pawn.loc))
                        .filter(|c| {
                            Vec3::new(c.x - at.x, 0.0, c.z - at.z).length() < 20.0
                                && level
                                    .ray_hit(
                                        at + Vec3::Y * 2.0,
                                        (*c - at - Vec3::Y * 2.0).normalize_or_zero(),
                                        (*c - at - Vec3::Y * 2.0).length(),
                                    )
                                    .is_none()
                        })
                        .min_by(|a, b| a.distance(at).total_cmp(&b.distance(at)))
                        .zip(circle);
                    let to = match target {
                        Some((c, r)) => {
                            let flat = Vec3::new(at.x - c.x, 0.0, at.z - c.z);
                            let a0 = flat.z.atan2(flat.x);
                            let a1 = a0 + spin * 4.0 * dt / r;
                            let want = Vec3::new(c.x + r * a1.cos(), at.y, c.z + r * a1.sin());
                            if level.push_out_for(want, 0.6, 2.2, Mover::Player).is_some()
                                || nav.cell_at(want).is_none()
                            {
                                spin = -spin;
                                at
                            } else {
                                want
                            }
                        }
                        None => ring[next],
                    };
                    let d = to - at;
                    let step = if target.is_some() { 6.0 * dt } else { 4.0 * dt };
                    let mut vel = Vec3::ZERO;
                    if target.is_some() {
                        if d.length() > 1e-4 {
                            let s = d.length().min(step);
                            vel = d.normalize() * s / dt;
                            at += d.normalize() * s;
                            if let Some(y) = level.ground_for(
                                at + Vec3::Y,
                                0.5,
                                at.y + 1.0,
                                at.y - 2.0,
                                Mover::Player,
                            ) {
                                at.y = y;
                            }
                        }
                    } else if d.length() <= step
                        || level
                            .ray_hit(at + Vec3::Y, d.normalize(), d.length())
                            .is_some()
                    {
                        at = to;
                        next = (next + 1) % ring.len();
                    } else {
                        vel = d.normalize() * 4.0;
                        at += vel * dt;
                        if let Some(y) = level.ground_for(
                            at + Vec3::Y,
                            0.5,
                            at.y + 1.0,
                            at.y - 2.0,
                            Mover::Player,
                        ) {
                            at.y = y;
                        }
                    }
                    {
                        let feet = ue::to_ue(at);
                        let p = &mut view.players[0];
                        p.feet = feet;
                        p.loc = feet + Vec3::Z * 110.0;
                        p.eyes = feet + Vec3::Z * 198.0;
                        p.nav_near = feet;
                        p.nav_near_time = world.shared.now;
                        p.vel = ue::dir_to_ue(vel) * ue::UU_PER_M;
                        if vel != Vec3::ZERO {
                            p.yaw = ue::yaw_of(ue::dir_to_ue(vel));
                        }
                    }
                    alert_left -= dt;
                    let alerting = (alert_left <= 0.0).then(|| (player, view.players[0].loc));
                    if alerting.is_some() {
                        alert_left = ALERT_SECS;
                    }
                    let mut slots: Vec<Slot> = ais
                        .iter_mut()
                        .map(|(e, ai, _)| Slot {
                            entity: *e,
                            ai,
                            dormant: false,
                        })
                        .collect();
                    view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
                    run_frame(
                        &mut world.shared,
                        &sounds,
                        &mut view,
                        &mut slots,
                        dt,
                        alerting,
                    );
                    for (k, (e, _, _)) in ais.iter().enumerate() {
                        fired[k] += world
                            .shared
                            .shots
                            .iter()
                            .filter(|s| s.shooter == *e)
                            .count()
                            + world.shared.launches.iter().filter(|l| l.by == *e).count()
                            + world
                                .shared
                                .player_damage
                                .iter()
                                .filter(|d| d.by == *e)
                                .count();
                    }
                    world.shared.shots.clear();
                    world.shared.launches.clear();
                    world.shared.loops.clear();
                    world.shared.player_damage.clear();
                    world.shared.effects.clear();
                    world.shared.effects_stopped.clear();
                    if frame % 15 != 0 {
                        continue;
                    }
                    let now = world.shared.now;
                    let eyes = view.players[0].eyes;
                    for (k, (_, ai, id)) in ais.iter().enumerate() {
                        let p = &ai.pawn;
                        let mid = p.loc;
                        let its_eyes = p.eyes();
                        let seen = view.fast_trace(eyes, its_eyes) || view.fast_trace(eyes, mid);
                        let dist = ue::from_ue(mid).distance(at);
                        let here = ue::from_ue(mid);
                        track[k].push((now, here, seen, dist, fired[k]));
                        let t = &track[k];
                        if t.len() < 13 || now - reported[k] < 10.0 {
                            continue;
                        }
                        let last = &t[t.len() - 13..];
                        let held = last.iter().all(|s| {
                            (s.1 - here).length() < 1.0 && s.2 && s.3 < 35.0 && s.4 == fired[k]
                        });
                        if !held {
                            continue;
                        }
                        reported[k] = now;
                        idle += 1;
                        let core = &ai.brain.core;
                        let run = &ai.brain.runner;
                        let op = run
                            .active
                            .as_ref()
                            .map(|o| core.d.name(o.task.symbol).to_string())
                            .unwrap_or_default();
                        let plan: Vec<String> = run
                            .plan
                            .iter()
                            .enumerate()
                            .map(|(i, t)| {
                                format!(
                                    "{}{}",
                                    if i == run.index { "*" } else { "" },
                                    core.d.name(t.symbol)
                                )
                            })
                            .collect();
                        let hist: Vec<String> = run
                            .history
                            .iter()
                            .rev()
                            .take(4)
                            .map(|(g, ts, _, st)| {
                                format!(
                                    "{} {:?} -> {:?}",
                                    core.d.name(*g),
                                    ts.iter().map(|s| core.d.name(*s)).collect::<Vec<_>>(),
                                    st
                                )
                            })
                            .collect();
                        // (Its eyes in a wall? What its own sight of him says.)
                        let e_m = ue::from_ue(its_eyes);
                        let eyes_in = level.push_out_for(e_m - Vec3::Y * 0.2, 0.2, 0.4, Mover::Ai);
                        let sees = view.fast_trace(its_eyes, eyes)
                            || view.fast_trace(its_eyes, view.players[0].loc);
                        let facing =
                            ue::normal(view.players[0].loc - p.loc).dot(p.look_direction());
                        let ws = |i| core.ws(i);
                        eprintln!(
                            "  seed {seed} t={now:5.1} {id} #{k} idle at {:.2} {:.2} {:.2} ({dist:.1} m from him at {:.1} {:.1} {:.1}), \
                             {} rounds: op {op} plan [{}] goal {} enemy {:?} alert {} | ws visible {} range {} clear_lof {} equipped {} ammo {} suspicious {} alert {} cover {} location {} | its sight of him {sees} facing {facing:.2} eyes {:.2} {:.2} {:.2} in wall {:?} | stance {} phys {:?} | history {hist:?}",
                            here.x,
                            here.y,
                            here.z,
                            at.x,
                            at.y,
                            at.z,
                            fired[k],
                            plan.join(" "),
                            core.d.name(run.goal.symbol),
                            core.enemy,
                            core.combat_alert_level,
                            ws(htn::WS_ENEMY_VISIBLE),
                            ws(htn::WS_ENEMY_RANGE),
                            ws(htn::WS_WEAPON_HAS_CLEAR_LINE_OF_FIRE),
                            ws(htn::WS_WEAPON_EQUIPPED),
                            ws(htn::WS_HAS_AMMO),
                            ws(htn::WS_ENEMY_LOCATION_SUSPICIOUS),
                            ws(htn::WS_ALERT_LEVEL),
                            ws(htn::WS_COVER_USAGE),
                            ws(htn::WS_LOCATION),
                            e_m.x,
                            e_m.y,
                            e_m.z,
                            eyes_in,
                            p.current_stance(),
                            p.physics,
                        );
                    }
                }
                for (k, (_, ai, id)) in ais.iter().enumerate() {
                    let t = &track[k];
                    let walked: f32 = t.windows(2).map(|w| (w[1].1 - w[0].1).length()).sum();
                    let seen = t.iter().filter(|s| s.2).count();
                    eprintln!(
                        "  seed {seed} {id} #{k}: walked {walked:.0} m, {} rounds and blows, in his sight {seen} of {} half seconds, now {:.1} m from him{}",
                        fired[k],
                        t.len(),
                        t.last().map_or(0.0, |s| s.3),
                        if ai.pawn.dying { " (dying)" } else { "" }
                    );
                }
            }
        }
        eprintln!("{idle} times idle");
    }

    /// A soldier swinging at its enemy beside it, its attack given up mid-swing (its
    /// plan dropped, as for a better one when he comes nearer), swings at him again
    /// and its blows land: the stopped attack's animation calls back (the next
    /// frame) for that attack alone (melee.rs's animation_complete). It had ended
    /// the next attack the frame after that began, and so each attack the one after
    /// it: the soldier stood by him restarting its swing every other frame, striking
    /// nothing and never firing (Ignition's soldier by the start's health station,
    /// its enemy walking round it).
    #[test]
    fn an_attack_given_up_mid_swing_leaves_the_next_alone() {
        let dir = crate::assets_dir();
        let Some(map) = crate::MAPS.iter().find(|m| m.stem == "ignition") else {
            return;
        };
        let (Some(level), Some(nav)) =
            (crate::load_level_data(map), NavMesh::load(&dir, "ignition"))
        else {
            eprintln!("ignition's level or navigation not built: skipped");
            return;
        };
        let data = AiData::load(&dir);
        let kinds = crate::enemies::Kinds::load(&dir);
        let id = "SoldierDec_Kaon";
        let (Some(b), Some(k)) = (data.behaviour(id), kinds.kinds.iter().find(|k| k.id == id))
        else {
            eprintln!("ai.txt or enemies.txt not built: skipped");
            return;
        };
        let melee = b
            .domain
            .as_ref()
            .and_then(|d| d.symbol_named("STS_MeleeEnemy"))
            .expect("the soldiers' melee");
        let sounds = Arc::new(audio::AiSounds::default());
        let mut ecs = bevy::ecs::world::World::new();
        let mut world = AiWorld {
            data: Arc::new(data.clone()),
            sounds: sounds.clone(),
            loaded: true,
            ..Default::default()
        };
        world.shared.tactical.reset(Some(&nav));
        // Him standing by the start's health station, the soldier 2.8 m off.
        let player = ecs.spawn_empty().id();
        let him = Vec3::new(27.0, 0.0, -12.0);
        let at = him + Vec3::new(-2.0, 0.0, -2.0);
        let mut view = WorldView {
            players: vec![PlayerView {
                radius: 140.0,
                ..standing_player(player, ue::to_ue(him) + Vec3::Z * pawn::FLOOR_DIST)
            }],
            nav: Some(&nav),
            level: Some(&level),
            ..Default::default()
        };
        let body = Body {
            feet: ue::to_ue(at) + Vec3::Z * pawn::FLOOR_DIST,
            yaw: ue::yaw_of(ue::to_ue(him) - ue::to_ue(at)),
            radius: k.radius * ue::UU_PER_M,
            half_height: k.half_height * ue::UU_PER_M,
            health: k.health,
            mesh_scale: k.scale,
            forms: forms_of(&kinds, k),
        };
        let e = ecs.spawn_empty().id();
        let mut ai = new_ai(&mut world, &view, e, 0, b.clone(), body, 5).expect("an AI");
        let dt = 1.0 / 30.0;
        let mut alert_left = 0.0;
        // When its attack was given up, and its blows before and after.
        let mut given_up: Option<f32> = None;
        let (mut before, mut after) = (0, 0);
        for _ in 0..(30.0 * 8.0) as usize {
            world.shared.now += dt;
            view.now = world.shared.now;
            alert_left -= dt;
            let alerting = (alert_left <= 0.0).then(|| (player, view.players[0].loc));
            if alerting.is_some() {
                alert_left = ALERT_SECS;
            }
            let mut slots = vec![Slot {
                entity: e,
                ai: &mut ai,
                dormant: false,
            }];
            view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
            run_frame(
                &mut world.shared,
                &sounds,
                &mut view,
                &mut slots,
                dt,
                alerting,
            );
            let blows = world
                .shared
                .player_damage
                .iter()
                .filter(|d| d.by == e && d.melee)
                .count();
            if given_up.is_some() {
                after += blows;
            } else {
                before += blows;
            }
            world.shared.shots.clear();
            world.shared.launches.clear();
            world.shared.loops.clear();
            world.shared.player_damage.clear();
            world.shared.effects.clear();
            world.shared.effects_stopped.clear();
            // Its second swing, a little way in: its plan given up.
            let swinging = ai
                .brain
                .runner
                .active
                .as_ref()
                .is_some_and(|o| o.task.symbol == melee);
            if given_up.is_none()
                && before > 0
                && swinging
                && (0.1..0.2).contains(&ai.pawn.melee.time)
            {
                given_up = Some(world.shared.now);
                let Ai { brain, pawn, .. } = &mut ai;
                let mut ctx = Ctx {
                    dt,
                    pawn,
                    view: &view,
                    shared: &mut world.shared,
                };
                // (Dropped and its next plan begun in the one tick, as a better plan
                // takes over a running one: the next attack begins before the
                // given-up one's animation has called back.)
                brain.abort_plan(&mut ctx, false);
                brain.tick(&mut ctx);
                assert!(
                    brain
                        .runner
                        .active
                        .as_ref()
                        .is_some_and(|o| o.task.symbol == melee)
                        && ctx.pawn.melee.attacking.is_some(),
                    "it didn't swing again at once"
                );
            }
        }
        eprintln!("{before} blows, its attack given up at {given_up:?}, then {after} blows");
        assert!(given_up.is_some(), "it never swung twice");
        // (A swing a little under a second long: some seven in the 7 s after.)
        assert!(after >= 3, "{after} blows after its attack was given up");
    }

    /// Scrappers down in Downfall's trench under the Photon Burst Rifle's walkway,
    /// their enemy up on the walkway over them: the straight way to him is cut
    /// off, so the game's side steps never clear their channel; after two the
    /// demo's way round (sensors.rs's WayRound) takes them up by the jump at the
    /// trench's end, and they fight him from the walkway.
    #[test]
    fn downfalls_scrappers_climb_out_of_the_trench() {
        let dir = crate::assets_dir();
        let Some(map) = crate::MAPS.iter().find(|m| m.stem == "downfall") else {
            return;
        };
        let (Some(level), Some(nav)) =
            (crate::load_level_data(map), NavMesh::load(&dir, "downfall"))
        else {
            eprintln!("downfall's level or navigation not built: skipped");
            return;
        };
        let data = AiData::load(&dir);
        let kinds = crate::enemies::Kinds::load(&dir);
        let (Some(b), Some(k)) = (
            data.behaviour("ScrapperAut"),
            kinds.kinds.iter().find(|k| k.id == "ScrapperAut"),
        ) else {
            eprintln!("ai.txt or enemies.txt not built: skipped");
            return;
        };
        let sounds = Arc::new(audio::AiSounds::default());
        let mut ecs = bevy::ecs::world::World::new();
        let mut world = AiWorld {
            data: Arc::new(data.clone()),
            sounds: sounds.clone(),
            loaded: true,
            ..Default::default()
        };
        world.shared.tactical.reset(Some(&nav));
        let player = ecs.spawn_empty().id();
        let walkway = Vec3::new(15.36, 2.01, 234.24);
        let feet = ue::to_ue(walkway);
        let mut view = WorldView {
            players: vec![standing_player(player, feet)],
            nav: Some(&nav),
            level: Some(&level),
            ..Default::default()
        };
        let trench = [
            Vec3::new(16.5, -2.9, 220.1),
            Vec3::new(20.1, -2.9, 220.7),
            Vec3::new(9.0, -2.9, 219.0),
        ];
        let mut ais: Vec<(Entity, Ai)> = Vec::new();
        for (n, at) in trench.iter().enumerate() {
            let at = level
                .ground_for(
                    *at + Vec3::Y,
                    k.radius,
                    at.y + 1.0,
                    at.y - 8.0,
                    crate::collision::Mover::Ai,
                )
                .map_or(*at, |y| Vec3::new(at.x, y, at.z));
            let body = Body {
                feet: ue::to_ue(at) + Vec3::Z * pawn::FLOOR_DIST,
                yaw: ue::yaw_of(feet - ue::to_ue(at)),
                radius: k.radius * ue::UU_PER_M,
                half_height: k.half_height * ue::UU_PER_M,
                health: k.health,
                mesh_scale: k.scale,
                forms: forms_of(&kinds, k),
            };
            let e = ecs.spawn_empty().id();
            let ai =
                new_ai(&mut world, &view, e, 0, b.clone(), body, 77 + n as u64).expect("an AI");
            ais.push((e, ai));
        }
        let d = b.domain.clone().unwrap();
        let dt = 1.0 / 30.0;
        let mut alert_left = 0.0;
        // When each first stood on the walkway, and whether each attacked from it.
        let mut up: Vec<Option<f32>> = vec![None; ais.len()];
        let mut attacked = vec![false; ais.len()];
        for _ in 0..(30.0 * 30.0) as usize {
            world.shared.now += dt;
            view.now = world.shared.now;
            alert_left -= dt;
            let alerting = (alert_left <= 0.0).then(|| (player, view.players[0].loc));
            if alerting.is_some() {
                alert_left = ALERT_SECS;
            }
            let mut slots: Vec<Slot> = ais
                .iter_mut()
                .map(|(e, ai)| Slot {
                    entity: *e,
                    ai,
                    dormant: false,
                })
                .collect();
            view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
            run_frame(
                &mut world.shared,
                &sounds,
                &mut view,
                &mut slots,
                dt,
                alerting,
            );
            world.shared.shots.clear();
            world.shared.launches.clear();
            world.shared.loops.clear();
            world.shared.player_damage.clear();
            world.shared.effects.clear();
            world.shared.effects_stopped.clear();
            for (k, (_, ai)) in ais.iter().enumerate() {
                let at = ue::from_ue(ai.pawn.loc);
                let on_walkway = ai.pawn.physics == pawn::Physics::Walking && at.y > 1.0;
                if on_walkway && up[k].is_none() {
                    up[k] = Some(world.shared.now);
                }
                if on_walkway
                    && ai
                        .brain
                        .runner
                        .active
                        .as_ref()
                        .is_some_and(|o| d.name(o.task.symbol) == "SCRAPPER_MeleeEnemy")
                {
                    attacked[k] = true;
                }
            }
        }
        eprintln!("up on the walkway at {up:?}, attacked {attacked:?}");
        assert!(
            up.iter().all(|t| t.is_some_and(|t| t < 20.0)),
            "still in the trench after 20 s: {up:?}"
        );
        assert!(
            attacked.iter().all(|a| *a),
            "not all attacked: {attacked:?}"
        );
    }

    /// The steel trusses off the end of Downfall's start hall (beam1_STAT, two
    /// along each of its roof's east and south edges): least and greatest corners
    /// of each one's collision hull, Bevy metres.
    const DOWNFALL_ROOF_TRUSSES: [(Vec3, Vec3); 4] = [
        (
            Vec3::new(120.18, 22.68, 4.91),
            Vec3::new(127.81, 30.31, 32.37),
        ),
        (
            Vec3::new(120.18, 22.68, 31.79),
            Vec3::new(127.81, 30.31, 59.25),
        ),
        (
            Vec3::new(66.35, 22.68, 51.39),
            Vec3::new(93.81, 30.31, 59.02),
        ),
        (
            Vec3::new(93.64, 22.76, 51.49),
            Vec3::new(120.55, 30.24, 58.97),
        ),
    ];

    /// Scrappers spawned on Downfall's start hall roof (its four spawn points), their
    /// enemy down on the ground at `ground`, for 40 s: whether each walked a truss's
    /// top, when each came down to the ground, and where each was caught on or in a
    /// truss (round it, under its top: dropped into its lattice or landed on its
    /// side). None if the map isn't built.
    #[allow(clippy::type_complexity)]
    fn roof_scrappers(ground: Vec3) -> Option<(Vec<bool>, Vec<Option<f32>>, Vec<Option<Vec3>>)> {
        let dir = crate::assets_dir();
        let map = crate::MAPS.iter().find(|m| m.stem == "downfall")?;
        let (Some(level), Some(nav)) =
            (crate::load_level_data(map), NavMesh::load(&dir, "downfall"))
        else {
            eprintln!("downfall's level or navigation not built: skipped");
            return None;
        };
        let data = AiData::load(&dir);
        let kinds = crate::enemies::Kinds::load(&dir);
        let (Some(b), Some(k)) = (
            data.behaviour("ScrapperAut"),
            kinds.kinds.iter().find(|k| k.id == "ScrapperAut"),
        ) else {
            eprintln!("ai.txt or enemies.txt not built: skipped");
            return None;
        };
        let sounds = Arc::new(audio::AiSounds::default());
        let mut ecs = bevy::ecs::world::World::new();
        let mut world = AiWorld {
            data: Arc::new(data.clone()),
            sounds: sounds.clone(),
            loaded: true,
            ..Default::default()
        };
        world.shared.tactical.reset(Some(&nav));
        let settle = |at: Vec3, radius: f32| {
            level
                .ground_for(
                    at + Vec3::Y,
                    radius,
                    at.y + 1.0,
                    at.y - 8.0,
                    crate::collision::Mover::Ai,
                )
                .map_or(at, |y| Vec3::new(at.x, y, at.z))
        };
        let player = ecs.spawn_empty().id();
        let feet = ue::to_ue(settle(ground, 0.5));
        let mut view = WorldView {
            players: vec![standing_player(player, feet)],
            nav: Some(&nav),
            level: Some(&level),
            ..Default::default()
        };
        let roof = [
            Vec3::new(86.72, 32.48, 19.84),
            Vec3::new(96.96, 32.48, 19.84),
            Vec3::new(86.72, 32.48, 30.08),
            Vec3::new(96.96, 32.48, 30.08),
        ];
        let mut ais: Vec<(Entity, Ai)> = Vec::new();
        for (n, at) in roof.iter().enumerate() {
            let at = settle(*at, k.radius);
            let body = Body {
                feet: ue::to_ue(at) + Vec3::Z * pawn::FLOOR_DIST,
                yaw: ue::yaw_of(feet - ue::to_ue(at)),
                radius: k.radius * ue::UU_PER_M,
                half_height: k.half_height * ue::UU_PER_M,
                health: k.health,
                mesh_scale: k.scale,
                forms: forms_of(&kinds, k),
            };
            let e = ecs.spawn_empty().id();
            let ai =
                new_ai(&mut world, &view, e, 0, b.clone(), body, 91 + n as u64).expect("an AI");
            ais.push((e, ai));
        }
        let round = |at: Vec3, (lo, hi): &(Vec3, Vec3)| {
            at.x > lo.x - 1.5 && at.x < hi.x + 1.5 && at.z > lo.z - 1.5 && at.z < hi.z + 1.5
        };
        let dt = 1.0 / 30.0;
        let mut alert_left = 0.0;
        let mut on_top = vec![false; ais.len()];
        let mut down: Vec<Option<f32>> = vec![None; ais.len()];
        let mut caught: Vec<Option<Vec3>> = vec![None; ais.len()];
        for _ in 0..(30.0 * 40.0) as usize {
            world.shared.now += dt;
            view.now = world.shared.now;
            alert_left -= dt;
            let alerting = (alert_left <= 0.0).then(|| (player, view.players[0].loc));
            if alerting.is_some() {
                alert_left = ALERT_SECS;
            }
            let mut slots: Vec<Slot> = ais
                .iter_mut()
                .map(|(e, ai)| Slot {
                    entity: *e,
                    ai,
                    dormant: false,
                })
                .collect();
            view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
            run_frame(
                &mut world.shared,
                &sounds,
                &mut view,
                &mut slots,
                dt,
                alerting,
            );
            world.shared.shots.clear();
            world.shared.launches.clear();
            world.shared.loops.clear();
            world.shared.player_damage.clear();
            world.shared.effects.clear();
            world.shared.effects_stopped.clear();
            for (k, (_, ai)) in ais.iter().enumerate() {
                let at = ue::from_ue(ai.pawn.loc);
                let walking = ai.pawn.physics == pawn::Physics::Walking;
                for truss in &DOWNFALL_ROOF_TRUSSES {
                    let (lo, hi) = truss;
                    if round(at, truss) && at.y > lo.y - 1.0 && at.y < hi.y - 0.5 && walking {
                        caught[k].get_or_insert(at);
                    }
                    if round(at, truss) && at.y > hi.y && walking {
                        on_top[k] = true;
                    }
                }
                if at.y < 5.0 && down[k].is_none() {
                    down[k] = Some(world.shared.now);
                }
            }
        }
        Some((on_top, down, caught))
    }

    /// Scrappers spawned on Downfall's start hall roof, their enemy down on the
    /// ground: their ways down are along the steel trusses off the roof and jumps off
    /// the trusses' sides, as the game's navigation has them. They walk on the
    /// trusses' tops (the game's collision for them, the hull round each) and come
    /// down, wherever he is (east of the trusses, under the south one, south of
    /// it); none drops into a truss's lattice or lands on its side bracing and stays
    /// there.
    #[test]
    fn downfalls_scrappers_cross_the_roof_trusses() {
        for ground in [
            Vec3::new(147.2, 1.3, 30.0),
            Vec3::new(100.0, 0.5, 55.0),
            Vec3::new(110.0, 0.5, 62.0),
        ] {
            let Some((on_top, down, caught)) = roof_scrappers(ground) else {
                return;
            };
            eprintln!(
                "enemy at {ground}: walked a truss {on_top:?}, came down at {down:?}, caught at {caught:?}"
            );
            assert!(
                caught.iter().all(Option::is_none),
                "enemy at {ground}: caught on a truss at {caught:?}"
            );
            assert!(
                on_top.iter().any(|t| *t),
                "enemy at {ground}: none walked a truss"
            );
            assert!(
                down.iter().all(Option::is_some),
                "enemy at {ground}: still up after 40 s: {down:?}"
            );
        }
    }
}
