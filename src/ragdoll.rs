//! Ragdolls: a dead robot falling as the game's physics asset says, its bodies held
//! together at their joints, onto the level.
//!
//! A physics asset (UE3's PhysicsAsset) is a set of bodies, each on a bone
//! (RB_BodySetup: its spheres, capsules, boxes and convex hulls, in the bone's own
//! space), and the joints between them (RB_ConstraintSetup: a child's bone held to
//! its parent's at the child's bone, within its swing and twist limits). The same
//! bodies are what shots strike while the robot lives (enemies.rs tests them: see
//! [`Shape`]), and what it falls as when it dies: the death sets' Ragdoll action
//! (TnDeathActionRagdoll) hands the skeleton over from its clips to the physics,
//! with the velocities the clip left it moving at (UseVelocityTransferTime), the
//! kill's momentum (its TearOffMomentum) and its ragdoll actions in their order
//! (radial impulses, impulses, halting its bodies, cutting pieces away: [`Kick`]).
//!
//! The physics here are a small rigid-body solver of their own (positions solved in
//! substeps, as extended position-based dynamics do: each joint pulls its two
//! bodies' anchors together and keeps the child's turn from its parent within the
//! limits; each body's contact spheres push it out of the level's triangles, with
//! friction). A body's mass is its physics asset's (High Moon's RB_BodySetup.Mass,
//! PhysX's units), else its shapes' volume at PhysX's density of 1 (UE3's physics
//! units are 50 uu: 8 to a cubic metre); its inertia is taken as the same about
//! every axis. The bodies don't collide with each other.
//!
//! [`Rig`] is a skeleton's part of it in the world: its bones, and whether it is
//! animated, falling or come to rest; [`drive_rigs`] runs the falling ones and poses
//! their bones, after the clips have (so whatever the clips do, a ragdoll's bones
//! are where its bodies are).

use std::sync::Arc;

use bevy::math::Affine3A;
use bevy::prelude::*;

/// A collision shape: in a bone's space as the physics asset has it (glTF axes,
/// metres at the skeleton's own size), or placed in the world.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Sphere {
        c: Vec3,
        r: f32,
    },
    /// The segment between two end centres, and its radius.
    Capsule {
        a: Vec3,
        b: Vec3,
        r: f32,
    },
    /// Its centre, its axes (unit length, at right angles) and its half sizes
    /// along them.
    Box {
        c: Vec3,
        axes: [Vec3; 3],
        half: Vec3,
    },
    /// Its corners and its faces' planes (outward normal n, distance d: n . p = d on
    /// the face).
    Convex {
        points: Vec<Vec3>,
        planes: Vec<(Vec3, f32)>,
    },
}

impl Shape {
    /// The shape placed by `m` (a bone's world transform: turned, moved and scaled
    /// the same along every axis).
    pub fn placed(&self, m: &Affine3A) -> Shape {
        let s = m.matrix3.x_axis.length();
        match self {
            Shape::Sphere { c, r } => Shape::Sphere {
                c: m.transform_point3(*c),
                r: r * s,
            },
            Shape::Capsule { a, b, r } => Shape::Capsule {
                a: m.transform_point3(*a),
                b: m.transform_point3(*b),
                r: r * s,
            },
            Shape::Box { c, axes, half } => Shape::Box {
                c: m.transform_point3(*c),
                axes: axes.map(|a| m.transform_vector3(a).normalize_or(a)),
                half: *half * s,
            },
            Shape::Convex { points, planes } => Shape::Convex {
                points: points.iter().map(|p| m.transform_point3(*p)).collect(),
                planes: planes
                    .iter()
                    .map(|(n, d)| {
                        let wn = m.transform_vector3(*n).normalize_or(*n);
                        (wn, wn.dot(m.transform_point3(*n * *d)))
                    })
                    .collect(),
            },
        }
    }

    /// The same shape grown by `by` all round (a round with a size of its own
    /// tested as a point).
    pub fn grown(&self, by: f32) -> Shape {
        match self {
            Shape::Sphere { c, r } => Shape::Sphere { c: *c, r: r + by },
            Shape::Capsule { a, b, r } => Shape::Capsule {
                a: *a,
                b: *b,
                r: r + by,
            },
            Shape::Box { c, axes, half } => Shape::Box {
                c: *c,
                axes: *axes,
                half: *half + Vec3::splat(by),
            },
            Shape::Convex { points, planes } => Shape::Convex {
                points: points.clone(),
                planes: planes.iter().map(|(n, d)| (*n, d + by)).collect(),
            },
        }
    }

    /// Distance along a ray (unit `dir`) to where it enters the shape, within `max`;
    /// 0 if it starts inside.
    pub fn ray(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        let t = match self {
            Shape::Sphere { c, r } => ray_sphere(origin, dir, *c, *r),
            Shape::Capsule { a, b, r } => ray_capsule(origin, dir, *a, *b, *r),
            Shape::Box { c, axes, half } => {
                let o = origin - *c;
                let lo = Vec3::new(o.dot(axes[0]), o.dot(axes[1]), o.dot(axes[2]));
                let ld = Vec3::new(dir.dot(axes[0]), dir.dot(axes[1]), dir.dot(axes[2]));
                ray_box(lo, ld, *half)
            }
            Shape::Convex { planes, .. } => ray_convex(origin, dir, planes),
        }?;
        (t <= max).then_some(t)
    }

    /// How far `p` is from the shape (0 inside it).
    pub fn reach(&self, p: Vec3) -> f32 {
        match self {
            Shape::Sphere { c, r } => (p.distance(*c) - r).max(0.0),
            Shape::Capsule { a, b, r } => (p.distance(closest_on_segment(p, *a, *b)) - r).max(0.0),
            Shape::Box { c, axes, half } => {
                let o = p - *c;
                let l = Vec3::new(o.dot(axes[0]), o.dot(axes[1]), o.dot(axes[2]));
                (l.abs() - *half).max(Vec3::ZERO).length()
            }
            Shape::Convex { planes, .. } => planes
                .iter()
                .map(|(n, d)| n.dot(p) - d)
                .fold(0.0f32, f32::max),
        }
    }

    /// Its middle.
    pub fn centre(&self) -> Vec3 {
        match self {
            Shape::Sphere { c, .. } | Shape::Box { c, .. } => *c,
            Shape::Capsule { a, b, .. } => (*a + *b) * 0.5,
            Shape::Convex { points, .. } => {
                points.iter().copied().sum::<Vec3>() / points.len().max(1) as f32
            }
        }
    }

    /// A sphere round it: its centre and radius.
    pub fn bounds(&self) -> (Vec3, f32) {
        let c = self.centre();
        let r = match self {
            Shape::Sphere { r, .. } => *r,
            Shape::Capsule { a, b, r } => a.distance(*b) * 0.5 + r,
            Shape::Box { half, .. } => half.length(),
            Shape::Convex { points, .. } => {
                points.iter().map(|p| p.distance(c)).fold(0.0, f32::max)
            }
        };
        (c, r)
    }

