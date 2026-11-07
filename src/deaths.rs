//! The game's deaths as its data has them (models/enemies/deaths.txt,
//! tools/build_death_sets.py): every enemy's death set and the players' robots'
//! (TR_Deaths_p.Sets.Basic_DEATHSET), each death's timeline, and the destructibles
//! its deaths spawn. As TransGame runs them:
//!
//! * **Its pick** (TnDeathPlayer.Pick): the first of its set's entries whose death
//!   types ([`Types`]) the kill has; a conditional set (TnConditionalDeathSet) one of
//!   its deaths at random by their weights. The kill's types (TnDeathContextFactory:
//!   [`Kill::types`]) are the side the blow came from, standing still or not, its
//!   form's (Robot, or Vehicle with its boost or hover), the damage type's own
//!   (DamageDeathTypes) and a gib's (the killer within its GibDistance); a robot's
//!   form adds a headshot, a hover, being in the air (TnRobotDeathContextFactory),
//!   the Sniper's insect a headshot and a ceiling (TnSniperDeathContextFactory). A
//!   kill no entry takes is a body gone at once.
//! * **Its timeline** (TnDeathTimeline, [`Timeline`]): keyframes, each a list of
//!   actions started together (after each one's own random Delay), the next starting
//!   once every one of this one's is done; the death over (the pawn gone: its
//!   LifeSpan 0.1 s) once the last is. What each action does and when it is done
//!   is its class's (see [`Do`]).
//! * **Its destructibles** (TnDeathActionSpawnDestructible, [`Destructible`]): an
//!   HmDestructibleComponent's pieces' effects in the state the death's notification
//!   takes them to: particle systems, gibs, sounds, decals and blast damage.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

/// The death data (tools/build_death_sets.py).
pub const DEATHS_FILE: &str = "models/enemies/deaths.txt";

// ---------------------------------------------------------------------------
// The death types
// ---------------------------------------------------------------------------

/// The death types (TransGame's TnDeathType classes, by the name after TnDeathType),
/// each a bit of [`Types`].
pub const TYPE_NAMES: [&str; 53] = [
    "Airborne",
    "Artillery",
    "Assassination",
    "Ballistic",
    "BruticusMelee",
    "Ceiling",
    "Corrosion",
    "Crushed",
    "Dismember",
    "Downed",
    "DrainGun",
    "Energy",
    "Explosive",
    "ExplosiveAssassination",
    "FallbackAssassination",
    "Fire",
    "FlashBang",
    "Gib",
    "Gravity",
    "GrimlockExecution",
    "GrimlockTRexExecution",
    "HeadShot",
    "HitFromBack",
    "HitFromFront",
    "HitFromLeft",
    "HitFromRight",
    "Hovering",
    "InstantKill",
    "LowFramerate",
    "Martyrdom",
    "Melee",
    "MeleeClub",
    "MeleeCrush",
    "MeleeCut",
    "Nanomite",
    "Poke",
    "Rammed",
    "RapidShot",
    "RetroBlaster",
    "RetroScopedPistol",
    "RetroTorqueRifle",
    "Robot",
    "RobotBooster",
    "SelfDestruct",
    "ShockwaveGun",
    "SingleShot",
    "Slow",
    "Stationary",
    "Tank",
    "Thrown",
    "Vehicle",
    "VehicleBoost",
    "VehicleHover",
];

/// A set of death types: what a kill has, what an entry of a death set needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Types(pub u64);

impl Types {
    /// The type of that name (none for a name it doesn't know).
    pub fn of(name: &str) -> Types {
        TYPE_NAMES
            .iter()
            .position(|n| *n == name)
            .map_or(Types(0), |i| Types(1 << i))
    }

    /// Those of a list of names.
    pub fn of_all<'a>(names: impl IntoIterator<Item = &'a str>) -> Types {
        names
            .into_iter()
            .fold(Types(0), |acc, n| acc.with(Types::of(n)))
    }

    pub fn with(self, other: Types) -> Types {
        Types(self.0 | other.0)
    }

    /// Whether it has every one of `needed` (TnDeathPlayer.AreAllTypesActive).
    pub fn has_all(self, needed: Types) -> bool {
        self.0 & needed.0 == needed.0
    }

    /// Their names (for the log).
    pub fn names(self) -> Vec<&'static str> {
        TYPE_NAMES
            .iter()
            .enumerate()
            .filter(|(i, _)| self.0 & (1 << i) != 0)
            .map(|(_, n)| *n)
            .collect()
    }
}

/// The side a blow came from (TnDeathContextFactory.GetDirectionDeathType: where it
/// struck from the pawn's Location, turned into the pawn's own frame, its yaw:
/// within 45° of its front, its left from there to 135°, its right, else its back).
/// `facing` is the way the pawn faces, along the ground.
pub fn side_struck(facing: Vec3, from_pawn: Vec3) -> Types {
    let ahead = Vec3::new(facing.x, 0.0, facing.z).normalize_or(Vec3::NEG_Z);
    let right = ahead.cross(Vec3::Y);
    let (x, y) = (from_pawn.dot(ahead), from_pawn.dot(right));
    if x * x + y * y < 1e-8 {
        return Types::of("HitFromFront");
    }
    // Unreal's yaw of the blow in the pawn's frame: 0 ahead, positive to its right.
    let yaw = y.atan2(x).to_degrees();
    if (-45.0..=45.0).contains(&yaw) {
        Types::of("HitFromFront")
    } else if (-135.0..=-45.0).contains(&yaw) {
        Types::of("HitFromLeft")
    } else if (45.0..=135.0).contains(&yaw) {
        Types::of("HitFromRight")
    } else {
        Types::of("HitFromBack")
    }
}

/// What the death types of a kill are made of (TnDeathContextFactory.FindActiveTypes
/// and its form's subclass's).
#[derive(Clone, Copy, Debug, Default)]
pub struct Kill {
    /// The damage type's DamageDeathTypes.
    pub damage: Types,
    /// The killer was within the damage type's GibDistance (ShouldGib).
    pub gib: bool,
    /// The side the blow came from (side_struck).
    pub side: Types,
    /// Its speed was within StationarySpeed (50 uu a second).
    pub stationary: bool,
    /// It died in a robot form (Robot) or a vehicle one (Vehicle: its alt form's
    /// boost, VehicleBoost, or hover, VehicleHover, when it has one: TnVehicleForm).
    pub robot: bool,
    pub vehicle_boost: Option<bool>,
    /// It was killed while downed (a player's).
    pub downed: bool,
    /// Its form's own: a robot's (TnRobotDeathContextFactory) a headshot, a hover
    /// and being in the air; the Sniper insect's (TnSniperDeathContextFactory) a
    /// headshot and a ceiling.
    pub head: bool,
    pub hovering: bool,
    pub airborne: bool,
    pub ceiling: bool,
    pub factory: Factory,
}

/// Which death context factory a form's death player has (its DeathPlayer's
/// ContextFactory: TnRobotForm's a TnRobotDeathContextFactory, the vehicle forms' the
/// base one, the Sniper's insect's its own).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Factory {
    #[default]
    Robot,
    Vehicle,
    Sniper,
}

impl Kill {
    /// Its death types.
    pub fn types(&self) -> Types {
        let mut t = self.side.with(self.damage);
        if self.stationary {
            t = t.with(Types::of("Stationary"));
        }
        if self.robot {
            t = t.with(Types::of("Robot"));
        } else {
            t = t.with(Types::of("Vehicle"));
            match self.vehicle_boost {
                Some(true) => t = t.with(Types::of("VehicleBoost")),
                Some(false) => t = t.with(Types::of("VehicleHover")),
                None => {}
            }
        }
        if self.downed {
            t = t.with(Types::of("Downed"));
        }
        if self.gib {
            t = t.with(Types::of("Gib"));
        }
        match self.factory {
            Factory::Robot => {
                if self.head {
                    t = t.with(Types::of("HeadShot"));
                }
                if self.hovering {
                    t = t.with(Types::of("Hovering"));
                }
                if self.airborne {
                    t = t.with(Types::of("Airborne"));
                }
            }
            Factory::Sniper => {
                if self.head {
                    t = t.with(Types::of("HeadShot"));
                }
                if self.ceiling {
                    t = t.with(Types::of("Ceiling"));
                }
            }
            Factory::Vehicle => {}
        }
        t
    }
}

// ---------------------------------------------------------------------------
// In the world
// ---------------------------------------------------------------------------

/// Unreal's units to the metre.
pub const UU: f32 = 100.0;

/// Where a death plays out (the dying pawn as it died, in the demo's space): its
/// Location (its collision's middle) and Rotation, where the blow struck
/// (LastTakeHitInfo.Location), the kill's TearOffMomentum (the blow's momentum over
/// the pawn's Mass: metres a second) and the speed the pawn was moving at.
#[derive(Clone, Copy, Debug)]
pub struct Place {
    pub at: Vec3,
    pub turn: Quat,
    pub hit: Vec3,
    pub momentum: Vec3,
    pub velocity: Vec3,
}

impl Place {
    /// A direction in the pawn's frame (Unreal's: forward, right, up), in the world
    /// (its forward the demo's -Z turned by `turn`).
    pub fn dir(&self, v: Vec3) -> Vec3 {
        self.turn * Vec3::new(v.y, v.z, -v.x)
    }

