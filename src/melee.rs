//! The multiplayer melee (tools/build_melee.py reads it from the game): each class's
//! sword swing, the Ion Blade it pulls out of the gun for it, and its blow.
//!
//! In multiplayer a melee is the class's melee set's sword swing (MELEE_WeaponAttack:
//! Melee_Sword_Infiltrator and the others, main.rs's ANIM_MELEE, the character's
//! robot_clips.glb clip), played at its own rate. As it starts (its
//! TnAnimNotify_WeaponMelee) the gun puts on its melee attachment, the Ion Blade
//! (WEP_IonBlade_SKEL: models/weapons/ionblade.glb), on its MeleeSocket - every gun
//! has one, on the elbow under it, placed and sized for the gun - and the blade's
//! pieces slide out of its housing (IonBlade_Equip); towards the swing's end they slide
//! back in (IonBlade_UnEquip) and it comes off as the swing ends. The blow is a sweep
//! (TnAnimNotify_DamageSweep, a TnTraceSweeper): a box ahead of the robot, at its
//! MeleeSocket_SmallRobot (on its root bone, at the height of its chest) and the
//! sweep's Offset turned as the robot faces (to its right, or to its left as it holds
//! its gun in the left hand: TnTraceSweeper.AddOffset), striking each enemy it meets
//! once over the sweep's Duration (main.rs's melee_sweeps). What it does is
//! Escalation's: its melee set's sword attack's 75 damage and the push that knocks
//! a soldier off its feet (main.rs's MELEE_DAMAGE and MELEE_MOMENTUM), not
//! multiplayer's 135 and nudge.
//!
//! The blade is drawn in its two materials: its housing and blade in the character's
//! paint on the rocket launcher's textures (WEP_IonBlade_MATINST, painted as the guns
//! are: character.rs), and a strip along its edge glowing red and orange with pulses
//! running through its circuitry (WEP_BLadeAut_MAT: [`BladeGlowMaterial`],
//! assets/shaders/ionblade.wgsl).

use std::path::Path;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::gltf::{Gltf, GltfMesh, GltfNode, GltfSkin};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{
    AsBindGroup, Face, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

use crate::character::{Characters, Look, PaintCache, PaintMaterial};

/// tools/build_melee.py's files.
const MELEE_FILE: &str = "models/weapons/melee.txt";
const BLADE_MODEL: &str = "models/weapons/ionblade.glb";
const BLADE_MASK: &str = "models/weapons/ionblade_mask.ktx2";
const GLOW_SHADER: &str = "shaders/ionblade.wgsl";
/// The blade's housing's material in robot.glb: the rocket launcher's (its paint's
/// texture set is WEP_IonBlade_MATINST's, WEP_RocketLauncher_p's).
const BODY_MATERIAL: &str = "WEP_RocketLauncher_MATINST";
/// The edge's material in ionblade.glb.
const GLOW_MATERIAL: &str = "WEP_BLadeAut_MAT";
/// The bones the guns' MeleeSockets are on (main.rs's GunBone).
const ELBOWS: [&str; 2] = ["R_Arm03_Elbow_XB", "L_Arm03_Elbow_XB"];
/// How far a blade's clock may drift from its swing's before it is set to it (another
/// player's swing comes with his snapshots; a new swing starts it over).
const RESYNC_SECS: f32 = 0.15;

pub struct MeleePlugin;

impl Plugin for MeleePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MeleeData::load(&crate::assets_dir()))
            .init_resource::<BladeAssets>()
            .add_plugins(MaterialPlugin::<BladeGlowMaterial>::default())
            .add_systems(Startup, load_blade)
            .add_systems(Update, (spawn_blades, fog_glow));
    }
}

// ---------------------------------------------------------------------------
// The data
// ---------------------------------------------------------------------------

/// A class's sword swing (melee.txt's swing line).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swing {
    /// Its clip's length (SequenceLength), seconds.
    pub length: f32,
    /// When the blade comes out (TnAnimNotify_WeaponMelee); None for a swing without
    /// it (the Destroyer's and the Titan's: no blade).
    pub blade_at: Option<f32>,
    /// When the blow's sweep starts and how long it lasts.
    pub sweep_at: f32,
    pub sweep_secs: f32,
    /// The sweep's socket in robot.glb's space (before the class's robot scale), and
    /// the offset from it and the box's half sizes in the robot's own axes (x to its
    /// right, y up, z behind it); metres.
    pub socket: Vec3,
    pub offset: Vec3,
    pub half: Vec3,
}

impl Swing {
    /// Without melee.txt: the Destroyer's (no blade).
    pub const DEFAULT: Swing = Swing {
        length: 1.2667,
        blade_at: None,
        sweep_at: 0.1474,
        sweep_secs: 0.2,
        socket: Vec3::new(0.45, 4.3, -5.5),
        offset: Vec3::new(2.0, 0.0, 0.0),
        half: Vec3::new(3.0, 4.0, 3.0),
    };

    /// Where the blow comes from for a robot standing at `feet` turned `rotation`, its
    /// meshes at `scale`: its socket.
    pub fn origin(&self, feet: Vec3, rotation: Quat, scale: f32) -> Vec3 {
        feet + rotation * (self.socket * scale)
    }

    /// The sweep's box for that robot, with his gun in his left hand or not.
    pub fn sweep_box(&self, feet: Vec3, rotation: Quat, scale: f32, left: bool) -> SweepBox {
        let side = if left { -1.0 } else { 1.0 };
        let offset = Vec3::new(self.offset.x * side, self.offset.y, self.offset.z);
        SweepBox {
            centre: self.origin(feet, rotation, scale) + rotation * offset,
            rotation,
            half: self.half,
        }
    }
}

/// A sweep's box: its centre, its turn and its half sizes along its axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepBox {
    pub centre: Vec3,
    pub rotation: Quat,
    pub half: Vec3,
}

