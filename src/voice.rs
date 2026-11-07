//! The characters' voices: the multiplayer's robot voices, which the Customize
//! screen's Select Voice picks for a build (TnDataProvider_RobotVoice: Voice 1, 2
//! or 3 of its class), and None, the demo's own: no voice at all.
//!
//! Each voice is a pair of the game's HmSoundEventSets, the Autobots' and the
//! Decepticons' (TransCustomization.ini's ObjectPath and DecepticonObjectPath:
//! SoundEvents.Multiplayer.CHR_MP_DX_AUTOBOT_0n and CHR_MP_DX_DECEPTICON_0n, Voice n
//! of every class), and a character speaks with his side's (on Downfall and Oblivion
//! the Decepticons'). A set gives each dialog event it answers (HmSoundEvent) a cue
//! of recorded lines, one picked at random as it plays: tools/build_sounds.py writes
//! the sets, the events' dialog groups and the groups' rules into
//! assets/sounds/voices.txt.
//!
//! As the game's players' dialog does (their pawns' IncidentalDialogService), a
//! character speaks when a reload begins (ReloadBeginDialog) and when it is done
//! (ReloadCompleteDialog), when he picks up a weapon, ammo or health, when he kills an
//! enemy (a headshot, a melee blow or the Gear Shredder have lines of their own;
//! otherwise his weapon's damage type's DialogDamageCategory picks the gun's, the
//! rocket's or the explosive's line, TnPlayerController.PlayTargetKilledDialog),
//! when he fires a grenade launcher, when he is hit (a grunt), when his health falls
//! below a tenth (TnPlayerPawn.UpdateHealthDialog: once each time it does), when he
//! dies (a death cry), when he swings a melee blow (a grunt), and when one of his
//! teammates dies. Whether he says it is up to the event's dialog group
//! (HmSoundEventDialogGroup): once a group has spoken it keeps quiet for a while
//! (ReTriggerDelayMin to ReTriggerDelayMax seconds), and so does each group above it
//! (the groups make a tree: AI_States_In_Combat_Group, over the kills, reloads and
//! grenades, keeps them a second or two apart); as he comes into the match each group
//! starts quiet for such a while unless a roll of its InitialChanceToPlay says
//! otherwise (GetInitialDelay). He says one line at a time; his death cry cuts in.
//!
//! The lines are this PC's player's: they go to the other players with his other
//! sounds (main.rs's net_capture), and their PCs play them at his Jazz.

use crate::sound::Sfx;
use bevy::prelude::*;
use std::collections::HashMap;
use std::path::Path;

/// A build's voice when it has none (the Select Voice list's None).
pub const NONE: &str = "None";

/// Health under this share of the most (TnPlayerPawn.UpdateHealthDialog's 0.1):
/// low (HealthIsLowDialog).
const LOW_HEALTH: f32 = 0.1;

/// What a character speaks about: the dialog events the demo's matches have.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Line {
    ReloadBegin,
    ReloadComplete,
    AmmoPickup,
    WeaponPickup,
    HealthPickup,
    Hit,
    HealthLow,
    Death,
    MeleeSwing,
    FiredGrenade,
    KilledByGun,
    KilledByHeadshot,
    KilledByMelee,
    KilledByShredder,
    KilledByRocket,
    KilledByExplosive,
    TeammateKilled,
}

