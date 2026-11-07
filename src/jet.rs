//! The Scientist's jets: the game's TnPlaneForm, hovering (TnHoverPlaneSimulation,
//! HoverPlane_Physics, Escalation's) and flying as it boosts (TnPlaneSimulation,
//! Plane_Physics), with no gravity either way (OnActivate: SetGravityFactor(0)),
//! as TransGame.xxx has them; each jet's numbers are its own (character::Plane and
//! Kit::handling, from tools/build_class.py).
//!
//! * It is in the player's hands from the moment he starts to change into it, with
//!   the jet's keys (TnTransformation.Execute activates its form before the change
//!   has played, and the controller goes over to the jet's at once: DoTransform's
//!   GotoNewFormState): hovering, or flying straight away with Speed Boost held as it
//!   takes over (TnPlaneForm.OnActivate; PlayerInVehicleForm.BeginState passes a held
//!   boost on).
//! * Hovering, the keys strafe it in the view's frame, pitch and all (Strafe: out to
//!   its MaxLinearSpeed, 21 m/s, at its MaxLinearAcceleration, 15 m/s², times how
//!   far the keys are pushed, its grip coming back over the drift after flying:
//!   DriftDuration 0.25 s); with nothing held it comes to a stop in mid-air.
//!   Jump/Ascend and Descend climb and dive along the view's up (VerticalDash: 10
//!   m/s at 100 m/s², each held frame keeping it going 0.1 s more). Its body turns
//!   to the view (its PitchOffsetFromCamera over it: Vortex's 5 degrees), leaning
//!   into its moves (AddMotionPhysics: banked by its sideways acceleration and its
//!   turn, pitched down as it speeds up), as its rigid body turns (Turn: the angular
//!   velocity that would close the gap in a frame, a tenth of it taken in roll and
//!   half in pitch and yaw each frame, its angular damping 6).
//! * Flying (Speed Boost held, once the drift is over: Hovering.UpdateBoosting), it
//!   thrusts along the view (Strafe on the view's forward alone: MaxSpeed 40 m/s,
//!   MaxAcceleration 25), its speed across its nose dragged away (ApplyDrag:
//!   DragCoefficient 600 over its mass, solved for the end of the frame), its body
//!   turned to the view with a lead into turns (CalculateFinalRotationToMatch: the
//!   mouse's yaw input banks it up to RollDueToYawValue 77 degrees and yaws it up to
//!   16 further, its pitch input holds its nose up to 27 back, faded out from 45 to
//!   60 degrees of climb; each eased in a tenth a frame), its damping 8. Letting go
//!   of the boost drops it back to hovering, drifting; so does a crash at over 30
//!   m/s (Flying.OnRigidBodyCollision: ReturnToHoverCrashSpeed 3000).
//! * The mouse's turn is what the flight reads as its yaw and pitch input
//!   (TnPlayerController.PlayerInPlaneForm.SetLocalInputs: the view's turn this
//!   frame, smoothed over two, over 450 and 550 units a frame, squared); and flying,
//!   the view itself turns as the flying camera turns it (TnFlyingFollowCameraBehavior:
//!   the mouse sets how fast it should turn, up to 560 degrees a second yawing and
//!   360 pitching, its turn speeding up at most 720 degrees a second each second and
//!   damped, rolling the view 0.05 degrees per degree a second it yaws: view_turn),
//!   its anchor swinging out to the side of a turn (flying_anchor). A controller's
//!   right stick (pad.rs) is the yaw and pitch input as it is, and the flying
//!   camera's stick.
//!
//! The game's physics and its per-frame shares run at 60 frames a second; here they
//! are taken in steps of that length (STEP), whatever the frame rate.

use super::*;
use crate::character::Plane;

