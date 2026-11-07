//! The enemies' sounds, from models/enemies/ai_sounds.txt (tools/build_ai.py: each
//! type's sound sets and the dialog rules) and the dialog lines' lengths in
//! sounds/voices.txt (tools/build_sounds.py):
//!
//! * **Sound sets**: a type's robot form's HmSoundEventSet (and its vehicle
//!   form's) gives each sound event the cue it plays for it (TnPawn.FindSoundCue:
//!   the robot form's set, else the vehicle form's). A dialog cue holds a line for
//!   each of the set's dialog characters: a pawn speaks as one of them (a soldier
//!   type has two voices, SLDD01 and SLDD02; each soldier is given one of them as
//!   he comes in).
//! * **Footsteps** (AnimNotify_Footstep, HmFootstepComponent.PlayFootStep): the
//!   floor's physical material's sound event for the step's type, which on every
//!   Cybertron floor is FS_DEFAULT_<type> (ENV_PHYSMAT_p's Default, Metal and
//!   Concrete materials give those, the component's defaults the crouching ones),
//!   its cue the pawn's set's, at its feet.
//! * **The animations' sounds**: HmAnimNotify_Sound's cue and
//!   HmAnimNotify_SoundEvent's event (the set's cue), at the pawn. (Their body
//!   impacts, HmAnimNotify_BodyImpact, would play the floor material's
//!   BodyImpactSounds, which no Cybertron floor has, nor the component a default:
//!   none.)
//! * **Dialog**: every line goes through the dialog manager (HmDialogEventManager:
//!   the animations' HmAnimNotify_Dialog, the planner's STS_PlayDialog, and a
//!   pawn's incidental dialog: SpawnedDialog as it comes in, DeathRX_Verbal and
//!   DeathRX_NonVerbalDialog as it dies unless by a headshot, and its teammates'
//!   within 50 m TeammateSpawnedDialog and TeammateKilledDialog). An event's dialog
//!   group, and each group above it, decides: a group switched off, or keeping
//!   quiet (its retrigger timer: a random time between its ReTriggerDelayMin and
//!   Max after it fills up, and as a match begins unless its InitialChanceToPlay
//!   roll lets it speak at once), refuses the line; a group with as many lines
//!   playing as its InstanceLimit takes it only if it matters more (DialogPriority)
//!   than the least of them, which is cut short. The speaker then says the event's
//!   cue (if his set has one), cutting his own line short; a death line
//!   (HmAnimNotify_DialogDeath) is his last.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use bevy::prelude::*;

use super::ue::Rng;

pub const SOUNDS_FILE: &str = "models/enemies/ai_sounds.txt";
/// The dialog lines' lengths (tools/build_sounds.py's `length` lines).
const LENGTHS_FILE: &str = "sounds/voices.txt";

/// HmFootstepComponent's sound events by AnimNotify_Footstep's EFootstepType: walk,
/// scuff, jump, land, run, jog, crouched run, walk, jog and scuff, hard landing.
pub const FOOTSTEP_EVENTS: [&str; 11] = [
    "FS_DEFAULT_WALK",
    "FS_DEFAULT_SCUFF",
    "FS_DEFAULT_JUMP",
    "FS_DEFAULT_LAND",
    "FS_DEFAULT_RUN",
    "FS_DEFAULT_JOG",
    "FS_DEFAULT_CROUCH_RUN",
    "FS_DEFAULT_CROUCH_WALK",
    "FS_DEFAULT_CROUCH_JOG",
    "FS_DEFAULT_CROUCH_SCUFF",
    "FS_DEFAULT_HARD_LAND",
];

/// The incidental dialog's events (TnPawn's).
pub const SPAWNED: &str = "SpawnedDialog";
pub const TEAMMATE_SPAWNED: &str = "TeammateSpawnedDialog";
pub const DEATH_VERBAL: &str = "DeathRX_Verbal";
pub const DEATH_NON_VERBAL: &str = "DeathRX_NonVerbalDialog";
pub const TEAMMATE_KILLED: &str = "TeammateKilledDialog";

