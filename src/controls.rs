//! The game's controls, as Fall of Cybertron's PC settings set them (Settings >
//! Controls: UI_GFxSettings_p.SettingsMenu_GFX's ControlsMenuProvider, as its PC
//! version lists it), kept in settings.txt with the other settings.
//!
//! - Active Game Controls: Keyboard/Mouse or Controller (pad.rs: a controller then
//!   plays Jazz, by the game's default controller layout, while one is connected).
//! - Keyboard/Mouse Layout: the game's keyboard layouts (Default and the schemes
//!   TransInput.ini adds, Kbd80ControlScheme to KbdLeftyControlScheme, named in
//!   TransGame.int: Annihilator, Quickdraw, Wheelless, Rebel and Southpaw), each
//!   with its keys for the robot and the vehicle (the screen's View Layout), as
//!   the screen's tables list them (mc_keyboardMouse's DefaultKeyboardLayouts and
//!   getKeyboardLayout; the game's have the campaign's Grimlock's too, which the
//!   demo, with no campaign, leaves out). The game only lets you pick one; here
//!   any key in the tables can also be changed, which makes a Custom layout of
//!   your own (`Controls::assign`).
//! - Controller Layout: the game's control schemes for the controller, one for
//!   each of the robot, the vehicle and the thumbsticks (padlayout.rs), which
//!   pad.rs plays by; its screen shows the controller labelled.
//! - LS / RS Button Delay and Camera Sensitivity (how fast the controller's right
//!   stick turns the view): as the game (pad.rs). Controller Vibration: kept (the
//!   demo's controller doesn't rumble).
//! - Toggle Vehicle Boost (Hold or Toggle), Robot Camera, Vehicle Camera and Jet
//!   Camera (Normal or Inverted: the mouse's or the right stick's up and down) and
//!   Mouse Sensitivity (3 to 120; the game's default, 60, is the demo's usual
//!   speed): as the game.

use bevy::prelude::*;
use std::collections::HashMap;

/// The Keyboard/Mouse Layout screen's View Layout: which form's keys its table
/// shows (KbdControlSchemeViews, without the campaign's Grimlock).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Robot,
    Vehicle,
}

impl Mode {
    pub const ALL: [Mode; 2] = [Mode::Robot, Mode::Vehicle];

    pub fn index(self) -> usize {
        self as usize
    }

    /// Its name on the screen (a UIText.int key and the English).
    pub fn text(self) -> (&'static str, &'static str) {
        match self {
            Mode::Robot => ("Settings.RobotControls", "Robot"),
            Mode::Vehicle => ("Settings.VehicleControls", "Vehicle"),
        }
    }

    /// The actions its table lists, in order.
    pub fn actions(self) -> &'static [Action] {
        match self {
            Mode::Robot => &ROBOT_ACTIONS,
            Mode::Vehicle => &VEHICLE_ACTIONS,
        }
    }

    /// A layout's keys for its actions (Default's for a layout that isn't one of
    /// the game's).
    fn layout(self, layout: usize) -> &'static [Bind] {
        let layout = if layout < LAYOUTS { layout } else { 0 };
        match self {
            Mode::Robot => &ROBOT_LAYOUTS[layout],
            Mode::Vehicle => &VEHICLE_LAYOUTS[layout],
        }
    }

    fn id(self) -> &'static str {
        match self {
            Mode::Robot => "robot",
            Mode::Vehicle => "vehicle",
        }
    }
}

/// What a key does: a row of the Keyboard/Mouse Layout screen's tables.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Action {
    Move,
    TurnLook,
    TurnSteer,
    SwapWeapons,
    Fire,
    Melee,
    FineAim,
    DashSprint,
    Jump,
    JumpAscend,
    Descend,
    SpeedBoost,
    Ability,
    Interact,
    ChangeForm,
    SwitchCamera,
    Reload,
    Pause,
    AssaultTech,
    UtilityTech,
    Scoreboard,
}

impl Action {
    /// Its name in the tables (a UIText.int key and the English).
    pub fn text(self) -> (&'static str, &'static str) {
        match self {
            Action::Move => ("KeyAction.Move", "Move"),
            Action::TurnLook => ("KeyAction.TurnLook", "Turn/Look Around"),
            Action::TurnSteer => ("KeyAction.TurnSteer", "Turn/Steer"),
            Action::SwapWeapons => ("KeyAction.SwapWeapons", "Swap Weapons"),
            Action::Fire => ("KeyAction.Fire", "Fire"),
            Action::Melee => ("KeyAction.Melee", "Melee"),
            Action::FineAim => ("KeyAction.FineAim", "Fine Aim"),
            Action::DashSprint => ("KeyAction.DashSprint", "Dash(Press)/Sprint(Hold)"),
            Action::Jump => ("KeyAction.Jump", "Jump"),
            Action::JumpAscend => ("KeyAction.JumpAscend", "Jump/Ascend (Jet)"),
            Action::Descend => ("KeyAction.Descend", "Descend (Jet)"),
            Action::SpeedBoost => ("KeyAction.SpeedBoost", "Speed Boost"),
            Action::Ability => ("KeyAction.Ability", "Ability"),
            Action::Interact => ("KeyAction.Interact", "Interact/Pick Up"),
            Action::ChangeForm => ("KeyAction.ChangeForm", "Change Form"),
            // (The game's key for it is ChangeHand.)
            Action::SwitchCamera => ("KeyAction.ChangeHand", "Switch Camera"),
            Action::Reload => ("KeyAction.Reload", "Reload Weapon"),
            Action::Pause => ("KeyAction.Pause", "Pause"),
            Action::AssaultTech => ("KeyAction.OffensiveEq", "Assault T.E.C.H. (SP)"),
            Action::UtilityTech => ("KeyAction.DefensiveEq", "Utility T.E.C.H. (SP)"),
            Action::Scoreboard => ("KeyAction.Scoreboard", "Scoreboard (MP)"),
        }
    }

