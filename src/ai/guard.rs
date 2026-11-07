//! The Guard's two robot forms and its scanner (TnGuardForms, its pawn's
//! service, and the TnScannerBeamComponent it hangs on the Guard's head):
//!
//! * **Investigate form** (form 0, STANCE_Patrol): slow, its arm cannon away, its
//!   aim held in to 30% of its turn (its aim animator's AimOffsetScale 0.3), seeing
//!   from its scanner head (C_AlertHead01Robo01_XT), half the damage of a blow
//!   (InvestigateDamageMultiplier) but twice that to the head (its
//!   BoneDamageOverrides' and HeadShotBoneNames' InvestigateHeadshot), alert level
//!   1, and its scanner on.
//! * **Battle form** (form 1, STANCE_GunArmShootFromHip): fast, its Guardian
//!   Blaster (BattleModeWeapon) in hand, its aim free, seeing from its back
//!   (C_BackpackRobo01_XT), 35% of a blow (BattleDamageMultiplier), alert level 2,
//!   the scanner off.
//!
//! A change of form (TnChangeRobotFormOperator) prepares it as its animation
//! starts (PrepareToActivateForm: the stance, the gun, the damage, WS_CurrentForm,
//! the aim node off) and activates it as it ends (ActivateForm). Its physics assets
//! (the two forms' collision for the death's ragdoll) aren't the demo's.
//!
//! The scanner: a vertical fan of light out of ScannerSocket (along the socket,
//! level: its roll taken off), InvestigateScanDistance long (50 m) and
//! InvestigateScanVerticalAngle wide (60°), its loop (GUARD_SCANNER_LP_START) going
//! while it is on. Every frame (its Tick's FireDecloakBeam) each pawn of the
//! other side within its reach whose cylinder the fan's plane cuts, within its
//! spread, and in the Guard's line of sight is exposed (a cloak given away:
//! ExposeSelf), hears the decloak (ABILITY_CLOAK_FORCE_DECLOAK), is jammed
//! (TnBuffAbilityJammed: 10 s without his ability) and is seen at once (its
//! vision sensor's SeeInstantly). The fan is drawn by scanner.rs; its back light
//! (TnScannerBeamComponent's: BackLightBrightness 80 in Colors.Green, 0 128 0,
//! BackLightOffset 150 uu along it, BackLightRadius 500, lighting only the
//! players: PlayerOnly) isn't.

use bevy::prelude::*;

use super::brain::{Brain, Ctx};
use super::data::Behaviour;
use super::htn;
use super::pawn::{Pawn, stance};
use super::ue;
use super::weapon;

/// The forms (EGuardForm).
pub const FORM_INVESTIGATE: i32 = 0;
pub const FORM_BATTLE: i32 = 1;

/// TnGuardForms' defaults: the view bones and the scanner head's headshot.
const INVESTIGATE_VIEW_BONE: &str = "C_AlertHead01Robo01_XT";
const BATTLE_VIEW_BONE: &str = "C_BackpackRobo01_XT";
const INVESTIGATE_HEADSHOT: (&str, f32) = ("C_AlertHead01Robo01_XT", 2.0);
/// The aim animator's AimOffsetScale in each form (ActivateForm).
const INVESTIGATE_AIM_SCALE: f32 = 0.3;
const BATTLE_AIM_SCALE: f32 = 1.0;

/// The socket the scanner hangs on.
pub const SCANNER_SOCKET: &str = "ScannerSocket";
/// Its loop (_ScannerLoopSound, the Guard's sound set's), let go over a second
/// (StopLoopingSoundEvent(.., 1.0)).
pub const SCANNER_LOOP_EVENT: &str = "GUARD_SCANNER_LP_START";
pub const SCANNER_LOOP_FADE: f32 = 1.0;
/// What a player it exposes hears (_DecloakSound, ABILITY_CLOAK_FORCE_DECLOAK: the
/// cue the players' sound sets give that event, mp_shared.xxx's).
pub const DECLOAK_CUE: &str = "BL_CHR_AI_GUARD.GUARD_SCANNER_DECLOAK";

/// The Guard's forms service and its scanner.
#[derive(Clone, Debug)]
pub struct GuardForms {
    pub battle_damage_multiplier: f32,
    pub investigate_damage_multiplier: f32,
    pub scanner: Scanner,
}

/// Its scanner beam.
#[derive(Clone, Debug, Default)]
pub struct Scanner {
    /// On its head (ActivateScannerBeam), or not (DeactivateScannerBeam).
    pub attached: bool,
    /// Its ProjectionParameters: how far it reaches and its spread (degrees).
    pub distance: f32,
    pub spread_angle: f32,
    /// Where it is and the way it points this frame (Unreal's space).
    pub at: Vec3,
    pub dir: Vec3,
}

impl GuardForms {
    /// TnGuardForms' constructor: its blueprint's values, its scanner made
    /// (InvestigateScanDistance, InvestigateScanVerticalAngle).
    pub fn new(b: &Behaviour) -> GuardForms {
        GuardForms {
            battle_damage_multiplier: b.f("BattleDamageMultiplier", 1.0),
            investigate_damage_multiplier: b.f("InvestigateDamageMultiplier", 1.0),
            scanner: Scanner {
                attached: false,
                distance: b.i("InvestigateScanDistance", 0) as f32,
                spread_angle: b.i("InvestigateScanVerticalAngle", 0) as f32,
                at: Vec3::ZERO,
                dir: Vec3::X,
            },
        }
    }
}

/// get_Form: its form (WS_CurrentForm).
pub fn form(b: &Brain) -> i32 {
    b.core.ws(htn::WS_CURRENT_FORM)
}

