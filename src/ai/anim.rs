//! An enemy's animation: its custom animation players (TnRobotAnimator's, one for
//! each of its anim tree's custom slots: the full body's, the upper body's, the
//! left arm's, the weapon's, the shoulder's, and TnAdditiveAnimatorService's
//! additive slot), as the AI's operators and services play animations through them
//! (TnCustomAnimationPlayer: a queue played each update, the new animation playing
//! if its priority lets it, its completion called back as it is about to blend
//! out, its failure as another takes over or it is stopped).
//!
//! What the anim tree makes of them and of the pawn's motion and stance for the
//! model's clips is the tree's own (tree.rs).

use std::sync::Arc;

use super::clips::ClipNotify;

/// ETnCustomAnimationMaskType.
pub const MASK_FULL_BODY: u8 = 0;
pub const MASK_UPPER_BODY: u8 = 1;
pub const MASK_LEFT_ARM: u8 = 2;
pub const MASK_WEAPON: u8 = 3;
pub const MASK_SHOULDER: u8 = 4;
/// The additive animator service's slot (TnAdditiveAnimatorService).
pub const MASK_ADDITIVE: u8 = 5;
pub const PLAYERS: usize = 6;

/// ETnCustomAnimationPlayOverride.
pub const OVERRIDE_IF_IDLE: u8 = 0;
pub const OVERRIDE_IF_HIGHER: u8 = 1;
pub const OVERRIDE_IF_HIGHER_OR_EQUAL: u8 = 2;

/// ERootMotionMode (UE3's: the custom animation params use its numbers).
pub const RMM_TRANSLATE: u8 = 0;
pub const RMM_VELOCITY: u8 = 1;
pub const RMM_IGNORE: u8 = 2;
pub const RMM_ACCEL: u8 = 3;

/// TnCustomAnimationParams.
#[derive(Clone, Debug)]
pub struct AnimParams {
    pub mask: u8,
    /// The animation's name, or a chooser group's (one of its animations is picked
    /// as it plays).
    pub name: Arc<str>,
    pub blend_in: f32,
    pub blend_out: f32,
    pub start_position: f32,
    pub play_rate: f32,
    pub looping: bool,
    pub synchronize: bool,
    pub mirroring_allowed: bool,
    pub complete_after_blend_out: bool,
    pub root_motion: u8,
    /// ERootMotionRotationMode: 0 ignore, 1 rotate the actor.
    pub root_rotation: u8,
    pub override_mode: u8,
    /// 0 low, 1 high.
    pub priority: u8,
}

impl Default for AnimParams {
    fn default() -> Self {
        AnimParams {
            mask: MASK_FULL_BODY,
            name: Arc::from("None"),
            blend_in: 0.1,
            blend_out: 0.1,
            start_position: 0.0,
            play_rate: 1.0,
            looping: false,
            synchronize: false,
            mirroring_allowed: true,
            complete_after_blend_out: false,
            root_motion: RMM_ACCEL,
            root_rotation: 0,
            override_mode: OVERRIDE_IF_HIGHER_OR_EQUAL,
            priority: 0,
        }
    }
}

impl AnimParams {
    /// Animation.MakeAnimationParams.
    pub fn make(name: &str, blend_in: f32, blend_out: f32, root_motion: u8) -> AnimParams {
        AnimParams {
            name: Arc::from(name),
            blend_in,
            blend_out,
            root_motion,
            ..Default::default()
        }
    }

    pub fn is_none(&self) -> bool {
        &*self.name == "None" || self.name.is_empty()
    }

