//! The game's mixer: its categories, and the presets its sounds turn on, which turn
//! other categories down for a while (tools/build_sounds.py's `mixcategory`,
//! `preset` and `channels` lines in sounds.txt; the game's SND_SoundConfig_p.
//! SoundMixerProperties and HmSoundMixerPresets).
//!
//! Every cue is in a mixer category (sounds.txt's `category`), and the categories
//! make a tree: Master over MASTER_DRY and MASTER_WET, MASTER_WET over SFX_WET and
//! DX_WET, SFX_WET over SFX_WET_COMBAT and the rest. A preset sets some categories'
//! volumes, and a category's volume scales its children's. A cue can turn a preset
//! on as it plays (its SoundNodeRoot's SoundMixerPreset: sounds.txt's `mixer`):
//!
//! * The announcer's lines, Optimus Prime's and Megatron's, turn on GLB_DX_DUCK_9DB:
//!   the effects (SFX_DRY, SFX_WET) and the music (MUSIC_DRY) 9 dB down while the
//!   line plays, the characters' own lines (DX_WET) as they are.
//! * An ability's sounds (BL_TRANS_POWER's: the Barrier going up, a heal, the
//!   Energon Recharger, a Sentry; COMBAT_TRANS_POWER_ACTIVATE): the music, the
//!   guns, footsteps, blasts and vehicles 6 dB down for 0.3 s.
//! * A vehicle's jump (VEH_JUMP) and the end of its boost (VEH_BOOST_END): its
//!   engine down; its boost loop (VEH_BOOST_LP) the ambience down while it plays.
//! * A wave's starting sting (STNG_ESCALATION_WAVE_BEGIN's
//!   MP_ESC_WAVE_COMPLETE_STNG): the effects 6 dB down for 3 s; the player's own
//!   blast damage (COMBAT_EXPL_PLAYER_DMG) the guns 12 dB down for half a second.
//!
//! A preset fades in over its FadeInTime as it comes on, and back out over its
//! FadeOutTime as it goes off: its Duration after it came on, or (a Duration of 0 or
//! -1) once the sounds that turned it on have stopped. Several on at once each turn
//! their categories down (their volumes multiply). A category can turn a preset on
//! itself while many of its sounds play (its ChannelCountSoundMixerPreset): eight
//! robots' shots at once (SFX_WET_COMBAT_ROBOT_WPN_SHOOT) turn the ambience down
//! 16 dB and the footsteps 6 dB.
//!
//! Only the presets' volumes: they also set their categories' filters, reverbs and
//! pitch, which the demo leaves out.

use bevy::prelude::*;
use std::collections::HashMap;

/// A preset that hasn't been heard playing yet stays on this long (its sounds may
/// not have started playing by the frame after it was turned on).
const WAIT_FOR_SOUND_SECS: f32 = 0.5;

/// The game's mixer categories and presets, as sounds.txt has them.
#[derive(Default, Debug)]
pub struct MixerSetup {
    categories: HashMap<String, u16>,
    /// Each category's parent (parents come before their children).
    parents: Vec<Option<u16>>,
    presets: Vec<Preset>,
    preset_names: HashMap<String, u16>,
    channels: Vec<Channels>,
}

#[derive(Debug)]
struct Preset {
    fade_in: f32,
    fade_out: f32,
    /// How long it stays on once turned on (seconds); 0 or less: while the sounds
    /// that turned it on play.
    duration: f32,
    /// The categories it turns down (or up), and their volumes (linear).
    volumes: Vec<(u16, f32)>,
}

/// A preset a category turns on while at least `count` of its sounds play (for
/// `secs` seconds).
#[derive(Debug)]
struct Channels {
    category: u16,
    count: usize,
    secs: f32,
    preset: u16,
}

