//! Escalation's waves, as the game runs them: its survival actor's goals and their
//! data store (the map packages' TransContent: TnSurvivalGoalTimePressure,
//! TnSurvivalGoalWaveSpawning, TnSurvivalDataStoreWaveSpawning, and the wave spawn
//! action TnSurvivalActionWaveSpawn with its cooldown), and TransGame's spawn point
//! manager (TnSpawnPointManager, scoring the clusters by the players' spawn
//! modifier rules), from each map's levels/<map>_waves.txt (tools/build_waves.py:
//! its spawn list, TR_Survival30_m.SurvivalSpawnList_<Map>Tuned_p, and its spawn
//! clusters and points):
//!
//! * **The countdown**: 10 s after a player is first in the match
//!   (StartGameDownTime), the countdown to the first wave, 30 s (DownTime), on the
//!   HUD's clock at the top right (hud.rs: the single-team game's scoreboard, the
//!   GRI's CurrentCountdown), ticking each second from 10 down (its
//!   LowCountdownTickSound). As it passes each whole second under 7
//!   (CountdownMessageTypes) the announcer (Optimus Prime, Megatron on the Decepticon
//!   maps) says "Next wave begins in..." with "Wave 1 Begins In..." typed under the
//!   announcements, then "Five" to "One" (TnSurvivalTimeMessage), the last with the
//!   wave's stinger (WaveBeginMusic).
//! * **A wave** (StartWave): one of its spawn groups at random (each wave has one),
//!   its budgets (what its enemies cost: LivingBudget for those in play at once,
//!   TotalBudget for all it sends), and every player's wave buffs: the damage he does
//!   (TnBuffSurvivalWavePlayerDamageDone: 1 down to 0.5 by wave 11) and takes
//!   (TnBuffSurvivalWavePlayerDamageTaken: 1 up to 1.8 by wave 9), on top of the
//!   difficulty's (TnBuffSurvivalDifficultyPlayerDamageDone and DamageTaken: on Easy
//!   the players do 1.5 times the damage and take 0.7 times, on Hard 0.6 and 1.5
//!   times). The HUD shows "WAVE 1
//!   OF 15" (mc_currentWaveMessage) from then.
//! * **Its enemies** come in one at a time, 1 to 3 s apart at random (the spawn
//!   action's cooldown), while the living budget has room, the total budget isn't
//!   spent and fewer than the map's most are in play (GlobalMaxAICount: 12 on Downfall,
//!   14 on the others): a type of the group's
//!   by its weight (GetAISpawnIndex: none at its most at once, nor one whose kind
//!   died less than its cooldown ago), at a point of the spawn cluster its spawn
//!   request has open (up to 4 tries for a point with room: MaxSpawnPointsToConsider).
//!   The spawn point manager keeps a cluster open for each request, scoring every
//!   cluster that serves it ten times a second (UpdateInterval): for each player up,
//!   100 if it is out of his sight and -100 if in it (the Swarmers' requests take
//!   either: 100), and 1000 over his distance in Unreal units; it switches to the best
//!   when that beats the open one by 5 (EarlyCutoffScoreDifference), or once the open
//!   one has been open 10 s (ActiveClusterTime). The clusters behind the locked doors
//!   are shut till the doors open (doors.rs).
//! * **Its end**: with the total budget spent and 3 or fewer left
//!   (AiLeftToTriggerEnemyView), they show on every player's HUD
//!   (TnBuffSeeEnemyObjectiveMarkers: their markers, through walls, the buff's scan
//!   heard as it starts); the announcer says "We are 3 kills from victory!" ("Three
//!   Enemies Remaining") as 3 are left and "We are 1 kill from victory!" ("One
//!   Enemy Remaining") as 1 is. With none left, "Wave complete" ("Wave 1 Complete",
//!   with WaveCompleteMusic): the round ends (RoundEnded: the dead come back and the
//!   downed get up, revive.rs), each player is paid the wave's reward (200 for waves
//!   1 to 5, 400 to 10, 600 after) and the countdown to the next starts.
//! * **A failed wave**: nobody up for 3 s and a life goes (revive.rs); the wave's
//!   enemies are gone at once (ForceEndWave), "Wave 1 Failed, Restarting Wave" with
//!   WaveFailMusic, 500 to each player (MoneyLostWaveRewardAmount), and the same wave
//!   again after the countdown.
//! * **The end**: the last life lost ends the game; the 15th wave won ends it 5 s
//!   later (WinEndGameDelay), the wave's line going. The final scores show either way
//!   (scoreboard.rs: "You survived until wave `r", the wave reached).
//!
//! The enemies' AI moves them once they are in (src/ai). Those the waves bring in by
//! air (at the air spawn points high over the map) fly themselves from there: a jet
//! to its fight (ai/jet.rs), a Sniper to a perch (the level's TnSniperPoints:
//! ai/sniper.rs); any other flies down to the nearest spawn point on the ground.
//! Ignition's Leapers come in at spawn points far outside its walls, whose
//! SpawnAnimationName ("Spawn") leaps them in: they leap to the nearest spawn point
//! on the ground. The other spawn animations aren't played. And as the enemies
//! don't move off their spawn points, a spawn the open cluster has no room for tries
//! the other clusters of its request, best first.
//!
//! The demo's own: an enemy no player up has seen for a while (30 s once the
//! wave's last few show on the HUD, a minute before), one stranded somewhere the
//! players can't get at it (behind a locked door's force field, in a scaffold), is
//! taken out of play, not killed, and its cost given back for another to come in
//! where the players are: a wave can always be won.
//!
//! Online, the host runs the waves and its rosters carry them (net.rs's WaveSnap:
//! the countdown, the messages, the markers, the rewards and the enemies in play,
//! which every PC brings in as the host does); each PC plays them out for its own
//! player. An enemy that dies at a PC dies on the others too
//! (JazzEvent::EnemyDeath, with how it died); one gone from the host's roster dies
//! (or is gone, ended by force) a moment later.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bevy::prelude::*;

use crate::enemies::{self, Death, Enemy, Foe, Form, Hit, Kind, Kinds};
use crate::hud::{HudEvent, HudState};
use crate::net::{EnemySnap, JazzEvent, Net, WaveSnap};
use crate::revive::Team;
use crate::sound::Sfx;
use crate::{CurrentMap, Level, LocalPlayer, Player, Score};

// ---------------------------------------------------------------------------
// The game's numbers
// ---------------------------------------------------------------------------

/// The countdown's tick (TnGameReplicationInfo's LowCountdownTickSound, each second
/// from LowCountdownTickThreshold down to 1).
const SND_TICK: &str = "BL_LVL_MP.MP_COUNTDOWN_BEEP";
const TICK_FROM: i16 = 10;
/// The scan as the last enemies show (TnBuffSeeEnemyObjectiveMarkers' ApplySound,
/// ABILITY_ORBITAL_BEACON_SCAN).
const SND_MARKERS: &str = "BL_TRANS_POWER.ORBITAL_BEACON_ONLINE_SCAN";

/// TnSurvivalTimeMessage's types (ESurvivalTimeMessageType): the last countdown
/// one (STMT_InOne), a wave won, lost, and one and three enemies left.
const MSG_ONE: u8 = 6;
const MSG_COMPLETE: u8 = 7;
const MSG_FAILED: u8 = 8;
const MSG_ONE_LEFT: u8 = 9;
const MSG_THREE_LEFT: u8 = 10;
/// TnSurvivalGoalTimePressure's CountdownMessageTypes: the message as the time left
/// passes each whole second under 7, by the seconds left (rounded down): One, Two,
/// Three, Four, Five, none, "Wave `n Begins In".
const COUNTDOWN_MESSAGES: [u8; 7] = [6, 5, 4, 3, 2, 0, 1];
/// TnSurvivalTimeMessage's DisplayMessages (TransContent.int), the wave as `n.
const TEXTS: [&str; 11] = [
    "",
    "Wave `n Begins In...",
    "Five...",
    "Four...",
    "Three...",
    "Two...",
    "One...",
    "Wave `n Complete",
    "Wave `n Failed, Restarting Wave",
    "One Enemy Remaining",
    "Three Enemies Remaining",
];
/// Its DialogMessages, as the announcer's sound event set plays them: Optimus
/// Prime's lines (MP_ESC_WAVE_COUNTDOWN_START, _5 to _1, MP_ESC_WAVE_COMPLETE,
/// MP_ESC_1_KILL_FROM_VICTORY, MP_ESC_3_KILLS_FROM_VICTORY), and Megatron's on the
/// Decepticon maps (tools/build_sounds.py writes them under their own names).
const VOICES: [Option<&str>; 11] = [
    None,
    Some("BL_DX_SWITCHBOARD.SC017783"),
    Some("BL_DX_SWITCHBOARD.SC017785"),
    Some("BL_DX_SWITCHBOARD.SC017787"),
    Some("BL_DX_SWITCHBOARD.SC017789"),
    Some("BL_DX_SWITCHBOARD.SC017791"),
    Some("BL_DX_SWITCHBOARD.SC017793"),
    Some("BL_DX_SWITCHBOARD.SC017781"),
    None,
    Some("BL_DX_SWITCHBOARD.SC019363"),
    Some("BL_DX_SWITCHBOARD.SC019361"),
];
const VOICES_DECEPTICONS: [Option<&str>; 11] = [
    None,
    Some("BL_DX_SWITCHBOARD.SC017783_MGTRON"),
    Some("BL_DX_SWITCHBOARD.SC017785_MGTRON"),
    Some("BL_DX_SWITCHBOARD.SC017787_MGTRON"),
    Some("BL_DX_SWITCHBOARD.SC017789_MGTRON"),
    Some("BL_DX_SWITCHBOARD.SC017791_MGTRON"),
    Some("BL_DX_SWITCHBOARD.SC017793_MGTRON"),
    Some("BL_DX_SWITCHBOARD.SC017781_MGTRON"),
    None,
    Some("BL_DX_SWITCHBOARD.SC019363_MGTRON"),
    Some("BL_DX_SWITCHBOARD.SC019361_MGTRON"),
];
/// Its WaveBeginMusic (with "One"), WaveCompleteMusic and WaveFailMusic.
const SND_WAVE_BEGIN: &str = "BL_LVL_MP.STNG_ESCALATION_WAVE_BEGIN";
const SND_WAVE_WON: &str = "BL_LVL_MP.STNG_ESCALATION_WAVE_OVER_SUCCESS";
const SND_WAVE_FAILED: &str = "BL_LVL_MP.STNG_ESCALATION_WAVE_OVER_FAIL";