    /// A TnCustomAnimationParams from the behaviour's values under `key`, `base`'s
    /// where it has none (its class's defaults).
    pub fn from_props(b: &super::data::Behaviour, key: &str, base: &AnimParams) -> AnimParams {
        let k = |f: &str| format!("{key}.{f}");
        let s = |f: &str| b.get(&k(f));
        AnimParams {
            mask: match s("MaskType") {
                Some("CAMT_UpperBody") => MASK_UPPER_BODY,
                Some("CAMT_LeftArm") => MASK_LEFT_ARM,
                Some("CAMT_Weapon") => MASK_WEAPON,
                Some("CAMT_Shoulder") => MASK_SHOULDER,
                Some("CAMT_FullBody") => MASK_FULL_BODY,
                _ => base.mask,
            },
            name: s("AnimName").map_or(base.name.clone(), Arc::from),
            blend_in: b.f(&k("BlendInTime"), base.blend_in),
            blend_out: b.f(&k("BlendOutTime"), base.blend_out),
            start_position: b.f(&k("StartPosition"), base.start_position),
            play_rate: b.f(&k("PlayRate"), base.play_rate),
            looping: b.b(&k("Looping"), base.looping),
            synchronize: b.b(&k("Synchronize"), base.synchronize),
            mirroring_allowed: b.b(&k("MirroringAllowed"), base.mirroring_allowed),
            complete_after_blend_out: b
                .b(&k("CompleteAfterBlendOut"), base.complete_after_blend_out),
            root_motion: match s("RootMotionMode") {
                Some("RMM_Translate") => RMM_TRANSLATE,
                Some("RMM_Velocity") => RMM_VELOCITY,
                Some("RMM_Ignore") => RMM_IGNORE,
                Some("RMM_Accel") => RMM_ACCEL,
                _ => base.root_motion,
            },
            root_rotation: match s("RootMotionRotationMode") {
                Some("RMRM_RotateActor") => 1,
                Some("RMRM_Ignore") => 0,
                _ => base.root_rotation,
            },
            override_mode: match s("Override") {
                Some("CAPO_PlayIfIdle") => OVERRIDE_IF_IDLE,
                Some("CAPO_PlayIfHigherPriority") => OVERRIDE_IF_HIGHER,
                Some("CAPO_PlayIfHigherOrEqualPriority") => OVERRIDE_IF_HIGHER_OR_EQUAL,
                _ => base.override_mode,
            },
            priority: match s("Priority") {
                Some("CAPR_HighPriority") => 1,
                Some("CAPR_LowPriority") => 0,
                _ => base.priority,
            },
        }
    }

    /// Animation.ExpandHighPriorityAnimationParameters.
    pub fn high_priority(mut self) -> AnimParams {
        self.priority = 1;
        self
    }
}

/// A delegate a custom animation calls back: the operator (by its serial number)
/// or the pawn (0), and which of its callbacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Callback {
    pub owner: u32,
    pub kind: u8,
}

/// The pawn's own callbacks (owner 0).
pub const CB_SPAWN_DONE: Callback = Callback { owner: 0, kind: 0 };
pub const CB_WAKE_UP: Callback = Callback { owner: 0, kind: 1 };

#[derive(Clone, Debug)]
struct Queued {
    params: AnimParams,
    complete: Option<Callback>,
    failed: Option<Callback>,
}

/// One of a slot's custom channels: the animation it plays (the chosen clip's
/// name), its time (the sequence's), length and rate (PlayRate times the clip's
/// RateScale), and its weight and the weight it blends to.
#[derive(Clone, Debug)]
pub struct Channel {
    pub anim: Arc<str>,
    pub time: f32,
    pub length: f32,
    pub rate: f32,
    pub looping: bool,
    pub weight: f32,
    pub target: f32,
}

/// A custom animation slot (AnimNodeSlot): its custom channels (the slot's two,
/// "Channel 01" and "Child3": a new animation plays on one as the other blends
/// out), blending together over `blend_left` seconds; its source has what weight
/// they leave. The root motion its animation gives the pawn (ERootMotionMode),
/// whether it turns the pawn, and whether the mirror may mirror it.
#[derive(Clone, Debug, Default)]
pub struct SlotState {
    pub channels: Vec<Channel>,
    pub blend_left: f32,
    pub root_motion: u8,
    pub root_rotation: u8,
    pub mirror_allowed: bool,
    /// The blend out its animation starts this long before its end
    /// (PendingBlendOutTime: a slot played without a player's completion).
    pending_blend_out: Option<f32>,
}

/// An AnimNodeSlot's custom channels.
const SLOT_CHANNELS: usize = 2;

