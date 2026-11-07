//! The players' deaths, as Escalation's soldiers die: every class's robot dies by
//! the soldiers' death set, TR_Deaths_p.Sets.Basic_DEATHSET (its ROBODEF's
//! DeathSet), and falls as their physics asset, TR_Shared_PHYSICS_p.RB_Shared_PHYS
//! (tools/build_deaths.py's models/robot_deaths.*, with the clips the set plays).
//!
//! * **His death**: the set's death for how he was killed (deaths.rs: TnDeathPlayer.
//!   Pick, the kill's death types as TnRobotDeathContextFactory gathers them: the
//!   damage type's, the side he was struck on, a headshot, a gib, in the air), played
//!   out as its timeline has it, as an enemy's is (enemies.rs): its clips cutting in
//!   over what he was doing (main.rs's animate_jazz; killed in the air or down, he
//!   falls as he is), his robot's bones handed over to its bodies (ragdoll.rs) with
//!   the clip's velocities, the speed he was moving at, the kill's momentum and the
//!   death's ragdoll actions, its effects on his sockets, its lights, sounds,
//!   camera shakes, destructibles and gibs, the camera following his body.
//! * **The dissolve**: as his death has it (TnDeresser, paint.wgsl), his decals going
//!   with it; he comes back once his body has gone (its mesh drawn no more), the
//!   respawn's wait at the soonest (main.rs's player_health). (His body starts to
//!   dissolve by DERES_BY at the latest: a death that would lie longer is cut short.)
//!
//! His PC decides how he died and tells the others (net.rs's JazzEvent::Death),
//! whose copies of him die the same way (the ragdoll's own fall as each PC runs it);
//! his sounds come from his PC, as his other sounds do.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

use crate::deaths;
use crate::enemies::{self, Death, DeathWorld, Dissolving, Hit};
use crate::levelfx::FxId;
use crate::net::JazzEvent;
use crate::ragdoll::{PhysicsAsset, Rig, RigState};
use crate::{JazzOf, JointOf, LocalPlayer, Player, RemotePlayer, RobotForm};

/// The players' deaths' data and clips (tools/build_deaths.py).
pub const DEATHS_FILE: &str = "models/robot_deaths.txt";
pub const DEATHS_MODEL: &str = "models/robot_deaths.glb";
/// The players' death set.
const DEATH_SET: &str = "Basic_DEATHSET";

/// The latest his body starts to dissolve, seconds after he died; the dissolve then
/// (the death sets' TnDeresser: its Duration and NormalizedTimeToMakeMeshInvisible).
const DERES_BY: f32 = 4.5;
const DERES_SECS: f32 = 3.0;
const DERES_INVISIBLE: f32 = 0.39685;
const SND_DERES: &str = "BL_DEATH.DERES_DISSOLVE_REMAINS";
/// A copy of another player's Jazz dead this long without word of how he died
/// falls as a plain ragdoll.
const NEWS_WAIT: f32 = 0.3;
/// His body falling faster than this (metres a second) and then stopping is it
/// hitting the floor.
const LANDING_SPEED: f32 = 2.5;
/// How quickly the camera follows his body (a second's share of the way).
const VIEW_RATE: f32 = 6.0;
/// The most of his own speed his body carries as it falls (metres a second).
const MAX_CARRY: f32 = 12.0;
/// His robot's mass (Pawn's Mass: his death's TearOffMomentum is the blow's
/// momentum over it).
const MASS: f32 = 100.0;
/// TnDeathContextFactory's StationarySpeed (metres a second).
const STATIONARY_SPEED: f32 = 0.5;

/// The players' death set's clips and physics asset (robot_deaths.txt).
#[derive(Resource)]
pub struct PlayerDeaths {
    pub asset: Arc<PhysicsAsset>,
    /// The clips by name (the death set's AnimName: Death_Gunfire_Front, a chooser
    /// group of them...): each one's number in robot_deaths.glb and its length
    /// (seconds).
    pub clips: HashMap<String, Vec<(usize, f32)>>,
    /// How many clips robot_deaths.glb has.
    pub count: usize,
}

impl PlayerDeaths {
    /// The players' deaths from the assets folder (none without both files).
    pub fn load(assets: &std::path::Path) -> Option<PlayerDeaths> {
        if !assets.join(DEATHS_MODEL).exists() {
            return None;
        }
        let text = std::fs::read_to_string(assets.join(DEATHS_FILE)).ok()?;
        Some(Self::read(&text)).filter(|d| d.count > 0 && !d.asset.bodies.is_empty())
    }

