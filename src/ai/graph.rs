//! The AI's pose (tree.rs) on its model: its set's animation graph, a node for
//! each clip in each of the pose's layers, unmirrored and mirrored (ai_<set>.glb
//! and, for a tree that mirrors, ai_<set>_mirror.glb), under the type's own clips'
//! root, the layers in the tree's order (the soldiers' below, from the top):
//!
//! ```text
//! the type's root (its idle and deaths beside)
//! └ additive: the additive slot's clips
//!   └ blend: the jumps
//!     └ blend: the full body's, weapon's and shoulder's slots
//!       └ blend: the upper body's slot (its clips cut down to the upper body)
//!         └ additive: the aimer's aim poses
//!           └ additive: the base's additive idles
//!             └ blend: the base (the stance blend's poses, the acrobatics slot)
//! ```
//!
//! Bevy blends a blend node's children by their weights, each child that has
//! nothing for a bone leaving it to the rest, and a blend node passes its blend on
//! at its own (fixed) weight: so a layer over the rest under it at share `s` is
//! its clips at `s / (1 - s)` of their shares beside the rest (at 1), which gives
//! the rest's pose `1 - s` and the layer's `s` on the bones the layer has, and
//! leaves the rest's alone on the others (a masked slot's: the legs).
//!
//! Each frame the pose's clips are played at their weights and held at their
//! times; every other clip is stopped.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::animation::AnimationTargetId;
use bevy::animation::graph::{AnimationNodeIndex, AnimationNodeType};
use bevy::gltf::Gltf;
use bevy::prelude::*;

use super::clips::{ClipKind, Clips, Mirror, ProfileKind, mirror_model, model};
use super::tree::{LAYERS, Layer, Pose};

/// The most share an override layer takes (its clips' weights stay finite).
const MOST: f32 = 0.999;

/// Each clip's nodes: by layer, unmirrored and mirrored; and how long it is in its
/// file and how fast its time runs there. The AI keeps a clip's time in the clip's
/// own seconds (its AnimSequence's, 0 to its SequenceLength), its RateScale
/// speeding it up as the game's AnimNodeSequence plays it; its file (umodel's
/// export at 30 frames a second times the RateScale) has it played at that rate
/// already: a clip's time there is its own over its rate.
#[derive(Debug, Default)]
pub struct AiNodes {
    nodes: Vec<[[Option<AnimationNodeIndex>; 2]; LAYERS]>,
    secs: Vec<f32>,
    rates: Vec<f32>,
}

impl AiNodes {
    pub fn get(&self, clip: usize, layer: Layer, mirrored: bool) -> Option<AnimationNodeIndex> {
        self.nodes.get(clip)?[layer as usize][mirrored as usize]
    }
}

/// The graph's clip nodes to fill as their files come in: (file, [(node, clip's
/// index in the file)]), the masked slots' apart (their clips cut down to their
/// bones first: (file, their bones, [(node, index)])).
pub struct Built {
    pub nodes: AiNodes,
    pub pending: Vec<(String, Vec<(AnimationNodeIndex, usize)>)>,
    pub masked: Vec<(
        String,
        Arc<HashSet<AnimationTargetId>>,
        Vec<(AnimationNodeIndex, usize)>,
    )>,
}

/// A masked slot's nodes waiting for their file: their clips, cut down to the
/// slot's bones.
pub struct MaskJob {
    pub graph: Handle<AnimationGraph>,
    pub file: Handle<Gltf>,
    pub bones: Arc<HashSet<AnimationTargetId>>,
    pub nodes: Vec<(AnimationNodeIndex, usize)>,
}

#[derive(Resource, Default)]
pub struct MaskJobs(pub Vec<MaskJob>);

/// A masked slot's bones as animation targets.
fn mask_targets(c: &Clips, layer: Layer) -> Arc<HashSet<AnimationTargetId>> {
    Arc::new(
        c.masks
            .get(&layer)
            .map(|m| {
                m.paths
                    .iter()
                    .map(|path| {
                        let names: Vec<Name> = path.iter().map(|n| Name::new(n.clone())).collect();
                        AnimationTargetId::from_names(names.iter())
                    })
                    .collect()
            })
            .unwrap_or_default(),
    )
}