impl SlotState {
    /// AnimNodeSlot.PlayCustomAnim: the animation from its start on a channel of
    /// its own, blended in as the rest blend out.
    fn play(&mut self, name: Arc<str>, length: f32, rate: f32, blend_in: f32, looping: bool) {
        if self.channels.len() >= SLOT_CHANNELS {
            // (Its channel: the faintest one's.)
            if let Some(i) = self
                .channels
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.weight.total_cmp(&b.1.weight))
                .map(|(i, _)| i)
            {
                self.channels.remove(i);
            }
        }
        for c in self.channels.iter_mut() {
            c.target = 0.0;
        }
        self.channels.push(Channel {
            anim: name,
            time: 0.0,
            length,
            rate,
            looping,
            weight: 0.0,
            target: 1.0,
        });
        self.blend_over(blend_in);
    }

    fn blend_over(&mut self, secs: f32) {
        if secs <= 0.0 {
            for c in self.channels.iter_mut() {
                c.weight = c.target;
            }
            self.blend_left = 0.0;
            self.channels.retain(|c| c.weight > 0.0);
        } else {
            self.blend_left = secs;
        }
    }

    /// StopCustomAnim: back to its source.
    fn stop(&mut self, blend_out: f32) {
        for c in self.channels.iter_mut() {
            c.target = 0.0;
        }
        self.blend_over(blend_out);
    }

    /// PlayCustomAnimWithCallbacks, as a slot's own logic plays it (the
    /// acrobatics slot's): it blends out `blend_out` before its end unless it
    /// loops.
    pub fn play_custom(
        &mut self,
        name: Arc<str>,
        length: f32,
        rate: f32,
        blend_in: f32,
        looping: bool,
        blend_out: f32,
    ) {
        self.play(name, length, rate, blend_in, looping);
        self.pending_blend_out = (!looping && blend_out > 0.0).then_some(blend_out);
    }

    /// StopCustomAnim.
    pub fn stop_custom(&mut self, blend_out: f32) {
        self.pending_blend_out = None;
        self.stop(blend_out);
    }

    /// A frame of a slot played by its own logic.
    pub fn advance_custom(&mut self, dt: f32) {
        self.advance(dt);
        if let Some(out) = self.pending_blend_out {
            let left = self.time_left();
            if self.current().is_some_and(|c| !c.looping) && left <= out {
                self.pending_blend_out = None;
                self.stop(left);
            }
        }
    }

    /// The channel it plays now (blending in or in full), if any.
    pub fn current(&self) -> Option<&Channel> {
        self.channels.last().filter(|c| c.target > 0.0)
    }

    fn current_mut(&mut self) -> Option<&mut Channel> {
        self.channels.last_mut().filter(|c| c.target > 0.0)
    }

    /// The custom channels' weight (the source has the rest).
    pub fn weight(&self) -> f32 {
        self.channels.iter().map(|c| c.weight).sum::<f32>().min(1.0)
    }

    /// The custom animation's time left (GetCustomAnimNodeSeq().GetTimeLeft()).
    pub fn time_left(&self) -> f32 {
        match self.current() {
            Some(c) if c.rate > 0.0 => ((c.length - c.time) / c.rate).max(0.0),
            _ => 0.0,
        }
    }

    /// IsPlayingAnimationOrBlendingOut.
    pub fn playing_or_blending_out(&self) -> bool {
        self.channels
            .iter()
            .any(|c| c.weight > 0.0 || c.target > 0.0)
    }

    fn advance(&mut self, dt: f32) {
        if self.blend_left > 0.0 {
            let k = (dt / self.blend_left).min(1.0);
            for c in self.channels.iter_mut() {
                c.weight += (c.target - c.weight) * k;
            }
            self.blend_left -= dt;
            if self.blend_left <= 0.0 {
                self.blend_over(0.0);
            }
        }
        let mut ended = false;
        let n = self.channels.len();
        for (i, c) in self.channels.iter_mut().enumerate() {
            c.time += dt * c.rate;
            if c.looping && c.length > 0.0 {
                c.time = c.time.rem_euclid(c.length);
            } else if c.time >= c.length {
                c.time = c.length;
                ended |= i + 1 == n && c.target > 0.0;
            }
        }
        // (A slot whose animation ends goes back to its source: the player has
        // blended it out already unless it never asked to.)
        if ended && self.blend_left <= 0.0 {
            self.stop(0.0);
        }
        self.channels.retain(|c| c.weight > 0.0 || c.target > 0.0);
    }
}