impl Line {
    /// Its dialog event (SoundEvents_Dialog's).
    pub fn event(self) -> &'static str {
        match self {
            Line::ReloadBegin => "ReloadBeginDialog",
            Line::ReloadComplete => "ReloadCompleteDialog",
            Line::AmmoPickup => "AmmoPickupObtainedDialog",
            Line::WeaponPickup => "WeaponPickupObtainedDialog",
            Line::HealthPickup => "HealthPickupObtainedDialog",
            Line::Hit => "HitRX_NonVerbalDialog",
            Line::HealthLow => "HealthIsLowDialog",
            Line::Death => "DeathRX_NonVerbalDialog",
            Line::MeleeSwing => "MeleeWeaponSwing_NonVerbalDialog",
            Line::FiredGrenade => "FiredGrenadeDialog",
            Line::KilledByGun => "TargetKilledByGunDialog",
            Line::KilledByHeadshot => "TargetKilledByHeadshotDialog",
            Line::KilledByMelee => "TargetKilledByMeleeDialog",
            Line::KilledByShredder => "TargetKilledByShredderDialog",
            Line::KilledByRocket => "TargetKilledByRocketDialog",
            Line::KilledByExplosive => "TargetKilledByExplosiveDialog",
            Line::TeammateKilled => "TeammateKilledDialog",
        }
    }

    /// The line for a kill (`death`: what the last blow did; `head`: a headshot;
    /// `category`: the weapon in hand's damage type's DialogDamageCategory): a
    /// headshot's, a melee blow's or the Gear Shredder's, else by the category, a
    /// gun's, a rocket's or an explosive's line. A burn, an EMP blast, the Path
    /// Blaster's shockwave and DDC_None weapons have none (the game's default line,
    /// which the multiplayer's voices don't answer).
    pub fn for_kill(
        death: crate::enemies::Death,
        head: bool,
        category: DialogCategory,
    ) -> Option<Line> {
        use crate::enemies::Death;
        match death {
            Death::Gunfire | Death::SingleShot if head => Some(Line::KilledByHeadshot),
            Death::Melee => Some(Line::KilledByMelee),
            Death::Dismember => Some(Line::KilledByShredder),
            Death::Plain | Death::Shockwave | Death::Fire => None,
            _ => match category {
                DialogCategory::Gun => Some(Line::KilledByGun),
                DialogCategory::Rocket => Some(Line::KilledByRocket),
                DialogCategory::Explosive => Some(Line::KilledByExplosive),
                DialogCategory::None => None,
            },
        }
    }
}

/// A damage type's DialogDamageCategory (TnDamageType.ETnDialogDamageCategory):
/// which of the kill lines its kills get.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DialogCategory {
    None,
    Gun,
    Rocket,
    Explosive,
}

/// A dialog group's rules (HmSoundEventDialogGroup).
#[derive(Clone, Debug, PartialEq)]
struct Group {
    parent: Option<String>,
    /// InitialChanceToPlay (%): the chance it may speak at once as a character comes
    /// in (otherwise it starts quiet for a retrigger delay).
    chance: u32,
    /// ReTriggerDelayMin and Max: how long it keeps quiet after speaking.
    delay: (f32, f32),
}

/// voices.txt: each set's events' cues and their lines' longest recording, and
/// the events' dialog groups.
#[derive(Resource, Default)]
pub struct VoiceData {
    sets: HashMap<String, HashMap<String, &'static str>>,
    lengths: HashMap<&'static str, f32>,
    events: HashMap<String, Option<String>>,
    groups: HashMap<String, Group>,
}

impl VoiceData {
    /// Reads `<sounds_dir>/voices.txt` (no voices without it).
    pub fn load(sounds_dir: &Path) -> Self {
        let mut data = VoiceData::default();
        let Ok(text) = std::fs::read_to_string(sounds_dir.join("voices.txt")) else {
            info!("No voices.txt in {sounds_dir:?}: the characters say nothing");
            return data;
        };
        data.read(&text);
        data
    }

    fn read(&mut self, text: &str) {
        // (The cue names live as long as the sound bank's.)
        let mut names: HashMap<String, &'static str> = HashMap::new();
        let mut name = |s: &str| -> &'static str {
            names
                .entry(s.to_string())
                .or_insert_with(|| Box::leak(s.to_string().into_boxed_str()))
        };
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
            let opt = |i: usize| f.get(i).filter(|v| **v != "-").map(|v| v.to_string());
            match f.first().copied() {
                Some("voice") if f.len() >= 4 => {
                    let cue = name(f[3]);
                    self.sets
                        .entry(f[1].to_string())
                        .or_default()
                        .insert(f[2].to_string(), cue);
                }
                Some("length") if f.len() >= 3 => {
                    self.lengths.insert(name(f[1]), num(2));
                }
                Some("event") if f.len() >= 3 => {
                    self.events.insert(f[1].to_string(), opt(2));
                }
                Some("group") if f.len() >= 6 => {
                    self.groups.insert(
                        f[1].to_string(),
                        Group {
                            parent: opt(2),
                            chance: num(3) as u32,
                            delay: (num(4), num(5).max(num(4))),
                        },
                    );
                }
                _ => {}
            }
        }
    }

    /// The cue a set says `line` with, if it has one.
    pub fn cue(&self, set: &str, line: Line) -> Option<&'static str> {
        self.sets.get(set)?.get(line.event()).copied()
    }

    /// The groups an event's line goes through: its own, then each above it.
    fn chain(&self, event: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut group = self.events.get(event).and_then(|g| g.as_deref());
        while let Some(g) = group {
            if out.contains(&g) {
                break;
            }
            out.push(g);
            group = self.groups.get(g).and_then(|g| g.parent.as_deref());
        }
        out
    }
}

