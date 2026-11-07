//! A Jazz's aim on his robot: the robots' anim tree's aim offset node
//! (TR_Shared_ANIMTREE_p.Robot_ANIMTREE's TnAnimNodeAimOffset, every class's),
//! which turns his upper body and raises or lowers his gun arm to where he aims,
//! over whatever his clips play: the game's own offsets for each of his bones
//! (robot_extras.txt's aim offset lines: tools/build_robot_extras.py), baked from
//! its aim poses (Aim_<stance>_<F/L/R>_<C/U/D>, Shared_ROBO_NAV_ANIM), put on his
//! bones as src/ai/aimoffset.rs puts an enemy's (AnimNodeAimOffset.GetBoneAtoms).
//!
//! Its aim is the pawn's (AimOffsetPct: where he looks from where he faces, a
//! quarter turn each way; x to his right, y up), eased in at the node's
//! InterpSpeed. Its profile is his stance's (StanceToProfileMappingOverrides):
//! shooting from the hip (his weapon's BaseStance, a heavy weapon's too) the
//! Default profile, the Aim_Shoot poses; aiming down the sights the FineAim
//! profile, the Aim_FineAim poses; hovering (the Hover Slam's hover pushes
//! STANCE_Hover) the Hover profile. Those two remap the pawn's aim
//! (RemapPawnAimRange): his whole aim up and down is the pawn's within its
//! PawnAimOffsetRange (the Default profile's, 54 degrees down to 62 up; the fine
//! aim's 90 down to 54 up), so that at the camera's limits his gun reaches the
//! poses' (some 50 degrees up and down from level).
//!
//! The mirror node over it (TnAnimNodeHandednessMirror) mirrors what it makes
//! with his gun in his left hand, its aim switched side to side
//! (SwitchAimWhenLeftHanded): here, its offsets go to the bones' partners on the
//! other side, reflected across the skeleton's middle, as his clips blend over
//! to their mirrored set (Player::mirror). The custom slots over it (the full
//! body's: a melee blow, a hit reaction; the support items' use over his upper
//! body) take its offsets off by their share.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::ai::aimoffset::offset_pose;
use crate::ai::clips::{AimComponent, AimOffsetData};
use crate::ai::pawn::stance;
use crate::ai::ue::finterp_to;
use crate::ragdoll::{Rig, RigState};
use crate::{JazzOf, RobotAnimState, RobotAnimations, RobotForm};

/// The least share of the node worth putting on his bones.
const RELEVANT: f32 = 1e-3;
/// How fast the node comes in and goes (as his additive layers blend: a share of
/// the gap a second), and how fast one profile's offsets give way to another's as
/// his stance changes (as his clips blend from one stance's to another's).
const STRENGTH_BLEND: f32 = 30.0;
const PROFILE_BLEND: f32 = 10.0;
/// The game's stances (ETnStance) that pick the robot's profiles.
pub const GUN_ARM_SHOOT_FROM_HIP: u8 = stance::GUN_ARM_SHOOT_FROM_HIP;
pub const GUN_ARM_FINE_AIM: u8 = 12;
pub const HOVER: u8 = 35;
pub const HEAVY_GUN_ARM_SHOOT_FROM_HIP: u8 = 37;

/// The node, from robot_extras.txt (None before it is built).
pub fn load() -> Option<AimOffsetData> {
    let path = crate::assets_dir().join(crate::ROBOT_EXTRAS_FILE);
    let node = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| AimOffsetData::read(&text));
    if node.is_none() {
        info!(
            "{} has no aim offset node: Jazz's upper body doesn't follow his aim",
            crate::ROBOT_EXTRAS_FILE
        );
    }
    node
}

/// A Jazz's aim offset node as his animation leaves it each frame (main.rs's
/// animate_jazz): its aim, eased in; how much of it his bones take; its profile
/// and the one it is taking over from (and how far it has: 1 all of it); and how
/// far his pose has blended over to its mirror image.
#[derive(Clone, Copy, Debug, Default)]
pub struct AimNode {
    pub aim: Vec2,
    pub strength: f32,
    pub profile: usize,
    pub from: usize,
    pub fade: f32,
    pub mirror: f32,
}