    fn read(text: &str) -> PlayerDeaths {
        let mut asset = PhysicsAsset::default();
        let mut clips: HashMap<String, Vec<(usize, f32)>> = HashMap::new();
        let mut count = 0;
        for line in text.lines().filter(|l| !l.starts_with('#')) {
            let f: Vec<&str> = line.split('\t').collect();
            match f[0] {
                "clip" if f.len() >= 4 => {
                    let (Ok(index), Ok(secs)) =
                        (f[2].trim().parse::<usize>(), f[3].trim().parse::<f32>())
                    else {
                        continue;
                    };
                    clips
                        .entry(f[1].to_string())
                        .or_default()
                        .push((index, secs));
                    count = count.max(index + 1);
                }
                "body" if f.len() >= 2 => {
                    asset.read_body(&f[1..]);
                }
                "mass" if f.len() >= 3 => {
                    asset.read_mass(&f[1..]);
                }
                "joint" if f.len() >= 2 => {
                    asset.read_joint(&f[1..]);
                }
                _ => {}
            }
        }
        PlayerDeaths {
            asset: Arc::new(asset),
            clips,
            count,
        }
    }
}

/// How a player died, as his PC tells the others (JazzEvent::Death): the blow
/// that killed him (its momentum with it), whether it struck his head, whether he
/// was in the air, and the number his death's choices are made from.
#[derive(Clone, Copy, Debug, Default)]
pub struct DeathNews {
    pub hit: Hit,
    pub head: bool,
    pub airborne: bool,
    pub seed: u32,
}

impl DeathNews {
    /// From the event (None for another kind of event).
    pub fn of_event(event: &JazzEvent) -> Option<DeathNews> {
        match *event {
            JazzEvent::Death {
                death,
                flags,
                seed,
                at,
                dir,
            } => Some(DeathNews {
                // (Its `dir` is the blow's momentum: its way, and its size.)
                hit: Hit {
                    gib: flags & 4 != 0,
                    ..Hit::new(Death::from_index(death), at, dir).with_momentum_vec(dir)
                },
                head: flags & 1 != 0,
                airborne: flags & 2 != 0,
                seed,
            }),
            _ => None,
        }
    }

    fn event(&self) -> JazzEvent {
        JazzEvent::Death {
            death: self.hit.death.index(),
            flags: u8::from(self.head)
                | (u8::from(self.airborne) << 1)
                | (u8::from(self.hit.gib) << 2),
            seed: self.seed,
            at: self.hit.at,
            dir: self.hit.momentum,
        }
    }
}

/// A player who has died, as his death plays out (until he comes back).
#[derive(Component)]
pub struct Downed {
    /// His robot's death (none without its ragdoll: the old death pose).
    fall: Option<Fall>,
    /// Seconds since he died.
    age: f32,
}

/// A player's robot dying: his death's timeline and where it plays out, his robot
/// (its Rig), the choices it makes, its meshes dissolving (and since when, if his
/// death was cut short: DERES_BY), its bones as he fell (put back as he comes
/// back), his decals, its light, the effects held on it, the bones it scales, and
/// its body's middle last frame and how fast it was falling (for the sound of it
/// landing).
struct Fall {
    timeline: deaths::Timeline,
    place: deaths::Place,
    robot: Entity,
    pick: usize,
    seed: u32,
    /// No clips (killed in the air or down: he falls as he is).
    no_clips: bool,
    fell: bool,
    dissolving: Vec<Dissolving>,
    forced: Option<f32>,
    /// His decals (decals.rs), hidden as the dissolve reaches them.
    decals: Vec<Entity>,
    pose: Vec<Transform>,
    light: Option<Entity>,
    effects: Vec<(FxId, Arc<str>)>,
    bones: Vec<(Entity, f32)>,
    hidden: bool,
    last: Option<Vec3>,
    falling: f32,
    /// His dissolve's overlay's reach (metres).
    reach: f32,
    /// The dissolve's value and its middle as last written to his materials.
    deres_drawn: Option<(f32, Vec3)>,
    /// His meshes have been given the dissolve's materials (`dissolving`).
    dissolve_begun: bool,
}

impl Fall {
    /// How far his dissolve has come (its deresser's value) and where his mesh is
    /// drawn no more, if it has begun.
    fn deres(&self, age: f32) -> Option<(f32, f32)> {
        deres_of(self.timeline.deres(), self.forced, age)
    }
}

/// A dissolve's value and where its mesh is drawn no more: the death's own
/// (`timeline`), or once it was cut short at DERES_BY (`forced`, since then), that
/// one's, whatever the death does after (a tank shell's deaths start their own at
/// 5.6 to 6.6 s, Electrocute01_DEATH at 5.9, Gravity_DEATH at 8.1, MeleeCrush_DEATH
/// at 12).
fn deres_of(timeline: Option<(f32, f32)>, forced: Option<f32>, age: f32) -> Option<(f32, f32)> {
    match forced {
        Some(from) => Some((((age - from) / DERES_SECS).clamp(0.0, 1.0), DERES_INVISIBLE)),
        None => timeline,
    }
}

