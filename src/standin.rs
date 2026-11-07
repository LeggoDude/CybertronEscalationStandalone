//! Big animation graphs posed through small ones.
//!
//! Bevy poses each animated bone by walking its player's whole graph, node by
//! node, looking every clip node up among the clips playing (bevy_animation's
//! animate_targets). A robot's graph has a node for every clip the game may play
//! (robot_graph: some 600, with the mirrored set), so each of its 270 bones walks
//! them all, every frame, to find the dozen that have any weight: most of the
//! time the demo spent animating.
//!
//! A player whose graph is big (BIG_GRAPH clips or more: a robot's, an enemy's with
//! its deaths) keeps it, and all that the game does with it (its clips played,
//! weighed and sought as ever), but its bones are posed by a stand-in: a player of
//! its own whose small graph is the big one's blend and add nodes with a row of
//! slots under each in place of its clips. Each frame, once the players' clips have
//! moved on, the clips with weight go into the slots, in their order, with their
//! weights and times. Bevy blends a node's children in the order of their indices,
//! and the slots keep the clips' order, so the pose is the one the big graph gives,
//! exactly.
//!
//! A row grows when more clips have weight under its node than it has slots: a
//! bigger graph is made, and until Bevy has it ready (the next frame) the bones are
//! posed by the big graph again. The rows start as big as the most any player with
//! that graph has needed. Graphs with masks, a clip under more than one node, or
//! blend nodes after the clips beside them aren't posed this way; nor is a graph
//! whose shape changes once it is in use (its clips may change, as they load).

use bevy::animation::AnimatedBy;
use bevy::animation::graph::{AnimationGraphNode, AnimationNodeType};
use bevy::asset::AssetEventSystems;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Graphs with this many clips or more are posed through a stand-in.
const BIG_GRAPH: usize = 12;
/// A row's slots to begin with (it doubles as it needs more).
const FIRST_SLOTS: usize = 4;
/// Not in a row (Layout::row_of): not a clip.
const NO_ROW: u32 = u32::MAX;

pub struct StandInPlugin;

impl Plugin for StandInPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Layouts>()
            .add_systems(
                PostUpdate,
                (follow_graphs, adopt_bones, pose)
                    .chain()
                    .after(bevy::animation::advance_animations)
                    .before(bevy::animation::animate_targets)
                    // (So the graphs' events read here have been read by Bevy's
                    // threading of them, earlier in the frame.)
                    .before(AssetEventSystems),
            )
            .add_systems(Last, drop_standins);
    }
}

/// How a big graph's clips go into slots: its blend and add nodes (the frame the
/// stand-in's graph copies), and its rows: each node with clips under it.
struct Layout {
    /// The blend and add nodes, in index order (the root among them).
    frame: Vec<AnimationNodeIndex>,
    /// Each frame node's kind (additive or not), weight and the frame nodes under
    /// it (their places in `frame`): to tell layouts apart.
    shape: Vec<(bool, u32, Vec<usize>)>,
    /// Each row's node and its number of clips.
    rows: Vec<(AnimationNodeIndex, usize)>,
    /// Each node's row (by its index), or NO_ROW.
    row_of: Vec<u32>,
    /// The most slots each row has needed so far, by any player with this layout.
    sizes: Vec<AtomicUsize>,
}

