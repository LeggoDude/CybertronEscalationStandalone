//! A car's or truck's camera, as the game's follows it: its Escalation character's
//! camera set's driving and hovering strategies (character::CamStrategy:
//! Car_Bumblebee_STRATEGY and HoverCar_Bumblebee_STRATEGY for Bumblebee's car,
//! Truck_Optimus_STRATEGY and HoverTruck_Optimus_STRATEGY for Optimus Prime's
//! truck...: tools/class_data.py's vehicle_camera_set), each its HmCameraStrategy's
//! behaviors.
//!
//! - Hovering, the player turns the view with the mouse and the car faces it.
//!   Boosting, the mouse steers the car instead (drive.rs) and the view turns with
//!   the car (TnDrivingOrbitRotationCameraBehavior.Update: its orbit's yaw is the
//!   car's own; its MatchRotationSpeed is its pitch's); the mouse still pitches it.
//!   Either way the view's turn is smoothed (HmOrbitSmootherCameraBehavior), so the
//!   car swings a little across the picture in a turn. Letting go of the boost, the
//!   car keeps facing the way it was going (Rig::facing).
//! - The camera blends from one strategy into the other over the one it goes to's
//!   TransitionTime as the car boosts and stops.
//! - It is anchored over the vehicle's middle (VehicleCamera::middle: where the
//!   game's pawn is, the middle of its mesh's bounds; the anchor in the view's
//!   axes) and orbits out behind it, offset in the view's axes by the view's pitch
//!   (TnScreenSpaceOffsetByPitchCameraBehavior: lower looking level),
//!   its field of view, orbit, offsets, roll and wiggle each the camera states'
//!   (TnCarForm's: HighSpeed after 1.5 s past a quarter of the top speed,
//!   TurningLeft and TurningRight after a quarter of a second past three quarters
//!   of the steering's lock, Jumping, HoverStrafing), eased in as the game eases
//!   them: driving Jazz's car the view widens from 90 to 95 degrees in a turn, slips
//!   out to the turn's side, lower, and rolls 2.5 degrees into it.
//! - Boosting blurs the picture as it moves (the driving strategy's
//!   CamPostProcessSettings: MotionBlur_Amount of each frame's motion, as UE3's blur
//!   takes it).

use super::*;
use crate::character::{CamStrategy, Kit, cam_state};

/// TnCarForm's camera states: HighSpeed once its speed ahead has been at least
/// HighSpeedPercentage (0.25) of its top speed for MaintainHighSpeedTime (1.5 s);
/// TurningLeft and TurningRight once the steering has been past
/// MaintainTurnPercentage (0.75) of full lock for MaintainTurnTime (0.25 s);
/// HoverStrafing hovering faster than MinHoverStrafeSpeed (100: 1 m/s).
const HIGH_SPEED_SHARE: f32 = 0.25;
const HIGH_SPEED_SECS: f32 = 1.5;
const TURN_SHARE: f32 = 0.75;
const TURN_SECS: f32 = 0.25;
const HOVER_STRAFE_SPEED: f32 = 1.0;
/// The mouse's steering (TnPlayerController.PlayerInCarForm.SetLocalInputs): the
/// game's mouse turn a frame (the counts by MouseSensitivity, 60 at the demo's
/// usual Mouse Sensitivity, and by the field of view over 90), smoothed by halves
/// a frame, full lock at 550; at the game's 60 frames a second.
const FULL_LOCK: f32 = 550.0;
const MOUSE_SENSITIVITY: f32 = 60.0;
const GAME_FPS: f32 = 60.0;
/// The wiggle is full size at FullWiggleSpeedThreshold (100: 1 m/s) and up, none
/// standing still (the cars' and trucks' VelocityAdjustPercent, 1).
const WIGGLE_FULL_SPEED: f32 = 1.0;
/// How fast the view's pitch is brought into a narrower range (radians a second).
const PITCH_EASE: f32 = 1.5;