/// The game's frame, which the shares taken a frame are of.
const STEP: f32 = 1.0 / 60.0;
/// VerticalDashAcceleration (10000).
const CLIMB_ACCEL: f32 = 100.0;
/// The share of the gap's angular velocity its body takes a frame (TurnRate (0.1,
/// 0.5, 0.5): roll, pitch, yaw), here in Bevy's local axes: pitch about X, yaw about
/// Y, roll about Z.
const TURN_RATE: Vec3 = Vec3::new(0.5, 0.5, 0.1);
/// Its rigid body's angular damping hovering (TnHoverPlaneSimulation's 6) and flying
/// (TnPlaneSimulation's 8), and its top angular speed (MaxAngularVelocity 50).
const HOVER_DAMPING: f32 = 6.0;
const FLY_DAMPING: f32 = 8.0;
const MAX_SPIN: f32 = 50.0;
/// The mass its flight's drag acts on (its physics asset's rigid body: taken as a
/// car's 1500).
const MASS: f32 = 1500.0;
/// A crash out of flying at more than this (ReturnToHoverCrashSpeed 3000).
pub(crate) const CRASH_SPEED: f32 = 30.0;
/// The view's turn that gives the flight full yaw and pitch input
/// (SetLocalInputs: 450 and 550 rotation units a frame, at 60 a second), degrees a
/// second.
const FULL_YAW_RATE: f32 = 450.0 * 60.0 * 360.0 / 65536.0;
const FULL_PITCH_RATE: f32 = 550.0 * 60.0 * 360.0 / 65536.0;
/// Unreal's rotation units in a degree.
const UNITS: f32 = 65536.0 / 360.0;
/// The hovering lean's smoothing a frame (AddMotionPhysics: its acceleration's 0.1,
/// its angular velocity's 0.9).
const LEAN_ACCEL_SHARE: f32 = 0.1;
const LEAN_SPIN_SHARE: f32 = 0.9;
/// The hover jets' (Hovering.UpdateFx: _SmoothLocalLinearAcceleration and
/// _SmoothLocalAngularAcceleration, VLerp by 0.3 a frame).
const FX_ACCEL_SHARE: f32 = 0.3;

/// A jet's flight on its player's car (car::Car::jet).
#[derive(Clone, Copy, Debug)]
pub(crate) struct JetState {
    /// Its body's turn in the world and its angular velocity in its own axes (radians
    /// a second); set up as it comes together.
    pub rot: Quat,
    pub spin: Vec3,
    pub ready: bool,
    /// Its tilt against its heading (pitch and roll: car.rs draws the model so).
    pub tilt: Quat,
    /// The hover's lean (AddMotionPhysics): its acceleration and angular velocity in
    /// its own axes, smoothed; and its velocity last step.
    lean_accel: Vec3,
    lean_spin: Vec3,
    last_vel: Vec3,
    /// The flight's lead into turns (pitch, yaw, roll, degrees: RotationDelta).
    lead: Vec3,
    /// A climb or dive (VerticalDash): seconds of it left, and which way (+1 up).
    climb_left: f32,
    climb_dir: f32,
    /// Ascend and Descend held (their sounds: car.rs).
    pub climbing: bool,
    pub diving: bool,
    /// The mouse's turn as the flight reads it: its yaw and pitch speeds smoothed
    /// (MouseSteeringSmoother, MousePitchingSmoother: degrees a second), and the yaw
    /// and pitch inputs they make (-1..1).
    steer: Vec2,
    pub yaw_input: f32,
    pub pitch_input: f32,
    /// How hard its hover jets push (0..1: the booster sound's parameter,
    /// Jet_Hover_Boosters, the largest of them; car.rs's vehicle_effects works it out
    /// with their size, as Hovering.UpdateFx does).
    pub thrust: f32,
    /// Its acceleration and angular acceleration in its own axes (m/s^2, rad/s^2),
    /// smoothed as Hovering.UpdateFx smooths them (a 0.3 share a frame), and its
    /// angular velocity last step: what its hover jets push against.
    pub fx_accel: Vec3,
    pub fx_spin_accel: Vec3,
    last_spin: Vec3,
    /// A crash out of flying this frame (its clip and sound).
    pub crashed: bool,
}

