//! The Scientist's Battle Sentry (mp_shared.xxx's TransContent.TnAbilitySpawnSentry,
//! from TransGame.TnAbilitySpawnSentryBase: a TnSentryPawnAbility with its
//! TnAiSentryController), as the game runs it in Escalation:
//!
//! * **The meter** (abilities.rs; "MP,ESC"): a full bar of 30 to start, 5 taken as it
//!   comes out and 0.5 a second while it lives (50 s), refilling at 1.5 a second; its
//!   running out destroys it (KillActiveSentry). Resource Recycler stops the drain
//!   (TnBuffSentryResourceDrainRate). Damage to it would come off the meter (0.1 a
//!   point) and a blow would destroy it at once; nothing here attacks it.
//! * **Out**: Ability puts it 4 m ahead of him and 3 m up (turned as he faces; a wall
//!   in the way stops it short: its 2.5 x 2.5 x 1.25 m trace), where it hovers
//!   (AP_AttackSentry_SKEL, a drone on its booster, painted in his colours:
//!   ApplyColor), with his trigger clip (Nav_ToStealth) and SENTRY_ACTIVATE. A sentry
//!   of his already out goes first; Ability again destroys it; it dies with him.
//! * **Waking** (ActivateTime): 2 s dormant, folded (AP_AttackSentry_Deactivate),
//!   then it unfolds (Nav_Shooting_Idle, its anim tree's loop) and hunts.
//! * **Hunting** (WEP_SentryAbility_p.Default_TURRETDEF): the nearest live enemy in
//!   sight within 60 m (AttackRange; whom its target tracker prefers wasn't read:
//!   the nearest is taken), turning to it at 270 degrees a second (LagDegreesPerSecond;
//!   pitch within 85 degrees), SENTRY_TARGET_AQUIRED and its laser sight
//!   (SentryLaser_FX on its MuzzleFlash) as it takes one on; once aimed within 3
//!   degrees (AimedAtTargetThreshold) it fires (TnWeaponDefaultSentryAbility,
//!   Default_WEPDATA): a round every 0.15 s, instant, 8 each (11.2 with Power Core,
//!   TnBuffIncreaseSentryPower), spread from 0.1 growing 0.005 a shot to 0.2, with
//!   AssaultRifle_VEH_MuzzleFlash_FX, AssaultRifle_Tracer_FX, Impact_OS_Turret_A_FX
//!   and IMPT_ENERGY_WORLD where it strikes, and SENTRY_SHOOT. SENTRY_IDLE_LP loops
//!   while it lives.
//! * **Rust Sentry** (SlowSentry: Slow_TURRETDEF, AP_AttackSentrySlow_SKEL): 40 m, and
//!   in place of the gun a beam (TnWeaponSlowSentryAbility, a TnWeaponBeam:
//!   SentrySlow_Beam_FX, Squib_RepairEnemy_FX where it strikes, the repair gun's
//!   SHOOT_LP, SHOOT_DMG_LP while it is on an enemy, SHOOT_LAST_SHOT) whose
//!   DamagePerSecond nothing sets: it does no damage, only slows (TnBuffRust, 0.5 s
//!   at a time: the enemy's speed halved). The enemies here don't move, so it only
//!   shows.
//! * **Destroyed**: SENTRY_EXPL and a blast (its destructible's death effects weren't
//!   read: Grenade_EXP_FX stands in), and it is gone.
//!
//! Online, its owner's PC runs it: its place, aim and state go in his snapshot
//! (net::SentrySnap) for the others' copies, and each round in an Ability event, so
//! their copies of the enemies take it too.

use bevy::gltf::GltfMaterialExtras;
use bevy::prelude::*;

use crate::abilities::{AbilityInbox, AbilityModels, End, Kind, POWER_CORE, RUST_SENTRY};
use crate::character::{PaintCache, PaintMaterial};
use crate::enemies::{Death, Hit};
use crate::levelfx::{FxId, FxSystems};
use crate::net::{self, JazzEvent};
use crate::sound::Sfx;
use crate::{Colliders, Enemy, Level, OwnAnimation, Player, RemotePlayer};

