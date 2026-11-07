//! What a death's destructibles set off (TnDeathActionSpawnDestructible: an
//! HmDestructibleComponent's pieces' effects, deaths.rs's [`Destructible`]), and the
//! gibs they throw.
//!
//! * **Its pieces' effects**: particle systems and sounds at their offsets in the
//!   dying pawn's frame; decals laid on what is under them (their material's
//!   decal system, levelfx.rs, as big as the piece says); a blast's damage on the
//!   players within its radius (HurtRadius: its damage falling off to nothing at its
//!   edge, its momentum throwing them).
//! * **Its gibs** (HmGibsDestructionEffectFactory): `count` chunks, each one of its
//!   meshes at random (their mesh systems, build_weapon_fx.py's `<mesh>_FX`), each
//!   starting at random within its SpawnVolumeScale round the effect's place, flying
//!   out at its LinearSpeed within its VelocityConeAngle round the effect's forward
//!   (and the pawn's speed times TransferVelocityMultiplier), spinning at its
//!   AngularSpeed (PhysX's limit, 7 radians a second, on it), bouncing off the level,
//!   its trail on it; over the last Duration of its LifeSpan it deresses (its
//!   DeresSound), drawn no more past its NormalizedTimeToMakeMeshInvisible, its trail
//!   let go; then gone. A chunk turns about its middle, strikes the level with its
//!   thickness (half its mesh's least size) and comes to rest lying flat, as a rigid
//!   body would: Downfall's scaffolds' pieces are bars up to 13 m long. A chunk
//!   whose mesh system is lit (the barrel's, the Swarmer's and the scaffolds'
//!   pieces, in their meshes' own materials: build_weapon_fx.py) is lit by the light
//!   where it is thrown from, as the game's light environment lights it
//!   (FxSystems::set_lit); the deaths' gibs glow in their own colour, unlit.
//! * **The debris pool** (HmDebrisPool, which every chunk and gib goes through): at
//!   most MAX_GIBS out at once, the oldest gone at once to make room for a new one,
//!   and no more than GIBS_PER_FRAME thrown a frame, the rest waiting their turn.

use std::sync::Arc;

use bevy::prelude::*;

use crate::deaths::{self, Destructible, Piece, Place, UU};
use crate::levelfx::{FxId, FxSystems};
use crate::sound::Sfx;

/// A gib's size against the level (metres: the chunks are small).
const GIB_RADIUS: f32 = 0.12;
/// How much of its speed a gib keeps along a surface it strikes, and off it.
const GIB_FRICTION: f32 = 0.6;
const GIB_BOUNCE: f32 = 0.3;
/// Slower than this (metres a second) on a floor, a gib lies still.
const GIB_REST: f32 = 0.4;
/// The deresser's NormalizedTimeToMakeMeshInvisible (TnDeresser's default).
const GIB_INVISIBLE: f32 = 0.39685;
/// The debris pool's limits (Default__HmDebrisPool: _MaxActiveDebris and
/// _MaxDebrisSpawnsPerFrame; no map sets its own). Its SpawnPendingDebrisChunk
/// destroys the oldest chunk out to make room (RemoveDebris(0)), and its Tick throws
/// the waiting ones, oldest first, that many a frame.
const MAX_GIBS: usize = 50;
const GIBS_PER_FRAME: usize = 5;
/// How long a chunk takes to settle flat once it comes to rest (seconds).
const GIB_SETTLE: f32 = 0.25;

/// A chunk waiting its turn in the pool: where the effect put it (its pivot, as the
/// game spawns its rigid body there) and how it flies.
struct Pending {
    mesh: Arc<str>,
    trail: Option<Arc<str>>,
    at: Vec3,
    v: Vec3,
    turn: Quat,
    spin: Vec3,
    life: f32,
    deres: f32,
    sound: Option<&'static str>,
    deres_sound: Option<&'static str>,
}

/// A gib flying (or lying): `at` its middle (its mesh's, `centre` from its pivot in
/// its own frame), `radius` its thickness against the level.
struct Gib {
    mesh: Option<FxId>,
    trail: Option<FxId>,
    at: Vec3,
    v: Vec3,
    turn: Quat,
    spin: Vec3,
    centre: Vec3,
    half: Vec3,
    radius: f32,
    /// Settling flat: from what turn to what, since when (its age).
    settle: Option<(Quat, Quat, f32)>,
    age: f32,
    life: f32,
    deres: f32,
    deres_sound: Option<&'static str>,
    deressing: bool,
    resting: bool,
}

