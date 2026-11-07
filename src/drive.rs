//! A car or truck on its wheels: TnCarForm's Driving state, which the car drops into
//! while Speed Boost is held, and its TnCarSimulation with its four
//! TnWheelAssemblies, the numbers the car physics Escalation gives it (the
//! vehicle's [`Drive`] and [`Handling`]: Car_Physics for the Infiltrator's cars,
//! tools/build_class.py's SKILL_INDEX).
//!
//! - The throttle is held full (Driving.UpdateSimulationInputs' Accelerator 1): its
//!   push, TnCarSimulation.UpdateBoost's, runs from MaxAcceleration at a standstill
//!   to what the drag takes at MaxSpeed, with more at low speeds
//!   (CalculateExtraLowSpeedBoostAcceleration: eight times MaxAcceleration, easing
//!   out by half MaxSpeed, at most 50 m/s²); the drag (UpdateDrag) grows with the
//!   square of the speed, gravity's worth at the PhysicsVolume's TerminalVelocity,
//!   so the push and the drag meet at MaxSpeed, where the car tops out. Jazz's car
//!   (Car_Physics: MaxSpeed 3100, MaxAcceleration 1200) is past 20 m/s within a
//!   second and near its 31 m/s top in four, as the game's is: down Ignition's long
//!   corridor in Escalation it runs about 30 m/s. (TnCarSimulation.Activate gives
//!   the body a DefaultLinearDamping of 0.2, but the game's car shows no sign of it
//!   holding the boost back: with it, it would top out at about 26 m/s.)
//! - The mouse steers (TnPlayerController.PlayerInCarForm.SetLocalInputs: its turn,
//!   smoothed by halves a frame, full lock at 550 of the game's mouse units a
//!   frame; vehicle_cam.rs reads it), squared for the wheels (UpdateSimulationInputs).
//!   The front wheels turn up to MaxSteeringAngle; each tyre pushes against its own
//!   sideways slide (TnTire: TireFrictionCoefficient of its load per unit of slide,
//!   up to twice its load), at its wheel, so the car turns, and slides, as its tyres
//!   let it: Jazz's car corners at up to about 120 degrees a second flat out. Its
//!   AngularDamping holds its turn less the harder it steers
//!   (UpdateAngularDamping). W, A, S and D do nothing on the wheels.
//! - In the air the air control takes over (UpdateAirControl): the steering slides
//!   and turns it gently.
//! - It leans out of turns (UpdateBanking: towards TurningBankValue at full lock,
//!   by the fourth power of the steering, eased in by TurningBankSpeed a frame and
//!   pulled a tenth of the way a frame, against its suspension's springs): Jazz's car
//!   about 15 degrees at full lock (car.rs's bank).
//!
//! Its body falls at the rigid bodies' gravity (WorldGravityZ, -2940), not the
//! robots'.

use super::*;
use crate::character::Drive;

/// The rigid bodies' gravity (DefaultGravityZ -2940, RBPhysicsGravityScaling 1),
/// m/s²: a car's or truck's falls at this rather than the robots' GRAVITY.
pub const VEHICLE_GRAVITY: f32 = -29.4;
/// The PhysicsVolume's TerminalVelocity (3500): the speed at which the drag would
/// match gravity (TnCarSimulation.CalculateDragAcceleration).
const TERMINAL_VELOCITY: f32 = 35.0;
/// The push at low speeds: LowSpeedBoostScale times MaxAcceleration from a
/// standstill, easing out by LowSpeedBoostThreshold of MaxSpeed, at most
/// MaxExtraAcceleration (m/s²).
const LOW_SPEED_BOOST: f32 = 8.0;
const LOW_SPEED_SHARE: f32 = 0.5;
const MAX_EXTRA_ACCEL: f32 = 50.0;
/// A tyre's sideways push tops out at twice its share of the weight (the TnTire's
/// MaxForce, TnWheelAssembly.InitializeTire).
const TYRE_MAX: f32 = 2.0;
/// The steps the car is moved in (shorter than a frame, for its tyres' sake).
const STEP: f32 = 1.0 / 120.0;

/// The steering the wheels take from the player's (-1 to 1, right): its square,
/// signed, as UpdateSimulationInputs gives a player's car.
pub fn wheel_steering(input: f32) -> f32 {
    let s = input.clamp(-1.0, 1.0);
    s.abs() * s
}

