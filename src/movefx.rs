//! The effects a robot's moves play under his feet, as the game's clips play them
//! (their HmAnimNotify_PlayEffect notifies: tools/build_robot_fx.py's
//! models/robot_fx.txt, each class's own clips' where it plays its own):
//!
//! * **Dashing** (Nav_Boost_F/B/L/R on the ground, Nav_Air_Boost_F/B/L/R in the
//!   air) and **jumping** (Nav_TakeOff_01): the boosters at his ankles and the soles
//!   of his feet fire (CarJump_01_FX on BoosterSocket_L/R and
//!   MeleeSocket_LeftFoot/RightFoot) in his EnergonColor (the notifies'
//!   TnParticleSystemParameterEnergonColor: his faction's, the Autobots' red as
//!   salmon-pink flames and sparks), and a dash to the side fires the boosters
//!   again to stop. (The dust the clips' notifies also kick up as a dash ends,
//!   Long_Fall_Landing_1_FX as a side dash touches down, Long_Fall_Landing_Sm_FX as
//!   one forward or back ends, a Destroyer's or Titan's back dash's
//!   Vehland_Low_Med_FX, is left out: DASH_DUST. The game's multiplayer shows none.)
//! * **Landing** (Nav_Land, Nav_Land_Hard_01): dust thrown up at his right ankle
//!   (Long_Fall_Landing_1_FX).
//!
//! Each starts as its clip passes its time (HmAnimNotify_PlayEffect.Notify), on its
//! socket, moved and turned there by its RelativeLocation (at his size: the
//! socket's matrix has his mesh's scale in it) and RelativeRotation, at its own
//! size (as the AI's are, and the abilities' on his sockets). An attached one
//! (bAttachToMesh: PlayEffectOnSocket) is carried with the socket and lit by the
//! level's light where it starts (levelfx's set_lit: attached to his skeletal mesh,
//! it takes the mesh's light environment, so the dust is the tan haze of the game's
//! videos, where an effect the game spawns loose takes no light and would be a dark
//! one; the sun only where it reaches him, as a light environment sees it). One
//! with bAutoDeactivate is stopped (Deactivate: what it has out lives on) once its
//! clip's length and 0.04 s have passed since it started. The clip is the one
//! animate_jazz last started for him (Player::move_clip), local or online alike (an
//! online player's clips arrive as JazzEvent::Clip).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

use crate::abilities::Sockets;
use crate::ai::ue;
use crate::levelfx::{FxId, FxSystems};
use crate::{Form, Player};

const TABLE: &str = "models/robot_fx.txt";
/// HmAnimNotify_PlayEffect's bAutoDeactivate: the clip's length and this long.
const AUTO_DEACTIVATE_EXTRA: f32 = 0.04;
/// How far (metres) the sun is looked for from where an effect starts.
const SUN_TRACE: f32 = 500.0;
/// The dashes' clips (Nav_Boost_*, Nav_Air_Boost_*: robot_fx.txt's names for them)
/// and the dust their notifies kick up as they end, which the demo leaves out: in
/// the game's multiplayer a dash leaves no dust (only the boosters fire).
const DASH_CLIPS: [&str; 2] = ["boost_", "air_boost_"];
const DASH_DUST: [&str; 3] = [
    "Long_Fall_Landing_1_FX",
    "Long_Fall_Landing_Sm_FX",
    "Vehland_Low_Med_FX",
];

pub struct MoveFxPlugin;

