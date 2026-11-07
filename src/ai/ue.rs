//! Unreal's space and Core.Math, as the AI's script uses them: positions in Unreal
//! units (centimetres) with z up, rotations in Unreal's units (65536 a turn).
//!
//! The demo's world is metres with y up: (ue x, ue y, ue z) is (x, z, y) x 0.01
//! there, and a heading (the demo's yaw about its up axis, from -z) is
//! atan2(-cos t, -sin t) for an Unreal yaw of t radians.

use bevy::math::{Mat3, Quat, Vec2, Vec3};

/// Unreal units in a metre.
pub const UU_PER_M: f32 = 100.0;
/// Unreal rotation units per degree (182.044449, as the script has it).
pub const ROT_PER_DEG: f32 = 182.044_45;
/// Degrees to radians, as the script has it (0.017453).
pub const DEG_TO_RAD: f32 = 0.017_453;

/// A demo position (metres, y up) in Unreal space.
pub fn to_ue(p: Vec3) -> Vec3 {
    Vec3::new(p.x, p.z, p.y) * UU_PER_M
}

/// An Unreal position in the demo's space.
pub fn from_ue(p: Vec3) -> Vec3 {
    Vec3::new(p.x, p.z, p.y) / UU_PER_M
}

/// A direction (no scaling).
pub fn dir_to_ue(d: Vec3) -> Vec3 {
    Vec3::new(d.x, d.z, d.y)
}

pub fn dir_from_ue(d: Vec3) -> Vec3 {
    Vec3::new(d.x, d.z, d.y)
}

/// An Unreal yaw (units) as the demo's heading (radians).
pub fn heading(yaw: f32) -> f32 {
    let t = yaw * std::f32::consts::TAU / 65536.0;
    (-t.cos()).atan2(-t.sin())
}

/// The demo's heading as an Unreal yaw (units, -32768..32768).
pub fn yaw_of_heading(h: f32) -> f32 {
    // heading = atan2(-cos t, -sin t): the facing (-sin h, -cos h) is (cos t, sin t).
    let t = (-h.cos()).atan2(-h.sin());
    t * 65536.0 / std::f32::consts::TAU
}

/// A rotator's yaw from a vector (cast50: Vector -> Rotator).
pub fn yaw_of(v: Vec3) -> f32 {
    if v.x == 0.0 && v.y == 0.0 {
        return 0.0;
    }
    v.y.atan2(v.x) * 65536.0 / std::f32::consts::TAU
}

/// A rotator's pitch from a vector.
pub fn pitch_of(v: Vec3) -> f32 {
    let h = (v.x * v.x + v.y * v.y).sqrt();
    v.z.atan2(h) * 65536.0 / std::f32::consts::TAU
}

/// A rotator (pitch, yaw) as a unit vector (cast39: Rotator -> Vector).
pub fn vec_of(pitch: f32, yaw: f32) -> Vec3 {
    let p = pitch * std::f32::consts::TAU / 65536.0;
    let y = yaw * std::f32::consts::TAU / 65536.0;
    Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), p.sin())
}

/// A rotator's axes (pitch, yaw and roll in rotation units): FRotationMatrix's X,
/// Y and Z.
pub fn rotator_axes(pitch: f32, yaw: f32, roll: f32) -> [Vec3; 3] {
    let k = std::f32::consts::TAU / 65536.0;
    let (sp, cp) = (pitch * k).sin_cos();
    let (sy, cy) = (yaw * k).sin_cos();
    let (sr, cr) = (roll * k).sin_cos();
    [
        Vec3::new(cp * cy, cp * sy, sp),
        Vec3::new(sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, -sr * cp),
        Vec3::new(-(cr * sp * cy + sr * sy), cy * sr - cr * sp * sy, cr * cp),
    ]
}

/// A rotator (pitch, yaw, roll: rotation units) from its axes (FMatrix.Rotator: the
/// pitch and yaw of x, then the roll that turns that rotator's y onto y and z).
pub fn rotator_from_axes(x: Vec3, y: Vec3, z: Vec3) -> Vec3 {
    let k = 32768.0 / std::f32::consts::PI;
    let pitch = x.z.atan2((x.x * x.x + x.y * x.y).sqrt()) * k;
    let yaw = x.y.atan2(x.x) * k;
    let sy = rotator_axes(pitch, yaw, 0.0)[1];
    let roll = z.dot(sy).atan2(y.dot(sy)) * k;
    Vec3::new(pitch, yaw, roll)
}

/// Core.Math.OrthoRotation: the rotator of the axes x, y, z.
pub fn ortho_rotation(x: Vec3, y: Vec3, z: Vec3) -> Vec3 {
    rotator_from_axes(x, y, z)
}

