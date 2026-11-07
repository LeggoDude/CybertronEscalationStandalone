//! The Infiltrator's EMP Grenade: the ability of builds that take it (the game's
//! FlashBangAbility: TnAbilityGrenade throwing a TnProjectileFlashBang), as
//! MP_Array_BASE_m.xxx and TransGame.xxx have it.
//!
//! * **The ability** (TnAbilityGrenade, Default__TnAbilityGrenade): a resource of
//!   15 that refills at 1 a second (twice as fast with EMP Intercooler); a throw
//!   takes all 15. Pressing Ability (Q) takes it and, a tenth of a second later
//!   (kDelayBeforeTrajectoryShowsUp), shows where the grenade would go
//!   (Grenade_Trajectory_FX run along a simulated throw, TnGrenadeThrower's
//!   SimulateTrajectory); letting go plays the throw (Grenade_Throw_MP) and the
//!   grenade leaves his hand 0.3 s later (ThrowDelay), towards where he looked as
//!   he let go. In the car he can't: changing form with it held gives the resource
//!   back.
//! * **The throw** (TnGrenadeThrower, WEP_FlashBangs_p.FlashBangs_WEPDATA): the
//!   target is the first thing the view meets from 5 to 50 m out (TraceRange), or
//!   50 m out; the grenade is tossed from his throwing hand to reach it in 0.1 to
//!   0.5 s (FlightTime, by the distance over the trace's range) under the world's
//!   gravity.
//! * **The grenade** (TnProjectileGrenadeBase, FlashBang_PROJDATA): it falls and
//!   bounces (each bounce reflects it and keeps 0.4 of its speed, BounceDampening)
//!   and stops once it slows below 1 m/s (LowSpeedThreshold). Its fuse starts as it
//!   first strikes something: 0.25 s (FuseTime; half with Short Fuse, "detonates
//!   quickly after impact"), then it goes off. It has no mesh of its own
//!   (FlashBang_PROJMESH is its effects): it flies as FlashBang_Grenade_FX with its
//!   flight loop, bounces with the grenade bounce, and starts the fuse's sound as it
//!   lands.
//! * **Going off** (TnProjectileFlashBang.Explode): Exp_FlashGrenade_FX and its
//!   blast sound; every sentry within 15 m (DamageRadius) with nothing of the level
//!   between is knocked out (it stops firing and turning: SetDormant) for the time
//!   the blast's RadiusDamageScale gives at its distance (DistanceScaler_9971: 4 s
//!   out to 7.5 m, then from 2 s down to none at 15 m), with EMPAttack_FX on it
//!   (TnBuffFlashBang's ApplyFx). Teammates aren't touched. With Speed Redirector,
//!   catching any gives the thrower half as much speed again for 3 s
//!   (TnBuffGrenadeIncreaseSpeed), with SpeedBoost_Trail_FX and the speed boost's
//!   sounds.
//!
//! The HUD shows the ability on robotAbility_mc (hud.rs). Thrown grenades go to the
//! other players' PCs (net.rs's EmpGrenade), whose copy knocks out their sentries.

use bevy::prelude::*;

use crate::controls::{Action, Controls, Mode};
use crate::enemies::Target;
use crate::hud::{HudAbility, HudState};
use crate::levelfx::{FxId, FxSystems};
use crate::sound::Sfx;
use crate::{Colliders, Enemy, Form, Level, LocalPlayer, Player, PlayerInput, choose, net};

/// The ability's id in the builds (customize.txt).
pub const ABILITY_ID: &str = "FlashBangAbility";
/// Its upgrades (TnSkillGrenadeFuseTime, TnSkillGrenadeResourceRefillRate,
/// TnSkillGrenadeIncreaseSpeed).
const SHORT_FUSE: &str = "GrenadeFuseTime";
const INTERCOOLER: &str = "GrenadeResourceRefillRate";
const SPEED_REDIRECTOR: &str = "GrenadeIncreaseSpeed";
/// Its icon on the HUD (hud.txt: the movie's AbilityIconFlashBangAbility).
const HUD_ICON: &str = "AbilityIconFlashBangAbility";