/// A car's or truck's camera strategies (driving, hovering): its vehicle's, or
/// Jazz's car's without the characters' files; None for a tank or a jet.
pub fn strategies(kit: &Kit) -> Option<[CamStrategy; 2]> {
    if kit.tank || kit.plane.is_some() {
        return None;
    }
    Some(
        kit.vehicle_camera
            .strategies
            .unwrap_or_else(character::jazz_car_camera),
    )
}

/// What one strategy has eased to: its field of view, orbit, offsets (ahead, side,
/// up) and roll (degrees, the game's: right side down), each with how fast it is
/// moving.
#[derive(Clone, Copy, Default)]
struct Eased {
    started: bool,
    fov: (f32, f32),
    distance: (f32, f32),
    offset: [(f32, f32); 3],
    roll: (f32, f32),
}

/// One strategy's wiggle (its TnWiggler3CameraBehavior's HmWiggler3Data): the view
/// turned a little in yaw, pitch and roll, each heading in a straight line from
/// where it is to a new spot picked at random within its amplitude (RS_Random, no
/// smoothing curve: the game's every profile), a new one every 1 / frequency
/// seconds; a change of profile (camera state) eases its amplitudes and
/// frequencies over its TransitionTime (UpdateProfileTransition).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Wiggler {
    /// The profile it is in (amplitudes, frequencies) and the one it goes to, with
    /// the time left to get there.
    amp: [f32; 3],
    hz: [f32; 3],
    goal: ([f32; 3], [f32; 3], f32),
    left: f32,
    started: bool,
    /// Each axis: where it is, the spot it heads for, the share of the way it has
    /// come and how long the way takes.
    at: [f32; 3],
    to: [f32; 3],
    done: [f32; 3],
    secs: [f32; 3],
    /// Its random numbers' count.
    draws: u32,
}

impl Wiggler {
    /// Moves on by `dt` in `profile` (amplitudes, frequencies, TransitionTime);
    /// where it has turned the view (yaw, pitch, roll; degrees, before its speed's
    /// share).
    pub(crate) fn step(
        &mut self,
        profile: ([f32; 3], [f32; 3], f32),
        seed: u32,
        dt: f32,
    ) -> [f32; 3] {
        if !self.started {
            self.started = true;
            (self.amp, self.hz, self.goal) = (profile.0, profile.1, profile);
        } else if profile != self.goal {
            self.goal = profile;
            self.left = profile.2;
        }
        if self.left > 0.0 {
            let share = dt.min(self.left) / self.left;
            for i in 0..3 {
                self.amp[i] += (self.goal.0[i] - self.amp[i]) * share;
                self.hz[i] += (self.goal.1[i] - self.hz[i]) * share;
            }
            self.left -= dt;
        } else {
            (self.amp, self.hz) = (self.goal.0, self.goal.1);
        }
        for i in 0..3 {
            if self.hz[i] <= 0.0 {
                self.at[i] = 0.0;
                continue;
            }
            let mut tick = dt;
            let over = dt - (1.0 - self.done[i]) * self.secs[i];
            if over >= 0.0 {
                // There: off to the next spot.
                self.at[i] = self.to[i];
                self.draws = self.draws.wrapping_add(1);
                let r = hash01(seed.wrapping_mul(7919).wrapping_add(self.draws));
                self.to[i] = (r * 2.0 - 1.0) * self.amp[i];
                tick = over;
                self.done[i] = 0.0;
                self.secs[i] = 1.0 / self.hz[i];
            }
            let done = (self.done[i] + tick / self.secs[i]).min(1.0);
            let step = (done - self.done[i]) / (1.0 - self.done[i]).max(1e-6);
            self.at[i] += (self.to[i] - self.at[i]) * step;
            self.done[i] = done;
        }
        self.at
    }
}