/// TnAbilitySpawnSentryBase's SpawnOffset (ahead, up: metres) and its placement
/// trace's half size.
const SPAWN_OFFSET: Vec2 = Vec2::new(4.0, 3.0);
const SPAWN_CLEARANCE: f32 = 1.25;
/// ActivateTime (seconds).
const WAKE_SECS: f32 = 2.0;
/// AttackRange (metres), Default and Slow; LagDegreesPerSecond; the pitch's reach
/// (VerticalRangeInDegrees); AimedAtTargetThreshold (degrees).
const RANGE: [f32; 2] = [60.0, 40.0];
const TURN_RATE: f32 = 270.0;
const PITCH_MAX: f32 = 85.0;
const AIMED: f32 = 3.0;
/// Default_WEPDATA: InstantHitDamage, FireInterval, spread (min, max, a shot's
/// growth), and Power Core's multiplier.
const DAMAGE: f32 = 8.0;
const FIRE_SECS: f32 = 0.15;
const SPREAD: (f32, f32, f32) = (0.1, 0.2, 0.005);
const POWER_DAMAGE: f32 = 1.4;
/// How the spread settles back with no shots (a second).
const SPREAD_SETTLE: f32 = 0.2;
/// Its muzzle (the MuzzleFlash socket on C_Robo06_XT, out along the gun) from its
/// middle as it faces level, metres: ahead, up.
const MUZZLE: Vec2 = Vec2::new(0.9, -0.55);

const FX_MUZZLE: &str = "AssaultRifle_VEH_MuzzleFlash_FX";
const FX_TRACER: &str = "AssaultRifle_Tracer_FX";
const FX_IMPACT: &str = "Impact_OS_Turret_A_FX";
const FX_LASER: &str = "SentryLaser_FX";
const FX_BEAM: &str = "SentrySlow_Beam_FX";
const FX_BEAM_IMPACT: &str = "Squib_RepairEnemy_FX";
const FX_BOOSTER: &str = "Thrown_Object_Trail_FX";
const FX_DEATH: &str = "Grenade_EXP_FX";
const SND_ACTIVATE: &str = "BL_TRANS_POWER.SENTRY_ACTIVATE";
const SND_IDLE: &str = "BL_TRANS_POWER.SENTRY_IDLE_LP";
const SND_ACQUIRED: &str = "BL_TRANS_POWER.SENTRY_TARGET_AQUIRED";
const SND_SHOOT: &str = "BL_TRANS_POWER.SENTRY_SHOOT";
const SND_IMPACT: &str = "BL_WPN_GUN__IMPT_BYS.IMPT_ENERGY_WORLD";
const SND_DEATH: &str = "BL_TRANS_POWER.SENTRY_EXPL";
const SND_BEAM: &str = "BL_WPN_GUN_REPAIR_WPN.SHOOT_LP";
const SND_BEAM_HIT: &str = "BL_WPN_GUN_REPAIR_WPN.SHOOT_DMG_LP";
const SND_BEAM_END: &str = "BL_WPN_GUN_REPAIR_WPN.SHOOT_LAST_SHOT";
const SND_BEAM_IMPACT: &str = "BL_WPN_GUN_REPAIR_WPN.IMPT_WORLD";

/// The Ability event of one of its rounds (from its muzzle, its way, how far it went
/// plus STRUCK if it struck an enemy).
pub const WHAT_SHOT: u8 = 5;
const STRUCK: f32 = 10000.0;

/// A Jazz's sentry as his snapshot carries it (and his PC's copy of it there): out,
/// waking, on a target, firing; Rust's; its place, aim (heading and pitch, radians)
/// and its beam's reach.
#[derive(Default)]
pub(crate) struct SentryLink {
    pub entity: Option<Entity>,
    pub flags: u8,
    pub at: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub reach: f32,
}

impl SentryLink {
    pub const OUT: u8 = 1;
    pub const WAKING: u8 = 2;
    pub const TARGET: u8 = 4;
    pub const FIRING: u8 = 8;
    pub const RUST: u8 = 16;

    pub fn snap(&self) -> net::SentrySnap {
        net::SentrySnap {
            flags: self.flags,
            at: self.at,
            yaw: self.yaw,
            pitch: self.pitch,
            reach: self.reach,
        }
    }

