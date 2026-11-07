//! The robot's over-the-shoulder camera sprinting, as the game's: its class's camera
//! set's over-the-shoulder strategy (character::Camera's, Car_MP_, Truck_ and
//! Plane_OverTheShoulder_STRATEGY) has entries for the sprint's camera state
//! (TnPCS_Sprinting, on while the robot sprints) that its behaviors ease into and
//! back out of (character::SprintCamera):
//!
//! - The view widens from 80 to 90 degrees over the state's SmoothTime (0.4 s;
//!   main.rs's camera_follow, on the camera's smoother as fine aim's).
//! - The camera comes in closer and lower, further out to the side (the Infiltrator's
//!   550 out, aimed 100 ahead, 180 to the side and 100 up), the orbit over the
//!   location behavior's PCSTransitionInTime and back over its -OutTime (0.5 s),
//!   the aim over the state's curve's SmoothTime (0.5 s) and back over the default
//!   curve's.
//! - The view is kept within 25 degrees of level (the state's look speed's
//!   PitchRange, which, like its look speeds, blends in over the orbit rotation's
//!   SpeedTransitionTime: 0.5 s); the stick turns it more slowly (pad.rs).
//! - It wiggles: a little turn in yaw, pitch and roll heading for a new spot at
//!   random within 0.6 degrees, 5, 12 and 8 times a second (the wiggler's profile
//!   for the state, eased into over its TransitionTime, 0.3 s), as big as the
//!   robot's speed allows (full at FullWiggleSpeedThreshold); the rest of the time
//!   its default profile's 0.1 degrees, 8, 18 and 12 times a second.
//!
//! Spinning (the Whirlwind: its MeleePCS, TnPCS_Whirlwind), the Titan's camera goes
//! out from 875 to 1175 over the location behavior's 0.5 s (and back over 0.55 s)
//! and widens to 90 degrees over 0.5 s, aimed as ever.

use super::*;
use crate::character::Camera;
use crate::vehicle_cam::Wiggler;

/// The orbit rotation's SpeedTransitionTime: a new look speed (its pitch range with
/// it) blends in over this long (UpdateCurrentLookSpeed, as pad.rs's StickLook).
const SPEED_TRANSITION_SECS: f32 = 0.5;
/// The wiggler's random numbers' seed (vehicle_cam's strategies take 1 and 2).
const WIGGLE_SEED: u32 = 3;

/// The robot camera's sprint state as it eases (on OrbitCamera): the orbit's and the
/// aim's change from the camera's usual (with their speeds), the view's pitch
/// range as it blends (degrees) and the wiggle.
#[derive(Clone, Copy, Debug, Default)]
pub struct RobotCam {
    distance: (f32, f32),
    offset: [(f32, f32); 3],
    range: Option<(f32, f32)>,
    range_goal: (f32, f32),
    range_secs: f32,
    wiggler: Wiggler,
}

/// The robot camera this frame, the sprint's camera state eased in or out: where it
/// aims (ahead, to the side and up from his middle: Camera::offset's at the view's
/// pitch, or the sprint's), how far out it orbits (metres), and the wiggle's turn of
/// the view (pitch, yaw, roll; degrees, as main.rs's shot takes it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    pub aim: Vec3,
    pub distance: f32,
    pub wiggle: Vec3,
}

impl RobotCam {
    /// The view's pitch range this frame (radians): its `usual` one (radians), or
    /// `sprinting` the sprint's, blending over SPEED_TRANSITION_SECS into the one
    /// the camera's state has.
    pub fn pitch_range(
        &mut self,
        camera: &Camera,
        usual: (f32, f32),
        sprinting: bool,
        dt: f32,
    ) -> (f32, f32) {
        let goal = if sprinting {
            camera.sprint.pitch_range
        } else {
            (usual.0.to_degrees(), usual.1.to_degrees())
        };
        if goal != self.range_goal {
            self.range_goal = goal;
            self.range_secs = 0.0;
        }
        self.range_secs += dt;
        let k = (self.range_secs / SPEED_TRANSITION_SECS).min(1.0);
        let range = self.range.get_or_insert(goal);
        range.0 += (goal.0 - range.0) * k;
        range.1 += (goal.1 - range.1) * k;
        (range.0.to_radians(), range.1.to_radians())
    }

