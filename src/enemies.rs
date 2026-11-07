//! Escalation's enemies: the AI types each map's spawn list fields
//! (tools/build_enemies.py's models/enemies), as the game builds them. The waves
//! bring them in (waves.rs); they come without their AI (they neither move nor
//! fight): each stands where it came in, or hovers there (a jet).
//!
//! * **Its look**: its model, a mesh set's slots each with one of their parts (or
//!   none) picked at random, as TnMeshSetRandom picks them, painted in one of its
//!   description's colour pairs (CharacterColorPairs) with its EnergonColor's glow,
//!   playing its idle (its anim tree's idle: the soldiers' patrol stance).
//! * **Its body**: its physics asset's bodies on its bones, where this frame's pose
//!   puts them, are what shots, blades and blasts strike ([`Body`]): its head's for
//!   a headshot.
//! * **Its health**: its behaviour's Health, its form's DamageMultiplier on what it
//!   takes ([`Enemy::take`], main.rs's hit code).
//! * **Its death**: its death set's death for the kill (deaths.rs: TnDeathPlayer.Pick,
//!   the first death whose death types the kill has: the damage type's
//!   DamageDeathTypes, the side it was struck on, a headshot, a gib, its form's),
//!   played out as its timeline has it: its clips, the ragdoll taking over
//!   (ragdoll.rs) with the clip's velocities, the kill's momentum and its ragdoll
//!   actions, its effects on its sockets, its lights, sounds and camera shakes, its
//!   destructibles' blasts, decals and gibs (gibs.rs), the dissolve (TnDeresser: its
//!   overlay, the game's cyan TR_DefragOverlay_MATINST for most deaths, drawn over
//!   it out from its middle, the body itself gone once the overlay has covered it,
//!   the overlay burning out: paint.wgsl's defrag_overlay); then it is gone.
//! * **Its energon**: Escalation's money for the kill (TR_Survival_m.
//!   SurvivalScoring_p: its AI type's score, times the difficulty's multiplier),
//!   split as TnSurvivalScoring does: the killer is paid for the health the others
//!   didn't take, each who helped for what he took (the player at this PC is paid
//!   his share, as each PC pays its own); a kill nobody made is shared, half to
//!   each.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::animation::graph::AnimationNodeIndex;
use bevy::gltf::GltfMaterialExtras;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

use crate::character::{PaintCache, PaintMaterial, is_paint};
use crate::ragdoll::{PhysicsAsset, Rig, RigState, Shape};
use crate::{GltfClips, LocalPlayer, Score, Sfx, deaths, gltf_clip, net, orbital, specials, xp};

/// The enemies' data (tools/build_enemies.py).
const ENEMIES_FILE: &str = "models/enemies/enemies.txt";
const MODEL_DIR: &str = "models/enemies";

/// TnDeresser's NormalizedTimeToMakeMeshInvisible: past this DefragValue the body
/// is drawn no more, only its overlay.
pub const INVISIBLE: f32 = 0.39685;
/// What a kill nobody made pays each player (TnSurvivalScoring's SharedKill).
const SHARED_KILL: f32 = 0.5;
/// The Titan's drones' death sound (their vehicle form's DeathSound:
/// TnVehicleForm.OnPlayDeath; their death set has none).
const SND_DRONE_DEATH: &str = "BL_CHR_AI_SENTINEL.SENTINEL_DEATH";

// ---------------------------------------------------------------------------
// The types
// ---------------------------------------------------------------------------

/// The form a type starts in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Robot,
    /// The Flyers' jets, hovering.
    Jet,
    /// The Titan's drones.
    Orb,
    /// The Swarmer.
    Insect,
}

/// The death set a type dies by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeathSet {
    /// TR_Deaths_p.Sets.Basic_DEATHSET: the soldiers' (and the players' robots').
    Basic,
    /// TR_Titan_AI_p.Deaths.Titan_DEATHSET: the Titan's and the Heavy Gunners'.
    Titan,
    Guardian,
    Leaper,
    Swarmer,
    Sniper,
    /// The drones' (TitanExplosiveDrone_DEATHSET).
    Drone,
    /// The Flyers' jets' (Flyer_DEATHSET).
    Flyer,
}

/// A clip of a type's: what it is for (idle, death_front...), its file, its index
/// there and its length. (Whether it loops, the file's last column, is the role's:
/// its idle does.)
#[derive(Clone, Debug)]
pub struct ClipRef {
    pub role: String,
    pub file: String,
    pub index: usize,
    pub secs: f32,
}

/// An enemy type (enemies.txt's `type` and the lines after it).
pub struct Kind {
    pub id: String,
    /// Its model (under assets/).
    pub model: String,
    pub form: Form,
    /// Its name as the game shows it (TransGame.int's [AICharacterNames]).
    pub name: String,
    pub ai_type: String,
    /// Escalation's money for killing one.
    pub score: f32,
    pub health: f32,
    /// What it takes of a blow (its form's DamageMultiplier).
    pub damage_scale: f32,
    /// Its collision cylinder (metres).
    pub radius: f32,
    pub half_height: f32,
    /// Its glow (EnergonColor, linear), and its alpha (half for the jets, none for
    /// a few).
    pub energon: Vec3,
    pub energon_alpha: f32,
    /// It is a Decepticon (its character's Faction; an Autobot's is the enum's
    /// first, left unwritten: "-").
    pub decepticon: bool,
    pub colours: Vec<[[u8; 3]; 2]>,
    /// Each slot's parts (their node names), None for the slot left empty.
    pub slots: Vec<Vec<Option<String>>>,
    pub physics: Arc<PhysicsAsset>,
    /// Its sockets: name, bone, place and turn on the bone; and the scales of those
    /// that have one (Unreal's axes: RelativeScale), by name.
    pub sockets: Vec<(String, String, Vec3, Quat)>,
    pub socket_scales: Vec<(String, Vec3)>,
    /// Its gun's mesh for each hand (right, left: their nodes), if it carries one
    /// (its weapon's TnWeaponMesh handedness meshes: its AI shows the one for the
    /// hand it holds it in), and those meshes' sockets: hand (0 right, 1 left),
    /// name, bone, place on the bone.
    pub guns: [Option<String>; 2],
    pub gun_sockets: Vec<(usize, String, String, Vec3, Quat)>,
    /// Its vehicle gun's meshes on its attachment sockets (a jet's dogfight gun's
    /// launchers: its AI shows them while it has that gun out), each its node and
    /// the gun's class by its attachment's number; and their sockets: number,
    /// name, bone, place and turn on the bone.
    pub vguns: Vec<(usize, String, String)>,
    pub vgun_sockets: Vec<(usize, String, String, Vec3, Quat)>,
    pub clips: Vec<ClipRef>,
    pub deaths: DeathSet,
    /// Its model's scale (its MeshSet's MeshScale).
    pub scale: f32,
    /// It has the soldiers' skeleton (and its AI plays their clips).
    pub npc: bool,
    /// Its clips' graph, and each role's nodes with their lengths; its AI's
    /// (ai/graph.rs), if its AI animates it.
    pub graph: Handle<AnimationGraph>,
    pub nodes: HashMap<String, Vec<(AnimationNodeIndex, f32)>>,
    pub ai: Option<Arc<crate::ai::graph::AiNodes>>,
    /// A type that changes form as it fights: the type its other form is (its
    /// `other` line: <id>~other, in no roster), whose model it brings along.
    pub other: Option<usize>,
}

impl Kind {
    /// Its gun is one mesh hung on each of its weapon's attachment sockets
    /// (TnWeapon._AttachmentSockets: the Guardian Blaster's WeaponSocket_Primary and
    /// WeaponSocket_Primary_L, an arm cannon on each arm), not a mesh for each hand:
    /// each hand's gun carries its sockets on its own bones (`<node>:<bone>`).
    /// TnWeapon.AttachWeaponMeshes attaches such a mesh on every one of them, so
    /// they all show while it has its gun out.
    pub fn guns_hung_on_both_arms(&self) -> bool {
        self.guns.iter().enumerate().all(|(h, gun)| {
            gun.as_ref().is_some_and(|node| {
                let own = format!("{node}:");
                let mut socks = self.gun_sockets.iter().filter(|s| s.0 == h).peekable();
                socks.peek().is_some() && socks.all(|s| s.2.starts_with(&own))
            })
        })
    }

    /// Its EnergonColor as its effects take it (TnPawn.get_EnergonColor: its
    /// character applier's, LinearColorToSRGBColor's, its alpha with it: UE3's
    /// FLinearColor::ToFColor(sRGB)).
    pub fn energon_color(&self) -> [u8; 4] {
        crate::car::effect_color(
            crate::car::energon_srgb(self.energon),
            (self.energon_alpha.clamp(0.0, 1.0) * 255.999) as u8,
        )
    }

    /// Its effects' module channel (TnParticleSystemParameterTeamIndex.Apply: an
    /// AI's team is none, 255, so its character's faction's: A for an Autobot's, B
    /// for a Decepticon's).
    pub fn module_channel(&self) -> [u8; 4] {
        if self.decepticon {
            crate::levelfx::MODULE_CHANNEL_B
        } else {
            crate::levelfx::MODULE_CHANNEL_A
        }
    }
}

/// The enemy types and each map's.
#[derive(Resource, Default)]
pub struct Kinds {
    pub kinds: Vec<Kind>,
    /// Each map's types (by its files' name: downfall...), in its spawn list's order.
    pub maps: HashMap<String, Vec<usize>>,
    /// The money's multiplier by difficulty (Easy, Normal, Hard).
    pub difficulty: Vec<f32>,
}

fn floats(s: &str) -> Vec<f32> {
    s.split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect()
}

/// A socket line's place on its bone and its turn there (none in older files).
fn socket_frame(f: &[&str]) -> (Vec3, Quat) {
    let v = f.first().map(|s| floats(s)).unwrap_or_default();
    let at = if v.len() >= 3 {
        Vec3::new(v[0], v[1], v[2])
    } else {
        Vec3::ZERO
    };
    let q = f.get(1).map(|s| floats(s)).unwrap_or_default();
    let turn = if q.len() >= 4 {
        Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize()
    } else {
        Quat::IDENTITY
    };
    (at, turn)
}

fn deaths_of(ai_type: &str, form: Form) -> DeathSet {
    match (ai_type, form) {
        (_, Form::Jet) => DeathSet::Flyer,
        (_, Form::Orb) => DeathSet::Drone,
        ("AICT_Titan" | "AICT_HeavyGunner", _) => DeathSet::Titan,
        ("AICT_Guard", _) => DeathSet::Guardian,
        ("AICT_Leaper", _) => DeathSet::Leaper,
        ("AICT_Swarmer", _) => DeathSet::Swarmer,
        ("AICT_Sniper", _) => DeathSet::Sniper,
        _ => DeathSet::Basic,
    }
}

