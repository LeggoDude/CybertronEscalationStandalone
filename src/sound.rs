//! Sound effects taken from the game by tools/build_sounds.py.
//!
//! The game builds each sound from a SoundCue: a few layers that start at set
//! times, each picking one of several recordings at random with a little random
//! pitch and volume. `assets/sounds/sounds.txt` lists those cues and this module
//! plays them the same way. Without that file the demo simply runs silent.
//!
//! Most cues are played once (`Sfx::play` and friends). A cue with looping layers
//! that a weapon keeps on between two of its events (the Nucleon Charge Rifle's
//! scope hum, say) is held instead (`Sfx::hold`): it plays for as long as its owner
//! asks for it each frame, its layers' volume and pitch following their curves, and
//! stops when he no longer does. A cue played for a Jazz (`Sfx::play_owned`) can
//! have his recordings of one sound cut short (`Sfx::cut`): Vortex's helicopter
//! engine, which its transformation starts, stops as the helicopter stops hovering.
//!
//! Every sound follows the Audio settings (`Volumes`): Master Volume, and the
//! volume of its class (`SoundClass`: effects, dialogue or music, from its cue's
//! mixer category in the game). And the game's mixer (mixer.rs's `Mix`): a cue can
//! turn other mixer categories down while it plays, as an announcer's line turns
//! the effects and music down 9 dB.

use crate::mixer::{Mix, MixerSetup};
use bevy::audio::{AudioSinkPlayback, Decodable, Source, Volume};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Overall loudness of the effects, in decibels. -2.5 dB is 75% of the game's
/// volume (0.0 = as in the game; every 6 dB halves or doubles it).
pub const SFX_VOLUME_DB: f32 = -2.5;

/// Which of the Audio settings' volumes a sound follows besides Master Volume: the
/// game's FX Volume, Dialog Volume or Music Volume. A cue's is its mixer category's
/// (sounds.txt's `category`, its SoundNodeRoot's Category in the game): the DX_
/// categories are dialogue, the MUSIC_ ones music and the rest (SFX_) effects.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SoundClass {
    #[default]
    Effects,
    Dialog,
    Music,
}

impl SoundClass {
    /// The class of a mixer category (DX_DRY, MUSIC_DRY, SFX_WET_COMBAT, ...).
    fn of_category(category: &str) -> Self {
        if category.starts_with("DX_") {
            SoundClass::Dialog
        } else if category.starts_with("MUSIC_") {
            SoundClass::Music
        } else {
            SoundClass::Effects
        }
    }
}

/// The Audio settings' volumes (0 to 1 for their sliders' 0 to 100): Master Volume,
/// the demo's own, scales every sound, and the game's FX Volume, Dialog Volume and
/// Music Volume (all 100 in a new profile) each scale their class's. The menu keeps
/// them in settings.txt.
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct Volumes {
    pub master: f32,
    pub effects: f32,
    pub dialog: f32,
    pub music: f32,
}

impl Default for Volumes {
    fn default() -> Self {
        Volumes {
            master: 1.0,
            effects: 1.0,
            dialog: 1.0,
            music: 1.0,
        }
    }
}

impl Volumes {
    /// What a sound of `class` is scaled by: Master Volume times its class's volume.
    pub fn gain(&self, class: SoundClass) -> f32 {
        let own = match class {
            SoundClass::Effects => self.effects,
            SoundClass::Dialog => self.dialog,
            SoundClass::Music => self.music,
        };
        self.master * own
    }
}

/// How loud a playing sound is before the Audio settings (linear), and its class:
/// `follow_volumes` keeps it at that times their volumes as they change (Bevy sets
/// a sound's volume only as it starts). Sounds whose systems set their volume every
/// frame (the menu music, held cues, the car's engine) go without.
#[derive(Component, Clone, Copy)]
pub struct Loudness {
    pub gain: f32,
    pub class: SoundClass,
}

impl Loudness {
    /// Its volume with the settings at `volumes`.
    pub fn volume(self, volumes: &Volumes) -> Volume {
        Volume::Linear(self.gain * volumes.gain(self.class))
    }
}

/// The Audio settings moved, or the mixer's presets turned categories up or down:
/// the sounds playing follow them (those following a volume curve or fading out as
/// they are cut short follow them as they go).
#[allow(clippy::type_complexity)]
pub fn follow_volumes(
    volumes: Res<Volumes>,
    mix: Res<Mix>,
    mut sinks: Query<(&Loudness, &mut AudioSink, Option<&Voice>), (Without<Fading>, Without<Cut>)>,
) {
    if !volumes.is_changed() && !mix.moved() {
        return;
    }
    for (loudness, mut sink, voice) in &mut sinks {
        let mixed = mix.level(voice.and_then(|v| v.category));
        sink.set_volume(Volume::Linear(
            loudness.gain * volumes.gain(loudness.class) * mixed,
        ));
    }
}

struct Layer {
    time: f32,
    volume: f32,
    pitch: f32,
    pitch_var: (f32, f32),
    volume_var: (f32, f32),
    looping: bool,
    waves: Vec<Handle<AudioSource>>,
    /// Volume (0-1) over the cue's time, as (seconds, volume) keys (music, held
    /// cues, and the one-off cues it shapes: the game plays every layer through its
    /// VolumeCurve, the AI heavy guns' shots cut to 0.4 s).
    envelope: Vec<(f32, f32)>,
    /// Pitch (semitones) over the cue's time, the same way (held cues only).
    pitch_curve: Vec<(f32, f32)>,
    /// Volume (0-1) by how far from the camera the cue plays, as (metres, volume)
    /// keys (the game's VolumeCurve on a cue whose SoundParameter is
    /// SOUND_DISTANCE): a gun's close-up shot near the camera, its distant one
    /// past it. None: the same at any distance.
    distance: Vec<(f32, f32)>,
    /// The chance (0-1) it stays silent, and how late it may start (seconds, at
    /// random between these): the game's ChanceToPlayNone, DelayMin and DelayMax
    /// (held cues only).
    chance_none: f32,
    delay: (f32, f32),
    /// Its speaker levels where they aren't a plain sound's (sounds.txt's `pan`),
    /// as the demo's two speakers take them (`flat_level`), and whether they are
    /// only the player's own (a smart pan: heard by the camera) or always its (2d).
    pan: Option<(f32, bool)>,
}

/// A layer's level (0 or 1) from its speakers' levels in dB (front left and right,
/// centre, back left and right): silent in all five (a car's boost exhaust, for
/// its driver), it isn't heard; in any of them, the demo's two speakers play it as
/// loud as a plain layer (the game's centre and back speakers, which its
/// dialogue and its surround layers go to, folded into them as they are).
fn flat_level(fl: f32, fr: f32, c: f32, bl: f32, br: f32) -> f32 {
    if [fl, fr, c, bl, br].iter().all(|&db| db <= -90.0) {
        0.0
    } else {
        1.0
    }
}

impl Layer {
    /// How loud its speaker levels have it (0-1): as they say where it is heard
    /// flat (a 2d layer always, a smart pan's when it is heard by the camera: the
    /// player's own), otherwise in full (placed round the camera).
    fn flat_gain(&self, by_camera: bool) -> f32 {
        match self.pan {
            Some((level, smart)) if !smart || by_camera => level,
            _ => 1.0,
        }
    }

    /// Its volume (0-1) `t` seconds into the cue.
    fn volume_at(&self, t: f32) -> f32 {
        curve_at(&self.envelope, t, 1.0)
    }

    /// Its pitch change (semitones) `t` seconds into the cue.
    fn pitch_at(&self, t: f32) -> f32 {
        curve_at(&self.pitch_curve, t, 0.0)
    }

    /// Its volume (0-1) `away` metres from the camera.
    fn volume_away(&self, away: f32) -> f32 {
        curve_at(&self.distance, away, 1.0).max(0.0)
    }
}

/// A curve's value at `t` (linear between its keys, held past either end), or
/// `none` without keys.
fn curve_at(keys: &[(f32, f32)], t: f32, none: f32) -> f32 {
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
        return none;
    };
    if t <= first.0 {
        return first.1;
    }
    for pair in keys.windows(2) {
        let ((t0, v0), (t1, v1)) = (pair[0], pair[1]);
        if t <= t1 {
            let k = if t1 > t0 { (t - t0) / (t1 - t0) } else { 1.0 };
            return v0 + (v1 - v0) * k;
        }
    }
    last.1
}

/// Which copy of a cue goes when one more would play than its max (its
/// InstanceLimiting): the oldest, the newest (the new one: it doesn't play), or the
/// farthest from the camera (the game's default; the new one, again, if it is the
/// farthest). See `to_cut`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Limiting {
    Oldest,
    Newest,
    Farthest,
}

struct Cue {
    class: SoundClass,
    volume: f32,
    pitch: f32,
    pitch_var: (f32, f32),
    volume_var: (f32, f32),
    /// The game's MaxConcurrentPlayCount: no more of it plays at once; which copy
    /// goes for a new one is its InstanceLimiting's (`Limiting`).
    max: usize,
    limiting: Limiting,
    /// Full volume up to `dist_min` metres from the camera, fading past it by the
    /// inverse of the distance `rolloff` scales (`distance_db`), silent past
    /// `dist_max`.
    dist_min: f32,
    dist_max: f32,
    rolloff: f32,
    layers: Vec<Layer>,
    /// Music: the whole cue starts over this many seconds in (0 = never). A held
    /// cue goes back to `loop_start` there instead (its LoopStart and LoopEnd).
    loop_end: f32,
    loop_start: f32,
    /// Its mixer category (the bank's `mixer`'s), whose volume its sounds follow as
    /// the presets on turn it down (`Mix`).
    category: Option<u16>,
    /// The mixer preset it turns on as it plays (sounds.txt's `mixer`: an
    /// announcer's line turns the effects and music down 9 dB), and whether only
    /// the local player's own copy does (one played by the camera).
    preset: Option<u16>,
    preset_local: bool,
}

/// Every cue in sounds.txt, with its recordings loaded.
#[derive(Resource, Default)]
pub struct SoundBank {
    cues: HashMap<String, Cue>,
    waves: HashMap<String, Handle<AudioSource>>,
    /// The game's mixer: its categories and presets (sounds.txt's mixer lines).
    mixer: MixerSetup,
}

impl SoundBank {
    /// Reads `<sounds_dir>/sounds.txt`. Recordings are `<sounds_dir>/<name>.ogg`,
    /// loaded through the asset server as `sounds/<name>.ogg`.
    pub fn load(sounds_dir: &Path, asset_server: &AssetServer) -> Self {
        let Ok(text) = std::fs::read_to_string(sounds_dir.join("sounds.txt")) else {
            info!("No sounds found in {:?}: the demo runs silent", sounds_dir);
            return SoundBank::default();
        };
        let bank = SoundBank::parse(&text, |name| {
            asset_server.load(format!("sounds/{name}.ogg"))
        });
        info!("Loaded {} sound cues", bank.cues.len());
        bank
    }