/// Moves `player`'s car (or truck) on its wheels for `dt` seconds, steered by
/// `input` (the player's steering, -1 to 1, right: none while he can't drive), and
/// returns its velocity along the ground. Its speeds ahead and to the right and its
/// turn (Player's drive_speed, strafe_speed and yaw_rate, the turn to the left) are
/// the car's, carried from frame to frame; its steering for the wheels and the lean
/// is left in `player.steer`.
pub fn drive_velocity(player: &mut Player, input: f32, dt: f32) -> Vec3 {
    let d: Drive = player.kit.drive;
    let handling = player.kit.handling;
    let steer = wheel_steering(input);
    player.steer = steer;
    let g = -VEHICLE_GRAVITY;
    let top = handling.max_speed.max(1.0);
    let drag_at = |speed: f32| speed * speed * g / (TERMINAL_VELOCITY * TERMINAL_VELOCITY);
    let lock = steer * d.max_steer.to_radians();
    let (sin, cos) = lock.sin_cos();
    // (Ahead, to the right, and the turn to the right, as the game's axes have them.)
    let (mut ahead, mut right) = (player.drive_speed, player.strafe_speed);
    let mut turn = -player.yaw_rate;
    let mut left = dt;
    while left > 1e-6 {
        let h = left.min(STEP);
        left -= h;
        let (mut push_ahead, mut push_right, mut twist) = (0.0, 0.0, 0.0);
        if player.grounded {
            // The throttle, full.
            let low = top * LOW_SPEED_SHARE;
            let below = ahead.clamp(0.0, low) / low;
            let extra =
                (handling.accel * LOW_SPEED_BOOST * (1.0 - below).powi(2)).min(MAX_EXTRA_ACCEL);
            push_ahead += handling.accel + (drag_at(top) - handling.accel) * (ahead / top) + extra;
            // The drag, against the way it goes.
            let speed = ahead.hypot(right);
            if speed > 1e-3 {
                let drag = drag_at(speed) / speed;
                push_ahead -= drag * ahead;
                push_right -= drag * right;
            }
            // The tyres, each axle carrying half the weight: the front ones turned
            // by the steering, each pushing against how fast it slides sideways.
            let tyre = |grip: f32, slide: f32| -(grip * slide).clamp(-TYRE_MAX, TYRE_MAX) * g * 0.5;
            let front = tyre(d.grip_front, cos * (right + turn * d.ahead) - sin * ahead);
            let back = tyre(d.grip_back, right - turn * d.behind);
            push_ahead -= sin * front;
            push_right += cos * front + back;
            twist += d.ahead * cos * front - d.behind * back;
        } else {
            // The air control, by the steering.
            push_right += steer * d.air_strafe;
            turn += steer * d.air_turn * h;
        }
        let (was_ahead, was_right) = (ahead, right);
        ahead += (push_ahead + turn * was_right) * h;
        right += (push_right - turn * was_ahead) * h;
        turn += twist / d.gyration * h;
        turn *= (-d.angular_damping * (1.0 - steer.abs()).powi(2) * h).exp();
        player.heading -= turn * h;
    }
    player.drive_speed = ahead;
    player.strafe_speed = right;
    player.yaw_rate = -turn;
    let facing = Quat::from_rotation_y(player.heading);
    facing * Vec3::NEG_Z * ahead + facing * Vec3::X * right
}