/// TnPawnIncidentalDialogService's close teammates: within 50 m (uu, squared).
const TEAMMATE_DISTANCE_SQ: f32 = 25_000_000.0;

/// How long a line lasts when its length isn't known (s).
const LINE_SECS: f32 = 1.5;

/// The name an event goes by here: its own (the game's names it with its package,
/// SoundEvents_Dialog.Blindfire; the blueprints' references by its own alone).
pub fn event_name(full: &str) -> &str {
    let full = full.trim();
    full.rsplit('.').next().unwrap_or(full)
}

/// An HmSoundEventSet: its dialog characters, and each event's cue.
#[derive(Debug, Default)]
pub struct SoundSet {
    pub chars: Vec<Arc<str>>,
    cues: HashMap<String, &'static str>,
}

/// An HmSoundEvent's dialog rules: its group (none: the default group) and its
/// DialogPriority.
#[derive(Clone, Copy, Debug)]
struct DialogEvent {
    group: Option<usize>,
    priority: f32,
}

/// An HmSoundEventDialogGroup.
#[derive(Clone, Debug)]
struct DialogGroup {
    parent: Option<usize>,
    /// InitialChanceToPlay (%).
    chance: u32,
    /// ReTriggerDelayMin and Max (s).
    delay: (f32, f32),
    /// InstanceLimit (0: no limit, but it keeps quiet after each line).
    limit: usize,
}

/// The enemies' sound sets and dialog rules.
#[derive(Debug, Default)]
pub struct AiSounds {
    sets: HashMap<String, Arc<SoundSet>>,
    /// Each type's robot form's set and vehicle form's.
    types: HashMap<String, [Option<Arc<SoundSet>>; 2]>,
    events: HashMap<String, DialogEvent>,
    groups: Vec<DialogGroup>,
    default_group: usize,
    /// The dialog lines: each speaker's (cue@speaker, tools/build_sounds.py) and
    /// every line's length (s).
    lines: HashMap<String, (&'static str, f32)>,
}

impl AiSounds {
    /// Reads ai_sounds.txt and the lines' lengths (no sounds without them).
    pub fn load(assets: &Path) -> AiSounds {
        let text = std::fs::read_to_string(assets.join(SOUNDS_FILE)).unwrap_or_default();
        let lengths = std::fs::read_to_string(assets.join(LENGTHS_FILE)).unwrap_or_default();
        if text.is_empty() {
            info!("{SOUNDS_FILE} not found: the enemies make no sounds of their own");
        }
        AiSounds::parse(&text, &lengths)
    }