impl Plugin for MoveFxPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MoveFx::load())
            .init_resource::<HeldFx>()
            .add_systems(OnEnter(crate::menu::AppState::Game), clear_held)
            .add_systems(
                PostUpdate,
                // (After the pose is on the bones, before the effects run.)
                play_move_fx
                    .after(bevy::transform::TransformSystems::Propagate)
                    .before(crate::levelfx::SpawnedFxRun)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

/// One of a clip's effects (an fx line).
#[derive(Clone, Debug, PartialEq)]
struct Notify {
    /// Seconds into the clip (as the demo plays it at speed 1).
    at: f32,
    system: Arc<str>,
    socket: Arc<str>,
    attached: bool,
    /// Its RelativeLocation (Unreal units) and RelativeRotation's axes in the
    /// socket's frame.
    offset: Vec3,
    turn: [Vec3; 3],
    /// The colour parameters set to his EnergonColor.
    colors: Vec<Arc<str>>,
    auto_deactivate: bool,
}

/// The clips' effects: by class (character::CLASSES) and robot.glb clip, the
/// clip's length and its effects in order.
#[derive(Resource, Default)]
pub struct MoveFx {
    clips: HashMap<(usize, usize), (f32, Vec<Notify>)>,
}

impl MoveFx {
    fn load() -> MoveFx {
        let path = crate::assets_dir().join(TABLE);
        match std::fs::read_to_string(&path) {
            Ok(text) => MoveFx::parse(&text),
            Err(_) => {
                info!(
                    "{TABLE} not built: no effects under the robots' feet (tools/build_robot_fx.py)"
                );
                MoveFx::default()
            }
        }
    }

    fn parse(text: &str) -> MoveFx {
        let class_of = |s: &str| crate::character::CLASSES.iter().position(|c| *c == s);
        let floats = |s: &str| -> Option<Vec3> {
            let v: Vec<f32> = s
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
        };
        let mut clips: HashMap<(usize, usize), (f32, Vec<Notify>)> = HashMap::new();
        let mut dashes = Vec::new();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            match (f.first().copied(), f.len()) {
                (Some("clip"), 5) => {
                    let (Some(class), Ok(clip), Ok(length)) =
                        (class_of(f[1]), f[2].parse(), f[4].parse())
                    else {
                        continue;
                    };
                    clips.entry((class, clip)).or_default().0 = length;
                    if DASH_CLIPS.iter().any(|d| f[3].starts_with(d)) {
                        dashes.push((class, clip));
                    }
                }
                (Some("fx"), 11) => {
                    let (Some(class), Ok(clip), Ok(at), Some(offset), Some(turn)) = (
                        class_of(f[1]),
                        f[2].parse::<usize>(),
                        f[3].parse::<f32>(),
                        floats(f[7]),
                        floats(f[8]),
                    ) else {
                        continue;
                    };
                    let colors = if f[9] == "-" {
                        Vec::new()
                    } else {
                        f[9].split(',')
                            .filter(|n| !n.starts_with('@'))
                            .map(Arc::from)
                            .collect()
                    };
                    clips.entry((class, clip)).or_default().1.push(Notify {
                        at,
                        system: Arc::from(f[4]),
                        socket: Arc::from(f[5]),
                        attached: f[6] == "1",
                        offset,
                        turn: ue::rotator_axes(turn.x, turn.y, turn.z),
                        colors,
                        auto_deactivate: f[10] == "1",
                    });
                }
                _ => {}
            }
        }
        for (key, (_, notifies)) in clips.iter_mut() {
            if dashes.contains(key) {
                notifies.retain(|n| !DASH_DUST.contains(&&*n.system));
            }
            notifies.sort_by(|a, b| a.at.total_cmp(&b.at));
        }
        MoveFx { clips }
    }

    /// The effects of clip `clip` of class `class` that start from `from` up to (not
    /// including) `to` seconds into it.
    fn between(
        &self,
        class: usize,
        clip: usize,
        from: f32,
        to: f32,
    ) -> impl Iterator<Item = &Notify> {
        self.clips
            .get(&(class, clip))
            .into_iter()
            .flat_map(|(_, n)| n.iter())
            .filter(move |n| n.at >= from && n.at < to)
    }

    /// How long clip `clip` of class `class` is, if it plays any effects.
    fn length(&self, class: usize, clip: usize) -> Option<f32> {
        self.clips
            .get(&(class, clip))
            .filter(|(_, n)| !n.is_empty())
            .map(|(l, _)| *l)
    }
}

/// The effects carried with their sockets: whose, on which socket, where on it,
/// and when it is stopped (the game's time), if it is.
#[derive(Resource, Default)]
struct HeldFx(Vec<Held>);

