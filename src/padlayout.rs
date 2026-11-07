//! The controller's layouts (Settings > Controls > Controller Layout), as the game
//! keeps them: its bindings for a controller (TnPlayerInput's PadBindings in
//! PC-TransInput.ini, over PlayerInput's own Bindings), and the control schemes
//! that change them (TnDataProvider_ControlScheme, in the same file; their names in
//! TransGame.int), one chosen for each of the screen's rows:
//!
//! - Robot (SchemeMode 0): Default, Tactician (switch camera and melee swapped:
//!   B and RS), Striker (change form on Y, swap weapons on the d-pad's up),
//!   Charger (change form on LB, dash and sprint on LS), Southpaw (the bumpers
//!   swapped, and the triggers: fire on LT, fine aim and boost on RT), Hoarder
//!   (interact and reload on the d-pad's up, change form on X).
//! - Vehicle (1): Default, Seeker (a jet's ascend and descend swapped: B and A).
//! - Thumbsticks (3): Default, Southpaw (move on the right stick, look on the left).
//!
//! (The game's screen has a Grimlock row too, SchemeMode 2, for the campaign's
//! Grimlock, its Default and Rebel schemes: the demo has no campaign and leaves it
//! out.)
//!
//! The game applies them as TnPlayerInput.ApplyAllControlSchemes does: its default
//! bindings, the controller's over them (ApplyPreferredControllerBindings), then
//! each active scheme in the order of its mode (SetBindNoSave: a binding's command
//! replaced within the key's commands, added after them, or a key bound anew). The
//! robot's, the vehicle's and the thumbsticks' are active together while a robot
//! or vehicle plays (SetPrimaryControlMode). The Controller Layout screen labels
//! each control by what it does in the row's mode (GetKeyDescription): each of its
//! commands' descriptions (TnPlayerInputCommandDescriptions, those meant for that
//! mode), joined by "/".

