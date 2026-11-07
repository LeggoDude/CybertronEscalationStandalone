//! The Soldier's Flak Shield (TransGame.TnAbilityShield driving his block controller:
//! TR_CustomSoldier_ROBODEF_p's TnRobotBlockControllerData_871, MP_Array_BASE_m.xxx),
//! as the game runs it in Escalation:
//!
//! * **The meter** (abilities.rs; "ESC": 1 needed of 4.5): it drains 0.25 a second
//!   while the shield is up (18 s from full) and refills at 0.5 a second; each point
//!   of damage the shield stops takes 0.015 more (DamageResourceMultiplier: 300
//!   damage from full; Brute Force halves it, TnBuffShieldDecreaseDamage).
//! * **Raised**: the shield (WEP_Shield_p.MP.WEP_ShieldLeader_SKEL, models/abilities/
//!   flak_shield.glb) unfolds on his left forearm (ShieldSocket_L; ShieldSocket_R with
//!   his gun in his left hand) with its ShieldGrimlock_Equip clip (at its RateScale
//!   0.5: 0.67 s; shown from 0.027 s of it) and SHIELD_ANIM_EQUIP; he moves in the
//!   shield stance (Shared_SCOUT_ROBO_ANIM's Nav_Shield_* clips: main.rs) at half his
//!   speed unless he sprints (BlockGroundSpeedMultiplier 0.5; Basher's 0.75,
//!   TnBuffShieldIncreasedMovement). He can jump with it up.
//! * **Lowered** by a press, by his shooting or striking (AutoStopShieldWhenAttacking)
//!   or by changing form: ShieldGrimlock_Drop (RateScale 1.75: 0.19 s; hidden from
//!   0.278 s of it) and SHIELD_ANIM_RETRACT.
//! * **Broken** as the meter runs out (DestroyShield): it is gone at once, he plays
//!   the shield's break (HitReaction_Shield_Break over his upper body, standing in for
//!   HitReactions_React_EnergonShield_Stand_R, which isn't exported) and can't raise
//!   it for 5 s (KnockOutOfBlockPenaltyTime). The Soldier's controller names no break
//!   sound or effect: SHIELD_ROBO_AUTOBOT_BREAK and Shield_Break_1A_FX are used.
//! * **Blocking** (TnBlockController.CanBlockDamage): blows coming at him from in
//!   front are stopped (BlockDamageMultiplier 0), costing the meter; Blastproof heals
//!   him a tenth of each (TnBuffShieldHealOnDamage). (The game stops what strikes the
//!   shield's own collision and melee from the front: the front half stands in.)
//!
//! The Escalation enemies don't attack here yet, so nothing comes at the shield but
//! his own rounds' blasts.

use bevy::gltf::{GltfMaterialExtras, GltfMaterialName};
use bevy::prelude::*;

use crate::abilities::{
    AbilityInbox, AbilityModels, BASHER, BLASTPROOF, BRUTE_FORCE, ClassAbility, End, Kind, Sockets,
};
use crate::character::{PaintCache, PaintMaterial};
use crate::levelfx::FxSystems;
use crate::net::JazzEvent;
use crate::sound::Sfx;
use crate::{OwnAnimation, Player, RemotePlayer};

/// BlockGroundSpeedMultiplier, and with Basher (times 1.5).
const BLOCK_SPEED: f32 = 0.5;
const BASHER_SPEED: f32 = 0.75;
/// DamageResourceMultiplier, Brute Force's multiplier on it, Blastproof's heal.
const DAMAGE_COST: f32 = 0.015;
const BRUTE_FORCE_COST: f32 = 0.5;
const BLASTPROOF_HEAL: f32 = 0.1;
/// KnockOutOfBlockPenaltyTime.
const BROKEN_SECS: f32 = 5.0;
/// The shield's clips (flak_shield.glb's, their RateScale in their lengths): equip
/// shown from SHOW_AT, drop hidden from HIDE_AT (TnAnimNotify_ToggleHidden).
const EQUIP: usize = 0;
const DROP: usize = 1;
const EQUIP_SECS: f32 = 0.6667;
const DROP_SECS: f32 = 0.1905;
const SHOW_AT: f32 = 0.0269 / 0.5;
const HIDE_AT: f32 = 0.2784 / 1.75;
/// The robot's break clip's length (robot_extras.txt's shield_break).
pub const BREAK_SECS: f32 = 1.0;

const SND_EQUIP: &str = "BL_CHR_GRIMLOCK.SHIELD_ANIM_EQUIP";
const SND_RETRACT: &str = "BL_CHR_GRIMLOCK.SHIELD_ANIM_RETRACT";
const SND_BREAK: &str = "BL_WPN_MELEE.SHIELD_ROBO_AUTOBOT_BREAK";
const FX_BREAK: &str = "Shield_Break_1A_FX";
/// The energon pane's glow: the team's energon (a team's own effects' cyan), this
/// bright.
const PANE_GLOW: f32 = 6.0;