impl SweepBox {
    /// The box's point nearest `p`.
    pub fn nearest(&self, p: Vec3) -> Vec3 {
        let local = self.rotation.inverse() * (p - self.centre);
        self.centre + self.rotation * local.clamp(-self.half, self.half)
    }
}

/// A Jazz's sword swing's blow as it goes (Player::melee_sweep): whether it has begun
/// (a local Jazz's: told to the other PCs), another player's as his PC told it (its
/// box's centre, his heading and the seconds since), and the enemies it has struck
/// (each once).
#[derive(Clone, Debug, Default)]
pub struct Sweep {
    pub begun: bool,
    pub given: Option<(Vec3, f32, f32)>,
    pub struck: Vec<Entity>,
    /// The exploding barrels it has struck (barrels.rs: their numbers).
    pub barrels: Vec<usize>,
}

/// The sword swing's lunge (Escalation's melee set, TR_Shared_MS_p.SharedMeleeSet,
/// every Escalation character's: its MELEE_WeaponAttack, Melee_EnergonSword_01, has
/// AllowFlight and an AttackDash of Speed 2500 and Time 0.25; multiplayer's sets'
/// have none). As every swing begins (TnMeleeManager.PlayMeleeAttack's StartFlight)
/// he flies (TnFlier: PHYS_Flying, its air speed the dash's Speed) for 0.25 s, at
/// full speed at once (its FlightAccelRate, 1000000): at the enemy the melee assist
/// picked under his crosshair, or with none straight ahead, the way he faces as he
/// swings (the flight's Direction, his yaw: TnFlier.Update flies along it when it
/// has no Target). At an enemy he stops dead as he reaches it (the attack's
/// DisableRootMotionAfterContact: TnMeleeManager.Attacking.OnUpdate's
/// DisableMotionOnContact once it is less than their two collision radii ahead of
/// him), or as his blow lands (ProcessMeleeHit). His air speed is the dash's times
/// the swing's GroundSpeedMultiplier, 0.75 (set after it: SetSpeedMultiplier scales
/// the air speed too), and his other speed multipliers.
pub const DASH_SPEED: f32 = 25.0;
pub const DASH_SECS: f32 = 0.25;
pub const SWING_SPEED_MULTIPLIER: f32 = 0.75;
/// How far an enemy may be for the melee assist to pick it
/// (TnTargetPickingTypeMeleeAssist's MeleeAssistRange, 2000).
pub const ASSIST_RANGE: f32 = 20.0;

/// A lunge under way (Player::lunge): the enemy it flies at, if any, where its
/// middle and its collision radius were last (it keeps on to there should the enemy
/// go), the way he faced as he swung (level: where it flies with no enemy), the
/// seconds of its flight left, and whether he has reached its enemy or his blow
/// has landed (stopped dead, still flying: no fall till its time is out,
/// TnFlier.StopFlight).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lunge {
    pub target: Option<Entity>,
    pub toward: Vec3,
    pub radius: f32,
    pub direction: Vec3,
    pub left: f32,
    pub reached: bool,
}

impl Lunge {
    /// A swing's lunge: at `target` if the melee assist picked one, else straight
    /// along `facing` (made level).
    pub fn new(target: Option<AssistTarget>, facing: Vec3) -> Self {
        Self {
            target: target.map(|t| t.entity),
            toward: target.map_or(Vec3::ZERO, |t| t.middle),
            radius: target.map_or(0.0, |t| t.radius),
            direction: Vec3::new(facing.x, 0.0, facing.z).normalize_or_zero(),
            left: DASH_SECS,
            reached: false,
        }
    }
}

/// An enemy as the melee assist sees it: its targeting point (its TargetingSocket),
/// the middle of its collision and its radius (where the lunge flies, and when it
/// has reached it), and its picking radius (assist_radius).
#[derive(Clone, Copy, Debug)]
pub struct AssistTarget {
    pub entity: Entity,
    pub aim: Vec3,
    pub middle: Vec3,
    pub radius: f32,
    pub pick: f32,
}

/// How near the crosshair's line an enemy's targeting point must be for the melee
/// assist to pick it: its targetable component's TnTargetPickingTypeMeleeAssist
/// picker's radius (MinRadiusCm: within MeleeAssistRange it is nearer than the
/// pickers' MinDistance, 25 m). The robots' definitions (every enemy's ROBODEF, the
/// Titan's drones' too) give 375 cm and the Flyers' jets (their VEHDEFs) 550; an
/// insect form's (TnInsectFormBlueprint's, the Sniper's on a wall and the Swarmer's)
/// has no pickers of its own, so TnTargetableComponent's own, 80 cm.
pub fn assist_radius(form: crate::enemies::Form) -> f32 {
    use crate::enemies::Form;
    match form {
        Form::Robot | Form::Orb => 3.75,
        Form::Jet => 5.5,
        Form::Insect => 0.8,
    }
}

/// The enemy the sword swing lunges at, if any (TnPlayerController.PlayerWalking.
/// Melee's PlayerTargeting.GetPickedTarget(TnTargetPickingTypeMeleeAssist)): of those
/// within ASSIST_RANGE of his middle `me` and ahead of the way he faces
/// (TnTargetPickingTypeMeleeAssist.IsEligibleToBePicked), whose targeting point the
/// crosshair's line (`eye` along `view`) passes within its picking radius, ahead of
/// the eye and in its sight (`blocked`: the level in the way from one point to
/// another), the one nearest that line.
pub fn assist_target(
    (eye, view): (Vec3, Vec3),
    (me, facing): (Vec3, Vec3),
    targets: &[AssistTarget],
    blocked: impl Fn(Vec3, Vec3) -> bool,
) -> Option<AssistTarget> {
    let view = view.normalize_or_zero();
    targets
        .iter()
        .filter(|t| t.middle.distance(me) <= ASSIST_RANGE && (t.middle - me).dot(facing) >= 0.0)
        .filter_map(|t| {
            let along = (t.aim - eye).dot(view);
            let off = (t.aim - (eye + view * along)).length();
            (along > 0.0 && off <= t.pick).then_some((t, off))
        })
        .filter(|(t, _)| !blocked(eye, t.aim))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(t, _)| *t)
}