impl Kinds {
    /// enemies.txt (no types without it).
    pub fn load(assets: &std::path::Path) -> Kinds {
        let mut out = Kinds {
            difficulty: vec![0.75, 1.0, 1.25],
            ..default()
        };
        let Ok(text) = std::fs::read_to_string(assets.join(ENEMIES_FILE)) else {
            info!("{ENEMIES_FILE} not found: no enemies");
            return out;
        };
        let mut index: HashMap<String, usize> = HashMap::new();
        let mut maps: Vec<(String, Vec<String>)> = Vec::new();
        let mut npc_clips: Vec<ClipRef> = Vec::new();
        let mut npc: HashSet<usize> = HashSet::new();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let kind = f.get(1).and_then(|id| index.get(*id).copied());
            match f[0] {
                "map" if f.len() >= 2 => {
                    maps.push((
                        f[1].to_string(),
                        f[2..].iter().map(|s| s.to_string()).collect(),
                    ));
                }
                "difficulty" => {
                    let d: Vec<f32> = f[1..]
                        .iter()
                        .filter_map(|x| x.trim().parse().ok())
                        .collect();
                    if !d.is_empty() {
                        out.difficulty = d;
                    }
                }
                "type" if f.len() >= 14 => {
                    let n = |i: usize| f[i].trim().parse::<f32>().unwrap_or(0.0);
                    let form = match f[3] {
                        "jet" => Form::Jet,
                        "orb" => Form::Orb,
                        "insect" => Form::Insect,
                        _ => Form::Robot,
                    };
                    let e = floats(f[12]);
                    let energon = if e.len() >= 3 {
                        Vec3::new(e[0], e[1], e[2])
                    } else {
                        Vec3::ONE
                    };
                    let energon_alpha = e.get(3).copied().unwrap_or(1.0);
                    index.insert(f[1].to_string(), out.kinds.len());
                    out.kinds.push(Kind {
                        id: f[1].to_string(),
                        model: format!("{MODEL_DIR}/{}", f[2]),
                        form,
                        name: f[4].to_string(),
                        ai_type: f[6].to_string(),
                        score: n(7),
                        health: n(8).max(1.0),
                        damage_scale: n(9),
                        radius: n(10),
                        half_height: n(11),
                        energon,
                        energon_alpha,
                        decepticon: f[5].trim() == "Decepticon",
                        colours: Vec::new(),
                        slots: Vec::new(),
                        physics: Arc::new(PhysicsAsset::default()),
                        sockets: Vec::new(),
                        socket_scales: Vec::new(),
                        guns: [None, None],
                        gun_sockets: Vec::new(),
                        vguns: Vec::new(),
                        vgun_sockets: Vec::new(),
                        clips: Vec::new(),
                        deaths: deaths_of(f[6], form),
                        scale: f.get(13).map_or(1.0, |_| n(13)).max(0.01),
                        npc: false,
                        graph: Handle::default(),
                        nodes: HashMap::new(),
                        ai: None,
                        other: None,
                    });
                }
                "colours" if f.len() >= 4 => {
                    if let Some(k) = kind {
                        let c = |s: &str| {
                            let v = floats(s);
                            (v.len() >= 3).then(|| [v[0] as u8, v[1] as u8, v[2] as u8])
                        };
                        if let (Some(a), Some(b)) = (c(f[2]), c(f[3])) {
                            out.kinds[k].colours.push([a, b]);
                        }
                    }
                }
                "slot" if f.len() >= 4 => {
                    if let Some(k) = kind {
                        let alts = f[3]
                            .split(',')
                            .map(|n| (n != "-").then(|| n.to_string()))
                            .collect();
                        out.kinds[k].slots.push(alts);
                    }
                }
                "body" if f.len() >= 3 => {
                    if let Some(k) = kind {
                        Arc::make_mut(&mut out.kinds[k].physics).read_body(&f[2..]);
                    }
                }
                "mass" if f.len() >= 4 => {
                    if let Some(k) = kind {
                        Arc::make_mut(&mut out.kinds[k].physics).read_mass(&f[2..]);
                    }
                }
                "joint" if f.len() >= 3 => {
                    if let Some(k) = kind {
                        Arc::make_mut(&mut out.kinds[k].physics).read_joint(&f[2..]);
                    }
                }
                "socket" if f.len() >= 5 => {
                    if let Some(k) = kind {
                        let (at, turn) = socket_frame(&f[4..]);
                        out.kinds[k]
                            .sockets
                            .push((f[2].to_string(), f[3].to_string(), at, turn));
                        let scale = f.get(6).map(|v| floats(v)).unwrap_or_default();
                        if scale.len() >= 3 {
                            out.kinds[k]
                                .socket_scales
                                .push((f[2].to_string(), Vec3::new(scale[0], scale[1], scale[2])));
                        }
                    }
                }
                "gun" if f.len() >= 4 => {
                    if let Some(k) = kind {
                        out.kinds[k].guns[usize::from(f[2] == "l")] = Some(f[3].to_string());
                    }
                }
                "gunsocket" if f.len() >= 6 => {
                    if let Some(k) = kind {
                        let (at, turn) = socket_frame(&f[5..]);
                        out.kinds[k].gun_sockets.push((
                            usize::from(f[2] == "l"),
                            f[3].to_string(),
                            f[4].to_string(),
                            at,
                            turn,
                        ));
                    }
                }
                "vgun" if f.len() >= 5 => {
                    if let (Some(k), Ok(n)) = (kind, f[2].parse::<usize>()) {
                        out.kinds[k]
                            .vguns
                            .push((n, f[3].to_string(), f[4].to_string()));
                    }
                }
                "vgunsocket" if f.len() >= 6 => {
                    if let (Some(k), Ok(n)) = (kind, f[2].parse::<usize>()) {
                        let (at, turn) = socket_frame(&f[5..]);
                        out.kinds[k].vgun_sockets.push((
                            n,
                            f[3].to_string(),
                            f[4].to_string(),
                            at,
                            turn,
                        ));
                    }
                }
                "clip" if f.len() >= 7 => {
                    if let Some(k) = kind {
                        out.kinds[k].clips.push(ClipRef {
                            role: f[2].to_string(),
                            file: format!("{MODEL_DIR}/{}", f[3]),
                            index: f[4].trim().parse().unwrap_or(0),
                            secs: f[5].trim().parse().unwrap_or(1.0),
                        });
                    }
                }
                "npc" => {
                    if let Some(k) = kind {
                        npc.insert(k);
                    }
                }
                "other" if f.len() >= 3 => {
                    if let (Some(k), Some(&o)) = (kind, index.get(f[2].trim())) {
                        out.kinds[k].other = Some(o);
                    }
                }
                "npcclip" if f.len() >= 6 => npc_clips.push(ClipRef {
                    role: f[1].to_string(),
                    file: format!("{MODEL_DIR}/{}", f[2]),
                    index: f[3].trim().parse().unwrap_or(0),
                    secs: f[4].trim().parse().unwrap_or(1.0),
                }),
                _ => {}
            }
        }
        // The soldiers' deaths are their skeleton's (each has its own idle).
        for k in npc {
            out.kinds[k].npc = true;
            let own: HashSet<String> = out.kinds[k].clips.iter().map(|c| c.role.clone()).collect();
            let extra: Vec<ClipRef> = npc_clips
                .iter()
                .filter(|c| !own.contains(&c.role))
                .cloned()
                .collect();
            out.kinds[k].clips.extend(extra);
        }
        for (map, ids) in maps {
            out.maps.insert(
                map,
                ids.iter().filter_map(|id| index.get(id).copied()).collect(),
            );
        }
        out
    }

    /// What killing one of type `kind` pays (its score at `difficulty`).
    pub fn money(&self, kind: usize, difficulty: usize) -> f32 {
        let m = self
            .difficulty
            .get(difficulty)
            .or(self.difficulty.last())
            .copied()
            .unwrap_or(1.0);
        self.kinds.get(kind).map_or(0.0, |k| k.score * m)
    }
}

/// An enemy model (or its clips' file), its meshes and pictures kept only where
/// they are drawn (the main world needs none of them: shots strike the bodies).
fn load_model<A: Asset>(
    asset_server: &AssetServer,
    path: impl Into<bevy::asset::AssetPath<'static>>,
) -> Handle<A> {
    use bevy::asset::RenderAssetUsages;
    asset_server.load_with_settings(path, |s: &mut bevy::gltf::GltfLoaderSettings| {
        s.load_meshes = RenderAssetUsages::RENDER_WORLD;
        s.load_materials = RenderAssetUsages::RENDER_WORLD;
    })
}

/// The clips of the types that have come into play, into their graphs (once their
/// files are in): only the map's types are loaded.
fn prepare_kinds(
    mut kinds: ResMut<Kinds>,
    foes: Query<&Foe, Added<Foe>>,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut clips: ResMut<GltfClips>,
    mut masks: ResMut<crate::ai::graph::MaskJobs>,
) {
    let wanted: Vec<usize> = foes
        .iter()
        .flat_map(|foe| {
            let other = kinds.kinds.get(foe.kind).and_then(|k| k.other);
            std::iter::once(foe.kind).chain(other)
        })
        .collect();
    for want in wanted {
        let Some(kind) = kinds.kinds.get_mut(want) else {
            continue;
        };
        if !kind.nodes.is_empty() {
            continue;
        }
        let mut graph = AnimationGraph::new();
        let root = graph.root;
        let mut pending: HashMap<String, Vec<(AnimationNodeIndex, usize)>> = HashMap::new();
        // Its AI's layers first (its anim tree's set, whatever its form: ai/graph.rs),
        // then its own clips.
        let mut masked = Vec::new();
        if let Some(data) = crate::ai::clips::for_type(&kind.id)
            && !data.clips.is_empty()
        {
            let built = crate::ai::graph::build(&mut graph, root, &data);
            for (file, list) in built.pending {
                pending.entry(file).or_default().extend(list);
            }
            masked = built.masked;
            kind.ai = Some(Arc::new(built.nodes));
        }
        let mut nodes: HashMap<String, Vec<(AnimationNodeIndex, f32)>> = HashMap::new();
        for c in &kind.clips {
            let node = gltf_clip(
                &mut graph,
                pending.entry(c.file.clone()).or_default(),
                c.index,
                root,
            );
            nodes
                .entry(c.role.clone())
                .or_default()
                .push((node, c.secs));
        }
        let handle = graphs.add(graph);
        for (file, list) in pending {
            clips.wait(load_model(&asset_server, file), handle.clone(), list);
        }
        for (file, bones, nodes) in masked {
            masks.0.push(crate::ai::graph::MaskJob {
                graph: handle.clone(),
                file: load_model(&asset_server, file),
                bones,
                nodes,
            });
        }
        kind.graph = handle;
        kind.nodes = nodes;
    }
}

// ---------------------------------------------------------------------------
// An enemy
// ---------------------------------------------------------------------------

/// How a death plays out, by the kill's death types (the damage type's
/// DamageDeathTypes, collapsed to what the death sets tell apart).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Death {
    /// None of its own (a Fireblast pellet's, a truck's rocket's): the ragdoll.
    #[default]
    Plain,
    /// Ballistic and RapidShot: the rifles', the X18 Scrapmaker's, the car guns'.
    Gunfire,
    /// Ballistic and SingleShot: the Scatter Blaster's, the pistols', the sniper's.
    SingleShot,
    /// Explosive: rockets, grenades, the launchers' rounds.
    Explosive,
    /// Explosive and Tank: a tank's shell.
    Tank,
    /// Energy: the E.D.K. TechVolt's.
    Energy,
    /// The Path Blaster's Lights Out shockwave (TnDamageTypeShockwaveGun).
    Shockwave,
    /// Slow: the Corrosive Slime Cannon's.
    Slow,
    /// DrainGun: the Energon Harvester's.
    Drain,
    /// Dismember: the Gear Shredder's.
    Dismember,
    Melee,
    /// Fire: burning (TnDamageTypeFireDot).
    Fire,
    /// SelfDestruct (TnDamageTypeSelfDestruct, TnDamageTypeExplodeOnDeath): an
    /// explosive drone's own blast, a drone's lifespan run out.
    SelfDestruct,
}

impl Death {
    /// Every death, in the order their numbers go (`index`).
    const ALL: [Death; 13] = [
        Death::Plain,
        Death::Gunfire,
        Death::SingleShot,
        Death::Explosive,
        Death::Tank,
        Death::Energy,
        Death::Shockwave,
        Death::Slow,
        Death::Drain,
        Death::Dismember,
        Death::Melee,
        Death::Fire,
        Death::SelfDestruct,
    ];

    /// Its number (sent online: a player's death, net.rs).
    pub fn index(self) -> u8 {
        Death::ALL.iter().position(|&d| d == self).unwrap_or(0) as u8
    }

    /// The death numbered `i` (Plain for a number it doesn't know).
    pub fn from_index(i: u8) -> Death {
        Death::ALL.get(i as usize).copied().unwrap_or_default()
    }

    /// A weapon's (WEAPONS' id: its damage type's DamageDeathTypes).
    pub fn of_weapon(id: &str) -> Death {
        match id {
            "AssaultRifle" | "IonBlaster" | "BurstRifle" | "HeavyMG" => Death::Gunfire,
            "Shotgun" | "SniperRifle" | "HeavyPistol" | "TracerDart" => Death::SingleShot,
            "Bazooka" | "HomingRocket" | "GrenadeLauncher" | "ClusterCannon" => Death::Explosive,
            "DrainGun" => Death::Drain,
            "ChainBeam" => Death::Energy,
            "AoESlow" => Death::Slow,
            "SawBlade" => Death::Dismember,
            _ => Death::Plain,
        }
    }

    /// A gun's round (Ballistic: an instant hit's trace, which names the bone it
    /// struck: a headshot, a bone's damage).
    pub fn is_gunshot(self) -> bool {
        matches!(self, Death::Gunfire | Death::SingleShot)
    }

    /// Its damage types' DamageDeathTypes (TransGame's TnDamageType classes': the
    /// rifles' TnDamageTypeAssaultRifle, the Scatter Blaster's TnDamageTypeShotgun,
    /// the Energon Harvester's TnDamageTypeDrainGun, the E.D.K. TechVolt's
    /// TnDamageTypeChainBeam, the Corrosive Slime Cannon's TnDamageTypeSlow, ...).
    pub fn types(self) -> deaths::Types {
        let names: &[&str] = match self {
            Death::Plain => &[],
            Death::Gunfire => &["Ballistic", "RapidShot"],
            Death::SingleShot => &["Ballistic", "SingleShot"],
            Death::Explosive => &["Explosive"],
            Death::Tank => &["Explosive", "Tank"],
            Death::Energy => &["Energy", "RapidShot"],
            Death::Shockwave => &["ShockwaveGun"],
            Death::Slow => &["Explosive", "Slow"],
            Death::Drain => &["Ballistic", "SingleShot", "DrainGun"],
            Death::Dismember => &["Dismember"],
            Death::Melee => &["Melee"],
            Death::Fire => &["Fire"],
            Death::SelfDestruct => &["SelfDestruct"],
        };
        deaths::Types::of_all(names.iter().copied())
    }

    /// The size of a blow's momentum (Unreal's units: TakeDamage's Momentum, the
    /// weapon's InstantHitMomentum or its round's MomentumTransfer) where its
    /// weapon doesn't give its own (Hit::with_momentum): the enemies' guns' and
    /// blasts'.
    pub fn momentum(self) -> f32 {
        match self {
            Death::Gunfire | Death::SingleShot | Death::Melee => 100000.0,
            Death::Explosive | Death::Tank => 200000.0,
            Death::Drain => 200000.0,
            Death::Energy => 75000.0,
            _ => 1.0,
        }
    }
}