/// TnSpawnPointManager's UpdateInterval, EarlyCutoffScoreDifference,
/// ActiveClusterTime and MaxSpawnPointsToConsider.
const CLUSTER_UPDATE_SECS: f32 = 0.1;
const EARLY_CUTOFF: f32 = 5.0;
const CLUSTER_SECS: f32 = 10.0;
const SPAWN_TRIES: usize = 4;
/// The demo's: the clusters' sight lines are traced again one cluster a frame (the
/// others' kept from before: tracing every cluster from every player ten times a
/// second costs more than a frame can spare), a full round about every second.
const TRACES_PER_FRAME: usize = 1;
/// The players' spawn modifier rules (TnPlayerPawnSurvival's SpawnModifierComponent0):
/// LOSRule's Factor (its score -1 or 1), and SpawnNearPlayers' 1000 over the distance
/// in Unreal units (its Factor 1), a metre being 100 of them.
const SIGHT_FACTOR: f32 = 100.0;
const NEAR_FACTOR: f32 = 1000.0;
const UNITS: f32 = 100.0;
/// The spawn requests' sight rules (ESpawnRequestLOSRequirement): out of the players'
/// sight, in it, either.
const SIGHT_OUT: u8 = 0;
const SIGHT_IN: u8 = 1;
const SIGHT_EITHER: u8 = 2;
/// A player's eyes, as a share of his height (the view point the sight rule traces
/// to).
const EYES: f32 = 0.9;

/// The demo's: how fast those flying in without an AI that flies them come down,
/// and how near another's place a place counts as taken.
const FLY_IN_SPEED: f32 = 20.0;
const TAKEN_REACH: f32 = 3.0;
/// A leap in (the demo's, for the spawn points whose enemies leap into the map, their
/// SpawnAnimationName LEAP_ANIM: Ignition's Leapers', far outside its walls): over
/// to the nearest spawn point on the ground at LEAP_SPEED, rising a fifth of the
/// way across (LEAP_ARC).
const LEAP_ANIM: &str = "Spawn";
/// How near a spawn point (across, and above or below: its floor) an enemy that
/// came in at it is (WaveData::spawn_anim_at).
const SPAWN_ANIM_REACH: f32 = 0.25;
const SPAWN_ANIM_DROP: f32 = 8.0;
const LEAP_SPEED: f32 = 40.0;
const LEAP_ARC: f32 = 0.2;
/// An enemy gone from the host's roster dies this long after (its death, with how it
/// died, may come first: JazzEvent::EnemyDeath).
const DOOM_SECS: f32 = 0.3;
/// The demo's: how often the director looks for enemies stranded out of the
/// players' reach, and how long one goes unseen by every player up before it is
/// taken out and another comes in (with the wave's last few on the HUD, and before).
const STRAND_CHECK: f32 = 0.5;
const STRANDED_SECS: (f32, f32) = (30.0, 60.0);
/// The waves the HUD's line counts to (mc_currentWaveMessage's totalWaves).
pub const WAVES_SHOWN: u32 = 15;

/// The damage each player takes is scaled by (the difficulty's and the wave's
/// DamageTaken buffs), as f32 bits: main.rs's damage_player and revive.rs's drain
/// take it.
static TAKEN: AtomicU32 = AtomicU32::new(0x3f80_0000);

/// What the damage this PC's player takes is scaled by now.
pub fn taken() -> f32 {
    f32::from_bits(TAKEN.load(Ordering::Relaxed))
}

/// What the damage the players do is scaled by (the difficulty's and the wave's
/// DamageDone buffs), as f32 bits: the enemies take it on as they come in (the
/// waves' and the Titans' drones, ai/drones.rs).
static DONE: AtomicU32 = AtomicU32::new(0x3f80_0000);

/// What the damage the players do is scaled by now.
pub fn done() -> f32 {
    f32::from_bits(DONE.load(Ordering::Relaxed))
}

// ---------------------------------------------------------------------------
// A map's waves
// ---------------------------------------------------------------------------

/// A map's waves file.
fn waves_file(stem: &str) -> String {
    format!("levels/{stem}_waves.txt")
}

/// The game's numbers for a map's waves (its settings line).
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// StartGameDownTime and DownTime: the wait before the first wave's countdown,
    /// and each countdown (seconds).
    pub first_wait: f32,
    pub wait: f32,
    /// AiLeftToTriggerEnemyView: this few left (the wave's all in), and they show.
    pub show_at: usize,
    /// GlobalMaxAICount: the most enemies in play at once.
    pub most: usize,
    /// WinEndGameDelay: the game ends this long after the last wave is won.
    pub won_after: f32,
    /// The spawn action's cooldown (TnSurvivalWeightConditionCooldown's
    /// CooldownRange), seconds.
    pub apart: (f32, f32),
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            first_wait: 10.0,
            wait: 30.0,
            show_at: 3,
            most: 20,
            won_after: 5.0,
            apart: (1.0, 3.0),
        }
    }
}

/// A wave: its budgets, its rewards (each player's money, won and lost), its
/// players' buffs (the damage they do and take) and its spawn groups.
#[derive(Clone, Debug, PartialEq)]
pub struct WaveDef {
    pub living: u32,
    pub total: u32,
    pub money: u32,
    pub money_lost: u32,
    pub done: f32,
    pub taken: f32,
    pub groups: Vec<u32>,
}

/// A spawn group's AI type: the enemy (enemies.rs's kind, if the demo has it), what
/// it costs of a budget, its weight, the most at once (0: any), its cooldown after
/// one dies, and its spawn request (the clusters it comes in at, and the sight rule).
#[derive(Clone, Debug, PartialEq)]
pub struct AiType {
    pub name: String,
    pub kind: Option<usize>,
    pub cost: u32,
    pub weight: f32,
    pub most: usize,
    pub cooldown: f32,
    pub request: u32,
    pub sight: u8,
}

/// A spawn cluster (TnSpawnClusterSurvival): the request it serves, its middle
/// (what is scored), its name (the doors' Kismet switches it by) and its points.
#[derive(Clone, Debug, PartialEq)]
pub struct Cluster {
    pub request: u32,
    pub centre: Vec3,
    pub name: String,
    pub points: Vec<SpawnPoint>,
}

/// A spawn point: where, the way it faces, whether it is in the air
/// (TnSurvivalAirSpawnPoint), whether its enemies leap in from it (see
/// LEAP_ANIM), and the animation they come in with if any other (its
/// SpawnAnimationName: WaveData::spawn_anims' by number).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnPoint {
    pub at: Vec3,
    pub heading: f32,
    pub air: bool,
    pub leap: bool,
    pub anim: Option<u8>,
}

/// The map in play's waves (none for a map without its waves file).
#[derive(Resource, Default)]
pub struct WaveData {
    /// The map they are for (its stem), once loaded.
    loaded: Option<Option<&'static str>>,
    pub settings: Settings,
    pub waves: Vec<WaveDef>,
    /// The spawn groups: each one's id and AI types.
    pub groups: Vec<(u32, Vec<AiType>)>,
    pub clusters: Vec<Cluster>,
    /// The snipers' perches (TnSniperPoint): where, and the way they face.
    pub perches: Vec<(Vec3, f32)>,
    /// The spawn points' animations (SpawnPoint::anim's).
    pub spawn_anims: Vec<Arc<str>>,
}

impl WaveData {
    /// The animation an enemy that came in at `at` comes in with: that of the spawn
    /// point on the ground it came in at (on the floor under it: try_cluster), if
    /// it has one.
    pub fn spawn_anim_at(&self, at: Vec3) -> Option<Arc<str>> {
        self.clusters
            .iter()
            .flat_map(|c| c.points.iter())
            .filter(|p| !p.air && !p.leap)
            .find(|p| flat(p.at, at) < SPAWN_ANIM_REACH && (p.at.y - at.y).abs() < SPAWN_ANIM_DROP)
            .and_then(|p| p.anim)
            .and_then(|i| self.spawn_anims.get(i as usize).cloned())
    }

    /// A waves file (tools/build_waves.py's format; its kinds unresolved).
    pub fn parse(text: &str) -> WaveData {
        let mut d = WaveData::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let n = |i: usize| {
                f.get(i)
                    .and_then(|s| s.trim().parse::<f32>().ok())
                    .unwrap_or(0.0)
            };
            let u = |i: usize| n(i).max(0.0) as u32;
            let v3 = |i: usize| Vec3::new(n(i), n(i + 1), n(i + 2));
            match f[0] {
                "settings" if f.len() >= 12 => {
                    d.settings = Settings {
                        first_wait: n(1),
                        wait: n(2),
                        show_at: u(3) as usize,
                        most: u(6) as usize,
                        won_after: n(9),
                        apart: (n(10), n(11).max(n(10))),
                    }
                }
                "wave" if f.len() >= 9 => d.waves.push(WaveDef {
                    living: u(2),
                    total: u(3),
                    money: u(4),
                    money_lost: u(5),
                    done: n(6),
                    taken: n(7),
                    groups: f[8]
                        .split(',')
                        .filter_map(|g| g.trim().parse().ok())
                        .collect(),
                }),
                "type" if f.len() >= 9 => {
                    let t = AiType {
                        name: f[2].to_string(),
                        kind: None,
                        cost: u(3),
                        weight: n(4),
                        most: u(5) as usize,
                        cooldown: n(6),
                        request: u(7),
                        sight: u(8) as u8,
                    };
                    let group = u(1);
                    match d.groups.iter_mut().find(|(id, _)| *id == group) {
                        Some((_, types)) => types.push(t),
                        None => d.groups.push((group, vec![t])),
                    }
                }
                "cluster" if f.len() >= 6 => d.clusters.push(Cluster {
                    request: u(2),
                    centre: v3(3),
                    name: f.get(6).map_or("", |s| s.trim()).to_string(),
                    points: Vec::new(),
                }),
                kind @ ("point" | "air") if f.len() >= 6 => {
                    let anim = f
                        .get(6)
                        .map(|a| a.trim())
                        .filter(|a| !a.is_empty() && *a != "-");
                    let leap = anim == Some(LEAP_ANIM);
                    let anim = anim.filter(|_| !leap).and_then(|a| {
                        let i = match d.spawn_anims.iter().position(|x| &**x == a) {
                            Some(i) => i,
                            None => {
                                d.spawn_anims.push(Arc::from(a));
                                d.spawn_anims.len() - 1
                            }
                        };
                        u8::try_from(i).ok()
                    });
                    if let Some(c) = d.clusters.get_mut(u(1) as usize) {
                        c.points.push(SpawnPoint {
                            at: v3(2),
                            heading: n(5),
                            air: kind == "air",
                            leap,
                            anim,
                        });
                    }
                }
                "sniper" if f.len() >= 5 => d.perches.push((v3(1), n(4))),
                _ => {}
            }
        }
        d
    }

    /// Each AI type's enemy, by its name among enemies.rs's kinds.
    fn resolve(&mut self, kinds: &Kinds) {
        for (_, types) in &mut self.groups {
            for t in types {
                t.kind = kinds.kinds.iter().position(|k| k.id == t.name);
                if t.kind.is_none() {
                    warn!("waves: no enemy {} (tools/build_enemies.py)", t.name);
                }
            }
        }
    }

    /// The last wave (the spawn list's FinalWave), counting from 0.
    fn final_wave(&self) -> i32 {
        self.waves.len() as i32 - 1
    }
}