/// The local player's vehicle camera (on OrbitCamera).
#[derive(Clone, Copy, Default)]
pub struct Rig {
    /// The mouse's steering, smoothed (-1 to 1, right).
    pub steer: f32,
    /// How far the camera has blended into the driving strategy (0 hovering, 1
    /// driving), and how fast.
    blend: f32,
    blend_vel: f32,
    eased: [Eased; 2],
    /// How long the car has been fast, and turning left and right.
    high_speed: f32,
    turning_left: f32,
    turning_right: f32,
    /// The view's turn, smoothed (yaw and pitch, radians), with their speeds.
    pub view: (f32, f32),
    view_vel: (f32, f32),
    /// How much of its own turn the view still gives the car to face, off its
    /// wheels (facing): 1 as it stops boosting, none by the time the camera has
    /// blended back into hovering.
    kept: f32,
    active: bool,
    /// Each strategy's wiggle (driving, hovering).
    wigglers: [Wiggler; 2],
}

/// This frame's camera (frame's).
pub struct Shot {
    /// Where it sits against the vehicle (rig_offsets'), its field of view
    /// (degrees, across), its roll (radians, Bevy's: the right side up), its wiggle
    /// (pitch, yaw and roll, degrees) and its blur (the shutter, a share of the
    /// demo's frame).
    pub offsets: Vec4,
    pub fov: f32,
    pub roll: f32,
    pub wiggle: Vec3,
    pub shutter: f32,
}

impl Rig {
    /// The mouse's steering this frame: `dx` the mouse's sideways counts, `scale`
    /// the Mouse Sensitivity's (Controls::mouse_scale), `fov` the view's.
    pub fn read_steering(&mut self, dx: f32, scale: f32, fov: f32, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        // (The game's mouse turn in one of its frames, at this frame's speed.)
        let counts = dx / dt / GAME_FPS;
        let turn = counts * MOUSE_SENSITIVITY * scale * fov / 90.0;
        let raw = turn / FULL_LOCK;
        self.steer += (raw - self.steer) * (1.0 - 0.5f32.powf(dt * GAME_FPS));
        self.steer = self.steer.clamp(-1.0, 1.0);
    }

