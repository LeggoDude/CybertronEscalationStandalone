//! Weapon firing effects: muzzle flashes, tracers, impact bursts, muzzle lights and
//! ejected shells, built from the game's own effect textures (assets/textures/fx,
//! exported from the weapons' particle systems) and timings.
//!
//! The game's effects are Unreal 3 particle systems. This module rebuilds them as
//! simple sprite particles: each effect is a few emitters (flash, flame, sparks,
//! smoke...) with the textures, blend modes, burst counts, lifetimes, sizes and
//! speeds read from those particle systems. Systems ask for an effect with
//! `FxQueue::spawn` and for a shot's tracer with `FxQueue::tracer`.

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::math::Affine2;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use std::collections::HashMap;
use std::path::Path;

use crate::live::{LiveMaterial, LiveTints, Wave, live_material, tinted};

/// The effects the weapons use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effect {
    /// The Neutron Assault Rifle's shot: its light (its TnWeaponEffectLight). Its
    /// flash is the game's own particle system (AssaultRifleArm_MuzzleFlash_FX),
    /// which levelfx.rs runs (main.rs's muzzle_system).
    RifleMuzzle,
    /// The Subsonic Repeater's shot: its light (its TnWeaponEffectLight). Its flash
    /// is the game's own particle system (Subsonic_Repeater_MuzzleFlash_FX), which
    /// levelfx.rs runs (main.rs's muzzle_system).
    SubsonicMuzzle,
    /// The same flash again at the Subsonic Repeater's second barrel, which every
    /// shot also plays there (TnWeaponEffectPlayFXOnSocket on MuzzleFlash2): no light
    /// (and its flash the game's own there too: main.rs's muzzle_system).
    SubsonicFlash,
    /// Photon Burst Rifle (Photon_BurstRifle_MuzzleFlash_Fx).
    PhotonMuzzle,
    /// The car's guns (AssaultRifle_VEH_MuzzleFlash_FX).
    CarMuzzle,
    /// Bullet impact of the rifle and the car guns (AssaultRifle_VEH_Squib_FX).
    BulletImpact,
    /// Subsonic Repeater impact (Subsonic_Repeater_Squib_FX).
    SubsonicImpact,
    /// Photon Burst Rifle impact (Photon_BurstRifle_Squib_Fx).
    PhotonImpact,
    /// Scatter Blaster (Scatterblaster_MuzzleFlash_FX).
    ScatterMuzzle,
    /// Scatter Blaster pellet impact (Scatterblaster_Squib_FX).
    ScatterImpact,
    /// A spent shell thrown from the gun (Sm_Shell3_FX).
    Shell,
    /// The Scatter Blaster's bigger shell (Med_Shell2_FX), one with every shot.
    MedShell,
    /// One puff of the steam a spent shell's effect lets out of the shell port for
    /// 1.5 s (the SmokeSteam of Sm_Shell3_FX: a wisp), and the Scatter Blaster's
    /// bigger shell's (Med_Shell2_FX: two).
    ShellSteam,
    MedShellSteam,
    /// Jazz's car jumping: its jets fire down (CarJump_01_FX).
    CarJump,
    /// One puff of a hover jet under a wheel (CarHover_A_01_FX).
    CarHover,
    /// The car landing from a short fall (Vehland_Low_Med_FX) and a long one
    /// (Vehland_High_XSm_FX).
    CarLandLow,
    CarLandHigh,
    /// The hover car landing (Vehland_Low_XSm_FX).
    CarLandHover,
    /// One flicker of a boost flame (Booster_Jazz_FX).
    CarBoost,
    /// Steam from the Neutron Assault Rifle as it reloads (its TnWeaponEffectPlayFXOnSocket
    /// on ShellEject: ReloadSteam_Med_FX).
    ReloadSteam,
    /// Steam from the Scatter Blaster as it reloads (ReloadSteam_Med_FX and
    /// ReloadSteam_Med02_FX on its ReloadFX socket).
    ScatterReloadSteam,
    /// The smoke flying along a Scatter Blaster pellet's line (Scatterblaster_Tracer_FX
    /// Smoky_Puffs_Dark and Smoky_Puffs), which FxQueue::tracer adds.
    ScatterTracerSmoke,
    /// A bullet hole left where a rifle or car-gun round strikes a wall
    /// (AssaultRifle_VEH_Squib_FX's decal emitter); `dir` is the surface normal.
    BulletHole,
    /// The Subsonic Repeater's, its rim glowing cyan (Subsonic_Repeater_Squib_FX's
    /// decal emitter).
    SubsonicHole,
    /// Steam from the Subsonic Repeater as it reloads (ReloadSteam_FX on its
    /// ReloadFX socket); the Nucleon Charge Rifle's too.
    SubsonicReloadSteam,
    /// Nucleon Charge Rifle (NucleonRifle_MuzzleFlash_FX), with its red light.
    NucleonMuzzle,
    /// Its round striking (Squib_SniperRifle_FX).
    NucleonImpact,
    /// The rings its tracer leaves along the shot's last few metres before it
    /// struck (NucleonRifle_Tracer_FX Rings_Export), which FxQueue::tracer adds;
    /// `dir` points back along the shot.
    NucleonRings,
    /// One ring of the glow around its barrel while it is fully charged
    /// (NucleonRifle_Charged_FX Ring02, 20 a second, which main.rs lets out at its
    /// muzzle).
    NucleonCharged,
    /// Path Blaster (PathBlaster_MuzzleFlash_FX), with its light.
    PathMuzzle,
    /// Its round striking (PathBlaster_Squib_FX).
    PathImpact,
    /// The scorch mark its round leaves on a wall (PathBlaster_Squib_FX's decal
    /// emitter); `dir` is the surface normal.
    PathScorch,
    /// Steam from the Path Blaster as it reloads (ReloadSteam_Med02_FX on its
    /// ReloadFX socket).
    PathReloadSteam,
    /// The Riot Cannon's shot: its light (its TnWeaponEffectLight). Its flash, flying
    /// round and blast are the game's own particle systems, which levelfx.rs runs
    /// (riot.rs starts them).
    RiotMuzzle,
    /// Steam from the Riot Cannon as it reloads (ReloadSteam_FX and
    /// ReloadSteam_Med02_FX on its ReloadFX socket).
    RiotReloadSteam,
    /// The Orbital Pistol's shot: its light (its TnWeaponEffectLight). Its flash,
    /// beam and impact are the game's own particle systems, which levelfx.rs runs
    /// (orbital.rs starts them).
    OrbitalMuzzle,
    /// The X18 Scrapmaker's, the Energon Harvester's and the Corrosive Slime
    /// Cannon's shots: their lights (their TnWeaponEffectLights); their flashes and
    /// the rest are the game's particle systems (arsenal.rs).
    ScrapmakerMuzzle,
    HarvesterMuzzle,
    SlimeMuzzle,
    /// The Gear Shredder's shot: its light (its TnWeaponEffectLight); its blades
    /// are the game's particle systems (arsenal.rs, riot.rs).
    ShredderMuzzle,
    /// The Thermo Rocket Cannon's, the A-4 Pulsar Cannon's and the Chaos-Rift
    /// Combustor's shots: their lights (their TnWeaponEffectLights); their flashes
    /// and rounds are the game's particle systems (riot.rs, arsenal.rs).
    ThermoMuzzle,
    PulsarMuzzle,
    CombustorMuzzle,
    /// Nothing: a gun whose shots bring no light of their own (the TechVolt's light
    /// is held while it fires: arsenal.rs).
    NoFlash,
}

/// Tracer looks (the weapons' tracer particle systems).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TracerKind {
    /// AssaultRifle_VEH_Tracer_FX: a spray of long fiery streaks (rifle and car guns).
    Bullet,
    /// Subsonic_Repeater_Tracer_FX: a pair of thin cyan bolts and a glowing core.
    Subsonic,
    /// Photon_BurstRifle_Tracer_Fx: a spray of long blue streaks, faster than the rifle's.
    Photon,
    /// Scatterblaster_Tracer_FX: a bundle of long fiery streaks per pellet, fanning
    /// out a little, with smoke along its path.
    Scatter,
    /// NucleonRifle_Tracer_FX: three cyan bolts at over 900 m/s, and rings where it
    /// struck (Effect::NucleonRings).
    Nucleon,
    /// PathBlaster_Tracer_FX: three red-hot bolts at 180-510 m/s.
    Path,
}

struct Request {
    effect: Effect,
    pos: Vec3,
    dir: Vec3,
    /// Velocity the particles start with on top of their own (the car's, for its
    /// jets, which the game draws in the car's own space).
    vel: Vec3,
    /// Extra direction for shells (Jazz's right).
    side: Vec3,
    floor: Option<f32>,
    /// Size of this one (the hover jets' push).
    scale: f32,
    /// How far along `dir` its particles go before they are cut off (a tracer's
    /// smoke, where the shot struck).
    stop: f32,
    /// Which puff of a steam this is (`spawn_puff`): its emitters let out no more
    /// puffs than their `puffs`.
    puff: u32,
}

struct TracerRequest {
    kind: TracerKind,
    origin: Vec3,
    dir: Vec3,
    stop: f32,
}

/// Queue of effects to spawn this frame.
#[derive(Resource, Default)]
pub struct FxQueue {
    requests: Vec<Request>,
    tracers: Vec<TracerRequest>,
    seed: u32,
}

impl FxQueue {
    /// An effect at `pos`, pointing along `dir` (the barrel for muzzle flashes, the
    /// surface normal for impacts).
    pub fn spawn(&mut self, effect: Effect, pos: Vec3, dir: Vec3) {
        self.requests.push(Request {
            effect,
            pos,
            dir: dir.normalize_or(Vec3::Y),
            vel: Vec3::ZERO,
            side: Vec3::X,
            floor: None,
            scale: 1.0,
            stop: f32::INFINITY,
            puff: 0,
        });
    }

    /// Puff `puff` (0 the first) of a steam let out a puff at a time (a reload's), at
    /// `pos` along `dir`: those of its emitters that have let out all theirs (their
    /// `puffs`) let out no more.
    pub fn spawn_puff(&mut self, effect: Effect, pos: Vec3, dir: Vec3, puff: u32) {
        self.spawn(effect, pos, dir);
        if let Some(req) = self.requests.last_mut() {
            req.puff = puff;
        }
    }

    /// An effect carried along at `vel` (jets on a moving car).
    pub fn spawn_moving(&mut self, effect: Effect, pos: Vec3, dir: Vec3, vel: Vec3) {
        self.spawn_scaled(effect, pos, dir, vel, 1.0);
    }

    /// The same, `scale` times its size (a hover jet pushing less than fully).
    pub fn spawn_scaled(&mut self, effect: Effect, pos: Vec3, dir: Vec3, vel: Vec3, scale: f32) {
        self.requests.push(Request {
            effect,
            pos,
            dir: dir.normalize_or(Vec3::Y),
            vel,
            side: Vec3::X,
            floor: None,
            scale,
            stop: f32::INFINITY,
            puff: 0,
        });
    }

    /// A shell (Effect::Shell or Effect::MedShell) thrown out towards `side`,
    /// landing on `floor` if given.
    pub fn shell(&mut self, effect: Effect, pos: Vec3, side: Vec3, floor: Option<f32>) {
        self.requests.push(Request {
            effect,
            pos,
            dir: Vec3::Y,
            vel: Vec3::ZERO,
            side: side.normalize_or(Vec3::X),
            floor,
            scale: 1.0,
            stop: f32::INFINITY,
            puff: 0,
        });
    }

    /// A shot's tracer, fired from `origin` along `dir`; it stops `stop` metres out,
    /// where the shot struck. So does any smoke it throws along its line.
    pub fn tracer(&mut self, kind: TracerKind, origin: Vec3, dir: Vec3, stop: f32) {
        let dir = dir.normalize_or(Vec3::NEG_Z);
        self.tracers.push(TracerRequest {
            kind,
            origin,
            dir,
            stop,
        });
        if let Some(effect) = tracer_smoke(kind) {
            self.requests.push(Request {
                effect,
                pos: origin,
                dir,
                vel: Vec3::ZERO,
                side: Vec3::X,
                floor: None,
                scale: 1.0,
                stop,
                puff: 0,
            });
        }
        // The Nucleon Charge Rifle's rings hang along the shot's last few metres.
        if kind == TracerKind::Nucleon {
            self.spawn(Effect::NucleonRings, origin + dir * stop, -dir);
        }
    }

    /// True `p` of the time (0 to 1).
    pub fn chance(&mut self, p: f32) -> bool {
        p >= 1.0 || self.random() < p
    }

    fn random(&mut self) -> f32 {
        let mut x = self.seed.wrapping_add(0x9E37_79B9) | 1;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, r: (f32, f32)) -> f32 {
        r.0 + (r.1 - r.0) * self.random()
    }