impl Default for JetState {
    fn default() -> Self {
        JetState {
            rot: Quat::IDENTITY,
            spin: Vec3::ZERO,
            ready: false,
            tilt: Quat::IDENTITY,
            lean_accel: Vec3::ZERO,
            lean_spin: Vec3::ZERO,
            last_vel: Vec3::ZERO,
            lead: Vec3::ZERO,
            climb_left: 0.0,
            climb_dir: 0.0,
            climbing: false,
            diving: false,
            steer: Vec2::ZERO,
            yaw_input: 0.0,
            pitch_input: 0.0,
            thrust: 0.0,
            fx_accel: Vec3::ZERO,
            fx_spin_accel: Vec3::ZERO,
            last_spin: Vec3::ZERO,
            crashed: false,
        }
    }
}

/// A share taken a game frame (`per_frame`), as taken over `h` seconds.
fn share(per_frame: f32, h: f32) -> f32 {
    1.0 - (1.0 - per_frame.clamp(0.0, 1.0)).powf(h / STEP)
}

/// The view's turn (Bevy's: yaw about Y, then pitch about X).
fn view_rotation(yaw: f32, pitch: f32) -> Quat {
    Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0)
}

/// Unreal's pitch, yaw and roll (degrees: nose up, to the right, right wing down)
/// as a turn in Bevy's axes, in a frame's own axes (UE3's rotator order).
fn rotator(pitch: f32, yaw: f32, roll: f32) -> Quat {
    Quat::from_rotation_y(-yaw.to_radians())
        * Quat::from_rotation_x(pitch.to_radians())
        * Quat::from_rotation_z(-roll.to_radians())
}

/// The heading of a turn (where its nose points, about the vertical).
fn heading_of(rot: Quat) -> f32 {
    let f = rot * Vec3::NEG_Z;
    if f.x.abs() + f.z.abs() < 1e-4 {
        // (Straight up or down: by its up instead.)
        let u = rot * Vec3::Y;
        return (u.x * -f.y.signum()).atan2(u.z * -f.y.signum());
    }
    (-f.x).atan2(-f.z)
}

impl JetState {
    /// Its body set level at `heading` (as it comes together), still.
    pub fn start(&mut self, heading: f32, vel: Vec3) {
        *self = JetState {
            rot: Quat::from_rotation_y(heading),
            ready: true,
            last_vel: vel,
            ..JetState::default()
        };
        self.tilt = Quat::IDENTITY;
    }

    /// Turns its body towards `target` over `h` seconds (Turn: each frame the
    /// angular velocity takes TURN_RATE of the way to what would close the gap in
    /// that frame, then the rigid body's damping).
    fn turn(&mut self, target: Quat, damping: f32, h: f32) {
        let mut q = self.rot.inverse() * target;
        if q.w < 0.0 {
            q = -q;
        }
        let (axis, angle) = q.normalize().to_axis_angle();
        let gap = axis * angle;
        let want = gap / h;
        let rate = Vec3::new(
            share(TURN_RATE.x, h),
            share(TURN_RATE.y, h),
            share(TURN_RATE.z, h),
        );
        self.spin += (want - self.spin) * rate;
        self.spin /= 1.0 + damping * h;
        self.spin = self.spin.clamp_length_max(MAX_SPIN);
        self.rot = (self.rot * Quat::from_scaled_axis(self.spin * h)).normalize();
    }
}