    pub fn parse(text: &str, lengths: &str) -> AiSounds {
        let mut out = AiSounds::default();
        // (Names live as long as the sound bank's.)
        let mut names: HashMap<String, &'static str> = HashMap::new();
        let mut name = |s: &str| -> &'static str {
            names
                .entry(s.to_string())
                .or_insert_with(|| Box::leak(s.to_string().into_boxed_str()))
        };
        let rows: Vec<Vec<&str>> = text
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| l.split('\t').map(str::trim).collect())
            .collect();
        // The groups first (their parents by name).
        let mut group_index: HashMap<&str, usize> = HashMap::new();
        for r in rows.iter().filter(|r| r[0] == "group" && r.len() >= 7) {
            group_index.insert(r[1], group_index.len());
        }
        let num = |s: &str| s.parse::<f32>().unwrap_or(0.0);
        out.groups = vec![
            DialogGroup {
                parent: None,
                chance: 100,
                delay: (0.0, 0.0),
                limit: 1,
            };
            group_index.len()
        ];
        let mut sets: HashMap<String, SoundSet> = HashMap::new();
        let mut type_rows = Vec::new();
        for r in &rows {
            match r[0] {
                "group" if r.len() >= 7 => {
                    let g = &mut out.groups[group_index[r[1]]];
                    g.parent = group_index.get(r[2]).copied();
                    g.chance = num(r[3]) as u32;
                    g.delay = (num(r[4]), num(r[5]).max(num(r[4])));
                    g.limit = num(r[6]) as usize;
                }
                "default" if r.len() >= 2 => {
                    out.default_group = group_index.get(r[1]).copied().unwrap_or(0);
                }
                "dialog" if r.len() >= 4 => {
                    out.events.insert(
                        event_name(r[1]).to_string(),
                        DialogEvent {
                            group: group_index.get(r[2]).copied(),
                            priority: num(r[3]),
                        },
                    );
                }
                "set" if r.len() >= 2 => {
                    sets.entry(r[1].to_string()).or_default().chars =
                        r[2..].iter().map(|c| Arc::from(*c)).collect();
                }
                "cue" if r.len() >= 4 => {
                    let cue = name(r[3]);
                    sets.entry(r[1].to_string())
                        .or_default()
                        .cues
                        .insert(event_name(r[2]).to_string(), cue);
                }
                "type" if r.len() >= 4 => type_rows.push(r.clone()),
                _ => {}
            }
        }
        out.sets = sets.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
        for r in type_rows {
            let set = |s: &str| out.sets.get(s).cloned();
            out.types.insert(r[1].to_string(), [set(r[2]), set(r[3])]);
        }
        for line in lengths.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.first() == Some(&"length") && f.len() >= 3 {
                out.lines.insert(f[1].to_string(), (name(f[1]), num(f[2])));
            }
        }
        out
    }

    /// A pawn of `type_id` coming in: its sets and the dialog character it speaks
    /// as (one of its set's, at random).
    pub fn voice(&self, type_id: &str, rng: &mut Rng) -> Voice {
        let sets = self.types.get(type_id).cloned().unwrap_or_default();
        let chars = sets[0]
            .as_ref()
            .or(sets[1].as_ref())
            .map(|s| s.chars.clone())
            .unwrap_or_default();
        let character = (!chars.is_empty()).then(|| {
            chars[((rng.frand() * chars.len() as f32) as usize).min(chars.len() - 1)].clone()
        });
        Voice {
            sets,
            character,
            ..Default::default()
        }
    }

    fn rules(&self, event: &str) -> DialogEvent {
        self.events.get(event).copied().unwrap_or(DialogEvent {
            group: None,
            priority: 0.0,
        })
    }
}

/// A pawn's voice: its sound sets, the dialog character it speaks as, and its
/// dialog component (the line it is saying; switched off after its last).
#[derive(Clone, Debug, Default)]
pub struct Voice {
    sets: [Option<Arc<SoundSet>>; 2],
    pub character: Option<Arc<str>>,
    line: Option<Line>,
    off: bool,
    serial: u32,
}

/// A line being said: its event, its cue (none: the event had none for this
/// pawn, nothing plays), until when, and the owner its recordings play as.
#[derive(Clone, Debug)]
struct Line {
    event: Arc<str>,
    cue: Option<&'static str>,
    until: f32,
    owner: u64,
}

impl Voice {
    /// TnPawn.FindSoundCue: the event's cue in its robot form's set, else its
    /// vehicle form's (a dialog cue: its character's line of it, with its length).
    pub fn find_cue(&self, sounds: &AiSounds, event: &str) -> Option<(&'static str, f32)> {
        let cue = self
            .sets
            .iter()
            .flatten()
            .find_map(|s| s.cues.get(event).copied())?;
        Some(self.line_of(sounds, cue))
    }

    /// A cue as it says it: a dialog cue its character's line of it (with its
    /// length), another cue as it is.
    pub fn line_of(&self, sounds: &AiSounds, cue: &'static str) -> (&'static str, f32) {
        if let Some(c) = &self.character
            && let Some(&line) = sounds.lines.get(&format!("{cue}@{c}"))
        {
            return line;
        }
        sounds.lines.get(cue).copied().unwrap_or((cue, LINE_SECS))
    }

    /// HmDialogEmitter.GetIsDialogPlayingEvent: whether it is saying `event` (an
    /// event it has no cue for: whether it is saying anything).
    fn is_saying(&self, event: &str, now: f32) -> bool {
        self.line
            .as_ref()
            .is_some_and(|l| now < l.until && (l.cue.is_none() || &*l.event == event))
    }
}