/// The map in play's waves, as it comes into play.
fn load_waves(current_map: Res<CurrentMap>, kinds: Res<Kinds>, mut data: ResMut<WaveData>) {
    let stem = current_map
        .0
        .and_then(|m| crate::MAPS.get(m))
        .map(|m| m.stem);
    if data.loaded == Some(stem) {
        return;
    }
    let mut d = match stem {
        Some(stem) => match std::fs::read_to_string(crate::assets_dir().join(waves_file(stem))) {
            Ok(text) => WaveData::parse(&text),
            Err(_) => {
                info!(
                    "{} not found (tools/build_waves.py makes it): no waves",
                    waves_file(stem)
                );
                WaveData::default()
            }
        },
        None => WaveData::default(),
    };
    d.resolve(&kinds);
    d.loaded = Some(stem);
    if !d.waves.is_empty() {
        info!(
            "waves: {} waves, {} spawn groups, {} clusters ({} points), {} perches",
            d.waves.len(),
            d.groups.len(),
            d.clusters.len(),
            d.clusters.iter().map(|c| c.points.len()).sum::<usize>(),
            d.perches.len()
        );
    }
    *data = d;
}

// ---------------------------------------------------------------------------
// The host's waves
// ---------------------------------------------------------------------------

/// TnSurvivalGoalTimePressure's TimeState.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// The wait before the first countdown (StartGameDownTime).
    FirstWait,
    /// The countdown to a wave (DownTime).
    Countdown,
    /// The wave's enemies coming in, and being fought.
    Spawning,
}

/// An AI type's enemies in play (TnSurvivalDataStoreWaveSpawning's LivingAI): their
/// numbers, and when the last of them died (the director's clock).
#[derive(Clone, Debug, Default)]
struct Living {
    ids: Vec<u32>,
    last_death: Option<f32>,
}

/// The cluster open for a spawn request (TnSpawnPointManager's ActiveCluster): the
/// request (its identifier and sight rule), the cluster, how long it has been open
/// and its score.
#[derive(Clone, Debug)]
struct Active {
    request: u32,
    sight: u8,
    cluster: Option<usize>,
    uptime: f32,
}

/// The host's (or a lone player's) waves: the game's goals and data store as one.
#[derive(Resource)]
struct Director {
    /// Seconds since the match started here (the game's TimeSeconds for cooldowns).
    clock: f32,
    rng: u32,
    // TnSurvivalGoalTimePressure
    seen_living: bool,
    phase: Phase,
    state_time: f32,
    /// ShowEnemiesActive, NumAIForAnnouncement, and the team's failed rounds acted on.
    markers: bool,
    announced: usize,
    seen_round: u8,
    // The GRI's countdown (ResetCountdown, DecrementCountdown each second).
    counting: bool,
    countdown: i32,
    tick_in: f32,
    // TnSurvivalGoalWaveSpawning: CurrentlySpawning as it last saw it, and
    // WaitingToEndGameTime.
    goal_spawning: bool,
    win_in: f32,
    won: bool,
    // TnSurvivalDataStoreWaveSpawning
    wave: i32,
    all_died: bool,
    spawning: bool,
    group: Option<usize>,
    living_budget: u32,
    max_living: u32,
    total: u32,
    used: u32,
    living: Vec<Living>,
    /// The wave's players' buffs are on (ApplyWaveBuffsToPlayers till
    /// RemoveWaveBuffsFromPlayers).
    on: bool,
    // The spawn action: when it last went, and its cooldown condition's
    // CurrentCooldownTime and OldTimeSinceLastActivation.
    last_spawn: Option<f32>,
    cooldown: f32,
    old_since: f32,
    // TnSpawnPointManager
    active: Vec<Active>,
    since_update: f32,
    /// Each cluster's sight lines to the players up (whether the level is in the
    /// way of each: the sight rule's traces), as last traced, and the cluster the
    /// next update's traces start at.
    blocked: Vec<Vec<bool>>,
    next_trace: usize,
    /// The enemies in play (the AI count's list), and the next one's number.
    enemies: Vec<EnemySnap>,
    next_id: u32,
    /// For the others: the messages so far (the last few), the rewards paid, the
    /// rounds won and the waves ended by force.
    messages: Vec<(u8, u16)>,
    next_message: u8,
    paid: (u8, u16),
    cleared: u8,
    purged: u8,
    /// The demo's: how long each enemy has gone unseen by every player up (looked
    /// at every STRAND_CHECK), the next look's countdown, and how many have been
    /// taken out so.
    unseen: HashMap<u32, f32>,
    strand_check: f32,
    withdrawn: u32,
}

impl Default for Director {
    fn default() -> Self {
        Director::new(0x9e37_79b9)
    }
}

/// What the director sees of the world this frame.
struct World<'a> {
    data: &'a WaveData,
    kinds: &'a Kinds,
    level: &'a Level,
    /// Each cluster on, as the doors have it (their Kismet switches some): worked
    /// out when first asked for.
    doors: Option<&'a crate::doors::DoorsState>,
    open: std::cell::OnceCell<Vec<bool>>,
    /// The players up: each one's middle, his eyes, his feet, his radius and height.
    players: Vec<Body>,
    /// The enemies here, living or dying: each one's feet, radius and height.
    enemies: Vec<(Vec3, f32, f32)>,
    /// The waves' enemies here living and in (not flying or leaping in): each one's
    /// number, feet and height.
    foes: Vec<(u32, Vec3, f32)>,
}

/// A player up, as the spawn point manager sees him.
#[derive(Clone, Copy, Debug)]
struct Body {
    centre: Vec3,
    eyes: Vec3,
    feet: Vec3,
    radius: f32,
    height: f32,
}