/// The car's lean (car.rs: radians, the right side up; `bank_vel` its speed),
/// `target` the lean it is easing towards: a frame of `dt` seconds with the wheels
/// steering `steer` (wheel_steering's), on the ground or not, the ground under its
/// wheels leaning `ground` (and changing at `ground.1` a second: car.rs's
/// ground_under). As TnCarSimulation.UpdateBanking has it, at the game's 60 frames
/// a second: the lean it is after is TurningBankValue out of the turn by the square
/// of the wheels' steering (the fourth power of the player's), from the view's
/// level (its CameraRotation, the view's, which has no roll), eased in by
/// TurningBankSpeed a frame; the body is pulled a tenth of the way to it a frame,
/// and its suspension's springs and dampers pull it to the ground's lean (the car's
/// AngularDamping too, less as it steers). So across a slope it leans a quarter of
/// the way to the ground's (Jazz's car's springs against the banking's pull). In
/// the air there is no pull either way but the damping.
#[allow(clippy::too_many_arguments)]
pub fn bank(
    d: &Drive,
    steer: f32,
    grounded: bool,
    (ground, ground_rate): (f32, f32),
    bank: &mut f32,
    bank_vel: &mut f32,
    target: &mut f32,
    dt: f32,
) {
    const FRAME: f32 = 1.0 / 60.0;
    const PULL: f32 = 0.1;
    let want = if grounded {
        steer.signum() * steer * steer * d.bank.to_radians()
    } else {
        0.0
    };
    *target += (want - *target) * (1.0 - (1.0 - d.bank_speed.clamp(0.0, 1.0)).powf(dt / FRAME));
    let mut left = dt;
    while left > 1e-6 {
        let h = left.min(STEP);
        left -= h;
        let mut accel = -d.angular_damping * (1.0 - steer.abs()).powi(2) * *bank_vel;
        if grounded {
            accel += PULL / FRAME * ((*target - *bank) / FRAME - *bank_vel)
                - d.roll_pull * (*bank - ground)
                - d.roll_damping * (*bank_vel - ground_rate);
        }
        *bank_vel += accel * h;
        *bank += *bank_vel * h;
    }
    *bank = bank.clamp(-0.6, 0.6);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Jazz's car flat out: past 20 m/s within a second and near its 31 m/s top
    /// (Car_Physics' MaxSpeed) in four, never past it, as its push and drag have it.
    #[test]
    fn jazz_car_speeds() {
        let mut player = new_player();
        player.grounded = true;
        let mut t = 0.0;
        let (mut at_one, mut at_four) = (0.0, 0.0);
        while t < 12.0 {
            drive_velocity(&mut player, 0.0, 1.0 / 60.0);
            t += 1.0 / 60.0;
            if (t - 1.0f32).abs() < 0.009 {
                at_one = player.drive_speed;
            }
            if (t - 4.0f32).abs() < 0.009 {
                at_four = player.drive_speed;
            }
            assert!(player.drive_speed <= 31.0 + 1e-3, "{}", player.drive_speed);
        }
        assert!(at_one > 19.5, "{at_one}");
        assert!(at_four > 29.5, "{at_four}");
        assert!(player.drive_speed > 30.8, "{}", player.drive_speed);
    }

    /// Flat out at full lock the tyres hold it to about 1.7 to 2.1 rad/s, sliding wide
    /// and slowing; with half the mouse's lock it turns, gently, the way it steers.
    #[test]
    fn jazz_car_turns() {
        let mut player = new_player();
        player.grounded = true;
        player.drive_speed = 24.6;
        for _ in 0..240 {
            drive_velocity(&mut player, 1.0, 1.0 / 60.0);
        }
        // (Right: the turn to the left negative.)
        assert!(
            player.yaw_rate < -1.6 && player.yaw_rate > -2.8,
            "{}",
            player.yaw_rate
        );
        let mut player = new_player();
        player.grounded = true;
        player.drive_speed = 24.6;
        for _ in 0..240 {
            drive_velocity(&mut player, -0.5, 1.0 / 60.0);
        }
        assert!(
            player.yaw_rate > 0.2 && player.yaw_rate < 1.2,
            "{}",
            player.yaw_rate
        );
    }

    /// The lean at full lock: about three quarters of TurningBankValue, out of the
    /// turn.
    #[test]
    fn jazz_car_leans() {
        let d = Drive::JAZZ;
        let (mut b, mut v, mut t) = (0.0, 0.0, 0.0);
        for _ in 0..120 {
            bank(
                &d,
                1.0,
                true,
                (0.0, 0.0),
                &mut b,
                &mut v,
                &mut t,
                1.0 / 60.0,
            );
        }
        assert!((b.to_degrees() - 15.0).abs() < 1.0, "{}", b.to_degrees());
    }

    /// Driving straight across a slope, its springs lean it to the ground against
    /// the banking's pull towards level: a quarter of the ground's lean (Jazz's car's
    /// roll pull, 120.5 a second squared, against the banking's tenth a frame, 360).
    #[test]
    fn jazz_car_leans_a_quarter_into_a_side_slope() {
        let d = Drive::JAZZ;
        let ground = 20f32.to_radians();
        let (mut b, mut v, mut t) = (0.0, 0.0, 0.0);
        for _ in 0..240 {
            bank(
                &d,
                0.0,
                true,
                (ground, 0.0),
                &mut b,
                &mut v,
                &mut t,
                1.0 / 60.0,
            );
        }
        let share = b / ground;
        assert!((share - 0.25).abs() < 0.02, "{share}");
    }
}
