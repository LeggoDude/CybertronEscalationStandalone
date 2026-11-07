//! Online, the host's AI plays every PC's enemies, as Fall of Cybertron's server
//! runs its AI and sends its pawns to the clients: each frame the host takes down
//! each of its enemies (net.rs's EnemyState: its pawn's place, velocity and
//! rotation, its physics, its pose, its gun, its forms, its plane's turn and its
//! laser sight) and what they did (EnemyEvent: their rounds' traces, their
//! projectiles and grenades, their blows to the players and to the barriers, the
//! players their scanners caught, their sounds, their dialog and the effects they
//! played), and its enemy packets carry them to the clients twenty times a second.
//!
//! A client's enemies think nothing. Each one's pawn is where the host's states put
//! it, a little behind the host's clock (SHOW_DELAY: each moment then falls between
//! two states come in, its place, turns, velocity, pose and change of form eased
//! between them), and its models are placed and posed from it as the host's are,
//! its sockets read from them; the loops its state holds (its gun's while it fires
//! or charges, its scanner's, its engines') play here. What they did plays out here
//! as it happened there, at the same delay: their tracers, impacts and sounds, their
//! effects on their sockets, their dialog, their projectiles and grenades (each PC
//! flies its own copies, their blasts hurting its own player), and their blows to
//! the players (the host's traces decide them; each player's PC takes his own).
//!
//! What the players do to the enemies goes as it did: each PC plays out every
//! player's shots (the enemy each struck by its number: JazzEvent::Shot), rounds and
//! blows on its copies, the host's AI taking up the blows it took; an enemy dies
//! where it first runs out of health, its death going to the others (waves.rs's
//! EnemyDeath).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use bevy::prelude::*;

use super::clips::Clips;
use super::pawn::{Pawn, Physics};
use super::tree::{LAYERS, Pose, PoseClip};
use super::{Ai, AiOutputs, AiWorld, NearGrenades, audio, guard, shared, ue};
use crate::enemies::{Death, Enemy, Foe};
use crate::equipment::Barrier;
use crate::net::{
    EnemyEvent, EnemyState, FormsState, LaserState, Net, effect_flag, enemy_flag, hurt_flag,
    shot_flag,
};
use crate::waves::{WaveFoe, Waves};

/// How far behind the host's clock a client shows its enemies: two of the host's
/// enemy packets' intervals (net.rs's ENEMIES_EVERY), so that each moment shown
/// falls between two states come in, one packet lost or late.
pub const SHOW_DELAY: f32 = 0.1;
/// The furthest a client carries an enemy on along its velocity past its newest
/// state (its packets late), and how far back each enemy's states are kept.
const EXTRAPOLATE_MAX: f32 = 0.25;
const KEEP_SECS: f32 = 1.0;
/// An enemy no state has come for this long is forgotten (it is dead or gone).
const FORGET_SECS: f32 = 3.0;
/// A clip whose time goes back between two states by less than this much short of
/// a turn of it has gone round (a loop's end to its start: eased on across it);
/// else it started again.
const WRAP_REACH: f32 = 0.25;
/// A player a scanner holds is caught every frame: the clients hear of it at most
/// this often (its jam lasts far longer).
const SCANNED_EVERY: f32 = 0.05;

/// This PC plays its enemies from the host's AI (it has joined an online match).
pub fn following(net: &Net) -> bool {
    !crate::waves::hosting(net)
}

/// A rotation (Unreal's units, any turn) as the network carries it (one turn
/// in 65536), and a pitch or roll back from it (about none).
fn rot16(r: f32) -> u16 {
    r.rem_euclid(65536.0).round() as u32 as u16
}

fn signed(r: u16) -> f32 {
    r as i16 as f32
}

/// UE3's EPhysics by its number.
fn physics_of(n: u8) -> Physics {
    match n {
        1 => Physics::Walking,
        2 => Physics::Falling,
        4 => Physics::Flying,
        7 => Physics::Interpolating,
        8 => Physics::Spider,
        10 => Physics::RigidBody,
        _ => Physics::None,
    }
}

// ---------------------------------------------------------------------------
// The host's enemies, taken down
// ---------------------------------------------------------------------------

/// One of the host's enemies as its pawn is this frame: numbered `id`, `scale` what
/// it makes of blows (its Enemy's AI damage scale: its damage multiplier, its
/// drones' shield).
pub fn state_of(ai: &Ai, id: u16, scale: f32) -> EnemyState {
    let p = &ai.pawn;
    let w = &p.weapon;
    let mut flags = 0;
    for (on, bit) in [
        (w.equipped, enemy_flag::EQUIPPED),
        (w.out, enemy_flag::OUT),
        (w.effects_firing, enemy_flag::FIRING),
        (w.charging, enemy_flag::CHARGING),
        (
            p.guard.as_ref().is_some_and(|g| g.scanner.attached),
            enemy_flag::SCANNER,
        ),
        (
            p.plane.as_ref().is_some_and(|f| f.boosting),
            enemy_flag::BOOSTING,
        ),
        (p.laser.active, enemy_flag::LASER),
    ] {
        if on {
            flags |= bit;
        }
    }
    let gun = ai
        .brain
        .blueprint
        .guns
        .iter()
        .position(|g| Arc::ptr_eq(g, &w.data))
        .or_else(|| {
            ai.brain
                .blueprint
                .guns
                .iter()
                .position(|g| g.class == w.data.class)
        })
        .map_or(u8::MAX, |i| i.min(254) as u8);
    EnemyState {
        id,
        loc: p.loc,
        vel: p.vel,
        rot: [rot16(p.yaw), rot16(p.pitch), rot16(p.roll)],
        physics: p.physics as u8,
        flags,
        gun,
        scale,
        pose: sent_pose(&p.pose),
        forms: p.forms.as_ref().map(|f| FormsState {
            current: f.current,
            target: f.target,
            hidden: f.hidden,
            fx_allowed: f.fx_allowed,
            change: f.change.as_ref().map(|c| (c.to, c.left, c.duration)),
            other: f.change.is_some().then(|| sent_pose(&f.other.pose)),
        }),
        plane: p.plane.as_ref().map(|f| {
            [
                rot16(f.rotation.x),
                rot16(f.rotation.y),
                rot16(f.rotation.z),
            ]
        }),
        laser: p.laser.active.then_some(LaserState {
            target: p.laser.target,
            smoothed: p.laser.smoothed,
            spread: p.laser.spread,
            roll: p.laser.roll,
        }),
    }
}

/// A clip weighing less than this in a pose isn't sent (nothing to see of it).
const LEAST_WEIGHT: f32 = 0.002;

/// A pose as it is sent: the clips that weigh anything.
fn sent_pose(p: &Pose) -> Pose {
    Pose {
        clips: p
            .clips
            .iter()
            .filter(|c| c.weight >= LEAST_WEIGHT)
            .copied()
            .collect(),
        shares: p.shares,
        mirror: p.mirror,
        aim_offset: p.aim_offset,
    }
}