impl Director {
    fn new(seed: u32) -> Self {
        Director {
            clock: 0.0,
            rng: seed.max(1),
            seen_living: false,
            phase: Phase::FirstWait,
            state_time: 0.0,
            markers: false,
            announced: 0,
            seen_round: 0,
            counting: false,
            countdown: 0,
            tick_in: 1.0,
            goal_spawning: false,
            win_in: 0.0,
            won: false,
            wave: -1,
            all_died: false,
            spawning: false,
            group: None,
            living_budget: 0,
            max_living: 0,
            total: 0,
            used: 0,
            living: Vec::new(),
            on: false,
            last_spawn: None,
            cooldown: -1.0,
            old_since: f32::MAX,
            active: Vec::new(),
            since_update: 0.0,
            blocked: Vec::new(),
            next_trace: 0,
            enemies: Vec::new(),
            next_id: 1,
            messages: Vec::new(),
            next_message: 0,
            paid: (0, 0),
            cleared: 0,
            purged: 0,
            unseen: HashMap::new(),
            strand_check: STRAND_CHECK,
            withdrawn: 0,
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    /// 0 to 1 (not 1).
    fn rand(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// 0 to n - 1.
    fn rand_int(&mut self, n: usize) -> usize {
        ((self.rand() * n as f32) as usize).min(n.saturating_sub(1))
    }

    /// The waves as the others are told of them.
    fn snap(&self) -> WaveSnap {
        WaveSnap {
            seq: 0,
            wave: self.wave.clamp(-1, 127) as i8,
            on: self.on,
            counting: self.counting,
            countdown: self.countdown.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            markers: self.markers,
            messages: self.messages.clone(),
            paid: self.paid,
            cleared: self.cleared,
            purged: self.purged,
            won: self.won,
            enemies: self.enemies.clone(),
        }
    }

    /// A message for every player (BroadcastLocalizedMessage of
    /// TnSurvivalTimeMessage): its type and wave as its switch.
    fn message(&mut self, kind: u8, wave: i32) {
        self.next_message = self.next_message.wrapping_add(1);
        let switch = kind as u16 + 100 * wave.clamp(0, 600) as u16;
        self.messages.push((self.next_message, switch));
        let extra = self.messages.len().saturating_sub(4);
        self.messages.drain(..extra);
    }

    /// GetMessageSwitch's wave: the next (`next`) or this one, counting from 1.
    fn message_wave(&self, next: bool) -> i32 {
        (self.wave + 1 + i32::from(next)).max(1)
    }

    /// The GRI's ResetCountdown: on (counting down from `value` each second) or off.
    fn reset_countdown(&mut self, on: bool, value: i32) {
        self.counting = on;
        self.countdown = value;
        self.tick_in = 1.0;
    }

    /// The enemies that have died (their sensor's RemoveAI): each one's type's
    /// living budget back, and its death's time.
    fn remove_dead(&mut self, types: &[AiType], alive: &impl Fn(u32) -> bool) {
        let clock = self.clock;
        for (i, l) in self.living.iter_mut().enumerate() {
            let before = l.ids.len();
            l.ids.retain(|id| alive(*id));
            let dead = (before - l.ids.len()) as u32;
            if dead > 0 {
                l.last_death = Some(clock);
                let cost = types.get(i).map_or(0, |t| t.cost);
                self.living_budget = self.living_budget.saturating_sub(cost * dead);
            }
        }
        self.enemies.retain(|e| alive(e.id));
    }

    /// A frame of the waves: the host's (or a lone player's), `alive` saying which
    /// of its enemies live here.
    fn step(&mut self, dt: f32, team: &mut Team, w: &World, alive: impl Fn(u32) -> bool) {
        self.clock += dt;
        let types: Vec<AiType> = self
            .group
            .and_then(|g| w.data.groups.get(g))
            .map(|(_, t)| t.clone())
            .unwrap_or_default();
        self.remove_dead(&types, &alive);
        if team.over {
            return;
        }
        self.strand(dt, &types, w);
        // The GRI's countdown, a second at a time.
        if self.counting {
            self.tick_in -= dt;
            while self.tick_in <= 0.0 {
                self.tick_in += 1.0;
                self.countdown -= 1;
            }
        }
        self.time_pressure(dt, team, w);
        self.wave_goal(dt, team, w);
        self.spawn_action(w);
        let n = w.data.clusters.len();
        for _ in 0..TRACES_PER_FRAME.min(n) {
            let c = self.next_trace % n;
            self.next_trace = (c + 1) % n;
            self.trace(c, w);
        }
        self.update_clusters(dt, w);
    }

    /// TnSurvivalGoalTimePressure.OnUpdate: from the first living player, the wait
    /// and the countdowns; the wave's end once its enemies are all in and dead, or
    /// once a life is lost (revive.rs counts nobody up, as the game does here). With
    /// nobody up it stands still.
    fn time_pressure(&mut self, dt: f32, team: &Team, w: &World) {
        let s = &w.data.settings;
        let anyone = !w.players.is_empty();
        if !self.seen_living {
            if !anyone {
                return;
            }
            self.seen_living = true;
            self.phase = Phase::FirstWait;
            self.state_time = s.first_wait;
        }
        if team.round != self.seen_round {
            self.seen_round = team.round;
            self.end_round(true, w);
            return;
        }
        if !anyone {
            return;
        }
        let old = self.state_time;
        self.state_time -= dt;
        // (A float's whole part, as UnrealScript's int() takes it: towards 0.)
        let whole = |t: f32| t as i32;
        if self.phase == Phase::Countdown
            && self.state_time < COUNTDOWN_MESSAGES.len() as f32
            && whole(self.state_time) != whole(old)
        {
            let kind = COUNTDOWN_MESSAGES[whole(self.state_time).max(0) as usize];
            if kind != 0 {
                let wave = self.message_wave(!self.all_died);
                self.message(kind, wave);
            }
        } else if self.phase == Phase::Spawning && self.used >= self.total {
            let left = self.enemies.len();
            if left == 0 {
                self.end_round(false, w);
                return;
            }
            if left <= s.show_at {
                self.markers = true;
                if self.announced != left {
                    self.announced = left;
                    if left == 1 {
                        self.message(MSG_ONE_LEFT, 0);
                    } else if left == 3 {
                        self.message(MSG_THREE_LEFT, 0);
                    }
                }
            }
        }
        if self.phase == Phase::Spawning || self.state_time > 0.0 {
            return;
        }
        match self.phase {
            Phase::FirstWait => {
                self.reset_countdown(true, s.wait as i32);
                self.state_time = s.wait;
                self.phase = Phase::Countdown;
            }
            Phase::Countdown => {
                self.reset_countdown(false, 0);
                // (Event 7: the spawning starts.)
                self.spawning = true;
                self.state_time = -1.0;
                self.phase = Phase::Spawning;
            }
            Phase::Spawning => {}
        }
    }

    /// EndRound: the countdown to the next wave; the spawning stopped (and, failed,
    /// the round's players all died); the markers off; the round's end (won, the
    /// round counted for revive.rs to bring the players back; a failure's, its life);
    /// and its message.
    fn end_round(&mut self, failed: bool, w: &World) {
        self.state_time = w.data.settings.wait;
        self.phase = Phase::Countdown;
        if failed {
            self.all_died = true;
        }
        self.spawning = false;
        self.markers = false;
        self.reset_countdown(true, w.data.settings.wait as i32);
        if !failed {
            self.cleared = self.cleared.wrapping_add(1);
        }
        let wave = self.message_wave(false);
        self.message(if failed { MSG_FAILED } else { MSG_COMPLETE }, wave);
    }

    /// TnSurvivalGoalWaveSpawning.OnUpdate: the game won a while after the last wave;
    /// as the spawning stops, the wave's end (a failed one's enemies gone first) or
    /// the win; as it starts, the next wave (the same again after a failure).
    fn wave_goal(&mut self, dt: f32, team: &mut Team, w: &World) {
        if self.win_in > 0.0 {
            self.win_in -= dt;
            if self.win_in <= 0.0 {
                self.won = true;
                team.over = true;
            }
            return;
        }
        if self.goal_spawning == self.spawning {
            return;
        }
        self.goal_spawning = self.spawning;
        if !self.spawning {
            if self.all_died {
                self.force_end_wave();
                self.end_wave(w);
            } else if self.wave >= w.data.final_wave() {
                // (The wave's line goes: past the last.)
                self.win_in = w.data.settings.won_after.max(1e-3);
                self.wave = w.data.final_wave() + 2;
            } else {
                self.end_wave(w);
            }
        } else {
            let again = self.wave >= 0 && self.all_died;
            let next = self.wave + i32::from(!again);
            self.start_wave(next, w);
        }
    }

    /// StartWave and the data store's NewWave: a spawn group of the wave's at
    /// random, its budgets, its types' lists, and the wave's buffs on.
    fn start_wave(&mut self, wave: i32, w: &World) {
        let Some(last) = w.data.waves.len().checked_sub(1) else {
            return;
        };
        let def = &w.data.waves[(wave.max(0) as usize).min(last)];
        let groups = def.groups.clone();
        let (living, total) = (def.living, def.total);
        let group = if groups.is_empty() {
            None
        } else {
            let id = groups[self.rand_int(groups.len())];
            w.data.groups.iter().position(|(g, _)| *g == id)
        };
        self.all_died = false;
        self.wave = wave;
        self.max_living = living;
        self.total = total;
        self.used = 0;
        self.group = group;
        let types = group.map_or(0, |g| w.data.groups[g].1.len());
        self.living = vec![Living::default(); types];
        self.on = true;
    }

    /// EndWave: the wave's buffs off, and each player's reward (the lost wave's, if
    /// the players all died).
    fn end_wave(&mut self, w: &World) {
        self.on = false;
        if self.wave < 0 {
            return;
        }
        let Some(def) = w.data.waves.get(self.wave as usize) else {
            return;
        };
        let reward = if self.all_died {
            def.money_lost
        } else {
            def.money
        };
        self.paid = (
            self.paid.0.wrapping_add(1),
            reward.min(u16::MAX as u32) as u16,
        );
    }

    /// ForceEndWave: the wave's enemies gone at once (not killed).
    fn force_end_wave(&mut self) {
        let gone: HashSet<u32> = self
            .living
            .iter_mut()
            .flat_map(|l| std::mem::take(&mut l.ids))
            .collect();
        self.enemies.retain(|e| !gone.contains(&e.id));
        self.living_budget = 0;
        self.spawning = false;
        self.purged = self.purged.wrapping_add(1);
    }

    /// The demo's: every STRAND_CHECK while the wave is spawning, each of its enemies
    /// in sight of a player up (from his eyes or his middle, its middle or its head
    /// clear of the level: a locked door's force field is in the way) seen; one
    /// unseen by all of them for STRANDED_SECS (the shorter once the wave's last few
    /// show) taken out.
    fn strand(&mut self, dt: f32, types: &[AiType], w: &World) {
        if !self.spawning {
            self.unseen.clear();
            self.strand_check = STRAND_CHECK;
            return;
        }
        self.strand_check -= dt;
        if self.strand_check > 0.0 {
            return;
        }
        let step = STRAND_CHECK - self.strand_check;
        self.strand_check = STRAND_CHECK;
        let roster: HashSet<u32> = self.enemies.iter().map(|e| e.id).collect();
        self.unseen.retain(|id, _| roster.contains(id));
        // (With nobody up nobody sees: the time stands still.)
        if w.players.is_empty() {
            return;
        }
        let limit = if self.markers {
            STRANDED_SECS.0
        } else {
            STRANDED_SECS.1
        };
        let mut out = Vec::new();
        for &(id, feet, height) in &w.foes {
            if !roster.contains(&id) {
                continue;
            }
            // (From his eyes or his middle: his eyes can be in a canopy he stands
            // under, or a wall he hugs.)
            let seen = w.players.iter().any(|p| {
                [p.eyes, p.centre].iter().any(|&from| {
                    [0.5, 0.9]
                        .iter()
                        .any(|share| !blocked(w.level, from, feet + Vec3::Y * (height * share)))
                })
            });
            let t = self.unseen.entry(id).or_insert(0.0);
            if seen {
                *t = 0.0;
            } else {
                *t += step;
                if *t >= limit {
                    out.push((id, feet, *t));
                }
            }
        }
        for (id, feet, t) in out {
            info!(
                "WAVES: enemy {id} at {feet:.1} unseen for {t:.0} s: taken out, another to come in"
            );
            self.withdraw(id, types);
        }
    }

    /// Takes one of the wave's enemies out of play (not killed: no death, no reward;
    /// the others told, purged) and gives its cost back, for another to come in.
    fn withdraw(&mut self, id: u32, types: &[AiType]) {
        for (i, l) in self.living.iter_mut().enumerate() {
            if let Some(at) = l.ids.iter().position(|&x| x == id) {
                l.ids.remove(at);
                let cost = types.get(i).map_or(0, |t| t.cost);
                self.living_budget = self.living_budget.saturating_sub(cost);
                self.used = self.used.saturating_sub(cost);
            }
        }
        self.enemies.retain(|e| e.id != id);
        self.unseen.remove(&id);
        self.purged = self.purged.wrapping_add(1);
        self.withdrawn += 1;
    }

    /// ShouldSpawn: spawning, the living budget with room and the total not spent.
    fn should_spawn(&self) -> bool {
        self.spawning && self.living_budget < self.max_living && self.used < self.total
    }

    /// The wave spawn action, its weight conditions checked each frame: not with as
    /// many in play as the map allows (ActionDisabled), its cooldown run out (a new
    /// one, 1 to 3 s, from each time it goes), and the wave spawning (ShouldSpawn).
    fn spawn_action(&mut self, w: &World) {
        if self.enemies.len() >= w.data.settings.most {
            return;
        }
        // (TimeSinceLastActivation: from the match's start before the first.)
        let since = self.clock - self.last_spawn.unwrap_or(0.0);
        let on_cooldown = since < self.cooldown;
        if self.old_since > since {
            let (lo, hi) = w.data.settings.apart;
            self.cooldown = lo + (hi - lo) * self.rand();
            self.old_since = since;
        }
        if on_cooldown || !self.should_spawn() {
            return;
        }
        self.last_spawn = Some(self.clock);
        self.spawn_one(w);
    }

    /// The spawn action's OnActivate: an AI type of the group's, at a spawn point of
    /// its request's open cluster.
    fn spawn_one(&mut self, w: &World) {
        let Some(g) = self.group else {
            return;
        };
        let types = &w.data.groups[g].1;
        if self.living.len() != types.len() {
            self.living.resize(types.len(), Living::default());
        }
        let Some(i) = self.pick_type(types) else {
            return;
        };
        let t = &types[i];
        let Some(kind) = t.kind.filter(|&k| k < w.kinds.kinds.len()) else {
            // (An enemy the demo hasn't: its cost spent, so the wave can end.)
            self.used += t.cost;
            return;
        };
        let k = &w.kinds.kinds[kind];
        let Some(point) = self.find_spawn_point(t.request, t.sight, k, w) else {
            return;
        };
        let (at, yaw, to) = self.placement(point, k, w);
        let id = self.next_id;
        self.next_id += 1;
        let seed = self.next_u32();
        self.enemies.push(EnemySnap {
            id,
            kind: kind as u8,
            at,
            yaw,
            seed,
            to,
            leap: point.leap,
            owner: 0,
        });
        self.living[i].ids.push(id);
        self.living_budget += t.cost;
        self.used += t.cost;
    }

    /// GetAISpawnIndex: by weight, among the types not at their most at once nor
    /// within their cooldown of one dying.
    fn pick_type(&mut self, types: &[AiType]) -> Option<usize> {
        let ok: Vec<usize> = (0..types.len())
            .filter(|&i| {
                let (t, l) = (&types[i], &self.living[i]);
                let full = t.most != 0 && l.ids.len() >= t.most;
                let cooling =
                    t.cooldown != 0.0 && l.last_death.is_some_and(|d| self.clock - d < t.cooldown);
                !full && !cooling
            })
            .collect();
        if ok.is_empty() {
            return None;
        }
        let total: f32 = ok.iter().map(|&i| types[i].weight).sum();
        let mut pick = self.rand() * total;
        for &i in &ok {
            pick -= types[i].weight;
            if pick <= 0.0 {
                return Some(i);
            }
        }
        ok.last().copied()
    }

    /// FindSpawnPoint: the request's open cluster (opened, and its best found, the
    /// first time it is asked for), up to 4 of its points at random for one with
    /// room; then (the demo's enemies never moving off theirs) the request's other
    /// clusters, best first.
    fn find_spawn_point(
        &mut self,
        request: u32,
        sight: u8,
        k: &Kind,
        w: &World,
    ) -> Option<SpawnPoint> {
        let i = match self
            .active
            .iter()
            .position(|a| a.request == request && a.sight == sight)
        {
            Some(i) => i,
            None => {
                self.active.push(Active {
                    request,
                    sight,
                    cluster: None,
                    uptime: 0.0,
                });
                let i = self.active.len() - 1;
                self.update_cluster(i, 0.0, w);
                i
            }
        };
        let open = self.active[i].cluster?;
        if let Some(p) = self.try_cluster(open, k, w) {
            return Some(p);
        }
        let mut others: Vec<(usize, f32)> = (0..w.data.clusters.len())
            .filter(|&c| c != open && valid(c, request, w))
            .map(|c| (c, self.score(c, sight, w)))
            .collect();
        others.sort_by(|a, b| b.1.total_cmp(&a.1));
        others
            .into_iter()
            .find_map(|(c, _)| self.try_cluster(c, k, w))
    }

    /// Up to 4 of a cluster's points at random (GetSpawnPoint), the first with room
    /// (IsSafeSpawnLocation): a point on the ground set on the floor under it.
    fn try_cluster(&mut self, c: usize, k: &Kind, w: &World) -> Option<SpawnPoint> {
        let cluster = &w.data.clusters[c];
        if cluster.points.is_empty() {
            return None;
        }
        for _ in 0..SPAWN_TRIES {
            let mut p = cluster.points[self.rand_int(cluster.points.len())];
            if !p.air {
                p.at = floor(w.level, p.at);
            }
            if room(p.at, k, w) {
                return Some(p);
            }
        }
        None
    }

    /// Where an enemy comes in at `p`, the way it faces, and where it goes from
    /// there: on the ground, nowhere; from the air, a jet or a Sniper nowhere (its
    /// AI flies it: to its fight, to a perch), anyone else to the nearest point on
    /// the ground nobody has taken, facing the way it flies; one leaping in, to the
    /// nearest point on the ground.
    fn placement(&self, p: SpawnPoint, k: &Kind, w: &World) -> (Vec3, f32, Vec3) {
        if !p.air && !p.leap {
            return (p.at, p.heading, p.at);
        }
        let taken = |at: Vec3| self.enemies.iter().any(|e| flat(e.to, at) < TAKEN_REACH);
        let nearest = |list: &mut dyn Iterator<Item = Vec3>| {
            list.filter(|&g| !taken(g))
                .min_by(|a, b| flat(*a, p.at).total_cmp(&flat(*b, p.at)))
        };
        let ground = || {
            w.data
                .clusters
                .iter()
                .enumerate()
                .filter(|(c, _)| w.is_open(*c))
                .flat_map(|(_, c)| c.points.iter())
                .filter(|q| !q.air && !q.leap)
                .map(|q| floor(w.level, q.at))
        };
        let to = if p.leap {
            nearest(&mut ground())
        } else if k.form == Form::Jet || k.ai_type == "AICT_Sniper" {
            // (A jet flies itself to its fight, a Sniper to its perch: their AIs,
            // ai/jet.rs and ai/sniper.rs.)
            Some(p.at)
        } else {
            nearest(&mut ground())
        }
        .unwrap_or(p.at);
        let way = Vec3::new(to.x - p.at.x, 0.0, to.z - p.at.z);
        let yaw = if way.length_squared() > 1e-4 {
            (-way.x).atan2(-way.z)
        } else {
            p.heading
        };
        (p.at, yaw, to)
    }

    /// UpdateClusters, ten times a second.
    fn update_clusters(&mut self, dt: f32, w: &World) {
        self.since_update += dt;
        if self.since_update < CLUSTER_UPDATE_SECS {
            return;
        }
        let step = std::mem::take(&mut self.since_update);
        for i in 0..self.active.len() {
            self.update_cluster(i, step, w);
        }
    }

    /// A request's open cluster: every cluster serving it scored, the best opened
    /// when it beats the open one by EARLY_CUTOFF, or the open one has been open
    /// CLUSTER_SECS (at once when it is first asked for: `dt` 0).
    fn update_cluster(&mut self, i: usize, dt: f32, w: &World) {
        self.active[i].uptime += dt;
        let (request, sight, open) = (
            self.active[i].request,
            self.active[i].sight,
            self.active[i].cluster,
        );
        let mut top: Option<usize> = None;
        let mut top_score = f32::MIN;
        let mut current = 0.0;
        for c in 0..w.data.clusters.len() {
            if !valid(c, request, w) {
                continue;
            }
            let s = self.score(c, sight, w);
            if Some(c) == open {
                current = s;
            }
            if s > top_score {
                top = Some(c);
                top_score = s;
            }
        }
        let a = &mut self.active[i];
        if top == a.cluster {
            return;
        }
        if dt == 0.0 || top_score - current >= EARLY_CUTOFF || a.uptime >= CLUSTER_SECS {
            a.cluster = top;
            a.uptime = 0.0;
        }
    }

    /// Cluster `c`'s sight lines to the players up, traced now.
    fn trace(&mut self, c: usize, w: &World) {
        if self.blocked.len() < w.data.clusters.len() {
            self.blocked.resize(w.data.clusters.len(), Vec::new());
        }
        let centre = w.data.clusters[c].centre;
        self.blocked[c] = w
            .players
            .iter()
            .map(|p| blocked(w.level, centre, p.eyes))
            .collect();
    }

    /// Cluster `c`'s score for a request with the sight rule `sight` (ScoreCluster:
    /// each player up's spawn modifier rules, summed), its sight lines as last
    /// traced (now, if there are others up since).
    fn score(&mut self, c: usize, sight: u8, w: &World) -> f32 {
        if self
            .blocked
            .get(c)
            .is_none_or(|b| b.len() != w.players.len())
        {
            self.trace(c, w);
        }
        let centre = w.data.clusters[c].centre;
        w.players
            .iter()
            .zip(&self.blocked[c])
            .map(|(p, &hidden)| {
                // (Out of sight wanted: hidden 1, seen -1; in sight wanted, the
                // other way round; either, 1.)
                let seen = match (sight, hidden) {
                    (SIGHT_EITHER, _) | (SIGHT_OUT, true) | (SIGHT_IN, false) => 1.0,
                    _ => -1.0,
                };
                let units = centre.distance(p.centre) * UNITS;
                let units = if units == 0.0 { 1.0 } else { units };
                SIGHT_FACTOR * seen + NEAR_FACTOR / units
            })
            .sum()
    }
}

/// Cluster `c` serves a request (ValidForScoring): on, and the request's.
fn valid(c: usize, request: u32, w: &World) -> bool {
    w.data.clusters[c].request == request && w.is_open(c)
}

impl World<'_> {
    /// Whether cluster `c` is on.
    fn is_open(&self, c: usize) -> bool {
        let open = self.open.get_or_init(|| {
            self.data
                .clusters
                .iter()
                .map(|cl| self.doors.is_none_or(|d| d.cluster_on(&cl.name)))
                .collect()
        });
        open.get(c).copied().unwrap_or(true)
    }
}

/// Whether the level is in the way from `from` to `to` (FastTrace: its geometry).
fn blocked(level: &Level, from: Vec3, to: Vec3) -> bool {
    let d = to - from;
    let len = d.length();
    len > 1e-3 && level.ray(from, d / len, len).is_some()
}

/// How far apart two places are across the ground.
fn flat(a: Vec3, b: Vec3) -> f32 {
    Vec2::new(a.x - b.x, a.z - b.z).length()
}

/// The floor under a spawn point (it stands a little over it), or the point.
fn floor(level: &Level, at: Vec3) -> Vec3 {
    level
        .ground(at, 0.3, at.y + 1.0, at.y - 8.0)
        .map_or(at, |y| Vec3::new(at.x, y, at.z))
}

/// Whether an enemy of kind `k` has room at `at` (its feet): no living enemy or
/// player up in its cylinder.
fn room(at: Vec3, k: &Kind, w: &World) -> bool {
    let (r, h) = (k.radius, k.half_height * 2.0);
    let clear = |feet: Vec3, radius: f32, height: f32| {
        flat(at, feet) >= r + radius || at.y + h <= feet.y || feet.y + height <= at.y
    };
    w.enemies.iter().all(|&(f, rr, hh)| clear(f, rr, hh))
        && w.players.iter().all(|p| clear(p.feet, p.radius, p.height))
}

/// This PC runs the waves (and its AI the enemies: src/ai): it hosts, or plays
/// without the network.
pub fn hosting(net: &Net) -> bool {
    let online = net.my_slot().is_some() && net.player_count() > 0;
    !online || net.is_host()
}

/// The host's (or a lone player's) waves, a frame on, for its rosters and itself.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn direct(
    time: Res<Time>,
    mut net: ResMut<Net>,
    (data, kinds, level, doors): (
        Res<WaveData>,
        Res<Kinds>,
        Res<Level>,
        Res<crate::doors::DoorsState>,
    ),
    mut director: ResMut<Director>,
    mut team: ResMut<Team>,
    waves: Res<Waves>,
    players: Query<(&Player, &Transform)>,
    enemies: Query<(&Transform, &Foe, Option<&Enemy>, Option<&WaveFoe>)>,
) {
    if !hosting(&net) || data.waves.is_empty() {
        return;
    }
    let world = World {
        data: &data,
        kinds: &kinds,
        level: &level,
        doors: Some(&doors),
        open: std::cell::OnceCell::new(),
        players: players
            .iter()
            .filter(|(p, _)| crate::revive::up(p))
            .map(|(p, t)| {
                let (radius, height) = p.size();
                let feet = t.translation;
                Body {
                    centre: feet + Vec3::Y * (height * 0.5),
                    eyes: feet + Vec3::Y * (height * EYES),
                    feet,
                    radius,
                    height,
                }
            })
            .collect(),
        // (The dying too: none comes in where a body lies.)
        enemies: enemies
            .iter()
            .filter_map(|(t, foe, ..)| {
                let k = kinds.kinds.get(foe.kind)?;
                Some((t.translation, k.radius, k.half_height * 2.0))
            })
            .collect(),
        foes: enemies
            .iter()
            .filter_map(|(t, foe, enemy, wave)| {
                let wave = wave.filter(|w| w.owner.is_none() && w.landed())?;
                enemy.filter(|e| e.alive())?;
                let k = kinds.kinds.get(foe.kind)?;
                Some((wave.id, t.translation, k.half_height * 2.0))
            })
            .collect(),
    };
    // (An enemy not brought in here yet lives; one that died, or has gone, doesn't.)
    let alive = |id: u32| {
        if waves.gone.contains(&id) {
            return false;
        }
        waves
            .known
            .get(&id)
            .is_none_or(|&e| enemies.get(e).is_ok_and(|(_, _, en, _)| en.is_some()))
    };
    director.step(time.delta_secs(), &mut team, &world, alive);
    // (The Titans' drones too, for the others to bring in: the waves don't count
    // them.)
    let mut snap = director.snap();
    snap.enemies
        .extend(waves.drones.iter().filter(|d| alive(d.id)).copied());
    net.set_waves(snap);
}

// ---------------------------------------------------------------------------
// Every PC's waves
// ---------------------------------------------------------------------------

/// One of the waves' enemies here (or a Titan's drone, `owner` its Titan's number):
/// its number, its last blow while it lived (its death goes to the others with it),
/// whether its death has been told (or came from the others), and its flight in, if
/// it flies in.
#[derive(Component)]
pub struct WaveFoe {
    pub id: u32,
    pub owner: Option<u32>,
    hit: Hit,
    told: bool,
    glide: Option<Glide>,
    /// The animation it comes in with, its spawn point's (ai/mod.rs's attach
    /// plays it, if its clips have it).
    pub spawn_anim: Option<Arc<str>>,
}

/// One of the waves' enemies taken out of play (a failed wave's, ForceEndWave; one
/// stranded out of the players' reach): not killed. Its AI lets it go (ai.rs's
/// think, ai/sync.rs's follow_host: out of its squad, its claims given up) and it
/// goes; any the AI hasn't, follow() takes the next frame.
#[derive(Component)]
pub struct Withdrawn;

impl WaveFoe {
    /// It has come down (or never flew in): it stands where it goes.
    pub fn landed(&self) -> bool {
        self.glide.is_none()
    }

