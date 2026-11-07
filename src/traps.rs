//! The maps' traps: what their consoles' purchases switch on, as the game plays it.
//! doors.rs runs the consoles (TnInteractCurrencyReceptacleMultiUse: their prices,
//! cooldowns and Kismet), their Matinees (the steam vents, the gunship circling) and
//! their sounds; this plays the traps' own actors (levels/<map>_doors.txt's emitter,
//! pain, light and sentry lines, tools/build_doors.py):
//!
//! * **Effects** (Emitter actors, their particle systems from fx/weapons_fx.txt):
//!   Ignition's Flame Jets (Grimlock's flamethrower, FX_CHR_Grimlock_p's
//!   Flamethrower_FX, half again as long), Oblivion's Combustion Chamber (Exodus's
//!   flamethrower at 3.5 times its size, and ESC_IncineratorFire_FX), each on while
//!   its Kismet (or its Matinee's toggle track) has it on, turned as the level has
//!   it; switched off, what is out of it lives out its life.
//! * **Lights** (PointLightToggleable): Oblivion's alarm light, on with its Kismet, its
//!   brightness as its Matinee's Brightness track pulses it (lit as the map's lights
//!   are: lighting.rs).
//! * **Pain volumes** (PhysicsVolume, DynamicPhysicsVolume): while one is on
//!   (bPainCausing), whatever touches it takes its DamagePerSec as it comes in
//!   (Touch: CausePainTo) and at each pop of its VolumeTimer, every second from the
//!   level's start (TimerPop: everything touching it); the players too, unless it
//!   leaves them out (bAffectPlayer: Ignition's 80 a second for the AI, beside the
//!   20 for everyone in each flame jet). The flame jets' (TnDamageTypeFlameZone) set
//!   what they hurt on fire for a second (TnBuffInFlameZone: Grimlock's fire on it,
//!   TnBuffOnFire's Flamethrower_Impact_FX, and on an enemy the explosion it carries,
//!   TnBuffExplodeOnDeathFire, which hurts the players near as it dies: specials.rs);
//!   Oblivion's combustion chamber's blast (TnDamageTypeEnvironmentExplosion,
//!   100000 a second) kills whatever is in it. The player at this PC is hurt by his
//!   PC (every PC its own), with the HUD's damage arrow pointing at the volume.
//! * **Sentries** (TnSentry, each from its factory as gameplay starts: Downfall's
//!   rocket turrets, RocketInteract_Sentry_TURRETDEF, hanging from the hangars'
//!   ceilings where their spawn points are, drawn at 1.5 times their size; the
//!   gunship's three guns, MGInteract_TURRETDEF, attached to its R_Robo01_XT bone,
//!   SeqAct_AttachToActor), on the players' side (painted in the sentries' own
//!   Cust_Color_A and Cust_COLOR_B, their energon glowing red as in the game's
//!   videos; their texture set at its full size), as their TnAiSentryController runs
//!   them. A rocket turret starts folded up (SentryRocket_DeActivate's end). Dormant
//!   (SeqAct_TnAiSentrySetDormant: gameplay start puts them to sleep, their console
//!   wakes them for 20 s, the gunship's for its flight), one hunts nothing: it drops
//!   its target, stops firing and folds up if it is out (a rocket turret's
//!   DeactivateSequenceName, SentryRocket_DeActivate). Awake, it takes the nearest
//!   live enemy within its AttackRange (320 m; the gunship's guns 160 m) that it can
//!   see from its muzzle (TnSentryPawn.GetViewLocation), turns its head
//!   to it (the anim tree's YawPitchControl on C_Robo01_XT, added to its clip's pose:
//!   LagDegreesPerSecond 50, the guns 60, round its own up and side whichever way it
//!   hangs), and fires once aimed within 3 degrees (AimedAtTargetThreshold) and done
//!   unfolding. A rocket turret unfolds as it takes a target and folds as it loses
//!   one (PlayAnimOnEnemyAcquisition: SentryRocket_Activate, SentryRocket_DeActivate);
//!   the gunship's guns unfold as they are spawned (PlayActivateOnSpawn) and stay so.
//!   Unfolding plays AI_TURRET_ENGAGE and lights its sights (the clip's notifies:
//!   Sentry_Ring_FX at 1.084 s, Sentry_Laser_FX at 1.122 s; their sockets aren't on
//!   the meshes, so they ride the head, facing the way it aims), folding
//!   AI_TURRET_DISENGAGE and puts them out.
//! * **The rocket turrets' weapon** (TnWeaponSentryInteractRocket,
//!   RocketInteract_Sentry_WEPDATA): bursts of 3 (ShotsPerBurst), a rocket every
//!   0.5 s (FireInterval) and 2.5 s between bursts (BurstDelay), all from
//!   MuzzleFlash_T (spread 0.02), with MuzzleFlash_HomRocket_FX and the rocket
//!   launcher's SHOOT; its rockets (TnProjectileSentryInteractRocket,
//!   RocketInteract_Sentry_PROJDATA: SENTRY_ROCKET) home on the target (riot.rs).
//! * **The gunship's guns** (MGInteract_Sentry_WEPDATA): instant rounds of 7
//!   (TnDamageTypeSentryGun: gunfire), firing faster as they keep at it (a round every
//!   0.25 s down to every 0.08 s over half a second), spread from 0.05 growing 0.0025
//!   a round to 0.15 and settling back 0.1 a second, from MuzzleFlash_L and
//!   MuzzleFlash_R in turn, with AttackDrone_Sentry_MuzzleFlash_FX,
//!   AssaultRifle_VEH_Tracer_FX, AssaultRifle_VEH_Squib_FX where they strike and
//!   IMPT_BALLISTIC_WORLD_ESC_GUNSHIP; ESCALATION_GUNSHIP_SHOOT_START loops while a
//!   gun fires, AI_TURRET_MG_SHOOT_END as it stops.
//!
//! Nothing here attacks the sentries (the rocket turrets' destructibles have 9999
//! health, the guns' 300). A trap's kills are nobody's: their energon is shared
//! (enemies.rs). Online every PC plays the traps itself, its sentries and volumes
//! hurting its own copies of the enemies, as the consoles' purchases come to all of
//! them (doors.rs).

use std::collections::HashMap;

use bevy::gltf::{Gltf, GltfMaterialExtras};
use bevy::prelude::*;

use crate::character::{AUTOBOT_ENERGON, PaintCache, PaintMaterial, is_paint};
use crate::doors::{DoorsState, PainKind, PainVolume, SentryKind, TrapThing};
use crate::enemies::{Death, Hit};
use crate::levelfx::{FxId, FxSystems};
use crate::riot::{Firer, RoundKind, RoundMods};
use crate::sound::Sfx;
use crate::{Colliders, Enemy, Level, LocalPlayer, Player};

/// The sentries' models and their data (tools/build_traps.py).
const TRAPS_FILE: &str = "models/traps/traps.txt";
const MODEL_FILES: [&str; 2] = [
    "models/traps/sentry_rocket.glb",
    "models/traps/sentry_mg.glb",
];
const MODEL_NAMES: [&str; 2] = ["sentry_rocket", "sentry_mg"];
/// The clips, by their roles in traps.txt.
const ACTIVATE: usize = 0;
const DEACTIVATE: usize = 1;

/// A volume's VolumeTimer: its pain every this many seconds (PainInterval).
const PAIN_INTERVAL: f32 = 1.0;
/// The fire on a player in a flame jet (TnBuffOnFire's ApplyFx).
const FX_BURN: &str = "Flamethrower_Impact_FX";

