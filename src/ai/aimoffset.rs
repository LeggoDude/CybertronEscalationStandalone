//! The anim trees' aim offset nodes (UE3's AnimNodeAimOffset, its offsets baked
//! from its aim poses: the Snipers' insects', the Guard's, the Leaper's) on the
//! enemies' models. The tree's node (tree.rs) gives its profile and its aim (the
//! pawn's AimOffsetPct, its aim animator's: mod.rs's update_aim_offset); here, once
//! the clips have posed the model, each of the profile's bones is turned and moved
//! by its offset at that aim (AnimNodeAimOffset.GetBoneAtoms: from the top of the
//! skeleton down, each bone's place in the mesh's space from its parent's as the
//! node left it, its offset's turn put in front of its own about its place and its
//! offset's move added to its place, then back into its parent's space).
//!
//! The custom slots over the node in the tree play their animations over what it
//! makes: here their shares take its offsets off the bones they play (a masked
//! slot's off its bones only).
//!
//! A bone no clip poses keeps its place from one frame to the next: what the node
//! did to it is undone before it is done again.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::math::Affine3A;
use bevy::prelude::*;

use super::Ai;
use super::clips::Clips;
use super::tree::Layer;
use crate::enemies::Enemy;
use crate::ragdoll::Rig;

/// The least offset weight worth putting on a bone.
const RELEVANT: f32 = 1e-3;

/// What the node knows of a model: the set and skeleton it matched its bones to
/// (the set's address, the rig's base), each profile's bones' places in the rig,
/// the masked slots' bones over the node (by their layers), and what it did last
/// frame (each bone, its place before and after).
pub struct Matched {
    key: (usize, Entity),
    bones: Vec<Vec<Option<usize>>>,
    masks: Vec<(Layer, HashSet<usize>)>,
    written: Vec<(Entity, Transform, Transform)>,
}

impl Matched {
    fn new(c: &Arc<Clips>, rig: &Rig) -> Matched {
        let by_name: HashMap<&str, usize> = rig
            .names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), i))
            .collect();
        let bones = c
            .aim_offset
            .as_ref()
            .map(|ao| {
                ao.profiles
                    .iter()
                    .map(|p| {
                        p.bones
                            .iter()
                            .map(|b| by_name.get(&*b.bone).copied())
                            .collect()
                    })
                    .collect()
            })
            .unwrap_or_default();
        let masks = c
            .masks
            .iter()
            .map(|(l, m)| {
                let set = m
                    .paths
                    .iter()
                    .filter_map(|path| path.last())
                    .filter_map(|n| by_name.get(n.as_str()).copied())
                    .collect();
                (*l, set)
            })
            .collect();
        Matched {
            key: (Arc::as_ptr(c) as usize, rig.base),
            bones,
            masks,
            written: Vec::new(),
        }
    }
}

