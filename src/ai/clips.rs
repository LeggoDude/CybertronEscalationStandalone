//! The AI's clips (tools/build_ai_anims.py's models/enemies/ai_anims_<set>.txt, a
//! set for each anim tree the enemy types play: the soldiers', the Shotgunners',
//! the Rocket Troopers', the Scrappers', the Titans', the Guard's, the Leaper's):
//! each clip's length, rate and kind and where it is in ai_<set>.glb; the chooser
//! groups; the notifies the AI acts on; the custom animations' root motion; and
//! what the anim tree's emulation (tree.rs) needs of the tree: its nodes over its
//! base pose (its custom slots, its masked slots' bones, its aimer, its jump blend,
//! its mirror), its velocity blends, its jump blend, its stance profiles and their
//! transitions, its velocity switch, its acrobatics slot's animations and its
//! aimer's poses.
//!
//! Unreal's names are case-blind, so clips and groups are found by their lower
//! case names.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use bevy::prelude::*;

use super::anim::{self, ClipBook};
use super::pawn::stance;
use super::tree::Layer;

/// The sets (each one's files: models/enemies/ai_anims_<set>.txt, ai_<set>.glb and,
/// for a tree that mirrors, ai_<set>_mirror.glb).
pub const SETS: &[&str] = &[
    "soldier",
    "shotgunner",
    "rocket",
    "scrapper",
    "titan",
    "guard",
    "leaper",
    "swarmer",
    "drone",
    // The Snipers' two forms (forms.rs): the robot's, the insect's.
    "sniper",
    "sniper_alt",
    // The jets' two forms: the robot's (on the soldiers' tree), the plane's.
    "flyer",
    "flyer_alt",
];

/// A set's data file, its clips and its mirrored clips.
pub fn anims_file(set: &str) -> String {
    format!("models/enemies/ai_anims_{set}.txt")
}

pub fn model(set: &str) -> String {
    format!("models/enemies/ai_{set}.glb")
}

pub fn mirror_model(set: &str) -> String {
    format!("models/enemies/ai_{set}_mirror.glb")
}

/// How the tree mirrors its pose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mirror {
    /// It doesn't.
    #[default]
    None,
    /// TnAnimNodeHandednessMirror: left for right while the pawn holds its gun
    /// left-handed.
    Handedness,
    /// TnAnimNodeMirror (the Leaper's): nothing of the AI's mirrors it.
    Node,
}

/// A masked slot (an AnimNodeBlendMultiBone's custom slot): the bones it takes over
/// (each one's path of names from the model's root) and the clips its types play
/// in it.
#[derive(Clone, Debug, Default)]
pub struct Mask {
    pub paths: Vec<Vec<String>>,
    pub clips: Vec<usize>,
}

/// What a clip is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipKind {
    /// Poses (the bones' own places and turns).
    Pose,
    /// An additive clip (ADD_*: changes put on the pose under it).
    Additive,
    /// One of the aimer's aim poses, as changes from its reference pose.
    Aim,
}

/// A clip's notify the AI acts on: when in the clip (the sequence's time), and
/// what.
#[derive(Clone, Debug)]
pub enum ClipNotify {
    /// TnAnimNotify_HtnEvent: the controller's AnimNotify event (AnimEvent: 0 a
    /// grenade's release) and its sockets.
    Htn {
        at: f32,
        event: u8,
        socket: Arc<str>,
        mirrored: Arc<str>,
    },
    /// TnAnimNotify_DamageSweep: a melee blow's box (its centre from the pawn's
    /// middle and its half sizes, uu, in the pawn's axes) for `secs` (the game's).
    Sweep {
        at: f32,
        secs: f32,
        offset: Vec3,
        half: Vec3,
    },
    /// TnAnimNotify_ImpactDamageSweep: the melee service's impact sweep (a box of
    /// `half` sizes, uu, on its socket, swept as it moves for `secs`), each player
    /// it meets taking its damage (DamageOverride) of its type and its push
    /// (HitMomentum: forward from the pawn along the ground, sideways, up).
    Impact {
        at: f32,
        secs: f32,
        socket: Arc<str>,
        half: Vec3,
        damage: f32,
        damage_type: Arc<str>,
        forward: f32,
        sideways: f32,
        vertical: f32,
    },
    /// TnAnimNotify_AIFireRound: one round, along the gun if `along_gun`.
    Round { at: f32, along_gun: bool },
    /// TnAnimNotify_AIFireWeapon: the gun fires for the duration (the sequence's
    /// time): blind fire.
    Burst { at: f32, secs: f32 },
    /// AnimNotify_Footstep: a footstep of `foot` (0 left, 1 right), its
    /// EFootstepType (`kind`: audio.rs's FOOTSTEP_EVENTS).
    Step {
        at: f32,
        foot: u8,
        kind: u8,
        min_weight: f32,
    },
    /// HmAnimNotify_Sound: a cue, at a socket.
    Sound {
        at: f32,
        cue: &'static str,
        socket: Arc<str>,
        min_weight: f32,
    },
    /// HmAnimNotify_SoundEvent: a sound event (its cue the pawn's sound set's), at
    /// a socket.
    Event {
        at: f32,
        event: Arc<str>,
        socket: Arc<str>,
        min_weight: f32,
    },
    /// HmAnimNotify_Dialog (and HmAnimNotify_DialogDeath: `death`, the pawn's
    /// dialog shut off after it): a line of dialog, through the dialog manager.
    Dialog {
        at: f32,
        event: Arc<str>,
        socket: Arc<str>,
        min_weight: f32,
        death: bool,
    },
    /// An HmAnimNotify_Dialog with a cue of its own (HmPawn.PlayDialog): the cue
    /// spoken in the pawn's voice, cutting short the one before, no dialog rules
    /// (the Swarmers' chirps).
    Voice {
        at: f32,
        cue: &'static str,
        socket: Arc<str>,
        min_weight: f32,
    },
    /// HmAnimNotify_PlayEffect: a particle system on a socket, held on it if
    /// `attached` (bAttachToMesh), else left where it starts; `offset` and `turn`
    /// its place and turn in the socket's frame (RelativeLocation, uu, and
    /// RelativeRotation: pitch, yaw and roll, Unreal's rotation units); `params`
    /// what its Parameters set as it starts (see shared::Effect). `auto_off`: for
    /// one turned off with its clip (bAutoDeactivate), how long after it starts:
    /// the clip's playback length and AUTO_OFF_PAD (GetAnimPlaybackLength's timer,
    /// as its clip plays at its RateScale; anim.rs makes it its player's, and no
    /// later than the clip's end, OnAnimSequenceStopped); None for one that plays
    /// on till it ends by itself.
    Effect {
        at: f32,
        system: Arc<str>,
        socket: Arc<str>,
        attached: bool,
        min_weight: f32,
        offset: Vec3,
        turn: Vec3,
        params: Option<Arc<[Arc<str>]>>,
        auto_off: Option<f32>,
    },
    /// TnAnimNotify_SpawnProjectile: `count` rounds of a projectile class fired
    /// from a socket (TnProjectileSpawner.SpawnProjectile: along the socket, turned
    /// at random within `cone` degrees, at the AI's enemy).
    Projectile {
        at: f32,
        socket: Arc<str>,
        class: Arc<str>,
        cone: f32,
        count: u32,
    },
    /// TnAnimNotify_ToggleHidden: the form whose model plays it hidden or shown (a
    /// change of form's halves: forms.rs).
    Toggle { at: f32, hide: bool },
    /// TnAnimNotify_ToggleVehicleFx: its alt form's effects allowed or not.
    VehicleFx { at: f32, allowed: bool },
}

