//! The map's parts that move by themselves in the game (levels/<map>_motion.txt,
//! tools/build_motion.py): its spinning actors and its pulsing glows.
//!
//! * **Spinning actors** (Physics PHYS_Rotating): each turns by its RotationRate
//!   every second, its Rotation's pitch, yaw and roll each going on by the rate's
//!   (Actor.physRotating), drawn as the level has it at each moment: Ignition's two
//!   turbines over the rooms beside the players' start (a turn every 6 s, about their
//!   upright axis), Downfall's map room's monitors. The map's model has each as a
//!   node of its own (named after its mesh, at the actor's place: tools/build_map.py),
//!   which is found as the map comes in, left out of the joined meshes (merge.rs) and
//!   turned every frame. None of them collides with anything.
//! * **Pulsing glows**: ENV_BRDF_A_MAT's glow with its Enable_Emissive_Pulse switch on
//!   is multiplied by clamp(sin(2 pi x rate x time) + 1, 0, 1.5) (its ENV_Emissive_MF):
//!   at half again its glow for a third of each cycle, dimming to nothing and back
//!   over the rest. Oblivion's machines' lights (the orange bars on the ordnance
//!   feeder's doors ahead of the players' start, OrdinanceDoor_MATINST, and the blue
//!   ones on its pipes and pillars, ArkDrivingPipe_ and ArkDrivingPillar_MATINST: once
//!   a second, all together), its warning signs, the consoles' lights, the storage
//!   pods' and others. Every part drawn with a material of that name in the map's
//!   models (its own, its doors', its consoles') with a glow (its emissive, as it
//!   came in) is drawn in a live material that scales it so in its shader, from the
//!   frame's time (live.rs: changing the material every frame cost the render
//!   thread more).

use bevy::prelude::*;
use bevy::scene::{SceneInstance, SceneSpawner};
use std::path::Path;

use crate::merge::MapScene;

/// A spinning actor: its mesh (its node's name), its place (Bevy metres, the node's
/// translation), its Rotation as the level starts and its RotationRate (Unreal's
/// units: pitch, yaw, roll; and a second), and its size (DrawScale3D x DrawScale,
/// Unreal's axes).
#[derive(Clone, Debug, PartialEq)]
pub struct Spin {
    pub mesh: String,
    pub at: Vec3,
    pub rotation: Vec3,
    pub rate: Vec3,
    pub scale: Vec3,
}

/// The map's parts that move by themselves.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct MapMotion {
    pub spins: Vec<Spin>,
    /// The materials whose glow pulses, and its cycles a second.
    pub pulses: Vec<(String, f32)>,
}

impl MapMotion {
    /// levels/<stem>_motion.txt's (none without it).
    pub fn load(assets: &Path, stem: &str) -> MapMotion {
        let path = assets.join(format!("levels/{stem}_motion.txt"));
        std::fs::read_to_string(&path)
            .map_or_else(|_| MapMotion::default(), |t| MapMotion::parse(&t))
    }

    pub fn parse(text: &str) -> MapMotion {
        let mut out = MapMotion::default();
        let v3 = |s: &str| -> Option<Vec3> {
            let n: Vec<f32> = s
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            (n.len() == 3).then(|| Vec3::new(n[0], n[1], n[2]))
        };
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            match (f.first().copied(), f.len()) {
                (Some("spin"), 6) => {
                    if let (Some(at), Some(rotation), Some(rate), Some(scale)) =
                        (v3(f[2]), v3(f[3]), v3(f[4]), v3(f[5]))
                    {
                        out.spins.push(Spin {
                            mesh: f[1].to_string(),
                            at,
                            rotation,
                            rate,
                            scale,
                        });
                    }
                }
                (Some("pulse"), 3) => {
                    if let Ok(rate) = f[2].parse() {
                        out.pulses.push((f[1].to_string(), rate));
                    }
                }
                _ => {}
            }
        }
        out
    }

    pub fn pulse_rate(&self, material: &str) -> Option<f32> {
        self.pulses
            .iter()
            .find(|(m, _)| m == material)
            .map(|(_, r)| *r)
    }
}