/// A line of dialog asked for.
#[derive(Clone, Debug)]
pub enum DialogRequest {
    /// A pawn's (PlayIncidentalDialog, an animation's notify): `last`, its dialog
    /// switched off after it (HmAnimNotify_DialogDeath).
    Say {
        speaker: Entity,
        event: Arc<str>,
        last: bool,
    },
    /// Its close teammates' (BroadcastIncidentalDialogToCloseTeammates): those
    /// alive within 50 m of `at` (Unreal's space).
    Teammates {
        from: Entity,
        at: Vec3,
        event: Arc<str>,
    },
    /// A cue of its own (HmPawn.PlayDialog: an animation's dialog notify with a
    /// cue, the Swarmers' chirps): said in its voice, cutting short the line
    /// before, with none of the dialog manager's rules.
    Cue { speaker: Entity, cue: &'static str },
}

impl DialogRequest {
    pub fn say(speaker: Entity, event: &str) -> DialogRequest {
        DialogRequest::Say {
            speaker,
            event: Arc::from(event_name(event)),
            last: false,
        }
    }

    pub fn teammates(from: Entity, at: Vec3, event: &str) -> DialogRequest {
        DialogRequest::Teammates {
            from,
            at,
            event: Arc::from(event),
        }
    }
}

/// What the dialog does with the sounds: a line said (cutting short the speaker's
/// last, `cut`), at a place (Unreal's space), or a speaker's line cut short.
#[derive(Clone, Debug)]
pub enum Say {
    Line {
        owner: u64,
        cut: Option<u64>,
        cue: &'static str,
        at: Vec3,
    },
    Hush {
        owner: u64,
    },
}

/// A speaker the dialog manager may hear from: its entity, its voice, where it is
/// (Unreal's space) and whether it is alive.
pub struct Speaker<'a> {
    pub entity: Entity,
    pub voice: &'a mut Voice,
    pub at: Vec3,
    pub alive: bool,
}

/// A dialog group's state (HmDialogEventManager.DialogPlaybackGroup): its
/// retrigger timer and the lines playing in it (speaker, event, priority), the
/// least important first.
#[derive(Clone, Debug, Default)]
struct Playback {
    retrigger: f32,
    instances: Vec<(Entity, Arc<str>, f32)>,
}

/// HmDialogEventManager.
#[derive(Clone, Debug, Default)]
pub struct DialogManager {
    groups: HashMap<usize, Playback>,
}

impl DialogManager {
    /// Tick: the retrigger timers run down.
    pub fn tick(&mut self, dt: f32) {
        for g in self.groups.values_mut() {
            g.retrigger = (g.retrigger - dt).max(0.0);
        }
    }

    /// A new match: no groups yet.
    pub fn clear(&mut self) {
        self.groups.clear();
    }

    /// FindPlaybackGroupIndex (CreateDialogPlaybackGroup: a group new to it starts
    /// with its GetInitialDelay).
    fn group(&mut self, sounds: &AiSounds, g: usize, rng: &mut Rng) -> &mut Playback {
        self.groups.entry(g).or_insert_with(|| {
            let rules = &sounds.groups[g];
            let retrigger = if ((rng.frand() * 100.0) as u32) < rules.chance {
                0.0
            } else {
                rules.delay.0 + (rules.delay.1 - rules.delay.0) * rng.frand()
            };
            Playback {
                retrigger,
                instances: Vec::new(),
            }
        })
    }