impl AimNode {
    /// A frame of the node: its aim towards the pawn's `want` (switched side to
    /// side with his gun in his left hand: `left`) at the node's InterpSpeed, its
    /// profile `stance`'s, its share towards `on` and its mirroring `mirror`.
    pub fn update(
        &mut self,
        node: &AimOffsetData,
        mut want: Vec2,
        stance: u8,
        (on, left, mirror): (f32, bool, f32),
        dt: f32,
    ) {
        if node.switch_left && left {
            want.x = -want.x;
        }
        let want = want.clamp(Vec2::splat(-1.0), Vec2::splat(1.0));
        self.aim = Vec2::new(
            finterp_to(self.aim.x, want.x, dt, node.interp_speed),
            finterp_to(self.aim.y, want.y, dt, node.interp_speed),
        );
        let profile = node.profile_for(stance);
        if profile != self.profile {
            // (Back to the one it was leaving: from as far as that had gone.)
            self.fade = if profile == self.from {
                1.0 - self.fade
            } else {
                0.0
            };
            self.from = self.profile;
            self.profile = profile;
        }
        self.fade += (1.0 - self.fade) * (dt * PROFILE_BLEND).min(1.0);
        if self.fade > 1.0 - RELEVANT {
            self.fade = 1.0;
            self.from = profile;
        }
        self.strength += (on - self.strength) * (dt * STRENGTH_BLEND).min(1.0);
        self.mirror = mirror;
    }

    /// Each bone's offset (by its place in the rig: `places`, each profile's
    /// bones' places and their partners' on the other side), the node's share of
    /// them and its mirroring in.
    fn offsets(
        &self,
        node: &AimOffsetData,
        places: &[Vec<(Option<usize>, Option<usize>)>],
    ) -> HashMap<usize, (Quat, Vec3)> {
        // (Each bone's offsets' weighted sum, its share of them and its moves'.)
        let mut sums: HashMap<usize, (Vec4, f32, Vec3)> = HashMap::new();
        let mut add = |i: Option<usize>, (turn, shift): (Quat, Vec3), w: f32| {
            let Some(i) = i else { return };
            let s = sums.entry(i).or_insert((Vec4::ZERO, 0.0, Vec3::ZERO));
            // (The turn's half nearer no turn, so that they add up.)
            let q = Vec4::from(if turn.w < 0.0 { -turn } else { turn });
            s.0 += q * w;
            s.1 += w;
            s.2 += shift * w;
        };
        let profiles = [(self.from, 1.0 - self.fade), (self.profile, self.fade)];
        for (k, share) in profiles {
            let (Some(p), Some(at)) = (node.profiles.get(k), places.get(k)) else {
                continue;
            };
            if share < RELEVANT {
                continue;
            }
            let aim = p.safe_aim(self.aim);
            for (comp, &(place, partner)) in p.bones.iter().zip(at) {
                let offset = comp.offset_at(aim);
                if self.mirror < 1.0 - RELEVANT {
                    add(place, offset, share * (1.0 - self.mirror));
                }
                if self.mirror > RELEVANT {
                    add(partner, mirrored(offset), share * self.mirror);
                }
            }
        }
        sums.into_iter()
            .map(|(i, (q, w, shift))| {
                // (What the profiles leave of the bone, unturned.)
                let q = q + Vec4::from(Quat::IDENTITY) * (1.0 - w).max(0.0);
                let turn = Quat::from_vec4(q).normalize();
                (
                    i,
                    (
                        Quat::IDENTITY.slerp(turn, self.strength),
                        shift * self.strength,
                    ),
                )
            })
            .collect()
    }
}