/// A gun's effects (GUN_FX's) by their place in the list (255 the default's).
fn gun_index(class: &str) -> u8 {
    super::GUN_FX
        .iter()
        .position(|(c, _)| *c == class)
        .map_or(u8::MAX, |i| i as u8)
}

/// The host's enemies for the clients, after its AI's frame (before the world
/// takes up what they did): each one's state, and what they did as events (those
/// that matter sent again for a while; their sounds, tracers and the flashes held
/// on their muzzles once).
#[allow(clippy::type_complexity)]
pub fn capture(
    mut net: ResMut<Net>,
    outputs: Res<AiOutputs>,
    ais: Query<(&Ai, &WaveFoe, Option<&Enemy>)>,
    numbered: Query<(Entity, &WaveFoe)>,
    players: Query<(Entity, &crate::PlayerSlot), With<crate::Player>>,
    barriers: Query<&Barrier>,
    mut scanned_at: Local<HashMap<u8, f32>>,
) {
    if !net.is_host() || !net.in_match() {
        return;
    }
    // (Every enemy by its number, the dying too: their cries, their deaths' effects.)
    let ids: HashMap<Entity, u16> = numbered.iter().map(|(e, w)| (e, w.id as u16)).collect();
    let states = ais
        .iter()
        .filter(|(ai, _, enemy)| !ai.pawn.dying && enemy.is_some_and(|e| e.alive()))
        .map(|(ai, w, enemy)| {
            let scale = enemy.map_or(1.0, |e| e.ai_damage_scale);
            state_of(ai, w.id as u16, scale)
        })
        .collect();
    net.set_enemy_states(states);
    let id = |e: Entity| ids.get(&e).copied().unwrap_or(0);
    let slot = |e: Entity| players.get(e).map_or(u8::MAX, |(_, s)| s.0.min(254) as u8);
    for s in &outputs.shots {
        let mut flags = 0;
        for (on, bit) in [
            (s.tracer, shot_flag::TRACER),
            (s.first, shot_flag::FIRST),
            (s.traced, shot_flag::TRACED),
        ] {
            if on {
                flags |= bit;
            }
        }
        net.push_enemy_event(
            EnemyEvent::Shot {
                by: id(s.shooter),
                from: s.from,
                to: s.to,
                gun: gun_index(&s.gun),
                flags,
                normal: s.hit_normal,
            },
            false,
        );
    }
    for d in &outputs.player_damage {
        let slot = slot(d.player);
        if slot == u8::MAX {
            continue;
        }
        let mut flags = 0;
        if d.melee {
            flags |= hurt_flag::MELEE;
        }
        if d.jam {
            flags |= hurt_flag::JAM;
        }
        if d.shot.is_some_and(|(_, _, gib)| gib) {
            flags |= hurt_flag::GIB;
        }
        net.push_enemy_event(
            EnemyEvent::Hurt {
                slot,
                by: id(d.by),
                amount: d.amount,
                at: d.at,
                dir: d.dir,
                flags,
                momentum: d.momentum,
                shot: d.shot.map(|(death, momentum, _)| (death.index(), momentum)),
                damage: crate::killfeed::damage_index(d.damage),
            },
            true,
        );
    }
    for &(b, amount) in &outputs.barrier_damage {
        if let Ok(barrier) = barriers.get(b) {
            net.push_enemy_event(
                EnemyEvent::Barrier {
                    at: barrier.foot(),
                    amount,
                },
                true,
            );
        }
    }
    for t in &outputs.throws {
        net.push_enemy_event(
            EnemyEvent::Throw {
                by: id(t.by),
                class: t.class.clone(),
                from: t.from,
                velocity: t.velocity,
            },
            true,
        );
    }
    for l in &outputs.launches {
        net.push_enemy_event(
            EnemyEvent::Launch {
                by: id(l.by),
                from: l.from,
                dir: l.dir,
                class: l.data.class.clone(),
                target: l.target.map_or(u8::MAX, slot),
            },
            true,
        );
    }
    for &(by, cue, at) in &outputs.sounds {
        net.push_enemy_event(
            EnemyEvent::Sound {
                by: id(by),
                cue: cue.to_string(),
                at,
            },
            false,
        );
    }
    for say in &outputs.says {
        let event = match say {
            audio::Say::Line {
                owner,
                cut,
                cue,
                at,
            } => EnemyEvent::Line {
                owner: *owner,
                cut: *cut,
                cue: cue.to_string(),
                at: *at,
            },
            audio::Say::Hush { owner } => EnemyEvent::Hush { owner: *owner },
        };
        net.push_enemy_event(event, true);
    }
    let now = net.clock();
    for s in &outputs.scanned {
        let slot = slot(s.player);
        if slot == u8::MAX
            || scanned_at
                .get(&slot)
                .is_some_and(|&t| now - t < SCANNED_EVERY && now >= t)
        {
            continue;
        }
        scanned_at.insert(slot, now);
        net.push_enemy_event(EnemyEvent::Scanned { slot, by: id(s.by) }, false);
    }
    for e in &outputs.effects {
        let mut flags = 0;
        if e.attached {
            flags |= effect_flag::ATTACHED;
        }
        if e.only_once {
            flags |= effect_flag::ONLY_ONCE;
        }
        if e.restart {
            flags |= effect_flag::RESTART;
        }
        if e.auto_off.is_some() {
            flags |= effect_flag::AUTO_OFF;
        }
        // (A flash held on a gun's muzzle comes with each round: sent once, as
        // its rounds' tracers are; the rest again for a while.)
        let resent = !super::GUN_FX.iter().any(|(_, f)| f.muzzle == &*e.system);
        net.push_enemy_event(
            EnemyEvent::Effect {
                by: id(e.by),
                system: e.system.to_string(),
                socket: e.socket.to_string(),
                at: e.at,
                axes: e.axes,
                offset: e.offset,
                turn: e.turn,
                flags,
                params: e
                    .params
                    .as_ref()
                    .map(|p| p.iter().map(|n| n.to_string()).collect()),
                off: e.auto_off.unwrap_or(0.0),
            },
            resent,
        );
    }
    for (by, system) in &outputs.effects_stopped {
        net.push_enemy_event(
            EnemyEvent::EffectStop {
                by: id(*by),
                system: system.to_string(),
            },
            true,
        );
    }
}

// ---------------------------------------------------------------------------
// A client's copies of them
// ---------------------------------------------------------------------------

/// A client's copies of the host's enemies: each one's states as they came in
/// (each with the host's time it is from, oldest first) and when the last came,
/// and their events still to play out (each at the host's time it happened).
#[derive(Resource, Default)]
pub struct Puppets {
    states: HashMap<u16, (f32, VecDeque<(f32, EnemyState)>)>,
    events: Vec<(f32, EnemyEvent)>,
}

