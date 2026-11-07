//! Escalation's locked doors and the unlock consoles that open them
//! (TnInteractCurrencyReceptacle and the maps' Kismet): from each map's
//! levels/<map>_doors.txt, levels/<map>_doors.bin and models/<map>_doors.glb, and the
//! console's models/esc_console.glb (tools/build_doors.py, which takes the doors and
//! consoles out of the map's own model and collision).
//!
//! * **The consoles** (TnInteractCurrencyReceptacle, in its own materials: its screen
//!   a flip book of 3 x 3 frames at 6 a second, its keyboard and panels glowing):
//!   standing in reach of the spot 5 m in front of one, facing it, in robot form and
//!   not down (its UseConditions, the Armory Recreators' too: recreator.rs), the HUD
//!   offers the console's own line from TransGame.int ("Press E to deposit *250 of
//!   *1500 to open door", the deposit in gold and the price in cyan) and shows the
//!   energon counter, red when there isn't enough (TnInteractActionHudCurrencyNotify).
//!   Each press puts the console's deposit (250; 500 for Ancients' bunker) into its
//!   total, with ESC_MONEY_DEPOSIT at the console (its BeginUseSequence); without
//!   enough, the error beep and the counter flashing red. Consoles that name the same
//!   TnDataStoreActor share one total (a door's consoles on both its sides), each
//!   with its own price. Its progress bar is its Activate_Console clip at total /
//!   price, moving to it at 0.7 of the bar a second
//!   (TnInteractActionSetAnimPositionByDataStore).
//! * **The glow**: the console's outline (ENV_OutlineHighlight_MF: OutlineColor
//!   (0.2155, 0.8865, 1), OutlineIntensity 2), flaring as the pods' does, while its
//!   Kismet leaves it on.
//! * **A purchase**: each console waits for its total to reach its price (its
//!   CheckIfFullSequence), wherever the deposits are made: then it plays
//!   ESC_MONEY_DEPOSIT_FULL, turns itself off (UnEquip_Console, its outline gone),
//!   and its Kismet runs (Minigame Success): what tools/build_doors.py's
//!   offline run of it found it does, each step at its time (the console's timeline):
//!   the Matinees it plays (the doors sliding, rising and sinking, the cover barriers
//!   folding, with their sounds), the Optimus doors it opens
//!   (TnSeqAct_TransformWorldProp: AP_OptimusDoor_Open, 3 s, with its notifies'
//!   sounds), the consoles it turns off (UnEquip_Console, the outline off: its own and
//!   its total's others, and on some maps more), the actors it hides, the player
//!   blockers it switches off and the spawn clusters it switches on; and on
//!   Downfall, the scaffolds it brings down (SeqAct_TnActivateDestructible: the
//!   Courtyard's and the Doors' consoles', DES_ASH_ESCBeam1_DSYS's
//!   HmKismetDestructionTrigger), which stood in the ways the purchase opens: each
//!   gone, its collision with it, as its crash (DEST_ASH_COLUMN_EXPL) plays at its
//!   foot. (The navigation opens the ways through them as the purchase's toggles
//!   say, nav.rs; with them standing, the enemies walked into them.)
//! * **Matinees** run as UE3's SeqAct_Interp: Play resumes where it is (one at its end
//!   starts again from the start), Reverse plays back, Pause pauses or resumes; their
//!   move tracks move each door by its offset from where it starts (UE3's curves:
//!   Hermite between keys with their tangents), the actors riding on a door with it;
//!   their sound tracks play as their keys are passed going forward (at their group's
//!   actors), and their animation tracks pose the skeletal doors.
//! * **Collision**: a door blocks where it stands as the level starts until it moves,
//!   and where it ends up once it gets there (neither as it moves; an Optimus door
//!   switches as its opening ends); a player blocker riding on a door blocks while
//!   the door is where it starts; the player blockers block players, not shots
//!   (collision.rs's groups).
//! * **Gameplay start**: the map's own start (its timeline): the covers held up (their
//!   Matinees paused just after they start), the spawn clusters behind the doors off
//!   (the waves bring no enemy in there till they open: waves.rs), the blockers it
//!   switches on. Each match starts the map's doors and consoles afresh.
//! * **Online**: each deposit is sent (Deposit) and added on every PC, where the
//!   consoles whose price it reaches are bought; the buyer's PC sends those purchases
//!   as well (Unlock), for a PC that missed a deposit.
//! * **The traps' consoles** (TnInteractCurrencyReceptacleMultiUse: Ignition's Flame
//!   Jets and GunShip Support, Oblivion's Combustion Chamber, Downfall's Defensive
//!   Sentries) stand on the floor (models/esc_console_floor.glb, the same clips) and
//!   can be bought again and again: a purchase clears their total
//!   (ReenableAfterCooldown's ClearTotal) and they come back on their Cooldown later
//!   (its WaitForCooldown, then FireEnableEvent: EnableIt, Equip_Console, the outline
//!   back), whatever their Kismet did to them meanwhile (one flag: the last word on
//!   it; one their Kismet turns off that wasn't bought stays off, as Downfall's
//!   hangar consoles turn each other off). Their Kismet switches the traps' effects,
//!   pain volumes and lights and wakes their sentries (traps.rs plays those), plays
//!   their sounds (a looping one, the flame jets' roar, held until its Stop: heard
//!   from the nearest point of its line, a line emitter's), and their Matinees turn
//!   what they move (the gunship circling), switch the effects as their toggle
//!   tracks say and pulse the lights (their Brightness tracks). A console's outline
//!   is the last word on it: its Kismet's, or its turning on or off (the game's
//!   OutlineIntensity, which both set). Consoles sharing a total are bought
//!   together, their timelines run once when they are the same.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use bevy::gltf::{Gltf, GltfLoaderSettings};
use bevy::prelude::*;

use crate::collision::LevelCollision;
use crate::hud::{HudEvent, HudState, Prompt};
use crate::net::JazzEvent;
use crate::{LocalPlayer, Player, PlayerInput, RemotePlayer, Score, Sfx};

/// The console's model (tools/build_doors.py): its clips, in this order, and its
/// outline's node.
pub const CONSOLE_MODEL: &str = "models/esc_console.glb";
/// The traps' floor console's model (its clips the same, in the same order).
pub const CONSOLE_FLOOR_MODEL: &str = "models/esc_console_floor.glb";
const ACTIVATE: usize = 0;
const EQUIP: usize = 1;
const UNEQUIP: usize = 2;
const OUTLINE_NODE: &str = "ConsoleOutline";
/// Its clips' length (49 frames at 30 a second each), until the model's own is known.
const CONSOLE_CLIP_SECS: f32 = 49.0 / 30.0;
/// The progress bar's speed: a share of the bar a second (the action's InterpSpeed).
const BAR_SPEED: f32 = 0.7;
/// The outline: OutlineColor and OutlineIntensity, flaring as the pods'
/// (recreator.rs) in its live material's shader (live.rs).
const OUTLINE_COLOR: [f32; 3] = [0.2155, 0.8865, 1.0];
const OUTLINE_INTENSITY: f32 = 2.0;
const OUTLINE_FLARE: f32 = 5.0;
const OUTLINE_FLARE_RATE: f32 = 0.2;
/// A scaffold coming down (its destroyed state's HmSoundDestructionEffectFactory).
const SND_COLLAPSE: &str = "BL_DEST_MTL.DEST_ASH_COLUMN_EXPL";
/// A deposit, and the one that makes a purchase.
const SND_DEPOSIT: &str = "BL_LVL_MP.ESC_MONEY_DEPOSIT";
const SND_FULL: &str = "BL_LVL_MP.ESC_MONEY_DEPOSIT_FULL";
/// The Optimus doors' opening's sound notifies (AP_OptimusDoor_Open's).
const NOTIFIES: [(&str, f32, &str); 3] = [
    (
        "AP_OptimusDoor_Open",
        0.0,
        "BL_LVL_02_OPTIPLEX.DOOR_IAC_OPTIMUS_OPEN_01",
    ),
    (
        "AP_OptimusDoor_Open",
        0.735,
        "BL_LVL_02_OPTIPLEX.DOOR_IAC_OPTIMUS_OPEN_02",
    ),
    (
        "AP_OptimusDoor_Open",
        1.599,
        "BL_LVL_02_OPTIPLEX.DOOR_IAC_OPTIMUS_OPEN_03",
    ),
];
/// A door counts as where it starts, or where it ends up, within this (metres).
const AT_REST: f32 = 0.02;
/// An animation track counts as at its start this far in (seconds): the covers'
/// Matinees are paused 0.01 s in as gameplay starts.
const START_SLACK: f32 = 0.05;
/// The longest step the doors take in a frame (seconds): a hitch slows them rather
/// than jumping them on.
const MAX_STEP: f32 = 0.1;
/// Gameplay start's first steps are run at once, in steps this long (seconds), up to
/// START_AT_ONCE in.
const START_STEP: f32 = 0.005;
const START_AT_ONCE: f32 = 0.1;
/// The owners the timelines' held sounds are held for (Sfx::hold): their numbers
/// above these bits, apart from any entity's.
const LOOP_OWNER: u64 = 0xD0_05 << 48;

/// A map's door models.
pub fn doors_model(stem: &str) -> String {
    format!("models/{stem}_doors.glb")
}

/// A name from a data file as the 'static string sounds and prompts take (each
/// kept once, for good).
fn kept(s: &str) -> &'static str {
    static KEPT: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut set = KEPT.get_or_init(Default::default).lock().unwrap();
    if let Some(k) = set.get(s) {
        return k;
    }
    let k: &'static str = Box::leak(s.to_string().into_boxed_str());
    set.insert(k);
    k
}

// ---------------------------------------------------------------------------
// The map's doors data
// ---------------------------------------------------------------------------

/// An unlock console: where it stands, the spot its user stands at, its price, its
/// deposit, its prompt (TransGame.int's line: `i the deposit, `c the price), the
/// total it adds to and its purchase's timeline.
#[derive(Clone, Debug)]
pub struct ConsoleSpot {
    pub id: String,
    pub at: Transform,
    pub spot: Vec3,
    pub cost: u32,
    pub deposit: u32,
    pub prompt: &'static str,
    pub total: usize,
    timeline: Option<usize>,
    /// A trap's floor console, and its cooldown (seconds from a purchase to its
    /// coming back on; 0: bought once).
    pub floor: bool,
    pub cooldown: f32,
}

/// What a trap's actor is (levels/<map>_doors.txt's emitter, pain and light lines).
#[derive(Clone, Debug)]
pub enum TrapThing {
    /// A particle effect (fx/weapons_fx.txt's system), its place and axes (Bevy's)
    /// and size (DrawScale3D), and whether the game lights it (its particle
    /// component accepts lights and has a light environment: levelfx.rs's
    /// FxSystems::set_lit; every trap's flames do).
    Emitter {
        system: String,
        at: Vec3,
        x: Vec3,
        y: Vec3,
        scale: Vec3,
        lit: bool,
    },
    Pain(PainVolume),
    /// A point light: its colour (linear), Brightness, Radius (metres) and
    /// FalloffExponent.
    Light {
        at: Vec3,
        color: Vec3,
        brightness: f32,
        radius: f32,
        falloff: f32,
    },
}

/// What a pain volume does to what is in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PainKind {
    /// TnDamageTypeFlameZone: it sets them on fire.
    Flame,
    /// TnDamageTypeEnvironmentExplosion.
    Explosion,
    Other,
}

/// A trap's pain volume (PhysicsVolume): its DamagePerSec, done to the players
/// too (bAffectPlayer) or only to the AI, and its brush.
#[derive(Clone, Debug)]
pub struct PainVolume {
    pub dps: f32,
    pub players: bool,
    pub kind: PainKind,
    /// Its faces: a point p is in it where n.p <= d for each (n, d).
    pub planes: Vec<(Vec3, f32)>,
}

impl PainVolume {
    pub fn contains(&self, p: Vec3) -> bool {
        !self.planes.is_empty() && self.planes.iter().all(|(n, d)| n.dot(p) <= *d + 1e-4)
    }
}

/// A trap's actor, and whether it is on as the level starts.
#[derive(Clone, Debug)]
pub struct TrapActor {
    pub id: String,
    pub thing: TrapThing,
    pub on: bool,
}

/// A trap's sentry kind: Downfall's rocket turrets (RocketInteract_Sentry_TURRETDEF)
/// or the gunship's machine guns (MGInteract_TURRETDEF).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SentryKind {
    Rocket,
    Mg,
}

/// A sentry a trap wakes: spawned `spawn` seconds into the match where its
/// factory's spawn point is (turned so, its mesh `scale` times its size), or riding
/// a door's bone (the gunship's: the door, the bone, its offset and turn there).
#[derive(Clone, Debug)]
pub struct SentrySpot {
    pub id: String,
    pub kind: SentryKind,
    pub at: Vec3,
    pub turn: Quat,
    pub scale: f32,
    pub spawn: f32,
    pub rides: Option<(usize, String, Vec3, Quat)>,
}

/// A door as the level starts: where it is (and for a skeletal one its turn and
/// size, and the pose it starts in: an animation of the map's doors model and a
/// time in it), its scene in that model, and its collision groups where it stands
/// closed and where it ends up open.
#[derive(Clone, Debug)]
pub struct DoorSpot {
    pub id: String,
    pub at: Vec3,
    pub turn: Quat,
    pub scale: Vec3,
    pub skeletal: bool,
    pub scene: usize,
    pub closed: Option<u16>,
    pub open: Option<u16>,
    pose: Option<(usize, f32)>,
    /// Moved by a Matinee (its own move track, or its base's), and where its
    /// Matinees leave it (the last key's offset); sized by one (its DrawScale).
    moved: bool,
    end: Vec3,
    scaled: bool,
    /// What rides on it: doors, and blocking volumes (their numbers).
    riders: Vec<usize>,
    rider_volumes: Vec<usize>,
    /// The door it rides on (riders move with their base).
    base: Option<usize>,
    /// A prop a purchase brings down: where it comes down (the middle of its foot).
    pub collapse: Option<Vec3>,
    /// What it does as it comes down (crumble lines): its destructible's broken
    /// state, set off where the actor stands and as it is turned (deaths.rs's Place,
    /// as barrels.rs sets off a barrel's).
    pub crumble: Option<Crumble>,
}

/// A prop's broken state (DES_ASH_ESCBeam1_DSYS's, Downfall's scaffolds': the
/// beam's pieces thrown, its falling bits), where to set it off.
#[derive(Clone, Debug)]
pub struct Crumble {
    pub state: Arc<crate::deaths::Destructible>,
    pub at: Vec3,
    pub turn: Quat,
}

