//! A game controller playing Jazz: Settings > Controls > Active Game Controls set to
//! Controller, as the game's PC version plays one. It takes its pad bindings in
//! place of its keyboard and mouse bindings then (TnPlayerInput.ApplyPreferred-
//! ControllerBindings), while a controller is connected (TnPlayerController.
//! UpdateControlScheme: with none, the keyboard and mouse again); the menus take
//! either. Its controls are the game's bindings for it with the control schemes
//! chosen on the Controller Layout screen (padlayout.rs: the robot's, the
//! vehicle's and the thumbsticks' together), each action on the controls bound to
//! its command. By default (PC-TransInput.ini's PadBindings):
//!
//! - Left stick: move (a vehicle hovers about on it). Right stick: look around (a
//!   car or truck on its wheels: steer, and the view up and down; a flying jet: its
//!   yaw and pitch, as the mouse's turn is).
//! - A: jump (a jet: climb). B: switch hands (a jet: dive; down, held a second:
//!   self destruct). X: tapped, reload (XButtonInputHandler: let go within
//!   ButtonTimeOut, 0.3 s); held, interact and pick up. Y: swap weapons.
//! - LB: dash (tap) or sprint (hold). RB: the ability. LT: fine aim (a vehicle:
//!   speed boost, held or toggled as Toggle Vehicle Boost says). RT: fire.
//! - Left stick click: change form. Right stick click: melee.
//! - D-pad left and right: the Assault and Utility T.E.C.H.
//! - Back: the scoreboard. Start: pause (menu.rs). Spectating, RB and LB cycle the
//!   player watched.
//!
//! The schemes move these (Tactician, Striker, Charger, Southpaw and Hoarder the
//! robot's, Seeker a jet's climb and dive; Southpaw thumbsticks move on the right
//! stick and look on the left). X's tap and hold go with its commands
//! (XButtonInputHandler: Hoarder's d-pad up reloads on a tap).
//!
//! The sticks have XInput's dead zones, the triggers its threshold. The right stick
//! turns the view as the camera's strategy turns it for a stick (TnOrbitRotation-
//! CameraBehavior.Update: `StickLook`). With LS / RS Button Delay on, a stick's click
//! counts once it has been held 0.2 s (TnPlayerInput's ThumbStickHoldDelay), so a
//! hard push on the stick doesn't change form or punch.

use crate::controls::{Action, Controls, Mode};
use bevy::ecs::system::SystemParam;
use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton};
use bevy::prelude::*;

/// The controller's buttons the game binds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    LT,
    RT,
    /// The sticks' clicks.
    LeftStick,
    RightStick,
    Back,
    Start,
    Up,
    Down,
    Left,
    Right,
}

impl Button {
    const ALL: [Button; 16] = [
        Button::A,
        Button::B,
        Button::X,
        Button::Y,
        Button::LB,
        Button::RB,
        Button::LT,
        Button::RT,
        Button::LeftStick,
        Button::RightStick,
        Button::Back,
        Button::Start,
        Button::Up,
        Button::Down,
        Button::Left,
        Button::Right,
    ];

    fn bit(self) -> u32 {
        1 << self as u32
    }

    /// Its key in the game's bindings.
    pub fn key(self) -> &'static str {
        match self {
            Button::A => "XboxTypeS_A",
            Button::B => "XboxTypeS_B",
            Button::X => "XboxTypeS_X",
            Button::Y => "XboxTypeS_Y",
            Button::LB => "XboxTypeS_LeftShoulder",
            Button::RB => "XboxTypeS_RightShoulder",
            Button::LT => "XboxTypeS_LeftTrigger",
            Button::RT => "XboxTypeS_RightTrigger",
            Button::LeftStick => "XboxTypeS_LeftThumbstick",
            Button::RightStick => "XboxTypeS_RightThumbstick",
            Button::Back => "XboxTypeS_Back",
            Button::Start => "XboxTypeS_Start",
            Button::Up => "XboxTypeS_DPad_Up",
            Button::Down => "XboxTypeS_DPad_Down",
            Button::Left => "XboxTypeS_DPad_Left",
            Button::Right => "XboxTypeS_DPad_Right",
        }
    }

    /// Bevy's button for it (an Xbox controller's names: South is A).
    fn gamepad(self) -> GamepadButton {
        match self {
            Button::A => GamepadButton::South,
            Button::B => GamepadButton::East,
            Button::X => GamepadButton::West,
            Button::Y => GamepadButton::North,
            Button::LB => GamepadButton::LeftTrigger,
            Button::RB => GamepadButton::RightTrigger,
            Button::LT => GamepadButton::LeftTrigger2,
            Button::RT => GamepadButton::RightTrigger2,
            Button::LeftStick => GamepadButton::LeftThumb,
            Button::RightStick => GamepadButton::RightThumb,
            Button::Back => GamepadButton::Select,
            Button::Start => GamepadButton::Start,
            Button::Up => GamepadButton::DPadUp,
            Button::Down => GamepadButton::DPadDown,
            Button::Left => GamepadButton::DPadLeft,
            Button::Right => GamepadButton::DPadRight,
        }
    }
}