/// Whether his meshes start to dissolve now: as his death's own dissolve starts
/// (`by_death`) or it is cut short (`forced_now`), the first time only. (Taken
/// again, his meshes' materials would be the dissolve's, and those are what he
/// would come back in: the overlay's glowing lines, his body gone.)
fn dissolve_begins(begun: bool, by_death: bool, forced_now: bool) -> bool {
    !begun && (by_death || forced_now)
}

impl Downed {
    /// Whether his body has gone (its death over, or its mesh dissolved away): he
    /// may come back.
    pub fn gone(&self) -> bool {
        self.fall
            .as_ref()
            .is_none_or(|f| f.timeline.gone() || f.deres(self.age).is_some_and(|(v, inv)| v >= inv))
    }
}

/// Each Jazz's robot gets its rig once its skeleton is in (its model's root and
/// the bones his parts are skinned to: main.rs's JointOf), and a new one when its
/// model's scene has been put in afresh (Bevy does that when robot.glb is loaded
/// again, as it can be while the game starts: character.rs's redress_respawned),
/// whose bones are new.
pub(crate) fn rig_robots(
    mut commands: Commands,
    deaths: Option<Res<PlayerDeaths>>,
    robots: Query<(Entity, Option<&Rig>), With<RobotForm>>,
    children: Query<&Children>,
    names: Query<&Name>,
    joints: Query<(), With<JointOf>>,
) {
    let Some(deaths) = deaths else {
        return;
    };
    for (robot, rig) in &robots {
        if rig.is_some_and(|r| names.contains(r.base)) {
            continue;
        }
        let Some(root) = children
            .iter_descendants(robot)
            .find(|e| names.get(*e).is_ok_and(|n| n.as_str() == "Root"))
        else {
            continue;
        };
        if !children.iter_descendants(root).any(|e| joints.contains(e)) {
            continue;
        }
        let rig = Rig::new(deaths.asset.clone(), root, &children, &names, |e| {
            joints.contains(e)
        });
        if deaths
            .asset
            .bodies
            .iter()
            .any(|b| rig.names.contains(&b.bone))
        {
            commands.entity(robot).insert(rig);
        }
    }
}

/// Whether `at` is nearest his head of his bodies (where his bones are now).
fn head_at(rig: &Rig, globals: &Query<&GlobalTransform>, at: Vec3) -> bool {
    let mut best: Option<(f32, bool)> = None;
    for body in &rig.asset.bodies {
        let Some(g) = rig
            .names
            .iter()
            .position(|n| *n == body.bone)
            .and_then(|i| globals.get(rig.bones[i]).ok())
        else {
            continue;
        };
        let head = body.bone.contains("Head");
        for s in &body.shapes {
            let d = s.placed(&g.affine()).reach(at);
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, head));
            }
        }
    }
    best.is_some_and(|(_, head)| head)
}

/// Whether a dissolve at `value` round `centre` is to be written to its materials:
/// the first time, and whenever its value has moved on or its body moved (more than
/// a millimetre) since `drawn`, what was last written. Once dissolved, a body at
/// rest writes nothing.
fn dissolve_moved(drawn: Option<(f32, Vec3)>, value: f32, centre: Vec3) -> bool {
    drawn.is_none_or(|(v, at)| v != value || at.distance_squared(centre) > 1e-6)
}

/// The speed his whole body was moving at, up to MAX_CARRY (a car's top speed would
/// throw him far out of it).
fn carried(player: &Player) -> Vec3 {
    (player.velocity + Vec3::Y * player.vertical_velocity).clamp_length_max(MAX_CARRY)
}

