//! Escalation's Armory Recreators (TransContent.TnInteractLootBox): the pods on every
//! map that, for 250 energon, make one thing from the game's loot table (a heavy
//! weapon nearly always), once a match each. From the maps' BASE packages, their
//! Kismet and TransGame.int (see tools/build_pods.py).
//!
//! * **The pods** (levels/<map>_pods.txt and models/storage_pod.glb, by
//!   tools/build_pods.py): where each map has them. The maps that draw them already
//!   (Downfall, Ancients, Ignition: build_map.py's AP_StoragePod_SKEL@bind, in their
//!   bind pose) have those taken out of the map's model as it comes in, before
//!   merge.rs joins its parts, for these, which open; Oblivion's, which its build
//!   leaves out, get a box (POD_BOX) for the collision the others' models have.
//! * **Using one** (its UseConditions, the unlock consoles' too): standing within 4 m
//!   of the spot 5 m in front of it, its height within his own and a metre, facing it
//!   (within 65 degrees), in robot form, not down and not in a melee blow. The HUD
//!   says "Press <<UseInteract>> to Activate Armory Recreator *250" and shows the
//!   energon counter, red when there isn't enough (TnInteractActionHudCurrencyNotify).
//!   Pressed without 250: the error beep (ESC_INSUFFICIENT_FUNDS) and the counter
//!   flashing red. With it: 250 spent, it makes its thing (one roll of
//!   ESC_LootTableBase: LOOT) and opens (AP_StoragePodOpen, 3 s, with its sound), its
//!   outline going off: done for the match (nothing turns it on again).
//! * **What it made** waits inside it, a weapon standing at its WeaponLootSocket
//!   (barrel up, top to the buyer: AlignPickupMeshToSocket, as a weapon station's),
//!   a support item at its MiscLootSocket. After 2.5 s (PickupDelay) it can be taken,
//!   from where the buyer stood (TnInteractActionLootBoxSpawn moves its pickup
//!   cylinder to the interact spot, twice as big): "Hold <<Pickup>> to pickup ...",
//!   main.rs's weapon_pickups. It never goes away (LifeSpan 0).
//! * **The outline** (ENV_OutlineHighlight_MF, the pod's body only): cyan while it can
//!   be used, flaring to six times as bright every 2.5 s.
//! * **The steam** its opening lets out (its HmAnimNotify_PlayEffects): five puffs of
//!   ReloadSteam_Titan_FX, from its door's top corners, its top and its handles, each
//!   following its socket (STEAM).
//! * **Online**: the buyer's PC rolls and sends what it made (net.rs's JazzEvent::Pod):
//!   every PC opens that pod with the same thing in it. Whoever takes it sends
//!   PodLoot, and it is gone from the others.
//!
//! Not in: the controller rumble its opening's notifies play for everyone else (the
//! demo is played with the keyboard and mouse).

use std::path::Path;

use bevy::prelude::*;

use crate::hud::{HudEvent, HudState, Prompt};
use crate::levelfx::{FxId, FxSystems};
use crate::net::JazzEvent;
use crate::shop::Equipment;
use crate::{Form, LocalPlayer, Player, PlayerInput, RemotePlayer, Score, Sfx};