    /// The cues of a sounds.txt, each recording handed out by `load` (by name).
    fn parse(text: &str, mut load: impl FnMut(&str) -> Handle<AudioSource>) -> Self {
        let mut bank = SoundBank::default();
        let mut current: Option<String> = None;
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let kind = parts.next().unwrap_or("");
            let rest: Vec<&str> = parts.collect();
            let f = |i: usize| {
                rest.get(i)
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(0.0)
            };
            // (The mixer's categories and presets, before the cues.)
            if bank.mixer.parse_line(kind, &rest) {
                continue;
            }
            match kind {
                "cue" if rest.len() >= 10 => {
                    let name = rest[0].to_string();
                    bank.cues.insert(
                        name.clone(),
                        Cue {
                            class: SoundClass::Effects,
                            volume: f(1),
                            pitch: f(2),
                            pitch_var: (f(3), f(4)),
                            volume_var: (f(5), f(6)),
                            max: (f(7) as usize).max(1),
                            dist_min: f(8),
                            dist_max: f(9).max(f(8) + 1.0),
                            // (Older lists have none: the game's default, 1.)
                            rolloff: rest
                                .get(10)
                                .and_then(|v| v.parse::<f32>().ok())
                                .unwrap_or(1.0)
                                .max(0.0),
                            // (Older lists have none: as the demo cut them before,
                            // the oldest.)
                            limiting: match rest.get(11).copied() {
                                Some("newest") => Limiting::Newest,
                                Some("farthest") => Limiting::Farthest,
                                _ => Limiting::Oldest,
                            },
                            layers: Vec::new(),
                            loop_end: 0.0,
                            loop_start: 0.0,
                            category: None,
                            preset: None,
                            preset_local: false,
                        },
                    );
                    current = Some(name);
                }
                "category" if !rest.is_empty() => {
                    if let Some(cue) = current.as_ref().and_then(|c| bank.cues.get_mut(c)) {
                        cue.class = SoundClass::of_category(rest[0]);
                        cue.category = bank.mixer.category(rest[0]);
                    }
                }
                "mixer" if !rest.is_empty() => {
                    if let Some(cue) = current.as_ref().and_then(|c| bank.cues.get_mut(c)) {
                        cue.preset = bank.mixer.preset(rest[0]);
                        cue.preset_local = rest.get(1) == Some(&"local");
                    }
                }
                "layer" if rest.len() >= 9 => {
                    let Some(cue) = current.as_ref().and_then(|c| bank.cues.get_mut(c)) else {
                        continue;
                    };
                    let mut waves = Vec::new();
                    for name in &rest[8..] {
                        let handle = bank
                            .waves
                            .entry(name.to_string())
                            .or_insert_with(|| load(name))
                            .clone();
                        waves.push(handle);
                    }
                    cue.layers.push(Layer {
                        time: f(0),
                        volume: f(1),
                        pitch: f(2),
                        pitch_var: (f(3), f(4)),
                        volume_var: (f(5), f(6)),
                        looping: f(7) != 0.0,
                        waves,
                        envelope: Vec::new(),
                        pitch_curve: Vec::new(),
                        distance: Vec::new(),
                        chance_none: 0.0,
                        delay: (0.0, 0.0),
                        pan: None,
                    });
                }
                "envelope" | "pitch" | "distance" if rest.len() >= 3 => {
                    let Some(cue) = current.as_ref().and_then(|c| bank.cues.get_mut(c)) else {
                        continue;
                    };
                    let Some(layer) = cue.layers.get_mut(f(0) as usize) else {
                        continue;
                    };
                    let keys = (1..rest.len() - 1)
                        .step_by(2)
                        .map(|i| (f(i), f(i + 1)))
                        .collect();
                    match kind {
                        "envelope" => layer.envelope = keys,
                        "pitch" => layer.pitch_curve = keys,
                        _ => layer.distance = keys,
                    }
                }
                // pan <layer> <front left> <front right> <centre> <back left>
                // <back right> (dB) <2d|smart>
                "pan" if rest.len() >= 7 => {
                    let Some(cue) = current.as_ref().and_then(|c| bank.cues.get_mut(c)) else {
                        continue;
                    };
                    if let Some(layer) = cue.layers.get_mut(f(0) as usize) {
                        layer.pan =
                            Some((flat_level(f(1), f(2), f(3), f(4), f(5)), rest[6] != "2d"));
                    }
                }
                "random" if rest.len() >= 4 => {
                    let Some(cue) = current.as_ref().and_then(|c| bank.cues.get_mut(c)) else {
                        continue;
                    };
                    if let Some(layer) = cue.layers.get_mut(f(0) as usize) {
                        layer.chance_none = (f(1) / 100.0).clamp(0.0, 1.0);
                        layer.delay = (f(2).max(0.0), f(3).max(f(2)).max(0.0));
                    }
                }
                "loop_end" | "loop_start" if !rest.is_empty() => {
                    if let Some(cue) = current.as_ref().and_then(|c| bank.cues.get_mut(c)) {
                        if kind == "loop_end" {
                            cue.loop_end = f(0);
                        } else {
                            cue.loop_start = f(0).max(0.0);
                        }
                    }
                }
                _ => {}
            }
        }
        bank
    }

    /// Whether there is a cue of that name.
    pub fn has_cue(&self, name: &str) -> bool {
        self.cues.contains_key(name)
    }

    /// Whether a cue loops (one of its layers does: it plays on until let go, so it
    /// is held, `Sfx::hold`, rather than played).
    pub fn loops(&self, name: &str) -> bool {
        self.cues
            .get(name)
            .is_some_and(|c| c.layers.iter().any(|l| l.looping))
    }

    /// Whether a cue loops back over one-off layers of its own (its loop playing
    /// them again each time round: the Insecticons' hover, VEH_INSECTICON_FLYING_LP,
    /// its wings' buzzes and rustles every 0.31 s, some passes silent).
    pub fn loops_one_offs(&self, name: &str) -> bool {
        self.cues.get(name).is_some_and(|c| {
            c.loop_end > c.loop_start
                && c.layers
                    .iter()
                    .any(|l| !l.looping && l.time >= c.loop_start && !l.waves.is_empty())
        })
    }

    /// A cue's class (effects for a cue the bank hasn't).
    pub fn class(&self, name: &str) -> SoundClass {
        self.cues.get(name).map_or(SoundClass::Effects, |c| c.class)
    }

    /// A cue's mixer category (for `Mix::level`; none for a cue the bank hasn't).
    pub fn category(&self, name: &str) -> Option<u16> {
        self.cues.get(name).and_then(|c| c.category)
    }

    /// A recording by name (for the car's engine loops).
    pub fn wave(&self, name: &str) -> Option<Handle<AudioSource>> {
        self.waves.get(name).cloned()
    }

    /// A music cue, for the caller to play itself (its layers start at set times,
    /// some loop and fade in, and the whole cue may start over).
    pub fn music(&self, name: &str) -> Option<Music> {
        let cue = self.cues.get(name)?;
        Some(Music {
            loop_end: cue.loop_end,
            class: cue.class,
            layers: cue
                .layers
                .iter()
                .filter_map(|l| {
                    Some(MusicLayer {
                        time: l.time,
                        volume_db: SFX_VOLUME_DB
                            + cue.volume
                            + l.volume
                            + 20.0 * l.flat_gain(true).max(1e-5).log10(),
                        looping: l.looping,
                        wave: l.waves.first()?.clone(),
                        envelope: l.envelope.clone(),
                    })
                })
                .collect(),
        })
    }
}

/// A music cue (see [`SoundBank::music`]).
pub struct Music {
    /// The cue starts over this many seconds in (0 = never).
    pub loop_end: f32,
    /// The Audio settings' volume it follows (the menu music's is Music Volume, the
    /// garage's ambience's FX Volume).
    pub class: SoundClass,
    pub layers: Vec<MusicLayer>,
}

pub struct MusicLayer {
    /// Seconds into the cue when this layer starts.
    pub time: f32,
    pub volume_db: f32,
    pub looping: bool,
    pub wave: Handle<AudioSource>,
    /// Volume (0-1) over the cue's time as (seconds, volume) keys; empty = full.
    pub envelope: Vec<(f32, f32)>,
}

impl MusicLayer {
    /// The envelope's volume `t` seconds into the cue (linear between keys).
    pub fn envelope_at(&self, t: f32) -> f32 {
        curve_at(&self.envelope, t, 1.0)
    }
}

struct Request {
    cue: &'static str,
    at: Option<Vec3>,
    delay: f32,
    time_scale: f32,
    /// The Jazz it is his (`Sfx::play_owned`), for `Sfx::cut`.
    owner: Option<u64>,
}

struct Pending {
    start: f32,
    /// Where it plays (None: by the camera).
    at: Option<Vec3>,
    cue: &'static str,
    /// Its layer in the cue, and whether that has a volume curve to follow.
    layer: usize,
    fades: bool,
    handle: Handle<AudioSource>,
    db: f32,
    speed: f32,
    owner: Option<u64>,
    /// Whether it is the first of its cue's recordings to start (as it does, the
    /// cue turns its mixer preset on).
    trigger: bool,
}