    /// Its name in settings.txt.
    fn id(self) -> &'static str {
        match self {
            Action::Move => "move",
            Action::TurnLook => "turn_look",
            Action::TurnSteer => "turn_steer",
            Action::SwapWeapons => "swap_weapons",
            Action::Fire => "fire",
            Action::Melee => "melee",
            Action::FineAim => "fine_aim",
            Action::DashSprint => "dash_sprint",
            Action::Jump => "jump",
            Action::JumpAscend => "jump_ascend",
            Action::Descend => "descend",
            Action::SpeedBoost => "speed_boost",
            Action::Ability => "ability",
            Action::Interact => "interact",
            Action::ChangeForm => "change_form",
            Action::SwitchCamera => "switch_camera",
            Action::Reload => "reload",
            Action::Pause => "pause",
            Action::AssaultTech => "assault_tech",
            Action::UtilityTech => "utility_tech",
            Action::Scoreboard => "scoreboard",
        }
    }

    /// Looking about goes with the mouse and Pause with Escape whatever the
    /// layout: they can't be changed.
    pub fn fixed(self) -> bool {
        matches!(self, Action::TurnLook | Action::TurnSteer | Action::Pause)
    }

    /// Done by holding its key (so not on the mouse wheel, which can only be
    /// turned): firing, fine aim, sprinting, boosting and picking up.
    fn is_held(self) -> bool {
        matches!(
            self,
            Action::Fire
                | Action::FineAim
                | Action::DashSprint
                | Action::SpeedBoost
                | Action::Interact
        )
    }
}

/// What an action is bound to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bind {
    /// A key (Shift, Control and Alt stand for either one).
    Key(KeyCode),
    Mouse(MouseButton),
    /// The mouse wheel turned either way, or one way.
    Wheel,
    WheelUp,
    WheelDown,
    /// The mouse moving (Turn/Look).
    Motion,
    /// Move's keys: forward, left, back and right (the arrow keys move too).
    Move([KeyCode; 4]),
}

/// A key, button or turn of the wheel pressed to bind it (Controls::assign).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Input {
    Key(KeyCode),
    Mouse(MouseButton),
    WheelUp,
    WheelDown,
}

impl Input {
    fn bind(self) -> Bind {
        match self {
            Input::Key(k) => Bind::Key(k),
            Input::Mouse(b) => Bind::Mouse(b),
            Input::WheelUp => Bind::WheelUp,
            Input::WheelDown => Bind::WheelDown,
        }
    }
}

const fn key(k: KeyCode) -> Bind {
    Bind::Key(k)
}

const fn mouse(b: MouseButton) -> Bind {
    Bind::Mouse(b)
}

const WASD: Bind = Bind::Move([KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD]);
const IJKL: Bind = Bind::Move([KeyCode::KeyI, KeyCode::KeyJ, KeyCode::KeyK, KeyCode::KeyL]);

/// The game's keyboard layouts: Default, then the TransInput.ini schemes
/// Kbd80ControlScheme (Annihilator), Kbd81 (Quickdraw), Kbd82 (Wheelless), Kbd83
/// (Rebel) and KbdLeftyControlScheme (84, Southpaw). Select Layout offers them, and
/// a Custom layout once a key has been changed.
pub const LAYOUTS: usize = 6;
pub const CUSTOM: usize = LAYOUTS;
/// Southpaw's place among them (the game's KbdLeftyControlScheme, 84: the HUD's
/// prompts swap their mouse buttons for it).
pub const SOUTHPAW: usize = 5;

/// A layout's name (a key into menu.txt and the English).
pub fn layout_text(layout: usize) -> (&'static str, &'static str) {
    match layout {
        0 => ("Settings.Default", "Default"),
        1 => ("TransGame.Kbd80ControlScheme.FriendlyName", "Annihilator"),
        2 => ("TransGame.Kbd81ControlScheme.FriendlyName", "Quickdraw"),
        3 => ("TransGame.Kbd82ControlScheme.FriendlyName", "Wheelless"),
        4 => ("TransGame.Kbd83ControlScheme.FriendlyName", "Rebel"),
        5 => ("TransGame.KbdLeftyControlScheme.FriendlyName", "Southpaw"),
        // (Not the game's: it has no layouts of the player's own.)
        _ => ("", "Custom"),
    }
}

const LAYOUT_IDS: [&str; LAYOUTS + 1] = [
    "default",
    "annihilator",
    "quickdraw",
    "wheelless",
    "rebel",
    "southpaw",
    "custom",
];

// The tables, from mc_keyboardMouse (the robot's and the vehicle's actions, and
// each layout's keys for them). In two places the screen's tables don't say what
// the game's bindings do (TransInput.ini's schemes change keys for every form, but
// the vehicle tables don't follow): Annihilator changes form with C in the vehicle
// too (its table says F), and Wheelless shows the scoreboard with 5 in the vehicle
// too (its table says Tab, which is Swap Weapons there). The demo goes by the
// bindings. (The game binds G to TossGrenade too, without listing it:
// Escalation has no grenade of its own, so the demo leaves it out.)

