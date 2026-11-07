//! Oblivion's exploding barrel (TnStaticDestructibleActor_4264, by the start: its
//! destructible ENV_IAC_ExplodingBarrel_p's DES_AutobotExplodingBarrel_DSYS), from
//! levels/<map>_barrels.txt, levels/<map>_barrels.bin and
//! models/props/exploding_barrel.glb (tools/build_barrels.py, which reads the
//! blueprint; its debris are tools/build_weapon_fx.py's mesh systems).
//!
//! * **Standing** (its first state, InitialHealth 25): the barrel at its components'
//!   Scale (0.7), its lights glowing red, the red outline round it (as the pickups'
//!   cyan one), solid to movement and shots (its collision group); its hum
//!   (EMIT_AMB_MACH_OSC_07) as each match starts. It is a target
//!   (TnTargetableDestructionEffectFactory): the crosshair turns red over it.
//! * **Damage** (HmDestructibleBlueprint): the players' rounds that strike it (a
//!   gun's, a car's, a vehicle's), the blasts that reach it (its rounds', the
//!   enemies' rockets and grenades) and the blows that land on it (a charging
//!   punch, a sword swing) take their damage off its health, as they would a
//!   pawn's: an AI's at half (AIDamageModifier 0.5), a blast's at a quarter when
//!   its middle is more than 1 m from it (RadiusDamageModifier 0.25,
//!   MinRadiusDamageDistanceForModifier 100), a damage type its DamageModifiers
//!   name at theirs (Corrosion's 5 times).
//! * **Blowing up**, its health gone (its link to AutobotExplodingBarrel_State2,
//!   whichever kind of damage brought it there): the barrel and its outline give
//!   way to the wreck (LPdamage2_STAT, unlit), and the collision with them; the
//!   explosion (Exp_GenericLvl_MED_FX, DEST_ENERGON_TANK_01_EXPLO); its debris
//!   flung out, as the deaths' gibs fly (gibs.rs: its own pieces, the big one
//!   trailing smoke and sparks, and six burning cover chunks); the fire left on the
//!   wreck (Fire_ExpBarrel_FX) until the state ends 10 s on (its
//!   AutomaticTransitionTime: TerminateEffectOnExitState); and its blast
//!   (HmRadialDamageDestructionEffectFactory: HurtRadius, 1.93 m up, 150 of
//!   TnDamageTypeBruticusFire to everything within 15 m in sight of it, all of it
//!   at any distance: bDoFullDamage, with its momentum, 125000). The blast is its
//!   destroyer's (the damage's instigator): one a player set off hurts the enemies
//!   (Escalation's players never hurt each other or themselves:
//!   SelfDamageMultiplier 0), one an enemy set off the players.
//! * **Online**: the players' rounds, blasts and blows strike it on every PC, as
//!   they strike the enemies there; a PC whose own player or AI (the host's) blows
//!   it up says so (JazzEvent::BarrelBlown), for a PC that missed a blow; there it
//!   blows up too, as its destroyer's.
//!
//! Each match starts with the barrels standing.

use std::path::Path;
use std::sync::Arc;

use bevy::gltf::{Gltf, GltfLoaderSettings};
use bevy::prelude::*;

use crate::collision::{Blocks, LevelCollision};
use crate::deaths::{self, Destructible, Piece, Place, UU};
use crate::enemies::{Enemy, Hit};
use crate::gltffiles::SceneOf;
use crate::hud::HudEvent;
use crate::levelfx::{FxId, FxSystems};
use crate::net::JazzEvent;
use crate::sound::Sfx;
use crate::{Level, LocalPlayer, Player, RemotePlayer};

/// The barrel's model (tools/build_barrels.py) and its nodes.
pub const MODEL: &str = "models/props/exploding_barrel.glb";
const BARREL_NODE: &str = "barrel";
const OUTLINE_NODE: &str = "barrel_outline";
const WRECK_NODE: &str = "wreck";
/// A round strikes a barrel where it stops within this of its shape (metres)...
const SKIN: f32 = 0.05;
/// ... and above its foot by this (one into the floor beside it doesn't).
const FOOT: f32 = 0.1;
/// Its shape when its collision can't be had: the standing barrel's, at 0.7 (metres).
const RADIUS: f32 = 1.26;
const HEIGHT: f32 = 3.33;
/// TnDamageTypeBruticusFire's kills are its deaths (enemies.rs: Fire).
const BLAST_TYPE: &str = "TnDamageTypeBruticusFire";
/// The players' pawns' Mass, a blow's momentum's divisor (Pawn.TakeDamage).
const PLAYER_MASS: f32 = 100.0;

/// A barrel's shape for what strikes it: an upright cylinder round its standing
/// collision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub axis: Vec2,
    pub radius: f32,
    pub bottom: f32,
    pub top: f32,
}