    /// Its volume (a convex hull's, as its box's), and its radius of gyration
    /// squared, the mean of its three axes' (what its inertia about its middle is
    /// over its mass).
    fn volume_and_gyration(&self) -> (f32, f32) {
        use std::f32::consts::PI;
        match self {
            Shape::Sphere { r, .. } => (4.0 / 3.0 * PI * r * r * r, 0.4 * r * r),
            Shape::Capsule { a, b, r } => {
                let l = a.distance(*b);
                let v = PI * r * r * l + 4.0 / 3.0 * PI * r * r * r;
                // (As a cylinder of its whole length: across, and along.)
                let length = l + 2.0 * r;
                let across = length * length / 12.0 + r * r / 4.0;
                (v, (2.0 * across + r * r / 2.0) / 3.0)
            }
            Shape::Box { half, .. } => {
                let v = 8.0 * half.x * half.y * half.z;
                (v, 2.0 * half.length_squared() / 9.0)
            }
            Shape::Convex { points, .. } => {
                let (lo, hi) = points.iter().fold(
                    (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                    |(lo, hi), p| (lo.min(*p), hi.max(*p)),
                );
                let half = ((hi - lo) * 0.5).max(Vec3::splat(1e-3));
                // (A hull fills about half its box.)
                (
                    4.0 * half.x * half.y * half.z,
                    2.0 * half.length_squared() / 9.0,
                )
            }
        }
    }

    /// The spheres it touches the world with (centre, radius: its own space): a
    /// sphere itself, a capsule's spheres along its segment, a box's in its corners
    /// (as big as its thinnest side allows), a hull's on its corners.
    fn probes(&self, out: &mut Vec<(Vec3, f32)>) {
        match self {
            Shape::Sphere { c, r } => out.push((*c, *r)),
            Shape::Capsule { a, b, r } => {
                let n = ((a.distance(*b) / r.max(1e-3)).ceil() as usize).clamp(1, 6);
                for i in 0..=n {
                    out.push((a.lerp(*b, i as f32 / n as f32), *r));
                }
            }
            Shape::Box { c, axes, half } => {
                let r = half.min_element().max(1e-3);
                let reach = (*half - Vec3::splat(r)).max(Vec3::ZERO);
                for i in 0..8 {
                    let s = Vec3::new(
                        if i & 1 == 0 { -1.0 } else { 1.0 },
                        if i & 2 == 0 { -1.0 } else { 1.0 },
                        if i & 4 == 0 { -1.0 } else { 1.0 },
                    );
                    let k = s * reach;
                    let p = *c + axes[0] * k.x + axes[1] * k.y + axes[2] * k.z;
                    if !out.iter().any(|(q, _)| q.distance_squared(p) < 1e-8) {
                        out.push((p, r));
                    }
                }
            }
            Shape::Convex { points, .. } => {
                let (_, r) = self.bounds();
                let small = (r * 0.04).max(0.02);
                out.extend(points.iter().map(|p| (*p, small)));
            }
        }
    }
}

/// Distance along a ray (unit `dir`) to where it enters a sphere; 0 if it starts inside.
pub fn ray_sphere(origin: Vec3, dir: Vec3, centre: Vec3, radius: f32) -> Option<f32> {
    let to = centre - origin;
    let along = to.dot(dir);
    let miss2 = to.length_squared() - along * along;
    let r2 = radius * radius;
    if miss2 > r2 {
        return None;
    }
    let half = (r2 - miss2).sqrt();
    if along + half < 0.0 {
        return None;
    }
    Some((along - half).max(0.0))
}

fn ray_capsule(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, r: f32) -> Option<f32> {
    let axis = b - a;
    let len = axis.length();
    if len < 1e-6 {
        return ray_sphere(origin, dir, a, r);
    }
    let u = axis / len;
    let mut best: Option<f32> = None;
    let mut keep = |t: f32| {
        if best.is_none_or(|b| t < b) {
            best = Some(t);
        }
    };
    // Inside already?
    if origin.distance(closest_on_segment(origin, a, b)) <= r {
        return Some(0.0);
    }
    // The side: the ray against the infinite cylinder, kept between the ends.
    let o = origin - a;
    let d_perp = dir - u * dir.dot(u);
    let o_perp = o - u * o.dot(u);
    let qa = d_perp.length_squared();
    if qa > 1e-10 {
        let qb = 2.0 * d_perp.dot(o_perp);
        let qc = o_perp.length_squared() - r * r;
        let disc = qb * qb - 4.0 * qa * qc;
        if disc >= 0.0 {
            let t = (-qb - disc.sqrt()) / (2.0 * qa);
            if t >= 0.0 {
                let along = (o + dir * t).dot(u);
                if (0.0..=len).contains(&along) {
                    keep(t);
                }
            }
        }
    }
    // The round ends.
    for c in [a, b] {
        if let Some(t) = ray_sphere(origin, dir, c, r) {
            keep(t);
        }
    }
    best
}

/// A ray in a box's own space (its centre at the origin) against the box.
fn ray_box(o: Vec3, d: Vec3, half: Vec3) -> Option<f32> {
    let mut t0 = 0.0f32;
    let mut t1 = f32::MAX;
    for i in 0..3 {
        if d[i].abs() < 1e-9 {
            if o[i].abs() > half[i] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / d[i];
        let (mut a, mut b) = ((-half[i] - o[i]) * inv, (half[i] - o[i]) * inv);
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

fn ray_convex(origin: Vec3, dir: Vec3, planes: &[(Vec3, f32)]) -> Option<f32> {
    let mut t0 = 0.0f32;
    let mut t1 = f32::MAX;
    for (n, d) in planes {
        let denom = n.dot(dir);
        let dist = d - n.dot(origin);
        if denom.abs() < 1e-9 {
            if dist < 0.0 {
                return None;
            }
            continue;
        }
        let t = dist / denom;
        if denom < 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

/// The point on the segment from `a` to `b` nearest `p`.
pub fn closest_on_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let ab = b - a;
    let l2 = ab.length_squared();
    if l2 < 1e-12 {
        return a;
    }
    a + ab * ((p - a).dot(ab) / l2).clamp(0.0, 1.0)
}

/// The point on a triangle nearest `p`.
fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    // (Ericson, Real-Time Collision Detection, 5.1.5.)
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

// ---------------------------------------------------------------------------
// The physics asset
// ---------------------------------------------------------------------------

/// A body: its bone and its shapes, in the bone's space, and its mass where its
/// physics asset sets it (PhysX's units).
#[derive(Clone, Debug)]
pub struct BodyDef {
    pub bone: String,
    pub shapes: Vec<Shape>,
    pub mass: Option<f32>,
}

/// A joint: the child's bone held to its parent's at the child's bone, the child's
/// twist and secondary axes (in its bone's space) and the parent's (in its), which
/// line up in the skeleton's reference pose, and its limits (radians, None where
/// free): Swing1 turns the twist axis towards the secondary one, Swing2 towards the
/// third, Twist about the twist axis itself.
#[derive(Clone, Debug)]
pub struct JointDef {
    pub child: String,
    pub parent: String,
    pub child_axes: [Vec3; 2],
    pub parent_axes: [Vec3; 2],
    pub swing1: Option<f32>,
    pub swing2: Option<f32>,
    pub twist: Option<f32>,
}

/// A physics asset: its bodies and joints.
#[derive(Clone, Debug, Default)]
pub struct PhysicsAsset {
    pub bodies: Vec<BodyDef>,
    pub joints: Vec<JointDef>,
}

fn floats(field: &str) -> Vec<f32> {
    field
        .split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect()
}

fn vec3(field: &str) -> Option<Vec3> {
    let v = floats(field);
    (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
}

impl PhysicsAsset {
    /// A body line's fields from its bone on (tools/build_enemies.py's `body`: the
    /// bone, the shape's kind and its numbers); the shape is added to the bone's
    /// body. False if they don't read.
    pub fn read_body(&mut self, f: &[&str]) -> bool {
        let Some(shape) = Self::shape(f.get(1..).unwrap_or(&[])) else {
            return false;
        };
        let bone = f[0];
        match self.bodies.iter_mut().find(|b| b.bone == bone) {
            Some(b) => b.shapes.push(shape),
            None => self.bodies.push(BodyDef {
                bone: bone.to_string(),
                shapes: vec![shape],
                mass: None,
            }),
        }
        true
    }

    /// A mass line's fields from its bone on (build_enemies.py's `mass`: the bone and
    /// its body's mass), on the body already read. False if they don't read.
    pub fn read_mass(&mut self, f: &[&str]) -> bool {
        let (Some(bone), Some(Ok(mass))) = (f.first(), f.get(1).map(|m| m.trim().parse::<f32>()))
        else {
            return false;
        };
        match self.bodies.iter_mut().find(|b| b.bone == *bone) {
            Some(b) if mass > 0.0 => {
                b.mass = Some(mass);
                true
            }
            _ => false,
        }
    }

    fn shape(f: &[&str]) -> Option<Shape> {
        let n = |i: usize| f.get(i).and_then(|s| s.trim().parse::<f32>().ok());
        let v = |i: usize| f.get(i).and_then(|s| vec3(s));
        match *f.first()? {
            "sphere" => Some(Shape::Sphere { c: v(1)?, r: n(2)? }),
            "capsule" => Some(Shape::Capsule {
                a: v(1)?,
                b: v(2)?,
                r: n(3)?,
            }),
            "box" => {
                let axes = [v(2)?, v(3)?, v(4)?].map(|a| a.normalize_or(Vec3::X));
                Some(Shape::Box {
                    c: v(1)?,
                    axes,
                    half: v(5)?,
                })
            }
            "convex" => {
                let np = n(1)? as usize;
                let points = (0..np).map(|i| v(2 + i)).collect::<Option<Vec<_>>>()?;
                let nq = n(2 + np)? as usize;
                let planes = (0..nq)
                    .map(|i| {
                        let p = floats(f.get(3 + np + i)?);
                        (p.len() == 4).then(|| (Vec3::new(p[0], p[1], p[2]), p[3]))
                    })
                    .collect::<Option<Vec<_>>>()?;
                (!points.is_empty() && !planes.is_empty())
                    .then_some(Shape::Convex { points, planes })
            }
            _ => None,
        }
    }

    /// A joint line's fields from its child bone on (build_enemies.py's `joint`: the
    /// child and parent bones, the frames' places and axes, child's then parent's,
    /// and the limits in degrees, "-" for free).
    pub fn read_joint(&mut self, f: &[&str]) -> bool {
        if f.len() < 11 {
            return false;
        }
        let v = |i: usize| vec3(f[i]);
        let limit =
            |i: usize| -> Option<f32> { f[i].trim().parse::<f32>().ok().map(f32::to_radians) };
        let (Some(pri1), Some(sec1), Some(pri2), Some(sec2)) = (v(3), v(4), v(6), v(7)) else {
            return false;
        };
        self.joints.push(JointDef {
            child: f[0].to_string(),
            parent: f[1].to_string(),
            child_axes: [pri1.normalize_or(Vec3::X), sec1.normalize_or(Vec3::Y)],
            parent_axes: [pri2.normalize_or(Vec3::X), sec2.normalize_or(Vec3::Y)],
            swing1: limit(8),
            swing2: limit(9),
            twist: limit(10),
        });
        true
    }
}

// ---------------------------------------------------------------------------
// The physics
// ---------------------------------------------------------------------------

/// The ragdolls' gravity (metres a second squared: the game's world's, -2940 units).
pub const GRAVITY: f32 = -29.4;
/// A body's mass where its physics asset leaves it to its shapes: their volume in
/// PhysX's units (UE3's U2PScale, 0.02: half a metre to the unit) at PhysX's density
/// of 1.
const PHYSX_UNITS_PER_CUBIC_METRE: f32 = 8.0;
/// PhysX's units to the metre (the angular impulses' lever).
const PHYSX_UNITS_PER_METRE: f32 = 2.0;
/// The fastest a body spins from a death's impulses (PhysX's default
/// maxAngularVelocity, radians a second), and the most a death's impulses leave
/// it moving at (metres a second; the joints share it out over the next steps).
const MAX_IMPULSE_SPIN: f32 = 7.0;
const MAX_IMPULSE_SPEED: f32 = 200.0;
/// Substeps a frame is solved in.
const SUBSTEPS: usize = 16;
/// Friction against the level (holding still, and sliding).
const STATIC_FRICTION: f32 = 0.8;
const DYNAMIC_FRICTION: f32 = 0.5;
/// Air drag on the bodies' movement and turning, and the joints' own friction
/// (their turn against their parent's, damped at this rate a second).
const LINEAR_DAMPING: f32 = 0.05;
const ANGULAR_DAMPING: f32 = 0.6;
const JOINT_DAMPING: f32 = 3.0;
/// A body touching the level turns slower at this rate besides (its rolling held
/// back: what lets a body lying on the floor come to rest).
const CONTACT_SPIN_DAMPING: f32 = 4.0;
/// The least a joint's limit is taken as (a limit of 0 locks it: as near as the
/// solver holds it).
const MIN_LIMIT: f32 = 0.5 * std::f32::consts::PI / 180.0;
/// How fast a joint started past its limits is brought back within them (radians
/// a second).
const GRACE_RATE: f32 = 1.5;
/// The fastest the level pushes a body out of it (metres a second at the
/// skeleton's size: PhysX's depenetration speed).
const MAX_PUSH_SPEED: f32 = 2.0;
/// Speeds a body is held under (metres a second; radians a second).
const MAX_SPEED: f32 = 40.0;
const MAX_SPIN: f32 = 25.0;
/// It has come to rest once every body moves slower than this (metres a second at
/// the skeleton's own size) for REST_SECS, or once it has fallen for SETTLE_SECS
/// (the Ragdoll action's MaximumRagdollSettleTime, 10).
const REST_SPEED: f32 = 0.3;
const REST_SECS: f32 = 0.4;
/// It has also come to rest once its middle has stayed within this of where it was
/// (metres at the skeleton's size) for this long, moving slower than SLOW_SPEED.
const DRIFT: f32 = 0.05;
const DRIFT_SECS: f32 = 0.6;
/// Moving slower than this (metres a second at the skeleton's size), it is held
/// back at this rate (a second) as well, so it comes to rest rather than creep.
const SLOW_SPEED: f32 = 1.0;
const SLOW_DAMPING: f32 = 4.0;
pub const SETTLE_SECS: f32 = 10.0;

/// What a ragdoll lands on: the level's triangles about a box, and the arena's
/// floor (its height) when there is no level.
pub trait World {
    fn triangles(&self, min: Vec3, max: Vec3, out: &mut Vec<[Vec3; 3]>);
    fn floor(&self) -> Option<f32>;
}

/// A flat floor at a height, and nothing else (the tests').
#[cfg(test)]
pub struct Floor(pub f32);

#[cfg(test)]
impl World for Floor {
    fn triangles(&self, _: Vec3, _: Vec3, _: &mut Vec<[Vec3; 3]>) {}
    fn floor(&self) -> Option<f32> {
        Some(self.0)
    }
}

#[derive(Clone, Debug)]
struct Body {
    /// Its bone (an index into the skeleton's bones as the ragdoll was made).
    bone: usize,
    /// Its centre of mass and turn (its bone's), and how fast they change.
    x: Vec3,
    q: Quat,
    v: Vec3,
    w: Vec3,
    /// Where they were at the substep's start.
    px: Vec3,
    pq: Quat,
    /// Its turn as the ragdoll took over.
    q0: Quat,
    inv_mass: f32,
    inv_inertia: f32,
    /// Its inertia over its mass (square metres): what an angular impulse on it in
    /// PhysX's units (its 50 uu, half a metre) turns it by.
    k2: f32,
    /// Its centre of mass in its bone's space (at the skeleton's size in the world).
    com: Vec3,
    /// The spheres it touches the world with (centre from its centre of mass in its
    /// bone's space, radius), the body each is of (its own, or one welded to it), and
    /// how far its furthest reaches.
    probes: Vec<(Vec3, f32)>,
    probes_of: Vec<usize>,
    reach: f32,
    /// Held fast to another body (a locked joint's: see Weld).
    weld: Option<Weld>,
}

/// A body held fast to its parent's (a joint with all of its limits locked, the
/// child a piece with nothing hung from it: the Guard's backpack plates): moved as a
/// part of it, its mass and its spheres the parent's while it is, rather than solved
/// as a body of its own (a light piece locked to a heavy one trembles in the solver).
/// Its place and turn in the parent's (from its centre of mass), and what it added
/// to the parent's mass and inertia.
#[derive(Clone, Debug)]
struct Weld {
    to: usize,
    at: Vec3,
    turn: Quat,
    mass: f32,
    inertia: f32,
}

#[derive(Clone, Debug)]
struct Joint {
    /// The parent's body and the child's.
    a: usize,
    b: usize,
    /// The anchor in each one's space (from its centre of mass).
    ra: Vec3,
    rb: Vec3,
    a_axes: [Vec3; 2],
    b_axes: [Vec3; 2],
    swing1: Option<f32>,
    swing2: Option<f32>,
    twist: Option<f32>,
    /// How far past its limits it may still be (radians): what the pose it started
    /// in had, let go of over GRACE_RATE (so a ragdoll that takes over from a clip
    /// past its limits isn't thrown by them).
    grace: f32,
    /// Its child is welded to its parent (Weld): it isn't solved.
    welded: bool,
}

/// A contact found in a substep, for its velocity pass: the body, the contact
/// point from its centre of mass (world), the normal and how hard it was pushed out.
struct Contact {
    body: usize,
    r: Vec3,
    n: Vec3,
    push: f32,
    /// How fast the contact point was coming in (along the normal: under 0) before
    /// the substep was solved.
    approach: f32,
}

/// A skeleton falling as its physics asset's bodies.
#[derive(Clone, Debug)]
pub struct Ragdoll {
    bodies: Vec<Body>,
    joints: Vec<Joint>,
    /// The skeleton's size in the world.
    scale: f32,
    /// How long it has fallen and how long it has been still; where its middle has
    /// stayed near, and for how long.
    age: f32,
    still: f32,
    drift: (Vec3, f32),
    /// Its gravity's factor (the death's SetGravityFactor: Mesh.SetGravityFactor).
    pub gravity: f32,
    /// It touched the level in its last step (what a death's CollisionTimer waits
    /// for: its rigid body collision notify).
    pub touched: bool,
}

/// A bone's place and turn in the world (the skeleton's size applies to it).
pub type BonePose = (Vec3, Quat);

impl Ragdoll {
    /// The ragdoll of a skeleton whose bones (`names`, in its order) are where `pose`
    /// says, `prev` a frame of `dt` seconds before (they go on at the speeds they
    /// had), at `scale`. None if none of its bones has a body.
    pub fn new(
        asset: &PhysicsAsset,
        names: &[String],
        pose: &[BonePose],
        prev: &[BonePose],
        scale: f32,
        dt: f32,
    ) -> Option<Ragdoll> {
        let index = |name: &str| names.iter().position(|n| n == name);
        let mut bodies = Vec::new();
        let mut body_of = vec![None; names.len()];
        for def in &asset.bodies {
            let Some(bone) = index(&def.bone) else {
                continue;
            };
            if body_of[bone].is_some() || def.shapes.is_empty() {
                continue;
            }
            // Its mass and inertia from its shapes (at the world's size).
            let shapes: Vec<Shape> = def
                .shapes
                .iter()
                .map(|s| s.placed(&Affine3A::from_scale(Vec3::splat(scale))))
                .collect();
            let parts: Vec<(f32, f32, Vec3)> = shapes
                .iter()
                .map(|s| {
                    let (v, k2) = s.volume_and_gyration();
                    (v.max(1e-6), k2, s.centre())
                })
                .collect();
            let volume: f32 = parts.iter().map(|p| p.0).sum();
            let com = parts.iter().map(|p| p.2 * p.0).sum::<Vec3>() / volume;
            let k2 = parts
                .iter()
                .map(|(m, k2, c)| m * (k2 + (*c - com).length_squared() * 2.0 / 3.0))
                .sum::<f32>()
                / volume;
            // (Its physics asset's mass, or its volume's at PhysX's density of 1.)
            let mass = def.mass.unwrap_or(volume * PHYSX_UNITS_PER_CUBIC_METRE);
            let inertia = mass * k2;
            let mut probes = Vec::new();
            for s in &shapes {
                s.probes(&mut probes);
            }
            for p in &mut probes {
                p.0 -= com;
            }
            let reach = probes
                .iter()
                .map(|(c, r)| c.length() + r)
                .fold(0.0, f32::max);
            let probes_of = vec![bodies.len(); probes.len()];
            let (p, q) = pose[bone];
            let (pp, pq) = prev.get(bone).copied().unwrap_or((p, q));
            let x = p + q * com;
            let px = pp + pq * com;
            let v = ((x - px) / dt.max(1e-4)).clamp_length_max(MAX_SPEED);
            let w = spin(q, pq, dt).clamp_length_max(MAX_SPIN);
            body_of[bone] = Some(bodies.len());
            bodies.push(Body {
                bone,
                x,
                q,
                v,
                w,
                px: x,
                pq: q,
                q0: q,
                inv_mass: 1.0 / mass,
                inv_inertia: 1.0 / inertia.max(1e-9),
                k2: k2.max(1e-6),
                com,
                probes,
                probes_of,
                reach,
                weld: None,
            });
        }
        if bodies.is_empty() {
            return None;
        }
        // (The masses kept within a hundredfold of each other: a finger beside a
        // chest makes the joints between them stiff to solve.)
        let heaviest = bodies.iter().map(|b| 1.0 / b.inv_mass).fold(0.0, f32::max);
        for b in &mut bodies {
            let m = 1.0 / b.inv_mass;
            if m < heaviest * 0.01 {
                let k = heaviest * 0.01 / m;
                b.inv_mass /= k;
                b.inv_inertia /= k;
            }
        }
        let mut joints = Vec::new();
        for def in &asset.joints {
            let (Some(cb), Some(pb)) = (index(&def.child), index(&def.parent)) else {
                continue;
            };
            let (Some(b), Some(a)) = (body_of[cb], body_of[pb]) else {
                continue;
            };
            if a == b {
                continue;
            }
            // Held where the child's bone is now (so the skeleton keeps its own
            // proportions, whatever the asset was made on).
            let at = pose[cb].0;
            let (ba, bb) = (&bodies[a], &bodies[b]);
            let mut joint = Joint {
                a,
                b,
                ra: ba.q.inverse() * (at - ba.x),
                rb: bb.q.inverse() * (at - bb.x),
                a_axes: def.parent_axes,
                b_axes: def.child_axes,
                swing1: def.swing1,
                swing2: def.swing2,
                twist: def.twist,
                grace: 0.0,
                welded: false,
            };
            joint.grace = joint.past_limits(ba.q, bb.q);
            joints.push(joint);
        }
        let mut r = Ragdoll {
            bodies,
            joints,
            scale,
            age: 0.0,
            still: 0.0,
            drift: (Vec3::ZERO, 0.0),
            gravity: 1.0,
            touched: false,
        };
        r.weld_locked();
        Some(r)
    }

    /// Each piece held by a locked joint (all three of its limits shut), with nothing
    /// hung from it, welded to its parent (Weld) in the turn the joint holds it at.
    fn weld_locked(&mut self) {
        let locked = |l: Option<f32>| l.is_some_and(|l| l <= MIN_LIMIT);
        for j in 0..self.joints.len() {
            let jt = &self.joints[j];
            let (a, b) = (jt.a, jt.b);
            if !(locked(jt.swing1) && locked(jt.swing2) && locked(jt.twist))
                || self.joints.iter().any(|o| o.a == b)
                || self.bodies[b].weld.is_some()
                || self.bodies[a].weld.is_some()
            {
                continue;
            }
            // (Its turn where the joint's frames line up, and its place where the
            // joint's anchors meet.)
            let frame = |axes: &[Vec3; 2]| {
                let x = axes[0].normalize_or(Vec3::X);
                let z = x.cross(axes[1]).normalize_or(Vec3::Z);
                Quat::from_mat3(&Mat3::from_cols(x, z.cross(x), z))
            };
            let turn = (frame(&jt.a_axes) * frame(&jt.b_axes).inverse()).normalize();
            let at = jt.ra - turn * jt.rb;
            self.joints[j].welded = true;
            let child = &self.bodies[b];
            let (m, i) = (1.0 / child.inv_mass, 1.0 / child.inv_inertia);
            let probes: Vec<(Vec3, f32)> = child
                .probes
                .iter()
                .map(|(c, r)| (at + turn * *c, *r))
                .collect();
            let inertia = i + m * at.length_squared() * 2.0 / 3.0;
            let parent = &mut self.bodies[a];
            let (pm, pi) = (1.0 / parent.inv_mass, 1.0 / parent.inv_inertia);
            parent.inv_mass = 1.0 / (pm + m);
            parent.inv_inertia = 1.0 / (pi + inertia);
            parent.k2 = (pi + inertia) / (pm + m);
            parent.reach = probes
                .iter()
                .map(|(c, r)| c.length() + r)
                .fold(parent.reach, f32::max);
            parent
                .probes_of
                .extend(std::iter::repeat_n(b, probes.len()));
            parent.probes.extend(probes);
            self.bodies[b].weld = Some(Weld {
                to: a,
                at,
                turn,
                mass: m,
                inertia,
            });
        }
        self.follow_welds();
    }

    /// The welded pieces where their parents hold them, moving as they do.
    fn follow_welds(&mut self) {
        for i in 0..self.bodies.len() {
            let Some(w) = self.bodies[i].weld.clone() else {
                continue;
            };
            let p = &self.bodies[w.to];
            let (x, q, v, wv) = (
                p.x + p.q * w.at,
                (p.q * w.turn).normalize(),
                p.v + p.w.cross(p.q * w.at),
                p.w,
            );
            let b = &mut self.bodies[i];
            (b.x, b.q, b.v, b.w) = (x, q, v, wv);
            (b.px, b.pq) = (x, q);
        }
    }

    /// A welded piece let go (its joint severed): a body of its own again, moving as
    /// its parent moved it.
    fn unweld(&mut self, i: usize) {
        self.follow_welds();
        let Some(w) = self.bodies[i].weld.take() else {
            return;
        };
        let parent = &mut self.bodies[w.to];
        let (pm, pi) = (
            1.0 / parent.inv_mass - w.mass,
            1.0 / parent.inv_inertia - w.inertia,
        );
        parent.inv_mass = 1.0 / pm.max(1e-6);
        parent.inv_inertia = 1.0 / pi.max(1e-9);
        parent.k2 = (pi / pm.max(1e-6)).max(1e-6);
        let mut k = 0;
        while k < parent.probes.len() {
            if parent.probes_of[k] == i {
                parent.probes.remove(k);
                parent.probes_of.remove(k);
            } else {
                k += 1;
            }
        }
        parent.reach = parent
            .probes
            .iter()
            .map(|(c, r)| c.length() + r)
            .fold(0.0, f32::max);
    }

    /// A blow's momentum: `dv` (metres a second) on the body nearest `at`, a share
    /// of it on the others by how far they are from it (the tests').
    #[cfg(test)]
    pub fn push(&mut self, at: Vec3, dv: Vec3) {
        let Some(hit) = self.nearest(at) else {
            return;
        };
        let reach = (2.0 * self.scale).max(0.5);
        for (i, b) in self.bodies.iter_mut().enumerate() {
            let k = if i == hit {
                1.0
            } else {
                0.35 * (1.0 - b.x.distance(at) / (reach * 2.0)).clamp(0.0, 1.0)
            };
            if k <= 0.0 {
                continue;
            }
            b.v += dv * k;
            // (Struck off its middle, it turns.)
            let r = at - b.x;
            b.w +=
                (r.cross(dv) * k * 0.4 / (b.reach * b.reach).max(1e-3)).clamp_length_max(MAX_SPIN);
        }
    }

    /// Every body moving `v` faster (what the whole of it was moving at besides its
    /// clip: a player running or in the air as he dies).
    pub fn carry(&mut self, v: Vec3) {
        let v = v.clamp_length_max(MAX_SPEED);
        for b in &mut self.bodies {
            b.v += v;
        }
    }

    /// A blast flinging every body away from `centre` at `speed`, less the further it
    /// is, and up by `up` (the Dimensional Decimator's black hole going off:
    /// equipment.rs).
    pub fn explode(&mut self, centre: Vec3, speed: f32, up: f32) {
        let reach = (3.0 * self.scale).max(1.0);
        for b in &mut self.bodies {
            let away = b.x - centre;
            let d = away.length();
            let k = (1.0 - d / (reach * 3.0)).clamp(0.3, 1.0);
            b.v += away.normalize_or(Vec3::Y) * speed * k + Vec3::Y * up * k;
        }
        self.unsettle();
    }

    /// The kill's momentum as the ragdoll takes over (TnDeathActionRagdoll.AddMomentum,
    /// for each body: its speed plus `momentum` times a factor at random within `k`,
    /// the whole turned at random within `direction` radians: VaryImpulseDirection).
    pub fn add_momentum(
        &mut self,
        momentum: Vec3,
        k: (f32, f32),
        direction: (f32, f32),
        seed: &mut u32,
    ) {
        for b in &mut self.bodies {
            let f = k.0 + (k.1 - k.0) * next(seed);
            let v = b.v + momentum * f;
            b.v = vary(v, direction, seed).clamp_length_max(MAX_SPEED);
        }
        self.unsettle();
    }

    /// A radial impulse (USkeletalMeshComponent.AddRadialImpulse: AddRadialImpulseToBody
    /// for each body): every body whose centre of mass is within `radius` of `origin`
    /// thrown away from it by `strength`, less the further it is if `linear`
    /// (RIF_Linear); a change in speed of that many metres a second
    /// (`velocity_change`, NX_VELOCITY_CHANGE), or an impulse its mass takes
    /// (NX_IMPULSE: `strength` in PhysX's mass units times metres a second, the
    /// game's uu-units' over 100), so the lighter bodies are thrown faster.
    pub fn radial(
        &mut self,
        origin: Vec3,
        radius: f32,
        strength: f32,
        linear: bool,
        velocity_change: bool,
    ) {
        for b in &mut self.bodies {
            let away = b.x - origin;
            let d = away.length();
            if d > radius {
                continue;
            }
            let s = if linear {
                strength * (1.0 - d / radius.max(1e-3))
            } else {
                strength
            };
            let k = if velocity_change { 1.0 } else { b.inv_mass };
            b.v = (b.v + away.normalize_or(Vec3::Y) * s * k).clamp_length_max(MAX_IMPULSE_SPEED);
        }
        self.unsettle();
    }

    /// Every body stopped (TnRagdollActionHaltBodies).
    pub fn halt(&mut self) {
        for b in &mut self.bodies {
            b.v = Vec3::ZERO;
            b.w = Vec3::ZERO;
        }
    }

    /// An impulse on its body of `bone`, or with none its root body (the one nearest
    /// the root of the skeleton: what the mesh's AddLinearVelocity, AddAngularVelocity,
    /// AddLinearImpulse and AddAngularImpulse act on with no body named;
    /// TnRagdollActionAddImpulse): `linear` and `angular` as a change in speed (metres
    /// and radians a second), or (`mass`) as an impulse it takes (PhysX's units: its
    /// mass times metres a second, and its inertia, its mass times its gyration in
    /// PhysX's half metres squared, times radians a second), an impulse at a point
    /// (`at`: UsePointImpulse) turning it too, by its reach from the body's centre of
    /// mass (a change in speed there is the body's alone). Its spin is held to
    /// PhysX's limit.
    pub fn impulse(
        &mut self,
        bone: Option<usize>,
        linear: Vec3,
        angular: Vec3,
        mass: bool,
        at: Option<Vec3>,
    ) {
        let target = match bone {
            Some(i) => self
                .body_of(i)
                .map(|b| self.bodies[b].weld.as_ref().map_or(b, |w| w.to)),
            None => self
                .bodies
                .iter()
                .enumerate()
                .min_by_key(|(_, b)| b.bone)
                .map(|(i, _)| i),
        };
        let Some(b) = target.and_then(|i| self.bodies.get_mut(i)) else {
            return;
        };
        let (lin, ang) = if mass {
            let i = b.k2 * PHYSX_UNITS_PER_METRE * PHYSX_UNITS_PER_METRE / b.inv_mass;
            // (At a point: the turn its reach gives the impulse, r x J over the
            // inertia, both in the demo's metres.)
            let turn = at.map_or(Vec3::ZERO, |p| (p - b.x).cross(linear) * b.inv_inertia);
            (linear * b.inv_mass, angular / i.max(1e-6) + turn)
        } else {
            (linear, angular)
        };
        b.v = (b.v + lin).clamp_length_max(MAX_IMPULSE_SPEED);
        let w = b.w + ang;
        b.w = w.clamp_length_max(MAX_IMPULSE_SPIN.max(b.w.length()));
        self.unsettle();
    }

    /// The joints holding each of `bones` (their indices in the skeleton) to its
    /// parent's body let go (Mutilation.SeverPieces: the piece falls away on its
    /// own), those of the bodies within `near` of a point only (UseMinDistanceFromHit);
    /// the bones cut away.
    pub fn sever(&mut self, bones: &[usize], near: Option<(Vec3, f32)>) -> Vec<usize> {
        self.follow_welds();
        let mut cut = Vec::new();
        let mut freed = Vec::new();
        let bodies = &self.bodies;
        self.joints.retain(|j| {
            let child = &bodies[j.b];
            let hit = bones.contains(&child.bone)
                && near.is_none_or(|(at, r)| child.x.distance_squared(at) <= r * r);
            if hit {
                cut.push(child.bone);
                if j.welded {
                    freed.push(j.b);
                }
            }
            !hit
        });
        for b in freed {
            self.unweld(b);
        }
        cut
    }

    /// `count` of its joints at random let go (Mutilation.SeverRandomLimbs, or
    /// SeverRandomLimbsWithDistance: of those whose child body is within `near` of a
    /// point); the bones cut away.
    pub fn sever_random(
        &mut self,
        count: usize,
        near: Option<(Vec3, f32)>,
        seed: &mut u32,
    ) -> Vec<usize> {
        self.follow_welds();
        let mut candidates: Vec<usize> = (0..self.joints.len())
            .filter(|&j| {
                let child = &self.bodies[self.joints[j].b];
                near.is_none_or(|(at, r)| child.x.distance_squared(at) <= r * r)
            })
            .collect();
        let mut chosen = Vec::new();
        while chosen.len() < count && !candidates.is_empty() {
            let i = ((next(seed) * candidates.len() as f32) as usize).min(candidates.len() - 1);
            chosen.push(candidates.swap_remove(i));
        }
        chosen.sort_unstable_by(|a, b| b.cmp(a));
        let mut cut = Vec::new();
        for j in chosen {
            let joint = self.joints.remove(j);
            cut.push(self.bodies[joint.b].bone);
            if joint.welded {
                self.unweld(joint.b);
            }
        }
        cut
    }

    /// A local angular acceleration for `dt` (TnDeathActionApplyAngularForce:
    /// AddLocalAngularAcceleration), `accel` in the axes each body had as the ragdoll
    /// took over (radians a second squared).
    pub fn spin(&mut self, accel: Vec3, dt: f32) {
        for b in &mut self.bodies {
            let turn = b.q * b.q0.inverse();
            let w = b.w + turn * accel * dt;
            b.w = w.clamp_length_max(MAX_IMPULSE_SPIN.max(b.w.length()));
        }
        self.unsettle();
    }

    /// A plane's boost kept on as it dies (TnDeathActionPlaneMotionControl: its
    /// BoostingInput): each body pushed along `forward` (as it was as the ragdoll took
    /// over, turned as it has turned since) up to `speed`, at `accel`.
    pub fn boost(&mut self, forward: Vec3, speed: f32, accel: f32, dt: f32) {
        for b in &mut self.bodies {
            let f = (b.q * b.q0.inverse()) * forward;
            let along = b.v.dot(f);
            if along < speed {
                b.v += f * (speed - along).min(accel * dt);
            }
        }
        self.unsettle();
    }

    /// Where the bone (its index in the skeleton) is: its body's bone's place (its
    /// centre of mass less its offset on the bone), if it has a body.
    pub fn bone_place(&self, bone: usize) -> Option<Vec3> {
        let b = &self.bodies[self.body_of(bone)?];
        Some(b.x - b.q * b.com)
    }

    /// Its body of `bone` (its index in the skeleton), if it has one.
    pub fn body_of(&self, bone: usize) -> Option<usize> {
        self.bodies.iter().position(|b| b.bone == bone)
    }

    /// (Moving again: it isn't at rest.)
    fn unsettle(&mut self) {
        self.still = 0.0;
        self.drift = (Vec3::ZERO, 0.0);
    }

    /// `dt` seconds of a pull on every body: its velocity changed by `accel` (metres a
    /// second squared) of where it is and how fast it goes (the Dimensional
    /// Decimator's black hole drawing a body in, equipment.rs). A body being pulled
    /// keeps falling: it doesn't settle while it is.
    pub fn pull(&mut self, dt: f32, accel: impl Fn(Vec3, Vec3) -> Vec3) {
        for b in &mut self.bodies {
            b.v = (b.v + accel(b.x, b.v) * dt).clamp_length_max(MAX_SPEED);
        }
        self.still = 0.0;
        self.drift = (Vec3::ZERO, 0.0);
        self.age = self.age.min(SETTLE_SECS * 0.5);
    }

    /// The middle of its bodies.
    pub fn centre(&self) -> Vec3 {
        let n = self.bodies.len().max(1) as f32;
        self.bodies.iter().map(|b| b.x).sum::<Vec3>() / n
    }

    /// The body nearest a point.
    #[cfg(test)]
    fn nearest(&self, at: Vec3) -> Option<usize> {
        self.bodies
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.x.distance_squared(at)
                    .total_cmp(&b.x.distance_squared(at))
            })
            .map(|(i, _)| i)
    }

    /// Whether it has come to rest (or fallen for long enough).
    pub fn settled(&self) -> bool {
        self.still >= REST_SECS || self.drift.1 >= DRIFT_SECS || self.age >= SETTLE_SECS
    }

    /// Each body's bone, where it is and its turn.
    pub fn bone_poses(&self) -> impl Iterator<Item = (usize, Vec3, Quat)> + '_ {
        self.bodies.iter().map(|b| (b.bone, b.x - b.q * b.com, b.q))
    }

    /// `dt` seconds of falling onto `world`.
    pub fn step(&mut self, dt: f32, world: &dyn World) {
        if dt <= 0.0 || self.settled() {
            return;
        }
        let dt = dt.min(1.0 / 20.0);
        self.age += dt;
        // The triangles each contact sphere could reach this frame.
        let margin = 0.3 * self.scale.max(0.2);
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for b in &self.bodies {
            let r = Vec3::splat(b.reach + b.v.length() * dt + margin);
            lo = lo.min(b.x - r);
            hi = hi.max(b.x + r);
        }
        let mut tris = Vec::new();
        world.triangles(lo, hi, &mut tris);
        let floor = world.floor();
        let mut near: Vec<Vec<Vec<usize>>> = Vec::with_capacity(self.bodies.len());
        for b in &self.bodies {
            if b.weld.is_some() {
                // (Its spheres are its parent's while it is welded.)
                near.push(Vec::new());
                continue;
            }
            let travel = b.v.length() * dt + b.w.length() * b.reach * dt + margin;
            near.push(
                b.probes
                    .iter()
                    .map(|(c, r)| {
                        let at = b.x + b.q * *c;
                        let reach = r + travel;
                        tris.iter()
                            .enumerate()
                            .filter(|(_, t)| {
                                let tlo = t[0].min(t[1]).min(t[2]);
                                let thi = t[0].max(t[1]).max(t[2]);
                                (at + Vec3::splat(reach)).cmpge(tlo).all()
                                    && (at - Vec3::splat(reach)).cmple(thi).all()
                            })
                            .map(|(i, _)| i)
                            .collect()
                    })
                    .collect(),
            );
        }
        let h = dt / SUBSTEPS as f32;
        let mut contacts = Vec::new();
        let mut touched_any = false;
        let max_push = MAX_PUSH_SPEED * self.scale.max(0.25);
        // (Where they were as the frame began: how fast they have moved over it.)
        let starts: Vec<(Vec3, Quat)> = self.bodies.iter().map(|b| (b.x, b.q)).collect();
        for _ in 0..SUBSTEPS {
            for b in self.bodies.iter_mut().filter(|b| b.weld.is_none()) {
                b.px = b.x;
                b.pq = b.q;
                b.v.y += GRAVITY * self.gravity * h;
                b.v *= 1.0 - (LINEAR_DAMPING * h).min(1.0);
                b.w *= 1.0 - (ANGULAR_DAMPING * h).min(1.0);
                b.x += b.v * h;
                b.q = turned(b.q, b.w * h);
            }
            contacts.clear();
            for (i, b) in self.bodies.iter_mut().enumerate() {
                if b.weld.is_some() {
                    continue;
                }
                for (k, near_k) in near[i].iter().enumerate() {
                    let (c, r) = b.probes[k];
                    let (c, r) = (&c, &r);
                    let at = b.x + b.q * *c;
                    // The deepest it is into the level.
                    let mut best: Option<(Vec3, f32)> = None;
                    for &t in near_k {
                        let [p0, p1, p2] = tris[t];
                        let q = closest_on_triangle(at, p0, p1, p2);
                        let d = at - q;
                        let l = d.length();
                        if l >= *r {
                            continue;
                        }
                        let n = if l > 1e-6 {
                            d / l
                        } else {
                            // (Its centre on the surface: out along the face, the way
                            // it came from.)
                            let fn_ = (p1 - p0).cross(p2 - p0).normalize_or(Vec3::Y);
                            let was = b.px + b.pq * *c;
                            if fn_.dot(was - q) < 0.0 { -fn_ } else { fn_ }
                        };
                        let depth = r - l;
                        if best.is_none_or(|(_, bd)| depth > bd) {
                            best = Some((n, depth));
                        }
                    }
                    if let Some(f) = floor {
                        let depth = f + r - at.y;
                        if depth > 0.0 && best.is_none_or(|(_, bd)| depth > bd) {
                            best = Some((Vec3::Y, depth));
                        }
                    }
                    let Some((n, depth)) = best else {
                        continue;
                    };
                    // Out of the surface, at its contact point.
                    let rw = b.q * *c - n * *r;
                    let w = b.inv_mass + b.inv_inertia * rw.cross(n).length_squared();
                    if w <= 0.0 {
                        continue;
                    }
                    let approach = (b.v + b.w.cross(rw)).dot(n);
                    let impulse = n * (depth / w);
                    apply(b, rw, impulse);
                    // Static friction: the contact point's slide this substep undone,
                    // while it is under what the push could hold.
                    let now = b.x + b.q * *c - n * *r;
                    let was = b.px + b.pq * *c - n * *r;
                    let slide = now - was;
                    let tangent = slide - n * slide.dot(n);
                    let tl = tangent.length();
                    if tl > 1e-7 && tl < STATIC_FRICTION * depth {
                        let rw = now - b.x;
                        let dir = tangent / tl;
                        let wt = b.inv_mass + b.inv_inertia * rw.cross(dir).length_squared();
                        if wt > 0.0 {
                            apply(b, rw, -tangent / wt);
                        }
                    }
                    contacts.push(Contact {
                        body: i,
                        r: b.q * *c - n * *r,
                        n,
                        push: depth / w,
                        approach,
                    });
                }
            }
            for j in 0..self.joints.len() {
                // (A limit it started past closing in a little each substep, so it
                // turns the joint back as steadily.)
                let joint = &mut self.joints[j];
                joint.grace = (joint.grace - GRACE_RATE * h).max(0.0);
                if !joint.welded {
                    self.solve_joint(j);
                }
            }
            for b in self.bodies.iter_mut().filter(|b| b.weld.is_none()) {
                b.v = ((b.x - b.px) / h).clamp_length_max(MAX_SPEED);
                b.w = spin(b.q, b.pq, h).clamp_length_max(MAX_SPIN);
            }
            // Sliding friction, and the joints' own.
            let spin_k = 1.0 - (CONTACT_SPIN_DAMPING * h).min(1.0);
            let mut touched = vec![false; self.bodies.len()];
            for c in &contacts {
                touched[c.body] = true;
            }
            for (b, _) in self.bodies.iter_mut().zip(&touched).filter(|(_, t)| **t) {
                b.w *= spin_k;
            }
            touched_any |= touched.iter().any(|t| *t);
            for c in &contacts {
                let b = &mut self.bodies[c.body];
                let vc = b.v + b.w.cross(c.r);
                let vn = vc.dot(c.n);
                let vt = vc - c.n * vn;
                let l = vt.length();
                if l > 1e-6 {
                    // (The push's impulse times the friction, or as much as stops the
                    // contact point sliding.)
                    let dir = -vt / l;
                    let w = b.inv_mass + b.inv_inertia * c.r.cross(dir).length_squared();
                    if w > 0.0 {
                        let p = dir * (DYNAMIC_FRICTION * c.push / h).min(l / w);
                        b.v += p * b.inv_mass;
                        b.w += b.inv_inertia * c.r.cross(p);
                    }
                }
                // (Not bouncing back: a point that came in leaves the surface no
                // faster than it lies on it, rather than at the speed it was pushed
                // out at; and one in it already, which the solve pushed out, no
                // faster than MAX_PUSH_SPEED: a body that starts in the floor rises
                // out of it rather than being thrown.)
                let over = if c.approach < 0.0 || vn < 0.0 {
                    vn
                } else {
                    (vn - max_push).max(0.0)
                };
                if over != 0.0 {
                    let w = b.inv_mass + b.inv_inertia * c.r.cross(c.n).length_squared();
                    if w > 0.0 {
                        let p = c.n * (-over / w);
                        b.v += p * b.inv_mass;
                        b.w += b.inv_inertia * c.r.cross(p);
                    }
                }
            }
            let k = (JOINT_DAMPING * h).min(0.5);
            for j in self.joints.iter().filter(|j| !j.welded) {
                let (wa, wb) = (self.bodies[j.a].w, self.bodies[j.b].w);
                let (ia, ib) = (self.bodies[j.a].inv_inertia, self.bodies[j.b].inv_inertia);
                let rel = (wb - wa) * k;
                let sum = ia + ib;
                if sum > 0.0 {
                    self.bodies[j.a].w += rel * (ia / sum);
                    self.bodies[j.b].w -= rel * (ib / sum);
                }
            }
        }
        self.touched = touched_any;
        self.follow_welds();
        // At rest yet? (By how far each has moved over the frame: a body that
        // trembles within it, held on the floor, has come to rest.)
        let fastest = self
            .bodies
            .iter()
            .zip(&starts)
            .map(|(b, (x0, q0))| b.x.distance(*x0) / dt + spin(b.q, *q0, dt).length() * b.reach)
            .fold(0.0, f32::max);
        let size = self.scale.max(0.25);
        if fastest < REST_SPEED * size {
            self.still += dt;
        } else {
            self.still = 0.0;
        }
        // (Or it has stayed where it is, its bodies only rocking a little.)
        let middle =
            self.bodies.iter().map(|b| b.x).sum::<Vec3>() / self.bodies.len().max(1) as f32;
        if fastest < SLOW_SPEED * size && middle.distance(self.drift.0) < DRIFT * size {
            self.drift.1 += dt;
        } else {
            self.drift = (middle, 0.0);
        }
        if fastest < SLOW_SPEED * size {
            let k = 1.0 - (SLOW_DAMPING * dt).min(1.0);
            for b in &mut self.bodies {
                b.v *= k;
                b.w *= k;
            }
        }
    }

    fn solve_joint(&mut self, j: usize) {
        let jt = self.joints[j].clone();
        let (a, b) = two_mut(&mut self.bodies, jt.a, jt.b);
        // The anchors together.
        let ra = a.q * jt.ra;
        let rb = b.q * jt.rb;
        let gap = (a.x + ra) - (b.x + rb);
        let c = gap.length();
        if c > 1e-7 {
            let n = gap / c;
            let wa = a.inv_mass + a.inv_inertia * ra.cross(n).length_squared();
            let wb = b.inv_mass + b.inv_inertia * rb.cross(n).length_squared();
            if wa + wb > 0.0 {
                let p = n * (c / (wa + wb));
                apply(b, rb, p);
                apply(a, ra, -p);
            }
        }
        let (ia, ib) = (a.inv_inertia, b.inv_inertia);
        if ia + ib <= 0.0 {
            return;
        }
        // The swing: the child's twist axis kept in its cone (an ellipse of the two
        // swing limits) round the parent's: its swing, as angles towards the
        // parent's secondary and third axes, taken to the nearest the ellipse
        // allows (a cone that is all but shut one way, a hinge's, holds its swing
        // to the open way without throwing it back across).
        let p = (a.q * jt.a_axes[0]).normalize_or(Vec3::X);
        let s = (a.q * jt.a_axes[1]).normalize_or(Vec3::Y);
        let t = p.cross(s).normalize_or(Vec3::Z);
        let s = t.cross(p);
        let d = (b.q * jt.b_axes[0]).normalize_or(Vec3::X);
        if jt.swing1.is_some() || jt.swing2.is_some() {
            let cos = p.dot(d).clamp(-1.0, 1.0);
            let angle = cos.acos();
            if angle > 1e-4 {
                let (ys, zs) = (d.dot(s), d.dot(t));
                let across = (ys * ys + zs * zs).sqrt().max(1e-9);
                let (u, v) = (angle * ys / across, angle * zs / across);
                let (l1, l2) = jt.swing_limits();
                if (u / l1).powi(2) + (v / l2).powi(2) > 1.0 {
                    let (u2, v2) = nearest_on_ellipse(u, v, l1, l2);
                    let to_angle = (u2 * u2 + v2 * v2).sqrt();
                    let side = (s * u2 + t * v2).normalize_or(s);
                    let want = p * to_angle.cos() + side * to_angle.sin();
                    let axis = d.cross(want);
                    let turn = d.dot(want).clamp(-1.0, 1.0).acos();
                    if let Some(axis) = axis.try_normalize() {
                        let fix = axis * turn;
                        // (The child turned to it, the parent the other way, each by
                        // its share.)
                        rotate(b, fix * (ib / (ia + ib)));
                        rotate(a, -fix * (ia / (ia + ib)));
                    }
                }
            }
        }
        // The twist: the child's secondary axis about its twist axis, from the
        // parent's carried along by the swing.
        if let Some(limit) = jt.twist {
            let p = (a.q * jt.a_axes[0]).normalize_or(Vec3::X);
            let s = (a.q * jt.a_axes[1]).normalize_or(Vec3::Y);
            let d = (b.q * jt.b_axes[0]).normalize_or(Vec3::X);
            let e = (b.q * jt.b_axes[1]).normalize_or(Vec3::Y);
            let carried = Quat::from_rotation_arc(p, d) * s;
            let e = (e - d * e.dot(d)).normalize_or(carried);
            let twist = carried.cross(e).dot(d).atan2(carried.dot(e));
            let limit = (limit + jt.grace).max(MIN_LIMIT);
            if twist.abs() > limit {
                let fix = d * (twist.abs() - limit) * twist.signum();
                rotate(b, -fix * (ib / (ia + ib)));
                rotate(a, fix * (ia / (ia + ib)));
            }
        }
    }
}

impl Joint {
    /// Its swing limits as they are now (half axes of its ellipse: radians).
    fn swing_limits(&self) -> (f32, f32) {
        let open = std::f32::consts::PI;
        (
            self.swing1.map_or(open, |l| l + self.grace).max(MIN_LIMIT),
            self.swing2.map_or(open, |l| l + self.grace).max(MIN_LIMIT),
        )
    }

    /// How far past its limits it is with its parent's body turned `qa` and its
    /// child's `qb` (radians: the least grace that would let it be so).
    fn past_limits(&self, qa: Quat, qb: Quat) -> f32 {
        let p = (qa * self.a_axes[0]).normalize_or(Vec3::X);
        let s0 = (qa * self.a_axes[1]).normalize_or(Vec3::Y);
        let t = p.cross(s0).normalize_or(Vec3::Z);
        let s = t.cross(p);
        let d = (qb * self.b_axes[0]).normalize_or(Vec3::X);
        let mut past = 0.0f32;
        if let Some(limit) = self.twist {
            let e = (qb * self.b_axes[1]).normalize_or(Vec3::Y);
            let carried = Quat::from_rotation_arc(p, d) * s0;
            let e = (e - d * e.dot(d)).normalize_or(carried);
            let twist = carried.cross(e).dot(d).atan2(carried.dot(e));
            past = past.max(twist.abs() - limit.max(MIN_LIMIT));
        }
        let angle = p.dot(d).clamp(-1.0, 1.0).acos();
        if (self.swing1.is_some() || self.swing2.is_some()) && angle > 1e-4 {
            let (ys, zs) = (d.dot(s), d.dot(t));
            let across = (ys * ys + zs * zs).sqrt().max(1e-9);
            let (u, v) = (angle * ys / across, angle * zs / across);
            let open = std::f32::consts::PI;
            let (l1, l2) = (self.swing1.unwrap_or(open), self.swing2.unwrap_or(open));
            let inside = |g: f32| {
                (u / (l1 + g).max(MIN_LIMIT)).powi(2) + (v / (l2 + g).max(MIN_LIMIT)).powi(2) <= 1.0
            };
            if !inside(0.0) {
                // (Halving between none and the whole swing, which lets it be.)
                let (mut lo, mut hi) = (0.0f32, angle);
                for _ in 0..24 {
                    let mid = 0.5 * (lo + hi);
                    if inside(mid) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                past = past.max(hi);
            }
        }
        past.max(0.0)
    }
}

/// The nearest point to (u, v), outside it, on the ellipse with half axes `a` and
/// `b`: (a² u / (k + a²), b² v / (k + b²)) for the k (over 0) that puts it there,
/// found by halving.
fn nearest_on_ellipse(u: f32, v: f32, a: f32, b: f32) -> (f32, f32) {
    let (au, bv) = (a * u.abs(), b * v.abs());
    let (a2, b2) = (a * a, b * b);
    let off = |k: f32| (au / (k + a2)).powi(2) + (bv / (k + b2)).powi(2) - 1.0;
    let (mut lo, mut hi) = (0.0f32, (au + bv).max(1e-9));
    for _ in 0..32 {
        let mid = 0.5 * (lo + hi);
        if off(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let k = 0.5 * (lo + hi);
    (a2 * u / (k + a2), b2 * v / (k + b2))
}

/// A random number 0 to 1 from a seed (xorshift).
fn next(seed: &mut u32) -> f32 {
    *seed = (*seed).max(1);
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed >> 8) as f32 / (1u32 << 24) as f32
}

/// `v` turned at random within a cone of a half angle at random between
/// `variance`'s two (radians), its speed kept (VaryImpulseDirection: none if either
/// is 0).
fn vary(v: Vec3, variance: (f32, f32), seed: &mut u32) -> Vec3 {
    let len = v.length();
    if variance.0 == 0.0 || variance.1 == 0.0 || len < 1e-6 {
        return v;
    }
    let half = variance.0 + (variance.1 - variance.0) * next(seed);
    let dir = v / len;
    // (A direction within the cone round `dir`: RandomVectorInCone.)
    let cos = 1.0 - next(seed) * (1.0 - half.cos());
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let phi = next(seed) * std::f32::consts::TAU;
    let (a, b) = dir.any_orthonormal_pair();
    (dir * cos + (a * phi.cos() + b * phi.sin()) * sin) * len
}

/// Two different bodies, both changeable.
fn two_mut(bodies: &mut [Body], a: usize, b: usize) -> (&mut Body, &mut Body) {
    if a < b {
        let (lo, hi) = bodies.split_at_mut(b);
        (&mut lo[a], &mut hi[0])
    } else {
        let (lo, hi) = bodies.split_at_mut(a);
        (&mut hi[0], &mut lo[b])
    }
}

/// A positional push `p` on a body at `r` from its centre of mass (world).
fn apply(b: &mut Body, r: Vec3, p: Vec3) {
    b.x += p * b.inv_mass;
    let turn = r.cross(p) * b.inv_inertia;
    b.q = turned(b.q, turn);
}

fn rotate(b: &mut Body, by: Vec3) {
    b.q = turned(b.q, by);
}

/// A turn `q` turned on by the small rotation `by` (axis times angle, world).
fn turned(q: Quat, by: Vec3) -> Quat {
    (q + Quat::from_xyzw(by.x, by.y, by.z, 0.0) * q * 0.5).normalize()
}

/// The spin (axis times radians a second, world) that takes `from` to `to` in `dt`.
fn spin(to: Quat, from: Quat, dt: f32) -> Vec3 {
    let d = to * from.inverse();
    let v = Vec3::new(d.x, d.y, d.z) * (2.0 / dt.max(1e-5));
    if d.w < 0.0 { -v } else { v }
}

// ---------------------------------------------------------------------------
// In the world
// ---------------------------------------------------------------------------

/// What a death's Ragdoll action does to the ragdoll as it takes over
/// (TnDeathActionRagdoll.OnStartRagdoll), or to the one already falling (a later
/// keyframe's): the speed the whole body was moving at besides its clip (`carry`,
/// metres a second: a player running or in the air), the kill's momentum on every
/// body (its TearOffMomentum, metres a second, times a factor at random within
/// `momentum_k` for each: MomentumImpulseVariance), each body's speed then turned at
/// random within `direction` (radians: DirectionImpulseVariance), and its ragdoll
/// actions in their order (ApplyRemainingRagdollActions: all as it takes over).
#[derive(Clone, Debug, Default)]
pub struct Kick {
    pub carry: Vec3,
    pub momentum: Vec3,
    pub momentum_k: (f32, f32),
    pub direction: (f32, f32),
    pub actions: Vec<KickAction>,
    pub seed: u32,
}

/// A ragdoll action (deaths.rs's RagdollDo, placed in the world).
#[derive(Clone, Debug, PartialEq)]
pub enum KickAction {
    /// TnRagdollActionRadialImpulse (Ragdoll::radial: metres; metres a second, or
    /// PhysX's mass units times metres a second): from `origin`, or with a bone from
    /// that far from the bone (GetBoneLocation: the bone's own place, the ragdoll's
    /// middle for a bone it hasn't a body for).
    Radial {
        origin: Vec3,
        bone: Option<String>,
        radius: f32,
        strength: f32,
        linear: bool,
        velocity_change: bool,
    },
    /// TnRagdollActionHaltBodies.
    Halt,
    /// TnRagdollActionAddImpulse (Ragdoll::impulse: its bone by name, none for the
    /// root body; at a point in the world, or its centre of mass).
    Impulse {
        bone: Option<String>,
        linear: Vec3,
        angular: Vec3,
        mass: bool,
        at: Option<Vec3>,
    },
    /// TnRagdollActionSeverPieces (its bones' pieces), TnRagdollActionSeverRandomPieces
    /// (`random` of them): only those within `near` of where it was struck, if it
    /// says; each cut piece trailing `trail` (its SeveredPieceParticleTemplate) at its
    /// `chance`.
    Sever {
        bones: Vec<String>,
        random: u32,
        near: Option<(Vec3, f32)>,
        trail: Option<Arc<str>>,
        chance: f32,
    },
}

impl Kick {
    /// The kick on a ragdoll (its bones' names to find a body by); the pieces it cut
    /// away that trail an effect (their bones' indices, and the effect).
    pub fn apply(&self, r: &mut Ragdoll, names: &[String]) -> Vec<(usize, Arc<str>)> {
        let mut seed = self.seed.max(1);
        let mut trails = Vec::new();
        r.carry(self.carry);
        r.add_momentum(self.momentum, self.momentum_k, self.direction, &mut seed);
        for a in &self.actions {
            match a {
                KickAction::Radial {
                    origin,
                    bone,
                    radius,
                    strength,
                    linear,
                    velocity_change,
                } => {
                    let from = match bone {
                        Some(b) => {
                            let place = names
                                .iter()
                                .position(|n| n == b)
                                .and_then(|i| r.bone_place(i))
                                .unwrap_or_else(|| r.centre());
                            place + *origin
                        }
                        None => *origin,
                    };
                    r.radial(from, *radius, *strength, *linear, *velocity_change)
                }
                KickAction::Halt => r.halt(),
                KickAction::Impulse {
                    bone,
                    linear,
                    angular,
                    mass,
                    at,
                } => {
                    let index = bone
                        .as_ref()
                        .and_then(|b| names.iter().position(|n| n == b))
                        .filter(|i| r.body_of(*i).is_some());
                    // (A bone without a body of its own: the root body.)
                    r.impulse(index, *linear, *angular, *mass, *at);
                }
                KickAction::Sever {
                    bones,
                    random,
                    near,
                    trail,
                    chance,
                } => {
                    let cut = if *random > 0 {
                        r.sever_random(*random as usize, *near, &mut seed)
                    } else {
                        let list: Vec<usize> = bones
                            .iter()
                            .filter_map(|b| names.iter().position(|n| n == b))
                            .collect();
                        r.sever(&list, *near)
                    };
                    if let Some(t) = trail {
                        for bone in cut {
                            if next(&mut seed) < *chance {
                                trails.push((bone, t.clone()));
                            }
                        }
                    }
                }
            }
        }
        trails
    }
}

/// How a rig's skeleton is posed.
#[derive(Clone, Debug, Default)]
pub enum RigState {
    /// By its clips.
    #[default]
    Animated,
    /// To start falling this frame.
    Falling(Kick),
    /// By its bodies.
    Ragdoll(Box<Ragdoll>),
    /// Come to rest: the bones' last poses, kept (their own transforms).
    Rest(Vec<Transform>),
}

/// A skeleton that can fall as its physics asset: its bones in the order their
/// hierarchy goes (each one's parent before it), their parents (in the list), the
/// entity they hang from (the model's root: the root bone's parent) and their
/// names.
#[derive(Component)]
pub struct Rig {
    pub asset: Arc<PhysicsAsset>,
    pub bones: Vec<Entity>,
    pub parents: Vec<Option<usize>>,
    pub names: Vec<String>,
    pub base: Entity,
    pub state: RigState,
    /// What a death does to its ragdoll each frame (deaths.rs): its gravity's
    /// factor, a spin to give it this frame (Ragdoll::spin), a plane's boost kept on
    /// (its forward as it took over and its speed), and a later Ragdoll action's
    /// kick to give the ragdoll already falling (or start it again, at rest).
    pub gravity: f32,
    pub spin: Vec3,
    pub boost: Option<(Vec3, f32)>,
    pub pending: Option<Kick>,
    /// Its ragdoll touched the level in its last step.
    pub touched: bool,
    /// The pieces its kicks cut away that trail an effect (their bones' indices and
    /// the effect), for its death to start.
    pub severed: Vec<(usize, Arc<str>)>,
}

impl Rig {
    /// A skeleton's rig: its bones under `base` (every named entity down from it that
    /// `is_bone` takes: its skins' joints), in their hierarchy's order.
    pub fn new(
        asset: Arc<PhysicsAsset>,
        base: Entity,
        children: &Query<&Children>,
        names: &Query<&Name>,
        is_bone: impl Fn(Entity) -> bool,
    ) -> Rig {
        let mut bones = Vec::new();
        let mut parents = Vec::new();
        let mut bone_names = Vec::new();
        // (Breadth first, so parents come before their children.)
        let mut queue: std::collections::VecDeque<(Entity, Option<usize>)> =
            std::collections::VecDeque::from([(base, None)]);
        while let Some((e, parent)) = queue.pop_front() {
            let mut me = parent;
            if e != base
                && is_bone(e)
                && let Ok(n) = names.get(e)
            {
                me = Some(bones.len());
                bones.push(e);
                parents.push(parent);
                bone_names.push(n.as_str().to_string());
            }
            if let Ok(kids) = children.get(e) {
                for k in kids.iter() {
                    queue.push_back((k, me));
                }
            }
        }
        Rig {
            asset,
            bones,
            parents,
            names: bone_names,
            base,
            state: RigState::Animated,
            gravity: 1.0,
            spin: Vec3::ZERO,
            boost: None,
            pending: None,
            touched: false,
            severed: Vec::new(),
        }
    }

    /// Back to its clips, the death's controls off (a player come back).
    pub fn reset(&mut self) {
        self.state = RigState::Animated;
        self.gravity = 1.0;
        self.spin = Vec3::ZERO;
        self.boost = None;
        self.pending = None;
        self.touched = false;
        self.severed.clear();
    }

    /// Its bone `name`'s entity, if it has that bone.
    pub fn bone(&self, name: &str) -> Option<Entity> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|i| self.bones[i])
    }

    /// Where its bodies' bones are (their middle, as last drawn), however it is
    /// posed: what a death's lights, sounds and dissolve keep to.
    pub fn body_centre(&self, globals: &Query<&GlobalTransform>) -> Option<Vec3> {
        let mut sum = Vec3::ZERO;
        let mut n = 0;
        for body in &self.asset.bodies {
            if let Some(g) = self
                .names
                .iter()
                .position(|b| *b == body.bone)
                .and_then(|i| globals.get(self.bones[i]).ok())
            {
                sum += g.translation();
                n += 1;
            }
        }
        (n > 0).then(|| sum / n as f32)
    }
}

/// The level as a ragdoll lands on it.
struct LevelWorld<'a>(&'a crate::Level);

impl World for LevelWorld<'_> {
    fn triangles(&self, min: Vec3, max: Vec3, out: &mut Vec<[Vec3; 3]>) {
        if let Some(t) = self.0.surfaces_in(min, max) {
            out.extend(t);
        }
    }
    fn floor(&self) -> Option<f32> {
        // (The arena's floor is a plane at 0.)
        self.0.collision.is_none().then_some(0.0)
    }
}

/// Starts the rigs that are to fall (their bones where this frame's clips put them,
/// going on at the speed they moved since the last), runs the falling ones and
/// poses their bones, and keeps the ones at rest posed (after the clips have run, so
/// their poses stand whatever the clips do; before the transforms are propagated).
pub fn drive_rigs(
    time: Res<Time>,
    level: Res<crate::Level>,
    mut rigs: Query<&mut Rig>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    let world = LevelWorld(&level);
    for mut rig in &mut rigs {
        let rig = &mut *rig;
        // (A later Ragdoll action: on the ragdoll already falling; at rest, it falls
        // again from where it lies.)
        if let Some(kick) = rig.pending.take() {
            match &mut rig.state {
                RigState::Ragdoll(r) => {
                    let cut = kick.apply(r, &rig.names);
                    rig.severed.extend(cut);
                }
                RigState::Rest(_) | RigState::Animated => rig.state = RigState::Falling(kick),
                RigState::Falling(k) => {
                    k.actions.extend(kick.actions);
                    k.momentum += kick.momentum;
                }
            }
        }
        match &mut rig.state {
            RigState::Animated => {}
            RigState::Rest(locals) => {
                for (e, t) in rig.bones.iter().zip(locals.iter()) {
                    if let Ok(mut tr) = transforms.get_mut(*e)
                        && *tr != *t
                    {
                        *tr = *t;
                    }
                }
            }
            RigState::Falling(kick) => {
                let kick = std::mem::take(kick);
                let Ok(base) = globals.get(rig.base) else {
                    continue;
                };
                // This frame's pose, from the bones' own transforms; last frame's,
                // as it was drawn.
                let locals: Vec<Transform> = rig
                    .bones
                    .iter()
                    .map(|e| transforms.get(*e).copied().unwrap_or_default())
                    .collect();
                let worlds = world_poses(base.affine(), &rig.parents, &locals);
                let pose: Vec<BonePose> = worlds.iter().map(pose_of).collect();
                let prev: Vec<BonePose> = rig
                    .bones
                    .iter()
                    .zip(worlds.iter())
                    .map(|(e, w)| globals.get(*e).map_or(*w, |g| g.affine()))
                    .map(|a| pose_of(&a))
                    .collect();
                let scale = worlds
                    .first()
                    .map_or(1.0, |w| w.matrix3.x_axis.length())
                    .max(1e-3);
                match Ragdoll::new(&rig.asset, &rig.names, &pose, &prev, scale, dt.max(1e-3)) {
                    Some(mut r) => {
                        let cut = kick.apply(&mut r, &rig.names);
                        rig.severed.extend(cut);
                        r.gravity = rig.gravity;
                        rig.state = RigState::Ragdoll(Box::new(r));
                    }
                    None => rig.state = RigState::Rest(locals),
                }
            }
            RigState::Ragdoll(r) => {
                r.gravity = rig.gravity;
                if rig.spin != Vec3::ZERO {
                    r.spin(rig.spin, dt);
                }
                if let Some((forward, speed)) = rig.boost {
                    r.boost(forward, speed, BOOST_ACCEL, dt);
                }
                r.step(dt, &world);
                rig.touched = r.touched;
                let Ok(base) = globals.get(rig.base) else {
                    continue;
                };
                let mut locals: Vec<Transform> = rig
                    .bones
                    .iter()
                    .map(|e| transforms.get(*e).copied().unwrap_or_default())
                    .collect();
                let mut placed: Vec<Option<(Vec3, Quat)>> = vec![None; rig.bones.len()];
                for (bone, at, q) in r.bone_poses() {
                    placed[bone] = Some((at, q));
                }
                // Down the hierarchy: a body's bone where its body is, the rest where
                // their parents carry them.
                let mut worlds: Vec<Affine3A> = Vec::with_capacity(locals.len());
                for i in 0..locals.len() {
                    let parent = rig.parents[i].map_or(base.affine(), |p| worlds[p]);
                    let w = match placed[i] {
                        Some((at, q)) => {
                            let s = parent.matrix3.x_axis.length().max(1e-3)
                                * locals[i].scale.x.abs().max(1e-3);
                            let w =
                                Affine3A::from_scale_rotation_translation(Vec3::splat(s), q, at);
                            let local = parent.inverse() * w;
                            let (ls, lr, lt) = local.to_scale_rotation_translation();
                            locals[i] = Transform {
                                translation: lt,
                                rotation: lr.normalize(),
                                scale: ls,
                            };
                            w
                        }
                        None => parent * locals[i].compute_affine(),
                    };
                    worlds.push(w);
                }
                for (e, t) in rig.bones.iter().zip(locals.iter()) {
                    if let Ok(mut tr) = transforms.get_mut(*e) {
                        *tr = *t;
                    }
                }
                // (Spun or boosted, it is still flying.)
                if r.settled() && rig.spin == Vec3::ZERO && rig.boost.is_none() {
                    rig.state = RigState::Rest(locals);
                }
            }
        }
    }
}

/// How fast a dying plane's boost brings it back up to its speed (metres a second
/// squared: its TnPlaneSimulation's thrust, as the demo takes it).
const BOOST_ACCEL: f32 = 30.0;

/// Runs the ragdolls (every rig's, the enemies' and the players').
pub struct RagdollPlugin;

impl Plugin for RagdollPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            drive_rigs
                .after(bevy::app::AnimationSystems)
                .before(bevy::transform::TransformSystems::Propagate),
        );
    }
}