/// Queue of sounds to play. Systems call `play`/`play_at`; `play_sounds` does the rest.
#[derive(Resource, Default)]
pub struct Sfx {
    requests: Vec<Request>,
    pending: Vec<Pending>,
    seed: u32,
    /// While the game's systems run (`begin_capture` to `end_capture`), the sounds
    /// asked for near the camera are this PC's Jazz's own: they are kept to send
    /// to the other players (cue, delay, time scale).
    capturing: bool,
    captured: Vec<(&'static str, f32, f32)>,
    /// Cue names received from other players, kept for good (one copy of each).
    names: HashMap<String, &'static str>,
    /// Held cues asked for this frame, and those playing.
    hold_requests: Vec<HoldRequest>,
    held: Vec<Held>,
    /// Recordings to cut short this frame: (owner, recording).
    cuts: Vec<(u64, &'static str)>,
}

/// A held cue asked for this frame: its owner, the cue, where (None: by the camera),
/// how long it fades out once let go, whether only its one-off layers play
/// (`Sfx::hold_one_offs`) and whether it plays through without its loop
/// (`Sfx::hold_through`).
struct HoldRequest {
    owner: u64,
    cue: &'static str,
    at: Option<Vec3>,
    fade: f32,
    one_offs: bool,
    through: bool,
}

impl HoldRequest {
    fn new(owner: u64, cue: &'static str, at: Option<Vec3>, fade: f32) -> Self {
        HoldRequest {
            owner,
            cue,
            at,
            fade,
            one_offs: false,
            through: false,
        }
    }
}

/// A held cue playing (`Sfx::hold`).
struct Held {
    owner: u64,
    cue: &'static str,
    /// Where it plays (None: by the camera), as last held, and whether it was
    /// held this frame.
    at: Option<Vec3>,
    wanted: bool,
    /// When it started, and its pass through the cue: when that began and the
    /// cue's time it began from (the cue goes back to LoopStart at LoopEnd).
    started: f32,
    pass_at: f32,
    pass_from: f32,
    /// Its random volume (dB) and pitch (semitones), picked as it started.
    db: f32,
    semitones: f32,
    voices: Vec<HeldVoice>,
    /// When it was let go: it fades out over `fade` seconds and stops.
    released: Option<f32>,
    fade: f32,
    /// Only its one-off layers (Sfx::hold_one_offs).
    one_offs: bool,
    /// Played through, its LoopStart and LoopEnd left alone (Sfx::hold_through).
    through: bool,
}

/// One recording of a held cue: its layer, when it starts, the entity playing it
/// once it has, and its own random volume and pitch.
struct HeldVoice {
    layer: usize,
    start: f32,
    entity: Option<Entity>,
    handle: Handle<AudioSource>,
    db: f32,
    semitones: f32,
}

/// Marks the entities playing held cues' recordings.
#[derive(Component)]
pub struct HeldSound;

/// A let-go held cue fades out over this long (rather than cutting off), as does a
/// recording cut short (`Sfx::cut`).
const HELD_FADE_SECS: f32 = 0.1;
const CUT_FADE_SECS: f32 = 0.1;

impl Sfx {
    /// Keeps a looping cue playing for `owner` (any number that tells its owners
    /// apart: a Jazz's entity bits), heard from `at` (None: by the camera). Ask for
    /// it every frame it should play; it starts the first frame it is asked for and
    /// fades out the first frame it isn't. Held cues aren't sent to the other
    /// players (their PCs hold them for their copy of the owner themselves).
    pub fn hold(&mut self, owner: u64, cue: &'static str, at: Option<Vec3>) {
        self.hold_requests
            .push(HoldRequest::new(owner, cue, at, HELD_FADE_SECS));
    }

    /// As `hold`, the cue played through on its own time, its looping layers
    /// looping and each one-off layer once at its time, its layers' volume and
    /// pitch curves over the seconds it has been held, without going back to its
    /// LoopStart at its LoopEnd: a vehicle's boost loops (BoostSounds' BoostLoops),
    /// whose curves and one-offs run over a minute of boosting (a truck's gears
    /// changing at 7.5, 16, 27 and 38 s, with a hiss of air at each), where their
    /// LoopEnd (2.4 s for a truck's) would go back before the first.
    pub fn hold_through(&mut self, owner: u64, cue: &'static str, at: Option<Vec3>) {
        self.hold_requests.push(HoldRequest {
            through: true,
            ..HoldRequest::new(owner, cue, at, HELD_FADE_SECS)
        });
    }

    /// As `hold`, fading out over `fade` seconds once it is no longer held (the
    /// weapon effect's LoopingFadeOutTime).
    pub fn hold_fading(&mut self, owner: u64, cue: &'static str, at: Option<Vec3>, fade: f32) {
        self.hold_requests
            .push(HoldRequest::new(owner, cue, at, fade.max(0.02)));
    }

    /// As `hold`, its one-off layers alone, again with each pass of its loop (those
    /// a cue plays round its looping ones, which an engine plays itself: the
    /// Insecticons' wings' buzzes and rustles over their hover's loops).
    pub fn hold_one_offs(&mut self, owner: u64, cue: &'static str, at: Option<Vec3>) {
        self.hold_requests.push(HoldRequest {
            one_offs: true,
            ..HoldRequest::new(owner, cue, at, HELD_FADE_SECS)
        });
    }

    /// A sound made by Jazz himself (always near the camera).
    pub fn play(&mut self, cue: &'static str) {
        self.request(cue, None, 0.0, 1.0, None);
    }

    /// A sound somewhere in the world; it fades with distance from the camera.
    pub fn play_at(&mut self, cue: &'static str, at: Vec3) {
        self.request(cue, Some(at), 0.0, 1.0, None);
    }

    /// A player's sound: `at` None for the Jazz this machine plays (near the camera),
    /// or where another player is.
    pub fn play_from(&mut self, cue: &'static str, at: Option<Vec3>) {
        self.request(cue, at, 0.0, 1.0, None);
    }

    /// A player's sound starting `delay` seconds from now (see `play_from`).
    pub fn play_after_from(&mut self, cue: &'static str, delay: f32, at: Option<Vec3>) {
        self.request(cue, at, delay, 1.0, None);
    }

    /// A player's sound starting `delay` seconds from now, its layers' start times
    /// scaled (when an animation plays faster than in the game; see `play_from`).
    pub fn play_scaled_after_from(
        &mut self,
        cue: &'static str,
        delay: f32,
        time_scale: f32,
        at: Option<Vec3>,
    ) {
        self.request(cue, at, delay, time_scale, None);
    }

    /// As `play_scaled_after_from`, a Jazz's own (`owner`: any number that tells
    /// the Jazzes apart, his entity's bits), so that its recordings can be cut
    /// short (`cut`).
    pub fn play_owned(
        &mut self,
        owner: u64,
        cue: &'static str,
        delay: f32,
        time_scale: f32,
        at: Option<Vec3>,
    ) {
        self.request(cue, at, delay, time_scale, Some(owner));
    }

    /// Cuts short `owner`'s recordings of `wave` (sounds.txt's name) from the cues
    /// played for him (`play_owned`, `play_remote`): those yet to start never do,
    /// those playing fade out at once (over CUT_FADE_SECS).
    pub fn cut(&mut self, owner: u64, wave: &'static str) {
        self.cuts.push((owner, wave));
    }

    /// Cuts short every recording of `owner`'s (as `cut`, whichever they are).
    pub fn silence(&mut self, owner: u64) {
        self.cuts.push((owner, ""));
    }

    /// A sound another player's Jazz (`owner`, as for `play_owned`) made: a cue
    /// name from his PC (unknown names are ignored), heard where he is.
    pub fn play_remote(
        &mut self,
        bank: &SoundBank,
        cue: &str,
        at: Vec3,
        delay: f32,
        time_scale: f32,
        owner: u64,
    ) {
        if !bank.has_cue(cue) {
            return;
        }
        let name = match self.names.get(cue) {
            Some(name) => *name,
            None => {
                // (Only cues the bank has, so this keeps at most one copy of each.)
                let name: &'static str = Box::leak(cue.to_string().into_boxed_str());
                self.names.insert(cue.to_string(), name);
                name
            }
        };
        self.request(
            name,
            Some(at),
            delay.clamp(0.0, 10.0),
            time_scale.clamp(0.1, 10.0),
            Some(owner),
        );
    }

    /// Starts keeping this PC's Jazz's sounds (see `captured`).
    pub fn begin_capture(&mut self) {
        self.capturing = true;
        self.captured.clear();
    }

    /// Stops keeping them, and hands over those kept.
    pub fn end_capture(&mut self) -> Vec<(&'static str, f32, f32)> {
        self.capturing = false;
        std::mem::take(&mut self.captured)
    }

    fn request(
        &mut self,
        cue: &'static str,
        at: Option<Vec3>,
        delay: f32,
        time_scale: f32,
        owner: Option<u64>,
    ) {
        // (A gun with no sound for something names none.)
        if cue.is_empty() {
            return;
        }
        if self.capturing && at.is_none() {
            self.captured.push((cue, delay, time_scale));
        }
        self.requests.push(Request {
            cue,
            at,
            delay,
            time_scale,
            owner,
        });
    }

    fn random(&mut self) -> f32 {
        // xorshift32
        let mut x = self.seed.wrapping_add(0x9E37_79B9) | 1;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, r: (f32, f32)) -> f32 {
        r.0 + (r.1 - r.0) * self.random()
    }
}

/// One playing recording, and the Jazz it is his, if it was played for one.
#[derive(Component)]
pub struct Voice {
    cue: &'static str,
    /// Its layer in the cue.
    layer: usize,
    born: f32,
    owner: Option<u64>,
    /// Where it plays (None: by the camera).
    at: Option<Vec3>,
    /// The recording (its file, however it plays: see `Decoded`).
    wave: Handle<AudioSource>,
    /// Its cue's mixer category (whose volume, `Mix::level`, it follows).
    category: Option<u16>,
}

/// A recording decoded into memory (its samples, the channels interleaved), which
/// plays without decoding its file again. Bevy plays an `AudioSource` by opening
/// its Ogg Vorbis afresh each time, and opening one (its setup header's codebooks)
/// takes about a millisecond on the frame that starts it: a big wave's footsteps,
/// shots and blasts, several new sounds a frame, cost 3 to 5 ms a frame that way.
#[derive(Asset, TypePath, Clone)]
pub struct Pcm {
    channels: u16,
    rate: u32,
    samples: Arc<[i16]>,
}

/// A [`Pcm`] as it plays.
pub struct PcmDecoder {
    pcm: Pcm,
    at: usize,
}

impl Iterator for PcmDecoder {
    type Item = i16;

    fn next(&mut self) -> Option<i16> {
        let sample = self.pcm.samples.get(self.at).copied();
        self.at += 1;
        sample
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.pcm.samples.len().saturating_sub(self.at);
        (left, Some(left))
    }
}

impl Source for PcmDecoder {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.pcm.channels
    }

    fn sample_rate(&self) -> u32 {
        self.pcm.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        let frames = self.pcm.samples.len() / usize::from(self.pcm.channels.max(1));
        Some(Duration::from_secs_f64(
            frames as f64 / f64::from(self.pcm.rate.max(1)),
        ))
    }
}

impl Decodable for Pcm {
    type DecoderItem = i16;
    type Decoder = PcmDecoder;