impl Layout {
    /// The layout of a big graph that can be posed through slots.
    fn of(graph: &AnimationGraph) -> Option<Layout> {
        if !graph.mask_groups.is_empty() {
            return None;
        }
        let g = &graph.graph;
        let is_clip = |i: AnimationNodeIndex| matches!(g[i].node_type, AnimationNodeType::Clip(_));
        if is_clip(graph.root) {
            return None;
        }
        let mut parents: Vec<Vec<AnimationNodeIndex>> = vec![Vec::new(); g.node_count()];
        for p in g.node_indices() {
            for k in g.neighbors(p) {
                parents[k.index()].push(p);
            }
        }
        let mut frame = Vec::new();
        let mut clips = 0;
        for i in g.node_indices() {
            if g[i].mask != 0 {
                return None;
            }
            if !is_clip(i) {
                frame.push(i);
                continue;
            }
            clips += 1;
            // A clip: a leaf, under one blend or add node.
            let up = &parents[i.index()];
            if g.neighbors(i).next().is_some() || up.len() != 1 || is_clip(up[0]) {
                return None;
            }
        }
        if clips < BIG_GRAPH {
            return None;
        }
        frame.sort_unstable();
        let mut rows = Vec::new();
        let mut row_of = vec![NO_ROW; g.node_count()];
        for &f in &frame {
            // (The slots come after all the frame's nodes: so must a node's clips
            // after the blend and add nodes beside them.)
            let last_node = g.neighbors(f).filter(|&k| !is_clip(k)).max();
            let first_clip = g.neighbors(f).filter(|&k| is_clip(k)).min();
            if let (Some(a), Some(b)) = (last_node, first_clip)
                && a > b
            {
                return None;
            }
            let count = g.neighbors(f).filter(|&k| is_clip(k)).count();
            if count > 0 {
                for k in g.neighbors(f) {
                    row_of[k.index()] = rows.len() as u32;
                }
                rows.push((f, count));
            }
        }
        let place = |i: AnimationNodeIndex| frame.binary_search(&i).unwrap_or(usize::MAX);
        let shape = frame
            .iter()
            .map(|&f| {
                let mut under: Vec<usize> =
                    g.neighbors(f).filter(|&k| !is_clip(k)).map(place).collect();
                under.sort_unstable();
                (
                    matches!(g[f].node_type, AnimationNodeType::Add),
                    g[f].weight.to_bits(),
                    under,
                )
            })
            .collect();
        // (Rows of frame nodes only hold clips: set back any frame node marked.)
        for &f in &frame {
            row_of[f.index()] = NO_ROW;
        }
        let sizes = rows
            .iter()
            .map(|&(_, n)| AtomicUsize::new(FIRST_SLOTS.min(n)))
            .collect();
        Some(Layout {
            frame,
            shape,
            rows,
            row_of,
            sizes,
        })
    }

    fn same(&self, other: &Layout) -> bool {
        self.frame == other.frame
            && self.shape == other.shape
            && self.rows == other.rows
            && self.row_of == other.row_of
    }

    /// The rows' sizes to begin with.
    fn first_sizes(&self) -> Vec<usize> {
        self.sizes
            .iter()
            .map(|s| s.load(Ordering::Relaxed))
            .collect()
    }

    /// The stand-in's graph: the big graph's frame (its blend and add nodes as
    /// they are), and under each row's node its slots (clip nodes, their clips put
    /// in as they are needed).
    fn standin_graph(
        &self,
        big: &AnimationGraph,
        sizes: &[usize],
    ) -> (AnimationGraph, Vec<Vec<AnimationNodeIndex>>) {
        let mut graph = AnimationGraph::new();
        let mut map: HashMap<AnimationNodeIndex, AnimationNodeIndex> = HashMap::default();
        for &f in &self.frame {
            let node = big.graph[f].clone();
            let i = if f == big.root {
                let root = graph.root;
                graph.graph[root] = node;
                root
            } else {
                graph.graph.add_node(node)
            };
            map.insert(f, i);
        }
        for &f in &self.frame {
            for k in big.graph.neighbors(f) {
                if let Some(&to) = map.get(&k) {
                    graph.graph.add_edge(map[&f], to, ());
                }
            }
        }
        let slots = self
            .rows
            .iter()
            .zip(sizes)
            .map(|(&(f, _), &n)| {
                (0..n)
                    .map(|_| {
                        let i = graph.graph.add_node(AnimationGraphNode {
                            node_type: AnimationNodeType::Clip(Handle::default()),
                            mask: 0,
                            weight: 1.0,
                        });
                        graph.graph.add_edge(map[&f], i, ());
                        i
                    })
                    .collect()
            })
            .collect();
        (graph, slots)
    }
}