/// The momentum of a player's weapon's blow (WEAPONS' id; Unreal's units): its
/// TnWeaponData's InstantHitMomentum (the Escalation data's: AssaultRifleArm_SV's 1,
/// none for the Ion Blaster's, the Photon Burst Rifle's or the Tracer Dart's,
/// HeavyMG_SV's and Shotgun_SV's 100000, HeavyPistol_SV's 150000, DrainGun's
/// 200000, ChainBeam_SV's 75000, the Nucleon Charge Rifle's 100000), or its round's
/// data's MomentumTransfer (Bazooka_SP_PROJDATA's 100000, RocketLauncher_SP's 1,
/// GrenadeLauncher_SP's 200000, ClusterCannon's 250000, SawBlade_SP's 1000; the
/// Corrosive Slime Cannon's has none).
pub fn weapon_momentum(id: &str) -> f32 {
    match id {
        "AssaultRifle" => 1.0,
        "HeavyMG" | "Shotgun" | "SniperRifle" | "Bazooka" => 100000.0,
        "HeavyPistol" => 150000.0,
        "DrainGun" | "GrenadeLauncher" => 200000.0,
        "ChainBeam" => 75000.0,
        "ClusterCannon" => 250000.0,
        "HomingRocket" => 1.0,
        "SawBlade" => 1000.0,
        _ => 0.0,
    }
}

/// The death a damage type's kill dies (its class's DamageDeathTypes: the enemies'
/// guns' TnDamageTypeBurstRifle, TnDamageTypeShotgun, TnDamageTypeSniperRifle...).
pub fn death_of_damage_type(name: &str) -> Death {
    match name.strip_prefix("TnDamageType").unwrap_or(name) {
        "AssaultRifle" | "AssaultRifleSlow" | "BurstRifle" | "CarMachineGun" | "DualHeavyMG"
        | "GatlingGun" | "HeavyIonBlaster" | "HeavyMG" | "IonBlaster" | "MountedIonBlaster"
        | "SentryGun" => Death::Gunfire,
        "Shotgun" | "EmpShotgun" | "SniperRifle" | "HeavyPistol" | "TracerDart" => {
            Death::SingleShot
        }
        "DrainGun" => Death::Drain,
        "TankShell" => Death::Tank,
        "ChainBeam" | "DeathBeam" | "Electricity" | "RepairEnemy" | "Environment" => Death::Energy,
        "Slow" => Death::Slow,
        "SawBlade" | "SawBladeRicochet" | "SawbladeMelee" => Death::Dismember,
        "ShockwaveGun" => Death::Shockwave,
        "SelfDestruct" | "ExplodeOnDeath" => Death::SelfDestruct,
        n if n.contains("Fire") || n == "MarauderFlame" => Death::Fire,
        n if n.contains("Melee") || n == "LeapSmash" || n == "Whirlwind" => Death::Melee,
        "Bazooka"
        | "ClusterBomb"
        | "DeathBlossomRocket"
        | "FlakGrenade"
        | "GrenadeLauncher"
        | "HeavyRocketTurret"
        | "HomingRocket"
        | "HoverGroundPound"
        | "HoverGroundPoundAi"
        | "KamikazeMine"
        | "MartyrGrenade"
        | "MountedRocketTurret"
        | "PlaneRocket"
        | "PlasmaCannon"
        | "RocketTurret"
        | "RollerMine"
        | "SentryRocket"
        | "VehicleExplosive"
        | "EnvironmentExplosion" => Death::Explosive,
        _ => Death::Plain,
    }
}

/// A damage type's GibDistance (metres; TnDamageType's ShouldGib: none for most).
pub fn gib_distance_of(name: &str) -> f32 {
    match name.strip_prefix("TnDamageType").unwrap_or(name) {
        "Shotgun" | "DrainGun" => 15.0,
        "EmpShotgun" => 10.0,
        "HomingRocket" => 7.0,
        "FlakGrenade" | "HoverGroundPound" | "HoverGroundPoundAi" => 12.5,
        _ => 0.0,
    }
}

/// A player's weapon's damage type's GibDistance (metres: TnDamageTypeShotgun's and
/// TnDamageTypeDrainGun's 1500 uu, TnDamageTypeHomingRocket's 700): a kill by it
/// from within that gibs (TnDeathTypeGib). None for the others.
pub fn gib_reach(id: &str) -> f32 {
    match id {
        "Shotgun" | "DrainGun" => 15.0,
        "HomingRocket" => 7.0,
        _ => 0.0,
    }
}

/// How an enemy (or a player) was struck: the death it dies of if it dies of this,
/// where it was struck, the way the blow went, its momentum and whether it gibs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hit {
    pub death: Death,
    pub at: Vec3,
    pub dir: Vec3,
    /// TakeDamage's Momentum (Unreal's units, the demo's axes: over the pawn's
    /// mass, the speed it gives it; its TearOffMomentum if it kills it).
    pub momentum: Vec3,
    /// Its damage type gibs what it kills (TnDamageType.ShouldGib: the instigator
    /// within its GibDistance).
    pub gib: bool,
    /// Its damage type's class, where the blow names one (a trap's, a sentry's, a
    /// support item's...: the kill feed's words, killfeed.rs); empty for one that
    /// goes by its death and its striker's weapon.
    pub damage: &'static str,
}

impl Hit {
    /// A blow of `death` at `at` going along `dir`, its momentum its death's
    /// (Death::momentum).
    pub fn new(death: Death, at: Vec3, dir: Vec3) -> Hit {
        let dir = dir.normalize_or_zero();
        Hit {
            death,
            at,
            dir,
            momentum: dir * death.momentum(),
            gib: false,
            damage: "",
        }
    }

    /// The same of damage type `damage` (its class: killfeed.rs's DAMAGE_TYPES).
    pub fn of_type(mut self, damage: &'static str) -> Hit {
        self.damage = damage;
        self
    }

    /// The same with its weapon's momentum (its size, along the blow).
    pub fn with_momentum(mut self, size: f32) -> Hit {
        self.momentum = self.dir * size;
        self
    }

    /// The same with its momentum `v` (Unreal's units, the demo's axes).
    pub fn with_momentum_vec(mut self, v: Vec3) -> Hit {
        self.momentum = v;
        self
    }

    /// The same gibbing what it kills if its instigator, at `from`, is within its
    /// damage type's GibDistance (`reach`, metres) of what it struck, at `to`.
    pub fn gibs_within(mut self, reach: f32, from: Vec3, to: Vec3) -> Hit {
        self.gib = reach > 0.0 && from.distance(to) <= reach;
        self
    }
}

/// The bones a gunshot on is a headshot (TnHtnAiPawn's HeadShotBoneNames: every
/// HTN enemy's, the soldiers' to the Titan's; a Swarmer's, a Sniper's, the Leaper's
/// and the Guard's own head bones aren't among them), each taking twice the damage
/// (its BoneDamageOverrides, which HmPawn.AdjustDamage puts on a blow by the bone
/// the round's trace struck).
pub const HEAD_BONES: [&str; 2] = ["C_Spine04_Head_XB", "C_Spine05HeadRobo01_XT"];
pub const HEAD_DAMAGE: f32 = 2.0;

/// What a part of an enemy's body is to a blow: its head's (HeadShotBoneNames) or
/// not, what a gunshot there takes (its bone's BoneDamageOverrides modifier, 1
/// for none) and its bone (its index in the body's bones; NO_BONE for none).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub head: bool,
    pub damage: f32,
    pub bone: u16,
}

/// A part without a bone (a body that is only its collision's capsule).
pub const NO_BONE: u16 = u16::MAX;

impl Default for Part {
    fn default() -> Self {
        Part {
            head: false,
            damage: 1.0,
            bone: NO_BONE,
        }
    }
}

/// An enemy's body where its bones are this frame: its physics asset's shapes in
/// the world (each with what its bone is to a blow), its bodies' bones (by the
/// parts' indices), its middle (its TargetingSocket) and a sphere round it all.
#[derive(Clone, Debug, Default)]
pub struct Body {
    shapes: Arc<[(Shape, Part)]>,
    bones: Arc<[Arc<str>]>,
    pub centre: Vec3,
    pub bounds: (Vec3, f32),
}

#[cfg(test)]
impl Body {
    /// A robot's body standing at the origin for tests: a capsule from 0.5 to 2 m
    /// up (0.5 m round) and its head a ball 0.4 m round at 2.9 m (HeadShotBoneNames'
    /// C_Spine04_Head_XB, taking twice a round's damage).
    pub(crate) fn standing_robot() -> Body {
        Body {
            shapes: Arc::from(vec![
                (
                    Shape::Capsule {
                        a: Vec3::new(0.0, 0.5, 0.0),
                        b: Vec3::new(0.0, 2.0, 0.0),
                        r: 0.5,
                    },
                    Part::default(),
                ),
                (
                    Shape::Sphere {
                        c: Vec3::new(0.0, 2.9, 0.0),
                        r: 0.4,
                    },
                    Part {
                        head: true,
                        damage: HEAD_DAMAGE,
                        bone: 0,
                    },
                ),
            ]),
            bones: Arc::from(vec![Arc::from("C_Spine04_Head_XB")]),
            centre: Vec3::new(0.0, 1.5, 0.0),
            bounds: (Vec3::new(0.0, 1.65, 0.0), 1.7),
        }
    }
}

impl Body {
    /// Its shapes where its bones are.
    pub fn shapes(&self) -> impl Iterator<Item = &Shape> {
        self.shapes.iter().map(|(s, _)| s)
    }

    /// Where a ray (unit `dir`) enters it within `max`, and the part it enters.
    pub fn ray_part(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<(f32, Part)> {
        let (c, r) = self.bounds;
        crate::ragdoll::ray_sphere(origin, dir, c, r).filter(|t| *t <= max)?;
        let mut best: Option<(f32, Part)> = None;
        for (s, part) in self.shapes.iter() {
            if let Some(t) = s.ray(origin, dir, max)
                && best.is_none_or(|(b, _)| t < b)
            {
                best = Some((t, *part));
            }
        }
        best
    }

    pub fn ray(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        self.ray_part(origin, dir, max).map(|(t, _)| t)
    }

    /// The same for a round with a size of its own (`by` all round).
    pub fn ray_wide(&self, origin: Vec3, dir: Vec3, max: f32, by: f32) -> Option<f32> {
        let (c, r) = self.bounds;
        crate::ragdoll::ray_sphere(origin, dir, c, r + by).filter(|t| *t <= max)?;
        self.shapes
            .iter()
            .filter_map(|(s, _)| s.grown(by).ray(origin, dir, max))
            .reduce(f32::min)
    }

    /// How far `p` is from it (0 inside).
    pub fn reach(&self, p: Vec3) -> f32 {
        if self.shapes.is_empty() {
            return (p.distance(self.bounds.0) - self.bounds.1).max(0.0);
        }
        self.shapes
            .iter()
            .map(|(s, _)| s.reach(p))
            .fold(f32::MAX, f32::min)
    }

    /// The part of it nearest `p` (where a round struck it).
    pub fn part_at(&self, p: Vec3) -> Option<Part> {
        self.shapes
            .iter()
            .map(|(s, part)| (s.reach(p), *part))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, part)| part)
    }

    /// The bone of a part (none for a part without one).
    pub fn bone(&self, part: Part) -> Option<Arc<str>> {
        self.bones.get(part.bone as usize).cloned()
    }

    /// Whether `p` is nearest its head.
    pub fn head_at(&self, p: Vec3) -> bool {
        self.part_at(p).is_some_and(|part| part.head)
    }
}

/// An enemy as rounds and blasts find it this frame: its entity, its middle and its
/// body.
#[derive(Clone, Debug)]
pub struct Target {
    pub entity: Entity,
    pub at: Vec3,
    pub body: Body,
    pub robot: bool,
}

/// An enemy that lives.
#[derive(Component)]
pub struct Enemy {
    /// Its type (Kinds), and whether it is a robot (its type's form: a jet, a drone
    /// or a swarmer isn't, which a homing weapon that can't lock on to robots
    /// locks on to).
    pub kind: usize,
    pub robot: bool,
    pub health: f32,
    pub max_health: f32,
    /// What it takes of a blow (its type's DamageMultiplier).
    pub damage_scale: f32,
    /// What its AI makes of blows (src/ai's pawn's): its own damage multiplier
    /// (TnPawn._DamageMultiplier: the Guard's form's), the bones a gunshot on is a
    /// headshot (HeadShotBoneNames) and what a gunshot on a bone takes
    /// (BoneDamageOverrides). Every type's start as TnHtnAiPawn's do.
    pub ai_damage_scale: f32,
    pub head_bones: Vec<Arc<str>>,
    pub bone_damage: Vec<(Arc<str>, f32)>,
    /// What its AI's buffs make of a blow from behind it (any blow's, and a melee
    /// blow's on top: the Leaper's), and its facing (the demo's axes, along the
    /// ground) that tells behind.
    pub from_behind: (f32, f32),
    pub facing: Vec3,
    /// Seconds left knocked out by an EMP grenade (emp.rs): the game's SetDormant
    /// (it would neither fire nor turn), and the blast's sparks play on it.
    pub emp_left: f32,
    pub emp_fx: Option<crate::levelfx::FxId>,
    /// Its mark from the Orbital Pistol (orbital.rs): it takes more damage while
    /// marked, and shows on the players' HUDs.
    pub mark: orbital::Mark,
    /// Its fire from a Fireblast (specials.rs): it burns for a while, and goes off
    /// when it dies.
    pub burn: specials::Burn,
    /// Whose EMP grenade knocked it out (while emp_left runs), and who last hurt it
    /// and how (its killer, when it dies): the experience awards (xp.rs).
    pub emp_by: Option<Entity>,
    pub hurt: xp::Hurt,
    /// How it was last struck (its death plays out by it), and what each Jazz took
    /// from it (the energon's split).
    pub last_hit: Hit,
    pub damage_by: Vec<(Entity, f32)>,
    /// Who struck its last blow, if a Jazz did, and how (the kill feed's killer:
    /// killfeed.rs).
    pub last_by: Option<Entity>,
    pub last_blow: xp::Blow,
    /// Its body where its bones are (update_bodies).
    pub body: Body,
    /// The blows it took since its AI last looked (ai.rs: its controller's
    /// NotifyTakeHit).
    pub hits: Vec<TakenHit>,
    /// Its pawn as its AI last moved it (what its death is made of).
    pub motion: Motion,
}