const ROBOT_ACTIONS: [Action; 17] = [
    Action::Move,
    Action::TurnLook,
    Action::SwapWeapons,
    Action::Fire,
    Action::Melee,
    Action::FineAim,
    Action::DashSprint,
    Action::Jump,
    Action::Ability,
    Action::Interact,
    Action::ChangeForm,
    Action::SwitchCamera,
    Action::Reload,
    Action::Pause,
    Action::AssaultTech,
    Action::UtilityTech,
    Action::Scoreboard,
];
const ROBOT_LAYOUTS: [[Bind; 17]; LAYOUTS] = [
    // Default
    [
        WASD,
        Bind::Motion,
        Bind::Wheel,
        mouse(MouseButton::Left),
        mouse(MouseButton::Middle),
        mouse(MouseButton::Right),
        key(KeyCode::ShiftLeft),
        key(KeyCode::Space),
        key(KeyCode::KeyQ),
        key(KeyCode::KeyE),
        key(KeyCode::KeyF),
        key(KeyCode::KeyC),
        key(KeyCode::KeyR),
        key(KeyCode::Escape),
        key(KeyCode::Digit1),
        key(KeyCode::Digit2),
        key(KeyCode::Tab),
    ],
    // Annihilator (scheme 80)
    [
        WASD,
        Bind::Motion,
        Bind::Wheel,
        mouse(MouseButton::Left),
        mouse(MouseButton::Middle),
        mouse(MouseButton::Right),
        key(KeyCode::ShiftLeft),
        key(KeyCode::Space),
        key(KeyCode::KeyE),
        key(KeyCode::KeyF),
        key(KeyCode::KeyC),
        key(KeyCode::KeyX),
        key(KeyCode::KeyR),
        key(KeyCode::Escape),
        key(KeyCode::Digit1),
        key(KeyCode::Digit2),
        key(KeyCode::Tab),
    ],
    // Quickdraw (81)
    [
        WASD,
        Bind::Motion,
        Bind::Wheel,
        mouse(MouseButton::Left),
        mouse(MouseButton::Middle),
        mouse(MouseButton::Right),
        key(KeyCode::ShiftLeft),
        key(KeyCode::Space),
        key(KeyCode::KeyR),
        key(KeyCode::KeyC),
        key(KeyCode::KeyF),
        key(KeyCode::KeyQ),
        key(KeyCode::KeyE),
        key(KeyCode::Escape),
        key(KeyCode::Digit1),
        key(KeyCode::Digit2),
        key(KeyCode::Tab),
    ],
    // Wheelless (82)
    [
        WASD,
        Bind::Motion,
        key(KeyCode::Tab),
        mouse(MouseButton::Left),
        key(KeyCode::KeyV),
        mouse(MouseButton::Right),
        key(KeyCode::ShiftLeft),
        key(KeyCode::Space),
        key(KeyCode::KeyQ),
        key(KeyCode::KeyE),
        key(KeyCode::KeyF),
        key(KeyCode::KeyC),
        key(KeyCode::KeyR),
        key(KeyCode::Escape),
        key(KeyCode::Digit1),
        key(KeyCode::Digit2),
        key(KeyCode::Digit5),
    ],
    // Rebel (83)
    [
        WASD,
        Bind::Motion,
        Bind::Wheel,
        mouse(MouseButton::Left),
        key(KeyCode::KeyE),
        mouse(MouseButton::Right),
        key(KeyCode::ShiftLeft),
        key(KeyCode::Space),
        key(KeyCode::KeyT),
        key(KeyCode::KeyC),
        key(KeyCode::KeyF),
        mouse(MouseButton::Middle),
        key(KeyCode::KeyR),
        key(KeyCode::Escape),
        key(KeyCode::Digit1),
        key(KeyCode::Digit2),
        key(KeyCode::Tab),
    ],
    // Southpaw (84)
    [
        IJKL,
        Bind::Motion,
        Bind::Wheel,
        mouse(MouseButton::Right),
        mouse(MouseButton::Middle),
        mouse(MouseButton::Left),
        key(KeyCode::Slash),
        key(KeyCode::Space),
        key(KeyCode::KeyO),
        key(KeyCode::KeyU),
        key(KeyCode::KeyH),
        key(KeyCode::KeyN),
        key(KeyCode::KeyY),
        key(KeyCode::Escape),
        key(KeyCode::Digit9),
        key(KeyCode::Digit0),
        key(KeyCode::Tab),
    ],
];
const VEHICLE_ACTIONS: [Action; 11] = [
    Action::Move,
    Action::TurnSteer,
    Action::Fire,
    Action::SpeedBoost,
    Action::JumpAscend,
    Action::Descend,
    Action::Ability,
    Action::ChangeForm,
    Action::Reload,
    Action::Pause,
    Action::Scoreboard,
];
const VEHICLE_LAYOUTS: [[Bind; 11]; LAYOUTS] = [
    // Default
    [
        WASD,
        Bind::Motion,
        mouse(MouseButton::Left),
        mouse(MouseButton::Right),
        key(KeyCode::Space),
        key(KeyCode::ControlLeft),
        key(KeyCode::KeyQ),
        key(KeyCode::KeyF),
        key(KeyCode::KeyR),
        key(KeyCode::Escape),
        key(KeyCode::Tab),
    ],
    // Annihilator (scheme 80)
    [
        WASD,
        Bind::Motion,
        mouse(MouseButton::Left),
        mouse(MouseButton::Right),
        key(KeyCode::Space),
        key(KeyCode::ControlLeft),
        key(KeyCode::KeyE),
        key(KeyCode::KeyC), // (the table: F)
        key(KeyCode::KeyR),
        key(KeyCode::Escape),
        key(KeyCode::Tab),
    ],
    // Quickdraw (81)
    [
        WASD,
        Bind::Motion,
        mouse(MouseButton::Left),
        mouse(MouseButton::Right),
        key(KeyCode::Space),
        key(KeyCode::ControlLeft),
        key(KeyCode::KeyR),
        key(KeyCode::KeyF),
        key(KeyCode::KeyE),
        key(KeyCode::Escape),
        key(KeyCode::Tab),
    ],
    // Wheelless (82)
    [
        WASD,
        Bind::Motion,
        mouse(MouseButton::Left),
        mouse(MouseButton::Right),
        key(KeyCode::Space),
        key(KeyCode::ControlLeft),
        key(KeyCode::KeyQ),
        key(KeyCode::KeyF),
        key(KeyCode::KeyR),
        key(KeyCode::Escape),
        key(KeyCode::Digit5), // (the table: Tab)
    ],
    // Rebel (83)
    [
        WASD,
        Bind::Motion,
        mouse(MouseButton::Left),
        mouse(MouseButton::Right),
        key(KeyCode::Space),
        key(KeyCode::ControlLeft),
        key(KeyCode::KeyT),
        key(KeyCode::KeyF),
        key(KeyCode::KeyR),
        key(KeyCode::Escape),
        key(KeyCode::Tab),
    ],
    // Southpaw (84)
    [
        IJKL,
        Bind::Motion,
        mouse(MouseButton::Right),
        mouse(MouseButton::Left),
        key(KeyCode::Space),
        key(KeyCode::ShiftLeft),
        key(KeyCode::KeyO),
        key(KeyCode::KeyH),
        key(KeyCode::KeyY),
        key(KeyCode::Escape),
        key(KeyCode::Tab),
    ],
];

