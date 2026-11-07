//! The Choose Character screen of a match, from the game's multiplayer: the movie
//! UI_GFxCustomize_p.CustomTransformers_GFX played with selectOnly, as the game's
//! multiplayer UI controller plays it.
//!
//! The game shows it as a match starts (TnUIControllerMultiplayer's InGameLobbyUI:
//! the player picks a character before he plays, and can't back out of it), and
//! from the pause menu's Choose Character (openCharacterSelect, with backout: Back
//! returns to the pause menu). It lists the builds made on the main menu's
//! Customize screen (customize.rs, kept in builds.txt), three under each class with
//! the class's level bar, and shows the focused build's loadout beside them: its
//! vehicle, name and level, its two weapons and its ability, each with its upgrade.
//!
//! Choosing a build makes it Jazz's character (TnPlayerController.SelectCharacter):
//! main.rs gives him its two weapons as he spawns. Chosen as the match starts, the
//! countdown to the match starts with it (the starting scoreboard over the map,
//! pregame.rs: until the match begins, the screen and the countdown show the map
//! through its level camera, with no Jazz on it), and Jazz spawns with it as the
//! match begins; chosen from the pause menu, the pause menu closes with it
//! (onCustomizeClose = closePauseMenu) and the new character is used when Jazz
//! next respawns, as the game's notification says (TnUIControllerMultiplayer.
//! OnCharacterSelected: UI_GFxOnlineNotifies_p.OnlineNotify_GFX, drawn here too).
//!
//! A class whose characters the demo hasn't got can't be chosen: its builds are
//! listed, dimmed as the movie's panel buttons are when disabled, and can't be
//! focused. Online, a match has one of each class: a class another player has
//! chosen is dimmed the same way, with its label (follow_classes).
//!
//! It is drawn through flash.rs in the movie's numbers (stage pixels): the root's
//! black and backdrop and its header, mc_customSelection (sprite 216) at (90, 155)
//! and mc_characterOverview (sprite 202) at (525, 275), with the images
//! tools/build_customize.py makes (assets/choose) and its text (customize.txt).

use crate::customize::{Build, CustData, SLOTS, SPECIALTIES};
use crate::customize_ui::CustImages;
use crate::flash::{
    Align, Canvas, Cx, DrawCmd, Ease, FlashFont, FlashFrame, FlashView, Glow, M3, STAGE_W,
    TextSpec, Tween, key,
};
use crate::menu::{AppState, Pause};
use crate::sound::Sfx;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use std::collections::HashMap;

const BUTTONS: usize = SLOTS * 4;

const IMG_DIR: &str = "choose/img";
const SHAPES_FILE: &str = "choose/shapes.txt";
const IMAGES: [&str; 13] = [
    "backdrop",
    "fade",
    "select_bar",
    "card_weapon",
    "card_ability",
    "corner",
    "class_vehicles",
    "chassis_car",
    "chassis_truck",
    "chassis_tank",
    "chassis_jet",
    "notify_panel",
    "notify_corner",
];
/// Each class's vehicle icon (attachIcon in updateDisplay), in SPECIALTIES' order.
const CHASSIS: [&str; 4] = [
    "chassis_car",
    "chassis_truck",
    "chassis_tank",
    "chassis_jet",
];

// ---------------------------------------------------------------------------
// The movie's numbers
// ---------------------------------------------------------------------------

/// background_BLACK_mc: black down to y 473.4, fading out by 714.6 (its gradient),
/// brought in to 40% over a second.
const BLACK_TOP: f32 = 473.4;
const BLACK_BOTTOM: f32 = 714.6;
const BLACK_ALPHA: f32 = 0.4;
const BLACK_SECS: f32 = 1.0;
/// The header: lobbyMode_txt, in header_mc (-80.85, 171.85) and its clip at
/// (x, -101.5), x sliding from 160 to 190 (and fading in) over frames 1 to 11.
const TITLE_Y: f32 = 171.85 - 101.5;
const TITLE_X: (f32, f32) = (-80.85 + 160.0, -80.85 + 190.0);
/// mc_customSelection's place, and mc_characterOverview's.
const LIST_AT: (f32, f32) = (90.0, 155.0);
const PANEL_AT: (f32, f32) = (525.0, 275.0);
/// The build buttons (button01_mc..button12_mc, the shared mc_panelButton): three
/// per class in SPECIALTIES' order, at x 29.4; 350 x 21 panels whose label sits at
/// y -4.3 (updatePanelSize) and x 12 (as placed), 0 while focused ('over') and 10
/// once it has gone 'out'.
const BUTTON_X: f32 = 29.4;
const BUTTON_Y: [f32; BUTTONS] = [
    46.0, 69.0, 92.0, 169.0, 192.0, 215.0, 292.0, 315.0, 338.0, 415.0, 438.0, 461.0,
];
const BUTTON_W: f32 = 350.0;
const BUTTON_H: f32 = 21.0;
const LABEL_Y: f32 = -4.325;
const LABEL_X_START: f32 = 12.0;
const LABEL_X_OVER: f32 = 0.0;
const LABEL_X_OUT: f32 = 10.0;
/// The label: #00FFD3, #034A3D over the selection bar (0.2 s).
const LABEL_OUT: [f32; 3] = [0.0, 255.0, 211.0];
const LABEL_OVER: [f32; 3] = [3.0, 74.0, 61.0];
const FOCUS_SECS: f32 = 0.2;
/// selection_mc: at (12.4, focus y + 11), scaled (1.2734, 1.0625), its 256 x 32
/// art scale-9 (10 / 13 / 6 / 13 pixels kept), moving 0.2 s.
const BAR_X: f32 = 12.4;
const BAR_SCALE: (f32, f32) = (1.273_391_7, 1.0625);
const BAR_START_Y: f32 = 55.95;
/// The class labels (scoutLabel_txt..., x 22) and their bars (x 22.6, 300 x 4.5):
/// #99FFFF at 35% under the class's progress through its level, #FFAF5D.
const CLASS_LABEL_Y: [f32; 4] = [10.6, 133.4, 256.4, 379.7];
const CLASS_BAR_Y: [f32; 4] = [39.0, 162.0, 285.0, 408.0];
const CLASS_BAR: [f32; 4] = [22.6, 0.0, 300.0, 4.5];
const TRACK: [f32; 3] = [153.0, 255.0, 255.0];
const TRACK_ALPHA: f32 = 0.351_562_5;
const PROGRESS: [f32; 3] = [255.0, 175.0, 93.0];
/// The weapon slots (mc_btnWeaponSlot1, twice) and the ability (mc_btnAbility1) in
/// charOverviewMenu_mc.
const WEAPON_AT: [(f32, f32); 2] = [(-73.0, -107.0), (226.0, -106.95)];
const ABILITY_AT: (f32, f32) = (-73.0, 149.0);
/// attachIcon's delays: the second weapon a tenth of a second after the first, the
/// ability 0.35 s; each fades in over half a second (Regular.easeIn) as its corners
/// spread out (border_mc's 'animate', frames 1 to 9).
const ICON_DELAY: [f32; 3] = [0.0, 0.1, 0.35];
const ICON_FADE_SECS: f32 = 0.5;
const CORNER_FRAMES: f32 = 8.0;
/// The typing of a field's new text (updateTextField's tweenFrom): this long per
/// character.
const TYPE_SECS: f32 = 0.05;
/// The vehicle icon: classHexagon_mc at (-17.85, -89.5) scaled 1.1308, the icon at
/// 220% in it, in #FFA346.
const CHASSIS_AT: (f32, f32) = (-17.85, -89.5);
const CHASSIS_SCALE: f32 = 1.130_798_3 * 2.2;
const CHASSIS_COLOR: [f32; 3] = [255.0, 163.0, 70.0];