/// The voice set a build's voice speaks with on a side (`voice` is a
/// TnDataProvider_RobotVoice's id, <Class>Voice<n>, whose ObjectPath and
/// DecepticonObjectPath are the sets 0<n>; Voice 1's without one, as Jazz plays
/// without the Choose Character screen); none for None.
pub fn voice_set(voice: &str, decepticon: bool) -> Option<String> {
    if voice == NONE {
        return None;
    }
    let n = voice
        .chars()
        .last()
        .and_then(|c| c.to_digit(10))
        .unwrap_or(1);
    let side = if decepticon { "DECEPTICON" } else { "AUTOBOT" };
    Some(format!("CHR_MP_DX_{side}_0{n}"))
}

/// The lines this PC's players are to say (asked for by the game's systems; spoken
/// by `speak` as the dialog groups allow), each with its player's seat (split
/// screen's guest speaks his own).
#[derive(Resource, Default)]
pub struct VoiceLines(Vec<(usize, Line)>);

impl VoiceLines {
    /// A line for this PC's (first) player.
    pub fn say(&mut self, line: Line) {
        self.0.push((0, line));
    }

    /// A line for the player in `seat` (split.rs).
    pub fn say_for(&mut self, seat: usize, line: Line) {
        self.0.push((seat, line));
    }
}

/// This PC's player's dialog: when each group may speak again, until when he is
/// speaking, his voice's set, and what he was like last frame (for the lines that
/// follow his state). (Split screen's guest has his own: `split::Second`.)
#[derive(Resource, Default)]
pub struct Dialog {
    quiet_until: HashMap<String, f32>,
    speaking_until: f32,
    set: Option<String>,
    last: Option<Last>,
    seed: u32,
}

#[derive(Clone, Copy, Debug)]
struct Last {
    alive: bool,
    reload_left: f32,
    reload_slot: usize,
    clip: u32,
    hurt: f32,
    low: bool,
    swinging: bool,
}

impl Dialog {
    fn rand(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    /// He comes into the match (or back in): each group starts quiet for a
    /// retrigger delay unless its InitialChanceToPlay roll lets it speak at once.
    fn start(&mut self, data: &VoiceData, now: f32) {
        self.quiet_until.clear();
        self.speaking_until = 0.0;
        let names: Vec<String> = data.groups.keys().cloned().collect();
        for name in names {
            let group = &data.groups[&name];
            let (chance, delay) = (group.chance, group.delay);
            if (self.rand() * 100.0) < chance as f32 {
                continue;
            }
            let wait = delay.0 + (delay.1 - delay.0) * self.rand();
            self.quiet_until.insert(name, now + wait);
        }
    }

    /// Says `line` if his groups let him (and he isn't speaking): its cue.
    fn try_say(&mut self, data: &VoiceData, line: Line, now: f32) -> Option<&'static str> {
        let cue = data.cue(self.set.as_deref()?, line)?;
        if line != Line::Death && now < self.speaking_until {
            return None;
        }
        let chain = data.chain(line.event());
        if chain
            .iter()
            .any(|g| self.quiet_until.get(*g).is_some_and(|&t| now < t))
        {
            return None;
        }
        for g in chain {
            let delay = data.groups.get(g).map_or((0.0, 0.0), |g| g.delay);
            if delay.1 > 0.0 {
                let wait = delay.0 + (delay.1 - delay.0) * self.rand();
                self.quiet_until.insert(g.to_string(), now + wait);
            }
        }
        self.speaking_until = now + data.lengths.get(cue).copied().unwrap_or(1.5);
        Some(cue)
    }
}