impl ClipNotify {
    pub fn at(&self) -> f32 {
        match *self {
            ClipNotify::Htn { at, .. }
            | ClipNotify::Sweep { at, .. }
            | ClipNotify::Impact { at, .. }
            | ClipNotify::Round { at, .. }
            | ClipNotify::Burst { at, .. }
            | ClipNotify::Step { at, .. }
            | ClipNotify::Sound { at, .. }
            | ClipNotify::Event { at, .. }
            | ClipNotify::Dialog { at, .. }
            | ClipNotify::Voice { at, .. }
            | ClipNotify::Effect { at, .. }
            | ClipNotify::Projectile { at, .. }
            | ClipNotify::Toggle { at, .. }
            | ClipNotify::VehicleFx { at, .. } => at,
        }
    }

    /// A sound (a footstep, a cue, a sound event, a line of dialog) or an effect:
    /// the notifies the anim tree's own nodes have too (the rest are the custom
    /// animations').
    pub fn in_tree(&self) -> bool {
        matches!(
            self,
            ClipNotify::Step { .. }
                | ClipNotify::Sound { .. }
                | ClipNotify::Event { .. }
                | ClipNotify::Dialog { .. }
                | ClipNotify::Voice { .. }
                | ClipNotify::Effect { .. }
        )
    }

    /// The weight its node must have in the tree for it to play (its MinWeight:
    /// the sounds' and the effects'; the others always play).
    pub fn min_weight(&self) -> f32 {
        match *self {
            ClipNotify::Step { min_weight, .. }
            | ClipNotify::Sound { min_weight, .. }
            | ClipNotify::Event { min_weight, .. }
            | ClipNotify::Dialog { min_weight, .. }
            | ClipNotify::Voice { min_weight, .. }
            | ClipNotify::Effect { min_weight, .. } => min_weight,
            _ => 0.0,
        }
    }
}

/// How much longer than its clip's playback length an effect turned off with its
/// clip plays on (HmAnimNotify_PlayEffect.Notify's AutoDeactivateTime: the
/// length and 0.04 s).
pub const AUTO_OFF_PAD: f32 = 0.04;

/// A sound notify's socket ("-" for none: its pawn's own place).
fn socket(s: &str) -> Arc<str> {
    Arc::from(if s.trim() == "-" { "" } else { s.trim() })
}

/// A clip's root bone's motion each frame from its first: forward, right and up
/// (uu at the mesh's own size) and its yaw (degrees, rightwards); `dt` the clip's
/// own seconds between its frames.
#[derive(Clone, Debug, Default)]
pub struct RootMotion {
    pub dt: f32,
    pub frames: Vec<[f32; 4]>,
}

impl RootMotion {
    /// The motion at `t` (the sequence's time), between its frames.
    pub fn at(&self, t: f32) -> [f32; 4] {
        if self.frames.is_empty() {
            return [0.0; 4];
        }
        let f = (t / self.dt.max(1e-6)).max(0.0);
        let i = (f.floor() as usize).min(self.frames.len() - 1);
        let j = (i + 1).min(self.frames.len() - 1);
        let k = (f - i as f32).clamp(0.0, 1.0);
        let (a, b) = (self.frames[i], self.frames[j]);
        [
            a[0] + (b[0] - a[0]) * k,
            a[1] + (b[1] - a[1]) * k,
            a[2] + (b[2] - a[2]) * k,
            a[3] + (b[3] - a[3]) * k,
        ]
    }
}

/// A clip: its name, its SequenceLength and RateScale, its place in the clips'
/// files, its kind, its notifies, where its first pose aims the aimer's bone (the
/// aimer's units) and its root motion.
#[derive(Clone, Debug)]
pub struct ClipInfo {
    pub name: Arc<str>,
    pub secs: f32,
    pub rate: f32,
    pub index: usize,
    pub kind: ClipKind,
    pub notifies: Vec<ClipNotify>,
    pub aim: Vec2,
    pub root: Option<RootMotion>,
}

/// A TnLinearVelocityBlend's child: its node's name, its sequence (by clip), its
/// chooser group, its sample velocity (uu a second: forward, right) and whether it
/// plays in the blends' synch group ('Strafers': all at one place in their cycles)
/// or in its own time.
#[derive(Clone, Debug)]
pub struct VChild {
    pub name: String,
    pub clip: Option<usize>,
    pub group: Option<Arc<str>>,
    pub velocity: Vec2,
    pub synched: bool,
    /// A TnStanceAnimNodeSequence's clips by stance (its picker's; its own clip
    /// for the rest).
    pub picks: Vec<(u8, usize)>,
}

/// A TnLinearVelocityBlend: its name, the stance profile it serves, its samples'
/// triangles and its children (the first its idle). A TnStraferAnimBlend is one
/// with no triangles and its BlendSpeed (`strafer`): its forward, backward, right
/// and left children weighted by the way the pawn moves. A TnFlyingRobotAnimBlend
/// (`fly`) is one with seven children: its idle, then forward, backward, right,
/// left, up and down, weighted by the way the pawn flies.
#[derive(Clone, Debug, Default)]
pub struct VBlend {
    pub name: String,
    pub stance: String,
    pub tris: Vec<[usize; 3]>,
    pub children: Vec<VChild>,
    pub strafer: Option<f32>,
    pub fly: Option<FlyBlend>,
}

/// A TnFlyingRobotAnimBlend's values: its weights' BlendSpeed, its
/// MinDiagonalWeight (the least each of two ways other than forward weighs for
/// both to play), and the speeds (uu a second, from and to) over which each way's
/// clip takes over: forward and backward, right and left, up and down.
#[derive(Clone, Copy, Debug)]
pub struct FlyBlend {
    pub blend_speed: f32,
    pub min_diagonal: f32,
    pub forward: Vec2,
    pub right: Vec2,
    pub up: Vec2,
}

/// A plane's tree's vehicle form switch (VEH_Plane_ANIMTREE's AmpAnimSwitch on its
/// form's _Boosting): its hovering state (an AnimNodeAdditiveBlending of a
/// TnAccelerationAnimBlend, whose seven children are its idle, forward, backward,
/// right, left, up and down, and its additive over them) and its boosting state (a
/// TnAnimTriggerSwitch: its transition's clip as it comes into play, then a
/// TnRollAnimBlend's left, centre and right), and each state's transition out (an
/// HmAnimTransitionWithBlend: its blend in and out and its clip, the form's
/// BoostMalfunction's: normal or after a collision).
#[derive(Clone, Debug, Default)]
pub struct PlaneTree {
    /// The acceleration blend's MaxVelocity (uu a second) and MinNormalizedInput,
    /// and the roll blend's MaxAngle (degrees).
    pub max_velocity: f32,
    pub min_input: f32,
    pub max_roll: f32,
    pub hover: [Option<usize>; 7],
    pub hover_add: Option<usize>,
    pub hover_add_group: Option<Arc<str>>,
    pub boost: [Option<usize>; 3],
    pub boost_in: Option<usize>,
    /// By state: 0 hovering, 1 boosting.
    pub outs: [PlaneOut; 2],
}