/// The gibs flying, and the blasts the deaths set off this frame (for the players'
/// side to hurt them: main.rs's death_blasts).
#[derive(Resource, Default)]
pub struct Gibs {
    list: Vec<Gib>,
    pending: std::collections::VecDeque<Pending>,
    pub blasts: Vec<Blast>,
}

/// A destructible's blast (HurtRadius): its damage type, damage, radius (metres),
/// momentum (Unreal's units) and middle.
#[derive(Clone, Debug)]
pub struct Blast {
    pub kind: Arc<str>,
    pub damage: f32,
    pub radius: f32,
    pub momentum: f32,
    pub at: Vec3,
}

/// A random direction (VRand).
fn unit(seed: &mut u32) -> Vec3 {
    let z = deaths::rand(seed) * 2.0 - 1.0;
    let phi = deaths::rand(seed) * std::f32::consts::TAU;
    let r = (1.0 - z * z).max(0.0).sqrt();
    Vec3::new(r * phi.cos(), z, r * phi.sin())
}

/// A direction within a cone of `half` radians round `dir` (VRandCone).
fn in_cone(dir: Vec3, half: f32, seed: &mut u32) -> Vec3 {
    let cos = 1.0 - deaths::rand(seed) * (1.0 - half.cos());
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let phi = deaths::rand(seed) * std::f32::consts::TAU;
    let (a, b) = dir.any_orthonormal_pair();
    dir * cos + (a * phi.cos() + b * phi.sin()) * sin
}

/// An effect's axes from a rotator in the pawn's frame (pitch, yaw, roll: Unreal's
/// units): its X and Y in the world.
fn piece_axes(place: &Place, turn: Vec3) -> (Vec3, Vec3) {
    let [x, y, _] = crate::ai::ue::rotator_axes(turn.x, turn.y, turn.z);
    (place.dir(x), place.dir(y))
}

/// Sets off destructible `d` where `place` says (its pieces' offsets in the pawn's
/// frame): its particle systems, its sounds (`sounds`: this PC's to play), its decals,
/// its blasts and its gibs.
#[allow(clippy::too_many_arguments)]
pub fn set_off(
    d: &Destructible,
    place: &Place,
    seed: &mut u32,
    sounds: bool,
    fx: &mut Option<ResMut<FxSystems>>,
    sfx: &mut Sfx,
    gibs: &mut Gibs,
) {
    let forward = place.dir(Vec3::X);
    for piece in &d.pieces {
        match piece {
            Piece::Particle { system, at } => {
                if let Some(fx) = fx.as_mut() {
                    let p = place.point(*at);
                    if let Some(id) = fx.spawn(system, p, forward, &[]) {
                        fx.place_turned(id, p, forward, place.dir(Vec3::Y));
                    }
                }
            }
            Piece::Sound(cue) => {
                if sounds {
                    sfx.play_at(cue, place.at);
                }
            }
            Piece::Decal {
                material,
                size,
                range,
                at,
                ..
            } => {
                if let Some(fx) = fx.as_mut() {
                    // (Its decal system's mark is 100 uu across: build_weapon_fx.py's
                    // decal_system.)
                    let w = size.0 + range.0 * deaths::rand(seed);
                    let p = place.point(*at);
                    let name = format!("{material}_DECAL_FX");
                    if let Some(id) = fx.spawn_scaled(&name, p, forward, (w / 100.0).max(0.01), &[])
                    {
                        fx.place_turned(id, p, forward, place.dir(Vec3::Y));
                    }
                }
            }
            Piece::Damage {
                kind,
                damage,
                radius,
                momentum,
                at,
            } => gibs.blasts.push(Blast {
                kind: kind.clone(),
                damage: *damage,
                radius: radius / UU,
                momentum: *momentum,
                at: place.point(*at),
            }),
            Piece::Gibs(g) => {
                let (x, y) = piece_axes(place, g.turn);
                let origin = place.point(g.at);
                for _ in 0..g.count {
                    if g.meshes.is_empty() {
                        break;
                    }
                    let mesh = &g.meshes[((deaths::rand(seed) * g.meshes.len() as f32) as usize)
                        .min(g.meshes.len() - 1)];
                    let r = unit(seed) * g.volume;
                    // (Its spawn volume in the effect's frame.)
                    let z = place.dir(crate::ai::ue::rotator_axes(g.turn.x, g.turn.y, g.turn.z)[2]);
                    let at = origin + (x * r.x + y * r.y + z * r.z) / UU;
                    let half = (g.cone * 0.5).to_radians();
                    let speed = g.speed.0 + (g.speed.1 - g.speed.0) * deaths::rand(seed);
                    let v = in_cone(x, half, seed) * speed / UU + place.velocity * g.carry;
                    let spin = g.spin.0 + (g.spin.1 - g.spin.0) * deaths::rand(seed);
                    let spin = (unit(seed) * spin.to_radians()).clamp_length_max(7.0);
                    // (Turned as the effect is: its mesh's X along the effect's X and
                    // its Y along the effect's Y, as fly_gibs draws it.)
                    let (x, y) = (x.normalize_or(Vec3::X), y.normalize_or(Vec3::Z));
                    gibs.pending.push_back(Pending {
                        mesh: mesh.clone(),
                        trail: g.trail.clone(),
                        at,
                        v,
                        turn: Quat::from_mat3(&Mat3::from_cols(x, y.cross(x), y)).normalize(),
                        spin,
                        life: g.lifespan.max(0.1),
                        deres: g.deres.min(g.lifespan).max(0.0),
                        sound: if sounds { g.sound } else { None },
                        deres_sound: if sounds { g.deres_sound } else { None },
                    });
                }
            }
            Piece::Other => {}
        }
    }
}