/// The clips the base plays: the stance blend's (its profiles' and their
/// transitions', the velocity switch's and blends', chooser groups whole), its
/// idle's turns in place and the acrobatics slot's.
fn base_clips(c: &Clips) -> HashSet<usize> {
    let mut s = HashSet::new();
    let mut add = |clip: Option<usize>, group: Option<&str>| {
        s.extend(clip);
        if let Some(g) = group.and_then(|g| c.group(g)) {
            s.extend(g.iter().copied());
        }
    };
    for p in &c.profiles {
        match &p.kind {
            ProfileKind::Seq { clip, group } | ProfileKind::Add { clip, group, .. } => {
                add(*clip, group.as_deref())
            }
            _ => {}
        }
        for t in &p.transitions {
            add(t.clip, None);
        }
    }
    for b in &c.vblends {
        for ch in &b.children {
            add(ch.clip, ch.group.as_deref());
            for (_, k) in &ch.picks {
                add(Some(*k), None);
            }
        }
    }
    add(c.switch.idle, None);
    // (TnAnimTurnInPlace plays its turns over the idle, in the base. Without
    // theirs, a Titan's or Heavy Gunner's turn on the spot (Nav_Gun_IdlePivot90_L/R)
    // had nothing under its aim poses: they turned its bones from nothing, and it lay
    // on its side in the air until its idle came back.)
    for &(_, k) in c.turn.iter().flat_map(|t| &t.turns) {
        add(Some(k), None);
    }
    for a in c.acro.values() {
        add(Some(a.clip), None);
    }
    s
}

/// The set's layers under `parent` (before any other node under it, so the
/// stand-in can pose it), and the clips to fill.
pub fn build(graph: &mut AnimationGraph, parent: AnimationNodeIndex, c: &Clips) -> Built {
    // The layers from the top down: each over the rest under it.
    let mut parents = [None; LAYERS];
    let mut above = parent;
    for &l in c.layers.iter().rev() {
        let n = match l {
            Layer::BaseAdd | Layer::Aim | Layer::Add => graph.add_additive_blend(1.0, above),
            _ => graph.add_blend(1.0, above),
        };
        parents[l as usize] = Some(n);
        above = n;
    }
    let in_base = base_clips(c);
    let jumps: HashSet<usize> = c
        .jump
        .children
        .iter()
        .flatten()
        .filter_map(|j| j.clip)
        .collect();
    let masked: HashMap<Layer, HashSet<usize>> = c
        .masks
        .iter()
        .map(|(l, m)| (*l, m.clips.iter().copied().collect()))
        .collect();
    let mut nodes = AiNodes {
        nodes: vec![[[None; 2]; LAYERS]; c.clips.len()],
        secs: c.clips.iter().map(|k| k.secs).collect(),
        rates: c.clips.iter().map(|k| k.rate).collect(),
    };
    let mirrored = c.mirror == Mirror::Handedness;
    let files = [model(&c.set), mirror_model(&c.set)];
    let sides = if mirrored { 2 } else { 1 };
    let mut pending: [Vec<(AnimationNodeIndex, usize)>; 2] = Default::default();
    let mut mask_pending: HashMap<(Layer, usize), Vec<(AnimationNodeIndex, usize)>> =
        HashMap::new();
    for (k, clip) in c.clips.iter().enumerate() {
        let layers: Vec<Layer> = match clip.kind {
            ClipKind::Pose => {
                let mut l = vec![Layer::Full];
                if in_base.contains(&k) {
                    l.push(Layer::Base);
                }
                if jumps.contains(&k) {
                    l.push(Layer::Jump);
                }
                for (ml, set) in &masked {
                    if set.contains(&k) {
                        l.push(*ml);
                    }
                }
                l
            }
            ClipKind::Additive => vec![Layer::BaseAdd, Layer::Add],
            ClipKind::Aim => vec![Layer::Aim],
        };
        for layer in layers {
            let Some(under) = parents[layer as usize] else {
                continue;
            };
            for m in 0..sides {
                let node = graph.add_clip(Handle::default(), 1.0, under);
                if masked.contains_key(&layer) {
                    mask_pending
                        .entry((layer, m))
                        .or_default()
                        .push((node, clip.index));
                } else {
                    pending[m].push((node, clip.index));
                }
                nodes.nodes[k][layer as usize][m] = Some(node);
            }
        }
    }
    let pending = files
        .iter()
        .zip(pending)
        .filter(|(_, l)| !l.is_empty())
        .map(|(f, l)| (f.clone(), l))
        .collect();
    let masked = mask_pending
        .into_iter()
        .map(|((layer, m), list)| (files[m].clone(), mask_targets(c, layer), list))
        .collect();
    Built {
        nodes,
        pending,
        masked,
    }
}