impl Puppets {
    /// What came in from the host, taken up: each state in its enemy's list in
    /// time order, each event in the queue.
    pub fn take_in(&mut self, states: Vec<(f32, EnemyState)>, events: Vec<(f32, EnemyEvent)>) {
        for (at, s) in states {
            let (last, list) = self
                .states
                .entry(s.id)
                .or_insert_with(|| (at, VecDeque::new()));
            *last = last.max(at);
            let i = list.partition_point(|(t, _)| *t <= at);
            // (The same moment twice: the newer copy.)
            if i > 0 && list[i - 1].0 == at {
                list[i - 1].1 = s;
            } else {
                list.insert(i, (at, s));
            }
        }
        self.events.extend(events);
    }

    /// The events due by the host's time `t`, in the order they happened.
    pub fn due(&mut self, t: f32) -> Vec<EnemyEvent> {
        self.events.sort_by(|a, b| a.0.total_cmp(&b.0));
        let n = self.events.partition_point(|(at, _)| *at <= t);
        self.events.drain(..n).map(|(_, e)| e).collect()
    }

    /// Those older than needed for the host's time `t` let go (but the two last of
    /// each), and the enemies no state has come for in a while.
    pub fn prune(&mut self, t: f32) {
        self.states.retain(|_, (last, list)| {
            while list.len() > 2 && list[1].0 < t - KEEP_SECS {
                list.pop_front();
            }
            t - *last < FORGET_SECS
        });
    }

    /// The states either side of the host's time `t` for enemy `id`, and how far
    /// from the first to the second it is (0 to 1; past the last, its seconds past
    /// it as a negative number, to carry it on by).
    pub fn bracket(&self, id: u16, t: f32) -> Option<(&EnemyState, &EnemyState, f32)> {
        let (_, list) = self.states.get(&id)?;
        let (first, last) = (list.front()?, list.back()?);
        if t <= first.0 {
            return Some((&first.1, &first.1, 0.0));
        }
        if t >= last.0 {
            return Some((&last.1, &last.1, -(t - last.0)));
        }
        let i = list.partition_point(|(at, _)| *at <= t);
        let (a, b) = (&list[i - 1], &list[i]);
        let k = ((t - a.0) / (b.0 - a.0).max(1e-4)).clamp(0.0, 1.0);
        Some((&a.1, &b.1, k))
    }
}

fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}

/// A rotation `k` of the way from `a` to `b`, the short way round.
fn lerp_rot(a: u16, b: u16, k: f32) -> u16 {
    let d = b.wrapping_sub(a) as i16 as f32;
    rot16(a as f32 + d * k)
}

/// A pose `k` of the way from `a` to `b` (each clip's weight eased from its own in
/// one to its own in the other, none where it isn't; its time eased on, across its
/// end to its start for a loop gone round: its length `secs` of a clip), the
/// layers' shares, the mirror and the aim offset eased.
pub fn blend_pose(a: &Pose, b: &Pose, k: f32, secs: &dyn Fn(u16) -> f32) -> Pose {
    let mut clips = Vec::with_capacity(b.clips.len() + 4);
    let same = |x: &PoseClip, y: &PoseClip| x.layer == y.layer && x.clip == y.clip;
    for cb in &b.clips {
        match a.clips.iter().find(|ca| same(ca, cb)) {
            Some(ca) => {
                let time = if cb.time >= ca.time {
                    lerp(ca.time, cb.time, k)
                } else {
                    let len = secs(cb.clip);
                    let on = cb.time + len - ca.time;
                    if len > 0.0 && on <= WRAP_REACH {
                        (ca.time + on * k).rem_euclid(len)
                    } else {
                        cb.time
                    }
                };
                clips.push(PoseClip {
                    weight: lerp(ca.weight, cb.weight, k),
                    time,
                    ..*cb
                });
            }
            None => clips.push(PoseClip {
                weight: cb.weight * k,
                ..*cb
            }),
        }
    }
    for ca in &a.clips {
        if !b.clips.iter().any(|cb| same(ca, cb)) {
            clips.push(PoseClip {
                weight: ca.weight * (1.0 - k),
                ..*ca
            });
        }
    }
    clips.retain(|c| c.weight > 1e-5);
    let mut shares = [0.0; LAYERS];
    for (i, s) in shares.iter_mut().enumerate() {
        *s = lerp(a.shares[i], b.shares[i], k);
    }
    let aim_offset = match (a.aim_offset, b.aim_offset) {
        (Some((pa, aa)), Some((pb, ab))) if pa == pb => Some((pb, aa.lerp(ab, k))),
        _ if k < 0.5 => a.aim_offset,
        _ => b.aim_offset,
    };
    Pose {
        clips,
        shares,
        mirror: lerp(a.mirror, b.mirror, k),
        aim_offset,
    }
}

/// The state `k` of the way from `a` to `b` (past `b`, `-k` seconds on: carried on
/// along its velocity, up to EXTRAPOLATE_MAX): its place, velocity, turns, laser
/// sight and change of form eased, its pose eased within a form (`secs`, `other_secs`
/// its clips' lengths, its other form's), the rest as the nearer has it.
pub fn blend(
    a: &EnemyState,
    b: &EnemyState,
    k: f32,
    secs: &dyn Fn(u16) -> f32,
    other_secs: &dyn Fn(u16) -> f32,
) -> EnemyState {
    if k < 0.0 {
        let mut s = b.clone();
        s.loc += s.vel * (-k).min(EXTRAPOLATE_MAX);
        return s;
    }
    let mut s = if k < 0.5 { a.clone() } else { b.clone() };
    s.loc = a.loc.lerp(b.loc, k);
    s.vel = a.vel.lerp(b.vel, k);
    s.rot = [0, 1, 2].map(|i| lerp_rot(a.rot[i], b.rot[i], k));
    let form = |s: &EnemyState| s.forms.as_ref().map(|f| f.current);
    if form(a) == form(b) {
        s.pose = blend_pose(&a.pose, &b.pose, k, secs);
        if let (Some(fa), Some(fb), Some(f)) = (&a.forms, &b.forms, s.forms.as_mut()) {
            if let (Some(ca), Some(cb)) = (fa.change, fb.change)
                && ca.0 == cb.0
            {
                f.change = Some((cb.0, lerp(ca.1, cb.1, k), cb.2));
            }
            if let (Some(oa), Some(ob)) = (&fa.other, &fb.other) {
                f.other = Some(blend_pose(oa, ob, k, other_secs));
            }
        }
    }
    if let (Some(pa), Some(pb)) = (a.plane, b.plane) {
        s.plane = Some([0, 1, 2].map(|i| lerp_rot(pa[i], pb[i], k)));
    }
    if let (Some(la), Some(lb), Some(l)) = (a.laser, b.laser, s.laser.as_mut()) {
        l.target = la.target.lerp(lb.target, k);
        l.smoothed = la.smoothed.lerp(lb.smoothed, k);
        l.spread = lerp(la.spread, lb.spread, k);
        let d = (lb.roll - la.roll + 180.0).rem_euclid(360.0) - 180.0;
        l.roll = la.roll + d * k;
    }
    s
}