/// PrepareToActivateForm: what a change to `target` does as it starts.
pub fn prepare_to_activate_form(b: &mut Brain, p: &mut Pawn, target: i32) {
    let Some(g) = p.guard.as_mut() else { return };
    let battle = g.battle_damage_multiplier;
    p.stance.clear();
    if target == FORM_INVESTIGATE {
        p.stance.push(stance::PATROL);
        weapon::unequip(b, p);
    } else {
        p.stance.push(stance::GUN_ARM_SHOOT_FROM_HIP);
        g.scanner.attached = false;
        p.damage_multiplier = battle;
        let (bone, scale) = INVESTIGATE_HEADSHOT;
        p.bone_damage
            .retain(|(n, s)| !(&**n == bone && *s == scale));
        if let Some(i) = p.head_bones.iter().position(|n| &**n == bone) {
            p.head_bones.remove(i);
        }
        // EquipWeapon(BattleModeWeapon): its blueprint's gun, its pawn's.
        let data = p.weapon.data.clone();
        weapon::equip(p, data);
        b.core.set_ws(htn::WS_ALERT_LEVEL, 2);
    }
    b.core.set_ws(htn::WS_CURRENT_FORM, target);
    p.aim_node_enabled = false;
}

/// ActivateForm: what a change to `target` does as it ends.
pub fn activate_form(b: &mut Brain, p: &mut Pawn, target: i32) {
    p.aim_node_enabled = true;
    let Some(g) = p.guard.as_mut() else { return };
    if target == FORM_INVESTIGATE {
        let investigate = g.investigate_damage_multiplier;
        g.scanner.attached = true;
        p.aim_offset_scale = INVESTIGATE_AIM_SCALE;
        p.view_bone = Some(INVESTIGATE_VIEW_BONE.into());
        p.damage_multiplier = investigate;
        let (bone, scale) = INVESTIGATE_HEADSHOT;
        p.bone_damage.push((bone.into(), scale));
        p.head_bones.push(bone.into());
        b.core.set_ws(htn::WS_ALERT_LEVEL, 1);
    } else {
        p.aim_offset_scale = BATTLE_AIM_SCALE;
        p.view_bone = Some(BATTLE_VIEW_BONE.into());
    }
}

/// A player the scanner caught this frame (FireDecloakBeam's): exposed, jammed by
/// the Guard, and seen.
#[derive(Clone, Copy, Debug)]
pub struct Scanned {
    pub player: Entity,
    pub by: Entity,
}

/// The scanner's frame (TnScannerBeamComponent.Tick): UpdateRotation (along its
/// socket, level) and FireDecloakBeam. The players it catches, for the game to
/// expose and jam (Shared::scanned); each one is seen at once.
pub fn tick_scanner(b: &mut Brain, ctx: &mut Ctx) {
    let p = &mut *ctx.pawn;
    let Some((at, axes)) = p.socket_frame(SCANNER_SOCKET) else {
        return;
    };
    let Some(g) = p.guard.as_mut() else { return };
    if !g.scanner.attached {
        return;
    }
    // (Its rotation with no roll: its X along the socket's.)
    let dir = ue::normal(axes[0]);
    g.scanner.at = at;
    g.scanner.dir = dir;
    let (distance, spread) = (g.scanner.distance, g.scanner.spread_angle);
    // FindPawnsIntersectingVerticalPieSlice.
    let normal = Vec3::Z.cross(dir);
    let cos_half = (spread * 0.5 * ue::DEG_TO_RAD).cos();
    let mut caught = Vec::new();
    for pl in &ctx.view.players {
        if !pl.alive || pl.team == b.core.team {
            continue;
        }
        let to = pl.loc - at;
        if to.length() > distance {
            continue;
        }
        if to.dot(normal).abs() > pl.radius {
            continue;
        }
        if ue::normal(to).dot(dir) < cos_half {
            continue;
        }
        caught.push(pl.entity);
    }
    for e in caught {
        // (LinesOfSightManager.StaticHasLineOfSight: the Guard's eyes on him.)
        if !super::sensors::line_of_sight(ctx, e, false) {
            continue;
        }
        ctx.shared.scanned.push(Scanned {
            player: e,
            by: b.core.entity,
        });
        super::sensors::see_instantly(b, ctx.view, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The scanner's slice: the fan's plane through a pawn's cylinder, and its
    /// spread up and down.
    #[test]
    fn the_scanner_catches_what_its_fan_cuts() {
        let at = Vec3::new(0.0, 0.0, 300.0);
        let dir = Vec3::X;
        let normal = Vec3::Z.cross(dir);
        let cos_half = (60.0f32 * 0.5 * ue::DEG_TO_RAD).cos();
        let cut = |loc: Vec3, radius: f32| {
            let to = loc - at;
            to.length() <= 5000.0
                && to.dot(normal).abs() <= radius
                && ue::normal(to).dot(dir) >= cos_half
        };
        // Ahead, level with it, and off to the side by less than his radius.
        assert!(cut(Vec3::new(2000.0, 0.0, 300.0), 60.0));
        assert!(cut(Vec3::new(2000.0, 50.0, 300.0), 60.0));
        // Off to the side by more, behind it, too far, or too far below.
        assert!(!cut(Vec3::new(2000.0, 100.0, 300.0), 60.0));
        assert!(!cut(Vec3::new(-2000.0, 0.0, 300.0), 60.0));
        assert!(!cut(Vec3::new(6000.0, 0.0, 300.0), 60.0));
        assert!(!cut(Vec3::new(500.0, 0.0, -300.0), 60.0));
    }
}