impl MixerSetup {
    /// Reads one of sounds.txt's mixer lines, `kind` and the words after it: false
    /// if it isn't one. (A category's parent comes before it, and a preset's
    /// categories before the preset, as tools/build_sounds.py writes them.)
    pub fn parse_line(&mut self, kind: &str, rest: &[&str]) -> bool {
        let num = |i: usize| rest.get(i).and_then(|v| v.parse::<f32>().ok());
        match kind {
            "mixcategory" if !rest.is_empty() => {
                let parent = rest.get(1).and_then(|p| self.categories.get(*p).copied());
                let index = self.parents.len() as u16;
                self.parents.push(parent);
                self.categories.insert(rest[0].to_string(), index);
            }
            "preset" if rest.len() >= 4 => {
                let volumes = rest[4..]
                    .chunks(2)
                    .filter_map(|pair| {
                        let category = *self.categories.get(pair[0])?;
                        let db = pair.get(1)?.parse::<f32>().ok()?;
                        Some((category, 10f32.powf(db / 20.0)))
                    })
                    .collect();
                let index = self.presets.len() as u16;
                self.presets.push(Preset {
                    fade_in: num(1).unwrap_or(0.0).max(0.0),
                    fade_out: num(2).unwrap_or(0.0).max(0.0),
                    duration: num(3).unwrap_or(0.0),
                    volumes,
                });
                self.preset_names.insert(rest[0].to_string(), index);
            }
            "channels" if rest.len() >= 4 => {
                if let (Some(&category), Some(&preset)) =
                    (self.categories.get(rest[0]), self.preset_names.get(rest[3]))
                {
                    self.channels.push(Channels {
                        category,
                        count: num(1).unwrap_or(0.0).max(1.0) as usize,
                        secs: num(2).unwrap_or(0.0).max(0.0),
                        preset,
                    });
                }
            }
            _ => return false,
        }
        true
    }

    /// A category, by name.
    pub fn category(&self, name: &str) -> Option<u16> {
        self.categories.get(name).copied()
    }

    /// A preset, by name.
    pub fn preset(&self, name: &str) -> Option<u16> {
        self.preset_names.get(name).copied()
    }

    /// How many presets there are (the size of `Mix::step`'s `playing`).
    pub fn presets(&self) -> usize {
        self.presets.len()
    }

    /// How many categories there are (the size of `Mix::step`'s `counts`).
    pub fn categories(&self) -> usize {
        self.parents.len()
    }
}

/// A preset on (or fading back out).
#[derive(Debug)]
struct Dip {
    preset: u16,
    /// How far it has faded in (0 to 1).
    weight: f32,
    /// When it was (last) turned on, when it goes off (a preset with a Duration), and
    /// whether its sounds have been heard playing since (one without).
    started: f32,
    until: f32,
    heard: bool,
}

/// The presets on, and the categories' volumes they make: what every sound in a
/// category is scaled by (`level`), besides the Audio settings. `play_sounds`
/// moves them on each frame.
#[derive(Resource, Default, Debug)]
pub struct Mix {
    dips: Vec<Dip>,
    levels: Vec<f32>,
    moved: bool,
    /// Since when each of the setup's category-count presets has had its count of
    /// sounds playing.
    crowded: Vec<Option<f32>>,
}

impl Mix {
    /// A category's volume now (linear; 1 for none, or one no preset touches).
    pub fn level(&self, category: Option<u16>) -> f32 {
        category
            .and_then(|c| self.levels.get(c as usize))
            .copied()
            .unwrap_or(1.0)
    }

    /// Whether the categories' volumes changed in the last step (the sounds playing
    /// follow them).
    pub fn moved(&self) -> bool {
        self.moved
    }

    /// Turns `preset` on at `now` (a sound that turns it on starts): one with a
    /// Duration for that long from now, one without until its sounds stop. On
    /// already, it stays on (from now on, for one with a Duration); fading out, it
    /// fades back in from where it is.
    pub fn turn_on(&mut self, setup: &MixerSetup, preset: u16, now: f32) {
        let Some(p) = setup.presets.get(preset as usize) else {
            return;
        };
        let until = now + p.duration.max(0.0);
        match self.dips.iter_mut().find(|d| d.preset == preset) {
            Some(d) => {
                d.started = now;
                d.until = d.until.max(until);
                d.heard = false;
            }
            None => self.dips.push(Dip {
                preset,
                weight: 0.0,
                started: now,
                until,
                heard: false,
            }),
        }
    }