/// The big graphs seen: each one's layout (None: not one to pose through slots).
/// Graphs laid out alike share one.
#[derive(Resource, Default)]
struct Layouts(HashMap<AssetId<AnimationGraph>, Option<Arc<Layout>>>);

impl Layouts {
    fn get(&mut self, id: AssetId<AnimationGraph>, graph: &AnimationGraph) -> Option<Arc<Layout>> {
        if let Some(l) = self.0.get(&id) {
            return l.clone();
        }
        let made = Layout::of(graph).map(|l| {
            self.0
                .values()
                .flatten()
                .find(|known| known.same(&l))
                .cloned()
                .unwrap_or_else(|| Arc::new(l))
        });
        self.0.insert(id, made.clone());
        made
    }
}

/// What a slot was given: the big graph's node, its clip and its weight's bits.
type Filled = Option<(AnimationNodeIndex, AssetId<AnimationClip>, u32)>;

/// A player posed through a stand-in.
#[derive(Component)]
pub struct Posed {
    standin: Entity,
    /// The big graph it is posing for, and its layout.
    big: AssetId<AnimationGraph>,
    layout: Arc<Layout>,
    /// The stand-in's graph, its rows' slots, and what each slot was last given
    /// (the big graph's node, its clip and weight).
    graph: Handle<AnimationGraph>,
    slots: Vec<Vec<AnimationNodeIndex>>,
    filled: Vec<Vec<Filled>>,
    /// Bevy has the stand-in's graph ready.
    ready: bool,
    /// Its bones are the stand-in's to pose (else the big player's).
    on_standin: bool,
    bones: Vec<Entity>,
}

impl Posed {
    /// A new graph for the stand-in, its slots empty (the bones the big player's
    /// until Bevy has it ready).
    fn regraph(
        &mut self,
        (graph, slots): (Handle<AnimationGraph>, Vec<Vec<AnimationNodeIndex>>),
        standins: &mut Query<(&mut AnimationPlayer, &mut AnimationGraphHandle), With<StandIn>>,
    ) {
        self.graph = graph;
        self.filled = slots.iter().map(|s| vec![None; s.len()]).collect();
        self.slots = slots;
        self.ready = false;
        if let Ok((mut player, mut handle)) = standins.get_mut(self.standin) {
            player.stop_all();
            handle.0 = self.graph.clone();
        }
    }
}

/// A stand-in player: posing the bones of its player's.
#[derive(Component)]
pub struct StandIn(pub Entity);

/// Points bones at a player.
fn point(bones: &mut Query<(Entity, &mut AnimatedBy)>, list: &[Entity], to: Entity) {
    for &b in list {
        if let Ok((_, mut by)) = bones.get_mut(b)
            && by.0 != to
        {
            by.0 = to;
        }
    }
}