/// The masked slots' nodes get their clips, cut down to their bones, once their
/// files are in.
pub fn fill_mask_clips(
    mut jobs: ResMut<MaskJobs>,
    gltfs: Res<Assets<Gltf>>,
    mut clips: ResMut<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    asset_server: Res<AssetServer>,
) {
    if jobs.0.is_empty() {
        return;
    }
    jobs.0.retain(|job| {
        let Some(gltf) = gltfs.get(&job.file) else {
            return !matches!(
                asset_server.load_state(&job.file),
                bevy::asset::LoadState::Failed(_)
            );
        };
        let mut cut: HashMap<usize, Handle<AnimationClip>> = HashMap::new();
        for &(_, index) in &job.nodes {
            if cut.contains_key(&index) {
                continue;
            }
            let Some(mut clip) = gltf
                .animations
                .get(index)
                .and_then(|h| clips.get(h))
                .cloned()
            else {
                continue;
            };
            clip.curves_mut().retain(|id, _| job.bones.contains(id));
            cut.insert(index, clips.add(clip));
        }
        if let Some(graph) = graphs.get_mut(&job.graph) {
            for &(node, index) in &job.nodes {
                if let (Some(n), Some(h)) = (graph.get_mut(node), cut.get(&index)) {
                    n.node_type = AnimationNodeType::Clip(h.clone());
                }
            }
        }
        false
    });
}

/// A clip's time `t` (its own seconds, of `secs`) in its file, whose time runs at
/// its `rate` (none: 1). (Not quite to its end: a clip sought to its very end
/// wraps round to its start.) Before, the clip's own time was sought in the file:
/// a Swarmer's jog (Nav_StrafeJog_*, RateScale 1.3: 0.7 s, 0.54 s in its file)
/// held its last frame for the last 0.16 s of every stride, the Swarmer gliding
/// on with its legs still, and stumbles (1.25) and the Leaper's turns (0.9, 1.1)
/// played too fast or too slow.
fn file_time(t: f32, secs: f32, rate: Option<f32>) -> f32 {
    let rate = rate.filter(|r| *r > 1e-3).unwrap_or(1.0);
    (t / rate).clamp(0.0, (secs / rate - 1e-3).max(0.0))
}