/// A kind of sentry as its TnSentry archetype has it: its AttackRange.Max and
/// SightRadius (metres), LagDegreesPerSecond, whether it unfolds as it takes a
/// target and folds as it loses one (PlayAnimOnEnemyAcquisition), unfolds as it is
/// spawned (PlayActivateOnSpawn) and folds as it is put to sleep (its
/// DeactivateSequenceName).
struct SentryDef {
    range: f32,
    sight: f32,
    lag: f32,
    on_acquire: bool,
    on_spawn: bool,
    folds_asleep: bool,
}

const ROCKET_DEF: SentryDef = SentryDef {
    range: 320.0,
    sight: 400.0,
    lag: 50.0,
    on_acquire: true,
    on_spawn: false,
    folds_asleep: true,
};
const MG_DEF: SentryDef = SentryDef {
    range: 160.0,
    sight: 300.0,
    lag: 60.0,
    on_acquire: false,
    on_spawn: true,
    folds_asleep: false,
};

fn def_of(kind: SentryKind) -> &'static SentryDef {
    match kind {
        SentryKind::Rocket => &ROCKET_DEF,
        SentryKind::Mg => &MG_DEF,
    }
}

/// AimedAtTargetThreshold (degrees).
const AIMED: f32 = 3.0;
/// How often a sentry looks again for the nearest enemy it can see (seconds).
const RETARGET_SECS: f32 = 0.2;
/// The unfolding clip's notifies: AI_TURRET_ENGAGE as it starts, the sights' effects
/// (seconds in); the folding clip's the same, AI_TURRET_DISENGAGE and the sights out.
const NOTIFY_RING: f32 = 1.084;
const NOTIFY_LASER: f32 = 1.122;
const FX_RING: &str = "Sentry_Ring_FX";
const FX_LASER: &str = "Sentry_Laser_FX";
const SND_ENGAGE: &str = "BL_WPN_GUN_AI_SENTRY.AI_TURRET_ENGAGE";
const SND_DISENGAGE: &str = "BL_WPN_GUN_AI_SENTRY.AI_TURRET_DISENGAGE";
/// The bone the head turns on (traps.txt's aim lines say it too), and the sockets
/// the sentries fire from.
const AIM_BONE: &str = "C_Robo01_XT";
const ROCKET_MUZZLE: &str = "MuzzleFlash_T";
const MG_MUZZLES: [&str; 2] = ["MuzzleFlash_L", "MuzzleFlash_R"];

/// The rocket turrets' weapon (RocketInteract_Sentry_WEPDATA): ShotsPerBurst,
/// FireInterval, BurstDelay and Spread, with its muzzle flash and shot.
const ROCKET_BURST: u32 = 3;
const ROCKET_INTERVAL: f32 = 0.5;
const ROCKET_BURST_DELAY: f32 = 2.5;
const ROCKET_SPREAD: f32 = 0.02;
const ROCKET_SND_SHOOT: &str = "BL_WPN_GUN_ROCKET.SHOOT";