/// TnCustomAnimationPlayer.
#[derive(Clone, Debug, Default)]
pub struct CustomPlayer {
    queue: Vec<Queued>,
    current: Option<AnimParams>,
    current_group: Option<Arc<str>>,
    complete_cb: Option<Callback>,
    failed_cb: Option<Callback>,
    pub suspended: bool,
    pub slot: SlotState,
    /// The anim tree has no slot of this player's name (FindAnimNode found none:
    /// it plays nothing, every animation fails).
    pub no_slot: bool,
}

/// The clips an animator can play: their lengths (the sequences' seconds) and
/// rates (RateScale), the chooser groups' clips, and the notifies in them.
pub trait ClipBook {
    fn length(&self, name: &str) -> Option<f32>;
    fn rate(&self, _name: &str) -> f32 {
        1.0
    }
    /// The group's clip picked (ChooseAnimName), if `name` is a group.
    fn choose(&self, name: &str, roll: f32) -> Option<Arc<str>>;
    fn notifies(&self, _name: &str) -> &[ClipNotify] {
        &[]
    }
    /// Animation.IsAdditive.
    fn is_additive(&self, _name: &str) -> bool {
        false
    }
}

/// The fired callbacks of an update.
pub type Fired = Vec<Callback>;

impl CustomPlayer {
    pub fn enqueue(
        &mut self,
        params: AnimParams,
        complete: Option<Callback>,
        failed: Option<Callback>,
    ) {
        self.queue.push(Queued {
            params,
            complete,
            failed,
        });
    }

    /// The animation playing, by its name (the clip's or its group's).
    pub fn is_playing(&self, name: Option<&str>) -> bool {
        let Some(cur) = &self.current else {
            return false;
        };
        match name {
            None => true,
            Some(n) => *cur.name == *n || self.current_group.as_deref() == Some(n),
        }
    }

    pub fn is_pending(&self, name: &str) -> bool {
        self.queue.iter().any(|q| &*q.params.name == name)
    }

    pub fn current(&self) -> Option<&AnimParams> {
        self.current.as_ref()
    }

    fn cleanup(&mut self) {
        self.current = None;
        self.current_group = None;
        self.complete_cb = None;
        self.failed_cb = None;
        // SetRootMotionMode(RMM_Ignore, RMRM_Ignore).
        self.slot.root_motion = RMM_IGNORE;
        self.slot.root_rotation = 0;
    }

    fn notify_completion(&mut self, fired: &mut Fired) {
        let cb = self.complete_cb;
        self.cleanup();
        fired.extend(cb);
    }

    fn notify_failure(&mut self, fired: &mut Fired) {
        let cb = self.failed_cb;
        self.cleanup();
        fired.extend(cb);
    }

    /// CanPlayAnimation.
    fn can_play(&self, p: &AnimParams) -> bool {
        if self.no_slot {
            return false;
        }
        let playing = self.is_playing(None);
        let cur = self.current.as_ref().map_or(0, |c| c.priority);
        match p.override_mode {
            OVERRIDE_IF_IDLE if playing => false,
            OVERRIDE_IF_HIGHER if playing && cur >= p.priority => false,
            OVERRIDE_IF_HIGHER_OR_EQUAL if playing && cur > p.priority => false,
            _ => true,
        }
    }

    /// PlayAnimation.
    fn play(
        &mut self,
        mut params: AnimParams,
        group: Option<Arc<str>>,
        complete: Option<Callback>,
        failed: Option<Callback>,
        book: &dyn ClipBook,
        fired: &mut Fired,
    ) {
        if params.is_none() {
            // (A stop: the slot blends out, the animation playing fails, and so does
            // this one, FailedNotify: an operator waiting on it, a hit reaction its
            // picker found none for, goes on. Its callbacks dropped, a soldier struck
            // standing still as its navigation's hit reaction was picked, which has
            // none for one standing, stood there waiting for good.)
            self.slot.stop(params.blend_out);
            self.notify_failure(fired);
            fired.extend(failed);
            return;
        }
        if !self.can_play(&params) {
            fired.extend(failed);
            return;
        }
        if self.is_playing(None) {
            self.notify_failure(fired);
        }
        self.complete_cb = complete;
        self.failed_cb = failed;
        let length = book.length(&params.name).unwrap_or(0.0);
        if length <= 0.0 {
            // (No such clip: AnimNodeSlot fails it.)
            self.notify_failure(fired);
            return;
        }
        if params.play_rate <= 0.0 {
            params.play_rate = 1.0;
        }
        let rate = params.play_rate * book.rate(&params.name);
        self.slot.play(
            params.name.clone(),
            length,
            rate,
            params.blend_in,
            params.looping,
        );
        // SetRootMotionMode (not for an additive animation).
        if !book.is_additive(&params.name) {
            self.slot.root_motion = params.root_motion;
            self.slot.root_rotation = params.root_rotation;
        }
        self.slot.mirror_allowed = params.mirroring_allowed;
        let start = params.start_position.clamp(0.0, length);
        if start > 0.0
            && let Some(c) = self.slot.current_mut()
        {
            c.time = start;
        }
        self.current_group = group;
        self.current = Some(params);
    }