/// AbilityResourceDatas (MP): the resource's size, what a throw takes, how fast it
/// refills (a second), and EMP Intercooler's multiplier on that
/// (TnBuffGrenadeResourceRefillRate).
const RESOURCE_MAX: f32 = 15.0;
const RESOURCE_COST: f32 = 15.0;
const REFILL_RATE: f32 = 1.0;
const INTERCOOLER_REFILL: f32 = 2.0;
/// Seconds from pressing Ability to the trajectory showing (kDelayBeforeTrajectoryShowsUp).
const PREVIEW_DELAY: f32 = 0.1;
/// Seconds from letting go to the grenade leaving his hand (ThrowDelay), and the
/// throw's animation (ThrowAnim: Grenade_Throw_MP, 0.9 s, at its PlayRate 1).
const THROW_DELAY: f32 = 0.3;
const THROW_SECS: f32 = 0.9;
const THROW_RATE: f32 = 1.0;
/// TraceRange (metres from the view) and FlightTime (seconds).
const TRACE_MIN: f32 = 5.0;
const TRACE_MAX: f32 = 50.0;
const FLIGHT_MIN: f32 = 0.1;
const FLIGHT_MAX: f32 = 0.5;
/// The world's gravity (WorldInfo's DefaultGravityZ, -2940 cm/s/s; the grenade's
/// GravityScale is 1).
const GRAVITY: f32 = -29.4;
/// BounceDampening, FuseTime and Short Fuse's multiplier on it
/// (TnBuffGrenadeFuseTime).
const BOUNCE_KEEP: f32 = 0.4;
const FUSE_SECS: f32 = 0.25;
const SHORT_FUSE_SCALE: f32 = 0.5;
/// LowSpeedThreshold: slower than this (m/s) it stops where it is.
const REST_SPEED: f32 = 1.0;
/// Bounces slower than this (m/s) make no sound.
const BOUNCE_SOUND_SPEED: f32 = 2.0;
/// It goes off by this many seconds after leaving the hand at the latest (one
/// thrown out of the level that never lands).
const MAX_FLIGHT_SECS: f32 = 10.0;
/// The grenade's size against walls (its collision cylinder): it stops this far
/// short of a surface it strikes.
const RADIUS: f32 = 0.1;
/// DamageRadius (metres) and RadiusDamageScale (DistanceScaler_9971): seconds a
/// sentry stays knocked out (see `knockout_secs`).
const BLAST_RADIUS: f32 = 15.0;
const KNOCKOUT_SECS: f32 = 4.0;
const KNOCKOUT_FULL_RANGE: f32 = 7.5;
/// Speed Redirector (TnBuffGrenadeIncreaseSpeed): his speed times this, for this
/// long.
pub const SPEED_BOOST: f32 = 1.5;
const SPEED_BOOST_SECS: f32 = 3.0;
/// TnGrenadeThrower's simulation of a throw: a place every this many seconds, for
/// at most this long.
const PREVIEW_STEP: f32 = 0.05;
const PREVIEW_MAX_SECS: f32 = 3.0;
/// Where his effect sockets are (CenterOfMass_Socket), over his feet.
const CENTRE_OF_MASS: f32 = 2.0;

/// The effects (fx/weapons_fx.txt).
const FX_FLIGHT: &str = "FlashBang_Grenade_FX";
const FX_EXPLOSION: &str = "Exp_FlashGrenade_FX";
const FX_TRAJECTORY: &str = "Grenade_Trajectory_FX";
const FX_STUNNED: &str = "EMPAttack_FX";
const FX_SPEED: &str = "SpeedBoost_Trail_FX";
/// The effects' EnergonColor: a team's own effects are cyan in a match
/// (TnCharacterApplier's MpSameTeamEnergonColor).
const ENERGON: &[(&str, [u8; 4])] = &[("EnergonColor", [0, 255, 255, 255])];