/// Each living AI's aim offset node on its model's bones (after its clips have
/// posed them, before the ragdolls take theirs and the transforms are carried
/// down).
pub fn apply_aim_offsets(
    ais: Query<(Entity, &Ai, &Enemy, &Rig)>,
    mut bones: Query<&mut Transform>,
    mut matched: Local<HashMap<Entity, Matched>>,
) {
    let mut seen = Vec::new();
    for (e, ai, enemy, rig) in &ais {
        let c = &ai.clips;
        let Some(ao) = c.aim_offset.as_ref() else {
            continue;
        };
        seen.push(e);
        let m = matched.entry(e).or_insert_with(|| Matched {
            key: (0, Entity::PLACEHOLDER),
            bones: Vec::new(),
            masks: Vec::new(),
            written: Vec::new(),
        });
        // (Last frame's offsets off the bones no clip posed again.)
        for (b, before, after) in m.written.drain(..) {
            if let Ok(mut t) = bones.get_mut(b)
                && *t == after
            {
                *t = before;
            }
        }
        if !enemy.alive() {
            continue;
        }
        let Some((profile, aim)) = ai.pawn.pose.aim_offset else {
            continue;
        };
        if m.key != (Arc::as_ptr(c) as usize, rig.base) {
            *m = Matched::new(c, rig);
        }
        let Some(p) = ao.profiles.get(profile as usize) else {
            continue;
        };
        let Some(places) = m.bones.get(profile as usize) else {
            continue;
        };
        let aim = p.safe_aim(aim);
        // The slots over the node: their shares of the bones they play.
        let pose = &ai.pawn.pose;
        let over = c.layer_index(Layer::Aim).map_or(0, |k| k + 1);
        let mut whole = 1.0;
        let mut masked: Vec<(&HashSet<usize>, f32)> = Vec::new();
        for &l in c.layers.iter().skip(over) {
            if !l.overrides() {
                continue;
            }
            let keep = (1.0 - pose.shares[l as usize]).clamp(0.0, 1.0);
            match m.masks.iter().find(|(ml, _)| *ml == l) {
                Some((_, set)) => masked.push((set, keep)),
                None => whole *= keep,
            }
        }
        if whole < RELEVANT {
            continue;
        }
        // Each bone's offset (by its place in the rig).
        let mut offsets: HashMap<usize, (Quat, Vec3)> = HashMap::new();
        for (comp, place) in p.bones.iter().zip(places) {
            let Some(i) = *place else { continue };
            let mut w = whole;
            for (set, keep) in &masked {
                if set.contains(&i) {
                    w *= keep;
                }
            }
            if w < RELEVANT {
                continue;
            }
            let (turn, shift) = comp.offset_at(aim);
            offsets.insert(i, (Quat::IDENTITY.slerp(turn, w), shift * w));
        }
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

/// AnimNodeAimOffset.GetBoneAtoms on a skeleton's bones (their places in their
/// parents' spaces, in the hierarchy's order: `parents`): from the top down, each
/// bone's place in the mesh's space from its parent's as it is left, an offset
/// bone's turn put in front of its own about its place and its move added to its
/// place, and that put back in its parent's space. The offset bones, changed.
pub fn offset_pose(
    locals: &mut [Transform],
    parents: &[Option<usize>],
    offsets: &HashMap<usize, (Quat, Vec3)>,
) -> Vec<usize> {
    let mut changed = Vec::new();
    let mut mesh: Vec<Affine3A> = Vec::with_capacity(locals.len());
    for (i, local) in locals.iter_mut().enumerate() {
        let parent = parents
            .get(i)
            .copied()
            .flatten()
            .map_or(Affine3A::IDENTITY, |k| mesh[k]);
        let here = parent * local.compute_affine();
        let Some(&(turn, shift)) = offsets.get(&i) else {
            mesh.push(here);
            continue;
        };
        let at = Vec3::from(here.translation);
        let moved = Affine3A::from_translation(at + shift)
            * Affine3A::from_quat(turn)
            * Affine3A::from_translation(-at)
            * here;
        let (_, rotation, translation) = (parent.inverse() * moved).to_scale_rotation_translation();
        local.rotation = rotation.normalize();
        local.translation = translation;
        changed.push(i);
        mesh.push(moved);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A turn of the top bone in the mesh's space carries its child round with it;
    /// a move of the child moves it alone; an offset turns a bone about its own
    /// place.
    #[test]
    fn offsets_turn_and_move_bones_in_the_mesh() {
        // (A root at the origin, a spine a metre up, a head a metre above that.)
        let mut locals = vec![
            Transform::IDENTITY,
            Transform::from_xyz(0.0, 1.0, 0.0),
            Transform::from_xyz(0.0, 1.0, 0.0),
        ];
        let parents = [None, Some(0), Some(1)];
        let quarter = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let offsets = HashMap::from([(1, (quarter, Vec3::ZERO)), (2, (Quat::IDENTITY, Vec3::X))]);
        let changed = offset_pose(&mut locals, &parents, &offsets);
        assert_eq!(changed, vec![1, 2]);
        let world = |locals: &[Transform], i: usize| {
            let mut m = Affine3A::IDENTITY;
            let mut chain = vec![i];
            while let Some(p) = parents[*chain.last().unwrap()] {
                chain.push(p);
            }
            for k in chain.into_iter().rev() {
                m = m * locals[k].compute_affine();
            }
            m
        };
        // The spine stays where it was, turned a quarter about z.
        let spine = world(&locals, 1);
        assert!((Vec3::from(spine.translation) - Vec3::Y).length() < 1e-5);
        // The head, a metre along the turned spine's y (-x), then moved a metre
        // along x in the mesh's space: back over the spine.
        let head = world(&locals, 2);
        assert!(
            (Vec3::from(head.translation) - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-5,
            "the head at {:?}",
            head.translation
        );
    }

    /// The insects' aim offsets (sniper_alt's): none aiming straight out, the
    /// corner poses' own at the corners, between them in between.
    #[test]
    fn the_insects_offsets_follow_the_aim() {
        let Some(c) = super::super::clips::Clips::load(&crate::assets_dir(), "sniper_alt") else {
            return;
        };
        let ao = c.aim_offset.as_ref().expect("the insect's aim offset node");
        assert_eq!(ao.interp_speed, 0.0);
        let p = &ao.profiles[0];
        assert_eq!(p.name, "Crawling");
        assert_eq!(p.bones.len(), 26);
        let spine = p
            .bones
            .iter()
            .find(|b| &*b.bone == "C_Spine01LumbarRobo01_XT")
            .expect("its spine");
        let (turn, shift) = spine.offset_at(Vec2::ZERO);
        assert!(turn.angle_between(Quat::IDENTITY) < 1e-4 && shift.length() < 1e-6);
        let (turn, _) = spine.offset_at(Vec2::new(-1.0, 1.0));
        assert!(turn.angle_between(spine.offsets[0].0) < 1e-4, "LU");
        let (half, _) = spine.offset_at(Vec2::new(0.0, -0.5));
        let down = spine.offsets[5].0;
        assert!(
            (half.angle_between(Quat::IDENTITY) - down.angle_between(Quat::IDENTITY) * 0.5).abs()
                < 0.01
        );
    }
}