/// The players who have just died start their deaths: this PC's (and its test
/// players') as their death set has it for how they were killed, told to the other
/// PCs; the other players' copies as their PCs tell (or, without word, as a plain
/// ragdoll). Without his robot's rig, the old death pose.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn start_deaths(
    mut commands: Commands,
    time: Res<Time>,
    deaths: Option<Res<PlayerDeaths>>,
    data: Res<deaths::DeathData>,
    mut sfx: ResMut<crate::Sfx>,
    mut players: Query<
        (
            Entity,
            &Transform,
            &mut Player,
            Has<RemotePlayer>,
            Has<LocalPlayer>,
        ),
        Without<Downed>,
    >,
    mut robots: Query<(Entity, &JazzOf, &mut Rig), With<RobotForm>>,
    bones: Query<&Transform, Without<Player>>,
    globals: Query<&GlobalTransform>,
    mut waiting: Local<HashMap<Entity, f32>>,
) {
    let dt = time.delta_secs();
    for (e, transform, mut player, remote, local) in &mut players {
        let news = player.death_news.take();
        let dead = player.dead_left > 0.0 || (remote && player.dying);
        if news.is_none() && !dead {
            waiting.remove(&e);
            continue;
        }
        if remote && news.is_none() {
            // (Waiting a moment for word of how he died.)
            let waited = waiting.entry(e).or_insert(0.0);
            *waited += dt;
            if *waited < NEWS_WAIT {
                continue;
            }
        }
        waiting.remove(&e);
        let rig = robots
            .iter_mut()
            .find(|(_, of, rig)| of.0 == e && !rig.bones.is_empty());
        let (Some(_), Some((robot, _, mut rig))) = (deaths.as_ref(), rig) else {
            if !remote {
                crate::start_action(&mut player, crate::ANIM_DEATH, crate::RESPAWN_SECS, 1.0);
            }
            commands.entity(e).insert(Downed {
                fall: None,
                age: 0.0,
            });
            continue;
        };
        // (Killed down, he goes limp as he lies, as in the air: revive.rs.)
        let airborne = !player.grounded || player.revive.died_downed;
        let news = match news {
            Some(n) => n,
            None if remote => DeathNews {
                airborne,
                seed: e.to_bits() as u32,
                ..default()
            },
            None => {
                let hit = player.last_hit;
                let head = hit.death.is_gunshot() && head_at(&rig, &globals, hit.at);
                let seed =
                    time.elapsed_secs().to_bits() ^ (e.to_bits() as u32).wrapping_mul(0x9e37_79b9);
                let n = DeathNews {
                    hit,
                    head,
                    airborne,
                    seed,
                };
                player.played.push(n.event());
                n
            }
        };
        // His death set's death for it (TnRobotDeathContextFactory's death types).
        let centre = player.robot_centre(transform.translation);
        let velocity = carried(&player);
        let kill = deaths::Kill {
            damage: news.hit.death.types(),
            gib: news.hit.gib,
            side: deaths::side_struck(*transform.forward(), news.hit.at - centre),
            stationary: velocity.length() <= STATIONARY_SPEED,
            robot: true,
            vehicle_boost: None,
            downed: player.revive.died_downed,
            head: news.head,
            hovering: false,
            airborne: news.airborne,
            ceiling: false,
            factory: deaths::Factory::Robot,
        };
        let mut seed = news.seed.max(1);
        let roll = deaths::rand(&mut seed);
        let def = data
            .pick(DEATH_SET, kill.types(), roll)
            .unwrap_or_else(|| Arc::new(deaths::DeathDef::default()));
        let pose = rig
            .bones
            .iter()
            .map(|b| bones.get(*b).copied().unwrap_or_default())
            .collect();
        rig.reset();
        player.action = None;
        player.death_clip = None;
        // The HUD's power-down (his own; the others hear it from his PC).
        if !remote {
            sfx.play_from(crate::SND_DEATH, crate::voice_at(local, centre));
        }
        let pick = (deaths::rand(&mut seed) * 65536.0) as usize;
        commands.entity(e).insert(Downed {
            fall: Some(Fall {
                timeline: deaths::Timeline::new(def, seed),
                place: deaths::Place {
                    at: centre,
                    turn: transform.rotation,
                    hit: news.hit.at,
                    momentum: news.hit.momentum / MASS / deaths::UU,
                    velocity,
                },
                robot,
                pick,
                seed,
                no_clips: news.airborne,
                fell: false,
                dissolving: Vec::new(),
                forced: None,
                decals: Vec::new(),
                pose,
                light: None,
                effects: Vec::new(),
                bones: Vec::new(),
                hidden: false,
                last: None,
                falling: 0.0,
                reach: 1.0,
                deres_drawn: None,
                dissolve_begun: false,
            }),
            age: 0.0,
        });
    }
}

/// His socket `name` on his robot (else his robot's bone of that name): its place
/// and its X and Y (Unreal's), in the world.
fn frame_of(
    sockets: &mut crate::abilities::Sockets,
    jazz: Entity,
    name: &str,
) -> Option<(Vec3, Vec3, Vec3)> {
    if let Some(f) = sockets.frame(jazz, name) {
        return Some(f);
    }
    let bone = sockets.bone(jazz, name)?;
    let g = sockets.global(bone)?;
    let r = g.rotation();
    Some((g.translation(), r * Vec3::X, r * Vec3::Z))
}

