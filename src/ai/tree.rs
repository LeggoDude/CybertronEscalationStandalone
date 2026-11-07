//! An enemy's anim tree (the soldiers' Soldier_ANIMTREE) as far as the pose it
//! makes goes: what the tree does with the pawn's stance, motion, jumps and aim
//! and with its custom animations, as the clips the model plays with their
//! weights and times (graph.rs puts them on the model's animation graph).
//!
//! From the bottom of the tree up:
//! * **TnStanceAnimBlend**: each stance's profile (the None profile's velocity
//!   switch, the cover poses with their additive idle, the patrol's velocity
//!   blend); a change of stance plays the profiles' transition animation (forwards,
//!   or backwards) and blends to the new profile as it ends, or blends straight to
//!   it over the profile's BlendTime.
//! * **TnVelocitySwitch** and **TnLinearVelocityBlend**: idle, or moving: the
//!   blend's children weighted by where the pawn's velocity (in its own axes) is
//!   among their samples (GenerateSampleWeights over their triangles), playing
//!   together in their synch group; a chooser group's child picks its animation as
//!   it comes into play. A **TnStraferAnimBlend** (the Swarmers') weights its
//!   forward, backward, right and left children by the way the pawn moves.
//! * **TnAnimTurnInPlace** on the switch's idle (where its set has turns): a turn
//!   of the pawn while it stands plays the turn's animation over the idle as it
//!   goes round.
//! * **The acrobatics slot** (TnAcrobaticsSlotLogic): the jump's take-off and loop,
//!   the fall's loop, as the pawn's acrobatics state changes.
//! * **TnAnimNodeAimer**: in its stances, the aim poses' changes added to the pose
//!   to turn its aim (the aim bone's) from the pose's own to where the pawn aims
//!   (AimOffsetPct), at its BlendSpeed.
//! * **The custom slots**: the upper body's (its bones only), the full body's and
//!   the weapon's over everything under them.
//! * **TnJumpTrajectoryBlend**: a jump's animations weighted by its trajectory
//!   (their samples, the clips' own), played over the jump's time; its landing as
//!   it touches down (on the full body's player).
//! * **The additive slot** (TnAdditiveAnimatorService's: additive hit reactions).
//! * **TnAnimNodeHandednessMirror**: all of it mirrored left for right while the
//!   pawn holds its gun left-handed (and no animation forbids it), blended over half
//!   a second.
//!
//! The soldiers' turn in place (TnAnimTurnInPlace) has no turns for their tactical
//! idle and isn't played; the skeletal controls (look-at, recoil, foot placement)
//! aren't either.

use bevy::prelude::*;

use super::anim::{self, AnimParams, Callback, SlotState};
use super::clips::{
    ACRO_FALL, ACRO_JUMP, ACRO_JUMP_LOOP, ClipKind, ClipNotify, Clips, Mirror, ProfileKind,
};
use super::pawn::{Acrobatics, Pawn, Physics};
use super::ue;

/// The pose's layers (graph.rs has a node for each clip in each), in the order a
/// tree has them (its set's `layers`: the base and its additive idles at the
/// bottom, the rest as the tree's nodes over its base are).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Layer {
    /// The stance blend's poses and the acrobatics slot's.
    Base = 0,
    /// Additive changes on the base: the cover poses' additive idle.
    BaseAdd = 1,
    /// The aimer's aim poses (additive).
    Aim = 2,
    /// The upper body slot's (on the upper body's bones).
    Upper = 3,
    /// The left arm slot's (on the left arm's bones).
    LeftArm = 4,
    /// The full body's, weapon's and shoulder's slots'.
    Full = 5,
    /// The jump blend's jumps.
    Jump = 6,
    /// The additive slot's.
    Add = 7,
}

pub const LAYERS: usize = 8;

impl Layer {
    /// An override layer (its clips over what is under it at its share), not an
    /// additive one.
    pub fn overrides(self) -> bool {
        matches!(
            self,
            Layer::Upper | Layer::LeftArm | Layer::Full | Layer::Jump
        )
    }
}

/// A clip the pose plays: its layer, its clip (ai_anims.txt's), its weight and
/// its time (the sequence's). An override layer's (Upper, Full, Jump) and the
/// base's weights are shares of their layer (the layer's own share is the
/// pose's); the additive layers' are their changes' own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoseClip {
    pub layer: Layer,
    pub clip: u16,
    pub weight: f32,
    pub time: f32,
}

/// The pose: its clips, the share each override layer has of what is under it,
/// how much of it is mirrored, and its aim offset node's profile and aim (its
/// bones' offsets are put on the pose the clips make: aimoffset.rs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pose {
    pub clips: Vec<PoseClip>,
    pub shares: [f32; LAYERS],
    pub mirror: f32,
    pub aim_offset: Option<(u8, Vec2)>,
}

/// The landing animation's callback (TnJumpTrajectoryBlend.LandingAnimationComplete).
pub const CB_JUMP_LANDED: Callback = Callback { owner: 0, kind: 3 };

/// TnLinearVelocityBlend's BlendTime.
const VELOCITY_BLEND_TIME: f32 = 0.1;
/// The velocity switch's blend between idle and moving, and the speed it calls
/// moving (uu a second).
const SWITCH_BLEND_TIME: f32 = 0.2;
const MOVING_SPEED: f32 = 10.0;
/// TnAnimNodeHandednessMirror's BlendTime.
const MIRROR_BLEND_TIME: f32 = 0.5;
/// A node in play (UE3's ZERO_ANIMWEIGHT_THRESH), and a velocity blend's least
/// weight to tick (its Tick's NodeTotalWeight).
const RELEVANT: f32 = 1e-5;
const VELOCITY_TICK: f32 = 0.01;

/// A blend's children's weights moving to their targets (AnimNodeBlendBase's
/// BlendTimeToGo: straight to them over the time).
#[derive(Clone, Debug, Default)]
struct Blend {
    w: Vec<f32>,
    target: Vec<f32>,
    left: f32,
}

impl Blend {
    fn new(n: usize, full: usize) -> Blend {
        let mut w = vec![0.0; n];
        if full < n {
            w[full] = 1.0;
        }
        Blend {
            target: w.clone(),
            w,
            left: 0.0,
        }
    }

    /// BlendToFullWeight.
    fn to_full(&mut self, child: usize, secs: f32) {
        for (i, t) in self.target.iter_mut().enumerate() {
            *t = if i == child { 1.0 } else { 0.0 };
        }
        self.start(secs);
    }

    /// SetTargetWeights.
    fn set_targets(&mut self, t: &[f32], secs: f32) {
        for (i, x) in self.target.iter_mut().enumerate() {
            *x = t.get(i).copied().unwrap_or(0.0);
        }
        self.start(secs);
    }

    fn start(&mut self, secs: f32) {
        if secs <= 0.0 {
            self.w.clone_from(&self.target);
            self.left = 0.0;
        } else {
            self.left = secs;
        }
    }

    /// UpdateBlend.
    fn update(&mut self, dt: f32) {
        if self.left <= 0.0 {
            return;
        }
        if self.left <= dt {
            self.w.clone_from(&self.target);
            self.left = 0.0;
        } else {
            let k = dt / self.left;
            for (w, t) in self.w.iter_mut().zip(&self.target) {
                *w += (t - *w) * k;
            }
            self.left -= dt;
        }
    }

    fn get(&self, i: usize) -> f32 {
        self.w.get(i).copied().unwrap_or(0.0)
    }
}

/// A sequence node: its clip, its time and whether it is in play.
#[derive(Clone, Debug, Default)]
struct Seq {
    clip: Option<usize>,
    time: f32,
    relevant: bool,
}

/// A sequence node as it was before its frame (for the notifies it passes).
#[derive(Clone, Copy, Debug)]
struct Was {
    clip: Option<usize>,
    time: f32,
    relevant: bool,
}

impl Seq {
    fn was(&self) -> Was {
        Was {
            clip: self.clip,
            time: self.time,
            relevant: self.relevant,
        }
    }

    /// The span its clip went through this frame (from where it was; an auto-play
    /// node come into play, from its start), and whether round its end.
    fn span(&self, was: Was, auto: bool) -> Option<(f32, f32, bool)> {
        if !self.relevant {
            return None;
        }
        if (was.relevant || !auto) && was.clip == self.clip {
            return Some((was.time, self.time, self.time < was.time));
        }
        auto.then_some((0.0, self.time, false))
    }

    /// A frame of a sequence node at `total` weight: an auto-play node starts
    /// over as it comes into play (its chooser group picking its clip: `pick`) and
    /// rewinds as it goes; a plain one only stops while out of play. `advance`:
    /// whether it moves on by itself (a synch group's member is moved by its
    /// group).
    fn tick(
        &mut self,
        total: f32,
        dt: f32,
        rate: f32,
        len: f32,
        auto: bool,
        advance: bool,
        pick: impl FnOnce() -> Option<usize>,
    ) {
        let now = total > RELEVANT;
        if now && !self.relevant {
            if auto {
                if let Some(c) = pick() {
                    self.clip = Some(c);
                }
                self.time = 0.0;
            } else if self.clip.is_none() {
                self.clip = pick();
            }
        } else if !now && self.relevant && auto {
            self.time = 0.0;
        }
        self.relevant = now;
        if now && advance && len > 0.0 {
            self.time = (self.time + dt * rate).rem_euclid(len);
        }
    }
}