    fn decoder(&self) -> PcmDecoder {
        PcmDecoder {
            pcm: self.clone(),
            at: 0,
        }
    }
}

/// The largest file decoded into memory (bytes of Ogg Vorbis, some 10 s of sound):
/// the longer ones (music, ambience, the long loops) play from their files, as
/// they start seldom.
const DECODE_MAX_BYTES: usize = 160 * 1024;

/// The recordings decoded into memory, or being decoded off the main thread, by
/// their file: each one is decoded the first time it plays (that time it plays from
/// its file), and from memory from then on (see [`Pcm`]).
#[derive(Resource, Default)]
pub struct Decoded(HashMap<AssetId<AudioSource>, Decoding>);

enum Decoding {
    Pending(Task<Option<Pcm>>),
    Ready(Handle<Pcm>),
    /// Too long to keep in memory, or not decodable.
    File,
}

/// What a recording plays from.
enum Playable {
    Memory(Handle<Pcm>),
    File(Handle<AudioSource>),
}

impl Playable {
    /// Its player.
    fn spawn(self, commands: &mut Commands, bundle: impl Bundle) -> Entity {
        match self {
            Playable::Memory(pcm) => commands.spawn((AudioPlayer::<Pcm>(pcm), bundle)).id(),
            Playable::File(wave) => commands.spawn((AudioPlayer::new(wave), bundle)).id(),
        }
    }
}

impl Decoded {
    /// What `wave` plays from: its samples in memory once they are decoded; its file
    /// meanwhile, its decoding started the first time (if it is short enough).
    fn playable(&mut self, wave: &Handle<AudioSource>, files: &Assets<AudioSource>) -> Playable {
        match self.0.get(&wave.id()) {
            Some(Decoding::Ready(pcm)) => return Playable::Memory(pcm.clone()),
            Some(_) => return Playable::File(wave.clone()),
            None => {}
        }
        // (Only once its file has loaded.)
        if let Some(file) = files.get(wave) {
            let state = if file.bytes.len() <= DECODE_MAX_BYTES {
                let file = file.clone();
                Decoding::Pending(AsyncComputeTaskPool::get().spawn(async move { decode(&file) }))
            } else {
                Decoding::File
            };
            self.0.insert(wave.id(), state);
        }
        Playable::File(wave.clone())
    }
}

/// A file's samples, all of them (None if it can't be read as sound).
fn decode(file: &AudioSource) -> Option<Pcm> {
    let decoder = std::panic::catch_unwind(|| file.decoder()).ok()?;
    let (channels, rate) = (decoder.channels(), decoder.sample_rate());
    let samples: Vec<i16> = decoder.collect();
    (channels > 0 && rate > 0 && !samples.is_empty()).then(|| Pcm {
        channels,
        rate,
        samples: samples.into(),
    })
}

/// Takes up the recordings decoded off the main thread.
pub fn finish_decoding(mut decoded: ResMut<Decoded>, mut pcms: ResMut<Assets<Pcm>>) {
    for state in decoded.0.values_mut() {
        let Decoding::Pending(task) = state else {
            continue;
        };
        if !task.is_finished() {
            continue;
        }
        *state = match block_on(poll_once(task)).flatten() {
            Some(pcm) => Decoding::Ready(pcms.add(pcm)),
            None => Decoding::File,
        };
    }
}

/// A one-off recording whose layer has a volume curve (sounds.txt's `envelope`,
/// written by tools/build_sounds.py for the one-off cues whose curves shape them):
/// `play_sounds` keeps it at the curve's volume, and stops it once the curve has
/// faded it out for good.
#[derive(Component)]
pub struct Fading;

/// A recording cut short (`Sfx::cut`) at `at`, at `level` (its share of its
/// loudness then, its volume curve's and its mixer category's): it fades out from
/// there over CUT_FADE_SECS and stops.
#[derive(Component)]
pub struct Cut {
    at: f32,
    level: f32,
}

/// The recordings following their volume curves, and their sinks.
type FadingVoices<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Voice,
        &'static Loudness,
        &'static mut AudioSink,
    ),
    (With<Fading>, Without<HeldSound>, Without<Cut>),
>;

/// The recordings cut short, fading out, and their sinks (once they have them).
type CutVoices<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Loudness,
        &'static Cut,
        Option<&'static mut AudioSink>,
    ),
    Without<HeldSound>,
>;

/// How far a sound plays from the camera (metres): none for one without a place
/// (the local player's own, heard where he is), as near as can be.
fn away_from(at: Option<Vec3>, ear: Option<Vec3>) -> f32 {
    match (at, ear) {
        (Some(at), Some(ear)) => at.distance(ear),
        _ => 0.0,
    }
}

/// Where a sound at `at` is heard from: the game camera nearest it (with two
/// players on one screen, each hears what goes on round him as loud as he would
/// alone), the first one for a sound without a place, None with no camera.
fn nearest_ear(at: Option<Vec3>, ears: &[Vec3]) -> Option<Vec3> {
    match at {
        Some(at) => ears
            .iter()
            .copied()
            .min_by(|a, b| a.distance_squared(at).total_cmp(&b.distance_squared(at))),
        None => ears.first().copied(),
    }
}

/// A layer's distance curve at or below this (-60 dB) leaves it out.
const SILENT_LEVEL: f32 = 1e-3;

/// How far a cue is turned down (dB) heard at `ear` from `at` (None: by the
/// camera): as the game fades it with distance, at full volume out to its near
/// distance and past it by the inverse of the distance, scaled by its rolloff factor
/// (its SoundNodeRoot's RolloffFactor, as FMOD's inverse rolloff: near / (near +
/// rolloff * (distance - near)); 0 doesn't fade it at all, 5 or 10 fade it fast).
/// None past the far one: it isn't heard. (It faded in a straight line from the near
/// distance to the far one, which kept the enemies' guns loud far past their near
/// distances: the Heavy Gunners' minigun loop, near at 10 m, at nine tenths at 30 m,
/// a third in the game.)
fn distance_db(cue: &Cue, at: Option<Vec3>, ear: Option<Vec3>) -> Option<f32> {
    let (Some(at), Some(ear)) = (at, ear) else {
        return Some(0.0);
    };
    let d = at.distance(ear);
    if d >= cue.dist_max {
        return None;
    }
    if d <= cue.dist_min {
        return Some(0.0);
    }
    let near = cue.dist_min.max(0.01);
    let gain = near / (near + cue.rolloff * (d - near));
    Some(20.0 * gain.max(1e-4).log10())
}