/// Unreal's rotation units to radians (65536 a turn).
const UNIT: f32 = std::f32::consts::TAU / 65536.0;

/// The turn of Unreal's rotator (pitch, yaw, roll: its units) in the glTF axes the
/// map's model is in (Unreal's x, z, y): FRotationMatrix, whose rows are where the
/// rotator takes the axes, its y and z swapped.
pub fn rotation_matrix(rotator: Vec3) -> Mat3 {
    let (sp, cp) = (rotator.x * UNIT).sin_cos();
    let (sy, cy) = (rotator.y * UNIT).sin_cos();
    let (sr, cr) = (rotator.z * UNIT).sin_cos();
    // (Unreal's: each row an axis's image.)
    let x = Vec3::new(cp * cy, cp * sy, sp);
    let y = Vec3::new(sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, -sr * cp);
    let z = Vec3::new(-(cr * sp * cy + sr * sy), cy * sr - cr * sp * sy, cr * cp);
    let swap = |v: Vec3| Vec3::new(v.x, v.z, v.y);
    // (The columns, Unreal's x, z, y axes' images, swapped likewise.)
    Mat3::from_cols(swap(x), swap(z), swap(y))
}

impl Spin {
    /// Its node's transform `t` seconds on (its Rotation gone on by `t` x its rate).
    pub fn transform(&self, t: f32) -> Transform {
        let turn = rotation_matrix(self.rotation + self.rate * t);
        let size = Vec3::new(self.scale.x, self.scale.z, self.scale.y);
        let m = turn * Mat3::from_diagonal(size);
        Transform::from_matrix(Mat4::from_cols(
            m.x_axis.extend(0.0),
            m.y_axis.extend(0.0),
            m.z_axis.extend(0.0),
            self.at.extend(1.0),
        ))
    }
}

/// A spinning actor's node: its Spin and when it was found (its clock's start).
#[derive(Component, Clone, Debug)]
pub struct Spinning {
    spin: Spin,
    since: f32,
}

/// A map's scene whose spinning actors have been looked for.
#[derive(Component)]
struct SpinsFound;

/// Its spinning actors' nodes, once the map's scene is in (before its parts are
/// joined: merge.rs leaves theirs out): each by its name and place.
#[allow(clippy::type_complexity)]
fn find_spinners(
    mut commands: Commands,
    time: Res<Time>,
    motion: Option<Res<MapMotion>>,
    roots: Query<(Entity, &SceneInstance), (With<MapScene>, Without<SpinsFound>)>,
    spawner: Res<SceneSpawner>,
    children: Query<&Children>,
    nodes: Query<(&Name, &Transform)>,
) {
    for (root, instance) in &roots {
        if !spawner.instance_is_ready(**instance) {
            continue;
        }
        commands.entity(root).insert(SpinsFound);
        let Some(motion) = motion.as_deref() else {
            continue;
        };
        let mut found = 0;
        for entity in children.iter_descendants(root) {
            let Ok((name, transform)) = nodes.get(entity) else {
                continue;
            };
            if let Some(spin) = motion
                .spins
                .iter()
                .find(|s| s.mesh == name.as_str() && s.at.distance(transform.translation) < 0.05)
            {
                commands.entity(entity).insert(Spinning {
                    spin: spin.clone(),
                    since: time.elapsed_secs(),
                });
                found += 1;
            }
        }
        if found < motion.spins.len() {
            warn!(
                "{} of the map's {} spinning actors not found in its model",
                motion.spins.len() - found,
                motion.spins.len()
            );
        }
    }
}

fn spin(time: Res<Time>, mut spinning: Query<(&Spinning, &mut Transform)>) {
    let now = time.elapsed_secs();
    for (s, mut transform) in &mut spinning {
        *transform = s.spin.transform(now - s.since);
    }
}