/// The text fields' filters: the titles' cyan glows, and the black glow (a drop
/// shadow at no distance) behind the loadout's lines.
const LABEL_GLOW: Glow = Glow {
    color: [51, 255, 255],
    strength: 0.75,
    blur: 8.0,
};
const NAME_GLOW: Glow = Glow {
    color: [51, 204, 255],
    strength: 1.0,
    blur: 10.0,
};
const SHADOW: Glow = Glow {
    color: [0, 0, 0],
    strength: 1.0,
    blur: 5.0,
};

/// OnlineNotify_GFX's message (mc_notifyMessage at messageAnchor_mc (860, 610)):
/// it opens over four frames, grows to its text over 0.2 s, shows for four
/// seconds from when it came, then drops 20 pixels as it fades out (0.5 s).
const NOTICE_AT: (f32, f32) = (860.0, 610.0);
const NOTICE_SECS: f32 = 4.0;
const NOTICE_OUT_SECS: f32 = 0.5;
/// Frames 0-4: the panel's x scale, x and alpha; the corner strokes' x scale and
/// alpha.
const NOTICE_PANEL_KEYS: [(f32, f32, f32); 5] = [
    (0.277, -37.5, 0.0),
    (0.722, -94.4, 0.218_75),
    (1.039, -135.05, 0.375),
    (1.229, -159.45, 0.468_75),
    (1.293, -167.5, 0.5),
];
const NOTICE_FRAME_KEYS: [(f32, f32); 5] = [
    (0.059, 0.0),
    (0.471, 0.4375),
    (0.765, 0.75),
    (0.941, 0.9375),
    (1.0, 1.0),
];
/// The panel's height as placed (128 x 0.805), and the message's height once its
/// text is in (content_mc's 56 plus 16): the panel grows to it plus 50.
const NOTICE_PANEL_H: f32 = 128.0 * 0.805;
const NOTICE_MESSAGE_H: f32 = 56.0 + 16.0;
/// The corner strokes' clip (sprite 7) turns cyan and back every 85 frames
/// (frames 48 to 83): its colour multiplier, and its green and blue offsets.
pub(crate) const NOTICE_PULSE: [(f32, f32); 36] = [
    (0.988, 3.0),
    (0.949, 13.0),
    (0.891, 28.0),
    (0.801, 50.0),
    (0.691, 79.0),
    (0.555, 113.0),
    (0.395, 154.0),
    (0.211, 201.0),
    (0.0, 255.0),
    (0.070, 237.0),
    (0.137, 220.0),
    (0.203, 203.0),
    (0.266, 187.0),
    (0.324, 172.0),
    (0.383, 157.0),
    (0.438, 143.0),
    (0.488, 130.0),
    (0.539, 117.0),
    (0.586, 105.0),
    (0.633, 94.0),
    (0.672, 83.0),
    (0.715, 73.0),
    (0.750, 64.0),
    (0.785, 55.0),
    (0.816, 47.0),
    (0.844, 39.0),
    (0.871, 33.0),
    (0.898, 26.0),
    (0.918, 21.0),
    (0.938, 16.0),
    (0.953, 12.0),
    (0.969, 8.0),
    (0.980, 5.0),
    (0.988, 3.0),
    (0.996, 1.0),
    (1.0, 0.0),
];
const NOTICE_PULSE_FIRST: usize = 48;
const NOTICE_PULSE_FRAMES: usize = 85;

const SND_UP: &str = "BL_HUD_INTERFACE.BUTTON_UP";
const SND_DOWN: &str = "BL_HUD_INTERFACE.BUTTON_DOWN";
const SND_ACCEPT: &str = "BL_HUD_INTERFACE.BUTTON_ACCEPT";
const SND_BACK: &str = "BL_HUD_INTERFACE.BUTTON_BACK";

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Between the pause menu and this screen.
#[derive(Resource, Default)]
pub struct ChooseLink {
    /// The screen's data is there (customize.txt and assets/choose).
    pub available: bool,
    /// Set by the pause menu: open the screen over it (openCharacterSelect).
    pub open_from_pause: bool,
    /// Set here as a screen opened from the pause menu closes: true when a
    /// character was chosen (the pause menu closes too), false for Back.
    pub closed: Option<bool>,
}

/// The character the player chose (his PlayerReplicationInfo's SelectedCharacter):
/// its build (class, slot), its two weapons and its ability and ability upgrade
/// (the game's ids; "" for none), which main.rs gives Jazz as he spawns (the EMP
/// grenade: emp.rs).
#[derive(Resource, Default, Clone)]
pub struct Chosen {
    pub build: Option<(usize, usize)>,
    pub weapons: Option<[String; 2]>,
    /// Its weapons' upgrades (the build's, in the weapons' order: upgrades.rs's
    /// pairs), installed on them as he takes them up.
    pub weapon_upgrades: [String; 2],
    pub ability: String,
    pub ability_upgrade: String,
    /// Its voice (the build's: a TnDataProvider_RobotVoice's id, or voice.rs's
    /// NONE), which he speaks with (voice.rs).
    pub voice: String,
    /// Its look (its parts and paint, character.rs) in the Autobot and Decepticon
    /// colours: Jazz wears it as he spawns (main.rs).
    pub looks: Option<[crate::character::Look; 2]>,
    /// He takes them up now: the match begins (pregame.rs), with what he chose as
    /// it started.
    pub arm_now: bool,
    /// The character chosen before this one (from the pause menu), his again should
    /// another player have taken this one's class at the same moment
    /// ([`follow_classes`]).
    previous: Option<Box<Chosen>>,
}

/// A field's text typing over the old (hms Tween on `text`: the new text's first
/// part, then what is left of the old).
#[derive(Clone, Debug, Default)]
struct Typed {
    from: String,
    to: String,
    at: f32,
    /// The field has been filled (until then it is hidden).
    shown: bool,
}

impl Typed {
    /// updateTextField: from whatever it shows now to `to`.
    fn set(&mut self, now: f32, to: String) {
        self.from = self.text(now);
        self.to = to;
        self.at = now;
        self.shown = true;
    }

    fn text(&self, now: f32) -> String {
        let n = self.to.chars().count();
        let secs = n as f32 * TYPE_SECS;
        if secs <= 0.0 || now >= self.at + secs {
            return self.to.clone();
        }
        let k = ((now - self.at) / secs).clamp(0.0, 1.0);
        let new = (k * n as f32) as usize;
        let old_n = self.from.chars().count();
        let old = (k * old_n as f32) as usize;
        self.to
            .chars()
            .take(new)
            .chain(self.from.chars().skip(old))
            .collect()
    }
}

/// A picture attached to a slot (attachIcon): it shows from `at` (its delay).
#[derive(Clone, Debug)]
struct Icon {
    id: String,
    at: f32,
}

/// mc_characterOverview: what it shows of the focused build.
#[derive(Default)]
struct Panel {
    build: Option<(usize, usize)>,
    name: String,
    weapon: [Typed; 2],
    weapon_upgrade: [Typed; 2],
    ability: Typed,
    ability_upgrade: Typed,
    /// The two weapons' pictures and the ability's.
    icons: [Option<Icon>; 3],
}