/// The pod (tools/build_pods.py): its scene, its opening (its one clip) and its
/// outline's node.
pub const POD_MODEL: &str = "models/storage_pod.glb";
const OUTLINE_NODE: &str = "StoragePodOutline";
/// The maps' own pods (build_map.py's bind-pose placements), taken out for these.
pub const BAKED_POD: &str = "AP_StoragePod_SKEL@bind";
/// The price (TnInteractLootBox.Cost; no map sets its own).
pub const COST: u32 = 250;
/// UseConditions: within 400 (flat) of the interact spot, the height between them
/// under his height and 100, facing it within 65 degrees (of the pod's facing
/// turned round).
const USE_REACH: f32 = 4.0;
const USE_RISE_PAD: f32 = 1.0;
const USE_FACING_DEG: f32 = 65.0;
/// AP_StoragePodOpen's first notify.
const SND_OPEN: &str = "BL_TRANS_PROP_INTERACT.LOOT_BOX_OPEN";
/// What it made can be taken this long after (TnDroppedPickup's PickupDelay), its
/// pickup cylinder this many times a dropped weapon's (main.rs's PICKUP_REACH, and
/// the height round it).
pub const LOOT_DELAY: f32 = 2.5;
pub const LOOT_SIZE: f32 = 2.0;
/// The outline: OutlineIntensity, and its flare (ScanLinesIntensity, at
/// ScanLinesPannerSpeed: 1 + 5 (1 - |sin(2 pi 0.2 t)|)^2 times as bright, in its
/// live material's shader: live.rs).
const OUTLINE_INTENSITY: f32 = 2.0;
const OUTLINE_FLARE: f32 = 5.0;
const OUTLINE_FLARE_RATE: f32 = 0.2;
/// The steam its opening lets out (AP_StoragePodOpen's HmAnimNotify_PlayEffects):
/// ReloadSteam_Titan_FX at each of its FX sockets at the notify's time (seconds into
/// the opening), following the socket while it plays (attached to it). The sockets
/// are the model's nodes (tools/build_pods.py's STEAM: the socket's frame turned by
/// the notify's offset).
const STEAM_FX: &str = "ReloadSteam_Titan_FX";
const STEAM: [(f32, &str); 5] = [
    (0.0213, "Steam_TopLeftFXSocket"),
    (0.0390, "Steam_TopRightFXSocket"),
    (0.4929, "Steam_TopCenterFXSocket"),
    (1.5709, "Steam_SideLeftFXSocket"),
    (1.5957, "Steam_SideRightFXSocket"),
];
/// The box that stands in for a pod's collision where the map's has none (Oblivion):
/// in its own axes, from its back to its door, side to side, and up (its model's
/// extent, a little inside it).
const POD_BOX: (Vec3, Vec3) = (Vec3::new(-2.6, 0.0, -1.8), Vec3::new(0.75, 7.3, 1.8));

/// One thing the loot table can make.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Loot {
    /// A weapon (its WEAPONS id).
    Weapon(&'static str),
    /// A support item.
    Gear(Equipment),
}

/// TR_Survival_m.ESC_LootTableBase (the same on every map): its "LootBox"
/// classification, which these roll once at ChanceToDrop 100 (always one thing),
/// each with its Weight (of TotalWeight 6.05).
pub const LOOT: [(Loot, f32); 9] = [
    (Loot::Weapon("SawBlade"), 1.0),
    (Loot::Weapon("HomingRocket"), 1.0),
    (Loot::Weapon("DrainGun"), 1.0),
    (Loot::Gear(Equipment::MinePayload), 1.0),
    (Loot::Weapon("AoESlow"), 0.65),
    (Loot::Weapon("GrenadeLauncher"), 0.65),
    (Loot::Weapon("HeavyMG"), 0.5),
    (Loot::Weapon("ClusterCannon"), 0.15),
    (Loot::Gear(Equipment::Decimator), 0.1),
];

/// The thing a roll `r` (0..1) of the loot table makes (its LOOT number).
pub fn roll(r: f32) -> usize {
    let total: f32 = LOOT.iter().map(|(_, w)| w).sum();
    let mut left = r.clamp(0.0, 1.0) * total;
    for (i, (_, w)) in LOOT.iter().enumerate() {
        if left < *w {
            return i;
        }
        left -= w;
    }
    LOOT.len() - 1
}

/// A map's pod: where it stands, its sockets and the spot in front of it.
#[derive(Clone, Copy, Debug)]
pub struct PodSpot {
    pub at: Transform,
    pub weapon_socket: Transform,
    pub misc_socket: Transform,
    pub spot: Vec3,
}