impl Crumble {
    /// Its place in deaths.rs's terms (its forward the demo's -Z turned; the map's
    /// axes are Unreal's x, z, y).
    pub fn place(&self) -> crate::deaths::Place {
        crate::deaths::Place {
            at: self.at,
            turn: self.turn * Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
            hit: self.at,
            momentum: Vec3::ZERO,
            velocity: Vec3::ZERO,
        }
    }
}

/// A Matinee's move track key: time, offset, interpolation (b'L' linear, b'C'
/// constant, else the curve) and its tangents.
#[derive(Clone, Copy, Debug)]
struct Key {
    t: f32,
    v: Vec3,
    mode: u8,
    arrive: Vec3,
    leave: Vec3,
}

/// An animation track's key: from `start` (Matinee time) the door plays `anim`
/// (an animation of the map's doors model).
#[derive(Clone, Debug)]
struct AnimKey {
    start: f32,
    anim: usize,
    start_offset: f32,
    end_offset: f32,
    rate: f32,
    looping: bool,
    reverse: bool,
}

/// Where a sound plays: at a door (where it is now), a place, or along a line (a
/// line emitter's: from its middle less to plus the half given; heard from its
/// point nearest the listener).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    Door(usize),
    At(Vec3),
    Line(Vec3, Vec3),
}