struct Held {
    id: FxId,
    by: Entity,
    socket: Arc<str>,
    offset: Vec3,
    turn: [Vec3; 3],
    until: Option<f32>,
}

/// An effect's place and axes (Bevy's) on its socket (`at` and `axes`, Bevy's, the
/// robot drawn `size` times its model's size there): the socket's frame moved by
/// `offset` and turned by `turn` there, as an attached component's
/// RelativeLocation and RelativeRotation (Unreal's space), the move at the robot's
/// size (the socket's matrix has his mesh's scale in it).
fn on_socket(
    at: Vec3,
    axes: [Vec3; 3],
    size: f32,
    offset: Vec3,
    turn: [Vec3; 3],
) -> (Vec3, Vec3, Vec3) {
    let axes = axes.map(ue::dir_to_ue);
    let world = |v: Vec3| axes[0] * v.x + axes[1] * v.y + axes[2] * v.z;
    let place = ue::to_ue(at) + world(offset * size);
    (
        ue::from_ue(place),
        ue::dir_from_ue(world(turn[0])),
        ue::dir_from_ue(world(turn[1])),
    )
}

/// Moves each Jazz's clip on and starts the effects it passes; carries the held
/// ones with their sockets, stopping those whose time is up and those whose Jazz
/// is gone.
#[allow(clippy::too_many_arguments)]
fn play_move_fx(
    time: Res<Time>,
    table: Res<MoveFx>,
    mut players: Query<(Entity, &mut Player)>,
    mut sockets: Sockets,
    mut fx: ResMut<FxSystems>,
    mut held: ResMut<HeldFx>,
    level: Option<Res<crate::Level>>,
    lighting: Option<Res<crate::lighting::MapLighting>>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    for (entity, mut player) in &mut players {
        let Some((clip, t, speed)) = player.move_clip else {
            continue;
        };
        let class = player.kit.class;
        let Some(length) = table.length(class, clip) else {
            player.move_clip = None;
            continue;
        };
        let to = t + dt * speed;
        player.move_clip = (to < length).then_some((clip, to, speed));
        if player.form != Form::Robot || player.waiting {
            continue;
        }
        let energon =
            crate::car::effect_color(crate::car::energon_srgb(player.look.energon()), 255);
        for n in table.between(class, clip, t, to) {
            let Some((at, axes, size)) = sockets.frame3(entity, &n.socket) else {
                continue;
            };
            let (place, x, y) = on_socket(at, axes, size, n.offset, n.turn);
            let params: Vec<(&str, [u8; 4])> = n.colors.iter().map(|c| (&**c, energon)).collect();
            let Some(id) = fx.spawn(&n.system, place, x, &params) else {
                continue;
            };
            fx.place_turned(id, place, x, y);
            if n.attached {
                if in_sun(level.as_deref(), lighting.as_deref(), place) {
                    fx.set_lit(id);
                } else {
                    fx.set_lit_shaded(id);
                }
                held.0.push(Held {
                    id,
                    by: entity,
                    socket: n.socket.clone(),
                    offset: n.offset,
                    turn: n.turn,
                    until: n
                        .auto_deactivate
                        .then(|| now + length / speed.max(1e-3) + AUTO_DEACTIVATE_EXTRA),
                });
            }
        }
    }
    held.0.retain(|h| {
        if !fx.alive(h.id) {
            return false;
        }
        if !players.contains(h.by) || h.until.is_some_and(|u| now >= u) {
            fx.stop(h.id);
            return false;
        }
        if let Some((at, axes, size)) = sockets.frame3(h.by, &h.socket) {
            let (place, x, y) = on_socket(at, axes, size, h.offset, h.turn);
            fx.place_turned(h.id, place, x, y);
        }
        true
    });
}

/// Whether the sun reaches `at` (the map's sun, unblocked by the level on the way
/// to it from half a metre over the spot), as the light environment the effect
/// takes from his mesh tells: out of it, it is lit by the level's lights alone.
pub(crate) fn in_sun(
    level: Option<&crate::Level>,
    lighting: Option<&crate::lighting::MapLighting>,
    at: Vec3,
) -> bool {
    let Some(sun) = lighting.and_then(|l| l.sun.as_ref()) else {
        return false;
    };
    let towards = (-sun.direction).normalize_or_zero();
    level.is_none_or(|level| {
        towards == Vec3::ZERO || level.ray(at + Vec3::Y * 0.5, towards, SUN_TRACE).is_none()
    })
}