/// A pose on a model's animation player: its clips at their weights (an override
/// layer's at its share over the rest), split between the unmirrored and mirrored
/// clips, held at their times; the rest stopped.
pub fn apply(pose: &Pose, nodes: &AiNodes, player: &mut AnimationPlayer) {
    let mut keep: HashSet<AnimationNodeIndex> = HashSet::with_capacity(pose.clips.len() * 2);
    let m = pose.mirror.clamp(0.0, 1.0);
    for pc in &pose.clips {
        let layer = pc.layer as usize;
        let w = if pc.layer.overrides() {
            let s = pose.shares[layer].clamp(0.0, MOST);
            pc.weight * s / (1.0 - s)
        } else {
            pc.weight
        };
        let secs = nodes.secs.get(pc.clip as usize).copied().unwrap_or(0.0);
        let t = file_time(pc.time, secs, nodes.rates.get(pc.clip as usize).copied());
        for (mirrored, share) in [(false, 1.0 - m), (true, m)] {
            let ww = w * share;
            if ww <= 1e-5 {
                continue;
            }
            // (A tree that doesn't mirror has no mirrored nodes: its own then.)
            let node = nodes
                .get(pc.clip as usize, pc.layer, mirrored)
                .or_else(|| nodes.get(pc.clip as usize, pc.layer, false));
            if let Some(node) = node {
                player.play(node).set_weight(ww).set_seek_time(t).pause();
                keep.insert(node);
            }
        }
    }
    let stale: Vec<AnimationNodeIndex> = player
        .playing_animations()
        .map(|(n, _)| *n)
        .filter(|n| !keep.contains(n))
        .collect();
    for n in stale {
        player.stop(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::pawn::Pawn;
    use crate::ai::tree::AnimTree;

    /// A Titan (and a Heavy Gunner: the Titan's set) and a Swarmer turned a
    /// quarter round as they stand play their turn in place (TnAnimTurnInPlace) on
    /// their models: every clip of their poses has its node in its layer, the turn's
    /// in the base.
    #[test]
    fn a_turn_in_place_plays_on_the_model() {
        for set in ["titan", "swarmer"] {
            let Some(c) = super::super::clips::set(set) else {
                continue;
            };
            let mut graph = AnimationGraph::new();
            let root = graph.root;
            let built = build(&mut graph, root, &c);
            let mut p = Pawn::new(Entity::PLACEHOLDER, Vec3::ZERO, 0.0, 250.0, 200.0, 100.0);
            let mut tree = AnimTree::default();
            let mut roll = || 0.5;
            let dt = 1.0 / 30.0;
            let mut turned = 0.0f32;
            for k in 0..80 {
                if (30..38).contains(&k) {
                    p.yaw += 16384.0 / 8.0;
                }
                let pose = tree.update(&c, &mut p, dt, &mut roll);
                for e in &pose.clips {
                    let name = &c.clips[e.clip as usize].name;
                    assert!(
                        built.nodes.get(e.clip as usize, e.layer, false).is_some(),
                        "{set}: {name} has no node in its layer ({:?})",
                        e.layer
                    );
                    if name.contains("IdlePivot90") {
                        turned = turned.max(e.weight);
                    }
                }
            }
            assert!(turned > 0.5, "{set}: its turn played at {turned}");
        }
    }

    /// A Swarmer's jog loops as the game plays it: its clips' RateScale (1.3)
    /// speeds their stride, one every 0.7 / 1.3 s, and on its model each frame of it
    /// is a new one, never its last held (its files' clips, 0.54 s long, have the
    /// rate in them already); its stumble's root motion takes its whole time.
    #[test]
    fn a_swarmers_jog_loops_without_a_stop() {
        let Some(c) = super::super::clips::set("swarmer") else {
            return;
        };
        let jog = c.find("Nav_StrafeJog_F").expect("its jog");
        let (secs, rate) = (c.clips[jog].secs, c.clips[jog].rate);
        assert!((rate - 1.3).abs() < 1e-3 && (secs - 0.7).abs() < 1e-3);
        let mut graph = AnimationGraph::new();
        let root = graph.root;
        let built = build(&mut graph, root, &c);
        let mut p = Pawn::new(Entity::PLACEHOLDER, Vec3::ZERO, 0.0, 250.0, 200.0, 100.0);
        p.vel = Vec3::X * 1000.0;
        p.accel = Vec3::X;
        let mut tree = AnimTree::default();
        let mut roll = || 0.5;
        let dt = 1.0 / 30.0;
        let (mut wraps, mut last, mut held) = (0, None::<f32>, 0);
        let frames = 120;
        for k in 0..frames {
            let pose = tree.update(&c, &mut p, dt, &mut roll);
            let Some(e) = pose
                .clips
                .iter()
                .find(|e| e.clip as usize == jog && e.weight > 0.5)
            else {
                continue;
            };
            let t = file_time(e.time, built.nodes.secs[jog], Some(built.nodes.rates[jog]));
            // (Within its file's clip: 0.7 / 1.3 s.)
            assert!(
                t < secs / rate,
                "frame {k}: {t} past its file's {}",
                secs / rate
            );
            if let Some(l) = last {
                if t < l {
                    wraps += 1;
                } else if (t - l).abs() < 1e-4 {
                    held += 1;
                }
            }
            last = Some(t);
        }
        assert_eq!(held, 0, "its jog stood still {held} frames");
        // (4 s at 1.3 strides a 0.7 s clip: 7.4 strides.)
        let want = (frames as f32 * dt * rate / secs).floor() as i32;
        assert!((wraps - want).abs() <= 1, "{wraps} strides, not {want}");
        // A soldier's stumble (1.25: its frames 1/37.5 s apart in its file, 1/30 s
        // of its own time) moves it back till its 29th frame, a little under 1 s
        // into it, not 0.77 s.
        if let Some(s) = super::super::clips::set("soldier")
            && let Some(k) = s.find("HitReact_StumbleBackward02")
            && let Some(r) = s.clips[k].root.as_ref()
        {
            let (a, b) = (r.at(0.9)[0], r.at(1.0)[0]);
            assert!(a - b > 10.0, "it stood still by 0.9 s: {a} {b}");
        }
    }
}