/// Gives each player whose graph is big a stand-in (and takes it away when its
/// graph no longer is). A new graph laid out as the old keeps the stand-in's.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn follow_graphs(
    mut commands: Commands,
    mut layouts: ResMut<Layouts>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut players: Query<
        (Entity, &AnimationGraphHandle, Option<&mut Posed>),
        (With<AnimationPlayer>, Without<StandIn>),
    >,
    changed: Query<
        Entity,
        (
            Changed<AnimationGraphHandle>,
            With<AnimationPlayer>,
            Without<StandIn>,
        ),
    >,
    mut bones: Query<(Entity, &mut AnimatedBy)>,
    mut standins: Query<(&mut AnimationPlayer, &mut AnimationGraphHandle), With<StandIn>>,
    mut waiting: Local<Vec<Entity>>,
) {
    let mut look: Vec<Entity> = changed.iter().collect();
    look.append(&mut waiting);
    look.sort_unstable();
    look.dedup();
    for entity in look {
        let Ok((entity, handle, posed)) = players.get_mut(entity) else {
            continue;
        };
        let id = handle.id();
        let Some(big) = graphs.get(id) else {
            // (Not in yet.)
            waiting.push(entity);
            continue;
        };
        let layout = layouts.get(id, big);
        match (layout, posed) {
            (None, None) => {}
            (None, Some(p)) => {
                point(&mut bones, &p.bones, entity);
                commands.entity(p.standin).despawn();
                commands.entity(entity).remove::<Posed>();
            }
            (Some(layout), Some(mut p)) => {
                p.big = id;
                if p.layout.same(&layout) {
                    continue;
                }
                // Laid out anew: a graph for it, the bones the big player's till
                // it is ready.
                let (graph, slots) = layout.standin_graph(big, &layout.first_sizes());
                p.layout = layout;
                p.regraph((graphs.add(graph), slots), &mut standins);
                point(&mut bones, &p.bones, entity);
                p.on_standin = false;
            }
            (Some(layout), None) => {
                let sizes = layout.first_sizes();
                let (graph, slots) = layout.standin_graph(big, &sizes);
                let graph = graphs.add(graph);
                let list: Vec<Entity> = bones
                    .iter()
                    .filter(|(_, by)| by.0 == entity)
                    .map(|(e, _)| e)
                    .collect();
                let standin = commands
                    .spawn((
                        AnimationPlayer::default(),
                        AnimationGraphHandle(graph.clone()),
                        StandIn(entity),
                        Name::new("Stand-in animation player"),
                    ))
                    .id();
                commands.entity(entity).insert(Posed {
                    standin,
                    big: id,
                    layout,
                    graph,
                    filled: slots.iter().map(|s| vec![None; s.len()]).collect(),
                    slots,
                    ready: false,
                    on_standin: false,
                    bones: list,
                });
            }
        }
    }
}

/// Bones that come in after their player has its stand-in: posed by it too.
fn adopt_bones(
    new_bones: Query<(Entity, &AnimatedBy), Added<AnimatedBy>>,
    mut players: Query<&mut Posed>,
) {
    for (bone, by) in &new_bones {
        if let Ok(mut p) = players.get_mut(by.0)
            && !p.bones.contains(&bone)
        {
            p.bones.push(bone);
            // (Pointed at the stand-in with the rest, below.)
            p.on_standin = false;
        }
    }
}