    /// The point `uu` (Unreal units, the pawn's frame: Vector >> Owner.Rotation) from
    /// its Location.
    pub fn point(&self, uu: Vec3) -> Vec3 {
        self.at + self.dir(uu) / UU
    }

    /// The kill's momentum's frame (Rotator(TearOffMomentum): its X along the
    /// momentum, no roll; Unreal's world axes with none), its X, Y and Z in the demo's
    /// space.
    pub fn momentum_axes(&self) -> [Vec3; 3] {
        // (Unreal's axes are the demo's x, z and y.)
        let m = Vec3::new(self.momentum.x, self.momentum.z, self.momentum.y);
        let yaw = m.y.atan2(m.x);
        let pitch = m.z.atan2((m.x * m.x + m.y * m.y).sqrt());
        let (sp, cp) = pitch.sin_cos();
        let (sy, cy) = yaw.sin_cos();
        [
            Vec3::new(cp * cy, cp * sy, sp),
            Vec3::new(-sy, cy, 0.0),
            Vec3::new(-sp * cy, -sp * sy, cp),
        ]
        .map(|a| Vec3::new(a.x, a.z, a.y))
    }
}

/// A direction turned at random within a cone of a half angle at random between
/// `variance`'s two (radians), its length kept (VaryImpulseDirection, RandomVectorInCone;
/// none if either is 0).
fn vary(v: Vec3, variance: (f32, f32), seed: &mut u32) -> Vec3 {
    let len = v.length();
    if variance.0 == 0.0 || variance.1 == 0.0 || len < 1e-6 {
        return v;
    }
    let half = rand_in(seed, variance);
    let dir = v / len;
    let cos = 1.0 - rand(seed) * (1.0 - half.cos());
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let phi = rand(seed) * std::f32::consts::TAU;
    let (a, b) = dir.any_orthonormal_pair();
    (dir * cos + (a * phi.cos() + b * phi.sin()) * sin) * len
}

/// What a Ragdoll action does to its ragdoll as it takes over (OnStartRagdoll:
/// CopyModifiedVelocitiesToRagdoll, then ApplyRemainingRagdollActions), placed in
/// the world: `carry` the speed the whole body moved at besides its clip, the kill's
/// momentum times the action's factor on every body, then its ragdoll actions in
/// their order: each radial impulse from its origin (the pawn, where it was struck,
/// a bone) and its offset in the pawn's frame, each impulse turned by the
/// momentum's frame (a turn's axis the other way round, Unreal's axes being
/// left-handed) on its bone (`struck`: the bone the kill's round struck, if it
/// named one; a blow's or a blast's names none, the root body's) and at its point,
/// each sever.
pub fn kick(
    rd: &RagdollDo,
    place: &Place,
    struck: Option<&str>,
    carry: Vec3,
    seed: &mut u32,
) -> crate::ragdoll::Kick {
    use crate::ragdoll::{Kick, KickAction};
    let axes = place.momentum_axes();
    let rel = |v: Vec3| axes[0] * v.x + axes[1] * v.y + axes[2] * v.z;
    // (A point at a source (ETnRagdollImpulseOrigin) and its offset: the pawn's
    // place, or where it was struck; at a bone, the offset from it.)
    let at_bone = |bone: Option<&str>, offset: Vec3| match bone {
        Some(b) => (place.dir(offset) / UU, Some(b.to_string())),
        None => (place.hit + place.dir(offset) / UU, None),
    };
    let mut actions = Vec::new();
    for a in &rd.actions {
        match a {
            RagdollAct::Radial(r) => {
                let (origin, bone) = match r.origin {
                    0 => (place.point(r.offset), None),
                    2 => at_bone(r.bone.as_deref(), r.offset),
                    3 => at_bone(struck, r.offset),
                    _ => (place.hit + place.dir(r.offset) / UU, None),
                };
                actions.push(KickAction::Radial {
                    origin,
                    bone,
                    radius: r.radius / UU,
                    strength: rand_in(seed, r.magnitude) / UU,
                    linear: r.linear,
                    velocity_change: r.velocity_change,
                });
            }
            RagdollAct::Halt => actions.push(KickAction::Halt),
            RagdollAct::Impulse(i) => actions.push(KickAction::Impulse {
                bone: match i.target {
                    0 => i.bone.as_ref().map(|b| b.to_string()),
                    _ => struck.map(str::to_string),
                },
                linear: vary(rel(i.linear), i.variance, seed) / UU,
                angular: -rel(i.angular),
                mass: i.mass,
                // (At a bone (2, 3): where it was struck, near enough; none of the
                // game's deaths applies one there.)
                at: i.at.map(|(source, offset)| match source {
                    0 => place.point(offset),
                    _ => place.hit + place.dir(offset) / UU,
                }),
            }),
            RagdollAct::Sever(s) => actions.push(KickAction::Sever {
                bones: s.bones.iter().map(|b| b.to_string()).collect(),
                random: s.random,
                near: s.near.map(|r| (place.hit, r / UU)),
                trail: s.trail.clone(),
                chance: s.chance,
            }),
        }
    }
    let seed = (rand(seed) * u32::MAX as f32) as u32;
    Kick {
        carry,
        momentum: place.momentum,
        momentum_k: rd.momentum,
        direction: rd.direction,
        actions,
        seed,
    }
}

// ---------------------------------------------------------------------------
// The data
// ---------------------------------------------------------------------------

/// An entry of a death set: the types a kill needs for it (RequiredTypes), and its
/// deaths: one, or a conditional set's with their weights.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: Arc<str>,
    pub required: Types,
    pub members: Vec<(u32, Arc<str>)>,
}

/// A death: its timeline's keyframes, each its actions.
#[derive(Clone, Debug, Default)]
pub struct DeathDef {
    pub name: Arc<str>,
    pub keys: Vec<Vec<Act>>,
}

/// An action of a keyframe: its Delay (at random between two), its AfterActionWait
/// (`hold`: how long more it waits once it is done) and what it does.
#[derive(Clone, Debug)]
pub struct Act {
    pub delay: (f32, f32),
    pub hold: f32,
    pub what: Do,
}

/// A radial impulse a ragdoll takes as it starts (TnRagdollActionRadialImpulse:
/// AddRadialImpulse from its origin, its magnitude at random between two, out to
/// its FalloffRadius, the same all the way out or less the further (its
/// FalloffType), on every body: a change in speed (ConsiderBodyMass off) or an
/// impulse on its mass).
#[derive(Clone, Debug, PartialEq)]
pub struct Radial {
    pub magnitude: (f32, f32),
    /// uu.
    pub radius: f32,
    pub linear: bool,
    /// ExplosionOrigin (ETnRagdollImpulseOrigin): 0 the pawn, 1 where it was
    /// struck, 2 its bone (BoneName), 3 the bone struck; its offset in the pawn's
    /// frame (uu: forward, right, up).
    pub origin: u8,
    pub offset: Vec3,
    pub bone: Option<Arc<str>>,
    pub velocity_change: bool,
}

/// An impulse on the ragdoll as it starts (TnRagdollActionAddImpulse's Impulses):
/// `linear` and `angular` in the kill's momentum's frame (ImpulseRelativeToMomentum,
/// uu a second; AngularVelocityRelativeToMomentum), on the bone it names (target 0,
/// ETRDIT_SpecificBone: none, the root body) or the bone struck (1, ETRDIT_HitBone);
/// an impulse on the mass or (`mass` off) a change in speed; its direction turned
/// at random within the variance (radians); applied at a point (`at`:
/// UsePointImpulse's PointSource, an ETnRagdollImpulseOrigin as a radial's
/// origin, and its PointOffset, uu in the pawn's frame) or at its centre of mass.
#[derive(Clone, Debug, PartialEq)]
pub struct Impulse {
    pub linear: Vec3,
    pub angular: Vec3,
    pub target: u8,
    pub bone: Option<Arc<str>>,
    pub mass: bool,
    pub variance: (f32, f32),
    pub at: Option<(u8, Vec3)>,
}

/// Pieces a ragdoll cuts away (TnRagdollActionSeverPieces: its bones' pieces;
/// TnRagdollActionSeverRandomPieces: `random` of them at random), only those within
/// `near` (uu) of where it was struck if it says (UseMinDistanceFromHit), each
/// trailing its effect at its chance (SeveredPieceParticleTemplate,
/// SeveredPieceEffectProbability).
#[derive(Clone, Debug, PartialEq)]
pub struct Sever {
    pub bones: Vec<Arc<str>>,
    pub random: u32,
    pub near: Option<f32>,
    pub trail: Option<Arc<str>>,
    pub chance: f32,
}

/// A ragdoll action (its RagdollActions: applied in their order as it takes over).
#[derive(Clone, Debug, PartialEq)]
pub enum RagdollAct {
    Radial(Radial),
    /// TnRagdollActionHaltBodies: its bodies stopped.
    Halt,
    Impulse(Impulse),
    Sever(Sever),
}

