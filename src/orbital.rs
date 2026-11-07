//! The Orbital Pistol: the game's TnWeaponTracerDart ("TracerDart" inside, the
//! Scientists' gun in multiplayer, built on the campaign's SysCorruptor), as
//! WEP_TracerDart_p, TransGame.xxx and its multiplayer maps have it. Its weapon data
//! (TracerDart_MP_WEPDATA) is main.rs's WEAPONS entry; this module is what it does
//! beyond an ordinary gun.
//!
//! * **Marks** (TnWeaponTracerDart.ProcessInstantHit): a round that strikes an enemy
//!   does its damage first (TnWeapon.ProcessInstantHit), then marks it: an enemy
//!   with no mark gets TnBuffHardLocked, a marked one the next (TnBuffHardLocked2,
//!   then TnBuffHardLocked3, which further hits renew), the old mark taken off and
//!   the new one put on afresh for its 10 s (BuffTime, multiplayer's). While marked
//!   it takes 1.1, 1.2 or 1.3 times the damage from everything that hurts it (the
//!   marks' FloatModifier: "decreases their armor"), and the mark shows on it:
//!   bright orange arrows closing in on it from either side, over and over, one a
//!   side for the first mark, two for the second, three for the third
//!   (OrbitalPistol_Marker1_FX, Marker2_FX, Marker3_FX: the marks' ApplyFx on its
//!   CenterOfMass socket), turned to face whoever looks at it
//!   (TnBuffHardLockedBase.ClientTickBuff). A mark is the Jazz's whose round put it
//!   on, and ends early if he dies (TnBuffHardLockedBase.GetBuffState).
//! * **Revealing** (TnBuffRevealPositionToEnemy, put on with every mark, a buff of
//!   its own that runs its time even if the marker dies): for 8 s the marked
//!   enemy's marker shows on the HUD of every player on the marker's team,
//!   wherever it is, walls or no
//!   (TnObjectiveMarkerTypeTransformerVersus.ShouldDisplayEnemyMarker): the HUD's
//!   mc_markerPawn in the enemy colour, with its mark's icon (BuffIconHardLocked,
//!   2 or 3) over the arrow (hud.rs).
//! * **Its effects** (TracerDart_WEPMESH): the muzzle flash (TracerDartMuzzle_FX,
//!   on the muzzle as it plays), the tracer - a beam from the muzzle to where the
//!   round struck, there at once and gone in a fifth of a second
//!   (TracerDart_TracerBeam_FX, its end set by SetTracerDistance) - and the impact
//!   (TracerDart_Exp_FX, along the surface's normal), all the game's particle
//!   systems, which levelfx.rs runs; and while the gun is ready in Jazz's hand,
//!   the wisps of dark steam it lets off from its ShellEject socket
//!   (TracerDart_Idle_FX, from ActiveStart to ActiveStop: TnWeapon's Active state,
//!   which firing, reloading and putting it away leave).
//!
//! Every PC marks its own sentries as the shots reach it (main.rs's fire and
//! remote_shot), so a mark and its reveal show to all the players.

use bevy::prelude::*;

use crate::levelfx::{FxId, FxSystems};
use crate::{Enemy, Form, GunBone, JazzOf, OrbitCamera, Player, WEAPONS};

/// The game's particle systems a gun plays (fx/weapons_fx.txt, levelfx.rs): its
/// muzzle flash (TnWeaponEffectPlayMuzzleFX), tracer (TnWeaponEffectPlayTracerFX)
/// and impact (TnWeaponEffectPlayImpactFX's DefaultParticleSystem), and an effect
/// it plays on a socket while it is ready (TnWeaponEffectPlayFXOnSocket from
/// ActiveStart to ActiveStop), if any.
pub(crate) struct ShotFx {
    pub muzzle: &'static str,
    pub tracer: &'static str,
    /// When its tracer plays.
    pub tracer_at: TracerAt,
    pub impact: &'static str,
    pub idle: Option<IdleFx>,
}

/// When a gun's tracer plays (its TnWeaponEffectPlayTracerFX's StartEvent and
/// Looping; it reaches to its shot's first impact: CreateTracerForEachImpact is
/// false for every gun that has one of these).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TracerAt {
    /// With every shot (FireShot).
    EveryShot,
    /// With the first shot of a firing only (StartFire: the Energon Harvester's,
    /// once a burst).
    StartFire,
    /// Held on the muzzle from StartFire to StopFire as a beam to where the latest
    /// round struck (Looping: the TechVolt's, arsenal.rs).
    Held,
}

/// An effect a gun plays while it is ready, and its socket on Jazz's elbow, right
/// and left: where (as main.rs's GunSockets, in the elbow bone's space) and its turn
/// (the socket's RelativeRotation: pitch, yaw and roll in Unreal's units; none of
/// its RelativeScale: levelfx's spawn_on_socket).
pub(crate) struct IdleFx {
    pub system: &'static str,
    pub socket: [(Vec3, [i32; 3]); 2],
}