/// An offset reflected across the skeleton's middle (its Z axis runs left to
/// right: tools/mirror_anims.py's S), for the bone's partner on the other side:
/// a turn about the mirrored axis the other way, a move mirrored.
pub fn mirrored((turn, shift): (Quat, Vec3)) -> (Quat, Vec3) {
    (
        Quat::from_xyzw(-turn.x, -turn.y, turn.z, turn.w),
        Vec3::new(shift.x, shift.y, -shift.z),
    )
}

/// A bone's partner on the other side (L_* and R_*; a middle bone its own).
pub fn partner(bone: &str) -> String {
    if let Some(rest) = bone.strip_prefix("L_") {
        format!("R_{rest}")
    } else if let Some(rest) = bone.strip_prefix("R_") {
        format!("L_{rest}")
    } else {
        bone.to_string()
    }
}

/// The node's view of a robot: its rig (its base), each profile's bones' places
/// in it and their partners', and what it did to the bones last frame (each one,
/// its place before and after).
pub struct Matched {
    base: Entity,
    places: Vec<Vec<(Option<usize>, Option<usize>)>>,
    written: Vec<(Entity, Transform, Transform)>,
}

impl Matched {
    fn new(node: &AimOffsetData, rig: &Rig) -> Matched {
        let by_name: HashMap<&str, usize> = rig
            .names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), i))
            .collect();
        let place = |c: &AimComponent| {
            (
                by_name.get(&*c.bone).copied(),
                by_name.get(partner(&c.bone).as_str()).copied(),
            )
        };
        Matched {
            base: rig.base,
            places: node
                .profiles
                .iter()
                .map(|p| p.bones.iter().map(place).collect())
                .collect(),
            written: Vec::new(),
        }
    }
}