    /// Play: `speaker` says `event` if its groups let it (and it isn't switched
    /// off): the lines it says and cuts short go to `out`.
    #[allow(clippy::too_many_arguments)]
    fn play(
        &mut self,
        sounds: &AiSounds,
        speakers: &mut [Speaker],
        speaker: Entity,
        event: &Arc<str>,
        last: bool,
        now: f32,
        rng: &mut Rng,
        out: &mut Vec<Say>,
    ) {
        if sounds.groups.is_empty() || !speakers.iter().any(|s| s.entity == speaker) {
            return;
        }
        let rules = sounds.rules(event);
        let first = rules.group.unwrap_or(sounds.default_group);
        // CleanupDialogPlaybackGroups: the lines no longer being said.
        for g in self.groups.values_mut() {
            g.instances.retain(|(e, ev, _)| {
                speakers
                    .iter()
                    .find(|s| s.entity == *e)
                    .is_some_and(|s| s.voice.is_saying(ev, now))
            });
        }
        // CanPlayDialog: each group up the chain.
        let mut g = Some(first);
        let mut guard = 0;
        while let Some(i) = g {
            guard += 1;
            if guard > 20 {
                break;
            }
            let limit = sounds.groups[i].limit;
            let pb = self.group(sounds, i, rng);
            if pb.retrigger > 0.0 {
                return;
            }
            if limit != 0 && pb.instances.len() >= limit {
                let (least, ev, priority) = pb.instances[0].clone();
                if rules.priority <= priority {
                    return;
                }
                // (The least of them is cut short.)
                if let Some(s) = speakers.iter_mut().find(|s| s.entity == least)
                    && s.voice.is_saying(&ev, now)
                    && let Some(l) = s.voice.line.take()
                {
                    out.push(Say::Hush { owner: l.owner });
                }
                pb.instances.remove(0);
            }
            g = sounds.groups[i].parent;
        }
        // PlayInternal: the line in each group (by priority), the groups filled up
        // keeping quiet for a while.
        let mut g = Some(first);
        let mut guard = 0;
        while let Some(i) = g {
            guard += 1;
            if guard > 20 {
                break;
            }
            let rules_g = sounds.groups[i].clone();
            let pb = self.group(sounds, i, rng);
            let at = pb
                .instances
                .iter()
                .position(|(_, _, p)| rules.priority < *p)
                .unwrap_or(pb.instances.len());
            pb.instances
                .insert(at, (speaker, event.clone(), rules.priority));
            if rules_g.limit == 0 || pb.instances.len() >= rules_g.limit {
                pb.retrigger = rules_g.delay.0 + (rules_g.delay.1 - rules_g.delay.0) * rng.frand();
            }
            g = rules_g.parent;
        }
        // Target.PlayDialogEvent: its cue for the event, if it has one.
        let Some(s) = speakers.iter_mut().find(|s| s.entity == speaker) else {
            return;
        };
        if s.voice.off {
            return;
        }
        let found = s.voice.find_cue(sounds, event);
        let cut = s
            .voice
            .line
            .take()
            .filter(|l| now < l.until && l.cue.is_some())
            .map(|l| l.owner);
        s.voice.serial = s.voice.serial.wrapping_add(1);
        let owner = line_owner(speaker, s.voice.serial);
        s.voice.line = Some(Line {
            event: event.clone(),
            cue: found.map(|(c, _)| c),
            until: now + found.map_or(0.0, |(_, secs)| secs),
            owner,
        });
        if last {
            s.voice.off = true;
        }
        match found {
            Some((cue, _)) => out.push(Say::Line {
                owner,
                cut,
                cue,
                at: s.at,
            }),
            None => {
                if let Some(owner) = cut {
                    out.push(Say::Hush { owner });
                }
            }
        }
    }
}

/// The owner a speaker's line plays as (its sounds': a new one for each line, so
/// that the one before can be cut short alone).
fn line_owner(speaker: Entity, serial: u32) -> u64 {
    (speaker.to_bits().rotate_left(17) ^ 0xD1A1_06E5_0000_0000) ^ serial as u64
}