/// A client's copy takes the form `to` (as the host's forms.rs's BeginTransformation
/// swaps them: its clips the form's from now on, its other form's the other's) with
/// its cylinder.
fn take_form(p: &mut Pawn, clips: &mut Arc<Clips>, to: u8) {
    let Some(f) = p.forms.as_mut() else {
        return;
    };
    if f.current == to {
        return;
    }
    if let Some(theirs) = f.other.clips.take() {
        f.other.clips = Some(std::mem::replace(clips, theirs));
    }
    f.current = to;
    let caps = f.caps[to as usize];
    if caps.radius > 0.0 {
        p.radius = caps.radius;
        p.half_height = caps.half_height;
    }
}

/// A clip's length in a set (none for one it hasn't).
fn clip_secs(clips: &Clips, clip: u16) -> f32 {
    clips.clips.get(clip as usize).map_or(0.0, |c| c.secs)
}

/// Puts a state on a client's copy of the enemy, its pawn as the host's was: its
/// place, turns and velocity, its physics, its forms (the form it is in taken, with
/// its clips: `take_form`), its pose, its gun, its scanner, its plane's turn and its
/// laser sight.
pub fn apply(ai: &mut Ai, s: &EnemyState) {
    let Ai {
        brain,
        pawn: p,
        clips,
    } = ai;
    p.loc = s.loc;
    p.vel = s.vel;
    p.yaw = s.rot[0] as f32;
    p.pitch = signed(s.rot[1]);
    p.roll = signed(s.rot[2]);
    p.physics = physics_of(s.physics);
    if let Some(fs) = s.forms.as_ref() {
        take_form(p, clips, fs.current);
        if let Some(f) = p.forms.as_mut() {
            f.target = fs.target;
            f.hidden = fs.hidden;
            f.fx_allowed = fs.fx_allowed;
            f.change =
                fs.change
                    .map(|(to, left, duration)| super::forms::Change { to, left, duration });
            if let Some(other) = fs.other.as_ref() {
                f.other.pose = other.clone();
            }
        }
    }
    p.pose = s.pose.clone();
    let w = &mut p.weapon;
    if let Some(g) = brain.blueprint.guns.get(s.gun as usize)
        && !Arc::ptr_eq(&w.data, g)
    {
        w.data = g.clone();
    }
    w.equipped = s.flags & enemy_flag::EQUIPPED != 0;
    w.out = s.flags & enemy_flag::OUT != 0;
    w.effects_firing = s.flags & enemy_flag::FIRING != 0;
    w.charging = s.flags & enemy_flag::CHARGING != 0;
    if let Some(g) = p.guard.as_mut() {
        g.scanner.attached = s.flags & enemy_flag::SCANNER != 0;
    }
    if let (Some(f), Some(r)) = (p.plane.as_mut(), s.plane) {
        f.rotation = Vec3::new(signed(r[0]), r[1] as f32, signed(r[2]));
        f.boosting = s.flags & enemy_flag::BOOSTING != 0;
    }
    match s.laser {
        Some(l) => {
            p.laser.active = true;
            p.laser.target = l.target;
            p.laser.smoothed = l.smoothed;
            p.laser.spread = l.spread;
            p.laser.roll = l.roll;
        }
        None => p.laser.active = false,
    }
}

/// The loops a client's copy holds this frame, as its state says (its pawn's frame
/// on the host would: its scanner's, its drone's engine, its jet's engine, its
/// gun's while it fires and while it charges), its gun's muzzle where its model's
/// last pose put it.
fn hold_loops(
    p: &mut Pawn,
    sounds: &audio::AiSounds,
    shared: &mut shared::Shared,
    loops: &mut Vec<(Entity, &'static str, Vec3, f32)>,
) {
    p.muzzle = match p.muzzle_local {
        Some(at) => p.feet() + ue::rotate_yaw(at, p.yaw),
        None => {
            p.loc
                + Vec3::Z * p.half_height * 0.45
                + super::weapon::gun_direction(p) * p.radius * 1.1
        }
    };
    if p.dying {
        return;
    }
    let entity = p.entity;
    if let Some(g) = p.guard.as_ref()
        && g.scanner.attached
        && let Some((cue, _)) = p.voice.find_cue(sounds, guard::SCANNER_LOOP_EVENT)
    {
        let at = p
            .socket_frame(guard::SCANNER_SOCKET)
            .map_or(p.loc, |(at, _)| at);
        loops.push((entity, cue, at, guard::SCANNER_LOOP_FADE));
    }
    if p.plane.is_some()
        && let Some((cue, _)) = p.voice.find_cue(sounds, super::drones::ENGINE_EVENT)
    {
        loops.push((entity, cue, p.loc, super::drones::ENGINE_FADE));
    }
    // (Its jet's engine's loops; its one-shots come from the host's.)
    super::jet::engine_sounds(p, sounds, shared, false);
    loops.append(&mut shared.loops);
    shared.sounds.clear();
    let fx = super::gun_fx(&p.weapon.data.class);
    if p.weapon.charging && !fx.charge_loop.is_empty() {
        loops.push((entity, fx.charge_loop, p.muzzle, fx.charge_loop_fade));
    }
    if p.weapon.effects_firing && !fx.shoot_loop.is_empty() {
        loops.push((entity, fx.shoot_loop, p.muzzle, super::LOOP_FADE));
    }
}

/// A name come from the host as the sounds want it (each one kept once, for good:
/// there are only so many cues).
fn intern(s: &str) -> &'static str {
    static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut names = NAMES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(&n) = names.get(s) {
        return n;
    }
    let n: &'static str = Box::leak(s.to_string().into_boxed_str());
    names.insert(n);
    n
}

/// A projectile's data by its class: one its animations spawn, or one of its
/// shooter's guns' (its blueprint's: `shooter`), or any type's guns' (its shooter
/// gone).
fn projectile_data(
    world: &AiWorld,
    shooter: Option<&super::blueprint::Blueprint>,
    class: &str,
) -> Option<Arc<super::weapon::ProjectileData>> {
    if let Some(d) = world.notify_projectiles.get(class) {
        return Some(d.clone());
    }
    let of_guns = |guns: &[Arc<super::weapon::GunData>]| {
        guns.iter()
            .filter_map(|g| g.projectile.as_ref())
            .find(|d| d.class == class)
            .cloned()
    };
    shooter
        .and_then(|bp| of_guns(&bp.guns))
        .or_else(|| world.blueprints.values().find_map(|bp| of_guns(&bp.guns)))
}