struct ScreenState {
    /// Opened from the pause menu (Back returns there), or as the match starts.
    from_pause: bool,
    opened: f32,
    builds: [[Build; SLOTS]; 4],
    /// The classes' experience and levels (xp.rs's, as the screen opened).
    progress: crate::xp::Progress,
    /// The focused button (class * SLOTS + slot), and whether it shows as
    /// focused ('over'; the mouse rolling off it leaves it 'out').
    focus: usize,
    over: bool,
    label_over: [Tween; BUTTONS],
    label_x: [Tween; BUTTONS],
    bar_y: Tween,
    /// The focused button pressed (Enter, A or a click): it is chosen as it comes
    /// up (HmMenu / HmButton's 'released').
    pressed: bool,
    /// The button under the mouse.
    hover: Option<usize>,
    last_cursor: Option<Vec2>,
    stick_y: f32,
    panel: Panel,
    /// The classes other players in the match have (one of each class in a match:
    /// [`follow_classes`]), greyed out.
    taken: [bool; 4],
}

/// A message of OnlineNotify_GFX: its title (the character's name), its message
/// (None: the game's, that it is used on respawn) and when it came.
struct Notice {
    title: String,
    message: Option<String>,
    at: f32,
}

#[derive(Resource, Default)]
pub(crate) struct ChooseScreen {
    state: Option<ScreenState>,
    notice: Option<Notice>,
    /// Who it is up for (split.rs's seat): as a match starts, the first player,
    /// then split screen's guest; from the pause menu, whoever opened it.
    seat: usize,
}

/// Split screen's guest's turn on the screen (as a match starts, once the first
/// player has chosen): a notice says so.
const GUEST_TURN_TITLE: &str = "PLAYER 2";
const GUEST_TURN_TEXT: &str = "Choose your character with your controller.";

#[derive(Resource)]
struct ChooseImages {
    images: HashMap<&'static str, Handle<Image>>,
    /// Where each vector shape's image goes, in the shape's own pixels.
    shapes: HashMap<String, [f32; 4]>,
}