    /// Moves the presets on to `now`, `dt` seconds after the last step: those whose
    /// sounds are playing (`playing`, by preset) stay on, the others go off as they
    /// say, and each fades towards where it is going; `counts` has how many sounds
    /// of each category are playing (for the categories' own presets).
    pub fn step(
        &mut self,
        setup: &MixerSetup,
        now: f32,
        dt: f32,
        playing: &[bool],
        counts: &[usize],
    ) {
        let mut playing = playing.to_vec();
        playing.resize(setup.presets.len(), false);
        // A category with many of its sounds playing turns its preset on.
        self.crowded.resize(setup.channels.len(), None);
        for (i, ch) in setup.channels.iter().enumerate() {
            if counts.get(ch.category as usize).copied().unwrap_or(0) >= ch.count {
                let since = *self.crowded[i].get_or_insert(now);
                if now - since >= ch.secs {
                    if !self.dips.iter().any(|d| d.preset == ch.preset) {
                        self.turn_on(setup, ch.preset, now);
                    }
                    playing[ch.preset as usize] = true;
                }
            } else {
                self.crowded[i] = None;
            }
        }
        for d in self.dips.iter_mut() {
            let Some(p) = setup.presets.get(d.preset as usize) else {
                d.weight = 0.0;
                continue;
            };
            let on = if p.duration > 0.0 {
                now < d.until
            } else if playing[d.preset as usize] {
                d.heard = true;
                true
            } else {
                !d.heard && now - d.started < WAIT_FOR_SOUND_SECS
            };
            d.weight = if on {
                if p.fade_in > 0.0 {
                    (d.weight + dt / p.fade_in).min(1.0)
                } else {
                    1.0
                }
            } else if p.fade_out > 0.0 {
                (d.weight - dt / p.fade_out).max(0.0)
            } else {
                0.0
            };
            // (Off and faded out: it goes.)
            if !on && d.weight <= 0.0 {
                d.weight = -1.0;
            }
        }
        self.dips.retain(|d| d.weight >= 0.0);
        // Each category's volume: its presets' (each as far as it has faded in, in
        // decibels), times its parent's.
        let mut levels = vec![1.0f32; setup.parents.len()];
        for d in &self.dips {
            for &(c, v) in &setup.presets[d.preset as usize].volumes {
                if let Some(l) = levels.get_mut(c as usize) {
                    *l *= v.max(1e-5).powf(d.weight);
                }
            }
        }
        for c in 0..levels.len() {
            if let Some(parent) = setup.parents[c] {
                levels[c] *= levels[parent as usize];
            }
        }
        self.moved = levels != self.levels;
        self.levels = levels;
    }