/// A velocity blend's state (a flying robot's blend's weights of the ways it
/// flies: `fly`).
#[derive(Clone, Debug, Default)]
struct VState {
    blend: Blend,
    seqs: Vec<Seq>,
    fly: Vec3,
}

/// A plane's vehicle form switch's state: its blend (0 hovering, 1 boosting, 2 a
/// transition's clip), the state it is in, the transition out playing (its
/// clip's sequence, the state it goes to, its blend out and whether that has
/// begun), the hovering state's sequences (its acceleration blend's children, its
/// additive), the boosting state's (its roll blend's children, its transition in),
/// its trigger switch's selection (1 its transition in, 0 its roll blend) and
/// whether its boosting state was in play.
#[derive(Clone, Debug, Default)]
struct PlaneState {
    ready: bool,
    blend: Blend,
    boosting: bool,
    out: Seq,
    out_to: usize,
    out_blend_out: f32,
    out_blending: bool,
    hover: [Seq; 7],
    add: Seq,
    boost: [Seq; 3],
    boost_in: Seq,
    trigger: u8,
    boost_relevant: bool,
}

/// The idle's turn in place: its blend (0 its source, 1 its player), its player's
/// sequence and whether it plays on, the turn it plays, the yaw it has yet to turn
/// through and the yaw it last saw, its yaw rates of late (HmFloatValueHistory's:
/// its clock and each rate's time), and whether it is in play.
#[derive(Clone, Debug, Default)]
struct TurnState {
    blend: Blend,
    player: Seq,
    playing: bool,
    index: usize,
    yaw_offset: f32,
    previous_yaw: f32,
    rates: std::collections::VecDeque<(f32, f32)>,
    clock: f32,
    relevant: bool,
}

/// TnAnimTurnInPlace's yaw rate history's sample time and kFinishedTurnThreshold
/// (rotation units: two degrees).
const TURN_RATE_SAMPLE_SECS: f32 = 0.3;
const FINISHED_TURN: f32 = 364.0889;
/// TnStraferAnimBlend's weights move their BlendSpeed's share of the way a tick:
/// the game's ticks a second.
const STRAFER_TICKS: f32 = 60.0;

/// The jump blend's state.
#[derive(Clone, Debug, Default)]
struct JumpState {
    blend: Blend,
    jumping: bool,
    master: usize,
    /// The 'Jumps' group's place in its jump (0..1) and its time.
    phase: f32,
    time: f32,
    seen: u32,
}

/// The tree's state.
#[derive(Clone, Debug, Default)]
pub struct AnimTree {
    ready: bool,
    // The stance blend: its children (0 the transition), the active profile, the
    // transition's sequence and its rate (1 or -1), and its blend into the profile.
    stance: Blend,
    active: usize,
    transition: Seq,
    transition_rate: f32,
    transition_blend: f32,
    // Each profile's auto-play sequences (its pose and its additive idle).
    profile_seqs: Vec<(Seq, Seq)>,
    // The velocity switch (0 idle, 1 moving) and its idle sequence.
    switch: Blend,
    idle: Seq,
    turn: TurnState,
    vblends: Vec<VState>,
    /// The 'Strafers' synch group's place in its cycle (0..1).
    strafers: f32,
    jump: JumpState,
    // The acrobatics slot: the state it played for, its slot, its take-off
    // playing (its end plays the jump's loop) and its last animation's blend out.
    acro_state: u8,
    acro: SlotState,
    acro_enter: Option<std::sync::Arc<str>>,
    acro_blend_out: f32,
    // The aimer's offset aim (the aimer's units) and how much it has.
    aim_offset: Vec2,
    aim_strength: f32,
    // The aim offset node's aim (TnAnimNodeAimOffset's LastAimX and LastAimY).
    ao_aim: Vec2,
    // The mirror's weight, its target and its blend.
    mirror: f32,
    mirror_target: f32,
    mirror_left: f32,
    /// The pawn's stance this frame (the stance-picked sequences').
    stance_now: u8,
    /// A plane's vehicle form switch.
    plane: PlaneState,
}

/// Animation.GenerateSampleWeights: the point's weights over the samples: the
/// barycentric weights of the triangle it is in, or of the nearest point of the
/// nearest triangle when it is outside them all.
pub fn sample_weights(p: Vec2, samples: &[Vec2], tris: &[[usize; 3]]) -> Vec<f32> {
    let mut out = vec![0.0; samples.len()];
    let mut best: Option<(f32, [usize; 3], Vec3)> = None;
    for t in tris {
        if t.iter().any(|&i| i >= samples.len()) {
            continue;
        }
        let (a, b, c) = (samples[t[0]], samples[t[1]], samples[t[2]]);
        let q = closest_on_triangle(p, a, b, c);
        let d = (p - q).length_squared();
        if best.as_ref().is_none_or(|(bd, ..)| d < *bd - 1e-4) {
            best = Some((d, *t, barycentric(q, a, b, c)));
            if d <= 1e-6 {
                break;
            }
        }
    }
    if let Some((_, t, w)) = best {
        out[t[0]] += w.x;
        out[t[1]] += w.y;
        out[t[2]] += w.z;
    } else if !out.is_empty() {
        out[0] = 1.0;
    }
    out
}