/// Speaks this PC's players' lines (split screen's guest his own, in his own voice):
/// those asked for, and those each one's state brings (a reload beginning and
/// ending, being hit, his health falling low, his death, a melee swing, a grenade
/// launcher fired), and a teammate's death. Runs before main.rs's net_capture, so
/// the lines go out with his other sounds.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn speak(
    time: Res<Time>,
    data: Res<VoiceData>,
    chosen: crate::split::BySeatRef<crate::choose::Chosen>,
    mut lines: ResMut<VoiceLines>,
    mut dialogs: crate::split::BySeat<Dialog>,
    mut sfx: ResMut<Sfx>,
    players: Query<
        (
            Entity,
            &crate::Player,
            Has<crate::LocalPlayer>,
            Option<&crate::split::Seat>,
        ),
        Or<(With<crate::LocalPlayer>, With<crate::RemotePlayer>)>,
    >,
    mut was_dead: Local<HashMap<Entity, bool>>,
) {
    let now = time.elapsed_secs();
    let asked_all = std::mem::take(&mut lines.0);
    // The players going down this frame (a teammate's death is said by the others).
    let mut went_down: Vec<Entity> = Vec::new();
    for (e, p, ..) in &players {
        let dead = p.dead_left > 0.0 || p.dying;
        let was = was_dead.insert(e, dead).unwrap_or(dead);
        if dead && !was && !p.waiting {
            went_down.push(e);
        }
    }
    was_dead.retain(|e, _| players.contains(*e));
    for seat in 0..crate::split::SEATS {
        let dialog = dialogs.seat(seat);
        let Some((me, p, ..)) = players
            .iter()
            .find(|(_, _, local, s)| *local && crate::split::seat_of(*s) == seat)
        else {
            dialog.last = None;
            continue;
        };
        if p.waiting {
            dialog.last = None;
            continue;
        }
        let mut asked: Vec<Line> = asked_all
            .iter()
            .filter(|(s, _)| *s == seat)
            .map(|(_, l)| *l)
            .collect();
        dialog.set = voice_set(&chosen.seat(seat).voice, p.look.decepticon);
        let alive = p.dead_left <= 0.0 && !p.dying;
        let slot = crate::gun_slot(p);
        let now_state = Last {
            alive,
            reload_left: p.reload_left,
            reload_slot: p.reload_slot,
            clip: p.ammo.get(p.reload_slot).map_or(0, |a| a.0),
            hurt: p.health + p.shield,
            low: p.health < p.max_health() * LOW_HEALTH,
            swinging: p.melee_winding_up(),
        };
        match dialog.last {
            // In he comes: his groups start as GetInitialDelay has them.
            None => {
                dialog.seed ^= (now.to_bits()).rotate_left(11 + seat as u32) ^ 0x9E37_79B9;
                dialog.start(&data, now);
            }
            Some(last) if alive && !last.alive => dialog.start(&data, now),
            Some(last) => {
                if alive {
                    if last.reload_left <= 0.0 && p.reload_left > 0.0 && p.reload_slot == slot {
                        asked.push(Line::ReloadBegin);
                    }
                    if last.reload_left > 0.0
                        && p.reload_left <= 0.0
                        && last.reload_slot == p.reload_slot
                        && now_state.clip > last.clip
                    {
                        asked.push(Line::ReloadComplete);
                    }
                    if now_state.hurt < last.hurt - 0.01 {
                        asked.push(Line::Hit);
                    }
                    if now_state.low && !last.low {
                        asked.push(Line::HealthLow);
                    }
                    if now_state.swinging && !last.swinging {
                        asked.push(Line::MeleeSwing);
                    }
                    // A grenade launcher fired (TnWeaponGrenadeLauncherBase's
                    // OnBeginStateWeaponFiring: the A-4 Pulsar Cannon's and the
                    // Chaos-Rift Combustor's rounds).
                    let launched = p.played.iter().any(|e| match e {
                        crate::net::JazzEvent::Round { weapon, .. }
                        | crate::net::JazzEvent::Lob { weapon, .. } => crate::WEAPONS
                            .get(*weapon as usize)
                            .is_some_and(|w| w.gear.is_some_and(|g| g.detonate.is_some())),
                        _ => false,
                    });
                    if launched {
                        asked.push(Line::FiredGrenade);
                    }
                } else if last.alive {
                    // (His death cry, and nothing after it.)
                    asked.clear();
                    asked.push(Line::Death);
                }
            }
        }
        dialog.last = Some(now_state);

        // A teammate's death (another player's Jazz going down on this PC).
        if alive && went_down.iter().any(|&e| e != me) {
            asked.push(Line::TeammateKilled);
        }

        for line in asked {
            if !alive && line != Line::Death {
                continue;
            }
            if let Some(cue) = dialog.try_say(&data, line, now) {
                sfx.play(cue);
            }
        }
    }
}