/// The layers of cues playing (by cue and layer): each copy's start, distance from
/// the camera and entity.
type Playing = HashMap<(&'static str, usize), Vec<(f32, f32, Entity)>>;

/// Which of the copies of a cue's layer playing (each one's start and distance from
/// the camera) its limiting cuts when one more, `away` from the camera, would be one
/// too many: Some(the copy's index) for the new one to take its place, None for the
/// new one not to play. Oldest: the one that started first. Newest: the new one.
/// Farthest: the farthest from the camera (the new one if it is farther than all of
/// them; of those as far, the oldest).
fn to_cut(limiting: Limiting, playing: &[(f32, f32)], away: f32) -> Option<usize> {
    let oldest = |ids: &mut dyn Iterator<Item = usize>| {
        ids.min_by(|&a, &b| playing[a].0.total_cmp(&playing[b].0))
    };
    match limiting {
        Limiting::Oldest => oldest(&mut (0..playing.len())),
        Limiting::Newest => None,
        Limiting::Farthest => {
            let far = playing
                .iter()
                .map(|c| c.1)
                .fold(f32::NEG_INFINITY, f32::max);
            if away > far {
                return None;
            }
            oldest(&mut (0..playing.len()).filter(|&i| playing[i].1 >= far))
        }
    }
}

/// The match's `play_sounds` (main.rs), for other systems to order against: the
/// system itself runs in the menu's chain too, so it can't be ordered against by
/// name (Bevy refuses an ordering against a system added twice).
#[derive(SystemSet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PlaySounds;

/// Turns queued requests into playing sounds, and keeps the held cues going.
#[allow(clippy::too_many_arguments)]
pub fn play_sounds(
    mut commands: Commands,
    time: Res<Time>,
    bank: Option<Res<SoundBank>>,
    mut sfx: ResMut<Sfx>,
    // (The game cameras, one a player: the HUD has 3D cameras of its own.)
    listener: Query<&GlobalTransform, (With<Camera3d>, With<crate::OrbitCamera>)>,
    voices: Query<(Entity, &Voice, Has<Cut>)>,
    volumes: Res<Volumes>,
    mut sinks: Query<&mut AudioSink, With<HeldSound>>,
    mut fading: FadingVoices,
    mut cut: CutVoices,
    (files, mut decoded): (Option<Res<Assets<AudioSource>>>, Option<ResMut<Decoded>>),
    mut mix: ResMut<Mix>,
) {
    let now = time.elapsed_secs();
    // What a recording plays from (see `Decoded`; its file where there is none).
    let mut playable = |wave: &Handle<AudioSource>| match (decoded.as_deref_mut(), files.as_deref())
    {
        (Some(decoded), Some(files)) => decoded.playable(wave, files),
        _ => Playable::File(wave.clone()),
    };
    let Some(bank) = bank else {
        sfx.requests.clear();
        sfx.hold_requests.clear();
        sfx.cuts.clear();
        return;
    };
    let ears: Vec<Vec3> = listener.iter().map(|t| t.translation()).collect();
    let ear = |at: Option<Vec3>| nearest_ear(at, &ears);

    let requests = std::mem::take(&mut sfx.requests);
    for req in requests {
        let Some(cue) = bank.cues.get(req.cue) else {
            continue;
        };
        let Some(atten) = distance_db(cue, req.at, ear(req.at)) else {
            continue;
        };
        // (How far from the camera, for its layers' distance curves.)
        let how_far = away_from(req.at, ear(req.at));
        let cue_db = cue.volume + sfx.range(cue.volume_var);
        let cue_pitch = cue.pitch + sfx.range(cue.pitch_var);
        let first = sfx.pending.len();
        for (i, layer) in cue.layers.iter().enumerate() {
            // Looping layers belong to sounds the demo drives itself (the engine).
            if layer.looping || layer.waves.is_empty() {
                continue;
            }
            // (One its distance curve silences there doesn't play, nor one its
            // speakers' levels silence: a car's boost exhaust for its driver.)
            let near = layer.volume_away(how_far) * layer.flat_gain(req.at.is_none());
            if near < SILENT_LEVEL {
                continue;
            }
            let pick =
                ((sfx.random() * layer.waves.len() as f32) as usize).min(layer.waves.len() - 1);
            let db = SFX_VOLUME_DB
                + cue_db
                + layer.volume
                + sfx.range(layer.volume_var)
                + atten
                + 20.0 * near.log10();
            let semitones = cue_pitch + layer.pitch + sfx.range(layer.pitch_var);
            sfx.pending.push(Pending {
                start: now + req.delay + layer.time * req.time_scale,
                at: req.at,
                cue: req.cue,
                layer: i,
                fades: !layer.envelope.is_empty(),
                handle: layer.waves[pick].clone(),
                db,
                speed: 2f32.powf(semitones / 12.0),
                owner: req.owner,
                trigger: false,
            });
        }
        // (Its first recording to start turns its mixer preset on.)
        if cue.preset.is_some()
            && let Some(p) = sfx.pending[first..]
                .iter_mut()
                .min_by(|a, b| a.start.total_cmp(&b.start))
        {
            p.trigger = true;
        }
    }

    // Recordings cut short: those yet to start are dropped, those playing fade out
    // from where their volume is now.
    let cuts = std::mem::take(&mut sfx.cuts);
    if !cuts.is_empty() {
        // (No wave: every recording of the owner's, `silence`.)
        let cuts: Vec<(u64, Option<Handle<AudioSource>>)> = cuts
            .into_iter()
            .filter_map(|(owner, wave)| {
                if wave.is_empty() {
                    return Some((owner, None));
                }
                Some((owner, Some(bank.waves.get(wave)?.clone())))
            })
            .collect();
        let hit = |owner: Option<u64>, handle: &Handle<AudioSource>| {
            cuts.iter()
                .any(|(o, h)| owner == Some(*o) && h.as_ref().is_none_or(|h| h == handle))
        };
        sfx.pending.retain(|p| !hit(p.owner, &p.handle));
        for (entity, voice, was_cut) in &voices {
            if was_cut || !hit(voice.owner, &voice.wave) {
                continue;
            }
            let level = bank
                .cues
                .get(voice.cue)
                .and_then(|c| c.layers.get(voice.layer))
                .map_or(1.0, |l| l.volume_at(l.time + (now - voice.born)))
                * mix.level(voice.category);
            commands.entity(entity).insert(Cut { at: now, level });
        }
    }

    // The mixer's presets: those whose sounds play stay on (a cue's own only for the
    // local player's copy, if it says so; a held cue until it is let go), the
    // others go off as they say, and the categories' volumes follow them (and the
    // presets categories turn on with many of their sounds playing: each recording
    // playing is one).
    let mut on = vec![false; bank.mixer.presets()];
    let mut counts = vec![0usize; bank.mixer.categories()];
    let turned_on = |cue: Option<&Cue>, at: Option<Vec3>, on: &mut [bool]| {
        if let Some(c) = cue
            && let Some(p) = c.preset
            && (!c.preset_local || at.is_none())
            && let Some(o) = on.get_mut(p as usize)
        {
            *o = true;
        }
    };
    for (_, voice, was_cut) in &voices {
        if was_cut {
            continue;
        }
        if let Some(n) = voice.category.and_then(|c| counts.get_mut(c as usize)) {
            *n += 1;
        }
        turned_on(bank.cues.get(voice.cue), voice.at, &mut on);
    }
    for h in sfx.held.iter().filter(|h| h.released.is_none()) {
        turned_on(bank.cues.get(h.cue), h.at, &mut on);
    }
    mix.step(&bank.mixer, now, time.delta_secs(), &on, &counts);

    // Start everything that is due.
    let mut live: Playing = HashMap::new();
    let away = |at: Option<Vec3>| away_from(at, ear(at));
    let mut started = false;
    let pending = std::mem::take(&mut sfx.pending);
    for p in pending {
        if p.start > now {
            sfx.pending.push(p);
            continue;
        }
        if !started {
            // (Those cut short are on their way out: they don't count.)
            for (entity, voice, was_cut) in &voices {
                if !was_cut {
                    live.entry((voice.cue, voice.layer)).or_default().push((
                        voice.born,
                        away(voice.at),
                        entity,
                    ));
                }
            }
            started = true;
        }
        let cue = bank.cues.get(p.cue);
        let list = live.entry((p.cue, p.layer)).or_default();
        if list.len() >= cue.map_or(16, |c| c.max) {
            // One too many: one of them goes, as the cue's limiting has it.
            let copies: Vec<(f32, f32)> = list.iter().map(|&(born, d, _)| (born, d)).collect();
            let limiting = cue.map_or(Limiting::Oldest, |c| c.limiting);
            let Some(gone) = to_cut(limiting, &copies, away(p.at)) else {
                continue;
            };
            let (.., gone) = list.remove(gone);
            commands.entity(gone).try_despawn();
        }
        let loudness = Loudness {
            gain: 10f32.powf(p.db / 20.0),
            class: cue.map_or(SoundClass::Effects, |c| c.class),
        };
        let category = cue.and_then(|c| c.category);
        // (One with a volume curve starts at the curve's volume.)
        let level = cue
            .and_then(|c| c.layers.get(p.layer))
            .filter(|_| p.fades)
            .map_or(1.0, |l| l.volume_at(l.time));
        // (Its cue's preset on as it starts.)
        if p.trigger
            && let Some(c) = cue
            && let Some(preset) = c.preset
            && (!c.preset_local || p.at.is_none())
        {
            mix.turn_on(&bank.mixer, preset, now);
        }
        let voice = playable(&p.handle).spawn(
            &mut commands,
            (
                PlaybackSettings::DESPAWN
                    .with_volume(Volume::Linear(
                        loudness.gain * level * volumes.gain(loudness.class) * mix.level(category),
                    ))
                    .with_speed(p.speed),
                Voice {
                    cue: p.cue,
                    layer: p.layer,
                    born: now,
                    owner: p.owner,
                    at: p.at,
                    wave: p.handle,
                    category,
                },
                loudness,
            ),
        );
        if p.fades {
            commands.entity(voice).insert(Fading);
        }
        list.push((now, away(p.at), voice));
    }

    // Those cut short fade out, and stop.
    for (entity, loudness, c, sink) in &mut cut {
        let fade = 1.0 - (now - c.at) / CUT_FADE_SECS;
        if fade <= 0.0 {
            commands.entity(entity).try_despawn();
        } else if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(
                loudness.gain * c.level * fade * volumes.gain(loudness.class),
            ));
        }
    }

    // One-off recordings with a volume curve follow it (over the cue's time: the
    // layer started at its own), and stop once it has faded them out for good.
    for (entity, voice, loudness, mut sink) in &mut fading {
        let Some(layer) = bank
            .cues
            .get(voice.cue)
            .and_then(|c| c.layers.get(voice.layer))
        else {
            continue;
        };
        let t = layer.time + (now - voice.born);
        let level = layer.volume_at(t);
        sink.set_volume(Volume::Linear(
            loudness.gain * level * volumes.gain(loudness.class) * mix.level(voice.category),
        ));
        if level <= 0.0 && layer.envelope.last().is_some_and(|&(end, _)| t >= end) {
            commands.entity(entity).try_despawn();
        }
    }

    // Held cues: start those newly asked for, keep the others' recordings at their
    // curves' volume and pitch (and where their owners are), and fade out the ones
    // no longer asked for.
    let asked = std::mem::take(&mut sfx.hold_requests);
    let mut held = std::mem::take(&mut sfx.held);
    for h in held.iter_mut() {
        h.wanted = false;
    }
    for HoldRequest {
        owner,
        cue: name,
        at,
        fade,
        one_offs,
        through,
    } in asked
    {
        if let Some(h) = held
            .iter_mut()
            .find(|h| h.owner == owner && h.cue == name && h.released.is_none())
        {
            h.wanted = true;
            h.at = at;
            h.fade = fade;
            continue;
        }
        let Some(cue) = bank.cues.get(name) else {
            continue;
        };
        let mut h = Held {
            owner,
            cue: name,
            at,
            wanted: true,
            started: now,
            pass_at: now,
            pass_from: 0.0,
            db: cue.volume + sfx.range(cue.volume_var),
            semitones: cue.pitch + sfx.range(cue.pitch_var),
            voices: Vec::new(),
            released: None,
            fade,
            one_offs,
            through,
        };
        schedule_held(&mut sfx, cue, &mut h, now, true);
        // (Its preset on as it starts.)
        if let Some(preset) = cue.preset
            && (!cue.preset_local || at.is_none())
        {
            mix.turn_on(&bank.mixer, preset, now);
        }
        held.push(h);
    }
    for h in held.iter_mut() {
        let Some(cue) = bank.cues.get(h.cue) else {
            continue;
        };
        let settings_gain = volumes.gain(cue.class) * mix.level(cue.category);
        if !h.wanted && h.released.is_none() {
            h.released = Some(now);
        }
        // The cue's loop: at LoopEnd it goes back to LoopStart, its one-off layers
        // from there on playing again (its looping ones play on).
        if h.released.is_none()
            && !h.through
            && cue.loop_end > cue.loop_start
            && h.pass_from + (now - h.pass_at) >= cue.loop_end
        {
            h.pass_at = now;
            h.pass_from = cue.loop_start;
            schedule_held(&mut sfx, cue, h, now, false);
        }
        let t = now - h.started;
        let fade = h
            .released
            .map_or(1.0, |r| (1.0 - (now - r) / h.fade).max(0.0));
        let atten = distance_db(cue, h.at, ear(h.at));
        let how_far = away_from(h.at, ear(h.at));
        for v in h.voices.iter_mut() {
            let Some(layer) = cue.layers.get(v.layer) else {
                continue;
            };
            let gain = atten.map_or(0.0, |a| {
                10f32.powf((SFX_VOLUME_DB + h.db + layer.volume + v.db + a) / 20.0)
                    * layer.volume_at(t)
                    * layer.volume_away(how_far)
                    * layer.flat_gain(h.at.is_none())
                    * fade
            });
            let semitones = h.semitones + layer.pitch + v.semitones + layer.pitch_at(t);
            let speed = 2f32.powf(semitones / 12.0);
            match v.entity {
                None if v.start <= now && h.released.is_none() => {
                    let settings = if layer.looping {
                        PlaybackSettings::LOOP
                    } else {
                        PlaybackSettings::DESPAWN
                    };
                    let entity = playable(&v.handle).spawn(
                        &mut commands,
                        (
                            settings
                                .with_volume(Volume::Linear(gain * settings_gain))
                                .with_speed(speed),
                            HeldSound,
                        ),
                    );
                    v.entity = Some(entity);
                }
                Some(e) => {
                    // (Its sink comes once it plays; a one-off's goes when it ends.)
                    if let Ok(mut sink) = sinks.get_mut(e) {
                        sink.set_volume(Volume::Linear(gain * settings_gain));
                        sink.set_speed(speed);
                    }
                }
                None => {}
            }
        }
        // One-off recordings long over are forgotten.
        let layers = &cue.layers;
        h.voices.retain(|v| {
            v.entity.is_none()
                || now - v.start < 10.0
                || layers.get(v.layer).is_some_and(|l| l.looping)
        });
    }
    held.retain(|h| {
        let over = h.released.is_some_and(|r| now - r >= h.fade);
        if over {
            for e in h.voices.iter().filter_map(|v| v.entity) {
                commands.entity(e).try_despawn();
            }
        }
        !over
    });
    sfx.held = held;
}