/// Puts each posed player's clips with weight into its stand-in's slots, in their
/// order, with their weights and times, and has the stand-in pose the bones.
#[allow(clippy::type_complexity)]
fn pose(
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut events: MessageReader<AssetEvent<AnimationGraph>>,
    mut players: Query<
        (Entity, &AnimationPlayer, &AnimationGraphHandle, &mut Posed),
        Without<StandIn>,
    >,
    mut standins: Query<(&mut AnimationPlayer, &mut AnimationGraphHandle), With<StandIn>>,
    mut bones: Query<(Entity, &mut AnimatedBy)>,
    mut rows: Local<Vec<Vec<(AnimationNodeIndex, f32, f32)>>>,
    mut changes: Local<Vec<(AnimationNodeIndex, Handle<AnimationClip>, f32)>>,
) {
    // The graphs Bevy has (re)made ready since last frame.
    let ready: HashSet<AssetId<AnimationGraph>> = events
        .read()
        .filter_map(|e| match *e {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            _ => None,
        })
        .collect();
    for (entity, player, handle, mut p) in &mut players {
        let p = &mut *p;
        if handle.id() != p.big {
            continue;
        }
        if !p.ready && ready.contains(&p.graph.id()) {
            p.ready = true;
        }
        let Some(big) = graphs.get(p.big) else {
            continue;
        };
        // The clips with weight, by row, in their order.
        rows.resize_with(p.layout.rows.len(), Vec::new);
        for row in rows.iter_mut() {
            row.clear();
        }
        for (&node, active) in player.playing_animations() {
            let weight = active.weight();
            if weight == 0.0 {
                continue;
            }
            match p.layout.row_of.get(node.index()) {
                Some(&r) if r != NO_ROW => {
                    rows[r as usize].push((node, weight, active.seek_time()))
                }
                _ => {}
            }
        }
        for row in rows.iter_mut() {
            row.sort_unstable_by_key(|c| c.0);
        }
        // More than a row holds: a bigger graph (the big player posing the bones
        // till it is ready).
        if rows.iter().zip(&p.slots).any(|(r, s)| r.len() > s.len()) {
            let sizes: Vec<usize> = rows
                .iter()
                .zip(&p.slots)
                .zip(&p.layout.rows)
                .map(|((r, s), &(_, count))| {
                    if r.len() > s.len() {
                        (s.len() * 2).max(r.len()).min(count)
                    } else {
                        s.len()
                    }
                })
                .collect();
            for (most, &n) in p.layout.sizes.iter().zip(&sizes) {
                most.fetch_max(n, Ordering::Relaxed);
            }
            let (graph, slots) = p.layout.standin_graph(big, &sizes);
            p.regraph((graphs.add(graph), slots), &mut standins);
            if p.on_standin {
                point(&mut bones, &p.bones, entity);
                p.on_standin = false;
            }
            continue;
        }
        if !p.ready {
            continue;
        }
        // Each slot's clip, where it changes.
        changes.clear();
        for (r, row) in rows.iter().enumerate() {
            for (k, &(node, _, _)) in row.iter().enumerate() {
                let n = &big.graph[node];
                let AnimationNodeType::Clip(clip) = &n.node_type else {
                    continue;
                };
                let want = Some((node, clip.id(), n.weight.to_bits()));
                if p.filled[r][k] != want {
                    p.filled[r][k] = want;
                    changes.push((p.slots[r][k], clip.clone(), n.weight));
                }
            }
        }
        if !changes.is_empty()
            && let Some(graph) = graphs.get_mut(&p.graph)
        {
            for (slot, clip, weight) in changes.drain(..) {
                if let Some(node) = graph.get_mut(slot) {
                    node.node_type = AnimationNodeType::Clip(clip);
                    node.weight = weight;
                }
            }
        }
        // The stand-in's clips: the weights and times of those in the slots.
        let Ok((mut sp, _)) = standins.get_mut(p.standin) else {
            continue;
        };
        for (row, slots) in rows.iter().zip(&p.slots) {
            for (k, &slot) in slots.iter().enumerate() {
                match row.get(k) {
                    Some(&(_, weight, time)) => {
                        sp.play(slot).set_weight(weight).set_seek_time(time).pause();
                    }
                    None => {
                        if sp.is_playing_animation(slot) {
                            sp.stop(slot);
                        }
                    }
                }
            }
        }
        if !p.on_standin {
            point(&mut bones, &p.bones, p.standin);
            p.on_standin = true;
        }
    }
}