const SND_FLIGHT: &str = "BL_WPN_GRENADE.EMP_FLIGHT_LP";
const SND_BOUNCE: &str = "BL_WPN_GRENADE.GRENADE_FOLEY_SHELL_BOUNCE_LIGHT";
const SND_FUSE: &str = "BL_WPN_GRENADE.EMP_FUSE_BUILD";
const SND_BLAST: &str = "BL_WPN_GRENADE.EMP_EXPL_IMPT_WORLD";
/// The speed boost's (ABILITY_SPEEDBOOST_LP_START, a loop, and _LP_STOP).
const SND_SPEED: &str = "BL_TRANS_POWER.SPEED_BOOST_START";
const SND_SPEED_END: &str = "BL_TRANS_POWER.SPEED_BOOST_END";
pub(crate) const SND_READY: &str = "BL_HUD_INTERFACE.HUD_ABILITY_READY";

pub struct EmpPlugin;

impl Plugin for EmpPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(crate::menu::AppState::Game), clear_grenades)
            .add_systems(
                Update,
                (
                    (equip_chosen_ability, use_ability)
                        .chain()
                        .after(crate::player_movement)
                        .before(crate::animate_robot),
                    (fly_grenades, knocked_out_sentries)
                        .chain()
                        .after(crate::melee_hits)
                        .before(crate::health_stations),
                    // (After net_capture: the HUD's sound is this PC's own.)
                    feed_ability_hud
                        .after(crate::net_capture)
                        .before(crate::sound::PlaySounds),
                )
                    .run_if(in_state(crate::menu::AppState::Game))
                    .before(crate::hud::HudSystems),
            );
    }
}

/// A Jazz whose character has the EMP grenade: its upgrade, its resource, the
/// button, the trajectory showing, a throw on its way, and the speed boost's
/// effect.
#[derive(Component)]
pub(crate) struct EmpAbility {
    short_fuse: bool,
    intercooler: bool,
    speed_redirector: bool,
    resource: f32,
    /// The button is held (the resource taken), for this long; and whether it was
    /// down last frame (a throw starts with a press).
    held: Option<f32>,
    button: bool,
    preview: Option<FxId>,
    /// Seconds to the grenade leaving his hand, and where it goes.
    throw_in: Option<(f32, Vec3)>,
    /// Ready last frame (the HUD's ready sound plays as it comes ready).
    ready: bool,
    speed_fx: Option<FxId>,
}

impl EmpAbility {
    fn new(upgrade: &str) -> EmpAbility {
        EmpAbility {
            short_fuse: upgrade == SHORT_FUSE,
            intercooler: upgrade == INTERCOOLER,
            speed_redirector: upgrade == SPEED_REDIRECTOR,
            resource: RESOURCE_MAX,
            held: None,
            button: false,
            preview: None,
            throw_in: None,
            ready: true,
            speed_fx: None,
        }
    }

    fn fuse_secs(&self) -> f32 {
        FUSE_SECS
            * if self.short_fuse {
                SHORT_FUSE_SCALE
            } else {
                1.0
            }
    }

    fn in_use(&self) -> bool {
        self.held.is_some() || self.throw_in.is_some()
    }
}

/// A thrown EMP grenade.
#[derive(Component)]
pub(crate) struct EmpGrenade {
    velocity: Vec3,
    /// Seconds since it left the hand, and its fuse's seconds left once it has
    /// struck something.
    age: f32,
    fuse: Option<f32>,
    fuse_secs: f32,
    resting: bool,
    /// The last surface it struck (its blast faces out of it).
    normal: Vec3,
    /// The Jazz who threw it, for Speed Redirector (None from another PC).
    thrower: Option<Entity>,
    flight_fx: Option<FxId>,
    /// Its effects' X: the way the thrower faced (it leaves turned as he is).
    facing: Vec3,
    /// The enemies near have been warned of it (_PawnsNotifiedOfGrenade).
    warned: bool,
}