/// XInput's dead zones (XINPUT_GAMEPAD_LEFT_THUMB_DEADZONE, 7849, and RIGHT_, 8689,
/// of 32767): a stick moves nothing until pushed this far over, then goes from
/// nothing to full over the rest of the way.
const LEFT_DEAD_ZONE: f32 = 7849.0 / 32767.0;
const RIGHT_DEAD_ZONE: f32 = 8689.0 / 32767.0;
/// XINPUT_GAMEPAD_TRIGGER_THRESHOLD (30 of 255): a trigger is down past this.
const TRIGGER_THRESHOLD: f32 = 30.0 / 255.0;
/// XButtonInputHandler's ButtonTimeOut: X let go within this is a tap (reload).
const TAP_SECS: f32 = 0.3;
/// TnPlayerInput's ThumbStickHoldDelay: with LS / RS Button Delay on, a stick's
/// click counts once it has been held this long.
const STICK_HOLD_DELAY: f32 = 0.2;

/// This frame's controller, for the game's systems (`read_pad`).
#[derive(Resource, Debug)]
pub struct Pad {
    /// It is the controls (Active Game Controls: Controller, and one is connected):
    /// the keyboard and mouse don't play Jazz.
    pub active: bool,
    /// The sticks past their dead zones (x to the right, y up: -1 to 1).
    pub left: Vec2,
    pub right: Vec2,
    /// The buttons down, gone down this frame (a stick's click once it counts),
    /// let go this frame, and tapped (let go within TAP_SECS): all none while the
    /// controller doesn't play Jazz (not the controls, or the game taking no
    /// orders: the pause menu, Choose Character, the countdown).
    held: u32,
    pressed: u32,
    released: u32,
    tapped: u32,
    /// The buttons as they are, whether or not it plays Jazz (so one held through
    /// the pause menu doesn't go down again as it closes), when each went down, and
    /// the stick clicks already counted.
    raw: u32,
    since: [f32; 16],
    clicked: u32,
    /// The controls' commands with the Controller Layout's schemes (each button's,
    /// padlayout.rs), the schemes they are for, and whether the sticks are swapped
    /// (Southpaw thumbsticks: the left stick looks, the right moves).
    commands: [String; 16],
    layout: Option<[usize; 3]>,
    swap_sticks: bool,
}

impl Default for Pad {
    /// No controller yet, its controls laid out by the default layout.
    fn default() -> Self {
        let mut pad = Pad {
            active: false,
            left: Vec2::ZERO,
            right: Vec2::ZERO,
            held: 0,
            pressed: 0,
            released: 0,
            tapped: 0,
            raw: 0,
            since: [0.0; 16],
            clicked: 0,
            commands: Default::default(),
            layout: None,
            swap_sticks: false,
        };
        pad.lay_out([0; 3]);
        pad
    }
}

impl Pad {
    pub fn held(&self, b: Button) -> bool {
        self.held & b.bit() != 0
    }

    pub fn pressed(&self, b: Button) -> bool {
        self.pressed & b.bit() != 0
    }

    /// Its controls laid out by the Controller Layout's schemes (`chosen`: each
    /// row's).
    fn lay_out(&mut self, chosen: [usize; 3]) {
        if self.layout == Some(chosen) {
            return;
        }
        let bindings = crate::padlayout::bindings(chosen);
        let command = |key: &str| {
            bindings
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, c)| c.clone())
                .unwrap_or_default()
        };
        self.commands = Button::ALL.map(|b| command(b.key()));
        self.swap_sticks =
            crate::padlayout::find_command(&command("XboxTypeS_LeftX"), "aTurn").is_some();
        self.layout = Some(chosen);
    }

    /// The buttons bound to `command`.
    fn bound(&self, command: &str) -> u32 {
        Button::ALL
            .iter()
            .filter(|b| {
                crate::padlayout::find_command(&self.commands[**b as usize], command).is_some()
            })
            .fold(0, |bits, b| bits | b.bit())
    }

    /// Takes in the controller as it is this frame: `raw` the buttons down, the
    /// sticks as they lie, at `now` (seconds); `delay`: LS / RS Button Delay;
    /// `playing`: it plays Jazz this frame.
    fn update(&mut self, raw: u32, left: Vec2, right: Vec2, now: f32, delay: bool, playing: bool) {
        let down = raw & !self.raw;
        let up = !raw & self.raw;
        let mut tapped = 0;
        for b in Button::ALL {
            let i = b as usize;
            if down & b.bit() != 0 {
                self.since[i] = now;
            }
            if up & b.bit() != 0 && now - self.since[i] < TAP_SECS {
                tapped |= b.bit();
            }
        }
        // A stick's click: as it goes down, or once held STICK_HOLD_DELAY with the
        // delay on; once.
        let wait = if delay { STICK_HOLD_DELAY } else { 0.0 };
        let mut clicks = 0;
        for b in [Button::LeftStick, Button::RightStick] {
            if raw & b.bit() == 0 {
                self.clicked &= !b.bit();
            } else if self.clicked & b.bit() == 0 && now - self.since[b as usize] >= wait {
                self.clicked |= b.bit();
                clicks |= b.bit();
            }
        }
        let sticks = Button::LeftStick.bit() | Button::RightStick.bit();
        self.raw = raw;
        if playing {
            self.held = raw;
            self.pressed = (down & !sticks) | clicks;
            self.released = up;
            self.tapped = tapped;
            // (Each stick past its own dead zone, then the one that moves and the
            // one that looks.)
            let (left, right) = (
                dead_zone(left, LEFT_DEAD_ZONE),
                dead_zone(right, RIGHT_DEAD_ZONE),
            );
            (self.left, self.right) = if self.swap_sticks {
                (right, left)
            } else {
                (left, right)
            };
        } else {
            self.held = 0;
            self.pressed = 0;
            self.released = 0;
            self.tapped = 0;
            self.left = Vec2::ZERO;
            self.right = Vec2::ZERO;
        }
    }
}