/// The pool throws the chunks waiting their turn (GIBS_PER_FRAME a frame, the oldest
/// out going at once to make room past MAX_GIBS), and the gibs fly, bounce off the
/// level, settle and deres, their meshes and trails with them.
fn fly_gibs(
    time: Res<Time>,
    level: Res<crate::Level>,
    mut gibs: ResMut<Gibs>,
    mut fx: Option<ResMut<FxSystems>>,
    mut sfx: ResMut<Sfx>,
) {
    let gibs = &mut *gibs;
    for _ in 0..GIBS_PER_FRAME {
        let Some(p) = gibs.pending.pop_front() else {
            break;
        };
        if gibs.list.len() >= MAX_GIBS {
            let old = gibs.list.remove(0);
            if let Some(fx) = fx.as_mut() {
                if let Some(m) = old.mesh {
                    fx.kill(m);
                }
                if let Some(t) = old.trail {
                    fx.stop(t);
                }
            }
        }
        gibs.list.push(throw(p, &mut fx, &mut sfx));
    }
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    gibs.list.retain_mut(|g| {
        g.age += dt;
        if !g.resting {
            g.v.y += crate::ragdoll::GRAVITY * dt;
            let step = g.v * dt;
            let len = step.length();
            if len > 1e-6 {
                let dir = step / len;
                match level.ray_hit(g.at, dir, len + g.radius) {
                    Some((t, n)) => {
                        g.at += dir * (t - g.radius).max(0.0);
                        let vn = g.v.dot(n);
                        let vt = g.v - n * vn;
                        g.v = vt * GIB_FRICTION - n * vn * GIB_BOUNCE;
                        g.spin *= GIB_FRICTION;
                        if n.y > 0.7 && g.v.length() < GIB_REST {
                            g.resting = true;
                            g.v = Vec3::ZERO;
                            g.spin = Vec3::ZERO;
                            g.settle = Some((g.turn, lying_flat(g.turn, g.half, n), g.age));
                        }
                    }
                    None => g.at += step,
                }
            }
            let w = g.spin.length();
            if w > 1e-6 {
                g.turn = (Quat::from_axis_angle(g.spin / w, w * dt) * g.turn).normalize();
            }
        }
        if let Some((from, to, since)) = g.settle {
            let k = ((g.age - since) / GIB_SETTLE).clamp(0.0, 1.0);
            g.turn = from.slerp(to, k * k * (3.0 - 2.0 * k));
            if k >= 1.0 {
                g.settle = None;
            }
        }
        let left = g.life - g.age;
        if !g.deressing && left <= g.deres {
            // Its deres (TnDeresser.Deres: its sound), its trail let go.
            g.deressing = true;
            if let Some(cue) = g.deres_sound {
                sfx.play_at(cue, g.at);
            }
            if let (Some(fx), Some(t)) = (fx.as_mut(), g.trail.take()) {
                fx.stop(t);
            }
        }
        let gone_at = g.deres * (1.0 - GIB_INVISIBLE);
        if let Some(fx) = fx.as_mut() {
            if g.deressing
                && left <= gone_at
                && let Some(m) = g.mesh.take()
            {
                fx.kill(m);
            }
            let (x, y) = (g.turn * Vec3::X, g.turn * Vec3::Z);
            if let Some(m) = g.mesh {
                // (Its pivot, from its middle.)
                fx.place_turned(m, g.at - g.turn * g.centre, x, y);
            }
            if let Some(t) = g.trail {
                fx.place(t, g.at, -g.v.normalize_or(x), g.v);
            }
        }
        if left <= 0.0 {
            if let Some(fx) = fx.as_mut() {
                if let Some(m) = g.mesh.take() {
                    fx.kill(m);
                }
                if let Some(t) = g.trail.take() {
                    fx.stop(t);
                }
            }
            return false;
        }
        true
    });
}