/// A Ragdoll action's settings (TnDeathActionRagdoll).
#[derive(Clone, Debug, PartialEq)]
pub struct RagdollDo {
    /// MaximumRagdollSettleTime: done this long after the ragdoll took over.
    pub settle: f32,
    /// VelocityTransferTimeBeforeAnimEnds (UseVelocityTransferTime): it takes over
    /// this long before the death's clip ends; None: at once.
    pub before: Option<f32>,
    /// The kill's momentum on every body (TearOffMomentum times this, at random
    /// between two for each body: MomentumImpulseVariance), and the direction of each
    /// body's speed turned at random within the variance (DirectionImpulseVariance,
    /// radians).
    pub momentum: (f32, f32),
    pub direction: (f32, f32),
    pub actions: Vec<RagdollAct>,
}

impl RagdollDo {
    /// Its radial impulses (the tests').
    #[cfg(test)]
    pub fn radials(&self) -> impl Iterator<Item = &Radial> {
        self.actions.iter().filter_map(|a| match a {
            RagdollAct::Radial(r) => Some(r),
            _ => None,
        })
    }
}

/// A light a death turns on (TnDeathActionTriggerLight's RampLightProperties, played
/// by its pawn's TnRampLightManager): its brightness ramping in over `ramp_in` and
/// out over the last `ramp_out` of its `secs`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightDo {
    pub brightness: f32,
    /// uu.
    pub radius: f32,
    pub falloff: f32,
    /// sRGB.
    pub color: [u8; 3],
    pub ramp_in: f32,
    pub ramp_out: f32,
    pub secs: f32,
    /// SyncPositionWithBounds: kept on the body's bounds' middle, else at its
    /// Translation from the pawn (uu, its frame).
    pub sync: bool,
    pub at: Vec3,
}

impl LightDo {
    /// Its brightness `t` seconds in (TnRampLightManager.UpdateLight), None once it
    /// is over.
    pub fn brightness_at(&self, t: f32) -> Option<f32> {
        if t >= self.secs {
            return None;
        }
        let out_start = self.secs - self.ramp_out;
        let alpha = if t >= out_start && self.ramp_out > 0.0 {
            1.0 - (t - out_start) / self.ramp_out
        } else if self.ramp_in > 0.0 {
            (t / self.ramp_in).min(1.0)
        } else {
            1.0
        };
        Some(self.brightness * alpha)
    }
}

/// A camera shake (TnDeathActionCameraShake: every local player within its radius
/// has it all; its ScreenShakeStruct in Unreal's units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShakeDo {
    /// uu.
    pub radius: f32,
    pub secs: f32,
    pub rot: Vec3,
    pub rot_freq: Vec3,
    pub loc: Vec3,
    pub loc_freq: Vec3,
    pub fov: (f32, f32),
}

/// What an action does (its class), and when it is done:
#[derive(Clone, Debug, PartialEq)]
pub enum Do {
    /// TnDeathActionPlayAnimation: its clip (AnimName at its PlayRate), done as it
    /// ends or once it has played CutoffTime (if it has one; a looping clip only
    /// then).
    Anim {
        name: Arc<str>,
        rate: f32,
        looping: bool,
        cutoff: f32,
        blend: f32,
    },
    /// TnDeathActionRagdoll: the ragdoll takes over (at once, or before the clip
    /// ends); done MaximumRagdollSettleTime after it did (not as it comes to rest).
    Ragdoll(Box<RagdollDo>),
    /// TnDeathActionWait: WaitTime.
    Wait(f32, f32),
    /// TnDeathActionHighlight: its Deresser runs (TnDeresser: its value from
    /// StartingRampValue up to 1 over its Duration, from DelayUntilRampStart on; the
    /// mesh drawn no more past NormalizedTimeToMakeMeshInvisible, only its overlay;
    /// its Sound as it starts); done once it has deressed, or after CutoffTime; done
    /// too while it waits out its delay (its IsDone's quirk). Its overlay reaches
    /// `radius` times the mesh's bounds' radius times 1.2 (RadiusAdditionalScale),
    /// its noise at its Deresser's MaterialTiling.
    Deres {
        secs: f32,
        start_look: DeresStart,
        invisible: f32,
        cutoff: f32,
        start: f32,
        ramp_delay: f32,
        sound: Option<&'static str>,
    },
    /// TnDeathActionPlaySound: its cue (as build_sounds.py names it), or its sound
    /// event; done at once.
    Sound(&'static str),
    Event(Arc<str>),
    /// TnDeathActionPlayEffect: its particle systems on their sockets (or the bones of
    /// that name), held there; all, or one at random (PlayType); done at once.
    Effect {
        systems: Vec<(Arc<str>, Arc<str>)>,
        random: bool,
    },
    /// TnDeathActionCameraShake; done at once.
    Shake(ShakeDo),
    /// TnDeathActionSpawnDestructible: one of them at random, set off; done at once.
    Destructible(Vec<Arc<str>>),
    /// TnDeathActionTriggerLight: done once its light is out.
    Light(LightDo),
    /// TnDeathActionSetGravityFactor: the ragdoll's gravity times this, set each
    /// frame it runs; done at once.
    Gravity(f32),
    /// TnDeathActionApplyAngularForce: a local angular acceleration (roll, pitch, yaw
    /// at random between two each), once, or every frame (ApplyContinuousAcceleration)
    /// while its keyframe runs; done at once.
    Spin {
        roll: (f32, f32),
        pitch: (f32, f32),
        yaw: (f32, f32),
        every: bool,
    },
    /// TnDeathActionCollisionTimer: done DurationBeforeCollision after it starts, or
    /// DurationAfterCollision after the ragdoll first touches something.
    Collide {
        before: (f32, f32),
        after: (f32, f32),
    },
    /// TnDeathActionPlaneMotionControl: a plane keeps boosting; done at once.
    Boost,
    /// TnDeathActionSetMeshHidden: its form hidden; done at once.
    Hide,
    /// TnDeathModifierControlBone: a SkelControlSingleBone's BoneScale; done at once.
    Bone {
        node: Arc<str>,
        scale: f32,
    },
    /// Anything else (MakeNoise, SetPhysicsAsset...): done at once.
    Other,
}

/// A destructible's piece effects (models/enemies/deaths.txt's dsys lines).
#[derive(Clone, Debug, Default)]
pub struct Destructible {
    pub pieces: Vec<Piece>,
}

/// Gibs (HmGibsDestructionEffectFactory, HmDebrisDestructionEffectFactory): `count`
/// chunks of each of its meshes in turn... (see tools/build_death_sets.py).
#[derive(Clone, Debug, PartialEq)]
pub struct Gibs {
    pub meshes: Vec<Arc<str>>,
    pub count: u32,
    /// Where on a sphere round its place they start (SpawnVolumeScale, uu).
    pub volume: Vec3,
    /// LinearSpeed (uu a second) within VelocityConeAngle (degrees, all round its
    /// orientation's forward), AngularSpeed.
    pub speed: (f32, f32),
    pub spin: (f32, f32),
    pub cone: f32,
    pub lifespan: f32,
    pub trail: Option<Arc<str>>,
    /// Its place (uu) and orientation (pitch, yaw, roll, Unreal's units) in the
    /// pawn's frame.
    pub at: Vec3,
    pub turn: Vec3,
    /// Their deres: the last `deres` seconds of their lifespan; the sound each plays
    /// while it flies and as it deresses; how much of the body's speed they take on.
    pub deres: f32,
    pub sound: Option<&'static str>,
    pub deres_sound: Option<&'static str>,
    pub carry: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    /// A particle system at its offset (uu, the pawn's frame).
    Particle {
        system: Arc<str>,
        at: Vec3,
    },
    Gibs(Gibs),
    Sound(&'static str),
    /// A decal laid on what is under it: its material, size (uu, at random up to
    /// the ranges more), thickness, lifetime and place.
    Decal {
        material: Arc<str>,
        size: (f32, f32),
        range: (f32, f32),
        thickness: f32,
        lifetime: f32,
        at: Vec3,
    },
    /// A blast's damage (HurtRadius: its damage type, damage, radius and momentum,
    /// uu, at its offset).
    Damage {
        kind: Arc<str>,
        damage: f32,
        radius: f32,
        momentum: f32,
        at: Vec3,
    },
    /// What the demo doesn't do (TnSkeletalGibDestructionEffectFactory: the body's
    /// own pieces flung).
    Other,
}

/// The death sets, their deaths and the destructibles.
#[derive(Resource, Clone, Debug, Default)]
pub struct DeathData {
    pub sets: HashMap<String, Vec<Entry>>,
    pub deaths: HashMap<Arc<str>, Arc<DeathDef>>,
    pub destructibles: HashMap<String, Arc<Destructible>>,
}

fn floats(s: &str) -> Vec<f32> {
    s.split([',', ' '])
        .filter(|x| !x.is_empty())
        .filter_map(|x| x.parse().ok())
        .collect()
}

fn range(s: &str) -> (f32, f32) {
    let v = floats(s);
    match v.len() {
        0 => (0.0, 0.0),
        1 => (v[0], v[0]),
        _ => (v[0], v[1]),
    }
}

fn vec3(s: &str) -> Vec3 {
    let v = floats(s);
    if v.len() >= 3 {
        Vec3::new(v[0], v[1], v[2])
    } else {
        Vec3::ZERO
    }
}

fn name(s: &str) -> Option<Arc<str>> {
    let s = s.trim();
    (!s.is_empty() && s != "-" && s != "None").then(|| Arc::from(s))
}

/// A sound cue's name, kept for as long as the game runs (the sound queue takes
/// static names; each cue is kept once): None for none.
fn cue(s: &str) -> Option<&'static str> {
    use std::sync::{Mutex, OnceLock};
    static CUES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let s = s.trim();
    if s.is_empty() || s == "-" || s == "None" {
        return None;
    }
    let mut cues = CUES.get_or_init(Default::default).lock().ok()?;
    if let Some(c) = cues.get(s) {
        return Some(c);
    }
    let c: &'static str = Box::leak(s.to_string().into_boxed_str());
    cues.insert(s.to_string(), c);
    Some(c)
}

/// The short name of a full one (its last part: Basic_DEATHSET).
pub fn short(full: &str) -> &str {
    full.rsplit('.').next().unwrap_or(full)
}

impl DeathData {
    pub fn load(dir: &std::path::Path) -> DeathData {
        match std::fs::read_to_string(dir.join(DEATHS_FILE)) {
            Ok(text) => DeathData::parse(&text),
            Err(e) => {
                warn!("{DEATHS_FILE} can't be read ({e}): the deaths are ragdolls");
                DeathData::default()
            }
        }
    }