/// Gives this PC's Jazz his chosen character's ability as he takes up its
/// loadout (match start, respawn): the EMP grenade, with its upgrade, or none.
fn equip_chosen_ability(
    mut commands: Commands,
    chosen: crate::split::BySeatRef<choose::Chosen>,
    mut fx: ResMut<FxSystems>,
    mut players: Query<
        (
            Entity,
            &mut Player,
            Option<&mut EmpAbility>,
            Option<&crate::split::Seat>,
        ),
        With<LocalPlayer>,
    >,
) {
    for (entity, mut player, ability, seat) in &mut players {
        if !std::mem::take(&mut player.rearmed) {
            continue;
        }
        // (Split screen's guest's from his own.)
        let chosen = chosen.seat(crate::split::seat_of(seat));
        player.speed_boost = 0.0;
        if let Some(mut a) = ability {
            for id in [a.preview.take(), a.speed_fx.take()].into_iter().flatten() {
                fx.kill(id);
            }
        }
        if chosen.ability == ABILITY_ID {
            commands
                .entity(entity)
                .insert(EmpAbility::new(&chosen.ability_upgrade));
        } else {
            commands.entity(entity).remove::<EmpAbility>();
        }
    }
}

/// A new match: no grenades left over from the last.
fn clear_grenades(
    mut commands: Commands,
    mut fx: ResMut<FxSystems>,
    grenades: Query<(Entity, &EmpGrenade)>,
    mut enemies: Query<&mut Enemy>,
) {
    for (entity, grenade) in &grenades {
        if let Some(id) = grenade.flight_fx {
            fx.kill(id);
        }
        commands.entity(entity).despawn();
    }
    for mut enemy in &mut enemies {
        enemy.emp_left = 0.0;
        if let Some(id) = enemy.emp_fx.take() {
            fx.kill(id);
        }
    }
}

/// Where his throwing hand is (TossSocketRight, or TossSocketLeft when he holds his
/// gun left-handed): where the grenade leaves from.
fn hand(transform: &Transform, player: &Player) -> Vec3 {
    let side = if player.left_handed { -0.9 } else { 0.9 };
    transform.translation + Vec3::Y * 2.9 + *transform.forward() * 0.9 + *transform.right() * side
}

/// TnGrenadeThrower's target: the first thing the view meets from TRACE_MIN to
/// TRACE_MAX out (the level, an arena pillar, a sentry), else the trace's far end.
fn target(
    level: &Level,
    colliders: &Colliders,
    sentries: &[Target],
    eye: Vec3,
    view: Vec3,
) -> Vec3 {
    let view = view.normalize_or(Vec3::NEG_Z);
    let from = eye + view * TRACE_MIN;
    let reach = TRACE_MAX - TRACE_MIN;
    let wall = level
        .shot_surface(colliders, from, view, reach)
        .map_or(reach, |(t, _)| t);
    let sentry = sentries
        .iter()
        .filter_map(|t| t.body.ray(from, view, reach))
        .reduce(f32::min);
    from + view * sentry.map_or(wall, |s| s.min(wall))
}

/// The velocity that carries the grenade from `start` to `target` in its flight
/// time (TnGrenadeThrower's toss).
fn toss(start: Vec3, target: Vec3) -> Vec3 {
    let d = target - start;
    let k = (d.length() / (TRACE_MAX - TRACE_MIN)).clamp(0.0, 1.0);
    let t = FLIGHT_MIN + (FLIGHT_MAX - FLIGHT_MIN) * k;
    let mut v = d / t;
    v.y -= 0.5 * GRAVITY * t;
    v
}

/// A grenade's step of `dt`: it falls, and bounces off what it strikes (the level,
/// an arena pillar, a sentry). The struck surface's normal if it struck one.
fn step(
    level: &Level,
    colliders: &Colliders,
    sentries: &[Target],
    at: &mut Vec3,
    velocity: &mut Vec3,
    dt: f32,
) -> Option<Vec3> {
    velocity.y += GRAVITY * dt;
    let motion = *velocity * dt;
    let len = motion.length();
    if len < 1e-6 {
        return None;
    }
    let dir = motion / len;
    let reach = len + RADIUS;
    let mut hit = level.shot_surface(colliders, *at, dir, reach);
    for target in sentries {
        if let Some(t) = target.body.ray(*at, dir, hit.map_or(reach, |(h, _)| h)) {
            hit = Some((t, (*at + dir * t - target.at).normalize_or(-dir)));
        }
    }
    match hit {
        Some((t, normal)) => {
            *at += dir * (t - RADIUS).max(0.0);
            let n = if normal.dot(dir) > 0.0 {
                -normal
            } else {
                normal
            };
            *velocity = (*velocity - 2.0 * velocity.dot(n) * n) * BOUNCE_KEEP;
            Some(n)
        }
        None => {
            *at += motion;
            None
        }
    }
}