/// A stick past its dead zone `dz` (round about its middle), stretched over the rest
/// of the way to full.
fn dead_zone(v: Vec2, dz: f32) -> Vec2 {
    let m = v.length();
    if m <= dz {
        Vec2::ZERO
    } else {
        v / m * ((m - dz) / (1.0 - dz)).min(1.0)
    }
}

/// The split screen guest's controller (split.rs), as `Pad` is the first
/// player's: always his controls (he has no keyboard and mouse), while it is
/// connected.
#[derive(Resource, Default, Debug, Deref, DerefMut)]
pub struct GuestPad(pub Pad);

/// This PC's players' controllers, by seat (split.rs).
#[derive(SystemParam)]
pub struct Pads<'w> {
    pub first: Res<'w, Pad>,
    pub guest: Res<'w, GuestPad>,
}

impl Pads<'_> {
    /// Seat `seat`'s controller.
    pub fn seat(&self, seat: usize) -> &Pad {
        if seat == 1 {
            &self.guest.0
        } else {
            &self.first
        }
    }
}

/// Reads the controllers into `Pad` (the first player's: the first connected that
/// isn't the guest's) and `GuestPad` (split screen's guest's), after the pause
/// menu has put the keys aside (pause_game_input): while the game takes no orders
/// the controllers give none either.
#[allow(clippy::too_many_arguments)]
pub fn read_pad(
    time: Res<Time>,
    controls: Res<Controls>,
    pause: Res<crate::menu::Pause>,
    pre: Res<crate::pregame::PreMatch>,
    split: Res<crate::split::Split>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut pad: ResMut<Pad>,
    mut guest: ResMut<GuestPad>,
) {
    let now = time.elapsed_secs();
    let taking_orders = !(pause.open || pause.choosing || pause.just_closed || pre.waiting);
    let gamepad = crate::split::first_pad(&split, gamepads.iter()).map(|(_, g)| g);
    pad.lay_out(controls.pad_schemes);
    pad.active = controls.controller && gamepad.is_some();
    let (raw, left, right) = gamepad.map_or((0, Vec2::ZERO, Vec2::ZERO), read_gamepad);
    let playing = pad.active && taking_orders;
    pad.update(raw, left, right, now, controls.stick_delay, playing);
    // The guest's.
    let gamepad = split
        .guest_pad()
        .and_then(|e| gamepads.get(e).ok())
        .map(|(_, g)| g);
    guest.lay_out(controls.pad_schemes);
    guest.active = gamepad.is_some();
    let (raw, left, right) = gamepad.map_or((0, Vec2::ZERO, Vec2::ZERO), read_gamepad);
    let playing = guest.active && taking_orders;
    guest.update(raw, left, right, now, controls.stick_delay, playing);
}

/// A controller's buttons down (a trigger down past its threshold) and its sticks.
fn read_gamepad(g: &Gamepad) -> (u32, Vec2, Vec2) {
    let mut raw = 0;
    for b in Button::ALL {
        let down = match b {
            Button::LT | Button::RT => trigger(g, b) > TRIGGER_THRESHOLD,
            _ => g.pressed(b.gamepad()),
        };
        if down {
            raw |= b.bit();
        }
    }
    (raw, g.left_stick(), g.right_stick())
}

/// A trigger's pull (0 to 1): its button's value, or its axis's on a controller that
/// has its triggers as axes.
fn trigger(g: &Gamepad, b: Button) -> f32 {
    let axis = if b == Button::LT {
        GamepadAxis::LeftZ
    } else {
        GamepadAxis::RightZ
    };
    g.get(b.gamepad())
        .unwrap_or(0.0)
        .max(g.get(axis).unwrap_or(0.0))
}

/// The command `action` is in `mode` (TnPlayerInput's, as the bindings name them),
/// if the controller has it there. (A car jumps with PressJump, a jet climbs with
/// Ascend: apart only with Seeker's.)
fn command(mode: Mode, action: Action) -> Option<&'static str> {
    let robot = mode != Mode::Vehicle;
    Some(match action {
        Action::SwapWeapons if robot => "NextWeapon",
        Action::Fire => "Fire",
        Action::Melee if robot => "Melee",
        Action::FineAim if robot => "FineAim",
        Action::SpeedBoost if !robot => "Boost",
        Action::DashSprint if robot => "Ability1",
        Action::Jump => "PressJump",
        Action::JumpAscend if !robot => "Ascend",
        Action::Descend if !robot => "Descend",
        Action::Ability => "Ability0",
        Action::Interact if robot => "UseInteract",
        Action::ChangeForm => "Transform",
        Action::SwitchCamera if robot => "ChangeHandedness",
        Action::Reload => "Reload",
        Action::Pause => "showmenu",
        Action::AssaultTech if robot => "OffensiveEquipment",
        Action::UtilityTech if robot => "DefensiveEquipment",
        Action::Scoreboard => "ShowScores",
        _ => return None,
    })
}