    /// The heading the car faces hovering (the player's input's yaw: main.rs's
    /// view_yaw), `yaw` the view's turn before its smoothing: the smoothed view's,
    /// as the game's player controller takes its camera's (UpdatePlayerViewPoint);
    /// but off its wheels the unsmoothed turn, which was the car's own while it
    /// boosted, giving way to the smoothed one over the hovering strategy's
    /// TransitionTime. So letting go of the boost the car keeps the way it was
    /// going, rather than turning at once to where the smoothed view had fallen
    /// behind it (well behind after a turn in the air).
    pub fn facing(&self, yaw: f32) -> f32 {
        let k = self.kept.clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k);
        self.view.0 + angle_to(self.view.0, yaw) * k
    }

    /// The view's turn, while it has been following a vehicle.
    pub fn view_if_active(&self) -> Option<(f32, f32)> {
        self.active.then_some(self.view)
    }

    /// Out of the vehicle: the next time it starts afresh, hovering.
    pub fn reset(&mut self, yaw: f32, pitch: f32) {
        *self = Rig {
            view: (yaw, pitch),
            ..Rig::default()
        };
    }

    /// The camera states the player is in (cam_state's bits), the clocks behind
    /// them moved on by `dt`.
    fn states(&mut self, player: &Player, car: &car::Car, dt: f32) -> u8 {
        let driving = !car.hover;
        let mut on = 0;
        let fast =
            driving && player.drive_speed >= HIGH_SPEED_SHARE * player.kit.handling.max_speed;
        self.high_speed = if fast { self.high_speed + dt } else { 0.0 };
        self.turning_left = if driving && self.steer < -TURN_SHARE {
            self.turning_left + dt
        } else {
            0.0
        };
        self.turning_right = if driving && self.steer > TURN_SHARE {
            self.turning_right + dt
        } else {
            0.0
        };
        if self.high_speed >= HIGH_SPEED_SECS {
            on |= cam_state::HIGH_SPEED;
        }
        if self.turning_left >= TURN_SECS {
            on |= cam_state::TURNING_LEFT;
        }
        if self.turning_right >= TURN_SECS {
            on |= cam_state::TURNING_RIGHT;
        }
        if driving {
            on |= cam_state::BOOSTING;
        }
        if car.in_jump.is_some() && !player.grounded {
            on |= cam_state::JUMPING;
        }
        if !driving && player.velocity.length() >= HOVER_STRAFE_SPEED {
            on |= cam_state::HOVER_STRAFING;
        }
        on
    }

    /// The camera this frame, for `player` in his car or truck with these
    /// `strategies` (driving, hovering): the view's turn (`yaw`, `pitch`: where the
    /// player has it, radians) turned with the car while it boosts and kept in the
    /// strategies' pitch range, then smoothed into `self.view`, which the camera
    /// looks along.
    pub fn frame(
        &mut self,
        strategies: &[CamStrategy; 2],
        player: &Player,
        car: &car::Car,
        yaw: &mut f32,
        pitch: &mut f32,
        dt: f32,
    ) -> Shot {
        let [drive, hover] = strategies;
        if !self.active {
            self.active = true;
            self.view = (*yaw, *pitch);
            self.view_vel = (0.0, 0.0);
        }
        let on = self.states(player, car, dt);
        // Blending into the strategy it is going to, over its TransitionTime.
        let (goal, secs) = if car.hover {
            (0.0, hover.blend)
        } else {
            (1.0, drive.blend)
        };
        smooth_damp(
            &mut self.blend,
            &mut self.blend_vel,
            goal,
            secs.max(0.01) * 0.5,
            dt,
        );
        let w = self.blend.clamp(0.0, 1.0);
        // Boosting, the view turns with the car: the driving strategy's orbit yaw is
        // the car's (TnDrivingOrbitRotationCameraBehavior.Update). (It used to come
        // round after it at MatchRotationSpeed, a third of a second behind, so it
        // fell well behind in a turn, and further in the air.)
        if car.hover {
            self.kept = (self.kept - dt / hover.blend.max(0.01)).max(0.0);
        } else {
            *yaw = player.heading;
            self.kept = 1.0;
        }
        let lo = (drive.pitch_range.0 * w + hover.pitch_range.0 * (1.0 - w)).to_radians();
        let hi = (drive.pitch_range.1 * w + hover.pitch_range.1 * (1.0 - w)).to_radians();
        // (Eased into a narrower range rather than cut to it.)
        let step = PITCH_EASE * dt;
        if *pitch > hi {
            *pitch = (*pitch - step).max(hi);
        } else if *pitch < lo {
            *pitch = (*pitch + step).min(lo);
        }
        // The orbit smoother.
        let smooth = (drive.smooth * w + hover.smooth * (1.0 - w)).max(1e-3) * 0.5;
        let (mut vy, mut vyv) = (0.0, self.view_vel.0);
        smooth_damp(&mut vy, &mut vyv, angle_to(self.view.0, *yaw), smooth, dt);
        self.view.0 += vy;
        self.view_vel.0 = vyv;
        let (mut vp, mut vpv) = (self.view.1, self.view_vel.1);
        smooth_damp(&mut vp, &mut vpv, *pitch, smooth, dt);
        self.view = (self.view.0, vp);
        self.view_vel.1 = vpv;
        let pitch_deg = self.view.1.to_degrees();

        // Each strategy's values for these states, eased.
        let speed = player.velocity.length();
        let mut shots = [(Vec4::ZERO, 0.0, 0.0, [0.0; 3]); 2];
        for (i, s) in strategies.iter().enumerate() {
            let e = &mut self.eased[i];
            let (fov, fov_secs) = s.fov.get(on);
            let distance = s.distance.get(on);
            let distance_secs = if distance != s.distance.get(0) {
                s.distance_secs.0
            } else {
                s.distance_secs.1
            };
            let curve = s.curve.get(on);
            let offset = curve.at(pitch_deg);
            let (roll, roll_secs) = s.roll.get(on);
            if !e.started {
                *e = Eased {
                    started: true,
                    fov: (fov, 0.0),
                    distance: (distance, 0.0),
                    offset: [(offset.x, 0.0), (offset.y, 0.0), (offset.z, 0.0)],
                    roll: (roll, 0.0),
                };
            }
            ease(&mut e.fov, fov, fov_secs, dt);
            ease(&mut e.distance, distance, distance_secs, dt);
            for (o, target) in e.offset.iter_mut().zip(offset.to_array()) {
                ease(o, target, curve.secs, dt);
            }
            ease(&mut e.roll, roll, roll_secs, dt);
            let wiggle = self.wigglers[i].step(s.wiggle.get(on), i as u32 + 1, dt);
            let [ahead, side, up] = e.offset.map(|(x, _)| x);
            // (Over its middle, that far ahead of its origin; a class built before
            // its middle over its strategies' origin.)
            let (middle, forward) = player.kit.vehicle_camera.middle.unwrap_or((s.origin, 0.0));
            let origin = middle + car.raised();
            let back = e.distance.0 - ahead - forward;
            let offsets = if s.anchor_turns {
                Vec4::new(side, origin, back, s.anchor + up)
            } else {
                Vec4::new(side, origin + s.anchor, back, up)
            };
            shots[i] = (offsets, e.fov.0, e.roll.0, wiggle);
        }
        let mix = |a: f32, b: f32| a * w + b * (1.0 - w);
        let (d, h) = (shots[0], shots[1]);
        // The wiggle, by the car's speed (UpdateScaleAdjust): yaw, pitch and roll
        // as the shot takes them (pitch, yaw, roll).
        let size = (speed / WIGGLE_FULL_SPEED).min(1.0);
        let [yaw, pitch, roll] = [0, 1, 2].map(|axis| mix(d.3[axis], h.3[axis]) * size);
        let blur = mix(drive.blur, hover.blur);
        Shot {
            offsets: h.0.lerp(d.0, w),
            fov: mix(d.1, h.1),
            roll: -mix(d.2, h.2).to_radians(),
            wiggle: Vec3::new(pitch, yaw, roll),
            shutter: blur,
        }
    }
}