/// A plane state's transition out (HmAnimTransitionWithBlend).
#[derive(Clone, Copy, Debug, Default)]
pub struct PlaneOut {
    pub blend_in: f32,
    pub blend_out: f32,
    pub clip: Option<usize>,
    pub collision: Option<usize>,
}

/// The idle's TnAnimTurnInPlace: its turns (degrees, right positive, each with
/// the clip its player plays), its blend time, its threshold angle (how far short
/// of a turn's angle it may start) and how far ahead it looks at the yaw rate.
#[derive(Clone, Debug, Default)]
pub struct TurnInPlace {
    pub turns: Vec<(f32, usize)>,
    pub blend_time: f32,
    pub threshold: f32,
    pub look_ahead: f32,
}

/// The TnJumpTrajectoryBlend's jump: its clip, its trajectory (its root motion's
/// forward and up, uu) and the landing it plays.
#[derive(Clone, Debug)]
pub struct JChild {
    pub clip: Option<usize>,
    pub trajectory: Vec2,
    pub landing: Arc<str>,
}

/// The TnJumpTrajectoryBlend: its triangles (over its children: the first, its
/// idle, has no trajectory and none of its own).
#[derive(Clone, Debug, Default)]
pub struct JBlend {
    pub tris: Vec<[usize; 3]>,
    /// Children 1.. (index 0 is the idle's place, unused).
    pub children: Vec<Option<JChild>>,
}

/// A stance profile's pose (TnStanceAnimBlend's child for the stance).
#[derive(Clone, Debug)]
pub enum ProfileKind {
    /// The velocity switch (idle or moving).
    Switch,
    /// A velocity blend (by index).
    VBlend(usize),
    /// An auto-play sequence: its clip and chooser group.
    Seq {
        clip: Option<usize>,
        group: Option<Arc<str>>,
    },
    /// An additive blending of two auto-play sequences.
    Add {
        clip: Option<usize>,
        group: Option<Arc<str>>,
        add: Option<usize>,
        add_group: Option<Arc<str>>,
    },
    /// A plane's vehicle form switch (the set's `plane`).
    Plane,
}

/// A profile's transition to another stance's.
#[derive(Clone, Debug)]
pub struct Transition {
    pub target: u8,
    pub clip: Option<usize>,
    pub reverse: bool,
    pub blend_in: f32,
    pub blend_out: f32,
}

/// TnStanceAnimBlend's profile.
#[derive(Clone, Debug)]
pub struct Profile {
    pub stance: u8,
    pub blend_time: f32,
    pub kind: ProfileKind,
    pub transitions: Vec<Transition>,
}

/// The None profile's TnVelocitySwitch: its idle sequence and its moving blend.
#[derive(Clone, Debug, Default)]
pub struct Switch {
    pub idle: Option<usize>,
    pub moving: Option<usize>,
}

/// An animation of the acrobatics slot's.
#[derive(Clone, Copy, Debug)]
pub struct AcroAnim {
    pub clip: usize,
    pub blend_in: f32,
    pub blend_out: f32,
    pub looping: bool,
}

/// The acrobatics slot's states with animations: the jump's (and its loop), the
/// fall's.
pub const ACRO_JUMP: u8 = 0;
pub const ACRO_FALL: u8 = 5;
pub const ACRO_JUMP_LOOP: u8 = 100;

/// The TnAnimNodeAimer: its reference pose, its blend speed, the stances it aims
/// in and its aim poses (each one's aim from the reference's).
#[derive(Clone, Debug, Default)]
pub struct AimData {
    pub reference: usize,
    pub speed: f32,
    pub stances: Vec<u8>,
    pub poses: Vec<(usize, Vec2)>,
}

/// The aim offset node (UE3's AnimNodeAimOffset, its offsets baked from its aim
/// poses: High Moon's HmAnimNodeAimOffset, the Snipers' insects', its aim the
/// pawn's AimOffsetPct; TnAnimNodeAimOffset, the Guard's and the Leaper's, that
/// aim eased in at its InterpSpeed and mirrored while the pawn is left-handed if it
/// switches), its profiles and the stances that pick them
/// (StanceToProfileMappingOverrides; the first profile for the rest).
#[derive(Clone, Debug, Default)]
pub struct AimOffsetData {
    pub interp_speed: f32,
    pub switch_left: bool,
    pub profiles: Vec<AimOffsetProfile>,
    pub stances: Vec<(u8, usize)>,
}

/// An aim offset profile: its aim ranges (min, max: the aim's own ends), the
/// pawn's aim ranges it takes for its whole aim each way where it remaps the
/// pawn's aim (TnAnimNodeAimOffset's RemapPawnAimRange and PawnAimOffsetRange:
/// horizontal, vertical), and its bones' offsets.
#[derive(Clone, Debug, Default)]
pub struct AimOffsetProfile {
    pub name: String,
    pub horizontal: Vec2,
    pub vertical: Vec2,
    pub remap: Option<(Vec2, Vec2)>,
    pub bones: Vec<AimComponent>,
}

/// An aim offset bone (AimComponent): its offset for each way the node aims (left,
/// centre, right; up, centre, down: LU, LC, LD, CU, CC, CD, RU, RC, RD), a turn
/// put in front of its own in the mesh's space and a move of its place there
/// (glTF's axes, metres).
#[derive(Clone, Debug)]
pub struct AimComponent {
    pub bone: Arc<str>,
    pub offsets: [(Quat, Vec3); 9],
}