impl Pad {
    /// The buttons `action` is on in `mode`.
    fn buttons(&self, mode: Mode, action: Action) -> u32 {
        command(mode, action).map_or(0, |c| self.bound(c))
    }

    /// `action` held down in `mode`.
    pub fn action_held(&self, mode: Mode, action: Action) -> bool {
        self.held & self.buttons(mode, action) != 0
    }

    /// `action` asked for this frame: its button gone down. Reload on a button
    /// with X's handler (XButton: X by default) is a tap, as XButtonInputHandler
    /// takes one; held, that button interacts and picks up.
    pub fn action_pressed(&self, mode: Mode, action: Action) -> bool {
        let buttons = self.buttons(mode, action);
        if action == Action::Reload {
            let tapped = buttons & self.bound("XButton");
            (self.tapped & tapped) | (self.pressed & (buttons & !tapped)) != 0
        } else {
            self.pressed & buttons != 0
        }
    }

    /// `action`'s button let go this frame.
    pub fn action_released(&self, mode: Mode, action: Action) -> bool {
        self.released & self.buttons(mode, action) != 0
    }

    /// The control `action` is on in `mode` (for the HUD's prompts), if it has one.
    pub fn control_name(&self, mode: Mode, action: Action) -> Option<&'static str> {
        let (look, moves) = if self.swap_sticks {
            ("LEFT STICK", "RIGHT STICK")
        } else {
            ("RIGHT STICK", "LEFT STICK")
        };
        match action {
            Action::Move => Some(moves),
            Action::TurnLook | Action::TurnSteer => Some(look),
            _ => {
                let bits = self.buttons(mode, action);
                Button::ALL
                    .iter()
                    .find(|b| bits & b.bit() != 0)
                    .map(|b| b.name())
            }
        }
    }
}

impl Button {
    /// Its name on the HUD.
    fn name(self) -> &'static str {
        match self {
            Button::A => "A",
            Button::B => "B",
            Button::X => "X",
            Button::Y => "Y",
            Button::LB => "LB",
            Button::RB => "RB",
            Button::LT => "LT",
            Button::RT => "RT",
            Button::LeftStick => "LS",
            Button::RightStick => "RS",
            Button::Back => "BACK",
            Button::Start => "START",
            Button::Up => "D-PAD UP",
            Button::Down => "D-PAD DOWN",
            Button::Left => "D-PAD LEFT",
            Button::Right => "D-PAD RIGHT",
        }
    }
}

/// A camera strategy's look speeds for a stick (TnOrbitRotationCameraBehavior's
/// LookSpeedSettings): degrees a second at full stick across (YawSpeed) and up and
/// down (PitchSpeed), and how many times more again a stick held well over makes it
/// at most (RampSpeedMultiplier).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LookSpeed {
    pub yaw: f32,
    pub pitch: f32,
    pub ramp: f32,
}

/// The robot's over-the-shoulder strategy's: its DefaultLookSpeed, and its
/// LookSpeedsByPCS fine aiming (with the Nucleon Charge Rifle's scope,
/// TnPCS_SniperRifle; with the Photon Burst Rifle's, TnPCS_BurstRifle). Sprinting,
/// its class's strategy's own (character::SprintCamera's look).
pub const ROBOT_LOOK: LookSpeed = LookSpeed {
    yaw: 50.0,
    pitch: 25.0,
    ramp: 2.5,
};
pub const FINE_AIM_LOOK: LookSpeed = LookSpeed {
    yaw: 25.0,
    pitch: 15.0,
    ramp: 2.5,
};
pub const SNIPER_LOOK: LookSpeed = LookSpeed {
    yaw: 6.5,
    pitch: 3.25,
    ramp: 2.5,
};
pub const BURST_LOOK: LookSpeed = LookSpeed {
    yaw: 14.0,
    pitch: 8.0,
    ramp: 2.5,
};
/// A hovering car's or truck's (HoverCar_MP_STRATEGY's, HoverTruck_MP_STRATEGY's, as
/// Escalation's HoverCar_Bumblebee_STRATEGY's and HoverTruck_Optimus_STRATEGY's; a
/// jet's are its own: jet.rs's HOVER_LOOK), and a tank's (MP_Tank_STRATEGY's).
pub const HOVER_LOOK: LookSpeed = LookSpeed {
    yaw: 50.0,
    pitch: 25.0,
    ramp: 2.5,
};
pub const TANK_LOOK: LookSpeed = LookSpeed {
    yaw: 45.0,
    pitch: 15.0,
    ramp: 2.5,
};
/// The game's mouse turn of the view in one of its frames (HmPlayerInput.
/// CalculateCameraRotation): the counts by MouseSensitivity (60 at the demo's usual
/// Mouse Sensitivity) and the field of view over 90 (PlayerInput.
/// AdjustMouseSensitivity), times MouseCameraSensitivityX and Y (0.00114286): a
/// vehicle's camera's input under the mouse.
const MOUSE_SENSITIVITY: f32 = 60.0;
const MOUSE_CAMERA_SENSITIVITY: f32 = 0.001_142_86;
/// The most of it a vehicle's camera takes under the mouse
/// (TnOrbitRotationCameraBehavior.AdjustStickInputForMouseControl: ±1.5; boosting,
/// TnPlayerController.AdjustStickInputForVehicleUnderMouseControl's ±1, after its
/// StickTimeAdjustment).
const MOUSE_MOST: f32 = 1.5;
const MOUSE_MOST_BOOSTING: f32 = 1.0;