/// Eases `x` (with its speed) to `target` over about `secs` (the game's HmC2Smoother:
/// smooth_damp with half its SmoothTime); at once for none.
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

    /// Jazz's car's driving camera, level and at rest: 1 m over its origin (its
    /// anchor's 1.5 m less the level view's 0.5 m), 5 m out, 90 degrees wide; and
    /// its hovering one 0.9 m over in the middle of its pitch range, 80 degrees.
    #[test]
    fn jazz_car_camera() {
        let [drive, hover] = character::jazz_car_camera();
        assert_eq!(drive.fov.get(0), (90.0, 1.0));
        assert_eq!(
            drive
                .fov
                .get(cam_state::TURNING_LEFT | cam_state::HIGH_SPEED)
                .0,
            95.0
        );
        assert!((drive.anchor + drive.curve.get(0).at(0.0).z - 1.0).abs() < 1e-4);
        assert_eq!(drive.distance.get(0), 5.0);
        assert_eq!(drive.follow, 3.0);
        // (Its curve's middle is the middle of its -25 to 30 degrees.)
        assert!((hover.anchor + hover.curve.get(0).at(2.5).z - 0.9).abs() < 1e-4);
        assert_eq!(hover.fov.get(cam_state::HOVER_STRAFING), (80.0, 0.5));
        assert_eq!(hover.distance.get(cam_state::HOVER_STRAFING), 5.25);
        // (The turn's offset is the high speed turn's once fast.)
        let fast_left = drive
            .curve
            .get(cam_state::TURNING_LEFT | cam_state::HIGH_SPEED);
        assert!((fast_left.at(0.0).y + 0.85).abs() < 1e-4);
        assert_eq!(drive.roll.get(cam_state::TURNING_RIGHT).0, 2.5);
        // (Strafing, its wiggle: yaw, pitch and roll, a new spot 10, 7 and 8 times
        // a second, eased into over a quarter of a second.)
        assert_eq!(
            hover.wiggle.get(cam_state::HOVER_STRAFING),
            ([0.1, 0.075, 0.1], [10.0, 7.0, 8.0], 0.25)
        );
    }

    /// Boosting, the view turns with the car (its orbit's yaw the car's), the
    /// smoothed view a little behind it in a turn. Let go of the boost after a fast
    /// turn and the car still faces the way it was going, not the smoothed view
    /// behind it, which comes round to it as the camera blends back into hovering;
    /// by then the car faces the smoothed view again, as it hovers.
    #[test]
    fn letting_go_of_the_boost_keeps_the_cars_heading() {
        let strategies = character::jazz_car_camera();
        let mut rig = Rig::default();
        let mut player = new_player();
        let mut car = car::Car::default();
        car.hover = false;
        let (mut yaw, mut pitch) = (0.0f32, 0.0f32);
        let dt = 1.0 / 60.0;
        // A second on its wheels turning 120 degrees a second (a turn in the air
        // after a jump, or a hard corner).
        for _ in 0..60 {
            player.heading += 120f32.to_radians() * dt;
            rig.frame(&strategies, &player, &car, &mut yaw, &mut pitch, dt);
            assert!(angle_to(yaw, player.heading).abs() < 1e-5);
        }
        let behind = angle_to(rig.view.0, player.heading).abs();
        assert!(behind > 5f32.to_radians(), "{}", behind.to_degrees());
        // Let go: it faces where it was going.
        car.hover = true;
        rig.frame(&strategies, &player, &car, &mut yaw, &mut pitch, dt);
        let off = angle_to(rig.facing(yaw), player.heading).abs();
        assert!(
            off < 0.01 * behind,
            "{} of {}",
            off.to_degrees(),
            behind.to_degrees()
        );
        // Hovering on (the car facing the view, the view standing still): the
        // smoothed view comes round to it, and the car faces the smoothed view.
        for _ in 0..120 {
            player.heading = rig.facing(yaw);
            rig.frame(&strategies, &player, &car, &mut yaw, &mut pitch, dt);
        }
        assert_eq!(rig.facing(yaw), rig.view.0);
        assert!(angle_to(rig.view.0, yaw).abs() < 0.5f32.to_radians());
        assert!(angle_to(player.heading, yaw).abs() < 0.5f32.to_radians());
    }

    /// The wiggle as the game's HmWiggler3Data runs it: never past its amplitude,
    /// each axis heading in a straight line to a new spot every 1 / frequency
    /// seconds, about two thirds of its amplitude from the last on average; so
    /// strafing it turns the view only a few tenths of a degree a second.
    #[test]
    fn the_wiggle_drifts_from_spot_to_spot() {
        let profile = ([0.1, 0.075, 0.1], [10.0, 7.0, 8.0], 0.25);
        let mut wiggler = Wiggler::default();
        let dt = 1.0 / 60.0;
        let secs = 20.0;
        let mut last = wiggler.step(profile, 2, dt);
        let mut moved = [0.0f32; 3];
        let mut turns = 0;
        let mut was = [0.0f32; 3];
        for _ in 0..(secs / dt) as usize {
            let now = wiggler.step(profile, 2, dt);
            for i in 0..3 {
                assert!(now[i].abs() <= profile.0[i] + 1e-6);
                let d = now[i] - last[i];
                moved[i] += d.abs();
                // (A straight line: the same step each frame, but where it turns.)
                if (d - was[i]).abs() > 1e-6 {
                    turns += 1;
                }
                was[i] = d;
            }
            last = now;
        }
        for i in 0..3 {
            let per_spot = moved[i] / (secs * profile.1[i]) / profile.0[i];
            assert!((0.45..0.9).contains(&per_spot), "{i}: {per_spot}");
            assert!(
                moved[i] / secs < 1.0,
                "{i}: {} degrees a second",
                moved[i] / secs
            );
        }
        // (It turns at most twice a spot: where it gets there, and the frame after.)
        let spots: f32 = profile.1.iter().map(|hz| hz * secs).sum();
        assert!(
            turns as f32 <= spots * 2.0 + 3.0,
            "{turns} turns for {spots} spots"
        );
    }
}