/// A waiting chunk thrown (HmDebrisPool.SpawnPendingDebrisChunk): its mesh and trail
/// started, its sound played. Its pivot is where the effect put it; it flies by its
/// middle, the middle of its mesh's box.
fn throw(p: Pending, fx: &mut Option<ResMut<FxSystems>>, sfx: &mut Sfx) -> Gib {
    let system = format!("{}_FX", p.mesh);
    let (lo, hi) = fx
        .as_mut()
        .and_then(|fx| fx.mesh_bounds(&system))
        .unwrap_or((Vec3::ZERO, Vec3::ZERO));
    let centre = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    let at = p.at + p.turn * centre;
    let x = p.turn * Vec3::X;
    let (mesh, trail) = match fx.as_mut() {
        Some(fx) => {
            let mesh = fx.spawn(&system, p.at, x, &[]);
            // (Lit by the light where it is thrown from, as the game's light
            // environment lights a chunk, where its system is: the barrel's, the
            // Swarmer's and the scaffolds' pieces, not the deaths' glowing gibs.)
            if let Some(id) = mesh {
                fx.set_lit(id);
            }
            let trail = p
                .trail
                .as_ref()
                .and_then(|t| fx.spawn(t, at, -p.v.normalize_or(x), &[]));
            (mesh, trail)
        }
        None => (None, None),
    };
    if let Some(cue) = p.sound {
        sfx.play_at(cue, at);
    }
    Gib {
        mesh,
        trail,
        at,
        v: p.v,
        turn: p.turn,
        spin: p.spin,
        centre,
        half,
        radius: half.min_element().max(GIB_RADIUS),
        settle: None,
        age: 0.0,
        life: p.life,
        deres: p.deres,
        deres_sound: p.deres_sound,
        deressing: false,
        resting: false,
    }
}