impl AimOffsetData {
    /// One of a file's aim offset node lines (ai_anims_<set>.txt's, robot_extras.
    /// txt's: aimoffset, aoprofile, aoremap, aobone, aostance; tab-separated
    /// fields) read into `node`; false for any other line.
    pub fn read_line(node: &mut Option<AimOffsetData>, r: &[&str]) -> bool {
        let f = |i: usize| num(r[i]);
        match r[0] {
            "aimoffset" if r.len() >= 4 => {
                *node = Some(AimOffsetData {
                    interp_speed: f(2),
                    switch_left: r[3].trim() == "1",
                    ..Default::default()
                });
            }
            "aoprofile" if r.len() >= 7 => {
                if let Some(ao) = node.as_mut() {
                    ao.profiles.push(AimOffsetProfile {
                        name: r[2].to_string(),
                        horizontal: Vec2::new(f(3), f(4)),
                        vertical: Vec2::new(f(5), f(6)),
                        remap: None,
                        bones: Vec::new(),
                    });
                }
            }
            "aoremap" if r.len() >= 6 => {
                let k: usize = r[1].trim().parse().unwrap_or(0);
                if let Some(p) = node.as_mut().and_then(|ao| ao.profiles.get_mut(k)) {
                    p.remap = Some((Vec2::new(f(2), f(3)), Vec2::new(f(4), f(5))));
                }
            }
            "aobone" if r.len() >= 12 => {
                let k: usize = r[1].trim().parse().unwrap_or(0);
                let mut offsets = [(Quat::IDENTITY, Vec3::ZERO); 9];
                for (o, f) in offsets.iter_mut().zip(&r[3..12]) {
                    let n: Vec<f32> = f
                        .split_whitespace()
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    if n.len() >= 7 {
                        let q = Quat::from_xyzw(n[0], n[1], n[2], n[3]);
                        *o = (q.normalize(), Vec3::new(n[4], n[5], n[6]));
                    }
                }
                if let Some(p) = node.as_mut().and_then(|ao| ao.profiles.get_mut(k)) {
                    p.bones.push(AimComponent {
                        bone: Arc::from(r[2]),
                        offsets,
                    });
                }
            }
            "aostance" if r.len() >= 3 => {
                if let Some(ao) = node.as_mut() {
                    ao.stances
                        .push((stance::number(r[1]), r[2].trim().parse().unwrap_or(0)));
                }
            }
            _ => return false,
        }
        true
    }

    /// A file's aim offset node (its lines), if it has one.
    pub fn read(text: &str) -> Option<AimOffsetData> {
        let mut node = None;
        for line in text.lines() {
            let r: Vec<&str> = line.split('\t').collect();
            AimOffsetData::read_line(&mut node, &r);
        }
        node
    }

    /// The profile the node plays in a stance.
    pub fn profile_for(&self, stance: u8) -> usize {
        self.stances
            .iter()
            .find(|(s, _)| *s == stance)
            .map_or(0, |(_, p)| *p)
            .min(self.profiles.len().saturating_sub(1))
    }
}

impl AimOffsetProfile {
    /// AnimNodeAimOffset.GetBoneAtoms' aim: each side of it over its range's end
    /// on that side, within a full aim each way. A profile that remaps the pawn's
    /// aim (TnAnimNodeAimOffset's RemapPawnAimRange) takes the ends of the pawn's
    /// own range (PawnAimOffsetRange) for its whole aim each way instead.
    pub fn safe_aim(&self, aim: Vec2) -> Vec2 {
        let scale = |v: f32, range: Vec2| {
            let (lo, hi) = (range.x, range.y);
            let v = if v <= 0.0 && lo != 0.0 {
                v / lo.abs()
            } else if v > 0.0 && hi != 0.0 {
                v / hi
            } else {
                v
            };
            v.clamp(-1.0, 1.0)
        };
        let (h, v) = self.remap.unwrap_or((self.horizontal, self.vertical));
        Vec2::new(scale(aim.x, h), scale(aim.y, v))
    }
}

impl AimComponent {
    /// The bone's offset at an aim (its safe aim): between the four ways around it
    /// (BiLerp of the quarter it is in, its turns' lerped and normalised).
    pub fn offset_at(&self, aim: Vec2) -> (Quat, Vec3) {
        const LU: usize = 0;
        const LC: usize = 1;
        const LD: usize = 2;
        const CU: usize = 3;
        const CC: usize = 4;
        const CD: usize = 5;
        const RU: usize = 6;
        const RC: usize = 7;
        const RD: usize = 8;
        let (corners, fx, fy) = match (aim.x >= 0.0, aim.y >= 0.0) {
            (true, true) => ([CC, RC, CU, RU], aim.x, aim.y),
            (true, false) => ([CD, RD, CC, RC], aim.x, aim.y + 1.0),
            (false, true) => ([LC, CC, LU, CU], aim.x + 1.0, aim.y),
            (false, false) => ([LD, CD, LC, CC], aim.x + 1.0, aim.y + 1.0),
        };
        let [a, b, c, d] = corners.map(|k| self.offsets[k]);
        let turn = a.0.lerp(b.0, fx).lerp(c.0.lerp(d.0, fx), fy);
        let place = a.1.lerp(b.1, fx).lerp(c.1.lerp(d.1, fx), fy);
        (turn, place)
    }
}

/// Everything a set's file holds.
#[derive(Default, Debug)]
pub struct Clips {
    /// The set's name and the types that play it.
    pub set: String,
    pub types: Vec<String>,
    pub clips: Vec<ClipInfo>,
    by_name: HashMap<String, usize>,
    /// The chooser groups (by lower case name): their clips.
    pub groups: HashMap<String, Vec<usize>>,
    /// The pose's layers from the bottom up (the base and its additive idles,
    /// then the tree's nodes over its base, as their layers: its aimer, its
    /// masked slots, its custom slots (the full body's, the weapon's and the
    /// shoulder's one layer), its jump blend, its additive slot).
    pub layers: Vec<Layer>,
    /// The custom animation players that have a slot in the tree (by mask).
    pub slots: [bool; anim::PLAYERS],
    pub mirror: Mirror,
    /// The masked slots' bones and clips (by their layers: Upper, LeftArm).
    pub masks: HashMap<Layer, Mask>,
    pub vblends: Vec<VBlend>,
    pub jump: JBlend,
    pub profiles: Vec<Profile>,
    pub switch: Switch,
    /// The velocity switch's idle's turn in place (none without turns).
    pub turn: Option<TurnInPlace>,
    pub acro: HashMap<u8, AcroAnim>,
    pub aim: Option<AimData>,
    pub aim_offset: Option<AimOffsetData>,
    /// A plane's vehicle form switch (its `plane` lines).
    pub plane: Option<PlaneTree>,
}

static CLIP_SETS: OnceLock<Vec<Arc<Clips>>> = OnceLock::new();

/// Every set's data, read once (the sets whose files are there).
pub fn sets() -> &'static [Arc<Clips>] {
    CLIP_SETS.get_or_init(|| {
        let dir = crate::assets_dir();
        SETS.iter()
            .filter_map(|s| Clips::load(&dir, s))
            .map(Arc::new)
            .collect()
    })
}

/// The set an enemy type plays.
pub fn for_type(id: &str) -> Option<Arc<Clips>> {
    sets()
        .iter()
        .find(|c| c.types.iter().any(|t| t == id))
        .cloned()
}

/// A set by name.
pub fn set(name: &str) -> Option<Arc<Clips>> {
    sets().iter().find(|c| c.set == name).cloned()
}

fn num(s: &str) -> f32 {
    s.trim().parse::<f32>().unwrap_or(0.0)
}

fn vec3(s: &str) -> Vec3 {
    let n: Vec<f32> = s
        .split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect();
    if n.len() >= 3 {
        Vec3::new(n[0], n[1], n[2])
    } else {
        Vec3::ZERO
    }
}

fn tris(f: &[&str]) -> Vec<[usize; 3]> {
    let n: Vec<usize> = f.iter().filter_map(|x| x.trim().parse().ok()).collect();
    n.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect()
}

fn group_name(s: &str) -> Option<Arc<str>> {
    (s != "-" && !s.is_empty() && s != "None").then(|| Arc::from(s))
}