/// The path a throw would take (TnGrenadeThrower.SimulateTrajectory): places
/// PREVIEW_STEP apart, until its fuse would run out after it first strikes
/// something, it stops, or PREVIEW_MAX_SECS.
fn simulate(
    level: &Level,
    colliders: &Colliders,
    sentries: &[Target],
    start: Vec3,
    velocity: Vec3,
    fuse_secs: f32,
) -> Vec<Vec3> {
    let (mut at, mut v) = (start, velocity);
    let mut path = vec![at];
    let mut fuse: Option<f32> = None;
    let mut t = 0.0;
    while t < PREVIEW_MAX_SECS {
        t += PREVIEW_STEP;
        if step(level, colliders, sentries, &mut at, &mut v, PREVIEW_STEP).is_some()
            && fuse.is_none()
        {
            fuse = Some(fuse_secs);
        }
        path.push(at);
        if let Some(f) = fuse.as_mut() {
            *f -= PREVIEW_STEP;
            if *f <= 0.0 {
                break;
            }
        }
        if fuse.is_some() && v.length() < REST_SPEED {
            break;
        }
    }
    path
}

/// The Ability button for this PC's Jazz (other PCs' players throw on their own
/// PCs): taking the resource, showing the trajectory, throwing, refilling; and
/// Speed Redirector's boost running out.
#[allow(clippy::too_many_arguments)]
fn use_ability(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut players: Query<
        (
            Entity,
            &Transform,
            &mut Player,
            &PlayerInput,
            &mut EmpAbility,
        ),
        With<LocalPlayer>,
    >,
    enemies: Query<(Entity, &Enemy), Without<Player>>,
) {
    let dt = time.delta_secs();
    let sentries = crate::arsenal::live_targets(enemies.iter());
    for (entity, transform, mut player, input, mut a) in &mut players {
        let player = &mut *player;
        let a = &mut *a;
        let alive = player.dead_left <= 0.0 && !player.dying;
        // (Down, his ability is ended and can't be used: revive.rs.)
        let robot = player.form == Form::Robot
            && player.transform_left <= 0.0
            && player.pending_form.is_none()
            && !player.revive.locked();
        let facing = transform.forward().as_vec3();
        // (Not in the middle of a blow, a landing or a throw.)
        let busy = matches!(
            player.action,
            Some((
                crate::ANIM_MELEE
                    | crate::ANIM_MELEE_BOOST
                    | crate::ANIM_THROW
                    | crate::ANIM_LAND_HARD
                    | crate::ANIM_DEATH,
                _
            ))
        );

        // Jammed (abilities.rs's jam): no throw, nor refilling (SetCooldownMultiplier
        // (0)), and one held is ended (ForceEndAbilityByClass: nothing is thrown, the
        // resource comes back).
        let jammed = crate::abilities::jammed(player);
        if jammed && a.held.is_some() {
            a.held = None;
            a.resource = RESOURCE_MAX;
            if let Some(id) = a.preview.take() {
                fx.kill(id);
            }
        }
        // Refilling, while it isn't held; he comes back from death with it full.
        // (Faster with the Intercooler's buff on him: equipment.rs.)
        if a.held.is_none() && !jammed {
            let rate = REFILL_RATE
                * if a.intercooler {
                    INTERCOOLER_REFILL
                } else {
                    1.0
                }
                * player.support.cooldown_scale();
            a.resource = (a.resource + rate * dt).min(RESOURCE_MAX);
        }
        if !alive {
            a.resource = RESOURCE_MAX;
            a.throw_in = None;
        }

        // Down, or changing out of robot form, with the button held: nothing is
        // thrown, and the resource comes back.
        if a.held.is_some() && (!alive || !robot) {
            a.held = None;
            a.resource = RESOURCE_MAX;
            if let Some(id) = a.preview.take() {
                fx.kill(id);
            }
        }

        let pressed = input.ability && !a.button;
        a.button = input.ability;
        match a.held {
            None => {
                // Pressed (TriggerAbility): it takes the resource, or does nothing
                // short of it.
                if pressed
                    && input.control
                    && alive
                    && robot
                    && !busy
                    && !jammed
                    && !player.melee_react.reacting()
                    && a.throw_in.is_none()
                    && a.resource >= RESOURCE_COST
                {
                    a.resource -= RESOURCE_COST;
                    a.held = Some(0.0);
                }
            }
            Some(held) if input.ability => {
                // Held: where it would go, run along a simulated throw every frame
                // (once SimulationDelayTimer is up).
                let held = held + dt;
                a.held = Some(held);
                if held >= PREVIEW_DELAY {
                    let start = hand(transform, player);
                    let aim = target(
                        &level,
                        &colliders,
                        &sentries,
                        input.view_origin,
                        input.view_dir,
                    );
                    let path = simulate(
                        &level,
                        &colliders,
                        &sentries,
                        start,
                        toss(start, aim),
                        a.fuse_secs(),
                    );
                    if a.preview.is_none_or(|id| !fx.alive(id)) {
                        a.preview = fx.spawn(FX_TRAJECTORY, start, facing, ENERGON);
                    }
                    if let Some(id) = a.preview {
                        let steps: Vec<(Vec3, Vec3)> = path.iter().map(|p| (*p, facing)).collect();
                        fx.replay(id, &steps, PREVIEW_STEP);
                    }
                }
            }
            Some(_) => {
                // Let go (UntriggerAbility): the throw, towards where he looks now.
                a.held = None;
                if let Some(id) = a.preview.take() {
                    fx.kill(id);
                }
                let aim = target(
                    &level,
                    &colliders,
                    &sentries,
                    input.view_origin,
                    input.view_dir,
                );
                a.throw_in = Some((THROW_DELAY, aim));
                crate::start_action(
                    player,
                    crate::ANIM_THROW,
                    THROW_SECS / THROW_RATE,
                    THROW_RATE,
                );
            }
        }

        // The grenade leaves his hand.
        if let Some((left, aim)) = a.throw_in {
            if left > dt {
                a.throw_in = Some((left - dt, aim));
            } else {
                a.throw_in = None;
                let start = hand(transform, player);
                let velocity = toss(start, aim);
                let fuse = a.fuse_secs();
                player.played.push(net::JazzEvent::EmpGrenade {
                    start,
                    velocity,
                    fuse,
                });
                spawn_emp_grenade(
                    &mut commands,
                    &mut fx,
                    start,
                    velocity,
                    fuse,
                    facing,
                    Some(entity),
                );
            }
        }

        // Speed Redirector's boost: its trail and loop while it lasts.
        if player.speed_boost > 0.0 {
            let at = transform.translation + Vec3::Y * CENTRE_OF_MASS;
            if a.speed_fx.is_none_or(|id| !fx.alive(id)) {
                // (At his robot's size, as the game attaches it to his mesh.)
                let size = player.kit.robot.effect_scale();
                a.speed_fx = fx.spawn_scaled(FX_SPEED, at, facing, size, ENERGON);
            }
            if let Some(id) = a.speed_fx {
                fx.place(id, at, facing, player.velocity);
            }
            player.speed_boost -= dt;
            if player.speed_boost > 0.0 && alive {
                sfx.hold(entity.to_bits(), SND_SPEED, None);
            } else {
                player.speed_boost = 0.0;
                if let Some(id) = a.speed_fx.take() {
                    fx.stop(id);
                }
                sfx.play(SND_SPEED_END);
            }
        }
    }
}