impl Shape {
    /// Round a set of triangles (x, z across, y up).
    fn of(tris: &[[f32; 9]]) -> Option<Shape> {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for t in tris {
            for p in t.chunks_exact(3) {
                let p = Vec3::new(p[0], p[1], p[2]);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        if tris.is_empty() {
            return None;
        }
        let axis = Vec2::new(lo.x + hi.x, lo.z + hi.z) * 0.5;
        let radius = tris
            .iter()
            .flat_map(|t| {
                t.chunks_exact(3)
                    .map(|p| Vec2::new(p[0], p[2]).distance(axis))
            })
            .fold(0.0, f32::max);
        Some(Shape {
            axis,
            radius,
            bottom: lo.y,
            top: hi.y,
        })
    }

    /// How far `p` is from it (0 inside).
    pub fn reach(&self, p: Vec3) -> f32 {
        let across = (Vec2::new(p.x, p.z).distance(self.axis) - self.radius).max(0.0);
        let up = (self.bottom - p.y).max(p.y - self.top).max(0.0);
        (across * across + up * up).sqrt()
    }

    /// Its point nearest `p`.
    pub fn nearest(&self, p: Vec3) -> Vec3 {
        let flat = Vec2::new(p.x, p.z) - self.axis;
        let flat = self.axis + flat.clamp_length_max(self.radius);
        Vec3::new(flat.x, p.y.clamp(self.bottom, self.top), flat.y)
    }

    /// Its middle.
    pub fn centre(&self) -> Vec3 {
        Vec3::new(self.axis.x, (self.bottom + self.top) * 0.5, self.axis.y)
    }

    /// Whether a round that stopped at `p` struck it.
    fn struck_at(&self, p: Vec3) -> bool {
        p.y > self.bottom + FOOT && self.reach(p) <= SKIN
    }
}

/// One of the map's barrels: its actor, its placement (its meshes' Scale in it), its
/// standing and wrecked collision groups and its shape.
#[derive(Clone, Debug)]
pub struct Spot {
    pub id: String,
    pub at: Vec3,
    pub turn: Quat,
    pub scale: f32,
    pub standing: Option<u16>,
    pub wreck: Option<u16>,
    pub shape: Shape,
}

impl Spot {
    /// Its actor's frame (Unreal's axes: forward, right, up), its effects' offsets
    /// in it (deaths.rs's Place: its forward the demo's -Z turned; the map's axes are
    /// Unreal's x, z, y).
    pub fn place(&self) -> Place {
        Place {
            at: self.at,
            turn: self.turn * Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
            hit: self.at,
            momentum: Vec3::ZERO,
            velocity: Vec3::ZERO,
        }
    }
}

/// Its blast (HurtRadius): its damage, radius (metres), momentum (Unreal's units)
/// and place (Unreal's units, the actor's frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blast {
    pub damage: f32,
    pub radius: f32,
    pub momentum: f32,
    pub at: Vec3,
}

/// What its blueprint says (tools/build_barrels.py).
#[derive(Clone, Debug)]
pub struct Rules {
    pub health: f32,
    /// AIDamageModifier; RadiusDamageModifier and its distance (metres).
    pub ai_scale: f32,
    pub radius_scale: f32,
    pub radius_near: f32,
    /// DamageModifiers: (damage type, its scale).
    pub modifiers: Vec<(String, f32)>,
    /// The standing state's effects (its hum), the blown state's (all but its
    /// blast) and how long that state lasts, the effects it ends with it (system,
    /// place: Unreal's units, the actor's frame) and its blast.
    pub standing: Arc<Destructible>,
    pub blown: Arc<Destructible>,
    pub blown_secs: f32,
    pub held: Vec<(Arc<str>, Vec3)>,
    pub blast: Option<Blast>,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            health: 1.0,
            ai_scale: 1.0,
            radius_scale: 1.0,
            radius_near: 0.5,
            modifiers: Vec::new(),
            standing: Arc::default(),
            blown: Arc::default(),
            blown_secs: 0.0,
            held: Vec::new(),
            blast: None,
        }
    }
}

/// A map's barrels and their rules.
#[derive(Clone, Debug, Default)]
pub struct BarrelsData {
    pub spots: Vec<Spot>,
    pub rules: Rules,
}

fn floats(s: &str) -> Vec<f32> {
    s.split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect()
}

fn vec3(s: &str) -> Vec3 {
    match floats(s)[..] {
        [x, y, z, ..] => Vec3::new(x, y, z),
        _ => Vec3::ZERO,
    }
}