/// A custom slot's mask (TnRobotAnimator's players' slots), by its node's name. (A
/// vehicle form's animator has one, Custom: TnVehicleForm.OnInitialize's.)
fn slot_mask(slot: &str) -> Option<u8> {
    Some(match slot {
        "FullBodyCustom" | "Custom" => anim::MASK_FULL_BODY,
        "UpperBodyCustom" => anim::MASK_UPPER_BODY,
        "LeftArmCustom" => anim::MASK_LEFT_ARM,
        "WeaponCustom" => anim::MASK_WEAPON,
        "ShoulderCustom" => anim::MASK_SHOULDER,
        _ => return None,
    })
}

/// A masked slot's layer.
fn mask_layer(slot: &str) -> Option<Layer> {
    match slot {
        "UpperBodyCustom" => Some(Layer::Upper),
        "LeftArmCustom" => Some(Layer::LeftArm),
        _ => None,
    }
}

impl Clips {
    /// A set's data, if its file is there.
    pub fn load(assets: &std::path::Path, set: &str) -> Option<Clips> {
        let file = anims_file(set);
        match std::fs::read_to_string(assets.join(&file)) {
            Ok(text) => {
                let mut c = Clips::parse(&text);
                c.set = set.to_string();
                Some(c)
            }
            Err(_) => {
                info!("{file} not found: its enemies' AI has no animations");
                None
            }
        }
    }

    /// A clip by name (any case).
    pub fn find(&self, name: &str) -> Option<usize> {
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }

    pub fn get(&self, i: usize) -> Option<&ClipInfo> {
        self.clips.get(i)
    }

    /// A chooser group's clips, if `name` is a group.
    pub fn group(&self, name: &str) -> Option<&[usize]> {
        self.groups
            .get(&name.to_ascii_lowercase())
            .map(|v| v.as_slice())
    }

    /// A name's clip: the group's pick (`roll` in 0..1) if it is a group, else the
    /// clip of that name.
    pub fn resolve(&self, name: &str, roll: f32) -> Option<usize> {
        if let Some(g) = self.group(name).filter(|g| !g.is_empty()) {
            return Some(g[((roll * g.len() as f32) as usize).min(g.len() - 1)]);
        }
        self.find(name)
    }