/// The controller's bindings: (key, its commands, "|" between them).
pub type Bindings = Vec<(&'static str, String)>;

/// PlayerInput's Bindings for the controller (PC-TransInput.ini's
/// [Engine.PlayerInput]), and TnPlayerInput's PadBindings over them.
const DEFAULT_BINDINGS: [(&str, &str); 2] = [
    ("XboxTypeS_Start", "|onrelease showmenu"),
    ("XboxTypeS_DPad_Down", "ErrorScreenHide"),
];
const PAD_BINDINGS: [(&str, &str); 17] = [
    (
        "XboxTypeS_LeftX",
        "MoveBind1 | MoveBind0 | Axis aStrafe Speed=1.0 DeadZone=0",
    ),
    (
        "XboxTypeS_LeftY",
        "PullLever | PullObject | Axis aBaseY Speed=1.0 DeadZone=0",
    ),
    ("XboxTypeS_RightX", "Axis aTurn Speed=1.0 DeadZone=0"),
    ("XboxTypeS_RightY", "Axis aLookup Speed=1.0 DeadZone=0"),
    ("XboxTypeS_A", "PressJump | Ascend | Confirm"),
    (
        "XboxTypeS_B",
        "ChangeHandedness | InteractCommand1 | Descend | SkipMatinee | DownedSuicide | CancelLever",
    ),
    (
        "XboxTypeS_X",
        "XButton | Assassinate | UseInteract | Pickup | Reload",
    ),
    ("XboxTypeS_Y", "NextWeapon"),
    ("XboxTypeS_RightThumbstick", "Melee"),
    ("XboxTypeS_LeftThumbstick", "Transform"),
    ("XboxTypeS_DPad_Left", "OffensiveEquipment"),
    ("XboxTypeS_DPad_Right", "DefensiveEquipment"),
    ("XboxTypeS_Back", "Focus | ShowScores"),
    ("XboxTypeS_RightTrigger", "Fire | ChooseOption2"),
    (
        "XboxTypeS_LeftTrigger",
        "FineAim | Boost | ToggleBoost | ChooseOption1",
    ),
    (
        "XboxTypeS_RightShoulder",
        "Ability0 | SwitchSpectatorTarget -1",
    ),
    (
        "XboxTypeS_LeftShoulder",
        "Ability1 | SwitchSpectatorTarget 1",
    ),
];

/// A scheme's change to a binding (its PadBindings): on `key` (or, with none, the
/// first key bound to `old`), `old` within its commands replaced by `new`, or with
/// no `old`, `new` added after them (the key bound to `new` alone if it has none).
#[derive(Clone, Copy, Debug)]
pub struct Rebind {
    pub key: Option<&'static str>,
    pub old: &'static str,
    pub new: &'static str,
}

const fn on(key: &'static str, old: &'static str, new: &'static str) -> Rebind {
    Rebind {
        key: Some(key),
        old,
        new,
    }
}

/// A control scheme: its name (TransGame.int's FriendlyName, as menu.txt keeps it,
/// and the English; the default's Settings.Default) and its changes.
#[derive(Clone, Copy, Debug)]
pub struct Scheme {
    pub key: &'static str,
    pub name: &'static str,
    /// Its id in settings.txt.
    pub id: &'static str,
    pub rebinds: &'static [Rebind],
}

const X_COMMANDS: &str = "XButton | Assassinate | UseInteract | Pickup | Reload";
const AIM_COMMANDS: &str = "FineAim | Boost | ToggleBoost";

/// Each row's schemes, in the screen's order (the game's ControlSchemes, by
/// SchemeID; Robot, Vehicle and Thumbsticks start with their default, no scheme).
const ROBOT: [Scheme; 6] = [
    Scheme {
        key: "Settings.Default",
        name: "Default",
        id: "default",
        rebinds: &[],
    },
    // RobotAltSideSwitchControlScheme (SchemeID 0).
    Scheme {
        key: "TransGame.RobotAltSideSwitchControlScheme.FriendlyName",
        name: "Tactician",
        id: "tactician",
        rebinds: &[
            on("XboxTypeS_B", "ChangeHandedness", "Melee"),
            on("XboxTypeS_RightThumbstick", "Melee", "ChangeHandedness"),
        ],
    },
    // RobotAltTransformControlScheme (1).
    Scheme {
        key: "TransGame.RobotAltTransformControlScheme.FriendlyName",
        name: "Striker",
        id: "striker",
        rebinds: &[
            on("XboxTypeS_LeftThumbstick", "Transform", ""),
            on("XboxTypeS_Y", "NextWeapon", "Transform"),
            on("XboxTypeS_DPad_Up", "", "NextWeapon"),
        ],
    },
    // RobotAltSprintControlScheme (2).
    Scheme {
        key: "TransGame.RobotAltSprintControlScheme.FriendlyName",
        name: "Charger",
        id: "charger",
        rebinds: &[
            on("XboxTypeS_LeftThumbstick", "Transform", "Ability1"),
            on("XboxTypeS_LeftShoulder", "Ability1", "Transform"),
        ],
    },
    // RobotAltBumpersAndTriggersControlScheme (3).
    Scheme {
        key: "TransGame.RobotAltBumpersAndTriggersControlScheme.FriendlyName",
        name: "Southpaw",
        id: "southpaw",
        rebinds: &[
            on("XboxTypeS_LeftShoulder", "Ability1", "Ability0"),
            on("XboxTypeS_RightShoulder", "Ability0", "Ability1"),
            on("XboxTypeS_LeftTrigger", AIM_COMMANDS, "Fire"),
            on("XboxTypeS_RightTrigger", "Fire", AIM_COMMANDS),
        ],
    },
    // RobotDPadReload (4).
    Scheme {
        key: "TransGame.RobotDPadReload.FriendlyName",
        name: "Hoarder",
        id: "hoarder",
        rebinds: &[
            on("XboxTypeS_LeftThumbstick", "Transform", ""),
            on("XboxTypeS_DPad_Up", "", X_COMMANDS),
            on("XboxTypeS_X", X_COMMANDS, "Transform"),
        ],
    },
];
const VEHICLE: [Scheme; 2] = [
    Scheme {
        key: "Settings.Default",
        name: "Default",
        id: "default",
        rebinds: &[],
    },
    // VehicleInvertAscensionControlScheme (10).
    Scheme {
        key: "TransGame.VehicleInvertAscensionControlScheme.FriendlyName",
        name: "Seeker",
        id: "seeker",
        rebinds: &[
            on("XboxTypeS_A", "Ascend", "Descend"),
            on("XboxTypeS_B", "Descend", "Ascend"),
        ],
    },
];
const STICKS: [Scheme; 2] = [
    Scheme {
        key: "Settings.Default",
        name: "Default",
        id: "default",
        rebinds: &[],
    },
    // SwapSticksControlScheme (30).
    Scheme {
        key: "TransGame.SwapSticksControlScheme.FriendlyName",
        name: "Southpaw",
        id: "southpaw",
        rebinds: &[
            on(
                "XboxTypeS_LeftX",
                "Axis aStrafe Speed=1.0 DeadZone=0",
                "Axis aTurn Speed=1.0 DeadZone=0",
            ),
            on(
                "XboxTypeS_LeftY",
                "Axis aBaseY Speed=1.0 DeadZone=0",
                "Axis aLookup Speed=-1.0 DeadZone=0",
            ),
            on(
                "XboxTypeS_RightX",
                "Axis aTurn Speed=1.0 DeadZone=0",
                "Axis aStrafe Speed=1.0 DeadZone=0",
            ),
            on(
                "XboxTypeS_RightY",
                "Axis aLookup Speed=1.0 DeadZone=0",
                "Axis aBaseY Speed=-1.0 DeadZone=0",
            ),
        ],
    },
];

/// The screen's rows (the schemes' modes, ControllerLayoutMenuProvider's order,
/// without the campaign's Grimlock).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Robot,
    Vehicle,
    Sticks,
}