impl BarrelsData {
    /// A barrels file's contents (tools/build_barrels.py), shapes not yet known.
    pub fn parse(text: &str) -> BarrelsData {
        let mut out = BarrelsData::default();
        let mut standing = None;
        let mut blown = None;
        let mut dsys = String::new();
        for line in text.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let f: Vec<&str> = line.split('\t').collect();
            let num =
                |i: usize, or: f32| f.get(i).and_then(|v| v.trim().parse().ok()).unwrap_or(or);
            let r = &mut out.rules;
            match (f[0], f.len()) {
                ("barrel", 5..) => {
                    let q = floats(f[3]);
                    let turn = match q[..] {
                        [x, y, z, w] => Quat::from_xyzw(x, y, z, w).normalize(),
                        _ => Quat::IDENTITY,
                    };
                    let at = vec3(f[2]);
                    let scale = num(4, 1.0);
                    out.spots.push(Spot {
                        id: f[1].to_string(),
                        at,
                        turn,
                        scale,
                        standing: None,
                        wreck: None,
                        shape: Shape {
                            axis: Vec2::new(at.x, at.z),
                            radius: RADIUS,
                            bottom: at.y,
                            top: at.y + HEIGHT,
                        },
                    });
                }
                ("health", 2..) => r.health = num(1, 1.0),
                ("aidamage", 2..) => r.ai_scale = num(1, 1.0),
                ("radiusdamage", 3..) => {
                    r.radius_scale = num(1, 1.0);
                    r.radius_near = num(2, 50.0) / UU;
                }
                ("modifier", 3..) => r.modifiers.push((f[1].to_string(), num(2, 1.0))),
                ("standing", 2..) => standing = Some(f[1].to_string()),
                ("blown", 3..) => {
                    blown = Some(f[1].to_string());
                    r.blown_secs = num(2, 0.0);
                }
                ("held", 3..) => r.held.push((Arc::from(f[1]), vec3(f[2]))),
                ("dsys", _) => {
                    dsys.push_str(line);
                    dsys.push('\n');
                }
                _ => {}
            }
        }
        // Its states' effects, as deaths.txt's destructibles (deaths.rs); the blown
        // state's blast its own (barrels.rs's, not gibs.rs's: its instigator's).
        let states = deaths::DeathData::parse(&dsys).destructibles;
        let state = |name: &Option<String>| {
            name.as_ref()
                .and_then(|n| states.get(n))
                .cloned()
                .unwrap_or_default()
        };
        out.rules.standing = state(&standing);
        let mut pieces = state(&blown).pieces.clone();
        pieces.retain(|p| match p {
            Piece::Damage {
                damage,
                radius,
                momentum,
                at,
                ..
            } => {
                out.rules.blast = Some(Blast {
                    damage: *damage,
                    radius: radius / UU,
                    momentum: *momentum,
                    at: *at,
                });
                false
            }
            _ => true,
        });
        out.rules.blown = Arc::new(Destructible { pieces });
        out
    }

    /// A map's barrels (none if it has no barrels file), their collision groups put
    /// into `collision` (the standing barrels' on, the wrecks' off).
    pub fn load(
        dir: &Path,
        stem: &str,
        collision: LevelCollision,
    ) -> (BarrelsData, LevelCollision) {
        let Ok(text) = std::fs::read_to_string(dir.join(format!("levels/{stem}_barrels.txt")))
        else {
            return (BarrelsData::default(), collision);
        };
        let mut data = BarrelsData::parse(&text);
        let groups = match crate::collision::load_groups(
            &dir.join(format!("levels/{stem}_barrels.bin")),
        ) {
            Ok(g) => g,
            Err(e) => {
                warn!(
                    "levels/{stem}_barrels.bin can't be read ({e}): the barrels have no collision"
                );
                Vec::new()
            }
        };
        let parts: Vec<(Vec<[f32; 9]>, Blocks)> =
            groups.iter().map(|g| (g.tris.clone(), g.blocks)).collect();
        let (mut collision, ids) = collision.with_groups(&parts);
        for (g, &id) in groups.iter().zip(&ids) {
            let Some((role, n)) = g.name.split_once(':') else {
                continue;
            };
            let Some(spot) = n.parse::<usize>().ok().and_then(|n| data.spots.get_mut(n)) else {
                continue;
            };
            match role {
                "barrel" => {
                    spot.standing = Some(id);
                    if let Some(s) = Shape::of(&g.tris) {
                        spot.shape = s;
                    }
                }
                "wreck" => {
                    spot.wreck = Some(id);
                    collision.set_group(id, false);
                }
                _ => {}
            }
        }
        (data, collision)
    }
}

/// Who did a barrel damage: a player (his Jazz, here or on another PC: he gets its
/// kills) or the AI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    Player(Entity),
    Ai,
}

/// A barrel in play.
#[derive(Clone, Debug, Default)]
struct Live {
    health: f32,
    /// Seconds since it blew up, while its blown state lasts; past it, the wreck.
    blown: Option<f32>,
    wrecked: bool,
    held: Vec<FxId>,
    /// Its model's nodes (the standing barrel's and outline's, the wreck's).
    standing_nodes: Vec<Entity>,
    wreck_nodes: Vec<Entity>,
    shown: Option<bool>,
}

/// A blow on a barrel this frame: the barrel, its damage (its modifiers' in) and
/// who struck it.
#[derive(Clone, Copy, Debug)]
struct Blow {
    barrel: usize,
    damage: f32,
    by: By,
}

/// The map's barrels in play.
#[derive(Resource, Default)]
pub struct Barrels {
    pub data: BarrelsData,
    live: Vec<Live>,
    blows: Vec<Blow>,
    /// The other PCs' word of barrels blown up: (the player whose PC said, the
    /// barrel, whether his PC's AI did it).
    inbox: Vec<(Entity, usize, bool)>,
    started: bool,
    seed: u32,
}

impl Barrels {
    pub fn new(data: BarrelsData) -> Barrels {
        let mut b = Barrels { data, ..default() };
        b.restart();
        b
    }

    /// A new match: the barrels standing.
    fn restart(&mut self) {
        let nodes: Vec<_> = self
            .live
            .drain(..)
            .map(|l| (l.standing_nodes, l.wreck_nodes))
            .collect();
        self.live = (0..self.data.spots.len())
            .map(|_| Live {
                health: self.data.rules.health,
                ..default()
            })
            .collect();
        for (l, (s, w)) in self.live.iter_mut().zip(nodes) {
            l.standing_nodes = s;
            l.wreck_nodes = w;
        }
        self.blows.clear();
        self.inbox.clear();
        self.started = false;
    }

    /// Whether barrel `i` stands.
    pub fn standing(&self, i: usize) -> bool {
        self.live
            .get(i)
            .is_some_and(|l| l.blown.is_none() && !l.wrecked)
    }