/// An enemy's pawn as its AI last moved it (ai/mod.rs's sync_blows), what its death
/// is made of (TnDeathContextFactory: its Velocity; its form's state): its Location
/// (its collision's middle, the demo's space) and Velocity (metres a second), its
/// Mass (TakeDamage's momentum over it is its death's TearOffMomentum), whether it
/// is in the air (PHYS_Falling: get_IsAirborne), its plane's boost (a plane's: its
/// vehicle form's get_IsBoosting) and its up axis (a Sniper's on a ceiling).
#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub at: Option<Vec3>,
    pub velocity: Vec3,
    pub mass: f32,
    pub airborne: bool,
    pub boosting: Option<bool>,
    pub up: Vec3,
}

impl Default for Motion {
    fn default() -> Self {
        Motion {
            at: None,
            velocity: Vec3::ZERO,
            // (Pawn's Mass.)
            mass: 100.0,
            airborne: false,
            boosting: None,
            up: Vec3::Y,
        }
    }
}

/// A blow an enemy took: what it took of it, from whom, how and where, and the
/// part of its body a round struck (a gunshot's trace names its bone; a blast's,
/// a blow's or a box's sweep none: the pawn's collision).
#[derive(Clone, Copy, Debug)]
pub struct TakenHit {
    pub amount: f32,
    pub by: Option<Entity>,
    pub blow: xp::Blow,
    pub hit: Hit,
    pub part: Option<Part>,
}

impl Enemy {
    /// Takes `damage` from `by` (a Jazz, if anyone's), times its damage multipliers
    /// (a gunshot's bone's too: twice on its head) and more if it is marked (the
    /// Orbital Pistol's: orbital.rs), struck as `blow` says and as `hit` (its death
    /// goes by the last); the damage it took.
    pub fn take(&mut self, damage: f32, by: Option<Entity>, blow: xp::Blow, hit: Hit) -> f32 {
        if self.health <= 0.0 || damage <= 0.0 {
            return 0.0;
        }
        // (The Jazz's own buffs: his hover's, Battle Medic's; abilities.rs.)
        let damage = damage * crate::abilities::dealt_scale(by);
        // HmPawn.AdjustDamage: the bone a round's trace struck (a blast or a blow
        // names none).
        let part = if hit.death.is_gunshot() {
            self.body.part_at(hit.at)
        } else {
            None
        };
        let bone = part.map_or(1.0, |p| p.damage);
        // Its buffs' blows from behind (TnBuffConditionDamagedFromActorBehind: the
        // blow come from its back half; TnBuffConditionDamagedFromBehind, a melee
        // blow's: struck by someone facing its way, as one does who strikes it
        // from behind), times each other (TnValueTimesProductBuffModifier).
        let behind = if self.from_behind != (1.0, 1.0)
            && hit.dir.x * self.facing.x + hit.dir.z * self.facing.z > 0.0
        {
            let melee = matches!(blow, xp::Blow::Melee) || hit.death == Death::Melee;
            self.from_behind.0 * if melee { self.from_behind.1 } else { 1.0 }
        } else {
            1.0
        };
        let dealt = (damage
            * bone
            * self.damage_scale
            * self.ai_damage_scale
            * behind
            * self.mark.damage_scale())
        .min(self.health);
        self.health -= dealt;
        self.hurt.by(by, blow);
        self.last_hit = hit;
        self.last_by = by;
        self.last_blow = blow;
        if self.hits.len() < 64 {
            self.hits.push(TakenHit {
                amount: dealt,
                by,
                blow,
                hit,
                part,
            });
        }
        if let Some(j) = by {
            match self.damage_by.iter_mut().find(|(e, _)| *e == j) {
                Some((_, d)) => *d += dealt,
                None => self.damage_by.push((j, dealt)),
            }
        }
        dealt
    }

    pub fn alive(&self) -> bool {
        self.health > 0.0
    }

    /// Whether its last blow was a headshot (WasLastHitHeadshot: a round of a gun
    /// struck its head).
    pub fn headshot(&self) -> bool {
        self.last_hit.death.is_gunshot() && self.body.head_at(self.last_hit.at)
    }

    /// Its middle (its TargetingSocket).
    pub fn centre(&self) -> Vec3 {
        self.body.centre
    }

    /// The radius of a sphere round its body.
    pub fn radius(&self) -> f32 {
        self.body.bounds.1
    }

    /// The top of it (over its middle).
    pub fn top(&self) -> Vec3 {
        let (c, r) = self.body.bounds;
        Vec3::new(self.body.centre.x, c.y + r * 0.8, self.body.centre.z)
    }

    /// Where a shot (unit `dir`) strikes it within `max` (none once it is dead).
    pub fn ray(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        self.alive()
            .then(|| self.body.ray(origin, dir, max))
            .flatten()
    }

    /// How far a point is from its body (0 inside).
    pub fn reach(&self, p: Vec3) -> f32 {
        self.body.reach(p)
    }

    /// It as rounds find it.
    pub fn target(&self, entity: Entity) -> Target {
        Target {
            entity,
            at: self.centre(),
            body: self.body.clone(),
            robot: self.robot,
        }
    }
}

/// Every enemy, living or dying: its type, where it stands, and its spawn's seed
/// (its look's).
#[derive(Component, Clone, Copy)]
pub struct Foe {
    pub kind: usize,
    pub spot: Vec3,
    pub seed: u32,
}

/// An enemy's model, once it is in: its root node (its skeleton's base), its
/// animation player, its bodies' shapes on their bones, its middle's socket and
/// its meshes.
#[derive(Component)]
pub struct Model {
    animator: Option<Entity>,
    shapes: Vec<(Entity, Shape, u16)>,
    /// Each shape's bone (by the index beside the shape), for what a blow there is.
    shape_bones: Arc<[Arc<str>]>,
    socket: Option<(Entity, Vec3)>,
    meshes: Vec<Entity>,
    guns: [Option<Entity>; 2],
    /// Its gun hangs on both arms (Kind::guns_hung_on_both_arms: the Guardian's arm
    /// cannons): both show while it has it out, whichever hand it aims with.
    guns_together: bool,
    /// Its vehicle gun's meshes and that gun's class (Kind.vguns).
    vguns: Vec<(Entity, Arc<str>)>,
    ai_sockets: Vec<ModelSocket>,
}

/// A socket its AI reads (Mesh.GetSocketWorldLocationAndRotation): its name, the
/// hand whose gun it is on (none for the skeleton's own), its bone and its place
/// and turn on the bone. (A bone the AI reads is one with its own name, at the
/// bone.)
pub struct ModelSocket {
    pub name: Arc<str>,
    pub hand: Option<usize>,
    pub bone: Entity,
    pub at: Vec3,
    pub turn: Quat,
}

/// The bones the AI reads (as sockets of their own names): the view bones its
/// pawn's _ViewBoneName names (where it sees from, and the way it looks): the
/// Guard's (TnGuardForms' InvestigateViewBoneName and BattleViewBoneName), the
/// Leaper's (its blueprint's).
const AI_BONES: [&str; 3] = [
    "C_AlertHead01Robo01_XT",
    "C_BackpackRobo01_XT",
    "C_Lumbar02Robo04_XT",
];

impl Model {
    /// Its animation player's entity.
    pub fn animator(&self) -> Option<Entity> {
        self.animator
    }

    /// Its gun's meshes (right hand, left hand).
    pub fn guns(&self) -> [Option<Entity>; 2] {
        self.guns
    }

    /// Both its gun's meshes show together (the Guardian's two arm cannons).
    pub fn guns_together(&self) -> bool {
        self.guns_together
    }

    /// Its vehicle gun's meshes, each with that gun's class.
    pub fn vguns(&self) -> &[(Entity, Arc<str>)] {
        &self.vguns
    }

    /// The sockets its AI reads: its skeleton's, its guns' and its bones'.
    pub fn ai_sockets(&self) -> &[ModelSocket] {
        &self.ai_sockets
    }

    /// Its skeleton's socket `name` where its bone is (the demo's space): its
    /// place and its X and Y axes (Unreal's: glTF's x and z).
    pub fn socket_world(
        &self,
        name: &str,
        globals: &Query<&GlobalTransform>,
    ) -> Option<(Vec3, Vec3, Vec3)> {
        let s = self
            .ai_sockets
            .iter()
            .find(|s| s.hand.is_none() && &*s.name == name)?;
        let bone = globals.get(s.bone).ok()?;
        let turn = bone.rotation() * s.turn;
        Some((bone.transform_point(s.at), turn * Vec3::X, turn * Vec3::Z))
    }
}

/// A transforming enemy's other form's model (its type's other's: Kind.other): a
/// child of the enemy, its scene that form's.
#[derive(Component)]
pub struct FormModel {
    pub owner: Entity,
    pub kind: usize,
}

/// A transforming enemy's forms (its AI's: ai/forms.rs, as it sets them here each
/// frame). Its models by index: 0 its type's own (the enemy's scene), 1 its other
/// form's (`other`, its FormModel). The enemy's Model (and rig) is the form it is
/// in's (`active`); the other's waits in `parked`, the two swapped as it takes the
/// other form (`want`), its type then the form's (`kinds`). Each model hidden or
/// shown (`hidden`); the other form's placed on the enemy (`local`).
#[derive(Component)]
pub struct Forms {
    pub other: Entity,
    pub kinds: [usize; 2],
    pub active: usize,
    pub want: usize,
    pub hidden: [bool; 2],
    pub local: Transform,
    pub parked: Option<Model>,
    pub parked_rig: Option<Rig>,
    /// Its own scene's top nodes (hidden to hide its own model).
    pub tops: Vec<Entity>,
}

/// What an other form's look is picked by: its own seed (its type's colour pair is
/// the enemy's own model's).
const OTHER_FORM_SEED: u32 = 0x0F0E_A17E;

/// Cheap repeatable pseudo-random numbers (each spawn's look is the same on every PC).
pub fn rand(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    ((*seed >> 8) & 0xFFFF) as f32 / 65536.0
}

fn srgb_to_linear(c: [u8; 3]) -> Vec3 {
    let l = |x: u8| {
        let v = x as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    Vec3::new(l(c[0]), l(c[1]), l(c[2]))
}

/// Spawns an enemy of `kind` at `spot` (its feet), turned to `yaw` (radians about
/// the up axis), its look from `seed`, what the players' blows do to it scaled by
/// `damage` as well as its own multiplier (its wave's TnBuffSurvivalWavePlayerDamageDone:
/// waves.rs): its entity (none for a kind there isn't).
pub fn spawn(
    commands: &mut Commands,
    asset_server: &AssetServer,
    kinds: &Kinds,
    kind: usize,
    (spot, yaw): (Vec3, f32),
    seed: u32,
    damage: f32,
) -> Option<Entity> {
    let k = kinds.kinds.get(kind)?;
    let half = k.half_height.max(0.5);
    let centre = spot + Vec3::Y * half;
    let body = Body {
        shapes: Arc::from(vec![(
            Shape::Capsule {
                a: spot + Vec3::Y * k.radius.min(half),
                b: spot + Vec3::Y * (2.0 * half - k.radius.min(half)),
                r: k.radius,
            },
            Part::default(),
        )]),
        bones: Arc::from(Vec::new()),
        centre,
        bounds: (centre, half.max(k.radius)),
    };
    let entity = commands.spawn((
        Enemy {
            kind,
            robot: k.form == Form::Robot,
            health: k.health,
            max_health: k.health,
            damage_scale: damage
                * if k.damage_scale > 0.0 {
                    k.damage_scale
                } else {
                    1.0
                },
            ai_damage_scale: 1.0,
            head_bones: HEAD_BONES.iter().map(|b| Arc::from(*b)).collect(),
            bone_damage: HEAD_BONES
                .iter()
                .map(|b| (Arc::from(*b), HEAD_DAMAGE))
                .collect(),
            from_behind: (1.0, 1.0),
            facing: Vec3::NEG_Z,
            emp_left: 0.0,
            emp_fx: None,
            mark: orbital::Mark::default(),
            burn: specials::Burn::default(),
            emp_by: None,
            hurt: xp::Hurt::default(),
            last_hit: Hit::default(),
            last_by: None,
            last_blow: xp::Blow::Weapon,
            damage_by: Vec::new(),
            body,
            hits: Vec::new(),
            motion: Motion::default(),
        },
        Foe { kind, spot, seed },
        // (Its scene from the file its clips come from: gltffiles.rs.)
        crate::gltffiles::SceneOf {
            file: load_model(asset_server, k.model.clone()),
            scene: 0,
        },
        // (Its own clips: not a Jazz's robot graph, attach_robot_animator.)
        crate::OwnAnimation,
        Transform::from_translation(spot).with_rotation(Quat::from_rotation_y(yaw)),
        Visibility::default(),
        Name::new(format!("Enemy {}", k.id)),
    ));
    let entity = entity.id();
    // A type that changes form brings its other form's model along (hidden: its AI
    // shows the form it is in).
    if let Some((o, ok)) = k.other.and_then(|o| kinds.kinds.get(o).map(|ok| (o, ok))) {
        let other = commands
            .spawn((
                FormModel {
                    owner: entity,
                    kind: o,
                },
                crate::gltffiles::SceneOf {
                    file: load_model(asset_server, ok.model.clone()),
                    scene: 0,
                },
                crate::OwnAnimation,
                Transform::IDENTITY,
                Visibility::Hidden,
                Name::new(format!("Enemy {} (other form)", k.id)),
                ChildOf(entity),
            ))
            .id();
        commands.entity(entity).insert(Forms {
            other,
            kinds: [kind, o],
            active: 0,
            want: 0,
            hidden: [false, true],
            local: Transform::IDENTITY,
            parked: None,
            parked_rig: None,
            tops: Vec::new(),
        });
    }
    Some(entity)
}

/// The entities of a model under `root`: its descendants, not another form's model
/// under it (a FormModel's).
fn model_entities(
    root: Entity,
    children: &Query<&Children>,
    form_models: &Query<&FormModel>,
) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut stack: Vec<Entity> = children.get(root).map(|c| c.to_vec()).unwrap_or_default();
    stack.reverse();
    while let Some(e) = stack.pop() {
        if form_models.contains(e) {
            continue;
        }
        out.push(e);
        if let Ok(c) = children.get(e) {
            stack.extend(c.iter().rev());
        }
    }
    out
}