/// The downed play out their deaths (TnDeathPlayer.Tick: deaths.rs's Timeline), each
/// frame's beat done to their robots as an enemy's is (enemies.rs's play_deaths):
/// its clips, the ragdoll taking over and kicked, its sounds (this PC's players':
/// the others' come from their PCs), effects, camera shakes, destructibles, lights,
/// dissolve, gravity, spin and hidden meshes; the camera follows the body.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn play_deaths(
    mut commands: Commands,
    chars: Res<crate::character::Characters>,
    deaths_res: Option<Res<PlayerDeaths>>,
    mut players: Query<(
        Entity,
        &Transform,
        &mut Player,
        &mut Downed,
        Has<RemotePlayer>,
        Has<LocalPlayer>,
    )>,
    mut rigs: Query<&mut Rig>,
    children: Query<&Children>,
    mesh_materials: enemies::MeshMaterials,
    mut w: DeathWorld,
    mut decals: Query<(&GlobalTransform, &mut Visibility), With<crate::decals::Decal>>,
    mut shown: Query<&mut Visibility, Without<crate::decals::Decal>>,
    mut sockets: crate::abilities::Sockets,
) {
    let dt = w.time.delta_secs();
    let mut shakes: Vec<(deaths::ShakeDo, Vec3)> = Vec::new();
    for (e, transform, mut player, mut downed, remote, local) in &mut players {
        let downed = &mut *downed;
        downed.age += dt;
        let age = downed.age;
        // (Word of his death come late, after his copy fell without it, is old.)
        player.death_news = None;
        let Some(fall) = downed.fall.as_mut() else {
            continue;
        };
        let Ok(mut rig) = rigs.get_mut(fall.robot) else {
            continue;
        };
        let centre = rig
            .body_centre(&w.globals)
            .unwrap_or_else(|| player.robot_centre(transform.translation));
        let energon = player.look.energon();
        let voice = crate::voice_at(local, centre);
        let reach = (player.kit.robot.height * 0.5).max(player.kit.robot.radius);
        // His clips (none in the air: he falls as he is).
        let pick = fall.pick;
        let clips = deaths_res.as_deref();
        let no_clips = fall.no_clips;
        let clip_of = |name: &str| {
            clips
                .filter(|_| !no_clips)
                .and_then(|d| d.clips.get(name))
                .filter(|l| !l.is_empty())
                .map(|l| l[pick % l.len()])
        };
        let length = |name: &str, rate: f32| clip_of(name).map(|(_, secs)| secs / rate.max(0.05));
        let env = deaths::Env {
            clip: &length,
            collided: rig.touched,
        };
        let beat = fall.timeline.step(dt, &env);
        if let Some((name, rate, _, _)) = &beat.anim
            && let Some((index, _)) = clip_of(name)
        {
            player.start_death_clip = Some((index, *rate));
            player.death_clip = Some(index);
        }
        if let Some(rd) = &beat.ragdoll {
            let carry = if fall.fell {
                Vec3::ZERO
            } else {
                fall.place.velocity
            };
            // (A player's round names no bone here: the root body's.)
            let kick = deaths::kick(rd, &fall.place, None, carry, &mut fall.seed);
            if matches!(rig.state, RigState::Animated) {
                rig.state = RigState::Falling(kick);
            } else {
                rig.pending = Some(kick);
            }
            fall.fell = true;
            player.limp = true;
            player.death_clip = None;
        }
        if !remote {
            // (His death's own dissolve starting after his body has dissolved, cut
            // short, makes no sound of its own.)
            let late_deres = fall.dissolve_begun && beat.deres_start.is_some();
            for cue in &beat.sounds {
                if !(late_deres && *cue == SND_DERES) {
                    w.sfx.play_from(cue, voice);
                }
            }
        }
        for (system, socket) in &beat.effects {
            let Some(fx) = w.fx.as_mut() else { break };
            let (at, x, y) = frame_of(&mut sockets, e, socket).unwrap_or((
                centre,
                fall.place.dir(Vec3::X),
                fall.place.dir(Vec3::Y),
            ));
            if let Some(id) = fx.spawn(system, at, x, &[]) {
                fx.place_turned(id, at, x, y);
                fall.effects.push((id, socket.clone()));
            }
        }
        // The trails of the pieces his ragdoll cut away.
        for (bone, trail) in std::mem::take(&mut rig.severed) {
            let (Some(fx), Some(name)) = (w.fx.as_mut(), rig.names.get(bone)) else {
                continue;
            };
            if let Some((at, x, y)) = frame_of(&mut sockets, e, name)
                && let Some(id) = fx.spawn(&trail, at, x, &[])
            {
                fx.place_turned(id, at, x, y);
                fall.effects.push((id, Arc::from(name.as_str())));
            }
        }
        for s in &beat.shakes {
            shakes.push((*s, centre));
        }
        for name in &beat.destructibles {
            if let Some(dsys) = w.data.destructibles.get(&**name).cloned() {
                let place = deaths::Place {
                    at: centre,
                    ..fall.place
                };
                crate::gibs::set_off(
                    &dsys,
                    &place,
                    &mut fall.seed,
                    !remote,
                    &mut w.fx,
                    &mut w.sfx,
                    &mut w.gibs,
                );
            }
        }
        // His dissolve: as his death has it, or cut short by DERES_BY.
        let forced_now =
            fall.timeline.deres().is_none() && fall.forced.is_none() && age >= DERES_BY;
        if forced_now {
            fall.forced = Some(age);
            if !remote {
                w.sfx.play_from(SND_DERES, voice);
            }
        }
        if dissolve_begins(fall.dissolve_begun, beat.deres_start.is_some(), forced_now) {
            fall.dissolve_begun = true;
            let start = beat.deres_start.clone().unwrap_or_default();
            let invisible = fall.deres(age).map_or(DERES_INVISIBLE, |(_, inv)| inv);
            let meshes: Vec<Entity> = children
                .iter_descendants(fall.robot)
                .filter(|m| {
                    mesh_materials
                        .get(*m)
                        .is_ok_and(|(p, s)| p.is_some() || s.is_some())
                })
                .collect();
            fall.dissolving = enemies::dissolve_meshes(
                &mut commands,
                &meshes,
                &mesh_materials,
                (&mut w.paints, &mut w.cache, &w.standard),
                w.overlay.as_deref(),
                &start,
                invisible,
                (centre, energon, reach),
            );
            fall.reach = enemies::overlay_reach(reach, start.radius);
            fall.decals = children
                .iter_descendants(fall.robot)
                .filter(|e| decals.contains(*e))
                .collect();
        }
        if let Some((value, invisible)) = fall.deres(age) {
            // (Written only as it changes: each material changed is prepared for
            // the GPU afresh, and his dissolved body lies there till the round
            // ends, every dead player's on every PC.)
            if dissolve_moved(fall.deres_drawn, value, centre) {
                enemies::dissolve_to(&mut w.paints, &fall.dissolving, centre, value);
                fall.deres_drawn = Some((value, centre));
            }
            // (His decals go as the overlay's front passes them, and all of them
            // once his mesh is drawn no more.)
            let front = if value > invisible {
                f32::MAX
            } else {
                enemies::dissolve_front(value, fall.reach)
            };
            crate::decals::hide_within(
                fall.decals.iter().copied(),
                &mut decals,
                Some((centre, front)),
            );
        }
        enemies::death_light(
            &mut commands,
            &mut fall.light,
            beat.light,
            centre,
            &fall.place,
            &mut w.lights,
        );
        if let Some(g) = beat.gravity {
            rig.gravity = g;
        }
        rig.spin = -fall.place.dir(beat.spin);
        if beat.hide
            && !fall.hidden
            && let Ok(mut v) = shown.get_mut(fall.robot)
        {
            *v = Visibility::Hidden;
            fall.hidden = true;
        }
        for (node, scale) in &beat.bones {
            if &**node == "Headshot"
                && let Some(b) = enemies::HEAD_BONES.iter().find_map(|h| rig.bone(h))
            {
                fall.bones.push((b, *scale));
            }
        }
        // His held effects carried with their sockets and bones.
        if let Some(fx) = w.fx.as_mut() {
            fall.effects.retain(|(id, socket)| {
                if !fx.alive(*id) {
                    return false;
                }
                if let Some((at, x, y)) = frame_of(&mut sockets, e, socket) {
                    fx.place_turned(*id, at, x, y);
                }
                true
            });
        }
        // His body hitting the floor (falling, then stopped).
        if let Some(last) = fall.last
            && dt > 0.0
        {
            let down = (last.y - centre.y) / dt;
            if fall.falling > LANDING_SPEED && down < fall.falling * 0.25 && !remote {
                // (A big robot's falls heavily, as its death clip's would.)
                let cue = if player.kit.robot.height > 4.0 {
                    chars.robot_sound(
                        player.kit.class,
                        crate::character::SOUND_IMPACT_HEAVY,
                        crate::SND_BODYFALL,
                    )
                } else {
                    crate::SND_BODYFALL
                };
                w.sfx.play_from(cue, voice);
                fall.falling = 0.0;
            } else {
                fall.falling = down.max(0.0);
            }
        }
        fall.last = Some(centre);
        // The camera looks at his body (main.rs's camera_follow: as if he stood
        // where it lies).
        let feet = centre - Vec3::Y * (player.kit.robot.height * 0.5);
        player.death_view = Some(match player.death_view {
            Some(v) => v.lerp(feet, (dt * VIEW_RATE).min(1.0)),
            None => feet,
        });
    }
    // The deaths' camera shakes, for the players at this PC near them.
    for (s, at) in shakes {
        for (_, t, mut p, _, _, local) in &mut players {
            if local && t.translation.distance(at) <= s.radius / deaths::UU {
                p.screen_shakes.push(crate::riot::ScreenShake {
                    radius: s.radius / deaths::UU,
                    secs: s.secs,
                    rot_amp: s.rot,
                    rot_freq: s.rot_freq,
                    loc_amp: s.loc,
                    loc_freq: s.loc_freq,
                    fov_amp: s.fov.0,
                    fov_freq: s.fov.1,
                });
            }
        }
    }
}