/// Lines up a held cue's recordings from its time `h.pass_from` on, starting `now`:
/// all its layers as it starts (`first`), its one-off layers from LoopStart on as
/// it loops back (its looping layers play on). Each picks one of its layer's
/// recordings and its own volume and pitch, and some stay silent or start a
/// little late (the layer's ChanceToPlayNone, DelayMin and DelayMax).
fn schedule_held(sfx: &mut Sfx, cue: &Cue, h: &mut Held, now: f32, first: bool) {
    for (i, layer) in cue.layers.iter().enumerate() {
        if layer.waves.is_empty()
            || layer.time < h.pass_from
            || (layer.looping && (!first || h.one_offs))
        {
            continue;
        }
        if layer.chance_none > 0.0 && sfx.random() < layer.chance_none {
            continue;
        }
        let pick = ((sfx.random() * layer.waves.len() as f32) as usize).min(layer.waves.len() - 1);
        let start = now + (layer.time - h.pass_from) + sfx.range(layer.delay);
        h.voices.push(HeldVoice {
            layer: i,
            start,
            entity: None,
            handle: layer.waves[pick].clone(),
            db: sfx.range(layer.volume_var),
            semitones: sfx.range(layer.pitch_var),
        });
    }
}

/// Leaving the match (for the menus, as the screen goes black): its sounds go with
/// it, as the game's level change stops every sound of the level (its audio
/// device's Flush). Those playing fade out over CUT_FADE_SECS and stop, the stingers
/// (a wave's plays on for half a minute after the countdown's "One...") and the
/// announcer's lines among them, its held cues are let go to fade out the same way,
/// and those it asked for that have yet to start never do; the menus' own come
/// after. (Its engine's loops are turned down: main.rs's quiet_engine.)
pub fn stop_match_sounds(
    mut commands: Commands,
    time: Res<Time>,
    bank: Option<Res<SoundBank>>,
    mut sfx: ResMut<Sfx>,
    voices: Query<(Entity, &Voice), Without<Cut>>,
    mut mix: ResMut<Mix>,
) {
    let now = time.elapsed_secs();
    sfx.requests.clear();
    sfx.pending.clear();
    sfx.cuts.clear();
    sfx.hold_requests.clear();
    for h in sfx.held.iter_mut() {
        h.released.get_or_insert(now);
        h.fade = h.fade.min(CUT_FADE_SECS);
    }
    for (entity, voice) in &voices {
        // (From where its volume is now, as `Sfx::cut` fades one.)
        let level = bank
            .as_ref()
            .and_then(|b| b.cues.get(voice.cue))
            .and_then(|c| c.layers.get(voice.layer))
            .map_or(1.0, |l| l.volume_at(l.time + (now - voice.born)))
            * mix.level(voice.category);
        commands.entity(entity).insert(Cut { at: now, level });
    }
    // (Its presets go with them: the menus' sounds play as they are.)
    mix.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use std::time::Duration;

    /// The game's fading with distance: full out to the near distance, then the
    /// inverse of the distance as the rolloff factor scales it, none past the far one.
    #[test]
    fn sounds_fade_by_the_inverse_of_their_distance() {
        let mut c = cue(SoundClass::Effects, false);
        c.dist_min = 10.0;
        c.dist_max = 200.0;
        let ear = Some(Vec3::ZERO);
        let db = |c: &Cue, d: f32| distance_db(c, Some(Vec3::new(d, 0.0, 0.0)), ear);
        assert_eq!(db(&c, 5.0), Some(0.0));
        // (Twice the near distance: half the volume, 6 dB down; three times, a third.)
        assert!((db(&c, 20.0).unwrap() + 6.02).abs() < 0.05);
        assert!((db(&c, 30.0).unwrap() + 9.54).abs() < 0.05);
        assert_eq!(db(&c, 250.0), None);
        // (A rolloff of 0 doesn't fade it; one of 5 fades it five times as fast.)
        c.rolloff = 0.0;
        assert_eq!(db(&c, 150.0), Some(0.0));
        c.rolloff = 5.0;
        assert!((db(&c, 12.0).unwrap() + 6.02).abs() < 0.05);
        // (Heard by the camera, as Jazz's own: never faded.)
        assert_eq!(distance_db(&c, None, ear), Some(0.0));
    }

    /// With two players on one screen, a sound is heard by the camera nearest it.
    #[test]
    fn each_sound_is_heard_by_the_nearest_camera() {
        let ears = [Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0)];
        let near_second = Some(Vec3::new(90.0, 0.0, 0.0));
        assert_eq!(nearest_ear(near_second, &ears), Some(ears[1]));
        assert_eq!(
            nearest_ear(Some(Vec3::new(5.0, 0.0, 0.0)), &ears),
            Some(ears[0])
        );
        assert_eq!(nearest_ear(None, &ears), Some(ears[0]));
        assert_eq!(nearest_ear(near_second, &[]), None);
    }

    /// One more copy of a cue than its max: which goes is its limiting's.
    #[test]
    fn one_too_many_of_a_cue_cuts_the_one_its_limiting_says() {
        // (Copies' starts, and distances from the camera.)
        let playing = [(1.0, 30.0), (0.5, 10.0), (2.0, 30.0)];
        assert_eq!(to_cut(Limiting::Oldest, &playing, 5.0), Some(1));
        assert_eq!(to_cut(Limiting::Newest, &playing, 5.0), None);
        // (The farthest goes, the oldest of those as far; a new one farther than
        // all doesn't play.)
        assert_eq!(to_cut(Limiting::Farthest, &playing, 5.0), Some(0));
        assert_eq!(to_cut(Limiting::Farthest, &playing, 30.0), Some(0));
        assert_eq!(to_cut(Limiting::Farthest, &playing, 31.0), None);
        // (All by the camera, as Jazz's own: the oldest goes.)
        let own = [(1.0, 0.0), (0.5, 0.0)];
        assert_eq!(to_cut(Limiting::Farthest, &own, 0.0), Some(1));
    }

    /// A cue of one recording of `class`, looping (for `Sfx::hold`) or played once.
    fn cue(class: SoundClass, looping: bool) -> Cue {
        Cue {
            class,
            volume: 0.0,
            pitch: 0.0,
            pitch_var: (0.0, 0.0),
            volume_var: (0.0, 0.0),
            max: 16,
            limiting: Limiting::Farthest,
            dist_min: 35.0,
            dist_max: 350.0,
            rolloff: 1.0,
            layers: vec![Layer {
                time: 0.0,
                volume: 0.0,
                pitch: 0.0,
                pitch_var: (0.0, 0.0),
                volume_var: (0.0, 0.0),
                looping,
                waves: vec![Handle::default()],
                envelope: Vec::new(),
                pitch_curve: Vec::new(),
                distance: Vec::new(),
                chance_none: 0.0,
                delay: (0.0, 0.0),
                pan: None,
            }],
            loop_end: 0.0,
            loop_start: 0.0,
            category: None,
            preset: None,
            preset_local: false,
        }
    }

    /// The announcer's lines in a match, Optimus Prime's and Megatron's (every one
    /// the demo plays: the waves', the game type's, an ally destroyed), turn on the
    /// game's GLB_DX_DUCK_9DB as they play: the effects and the music 9 dB down,
    /// the dialogue as it is. They play at their cues' own volumes, as every cue
    /// does (they were 10% louder); the characters' own lines in the same bank and
    /// the Dialog Volume slider's sample line turn nothing down.
    #[test]
    fn the_announcers_lines_turn_the_effects_and_music_down() {
        let Ok(text) =
            std::fs::read_to_string(crate::assets_dir().join("sounds").join("sounds.txt"))
        else {
            assert!(
                std::env::var_os("BEVY_ASSET_ROOT").is_none(),
                "no sounds.txt"
            );
            return;
        };
        // The cues' volumes as the file has them.
        let listed: HashMap<&str, f32> = text
            .lines()
            .filter_map(|l| {
                let mut p = l.split_whitespace();
                (p.next() == Some("cue")).then_some(())?;
                Some((p.next()?, p.next()?.parse().ok()?))
            })
            .collect();
        let bank = SoundBank::parse(&text, |_| Handle::default());
        assert!(listed.iter().all(|(c, v)| bank.cues[*c].volume == *v));
        let duck = bank.mixer.preset("GLB_DX_DUCK_9DB");
        assert!(duck.is_some());
        let mut announcer = 0;
        for side in ["", "_MGTRON"] {
            for line in [
                "SC009019", "SC017781", "SC017783", "SC017785", "SC017787", "SC017789", "SC017791",
                "SC017793", "SC019358", "SC019361", "SC019363",
            ] {
                let cue = &bank.cues[&format!("BL_DX_SWITCHBOARD.{line}{side}")];
                assert_eq!(cue.preset, duck, "{line}{side}");
                assert!(!cue.preset_local);
                announcer += 1;
            }
        }
        assert_eq!(announcer, 22);
        let ducking = bank.cues.values().filter(|c| c.preset == duck).count();
        assert_eq!(ducking, announcer);
        assert_eq!(bank.cues["BL_DX_SWITCHBOARD.SC016813"].preset, None);
        assert_eq!(bank.cues["BL_HUD_INTERFACE.DX_SLIDER_VOLUME"].preset, None);
        // As one plays: the guns, the music and the HUD's sounds down 9 dB, the
        // dialogue as it is.
        let mut mix = Mix::default();
        let line = bank.cues["BL_DX_SWITCHBOARD.SC017783"].preset.unwrap();
        mix.turn_on(&bank.mixer, line, 0.0);
        let mut on = vec![false; bank.mixer.presets()];
        on[line as usize] = true;
        let counts = vec![0; bank.mixer.categories()];
        mix.step(&bank.mixer, 1.0, 1.0, &on, &counts);
        let db = |cue: &str| 20.0 * mix.level(bank.cues[cue].category).log10();
        for cue in [
            "BL_WPN_GUN_PHOTON_ELIMINATOR.SHOOT",
            "BL_LVL_MP.STNG_ESCALATION_WAVE_BEGIN",
            "BL_HUD_INTERFACE.BUTTON_ACCEPT",
        ] {
            assert!((db(cue) + 9.0).abs() < 0.01, "{cue} {}", db(cue));
        }
        assert_eq!(db("BL_DX_SWITCHBOARD.SC016813"), 0.0);
        assert_eq!(db("BL_DX_SWITCHBOARD.SC017783"), 0.0);
    }

    /// A cue that turns a preset on does so as its first recording starts, and the
    /// preset stays on while it plays; a cue whose preset is only the local
    /// player's (a vehicle's jump) turns it on for his own copy and not for another
    /// player's.
    #[test]
    fn a_cue_turns_its_preset_on_as_it_plays() {
        const LINE: &str = "BL_DX_SWITCHBOARD.SC017783";
        const JUMP: &str = "BL_VEH_CAR.VEH_SPORT_01_DRIVE_JUMP_START";
        const SHOT: &str = "BL_WPN_GUN_PHOTON_ELIMINATOR.SHOOT";
        let mut bank = SoundBank::default();
        for line in [
            "mixcategory Master -",
            "mixcategory SFX_WET Master",
            "mixcategory SFX_WET_VEH_ENGINE SFX_WET",
            "mixcategory DX_DRY Master",
            "preset GLB_DX_DUCK_9DB 0.15 0.5 -1 SFX_WET -9",
            "preset VEH_JUMP 0.2 0.6 0.6 SFX_WET_VEH_ENGINE -10",
        ] {
            let words: Vec<&str> = line.split_whitespace().collect();
            assert!(bank.mixer.parse_line(words[0], &words[1..]));
        }
        let duck = bank.mixer.preset("GLB_DX_DUCK_9DB");
        let jump = bank.mixer.preset("VEH_JUMP");
        let mut line = cue(SoundClass::Dialog, false);
        line.category = bank.mixer.category("DX_DRY");
        line.preset = duck;
        let mut jumping = cue(SoundClass::Effects, false);
        jumping.category = bank.mixer.category("SFX_WET");
        jumping.preset = jump;
        jumping.preset_local = true;
        let mut shot = cue(SoundClass::Effects, false);
        shot.category = bank.mixer.category("SFX_WET");
        let engine = bank.mixer.category("SFX_WET_VEH_ENGINE");
        bank.cues.insert(LINE.to_string(), line);
        bank.cues.insert(JUMP.to_string(), jumping);
        bank.cues.insert(SHOT.to_string(), shot);
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Volumes::default());
        world.insert_resource(bank);
        world.insert_resource(Sfx::default());
        world.init_resource::<Mix>();
        let level = |world: &World, c: Option<u16>| 20.0 * world.resource::<Mix>().level(c).log10();
        let wet = world.resource::<SoundBank>().mixer.category("SFX_WET");
        // Another player's jump, 10 m off: nothing.
        world
            .resource_mut::<Sfx>()
            .play_at(JUMP, Vec3::new(10.0, 0.0, 0.0));
        frames(&mut world, 10);
        assert_eq!(level(&world, engine), 0.0);
        // The announcer's line, kept playing (its recording never ends here).
        world.resource_mut::<Sfx>().play(LINE);
        frames(&mut world, 60);
        assert!((level(&world, wet) + 9.0).abs() < 0.01);
        // A shot meanwhile starts at its category's volume, 9 dB down.
        world.resource_mut::<Sfx>().play(SHOT);
        frames(&mut world, 1);
        let shot: Vec<f32> = world
            .query::<(&Voice, &PlaybackSettings)>()
            .iter(&world)
            .filter(|(v, _)| v.cue == SHOT)
            .map(|(_, s)| 20.0 * s.volume.to_linear().log10() - SFX_VOLUME_DB)
            .collect();
        assert_eq!(shot.len(), 1);
        assert!((shot[0] + 9.0).abs() < 0.01, "{shot:?}");
        assert!((level(&world, engine) + 9.0).abs() < 0.01);
        // His own jump: his engine 10 dB further down.
        world.resource_mut::<Sfx>().play(JUMP);
        frames(&mut world, 20);
        assert!((level(&world, engine) + 19.0).abs() < 0.01);
        // The line ends (its recording gone): the effects come back.
        let lines: Vec<Entity> = world
            .query::<(Entity, &Voice)>()
            .iter(&world)
            .filter(|(_, v)| v.cue == LINE)
            .map(|(e, _)| e)
            .collect();
        assert_eq!(lines.len(), 1);
        world.despawn(lines[0]);
        frames(&mut world, 60);
        assert_eq!(level(&world, wet), 0.0);
        assert_eq!(level(&world, engine), 0.0);
    }

    /// The Photon Eliminator's SHOOT (the enemy soldiers' shot, the Photon Burst
    /// Rifle's too), its layers' distance curves as sounds.txt has them: its
    /// close-up shot, its tail and its distant shot.
    fn eliminator_shoot() -> Cue {
        let mut c = cue(SoundClass::Effects, false);
        let layer = c.layers.pop().unwrap();
        for keys in [
            vec![(4.9358, 1.0), (7.48363, 0.0)],
            vec![(3.91284, 1.0), (11.0298, 0.00393701), (88.8571, 0.791339)],
            vec![
                (4.48286, 0.0),
                (8.04527, 1.0),
                (59.7006, 1.0),
                (179.77, 0.511811),
            ],
        ] {
            c.layers.push(Layer {
                distance: keys,
                waves: vec![Handle::default()],
                envelope: Vec::new(),
                pitch_curve: Vec::new(),
                ..layer
            });
        }
        c
    }

    /// A gun's shot by the camera is its close-up recording, and away from it its
    /// distant one: the enemy soldiers' rifle, 20 m off, sounds its distant shot
    /// alone (its close-up burst, the Photon Burst Rifle's sound, only within 5 to
    /// 7.5 m), and a curve leaves a layer as it is at any distance.
    #[test]
    fn a_guns_shot_far_off_is_its_distant_recording() {
        let c = eliminator_shoot();
        let at = |d: f32| -> Vec<f32> { c.layers.iter().map(|l| l.volume_away(d)).collect() };
        assert_eq!(at(0.0), vec![1.0, 1.0, 0.0]);
        let far = at(20.0);
        assert_eq!(far[0], 0.0);
        assert!(far[1] < 0.1 && far[2] == 1.0, "{far:?}");
        let crossing = at(6.0);
        assert!(crossing.iter().all(|&v| v > 0.0 && v < 1.0), "{crossing:?}");
        assert_eq!(
            cue(SoundClass::Effects, false).layers[0].volume_away(123.0),
            1.0
        );
    }

    /// A cue looping over one-off layers held for them alone (an engine's hover, its
    /// loops the engine's own): its one-offs play, and again each time round its
    /// loop, and its looping layers never do.
    #[test]
    fn a_held_cues_one_offs_play_again_each_time_round() {
        const HOVER: &str = "BL_VEH_JET.VEH_INSECTICON_FLYING_LP";
        let mut c = cue(SoundClass::Effects, true);
        let looping = c.layers.pop().unwrap();
        c.layers.push(Layer {
            waves: vec![Handle::default()],
            envelope: Vec::new(),
            pitch_curve: Vec::new(),
            distance: Vec::new(),
            ..looping
        });
        c.layers.push(Layer {
            time: 0.2,
            looping: false,
            waves: vec![Handle::default()],
            envelope: Vec::new(),
            pitch_curve: Vec::new(),
            distance: Vec::new(),
            ..looping
        });
        c.loop_start = 0.05;
        c.loop_end = 0.35;
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Volumes::default());
        let mut bank = SoundBank::default();
        bank.cues.insert(HOVER.to_string(), c);
        assert!(bank.loops_one_offs(HOVER));
        world.insert_resource(bank);
        world.insert_resource(Sfx::default());
        world.init_resource::<Mix>();
        // (Held for a second: three passes of its 0.3 s loop after the first.)
        for _ in 0..60 {
            world.resource_mut::<Sfx>().hold_one_offs(7, HOVER, None);
            frames(&mut world, 1);
        }
        let sfx = world.resource::<Sfx>();
        let layers: Vec<usize> = sfx.held[0].voices.iter().map(|v| v.layer).collect();
        assert!(layers.iter().all(|&l| l == 1), "{layers:?}");
        assert!(layers.len() >= 3, "{layers:?}");
        // (A cue without such a loop, an engine's plain loops, has none.)
        let mut plain = SoundBank::default();
        plain
            .cues
            .insert(HOVER.to_string(), cue(SoundClass::Effects, true));
        assert!(!plain.loops_one_offs(HOVER));
    }

    /// A cue held through (a vehicle's boost loops) never goes back to its
    /// LoopStart: its one-off layer past its LoopEnd plays once, at its time, where
    /// held plainly it would play again with each pass.
    #[test]
    fn a_cue_held_through_plays_its_one_offs_once() {
        const BOOST: &str = "BL_VEH_TRUCK.VEH_TRUCK_02_BOOST_LOOP";
        let mut c = cue(SoundClass::Effects, true);
        let looping = c.layers.pop().unwrap();
        c.layers.push(Layer {
            waves: vec![Handle::default()],
            envelope: Vec::new(),
            pitch_curve: Vec::new(),
            distance: Vec::new(),
            ..looping
        });
        c.layers.push(Layer {
            time: 0.5,
            looping: false,
            waves: vec![Handle::default()],
            envelope: Vec::new(),
            pitch_curve: Vec::new(),
            distance: Vec::new(),
            ..looping
        });
        c.loop_start = 0.1;
        c.loop_end = 0.2;
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Volumes::default());
        let mut bank = SoundBank::default();
        bank.cues.insert(BOOST.to_string(), c);
        world.insert_resource(bank);
        world.insert_resource(Sfx::default());
        world.init_resource::<Mix>();
        for _ in 0..60 {
            world.resource_mut::<Sfx>().hold_through(7, BOOST, None);
            frames(&mut world, 1);
        }
        let sfx = world.resource::<Sfx>();
        let layers: Vec<usize> = sfx.held[0].voices.iter().map(|v| v.layer).collect();
        assert_eq!(layers, vec![0, 1]);
    }

    /// Every car, truck and tank whose audio component has boost loops has them in
    /// the sound bank, looping (Soundwave's truck VEH_TRUCK_02's, Megatron's tank
    /// VEH_TANK_01's), its boost's wheels where its component plays them (the
    /// trucks', not the tanks'); the Dinobots' components have none, the jets' play
    /// their own way (main.rs's ENGINES).
    #[test]
    fn the_ground_vehicles_have_their_boost_loops() {
        use crate::character::{
            CLASSES, Characters, SOUND_BOOST_LOOP, SOUND_BOOST_WHEELS, class_files_present,
        };
        let assets = crate::assets_dir();
        if !(0..CLASSES.len()).all(|c| class_files_present(&assets, c)) {
            return;
        }
        let Ok(text) = std::fs::read_to_string(assets.join("sounds/sounds.txt")) else {
            return;
        };
        let bank = SoundBank::parse(&text, |_| Handle::default());
        let chars = Characters::load(&assets);
        let mut with = 0;
        for (i, v) in chars.vehicles.iter().enumerate() {
            if v.plane.is_some() {
                continue;
            }
            let Some(cue) = chars.car_sound(Some(i), SOUND_BOOST_LOOP, "") else {
                continue;
            };
            with += 1;
            assert!(bank.loops(cue), "{} {cue}", v.form);
        }
        // (The Infiltrator's seven, the Destroyer's ten and the Titan's eight.)
        assert_eq!(with, 25);
        let at = |form: &str| chars.vehicles.iter().position(|v| v.form == form);
        let boost = |form: &str, event: &str| chars.car_sound(at(form), event, "");
        assert_eq!(
            boost("SoundwaveTruckForm", SOUND_BOOST_LOOP),
            Some("BL_VEH_TRUCK.VEH_TRUCK_02_BOOST_LOOP")
        );
        assert!(boost("SoundwaveTruckForm", SOUND_BOOST_WHEELS).is_some_and(|c| bank.has_cue(c)));
        assert_eq!(
            boost("MegatronVehicleForm", SOUND_BOOST_LOOP),
            Some("BL_VEH_TANK.VEH_TANK_01_BOOST_LOOP")
        );
        assert_eq!(boost("MegatronVehicleForm", SOUND_BOOST_WHEELS), None);
        assert_eq!(
            boost("GrimlockMultiplayerVehicleForm", SOUND_BOOST_LOOP),
            None
        );
    }

    /// One heard where Jazz is (his own, no place given: by the camera) plays the
    /// layers its distance curves give there, and not the ones they silence.
    #[test]
    fn a_layer_its_distance_silences_does_not_play() {
        const SHOOT: &str = "BL_WPN_GUN_PHOTON_ELIMINATOR.SHOOT";
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Volumes::default());
        let mut bank = SoundBank::default();
        bank.cues.insert(SHOOT.to_string(), eliminator_shoot());
        world.insert_resource(bank);
        world.insert_resource(Sfx::default());
        world.init_resource::<Mix>();
        world.resource_mut::<Sfx>().play(SHOOT);
        frames(&mut world, 1);
        let mut layers: Vec<usize> = world
            .query::<&Voice>()
            .iter(&world)
            .map(|v| v.layer)
            .collect();
        layers.sort_unstable();
        assert_eq!(layers, vec![0, 1]);
    }

    /// `n` frames of sounds being played (play_sounds runs in the menu's chain too).
    fn frames(world: &mut World, n: usize) {
        for _ in 0..n {
            world
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(16));
            world.run_system_once(play_sounds).unwrap();
        }
    }

    /// How many recordings of `cue` are playing.
    fn playing(world: &mut World, cue: &str) -> usize {
        world
            .query::<&Voice>()
            .iter(world)
            .filter(|v| v.cue == cue)
            .count()
    }

    /// Leaving the match for the menus: a wave's stinger still playing fades out and
    /// stops within a tenth of a second, a held loop is let go the same way, and what
    /// the match asked for that had yet to start (a cue's later layer, a request not
    /// taken up yet) never plays; the menus' own sounds play as ever.
    #[test]
    fn leaving_the_match_stops_its_sounds() {
        const STINGER: &str = "BL_LVL_MP.STNG_ESCALATION_WAVE_BEGIN";
        const LATER: &str = "BL_LVL_MP.MX_MATCH_BEGIN";
        const LOOP: &str = "BL_WPN_GUN_TECH_VOLT.FIRE_LP";
        const CLICK: &str = "BL_HUD_INTERFACE.BUTTON_ACCEPT";
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Volumes::default());
        let mut bank = SoundBank::default();
        for (name, class, looping) in [
            (STINGER, SoundClass::Music, false),
            (LATER, SoundClass::Music, false),
            (LOOP, SoundClass::Effects, true),
            (CLICK, SoundClass::Effects, false),
        ] {
            bank.cues.insert(name.to_string(), cue(class, looping));
        }
        world.insert_resource(bank);
        world.insert_resource(Sfx::default());
        world.init_resource::<Mix>();
        // In the match: the stinger starts, a loop is held.
        world.resource_mut::<Sfx>().play(STINGER);
        for _ in 0..2 {
            world.resource_mut::<Sfx>().hold(1, LOOP, None);
            frames(&mut world, 1);
        }
        assert_eq!(playing(&mut world, STINGER), 1);
        let held: Vec<Entity> = world
            .query_filtered::<Entity, With<HeldSound>>()
            .iter(&world)
            .collect();
        assert_eq!(held.len(), 1);
        // What it asked for last, still to start: a cue's layer due a little later,
        // and a request play_sounds has yet to take up.
        let now = world.resource::<Time>().elapsed_secs();
        let mut sfx = world.resource_mut::<Sfx>();
        sfx.pending.push(Pending {
            start: now + 0.05,
            at: None,
            cue: LATER,
            layer: 0,
            fades: false,
            handle: Handle::default(),
            db: 0.0,
            speed: 1.0,
            owner: None,
            trigger: false,
        });
        sfx.play(LATER);
        // Leaving it (its systems stop: nothing holds the loop from then on), and the
        // menu's button sound as it comes up.
        world.run_system_once(stop_match_sounds).unwrap();
        world.resource_mut::<Sfx>().play(CLICK);
        frames(&mut world, 1);
        assert_eq!(playing(&mut world, CLICK), 1, "the menu's sound plays");
        frames(&mut world, 8);
        assert_eq!(playing(&mut world, STINGER), 0, "the stinger stopped");
        assert_eq!(
            playing(&mut world, LATER),
            0,
            "what was yet to start never did"
        );
        assert!(
            held.iter().all(|e| world.get_entity(*e).is_err()),
            "the loop stopped"
        );
        assert!(world.resource::<Sfx>().held.is_empty());
    }

    /// A recording plays from its file the first time, its decoding started off the
    /// main thread, and from memory once that is done: the same samples, channels
    /// and rate as playing its file gives. One too long to keep in memory plays from
    /// its file for good.
    #[test]
    fn recordings_play_from_memory_once_decoded() {
        let Ok(bytes) = std::fs::read(crate::assets_dir().join("sounds/FS_BOT_LRG_DFLT_01.ogg"))
        else {
            eprintln!("the sounds aren't built: skipped");
            return;
        };
        AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let mut world = World::new();
        world.init_resource::<Assets<AudioSource>>();
        world.init_resource::<Assets<Pcm>>();
        world.init_resource::<Decoded>();
        let file = AudioSource {
            bytes: bytes.into(),
        };
        let mut long = file.clone();
        long.bytes = vec![0u8; DECODE_MAX_BYTES + 1].into();
        let wave = world
            .resource_mut::<Assets<AudioSource>>()
            .add(file.clone());
        let too_long = world.resource_mut::<Assets<AudioSource>>().add(long);
        let play = |world: &mut World, wave: &Handle<AudioSource>| {
            world.resource_scope(|world, mut decoded: Mut<Decoded>| {
                decoded.playable(wave, world.resource::<Assets<AudioSource>>())
            })
        };
        assert!(matches!(play(&mut world, &wave), Playable::File(_)));
        assert!(matches!(play(&mut world, &too_long), Playable::File(_)));
        let pcm = (0..2000)
            .find_map(|_| {
                world.run_system_once(finish_decoding).unwrap();
                match play(&mut world, &wave) {
                    Playable::Memory(pcm) => Some(pcm),
                    Playable::File(_) => {
                        std::thread::sleep(Duration::from_millis(5));
                        None
                    }
                }
            })
            .expect("decoded");
        let pcm = world.resource::<Assets<Pcm>>().get(&pcm).unwrap().clone();
        let from_file = file.decoder();
        assert_eq!(pcm.decoder().channels(), from_file.channels());
        assert_eq!(pcm.decoder().sample_rate(), from_file.sample_rate());
        assert!(pcm.decoder().eq(from_file));
        assert!(matches!(play(&mut world, &too_long), Playable::File(_)));
    }

    /// A layer's speaker levels as the demo's two speakers take them: a plain
    /// layer's, one only in the centre or the back speakers, in full; one silent
    /// in all five (a car's boost exhaust) silent; and a smart pan's only for the
    /// player's own (heard by the camera: placed round it, the levels aren't the
    /// game's), a 2d layer's wherever it is heard.
    #[test]
    fn a_layers_speaker_levels_fold_down_to_two() {
        assert_eq!(flat_level(0.0, 0.0, -96.0, -96.0, -96.0), 1.0);
        assert_eq!(flat_level(-96.0, -96.0, -96.0, 0.0, 0.0), 1.0);
        assert_eq!(flat_level(-96.0, -96.0, 0.0, -96.0, -96.0), 1.0);
        assert_eq!(flat_level(-96.0, -96.0, -96.0, -96.0, -96.0), 0.0);
        let text = "cue EXHAUST -3 0 0 0 0 0 2 9 30 6 farthest\n\
                    layer 0 0 0 0 0 0 0 1 A\n\
                    pan 0 -96 -96 -96 -96 -96 smart\n\
                    cue BLAST -6 0 0 0 0 0 4 4 64 1 farthest\n\
                    layer 0 0 0 0 0 0 0 0 B\n\
                    layer 0 0 0 0 0 0 0 0 C\n\
                    pan 1 -96 -96 -96 -96 -96 2d\n";
        let bank = SoundBank::parse(text, |_| Handle::default());
        let exhaust = &bank.cues["EXHAUST"].layers[0];
        assert_eq!(exhaust.flat_gain(true), 0.0);
        assert_eq!(exhaust.flat_gain(false), 1.0);
        let blast = &bank.cues["BLAST"].layers;
        assert_eq!(blast[0].flat_gain(false), 1.0);
        assert_eq!(blast[1].flat_gain(false), 0.0);
        assert_eq!(blast[1].flat_gain(true), 0.0);
    }

    /// A car's boost exhaust, only in the game's back speakers (its PanFrontLeft
    /// and PanFrontRight -96 dB, the rest silent too), isn't heard by its driver:
    /// the Infiltrators' sports cars', which was the loudest of their boost's
    /// sounds by some 9 dB; it is by everyone else (placed round them).
    #[test]
    fn a_cars_boost_exhaust_is_silent_for_its_driver() {
        let Ok(text) =
            std::fs::read_to_string(crate::assets_dir().join("sounds").join("sounds.txt"))
        else {
            eprintln!("No sounds.txt: skipped");
            return;
        };
        let bank = SoundBank::parse(&text, |_| Handle::default());
        let Some(cue) = bank.cues.get("BL_VEH_CAR.VEH_SPORT_02_BOOST_LOOP_EXHAUST") else {
            eprintln!("No Infiltrator boost exhaust: skipped");
            return;
        };
        if cue.layers.iter().all(|l| l.pan.is_none()) {
            eprintln!("sounds.txt made before its speaker levels: skipped");
            return;
        }
        assert!(cue.layers.iter().all(|l| l.flat_gain(true) == 0.0));
        assert!(cue.layers.iter().all(|l| l.flat_gain(false) == 1.0));
        let main = &bank.cues["BL_VEH_CAR.VEH_SPORT_02_BOOST_LOOP"];
        assert!(main.layers.iter().all(|l| l.flat_gain(true) == 1.0));
    }
}