/// The keys that can be bound, with their names: in settings.txt (Bevy's), on the
/// screen (a UIText.int key, for the keys the game names, and the English).
/// Shift, Control and Alt are either one. Escape stays Pause (and backs out of
/// binding a key) and the arrow keys always move, so they aren't offered.
const KEYS: &[(KeyCode, &str, &str, &str)] = &[
    (KeyCode::KeyA, "KeyA", "Keyboard.AKey", "A"),
    (KeyCode::KeyB, "KeyB", "Keyboard.BKey", "B"),
    (KeyCode::KeyC, "KeyC", "Keyboard.CKey", "C"),
    (KeyCode::KeyD, "KeyD", "Keyboard.DKey", "D"),
    (KeyCode::KeyE, "KeyE", "Keyboard.EKey", "E"),
    (KeyCode::KeyF, "KeyF", "Keyboard.FKey", "F"),
    (KeyCode::KeyG, "KeyG", "Keyboard.GKey", "G"),
    (KeyCode::KeyH, "KeyH", "Keyboard.HKey", "H"),
    (KeyCode::KeyI, "KeyI", "Keyboard.IKey", "I"),
    (KeyCode::KeyJ, "KeyJ", "Keyboard.JKey", "J"),
    (KeyCode::KeyK, "KeyK", "Keyboard.KKey", "K"),
    (KeyCode::KeyL, "KeyL", "Keyboard.LKey", "L"),
    (KeyCode::KeyM, "KeyM", "Keyboard.MKey", "M"),
    (KeyCode::KeyN, "KeyN", "Keyboard.NKey", "N"),
    (KeyCode::KeyO, "KeyO", "Keyboard.OKey", "O"),
    (KeyCode::KeyP, "KeyP", "Keyboard.PKey", "P"),
    (KeyCode::KeyQ, "KeyQ", "Keyboard.QKey", "Q"),
    (KeyCode::KeyR, "KeyR", "Keyboard.RKey", "R"),
    (KeyCode::KeyS, "KeyS", "Keyboard.SKey", "S"),
    (KeyCode::KeyT, "KeyT", "Keyboard.TKey", "T"),
    (KeyCode::KeyU, "KeyU", "Keyboard.UKey", "U"),
    (KeyCode::KeyV, "KeyV", "Keyboard.VKey", "V"),
    (KeyCode::KeyW, "KeyW", "Keyboard.WKey", "W"),
    (KeyCode::KeyX, "KeyX", "Keyboard.XKey", "X"),
    (KeyCode::KeyY, "KeyY", "Keyboard.YKey", "Y"),
    (KeyCode::KeyZ, "KeyZ", "Keyboard.ZKey", "Z"),
    (KeyCode::Digit0, "Digit0", "Keyboard.Num0Key", "0"),
    (KeyCode::Digit1, "Digit1", "Keyboard.Num1Key", "1"),
    (KeyCode::Digit2, "Digit2", "Keyboard.Num2Key", "2"),
    (KeyCode::Digit3, "Digit3", "Keyboard.Num3Key", "3"),
    (KeyCode::Digit4, "Digit4", "Keyboard.Num4Key", "4"),
    (KeyCode::Digit5, "Digit5", "Keyboard.Num5Key", "5"),
    (KeyCode::Digit6, "Digit6", "Keyboard.Num6Key", "6"),
    (KeyCode::Digit7, "Digit7", "Keyboard.Num7Key", "7"),
    (KeyCode::Digit8, "Digit8", "Keyboard.Num8Key", "8"),
    (KeyCode::Digit9, "Digit9", "Keyboard.Num9Key", "9"),
    (
        KeyCode::ShiftLeft,
        "ShiftLeft",
        "Keyboard.ShiftKey",
        "Shift",
    ),
    (
        KeyCode::ControlLeft,
        "ControlLeft",
        "Keyboard.Control",
        "Control",
    ),
    (KeyCode::AltLeft, "AltLeft", "", "Alt"),
    (KeyCode::Space, "Space", "Keyboard.Space", "Space"),
    (KeyCode::Tab, "Tab", "Keyboard.Tab", "Tab"),
    (KeyCode::Enter, "Enter", "", "Enter"),
    (KeyCode::Backspace, "Backspace", "", "Backspace"),
    (KeyCode::CapsLock, "CapsLock", "", "Caps Lock"),
    (KeyCode::Semicolon, "Semicolon", "Keyboard.Semicolon", ";"),
    (KeyCode::Slash, "Slash", "Keyboard.ForwardSlash", "/"),
    (KeyCode::Backquote, "Backquote", "", "`"),
    (KeyCode::Minus, "Minus", "", "-"),
    (KeyCode::Equal, "Equal", "", "="),
    (KeyCode::BracketLeft, "BracketLeft", "", "["),
    (KeyCode::BracketRight, "BracketRight", "", "]"),
    (KeyCode::Backslash, "Backslash", "", "\\"),
    (KeyCode::Quote, "Quote", "", "'"),
    (KeyCode::Comma, "Comma", "", ","),
    (KeyCode::Period, "Period", "", "."),
    (KeyCode::Insert, "Insert", "", "Insert"),
    (KeyCode::Delete, "Delete", "", "Delete"),
    (KeyCode::Home, "Home", "", "Home"),
    (KeyCode::End, "End", "", "End"),
    (KeyCode::PageUp, "PageUp", "", "Page Up"),
    (KeyCode::PageDown, "PageDown", "", "Page Down"),
    (KeyCode::Numpad0, "Numpad0", "", "Num 0"),
    (KeyCode::Numpad1, "Numpad1", "", "Num 1"),
    (KeyCode::Numpad2, "Numpad2", "", "Num 2"),
    (KeyCode::Numpad3, "Numpad3", "", "Num 3"),
    (KeyCode::Numpad4, "Numpad4", "", "Num 4"),
    (KeyCode::Numpad5, "Numpad5", "", "Num 5"),
    (KeyCode::Numpad6, "Numpad6", "", "Num 6"),
    (KeyCode::Numpad7, "Numpad7", "", "Num 7"),
    (KeyCode::Numpad8, "Numpad8", "", "Num 8"),
    (KeyCode::Numpad9, "Numpad9", "", "Num 9"),
    (KeyCode::NumpadAdd, "NumpadAdd", "", "Num +"),
    (KeyCode::NumpadSubtract, "NumpadSubtract", "", "Num -"),
    (KeyCode::NumpadMultiply, "NumpadMultiply", "", "Num *"),
    (KeyCode::NumpadDivide, "NumpadDivide", "", "Num /"),
    (KeyCode::NumpadDecimal, "NumpadDecimal", "", "Num ."),
    (KeyCode::NumpadEnter, "NumpadEnter", "", "Num Enter"),
    (KeyCode::F1, "F1", "", "F1"),
    (KeyCode::F2, "F2", "", "F2"),
    (KeyCode::F3, "F3", "", "F3"),
    (KeyCode::F4, "F4", "", "F4"),
    (KeyCode::F5, "F5", "", "F5"),
    (KeyCode::F6, "F6", "", "F6"),
    (KeyCode::F7, "F7", "", "F7"),
    (KeyCode::F8, "F8", "", "F8"),
    (KeyCode::F9, "F9", "", "F9"),
    (KeyCode::F10, "F10", "", "F10"),
    (KeyCode::F11, "F11", "", "F11"),
    (KeyCode::F12, "F12", "", "F12"),
];