/// An event from the host, for the world here to play out as the AI's own
/// (AiOutputs): each enemy its copy here (none: a shot's or a sound's still plays,
/// as nobody's), each player his Jazz here, a barrier the one standing there.
#[allow(clippy::too_many_arguments)]
fn play_event(
    event: EnemyEvent,
    enemy: &dyn Fn(u16) -> Option<Entity>,
    player: &dyn Fn(u8) -> Option<Entity>,
    world: &AiWorld,
    ais: &dyn Fn(Entity) -> Option<Arc<super::blueprint::Blueprint>>,
    barriers: &[(Entity, Vec3)],
    outputs: &mut AiOutputs,
) {
    let anyone = |id: u16| enemy(id).unwrap_or(Entity::PLACEHOLDER);
    match event {
        EnemyEvent::Shot {
            by,
            from,
            to,
            gun,
            flags,
            normal,
        } => outputs.shots.push(shared::Shot {
            shooter: anyone(by),
            from,
            to,
            hit_player: None,
            hit_normal: normal,
            tracer: flags & shot_flag::TRACER != 0,
            gun: super::GUN_FX
                .get(gun as usize)
                .map_or(String::new(), |(c, _)| c.to_string()),
            first: flags & shot_flag::FIRST != 0,
            traced: flags & shot_flag::TRACED != 0,
            damage: 0.0,
            damage_type: "",
        }),
        EnemyEvent::Launch {
            by,
            from,
            dir,
            class,
            target,
        } => {
            let blueprint = enemy(by).and_then(ais);
            if let Some(data) = projectile_data(world, blueprint.as_deref(), &class) {
                outputs.launches.push(shared::Launch {
                    by: anyone(by),
                    from,
                    dir,
                    data,
                    target: player(target),
                });
            }
        }
        EnemyEvent::Throw {
            by,
            class,
            from,
            velocity,
        } => outputs.throws.push(shared::Throw {
            by: anyone(by),
            class,
            from,
            velocity,
        }),
        EnemyEvent::Hurt {
            slot,
            by,
            amount,
            at,
            dir,
            flags,
            momentum,
            shot,
            damage,
        } => {
            if let Some(p) = player(slot) {
                outputs.player_damage.push(shared::PlayerDamage {
                    player: p,
                    by: anyone(by),
                    amount,
                    at,
                    dir,
                    melee: flags & hurt_flag::MELEE != 0,
                    momentum,
                    jam: flags & hurt_flag::JAM != 0,
                    shot: shot.map(|(death, momentum)| {
                        (
                            Death::from_index(death),
                            momentum,
                            flags & hurt_flag::GIB != 0,
                        )
                    }),
                    damage: crate::killfeed::damage_of_index(damage),
                });
            }
        }
        EnemyEvent::Barrier { at, amount } => {
            if let Some(&(b, _)) = barriers
                .iter()
                .filter(|(_, foot)| foot.distance(at) < 1.0)
                .min_by(|x, y| x.1.distance(at).total_cmp(&y.1.distance(at)))
            {
                outputs.barrier_damage.push((b, amount));
            }
        }
        EnemyEvent::Scanned { slot, by } => {
            if let Some(p) = player(slot) {
                outputs.scanned.push(guard::Scanned {
                    player: p,
                    by: anyone(by),
                });
            }
        }
        EnemyEvent::Sound { by, cue, at } => outputs.sounds.push((anyone(by), intern(&cue), at)),
        EnemyEvent::Line {
            owner,
            cut,
            cue,
            at,
        } => outputs.says.push(audio::Say::Line {
            owner,
            cut,
            cue: intern(&cue),
            at,
        }),
        EnemyEvent::Hush { owner } => outputs.says.push(audio::Say::Hush { owner }),
        EnemyEvent::Effect {
            by,
            system,
            socket,
            at,
            axes,
            offset,
            turn,
            flags,
            params,
            off,
        } => outputs.effects.push(shared::Effect {
            by: anyone(by),
            system: Arc::from(system),
            socket: Arc::from(socket),
            at,
            axes,
            offset,
            turn,
            attached: flags & effect_flag::ATTACHED != 0,
            only_once: flags & effect_flag::ONLY_ONCE != 0,
            restart: flags & effect_flag::RESTART != 0,
            params: params.map(|names| {
                names
                    .into_iter()
                    .map(Arc::from)
                    .collect::<Vec<Arc<str>>>()
                    .into()
            }),
            auto_off: (flags & effect_flag::AUTO_OFF != 0).then_some(off),
        }),
        EnemyEvent::EffectStop { by, system } => outputs
            .effects_stopped
            .push((anyone(by), Arc::from(system))),
    }
}

/// A client's enemies' frame, in place of the AI's (think): each one's pawn put
/// where the host's states have it now (SHOW_DELAY behind its clock), its models
/// placed and its Enemy told what blows do to it, the loops it holds; a dead one's
/// pawn dying (its AI gone, as the host's is); and what the host's did, due by
/// now, for the world to play out.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn follow_host(
    mut commands: Commands,
    mut net: ResMut<Net>,
    mut puppets: ResMut<Puppets>,
    mut world: ResMut<AiWorld>,
    mut outputs: ResMut<AiOutputs>,
    mut near_grenades: ResMut<NearGrenades>,
    waves: Res<Waves>,
    mut ais: Query<(
        Entity,
        &mut Ai,
        &mut Transform,
        &mut Foe,
        Option<&mut Enemy>,
        Option<&mut crate::enemies::Forms>,
        Option<&WaveFoe>,
        Has<crate::waves::Withdrawn>,
    )>,
    players: Query<(Entity, &crate::PlayerSlot), With<crate::Player>>,
    barriers: Query<(Entity, &Barrier)>,
) {
    let states = net.take_enemy_states();
    let events = net.take_enemy_events();
    puppets.take_in(states, events);
    // (The players' grenades the AI would hear of, and its own kills: the host's.)
    near_grenades.0.clear();
    world.shared.kills.clear();
    let shown = net.host_clock().map(|t| t - SHOW_DELAY);
    let world = &mut *world;
    let sounds = world.sounds.clone();
    for (entity, mut ai, mut transform, mut foe, enemy, mut model_forms, wave, withdrawn) in
        &mut ais
    {
        let ai = &mut *ai;
        // (Taken out of play by the host's waves, not killed: out of its squad, and
        // gone.)
        if withdrawn {
            let now = world.shared.now;
            world
                .shared
                .squad
                .remove_member(entity, now, &mut Vec::new());
            commands.entity(entity).despawn();
            continue;
        }
        let alive = enemy.as_ref().is_some_and(|e| e.alive());
        if !alive {
            // (It died here, or the host's did: it dies as it is.)
            if !ai.pawn.dying {
                ai.pawn.dying = true;
                super::forms::died(&mut ai.pawn);
                if let Some(mf) = model_forms.as_deref_mut() {
                    super::show_forms(&ai.pawn, mf);
                }
                // (Out of the squad its AI joined here as it came in.)
                let now = world.shared.now;
                let mut writes = Vec::new();
                world.shared.squad.remove_member(entity, now, &mut writes);
            }
            commands.entity(entity).remove::<Ai>();
            continue;
        }
        let mut enemy = enemy.expect("alive");
        // (The blows it took here: the host's AI takes them up there.)
        enemy.hits.clear();
        if let (Some(t), Some(w)) = (shown, wave) {
            let id = w.id as u16;
            if let Some((a, b, k)) = puppets.bracket(id, t) {
                let near = if k >= 0.5 { b } else { a };
                if let Some(fs) = near.forms.as_ref() {
                    let Ai { pawn, clips, .. } = ai;
                    take_form(pawn, clips, fs.current);
                }
                let main = ai.clips.clone();
                let other = ai.pawn.forms.as_ref().and_then(|f| f.other.clips.clone());
                let s = blend(a, b, k, &|c| clip_secs(&main, c), &|c| {
                    other.as_ref().map_or(0.0, |o| clip_secs(o, c))
                });
                apply(ai, &s);
                if enemy.ai_damage_scale != s.scale {
                    enemy.ai_damage_scale = s.scale;
                }
            }
            // (A drone's Titan: its beam's other end.)
            if ai.brain.sensors.defend_actor.is_none()
                && let Some(owner) = w.owner.and_then(|o| waves.entity_of(o))
            {
                ai.brain.sensors.defend_actor = Some(owner);
            }
        }
        super::place_models(
            &ai.pawn,
            &mut transform,
            &mut foe,
            model_forms.as_deref_mut(),
        );
        let scale = enemy.ai_damage_scale;
        super::sync_blows(&mut enemy, &ai.pawn, 1.0);
        enemy.ai_damage_scale = scale;
        hold_loops(&mut ai.pawn, &sounds, &mut world.shared, &mut outputs.loops);
    }
    // What the host's did, due by now.
    let Some(t) = shown else {
        return;
    };
    puppets.prune(t);
    let due = puppets.due(t);
    if due.is_empty() {
        return;
    }
    let enemy = |id: u16| waves.entity_of(id as u32);
    let player = |slot: u8| {
        players
            .iter()
            .find(|(_, s)| s.0 == slot as usize)
            .map(|(e, _)| e)
    };
    let blueprints: HashMap<Entity, Arc<super::blueprint::Blueprint>> = ais
        .iter()
        .map(|(e, ai, ..)| (e, ai.brain.blueprint.clone()))
        .collect();
    let blueprint = |e: Entity| blueprints.get(&e).cloned();
    let standing: Vec<(Entity, Vec3)> = barriers.iter().map(|(e, b)| (e, b.foot())).collect();
    for event in due {
        play_event(
            event,
            &enemy,
            &player,
            world,
            &blueprint,
            &standing,
            &mut outputs,
        );
    }
}