    /// The standing barrels' numbers.
    pub fn standing_ones(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.live.len()).filter(|&i| self.standing(i))
    }

    /// The standing barrel a round that stopped at `p` struck.
    pub fn struck_at(&self, p: Vec3) -> Option<usize> {
        self.standing_ones()
            .find(|&i| self.data.spots[i].shape.struck_at(p))
    }

    /// How far `p` is from standing barrel `i` (0 inside it).
    pub fn reach(&self, i: usize, p: Vec3) -> f32 {
        self.data.spots[i].shape.reach(p)
    }

    /// Its damage to a barrel, its modifiers' (HmDestructibleComponent: the
    /// blueprint's DamageModifiers by damage type, and an AI's AIDamageModifier).
    fn scaled(&self, damage: f32, kind: &str, by: By) -> f32 {
        let r = &self.data.rules;
        let typed = r
            .modifiers
            .iter()
            .find(|(k, _)| k == kind)
            .map_or(1.0, |(_, s)| *s);
        let ai = if by == By::Ai { r.ai_scale } else { 1.0 };
        damage * typed * ai
    }

    /// A blow (a round's, a punch's...) of `damage` of `kind` on standing barrel
    /// `i`.
    pub fn strike(&mut self, i: usize, damage: f32, kind: &str, by: By) {
        if damage > 0.0 && self.standing(i) {
            let damage = self.scaled(damage, kind, by);
            self.blows.push(Blow {
                barrel: i,
                damage,
                by,
            });
        }
    }

    /// A round that stopped at `p`: the barrel it struck takes its damage. Whether
    /// it struck one.
    pub fn shot(&mut self, p: Vec3, damage: f32, kind: &str, by: By) -> bool {
        match self.struck_at(p) {
            Some(i) => {
                self.strike(i, damage, kind, by);
                true
            }
            None => false,
        }
    }

    /// A blast at `centre` reaching `radius` (metres): each standing barrel in
    /// reach takes `damage(reach)` (TakeRadiusDamage), a quarter of it should the
    /// blast's middle be more than RadiusDamageModifier's distance from it.
    pub fn blast(
        &mut self,
        centre: Vec3,
        radius: f32,
        damage: impl Fn(f32) -> f32,
        kind: &str,
        by: By,
    ) {
        let r = &self.data.rules;
        let (far_scale, near) = (r.radius_scale, r.radius_near);
        let hits: Vec<(usize, f32)> = self
            .standing_ones()
            .filter_map(|i| {
                let reach = self.reach(i, centre);
                (reach <= radius).then(|| {
                    let far = if reach > near { far_scale } else { 1.0 };
                    (i, damage(reach) * far)
                })
            })
            .collect();
        for (i, d) in hits {
            self.strike(i, d, kind, by);
        }
    }

    /// Another PC's barrel blown up (main.rs's net_apply): `from` the player whose PC
    /// said, `ai` whether its AI did it.
    pub fn heard_blown(&mut self, from: Entity, barrel: u8, ai: bool) {
        self.inbox.push((from, barrel as usize, ai));
    }
}

/// A barrel's model, its number in the map's list.
#[derive(Component)]
pub struct Barrel {
    pub index: usize,
}

/// The barrel model's textures sampled as the map's are (main.rs).
fn map_sampler(settings: &mut GltfLoaderSettings) {
    settings.default_sampler = Some(crate::graphics::map_sampler());
}

/// The map's barrels, as it comes in (main.rs): their models.
pub fn spawn(
    commands: &mut Commands,
    asset_server: &AssetServer,
    data: &BarrelsData,
) -> Vec<Entity> {
    if data.spots.is_empty() || !crate::assets_dir().join(MODEL).exists() {
        return Vec::new();
    }
    let file: Handle<Gltf> = asset_server.load_with_settings(MODEL, map_sampler);
    data.spots
        .iter()
        .enumerate()
        .map(|(index, s)| {
            commands
                .spawn((
                    Barrel { index },
                    Name::new(s.id.clone()),
                    SceneOf {
                        file: file.clone(),
                        scene: 0,
                    },
                    Transform {
                        translation: s.at,
                        rotation: s.turn,
                        scale: Vec3::splat(s.scale),
                    },
                    Visibility::default(),
                ))
                .id()
        })
        .collect()
}

/// The barrels' systems.
pub struct BarrelsPlugin;