    pub fn parse(text: &str) -> DeathData {
        let mut out = DeathData::default();
        let mut deaths: HashMap<Arc<str>, DeathDef> = HashMap::new();
        let mut dsys: HashMap<String, Destructible> = HashMap::new();
        for line in text.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let f: Vec<&str> = line.split('\t').collect();
            match f[0] {
                "set" if f.len() >= 4 => {
                    let required = if f[3] == "-" {
                        Types(0)
                    } else {
                        Types::of_all(f[3].split(','))
                    };
                    let entry = Arc::<str>::from(f[2]);
                    out.sets.entry(f[1].to_string()).or_default().push(Entry {
                        name: entry.clone(),
                        required,
                        // (A death of its own until a member line says otherwise.)
                        members: vec![(1, entry)],
                    });
                }
                "member" if f.len() >= 5 => {
                    if let Some(e) = out
                        .sets
                        .get_mut(f[1])
                        .and_then(|list| list.iter_mut().rev().find(|e| &*e.name == f[2]))
                    {
                        if e.members.len() == 1 && e.members[0].1 == e.name {
                            e.members.clear();
                        }
                        e.members.push((f[3].parse().unwrap_or(1), Arc::from(f[4])));
                    }
                }
                "act" if f.len() >= 6 => {
                    let key: usize = f[2].parse().unwrap_or(0);
                    let delay = (f[3].parse().unwrap_or(0.0), f[4].parse().unwrap_or(0.0));
                    let kv: Vec<(&str, &str)> =
                        f[6..].iter().filter_map(|x| x.split_once('=')).collect();
                    let get = |k: &str| kv.iter().find(|(n, _)| *n == k).map(|(_, v)| *v);
                    let num = |k: &str, d: f32| get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
                    let what = match f[5] {
                        "anim" => Do::Anim {
                            name: Arc::from(get("name").unwrap_or("None")),
                            rate: num("rate", 1.0),
                            looping: get("loop") == Some("1"),
                            cutoff: num("cutoff", 0.0),
                            blend: num("blend", 0.1),
                        },
                        "ragdoll" => Do::Ragdoll(Box::new(RagdollDo::parse(&kv))),
                        "wait" => {
                            let (lo, hi) = range(get("secs").unwrap_or("0"));
                            Do::Wait(lo, hi)
                        }
                        "deres" => Do::Deres {
                            secs: num("secs", 3.0),
                            start_look: DeresStart {
                                overlay: Arc::from(get("overlay").unwrap_or("-")),
                                radius: num("radius", 1.0),
                                tiling: num("tiling", 3.0),
                            },
                            invisible: num("invisible", 0.39685),
                            cutoff: num("cutoff", 0.0),
                            start: num("start", 0.0),
                            ramp_delay: num("ramp_delay", 0.0),
                            sound: get("sound").and_then(cue),
                        },
                        "sound" => match (get("cue"), get("event")) {
                            (Some(c), _) => cue(c).map_or(Do::Other, Do::Sound),
                            (None, Some(e)) => Do::Event(Arc::from(e)),
                            _ => Do::Other,
                        },
                        "effect" => Do::Effect {
                            systems: get("systems")
                                .unwrap_or("")
                                .split(',')
                                .filter_map(|s| s.split_once('@'))
                                .map(|(sys, socket)| (Arc::from(sys), Arc::from(socket)))
                                .collect(),
                            random: get("random") == Some("1"),
                        },
                        "shake" => {
                            let fov = range(get("fov").unwrap_or("2,5"));
                            Do::Shake(ShakeDo {
                                radius: num("radius", 0.0),
                                secs: num("secs", 1.0),
                                rot: vec3(get("rot").unwrap_or("")),
                                rot_freq: vec3(get("rotf").unwrap_or("")),
                                loc: vec3(get("loc").unwrap_or("")),
                                loc_freq: vec3(get("locf").unwrap_or("")),
                                fov,
                            })
                        }
                        "dsys" => Do::Destructible(
                            get("names")
                                .unwrap_or("")
                                .split(',')
                                .filter_map(name)
                                .collect(),
                        ),
                        "light" => {
                            let c = floats(get("color").unwrap_or("255,255,255"));
                            let c = |i: usize| c.get(i).copied().unwrap_or(255.0) as u8;
                            Do::Light(LightDo {
                                brightness: num("brightness", 25.0),
                                radius: num("radius", 512.0),
                                falloff: num("falloff", 10.0),
                                color: [c(0), c(1), c(2)],
                                ramp_in: num("ramp_in", 0.5),
                                ramp_out: num("ramp_out", 0.0),
                                secs: num("secs", 10.0),
                                sync: get("sync") == Some("1"),
                                at: vec3(get("at").unwrap_or("")),
                            })
                        }
                        "gravity" => Do::Gravity(num("factor", 1.0)),
                        "spin" => Do::Spin {
                            roll: range(get("roll").unwrap_or("0")),
                            pitch: range(get("pitch").unwrap_or("0")),
                            yaw: range(get("yaw").unwrap_or("0")),
                            every: get("every") == Some("1"),
                        },
                        "collide" => Do::Collide {
                            before: range(get("before").unwrap_or("0")),
                            after: range(get("after").unwrap_or("0")),
                        },
                        "boost" => Do::Boost,
                        "hide" => Do::Hide,
                        "bone" => Do::Bone {
                            node: Arc::from(get("node").unwrap_or("None")),
                            scale: num("scale", 1.0),
                        },
                        _ => Do::Other,
                    };
                    let death = Arc::<str>::from(f[1]);
                    let def = deaths.entry(death.clone()).or_insert_with(|| DeathDef {
                        name: death,
                        keys: Vec::new(),
                    });
                    while def.keys.len() <= key {
                        def.keys.push(Vec::new());
                    }
                    def.keys[key].push(Act {
                        delay,
                        hold: num("hold", 0.0),
                        what,
                    });
                }
                "dsys" if f.len() >= 3 => {
                    let d = dsys.entry(f[1].to_string()).or_default();
                    let piece = match f[2] {
                        "particle" if f.len() >= 5 => Piece::Particle {
                            system: Arc::from(f[3]),
                            at: vec3(f[4]),
                        },
                        "sound" if f.len() >= 4 => match cue(f[3]) {
                            Some(c) => Piece::Sound(c),
                            None => Piece::Other,
                        },
                        "decal" if f.len() >= 11 => Piece::Decal {
                            material: Arc::from(f[3]),
                            size: (f[4].parse().unwrap_or(0.0), f[5].parse().unwrap_or(0.0)),
                            range: (f[6].parse().unwrap_or(0.0), f[7].parse().unwrap_or(0.0)),
                            thickness: f[8].parse().unwrap_or(0.0),
                            lifetime: f[9].parse().unwrap_or(0.0),
                            at: vec3(f[10]),
                        },
                        "damage" if f.len() >= 8 => Piece::Damage {
                            kind: Arc::from(f[3]),
                            damage: f[4].parse().unwrap_or(0.0),
                            radius: f[5].parse().unwrap_or(0.0),
                            momentum: f[6].parse().unwrap_or(0.0),
                            at: vec3(f[7]),
                        },
                        "gibs" if f.len() >= 16 => Piece::Gibs(Gibs {
                            meshes: f[3].split(',').filter_map(name).collect(),
                            count: f[4].parse().unwrap_or(1),
                            volume: vec3(f[5]),
                            speed: range(f[6]),
                            spin: range(f[7]),
                            cone: f[8].parse().unwrap_or(360.0),
                            lifespan: f[9].parse().unwrap_or(5.0),
                            trail: name(f[10]),
                            at: vec3(f[11]),
                            turn: vec3(f[12]),
                            deres: f[13].parse().unwrap_or(0.0),
                            sound: cue(f[14]),
                            deres_sound: cue(f[15]),
                            carry: f.get(16).and_then(|x| x.parse().ok()).unwrap_or(1.0),
                        }),
                        _ => Piece::Other,
                    };
                    d.pieces.push(piece);
                }
                _ => {}
            }
        }
        out.deaths = deaths.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
        out.destructibles = dsys.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
        out
    }