/// Escape's names (it shows as Pause's key, but isn't one of the keys offered).
const ESCAPE: (&str, &str) = ("Keyboard.Escape", "Escape");

/// A key as it can be bound: the right-hand Shift, Control and Alt as the
/// left-hand ones; None for a key that can't be (Escape, the arrow keys, and keys
/// not in KEYS).
pub fn bindable(k: KeyCode) -> Option<KeyCode> {
    let k = match k {
        KeyCode::ShiftRight => KeyCode::ShiftLeft,
        KeyCode::ControlRight => KeyCode::ControlLeft,
        KeyCode::AltRight => KeyCode::AltLeft,
        k => k,
    };
    KEYS.iter().any(|e| e.0 == k).then_some(k)
}

/// A key and its right-hand twin, if it has one (Shift, Control, Alt).
fn sides(k: KeyCode) -> [KeyCode; 2] {
    match k {
        KeyCode::ShiftLeft => [KeyCode::ShiftLeft, KeyCode::ShiftRight],
        KeyCode::ControlLeft => [KeyCode::ControlLeft, KeyCode::ControlRight],
        KeyCode::AltLeft => [KeyCode::AltLeft, KeyCode::AltRight],
        k => [k, k],
    }
}

fn key_names(k: KeyCode) -> (&'static str, String) {
    if k == KeyCode::Escape {
        return (ESCAPE.0, ESCAPE.1.to_string());
    }
    KEYS.iter()
        .find(|e| e.0 == k)
        .map(|e| (e.2, e.3.to_string()))
        .unwrap_or(("", format!("{k:?}")))
}

fn key_id(k: KeyCode) -> String {
    if k == KeyCode::Escape {
        return "Escape".to_string();
    }
    KEYS.iter()
        .find(|e| e.0 == k)
        .map(|e| e.1.to_string())
        .unwrap_or_else(|| format!("{k:?}"))
}

fn parse_key(id: &str) -> Option<KeyCode> {
    KEYS.iter().find(|e| e.1 == id).map(|e| e.0)
}

/// Move's keys in the order the screen lists them: "W,A,S,D or Arrow Keys".
pub const MOVE_KEYS: [&str; 4] = ["Forward", "Left", "Back", "Right"];

impl Bind {
    /// Its name on the screen: a UIText.int key ("" if the game has none) and the
    /// English.
    pub fn text(self) -> (&'static str, String) {
        match self {
            Bind::Key(k) => key_names(k),
            Bind::Mouse(MouseButton::Left) => {
                ("Keyboard.LeftMouseButton", "Left Mouse Button".to_string())
            }
            Bind::Mouse(MouseButton::Right) => (
                "Keyboard.RightMouseButton",
                "Right Mouse Button".to_string(),
            ),
            Bind::Mouse(MouseButton::Middle) => (
                "Keyboard.MiddleMouseButton",
                "Middle Mouse Button".to_string(),
            ),
            Bind::Mouse(MouseButton::Back) => ("", "Mouse Button 4".to_string()),
            Bind::Mouse(MouseButton::Forward) => ("", "Mouse Button 5".to_string()),
            Bind::Mouse(MouseButton::Other(n)) => ("", format!("Mouse Button {n}")),
            Bind::Wheel => ("Keyboard.Wheel", "Mouse Wheel".to_string()),
            Bind::WheelUp => ("Keyboard.MouseWheelUp", "Mouse Wheel Up".to_string()),
            Bind::WheelDown => ("", "Mouse Wheel Down".to_string()),
            Bind::Motion => ("Keyboard.Mouse", "Mouse".to_string()),
            WASD => ("Keyboard.Move", "W,A,S,D or Arrow Keys".to_string()),
            IJKL => ("Keyboard.MoveSouthPaw", "I,K,J,L or Arrow Keys".to_string()),
            Bind::Move(keys) => {
                let names: Vec<String> = keys.iter().map(|&k| key_names(k).1).collect();
                ("", format!("{} or Arrow Keys", names.join(",")))
            }
        }
    }