impl Plugin for BarrelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Barrels>()
            .add_systems(OnEnter(crate::menu::AppState::Game), reset)
            .add_systems(Update, find_nodes)
            .add_systems(
                Update,
                (
                    run_barrels
                        .after(crate::net_apply)
                        .after(crate::fire)
                        .after(crate::melee_hits)
                        .after(crate::melee_sweeps)
                        .before(crate::sound::PlaySounds),
                    show_barrels.after(run_barrels).after(find_nodes),
                )
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

/// A new match: the barrels stand again (their effects gone, their collision as
/// it was).
fn reset(
    mut barrels: ResMut<Barrels>,
    mut level: ResMut<Level>,
    mut fx: Option<ResMut<FxSystems>>,
) {
    for l in &mut barrels.live {
        for id in l.held.drain(..) {
            if let Some(fx) = fx.as_mut() {
                fx.kill(id);
            }
        }
    }
    barrels.restart();
    if let Some(c) = level.collision.as_mut() {
        for s in &barrels.data.spots {
            if let Some(g) = s.standing {
                c.set_group(g, true);
            }
            if let Some(g) = s.wreck {
                c.set_group(g, false);
            }
        }
    }
}

/// The barrels' nodes as their scenes come in (the standing barrel's and its
/// outline's, the wreck's), the outline cast no shadow.
fn find_nodes(
    mut commands: Commands,
    mut barrels: ResMut<Barrels>,
    added: Query<(Entity, &Name), Added<Name>>,
    parents: Query<&ChildOf>,
    owners: Query<&Barrel>,
) {
    for (e, name) in &added {
        let role = name.as_str();
        if role != BARREL_NODE && role != OUTLINE_NODE && role != WRECK_NODE {
            continue;
        }
        let Some(b) = parents.iter_ancestors(e).find_map(|a| owners.get(a).ok()) else {
            continue;
        };
        let Some(l) = barrels.live.get_mut(b.index) else {
            continue;
        };
        // (Those of a scene instance since gone, gone.)
        l.standing_nodes.retain(|n| parents.contains(*n));
        l.wreck_nodes.retain(|n| parents.contains(*n));
        if role == WRECK_NODE {
            l.wreck_nodes.push(e);
        } else {
            l.standing_nodes.push(e);
        }
        if role == OUTLINE_NODE {
            commands
                .entity(e)
                .insert((bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
        }
        l.shown = None;
    }
}

/// The barrels standing or wrecked, as they are.
fn show_barrels(mut barrels: ResMut<Barrels>, mut vis: Query<&mut Visibility>) {
    for l in &mut barrels.live {
        let standing = l.blown.is_none() && !l.wrecked;
        if l.shown == Some(standing) {
            continue;
        }
        l.shown = Some(standing);
        for (nodes, on) in [(&l.standing_nodes, standing), (&l.wreck_nodes, !standing)] {
            for &n in nodes {
                if let Ok(mut v) = vis.get_mut(n) {
                    *v = if on {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                }
            }
        }
    }
}

/// The blows on the barrels this frame (and the other PCs' word of them blowing
/// up): those whose health runs out blow up, as their destroyer's; the blown ones'
/// fires burn out.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn run_barrels(
    time: Res<Time>,
    mut barrels: ResMut<Barrels>,
    mut level: ResMut<Level>,
    mut fx: Option<ResMut<FxSystems>>,
    mut sfx: ResMut<Sfx>,
    mut gibs: ResMut<crate::gibs::Gibs>,
    mut players: Query<(Entity, &Transform, &mut Player, Has<LocalPlayer>), Without<RemotePlayer>>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
) {
    let barrels = &mut *barrels;
    if barrels.live.is_empty() {
        return;
    }
    // Each match's start: their hum.
    if !barrels.started {
        barrels.started = true;
        barrels.seed = (time.elapsed_secs() * 1000.0) as u32 ^ 0x9e37_79b9;
        for i in 0..barrels.live.len() {
            let place = barrels.data.spots[i].place();
            let standing = barrels.data.rules.standing.clone();
            crate::gibs::set_off(
                &standing,
                &place,
                &mut barrels.seed,
                true,
                &mut fx,
                &mut sfx,
                &mut gibs,
            );
        }
    }
    // The blows, and the other PCs' word.
    let mut blown: Vec<(usize, By, bool)> = Vec::new();
    for b in std::mem::take(&mut barrels.blows) {
        let Some(l) = barrels.live.get_mut(b.barrel) else {
            continue;
        };
        if l.blown.is_some() || l.wrecked || l.health <= 0.0 {
            continue;
        }
        l.health -= b.damage;
        if l.health <= 0.0 {
            blown.push((b.barrel, b.by, true));
        }
    }
    for (from, i, ai) in std::mem::take(&mut barrels.inbox) {
        if let Some(l) = barrels.live.get_mut(i)
            && l.blown.is_none()
            && !l.wrecked
        {
            l.health = 0.0;
            let by = if ai { By::Ai } else { By::Player(from) };
            blown.push((i, by, false));
        }
    }
    let local_ones: Vec<Entity> = players
        .iter()
        .filter(|(_, _, _, local)| *local)
        .map(|(e, ..)| e)
        .collect();
    for (i, by, here) in blown {
        if barrels.live[i].blown.is_some() {
            continue;
        }
        // (Said to the other PCs by the PC whose player, or AI, did it.)
        let teller = match by {
            By::Player(e) if here && local_ones.contains(&e) => Some((e, false)),
            By::Ai if here => local_ones.first().map(|&e| (e, true)),
            _ => None,
        };
        if let Some((e, ai)) = teller
            && let Ok((_, _, mut p, _)) = players.get_mut(e)
        {
            p.played.push(JazzEvent::BarrelBlown {
                barrel: i as u8,
                ai,
            });
        }
        blow_up(
            barrels,
            i,
            by,
            &mut level,
            (&mut fx, &mut sfx, &mut gibs),
            &mut players,
            &mut enemies,
        );
    }
    // The blown state's end: its fire out, the wreck left.
    let dt = time.delta_secs();
    let secs = barrels.data.rules.blown_secs;
    for l in &mut barrels.live {
        let Some(t) = l.blown.as_mut() else {
            continue;
        };
        *t += dt;
        if *t >= secs {
            l.blown = None;
            l.wrecked = true;
            for id in l.held.drain(..) {
                if let Some(fx) = fx.as_mut() {
                    fx.stop(id);
                }
            }
        }
    }
}

/// Barrel `i` blows up (its blown state): the wreck for it, and its collision; its
/// effects, debris and fire; its blast, as `by`'s.
#[allow(clippy::type_complexity)]
fn blow_up(
    barrels: &mut Barrels,
    i: usize,
    by: By,
    level: &mut Level,
    (fx, sfx, gibs): (
        &mut Option<ResMut<FxSystems>>,
        &mut Sfx,
        &mut crate::gibs::Gibs,
    ),
    players: &mut Query<(Entity, &Transform, &mut Player, Has<LocalPlayer>), Without<RemotePlayer>>,
    enemies: &mut Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
) {
    let spot = barrels.data.spots[i].clone();
    let rules = &barrels.data.rules;
    let place = spot.place();
    info!("{}: blown up ({by:?}'s)", spot.id);
    barrels.live[i].blown = Some(0.0);
    barrels.live[i].health = 0.0;
    crate::gibs::set_off(&rules.blown, &place, &mut barrels.seed, true, fx, sfx, gibs);
    let forward = place.dir(Vec3::X);
    for (system, at) in &rules.held {
        if let Some(fx) = fx.as_mut() {
            let p = place.point(*at);
            if let Some(id) = fx.spawn(system, p, forward, &[]) {
                fx.place_turned(id, p, forward, place.dir(Vec3::Y));
                barrels.live[i].held.push(id);
            }
        }
    }
    // Its blast, from where it stood (seen past neither the barrel nor its wreck).
    if let Some(c) = level.collision.as_mut() {
        for g in [spot.standing, spot.wreck].into_iter().flatten() {
            c.set_group(g, false);
        }
    }
    if let Some(b) = rules.blast {
        let centre = place.point(b.at);
        let hit = |at: Vec3| {
            Hit::new(
                crate::enemies::death_of_damage_type(BLAST_TYPE),
                at,
                at - centre,
            )
            .with_momentum(b.momentum)
            .of_type(BLAST_TYPE)
        };
        match by {
            // (A player's: the enemies'.)
            By::Player(owner) => {
                for (_, _, mut e) in enemies.iter_mut() {
                    let at = e.centre();
                    if !e.alive() || e.reach(centre) > b.radius || level.blocked(centre, at) {
                        continue;
                    }
                    e.take(b.damage, Some(owner), crate::xp::Blow::Weapon, hit(at));
                }
            }
            // (An enemy's: the players at this PC.)
            By::Ai => {
                for (_, t, mut p, local) in players.iter_mut() {
                    let (body, _) = p.size();
                    let mid = p.robot_centre(t.translation);
                    let reach = (mid.distance(centre) - body).max(0.0);
                    if reach > b.radius || level.blocked(centre, mid) {
                        continue;
                    }
                    if crate::damage_player(&mut p, b.damage, hit(mid)) {
                        sfx.play_from(crate::SND_PLAYER_HIT, crate::voice_at(local, t.translation));
                        if local {
                            p.hud_events.push(HudEvent::Damaged {
                                yaw: crate::heading(centre - t.translation),
                                amount: b.damage / crate::HIT_INDICATOR_HEALTH,
                            });
                        }
                        // Its push (Pawn.TakeDamage: bExtraMomentumZ lifting a
                        // walking pawn by at least 0.4 of it, over his Mass).
                        let mut m = (mid - centre).normalize_or(Vec3::Y) * b.momentum;
                        if p.grounded {
                            m.y = m.y.max(0.4 * m.length());
                        }
                        crate::knock(&mut p, m / (PLAYER_MASS * UU));
                    }
                }
            }
        }
    }
    if let Some(c) = level.collision.as_mut()
        && let Some(g) = spot.wreck
    {
        c.set_group(g, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\
# a test
barrel\tB_0\t1 0 2\t0 0 0 1\t0.7
aidamage\t0.5
radiusdamage\t0.25\t100
modifier\tTnDamageTypeCorrosion\t5
health\t25
standing\tState1
blown\tState2\t10
held\tFire_ExpBarrel_FX\t0 16 96
dsys\tState1\tsound\tBL_LVL_06_MEGATRON.EMIT_AMB_MACH_OSC_07
dsys\tState2\tparticle\tExp_GenericLvl_MED_FX\t0 0 176
dsys\tState2\tdamage\tTnDamageTypeBruticusFire\t150\t1500\t125000\t0 0 193
dsys\tState2\tsound\tBL_DEST_ELEC.DEST_ENERGON_TANK_01_EXPLO
";

    fn barrels() -> Barrels {
        Barrels::new(BarrelsData::parse(FILE))
    }

    #[test]
    fn its_file_is_read() {
        let d = BarrelsData::parse(FILE);
        assert_eq!(d.spots.len(), 1);
        assert_eq!(d.spots[0].at, Vec3::new(1.0, 0.0, 2.0));
        let r = &d.rules;
        assert_eq!((r.health, r.ai_scale, r.radius_scale), (25.0, 0.5, 0.25));
        assert!((r.radius_near - 1.0).abs() < 1e-6);
        assert_eq!(r.blown_secs, 10.0);
        assert_eq!(r.held.len(), 1);
        assert_eq!(r.standing.pieces.len(), 1);
        // (Its blast is taken out of its effects.)
        assert_eq!(r.blown.pieces.len(), 2);
        let b = r.blast.unwrap();
        assert_eq!((b.damage, b.radius, b.momentum), (150.0, 15.0, 125000.0));
        assert_eq!(b.at, Vec3::new(0.0, 0.0, 193.0));
    }

    #[test]
    fn its_effects_are_placed_in_its_frame() {
        let d = BarrelsData::parse(FILE);
        let place = d.spots[0].place();
        // (Unreal's x, y, z: the map's x, z, y.)
        let p = place.point(Vec3::new(0.0, 16.0, 96.0));
        assert!(p.distance(Vec3::new(1.0, 0.96, 2.16)) < 1e-4, "{p}");
        let x = place.dir(Vec3::X);
        assert!(x.distance(Vec3::X) < 1e-5, "{x}");
    }

    #[test]
    fn rounds_strike_it_but_not_the_floor_beside_it() {
        let b = barrels();
        // Its side, half way up.
        assert_eq!(b.struck_at(Vec3::new(1.0 + RADIUS, 1.5, 2.0)), Some(0));
        // Its top.
        assert_eq!(b.struck_at(Vec3::new(1.2, HEIGHT, 2.0)), Some(0));
        // The floor by its foot, and a wall a metre off.
        assert_eq!(b.struck_at(Vec3::new(1.0 + RADIUS - 0.2, 0.0, 2.0)), None);
        assert_eq!(b.struck_at(Vec3::new(1.0 + RADIUS + 1.0, 1.5, 2.0)), None);
    }

    #[test]
    fn its_health_takes_the_blows_as_its_blueprint_says() {
        let mut b = barrels();
        b.strike(0, 10.0, "TnDamageTypeAssaultRifle", By::Ai);
        assert_eq!(b.blows[0].damage, 5.0);
        b.strike(
            0,
            2.0,
            "TnDamageTypeCorrosion",
            By::Player(Entity::PLACEHOLDER),
        );
        assert_eq!(b.blows[1].damage, 10.0);
        b.blows.clear();
        // A blast within a metre of it whole, one further off a quarter.
        let side = Vec3::new(1.0 + RADIUS, 1.5, 2.0);
        b.blast(
            side + Vec3::X * 0.5,
            6.0,
            |_| 100.0,
            "TnDamageTypeBazooka",
            By::Ai,
        );
        b.blast(
            side + Vec3::X * 3.0,
            6.0,
            |_| 100.0,
            "TnDamageTypeBazooka",
            By::Ai,
        );
        b.blast(
            side + Vec3::X * 7.0,
            6.0,
            |_| 100.0,
            "TnDamageTypeBazooka",
            By::Ai,
        );
        let d: Vec<f32> = b.blows.iter().map(|x| x.damage).collect();
        assert_eq!(d, vec![50.0, 12.5]);
    }

    /// A barrel on a floor, in an app with what blowing it up needs.
    fn app_with_barrel() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).insert_resource(
            bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_millis(100)),
        );
        let floor = vec![
            [-50.0, 0.0, -50.0, 50.0, 0.0, -50.0, 50.0, 0.0, 50.0],
            [-50.0, 0.0, -50.0, 50.0, 0.0, 50.0, -50.0, 0.0, 50.0],
        ];
        let collision = LevelCollision::from_parts(&floor, &[], 4.0);
        // (Its standing barrel a box round it, the wreck a lower one.)
        let block = |h: f32| -> Vec<[f32; 9]> {
            let (a, c) = (
                Vec3::new(1.0 - RADIUS, 0.0, 2.0 - RADIUS),
                Vec3::new(1.0 + RADIUS, h, 2.0 + RADIUS),
            );
            vec![
                [a.x, a.y, a.z, c.x, a.y, a.z, c.x, c.y, a.z],
                [a.x, a.y, a.z, c.x, c.y, a.z, a.x, c.y, a.z],
            ]
        };
        let (collision, ids) =
            collision.with_groups(&[(block(HEIGHT), Blocks::Solid), (block(1.8), Blocks::Solid)]);
        let mut data = BarrelsData::parse(FILE);
        data.spots[0].standing = Some(ids[0]);
        data.spots[0].wreck = Some(ids[1]);
        app.insert_resource(Level {
            collision: Some(collision),
            ..default()
        });
        app.insert_resource(Barrels::new(data));
        app.init_resource::<crate::gibs::Gibs>();
        app.init_resource::<crate::hud::HudState>();
        app.insert_resource(Sfx::default());
        let player = app
            .world_mut()
            .spawn((
                Transform::from_xyz(1.0, 0.0, 2.0 - 8.0),
                crate::new_player(),
                LocalPlayer,
            ))
            .id();
        app.add_systems(Update, run_barrels);
        (app, player)
    }

    #[test]
    fn an_enemys_blows_blow_it_up_on_the_players_and_say_so() {
        let (mut app, player) = app_with_barrel();
        app.update();
        let health = app.world().get::<Player>(player).unwrap().health;
        // An enemy's round at half: 24 doesn't do it, 2 more does.
        app.world_mut().resource_mut::<Barrels>().shot(
            Vec3::new(1.0, 1.5, 2.0 - RADIUS),
            48.0,
            "TnDamageTypeAssaultRifle",
            By::Ai,
        );
        app.update();
        assert!(app.world().resource::<Barrels>().standing(0));
        app.world_mut().resource_mut::<Barrels>().shot(
            Vec3::new(1.0, 1.5, 2.0 - RADIUS),
            4.0,
            "TnDamageTypeAssaultRifle",
            By::Ai,
        );
        app.update();
        let b = app.world().resource::<Barrels>();
        assert!(!b.standing(0));
        // Its blast took its 150 off the player 8 m off (all of it at any
        // distance), and his PC said so.
        let p = app.world().get::<Player>(player).unwrap();
        assert!(p.health < health, "{} {}", p.health, health);
        assert!(p.played.iter().any(|e| matches!(
            e,
            JazzEvent::BarrelBlown {
                barrel: 0,
                ai: true
            }
        )));
        // The wreck's collision for the barrel's.
        let level = app.world().resource::<Level>();
        let c = level.collision.as_ref().unwrap();
        let s = &b.data.spots[0];
        assert!(!c.group_on(s.standing.unwrap()));
        assert!(c.group_on(s.wreck.unwrap()));
        // No more blows on it.
        assert_eq!(b.struck_at(Vec3::new(1.0, 1.5, 2.0 - RADIUS)), None);
    }

    #[test]
    fn a_players_blast_spares_the_players_and_its_wreck_is_left_for_good() {
        let (mut app, player) = app_with_barrel();
        app.update();
        let health = app.world().get::<Player>(player).unwrap().health;
        app.world_mut().resource_mut::<Barrels>().strike(
            0,
            25.0,
            "TnDamageTypeAssaultRifle",
            By::Player(player),
        );
        app.update();
        let p = app.world().get::<Player>(player).unwrap();
        assert_eq!(p.health, health);
        assert!(p.played.iter().any(|e| matches!(
            e,
            JazzEvent::BarrelBlown {
                barrel: 0,
                ai: false
            }
        )));
        // Its blown state's 10 s (in steps of 0.1 s), then the wreck alone.
        for _ in 0..95 {
            app.update();
        }
        assert!(app.world().resource::<Barrels>().live[0].blown.is_some());
        for _ in 0..10 {
            app.update();
        }
        let b = app.world().resource::<Barrels>();
        assert!(b.live[0].wrecked && b.live[0].blown.is_none());
        assert!(!b.standing(0));
    }

    #[test]
    fn another_pcs_word_blows_it_up_here_as_its_destroyers() {
        let (mut app, player) = app_with_barrel();
        app.update();
        let health = app.world().get::<Player>(player).unwrap().health;
        let remote = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<Barrels>()
            .heard_blown(remote, 0, true);
        app.update();
        let p = app.world().get::<Player>(player).unwrap();
        assert!(p.health < health);
        // (Not said again from here.)
        assert!(
            !p.played
                .iter()
                .any(|e| matches!(e, JazzEvent::BarrelBlown { .. }))
        );
        assert!(!app.world().resource::<Barrels>().standing(0));
    }

    /// Oblivion's own: on its floor, struck by a round from the start's side, solid
    /// until it blows up and then its wreck, lower; the enemies' way round it.
    #[test]
    fn oblivions_barrel_stands_on_its_floor_by_the_start() {
        let dir = crate::assets_dir();
        let Ok(collision) = LevelCollision::load(&dir.join("levels/oblivion_col.bin")) else {
            return;
        };
        let (data, mut c) = BarrelsData::load(&dir, "oblivion", collision);
        assert_eq!(data.spots.len(), 1);
        // (Its blueprint: its hum, its explosion's effects, debris and blast, its
        // fire for 10 s.)
        let r = &data.rules;
        assert_eq!(r.health, 25.0);
        assert_eq!(r.standing.pieces.len(), 1);
        assert_eq!(r.blown.pieces.len(), 7);
        assert_eq!(r.blown_secs, 10.0);
        assert_eq!(r.held.len(), 1);
        assert!(
            r.blast
                .is_some_and(|b| b.damage == 150.0 && b.radius == 15.0)
        );
        let s = &data.spots[0];
        let sh = s.shape;
        assert!((sh.radius - 1.26).abs() < 0.05, "{sh:?}");
        assert!((sh.top - sh.bottom - 3.33).abs() < 0.05, "{sh:?}");
        let (standing, wreck) = (s.standing.unwrap(), s.wreck.unwrap());
        assert!(c.group_on(standing) && !c.group_on(wreck));
        // (Its foot on the floor.)
        c.set_group(standing, false);
        let floor = c.ground(sh.axis.x, sh.axis.y, sh.bottom + 1.0, sh.bottom - 2.0);
        assert!(
            floor.is_some_and(|y| (y - sh.bottom).abs() < 0.15),
            "{floor:?} {sh:?}"
        );
        c.set_group(standing, true);
        // A round from 10 m towards the start, at 1.5 m, stops on it.
        let from = Vec3::new(sh.axis.x, sh.bottom + 1.5, sh.axis.y - 10.0);
        let t = c
            .raycast_hit(from.to_array(), [0.0, 0.0, 1.0], 20.0)
            .map(|h| h.0);
        let b = Barrels::new(data.clone());
        let hit = t.map(|t| from + Vec3::Z * t);
        assert_eq!(hit.and_then(|p| b.struck_at(p)), Some(0), "{hit:?}");
        // (The enemies' navigation goes round it, as the game's was built with it
        // there.)
        if let Some(nav) = crate::nav::NavMesh::load(&dir, "oblivion") {
            let foot = Vec3::new(sh.axis.x, sh.bottom, sh.axis.y);
            assert_eq!(nav.cell_at(foot), None);
            assert!(nav.cell_at(foot + Vec3::X * 3.0).is_some());
        }
        // Blown up, the wreck stands lower: a round over it passes.
        c.set_group(standing, false);
        c.set_group(wreck, true);
        let over = from + Vec3::Y * 1.0;
        let t2 = c
            .raycast_hit(over.to_array(), [0.0, 0.0, 1.0], 12.0)
            .map(|h| h.0);
        assert!(t2.is_none_or(|t| t > 11.0), "{t2:?}");
        let t3 = c
            .raycast_hit(from.to_array(), [0.0, 0.0, 1.0], 20.0)
            .map(|h| h.0);
        assert!(t3.is_some_and(|t| t < 11.0), "{t3:?}");
    }
}