/// A type's colour pair for a spawn's `seed` (picked after its slots' parts, as
/// its model is dressed).
fn colour_pair(k: &Kind, seed: u32) -> [[u8; 3]; 2] {
    let mut seed = seed;
    for _ in &k.slots {
        rand(&mut seed);
    }
    if k.colours.is_empty() {
        [[255, 255, 255], [255, 255, 255]]
    } else {
        k.colours[((rand(&mut seed) * k.colours.len() as f32) as usize).min(k.colours.len() - 1)]
    }
}

/// An enemy's scene as it comes in: one part in each slot (or none) and the rest
/// hidden, its painted parts in its colour pair, its idle playing, its bodies found
/// on its bones and its ragdoll rig made.
#[allow(clippy::too_many_arguments)]
fn dress(
    ready: On<SceneInstanceReady>,
    mut commands: Commands,
    kinds: Res<Kinds>,
    foes: Query<&Foe>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut players: Query<&mut AnimationPlayer>,
    meshes: Query<(
        &MeshMaterial3d<StandardMaterial>,
        Option<&GltfMaterialExtras>,
    )>,
    skins: Query<&SkinnedMesh>,
    standard: Res<Assets<StandardMaterial>>,
    mut paints: ResMut<Assets<PaintMaterial>>,
    mut cache: ResMut<PaintCache>,
    (form_models, mut forms): (Query<&FormModel>, Query<&mut Forms>),
) {
    let root = ready.entity;
    // The enemy's own model, or its other form's (a transforming type's).
    let (foe, kind, owner) = if let Ok(foe) = foes.get(root) {
        (*foe, foe.kind, root)
    } else if let Ok(fm) = form_models.get(root)
        && let Ok(foe) = foes.get(fm.owner)
    {
        (*foe, fm.kind, fm.owner)
    } else {
        return;
    };
    let Some(k) = kinds.kinds.get(kind) else {
        return;
    };
    let other_form = owner != root;
    let mut seed = if other_form {
        foe.seed ^ OTHER_FORM_SEED
    } else {
        foe.seed
    };
    // (Its own entities: not its other form's model under it.)
    let mine = model_entities(root, &children, &form_models);
    let mut named: HashMap<String, Entity> = HashMap::new();
    for &e in &mine {
        if let Ok(n) = names.get(e) {
            named.entry(n.as_str().to_string()).or_insert(e);
        }
    }
    // One part in each slot.
    for slot in &k.slots {
        let pick =
            ((rand(&mut seed) * slot.len() as f32) as usize).min(slot.len().saturating_sub(1));
        for (i, alt) in slot.iter().enumerate() {
            if i != pick
                && let Some(e) = alt.as_ref().and_then(|n| named.get(n))
            {
                commands.entity(*e).insert(Visibility::Hidden);
            }
        }
    }
    // Its paint: its type's pair (an other form's, the enemy's own model's).
    let pair = colour_pair(kinds.kinds.get(foe.kind).unwrap_or(k), foe.seed);
    let colours = (srgb_to_linear(pair[0]), srgb_to_linear(pair[1]), k.energon);
    let mut mesh_list = Vec::new();
    let mut joints: HashSet<Entity> = HashSet::new();
    for &e in &mine {
        // (Its skeleton's bones: not a gun's own, <gun>:<bone>, which rides its
        // socket on them: the Guard's arm cannon.)
        if let Ok(skin) = skins.get(e) {
            joints.extend(
                skin.joints
                    .iter()
                    .copied()
                    .filter(|j| names.get(*j).is_ok_and(|n| !n.as_str().contains(':'))),
            );
        }
        let Ok((m, extras)) = meshes.get(e) else {
            continue;
        };
        mesh_list.push(e);
        if is_paint(extras.map(|x| x.value.as_str()))
            && let Some(p) = cache.colored(&m.0, colours, true, &standard, &mut paints)
        {
            commands
                .entity(e)
                .remove::<MeshMaterial3d<StandardMaterial>>()
                .insert(MeshMaterial3d(p));
        }
    }
    // Its idle.
    let animator = mine.iter().copied().find(|e| players.contains(*e));
    if let Some(a) = animator
        && let Ok(mut player) = players.get_mut(a)
    {
        commands
            .entity(a)
            .insert(AnimationGraphHandle(k.graph.clone()));
        player.stop_all();
        if let Some(&(idle, _)) = k.nodes.get("idle").and_then(|n| n.first()) {
            player.play(idle).repeat();
        }
    }
    // Its bodies on their bones, and its middle.
    let mut shapes = Vec::new();
    let mut shape_bones: Vec<Arc<str>> = Vec::new();
    for b in &k.physics.bodies {
        if let Some(&bone) = named.get(&b.bone) {
            let i = shape_bones.len() as u16;
            shape_bones.push(Arc::from(b.bone.as_str()));
            shapes.extend(b.shapes.iter().map(|s| (bone, s.clone(), i)));
        }
    }
    let socket = ["TargetingSocket", "Chest_FX", "PositionSocket"]
        .iter()
        .find_map(|want| k.sockets.iter().find(|(n, ..)| n == want))
        .and_then(|(_, bone, at, _)| named.get(bone).map(|e| (*e, *at)));
    // Its gun: the right hand's (the left's when its AI holds it in the other).
    let guns = k
        .guns
        .clone()
        .map(|n| n.and_then(|n| named.get(&n).copied()));
    if let Some(left) = guns[1] {
        commands.entity(left).insert(Visibility::Hidden);
    }
    // Its vehicle gun, hidden till its AI has it out.
    let vguns: Vec<(Entity, Arc<str>)> = k
        .vguns
        .iter()
        .filter_map(|(_, node, class)| {
            named
                .get(node)
                .map(|e| (*e, Arc::<str>::from(class.as_str())))
        })
        .collect();
    for (e, _) in &vguns {
        commands.entity(*e).insert(Visibility::Hidden);
    }
    let mut ai_sockets: Vec<ModelSocket> = k
        .sockets
        .iter()
        .filter_map(|(n, bone, at, turn)| {
            named.get(bone).map(|b| ModelSocket {
                name: Arc::from(n.as_str()),
                hand: None,
                bone: *b,
                at: *at,
                turn: *turn,
            })
        })
        .collect();
    ai_sockets.extend(
        k.gun_sockets
            .iter()
            .filter_map(|(hand, n, bone, at, turn)| {
                named.get(bone).map(|b| ModelSocket {
                    name: Arc::from(n.as_str()),
                    hand: Some(*hand),
                    bone: *b,
                    at: *at,
                    turn: *turn,
                })
            }),
    );
    // (Its vehicle gun's, as vgun<n>:<name>.)
    ai_sockets.extend(
        k.vgun_sockets
            .iter()
            .filter_map(|(n, name, bone, at, turn)| {
                named.get(bone).map(|b| ModelSocket {
                    name: Arc::from(format!("vgun{n}:{name}").as_str()),
                    hand: None,
                    bone: *b,
                    at: *at,
                    turn: *turn,
                })
            }),
    );
    ai_sockets.extend(AI_BONES.iter().filter_map(|n| {
        named.get(*n).map(|b| ModelSocket {
            name: Arc::from(*n),
            hand: None,
            bone: *b,
            at: Vec3::ZERO,
            turn: Quat::IDENTITY,
        })
    }));
    // Its bones, which a sticky round stuck in it rides (riot.rs: the nearest to
    // where it struck, as on a Jazz).
    for &j in &joints {
        commands.entity(j).try_insert(crate::JointOf(owner));
    }
    // Its skeleton's rig, from its model's root.
    let rig = named.get("Root").map(|&base| {
        Rig::new(k.physics.clone(), base, &children, &names, |e| {
            joints.contains(&e)
        })
    });
    let model = Model {
        animator,
        shapes,
        shape_bones: Arc::from(shape_bones),
        socket,
        meshes: mesh_list,
        guns,
        guns_together: k.guns_hung_on_both_arms(),
        vguns,
        ai_sockets,
    };
    // (An other form's, parked till its enemy takes that form: sync_forms.)
    if other_form {
        if let Ok(mut f) = forms.get_mut(owner) {
            f.parked = Some(model);
            f.parked_rig = rig;
        }
        return;
    }
    if let Some(rig) = rig {
        commands.entity(root).insert(rig);
    }
    commands.entity(root).insert(model);
    // (A transforming enemy's own scene's nodes, which its forms hide or show.)
    if let Ok(mut f) = forms.get_mut(root) {
        f.tops = children
            .get(root)
            .map(|c| c.iter().filter(|e| !form_models.contains(*e)).collect())
            .unwrap_or_default();
    }
}

/// A transforming enemy's forms as its AI sets them (Forms): the model of the form
/// it is in the enemy's (its Model and rig swapped with the parked ones, its type
/// and what it takes of blows the form's), each form's model hidden or shown (its
/// own scene's nodes, its other form's model), its other form's placed.
#[allow(clippy::type_complexity)]
pub fn sync_forms(
    mut commands: Commands,
    kinds: Res<Kinds>,
    mut enemies: Query<(
        Entity,
        &mut Forms,
        Option<&mut Model>,
        Option<&mut Rig>,
        Option<&mut Enemy>,
    )>,
    mut shown: Query<&mut Visibility>,
    mut places: Query<&mut Transform, With<FormModel>>,
) {
    for (e, mut forms, model, rig, enemy) in &mut enemies {
        let forms = &mut *forms;
        if forms.want != forms.active
            && let Some(mut model) = model
            && let Some(parked) = forms.parked.as_mut()
        {
            std::mem::swap(&mut *model, parked);
            match (rig, forms.parked_rig.as_mut()) {
                (Some(mut r), Some(p)) => std::mem::swap(&mut *r, p),
                (None, Some(_)) => {
                    if let Some(p) = forms.parked_rig.take() {
                        commands.entity(e).insert(p);
                    }
                }
                _ => {}
            }
            let (was, now) = (forms.kinds[forms.active], forms.kinds[forms.want]);
            forms.active = forms.want;
            if let Some(mut enemy) = enemy
                && let (Some(was), Some(k)) = (kinds.kinds.get(was), kinds.kinds.get(now))
            {
                let scale = |k: &Kind| {
                    if k.damage_scale > 0.0 {
                        k.damage_scale
                    } else {
                        1.0
                    }
                };
                enemy.kind = now;
                enemy.robot = k.form == Form::Robot;
                enemy.damage_scale *= scale(k) / scale(was);
            }
        }
        let vis = |hide: bool| {
            if hide {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            }
        };
        let other = forms.other;
        for (c, hide) in forms
            .tops
            .iter()
            .map(|&c| (c, forms.hidden[0]))
            .chain([(other, forms.hidden[1])])
        {
            if let Ok(mut v) = shown.get_mut(c)
                && *v != vis(hide)
            {
                *v = vis(hide);
            }
        }
        if let Ok(mut t) = places.get_mut(forms.other)
            && *t != forms.local
        {
            *t = forms.local;
        }
    }
}

/// Each living enemy's body where its bones are now (after the transforms are
/// propagated, for the next frame's shots).
fn update_bodies(mut enemies: Query<(&mut Enemy, &Model)>, globals: Query<&GlobalTransform>) {
    for (mut enemy, model) in &mut enemies {
        if model.shapes.is_empty() {
            continue;
        }
        // What a blow on each bone is now (its AI changes them: the Guard's forms).
        let parts: Vec<Part> = model
            .shape_bones
            .iter()
            .enumerate()
            .map(|(i, b)| Part {
                head: enemy.head_bones.iter().any(|h| h == b),
                damage: enemy
                    .bone_damage
                    .iter()
                    .find(|(n, _)| n == b)
                    .map_or(1.0, |(_, d)| *d),
                bone: i as u16,
            })
            .collect();
        let mut list = Vec::with_capacity(model.shapes.len());
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (bone, shape, i) in &model.shapes {
            let Ok(g) = globals.get(*bone) else {
                continue;
            };
            let placed = shape.placed(&g.affine());
            let (c, r) = placed.bounds();
            lo = lo.min(c - Vec3::splat(r));
            hi = hi.max(c + Vec3::splat(r));
            list.push((placed, parts.get(*i as usize).copied().unwrap_or_default()));
        }
        if list.is_empty() {
            continue;
        }
        let mid = (lo + hi) * 0.5;
        let centre = model
            .socket
            .and_then(|(bone, at)| globals.get(bone).ok().map(|g| g.transform_point(at)))
            .unwrap_or(mid);
        enemy.body = Body {
            shapes: Arc::from(list),
            bones: model.shape_bones.clone(),
            centre,
            bounds: (mid, (hi - lo).length() * 0.5),
        };
    }
}