/// The flight's yaw and pitch input from the mouse's turn this frame
/// (SetLocalInputs: its aTurn and aLookUp): `yaw_rate` and `pitch_rate` in degrees a
/// second (to the right, up), smoothed over two frames, over FULL_YAW_RATE and
/// FULL_PITCH_RATE, brought back to the unit circle beyond it, and squared (the
/// sign kept).
fn steer_inputs(state: &mut JetState, yaw_rate: f32, pitch_rate: f32, dt: f32) {
    let k = share(0.5, dt);
    state.steer += (Vec2::new(yaw_rate, pitch_rate) - state.steer) * k;
    let mut y = state.steer.x / FULL_YAW_RATE;
    let mut p = state.steer.y / FULL_PITCH_RATE;
    if y.abs() > 1.0 || p.abs() > 1.0 {
        let h = (y * y + p * p).sqrt();
        y /= h;
        p /= h;
    }
    let (y, p) = (y.clamp(-1.0, 1.0), p.clamp(-1.0, 1.0));
    state.yaw_input = y * y.abs();
    state.pitch_input = p * p.abs();
}

/// A jet's move this frame (move_jazz's: from the moment he starts to change into
/// it, as the game's TnTransformation.Execute activates its form at once, before the
/// change has played): hovering or flying as Speed Boost says, its velocity (metres
/// a second, up and all: Car::hover_vel, which it starts from) and its body turned
/// (JetState::rot; the player's heading is its nose's). `free`: he can steer (not
/// dead).
pub(crate) fn fly(
    player: &mut Player,
    car: &mut car::Car,
    keys: &PlayerInput,
    free: bool,
    dt: f32,
) -> Vec3 {
    let Some(plane) = player.kit.plane else {
        return car.hover_vel;
    };
    let handling = player.kit.handling;
    if !car.jet.ready {
        car.jet.start(player.heading, car.hover_vel);
        // OnActivate: flying at once with Speed Boost held as it takes over, or else
        // hovering, drifting first (Hovering.BeginState: Drift).
        car.hover = !(free && keys.boost);
        car.drift = if car.hover { handling.drift_secs } else { 0.0 };
    }
    car.jet.crashed = false;
    // Flying while Speed Boost is held, from when the drift after flying is over
    // (Hovering.UpdateBoosting, Flying.UpdateBoosting); hovering it drifts at first
    // (Hovering.BeginState: Drift).
    let boost = free && keys.boost;
    if car.hover && boost && car.drift <= 0.0 {
        car.hover = false;
    } else if !car.hover && !boost {
        car.hover = true;
        car.drift = handling.drift_secs;
    }
    let input = if free { keys.movement } else { Vec2::ZERO };
    // The view, and the mouse's turn (or the controller's stick) as the flight reads
    // it.
    let pitch = keys.view_dir.y.clamp(-1.0, 1.0).asin();
    let view = view_rotation(keys.yaw, pitch);
    match keys.stick {
        // A controller's right stick is the flight's yaw and pitch input as it is
        // (SetLocalInputs: GetNormalizedTurn, GetNormalizedLookUp).
        Some(stick) => {
            let stick = if free { stick } else { Vec2::ZERO };
            car.jet.steer = Vec2::ZERO;
            car.jet.yaw_input = stick.x.clamp(-1.0, 1.0);
            car.jet.pitch_input = stick.y.clamp(-1.0, 1.0);
        }
        None => {
            let steer = if free { keys.steer } else { Vec2::ZERO };
            steer_inputs(&mut car.jet, steer.x, steer.y, dt);
        }
    }
    // Ascend and Descend (Hovering.UpdateVerticalDashing): each held frame starts the
    // climb over (VerticalDash), up first if both are.
    let (up, down) = (free && keys.ascend, free && keys.descend);
    car.jet.climbing = up && car.hover;
    car.jet.diving = down && !up && car.hover;
    let mut vel = car.hover_vel;
    let mut left = dt;
    while left > 1e-6 {
        let h = left.min(STEP);
        left -= h;
        if car.hover {
            // (Held all this frame: every game frame of it starts the climb over.)
            if up || down {
                car.jet.climb_left = plane.climb_secs;
                car.jet.climb_dir = if up { 1.0 } else { -1.0 };
            }
            vel = hover_step(car, &plane, handling, view, pitch, input, vel, h);
        } else {
            vel = fly_step(car, &plane, handling, view, vel, h);
        }
    }
    car.hover_vel = vel;
    player.heading = heading_of(car.jet.rot);
    car.jet.tilt = Quat::from_rotation_y(player.heading).inverse() * car.jet.rot;
    // (Its speed along its nose and across it, for the engine and the effects.)
    let facing = Quat::from_rotation_y(player.heading);
    player.drive_speed = vel.dot(facing * Vec3::NEG_Z);
    player.strafe_speed = vel.dot(facing * Vec3::X);
    player.yaw_rate = 0.0;
    vel
}