/// The Ability event of a break (its effect where the shield was).
pub const WHAT_BREAK: u8 = 3;

/// A Jazz's shield: up or not, its model on his arm (and the bone it hangs on), the
/// clip it plays and how far in, how far his stance has gone over to the shield's,
/// and the seconds before he can raise it again (and since it broke).
#[derive(Default)]
pub(crate) struct ShieldState {
    pub up: bool,
    model: Option<(Entity, Entity)>,
    clip: Option<(usize, f32)>,
    pub stance: f32,
    pub broken_left: f32,
    pub broke: Option<f32>,
}

/// The shield's model under his arm: whose, its animation player, painted yet.
#[derive(Component)]
pub(crate) struct ShieldModel {
    jazz: Entity,
    animator: Option<Entity>,
    painted: bool,
}

/// Whether he may raise it (not while it is knocked out).
pub fn can_start(a: &ClassAbility) -> bool {
    a.shield.broken_left <= 0.0
}

/// His ground speed's multiplier with it up (not sprinting: abilities.rs).
pub fn speed(a: &ClassAbility) -> f32 {
    if a.has(BASHER) {
        BASHER_SPEED
    } else {
        BLOCK_SPEED
    }
}

/// A blow `amount` coming at him with the shield up (abilities.rs's hurt): from in
/// front it is stopped, costing the meter (and healing him with Blastproof);
/// otherwise it lands. Returns what lands.
pub fn block(
    player: &mut Player,
    amount: f32,
    hit: &crate::enemies::Hit,
    _feet: Option<Vec3>,
) -> f32 {
    let facing = Quat::from_rotation_y(player.heading) * Vec3::NEG_Z;
    // (The blow's way against his facing: from in front it comes back at him.)
    if hit.dir.with_y(0.0).dot(facing) >= 0.0 {
        return amount;
    }
    let a = &mut player.ability;
    let cost = DAMAGE_COST
        * if a.has(BRUTE_FORCE) {
            BRUTE_FORCE_COST
        } else {
            1.0
        };
    a.add(-amount * cost);
    if a.resource <= 0.0 {
        a.end(End::RanOut);
    }
    if a.has(BLASTPROOF) {
        crate::healbeam::heal(player, amount * BLASTPROOF_HEAL);
    }
    0.0
}