/// The Orbital Pistol's (TracerDart_WEPMESH): the steam on its ShellEject socket
/// (RB_SysCorruptorR_SKEL's and RB_SysCorruptorL_SKEL's).
pub(crate) const ORBITAL_FX: ShotFx = ShotFx {
    muzzle: "TracerDartMuzzle_FX",
    tracer: "TracerDart_TracerBeam_FX",
    tracer_at: TracerAt::EveryShot,
    impact: "TracerDart_Exp_FX",
    idle: Some(IdleFx {
        system: "TracerDart_Idle_FX",
        socket: [
            (Vec3::new(-0.3190, -0.9507, -0.8874), [8192, -16384, 0]),
            (Vec3::new(0.2913, 0.9579, 0.9360), [-7281, 16384, 32768]),
        ],
    }),
};

/// The marks' damage multipliers (TnBuffHardLocked, 2 and 3: FloatModifier, without
/// the Energon Stinger upgrade), how long a mark lasts (BuffTime: multiplayer's) and
/// how long each mark reveals the enemy for (TnBuffRevealPositionToEnemy's BuffTime).
pub(crate) const MARK_DAMAGE: [f32; 3] = [1.1, 1.2, 1.3];
pub(crate) const MARK_SECS: f32 = 10.0;
pub(crate) const REVEAL_SECS: f32 = 8.0;

/// The marks' damage multipliers from a pistol with the Energon Stinger upgrade
/// (TnBuffImprovedHardLock on the weapon: TnWeaponTracerDart.ProcessInstantHit puts
/// each mark on at buff level 1, the marks' second FloatModifier).
pub(crate) const IMPROVED_MARK_DAMAGE: [f32; 3] = [1.15, 1.25, 1.35];

/// The marks' effects (their ApplyFx).
const MARK_FX: [&str; 3] = [
    "OrbitalPistol_Marker1_FX",
    "OrbitalPistol_Marker2_FX",
    "OrbitalPistol_Marker3_FX",
];

/// The effects' EnergonColor: a team's own effects are cyan in a match (as the
/// Riot Cannon's and the EMP grenade's).
const ENERGON: &[(&str, [u8; 4])] = &[("EnergonColor", [0, 255, 255, 255])];

/// A sentry's mark from the Orbital Pistol, if it has one.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Mark {
    /// Which mark (1 to 3: TnBuffHardLocked, 2, 3), 0 for none.
    pub level: u8,
    /// Seconds left of the mark, and of its reveal (a buff of its own).
    pub left: f32,
    pub reveal: f32,
    /// The Jazz whose round put the mark on (its Instigator).
    by: Option<Entity>,
    /// Put on by a pistol with the Energon Stinger upgrade (its buff level 1).
    improved: bool,
    /// Its effect, and the mark it is for.
    fx: Option<(FxId, u8)>,
}

impl Mark {
    /// How much harder what strikes it hits while it is marked.
    pub fn damage_scale(&self) -> f32 {
        let table = if self.improved {
            &IMPROVED_MARK_DAMAGE
        } else {
            &MARK_DAMAGE
        };
        match self.level {
            1..=3 if self.left > 0.0 => table[self.level as usize - 1],
            _ => 1.0,
        }
    }

    /// A round of the Orbital Pistol fired by `by` (his Jazz) has struck it (after
    /// its damage): the next mark (the third renewed), now his, and the reveal, each
    /// for its full time. The pistol's upgrades (`mods`) make the new mark stronger
    /// (Energon Stinger) or longer (Orbital Trace Hack: its time times the buff's
    /// modifier, TnValueTimesProductBuffModifier.ModifyFloatSingle, the mark's
    /// OverrideBuffTime; the reveal keeps its own).
    pub fn hit(&mut self, by: Entity, mods: &crate::upgrades::Mods) {
        self.level = (self.level + 1).min(MARK_DAMAGE.len() as u8);
        self.left = MARK_SECS * mods.mark_secs;
        self.reveal = REVEAL_SECS;
        self.by = Some(by);
        self.improved = mods.improved_marks;
    }

    /// The Jazz whose mark it carries, while it has one (the mark's Instigator).
    pub fn marker(&self) -> Option<Entity> {
        (self.level > 0 && self.left > 0.0)
            .then_some(self.by)
            .flatten()
    }

    /// The enemy shows on the players' HUDs (its reveal, which outlasts a mark that
    /// ends with its marker's death).
    pub fn revealed(&self) -> bool {
        self.reveal > 0.0
    }
}

pub struct OrbitalPlugin;