/// The lunge's velocity this frame (TnFlier.Update's pull at its target, or with
/// none along its direction, with his own run added but never back from the way he
/// faces: TnMeleeManager.Attacking.AdjustAcceleration; flying at once at its full
/// `speed`), from his middle `me` facing `facing` with his run `run` (a world
/// direction, or zero); or None once he has reached its enemy (less than their two
/// radii, his `radius` and the target's, ahead of him: then he stops).
pub fn lunge_velocity(
    lunge: &Lunge,
    me: Vec3,
    facing: Vec3,
    run: Vec3,
    radius: f32,
    speed: f32,
) -> Option<Vec3> {
    let flat = Vec3::new(facing.x, 0.0, facing.z).normalize_or_zero();
    let to = match lunge.target {
        Some(_) => {
            let to = lunge.toward - me;
            if to.dot(flat) < lunge.radius + radius {
                return None;
            }
            to
        }
        None => lunge.direction,
    };
    // (Both pulls are full: his run's at his AccelRate, the flier's.)
    let run = run.normalize_or_zero();
    let run = if run.dot(flat) < 0.0 {
        run - flat * run.dot(flat)
    } else {
        run
    };
    Some((to.normalize_or_zero() + run).normalize_or_zero() * speed)
}

/// A bone's keys in one of the blade's clips: seconds, translation, rotation.
pub type Keys = Vec<(f32, Vec3, Quat)>;

/// The Ion Blade's clips (melee.txt's bladeclip and bladekey lines): its length, and
/// each bone it moves with its keys.
#[derive(Clone, Debug, Default)]
pub struct BladeClip {
    pub length: f32,
    pub tracks: Vec<(String, Keys)>,
}

/// melee.txt: each class's swing, the blade's clips and each gun's MeleeSockets.
#[derive(Resource, Default, Debug)]
pub struct MeleeData {
    swings: [Option<Swing>; crate::character::CLASSES.len()],
    equip: BladeClip,
    unequip: BladeClip,
    /// Each gun's (main.rs's WEAPONS) MeleeSocket on its right-arm and left-arm
    /// meshes, on the elbow under it (umodel's glTF axes, metres).
    sockets: Vec<[Option<Transform>; 2]>,
}

fn floats<const N: usize>(s: &str) -> Option<[f32; N]> {
    let v: Vec<f32> = s
        .split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect();
    (v.len() == N).then(|| std::array::from_fn(|i| v[i]))
}

impl MeleeData {
    pub fn load(assets: &Path) -> MeleeData {
        match std::fs::read_to_string(assets.join(MELEE_FILE)) {
            Ok(text) => {
                let data = MeleeData::parse(&text);
                info!(
                    "Melee: {} swings, {} guns' blade sockets",
                    data.swings.iter().flatten().count(),
                    data.sockets.iter().flatten().flatten().count()
                );
                data
            }
            Err(_) => {
                info!("{MELEE_FILE} not found (tools/build_melee.py makes it): no Ion Blade");
                MeleeData::default()
            }
        }
    }

    pub fn parse(text: &str) -> MeleeData {
        let mut data = MeleeData {
            sockets: vec![[None; 2]; crate::WEAPONS.len()],
            ..default()
        };
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            match (f[0], f.len()) {
                ("swing", 9) => {
                    let Some(class) = crate::character::CLASSES.iter().position(|c| *c == f[1])
                    else {
                        continue;
                    };
                    let n = |i: usize| f[i].parse::<f32>().ok();
                    // (A blade at "-": the swing has none.)
                    let blade_at = match f[3] {
                        "-" => Some(None),
                        _ => n(3).map(Some),
                    };
                    let (Some(length), Some(blade_at), Some(sweep_at), Some(sweep_secs)) =
                        (n(2), blade_at, n(4), n(5))
                    else {
                        continue;
                    };
                    let (Some(socket), Some(offset), Some(half)) =
                        (floats::<3>(f[6]), floats::<3>(f[7]), floats::<3>(f[8]))
                    else {
                        continue;
                    };
                    data.swings[class] = Some(Swing {
                        length,
                        blade_at,
                        sweep_at,
                        sweep_secs,
                        socket: Vec3::from(socket),
                        offset: Vec3::from(offset),
                        half: Vec3::from(half).abs(),
                    });
                }
                ("bladeclip", 3) => {
                    let clip = match f[1] {
                        "equip" => &mut data.equip,
                        "unequip" => &mut data.unequip,
                        _ => continue,
                    };
                    clip.length = f[2].parse().unwrap_or(0.0);
                }
                ("bladekey", 6) => {
                    let clip = match f[1] {
                        "equip" => &mut data.equip,
                        "unequip" => &mut data.unequip,
                        _ => continue,
                    };
                    let (Ok(t), Some(p), Some(q)) =
                        (f[3].parse::<f32>(), floats::<3>(f[4]), floats::<4>(f[5]))
                    else {
                        continue;
                    };
                    let key = (t, Vec3::from(p), Quat::from_array(q).normalize());
                    match clip.tracks.iter_mut().find(|(b, _)| b == f[2]) {
                        Some((_, keys)) => keys.push(key),
                        None => clip.tracks.push((f[2].to_string(), vec![key])),
                    }
                }
                ("bladesocket", 6) => {
                    let Some((weapon, left)) =
                        crate::WEAPONS.iter().enumerate().find_map(|(i, w)| {
                            (w.mesh == f[1])
                                .then_some((i, false))
                                .or((w.mesh_left == f[1]).then_some((i, true)))
                        })
                    else {
                        continue;
                    };
                    // (On the elbow the blade hangs from.)
                    if f[2] != ELBOWS[usize::from(left)] {
                        warn!(
                            "{}'s MeleeSocket is on {}, not its elbow: no blade on it",
                            f[1], f[2]
                        );
                        continue;
                    }
                    let (Some(t), Some(q), Some(s)) =
                        (floats::<3>(f[3]), floats::<4>(f[4]), floats::<3>(f[5]))
                    else {
                        continue;
                    };
                    data.sockets[weapon][usize::from(left)] = Some(Transform {
                        translation: Vec3::from(t),
                        rotation: Quat::from_array(q).normalize(),
                        scale: Vec3::from(s),
                    });
                }
                _ => {}
            }
        }
        for clip in [&mut data.equip, &mut data.unequip] {
            for (_, keys) in &mut clip.tracks {
                keys.sort_by(|a, b| a.0.total_cmp(&b.0));
            }
        }
        data
    }

    /// A class's swing (the Destroyer's without melee.txt).
    pub fn swing(&self, class: usize) -> Swing {
        self.swings
            .get(class)
            .copied()
            .flatten()
            .unwrap_or(Swing::DEFAULT)
    }

    /// A gun's MeleeSocket on its right-arm or left-arm mesh.
    pub fn socket(&self, weapon: usize, left: bool) -> Option<Transform> {
        self.sockets.get(weapon)?[usize::from(left)]
    }
}