    pub fn parse(text: &str) -> Clips {
        let mut out = Clips::default();
        // (Lines that name clips come after the clips: they are read in a second
        // pass.)
        let lines: Vec<Vec<&str>> = text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(|l| l.split('\t').collect())
            .collect();
        for r in &lines {
            if r[0] == "clip" && r.len() >= 6 {
                let kind = match r[5].trim() {
                    "aim" => ClipKind::Aim,
                    "additive" => ClipKind::Additive,
                    _ => ClipKind::Pose,
                };
                let rate = num(r[3]);
                out.by_name
                    .insert(r[1].to_ascii_lowercase(), out.clips.len());
                out.clips.push(ClipInfo {
                    name: Arc::from(r[1]),
                    secs: num(r[2]),
                    rate: if rate > 0.0 { rate } else { 1.0 },
                    index: r[4].trim().parse().unwrap_or(0),
                    kind,
                    notifies: Vec::new(),
                    aim: Vec2::ZERO,
                    root: None,
                });
            }
        }
        let clip = |out: &Clips, s: &str| out.find(s);
        let mut aim_poses = Vec::new();
        let mut chain: Vec<Vec<&str>> = Vec::new();
        for r in &lines {
            match r[0] {
                "group" if r.len() >= 2 => {
                    let members = r[2..].iter().filter_map(|m| clip(&out, m)).collect();
                    out.groups.insert(r[1].to_ascii_lowercase(), members);
                }
                "htn" if r.len() >= 6 => {
                    if let Some(c) = clip(&out, r[1]) {
                        out.clips[c].notifies.push(ClipNotify::Htn {
                            at: num(r[2]),
                            event: num(r[3]) as u8,
                            socket: Arc::from(r[4]),
                            mirrored: Arc::from(r[5]),
                        });
                    }
                }
                "sweep" if r.len() >= 6 => {
                    if let Some(c) = clip(&out, r[1]) {
                        out.clips[c].notifies.push(ClipNotify::Sweep {
                            at: num(r[2]),
                            secs: num(r[3]),
                            offset: vec3(r[4]),
                            half: vec3(r[5]),
                        });
                    }
                }
                "impact" if r.len() >= 11 => {
                    if let Some(c) = clip(&out, r[1]) {
                        out.clips[c].notifies.push(ClipNotify::Impact {
                            at: num(r[2]),
                            secs: num(r[3]),
                            socket: Arc::from(r[4].trim()),
                            half: vec3(r[5]),
                            damage: num(r[6]),
                            damage_type: Arc::from(r[7].trim()),
                            forward: num(r[8]),
                            sideways: num(r[9]),
                            vertical: num(r[10]),
                        });
                    }
                }
                "toggle" if r.len() >= 4 => {
                    if let Some(c) = clip(&out, r[1]) {
                        out.clips[c].notifies.push(ClipNotify::Toggle {
                            at: num(r[2]),
                            hide: r[3].trim() == "1",
                        });
                    }
                }
                "vehiclefx" if r.len() >= 4 => {
                    if let Some(c) = clip(&out, r[1]) {
                        out.clips[c].notifies.push(ClipNotify::VehicleFx {
                            at: num(r[2]),
                            allowed: r[3].trim() == "1",
                        });
                    }
                }
                "round" if r.len() >= 4 => {
                    if let Some(c) = clip(&out, r[1]) {
                        out.clips[c].notifies.push(ClipNotify::Round {
                            at: num(r[2]),
                            along_gun: r[3].trim() == "1",
                        });
                    }
                }
                "burst" if r.len() >= 4 => {
                    if let Some(c) = clip(&out, r[1]) {
                        out.clips[c].notifies.push(ClipNotify::Burst {
                            at: num(r[2]),
                            secs: num(r[3]),
                        });
                    }
                }
                "step" if r.len() >= 6 => {
                    if let Some(c) = clip(&out, r[1]) {
                        out.clips[c].notifies.push(ClipNotify::Step {
                            at: num(r[2]),
                            foot: num(r[3]) as u8,
                            kind: num(r[4]) as u8,
                            min_weight: num(r[5]),
                        });
                    }
                }
                "sound" if r.len() >= 6 => {
                    if let Some(c) = clip(&out, r[1]) {
                        // (The cue's name lives as long as the clips.)
                        let cue: &'static str = Box::leak(r[3].trim().to_string().into_boxed_str());
                        out.clips[c].notifies.push(ClipNotify::Sound {
                            at: num(r[2]),
                            cue,
                            socket: socket(r[4]),
                            min_weight: num(r[5]),
                        });
                    }
                }
                "voice" if r.len() >= 6 => {
                    if let Some(c) = clip(&out, r[1]) {
                        let cue: &'static str = Box::leak(r[3].trim().to_string().into_boxed_str());
                        out.clips[c].notifies.push(ClipNotify::Voice {
                            at: num(r[2]),
                            cue,
                            socket: socket(r[4]),
                            min_weight: num(r[5]),
                        });
                    }
                }
                "event" if r.len() >= 6 => {
                    if let Some(c) = clip(&out, r[1]) {
                        out.clips[c].notifies.push(ClipNotify::Event {
                            at: num(r[2]),
                            event: Arc::from(super::audio::event_name(r[3])),
                            socket: socket(r[4]),
                            min_weight: num(r[5]),
                        });
                    }
                }
                "dialog" if r.len() >= 7 => {
                    if let Some(c) = clip(&out, r[1]) {
                        out.clips[c].notifies.push(ClipNotify::Dialog {
                            at: num(r[2]),
                            event: Arc::from(super::audio::event_name(r[3])),
                            socket: socket(r[4]),
                            min_weight: num(r[5]),
                            death: r[6].trim() == "1",
                        });
                    }
                }
                "fx" if r.len() >= 7 => {
                    if let Some(c) = clip(&out, r[1]) {
                        let playback = out.clips[c].secs / out.clips[c].rate;
                        out.clips[c].notifies.push(ClipNotify::Effect {
                            at: num(r[2]),
                            system: Arc::from(r[3].trim()),
                            socket: socket(r[4]),
                            attached: r[5].trim() == "1",
                            min_weight: num(r[6]),
                            offset: r.get(7).map_or(Vec3::ZERO, |v| vec3(v)),
                            turn: r.get(8).map_or(Vec3::ZERO, |v| vec3(v)),
                            params: r
                                .get(9)
                                .map(|v| v.trim())
                                .filter(|v| !v.is_empty() && *v != "-")
                                .map(|v| v.split(',').map(Arc::from).collect()),
                            auto_off: (r.get(10).map(|v| v.trim()) == Some("1"))
                                .then_some(playback + AUTO_OFF_PAD),
                        });
                    }
                }
                "proj" if r.len() >= 7 => {
                    if let Some(c) = clip(&out, r[1]) {
                        out.clips[c].notifies.push(ClipNotify::Projectile {
                            at: num(r[2]),
                            socket: socket(r[3]),
                            class: Arc::from(r[4].trim()),
                            cone: num(r[5]),
                            count: r[6].trim().parse().unwrap_or(1),
                        });
                    }
                }
                "types" => out.types = r[1..].iter().map(|t| t.trim().to_string()).collect(),
                "chain" if r.len() >= 2 => chain.push(r[1..].iter().map(|x| x.trim()).collect()),
                "mask" if r.len() >= 3 => {
                    if let Some(l) = mask_layer(r[1]) {
                        out.masks
                            .entry(l)
                            .or_default()
                            .paths
                            .push(r[2].split('/').map(|s| s.to_string()).collect());
                    }
                }
                "maskclips" if r.len() >= 3 => {
                    if let Some(l) = mask_layer(r[1]) {
                        let list: Vec<usize> =
                            r[2..].iter().filter_map(|n| clip(&out, n)).collect();
                        out.masks.entry(l).or_default().clips.extend(list);
                    }
                }
                "vpick" if r.len() >= 5 => {
                    let k: usize = r[2].trim().parse().unwrap_or(usize::MAX);
                    let st = stance::number(r[3]);
                    if let Some(c) = clip(&out, r[4])
                        && let Some(ch) = out
                            .vblends
                            .iter_mut()
                            .rev()
                            .find(|b| b.name == r[1])
                            .and_then(|b| b.children.get_mut(k))
                    {
                        ch.picks.push((st, c));
                    }
                }
                "aimof" if r.len() >= 4 => {
                    if let Some(c) = clip(&out, r[1]) {
                        out.clips[c].aim = Vec2::new(num(r[2]), num(r[3]));
                    }
                }
                "rootm" if r.len() >= 4 => {
                    if let Some(c) = clip(&out, r[1]) {
                        let frames = r[3..]
                            .iter()
                            .map(|s| {
                                let n: Vec<f32> = s
                                    .split_whitespace()
                                    .filter_map(|x| x.parse().ok())
                                    .collect();
                                [
                                    n.first().copied().unwrap_or(0.0),
                                    n.get(1).copied().unwrap_or(0.0),
                                    n.get(2).copied().unwrap_or(0.0),
                                    n.get(3).copied().unwrap_or(0.0),
                                ]
                            })
                            .collect();
                        // (Its frames as its file has them: 30 a second times its
                        // RateScale, rate scaled already. A stumble (1.25) ran
                        // through its motion in the first 80% of its time.)
                        out.clips[c].root = Some(RootMotion {
                            dt: num(r[2]) * out.clips[c].rate,
                            frames,
                        });
                    }
                }
                "vblend" if r.len() >= 3 => {
                    out.vblends.push(VBlend {
                        name: r[1].to_string(),
                        stance: r[2].to_string(),
                        tris: tris(&r[3..]),
                        children: Vec::new(),
                        strafer: None,
                        fly: None,
                    });
                }
                "flyblend" if r.len() >= 10 => {
                    out.vblends.push(VBlend {
                        name: r[1].to_string(),
                        stance: "None".to_string(),
                        tris: Vec::new(),
                        children: Vec::new(),
                        strafer: None,
                        fly: Some(FlyBlend {
                            blend_speed: num(r[2]),
                            min_diagonal: num(r[3]),
                            forward: Vec2::new(num(r[4]), num(r[5])),
                            right: Vec2::new(num(r[6]), num(r[7])),
                            up: Vec2::new(num(r[8]), num(r[9])),
                        }),
                    });
                }
                "fchild" if r.len() >= 7 => {
                    let c = VChild {
                        name: r[3].to_string(),
                        clip: clip(&out, r[4]),
                        group: group_name(r[5]),
                        velocity: Vec2::ZERO,
                        synched: r[6].trim() != "-",
                        picks: Vec::new(),
                    };
                    if let Some(b) = out.vblends.iter_mut().rev().find(|b| b.name == r[1]) {
                        b.children.push(c);
                    }
                }
                "plane" if r.len() >= 4 => {
                    let p = out.plane.get_or_insert_with(PlaneTree::default);
                    p.max_velocity = num(r[1]);
                    p.min_input = num(r[2]);
                    p.max_roll = num(r[3]);
                }
                "phover" if r.len() >= 8 => {
                    let hover: Vec<Option<usize>> = r[1..8].iter().map(|n| clip(&out, n)).collect();
                    let p = out.plane.get_or_insert_with(PlaneTree::default);
                    for (i, k) in hover.into_iter().enumerate() {
                        p.hover[i] = k;
                    }
                }
                "padd" if r.len() >= 3 => {
                    let (add, group) = (clip(&out, r[1]), group_name(r[2]));
                    let p = out.plane.get_or_insert_with(PlaneTree::default);
                    p.hover_add = add;
                    p.hover_add_group = group;
                }
                "pboost" if r.len() >= 4 => {
                    let boost = [clip(&out, r[1]), clip(&out, r[2]), clip(&out, r[3])];
                    out.plane.get_or_insert_with(PlaneTree::default).boost = boost;
                }
                "pboostin" if r.len() >= 2 => {
                    let k = clip(&out, r[1]);
                    out.plane.get_or_insert_with(PlaneTree::default).boost_in = k;
                }
                "pout" if r.len() >= 6 => {
                    let state: usize = r[1].trim().parse().unwrap_or(usize::MAX);
                    let o = PlaneOut {
                        blend_in: num(r[2]),
                        blend_out: num(r[3]),
                        clip: clip(&out, r[4]),
                        collision: clip(&out, r[5]),
                    };
                    if let Some(slot) = out
                        .plane
                        .get_or_insert_with(PlaneTree::default)
                        .outs
                        .get_mut(state)
                    {
                        *slot = o;
                    }
                }
                "strafer" if r.len() >= 3 => {
                    if let Some(b) = out.vblends.iter_mut().rev().find(|b| b.name == r[1]) {
                        b.strafer = Some(num(r[2]));
                    }
                }
                "turn" if r.len() >= 3 => {
                    if let Some(c) = clip(&out, r[2]) {
                        out.turn
                            .get_or_insert_with(TurnInPlace::default)
                            .turns
                            .push((num(r[1]), c));
                    }
                }
                "turnparams" if r.len() >= 4 => {
                    if let Some(t) = out.turn.as_mut() {
                        t.blend_time = num(r[1]);
                        t.threshold = num(r[2]);
                        t.look_ahead = num(r[3]);
                    }
                }
                "vchild" if r.len() >= 8 => {
                    let c = VChild {
                        name: r[3].to_string(),
                        clip: clip(&out, r[4]),
                        group: group_name(r[5]),
                        velocity: Vec2::new(num(r[6]), num(r[7])),
                        synched: r.get(8).is_some_and(|g| g.trim() != "-"),
                        picks: Vec::new(),
                    };
                    if let Some(b) = out.vblends.iter_mut().rev().find(|b| b.name == r[1]) {
                        b.children.push(c);
                    }
                }
                "jblend" => out.jump.tris = tris(&r[1..]),
                "jchild" if r.len() >= 7 => {
                    let k: usize = r[1].trim().parse().unwrap_or(0);
                    if out.jump.children.len() <= k {
                        out.jump.children.resize(k + 1, None);
                    }
                    out.jump.children[k] = Some(JChild {
                        clip: clip(&out, r[3]),
                        trajectory: Vec2::new(num(r[4]), num(r[5])),
                        landing: Arc::from(r[6]),
                    });
                }
                "sprofile" if r.len() >= 5 => {
                    let kind = match r[4] {
                        "switch" => ProfileKind::Switch,
                        // (A flying robot's blend: its flyblend's, read before.)
                        "vblend" | "fly" => ProfileKind::VBlend(
                            r.get(5)
                                .and_then(|n| out.vblends.iter().position(|b| b.name == *n))
                                .unwrap_or(0),
                        ),
                        "plane" => ProfileKind::Plane,
                        "add" if r.len() >= 9 => ProfileKind::Add {
                            clip: clip(&out, r[5]),
                            group: group_name(r[6]),
                            add: clip(&out, r[7]),
                            add_group: group_name(r[8]),
                        },
                        _ => ProfileKind::Seq {
                            clip: r.get(5).and_then(|n| clip(&out, n)),
                            group: r.get(6).and_then(|n| group_name(n)),
                        },
                    };
                    out.profiles.push(Profile {
                        stance: stance::number(r[2]),
                        blend_time: num(r[3]),
                        kind,
                        transitions: Vec::new(),
                    });
                }
                "strans" if r.len() >= 7 => {
                    let k: usize = r[1].trim().parse().unwrap_or(usize::MAX);
                    let t = Transition {
                        target: stance::number(r[2]),
                        clip: clip(&out, r[3]),
                        reverse: r[4].trim() == "1",
                        blend_in: num(r[5]),
                        blend_out: num(r[6]),
                    };
                    if let Some(p) = out.profiles.get_mut(k) {
                        p.transitions.push(t);
                    }
                }
                "switch" if r.len() >= 3 => {
                    out.switch = Switch {
                        idle: clip(&out, r[1]),
                        moving: out.vblends.iter().position(|b| b.name == r[2]),
                    };
                }
                "acro" if r.len() >= 6 => {
                    if let Some(c) = clip(&out, r[2]) {
                        out.acro.insert(
                            r[1].trim().parse().unwrap_or(255),
                            AcroAnim {
                                clip: c,
                                blend_in: num(r[3]),
                                blend_out: num(r[4]),
                                looping: r[5].trim() == "1",
                            },
                        );
                    }
                }
                "aim" if r.len() >= 4 => {
                    out.aim = clip(&out, r[2]).map(|reference| AimData {
                        reference,
                        speed: num(r[3]),
                        stances: r[4..].iter().map(|s| stance::number(s)).collect(),
                        poses: Vec::new(),
                    });
                }
                "aimpose" if r.len() >= 5 => {
                    if let Some(c) = clip(&out, r[2]) {
                        aim_poses.push((c, Vec2::new(num(r[3]), num(r[4]))));
                    }
                }
                _ if AimOffsetData::read_line(&mut out.aim_offset, r) => {}
                _ => {}
            }
        }
        if let Some(a) = out.aim.as_mut() {
            a.poses = aim_poses;
        }
        out.read_chain(&chain);
        out
    }