/// Core.Math.RLerp with bShortestPath: each of a rotator's (pitch, yaw, roll) the
/// shorter way round from `a` to `b`.
pub fn rlerp(a: Vec3, b: Vec3, alpha: f32) -> Vec3 {
    let d = Vec3::new(
        wrap_rot(b.x - a.x),
        wrap_rot(b.y - a.y),
        wrap_rot(b.z - a.z),
    );
    a + d * alpha
}

/// Core.Math.QuatFromRotator (in Unreal's axes: the rotation taking x, y, z to the
/// rotator's axes).
pub fn quat_from_rotator(r: Vec3) -> Quat {
    let [x, y, z] = rotator_axes(r.x, r.y, r.z);
    Quat::from_mat3(&Mat3::from_cols(x, y, z)).normalize()
}

/// Core.Math.QuatToRotator.
pub fn quat_to_rotator(q: Quat) -> Vec3 {
    rotator_from_axes(q * Vec3::X, q * Vec3::Y, q * Vec3::Z)
}

/// Core.Math.QuatSlerp, the shorter way round.
pub fn quat_slerp(a: Quat, b: Quat, alpha: f32) -> Quat {
    let b = if a.dot(b) < 0.0 { -b } else { b };
    a.slerp(b, alpha.clamp(0.0, 1.0))
}

/// A vector turned by a yaw (units) about the up axis.
pub fn rotate_yaw(v: Vec3, yaw: f32) -> Vec3 {
    let (s, c) = (yaw * std::f32::consts::TAU / 65536.0).sin_cos();
    Vec3::new(v.x * c - v.y * s, v.x * s + v.y * c, v.z)
}

/// A yaw's forward vector in the ground plane.
pub fn yaw_vec(yaw: f32) -> Vec3 {
    vec_of(0.0, yaw)
}

/// An angle in rotation units wrapped to -32768..32768.
pub fn wrap_rot(a: f32) -> f32 {
    let mut a = a % 65536.0;
    if a > 32768.0 {
        a -= 65536.0;
    } else if a < -32768.0 {
        a += 65536.0;
    }
    a
}

// Core.Math in Unreal's axes (2-D is x, y).

pub fn vsize2d(v: Vec3) -> f32 {
    (v.x * v.x + v.y * v.y).sqrt()
}

pub fn vsize_sq2d(v: Vec3) -> f32 {
    v.x * v.x + v.y * v.y
}

pub fn dot2d(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y
}

/// Core.Math.Normal (zero stays zero).
pub fn normal(v: Vec3) -> Vec3 {
    v.normalize_or_zero()
}

/// Core.Math.Normal2D.
pub fn normal2d(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, 0.0).normalize_or_zero()
}

/// Core.Math.NormalizeDot2D.
pub fn normalize_dot2d(a: Vec3, b: Vec3) -> f32 {
    dot2d(normal2d(a), normal2d(b))
}

/// Core.Math.FPctByRange and FClampPctByRange.
pub fn pct_by_range(v: f32, lo: f32, hi: f32) -> f32 {
    (v - lo) / (hi - lo)
}