/// The rocket turrets' rockets (TnProjectileSentryInteractRocket:
/// RocketInteract_Sentry_PROJDATA, a TnProjectileDataHoming, and
/// RocketInteract_Sentry_PROJMESH): 100 m/s, homing on the target at 60 m/s^2 and,
/// within 20 m of it, closing at 60 m/s^2, going off where they are half a second
/// into that (HomingForce, ClosingDistance, ClosingForce, ClosingTime; no
/// HomingForceScale: UseHomingForceScale is off); 75 to everything within 12.5 m at
/// full strength (its RadiusDamageScale has no Scales), explosive
/// (TnDamageTypeSentryRocket); flying as FX_Swortex_p's Rocket_Trail_FX (the rocket
/// and its smoke: SentryRocket_Trail_FX here) with the rocket launcher's TRAIL_LP,
/// going off as the metro turret's EXP_Metroturret_FX at a quarter of its size
/// (ExplosionEffectScale) with EXPL_IMPT_WORLD.
pub(crate) const SENTRY_ROCKET: RoundKind = RoundKind {
    speed: 100.0,
    life_secs: 4.0,
    damage: 75.0,
    radius: 12.5,
    self_damage: crate::riot::SELF_DAMAGE,
    muzzle_fx: "MuzzleFlash_HomRocket_FX",
    flight_fx: "SentryRocket_Trail_FX",
    explosion_fx: "EXP_Metroturret_FX",
    explosion_scale: 0.25,
    flight_sound: "BL_WPN_GUN_ROCKET.TRAIL_LP",
    explosion_sound: "BL_WPN_GUN_ROCKET.EXPL_IMPT_WORLD",
    light: None,
    falloff: None,
    homing: Some(crate::riot::Homing {
        force: 60.0,
        closing_distance: 20.0,
        closing_force: 60.0,
        closing_secs: 0.5,
    }),
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

/// The gunship's guns' weapon (MGInteract_Sentry_WEPDATA): InstantHitDamage, its
/// fire interval from the first round to the fastest and how long it takes to get
/// there, its spread (least, most, a round's growth) and how fast it settles back
/// (a second), and its reach (metres).
const MG_DAMAGE: f32 = 7.0;
const MG_INTERVALS: (f32, f32, f32) = (0.25, 0.08, 0.5);
const MG_SPREAD: (f32, f32, f32) = (0.05, 0.15, 0.0025);
const MG_SPREAD_SETTLE: f32 = 0.1;
const MG_RANGE: f32 = 500.0;
const MG_FX_MUZZLE: &str = "AttackDrone_Sentry_MuzzleFlash_FX";
const MG_FX_TRACER: &str = "AssaultRifle_VEH_Tracer_FX";
const MG_FX_IMPACT: &str = "AssaultRifle_VEH_Squib_FX";
const MG_SND_IMPACT: &str = "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_WORLD_ESC_GUNSHIP";
const MG_SND_LOOP: &str = "BL_WPN_GUN_AI_SENTRY.ESCALATION_GUNSHIP_SHOOT_START";
const MG_SND_END: &str = "BL_WPN_GUN_AI_SENTRY.AI_TURRET_MG_SHOOT_END";

/// The sentries' held sounds' owners (Sfx::hold): their entities' bits with these.
const LOOP_OWNER: u64 = 0x7E_5E << 40;

// ---------------------------------------------------------------------------
// The sentries' models
// ---------------------------------------------------------------------------

/// A sentry model (traps.txt): its file (and, once it is in, its scene and its clips
/// in their graph), its clips (activate, deactivate: their animations in the file
/// and their lengths), its sockets (their bones and frames there), its customization
/// colours (linear) and its aim bone.
struct Model {
    file: Handle<Gltf>,
    clips: [usize; 2],
    parts: Option<ModelParts>,
    /// Its file came in without what traps.txt says it has (warned of once).
    lacking: bool,
    secs: [f32; 2],
    sockets: HashMap<String, (String, Transform)>,
    paint: (Vec3, Vec3),
    aim: String,
}

/// A sentry model's scene and its clips' graph (their nodes by role), from its file.
struct ModelParts {
    scene: Handle<Scene>,
    graph: Handle<AnimationGraph>,
    nodes: Vec<AnimationNodeIndex>,
}

/// The two models: the rocket turret's and the gunship's gun's.
struct Models([Option<Model>; 2]);

impl Models {
    /// traps.txt read, and the models' files asked for (each file as a whole, its
    /// scene and clips taken from it as it comes in: asking for its parts one by one
    /// would read it again for each, every part already in being made again).
    fn load(asset_server: &AssetServer) -> Models {
        let path = crate::assets_dir().join(TRAPS_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                warn!("traps: {} can't be read ({e}): no sentries", path.display());
                return Models([None, None]);
            }
        };
        let mut clips: [Vec<(usize, usize, f32)>; 2] = [Vec::new(), Vec::new()];
        let mut sockets: [HashMap<String, (String, Transform)>; 2] = Default::default();
        let mut paints = [(Vec3::ONE, Vec3::ONE); 2];
        let mut aims = [AIM_BONE.to_string(), AIM_BONE.to_string()];
        let floats = |s: &str| -> Vec<f32> {
            s.split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect()
        };
        for line in text.lines().filter(|l| !l.starts_with('#')) {
            let r: Vec<&str> = line.split('\t').collect();
            let Some(m) = r
                .get(1)
                .and_then(|n| MODEL_NAMES.iter().position(|x| x == n))
            else {
                continue;
            };
            match (r[0], r.len()) {
                ("clip", 6..) => {
                    let role = match r[2] {
                        "activate" => ACTIVATE,
                        "deactivate" => DEACTIVATE,
                        _ => continue,
                    };
                    clips[m].push((
                        role,
                        r[3].trim().parse().unwrap_or(0),
                        r[4].trim().parse().unwrap_or(0.0),
                    ));
                }
                ("socket", 7..) => {
                    let (t, q, s) = (floats(r[4]), floats(r[5]), floats(r[6]));
                    if t.len() < 3 || q.len() < 4 || s.len() < 3 {
                        continue;
                    }
                    sockets[m].insert(
                        r[2].to_string(),
                        (
                            r[3].to_string(),
                            Transform {
                                translation: Vec3::new(t[0], t[1], t[2]),
                                rotation: Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize(),
                                scale: Vec3::new(s[0], s[1], s[2]),
                            },
                        ),
                    );
                }
                ("paint", 4..) => {
                    let (a, b) = (floats(r[2]), floats(r[3]));
                    if a.len() >= 3 && b.len() >= 3 {
                        paints[m] = (Vec3::new(a[0], a[1], a[2]), Vec3::new(b[0], b[1], b[2]));
                    }
                }
                ("aim", 3..) => aims[m] = r[2].trim().to_string(),
                _ => {}
            }
        }
        let mut out = [None, None];
        for m in 0..2 {
            let mut secs = [0.0; 2];
            let mut indices = [0; 2];
            let mut found = 0;
            for role in [ACTIVATE, DEACTIVATE] {
                let Some(&(_, index, length)) = clips[m].iter().find(|c| c.0 == role) else {
                    continue;
                };
                secs[role] = length;
                indices[role] = index;
                found += 1;
            }
            if found != 2 {
                warn!("traps: {} has no clips in {TRAPS_FILE}", MODEL_NAMES[m]);
                continue;
            }
            out[m] = Some(Model {
                file: asset_server.load(MODEL_FILES[m]),
                clips: indices,
                parts: None,
                lacking: false,
                secs,
                sockets: std::mem::take(&mut sockets[m]),
                paint: paints[m],
                aim: aims[m].clone(),
            });
        }
        Models(out)
    }

    /// The models whose files are in given their scenes and clips.
    fn update(&mut self, gltfs: &Assets<Gltf>, graphs: &mut Assets<AnimationGraph>) {
        for (m, model) in self.0.iter_mut().enumerate() {
            let Some(model) = model else {
                continue;
            };
            if model.parts.is_some() || model.lacking {
                continue;
            }
            let Some(gltf) = gltfs.get(&model.file) else {
                continue;
            };
            let clips: Option<Vec<Handle<AnimationClip>>> = model
                .clips
                .iter()
                .map(|&i| gltf.animations.get(i).cloned())
                .collect();
            let (Some(scene), Some(clips)) = (gltf.scenes.first(), clips) else {
                warn!(
                    "traps: {} lacks its scene or clips ({TRAPS_FILE}'s): no sentries of it",
                    MODEL_FILES[m]
                );
                model.lacking = true;
                continue;
            };
            let (graph, nodes) = AnimationGraph::from_clips(clips);
            model.parts = Some(ModelParts {
                scene: scene.clone(),
                graph: graphs.add(graph),
                nodes,
            });
        }
    }

    fn get(&self, kind: SentryKind) -> Option<&Model> {
        self.0[match kind {
            SentryKind::Rocket => 0,
            SentryKind::Mg => 1,
        }]
        .as_ref()
        .filter(|m| !m.lacking)
    }
}

// ---------------------------------------------------------------------------
// The traps in play
// ---------------------------------------------------------------------------

/// The traps as this match plays them: the map and match they are for (the doors'
/// clock as last seen: one gone back is a new match), the sentries' models, each
/// trap actor's effect, light, middle (a pain volume's) and what touches it, the
/// volumes' pops so far, the sentries spawned (by their sentry lines), and the Jazzes
/// the flame jets have set alight (the time left, and the fire on them).
#[derive(Resource, Default)]
pub struct Traps {
    stem: String,
    clock: Option<f32>,
    models: Option<Models>,
    effects: Vec<Option<FxId>>,
    lights: Vec<Option<Entity>>,
    middles: Vec<Vec3>,
    touching: Vec<Vec<Entity>>,
    pops: i64,
    sentries: Vec<Option<Entity>>,
    burning: Vec<(Entity, f32, Option<FxId>)>,
    /// The effects the demo hasn't got (warned of once).
    missing: Vec<usize>,
}

impl Traps {
    /// Everything put away (a new match, another map): the effects stopped, the
    /// lights and sentries gone.
    fn clear(&mut self, commands: &mut Commands, fx: &mut FxSystems) {
        for id in self.effects.drain(..).flatten() {
            fx.stop(id);
        }
        for e in self.lights.drain(..).flatten() {
            commands.entity(e).try_despawn();
        }
        for e in self.sentries.drain(..).flatten() {
            commands.entity(e).try_despawn();
        }
        for (_, _, id) in self.burning.drain(..) {
            if let Some(id) = id {
                fx.stop(id);
            }
        }
        self.middles.clear();
        self.touching.clear();
        self.pops = 0;
        self.clock = None;
    }
}

/// A trap's sentry in play (its sentry line's number): awake or not, its head's aim
/// (yaw and pitch in its own frame, radians: Unreal's, round its own up and side),
/// its target and when it looks again, the clip it plays (and how far in), whether
/// it is unfolded, its weapon's state (the next round's wait, rounds left in the
/// burst, how long it has kept firing, its spread, which muzzle is next, its rounds
/// so far, whether it fired last frame), its model's parts (its animation player and
/// bones by name, painted yet) and its sights' effects; and the aim bone's pose
/// before its head's turn (aim_sentries).
#[derive(Component)]
pub(crate) struct TrapSentry {
    spot: usize,
    kind: SentryKind,
    scale: f32,
    awake: bool,
    yaw: f32,
    pitch: f32,
    target: Option<Entity>,
    retarget: f32,
    clip: Option<(usize, f32)>,
    unfolded: bool,
    cooldown: f32,
    burst_left: u32,
    firing_secs: f32,
    spread: f32,
    right: bool,
    shots: u32,
    firing: bool,
    animator: Option<Entity>,
    bones: HashMap<String, Entity>,
    painted: bool,
    ring: Option<FxId>,
    laser: Option<FxId>,
    /// The aim bone's rotation as its clip leaves it, and as it was last turned.
    base: Option<Quat>,
    turned: Option<Quat>,
}