    /// Its death has been told (or came from the others: then it is theirs, not
    /// this PC's, to tell of: killfeed.rs).
    pub fn told(&self) -> bool {
        self.told
    }
}

/// The numbers the host's enemies brought in but by the waves (its Titans' drones)
/// are given (after any the waves' could reach in a match), up to the most the
/// network's two bytes carry.
const DRONE_IDS: u32 = 0xc000;
const DRONE_ID_COUNT: u32 = 0x4000;

/// The waves' systems (the AI's come after: ai.rs).
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WaveSystems;

/// A way in: from where it came in to where it goes, whether it leaps there and
/// how high it rises over the way (its arc), how long it takes and how far it has
/// come.
#[derive(Clone, Copy, Debug)]
struct Glide {
    from: Vec3,
    to: Vec3,
    leap: bool,
    arc: f32,
    secs: f32,
    t: f32,
}

impl Glide {
    /// From `from` to `to`: flying in (easing in to where it stops), or leaping
    /// (rising a fifth of the way across, over any drop).
    fn new(from: Vec3, to: Vec3, leap: bool) -> Option<Glide> {
        let way = from.distance(to);
        if way <= 0.05 {
            return None;
        }
        let (arc, speed) = if leap {
            let arc = flat(from, to) * LEAP_ARC - (from.y - to.y).max(0.0);
            (arc.max(0.0), LEAP_SPEED)
        } else {
            (0.0, FLY_IN_SPEED)
        };
        Some(Glide {
            from,
            to,
            leap,
            arc,
            secs: (way / speed).max(0.5),
            t: 0.0,
        })
    }