/// One step of hovering (TnHoverPlaneSimulation.Update): strafing in the view's
/// frame, climbing or diving, and the body turned to the view, leaning.
#[allow(clippy::too_many_arguments)]
fn hover_step(
    car: &mut car::Car,
    plane: &Plane,
    handling: character::Handling,
    view: Quat,
    view_pitch: f32,
    input: Vec2,
    vel: Vec3,
    h: f32,
) -> Vec3 {
    let st = &mut car.jet;
    // (Its frame of reference is the view pitched by its PitchOffsetFromCamera.)
    let frame = view * Quat::from_rotation_x(plane.hover_pitch.to_radians());
    // UpdateStrafe: its grip back as (1 - drift left / DriftDuration)^2, or as far
    // as the keys are pushed.
    car.drift = (car.drift - h).max(0.0);
    let drift = (1.0 - car.drift / handling.drift_secs.max(0.01))
        .clamp(0.0, 1.0)
        .powi(2);
    let accel = handling.hover_accel * drift.max(input.x.abs().max(input.y.abs()));
    let local = frame.inverse() * vel;
    let want = Vec3::new(input.x, 0.0, -input.y) * handling.hover_speed;
    let mut a = ((want - local) / h).clamp_length_max(accel);
    // UpdateVerticalDash: along the frame's up alone, as well.
    if st.climb_left > 0.0 {
        st.climb_left = (st.climb_left - h).max(0.0);
        let up =
            ((st.climb_dir * plane.climb_speed - local.y) / h).clamp(-CLIMB_ACCEL, CLIMB_ACCEL);
        a.y += up;
    }
    let vel = vel + frame * a * h;
    // UpdateTurn: to the frame, leaning (AddMotionPhysics: its acceleration and turn
    // in its own axes, smoothed, bank it, Unreal's units of acceleration giving as
    // many of turn, and pitch it).
    let accel_local = st.rot.inverse() * ((vel - st.last_vel) / h);
    st.last_vel = vel;
    st.lean_accel += (accel_local - st.lean_accel) * share(LEAN_ACCEL_SHARE, h);
    st.lean_spin += (st.spin - st.lean_spin) * share(LEAN_SPIN_SHARE, h);
    st.fx_accel += (accel_local - st.fx_accel) * share(FX_ACCEL_SHARE, h);
    let roll_units = (st.lean_accel.x * 100.0 - st.lean_spin.y * 1000.0) * plane.lean;
    let pitch_units = st.lean_accel.z * 100.0 * 0.5 * plane.lean;
    let yaw = heading_of(view);
    let target = Quat::from_rotation_y(yaw)
        * Quat::from_rotation_x(
            view_pitch + (plane.hover_pitch + pitch_units / UNITS).to_radians(),
        )
        * Quat::from_rotation_z(-(roll_units / UNITS).to_radians());
    st.turn(target, HOVER_DAMPING, h);
    let spin_accel = (st.spin - st.last_spin) / h;
    st.last_spin = st.spin;
    st.fx_spin_accel += (spin_accel - st.fx_spin_accel) * share(FX_ACCEL_SHARE, h);
    vel
}