/// How far behind the pod's middle its back wall inside is at its nearest (its
/// front is +X: models/storage_pod.glb's back wall runs from 1.0 m back near the
/// floor to 1.25 m under the roof). What it makes stands in front of it, but for a
/// weapon's back, which is in it.
const POD_BACK_WALL: f32 = 1.0;

impl PodSpot {
    /// The pod's back wall inside, the plane behind what it makes (facing out of
    /// the pod): its outline goes no further back (outline.rs).
    pub fn back_wall(&self) -> crate::outline::OutlineBackstop {
        crate::outline::OutlineBackstop::facing(
            self.at.rotation * Vec3::X,
            self.at.transform_point(Vec3::new(-POD_BACK_WALL, 0.0, 0.0)),
        )
    }
}

/// A map's pods (its levels/<map>_pods.txt), and whether its collision lacks them.
pub fn load_pods(path: &Path) -> (Vec<PodSpot>, bool) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return (Vec::new(), false);
    };
    let mut pods = Vec::new();
    let mut collide = false;
    for line in text.lines() {
        let mut parts = line.split('\t');
        match parts.next() {
            Some("collide") => collide = true,
            Some("pod") => {
                let _name = parts.next();
                let v: Vec<f32> = parts
                    .flat_map(|p| p.split_whitespace())
                    .filter_map(|x| x.parse().ok())
                    .collect();
                if v.len() < 27 {
                    continue;
                }
                let vec3 = |i: usize| Vec3::new(v[i], v[i + 1], v[i + 2]);
                let quat =
                    |i: usize| Quat::from_xyzw(v[i], v[i + 1], v[i + 2], v[i + 3]).normalize();
                pods.push(PodSpot {
                    at: Transform {
                        translation: vec3(0),
                        rotation: quat(3),
                        scale: vec3(7),
                    },
                    weapon_socket: Transform::from_translation(vec3(10)).with_rotation(quat(13)),
                    misc_socket: Transform::from_translation(vec3(17)).with_rotation(quat(20)),
                    spot: vec3(24),
                });
            }
            _ => {}
        }
    }
    (pods, collide)
}

/// Solid triangles of each pod's stand-in box (POD_BOX), for a map without their
/// collision.
pub fn pod_boxes(pods: &[PodSpot]) -> Vec<[f32; 9]> {
    let (lo, hi) = POD_BOX;
    let corner = |x: bool, y: bool, z: bool| {
        Vec3::new(
            if x { hi.x } else { lo.x },
            if y { hi.y } else { lo.y },
            if z { hi.z } else { lo.z },
        )
    };
    // Its six faces, two triangles each (as corners: x, y, z high or not).
    const FACES: [[(bool, bool, bool); 4]; 6] = [
        [
            (false, false, false),
            (false, true, false),
            (false, true, true),
            (false, false, true),
        ],
        [
            (true, false, false),
            (true, false, true),
            (true, true, true),
            (true, true, false),
        ],
        [
            (false, false, false),
            (false, false, true),
            (true, false, true),
            (true, false, false),
        ],
        [
            (false, true, false),
            (true, true, false),
            (true, true, true),
            (false, true, true),
        ],
        [
            (false, false, false),
            (true, false, false),
            (true, true, false),
            (false, true, false),
        ],
        [
            (false, false, true),
            (false, true, true),
            (true, true, true),
            (true, false, true),
        ],
    ];
    let mut tris = Vec::new();
    for pod in pods {
        let m = pod.at.to_matrix();
        for face in FACES {
            let p = face.map(|(x, y, z)| m.transform_point3(corner(x, y, z)));
            for [a, b, c] in [[p[0], p[1], p[2]], [p[0], p[2], p[3]]] {
                tris.push([a.x, a.y, a.z, b.x, b.y, b.z, c.x, c.y, c.z]);
            }
        }
    }
    tris
}