/// Stand-ins whose players are gone go too.
fn drop_standins(
    mut commands: Commands,
    standins: Query<(Entity, &StandIn)>,
    players: Query<&Posed>,
) {
    for (entity, of) in &standins {
        if !players.get(of.0).is_ok_and(|p| p.standin == entity) {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A graph shaped as the robot's (robot_graph): an additive blend over the
    /// ordinary blend, the ordinary clips under that and the additive ones (every
    /// fifth) beside it.
    fn robot_like_with(clips: &[Handle<AnimationClip>]) -> AnimationGraph {
        let mut graph = AnimationGraph::new();
        let layers = graph.add_additive_blend(1.0, graph.root);
        let base = graph.add_blend(1.0, layers);
        for (i, clip) in clips.iter().enumerate() {
            let parent = if i % 5 == 4 { layers } else { base };
            graph.add_clip(clip.clone(), 1.0, parent);
        }
        graph
    }

    fn robot_like(clips: usize) -> AnimationGraph {
        robot_like_with(&vec![Handle::default(); clips])
    }

    #[test]
    fn robot_like_graphs_are_laid_out_in_rows() {
        let graph = robot_like(40);
        let layout = Layout::of(&graph).expect("a big robot-like graph");
        assert_eq!(layout.frame.len(), 3);
        assert_eq!(layout.rows.len(), 2);
        let total: usize = layout.rows.iter().map(|r| r.1).sum();
        assert_eq!(total, 40);
        // Every clip has a row, no blend node has.
        for i in graph.graph.node_indices() {
            let clip = matches!(graph.graph[i].node_type, AnimationNodeType::Clip(_));
            assert_eq!(layout.row_of[i.index()] != NO_ROW, clip);
        }
        // A small graph isn't one.
        assert!(Layout::of(&robot_like(5)).is_none());
    }

    #[test]
    fn standin_graphs_keep_the_frame_and_the_order() {
        let graph = robot_like(40);
        let layout = Layout::of(&graph).unwrap();
        let (standin, slots) = layout.standin_graph(&graph, &[4, 3]);
        assert_eq!(standin.graph.node_count(), 3 + 4 + 3);
        // The frame as it was: root, then the additive blend, then the blend.
        assert!(matches!(
            standin.graph[standin.root].node_type,
            AnimationNodeType::Blend
        ));
        let layers = AnimationNodeIndex::new(1);
        let base = AnimationNodeIndex::new(2);
        assert!(matches!(
            standin.graph[layers].node_type,
            AnimationNodeType::Add
        ));
        assert!(matches!(
            standin.graph[base].node_type,
            AnimationNodeType::Blend
        ));
        // Each row's slots under its node, after the frame nodes beside them.
        for (row, (&(f, _), slots)) in layout.rows.iter().zip(&slots).enumerate() {
            let parent = if f == AnimationNodeIndex::new(2) {
                base
            } else {
                layers
            };
            for w in slots.windows(2) {
                assert!(w[0] < w[1], "row {row}'s slots in order");
            }
            for &s in slots {
                assert!(standin.graph.neighbors(parent).any(|k| k == s));
            }
        }
        assert!(standin.graph.neighbors(layers).any(|k| k == base));
    }

    /// Two players with the same clips playing, one posed through a stand-in and
    /// one by its own graph as Bevy has it (a mask group for a bone neither has
    /// keeps it out): their bones' poses, frame by frame as clips come and go
    /// (and the rows grow), are the same to the bit.
    #[test]
    fn the_standin_poses_the_bones_as_the_big_graph_would() {
        use bevy::animation::AnimationTargetId;
        use bevy::animation::animated_field;
        use bevy::animation::animation_curves::{AnimatableCurve, AnimatableKeyframeCurve};
        use bevy::time::TimeUpdateStrategy;

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::animation::AnimationPlugin,
            StandInPlugin,
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ));
        let ids: Vec<AnimationTargetId> = (0..6)
            .map(|i| AnimationTargetId::from_name(&Name::new(format!("Bone{i}"))))
            .collect();
        // Forty clips, each moving and turning the bones its own way.
        let clips: Vec<Handle<AnimationClip>> = (0..40)
            .map(|c| {
                let mut clip = AnimationClip::default();
                for (b, &id) in ids.iter().enumerate() {
                    let f = (c * 7 + b * 3) as f32;
                    let moves = AnimatableKeyframeCurve::new([
                        (0.0, Vec3::new(f.sin(), f.cos(), f * 0.01)),
                        (1.0, Vec3::new(f.cos(), 0.5, -f.sin())),
                    ])
                    .unwrap();
                    let turns = AnimatableKeyframeCurve::new([
                        (0.0, Quat::from_rotation_y(f)),
                        (1.0, Quat::from_euler(EulerRot::XYZ, f * 0.3, 0.2, f * 0.1)),
                    ])
                    .unwrap();
                    clip.add_curve_to_target(
                        id,
                        AnimatableCurve::new(animated_field!(Transform::translation), moves),
                    );
                    clip.add_curve_to_target(
                        id,
                        AnimatableCurve::new(animated_field!(Transform::rotation), turns),
                    );
                }
                app.world_mut()
                    .resource_mut::<Assets<AnimationClip>>()
                    .add(clip)
            })
            .collect();
        let posed = robot_like_with(&clips);
        let mut plain = robot_like_with(&clips);
        plain.add_target_to_mask_group(AnimationTargetId::from_name(&Name::new("Nobody")), 0);
        let mut skeleton = |graph: AnimationGraph| {
            let world = app.world_mut();
            let graph = world.resource_mut::<Assets<AnimationGraph>>().add(graph);
            let player = world
                .spawn((AnimationPlayer::default(), AnimationGraphHandle(graph)))
                .id();
            let bones: Vec<Entity> = ids
                .iter()
                .map(|&id| {
                    world
                        .spawn((id, AnimatedBy(player), Transform::default()))
                        .id()
                })
                .collect();
            (player, bones)
        };
        let (posed_player, posed_bones) = skeleton(posed);
        let (plain_player, plain_bones) = skeleton(plain);
        let mut by_standin = 0;
        for frame in 0..40u32 {
            // (From one clip to most of them and back: the rows grow.)
            let count = 1 + (frame * 7) % 30;
            for player in [posed_player, plain_player] {
                let mut p = app.world_mut().get_mut::<AnimationPlayer>(player).unwrap();
                p.stop_all();
                for k in 0..count {
                    let node = AnimationNodeIndex::new(3 + ((frame * 13 + k * 17) % 40) as usize);
                    p.play(node)
                        .set_weight(0.1 + ((frame + k) % 9) as f32 * 0.1)
                        .seek_to(((frame * 31 + k * 7) % 90) as f32 / 100.0);
                }
            }
            app.update();
            let world = app.world();
            for (a, b) in posed_bones.iter().zip(&plain_bones) {
                let (a, b) = (
                    world.get::<Transform>(*a).unwrap(),
                    world.get::<Transform>(*b).unwrap(),
                );
                assert_eq!(
                    (
                        a.translation.to_array().map(f32::to_bits),
                        a.rotation.to_array().map(f32::to_bits)
                    ),
                    (
                        b.translation.to_array().map(f32::to_bits),
                        b.rotation.to_array().map(f32::to_bits)
                    ),
                    "frame {frame}: {count} clips"
                );
                // (Once Bevy has the graphs ready.)
                if frame >= 2 {
                    assert_ne!(a.translation, Vec3::ZERO, "frame {frame}: posed");
                }
            }
            if world.get::<AnimatedBy>(posed_bones[0]).unwrap().0 != posed_player {
                by_standin += 1;
            }
            assert_eq!(
                world.get::<AnimatedBy>(plain_bones[0]).unwrap().0,
                plain_player
            );
        }
        // (Most frames by the stand-in: the others while a bigger graph was made.)
        assert!(by_standin > 25, "{by_standin} frames posed by the stand-in");
    }

    #[test]
    fn masked_or_shared_clips_are_not_laid_out() {
        let mut graph = robot_like(40);
        graph.add_target_to_mask_group(
            bevy::animation::AnimationTargetId::from_name(&Name::new("Hips")),
            0,
        );
        assert!(Layout::of(&graph).is_none());
        // A clip under two nodes.
        let mut graph = robot_like(40);
        let extra = graph.add_blend(1.0, graph.root);
        graph.add_edge(extra, AnimationNodeIndex::new(5));
        assert!(Layout::of(&graph).is_none());
    }
}