    /// The death `set` (its short name: Basic_DEATHSET) plays for a kill of `types`
    /// (TnDeathPlayer.Pick: the first entry whose types it has; a conditional set's
    /// member by its weight, `roll` 0 to 1), if any.
    pub fn pick(&self, set: &str, types: Types, roll: f32) -> Option<Arc<DeathDef>> {
        let entry = self
            .sets
            .get(set)?
            .iter()
            .find(|e| types.has_all(e.required))?;
        let total: u32 = entry.members.iter().map(|m| m.0.max(1)).sum();
        let mut at = (roll.clamp(0.0, 0.999_999) * total as f32) as u32;
        for (w, death) in &entry.members {
            let w = (*w).max(1);
            if at < w {
                return self.deaths.get(death).cloned();
            }
            at -= w;
        }
        entry
            .members
            .last()
            .and_then(|(_, d)| self.deaths.get(d).cloned())
    }
}

impl RagdollDo {
    fn parse(kv: &[(&str, &str)]) -> RagdollDo {
        let get = |k: &str| kv.iter().find(|(n, _)| *n == k).map(|(_, v)| *v);
        let num = |k: &str, d: f32| get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        let before = num("before", 0.1);
        let mut out = RagdollDo {
            settle: num("settle", 10.0),
            before: (before >= 0.0).then_some(before),
            momentum: range(get("momentum").unwrap_or("1,1")),
            direction: range(get("direction").unwrap_or("0,0")),
            actions: Vec::new(),
        };
        // (In their order, as the data lists them.)
        for (k, v) in kv {
            match *k {
                "radial" => {
                    let x = floats(v);
                    if x.len() >= 9 {
                        // (Its bone, for an origin at one: the name after the numbers.)
                        let bone = v
                            .rsplit(',')
                            .next()
                            .filter(|b| b.parse::<f32>().is_err())
                            .and_then(name);
                        out.actions.push(RagdollAct::Radial(Radial {
                            magnitude: (x[0], x[1]),
                            radius: x[2],
                            linear: x[3] != 0.0,
                            origin: x[4] as u8,
                            offset: Vec3::new(x[5], x[6], x[7]),
                            bone,
                            velocity_change: x[8] != 0.0,
                        }));
                    }
                }
                "halt" => out.actions.push(RagdollAct::Halt),
                "impulse" => {
                    let p: Vec<&str> = v.split(';').collect();
                    out.actions.push(RagdollAct::Impulse(Impulse {
                        linear: vec3(p.first().copied().unwrap_or("")),
                        angular: vec3(p.get(1).copied().unwrap_or("")),
                        target: p.get(2).and_then(|t| t.parse().ok()).unwrap_or(2),
                        bone: p.get(3).and_then(|b| name(b)),
                        mass: p.get(4) == Some(&"1"),
                        variance: range(p.get(5).copied().unwrap_or("0,0")),
                        at: p.get(6).and_then(|a| {
                            let x = floats(a);
                            (x.len() >= 4).then(|| (x[0] as u8, Vec3::new(x[1], x[2], x[3])))
                        }),
                    }));
                }
                "sever" => {
                    let p: Vec<&str> = v.split(';').collect();
                    let bones = p
                        .first()
                        .copied()
                        .unwrap_or("")
                        .split(',')
                        .filter_map(name)
                        .collect();
                    out.actions.push(RagdollAct::Sever(Sever {
                        bones,
                        trail: p.get(1).and_then(|t| name(t)),
                        chance: p.get(2).and_then(|x| x.parse().ok()).unwrap_or(1.0),
                        random: p.get(3).and_then(|x| x.parse().ok()).unwrap_or(0),
                        near: p.get(4).and_then(|x| x.parse().ok()),
                    }));
                }
                _ => {}
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// The timeline
// ---------------------------------------------------------------------------

/// A random number 0 to 1 from a seed (the demo's deaths' own, so a death plays out
/// the same on every PC that knows its seed).
pub fn rand(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed >> 8) as f32 / (1u32 << 24) as f32
}

fn rand_in(seed: &mut u32, (lo, hi): (f32, f32)) -> f32 {
    lo + (hi - lo) * rand(seed)
}

/// An action as it runs (TnDeathAction's timers and its class's own state).
#[derive(Clone, Debug, Default)]
struct Running {
    /// Its DelayTimer's time left (while it waits to start), its WaitTimer's
    /// (AfterActionWait, once it is done: None until then).
    delay: Option<f32>,
    hold: Option<f32>,
    started: bool,
    /// Its class's: a clip's PlayDuration and whether it plays (and its length); a
    /// ragdoll's start (its delay timer, and the clock as it took over); a wait's,
    /// a collision timer's or a dissolve's time; a spin's acceleration.
    time: f32,
    playing: bool,
    length: f32,
    initialized: bool,
    ragdoll_in: Option<f32>,
    ragdoll_at: Option<f32>,
    duration: f32,
    collided: bool,
    spin: Vec3,
}

/// What a death calls for this frame (its actions as they start and run).
#[derive(Debug, Default)]
pub struct Beat {
    /// A clip to play now: its name, speed, whether it loops, its blend in.
    pub anim: Option<(Arc<str>, f32, bool, f32)>,
    /// The ragdoll to take over now, so.
    pub ragdoll: Option<RagdollDo>,
    pub sounds: Vec<&'static str>,
    pub events: Vec<Arc<str>>,
    /// Particle systems to start, each on its socket (or bone).
    pub effects: Vec<(Arc<str>, Arc<str>)>,
    pub shakes: Vec<ShakeDo>,
    pub destructibles: Vec<Arc<str>>,
    /// The dissolve begun now (its overlay: TR_DefragOverlay_MATINST..., and what its
    /// reach is scaled by), and how far it has come since (the deresser's value, 0
    /// to 1, and past which the mesh is drawn no more).
    pub deres_start: Option<DeresStart>,
    pub deres: Option<(f32, f32)>,
    /// The death light as it is now: its settings and brightness.
    pub light: Option<(LightDo, f32)>,
    /// The ragdoll's gravity times this (set this frame).
    pub gravity: Option<f32>,
    /// A local angular acceleration on the ragdoll this frame (roll, pitch, yaw;
    /// once: `spin_once`).
    pub spin: Vec3,
    pub boost: bool,
    pub hide: bool,
    pub bones: Vec<(Arc<str>, f32)>,
    /// Its timeline is over: the body goes (its LifeSpan, 0.1 s).
    pub over: bool,
}

/// How a dissolve draws its overlay (TnDeathActionHighlight's Deresser): its
/// material (TR_DefragOverlay_MATINST...), what its reach (the mesh's bounds'
/// radius times 1.2) is scaled by, and its noise's tiling.
#[derive(Clone, Debug, PartialEq)]
pub struct DeresStart {
    pub overlay: Arc<str>,
    pub radius: f32,
    pub tiling: f32,
}

impl Default for DeresStart {
    /// TnDeresser's own: TR_DefragOverlay_MATINST, tiled 3 times.
    fn default() -> Self {
        DeresStart {
            overlay: Arc::from("TR_DefragOverlay_MATINST"),
            radius: 1.0,
            tiling: 3.0,
        }
    }
}

/// What a death's timeline needs to know of its body each frame.
pub struct Env<'a> {
    /// Its model's clip of that name, if it has one: its length at the speed it
    /// would play at (seconds).
    pub clip: &'a dyn Fn(&str, f32) -> Option<f32>,
    /// Its ragdoll has touched something since the last frame.
    pub collided: bool,
}

/// A death as it plays out: its keyframe and its actions' states, the death light
/// (its pawn's one TnRampLightManager), its dissolve's value.
#[derive(Clone, Debug)]
pub struct Timeline {
    pub def: Arc<DeathDef>,
    key: usize,
    running: Vec<Running>,
    /// Its clock (the world's time since the death).
    t: f32,
    /// Its clip's time left (TnAnimatorService.GetAnimationTimeRemaining), as of its
    /// last frame.
    clip_left: f32,
    light: Option<(LightDo, f32)>,
    deres: Option<(f32, f32)>,
    seed: u32,
    started: bool,
    over_for: Option<f32>,
}

/// How long a pawn whose death is over stays (TnDeathPlayer.DestroyOwner: its
/// LifeSpan).
pub const LIFESPAN: f32 = 0.1;

impl Timeline {
    pub fn new(def: Arc<DeathDef>, seed: u32) -> Timeline {
        Timeline {
            def,
            key: 0,
            running: Vec::new(),
            t: 0.0,
            clip_left: 0.0,
            light: None,
            deres: None,
            seed: seed.max(1),
            started: false,
            over_for: None,
        }
    }

    /// Whether its last keyframe is done (TnDeathTimeline.get_IsDone).
    pub fn done(&self) -> bool {
        self.key >= self.def.keys.len()
    }

    /// Whether the body should go now (its LifeSpan out since the timeline ended).
    pub fn gone(&self) -> bool {
        self.over_for.is_some_and(|t| t >= LIFESPAN)
    }

    /// The dissolve's value and where the mesh goes, if it has begun.
    pub fn deres(&self) -> Option<(f32, f32)> {
        self.deres
    }

    /// TnDeathTimeline.StartCurrentKeyframe: each of its actions started (OnStart at
    /// once for one without a delay).
    fn start_key(&mut self, beat: &mut Beat, env: &Env) {
        self.running.clear();
        if self.done() {
            return;
        }
        let def = self.def.clone();
        for act in &def.keys[self.key] {
            let mut r = Running::default();
            if act.delay.1 > 0.0 {
                r.delay = Some(rand_in(&mut self.seed, act.delay));
            } else {
                self.on_start(act, &mut r, beat, env);
            }
            self.running.push(r);
        }
    }

    fn on_start(&mut self, act: &Act, r: &mut Running, beat: &mut Beat, env: &Env) {
        r.started = true;
        match &act.what {
            Do::Anim {
                name,
                rate,
                looping,
                blend,
                ..
            } => match (env.clip)(name, *rate) {
                Some(len) => {
                    r.playing = true;
                    r.time = 0.0;
                    r.length = if *looping { f32::MAX } else { len };
                    self.clip_left = r.length;
                    beat.anim = Some((name.clone(), *rate, *looping, *blend));
                }
                // (A clip its model hasn't got: nothing plays, done at once.)
                None => r.playing = false,
            },
            Do::Ragdoll(_) => {
                r.initialized = false;
            }
            Do::Wait(lo, hi) => r.time = rand_in(&mut self.seed, (*lo, *hi)),
            Do::Deres {
                start_look,
                start,
                invisible,
                sound,
                ..
            } => {
                r.time = 0.0;
                self.deres = Some((*start, *invisible));
                beat.deres_start = Some(start_look.clone());
                beat.sounds.extend(sound.iter().copied());
            }
            Do::Sound(cue) => beat.sounds.push(*cue),
            Do::Event(e) => beat.events.push(e.clone()),
            Do::Effect { systems, random } => {
                if *random && !systems.is_empty() {
                    let i = ((rand(&mut self.seed) * systems.len() as f32) as usize)
                        .min(systems.len() - 1);
                    beat.effects.push(systems[i].clone());
                } else {
                    beat.effects.extend(systems.iter().cloned());
                }
            }
            Do::Shake(s) => beat.shakes.push(*s),
            Do::Destructible(list) => {
                if !list.is_empty() {
                    let i =
                        ((rand(&mut self.seed) * list.len() as f32) as usize).min(list.len() - 1);
                    beat.destructibles.push(list[i].clone());
                }
            }
            Do::Light(l) => self.light = Some((*l, 0.0)),
            Do::Spin {
                roll, pitch, yaw, ..
            } => {
                r.spin = Vec3::new(
                    rand_in(&mut self.seed, *roll),
                    rand_in(&mut self.seed, *pitch),
                    rand_in(&mut self.seed, *yaw),
                );
                beat.spin += r.spin;
            }
            Do::Collide { before, .. } => {
                r.duration = rand_in(&mut self.seed, *before);
                r.time = 0.0;
            }
            Do::Boost => beat.boost = true,
            Do::Hide => beat.hide = true,
            Do::Bone { node, scale } => beat.bones.push((node.clone(), *scale)),
            Do::Gravity(_) | Do::Other => {}
        }
    }

    /// TnDeathAction.OnTick, by its class.
    fn on_tick(&mut self, act: &Act, r: &mut Running, dt: f32, beat: &mut Beat, env: &Env) {
        match &act.what {
            Do::Anim { .. } => {
                if r.playing {
                    r.time += dt;
                    if r.time >= r.length {
                        r.playing = false;
                    }
                    self.clip_left = (r.length - r.time).max(0.0);
                }
            }
            Do::Ragdoll(rd) => {
                if !r.initialized {
                    // InitializeRagdollTimer: before the clip ends, or at once.
                    r.initialized = true;
                    let wait = rd.before.map_or(0.0, |b| (self.clip_left - b).max(0.0));
                    if wait > 0.0 {
                        r.ragdoll_in = Some(wait);
                    } else {
                        r.ragdoll_at = Some(self.t);
                        beat.ragdoll = Some((**rd).clone());
                    }
                }
                if let Some(left) = r.ragdoll_in {
                    let left = left - dt;
                    if left <= 0.0 {
                        r.ragdoll_in = None;
                        r.ragdoll_at = Some(self.t);
                        beat.ragdoll = Some((**rd).clone());
                    } else {
                        r.ragdoll_in = Some(left);
                    }
                }
            }
            Do::Wait(..) => r.time -= dt,
            Do::Deres {
                secs, ramp_delay, ..
            } => {
                r.time += dt;
                if r.time >= *ramp_delay
                    && let Some((v, inv)) = self.deres
                {
                    self.deres = Some(((v + dt / secs.max(1e-3)).clamp(0.0, 1.0), inv));
                }
            }
            Do::Gravity(f) => beat.gravity = Some(*f),
            Do::Spin { every, .. } => {
                if *every {
                    beat.spin += r.spin;
                }
            }
            Do::Collide { after, .. } => {
                r.time += dt;
                if env.collided && !r.collided {
                    r.collided = true;
                    r.duration = rand_in(&mut self.seed, *after);
                    r.time = 0.0;
                }
            }
            _ => {}
        }
    }

    /// Its class's IsDone (with TnDeathAction's: neither timer running).
    fn is_done(&self, act: &Act, r: &Running) -> bool {
        let base = r.delay.is_none() && r.hold.is_none_or(|h| h <= 0.0);
        match &act.what {
            // (TnDeathActionHighlight.get_IsDone: true while its delay runs.)
            Do::Deres { cutoff, .. } => {
                if !base {
                    return true;
                }
                (*cutoff > 0.0 && r.time >= *cutoff) || self.deres.is_some_and(|(v, _)| v >= 1.0)
            }
            _ if !base => false,
            Do::Anim { cutoff, .. } => !r.playing || (*cutoff > 0.0 && r.time > *cutoff),
            Do::Ragdoll(rd) => r.ragdoll_at.is_some_and(|at| self.t - at >= rd.settle),
            Do::Wait(..) => r.time <= 0.0,
            Do::Light(_) => self.light.is_none(),
            Do::Collide { .. } => r.time >= r.duration,
            _ => true,
        }
    }

    /// `dt` seconds on (TnDeathPlayer.Tick: TnDeathTimeline.Tick, the light's
    /// UpdateLight).
    pub fn step(&mut self, dt: f32, env: &Env) -> Beat {
        let mut beat = Beat::default();
        if !self.started {
            // (TnDeathPlayer.Activate: its first keyframe started as it dies.)
            self.started = true;
            self.start_key(&mut beat, env);
        }
        self.t += dt;
        if !self.done() {
            let def = self.def.clone();
            let acts = &def.keys[self.key];
            let mut running = std::mem::take(&mut self.running);
            let mut all_done = true;
            for (act, r) in acts.iter().zip(running.iter_mut()) {
                // TnDeathAction.Tick: its delay, its wait after, else its own tick.
                if let Some(left) = r.delay {
                    let left = left - dt;
                    if left <= 0.0 {
                        r.delay = None;
                        self.on_start(act, r, &mut beat, env);
                    } else {
                        r.delay = Some(left);
                    }
                } else if let Some(h) = r.hold
                    && h > 0.0
                {
                    r.hold = Some(h - dt);
                } else {
                    self.on_tick(act, r, dt, &mut beat, env);
                }
                if act.hold > 0.0 && r.hold.is_none() && self.is_done(act, r) {
                    r.hold = Some(act.hold);
                }
                all_done &= self.is_done(act, r);
            }
            self.running = running;
            if all_done {
                self.key += 1;
                self.start_key(&mut beat, env);
            }
        }
        // Its light (TnRampLightManager.UpdateLight).
        if let Some((l, t)) = self.light {
            let t = t + dt;
            match l.brightness_at(t) {
                Some(b) => {
                    self.light = Some((l, t));
                    beat.light = Some((l, b));
                }
                None => self.light = None,
            }
        }
        beat.deres = self.deres;
        if self.done() {
            beat.over = true;
            self.over_for = Some(self.over_for.map_or(0.0, |t| t + dt));
        }
        beat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Option<DeathData> {
        let d = DeathData::load(&crate::assets_dir());
        (!d.sets.is_empty()).then_some(d)
    }

    fn robot_kill(damage: &[&str]) -> Kill {
        Kill {
            damage: Types::of_all(damage.iter().copied()),
            side: Types::of("HitFromFront"),
            robot: true,
            ..Default::default()
        }
    }

    /// Runs a death at 30 frames a second for `secs`, its clips `clip` long (each
    /// of them), with a collision from `collide_at` on; the beats with their times.
    fn run(def: Arc<DeathDef>, secs: f32, clip: f32, collide_at: f32) -> Vec<(f32, Beat)> {
        let mut tl = Timeline::new(def, 7);
        let lookup = move |_: &str, rate: f32| Some(clip / rate.max(0.05));
        let mut out = Vec::new();
        let dt = 1.0 / 30.0;
        for i in 0..(secs * 30.0) as usize {
            let t = (i + 1) as f32 * dt;
            let env = Env {
                clip: &lookup,
                collided: t >= collide_at,
            };
            let beat = tl.step(dt, &env);
            out.push((t, beat));
            if tl.gone() {
                break;
            }
        }
        out
    }

    fn first(beats: &[(f32, Beat)], f: impl Fn(&Beat) -> bool) -> Option<f32> {
        beats.iter().find(|(_, b)| f(b)).map(|(t, _)| *t)
    }

    #[test]
    fn reads_the_death_sets() {
        let Some(d) = data() else { return };
        for set in [
            "Basic_DEATHSET",
            "Titan_DEATHSET",
            "TitanExplosiveDrone_DEATHSET",
            "Flyer_DEATHSET",
            "Sniper_DEATHSET",
            "Guardian_DEATHSET",
            "Swarmer_DEATHSET",
            "Leaper_DEATHSET",
        ] {
            assert!(d.sets.get(set).is_some_and(|s| !s.is_empty()), "{set}");
        }
        assert!(d.deaths.len() >= 100, "{} deaths", d.deaths.len());
        // Every entry's deaths are there, each with its keyframes.
        for (set, entries) in &d.sets {
            for e in entries {
                for (_, m) in &e.members {
                    let def = d.deaths.get(m).unwrap_or_else(|| panic!("{set}: {m}"));
                    assert!(!def.keys.is_empty(), "{m}");
                }
            }
        }
        // The destructibles the deaths spawn.
        let gibs = &d.destructibles["DefaultGibs_DSYS"];
        assert!(
            gibs.pieces
                .iter()
                .any(|p| matches!(p, Piece::Gibs(g) if g.count == 4 && g.meshes.len() == 11))
        );
        let titan = &d.destructibles["TitanExplosion_DSYS"];
        assert!(
            titan
                .pieces
                .iter()
                .any(|p| matches!(p, Piece::Damage { radius, .. } if *radius == 3000.0))
        );
    }

    /// TnDeathPlayer.Pick: a gunfire kill of a soldier from the front plays
    /// Gunfire_Front (the sets' gunfire deaths from the back and the sides come after
    /// it, so it is the one), a headshot one of the headshots, a shotgun's within
    /// its reach a gib, an explosion one of Explosive_CONDEATHSET's (its gibs' set,
    /// after it, never), a tank's shell ExplosiveTank's.
    #[test]
    fn picks_the_deaths_by_their_types() {
        let Some(d) = data() else { return };
        let pick = |kill: Kill, roll: f32| {
            d.pick("Basic_DEATHSET", kill.types(), roll)
                .map(|x| short(&x.name).to_string())
                .unwrap_or_default()
        };
        assert_eq!(
            pick(robot_kill(&["Ballistic", "RapidShot"]), 0.5),
            "Gunfire_Front_DEATH"
        );
        let mut back = robot_kill(&["Ballistic", "RapidShot"]);
        back.side = Types::of("HitFromBack");
        assert_eq!(pick(back, 0.5), "Gunfire_Front_DEATH");
        let mut head = robot_kill(&["Ballistic", "SingleShot"]);
        head.head = true;
        assert!(
            pick(head, 0.2).starts_with("Headshot0"),
            "{}",
            pick(head, 0.2)
        );
        let mut gib = robot_kill(&["Ballistic", "SingleShot"]);
        gib.gib = true;
        assert!(pick(gib, 0.5).starts_with("ShotgunGib"));
        assert!(pick(robot_kill(&["Ballistic", "SingleShot"]), 0.5).starts_with("Shotgun"));
        assert!(pick(robot_kill(&["Explosive"]), 0.5).starts_with("Explosive0"));
        let mut grenade = robot_kill(&["Explosive"]);
        grenade.gib = true;
        assert!(pick(grenade, 0.5).starts_with("Explosive0"));
        assert!(pick(robot_kill(&["Explosive", "Tank"]), 0.5).starts_with("ExplosiveTank"));
        assert_eq!(pick(robot_kill(&["Explosive", "Slow"]), 0.5), "Slow_DEATH");
        assert!(pick(robot_kill(&["Energy", "RapidShot"]), 0.5).starts_with("Electrocute0"));
        assert_eq!(pick(robot_kill(&["ShockwaveGun"]), 0.5), "ShockBlast_DEATH");
        assert_eq!(pick(robot_kill(&["Dismember"]), 0.5), "Dismember_DEATH");
        assert_eq!(pick(robot_kill(&["Fire"]), 0.5), "Fire_DEATH");
        assert_eq!(pick(robot_kill(&[]), 0.5), "Ragdoll_DEATH");
        let mut air = robot_kill(&["Ballistic", "RapidShot"]);
        air.airborne = true;
        assert_eq!(pick(air, 0.5), "Airborne_DEATH");
        // A conditional set's weights: Artillery's vaporize ten to one.
        let w: Vec<String> = (0..11)
            .map(|i| pick(robot_kill(&["Artillery"]), (i as f32 + 0.5) / 11.0))
            .collect();
        assert_eq!(
            w.iter()
                .filter(|n| n.as_str() == "ArtilleryGib_DEATH")
                .count(),
            1,
            "{w:?}"
        );
        // The jets' planes: hovering, boosting, the rest.
        let jet = |boost| Kill {
            damage: Types::of_all(["Ballistic", "RapidShot"]),
            robot: false,
            vehicle_boost: Some(boost),
            factory: Factory::Vehicle,
            ..Default::default()
        };
        let flyer = |k: Kill| {
            d.pick("Flyer_DEATHSET", k.types(), 0.5)
                .map(|x| short(&x.name).to_string())
                .unwrap_or_default()
        };
        assert!(
            flyer(jet(false)).starts_with("FlyerHover"),
            "{}",
            flyer(jet(false))
        );
        assert_eq!(flyer(jet(true)), "FlyerBoost_DEATH");
    }

    /// Gunfire_Front_DEATH: its clip, the ragdoll 0.1 s before it ends (its settle
    /// time 0: done as it takes over), its light; then the dissolve (3 s, the mesh
    /// gone at 40% of it) once the light's 2.25 s are out, the death over as it has
    /// dissolved.
    #[test]
    fn gunfire_falls_from_its_clip_and_dissolves() {
        let Some(d) = data() else { return };
        let def = d.deaths["TR_Deaths_p.Deaths.Gunfire_Front_DEATH"].clone();
        let beats = run(def, 12.0, 1.0, f32::MAX);
        assert!(
            beats[0]
                .1
                .anim
                .as_ref()
                .is_some_and(|a| &*a.0 == "Death_Gunfire_Front")
        );
        let fall = first(&beats, |b| b.ragdoll.is_some()).expect("it never fell");
        assert!((0.85..=0.95).contains(&fall), "it fell at {fall}");
        let deres = first(&beats, |b| b.deres_start.is_some()).expect("it never dissolved");
        // (Keyframe 0 ends with its light, 2.25 s.)
        assert!((2.2..=2.35).contains(&deres), "it dissolved at {deres}");
        let over = first(&beats, |b| b.over).expect("its death never ended");
        assert!(
            (over - deres - 3.0).abs() < 0.1,
            "over at {over}, the dissolve at {deres}"
        );
        assert!(beats.iter().any(|(_, b)| b.light.is_some()));
        // The dissolve's value climbs to 1 over its 3 s.
        let half = beats
            .iter()
            .find(|(t, _)| *t >= deres + 1.5)
            .and_then(|(_, b)| b.deres)
            .unwrap();
        assert!((half.0 - 0.5).abs() < 0.05, "{half:?}");
    }

    /// Electrocute01_DEATH: the electrocution's clip at 0.75 with its effects; the
    /// ragdoll as it ends (keyframe 1), thrown by its blast, the gibs and the shake
    /// with it, a wait of 2 to 4 s; the dissolve.
    #[test]
    fn electrocution_throws_and_gibs() {
        let Some(d) = data() else { return };
        let def = d.deaths["TR_Deaths_p.Deaths.Electrocute01_DEATH"].clone();
        let beats = run(def, 14.0, 2.0, f32::MAX);
        let clip = beats[0].1.anim.as_ref().expect("its clip");
        assert_eq!((&*clip.0, clip.1), ("Death_Electrocute", 0.75));
        // (Its sparks on its head at once, the chest's after their delay, 1.75 s.)
        assert_eq!(beats[0].1.effects.len(), 1, "{:?}", beats[0].1.effects);
        let chest = first(&beats, |b| b.effects.iter().any(|e| &*e.1 == "Chest_FX")).unwrap();
        assert!((chest - 1.75).abs() < 0.05, "the chest's at {chest}");
        let fall = first(&beats, |b| b.ragdoll.is_some()).unwrap();
        let clip_end = 2.0 / 0.75;
        assert!(
            (fall - clip_end).abs() < 0.1,
            "fell at {fall}, the clip ends at {clip_end}"
        );
        let r = beats.iter().find_map(|(_, b)| b.ragdoll.clone()).unwrap();
        assert!(r.radials().next().is_some(), "no blast: {r:?}");
        // (Its bodies stopped first, then thrown: the actions' order.)
        assert!(
            matches!(r.actions.first(), Some(RagdollAct::Halt)),
            "{:?}",
            r.actions
        );
        let gibs = first(&beats, |b| !b.destructibles.is_empty()).unwrap();
        assert!((gibs - fall).abs() < 0.05);
        assert!(first(&beats, |b| !b.shakes.is_empty()).is_some());
        let deres = first(&beats, |b| b.deres_start.is_some()).unwrap();
        assert!(
            (fall + 1.9..=fall + 4.1).contains(&deres),
            "dissolved at {deres}, fell at {fall}"
        );
    }

    /// A melee kill (Melee_CONDEATHSET: MeleeRagdoll_DEATH once in three,
    /// MeleeDismember01_DEATH twice) kicks its ragdoll as the game's actions say:
    /// the thrown one's first impulse on the bone struck (none for a blow: the root
    /// body), its second on the lower back, both at where it was struck; the
    /// dismembered one's blast from 3 m under where it was struck and its impulse
    /// on the lower back there; a blast at a bone (Electrocute01_DEATH's, from the
    /// lower back) from that bone.
    #[test]
    fn a_melee_kill_kicks_where_it_was_struck() {
        use crate::ragdoll::KickAction;
        let Some(d) = data() else { return };
        let ragdoll = |name: &str| {
            let def = d.deaths[name].clone();
            run(def, 6.0, 2.0, f32::MAX)
                .iter()
                .find_map(|(_, b)| b.ragdoll.clone())
                .unwrap()
        };
        let place = Place {
            at: Vec3::new(0.0, 2.2, 0.0),
            turn: Quat::IDENTITY,
            hit: Vec3::new(0.0, 3.5, -0.8),
            momentum: Vec3::new(0.0, 1.0, -2.0),
            velocity: Vec3::ZERO,
        };
        let mut seed = 7;
        let thrown = kick(
            &ragdoll("TR_Deaths_p.Deaths.MeleeRagdoll_DEATH"),
            &place,
            None,
            Vec3::ZERO,
            &mut seed,
        );
        let impulses: Vec<_> = thrown
            .actions
            .iter()
            .filter_map(|a| match a {
                KickAction::Impulse { bone, at, mass, .. } => Some((bone.clone(), *at, *mass)),
                _ => None,
            })
            .collect();
        assert_eq!(
            impulses,
            vec![
                (None, Some(place.hit), true),
                (
                    Some("C_Spine01_Lumbar01_XB".to_string()),
                    Some(place.hit),
                    true
                ),
            ]
        );
        // (Struck by a round on its chest, the first is on the chest.)
        let shot = kick(
            &ragdoll("TR_Deaths_p.Deaths.MeleeRagdoll_DEATH"),
            &place,
            Some("C_Spine03_Thoracic01_XB"),
            Vec3::ZERO,
            &mut seed,
        );
        assert!(matches!(
            shot.actions.first(),
            Some(KickAction::Impulse { bone: Some(b), .. }) if b == "C_Spine03_Thoracic01_XB"
        ));
        let cut = kick(
            &ragdoll("TR_Deaths_p.Deaths.MeleeDismember01_DEATH"),
            &place,
            None,
            Vec3::ZERO,
            &mut seed,
        );
        let blast = cut
            .actions
            .iter()
            .find_map(|a| match a {
                KickAction::Radial { origin, bone, .. } => Some((*origin, bone.clone())),
                _ => None,
            })
            .expect("no blast");
        assert!(blast.1.is_none());
        assert!(
            (blast.0 - (place.hit - Vec3::Y * 3.0)).length() < 1e-4,
            "{blast:?}"
        );
        assert!(cut.actions.iter().any(|a| matches!(
            a,
            KickAction::Impulse { bone: Some(b), at: Some(at), .. }
                if b == "C_Spine01_Lumbar01_XB" && *at == place.hit
        )));
        let zap = kick(
            &ragdoll("TR_Deaths_p.Deaths.Electrocute01_DEATH"),
            &place,
            None,
            Vec3::ZERO,
            &mut seed,
        );
        assert!(zap.actions.iter().any(|a| matches!(
            a,
            KickAction::Radial { bone: Some(b), .. } if b == "C_Spine01_Lumbar01_XB"
        )));
    }

    /// The Titan's Generic_DEATH: its death clip, the ragdoll 0.7 s before it ends;
    /// its explosion (TitanExplosion_DSYS) at 5.5 s, keyframe 0 lasting as long.
    #[test]
    fn the_titan_blows_up_at_five_and_a_half() {
        let Some(d) = data() else { return };
        let def = d.deaths["TR_Titan_AI_p.Deaths.Generic_DEATH"].clone();
        let beats = run(def, 16.0, 6.2, f32::MAX);
        let blast = beats
            .iter()
            .find(|(_, b)| {
                b.destructibles
                    .iter()
                    .any(|x| &**x == "TitanExplosion_DSYS")
            })
            .map(|(t, _)| *t)
            .expect("no blast");
        assert!((5.45..=5.6).contains(&blast), "blew up at {blast}");
        let fall = first(&beats, |b| b.ragdoll.is_some()).unwrap();
        assert!((fall - (6.2 - 0.7)).abs() < 0.1, "fell at {fall}");
    }

    /// Ragdoll_DEATH (what kills nothing else takes): the ragdoll at once, the
    /// dissolve once it has settled.
    #[test]
    fn a_plain_death_falls_at_once() {
        let Some(d) = data() else { return };
        let def = d.deaths["TR_Deaths_p.Deaths.Ragdoll_DEATH"].clone();
        let beats = run(def, 16.0, 0.0, f32::MAX);
        let fall = first(&beats, |b| b.ragdoll.is_some()).unwrap();
        assert!(fall < 0.1, "fell at {fall}");
        assert!(first(&beats, |b| b.deres_start.is_some()).is_some());
        assert!(beats.last().is_some_and(|(_, b)| b.over));
    }

    /// FlyerHover_DEATH: a jet shot down hovering falls at once, spinning, keyframe 0
    /// lasting until its collision timer is done (3 to 5 s, or soon after it hits
    /// something); then its gibs, and the jet is gone (it has no dissolve).
    #[test]
    fn a_jet_spins_down_and_bursts() {
        let Some(d) = data() else { return };
        let name = d
            .deaths
            .keys()
            .find(|k| k.ends_with("FlyerHover_DEATH"))
            .unwrap()
            .clone();
        let def = d.deaths[&name].clone();
        let beats = run(def.clone(), 10.0, 0.0, 1.5);
        assert!(
            beats[0].1.ragdoll.is_some() || first(&beats, |b| b.ragdoll.is_some()).unwrap() < 0.1
        );
        assert!(beats.iter().take(10).any(|(_, b)| b.spin != Vec3::ZERO));
        let burst = beats
            .iter()
            .filter(|(_, b)| !b.destructibles.is_empty())
            .map(|(t, _)| *t)
            .collect::<Vec<_>>();
        assert!(burst.len() >= 2, "{burst:?}");
        // (It hit at 1.5 s: its timer done soon after, not 3-5 s in.)
        assert!(burst[1] < 2.5, "burst at {burst:?}");
        assert!(first(&beats, |b| b.over).is_some_and(|t| t < 3.0));
        // Not hitting anything: 3 to 5 s.
        let beats = run(def, 10.0, 0.0, f32::MAX);
        let t = beats
            .iter()
            .filter(|(_, b)| !b.destructibles.is_empty())
            .map(|(t, _)| *t)
            .nth(1)
            .unwrap();
        assert!((3.2..=5.4).contains(&t), "burst at {t}");
    }

    /// The light's ramp (TnRampLightManager.UpdateLight).
    #[test]
    fn a_death_light_ramps_in_and_out() {
        let l = LightDo {
            brightness: 10.0,
            radius: 500.0,
            falloff: 4.0,
            color: [0, 54, 255],
            ramp_in: 0.5,
            ramp_out: 0.5,
            secs: 2.0,
            sync: true,
            at: Vec3::ZERO,
        };
        assert_eq!(l.brightness_at(0.25), Some(5.0));
        assert_eq!(l.brightness_at(1.0), Some(10.0));
        assert!((l.brightness_at(1.75).unwrap() - 5.0).abs() < 1e-4);
        assert_eq!(l.brightness_at(2.0), None);
    }

    /// GetDirectionDeathType: the side a blow came from in the pawn's frame.
    #[test]
    fn tells_the_side_a_blow_came_from() {
        // Facing -Z: a blow from ahead is its front, from +X its right.
        let f = Vec3::NEG_Z;
        assert_eq!(
            side_struck(f, Vec3::new(0.0, 0.0, -1.0)),
            Types::of("HitFromFront")
        );
        assert_eq!(
            side_struck(f, Vec3::new(1.0, 0.0, 0.0)),
            Types::of("HitFromRight")
        );
        assert_eq!(
            side_struck(f, Vec3::new(-1.0, 0.0, 0.0)),
            Types::of("HitFromLeft")
        );
        assert_eq!(
            side_struck(f, Vec3::new(0.0, 0.0, 1.0)),
            Types::of("HitFromBack")
        );
        // Facing +X: from +X its front, from +Z its right.
        let f = Vec3::X;
        assert_eq!(
            side_struck(f, Vec3::new(1.0, 0.0, 0.0)),
            Types::of("HitFromFront")
        );
        assert_eq!(
            side_struck(f, Vec3::new(0.0, 0.5, 1.0)),
            Types::of("HitFromRight")
        );
    }
}