/// A new match: no states, no events.
pub fn reset(mut puppets: ResMut<Puppets>) {
    *puppets = Puppets::default();
}

#[cfg(test)]
mod tests {
    use super::super::world::{PlayerView, WorldView};
    use super::super::{
        ALERT_SECS, Body, Slot, ai_view, clips, data::AiData, forms, forms_of, new_ai, pawn,
        run_frame,
    };
    use super::*;
    use crate::ai::tree::Layer;
    use crate::nav::NavMesh;

    fn clip(layer: Layer, clip: u16, weight: f32, time: f32) -> PoseClip {
        PoseClip {
            layer,
            clip,
            weight,
            time,
        }
    }

    fn state(id: u16, x: f32, yaw: u16, pose: Pose) -> EnemyState {
        EnemyState {
            id,
            loc: Vec3::new(x, 0.0, 0.0),
            vel: Vec3::new(100.0, 0.0, 0.0),
            rot: [yaw, 0, 0],
            physics: 1,
            scale: 1.0,
            pose,
            ..Default::default()
        }
    }

    #[test]
    fn a_pose_eases_between_states() {
        let a = Pose {
            clips: vec![
                clip(Layer::Base, 1, 1.0, 0.9),
                clip(Layer::Base, 2, 0.5, 0.2),
                clip(Layer::Full, 3, 1.0, 1.5),
            ],
            shares: [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            mirror: 0.0,
            aim_offset: Some((1, Vec2::new(0.0, 0.5))),
        };
        let b = Pose {
            clips: vec![
                // (Gone round its end: 1 s long.)
                clip(Layer::Base, 1, 1.0, 0.0),
                clip(Layer::Base, 2, 1.0, 0.25),
                // (Started again, from well before its end.)
                clip(Layer::Full, 3, 1.0, 0.1),
                clip(Layer::Upper, 4, 1.0, 0.3),
            ],
            shares: [0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0],
            mirror: 1.0,
            aim_offset: Some((1, Vec2::new(1.0, 0.5))),
        };
        let secs = |c: u16| match c {
            1 => 1.0,
            3 => 3.0,
            _ => 0.0,
        };
        let p = blend_pose(&a, &b, 0.5, &secs);
        let find = |l: Layer, c: u16| {
            *p.clips
                .iter()
                .find(|x| x.layer == l && x.clip == c)
                .unwrap()
        };
        // The loop eased on across its end (0.9 to 1.0 to 0.0: 0.95 half way).
        assert!((find(Layer::Base, 1).time - 0.95).abs() < 1e-4);
        assert!((find(Layer::Base, 2).weight - 0.75).abs() < 1e-4);
        assert!((find(Layer::Base, 2).time - 0.225).abs() < 1e-4);
        // The one started again at its new time; the new one coming in.
        assert!((find(Layer::Full, 3).time - 0.1).abs() < 1e-4);
        assert!((find(Layer::Upper, 4).weight - 0.5).abs() < 1e-4);
        assert!((p.shares[5] - 0.5).abs() < 1e-4 && (p.shares[3] - 0.25).abs() < 1e-4);
        assert!((p.mirror - 0.5).abs() < 1e-4);
        assert_eq!(p.aim_offset, Some((1, Vec2::new(0.5, 0.5))));
        // (A clip only in the first going out.)
        let c = blend_pose(&b, &a, 0.75, &secs);
        let upper = c.clips.iter().find(|x| x.layer == Layer::Upper).unwrap();
        assert!((upper.weight - 0.25).abs() < 1e-4);
    }

    #[test]
    fn a_state_is_found_between_those_come_in() {
        let mut p = Puppets::default();
        let pose = Pose::default();
        p.take_in(
            vec![
                (1.1, state(7, 110.0, 65000, pose.clone())),
                (1.0, state(7, 100.0, 64000, pose.clone())),
                (1.2, state(7, 120.0, 400, pose.clone())),
            ],
            vec![
                (1.15, EnemyEvent::Hush { owner: 2 }),
                (1.05, EnemyEvent::Hush { owner: 1 }),
            ],
        );
        let none = |_: u16| 0.0;
        let at = |t: f32| {
            let (a, b, k) = p.bracket(7, t).unwrap();
            blend(a, b, k, &none, &none)
        };
        // Between two, eased (the short way round across a turn's end).
        let s = at(1.15);
        assert!((s.loc.x - 115.0).abs() < 1e-3);
        assert_eq!(s.rot[0], 65468);
        // Before the first: the first; past the last, carried on along its velocity
        // (no further than EXTRAPOLATE_MAX).
        assert!((at(0.5).loc.x - 100.0).abs() < 1e-3);
        assert!((at(1.3).loc.x - 130.0).abs() < 1e-2);
        assert!((at(5.0).loc.x - (120.0 + 100.0 * EXTRAPOLATE_MAX)).abs() < 1e-2);
        // The events due, in order.
        let owner = |e: &EnemyEvent| match e {
            EnemyEvent::Hush { owner } => *owner,
            _ => 0,
        };
        assert_eq!(p.due(1.1).iter().map(owner).collect::<Vec<_>>(), vec![1]);
        assert_eq!(p.due(2.0).iter().map(owner).collect::<Vec<_>>(), vec![2]);
        // Those no longer needed let go, the enemy forgotten once nothing comes.
        p.prune(3.0);
        assert_eq!(p.states[&7].1.len(), 2);
        p.prune(1.2 + FORGET_SECS + 0.1);
        assert!(p.bracket(7, 5.0).is_none());
    }

    /// A player standing at `feet` (Unreal space), as the AI sees him.
    fn standing(entity: Entity, feet: Vec3) -> PlayerView {
        PlayerView {
            entity,
            loc: feet + Vec3::Z * 110.0,
            feet,
            vel: Vec3::ZERO,
            yaw: 0.0,
            radius: 50.0,
            half_height: 110.0,
            health: 200.0,
            health_max: 200.0,
            alive: true,
            downed: false,
            cloaked: false,
            form: 1,
            airborne: false,
            jumping: false,
            physics: 1,
            transforming: false,
            shielded: false,
            eyes: feet + Vec3::Z * 198.0,
            nav_near: feet,
            nav_near_time: 0.0,
            team: super::super::world::PLAYER_TEAM,
            weakened: false,
            dodge: None,
            body: None,
        }
    }

    /// What came of a client's copies following the host's: the furthest any was
    /// from where the host's was at the moment shown (uu), the mean, the moments
    /// shown, those in another form than the host's, and the forms the host's took.
    #[derive(Debug, Default)]
    struct Followed {
        worst: f32,
        mean: f32,
        moments: usize,
        other_form: usize,
        forms: Vec<u8>,
        clips: usize,
    }

    /// A host's fight on `map` (its AI run at 30 frames a second for `secs`, a player
    /// standing at its start), `type_id` brought in at `spots` (each its feet: none
    /// found, the type's air spawn point nearest the start), and a client's copies of
    /// them following the host's states as its enemy packets would bring them (20 a
    /// second, `latency` on their way), shown at 60 frames a second SHOW_DELAY behind:
    /// each moment shown compared with the host's then.
    fn follow_fight(
        map: &str,
        type_id: &str,
        (player_at, spots): (
            impl Fn(&NavMesh, &crate::Level, Vec3) -> Vec3,
            impl Fn(&NavMesh, &crate::Level, Vec3) -> Vec<Vec3>,
        ),
        secs: f32,
        latency: f32,
    ) -> Option<Followed> {
        let dir = crate::assets_dir();
        let m = crate::MAPS.iter().find(|m| m.stem == map)?;
        let (Some(level), Some(nav)) = (crate::load_level_data(m), NavMesh::load(&dir, map)) else {
            eprintln!("{map}'s level or navigation not built: skipped");
            return None;
        };
        let data = AiData::load(&dir);
        let Some(b) = data.behaviour(type_id) else {
            eprintln!("ai.txt not built: skipped");
            return None;
        };
        if clips::for_type(type_id).is_none() {
            eprintln!("{type_id}'s clips aren't built: skipped");
            return None;
        }
        let kinds = crate::enemies::Kinds::load(&dir);
        let k = kinds.kinds.iter().find(|k| k.id == type_id)?;
        let sounds = Arc::new(audio::AiSounds::load(&dir));
        let new_world = || {
            let mut w = AiWorld {
                data: Arc::new(data.clone()),
                sounds: sounds.clone(),
                loaded: true,
                ..Default::default()
            };
            w.shared.tactical.reset(Some(&nav));
            w
        };
        let (mut host, mut client) = (new_world(), new_world());
        let mut ecs = bevy::ecs::world::World::new();
        let floor = |at: Vec3, radius: f32| {
            WorldView {
                level: Some(&level),
                ..Default::default()
            }
            .ground_at(at, radius, pawn::MAX_STEP_UU, 300.0)
            .map_or(at, |z| Vec3::new(at.x, at.y, z + pawn::FLOOR_DIST))
        };
        let start = player_at(&nav, &level, floor(ue::to_ue(level.player_start), 50.0));
        let player = ecs.spawn_empty().id();
        let mut view = WorldView {
            players: vec![standing(player, start)],
            nav: Some(&nav),
            level: Some(&level),
            ..Default::default()
        };
        let at = spots(&nav, &level, start);
        assert!(!at.is_empty(), "nowhere to bring them in");
        let mut ais = Vec::new();
        let mut copies = Vec::new();
        for (i, feet) in at.iter().enumerate() {
            let e = ecs.spawn_empty().id();
            let body = Body {
                feet: *feet,
                yaw: ue::yaw_of(start - *feet),
                radius: k.radius * ue::UU_PER_M,
                half_height: k.half_height * ue::UU_PER_M,
                health: k.health,
                mesh_scale: k.scale,
                forms: forms_of(&kinds, k),
            };
            let seed = 11 + i as u64;
            ais.push((e, new_ai(&mut host, &view, e, 0, b.clone(), body, seed)?));
            let c = ecs.spawn_empty().id();
            copies.push(new_ai(&mut client, &view, c, 0, b.clone(), body, seed)?);
        }
        // The host's frames: each one's state, by the host's time.
        let dt = 1.0 / 30.0;
        let mut frames: Vec<(f32, Vec<EnemyState>)> = Vec::new();
        let mut alert_left = 0.0;
        for _ in 0..(secs / dt) as usize {
            host.shared.now += dt;
            view.now = host.shared.now;
            alert_left -= dt;
            let alerting = (alert_left <= 0.0).then(|| (player, view.players[0].loc));
            if alerting.is_some() {
                alert_left = ALERT_SECS;
            }
            let mut slots: Vec<Slot> = ais
                .iter_mut()
                .map(|(e, ai)| Slot {
                    entity: *e,
                    ai,
                    dormant: false,
                })
                .collect();
            view.ais = slots.iter().map(|s| ai_view(s.ai, true)).collect();
            run_frame(
                &mut host.shared,
                &sounds,
                &mut view,
                &mut slots,
                dt,
                alerting,
            );
            // (What they did is the world's: not followed here.)
            host.shared.shots.clear();
            host.shared.launches.clear();
            host.shared.throws.clear();
            host.shared.sounds.clear();
            host.shared.loops.clear();
            host.shared.effects.clear();
            host.shared.effects_stopped.clear();
            host.shared.player_damage.clear();
            host.shared.barrier_damage.clear();
            host.shared.dialog.clear();
            host.shared.scanned.clear();
            let now = host.shared.now;
            let states = ais
                .iter()
                .enumerate()
                .map(|(i, (_, ai))| state_of(ai, i as u16 + 1, 1.0))
                .collect();
            frames.push((now, states));
        }
        // The client: the host's packets (20 a second, each its frame's states, by
        // the network) as they arrive, each frame shown SHOW_DELAY behind the host's
        // clock as it knows it.
        let mut puppets = Puppets::default();
        let mut next_packet = 0.0;
        let mut packets: Vec<(f32, f32, Vec<EnemyState>)> = Vec::new();
        for (t, states) in &frames {
            if *t >= next_packet {
                next_packet = t + 0.05;
                let wired = states.iter().map(crate::net::wired_state).collect();
                packets.push((t + latency, *t, wired));
            }
        }
        let mut out = Followed::default();
        let (first, last) = (frames[0].0, frames.last().unwrap().0);
        let mut sum = 0.0;
        let mut t = first;
        let mut taken = 0;
        while t < last + latency {
            t += 1.0 / 60.0;
            while taken < packets.len() && packets[taken].0 <= t {
                let (_, at, states) = &packets[taken];
                puppets.take_in(
                    states.iter().map(|s| (*at, s.clone())).collect(),
                    Vec::new(),
                );
                taken += 1;
            }
            let shown = t - latency - SHOW_DELAY;
            if shown < first + 0.2 || shown > last {
                continue;
            }
            // The host's then: its frames either side.
            let i = frames
                .partition_point(|(ft, _)| *ft <= shown)
                .min(frames.len() - 1);
            let (h0, h1) = (&frames[i.saturating_sub(1)], &frames[i]);
            let k = ((shown - h0.0) / (h1.0 - h0.0).max(1e-4)).clamp(0.0, 1.0);
            for (n, copy) in copies.iter_mut().enumerate() {
                let id = n as u16 + 1;
                let Some((a, b, kk)) = puppets.bracket(id, shown) else {
                    continue;
                };
                let near = if kk >= 0.5 { b } else { a };
                if let Some(fs) = near.forms.as_ref() {
                    let Ai { pawn, clips, .. } = copy;
                    take_form(pawn, clips, fs.current);
                }
                let main = copy.clips.clone();
                let other = copy.pawn.forms.as_ref().and_then(|f| f.other.clips.clone());
                let s = blend(a, b, kk, &|c| clip_secs(&main, c), &|c| {
                    other.as_ref().map_or(0.0, |o| clip_secs(o, c))
                });
                apply(copy, &s);
                // Where the host's was then.
                let host_at = h0.1[n].loc.lerp(h1.1[n].loc, k);
                let off = copy.pawn.loc.distance(host_at);
                out.worst = out.worst.max(off);
                sum += off;
                out.moments += 1;
                // Its form, and its pose's clips the form's.
                let host_form = if k < 0.5 { &h0.1[n] } else { &h1.1[n] }
                    .forms
                    .as_ref()
                    .map(|f| f.current);
                let form = copy.pawn.forms.as_ref().map(|f| f.current);
                if form != host_form {
                    out.other_form += 1;
                }
                if let Some(f) = host_form
                    && out.forms.last() != Some(&f)
                {
                    out.forms.push(f);
                }
                for c in &copy.pawn.pose.clips {
                    assert!(
                        (c.clip as usize) < copy.clips.clips.len(),
                        "a clip {c:?} not its form's ({} clips)",
                        copy.clips.clips.len()
                    );
                }
                out.clips = out.clips.max(copy.pawn.pose.clips.len());
            }
        }
        out.mean = sum / out.moments.max(1) as f32;
        Some(out)
    }

    /// A jet over Downfall: its copy flies, turns and takes its forms as the host's
    /// does, shown between the host's states.
    #[test]
    fn a_clients_jet_follows_the_hosts() {
        // (Brought in at the air spawn point nearest the map's start, the player on
        // the floor nearest it that it can see from there: ai/mod.rs's jet_fight.)
        let spawn = |nav: &NavMesh, start: Vec3| {
            nav.points
                .iter()
                .filter(|p| {
                    p.kind == crate::nav::PointKind::Air
                        && p.name.starts_with("TnSurvivalAirSpawnPoint")
                })
                .min_by(|a, b| {
                    a.at.distance(ue::from_ue(start))
                        .total_cmp(&b.at.distance(ue::from_ue(start)))
                })
                .map(|p| ue::to_ue(p.at))
        };
        let player_at = |nav: &NavMesh, level: &crate::Level, map_start: Vec3| {
            let Some(from) = spawn(nav, map_start) else {
                return map_start;
            };
            let sight = WorldView {
                level: Some(level),
                ..Default::default()
            };
            nav.points
                .iter()
                .filter(|p| p.kind == crate::nav::PointKind::Cell)
                .map(|p| ue::to_ue(p.at) + Vec3::Z * pawn::FLOOR_DIST)
                .filter(|at| sight.fast_trace(from, *at + Vec3::Z * 150.0))
                .min_by(|a, b| ue::vsize2d(*a - from).total_cmp(&ue::vsize2d(*b - from)))
                .unwrap_or(map_start)
        };
        let spots = |nav: &NavMesh, level: &crate::Level, at: Vec3| {
            // (The spawn point nearest the map's start, as the player's was found.)
            let map_start = ue::to_ue(level.player_start);
            spawn(nav, map_start)
                .or_else(|| spawn(nav, at))
                .into_iter()
                .collect()
        };
        let Some(f) = follow_fight("downfall", "Flyer_Aut", (player_at, spots), 40.0, 0.04) else {
            return;
        };
        eprintln!("{f:?}");
        assert!(f.moments > 1000);
        assert!(f.forms.contains(&forms::ROBOT) && f.forms.contains(&forms::ALT));
        // (Shown between states 50 ms apart: a jet flying fast and turning may be
        // a little off the line between them.)
        assert!(f.mean < 15.0, "on average {} uu off", f.mean);
        assert!(f.worst < 150.0, "as far as {} uu off", f.worst);
        assert!(
            f.other_form < f.moments / 50,
            "{} in another form",
            f.other_form
        );
    }

    /// Soldiers on Ignition coming for the player: their copies walk, run and take
    /// cover where the host's do.
    #[test]
    fn a_clients_soldiers_follow_the_hosts() {
        let cells = |nav: &NavMesh, _: &crate::Level, start: Vec3| {
            let mut out: Vec<Vec3> = Vec::new();
            for p in &nav.points {
                if p.kind != crate::nav::PointKind::Cell {
                    continue;
                }
                let at = ue::to_ue(p.at) + Vec3::Z * pawn::FLOOR_DIST;
                let d = ue::vsize2d(at - start);
                if (2500.0..4500.0).contains(&d)
                    && (at.z - start.z).abs() < 300.0
                    && out.iter().all(|o| ue::vsize2d(*o - at) > 1500.0)
                {
                    out.push(at);
                }
                if out.len() == 3 {
                    break;
                }
            }
            out
        };
        let start = |_: &NavMesh, _: &crate::Level, at: Vec3| at;
        let Some(f) = follow_fight("ignition", "SoldierDec_Kaon", (start, cells), 25.0, 0.04)
        else {
            return;
        };
        eprintln!("{f:?}");
        assert!(f.moments > 1000);
        assert!(f.mean < 5.0, "on average {} uu off", f.mean);
        assert!(f.worst < 60.0, "as far as {} uu off", f.worst);
        assert!(f.clips > 0, "no pose");
    }
}