/// A thrown EMP grenade leaving `start` at `velocity` (fly_grenades flies it and
/// sets it off); `fuse`: its fuse's seconds once it strikes something. Also for
/// another PC's throw (net.rs), with no thrower here.
pub(crate) fn spawn_emp_grenade(
    commands: &mut Commands,
    fx: &mut FxSystems,
    start: Vec3,
    velocity: Vec3,
    fuse: f32,
    facing: Vec3,
    thrower: Option<Entity>,
) {
    let flight_fx = fx.spawn(FX_FLIGHT, start, facing, ENERGON);
    commands.spawn((
        EmpGrenade {
            velocity,
            age: 0.0,
            fuse: None,
            fuse_secs: fuse.clamp(0.01, 5.0),
            resting: false,
            normal: Vec3::Y,
            thrower,
            flight_fx,
            facing,
            warned: false,
        },
        Transform::from_translation(start),
    ));
}

/// Seconds a sentry `distance` from the blast stays knocked out
/// (DistanceScaler_9971: a constant 4 out to 7.5 m, a line from 4 at the blast to
/// none at 15 m beyond that, nothing past 15 m).
fn knockout_secs(distance: f32) -> f32 {
    if distance <= KNOCKOUT_FULL_RANGE {
        KNOCKOUT_SECS
    } else if distance <= BLAST_RADIUS {
        KNOCKOUT_SECS * (1.0 - distance / BLAST_RADIUS)
    } else {
        0.0
    }
}