/// Each Jazz's aim offset node on his robot's bones, once his clips have posed
/// them (before his ragdoll takes them, and before the transforms are carried
/// down). A bone no clip posed again keeps last frame's place: what the node did
/// to it is undone first.
pub fn apply_robot_aim(
    anims: Option<Res<RobotAnimations>>,
    robots: Query<(Entity, &JazzOf, &Rig), With<RobotForm>>,
    states: Query<&RobotAnimState>,
    mut bones: Query<&mut Transform>,
    mut matched: Local<HashMap<Entity, Matched>>,
) {
    let Some(node) = anims.as_ref().and_then(|a| a.aim.as_ref()) else {
        return;
    };
    let mut seen = Vec::new();
    for (robot, of, rig) in &robots {
        seen.push(robot);
        let m = matched
            .entry(robot)
            .or_insert_with(|| Matched::new(node, rig));
        for (b, before, after) in m.written.drain(..) {
            if let Ok(mut t) = bones.get_mut(b)
                && *t == after
            {
                *t = before;
            }
        }
        if m.base != rig.base {
            *m = Matched::new(node, rig);
        }
        let Ok(state) = states.get(of.0) else {
            continue;
        };
        let aim = &state.aim;
        if aim.strength < RELEVANT || !matches!(rig.state, RigState::Animated) {
            continue;
        }
        let offsets = aim.offsets(node, &m.places);
        if offsets.is_empty() {
            continue;
        }
        let mut locals: Vec<Transform> = rig
            .bones
            .iter()
            .map(|&b| bones.get(b).copied().unwrap_or_default())
            .collect();
        for i in offset_pose(&mut locals, &rig.parents, &offsets) {
            if let Ok(mut t) = bones.get_mut(rig.bones[i]) {
                let before = *t;
                *t = locals[i];
                m.written.push((rig.bones[i], before, *t));
            }
        }
    }
    matched.retain(|e, _| seen.contains(e));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> Option<AimOffsetData> {
        let text =
            std::fs::read_to_string(crate::assets_dir().join(crate::ROBOT_EXTRAS_FILE)).ok()?;
        AimOffsetData::read(&text)
    }

    /// The robot tree's node as the game has it: eased in, switched side to side
    /// left-handed; shooting from the hip (a heavy weapon too) the Aim_Shoot poses'
    /// profile, fine aim the Aim_FineAim poses', the hover its own; the first two
    /// remapping the pawn's aim up and down to the ends of their ranges.
    #[test]
    fn the_robots_aim_offset_node_is_the_games() {
        let Some(node) = node() else {
            return;
        };
        assert_eq!(node.interp_speed, 12.0);
        assert!(node.switch_left);
        for (s, name) in [
            (GUN_ARM_SHOOT_FROM_HIP, "STANCE_GunArmShootFromHip"),
            (GUN_ARM_FINE_AIM, "STANCE_GunArmFineAim"),
            (HOVER, "STANCE_Hover"),
            (
                HEAVY_GUN_ARM_SHOOT_FROM_HIP,
                "STANCE_HeavyGunArmShootFromHip",
            ),
        ] {
            assert_eq!(s, stance::number(name), "{name}");
        }
        let name = |s: u8| node.profiles[node.profile_for(s)].name.as_str();
        assert_eq!(name(GUN_ARM_SHOOT_FROM_HIP), "Default");
        assert_eq!(name(HEAVY_GUN_ARM_SHOOT_FROM_HIP), "Default");
        assert_eq!(name(GUN_ARM_FINE_AIM), "FineAim");
        assert_eq!(name(HOVER), "Hover");
        let shoot = &node.profiles[0];
        // The gun arm and the spine, not the left arm.
        let bones: Vec<&str> = shoot.bones.iter().map(|b| &*b.bone).collect();
        for b in [
            "C_Spine01_Lumbar01_XB",
            "C_Spine04_Head_XB",
            "R_Arm03_Elbow_XB",
        ] {
            assert!(bones.contains(&b), "{b}");
        }
        assert!(!bones.iter().any(|b| b.starts_with("L_Arm")));
        // Its whole aim up 0.69 of a quarter turn (62 degrees), down 0.6 (54).
        let at =
            |p: &crate::ai::clips::AimOffsetProfile, x: f32, y: f32| p.safe_aim(Vec2::new(x, y));
        assert!((at(shoot, 0.0, 0.69).y - 1.0).abs() < 1e-5);
        assert!((at(shoot, 0.0, 0.345).y - 0.5).abs() < 1e-5);
        assert!((at(shoot, 0.0, -0.6).y + 1.0).abs() < 1e-5);
        assert!((at(shoot, 0.0, -0.3).y + 0.5).abs() < 1e-5);
        assert_eq!(at(shoot, 0.0, 0.9).y, 1.0);
        assert!((at(shoot, -0.5, 0.0).x + 0.5).abs() < 1e-5);
        let fine = &node.profiles[1];
        assert!((at(fine, 0.0, 0.6).y - 1.0).abs() < 1e-5);
        assert!((at(fine, 0.0, -1.0).y + 1.0).abs() < 1e-5);
        // Looking up, the elbow's offset is the up pose's.
        let elbow = shoot
            .bones
            .iter()
            .find(|b| &*b.bone == "R_Arm03_Elbow_XB")
            .unwrap();
        let (up, _) = elbow.offset_at(at(shoot, 0.0, 0.69));
        assert!(up.angle_between(elbow.offsets[3].0) < 1e-4);
        assert!(up.angle_between(Quat::IDENTITY) > 0.3);
    }

    /// The node eases its aim in, fades from one profile to the next, and comes in
    /// and goes as it is on and off; left-handed, its aim is switched side to side.
    #[test]
    fn the_node_eases_its_aim_and_profiles() {
        let Some(node) = node() else {
            return;
        };
        let mut a = AimNode::default();
        let dt = 1.0 / 60.0;
        a.update(
            &node,
            Vec2::new(0.5, 0.4),
            GUN_ARM_SHOOT_FROM_HIP,
            (1.0, false, 0.0),
            dt,
        );
        // (12 a second: a fifth of the way in a frame.)
        assert!((a.aim.x - 0.1).abs() < 1e-4 && (a.aim.y - 0.08).abs() < 1e-4);
        for _ in 0..60 {
            a.update(
                &node,
                Vec2::new(0.5, 0.4),
                GUN_ARM_SHOOT_FROM_HIP,
                (1.0, false, 0.0),
                dt,
            );
        }
        assert!((a.aim - Vec2::new(0.5, 0.4)).length() < 1e-3);
        assert!(a.strength > 0.99 && a.fade == 1.0 && a.profile == 0);
        a.update(
            &node,
            Vec2::new(0.5, 0.4),
            GUN_ARM_FINE_AIM,
            (1.0, false, 0.0),
            dt,
        );
        assert_eq!((a.from, a.profile), (0, 1));
        assert!(a.fade > 0.1 && a.fade < 0.2);
        // (Back before it is done: from where it had got to.)
        let gone = a.fade;
        a.update(
            &node,
            Vec2::new(0.5, 0.4),
            GUN_ARM_SHOOT_FROM_HIP,
            (1.0, false, 0.0),
            dt,
        );
        assert_eq!((a.from, a.profile), (1, 0));
        assert!(a.fade > 1.0 - gone);
        for _ in 0..120 {
            a.update(
                &node,
                Vec2::new(0.5, 0.4),
                GUN_ARM_SHOOT_FROM_HIP,
                (0.0, true, 1.0),
                dt,
            );
        }
        assert!(a.strength < 1e-3 && a.fade == 1.0 && a.from == 0);
        assert!(
            (a.aim.x + 0.5).abs() < 1e-3,
            "left-handed, aiming right is aiming left"
        );
    }

    /// Mirrored, a bone's offset goes to its partner reflected: the head's turn to
    /// the right becomes a turn to the left, and the gun arm's offsets land on the
    /// left arm.
    #[test]
    fn mirrored_offsets_go_to_the_other_side() {
        let Some(node) = node() else {
            return;
        };
        let p = &node.profiles[0];
        // (A rig of the profile's bones and their partners: their places by name.)
        let names: Vec<String> = p
            .bones
            .iter()
            .flat_map(|b| [b.bone.to_string(), partner(&b.bone)])
            .collect();
        let place = |n: &str| names.iter().position(|m| m == n);
        let places = vec![
            p.bones
                .iter()
                .map(|b| (place(&b.bone), place(&partner(&b.bone))))
                .collect::<Vec<_>>(),
        ];
        let mut a = AimNode {
            aim: Vec2::new(1.0, 0.0),
            strength: 1.0,
            fade: 1.0,
            ..default()
        };
        let plain = a.offsets(&node, &places);
        a.mirror = 1.0;
        let flipped = a.offsets(&node, &places);
        let elbow = place("R_Arm03_Elbow_XB").unwrap();
        let left_elbow = place("L_Arm03_Elbow_XB").unwrap();
        assert!(plain.contains_key(&elbow) && !plain.contains_key(&left_elbow));
        assert!(flipped.contains_key(&left_elbow) && !flipped.contains_key(&elbow));
        let head = place("C_Spine04_Head_XB").unwrap();
        // (The head turns about the up axis (y) the other way.)
        let yaw = |q: Quat| q.to_euler(EulerRot::YXZ).0;
        assert!(yaw(plain[&head].0) * yaw(flipped[&head].0) < 0.0);
        assert!((yaw(plain[&head].0) + yaw(flipped[&head].0)).abs() < 1e-4);
        let (turn, shift) = mirrored((Quat::from_rotation_y(0.5), Vec3::new(1.0, 2.0, 3.0)));
        assert!(turn.angle_between(Quat::from_rotation_y(-0.5)) < 1e-5);
        assert_eq!(shift, Vec3::new(1.0, 2.0, -3.0));
        assert_eq!(partner("R_ClavRobo01_XT"), "L_ClavRobo01_XT");
        assert_eq!(partner("C_Spine04_Head_XB"), "C_Spine04_Head_XB");
    }
}