    fn unit(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(
                self.random() * 2.0 - 1.0,
                self.random() * 2.0 - 1.0,
                self.random() * 2.0 - 1.0,
            );
            let l = v.length_squared();
            if l > 1e-4 && l <= 1.0 {
                return v / l.sqrt();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Effect descriptions
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tex {
    LightBurst,
    Bursts,
    CloudsDark,
    Bolt,
    FireSpears,
    IonCap,
    IonCore,
    MuzzleFlash2,
    Scrapmaker,
    SmokeClouds,
    SparkTrail,
    SphereGlow,
    Embers,
    LightBeam,
    LightningRing,
    Sparks,
    Spot,
    Streak,
    StarBurstStar,
    StarBurstChunks,
    LightStar,
    MuzzleLight,
    SmokeBall,
    SmokeLarge,
    SmokeSpear,
    MetalDebris,
    MuzzleCore,
    SmokeThin,
    BulletHole,
    BurstsMask,
    Ring,
    MuzzleCap,
    MuzzleSparks,
    SparkBase,
    Flare,
    PathCore,
    MuzzleCapGrey,
    EmberClip,
    Flame,
    Scorch,
}

/// File name in assets/textures/fx and sub-image grid (columns, rows).
const TEXTURES: [(Tex, &str, (u32, u32)); 40] = [
    (Tex::LightBurst, "2x2SubUV_lightburst_CLR", (2, 2)),
    (Tex::Bursts, "4x4_Bursts_CLR", (4, 4)),
    (Tex::CloudsDark, "4x4_Clouds_Dark_CLR", (4, 4)),
    (Tex::Bolt, "Bolt_CLR", LYING),
    (Tex::FireSpears, "FireSpears_CLR", (4, 1)),
    (Tex::IonCap, "Ion_Cannon_MuzzleFlash_Cap_CLR", (1, 1)),
    (Tex::IonCore, "Ion_Cannon_MuzzleFlash_Core_CLR", (1, 1)),
    (Tex::MuzzleFlash2, "MuzzleFlash2_CLR", (1, 1)),
    (Tex::Scrapmaker, "MuzzleFlash_Scrapmaker_CLR", (4, 1)),
    (Tex::SmokeClouds, "SmokeClouds_01_CLR", (4, 4)),
    (Tex::SparkTrail, "Spark_Trail_CLR", (1, 1)),
    (Tex::SphereGlow, "SphereGlow_01_CLR", (1, 1)),
    (Tex::Embers, "embers_01_CLR", (1, 1)),
    (Tex::LightBeam, "LightBeam_Falloff_01_CLR", (1, 1)),
    (Tex::LightningRing, "lightningring_01_CLR", (1, 1)),
    (Tex::Sparks, "Sparks_01_CLR", (4, 2)),
    (Tex::Spot, "Spot_CLR", (1, 1)),
    // The Photon Burst Rifle's (tools/build_fx_textures.py; the two StarBurst images
    // are channels of StarBurst_Greypak_01_CLR).
    (Tex::Streak, "Streak_CLR", LYING),
    (Tex::StarBurstStar, "StarBurst_Star_CLR", (1, 1)),
    (Tex::StarBurstChunks, "StarBurst_Chunks_CLR", (1, 1)),
    (Tex::LightStar, "Light_Star_Streaky_CLR", (1, 1)),
    (Tex::MuzzleLight, "MuzzleFlash_Light_CLR", (4, 1)),
    // The grenade explosion's (tools/build_fx_textures.py).
    (Tex::SmokeBall, "SmokeBall_CLR", (1, 1)),
    (Tex::SmokeLarge, "Smoke_Lrg_CLR", (2, 2)),
    (Tex::SmokeSpear, "SmokeSpear_CLR", (4, 1)),
    (Tex::MetalDebris, "MetalDebris_CLR", (2, 2)),
    // The Scatter Blaster's fire burst (tools/build_fx_textures.py): four flames.
    (Tex::MuzzleCore, "MuzzleCore_ClusterBomb_CLR", (4, 1)),
    // The rifle's reload steam and bullet holes (tools/build_fx_textures.py).
    (Tex::SmokeThin, "SmokeThin_CLR", (1, 1)),
    (Tex::BulletHole, "BulletHole_CLR", (1, 1)),
    // 4x4_Bursts_CLR as an opacity mask, for the Subsonic Repeater's dark impact
    // bursts (tools/build_fx_textures.py).
    (Tex::BurstsMask, "4x4_Bursts_Mask_CLR", (4, 4)),
    // The Nucleon Charge Rifle's (tools/build_fx_textures.py): its tracer's rings
    // (white, their brightness their opacity), its impact's fire burst, its muzzle
    // flash's four flames (one on each plane of the cap mesh), and its impact's
    // white-hot spark burst and flares.
    (Tex::Ring, "Ring_CLR", (1, 1)),
    (Tex::MuzzleCap, "MuzzleCap_ClusterBomb_CLR", (1, 1)),
    (Tex::MuzzleSparks, "MuzzleFlash_Sparks_CLR", (1, 1)),
    (Tex::SparkBase, "Spark_Base_CLR", (1, 1)),
    (Tex::Flare, "smokeball_01_CLR", (1, 1)),
    // The Path Blaster's (tools/build_fx_textures.py): its muzzle flash's fiery iris
    // ring (MuzzleCap_Dup's PathBlaster_MuzzleCore_Mat, its four textures put
    // together), its fire burst (MuzzleCore_Export's Burst_04_Mesh_MAT_INST: the red
    // of MuzzleCap_ClusterBomb_CLR, grey) and its impact's sparkles, flames and
    // scorch mark.
    (Tex::PathCore, "PathBlaster_MuzzleCore_CLR", (1, 1)),
    (Tex::MuzzleCapGrey, "MuzzleCap_ClusterBomb_R_CLR", (1, 1)),
    (Tex::EmberClip, "ember_clip_CLR", (1, 1)),
    (Tex::Flame, "Flame_CLR", (1, 1)),
    (Tex::Scorch, "Blastmark_CLR", (1, 1)),
];

/// Marks a texture drawn sideways (one image, turned a quarter).
const LYING: (u32, u32) = (1, 0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    /// Faces the camera.
    Camera,
    /// Stretched along its direction of travel, turned to face the camera.
    Velocity,
    /// Starts at the muzzle and points along the effect's direction (a flame);
    /// spawned as two crossed sprites.
    Flame,
    /// The car's jet cone (assets/models/fx/booster_cone.glb): starts at the muzzle
    /// and points along the effect's direction, spinning about it; a flame sprite
    /// stands in without it.
    Cone,
    /// Lies flat on a surface, facing out along the effect's direction (the surface
    /// normal), turned at random about it: a decal. (The Path Blaster's muzzle-flash
    /// caps face out along the barrel the same way.)
    Decal,
}

/// Whether an emitter needs the Ion core mesh: sprite stand-ins are used when
/// assets/models/fx/ion_core.glb is missing. WithTexture: it is left out when its
/// texture is (a plain square would stand in badly for a bullet hole or a wisp).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Need {
    Always,
    WithMesh,
    WithoutMesh,
    WithTexture,
}

impl Need {
    fn met(self, have_mesh: bool) -> bool {
        match self {
            Need::Always | Need::WithTexture => true,
            Need::WithMesh => have_mesh,
            Need::WithoutMesh => !have_mesh,
        }
    }
}

/// A value over a particle's life (`t` from 0 to 1), from evenly spaced samples;
/// 1 when there are none.
fn sample(curve: &[f32], t: f32) -> f32 {
    match curve.len() {
        0 => 1.0,
        1 => curve[0],
        n => {
            let x = t.clamp(0.0, 1.0) * (n - 1) as f32;
            let i = (x as usize).min(n - 2);
            let f = x - i as f32;
            curve[i] + (curve[i + 1] - curve[i]) * f
        }
    }
}

#[derive(Clone, Copy)]
struct Emitter {
    tex: Tex,
    additive: bool,
    align: Align,
    count: (u32, u32),
    /// Seconds.
    life: (f32, f32),
    /// Width and length in metres, and how much it grows by the end of its life.
    width: (f32, f32),
    length: (f32, f32),
    grow: f32,
    /// Speed along the effect direction, sideways spread and gravity (m/s, m/s²).
    speed: (f32, f32),
    spread: f32,
    gravity: f32,
    /// Linear colour; values above 1 make additive sprites brighter.
    color: [f32; 4],
    delay: f32,
    need: Need,
    /// Width and length multipliers over its life (the game's SizeMultiplyLife),
    /// used instead of `grow` when given.
    width_curve: &'static [f32],
    length_curve: &'static [f32],
    /// Turns per second about its length (meshes), or about its facing (decals).
    spin: (f32, f32),
    /// Its speed's multiplier over its life (the game's VelocityOverLifetime).
    speed_curve: &'static [f32],
    /// Its opacity over its life (the game's alpha over life), in place of the
    /// usual fade-out when given.
    fade_curve: &'static [f32],
    /// Its colour's multiplier over its life (the game's colour over life): a
    /// bullet hole's rim cooling.
    heat_curve: &'static [f32],
    /// How far it is turned (rolled) about its direction, in turns, at random
    /// between these (the game's initial rotation): anywhere for most.
    turn: (f32, f32),
    /// Where it starts: this far along the effect's direction (metres, at random
    /// between these) and up to `around` off that line (the game's initial
    /// location); at the effect's point for most.
    along: (f32, f32),
    around: f32,
    /// A steam's: how many of its puffs it lets out (`FxQueue::spawn_puff`; 0: all),
    /// as many as its cooked peak holds (its emitter keeps no more particles than
    /// that out at once: see SCATTER_MUZZLE).
    puffs: u32,
}

const E: Emitter = Emitter {
    tex: Tex::LightBurst,
    additive: true,
    align: Align::Camera,
    count: (1, 1),
    life: (0.1, 0.1),
    width: (1.0, 1.0),
    length: (1.0, 1.0),
    grow: 1.0,
    speed: (0.0, 0.0),
    spread: 0.0,
    gravity: 0.0,
    color: [1.0, 1.0, 1.0, 1.0],
    delay: 0.0,
    need: Need::Always,
    width_curve: &[],
    length_curve: &[],
    spin: (0.0, 0.0),
    speed_curve: &[],
    fade_curve: &[],
    heat_curve: &[],
    turn: (0.0, 1.0),
    along: (0.0, 0.0),
    around: 0.0,
    puffs: 0,
};

/// Ion_Cannon_MuzzleFlash_STAT's own size: 2.235 m long, 1.118 m across.
const ION_CORE_LENGTH: f32 = 2.235;
const ION_CORE_ACROSS: f32 = 1.118;

/// Subsonic_Repeater_Tracer_FX Tracer_Trail: the trail stretches from 2x to about
/// 45x its length as it flies.
const ION_TRAIL_LENGTH: &[f32] = &[
    2.0, 2.805, 8.32, 14.351, 19.754, 24.555, 28.779, 32.452, 35.599, 38.247, 40.42, 42.143,
    43.444, 44.347, 44.877, 45.061, 44.811, 44.424, 44.261, 44.205, 44.14,
];

// Sizes and speeds below are the particle systems' values (centimetres) in metres;
// `size_scale` scales the weapons' effects to the mesh they play on.

/// AssaultRifleArm_MuzzleFlash_FX's Spark_Bolts (which other effects' sparks share):
/// each streak starts out 7x as long and shrinks back (SizeMultiplyLife).
const RIFLE_SPARK_LENGTH: &[f32] = &[
    7.0, 6.45, 5.9, 5.35, 4.8, 4.25, 3.7, 3.15, 2.6, 2.467, 2.333, 2.2, 2.067, 1.933, 1.8, 1.667,
    1.533, 1.4, 1.267, 1.133, 1.0,
];

/// The Neutron Assault Rifle's flash: its light alone (light_for). The flash itself
/// is the game's own particle system, AssaultRifleArm_MuzzleFlash_FX
/// (fx/weapons_fx.txt's NeutronRifle_MuzzleFlash_FX, which levelfx.rs plays on the
/// barrel that fired: main.rs's muzzle_system): its flame cap and the black smoke
/// cap round it (Flash_Blast and Flash_Black, on the cap mesh), spark bolts, a faint
/// haze of smoke and the glow's darkening (MOD_Glow_Flash), as the game draws them.
/// The demo drew it here as three pairs of crossed flame sprites, paler than the
/// game's cap (its colour 1.2, 1, 0.5 where the cap's starts at 3, 3, 1) and
/// without the black caps: a small, pale flash beside the game's fiery burst. (Its
/// dark smoke, Smoke_Dark, the game never draws: its material,
/// Clouds_Basic_MAT, has no shaders for the sub-UV sprites with dynamic parameters
/// the emitter is drawn as, tools/levelfx.py's _undrawable.)
const RIFLE_MUZZLE: &[Emitter] = &[];

/// The Subsonic Repeater's flash: its light alone (light_for). The flash itself is
/// the game's own particle system, Subsonic_Repeater_MuzzleFlash_FX
/// (fx/weapons_fx.txt), which levelfx.rs plays on the barrel that fired and on the
/// gun's MuzzleFlash2 socket with every shot (main.rs's muzzle_system and the
/// gun's GunSockets::flash), at the robot's size (arsenal.rs's GunFx::robot_sized):
/// up to six caps of white-blue squiggles on the cap mesh (Flash_Cap), up to six
/// copies of the Ion core mesh bursting out along the barrel (Flash_Blast; each
/// burst asks for 0 to 6, uncapped as the rifle's caps are), a starburst on the
/// cap mesh (BlastCap_glow) and its glows
/// (MOD_Glow and the flashes, modulated: they brighten what is behind them). The
/// demo drew it here with flat sprites for the meshes, big sheets with straight
/// edges standing out from the gun, where the game's caps and cores wrap round the
/// barrel in a tight white-blue burst.
const SUBSONIC_MUZZLE: &[Emitter] = &[];

/// The Photon Burst Rifle's flash: its light alone (light_for). The flash itself is
/// the game's own particle system, Photon_BurstRifle_MuzzleFlash_Fx
/// (fx/weapons_fx.txt), which levelfx.rs plays on its barrel with every round of a
/// burst (main.rs's muzzle_system), at the robot's size: its blue streaks along the
/// barrel (Streak, and Streak_Dup half the time), two blast caps, chunky bits and
/// a starburst glow on the cap mesh, a flame on the flash mesh and the glow's
/// brightening (ModGlow). The demo drew it here as four flat sprites (a streak, the
/// StarBurst star, the MuzzleFlash_Light flames and a glow).
const PHOTON_MUZZLE: &[Emitter] = &[];

/// The car's guns' flash: its light alone (light_for). The flash itself is the
/// game's own particle system, AssaultRifle_VEH_MuzzleFlash_FX (fx/weapons_fx.txt:
/// the rifle's flash, a size down, as the Battle Sentry's gun has it too), which
/// riot.rs keeps on the gun that fired (main.rs's fire and remote_shot): its flame
/// caps and black smoke caps on the cap mesh, spark bolts, smoke and the glow's
/// darkening. The demo drew it here as a flame sprite and a glow. (Its dark smoke,
/// Smoke_Dark, the game never draws: see RIFLE_MUZZLE.)
const CAR_MUZZLE: &[Emitter] = &[];

/// AssaultRifle_VEH_Squib_FX: flash (bLast, a twentieth of a second after the rest:
/// its EmitterDelay), fire spears and embers. Its bullet hole is BULLET_HOLE, left on
/// walls only. Left out: its dark smoke (Smoke_Dark_Export, 4-5 puffs that swelled
/// to 5x), which the game never draws, as the muzzle flash's (RIFLE_MUZZLE).
const BULLET_IMPACT: &[Emitter] = &[
    Emitter {
        tex: Tex::MuzzleFlash2,
        life: (0.09, 0.15),
        // bLast / bLast_locked: 25-80 cm.
        width: (0.3, 0.7),
        length: (0.3, 0.7),
        color: [1.8, 1.2, 0.7, 1.0],
        delay: 0.05,
        ..E
    },
    Emitter {
        tex: Tex::Scrapmaker,
        align: Align::Velocity,
        count: (3, 6),
        life: (0.09, 0.15),
        width: (0.35, 0.5),
        length: (0.7, 1.2),
        grow: 1.5,
        speed: (1.0, 2.5),
        spread: 1.5,
        color: [1.5, 1.2, 0.9, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::Embers,
        count: (5, 7),
        life: (0.3, 0.55),
        width: (0.12, 0.2),
        length: (0.12, 0.2),
        grow: 0.4,
        speed: (3.0, 5.0),
        spread: 3.5,
        gravity: -12.0,
        color: [2.5, 1.6, 0.8, 1.0],
        ..E
    },
];

/// The bullet hole's opacity over its life (its alpha over life), and how hot its
/// rim glows: the decal's colour (5, 3, 1) cooling through its colour over life and
/// colour scale over life, which fall the same way.
const HOLE_FADE: &[f32] = &[
    1.0, 0.993, 0.972, 0.939, 0.896, 0.844, 0.784, 0.718, 0.648, 0.575, 0.5, 0.425, 0.352, 0.282,
    0.216, 0.156, 0.104, 0.061, 0.028, 0.007, 0.0,
];
const HOLE_HEAT: &[f32] = &[
    1.0, 0.986, 0.945, 0.882, 0.803, 0.712, 0.615, 0.516, 0.42, 0.331, 0.25, 0.181, 0.124, 0.08,
    0.047, 0.024, 0.011, 0.004, 0.001, 0.0, 0.0,
];

/// AssaultRifle_VEH_Squib_FX's decal emitter (-Z, ENV_ASH_BulletHoles): a metre-wide
/// decal of two bullet holes, whose rims glow orange-hot (the material's rim mask,
/// lit 4x by a metal cube map and 2x again, times the decal's colour) and cool as it
/// fades, over 0-7 seconds.
const BULLET_HOLE: &[Emitter] = &[Emitter {
    tex: Tex::BulletHole,
    additive: false,
    align: Align::Decal,
    life: (0.0, 7.0),
    width: (1.0, 1.0),
    length: (1.0, 1.0),
    color: [5.0, 3.0, 1.0, 1.0],
    need: Need::WithTexture,
    fade_curve: HOLE_FADE,
    heat_curve: HOLE_HEAT,
    ..E
}];

/// ReloadSteam_Med_FX SmokeSteam: a cloud of steam rising from the gun through the
/// reload (5 wisps, then 20 a second; here one of these bursts of five every quarter
/// of a second, which main.rs's fire() lets out at the gun's shell port as it
/// moves), each 1.2-2.5 m across, starting at a quarter of that and swelling to
/// 1.75x, for 1.5-2 s, fading in and out (its alpha over life, up to 0.75). Its
/// cooked peak is 15, and its wisps outlive its 1.5 s, so it lets out 15 in all,
/// three puffs, by half a second in (the demo let one out every quarter of a second
/// for the whole 1.5 s, 35 wisps). Left out: its heat haze (Distort) and the thin
/// jets (SteamJet_Dup).
const STEAM_FADE: &[f32] = &[
    0.0, 0.022, 0.081, 0.167, 0.272, 0.385, 0.498, 0.601, 0.684, 0.738, 0.753, 0.727, 0.667, 0.581,
    0.479, 0.369, 0.259, 0.159, 0.076, 0.021, 0.0,
];
const STEAM: Emitter = Emitter {
    tex: Tex::SmokeThin,
    additive: false,
    count: (5, 5),
    life: (1.5, 2.0),
    width: (0.3, 0.62),
    length: (0.3, 0.62),
    grow: 7.0,
    speed: (0.1, 0.3),
    spread: 0.3,
    color: [0.5, 0.425, 0.351, 1.0],
    need: Need::WithTexture,
    fade_curve: STEAM_FADE,
    puffs: 3,
    ..E
};
const RELOAD_STEAM: &[Emitter] = &[STEAM];

/// ReloadSteam_Med02_FX SmokeSteam, which the Scatter Blaster lets out with the
/// above: a paler grey cloud, 1.2-2 m across at full size and up to half opaque; its
/// cooked peak is 5, a single puff.
const STEAM02_FADE: &[f32] = &[
    0.0, 0.028, 0.099, 0.197, 0.303, 0.401, 0.472, 0.5, 0.492, 0.468, 0.432, 0.387, 0.335, 0.279,
    0.221, 0.165, 0.113, 0.068, 0.032, 0.008, 0.0,
];
const SCATTER_RELOAD_STEAM: &[Emitter] = &[STEAM, STEAM02];
const STEAM02: Emitter = Emitter {
    width: (0.3, 0.5),
    length: (0.3, 0.5),
    color: [0.5, 0.5, 0.5, 1.0],
    fade_curve: STEAM02_FADE,
    puffs: 1,
    ..STEAM
};

/// Sm_Shell3_FX and Med_Shell2_FX SmokeSteam_Export: the steam each spent shell's
/// effect lets out of the shell port, each wisp living 0.5-1 s and swelling from a
/// quarter of 0.8-3.5 m to 1.75x it, as opaque as the reload's steam but grey. Its
/// burst asks for 5 and then 20 a second for 1.5 s, and the rifle's shell keeps its
/// cooked peak of 1 (the Scatter Blaster's, MED_SHELL_STEAM, 2): a wisp at a time,
/// the next as the last one goes, so two in turn (main.rs's fire() lets them out at
/// the start and 0.75 s on, their middling life). The demo let out a puff of five
/// every quarter of a second, 30 a shell, five steaming shells a second: the white
/// cloud round the rifle as it fired. The rifle's lets it out for one shot in five;
/// the Scatter Blaster's with every shot.
const SHELL_STEAM: &[Emitter] = &[Emitter {
    count: (1, 1),
    life: (0.5, 1.0),
    width: (0.2, 0.875),
    length: (0.2, 0.875),
    color: [0.5, 0.5, 0.5, 1.0],
    puffs: 0,
    ..STEAM
}];
const MED_SHELL_STEAM: &[Emitter] = &[Emitter {
    count: (2, 2),
    ..SHELL_STEAM[0]
}];

/// Subsonic_Repeater_Squib_FX Black_Spears: dark spears that stretch to two and a
/// half times their length in the first sixth of their life and on across to four
/// times their width (SizeMultiplyLife), holding dark and then fading.
const SUBSONIC_SPEARS_WIDTH: &[f32] = &[
    1.0, 1.0, 1.0, 1.051, 1.152, 1.253, 1.354, 1.456, 1.557, 1.658, 1.759, 1.929, 2.145, 2.36,
    2.576, 2.791, 3.007, 3.222, 3.438, 3.653, 3.869,
];
const SUBSONIC_SPEARS_LENGTH: &[f32] = &[
    1.0, 1.6, 2.2, 2.5, 2.5, 2.5, 2.5, 2.5, 2.5, 2.5, 2.5, 2.5, 2.5, 2.5, 2.5, 2.5, 2.5, 2.5, 2.5,
    2.5, 2.5,
];
const SUBSONIC_SPEARS_FADE: &[f32] = &[
    1.0, 1.002, 1.006, 1.01, 1.014, 1.017, 1.015, 1.01, 0.999, 0.982, 0.956, 0.921, 0.874, 0.816,
    0.744, 0.658, 0.555, 0.436, 0.298, 0.14, 0.0,
];
/// Its Black_Spears_Locked: a dark burst facing the camera that swells from a
/// fifth of its size to two and a half times it.
const SUBSONIC_BURST_SIZE: &[f32] = &[
    0.2, 0.52, 0.84, 1.051, 1.152, 1.253, 1.354, 1.456, 1.557, 1.658, 1.759, 1.836, 1.897, 1.957,
    2.017, 2.078, 2.138, 2.198, 2.259, 2.319, 2.379,
];
/// Its Spark_Bolts' speed: three times their own at first, slowing to 1.1 times
/// it by the end (VelocityOverLifetime, along the normal).
const SUBSONIC_SPARK_SPEED: &[f32] = &[
    3.0, 2.705, 2.41, 2.115, 1.82, 1.525, 1.23, 1.221, 1.211, 1.202, 1.193, 1.184, 1.174, 1.165,
    1.156, 1.146, 1.137, 1.128, 1.119, 1.11, 1.1,
];

/// Subsonic_Repeater_Squib_FX, where a round strikes: one or two dark spears lying out
/// from the point along the surface (Black_Spears: 4x4_Bursts as a mask (BurstsMask),
/// 1.5-2.5 m, stretching out, 0.08-0.17 s) and a dark burst facing the camera
/// (Black_Spears_Locked, 1-2 m, swelling, 0.1-0.35 s), a blue flash a twentieth of a
/// second later (Flash_Glow and Flash_Glow_Locked: 0.75-2.8 m, colour 1, 1.5, 2,
/// swelling to twice that), a burst of white-blue bits (Bits_Flash: the StarBurst
/// greypack's chunks, 1-3 m, swelling to three times that and fading over 0.15-0.2 s,
/// colour 1-2, 1.5-1.7, 1.3-2 at twice its strength) and nine sparks thrown out at
/// 3-12 m/s and up to 10 m/s sideways (Spark_Bolts, like the rifle's muzzle sparks,
/// blue-white). Its bullet hole is SUBSONIC_HOLE, left on walls only. Left out: the
/// glows that darken or brighten what is behind them (MOD_Glow_Flash, modulated).
const SUBSONIC_IMPACT: &[Emitter] = &[
    Emitter {
        tex: Tex::BurstsMask,
        additive: false,
        align: Align::Velocity,
        count: (1, 2),
        life: (0.08, 0.17),
        width: (1.5, 2.5),
        length: (1.5, 2.5),
        speed: (0.0, 0.0),
        spread: 1.0,
        color: [0.01, 0.01, 0.01, 1.0],
        need: Need::WithTexture,
        width_curve: SUBSONIC_SPEARS_WIDTH,
        length_curve: SUBSONIC_SPEARS_LENGTH,
        fade_curve: SUBSONIC_SPEARS_FADE,
        ..E
    },
    Emitter {
        tex: Tex::BurstsMask,
        additive: false,
        count: (1, 2),
        life: (0.1, 0.35),
        width: (1.0, 2.0),
        length: (1.0, 2.0),
        color: [0.01, 0.01, 0.01, 1.0],
        need: Need::WithTexture,
        width_curve: SUBSONIC_BURST_SIZE,
        length_curve: SUBSONIC_BURST_SIZE,
        fade_curve: &[1.0, 0.0],
        ..E
    },
    Emitter {
        tex: Tex::SphereGlow,
        count: (2, 2),
        life: (0.045, 0.06),
        width: (0.75, 2.8),
        length: (0.75, 2.8),
        grow: 2.0,
        color: [1.0, 1.5, 2.0, 1.0],
        delay: 0.05,
        ..E
    },
    Emitter {
        tex: Tex::StarBurstChunks,
        count: (1, 3),
        life: (0.15, 0.2),
        width: (1.0, 3.0),
        length: (1.0, 3.0),
        grow: 3.0,
        color: [1.6, 2.0, 2.6, 1.0],
        fade_curve: &[1.0, 0.0],
        ..E
    },
    Emitter {
        tex: Tex::SparkTrail,
        align: Align::Velocity,
        count: (9, 9),
        life: (0.3, 0.5),
        width: (0.08, 0.2),
        length: (0.16, 0.4),
        speed: (3.0, 12.0),
        spread: 10.0,
        color: [1.2, 2.6, 2.8, 1.0],
        length_curve: RIFLE_SPARK_LENGTH,
        speed_curve: SUBSONIC_SPARK_SPEED,
        ..E
    },
];

/// Subsonic_Repeater_Squib_FX's decal emitter: the rifle's bullet hole
/// (BULLET_HOLE) with its rim glowing cyan (the decal's colour 0.5, 3, 3), cooling
/// as it fades over 0-7 s.
const SUBSONIC_HOLE: &[Emitter] = &[Emitter {
    color: [0.5, 3.0, 3.0, 1.0],
    ..BULLET_HOLE[0]
}];

/// ReloadSteam_FX, the Subsonic Repeater's: ReloadSteam_Med_FX's steam (STEAM) at
/// half the rate (10 wisps a second), its cooked peak 10: four puffs.
const SUBSONIC_RELOAD_STEAM: &[Emitter] = &[Emitter {
    count: (2, 3),
    puffs: 4,
    ..STEAM
}];

/// Photon_BurstRifle_Squib_Fx: a streaky blue star (225-475 cm), the StarBurst
/// chunks (100-150 cm), a hot centre (20-80 cm) and six streaks thrown out
/// (200-300 cm long, 60-100 cm wide). Colours toned down as for the muzzle flash.
const PHOTON_IMPACT: &[Emitter] = &[
    Emitter {
        tex: Tex::LightStar,
        life: (0.1, 0.15),
        width: (2.25, 4.75),
        length: (2.25, 4.75),
        color: [0.6, 1.6, 2.8, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::StarBurstChunks,
        life: (0.12, 0.18),
        width: (1.0, 1.5),
        length: (1.0, 1.5),
        grow: 1.4,
        color: [0.5, 1.0, 2.0, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::SphereGlow,
        life: (0.08, 0.12),
        width: (0.2, 0.8),
        length: (0.2, 0.8),
        color: [1.5, 2.0, 3.0, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::Streak,
        align: Align::Velocity,
        count: (6, 6),
        life: (0.1, 0.16),
        width: (0.6, 1.0),
        length: (2.0, 3.0),
        speed: (4.0, 8.0),
        spread: 6.0,
        color: [0.6, 1.2, 2.4, 1.0],
        ..E
    },
];

/// The Scatter Blaster's flash: its light alone (light_for). The flash itself is
/// the game's own particle system, Scatterblaster_MuzzleFlash_FX
/// (fx/weapons_fx.txt), which levelfx.rs plays on its barrel with every shot
/// (main.rs's muzzle_system), at the robot's size: its burst of fire on the cap
/// mesh (MuzzleCore, half the time: its burst asks for 0 or 1), white-hot sparks
/// thrown forward (SparksSM, SparksLG), smoke rolling out ahead of the barrel
/// (SmokeFront, SmokeBlendLite, SmokeBlendFB), the heat-distortion flash and rings
/// and the glow's brightening (ModFlash). The demo drew it here with flat sprites:
/// crossed flames for the cap mesh, a glow, the sparks and two smokes, one of them
/// the dark smoke the game never draws (SmokeDark: its material, FireSmoke_Lrg_Mat,
/// has no shaders for the sub-UV sprites with dynamic parameters it is drawn as,
/// tools/levelfx.py's _undrawable), and without the distortion.
///
/// The Scatter Blaster's effects (this, its impact and its tracer) have no more
/// particles out at once than the game cooks for each emitter (its LOD level's
/// PeakActiveParticles, levelfx.rs's EmitterDef::peak): the game's arrays are that
/// big, and a burst asking for more spawns no more. The smoke's bursts ask for
/// 12-15 puffs, and the game has 5: its videos show the light smoke and the few
/// streaks that leaves (levelfx.rs's PEAK_CAPPED has the flash, and the
/// shotgunners' and Fireblast's effects, which keep to theirs too).
const SCATTER_MUZZLE: &[Emitter] = &[];

/// Scatterblaster_Squib_FX, where each pellet strikes: a warm flash on the surface
/// (Flash_Glow, 2-4 m) and a glow that lingers as the metal cools (Burn_Glow,
/// 1-1.5 m for 1.6-2 s), sparks bouncing off (Sparks_Small, Sparks_Large) and a fast
/// one (Sparks_Large_Powerful), dark smoke and embers (the rifle's, two puffs and
/// two embers: their cooked peaks, where their bursts ask for 4-5 and 5-7; see
/// SCATTER_MUZZLE). The game's glows darken or brighten what is behind them
/// (modulated); here they are added light. Left out: the chips of the surface
/// (Impact_Clip).
const SCATTER_IMPACT: &[Emitter] = &[
    Emitter {
        tex: Tex::LightBurst,
        life: (0.2, 0.25),
        width: (2.0, 4.0),
        length: (2.0, 4.0),
        color: [0.45, 0.32, 0.12, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::LightBurst,
        life: (1.6, 2.0),
        width: (1.0, 1.5),
        length: (1.0, 1.5),
        color: [0.35, 0.18, 0.05, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::MuzzleFlash2,
        align: Align::Velocity,
        count: (1, 3),
        life: (0.3, 0.65),
        width: (0.05, 0.08),
        length: (0.3, 0.5),
        speed: (1.0, 5.0),
        spread: 2.0,
        gravity: -9.8,
        color: [1.5, 1.2, 1.0, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::MuzzleFlash2,
        align: Align::Velocity,
        count: (1, 3),
        life: (0.2, 0.65),
        width: (0.08, 0.12),
        length: (0.5, 0.9),
        speed: (2.0, 8.0),
        spread: 2.0,
        gravity: -9.8,
        color: [2.0, 1.3, 0.9, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::MuzzleFlash2,
        align: Align::Velocity,
        life: (0.05, 0.19),
        width: (0.06, 0.09),
        length: (0.8, 1.4),
        speed: (9.0, 18.0),
        spread: 0.25,
        color: [2.0, 1.3, 0.9, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::CloudsDark,
        additive: false,
        count: (2, 2),
        life: (0.3, 0.9),
        width: (0.75, 1.0),
        length: (0.75, 1.0),
        grow: 2.5,
        speed: (4.0, 6.0),
        spread: 0.3,
        color: [1.0, 1.0, 1.0, 0.55],
        ..E
    },
    Emitter {
        tex: Tex::Embers,
        count: (2, 2),
        life: (0.3, 0.55),
        width: (0.12, 0.2),
        length: (0.12, 0.2),
        grow: 0.4,
        speed: (3.0, 5.0),
        spread: 3.5,
        gravity: -12.0,
        color: [2.5, 1.6, 0.8, 1.0],
        ..E
    },
];

/// The Nucleon Charge Rifle's flash: its red light alone (light_for). The flash
/// itself is the game's own particle system, NucleonRifle_MuzzleFlash_FX
/// (fx/weapons_fx.txt), which levelfx.rs plays on its barrel with every shot
/// (main.rs's muzzle_system), at the robot's size: up to six flame caps bursting out
/// of the barrel on the flash mesh (Flash_Blast_Export, in PathBlaster_Muzzle_MAT),
/// a cloud of fiery smoke rolling out ahead of it (SmokeFront_Export: 12-15 puffs)
/// and glowing puffs in it (SmokeBlendLite_Export: up to five). The demo drew it
/// here with flat sprites: the flames crossed on the cap mesh, the smoke and the
/// puffs. The Path Blaster's muzzle flash has the same three emitters.
const NUCLEON_MUZZLE: &[Emitter] = &[];

/// Squib_SniperRifle_FX's flares: they shrink from 1.5 times their size to a tenth
/// (SizeMultiplyLife), at full strength for the first 30% of their life and then
/// fading (alpha over life), cooling from white-hot (colour over life).
const NUCLEON_FLARE_SIZE: &[f32] = &[1.5, 0.1];
const NUCLEON_FLARE_FADE: &[f32] = &[
    1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.987, 0.951, 0.896, 0.825, 0.741, 0.648, 0.55, 0.45, 0.352,
    0.259, 0.175, 0.104, 0.049, 0.013, 0.0,
];
const NUCLEON_FLARE_HEAT: &[f32] = &[1.0, 0.9, 0.75, 0.55, 0.35, 0.2];
/// Its fire burst (bLast): swelling from nothing to nearly twice its size by the
/// middle of its life and back (SizeMultiplyLife).
const NUCLEON_BURST_SIZE: &[f32] = &[
    0.0, 0.208, 0.417, 0.625, 0.834, 1.042, 1.251, 1.459, 1.667, 1.876, 1.914, 1.701, 1.489, 1.276,
    1.063, 0.851, 0.638, 0.425, 0.213, 0.0,
];
/// Its lingering fire (FireLinger): swelling from nothing to its full size, bright
/// for the first fifth of its life and then a dim glow (its colour over life, 7 to
/// 0.3 and on down).
const NUCLEON_FIRE_SIZE: &[f32] = &[
    0.0, 0.071, 0.136, 0.194, 0.247, 0.295, 0.339, 0.381, 0.42, 0.457, 0.494, 0.531, 0.569, 0.609,
    0.651, 0.697, 0.746, 0.8, 0.86, 0.927, 1.0,
];
const NUCLEON_FIRE_HEAT: &[f32] = &[
    1.0, 0.76, 0.52, 0.28, 0.05, 0.045, 0.04, 0.035, 0.03, 0.02, 0.0,
];
/// Its glow on the surface (Burn_Glow), coming up over the first half of its life
/// and dying away.
const NUCLEON_BURN_FADE: &[f32] = &[0.0, 0.42, 0.7, 0.85, 0.9, 0.85, 0.7, 0.5, 0.3, 0.12, 0.0];
/// Its smoke (Smoke_Dup): fading in to 70% and out (alpha over life).
const NUCLEON_IMPACT_SMOKE_FADE: &[f32] = &[
    0.0, 0.113, 0.355, 0.595, 0.7, 0.689, 0.665, 0.634, 0.598, 0.562, 0.53, 0.508, 0.494, 0.455,
    0.391, 0.31, 0.224, 0.14, 0.068, 0.019, 0.0,
];

/// Squib_SniperRifle_FX, where a Nucleon Charge Rifle round strikes: white-hot
/// flares shooting out along the surface's normal (Flare_Tall: smokeball_01, ten,
/// 25-35 cm wide and 0.75-1.5 m long, 0.2-0.5 s), long white flares and a white
/// burst (Flare_Tall_Dup and _Dup_Dup: Spark_Base_CLR, 0.1-0.15 s), an orange glow
/// (GLOW, 0.6-1.2 m, 0.5-1 s), showers of orange sparks (Sparks_bolts and
/// Sparks_bolts_Long: Sparks_01, 2-10 m/s; Spark_Bolts: Spark_Trail, as the
/// Subsonic Repeater's impact sparks, hot yellow), a flash of bits (Bits_Flash: the
/// StarBurst chunks), a fire burst a twentieth of a second later (bLast and
/// bLast_locked: MuzzleCap_ClusterBomb_CLR, swelling to some 2 m and back over
/// 0.15-0.5 s), metal chips thrown out (Debris: MetalDebris, 0.2-0.4 s), a ball of
/// fire that flares and smoulders (FireLinger: SmokeBall, 0.9-1.2 s), a glow left
/// on the surface (Burn_Glow, 2-3 m, 1.6-2 s; the game's brightens the surface,
/// modulated: here it is added light) and grey smoke (Smoke_Dup: SmokeThin). The
/// rings around the shot's last metres are its tracer's (NUCLEON_RINGS). Left out:
/// the chips of the surface's own material (Impact_Clip).
const NUCLEON_IMPACT: &[Emitter] = &[
    Emitter {
        tex: Tex::Flare,
        align: Align::Velocity,
        count: (10, 10),
        life: (0.2, 0.5),
        width: (0.25, 0.35),
        length: (0.75, 1.5),
        speed: (1.5, 2.0),
        spread: 0.3,
        color: [2.5, 2.2, 1.5, 1.0],
        width_curve: NUCLEON_FLARE_SIZE,
        length_curve: NUCLEON_FLARE_SIZE,
        fade_curve: NUCLEON_FLARE_FADE,
        heat_curve: NUCLEON_FLARE_HEAT,
        ..E
    },
    Emitter {
        tex: Tex::SparkBase,
        align: Align::Velocity,
        count: (4, 6),
        life: (0.1, 0.15),
        width: (0.35, 1.0),
        length: (1.5, 4.0),
        speed: (1.5, 2.0),
        spread: 1.0,
        color: [2.2, 2.2, 2.2, 1.0],
        width_curve: NUCLEON_FLARE_SIZE,
        length_curve: NUCLEON_FLARE_SIZE,
        fade_curve: NUCLEON_FLARE_FADE,
        need: Need::WithTexture,
        ..E
    },
    Emitter {
        tex: Tex::SparkBase,
        count: (2, 3),
        life: (0.1, 0.15),
        width: (1.0, 3.0),
        length: (1.0, 3.0),
        color: [1.8, 1.8, 1.8, 1.0],
        width_curve: NUCLEON_FLARE_SIZE,
        length_curve: NUCLEON_FLARE_SIZE,
        fade_curve: NUCLEON_FLARE_FADE,
        need: Need::WithTexture,
        ..E
    },
    Emitter {
        tex: Tex::SphereGlow,
        count: (3, 3),
        life: (0.5, 1.0),
        width: (0.6, 1.2),
        length: (0.6, 1.2),
        color: [1.2, 0.55, 0.05, 1.0],
        heat_curve: &[1.0, 0.5, 0.2, 0.05],
        ..E
    },
    Emitter {
        tex: Tex::Sparks,
        align: Align::Velocity,
        count: (25, 35),
        life: (0.2, 0.4),
        width: (0.04, 0.06),
        length: (0.25, 0.35),
        speed: (2.0, 10.0),
        spread: 4.0,
        gravity: -9.8,
        color: [3.0, 1.8, 0.5, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::Sparks,
        align: Align::Velocity,
        count: (20, 30),
        life: (0.3, 0.6),
        width: (0.03, 0.05),
        length: (0.6, 1.5),
        speed: (0.5, 10.0),
        spread: 2.0,
        gravity: -9.8,
        color: [2.6, 1.3, 0.35, 1.0],
        length_curve: &[1.0, 0.2, 0.1],
        ..E
    },
    Emitter {
        tex: Tex::SparkTrail,
        align: Align::Velocity,
        count: (9, 9),
        life: (0.3, 0.5),
        width: (0.08, 0.2),
        length: (0.16, 0.4),
        speed: (3.0, 12.0),
        spread: 10.0,
        color: [3.0, 2.4, 1.0, 1.0],
        length_curve: RIFLE_SPARK_LENGTH,
        speed_curve: SUBSONIC_SPARK_SPEED,
        heat_curve: &[1.0, 0.6, 0.35],
        ..E
    },
    Emitter {
        tex: Tex::StarBurstChunks,
        count: (3, 4),
        life: (0.15, 0.2),
        width: (1.0, 3.0),
        length: (1.0, 3.0),
        grow: 3.0,
        color: [1.6, 1.5, 1.3, 1.0],
        fade_curve: &[1.0, 0.0],
        ..E
    },
    Emitter {
        tex: Tex::MuzzleCap,
        count: (3, 5),
        life: (0.15, 0.5),
        width: (0.8, 1.4),
        length: (0.8, 1.4),
        speed: (0.2, 0.6),
        color: [2.4, 1.2, 0.6, 1.0],
        delay: 0.05,
        width_curve: NUCLEON_BURST_SIZE,
        length_curve: NUCLEON_BURST_SIZE,
        fade_curve: &[1.0, 1.0],
        heat_curve: &[2.0, 1.0, 0.5],
        need: Need::WithTexture,
        ..E
    },
    Emitter {
        tex: Tex::MuzzleCap,
        count: (3, 5),
        life: (0.15, 0.5),
        width: (0.6, 1.0),
        length: (0.6, 1.0),
        speed: (0.8, 1.2),
        color: [2.0, 1.0, 1.0, 1.0],
        delay: 0.05,
        width_curve: NUCLEON_BURST_SIZE,
        length_curve: NUCLEON_BURST_SIZE,
        fade_curve: &[1.0, 1.0],
        heat_curve: &[2.0, 1.0, 0.5],
        need: Need::WithTexture,
        ..E
    },
    Emitter {
        tex: Tex::MetalDebris,
        additive: false,
        count: (25, 35),
        life: (0.2, 0.4),
        width: (0.07, 0.15),
        length: (0.07, 0.15),
        speed: (6.0, 10.0),
        spread: 4.0,
        gravity: -15.0,
        color: [0.55, 0.55, 0.55, 1.0],
        need: Need::WithTexture,
        ..E
    },
    Emitter {
        tex: Tex::SmokeBall,
        count: (5, 6),
        life: (0.87, 1.2),
        width: (0.6, 1.0),
        length: (0.6, 1.0),
        speed: (0.5, 1.5),
        spread: 0.5,
        color: [1.4, 0.55, 0.12, 1.0],
        width_curve: NUCLEON_FIRE_SIZE,
        length_curve: NUCLEON_FIRE_SIZE,
        fade_curve: &[1.0, 1.0],
        heat_curve: NUCLEON_FIRE_HEAT,
        ..E
    },
    Emitter {
        tex: Tex::LightBurst,
        count: (2, 2),
        life: (1.6, 2.0),
        width: (2.0, 3.0),
        length: (2.0, 3.0),
        color: [0.45, 0.22, 0.05, 1.0],
        fade_curve: NUCLEON_BURN_FADE,
        ..E
    },
    Emitter {
        tex: Tex::SmokeThin,
        additive: false,
        count: (6, 8),
        life: (0.5, 0.75),
        width: (0.5, 1.0),
        length: (0.5, 1.0),
        grow: 3.0,
        speed: (1.5, 3.0),
        spread: 1.5,
        color: [0.83, 0.84, 0.9, 1.0],
        fade_curve: NUCLEON_IMPACT_SMOKE_FADE,
        need: Need::WithTexture,
        ..E
    },
];

/// NucleonRifle_Tracer_FX Rings_Export: see-through rings hanging round the shot's
/// line (Ring_CLR on its distortion material, which shows them pale), here five
/// strung along its last 0.3-3 m before it struck, each facing the camera, swelling
/// from half its size (0.5-1.1 m) to three times it and fading over 0.3-0.375 s as
/// it drifts on along the shot.
const NUCLEON_RINGS: &[Emitter] = &[Emitter {
    tex: Tex::Ring,
    additive: false,
    count: (4, 6),
    life: (0.3, 0.375),
    width: (0.25, 0.55),
    length: (0.25, 0.55),
    grow: 6.0,
    speed: (-3.0, 0.0),
    color: [1.0, 1.0, 1.0, 0.7],
    need: Need::WithTexture,
    fade_curve: &[1.0, 0.0],
    along: (0.3, 3.0),
    around: 0.15,
    ..E
}];

/// NucleonRifle_Charged_FX Ring02: once the rifle is fully charged, glowing red
/// rings round its barrel, 20 a second, each drifting back along it for half a
/// second (Ring_Mesh_STAT with Scanner_Beam_Red; here Ring_CLR across the barrel).
const NUCLEON_CHARGED: &[Emitter] = &[Emitter {
    tex: Tex::Ring,
    align: Align::Decal,
    life: (0.5, 0.5),
    width: (0.35, 0.35),
    length: (0.35, 0.35),
    grow: 1.6,
    speed: (-0.4, -0.4),
    color: [2.0, 0.35, 0.25, 1.0],
    need: Need::WithTexture,
    fade_curve: &[1.0, 0.0],
    ..E
}];

/// The Path Blaster's flash: its orange light alone (light_for). The flash itself
/// is the game's own particle system, PathBlaster_MuzzleFlash_FX
/// (fx/weapons_fx.txt), which levelfx.rs plays on the barrel that fired with every
/// shot (main.rs's muzzle_system), at the robot's size: its three caps on the dish
/// mesh facing out along the barrel (a white-hot flash, MuzzleCap_Export_Export; a
/// spinning fire burst, MuzzleCore_Export_Export; and a fiery iris ring,
/// MuzzleCap_Dup, in PathBlaster_MuzzleCore_Mat: each half the time, its burst
/// asking for 0 or 1) and the Nucleon Charge Rifle's three (its flame caps, fiery
/// smoke and glowing puffs). The demo drew it here with flat sprites: the caps as
/// flat discs facing along the barrel, and the Nucleon's.
const PATH_MUZZLE: &[Emitter] = &[];

/// PathBlaster_Squib_FX Fire_Export's alpha over life: in to 85% over the first
/// fifth of its life and out.
const PATH_FIRE_FADE: &[f32] = &[
    0.15, 0.332, 0.515, 0.697, 0.841, 0.785, 0.729, 0.673, 0.617, 0.561, 0.505, 0.449, 0.392,
    0.336, 0.28, 0.224, 0.168, 0.112, 0.056, 0.0,
];
/// FireLinger_Export's: in fast and slowly out.
const PATH_LINGER_FADE: &[f32] = &[
    0.0, 0.532, 1.0, 0.988, 0.959, 0.914, 0.856, 0.787, 0.71, 0.627, 0.54, 0.453, 0.366, 0.283,
    0.207, 0.139, 0.082, 0.038, 0.01, 0.0,
];
/// SmokeLinger_Export's: shrinking from one and a half times its size to nothing
/// (SizeMultiplyLife), fading in over the first fifth of its life and out.
#[allow(clippy::approx_constant)]
const PATH_SMOKE_SIZE: &[f32] = &[
    1.5, 1.369, 1.237, 1.106, 0.975, 0.844, 0.712, 0.581, 0.45, 0.318, 0.198, 0.178, 0.158, 0.139,
    0.119, 0.099, 0.079, 0.059, 0.04, 0.02, 0.0,
];
const PATH_SMOKE_FADE: &[f32] = &[
    0.0, 0.261, 0.521, 0.782, 0.989, 0.923, 0.857, 0.791, 0.726, 0.66, 0.594, 0.528, 0.462, 0.396,
    0.33, 0.264, 0.198, 0.132, 0.066, 0.0,
];
/// Fire_A's: swelling to one and a half times its size halfway through its life and
/// back to 1.2 (SizeMultiplyLife); in to 70% by 40% of its life and out (alpha over
/// life); cooling from yellow-white to red over its second half (colour over life:
/// 6, 5, 2 to 1, 0.1, 0.1, as a brightness here).
const PATH_FLAME_SIZE: &[f32] = &[
    1.0, 1.05, 1.1, 1.15, 1.2, 1.25, 1.3, 1.35, 1.4, 1.45, 1.5, 1.47, 1.44, 1.41, 1.38, 1.35, 1.32,
    1.29, 1.26, 1.23, 1.2,
];
const PATH_FLAME_FADE: &[f32] = &[
    0.0, 0.087, 0.175, 0.262, 0.35, 0.438, 0.525, 0.613, 0.7, 0.642, 0.583, 0.525, 0.467, 0.408,
    0.35, 0.292, 0.233, 0.175, 0.117, 0.058, 0.0,
];
const PATH_FLAME_HEAT: &[f32] = &[
    1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.936, 0.872, 0.808, 0.744, 0.679, 0.615, 0.551, 0.487,
    0.423, 0.359, 0.295, 0.231, 0.167,
];

/// PathBlaster_Squib_FX, where a Path Blaster round strikes: a burst of fire
/// (Fire_Export: eight puffs up to a metre around it, 0.2-0.27 s, flaring white-hot
/// and burning orange), fire lingering on (FireLinger_Export: ten puffs swelling over
/// 0.34-0.47 s, bright for the first fifth of their life, as the Nucleon Charge
/// Rifle's), dark smoke (SmokeLinger_Export: ten puffs shrinking away over 0.26-0.78
/// s), a spray of golden sparkles (EmbersVelocity_Export: seven big ember_clip
/// sprites stretched along their flight, 1.25-2.5 m wide and 3.5-4.5 m long, thrown
/// out at up to 2.5 m/s at twice that at first and slowing, gold (8-10, 5, 2),
/// fading out) and flames licking up from it (Fire_A: Flame_CLR, 0.85-1.25 m, for
/// 0.2-0.4 s). The scorch mark it leaves on a wall is PATH_SCORCH. Left out: its heat
/// haze (Shimmer).
const PATH_IMPACT: &[Emitter] = &[
    Emitter {
        tex: Tex::SmokeBall,
        count: (8, 8),
        life: (0.2, 0.27),
        width: (0.6, 1.2),
        length: (0.6, 1.2),
        grow: 1.5,
        speed: (1.0, 1.25),
        spread: 0.5,
        color: [2.2, 0.85, 0.2, 1.0],
        fade_curve: PATH_FIRE_FADE,
        heat_curve: &[1.8, 1.0, 1.0],
        around: 1.0,
        ..E
    },
    Emitter {
        tex: Tex::SmokeBall,
        count: (10, 10),
        life: (0.34, 0.47),
        width: (0.6, 1.0),
        length: (0.6, 1.0),
        speed: (1.25, 1.75),
        spread: 0.5,
        color: [1.4, 0.55, 0.12, 1.0],
        width_curve: NUCLEON_FIRE_SIZE,
        length_curve: NUCLEON_FIRE_SIZE,
        fade_curve: PATH_LINGER_FADE,
        heat_curve: NUCLEON_FIRE_HEAT,
        around: 0.5,
        ..E
    },
    Emitter {
        tex: Tex::SmokeBall,
        additive: false,
        count: (10, 10),
        life: (0.26, 0.78),
        width: (0.6, 1.0),
        length: (0.6, 1.0),
        speed: (1.0, 2.0),
        spread: 1.0,
        color: [0.25, 0.2, 0.17, 0.75],
        width_curve: PATH_SMOKE_SIZE,
        length_curve: PATH_SMOKE_SIZE,
        fade_curve: PATH_SMOKE_FADE,
        around: 0.5,
        ..E
    },
    Emitter {
        tex: Tex::EmberClip,
        align: Align::Velocity,
        count: (7, 7),
        life: (0.3, 0.5),
        width: (1.25, 2.5),
        length: (3.5, 4.5),
        speed: (0.0, 2.5),
        spread: 2.5,
        color: [3.0, 1.6, 0.6, 1.0],
        speed_curve: &[2.0, 0.1],
        fade_curve: &[1.0, 0.0],
        need: Need::WithTexture,
        ..E
    },
    Emitter {
        tex: Tex::Flame,
        count: (1, 2),
        life: (0.2, 0.4),
        width: (0.85, 1.25),
        length: (0.85, 1.25),
        speed: (0.2, 0.5),
        spread: 0.85,
        gravity: 2.0,
        color: [2.4, 1.6, 0.6, 1.0],
        width_curve: PATH_FLAME_SIZE,
        length_curve: PATH_FLAME_SIZE,
        fade_curve: PATH_FLAME_FADE,
        heat_curve: PATH_FLAME_HEAT,
        turn: (-0.25, 0.25),
        need: Need::WithTexture,
        ..E
    },
];

/// PathBlaster_Squib_FX's decal emitter (-Z_Export, DeathDecal_MAT): a scorch mark
/// 1.25-1.6 m across where the round struck (blastmark_glow_CLR's burn, darkening
/// the wall), held for half of its 5.9-11.7 s and then fading.
const PATH_SCORCH: &[Emitter] = &[Emitter {
    tex: Tex::Scorch,
    additive: false,
    align: Align::Decal,
    life: (5.9, 11.7),
    width: (1.25, 1.6),
    length: (1.25, 1.6),
    color: [1.0, 1.0, 1.0, 0.9],
    need: Need::WithTexture,
    fade_curve: &[1.0, 1.0, 0.0],
    ..E
}];

/// The Riot Cannon's: ReloadSteam_FX's steam (as the Subsonic Repeater's) and
/// ReloadSteam_Med02_FX's (as the Path Blaster's), both on its ReloadFX socket.
const RIOT_RELOAD_STEAM: &[Emitter] = &[SUBSONIC_RELOAD_STEAM[0], STEAM02];

/// ReloadSteam_Med02_FX on the Path Blaster's ReloadFX socket (the second of the
/// Scatter Blaster's two steams).
const PATH_RELOAD_STEAM: &[Emitter] = &[STEAM02];

// Jazz's car (FX_Navigation_p). The jets are mesh particles on Boostermesh_03, a
// flared cone 0.5 m long, drawn with a panning beam texture: the sizes below are
// that cone's scaled by the emitters' sizes, and the lengths grow over life as the
// emitters' SizeMultiplyLife curves have them. The cones fire along the effect's
// direction (down under the car, back from the boosters).

/// CarJump_01_FX (JumpBoostSocket_C, under the back of the car): two cyan pulses,
/// two lingering ones a moment later, an orange glow, a lightning ring and a burst
/// of sparks.
const CAR_JUMP: &[Emitter] = &[
    Emitter {
        tex: Tex::LightBeam,
        align: Align::Cone,
        count: (2, 2),
        life: (0.3, 0.4),
        width: (0.35, 0.35),
        length: (0.6, 0.6),
        width_curve: &[1.0, 1.25, 1.5],
        length_curve: &[1.0, 2.0, 3.0, 4.0],
        color: [0.6, 2.0, 2.0, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::LightBeam,
        align: Align::Cone,
        count: (2, 2),
        life: (0.6, 0.8),
        width: (0.35, 0.35),
        length: (0.6, 0.6),
        width_curve: &[1.0, 1.1, 1.2],
        length_curve: &[1.0, 2.0, 2.0],
        color: [0.4, 1.3, 1.3, 1.0],
        delay: 0.2,
        ..E
    },
    Emitter {
        tex: Tex::SphereGlow,
        life: (0.2, 0.3),
        width: (0.4, 0.6),
        length: (0.4, 0.6),
        grow: 5.0,
        color: [2.0, 1.0, 0.5, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::LightningRing,
        count: (4, 4),
        life: (0.1, 0.2),
        width: (0.5, 0.5),
        length: (0.5, 0.5),
        grow: 4.0,
        color: [1.0, 0.5, 0.2, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::SparkTrail,
        align: Align::Velocity,
        count: (30, 30),
        life: (0.1, 0.3),
        width: (0.02, 0.05),
        length: (0.2, 0.4),
        speed: (6.0, 12.0),
        spread: 4.0,
        gravity: -10.0,
        color: [2.5, 2.0, 2.0, 1.0],
        ..E
    },
];

/// CarHover_A_01_FX (Hover_LF/RF/LB/RB, at the wheels, which turn flat to hover):
/// one puff of its jet pulse and sparks. car.rs fires these several times a second.
const CAR_HOVER: &[Emitter] = &[
    Emitter {
        tex: Tex::LightBeam,
        align: Align::Cone,
        life: (0.3, 0.4),
        width: (0.3, 0.3),
        length: (0.4, 0.4),
        width_curve: &[1.0, 1.25, 1.5],
        length_curve: &[1.0, 2.0, 3.0, 4.0],
        color: [0.4, 2.0, 2.0, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::SparkTrail,
        align: Align::Velocity,
        count: (1, 2),
        life: (0.1, 0.3),
        width: (0.015, 0.04),
        length: (0.15, 0.3),
        speed: (6.0, 12.0),
        spread: 3.0,
        gravity: -10.0,
        color: [2.5, 2.0, 2.0, 1.0],
        ..E
    },
];

/// Vehland_Low_Med_FX (a short fall): sparks sprayed out along the ground and dust
/// thrown up and out.
const CAR_LAND_LOW: &[Emitter] = &[
    Emitter {
        tex: Tex::Sparks,
        align: Align::Velocity,
        count: (40, 40),
        life: (0.3, 1.0),
        width: (0.02, 0.03),
        length: (0.3, 0.6),
        speed: (3.0, 10.0),
        spread: 15.0,
        gravity: -15.0,
        color: [2.0, 2.0, 2.0, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::SmokeClouds,
        additive: false,
        count: (10, 12),
        life: (0.3, 0.6),
        width: (0.8, 1.0),
        length: (0.8, 1.0),
        grow: 3.0,
        speed: (1.0, 3.0),
        spread: 1.0,
        color: [0.55, 0.52, 0.5, 0.45],
        ..E
    },
    Emitter {
        tex: Tex::SmokeClouds,
        additive: false,
        count: (8, 8),
        life: (0.3, 0.6),
        width: (0.8, 1.0),
        length: (0.8, 1.0),
        grow: 3.0,
        speed: (0.0, 0.5),
        spread: 5.0,
        color: [0.5, 0.48, 0.46, 0.35],
        ..E
    },
];

/// Vehland_High_XSm_FX (a long fall): the same, with thinner sparks and less dust.
const CAR_LAND_HIGH: &[Emitter] = &[
    Emitter {
        tex: Tex::Sparks,
        align: Align::Velocity,
        count: (40, 40),
        life: (0.3, 1.0),
        width: (0.02, 0.02),
        length: (0.15, 0.6),
        speed: (3.0, 10.0),
        spread: 15.0,
        gravity: -15.0,
        color: [2.0, 2.0, 2.0, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::SmokeClouds,
        additive: false,
        count: (10, 12),
        life: (0.3, 0.6),
        width: (0.5, 0.8),
        length: (0.5, 0.8),
        grow: 3.0,
        speed: (1.0, 3.0),
        spread: 1.0,
        color: [0.55, 0.52, 0.5, 0.35],
        ..E
    },
    Emitter {
        tex: Tex::SmokeClouds,
        additive: false,
        count: (8, 8),
        life: (0.3, 0.6),
        width: (0.5, 0.8),
        length: (0.5, 0.8),
        grow: 3.0,
        speed: (0.0, 0.5),
        spread: 5.0,
        color: [0.5, 0.48, 0.46, 0.3],
        ..E
    },
];

/// Vehland_Low_XSm_FX (the hover car settling back down on its jets): two sprays
/// of long sparks, a puff of dust thrown up and a wide, low sheet of dust spreading
/// out under the car.
const CAR_LAND_HOVER: &[Emitter] = &[
    Emitter {
        tex: Tex::Sparks,
        align: Align::Velocity,
        count: (30, 30),
        life: (0.3, 1.0),
        width: (0.02, 0.02),
        length: (0.3, 0.7),
        speed: (3.0, 10.0),
        spread: 15.0,
        gravity: -15.0,
        color: [2.0, 2.0, 2.0, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::Sparks,
        align: Align::Velocity,
        count: (15, 15),
        life: (0.3, 1.0),
        width: (0.03, 0.04),
        length: (0.1, 0.2),
        speed: (2.0, 6.0),
        spread: 10.0,
        gravity: -15.0,
        color: [2.0, 2.0, 2.0, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::SmokeClouds,
        additive: false,
        count: (10, 12),
        life: (0.3, 0.6),
        width: (0.5, 0.8),
        length: (0.5, 0.8),
        grow: 3.0,
        speed: (1.0, 3.0),
        spread: 1.0,
        color: [0.55, 0.52, 0.5, 0.35],
        ..E
    },
    Emitter {
        tex: Tex::SmokeClouds,
        additive: false,
        count: (6, 6),
        life: (0.3, 0.6),
        width: (2.5, 5.0),
        length: (2.5, 5.0),
        grow: 1.5,
        speed: (0.0, 0.3),
        spread: 4.0,
        color: [0.5, 0.48, 0.46, 0.15],
        ..E
    },
];

/// Booster_Jazz_FX (BoostSocket_L/R, the two exhausts at the back): a flickering
/// orange-red flame and a hot glow. car.rs fires these every frame while boosting.
const CAR_BOOST: &[Emitter] = &[
    Emitter {
        tex: Tex::LightBeam,
        align: Align::Cone,
        life: (0.05, 0.1),
        width: (0.25, 0.3),
        length: (0.6, 0.9),
        width_curve: &[1.0, 1.0],
        length_curve: &[1.0, 1.5, 2.0, 2.5, 3.0, 2.9],
        color: [1.6, 0.5, 0.25, 1.0],
        ..E
    },
    Emitter {
        tex: Tex::Spot,
        life: (0.08, 0.1),
        width: (0.9, 1.1),
        length: (0.9, 1.1),
        color: [0.5, 0.12, 0.08, 1.0],
        ..E
    },
];

/// How big an effect is drawn against the sizes above. The weapons' effects play on
/// their gun's mesh (TnWeaponEffectPlayFXOnSocket), so they take its scale: Jazz's
/// robot meshes are drawn at 0.6 (his MeshScale), the car at its own size. The
/// emitters above were first sized for a Jazz 6 m tall and a car 2.2 times its
/// size; these bring them back to the game's.
fn size_scale(effect: Effect) -> f32 {
    match effect {
        Effect::RifleMuzzle
        | Effect::SubsonicMuzzle
        | Effect::PhotonMuzzle
        | Effect::ScatterMuzzle
        | Effect::SubsonicFlash
        | Effect::ReloadSteam
        | Effect::ScatterReloadSteam
        | Effect::SubsonicReloadSteam
        | Effect::ShellSteam
        | Effect::MedShellSteam
        | Effect::NucleonMuzzle
        | Effect::NucleonCharged
        | Effect::PathMuzzle
        | Effect::PathReloadSteam
        | Effect::RiotReloadSteam => 0.6,
        Effect::CarMuzzle => 0.7,
        _ => 1.0,
    }
}

/// The bullet hole an impact leaves on a wall, if any (its squib's decal emitter).
pub fn hole_for(impact: Effect) -> Option<Effect> {
    match impact {
        Effect::BulletImpact => Some(Effect::BulletHole),
        Effect::SubsonicImpact => Some(Effect::SubsonicHole),
        Effect::PathImpact => Some(Effect::PathScorch),
        _ => None,
    }
}

/// Muzzle light (the weapons' TnWeaponEffectLight): colour, brightness, range.
fn light_for(effect: Effect) -> Option<(Color, f32, f32)> {
    match effect {
        // RampLightProperties: Brightness 5 (rifle), 8 (car), 15 with colour
        // (95, 153, 207) for the Subsonic Repeater (whose second barrel's flash,
        // SubsonicFlash, brings none).
        Effect::RifleMuzzle => Some((Color::srgb(1.0, 0.75, 0.45), 5.0, 12.0)),
        Effect::CarMuzzle => Some((Color::srgb(1.0, 0.75, 0.45), 8.0, 12.0)),
        Effect::SubsonicMuzzle => Some((Color::srgb_u8(95, 153, 207), 15.0, 12.0)),
        // The Photon Burst Rifle's: Brightness 8, colour (156, 217, 255).
        Effect::PhotonMuzzle => Some((Color::srgb_u8(156, 217, 255), 8.0, 12.0)),
        // The Scatter Blaster's: Brightness 8, the default (fire) colour.
        Effect::ScatterMuzzle => Some((Color::srgb(1.0, 0.75, 0.45), 8.0, 12.0)),
        // The Nucleon Charge Rifle's: Brightness 10, colour (126, 50, 48): the
        // floor round Jazz goes red with every shot.
        Effect::NucleonMuzzle => Some((Color::srgb_u8(126, 50, 48), 10.0, 12.0)),
        // The Path Blaster's sets only its colour, (197, 103, 65): the default
        // Brightness, 25, the brightest of them.
        Effect::PathMuzzle => Some((Color::srgb_u8(197, 103, 65), 25.0, 12.0)),
        // The Riot Cannon's: Brightness 50, colour (56, 46, 105): a violet flash
        // that lights up everything round Jazz with every shot.
        Effect::RiotMuzzle => Some((Color::srgb_u8(56, 46, 105), 50.0, 12.0)),
        // The Orbital Pistol's sets only its colour, (117, 255, 229): the default
        // Brightness, 25, in a pale cyan.
        Effect::OrbitalMuzzle => Some((Color::srgb_u8(117, 255, 229), 25.0, 12.0)),
        // The X18 Scrapmaker's: Brightness 15, colour (221, 104, 96).
        Effect::ScrapmakerMuzzle => Some((Color::srgb_u8(221, 104, 96), 15.0, 12.0)),
        // The Energon Harvester's sets only its colour, (106, 181, 245): the default
        // Brightness, 25.
        Effect::HarvesterMuzzle => Some((Color::srgb_u8(106, 181, 245), 25.0, 12.0)),
        // The Corrosive Slime Cannon's: Brightness 3 (Radius 256), the default
        // colour, (245, 174, 122).
        Effect::SlimeMuzzle => Some((Color::srgb_u8(245, 174, 122), 3.0, 12.0)),
        // The Gear Shredder's: Brightness 8, colour (164, 232, 255).
        Effect::ShredderMuzzle => Some((Color::srgb_u8(164, 232, 255), 8.0, 12.0)),
        // The Thermo Rocket Cannon's and the Chaos-Rift Combustor's set nothing: the
        // defaults, Brightness 25 in (245, 174, 122). The A-4 Pulsar Cannon's sets
        // its colour, (121, 75, 108).
        Effect::ThermoMuzzle | Effect::CombustorMuzzle => {
            Some((Color::srgb_u8(245, 174, 122), 25.0, 12.0))
        }
        Effect::PulsarMuzzle => Some((Color::srgb_u8(121, 75, 108), 25.0, 12.0)),
        // The blast lights up the area around it.
        _ => None,
    }
}

/// How far a decal sits off its surface (metres), clear of it at any distance.
const DECAL_LIFT: f32 = 0.02;

/// Lumens per unit of the game's light brightness.
pub(crate) const LIGHT_LUMENS: f32 = 60_000.0;
const LIGHT_SECS: f32 = 0.06;

/// One emitter of a tracer particle system. All particles leave the muzzle together
/// along the shot, each with its own speed, so they string out as they fly.
struct TracerSpec {
    tex: Tex,
    /// Pick a random sub-image per particle (else the first).
    random_frame: bool,
    count: (u32, u32),
    /// Seconds.
    life: (f32, f32),
    /// Metres per second along the shot.
    speed: (f32, f32),
    /// Metres.
    width: (f32, f32),
    length: (f32, f32),
    /// Start up to this far (metres) off the line of fire.
    scatter: f32,
    color: [f32; 4],
    /// Fraction of its life after which it fades out.
    fade_from: f32,
    /// Drawn with the Ion core mesh (trailing behind its head) instead of a sprite.
    mesh: bool,
    need: Need,
    /// Length multiplier over its life.
    length_curve: &'static [f32],
    /// Turns per second about its length (meshes).
    spin: (f32, f32),
    /// Centred on the particle, as the game draws its sprites (half its length
    /// ahead of where the particle is), rather than trailing behind it.
    centred: bool,
    /// Its texture turned round along it, its top at the tail (the game's
    /// SizeMultiplyLife along its length is negative: the FireSpears streaks'), so
    /// its flame trails behind the bright head at its particle; drawn whole, centred
    /// on its particle (the half ahead of it is clear).
    trailing: bool,
    /// Sideways speed (metres per second, up and across, each at random up to this
    /// either way) on top of `speed`: each streak flies a little off the line.
    drift: f32,
    /// Drawn additively (glowing); else see-through (smoke).
    additive: bool,
}

const T: TracerSpec = TracerSpec {
    tex: Tex::Bolt,
    random_frame: false,
    count: (1, 1),
    life: (1.0, 1.0),
    speed: (100.0, 100.0),
    width: (0.1, 0.1),
    length: (1.0, 1.0),
    scatter: 0.0,
    color: [1.0, 1.0, 1.0, 1.0],
    fade_from: 0.8,
    mesh: false,
    need: Need::Always,
    length_curve: &[],
    spin: (0.0, 0.0),
    centred: false,
    trailing: false,
    drift: 0.0,
    additive: true,
};

// The values are the tracer particle systems' (centimetres in metres): lifetime,
// initial size (width x length of the velocity-aligned sprite), initial velocity,
// burst count, initial location and colour.

/// AssaultRifle_VEH_Tracer_FX Bolt_Export's SizeMultiplyLife along its length: a
/// streak 0.3x its length at the muzzle stretches to 2x in the first fifth of its
/// life, then shrinks to 0.2x.
const BULLET_TRACER_LENGTH: &[f32] = &[
    0.3, 0.725, 1.15, 1.575, 2.0, 1.888, 1.775, 1.663, 1.55, 1.438, 1.325, 1.212, 1.1, 0.987,
    0.875, 0.763, 0.65, 0.537, 0.425, 0.312, 0.2,
];

/// AssaultRifle_VEH_Tracer_FX (Bolt_Export): one FireSpears streak, 15-25 cm wide
/// and 9-18 m long, at 120-150 m/s for 0.6 s. Its burst asks for 5-10, and the game
/// keeps its cooked peak of 1 (see SCATTER_MUZZLE): its videos show a single streak
/// with each shot, where the demo's bundle of 5-10, each at its own speed, drew out
/// into a longer, thicker and slower-looking spray. The sprite is centred on its
/// particle and stretches out to twice its length as it leaves (above); its
/// length's multiplier is negative, which turns the texture round, so the flame's
/// bright head is at the particle and its tail streams out behind (`trailing`). It
/// stays at full brightness, and goes as it reaches where the shot struck (the
/// game's SetTracerDistance).
const BULLET_TRACER: &[TracerSpec] = &[TracerSpec {
    tex: Tex::FireSpears,
    random_frame: true,
    count: (1, 1),
    life: (0.6, 0.6),
    speed: (120.0, 150.0),
    width: (0.15, 0.25),
    length: (9.0, 18.0),
    scatter: 0.0,
    color: [1.859, 1.71, 0.769, 1.0],
    fade_from: 1.0,
    length_curve: BULLET_TRACER_LENGTH,
    centred: true,
    trailing: true,
    ..T
}];

/// Subsonic_Repeater_Tracer_FX: two thin bolts (Bolt, Bolt_Dup), 2-5 cm wide and
/// 1.5-5 m long, each centred on its particle as the game draws them, plus the
/// squiggly core (Tracer_Trail), all at 200 m/s for about 2 s, cut off where the
/// round struck. Left out: the heat-haze rings along its path (Distortion_Rings).
const SUBSONIC_TRACER: &[TracerSpec] = &[
    TracerSpec {
        tex: Tex::Bolt,
        count: (2, 2),
        life: (1.9, 2.2),
        speed: (200.0, 200.0),
        width: (0.02, 0.05),
        length: (1.5, 5.0),
        // Colour 10 scaled by (1, 5, 5): white-hot cyan.
        color: [0.7, 2.6, 2.8, 1.0],
        fade_from: 0.8,
        centred: true,
        ..T
    },
    // Tracer_Trail: six copies of the Ion core mesh, 0.1-0.2x as wide and 0.7-1x as
    // long as the mesh, each turned its own way and spinning up to two turns a
    // second, stretching out behind the shot to about 45x their length.
    TracerSpec {
        tex: Tex::IonCore,
        count: (6, 6),
        life: (1.9, 2.2),
        speed: (200.0, 200.0),
        width: (0.1 * ION_CORE_ACROSS, 0.2 * ION_CORE_ACROSS),
        length: (0.7 * ION_CORE_LENGTH, ION_CORE_LENGTH),
        color: [0.6, 1.3, 2.0, 1.0],
        fade_from: 0.8,
        mesh: true,
        need: Need::WithMesh,
        length_curve: ION_TRAIL_LENGTH,
        spin: (0.0, 2.0),
        ..T
    },
    // Stand-in when the mesh is missing: a glowing core sprite.
    TracerSpec {
        tex: Tex::IonCore,
        life: (1.9, 2.2),
        speed: (200.0, 200.0),
        width: (0.3, 0.45),
        length: (0.9, 1.4),
        color: [0.5, 1.1, 1.8, 1.0],
        fade_from: 0.8,
        need: Need::WithoutMesh,
        ..T
    },
];

/// Photon_BurstRifle_Tracer_Fx (Bolt_Export_Export): 10 FireSpears streaks, 0.4-1.2 m
/// wide and 48-58 m long, at 320-350 m/s for 2 s, centred on their particles and
/// stretched over their lives as the rifle's are (its SizeMultiplyLife, negative
/// along its length: the flame trails behind its head), at full brightness, gone as
/// they reach where the shot struck. Its colour (1.86, 10.7, 100.8) is a deep blue
/// far brighter than the other tracers'; it keeps that hue at their brightness.
const PHOTON_TRACER: &[TracerSpec] = &[TracerSpec {
    tex: Tex::FireSpears,
    random_frame: true,
    count: (10, 10),
    life: (2.0, 2.0),
    speed: (320.0, 350.0),
    width: (0.4, 1.2),
    length: (48.0, 58.0),
    scatter: 0.0,
    color: [0.074, 0.42, 4.0, 1.0],
    fade_from: 1.0,
    length_curve: BULLET_TRACER_LENGTH,
    centred: true,
    trailing: true,
    ..T
}];

/// NucleonRifle_Tracer_FX Bolt_Sled: three bolts 10-15 cm wide and 7-13 m long, up
/// to 50 cm apart, at 900-970 m/s for 0.6-0.9 s: cyan (its colour, 1, scaled by 0,
/// 2, 2), cut off where the round struck. (Bolt_Sled_Smoke's bolts are all but
/// black, and its smoke and squiggle trails don't show: left out. Its rings are
/// NUCLEON_RINGS.)
const NUCLEON_TRACER: &[TracerSpec] = &[TracerSpec {
    tex: Tex::Bolt,
    count: (3, 3),
    life: (0.6, 0.9),
    speed: (900.0, 970.0),
    width: (0.10, 0.15),
    length: (7.0, 13.0),
    scatter: 0.5,
    color: [0.2, 2.0, 2.2, 1.0],
    fade_from: 0.6,
    ..T
}];

/// PathBlaster_Tracer_FX Bolt_Sled: three bolts 10-15 cm wide and 7-13 m long, up to
/// 50 cm apart, at 180-510 m/s for 0.6-0.9 s, cut off where the round struck (the
/// Nucleon Charge Rifle's Bolt_Sled at a third of its speed or less). Its colour,
/// (50, 2, 2), burns white at the core with a red fringe; here a red-hot white.
/// (Its Trail_Dup, smoke puffs left at the muzzle, is left out, as the Nucleon
/// Charge Rifle's.)
const PATH_TRACER: &[TracerSpec] = &[TracerSpec {
    tex: Tex::Bolt,
    count: (3, 3),
    life: (0.6, 0.9),
    speed: (180.0, 510.0),
    width: (0.10, 0.15),
    length: (7.0, 13.0),
    scatter: 0.5,
    color: [4.0, 1.4, 1.2, 1.0],
    fade_from: 0.6,
    ..T
}];

/// Scatterblaster_Tracer_FX, one per pellet (CreateTracerForEachImpact): its two
/// bolt emitters (Bolt, Bolts_Dup) throw FireSpears streaks, 25-50 cm wide and
/// 12-20 m long, at 120-150 m/s with up to 7 m/s (Bolt) or 10 m/s (Bolts_Dup) of
/// sideways drift each way, so the bundle fans out a little: their bursts ask for
/// 5-10 each, and the game keeps 1 and 2 (their cooked peaks: see SCATTER_MUZZLE),
/// three streaks a pellet. Like
/// the rifle's, each streak is centred on its particle, its flame trailing behind
/// its head, and stretches from 0.3x to 2x its length and back to 0.2x over its
/// life (2 s here, so it reaches its longest 0.4 s out), in the same fiery colour
/// at full brightness, and goes as it reaches where the pellet struck. With them fly
/// up to three dark smoke streaks (Smoky_Bits: 4x4_Clouds_Dark, 1-2.5 m wide and 4-6
/// m long, 30-80% opaque: a burst of 0-3 and one more from its rate, its peak 3);
/// puffs of smoke follow (SCATTER_TRACER_SMOKE).
const SCATTER_TRACER: &[TracerSpec] = &[
    TracerSpec {
        tex: Tex::FireSpears,
        random_frame: true,
        count: (1, 1),
        life: (2.0, 2.0),
        speed: (120.0, 150.0),
        width: (0.25, 0.5),
        length: (12.0, 20.0),
        color: [1.859, 1.71, 0.769, 1.0],
        fade_from: 1.0,
        length_curve: BULLET_TRACER_LENGTH,
        centred: true,
        trailing: true,
        drift: 7.0,
        ..T
    },
    TracerSpec {
        tex: Tex::FireSpears,
        random_frame: true,
        count: (2, 2),
        life: (2.0, 2.0),
        speed: (120.0, 150.0),
        width: (0.25, 0.5),
        length: (12.0, 20.0),
        color: [1.859, 1.71, 0.769, 1.0],
        fade_from: 1.0,
        length_curve: BULLET_TRACER_LENGTH,
        centred: true,
        trailing: true,
        drift: 10.0,
        ..T
    },
    TracerSpec {
        tex: Tex::CloudsDark,
        random_frame: true,
        count: (1, 3),
        life: (2.0, 2.0),
        speed: (120.0, 150.0),
        width: (1.0, 2.5),
        length: (4.0, 6.0),
        color: [1.0, 1.0, 1.0, 0.55],
        fade_from: 1.0,
        centred: true,
        drift: 7.0,
        additive: false,
        ..T
    },
];

/// Scatterblaster_Tracer_FX's smoke puffs, thrown along each pellet's line at
/// 80-100 m/s with up to 7 m/s of drift and cut off where it struck: a dark one
/// (Smoky_Puffs_Dark: 4x4_Clouds_Dark, 2.5-6 m across swelling to twice that,
/// 10-20% opaque, fading out by 80% of their 0.4-0.6 s) and a faint glowing one
/// (Smoky_Puffs: SmokeClouds_01, additive, colour 5.6, 5.1, 2.3 at 1-5% opacity,
/// fading out by 95% of their life): their bursts ask for up to 3 each, and the
/// game keeps 1 (their cooked peaks: see SCATTER_MUZZLE). Left out: the heat haze
/// the glowing one's material adds.
const SMOKY_DARK_FADE: &[f32] = &[
    1.0, 0.996, 0.983, 0.962, 0.933, 0.896, 0.851, 0.799, 0.738, 0.671, 0.596, 0.514, 0.424, 0.328,
    0.225, 0.116, 0.0, 0.0, 0.0, 0.0, 0.0,
];
const SMOKY_GLOW_FADE: &[f32] = &[
    1.0, 0.997, 0.988, 0.973, 0.952, 0.926, 0.893, 0.855, 0.812, 0.764, 0.71, 0.651, 0.587, 0.518,
    0.443, 0.364, 0.28, 0.192, 0.098, 0.0, 0.0,
];
const SCATTER_TRACER_SMOKE: &[Emitter] = &[
    Emitter {
        tex: Tex::CloudsDark,
        additive: false,
        count: (1, 1),
        life: (0.4, 0.6),
        width: (2.5, 6.0),
        length: (2.5, 6.0),
        grow: 2.0,
        speed: (80.0, 100.0),
        spread: 7.0,
        color: [1.0, 1.0, 1.0, 0.15],
        fade_curve: SMOKY_DARK_FADE,
        ..E
    },
    Emitter {
        tex: Tex::SmokeClouds,
        count: (1, 1),
        life: (0.4, 0.6),
        width: (2.5, 6.0),
        length: (2.5, 6.0),
        grow: 2.0,
        speed: (80.0, 100.0),
        spread: 7.0,
        color: [0.167, 0.154, 0.069, 1.0],
        fade_curve: SMOKY_GLOW_FADE,
        ..E
    },
];

/// The smoke a tracer throws along its line, if any (an effect whose particles are
/// cut off where the shot struck).
fn tracer_smoke(kind: TracerKind) -> Option<Effect> {
    match kind {
        TracerKind::Scatter => Some(Effect::ScatterTracerSmoke),
        _ => None,
    }
}

fn tracer_specs(kind: TracerKind) -> &'static [TracerSpec] {
    match kind {
        TracerKind::Bullet => BULLET_TRACER,
        TracerKind::Subsonic => SUBSONIC_TRACER,
        TracerKind::Photon => PHOTON_TRACER,
        TracerKind::Scatter => SCATTER_TRACER,
        TracerKind::Nucleon => NUCLEON_TRACER,
        TracerKind::Path => PATH_TRACER,
    }
}

/// Thinnest a tracer is drawn, as a fraction of its distance from the camera
/// (about two pixels at 1080p), so far-off thin bolts don't vanish.
const TRACER_MIN_WIDTH: f32 = 0.0018;

fn emitters(effect: Effect) -> &'static [Emitter] {
    match effect {
        Effect::RifleMuzzle => RIFLE_MUZZLE,
        Effect::SubsonicMuzzle | Effect::SubsonicFlash => SUBSONIC_MUZZLE,
        Effect::CarMuzzle => CAR_MUZZLE,
        Effect::BulletImpact => BULLET_IMPACT,
        Effect::SubsonicImpact => SUBSONIC_IMPACT,
        Effect::PhotonMuzzle => PHOTON_MUZZLE,
        Effect::PhotonImpact => PHOTON_IMPACT,
        Effect::ScatterMuzzle => SCATTER_MUZZLE,
        Effect::ScatterImpact => SCATTER_IMPACT,
        Effect::Shell | Effect::MedShell => &[],
        Effect::ShellSteam => SHELL_STEAM,
        Effect::MedShellSteam => MED_SHELL_STEAM,
        Effect::CarJump => CAR_JUMP,
        Effect::CarHover => CAR_HOVER,
        Effect::CarLandLow => CAR_LAND_LOW,
        Effect::CarLandHigh => CAR_LAND_HIGH,
        Effect::CarLandHover => CAR_LAND_HOVER,
        Effect::CarBoost => CAR_BOOST,
        Effect::ReloadSteam => RELOAD_STEAM,
        Effect::ScatterReloadSteam => SCATTER_RELOAD_STEAM,
        Effect::ScatterTracerSmoke => SCATTER_TRACER_SMOKE,
        Effect::BulletHole => BULLET_HOLE,
        Effect::SubsonicHole => SUBSONIC_HOLE,
        Effect::SubsonicReloadSteam => SUBSONIC_RELOAD_STEAM,
        Effect::NucleonMuzzle => NUCLEON_MUZZLE,
        Effect::NucleonImpact => NUCLEON_IMPACT,
        Effect::NucleonRings => NUCLEON_RINGS,
        Effect::NucleonCharged => NUCLEON_CHARGED,
        Effect::PathMuzzle => PATH_MUZZLE,
        Effect::PathImpact => PATH_IMPACT,
        Effect::PathScorch => PATH_SCORCH,
        Effect::PathReloadSteam => PATH_RELOAD_STEAM,
        Effect::RiotMuzzle => &[],
        Effect::RiotReloadSteam => RIOT_RELOAD_STEAM,
        Effect::OrbitalMuzzle
        | Effect::ScrapmakerMuzzle
        | Effect::HarvesterMuzzle
        | Effect::SlimeMuzzle
        | Effect::ShredderMuzzle
        | Effect::ThermoMuzzle
        | Effect::PulsarMuzzle
        | Effect::CombustorMuzzle
        | Effect::NoFlash => &[],
    }
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

/// Meshes and textures for the effects.
#[derive(Resource)]
pub struct FxAssets {
    centred: Handle<Mesh>,
    /// A quad whose top edge is at its origin (tracers: the head leads).
    head: Handle<Mesh>,
    /// A quad whose bottom edge is at its origin (flames start at the muzzle).
    base: Handle<Mesh>,
    textures: Vec<(Tex, Option<Handle<Image>>, (u32, u32))>,
    /// The spent shells (Effect::Shell, Effect::MedShell).
    shells: [ShellLook; 2],
    shell_material: Handle<StandardMaterial>,
    shell_glow: Handle<StandardMaterial>,
    /// Ion_Cannon_MuzzleFlash_STAT (assets/models/fx/ion_core.glb), scaled to run
    /// from its pivot to y = 1 and be 1 across.
    ion_core: Option<Handle<Mesh>>,
    /// Boostermesh_03 (assets/models/fx/booster_cone.glb), scaled the same way.
    booster: Option<Handle<Mesh>>,
}

/// A spent shell's meshes: its casing, the disc of its open end and where that
/// sits (on the casing's -Y end), and how high its middle lies on the floor.
struct ShellLook {
    casing: Handle<Mesh>,
    end: Handle<Mesh>,
    end_at: f32,
    rest: f32,
}

/// The Ion core mesh, built by tools/build_fx_meshes.py.
const ION_CORE_MODEL: &str = "models/fx/ion_core.glb";
/// The car's jet cone, built by tools/build_fx_meshes.py.
const BOOSTER_MODEL: &str = "models/fx/booster_cone.glb";

impl FxAssets {
    fn texture(&self, tex: Tex) -> (Option<Handle<Image>>, (u32, u32)) {
        self.textures
            .iter()
            .find(|t| t.0 == tex)
            .map(|t| (t.1.clone(), t.2))
            .unwrap_or((None, (1, 1)))
    }
}

/// Loads the effect textures (when present) and builds the shared meshes.
pub fn setup_fx(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets_dir: Res<FxDir>,
) {
    let mut textures = Vec::new();
    let mut found = 0;
    for (tex, file, grid) in TEXTURES {
        let rel = format!("textures/fx/{file}.png");
        let handle = if assets_dir.0.join(&rel).exists() {
            found += 1;
            Some(asset_server.load(rel))
        } else {
            None
        };
        textures.push((tex, handle, grid));
    }
    if found == 0 {
        info!("Effect textures not found in assets/textures/fx: using plain sprites");
    }
    let centred = meshes.add(Rectangle::new(1.0, 1.0));
    let head =
        meshes.add(Mesh::from(Rectangle::new(1.0, 1.0)).translated_by(Vec3::new(0.0, -0.5, 0.0)));
    let base =
        meshes.add(Mesh::from(Rectangle::new(1.0, 1.0)).translated_by(Vec3::new(0.0, 0.5, 0.0)));
    // Sm_Shell3_FX's casing (shell3_STAT) at Jazz's scale, and Med_Shell2_FX's
    // (shell2_STAT: 19 units across and 56 long, 2.3 times as long) at its size of
    // 0.75 on Jazz's 0.6. Each glows in its open end (WEP_shellcasings_CLR's mask,
    // lit in Jazz's energon cyan), here a disc across four fifths of it.
    let mut shell = |radius: f32, length: f32, rest: f32| ShellLook {
        casing: meshes.add(Cylinder::new(radius, length)),
        end: meshes.add(Cylinder::new(radius * 0.8, 0.004)),
        end_at: -length * 0.5 - 0.002,
        rest,
    };
    let shells = [shell(0.021, 0.1, 0.04), shell(0.044, 0.25, 0.08)];
    let shell_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.45, 0.25),
        metallic: 0.9,
        perceptual_roughness: 0.35,
        ..default()
    });
    let shell_glow = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(0.0, 3.0, 3.0),
        ..default()
    });
    let ion_core = if assets_dir.0.join(ION_CORE_MODEL).exists() {
        Some(
            asset_server.load(
                GltfAssetLabel::Primitive {
                    mesh: 0,
                    primitive: 0,
                }
                .from_asset(ION_CORE_MODEL),
            ),
        )
    } else {
        info!("{ION_CORE_MODEL} not found: the Subsonic Repeater's core uses sprites");
        None
    };
    let booster = if assets_dir.0.join(BOOSTER_MODEL).exists() {
        Some(
            asset_server.load(
                GltfAssetLabel::Primitive {
                    mesh: 0,
                    primitive: 0,
                }
                .from_asset(BOOSTER_MODEL),
            ),
        )
    } else {
        info!("{BOOSTER_MODEL} not found: the car's jets use sprites");
        None
    };
    commands.insert_resource(FxAssets {
        centred,
        head,
        base,
        textures,
        shells,
        shell_material,
        shell_glow,
        ion_core,
        booster,
    });
}

/// Folder the assets live in (to check which effect textures exist).
#[derive(Resource)]
pub struct FxDir(pub std::path::PathBuf);

impl FxDir {
    pub fn new(path: &Path) -> Self {
        FxDir(path.to_path_buf())
    }
}

fn sprite_material(
    image: Option<Handle<Image>>,
    grid: (u32, u32),
    frame: u32,
    additive: bool,
    color: [f32; 4],
    turned: bool,
) -> StandardMaterial {
    let (cols, rows) = (grid.0.max(1), grid.1.max(1));
    let (col, row) = (frame % cols, (frame / cols) % rows);
    let scale = Vec2::new(1.0 / cols as f32, 1.0 / rows as f32);
    // (`turned`: the image upside down, its top at the sprite's bottom: a trailing
    // tracer's, TracerSpec::trailing.)
    let (v_scale, v_from) = if turned {
        (-scale.y, (row + 1) as f32 * scale.y)
    } else {
        (scale.y, row as f32 * scale.y)
    };
    // Bolt_CLR is drawn lying down (bright end on the left); turn it so its length
    // runs along the sprite with the bright end at the head.
    let lying = grid == LYING;
    // Without the texture, a soft-looking flat colour stands in.
    let alpha = if image.is_some() {
        color[3]
    } else {
        color[3] * 0.5
    };
    StandardMaterial {
        base_color: Color::linear_rgba(color[0], color[1], color[2], alpha),
        base_color_texture: image,
        uv_transform: if lying {
            Affine2::from_scale_angle_translation(Vec2::ONE, -std::f32::consts::FRAC_PI_2, Vec2::Y)
        } else {
            Affine2::from_scale_angle_translation(
                Vec2::new(scale.x, v_scale),
                0.0,
                Vec2::new(col as f32 * scale.x, v_from),
            )
        },
        unlit: true,
        alpha_mode: if additive {
            AlphaMode::Add
        } else {
            AlphaMode::Blend
        },
        // Bevy's fog blends its colour into every pixel of a sprite, the black ones
        // too, which an additive sprite then adds: its whole quad would show as a
        // see-through square in the fog. Additive sprites are dimmed by the fog
        // instead (fog_clear), as the game's are.
        fog_enabled: !additive,
        cull_mode: None,
        double_sided: true,
        ..default()
    }
}

/// A sprite's look: its texture's frame, how it is blended and whether it is turned
/// (sprite_material's).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct LookKey {
    image: Option<AssetId<Image>>,
    grid: (u32, u32),
    frame: u32,
    additive: bool,
    turned: bool,
}

/// The sprites' materials made so far: one for each look, in white, each sprite
/// drawn in its own colour (live.rs's LiveTints, by its mesh tag), set as it fades.
/// Each sprite had a material of its own, changed every frame as it faded: a
/// changed or new standard material makes Bevy make anew the bind group of the
/// group of materials it is in (live.rs says more), and a gun firing has over a
/// hundred sprites fading at once.
#[derive(Resource, Default)]
pub struct SpriteLooks(HashMap<LookKey, Handle<LiveMaterial>>);

impl SpriteLooks {
    /// The material of the look (sprite_material's), made the first time.
    fn get(
        &mut self,
        live: &mut Assets<LiveMaterial>,
        image: Option<Handle<Image>>,
        grid: (u32, u32),
        frame: u32,
        additive: bool,
        turned: bool,
    ) -> Handle<LiveMaterial> {
        let key = LookKey {
            image: image.as_ref().map(Handle::id),
            grid,
            frame,
            additive,
            turned,
        };
        self.0
            .entry(key)
            .or_insert_with(|| {
                let mut base = sprite_material(image, grid, frame, additive, [1.0; 4], turned);
                base.base_color = Color::WHITE;
                live.add(live_material(base, Wave::Tinted))
            })
            .clone()
    }
}

/// One sprite particle.
#[derive(Component)]
pub struct Particle {
    age: f32,
    life: f32,
    delay: f32,
    velocity: Vec3,
    gravity: f32,
    width: f32,
    length: f32,
    grow: f32,
    align: Align,
    /// For flames and meshes: the direction they point and their roll around it.
    axis: Vec3,
    roll: f32,
    /// Radians per second about `axis` (meshes and decals).
    spin: f32,
    width_curve: &'static [f32],
    length_curve: &'static [f32],
    speed_curve: &'static [f32],
    fade_curve: &'static [f32],
    heat_curve: &'static [f32],
    color: [f32; 4],
    additive: bool,
    /// Seconds after which it is cut off (a tracer's smoke passing where the shot
    /// struck); infinite for others.
    cut: f32,
}

/// A spent shell.
#[derive(Component)]
pub struct ShellCasing {
    age: f32,
    velocity: Vec3,
    spin: Vec3,
    /// How low its middle comes before it bounces (the floor, plus ShellLook::rest).
    floor: Option<f32>,
}

/// A short flash of light at the muzzle.
#[derive(Component)]
pub struct FlashLight {
    age: f32,
    intensity: f32,
}

/// One tracer streak: a sprite stretched along the shot, facing the camera. Its
/// head flies out from the muzzle; it never reaches back past the muzzle nor on
/// past the point the shot struck.
#[derive(Component)]
pub struct Tracer {
    origin: Vec3,
    dir: Vec3,
    speed: f32,
    stop: f32,
    width: f32,
    length: f32,
    age: f32,
    life: f32,
    color: [f32; 4],
    fade_from: f32,
    /// A mesh trail (the Ion core) rather than a camera-facing sprite: its roll,
    /// spin (radians per second) and length over life.
    mesh: bool,
    roll: f32,
    spin: f32,
    length_curve: &'static [f32],
    /// Centred on its particle (TracerSpec::centred), its flame trailing
    /// (TracerSpec::trailing).
    centred: bool,
    trailing: bool,
    /// Glowing (TracerSpec::additive), or see-through smoke.
    additive: bool,
    /// Drawn with its texture (without, a soft flat colour, half as strong).
    textured: bool,
}

/// Spawns the queued effects.
pub fn spawn_fx(
    mut commands: Commands,
    fx: Option<Res<FxAssets>>,
    mut queue: ResMut<FxQueue>,
    (mut live, mut looks): (ResMut<Assets<LiveMaterial>>, ResMut<SpriteLooks>),
) {
    let Some(fx) = fx else {
        queue.requests.clear();
        queue.tracers.clear();
        return;
    };
    let have_mesh = fx.ion_core.is_some();
    let tracers = std::mem::take(&mut queue.tracers);
    for req in tracers {
        // Across the shot and up it, for the streaks' drift.
        let across = req.dir.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = across.cross(req.dir);
        for spec in tracer_specs(req.kind) {
            if !spec.need.met(have_mesh) {
                continue;
            }
            let mesh = match (&fx.ion_core, spec.mesh) {
                (Some(core), true) => core.clone(),
                _ => fx.head.clone(),
            };
            let (image, grid) = fx.texture(spec.tex);
            let n =
                spec.count.0 + (queue.random() * (spec.count.1 - spec.count.0 + 1) as f32) as u32;
            for _ in 0..n.min(spec.count.1) {
                let frames = (grid.0 * grid.1).max(1);
                let frame = if spec.random_frame {
                    ((queue.random() * frames as f32) as u32).min(frames - 1)
                } else {
                    0
                };
                let offset = if spec.scatter > 0.0 {
                    let v = queue.unit();
                    (v - req.dir * v.dot(req.dir)) * spec.scatter * queue.random()
                } else {
                    Vec3::ZERO
                };
                // Drifting off the line: a straight flight a little off it, cut off as
                // far along the shot as where it struck.
                let speed = queue.range(spec.speed);
                let (dir, speed, stop) = if spec.drift > 0.0 {
                    let drift = (across * (queue.random() * 2.0 - 1.0)
                        + up * (queue.random() * 2.0 - 1.0))
                        * spec.drift;
                    let v = req.dir * speed + drift;
                    let dir = v.normalize_or(req.dir);
                    (dir, v.length(), req.stop / dir.dot(req.dir).max(0.5))
                } else {
                    (req.dir, speed, req.stop)
                };
                let material = looks.get(
                    &mut live,
                    image.clone(),
                    grid,
                    frame,
                    spec.additive,
                    spec.trailing,
                );
                commands.spawn((
                    Tracer {
                        origin: req.origin + offset,
                        dir,
                        speed,
                        stop,
                        width: queue.range(spec.width),
                        length: queue.range(spec.length),
                        age: 0.0,
                        life: queue.range(spec.life),
                        color: spec.color,
                        fade_from: spec.fade_from,
                        mesh: spec.mesh,
                        roll: queue.random() * std::f32::consts::TAU,
                        spin: queue.range(spec.spin) * std::f32::consts::TAU,
                        length_curve: spec.length_curve,
                        centred: spec.centred,
                        trailing: spec.trailing,
                        additive: spec.additive,
                        textured: image.is_some(),
                    },
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material),
                    // (Its colour: set as it fades.)
                    tinted(),
                    // Hidden until its first update places it.
                    Transform::from_translation(req.origin).with_scale(Vec3::ZERO),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
        }
    }
    let requests = std::mem::take(&mut queue.requests);
    for req in requests {
        if matches!(req.effect, Effect::Shell | Effect::MedShell) {
            // Thrown out to the side and up, tumbling, its open end glowing.
            let velocity = req.side * queue.range((3.0, 5.0))
                + Vec3::Y * queue.range((2.5, 4.0))
                + queue.unit() * 0.8;
            let spin = queue.unit() * 18.0;
            let look = &fx.shells[(req.effect == Effect::MedShell) as usize];
            commands.spawn((
                ShellCasing {
                    age: 0.0,
                    velocity,
                    spin,
                    floor: req.floor.map(|f| f + look.rest),
                },
                Mesh3d(look.casing.clone()),
                MeshMaterial3d(fx.shell_material.clone()),
                Transform::from_translation(req.pos),
                NotShadowCaster,
                children![(
                    Mesh3d(look.end.clone()),
                    MeshMaterial3d(fx.shell_glow.clone()),
                    Transform::from_xyz(0.0, look.end_at, 0.0),
                    NotShadowCaster,
                )],
            ));
            continue;
        }
        if let Some((color, brightness, range)) = light_for(req.effect) {
            let intensity = brightness * LIGHT_LUMENS;
            commands.spawn((
                FlashLight {
                    age: 0.0,
                    intensity,
                },
                crate::graphics::LiveLight,
                PointLight {
                    color,
                    intensity,
                    range,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(req.pos + req.dir * 0.5),
            ));
        }
        for em in emitters(req.effect) {
            if !em.need.met(have_mesh) || (em.puffs > 0 && req.puff >= em.puffs) {
                continue;
            }
            let (image, grid) = fx.texture(em.tex);
            if em.need == Need::WithTexture && image.is_none() {
                continue;
            }
            let n = em.count.0 + (queue.random() * (em.count.1 - em.count.0 + 1) as f32) as u32;
            let n = n.min(em.count.1);
            for _ in 0..n {
                let frames = (grid.0 * grid.1).max(1);
                let frame = ((queue.random() * frames as f32) as u32).min(frames - 1);
                let life = queue.range(em.life);
                let k = size_scale(req.effect) * req.scale;
                let width = queue.range(em.width) * k;
                let length = if em.width == em.length {
                    width
                } else {
                    queue.range(em.length) * k
                };
                let side = queue.unit();
                let side = (side - req.dir * side.dot(req.dir)).normalize_or_zero();
                let velocity = req.vel
                    + (req.dir * queue.range(em.speed) + side * em.spread * queue.random()) * k;
                // Where it starts: some emitters spread their particles out along and
                // around the effect's line.
                let start = if em.along != (0.0, 0.0) || em.around > 0.0 {
                    let off = queue.unit();
                    let off = (off - req.dir * off.dot(req.dir)).normalize_or_zero();
                    req.pos
                        + (req.dir * queue.range(em.along) + off * em.around * queue.random()) * k
                } else {
                    req.pos
                };
                // Past where the shot struck it is gone (a tracer's smoke).
                let cut = if req.stop.is_finite() {
                    req.stop / velocity.dot(req.dir).max(1.0)
                } else {
                    f32::INFINITY
                };
                // Without the cone mesh, jets are drawn as flames.
                let align = match (em.align, &fx.booster) {
                    (Align::Cone, None) => Align::Flame,
                    (a, _) => a,
                };
                let copies: &[f32] = if align == Align::Flame {
                    &[0.0, std::f32::consts::FRAC_PI_2]
                } else {
                    &[0.0]
                };
                let roll0 = queue.range(em.turn) * std::f32::consts::TAU;
                let spin = queue.range(em.spin) * std::f32::consts::TAU;
                for &roll in copies {
                    let material =
                        looks.get(&mut live, image.clone(), grid, frame, em.additive, false);
                    let mesh = match (align, &fx.booster) {
                        (Align::Flame, _) => fx.base.clone(),
                        (Align::Cone, Some(cone)) => cone.clone(),
                        _ => fx.centred.clone(),
                    };
                    commands.spawn((
                        Particle {
                            age: 0.0,
                            life,
                            delay: em.delay,
                            velocity,
                            gravity: em.gravity,
                            width,
                            length,
                            grow: em.grow,
                            align,
                            axis: req.dir,
                            roll: roll0 + roll,
                            spin,
                            width_curve: em.width_curve,
                            length_curve: em.length_curve,
                            speed_curve: em.speed_curve,
                            fade_curve: em.fade_curve,
                            heat_curve: em.heat_curve,
                            color: em.color,
                            additive: em.additive,
                            cut,
                        },
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        // (Its colour: set as it fades.)
                        tinted(),
                        // Hidden until its first update places it. A decal sits just
                        // off its surface.
                        Transform::from_translation(if align == Align::Decal {
                            start + req.dir * DECAL_LIFT
                        } else {
                            start
                        })
                        .with_scale(Vec3::ZERO),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
        }
    }
}

/// How much of an additive sprite's light gets through the camera's fog from
/// `distance` away: what Bevy's fog would leave of the sprite's own colour, without
/// the fog colour it would add (see sprite_material). The game's fog does the same
/// to additive effects.
fn fog_clear(fog: Option<&DistanceFog>, distance: f32) -> f32 {
    let Some(fog) = fog else {
        return 1.0;
    };
    let amount = match fog.falloff {
        FogFalloff::Linear { start, end } => {
            1.0 - ((end - distance) / (end - start)).clamp(0.0, 1.0)
        }
        FogFalloff::Exponential { density } => 1.0 - (-distance * density).exp(),
        FogFalloff::ExponentialSquared { density } => 1.0 - (-(distance * density).powi(2)).exp(),
        FogFalloff::Atmospheric { extinction, .. } => {
            1.0 - (-distance * (extinction.x + extinction.y + extinction.z) / 3.0).exp()
        }
    };
    1.0 - amount * fog.color.alpha()
}

/// Rotation for a sprite whose length runs along `axis` and whose face turns
/// towards `to_camera` as far as it can.
fn along(axis: Vec3, to_camera: Vec3) -> Quat {
    let y = axis.normalize_or(Vec3::Y);
    let mut z = to_camera - y * to_camera.dot(y);
    if z.length_squared() < 1e-6 {
        z = y.any_orthonormal_vector();
    }
    let z = z.normalize();
    let x = y.cross(z);
    Quat::from_mat3(&Mat3::from_cols(x, y, z))
}

/// Moves, turns, grows and fades the particles, shells, lights and tracers.
pub fn update_fx(
    mut commands: Commands,
    time: Res<Time>,
    // (The game camera: the HUD has a 3D camera of its own.)
    camera: Query<
        (&Transform, Option<&DistanceFog>),
        (
            With<Camera3d>,
            With<crate::OrbitCamera>,
            Without<Particle>,
            Without<Tracer>,
        ),
    >,
    mut particles: Query<(Entity, &mut Transform, &mut Particle, &MeshTag)>,
    mut shells: Query<
        (Entity, &mut Transform, &mut ShellCasing),
        (Without<Particle>, Without<Tracer>, Without<Camera3d>),
    >,
    mut lights: Query<(Entity, &mut PointLight, &mut FlashLight)>,
    mut tracers: Query<
        (Entity, &mut Transform, &mut Tracer, &MeshTag),
        (Without<Particle>, Without<ShellCasing>, Without<Camera3d>),
    >,
    mut tints: ResMut<LiveTints>,
) {
    // (Their colours set as they fade, the colours marked changed once if any did.)
    let mut recoloured = false;
    let dt = time.delta_secs();
    let fog = camera.iter().next().and_then(|(_, fog)| fog);
    // (Each turned to the camera nearest it: with two players on one screen, the
    // one whose view it is most likely in.)
    let eyes: Vec<Vec3> = camera.iter().map(|(t, _)| t.translation).collect();
    let eye_for = |at: Vec3| {
        eyes.iter()
            .copied()
            .min_by(|a, b| a.distance_squared(at).total_cmp(&b.distance_squared(at)))
            .unwrap_or(Vec3::ZERO)
    };

    for (entity, mut transform, mut p, tag) in &mut particles {
        if p.delay > 0.0 {
            p.delay -= dt;
            continue;
        }
        p.age += dt;
        if p.age >= p.life || p.age >= p.cut {
            commands.entity(entity).despawn();
            continue;
        }
        let t = p.age / p.life;
        p.velocity.y += p.gravity * dt;
        transform.translation += p.velocity * sample(p.speed_curve, t) * dt;
        let size = 1.0 + (p.grow - 1.0) * t;
        let to_camera = eye_for(transform.translation) - transform.translation;
        transform.rotation = match p.align {
            Align::Camera => {
                let look = Transform::IDENTITY.looking_to(-to_camera, Vec3::Y).rotation;
                look * Quat::from_rotation_z(p.roll)
            }
            Align::Velocity => along(p.velocity, to_camera),
            Align::Flame => {
                let base = along(p.axis, Vec3::Y.cross(p.axis).normalize_or(Vec3::X));
                base * Quat::from_rotation_y(p.roll)
            }
            Align::Cone => {
                Quat::from_rotation_arc(Vec3::Y, p.axis.normalize_or(Vec3::Y))
                    * Quat::from_rotation_y(p.roll + p.spin * p.age)
            }
            Align::Decal => {
                Transform::IDENTITY.looking_to(-p.axis, Vec3::Y).rotation
                    * Quat::from_rotation_z(p.roll + p.spin * p.age)
            }
        };
        let (width_k, length_k) = if p.width_curve.is_empty() && p.length_curve.is_empty() {
            (size, size)
        } else {
            (sample(p.width_curve, t), sample(p.length_curve, t))
        };
        let width = p.width * width_k;
        // Meshes have depth too (the core's fins); sprites are flat.
        let depth = if p.align == Align::Cone { width } else { 1.0 };
        transform.scale = Vec3::new(width, p.length * length_k, depth);
        // Fade out over the second half of its life (flashes over all of it), or
        // as its own curve has it.
        let fade = if !p.fade_curve.is_empty() {
            sample(p.fade_curve, t)
        } else if p.life < 0.12 {
            1.0 - t
        } else {
            (1.0 - (t - 0.5).max(0.0) * 2.0).clamp(0.0, 1.0)
        };
        let c = p.color;
        // Additive sprites fade by darkening (and dim in the fog), see-through
        // ones by alpha.
        let k = if p.additive {
            fade * fog_clear(fog, to_camera.length())
        } else {
            1.0
        } * sample(p.heat_curve, t);
        // (An additive one with its own fade curve fades by that alone: its
        // alpha would scale its colour again.)
        let alpha = if p.additive && !p.fade_curve.is_empty() {
            c[3]
        } else {
            c[3] * fade
        };
        recoloured |= tints
            .bypass_change_detection()
            .set(tag.0, [c[0] * k, c[1] * k, c[2] * k, alpha]);
    }

    for (entity, mut transform, mut shell) in &mut shells {
        shell.age += dt;
        if shell.age > 1.2 {
            commands.entity(entity).despawn();
            continue;
        }
        shell.velocity.y -= 25.0 * dt;
        transform.translation += shell.velocity * dt;
        let spin = shell.spin * dt;
        transform.rotate(Quat::from_scaled_axis(spin));
        if let Some(floor) = shell.floor {
            if transform.translation.y < floor && shell.velocity.y < 0.0 {
                // Bounce off the floor, losing most of the energy.
                transform.translation.y = floor;
                shell.velocity = Vec3::new(
                    shell.velocity.x * 0.4,
                    -shell.velocity.y * 0.3,
                    shell.velocity.z * 0.4,
                );
                shell.spin *= 0.5;
            }
        }
    }

    for (entity, mut light, mut flash) in &mut lights {
        flash.age += dt;
        if flash.age >= LIGHT_SECS {
            commands.entity(entity).despawn();
        } else {
            light.intensity = flash.intensity * (1.0 - flash.age / LIGHT_SECS);
        }
    }

    for (entity, mut transform, mut tracer, tag) in &mut tracers {
        tracer.age += dt;
        let travelled = tracer.speed * tracer.age;
        let t = tracer.age / tracer.life;
        let length = tracer.length * sample(tracer.length_curve, t);
        // It goes as its particle reaches where the shot struck, as the game's
        // tracers do (SetTracerDistance), or as its life ends: nothing of it lingers
        // there while the rest of it catches up.
        if tracer.age >= tracer.life || travelled >= tracer.stop {
            commands.entity(entity).despawn();
            continue;
        }
        let (head, tail) = if tracer.trailing {
            // Centred on its particle and drawn whole, as the game draws its sprite
            // (the clear half ahead of the particle runs on into what it strikes).
            (travelled + length * 0.5, travelled - length * 0.5)
        } else {
            // Its head leads the particle by half its length when centred on it.
            let lead = if tracer.centred { length * 0.5 } else { 0.0 };
            (
                (travelled + lead).min(tracer.stop),
                (travelled + lead - length).max(0.0),
            )
        };
        let shown = (head - tail).max(0.01);
        transform.translation = tracer.origin + tracer.dir * head;
        if tracer.mesh {
            // The core mesh runs from its pivot (the comets' bright heads) along +Y:
            // point that back down the shot so it trails behind its head.
            transform.rotation = Quat::from_rotation_arc(Vec3::Y, -tracer.dir)
                * Quat::from_rotation_y(tracer.roll + tracer.spin * tracer.age);
            transform.scale = Vec3::new(tracer.width, shown, tracer.width);
        } else {
            let to_camera = eye_for(transform.translation) - transform.translation;
            transform.rotation = along(tracer.dir, to_camera);
            let width = tracer.width.max(to_camera.length() * TRACER_MIN_WIDTH);
            transform.scale = Vec3::new(width, shown, 1.0);
        }
        let fade = if t > tracer.fade_from {
            (1.0 - (t - tracer.fade_from) / (1.0 - tracer.fade_from)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        // Glowing tracers are dimmed in the fog as far away as their middle is;
        // smoky ones fade by their opacity alone (the fog colours them itself).
        let k = if tracer.additive {
            let middle = tracer.origin + tracer.dir * (head - shown * 0.5);
            fade * fog_clear(fog, eye_for(middle).distance(middle))
        } else {
            1.0
        };
        let c = tracer.color;
        // (Half as strong without its texture, as sprite_material makes it.)
        let a = if tracer.textured { c[3] } else { c[3] * 0.5 };
        recoloured |= tints
            .bypass_change_detection()
            .set(tag.0, [c[0] * k, c[1] * k, c[2] * k, a * fade]);
    }
    if recoloured {
        tints.set_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The most wisps a steam lets out over its first `puffs` puffs (each emitter
    /// at most its count a puff, and only through its own `puffs`).
    fn most_wisps(effect: Effect, puffs: u32) -> u32 {
        (0..puffs)
            .map(|puff| {
                emitters(effect)
                    .iter()
                    .filter(|em| em.puffs == 0 || puff < em.puffs)
                    .map(|em| em.count.1)
                    .sum::<u32>()
            })
            .sum()
    }

    /// The Neutron Assault Rifle's effects as the game draws them: no dark smoke at
    /// the car guns' muzzle, nor where the rounds strike (the game never draws their
    /// Smoke_Dark); the rifle's flash the game's own (main.rs's muzzle_system), this
    /// its light alone; and one tracer streak a shot.
    #[test]
    fn the_rifles_smoke_and_tracer_are_the_games() {
        for effect in [Effect::CarMuzzle, Effect::BulletImpact] {
            assert!(
                emitters(effect).iter().all(|em| em.tex != Tex::CloudsDark),
                "{effect:?}"
            );
        }
        assert!(RIFLE_MUZZLE.is_empty());
        assert!(light_for(Effect::RifleMuzzle).is_some());
        let streaks: u32 = tracer_specs(TracerKind::Bullet)
            .iter()
            .map(|t| t.count.1)
            .sum();
        assert_eq!(streaks, 1);
    }

    /// The steams keep to their cooked peaks over the reload's seven puffs: the
    /// rifle's 15 wisps (ReloadSteam_Med_FX), the Scatter Blaster's those and 5 more
    /// (ReloadSteam_Med02_FX), the Path Blaster's 5, the Subsonic Repeater's about 10
    /// (ReloadSteam_FX); a steaming shell's a wisp at a time (two for the Scatter
    /// Blaster's).
    #[test]
    fn steams_keep_to_their_peaks() {
        assert_eq!(most_wisps(Effect::ReloadSteam, 7), 15);
        assert_eq!(most_wisps(Effect::ScatterReloadSteam, 7), 20);
        assert_eq!(most_wisps(Effect::PathReloadSteam, 7), 5);
        assert_eq!(most_wisps(Effect::SubsonicReloadSteam, 7), 12);
        assert_eq!(most_wisps(Effect::RiotReloadSteam, 7), 17);
        assert_eq!(most_wisps(Effect::ShellSteam, 1), 1);
        assert_eq!(most_wisps(Effect::MedShellSteam, 1), 2);
    }
}