    /// Where it is now.
    fn at(&self) -> Vec3 {
        let k = (self.t / self.secs).clamp(0.0, 1.0);
        if self.leap {
            self.from.lerp(self.to, k) + Vec3::Y * (4.0 * self.arc * k * (1.0 - k))
        } else {
            self.from.lerp(self.to, 1.0 - (1.0 - k) * (1.0 - k))
        }
    }
}

/// The waves as this PC plays them out: the host's (its own, or its rosters'), and
/// its enemies here.
#[derive(Resource, Default)]
pub struct Waves {
    /// What this PC shows: the waves as last taken up.
    pub snap: WaveSnap,
    /// The number of the waves last taken up, and of the last message played.
    seen: Option<u16>,
    last_message: Option<u8>,
    /// The enemies brought in here (by number), those gone (dead or ended), and
    /// those gone from the host's roster still to die here (and when).
    known: HashMap<u32, Entity>,
    gone: HashSet<u32>,
    doomed: Vec<(Entity, f32)>,
    /// The host's (or a lone player's) enemies come in but by the waves (its
    /// Titans' drones), for its rosters (the waves don't count them), and how many
    /// it has numbered.
    drones: Vec<EnemySnap>,
    drones_made: u32,
}

impl Waves {
    /// The enemy here numbered `id` (the waves', a drone's), while it is.
    pub fn entity_of(&self, id: u32) -> Option<Entity> {
        self.known.get(&id).copied()
    }

    /// An enemy come in here as `entity` but by the waves (the host's, or a lone
    /// player's: a Titan's drone, `snap`'s owner its Titan's number): its number, for
    /// the others to bring it in too (from `snap`: as it came in) and to be told of
    /// its death; and what it is here (its WaveFoe).
    pub fn adopt(&mut self, entity: Entity, mut snap: EnemySnap) -> WaveFoe {
        let id = DRONE_IDS + self.drones_made % DRONE_ID_COUNT;
        self.drones_made += 1;
        snap.id = id;
        self.known.insert(id, entity);
        self.gone.remove(&id);
        self.drones.push(snap);
        WaveFoe {
            id,
            owner: (snap.owner != 0).then_some(snap.owner),
            hit: Hit::default(),
            told: false,
            glide: None,
            spawn_anim: None,
        }
    }

    /// The wave the HUD's line shows (the GRI's _CurrentWave: 0 before the first,
    /// past the last once it is won).
    pub fn shown_wave(&self) -> u32 {
        if self.snap.wave < 0 {
            0
        } else {
            (self.snap.wave as u32 + 1).max(1)
        }
    }

    /// The last enemies show on the HUD (TnBuffSeeEnemyObjectiveMarkers).
    pub fn markers(&self) -> bool {
        self.snap.markers
    }

    /// The wave the team reached, for the final scores: the last one won's or the one
    /// it was on, from 1 (none past the last).
    pub fn reached(&self) -> u32 {
        self.shown_wave().clamp(1, WAVES_SHOWN)
    }
}

/// The other players' enemies' deaths (net_apply's JazzEvent::EnemyDeath): number,
/// how it died, its flags (bit 0: the blow gibs it), where the blow struck and its
/// momentum.
#[derive(Resource, Default)]
pub struct WaveInbox(pub Vec<(u32, u8, u8, Vec3, Vec3)>);

/// A message (TnSurvivalTimeMessage.ClientReceive): the announcer's line (by the
/// side the players are on), a stinger with some, and its text typed under the
/// announcements, the wave in it.
fn play_message(
    switch: u16,
    decepticons: bool,
    sfx: &mut Sfx,
    huds: [&mut HudState; crate::split::SEATS],
) {
    let kind = (switch % 100) as usize;
    let wave = switch / 100;
    let voices = if decepticons {
        &VOICES_DECEPTICONS
    } else {
        &VOICES
    };
    if let Some(&Some(cue)) = voices.get(kind) {
        sfx.play(cue);
    }
    match kind as u8 {
        MSG_ONE => sfx.play(SND_WAVE_BEGIN),
        MSG_COMPLETE => sfx.play(SND_WAVE_WON),
        MSG_FAILED => sfx.play(SND_WAVE_FAILED),
        _ => {}
    }
    // (On each of this PC's players' HUDs.)
    if let Some(text) = TEXTS.get(kind).filter(|t| !t.is_empty()) {
        for hud in huds {
            hud.events.push(HudEvent::RewardAnnouncement(
                text.replace("`n", &wave.to_string()),
            ));
        }
    }
}

/// Every PC plays out the waves for its own player: the deaths the others tell of
/// (and its own, told to them), the host's waves as they change (its messages, the
/// countdown's ticks, the markers' scan, its rewards, its round won, its enemies
/// brought in and gone), and the HUD's part (the countdown, the wave's line); and
/// what the damage he takes is scaled by.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn follow(
    mut commands: Commands,
    time: Res<Time>,
    net: Res<Net>,
    (data, kinds, asset_server, current_map): (
        Res<WaveData>,
        Res<Kinds>,
        Res<AssetServer>,
        Res<CurrentMap>,
    ),
    (mut waves, mut inbox): (ResMut<Waves>, ResMut<WaveInbox>),
    (mut team, mut score, mut tally): (
        ResMut<Team>,
        crate::split::BySeat<Score>,
        crate::split::BySeat<crate::scoreboard::Tally>,
    ),
    (mut huds, mut sfx): (crate::split::BySeat<HudState>, ResMut<Sfx>),
    mut foes: Query<(&mut WaveFoe, Option<&mut Enemy>)>,
    mut me: Query<(&mut Player, Option<&crate::split::Seat>), With<LocalPlayer>>,
    withdrawn: Query<Entity, With<Withdrawn>>,
) {
    let waves = &mut *waves;
    let dt = time.delta_secs();
    // (Those taken out of play last frame that the AI didn't let go.)
    for e in &withdrawn {
        commands.entity(e).try_despawn();
    }
    let decepticons = current_map
        .0
        .and_then(|m| crate::MAPS.get(m))
        .is_some_and(|m| m.decepticons);
    // The deaths the others tell of: here too, as they died there.
    for (id, death, flags, at, momentum) in inbox.0.drain(..) {
        if let Some(&e) = waves.known.get(&id)
            && let Ok((mut wf, Some(mut enemy))) = foes.get_mut(e)
            && enemy.alive()
        {
            enemy.last_hit = Hit {
                gib: flags & 1 != 0,
                ..Hit::new(Death::from_index(death), at, momentum).with_momentum_vec(momentum)
            };
            enemy.health = 0.0;
            wf.told = true;
        }
    }
    // Those gone from the host's roster: dead here, their death not heard of.
    for (e, left) in &mut waves.doomed {
        *left -= dt;
        if *left <= 0.0
            && let Ok((mut wf, Some(mut enemy))) = foes.get_mut(*e)
        {
            enemy.health = 0.0;
            wf.told = true;
        }
    }
    waves.doomed.retain(|(_, left)| *left > 0.0);
    // Each enemy's last blow while it lives; its death here, for the others.
    let online = net.in_match();
    let mut told = Vec::new();
    for (mut wf, enemy) in &mut foes {
        match enemy {
            Some(e) => wf.hit = e.last_hit,
            None if !wf.told => {
                wf.told = true;
                told.push(JazzEvent::EnemyDeath {
                    id: wf.id,
                    death: wf.hit.death.index(),
                    flags: wf.hit.gib as u8,
                    at: wf.hit.at,
                    momentum: wf.hit.momentum,
                });
            }
            None => {}
        }
    }
    // (Told by the first player: no one online plays alongside split screen.)
    if online
        && let Some((mut player, _)) = me.iter_mut().find(|(_, s)| crate::split::seat_of(*s) == 0)
    {
        player.played.extend(told);
    }

    // The damage he takes: the difficulty's, and the wave's while it is on.
    let difficulty = crate::revive::DIFFICULTY_DAMAGE[net.difficulty().min(2)];
    let snap = net.waves().cloned();
    let wave_taken = snap
        .as_ref()
        .filter(|s| s.on && s.wave >= 0)
        .and_then(|s| data.waves.get(s.wave as usize))
        .map_or(1.0, |w| w.taken);
    TAKEN.store((difficulty * wave_taken).to_bits(), Ordering::Relaxed);
    // And the damage he does: the difficulty's (1.5 on Easy, 0.6 on Hard), and
    // the wave's while it is on.
    let wave_done = snap
        .as_ref()
        .filter(|s| s.on)
        .and_then(|s| data.waves.get(s.wave.max(0) as usize))
        .map_or(1.0, |w| w.done);
    let difficulty_done = crate::revive::DIFFICULTY_DAMAGE_DONE[net.difficulty().min(2)];
    DONE.store((difficulty_done * wave_done).to_bits(), Ordering::Relaxed);

    let Some(snap) = snap else {
        for hud in huds.each() {
            hud.wave = 0;
            hud.counting = false;
        }
        return;
    };
    if waves.seen != Some(snap.seq) {
        waves.seen = Some(snap.seq);
        let old = std::mem::take(&mut waves.snap);
        for &(seq, switch) in &snap.messages {
            if waves
                .last_message
                .is_none_or(|last| (seq.wrapping_sub(last) as i8) > 0)
            {
                waves.last_message = Some(seq);
                play_message(switch, decepticons, &mut sfx, huds.each());
            }
        }
        // (OnCountdownChange: each second from 10 down to 1.)
        if snap.counting
            && snap.countdown != old.countdown
            && (1..=TICK_FROM).contains(&snap.countdown)
        {
            sfx.play(SND_TICK);
        }
        if snap.markers && !old.markers {
            sfx.play(SND_MARKERS);
        }
        // The wave's reward (EndWave's ScoreEvent: each player's).
        if snap.paid.0 != old.paid.0 {
            let pay = snap.paid.1 as u32;
            for score in score.each() {
                score.energon += pay;
            }
            for tally in tally.each() {
                tally.earned(pay);
            }
        }
        // A round won brings the dead back and the downed up (revive.rs).
        team.cleared = snap.cleared;
        // The enemies brought in as the host has them, the players' buffs on what
        // is done to them (the difficulty's and the wave's DamageDone: done); those
        // it no longer has gone (at once, ended by force) or dying.
        let done = done();
        for e in &snap.enemies {
            if waves.known.contains_key(&e.id) || waves.gone.contains(&e.id) {
                continue;
            }
            let spawned = enemies::spawn(
                &mut commands,
                &asset_server,
                &kinds,
                e.kind as usize,
                (e.at, e.yaw),
                e.seed,
                done,
            );
            let Some(entity) = spawned else {
                waves.gone.insert(e.id);
                continue;
            };
            commands.entity(entity).insert(WaveFoe {
                id: e.id,
                owner: (e.owner != 0).then_some(e.owner),
                hit: Hit::default(),
                told: false,
                glide: Glide::new(e.at, e.to, e.leap),
                spawn_anim: data.spawn_anim_at(e.at),
            });
            waves.known.insert(e.id, entity);
        }
        let listed: HashSet<u32> = snap.enemies.iter().map(|e| e.id).collect();
        let missing: Vec<u32> = waves
            .known
            .keys()
            .filter(|id| !listed.contains(id))
            .copied()
            .collect();
        let purged = snap.purged != old.purged;
        for id in missing {
            let Some(e) = waves.known.remove(&id) else {
                continue;
            };
            waves.gone.insert(id);
            let alive = foes.get(e).is_ok_and(|(_, en)| en.is_some());
            if !alive {
                continue;
            }
            if purged {
                commands.entity(e).insert(Withdrawn);
            } else {
                waves.doomed.push((e, DOOM_SECS));
            }
        }
        // (The drones gone, gone from the roster too.)
        let gone = &waves.gone;
        waves.drones.retain(|d| !gone.contains(&d.id));
        waves.snap = snap;
    }
    // The HUD: the wave's line and the countdown (on each of this PC's players').
    for hud in huds.each() {
        hud.wave = waves.shown_wave();
        hud.counting = waves.snap.counting;
        hud.countdown = waves.snap.countdown as i32;
    }
}