    /// Every preset off at once (leaving the match: its sounds stop with it).
    pub fn clear(&mut self) {
        self.dips.clear();
        self.crowded.clear();
        self.moved = self.levels.iter().any(|&l| l != 1.0);
        self.levels.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mixer of the game's: its categories' tree, as tools/build_sounds.py writes
    /// it, and a few of its presets.
    fn setup() -> MixerSetup {
        let mut s = MixerSetup::default();
        for line in [
            "mixcategory Master -",
            "mixcategory MASTER_DRY Master",
            "mixcategory SFX_DRY MASTER_DRY",
            "mixcategory MUSIC_DRY MASTER_DRY",
            "mixcategory DX_DRY MASTER_DRY",
            "mixcategory MASTER_WET Master",
            "mixcategory SFX_WET MASTER_WET",
            "mixcategory SFX_WET_COMBAT SFX_WET",
            "mixcategory SFX_WET_COMBAT_ROBOT_WPN_SHOOT SFX_WET_COMBAT",
            "mixcategory SFX_WET_AMB SFX_WET",
            "mixcategory DX_WET MASTER_WET",
            "preset GLB_DX_DUCK_9DB 0.15 0.5 -1 MUSIC_DRY -9 SFX_DRY -9 SFX_WET -9",
            "preset COMBAT_TRANS_POWER_ACTIVATE 0.05 1.5 0.3 SFX_WET_COMBAT -6",
            "preset COMBAT_SHOOT_VOICE_THRESHOLD 1 0.75 -1 SFX_WET_AMB -16",
            "channels SFX_WET_COMBAT_ROBOT_WPN_SHOOT 8 0 COMBAT_SHOOT_VOICE_THRESHOLD",
        ] {
            let mut words = line.split_whitespace();
            let kind = words.next().unwrap();
            let rest: Vec<&str> = words.collect();
            assert!(s.parse_line(kind, &rest), "{line}");
        }
        s
    }

    fn db(level: f32) -> f32 {
        20.0 * level.log10()
    }

    /// Runs `secs` of 60 frames a second from `from`, the preset `held` playing.
    fn run(mix: &mut Mix, s: &MixerSetup, from: f32, secs: f32, held: Option<u16>) -> f32 {
        let mut playing = vec![false; s.presets()];
        if let Some(p) = held {
            playing[p as usize] = true;
        }
        let counts = vec![0; s.categories()];
        let frames = (secs * 60.0).round() as usize;
        let mut now = from;
        for _ in 0..frames {
            now += 1.0 / 60.0;
            mix.step(s, now, 1.0 / 60.0, &playing, &counts);
        }
        now
    }

    /// An announcer's line: the effects and the music fade 9 dB down over 0.15 s
    /// (every effects category under SFX_WET with it), the characters' lines and
    /// the line itself as they are; once it stops they fade back over half a
    /// second.
    #[test]
    fn an_announcers_line_turns_the_effects_and_music_down_9_db() {
        let s = setup();
        let mut mix = Mix::default();
        let duck = s.preset("GLB_DX_DUCK_9DB").unwrap();
        let cat = |n: &str| s.category(n);
        mix.turn_on(&s, duck, 0.0);
        let now = run(&mut mix, &s, 0.0, 0.08, Some(duck));
        let halfway = db(mix.level(cat("SFX_WET_COMBAT")));
        assert!(halfway < -3.0 && halfway > -6.0, "{halfway}");
        let now = run(&mut mix, &s, now, 2.0, Some(duck));
        for c in [
            "SFX_WET",
            "SFX_WET_COMBAT",
            "SFX_WET_AMB",
            "SFX_DRY",
            "MUSIC_DRY",
        ] {
            assert!((db(mix.level(cat(c))) + 9.0).abs() < 0.01, "{c}");
        }
        for c in ["DX_DRY", "DX_WET", "Master"] {
            assert_eq!(mix.level(cat(c)), 1.0, "{c}");
        }
        // (The line ends.)
        let now = run(&mut mix, &s, now, 0.25, None);
        let back = db(mix.level(cat("MUSIC_DRY")));
        assert!(back > -6.0 && back < -3.0, "{back}");
        run(&mut mix, &s, now, 0.3, None);
        assert_eq!(mix.level(cat("MUSIC_DRY")), 1.0);
        assert!(mix.dips.is_empty());
    }

    /// One with a Duration (transforming's) stays on that long whatever its sound
    /// does, then fades back; one without waits a moment for its sound to start
    /// before it goes off for want of it.
    #[test]
    fn a_preset_with_a_duration_stays_on_that_long() {
        let s = setup();
        let mut mix = Mix::default();
        let trans = s.preset("COMBAT_TRANS_POWER_ACTIVATE").unwrap();
        let combat = s.category("SFX_WET_COMBAT");
        mix.turn_on(&s, trans, 0.0);
        let now = run(&mut mix, &s, 0.0, 0.25, None);
        assert!((db(mix.level(combat)) + 6.0).abs() < 0.01);
        // (Off at 0.3 s, a fifth of the way back by 0.6 s: its fade out is 1.5 s.)
        run(&mut mix, &s, now, 0.35, None);
        let back = db(mix.level(combat));
        assert!(back > -6.0 && back < -4.0, "{back}");
        // A line that never turns up: the duck goes after WAIT_FOR_SOUND_SECS.
        let mut mix = Mix::default();
        let duck = s.preset("GLB_DX_DUCK_9DB").unwrap();
        mix.turn_on(&s, duck, 0.0);
        run(&mut mix, &s, 0.0, 0.3, None);
        assert!(db(mix.level(s.category("SFX_WET"))) < -8.9);
        run(&mut mix, &s, 0.3, 1.0, None);
        assert_eq!(mix.level(s.category("SFX_WET")), 1.0);
    }

    /// Two at once both turn their categories down; a category's own preset comes
    /// on once eight of its sounds play.
    #[test]
    fn presets_on_at_once_both_turn_down_and_crowds_turn_theirs_on() {
        let s = setup();
        let mut mix = Mix::default();
        mix.turn_on(&s, s.preset("GLB_DX_DUCK_9DB").unwrap(), 0.0);
        mix.turn_on(&s, s.preset("COMBAT_TRANS_POWER_ACTIVATE").unwrap(), 0.0);
        let mut playing = vec![false; s.presets()];
        playing[s.preset("GLB_DX_DUCK_9DB").unwrap() as usize] = true;
        let mut counts = vec![0; s.categories()];
        let shots = s.category("SFX_WET_COMBAT_ROBOT_WPN_SHOOT").unwrap() as usize;
        counts[shots] = 8;
        let mut now = 0.0;
        for _ in 0..12 {
            now += 1.0 / 60.0;
            mix.step(&s, now, 1.0 / 60.0, &playing, &counts);
        }
        assert!((db(mix.level(s.category("SFX_WET_COMBAT"))) + 15.0).abs() < 0.01);
        // (The shots' crowd, its 1 s fade in under way: the ambience going down.)
        let amb = db(mix.level(s.category("SFX_WET_AMB")));
        assert!(amb < -9.5 && amb > -16.0, "{amb}");
        counts[shots] = 7;
        for _ in 0..120 {
            now += 1.0 / 60.0;
            mix.step(&s, now, 1.0 / 60.0, &playing, &counts);
        }
        assert!((db(mix.level(s.category("SFX_WET_AMB"))) + 9.0).abs() < 0.01);
    }
}