#[derive(Clone, Debug)]
struct Matinee {
    length: f32,
    rate: f32,
    looping: bool,
    moves: Vec<(usize, Vec<Key>)>,
    /// Its blocking volumes' moves, and its doors' sizes (DrawScale: the keys' x).
    volume_moves: Vec<(usize, Vec<Key>)>,
    scales: Vec<(usize, Vec<Key>)>,
    sounds: Vec<(f32, &'static str, Vec<Place>)>,
    /// Each posed door's keys, in order of time.
    anims: Vec<(usize, Vec<AnimKey>)>,
    /// Its doors' turns: the door's turn as the level has it (Bevy's axes) and its
    /// rotation keys (roll, pitch and yaw in degrees in the keys' v: relative to it).
    turns: Vec<(usize, Quat, Vec<Key>)>,
    /// Its toggle tracks' keys: when, how, and the traps' actors switched.
    toggles: Vec<(f32, Switch, Vec<usize>)>,
    /// Its lights' Brightness tracks (the keys' x), by trap actor.
    brightness: Vec<(usize, Vec<Key>)>,
}

/// What a purchase (or gameplay start) does.
#[derive(Clone, Debug, PartialEq)]
enum Op {
    Play(usize),
    Reverse(usize),
    Pause(usize),
    Resume(usize),
    Stop(usize),
    Switch(Vec<Target>, Switch),
    Outline(Vec<usize>, bool),
    Hide(Vec<usize>, bool),
    Transform(Vec<usize>, bool),
    Sound(&'static str, Vec<Place>),
    /// A looping sound stopped (SeqAct_PlaySound's Stop).
    SoundStop(&'static str, Vec<Place>),
    /// Sentries woken (true) or put to sleep (SeqAct_TnAiSentrySetDormant).
    Wake(Vec<usize>, bool),
    /// Props brought down (SeqAct_TnActivateDestructible): gone, with their
    /// collision.
    Collapse(Vec<usize>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Switch {
    On,
    Off,
    Toggle,
}

impl Switch {
    fn apply(self, on: bool) -> bool {
        match self {
            Switch::On => true,
            Switch::Off => false,
            Switch::Toggle => !on,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Console(usize),
    Volume(usize),
    Cluster(usize),
    /// A trap's actor (an effect, a pain volume, a light).
    Trap(usize),
}

/// A map's doors (levels/<map>_doors.txt and .bin).
#[derive(Clone, Debug, Default)]
pub struct DoorsData {
    pub stem: String,
    pub consoles: Vec<ConsoleSpot>,
    pub doors: Vec<DoorSpot>,
    /// The blocking volumes: their names and collision groups.
    pub volumes: Vec<(String, Option<u16>)>,
    /// The doors model's animations: their names and lengths (seconds).
    anims: Vec<(String, f32)>,
    matinees: Vec<Matinee>,
    /// The timelines (each its steps in order of time), the totals' names, and
    /// gameplay start's timeline.
    timelines: Vec<Vec<(f32, Op)>>,
    totals: Vec<String>,
    start: Option<usize>,
    /// The spawn clusters: their names and spawn points (numbers among the points
    /// file's enemy lines).
    pub clusters: Vec<(String, Vec<usize>)>,
    /// The traps' actors and sentries (traps.rs plays them).
    pub traps: Vec<TrapActor>,
    pub sentries: Vec<SentrySpot>,
    /// The cues the timelines start that loop (held until stopped): known once the
    /// sound bank is (run_doors).
    looping: HashSet<&'static str>,
    looping_known: bool,
}

fn floats(s: &str) -> Vec<f32> {
    s.split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect()
}

fn vec3(s: &str) -> Option<Vec3> {
    let v = floats(s);
    (v.len() >= 3).then(|| Vec3::new(v[0], v[1], v[2]))
}

fn quat(s: &str) -> Option<Quat> {
    let v = floats(s);
    (v.len() >= 4).then(|| Quat::from_xyzw(v[0], v[1], v[2], v[3]).normalize())
}

fn group(s: &str) -> Option<u16> {
    s.trim()
        .parse::<i32>()
        .ok()
        .filter(|g| *g >= 0)
        .map(|g| g as u16)
}

fn num<T: std::str::FromStr>(s: &str, or: T) -> T {
    s.trim().parse().unwrap_or(or)
}

impl DoorsData {
    /// A map's doors (none if it has no doors file), with their collision groups
    /// put into `collision` (the doors where they stand closed, the volumes on).
    pub fn load(dir: &Path, stem: &str, collision: LevelCollision) -> (DoorsData, LevelCollision) {
        let path = dir.join(format!("levels/{stem}_doors.txt"));
        let Ok(text) = std::fs::read_to_string(&path) else {
            return (DoorsData::default(), collision);
        };
        let groups =
            match crate::collision::load_groups(&dir.join(format!("levels/{stem}_doors.bin"))) {
                Ok(g) => g,
                Err(e) => {
                    warn!(
                        "levels/{stem}_doors.bin can't be read ({e}): the doors have no collision"
                    );
                    Vec::new()
                }
            };
        let kinds = |kind: &str| groups.iter().filter(|g| g.name.starts_with(kind)).count();
        let counts = (kinds("closed:"), kinds("open:"), kinds("volume:"));
        let parts: Vec<(Vec<[f32; 9]>, crate::collision::Blocks)> =
            groups.into_iter().map(|g| (g.tris, g.blocks)).collect();
        let (mut collision, ids) = collision.with_groups(&parts);
        let id = |g: Option<u16>| g.and_then(|g| ids.get(g as usize).copied());
        let mut d = DoorsData {
            stem: stem.to_string(),
            ..default()
        };
        let rows: Vec<Vec<&str>> = text
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.split('\t').collect())
            .collect();

        // The places (doors, volumes, spots) and the clips first, then what refers to
        // them.
        let mut prompts: HashMap<&str, &'static str> = HashMap::new();
        let mut door_of: HashMap<&str, usize> = HashMap::new();
        let mut volume_of: HashMap<&str, usize> = HashMap::new();
        let mut spots: HashMap<&str, Place> = HashMap::new();
        let mut clip_of: HashMap<(usize, &str), usize> = HashMap::new();
        let mut trap_of: HashMap<&str, usize> = HashMap::new();
        for r in &rows {
            let trap = match (r[0], r.len()) {
                ("prompt", 3..) => {
                    prompts.insert(r[1], kept(r[2]));
                    None
                }
                ("clip", 5..) => {
                    let anim: usize = num(r[3], 0);
                    if d.anims.len() <= anim {
                        d.anims.resize(anim + 1, (String::new(), 0.0));
                    }
                    d.anims[anim] = (r[2].to_string(), num(r[4], 0.0));
                    clip_of.insert((num(r[1], 0), r[2]), anim);
                    None
                }
                ("volume", 3..) => {
                    volume_of.insert(r[1], d.volumes.len());
                    d.volumes.push((r[1].to_string(), id(group(r[2]))));
                    None
                }
                ("spot", 3..) => {
                    let at = vec3(r[2]).unwrap_or_default();
                    let place = match r.get(3).and_then(|h| vec3(h)) {
                        Some(half) => Place::Line(at, half),
                        None => Place::At(at),
                    };
                    spots.insert(r[1], place);
                    None
                }
                ("emitter", 8..) => Some((
                    TrapThing::Emitter {
                        system: r[2].to_string(),
                        at: vec3(r[3]).unwrap_or_default(),
                        x: vec3(r[4]).unwrap_or(Vec3::X),
                        y: vec3(r[5]).unwrap_or(Vec3::Z),
                        scale: vec3(r[6]).unwrap_or(Vec3::ONE),
                        lit: r.get(8).is_some_and(|l| l.trim() == "1"),
                    },
                    r[7].trim() == "1",
                )),
                ("pain", 11..) => {
                    let kind = match r[5] {
                        "flame" => PainKind::Flame,
                        "explosion" => PainKind::Explosion,
                        _ => PainKind::Other,
                    };
                    let planes = r[7..]
                        .iter()
                        .filter_map(|p| {
                            let v = floats(p);
                            (v.len() >= 4).then(|| (Vec3::new(v[0], v[1], v[2]), v[3]))
                        })
                        .collect();
                    Some((
                        TrapThing::Pain(PainVolume {
                            dps: num(r[2], 0.0),
                            players: r[3] == "1",
                            kind,
                            planes,
                        }),
                        // (A pain volume pains while bPainCausing; it is off till its
                        // purchase turns it on, by gameplay start's Kismet.)
                        r[6].trim() == "1",
                    ))
                }
                ("light", 8..) => Some((
                    TrapThing::Light {
                        at: vec3(r[2]).unwrap_or_default(),
                        color: vec3(r[3]).unwrap_or(Vec3::ONE),
                        brightness: num(r[4], 1.0),
                        radius: num(r[5], 10.0),
                        falloff: num(r[6], 2.0),
                    },
                    r[7].trim() == "1",
                )),
                _ => None,
            };
            if let Some((thing, on)) = trap {
                trap_of.insert(r[1], d.traps.len());
                d.traps.push(TrapActor {
                    id: r[1].to_string(),
                    thing,
                    on,
                });
            }
        }
        for r in &rows {
            let door = match (r[0], r.len()) {
                ("door", 6..) => DoorSpot {
                    id: r[1].to_string(),
                    at: vec3(r[2]).unwrap_or_default(),
                    turn: Quat::IDENTITY,
                    scale: Vec3::ONE,
                    skeletal: false,
                    scene: num(r[3], 0),
                    closed: id(group(r[4])),
                    open: id(group(r[5])),
                    pose: None,
                    moved: false,
                    end: Vec3::ZERO,
                    scaled: false,
                    riders: Vec::new(),
                    rider_volumes: Vec::new(),
                    base: None,
                    collapse: None,
                    crumble: None,
                },
                ("skeletal", 10..) => {
                    let scene = num(r[5], 0);
                    DoorSpot {
                        id: r[1].to_string(),
                        at: vec3(r[2]).unwrap_or_default(),
                        turn: quat(r[3]).unwrap_or_default(),
                        scale: vec3(r[4]).unwrap_or(Vec3::ONE),
                        skeletal: true,
                        scene,
                        closed: id(group(r[6])),
                        open: id(group(r[7])),
                        pose: clip_of.get(&(scene, r[8])).map(|&a| (a, num(r[9], 0.0))),
                        moved: false,
                        end: Vec3::ZERO,
                        scaled: false,
                        riders: Vec::new(),
                        rider_volumes: Vec::new(),
                        base: None,
                        collapse: None,
                        crumble: None,
                    }
                }
                _ => continue,
            };
            door_of.insert(r[1], d.doors.len());
            d.doors.push(door);
        }
        for r in &rows {
            if r[0] == "collapse"
                && r.len() >= 3
                && let Some(&i) = door_of.get(r[1])
            {
                d.doors[i].collapse = vec3(r[2]);
            }
        }
        // (What they do as they come down: their broken states, dsys lines as
        // deaths.rs reads a destructible's.)
        let dsys: String = text
            .lines()
            .filter(|l| l.starts_with("dsys\t"))
            .flat_map(|l| [l, "\n"])
            .collect();
        let states = crate::deaths::DeathData::parse(&dsys).destructibles;
        for r in &rows {
            if r[0] == "crumble"
                && r.len() >= 5
                && let Some(&i) = door_of.get(r[1])
                && let Some(state) = states.get(r[2])
            {
                d.doors[i].crumble = Some(Crumble {
                    state: state.clone(),
                    at: vec3(r[3]).unwrap_or(d.doors[i].at),
                    turn: quat(r[4]).unwrap_or_default(),
                });
            }
        }
        let place = |name: &str| {
            door_of
                .get(name)
                .map(|&i| Place::Door(i))
                .or_else(|| spots.get(name).copied())
        };

        // The timelines, the totals and the consoles.
        let mut timeline_of: HashMap<&str, usize> = HashMap::new();
        for r in &rows {
            if r[0] == "do" && r.len() >= 4 && !timeline_of.contains_key(r[1]) {
                timeline_of.insert(r[1], timeline_of.len());
            }
        }
        d.timelines = vec![Vec::new(); timeline_of.len()];
        d.start = timeline_of.get("start").copied();
        let mut total_of: HashMap<String, usize> = HashMap::new();
        for r in &rows {
            if r[0] != "console" || r.len() < 9 {
                continue;
            }
            let at = Transform::from_translation(vec3(r[2]).unwrap_or_default())
                .with_rotation(quat(r[3]).unwrap_or_default());
            // (A console that names no data store keeps its own total.)
            let name = if r[8].trim() == "-" {
                r[1]
            } else {
                r[8].trim()
            };
            let n = total_of.len();
            let total = *total_of.entry(name.to_string()).or_insert(n);
            let floor = r.get(9).is_some_and(|k| k.trim() == "floor");
            d.consoles.push(ConsoleSpot {
                id: r[1].to_string(),
                at,
                spot: vec3(r[4]).unwrap_or(at.translation),
                cost: num(r[5], 0),
                deposit: num(r[6], 250u32).max(1),
                prompt: prompts.get(r[7]).copied().unwrap_or(""),
                total,
                timeline: timeline_of.get(r[1]).copied(),
                floor,
                cooldown: r.get(10).map_or(0.0, |c| num(c, 0.0f32).max(0.0)),
            });
        }
        d.totals = vec![String::new(); total_of.len()];
        for (name, i) in total_of {
            d.totals[i] = name;
        }
        let console_of: HashMap<&str, usize> = d
            .consoles
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id.as_str(), i))
            .collect();

        // What rides on what, the spawn clusters and the Matinees.
        let mut matinee_of: HashMap<&str, usize> = HashMap::new();
        for r in &rows {
            match (r[0], r.len()) {
                ("rider", 3..) => {
                    if let Some(&base) = door_of.get(r[1]) {
                        if let Some(&rider) = door_of.get(r[2]) {
                            d.doors[base].riders.push(rider);
                            d.doors[rider].base = Some(base);
                        } else if let Some(&v) = volume_of.get(r[2]) {
                            d.doors[base].rider_volumes.push(v);
                        }
                    }
                }
                ("cluster", 2..) => {
                    let points = r[2..]
                        .iter()
                        .filter_map(|p| p.trim().parse().ok())
                        .collect();
                    d.clusters.push((r[1].to_string(), points));
                }
                ("matinee", 5..) => {
                    matinee_of.insert(r[1], d.matinees.len());
                    d.matinees.push(Matinee {
                        length: num(r[2], 0.0),
                        rate: num(r[3], 1.0),
                        looping: r[4].trim() == "1",
                        moves: Vec::new(),
                        volume_moves: Vec::new(),
                        scales: Vec::new(),
                        sounds: Vec::new(),
                        anims: Vec::new(),
                        turns: Vec::new(),
                        toggles: Vec::new(),
                        brightness: Vec::new(),
                    });
                }
                // A trap's sentry (riding a door's bone: the gunship's guns).
                ("sentry", 7..) => {
                    let rides = (r.len() >= 11)
                        .then(|| {
                            door_of.get(r[7]).map(|&door| {
                                (
                                    door,
                                    r[8].to_string(),
                                    vec3(r[9]).unwrap_or_default(),
                                    quat(r[10]).unwrap_or_default(),
                                )
                            })
                        })
                        .flatten();
                    d.sentries.push(SentrySpot {
                        id: r[1].to_string(),
                        kind: if r[2] == "rocket" {
                            SentryKind::Rocket
                        } else {
                            SentryKind::Mg
                        },
                        at: vec3(r[3]).unwrap_or_default(),
                        turn: quat(r[4]).unwrap_or_default(),
                        scale: num(r[5], 1.0),
                        spawn: num(r[6], 0.0),
                        rides,
                    });
                }
                _ => {}
            }
        }
        let sentry_of: HashMap<&str, usize> = d
            .sentries
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.as_str(), i))
            .collect();
        let cluster_of: HashMap<&str, usize> = d
            .clusters
            .iter()
            .enumerate()
            .map(|(i, c)| (c.0.as_str(), i))
            .collect();
        for r in &rows {
            match (r[0], r.len()) {
                ("move", 4..) => {
                    let Some(&m) = matinee_of.get(r[1]) else {
                        continue;
                    };
                    let keys: Vec<Key> = r[3..].iter().filter_map(|k| parse_key(k)).collect();
                    let Some(last) = keys.last() else {
                        continue;
                    };
                    if let Some(&door) = door_of.get(r[2]) {
                        d.doors[door].end = last.v;
                        d.doors[door].moved = true;
                        d.matinees[m].moves.push((door, keys));
                    } else if let Some(&v) = volume_of.get(r[2]) {
                        d.matinees[m].volume_moves.push((v, keys));
                    }
                }
                ("turn", 5..) => {
                    let (Some(&m), Some(&door)) = (matinee_of.get(r[1]), door_of.get(r[2])) else {
                        continue;
                    };
                    let keys: Vec<Key> = r[4..].iter().filter_map(|k| parse_key(k)).collect();
                    if !keys.is_empty() {
                        d.matinees[m]
                            .turns
                            .push((door, quat(r[3]).unwrap_or_default(), keys));
                    }
                }
                ("toggle", 5..) => {
                    let Some(&m) = matinee_of.get(r[1]) else {
                        continue;
                    };
                    let switch = match r[3] {
                        "on" => Switch::On,
                        "off" => Switch::Off,
                        _ => Switch::Toggle,
                    };
                    let who: Vec<usize> = r[4..]
                        .iter()
                        .filter_map(|a| trap_of.get(a).copied())
                        .collect();
                    d.matinees[m].toggles.push((num(r[2], 0.0), switch, who));
                }
                ("brightness", 4..) => {
                    let (Some(&m), Some(&light)) = (matinee_of.get(r[1]), trap_of.get(r[2])) else {
                        continue;
                    };
                    let keys: Vec<Key> = r[3..].iter().filter_map(|k| parse_scale_key(k)).collect();
                    if !keys.is_empty() {
                        d.matinees[m].brightness.push((light, keys));
                    }
                }
                ("scale", 4..) => {
                    let (Some(&m), Some(&door)) = (matinee_of.get(r[1]), door_of.get(r[2])) else {
                        continue;
                    };
                    let keys: Vec<Key> = r[3..].iter().filter_map(|k| parse_scale_key(k)).collect();
                    if !keys.is_empty() {
                        d.doors[door].scaled = true;
                        d.matinees[m].scales.push((door, keys));
                    }
                }
                ("sound", 4..) => {
                    if let Some(&m) = matinee_of.get(r[1]) {
                        let at = r[4..].iter().filter_map(|a| place(a)).collect();
                        d.matinees[m].sounds.push((num(r[2], 0.0), kept(r[3]), at));
                    }
                }
                ("anim", 10..) => {
                    let (Some(&m), Some(&door)) = (matinee_of.get(r[1]), door_of.get(r[2])) else {
                        continue;
                    };
                    let Some(&anim) = clip_of.get(&(d.doors[door].scene, r[4])) else {
                        continue;
                    };
                    let key = AnimKey {
                        start: num(r[3], 0.0),
                        anim,
                        start_offset: num(r[5], 0.0),
                        end_offset: num(r[6], 0.0),
                        rate: num(r[7], 1.0),
                        looping: r[8] == "1",
                        reverse: r[9].trim() == "1",
                    };
                    let anims = &mut d.matinees[m].anims;
                    match anims.iter_mut().find(|(i, _)| *i == door) {
                        Some((_, keys)) => keys.push(key),
                        None => anims.push((door, vec![key])),
                    }
                }
                ("do", 4..) => {
                    let Some(&tl) = timeline_of.get(r[1]) else {
                        continue;
                    };
                    let t: f32 = num(r[2], 0.0);
                    let args = &r[4..];
                    let matinee = || args.first().and_then(|a| matinee_of.get(*a)).copied();
                    let targets = || {
                        args.iter()
                            .filter_map(|a| {
                                console_of
                                    .get(*a)
                                    .map(|&c| Target::Console(c))
                                    .or_else(|| volume_of.get(*a).map(|&v| Target::Volume(v)))
                                    .or_else(|| cluster_of.get(*a).map(|&c| Target::Cluster(c)))
                                    .or_else(|| trap_of.get(*a).map(|&t| Target::Trap(t)))
                            })
                            .collect::<Vec<_>>()
                    };
                    let sentries = || {
                        args.iter()
                            .filter_map(|a| sentry_of.get(*a).copied())
                            .collect::<Vec<_>>()
                    };
                    let doors = || {
                        args.iter()
                            .filter_map(|a| door_of.get(*a).copied())
                            .collect::<Vec<_>>()
                    };
                    let consoles = || {
                        args.iter()
                            .filter_map(|a| console_of.get(*a).copied())
                            .collect::<Vec<_>>()
                    };
                    let op = match r[3] {
                        "play" => matinee().map(Op::Play),
                        "reverse" => matinee().map(Op::Reverse),
                        "pause" => matinee().map(Op::Pause),
                        "resume" => matinee().map(Op::Resume),
                        "stop" => matinee().map(Op::Stop),
                        "on" => Some(Op::Switch(targets(), Switch::On)),
                        "off" => Some(Op::Switch(targets(), Switch::Off)),
                        "toggle" => Some(Op::Switch(targets(), Switch::Toggle)),
                        "outline_on" => Some(Op::Outline(consoles(), true)),
                        "outline_off" => Some(Op::Outline(consoles(), false)),
                        "hide" => Some(Op::Hide(doors(), true)),
                        "unhide" => Some(Op::Hide(doors(), false)),
                        "transform" => Some(Op::Transform(doors(), true)),
                        "untransform" => Some(Op::Transform(doors(), false)),
                        "sound" if !args.is_empty() => Some(Op::Sound(
                            kept(args[0]),
                            args[1..].iter().filter_map(|a| place(a)).collect(),
                        )),
                        "sound_stop" if !args.is_empty() => Some(Op::SoundStop(
                            kept(args[0]),
                            args[1..].iter().filter_map(|a| place(a)).collect(),
                        )),
                        "wake" => Some(Op::Wake(sentries(), true)),
                        "sleep" => Some(Op::Wake(sentries(), false)),
                        "collapse" => Some(Op::Collapse(doors())),
                        _ => None,
                    };
                    if let Some(op) = op {
                        d.timelines[tl].push((t, op));
                    }
                }
                _ => {}
            }
        }
        for steps in &mut d.timelines {
            steps.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        for m in &mut d.matinees {
            for (_, keys) in &mut m.anims {
                keys.sort_by(|a, b| a.start.total_cmp(&b.start));
            }
            m.toggles.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        // (Riders go where their base goes.)
        for i in 0..d.doors.len() {
            if let Some(base) = d.doors[i].base
                && !d.doors[i].moved
            {
                d.doors[i].moved = d.doors[base].moved;
                d.doors[i].end = d.doors[base].end;
            }
        }
        // As the level starts: the doors closed (not where they end up).
        for door in &d.doors {
            if let Some(g) = door.open {
                collision.set_group(g, false);
            }
        }
        info!(
            "{stem}: {} unlock consoles ({} the traps'), {} doors, {} Matinees, {} blocking \
             volumes, {} trap actors, {} sentries; collision for {} doors closed, {} open, \
             {} volumes",
            d.consoles.len(),
            d.consoles.iter().filter(|c| c.floor).count(),
            d.doors.len(),
            d.matinees.len(),
            d.volumes.len(),
            d.traps.len(),
            d.sentries.len(),
            counts.0,
            counts.1,
            counts.2
        );
        (d, collision)
    }

    /// The enemy spawn points (numbers among the points file's enemy lines) behind
    /// the doors as gameplay starts: in the spawn clusters its start switches off
    /// and in no other.
    #[cfg(test)]
    pub fn locked_spawn_points(&self) -> HashSet<usize> {
        let mut on = vec![true; self.clusters.len()];
        if let Some(start) = self.start {
            for (_, op) in &self.timelines[start] {
                if let Op::Switch(targets, switch) = op {
                    for t in targets {
                        if let Target::Cluster(c) = *t {
                            on[c] = switch.apply(on[c]);
                        }
                    }
                }
            }
        }
        let points = |want: bool| {
            self.clusters
                .iter()
                .zip(&on)
                .filter(move |(_, on)| **on == want)
                .flat_map(|(c, _)| c.1.iter().copied())
        };
        let open: HashSet<usize> = points(true).collect();
        points(false).filter(|p| !open.contains(p)).collect()
    }

    fn anim_secs(&self, anim: usize) -> f32 {
        self.anims.get(anim).map_or(0.0, |a| a.1)
    }
}

/// A move track key, "t dx dy dz mode ax ay az lx ly lz".
fn parse_key(k: &str) -> Option<Key> {
    let w: Vec<&str> = k.split_whitespace().collect();
    if w.len() < 11 {
        return None;
    }
    let n = |i: usize| num(w[i], 0.0f32);
    Some(Key {
        t: n(0),
        v: Vec3::new(n(1), n(2), n(3)),
        mode: w[4].bytes().next().unwrap_or(b'A'),
        arrive: Vec3::new(n(5), n(6), n(7)),
        leave: Vec3::new(n(8), n(9), n(10)),
    })
}

/// A DrawScale track key, "t scale mode arrive leave" (as a move key along x).
fn parse_scale_key(k: &str) -> Option<Key> {
    let w: Vec<&str> = k.split_whitespace().collect();
    if w.len() < 5 {
        return None;
    }
    let n = |i: usize| num(w[i], 0.0f32);
    Some(Key {
        t: n(0),
        v: Vec3::new(n(1), 0.0, 0.0),
        mode: w[2].bytes().next().unwrap_or(b'A'),
        arrive: Vec3::new(n(3), 0.0, 0.0),
        leave: Vec3::new(n(4), 0.0, 0.0),
    })
}

/// UE3's InterpCurveVector at `t`: Hermite between keys with their tangents, linear
/// or constant from a key that says so.
fn eval_keys(keys: &[Key], t: f32) -> Vec3 {
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
        return Vec3::ZERO;
    };
    if t <= first.t {
        return first.v;
    }
    if t >= last.t {
        return last.v;
    }
    for pair in keys.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if t > b.t {
            continue;
        }
        let span = b.t - a.t;
        let s = if span > 0.0 { (t - a.t) / span } else { 0.0 };
        return match a.mode {
            b'C' => a.v,
            b'L' => a.v.lerp(b.v, s),
            _ => {
                let (s2, s3) = (s * s, s * s * s);
                let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
                let h10 = s3 - 2.0 * s2 + s;
                let h01 = -2.0 * s3 + 3.0 * s2;
                let h11 = s3 - s2;
                a.v * h00 + a.leave * (h10 * span) + b.v * h01 + b.arrive * (h11 * span)
            }
        };
    }
    last.v
}

/// Where a door is in its opening: where it starts, on the way, or where it ends up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Start,
    Moving,
    End,
}

impl Phase {
    /// Its collision then: closed on, open on.
    fn collision(self) -> (bool, bool) {
        match self {
            Phase::Start => (true, false),
            Phase::Moving => (false, false),
            Phase::End => (false, true),
        }
    }
}

/// Where an animation track's key has its door `run` seconds after it starts: the
/// time in the clip (of `length` seconds), and how far along the key is.
fn anim_time(k: &AnimKey, length: f32, run: f32) -> (f32, Phase) {
    let span = (length - k.start_offset - k.end_offset).max(0.0);
    let mut played = run.max(0.0) * k.rate;
    let phase = if k.looping || run <= START_SLACK {
        Phase::Start
    } else if played >= span {
        Phase::End
    } else {
        Phase::Moving
    };
    if k.looping && span > 0.0 {
        played = played.rem_euclid(span);
    }
    let played = played.clamp(0.0, span);
    let t = if k.reverse {
        length - k.end_offset - played
    } else {
        k.start_offset + played
    };
    (t, phase)
}

// ---------------------------------------------------------------------------
// In play
// ---------------------------------------------------------------------------

/// A console in the map, by its number.
#[derive(Component)]
pub struct Console {
    pub index: usize,
}

/// A door in the map, by its number.
#[derive(Component)]
pub struct Door {
    pub index: usize,
}

#[derive(Clone, Debug)]
struct ConsoleState {
    enabled: bool,
    /// Its outline shown (the last word on it: its Kismet's, or its turning on or
    /// off, as the game's OutlineIntensity has it).
    outline: bool,
    bought: bool,
    /// The progress bar as shown (0..1).
    bar: f32,
    /// A clip it plays through (Equip_Console or UnEquip_Console), and how far.
    playing: Option<(usize, f32)>,
    /// A trap's console bought: when (the doors' clock) its cooldown ends.
    reenable_at: Option<f32>,
}

#[derive(Clone, Debug, Default)]
struct MatineeState {
    pos: f32,
    playing: bool,
    reverse: bool,
    paused: bool,
    /// Played since the level started (its tracks drive what they bind).
    touched: bool,
}

#[derive(Clone, Debug, Default)]
struct DoorState {
    offset: Vec3,
    /// Its size as a Matinee has it (None: its own).
    scale: Option<f32>,
    hidden: bool,
    /// Its pose: an animation and the time in it.
    pose: Option<(usize, f32)>,
    /// How far its animation track has it (since a Matinee posed it).
    anim_phase: Option<Phase>,
    /// A transform it plays (forward, or back).
    transforming: Option<bool>,
    /// The collision it was last given (closed on, open on).
    shown: Option<(bool, bool)>,
    /// Its turn as a Matinee has it (None: its own).
    turn: Option<Quat>,
    /// Brought down (a collapse): gone, its collision too.
    down: bool,
}

/// What a match does to the map's doors.
#[derive(Default)]
struct Live {
    started: bool,
    clock: f32,
    consoles: Vec<ConsoleState>,
    totals: Vec<u32>,
    matinees: Vec<MatineeState>,
    doors: Vec<DoorState>,
    volumes: Vec<bool>,
    clusters: Vec<bool>,
    /// The timelines running: which, when they began, and their next step.
    running: Vec<(usize, f32, usize)>,
    /// The blocking volumes' offsets from where they start.
    volume_offsets: Vec<Vec3>,
    /// The consoles bought since nav.rs last asked (it opens the paths their
    /// Kismet opens).
    fresh: Vec<usize>,
    /// The traps' actors on or off, and the lights' brightness as their Matinees
    /// have it.
    traps: Vec<bool>,
    brightness: Vec<Option<f32>>,
    /// The traps' sentries awake (not dormant).
    awake: Vec<bool>,
    /// The looping sounds the timelines started and haven't stopped, each with the
    /// number it is held by.
    loops: Vec<(u64, &'static str, Place)>,
    next_loop: u64,
    /// The props brought down since run_doors last set off their broken states,
    /// and the pieces' dice.
    fallen: Vec<usize>,
    seed: u32,
}

impl Live {
    fn new(d: &DoorsData) -> Self {
        Live {
            consoles: vec![
                ConsoleState {
                    enabled: true,
                    outline: true,
                    bought: false,
                    bar: 0.0,
                    playing: None,
                    reenable_at: None,
                };
                d.consoles.len()
            ],
            traps: d.traps.iter().map(|t| t.on).collect(),
            brightness: vec![None; d.traps.len()],
            awake: vec![true; d.sentries.len()],
            totals: vec![0; d.totals.len()],
            matinees: vec![MatineeState::default(); d.matinees.len()],
            doors: d
                .doors
                .iter()
                .map(|door| DoorState {
                    pose: door.pose,
                    ..default()
                })
                .collect(),
            volumes: vec![true; d.volumes.len()],
            volume_offsets: vec![Vec3::ZERO; d.volumes.len()],
            clusters: vec![true; d.clusters.len()],
            seed: 0x5ca1_f01d,
            ..default()
        }
    }
}

/// A console's or skeletal door's parts in the map: its animation player and (a
/// console's) its outline's meshes.
#[derive(Clone, Debug, Default)]
struct Parts {
    player: Option<Entity>,
    outline: Vec<Entity>,
}

/// The models the map's consoles and doors come from (each file asked for as a
/// whole: its parts asked for one by one would have it read again for each, every
/// part already in made again and its scenes spawned again), kept while the map is
/// in play: the console's, the floor console's and the map's doors model, those it
/// needs.
#[derive(Default)]
struct Files {
    console: Option<Handle<Gltf>>,
    floor: Option<Handle<Gltf>>,
    doors: Option<Handle<Gltf>>,
}

/// A console's or door's scene to come from its model once the model is in and the
/// graphs are made (door_scenes): the file and the scene's number in it.
#[derive(Component)]
struct SceneOf {
    file: Handle<Gltf>,
    scene: usize,
}

struct Graphs {
    console: Handle<AnimationGraph>,
    console_nodes: Vec<AnimationNodeIndex>,
    console_clips: Vec<Handle<AnimationClip>>,
    /// The floor console's (the traps'), if the map has one.
    floor: Option<Handle<AnimationGraph>>,
    floor_nodes: Vec<AnimationNodeIndex>,
    floor_clips: Vec<Handle<AnimationClip>>,
    doors: Option<Handle<AnimationGraph>>,
    /// The doors' animations' nodes, by their number in the doors model.
    door_nodes: Vec<AnimationNodeIndex>,
}

/// The map's doors in play: its data, what the match has done to them, and their
/// parts.
#[derive(Resource, Default)]
pub struct DoorsState {
    data: DoorsData,
    live: Live,
    consoles: Vec<Parts>,
    doors: Vec<Parts>,
    files: Option<Files>,
    graphs: Option<Graphs>,
    /// The console each of this PC's players is at (for the currency notify), and
    /// whether his use key was down (by split screen's seat).
    touched: [Option<usize>; crate::split::SEATS],
    pressed_before: [bool; crate::split::SEATS],
}

impl DoorsState {
    pub fn new(data: DoorsData) -> Self {
        DoorsState {
            live: Live::new(&data),
            consoles: vec![Parts::default(); data.consoles.len()],
            doors: vec![Parts::default(); data.doors.len()],
            data,
            ..default()
        }
    }