    /// The tree's nodes over its base (the chain lines, from the bottom up) as the
    /// pose's layers, its slots and its mirror.
    fn read_chain(&mut self, chain: &[Vec<&str>]) {
        let mut layers = vec![Layer::Base, Layer::BaseAdd];
        let push = |layers: &mut Vec<Layer>, l: Layer| {
            if !layers.contains(&l) {
                layers.push(l);
            }
        };
        for c in chain {
            match (c[0], c.get(1).copied()) {
                ("aim" | "aimoffset", _) => push(&mut layers, Layer::Aim),
                ("masked", Some(slot)) => {
                    if let (Some(l), Some(m)) = (mask_layer(slot), slot_mask(slot)) {
                        push(&mut layers, l);
                        self.slots[m as usize] = true;
                    }
                }
                ("slot", Some("AdditiveSlot")) => {
                    push(&mut layers, Layer::Add);
                    self.slots[anim::MASK_ADDITIVE as usize] = true;
                }
                ("slot", Some(slot)) => {
                    if let Some(m) = slot_mask(slot) {
                        push(&mut layers, Layer::Full);
                        self.slots[m as usize] = true;
                    }
                }
                ("jump", _) => push(&mut layers, Layer::Jump),
                ("mirror", _) => self.mirror = Mirror::Handedness,
                ("nodemirror", _) => self.mirror = Mirror::Node,
                _ => {}
            }
        }
        self.layers = layers;
    }

    /// Where a layer is in the pose, from the bottom (None: the tree hasn't it).
    pub fn layer_index(&self, l: Layer) -> Option<usize> {
        self.layers.iter().position(|x| *x == l)
    }
}

impl ClipBook for Clips {
    fn length(&self, name: &str) -> Option<f32> {
        self.find(name).map(|c| self.clips[c].secs)
    }