/// Every Jazz's shield: raised and lowered (his PC's from his key, the others' from
/// his snapshot), broken, its model on his arm with its clips, its sounds and effects,
/// and his stance easing over to the shield's and back.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_shield(
    mut commands: Commands,
    time: Res<Time>,
    models: Option<Res<AbilityModels>>,
    mut fx: ResMut<FxSystems>,
    mut sfx: ResMut<Sfx>,
    mut inbox: ResMut<AbilityInbox>,
    mut sockets: Sockets,
    mut players: Query<(Entity, &mut Player, Has<RemotePlayer>)>,
) {
    let dt = time.delta_secs();
    for (_, at, dir, _) in inbox.take(WHAT_BREAK) {
        fx.spawn(FX_BREAK, at, dir, &[]);
    }
    for (entity, mut player, remote) in &mut players {
        let player = &mut *player;
        let left = player.left_handed;
        if player.ability.kind != Kind::Shield {
            let s = &mut player.ability.shield;
            if let Some((m, _)) = s.model.take() {
                commands.entity(m).try_despawn();
            }
            s.up = false;
            s.stance = 0.0;
            continue;
        }
        let a = &mut player.ability;
        a.shield.broken_left = (a.shield.broken_left - dt).max(0.0);
        if let Some(t) = a.shield.broke.as_mut() {
            *t += dt;
            if *t > BREAK_SECS {
                a.shield.broke = None;
            }
        }
        if a.started {
            a.shield.up = true;
            a.shield.clip = Some((EQUIP, 0.0));
            if !remote {
                sfx.play(SND_EQUIP);
            }
        }
        if let Some(how) = a.ended {
            a.shield.up = false;
            if how == End::RanOut {
                // Broken: gone at once, knocked out for a while.
                a.shield.clip = None;
                a.shield.broken_left = BROKEN_SECS;
                a.shield.broke = Some(0.0);
                let at = a
                    .shield
                    .model
                    .and_then(|(m, _)| sockets.global(m))
                    .map(|g| g.translation());
                if !remote {
                    sfx.play(SND_BREAK);
                    if let Some(at) = at {
                        fx.spawn(FX_BREAK, at, Vec3::Y, &[]);
                        player.played.push(JazzEvent::Ability {
                            what: WHAT_BREAK,
                            at,
                            dir: Vec3::Y,
                            value: 0.0,
                        });
                    }
                }
            } else {
                a.shield.clip = Some((DROP, 0.0));
                if !remote {
                    sfx.play(SND_RETRACT);
                }
            }
        }
        let a = &mut player.ability;
        let target = if a.shield.up { 1.0 } else { 0.0 };
        a.shield.stance += (target - a.shield.stance) * (dt * 8.0).min(1.0);
        // Its clip running on.
        let mut shown = a.shield.up;
        if let Some((clip, t)) = a.shield.clip.as_mut() {
            *t += dt;
            match *clip {
                EQUIP => {
                    shown = *t >= SHOW_AT;
                    if *t >= EQUIP_SECS {
                        a.shield.clip = None;
                    }
                }
                _ => {
                    shown = *t < HIDE_AT;
                    if *t >= DROP_SECS {
                        a.shield.clip = None;
                    }
                }
            }
        }
        // Its model: on his arm's socket while it shows (put on afresh should his
        // robot be rebuilt or his gun change hands).
        let socket = if left {
            "ShieldSocket_R"
        } else {
            "ShieldSocket_L"
        };
        let place = sockets.socket(entity, socket);
        let Some(models) = models.as_ref() else {
            continue;
        };
        let want = shown || a.shield.clip.is_some();
        match (a.shield.model, place) {
            (Some((m, bone)), Some((b, _))) if bone == b && want => {
                commands.entity(m).try_insert(if shown {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
            (old, Some((bone, local))) if want => {
                if let Some((m, _)) = old {
                    commands.entity(m).try_despawn();
                }
                let Some(scene) = models.shield.clone() else {
                    continue;
                };
                let m = commands
                    .spawn((
                        scene,
                        local,
                        if shown {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        },
                        ShieldModel {
                            jazz: entity,
                            animator: None,
                            painted: false,
                        },
                        OwnAnimation,
                        ChildOf(bone),
                    ))
                    .id();
                a.shield.model = Some((m, bone));
            }
            (Some((m, _)), _) if !want => {
                commands.entity(m).try_despawn();
                a.shield.model = None;
            }
            _ => {}
        }
    }
}

/// The shields' models: their clips (as their Jazzes' shields have them), and their
/// paint in their Jazzes' looks, its energon pane glowing.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn shield_models(
    mut commands: Commands,
    models: Option<Res<AbilityModels>>,
    players: Query<&Player>,
    mut shields: Query<(Entity, &mut ShieldModel)>,
    children: Query<&Children>,
    mut animators: Query<&mut AnimationPlayer>,
    meshes: Query<(
        &MeshMaterial3d<StandardMaterial>,
        Option<&GltfMaterialExtras>,
        Option<&GltfMaterialName>,
    )>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut paints: ResMut<Assets<PaintMaterial>>,
    mut cache: ResMut<PaintCache>,
) {
    let Some(models) = models else {
        return;
    };
    for (root, mut model) in &mut shields {
        let Ok(player) = players.get(model.jazz) else {
            continue;
        };
        if model.animator.is_none() {
            model.animator = children
                .iter_descendants(root)
                .find(|e| animators.contains(*e));
            if let Some(a) = model.animator {
                commands
                    .entity(a)
                    .insert(AnimationGraphHandle(models.shield_graph.clone()));
            }
        }
        if !model.painted {
            let mut any = false;
            for e in children.iter_descendants(root) {
                let Ok((m, extras, name)) = meshes.get(e) else {
                    continue;
                };
                any = true;
                if name.is_some_and(|n| n.0 == "ShieldEnergon") {
                    // (The pane: its pattern lit in the team's energon.)
                    if let Some(mut pane) = standard.get(&m.0).cloned() {
                        let c = crate::character::SAME_TEAM_ENERGON * PANE_GLOW;
                        pane.emissive = LinearRgba::rgb(c.x, c.y, c.z);
                        let h = standard.add(pane);
                        commands.entity(e).insert(MeshMaterial3d(h));
                    }
                } else if extras.is_some_and(|x| x.value.contains("paint"))
                    && let Some(p) =
                        cache.get(&m.0, &player.look, false, true, &standard, &mut paints)
                {
                    commands
                        .entity(e)
                        .remove::<MeshMaterial3d<StandardMaterial>>()
                        .insert(MeshMaterial3d(p));
                }
            }
            model.painted = any;
        }
        // Its clip, held at where its Jazz's shield has it (folded away at the end of
        // its drop, unfolded otherwise).
        let Some(mut animation_player) = model.animator.and_then(|a| animators.get_mut(a).ok())
        else {
            continue;
        };
        let s = &player.ability.shield;
        let (equip, drop) = (models.shield_clips[EQUIP], models.shield_clips[DROP]);
        let (node, other, t, length) = match s.clip {
            Some((DROP, t)) => (drop, equip, t, DROP_SECS),
            Some((_, t)) => (equip, drop, t, EQUIP_SECS),
            None if s.up => (equip, drop, EQUIP_SECS, EQUIP_SECS),
            None => (drop, equip, DROP_SECS, DROP_SECS),
        };
        animation_player.stop(other);
        // (Held just short of its end: a clip seeked to its very end wraps round to
        // its start.)
        let active = animation_player.play(node);
        active
            .set_speed(0.0)
            .seek_to(t.clamp(0.0, (length - 1e-3).max(0.0)));
    }
}