    /// StopAnimation: the animation (or any, for None) failed and blended out
    /// over `blend_out` (its own if 0), and taken from the queue.
    pub fn stop(&mut self, blend_out: f32, name: Option<&str>, fired: &mut Fired) {
        if self.is_playing(None) && self.is_playing(name) {
            let out = if blend_out == 0.0 {
                self.current.as_ref().map_or(0.1, |c| c.blend_out)
            } else {
                blend_out
            };
            self.notify_failure(fired);
            self.slot.stop(out);
        }
        if let Some(n) = name
            && let Some(i) = self.queue.iter().rposition(|q| &*q.params.name == n)
        {
            let q = self.queue.remove(i);
            fired.extend(q.failed);
        }
    }

    /// Update: the slot a frame on (and the notifies its animation passed),
    /// completion checked, then the queue played.
    pub fn update(
        &mut self,
        dt: f32,
        book: &dyn ClipBook,
        roll: &mut dyn FnMut() -> f32,
        fired: &mut Fired,
        notifies: &mut Vec<(ClipNotify, f32)>,
    ) {
        let before = self.slot.current().map(|c| (c.anim.clone(), c.time));
        self.slot.advance(dt);
        if let (Some((anim, t0)), Some(c)) = (before, self.slot.current())
            && c.anim == anim
        {
            // (Its notifies from where it was to where it is, at its weight; one
            // played again from its start, from there.)
            let (t0, wrapped) = match (c.looping, c.time < t0) {
                (true, true) => (t0, true),
                (false, true) => (0.0, false),
                _ => (t0, false),
            };
            let end = !c.looping && c.time >= c.length;
            for n in book.notifies(&anim) {
                if passed(n.at(), t0, c.time, wrapped, end) {
                    notifies.push((turned_off_with(n, c), c.weight));
                }
            }
        }
        // CheckForCompletion.
        if let Some(cur) = &self.current {
            if cur.complete_after_blend_out {
                if !cur.looping && self.slot.weight() == 0.0 && self.slot.blend_left == 0.0 {
                    self.notify_completion(fired);
                }
            } else if !cur.looping && self.slot.time_left() - dt < cur.blend_out {
                let out = cur.blend_out;
                self.notify_completion(fired);
                self.slot.stop(out);
            }
        }
        // PlayPendingAnimations.
        if self.queue.is_empty() || self.suspended {
            if !self.suspended {
                self.queue.clear();
            }
            return;
        }
        for q in std::mem::take(&mut self.queue) {
            let mut params = q.params;
            let mut group = None;
            if let Some(chosen) = book.choose(&params.name, roll()) {
                group = Some(params.name.clone());
                params.name = chosen;
            }
            self.play(params, group, q.complete, q.failed, book, fired);
        }
    }
}

/// A notify as channel `c` passes it: an effect turned off with its clip
/// (bAutoDeactivate) is off after the clip's playback length as it plays here
/// (GetAnimPlaybackLength: its play rate's too) and AUTO_OFF_PAD, and no later
/// than the clip's end, a clip that doesn't loop stopping there
/// (OnAnimSequenceStopped).
fn turned_off_with(n: &ClipNotify, c: &Channel) -> ClipNotify {
    let mut n = n.clone();
    if let ClipNotify::Effect {
        auto_off: Some(off),
        ..
    } = &mut n
    {
        let rate = c.rate.max(1e-3);
        *off = c.length / rate + super::clips::AUTO_OFF_PAD;
        if !c.looping {
            *off = off.min(((c.length - c.time) / rate).max(0.0));
        }
    }
    n
}