impl Row {
    pub const ALL: [Row; 3] = [Row::Robot, Row::Vehicle, Row::Sticks];

    pub fn index(self) -> usize {
        self as usize
    }

    /// Its schemes, in order.
    pub fn schemes(self) -> &'static [Scheme] {
        match self {
            Row::Robot => &ROBOT,
            Row::Vehicle => &VEHICLE,
            Row::Sticks => &STICKS,
        }
    }

    /// Its label (a UIText.int key and the English).
    pub fn text(self) -> (&'static str, &'static str) {
        match self {
            Row::Robot => ("Settings.RobotControls", "Robot"),
            Row::Vehicle => ("Settings.VehicleControls", "Vehicle"),
            Row::Sticks => ("Settings.StickLayout", "Thumbsticks"),
        }
    }

    /// Its id in settings.txt.
    pub fn id(self) -> &'static str {
        match self {
            Row::Robot => "controller_robot_layout",
            Row::Vehicle => "controller_vehicle_layout",
            Row::Sticks => "controller_thumbsticks",
        }
    }

    /// Its mode (SchemeMode: the thumbsticks' is 3, after the campaign's
    /// Grimlock's).
    fn mode(self) -> u8 {
        match self {
            Row::Robot => 0,
            Row::Vehicle => 1,
            Row::Sticks => 3,
        }
    }
}

/// FindCommand: where `search` is in `line` as a command of its own (between
/// spaces, bars or the ends), if it is.
pub fn find_command(line: &str, search: &str) -> Option<usize> {
    if search.is_empty() {
        return None;
    }
    let i = line.find(search)?;
    let end = i + search.len();
    let apart = |c: Option<char>| c.is_none_or(|c| c == ' ' || c == '|');
    if apart(line[..i].chars().next_back()) && apart(line[end..].chars().next()) {
        Some(i)
    } else {
        find_command(&line[end..], search).map(|j| end + j)
    }
}

/// SetBindNoSave: one of a scheme's changes made.
fn rebind(bindings: &mut Bindings, r: &Rebind) {
    let found = bindings.iter().position(|(key, command)| match r.key {
        Some(k) => *key == k,
        None => find_command(command, r.old).is_some(),
    });
    let Some(i) = found else {
        if let Some(key) = r.key {
            bindings.push((key, r.new.to_string()));
        }
        return;
    };
    let command = &mut bindings[i].1;
    if r.old.is_empty() {
        // (Command @= "|" @ NewCommand.)
        *command = format!("{command} | {}", r.new);
    } else if let Some(at) = find_command(command, r.old) {
        command.replace_range(at..at + r.old.len(), r.new);
    }
}