impl ChooseImages {
    fn get(&self, name: &str) -> Handle<Image> {
        self.images.get(name).cloned().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct ChoosePlugin;

impl Plugin for ChoosePlugin {
    fn build(&self, app: &mut App) {
        // (CustomizePlugin, added first, has loaded customize.txt.)
        let assets = crate::assets_dir();
        let available = app.world().contains_resource::<CustData>()
            && assets.join(SHAPES_FILE).exists()
            && IMAGES
                .iter()
                .all(|n| assets.join(IMG_DIR).join(format!("{n}.png")).exists());
        if !available {
            info!("No Choose Character screen (tools/build_customize.py makes assets/choose)");
        }
        app.insert_resource(ChooseLink {
            available,
            ..default()
        })
        .init_resource::<Chosen>()
        .init_resource::<ChooseScreen>()
        .add_systems(Startup, load_images)
        .add_systems(OnEnter(AppState::Game), open_at_match_start)
        .add_systems(OnExit(AppState::Game), close_on_leaving)
        // (Its input comes after the pause menu's and before the game's: main.rs.)
        .add_systems(
            Update,
            draw_choose
                .after(choose_input)
                .before(crate::flash::FlashSystems),
        );
    }
}

fn load_images(mut commands: Commands, asset_server: Res<AssetServer>, link: Res<ChooseLink>) {
    if !link.available {
        return;
    }
    let images = IMAGES
        .iter()
        .map(|n| (*n, asset_server.load(format!("{IMG_DIR}/{n}.png"))))
        .collect();
    let text = std::fs::read_to_string(crate::assets_dir().join(SHAPES_FILE)).unwrap_or_default();
    let shapes = text
        .lines()
        .filter_map(|line| {
            let w: Vec<&str> = line.split_whitespace().collect();
            let v: Vec<f32> = w.iter().skip(1).filter_map(|s| s.parse().ok()).collect();
            (w.len() == 5 && v.len() == 4).then(|| (w[0].to_string(), [v[0], v[1], v[2], v[3]]))
        })
        .collect();
    commands.insert_resource(ChooseImages { images, shapes });
}

/// WaitingOnGameStart.OpenGameStartUI: the screen comes up as the match starts
/// (the character chosen in an earlier match is asked for again).
#[allow(clippy::too_many_arguments)]
fn open_at_match_start(
    time: Res<Time>,
    link: Res<ChooseLink>,
    data: Option<Res<CustData>>,
    mut chosen: crate::split::BySeat<Chosen>,
    mut screen: ResMut<ChooseScreen>,
    mut pause: ResMut<Pause>,
    progress: Res<crate::xp::Progress>,
    net: Res<crate::net::Net>,
) {
    for chosen in chosen.each() {
        *chosen = Chosen::default();
    }
    screen.notice = None;
    // (The first player first: split screen's guest after him.)
    screen.seat = 0;
    let Some(data) = data.filter(|_| link.available) else {
        return;
    };
    let taken = taken_classes(&net, 0);
    open(
        &mut screen,
        &data,
        &progress,
        taken,
        time.elapsed_secs(),
        false,
    );
    pause.choosing = true;
}

/// Leaving the game (Quit Game, or the online game ending): the screen goes.
fn close_on_leaving(mut screen: ResMut<ChooseScreen>, mut pause: ResMut<Pause>) {
    screen.state = None;
    screen.notice = None;
    screen.seat = 0;
    pause.choosing = false;
}

/// UpdatePlayableCharacterData: the builds as saved, the list focused on its first
/// button and the overview showing it (HmMenu.setFocus's selectionUpdated).
fn open(
    screen: &mut ChooseScreen,
    data: &CustData,
    progress: &crate::xp::Progress,
    taken: [bool; 4],
    now: f32,
    from_pause: bool,
) {
    let mut s = ScreenState {
        from_pause,
        opened: now,
        builds: crate::customize::load_builds(data, progress),
        progress: progress.clone(),
        focus: 0,
        over: false,
        label_over: [Tween::at(0.0); BUTTONS],
        label_x: [Tween::at(LABEL_X_START); BUTTONS],
        bar_y: Tween::at(BAR_START_Y),
        pressed: false,
        hover: None,
        last_cursor: None,
        stick_y: 0.0,
        panel: Panel::default(),
        taken,
    };
    let first = (0..BUTTONS).find(|&i| enabled(data, &s, i)).unwrap_or(0);
    focus(&mut s, data, first, now);
    screen.state = Some(s);
}

/// A build Jazz can play: of a class whose characters the demo has and no other
/// player in the match has, in a slot its level has opened (the others show
/// "Unlocks at level n", disabled: the game has no character there yet,
/// TnCharacterCustomizationData.GetSlotsForSpecialty).
fn enabled(data: &CustData, s: &ScreenState, button: usize) -> bool {
    let (class, slot) = (button / SLOTS, button % SLOTS);
    data.playable(class) && !s.taken[class] && slot_open(s, class, slot)
}

/// The classes other players in the match have chosen (net.rs), for the screen's
/// player (`seat`: split screen's guest's the first player's among them).
fn taken_classes(net: &crate::net::Net, seat: usize) -> [bool; 4] {
    let mut taken = [false; 4];
    let classes = match net.guest_slot().filter(|_| seat == 1) {
        Some(slot) => net.classes_taken_from(slot),
        None => net.classes_taken(),
    };
    for (class, _) in classes {
        if let Some(t) = taken.get_mut(class) {
            *t = true;
        }
    }
    taken
}

/// One of each class in a match (online): this PC's player's class goes to the
/// others (net.rs: his snapshots), the classes they have are greyed out on his
/// screen (the focus moving off one as it is taken), and should another player
/// have chosen the same class at the same moment from a lower slot, who keeps it,
/// this player gives it up: as the match starts he chooses again (the screen comes
/// back, the countdown waiting for him), and in the match his last character is his
/// again; a notice says who took the class.
#[allow(clippy::too_many_arguments)]
pub(crate) fn follow_classes(
    time: Res<Time>,
    data: Option<Res<CustData>>,
    link: Res<ChooseLink>,
    mut net: ResMut<crate::net::Net>,
    mut chosen: crate::split::BySeat<Chosen>,
    mut screen: ResMut<ChooseScreen>,
    mut pause: ResMut<Pause>,
    mut pre: ResMut<crate::pregame::PreMatch>,
    progress: Res<crate::xp::Progress>,
) {
    let now = time.elapsed_secs();
    // (Split screen's guest's class too: no one online plays alongside him, so
    // his is never taken at the same moment.)
    net.set_guest_class(chosen.seat(1).build.map(|(class, _)| class));
    let chosen = chosen.first();
    net.set_class(chosen.build.map(|(class, _)| class));
    let taken = taken_classes(&net, screen.seat);
    if let (Some(data), Some(s)) = (data.as_deref(), screen.state.as_mut())
        && s.taken != taken
    {
        s.taken = taken;
        if s.over && !enabled(data, s, s.focus) {
            match next_enabled(data, s, s.focus, true) {
                Some(b) => focus(s, data, b, now),
                None => go_out(s, now),
            }
        }
    }
    if !net.class_lost() {
        return;
    }
    let Some((class, _)) = chosen.build else {
        return;
    };
    let by = net
        .classes_taken()
        .into_iter()
        .find(|(c, _)| *c == class)
        .map(|(_, name)| name)
        .unwrap_or_default();
    let class_name = data
        .as_deref()
        .map(|d| d.class_name(class))
        .unwrap_or_default();
    let message = format!("{by} chose the {class_name} first.");
    if pre.waiting {
        // As the match starts: he chooses again.
        *chosen = Chosen::default();
        net.set_class(None);
        pre.choose_again();
        if let Some(data) = data.filter(|_| link.available) {
            screen.seat = 0;
            open(&mut screen, &data, &progress, taken, now, false);
            pause.choosing = true;
            screen.notice = Some(Notice {
                title: class_name.to_uppercase(),
                message: Some(message),
                at: now,
            });
        }
    } else {
        // In the match: his last character again (or, had he none, no class of
        // his until he chooses again).
        let previous = chosen.previous.take();
        *chosen = previous.map(|p| *p).unwrap_or_default();
        net.set_class(chosen.build.map(|(class, _)| class));
        let title = match (data.as_deref(), chosen.build) {
            (Some(d), Some((c, slot))) => {
                let builds = crate::customize::load_builds(d, &progress);
                let name = &builds[c][slot].name;
                if name.is_empty() {
                    format!("{} {}", d.class_name(c), slot + 1)
                } else {
                    name.clone()
                }
            }
            _ => class_name.to_uppercase(),
        };
        screen.notice = Some(Notice {
            title,
            message: Some(message),
            at: now,
        });
    }
}

fn slot_open(s: &ScreenState, class: usize, slot: usize) -> bool {
    slot < crate::xp::builds_open(s.progress.level(class))
}

/// A build's name as the list and overview show it (a new build is named after
/// its class and slot).
fn build_name(data: &CustData, s: &ScreenState, button: usize) -> String {
    let (class, slot) = (button / SLOTS, button % SLOTS);
    if !slot_open(s, class, slot) {
        return data
            .string("Customization.UnlockLevel", "Unlocks at level `n")
            .replace("`n", &crate::xp::build_unlock_level(slot).to_string());
    }
    let name = &s.builds[class][slot].name;
    if name.is_empty() {
        format!("{} {}", data.class_name(class), slot + 1)
    } else {
        name.clone()
    }
}

/// setFocus: the old button goes 'out', the new one 'over' (its label slides to
/// the left edge and darkens), the selection bar moves to it, and the overview
/// shows its build.
fn focus(s: &mut ScreenState, data: &CustData, button: usize, now: f32) {
    if s.over && s.focus != button {
        go_out(s, now);
    }
    s.focus = button;
    s.over = true;
    s.pressed = false;
    s.label_over[button].to(now, 1.0, FOCUS_SECS, Ease::RegularOut);
    s.label_x[button].to(now, LABEL_X_OVER, FOCUS_SECS, Ease::RegularOut);
    s.bar_y
        .to(now, BUTTON_Y[button] + 11.0, FOCUS_SECS, Ease::RegularOut);
    show_build(s, data, button, now);
}

/// The focused button goes 'out' (the selection bar hides with it).
fn go_out(s: &mut ScreenState, now: f32) {
    let b = s.focus;
    s.over = false;
    s.pressed = false;
    s.label_over[b].to(now, 0.0, FOCUS_SECS, Ease::RegularOut);
    s.label_x[b].to(now, LABEL_X_OUT, FOCUS_SECS, Ease::RegularOut);
}

/// charOverviewMenu's updateDisplay for a build.
fn show_build(s: &mut ScreenState, data: &CustData, button: usize, now: f32) {
    let (class, slot) = (button / SLOTS, button % SLOTS);
    let b = s.builds[class][slot].clone();
    let name = build_name(data, s, button);
    let upgrade = |list: &[crate::customize::Item], id: &str| {
        // configUpgradeText: "Upgrade : <name>", or None.
        let label = data.string("CustomCharacter.SelectedUpgrade", "Upgrade");
        let what = CustData::item(list, id)
            .filter(|_| !id.is_empty())
            .map(|i| i.name.clone())
            .unwrap_or_else(|| data.string("CustomCharacter.None", "None"));
        format!("{label} : {what}")
    };
    let p = &mut s.panel;
    p.build = Some((class, slot));
    p.name = name;
    for w in 0..2 {
        let id = &b.weapons[w];
        let label = CustData::item(&data.weapons, id)
            .map(|i| i.name.clone())
            .unwrap_or_default();
        p.weapon[w].set(now, label);
        p.weapon_upgrade[w].set(now, upgrade(&data.weapon_upgrades, &b.weapon_upgrades[w]));
        attach(&mut p.icons[w], id, now + ICON_DELAY[w]);
    }
    if b.ability.is_empty() {
        p.ability
            .set(now, data.string("Customization.None", "No upgrade"));
        p.icons[2] = None;
    } else {
        let label = CustData::item(&data.abilities, &b.ability)
            .map(|i| i.name.clone())
            .unwrap_or_default();
        p.ability.set(now, label);
        p.ability_upgrade
            .set(now, upgrade(&data.skills, &b.ability_upgrade));
        attach(&mut p.icons[2], &b.ability, now + ICON_DELAY[2]);
    }
}

/// attachIcon: a new picture fades in after its delay (the same one stays as it is).
fn attach(icon: &mut Option<Icon>, id: &str, at: f32) {
    if icon.as_ref().is_some_and(|i| i.id == id) {
        return;
    }
    *icon = Some(Icon {
        id: id.to_string(),
        at,
    });
}

/// The next button that can be focused from `from`, one way round the list (the
/// buttons' focusUp / focusDown wrap from the last to the first).
fn next_enabled(data: &CustData, s: &ScreenState, from: usize, down: bool) -> Option<usize> {
    (1..BUTTONS)
        .map(|k| {
            if down {
                (from + k) % BUTTONS
            } else {
                (from + BUTTONS - k) % BUTTONS
            }
        })
        .find(|&i| enabled(data, s, i))
}

/// The button the stage point `p` is on.
fn button_at(p: Vec2) -> Option<usize> {
    let x = p.x - LIST_AT.0 - BUTTON_X;
    (0..BUTTONS).rev().find(|&i| {
        let y = p.y - LIST_AT.1 - BUTTON_Y[i];
        (0.0..BUTTON_W).contains(&x) && (-1.0..BUTTON_H + 1.0).contains(&y)
    })
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// The screen's input: keys, the gamepad and the mouse move the focus round the
/// builds that can be chosen, and choose or go back. (main.rs runs it after the
/// pause menu's input and before the game's input is put aside while it is up.)
#[allow(clippy::too_many_arguments)]
pub(crate) fn choose_input(
    time: Res<Time>,
    (keys, mouse): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>),
    gamepads: Query<(Entity, &Gamepad)>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    view: Res<FlashView>,
    data: Option<Res<CustData>>,
    mut link: ResMut<ChooseLink>,
    mut chosen: crate::split::BySeat<Chosen>,
    mut screen: ResMut<ChooseScreen>,
    mut pause: ResMut<Pause>,
    mut sfx: ResMut<Sfx>,
    chars: Res<crate::character::Characters>,
    progress: Res<crate::xp::Progress>,
    (net, split): (Res<crate::net::Net>, Res<crate::split::Split>),
) {
    let now = time.elapsed_secs();
    let Some(data) = data else {
        return;
    };
    if std::mem::take(&mut link.open_from_pause) && link.available {
        // (Up for whoever opened the pause menu: split screen's guest his own.)
        screen.seat = if split.active() { pause.seat.min(1) } else { 0 };
        let taken = taken_classes(&net, screen.seat);
        open(&mut screen, &data, &progress, taken, now, true);
        pause.choosing = true;
    }
    let screen = &mut *screen;
    let seat = screen.seat;
    let chosen = chosen.seat(seat);
    let Some(s) = screen.state.as_mut() else {
        return;
    };
    // The screen's player's controls: split screen's guest's controller (the
    // keyboard and mouse too should it have gone), the first player's the
    // keyboard, the mouse and the other controllers.
    let guest_pad = split.guest_pad().filter(|e| gamepads.contains(*e));
    let guest_turn = seat == 1;
    let desk = !guest_turn || guest_pad.is_none();
    let pads = gamepads.iter().filter(|(e, _)| {
        if guest_turn {
            Some(*e) == guest_pad
        } else {
            !crate::split::is_guest_pad(&split, *e)
        }
    });
    // The menu has the mouse while it is up.
    if cursor.grab_mode != CursorGrabMode::None || !cursor.visible {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
    let at = window
        .cursor_position()
        .filter(|_| desk)
        .map(|p| view.to_stage(p));
    if s.opened == now {
        // What opened it (the pause menu's Enter or click) is not for it; nor is
        // where the mouse already is.
        s.last_cursor = at;
        return;
    }

    // Keys, the gamepad's d-pad, stick, A and B.
    let key = |k: &[KeyCode]| desk && keys.any_just_pressed(k.iter().copied());
    let mut up = key(&[KeyCode::ArrowUp, KeyCode::KeyW]);
    let mut down = key(&[KeyCode::ArrowDown, KeyCode::KeyS]);
    let accept_keys = [KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space];
    let mut accept_down = key(&accept_keys);
    let mut accept_up = desk && keys.any_just_released(accept_keys);
    let clicked = |b: MouseButton| desk && mouse.just_pressed(b);
    let mut back = key(&[KeyCode::Escape, KeyCode::Backspace]) || clicked(MouseButton::Right);
    let mut stick = 0.0f32;
    for (_, pad) in pads {
        up |= pad.just_pressed(GamepadButton::DPadUp);
        down |= pad.just_pressed(GamepadButton::DPadDown);
        accept_down |= pad.just_pressed(GamepadButton::South);
        accept_up |= pad.just_released(GamepadButton::South);
        back |= pad.just_pressed(GamepadButton::East);
        let y = pad.left_stick().y;
        if y.abs() > stick.abs() {
            stick = y;
        }
    }
    if stick > 0.6 && s.stick_y <= 0.6 {
        up = true;
    }
    if stick < -0.6 && s.stick_y >= -0.6 {
        down = true;
    }
    s.stick_y = stick;

    // The mouse: rolling over a button focuses it, rolling off leaves it 'out'
    // (HmButton.onRollOver / onRollOut); a press and release on it chooses it.
    let moved = at.is_some() && at != s.last_cursor;
    s.last_cursor = at;
    if moved {
        let hover = at.and_then(button_at).filter(|&i| enabled(&data, s, i));
        if hover != s.hover {
            if s.hover.is_some() && s.over {
                go_out(s, now);
            }
            if let Some(b) = hover {
                focus(s, &data, b, now);
                sfx.play(SND_UP);
            }
            s.hover = hover;
        }
    }
    let mut chosen_now = false;
    if clicked(MouseButton::Left) && s.hover.is_some() && s.over {
        s.pressed = true;
    }
    if desk && mouse.just_released(MouseButton::Left) && std::mem::take(&mut s.pressed) {
        if s.hover == Some(s.focus) && s.over {
            chosen_now = true;
        }
    }

    // Up and down move the focus round the builds that can be chosen.
    if !chosen_now && (up || down) {
        if let Some(b) = next_enabled(&data, s, s.focus, down) {
            focus(s, &data, b, now);
            sfx.play(if down { SND_DOWN } else { SND_UP });
        }
    }
    // A (Enter): pressed on the focused button, it chooses as it comes up.
    if accept_down && s.over {
        s.pressed = true;
    }
    if accept_up && std::mem::take(&mut s.pressed) && s.over {
        chosen_now = true;
    }

    if chosen_now {
        // processPress: SelectCharacter, then the screen closes (and, from the
        // pause menu, the pause menu with it).
        sfx.play(SND_ACCEPT);
        let (class, slot) = (s.focus / SLOTS, s.focus % SLOTS);
        let build = &s.builds[class][slot];
        let is_new = chosen.build.is_some() && chosen.build != Some((class, slot));
        // (The one before, his again should another player have taken this class
        // at the same moment: follow_classes.)
        let previous = chosen.build.is_some().then(|| {
            let mut before = chosen.clone();
            before.previous = None;
            Box::new(before)
        });
        chosen.previous = previous;
        chosen.build = Some((class, slot));
        chosen.weapons = Some(build.weapons.clone());
        chosen.weapon_upgrades = build.weapon_upgrades.clone();
        chosen.ability = build.ability.clone();
        chosen.ability_upgrade = build.ability_upgrade.clone();
        chosen.voice = build.voice.clone();
        chosen.looks = Some(crate::customize::build_looks(&data, &chars, class, build));
        let name = build_name(&data, s, s.focus);
        let from_pause = s.from_pause;
        screen.state = None;
        // As the match starts, split screen's guest chooses next (the first
        // player's class out of his reach), the screen up till he has.
        if !from_pause && seat == 0 && net.guest_slot().is_some() {
            let mut taken = taken_classes(&net, 1);
            if let Some(t) = taken.get_mut(class) {
                *t = true;
            }
            screen.seat = 1;
            open(screen, &data, &progress, taken, now, false);
            screen.notice = Some(Notice {
                title: GUEST_TURN_TITLE.to_string(),
                message: Some(GUEST_TURN_TEXT.to_string()),
                at: now,
            });
            return;
        }
        screen.seat = 0;
        pause.choosing = false;
        if from_pause {
            // OnCharacterSelected: a different character than before is used on
            // respawn, as the notification says.
            if is_new {
                screen.notice = Some(Notice {
                    title: name,
                    message: None,
                    at: now,
                });
            }
            link.closed = Some(true);
        } else {
            // The countdown to the match starts (pregame.rs: as it runs out Jazz
            // takes the character up and the match starts for him); the game has
            // the mouse back.
            pause.just_closed = true;
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        }
        return;
    }
    if back {
        // closeMenu (B): back to the pause menu when opened from it; as the match
        // starts there is no going back.
        sfx.play(SND_BACK);
        if s.from_pause {
            screen.state = None;
            screen.seat = 0;
            pause.choosing = false;
            link.closed = Some(false);
        }
    }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

struct Painter<'a> {
    cmds: Vec<DrawCmd>,
    img: &'a ChooseImages,
}

impl Painter<'_> {
    fn canvas(&mut self) -> Canvas<'_> {
        Canvas {
            cmds: &mut self.cmds,
        }
    }

    fn image(&mut self, k: u64, name: &str, rect: [f32; 4], m: &M3, cx: Cx) {
        let h = self.img.get(name);
        self.canvas().image(k, &h, rect, m, cx);
    }

    fn handle(&mut self, k: u64, h: &Handle<Image>, rect: [f32; 4], m: &M3, cx: Cx) {
        self.canvas().image(k, h, rect, m, cx);
    }

    fn fill(&mut self, k: u64, rect: [f32; 4], m: &M3, color: [f32; 3], alpha: f32) {
        self.canvas().fill(k, rect, m, color, alpha);
    }

    fn text(&mut self, k: u64, s: TextSpec, x: f32, y: f32, m: &M3, cx: Cx) {
        if s.text.is_empty() {
            return;
        }
        self.canvas().text(k, s, x, y, m, cx);
    }
}

#[allow(clippy::too_many_arguments)]
fn spec(
    text: impl Into<String>,
    font: FlashFont,
    size: f32,
    color: [u8; 3],
    width: f32,
    glow: Option<Glow>,
) -> TextSpec {
    TextSpec {
        text: text.into(),
        font,
        size,
        color,
        align: Align::Left,
        width,
        shadow: None,
        glow,
        zoom: 1.0,
    }
}

fn rgb(c: [f32; 3]) -> [u8; 3] {
    c.map(|v| v.round().clamp(0.0, 255.0) as u8)
}

fn draw_choose(
    time: Res<Time>,
    data: Option<Res<CustData>>,
    images: Option<Res<ChooseImages>>,
    wires: Option<Res<CustImages>>,
    mut screen: ResMut<ChooseScreen>,
    mut frame: ResMut<FlashFrame>,
) {
    let now = time.elapsed_secs();
    let (Some(data), Some(img)) = (data, images) else {
        return;
    };
    if screen
        .notice
        .as_ref()
        .is_some_and(|n| now - n.at >= NOTICE_SECS + NOTICE_OUT_SECS)
    {
        screen.notice = None;
    }
    if screen.state.is_none() && screen.notice.is_none() {
        return;
    }
    let mut p = Painter {
        cmds: Vec::new(),
        img: &img,
    };
    if let Some(s) = &screen.state {
        draw_root(&mut p, &data, s, now);
        draw_list(&mut p, &data, s, now);
        draw_panel(&mut p, &data, wires.as_deref(), s, now);
    }
    if let Some(n) = &screen.notice {
        draw_notice(&mut p, &data, n, now);
    }
    frame.cmds.extend(p.cmds);
}

/// The root: background_BLACK_mc, the backdrop (shape 424) and the header.
fn draw_root(p: &mut Painter, data: &CustData, s: &ScreenState, now: f32) {
    let t = now - s.opened;
    let black = BLACK_ALPHA * Ease::RegularOut.apply(t / BLACK_SECS);
    p.fill(
        key("ch_black", 0),
        [-2000.0, -2000.0, STAGE_W + 2000.0, BLACK_TOP],
        &M3::IDENTITY,
        [0.0; 3],
        black,
    );
    p.image(
        key("ch_black", 1),
        "fade",
        [-2000.0, BLACK_TOP, STAGE_W + 2000.0, BLACK_BOTTOM],
        &M3::IDENTITY,
        Cx::alpha(black),
    );
    p.image(
        key("ch_backdrop", 0),
        "backdrop",
        [0.0, 0.0, STAGE_W, 720.0],
        &M3::IDENTITY,
        Cx::IDENTITY,
    );
    // header_mc: frames 1 to 11 bring the title in.
    let k = Ease::RegularOut.apply((t * 30.0 - 1.0) / 10.0);
    let title = data
        .string("Customization.ChooseTransformer", "Choose Character")
        .to_uppercase();
    p.text(
        key("ch_title", 0),
        spec(
            title,
            FlashFont::Normal,
            36.0,
            [255; 3],
            505.0,
            Some(LABEL_GLOW),
        ),
        TITLE_X.0 + (TITLE_X.1 - TITLE_X.0) * k + 2.0,
        TITLE_Y + 2.0,
        &M3::IDENTITY,
        Cx::alpha(k),
    );
}

/// mc_customSelection: the classes and their builds.
fn draw_list(p: &mut Painter, data: &CustData, s: &ScreenState, now: f32) {
    let list = M3::translate(LIST_AT.0, LIST_AT.1, 0.0);
    // Shape 205: the classes' vehicles, black at half opacity, behind the list.
    if let Some(r) = p.img.shapes.get("class_vehicles").copied() {
        p.image(
            key("ch_vehicles", 0),
            "class_vehicles",
            r,
            &list,
            Cx::fill([0.0; 3], 0.5),
        );
    }
    // selection_mc, shown while a button is focused.
    if s.over {
        let y = s.bar_y.get(now);
        let (w, h) = (256.0 * BAR_SCALE.0, 32.0 * BAR_SCALE.1);
        let bar = p.img.get("select_bar");
        p.canvas().nine(
            key("ch_bar", 0),
            &bar,
            [BAR_X, y - h * 0.5, BAR_X + w, y + h * 0.5],
            [10.0, 13.0, 6.0, 13.0],
            [10.0 / 256.0, 13.0 / 32.0, 6.0 / 256.0, 13.0 / 32.0],
            &list,
            Cx::IDENTITY,
        );
    }
    // The build buttons' labels.
    for i in 0..BUTTONS {
        let over = s.label_over[i].get(now);
        let color = rgb(std::array::from_fn(|c| {
            LABEL_OUT[c] + (LABEL_OVER[c] - LABEL_OUT[c]) * over
        }));
        let alpha = if enabled(data, s, i) { 1.0 } else { 0.5 };
        let m = list.then(&M3::translate(BUTTON_X, BUTTON_Y[i], 0.0));
        p.text(
            key("ch_button", i),
            spec(
                build_name(data, s, i),
                FlashFont::Header,
                20.0,
                color,
                330.0,
                None,
            ),
            s.label_x[i].get(now),
            LABEL_Y,
            &m,
            Cx::alpha(alpha),
        );
    }
    // The class labels (upper-cased by the movie), then their level bars; a class
    // another player has is greyed out with its builds.
    for c in 0..4 {
        let label = data
            .string(
                &format!("Customization.{}", SPECIALTIES[c]),
                &data.class_name(c),
            )
            .to_uppercase();
        p.text(
            key("ch_class", c),
            spec(
                label,
                FlashFont::Normal,
                24.0,
                [255; 3],
                503.0,
                Some(LABEL_GLOW),
            ),
            22.0,
            CLASS_LABEL_Y[c],
            &list,
            Cx::alpha(if s.taken[c] { 0.5 } else { 1.0 }),
        );
    }
    let [x, _, w, h] = CLASS_BAR;
    for c in 0..4 {
        let y = CLASS_BAR_Y[c];
        p.fill(
            key("ch_track", c),
            [x, y, x + w, y + h],
            &list,
            TRACK,
            TRACK_ALPHA,
        );
    }
    // Each class's progress through its level (its <class>Prog_mc's width: the
    // experience past this level's start over what the next needs; at the top
    // level, kLevelTooHigh in its place, none).
    for c in 0..4 {
        let level = s.progress.level(c);
        let xp = s.progress.xp(c);
        let last = crate::xp::xp_for(level).unwrap_or(0);
        let progress = crate::xp::xp_for(level + 1)
            .filter(|&n| n > last)
            .map_or(0.0, |n| {
                (xp.saturating_sub(last) as f32 / (n - last) as f32).clamp(0.0, 1.0)
            });
        if progress <= 0.0 {
            continue;
        }
        let y = CLASS_BAR_Y[c];
        p.fill(
            key("ch_progress", c),
            [x, y, x + w * progress, y + h],
            &list,
            PROGRESS,
            1.0,
        );
    }
}

/// The corners round an icon (border_mc, sprites 133 and 101): from 8 x 10.5
/// pixels off its middle out to `half` over frames 1 to 9 of 'animate', fading in.
#[allow(clippy::too_many_arguments)]
fn draw_corners(
    p: &mut Painter,
    k: usize,
    m: &M3,
    mid: (f32, f32),
    half: (f32, f32),
    t: f32,
    cx: [f32; 3],
    tint: bool,
) {
    if t < 0.0 {
        return;
    }
    let c = Ease::RegularOut.apply(t * 30.0 / CORNER_FRAMES);
    let (hx, hy) = (8.0 + (half.0 - 8.0) * c, 10.5 + (half.1 - 10.5) * c);
    // sprite 100 (and its 50% sprite 99): the art's corner at (-16, -8) of it.
    let corners = [(-1.0, -1.0), (1.0, 1.0), (1.0, -1.0), (-1.0, 1.0)];
    for (i, (sx, sy)) in corners.iter().enumerate() {
        let at = M3::flat(-sx, 0.0, 0.0, -sy, mid.0 + sx * hx, mid.1 + sy * hy);
        let colour = if tint {
            Cx::fill(cx, 0.5 * c)
        } else {
            Cx::alpha(0.5 * c)
        };
        p.image(
            key("ch_corner", k * 4 + i),
            "corner",
            [-16.0, -8.0, 16.0, 8.0],
            &m.then(&at),
            colour,
        );
    }
}

/// mc_characterOverview: the focused build's loadout.
fn draw_panel(
    p: &mut Painter,
    data: &CustData,
    wires: Option<&CustImages>,
    s: &ScreenState,
    now: f32,
) {
    let panel = M3::translate(PANEL_AT.0, PANEL_AT.1, 0.0);
    let o = &s.panel;
    let icon_alpha = |icon: &Option<Icon>| {
        icon.as_ref().map_or(0.0, |i| {
            Ease::RegularIn.apply((now - i.at) / ICON_FADE_SECS)
        })
    };
    let shown_for = |icon: &Option<Icon>| icon.as_ref().map_or(-1.0, |i| now - i.at);
    let picture = |icon: &Option<Icon>| {
        icon.as_ref()
            .and_then(|i| wires.and_then(|w| w.wire(&i.id)))
    };

    // The ability (charOverviewMenu's lowest clip): its card, picture and lines.
    let ab = panel.then(&M3::translate(ABILITY_AT.0, ABILITY_AT.1, 0.0));
    p.image(
        key("ch_card", 2),
        "card_ability",
        [0.0, 55.0, 512.0, 183.0],
        &ab,
        Cx::alpha(0.488_281_25),
    );
    let icon_mc = ab.then(&M3::translate(153.9, 96.35, 0.0));
    let anchor = icon_mc.then(&M3::flat(0.449_997, 0.0, 0.0, 0.449_677, -49.0, 6.0));
    if let Some(h) = picture(&o.icons[2]) {
        let a = icon_alpha(&o.icons[2]);
        p.handle(
            key("ch_icon", 2),
            &h,
            [-128.0, -128.0, 128.0, 128.0],
            &anchor,
            Cx::fill([255.0, 255.0, 204.0], a),
        );
    }
    draw_corners(
        p,
        2,
        &icon_mc.then(&M3::translate(11.0, 8.5, 0.0)),
        (-60.0, 0.0),
        (58.0, 60.5),
        shown_for(&o.icons[2]),
        [0.0; 3],
        false,
    );
    if o.ability.shown {
        p.text(
            key("ch_ability", 0),
            spec(
                o.ability.text(now),
                FlashFont::Normal,
                22.0,
                [255, 163, 69],
                221.0,
                Some(SHADOW),
            ),
            191.0,
            81.0,
            &ab,
            Cx::IDENTITY,
        );
    }
    if o.ability_upgrade.shown {
        p.text(
            key("ch_ability", 1),
            spec(
                o.ability_upgrade.text(now),
                FlashFont::Normal,
                16.0,
                [193, 100, 4],
                253.0,
                Some(SHADOW),
            ),
            191.5,
            109.0,
            &ab,
            Cx::IDENTITY,
        );
    }

    // The weapon slots: card, picture (in #99FFFF, its corners with it) and lines.
    for w in 0..2 {
        let slot = panel.then(&M3::translate(WEAPON_AT[w].0, WEAPON_AT[w].1, 0.0));
        p.image(
            key("ch_card", w),
            "card_weapon",
            [0.0, 148.0, 256.0, 276.0],
            &slot,
            Cx::alpha(0.5),
        );
        let anchor = slot.then(&M3::translate(148.0, 143.0, 0.0));
        draw_corners(
            p,
            w,
            &anchor.then(&M3::translate(0.0, 0.5, 0.0)),
            (0.0, 0.0),
            (118.0, 60.5),
            shown_for(&o.icons[w]),
            [153.0, 255.0, 255.0],
            true,
        );
        if let Some(h) = picture(&o.icons[w]) {
            let a = icon_alpha(&o.icons[w]);
            p.handle(
                key("ch_icon", w),
                &h,
                [-125.0, -62.5, 125.0, 62.5],
                &anchor,
                Cx::fill([153.0, 255.0, 255.0], a),
            );
        }
        if o.weapon[w].shown {
            p.text(
                key("ch_weapon", w),
                spec(
                    o.weapon[w].text(now),
                    FlashFont::Normal,
                    22.0,
                    [153, 255, 255],
                    253.0,
                    Some(SHADOW),
                ),
                23.0,
                212.0,
                &slot,
                Cx::IDENTITY,
            );
        }
        if o.weapon_upgrade[w].shown {
            p.text(
                key("ch_wupgrade", w),
                spec(
                    o.weapon_upgrade[w].text(now),
                    FlashFont::Normal,
                    16.0,
                    [3, 169, 154],
                    253.0,
                    Some(SHADOW),
                ),
                23.0,
                240.0,
                &slot,
                Cx::IDENTITY,
            );
        }
    }

    let Some((class, _)) = o.build else {
        return;
    };
    // charLevel_txt: "LEVEL <n>", the build's class's.
    let level = data
        .string("CharacterSelect.CharacterLevel", "Level `c")
        .replace("`c", &s.progress.level(class).to_string())
        .to_uppercase();
    p.text(
        key("ch_level", 0),
        spec(
            level,
            FlashFont::Normal,
            18.0,
            [255, 163, 69],
            202.0,
            Some(SHADOW),
        ),
        60.0,
        -80.4,
        &panel,
        Cx::IDENTITY,
    );
    // classHexagon_mc: the class's vehicle.
    let chassis = CHASSIS[class];
    if let Some(r) = p.img.shapes.get(chassis).copied() {
        let m = panel
            .then(&M3::translate(CHASSIS_AT.0, CHASSIS_AT.1, 0.0))
            .then(&M3::scale(CHASSIS_SCALE, CHASSIS_SCALE));
        p.image(
            key("ch_chassis", 0),
            chassis,
            r,
            &m,
            Cx::fill(CHASSIS_COLOR, 1.0),
        );
    }
    // charID_txt: the build's name.
    p.text(
        key("ch_name", 0),
        spec(
            o.name.clone(),
            FlashFont::Header,
            32.0,
            [0, 255, 211],
            444.0,
            Some(NAME_GLOW),
        ),
        60.0,
        -117.0,
        &panel,
        Cx::IDENTITY,
    );
}

/// OnlineNotify_GFX's message: "<the character's name>" over "Selected character
/// used on respawn."
fn draw_notice(p: &mut Painter, data: &CustData, n: &Notice, now: f32) {
    let t = now - n.at;
    if t < 0.0 {
        return;
    }
    // fadeOut: down 20 pixels and away.
    let out = Ease::RegularOut.apply((t - NOTICE_SECS) / NOTICE_OUT_SECS);
    let alpha = 1.0 - out;
    let msg = M3::translate(NOTICE_AT.0, NOTICE_AT.1 + 20.0 * out, 0.0);
    // Frames 0-4 open it; at frame 4 its text is in and it grows to it (0.2 s).
    let f = (t * 30.0).min(4.0);
    let i = (f.floor() as usize).min(3);
    let k = f - i as f32;
    let lerp = |a: f32, b: f32| a + (b - a) * k;
    let (pa, pb) = (NOTICE_PANEL_KEYS[i], NOTICE_PANEL_KEYS[i + 1]);
    let (fa, fb) = (NOTICE_FRAME_KEYS[i], NOTICE_FRAME_KEYS[i + 1]);
    let grow = Ease::RegularOut.apply((t - 4.0 / 30.0) / 0.2);
    let panel_h = NOTICE_PANEL_H + (NOTICE_MESSAGE_H + 50.0 - NOTICE_PANEL_H) * grow;
    // background_mc: the panel (its art's middle at y 2), at most half opaque.
    let panel = msg.then(&M3::flat(
        lerp(pa.0, pb.0),
        0.0,
        0.0,
        panel_h / 128.0,
        lerp(pa.1, pb.1),
        2.0,
    ));
    p.image(
        key("ch_notice", 0),
        "notify_panel",
        [0.0, -64.0, 256.0, 64.0],
        &panel,
        Cx::alpha(lerp(pa.2, pb.2) * alpha),
    );
    // frameBottom_mc and frameTop_mc: the corner strokes, pulsing cyan.
    let frame_sx = lerp(fa.0, fb.0);
    let frame_alpha = lerp(fa.1, fb.1) * alpha;
    let pulse_frame = (t * 30.0) as usize % NOTICE_PULSE_FRAMES;
    let pulse = pulse_frame
        .checked_sub(NOTICE_PULSE_FIRST)
        .and_then(|j| NOTICE_PULSE.get(j))
        .copied()
        .unwrap_or((1.0, 0.0));
    let cx = Cx::new(
        [pulse.0, pulse.0, pulse.0, frame_alpha],
        [0.0, pulse.1, pulse.1, 0.0],
    );
    // frameBottom_mc from -2 up to 2 - height / 2, frameTop_mc from 2 down to
    // 2 + height / 2 (the art is upside down in them).
    let half = NOTICE_MESSAGE_H * 0.5;
    let bottom = msg.then(&M3::flat(
        frame_sx,
        0.0,
        0.0,
        1.0,
        0.0,
        -2.0 + (4.0 - half) * grow,
    ));
    let top = msg.then(&M3::flat(frame_sx, 0.0, 0.0, 1.0, 0.0, 2.0 + half * grow));
    let strokes = [
        (bottom, M3::flat(-1.0, 0.0, 0.0, 1.0, -59.05, -35.0)),
        (bottom, M3::flat(1.031, 0.0, 0.0, 1.0, 55.95, -35.0)),
        (top, M3::flat(-1.0, 0.0, 0.0, -1.0, -59.05, 37.0)),
        (top, M3::flat(1.0, 0.0, 0.0, -1.0, 57.95, 37.0)),
    ];
    // Shape 5 shows its art mirrored.
    let mirror = M3::flat(-1.0, 0.0, 0.0, 1.0, 128.0, 0.0);
    for (j, (clip, child)) in strokes.iter().enumerate() {
        p.image(
            key("ch_notice", 1 + j),
            "notify_corner",
            [0.0, 0.0, 128.0, 64.0],
            &clip.then(child).then(&mirror),
            cx,
        );
    }
    // content_mc, from frame 4: the title and, under it, the message.
    if t * 30.0 >= 4.0 {
        let content = msg.then(&M3::translate(-133.0, -28.0, 0.0));
        p.text(
            key("ch_notice", 5),
            spec(
                n.title.clone(),
                FlashFont::Title,
                20.0,
                [204, 255, 255],
                266.0,
                Some(Glow {
                    color: [0, 204, 153],
                    strength: 1.0,
                    blur: 5.0,
                }),
            ),
            2.0,
            2.0,
            &content,
            Cx::alpha(alpha),
        );
        let message = n.message.clone().unwrap_or_else(|| {
            data.string(
                "MessagePrompts.CharacterSelectedInGame",
                "Selected character used on respawn.",
            )
        });
        p.text(
            key("ch_notice", 6),
            spec(
                message,
                FlashFont::Normal,
                12.0,
                [255, 255, 204],
                600.0,
                None,
            ),
            2.0,
            2.0 + 24.0 + 4.0,
            &content,
            Cx::alpha(alpha),
        );
    }
}