/// How hard a hover jet pushes (Hovering.CalculateBoosterFxAmount, 0..1): `at`, where
/// it is, and `push`, the way it pushes (against its flame), in the jet's own axes,
/// against the jet's smoothed acceleration (HoverPhysics.CalculateThrusterLinear-
/// Contribution: its share along the push of ThrusterLinearAccelerationLimit, 30
/// m/s^2, with no gravity, a jet's being turned off) and angular acceleration
/// (CalculateThrusterAngularContribution: the turn it would give, of 6 rad/s^2).
pub(crate) fn hover_jet_push(state: &JetState, at: Vec3, push: Vec3) -> f32 {
    let linear = (state.fx_accel.dot(push) / 30.0).clamp(0.0, 1.0);
    let angular =
        (at.normalize_or_zero().cross(push).dot(state.fx_spin_accel) / 6.0).clamp(0.0, 1.0);
    (linear + angular).min(1.0)
}

/// One step of flying (TnPlaneSimulation.Update): thrust along the view, drag across
/// the nose, and the body turned to the view with its lead into turns.
fn fly_step(
    car: &mut car::Car,
    plane: &Plane,
    handling: character::Handling,
    view: Quat,
    vel: Vec3,
    h: f32,
) -> Vec3 {
    let st = &mut car.jet;
    // UpdateFly: Strafe((1, 0, 0), mask (1, 0, 0), the view): the clamped push
    // towards top speed along the view, on its forward alone.
    let local = view.inverse() * vel;
    let want = Vec3::new(0.0, 0.0, -handling.max_speed);
    let a = ((want - local) / h).clamp_length_max(handling.accel);
    let mut vel = vel + view * Vec3::new(0.0, 0.0, a.z) * h;
    // ApplyDrag: the speed across its nose (its own Y and Z), solved for the end of
    // the step (s = (-1 + sqrt(1 + 4 A s0)) / 2A, A = dt Drag / Mass, Unreal units).
    let body = st.rot.inverse() * vel;
    let across = Vec3::new(body.x, body.y, 0.0);
    let s = across.length() * 100.0;
    if s > 1e-4 {
        let k = h * plane.drag / MASS;
        let after = (-1.0 + (1.0 + 4.0 * k * s).sqrt()) / (2.0 * k);
        vel = st.rot * (across * (after / s) + Vec3::new(0.0, 0.0, body.z));
    }
    st.last_vel = vel;
    // CalculateFinalRotationToMatch: the lead, its pitch input's faded out between
    // FullPitchThreshold and MaxPitchValue of climb or dive.
    let climb = (st.rot * Vec3::NEG_Z)
        .y
        .clamp(-1.0, 1.0)
        .asin()
        .to_degrees()
        .abs();
    let fade = if climb < plane.full_pitch {
        1.0
    } else if climb >= plane.max_pitch {
        0.0
    } else {
        ((plane.max_pitch - climb) / (plane.max_pitch - plane.full_pitch).max(1e-3)).clamp(0.0, 1.0)
    };
    let p = st.pitch_input;
    let y = st.yaw_input;
    let want = Vec3::new(
        -(p * p.abs()) * fade * plane.pitch_lead,
        y * y.abs() * plane.yaw_lead,
        y * y.abs() * plane.roll_lead,
    );
    st.lead += (want - st.lead) * share(plane.lead_lerp, h);
    let target = view * rotator(st.lead.x, st.lead.y, st.lead.z);
    st.turn(target, FLY_DAMPING, h);
    vel
}

/// It struck something (a wall, the floor or a ceiling) moving into it at `speed`
/// (metres a second): flying, over CRASH_SPEED drops it back to hovering
/// (Flying.OnRigidBodyCollision), drifting; and it loses its speed into it.
pub(crate) fn struck(player: &Player, car: &mut car::Car, normal: Vec3, speed: f32) {
    let into = car.hover_vel.dot(normal);
    if into < 0.0 {
        car.hover_vel -= normal * into;
    }
    if !car.hover && speed > CRASH_SPEED {
        car.hover = true;
        car.drift = player.kit.handling.drift_secs;
        car.jet.crashed = true;
        // (Ending a toggled boost, as a car's crash does: read_local_input.)
        car.boost_cooldown = car.boost_cooldown.max(car.drift.max(0.1));
    }
}