impl Plugin for OrbitalPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(crate::menu::AppState::Game), clear_marks)
            .add_systems(
                Update,
                (marked_sentries, idle_steam)
                    .after(crate::fire)
                    .before(crate::hud::HudSystems)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

/// A shot's effects: the muzzle flash at the muzzle `origin` along the barrel
/// (`barrel`; kept on the muzzle of `gun`, the Jazz and the weapon in his hand, while
/// it plays), the tracer along `dir` out to `distance` metres, and where it struck
/// (`impact`: the place and the surface's normal there), the impact; the flash and
/// the tracer only as `parts` say (a shotgun's further pellets have neither, a gun
/// that flashes on its own sockets no flash here: arsenal.rs).
#[allow(clippy::too_many_arguments)]
pub(crate) fn shot_fx(
    commands: &mut Commands,
    fx: &mut FxSystems,
    kind: &ShotFx,
    (origin, barrel): (Vec3, Vec3),
    (dir, distance): (Vec3, f32),
    impact: Option<(Vec3, Vec3)>,
    gun: Option<(Entity, usize)>,
    (flash, tracer): (bool, bool),
) {
    if flash {
        crate::riot::flash_on_muzzle(commands, fx, kind.muzzle, origin, barrel, gun);
    }
    if tracer {
        fx.spawn_tracer(kind.tracer, origin, dir, distance);
    }
    if let Some((at, normal)) = impact {
        fx.spawn(kind.impact, at, normal, ENERGON);
    }
}

/// UE3's FRotationMatrix's X and Y for a rotator (Unreal's units), in a bone space
/// as main.rs's GunSockets have it (Unreal's, its Y and Z swapped as umodel turns
/// them).
pub(crate) fn socket_axes([pitch, yaw, roll]: [i32; 3]) -> (Vec3, Vec3) {
    let k = std::f32::consts::TAU / 65536.0;
    let (sp, cp) = (pitch as f32 * k).sin_cos();
    let (sy, cy) = (yaw as f32 * k).sin_cos();
    let (sr, cr) = (roll as f32 * k).sin_cos();
    let x = Vec3::new(cp * cy, cp * sy, sp);
    let y = Vec3::new(sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, -sr * cp);
    let swap = |v: Vec3| Vec3::new(v.x, v.z, v.y);
    (swap(x), swap(y))
}

/// A new match: no marks left over from the last.
fn clear_marks(mut fx: ResMut<FxSystems>, mut enemies: Query<&mut Enemy>) {
    for mut enemy in &mut enemies {
        if let Some((id, _)) = enemy.mark.fx.take() {
            fx.kill(id);
        }
        enemy.mark = Mark::default();
    }
}

/// The sentries' marks: their time running out, and each one's arrows on the
/// sentry (following it as it bobs, turned towards the camera), the effect of the
/// mark it has now. A mark ends early when the Jazz who put it on dies
/// (TnBuffHardLockedBase.GetBuffState: its Instigator dying or dead); his reveal
/// runs on (TnBuffRevealPositionToEnemy is a plain TnBuff).
fn marked_sentries(
    time: Res<Time>,
    mut fx: ResMut<FxSystems>,
    camera: Query<&Transform, With<OrbitCamera>>,
    players: Query<&Player>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy)>,
    mut showing: Local<Vec<(Entity, FxId)>>,
) {
    let dt = time.delta_secs();
    // (With two players on one screen, each sentry's towards the camera nearest it.)
    let eyes: Vec<Vec3> = camera.iter().map(|t| t.translation).collect();
    let before = std::mem::take(&mut *showing);
    for (entity, _, mut enemy) in &mut enemies {
        let at = enemy.centre();
        let mark = &mut enemy.mark;
        mark.reveal = (mark.reveal - dt).max(0.0);
        if mark.level > 0 {
            mark.left = (mark.left - dt).max(0.0);
            // (A marker who has left the match leaves his marks to run out.)
            let marker_down = mark
                .by
                .and_then(|by| players.get(by).ok())
                .is_some_and(|p| p.dying || p.dead_left > 0.0);
            if mark.left <= 0.0 || marker_down {
                mark.level = 0;
                mark.left = 0.0;
            }
        }
        // (Its X towards the camera, level: the arrows close in from its sides as
        // whoever looks at it sees them.)
        let facing = eyes
            .iter()
            .min_by(|a, b| a.distance_squared(at).total_cmp(&b.distance_squared(at)))
            .map(|e| Vec3::new(e.x - at.x, 0.0, e.z - at.z))
            .and_then(|v| v.try_normalize())
            .unwrap_or(Vec3::X);
        // A new mark takes the last one's place (its arrows go, the new ones come).
        if let Some((id, level)) = mark.fx
            && level != mark.level
        {
            fx.stop(id);
            mark.fx = None;
        }
        if mark.level > 0 && mark.fx.is_none() {
            mark.fx = fx
                .spawn(MARK_FX[mark.level as usize - 1], at, facing, ENERGON)
                .map(|id| (id, mark.level));
        }
        if let Some((id, _)) = mark.fx {
            fx.place(id, at, facing, Vec3::ZERO);
            showing.push((entity, id));
        }
    }
    // A sentry destroyed while marked takes its arrows with it.
    for (entity, id) in before {
        if !showing.iter().any(|(e, _)| *e == entity) && enemies.get(entity).is_err() {
            fx.kill(id);
        }
    }
}