impl TrapSentry {
    fn new(spot: usize, kind: SentryKind, scale: f32, awake: bool) -> Self {
        TrapSentry {
            spot,
            kind,
            scale,
            awake,
            yaw: 0.0,
            pitch: 0.0,
            target: None,
            retarget: 0.0,
            clip: None,
            unfolded: false,
            cooldown: 0.0,
            burst_left: ROCKET_BURST,
            firing_secs: 0.0,
            spread: MG_SPREAD.0,
            right: false,
            shots: 0,
            firing: false,
            animator: None,
            bones: HashMap::new(),
            painted: false,
            ring: None,
            laser: None,
            base: None,
            turned: None,
        }
    }

    /// Starts clip `role` from its start (its notifies' sound now).
    fn play(&mut self, role: usize, at: Vec3, sfx: &mut Sfx) {
        self.clip = Some((role, 0.0));
        self.unfolded = role == ACTIVATE;
        sfx.play_at(
            if role == ACTIVATE {
                SND_ENGAGE
            } else {
                SND_DISENGAGE
            },
            at,
        );
    }
}

/// A sentry riding on a door's bone (the gunship's guns, SeqAct_AttachToActor): the
/// door, the bone (and its entity as last found: the door's scene spawned again has
/// new ones), and the sentry's place and turn on it (its size too). Not the bone's
/// child, so its door's scene coming in again doesn't take it (ride_sentries).
#[derive(Component)]
struct Ride {
    door: usize,
    bone: String,
    found: Option<Entity>,
    on: Transform,
}

pub struct TrapsPlugin;