// ---------------------------------------------------------------------------
// Death
// ---------------------------------------------------------------------------

impl DeathSet {
    /// Its name in the death data (tools/build_death_sets.py's sets, by their short
    /// names).
    pub fn name(self) -> &'static str {
        match self {
            DeathSet::Basic => "Basic_DEATHSET",
            DeathSet::Titan => "Titan_DEATHSET",
            DeathSet::Guardian => "Guardian_DEATHSET",
            DeathSet::Leaper => "Leaper_DEATHSET",
            DeathSet::Swarmer => "Swarmer_DEATHSET",
            DeathSet::Sniper => "Sniper_DEATHSET",
            DeathSet::Drone => "TitanExplosiveDrone_DEATHSET",
            DeathSet::Flyer => "Flyer_DEATHSET",
        }
    }
}

/// TnDeathContextFactory's StationarySpeed (uu a second): a pawn moving no faster
/// dies Stationary.
const STATIONARY_SPEED: f32 = 50.0;

/// An enemy that has died, as its death plays out (its death set's death:
/// deaths.rs's Timeline), and what its beats have left on it: the materials it
/// dissolves in, its light, the effects held on its sockets and bones, the bones
/// its death scales.
#[derive(Component)]
pub struct Dying {
    /// Its type as it died (the form it was in).
    kind: usize,
    timeline: deaths::Timeline,
    place: deaths::Place,
    /// The bone the kill's round struck, if its trace named one (its ragdoll's
    /// impulses on the bone struck).
    struck: Option<Arc<str>>,
    /// What its choices are made from: its clip among those of its clip's name (a
    /// chooser group's: one at random), its kicks.
    pick: u32,
    seed: u32,
    /// Its ragdoll has taken over (the first kick carries the pawn's speed).
    fell: bool,
    dissolving: Vec<Dissolving>,
    /// A sphere round its body as it died (its dissolve's reach).
    reach: f32,
    light: Option<Entity>,
    /// The effects held on it: each its socket's (or bone's) name.
    effects: Vec<(crate::levelfx::FxId, Arc<str>)>,
    /// Bones its death scales (TnDeathModifierControlBone), held so after the clips.
    bones: Vec<(Entity, f32)>,
}

/// A dying enemy's or player's death light (its pawn's TnRampLightManager's one
/// light).
#[derive(Component)]
pub struct DeathLight;

/// The meshes' materials: painted, or plain.
pub type MeshMaterials<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static MeshMaterial3d<PaintMaterial>>,
        Option<&'static MeshMaterial3d<StandardMaterial>>,
    ),
>;

/// A mesh dissolving: its dissolve's own material, and the one it had.
pub struct Dissolving {
    mesh: Entity,
    paint: Handle<PaintMaterial>,
    had: Had,
}

/// A mesh's material before its dissolve.
enum Had {
    Paint(Handle<PaintMaterial>),
    Plain(Handle<StandardMaterial>),
}

/// A dissolve's overlay's look: TR_DefragOverlay_AB_Mat's parameters in its
/// material instance (paint.wgsl's OverlayLook; tools/build_cloak.py checks them
/// against the game's): its rim's colour, intensity and exponent (Fresnel Color,
/// Intensity, Exponent), its front's colour and intensity (Defrag Lead Color, Lead
/// Intensity), the glow round its middle (Core Intensity, Exponent), its burn-out
/// colour (Burn Out Color), its lines' intensity and contrast (Energon Intensity,
/// Contrast), its noise's multiplier (Energon Fractal Alpha Multiplier) and whether
/// that noise is ElectricArcs (its Energon Fractal Alpha), not fractalMaps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DefragLook {
    pub fresnel: Vec3,
    pub fresnel_intensity: f32,
    pub fresnel_exponent: f32,
    pub lead: Vec3,
    pub lead_intensity: f32,
    pub core_intensity: f32,
    pub core_exponent: f32,
    pub burn: Vec3,
    pub energon_intensity: f32,
    pub energon_contrast: f32,
    pub noise_scale: f32,
    pub arcs: bool,
}

/// The game's dissolve overlays (the death sets' Deressers' OverlayMaterial): the
/// cyan TR_DefragOverlay_MATINST nearly every death dissolves in, the electrocutions'
/// pale blue, the corrosion's and slowing deaths' brown, the fires' red and the Retro
/// pistol's purple (its TR_DefragOverlayColorStrobe_Mat strobes between two more
/// colours as well, which the demo leaves out: no Escalation weapon kills with it).
/// What an instance doesn't set is TR_DefragOverlay_AB_Mat's own: Lead Intensity 4,
/// Core Intensity 7 and Exponent 32, the noise multiplier 1.
pub const DEFRAG_LOOKS: [(&str, DefragLook); 5] = [
    (
        "TR_DefragOverlay_MATINST",
        DefragLook {
            fresnel: Vec3::new(0.0, 0.8393, 1.0),
            fresnel_intensity: 7.0,
            fresnel_exponent: 3.913726,
            lead: Vec3::new(0.0, 0.129608, 0.864130),
            lead_intensity: 45.0,
            core_intensity: 35.0,
            core_exponent: 24.0,
            burn: Vec3::new(0.0, 0.203925, 1.0),
            energon_intensity: 5.27451,
            energon_contrast: 5.0,
            noise_scale: 1.0,
            arcs: false,
        },
    ),
    (
        "TR_ElectricalDefragOverlay_MATINST",
        DefragLook {
            fresnel: Vec3::new(0.305882, 0.564706, 0.913726),
            fresnel_intensity: 8.286275,
            fresnel_exponent: 4.407844,
            lead: Vec3::new(0.047059, 0.180392, 1.0),
            lead_intensity: 4.0,
            core_intensity: 7.0,
            core_exponent: 32.0,
            burn: Vec3::new(0.486275, 0.525490, 1.0),
            energon_intensity: 8.980392,
            energon_contrast: 1.101961,
            noise_scale: 5.0,
            arcs: true,
        },
    ),
    (
        "TR_CorrosionOverlay_MATINST",
        DefragLook {
            fresnel: Vec3::new(0.235294, 0.090196, 0.0),
            fresnel_intensity: 7.078432,
            fresnel_exponent: 2.247059,
            lead: Vec3::new(0.188235, 0.078431, 0.0),
            lead_intensity: 4.156863,
            core_intensity: 4.380393,
            core_exponent: 24.0,
            burn: Vec3::new(0.388235, 0.211765, 0.0),
            energon_intensity: 1.011765,
            energon_contrast: 7.529412,
            noise_scale: 1.0,
            arcs: false,
        },
    ),
    (
        "TR_MoltenOverlay_MATINST",
        DefragLook {
            fresnel: Vec3::new(1.0, 0.098039, 0.0),
            fresnel_intensity: 7.0,
            fresnel_exponent: 3.913726,
            lead: Vec3::new(0.905882, 0.0, 0.0),
            lead_intensity: 45.0,
            core_intensity: 35.0,
            core_exponent: 24.0,
            burn: Vec3::new(1.0, 0.0, 0.0),
            energon_intensity: 5.27451,
            energon_contrast: 5.0,
            noise_scale: 1.0,
            arcs: false,
        },
    ),
    (
        "TR_RetroPistolOverlay_MATINST",
        DefragLook {
            fresnel: Vec3::new(0.634831, 0.106994, 0.346337),
            fresnel_intensity: 7.078432,
            fresnel_exponent: 2.247059,
            lead: Vec3::new(0.4, 0.078431, 1.0),
            lead_intensity: 4.156863,
            core_intensity: 4.380393,
            core_exponent: 24.0,
            burn: Vec3::new(0.522472, 0.015217, 0.436239),
            energon_intensity: 1.011765,
            energon_contrast: 7.529412,
            noise_scale: 1.0,
            arcs: false,
        },
    ),
];

/// An overlay's look by its material's name (TR_DefragOverlay_MATINST's for one
/// the demo hasn't).
pub fn defrag_look(overlay: &str) -> &'static DefragLook {
    DEFRAG_LOOKS
        .iter()
        .find(|(n, _)| *n == overlay)
        .map_or(&DEFRAG_LOOKS[0].1, |(_, l)| l)
}

/// Each of `meshes` given a material of its own for its dissolve (TnDeresser drawing
/// its overlay over the mesh by its HmMeshHighlightComponent: paint.wgsl's
/// defrag_overlay), from its paint (an unpainted one painted white, its energon
/// `energon`): masked, so the depth prepass and the shadows take out what is gone
/// too (paint_prepass.wgsl), its overlay `start`'s (its material's look, its reach
/// `radius` times the scale, its tiling), out from `centre`, its DefragValue at 0.
#[allow(clippy::too_many_arguments)]
pub fn dissolve_meshes(
    commands: &mut Commands,
    meshes: &[Entity],
    materials: &MeshMaterials,
    (paints, cache, standard): (
        &mut Assets<PaintMaterial>,
        &mut PaintCache,
        &Assets<StandardMaterial>,
    ),
    textures: Option<&crate::cloak::CloakTextures>,
    start: &deaths::DeresStart,
    invisible: f32,
    (centre, energon, radius): (Vec3, Vec3, f32),
) -> Vec<Dissolving> {
    let look = defrag_look(&start.overlay);
    let mut out = Vec::new();
    for &m in meshes {
        let Ok((paint, plain)) = materials.get(m) else {
            continue;
        };
        let (base, had) = match (paint, plain) {
            (Some(p), _) => (paints.get(&p.0).cloned(), Had::Paint(p.0.clone())),
            (None, Some(s)) => (
                cache
                    .colored(
                        &s.0,
                        (Vec3::ONE, Vec3::ONE, energon),
                        false,
                        standard,
                        paints,
                    )
                    .and_then(|h| paints.get(&h).cloned()),
                Had::Plain(s.0.clone()),
            ),
            _ => continue,
        };
        let Some(mut mat) = base else {
            continue;
        };
        mat.base.alpha_mode = AlphaMode::Mask(0.5);
        let c = &mut mat.extension.colors;
        c.deres = centre.extend(overlay_reach(radius, start.radius));
        c.deres_fx = Vec4::new(
            0.0,
            invisible,
            look.noise_scale,
            if look.arcs { 1.0 } else { 0.0 },
        );
        c.deres_fresnel = (look.fresnel * look.fresnel_intensity).extend(look.fresnel_exponent);
        c.deres_lead = look.lead.extend(look.lead_intensity);
        c.deres_burn = look.burn.extend(look.core_intensity);
        c.deres_energon = Vec4::new(
            look.energon_intensity,
            look.energon_contrast,
            look.core_exponent,
            start.tiling,
        );
        if let Some(t) = textures {
            t.give_dissolve(&mut mat.extension);
        }
        let h = paints.add(mat);
        commands
            .entity(m)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(h.clone()));
        out.push(Dissolving {
            mesh: m,
            paint: h,
            had,
        });
    }
    out
}

/// The overlay's reach for a body whose bounds' radius is `radius` (metres): times
/// 1.2 (TnDeathActionHighlight.UpdateDeresserObjectRadiusParam) and its death's
/// RadiusAdditionalScale.
pub fn overlay_reach(radius: f32, scale: f32) -> f32 {
    radius * 1.2 * scale
}

/// How far out the dissolve's front has come at DefragValue `value` over a reach
/// `reach`: where the noise's k (at least the value, to the 0.75th) meets the
/// distance (0.75 of the reach at 1). (The body's decals go as it passes them.)
pub fn dissolve_front(value: f32, reach: f32) -> f32 {
    value.max(0.0).powf(0.75) / 0.75 * reach
}

/// The dissolve at DefragValue `value` (its deresser's), its overlay out from
/// `centre` (following the body: PositionRealtimeUpdate).
pub fn dissolve_to(
    paints: &mut Assets<PaintMaterial>,
    list: &[Dissolving],
    centre: Vec3,
    value: f32,
) {
    for d in list {
        if let Some(m) = paints.get_mut(&d.paint) {
            let c = &mut m.extension.colors;
            c.deres = centre.extend(c.deres.w);
            c.deres_fx.x = value;
        }
    }
}

/// The meshes' own materials back (a player's robot, come back after his death).
pub fn undissolve(commands: &mut Commands, list: Vec<Dissolving>) {
    for d in list {
        let mut e = commands.entity(d.mesh);
        match d.had {
            Had::Paint(p) => {
                e.try_insert(MeshMaterial3d(p));
            }
            Had::Plain(s) => {
                e.try_remove::<MeshMaterial3d<PaintMaterial>>()
                    .try_insert(MeshMaterial3d(s));
            }
        }
    }
}

/// The death lights (DeathLight's: kept apart from the players').
pub type DeathLights<'w, 's> = Query<
    'w,
    's,
    (&'static mut PointLight, &'static mut Transform),
    (With<DeathLight>, Without<crate::Player>),
>;

/// The players at this PC (a death's camera shake's).
pub type ShakenPlayers<'w, 's> = Query<
    'w,
    's,
    (&'static Transform, &'static mut crate::Player),
    (With<LocalPlayer>, Without<DeathLight>),
>;