fn barycentric(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> Vec3 {
    let v0 = b - a;
    let v1 = c - a;
    let v2 = p - a;
    let d00 = v0.dot(v0);
    let d01 = v0.dot(v1);
    let d11 = v1.dot(v1);
    let d20 = v2.dot(v0);
    let d21 = v2.dot(v1);
    let den = d00 * d11 - d01 * d01;
    if den.abs() < 1e-9 {
        // (A flat triangle: its nearest corner.)
        let ds = [
            (p - a).length_squared(),
            (p - b).length_squared(),
            (p - c).length_squared(),
        ];
        let i = (0..3).min_by(|&x, &y| ds[x].total_cmp(&ds[y])).unwrap_or(0);
        let mut w = Vec3::ZERO;
        w[i] = 1.0;
        return w;
    }
    let v = (d11 * d20 - d01 * d21) / den;
    let w = (d00 * d21 - d01 * d20) / den;
    let u = 1.0 - v - w;
    let out = Vec3::new(u, v, w).max(Vec3::ZERO);
    let s = out.x + out.y + out.z;
    if s > 0.0 {
        out / s
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    }
}

fn closest_on_segment(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let d = b - a;
    let l = d.length_squared();
    if l < 1e-9 {
        return a;
    }
    a + d * ((p - a).dot(d) / l).clamp(0.0, 1.0)
}

fn closest_on_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> Vec2 {
    let cross = |o: Vec2, x: Vec2, y: Vec2| (x - o).perp_dot(y - o);
    let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
    let neg = d1 < -1e-6 || d2 < -1e-6 || d3 < -1e-6;
    let pos = d1 > 1e-6 || d2 > 1e-6 || d3 > 1e-6;
    if !(neg && pos) {
        return p;
    }
    let mut best = closest_on_segment(p, a, b);
    for q in [closest_on_segment(p, b, c), closest_on_segment(p, c, a)] {
        if (p - q).length_squared() < (p - best).length_squared() {
            best = q;
        }
    }
    best
}

/// The sound notifies a node's clip passed going from `t0` to `t1` (`wrapped`:
/// round its end; at its `end`, the end's too), at the node's whole weight (its
/// share of the pose: NodeTotalWeight).
#[allow(clippy::too_many_arguments)]
fn sound_notes(
    c: &Clips,
    clip: Option<usize>,
    t0: f32,
    t1: f32,
    wrapped: bool,
    end: bool,
    weight: f32,
    out: &mut Vec<(ClipNotify, f32)>,
) {
    let Some(k) = clip.and_then(|k| c.get(k)) else {
        return;
    };
    if weight <= RELEVANT {
        return;
    }
    for n in k.notifies.iter().filter(|n| n.in_tree()) {
        if anim::passed(n.at(), t0, t1, wrapped, end) {
            out.push((n.clone(), weight));
        }
    }
}

/// The pose's clips as they are added up (the same clip from two nodes of a
/// layer plays once, at the heavier one's time).
#[derive(Default)]
struct Builder {
    clips: Vec<(Layer, usize, f32, f32, f32)>,
}

impl Builder {
    fn add(&mut self, layer: Layer, clip: Option<usize>, w: f32, t: f32) {
        let Some(clip) = clip else { return };
        if w <= RELEVANT {
            return;
        }
        if let Some(e) = self.clips.iter_mut().find(|e| e.0 == layer && e.1 == clip) {
            e.2 += w;
            if w > e.4 {
                e.3 = t;
                e.4 = w;
            }
        } else {
            self.clips.push((layer, clip, w, t, w));
        }
    }
}

/// A slot's clips into a layer: each channel's share of the slot, and the slot's
/// share.
fn slot_clips(b: &mut Builder, c: &Clips, slot: &SlotState, layer: Layer) -> f32 {
    let total = slot.weight();
    if total <= RELEVANT {
        return 0.0;
    }
    for ch in &slot.channels {
        b.add(layer, c.find(&ch.anim), ch.weight / total, ch.time);
    }
    total
}

/// A synch group member's frame: its clip's span this frame and its weight (the
/// group's notifies are its heaviest member's).
#[derive(Clone, Copy, Debug)]
struct Synched {
    clip: Option<usize>,
    t0: f32,
    t1: f32,
    wrapped: bool,
    weight: f32,
}

impl AnimTree {
    /// Its first frame: in the stance's profile, idle or moving as the pawn is,
    /// not jumping.
    fn init(&mut self, c: &Clips, p: &Pawn) {
        self.ready = true;
        let n = c.profiles.len();
        let stance = p.current_stance();
        self.active = c
            .profiles
            .iter()
            .position(|pr| pr.stance == stance)
            .unwrap_or(0);
        self.stance = Blend::new(n + 1, self.active + 1);
        self.transition_rate = 1.0;
        self.profile_seqs = vec![(Seq::default(), Seq::default()); n];
        let moving = ue::vsize2d(p.vel) > MOVING_SPEED;
        self.switch = Blend::new(2, moving as usize);
        self.turn.blend = Blend::new(2, 0);
        self.vblends = c
            .vblends
            .iter()
            .map(|b| {
                let samples: Vec<Vec2> = b.children.iter().map(|ch| ch.velocity).collect();
                let w = sample_weights(local_velocity(p), &samples, &b.tris);
                let mut blend = Blend::new(b.children.len(), 0);
                blend.set_targets(&w, 0.0);
                VState {
                    blend,
                    seqs: vec![Seq::default(); b.children.len()],
                    fly: Vec3::ZERO,
                }
            })
            .collect();
        self.jump.blend = Blend::new(c.jump.children.len().max(1), 0);
        self.jump.seen = p.jumps;
        self.acro_state = p.acrobatics() as u8;
        self.acro_blend_out = 0.1;
        self.mirror = if p.stance.handedness == 1 { 1.0 } else { 0.0 };
        self.mirror_target = self.mirror;
    }

    /// TnStanceAnimBlend.ActivateProfile.
    fn activate_profile(&mut self, c: &Clips, new: usize) {
        if self.active == new {
            return;
        }
        let target = c.profiles[new].stance;
        match c.profiles[self.active]
            .transitions
            .iter()
            .find(|t| t.target == target)
        {
            Some(t) => {
                let len = t.clip.and_then(|k| c.get(k)).map_or(0.0, |k| k.secs);
                self.transition.clip = t.clip;
                self.transition_rate = if t.reverse { -1.0 } else { 1.0 };
                self.transition.time = if t.reverse { len } else { 0.0 };
                self.transition_blend = (len - t.blend_in).clamp(0.0, t.blend_out.max(0.0));
                self.stance.to_full(0, t.blend_in);
            }
            None => self.stance.to_full(new + 1, c.profiles[new].blend_time),
        }
        self.active = new;
    }

    /// GetTransitionTimeRemaining.
    fn transition_left(&self, c: &Clips) -> f32 {
        let len = self
            .transition
            .clip
            .and_then(|k| c.get(k))
            .map_or(0.0, |k| k.secs);
        let end = if self.transition_rate < 0.0 { 0.0 } else { len };
        (end - self.transition.time) * self.transition_rate
    }

    /// The tree's frame, after the pawn's and its animator's: the pose its model
    /// plays (the jump's landing, if it lands, goes to its animator).
    pub fn update(
        &mut self,
        c: &Clips,
        p: &mut Pawn,
        dt: f32,
        roll: &mut dyn FnMut() -> f32,
    ) -> Pose {
        if c.profiles.is_empty() {
            return Pose::default();
        }
        if !self.ready {
            self.init(c, p);
        }
        let left_handed = p.stance.handedness == 1;
        // The sound notifies its nodes pass (the custom slots' are the animator's).
        let mut notes: Vec<(ClipNotify, f32)> = Vec::new();

        // The custom slots over the base (the full body's, the weapon's and the
        // shoulder's: those the tree has).
        let slot_w = |m: u8| {
            if c.slots[m as usize] {
                p.anim.players[m as usize].slot.weight()
            } else {
                0.0
            }
        };
        let full_ws = [
            slot_w(anim::MASK_FULL_BODY),
            slot_w(anim::MASK_WEAPON),
            slot_w(anim::MASK_SHOULDER),
        ];

        // TnJumpTrajectoryBlend: a new jump, its blend, its landing.
        if p.jumps != self.jump.seen {
            self.jump.seen = p.jumps;
            if let Some(j) = p.jump {
                self.client_jump(c, Vec2::new(j.x, j.y), j.time);
            }
        }
        self.jump.blend.update(dt);
        let jump_was = self.jump.phase;
        if self.jump.jumping {
            self.jump.phase = (self.jump.phase + dt / self.jump.time.max(1e-3)).min(1.0);
            if p.physics != Physics::Falling {
                let out = if self.play_landing(c, p) { 0.1 } else { 0.2 };
                self.jump.blend.to_full(0, out);
                self.jump.jumping = false;
            }
        }
        let jump_share = if c.layer_index(Layer::Jump).is_some() {
            (1.0 - self.jump.blend.get(0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let full_share = 1.0 - full_ws.iter().map(|w| 1.0 - w).product::<f32>();
        // (Each base node's whole weight: what the slots and the jump leave.)
        let base_scale = (1.0 - full_share) * (1.0 - jump_share);

        // TnStanceAnimBlend.Tick.
        let stance = p.current_stance();
        self.stance_now = stance;
        let profile = c
            .profiles
            .iter()
            .position(|pr| pr.stance == stance)
            .unwrap_or(0);
        self.activate_profile(c, profile);
        if (self.stance.get(0) - 1.0).abs() < 0.001
            && self.transition_left(c) <= self.transition_blend
        {
            self.stance.to_full(self.active + 1, self.transition_blend);
        }
        self.stance.update(dt);

        // The acrobatics slot (and its animations' sounds: each channel from where
        // it was, or from its start).
        let acro_was: Vec<(std::sync::Arc<str>, f32)> = self
            .acro
            .channels
            .iter()
            .map(|ch| (ch.anim.clone(), ch.time))
            .collect();
        self.update_acro(c, p, dt, base_scale > RELEVANT);
        for ch in &self.acro.channels {
            let t0 = acro_was
                .iter()
                .find(|(a, _)| *a == ch.anim)
                .map_or(0.0, |(_, t)| *t);
            let wrapped = ch.looping && ch.time < t0;
            let end = !ch.looping && ch.time >= ch.length;
            sound_notes(
                c,
                c.find(&ch.anim),
                t0,
                ch.time,
                wrapped,
                end,
                base_scale * ch.weight,
                &mut notes,
            );
        }
        let acro = self.acro.weight();
        let stance_scale = base_scale * (1.0 - acro);

        // The velocity switch.
        let moving = ue::vsize2d(p.vel) > MOVING_SPEED;
        let want = if moving { 1 } else { 0 };
        if self.switch.target.get(want).copied() != Some(1.0) {
            self.switch.to_full(want, SWITCH_BLEND_TIME);
        }
        self.switch.update(dt);

        // The velocity blends.
        let mut local = local_velocity(p);
        if left_handed {
            local.y = -local.y;
        }
        for k in 0..c.vblends.len() {
            let total = stance_scale * self.vblend_weight(c, k);
            if total < VELOCITY_TICK {
                // (A flying robot's blend starts from none as it comes back into
                // play: OnBecomeRelevant.)
                if c.vblends[k].fly.is_some() {
                    self.vblends[k].fly = Vec3::ZERO;
                }
                continue;
            }
            let b = &c.vblends[k];
            // TnFlyingRobotAnimBlend.Tick: its velocity in its own axes gives the
            // weights of the ways it flies (GetWeightsFromVelocity), its own eased
            // toward them at its BlendSpeed (VInterpTo), its children's from them
            // (SetWeightsFromDirection).
            if let Some(f) = b.fly {
                let desired = fly_weights(&f, local3(p, p.vel));
                let v = &mut self.vblends[k];
                v.fly = ue::vinterp_to(v.fly, desired, dt, f.blend_speed);
                let w = v.fly;
                let targets = [
                    1.0 - (w.x.abs() + w.y.abs() + w.z.abs()),
                    w.x.max(0.0),
                    (-w.x).max(0.0),
                    w.y.max(0.0),
                    (-w.y).max(0.0),
                    w.z.max(0.0),
                    (-w.z).max(0.0),
                ];
                for (i, t) in targets.iter().enumerate() {
                    if let Some(x) = v.blend.w.get_mut(i) {
                        *x = *t;
                    }
                }
                v.blend.target.clone_from(&v.blend.w);
                continue;
            }
            // TnStraferAnimBlend.Tick: the way it moves (its velocity and its
            // acceleration in its own axes along the ground, mirrored while it is
            // left-handed) gives its forward, backward, right and left weights
            // (CalculateWeightsFromDirection), each one's moving its BlendSpeed's
            // share of the way there a tick.
            if let Some(speed) = b.strafer {
                let mut accel = local_of(p, p.accel * p.accel_rate);
                if left_handed {
                    accel.y = -accel.y;
                }
                let d = (local + accel).normalize_or_zero();
                let w = if d.length_squared() < 0.1 {
                    Vec2::X
                } else {
                    Vec2::new(d.x.signum() * d.x * d.x, d.y.signum() * d.y * d.y)
                };
                let targets = [w.x.max(0.0), (-w.x).max(0.0), w.y.max(0.0), (-w.y).max(0.0)];
                let share = 1.0 - (1.0 - speed.clamp(0.0, 1.0)).powf(dt * STRAFER_TICKS);
                let v = &mut self.vblends[k];
                for (i, t) in targets.iter().enumerate() {
                    if let Some(x) = v.blend.w.get_mut(i) {
                        *x += (t - *x) * share;
                    }
                }
                v.blend.target.clone_from(&v.blend.w);
                continue;
            }
            let samples: Vec<Vec2> = b.children.iter().map(|ch| ch.velocity).collect();
            let w = sample_weights(local, &samples, &b.tris);
            let v = &mut self.vblends[k];
            v.blend.set_targets(&w, VELOCITY_BLEND_TIME);
            v.blend.update(dt);
        }

        // The mirror (TnAnimNodeHandednessMirror's, in a tree with one).
        let want = if c.mirror == Mirror::Handedness && left_handed && p.anim.mirroring_allowed() {
            1.0
        } else {
            0.0
        };
        if want != self.mirror_target {
            self.mirror_target = want;
            self.mirror_left = MIRROR_BLEND_TIME;
        }
        if self.mirror_left > 0.0 {
            if self.mirror_left <= dt {
                self.mirror = self.mirror_target;
                self.mirror_left = 0.0;
            } else {
                self.mirror += (self.mirror_target - self.mirror) * dt / self.mirror_left;
                self.mirror_left -= dt;
            }
        }

        // The base's nodes in play and their times, and its clips.
        let mut b = Builder::default();
        let rest = 1.0 - acro;
        // The transition.
        let tw = self.stance.get(0);
        if tw * stance_scale > RELEVANT {
            let t0 = self.transition.time;
            if let Some(k) = self.transition.clip.and_then(|k| c.get(k)) {
                let len = k.secs;
                self.transition.time =
                    (self.transition.time + dt * self.transition_rate * k.rate).clamp(0.0, len);
                // (Played backwards, its notifies the other way.)
                let (a, b2) = if self.transition_rate < 0.0 {
                    (self.transition.time, t0)
                } else {
                    (t0, self.transition.time)
                };
                let end = self.transition_rate > 0.0 && self.transition.time >= len;
                sound_notes(
                    c,
                    self.transition.clip,
                    a,
                    b2,
                    false,
                    end,
                    stance_scale * tw,
                    &mut notes,
                );
            }
            self.transition.relevant = true;
        } else {
            self.transition.relevant = false;
        }
        b.add(
            Layer::Base,
            self.transition.clip,
            rest * tw,
            self.transition.time,
        );
        // The 'Strafers' group's master: its heaviest member in play (only its
        // notifies play: the group fires no slave's, bFireSlaveNotifies).
        let mut master: Option<(f32, f32)> = None;
        let mut synched: Vec<Synched> = Vec::new();
        for (pi, pr) in c.profiles.iter().enumerate() {
            let pw = self.stance.get(pi + 1);
            match &pr.kind {
                ProfileKind::Switch => {
                    let idle_w = pw * self.switch.get(0);
                    // (Its turn in place: its source's share, and its player's.)
                    let (source_w, turn_w) = match c.turn.as_ref() {
                        Some(t) => {
                            self.turn_in_place(c, t, p, stance_scale * idle_w, dt, &mut notes);
                            (self.turn.blend.get(0), self.turn.blend.get(1))
                        }
                        None => (1.0, 0.0),
                    };
                    let idle_clip = c.switch.idle;
                    let len = idle_clip.and_then(|k| c.get(k)).map_or(0.0, |k| k.secs);
                    let was = self.idle.was();
                    self.idle.tick(
                        stance_scale * idle_w * source_w,
                        dt,
                        rate_of(c, idle_clip),
                        len,
                        false,
                        true,
                        || idle_clip,
                    );
                    if let Some((t0, t1, wrapped)) = self.idle.span(was, false) {
                        sound_notes(
                            c,
                            self.idle.clip,
                            t0,
                            t1,
                            wrapped,
                            false,
                            stance_scale * idle_w * source_w,
                            &mut notes,
                        );
                    }
                    b.add(
                        Layer::Base,
                        self.idle.clip,
                        rest * idle_w * source_w,
                        self.idle.time,
                    );
                    if turn_w > RELEVANT {
                        b.add(
                            Layer::Base,
                            self.turn.player.clip,
                            rest * idle_w * turn_w,
                            self.turn.player.time,
                        );
                    }
                    if let Some(k) = c.switch.moving {
                        let mw = pw * self.switch.get(1);
                        self.vblend_pose(
                            c,
                            k,
                            mw,
                            stance_scale,
                            rest,
                            dt,
                            roll,
                            &mut b,
                            &mut master,
                            (&mut notes, &mut synched),
                        );
                    }
                }
                ProfileKind::VBlend(k) => {
                    self.vblend_pose(
                        c,
                        *k,
                        pw,
                        stance_scale,
                        rest,
                        dt,
                        roll,
                        &mut b,
                        &mut master,
                        (&mut notes, &mut synched),
                    );
                }
                ProfileKind::Seq { clip, group } => {
                    let pick = pick_of(c, *clip, group.as_deref(), roll());
                    let seq = &mut self.profile_seqs[pi].0;
                    let len = seq
                        .clip
                        .or(*clip)
                        .and_then(|k| c.get(k))
                        .map_or(0.0, |k| k.secs);
                    let was = seq.was();
                    let rate = rate_of(c, seq.clip.or(*clip));
                    seq.tick(stance_scale * pw, dt, rate, len, true, true, || pick);
                    if let Some((t0, t1, wrapped)) = seq.span(was, true) {
                        sound_notes(
                            c,
                            seq.clip,
                            t0,
                            t1,
                            wrapped,
                            false,
                            stance_scale * pw,
                            &mut notes,
                        );
                    }
                    b.add(Layer::Base, seq.clip, rest * pw, seq.time);
                }
                ProfileKind::Add {
                    clip,
                    group,
                    add,
                    add_group,
                } => {
                    let pick = pick_of(c, *clip, group.as_deref(), roll());
                    let pick_add = pick_of(c, *add, add_group.as_deref(), roll());
                    let (seq, add_seq) = &mut self.profile_seqs[pi];
                    let len = seq
                        .clip
                        .or(*clip)
                        .and_then(|k| c.get(k))
                        .map_or(0.0, |k| k.secs);
                    let (was, add_was) = (seq.was(), add_seq.was());
                    let rate = rate_of(c, seq.clip.or(*clip));
                    seq.tick(stance_scale * pw, dt, rate, len, true, true, || pick);
                    let len = add_seq
                        .clip
                        .or(*add)
                        .and_then(|k| c.get(k))
                        .map_or(0.0, |k| k.secs);
                    let rate = rate_of(c, add_seq.clip.or(*add));
                    add_seq.tick(stance_scale * pw, dt, rate, len, true, true, || pick_add);
                    for (s, w) in [(&*seq, was), (&*add_seq, add_was)] {
                        if let Some((t0, t1, wrapped)) = s.span(w, true) {
                            sound_notes(
                                c,
                                s.clip,
                                t0,
                                t1,
                                wrapped,
                                false,
                                stance_scale * pw,
                                &mut notes,
                            );
                        }
                    }
                    b.add(Layer::Base, seq.clip, rest * pw, seq.time);
                    b.add(Layer::BaseAdd, add_seq.clip, rest * pw, add_seq.time);
                }
                ProfileKind::Plane => {
                    if let Some(pt) = c.plane.as_ref() {
                        self.plane_pose(
                            c,
                            pt,
                            p,
                            pw,
                            (stance_scale, rest),
                            dt,
                            roll,
                            &mut b,
                            &mut notes,
                        );
                    }
                }
            }
        }
        if let Some(m) = synched.iter().max_by(|a, b| a.weight.total_cmp(&b.weight)) {
            sound_notes(
                c, m.clip, m.t0, m.t1, m.wrapped, false, m.weight, &mut notes,
            );
        }
        // The 'Strafers' group moves on by its master's clip, as fast as its
        // RateScale plays it (`master`'s length is its cycle's: its clip's over
        // its rate).
        if let Some((_, len)) = master
            && len > 0.0
        {
            self.strafers = (self.strafers + dt / len).rem_euclid(1.0);
        }
        // The acrobatics slot's clips.
        if acro > RELEVANT {
            for ch in &self.acro.channels {
                b.add(Layer::Base, c.find(&ch.anim), ch.weight, ch.time);
            }
        }

        // The custom slots and the jump.
        let players = &p.anim.players;
        let mut shares = [0.0; LAYERS];
        for l in [Layer::Base, Layer::BaseAdd, Layer::Aim, Layer::Add] {
            shares[l as usize] = 1.0;
        }
        for (layer, mask) in [
            (Layer::Upper, anim::MASK_UPPER_BODY),
            (Layer::LeftArm, anim::MASK_LEFT_ARM),
        ] {
            if c.slots[mask as usize] {
                shares[layer as usize] = slot_clips(&mut b, c, &players[mask as usize].slot, layer);
            }
        }
        let mut full_total = 0.0;
        for mask in [anim::MASK_FULL_BODY, anim::MASK_WEAPON, anim::MASK_SHOULDER] {
            if c.slots[mask as usize] {
                full_total += slot_clips(&mut b, c, &players[mask as usize].slot, Layer::Full);
            }
        }
        if full_total > 0.0 {
            // (The slots share the layer by their weights.)
            for e in b.clips.iter_mut().filter(|e| e.0 == Layer::Full) {
                e.2 *= 1.0 / full_total.max(1.0);
            }
        }
        shares[Layer::Full as usize] = full_share;
        if jump_share > RELEVANT {
            for (i, ch) in c.jump.children.iter().enumerate().skip(1) {
                let Some(ch) = ch else { continue };
                let w = self.jump.blend.get(i) / jump_share.max(1e-6);
                let secs = ch.clip.and_then(|k| c.get(k)).map_or(0.0, |k| k.secs);
                let at = |phase: f32| (phase * secs).min((secs - 1e-3).max(0.0));
                // (The 'Jumps' group's notifies: its master's.)
                if i == self.jump.master {
                    sound_notes(
                        c,
                        ch.clip,
                        at(jump_was),
                        at(self.jump.phase),
                        false,
                        false,
                        jump_share * w,
                        &mut notes,
                    );
                }
                b.add(Layer::Jump, ch.clip, w, at(self.jump.phase));
            }
        }
        shares[Layer::Jump as usize] = jump_share;
        let add = &players[anim::MASK_ADDITIVE as usize].slot;
        for ch in &add.channels {
            b.add(Layer::Add, c.find(&ch.anim), ch.weight, ch.time);
        }

        // TnAnimNodeAimer: the aim poses' changes from the aim of the pose under it
        // (the base's, and each override layer's under it over that at its share)
        // to the pawn's.
        if let Some(aim) = c.aim.as_ref() {
            let enabled = aim.stances.contains(&stance);
            let aim_of = |layer: Layer| {
                let (mut sum, mut total) = (Vec2::ZERO, 0.0);
                for &(l, k, w, ..) in &b.clips {
                    if l == layer && c.clips[k].kind == ClipKind::Pose {
                        sum += c.clips[k].aim * w;
                        total += w;
                    }
                }
                (total > 0.0).then(|| sum / total)
            };
            let mut input = aim_of(Layer::Base).unwrap_or(Vec2::ZERO);
            let below = c.layer_index(Layer::Aim).unwrap_or(c.layers.len());
            for &l in &c.layers[..below] {
                let share = shares[l as usize];
                if l.overrides()
                    && share > RELEVANT
                    && let Some(over) = aim_of(l)
                {
                    input = input.lerp(over, share.min(1.0));
                }
            }
            // (TnAimAnimator.get_AimOffset: its AimOffsetScale's share of the aim,
            // within a full turn each way.)
            let scale = p.aim_offset_scale;
            let mut desired = Vec2::new(
                p.aimer.current.1 / 16384.0 * scale,
                p.aimer.current.0 / 16384.0 * scale,
            );
            if left_handed {
                desired.x = -desired.x;
            }
            let desired = desired.clamp(Vec2::splat(-1.0), Vec2::splat(1.0));
            let change = (aim.speed * dt).min(1.0);
            self.aim_offset = self.aim_offset.lerp(desired - input, change);
            let target = if enabled && p.aim_node_enabled {
                1.0
            } else {
                0.0
            };
            self.aim_strength += (target - self.aim_strength) * change;
            if self.aim_strength > RELEVANT && !aim.poses.is_empty() {
                let samples: Vec<Vec2> = aim.poses.iter().map(|(_, d)| *d).collect();
                let n = samples.len();
                let fan: Vec<[usize; 3]> = (1..n)
                    .map(|i| [0, i, if i + 1 < n { i + 1 } else { 1 }])
                    .collect();
                let w = sample_weights(self.aim_offset, &samples, &fan);
                for (i, (k, _)) in aim.poses.iter().enumerate() {
                    b.add(Layer::Aim, Some(*k), w[i] * self.aim_strength, 0.0);
                }
            }
        }
        // The aim offset node (AnimNodeAimOffset): its aim the pawn's
        // AimOffsetPct, mirrored while it is left-handed if it switches
        // (SwitchAimWhenLeftHanded), eased in at its InterpSpeed
        // (TnAnimNodeAimOffset's; High Moon's takes it as it is); its profile by
        // the pawn's stance.
        let aim_offset = c.aim_offset.as_ref().map(|ao| {
            let mut want = p.aim_offset_pct;
            if ao.switch_left && left_handed {
                want.x = -want.x;
            }
            self.ao_aim = if ao.interp_speed > 0.0 {
                Vec2::new(
                    ue::finterp_to(self.ao_aim.x, want.x, dt, ao.interp_speed),
                    ue::finterp_to(self.ao_aim.y, want.y, dt, ao.interp_speed),
                )
            } else {
                want
            };
            (ao.profile_for(stance) as u8, self.ao_aim)
        });
        p.anim
            .notifies
            .extend(notes.into_iter().map(|(n, w)| (anim::TREE, n, w)));
        Pose {
            aim_offset,
            clips: b
                .clips
                .into_iter()
                .map(|(layer, clip, weight, time, _)| PoseClip {
                    layer,
                    clip: clip as u16,
                    weight,
                    time,
                })
                .collect(),
            shares,
            mirror: self.mirror,
        }
    }

    /// TnAnimTurnInPlace.Tick at `total` weight: the yaw the pawn has turned through
    /// standing (and the way it is turning: its yaw rate of late, looked ahead), a
    /// turn's animation played for it once it is near enough the turn's angle,
    /// blended back to its source as the turn is done.
    fn turn_in_place(
        &mut self,
        c: &Clips,
        t: &super::clips::TurnInPlace,
        p: &Pawn,
        total: f32,
        dt: f32,
        notes: &mut Vec<(ClipNotify, f32)>,
    ) {
        let s = &mut self.turn;
        let turning = |s: &TurnState| {
            s.player.clip.is_some()
                && ((s.blend.get(0) - 1.0).abs() > 1e-4 || s.blend.target.get(1) == Some(&1.0))
        };
        if total < 1e-4 || dt <= 0.0 {
            if turning(s) {
                s.blend = Blend::new(2, 0);
                s.playing = false;
            }
            s.relevant = false;
            return;
        }
        // OnBecomeRelevant.
        if !s.relevant {
            s.relevant = true;
            s.previous_yaw = p.yaw;
            s.yaw_offset = 0.0;
            s.rates.clear();
        }
        let delta = ue::wrap_rot(p.yaw - s.previous_yaw);
        s.clock += dt;
        s.rates.push_back((s.clock, delta / dt));
        while s
            .rates
            .front()
            .is_some_and(|(at, _)| s.clock - at > TURN_RATE_SAMPLE_SECS)
        {
            s.rates.pop_front();
        }
        let average = s.rates.iter().map(|(_, r)| r).sum::<f32>() / s.rates.len().max(1) as f32;
        s.yaw_offset = ue::wrap_rot(s.yaw_offset + delta);
        s.previous_yaw = p.yaw;
        let amount = s.yaw_offset + average * t.look_ahead;
        if turning(s) {
            let (angle, clip) = t.turns[s.index.min(t.turns.len() - 1)];
            let len = c.get(clip).map_or(0.0, |k| k.secs).max(1e-3);
            let remaining = len - s.player.time;
            let max_rate = angle.abs() / len * 182.04445;
            let turn = (amount / dt).clamp(-max_rate, max_rate) * dt;
            s.yaw_offset -= turn;
            let complete = !s.playing || remaining <= t.blend_time || amount.abs() < FINISHED_TURN;
            if complete && s.blend.target.first() != Some(&1.0) {
                s.blend.to_full(0, t.blend_time.min(remaining.max(0.0)));
            }
        } else {
            // FindBestTurnIndex: the turn the right way whose angle it is past (less
            // the threshold) and nearest.
            let degrees = amount * 0.005493;
            let best = t
                .turns
                .iter()
                .enumerate()
                .filter(|(_, (a, _))| degrees * a >= 0.0 && degrees.abs() >= a.abs() - t.threshold)
                .min_by(|(_, (a, _)), (_, (b, _))| {
                    (degrees - a).abs().total_cmp(&(degrees - b).abs())
                })
                .map(|(i, _)| i);
            match best {
                Some(i) => {
                    s.index = i;
                    s.player.clip = Some(t.turns[i].1);
                    s.player.time = 0.0;
                    s.playing = true;
                    s.blend.to_full(1, t.blend_time);
                }
                None => {
                    if s.blend.target.first() != Some(&1.0) {
                        s.blend.to_full(0, t.blend_time);
                    }
                }
            }
        }
        s.blend.update(dt);
        // Its player (played once, not looping) and its sounds.
        if s.playing
            && let Some(k) = s.player.clip.and_then(|k| c.get(k))
        {
            let t0 = s.player.time;
            s.player.time = (s.player.time + dt * k.rate).min(k.secs);
            let end = s.player.time >= k.secs;
            if end {
                s.playing = false;
            }
            sound_notes(
                c,
                s.player.clip,
                t0,
                s.player.time,
                false,
                end,
                total * s.blend.get(1),
                notes,
            );
        }
    }

    /// The weight a velocity blend has of the stance blend's pose.
    fn vblend_weight(&self, c: &Clips, k: usize) -> f32 {
        let mut w = 0.0;
        for (pi, pr) in c.profiles.iter().enumerate() {
            let pw = self.stance.get(pi + 1);
            match pr.kind {
                ProfileKind::Switch if c.switch.moving == Some(k) => w += pw * self.switch.get(1),
                ProfileKind::VBlend(b) if b == k => w += pw,
                _ => {}
            }
        }
        w
    }

    /// A velocity blend's children at `weight` of the stance blend's pose: their
    /// nodes in play and times, into the base.
    #[allow(clippy::too_many_arguments)]
    fn vblend_pose(
        &mut self,
        c: &Clips,
        k: usize,
        weight: f32,
        stance_scale: f32,
        rest: f32,
        dt: f32,
        roll: &mut dyn FnMut() -> f32,
        b: &mut Builder,
        master: &mut Option<(f32, f32)>,
        (notes, synched): (&mut Vec<(ClipNotify, f32)>, &mut Vec<Synched>),
    ) {
        let Some(blend) = c.vblends.get(k) else {
            return;
        };
        let phase = self.strafers;
        let stance = self.stance_now;
        let v = &mut self.vblends[k];
        for (i, ch) in blend.children.iter().enumerate() {
            let w = weight * v.blend.get(i);
            let total = stance_scale * w;
            // (A TnStanceAnimNodeSequence: its picker's clip for the stance.)
            let by_stance = (!ch.picks.is_empty()).then(|| {
                ch.picks
                    .iter()
                    .find(|(st, _)| *st == stance)
                    .map_or(ch.clip, |(_, k)| Some(*k))
            });
            let pick =
                by_stance.unwrap_or_else(|| pick_of(c, ch.clip, ch.group.as_deref(), roll()));
            let seq = &mut v.seqs[i];
            // (It changes its animation as the stance changes, in play or not.)
            if let Some(Some(want)) = by_stance
                && seq.clip.is_some()
                && seq.clip != Some(want)
            {
                seq.clip = Some(want);
            }
            let len = seq
                .clip
                .or(ch.clip)
                .and_then(|x| c.get(x))
                .map_or(0.0, |x| x.secs);
            // (Its idle is a plain sequence; the rest play as they come in.)
            let auto = i > 0;
            let was = seq.was();
            let rate = rate_of(c, seq.clip.or(ch.clip));
            seq.tick(total, dt, rate, len, auto, !ch.synched, || pick);
            let len = seq.clip.and_then(|x| c.get(x)).map_or(0.0, |x| x.secs);
            if ch.synched && total > RELEVANT {
                seq.time = phase * len;
                if master.is_none_or(|(mw, _)| total > mw) {
                    *master = Some((total, len / rate_of(c, seq.clip)));
                }
                // (A member come into play joins the group where it is: nothing
                // passed.)
                if was.relevant && was.clip == seq.clip {
                    synched.push(Synched {
                        clip: seq.clip,
                        t0: was.time,
                        t1: seq.time,
                        wrapped: seq.time < was.time,
                        weight: total,
                    });
                }
            } else if !ch.synched
                && let Some((t0, t1, wrapped)) = seq.span(was, auto)
            {
                sound_notes(c, seq.clip, t0, t1, wrapped, false, total, notes);
            }
            b.add(Layer::Base, seq.clip, rest * w, seq.time);
        }
    }

    /// A plane's vehicle form switch at `weight` of the stance blend's pose
    /// (VEH_Plane_ANIMTREE's 'VehicleForm', its selector the plane form's
    /// _Boosting): its states' nodes in play and times, into the base.
    ///
    /// * **A change of state** plays the state it leaves' transition out
    ///   (HmAnimTransitionWithBlend): its clip blended in over its BlendInTime,
    ///   then the new state blended in over its BlendOutTime as the clip ends; with
    ///   no clip, the new state blended in over its BlendInTime. (Its clip is its
    ///   switch's Normal one: the switch's selector, the form's BoostMalfunction,
    ///   is no property of any form's, so it never picks Collision.)
    /// * **Hovering**: TnAccelerationAnimBlend (its velocity, no faster than its
    ///   MaxVelocity, in its own axes over that, mirrored while it is left-handed,
    ///   scaled by how upright it is: its idle, forward, backward, right, left, up
    ///   and down by SetWeightsFromAxes) with its additive over it (at full weight).
    ///   A child whose clip its set lacks (the jets' have no ascent) plays the
    ///   skeleton's reference pose in the game: here its idle's.
    /// * **Boosting**: its trigger switch (TnAnimTriggerSwitch) plays its transition
    ///   in as the state comes into play, then its roll blend (TnRollAnimBlend: its
    ///   roll over its MaxAngle, to the left or to the right of its centre).
    #[allow(clippy::too_many_arguments)]
    fn plane_pose(
        &mut self,
        c: &Clips,
        pt: &super::clips::PlaneTree,
        p: &Pawn,
        weight: f32,
        (stance_scale, rest): (f32, f32),
        dt: f32,
        roll: &mut dyn FnMut() -> f32,
        b: &mut Builder,
        notes: &mut Vec<(ClipNotify, f32)>,
    ) {
        let (boosting, rot) = match p.plane.as_ref() {
            Some(f) => (f.boosting, f.rotation),
            None => (false, Vec3::new(0.0, p.yaw, 0.0)),
        };
        let s = &mut self.plane;
        if !s.ready {
            s.ready = true;
            s.boosting = boosting;
            s.blend = Blend::new(3, boosting as usize);
            s.boost_relevant = boosting;
        }
        // The switch: a change of state, its transition out.
        if boosting != s.boosting {
            let out = pt.outs[s.boosting as usize];
            let to = boosting as usize;
            match out.clip {
                Some(k) => {
                    s.out.clip = Some(k);
                    s.out.time = 0.0;
                    s.out.relevant = false;
                    s.out_to = to;
                    s.out_blend_out = out.blend_out;
                    s.out_blending = false;
                    s.blend.to_full(2, out.blend_in);
                }
                None => s.blend.to_full(to, out.blend_in),
            }
            s.boosting = boosting;
        }
        // (Its transition's clip near its end: the new state blended in.)
        if s.blend.target.get(2).copied() == Some(1.0) && !s.out_blending {
            let len = s.out.clip.and_then(|k| c.get(k)).map_or(0.0, |k| k.secs);
            if s.out.time >= len - s.out_blend_out {
                s.blend.to_full(s.out_to, s.out_blend_out);
                s.out_blending = true;
            }
        }
        s.blend.update(dt);
        let hover_w = weight * s.blend.get(0);
        let boost_w = weight * s.blend.get(1);
        let out_w = weight * s.blend.get(2);

        // A sequence's frame: its clip's notifies passed, its clip into the base.
        let play = |seq: &mut Seq,
                    clip: Option<usize>,
                    w: f32,
                    looping: bool,
                    layer: Layer,
                    b: &mut Builder,
                    notes: &mut Vec<(ClipNotify, f32)>| {
            let len = seq
                .clip
                .or(clip)
                .and_then(|k| c.get(k))
                .map_or(0.0, |k| k.secs);
            let was = seq.was();
            let rate = rate_of(c, seq.clip.or(clip));
            seq.tick(stance_scale * w, dt, rate, len, !looping, looping, || clip);
            if !looping && seq.relevant && len > 0.0 {
                seq.time = (seq.time + dt * rate).min(len);
            }
            if let Some((t0, t1, wrapped)) = seq.span(was, !looping) {
                let end = !looping && seq.time >= len;
                sound_notes(c, seq.clip, t0, t1, wrapped, end, stance_scale * w, notes);
            }
            b.add(layer, seq.clip, rest * w, seq.time);
        };

        // Hovering: its acceleration blend and its additive.
        let mut hw = accel_weights(pt, p, rot);
        for i in 1..7 {
            if pt.hover[i].is_none() || pt.hover[i] == pt.hover[0] {
                hw[0] += hw[i];
                hw[i] = 0.0;
            }
        }
        for ((seq, clip), w) in s.hover.iter_mut().zip(pt.hover).zip(hw) {
            play(seq, clip, hover_w * w, true, Layer::Base, b, notes);
        }
        let add = pick_of(c, pt.hover_add, pt.hover_add_group.as_deref(), roll());
        if s.add.clip.is_none() {
            s.add.clip = add;
        }
        play(&mut s.add, add, hover_w, true, Layer::BaseAdd, b, notes);

        // Boosting: its trigger switch, its transition in then its roll blend.
        let boost_now = stance_scale * boost_w > RELEVANT;
        if boost_now && !s.boost_relevant {
            s.trigger = 1;
        }
        s.boost_relevant = boost_now;
        if s.trigger == 1 && pt.boost_in.is_none() {
            s.trigger = 0;
        }
        let in_w = if s.trigger == 1 { boost_w } else { 0.0 };
        play(
            &mut s.boost_in,
            pt.boost_in,
            in_w,
            false,
            Layer::Base,
            b,
            notes,
        );
        // (OnChildAnimEndNotify: its transition in done, its roll blend.)
        let in_len = pt.boost_in.and_then(|k| c.get(k)).map_or(0.0, |k| k.secs);
        if s.trigger == 1 && s.boost_in.relevant && s.boost_in.time >= in_len {
            s.trigger = 0;
        }
        let roll_deg = ue::wrap_rot(rot.z) * 360.0 / 65536.0;
        let rw = (roll_deg.abs() / pt.max_roll.max(1e-3)).clamp(0.0, 1.0);
        let lr = if roll_deg < 0.0 {
            [rw, 1.0 - rw, 0.0]
        } else {
            [0.0, 1.0 - rw, rw]
        };
        let steady_w = if s.trigger == 0 { boost_w } else { 0.0 };
        for ((seq, clip), w) in s.boost.iter_mut().zip(pt.boost).zip(lr) {
            play(seq, clip, steady_w * w, true, Layer::Base, b, notes);
        }

        // The transition out's clip.
        let clip = s.out.clip;
        play(&mut s.out, clip, out_w, false, Layer::Base, b, notes);
    }

    /// TnJumpTrajectoryBlend.ClientJump: its jumps weighted by the trajectory, the
    /// heaviest the group's master, the group played over the jump's time.
    fn client_jump(&mut self, c: &Clips, trajectory: Vec2, time: f32) {
        let j = &c.jump;
        if j.children.len() < 3 || j.tris.is_empty() {
            return;
        }
        let samples: Vec<Vec2> = j
            .children
            .iter()
            .map(|ch| ch.as_ref().map_or(Vec2::ZERO, |ch| ch.trajectory))
            .collect();
        let w = sample_weights(trajectory, &samples, &j.tris);
        self.jump.blend.set_targets(&w, 0.1);
        self.jump.master = (0..w.len())
            .max_by(|&a, &b| w[a].total_cmp(&w[b]))
            .unwrap_or(0);
        self.jump.time = time.max(0.05);
        self.jump.phase = 0.0;
        self.jump.jumping = true;
    }

    /// PlayLandingAnimation: the master jump's landing on the full body's player
    /// (whether it plays: the pawn holds still while it does).
    fn play_landing(&mut self, c: &Clips, p: &mut Pawn) -> bool {
        let m = self.jump.master;
        let Some(Some(ch)) = c.jump.children.get(m) else {
            return false;
        };
        if m == 0 || &*ch.landing == "None" || ch.landing.is_empty() {
            return false;
        }
        let mut params = AnimParams::make(&ch.landing, 0.1, 0.2, anim::RMM_ACCEL);
        params.priority = 1;
        // (LandingAnimationEnabled(false): its acrobatics service's landing not
        // played over it.)
        p.landing_animation_enabled = false;
        p.anim
            .enqueue(params, Some(CB_JUMP_LANDED), Some(CB_JUMP_LANDED));
        p.motion.playing_jump_landing = true;
        true
    }

    /// TnAcrobaticsSlotLogic.Tick: a new acrobatics state's animation (the jump's
    /// take-off, then its loop; the fall's loop), or the slot back to the pose.
    fn update_acro(&mut self, c: &Clips, p: &Pawn, dt: f32, in_play: bool) {
        let state = acro_state(p);
        if state != self.acro_state {
            self.acro_state = state;
            self.acro_enter = None;
            match state {
                ACRO_JUMP => self.acro_play(c, ACRO_JUMP, in_play),
                ACRO_FALL => self.acro_play(c, ACRO_FALL, in_play),
                _ => self.acro.stop_custom(self.acro_blend_out),
            }
        }
        self.acro.advance_custom(dt);
        // The take-off's end (EnterAnimationComplete): the jump's loop.
        if let Some(anim) = self.acro_enter.clone()
            && self
                .acro
                .channels
                .iter()
                .find(|ch| ch.anim == anim)
                .is_none_or(|ch| ch.time >= ch.length)
        {
            self.acro_enter = None;
            if self.acro_state == ACRO_JUMP {
                self.acro_play(c, ACRO_JUMP_LOOP, in_play);
            }
        }
    }

    fn acro_play(&mut self, c: &Clips, which: u8, blend: bool) {
        let Some(a) = c.acro.get(&which) else {
            return;
        };
        let Some(k) = c.get(a.clip) else { return };
        let blend_in = if blend { a.blend_in } else { 0.0 };
        self.acro.play_custom(
            k.name.clone(),
            k.secs,
            k.rate,
            blend_in,
            a.looping,
            a.blend_out,
        );
        self.acro_blend_out = a.blend_out;
        if !a.looping {
            self.acro_enter = Some(k.name.clone());
        }
    }
}

/// The acrobatics state the slot plays for (its states with animations: the jump
/// and the fall).
fn acro_state(p: &Pawn) -> u8 {
    match p.acrobatics() {
        Acrobatics::Jumping => ACRO_JUMP,
        Acrobatics::Falling => ACRO_FALL,
        a => a as u8,
    }
}

/// A clip's RateScale (none: 1): the game's AnimNodeSequence plays its clip that
/// much faster (Rate times its AnimSeq's RateScale), its time in the clip's own
/// seconds. (A Swarmer's jog, 1.3; a Guardian's investigating walk, 0.9.)
fn rate_of(c: &Clips, clip: Option<usize>) -> f32 {
    clip.and_then(|k| c.get(k))
        .map_or(1.0, |k| k.rate)
        .max(1e-3)
}

/// A sequence's clip as it comes into play: its chooser group's pick, else its
/// own.
fn pick_of(c: &Clips, clip: Option<usize>, group: Option<&str>, roll: f32) -> Option<usize> {
    group.and_then(|g| c.resolve(g, roll)).or(clip)
}

/// The pawn's velocity in its own axes (Velocity << Rotation: forward, right).
fn local_velocity(p: &Pawn) -> Vec2 {
    local_of(p, p.vel)
}

/// A vector in the pawn's own axes along the ground (forward, right).
fn local_of(p: &Pawn, v: Vec3) -> Vec2 {
    let f = ue::yaw_vec(p.yaw);
    let r = ue::yaw_vec(p.yaw + 16384.0);
    Vec2::new(v.dot(f), v.dot(r))
}

/// A vector in the pawn's own axes (Velocity << Rotation: forward, right, up).
fn local3(p: &Pawn, v: Vec3) -> Vec3 {
    let l = local_of(p, v);
    Vec3::new(l.x, l.y, v.z)
}

/// TnFlyingRobotAnimBlend.GetWeightsFromVelocity: each way's share (Pct: of its
/// speeds' range, signed); short of full speed forward (or going nowhere else)
/// only forward and backward; else two other ways both past MinDiagonalWeight
/// share between them, or the greater of them shares with forward.
fn fly_weights(f: &super::clips::FlyBlend, v: Vec3) -> Vec3 {
    let pct = |x: f32, r: Vec2| {
        let span = r.y - r.x;
        let t = if span.abs() > 1e-6 {
            (x.abs() - r.x) / span
        } else {
            0.0
        };
        if x == 0.0 {
            0.0
        } else {
            x.signum() * t.clamp(0.0, 1.0)
        }
    };
    let forward = pct(v.x, f.forward);
    let right = pct(v.y, f.right);
    let up = pct(v.z, f.up);
    let non_forward = right.abs() + up.abs();
    if forward.abs() < 1.0 || non_forward == 0.0 {
        Vec3::new(forward, 0.0, 0.0)
    } else if right.abs() >= f.min_diagonal && up.abs() >= f.min_diagonal {
        Vec3::new(0.0, right / non_forward, up / non_forward)
    } else if right.abs() > up.abs() {
        Vec3::new(1.0 - right.abs(), right, 0.0)
    } else {
        Vec3::new(1.0 - up.abs(), 0.0, up)
    }
}

/// TnAccelerationAnimBlend's children's weights (Tick: CalculateLinearVelocityWeights
/// scaled by how upright the pawn is, then SetWeightsFromAxes): idle, forward,
/// backward, right, left, up, down. `rot`: its rotation (pitch, yaw, roll).
fn accel_weights(pt: &super::clips::PlaneTree, p: &Pawn, rot: Vec3) -> [f32; 7] {
    let [x, y, z] = ue::rotator_axes(rot.x, rot.y, rot.z);
    let max = pt.max_velocity.max(1.0);
    let v = p.vel.clamp_length_max(max);
    let mut local = Vec3::new(v.dot(x), v.dot(y), v.dot(z)) / max;
    if p.stance.handedness == 1 {
        local.y = -local.y;
    }
    let local = local * z.z.max(0.0);
    let motion = local.clamp_length_max(1.0);
    let length = motion.length();
    let mut out = [0.0; 7];
    if length < pt.min_input.max(0.0001) {
        out[0] = 1.0;
        return out;
    }
    let w = Vec3::new(
        motion.x.signum() * motion.x * motion.x,
        motion.y.signum() * motion.y * motion.y,
        motion.z.signum() * motion.z * motion.z,
    ) / length;
    out[0] = 1.0 - length;
    out[1] = w.x.max(0.0);
    out[2] = (-w.x).max(0.0);
    out[3] = w.y.max(0.0);
    out[4] = (-w.y).max(0.0);
    out[5] = w.z.max(0.0);
    out[6] = (-w.z).max(0.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Swarmers' tree: moving to its right a Swarmer strafes right
    /// (TnStraferAnimBlend), and turned a quarter round as it stands, it plays its
    /// pivot (TnAnimTurnInPlace) over its idle.
    #[test]
    fn a_swarmer_strafes_and_turns_in_place() {
        let Some(c) = super::super::clips::set("swarmer") else {
            return;
        };
        let mut p = Pawn::new(Entity::PLACEHOLDER, Vec3::ZERO, 0.0, 250.0, 200.0, 100.0);
        let mut tree = AnimTree::default();
        let mut roll = || 0.5;
        let dt = 1.0 / 30.0;
        let heaviest = |pose: &Pose| {
            pose.clips
                .iter()
                .filter(|k| k.layer == Layer::Base)
                .max_by(|a, b| a.weight.total_cmp(&b.weight))
                .map(|k| c.clips[k.clip as usize].name.to_string())
        };
        let pose = tree.update(&c, &mut p, dt, &mut roll);
        assert_eq!(heaviest(&pose).as_deref(), Some("Nav_Idle_03"));
        let right = ue::yaw_vec(p.yaw + 16384.0);
        p.vel = right * 1500.0;
        p.accel = right;
        let mut pose = Pose::default();
        for _ in 0..30 {
            pose = tree.update(&c, &mut p, dt, &mut roll);
        }
        assert_eq!(heaviest(&pose).as_deref(), Some("Nav_StrafeJog_R"));
        // Stopped, back to its idle; then turned right a quarter round.
        p.vel = Vec3::ZERO;
        p.accel = Vec3::ZERO;
        for _ in 0..30 {
            tree.update(&c, &mut p, dt, &mut roll);
        }
        let mut pivot = 0.0f32;
        for k in 0..20 {
            if k < 8 {
                p.yaw += 16384.0 / 8.0;
            }
            let pose = tree.update(&c, &mut p, dt, &mut roll);
            for e in &pose.clips {
                if &*c.clips[e.clip as usize].name == "Nav_IdlePivot90_R" {
                    pivot = pivot.max(e.weight);
                }
            }
        }
        assert!(pivot > 0.5, "the pivot played at {pivot}");
        // Done, it stands in its idle again.
        for _ in 0..30 {
            tree.update(&c, &mut p, dt, &mut roll);
        }
        let pose = tree.update(&c, &mut p, dt, &mut roll);
        assert_eq!(heaviest(&pose).as_deref(), Some("Nav_Idle_03"));
    }

    /// The heaviest clip of the pose's base.
    fn heaviest(c: &Clips, pose: &Pose) -> Option<String> {
        pose.clips
            .iter()
            .filter(|k| k.layer == Layer::Base)
            .max_by(|a, b| a.weight.total_cmp(&b.weight))
            .map(|k| c.clips[k.clip as usize].name.to_string())
    }

    /// A Sniper's flying robot (TnFlyingRobotAnimBlend): its idle standing still,
    /// its forward flight's clip flying ahead at full speed (eased in at its
    /// BlendSpeed), back to its idle as it stops.
    #[test]
    fn a_flying_robot_blends_by_the_way_it_flies() {
        let Some(c) = super::super::clips::set("sniper") else {
            return;
        };
        let mut p = Pawn::new(Entity::PLACEHOLDER, Vec3::ZERO, 0.0, 175.0, 200.0, 200.0);
        p.physics = Physics::Flying;
        let mut tree = AnimTree::default();
        let mut roll = || 0.5;
        let dt = 1.0 / 30.0;
        let pose = tree.update(&c, &mut p, dt, &mut roll);
        assert_eq!(heaviest(&c, &pose).as_deref(), Some("NAV_Idle"));
        p.vel = ue::yaw_vec(p.yaw) * 2000.0;
        let mut pose = Pose::default();
        for _ in 0..30 {
            pose = tree.update(&c, &mut p, dt, &mut roll);
        }
        assert_eq!(heaviest(&c, &pose).as_deref(), Some("Nav_Strafe_F"));
        p.vel = Vec3::ZERO;
        for _ in 0..60 {
            pose = tree.update(&c, &mut p, dt, &mut roll);
        }
        assert_eq!(heaviest(&c, &pose).as_deref(), Some("NAV_Idle"));
    }

    /// A jet's plane (VEH_Plane_ANIMTREE's vehicle form switch): its hover pose,
    /// its forward hover moving ahead; boosting, its transition in and then its
    /// boost (rolled right, its right turn's); back to hovering through its boost's
    /// transition out.
    #[test]
    fn a_plane_hovers_and_boosts() {
        let Some(c) = super::super::clips::set("flyer_alt") else {
            return;
        };
        assert!(c.plane.is_some(), "its vehicle form switch");
        let mut p = Pawn::new(Entity::PLACEHOLDER, Vec3::ZERO, 0.0, 445.0, 227.0, 100.0);
        let mut f = super::super::drones::PlaneForm::new(
            super::super::motion::MotionProfile::default(),
            800.0,
            Vec3::ZERO,
            0.0,
        );
        f.flight = None;
        p.plane = Some(Box::new(f));
        p.physics = Physics::RigidBody;
        let mut tree = AnimTree::default();
        let mut roll = || 0.5;
        let dt = 1.0 / 30.0;
        let mut run = |p: &mut Pawn, frames: usize| {
            let mut pose = Pose::default();
            let mut seen = Vec::new();
            for _ in 0..frames {
                pose = tree.update(&c, p, dt, &mut roll);
                if let Some(h) = heaviest(&c, &pose)
                    && seen.last() != Some(&h)
                {
                    seen.push(h);
                }
            }
            (pose, seen)
        };
        let (pose, _) = run(&mut p, 2);
        assert_eq!(heaviest(&c, &pose).as_deref(), Some("Nav_Hover_Pose_VEH"));
        assert!(
            pose.clips.iter().any(|k| k.layer == Layer::BaseAdd
                && &*c.clips[k.clip as usize].name == "ADD_Nav_Hover_TailFlaps_VEH"),
            "its additive over its hover"
        );
        p.vel = ue::yaw_vec(p.yaw) * 2500.0;
        let (pose, _) = run(&mut p, 2);
        assert_eq!(heaviest(&c, &pose).as_deref(), Some("Nav_Hover_F_VEH"));
        // Boosting: its transition in, then its boost.
        p.plane.as_mut().unwrap().boosting = true;
        let (_, seen) = run(&mut p, 60);
        assert!(
            seen.iter().any(|s| s == "Nav_Hover_to_Boost_VEH"),
            "no transition in: {seen:?}"
        );
        assert_eq!(seen.last().map(|s| s.as_str()), Some("Nav_Boost_VEH"));
        p.plane.as_mut().unwrap().rotation.z = 40.0 * ue::ROT_PER_DEG;
        let (pose, _) = run(&mut p, 2);
        assert_eq!(heaviest(&c, &pose).as_deref(), Some("Nav_Boost_R_VEH"));
        // And back.
        p.plane.as_mut().unwrap().boosting = false;
        p.plane.as_mut().unwrap().rotation.z = 0.0;
        p.vel = Vec3::ZERO;
        let (_, seen) = run(&mut p, 60);
        assert!(
            seen.iter().any(|s| s == "Nav_Boost_to_Hover_VEH"),
            "no transition out: {seen:?}"
        );
        assert_eq!(seen.last().map(|s| s.as_str()), Some("Nav_Hover_Pose_VEH"));
    }

    #[test]
    fn sample_weights_inside_and_outside() {
        let s = [Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)];
        let t = [[0, 1, 2]];
        let w = sample_weights(Vec2::new(0.25, 0.25), &s, &t);
        assert!(
            (w[0] - 0.5).abs() < 1e-4 && (w[1] - 0.25).abs() < 1e-4 && (w[2] - 0.25).abs() < 1e-4
        );
        // Beyond the hull: its nearest point's weights.
        let w = sample_weights(Vec2::new(2.0, 0.0), &s, &t);
        assert!((w[1] - 1.0).abs() < 1e-4);
        let w = sample_weights(Vec2::new(1.0, 1.0), &s, &t);
        assert!((w[1] - 0.5).abs() < 1e-4 && (w[2] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn blend_moves_straight_to_its_targets() {
        let mut b = Blend::new(2, 0);
        b.to_full(1, 0.2);
        b.update(0.1);
        assert!((b.get(1) - 0.5).abs() < 1e-4);
        b.update(0.1);
        assert!((b.get(1) - 1.0).abs() < 1e-4 && b.get(0).abs() < 1e-4);
    }
}