/// HoverPlane_MP_STRATEGY's look speeds (its TnOrbitRotationCameraBehavior's
/// DefaultLookSpeed: degrees a second at an input of 1, across and up and down) and
/// its RampSpeedMultiplier, which under the mouse is always in full (pad.rs's
/// vehicle_mouse_turn: hovering, no faster than 262.5 degrees a second across at
/// the usual Camera Sensitivity, so a flick can't roll the jet over: its body,
/// turned after the view, leans into the turn in hover_step).
pub(crate) const HOVER_LOOK: crate::pad::LookSpeed = crate::pad::LookSpeed {
    yaw: 70.0,
    pitch: 35.0,
    ramp: 1.0,
};
/// Its RampTime (a controller's stick).
pub(crate) const HOVER_RAMP_SECS: f32 = 0.4;

/// The view's turn while flying (TnFlyingFollowCameraBehavior.Update): `want` is how
/// fast the mouse would turn it (degrees a second: yaw to the right, pitch up, as
/// the demo's mouse look turns it), which sets its target speed (its stick: that
/// over what it settles at, MaxYawVelocity / (1 + YawDamping), clamped); its speed
/// (`spin`, degrees a second: _YawVelocity, _PitchVelocity) closes on it at most
/// MaxYawAcceleration a second each second, damped by YawDamping, and the same for
/// its pitch. Returns this frame's turn (radians: yaw to the right, pitch up) and
/// the view's roll (radians, Bevy's about its Z: RollDueToYawVelocity, rolling it
/// into the turn).
pub(crate) fn view_turn(spin: &mut Vec2, plane: &Plane, want: Vec2, dt: f32) -> (Vec2, f32) {
    let mut turn = Vec2::ZERO;
    let mut left = dt;
    while left > 1e-6 {
        let h = left.min(STEP);
        left -= h;
        let axes = [
            (
                want.x,
                plane.turn_speed,
                plane.turn_accel,
                plane.turn_damping,
            ),
            (
                want.y,
                plane.pitch_speed,
                plane.pitch_accel,
                plane.pitch_damping,
            ),
        ];
        for (i, (want, speed, accel, damping)) in axes.into_iter().enumerate() {
            let settles = speed / (1.0 + damping);
            let stick = (want / settles.max(1.0)).clamp(-1.0, 1.0);
            let v = &mut spin[i];
            let a = (stick * speed - *v).clamp(-accel, accel) - *v * damping;
            *v += a * h;
            turn[i] += v.to_radians() * h;
        }
    }
    // (Yawing to the right rolls it right: a negative turn about Bevy's Z.)
    (turn, -(plane.camera_roll * spin.x).to_radians())
}

/// The flying camera's anchor over the jet (TnFlyingFollowCameraBehavior.
/// UpdateAnchorPoint): out to the side of the turn by the square of how fast it
/// yaws (AnchorYawOffsetScalar 0.0325 units per degree a second squared), up by the
/// view's pitch (AnchorPitchUpOffsetScalar 1 and AnchorPitchDownOffsetScalar -1.5
/// units a degree) and down by its yaw speed squared (0.003), eased in a tenth a
/// frame (`anchor`: where it is, _OldOffset): metres in the view's axes (x right, y
/// up), from `up` metres over it; `spin` is the view's turn (view_turn's).
pub(crate) fn flying_anchor(anchor: &mut Vec3, spin: Vec2, up: f32, pitch: f32, dt: f32) -> Vec3 {
    let yaw_speed = spin.x;
    let pitch_deg = pitch.to_degrees();
    let mut offset = Vec3::new(0.0, up, 0.0);
    offset.x += yaw_speed.signum() * yaw_speed * yaw_speed * 0.0325 * 0.01;
    offset.y += pitch_deg * if pitch_deg < 0.0 { -1.5 } else { 1.0 } * 0.01;
    offset.y -= yaw_speed * yaw_speed * 0.003 * 0.01;
    *anchor += (offset - *anchor) * share(0.1, dt);
    *anchor
}