/// Those flying or leaping in go over to where they go, then stand (or hover)
/// there.
fn glide(time: Res<Time>, mut foes: Query<(&mut WaveFoe, &mut Transform, &mut Foe), With<Enemy>>) {
    let dt = time.delta_secs();
    for (mut wf, mut transform, mut foe) in &mut foes {
        let Some(g) = wf.glide.as_mut() else {
            continue;
        };
        g.t = (g.t + dt).min(g.secs);
        transform.translation = g.at();
        if g.t >= g.secs {
            foe.spot = g.to;
            wf.glide = None;
        }
    }
}

/// A new match: no waves yet, here or for the others.
fn reset(
    time: Res<Time>,
    mut director: ResMut<Director>,
    mut waves: ResMut<Waves>,
    mut inbox: ResMut<WaveInbox>,
    mut net: ResMut<Net>,
) {
    let seed = (time.elapsed_secs_f64() * 1000.0) as u32 ^ 0x2545_f491;
    *director = Director::new(seed);
    *waves = Waves::default();
    inbox.0.clear();
    net.clear_waves();
    TAKEN.store(1f32.to_bits(), Ordering::Relaxed);
}

pub struct WavesPlugin;

impl Plugin for WavesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaveData>()
            .init_resource::<Director>()
            .init_resource::<Waves>()
            .init_resource::<WaveInbox>()
            .add_systems(OnEnter(crate::menu::AppState::Game), reset)
            .add_systems(Update, load_waves)
            .add_systems(
                Update,
                (direct, follow, glide)
                    .chain()
                    .in_set(WaveSystems)
                    .after(crate::revive::team_lives)
                    .before(crate::revive::round_end)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map's waves file as tools/build_waves.py writes it.
    const SAMPLE: &str = "\
# a test map's waves
settings\t10.0\t30.0\t3\t3.0\t3\t20\t2\t250\t5.0\t1.0\t3.0
wave\t1\t6\t8\t200\t500\t1.0\t1.0\t0\t-
wave\t2\t6\t8\t400\t500\t0.9\t1.2\t1\t-
type\t0\tGrunt\t2\t1.0\t0\t0.0\t1\t0
type\t1\tGrunt\t2\t1.0\t0\t0.0\t1\t0
type\t1\tFlyer\t4\t5.0\t1\t10.0\t3\t0
cluster\t0\t1\t0.0\t0.0\t-50.0\tNear
point\t0\t0.0\t0.0\t-50.0\t0.0\t-
point\t0\t10.0\t0.0\t-50.0\t0.0\t-
point\t0\t-10.0\t0.0\t-50.0\t0.0\t-
point\t0\t20.0\t0.0\t-50.0\t0.0\t-
cluster\t1\t1\t0.0\t0.0\t-200.0\tFar
point\t1\t0.0\t0.0\t-200.0\t0.0\t-
point\t1\t10.0\t0.0\t-200.0\t0.0\t-
cluster\t2\t3\t0.0\t60.0\t0.0\tSky
air\t2\t0.0\t60.0\t0.0\t0.0
sniper\t5.0\t3.0\t5.0\t0.0
";

    fn kinds() -> Kinds {
        let mut kinds = Kinds::default();
        for (id, form) in [("Grunt", Form::Robot), ("Flyer", Form::Jet)] {
            kinds.kinds.push(Kind {
                id: id.to_string(),
                model: String::new(),
                form,
                name: id.to_string(),
                ai_type: String::new(),
                score: 10.0,
                health: 100.0,
                damage_scale: 1.0,
                radius: 1.0,
                half_height: 1.0,
                energon: Vec3::ONE,
                energon_alpha: 1.0,
                decepticon: false,
                colours: Vec::new(),
                slots: Vec::new(),
                physics: Default::default(),
                sockets: Vec::new(),
                socket_scales: Vec::new(),
                clips: Vec::new(),
                deaths: enemies::DeathSet::Basic,
                guns: [None, None],
                gun_sockets: Vec::new(),
                vguns: Vec::new(),
                vgun_sockets: Vec::new(),
                scale: 1.0,
                npc: false,
                graph: Handle::default(),
                nodes: HashMap::new(),
                ai: None,
                other: None,
            });
        }
        kinds
    }

    #[test]
    fn reads_a_maps_waves() {
        let mut d = WaveData::parse(SAMPLE);
        d.resolve(&kinds());
        assert_eq!(d.settings.wait, 30.0);
        assert_eq!(d.settings.apart, (1.0, 3.0));
        assert_eq!(d.waves.len(), 2);
        assert_eq!((d.waves[1].money, d.waves[1].taken), (400, 1.2));
        assert_eq!(d.groups.len(), 2);
        assert_eq!(d.groups[1].1[1].kind, Some(1));
        assert_eq!(d.clusters.len(), 3);
        assert_eq!(d.clusters[0].points.len(), 4);
        assert!(d.clusters[2].points[0].air);
        assert_eq!(d.clusters[1].name, "Far");
        assert_eq!(d.perches.len(), 1);
        assert_eq!(d.final_wave(), 1);
    }

    /// A spawn point's animation (its SpawnAnimationName) is kept for the enemies
    /// that come in at it, at the point or on the floor under it; a leap's isn't
    /// one. Ancients' Swarmers come in at points whose animation brings them up out
    /// of the ground, and their clips have it.
    #[test]
    fn enemies_come_in_with_their_spawn_points_animation() {
        let text = SAMPLE
            .replace(
                "point\t0\t10.0\t0.0\t-50.0\t0.0\t-",
                "point\t0\t10.0\t0.5\t-50.0\t0.0\tNav_Spawn_Ground",
            )
            .replace(
                "point\t1\t10.0\t0.0\t-200.0\t0.0\t-",
                "point\t1\t10.0\t0.0\t-200.0\t0.0\tSpawn",
            );
        let d = WaveData::parse(&text);
        assert_eq!(d.spawn_anims, vec![Arc::<str>::from("Nav_Spawn_Ground")]);
        assert!(d.clusters[1].points[1].leap && d.clusters[1].points[1].anim.is_none());
        let on_floor = Vec3::new(10.0, -0.2, -50.0);
        assert_eq!(
            d.spawn_anim_at(on_floor).as_deref(),
            Some("Nav_Spawn_Ground")
        );
        assert_eq!(d.spawn_anim_at(Vec3::new(0.0, 0.0, -50.0)), None);
        assert_eq!(d.spawn_anim_at(Vec3::new(10.0, 0.0, -200.0)), None);

        let assets = crate::assets_dir();
        let Ok(text) = std::fs::read_to_string(assets.join(waves_file("ancients"))) else {
            return;
        };
        let d = WaveData::parse(&text);
        let animated: Vec<&SpawnPoint> = d
            .clusters
            .iter()
            .filter(|c| c.request == 2)
            .flat_map(|c| c.points.iter())
            .filter(|p| {
                p.anim
                    .is_some_and(|i| &*d.spawn_anims[i as usize] == "Nav_Spawn_Ground")
            })
            .collect();
        assert!(animated.len() >= 20, "{}", animated.len());
        if let Some(clips) = crate::ai::clips::for_type("SwarmerLite") {
            assert!(clips.find("Nav_Spawn_Ground").is_some());
        }
    }

    #[test]
    fn every_maps_waves_have_their_enemies() {
        let assets = crate::assets_dir();
        let kinds = Kinds::load(&assets);
        if kinds.kinds.is_empty() {
            return;
        }
        for map in crate::MAPS {
            let Ok(text) = std::fs::read_to_string(assets.join(waves_file(map.stem))) else {
                continue;
            };
            let mut d = WaveData::parse(&text);
            d.resolve(&kinds);
            assert_eq!(d.waves.len(), 15, "{}", map.stem);
            // (GlobalMaxAICount: 12 on Downfall, 14 on the others.)
            assert_eq!(
                d.settings.most,
                if map.stem == "downfall" { 12 } else { 14 },
                "{}",
                map.stem
            );
            for w in &d.waves {
                assert!(w.total >= w.living.min(w.total) && w.money > 0);
                for g in &w.groups {
                    let types = &d.groups.iter().find(|(id, _)| id == g).unwrap().1;
                    assert!(types.iter().all(|t| t.kind.is_some()), "{}", map.stem);
                    // (Each wave's types can fill its living budget.)
                    assert!(!types.is_empty());
                }
            }
            // Every type's request has clusters with points.
            for (_, types) in &d.groups {
                for t in types {
                    assert!(
                        d.clusters
                            .iter()
                            .any(|c| c.request == t.request && !c.points.is_empty()),
                        "{} {}",
                        map.stem,
                        t.name
                    );
                }
            }
        }
    }

    /// The test map's world: one player up at the middle, on flat ground.
    fn test_world<'a>(
        data: &'a WaveData,
        kinds: &'a Kinds,
        level: &'a Level,
        enemies: &[EnemySnap],
    ) -> World<'a> {
        World {
            data,
            kinds,
            level,
            doors: None,
            open: std::cell::OnceCell::new(),
            players: vec![Body {
                centre: Vec3::Y,
                eyes: Vec3::Y * 1.8,
                feet: Vec3::ZERO,
                radius: 0.5,
                height: 2.0,
            }],
            enemies: enemies.iter().map(|e| (e.to, 1.0, 2.0)).collect(),
            foes: enemies.iter().map(|e| (e.id, e.to, 2.0)).collect(),
        }
    }

    /// A director stepped through a test map's two waves, its one player killing
    /// an enemy whenever the wave's living budget is full, and once it is all in,
    /// one every 5 s.
    #[test]
    fn the_waves_run_as_the_game_runs_them() {
        let mut data = WaveData::parse(SAMPLE);
        let kinds = kinds();
        data.resolve(&kinds);
        let level = Level::default();
        let mut team = Team::default();
        let mut d = Director::new(7);
        let dt = 0.05;
        let mut dead: HashSet<u32> = HashSet::new();
        let mut recorded: HashSet<u32> = HashSet::new();
        let mut kinds_spawned = Vec::new();
        let mut jets_in_air = 0;
        let mut messages = Vec::new();
        let mut seen_message = 0u8;
        let mut first_count = None;
        let mut first_spawn = None;
        let mut paid = Vec::new();
        for frame in 0..8000 {
            let t = frame as f32 * dt;
            let w = test_world(&data, &kinds, &level, &d.enemies);
            d.step(dt, &mut team, &w, |id| !dead.contains(&id));
            if d.counting && first_count.is_none() {
                first_count = Some(t);
            }
            for e in &d.enemies {
                if recorded.insert(e.id) {
                    first_spawn.get_or_insert(t);
                    kinds_spawned.push((d.wave, e.kind));
                    // (A jet stays where it comes in: its AI flies it.)
                    if e.kind == 1 && e.to == e.at {
                        jets_in_air += 1;
                    }
                }
            }
            let alive: Vec<u32> = d
                .enemies
                .iter()
                .map(|e| e.id)
                .filter(|id| !dead.contains(id))
                .collect();
            let kill = if d.used < d.total {
                d.living_budget >= d.max_living && frame % 20 == 0
            } else {
                frame % 100 == 0
            };
            if kill && let Some(&id) = alive.first() {
                dead.insert(id);
            }
            for &(seq, switch) in &d.messages {
                if (seq.wrapping_sub(seen_message) as i8) > 0 {
                    seen_message = seq;
                    messages.push((t, switch));
                }
            }
            if d.paid.0 as usize > paid.len() {
                paid.push(d.paid.1);
            }
            if team.over {
                break;
            }
        }
        // The first countdown starts 10 s in, and the first wave 30 s after it.
        let first_count = first_count.expect("a countdown");
        assert!((first_count - 10.0).abs() < 0.1, "{first_count}");
        let first_spawn = first_spawn.expect("an enemy");
        assert!((first_spawn - 40.0).abs() < 0.2, "{first_spawn}");
        // "Wave 1 Begins In..." as 7 s are left, then Five to One a second apart.
        let kinds_of: Vec<u16> = messages.iter().map(|(_, s)| *s).collect();
        assert_eq!(&kinds_of[..6], &[101, 102, 103, 104, 105, 106]);
        assert!((messages[0].0 - 33.0).abs() < 0.1, "{}", messages[0].0);
        assert!((messages[5].0 - 39.0).abs() < 0.1, "{}", messages[5].0);
        // The first wave's group's type only (the flyer is the second wave's, flying
        // in from the air), and then the second's.
        assert!(kinds_spawned.iter().all(|(w, k)| *w == 1 || *k == 0));
        assert!(kinds_spawned.iter().any(|(w, k)| *w == 1 && *k == 1));
        assert!(jets_in_air > 0);
        // Three left and one left, wave 1 complete, its reward; the second wave's
        // countdown names wave 2; won, the game ends.
        assert!(kinds_of.contains(&10) && kinds_of.contains(&9));
        assert!(kinds_of.contains(&107));
        assert!(kinds_of.contains(&201));
        assert!(kinds_of.contains(&207));
        assert_eq!(paid, vec![200]);
        assert!(d.won && team.over);
        assert_eq!(d.cleared, 2);
    }

    /// A life lost mid-wave: the wave's enemies gone at once, its failure, the lost
    /// wave's reward, and the same wave again.
    #[test]
    fn a_failed_wave_starts_again() {
        let mut data = WaveData::parse(SAMPLE);
        let kinds = kinds();
        data.resolve(&kinds);
        let level = Level::default();
        let mut team = Team::default();
        let mut d = Director::new(3);
        let w = test_world(&data, &kinds, &level, &[]);
        let dt = 0.05;
        // Into the first wave, its enemies coming in.
        for _ in 0..(45.0 / dt) as usize {
            d.step(dt, &mut team, &w, |_| true);
        }
        assert_eq!(d.wave, 0);
        assert!(!d.enemies.is_empty());
        // A life lost (revive.rs counts it).
        team.round = 1;
        d.step(dt, &mut team, &w, |_| true);
        d.step(dt, &mut team, &w, |_| true);
        assert!(d.enemies.is_empty() && d.purged == 1);
        assert_eq!(d.messages.last().map(|m| m.1), Some(108));
        assert_eq!(d.paid, (1, 500));
        assert!(d.counting && d.countdown == 30);
        // The countdown names the same wave, and it starts again.
        for _ in 0..(31.0 / dt) as usize {
            d.step(dt, &mut team, &w, |_| true);
        }
        assert!(d.messages.iter().any(|m| m.1 == 106));
        assert_eq!(d.wave, 0);
        assert_eq!(d.cleared, 0);
    }

    /// Into the first wave, its enemies coming in and none killed: with a wall
    /// between them and the player (the far side of a locked door), each goes
    /// unseen a minute (the wave's last few not yet shown) and is taken out, its cost
    /// given back and another brought in; in sight of him, none is.
    #[test]
    fn enemies_stranded_out_of_sight_are_taken_out_and_others_come_in() {
        let mut data = WaveData::parse(SAMPLE);
        let kinds = kinds();
        data.resolve(&kinds);
        let wall = [
            [
                -100.0, -10.0, -20.0, 100.0, -10.0, -20.0, 100.0, 50.0, -20.0,
            ],
            [
                -100.0, -10.0, -20.0, 100.0, 50.0, -20.0, -100.0, 50.0, -20.0,
            ],
        ];
        let walled = Level {
            collision: Some(crate::collision::LevelCollision::from_parts(
                &wall,
                &[],
                4.0,
            )),
            ..Level::default()
        };
        let open = Level::default();
        let dt = 0.05;
        let run = |level: &Level, secs: f32| {
            let mut team = Team::default();
            let mut d = Director::new(3);
            let mut first = Vec::new();
            for frame in 0..(secs / dt) as usize {
                let w = test_world(&data, &kinds, level, &d.enemies);
                d.step(dt, &mut team, &w, |_| true);
                if frame as f32 * dt < 45.0 {
                    first = d.enemies.iter().map(|e| e.id).collect();
                }
            }
            (d, first)
        };
        // In by 45 s; a minute after each came in (with time for its look), it is
        // taken out.
        let (d, first) = run(&walled, 112.0);
        assert!(!first.is_empty());
        assert_eq!(d.wave, 0);
        assert!(
            d.withdrawn as usize >= first.len(),
            "{} of {}",
            d.withdrawn,
            first.len()
        );
        assert!(
            first
                .iter()
                .all(|id| !d.enemies.iter().any(|e| e.id == *id))
        );
        assert!(d.purged as u32 >= d.withdrawn);
        // Others came in for them, the budgets kept.
        assert!(
            d.enemies
                .iter()
                .any(|e| e.id > *first.iter().max().unwrap())
        );
        let living: u32 = d.living.iter().map(|l| l.ids.len() as u32 * 2).sum();
        assert_eq!(d.living_budget, living);
        assert!(d.used <= d.total);
        // In sight, nobody is.
        let (d, _) = run(&open, 112.0);
        assert_eq!((d.withdrawn, d.purged), (0, 0));
    }

    #[test]
    fn those_from_the_air_come_down_and_leapers_leap() {
        let down = Glide::new(
            Vec3::new(0.0, 60.0, 0.0),
            Vec3::new(0.0, 15.0, -50.0),
            false,
        )
        .unwrap();
        assert_eq!(down.arc, 0.0);
        let half = Glide {
            t: down.secs * 0.5,
            ..down
        };
        assert!((half.at() - Vec3::new(0.0, 26.25, -37.5)).length() < 0.01);
        let leap = Glide::new(
            Vec3::new(0.0, 4.0, -300.0),
            Vec3::new(0.0, 4.0, -120.0),
            true,
        )
        .unwrap();
        assert!((leap.arc - 36.0).abs() < 0.01);
        assert!((leap.secs - 4.5).abs() < 0.01);
        let mid = Glide {
            t: leap.secs * 0.5,
            ..leap
        };
        assert!((mid.at() - Vec3::new(0.0, 40.0, -210.0)).length() < 0.01);
        let end = Glide {
            t: leap.secs,
            ..leap
        };
        assert!((end.at() - leap.to).length() < 1e-4);
        assert!(Glide::new(Vec3::ONE, Vec3::ONE, true).is_none());
    }

    #[test]
    fn clusters_out_of_sight_and_near_score_best() {
        let mut data = WaveData::parse(SAMPLE);
        let kinds = kinds();
        data.resolve(&kinds);
        let level = Level::default();
        let w = test_world(&data, &kinds, &level, &[]);
        // (Flat ground: both clusters in sight.) The near one scores 1000/5000 over
        // the far one's 1000/20000.
        let mut d = Director::new(5);
        let near = d.score(0, SIGHT_OUT, &w);
        let far = d.score(1, SIGHT_OUT, &w);
        assert!(near > far && (near - (-100.0 + 1000.0 / 5000.0)).abs() < 0.01);
        assert_eq!(d.score(1, SIGHT_EITHER, &w).round(), 100.0);
        let p = d.find_spawn_point(1, SIGHT_OUT, &kinds.kinds[0], &w);
        assert!(p.is_some_and(|p| p.at.z == -50.0));
        // A jet coming in from the air stays there (its AI flies it); a robot
        // flies down to the nearest point on the ground.
        let air = data.clusters[2].points[0];
        let (at, _, to) = d.placement(air, &kinds.kinds[1], &w);
        assert_eq!((at, to), (air.at, air.at));
        let (_, _, to) = d.placement(air, &kinds.kinds[0], &w);
        assert!((to - Vec3::new(0.0, 0.0, -50.0)).length() < 0.01, "{to}");
    }
}