/// The notifies' source for the anim tree's own nodes (the rest are its players').
pub const TREE: u8 = u8::MAX;

/// Whether a notify at `at` is passed as a sequence goes from `t0` to `t1`
/// (AnimNodeSequence.IssueNotifies: those from where it was up to where it is,
/// `wrapped` round its end and from its start again; at its `end`, the end's too).
pub fn passed(at: f32, t0: f32, t1: f32, wrapped: bool, end: bool) -> bool {
    if wrapped {
        at >= t0 || at < t1
    } else {
        at >= t0 && (at < t1 || (end && at <= t1 && t0 < t1))
    }
}

/// The pawn's animator service (TnRobotAnimator and the additive animator).
#[derive(Clone, Debug, Default)]
pub struct Animator {
    pub players: [CustomPlayer; PLAYERS],
    /// Callbacks fired this update, for the brain to hand on.
    pub fired: Fired,
    /// The notifies its animations passed this update (by player: TREE for the
    /// anim tree's own nodes'), with their nodes' weights, for the pawn and its
    /// controller.
    pub notifies: Vec<(u8, ClipNotify, f32)>,
}

impl Animator {
    /// EnqueueReplicatedAnimation (and EnqueueLocalAnimation): on its mask's
    /// player.
    pub fn enqueue(
        &mut self,
        params: AnimParams,
        complete: Option<Callback>,
        failed: Option<Callback>,
    ) {
        let m = (params.mask as usize).min(PLAYERS - 1);
        self.players[m].enqueue(params, complete, failed);
    }

    /// The additive animator service's EnqueueReplicatedAnimation.
    pub fn enqueue_additive(
        &mut self,
        mut params: AnimParams,
        complete: Option<Callback>,
        failed: Option<Callback>,
    ) {
        params.mask = MASK_ADDITIVE;
        self.players[MASK_ADDITIVE as usize].enqueue(params, complete, failed);
    }

    /// get_IsPlayingAnimation (any of the robot's players).
    pub fn is_playing_any(&self) -> bool {
        self.players[..MASK_ADDITIVE as usize]
            .iter()
            .any(|p| p.is_playing(None))
    }

    /// IsPlayingNamedAnimation.
    pub fn is_playing_named(&self, name: &str) -> bool {
        self.players[..MASK_ADDITIVE as usize]
            .iter()
            .any(|p| p.is_playing(Some(name)))
    }

    pub fn is_playing_masked(&self, mask: u8) -> bool {
        self.players
            .get(mask as usize)
            .is_some_and(|p| p.slot.playing_or_blending_out())
    }

    pub fn is_pending(&self, name: &str) -> bool {
        self.players.iter().any(|p| p.is_pending(name))
    }

    /// StopAnimation (every robot player's).
    pub fn stop(&mut self, blend_out: f32, name: Option<&str>) {
        let mut fired = std::mem::take(&mut self.fired);
        for p in self.players[..MASK_ADDITIVE as usize].iter_mut() {
            p.stop(blend_out, name, &mut fired);
        }
        self.fired = fired;
    }

    /// The additive service's StopAnimation.
    pub fn stop_additive(&mut self, blend_out: f32, name: Option<&str>) {
        let mut fired = std::mem::take(&mut self.fired);
        self.players[MASK_ADDITIVE as usize].stop(blend_out, name, &mut fired);
        self.fired = fired;
    }

    /// The time left of the mask's animation (GetAnimationTimeRemaining).
    pub fn time_remaining(&self, mask: u8) -> f32 {
        let p = &self.players[(mask as usize).min(PLAYERS - 1)];
        if !p.is_playing(None) {
            return 0.0;
        }
        p.slot.time_left()
    }

    pub fn update(&mut self, dt: f32, book: &dyn ClipBook, roll: &mut dyn FnMut() -> f32) {
        let mut fired = std::mem::take(&mut self.fired);
        let mut found = Vec::new();
        for (m, p) in self.players.iter_mut().enumerate() {
            p.update(dt, book, roll, &mut fired, &mut found);
            self.notifies
                .extend(found.drain(..).map(|(n, w)| (m as u8, n, w)));
        }
        self.fired = fired;
    }

    /// The slots the anim tree has (the soldiers': no left arm's or shoulder's).
    pub fn set_slots(&mut self, has: [bool; PLAYERS]) {
        for (p, h) in self.players.iter_mut().zip(has) {
            p.no_slot = !h;
        }
    }