    /// The consoles bought since this was last asked (their names: nav.rs opens
    /// the paths their Kismet opens).
    pub fn take_purchases(&mut self) -> Vec<String> {
        std::mem::take(&mut self.live.fresh)
            .into_iter()
            .filter_map(|i| self.data.consoles.get(i).map(|c| c.id.clone()))
            .collect()
    }

    /// Whether the spawn cluster `name` is on: the doors' Kismet switches some off as
    /// gameplay starts and on as their doors open (waves.rs brings no enemy in where
    /// one is off). One the doors don't name is on.
    pub fn cluster_on(&self, name: &str) -> bool {
        self.data
            .clusters
            .iter()
            .position(|(n, _)| n == name)
            .is_none_or(|i| self.live.clusters.get(i).copied().unwrap_or(true))
    }

    /// The map's doors data (its traps' actors and sentries among it).
    pub fn data(&self) -> &DoorsData {
        &self.data
    }

    /// The match's time on the doors' clock (seconds since gameplay started), and
    /// whether it has started.
    pub fn clock(&self) -> Option<f32> {
        self.live.started.then_some(self.live.clock)
    }

    /// Whether trap actor `i` is on.
    pub fn trap_on(&self, i: usize) -> bool {
        self.live.traps.get(i).copied().unwrap_or(false)
    }

    /// A trap light's brightness as its Matinee has it now (None: its own).
    pub fn light_brightness(&self, i: usize) -> Option<f32> {
        self.live.brightness.get(i).copied().flatten()
    }