/// A clip's key at `t` (between its keys, blended; held past its ends).
fn sample(keys: &[(f32, Vec3, Quat)], t: f32) -> Option<(Vec3, Quat)> {
    let first = keys.first()?;
    if t <= first.0 {
        return Some((first.1, first.2));
    }
    for pair in keys.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if t < b.0 {
            let k = ((t - a.0) / (b.0 - a.0).max(1e-6)).clamp(0.0, 1.0);
            return Some((a.1.lerp(b.1, k), a.2.slerp(b.2, k)));
        }
    }
    keys.last().map(|k| (k.1, k.2))
}

/// Where the blade is in its clips: seconds into its equip, out (at rest), or
/// seconds into its unequip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BladePose {
    Equip(f32),
    Out,
    Unequip(f32),
}

/// The blade's pose `t` seconds after it comes out, in a swing `length` long (its
/// clip's SequenceLength): it slides out (its equip), and back in as the swing's
/// length less its unequip's has gone by since it came out (TnWeapon's UnequipTime).
pub fn blade_pose(data: &MeleeData, t: f32, length: f32) -> BladePose {
    let unequip_at = length - data.unequip.length;
    if t < data.equip.length {
        BladePose::Equip(t.max(0.0))
    } else if t >= unequip_at {
        BladePose::Unequip(t - unequip_at)
    } else {
        BladePose::Out
    }
}

/// The blade's pose in a Whirlwind (`a`, his ability): sliding out from its start
/// (its clip's TnAnimNotify_WeaponMelee at 0 s), out while he spins and sliding back
/// in once it ends (as the melee attack's end puts the weapon away). None when he
/// isn't spinning or it is away.
pub(crate) fn whirl_pose(
    data: &MeleeData,
    a: &crate::abilities::ClassAbility,
) -> Option<BladePose> {
    if a.kind != crate::abilities::Kind::Whirlwind || a.clock <= 0.0 {
        return None;
    }
    if a.active {
        Some(if a.clock < data.equip.length {
            BladePose::Equip(a.clock)
        } else {
            BladePose::Out
        })
    } else {
        (a.since_end < data.unequip.length).then_some(BladePose::Unequip(a.since_end))
    }
}

// ---------------------------------------------------------------------------
// The blade's model
// ---------------------------------------------------------------------------

/// The blade's model and materials.
#[derive(Resource, Default)]
pub struct BladeAssets {
    gltf: Handle<Gltf>,
    /// robot.glb, whose rocket launcher's material the housing is painted from.
    robot: Handle<Gltf>,
    /// The edge's material, culling the back faces and (for a mirrored blade) the
    /// front ones.
    glow: [Handle<BladeGlowMaterial>; 2],
    /// The housing's without robot.glb's material.
    plain: Handle<StandardMaterial>,
    rig: Option<Rig>,
}

/// The blade's skeleton and meshes as its model has them.
struct Rig {
    /// Its bones in an order that has each one's parent (an earlier bone, or none:
    /// on the blade's socket) before it, with their rest poses.
    bones: Vec<(String, Option<usize>, Transform)>,
    /// The skin's joints (indices into `bones`) and inverse bind poses.
    joints: Vec<usize>,
    bindposes: Handle<SkinnedMeshInverseBindposes>,
    /// Its meshes, and which is the edge's.
    parts: Vec<(Handle<Mesh>, bool)>,
    /// Each clip's keys for each bone (by index into `bones`).
    equip: Vec<Option<Keys>>,
    unequip: Vec<Option<Keys>>,
}

impl Rig {
    /// The rig from the loaded model: its skinned mesh's node, the skeleton under
    /// it and the skin's joints.
    fn new(
        gltf: &Gltf,
        (nodes, meshes, skins): (&Assets<GltfNode>, &Assets<GltfMesh>, &Assets<GltfSkin>),
        data: &MeleeData,
    ) -> Option<Rig> {
        let mesh_node = gltf
            .nodes
            .iter()
            .filter_map(|h| nodes.get(h))
            .find(|n| n.mesh.is_some() && n.skin.is_some())?;
        let skin = skins.get(mesh_node.skin.as_ref()?)?;
        let mesh = meshes.get(mesh_node.mesh.as_ref()?)?;
        let glow_material = gltf.named_materials.get(GLOW_MATERIAL);
        let mut bones = Vec::new();
        let mut index = Vec::new();
        let mut todo: Vec<(Handle<GltfNode>, Option<usize>)> = mesh_node
            .children
            .iter()
            .map(|h| (h.clone(), None))
            .collect();
        while let Some((h, parent)) = todo.pop() {
            let node = nodes.get(&h)?;
            let me = bones.len();
            bones.push((node.name.clone(), parent, node.transform));
            index.push(node.index);
            todo.extend(node.children.iter().map(|c| (c.clone(), Some(me))));
        }
        let joints = skin
            .joints
            .iter()
            .map(|h| {
                let i = nodes.get(h)?.index;
                index.iter().position(|&n| n == i)
            })
            .collect::<Option<Vec<usize>>>()?;
        let parts = mesh
            .primitives
            .iter()
            .map(|p| (p.mesh.clone(), p.material.as_ref() == glow_material))
            .collect();
        let tracks = |clip: &BladeClip| {
            bones
                .iter()
                .map(|(name, _, _)| {
                    clip.tracks
                        .iter()
                        .find(|(b, _)| b == name)
                        .map(|(_, keys)| keys.clone())
                })
                .collect()
        };
        Some(Rig {
            equip: tracks(&data.equip),
            unequip: tracks(&data.unequip),
            bones,
            joints,
            bindposes: skin.inverse_bind_matrices.clone(),
            parts,
        })
    }
}