    /// Its name in settings.txt.
    fn id(self) -> String {
        match self {
            Bind::Key(k) => key_id(k),
            Bind::Mouse(MouseButton::Left) => "MouseLeft".to_string(),
            Bind::Mouse(MouseButton::Right) => "MouseRight".to_string(),
            Bind::Mouse(MouseButton::Middle) => "MouseMiddle".to_string(),
            Bind::Mouse(MouseButton::Back) => "MouseBack".to_string(),
            Bind::Mouse(MouseButton::Forward) => "MouseForward".to_string(),
            Bind::Mouse(MouseButton::Other(n)) => format!("Mouse{n}"),
            Bind::Wheel => "MouseWheel".to_string(),
            Bind::WheelUp => "MouseWheelUp".to_string(),
            Bind::WheelDown => "MouseWheelDown".to_string(),
            Bind::Motion => "MouseMotion".to_string(),
            Bind::Move(keys) => keys.map(key_id).join(","),
        }
    }

    fn parse(id: &str) -> Option<Bind> {
        Some(match id {
            "MouseLeft" => Bind::Mouse(MouseButton::Left),
            "MouseRight" => Bind::Mouse(MouseButton::Right),
            "MouseMiddle" => Bind::Mouse(MouseButton::Middle),
            "MouseBack" => Bind::Mouse(MouseButton::Back),
            "MouseForward" => Bind::Mouse(MouseButton::Forward),
            "MouseWheel" => Bind::Wheel,
            "MouseWheelUp" => Bind::WheelUp,
            "MouseWheelDown" => Bind::WheelDown,
            "MouseMotion" => Bind::Motion,
            id if id.contains(',') => {
                let keys: Vec<KeyCode> = id.split(',').filter_map(parse_key).collect();
                Bind::Move(keys.try_into().ok()?)
            }
            id => match id.strip_prefix("Mouse").map(str::parse::<u16>) {
                Some(Ok(n)) => Bind::Mouse(MouseButton::Other(n)),
                _ => Bind::Key(parse_key(id)?),
            },
        })
    }

    /// Held down this frame (the wheel: turned this frame).
    pub fn held(self, input: &Inputs) -> bool {
        match self {
            Bind::Key(k) => sides(k).iter().any(|&k| input.keys.pressed(k)),
            Bind::Mouse(b) => input.mouse.pressed(b),
            Bind::Wheel => input.wheel != 0.0,
            Bind::WheelUp => input.wheel > 0.0,
            Bind::WheelDown => input.wheel < 0.0,
            Bind::Motion | Bind::Move(_) => false,
        }
    }

    /// Pressed this frame (the wheel: turned this frame).
    pub fn pressed(self, input: &Inputs) -> bool {
        match self {
            Bind::Key(k) => sides(k).iter().any(|&k| input.keys.just_pressed(k)),
            Bind::Mouse(b) => input.mouse.just_pressed(b),
            wheel => wheel.held(input),
        }
    }

    /// Let go this frame (never the wheel). Keys put aside while the pause menu
    /// is up haven't been let go.
    pub fn released(self, input: &Inputs) -> bool {
        match self {
            Bind::Key(k) => sides(k).iter().any(|&k| input.keys.just_released(k)),
            Bind::Mouse(b) => input.mouse.just_released(b),
            _ => false,
        }
    }

    /// Its name in the HUD's list of keys, short and in capitals: "WASD", "SHIFT",
    /// "LEFT MOUSE", "WHEEL".
    pub fn short_text(self) -> String {
        match self {
            Bind::Key(k) => key_names(k).1.to_uppercase(),
            Bind::Mouse(MouseButton::Left) => "LEFT MOUSE".to_string(),
            Bind::Mouse(MouseButton::Right) => "RIGHT MOUSE".to_string(),
            Bind::Mouse(MouseButton::Middle) => "MIDDLE MOUSE".to_string(),
            Bind::Mouse(MouseButton::Back) => "MOUSE 4".to_string(),
            Bind::Mouse(MouseButton::Forward) => "MOUSE 5".to_string(),
            Bind::Mouse(MouseButton::Other(n)) => format!("MOUSE {n}"),
            Bind::Wheel => "WHEEL".to_string(),
            Bind::WheelUp => "WHEEL UP".to_string(),
            Bind::WheelDown => "WHEEL DOWN".to_string(),
            Bind::Motion => "MOUSE".to_string(),
            // Forward, left, back and right: "WASD", or "UP/LEFT/DOWN/RIGHT" for
            // longer names.
            Bind::Move(keys) => {
                let names = keys.map(|k| key_names(k).1.to_uppercase());
                let single = names.iter().all(|n| n.chars().count() == 1);
                names.join(if single { "" } else { "/" })
            }
        }
    }

    /// Whether `input` is (part of) it, and for Move which of its keys.
    fn uses(self, input: Input) -> Option<usize> {
        match (self, input) {
            (Bind::Move(keys), Input::Key(k)) => keys.iter().position(|&m| m == k),
            (Bind::Wheel, Input::WheelUp | Input::WheelDown) => Some(0),
            (bind, input) => (bind == input.bind()).then_some(0),
        }
    }
}

/// This frame's keys, buttons and wheel, to read the actions from.
pub struct Inputs<'a> {
    pub keys: &'a ButtonInput<KeyCode>,
    pub mouse: &'a ButtonInput<MouseButton>,
    /// How far the wheel turned this frame (up positive).
    pub wheel: f32,
}