/// The view's turn in a vehicle under the mouse, as the game's camera turns it
/// (TnOrbitRotationCameraBehavior.Update, a vehicle's: hovering, and a tank
/// boosting): the mouse's camera input this frame (its counts `delta`, x to the
/// right and y up; `scale` the Mouse Sensitivity's, `fov` the view's, degrees) as
/// the game would read it in one of its frames, held within 1.5 either way (1
/// `boosting`, scaled down too should a frame take over 0.05 s), turns the view at
/// the strategy's `look` speeds times Camera Sensitivity's multiplier (0 to 100:
/// Low- to HighSensitivityMultiplier, 0.5 to 2) and one more than its
/// RampSpeedMultiplier (GetCurrentSpeedMultiplier, under the mouse always in full)
/// a second. However fast the mouse goes, the view turns no faster than that: at
/// the usual Camera Sensitivity a hovering car's or truck's 328 degrees a second
/// across, a tank's 295 (197 boosting), a hovering jet's 262.5. Returns degrees (to
/// the right, up).
pub fn vehicle_mouse_turn(
    delta: Vec2,
    look: LookSpeed,
    boosting: bool,
    scale: f32,
    fov: f32,
    sensitivity: u32,
    dt: f32,
) -> Vec2 {
    if dt <= 0.0 {
        return Vec2::ZERO;
    }
    let per_game_frame = delta / dt / 60.0;
    let mut input =
        per_game_frame * MOUSE_SENSITIVITY * scale * fov / 90.0 * MOUSE_CAMERA_SENSITIVITY;
    let most = if boosting {
        // (StickTimeAdjustment: 0.05 s over the frame's, cubed, from 0.05 to 1.)
        input *= (0.05 / dt).powi(3).clamp(0.05, 1.0);
        MOUSE_MOST_BOOSTING
    } else {
        MOUSE_MOST
    };
    let input = input.clamp(Vec2::splat(-most), Vec2::splat(most));
    let s = sensitivity.min(100) as f32 / 100.0;
    let k = (LOW_SENSITIVITY + (HIGH_SENSITIVITY - LOW_SENSITIVITY) * s) * (1.0 + look.ramp) * dt;
    Vec2::new(input.x * look.yaw * k, input.y * look.pitch * k)
}

/// RampTime: how long a stick held all the way over takes to bring the ramp in fully
/// (the robot's strategy's; the vehicles').
pub const ROBOT_RAMP_SECS: f32 = 0.4;
pub const VEHICLE_RAMP_SECS: f32 = 0.3;
/// RampZone: the ramp builds while the stick is further over than this (and stops
/// at once when it isn't, or when it jumps by MaxInputChange in a frame).
const RAMP_ZONE: f32 = 0.7;
const MAX_INPUT_CHANGE: f32 = 1.0;
/// SpeedTransitionTime: new look speeds (fine aim coming up, say) blend in over this.
const SPEED_TRANSITION_SECS: f32 = 0.5;
/// Low/HighSensitivityMultiplier: Camera Sensitivity from 0 to 100 turns the view
/// from this many times as fast to this many.
const LOW_SENSITIVITY: f32 = 0.5;
const HIGH_SENSITIVITY: f32 = 2.0;

/// The right stick's turn of the view (TnOrbitRotationCameraBehavior.Update): its
/// look speeds now (blending to new ones: UpdateCurrentLookSpeed), its ramp
/// (TnOrbitSpeedRamper) and the stick last frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct StickLook {
    current: Option<LookSpeed>,
    target: Option<LookSpeed>,
    blend_secs: f32,
    ramp: f32,
    last: Vec2,
}

impl StickLook {
    /// This frame's turn (degrees: across, to the right; up) for `stick` (x to the
    /// right, y up) over `dt` seconds, with the look speeds `speeds`, the ramp
    /// building over `ramp_secs`, at Camera Sensitivity `sensitivity` (0 to 100).
    pub fn turn(
        &mut self,
        stick: Vec2,
        speeds: LookSpeed,
        ramp_secs: f32,
        sensitivity: u32,
        dt: f32,
    ) -> Vec2 {
        if self.target != Some(speeds) {
            self.target = Some(speeds);
            self.blend_secs = 0.0;
        }
        self.blend_secs += dt;
        let k = (self.blend_secs / SPEED_TRANSITION_SECS).min(1.0);
        let now = self.current.get_or_insert(speeds);
        now.yaw += (speeds.yaw - now.yaw) * k;
        now.pitch += (speeds.pitch - now.pitch) * k;
        now.ramp += (speeds.ramp - now.ramp) * k;
        let speeds = *now;
        // (TnOrbitSpeedRamper.UpdateRampPercentage.)
        let change = (stick - self.last).length();
        let m = stick.length();
        self.ramp = if change < MAX_INPUT_CHANGE && m > RAMP_ZONE {
            if ramp_secs <= 0.0 {
                1.0
            } else {
                (self.ramp + dt / ramp_secs * (m - RAMP_ZONE) / (1.0 - RAMP_ZONE)).min(1.0)
            }
        } else {
            0.0
        };
        self.last = stick;
        let s = sensitivity.min(100) as f32 / 100.0;
        let scale = LOW_SENSITIVITY + (HIGH_SENSITIVITY - LOW_SENSITIVITY) * s;
        let k = scale * (1.0 + speeds.ramp * self.ramp) * dt;
        Vec2::new(stick.x * speeds.yaw * k, stick.y * speeds.pitch * k)
    }