/// A pod in the map (its number in the map's list), whether it has been used, and its
/// parts as its scene comes in: the bones' player, its outline and its steam's
/// sockets; once it opens, when, the puffs of steam let out and those still going.
#[derive(Component, Default)]
pub struct Pod {
    pub index: usize,
    pub used: bool,
    opened: Option<f32>,
    player: Option<Entity>,
    outline: Option<Entity>,
    sockets: [Option<Entity>; STEAM.len()],
    puffs: usize,
    steam: Vec<(FxId, usize)>,
}

/// What a pod made, waiting to be taken (main.rs's weapon_pickups): its pod, its LOOT
/// number, the spot it is taken from and the seconds before it can be.
#[derive(Component)]
pub struct PodLoot {
    pub pod: usize,
    pub item: usize,
    pub spot: Vec3,
    pub delay: f32,
}

/// The other players' pod events (net_apply's), to play out here.
#[derive(Resource, Default)]
pub struct PodInbox(pub Vec<JazzEvent>);

/// The pod's opening as an animation graph (every pod plays it).
#[derive(Resource)]
struct PodAnimation {
    graph: Handle<AnimationGraph>,
    node: AnimationNodeIndex,
}

pub struct RecreatorPlugin;

impl Plugin for RecreatorPlugin {
    fn build(&self, app: &mut App) {
        // (The pods' outlines glow in their live material: live.rs.)
        app.init_resource::<crate::live::LiveOutlines>();
        app.world_mut()
            .resource_mut::<crate::live::LiveOutlines>()
            .0
            .push(crate::live::Outline {
                prefix: OUTLINE_NODE,
                glow: LinearRgba::new(0.0, OUTLINE_INTENSITY, OUTLINE_INTENSITY, 1.0),
                wave: crate::live::Wave::Flare {
                    rate: OUTLINE_FLARE_RATE,
                    amount: OUTLINE_FLARE,
                },
            });
        app.init_resource::<PodInbox>()
            .add_systems(Startup, load_animation)
            .add_systems(OnEnter(crate::menu::AppState::Game), reset_pods)
            // (Whatever the screen: a map comes in, and is joined, while the menus are
            // up, at the start and through the lobby's countdown.)
            .add_systems(Update, drop_baked_pods.before(crate::merge::JoinSystems))
            .add_systems(
                Update,
                (
                    attach_pods,
                    use_pods
                        .after(crate::weapon_pickups)
                        .before(crate::hud::HudSystems),
                    remote_pods.after(crate::net_apply),
                    play_pods,
                )
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

fn load_animation(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut clips: ResMut<crate::GltfClips>,
) {
    // (From the pod's file as a whole, as its scene: gltffiles.rs.)
    let mut graph = AnimationGraph::new();
    let mut pending = Vec::new();
    let root = graph.root;
    let node = crate::gltf_clip(&mut graph, &mut pending, 0, root);
    let graph = graphs.add(graph);
    clips.wait(asset_server.load(POD_MODEL), graph.clone(), pending);
    commands.insert_resource(PodAnimation { graph, node });
}

/// A new match (each a new one in the game): every pod closed and usable again,
/// what they made gone.
fn reset_pods(
    mut commands: Commands,
    animation: Option<Res<PodAnimation>>,
    mut fx: ResMut<FxSystems>,
    mut pods: Query<&mut Pod>,
    mut players: Query<&mut AnimationPlayer>,
    loot: Query<Entity, With<PodLoot>>,
    mut inbox: ResMut<PodInbox>,
) {
    inbox.0.clear();
    for e in &loot {
        commands.entity(e).despawn();
    }
    for mut pod in &mut pods {
        if !pod.used {
            continue;
        }
        pod.used = false;
        pod.opened = None;
        pod.puffs = 0;
        for (id, _) in std::mem::take(&mut pod.steam) {
            fx.stop(id);
        }
        // (Held closed: its opening's first frame.)
        if let (Some(animation), Some(Ok(mut player))) =
            (animation.as_deref(), pod.player.map(|p| players.get_mut(p)))
        {
            player.stop_all();
            player.play(animation.node).set_speed(0.0);
        }
    }
}

/// The map's pods, as it comes in (main.rs).
pub fn spawn_pods(
    commands: &mut Commands,
    asset_server: &AssetServer,
    pods: &[PodSpot],
) -> Vec<Entity> {
    if pods.is_empty() {
        return Vec::new();
    }
    let scene = crate::gltffiles::SceneOf::load(asset_server, POD_MODEL);
    pods.iter()
        .enumerate()
        .map(|(index, pod)| {
            commands
                .spawn((
                    Pod { index, ..default() },
                    scene.clone(),
                    pod.at,
                    Visibility::default(),
                    crate::OwnAnimation,
                ))
                .id()
        })
        .collect()
}

/// The maps' own pods out of the map's model, once its scene is in and before its
/// parts are joined (they would be joined with it).
fn drop_baked_pods(
    mut commands: Commands,
    roots: Query<(Entity, &bevy::scene::SceneInstance), With<crate::merge::MapScene>>,
    spawner: Res<bevy::scene::SceneSpawner>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut done: Local<Vec<Entity>>,
) {
    done.retain(|root| roots.contains(*root));
    for (root, instance) in &roots {
        if done.contains(&root) || !spawner.instance_is_ready(**instance) {
            continue;
        }
        done.push(root);
        let baked: Vec<Entity> = children
            .iter_descendants(root)
            .filter(|e| names.get(*e).is_ok_and(|n| n.as_str() == BAKED_POD))
            .collect();
        for e in baked {
            commands.entity(e).despawn();
        }
    }
}

/// A pod's parts as its scene comes in: its bones' player is given the opening's
/// graph, and its outline (the part drawn with its material, under its node) and
/// its steam's sockets are found.
#[allow(clippy::type_complexity)]
fn attach_pods(
    mut commands: Commands,
    animation: Option<Res<PodAnimation>>,
    added: Query<
        (Entity, Option<&Name>, Has<AnimationPlayer>, Has<Mesh3d>),
        Or<(Added<AnimationPlayer>, Added<Name>)>,
    >,
    parents: Query<&ChildOf>,
    mut pods: Query<&mut Pod>,
) {
    for (e, name, player, drawn) in &added {
        let outline = drawn && name.is_some_and(|n| n.as_str().starts_with(OUTLINE_NODE));
        let socket = name.and_then(|n| STEAM.iter().position(|(_, s)| n.as_str() == *s));
        if !player && !outline && socket.is_none() {
            continue;
        }
        let Some(pod_entity) = parents.iter_ancestors(e).find(|a| pods.contains(*a)) else {
            continue;
        };
        let Ok(mut pod) = pods.get_mut(pod_entity) else {
            continue;
        };
        if player && let Some(animation) = animation.as_deref() {
            commands
                .entity(e)
                .insert(AnimationGraphHandle(animation.graph.clone()));
            pod.player = Some(e);
        }
        if outline {
            pod.outline = Some(e);
            commands
                .entity(e)
                .insert((bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
        }
        if let Some(k) = socket {
            pod.sockets[k] = Some(e);
        }
    }
}

/// Whether he can use a pod whose front spot is `spot` and which faces `front`: in
/// reach of the spot, facing the pod, able (UseConditions).
pub(crate) fn can_use(player: &Player, at: Vec3, forward: Vec3, spot: Vec3, front: Vec3) -> bool {
    let flat = Vec2::new(spot.x - at.x, spot.z - at.z).length();
    let rise = (spot.y - at.y).abs();
    let facing = Vec2::new(forward.x, forward.z).normalize_or_zero();
    let toward = Vec2::new(-front.x, -front.z).normalize_or_zero();
    let meleeing = player
        .action
        .is_some_and(|(clip, _)| clip == crate::ANIM_MELEE || clip == crate::ANIM_MELEE_BOOST);
    flat < USE_REACH
        && rise < player.kit.robot.height + USE_RISE_PAD
        && facing.dot(toward) >= USE_FACING_DEG.to_radians().cos()
        && player.form == Form::Robot
        && player.dead_left <= 0.0
        && !player.dying
        && player.transform_left <= 0.0
        && player.pending_form.is_none()
        && !player.revive.locked()
        && player.revive.hold_target.is_none()
        && !meleeing
}

/// This PC's player at the pods: the offer while one is in reach (unless something
/// to pick up is, weapon_pickups's), and its use as he presses the key.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn use_pods(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    level: Res<crate::Level>,
    mut score: crate::split::BySeat<Score>,
    mut sfx: ResMut<Sfx>,
    mut hud: crate::split::BySeat<HudState>,
    mut me: Query<
        (
            &Transform,
            &mut Player,
            &PlayerInput,
            Option<&crate::split::Seat>,
        ),
        (With<LocalPlayer>, Without<RemotePlayer>),
    >,
    mut pods: Query<(&mut Pod, &Transform), Without<Player>>,
    (mut pressed_before, mut touched, mut seed): (
        Local<[bool; crate::split::SEATS]>,
        Local<[Option<usize>; crate::split::SEATS]>,
        Local<u32>,
    ),
) {
    // (Each of this PC's players: split screen's guest with his own energon and HUD.)
    for (transform, player, input, seat) in &mut me {
        let seat = crate::split::seat_of(seat);
        pod_for(
            (&mut commands, &asset_server, &level),
            (score.seat(seat), &mut sfx, hud.seat(seat)),
            (transform, player, input),
            &mut pods,
            (&mut pressed_before[seat], &mut touched[seat], &mut seed),
        );
    }
}

/// One of this PC's players at the pods (use_pods).
#[allow(clippy::type_complexity)]
fn pod_for(
    (commands, asset_server, level): (&mut Commands, &AssetServer, &crate::Level),
    (score, sfx, hud): (&mut Score, &mut Sfx, &mut HudState),
    (transform, mut player, input): (&Transform, Mut<Player>, &PlayerInput),
    pods: &mut Query<(&mut Pod, &Transform), Without<Player>>,
    (pressed_before, touched, seed): (&mut bool, &mut Option<usize>, &mut u32),
) {
    let pressed = input.pick_up && !*pressed_before;
    *pressed_before = input.pick_up;
    let at = transform.translation;
    let forward = *transform.forward();
    // (Something to pick up in reach has the key: weapon_pickups's offer stands.)
    let free = hud.prompt.is_none();
    let target = pods
        .iter_mut()
        .filter(|(pod, _)| !pod.used)
        .filter_map(|(pod, pt)| {
            let spot = level.pods.get(pod.index)?.spot;
            can_use(&player, at, forward, spot, pt.rotation * Vec3::X)
                .then(|| (pod.index, Vec2::new(spot.x - at.x, spot.z - at.z).length()))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
        .filter(|_| free);
    let Some(index) = target else {
        *touched = None;
        return;
    };
    hud.prompt = Some(Prompt::Recreator(COST));
    // (Touched: the counter shown, red when short.)
    if *touched != Some(index) {
        *touched = Some(index);
        hud.events.push(HudEvent::CurrencyNotify {
            low: score.energon < COST,
        });
    }
    if !pressed {
        return;
    }
    if score.energon < COST {
        sfx.play(crate::SND_NO_FUNDS);
        hud.events.push(HudEvent::CurrencyNotify { low: true });
        return;
    }
    score.energon -= COST;
    // One roll of the loot table.
    if *seed == 0 {
        *seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.subsec_nanos() | 1);
    }
    let mut x = *seed;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *seed = x;
    let item = roll((x >> 8) as f32 / (1u32 << 24) as f32);
    hud.prompt = None;
    if let Some((mut pod, _)) = pods.iter_mut().find(|(p, _)| p.index == index) {
        open(commands, asset_server, sfx, &level.pods, &mut pod, item);
    }
    player.played.push(JazzEvent::Pod {
        pod: index as u8,
        item: item as u8,
    });
}

/// A pod used (here or on the buyer's PC): it opens, its outline goes, and what it
/// made (`item`, a LOOT number) waits in it.
fn open(
    commands: &mut Commands,
    asset_server: &AssetServer,
    sfx: &mut Sfx,
    spots: &[PodSpot],
    pod: &mut Pod,
    item: usize,
) {
    if pod.used {
        return;
    }
    pod.used = true;
    let Some(spot) = spots.get(pod.index) else {
        return;
    };
    sfx.play_at(SND_OPEN, spot.at.translation + Vec3::Y * 2.0);
    let Some(&(loot, _)) = LOOT.get(item) else {
        return;
    };
    let (frame, model) = match loot {
        Loot::Weapon(id) => {
            let Some(model) = crate::weapon_index(id).map(|w| crate::WEAPONS[w].pickup_model)
            else {
                return;
            };
            (
                spot.weapon_socket * crate::shop::goods_transform(model),
                model,
            )
        }
        Loot::Gear(kind) => (spot.misc_socket, kind.model()),
    };
    let mut e = commands.spawn((
        PodLoot {
            pod: pod.index,
            item,
            spot: spot.spot,
            delay: LOOT_DELAY,
        },
        frame,
        Visibility::default(),
        crate::MapPart,
        spot.back_wall(),
    ));
    if crate::assets_dir().join(model).exists() {
        e.insert(SceneRoot(
            asset_server.load(GltfAssetLabel::Scene(0).from_asset(model)),
        ));
    }
}

/// The other players' pods used and their loot taken (net_apply's PodInbox).
fn remote_pods(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    level: Res<crate::Level>,
    mut sfx: ResMut<Sfx>,
    mut inbox: ResMut<PodInbox>,
    mut pods: Query<&mut Pod>,
    loot: Query<(Entity, &PodLoot)>,
) {
    for event in std::mem::take(&mut inbox.0) {
        match event {
            JazzEvent::Pod { pod, item } => {
                if let Some(mut p) = pods.iter_mut().find(|p| p.index == pod as usize) {
                    open(
                        &mut commands,
                        &asset_server,
                        &mut sfx,
                        &level.pods,
                        &mut p,
                        item as usize,
                    );
                }
            }
            JazzEvent::PodLoot { pod } => {
                for (e, l) in &loot {
                    if l.pod == pod as usize {
                        commands.entity(e).despawn();
                    }
                }
            }
            _ => {}
        }
    }
}

/// Where a steam socket is: its place and its X and Y (Bevy's axes).
fn socket_frame(g: &GlobalTransform) -> (Vec3, Vec3, Vec3) {
    let (_, turn, at) = g.to_scale_rotation_translation();
    (at, turn * Vec3::X, turn * Vec3::Y)
}

/// The pods' openings played, with their steam, and their outlines: shown while
/// they can be used (flaring in their live material: live.rs), gone once used.
fn play_pods(
    time: Res<Time>,
    animation: Option<Res<PodAnimation>>,
    mut fx: ResMut<FxSystems>,
    mut pods: Query<&mut Pod>,
    mut players: Query<&mut AnimationPlayer>,
    sockets: Query<&GlobalTransform>,
    mut outlines: Query<&mut Visibility, With<Mesh3d>>,
) {
    let t = time.elapsed_secs();
    for mut pod in &mut pods {
        if pod.used
            && pod.opened.is_none()
            && let (Some(animation), Some(Ok(mut player))) =
                (animation.as_deref(), pod.player.map(|p| players.get_mut(p)))
        {
            player.start(animation.node).set_speed(1.0);
            pod.opened = Some(t);
        }
        // Its steam: each puff as its time comes, following its socket.
        if let Some(start) = pod.opened {
            while let Some(&(at_secs, _)) = STEAM.get(pod.puffs) {
                if t - start < at_secs {
                    break;
                }
                let k = pod.puffs;
                pod.puffs += 1;
                if let Some(Ok(g)) = pod.sockets[k].map(|s| sockets.get(s)) {
                    let (at, x, y) = socket_frame(g);
                    if let Some(id) = fx.spawn(STEAM_FX, at, x, &[]) {
                        fx.place_turned(id, at, x, y);
                        pod.steam.push((id, k));
                    }
                }
            }
            let sockets_of = pod.sockets;
            pod.steam.retain(|&(id, k)| {
                if !fx.alive(id) {
                    return false;
                }
                if let Some(Ok(g)) = sockets_of[k].map(|s| sockets.get(s)) {
                    let (at, x, y) = socket_frame(g);
                    fx.place_turned(id, at, x, y);
                }
                true
            });
        }
        if let Some(Ok(mut visibility)) = pod.outline.map(|o| outlines.get_mut(o)) {
            let want = if pod.used {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
            if *visibility != want {
                *visibility = want;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plane behind a pod's loot is its back wall, 1 m behind its middle, facing
    /// out of the pod (its +X, wherever the pod is turned): its weapon socket is in
    /// front of it, the spot further in front.
    #[test]
    fn a_pods_back_wall_is_behind_its_sockets() {
        let at = Transform::from_xyz(10.0, -5.0, 3.0).with_rotation(Quat::from_rotation_y(0.7));
        let pod = PodSpot {
            at,
            weapon_socket: Transform::from_translation(
                at.transform_point(Vec3::new(-0.65, 2.0, 0.0)),
            ),
            misc_socket: Transform::from_translation(
                at.transform_point(Vec3::new(-1.13, 2.5, 0.0)),
            ),
            spot: at.transform_point(Vec3::new(5.0, 0.0, 0.0)),
        };
        let wall = pod.back_wall().0;
        let side = |p: Vec3| wall.truncate().dot(p) + wall.w;
        assert!((side(at.transform_point(Vec3::new(-1.0, 3.0, 0.4)))).abs() < 1e-4);
        assert!((side(pod.weapon_socket.translation) - 0.35).abs() < 1e-4);
        assert!(side(pod.spot) > 5.0);
    }

    #[test]
    fn the_loot_table_is_the_games() {
        let total: f32 = LOOT.iter().map(|(_, w)| w).sum();
        assert!((total - 6.05).abs() < 1e-4);
        assert_eq!(roll(0.0), 0);
        assert_eq!(roll(0.999_999), LOOT.len() - 1);
        // Each comes up as often as its weight says.
        let mut counts = [0usize; LOOT.len()];
        let n = 60_500;
        for i in 0..n {
            counts[roll((i as f32 + 0.5) / n as f32)] += 1;
        }
        for (i, (_, w)) in LOOT.iter().enumerate() {
            let expect = n as f32 * w / total;
            assert!(
                (counts[i] as f32 - expect).abs() <= 2.0,
                "{i}: {} vs {expect}",
                counts[i]
            );
        }
        // Every weapon in it is one the demo has.
        for (loot, _) in LOOT {
            if let Loot::Weapon(id) = loot {
                assert!(crate::weapon_index(id).is_some(), "{id}");
            }
        }
    }

    #[test]
    fn reads_the_pods() {
        let path = crate::assets_dir().join("levels/downfall_pods.txt");
        let (pods, collide) = load_pods(&path);
        if pods.is_empty() {
            eprintln!("{} not built: skipped", path.display());
            return;
        }
        assert_eq!(pods.len(), 8);
        assert!(!collide);
        // Pod10654 (yaw 0): its spot 5 m along its front (+X), its weapon socket
        // (-65, 0, 200) on it.
        let p = pods[0];
        assert!((p.spot - (p.at.translation + Vec3::X * 5.0)).length() < 1e-3);
        assert!(
            (p.weapon_socket.translation - (p.at.translation + Vec3::new(-0.65, 2.0, 0.0)))
                .length()
                < 1e-3
        );
        let (obl, collide) = load_pods(&crate::assets_dir().join("levels/oblivion_pods.txt"));
        if !obl.is_empty() {
            assert!(collide);
            assert_eq!(pod_boxes(&obl).len(), obl.len() * 12);
        }
    }
}