/// The Controls settings.
#[derive(Resource, Clone, PartialEq, Debug)]
pub struct Controls {
    /// Active Game Controls: Controller (else Keyboard/Mouse).
    pub controller: bool,
    /// Select Layout: one of the game's layouts (LAYOUTS), or CUSTOM.
    pub layout: usize,
    /// The player's own layout, once a key has been changed: each mode's keys, in
    /// its actions' order.
    pub custom: Option<[Vec<Bind>; 2]>,
    pub vibration: bool,
    /// Toggle Vehicle Boost: Hold (true) or Toggle.
    pub hold_boost: bool,
    /// LS / RS Button Delay (the game's HeavyThumb).
    pub stick_delay: bool,
    /// Robot Camera, Vehicle Camera and Jet Camera: inverted.
    pub invert: [bool; 3],
    /// Camera Sensitivity: 0 to 100 in steps of 5.
    pub camera_sensitivity: u32,
    /// Mouse Sensitivity: 3 to 120 in steps of 3 (the game's MenuSlider).
    pub mouse_sensitivity: u32,
    /// Controller Layout: each row's scheme (padlayout::Row's order), an index
    /// into its schemes.
    pub pad_schemes: [usize; 3],
}

/// The sliders' ranges and steps: (minimum, maximum, step), as the movie's
/// ControlsMenuProvider gives them.
pub const CAMERA_SENSITIVITY: (u32, u32, u32) = (0, 100, 5);
pub const MOUSE_SENSITIVITY: (u32, u32, u32) = (3, 120, 3);
/// PlayerInput's MouseSensitivity in the game's input settings (PC-TransInput.ini):
/// the mouse's speed in the demo up to now.
const MOUSE_SENSITIVITY_DEFAULT: u32 = 60;

impl Default for Controls {
    fn default() -> Self {
        Controls {
            controller: false,
            layout: 0,
            custom: None,
            vibration: true,
            hold_boost: true,
            stick_delay: false,
            invert: [false; 3],
            camera_sensitivity: 50,
            mouse_sensitivity: MOUSE_SENSITIVITY_DEFAULT,
            pad_schemes: [0; 3],
        }
    }
}

impl Controls {
    /// The controls saved in settings.txt (or the defaults).
    pub fn load() -> Self {
        Controls::from_settings(&crate::menu::read_settings())
    }

    fn from_settings(saved: &HashMap<String, String>) -> Self {
        let mut c = Controls::default();
        let flag =
            |name: &str, default: bool| saved.get(name).map_or(default, |v| v.as_str() == "1");
        let number = |name: &str, (min, max, step): (u32, u32, u32), default: u32| {
            saved
                .get(name)
                .and_then(|v| v.parse::<u32>().ok())
                .map_or(default, |v| (v.clamp(min, max) - min) / step * step + min)
        };
        c.controller = saved.get("active_controls").map(String::as_str) == Some("controller");
        c.vibration = flag("controller_vibration", c.vibration);
        c.hold_boost = flag("hold_vehicle_boost", c.hold_boost);
        c.stick_delay = flag("stick_button_delay", c.stick_delay);
        c.invert = [
            flag("invert_robot_camera", false),
            flag("invert_vehicle_camera", false),
            flag("invert_jet_camera", false),
        ];
        c.camera_sensitivity = number(
            "camera_sensitivity",
            CAMERA_SENSITIVITY,
            c.camera_sensitivity,
        );
        c.mouse_sensitivity = number("mouse_sensitivity", MOUSE_SENSITIVITY, c.mouse_sensitivity);
        c.pad_schemes = crate::padlayout::parse_chosen(|id| saved.get(id).cloned());
        c.layout = saved
            .get("keyboard_layout")
            .and_then(|v| LAYOUT_IDS.iter().position(|id| id == v))
            .unwrap_or(0);
        // The player's own keys: Default's, with those saved in their place.
        let saved_binds = Mode::ALL.iter().any(|m| {
            m.actions()
                .iter()
                .any(|a| saved.contains_key(&bind_name(*m, *a)))
        });
        if c.layout == CUSTOM || saved_binds {
            c.custom = Some(Mode::ALL.map(|mode| {
                mode.actions()
                    .iter()
                    .zip(mode.layout(0))
                    .map(|(&action, &default)| {
                        let saved = saved
                            .get(&bind_name(mode, action))
                            .and_then(|v| Bind::parse(v))
                            .filter(|_| !action.fixed());
                        // (Move stays four keys, the rest single.)
                        match (default, saved) {
                            (Bind::Move(_), Some(b @ Bind::Move(_))) => b,
                            (Bind::Move(_), _) | (_, Some(Bind::Move(_))) => default,
                            (_, Some(b)) => b,
                            (_, None) => default,
                        }
                    })
                    .collect()
            }));
        }
        c
    }

    /// The settings.txt lines for these controls (with the player's own keys only
    /// when there are any).
    pub fn settings(&self) -> Vec<(String, String)> {
        let flag = |b: bool| if b { "1" } else { "0" }.to_string();
        let mut lines = vec![
            (
                "keyboard_layout".to_string(),
                LAYOUT_IDS[self.layout.min(CUSTOM)].to_string(),
            ),
            (
                "active_controls".to_string(),
                if self.controller {
                    "controller"
                } else {
                    "keyboard_mouse"
                }
                .to_string(),
            ),
            ("controller_vibration".to_string(), flag(self.vibration)),
            ("hold_vehicle_boost".to_string(), flag(self.hold_boost)),
            ("stick_button_delay".to_string(), flag(self.stick_delay)),
            ("invert_robot_camera".to_string(), flag(self.invert[0])),
            ("invert_vehicle_camera".to_string(), flag(self.invert[1])),
            ("invert_jet_camera".to_string(), flag(self.invert[2])),
            (
                "camera_sensitivity".to_string(),
                self.camera_sensitivity.to_string(),
            ),
            (
                "mouse_sensitivity".to_string(),
                self.mouse_sensitivity.to_string(),
            ),
        ];
        for row in crate::padlayout::Row::ALL {
            let schemes = row.schemes();
            let chosen = &schemes[self.pad_schemes[row.index()].min(schemes.len() - 1)];
            lines.push((row.id().to_string(), chosen.id.to_string()));
        }
        if let Some(custom) = &self.custom {
            for mode in Mode::ALL {
                for (action, bind) in mode.actions().iter().zip(&custom[mode.index()]) {
                    if !action.fixed() {
                        lines.push((bind_name(mode, *action), bind.id()));
                    }
                }
            }
        }
        lines
    }