    fn rate(&self, name: &str) -> f32 {
        self.find(name).map_or(1.0, |c| self.clips[c].rate)
    }

    fn choose(&self, name: &str, roll: f32) -> Option<Arc<str>> {
        let g = self.group(name)?;
        if g.is_empty() {
            return None;
        }
        let c = g[((roll * g.len() as f32) as usize).min(g.len() - 1)];
        Some(self.clips[c].name.clone())
    }

    fn notifies(&self, name: &str) -> &[ClipNotify] {
        self.find(name)
            .map_or(&[], |c| self.clips[c].notifies.as_slice())
    }

    fn is_additive(&self, name: &str) -> bool {
        self.find(name)
            .is_some_and(|c| self.clips[c].kind != ClipKind::Pose)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_soldiers_clips() {
        let Some(c) = Clips::load(&crate::assets_dir(), "soldier") else {
            return;
        };
        let melee = c.group("AI_Melee_01").expect("the melee group");
        assert_eq!(melee.len(), 3);
        let grenade = c.find("GrenadeThrow").expect("the grenade throw");
        assert!(
            c.clips[grenade]
                .notifies
                .iter()
                .any(|n| matches!(n, ClipNotify::Htn { event: 0, .. }))
        );
        assert_eq!(c.profiles.len(), 12);
        assert!(matches!(c.profiles[0].kind, ProfileKind::Switch));
        assert_eq!(c.profiles[0].transitions.len(), 3);
        assert!(c.switch.idle.is_some() && c.switch.moving.is_some());
        let strafe = &c.vblends[c.switch.moving.unwrap()];
        assert_eq!(strafe.children.len(), 13);
        assert!((strafe.children[1].velocity.x - 1192.0).abs() < 2.0);
        assert_eq!(c.jump.children.len(), 7);
        assert_eq!(c.aim.as_ref().map(|a| a.poses.len()), Some(9));
        assert!(c.acro.contains_key(&ACRO_FALL));
        let dive = c.find("Grenade_Reaction_B").expect("a dive");
        assert!(
            c.clips[dive]
                .root
                .as_ref()
                .is_some_and(|r| r.at(10.0)[0] < -1900.0)
        );
        assert!(!c.masks[&Layer::Upper].paths.is_empty());
        assert_eq!(c.masks[&Layer::Upper].clips.len(), 3);
        assert_eq!(
            c.layers,
            vec![
                Layer::Base,
                Layer::BaseAdd,
                Layer::Aim,
                Layer::Upper,
                Layer::Full,
                Layer::Jump,
                Layer::Add
            ]
        );
        assert_eq!(c.slots, [true, true, false, true, false, true]);
        assert_eq!(c.mirror, Mirror::Handedness);
        assert!(c.types.iter().any(|t| t == "SharpShooterAut"));
    }

    /// The Leaper's booster glow (Leaper_Boosters_LowBurn_FX, which loops for
    /// good) is turned off with the clips that start it, its fist attack's, its
    /// smash recovery's and its hit reactions', as is every effect the game turns
    /// off with its clip; its lunge's smoke trail plays on to its own end.
    #[test]
    fn the_leapers_boosters_go_off_with_their_clips() {
        let Some(c) = Clips::load(&crate::assets_dir(), "leaper") else {
            return;
        };
        let effects = |clip: &str| -> Vec<(Arc<str>, Option<f32>)> {
            let k = c.find(clip).unwrap_or_else(|| panic!("{clip}"));
            c.clips[k]
                .notifies
                .iter()
                .filter_map(|n| match n {
                    ClipNotify::Effect {
                        system, auto_off, ..
                    } => Some((system.clone(), *auto_off)),
                    _ => None,
                })
                .collect()
        };
        for clip in [
            "Melee_FistAttack",
            "Melee_LeapSmash_Recovery",
            "ADD_HitReaction_React_Normal_B",
        ] {
            let k = c.find(clip).unwrap();
            let playback = c.clips[k].secs / c.clips[k].rate + AUTO_OFF_PAD;
            let glow: Vec<Option<f32>> = effects(clip)
                .into_iter()
                .filter(|(s, _)| &**s == "Leaper_Boosters_LowBurn_FX")
                .map(|(_, off)| off)
                .collect();
            assert_eq!(glow.len(), 4, "{clip}");
            for off in glow {
                assert!(
                    off.is_some_and(|o| (o - playback).abs() < 1e-4),
                    "{clip}: {off:?}"
                );
            }
        }
        let tell = effects("Melee_Lunge_Tell");
        assert!(
            tell.iter()
                .any(|(s, off)| &**s == "SmokeTrail_FX" && off.is_none())
        );
        // (The other sets' files have it too: some of their effects go off.)
        for set in [
            "soldier",
            "shotgunner",
            "rocket",
            "scrapper",
            "titan",
            "flyer",
        ] {
            let Some(c) = Clips::load(&crate::assets_dir(), set) else {
                continue;
            };
            let off = c.clips.iter().flat_map(|k| k.notifies.iter()).any(|n| {
                matches!(
                    n,
                    ClipNotify::Effect {
                        auto_off: Some(_),
                        ..
                    }
                )
            });
            assert!(off, "{set}: none goes off");
        }
    }

    #[test]
    fn reads_every_sets_tree() {
        for set in SETS {
            let Some(c) = Clips::load(&crate::assets_dir(), set) else {
                continue;
            };
            assert!(!c.types.is_empty(), "{set}: no types");
            assert!(!c.profiles.is_empty(), "{set}: no base pose");
            assert!(
                c.slots[anim::MASK_FULL_BODY as usize],
                "{set}: no full body slot"
            );
            assert_eq!(c.layers[..2], [Layer::Base, Layer::BaseAdd]);
        }
        // The Shotgunners aim over their jumps; the Titans have no jumps and two
        // masked slots; the Rocket Troopers' idle picks by stance.
        if let Some(c) = Clips::load(&crate::assets_dir(), "shotgunner") {
            assert!(c.layer_index(Layer::Aim) > c.layer_index(Layer::Jump));
        }
        if let Some(c) = Clips::load(&crate::assets_dir(), "titan") {
            assert!(c.layer_index(Layer::Jump).is_none());
            assert!(c.masks.contains_key(&Layer::LeftArm));
            assert!(matches!(c.profiles[0].kind, ProfileKind::Switch));
        }
        // The Swarmers' moving is a strafer, their idle turns in place.
        if let Some(c) = Clips::load(&crate::assets_dir(), "swarmer") {
            let moving = c.switch.moving.map(|k| &c.vblends[k]).expect("its moving");
            assert!(moving.strafer.is_some() && moving.children.len() == 4);
            assert!(c.switch.idle.is_some());
            assert_eq!(c.turn.as_ref().map_or(0, |t| t.turns.len()), 4);
        }
        if let Some(c) = Clips::load(&crate::assets_dir(), "rocket") {
            assert!(
                c.vblends
                    .iter()
                    .any(|b| b.children.iter().any(|ch| !ch.picks.is_empty()))
            );
        }
    }
}