/// A chunk at rest on a floor (its normal `up`) turned to lie flat on it: its
/// thinnest side's axis (of its box's `half` sizes) along the normal, the way that
/// axis was nearest to pointing, turned no more than that takes.
fn lying_flat(turn: Quat, half: Vec3, up: Vec3) -> Quat {
    let axis = if half.x <= half.y && half.x <= half.z {
        Vec3::X
    } else if half.y <= half.z {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let now = turn * axis;
    let want = if now.dot(up) >= 0.0 { up } else { -up };
    (Quat::from_rotation_arc(now.normalize_or(want), want) * turn).normalize()
}

/// The deaths' blasts hurt the players at this PC within their radius (HurtRadius:
/// all of it at its middle, nothing at its edge), with their momentum.
fn death_blasts(
    mut gibs: ResMut<Gibs>,
    mut players: Query<(&Transform, &mut crate::Player), Without<crate::RemotePlayer>>,
) {
    for b in std::mem::take(&mut gibs.blasts) {
        // (TnDamageTypeHoverGroundPoundAi: the Titan's; the Thrown deaths' own.)
        let death = if b.kind.contains("GroundPound") {
            crate::enemies::Death::Explosive
        } else {
            crate::enemies::Death::Plain
        };
        for (t, mut p) in &mut players {
            let centre = p.robot_centre(t.translation);
            let d = centre.distance(b.at);
            if d > b.radius || b.damage <= 0.0 {
                continue;
            }
            let k = 1.0 - d / b.radius.max(1e-3);
            let dir = (centre - b.at).normalize_or(Vec3::Y);
            let hit = crate::enemies::Hit::new(death, centre, dir)
                .with_momentum(b.momentum * k)
                .of_type(crate::killfeed::damage_name(&b.kind));
            crate::damage_player(&mut p, b.damage * k, hit);
        }
    }
}

/// Every gib gone (a new match).
fn clear_gibs(mut gibs: ResMut<Gibs>, mut fx: Option<ResMut<FxSystems>>) {
    for g in gibs.list.drain(..) {
        if let Some(fx) = fx.as_mut() {
            if let Some(m) = g.mesh {
                fx.kill(m);
            }
            if let Some(t) = g.trail {
                fx.stop(t);
            }
        }
    }
    gibs.pending.clear();
    gibs.blasts.clear();
}

pub struct GibsPlugin;

impl Plugin for GibsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Gibs>()
            .add_systems(
                Update,
                (fly_gibs, death_blasts).run_if(in_state(crate::menu::AppState::Game)),
            )
            .add_systems(OnExit(crate::menu::AppState::Game), clear_gibs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chunk come to rest turns to lie flat on its thinnest side, the least turn
    /// that takes, its middle where it was (it strikes the level with that
    /// thickness).
    #[test]
    fn a_chunk_at_rest_lies_on_its_flat() {
        // A bar 12 m long, 0.6 m deep and 1 m across, at rest tilted.
        let half = Vec3::new(6.0, 0.3, 0.5);
        let up = Vec3::Y;
        for turn in [
            Quat::from_euler(EulerRot::XYZ, 0.4, 1.0, 0.7),
            Quat::from_euler(EulerRot::XYZ, 2.9, -0.3, 1.9),
            Quat::IDENTITY,
        ] {
            let flat = lying_flat(turn, half, up);
            // (Its thinnest side's axis along the floor's normal.)
            assert!((flat * Vec3::Y).dot(up).abs() > 0.9999, "{turn:?}");
            // (No more turn than that takes: as far as that axis was from it.)
            let tilt = (turn * Vec3::Y).dot(up).abs().clamp(0.0, 1.0).acos();
            assert!(flat.angle_between(turn) <= tilt + 1e-3, "{turn:?}");
        }
        // Lying flat already: left as it is.
        let flat = Quat::from_rotation_y(0.7);
        assert!(lying_flat(flat, half, up).angle_between(flat) < 1e-4);
        // On its thickness, not a gib's least radius.
        let g = half.min_element().max(GIB_RADIUS);
        assert_eq!(g, 0.3);
    }

    /// The pool throws no more than GIBS_PER_FRAME a frame and keeps no more than
    /// MAX_GIBS out, the oldest gone first.
    #[test]
    fn the_pool_throws_a_few_a_frame_and_keeps_the_newest() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::Level::default())
            .insert_resource(Sfx::default())
            .init_resource::<Gibs>()
            .add_systems(Update, fly_gibs);
        let pending = |k: usize| Pending {
            mesh: Arc::from(format!("piece{k}")),
            trail: None,
            at: Vec3::new(k as f32, 100.0, 0.0),
            v: Vec3::ZERO,
            turn: Quat::IDENTITY,
            spin: Vec3::ZERO,
            life: 100.0,
            deres: 0.0,
            sound: None,
            deres_sound: None,
        };
        let total = MAX_GIBS + 30;
        app.world_mut()
            .resource_mut::<Gibs>()
            .pending
            .extend((0..total).map(pending));
        app.update();
        assert_eq!(app.world().resource::<Gibs>().list.len(), GIBS_PER_FRAME);
        for _ in 0..total {
            app.update();
        }
        let gibs = app.world().resource::<Gibs>();
        assert!(gibs.pending.is_empty());
        assert_eq!(gibs.list.len(), MAX_GIBS);
        // (The first thrown are the ones gone.)
        assert_eq!(gibs.list[0].at.x, (total - MAX_GIBS) as f32);
    }
}