/// The controller's bindings with the schemes chosen (`chosen`: each row's, an
/// index into its schemes), all three active together as they are while a robot or
/// vehicle plays (SetPrimaryControlMode).
pub fn bindings(chosen: [usize; 3]) -> Bindings {
    let mut bindings: Bindings = DEFAULT_BINDINGS
        .iter()
        .map(|&(k, c)| (k, c.to_string()))
        .collect();
    for &(key, command) in &PAD_BINDINGS {
        match bindings.iter_mut().find(|(k, _)| *k == key) {
            Some(b) => b.1 = command.to_string(),
            None => bindings.push((key, command.to_string())),
        }
    }
    for row in Row::ALL {
        let schemes = row.schemes();
        let scheme = &schemes[chosen[row.index()].min(schemes.len() - 1)];
        for r in scheme.rebinds {
            rebind(&mut bindings, r);
        }
    }
    bindings
}

/// TnPlayerInputCommandDescriptions: a command, the modes it is shown in, and its
/// description (TransGame.int), in the game's order (the first that a command is
/// counts), without the campaign's Grimlock's (mode 2: Execute, Grab/Throw).
const DESCRIPTIONS: [(&str, &[u8], &str); 21] = [
    ("PressJump", &[0, 1], "Jump"),
    ("Ascend", &[1], "Ascend (Jet)"),
    ("ChangeHandedness", &[0], "Switch Camera"),
    ("Descend", &[1], "Descend (Jet)"),
    ("ShowScores", &[0, 1], "Scoreboard (MP)"),
    ("UseInteract", &[0], "Interact"),
    ("NextWeapon", &[0, 1], "Switch Weapon"),
    ("Fire", &[0, 1], "Fire Weapon"),
    ("FineAim", &[0], "Fine Aim"),
    ("Block", &[0], "Block"),
    ("Boost", &[1], "Boost"),
    ("Ability0", &[0, 1], "Ability"),
    ("Ability1", &[0], "Dash(Press)/Sprint(Hold)"),
    ("Melee", &[0], "Melee"),
    ("Transform", &[0, 1], "Change Form"),
    ("Reload", &[0, 1], "Reload"),
    ("OffensiveEquipment", &[0], "Assault T.E.C.H. (SP)"),
    ("DefensiveEquipment", &[0], "Utility T.E.C.H. (SP)"),
    ("showmenu", &[0, 1], "Pause"),
    ("aStrafe", &[0, 1, 3], "Move"),
    ("aTurn", &[0, 1, 3], "Camera"),
];

/// GetKeyDescription: what `key` does in `row`'s mode, by its bound commands'
/// descriptions for that mode, "/" between them ("" for nothing).
pub fn description(bindings: &Bindings, key: &str, row: Row) -> String {
    let Some((_, command)) = bindings.iter().find(|(k, _)| *k == key) else {
        return String::new();
    };
    let mode = row.mode();
    let mut out: Vec<&str> = Vec::new();
    for part in command.split('|') {
        let part = part.to_ascii_uppercase();
        let found = DESCRIPTIONS
            .iter()
            .find(|(c, _, _)| find_command(&part, &c.to_ascii_uppercase()).is_some());
        if let Some((_, modes, text)) = found
            && modes.contains(&mode)
        {
            out.push(text);
        }
    }
    out.join("/")
}