/// The bones the downed's deaths scale (a headshot's head shrunk away), held so after
/// the clips have posed them, before the ragdolls pose theirs.
pub(crate) fn scale_bones(downed: Query<&Downed>, mut transforms: Query<&mut Transform>) {
    for d in &downed {
        for (bone, scale) in d.fall.iter().flat_map(|f| f.bones.iter()) {
            if let Ok(mut t) = transforms.get_mut(*bone)
                && t.scale != Vec3::splat(*scale)
            {
                t.scale = Vec3::splat(*scale);
            }
        }
    }
}

/// The downed who have come back (main.rs's player_health; another player's copy
/// as his PC says): his robot whole again, posed by its clips, its death's light,
/// effects and hidden meshes gone.
#[allow(clippy::type_complexity)]
pub(crate) fn end_deaths(
    mut commands: Commands,
    mut players: Query<(Entity, &mut Player, &mut Downed, Has<RemotePlayer>)>,
    mut rigs: Query<&mut Rig>,
    mut bones: Query<&mut Transform, Without<Player>>,
    mut decals: Query<(&GlobalTransform, &mut Visibility), With<crate::decals::Decal>>,
    mut shown: Query<&mut Visibility, Without<crate::decals::Decal>>,
    mut fx: Option<ResMut<crate::levelfx::FxSystems>>,
) {
    for (e, mut player, mut downed, remote) in &mut players {
        // (A copy's death isn't cut short by word of his life from before it.)
        let back = player.dead_left <= 0.0 && !player.dying && (!remote || downed.age > 0.5);
        if !back {
            continue;
        }
        if let Some(fall) = downed.fall.take() {
            enemies::undissolve(&mut commands, fall.dissolving);
            crate::decals::hide_within(fall.decals.iter().copied(), &mut decals, None);
            if let Ok(mut rig) = rigs.get_mut(fall.robot) {
                rig.reset();
                for (b, t) in rig.bones.iter().zip(&fall.pose) {
                    if let Ok(mut tr) = bones.get_mut(*b) {
                        *tr = *t;
                    }
                }
            }
            if let Some(l) = fall.light {
                commands.entity(l).try_despawn();
            }
            if let Some(fx) = fx.as_mut() {
                for (id, _) in fall.effects {
                    fx.stop(id);
                }
            }
            if fall.hidden
                && let Ok(mut v) = shown.get_mut(fall.robot)
            {
                *v = Visibility::Inherited;
            }
        }
        player.limp = false;
        player.death_clip = None;
        player.start_death_clip = None;
        player.death_view = None;
        commands.entity(e).remove::<Downed>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_deaths() {
        let path = crate::assets_dir().join(DEATHS_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("{} not built: skipped", path.display());
            return;
        };
        let d = PlayerDeaths::read(&text);
        assert_eq!(d.count, 23);
        assert_eq!(d.clips["Death_Gunfire_Front"].len(), 16);
        for name in [
            "Death_Headshot03",
            "Death_Electrocute",
            "Death_ProjectileMult_Stagger_F",
            "Death_ProjectileSing_Reaching",
            "Death_BlowBack",
            "Death_EnergyRanged_B_01",
            "Death_EnergyRanged_B_02",
        ] {
            assert_eq!(d.clips[name].len(), 1, "{name}");
        }
        assert_eq!(d.asset.bodies.len(), 18);
        assert_eq!(d.asset.joints.len(), 17);
        // (Its bodies' masses: RB_Shared_PHYS's.)
        let hips = d
            .asset
            .bodies
            .iter()
            .find(|b| b.bone == "C_Spine00_Hips_XB")
            .unwrap();
        assert_eq!(hips.mass, Some(7.5));
    }

    #[test]
    fn deaths_go_online_as_they_were() {
        let news = DeathNews {
            hit: Hit {
                gib: true,
                ..Hit::new(Death::Shockwave, Vec3::new(1.0, 2.0, 3.0), Vec3::X)
                    .with_momentum(2000.0)
            },
            head: true,
            airborne: false,
            seed: 0xdead_beef,
        };
        let back = DeathNews::of_event(&news.event()).unwrap();
        assert_eq!(back.hit.death, Death::Shockwave);
        assert_eq!(back.hit.at, news.hit.at);
        assert_eq!(back.hit.dir, Vec3::X);
        assert_eq!(back.hit.momentum, Vec3::X * 2000.0);
        assert!(back.head && !back.airborne && back.hit.gib);
        assert_eq!(back.seed, 0xdead_beef);
        for i in 0..12 {
            assert_eq!(Death::from_index(i).index(), i);
        }
        assert_eq!(Death::from_index(200), Death::Plain);
    }

    /// A dead player's dissolve is written to his materials as it goes on, and not
    /// again once it has run its course with his body at rest (each write has every
    /// one of his materials prepared for the GPU again: before, a body lying
    /// dissolved cost every PC about 15 ms a frame, more with each player dead).
    #[test]
    fn a_dissolved_body_writes_its_materials_no_more() {
        let at = Vec3::new(4.0, 0.5, -2.0);
        // (Beginning, and going on.)
        assert!(dissolve_moved(None, 0.0, at));
        assert!(dissolve_moved(Some((0.4, at)), 0.45, at));
        // (Its body still falling as it dissolves: the overlay follows it.)
        assert!(dissolve_moved(Some((0.5, at)), 0.5, at + Vec3::Y * 0.01));
        // (Done and at rest: nothing more, frame after frame.)
        let mut drawn = Some((1.0, at));
        let mut writes = 0;
        for _ in 0..600 {
            if dissolve_moved(drawn, 1.0, at + Vec3::X * 1e-5) {
                writes += 1;
                drawn = Some((1.0, at));
            }
        }
        assert_eq!(writes, 0);
    }

    /// Killed by a tank's shell (or electrocuted, crushed or by gravity), a player's
    /// death starts its own dissolve after DERES_BY, when his body has been
    /// dissolving since: it dissolves once, on and on to the end, and his meshes are
    /// given the dissolve's materials only once (taken again, they came back in the
    /// dissolve's materials: the overlay's glowing lines alone).
    #[test]
    fn a_death_cut_short_dissolves_once() {
        let data = deaths::DeathData::load(&crate::assets_dir());
        if data.sets.is_empty() {
            return;
        }
        let kill = deaths::Kill {
            damage: Death::Tank.types(),
            side: deaths::Types::of("HitFromFront"),
            robot: true,
            ..Default::default()
        };
        let def = data.pick(DEATH_SET, kill.types(), 0.9).unwrap();
        assert!(
            deaths::short(&def.name).starts_with("ExplosiveTank"),
            "{}",
            def.name
        );
        let mut t = deaths::Timeline::new(def, 1);
        let lookup = |_: &str, _: f32| Some(1.0);
        let (mut forced, mut begun, mut begins, mut own_at) = (None, false, 0, None);
        let mut last = 0.0f32;
        for i in 1..=1500 {
            let age = i as f32 * 0.01;
            let env = deaths::Env {
                clip: &lookup,
                collided: age > 1.0,
            };
            let beat = t.step(0.01, &env);
            if beat.deres_start.is_some() {
                own_at = Some(age);
            }
            // (As play_deaths has it.)
            let forced_now = t.deres().is_none() && forced.is_none() && age >= DERES_BY;
            if forced_now {
                forced = Some(age);
            }
            if dissolve_begins(begun, beat.deres_start.is_some(), forced_now) {
                begun = true;
                begins += 1;
            }
            if let Some((v, _)) = deres_of(t.deres(), forced, age) {
                assert!(v >= last, "at {age}: {v} after {last}");
                last = v;
            }
        }
        assert!(own_at.is_some_and(|at| at > DERES_BY), "{own_at:?}");
        assert_eq!(begins, 1);
        assert_eq!(last, 1.0);
        // (A death that starts its own in time dissolves by it.)
        assert_eq!(deres_of(Some((0.3, 0.4)), None, 2.0), Some((0.3, 0.4)));
    }

    /// A player's death from his death set: a gunfire kill plays its clip and falls
    /// 0.1 s before it ends; his body has gone (and he may come back) once its
    /// dissolve has taken its mesh, by DERES_BY and its dissolve at the latest.
    #[test]
    fn a_players_body_dissolves_in_time() {
        let data = deaths::DeathData::load(&crate::assets_dir());
        if data.sets.is_empty() {
            return;
        }
        let kill = deaths::Kill {
            damage: Death::Gunfire.types(),
            side: deaths::Types::of("HitFromFront"),
            robot: true,
            ..Default::default()
        };
        let def = data.pick(DEATH_SET, kill.types(), 0.5).unwrap();
        assert_eq!(deaths::short(&def.name), "Gunfire_Front_DEATH");
        let mut t = deaths::Timeline::new(def, 1);
        let lookup = |_: &str, _: f32| Some(1.0);
        let env = deaths::Env {
            clip: &lookup,
            collided: false,
        };
        let mut fell = None;
        for i in 0..400 {
            let beat = t.step(0.01, &env);
            if beat.ragdoll.is_some() && fell.is_none() {
                fell = Some(i as f32 * 0.01);
            }
        }
        assert!((fell.unwrap() - 0.9).abs() < 0.03, "{fell:?}");
    }
}