    /// The stick let go (out of the controller's hands): the ramp starts again.
    pub fn rest(&mut self) {
        self.ramp = 0.0;
        self.last = Vec2::ZERO;
    }
}

pub struct PadPlugin;

impl Plugin for PadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pad>().init_resource::<GuestPad>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In a vehicle, the mouse turns the view as fast as it goes until the game's
    /// vehicle camera's top speed (its strategy's look speeds at an input of 1.5,
    /// times 1.25 at the usual Camera Sensitivity and one more than its ramp: a
    /// hovering car's or truck's 328 degrees a second across, a tank's 295, a
    /// hovering jet's 262.5 across and 131.25 up and down), whatever the frame rate;
    /// a tank boosting, at an input of 1 (197).
    #[test]
    fn a_vehicles_view_turns_no_faster_than_the_games() {
        let frame = 1.0 / 60.0;
        let jet = crate::jet::HOVER_LOOK;
        let turn = |d: Vec2, look: LookSpeed, boost: bool, sens: u32, dt: f32| {
            vehicle_mouse_turn(d, look, boost, 1.0, 80.0, sens, dt)
        };
        // A slow mouse: 2 counts a frame, at the usual Mouse Sensitivity and the
        // view's 80 degrees: 0.12 of the camera's input, 21 degrees a second (a jet).
        let slow = turn(Vec2::new(2.0, 0.0), jet, false, 50, frame);
        let input = 2.0 * 60.0 * 80.0 / 90.0 * MOUSE_CAMERA_SENSITIVITY;
        assert!(
            (slow.x / frame - input * 70.0 * 1.25 * 2.0).abs() < 1e-3,
            "{slow:?}"
        );
        assert_eq!(slow.y, 0.0);
        // Twice as fast, twice the turn.
        let twice = turn(Vec2::new(4.0, 0.0), jet, false, 50, frame);
        assert!((twice.x - 2.0 * slow.x).abs() < 1e-5);
        // A flick: held to the top speeds, across and up and down.
        let flick = turn(Vec2::new(600.0, 600.0), jet, false, 50, frame);
        assert!((flick.x / frame - 262.5).abs() < 1e-2, "{flick:?}");
        assert!((flick.y / frame - 131.25).abs() < 1e-2, "{flick:?}");
        let back = turn(Vec2::new(-6000.0, 0.0), jet, false, 50, frame);
        assert!((back.x / frame + 262.5).abs() < 1e-2);
        let car = turn(Vec2::new(600.0, 600.0), HOVER_LOOK, false, 50, frame);
        assert!((car.x / frame - 328.125).abs() < 1e-2, "{car:?}");
        assert!((car.y / frame - 164.0625).abs() < 1e-2, "{car:?}");
        let tank = turn(Vec2::new(600.0, 0.0), TANK_LOOK, false, 50, frame);
        assert!((tank.x / frame - 295.3125).abs() < 1e-2, "{tank:?}");
        let boosting = turn(Vec2::new(600.0, 0.0), TANK_LOOK, true, 50, frame);
        assert!((boosting.x / frame - 196.875).abs() < 1e-2, "{boosting:?}");
        // (Boosting at 10 frames a second: the input an eighth of itself.)
        let slow_frames = turn(Vec2::new(2.0, 0.0) * 6.0, TANK_LOOK, true, 50, 0.1);
        let fine = turn(Vec2::new(2.0, 0.0), TANK_LOOK, false, 50, frame);
        assert!((slow_frames.x / 0.1 - fine.x / frame / 8.0).abs() < 1e-3);
        // The same mouse at 120 frames a second: half the counts a frame, the same
        // turn a second.
        let fast_frames = turn(Vec2::new(1.0, 0.0), jet, false, 50, frame / 2.0);
        assert!((fast_frames.x / (frame / 2.0) - slow.x / frame).abs() < 1e-3);
        // Camera Sensitivity at its top: four times its bottom's (2 against 0.5).
        let hi = turn(Vec2::new(600.0, 0.0), HOVER_LOOK, false, 100, frame);
        let lo = turn(Vec2::new(600.0, 0.0), HOVER_LOOK, false, 0, frame);
        assert!((hi.x / lo.x - 4.0).abs() < 1e-4);
        assert_eq!(turn(Vec2::new(5.0, 5.0), jet, false, 50, 0.0), Vec2::ZERO);
    }

    fn bits(buttons: &[Button]) -> u32 {
        buttons.iter().fold(0, |m, b| m | b.bit())
    }

    #[test]
    fn sticks_have_dead_zones() {
        assert_eq!(dead_zone(Vec2::new(0.2, 0.0), LEFT_DEAD_ZONE), Vec2::ZERO);
        let half = dead_zone(Vec2::new(0.0, 0.62), LEFT_DEAD_ZONE);
        assert!(half.x == 0.0 && (half.y - 0.5).abs() < 0.01, "{half}");
        assert!((dead_zone(Vec2::new(1.0, 0.0), RIGHT_DEAD_ZONE).x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn x_tapped_reloads_and_held_picks_up() {
        let mut pad = Pad::default();
        pad.update(bits(&[Button::X]), Vec2::ZERO, Vec2::ZERO, 1.0, false, true);
        assert!(pad.pressed(Button::X) && pad.held(Button::X));
        assert!(pad.action_held(Mode::Robot, Action::Interact));
        assert!(!pad.action_pressed(Mode::Robot, Action::Reload));
        pad.update(0, Vec2::ZERO, Vec2::ZERO, 1.2, false, true);
        assert!(
            pad.action_pressed(Mode::Robot, Action::Reload),
            "let go within 0.3 s"
        );
        pad.update(bits(&[Button::X]), Vec2::ZERO, Vec2::ZERO, 2.0, false, true);
        pad.update(0, Vec2::ZERO, Vec2::ZERO, 2.5, false, true);
        assert!(
            pad.action_released(Mode::Robot, Action::Interact)
                && !pad.action_pressed(Mode::Robot, Action::Reload),
            "held 0.5 s"
        );
    }

    #[test]
    fn stick_clicks_wait_with_the_delay() {
        let mut pad = Pad::default();
        let click = bits(&[Button::LeftStick]);
        pad.update(click, Vec2::ZERO, Vec2::ZERO, 1.0, true, true);
        assert!(!pad.pressed(Button::LeftStick));
        pad.update(click, Vec2::ZERO, Vec2::ZERO, 1.1, true, true);
        assert!(!pad.pressed(Button::LeftStick));
        pad.update(click, Vec2::ZERO, Vec2::ZERO, 1.25, true, true);
        assert!(pad.pressed(Button::LeftStick), "held 0.25 s");
        pad.update(click, Vec2::ZERO, Vec2::ZERO, 1.3, true, true);
        assert!(!pad.pressed(Button::LeftStick), "counted once");
        // A short push with the delay counts for nothing; without it, at once.
        pad.update(0, Vec2::ZERO, Vec2::ZERO, 2.0, true, true);
        pad.update(click, Vec2::ZERO, Vec2::ZERO, 2.1, true, true);
        pad.update(0, Vec2::ZERO, Vec2::ZERO, 2.15, true, true);
        assert!(!pad.pressed(Button::LeftStick));
        pad.update(click, Vec2::ZERO, Vec2::ZERO, 3.0, false, true);
        assert!(pad.pressed(Button::LeftStick));
    }

    #[test]
    fn nothing_while_it_does_not_play() {
        let mut pad = Pad::default();
        // A held through the pause menu doesn't go down again as it closes.
        pad.update(bits(&[Button::A]), Vec2::X, Vec2::X, 1.0, false, false);
        assert!(!pad.held(Button::A) && pad.left == Vec2::ZERO);
        pad.update(bits(&[Button::A]), Vec2::X, Vec2::X, 1.1, false, true);
        assert!(pad.held(Button::A) && !pad.pressed(Button::A));
        assert!(pad.left.x > 0.99);
    }

    #[test]
    fn the_view_turns_as_the_game_turns_it() {
        let mut look = StickLook::default();
        // A full stick at the middle Camera Sensitivity: 50 degrees a second across at
        // 1.25 times (pushed all the way over at once: no ramp yet), the ramp
        // building over 0.4 s to 3.5 times that.
        let first = look.turn(Vec2::X, ROBOT_LOOK, ROBOT_RAMP_SECS, 50, 0.1);
        assert!((first.x - 50.0 * 1.25 * 0.1).abs() < 1e-3, "{first}");
        let second = look.turn(Vec2::X, ROBOT_LOOK, ROBOT_RAMP_SECS, 50, 0.1);
        assert!(
            (second.x - 50.0 * 1.25 * (1.0 + 2.5 * 0.25) * 0.1).abs() < 1e-3,
            "{second}"
        );
        for _ in 0..10 {
            look.turn(Vec2::X, ROBOT_LOOK, ROBOT_RAMP_SECS, 50, 0.1);
        }
        let full = look.turn(Vec2::X, ROBOT_LOOK, ROBOT_RAMP_SECS, 50, 0.1);
        assert!((full.x - 50.0 * 1.25 * 3.5 * 0.1).abs() < 1e-3, "{full}");
        // Half over, no ramp; and up and down at its own speed.
        let mut look = StickLook::default();
        let half = look.turn(Vec2::new(0.0, 0.5), ROBOT_LOOK, ROBOT_RAMP_SECS, 0, 0.1);
        assert!((half.y - 0.5 * 25.0 * 0.5 * 0.1).abs() < 1e-4, "{half}");
    }

    #[test]
    fn fine_aim_blends_in() {
        let mut look = StickLook::default();
        look.turn(Vec2::new(0.5, 0.0), ROBOT_LOOK, ROBOT_RAMP_SECS, 0, 0.1);
        let early = look.turn(Vec2::new(0.5, 0.0), FINE_AIM_LOOK, ROBOT_RAMP_SECS, 0, 0.1);
        let mut later = Vec2::ZERO;
        for _ in 0..10 {
            later = look.turn(Vec2::new(0.5, 0.0), FINE_AIM_LOOK, ROBOT_RAMP_SECS, 0, 0.1);
        }
        let want = 0.5 * 25.0 * 0.5 * 0.1;
        assert!(
            early.x > want + 0.05 && (later.x - want).abs() < 1e-4,
            "{early} {later}"
        );
    }

    #[test]
    fn the_layout_names_its_controls() {
        let pad = Pad::default();
        assert_eq!(pad.control_name(Mode::Robot, Action::Interact), Some("X"));
        assert_eq!(pad.control_name(Mode::Robot, Action::Ability), Some("RB"));
        assert_eq!(
            pad.control_name(Mode::Robot, Action::AssaultTech),
            Some("D-PAD LEFT")
        );
        assert_eq!(
            pad.control_name(Mode::Vehicle, Action::SpeedBoost),
            Some("LT")
        );
        assert_eq!(pad.control_name(Mode::Vehicle, Action::SwapWeapons), None);
        assert_eq!(
            pad.control_name(Mode::Robot, Action::Move),
            Some("LEFT STICK")
        );
    }

    /// The Controller Layout's schemes move the actions to their buttons, and
    /// Southpaw thumbsticks swap the sticks.
    #[test]
    fn the_schemes_move_the_actions() {
        let press = |chosen: [usize; 3], buttons: &[Button]| {
            let mut pad = Pad::default();
            pad.lay_out(chosen);
            pad.update(bits(buttons), Vec2::ZERO, Vec2::ZERO, 1.0, false, true);
            pad
        };
        // Tactician: B punches, RS switches hands.
        let pad = press([1, 0, 0], &[Button::B]);
        assert!(pad.action_pressed(Mode::Robot, Action::Melee));
        assert!(!pad.action_pressed(Mode::Robot, Action::SwitchCamera));
        assert_eq!(
            pad.control_name(Mode::Robot, Action::SwitchCamera),
            Some("RS")
        );
        // Striker: Y changes form, the d-pad's up swaps weapons, LS does nothing.
        let pad = press([2, 0, 0], &[Button::Y, Button::Up]);
        assert!(pad.action_pressed(Mode::Robot, Action::ChangeForm));
        assert!(pad.action_pressed(Mode::Robot, Action::SwapWeapons));
        assert_eq!(pad.control_name(Mode::Robot, Action::ChangeForm), Some("Y"));
        // Charger: LS dashes and sprints, LB changes form.
        let pad = press([3, 0, 0], &[Button::LB]);
        assert!(pad.action_pressed(Mode::Robot, Action::ChangeForm));
        assert!(!pad.action_held(Mode::Robot, Action::DashSprint));
        // Southpaw: LT fires, RT aims (and boosts the car), LB the ability.
        let pad = press([4, 0, 0], &[Button::LT, Button::LB]);
        assert!(pad.action_held(Mode::Robot, Action::Fire));
        assert!(!pad.action_held(Mode::Robot, Action::FineAim));
        assert!(pad.action_held(Mode::Robot, Action::Ability));
        assert_eq!(
            pad.control_name(Mode::Vehicle, Action::SpeedBoost),
            Some("RT")
        );
        // Hoarder: X changes form; the d-pad's up interacts held, reloads tapped.
        let mut pad = press([5, 0, 0], &[Button::X, Button::Up]);
        assert!(pad.action_pressed(Mode::Robot, Action::ChangeForm));
        assert!(pad.action_held(Mode::Robot, Action::Interact));
        pad.update(0, Vec2::ZERO, Vec2::ZERO, 1.1, false, true);
        assert!(pad.action_pressed(Mode::Robot, Action::Reload));
        // Seeker: a jet climbs on B and dives on A; a car still jumps on A.
        let pad = press([0, 1, 0], &[Button::A]);
        assert!(pad.action_held(Mode::Vehicle, Action::Descend));
        assert!(!pad.action_held(Mode::Vehicle, Action::JumpAscend));
        assert!(pad.action_pressed(Mode::Vehicle, Action::Jump));
        let pad = press([0, 1, 0], &[Button::B]);
        assert!(pad.action_held(Mode::Vehicle, Action::JumpAscend));
        // Southpaw thumbsticks: the right stick moves, the left looks.
        let mut pad = Pad::default();
        pad.lay_out([0, 0, 1]);
        pad.update(0, Vec2::X, Vec2::Y, 1.0, false, true);
        assert_eq!((pad.left, pad.right), (Vec2::Y, Vec2::X));
        assert_eq!(
            pad.control_name(Mode::Robot, Action::Move),
            Some("RIGHT STICK")
        );
    }

    #[test]
    fn actions_go_by_the_form() {
        let mut pad = Pad::default();
        pad.update(
            bits(&[Button::B, Button::A]),
            Vec2::ZERO,
            Vec2::ZERO,
            1.0,
            false,
            true,
        );
        // B switches hands on foot, dives in a jet; A jumps, and climbs.
        assert!(pad.action_pressed(Mode::Robot, Action::SwitchCamera));
        assert!(!pad.action_held(Mode::Robot, Action::Descend));
        assert!(pad.action_held(Mode::Vehicle, Action::Descend));
        assert!(pad.action_pressed(Mode::Robot, Action::Jump));
        assert!(pad.action_held(Mode::Vehicle, Action::JumpAscend));
        // X: held picks up at once; reload only as a tap ends.
        pad.update(bits(&[Button::X]), Vec2::ZERO, Vec2::ZERO, 2.0, false, true);
        assert!(pad.action_held(Mode::Robot, Action::Interact));
        assert!(!pad.action_pressed(Mode::Robot, Action::Reload));
        pad.update(0, Vec2::ZERO, Vec2::ZERO, 2.1, false, true);
        assert!(pad.action_pressed(Mode::Robot, Action::Reload));
        assert!(pad.action_pressed(Mode::Vehicle, Action::Reload));
    }
}