/// Flies the EMP grenades, bouncing, and sets them off as their fuses run out.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn fly_grenades(
    mut commands: Commands,
    time: Res<Time>,
    level: Res<Level>,
    colliders: Res<Colliders>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut grenades: Query<(Entity, &mut Transform, &mut EmpGrenade)>,
    mut enemies: Query<(Entity, &mut Enemy), (Without<Player>, Without<EmpGrenade>)>,
    mut throwers: Query<(&mut Player, &EmpAbility)>,
    mut near: ResMut<crate::ai::NearGrenades>,
) {
    let dt = time.delta_secs();
    let sentries = crate::arsenal::live_targets(enemies.iter());
    for (entity, mut transform, mut g) in &mut grenades {
        let g = &mut *g;
        g.age += dt;
        let mut at = transform.translation;
        if !g.resting {
            let speed = g.velocity.length();
            if let Some(normal) = step(&level, &colliders, &sentries, &mut at, &mut g.velocity, dt)
            {
                g.normal = normal;
                if speed > BOUNCE_SOUND_SPEED {
                    sfx.play_at(SND_BOUNCE, at);
                }
                if g.fuse.is_none() {
                    g.fuse = Some(g.fuse_secs);
                    sfx.play_at(SND_FUSE, at);
                }
                // (OnHitThing's NotifyPawnsOfGrenade: once it bounces slowly
                // enough across the ground, the enemies near take cover from it.)
                let across = Vec2::new(g.velocity.x, g.velocity.z).length();
                if !g.warned && across <= crate::ai::GRENADE_WARN_SPEED {
                    g.warned = true;
                    near.0.push(at);
                }
                if g.velocity.length() < REST_SPEED {
                    g.resting = true;
                    g.velocity = Vec3::ZERO;
                }
            }
            transform.translation = at;
        }
        if let Some(id) = g.flight_fx {
            fx.place(id, at, g.facing, g.velocity);
        }
        if let Some(f) = g.fuse.as_mut() {
            *f -= dt;
        }
        let gone_off = g.fuse.is_some_and(|f| f <= 0.0) || g.age > MAX_FLIGHT_SECS;
        if !gone_off {
            sfx.hold(entity.to_bits(), SND_FLIGHT, Some(at));
            continue;
        }

        // Going off.
        commands.entity(entity).despawn();
        if let Some(id) = g.flight_fx.take() {
            fx.stop(id);
        }
        fx.spawn(FX_EXPLOSION, at, g.normal, ENERGON);
        sfx.play_at(SND_BLAST, at);
        // (Seen from a little off the surface it rests on.)
        let centre = at + g.normal * 0.3;
        let mut caught = false;
        for (_, mut enemy) in &mut enemies {
            let p = enemy.centre();
            let secs = knockout_secs(enemy.reach(centre));
            if secs <= 0.0 || !enemy.alive() || level.blocked(centre, p) {
                continue;
            }
            enemy.emp_left = enemy.emp_left.max(secs);
            // (Its knock-out is the thrower's now: xp.rs's assists and melee kills.)
            enemy.emp_by = g.thrower;
            caught = true;
        }
        if caught
            && let Some((mut player, ability)) = g.thrower.and_then(|t| throwers.get_mut(t).ok())
            && ability.speed_redirector
            && player.dead_left <= 0.0
        {
            player.speed_boost = SPEED_BOOST_SECS;
        }
    }
}