/// A death light as its death's beat has it this frame (`beat`: its settings and
/// brightness, none once it is out): put on, set (kept on the body's middle `sync`
/// when it says, else at its offset from the pawn), or taken away.
pub fn death_light(
    commands: &mut Commands,
    light: &mut Option<Entity>,
    beat: Option<(deaths::LightDo, f32)>,
    sync: Vec3,
    place: &deaths::Place,
    lights: &mut DeathLights,
) {
    let Some((l, brightness)) = beat else {
        if let Some(e) = light.take() {
            commands.entity(e).try_despawn();
        }
        return;
    };
    let at = if l.sync { sync } else { place.point(l.at) };
    let color = Color::srgb_u8(l.color[0], l.color[1], l.color[2]);
    let intensity = brightness * crate::fx::LIGHT_LUMENS;
    let range = l.radius / deaths::UU;
    if let Some((mut p, mut t)) = light.and_then(|e| lights.get_mut(e).ok()) {
        p.intensity = intensity;
        p.color = color;
        p.range = range;
        t.translation = at;
        return;
    }
    *light = Some(
        commands
            .spawn((
                DeathLight,
                crate::graphics::LiveLight,
                PointLight {
                    color,
                    intensity,
                    range,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(at),
            ))
            .id(),
    );
}

/// A death's camera shake (TnDeathActionCameraShake): all of it for each player at
/// this PC within its radius of `at`.
pub fn death_shake(s: &deaths::ShakeDo, at: Vec3, me: &mut ShakenPlayers) {
    for (t, mut p) in me.iter_mut() {
        if t.translation.distance(at) <= s.radius / deaths::UU {
            p.screen_shakes.push(crate::riot::ScreenShake {
                radius: s.radius / deaths::UU,
                secs: s.secs,
                rot_amp: s.rot,
                rot_freq: s.rot_freq,
                loc_amp: s.loc,
                loc_freq: s.loc_freq,
                fov_amp: s.fov.0,
                fov_freq: s.fov.1,
            });
        }
    }
}

/// Where an enemy's socket `name` is (its skeleton's), else its bone of that name:
/// its place and its X and Y (Unreal's: glTF's x and z).
fn frame_of(
    name: &str,
    model: Option<&Model>,
    rig: Option<&Rig>,
    globals: &Query<&GlobalTransform>,
) -> Option<(Vec3, Vec3, Vec3)> {
    if let Some(f) = model.and_then(|m| m.socket_world(name, globals)) {
        return Some(f);
    }
    let g = globals.get(rig?.bone(name)?).ok()?;
    let r = g.rotation();
    Some((g.translation(), r * Vec3::X, r * Vec3::Z))
}

/// The enemies whose health has run out die: the kill's money for the player at
/// this PC (and his score, with the kill if it was his: scoreboard.rs), and the
/// death its death set has for the kill (TnDeathPlayer.Pick: the first entry whose
/// death types the kill has, as its context factory finds them: deaths.rs's Kill),
/// begun (TnDeathPlayer.Activate: play_deaths, this frame); it stops being an enemy
/// (what rounds and the HUD find), and the kill is counted (xp.rs sees it go). A
/// kill its set has no death for leaves no body.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn deaths(
    mut commands: Commands,
    kinds: Res<Kinds>,
    data: Res<deaths::DeathData>,
    net: Res<net::Net>,
    mut score: crate::split::BySeat<Score>,
    mut tally: crate::split::BySeat<crate::scoreboard::Tally>,
    me: Query<(Entity, Option<&crate::split::Seat>), With<LocalPlayer>>,
    everyone: Query<Entity, With<crate::Player>>,
    enemies: Query<(Entity, &Enemy, &Foe, &Transform)>,
    mut sfx: ResMut<Sfx>,
) {
    for (e, enemy, foe, transform) in &enemies {
        if enemy.alive() {
            continue;
        }
        let Some(k) = kinds.kinds.get(enemy.kind) else {
            commands.entity(e).despawn();
            continue;
        };
        for score in score.each() {
            score.destroyed += 1;
        }
        // The money: the killer paid for what the others didn't take, the others
        // for what they did; a kill nobody made, half to each. (Each of this PC's
        // players his own: split screen's guest has his.)
        let money = kinds.money(enemy.kind, net.difficulty());
        let max = enemy.max_health.max(1.0);
        for (me, seat) in &me {
            let seat = crate::split::seat_of(seat);
            let share = match enemy.hurt.last {
                Some((killer, _)) => {
                    let mine: f32 = enemy
                        .damage_by
                        .iter()
                        .filter(|(j, _)| *j == me)
                        .map(|(_, d)| *d)
                        .sum();
                    if killer == me {
                        let others: f32 = enemy
                            .damage_by
                            .iter()
                            .filter(|(j, _)| *j != me && everyone.contains(*j))
                            .map(|(_, d)| *d)
                            .sum();
                        ((max - others) / max).clamp(0.0, 1.0)
                    } else {
                        (mine / max).clamp(0.0, 1.0)
                    }
                }
                None => SHARED_KILL,
            };
            let pay = (money * share).round() as u32;
            if pay > 0 {
                score.seat(seat).energon += pay;
                tally.seat(seat).earned(pay);
            }
            // (A kill is his who dealt the last blow.)
            if let Some((killer, _)) = enemy.hurt.last
                && killer == me
            {
                tally.seat(seat).killed();
            }
        }
        // Its death: the kill's death types (TnDeathContextFactory.FindActiveTypes
        // and its form's factory's). (Its chances by its spawn's seed and where the
        // blow struck, which every PC has alike: the same death on each.)
        let a = enemy.last_hit.at;
        let mut seed = foe.seed
            ^ a.x.to_bits().rotate_left(5)
            ^ a.y.to_bits().rotate_left(13)
            ^ a.z.to_bits().rotate_left(21);
        let hit = enemy.last_hit;
        let m = enemy.motion;
        let at =
            m.at.unwrap_or(transform.translation + Vec3::Y * k.half_height);
        let robot = k.form == Form::Robot;
        let kill = deaths::Kill {
            damage: hit.death.types(),
            gib: hit.gib,
            side: deaths::side_struck(enemy.facing, hit.at - at),
            stationary: m.velocity.length() * deaths::UU <= STATIONARY_SPEED,
            robot,
            vehicle_boost: (!robot).then_some(m.boosting.unwrap_or(false)),
            downed: false,
            head: enemy.headshot(),
            hovering: false,
            airborne: m.airborne,
            ceiling: m.up.y < -std::f32::consts::FRAC_1_SQRT_2,
            factory: match k.form {
                Form::Robot => deaths::Factory::Robot,
                Form::Insect if k.deaths == DeathSet::Sniper => deaths::Factory::Sniper,
                _ => deaths::Factory::Vehicle,
            },
        };
        if k.deaths == DeathSet::Drone {
            sfx.play_at(SND_DRONE_DEATH, at);
        }
        let roll = deaths::rand(&mut seed);
        let Some(def) = data.pick(k.deaths.name(), kill.types(), roll) else {
            commands.entity(e).despawn();
            continue;
        };
        debug!(
            "{} dies of {:?}: {}",
            k.id,
            kill.types().names(),
            deaths::short(&def.name)
        );
        let place = deaths::Place {
            at,
            turn: transform.rotation,
            hit: hit.at,
            // (TearOffMomentum: the blow's momentum over the pawn's mass.)
            momentum: hit.momentum / m.mass.max(1.0) / deaths::UU,
            velocity: m.velocity,
        };
        let pick = (deaths::rand(&mut seed) * 65536.0) as u32;
        let struck = if hit.death.is_gunshot() {
            enemy.body.part_at(hit.at).and_then(|p| enemy.body.bone(p))
        } else {
            None
        };
        commands.entity(e).remove::<Enemy>().insert(Dying {
            kind: enemy.kind,
            timeline: deaths::Timeline::new(def, seed),
            place,
            struck,
            pick,
            seed,
            fell: false,
            dissolving: Vec::new(),
            reach: enemy.radius(),
            light: None,
            effects: Vec::new(),
            bones: Vec::new(),
        });
    }
}

/// The world the dying play out their deaths in (play_deaths' resources).
#[derive(bevy::ecs::system::SystemParam)]
pub struct DeathWorld<'w, 's> {
    pub time: Res<'w, Time>,
    pub data: Res<'w, deaths::DeathData>,
    pub sfx: ResMut<'w, Sfx>,
    pub fx: Option<ResMut<'w, crate::levelfx::FxSystems>>,
    pub gibs: ResMut<'w, crate::gibs::Gibs>,
    pub globals: Query<'w, 's, &'static GlobalTransform>,
    pub lights: DeathLights<'w, 's>,
    pub paints: ResMut<'w, Assets<PaintMaterial>>,
    pub cache: ResMut<'w, PaintCache>,
    pub standard: Res<'w, Assets<StandardMaterial>>,
    /// The dissolve's textures (the cloak's overlay's and its own).
    pub overlay: Option<Res<'w, crate::cloak::CloakTextures>>,
}

/// The dying play out their deaths (TnDeathPlayer.Tick: deaths.rs's Timeline), each
/// frame's beat done to their bodies: their clips, their ragdolls taking over and
/// kicked, their sounds, their effects held on their sockets and bones (and the
/// trails of the pieces their ragdolls cut away), camera shakes, destructibles,
/// their lights, their dissolve, their ragdolls' gravity, spin and boost, their
/// meshes hidden, their bones scaled; gone once their deaths are over.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn play_deaths(
    mut commands: Commands,
    kinds: Res<Kinds>,
    mut w: DeathWorld,
    mut dying: Query<(Entity, &mut Dying, Option<&Model>, Option<&mut Rig>)>,
    mut players: Query<&mut AnimationPlayer>,
    mut shown: Query<&mut Visibility>,
    mesh_materials: MeshMaterials,
    mut me: ShakenPlayers,
) {
    let dt = w.time.delta_secs();
    for (e, mut d, model, mut rig) in &mut dying {
        let d = &mut *d;
        let k = kinds.kinds.get(d.kind);
        let energon = k.map_or(Vec3::ONE, |k| k.energon);
        // Its clips (each name's, one of them for a chooser group's).
        let pick = d.pick as usize;
        let clip_of = |name: &str| {
            k.and_then(|k| k.nodes.get(name))
                .filter(|l| !l.is_empty())
                .map(|l| l[pick % l.len()])
        };
        let length = |name: &str, rate: f32| clip_of(name).map(|(_, secs)| secs / rate.max(0.05));
        let env = deaths::Env {
            clip: &length,
            collided: rig.as_ref().is_some_and(|r| r.touched),
        };
        let beat = d.timeline.step(dt, &env);
        // Where its body is now.
        let centre = rig
            .as_ref()
            .and_then(|r| r.body_centre(&w.globals))
            .unwrap_or(d.place.at);
        let animator = model.and_then(|m| m.animator);
        if let Some((name, rate, looping, _)) = &beat.anim
            && let Some((node, _)) = clip_of(name)
            && let Some(mut p) = animator.and_then(|a| players.get_mut(a).ok())
        {
            p.stop_all();
            let playing = p.play(node);
            playing.set_speed(*rate);
            if *looping {
                playing.repeat();
            }
        }
        if let Some(rd) = &beat.ragdoll {
            let carry = if d.fell { Vec3::ZERO } else { d.place.velocity };
            let kick = deaths::kick(rd, &d.place, d.struck.as_deref(), carry, &mut d.seed);
            if let Some(r) = rig.as_mut() {
                if matches!(r.state, RigState::Animated) {
                    r.state = RigState::Falling(kick);
                } else {
                    r.pending = Some(kick);
                }
            }
            d.fell = true;
            if let Some(mut p) = animator.and_then(|a| players.get_mut(a).ok()) {
                p.stop_all();
            }
        }
        for cue in &beat.sounds {
            w.sfx.play_at(cue, centre);
        }
        // (Its sound events are its dialog's: its AI says its death cries.)
        for (system, socket) in &beat.effects {
            let Some(fx) = w.fx.as_mut() else { break };
            let (at, x, y) = frame_of(socket, model, rig.as_deref(), &w.globals).unwrap_or((
                centre,
                d.place.dir(Vec3::X),
                d.place.dir(Vec3::Y),
            ));
            if let Some(id) = fx.spawn(system, at, x, &[]) {
                fx.place_turned(id, at, x, y);
                d.effects.push((id, socket.clone()));
            }
        }
        // The trails of the pieces its ragdoll cut away.
        if let Some(r) = rig.as_mut() {
            for (bone, trail) in std::mem::take(&mut r.severed) {
                let (Some(fx), Some(name)) = (w.fx.as_mut(), r.names.get(bone)) else {
                    continue;
                };
                let frame = r
                    .bones
                    .get(bone)
                    .and_then(|b| w.globals.get(*b).ok())
                    .map(|g| {
                        (
                            g.translation(),
                            g.rotation() * Vec3::X,
                            g.rotation() * Vec3::Z,
                        )
                    });
                if let Some((at, x, y)) = frame
                    && let Some(id) = fx.spawn(&trail, at, x, &[])
                {
                    fx.place_turned(id, at, x, y);
                    d.effects.push((id, Arc::from(name.as_str())));
                }
            }
        }
        for s in &beat.shakes {
            death_shake(s, centre, &mut me);
        }
        for name in &beat.destructibles {
            if let Some(dsys) = w.data.destructibles.get(&**name).cloned() {
                let place = deaths::Place {
                    at: centre,
                    ..d.place
                };
                crate::gibs::set_off(
                    &dsys,
                    &place,
                    &mut d.seed,
                    true,
                    &mut w.fx,
                    &mut w.sfx,
                    &mut w.gibs,
                );
            }
        }
        if let Some(start) = &beat.deres_start {
            // (Its bounds: its physics asset's bodies' as it died, as the game's
            // skeletal meshes' are.)
            let invisible = beat.deres.map_or(INVISIBLE, |(_, inv)| inv);
            d.dissolving = dissolve_meshes(
                &mut commands,
                model.map(|m| m.meshes.as_slice()).unwrap_or(&[]),
                &mesh_materials,
                (&mut w.paints, &mut w.cache, &w.standard),
                w.overlay.as_deref(),
                start,
                invisible,
                (centre, energon, d.reach),
            );
        }
        if let Some((value, _)) = beat.deres {
            dissolve_to(&mut w.paints, &d.dissolving, centre, value);
        }
        death_light(
            &mut commands,
            &mut d.light,
            beat.light,
            centre,
            &d.place,
            &mut w.lights,
        );
        if let Some(r) = rig.as_mut() {
            if let Some(g) = beat.gravity {
                r.gravity = g;
            }
            // (The spin's axes are the pawn's, a turn the other way round in the
            // demo's: Unreal's axes are left-handed.)
            r.spin = -d.place.dir(beat.spin);
            if beat.boost && r.boost.is_none() {
                r.boost = Some((d.place.dir(Vec3::X), d.place.velocity.length()));
            }
        }
        if beat.hide
            && let Ok(mut v) = shown.get_mut(e)
        {
            *v = Visibility::Hidden;
        }
        for (node, scale) in &beat.bones {
            // (The head's SkelControlSingleBone, Headshot: the head bones.)
            if &**node == "Headshot"
                && let Some(b) = rig
                    .as_ref()
                    .and_then(|r| HEAD_BONES.iter().find_map(|h| r.bone(h)))
            {
                d.bones.push((b, *scale));
            }
        }
        // Its held effects carried with their sockets and bones.
        if let Some(fx) = w.fx.as_mut() {
            d.effects.retain(|(id, socket)| {
                if !fx.alive(*id) {
                    return false;
                }
                if let Some((at, x, y)) = frame_of(socket, model, rig.as_deref(), &w.globals) {
                    fx.place_turned(*id, at, x, y);
                }
                true
            });
        }
        if d.timeline.gone() {
            if let Some(fx) = w.fx.as_mut() {
                for (id, _) in d.effects.drain(..) {
                    fx.stop(id);
                }
            }
            if let Some(l) = d.light.take() {
                commands.entity(l).try_despawn();
            }
            commands.entity(e).despawn();
        }
    }
}