/// The scheme ids saved for each row (settings.txt), as indices (Default for one
/// not known).
pub fn parse_chosen(get: impl Fn(&str) -> Option<String>) -> [usize; 3] {
    Row::ALL.map(|row| {
        get(row.id())
            .and_then(|id| row.schemes().iter().position(|s| s.id == id))
            .unwrap_or(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYS: [&str; 18] = [
        "XboxTypeS_LeftShoulder",
        "XboxTypeS_LeftTrigger",
        "XboxTypeS_Back",
        "XboxTypeS_LeftX",
        "XboxTypeS_LeftThumbstick",
        "XboxTypeS_DPad_Up",
        "XboxTypeS_DPad_Left",
        "XboxTypeS_DPad_Right",
        "XboxTypeS_DPad_Down",
        "XboxTypeS_RightShoulder",
        "XboxTypeS_RightTrigger",
        "XboxTypeS_Start",
        "XboxTypeS_Y",
        "XboxTypeS_B",
        "XboxTypeS_X",
        "XboxTypeS_A",
        "XboxTypeS_RightX",
        "XboxTypeS_RightThumbstick",
    ];

    fn labels(chosen: [usize; 3], row: Row) -> Vec<String> {
        let b = bindings(chosen);
        KEYS.iter().map(|k| description(&b, k, row)).collect()
    }

    /// The robot's default layout is labelled as the game's Controller Layout
    /// screen labels it (the user's screenshot of it).
    #[test]
    fn the_robots_default_layout_reads_as_the_game_labels_it() {
        let l = labels([0; 3], Row::Robot);
        let want = [
            "Dash(Press)/Sprint(Hold)",
            "Fine Aim",
            "Scoreboard (MP)",
            "Move",
            "Change Form",
            "",
            "Assault T.E.C.H. (SP)",
            "Utility T.E.C.H. (SP)",
            "",
            "Ability",
            "Fire Weapon",
            "Pause",
            "Switch Weapon",
            "Switch Camera",
            "Interact/Reload",
            "Jump",
            "Camera",
            "Melee",
        ];
        assert_eq!(l, want);
    }

    /// The schemes move the commands as the game's ini has them.
    #[test]
    fn the_schemes_move_their_controls() {
        // Tactician: melee on B, switch camera on RS.
        let l = labels([1, 0, 0], Row::Robot);
        assert_eq!((l[13].as_str(), l[17].as_str()), ("Melee", "Switch Camera"));
        // Striker: change form on Y, swap weapons on the d-pad's up, LS nothing.
        let l = labels([2, 0, 0], Row::Robot);
        assert_eq!(
            (l[12].as_str(), l[5].as_str(), l[4].as_str()),
            ("Change Form", "Switch Weapon", "")
        );
        // Charger: change form on LB, dash and sprint on LS.
        let l = labels([3, 0, 0], Row::Robot);
        assert_eq!(
            (l[0].as_str(), l[4].as_str()),
            ("Change Form", "Dash(Press)/Sprint(Hold)")
        );
        // Southpaw: the bumpers and the triggers swapped.
        let l = labels([4, 0, 0], Row::Robot);
        assert_eq!(
            [&l[0], &l[9], &l[1], &l[10]].map(|s| s.as_str()),
            [
                "Ability",
                "Dash(Press)/Sprint(Hold)",
                "Fire Weapon",
                "Fine Aim"
            ]
        );
        // Hoarder: interact and reload on the d-pad's up, change form on X.
        let l = labels([5, 0, 0], Row::Robot);
        assert_eq!(
            [&l[5], &l[14], &l[4]].map(|s| s.as_str()),
            ["Interact/Reload", "Change Form", ""]
        );
        // The vehicle: Default, then Seeker (a jet's ascend on B, descend on A).
        let l = labels([0; 3], Row::Vehicle);
        assert_eq!(
            [&l[15], &l[13], &l[1], &l[14]].map(|s| s.as_str()),
            ["Jump/Ascend (Jet)", "Descend (Jet)", "Boost", "Reload"]
        );
        let l = labels([0, 1, 0], Row::Vehicle);
        assert_eq!(
            (l[15].as_str(), l[13].as_str()),
            ("Jump/Descend (Jet)", "Ascend (Jet)")
        );
        // The thumbsticks: only the sticks have labels; Southpaw swaps them, for
        // the robot too.
        let l = labels([0; 3], Row::Sticks);
        let shown: Vec<_> = l.iter().filter(|s| !s.is_empty()).collect();
        assert_eq!(shown, ["Move", "Camera"]);
        let l = labels([0, 0, 1], Row::Sticks);
        assert_eq!((l[3].as_str(), l[16].as_str()), ("Camera", "Move"));
        let l = labels([0, 0, 1], Row::Robot);
        assert_eq!((l[3].as_str(), l[16].as_str()), ("Camera", "Move"));
    }

    #[test]
    fn a_command_is_found_only_as_itself() {
        assert_eq!(
            find_command("FineAim | Boost | ToggleBoost", "Boost"),
            Some(10)
        );
        assert_eq!(find_command("ToggleBoost", "Boost"), None);
        assert_eq!(find_command("ToggleBoost | Boost", "Boost"), Some(14));
        assert_eq!(find_command("|onrelease showmenu", "showmenu"), Some(11));
        assert_eq!(find_command("anything", ""), None);
    }
}