fn load_blade(
    asset_server: Res<AssetServer>,
    data: Res<MeleeData>,
    mut assets: ResMut<BladeAssets>,
    mut glows: ResMut<Assets<BladeGlowMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
) {
    if data.sockets.iter().flatten().all(Option::is_none)
        || !crate::assets_dir().join(BLADE_MODEL).exists()
    {
        return;
    }
    assets.gltf = asset_server.load(BLADE_MODEL);
    assets.robot = asset_server.load(crate::ROBOT_MODEL);
    let mask: Handle<Image> = asset_server.load(BLADE_MASK);
    assets.glow = [false, true].map(|flip| {
        glows.add(BladeGlowMaterial {
            fog: Vec4::ZERO,
            params: Vec4::ZERO,
            mask: mask.clone(),
            flip,
        })
    });
    assets.plain = standard.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.36, 0.38),
        metallic: 0.8,
        perceptual_roughness: 0.4,
        ..default()
    });
}

/// An Ion Blade on a robot's elbow, hidden until its Jazz swings with his gun on
/// that arm (show_blades).
#[derive(Component)]
pub struct Blade {
    /// On the left elbow.
    pub left: bool,
    /// Its bones (as Rig::bones) and meshes (the edge's: true).
    bones: Vec<Entity>,
    parts: Vec<(Entity, bool)>,
    /// Seconds since it came out (its own clock: see RESYNC_SECS), while it shows.
    clock: Option<f32>,
    /// The look and mirroring its materials are in.
    painted: Option<(Look, bool)>,
}

/// An elbow that has its blade.
#[derive(Component)]
pub struct Armed;

/// One of a blade's meshes (main.rs's tag_jazz_joints leaves its joints alone: they
/// aren't his bones).
#[derive(Component)]
pub struct BladePart;

/// One of a blade's bones.
#[derive(Component)]
pub struct BladeBone;

/// Gives each robot's elbows (main.rs's GunBone) their blades, once the blade's model
/// is in.
#[allow(clippy::type_complexity)]
fn spawn_blades(
    mut commands: Commands,
    data: Res<MeleeData>,
    mut assets: ResMut<BladeAssets>,
    gltfs: Res<Assets<Gltf>>,
    gltf_parts: (
        Res<Assets<GltfNode>>,
        Res<Assets<GltfMesh>>,
        Res<Assets<GltfSkin>>,
    ),
    elbows: Query<(Entity, &crate::GunBone), Without<Armed>>,
) {
    if elbows.is_empty() {
        return;
    }
    if assets.rig.is_none() {
        let Some(gltf) = gltfs.get(&assets.gltf) else {
            return;
        };
        let (nodes, meshes, skins) = &gltf_parts;
        assets.rig = Rig::new(gltf, (nodes, meshes, skins), &data);
        match &assets.rig {
            Some(rig) => info!(
                "Ion Blade: {} bones, {} meshes",
                rig.bones.len(),
                rig.parts.len()
            ),
            None => {
                warn!("{BLADE_MODEL} has no skinned mesh: no Ion Blade");
                // (Its elbows are left bare.)
                for (elbow, _) in &elbows {
                    commands.entity(elbow).insert(Armed);
                }
                return;
            }
        }
    }
    let Some(rig) = &assets.rig else {
        for (elbow, _) in &elbows {
            commands.entity(elbow).insert(Armed);
        }
        return;
    };
    for (elbow, bone) in &elbows {
        let root = commands
            .spawn((Transform::default(), Visibility::Hidden, ChildOf(elbow)))
            .id();
        let mut bones: Vec<Entity> = Vec::with_capacity(rig.bones.len());
        for (_, parent, rest) in &rig.bones {
            let parent = parent.map_or(root, |p| bones[p]);
            bones.push(commands.spawn((*rest, BladeBone, ChildOf(parent))).id());
        }
        let joints: Vec<Entity> = rig.joints.iter().map(|&j| bones[j]).collect();
        let parts = rig
            .parts
            .iter()
            .map(|(mesh, glow)| {
                let e = commands
                    .spawn((
                        Mesh3d(mesh.clone()),
                        SkinnedMesh {
                            inverse_bindposes: rig.bindposes.clone(),
                            joints: joints.clone(),
                        },
                        Transform::default(),
                        Visibility::Inherited,
                        // (Its bounds would be its model's, not where it is.)
                        NoFrustumCulling,
                        BladePart,
                        ChildOf(root),
                    ))
                    .id();
                (e, *glow)
            })
            .collect();
        commands.entity(root).insert(Blade {
            left: bone.0,
            bones,
            parts,
            clock: None,
            painted: None,
        });
        commands.entity(elbow).insert(Armed);
    }
}