    pub fn follow(&mut self, s: &net::SentrySnap) {
        self.flags = s.flags;
        self.at = s.at;
        self.yaw = s.yaw;
        self.pitch = s.pitch;
        self.reach = s.reach.clamp(0.0, 100.0);
    }
}

/// A Battle Sentry: whose, Rust's or not, its owner's Power Core, seconds out, its
/// aim (heading and pitch), its target, its gun's cooldown and spread, its model and
/// that model's animation player and clips, its effects, and whether it is another
/// player's copy (following his snapshot).
#[derive(Component)]
pub(crate) struct Sentry {
    owner: Entity,
    rust: bool,
    power: bool,
    age: f32,
    yaw: f32,
    pitch: f32,
    target: Option<Entity>,
    cooldown: f32,
    spread: f32,
    shots: u32,
    model: Entity,
    animator: Option<Entity>,
    painted: bool,
    laser: Option<FxId>,
    beam: Option<FxId>,
    booster: Option<FxId>,
    firing: bool,
    reach: f32,
    remote: bool,
}

/// Its model, under it (turned as it aims).
#[derive(Component)]
pub(crate) struct SentryModel;

pub struct SentryPlugin;

impl Plugin for SentryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(crate::menu::AppState::Game), clear_sentries)
            .add_systems(
                Update,
                (sentry_models, crate::flakshield::shield_models)
                    .after(crate::abilities::AbilitySystems)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

/// Where his sentry comes out (TnAbilitySpawnSentryBase.GetSpawnLocation): SPAWN_OFFSET
/// from his middle as he faces, or short of a wall in the way.
fn spawn_place(level: &Level, colliders: &Colliders, centre: Vec3, heading: f32) -> Vec3 {
    let turn = Quat::from_rotation_y(heading);
    let offset = turn * Vec3::NEG_Z * SPAWN_OFFSET.x + Vec3::Y * SPAWN_OFFSET.y;
    let len = offset.length();
    let dir = offset / len;
    let reach = level
        .shot_surface(colliders, centre, dir, len + SPAWN_CLEARANCE)
        .map_or(len, |(t, _)| (t - SPAWN_CLEARANCE).clamp(0.0, len));
    centre + dir * reach
}

/// Every Jazz's sentry as his ability has it: out as it starts (his PC's; the others'
/// copies as their snapshots say), destroyed as it ends.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_sentry_ability(
    mut commands: Commands,
    level: Res<Level>,
    colliders: Res<Colliders>,
    models: Option<Res<AbilityModels>>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut players: Query<(Entity, &Transform, &mut Player, Has<RemotePlayer>)>,
    mut sentries: Query<(&Transform, &mut Sentry), Without<Player>>,
) {
    for (entity, transform, mut player, remote) in &mut players {
        let player = &mut *player;
        let a = &mut player.ability;
        // (Gone by itself.)
        if a.sentry.entity.is_some_and(|s| sentries.get(s).is_err()) {
            a.sentry.entity = None;
            if !remote {
                a.end(End::Forced);
            }
        }
        let kind = a.kind;
        let ending = a.ended.is_some() || kind != Kind::Sentry;
        // Destroyed as the ability ends (or its owner changes character).
        if ending && let Some(s) = a.sentry.entity.take() {
            destroy(&mut commands, &mut fx, &mut sfx, &mut sentries, s);
            a.sentry.flags = 0;
        }
        if kind != Kind::Sentry {
            continue;
        }
        let Some(models) = models.as_ref() else {
            continue;
        };
        let rust = a.has(RUST_SENTRY);
        let power = a.has(POWER_CORE);
        if !remote && a.started {
            // (A sentry of his out already goes first.)
            if let Some(s) = a.sentry.entity.take() {
                destroy(&mut commands, &mut fx, &mut sfx, &mut sentries, s);
            }
            let centre = player.centre(transform.translation);
            let at = spawn_place(&level, &colliders, centre, player.heading);
            let s = spawn_sentry(
                &mut commands,
                models,
                entity,
                at,
                player.heading,
                rust,
                power,
                false,
            );
            let a = &mut player.ability;
            a.sentry = SentryLink {
                entity: Some(s),
                flags: SentryLink::OUT
                    | SentryLink::WAKING
                    | if rust { SentryLink::RUST } else { 0 },
                at,
                yaw: player.heading,
                pitch: 0.0,
                reach: 0.0,
            };
            player.support.trigger_anim = Some(0.0);
            sfx.play(SND_ACTIVATE);
        }
        // Another player's: his copy here, as his snapshot has it.
        if remote {
            let a = &mut player.ability;
            let out = a.sentry.flags & SentryLink::OUT != 0;
            match (out, a.sentry.entity) {
                (true, None) => {
                    let rust = a.sentry.flags & SentryLink::RUST != 0;
                    let s = spawn_sentry(
                        &mut commands,
                        models,
                        entity,
                        a.sentry.at,
                        a.sentry.yaw,
                        rust,
                        false,
                        true,
                    );
                    a.sentry.entity = Some(s);
                    sfx.play_at(SND_ACTIVATE, a.sentry.at);
                }
                (false, Some(s)) => {
                    destroy(&mut commands, &mut fx, &mut sfx, &mut sentries, s);
                    a.sentry.entity = None;
                }
                _ => {}
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_sentry(
    commands: &mut Commands,
    models: &AbilityModels,
    owner: Entity,
    at: Vec3,
    yaw: f32,
    rust: bool,
    power: bool,
    remote: bool,
) -> Entity {
    let k = usize::from(rust);
    let mut model = commands.spawn((
        Transform::from_rotation(Quat::from_rotation_y(yaw)),
        Visibility::Inherited,
        SentryModel,
        OwnAnimation,
    ));
    if let Some(scene) = models.sentry[k].clone() {
        model.insert(scene);
    }
    let model = model.id();
    commands
        .spawn((
            Sentry {
                owner,
                rust,
                power,
                age: 0.0,
                yaw,
                pitch: 0.0,
                target: None,
                cooldown: 0.0,
                spread: SPREAD.0,
                shots: 0,
                model,
                animator: None,
                painted: false,
                laser: None,
                beam: None,
                booster: None,
                firing: false,
                reach: 0.0,
                remote,
            },
            Transform::from_translation(at),
            Visibility::Inherited,
        ))
        .add_child(model)
        .id()
}

/// A sentry destroyed (Kill): its blast and sound, its effects stopped, and gone.
fn destroy(
    commands: &mut Commands,
    fx: &mut FxSystems,
    sfx: &mut Sfx,
    sentries: &mut Query<(&Transform, &mut Sentry), Without<Player>>,
    s: Entity,
) {
    if let Ok((t, mut sentry)) = sentries.get_mut(s) {
        sfx.play_at(SND_DEATH, t.translation);
        fx.spawn(FX_DEATH, t.translation, Vec3::X, &[]);
        for id in [
            sentry.laser.take(),
            sentry.beam.take(),
            sentry.booster.take(),
        ]
        .into_iter()
        .flatten()
        {
            fx.stop(id);
        }
        if sentry.firing && sentry.rust {
            sfx.play_at(SND_BEAM_END, t.translation);
        }
    }
    commands.entity(s).try_despawn();
}

/// A new match: no sentries left out.
fn clear_sentries(
    mut commands: Commands,
    mut fx: ResMut<FxSystems>,
    sentries: Query<(Entity, &Sentry)>,
    mut players: Query<&mut Player>,
) {
    for (e, s) in &sentries {
        for id in [s.laser, s.beam, s.booster].into_iter().flatten() {
            fx.kill(id);
        }
        commands.entity(e).despawn();
    }
    for mut p in &mut players {
        p.ability.sentry = SentryLink::default();
    }
}

/// Turns `from` towards `to` (radians) by at most `step`.
fn turn_towards(from: f32, to: f32, step: f32) -> f32 {
    let d =
        (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    from + d.clamp(-step, step)
}

/// The way a sentry aiming at `yaw` and `pitch` points.
fn aim_dir(yaw: f32, pitch: f32) -> Vec3 {
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Vec3::NEG_Z
}

/// Every sentry: waking, hunting, turning and firing (its owner's PC's; the other
/// PCs' copies turn as his snapshot says and play his rounds from his events), its
/// model's clips, paint and aim, and its effects and sounds.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn sentry_models(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    models: Option<Res<AbilityModels>>,
    (mut fx, mut sfx, mut inbox): (ResMut<FxSystems>, ResMut<Sfx>, ResMut<AbilityInbox>),
    mut sentries: Query<(Entity, &Transform, &mut Sentry), Without<Player>>,
    mut players: Query<&mut Player>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy), (Without<Player>, Without<Sentry>)>,
    mut turned: Query<
        &mut Transform,
        (
            With<SentryModel>,
            Without<Sentry>,
            Without<Player>,
            Without<Enemy>,
        ),
    >,
    children: Query<&Children>,
    mut animators: Query<&mut AnimationPlayer>,
    meshes: Query<(
        &MeshMaterial3d<StandardMaterial>,
        Option<&GltfMaterialExtras>,
    )>,
    (standard, mut paints, mut cache): (
        Res<Assets<StandardMaterial>>,
        ResMut<Assets<PaintMaterial>>,
        ResMut<PaintCache>,
    ),
) {
    let Some(models) = models else {
        return;
    };
    let dt = time.delta_secs();
    // The other players' sentries' rounds, here.
    for (owner, at, dir, value) in inbox.take(WHAT_SHOT) {
        let struck = value >= STRUCK;
        let distance = if struck { value - STRUCK } else { value }.clamp(0.0, 100.0);
        let dir = dir.normalize_or(Vec3::NEG_Z);
        let end = at + dir * distance;
        fx.spawn(FX_MUZZLE, at, dir, &[]);
        fx.spawn_tracer(FX_TRACER, at, dir, distance);
        fx.spawn(FX_IMPACT, end, -dir, &[]);
        sfx.play_at(SND_SHOOT, at);
        if struck {
            // (Its damage to the enemy here nearest where it struck.)
            let power = players.get(owner).is_ok_and(|p| p.ability.has(POWER_CORE));
            if let Some((_, _, mut enemy)) = enemies
                .iter_mut()
                .filter(|(_, _, e)| e.alive())
                .min_by(|a, b| a.2.reach(end).total_cmp(&b.2.reach(end)))
                .filter(|(_, _, e)| e.reach(end) < 2.0)
            {
                enemy.take(
                    damage(power),
                    Some(owner),
                    crate::xp::Blow::Weapon,
                    // (Default_WEPDATA's TnDamageTypeSentryGun.)
                    Hit::new(Death::Gunfire, end, dir).of_type("TnDamageTypeSentryGun"),
                );
            }
        } else {
            sfx.play_at(SND_IMPACT, end);
        }
    }
    for (entity, transform, mut s) in &mut sentries {
        let s = &mut *s;
        let at = transform.translation;
        s.age += dt;
        let k = usize::from(s.rust);
        // Its model: its clips' player, its paint in its owner's look.
        if s.animator.is_none() {
            s.animator = children
                .iter_descendants(s.model)
                .find(|e| animators.contains(*e));
            if let Some(a) = s.animator {
                commands
                    .entity(a)
                    .insert(AnimationGraphHandle(models.sentry_graph[k].clone()));
            }
        }
        if !s.painted
            && let Ok(owner) = players.get(s.owner)
        {
            let look = owner.look;
            let mut any = false;
            for e in children.iter_descendants(s.model) {
                let Ok((m, extras)) = meshes.get(e) else {
                    continue;
                };
                any = true;
                if extras.is_some_and(|x| x.value.contains("paint"))
                    && let Some(p) = cache.get(&m.0, &look, false, true, &standard, &mut paints)
                {
                    commands
                        .entity(e)
                        .remove::<MeshMaterial3d<StandardMaterial>>()
                        .insert(MeshMaterial3d(p));
                }
            }
            s.painted = any;
        }
        let waking = s.age < WAKE_SECS;
        if let Some(mut player) = s.animator.and_then(|a| animators.get_mut(a).ok()) {
            let [idle, _activate, deactivate] = models.sentry_clips[k];
            if waking {
                player.stop(idle);
                player.play(deactivate).set_speed(0.0).seek_to(0.0);
            } else if !player.is_playing_animation(idle) {
                player.stop(deactivate);
                player.play(idle).repeat();
            }
        }
        // Hunting (its owner's PC): the nearest live enemy in sight within its range.
        let mut fire = false;
        if !s.remote {
            let range = RANGE[k];
            let had = s.target;
            s.target = if waking {
                None
            } else {
                enemies
                    .iter()
                    .filter(|(_, _, e)| e.alive())
                    .map(|(e, _, en)| (e, en.centre()))
                    .filter(|(_, c)| c.distance(at) <= range && !level.blocked(at, *c))
                    .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
                    .map(|(e, _)| e)
            };
            if s.target.is_some() && s.target != had {
                sfx.play_at(SND_ACQUIRED, at);
            }
            if let Some(c) = s
                .target
                .and_then(|t| enemies.get(t).ok())
                .map(|(_, _, e)| e.centre())
            {
                let d = c - at;
                let want_yaw = (-d.x).atan2(-d.z);
                let want_pitch =
                    d.y.atan2(d.with_y(0.0).length())
                        .clamp(-PITCH_MAX.to_radians(), PITCH_MAX.to_radians());
                let step = TURN_RATE.to_radians() * dt;
                s.yaw = turn_towards(s.yaw, want_yaw, step);
                s.pitch = turn_towards(s.pitch, want_pitch, step);
                let off = aim_dir(s.yaw, s.pitch).angle_between(d.normalize_or(Vec3::NEG_Z));
                fire = off.to_degrees() <= AIMED;
                s.reach = d.length();
            } else {
                // (No target: it levels off, LagDegrees at a time.)
                s.pitch = turn_towards(s.pitch, 0.0, TURN_RATE.to_radians() * dt);
            }
        } else if let Ok(owner) = players.get(s.owner) {
            // Another player's: as his snapshot has it.
            let link = &owner.ability.sentry;
            let step = TURN_RATE.to_radians() * dt;
            s.yaw = turn_towards(s.yaw, link.yaw, step * 2.0);
            s.pitch = turn_towards(s.pitch, link.pitch, step * 2.0);
            fire = link.flags & SentryLink::FIRING != 0;
            s.reach = link.reach;
            let target = link.flags & SentryLink::TARGET != 0;
            if target && s.laser.is_none() {
                sfx.play_at(SND_ACQUIRED, at);
            }
            s.target = target.then_some(s.owner);
        }
        if let Ok(mut t) = turned.get_mut(s.model) {
            // (The model's turn: its heading, and its pitch about its own side.)
            t.rotation = Quat::from_rotation_y(s.yaw) * Quat::from_rotation_x(s.pitch);
        }
        let aim = aim_dir(s.yaw, s.pitch);
        let muzzle = at + aim * MUZZLE.x + Quat::from_rotation_y(s.yaw) * Vec3::Y * MUZZLE.y;
        // Its laser sight while it has a target.
        if s.target.is_some() && !waking {
            if s.laser.is_none_or(|id| !fx.alive(id)) {
                s.laser = fx.spawn(FX_LASER, muzzle, aim, &[]);
            }
            if let Some(id) = s.laser {
                fx.place(id, muzzle, aim, Vec3::ZERO);
            }
        } else if let Some(id) = s.laser.take() {
            fx.stop(id);
        }
        // Its booster (the idle clip's notify; the Rust sentry's mesh has none).
        if !s.rust && !waking {
            let down = at - Vec3::Y * 0.6;
            if s.booster.is_none_or(|id| !fx.alive(id)) {
                s.booster = fx.spawn(FX_BOOSTER, down, Vec3::NEG_Y, &[]);
            }
            if let Some(id) = s.booster {
                fx.place(id, down, Vec3::NEG_Y, Vec3::ZERO);
            }
        }
        sfx.hold(entity.to_bits() ^ 0x53454e, SND_IDLE, Some(at));
        // (What a frame overruns the gun's time by comes off the next round's, so it
        // fires at its rate whatever the frame rate.)
        s.cooldown = (s.cooldown - dt).max(-FIRE_SECS);
        if !fire {
            s.cooldown = s.cooldown.max(0.0);
            s.spread = (s.spread - SPREAD_SETTLE * dt).max(SPREAD.0);
        }
        if s.rust {
            // The Rust beam: on while it fires, its loops held.
            if fire {
                let end = muzzle + aim * s.reach.max(1.0);
                if s.beam.is_none_or(|id| !fx.alive(id)) {
                    s.beam = fx.spawn(FX_BEAM, muzzle, aim, &[]);
                    fx.spawn(FX_BEAM_IMPACT, end, -aim, &[]);
                }
                if let Some(id) = s.beam {
                    fx.place(id, muzzle, aim, Vec3::ZERO);
                    fx.set_beam_end(id, end);
                }
                sfx.hold(entity.to_bits() ^ 0x42, SND_BEAM, Some(muzzle));
                sfx.hold(entity.to_bits() ^ 0x43, SND_BEAM_HIT, Some(end));
            } else if let Some(id) = s.beam.take() {
                fx.stop(id);
                sfx.play_at(SND_BEAM_END, muzzle);
                sfx.play_at(SND_BEAM_IMPACT, muzzle + aim * s.reach);
            }
        } else if fire && !s.remote && s.cooldown <= 0.0 {
            // A round: instant, along its aim give or take its spread.
            s.cooldown = (s.cooldown + FIRE_SECS).max(0.0);
            s.shots = s.shots.wrapping_add(1);
            let dir = crate::spread_shot(
                aim,
                s.spread,
                (entity.to_bits() as u32) ^ s.shots.wrapping_mul(2654435761),
            );
            s.spread = (s.spread + SPREAD.2).min(SPREAD.1);
            let wall = level
                .shot_surface(&colliders, muzzle, dir, RANGE[0])
                .map_or(RANGE[0], |(t, _)| t);
            let struck = enemies
                .iter()
                .filter_map(|(e, _, en)| en.ray(muzzle, dir, wall).map(|t| (e, t)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let distance = struck.map_or(wall, |(_, t)| t);
            let end = muzzle + dir * distance;
            fx.spawn(FX_MUZZLE, muzzle, dir, &[]);
            fx.spawn_tracer(FX_TRACER, muzzle, dir, distance);
            fx.spawn(FX_IMPACT, end, -dir, &[]);
            sfx.play_at(SND_SHOOT, muzzle);
            if let Some((e, _)) = struck {
                if let Ok((_, _, mut enemy)) = enemies.get_mut(e) {
                    enemy.take(
                        damage(s.power),
                        Some(s.owner),
                        crate::xp::Blow::Weapon,
                        Hit::new(Death::Gunfire, end, dir).of_type("TnDamageTypeSentryGun"),
                    );
                }
            } else if distance < RANGE[0] {
                sfx.play_at(SND_IMPACT, end);
            }
            if let Ok(mut owner) = players.get_mut(s.owner) {
                owner.played.push(JazzEvent::Ability {
                    what: WHAT_SHOT,
                    at: muzzle,
                    dir,
                    value: distance + if struck.is_some() { STRUCK } else { 0.0 },
                });
            }
        }
        s.firing = fire;
        // Its state for its owner's snapshot.
        if !s.remote
            && let Ok(mut owner) = players.get_mut(s.owner)
        {
            let link = &mut owner.ability.sentry;
            link.at = at;
            link.yaw = s.yaw;
            link.pitch = s.pitch;
            link.reach = s.reach;
            link.flags = SentryLink::OUT
                | if waking { SentryLink::WAKING } else { 0 }
                | if s.target.is_some() {
                    SentryLink::TARGET
                } else {
                    0
                }
                | if fire { SentryLink::FIRING } else { 0 }
                | if s.rust { SentryLink::RUST } else { 0 };
        }
    }
}

/// A round's damage (Power Core's more).
fn damage(power: bool) -> f32 {
    DAMAGE * if power { POWER_DAMAGE } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turning_and_aiming() {
        assert!((turn_towards(0.0, 1.0, 0.25) - 0.25).abs() < 1e-6);
        assert!((turn_towards(3.0, -3.0, 1.0) - (3.0 + 0.2831853)).abs() < 1e-4);
        assert!((aim_dir(0.0, 0.0) - Vec3::NEG_Z).length() < 1e-6);
        assert_eq!(damage(true), 11.2);
        let _ = Kind::Sentry;
    }
}