impl Plugin for TrapsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Traps>()
            .add_systems(OnEnter(crate::menu::AppState::Game), reset_traps)
            .add_systems(
                Update,
                (sentry_parts, run_traps)
                    .chain()
                    .after(crate::doors::DoorsSystems)
                    .before(crate::hud::HudSystems)
                    .before(crate::sound::PlaySounds)
                    .run_if(in_state(crate::menu::AppState::Game)),
            )
            .add_systems(
                PostUpdate,
                (ride_sentries, aim_sentries)
                    .chain()
                    .after(bevy::app::AnimationSystems)
                    .before(bevy::transform::TransformSystems::Propagate)
                    .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

/// A new match: the traps start afresh (each match is a new level in the game).
fn reset_traps(mut commands: Commands, mut traps: ResMut<Traps>, mut fx: ResMut<FxSystems>) {
    traps.clear(&mut commands, &mut fx);
    traps.stem.clear();
}

/// A UE3 head turn (yaw and pitch in radians, Unreal's: round its up and its side)
/// in the model's frame (Bevy's axes: Unreal's y and z swapped).
fn head_turn(yaw: f32, pitch: f32) -> Quat {
    crate::doors::ue_turn(Vec3::new(0.0, pitch.to_degrees(), yaw.to_degrees()))
}

/// Unreal's yaw and pitch (radians) of a direction in the model's frame.
fn yaw_pitch(local: Vec3) -> (f32, f32) {
    // (Unreal's x, y, z: Bevy's x, z, y.)
    let (x, y, z) = (local.x, local.z, local.y);
    (y.atan2(x), z.atan2((x * x + y * y).sqrt()))
}

/// Turns `from` towards `to` (radians) by at most `step`, the short way round.
fn turn_towards(from: f32, to: f32, step: f32) -> f32 {
    let d =
        (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    from + d.clamp(-step, step)
}

/// An angle (radians) brought into -pi..pi.
fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// The middle of a convex volume given by its faces (the corners where three of
/// them meet, inside all of them, averaged).
fn volume_middle(v: &PainVolume) -> Vec3 {
    let p = &v.planes;
    let mut sum = Vec3::ZERO;
    let mut n = 0;
    for i in 0..p.len() {
        for j in i + 1..p.len() {
            for k in j + 1..p.len() {
                let (a, b, c) = (p[i].0, p[j].0, p[k].0);
                let det = a.dot(b.cross(c));
                if det.abs() < 1e-6 {
                    continue;
                }
                let x = (b.cross(c) * p[i].1 + c.cross(a) * p[j].1 + a.cross(b) * p[k].1) / det;
                if v.contains(x) || p.iter().all(|(nn, d)| nn.dot(x) <= d + 1e-3) {
                    sum += x;
                    n += 1;
                }
            }
        }
    }
    if n == 0 { Vec3::ZERO } else { sum / n as f32 }
}

/// Whether an upright cylinder (its middle, radius and half its height) touches a
/// convex volume (each face's plane: none has it all outside).
fn touches(v: &PainVolume, middle: Vec3, radius: f32, half_height: f32) -> bool {
    !v.planes.is_empty()
        && v.planes.iter().all(|(n, d)| {
            let reach = radius * Vec2::new(n.x, n.z).length() + half_height * n.y.abs();
            n.dot(middle) - reach <= *d
        })
}

/// A trap light's strength: its Brightness as the map's lights are matched to the
/// game's (lighting.rs's spawn_light: a quarter of the way out).
fn light_intensity(brightness: f32, radius: f32, falloff: f32) -> f32 {
    let d = radius * 0.25;
    let fade = (1.0 - 0.25f32 * 0.25).powf(falloff);
    brightness.max(0.0)
        * fade
        * 4.0
        * std::f32::consts::PI
        * std::f32::consts::PI
        * d
        * d
        * crate::lighting::unit()
}

/// A pain volume's death for what it kills.
fn death_of(kind: PainKind) -> Death {
    match kind {
        PainKind::Flame => Death::Fire,
        PainKind::Explosion => Death::Explosive,
        PainKind::Other => Death::Plain,
    }
}

/// A pain volume's damage type (its DamageType: the kill feed's; by nobody's hand,
/// each a suicide in TnDamageType's words).
fn damage_type_of(kind: PainKind) -> &'static str {
    match kind {
        PainKind::Flame => "TnDamageTypeFlameZone",
        PainKind::Explosion => "TnDamageTypeEnvironmentExplosion",
        PainKind::Other => "TnDamageType",
    }
}

/// Each sentry's model: its scene once its file is in, and its parts as they come
/// in: its animation player (given the model's graph), its bones by name, and its
/// paint (the material's colours, the energon red). (Its scene spawned again, they
/// are found again.)
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sentry_parts(
    mut commands: Commands,
    traps: Res<Traps>,
    mut sentries: Query<(Entity, &mut TrapSentry, Has<SceneRoot>)>,
    children: Query<&Children>,
    animators: Query<(), With<AnimationPlayer>>,
    names: Query<&Name>,
    meshes: Query<(
        &MeshMaterial3d<StandardMaterial>,
        Option<&GltfMaterialExtras>,
    )>,
    (standard, mut paints, mut cache): (
        Res<Assets<StandardMaterial>>,
        ResMut<Assets<PaintMaterial>>,
        ResMut<PaintCache>,
    ),
) {
    let Some(models) = traps.models.as_ref() else {
        return;
    };
    for (entity, mut s, has_scene) in &mut sentries {
        let Some(model) = models.get(s.kind) else {
            continue;
        };
        let Some(parts) = model.parts.as_ref() else {
            continue;
        };
        if !has_scene {
            commands
                .entity(entity)
                .insert(SceneRoot(parts.scene.clone()));
            continue;
        }
        if s.animator.is_some_and(|a| !animators.contains(a)) {
            s.animator = None;
            s.bones.clear();
            s.painted = false;
            s.base = None;
            s.turned = None;
        }
        if s.animator.is_some() && s.painted {
            continue;
        }
        let mut found = false;
        for e in children.iter_descendants(entity) {
            found = true;
            if s.animator.is_none() && animators.contains(e) {
                s.animator = Some(e);
                commands
                    .entity(e)
                    .insert(AnimationGraphHandle(parts.graph.clone()));
            }
            if let Ok(n) = names.get(e) {
                s.bones.entry(n.as_str().to_string()).or_insert(e);
            }
            if !s.painted
                && let Ok((m, extras)) = meshes.get(e)
                && is_paint(extras.map(|x| x.value.as_str()))
                && let Some(p) = cache.colored(
                    &m.0,
                    (model.paint.0, model.paint.1, AUTOBOT_ENERGON),
                    true,
                    &standard,
                    &mut paints,
                )
            {
                commands
                    .entity(e)
                    .remove::<MeshMaterial3d<StandardMaterial>>()
                    .insert(MeshMaterial3d(p));
            }
        }
        // (Painted once its meshes are in: they come with its scene.)
        if found && meshes.iter_many(children.iter_descendants(entity)).count() > 0 {
            s.painted = true;
        }
    }
}

/// The traps this frame: the map's effects, lights and pain volumes as the doors'
/// Kismet has them, the flame jets' fire on the Jazzes, and the sentries.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn run_traps(
    mut commands: Commands,
    time: Res<Time>,
    (asset_server, gltfs, mut graphs): (
        Res<AssetServer>,
        Res<Assets<Gltf>>,
        ResMut<Assets<AnimationGraph>>,
    ),
    mut traps: ResMut<Traps>,
    doors: Res<DoorsState>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    (mut fx, mut sfx): (ResMut<FxSystems>, ResMut<Sfx>),
    mut players: Query<(Entity, &Transform, &mut Player, Has<LocalPlayer>)>,
    mut enemies: Query<(Entity, &mut Enemy)>,
    mut sentries: Query<(Entity, &mut TrapSentry)>,
    door_parts: Query<(Entity, &crate::doors::Door)>,
    (children, names, globals): (Query<&Children>, Query<&Name>, Query<&GlobalTransform>),
    mut animators: Query<&mut AnimationPlayer>,
    mut lights: Query<(&mut PointLight, &mut Visibility)>,
) {
    let traps = &mut *traps;
    let d = doors.data();
    // A new map or match: everything afresh.
    let clock = doors.clock();
    let new_match = match (traps.clock, clock) {
        (Some(was), Some(now)) => now + 0.5 < was,
        (Some(_), None) => true,
        _ => false,
    };
    if traps.stem != d.stem || new_match {
        traps.clear(&mut commands, &mut fx);
        traps.stem = d.stem.clone();
    }
    let Some(clock) = clock else {
        return;
    };
    traps.clock = Some(clock);
    if d.traps.is_empty() && d.sentries.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    if traps.effects.len() != d.traps.len() {
        traps.effects = vec![None; d.traps.len()];
        traps.lights = vec![None; d.traps.len()];
        traps.touching = vec![Vec::new(); d.traps.len()];
        traps.middles = d
            .traps
            .iter()
            .map(|t| match &t.thing {
                TrapThing::Pain(v) => volume_middle(v),
                TrapThing::Emitter { at, .. } | TrapThing::Light { at, .. } => *at,
            })
            .collect();
        // (The pops already past as the traps start being played.)
        traps.pops = (clock / PAIN_INTERVAL).floor() as i64;
    }
    if traps.sentries.len() != d.sentries.len() {
        traps.sentries = vec![None; d.sentries.len()];
    }
    if traps.models.is_none() && !d.sentries.is_empty() {
        traps.models = Some(Models::load(&asset_server));
    }
    if let Some(models) = traps.models.as_mut() {
        models.update(&gltfs, &mut graphs);
    }

    // ---- the effects and lights ----
    for (i, t) in d.traps.iter().enumerate() {
        let on = doors.trap_on(i);
        match &t.thing {
            TrapThing::Emitter {
                system,
                at,
                x,
                y,
                scale,
                lit,
            } => {
                let slot = &mut traps.effects[i];
                if on {
                    if slot.is_none_or(|id| !fx.alive(id)) {
                        *slot = fx.spawn_sized(system, *at, *x, *scale);
                        match *slot {
                            Some(id) => {
                                fx.place_turned(id, *at, *x, *y);
                                if *lit {
                                    fx.set_lit(id);
                                }
                            }
                            None if !traps.missing.contains(&i) => {
                                warn!("traps: {}'s effect {system} isn't in the effects", t.id);
                                traps.missing.push(i);
                            }
                            None => {}
                        }
                    }
                } else if let Some(id) = slot.take() {
                    fx.stop(id);
                }
            }
            TrapThing::Light {
                at,
                color,
                brightness,
                radius,
                falloff,
            } => {
                let e = *traps.lights[i].get_or_insert_with(|| {
                    commands
                        .spawn((
                            PointLight {
                                color: Color::linear_rgb(color.x, color.y, color.z),
                                intensity: 0.0,
                                range: *radius,
                                radius: 0.1,
                                shadows_enabled: false,
                                ..default()
                            },
                            Transform::from_translation(*at),
                            Visibility::Hidden,
                            crate::MapPart,
                        ))
                        .id()
                });
                if let Ok((mut light, mut vis)) = lights.get_mut(e) {
                    let b = doors.light_brightness(i).unwrap_or(*brightness);
                    let intensity = light_intensity(b, *radius, *falloff);
                    if (light.intensity - intensity).abs() > 1e-3 {
                        light.intensity = intensity;
                    }
                    let want = if on && b > 0.0 {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                    if *vis != want {
                        *vis = want;
                    }
                }
            }
            TrapThing::Pain(_) => {}
        }
    }

    // ---- the pain volumes ----
    let pops_now = (clock / PAIN_INTERVAL).floor() as i64;
    let popped = pops_now > traps.pops;
    traps.pops = pops_now;
    let mut flamed: Vec<Entity> = Vec::new();
    for (i, t) in d.traps.iter().enumerate() {
        let TrapThing::Pain(v) = &t.thing else {
            continue;
        };
        let on = doors.trap_on(i);
        let middle = traps.middles[i];
        let damage = v.dps * PAIN_INTERVAL;
        let death = death_of(v.kind);
        let mut now_touching = Vec::new();
        // The enemies in it.
        for (e, mut enemy) in &mut enemies {
            if !enemy.alive() {
                continue;
            }
            let (c, r) = enemy.body.bounds;
            if !touches(v, c, r * 0.7, r * 0.7) {
                continue;
            }
            now_touching.push(e);
            let came_in = !traps.touching[i].contains(&e);
            if on && (popped || came_in) {
                if v.kind == PainKind::Flame {
                    enemy.burn.in_flames();
                }
                let at = enemy.centre();
                enemy.take(
                    damage,
                    None,
                    if v.kind == PainKind::Flame {
                        crate::xp::Blow::Burn
                    } else {
                        crate::xp::Blow::Weapon
                    },
                    Hit::new(death, at, at - middle).of_type(damage_type_of(v.kind)),
                );
            }
        }
        // The Jazzes in it (the player at this PC hurt by his PC, every Jazz's fire
        // shown).
        if v.players {
            for (e, transform, mut player, local) in &mut players {
                if player.dead_left > 0.0 || player.dying || player.waiting {
                    continue;
                }
                let (radius, height) = crate::riot::body_of(&player);
                let feet = transform.translation;
                if !touches(v, feet + Vec3::Y * height * 0.5, radius, height * 0.5) {
                    continue;
                }
                now_touching.push(e);
                let came_in = !traps.touching[i].contains(&e);
                if !(on && (popped || came_in)) {
                    continue;
                }
                if v.kind == PainKind::Flame {
                    flamed.push(e);
                }
                if !local {
                    continue;
                }
                let centre = player.centre(feet);
                let hit = Hit::new(death, centre, centre - middle).of_type(damage_type_of(v.kind));
                if crate::damage_player(&mut player, damage, hit) {
                    sfx.play_from(crate::SND_PLAYER_HIT, None);
                    player.hud_events.push(crate::hud::HudEvent::Damaged {
                        yaw: crate::heading(middle - feet),
                        amount: damage / crate::HIT_INDICATOR_HEALTH,
                    });
                }
            }
        }
        traps.touching[i] = now_touching;
    }
    // The flame jets' fire on the Jazzes they hurt, for its second.
    for e in flamed {
        match traps.burning.iter_mut().find(|b| b.0 == e) {
            Some(b) => b.1 = crate::specials::FLAME_ZONE_SECS,
            None => traps
                .burning
                .push((e, crate::specials::FLAME_ZONE_SECS, None)),
        }
    }
    traps.burning.retain_mut(|(e, left, id)| {
        *left -= dt;
        // (The fire at his robot's size, as the game attaches it to his mesh.)
        let at = players
            .get(*e)
            .ok()
            .filter(|(_, _, p, _)| p.dead_left <= 0.0 && !p.dying)
            .map(|(_, t, p, _)| (p.centre(t.translation), p.kit.robot.effect_scale()));
        match (at, *left > 0.0) {
            (Some((at, size)), true) => {
                match *id {
                    Some(i) if fx.alive(i) => fx.place(i, at, Vec3::Y, Vec3::ZERO),
                    _ => *id = fx.spawn_scaled(FX_BURN, at, Vec3::Y, size, &[]),
                }
                true
            }
            _ => {
                if let Some(i) = id.take() {
                    fx.stop(i);
                }
                false
            }
        }
    });

    // ---- the sentries ----
    let Some(models) = traps.models.as_ref() else {
        return;
    };
    let decepticon = crate::MAPS
        .iter()
        .find(|m| m.stem == d.stem)
        .is_some_and(|m| m.decepticons);
    // Spawned as their time comes (their factories, as gameplay starts), their models
    // coming with their files (sentry_parts).
    for (i, spot) in d.sentries.iter().enumerate() {
        if traps.sentries[i].is_some_and(|e| sentries.contains(e)) || clock < spot.spawn {
            continue;
        }
        if models.get(spot.kind).is_none() {
            continue;
        }
        let mut sentry = TrapSentry::new(i, spot.kind, spot.scale, doors.sentry_awake(i));
        // A rocket turret starts folded up (its DeActivate clip's end), as it waits
        // for its console: it unfolds only as it takes a target.
        if def_of(spot.kind).on_acquire
            && let Some(model) = models.get(spot.kind)
        {
            sentry.clip = Some((DEACTIVATE, model.secs[DEACTIVATE]));
        }
        let entity = match &spot.rides {
            // On a door's bone (the gunship's guns): once its scene is in, put where
            // the bone is as the transforms are worked out (ride_sentries).
            Some((door, bone, offset, turn)) => {
                let Some(found) = door_parts.iter().find(|(_, p)| p.index == *door).and_then(
                    |(door_entity, _)| {
                        children
                            .iter_descendants(door_entity)
                            .find(|e| names.get(*e).is_ok_and(|n| n.as_str() == bone))
                    },
                ) else {
                    continue;
                };
                commands
                    .spawn((
                        sentry,
                        Ride {
                            door: *door,
                            bone: bone.clone(),
                            found: Some(found),
                            on: Transform {
                                translation: *offset,
                                rotation: *turn,
                                scale: Vec3::splat(spot.scale),
                            },
                        },
                        Transform::default(),
                        Visibility::Hidden,
                        crate::OwnAnimation,
                        crate::MapPart,
                    ))
                    .id()
            }
            None => commands
                .spawn((
                    sentry,
                    Transform {
                        translation: spot.at,
                        rotation: spot.turn,
                        scale: Vec3::splat(spot.scale),
                    },
                    Visibility::Inherited,
                    crate::OwnAnimation,
                    crate::MapPart,
                ))
                .id(),
        };
        traps.sentries[i] = Some(entity);
    }
    let world = (&*level, &*colliders);
    for (entity, mut s) in &mut sentries {
        let s = &mut *s;
        let def = def_of(s.kind);
        let Some(model) = models.get(s.kind) else {
            continue;
        };
        let Ok(root) = globals.get(entity) else {
            continue;
        };
        let (_, root_turn, root_at) = root.to_scale_rotation_translation();
        // Its head: where it turns (its aim bone's place: the root's, until the
        // model is in).
        let head = s
            .bones
            .get(&model.aim)
            .and_then(|b| globals.get(*b).ok())
            .map_or(root_at, |g| g.translation());
        // Unfolding as it is spawned (the gunship's guns). (Its clips run whether
        // its model is in yet or not: the model follows them.)
        if s.clip.is_none() && !s.unfolded && def.on_spawn {
            s.play(ACTIVATE, head, &mut sfx);
        }
        // Awake or asleep, as its Kismet has it.
        let awake = doors.sentry_awake(s.spot);
        if awake != s.awake {
            s.awake = awake;
            if !awake {
                // SetDormant: no target, no fire, and folding up (a rocket turret:
                // PlayReplicatedDeactivateAnim, from the clip's start: one already
                // folded stays so).
                s.target = None;
                if def.folds_asleep && s.unfolded {
                    s.play(DEACTIVATE, head, &mut sfx);
                }
            }
        }
        // Its eye: its weapon's muzzle (TnSentryPawn.GetViewLocation:
        // W.GetMuzzleLocation; its head until its model is in). (Not its head: folded
        // up, a hanging rocket turret's is up in the ceiling it hangs from.)
        let eye = socket_at(
            model,
            &s.bones,
            &globals,
            match s.kind {
                SentryKind::Rocket => ROCKET_MUZZLE,
                SentryKind::Mg => MG_MUZZLES[0],
            },
        )
        .map_or(head, |(at, _)| at);
        // Its target: the nearest live enemy within its AttackRange of it (its
        // Location: TnSentryTargetTracker.IsValidTarget) that it can see from its
        // eye (looked for again as its sight is checked, every RETARGET_SECS; one
        // that dies or leaves its range is dropped at once).
        let had = s.target;
        if !s.awake {
            s.target = None;
        } else {
            s.retarget -= dt;
            let valid = |e: &Enemy| e.alive() && e.centre().distance(root_at) <= def.range;
            let still = s
                .target
                .and_then(|t| enemies.get(t).ok())
                .is_some_and(|(_, e)| valid(e));
            if !still || s.retarget <= 0.0 {
                s.retarget = RETARGET_SECS;
                s.target = enemies
                    .iter()
                    .filter(|(_, e)| valid(e))
                    .map(|(e, en)| (e, en.centre()))
                    .filter(|(_, c)| c.distance(eye) <= def.sight && !level.blocked(eye, *c))
                    .min_by(|a, b| a.1.distance(root_at).total_cmp(&b.1.distance(root_at)))
                    .map(|(e, _)| e);
            }
        }
        // A rocket turret unfolds as it takes a target and folds as it loses one
        // (TnSentryPawn.EnemyUpdated: EnemyAcquired, EnemyLost).
        if def.on_acquire && s.target.is_some() != had.is_some() {
            s.play(
                if s.target.is_some() {
                    ACTIVATE
                } else {
                    DEACTIVATE
                },
                head,
                &mut sfx,
            );
        }
        // Its clip on (its notifies: the sights lit as it unfolds, out as it folds).
        if let Some((role, t)) = s.clip.as_mut() {
            let before = *t;
            *t += dt;
            let after = *t;
            let role = *role;
            let length = model.secs[role].max(1e-3);
            for (at, which) in [(NOTIFY_RING, 0), (NOTIFY_LASER, 1)] {
                if before <= at && at < after {
                    let slot = if which == 0 {
                        &mut s.ring
                    } else {
                        &mut s.laser
                    };
                    if role == ACTIVATE {
                        if slot.is_none_or(|id| !fx.alive(id)) {
                            *slot = fx.spawn_scaled(
                                if which == 0 { FX_RING } else { FX_LASER },
                                head,
                                root_turn * Vec3::X,
                                s.scale,
                                &[],
                            );
                        }
                    } else if let Some(id) = slot.take() {
                        fx.stop(id);
                    }
                }
            }
            if after >= length {
                // (Held at its end.)
                if let Some((_, t)) = s.clip.as_mut() {
                    *t = length;
                }
            }
        }
        if let (Some(a), Some((role, t))) = (s.animator, s.clip)
            && let Ok(mut player) = animators.get_mut(a)
            && let Some(&node) = model.parts.as_ref().and_then(|p| p.nodes.get(role))
        {
            crate::doors::hold(&mut player, node, t, model.secs[role]);
        }
        let unfolding = s
            .clip
            .is_some_and(|(role, t)| role == ACTIVATE && t < model.secs[ACTIVATE]);
        // Its head: to its target at its LagDegreesPerSecond, else back to rest.
        let step = def.lag.to_radians() * dt;
        let target_at = s
            .target
            .and_then(|t| enemies.get(t).ok())
            .map(|(_, e)| e.centre());
        let mut aimed = false;
        match target_at {
            Some(c) => {
                let local = root_turn.inverse() * (c - head);
                let (want_yaw, want_pitch) = yaw_pitch(local);
                s.yaw = wrap(turn_towards(s.yaw, want_yaw, step));
                s.pitch = wrap(turn_towards(s.pitch, want_pitch, step));
                let aim = root_turn * head_turn(s.yaw, s.pitch) * Vec3::X;
                aimed = aim.angle_between((c - head).normalize_or(aim)).to_degrees() <= AIMED;
            }
            None => {
                s.yaw = wrap(turn_towards(s.yaw, 0.0, step));
                s.pitch = wrap(turn_towards(s.pitch, 0.0, step));
            }
        }
        let head_world = root_turn * head_turn(s.yaw, s.pitch);
        // Its sights on its head, facing the way it aims.
        for id in [s.ring, s.laser].into_iter().flatten() {
            fx.place_turned(id, head, head_world * Vec3::X, head_world * Vec3::Z);
        }
        // Firing (TnSentryPawn.CanFire: a target in its AttackRange, done unfolding,
        // aimed).
        let fire = s.awake
            && aimed
            && !unfolding
            && target_at.is_some_and(|c| c.distance(root_at) <= def.range);
        let target = s.target;
        match s.kind {
            SentryKind::Rocket => {
                s.cooldown = (s.cooldown - dt).max(-ROCKET_INTERVAL);
                if !fire {
                    s.cooldown = s.cooldown.max(0.0);
                } else if s.cooldown <= 0.0
                    && let Some(c) = target_at
                {
                    let (muzzle, frame) = socket_at(model, &s.bones, &globals, ROCKET_MUZZLE)
                        .unwrap_or((head + head_world * Vec3::X * s.scale, head_world));
                    s.shots = s.shots.wrapping_add(1);
                    let dir = crate::spread_shot(
                        (c - muzzle).normalize_or(frame * Vec3::X),
                        ROCKET_SPREAD,
                        (entity.to_bits() as u32) ^ s.shots.wrapping_mul(2654435761),
                    );
                    fx.spawn(SENTRY_ROCKET.muzzle_fx, muzzle, frame * Vec3::X, &[]);
                    sfx.play_at(ROCKET_SND_SHOOT, muzzle);
                    crate::riot::fire_homing_round(
                        &mut commands,
                        &mut fx,
                        &SENTRY_ROCKET,
                        muzzle,
                        dir,
                        Firer {
                            owner: None,
                            shooter: None,
                            decepticon,
                            mods: RoundMods::NONE,
                        },
                        target,
                    );
                    // (A burst's rounds FireInterval apart, then BurstDelay.)
                    s.burst_left = s.burst_left.saturating_sub(1);
                    if s.burst_left == 0 {
                        s.burst_left = ROCKET_BURST;
                        s.cooldown += ROCKET_INTERVAL + ROCKET_BURST_DELAY;
                    } else {
                        s.cooldown += ROCKET_INTERVAL;
                    }
                }
            }
            SentryKind::Mg => {
                let owner = LOOP_OWNER | (entity.to_bits() & 0xFF_FFFF_FFFF);
                let (first, fastest, ramp) = MG_INTERVALS;
                s.cooldown = (s.cooldown - dt).max(-fastest);
                if fire {
                    s.firing_secs += dt;
                } else {
                    s.firing_secs = 0.0;
                    s.cooldown = s.cooldown.max(0.0);
                    s.spread = (s.spread - MG_SPREAD_SETTLE * dt).max(MG_SPREAD.0);
                }
                let muzzle_name = MG_MUZZLES[usize::from(s.right)];
                // (Before its model is in: out of its head, as its guns stand.)
                let side = if s.right { -1.0 } else { 1.0 };
                let muzzle = socket_at(model, &s.bones, &globals, muzzle_name).or(Some((
                    head + head_world * Vec3::new(1.4, 0.0, side * 1.05) * s.scale,
                    head_world,
                )));
                if fire && let Some((m, _)) = muzzle {
                    sfx.hold(owner, MG_SND_LOOP, Some(m));
                } else if s.firing {
                    sfx.play_at(MG_SND_END, head);
                }
                if fire
                    && s.cooldown <= 0.0
                    && let (Some(c), Some((from, frame))) = (target_at, muzzle)
                {
                    let k = (s.firing_secs / ramp).clamp(0.0, 1.0);
                    s.cooldown += first + (fastest - first) * k;
                    s.shots = s.shots.wrapping_add(1);
                    s.right = !s.right;
                    let barrel = frame * Vec3::X;
                    let dir = crate::spread_shot(
                        (c - from).normalize_or(barrel),
                        s.spread,
                        (entity.to_bits() as u32) ^ s.shots.wrapping_mul(2654435761),
                    );
                    s.spread = (s.spread + MG_SPREAD.2).min(MG_SPREAD.1);
                    let wall = world
                        .0
                        .shot_surface(world.1, from, dir, MG_RANGE)
                        .map_or((MG_RANGE, -dir), |(t, n)| (t, n));
                    let struck = enemies
                        .iter()
                        .filter_map(|(e, en)| en.ray(from, dir, wall.0).map(|t| (e, t)))
                        .min_by(|a, b| a.1.total_cmp(&b.1));
                    let distance = struck.map_or(wall.0, |(_, t)| t);
                    let end = from + dir * distance;
                    fx.spawn(MG_FX_MUZZLE, from, barrel, &[]);
                    fx.spawn_tracer(MG_FX_TRACER, from, dir, distance);
                    match struck {
                        Some((e, _)) => {
                            fx.spawn(MG_FX_IMPACT, end, -dir, &[]);
                            if let Ok((_, mut enemy)) = enemies.get_mut(e) {
                                enemy.take(
                                    MG_DAMAGE,
                                    None,
                                    crate::xp::Blow::Weapon,
                                    Hit::new(Death::Gunfire, end, dir)
                                        .of_type("TnDamageTypeSentryGun"),
                                );
                            }
                        }
                        None if distance < MG_RANGE => {
                            fx.spawn(MG_FX_IMPACT, end, wall.1, &[]);
                            sfx.play_at(MG_SND_IMPACT, end);
                        }
                        None => {}
                    }
                }
            }
        }
        s.firing = fire;
    }
}

/// Where a model's socket is in the world (its place, and its frame's turn), from
/// its bone's world frame: None until the bone is found.
fn socket_at(
    model: &Model,
    bones: &HashMap<String, Entity>,
    globals: &Query<&GlobalTransform>,
    name: &str,
) -> Option<(Vec3, Quat)> {
    let (bone, local) = model.sockets.get(name)?;
    let g = globals.get(*bones.get(bone)?).ok()?;
    let world = g.mul_transform(*local);
    let (_, turn, at) = world.to_scale_rotation_translation();
    Some((at, turn))
}

/// The sentries riding on doors' bones (the gunship's guns) where their bones are
/// this frame, once the doors' clips have posed them (before the transforms are
/// propagated): the bone's frame from the transforms up its ancestors, the sentry's
/// place on it after. Hidden while the bone isn't there (its door's scene coming in
/// again: found again by its name).
fn ride_sentries(
    mut riders: Query<(&mut Ride, &mut Transform, &mut Visibility), With<TrapSentry>>,
    doors: Query<(Entity, &crate::doors::Door)>,
    children: Query<&Children>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    locals: Query<&Transform, Without<TrapSentry>>,
) {
    for (mut ride, mut t, mut vis) in &mut riders {
        let ride = &mut *ride;
        let named = |e: Entity| names.get(e).is_ok_and(|n| n.as_str() == ride.bone);
        let bone = ride.found.filter(|b| named(*b)).or_else(|| {
            let (door, _) = doors.iter().find(|(_, d)| d.index == ride.door)?;
            children.iter_descendants(door).find(|e| named(*e))
        });
        ride.found = bone;
        let world = bone.and_then(|b| {
            let chain: Vec<Entity> = std::iter::once(b)
                .chain(parents.iter_ancestors(b))
                .collect();
            chain
                .iter()
                .rev()
                .try_fold(GlobalTransform::IDENTITY, |g, e| {
                    locals.get(*e).ok().map(|l| g.mul_transform(*l))
                })
        });
        match world {
            Some(g) => {
                let placed = g.mul_transform(ride.on).compute_transform();
                if *t != placed {
                    *t = placed;
                }
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
            None => {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
}

/// Each sentry's head turned to its aim, once its clip has posed it (before the
/// transforms are propagated): its YawPitchControl's turn added to the clip's pose,
/// in the model's own frame (bAddRotation, in the sentry's space).
fn aim_sentries(
    mut sentries: Query<(Entity, &mut TrapSentry)>,
    mut transforms: Query<&mut Transform, Without<TrapSentry>>,
    parents: Query<&ChildOf>,
    traps: Res<Traps>,
) {
    let Some(models) = traps.models.as_ref() else {
        return;
    };
    for (root, mut s) in &mut sentries {
        let Some(model) = models.get(s.kind) else {
            continue;
        };
        let Some(&bone) = s.bones.get(&model.aim) else {
            continue;
        };
        // The bone's parent's turn in the model's frame (its clip's pose this frame).
        let mut parent = Quat::IDENTITY;
        let mut e = bone;
        while let Ok(child_of) = parents.get(e) {
            e = child_of.parent();
            if e == root {
                break;
            }
            let Ok(t) = transforms.get(e) else {
                break;
            };
            parent = t.rotation * parent;
        }
        let Ok(mut t) = transforms.get_mut(bone) else {
            continue;
        };
        // (Its clip's pose: what the clip set this frame, or, where the clip leaves
        // the bone alone, the pose before last frame's turn.)
        let base = match (s.turned, s.base) {
            (Some(turned), Some(base)) if t.rotation == turned => base,
            _ => t.rotation,
        };
        let turned = parent.inverse() * head_turn(s.yaw, s.pitch) * parent * base;
        t.rotation = turned;
        s.base = Some(base);
        s.turned = Some(turned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A head's aim from a direction and back: yaw round its up, pitch up from level.
    #[test]
    fn heads_aim_as_unreals() {
        for (yaw, pitch) in [(0.0f32, 0.0f32), (0.7, 0.2), (-2.5, -0.9), (3.0, 1.2)] {
            let dir = head_turn(yaw, pitch) * Vec3::X;
            let (y, p) = yaw_pitch(dir);
            assert!(
                (y - yaw).abs() < 1e-4 && (p - pitch).abs() < 1e-4,
                "{yaw} {pitch}: {y} {p}"
            );
        }
        // Unreal's yaw turns its x towards its y: Bevy's x towards its z.
        let (y, p) = yaw_pitch(Vec3::Z);
        assert!((y - std::f32::consts::FRAC_PI_2).abs() < 1e-5 && p.abs() < 1e-5);
        // Up is Bevy's y.
        assert!((yaw_pitch(Vec3::Y).1 - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    }

    /// A box's middle, and what touches it.
    #[test]
    fn volumes() {
        let v = PainVolume {
            dps: 20.0,
            players: true,
            kind: PainKind::Flame,
            planes: vec![
                (Vec3::X, 2.0),
                (Vec3::NEG_X, 0.0),
                (Vec3::Y, 1.0),
                (Vec3::NEG_Y, 1.0),
                (Vec3::Z, 3.0),
                (Vec3::NEG_Z, -1.0),
            ],
        };
        assert!(volume_middle(&v).distance(Vec3::new(1.0, 0.0, 2.0)) < 1e-4);
        assert!(touches(&v, Vec3::new(1.0, 0.0, 2.0), 0.1, 0.1));
        // Outside by less than its radius: touching; by more: not.
        assert!(touches(&v, Vec3::new(2.4, 0.0, 2.0), 0.5, 0.5));
        assert!(!touches(&v, Vec3::new(2.6, 0.0, 2.0), 0.5, 0.5));
        // Above by less than half its height.
        assert!(touches(&v, Vec3::new(1.0, 1.9, 2.0), 0.1, 1.0));
        assert!(!touches(&v, Vec3::new(1.0, 2.1, 2.0), 0.1, 1.0));
    }

    #[test]
    fn turning() {
        assert!((turn_towards(0.0, 1.0, 0.25) - 0.25).abs() < 1e-6);
        // (The short way round.)
        assert!(turn_towards(3.0, -3.0, 1.0) > 3.0);
    }
}