/// The Customize screen's preview of a voice (TnCharacterScriptBinding's
/// PreviewSound, as the preview character's voice alone changes: OnlyNeedsVoiceChange):
/// the voice's line for killing an enemy with a gun, on its side's set.
pub fn preview_cue(data: &VoiceData, voice: &str, decepticon: bool) -> Option<&'static str> {
    data.cue(&voice_set(voice, decepticon)?, Line::KilledByGun)
}

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(VoiceData::load(&crate::assets_dir().join("sounds")))
            .init_resource::<VoiceLines>()
            .init_resource::<Dialog>()
            .init_resource::<crate::split::Second<Dialog>>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "\
voice CHR_MP_DX_AUTOBOT_01 ReloadBeginDialog BL_DX_SWITCHBOARD.SC016813
voice CHR_MP_DX_AUTOBOT_01 TargetKilledByGunDialog BL_DX_SWITCHBOARD.SC016888
voice CHR_MP_DX_AUTOBOT_01 TargetKilledByRocketDialog BL_DX_SWITCHBOARD.SC016903
length BL_DX_SWITCHBOARD.SC016813 1.25
event ReloadBeginDialog ReloadWeaponBeginGroup 75
event TargetKilledByGunDialog TargetKilledGroup 75
event TargetKilledByRocketDialog TargetKilledGroup 75
group ReloadWeaponBeginGroup AI_States_In_Combat_Group 33 15 55 1
group TargetKilledGroup AI_States_In_Combat_Group 65 25 65 1
group AI_States_In_Combat_Group AI_States_Non_Death_Group 100 1 2 1
group AI_States_Non_Death_Group DefaultDialogGroup 100 0 0 1
group DefaultDialogGroup - 100 0 0 4
";

    fn data() -> VoiceData {
        let mut d = VoiceData::default();
        d.read(TEXT);
        d
    }

    #[test]
    fn voices_pair_with_their_sets() {
        assert_eq!(
            voice_set("ScoutVoice2", false).as_deref(),
            Some("CHR_MP_DX_AUTOBOT_02")
        );
        assert_eq!(
            voice_set("SoldierVoice3", true).as_deref(),
            Some("CHR_MP_DX_DECEPTICON_03")
        );
        assert_eq!(voice_set(NONE, false), None);
        let d = data();
        assert_eq!(
            preview_cue(&d, "LeaderVoice1", false),
            Some("BL_DX_SWITCHBOARD.SC016888")
        );
        assert_eq!(preview_cue(&d, NONE, false), None);
        assert_eq!(
            d.chain("ReloadBeginDialog"),
            [
                "ReloadWeaponBeginGroup",
                "AI_States_In_Combat_Group",
                "AI_States_Non_Death_Group",
                "DefaultDialogGroup"
            ]
        );
    }

    #[test]
    fn groups_keep_quiet_after_speaking() {
        let d = data();
        let mut dialog = Dialog {
            set: Some("CHR_MP_DX_AUTOBOT_01".into()),
            ..default()
        };
        // (Every group free to speak at once.)
        assert_eq!(
            dialog.try_say(&d, Line::ReloadBegin, 0.0),
            Some("BL_DX_SWITCHBOARD.SC016813")
        );
        // Speaking: nothing else till the line is over (its recording's 1.25 s).
        assert_eq!(dialog.try_say(&d, Line::KilledByGun, 1.0), None);
        // The combat group above keeps quiet a second or two after a line.
        let combat = dialog.quiet_until["AI_States_In_Combat_Group"];
        assert!((1.0..=2.0).contains(&combat));
        assert!(dialog.try_say(&d, Line::KilledByGun, 2.01).is_some());
        // The kills' group keeps quiet 25 to 65 seconds: the rocket's line shares it.
        assert_eq!(dialog.try_say(&d, Line::KilledByRocket, 20.0), None);
        assert!(dialog.try_say(&d, Line::KilledByRocket, 100.0).is_some());
        // A set without the event says nothing.
        assert_eq!(dialog.try_say(&d, Line::HealthPickup, 200.0), None);
    }
}