/// The pulse of ENV_Emissive_MF at `t` seconds, `rate` cycles a second, as the
/// pulsing parts' shader works it out (live.rs's Wave::Pulse; for the tests).
#[cfg(test)]
pub fn pulse(rate: f32, t: f32) -> f32 {
    ((std::f32::consts::TAU * rate * t).sin() + 1.0).clamp(0.0, 1.5)
}

pub struct MapMotionPlugin;

impl Plugin for MapMotionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (find_spinners, spin).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The turbines as the map's model has them (its nodes' matrices, from Ignition's
    /// InterpActor_16350 and _873: turned a quarter turn in pitch and yaw, one
    /// mirrored) come out of their Rotation and size; turning at -10922 a second in
    /// roll, each goes round its upright axis (the model's y) once in 6 s, the
    /// mirrored one the other way round.
    #[test]
    fn the_turbines_turn_about_their_upright_axis() {
        let text = "spin\tturbine_STAT\t187.857 -3.52544 104.726\t-16384 -16384 0\t0 0 -10922\t-0.26615 0.26615 0.26615\n\
                    spin\tturbine_STAT\t187.857 -3.52544 -94.4865\t16384 16384 -32768\t0 0 -10922\t0.26615 0.26615 0.26615\n\
                    pulse\tArkDrivingPillar_MATINST\t1\n";
        let m = MapMotion::parse(text);
        assert_eq!(m.spins.len(), 2);
        assert_eq!(m.pulse_rate("ArkDrivingPillar_MATINST"), Some(1.0));
        let s = 0.26615;
        // (The model's: column by column.)
        let model = [
            Mat3::from_cols(
                Vec3::new(0.0, s, 0.0),
                Vec3::new(0.0, 0.0, -s),
                Vec3::new(s, 0.0, 0.0),
            ),
            Mat3::from_cols(
                Vec3::new(0.0, s, 0.0),
                Vec3::new(0.0, 0.0, s),
                Vec3::new(s, 0.0, 0.0),
            ),
        ];
        for (spin, want) in m.spins.iter().zip(model) {
            let t = spin.transform(0.0);
            let got = Mat3::from_quat(t.rotation) * Mat3::from_diagonal(t.scale);
            assert!(
                (got - want).abs_diff_eq(Mat3::ZERO, 1e-4),
                "{got} vs {want}"
            );
            assert!(t.translation.distance(spin.at) < 1e-5);
            // A sixth of a turn a second about the upright axis: a point off its axis
            // (the mesh's y: its x is the axis, Unreal's roll's) goes round level,
            // back after 6 s.
            let p = Vec3::Y;
            let at = |secs: f32| {
                let t = spin.transform(secs);
                t.transform_point(p) - t.translation
            };
            let (p0, p1, p6) = (at(0.0), at(1.0), at(6.0));
            assert!((p0.y - p1.y).abs() < 1e-4, "level");
            let turned = p0.xz().angle_to(p1.xz()).to_degrees();
            assert!((turned.abs() - 60.0).abs() < 0.1, "{turned}");
            assert!(p0.distance(p6) < 1e-3);
        }
        let turn = |spin: &Spin| {
            let p = Vec3::Y;
            let a = spin.transform(0.0).transform_point(p) - spin.at;
            let b = spin.transform(1.0).transform_point(p) - spin.at;
            a.xz().angle_to(b.xz())
        };
        assert!(
            turn(&m.spins[0]) * turn(&m.spins[1]) < 0.0,
            "mirrored, the other way round"
        );
    }

    /// ENV_Emissive_MF's pulse: clamp(sin + 1, 0, 1.5): 1 at the cycle's start, held
    /// at 1.5 over the top third, nothing at its three quarters.
    #[test]
    fn the_glow_pulses_as_its_material_has_it() {
        assert!((pulse(1.0, 0.0) - 1.0).abs() < 1e-5);
        assert!((pulse(1.0, 0.25) - 1.5).abs() < 1e-5);
        assert!((pulse(1.0, 1.0 / 12.0 + 0.01) - 1.5).abs() < 1e-5);
        assert!(pulse(1.0, 0.75) < 1e-5);
        assert!((pulse(0.5, 0.5) - 1.5).abs() < 1e-5);
    }
}