pub fn clamp_pct_by_range(v: f32, lo: f32, hi: f32) -> f32 {
    pct_by_range(v, lo, hi).clamp(0.0, 1.0)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Core.Math.RadianAngleBetweenUnitVectors.
pub fn radian_angle_between_unit(a: Vec3, b: Vec3) -> f32 {
    a.dot(b).clamp(-1.0, 1.0).acos()
}

/// AI.CalculateRangePointToPoint: the distance between two cylinders' surfaces
/// (their radii off the ground-plane distance, their half heights off the height).
pub fn range_point_to_point(
    from: Vec3,
    from_radius: f32,
    to: Vec3,
    to_radius: f32,
    from_height: f32,
    to_height: f32,
) -> f32 {
    let dxy = vsize2d(from - to);
    let dz = (from.z - to.z).abs();
    let xy = (dxy - from_radius - to_radius).max(0.0);
    let z = (dz - from_height - to_height).max(0.0);
    (xy * xy + z * z).sqrt()
}

/// AI.Calculate2DRangePointToPoint.
pub fn range_2d(from: Vec3, from_radius: f32, to: Vec3, to_radius: f32) -> f32 {
    range_point_to_point(
        Vec3::new(from.x, from.y, 0.0),
        from_radius,
        Vec3::new(to.x, to.y, 0.0),
        to_radius,
        0.0,
        0.0,
    )
}

/// Core.Math.CalculateJumpVelocityFromPeakJumpHeight: the launch velocity of a
/// jump peaking `peak` over the higher of its ends, its time in the air and its
/// speed across.
pub fn jump_velocity(
    launch: Vec3,
    land: Vec3,
    peak: f32,
    gravity_z: f32,
) -> Option<(Vec3, f32, f32)> {
    if peak <= 0.0 {
        return None;
    }
    let d = land - launch;
    let mut peak_height = peak;
    if d.z > 0.0 {
        peak_height += d.z;
    }
    let g = gravity_z.abs();
    let vz = (peak_height * 2.0 * g).sqrt();
    if vz <= 0.0 {
        return None;
    }
    let vog = vz / g;
    let disc = (vog * vog - 2.0 * d.z / g).max(0.0).sqrt();
    let t = vog + disc;
    if t <= 0.0 {
        return None;
    }
    let horizontal = vsize2d(d) / t;
    let dir = normal2d(d);
    Some((dir * horizontal + Vec3::Z * vz, t, horizontal))
}

/// Where along the segment from `a` to `b` (0 to 1) it first enters the upright
/// cylinder of `radius` and half height `half` round `c`, if it does.
pub fn segment_cylinder(a: Vec3, b: Vec3, c: Vec3, radius: f32, half: f32) -> Option<f32> {
    let d = b - a;
    let f = a - c;
    // Across: |f + t d| within the radius, in the ground plane.
    let qa = d.x * d.x + d.y * d.y;
    let qb = 2.0 * (f.x * d.x + f.y * d.y);
    let qc = f.x * f.x + f.y * f.y - radius * radius;
    let (mut lo, mut hi) = if qa < 1e-6 {
        if qc > 0.0 {
            return None;
        }
        (0.0f32, 1.0f32)
    } else {
        let disc = qb * qb - 4.0 * qa * qc;
        if disc < 0.0 {
            return None;
        }
        let r = disc.sqrt();
        ((-qb - r) / (2.0 * qa), (-qb + r) / (2.0 * qa))
    };
    // Up: within its half height.
    if d.z.abs() < 1e-6 {
        if f.z.abs() > half {
            return None;
        }
    } else {
        let t0 = (-half - f.z) / d.z;
        let t1 = (half - f.z) / d.z;
        lo = lo.max(t0.min(t1));
        hi = hi.min(t0.max(t1));
    }
    lo = lo.max(0.0);
    hi = hi.min(1.0);
    (lo <= hi).then_some(lo)
}

/// Core.Math.VInterpTo: `current` moved towards `target` by `speed` times the
/// way left each second.
pub fn vinterp_to(current: Vec3, target: Vec3, dt: f32, speed: f32) -> Vec3 {
    if speed <= 0.0 {
        return target;
    }
    let d = target - current;
    if d.length_squared() < 1e-8 {
        return target;
    }
    current + d * (dt * speed).clamp(0.0, 1.0)
}

/// Core.Math.FInterpTo: `current` moved towards `target` by `speed` times the way
/// left each second.
pub fn finterp_to(current: f32, target: f32, dt: f32, speed: f32) -> f32 {
    if speed <= 0.0 {
        return target;
    }
    let d = target - current;
    if d * d < 1e-8 {
        return target;
    }
    current + d * (dt * speed).clamp(0.0, 1.0)
}

/// Core.Math.GetAngularDistance: the azimuth (x: from AxisX round towards AxisY,
/// measured from AxisX's line whichever way the direction faces along it, so
/// within a quarter turn either side) and the elevation (y: towards AxisZ) of a
/// direction in the frame of the axes, radians.
pub fn angular_distance(dir: Vec3, x: Vec3, y: Vec3, z: Vec3) -> Vec2 {
    // GetDotDistance.
    let n = dir.normalize_or_zero();
    let no_z = (n - z * n.dot(z)).normalize_or_zero();
    let sign = if no_z.dot(y) < 0.0 { -1.0 } else { 1.0 };
    let dot = Vec2::new(sign * no_z.dot(x).abs(), n.dot(z));
    // GetAngularFromDotDist.
    let sign = if dot.x < 0.0 { -1.0 } else { 1.0 };
    Vec2::new(
        dot.x.abs().clamp(-1.0, 1.0).acos() * sign,
        std::f32::consts::FRAC_PI_2 - dot.y.clamp(-1.0, 1.0).acos(),
    )
}

/// AI.RandomizeFloatValue: `v` times a random factor within `fraction` of 1.
pub fn randomize(v: f32, fraction: f32, rng: &mut Rng) -> f32 {
    v * rng.range(1.0 - fraction, 1.0 + fraction)
}

/// A small deterministic random number generator (Core.Random's RandRange...).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u32(&mut self) -> u32 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    /// FRand: 0..1.
    pub fn frand(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// RandRangeFloat.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.frand()
    }

    /// Rand(n): 0..n-1.
    pub fn rand(&mut self, n: i32) -> i32 {
        if n <= 0 {
            return 0;
        }
        (self.next_u32() % n as u32) as i32
    }

    /// Core.Random.RandomIntInRange: lo..=hi.
    pub fn int_in_range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.rand(hi - lo + 1)
    }

    /// Core.Random.RandomUnitVector (VRand): a way picked evenly over the sphere.
    pub fn unit_vector(&mut self) -> Vec3 {
        let z = self.range(-1.0, 1.0);
        let theta = self.range(0.0, std::f32::consts::TAU);
        let r = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(r * theta.cos(), r * theta.sin(), z)
    }

    /// Core.Random.RandomVectorInCone (VRandCone): a way within `half_angle`
    /// (radians) of `dir` (its angle off it the sphere's even pick's, wrapped into
    /// the cone; turned round it by a random angle).
    pub fn vector_in_cone(&mut self, dir: Vec3, half_angle: f32) -> Vec3 {
        let dir = normal(dir);
        if half_angle <= 0.0 {
            return dir;
        }
        let theta = std::f32::consts::TAU * self.frand();
        let phi = (2.0 * self.frand() - 1.0).clamp(-1.0, 1.0).acos() % half_angle;
        let [x, y, _] = rotator_axes(pitch_of(dir), yaw_of(dir), 0.0);
        let r = Quat::from_axis_angle(y, phi) * dir;
        normal(Quat::from_axis_angle(x, theta) * r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces_round_trip() {
        let p = Vec3::new(1.5, 2.0, -3.0);
        assert!((from_ue(to_ue(p)) - p).length() < 1e-5);
        for yaw in [0.0, 16384.0, -16384.0, 32000.0, 5000.0] {
            let back = yaw_of_heading(heading(yaw));
            assert!(wrap_rot(back - yaw).abs() < 0.5, "{yaw} -> {back}");
        }
        // A yaw's facing in the demo is where the model looks (enemies.rs's heading).
        let yaw = 10000.0;
        let h = heading(yaw);
        let facing_demo = Vec3::new(-h.sin(), 0.0, -h.cos());
        let facing_ue = dir_from_ue(yaw_vec(yaw));
        assert!((facing_demo - facing_ue).length() < 1e-4);
    }

    #[test]
    fn rotators_round_trip() {
        for r in [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4000.0, 12000.0, -3000.0),
            Vec3::new(-16000.0, -20000.0, 9000.0),
            Vec3::new(16384.0 * 0.9, 30000.0, 0.0),
        ] {
            let [x, y, z] = rotator_axes(r.x, r.y, r.z);
            let back = ortho_rotation(x, y, z);
            let d = Vec3::new(
                wrap_rot(back.x - r.x),
                wrap_rot(back.y - r.y),
                wrap_rot(back.z - r.z),
            );
            assert!(d.abs().max_element() < 1.0, "{r} -> {back}");
            let q = quat_to_rotator(quat_from_rotator(r));
            let d = Vec3::new(
                wrap_rot(q.x - r.x),
                wrap_rot(q.y - r.y),
                wrap_rot(q.z - r.z),
            );
            assert!(d.abs().max_element() < 1.0, "{r} -> quat -> {q}");
        }
        // A wall's rotator: its z the wall's normal.
        let n = Vec3::new(0.0, -1.0, 0.0);
        let fwd = Vec3::new(0.0, 0.0, -1.0);
        let right = n.cross(fwd).normalize();
        let fwd = right.cross(n).normalize();
        let r = ortho_rotation(fwd, right, n);
        let [_, _, z] = rotator_axes(r.x, r.y, r.z);
        assert!((z - n).length() < 1e-3, "{z}");
        assert_eq!(
            rlerp(
                Vec3::new(0.0, 30000.0, 0.0),
                Vec3::new(0.0, -30000.0, 0.0),
                0.5
            )
            .y,
            32768.0
        );
    }

    #[test]
    fn jumps_land() {
        let (v, t, _) =
            jump_velocity(Vec3::ZERO, Vec3::new(1000.0, 0.0, 300.0), 150.0, -2500.0).unwrap();
        let land = v * t + Vec3::new(0.0, 0.0, -2500.0) * 0.5 * t * t;
        assert!(
            (land - Vec3::new(1000.0, 0.0, 300.0)).length() < 1.0,
            "{land}"
        );
    }
}