/// Shows each Jazz's blade while he swings (his class's sword swing, ANIM_MELEE) or
/// spins (the Whirlwind): on his gun's arm at its MeleeSocket from when it comes out,
/// sliding out, then back in with the swing's or spin's end, in his paint. Runs with
/// main.rs's show_weapon_meshes.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn show_blades(
    mut commands: Commands,
    time: Res<Time>,
    data: Res<MeleeData>,
    assets: Res<BladeAssets>,
    chars: Res<Characters>,
    gltfs: Res<Assets<Gltf>>,
    players: Query<&crate::Player>,
    mut blades: Query<(&mut Blade, &crate::JazzOf, &mut Transform, &mut Visibility)>,
    mut bones: Query<&mut Transform, (With<BladeBone>, Without<Blade>)>,
    (mut cache, mut standard, mut paints): (
        ResMut<PaintCache>,
        ResMut<Assets<StandardMaterial>>,
        ResMut<Assets<PaintMaterial>>,
    ),
) {
    let Some(rig) = &assets.rig else {
        return;
    };
    let dt = time.delta_secs();
    for (mut blade, of, mut transform, mut visibility) in &mut blades {
        // His swing, seconds since the blade came out, and its place on his gun; or
        // his Whirlwind's spin (its clip's TnAnimNotify_WeaponMelee at its start: out
        // as it starts, in once it ends).
        let Some((player, socket, how)) = players.get(of.0).ok().and_then(|player| {
            if player.form != crate::Form::Robot || crate::gun_on_left(player) != blade.left {
                return None;
            }
            let socket = data.socket(player.weapon, blade.left)?;
            if let Some(pose) = whirl_pose(&data, &player.ability) {
                return Some((player, socket, Err(pose)));
            }
            let (clip, left) = player.action?;
            if clip != crate::ANIM_MELEE {
                return None;
            }
            let swing = data.swing(player.kit.class);
            // (None for a swing without the blade: the Destroyer's and the Titan's.)
            let t = player.kit.robot.melee_secs - left - swing.blade_at?;
            (t >= 0.0).then_some((player, socket, Ok((t, swing))))
        }) else {
            blade.clock = None;
            if *visibility != Visibility::Hidden {
                *visibility = Visibility::Hidden;
            }
            continue;
        };
        let pose = match how {
            Ok((t, swing)) => {
                let clock = match blade.clock {
                    Some(c) if (c + dt - t).abs() <= RESYNC_SECS => c + dt,
                    _ => t,
                };
                blade.clock = Some(clock);
                blade_pose(&data, clock, swing.length)
            }
            Err(pose) => {
                blade.clock = None;
                pose
            }
        };
        // Its place and pose.
        if *transform != socket {
            *transform = socket;
        }
        for (i, &bone) in blade.bones.iter().enumerate() {
            let Ok(mut bone) = bones.get_mut(bone) else {
                continue;
            };
            let rest = rig.bones[i].2;
            let keyed = match pose {
                BladePose::Equip(at) => rig.equip[i].as_deref().and_then(|k| sample(k, at)),
                BladePose::Unequip(at) => rig.unequip[i].as_deref().and_then(|k| sample(k, at)),
                BladePose::Out => None,
            };
            let (translation, rotation) = keyed.unwrap_or((rest.translation, rest.rotation));
            if bone.translation != translation || bone.rotation != rotation {
                bone.translation = translation;
                bone.rotation = rotation;
            }
        }
        // Its materials, in his look (mirrored on the left: the socket turns it over).
        let flip = socket.scale.is_negative_bitmask().count_ones() & 1 == 1;
        if blade.painted != Some((player.look, flip)) {
            let robot_material = gltfs
                .get(&assets.robot)
                .and_then(|g| g.named_materials.get(BODY_MATERIAL))
                .cloned();
            let painted = robot_material.as_ref().and_then(|m| {
                let base = cache.gun_base(&chars, BODY_MATERIAL, m, &mut standard)?;
                cache.get(&base, &player.look, flip, false, &standard, &mut paints)
            });
            let plain = cache.plain(
                robot_material.as_ref().unwrap_or(&assets.plain),
                flip,
                &mut standard,
            );
            for &(part, glow) in &blade.parts {
                let mut e = commands.entity(part);
                if glow {
                    e.try_insert(MeshMaterial3d(assets.glow[usize::from(flip)].clone()));
                } else if let Some(p) = &painted {
                    e.try_remove::<MeshMaterial3d<StandardMaterial>>()
                        .try_insert(MeshMaterial3d(p.clone()));
                } else {
                    e.try_remove::<MeshMaterial3d<PaintMaterial>>()
                        .try_insert(MeshMaterial3d(plain.clone()));
                }
            }
            blade.painted = Some((player.look, flip));
        }
        if *visibility != Visibility::Inherited {
            *visibility = Visibility::Inherited;
        }
    }
}

// ---------------------------------------------------------------------------
// The edge's material
// ---------------------------------------------------------------------------

/// The blade's edge (WEP_BLadeAut_MAT: see the top of this file and
/// assets/shaders/ionblade.wgsl): unlit, added to what is behind it.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(GlowKey)]
pub struct BladeGlowMaterial {
    /// The game camera's fog: rgb its colour (linear), a how much of it shows.
    #[uniform(0)]
    fog: Vec4,
    /// x unused; y: the fog's kind (0 none, 1 linear, 2 exponential, 3 exponential
    /// squared); z, w: its density, or its start and end (linear).
    #[uniform(1)]
    params: Vec4,
    /// WEP_IaconMask: its circuitry (RGB, data).
    #[texture(2)]
    #[sampler(3)]
    mask: Handle<Image>,
    /// Its faces wind the other way round (a mirrored blade).
    flip: bool,
}

/// What a BladeGlowMaterial's pipeline depends on: which faces it culls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlowKey {
    flip: bool,
}

impl From<&BladeGlowMaterial> for GlowKey {
    fn from(material: &BladeGlowMaterial) -> Self {
        GlowKey {
            flip: material.flip,
        }
    }
}