/// Sentries knocked out by an EMP grenade: the time running out, and the blast's
/// sparks on them (EMPAttack_FX, following them as they bob) meanwhile.
fn knocked_out_sentries(
    time: Res<Time>,
    mut fx: ResMut<FxSystems>,
    mut enemies: Query<(Entity, &mut Enemy)>,
    mut showing: Local<Vec<(Entity, FxId)>>,
) {
    let dt = time.delta_secs();
    let before = std::mem::take(&mut *showing);
    for (entity, mut enemy) in &mut enemies {
        let enemy = &mut *enemy;
        let at = enemy.centre();
        if enemy.emp_left > 0.0 {
            enemy.emp_left = (enemy.emp_left - dt).max(0.0);
            if enemy.emp_fx.is_none() {
                enemy.emp_fx = fx.spawn(FX_STUNNED, at, Vec3::X, ENERGON);
            }
        }
        if let Some(id) = enemy.emp_fx {
            if enemy.emp_left > 0.0 {
                fx.place(id, at, Vec3::X, Vec3::ZERO);
                showing.push((entity, id));
            } else {
                fx.stop(id);
                enemy.emp_fx = None;
            }
        }
    }
    // A sentry destroyed while knocked out takes its sparks with it.
    for (entity, id) in before {
        if !showing.iter().any(|(e, _)| *e == entity) && enemies.get(entity).is_err() {
            fx.kill(id);
        }
    }
}

/// The HUD's ability (robotAbility_mc) for this PC's Jazz's EMP grenade, and its
/// ready sound as it comes ready (the other abilities' are abilities.rs's).
#[allow(clippy::type_complexity)]
pub(crate) fn feed_ability_hud(
    mut huds: crate::split::BySeat<HudState>,
    controls: Res<Controls>,
    mut sfx: ResMut<Sfx>,
    mut players: Query<
        (
            &Player,
            Option<&mut EmpAbility>,
            Option<&crate::split::Seat>,
        ),
        With<LocalPlayer>,
    >,
) {
    // (Each of this PC's players' HUD: split screen's guest's his own.)
    for (player, ability, seat) in &mut players {
        if let Some(a) = ability {
            feed_one(
                huds.seat(crate::split::seat_of(seat)),
                &controls,
                &mut sfx,
                player,
                a,
            );
        }
    }
}

/// One of this PC's players' EMP grenade on his HUD (feed_ability_hud).
fn feed_one(
    hud: &mut HudState,
    controls: &Controls,
    sfx: &mut Sfx,
    player: &Player,
    mut a: Mut<EmpAbility>,
) {
    let jammed = crate::abilities::jammed(player);
    let ready = a.resource >= RESOURCE_COST && !a.in_use() && !jammed;
    if ready && !a.ready && player.dead_left <= 0.0 {
        sfx.play(SND_READY);
    }
    a.ready = ready;
    let key = controls.short_key_name(Mode::Robot, Action::Ability);
    let shown = HudAbility {
        icon: HUD_ICON,
        meter: a.resource / RESOURCE_MAX,
        cost: RESOURCE_COST / RESOURCE_MAX,
        in_use: a.in_use(),
        key,
        cancel: false,
        active: a.in_use(),
        juice: 0.0,
        jammed,
    };
    if hud.ability.as_ref() != Some(&shown) {
        hud.ability = Some(shown);
    }
}