    /// The frame `dt` on: the aim and orbit eased toward the sprint's (`sprinting`)
    /// or back to the camera's usual at the view's `pitch` (radians), the orbit
    /// out to the Whirlwind's while he spins (`whirling`, the sprint's first: its
    /// state's entries come first), and the wiggle at the robot's `speed` (m/s).
    pub fn frame(
        &mut self,
        camera: &Camera,
        (sprinting, whirling): (bool, bool),
        pitch: f32,
        speed: f32,
        dt: f32,
    ) -> Frame {
        let s = &camera.sprint;
        let (distance, distance_secs, offset, offset_secs, profile) = if sprinting {
            (
                s.distance - camera.distance,
                s.distance_secs.0,
                s.offset - camera.offset(pitch),
                s.offset_secs.0,
                s.wiggle,
            )
        } else {
            let (distance, secs) = match s.whirl {
                Some((whirl, _)) if whirling => (whirl - camera.distance, s.distance_secs.0),
                _ => (0.0, s.distance_secs.1),
            };
            (distance, secs, Vec3::ZERO, s.offset_secs.1, s.calm)
        };
        ease(&mut self.distance, distance, distance_secs, dt);
        for (o, target) in self.offset.iter_mut().zip(offset.to_array()) {
            ease(o, target, offset_secs, dt);
        }
        // (Yaw, pitch and roll, by the robot's speed: UpdateScaleAdjust.)
        let [yaw, pitch_turn, roll] = self.wiggler.step(profile, WIGGLE_SEED, dt);
        let size = if s.full_wiggle_speed > 0.0 {
            (speed / s.full_wiggle_speed).min(1.0)
        } else {
            1.0
        };
        Frame {
            aim: camera.offset(pitch) + Vec3::from_array(self.offset.map(|(x, _)| x)),
            distance: camera.distance + self.distance.0,
            wiggle: Vec3::new(pitch_turn, yaw, roll) * size,
        }
    }
}