/// The bones' world transforms from their parents' and their own.
fn world_poses(base: Affine3A, parents: &[Option<usize>], locals: &[Transform]) -> Vec<Affine3A> {
    let mut out: Vec<Affine3A> = Vec::with_capacity(locals.len());
    for (i, l) in locals.iter().enumerate() {
        let parent = parents[i].map_or(base, |p| out[p]);
        out.push(parent * l.compute_affine());
    }
    out
}

fn pose_of(a: &Affine3A) -> BonePose {
    let (_, r, t) = a.to_scale_rotation_translation();
    (t, r.normalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(asset: &mut PhysicsAsset, l: &str) {
        let f: Vec<&str> = l.split('\t').collect();
        match f[0] {
            "body" => assert!(asset.read_body(&f[1..]), "{l}"),
            "joint" => assert!(asset.read_joint(&f[1..]), "{l}"),
            _ => panic!(),
        }
    }

    /// A game ragdoll to test: its name, its physics asset, its bones (those with
    /// bodies), standing in its reference pose (its joints' frames lined up, its feet
    /// under its head where it has both) on the floor at y 0, and its size.
    struct GameRagdoll {
        name: String,
        asset: PhysicsAsset,
        names: Vec<String>,
        pose: Vec<BonePose>,
        scale: f32,
    }

    /// A joint frame (its place and twist and secondary axes in its bone's space).
    fn frame(at: Vec3, pri: Vec3, sec: Vec3) -> Affine3A {
        let x = pri.normalize_or(Vec3::X);
        let z = x.cross(sec).normalize_or(Vec3::Z);
        let y = z.cross(x);
        Affine3A::from_mat3_translation(Mat3::from_cols(x, y, z), at)
    }

    /// The ragdoll of a physics asset's lines (each its kind, then its fields from its
    /// bone on), at `scale`.
    fn game_ragdoll(name: &str, lines: &[Vec<&str>], scale: f32) -> Option<GameRagdoll> {
        let mut asset = PhysicsAsset::default();
        let mut frames = Vec::new();
        for l in lines {
            let f = &l[1..];
            match l[0] {
                "body" => assert!(asset.read_body(f), "{name}: {l:?}"),
                "mass" => assert!(asset.read_mass(f), "{name}: {l:?}"),
                "joint" => {
                    assert!(asset.read_joint(f), "{name}: {l:?}");
                    let v = |i: usize| vec3(f[i]).unwrap();
                    frames.push((f[0], f[1], frame(v(2), v(3), v(4)), frame(v(5), v(6), v(7))));
                }
                _ => {}
            }
        }
        if asset.joints.is_empty() {
            return None;
        }
        let names: Vec<String> = asset.bodies.iter().map(|b| b.bone.clone()).collect();
        let index = |n: &str| names.iter().position(|m| m == n);
        // The roots where they are; each child where its joint's frames line up.
        let mut world: Vec<Option<Affine3A>> = names
            .iter()
            .map(|n| (!frames.iter().any(|(c, ..)| c == n)).then_some(Affine3A::IDENTITY))
            .collect();
        loop {
            let mut more = false;
            for (c, p, child, parent) in &frames {
                let (Some(c), Some(p)) = (index(c), index(p)) else {
                    continue;
                };
                if let (None, Some(tp)) = (world[c], world[p]) {
                    world[c] = Some(tp * *parent * child.inverse());
                    more = true;
                }
            }
            if !more {
                break;
            }
        }
        let world: Vec<Affine3A> = world
            .into_iter()
            .map(|w| w.unwrap_or(Affine3A::IDENTITY))
            .collect();
        let mean = |parts: &[&str]| -> Option<Vec3> {
            let at: Vec<Vec3> = names
                .iter()
                .zip(&world)
                .filter(|(n, _)| parts.iter().any(|p| n.contains(p)))
                .map(|(_, w)| Vec3::from(w.translation))
                .collect();
            (!at.is_empty()).then(|| at.iter().sum::<Vec3>() / at.len() as f32)
        };
        let turn = match (mean(&["Head"]), mean(&["Ankle", "Foot", "Toe"])) {
            (Some(head), Some(feet)) => {
                Quat::from_rotation_arc((head - feet).normalize_or(Vec3::Y), Vec3::Y)
            }
            _ => Quat::IDENTITY,
        };
        let mut pose: Vec<BonePose> = world
            .iter()
            .map(|w| {
                let (_, q, t) = w.to_scale_rotation_translation();
                (turn * t * scale, (turn * q).normalize())
            })
            .collect();
        // (Its lowest contact sphere just over the floor.)
        let mut lowest = f32::MAX;
        for def in &asset.bodies {
            let (at, q) = pose[index(&def.bone).unwrap()];
            let m = Affine3A::from_scale_rotation_translation(Vec3::splat(scale), q, at);
            let mut probes = Vec::new();
            for s in &def.shapes {
                s.placed(&m).probes(&mut probes);
            }
            for (c, r) in probes {
                lowest = lowest.min(c.y - r);
            }
        }
        for p in &mut pose {
            p.0.y += 0.01 - lowest;
        }
        Some(GameRagdoll {
            name: name.to_string(),
            asset,
            names,
            pose,
            scale,
        })
    }

    /// Every enemy's ragdoll (enemies.txt's) and the players' (robot_deaths.txt's).
    fn game_ragdolls() -> Vec<GameRagdoll> {
        let dir = crate::assets_dir();
        let mut out = Vec::new();
        if let Ok(text) = std::fs::read_to_string(dir.join("models/enemies/enemies.txt")) {
            let mut kinds: Vec<(&str, f32, Vec<Vec<&str>>)> = Vec::new();
            for l in text.lines() {
                let f: Vec<&str> = l.split('\t').collect();
                match f[0] {
                    "type" if f.len() > 2 => {
                        let scale = f.last().and_then(|s| s.parse().ok()).unwrap_or(1.0);
                        kinds.push((f[1], scale, Vec::new()));
                    }
                    "body" | "mass" | "joint" if f.len() > 2 => {
                        if let Some(k) = kinds.iter_mut().find(|k| k.0 == f[1]) {
                            let mut line = vec![f[0]];
                            line.extend(&f[2..]);
                            k.2.push(line);
                        }
                    }
                    _ => {}
                }
            }
            for (id, scale, lines) in &kinds {
                out.extend(game_ragdoll(id, lines, *scale));
            }
        }
        if let Ok(text) = std::fs::read_to_string(dir.join(crate::downed::DEATHS_FILE)) {
            let lines: Vec<Vec<&str>> = text
                .lines()
                .map(|l| l.split('\t').collect::<Vec<_>>())
                .filter(|f| matches!(f[0], "body" | "mass" | "joint"))
                .collect();
            out.extend(game_ragdoll("player", &lines, 1.0));
        }
        out
    }

    #[test]
    fn the_games_ragdolls_fall_whole_and_come_to_rest() {
        let ragdolls = game_ragdolls();
        if ragdolls.is_empty() {
            eprintln!("no ragdolls built: skipped");
            return;
        }
        // Struck from in front (-Z): as the deaths kick (deaths.rs's kick, at their
        // numbers over 100: the game's units to metres).
        type KickFn = fn(&mut Ragdoll, &mut u32);
        let kicks: [(&str, KickFn); 6] = [
            ("falls", |_, _| {}),
            // Gunfire_Front_DEATH: the kill's momentum (100000 over a mass of 100).
            ("gunfire", |r, seed| {
                r.add_momentum(Vec3::Z * 10.0, (1.0, 1.0), (0.0, 0.0), seed)
            }),
            // ShotgunGibFlyBack_DEATH: the momentum thrice, and its root body
            // thrown back and up and spun.
            ("fly back", |r, seed| {
                r.add_momentum(Vec3::Z * 10.0, (3.0, 3.0), (0.0, 0.0), seed);
                r.impulse(
                    None,
                    Vec3::new(0.0, 150.0, 100.0),
                    Vec3::new(30.0, 0.0, 0.0),
                    false,
                    None,
                );
            }),
            // Explosive02_DEATH: an impulse from 4 m below (in front), 15 m round,
            // 60 to 85 (less the further), six joints let go.
            ("explosive", |r, seed| {
                let at = r.centre() + Vec3::new(0.0, -4.0, -1.0);
                r.halt();
                r.sever_random(6, None, seed);
                r.radial(at, 15.0, 72.5, true, false);
            }),
            // ShotgunGibTorso01_DEATH: cut at the waist, its root body thrown.
            ("cut in two", |r, _| {
                let lumbar = r
                    .bodies
                    .iter()
                    .map(|b| b.bone)
                    .filter(|&b| r.joints.iter().any(|j| r.bodies[j.b].bone == b))
                    .min();
                if let Some(b) = lumbar {
                    r.sever(&[b], None);
                }
                r.impulse(None, Vec3::new(0.0, 95.0, 110.0), Vec3::ZERO, false, None);
            }),
            // MeleeRagdoll_DEATH: twice the momentum, impulses its mass takes.
            ("melee", |r, seed| {
                r.add_momentum(Vec3::Z * 10.0, (2.0, 2.0), (0.0, 0.35), seed);
                r.impulse(
                    None,
                    Vec3::new(0.0, 50.0, 80.0),
                    Vec3::new(5.0, 0.0, 5.0),
                    true,
                    None,
                );
                r.impulse(
                    None,
                    Vec3::new(0.0, 75.0, 100.0),
                    Vec3::new(0.0, 15.0, 0.0),
                    true,
                    None,
                );
            }),
        ];
        let dt = 1.0 / 60.0;
        let (mut report, mut failed) = (Vec::new(), Vec::new());
        for g in &ragdolls {
            for (how, kick) in kicks {
                let mut r =
                    Ragdoll::new(&g.asset, &g.names, &g.pose, &g.pose, g.scale, dt).unwrap();
                let mut seed = 7;
                kick(&mut r, &mut seed);
                let (mut fastest, mut highest, mut rested) = (0.0f32, 0.0f32, None);
                for _ in 0..(12.0 / dt) as usize {
                    r.step(dt, &Floor(0.0));
                    for b in &r.bodies {
                        assert!(
                            b.x.is_finite()
                                && b.q.is_finite()
                                && b.v.is_finite()
                                && b.w.is_finite(),
                            "{} {how}: {b:?}",
                            g.name
                        );
                        fastest = fastest.max(b.v.length());
                        highest = highest.max(b.x.y);
                    }
                    if rested.is_none() && r.settled() {
                        rested = Some(r.age);
                    }
                }
                let lowest = r.bodies.iter().map(|b| b.x.y).fold(f32::MAX, f32::min);
                let gap = r
                    .joints
                    .iter()
                    .map(|j| {
                        let (a, b) = (&r.bodies[j.a], &r.bodies[j.b]);
                        ((a.x + a.q * j.ra) - (b.x + b.q * j.rb)).length()
                    })
                    .fold(0.0, f32::max);
                let spin = r.bodies.iter().map(|b| b.w.length()).fold(0.0, f32::max);
                report.push(format!(
                    "{:<20} {how:<10} rest {:>5} fastest {fastest:5.1} highest {highest:6.2} lowest {lowest:5.2} gap {gap:.3} spin {spin:.2}",
                    g.name,
                    rested.map_or("-".into(), |t| format!("{t:.2}")),
                ));
                // Still on the floor, held together, and at rest (by itself, not
                // only for having fallen SETTLE_SECS).
                if lowest < -0.05 * g.scale
                    || gap > 0.1 * g.scale
                    || !rested.is_some_and(|t| t < SETTLE_SECS)
                {
                    failed.push(report.last().unwrap().clone());
                }
            }
        }
        eprintln!("{}", report.join("\n"));
        assert!(failed.is_empty(), "\n{}", failed.join("\n"));
    }

    #[test]
    fn shapes_read_and_strike() {
        let mut a = PhysicsAsset::default();
        line(&mut a, "body\tHips\tsphere\t0 0 0\t0.5");
        line(&mut a, "body\tHips\tcapsule\t0 0 0\t0 2 0\t0.25");
        line(
            &mut a,
            "body\tChest\tbox\t0 0 0\t1 0 0\t0 0 1\t0 1 0\t1 0.5 0.25",
        );
        line(
            &mut a,
            "body\tJet\tconvex\t2\t-1 0 0\t1 0 0\t6\t1 0 0 1\t-1 0 0 1\t0 1 0 1\t0 -1 0 1\t0 0 1 1\t0 0 -1 1",
        );
        line(
            &mut a,
            "joint\tChest\tHips\t0 0 0\t1 0 0\t0 0 1\t0 1 0\t1 0 0\t0 0 1\t10\t-\t2",
        );
        assert_eq!(a.bodies.len(), 3);
        assert_eq!(a.bodies[0].shapes.len(), 2);
        assert_eq!(a.joints[0].swing2, None);
        assert!((a.joints[0].swing1.unwrap() - 10f32.to_radians()).abs() < 1e-6);
        // A sphere from 5 m out.
        let s = &a.bodies[0].shapes[0];
        let t = s.ray(Vec3::new(0.0, 0.0, 5.0), -Vec3::Z, 100.0).unwrap();
        assert!((t - 4.5).abs() < 1e-4);
        assert!(s.ray(Vec3::new(0.0, 0.0, 5.0), Vec3::Z, 100.0).is_none());
        // The capsule's side and its top.
        let c = &a.bodies[0].shapes[1];
        let t = c.ray(Vec3::new(3.0, 1.0, 0.0), -Vec3::X, 100.0).unwrap();
        assert!((t - 2.75).abs() < 1e-4);
        let t = c.ray(Vec3::new(0.0, 5.0, 0.0), -Vec3::Y, 100.0).unwrap();
        assert!((t - 2.75).abs() < 1e-4);
        assert!((c.reach(Vec3::new(1.0, 1.0, 0.0)) - 0.75).abs() < 1e-4);
        // The box: its x half size 1, its second axis (the world's z) 0.5.
        let b = &a.bodies[1].shapes[0];
        let t = b.ray(Vec3::new(0.0, 0.0, 3.0), -Vec3::Z, 100.0).unwrap();
        assert!((t - 2.5).abs() < 1e-4);
        assert!((b.reach(Vec3::new(0.0, 1.25, 0.0)) - 1.0).abs() < 1e-4);
        // The hull: a cube of 1 each way.
        let h = &a.bodies[2].shapes[0];
        let t = h.ray(Vec3::new(0.0, 4.0, 0.0), -Vec3::Y, 100.0).unwrap();
        assert!((t - 3.0).abs() < 1e-4);
        assert!(h.ray(Vec3::new(3.0, 4.0, 0.0), -Vec3::Y, 100.0).is_none());
        assert_eq!(h.reach(Vec3::ZERO), 0.0);
        // Placed by a bone: turned, moved, at twice the size.
        let m = Affine3A::from_scale_rotation_translation(
            Vec3::splat(2.0),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            Vec3::new(10.0, 0.0, 0.0),
        );
        match c.placed(&m) {
            Shape::Capsule { a, b, r } => {
                assert!(a.distance(Vec3::new(10.0, 0.0, 0.0)) < 1e-4);
                assert!(b.distance(Vec3::new(6.0, 0.0, 0.0)) < 1e-4);
                assert!((r - 0.5).abs() < 1e-6);
            }
            _ => panic!(),
        }
        match h.placed(&m) {
            Shape::Convex { planes, .. } => {
                // (Its +x face now faces +y, 2 m out from (10, 0, 0).)
                let (n, d) = planes[0];
                assert!(n.distance(Vec3::Y) < 1e-4, "{n}");
                assert!((d - 2.0).abs() < 1e-4, "{d}");
            }
            _ => panic!(),
        }
    }

    /// A straight-legged two-body chain: a body on a bone and one on its child.
    fn chain() -> (PhysicsAsset, Vec<String>) {
        let mut a = PhysicsAsset::default();
        line(&mut a, "body\tUpper\tcapsule\t0 0 0\t1 0 0\t0.2");
        line(&mut a, "body\tLower\tcapsule\t0 0 0\t1 0 0\t0.2");
        line(
            &mut a,
            "joint\tLower\tUpper\t0 0 0\t1 0 0\t0 1 0\t1 0 0\t1 0 0\t0 1 0\t-\t-\t-",
        );
        (a, vec!["Upper".into(), "Lower".into()])
    }

    /// An impulse its mass takes at a point (UsePointImpulse) turns its body by the
    /// point's reach from its centre of mass (r x J over its inertia) as well as
    /// moving it; at its centre of mass, or as a change in speed, it only moves it.
    #[test]
    fn an_impulse_at_a_point_turns_its_body() {
        let mut a = PhysicsAsset::default();
        line(&mut a, "body\tBall\tsphere\t0 0 0\t0.5");
        let names = vec!["Ball".to_string()];
        let pose = [(Vec3::new(0.0, 3.0, 0.0), Quat::IDENTITY)];
        let fresh = || Ragdoll::new(&a, &names, &pose, &pose, 1.0, 1.0 / 60.0).unwrap();
        let j = Vec3::new(0.0, 0.0, 0.5);
        let mut r = fresh();
        r.impulse(None, j, Vec3::ZERO, true, Some(Vec3::new(0.0, 3.5, 0.0)));
        let b = &r.bodies[0];
        assert!((b.v - j * b.inv_mass).length() < 1e-5, "{}", b.v);
        let want = Vec3::new(0.5 * 0.5, 0.0, 0.0) * b.inv_inertia;
        assert!((b.w - want).length() < 1e-4, "{} against {want}", b.w);
        let mut r = fresh();
        r.impulse(None, j, Vec3::ZERO, true, None);
        assert_eq!(r.bodies[0].w, Vec3::ZERO);
        let mut r = fresh();
        r.impulse(None, j, Vec3::ZERO, false, Some(Vec3::new(0.0, 3.5, 0.0)));
        assert_eq!(r.bodies[0].w, Vec3::ZERO);
        assert!((r.bodies[0].v - j).length() < 1e-6);
    }

    #[test]
    fn a_ball_comes_to_rest_on_the_floor() {
        let mut a = PhysicsAsset::default();
        line(&mut a, "body\tBall\tsphere\t0 0 0\t0.5");
        let names = vec!["Ball".to_string()];
        let pose = [(Vec3::new(0.0, 3.0, 0.0), Quat::IDENTITY)];
        let mut r = Ragdoll::new(&a, &names, &pose, &pose, 1.0, 1.0 / 60.0).unwrap();
        for _ in 0..300 {
            r.step(1.0 / 60.0, &Floor(0.0));
        }
        let (_, at, _) = r.bone_poses().next().unwrap();
        assert!((at.y - 0.5).abs() < 0.02, "{at}");
        assert!(r.settled());
    }

    #[test]
    fn a_chain_holds_together_as_it_falls_and_lands() {
        let (a, names) = chain();
        let pose = [
            (Vec3::new(0.0, 4.0, 0.0), Quat::IDENTITY),
            (Vec3::new(1.0, 4.0, 0.0), Quat::from_rotation_z(-0.5)),
        ];
        let mut r = Ragdoll::new(&a, &names, &pose, &pose, 1.0, 1.0 / 60.0).unwrap();
        r.push(Vec3::new(1.5, 4.0, 0.0), Vec3::new(0.0, 0.0, 4.0));
        for _ in 0..600 {
            r.step(1.0 / 60.0, &Floor(0.0));
            let poses: Vec<_> = r.bone_poses().collect();
            // (The lower's bone stays at the upper's end.)
            let end = poses[0].1 + poses[0].2 * Vec3::X;
            assert!(end.distance(poses[1].1) < 0.05, "{end} {}", poses[1].1);
        }
        for (_, at, q) in r.bone_poses() {
            // Lying on the floor: both ends at its radius above it.
            for p in [at, at + q * Vec3::X] {
                assert!(p.y > 0.1 && p.y < 0.35, "{p}");
            }
        }
        assert!(r.settled());
    }

    #[test]
    fn a_joint_keeps_to_its_swing_limit() {
        let mut a = PhysicsAsset::default();
        line(&mut a, "body\tUpper\tsphere\t0.5 0 0\t0.2");
        line(&mut a, "body\tLower\tsphere\t0.5 0 0\t0.2");
        line(
            &mut a,
            "joint\tLower\tUpper\t0 0 0\t1 0 0\t0 1 0\t1 0 0\t1 0 0\t0 1 0\t20\t20\t5",
        );
        let names = vec!["Upper".to_string(), "Lower".to_string()];
        // The child bent 60 degrees, the parent held (no gravity: high in the air
        // for a moment, both falling alike).
        let pose = [
            (Vec3::new(0.0, 50.0, 0.0), Quat::IDENTITY),
            (
                Vec3::new(1.0, 50.0, 0.0),
                Quat::from_rotation_z(60f32.to_radians()),
            ),
        ];
        let mut r = Ragdoll::new(&a, &names, &pose, &pose, 1.0, 1.0 / 60.0).unwrap();
        for _ in 0..30 {
            r.step(1.0 / 60.0, &Floor(-1000.0));
        }
        let q: Vec<Quat> = r.bone_poses().map(|(_, _, q)| q).collect();
        let bend = (q[0] * Vec3::X).angle_between(q[1] * Vec3::X).to_degrees();
        assert!(bend < 22.0, "bent {bend}");
    }

    #[test]
    fn rises_out_of_the_floor_rather_than_being_thrown() {
        // A ball sunk most of the way into the floor (as a robot's feet start in it).
        let mut a = PhysicsAsset::default();
        line(&mut a, "body\tBall\tsphere\t0 0 0\t0.5");
        let names = vec!["Ball".to_string()];
        let pose = [(Vec3::new(0.0, 0.05, 0.0), Quat::IDENTITY)];
        let mut r = Ragdoll::new(&a, &names, &pose, &pose, 1.0, 1.0 / 60.0).unwrap();
        let mut top = 0.0f32;
        for _ in 0..120 {
            r.step(1.0 / 60.0, &Floor(0.0));
            top = top.max(r.bone_poses().next().unwrap().1.y);
        }
        let (_, at, _) = r.bone_poses().next().unwrap();
        assert!((at.y - 0.5).abs() < 0.02, "{at}");
        assert!(top < 0.6, "thrown up to {top}");
    }

    #[test]
    fn a_joint_started_past_its_limits_is_eased_back() {
        // As a_joint_keeps_to_its_swing_limit's: bent 60 degrees, 40 past its limit,
        // brought back no faster than GRACE_RATE (not thrown back at once).
        let mut a = PhysicsAsset::default();
        line(&mut a, "body\tUpper\tsphere\t0.5 0 0\t0.2");
        line(&mut a, "body\tLower\tsphere\t0.5 0 0\t0.2");
        line(
            &mut a,
            "joint\tLower\tUpper\t0 0 0\t1 0 0\t0 1 0\t1 0 0\t1 0 0\t0 1 0\t20\t20\t5",
        );
        let names = vec!["Upper".to_string(), "Lower".to_string()];
        let pose = [
            (Vec3::new(0.0, 50.0, 0.0), Quat::IDENTITY),
            (
                Vec3::new(1.0, 50.0, 0.0),
                Quat::from_rotation_z(60f32.to_radians()),
            ),
        ];
        let mut r = Ragdoll::new(&a, &names, &pose, &pose, 1.0, 1.0 / 60.0).unwrap();
        let mut fastest = 0.0f32;
        for _ in 0..60 {
            r.step(1.0 / 60.0, &Floor(-1000.0));
            fastest = fastest.max(r.bodies.iter().map(|b| b.w.length()).fold(0.0, f32::max));
        }
        assert!(fastest < GRACE_RATE * 2.5, "spun at {fastest}");
        let q: Vec<Quat> = r.bone_poses().map(|(_, _, q)| q).collect();
        let bend = (q[0] * Vec3::X).angle_between(q[1] * Vec3::X).to_degrees();
        assert!(bend < 22.0, "bent {bend}");
    }

    #[test]
    fn the_nearest_point_on_an_ellipse() {
        let near = |u: f32, v: f32, a: f32, b: f32| nearest_on_ellipse(u, v, a, b);
        let (x, y) = near(3.0, 0.0, 2.0, 1.0);
        assert!((x - 2.0).abs() < 1e-4 && y.abs() < 1e-4);
        let (x, y) = near(0.0, -3.0, 2.0, 1.0);
        assert!(x.abs() < 1e-4 && (y + 1.0).abs() < 1e-4);
        // A hinge's: all but shut one way (half a degree), open 45 degrees the
        // other; a swing a little off the open way is brought to it, not pulled
        // back along the open way.
        let (x, y) = near(0.02, 0.7, 0.0087, 0.785);
        assert!(((x / 0.0087).powi(2) + (y / 0.785).powi(2) - 1.0).abs() < 1e-3);
        assert!((y - 0.7).abs() < 0.005 && x > 0.0 && x < 0.0087, "{x} {y}");
    }

    #[test]
    fn lands_on_triangles() {
        struct Tris(Vec<[Vec3; 3]>);
        impl World for Tris {
            fn triangles(&self, _: Vec3, _: Vec3, out: &mut Vec<[Vec3; 3]>) {
                out.extend(self.0.iter().copied());
            }
            fn floor(&self) -> Option<f32> {
                None
            }
        }
        // A floor of two triangles at 1 m, tilted a little.
        let world = Tris(vec![
            [
                Vec3::new(-10.0, 1.0, -10.0),
                Vec3::new(10.0, 1.0, -10.0),
                Vec3::new(10.0, 1.5, 10.0),
            ],
            [
                Vec3::new(-10.0, 1.0, -10.0),
                Vec3::new(10.0, 1.5, 10.0),
                Vec3::new(-10.0, 1.5, 10.0),
            ],
        ]);
        let (a, names) = chain();
        let pose = [
            (Vec3::new(0.0, 4.0, 0.0), Quat::IDENTITY),
            (Vec3::new(1.0, 4.0, 0.0), Quat::IDENTITY),
        ];
        let mut r = Ragdoll::new(&a, &names, &pose, &pose, 1.0, 1.0 / 60.0).unwrap();
        for _ in 0..600 {
            r.step(1.0 / 60.0, &world);
        }
        for (_, at, _) in r.bone_poses() {
            let ground = 1.0 + (at.z + 10.0) / 20.0 * 0.5;
            assert!(
                at.y > ground + 0.1 && at.y < ground + 0.35,
                "{at} over {ground}"
            );
        }
    }
}