/// The bones the dying's deaths scale (TnDeathModifierControlBone: a headshot's head
/// shrunk away), held so after the clips have posed them, before the ragdolls pose
/// theirs.
fn scale_death_bones(dying: Query<&Dying>, mut transforms: Query<&mut Transform>) {
    for d in &dying {
        for (bone, scale) in &d.bones {
            if let Ok(mut t) = transforms.get_mut(*bone)
                && t.scale != Vec3::splat(*scale)
            {
                t.scale = Vec3::splat(*scale);
            }
        }
    }
}

/// The dying let go (a new match): their lights and effects with them.
fn clear_dying(
    mut commands: Commands,
    dying: Query<Entity, With<Dying>>,
    lights: Query<Entity, With<DeathLight>>,
) {
    for e in dying.iter().chain(lights.iter()) {
        commands.entity(e).try_despawn();
    }
}

/// The name the HUD shows for an enemy.
pub fn name_of<'a>(kinds: &'a Kinds, enemy: &Enemy) -> &'a str {
    kinds.kinds.get(enemy.kind).map_or("", |k| k.name.as_str())
}

pub struct EnemiesPlugin;

impl Plugin for EnemiesPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Kinds::load(&crate::assets_dir()))
            .init_resource::<crate::ai::graph::MaskJobs>()
            .insert_resource(deaths::DeathData::load(&crate::assets_dir()))
            .add_systems(Update, prepare_kinds)
            .add_observer(dress)
            // (Deaths before the clips run, so a death's clip starts this frame and
            // its ragdoll takes over this frame; its bones scaled after the clips;
            // the bodies after the transforms are in, for the next frame's shots.)
            .add_systems(
                PostUpdate,
                (deaths, play_deaths)
                    .chain()
                    .before(bevy::app::AnimationSystems)
                    .run_if(in_state(crate::menu::AppState::Game)),
            )
            .add_systems(
                PostUpdate,
                scale_death_bones
                    .after(bevy::app::AnimationSystems)
                    .before(crate::ragdoll::drive_rigs),
            )
            .add_systems(OnExit(crate::menu::AppState::Game), clear_dying)
            .add_systems(
                PostUpdate,
                update_bodies.after(bevy::transform::TransformSystems::Propagate),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Guardian's arm cannons hang one on each arm (its Guardian Blaster's two
    /// attachment sockets), so both show; every other enemy's gun is a mesh for
    /// each hand, only the one it holds it in showing.
    #[test]
    fn the_guardian_has_a_cannon_on_each_arm() {
        let kinds = Kinds::load(&crate::assets_dir());
        let Some(guard) = kinds.kinds.iter().find(|k| k.id == "Guard") else {
            return;
        };
        assert!(guard.guns.iter().all(Option::is_some));
        assert!(guard.guns_hung_on_both_arms());
        for k in kinds.kinds.iter().filter(|k| k.id != "Guard") {
            assert!(!k.guns_hung_on_both_arms(), "{}", k.id);
        }
    }

    #[test]
    fn reads_the_enemies() {
        let kinds = Kinds::load(&crate::assets_dir());
        if kinds.kinds.is_empty() {
            return;
        }
        // Downfall's roster, the Titan's drones with it.
        let downfall = &kinds.maps["downfall"];
        assert!(downfall.len() >= 9);
        let titan = kinds.kinds.iter().find(|k| k.id == "Titan").unwrap();
        assert_eq!(titan.deaths, DeathSet::Titan);
        assert!(titan.health >= 2000.0);
        assert!(titan.clips.iter().any(|c| c.role == "Death"));
        // A soldier: its parts, its body and joints, its deaths from npc_clips.
        let soldier = kinds
            .kinds
            .iter()
            .find(|k| k.id == "SoldierAut_Prime")
            .unwrap();
        assert_eq!(soldier.deaths, DeathSet::Basic);
        assert!(
            soldier.physics.bodies.len() >= 10,
            "{}",
            soldier.physics.bodies.len()
        );
        assert!(soldier.physics.joints.len() >= 10);
        // (Its bodies' masses: its physics asset's, where it sets one.)
        let mass = |bone: &str| {
            soldier
                .physics
                .bodies
                .iter()
                .find(|b| b.bone == bone)
                .unwrap()
                .mass
        };
        assert_eq!(mass("C_Spine04_Head_XB"), Some(10.0));
        assert_eq!(mass("C_Spine00_Hips_XB"), None);
        assert!(
            soldier
                .clips
                .iter()
                .filter(|c| c.role == "Death_Gunfire_Front")
                .count()
                >= 10
        );
        assert!(!soldier.colours.is_empty());
        assert!(soldier.slots.iter().any(|s| s.iter().any(|a| a.is_none())));
        // Ancients' and Ignition's Snipers in their own material's paint, lavender
        // with gunmetal trim, as the game shows them (not their blueprint's red and
        // gold), in both forms.
        for id in ["Sniper", "Sniper~other"] {
            let k = kinds.kinds.iter().find(|k| k.id == id).unwrap();
            assert_eq!(k.colours, vec![[[181, 158, 233], [112, 113, 124]]], "{id}");
        }
        // The jets' hull.
        let jet = kinds.kinds.iter().find(|k| k.form == Form::Jet).unwrap();
        assert!(matches!(
            jet.physics.bodies[0].shapes[0],
            Shape::Convex { .. }
        ));
        assert_eq!(jet.deaths, DeathSet::Flyer);
        // The money: the Titan's 500 on Normal.
        let t = kinds.kinds.iter().position(|k| k.id == "Titan").unwrap();
        assert_eq!(kinds.money(t, 1), 500.0);
    }

    /// Each type's death set's death for a kill (TnDeathPlayer.Pick, the death
    /// types as the context factories gather them).
    #[test]
    fn deaths_by_how_they_died() {
        let data = deaths::DeathData::load(&crate::assets_dir());
        if data.sets.is_empty() {
            return;
        }
        let robot = |death: Death, head: bool| deaths::Kill {
            damage: death.types(),
            side: deaths::Types::of("HitFromFront"),
            robot: true,
            head,
            ..Default::default()
        };
        let pick = |set: DeathSet, kill: deaths::Kill| {
            data.pick(set.name(), kill.types(), 0.5)
                .map(|d| deaths::short(&d.name).to_string())
                .unwrap_or_default()
        };
        assert_eq!(
            pick(DeathSet::Basic, robot(Death::Gunfire, false)),
            "Gunfire_Front_DEATH"
        );
        assert!(pick(DeathSet::Basic, robot(Death::SingleShot, true)).starts_with("Headshot"));
        assert!(pick(DeathSet::Basic, robot(Death::SingleShot, false)).starts_with("Shotgun"));
        assert_eq!(
            pick(DeathSet::Basic, robot(Death::Drain, false)),
            "DrainGun_DEATH"
        );
        assert_eq!(
            pick(DeathSet::Titan, robot(Death::Gunfire, false)),
            "Generic_DEATH"
        );
        // A drone (a plane) shot down, and one that blew itself up.
        let drone = |death: Death| deaths::Kill {
            damage: death.types(),
            vehicle_boost: Some(false),
            factory: deaths::Factory::Vehicle,
            ..Default::default()
        };
        assert_eq!(
            pick(DeathSet::Drone, drone(Death::Gunfire)),
            "Generic_DEATH"
        );
        assert_eq!(
            pick(DeathSet::Drone, drone(Death::SelfDestruct)),
            "SelfDestruct_DEATH"
        );
        // Every set has a death for every kill of its form (its last entry takes
        // what the others don't).
        for set in [
            DeathSet::Basic,
            DeathSet::Titan,
            DeathSet::Guardian,
            DeathSet::Leaper,
            DeathSet::Swarmer,
            DeathSet::Sniper,
            DeathSet::Drone,
            DeathSet::Flyer,
        ] {
            let robot_form = !matches!(set, DeathSet::Drone | DeathSet::Flyer | DeathSet::Swarmer);
            for i in 0..13 {
                let death = Death::from_index(i);
                let mut kill = robot(death, false);
                kill.robot = robot_form;
                if !robot_form {
                    kill.vehicle_boost = Some(false);
                    kill.factory = deaths::Factory::Vehicle;
                }
                assert!(!pick(set, kill).is_empty(), "{set:?} {death:?}");
            }
        }
        // The deaths' numbers hold (sent online).
        assert_eq!(
            Death::from_index(Death::SelfDestruct.index()),
            Death::SelfDestruct
        );
        assert_eq!(Death::Fire.index(), 11);
    }

    #[test]
    fn bodies_take_shots_and_headshots() {
        let body = Body::standing_robot();
        let (t, part) = body
            .ray_part(Vec3::new(0.0, 2.9, 10.0), -Vec3::Z, 100.0)
            .unwrap();
        assert!(part.head && (t - 9.6).abs() < 1e-3);
        let (t, part) = body
            .ray_part(Vec3::new(0.0, 1.0, 10.0), -Vec3::Z, 100.0)
            .unwrap();
        assert!(!part.head && (t - 9.5).abs() < 1e-3);
        assert!(
            body.ray(Vec3::new(3.0, 1.0, 10.0), -Vec3::Z, 100.0)
                .is_none()
        );
        assert!(
            body.ray_wide(Vec3::new(0.9, 1.0, 10.0), -Vec3::Z, 100.0, 0.5)
                .is_some()
        );
        assert!((body.reach(Vec3::new(2.0, 1.0, 0.0)) - 1.5).abs() < 1e-4);
        assert!(body.head_at(Vec3::new(0.1, 3.2, 0.0)));
        // A gunshot on its head takes twice the damage; a blast there, or a round
        // anywhere else, doesn't.
        let mut enemy = Enemy {
            kind: 0,
            robot: true,
            health: 1000.0,
            max_health: 1000.0,
            damage_scale: 1.0,
            ai_damage_scale: 1.0,
            head_bones: Vec::new(),
            bone_damage: Vec::new(),
            from_behind: (1.0, 1.0),
            facing: Vec3::NEG_Z,
            emp_left: 0.0,
            emp_fx: None,
            mark: orbital::Mark::default(),
            burn: specials::Burn::default(),
            emp_by: None,
            hurt: xp::Hurt::default(),
            last_hit: Hit::default(),
            last_by: None,
            last_blow: xp::Blow::Weapon,
            damage_by: Vec::new(),
            body,
            hits: Vec::new(),
            motion: Motion::default(),
        };
        let head = Vec3::new(0.0, 2.9, 0.4);
        let chest = Vec3::new(0.0, 1.0, 0.5);
        let shot = |at| Hit::new(Death::Gunfire, at, -Vec3::Z);
        assert_eq!(enemy.take(10.0, None, xp::Blow::Weapon, shot(head)), 20.0);
        assert!(enemy.headshot());
        assert_eq!(enemy.take(10.0, None, xp::Blow::Weapon, shot(chest)), 10.0);
        assert!(!enemy.headshot());
        let blast = Hit::new(Death::Explosive, head, -Vec3::Z);
        assert_eq!(enemy.take(10.0, None, xp::Blow::Weapon, blast), 10.0);
        assert!(!enemy.headshot());
        // Its AI's own multiplier (the Guard's form's) on all of it.
        enemy.ai_damage_scale = 0.5;
        assert_eq!(enemy.take(10.0, None, xp::Blow::Weapon, shot(head)), 10.0);
    }
}