    /// How many layouts Select Layout offers (the game's, and Custom once there
    /// is one).
    pub fn layouts(&self) -> usize {
        LAYOUTS + usize::from(self.custom.is_some())
    }

    /// The layout in use's keys for `mode`'s actions.
    pub fn binds(&self, mode: Mode) -> &[Bind] {
        match &self.custom {
            Some(custom) if self.layout == CUSTOM => &custom[mode.index()],
            _ => mode.layout(self.layout),
        }
    }

    /// What `action` is bound to in `mode` (None if the mode has no such action).
    pub fn bind(&self, mode: Mode, action: Action) -> Option<Bind> {
        let i = mode.actions().iter().position(|&a| a == action)?;
        self.binds(mode).get(i).copied()
    }

    pub fn held(&self, mode: Mode, action: Action, input: &Inputs) -> bool {
        self.bind(mode, action).is_some_and(|b| b.held(input))
    }

    pub fn pressed(&self, mode: Mode, action: Action, input: &Inputs) -> bool {
        self.bind(mode, action).is_some_and(|b| b.pressed(input))
    }

    pub fn released(&self, mode: Mode, action: Action, input: &Inputs) -> bool {
        self.bind(mode, action).is_some_and(|b| b.released(input))
    }

    /// The movement keys held (x to the right, y forward, each -1, 0 or 1): Move's
    /// keys, and the arrow keys (which the game always moves with).
    pub fn movement(&self, mode: Mode, keys: &ButtonInput<KeyCode>) -> Vec2 {
        let mut held = [
            keys.pressed(KeyCode::ArrowUp),
            keys.pressed(KeyCode::ArrowLeft),
            keys.pressed(KeyCode::ArrowDown),
            keys.pressed(KeyCode::ArrowRight),
        ];
        if let Some(Bind::Move(move_keys)) = self.bind(mode, Action::Move) {
            for (h, k) in held.iter_mut().zip(move_keys) {
                *h |= sides(k).iter().any(|&k| keys.pressed(k));
            }
        }
        let axis = |plus: bool, minus: bool| f32::from(u8::from(plus)) - f32::from(u8::from(minus));
        Vec2::new(axis(held[3], held[1]), axis(held[0], held[2]))
    }

    /// The mouse's speed, as a multiple of the usual (Mouse Sensitivity 60).
    pub fn mouse_scale(&self) -> f32 {
        self.mouse_sensitivity as f32 / MOUSE_SENSITIVITY_DEFAULT as f32
    }

    /// The name of the key `action` is bound to in `mode` (the English), for the
    /// HUD's prompts.
    pub fn key_name(&self, mode: Mode, action: Action) -> String {
        self.bind(mode, action)
            .map(|b| b.text().1)
            .unwrap_or_default()
    }

    /// The same, short and in capitals, for the HUD's list of keys.
    pub fn short_key_name(&self, mode: Mode, action: Action) -> String {
        self.bind(mode, action)
            .map(Bind::short_text)
            .unwrap_or_default()
    }

    /// Binds row `row` of `mode`'s table (for Move, its key `slot`: forward, left,
    /// back or right) to `input`. The row already bound to it, if any, gets this
    /// one's key in exchange (a wheel bound either way keeps the other way). The
    /// layout becomes the Custom one (the layout showing, with the change). False,
    /// and nothing changes, if it can't be done: a fixed row, a mouse button or
    /// the wheel for Move, the wheel for an action that is held, or an exchange
    /// that would put one there.
    pub fn assign(&mut self, mode: Mode, row: usize, slot: usize, input: Input) -> bool {
        let actions = mode.actions();
        if actions.get(row).is_none_or(|a| a.fixed()) {
            return false;
        }
        let wheel = |b: Bind| matches!(b, Bind::Wheel | Bind::WheelUp | Bind::WheelDown);
        if actions[row].is_held() && wheel(input.bind()) {
            return false;
        }
        let mut table = self.binds(mode).to_vec();
        // What the row gives up.
        let old = match (table[row], input) {
            (Bind::Move(keys), Input::Key(_)) => Bind::Key(keys[slot.min(3)]),
            (Bind::Move(_), _) => return false,
            (bind, _) => bind,
        };
        if old == input.bind() {
            return true;
        }
        // The row (and key of Move) already using the input, if one is.
        let taken = table.iter().enumerate().find_map(|(r, b)| {
            let s = b.uses(input)?;
            (!(r == row && matches!(b, Bind::Move(_)) && s == slot)).then_some((r, s))
        });
        if let Some((r, s)) = taken {
            if actions[r].is_held() && wheel(old) {
                return false;
            }
            table[r] = match (table[r], input, old) {
                // A wheel bound either way keeps the way not taken.
                (Bind::Wheel, Input::WheelUp, _) => Bind::WheelDown,
                (Bind::Wheel, Input::WheelDown, _) => Bind::WheelUp,
                (Bind::Move(mut keys), _, Bind::Key(k)) => {
                    keys[s] = k;
                    Bind::Move(keys)
                }
                (Bind::Move(_), _, _) => return false,
                (_, _, old) => old,
            };
        }
        table[row] = match (table[row], input) {
            (Bind::Move(mut keys), Input::Key(k)) => {
                keys[slot.min(3)] = k;
                Bind::Move(keys)
            }
            (_, input) => input.bind(),
        };
        let mut custom = match self.custom.take() {
            Some(custom) if self.layout == CUSTOM => custom,
            _ => Mode::ALL.map(|m| self.binds(m).to_vec()),
        };
        custom[mode.index()] = table;
        self.custom = Some(custom);
        self.layout = CUSTOM;
        true
    }
}

fn bind_name(mode: Mode, action: Action) -> String {
    format!("bind_{}_{}", mode.id(), action.id())
}