    /// The root motion the full body's animation gives the pawn now, if it gives
    /// any: its mode, rotation mode, clip and time, and its slot's weight.
    pub fn root_motion(&self) -> Option<(u8, u8, Arc<str>, f32, f32)> {
        let p = &self.players[MASK_FULL_BODY as usize];
        let s = &p.slot;
        if p.current.is_none() || s.root_motion == RMM_IGNORE {
            return None;
        }
        let c = s.current()?;
        Some((
            s.root_motion,
            s.root_rotation,
            c.anim.clone(),
            c.time,
            s.weight(),
        ))
    }

    /// IsMirroringAllowed: no animation playing forbids it.
    pub fn mirroring_allowed(&self) -> bool {
        self.players
            .iter()
            .all(|p| p.current.as_ref().is_none_or(|c| c.mirroring_allowed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No clips at all.
    struct NoClips;

    impl ClipBook for NoClips {
        fn length(&self, _name: &str) -> Option<f32> {
            None
        }

        fn choose(&self, _name: &str, _roll: f32) -> Option<Arc<str>> {
            None
        }
    }

    /// Asked to play no animation (a hit reaction picker that found none), a player
    /// calls back the request's failure (TnCustomAnimationPlayer.PlayAnimation's
    /// FailedNotify), so the operator waiting on it goes on.
    #[test]
    fn playing_no_animation_fails_it() {
        let mut p = CustomPlayer::default();
        let done = Callback { owner: 9, kind: 0 };
        let failed = Callback { owner: 9, kind: 1 };
        p.enqueue(AnimParams::default(), Some(done), Some(failed));
        let (mut fired, mut notifies) = (Vec::new(), Vec::new());
        p.update(1.0 / 30.0, &NoClips, &mut || 0.5, &mut fired, &mut notifies);
        assert_eq!(fired, vec![failed]);
    }

    /// An effect turned off with its clip goes off as the clip ends, played at
    /// its rate (a clip that doesn't loop stops there: OnAnimEnd), or for a loop
    /// its playback length and AUTO_OFF_PAD after it starts; one the game leaves
    /// on stays so.
    #[test]
    fn an_effect_goes_off_with_its_clip_as_it_plays() {
        use bevy::math::Vec3;
        let effect = |auto_off| ClipNotify::Effect {
            at: 0.3,
            system: Arc::from("Leaper_Boosters_LowBurn_FX"),
            socket: Arc::from("FX_Booster_Back_R"),
            attached: true,
            min_weight: 0.0,
            offset: Vec3::ZERO,
            turn: Vec3::ZERO,
            params: None,
            auto_off,
        };
        let channel = |time, rate, looping| Channel {
            anim: Arc::from("Melee_FistAttack"),
            time,
            length: 2.0,
            rate,
            looping,
            weight: 1.0,
            target: 1.0,
        };
        let off = |n: ClipNotify| match n {
            ClipNotify::Effect { auto_off, .. } => auto_off,
            _ => None,
        };
        let pad = super::super::clips::AUTO_OFF_PAD;
        // At its start: the whole clip; later on, the rest of it.
        let a = off(turned_off_with(
            &effect(Some(9.0)),
            &channel(0.0, 1.0, false),
        ))
        .unwrap();
        assert!((a - 2.0).abs() < 1e-5, "{a}");
        let b = off(turned_off_with(
            &effect(Some(9.0)),
            &channel(0.5, 1.0, false),
        ))
        .unwrap();
        assert!((b - 1.5).abs() < 1e-5, "{b}");
        // Played twice as fast: half as long.
        let c = off(turned_off_with(
            &effect(Some(9.0)),
            &channel(0.5, 2.0, false),
        ))
        .unwrap();
        assert!((c - 0.75).abs() < 1e-5, "{c}");
        // A loop has no end: its length and the pad.
        let d = off(turned_off_with(
            &effect(Some(9.0)),
            &channel(1.9, 1.0, true),
        ))
        .unwrap();
        assert!((d - (2.0 + pad)).abs() < 1e-5, "{d}");
        assert_eq!(
            off(turned_off_with(&effect(None), &channel(0.0, 1.0, false))),
            None
        );
    }
}