/// Every held effect let go (a new match).
fn clear_held(mut fx: ResMut<FxSystems>, mut held: ResMut<HeldFx>) {
    for h in held.0.drain(..) {
        fx.stop(h.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "# x\n\
clip\tinfiltrator\t13\tboost_l\t1.2\n\
fx\tinfiltrator\t13\t0.5164\tCarJump_01_FX\tBoosterSocket_R\t1\t50 0 0\t16384 0 0\tColor\t1\n\
fx\tinfiltrator\t13\t0.1235\tCarJump_01_FX\tBoosterSocket_R\t1\t50 0 0\t0 0 0\tColor\t1\n\
fx\tinfiltrator\t13\t0.44\tLong_Fall_Landing_1_FX\tRootSocket\t1\t0 0 0\t0 0 0\t-\t1\n\
clip\ttitan\t10\tland\t0.5667\n\
fx\ttitan\t10\t0.00122\tLong_Fall_Landing_1_FX\tBoosterSocket_R\t0\t0 0 0\t16384 0 0\t-\t0\n";

    /// The table's effects by clip, in order; a dash's dust left out (DASH_DUST),
    /// a landing's kept.
    #[test]
    fn reads_the_table_in_clip_order() {
        let t = MoveFx::parse(TEXT);
        assert_eq!(t.length(0, 13), Some(1.2));
        let (_, n) = &t.clips[&(0, 13)];
        assert_eq!(n.len(), 2);
        assert!(n.windows(2).all(|w| w[0].at <= w[1].at));
        assert_eq!(&*n[0].colors[0], "Color");
        assert!(n.iter().all(|n| &*n.system == "CarJump_01_FX"));
        assert!(n[0].attached && n[0].auto_deactivate);
        let titan = crate::character::CLASSES
            .iter()
            .position(|c| *c == "titan")
            .unwrap();
        let (_, land) = &t.clips[&(titan, 10)];
        assert!(!land[0].attached && !land[0].auto_deactivate && land[0].colors.is_empty());
        assert_eq!(&*land[0].system, "Long_Fall_Landing_1_FX");
        // A clip without effects plays none.
        assert_eq!(t.length(0, 99), None);
    }

    #[test]
    fn starts_each_effect_once_as_the_clip_passes_it() {
        let t = MoveFx::parse(TEXT);
        // Frame by frame at 60 a second: each starts in exactly one frame.
        let mut fired = Vec::new();
        let mut at = 0.0;
        while at < 1.2 {
            let to = at + 1.0 / 60.0;
            fired.extend(t.between(0, 13, at, to).map(|n| n.at));
            at = to;
        }
        assert_eq!(fired, vec![0.1235, 0.5164]);
        // The landing's starts on its first frame.
        let titan = crate::character::CLASSES
            .iter()
            .position(|c| *c == "titan")
            .unwrap();
        assert_eq!(t.between(titan, 10, 0.0, 1.0 / 60.0).count(), 1);
    }

    #[test]
    fn places_effects_in_the_sockets_frame() {
        // A socket at (1, 2, 3) lined up with Unreal's axes (in Bevy's: X along x,
        // Y along z, Z up) on a robot drawn at 0.6: 50 uu along its X is 0.3 m
        // along x, and a quarter turn of pitch points the effect's X up.
        let (place, x, _) = on_socket(
            Vec3::new(1.0, 2.0, 3.0),
            [Vec3::X, Vec3::Z, Vec3::Y],
            0.6,
            Vec3::new(50.0, 0.0, 0.0),
            ue::rotator_axes(16384.0, 0.0, 0.0),
        );
        assert!(place.distance(Vec3::new(1.3, 2.0, 3.0)) < 1e-5);
        assert!(x.distance(Vec3::Y) < 1e-4, "{x}");
    }
}