/// The lines asked for this frame, said as the dialog manager lets them (the
/// teammates' first found: those alive within 50 m but the one it is about).
pub fn speak(
    manager: &mut DialogManager,
    requests: Vec<DialogRequest>,
    sounds: &AiSounds,
    speakers: &mut [Speaker],
    now: f32,
    rng: &mut Rng,
    out: &mut Vec<Say>,
) {
    for r in requests {
        match r {
            DialogRequest::Say {
                speaker,
                event,
                last,
            } => {
                manager.play(sounds, speakers, speaker, &event, last, now, rng, out);
            }
            DialogRequest::Teammates { from, at, event } => {
                let near: Vec<Entity> = speakers
                    .iter()
                    .filter(|s| {
                        s.alive
                            && s.entity != from
                            && (s.at - at).length_squared() <= TEAMMATE_DISTANCE_SQ
                    })
                    .map(|s| s.entity)
                    .collect();
                for e in near {
                    manager.play(sounds, speakers, e, &event, false, now, rng, out);
                }
            }
            DialogRequest::Cue { speaker, cue } => {
                let Some(s) = speakers.iter_mut().find(|s| s.entity == speaker) else {
                    continue;
                };
                if s.voice.off {
                    continue;
                }
                let cut = s
                    .voice
                    .line
                    .take()
                    .filter(|l| now < l.until && l.cue.is_some())
                    .map(|l| l.owner);
                s.voice.serial = s.voice.serial.wrapping_add(1);
                let owner = line_owner(speaker, s.voice.serial);
                // (HmPawn.PlayDialog: its dialog character's line of the cue.)
                let (cue, secs) = s.voice.line_of(sounds, cue);
                s.voice.line = Some(Line {
                    event: Arc::from(""),
                    cue: Some(cue),
                    until: now + secs,
                    owner,
                });
                out.push(Say::Line {
                    owner,
                    cut,
                    cue,
                    at: s.at,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA: &str = "type\tSoldier\tSET\t-\n\
set\tSET\tA\tB\n\
cue\tSET\tSoundEvents_Footsteps.FS_DEFAULT_RUN\tFS.RUN\n\
cue\tSET\tSoundEvents_Dialog.PeeksFromCover\tDX.PEEK\n\
cue\tSET\tSoundEvents_Dialog.FiredGrenadeDialog\tDX.GREN\n\
dialog\tSoundEvents_Dialog.PeeksFromCover\tTaunts\t0\n\
dialog\tSoundEvents_Dialog.FiredGrenadeDialog\tGrenade\t75\n\
group\tCombat\tDefaultDialogGroup\t100\t1\t2\t1\n\
group\tTaunts\tCombat\t100\t5\t5\t1\n\
group\tGrenade\tCombat\t100\t40\t40\t1\n\
group\tDefaultDialogGroup\t-\t100\t0\t0\t4\n\
default\tDefaultDialogGroup\n";
    const LENGTHS: &str = "length DX.PEEK@A 2\nlength DX.PEEK@B 3\nlength DX.GREN@A 1\n";

    fn soldier(sounds: &AiSounds, entity: Entity, seed: u64) -> Voice {
        let mut rng = Rng::new(seed);
        let mut v = sounds.voice("Soldier", &mut rng);
        let _ = entity;
        v.character = Some(Arc::from("A"));
        v
    }

    /// The game's: a Kaon soldier's footsteps and his lines are in the sound bank.
    #[test]
    fn the_soldiers_sounds_are_in_the_bank() {
        let dir = crate::assets_dir();
        let s = AiSounds::load(&dir);
        let Ok(bank) = std::fs::read_to_string(dir.join("sounds/sounds.txt")) else {
            return;
        };
        if s.sets.is_empty() {
            eprintln!("ai_sounds.txt not built: skipped");
            return;
        }
        let cues: std::collections::HashSet<&str> = bank
            .lines()
            .filter_map(|l| l.strip_prefix("cue "))
            .filter_map(|l| l.split_whitespace().next())
            .collect();
        for character in ["SLDD01", "SLDD02"] {
            let mut v = s.voice("SoldierDec_Kaon", &mut Rng::new(1));
            v.character = Some(Arc::from(character));
            let (step, _) = v
                .find_cue(&s, FOOTSTEP_EVENTS[5])
                .expect("a jog's footstep");
            assert_eq!(step, "BL_FS_LRG_BOT.FS_JOG_DEFAULT");
            assert!(cues.contains(step));
            for event in [
                "PeeksFromCover",
                "Blindfire",
                "FiredGrenadeDialog",
                DEATH_NON_VERBAL,
            ] {
                let (line, secs) = v.find_cue(&s, event).expect(event);
                assert!(line.ends_with(character), "{event}: {line}");
                assert!(cues.contains(line), "{line} not in the bank");
                assert!(secs > 0.2, "{line}: {secs} s");
            }
        }
        // Every set's cue is in the bank (a dialog cue as each speaker's line).
        let mut missing = Vec::new();
        for set in s.sets.values() {
            for cue in set.cues.values() {
                let have = cues.contains(cue)
                    || set
                        .chars
                        .iter()
                        .any(|c| cues.contains(format!("{cue}@{c}").as_str()));
                if !have {
                    missing.push(*cue);
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.len() < 10, "cues not in the bank: {missing:?}");
    }

    /// An animation's voice notify (HmAnimNotify_Dialog, HmPawn.PlayDialog): the
    /// Swarmer's chirps and cries are its dialog character's lines.
    #[test]
    fn a_swarmers_chirp_is_its_own_line() {
        let dir = crate::assets_dir();
        let s = AiSounds::load(&dir);
        if s.sets.is_empty() {
            eprintln!("ai_sounds.txt not built: skipped");
            return;
        }
        let v = s.voice("SwarmerLite", &mut Rng::new(1));
        for cue in [
            "BL_CHR_AI_INSCT_SWARMER.VOX_NAV_CHIRP",
            "BL_CHR_AI_INSCT_SWARMER.VOX_DMG_HIT_RX",
            "BL_CHR_AI_INSCT_SWARMER.VOX_ATTACK_LUNGE",
        ] {
            let (line, secs) = v.line_of(&s, cue);
            assert_eq!(line, format!("{cue}@SWRM01"));
            assert!(secs > 0.5, "{line}: {secs} s");
        }
    }

    #[test]
    fn a_set_gives_each_speaker_his_own_line() {
        let s = AiSounds::parse(DATA, LENGTHS);
        let mut v = soldier(&s, Entity::PLACEHOLDER, 1);
        assert_eq!(
            v.find_cue(&s, "FS_DEFAULT_RUN").map(|c| c.0),
            Some("FS.RUN")
        );
        assert_eq!(v.find_cue(&s, "PeeksFromCover"), Some(("DX.PEEK@A", 2.0)));
        v.character = Some(Arc::from("B"));
        assert_eq!(v.find_cue(&s, "PeeksFromCover"), Some(("DX.PEEK@B", 3.0)));
        assert_eq!(v.find_cue(&s, "Nothing"), None);
    }

    #[test]
    fn groups_keep_quiet_and_lines_outrank_each_other() {
        let s = AiSounds::parse(DATA, LENGTHS);
        let (a, b) = (
            Entity::from_bits(1 << 32 | 5),
            Entity::from_bits(1 << 32 | 6),
        );
        let mut va = soldier(&s, a, 1);
        let mut vb = soldier(&s, b, 2);
        let mut m = DialogManager::default();
        let mut rng = Rng::new(3);
        let mut out = Vec::new();
        let mut speakers = vec![
            Speaker {
                entity: a,
                voice: &mut va,
                at: Vec3::ZERO,
                alive: true,
            },
            Speaker {
                entity: b,
                voice: &mut vb,
                at: Vec3::ZERO,
                alive: true,
            },
        ];
        // A peeks: his line plays, and his group and those above it keep quiet.
        speak(
            &mut m,
            vec![DialogRequest::say(a, "PeeksFromCover")],
            &s,
            &mut speakers,
            0.0,
            &mut rng,
            &mut out,
        );
        assert!(matches!(
            out[..],
            [Say::Line {
                cue: "DX.PEEK@A",
                cut: None,
                ..
            }]
        ));
        out.clear();
        speak(
            &mut m,
            vec![DialogRequest::say(b, "PeeksFromCover")],
            &s,
            &mut speakers,
            0.5,
            &mut rng,
            &mut out,
        );
        assert!(out.is_empty(), "the taunts keep quiet after a line");
        // Long after, B's grenade line: Combat (limit 1) still holds A's line?
        // A's has finished by then (2 s), so B speaks.
        m.tick(10.0);
        speak(
            &mut m,
            vec![DialogRequest::say(b, "FiredGrenadeDialog")],
            &s,
            &mut speakers,
            10.0,
            &mut rng,
            &mut out,
        );
        assert_eq!(out.len(), 1);
    }
}