impl Material for BladeGlowMaterial {
    fn fragment_shader() -> ShaderRef {
        GLOW_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// Its back faces culled (its front ones on a mirrored blade, whose triangles
    /// wind the other way round).
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = Some(if key.bind_group_data.flip {
            Face::Front
        } else {
            Face::Back
        });
        Ok(())
    }
}

/// Fogs the edges as the game camera's fog fogs everything else (energon.rs's way:
/// the shader draws them itself), fading them out with it.
fn fog_glow(
    camera: Query<Option<&DistanceFog>, (With<Camera3d>, With<crate::OrbitCamera>)>,
    assets: Res<BladeAssets>,
    mut materials: ResMut<Assets<BladeGlowMaterial>>,
) {
    let (fog, params) = crate::energon::fog_uniforms(camera.iter().next().flatten());
    for h in &assets.glow {
        let changed = materials
            .get(h)
            .is_some_and(|m| m.fog != fog || m.params != params);
        if changed && let Some(m) = materials.get_mut(h) {
            m.fog = fog;
            m.params = params;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(n: u32, at: Vec3, pick: f32) -> AssistTarget {
        AssistTarget {
            entity: Entity::from_raw_u32(n).unwrap(),
            aim: at + Vec3::Y * 0.4,
            middle: at,
            radius: 0.6,
            pick,
        }
    }

    /// The melee assist picks the enemy the crosshair is on: its targeting point
    /// within its picking radius of the crosshair's line (a robot's 3.75 m, a jet's
    /// 5.5, an insect's 0.8), the nearest that line of two; none behind him, past
    /// 20 m, out of sight or off the line.
    #[test]
    fn the_melee_assist_picks_the_enemy_under_the_crosshair() {
        use crate::enemies::Form;
        assert_eq!(
            (
                assist_radius(Form::Robot),
                assist_radius(Form::Jet),
                assist_radius(Form::Insect)
            ),
            (3.75, 5.5, 0.8)
        );
        let eye = (Vec3::new(1.0, 3.0, 4.0), Vec3::NEG_Z);
        let me = (Vec3::new(0.0, 1.5, 0.0), Vec3::NEG_Z);
        let open = |_: Vec3, _: Vec3| false;
        let near_line = target(1, Vec3::new(1.5, 1.2, -6.0), 3.75);
        let far_off = target(2, Vec3::new(4.0, 1.2, -5.0), 3.75);
        let picked = assist_target(eye, me, &[far_off, near_line], open).map(|t| t.entity);
        assert_eq!(picked, Some(near_line.entity));
        // Off the line by more than its radius: an insect 1.6 m off.
        let insect = target(3, Vec3::new(2.6, 1.6, -6.0), 0.8);
        assert!(assist_target(eye, me, &[insect], open).is_none());
        // Behind him, past MeleeAssistRange, or behind a wall.
        let behind = target(4, Vec3::new(1.0, 1.2, 2.0), 3.75);
        let far = target(5, Vec3::new(1.0, 1.2, -21.0), 3.75);
        assert!(assist_target(eye, me, &[behind, far], open).is_none());
        assert!(assist_target(eye, me, &[near_line], |_, _| true).is_none());
    }

    /// The lunge flies at its enemy at its speed, his run added but never back from
    /// the way he faces, and is over once he is within their two radii of it ahead.
    #[test]
    fn the_lunge_flies_at_its_enemy_and_stops_on_reaching_it() {
        let enemy = AssistTarget {
            entity: Entity::from_raw_u32(1).unwrap(),
            aim: Vec3::new(0.0, 2.0, -6.0),
            middle: Vec3::new(0.0, 1.5, -6.0),
            radius: 0.6,
            pick: 3.75,
        };
        let (me, facing) = (Vec3::new(0.0, 1.5, 0.0), Vec3::NEG_Z);
        let lunge = Lunge::new(Some(enemy), facing);
        let v = lunge_velocity(&lunge, me, facing, Vec3::ZERO, 0.8, 18.75).unwrap();
        assert!((v - Vec3::new(0.0, 0.0, -18.75)).length() < 1e-4, "{v}");
        // Running back from it: the flier's pull alone.
        let back = lunge_velocity(&lunge, me, facing, Vec3::Z, 0.8, 18.75).unwrap();
        assert!((back - v).length() < 1e-4, "{back}");
        // Strafing: half his run's way.
        let side = lunge_velocity(&lunge, me, facing, Vec3::X, 0.8, 18.75).unwrap();
        assert!((side.normalize() - Vec3::new(1.0, 0.0, -1.0).normalize()).length() < 1e-4);
        assert!((side.length() - 18.75).abs() < 1e-3);
        // Within 0.6 + 0.8 m of it ahead: there.
        let close = Vec3::new(0.0, 1.5, -4.7);
        assert!(lunge_velocity(&lunge, close, facing, Vec3::ZERO, 0.8, 18.75).is_none());
    }

    /// With no enemy picked, every swing still lunges: straight ahead along the way
    /// he faced as he swung (level, even looking up), at the same speed, his run
    /// added as at an enemy, for the dash's time (no enemy to reach).
    #[test]
    fn with_no_enemy_the_lunge_flies_straight_ahead() {
        let looking_up = Vec3::new(0.6, 0.5, -0.6);
        let lunge = Lunge::new(None, looking_up);
        assert!(lunge.target.is_none() && lunge.left == DASH_SECS);
        let ahead = Vec3::new(1.0, 0.0, -1.0).normalize();
        assert!((lunge.direction - ahead).length() < 1e-5);
        let me = Vec3::new(0.0, 1.5, 0.0);
        let v = lunge_velocity(&lunge, me, ahead, Vec3::ZERO, 0.8, 18.75).unwrap();
        assert!((v - ahead * 18.75).length() < 1e-3, "{v}");
        // Far along, it still flies (only its time ends it).
        let far = me + ahead * 50.0;
        assert!(lunge_velocity(&lunge, far, ahead, Vec3::ZERO, 0.8, 18.75).is_some());
        // Running back: the flight's pull alone.
        let back = lunge_velocity(&lunge, me, ahead, -ahead, 0.8, 18.75).unwrap();
        assert!((back - v).length() < 1e-3, "{back}");
    }

    const SAMPLE: &str = "# melee\n\
bladeclip\tequip\t0.3333\n\
bladeclip\tunequip\t0.3667\n\
bladekey\tequip\tC_Robo01_XT\t0.0000\t-0.74 0 0\t0 0 0 1\n\
bladekey\tequip\tC_Robo01_XT\t0.3000\t0.43 0 0\t0 0 0 1\n\
bladekey\tunequip\tC_Robo01_XT\t0.0000\t0.43 0 0\t0 0 0 1\n\
bladekey\tunequip\tC_Robo01_XT\t0.3333\t-0.93 0 0\t0 0 0 1\n\
bladesocket\tRB_AssaultRifleR_SKEL\tR_Arm03_Elbow_XB\t-0.75 -0.586357 -0.0890347\t-0 -0 1 6.12323e-17\t0.7 0.7 0.7\n\
bladesocket\tRB_AssaultRifleL_SKEL\tL_Arm03_Elbow_XB\t0.75 0.586357 0.0890347\t-0 -0 0 1\t0.7 0.7 0.7\n\
bladesocket\tRB_IonBlasterL_SKEL\tL_Arm03_Elbow_XB\t1.08205 0.391368 0.0998529\t-0 -0 0 1\t0.7 -0.7 0.7\n\
swing\tinfiltrator\t1.2667\t0.0086\t0.1496\t0.2500\t0.45 4.3 -5.5\t1.1 0 -0\t2.7 3.5 2.7\n\
swing\ttitan\t1.2667\t-\t0.1523\t0.2000\t0.45 4.3 -5.5\t1.4 0 -0\t3.5 4 3.5\n";

    fn weapon(id: &str) -> usize {
        crate::WEAPONS.iter().position(|w| w.id == id).unwrap()
    }

    #[test]
    fn reads_swings_clips_and_sockets() {
        let d = MeleeData::parse(SAMPLE);
        let inf = d.swing(0);
        assert_eq!(inf.blade_at, Some(0.0086));
        assert_eq!(inf.sweep_at, 0.1496);
        assert_eq!(inf.sweep_secs, 0.25);
        assert_eq!(inf.offset, Vec3::new(1.1, 0.0, 0.0));
        assert_eq!(inf.half, Vec3::new(2.7, 3.5, 2.7));
        // (The Destroyer has no line here: the default.)
        assert_eq!(d.swing(1), Swing::DEFAULT);
        assert_eq!(d.swing(2).half, Vec3::new(3.5, 4.0, 3.5));
        // (The Titan swings without the blade.)
        assert_eq!(d.swing(2).blade_at, None);
        assert_eq!(d.equip.length, 0.3333);
        assert_eq!(d.equip.tracks.len(), 1);
        assert_eq!(d.equip.tracks[0].1.len(), 2);
        let ar = weapon("AssaultRifle");
        let right = d.socket(ar, false).unwrap();
        assert!((right.scale - Vec3::splat(0.7)).abs().max_element() < 1e-6);
        assert!(d.socket(ar, true).is_some());
        let ib = weapon("IonBlaster");
        assert!(d.socket(ib, false).is_none());
        assert_eq!(d.socket(ib, true).unwrap().scale, Vec3::new(0.7, -0.7, 0.7));
    }

    #[test]
    fn blade_slides_out_rests_and_slides_back() {
        let d = MeleeData::parse(SAMPLE);
        let length = 1.2667;
        assert_eq!(blade_pose(&d, 0.15, length), BladePose::Equip(0.15));
        let (p, _) = sample(&d.equip.tracks[0].1, 0.15).unwrap();
        assert!((p.x - (-0.74 + (0.43 + 0.74) * 0.5)).abs() < 1e-4);
        assert_eq!(blade_pose(&d, 0.6, length), BladePose::Out);
        // (Back in from the swing's length less the unequip's: 0.9 s out.)
        let BladePose::Unequip(at) = blade_pose(&d, 1.0, length) else {
            panic!("not sliding back in at 1 s");
        };
        assert!((at - (1.0 - (1.2667 - 0.3667))).abs() < 1e-4);
        // (Held at its last key past its end.)
        let (p, _) = sample(&d.unequip.tracks[0].1, 1.0).unwrap();
        assert_eq!(p.x, -0.93);
    }

    #[test]
    fn sweep_box_sits_ahead_and_to_the_gun_side() {
        let d = MeleeData::parse(SAMPLE);
        let inf = d.swing(0);
        let feet = Vec3::new(10.0, 0.0, 5.0);
        // Facing -Z (no turn): the socket 3.3 m ahead and 2.58 m up at 0.6, the box
        // 1.1 m to his right of it.
        let b = inf.sweep_box(feet, Quat::IDENTITY, 0.6, false);
        assert!((b.centre - (feet + Vec3::new(0.27 + 1.1, 2.58, -3.3))).length() < 1e-4);
        let l = inf.sweep_box(feet, Quat::IDENTITY, 0.6, true);
        assert!((l.centre - (feet + Vec3::new(0.27 - 1.1, 2.58, -3.3))).length() < 1e-4);
        // Turned to face +X (a quarter turn clockwise from above): ahead is +X.
        let turn = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
        let b = inf.sweep_box(feet, turn, 0.6, false);
        assert!((b.centre - (feet + Vec3::new(3.3, 2.58, 0.27 + 1.1))).length() < 1e-4);
        // Its nearest point to an enemy ahead, beyond it: its front face.
        let far = b.nearest(feet + Vec3::new(20.0, 2.0, 1.0));
        assert!((far.x - (feet.x + 3.3 + 2.7)).abs() < 1e-4);
        // Inside: the point itself.
        let p = feet + Vec3::new(2.0, 1.0, 0.5);
        assert!((b.nearest(p) - p).length() < 1e-5);
    }
}