/// The homing rockets' lock (TnWeaponHoming.UpdateLockOnTarget): a sentry under the
/// crosshair (pick_target) for LockOnTime is locked on (StartLockOn: the lock tone
/// holds on), and stays so for HoldLockOnTime after it leaves the crosshair, or
/// until it is gone (CanLockOnTo); its rockets home on it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Lock {
    pub target: Option<Entity>,
    timer: f32,
    hold: f32,
    pub locked: bool,
}

/// HomingRocket_Vehicle_WEPDATA's LockOnTime and HoldLockOnTime (a jet's lock, as
/// in multiplayer; the Thermo Rocket Cannon's are arsenal.rs's LockOn).
pub(crate) const JET_LOCK: (f32, f32) = (0.25, 3.0);
/// How far off the crosshair, and how far away, a sentry can be picked to lock on
/// to (its picking: TnTargetPickingTypeHomingLock, out to the weapon's
/// TargetSnapRange 16852).
const LOCK_CONE_DEGREES: f32 = 7.5;
const LOCK_RANGE: f32 = 168.52;

impl Lock {
    /// A frame of it: `picked` is the sentry under the crosshair, if any; `alive`
    /// tells whether a sentry can still be locked on to; `(secs, hold)` are the
    /// weapon's LockOnTime and HoldLockOnTime.
    pub fn update(
        &mut self,
        picked: Option<Entity>,
        alive: impl Fn(Entity) -> bool,
        (secs, hold): (f32, f32),
        dt: f32,
    ) {
        match picked {
            Some(target) => {
                if self.target != Some(target) {
                    self.stop();
                    self.target = Some(target);
                }
                self.timer = (self.timer + dt).min(secs);
                self.hold = 0.0;
                if self.timer >= secs {
                    self.locked = true;
                }
            }
            None if self.locked => {
                self.hold += dt;
                if self.hold > hold || !self.target.is_some_and(&alive) {
                    self.stop();
                }
            }
            None => {
                self.timer = 0.0;
                self.hold = 0.0;
            }
        }
    }

    /// How far its lock has come, 0 to 1 (GetNormalizedLockOnTime: the timer over
    /// LockOnTime `secs`): the lock-on marker's (TnObjectiveMarkerTypeLockOn).
    pub fn progress(&self, secs: f32) -> f32 {
        if secs <= 0.0 {
            1.0
        } else {
            (self.timer / secs).min(1.0)
        }
    }

    /// No lock (StopLockOn).
    pub fn stop(&mut self) {
        *self = Lock::default();
    }

    /// What its rockets home on.
    pub fn homing(&self) -> Option<Entity> {
        self.target.filter(|_| self.locked)
    }
}

/// The enemy under the crosshair to lock on to: of those within LOCK_RANGE and
/// LOCK_CONE_DEGREES of the view (their middles), in sight of it, the nearest to its
/// line.
pub(crate) fn pick_target(
    level: &Level,
    colliders: &Colliders,
    (eye, view): (Vec3, Vec3),
    sentries: &[crate::enemies::Target],
) -> Option<Entity> {
    let cos = LOCK_CONE_DEGREES.to_radians().cos();
    sentries
        .iter()
        .filter_map(|t| {
            let to = t.at - eye;
            let dist = to.length();
            let along = to.dot(view) / dist.max(1e-4);
            (dist <= LOCK_RANGE && along >= cos).then_some((t, dist, along))
        })
        .filter(|&(t, dist, _)| {
            let dir = (t.at - eye) / dist.max(1e-4);
            // (The level in the way before its body.)
            let body = t.body.ray(eye, dir, dist).unwrap_or(dist);
            level
                .shot_hit(colliders, eye, dir, dist)
                .is_none_or(|wall| wall >= body - 0.05)
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(t, _, _)| t.entity)
}