/// The effect a gun plays while it is ready (ShotFx::idle; the Orbital Pistol's
/// steam): from when it is in a Jazz's hand in robot form, unfolded and at rest
/// (TnWeapon's Active state: ActiveStart), until he fires it, reloads it, puts it
/// away (a swap, or holstering it as he sprints: OnHolster), moves it to his other
/// hand or changes form (ActiveStop), on its socket as the gun moves. What is out
/// of it when it stops lives out its life on the gun, as the game's does on the
/// weapon mesh it is attached to, and goes with the gun: once the gun is put away
/// (a swap, holstered, another form, or his death) it is gone at once. (Left where
/// it stopped, the Energon Harvester's arcs hung in the air where the gun had been
/// swapped away.)
fn idle_steam(
    mut fx: ResMut<FxSystems>,
    players: Query<(Entity, &Player)>,
    gun_bones: Query<(&GlobalTransform, &GunBone, &JazzOf)>,
    mut playing: Local<Vec<IdlePlaying>>,
) {
    let before = std::mem::take(&mut *playing);
    // Where a gun's idle socket is now: on its arm's elbow, if the gun shows there.
    let socket_at = |jazz: Entity, player: &Player, weapon: usize, left: bool| {
        let shows = player.form == Form::Robot
            && player.weapon == weapon
            && !player.holstered
            && player.dead_left <= 0.0
            && !player.dying
            && (player.left_handed == left || player.hand_switch.is_some());
        if !shows {
            return None;
        }
        let idle = WEAPONS
            .get(weapon)
            .and_then(|w| w.level_fx)
            .and_then(|f| f.idle.as_ref())?;
        let elbow = gun_bones
            .iter()
            .find(|(_, b, of)| b.0 == left && of.0 == jazz)
            .map(|(t, _, _)| *t)?;
        let (pos, rot) = idle.socket[left as usize];
        let (x, y) = socket_axes(rot);
        let at = elbow.transform_point(pos);
        let x = elbow.affine().transform_vector3(x).normalize_or(Vec3::X);
        let y = elbow.affine().transform_vector3(y).normalize_or(Vec3::Y);
        Some((idle.system, at, x, y))
    };
    for (jazz, player) in &players {
        let ready = player.form == Form::Robot
            && !player.holstered
            && player.equipping.is_none()
            && player.hand_switch.is_none()
            && !player.firing
            && player.fire_cooldown <= 0.0
            && player.reload_left <= 0.0
            && player.dead_left <= 0.0
            && !player.dying
            && player.transform_left <= 0.0;
        let (weapon, left) = (player.weapon, player.left_handed);
        let Some((system, at, x, y)) = socket_at(jazz, player, weapon, left).filter(|_| ready)
        else {
            continue;
        };
        let id = match before
            .iter()
            .find(|p| p.jazz == jazz && !p.stopped && p.weapon == weapon && p.left == left)
        {
            Some(p) if fx.alive(p.id) => Some(p.id),
            _ => fx.spawn_on_socket(system, at, x, ENERGON),
        };
        if let Some(id) = id {
            fx.place_turned(id, at, x, y);
            playing.push(IdlePlaying {
                jazz,
                id,
                weapon,
                left,
                stopped: false,
            });
        }
    }
    // The ones no longer ready stop, what is out of them riding the gun while it
    // shows; gone with it.
    for mut p in before {
        if playing.iter().any(|q| q.id == p.id) || !fx.alive(p.id) {
            continue;
        }
        let place = players
            .get(p.jazz)
            .ok()
            .and_then(|(_, player)| socket_at(p.jazz, player, p.weapon, p.left));
        match place {
            Some((_, at, x, y)) => {
                if !p.stopped {
                    fx.stop(p.id);
                    p.stopped = true;
                }
                fx.place_turned(p.id, at, x, y);
                playing.push(p);
            }
            None => fx.kill(p.id),
        }
    }
}

/// A gun's idle effect playing (idle_steam), or dying out once stopped: whose, on
/// which gun and on which arm.
struct IdlePlaying {
    jazz: Entity,
    id: FxId,
    weapon: usize,
    left: bool,
    stopped: bool,
}