    /// Whether the trap's sentry `i` is awake.
    pub fn sentry_awake(&self, i: usize) -> bool {
        self.live.awake.get(i).copied().unwrap_or(false)
    }
}

/// The other players' deposits and purchases (net_apply's).
#[derive(Resource, Default)]
pub struct DoorInbox(pub Vec<JazzEvent>);

/// The doors' systems (nav.rs follows their purchases after them).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DoorsSystems;

pub struct DoorsPlugin;

impl Plugin for DoorsPlugin {
    fn build(&self, app: &mut App) {
        // (The consoles' outlines glow in their live material: live.rs.)
        app.init_resource::<crate::live::LiveOutlines>();
        app.world_mut()
            .resource_mut::<crate::live::LiveOutlines>()
            .0
            .push(crate::live::Outline {
                prefix: OUTLINE_NODE,
                glow: LinearRgba::new(
                    OUTLINE_COLOR[0] * OUTLINE_INTENSITY,
                    OUTLINE_COLOR[1] * OUTLINE_INTENSITY,
                    OUTLINE_COLOR[2] * OUTLINE_INTENSITY,
                    1.0,
                ),
                wave: crate::live::Wave::Flare {
                    rate: OUTLINE_FLARE_RATE,
                    amount: OUTLINE_FLARE,
                },
            });
        app.init_resource::<DoorsState>()
            .init_resource::<DoorInbox>()
            .add_systems(OnEnter(crate::menu::AppState::Game), reset)
            // (Whatever the screen: the map is in play from the menus on.)
            .add_systems(Update, (prepare_graphs, door_scenes, attach_doors).chain())
            .add_systems(
                Update,
                (
                    use_consoles
                        .after(crate::weapon_pickups)
                        .after(crate::recreator::use_pods)
                        .before(crate::hud::HudSystems),
                    remote_doors.after(crate::net_apply),
                    run_doors
                        .after(use_consoles)
                        .after(remote_doors)
                        .in_set(DoorsSystems),
                    hold_door_loops
                        .after(run_doors)
                        .before(crate::sound::PlaySounds),
                    show_doors.after(run_doors).after(attach_doors),
                )
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

/// A new match: the map's doors and consoles as the level starts (each match is a
/// new one in the game).
fn reset(mut state: ResMut<DoorsState>, mut inbox: ResMut<DoorInbox>) {
    let state = &mut *state;
    state.live = Live::new(&state.data);
    state.touched = Default::default();
    inbox.0.clear();
}

/// The doors model's textures sampled as the map's are (main.rs).
fn map_sampler(settings: &mut GltfLoaderSettings) {
    settings.default_sampler = Some(crate::graphics::map_sampler());
}

/// The map's consoles and doors, as it comes in (main.rs): their models asked for
/// (each file once), their scenes to come (door_scenes).
pub fn spawn(commands: &mut Commands, asset_server: &AssetServer, data: &DoorsData) -> Vec<Entity> {
    let mut out = Vec::new();
    if !data.consoles.is_empty() {
        let file: Handle<Gltf> = asset_server.load(CONSOLE_MODEL);
        let floor: Option<Handle<Gltf>> = data
            .consoles
            .iter()
            .any(|c| c.floor)
            .then(|| asset_server.load(CONSOLE_FLOOR_MODEL));
        for (index, c) in data.consoles.iter().enumerate() {
            let file = match (&floor, c.floor) {
                (Some(f), true) => f.clone(),
                _ => file.clone(),
            };
            out.push(
                commands
                    .spawn((
                        Console { index },
                        Name::new(c.id.clone()),
                        SceneOf { file, scene: 0 },
                        c.at,
                        Visibility::default(),
                        crate::OwnAnimation,
                    ))
                    .id(),
            );
        }
    }
    let model = doors_model(&data.stem);
    if !data.doors.is_empty() && crate::assets_dir().join(&model).exists() {
        let file: Handle<Gltf> = asset_server.load_with_settings(model, map_sampler);
        for (index, d) in data.doors.iter().enumerate() {
            let at = if d.skeletal {
                Transform {
                    translation: d.at,
                    rotation: d.turn,
                    scale: d.scale,
                }
            } else {
                Transform::from_translation(d.at)
            };
            let mut e = commands.spawn((
                Door { index },
                Name::new(d.id.clone()),
                SceneOf {
                    file: file.clone(),
                    scene: d.scene,
                },
                at,
                Visibility::default(),
            ));
            if d.skeletal {
                e.insert(crate::OwnAnimation);
            }
            out.push(e.id());
        }
    }
    out
}

/// The animation graphs, once the models are in (or can't be): the console's three
/// clips (the floor console's its own), and every animation of the map's doors
/// model. The models are the files spawn asked for (asked for again here: the same
/// ones), kept while the map is in play.
fn prepare_graphs(
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut state: ResMut<DoorsState>,
) {
    let state = &mut *state;
    let d = &state.data;
    if state.graphs.is_some() || d.consoles.is_empty() && d.doors.is_empty() {
        return;
    }
    let files = state.files.get_or_insert_with(|| {
        let model = doors_model(&d.stem);
        Files {
            console: (!d.consoles.is_empty()).then(|| asset_server.load(CONSOLE_MODEL)),
            floor: d
                .consoles
                .iter()
                .any(|c| c.floor)
                .then(|| asset_server.load(CONSOLE_FLOOR_MODEL)),
            doors: (!d.doors.is_empty() && crate::assets_dir().join(&model).exists())
                .then(|| asset_server.load_with_settings(model, map_sampler)),
        }
    });
    let ready = |f: &Option<Handle<Gltf>>| {
        f.as_ref()
            .is_none_or(|f| gltfs.contains(f) || asset_server.load_state(f).is_failed())
    };
    if !(ready(&files.console) && ready(&files.floor) && ready(&files.doors)) {
        return;
    }
    // (A model that can't be read has no clips: its consoles stay as they stand.)
    let clips = |f: &Option<Handle<Gltf>>, n: usize| -> Vec<Handle<AnimationClip>> {
        f.as_ref()
            .and_then(|f| gltfs.get(f))
            .map(|g| g.animations.iter().take(n).cloned().collect())
            .unwrap_or_default()
    };
    let console_clips = clips(&files.console, 3);
    let (console, console_nodes) = AnimationGraph::from_clips(console_clips.iter().cloned());
    let floor_clips = clips(&files.floor, 3);
    let (floor, floor_nodes) = if floor_clips.is_empty() {
        (None, Vec::new())
    } else {
        let (g, nodes) = AnimationGraph::from_clips(floor_clips.iter().cloned());
        (Some(graphs.add(g)), nodes)
    };
    let door_clips = clips(&files.doors, d.anims.len());
    let (doors, door_nodes) = if door_clips.is_empty() {
        (None, Vec::new())
    } else {
        let (g, nodes) = AnimationGraph::from_clips(door_clips);
        (Some(graphs.add(g)), nodes)
    };
    state.graphs = Some(Graphs {
        console: graphs.add(console),
        console_nodes,
        console_clips,
        floor,
        floor_nodes,
        floor_clips,
        doors,
        door_nodes,
    });
}

/// The consoles' and doors' scenes from their models, once the graphs are made (so
/// that attach_doors has them for their animation players as they come in). One a
/// model hasn't got (or a model that can't be read) is left out.
fn door_scenes(
    mut commands: Commands,
    state: Res<DoorsState>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    pending: Query<(Entity, &SceneOf, &Name)>,
) {
    if state.graphs.is_none() {
        return;
    }
    for (e, of, name) in &pending {
        let scene = match gltfs.get(&of.file) {
            Some(g) => g.scenes.get(of.scene).cloned(),
            None if asset_server.load_state(&of.file).is_failed() => None,
            None => continue,
        };
        let mut e = commands.entity(e);
        e.remove::<SceneOf>();
        match scene {
            Some(scene) => {
                e.insert(SceneRoot(scene));
            }
            None => warn!(
                "{name}: its model (or its scene {}) can't be had: left out",
                of.scene
            ),
        }
    }
}

/// The consoles' and skeletal doors' parts as their scenes come in: their
/// animation players (given their graph) and the consoles' outlines (the meshes
/// drawn with its material, under its node).
#[allow(clippy::type_complexity)]
fn attach_doors(
    mut commands: Commands,
    mut state: ResMut<DoorsState>,
    added: Query<
        (Entity, Option<&Name>, Has<AnimationPlayer>, Has<Mesh3d>),
        Or<(Added<AnimationPlayer>, Added<Name>)>,
    >,
    parents: Query<&ChildOf>,
    consoles: Query<&Console>,
    doors: Query<&Door>,
) {
    let state = &mut *state;
    let Some(graphs) = state.graphs.as_ref() else {
        return;
    };
    for (e, name, player, drawn) in &added {
        let outline = drawn && name.is_some_and(|n| n.as_str().starts_with(OUTLINE_NODE));
        if !player && !outline {
            continue;
        }
        for a in parents.iter_ancestors(e) {
            if let Ok(c) = consoles.get(a) {
                let floor = state.data.consoles.get(c.index).is_some_and(|s| s.floor);
                let Some(parts) = state.consoles.get_mut(c.index) else {
                    break;
                };
                if player {
                    let graph = match (&graphs.floor, floor) {
                        (Some(g), true) => g.clone(),
                        _ => graphs.console.clone(),
                    };
                    commands.entity(e).insert(AnimationGraphHandle(graph));
                    parts.player = Some(e);
                }
                if outline && !parts.outline.contains(&e) {
                    // (Those of a scene instance since gone, gone.)
                    parts.outline.retain(|o| parents.contains(*o));
                    parts.outline.push(e);
                    commands
                        .entity(e)
                        .insert((bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
                }
                break;
            }
            if let Ok(d) = doors.get(a) {
                if player
                    && let Some(g) = &graphs.doors
                    && let Some(parts) = state.doors.get_mut(d.index)
                {
                    commands.entity(e).insert(AnimationGraphHandle(g.clone()));
                    parts.player = Some(e);
                }
                break;
            }
        }
    }
}

/// This PC's player at the consoles: the offer while one he can use is in reach
/// (unless something else is offered: a pickup, a pod), and a deposit as he presses
/// the key.
#[allow(clippy::type_complexity)]
fn use_consoles(
    mut state: ResMut<DoorsState>,
    mut score: crate::split::BySeat<Score>,
    mut sfx: ResMut<Sfx>,
    mut hud: crate::split::BySeat<HudState>,
    mut me: Query<
        (
            &Transform,
            &mut Player,
            &PlayerInput,
            Option<&crate::split::Seat>,
        ),
        (With<LocalPlayer>, Without<RemotePlayer>),
    >,
) {
    // (Each of this PC's players: split screen's guest with his own energon and HUD.)
    for (transform, player, input, seat) in &mut me {
        let seat = crate::split::seat_of(seat);
        console_for(
            &mut state,
            seat,
            (score.seat(seat), &mut sfx, hud.seat(seat)),
            (transform, player, input),
        );
    }
}

/// One of this PC's players at the consoles (use_consoles).
fn console_for(
    state: &mut DoorsState,
    seat: usize,
    (score, sfx, hud): (&mut Score, &mut Sfx, &mut HudState),
    (transform, mut player, input): (&Transform, Mut<Player>, &PlayerInput),
) {
    let pressed = input.pick_up && !state.pressed_before[seat];
    state.pressed_before[seat] = input.pick_up;
    let at = transform.translation;
    let forward = *transform.forward();
    // (Something else offered has the key.)
    let free = hud.prompt.is_none();
    let (d, live) = (&state.data, &state.live);
    let target = d
        .consoles
        .iter()
        .enumerate()
        .filter(|(i, c)| {
            let s = &live.consoles[*i];
            s.enabled && !s.bought && live.totals[c.total] < c.cost
        })
        .filter(|(_, c)| {
            let front = (c.spot - c.at.translation).normalize_or_zero();
            crate::recreator::can_use(&player, at, forward, c.spot, front)
        })
        .min_by(|a, b| {
            let flat = |c: &ConsoleSpot| Vec2::new(c.spot.x - at.x, c.spot.z - at.z).length();
            flat(a.1).total_cmp(&flat(b.1))
        })
        .map(|(i, _)| i)
        .filter(|_| free);
    let Some(index) = target else {
        state.touched[seat] = None;
        return;
    };
    let c = &d.consoles[index];
    let (deposit, cost) = (c.deposit, c.cost);
    hud.prompt = Some(Prompt::Deposit(c.prompt, deposit, cost));
    // (Touched: the counter shown, red when short.)
    if state.touched[seat] != Some(index) {
        state.touched[seat] = Some(index);
        hud.events.push(HudEvent::CurrencyNotify {
            low: score.energon < deposit,
        });
    }
    if !pressed {
        return;
    }
    if score.energon < deposit {
        sfx.play(crate::SND_NO_FUNDS);
        hud.events.push(HudEvent::CurrencyNotify { low: true });
        return;
    }
    score.energon -= deposit;
    player.played.push(JazzEvent::Deposit {
        console: index as u8,
    });
    let mut play = |cue: &'static str, at: Option<Vec3>| sfx.play_from(cue, at);
    for bought in deposit_into(&state.data, &mut state.live, &mut play, index) {
        player.played.push(JazzEvent::Unlock {
            console: bought as u8,
        });
    }
}

/// A deposit into console `index` (this PC's player's or another's), with its sound:
/// the consoles it buys (those of its total, still on, whose price the total now
/// reaches).
fn deposit_into(
    d: &DoorsData,
    s: &mut Live,
    play: &mut impl FnMut(&'static str, Option<Vec3>),
    index: usize,
) -> Vec<usize> {
    let Some(c) = d.consoles.get(index) else {
        return Vec::new();
    };
    // (A trap's console in its cooldown takes none: another PC's deposit come in
    // after its purchase.)
    if c.cooldown > 0.0 && s.consoles[index].bought {
        return Vec::new();
    }
    play(SND_DEPOSIT, Some(c.at.translation));
    let total = c.total;
    s.totals[total] = s.totals[total].saturating_add(c.deposit);
    let full: Vec<usize> = (0..d.consoles.len())
        .filter(|&i| {
            let (other, cs) = (&d.consoles[i], &s.consoles[i]);
            other.total == total && cs.enabled && !cs.bought && s.totals[total] >= other.cost
        })
        .collect();
    for &i in &full {
        buy(d, s, play, i);
    }
    full
}

/// Console `index` bought (once, or a trap's once a cooldown): its full sound, it
/// turns itself off (FireDisableWhenFull: its outline with it), and its timeline
/// runs (FireSuccessWhenFull). A trap's clears its total and comes back on its
/// cooldown later (ReenableAfterCooldown). (Consoles sharing a total are bought
/// together: a timeline the same as one started this moment runs once.)
fn buy(
    d: &DoorsData,
    s: &mut Live,
    play: &mut impl FnMut(&'static str, Option<Vec3>),
    index: usize,
) {
    let Some(c) = d.consoles.get(index) else {
        return;
    };
    let cs = &mut s.consoles[index];
    if std::mem::replace(&mut cs.bought, true) {
        return;
    }
    play(SND_FULL, Some(c.at.translation));
    if cs.enabled {
        cs.enabled = false;
        cs.outline = false;
        cs.playing = Some((UNEQUIP, 0.0));
    }
    if c.cooldown > 0.0 {
        cs.reenable_at = Some(s.clock + c.cooldown);
        s.totals[c.total] = 0;
    } else {
        s.totals[c.total] = s.totals[c.total].max(c.cost);
    }
    if let Some(tl) = c.timeline {
        let clock = s.clock;
        let same = s.running.iter().any(|&(other, began, _)| {
            began == clock && other != tl && d.timelines[other] == d.timelines[tl]
        });
        if !same {
            s.running.push((tl, clock, 0));
        }
    }
    s.fresh.push(index);
}

/// The other players' deposits and purchases.
fn remote_doors(mut state: ResMut<DoorsState>, mut sfx: ResMut<Sfx>, mut inbox: ResMut<DoorInbox>) {
    let state = &mut *state;
    let mut play = |cue: &'static str, at: Option<Vec3>| sfx.play_from(cue, at);
    for event in std::mem::take(&mut inbox.0) {
        match event {
            JazzEvent::Deposit { console } => {
                deposit_into(&state.data, &mut state.live, &mut play, console as usize);
            }
            JazzEvent::Unlock { console } => {
                buy(&state.data, &mut state.live, &mut play, console as usize);
            }
            _ => {}
        }
    }
}

/// Where a sound plays.
fn place_at(d: &DoorsData, s: &Live, place: Place) -> Vec3 {
    match place {
        Place::Door(i) => d.doors[i].at + s.doors[i].offset,
        Place::At(p) | Place::Line(p, _) => p,
    }
}

/// Where a sound at `place` is heard from by a listener at `ear`: a line's point
/// nearest it.
fn heard_at(d: &DoorsData, s: &Live, place: Place, ear: Option<Vec3>) -> Vec3 {
    match (place, ear) {
        (Place::Line(mid, half), Some(ear)) => {
            let len2 = half.length_squared();
            if len2 < 1e-8 {
                return mid;
            }
            let k = ((ear - mid).dot(half) / len2).clamp(-1.0, 1.0);
            mid + half * k
        }
        _ => place_at(d, s, place),
    }
}

/// A UE3 rotator given as roll, pitch and yaw in degrees (a Matinee's rotation
/// track's key, Euler's X, Y and Z) as a turn in Bevy's axes (Unreal's y and z
/// swapped: a mirror, so the turn goes the other way about the swapped axis).
pub(crate) fn ue_turn(euler: Vec3) -> Quat {
    let r = crate::ai::ue::ROT_PER_DEG;
    let q = crate::ai::ue::quat_from_rotator(Vec3::new(euler.y * r, euler.z * r, euler.x * r));
    Quat::from_xyzw(-q.x, -q.z, -q.y, q.w).normalize()
}

/// A sound at its places (without one, where the player is).
fn play_at_places(
    d: &DoorsData,
    s: &Live,
    play: &mut impl FnMut(&'static str, Option<Vec3>),
    cue: &'static str,
    at: &[Place],
) {
    if at.is_empty() {
        play(cue, None);
    }
    for &p in at {
        play(cue, Some(place_at(d, s, p)));
    }
}

/// One step of a timeline.
fn run_op(d: &DoorsData, s: &mut Live, play: &mut impl FnMut(&'static str, Option<Vec3>), op: &Op) {
    match op {
        Op::Play(m) => {
            let length = d.matinees[*m].length;
            let ms = &mut s.matinees[*m];
            // (One at its end starts again from the start.)
            if ms.pos >= length - 1e-4 {
                ms.pos = 0.0;
            }
            ms.playing = true;
            ms.reverse = false;
            ms.paused = false;
            ms.touched = true;
        }
        Op::Reverse(m) => {
            let ms = &mut s.matinees[*m];
            ms.playing = true;
            ms.reverse = true;
            ms.paused = false;
            ms.touched = true;
        }
        // (Pause pauses one playing, or resumes it: SeqAct_Interp's Pause.)
        Op::Pause(m) => {
            let ms = &mut s.matinees[*m];
            if ms.playing {
                ms.paused = !ms.paused;
            }
        }
        Op::Resume(m) => s.matinees[*m].paused = false,
        Op::Stop(m) => {
            let ms = &mut s.matinees[*m];
            ms.playing = false;
            ms.paused = false;
        }
        Op::Switch(targets, switch) => {
            for t in targets {
                match *t {
                    Target::Console(c) => {
                        // (TnInteractBase.OnToggle: its Enabled or Disabled event,
                        // EnableIt's or DisableIt's clip, and its outline with it.)
                        let cs = &mut s.consoles[c];
                        let on = switch.apply(cs.enabled);
                        if on != cs.enabled {
                            cs.enabled = on;
                            cs.outline = on;
                            cs.playing = Some((if on { EQUIP } else { UNEQUIP }, 0.0));
                        }
                    }
                    Target::Volume(v) => s.volumes[v] = switch.apply(s.volumes[v]),
                    Target::Cluster(c) => s.clusters[c] = switch.apply(s.clusters[c]),
                    Target::Trap(i) => s.traps[i] = switch.apply(s.traps[i]),
                }
            }
        }
        Op::Outline(consoles, on) => {
            for &c in consoles {
                s.consoles[c].outline = *on;
            }
        }
        Op::Hide(doors, hide) => {
            for &i in doors {
                s.doors[i].hidden = *hide;
            }
        }
        Op::Collapse(doors) => {
            for &i in doors {
                if !std::mem::replace(&mut s.doors[i].down, true) {
                    let foot = d.doors[i].collapse.unwrap_or(d.doors[i].at);
                    play(SND_COLLAPSE, Some(foot));
                    s.fallen.push(i);
                }
            }
        }
        Op::Transform(doors, forward) => {
            for &i in doors {
                if s.doors[i].pose.is_some() {
                    s.doors[i].transforming = Some(*forward);
                }
            }
        }
        Op::Sound(cue, at) => {
            if d.looping.contains(cue) {
                // (A looping cue plays on till its Stop: held, hold_door_loops.)
                let places: Vec<Option<Place>> = if at.is_empty() {
                    vec![None]
                } else {
                    at.iter().map(|&p| Some(p)).collect()
                };
                for p in places {
                    let place = p.unwrap_or(Place::At(Vec3::NAN));
                    if !s
                        .loops
                        .iter()
                        .any(|(_, c, q)| c == cue && same_place(*q, place))
                    {
                        s.next_loop += 1;
                        s.loops.push((s.next_loop, cue, place));
                    }
                }
            } else {
                play_at_places(d, s, play, cue, at);
            }
        }
        Op::SoundStop(cue, at) => {
            s.loops.retain(|(_, c, q)| {
                c != cue || !(at.is_empty() || at.iter().any(|&p| same_place(*q, p)))
            });
        }
        Op::Wake(sentries, awake) => {
            for &i in sentries {
                if let Some(a) = s.awake.get_mut(i) {
                    *a = *awake;
                }
            }
        }
    }
}

/// Whether two places are the same (a place-less sound's, NaN, among them).
fn same_place(a: Place, b: Place) -> bool {
    match (a, b) {
        (Place::At(p), Place::At(q)) if p.is_nan() && q.is_nan() => true,
        _ => a == b,
    }
}

/// The doors' clock run on by `dt`: the timelines' steps as their times come, the
/// Matinees played on (their doors moved and posed, their sounds), the transforms,
/// the consoles' bars and clips.
fn advance(
    d: &DoorsData,
    s: &mut Live,
    play: &mut impl FnMut(&'static str, Option<Vec3>),
    dt: f32,
) {
    s.clock += dt;
    let clock = s.clock;
    // The consoles bought as their totals reach their prices (CheckIfFullSequence
    // waits for it: one whose total another console's deposit, or purchase, fills).
    for i in 0..d.consoles.len() {
        let (c, cs) = (&d.consoles[i], &s.consoles[i]);
        if cs.enabled && !cs.bought && s.totals[c.total] >= c.cost {
            buy(d, s, play, i);
        }
    }
    // The timelines.
    let mut k = 0;
    while k < s.running.len() {
        let (tl, began, mut next) = s.running[k];
        let steps = &d.timelines[tl];
        while let Some((t, op)) = steps.get(next) {
            if clock - began < *t - 1e-5 {
                break;
            }
            run_op(d, s, play, op);
            next += 1;
        }
        if next >= steps.len() {
            s.running.remove(k);
        } else {
            s.running[k].2 = next;
            k += 1;
        }
    }
    // The traps' consoles back on as their cooldowns end (ReenableAfterCooldown's
    // FireEnableEvent: EnableIt, its Equip_Console and its outline, whatever their
    // Kismet did to them meanwhile: one flag, the last word on it).
    for cs in s.consoles.iter_mut() {
        if cs.reenable_at.is_some_and(|t| clock >= t - 1e-5) {
            cs.reenable_at = None;
            cs.bought = false;
            cs.outline = true;
            if !cs.enabled {
                cs.enabled = true;
                cs.playing = Some((EQUIP, 0.0));
            }
        }
    }

    // The Matinees.
    for (m, md) in d.matinees.iter().enumerate() {
        let ms = &mut s.matinees[m];
        if !ms.touched {
            continue;
        }
        let before = ms.pos;
        let mut wrapped = false;
        if ms.playing && !ms.paused {
            if ms.reverse {
                ms.pos -= dt * md.rate;
                if ms.pos <= 0.0 {
                    ms.pos = 0.0;
                    ms.playing = false;
                }
            } else {
                ms.pos += dt * md.rate;
                if ms.pos >= md.length {
                    if md.looping && md.length > 0.0 {
                        ms.pos -= md.length;
                        wrapped = true;
                    } else {
                        ms.pos = md.length;
                        ms.playing = false;
                    }
                }
            }
        }
        let (pos, reverse) = (ms.pos, ms.reverse);
        // Its sounds and toggle keys, as they are passed going forward.
        if !reverse && (pos > before || wrapped) {
            let ended = pos >= md.length;
            let passed = |t: f32| {
                if wrapped {
                    t >= before || t < pos
                } else {
                    t >= before && (t < pos || (ended && t <= pos))
                }
            };
            for (t, cue, at) in &md.sounds {
                if passed(*t) {
                    play_at_places(d, s, play, cue, at);
                }
            }
            for (t, switch, who) in &md.toggles {
                if passed(*t) {
                    for &i in who {
                        s.traps[i] = switch.apply(s.traps[i]);
                    }
                }
            }
        }
        // Its moves (the riders with their base), sizes and poses.
        for (door, keys) in &md.moves {
            let offset = eval_keys(keys, pos);
            s.doors[*door].offset = offset;
            for &r in &d.doors[*door].riders {
                s.doors[r].offset = offset;
            }
        }
        for (v, keys) in &md.volume_moves {
            s.volume_offsets[*v] = eval_keys(keys, pos);
        }
        for (door, keys) in &md.scales {
            s.doors[*door].scale = Some(eval_keys(keys, pos).x);
        }
        // (IMF_RelativeToInitial: the keys' turn after the actor's own, base · key.)
        for (door, base, keys) in &md.turns {
            s.doors[*door].turn = Some(*base * ue_turn(eval_keys(keys, pos)) * base.inverse());
        }
        for (light, keys) in &md.brightness {
            s.brightness[*light] = Some(eval_keys(keys, pos).x);
        }
        for (door, keys) in &md.anims {
            let Some(k) = keys.iter().rev().find(|k| k.start <= pos) else {
                continue;
            };
            let (t, phase) = anim_time(k, d.anim_secs(k.anim), pos - k.start);
            let ds = &mut s.doors[*door];
            ds.pose = Some((k.anim, t));
            ds.anim_phase = Some(phase);
        }
    }

    // The transforms (the Optimus doors), and their notifies' sounds.
    for (i, ds) in s.doors.iter_mut().enumerate() {
        let (Some(forward), Some((anim, t))) = (ds.transforming, ds.pose) else {
            continue;
        };
        let length = d.anim_secs(anim);
        let next = if forward {
            (t + dt).min(length)
        } else {
            (t - dt).max(0.0)
        };
        if forward {
            let name = d.anims.get(anim).map_or("", |a| a.0.as_str());
            for (clip, at, cue) in NOTIFIES {
                if clip == name && t <= at && at < next {
                    play(cue, Some(d.doors[i].at + ds.offset));
                }
            }
        }
        ds.pose = Some((anim, next));
        if (forward && next >= length) || (!forward && next <= 0.0) {
            ds.transforming = None;
        }
    }

    // The consoles' bars and clips.
    for (c, cs) in s.consoles.iter_mut().enumerate() {
        let spot = &d.consoles[c];
        let target = (s.totals[spot.total] as f32 / spot.cost.max(1) as f32).clamp(0.0, 1.0);
        let step = BAR_SPEED * dt;
        cs.bar = if cs.bar < target {
            (cs.bar + step).min(target)
        } else {
            (cs.bar - step).max(target)
        };
        if let Some((clip, t)) = cs.playing.as_mut() {
            *t += dt;
            // (Equip_Console played through, the bar shows again.)
            if *clip == EQUIP && *t >= CONSOLE_CLIP_SECS {
                cs.playing = None;
            }
        }
    }
}

/// Where a door is in its opening now.
fn door_phase(d: &DoorsData, s: &Live, i: usize) -> Phase {
    let (spot, ds) = (&d.doors[i], &s.doors[i]);
    let mut moving = false;
    let mut ended = false;
    let mut note = |p: Phase| {
        moving |= p == Phase::Moving;
        ended |= p == Phase::End;
    };
    if spot.moved {
        note(if ds.offset.length() < AT_REST {
            Phase::Start
        } else if spot.end != Vec3::ZERO && ds.offset.distance(spot.end) < AT_REST {
            Phase::End
        } else {
            Phase::Moving
        });
    }
    if spot.scaled {
        note(match ds.scale {
            Some(f) if f <= 0.05 => Phase::End,
            Some(f) if f < 0.9 => Phase::Moving,
            _ => Phase::Start,
        });
    }
    if let Some(p) = ds.anim_phase {
        note(p);
    } else if spot.skeletal
        && let (Some((a0, t0)), Some((a, t))) = (spot.pose, ds.pose)
    {
        note(if a == a0 && (t - t0).abs() < 1e-3 {
            Phase::Start
        } else {
            Phase::End
        });
    }
    if moving {
        Phase::Moving
    } else if ended {
        Phase::End
    } else {
        Phase::Start
    }
}

/// A door's collision as it stands now: closed on, open on (an Optimus door's
/// switches as its opening ends).
fn door_collision(d: &DoorsData, s: &Live, i: usize) -> (bool, bool) {
    let ds = &s.doors[i];
    if ds.down {
        return (false, false);
    }
    if ds.transforming.is_some() {
        return ds.shown.unwrap_or((true, false));
    }
    door_phase(d, s, i).collision()
}

/// The doors' clock: gameplay start's first steps at once as the match begins, then
/// on with the game's; and the map's collision as the doors stand.
fn run_doors(
    time: Res<Time>,
    mut state: ResMut<DoorsState>,
    mut sfx: ResMut<Sfx>,
    mut level: ResMut<crate::Level>,
    bank: Option<Res<crate::sound::SoundBank>>,
    (mut fx, mut gibs): (
        Option<ResMut<crate::levelfx::FxSystems>>,
        Option<ResMut<crate::gibs::Gibs>>,
    ),
) {
    let state = &mut *state;
    if state.data.consoles.is_empty() && state.data.doors.is_empty() {
        return;
    }
    // (The timelines' looping cues, once the sound bank is known: held until stopped.)
    if !state.data.looping_known
        && let Some(bank) = bank.as_deref()
    {
        let d = &mut state.data;
        d.looping = d
            .timelines
            .iter()
            .flatten()
            .filter_map(|(_, op)| match op {
                Op::Sound(cue, _) => Some(*cue),
                _ => None,
            })
            .filter(|cue| bank.loops(cue))
            .collect();
        d.looping_known = true;
    }
    let (d, s) = (&state.data, &mut state.live);
    let mut play = |cue: &'static str, at: Option<Vec3>| sfx.play_from(cue, at);
    if !s.started {
        s.started = true;
        if let Some(start) = d.start {
            s.running.push((start, 0.0, 0));
        }
        advance(d, s, &mut play, 0.0);
        while s.clock < START_AT_ONCE && !s.running.is_empty() {
            advance(d, s, &mut play, START_STEP);
        }
    } else {
        advance(d, s, &mut play, time.delta_secs().min(MAX_STEP));
    }
    // The props brought down crumble: their broken states set off where they stood
    // (the beam's pieces thrown, its falling bits; the crash is played above, at
    // the foot).
    for i in std::mem::take(&mut s.fallen) {
        if let (Some(c), Some(gibs)) = (&d.doors[i].crumble, gibs.as_deref_mut()) {
            crate::gibs::set_off(
                &c.state,
                &c.place(),
                &mut s.seed,
                false,
                &mut fx,
                &mut sfx,
                gibs,
            );
        }
    }
    // The collision as the doors stand (switched only where it changes).
    let Some(collision) = level.collision.as_ref() else {
        return;
    };
    let mut changes: Vec<(u16, bool)> = Vec::new();
    let mut want = |group: Option<u16>, on: bool| {
        if let Some(g) = group
            && collision.group_on(g) != on
        {
            changes.push((g, on));
        }
    };
    let mut base_closed = vec![None; d.volumes.len()];
    for (i, door) in d.doors.iter().enumerate() {
        let (closed, open) = door_collision(d, s, i);
        s.doors[i].shown = Some((closed, open));
        want(door.closed, closed);
        want(door.open, open);
        for &v in &door.rider_volumes {
            base_closed[v] = Some(closed);
        }
    }
    for (v, (_, g)) in d.volumes.iter().enumerate() {
        // (A volume riding on a door: on while the door is where it starts; one a
        // Matinee moves, while it is where it starts.)
        let here = s.volume_offsets[v].length() < AT_REST;
        want(*g, s.volumes[v] && base_closed[v].unwrap_or(true) && here);
    }
    if changes.is_empty() {
        return;
    }
    if let Some(collision) = level.collision.as_mut() {
        for (g, on) in changes {
            collision.set_group(g, on);
        }
    }
}

/// The looping sounds the timelines started and haven't stopped (the flame jets'
/// roar, the gunship's engines), held each frame where they play: a line's point
/// nearest the camera, as the game's line emitters are heard.
fn hold_door_loops(
    state: Res<DoorsState>,
    mut sfx: ResMut<Sfx>,
    listener: Query<&GlobalTransform, (With<Camera3d>, With<crate::OrbitCamera>)>,
) {
    let (d, s) = (&state.data, &state.live);
    if s.loops.is_empty() {
        return;
    }
    // (With two players on one screen, the point nearest either camera.)
    let ears: Vec<Vec3> = listener.iter().map(|t| t.translation()).collect();
    for &(id, cue, place) in &s.loops {
        let at = match place {
            Place::At(p) if p.is_nan() => None,
            _ => Some(
                ears.iter()
                    .map(|&ear| {
                        let at = heard_at(d, s, place, Some(ear));
                        (at.distance_squared(ear), at)
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map_or_else(|| heard_at(d, s, place, None), |(_, at)| at),
            ),
        };
        sfx.hold(LOOP_OWNER | id, cue, at);
    }
}

/// The consoles and doors as they stand: the doors moved, hidden and posed, the
/// consoles' clips (the bar, or Equip_Console and UnEquip_Console), their outlines
/// shown or not (flaring in their live material: live.rs).
#[allow(clippy::type_complexity)]
fn show_doors(
    state: Res<DoorsState>,
    clips: Res<Assets<AnimationClip>>,
    mut places: Query<(&Door, &mut Transform, &mut Visibility)>,
    mut players: Query<&mut AnimationPlayer>,
    mut outlines: Query<&mut Visibility, (Without<Door>, Without<Console>)>,
) {
    let Some(graphs) = state.graphs.as_ref() else {
        return;
    };
    let (d, s) = (&state.data, &state.live);
    for (door, mut t, mut v) in &mut places {
        let (Some(spot), Some(ds)) = (d.doors.get(door.index), s.doors.get(door.index)) else {
            continue;
        };
        let at = spot.at + ds.offset;
        if t.translation != at {
            t.translation = at;
        }
        let turn = ds.turn.map_or(spot.turn, |q| q * spot.turn);
        if t.rotation != turn {
            t.rotation = turn;
        }
        let scale = spot.scale * ds.scale.unwrap_or(1.0);
        if t.scale != scale {
            t.scale = scale;
        }
        let want = if ds.hidden || ds.down {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *v != want {
            *v = want;
        }
    }
    for (i, parts) in state.doors.iter().enumerate() {
        if let (Some(p), Some((anim, t))) = (parts.player, s.doors[i].pose)
            && let Ok(mut player) = players.get_mut(p)
            && let Some(&node) = graphs.door_nodes.get(anim)
        {
            hold(&mut player, node, t, d.anim_secs(anim));
        }
    }
    // (A floor console's clips are its own model's.)
    let secs = |floor: bool, clip: usize| {
        let list = if floor {
            &graphs.floor_clips
        } else {
            &graphs.console_clips
        };
        list.get(clip)
            .and_then(|h| clips.get(h))
            .map_or(CONSOLE_CLIP_SECS, |c| c.duration())
    };
    for (c, parts) in state.consoles.iter().enumerate() {
        let cs = &s.consoles[c];
        let floor = d.consoles[c].floor;
        if let Some(p) = parts.player
            && let Ok(mut player) = players.get_mut(p)
        {
            let (clip, t) = match cs.playing {
                Some((clip, t)) => (clip, t),
                // (The bar: Activate_Console at its share.)
                None if cs.enabled => (ACTIVATE, cs.bar * secs(floor, ACTIVATE)),
                None => (UNEQUIP, secs(floor, UNEQUIP)),
            };
            let nodes = if floor {
                &graphs.floor_nodes
            } else {
                &graphs.console_nodes
            };
            if let Some(&node) = nodes.get(clip) {
                hold(&mut player, node, t, secs(floor, clip));
            }
        }
        // (The outline as last set: by the console's turning on or off, or its
        // Kismet's.)
        let want = if cs.outline {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        for &o in &parts.outline {
            if let Ok(mut v) = outlines.get_mut(o)
                && *v != want
            {
                *v = want;
            }
        }
    }
}

/// An animation player held `t` seconds into `node`'s clip (of `length` seconds),
/// and playing nothing else.
pub(crate) fn hold(player: &mut AnimationPlayer, node: AnimationNodeIndex, t: f32, length: f32) {
    // (A clip held at its very end would wrap round to its start.)
    let t = t.clamp(0.0, (length - 1e-3).max(0.0));
    if !player.is_playing_animation(node) || player.playing_animations().count() != 1 {
        player.stop_all();
        player.play(node);
    }
    if let Some(a) = player.animation_mut(node) {
        if a.speed() != 0.0 {
            a.set_speed(0.0);
        }
        if (a.seek_time() - t).abs() > 1e-4 {
            a.seek_to(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(t: f32, x: f32, mode: u8) -> Key {
        Key {
            t,
            v: Vec3::new(x, 0.0, 0.0),
            mode,
            arrive: Vec3::ZERO,
            leave: Vec3::ZERO,
        }
    }

    #[test]
    fn curves_are_ue3s() {
        // Two keys with flat tangents: a smoothstep.
        let keys = [key(0.0, 0.0, b'A'), key(1.0, 20.0, b'A')];
        assert_eq!(eval_keys(&keys, -1.0).x, 0.0);
        assert!((eval_keys(&keys, 0.5).x - 10.0).abs() < 1e-4);
        let s = 0.25f32;
        assert!((eval_keys(&keys, s).x - 20.0 * (3.0 * s * s - 2.0 * s * s * s)).abs() < 1e-4);
        assert_eq!(eval_keys(&keys, 2.0).x, 20.0);
        let lin = [key(0.0, 0.0, b'L'), key(2.0, 4.0, b'L')];
        assert!((eval_keys(&lin, 0.5).x - 1.0).abs() < 1e-5);
        let con = [key(0.0, 3.0, b'C'), key(2.0, 4.0, b'C')];
        assert_eq!(eval_keys(&con, 1.9).x, 3.0);
        let k = parse_key("0.5 1 2 3 L 0 0 0 0 0 0").unwrap();
        assert_eq!((k.t, k.v, k.mode), (0.5, Vec3::new(1.0, 2.0, 3.0), b'L'));
    }

    #[test]
    fn anim_tracks_play_reversed() {
        let k = AnimKey {
            start: 0.0,
            anim: 0,
            start_offset: 0.0,
            end_offset: 0.0,
            rate: 1.0,
            looping: false,
            reverse: true,
        };
        let (t, phase) = anim_time(&k, 5.0, 0.01);
        assert!((t - 4.99).abs() < 1e-5);
        assert_eq!(phase, Phase::Start);
        assert_eq!(anim_time(&k, 5.0, 2.0).1, Phase::Moving);
        assert_eq!(anim_time(&k, 5.0, 9.0), (0.0, Phase::End));
    }

    fn loaded(stem: &str) -> Option<(DoorsData, LevelCollision)> {
        let dir = crate::assets_dir();
        if !dir.join(format!("levels/{stem}_doors.txt")).exists() {
            eprintln!("{stem}'s doors not built: skipped");
            return None;
        }
        Some(DoorsData::load(
            &dir,
            stem,
            LevelCollision::from_parts(&[], &[], 4.0),
        ))
    }

    /// Gameplay start, its first steps at once (as run_doors runs it).
    fn started(d: &DoorsData) -> Live {
        let mut s = Live::new(d);
        let mut play = |_: &'static str, _: Option<Vec3>| {};
        s.started = true;
        s.running.push((d.start.unwrap(), 0.0, 0));
        advance(d, &mut s, &mut play, 0.0);
        while s.clock < START_AT_ONCE && !s.running.is_empty() {
            advance(d, &mut s, &mut play, START_STEP);
        }
        s
    }

    #[test]
    fn reads_the_doors() {
        for stem in ["downfall", "oblivion", "ancients", "ignition"] {
            let Some((d, c)) = loaded(stem) else {
                continue;
            };
            assert!(!d.consoles.is_empty(), "{stem}");
            for con in &d.consoles {
                assert!(
                    con.cost > 0 && con.cost % con.deposit == 0,
                    "{stem} {}",
                    con.id
                );
                // (A trap's line may leave out its price: the Flame Jets' "Press E
                // to $250 activate Flame Jets".)
                assert!(
                    if con.floor {
                        con.prompt.contains("`i")
                    } else {
                        con.prompt.contains("deposit") && con.prompt.contains("`c")
                    },
                    "{stem} {}",
                    con.id
                );
                assert_eq!(con.floor, con.cooldown > 0.0, "{stem} {}", con.id);
                // Every console's purchase does something (Ignition's second
                // AirStrike console's through the other's, their total shared).
                assert!(
                    d.consoles.iter().any(|other| other.total == con.total
                        && other.timeline.is_some_and(|t| !d.timelines[t].is_empty())),
                    "{stem} {}",
                    con.id
                );
            }
            assert!(d.start.is_some(), "{stem}");
            assert!(c.triangle_count() > 0, "{stem}");
            // Some spawn points start behind the doors.
            assert!(!d.locked_spawn_points().is_empty(), "{stem}");
            // The skeletal doors have their poses, in their clips.
            for door in d.doors.iter().filter(|d| d.skeletal) {
                let (anim, t) = door.pose.unwrap_or_else(|| panic!("{stem} {}", door.id));
                assert!(t <= d.anim_secs(anim) + 1e-4, "{stem} {}", door.id);
            }
        }
    }

    /// As gameplay starts, the waves' spawn clusters behind the doors are off (by
    /// the names the waves' data has them under), and the rest on.
    #[test]
    fn closed_doors_keep_the_waves_out() {
        for stem in ["downfall", "oblivion", "ancients", "ignition"] {
            let Some((d, _)) = loaded(stem) else {
                continue;
            };
            let path = crate::assets_dir().join(format!("levels/{stem}_waves.txt"));
            let waves = crate::waves::WaveData::parse(&std::fs::read_to_string(path).unwrap());
            let mut state = DoorsState::new(d);
            state.live = started(&state.data);
            let off: Vec<&str> = waves
                .clusters
                .iter()
                .filter(|c| !state.cluster_on(&c.name))
                .map(|c| c.name.as_str())
                .collect();
            assert!(
                !off.is_empty() && off.len() < waves.clusters.len(),
                "{stem}"
            );
            for (i, (name, _)) in state.data.clusters.iter().enumerate() {
                assert!(
                    waves.clusters.iter().any(|c| &c.name == name),
                    "{stem} {name}"
                );
                assert_eq!(
                    state.live.clusters[i],
                    !off.contains(&name.as_str()),
                    "{stem} {name}"
                );
            }
        }
    }

    /// Every door console's purchase, from the level's start: it turns itself off
    /// and opens something (a door gets to where it ends up, or a blocker goes),
    /// and everything comes to rest.
    #[test]
    fn purchases_open_doors() {
        for stem in ["downfall", "oblivion", "ancients", "ignition"] {
            let Some((d, _)) = loaded(stem) else {
                continue;
            };
            for (index, con) in d.consoles.iter().enumerate().filter(|(_, c)| !c.floor) {
                let mut s = started(&d);
                let mut sounds = 0;
                let mut play = |_: &'static str, _: Option<Vec3>| sounds += 1;
                let before: Vec<Phase> =
                    (0..d.doors.len()).map(|i| door_phase(&d, &s, i)).collect();
                let volumes_before = s.volumes.clone();
                let mut deposits = 1;
                while !deposit_into(&d, &mut s, &mut play, index).contains(&index) {
                    deposits += 1;
                    assert!(deposits < 100, "{stem} {}", con.id);
                }
                assert_eq!(deposits * con.deposit, con.cost, "{stem} {}", con.id);
                for _ in 0..60 * 12 {
                    advance(&d, &mut s, &mut play, 1.0 / 60.0);
                }
                assert!(!s.consoles[index].enabled, "{stem} {} stays on", con.id);
                let opened = (0..d.doors.len())
                    .any(|i| before[i] != Phase::End && door_phase(&d, &s, i) == Phase::End);
                let unblocked = s.volumes.iter().zip(&volumes_before).any(|(a, b)| !a && *b);
                assert!(opened || unblocked, "{stem} {} opens nothing", con.id);
                assert!(sounds as u32 >= con.cost / con.deposit, "{stem} {}", con.id);
                assert!(s.running.is_empty(), "{stem} {}", con.id);
                assert!(
                    s.doors.iter().all(|ds| ds.transforming.is_none()),
                    "{stem} {}",
                    con.id
                );
                assert!(
                    (0..d.doors.len()).all(|i| door_phase(&d, &s, i) != Phase::Moving),
                    "{stem} {}",
                    con.id
                );
            }
        }
    }

    /// Downfall's scaffolds stand across the ways the Courtyard opens until it is
    /// bought (an enemy's body, 1.5 m round and 4.4 m tall, can't stand in one, on
    /// the cells its navigation has there, blocked till then); its purchase brings
    /// down those its Kismet names, with their collision (the body stands there
    /// now, on cells open now), and leaves the others standing.
    /// Every scaffold a purchase brings down crumbles as the game's do
    /// (DES_ASH_ESCBeam1_DSYS's broken state): twelve pieces of the beam (two of its
    /// first part, two of its second, three of its third, five of its fourth),
    /// thrown from up its height round where the actor stands (its top, 16.5 m over
    /// its foot), with its falling bits; and each piece's mesh is there to throw.
    #[test]
    fn the_scaffolds_crumble_as_they_come_down() {
        use crate::deaths::Piece;
        let Some(map) = crate::MAPS.iter().find(|m| m.stem == "downfall") else {
            return;
        };
        let Some(level) = crate::load_level_data(map) else {
            eprintln!("Downfall's level not built: skipped");
            return;
        };
        let d = &level.doors;
        let fallen: Vec<&DoorSpot> = d.doors.iter().filter(|x| x.collapse.is_some()).collect();
        if fallen.is_empty() {
            eprintln!("Downfall's scaffolds not built as doors: skipped");
            return;
        }
        assert_eq!(fallen.len(), 19);
        let mut fx = crate::levelfx::FxSystems::default();
        for door in fallen {
            let c = door
                .crumble
                .as_ref()
                .unwrap_or_else(|| panic!("{} doesn't crumble", door.id));
            let foot = door.collapse.unwrap();
            assert!(
                c.at.xz().distance(foot.xz()) < 2.0 && (c.at.y - foot.y - 16.5).abs() < 0.3,
                "{}: at {} over its foot {}",
                door.id,
                c.at,
                foot
            );
            let mut chunks = Vec::new();
            let mut bits = false;
            for piece in &c.state.pieces {
                match piece {
                    Piece::Gibs(g) => {
                        assert_eq!(g.meshes.len(), 1);
                        chunks.push((g.meshes[0].to_string(), g.count));
                        // (Thrown fast: 10 to 80 m a second, all round.)
                        assert!(g.speed.0 >= 1000.0 && g.speed.1 <= 8000.0, "{:?}", g.speed);
                        assert_eq!(g.cone, 360.0);
                    }
                    Piece::Particle { system, .. } => bits |= &**system == "Catwalk_Debris_FX",
                    _ => {}
                }
            }
            chunks.sort();
            assert_eq!(
                chunks,
                [
                    ("Beam1_pt1_STAT".to_string(), 2),
                    ("Beam1_pt2_STAT".to_string(), 2),
                    ("Beam1_pt3_STAT".to_string(), 3),
                    ("Beam1_pt4_STAT".to_string(), 5),
                ]
            );
            assert!(bits, "{}: no falling bits", door.id);
        }
        // The pieces' meshes: bars of the beam, the second part's 13.4 m long.
        for k in 1..=4 {
            let (lo, hi) = fx
                .mesh_bounds(&format!("Beam1_pt{k}_STAT_FX"))
                .unwrap_or_else(|| panic!("no Beam1_pt{k}_STAT_FX"));
            let size = hi - lo;
            assert!(
                size.min_element() > 0.0 && size.max_element() < 14.0,
                "{k}: {size}"
            );
        }
        let (lo, hi) = fx.mesh_bounds("Beam1_pt2_STAT_FX").unwrap();
        assert!(((hi - lo).x - 13.45).abs() < 0.1, "{}", hi - lo);
    }

    #[test]
    fn the_courtyard_brings_its_scaffolds_down() {
        use crate::collision::Mover;
        let Some(map) = crate::MAPS.iter().find(|m| m.stem == "downfall") else {
            return;
        };
        let dir = crate::assets_dir();
        let (Some(mut level), Some(mut nav)) = (
            crate::load_level_data(map),
            crate::nav::NavMesh::load(&dir, "downfall"),
        ) else {
            eprintln!("Downfall's level or navigation not built: skipped");
            return;
        };
        let d = level.doors.clone();
        let find = |id: &str| d.doors.iter().position(|x| x.id == id);
        let (Some(fallen), Some(other)) = (
            find("TnStaticDestructibleActor_16091"),
            find("TnStaticDestructibleActor_6752"),
        ) else {
            eprintln!("Downfall's scaffolds not built as doors: skipped");
            return;
        };
        let foot = d.doors[fallen].collapse.expect("where it comes down");
        // (Heavy ammo station 5's scaffold: its foot, 6 m across, on the cells there,
        // every 0.5 m; all but its first 2.5 m from the ledge, where a body touches
        // the cover blocks along the ledge's edge.)
        let mut spots = Vec::new();
        for i in 0..=12 {
            for k in 0..=7 {
                let (x, z) = (foot.x - 3.0 + i as f32 * 0.5, foot.z - 0.5 + k as f32 * 0.5);
                if let Some(cell) = nav.cell_at(Vec3::new(x, foot.y, z)) {
                    let y = nav.height_at(cell, x, z);
                    if (y - foot.y).abs() < 1.5 {
                        spots.push((Vec3::new(x, y, z), cell));
                    }
                }
            }
        }
        let stopped = |level: &crate::Level| {
            spots
                .iter()
                .filter(|(feet, _)| {
                    level
                        .push_out_for(*feet, 1.5, 4.4, Mover::Ai)
                        .is_some_and(|v| v.length() > 0.05)
                })
                .count()
        };
        let blocked = |nav: &crate::nav::NavMesh| {
            spots
                .iter()
                .filter(|(_, c)| nav.points[*c as usize].blocked)
                .count()
        };
        let (before, cells_before) = (stopped(&level), blocked(&nav));
        eprintln!(
            "{} places on its cells: {before} stop the body, {cells_before} on blocked cells",
            spots.len()
        );
        assert!(
            spots.len() >= 70 && before * 2 >= spots.len(),
            "the scaffold lets the body through"
        );
        assert_eq!(cells_before, spots.len(), "its cells are open");
        let mut s = started(&d);
        let mut crashes = Vec::new();
        let mut play = |cue: &'static str, at: Option<Vec3>| {
            if cue == SND_COLLAPSE {
                crashes.push(at);
            }
        };
        let console = "TnInteractCurrencyReceptacle_3800";
        let index = d.consoles.iter().position(|c| c.id == console).unwrap();
        buy(&d, &mut s, &mut play, index);
        for _ in 0..60 {
            advance(&d, &mut s, &mut play, 1.0 / 60.0);
        }
        assert!(s.doors[fallen].down && !s.doors[other].down);
        assert_eq!(door_collision(&d, &s, fallen), (false, false));
        assert_eq!(door_collision(&d, &s, other), (true, false));
        // (Each one crashing down once, at its foot.)
        let down = s.doors.iter().filter(|ds| ds.down).count();
        assert_eq!(down, 15);
        assert_eq!(crashes.len(), down);
        assert!(crashes.contains(&Some(foot)));
        // The collision as run_doors leaves it, and the navigation as the purchase's
        // toggles leave it.
        let collision = level.collision.as_mut().unwrap();
        for i in 0..d.doors.len() {
            let (closed, open) = door_collision(&d, &s, i);
            if let Some(g) = d.doors[i].closed {
                collision.set_group(g, closed);
            }
            if let Some(g) = d.doors[i].open {
                collision.set_group(g, open);
            }
        }
        for t in nav.bought(console) {
            nav.switch(&t);
        }
        let (after, cells_after) = (stopped(&level), blocked(&nav));
        eprintln!("now {after} stop the body, {cells_after} on blocked cells");
        assert_eq!(after, 0, "the scaffold still stands in the way");
        assert_eq!(cells_after, 0, "its cells are still blocked");
    }

    /// Whichever of Downfall's consoles is bought, no way its navigation opens runs
    /// under a scaffold left standing: the middle of each one's foot (3 m across)
    /// is on no cell open then.
    #[test]
    fn no_way_opens_under_a_standing_scaffold() {
        let dir = crate::assets_dir();
        let (Some((d, _)), Some(start_nav)) = (
            loaded("downfall"),
            crate::nav::NavMesh::load(&dir, "downfall"),
        ) else {
            return;
        };
        let scaffolds: Vec<usize> = (0..d.doors.len())
            .filter(|&i| d.doors[i].collapse.is_some())
            .collect();
        if scaffolds.is_empty() {
            eprintln!("Downfall's scaffolds not built as doors: skipped");
            return;
        }
        let under = |nav: &crate::nav::NavMesh, i: usize| {
            let foot = d.doors[i].collapse.unwrap();
            let mut open = 0;
            for a in 0..=6 {
                for b in 0..=6 {
                    let p = foot + Vec3::new(a as f32 * 0.5 - 1.5, 0.0, b as f32 * 0.5 - 1.5);
                    if let Some(c) = nav.cell_at(p)
                        && (nav.height_at(c, p.x, p.z) - foot.y).abs() < 1.5
                        && !nav.points[c as usize].blocked
                    {
                        open += 1;
                    }
                }
            }
            open
        };
        for &i in &scaffolds {
            assert_eq!(under(&start_nav, i), 0, "{} at the start", d.doors[i].id);
        }
        for (index, con) in d.consoles.iter().enumerate().filter(|(_, c)| !c.floor) {
            let mut s = started(&d);
            let mut play = |_: &'static str, _: Option<Vec3>| {};
            buy(&d, &mut s, &mut play, index);
            for _ in 0..60 {
                advance(&d, &mut s, &mut play, 1.0 / 60.0);
            }
            let mut nav = start_nav.clone();
            for t in nav.bought(&con.id) {
                nav.switch(&t);
            }
            for &i in &scaffolds {
                if !s.doors[i].down {
                    assert_eq!(
                        under(&nav, i),
                        0,
                        "{} with {} bought",
                        d.doors[i].id,
                        con.id
                    );
                }
            }
        }
    }

    /// A Matinee's rotation key as a turn in Bevy's axes (Unreal's y and z swapped).
    #[test]
    fn rotation_keys_turn_as_ue3s() {
        let close = |a: Vec3, b: Vec3| (a - b).length() < 1e-4;
        assert!(ue_turn(Vec3::ZERO).angle_between(Quat::IDENTITY) < 1e-4);
        // Yaw: Unreal's x onto its y, Bevy's x onto its z (a turn the other way
        // about Bevy's up).
        let yaw = ue_turn(Vec3::new(0.0, 0.0, 90.0));
        assert!(yaw.angle_between(Quat::from_rotation_y(-90f32.to_radians())) < 1e-4);
        assert!(close(yaw * Vec3::X, Vec3::Z));
        // Pitch: the nose up, towards Unreal's z (Bevy's y).
        let (s, c) = 30f32.to_radians().sin_cos();
        assert!(close(
            ue_turn(Vec3::new(0.0, 30.0, 0.0)) * Vec3::X,
            Vec3::new(c, s, 0.0)
        ));
        // Roll: Unreal's y (Bevy's z) down, towards its -z (Bevy's -y).
        assert!(close(
            ue_turn(Vec3::new(30.0, 0.0, 0.0)) * Vec3::Z,
            Vec3::new(0.0, -s, c)
        ));
    }

    /// The traps' consoles, bought again and again: a purchase clears the total,
    /// turns the console off and runs its Kismet (an effect, a pain volume or a
    /// light switched on, sentries woken, the flame jets' roar held), and it all
    /// ends: the pain and the effects off, the sentries asleep, the roar stopped,
    /// and the console back on its cooldown later.
    #[test]
    fn trap_consoles_come_back() {
        let mut traps = 0;
        for stem in ["downfall", "oblivion", "ignition"] {
            let Some((mut d, _)) = loaded(stem) else {
                continue;
            };
            // (Their looping cues, as the sound bank would have them.)
            d.looping = d
                .timelines
                .iter()
                .flatten()
                .filter_map(|(_, op)| match op {
                    Op::SoundStop(cue, _) => Some(*cue),
                    _ => None,
                })
                .collect();
            d.looping_known = true;
            // (Every trap's flames are lit, as the game lights them.)
            assert!(
                d.traps.iter().all(|t| match t.thing {
                    TrapThing::Emitter { lit, .. } => lit,
                    _ => true,
                }),
                "{stem}: a trap's emitter unlit"
            );
            let mut play = |_: &'static str, _: Option<Vec3>| {};
            for (index, con) in d.consoles.iter().enumerate().filter(|(_, c)| c.floor) {
                traps += 1;
                let mut s = started(&d);
                // (Past gameplay start's steps: the sentries put to sleep.)
                for _ in 0..180 {
                    advance(&d, &mut s, &mut play, 1.0 / 60.0);
                }
                assert!(s.awake.iter().all(|a| !a), "{stem}: sentries awake");
                assert!(
                    d.traps.iter().zip(&s.traps).all(|(t, on)| !on
                        || !matches!(t.thing, TrapThing::Pain(_) | TrapThing::Emitter { .. })),
                    "{stem}: a trap on as the match starts"
                );
                let before = (s.traps.clone(), s.awake.clone());
                let mut deposits = 1;
                while !deposit_into(&d, &mut s, &mut play, index).contains(&index) {
                    deposits += 1;
                    assert!(deposits < 100, "{stem} {}", con.id);
                }
                assert_eq!(deposits * con.deposit, con.cost, "{stem} {}", con.id);
                assert_eq!(s.totals[con.total], 0, "{stem} {}: not cleared", con.id);
                let cs = &s.consoles[index];
                assert!(!cs.enabled && !cs.outline && cs.bought, "{stem} {}", con.id);
                // (Its total's other consoles bought with it, their Kismet run once
                // when it is the same: Oblivion's two.)
                let shared: Vec<usize> = (0..d.consoles.len())
                    .filter(|&i| d.consoles[i].total == con.total)
                    .collect();
                assert!(
                    shared.iter().all(|&i| s.consoles[i].bought),
                    "{stem} {}",
                    con.id
                );
                let running = |s: &Live| s.running.iter().filter(|r| Some(r.0) != d.start).count();
                assert_eq!(running(&s), 1, "{stem} {}", con.id);
                // (The flame jets' roar and the gunship's engines are held till their
                // Kismet stops them.)
                let holds = con.timeline.is_some_and(|tl| {
                    d.timelines[tl]
                        .iter()
                        .any(|(_, op)| matches!(op, Op::SoundStop(..)))
                });
                // A deposit while it cools down goes nowhere.
                assert!(deposit_into(&d, &mut s, &mut play, index).is_empty());
                assert_eq!(s.totals[con.total], 0, "{stem} {}", con.id);
                let (mut switched, mut woke, mut held) = (false, false, false);
                let steps = ((con.cooldown + 0.5) * 60.0) as usize;
                for step in 1..=steps {
                    advance(&d, &mut s, &mut play, 1.0 / 60.0);
                    switched |= s.traps.iter().zip(&before.0).any(|(a, b)| *a && !*b);
                    woke |= s.awake.iter().zip(&before.1).any(|(a, b)| *a && !*b);
                    held |= !s.loops.is_empty();
                    if (step as f32) / 60.0 < con.cooldown - 0.1 {
                        assert!(!s.consoles[index].enabled, "{stem} {} back early", con.id);
                    }
                }
                assert!(switched || woke, "{stem} {} does nothing", con.id);
                assert_eq!(held, holds, "{stem} {}", con.id);
                let cs = &s.consoles[index];
                assert!(
                    cs.enabled && cs.outline && !cs.bought,
                    "{stem} {} stays off",
                    con.id
                );
                assert_eq!(running(&s), 0, "{stem} {}", con.id);
                assert!(s.loops.is_empty(), "{stem} {}: a sound held on", con.id);
                assert!(
                    s.awake.iter().all(|a| !a),
                    "{stem} {}: sentries awake",
                    con.id
                );
                for (t, on) in d.traps.iter().zip(&s.traps) {
                    if matches!(t.thing, TrapThing::Pain(_) | TrapThing::Emitter { .. }) {
                        assert!(!on, "{stem} {}: {} left on", con.id, t.id);
                    }
                }
            }
        }
        if traps > 0 {
            // (Ignition's four Flame Jets and its GunShip, Oblivion's two, Downfall's
            // three.)
            assert_eq!(traps, 10);
        }
    }

    /// The gunship's Matinee turns it as it flies (its rotation keys after its own
    /// turn), and its sentries ride its bone.
    #[test]
    fn the_gunship_circles() {
        let Some((d, _)) = loaded("ignition") else {
            return;
        };
        let Some((m, (door, base, keys))) = d
            .matinees
            .iter()
            .enumerate()
            .find_map(|(m, md)| md.turns.first().map(|t| (m, t)))
        else {
            return;
        };
        assert!(d.doors[*door].skeletal);
        let riders: Vec<&SentrySpot> = d
            .sentries
            .iter()
            .filter(|s| s.rides.as_ref().is_some_and(|r| r.0 == *door))
            .collect();
        assert_eq!(riders.len(), 3);
        assert!(riders.iter().all(|s| s.kind == SentryKind::Mg));
        // It faces the way it flies (its nose its -x as the mesh has it: the turn
        // and the move's direction agree within a quarter turn), half way through.
        let md = &d.matinees[m];
        let (_, moves) = md.moves.iter().find(|(i, _)| i == door).unwrap();
        let t = md.length * 0.5;
        let way = (eval_keys(moves, t + 0.5) - eval_keys(moves, t - 0.5)).normalize();
        let turn = *base * ue_turn(eval_keys(keys, t)) * base.inverse() * d.doors[*door].turn;
        let nose = turn * Vec3::X;
        assert!(nose.dot(way).abs() > 0.7, "nose {nose:?} way {way:?}");
        // It circles the arena (its path from where it starts, its first key's turn
        // taken out: doors_kismet.py), 60 to 140 m up, its guns' 160 m from its
        // console all the way round, and right over it (the statue) once, as the
        // game's video has it.
        let console = d
            .consoles
            .iter()
            .find(|c| c.id.ends_with("_15651"))
            .unwrap()
            .at
            .translation;
        let flat = |v: Vec3| Vec2::new(v.x, v.z);
        let mut nearest = f32::MAX;
        for i in 0..=55 {
            let t = 10.0 + i as f32 * 0.5;
            let at = d.doors[*door].at + eval_keys(moves, t);
            let away = flat(at).distance(flat(console));
            nearest = nearest.min(away);
            assert!(away < 160.0, "{t}: {at:?}");
            assert!((60.0..140.0).contains(&(at.y - console.y)), "{t}: {at:?}");
        }
        assert!(nearest < 30.0, "{nearest}");
    }

    /// As gameplay starts, the covers are up, their collision where they stand.
    #[test]
    fn gameplay_start_holds_the_covers() {
        for stem in ["downfall", "ancients"] {
            let Some((d, _)) = loaded(stem) else {
                continue;
            };
            let s = started(&d);
            let covers: Vec<usize> = (0..d.doors.len())
                .filter(|&i| s.doors[i].anim_phase.is_some())
                .collect();
            assert!(!covers.is_empty(), "{stem}");
            for i in covers {
                assert_eq!(
                    door_collision(&d, &s, i),
                    (true, false),
                    "{stem} {}",
                    d.doors[i].id
                );
            }
            assert!(s.matinees.iter().any(|m| m.touched && m.paused), "{stem}");
        }
    }
}