/// Eases `x` (with its speed) to `target` over about `secs` (the game's HmC2Smoother:
/// smooth_damp with half its SmoothTime, as vehicle_cam's); at once for none.
fn ease(x: &mut (f32, f32), target: f32, secs: f32, dt: f32) {
    if secs <= 0.0 {
        *x = (target, 0.0);
    } else {
        smooth_damp(&mut x.0, &mut x.1, target, secs * 0.5, dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::SprintCamera;

    /// Sprinting, the Infiltrator's camera comes to the sprint's place (550 out, aimed
    /// 100 ahead, 180 to the side and 100 up) whatever the view's pitch, nearly all
    /// the way within a second, and goes back to its usual as the sprint ends.
    #[test]
    fn the_camera_comes_to_the_sprints_place_and_back() {
        let camera = Camera::JAZZ;
        let mut cam = RobotCam::default();
        let dt = 1.0 / 60.0;
        let pitch = -0.2;
        let mut f = Frame::default();
        for _ in 0..60 {
            f = cam.frame(&camera, (true, false), pitch, 20.0, dt);
        }
        assert!(
            (f.aim - Vec3::new(1.0, 1.8, 1.0)).length() < 0.02,
            "{}",
            f.aim
        );
        assert!((f.distance - 5.5).abs() < 0.01);
        for _ in 0..90 {
            f = cam.frame(&camera, (false, false), pitch, 0.0, dt);
        }
        assert!((f.aim - camera.offset(pitch)).length() < 0.01);
        assert!((f.distance - camera.distance).abs() < 0.01);
        // (The Destroyer's comes in from 8.75 to 8 m.)
        let mut truck = Camera::JAZZ;
        truck.distance = 8.75;
        truck.sprint = SprintCamera::TRUCK;
        let mut cam = RobotCam::default();
        for _ in 0..90 {
            f = cam.frame(&truck, (true, false), 0.0, 20.0, dt);
        }
        assert!((f.distance - 8.0).abs() < 0.01);
    }

    /// Spinning (the Whirlwind), the Titan's camera goes out from 8.75 to 11.75 m,
    /// most of the way in half a second and nearly all within 0.75 s, its aim as
    /// ever; and back after.
    /// The Infiltrator's has no such state.
    #[test]
    fn the_whirlwind_takes_the_camera_out() {
        let mut truck = Camera::JAZZ;
        truck.distance = 8.75;
        truck.sprint = SprintCamera::TRUCK;
        let mut cam = RobotCam::default();
        let dt = 1.0 / 60.0;
        let mut f = Frame::default();
        for _ in 0..30 {
            f = cam.frame(&truck, (false, true), 0.0, 0.0, dt);
        }
        assert!(f.distance > 11.25, "{}", f.distance);
        for _ in 0..15 {
            f = cam.frame(&truck, (false, true), 0.0, 0.0, dt);
        }
        assert!((f.distance - 11.75).abs() < 0.1, "{}", f.distance);
        assert!((f.aim - truck.offset(0.0)).length() < 1e-4);
        for _ in 0..90 {
            f = cam.frame(&truck, (false, false), 0.0, 0.0, dt);
        }
        assert!((f.distance - 8.75).abs() < 0.01);
        let jazz = Camera::JAZZ;
        let mut cam = RobotCam::default();
        for _ in 0..30 {
            f = cam.frame(&jazz, (false, true), 0.0, 0.0, dt);
        }
        assert!((f.distance - jazz.distance).abs() < 1e-4);
    }

    /// Sprinting, the view's pitch range closes in to 25 degrees either way over half
    /// a second, and opens out again after.
    #[test]
    fn sprinting_keeps_the_view_near_level() {
        let camera = Camera::JAZZ;
        let mut cam = RobotCam::default();
        let dt = 1.0 / 60.0;
        let usual = (-60f32.to_radians(), 55f32.to_radians());
        let (lo, hi) = cam.pitch_range(&camera, usual, false, dt);
        assert!((lo.to_degrees() + 60.0).abs() < 1e-3 && (hi.to_degrees() - 55.0).abs() < 1e-3);
        let mut range = (lo, hi);
        let mut steps = 0;
        for _ in 0..60 {
            let r = cam.pitch_range(&camera, usual, true, dt);
            assert!(
                r.0 >= range.0 - 1e-6 && r.1 <= range.1 + 1e-6,
                "it only closes in"
            );
            range = r;
            steps += 1;
            if (range.0.to_degrees() + 25.0).abs() < 0.01 {
                break;
            }
        }
        assert!(
            steps <= 31,
            "closed in within half a second: {steps} frames"
        );
        for _ in 0..60 {
            range = cam.pitch_range(&camera, usual, false, dt);
        }
        assert!((range.0.to_degrees() + 60.0).abs() < 0.01);
    }

    /// Sprinting the view wiggles up to 0.6 degrees each way (the Infiltrator's,
    /// at 20 m/s, past its 10 m/s for full size), standing still not at all, and
    /// jogging only its default 0.1.
    #[test]
    fn the_sprint_wiggles_the_view() {
        let camera = Camera::JAZZ;
        let dt = 1.0 / 60.0;
        let biggest = |sprinting: bool, speed: f32| {
            let mut cam = RobotCam::default();
            let mut most = Vec3::ZERO;
            for _ in 0..240 {
                let w = cam
                    .frame(&camera, (sprinting, false), 0.0, speed, dt)
                    .wiggle;
                most = most.max(w.abs());
            }
            most
        };
        let sprint = biggest(true, 20.0);
        assert!(
            sprint.max_element() <= 0.6 + 1e-4 && sprint.min_element() > 0.3,
            "{sprint}"
        );
        assert_eq!(biggest(false, 0.0), Vec3::ZERO);
        let jog = biggest(false, 12.0);
        assert!(
            jog.max_element() <= 0.1 + 1e-4 && jog.max_element() > 0.03,
            "{jog}"
        );
    }
}
